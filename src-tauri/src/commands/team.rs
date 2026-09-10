use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use tauri::{Manager, State};

use crate::{
    services::team::{
        api::Cancellation,
        provider::{self, ProviderPreview, TeamProviderError},
        sync::{self, SyncBatchResult, SyncOverwriteDecision, SyncPlan, TeamSyncError},
        teamai_sync,
        types::{Manifest, TeamConnection, TeamError},
        worker::{Operation, Worker, WorkerError},
        TeamService,
    },
    store::AppState,
};

pub struct TeamServiceState {
    service: Result<Arc<TeamService>, TeamCommandError>,
    current_operation: OperationSlot,
}

type OperationSlot = Arc<Mutex<Option<ActiveOperation>>>;
struct ActiveOperation {
    id: uuid::Uuid,
    cancellation: Cancellation,
    background: bool,
}
struct OperationGuard {
    id: uuid::Uuid,
    cancellation: Cancellation,
    slot: OperationSlot,
}
impl std::ops::Deref for OperationGuard {
    type Target = Cancellation;
    fn deref(&self) -> &Cancellation {
        &self.cancellation
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut current = self
            .slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current
            .as_ref()
            .is_some_and(|operation| operation.id == self.id)
        {
            *current = None;
        }
    }
}
fn claim_operation(slot: &OperationSlot, background: bool) -> Option<OperationGuard> {
    let mut current = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if background && current.is_some() {
        return None;
    }
    let id = uuid::Uuid::new_v4();
    let cancellation = Cancellation::default();
    if let Some(previous) = current.replace(ActiveOperation {
        id,
        cancellation: cancellation.clone(),
        background,
    }) {
        previous.cancellation.cancel();
    }
    Some(OperationGuard {
        id,
        cancellation,
        slot: slot.clone(),
    })
}
impl Drop for TeamServiceState {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod operation_tests {
    use super::*;
    #[test]
    fn foreground_preempts_background_without_losing_its_ownership() {
        let slot: OperationSlot = Arc::new(Mutex::new(None));
        let background = claim_operation(&slot, true).unwrap();
        let foreground = claim_operation(&slot, false).unwrap();
        assert!(background.is_cancelled());
        drop(background);
        assert!(claim_operation(&slot, true).is_none());
        assert!(!foreground.is_cancelled());
        drop(foreground);
        assert!(claim_operation(&slot, true).is_some());
    }

    #[test]
    fn state_shutdown_cancels_a_running_background_request() {
        let slot: OperationSlot = Arc::new(Mutex::new(None));
        let background = claim_operation(&slot, true).unwrap();
        let state = TeamServiceState {
            service: Err(TeamError::Storage.into()),
            current_operation: slot,
        };
        drop(state);
        assert!(background.is_cancelled());
    }
}

impl TeamServiceState {
    pub fn open(data_dir: &Path, app_state: &AppState) -> Self {
        let service = TeamService::open(data_dir).inspect(|service| {
            if let Err(error) = sync::repair_pending_upstream(service, app_state) {
                log::warn!("Team upstream recovery was deferred: {error}");
            }
        });
        let service = service.map(Arc::new).map_err(TeamCommandError::from);
        let current_operation: OperationSlot = Arc::new(Mutex::new(None));
        if let Ok(service) = &service {
            let weak = Arc::downgrade(service);
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let Some(service) = weak.upgrade() else {
                        break;
                    };
                    let _ = service.upload_tool_usage().await;
                }
            });
            let weak = Arc::downgrade(service);
            let operations = current_operation.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let Some(service) = weak.upgrade() else {
                        break;
                    };
                    if let Some(operation) = claim_operation(&operations, true) {
                        let _ = teamai_sync::retry_background(&service, &operation).await;
                    }
                }
            });
        }
        Self {
            service,
            current_operation,
        }
    }

    fn service(&self) -> Result<&TeamService, TeamCommandError> {
        self.service.as_ref().map(Arc::as_ref).map_err(Clone::clone)
    }

    fn begin_operation(&self) -> OperationGuard {
        claim_operation(&self.current_operation, false)
            .expect("foreground operation always claims its slot")
    }

    fn cancel(&self) {
        if let Ok(current) = self.current_operation.lock() {
            if let Some(operation) = current.as_ref() {
                operation.cancellation.cancel();
            }
        }
    }

    fn cancel_background(&self) {
        if let Ok(current) = self.current_operation.lock() {
            if let Some(operation) = current.as_ref().filter(|operation| operation.background) {
                operation.cancellation.cancel();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamCommandError {
    pub code: String,
    pub message: String,
}

impl From<WorkerError> for TeamCommandError {
    fn from(value: WorkerError) -> Self {
        Self {
            code: value.to_string(),
            message: value.to_string(),
        }
    }
}

#[derive(Serialize)]
pub struct TeamAIAckStatus {
    connection: Option<TeamConnection>,
    status: crate::services::team::state::teamai::AckQueueStatus,
}

#[tauri::command]
pub fn teamai_ack_status(
    team: State<'_, TeamServiceState>,
    runtime: crate::services::team::api::teamai::Runtime,
) -> Result<TeamAIAckStatus, TeamCommandError> {
    let (connection, status) = teamai_sync::ack_status(team.service()?, runtime)?;
    Ok(TeamAIAckStatus { connection, status })
}

#[tauri::command]
pub async fn teamai_preview(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    runtime: crate::services::team::api::teamai::Runtime,
    asset_ids: Option<Vec<i64>>,
) -> Result<teamai_sync::Preview, TeamCommandError> {
    let cancellation = team.begin_operation();
    teamai_sync::preview(
        team.service()?,
        app_state.inner(),
        runtime,
        asset_ids.as_deref(),
        &cancellation,
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn teamai_apply(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    runtime: crate::services::team::api::teamai::Runtime,
    reviewed: Vec<teamai_sync::ReviewedCommand>,
    legacy_reviewed: Option<Vec<teamai_sync::ReviewedAsset>>,
    overwrite: Vec<sync::SyncOverwriteDecision>,
) -> Result<teamai_sync::ApplyResult, TeamCommandError> {
    let cancellation = team.begin_operation();
    teamai_sync::apply(
        team.service()?,
        app_state.inner(),
        runtime,
        &reviewed,
        legacy_reviewed.as_deref().unwrap_or(&[]),
        &overwrite,
        &cancellation,
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn teamai_retry_acks(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    runtime: crate::services::team::api::teamai::Runtime,
) -> Result<teamai_sync::AckSummary, TeamCommandError> {
    let cancellation = team.begin_operation();
    teamai_sync::retry(team.service()?, app_state.inner(), runtime, &cancellation)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn teamai_run(
    app: tauri::AppHandle,
    team: State<'_, TeamServiceState>,
    request: Operation,
) -> Result<serde_json::Value, TeamCommandError> {
    let cancellation = team.begin_operation();
    let service = team.service()?;
    // Share ownership with team_wait_idle so cancel never unlocks the UI before this worker exits.
    let _operation = service.sync_operation.lock().await;
    let worker = load_teamai_worker(&app).await?;
    worker.run(request, &cancellation).await.map_err(Into::into)
}

async fn load_teamai_worker(app: &tauri::AppHandle) -> Result<Worker, TeamCommandError> {
    let directory = app
        .path()
        .resource_dir()
        .map_err(|_| WorkerError::WorkerUnavailable)?
        .join("teamai");
    #[cfg(debug_assertions)]
    let directory = if directory.exists() {
        directory
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.teamai-build")
    };
    let worker = tauri::async_runtime::spawn_blocking(move || Worker::open(&directory))
        .await
        .map_err(|_| WorkerError::WorkerUnavailable)??;
    Ok(worker)
}

#[tauri::command]
pub async fn teamai_export_candidate(
    app: tauri::AppHandle,
    team: State<'_, TeamServiceState>,
    request: crate::services::team::candidate::ExportRequest,
) -> Result<crate::services::team::candidate::ExportSummary, TeamCommandError> {
    let cancellation = team.begin_operation();
    let service = team.service()?;
    let _guard = tokio::select! {_ = cancellation.cancelled()=>return Err(TeamError::Cancelled.into()),guard=service.sync_operation.lock()=>guard};
    let worker = load_teamai_worker(&app).await?;
    crate::services::team::candidate::export(&worker, request, &cancellation)
        .await
        .map_err(Into::into)
}

impl From<TeamError> for TeamCommandError {
    fn from(value: TeamError) -> Self {
        Self {
            code: value.code().into(),
            message: value.to_string(),
        }
    }
}

impl From<TeamProviderError> for TeamCommandError {
    fn from(value: TeamProviderError) -> Self {
        Self {
            code: value.code().into(),
            message: value.to_string(),
        }
    }
}

impl From<TeamSyncError> for TeamCommandError {
    fn from(value: TeamSyncError) -> Self {
        Self {
            code: value.code().into(),
            message: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamRefresh {
    pub connection: TeamConnection,
    pub manifest: Manifest,
}

#[tauri::command]
pub fn team_feature_enabled() -> bool {
    true
}

#[tauri::command]
pub fn team_tool_usage_status(
    team: State<'_, TeamServiceState>,
) -> Result<crate::services::team::tool_usage::UsageStatus, TeamCommandError> {
    let service = team.service()?;
    let connection = service.status()?.ok_or(TeamError::NotConnected)?;
    service
        .tool_usage
        .status(&connection.id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn team_configure_tool_usage(
    team: State<'_, TeamServiceState>,
    enabled: bool,
) -> Result<crate::services::team::tool_usage::UsageStatus, TeamCommandError> {
    let service = team.service()?;
    service
        .configure_tool_usage(enabled)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn team_tool_usage_history(
    team: State<'_, TeamServiceState>,
) -> Result<Vec<crate::services::team::tool_usage::history::DailyUsage>, TeamCommandError> {
    let service = team.service()?;
    service.tool_usage_history().await.map_err(Into::into)
}

#[tauri::command]
pub async fn team_tool_observation(
    team: State<'_, TeamServiceState>,
) -> Result<Vec<crate::services::team::tool_usage::observation::ToolObservation>, TeamCommandError>
{
    team.service()?.tool_observation().await.map_err(Into::into)
}

#[tauri::command]
pub async fn team_clear_tool_usage_history(
    team: State<'_, TeamServiceState>,
) -> Result<(), TeamCommandError> {
    let service = team.service()?;
    service.clear_tool_usage_history().await.map_err(Into::into)
}

#[tauri::command]
pub async fn team_upload_tool_usage(
    team: State<'_, TeamServiceState>,
) -> Result<crate::services::team::tool_usage::UsageStatus, TeamCommandError> {
    team.service()?
        .upload_tool_usage()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn team_status(
    team: State<'_, TeamServiceState>,
) -> Result<Option<TeamConnection>, TeamCommandError> {
    team.service()?.status().map_err(Into::into)
}

#[tauri::command]
pub async fn team_connect(
    team: State<'_, TeamServiceState>,
    gateway: String,
    key: String,
) -> Result<TeamConnection, TeamCommandError> {
    let cancellation = team.begin_operation();
    team.service()?
        .connect(&gateway, &key, &cancellation)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn team_refresh(
    team: State<'_, TeamServiceState>,
) -> Result<TeamRefresh, TeamCommandError> {
    let cancellation = team.begin_operation();
    let service = team.service()?;
    let _sync = tokio::select! {
        _ = cancellation.cancelled() => return Err(TeamError::Cancelled.into()),
        lock = service.sync_operation.lock() => lock,
    };
    let (connection, manifest) = service.refresh(&cancellation).await?;
    Ok(TeamRefresh {
        connection,
        manifest,
    })
}

#[tauri::command]
pub fn team_cancel(team: State<'_, TeamServiceState>) {
    team.cancel();
}

#[tauri::command]
pub async fn team_wait_idle(team: State<'_, TeamServiceState>) -> Result<(), TeamCommandError> {
    team.cancel_background();
    team.service()?.wait_for_idle().await;
    Ok(())
}

#[tauri::command]
pub async fn team_disconnect(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    remove_provider_credentials: bool,
) -> Result<(), TeamCommandError> {
    let _operation = team.begin_operation();
    let service = team.service()?;
    let _sync = service.sync_operation.lock().await;
    if remove_provider_credentials {
        provider::scrub_provider_credentials(service, app_state.inner())?;
    }
    service.disconnect_locked().await.map_err(Into::into)
}

#[tauri::command]
pub async fn team_preview_provider(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    app: String,
    model: Option<String>,
) -> Result<ProviderPreview, TeamCommandError> {
    let cancellation = team.begin_operation();
    let service = team.service()?;
    let _sync = tokio::select! {
        _ = cancellation.cancelled() => return Err(TeamError::Cancelled.into()),
        lock = service.sync_operation.lock() => lock,
    };
    service.refresh(&cancellation).await?;
    provider::preview_provider(service, app_state.inner(), &app, model.as_deref())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn team_apply_provider(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    app: String,
    model: Option<String>,
    confirm_update: bool,
    decision_token: Option<String>,
) -> Result<ProviderPreview, TeamCommandError> {
    let cancellation = team.begin_operation();
    let service = team.service()?;
    let _sync = tokio::select! {
        _ = cancellation.cancelled() => return Err(TeamError::Cancelled.into()),
        lock = service.sync_operation.lock() => lock,
    };
    service.refresh(&cancellation).await?;
    provider::apply_reviewed_provider(
        service,
        app_state.inner(),
        &app,
        model.as_deref(),
        confirm_update,
        decision_token.as_deref(),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub async fn team_activate_provider(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    app: String,
    decision_token: Option<String>,
) -> Result<(), TeamCommandError> {
    let cancellation = team.begin_operation();
    let service = team.service()?;
    let _sync = tokio::select! {
        _ = cancellation.cancelled() => return Err(TeamError::Cancelled.into()),
        lock = service.sync_operation.lock() => lock,
    };
    service.refresh(&cancellation).await?;
    provider::activate_reviewed_provider(
        service,
        app_state.inner(),
        &app,
        decision_token.as_deref(),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub async fn team_preview_sync(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    app: String,
) -> Result<SyncPlan, TeamCommandError> {
    let cancellation = team.begin_operation();
    sync::preview_sync(team.service()?, app_state.inner(), &app, &cancellation)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn team_sync(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    app: String,
    overwrite_local: Vec<SyncOverwriteDecision>,
) -> Result<SyncBatchResult, TeamCommandError> {
    let cancellation = team.begin_operation();
    sync::sync_all(
        team.service()?,
        app_state.inner(),
        &app,
        &overwrite_local,
        &cancellation,
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn team_restore_backup(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    app: String,
    asset_id: i64,
) -> Result<sync::LocalRestoreResult, TeamCommandError> {
    sync::restore_local_backup(team.service()?, app_state.inner(), &app, asset_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn team_local_history(
    team: State<'_, TeamServiceState>,
    app: String,
) -> Result<Vec<sync::LocalAssetHistory>, TeamCommandError> {
    sync::local_history(team.service()?, &app).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_team_database_is_reported_without_panicking_app_setup() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("team.db"), b"not a sqlite database").unwrap();
        let app_state = AppState::new(Arc::new(crate::database::Database::memory().unwrap()));
        let state = TeamServiceState::open(temp.path(), &app_state);
        let error = state
            .service()
            .err()
            .expect("Team state should be unavailable");
        assert_eq!(error.code, "storage");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn a_new_operation_cancels_the_previous_token() {
        let temp = tempfile::tempdir().unwrap();
        let app_state = AppState::new(Arc::new(crate::database::Database::memory().unwrap()));
        let state = TeamServiceState::open(temp.path(), &app_state);
        let first = state.begin_operation();
        let second = state.begin_operation();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }
}
