use super::*;
use crate::{
    database::Database,
    services::team::{
        api::tests::serve, credentials::fixtures::MemoryCredentialStore, types::fixture_profile,
    },
};
use axum::{
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Home(tempfile::TempDir);
impl Home {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        crate::settings::reload_settings().unwrap();
        Self(temp)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
        let _ = crate::settings::reload_settings();
    }
}

#[tokio::test]
#[serial_test::serial]
async fn skill_install_queues_failed_ack_and_retry_does_not_download_again() {
    let home = Home::new();
    let downloads = Arc::new(AtomicUsize::new(0));
    let acks = Arc::new(AtomicUsize::new(0));
    let download_counter = downloads.clone();
    let ack_counter = acks.clone();
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/protocol-v1.json")).unwrap();
    let skill = vectors["skill"].clone();
    let bytes = STANDARD
        .decode(include_str!("../testdata/skill-input.tar.gz.base64").trim())
        .unwrap();
    let asset = json!({"asset_id":17,"kind":"skill","slug":"fixture","name":"Fixture","revision":23,"content_hash":skill["content_hash"],"archive_sha256":skill["input_archive_sha256"],"download_url":"/api/v1/assets/17/revisions/23/download","content_type":"application/gzip","byte_size":bytes.len(),"files":[]});
    let command = json!({"command_id":"33a604cecb6756d44dd354df28c5d6029dcdf279e7f42f40f66e7ac2e15e5c90","asset_id":17,"revision_id":23,"kind":"skill","type":"install_skill","runtime":"codex","content_hash":skill["content_hash"],"download_url":"/api/v1/assets/17/revisions/23/download","issued_at":Utc::now(),"expires_at":Utc::now()+chrono::Duration::minutes(29),"outcome":null});
    let prompt_body = b"Fixture inactive prompt";
    let prompt_hash = crate::services::team::content::sha256_hex(prompt_body);
    let prompt = json!({"asset_id":19,"kind":"prompt","slug":"fixture-prompt","name":"Fixture prompt","revision":7,"content_hash":prompt_hash,"archive_sha256":null,"download_url":"/api/v1/assets/19/revisions/7/download","content_type":"text/plain; charset=utf-8","byte_size":prompt_body.len(),"files":[]});
    let router=Router::new()
        .route("/api/v1/me/team-profile",get(|headers:HeaderMap|async move {let mut profile=fixture_profile();profile.gateway_url=format!("http://{}/",headers.get("host").unwrap().to_str().unwrap());profile.base_url=format!("{}v1",profile.gateway_url);Json(json!({"code":0,"data":profile}))}))
        .route("/api/v1/me/assets/manifest",get(move ||{let asset=asset.clone();let prompt=prompt.clone();async move{Json(json!({"code":0,"data":{"schema_version":1,"assets":[asset,prompt],"conflicts":[]}}))}}))
        .route("/api/v1/assets/19/revisions/7/download",get(move ||async move{prompt_body.to_vec()}))
        .route("/api/v1/assets/17/revisions/23/download",get(move ||{let bytes=bytes.clone();let count=download_counter.clone();async move{count.fetch_add(1,Ordering::SeqCst);bytes}}))
        .route("/api/v1/me/teamai/projects",get(||async{Json(json!({"code":0,"data":{"schema_version":1,"projects":[{"id":1,"name":"Fixture","organization_id":"local","workspace_id":"local","member_id":11}]}}))}))
        .route("/api/v1/me/teamai/config",get(||async{Json(json!({"code":0,"data":{"schema_version":1,"project_id":1,"runtimes":["codex","claude-code"],"resource_types":["skill","rule"],"operations":["projects","config","report","sync","ack"]}}))}))
        .route("/api/v1/me/teamai/report",post(||async{Json(json!({"code":0,"data":{"schema_version":1,"accepted":true,"server_time":Utc::now()}}))}))
        .route("/api/v1/me/teamai/sync",post(move |Json(body):Json<serde_json::Value>|{let command=command.clone();async move{let commands=if body.get("asset_ids").is_none() || body["asset_ids"].as_array().unwrap().contains(&json!(17)){vec![command]}else{vec![]};Json(json!({"code":0,"data":{"schema_version":1,"commands":commands}}))}}))
        .route("/api/v1/me/teamai/ack",post(move ||{let count=ack_counter.clone();async move{if count.fetch_add(1,Ordering::SeqCst)==0 {(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"code":503}))) }else{(StatusCode::OK,Json(json!({"code":0,"data":{"schema_version":1,"command_id":"33a604cecb6756d44dd354df28c5d6029dcdf279e7f42f40f66e7ac2e15e5c90","outcome":"applied"}})))}}}));
    let (gateway, server) = serve(router).await;
    let credentials = Arc::new(MemoryCredentialStore::default());
    let team =
        TeamService::with_credentials(&home.0.path().join("state"), credentials.clone()).unwrap();
    let state = AppState::new(Arc::new(Database::memory().unwrap()));
    let cancel = Cancellation::default();
    let connection = team
        .connect(&gateway, "nx_fixture_teamai", &cancel)
        .await
        .unwrap();
    let review = preview(&team, &state, Runtime::Codex, Some(&[17]), &cancel)
        .await
        .unwrap();
    team.state
        .prepare_teamai_ack(
            &connection.id,
            "codex",
            &AckIntent {
                project: review.project.clone(),
                command: review.commands[0].clone(),
            },
        )
        .unwrap();
    retry_background(&team, &cancel).await.unwrap();
    assert_eq!(
        acks.load(Ordering::SeqCst),
        0,
        "uncommitted intents cannot be acknowledged"
    );
    let stale = [ReviewedCommand {
        asset_id: 17,
        command_id: "stale".into(),
    }];
    assert!(
        apply(&team, &state, Runtime::Codex, &stale, &[], &[], &cancel)
            .await
            .is_err()
    );
    assert_eq!(downloads.load(Ordering::SeqCst), 0);
    let reviewed = review
        .commands
        .iter()
        .map(|command| ReviewedCommand {
            asset_id: command.asset_id,
            command_id: command.command_id.clone(),
        })
        .collect::<Vec<_>>();
    assert_eq!(review.legacy_items.len(), 1);
    let invalid_legacy = [ReviewedAsset {
        asset_id: 19,
        revision: 8,
        content_hash: prompt_hash.clone(),
    }];
    assert!(apply(
        &team,
        &state,
        Runtime::Codex,
        &reviewed,
        &invalid_legacy,
        &[],
        &cancel
    )
    .await
    .is_err());
    let bypass = [ReviewedAsset {
        asset_id: 17,
        revision: 23,
        content_hash: skill["content_hash"].as_str().unwrap().into(),
    }];
    assert!(
        apply(&team, &state, Runtime::Codex, &[], &bypass, &[], &cancel)
            .await
            .is_err()
    );
    assert_eq!(downloads.load(Ordering::SeqCst), 0);
    let legacy = [ReviewedAsset {
        asset_id: 19,
        revision: 7,
        content_hash: prompt_hash,
    }];
    let result = apply(
        &team,
        &state,
        Runtime::Codex,
        &reviewed,
        &legacy,
        &[],
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(result.install.items[0].outcome, SyncOutcome::Installed);
    assert_eq!(
        result.install.items[1].outcome,
        SyncOutcome::ImportedInactive
    );
    assert_eq!(result.acknowledgements.waiting, 1);
    assert_eq!(acks.load(Ordering::SeqCst), 1);
    let path = std::path::Path::new(&result.install.items[0].install_path).join("SKILL.md");
    let before = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    drop(team);
    let team = TeamService::with_credentials(&home.0.path().join("state"), credentials).unwrap();
    let waiting = retry(&team, &state, Runtime::Codex, &cancel).await.unwrap();
    assert_eq!(waiting.waiting, 1);
    assert_eq!(acks.load(Ordering::SeqCst), 1);
    // Advance only the synthetic queue's persisted deadline, not production timing policy.
    let database = rusqlite::Connection::open(home.0.path().join("state/team.db")).unwrap();
    database
        .execute("UPDATE teamai_ack_queue SET retry_at=NULL", [])
        .unwrap();
    drop(database);
    let foreground = team.sync_operation.lock().await;
    retry_background(&team, &cancel).await.unwrap();
    assert_eq!(
        acks.load(Ordering::SeqCst),
        1,
        "busy foreground work wins over a background tick"
    );
    drop(foreground);
    retry_background(&team, &cancel).await.unwrap();
    assert_eq!(ack_status(&team, Runtime::Codex).unwrap().1.pending, 0);
    assert_eq!(acks.load(Ordering::SeqCst), 2);
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert!(team
        .state
        .pending_teamai_acks(&connection.id, "codex")
        .unwrap()
        .is_empty());
    server.abort();
}
