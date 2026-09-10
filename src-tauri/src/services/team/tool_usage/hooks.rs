//! Hook installation follows SkillOps adapters/{codex,claude}/install.mjs.
//! Preserve unrelated handlers and keep recoverable private backups.
use super::TeamError;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MARKER: &str = "--nexusops-skillops-hook";
const EVENTS: &[&str] = &["SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Registration {
    Installed,
    NotInstalled,
    NeedsRepair,
    Unreadable,
    DisabledByTool,
}

fn registration(config: &Value, runtime: &str, exe: &Path, directory: &Path) -> Registration {
    if !config.is_object() {
        return Registration::Unreadable;
    }
    if config.get("disableAllHooks").and_then(Value::as_bool) == Some(true) {
        return Registration::DisabledByTool;
    }
    let Some(hooks) = config.get("hooks") else {
        return Registration::NotInstalled;
    };
    let Some(hooks) = hooks.as_object() else {
        return Registration::Unreadable;
    };
    let Ok(expected) = merge(json!({}), runtime, exe, directory, true) else {
        return Registration::Unreadable;
    };
    let mut owned = Vec::new();
    for (event, groups) in hooks {
        let Some(groups) = groups.as_array() else {
            return Registration::Unreadable;
        };
        for group in groups {
            let Some(entries) = group.get("hooks").and_then(Value::as_array) else {
                return Registration::Unreadable;
            };
            for entry in entries {
                if ["command", "commandWindows"].iter().any(|key| {
                    entry
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.contains(MARKER))
                }) {
                    owned.push(event.as_str());
                    if !EVENTS.contains(&event.as_str())
                        || entry != &expected["hooks"][event][0]["hooks"][0]
                        || group.get("matcher") != expected["hooks"][event][0].get("matcher")
                    {
                        return Registration::NeedsRepair;
                    }
                }
            }
        }
    }
    if owned.is_empty() {
        return Registration::NotInstalled;
    }
    if EVENTS
        .iter()
        .all(|event| owned.iter().filter(|found| *found == event).count() == 1)
        && owned.len() == EVENTS.len()
    {
        Registration::Installed
    } else {
        Registration::NeedsRepair
    }
}

fn inspect_file(file: &Path, runtime: &str, exe: &Path, directory: &Path) -> Registration {
    use std::io::Read;
    let handle = match fs::File::open(file) {
        Ok(handle) => handle,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Registration::NotInstalled
        }
        Err(_) => return Registration::Unreadable,
    };
    let mut bytes = Vec::new();
    if handle.take((1 << 20) + 1).read_to_end(&mut bytes).is_err() || bytes.len() > 1 << 20 {
        return Registration::Unreadable;
    }
    match serde_json::from_slice(&bytes) {
        Ok(config) => registration(&config, runtime, exe, directory),
        Err(_) => Registration::Unreadable,
    }
}

pub fn inspect(directory: &Path) -> Result<Vec<(&'static str, Registration)>, TeamError> {
    let exe = std::env::current_exe().map_err(|_| TeamError::Storage)?;
    Ok(targets()
        .into_iter()
        .map(|(file, runtime)| (runtime, inspect_file(&file, runtime, &exe, directory)))
        .collect())
}

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

fn quote_powershell(value: &Path) -> Result<String, TeamError> {
    let text = value.to_str().ok_or(TeamError::Storage)?;
    if text.contains(['\0', '\n', '\r']) {
        return Err(TeamError::Storage);
    }
    Ok(format!("'{}'", text.replace('\'', "''")))
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
                    "& {} {MARKER} {runtime} {}",
                    quote_powershell(exe)?,
                    quote_powershell(directory)?
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
    fn registration_checks_all_owned_commands_without_writing_config() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("settings.json");
        let exe = Path::new("C:/App/client.exe");
        for runtime in ["codex", "claude-code"] {
            let installed = merge(
                json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"personal"}]}]}}),
                runtime,
                exe,
                directory.path(),
                true,
            )
            .unwrap();
            assert_eq!(
                registration(&installed, runtime, exe, directory.path()),
                Registration::Installed
            );
            assert_eq!(
                registration(
                    &installed,
                    runtime,
                    Path::new("C:/Moved/client.exe"),
                    directory.path()
                ),
                Registration::NeedsRepair
            );
            let mut duplicate = installed.clone();
            let group = duplicate["hooks"]["SessionStart"][0].clone();
            duplicate["hooks"]["SessionStart"]
                .as_array_mut()
                .unwrap()
                .push(group);
            assert_eq!(
                registration(&duplicate, runtime, exe, directory.path()),
                Registration::NeedsRepair
            );
            let mut missing = installed.clone();
            missing["hooks"].as_object_mut().unwrap().remove("Stop");
            assert_eq!(
                registration(&missing, runtime, exe, directory.path()),
                Registration::NeedsRepair
            );
            let mut blocked = installed.clone();
            blocked["disableAllHooks"] = json!(true);
            assert_eq!(
                registration(&blocked, runtime, exe, directory.path()),
                Registration::DisabledByTool
            );
            let bytes = serde_json::to_vec(&installed).unwrap();
            fs::write(&file, &bytes).unwrap();
            assert_eq!(
                inspect_file(&file, runtime, exe, directory.path()),
                Registration::Installed
            );
            assert_eq!(fs::read(&file).unwrap(), bytes);
        }
        assert_eq!(
            registration(&json!({}), "codex", exe, directory.path()),
            Registration::NotInstalled
        );
        fs::write(&file, b"not-json").unwrap();
        assert_eq!(
            inspect_file(&file, "codex", exe, directory.path()),
            Registration::Unreadable
        );
        fs::write(&file, vec![b' '; (1 << 20) + 1]).unwrap();
        assert_eq!(
            inspect_file(&file, "codex", exe, directory.path()),
            Registration::Unreadable
        );
    }
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
