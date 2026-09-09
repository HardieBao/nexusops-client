use super::*;
use crate::services::team::{credentials::fixtures::MemoryCredentialStore, types::fixture_profile};
use axum::{
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn queue_retries_identical_ids_and_disconnect_stops_collection() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let counter = attempts.clone();
    let received = bodies.clone();
    let router = Router::new()
        .route(
            "/api/v1/me/team-profile",
            get(|headers: HeaderMap| async move {
                let mut profile = fixture_profile();
                let gateway = format!("http://{}/", headers["host"].to_str().unwrap());
                profile.gateway_url = gateway.clone();
                profile.base_url = format!("{gateway}v1");
                Json(serde_json::json!({"code":0,"data":profile}))
            }),
        )
        .route(
            "/api/v1/me/tool-usage",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let counter = counter.clone();
                let received = received.clone();
                async move {
                    assert_eq!(headers["x-nexus-member-key"], "nx_synthetic_usage");
                    assert!(!body.to_string().contains("PRIVATE"));
                    received.lock().unwrap().push(body);
                    if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                        StatusCode::SERVICE_UNAVAILABLE
                    } else {
                        StatusCode::NO_CONTENT
                    }
                }
            }),
        );
    let (gateway, server) = crate::services::team::api::tests::serve(router).await;
    let dir = tempfile::tempdir().unwrap();
    let service =
        TeamService::with_credentials(dir.path(), Arc::new(MemoryCredentialStore::default()))
            .unwrap();
    let connection = service
        .connect(&gateway, "nx_synthetic_usage", &Cancellation::default())
        .await
        .unwrap();
    service.tool_usage.configure(&connection.id, true).unwrap();
    service
        .tool_usage
        .capture(
            "codex",
            &serde_json::json!({"hook_event_name":"UserPromptSubmit","prompt":"PRIVATE"}),
        )
        .unwrap();
    assert_eq!(
        service.upload_tool_usage().await.unwrap_err(),
        TeamError::Unavailable
    );
    assert_eq!(
        service.tool_usage.status(&connection.id).unwrap().pending,
        1
    );
    assert_eq!(service.upload_tool_usage().await.unwrap().uploaded, 1);
    {
        let received = bodies.lock().unwrap();
        assert_eq!(received[0], received[1]);
    }
    assert_eq!(service.upload_tool_usage().await.unwrap().pending, 0);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    service.disconnect().await.unwrap();
    service
        .tool_usage
        .capture(
            "claude-code",
            &serde_json::json!({"hook_event_name":"SessionStart"}),
        )
        .unwrap();
    assert!(service.tool_usage.pending().unwrap().is_empty());
    assert_eq!(
        service.upload_tool_usage().await.unwrap_err(),
        TeamError::NotConnected
    );
    server.abort();
}

#[tokio::test]
async fn upload_refuses_redirects_and_authentication_failures() {
    for (status, error) in [
        (302, TeamError::Redirect),
        (401, TeamError::AuthenticationRequired),
        (403, TeamError::AccessDenied),
    ] {
        let router = Router::new().route(
            "/api/v1/me/tool-usage",
            post(move || async move {
                axum::response::Response::builder()
                    .status(status)
                    .header("Location", "http://127.0.0.1:1/private")
                    .body(axum::body::Body::empty())
                    .unwrap()
            }),
        );
        let (gateway, server) = crate::services::team::api::tests::serve(router).await;
        let result = TeamApi::new(&gateway)
            .unwrap()
            .upload_tool_usage("nx_synthetic_usage", &[], &Cancellation::default())
            .await;
        assert_eq!(result.unwrap_err(), error);
        server.abort();
    }
}
