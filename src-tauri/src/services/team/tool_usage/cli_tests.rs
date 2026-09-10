use super::*;
use axum::{routing::post, Json, Router};
use std::{fs, process::Stdio};

async fn exercise(runtime: &str, variable: &str) {
    let binary = std::env::var_os(variable).expect("explicit real CLI path required");
    let client = PathBuf::from(
        std::env::var_os("NEXUSOPS_TEST_CLIENT_BIN")
            .expect("explicit release client executable required"),
    );
    // Codex refuses its executable aliases beneath the system temp directory.
    // Keep the isolated CLI home in the owned build directory instead.
    let root = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    let config = root.path().join("tool config");
    let queue = root.path().join(if runtime == "codex" {
        "usage $data' quoted"
    } else {
        "usage data"
    });
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    let store = UsageStore::open(&queue).unwrap();
    store.bind_history("synthetic-workspace", 17).unwrap();
    store.configure("synthetic-workspace", true).unwrap();
    let hooks = hooks::merge(serde_json::json!({}), runtime, &client, &queue, true).unwrap();
    fs::write(
        config.join(if runtime == "codex" {
            "hooks.json"
        } else {
            "settings.json"
        }),
        serde_json::to_vec(&hooks).unwrap(),
    )
    .unwrap();
    let router = Router::new()
        .route("/v1/responses", post(|| async {
            let message=serde_json::json!({"id":"msg_fixture","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"fixture"}]});
            let events=[serde_json::json!({"type":"response.created","response":{"id":"resp_fixture","status":"in_progress"}}),serde_json::json!({"type":"response.output_item.done","output_index":0,"item":message}),serde_json::json!({"type":"response.completed","response":{"id":"resp_fixture","status":"completed","output":[message],"usage":{"input_tokens":10,"output_tokens":1,"total_tokens":11}}})];
            let stream=events.iter().map(|event|format!("data: {event}\n\n")).collect::<String>();
            ([("Content-Type","text/event-stream")],stream)
        }))
        .route("/v1/messages/count_tokens", post(|| async { Json(serde_json::json!({"input_tokens":10})) }))
        .route("/v1/messages",post(|| async {
            let events = [
                serde_json::json!({"type":"message_start","message":{"id":"msg_fixture","type":"message","role":"assistant","model":"fixture","content":[],"stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0}}}),
                serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"fixture"}}),
                serde_json::json!({"type":"content_block_stop","index":0}),
                serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}}),
                serde_json::json!({"type":"message_stop"}),
            ];
            let stream = events.iter().map(|event| format!("event: {}\ndata: {}\n\n",event["type"].as_str().unwrap(),event)).collect::<String>();
            ([("Content-Type","text/event-stream")],stream)
        }));
    let (gateway, server) = crate::services::team::api::tests::serve(router).await;
    let mut command = tokio::process::Command::new(binary);
    command
        .env_clear()
        .current_dir(&workspace)
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
    command.env("PATHEXT", ".COM;.EXE;.BAT;.CMD");
    for name in ["TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    if runtime == "codex" {
        fs::write(config.join("config.toml"),format!("model=\"fixture-model\"\nmodel_provider=\"local_fixture\"\n[model_providers.local_fixture]\nname=\"Local fixture\"\nbase_url=\"{gateway}v1\"\nwire_api=\"responses\"\nrequires_openai_auth=false\nrequest_max_retries=0\nstream_max_retries=0\n")).unwrap();
        command.env("CODEX_HOME", &config).args([
            "--ask-for-approval",
            "never",
            // Only the generated fixture hooks above are loaded. Production
            // onboarding still requires reviewing hooks in the CLI.
            "--dangerously-bypass-hook-trust",
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "Reply with fixture.",
        ]);
    } else {
        let bash =
            std::env::var_os("NEXUSOPS_TEST_GIT_BASH").expect("explicit Git Bash path required");
        command
            .env("CLAUDE_CODE_GIT_BASH_PATH", bash)
            .env("CLAUDE_CONFIG_DIR", &config)
            .env("ANTHROPIC_BASE_URL", gateway.trim_end_matches('/'))
            .env("ANTHROPIC_API_KEY", "synthetic-fixture-key")
            .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
            .env("DISABLE_AUTOUPDATER", "1")
            .args([
                "--print",
                "--no-session-persistence",
                "--setting-sources",
                "user",
                "--strict-mcp-config",
                "--permission-mode",
                "plan",
                "Reply with fixture.",
            ]);
    }
    let result = tokio::time::timeout(Duration::from_secs(45), command.output()).await;
    server.abort();
    let output = result
        .expect("real CLI timed out")
        .expect("real CLI failed to launch");
    let events = store.pending().unwrap();
    if events.is_empty() {
        for line in String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.contains("hook"))
            .take(8)
        {
            eprintln!(
                "isolated hook diagnostic: {}",
                line.chars().take(1200).collect::<String>()
            );
        }
        for line in String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| {
                line.to_lowercase().contains("error")
                    || line.to_lowercase().contains("hook")
                    || line.to_lowercase().contains("warning")
                    || line.contains("unknown variant")
            })
            .take(6)
        {
            eprintln!(
                "isolated CLI diagnostic: {}",
                line.chars().take(500).collect::<String>()
            );
        }
    }
    assert!(
        !events.is_empty(),
        "no real {runtime} Hook events; CLI exit={:?}",
        output.status.code()
    );
    assert!(events.iter().all(|event| event.runtime == runtime));
    assert!(events.iter().any(|event| event.event == "session.started"));
    let history = store.history("synthetic-workspace").unwrap();
    assert_eq!(
        history.iter().map(|row| row.count).sum::<i64>(),
        events.len() as i64
    );
    store.acknowledge(&events).unwrap();
    assert!(store.pending().unwrap().is_empty());
    assert_eq!(
        store
            .history("synthetic-workspace")
            .unwrap()
            .iter()
            .map(|row| row.count)
            .sum::<i64>(),
        events.len() as i64
    );
    println!("{runtime}: {} real lifecycle events reached the release collector; history retained after ACK",events.len());
}

#[tokio::test]
#[ignore = "requires actual Codex and release collector paths; no personal configuration is read"]
async fn real_codex_calls_release_collector() {
    exercise("codex", "NEXUSOPS_TEST_CODEX_BIN").await;
}

#[tokio::test]
#[ignore = "requires actual Claude, Git Bash and release collector paths; synthetic local model only"]
async fn real_claude_calls_release_collector() {
    exercise("claude-code", "NEXUSOPS_TEST_CLAUDE_BIN").await;
}
