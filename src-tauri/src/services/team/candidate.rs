use super::{
    api::Cancellation,
    content::{self, ContentLimits, VerifiedFile},
    types::FileEntry,
    worker::{Operation, RuleTarget, Worker, WorkerError},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Skill,
    Rule,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub kind: Kind,
    pub root: PathBuf,
    pub entry: Option<String>,
    pub output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedFile {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inspection {
    kind: Kind,
    name: String,
    description: String,
    entry: String,
    files: Vec<ObservedFile>,
    content: Option<String>,
    extension: Option<String>,
}

#[derive(Serialize)]
pub struct Origin {
    format: &'static str,
    module_commit: &'static str,
    module_version: &'static str,
}

#[derive(Serialize)]
pub struct Candidate {
    schema_version: u32,
    origin: Origin,
    kind: Kind,
    name: String,
    description: String,
    content_type: &'static str,
    content_hash: String,
    archive_sha256: Option<String>,
    files: Vec<FileEntry>,
    payload_base64: String,
}

#[derive(Serialize)]
pub struct ExportSummary {
    pub output: String,
    pub kind: Kind,
    pub name: String,
    pub content_hash: String,
    pub bytes: u64,
}

fn plain(path: &Path, directory: bool) -> Result<fs::Metadata, WorkerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| WorkerError::UnsafePath)?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if metadata.file_type().is_symlink()
        || reparse
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(WorkerError::UnsafePath);
    }
    Ok(metadata)
}

fn read_observed(
    root: &Path,
    file: &ObservedFile,
    cancel: &Cancellation,
) -> Result<VerifiedFile, WorkerError> {
    if cancel.is_cancelled() {
        return Err(WorkerError::Cancelled);
    }
    content::portable_path(&file.path, false, ContentLimits::default())
        .map_err(|_| WorkerError::UnsafePath)?;
    if file.size > 100 << 20 {
        return Err(WorkerError::TooLarge);
    }
    let path = root.join(&file.path);
    let mut cursor = root.to_path_buf();
    let components = Path::new(&file.path).components().collect::<Vec<_>>();
    for (index, part) in components.iter().enumerate() {
        cursor.push(part);
        plain(&cursor, index + 1 < components.len())?;
    }
    let resolved = dunce::canonicalize(&path).map_err(|_| WorkerError::UnsafePath)?;
    if !resolved.starts_with(root) {
        return Err(WorkerError::UnsafePath);
    }
    let handle = fs::File::open(&path).map_err(|_| WorkerError::UnsafePath)?;
    let metadata = handle.metadata().map_err(|_| WorkerError::UnsafePath)?;
    if !metadata.is_file() || metadata.len() != file.size {
        return Err(WorkerError::InvalidInput);
    }
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(WorkerError::UnsafePath);
        }
        metadata.mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    let mut bytes = Vec::new();
    handle
        .take(file.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WorkerError::InvalidInput)?;
    if bytes.len() as u64 != file.size || content::sha256_hex(&bytes) != file.sha256 {
        return Err(WorkerError::InvalidInput);
    }
    Ok(VerifiedFile {
        entry: FileEntry {
            path: file.path.clone(),
            sha256: file.sha256.clone(),
            size: file.size,
            executable,
        },
        body: bytes,
    })
}

struct BoundedBytes(Vec<u8>);
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + bytes.len() > 20 << 20 {
            return Err(std::io::Error::other("archive limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn archive(files: &[VerifiedFile]) -> Result<Vec<u8>, WorkerError> {
    let gzip = flate2::GzBuilder::new()
        .mtime(0)
        .write(BoundedBytes(Vec::new()), flate2::Compression::default());
    let mut builder = tar::Builder::new(gzip);
    for file in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(file.body.len() as u64);
        header.set_mode(if file.entry.executable { 0o755 } else { 0o644 });
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_cksum();
        builder
            .append_data(&mut header, &file.entry.path, file.body.as_slice())
            .map_err(|_| WorkerError::TooLarge)?;
    }
    builder
        .into_inner()
        .map_err(|_| WorkerError::InvalidInput)?
        .finish()
        .map(|out| out.0)
        .map_err(|_| WorkerError::TooLarge)
}

fn build(
    root: &Path,
    inspection: Inspection,
    cancel: &Cancellation,
) -> Result<Candidate, WorkerError> {
    plain(root, true)?;
    if inspection.files.is_empty() || inspection.files.len() > 2000 {
        return Err(WorkerError::TooLarge);
    }
    let mut total = 0u64;
    let mut originals = Vec::new();
    for file in &inspection.files {
        total = total.checked_add(file.size).ok_or(WorkerError::TooLarge)?;
        if total > 100 << 20 {
            return Err(WorkerError::TooLarge);
        }
        originals.push(read_observed(root, file, cancel)?);
    }
    let (payload, hash, archive_hash, files, content_type) = match inspection.kind {
        Kind::Skill => {
            if inspection.entry != "SKILL.md"
                || inspection.content.is_some()
                || inspection.extension.is_some()
            {
                return Err(WorkerError::InvalidResponse);
            }
            let raw = archive(&originals)?;
            drop(originals);
            let verified = content::verify_skill(&raw, ContentLimits::default())
                .map_err(|_| WorkerError::InvalidInput)?;
            drop(raw);
            let payload = archive(&verified.files)?;
            let digest = content::sha256_hex(&payload);
            (
                payload,
                verified.content_hash,
                Some(digest),
                verified.files.into_iter().map(|file| file.entry).collect(),
                "application/gzip",
            )
        }
        Kind::Rule => {
            if originals.len() != 1
                || inspection.entry != originals[0].entry.path
                || inspection.extension.as_deref() != Some(".md")
            {
                return Err(WorkerError::InvalidResponse);
            }
            let normalized = content::normalize_text(&originals[0].body, ContentLimits::default())
                .map_err(|_| WorkerError::InvalidInput)?;
            if inspection.content.as_deref().map(str::as_bytes) != Some(normalized.as_slice()) {
                return Err(WorkerError::InvalidResponse);
            }
            let hash = content::sha256_hex(&normalized);
            (
                normalized,
                hash,
                None,
                Vec::new(),
                "text/plain; charset=utf-8",
            )
        }
    };
    Ok(Candidate {
        schema_version: 1,
        origin: Origin {
            format: "teamai",
            module_commit: "6ae0619d067b1699bb2c6e435abf3ffe11a21d71",
            module_version: "0.22.0",
        },
        kind: inspection.kind,
        name: inspection.name,
        description: inspection.description,
        content_type,
        content_hash: hash,
        archive_sha256: archive_hash,
        files,
        payload_base64: STANDARD.encode(payload),
    })
}

pub async fn export(
    worker: &Worker,
    request: ExportRequest,
    cancel: &Cancellation,
) -> Result<ExportSummary, WorkerError> {
    plain(&request.root, true)?;
    let root = dunce::canonicalize(&request.root).map_err(|_| WorkerError::UnsafePath)?;
    if !request.output.is_absolute()
        || !request
            .output
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".nexusops-asset.json"))
    {
        return Err(WorkerError::UnsafePath);
    }
    let parent = dunce::canonicalize(request.output.parent().ok_or(WorkerError::UnsafePath)?)
        .map_err(|_| WorkerError::UnsafePath)?;
    if parent.starts_with(&root) {
        return Err(WorkerError::UnsafePath);
    }
    let output = parent.join(request.output.file_name().ok_or(WorkerError::UnsafePath)?);
    let operation = match request.kind {
        Kind::Skill => Operation::InspectSkill { root: root.clone() },
        Kind::Rule => Operation::ConvertRule {
            root: root.clone(),
            entry: request.entry.ok_or(WorkerError::InvalidInput)?,
            target: RuleTarget::Codex,
        },
    };
    let value = worker.run(operation, cancel).await?;
    let inspection: Inspection =
        serde_json::from_value(value).map_err(|_| WorkerError::InvalidResponse)?;
    if inspection.kind != request.kind {
        return Err(WorkerError::InvalidResponse);
    }
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || {
        let candidate = build(&root, inspection, &cancel)?;
        if cancel.is_cancelled() {
            return Err(WorkerError::Cancelled);
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(&parent).map_err(|_| WorkerError::InvalidInput)?;
        serde_json::to_writer(temporary.as_file_mut(), &candidate)
            .map_err(|_| WorkerError::InvalidInput)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| WorkerError::InvalidInput)?;
        let bytes = temporary
            .as_file()
            .metadata()
            .map_err(|_| WorkerError::InvalidInput)?
            .len();
        if bytes > 32 << 20 {
            return Err(WorkerError::TooLarge);
        }
        if cancel.is_cancelled() {
            return Err(WorkerError::Cancelled);
        }
        temporary
            .persist_noclobber(&output)
            .map_err(|_| WorkerError::InvalidInput)?;
        Ok(ExportSummary {
            output: output.to_string_lossy().into_owned(),
            kind: candidate.kind,
            name: candidate.name,
            content_hash: candidate.content_hash,
            bytes,
        })
    })
    .await
    .map_err(|_| WorkerError::InvalidResponse)?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observed(root: &Path, name: &str, body: &[u8]) -> ObservedFile {
        fs::write(root.join(name), body).unwrap();
        ObservedFile {
            path: name.into(),
            size: body.len() as u64,
            sha256: content::sha256_hex(body),
        }
    }
    #[test]
    fn rule_export_rechecks_source_and_worker_normalization() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"\xef\xbb\xbf# Rule\r\n";
        let file = observed(root.path(), "rule.md", bytes);
        let result = build(
            root.path(),
            Inspection {
                kind: Kind::Rule,
                name: "rule".into(),
                description: String::new(),
                entry: "rule.md".into(),
                files: vec![file],
                content: Some("# Rule\n".into()),
                extension: Some(".md".into()),
            },
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(STANDARD.decode(result.payload_base64).unwrap(), b"# Rule\n");
        assert_eq!(result.content_hash, content::sha256_hex(b"# Rule\n"));
        let file = observed(root.path(), "rule.md", b"original");
        fs::write(root.path().join("rule.md"), b"modified").unwrap();
        assert!(build(
            root.path(),
            Inspection {
                kind: Kind::Rule,
                name: "rule".into(),
                description: String::new(),
                entry: "rule.md".into(),
                files: vec![file],
                content: Some("original".into()),
                extension: Some(".md".into())
            },
            &Cancellation::default()
        )
        .is_err());
    }

    #[test]
    fn skill_export_is_canonical_and_compatible_with_the_installer() {
        let root = tempfile::tempdir().unwrap();
        let file = observed(
            root.path(),
            "SKILL.md",
            b"\xef\xbb\xbf---\r\nname: fixture\r\n---\r\n# Skill\r\n",
        );
        let result = build(
            root.path(),
            Inspection {
                kind: Kind::Skill,
                name: "fixture".into(),
                description: String::new(),
                entry: "SKILL.md".into(),
                files: vec![file],
                content: None,
                extension: None,
            },
            &Cancellation::default(),
        )
        .unwrap();
        let bytes = STANDARD.decode(&result.payload_base64).unwrap();
        let item = super::super::types::ManifestItem {
            asset_id: 1,
            kind: super::super::types::AssetKind::Skill,
            slug: "fixture".into(),
            name: "Fixture".into(),
            revision: 1,
            content_hash: result.content_hash,
            archive_sha256: result.archive_sha256,
            download_url: String::new(),
            content_type: result.content_type.into(),
            byte_size: bytes.len() as u64,
            files: result.files,
        };
        let verified = content::verify_download(&item, &bytes, ContentLimits::default()).unwrap();
        assert_eq!(
            verified.files[0].body,
            b"---\nname: fixture\n---\n# Skill\n"
        );
    }

    #[test]
    fn untrusted_worker_paths_do_not_escape_the_selected_directory() {
        let root = tempfile::tempdir().unwrap();
        let file = ObservedFile {
            path: "../outside.md".into(),
            size: 0,
            sha256: content::sha256_hex(b""),
        };
        assert!(matches!(
            read_observed(root.path(), &file, &Cancellation::default()),
            Err(WorkerError::UnsafePath)
        ));
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(matches!(
            read_observed(root.path(), &file, &cancel),
            Err(WorkerError::Cancelled)
        ));
    }

    #[tokio::test]
    #[cfg(windows)]
    #[ignore = "exports real worker fixtures for the opted-in Go/PostgreSQL cross-language test"]
    async fn export_cross_language_candidates() {
        let output = PathBuf::from(
            std::env::var("NEXUSOPS_CANDIDATE_FIXTURE_DIR").expect("explicit fixture directory"),
        );
        assert!(output.is_absolute() && output.is_dir());
        let source = tempfile::tempdir().unwrap();
        let worker =
            Worker::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.teamai-build")).unwrap();
        for (kind, name, entry) in [
            (Kind::Skill, "skill", "SKILL.md"),
            (Kind::Rule, "rule", "rule.md"),
        ] {
            let root = source.path().join(name);
            fs::create_dir(&root).unwrap();
            fs::write(root.join(entry), "---\r\nname: cross-language\r\ndescription: 中文跨语言验证\r\n---\r\n# Fixture\r\nKeep trailing spaces.  \r\n").unwrap();
            if kind == Kind::Skill {
                fs::create_dir(root.join("references")).unwrap();
                fs::write(root.join("references/说明.md"), "# 中文\r\nReference\r\n").unwrap();
                fs::write(root.join("references/data.bin"), [0, 255, 1, 13, 10]).unwrap();
            }
            export(
                &worker,
                ExportRequest {
                    kind,
                    root,
                    entry: (kind == Kind::Rule).then(|| entry.into()),
                    output: output.join(format!("{name}.nexusops-asset.json")),
                },
                &Cancellation::default(),
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    #[cfg(windows)]
    #[ignore = "requires pnpm teamai:prepare; exercises the actual pinned worker and atomic artifact export"]
    async fn pinned_worker_exports_both_kinds_without_overwriting_sources_or_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let worker =
            Worker::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.teamai-build")).unwrap();
        for (kind, name, entry) in [
            (Kind::Skill, "skill", "SKILL.md"),
            (Kind::Rule, "rule", "rule.md"),
        ] {
            let root = directory.path().join(format!("中文 {name}"));
            fs::create_dir(&root).unwrap();
            let body = b"---\nname: worker-candidate\n---\n# Fixture\n";
            fs::write(root.join(entry), body).unwrap();
            let output = directory.path().join(format!("{name}.nexusops-asset.json"));
            let request = || ExportRequest {
                kind,
                root: root.clone(),
                entry: if kind == Kind::Rule {
                    Some(entry.into())
                } else {
                    None
                },
                output: output.clone(),
            };
            let summary = export(&worker, request(), &Cancellation::default())
                .await
                .unwrap();
            assert_eq!(summary.name, "worker-candidate");
            let original = fs::read(&output).unwrap();
            let candidate: serde_json::Value = serde_json::from_slice(&original).unwrap();
            assert_eq!(
                candidate["origin"]["module_commit"],
                "6ae0619d067b1699bb2c6e435abf3ffe11a21d71"
            );
            assert!(export(&worker, request(), &Cancellation::default())
                .await
                .is_err());
            assert_eq!(fs::read(&output).unwrap(), original);
            assert_eq!(fs::read(root.join(entry)).unwrap(), body);
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
            let mut inside = request();
            inside.output = root.join("inside.nexusops-asset.json");
            assert!(export(&worker, inside, &Cancellation::default())
                .await
                .is_err());
        }
    }
}
