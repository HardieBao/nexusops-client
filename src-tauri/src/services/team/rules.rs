//! Native instruction activation. Markdown rules never become Codex exec-policy rules.
use super::content::sha256_hex;
use crate::app_config::AppType;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const LIMIT: usize = 1 << 20;
const PREFIX: &str = "<!-- nexusops-rule:";

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    connection: String,
    asset: i64,
    app: String,
    root: String,
    target: String,
    pub body: Option<String>,
    document_existed: bool,
}

pub fn supported(app: &AppType) -> bool {
    matches!(app, AppType::Codex | AppType::Claude)
}

fn root(app: &AppType) -> Result<PathBuf, String> {
    let root = match app {
        AppType::Codex => crate::codex_config::get_codex_config_dir(),
        AppType::Claude => crate::config::get_claude_config_dir(),
        _ => return Err("This tool does not support managed Markdown rules".into()),
    };
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("The rule configuration directory must be an absolute path".into());
    }
    Ok(root)
}

fn root_identity(root: &Path) -> Result<String, String> {
    let mut existing = root.to_path_buf();
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(
            existing
                .file_name()
                .ok_or("Invalid rule directory")?
                .to_owned(),
        );
        if !existing.pop() {
            return Err("Invalid rule directory".into());
        }
    }
    let mut resolved =
        dunce::canonicalize(existing).map_err(|_| "Cannot resolve rule directory")?;
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    resolved
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Rule directory is not valid UTF-8".into())
}

fn safe_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    for part in std::iter::once(None).chain(Path::new(relative).components().map(Some)) {
        if let Some(Component::Normal(name)) = part {
            current.push(name);
        } else if part.is_some() {
            return Err("Unsafe rule path".into());
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                #[cfg(windows)]
                let reparse = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let reparse = false;
                if metadata.file_type().is_symlink() || reparse {
                    return Err("Rule paths cannot cross links or reparse points".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot inspect rule path".into()),
        }
    }
    Ok(current)
}

fn read_file(root: &Path, relative: &str) -> Result<Option<String>, String> {
    let path = safe_path(root, relative)?;
    match fs::metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Ok(metadata) if metadata.is_file() && metadata.len() <= LIMIT as u64 => {
            payload(&path).map(Some)
        }
        _ => Err("Rule instructions are unavailable or exceed the size limit".into()),
    }
}

pub fn payload(path: &Path) -> Result<String, String> {
    let mut text = String::new();
    fs::File::open(path)
        .map_err(|_| "Cannot read rule instructions")?
        .take((LIMIT + 1) as u64)
        .read_to_string(&mut text)
        .map_err(|_| "Rule instructions must be UTF-8")?;
    if text.len() > LIMIT {
        return Err("Rule instructions exceed the size limit".into());
    }
    Ok(text)
}

fn markers(connection: &str, asset: i64) -> Result<(String, String), String> {
    if asset <= 0
        || connection.len() < 12
        || connection.len() > 64
        || !connection.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Invalid managed rule identity".into());
    }
    Ok((
        format!("{PREFIX}{connection}:{asset}:start -->"),
        format!("{PREFIX}{connection}:{asset}:end -->"),
    ))
}

fn span(
    text: &str,
    connection: &str,
    asset: i64,
) -> Result<Option<(std::ops::Range<usize>, String)>, String> {
    let (start, end) = markers(connection, asset)?;
    let starts = text.match_indices(&start).collect::<Vec<_>>();
    let ends = text.match_indices(&end).collect::<Vec<_>>();
    if starts.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if starts.len() != 1 || ends.len() != 1 {
        return Err("The managed rule markers are damaged".into());
    }
    let start_index = starts[0].0;
    let mut body_start = start_index + start.len();
    let end_index = ends[0].0;
    if body_start >= end_index {
        return Err("The managed rule markers are out of order".into());
    }
    if text[body_start..].starts_with("\r\n") {
        body_start += 2;
    } else if text[body_start..].starts_with('\n') {
        body_start += 1;
    } else {
        return Err("The managed rule block is damaged".into());
    }
    let mut body_end = end_index;
    if text[..body_end].ends_with("\r\n") {
        body_end -= 2;
    } else if text[..body_end].ends_with('\n') {
        body_end -= 1;
    } else {
        return Err("The managed rule block is damaged".into());
    }
    if body_end < body_start || text[body_start..body_end].contains(PREFIX) {
        return Err("Nested managed rule markers are not supported".into());
    }
    let mut from = start_index;
    if text[..from].ends_with("\r\n") {
        from -= 2;
    } else if text[..from].ends_with('\n') {
        from -= 1;
    }
    let mut to = end_index + end.len();
    if text[to..].starts_with("\r\n") {
        to += 2;
    } else if text[to..].starts_with('\n') {
        to += 1;
    }
    Ok(Some((from..to, text[body_start..body_end].to_owned())))
}

pub fn read(app: &AppType, connection: &str, asset: i64) -> Result<Snapshot, String> {
    markers(connection, asset)?;
    let root = root(app)?;
    let target = if *app == AppType::Codex {
        let override_text = read_file(&root, "AGENTS.override.md")?;
        if override_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
        {
            "AGENTS.override.md".into()
        } else {
            "AGENTS.md".into()
        }
    } else {
        format!("rules/nexusops-{}-asset-{asset}.md", &connection[..12])
    };
    let text = read_file(&root, &target)?;
    let body = if *app == AppType::Codex {
        span(text.as_deref().unwrap_or(""), connection, asset)?.map(|(_, body)| body)
    } else {
        text.clone()
    };
    Ok(Snapshot {
        connection: connection.into(),
        asset,
        app: app.as_str().into(),
        root: root_identity(&root)?,
        target,
        body,
        document_existed: text.is_some(),
    })
}

pub fn fingerprint(snapshot: &Snapshot) -> String {
    // An absent rule stays absent when an emptied override reveals AGENTS.md.
    let target = snapshot.body.as_ref().map(|_| snapshot.target.as_str());
    sha256_hex(
        serde_json::json!([
            snapshot.connection,
            snapshot.asset,
            snapshot.app,
            snapshot.root,
            target,
            snapshot.body
        ])
        .to_string()
        .as_bytes(),
    )
}

pub fn activation_path(app: &AppType, connection: &str, asset: i64) -> Result<String, String> {
    let snapshot = read(app, connection, asset)?;
    Ok(Path::new(&snapshot.root)
        .join(snapshot.target)
        .to_string_lossy()
        .into_owned())
}

fn render(
    app: &AppType,
    connection: &str,
    asset: i64,
    document: &str,
    body: Option<&str>,
) -> Result<String, String> {
    if body.is_some_and(|body| body.len() > LIMIT || body.contains(PREFIX)) {
        return Err("Rule content exceeds its limit or contains reserved markers".into());
    }
    if *app == AppType::Claude {
        return Ok(body.unwrap_or("").into());
    }
    let (start, end) = markers(connection, asset)?;
    let block = body
        .map(|body| format!("\n{start}\n{body}\n{end}\n"))
        .unwrap_or_default();
    let output = if let Some((range, _)) = span(document, connection, asset)? {
        format!(
            "{}{}{}",
            &document[..range.start],
            block,
            &document[range.end..]
        )
    } else {
        format!("{document}{block}")
    };
    if body.is_none() {
        return Ok(output);
    }
    let root = root(app)?;
    let config = read_file(&root, "config.toml")?.unwrap_or_default();
    let config = config
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Cannot determine Codex instruction budget from config.toml")?;
    let maximum = match config.get("project_doc_max_bytes") {
        Some(value) => usize::try_from(
            value
                .as_integer()
                .ok_or("Invalid Codex instruction budget")?,
        )
        .map_err(|_| "Invalid Codex instruction budget")?,
        None => 32768,
    }
    .min(LIMIT);
    if output.len() > maximum {
        return Err("The combined Codex instructions exceed project_doc_max_bytes; review the instruction budget before syncing".into());
    }
    Ok(output)
}

pub fn desired(
    app: &AppType,
    connection: &str,
    asset: i64,
    body: Option<String>,
) -> Result<Snapshot, String> {
    let mut snapshot = read(app, connection, asset)?;
    let document = read_file(&root(app)?, &snapshot.target)?.unwrap_or_default();
    render(app, connection, asset, &document, body.as_deref())?;
    snapshot.body = body;
    Ok(snapshot)
}

pub fn apply(
    app: &AppType,
    connection: &str,
    asset: i64,
    target: &Snapshot,
    expected: Option<&str>,
) -> Result<(), String> {
    let current = read(app, connection, asset)?;
    if target.connection != connection
        || target.asset != asset
        || target.app != app.as_str()
        || current.root != target.root
        || current.target != target.target
        || expected != Some(fingerprint(&current).as_str())
    {
        return Err("The rule instructions changed after preview; review them again".into());
    }
    let root = root(app)?;
    let before = read_file(&root, &current.target)?;
    let rendered = render(
        app,
        connection,
        asset,
        before.as_deref().unwrap_or(""),
        target.body.as_deref(),
    )?;
    let path = safe_path(&root, &current.target)?;
    fs::create_dir_all(path.parent().ok_or("Invalid rule target")?)
        .map_err(|_| "Cannot create rule directory")?;
    safe_path(&root, &current.target)?;
    if read_file(&root, &current.target)? != before {
        return Err("Rule instructions changed during synchronization".into());
    }
    if rendered.is_empty() && !target.document_existed {
        if path.exists() {
            fs::remove_file(&path).map_err(|_| "Cannot remove the managed rule")?;
        }
    } else {
        crate::config::atomic_write(&path, rendered.as_bytes())
            .map_err(|_| "Cannot write rule instructions")?;
    }
    if fingerprint(&read(app, connection, asset)?) != fingerprint(target) {
        return Err("The tool did not reach the expected rule state".into());
    }
    Ok(())
}
