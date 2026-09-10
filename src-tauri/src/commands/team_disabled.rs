const DISABLED: &str = "The NexusOps Team feature is disabled in this build";

#[tauri::command]
pub async fn team_tool_observation() -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_tool_usage_history() -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_clear_tool_usage_history() -> Result<(), String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn teamai_export_candidate(
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    drop(request);
    Err(DISABLED.into())
}

#[tauri::command]
pub fn teamai_ack_status(runtime: serde_json::Value) -> Result<serde_json::Value, String> {
    drop(runtime);
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn teamai_preview(
    runtime: serde_json::Value,
    asset_ids: Option<Vec<i64>>,
) -> Result<serde_json::Value, String> {
    drop((runtime, asset_ids));
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn teamai_apply(
    runtime: serde_json::Value,
    reviewed: serde_json::Value,
    legacy_reviewed: Option<serde_json::Value>,
    overwrite: serde_json::Value,
) -> Result<serde_json::Value, String> {
    drop((runtime, reviewed, legacy_reviewed, overwrite));
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn teamai_retry_acks(runtime: serde_json::Value) -> Result<serde_json::Value, String> {
    drop(runtime);
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn teamai_run(request: serde_json::Value) -> Result<serde_json::Value, String> {
    drop(request);
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_tool_usage_status() -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}
#[tauri::command]
pub async fn team_configure_tool_usage(_enabled: bool) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}
#[tauri::command]
pub async fn team_upload_tool_usage() -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_feature_enabled() -> bool {
    false
}

#[tauri::command]
pub fn team_status() -> Result<Option<serde_json::Value>, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn team_connect(_gateway: String, _key: String) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn team_refresh() -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_cancel() {}

#[tauri::command]
pub async fn team_wait_idle() -> Result<(), String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn team_disconnect(_remove_provider_credentials: bool) -> Result<(), String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_preview_provider(
    _app: String,
    _model: Option<String>,
) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_apply_provider(
    _app: String,
    _model: Option<String>,
    _confirm_update: bool,
    _decision_token: Option<String>,
) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_activate_provider(_app: String, _decision_token: Option<String>) -> Result<(), String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn team_preview_sync(_app: String) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn team_sync(
    _app: String,
    _overwrite_local: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub async fn team_restore_backup(
    _app: String,
    _asset_id: i64,
) -> Result<serde_json::Value, String> {
    Err(DISABLED.into())
}

#[tauri::command]
pub fn team_local_history(_app: String) -> Result<Vec<serde_json::Value>, String> {
    Err(DISABLED.into())
}
