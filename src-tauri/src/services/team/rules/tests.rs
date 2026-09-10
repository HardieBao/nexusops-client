use super::*;

const CONNECTION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

async fn converted_rule(home: &Home, app: &AppType, body: String) -> String {
    use crate::services::team::{
        api::Cancellation,
        worker::{Operation, RuleTarget, Worker},
    };
    let source = home.path("teamai-source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("rule.md"),
        format!("---\nname: fixture-rule\ndescription: CLI loading fixture\n---\n{body}"),
    )
    .unwrap();
    let worker = Worker::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.teamai-build"))
        .expect("run pnpm teamai:prepare first");
    let result = worker
        .run(
            Operation::ConvertRule {
                root: source,
                entry: "rule.md".into(),
                target: if *app == AppType::Codex {
                    RuleTarget::Codex
                } else {
                    RuleTarget::ClaudeCode
                },
            },
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(result["name"], "fixture-rule");
    result["content"].as_str().unwrap().to_owned()
}
struct Home {
    directory: tempfile::TempDir,
    previous: Option<std::ffi::OsString>,
}
impl Home {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("NEXUSOPS_CLIENT_TEST_HOME");
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", directory.path());
        crate::settings::reload_settings().unwrap();
        Self {
            directory,
            previous,
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_ref() {
            std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", previous);
        } else {
            std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
        }
        let _ = crate::settings::reload_settings();
    }
}

#[test]
#[serial_test::serial]
fn codex_updates_only_its_own_block_and_restores_personal_bytes() {
    let home = Home::new();
    fs::create_dir(home.path(".codex")).unwrap();
    let path = home.path(".codex/AGENTS.md");
    let personal = "Personal guidance\r\n保留原文";
    fs::write(&path, personal).unwrap();
    let before = read(&AppType::Codex, CONNECTION, 17).unwrap();
    let first = desired(&AppType::Codex, CONNECTION, 17, Some("Use pnpm.\n".into())).unwrap();
    apply(
        &AppType::Codex,
        CONNECTION,
        17,
        &first,
        Some(&fingerprint(&before)),
    )
    .unwrap();
    let second_before = read(&AppType::Codex, CONNECTION, 18).unwrap();
    let second = desired(&AppType::Codex, CONNECTION, 18, Some("Other rule".into())).unwrap();
    apply(
        &AppType::Codex,
        CONNECTION,
        18,
        &second,
        Some(&fingerprint(&second_before)),
    )
    .unwrap();
    let mut external = fs::read_to_string(&path).unwrap();
    external.push_str("\r\nNew personal guidance");
    fs::write(&path, &external).unwrap();
    let updated = desired(
        &AppType::Codex,
        CONNECTION,
        17,
        Some("Use pnpm and run tests.\n".into()),
    )
    .unwrap();
    apply(
        &AppType::Codex,
        CONNECTION,
        17,
        &updated,
        Some(&fingerprint(&first)),
    )
    .unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.starts_with(personal));
    assert!(content.ends_with("\r\nNew personal guidance"));
    assert!(content.contains("Other rule"));
    apply(
        &AppType::Codex,
        CONNECTION,
        17,
        &before,
        Some(&fingerprint(&updated)),
    )
    .unwrap();
    let current = read(&AppType::Codex, CONNECTION, 18).unwrap();
    apply(
        &AppType::Codex,
        CONNECTION,
        18,
        &second_before,
        Some(&fingerprint(&current)),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        format!("{personal}\r\nNew personal guidance")
    );
}

#[test]
#[serial_test::serial]
fn codex_honors_override_and_rejects_over_budget_before_writing() {
    let home = Home::new();
    fs::create_dir(home.path(".codex")).unwrap();
    fs::write(home.path(".codex/AGENTS.md"), "Base instructions").unwrap();
    fs::write(home.path(".codex/AGENTS.override.md"), b"\xef\xbb\xbf").unwrap();
    assert_eq!(
        read(&AppType::Codex, CONNECTION, 17).unwrap().target,
        "AGENTS.override.md"
    );
    fs::write(
        home.path(".codex/AGENTS.override.md"),
        "Override instructions",
    )
    .unwrap();
    let before = read(&AppType::Codex, CONNECTION, 17).unwrap();
    assert_eq!(before.target, "AGENTS.override.md");
    let target = desired(&AppType::Codex, CONNECTION, 17, Some("Rule marker".into())).unwrap();
    apply(
        &AppType::Codex,
        CONNECTION,
        17,
        &target,
        Some(&fingerprint(&before)),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(home.path(".codex/AGENTS.md")).unwrap(),
        "Base instructions"
    );
    assert!(fs::read_to_string(home.path(".codex/AGENTS.override.md"))
        .unwrap()
        .contains("Rule marker"));
    fs::write(
        home.path(".codex/config.toml"),
        "project_doc_max_bytes = 10\n",
    )
    .unwrap();
    assert!(desired(&AppType::Codex, CONNECTION, 17, Some("Another rule".into())).is_err());
    // Removal must remain possible when personal instructions exceed a new budget.
    apply(
        &AppType::Codex,
        CONNECTION,
        17,
        &before,
        Some(&fingerprint(&target)),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(home.path(".codex/AGENTS.override.md")).unwrap(),
        "Override instructions"
    );
    assert!(!home.path(".codex/rules").exists());
}

#[test]
#[serial_test::serial]
fn claude_preserves_path_conditions_and_refuses_stale_overwrites() {
    let home = Home::new();
    let before = read(&AppType::Claude, CONNECTION, 17).unwrap();
    let body = "---\npaths:\n  - src/**/*.rs\n---\nUse checked arithmetic.\n";
    let target = desired(&AppType::Claude, CONNECTION, 17, Some(body.into())).unwrap();
    apply(
        &AppType::Claude,
        CONNECTION,
        17,
        &target,
        Some(&fingerprint(&before)),
    )
    .unwrap();
    let path = home.path(".claude").join(&target.target);
    assert_eq!(fs::read_to_string(&path).unwrap(), body);
    fs::write(&path, "manual change").unwrap();
    assert!(apply(
        &AppType::Claude,
        CONNECTION,
        17,
        &target,
        Some(&fingerprint(&target))
    )
    .is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "manual change");
    let current = read(&AppType::Claude, CONNECTION, 17).unwrap();
    apply(
        &AppType::Claude,
        CONNECTION,
        17,
        &before,
        Some(&fingerprint(&current)),
    )
    .unwrap();
    assert!(!path.exists());
}

#[test]
#[serial_test::serial]
fn damaged_markers_and_foreign_snapshots_are_rejected() {
    let home = Home::new();
    fs::create_dir(home.path(".codex")).unwrap();
    let before = read(&AppType::Codex, CONNECTION, 17).unwrap();
    let target = desired(&AppType::Codex, CONNECTION, 17, Some("rule".into())).unwrap();
    assert!(apply(
        &AppType::Codex,
        CONNECTION,
        18,
        &target,
        Some(&fingerprint(&before))
    )
    .is_err());
    assert!(desired(
        &AppType::Codex,
        CONNECTION,
        17,
        Some(format!("{PREFIX}forged"))
    )
    .is_err());
    fs::write(
        home.path(".codex/AGENTS.md"),
        markers(CONNECTION, 17).unwrap().0,
    )
    .unwrap();
    assert!(read(&AppType::Codex, CONNECTION, 17).is_err());
}

#[test]
#[serial_test::serial]
fn rule_directory_aliases_cannot_redirect_activation() {
    let home = Home::new();
    fs::create_dir(home.path(".claude")).unwrap();
    fs::create_dir(home.path("outside")).unwrap();
    let link = home.path(".claude").join("rules");
    let outside = home.path("outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        assert!(std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .creation_flags(0x08000000)
            .output()
            .unwrap()
            .status
            .success());
    }
    assert!(desired(&AppType::Claude, CONNECTION, 17, Some("rule".into())).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[tokio::test]
#[serial_test::serial]
#[ignore = "requires NEXUSOPS_TEST_CODEX_BIN; launches the real CLI against a local capture endpoint"]
async fn real_codex_cli_loads_the_managed_instruction_block() {
    use axum::{body::Bytes, http::StatusCode, routing::post, Router};
    use std::{
        process::Stdio,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };
    let binary =
        std::env::var_os("NEXUSOPS_TEST_CODEX_BIN").expect("explicit Codex binary path required");
    let home = Home::new();
    const MARKER: &str = "NEXUSOPS_RULE_CLI_CAPTURE_7b4";
    let captured = Arc::new(AtomicBool::new(false));
    let capture = captured.clone();
    let (gateway, server) = crate::services::team::api::tests::serve(Router::new().route(
        "/v1/responses",
        post(move |body: Bytes| {
            let captured = capture.clone();
            async move {
                // Inspect only the fixture marker; never persist or print the composed prompt.
                captured.store(
                    body.windows(MARKER.len())
                        .any(|part| part == MARKER.as_bytes()),
                    Ordering::SeqCst,
                );
                (
                    StatusCode::BAD_REQUEST,
                    "synthetic instruction capture complete",
                )
            }
        }),
    ))
    .await;
    fs::create_dir(home.path(".codex")).unwrap();
    fs::write(home.path(".codex/config.toml"),format!("model = \"fixture-model\"\nmodel_provider = \"nexusops_fixture\"\n[model_providers.nexusops_fixture]\nname = \"Local fixture\"\nbase_url = \"{gateway}v1\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nrequest_max_retries = 0\nstream_max_retries = 0\n")).unwrap();
    // Windows editors can leave a visually empty override containing only a UTF-8 BOM.
    fs::write(home.path(".codex/AGENTS.override.md"), b"\xef\xbb\xbf").unwrap();
    let before = read(&AppType::Codex, CONNECTION, 17).unwrap();
    let target = desired(
        &AppType::Codex,
        CONNECTION,
        17,
        Some(
            converted_rule(
                &home,
                &AppType::Codex,
                format!("Preserve the test marker {MARKER}.\n"),
            )
            .await,
        ),
    )
    .unwrap();
    apply(
        &AppType::Codex,
        CONNECTION,
        17,
        &target,
        Some(&fingerprint(&before)),
    )
    .unwrap();
    let workspace = home.path("workspace");
    fs::create_dir(&workspace).unwrap();
    let mut command = tokio::process::Command::new(binary);
    command
        .env_clear()
        .env("CODEX_HOME", home.path(".codex"))
        .current_dir(workspace)
        .args([
            "--ask-for-approval",
            "never",
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "Reply with fixture.",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(system) = std::env::var_os("SystemRoot") {
        command
            .env("SystemRoot", &system)
            .env("PATH", Path::new(&system).join("System32"));
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let result = tokio::time::timeout(Duration::from_secs(30), command.output()).await;
    server.abort();
    assert!(result.is_ok(), "CLI capture timed out");
    let output = result.unwrap().expect("launch CLI fixture");
    assert!(
        captured.load(Ordering::SeqCst),
        "CLI did not load the managed marker; exit={:?}",
        output.status.code()
    );
}

#[tokio::test]
#[serial_test::serial]
#[ignore = "requires NEXUSOPS_TEST_CLAUDE_BIN; launches the real CLI against a local capture endpoint"]
async fn real_claude_cli_loads_the_managed_rule_file() {
    use axum::{body::Bytes, response::IntoResponse, routing::post, Json, Router};
    use serde_json::json;
    use std::{
        process::Stdio,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };
    let binary =
        std::env::var_os("NEXUSOPS_TEST_CLAUDE_BIN").expect("explicit Claude binary path required");
    let home = Home::new();
    const MARKER: &str = "NEXUSOPS_CLAUDE_RULE_CAPTURE_4d9";
    let captured = Arc::new(AtomicBool::new(false));
    let capture = captured.clone();
    let(gateway,server)=crate::services::team::api::tests::serve(Router::new()
        .route("/v1/messages/count_tokens",post(||async{Json(json!({"input_tokens":10}))}))
        .route("/v1/messages",post(move |body:Bytes|{let capture=capture.clone();async move {
            capture.fetch_or(body.windows(MARKER.len()).any(|part|part==MARKER.as_bytes()),Ordering::SeqCst);
            let request:serde_json::Value=serde_json::from_slice(&body).unwrap_or_default();
            let message=json!({"id":"msg_fixture","type":"message","role":"assistant","model":request["model"],"content":[{"type":"text","text":"fixture"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}});
            if request["stream"]!=true{return Json(message).into_response();}
            let mut start=message;start["content"]=json!([]);start["stop_reason"]=serde_json::Value::Null;
            let events=[
                json!({"type":"message_start","message":start}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"fixture"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}}),
                json!({"type":"message_stop"}),
            ];
            let stream=events.iter().map(|event|format!("event: {}\ndata: {}\n\n",event["type"].as_str().unwrap(),event)).collect::<String>();
            ([("Content-Type","text/event-stream")],stream).into_response()
        }}))).await;
    let before = read(&AppType::Claude, CONNECTION, 17).unwrap();
    let target = desired(
        &AppType::Claude,
        CONNECTION,
        17,
        Some(
            converted_rule(
                &home,
                &AppType::Claude,
                format!("Preserve the test marker {MARKER}.\n"),
            )
            .await,
        ),
    )
    .unwrap();
    apply(
        &AppType::Claude,
        CONNECTION,
        17,
        &target,
        Some(&fingerprint(&before)),
    )
    .unwrap();
    let workspace = home.path("workspace");
    fs::create_dir(&workspace).unwrap();
    let mut command = tokio::process::Command::new(binary);
    command
        .env_clear()
        .env("CLAUDE_CONFIG_DIR", home.path(".claude"))
        .env("ANTHROPIC_BASE_URL", gateway.trim_end_matches('/'))
        .env("ANTHROPIC_API_KEY", "synthetic-fixture-key")
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .env("DISABLE_AUTOUPDATER", "1")
        .current_dir(workspace)
        .args([
            "--print",
            "--no-session-persistence",
            "--setting-sources",
            "user",
            "--strict-mcp-config",
            "--permission-mode",
            "plan",
            "Reply with fixture.",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system);
    }
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    if let Some(bash) = std::env::var_os("NEXUSOPS_TEST_GIT_BASH") {
        command.env("CLAUDE_CODE_GIT_BASH_PATH", bash);
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let result = tokio::time::timeout(Duration::from_secs(30), command.output()).await;
    server.abort();
    assert!(result.is_ok(), "CLI capture timed out");
    let output = result.unwrap().expect("launch CLI fixture");
    assert!(
        captured.load(Ordering::SeqCst),
        "CLI did not load the managed marker; exit={:?}",
        output.status.code()
    );
}
