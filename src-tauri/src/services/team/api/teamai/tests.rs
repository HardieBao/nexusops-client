use super::*;
use crate::services::team::{api::tests::serve, types::fixture_profile};
use axum::{
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const KEY: &str = "nx_synthetic_teamai_transport_key";
const ID: &str = "33a604cecb6756d44dd354df28c5d6029dcdf279e7f42f40f66e7ac2e15e5c90";

fn project() -> Project {
    Project {
        id: 1,
        name: "Fixture".into(),
        organization_id: "local".into(),
        workspace_id: "local".into(),
        member_id: 11,
    }
}
fn command() -> serde_json::Value {
    let now = Utc::now();
    json!({"command_id":ID,"asset_id":17,"revision_id":23,"kind":"rule","type":"install_rule","runtime":"codex","content_hash":"a".repeat(64),"download_url":"/api/v1/assets/17/revisions/23/download","issued_at":now,"expires_at":now+chrono::Duration::minutes(30),"outcome":null})
}

#[tokio::test]
async fn typed_protocol_round_trip_uses_only_dedicated_credentials() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let capture = calls.clone();
    let router=Router::new()
        .route("/api/v1/me/teamai/projects",get(|| async { Json(json!({"code":0,"data":{"schema_version":1,"projects":[project()]}})) }))
        .route("/api/v1/me/teamai/config",get(|| async { Json(json!({"code":0,"data":{"schema_version":1,"project_id":1,"runtimes":["codex","claude-code"],"resource_types":["skill","rule"],"operations":["projects","config","report","sync","ack"]}})) }))
        .route("/api/v1/me/teamai/report",post(move |headers:HeaderMap,Json(body):Json<serde_json::Value>| { let capture=capture.clone(); async move { capture.lock().unwrap().push((headers,body)); Json(json!({"code":0,"data":{"schema_version":1,"accepted":true,"server_time":Utc::now()}})) } }))
        .route("/api/v1/me/teamai/sync",post(|| async { Json(json!({"code":0,"data":{"schema_version":1,"commands":[command()]}})) }))
        .route("/api/v1/me/teamai/ack",post(|Json(body):Json<serde_json::Value>| async move { assert_eq!(body["command_id"],ID); Json(json!({"code":0,"data":{"schema_version":1,"command_id":ID,"outcome":"applied"}})) }));
    let (gateway, server) = serve(router).await;
    let api = TeamApi::new(&gateway).unwrap();
    let cancel = Cancellation::default();
    let project = api
        .teamai_project(KEY, &fixture_profile(), &cancel)
        .await
        .unwrap();
    api.teamai_config(KEY, &cancel).await.unwrap();
    api.teamai_report(KEY, Runtime::Codex, &cancel)
        .await
        .unwrap();
    let commands = api
        .teamai_sync(KEY, &project, Runtime::Codex, Some(&[17]), &cancel)
        .await
        .unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(
        api.teamai_ack(
            KEY,
            &project,
            &commands[0],
            Outcome::AlreadyCurrent,
            &cancel
        )
        .await
        .unwrap()
        .outcome,
        Outcome::Applied
    );
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded[0].0.get("X-Nexus-Member-Key").unwrap(), KEY);
    assert!(!recorded[0].0.contains_key("Authorization"));
    assert!(!recorded[0].0.contains_key("X-API-Token"));
    assert_eq!(
        recorded[0].1,
        json!({"schema_version":1,"project_id":1,"runtime":"codex","bridge_version":env!("CARGO_PKG_VERSION")})
    );
    server.abort();
}

#[tokio::test]
async fn project_scope_and_old_gateway_are_distinguished() {
    let (gateway, server) = serve(Router::new().route(
        "/api/v1/me/teamai/projects",
        get(|| async {
            let mut other = project();
            other.organization_id = "other".into();
            Json(json!({"code":0,"data":{"schema_version":1,"projects":[other]}}))
        }),
    ))
    .await;
    let api = TeamApi::new(&gateway).unwrap();
    assert_eq!(
        api.teamai_project(KEY, &fixture_profile(), &Cancellation::default())
            .await
            .unwrap_err(),
        TeamError::DifferentIdentity
    );
    assert_eq!(
        api.teamai_config(KEY, &Cancellation::default())
            .await
            .unwrap_err(),
        TeamError::UpgradeRequired
    );
    server.abort();
}

#[tokio::test]
async fn unsafe_commands_fail_before_any_installation() {
    for change in [
        "type",
        "member",
        "expiry",
        "url",
        "duplicate",
        "extra",
        "selection",
    ] {
        let mut item = command();
        match change {
            "type" => item["type"] = json!("install_plugin"),
            "member" => item["command_id"] = json!("b".repeat(64)),
            "expiry" => {
                item["issued_at"] = json!(Utc::now() - chrono::Duration::minutes(31));
                item["expires_at"] = json!(Utc::now() - chrono::Duration::minutes(1));
            }
            "url" => item["download_url"] = json!("https://foreign.invalid/private"),
            "extra" => item["prompt"] = json!("private body"),
            _ => {}
        }
        let items = if change == "duplicate" {
            json!([item.clone(), item])
        } else {
            json!([item])
        };
        let (gateway, server) = serve(Router::new().route(
            "/api/v1/me/teamai/sync",
            post(move || {
                let items = items.clone();
                async move { Json(json!({"code":0,"data":{"schema_version":1,"commands":items}})) }
            }),
        ))
        .await;
        let result = TeamApi::new(&gateway)
            .unwrap()
            .teamai_sync(
                KEY,
                &project(),
                Runtime::Codex,
                Some(if change == "selection" { &[99] } else { &[17] }),
                &Cancellation::default(),
            )
            .await;
        assert!(result.is_err(), "{change} unexpectedly accepted");
        server.abort();
    }
}

#[tokio::test]
async fn retry_after_is_bounded_and_no_post_is_automatically_replayed() {
    let count = Arc::new(AtomicUsize::new(0));
    let captured = count.clone();
    let (gateway, server) = serve(Router::new().route(
        "/api/v1/me/teamai/report",
        post(move || {
            let count = captured.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("Retry-After", "17")],
                    "private diagnostic",
                )
            }
        }),
    ))
    .await;
    assert_eq!(
        TeamApi::new(&gateway)
            .unwrap()
            .teamai_report(KEY, Runtime::Codex, &Cancellation::default())
            .await
            .unwrap_err(),
        TeamError::RateLimited {
            retry_after_seconds: 17
        }
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn post_redirect_does_not_forward_credentials() {
    let count = Arc::new(AtomicUsize::new(0));
    let captured = count.clone();
    let (foreign, foreign_server) = serve(Router::new().route(
        "/",
        post(move || {
            let count = captured.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                StatusCode::OK
            }
        }),
    ))
    .await;
    let (gateway, server) = serve(Router::new().route(
        "/api/v1/me/teamai/report",
        post(move || {
            let foreign = foreign.clone();
            async move { (StatusCode::TEMPORARY_REDIRECT, [("Location", foreign)]) }
        }),
    ))
    .await;
    assert_eq!(
        TeamApi::new(&gateway)
            .unwrap()
            .teamai_report(KEY, Runtime::Codex, &Cancellation::default())
            .await
            .unwrap_err(),
        TeamError::Redirect
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    server.abort();
    foreign_server.abort();
}

#[tokio::test]
async fn post_timeout_and_pre_cancel_are_explicit() {
    let (gateway, server) = serve(Router::new().route(
        "/api/v1/me/teamai/report",
        post(|| async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            StatusCode::OK
        }),
    ))
    .await;
    let api = TeamApi::with_timeout(&gateway, Duration::from_millis(30)).unwrap();
    assert_eq!(
        api.teamai_report(KEY, Runtime::Codex, &Cancellation::default())
            .await
            .unwrap_err(),
        TeamError::Timeout
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        api.teamai_report(KEY, Runtime::Codex, &cancel)
            .await
            .unwrap_err(),
        TeamError::Cancelled
    );
    server.abort();
}

#[test]
fn command_id_matches_the_cross_language_fixture() {
    assert_eq!(command_id(&project(), 17, 23, Runtime::Codex), ID);
}
