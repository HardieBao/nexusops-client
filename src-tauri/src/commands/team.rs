use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use tauri::State;

use crate::{
    services::team::{
        api::Cancellation,
        provider::{self, ProviderPreview, TeamProviderError},
        sync::{self, SyncBatchResult, SyncOverwriteDecision, SyncPlan, TeamSyncError},
        types::{Manifest, TeamConnection, TeamError},
        TeamService,
    },
    store::AppState,
};

pub struct TeamServiceState {
    service: Result<Arc<TeamService>, TeamCommandError>,
    current_operation: Mutex<Option<Cancellation>>,
}

impl TeamServiceState {
    pub fn open(data_dir: &Path, app_state: &AppState) -> Self {
        let service = TeamService::open(data_dir).inspect(|service| {
            if let Err(error) = sync::repair_pending_upstream(service, app_state) {
                log::warn!("Team upstream recovery was deferred: {error}");
            }
        });
        Self {
            service: service.map(Arc::new).map_err(TeamCommandError::from),
            current_operation: Mutex::new(None),
        }
    }

    fn service(&self) -> Result<&TeamService, TeamCommandError> {
        self.service.as_ref().map(Arc::as_ref).map_err(Clone::clone)
    }

    fn begin_operation(&self) -> Cancellation {
        let cancellation = Cancellation::default();
        if let Ok(mut current) = self.current_operation.lock() {
            if let Some(previous) = current.replace(cancellation.clone()) {
                previous.cancel();
            }
        }
        cancellation
    }

    fn cancel(&self) {
        if let Ok(current) = self.current_operation.lock() {
            if let Some(cancellation) = current.as_ref() {
                cancellation.cancel();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamCommandError {
    pub code: String,
    pub message: String,
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
pub async fn team_disconnect(
    team: State<'_, TeamServiceState>,
    app_state: State<'_, AppState>,
    remove_provider_credentials: bool,
) -> Result<(), TeamCommandError> {
    team.cancel();
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
