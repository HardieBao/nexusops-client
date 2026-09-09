//! Hook installation follows SkillOps adapters/{codex,claude}/install.mjs.
//! Preserve unrelated handlers and keep recoverable private backups.
use super::TeamError;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MARKER: &str = "--nexusops-skillops-hook";
const EVENTS: &[&str] = &["SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"];

fn quote(value: &Path, windows: bool) -> Result<String, TeamError> {
    let text = value.to_str().ok_or(TeamError::Storage)?;
    if text.contains(['\n', '\r', '"', '%', '!']) {
        return Err(TeamError::Storage);
    }
    Ok(if windows {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    })
}

pub fn merge(
    mut config: Value,
    runtime: &str,
    exe: &Path,
    directory: &Path,
    enabled: bool,
) -> Result<Value, TeamError> {
    let object = config.as_object_mut().ok_or(TeamError::InvalidResponse)?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or(TeamError::InvalidResponse)?;
    for groups in hooks.values_mut() {
        let groups = groups.as_array_mut().ok_or(TeamError::InvalidResponse)?;
        for group in groups.iter_mut() {
            let entries = group
                .get_mut("hooks")
                .and_then(Value::as_array_mut)
                .ok_or(TeamError::InvalidResponse)?;
            entries.retain(|hook| {
                !["command", "commandWindows"].iter().any(|key| {
                    hook.get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|command| command.contains(MARKER))
                })
            });
        }
        groups.retain(|g| g["hooks"].as_array().is_some_and(|items| !items.is_empty()));
    }
    hooks.retain(|_, groups| groups.as_array().is_some_and(|items| !items.is_empty()));
    if enabled {
        for event in EVENTS {
            // Codex selects commandWindows itself; Claude uses the platform command.
            let command = format!(
                "{} {MARKER} {runtime} {}",
                quote(exe, cfg!(windows) && runtime == "claude-code")?,
                quote(directory, cfg!(windows) && runtime == "claude-code")?
            );
            let mut hook = json!({"type":"command", "command":command, "timeout":5});
            if runtime == "codex" {
                hook["commandWindows"] = json!(format!(
                    "{} {MARKER} {runtime} {}",
                    quote(exe, true)?,
                    quote(directory, true)?
                ));
            }
            let groups = hooks
                .entry(*event)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or(TeamError::InvalidResponse)?;
            groups.push(if *event == "PostToolUse" {
                json!({"matcher":"*","hooks":[hook]})
            } else {
                json!({"hooks":[hook]})
            });
        }
    }
    Ok(config)
}

fn targets() -> [(PathBuf, &'static str); 2] {
    let codex = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(crate::codex_config::get_codex_config_dir);
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::config::get_claude_config_dir);
    [
        (codex.join("hooks.json"), "codex"),
        (claude.join("settings.json"), "claude-code"),
    ]
}

pub fn install(directory: &Path, enabled: bool) -> Result<(), TeamError> {
    let exe = std::env::current_exe().map_err(|_| TeamError::Storage)?;
    let mut changes = Vec::new();
    for (file, runtime) in targets() {
        let previous = match fs::read(&file) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(TeamError::Storage),
        };
        if previous.is_none() && !enabled {
            continue;
        }
        let current: Value = match &previous {
            Some(bytes) => serde_json::from_slice(bytes).map_err(|_| TeamError::InvalidResponse)?,
            None => json!({}),
        };
        let next = merge(current.clone(), runtime, &exe, directory, enabled)?;
        if current != next {
            let bytes = serde_json::to_vec_pretty(&next).map_err(|_| TeamError::Storage)?;
            changes.push((file, previous, bytes));
        }
    }
    for (file, previous, next) in changes {
        if let Some(bytes) = previous {
            let backup =
                file.with_extension(format!("json.nexusops-backup-{}", uuid::Uuid::new_v4()));
            crate::config::atomic_write_private(&backup, &bytes).map_err(|_| TeamError::Storage)?;
        }
        crate::config::atomic_write_private(&file, &next).map_err(|_| TeamError::Storage)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_is_idempotent_and_preserves_other_hooks_and_settings() {
        let original = json!({"env":{"API_KEY":"synthetic"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-hook"}]}]}});
        let exe = Path::new("C:/Apps/Nexus Ops/client.exe");
        let dir = Path::new("C:/Data/Nexus Ops/team");
        for runtime in ["codex", "claude-code"] {
            let installed = merge(original.clone(), runtime, exe, dir, true).unwrap();
            assert_eq!(
                installed,
                merge(installed.clone(), runtime, exe, dir, true).unwrap()
            );
            assert_eq!(
                original,
                merge(installed, runtime, exe, dir, false).unwrap()
            );
        }
        assert!(merge(
            json!({"hooks":{"Stop":"malformed"}}),
            "codex",
            exe,
            dir,
            true
        )
        .is_err());
    }
}
