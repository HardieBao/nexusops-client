use std::{
    collections::HashMap,
    io::{Cursor, Read},
};

use flate2::bufread::GzDecoder;
use sha2::{Digest, Sha256};
use unicode_casefold::{Locale, UnicodeCaseFold, Variant};
use unicode_normalization::UnicodeNormalization;

use super::types::{AssetKind, AssetLimits, FileEntry, ManifestItem};

const UTF8_BOM: &[u8] = &[0xef, 0xbb, 0xbf];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentLimits {
    pub max_text_bytes: u64,
    pub max_archive_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub max_files: usize,
    pub max_depth: usize,
    pub max_path_bytes: usize,
}

impl Default for ContentLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: 1 << 20,
            max_archive_bytes: 20 << 20,
            max_unpacked_bytes: 100 << 20,
            max_files: 2_000,
            max_depth: 16,
            max_path_bytes: 240,
        }
    }
}

impl TryFrom<&AssetLimits> for ContentLimits {
    type Error = ContentError;

    fn try_from(value: &AssetLimits) -> Result<Self, Self::Error> {
        let limits = Self {
            max_text_bytes: value.max_text_bytes,
            max_archive_bytes: value.max_archive_bytes,
            max_unpacked_bytes: value.max_unpacked_bytes,
            max_files: usize::try_from(value.max_files).map_err(|_| ContentError::InvalidLimits)?,
            max_depth: usize::try_from(value.max_path_depth)
                .map_err(|_| ContentError::InvalidLimits)?,
            max_path_bytes: usize::try_from(value.max_path_bytes)
                .map_err(|_| ContentError::InvalidLimits)?,
        };
        limits.checked()
    }
}

impl ContentLimits {
    pub(super) fn checked(self) -> Result<Self, ContentError> {
        let hard = Self::default();
        if self.max_text_bytes == 0
            || self.max_text_bytes > hard.max_text_bytes
            || self.max_archive_bytes == 0
            || self.max_archive_bytes > hard.max_archive_bytes
            || self.max_unpacked_bytes == 0
            || self.max_unpacked_bytes > hard.max_unpacked_bytes
            || self.max_files == 0
            || self.max_files > hard.max_files
            || self.max_depth == 0
            || self.max_depth > hard.max_depth
            || self.max_path_bytes == 0
            || self.max_path_bytes > hard.max_path_bytes
        {
            return Err(ContentError::InvalidLimits);
        }
        Ok(self)
    }
}

pub fn validate_manifest_item(
    item: &ManifestItem,
    limits: ContentLimits,
) -> Result<(), ContentError> {
    let limits = limits.checked()?;
    if !valid_sha256(&item.content_hash) {
        return Err(ContentError::Invalid("invalid content hash"));
    }
    if item.kind != AssetKind::Skill {
        if item.content_type != "text/plain; charset=utf-8"
            || item.archive_sha256.is_some()
            || !item.files.is_empty()
            || item.byte_size > limits.max_text_bytes
        {
            return Err(ContentError::Invalid("invalid text asset manifest"));
        }
        return Ok(());
    }
    if item.content_type != "application/gzip"
        || item.byte_size == 0
        || item.byte_size > limits.max_archive_bytes
        || item.files.len() > limits.max_files
        || item
            .archive_sha256
            .as_deref()
            .is_none_or(|hash| !valid_sha256(hash))
    {
        return Err(ContentError::Invalid("invalid Skill asset manifest"));
    }
    if item.files.is_empty() {
        // Gateway withholds private Skill paths from member manifests. The
        // downloaded archive is still unpacked safely and its canonical tree
        // must match content_hash before installation.
        return Ok(());
    }
    let mut nodes = HashMap::new();
    let mut total = 0u64;
    let mut previous: Option<&str> = None;
    for file in &item.files {
        let portable = portable_path(&file.path, false, limits)?;
        if portable != file.path
            || previous.is_some_and(|prior| prior.as_bytes() >= file.path.as_bytes())
            || !valid_sha256(&file.sha256)
        {
            return Err(ContentError::Invalid("non-canonical Skill file manifest"));
        }
        reserve_path(&mut nodes, &portable, false)?;
        total = total
            .checked_add(file.size)
            .ok_or(ContentError::TooLarge("unpacked file bytes"))?;
        if total > limits.max_unpacked_bytes
            || (is_text_path(&portable) && file.size > limits.max_text_bytes)
        {
            return Err(ContentError::TooLarge("unpacked file bytes"));
        }
        previous = Some(&file.path);
    }
    if !item.files.iter().any(|file| file.path == "SKILL.md")
        || sha256_hex(&canonical_manifest(&item.files)?) != item.content_hash
    {
        return Err(ContentError::FileManifestMismatch);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("invalid Team content limits")]
    InvalidLimits,
    #[error("Team content exceeds the supported limit: {0}")]
    TooLarge(&'static str),
    #[error("invalid Team content: {0}")]
    Invalid(&'static str),
    #[error("downloaded Team content does not match its manifest")]
    HashMismatch,
    #[error("downloaded Team file list does not match its manifest")]
    FileManifestMismatch,
    #[error("could not read Team archive")]
    Archive(#[source] std::io::Error),
    #[error("could not encode Team content manifest")]
    Manifest(#[source] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFile {
    pub entry: FileEntry,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedContent {
    pub content_hash: String,
    pub archive_sha256: Option<String>,
    pub canonical_manifest: Vec<u8>,
    pub files: Vec<VerifiedFile>,
    pub body: Vec<u8>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn normalize_text(bytes: &[u8], limits: ContentLimits) -> Result<Vec<u8>, ContentError> {
    let limits = limits.checked()?;
    if bytes.len() as u64 > limits.max_text_bytes {
        return Err(ContentError::TooLarge("text bytes"));
    }
    std::str::from_utf8(bytes).map_err(|_| ContentError::Invalid("text must be UTF-8"))?;
    let without_bom = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let mut normalized = Vec::with_capacity(without_bom.len());
    let mut remaining = without_bom;
    while let Some(index) = remaining.windows(2).position(|pair| pair == b"\r\n") {
        normalized.extend_from_slice(&remaining[..index]);
        normalized.push(b'\n');
        remaining = &remaining[index + 2..];
    }
    normalized.extend_from_slice(remaining);
    Ok(normalized)
}

pub fn canonical_manifest(files: &[FileEntry]) -> Result<Vec<u8>, ContentError> {
    serde_json::to_vec(files).map_err(ContentError::Manifest)
}

pub fn verify_download(
    item: &ManifestItem,
    downloaded: &[u8],
    limits: ContentLimits,
) -> Result<VerifiedContent, ContentError> {
    let limits = limits.checked()?;
    validate_manifest_item(item, limits)?;
    if downloaded.len() as u64 != item.byte_size {
        return Err(ContentError::HashMismatch);
    }
    if item.kind == AssetKind::Skill {
        if item.content_type != "application/gzip" {
            return Err(ContentError::Invalid("invalid Skill content type"));
        }
        let expected_archive = item
            .archive_sha256
            .as_deref()
            .ok_or(ContentError::Invalid("Skill archive hash is missing"))?;
        if !valid_sha256(expected_archive) || sha256_hex(downloaded) != expected_archive {
            return Err(ContentError::HashMismatch);
        }
        let mut verified = verify_skill(downloaded, limits)?;
        if verified.content_hash != item.content_hash {
            return Err(ContentError::HashMismatch);
        }
        if !item.files.is_empty()
            && item.files
                != verified
                    .files
                    .iter()
                    .map(|file| file.entry.clone())
                    .collect::<Vec<_>>()
        {
            return Err(ContentError::FileManifestMismatch);
        }
        verified.archive_sha256 = Some(expected_archive.to_owned());
        return Ok(verified);
    }
    if item.content_type != "text/plain; charset=utf-8"
        || item.archive_sha256.is_some()
        || !item.files.is_empty()
        || !valid_sha256(&item.content_hash)
    {
        return Err(ContentError::Invalid("invalid text asset manifest"));
    }
    let body = normalize_text(downloaded, limits)?;
    let content_hash = sha256_hex(&body);
    if content_hash != item.content_hash {
        return Err(ContentError::HashMismatch);
    }
    Ok(VerifiedContent {
        content_hash,
        archive_sha256: None,
        canonical_manifest: Vec::new(),
        files: Vec::new(),
        body,
    })
}

fn verify_skill(bytes: &[u8], limits: ContentLimits) -> Result<VerifiedContent, ContentError> {
    if bytes.len() as u64 > limits.max_archive_bytes {
        return Err(ContentError::TooLarge("compressed archive bytes"));
    }
    let max_entries = limits
        .max_files
        .checked_mul(limits.max_depth + 1)
        .ok_or(ContentError::InvalidLimits)?;
    let decoded_limit = limits
        .max_unpacked_bytes
        .checked_add((max_entries as u64).saturating_mul(1_024))
        .and_then(|value| value.checked_add(1_024))
        .ok_or(ContentError::InvalidLimits)?;
    let mut decoder = GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    let mut chunk = [0u8; 32 * 1_024];
    loop {
        let count = decoder.read(&mut chunk).map_err(ContentError::Archive)?;
        if count == 0 {
            break;
        }
        if decoded.len() as u64 + count as u64 > decoded_limit {
            return Err(ContentError::TooLarge("tar stream bytes"));
        }
        decoded.extend_from_slice(&chunk[..count]);
    }
    if !decoder.into_inner().is_empty() {
        return Err(ContentError::Invalid("trailing compressed data"));
    }
    validate_raw_entries(&decoded, max_entries)?;

    let mut archive = tar::Archive::new(Cursor::new(decoded.as_slice()));
    let mut nodes = HashMap::new();
    let mut files = Vec::new();
    let mut unpacked = 0u64;
    let entries = archive.entries().map_err(ContentError::Archive)?;
    for entry in entries {
        let mut entry = entry.map_err(ContentError::Archive)?;
        let kind = entry.header().entry_type();
        let directory = kind.is_dir();
        if !directory && !kind.is_file() {
            return Err(ContentError::Invalid(
                "only regular files and directories are allowed",
            ));
        }
        if let Some(extensions) = entry.pax_extensions().map_err(ContentError::Archive)? {
            for extension in extensions {
                let extension = extension.map_err(ContentError::Archive)?;
                let key = extension
                    .key()
                    .map_err(|_| ContentError::Invalid("PAX key must be UTF-8"))?;
                if key.starts_with("GNU.sparse") || key.starts_with("SCHILY.") {
                    return Err(ContentError::Invalid(
                        "extended file semantics are unsupported",
                    ));
                }
            }
        }
        let path_bytes = entry.path_bytes();
        let raw_name = std::str::from_utf8(path_bytes.as_ref())
            .map_err(|_| ContentError::Invalid("archive path must be UTF-8"))?;
        let name = portable_path(raw_name, directory, limits)?;
        reserve_path(&mut nodes, &name, directory)?;
        if directory {
            if entry.size() != 0 {
                return Err(ContentError::Invalid("directory has a body"));
            }
            continue;
        }
        if files.len() >= limits.max_files {
            return Err(ContentError::TooLarge("file count"));
        }
        let size = entry.size();
        if size > limits.max_unpacked_bytes.saturating_sub(unpacked) {
            return Err(ContentError::TooLarge("unpacked file bytes"));
        }
        if is_text_path(&name) && size > limits.max_text_bytes {
            return Err(ContentError::TooLarge("text file bytes"));
        }
        let mut body = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        entry
            .read_to_end(&mut body)
            .map_err(ContentError::Archive)?;
        if body.len() as u64 != size {
            return Err(ContentError::Invalid("truncated file"));
        }
        unpacked += size;
        if is_text_path(&name) {
            body = normalize_text(&body, limits)?;
        }
        let mode = entry.header().mode().map_err(ContentError::Archive)?;
        files.push(VerifiedFile {
            entry: FileEntry {
                path: name,
                sha256: sha256_hex(&body),
                size: body.len() as u64,
                executable: mode & 0o111 != 0,
            },
            body,
        });
    }
    let position = archive.into_inner().position() as usize;
    validate_tar_footer(&decoded, position)?;
    let skill = nodes.get(&simple_fold_key("SKILL.md"));
    if !matches!(skill, Some(node) if !node.directory && node.name == "SKILL.md") {
        return Err(ContentError::Invalid("root SKILL.md is required"));
    }
    files.sort_by(|left, right| left.entry.path.as_bytes().cmp(right.entry.path.as_bytes()));
    let entries = files
        .iter()
        .map(|file| file.entry.clone())
        .collect::<Vec<_>>();
    let manifest = canonical_manifest(&entries)?;
    Ok(VerifiedContent {
        content_hash: sha256_hex(&manifest),
        archive_sha256: None,
        canonical_manifest: manifest,
        files,
        body: Vec::new(),
    })
}

fn validate_raw_entries(decoded: &[u8], max_entries: usize) -> Result<(), ContentError> {
    let mut archive = tar::Archive::new(Cursor::new(decoded));
    let entries = archive.entries().map_err(ContentError::Archive)?.raw(true);
    for (index, entry) in entries.enumerate() {
        if index >= max_entries {
            return Err(ContentError::TooLarge("tar entry count"));
        }
        let entry = entry.map_err(ContentError::Archive)?;
        let kind = entry.header().entry_type();
        if kind.is_gnu_sparse() || kind.is_pax_global_extensions() {
            return Err(ContentError::Invalid("unsupported archive extension"));
        }
    }
    let position = archive.into_inner().position() as usize;
    validate_tar_footer(decoded, position)
}

fn validate_tar_footer(decoded: &[u8], position: usize) -> Result<(), ContentError> {
    if position < 512 || decoded.len() < position.saturating_add(512) {
        return Err(ContentError::Invalid("missing tar end marker"));
    }
    if decoded[position - 512..].iter().any(|byte| *byte != 0) {
        return Err(ContentError::Invalid("data after tar end marker"));
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct PathNode {
    name: String,
    directory: bool,
    explicit: bool,
}

pub(super) fn portable_path(
    raw_name: &str,
    directory: bool,
    limits: ContentLimits,
) -> Result<String, ContentError> {
    let name = if directory {
        raw_name.strip_suffix('/').unwrap_or(raw_name)
    } else {
        raw_name
    };
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(['\\', '<', '>', ':', '"', '|', '?', '*'])
    {
        return Err(ContentError::Invalid("non-portable archive path"));
    }
    if !name.nfc().eq(name.chars()) {
        return Err(ContentError::Invalid("archive path must use NFC Unicode"));
    }
    if name.len() > limits.max_path_bytes {
        return Err(ContentError::TooLarge("path bytes"));
    }
    let parts = name.split('/').collect::<Vec<_>>();
    if parts.len() > limits.max_depth {
        return Err(ContentError::TooLarge("path depth"));
    }
    if name
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
    {
        return Err(ContentError::Invalid("control character in archive path"));
    }
    for part in parts {
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with('.')
            || part.ends_with(' ')
            || windows_reserved(part)
        {
            return Err(ContentError::Invalid("non-portable path component"));
        }
    }
    Ok(name.to_owned())
}

fn windows_reserved(part: &str) -> bool {
    let base = part.split_once('.').map_or(part, |(base, _)| base);
    let upper = base.to_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(suffix) = upper.strip_prefix(prefix) {
            if suffix.chars().count() == 1
                && suffix
                    .chars()
                    .all(|character| "123456789¹²³".contains(character))
            {
                return true;
            }
        }
    }
    false
}

pub(super) fn simple_fold_key(name: &str) -> String {
    name.chars().map(protocol_simple_fold).collect()
}

// unicode-casefold 0.2.0 carries Unicode 9 tables. These additions are the
// simple-fold pairs present in the Gateway's Go 1.26.6 Unicode tables. Mapping
// the first member to the second gives the same collision classes as Go's
// orbit-minimum key while retaining the crate's handling of older orbits.
fn protocol_simple_fold(character: char) -> char {
    let code = character as u32;
    let supplemented = match code {
        0x1c90..=0x1cba => char::from_u32(code - 0xbc0),
        0x1cbd..=0x1cbf => char::from_u32(code - 0xbc0),
        0x10570..=0x1057a => char::from_u32(code + 0x27),
        0x1057c..=0x1058a => char::from_u32(code + 0x27),
        0x1058c..=0x10592 => char::from_u32(code + 0x27),
        0x10594..=0x10595 => char::from_u32(code + 0x27),
        0x16e40..=0x16e5f => char::from_u32(code + 0x20),
        0xa7b8 | 0xa7ba | 0xa7bc | 0xa7be | 0xa7c0 | 0xa7c2 | 0xa7c7 | 0xa7c9 | 0xa7d0 | 0xa7d6
        | 0xa7d8 | 0xa7f5 => char::from_u32(code + 1),
        0xa7c4 => char::from_u32(0xa794),
        0xa7c5 => char::from_u32(0x0282),
        0xa7c6 => char::from_u32(0x1d8e),
        0x2c2f => char::from_u32(0x2c5f),
        _ => None,
    };
    supplemented.unwrap_or_else(|| {
        character
            .case_fold_with(Variant::Simple, Locale::NonTurkic)
            .next()
            .expect("one input scalar always has a simple fold result")
    })
}

pub(super) fn reserve_path(
    nodes: &mut HashMap<String, PathNode>,
    name: &str,
    directory: bool,
) -> Result<(), ContentError> {
    let parts = name.split('/').collect::<Vec<_>>();
    for index in 0..parts.len() {
        let prefix = parts[..=index].join("/");
        let key = simple_fold_key(&prefix);
        let final_part = index + 1 == parts.len();
        let is_directory = !final_part || directory;
        if let Some(previous) = nodes.get_mut(&key) {
            if previous.name != prefix
                || !previous.directory
                || !is_directory
                || (final_part && previous.explicit)
            {
                return Err(ContentError::Invalid(
                    "duplicate or conflicting archive path",
                ));
            }
            if final_part {
                previous.explicit = true;
            }
        } else {
            nodes.insert(
                key,
                PathNode {
                    name: prefix,
                    directory: is_directory,
                    explicit: final_part,
                },
            );
        }
    }
    Ok(())
}

pub(super) fn is_text_path(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    if matches!(base, "LICENSE" | "README") {
        return true;
    }
    let extension = base
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    matches!(
        extension.as_deref(),
        Some(
            "md" | "txt"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "html"
                | "css"
                | "js"
                | "ts"
                | "py"
                | "sh"
                | "ps1"
                | "ini"
                | "cfg"
                | "csv"
                | "svg"
        )
    )
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use base64::{engine::general_purpose::STANDARD, Engine};
    use flate2::{write::GzEncoder, Compression};
    use serde::Deserialize;
    use tar::{Builder, EntryType, Header};

    use super::*;

    #[derive(Deserialize)]
    struct ProtocolVectors {
        texts: Vec<TextVector>,
        skill: SkillVector,
    }

    #[derive(Deserialize)]
    struct TextVector {
        input_base64: String,
        normalized_base64: String,
        sha256: String,
    }

    #[derive(Deserialize)]
    struct SkillVector {
        canonical_manifest: String,
        content_hash: String,
        input_archive_sha256: String,
    }

    fn fixture_item(bytes: &[u8], hash: &str, archive_hash: &str) -> ManifestItem {
        let files = verify_skill(bytes, ContentLimits::default())
            .map(|verified| verified.files.into_iter().map(|file| file.entry).collect())
            .unwrap_or_default();
        ManifestItem {
            asset_id: 1,
            kind: AssetKind::Skill,
            slug: "fixture".into(),
            name: "Fixture".into(),
            revision: 1,
            content_hash: hash.into(),
            archive_sha256: Some(archive_hash.into()),
            download_url: "/api/v1/assets/1/revisions/1/download".into(),
            content_type: "application/gzip".into(),
            byte_size: bytes.len() as u64,
            files,
        }
    }

    fn archive(entries: &[(&str, &[u8], u32, EntryType)]) -> Vec<u8> {
        let mut plain = Vec::new();
        {
            let mut builder = Builder::new(&mut plain);
            for (name, body, mode, kind) in entries {
                let mut header = Header::new_gnu();
                header.set_entry_type(*kind);
                header.set_mode(*mode);
                header.set_size(body.len() as u64);
                header.set_cksum();
                builder.append_data(&mut header, *name, *body).unwrap();
            }
            builder.finish().unwrap();
        }
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&plain).unwrap();
        gzip.finish().unwrap()
    }

    #[test]
    fn matches_go_protocol_v1_vectors() {
        let vectors: ProtocolVectors =
            serde_json::from_str(include_str!("testdata/protocol-v1.json"))
                .expect("parse shared Go vectors");
        for vector in vectors.texts {
            let input = STANDARD.decode(vector.input_base64).unwrap();
            let normalized = normalize_text(&input, ContentLimits::default()).unwrap();
            assert_eq!(STANDARD.encode(&normalized), vector.normalized_base64);
            assert_eq!(sha256_hex(&normalized), vector.sha256);
        }
        let bytes = STANDARD
            .decode(include_str!("testdata/skill-input.tar.gz.base64").trim())
            .unwrap();
        assert_eq!(sha256_hex(&bytes), vectors.skill.input_archive_sha256);
        let item = fixture_item(
            &bytes,
            &vectors.skill.content_hash,
            &vectors.skill.input_archive_sha256,
        );
        let verified = verify_download(&item, &bytes, ContentLimits::default()).unwrap();
        assert_eq!(
            verified.canonical_manifest,
            vectors.skill.canonical_manifest.as_bytes()
        );
        assert_eq!(verified.content_hash, vectors.skill.content_hash);
        assert_eq!(
            verified.files[2].body,
            vec![0xef, 0xbb, 0xbf, 0, 0xff, b'\r', b'\n']
        );
    }

    #[test]
    fn rejects_links_traversal_case_nfc_and_reserved_paths() {
        for name in [
            "../escape",
            "/absolute",
            "C:/drive",
            "folder\\escape",
            "NUL.txt",
            "cafe\u{301}.txt",
        ] {
            assert!(matches!(
                portable_path(name, false, ContentLimits::default()),
                Err(ContentError::Invalid(_))
            ));
        }
        for kind in [
            EntryType::Symlink,
            EntryType::Link,
            EntryType::Char,
            EntryType::Block,
            EntryType::Fifo,
        ] {
            let bytes = archive(&[
                ("SKILL.md", b"# Test", 0o644, EntryType::Regular),
                ("special", b"x", 0o644, kind),
            ]);
            assert!(matches!(
                verify_skill(&bytes, ContentLimits::default()),
                Err(ContentError::Invalid(_)) | Err(ContentError::Archive(_))
            ));
        }
        let bytes = archive(&[
            ("SKILL.md", b"# Test", 0o644, EntryType::Regular),
            ("Dir/a.txt", b"a", 0o644, EntryType::Regular),
            ("dir/b.txt", b"b", 0o644, EntryType::Regular),
        ]);
        assert!(matches!(
            verify_skill(&bytes, ContentLimits::default()),
            Err(ContentError::Invalid(
                "duplicate or conflicting archive path"
            ))
        ));
    }

    #[test]
    fn rejects_tampering_and_archive_ambiguity() {
        let valid = archive(&[("SKILL.md", b"# Test", 0o644, EntryType::Regular)]);
        let valid_content = verify_skill(&valid, ContentLimits::default()).unwrap();
        let mut item = fixture_item(&valid, &valid_content.content_hash, &sha256_hex(&valid));
        let mut tampered = valid.clone();
        tampered[20] ^= 1;
        assert!(matches!(
            verify_download(&item, &tampered, ContentLimits::default()),
            Err(ContentError::HashMismatch)
        ));

        let mut concatenated = valid.clone();
        concatenated.extend_from_slice(&valid);
        item.byte_size = concatenated.len() as u64;
        item.archive_sha256 = Some(sha256_hex(&concatenated));
        assert!(matches!(
            verify_download(&item, &concatenated, ContentLimits::default()),
            Err(ContentError::Invalid("trailing compressed data"))
        ));

        let tiny_limits = ContentLimits {
            max_unpacked_bytes: 1,
            ..ContentLimits::default()
        };
        let item = fixture_item(&valid, &valid_content.content_hash, &sha256_hex(&valid));
        assert!(matches!(
            verify_download(&item, &valid, tiny_limits),
            Err(ContentError::TooLarge("unpacked file bytes"))
        ));
    }

    #[test]
    fn text_rules_keep_lone_cr_and_reject_invalid_utf8() {
        assert_eq!(
            normalize_text(b"\xef\xbb\xbfa\r\nb\r", ContentLimits::default()).unwrap(),
            b"a\nb\r"
        );
        assert!(matches!(
            normalize_text(&[0xff], ContentLimits::default()),
            Err(ContentError::Invalid("text must be UTF-8"))
        ));
    }

    #[test]
    fn path_casefold_matches_go_old_orbits_and_post_unicode_9_pairs() {
        for (left, right) in [
            ('A', 'a'),
            ('Σ', 'ς'),
            ('S', 'ſ'),
            ('\u{a7c5}', '\u{282}'),
            ('\u{1c90}', '\u{10d0}'),
            ('\u{10570}', '\u{10597}'),
            ('\u{16e40}', '\u{16e60}'),
        ] {
            assert_eq!(
                simple_fold_key(&left.to_string()),
                simple_fold_key(&right.to_string())
            );
        }
        assert_ne!(simple_fold_key("I"), simple_fold_key("ı"));
    }
}
