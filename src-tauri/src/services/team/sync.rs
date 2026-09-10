use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

use super::rules;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    prompt::Prompt,
    services::{PromptService, SkillService},
    store::AppState,
    AppType,
};

use super::{
    api::Cancellation,
    content::ContentLimits,
    install::{
        classify_drift, inspect_local_with_limits, inspect_skill_physical_with_limits,
        install_verified_with_limits_and_commit, DriftStatus, InstallError, InstallPolicy,
        InstallReceipt,
    },
    types::{
        AssetKind, AssetLimits, ManagedAssetState, Manifest, ManifestItem, PendingDiskState,
        TeamConnection, TeamError,
    },
    TeamService,
};

pub(crate) fn recover_known_installs(state: &super::state::TeamState) -> Result<usize, TeamError> {
    let mut roots: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    if let Some(connection) = state.connection()? {
        if !valid_connection_id(&connection.id) {
            return Err(TeamError::Storage);
        }
        for app in ["claude", "codex", "gemini", "grokbuild", "opencode"] {
            add_recovery_roots(&mut roots, &connection.id, app)?;
        }
    }
    let mut asset_states = state.all_asset_states()?;
    for asset in &mut asset_states {
        if !valid_connection_id(&asset.connection_id) {
            return Err(TeamError::Storage);
        }
        add_recovery_roots(&mut roots, &asset.connection_id, &asset.app)?;
        if let Some(operation) = asset.install_operation.as_ref() {
            if uuid::Uuid::parse_str(operation).is_err() {
                return Err(TeamError::Storage);
            }
            let app = supported_app(&asset.app).map_err(|_| TeamError::Storage)?;
            let (skill_root, managed_root) = recovery_roots(&asset.connection_id, &app)?;
            let expected = if asset.asset_kind == Some(AssetKind::Skill) {
                skill_root
            } else if asset.asset_kind.is_some() {
                managed_root
            } else {
                return Err(TeamError::Storage);
            };
            if !stored_path_matches(&asset.install_root, &expected) {
                // The tool's configured Skill root may legitimately change
                // after a completed install. Do not follow a stale SQLite path;
                // discard the obsolete cleanup marker and use the new root for
                // future installs.
                asset.install_operation = None;
                state.save_asset_state(asset)?;
                continue;
            }
            roots.entry(expected).or_default().insert(operation.clone());
        }
    }
    let mut recovered = 0usize;
    for (root, committed) in roots {
        if root.exists() {
            recovered = recovered
                .checked_add(
                    super::install::recover_installs_with_committed(&root, &committed).map_err(
                        |error| match error {
                            InstallError::RecoveryConflict(_) => TeamError::RecoveryConflict,
                            _ => TeamError::Storage,
                        },
                    )?,
                )
                .ok_or(TeamError::Storage)?;
        }
    }
    // Every journal under the currently trusted roots has now either been
    // rolled back or finalized. Operation UUIDs are cleanup markers, not
    // permanent asset identity, so retaining them would couple future startup
    // to an obsolete tool-root override.
    for mut asset in asset_states {
        if asset.install_operation.is_some() {
            asset.install_operation = None;
            state.save_asset_state(&asset)?;
        }
    }
    Ok(recovered)
}

fn add_recovery_roots(
    roots: &mut HashMap<PathBuf, HashSet<String>>,
    connection_id: &str,
    app: &str,
) -> Result<(), TeamError> {
    let app = supported_app(app).map_err(|_| TeamError::Storage)?;
    let (skill_root, managed_root) = recovery_roots(connection_id, &app)?;
    roots.entry(skill_root).or_default();
    roots.entry(managed_root).or_default();
    Ok(())
}

fn recovery_roots(connection_id: &str, app: &AppType) -> Result<(PathBuf, PathBuf), TeamError> {
    let skill_root = SkillService::get_app_skills_dir(app).map_err(|_| TeamError::Storage)?;
    let managed_root = crate::config::get_app_config_dir()
        .join("team")
        .join("managed")
        .join(&connection_id[..12])
        .join(app.as_str());
    Ok((skill_root, managed_root))
}

fn valid_connection_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncSupport {
    ToolSkill,
    ToolRule,
    InactivePrompt,
    ManagedDownload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncPlanItem {
    pub asset: ManifestItem,
    pub drift: DriftStatus,
    pub support: SyncSupport,
    pub install_path: String,
    #[serde(default)]
    pub activation_path: Option<String>,
    pub previous_revision: Option<i64>,
    pub subscribed: bool,
    pub has_backup: bool,
    pub last_synced_at: Option<String>,
    pub decision_token: String,
    pub disk_fingerprint: Option<String>,
    pub upstream_fingerprint: Option<String>,
    pub inspection_error_code: Option<String>,
    pub inspection_error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncOverwriteDecision {
    pub asset_id: i64,
    pub decision_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncPlan {
    pub connection: TeamConnection,
    pub conflicts: Vec<super::types::ManifestConflict>,
    pub items: Vec<SyncPlanItem>,
    pub withdrawn: Vec<LocalAssetHistory>,
    pub last_successful_sync: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalAssetHistory {
    pub asset_id: i64,
    pub name: String,
    pub kind: Option<AssetKind>,
    pub revision: i64,
    pub last_synced_at: Option<String>,
    pub has_backup: bool,
    pub local_file_present: bool,
    pub recovery_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Installed,
    Downloaded,
    ImportedInactive,
    Unchanged,
    Skipped,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncItemResult {
    pub asset_id: i64,
    pub revision: i64,
    pub outcome: SyncOutcome,
    pub drift: DriftStatus,
    pub install_path: String,
    pub backup_path: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncBatchResult {
    pub connection: TeamConnection,
    pub conflicts: Vec<super::types::ManifestConflict>,
    pub items: Vec<SyncItemResult>,
    pub last_successful_sync: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRestoreResult {
    pub asset_id: i64,
    pub restored: bool,
    pub has_undo_backup: bool,
    pub upstream_pending: bool,
    pub error_message: Option<String>,
}

pub fn local_history(
    team: &TeamService,
    app: &str,
) -> Result<Vec<LocalAssetHistory>, TeamSyncError> {
    let connection = team.status()?.ok_or(TeamError::NotConnected)?;
    let app = supported_app(app)?;
    team.state
        .asset_states(&connection.id)?
        .into_iter()
        .filter(|state| state.app == app.as_str())
        .map(|state| {
            Ok(LocalAssetHistory {
                asset_id: state.asset_id,
                name: if state.asset_name.is_empty() {
                    format!("Asset {}", state.asset_id)
                } else {
                    state.asset_name
                },
                kind: state.asset_kind,
                revision: state.revision,
                last_synced_at: state.last_synced_at,
                has_backup: state.backup_root_path.is_some() || state.backup_path.is_some(),
                local_file_present: Path::new(&state.install_path).exists(),
                recovery_error: state.upstream_pending.then(|| {
                    "A local Team recovery is pending; retry or restore its backup".into()
                }),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamBackup {
    version: u32,
    #[serde(default)]
    limits: Option<AssetLimits>,
    previous_state: Option<ManagedAssetState>,
    previous_prompt: Option<Prompt>,
    #[serde(default)]
    previous_rule: Option<rules::Snapshot>,
}

const UPSTREAM_BACKUP_FILE: &str = "upstream.json";

#[derive(Debug, thiserror::Error)]
pub enum TeamSyncError {
    #[error(transparent)]
    Team(#[from] TeamError),
    #[error("unsupported Team sync app: {0}")]
    UnsupportedApp(String),
    #[error("could not resolve the Team installation path")]
    Path,
    #[error("local Team state could not be inspected: {0}")]
    Local(String),
}

impl TeamSyncError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Team(error) => error.code(),
            Self::UnsupportedApp(_) => "unsupported_app",
            Self::Path => "install_path",
            Self::Local(_) => "local_state",
        }
    }
}

pub async fn preview_sync(
    team: &TeamService,
    app_state: &AppState,
    app: &str,
    cancel: &Cancellation,
) -> Result<SyncPlan, TeamSyncError> {
    let _sync = tokio::select! {
        _ = cancel.cancelled() => return Err(TeamError::Cancelled.into()),
        lock = team.sync_operation.lock() => lock,
    };
    recover_known_installs(&team.state)?;
    let repairs = repair_pending_upstream(team, app_state)?;
    load_plan(team, app_state, app, cancel, &repairs.errors).await
}

pub(super) async fn load_plan(
    team: &TeamService,
    app_state: &AppState,
    app: &str,
    cancel: &Cancellation,
    pending_errors: &HashMap<(String, i64), String>,
) -> Result<SyncPlan, TeamSyncError> {
    let app_type = supported_app(app)?;
    let (connection, manifest) = team.refresh(cancel).await?;
    reconcile_removed(team, &connection, &manifest, app_type.as_str())?;
    build_plan(
        team,
        app_state,
        connection,
        manifest,
        &app_type,
        pending_errors,
    )
}

pub async fn sync_all(
    team: &TeamService,
    app_state: &AppState,
    app: &str,
    overwrite_local: &[SyncOverwriteDecision],
    cancel: &Cancellation,
) -> Result<SyncBatchResult, TeamSyncError> {
    let _sync = tokio::select! {
        _ = cancel.cancelled() => return Err(TeamError::Cancelled.into()),
        lock = team.sync_operation.lock() => lock,
    };
    recover_known_installs(&team.state)?;
    let repairs = repair_pending_upstream(team, app_state)?;
    let plan = load_plan(team, app_state, app, cancel, &repairs.errors).await?;
    apply_plan(team, app_state, app, overwrite_local, cancel, plan, None).await
}

// Caller holds TeamService::sync_operation and supplies a freshly validated plan.
pub(super) async fn apply_plan(
    team: &TeamService,
    app_state: &AppState,
    app: &str,
    overwrite_local: &[SyncOverwriteDecision],
    cancel: &Cancellation,
    plan: SyncPlan,
    deadline: Option<chrono::DateTime<Utc>>,
) -> Result<SyncBatchResult, TeamSyncError> {
    let app_type = supported_app(app)?;
    let overwrite = overwrite_local
        .iter()
        .map(|decision| (decision.asset_id, decision.decision_token.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let limits = ContentLimits::try_from(&plan.connection.profile.limits)
        .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    let previous = team.state.asset_states(&plan.connection.id)?;
    let mut results = Vec::with_capacity(plan.items.len());
    let mut remote_access_error: Option<(String, String)> = None;

    for item in &plan.items {
        if deadline.is_some_and(|expires| expires <= Utc::now()) {
            return Err(TeamError::Conflict.into());
        }
        if cancel.is_cancelled() {
            return Err(TeamError::Cancelled.into());
        }
        if let Some((code, message)) = remote_access_error.as_ref() {
            results.push(result_for(
                item,
                SyncOutcome::Skipped,
                None,
                Some((code, message)),
            ));
            continue;
        }
        let prior = previous.iter().find(|state| {
            state.app == app_type.as_str()
                && state.asset_id == item.asset.asset_id
                && !state.restored_unmanaged
        });
        if let Some(message) = item.inspection_error_message.as_deref() {
            results.push(result_for(
                item,
                SyncOutcome::Failed,
                None,
                Some((
                    item.inspection_error_code
                        .as_deref()
                        .unwrap_or("local_state"),
                    message,
                )),
            ));
            continue;
        }
        if item.drift == DriftStatus::Same {
            if !plan_item_is_current(
                app_state,
                &plan.connection,
                &app_type,
                item,
                prior,
                &previous,
                limits,
            )? {
                results.push(result_for(
                    item,
                    SyncOutcome::Conflict,
                    None,
                    Some((
                        "preview_stale",
                        "The asset or local state changed after preview; review it again",
                    )),
                ));
                continue;
            }
            if let Some(prior) = prior {
                let mut current = prior.clone();
                current.asset_name = item.asset.name.clone();
                current.asset_kind = Some(item.asset.kind.clone());
                current.revision = item.asset.revision;
                current.content_hash = item.asset.content_hash.clone();
                current.archive_sha256 = item.asset.archive_sha256.clone();
                current.asset_files = item.asset.files.clone();
                current.subscribed = true;
                current.last_synced_at = Some(Utc::now().to_rfc3339());
                current.upstream_fingerprint = item.upstream_fingerprint.clone();
                current.shared_upstream_fingerprint = if item.asset.kind == AssetKind::Skill {
                    Some(
                        current_shared_skill_fingerprint(
                            app_state,
                            &plan.connection,
                            &app_type,
                            &item.asset,
                        )?
                        .0,
                    )
                } else {
                    None
                };
                current.pending_previous_upstream_fingerprint = None;
                team.state.save_installed_asset_state(&current)?;
            }
            results.push(result_for(item, SyncOutcome::Unchanged, None, None));
            continue;
        }
        let overwrite_requested = overwrite.get(&item.asset.asset_id).copied();
        let overwrite_allowed = overwrite_requested == Some(item.decision_token.as_str());
        if matches!(
            item.drift,
            DriftStatus::LocalModified | DriftStatus::BothModified
        ) && !overwrite_allowed
        {
            let (code, message) = if overwrite_requested.is_some() {
                (
                    "preview_stale",
                    "The asset or local file changed after preview; review it again",
                )
            } else {
                (
                    "local_modified",
                    "Local changes require an explicit overwrite decision",
                )
            };
            results.push(result_for(
                item,
                SyncOutcome::Conflict,
                None,
                Some((code, message)),
            ));
            continue;
        }
        if item.asset.kind == AssetKind::Prompt {
            if let Some(prior) = prior {
                let clean = prompt_state_fingerprint(&prior.local_hash, false);
                if item.upstream_fingerprint.as_deref() != Some(clean.as_str())
                    && !overwrite_allowed
                {
                    results.push(result_for(
                        item,
                        SyncOutcome::Conflict,
                        None,
                        Some((
                            "prompt_modified",
                            "Resolve the Team prompt in the Prompt page before syncing it",
                        )),
                    ));
                    continue;
                }
            }
            let prompt_id = managed_item_id(&plan.connection, &app_type, item.asset.asset_id);
            let prompts = PromptService::get_prompts(app_state, app_type.clone())
                .map_err(|error| TeamSyncError::Local(error.to_string()))?;
            if prompts.get(&prompt_id).is_some_and(|prompt| prompt.enabled) {
                results.push(result_for(
                    item,
                    SyncOutcome::Conflict,
                    None,
                    Some((
                        "active_prompt",
                        "Disable the active Team prompt before replacing it",
                    )),
                ));
                continue;
            }
        }

        let (root, relative, _) = target_for(&plan.connection, &app_type, &item.asset)?;
        if let Err(error) = fs::create_dir_all(&root) {
            results.push(result_for(
                item,
                SyncOutcome::Failed,
                None,
                Some(("install_path", &error.to_string())),
            ));
            continue;
        }
        let bytes = match team.download(&item.asset, cancel).await {
            Ok(bytes) => bytes,
            Err(error) => {
                let code = error.code();
                let message = error.to_string();
                results.push(result_for(
                    item,
                    SyncOutcome::Failed,
                    None,
                    Some((code, &message)),
                ));
                if matches!(
                    error,
                    TeamError::AuthenticationRequired | TeamError::AccessDenied
                ) {
                    remote_access_error = Some((code.into(), message));
                }
                continue;
            }
        };
        let previous_state = prior.cloned();
        let mut upstream_warning = None;
        if deadline.is_some_and(|expires| expires <= Utc::now()) {
            return Err(TeamError::Conflict.into());
        }
        // The canonical payload can already exist while its tool activation is missing or changed.
        // Force the commit callback in that case, without changing the payload's bytes.
        let rule_activation_only = item.support == SyncSupport::ToolRule
            && item.disk_fingerprint.as_deref() == Some(item.asset.content_hash.as_str());
        let receipt = match install_verified_with_limits_and_commit(
            &item.asset,
            &bytes,
            &root,
            &relative,
            if rule_activation_only {
                None
            } else {
                prior.map(|state| state.local_hash.as_str())
            },
            Some(item.disk_fingerprint.as_deref()),
            InstallPolicy {
                overwrite_local: overwrite_allowed || rule_activation_only,
            },
            limits,
            |receipt| {
                let current_upstream = current_upstream_fingerprint(
                    app_state,
                    &plan.connection,
                    &app_type,
                    &item.asset,
                    limits,
                )
                .map_err(|error| InstallError::Commit(error.to_string()))?;
                if current_upstream != item.upstream_fingerprint {
                    return Err(InstallError::PreviewStale);
                }
                let target_upstream = target_upstream_fingerprint(
                    &plan.connection,
                    &app_type,
                    &item.asset,
                    &receipt.install_path,
                    &receipt.local_hash,
                )
                .map_err(|error| InstallError::Commit(error.to_string()))?;
                let pending_disk_hash = if item.asset.kind == AssetKind::Skill {
                    strict_skill_path_hash(&item.asset, &receipt.install_path, limits)
                        .map_err(|error| InstallError::Commit(error.to_string()))?
                } else {
                    receipt.local_hash.clone()
                };
                write_upstream_backup(
                    app_state,
                    &plan.connection,
                    &app_type,
                    &item.asset,
                    previous_state.as_ref(),
                    receipt,
                )
                .map_err(InstallError::Commit)?;
                let mut next_state = ManagedAssetState {
                    connection_id: plan.connection.id.clone(),
                    app: app_type.as_str().into(),
                    asset_id: item.asset.asset_id,
                    asset_name: item.asset.name.clone(),
                    asset_kind: Some(item.asset.kind.clone()),
                    revision: item.asset.revision,
                    content_hash: item.asset.content_hash.clone(),
                    archive_sha256: item.asset.archive_sha256.clone(),
                    asset_files: item.asset.files.clone(),
                    local_hash: receipt.local_hash.clone(),
                    install_path: receipt.install_path.to_string_lossy().into_owned(),
                    install_root: root.to_string_lossy().into_owned(),
                    relative_path: relative.to_string_lossy().into_owned(),
                    backup_path: receipt
                        .backup
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    backup_root_path: receipt
                        .backup_dir
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    backup_created_at: receipt.backup_dir.as_ref().map(|_| Utc::now().to_rfc3339()),
                    install_operation: receipt.operation.clone(),
                    upstream_pending: true,
                    upstream_operation: receipt.operation.clone(),
                    upstream_backup_path: None,
                    upstream_fingerprint: previous_state
                        .as_ref()
                        .and_then(|state| state.upstream_fingerprint.clone()),
                    shared_upstream_fingerprint: target_shared_upstream_fingerprint(
                        &plan.connection,
                        &item.asset,
                        &receipt.install_path,
                        &receipt.local_hash,
                    )
                    .map_err(|error| InstallError::Commit(error.to_string()))?,
                    pending_previous_upstream_fingerprint: item.upstream_fingerprint.clone(),
                    pending_target_upstream_fingerprint: target_upstream,
                    restored_unmanaged: false,
                    pending_disk_state: Some(PendingDiskState::Hash(pending_disk_hash)),
                    pending_limits: Some(plan.connection.profile.limits.clone()),
                    subscribed: true,
                    last_synced_at: previous_state
                        .as_ref()
                        .and_then(|state| state.last_synced_at.clone()),
                };
                team.state
                    .save_installed_asset_state(&next_state)
                    .map_err(|error| InstallError::Commit(error.to_string()))?;
                if let Err(error) = integrate_with_upstream(
                    app_state,
                    &plan.connection,
                    &app_type,
                    &item.asset,
                    &receipt.install_path,
                    &receipt.local_hash,
                    limits,
                    receipt.operation.as_deref(),
                    item.upstream_fingerprint.as_deref(),
                ) {
                    upstream_warning = Some(error);
                    return Ok(());
                }
                match current_upstream_fingerprint(
                    app_state,
                    &plan.connection,
                    &app_type,
                    &item.asset,
                    limits,
                ) {
                    Ok(fingerprint)
                        if fingerprint == next_state.pending_target_upstream_fingerprint => {}
                    Ok(_) => {
                        upstream_warning =
                            Some("The tool catalog did not reach the expected Team state".into());
                        return Ok(());
                    }
                    Err(error) => {
                        upstream_warning = Some(error.to_string());
                        return Ok(());
                    }
                }
                next_state.upstream_pending = false;
                next_state.upstream_operation = None;
                next_state.pending_previous_upstream_fingerprint = None;
                next_state.upstream_fingerprint =
                    next_state.pending_target_upstream_fingerprint.take();
                next_state.pending_disk_state = None;
                next_state.pending_limits = None;
                next_state.last_synced_at = Some(Utc::now().to_rfc3339());
                if let Err(error) = team.state.save_installed_asset_state(&next_state) {
                    upstream_warning = Some(format!(
                        "The upstream item was updated, but its completion marker could not be saved: {error}"
                    ));
                }
                Ok(())
            },
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                let (code, message) = install_error(&error);
                let outcome = if matches!(
                    error,
                    InstallError::Conflict(_) | InstallError::PreviewStale
                ) {
                    SyncOutcome::Conflict
                } else {
                    SyncOutcome::Failed
                };
                results.push(result_for(item, outcome, None, Some((code, &message))));
                continue;
            }
        };

        if let Some(message) = upstream_warning {
            results.push(result_for(
                item,
                SyncOutcome::Failed,
                receipt.backup.as_ref(),
                Some(("upstream_pending", &message)),
            ));
            continue;
        }

        let outcome = match item.support {
            SyncSupport::ToolSkill | SyncSupport::ToolRule => SyncOutcome::Installed,
            SyncSupport::InactivePrompt => SyncOutcome::ImportedInactive,
            SyncSupport::ManagedDownload => SyncOutcome::Downloaded,
        };
        results.push(result_for(item, outcome, receipt.backup.as_ref(), None));
    }

    let last_successful_sync = team
        .state
        .asset_states(&plan.connection.id)?
        .into_iter()
        .filter(|state| state.app == app_type.as_str())
        .filter(|state| !state.restored_unmanaged)
        .filter(|state| !state.upstream_pending)
        .filter_map(|state| state.last_synced_at)
        .max();
    let connection = team.status()?.unwrap_or(plan.connection);
    Ok(SyncBatchResult {
        connection,
        conflicts: plan.conflicts,
        items: results,
        last_successful_sync,
    })
}

pub async fn restore_local_backup(
    team: &TeamService,
    app_state: &AppState,
    app: &str,
    asset_id: i64,
) -> Result<LocalRestoreResult, TeamSyncError> {
    let _sync = team.sync_operation.lock().await;
    recover_known_installs(&team.state)?;
    let _ = repair_pending_upstream(team, app_state)?;
    let connection = team.status()?.ok_or(TeamError::NotConnected)?;
    let app_type = supported_app(app)?;
    let state = team
        .state
        .asset_states(&connection.id)?
        .into_iter()
        .find(|state| state.app == app_type.as_str() && state.asset_id == asset_id)
        .ok_or_else(|| TeamSyncError::Local("managed asset state is missing".into()))?;
    let kind = state
        .asset_kind
        .clone()
        .ok_or_else(|| TeamSyncError::Local("managed asset kind is missing".into()))?;
    let (root, relative, _) = target_for_kind(&connection, &app_type, asset_id, &kind)?;
    if !stored_path_matches(&state.install_root, &root)
        || Path::new(&state.relative_path) != relative
        || !stored_path_matches(&state.install_path, &root.join(&relative))
    {
        return Err(TeamSyncError::Local(
            "managed asset paths do not match the current safe target".into(),
        ));
    }
    let selected = state
        .backup_root_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| {
            state
                .backup_path
                .as_deref()
                .and_then(|path| Path::new(path).parent().map(Path::to_path_buf))
        })
        .ok_or_else(|| TeamSyncError::Local("no local backup is available".into()))?;
    let previous = read_upstream_backup(&root, &selected)?;
    let limits = restore_content_limits(&previous, &connection.profile.limits)?;
    let current_state = state.clone();
    let current_item = manifest_item_from_state(&current_state, kind.clone());
    if root.exists() {
        inspect_local_with_limits(&current_item, &root, &relative, ContentLimits::default())
            .map_err(|error| {
                TeamSyncError::Local(format!(
                    "the current local asset cannot be captured in a safe undo backup: {error}"
                ))
            })?;
    }
    if kind == AssetKind::Prompt {
        ensure_managed_prompt_inactive(app_state, &connection, &app_type, asset_id)?;
    }
    let previous_upstream_fingerprint =
        current_upstream_fingerprint(app_state, &connection, &app_type, &current_item, limits)?;
    let mut upstream_warning = None;
    let restored = super::install::restore_backup_with_limits_and_commit(
        &root,
        &relative,
        &selected,
        kind.clone(),
        limits,
        |receipt| {
            if kind == AssetKind::Prompt {
                ensure_managed_prompt_inactive(app_state, &connection, &app_type, asset_id)
                    .map_err(|_| InstallError::PreviewStale)?;
            }
            write_upstream_backup_at(
                app_state,
                &connection,
                &app_type,
                &current_item,
                Some(&current_state),
                &receipt.backup_dir,
                &hard_recovery_asset_limits(&connection.profile.limits),
            )
            .map_err(InstallError::Commit)?;
            // A local restore does not change the server's current release or
            // the last Team-installed baseline. Keeping that baseline makes the
            // restored file an explicit LocalModified state until the member
            // chooses whether to keep it or overwrite it again.
            let mut restored_state = current_state.clone();
            restored_state.restored_unmanaged = previous
                .previous_state
                .as_ref()
                .is_none_or(|state| state.restored_unmanaged);
            restored_state.install_path = receipt.install_path.to_string_lossy().into_owned();
            restored_state.install_root = root.to_string_lossy().into_owned();
            restored_state.relative_path = relative.to_string_lossy().into_owned();
            restored_state.backup_path = receipt
                .displaced_backup
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned());
            restored_state.backup_root_path =
                Some(receipt.backup_dir.to_string_lossy().into_owned());
            restored_state.backup_created_at = Some(Utc::now().to_rfc3339());
            restored_state.install_operation = Some(receipt.operation.clone());
            restored_state.upstream_pending = true;
            restored_state.upstream_operation = Some(receipt.operation.clone());
            restored_state.pending_disk_state = Some(match receipt.local_hash.as_ref() {
                Some(_) if kind == AssetKind::Skill => PendingDiskState::Hash(
                    strict_skill_path_hash(&current_item, &receipt.install_path, limits)
                        .map_err(|error| InstallError::Commit(error.to_string()))?,
                ),
                Some(hash) => PendingDiskState::Hash(hash.clone()),
                None => PendingDiskState::Absent,
            });
            restored_state.pending_limits = previous
                .limits
                .clone()
                .or_else(|| Some(connection.profile.limits.clone()));
            restored_state.pending_previous_upstream_fingerprint =
                previous_upstream_fingerprint.clone();
            restored_state.pending_target_upstream_fingerprint = match kind {
                AssetKind::Prompt => Some(if let Some(prompt) = previous.previous_prompt.as_ref() {
                    let normalized = super::content::normalize_text(prompt.content.as_bytes(), limits)
                        .map_err(|error| InstallError::Commit(error.to_string()))?;
                    prompt_state_fingerprint(
                        &super::content::sha256_hex(&normalized),
                        false,
                    )
                } else {
                    local_state_marker("missing_prompt", None)
                }),
                AssetKind::Skill if previous.previous_state.is_some() => {
                    previous_upstream_fingerprint.clone()
                }
                AssetKind::Skill => Some(
                    skill_target_after_remove(app_state, &connection, &app_type, &current_item)
                        .map_err(|error| InstallError::Commit(error.to_string()))?,
                ),
                AssetKind::Rule if rules::supported(&app_type) => Some(rules::fingerprint(&match previous.previous_rule.clone() {
                    Some(snapshot) => snapshot,
                    None => rules::desired(&app_type,&connection.id,asset_id,None).map_err(InstallError::Commit)?,
                })),
                AssetKind::Rule | AssetKind::Workflow | AssetKind::Agent => None,
            };
            restored_state.upstream_backup_path = Some(
                selected
                    .join(UPSTREAM_BACKUP_FILE)
                    .to_string_lossy()
                    .into_owned(),
            );
            team.state
                .save_asset_state(&restored_state)
                .map_err(|error| InstallError::Commit(error.to_string()))?;
            if let Err(error) = apply_upstream_backup(
                app_state,
                &connection,
                &app_type,
                asset_id,
                &kind,
                &receipt.install_path,
                &previous,
                limits,
                previous_upstream_fingerprint.as_deref(),
            ) {
                upstream_warning = Some(error);
                return Ok(());
            }
            let restored_fingerprint = current_upstream_fingerprint(
                app_state,
                &connection,
                &app_type,
                &manifest_item_from_state(&restored_state, kind.clone()),
                limits,
            )
            .map_err(|error| InstallError::Commit(error.to_string()))?;
            if restored_fingerprint != restored_state.pending_target_upstream_fingerprint {
                upstream_warning = Some(
                    "The tool catalog did not reach the expected restored state".into(),
                );
                return Ok(());
            }
            restored_state.upstream_pending = false;
            restored_state.upstream_operation = None;
            restored_state.upstream_backup_path = None;
            restored_state.pending_previous_upstream_fingerprint = None;
            restored_state.pending_target_upstream_fingerprint = None;
            restored_state.pending_disk_state = None;
            restored_state.pending_limits = None;
            if let Err(error) = team.state.save_asset_state(&restored_state) {
                upstream_warning = Some(format!(
                    "The upstream item was restored, but its completion marker could not be saved: {error}"
                ));
            }
            Ok(())
        },
    )
    .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    Ok(LocalRestoreResult {
        asset_id,
        restored: true,
        has_undo_backup: restored.backup_dir.exists(),
        upstream_pending: upstream_warning.is_some(),
        error_message: upstream_warning,
    })
}

fn write_upstream_backup(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
    previous_state: Option<&ManagedAssetState>,
    receipt: &InstallReceipt,
) -> Result<(), String> {
    let backup_dir = receipt
        .backup_dir
        .as_ref()
        .ok_or_else(|| "the Team install did not provide a recovery directory".to_string())?;
    write_upstream_backup_at(
        state,
        connection,
        app,
        item,
        previous_state,
        backup_dir,
        &connection.profile.limits,
    )
}

fn write_upstream_backup_at(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
    previous_state: Option<&ManagedAssetState>,
    backup_dir: &Path,
    limits: &AssetLimits,
) -> Result<(), String> {
    let previous_prompt = if item.kind == AssetKind::Prompt {
        PromptService::get_prompts(state, app.clone())
            .map_err(|error| error.to_string())?
            .get(&managed_item_id(connection, app, item.asset_id))
            .cloned()
    } else {
        None
    };
    let body = serde_json::to_vec(&UpstreamBackup {
        version: 1,
        limits: Some(limits.clone()),
        previous_state: previous_state.cloned(),
        previous_prompt,
        previous_rule: if item.kind == AssetKind::Rule && rules::supported(app) {
            Some(rules::read(app, &connection.id, item.asset_id)?)
        } else {
            None
        },
    })
    .map_err(|error| error.to_string())?;
    let path = backup_dir.join(UPSTREAM_BACKUP_FILE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    file.write_all(&body).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn read_upstream_backup(root: &Path, backup_dir: &Path) -> Result<UpstreamBackup, TeamSyncError> {
    let backup_root = root.join(".nexusops-team").join("backups");
    if !backup_dir
        .parent()
        .is_some_and(|parent| paths_resolve_equal(parent, &backup_root))
        || backup_dir
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || backup_dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| uuid::Uuid::parse_str(name).ok())
            .is_none()
    {
        return Err(TeamSyncError::Local(
            "the selected Team backup path is outside the managed backup directory".into(),
        ));
    }
    for directory in [
        root.to_path_buf(),
        root.join(".nexusops-team"),
        backup_root,
        backup_dir.to_path_buf(),
    ] {
        let metadata = directory
            .symlink_metadata()
            .map_err(|_| TeamSyncError::Local("the Team backup directory is unavailable".into()))?;
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || reparse {
            return Err(TeamSyncError::Local(
                "the Team backup directory crosses an unsafe link".into(),
            ));
        }
    }
    let path = backup_dir.join(UPSTREAM_BACKUP_FILE);
    let metadata = path
        .symlink_metadata()
        .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16 * 1024 * 1024
    {
        return Err(TeamSyncError::Local(
            "the Team upstream backup is missing or invalid".into(),
        ));
    }
    let value: UpstreamBackup = serde_json::from_slice(
        &fs::read(&path).map_err(|error| TeamSyncError::Local(error.to_string()))?,
    )
    .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    if value.version != 1 {
        return Err(TeamSyncError::Local(
            "the Team upstream backup version is unsupported".into(),
        ));
    }
    Ok(value)
}

fn restore_content_limits(
    backup: &UpstreamBackup,
    current: &AssetLimits,
) -> Result<ContentLimits, TeamSyncError> {
    ContentLimits::try_from(backup.limits.as_ref().unwrap_or(current))
        .map_err(|error| TeamSyncError::Local(error.to_string()))
}

fn hard_recovery_asset_limits(template: &AssetLimits) -> AssetLimits {
    let hard = ContentLimits::default();
    AssetLimits {
        max_assets: template.max_assets,
        max_revisions_per_asset: template.max_revisions_per_asset,
        max_text_bytes: hard.max_text_bytes,
        max_archive_bytes: hard.max_archive_bytes,
        max_unpacked_bytes: hard.max_unpacked_bytes,
        max_files: hard.max_files as u64,
        max_path_depth: hard.max_depth as u64,
        max_path_bytes: hard.max_path_bytes as u64,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_upstream_backup(
    app_state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    asset_id: i64,
    kind: &AssetKind,
    install_path: &Path,
    backup: &UpstreamBackup,
    limits: ContentLimits,
    expected_upstream_fingerprint: Option<&str>,
) -> Result<(), String> {
    if let Some(previous) = backup.previous_state.as_ref() {
        if previous.connection_id != connection.id
            || previous.app != app.as_str()
            || previous.asset_id != asset_id
            || previous.asset_kind.as_ref() != Some(kind)
        {
            return Err("the Team backup identity does not match the selected asset".into());
        }
    }
    match kind {
        AssetKind::Prompt => {
            let id = managed_item_id(connection, app, asset_id);
            let existing = PromptService::get_prompts(app_state, app.clone())
                .map_err(|error| error.to_string())?;
            let current_fingerprint = if let Some(prompt) = existing.get(&id) {
                let normalized = super::content::normalize_text(prompt.content.as_bytes(), limits)
                    .map_err(|error| error.to_string())?;
                prompt_state_fingerprint(&super::content::sha256_hex(&normalized), prompt.enabled)
            } else {
                local_state_marker("missing_prompt", None)
            };
            if expected_upstream_fingerprint != Some(current_fingerprint.as_str()) {
                return Err("The Prompt changed after restore preview; review it again".into());
            }
            if existing.get(&id).is_some_and(|prompt| prompt.enabled) {
                return Err("Disable the active Team prompt before restoring its backup".into());
            }
            if let Some(prompt) = backup.previous_prompt.clone() {
                if prompt.id != id || prompt.enabled {
                    return Err("the Team Prompt backup is invalid".into());
                }
                PromptService::upsert_inactive(app_state, app.clone(), &id, prompt)
                    .map_err(|error| error.to_string())?;
            } else {
                PromptService::delete_prompt(app_state, app.clone(), &id)
                    .map_err(|error| error.to_string())?;
            }
        }
        AssetKind::Skill => {
            let id = managed_skill_id(connection, asset_id);
            let directory = install_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "invalid Team skill path".to_string())?;
            if backup.previous_state.is_none() {
                SkillService::remove_team_managed(&app_state.db, app, &id, directory)
                    .map_err(|error| error.to_string())?;
            }
        }
        AssetKind::Rule if rules::supported(app) => {
            let target = match backup.previous_rule.clone() {
                Some(snapshot) => snapshot,
                None => rules::desired(app, &connection.id, asset_id, None)?,
            };
            rules::apply(
                app,
                &connection.id,
                asset_id,
                &target,
                expected_upstream_fingerprint,
            )?;
        }
        AssetKind::Rule | AssetKind::Workflow | AssetKind::Agent => {}
    }
    Ok(())
}

fn ensure_managed_prompt_inactive(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    asset_id: i64,
) -> Result<(), TeamSyncError> {
    let prompts = PromptService::get_prompts(state, app.clone())
        .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    if prompts
        .get(&managed_item_id(connection, app, asset_id))
        .is_some_and(|prompt| prompt.enabled)
    {
        return Err(TeamSyncError::Local(
            "disable the active Team Prompt before restoring its local backup".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct PendingRepairReport {
    pub repaired: usize,
    pub errors: HashMap<(String, i64), String>,
}

pub fn repair_pending_upstream(
    team: &TeamService,
    app_state: &AppState,
) -> Result<PendingRepairReport, TeamSyncError> {
    let Some(connection) = team.status()? else {
        return Ok(PendingRepairReport::default());
    };
    let limits = ContentLimits::try_from(&connection.profile.limits)
        .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    let mut report = PendingRepairReport::default();
    for managed in team.state.asset_states(&connection.id)? {
        if !managed.upstream_pending {
            continue;
        }
        match repair_one_pending(team, app_state, &connection, managed.clone(), limits) {
            Ok(()) => report.repaired += 1,
            Err(error) => {
                report
                    .errors
                    .insert((managed.app.clone(), managed.asset_id), error.to_string());
            }
        }
    }
    Ok(report)
}

fn repair_one_pending(
    team: &TeamService,
    app_state: &AppState,
    connection: &TeamConnection,
    mut managed: ManagedAssetState,
    current_limits: ContentLimits,
) -> Result<(), TeamSyncError> {
    let limits = if let Some(stored) = managed.pending_limits.as_ref() {
        ContentLimits::try_from(stored).map_err(|error| TeamSyncError::Local(error.to_string()))?
    } else {
        current_limits
    };
    let app = supported_app(&managed.app)?;
    let kind = managed
        .asset_kind
        .clone()
        .ok_or_else(|| TeamSyncError::Local("managed asset kind is missing".into()))?;
    let (root, relative, _) = target_for_kind(connection, &app, managed.asset_id, &kind)?;
    if !stored_path_matches(&managed.install_root, &root)
        || Path::new(&managed.relative_path) != relative
        || !stored_path_matches(&managed.install_path, &root.join(&relative))
    {
        return Err(TeamSyncError::Local(
            "pending Team asset paths do not match the current safe target".into(),
        ));
    }
    let item = manifest_item_from_state(&managed, kind);
    let disk_hash = if item.kind == AssetKind::Skill {
        let path = root.join(&relative);
        if path.exists() {
            Some(strict_skill_path_hash(&item, &path, limits)?)
        } else {
            None
        }
    } else {
        inspect_local_with_limits(&item, &root, &relative, limits)
            .map_err(|error| TeamSyncError::Local(error.to_string()))?
    };
    let expected_disk_hash = match managed.pending_disk_state.as_ref() {
        Some(PendingDiskState::Absent) => None,
        Some(PendingDiskState::Hash(hash)) => Some(hash.as_str()),
        None => Some(managed.local_hash.as_str()),
    };
    if disk_hash.as_deref() != expected_disk_hash {
        return Err(TeamSyncError::Local(
            "pending Team asset changed locally before upstream recovery".into(),
        ));
    }
    let mut current_fingerprint =
        current_upstream_fingerprint(app_state, connection, &app, &item, limits)?;
    let expected_target_fingerprint = managed.pending_target_upstream_fingerprint.clone();
    let recovered_skill_operation = item.kind == AssetKind::Skill
        && managed.upstream_backup_path.is_none()
        && managed.upstream_operation.is_some();
    if recovered_skill_operation {
        integrate_with_upstream(
            app_state,
            connection,
            &app,
            &item,
            &root.join(&relative),
            &managed.local_hash,
            limits,
            managed.upstream_operation.as_deref(),
            managed.pending_previous_upstream_fingerprint.as_deref(),
        )
        .map_err(TeamSyncError::Local)?;
        current_fingerprint =
            current_upstream_fingerprint(app_state, connection, &app, &item, limits)?;
    }
    let already_applied =
        expected_target_fingerprint.is_some() && current_fingerprint == expected_target_fingerprint;
    if !recovered_skill_operation
        && !already_applied
        && current_fingerprint != managed.pending_previous_upstream_fingerprint
    {
        return Err(TeamSyncError::Local(
                "the Prompt or Skill catalog changed while Team recovery was pending; review the local change before retrying"
                    .into(),
            ));
    }
    if !recovered_skill_operation && !already_applied {
        if let Some(path) = managed.upstream_backup_path.as_deref() {
            let path = PathBuf::from(path);
            if path.file_name().and_then(|name| name.to_str()) != Some(UPSTREAM_BACKUP_FILE) {
                return Err(TeamSyncError::Local(
                    "the pending Team upstream backup path is invalid".into(),
                ));
            }
            let backup_dir = path
                .parent()
                .ok_or_else(|| TeamSyncError::Local("invalid Team backup path".into()))?;
            let backup = read_upstream_backup(&root, backup_dir)?;
            apply_upstream_backup(
                app_state,
                connection,
                &app,
                managed.asset_id,
                &item.kind,
                &root.join(&relative),
                &backup,
                limits,
                current_fingerprint.as_deref(),
            )
            .map_err(TeamSyncError::Local)?;
        } else {
            integrate_with_upstream(
                app_state,
                connection,
                &app,
                &item,
                &root.join(&relative),
                &managed.local_hash,
                limits,
                managed.upstream_operation.as_deref(),
                current_fingerprint.as_deref(),
            )
            .map_err(TeamSyncError::Local)?;
        }
    }
    let repaired_fingerprint =
        current_upstream_fingerprint(app_state, connection, &app, &item, limits)?;
    if expected_target_fingerprint.is_some() && repaired_fingerprint != expected_target_fingerprint
    {
        return Err(TeamSyncError::Local(
            "the Prompt or Skill catalog did not reach the expected recovery state".into(),
        ));
    }
    let is_local_restore = managed.upstream_backup_path.is_some();
    if !is_local_restore {
        managed.upstream_fingerprint = repaired_fingerprint;
        managed.shared_upstream_fingerprint = if item.kind == AssetKind::Skill {
            Some(current_shared_skill_fingerprint(app_state, connection, &app, &item)?.0)
        } else {
            None
        };
        managed.last_synced_at = Some(Utc::now().to_rfc3339());
    }
    managed.upstream_pending = false;
    managed.upstream_operation = None;
    managed.upstream_backup_path = None;
    managed.pending_previous_upstream_fingerprint = None;
    managed.pending_target_upstream_fingerprint = None;
    managed.pending_disk_state = None;
    managed.pending_limits = None;
    if is_local_restore {
        team.state.save_asset_state(&managed)?;
    } else {
        team.state.save_installed_asset_state(&managed)?;
    }
    Ok(())
}

fn manifest_item_from_state(state: &ManagedAssetState, kind: AssetKind) -> ManifestItem {
    ManifestItem {
        asset_id: state.asset_id,
        kind,
        slug: format!("asset-{}", state.asset_id),
        name: if state.asset_name.is_empty() {
            format!("Asset {}", state.asset_id)
        } else {
            state.asset_name.clone()
        },
        revision: state.revision,
        content_hash: state.content_hash.clone(),
        archive_sha256: state.archive_sha256.clone(),
        download_url: String::new(),
        content_type: String::new(),
        byte_size: 0,
        files: state.asset_files.clone(),
    }
}

fn stored_path_matches(stored: &str, expected: &Path) -> bool {
    let stored = Path::new(stored);
    if stored == expected {
        return true;
    }
    matches!(
        (stored.canonicalize(), expected.canonicalize()),
        (Ok(stored), Ok(expected)) if stored == expected
    )
}

fn paths_resolve_equal(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    if matches!((left.canonicalize(), right.canonicalize()), (Ok(left), Ok(right)) if left == right)
    {
        return true;
    }
    let resolve_entry =
        |path: &Path| Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?));
    matches!((resolve_entry(left), resolve_entry(right)), (Some(left), Some(right)) if left == right)
}

fn plan_item_is_current(
    app_state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &SyncPlanItem,
    prior: Option<&ManagedAssetState>,
    all_states: &[ManagedAssetState],
    limits: ContentLimits,
) -> Result<bool, TeamSyncError> {
    let (root, relative, _) = target_for(connection, app, &item.asset)?;
    let disk_hash = if root.exists() {
        inspect_local_with_limits(&item.asset, &root, &relative, limits)
            .map_err(|error| TeamSyncError::Local(error.to_string()))?
    } else {
        None
    };
    let (local_hash, upstream_fingerprint) = effective_local_hash(
        app_state,
        connection,
        app,
        &item.asset,
        prior,
        all_states,
        disk_hash.clone(),
        limits,
    )?;
    let drift = classify_drift(
        prior.map(|state| state.local_hash.as_str()),
        local_hash.as_deref(),
        &item.asset.content_hash,
    );
    Ok(disk_hash == item.disk_fingerprint
        && upstream_fingerprint == item.upstream_fingerprint
        && sync_decision_token(
            &connection.id,
            app.as_str(),
            &item.asset,
            drift,
            local_hash.as_deref(),
        ) == item.decision_token)
}

fn build_plan(
    team: &TeamService,
    app_state: &AppState,
    connection: TeamConnection,
    manifest: Manifest,
    app_type: &AppType,
    pending_errors: &HashMap<(String, i64), String>,
) -> Result<SyncPlan, TeamSyncError> {
    let limits = ContentLimits::try_from(&connection.profile.limits)
        .map_err(|error| TeamSyncError::Local(error.to_string()))?;
    let states = team.state.asset_states(&connection.id)?;
    let authorized = manifest
        .assets
        .iter()
        .map(|asset| asset.asset_id)
        .collect::<HashSet<_>>();
    let withdrawn = states
        .iter()
        .filter(|state| state.app == app_type.as_str() && !authorized.contains(&state.asset_id))
        .map(|state| LocalAssetHistory {
            asset_id: state.asset_id,
            name: if state.asset_name.is_empty() {
                format!("Asset {}", state.asset_id)
            } else {
                state.asset_name.clone()
            },
            kind: state.asset_kind.clone(),
            revision: state.revision,
            last_synced_at: state.last_synced_at.clone(),
            has_backup: state.backup_root_path.is_some() || state.backup_path.is_some(),
            local_file_present: std::path::Path::new(&state.install_path).exists(),
            recovery_error: pending_errors
                .get(&(state.app.clone(), state.asset_id))
                .cloned(),
        })
        .collect();
    let last_successful_sync = states
        .iter()
        .filter(|state| state.app == app_type.as_str())
        .filter(|state| !state.restored_unmanaged)
        .filter(|state| !state.upstream_pending)
        .filter_map(|state| state.last_synced_at.clone())
        .max();
    let mut items = Vec::with_capacity(manifest.assets.len());
    for asset in manifest.assets {
        let history = states
            .iter()
            .find(|state| state.app == app_type.as_str() && state.asset_id == asset.asset_id);
        let prior = history.filter(|state| !state.restored_unmanaged);
        if let Some(message) = pending_errors.get(&(app_type.as_str().into(), asset.asset_id)) {
            items.push(failed_plan_item(
                &connection,
                app_type,
                asset,
                prior,
                history,
                SyncSupport::ManagedDownload,
                String::new(),
                message.clone(),
            ));
            continue;
        }
        let (root, relative, support) = match target_for(&connection, app_type, &asset) {
            Ok(target) => target,
            Err(error) => {
                items.push(failed_plan_item(
                    &connection,
                    app_type,
                    asset,
                    prior,
                    history,
                    SyncSupport::ManagedDownload,
                    String::new(),
                    error.to_string(),
                ));
                continue;
            }
        };
        let disk_hash = if root.exists() {
            match inspect_local_with_limits(&asset, &root, &relative, limits) {
                Ok(hash) => hash,
                Err(error) => {
                    items.push(failed_plan_item(
                        &connection,
                        app_type,
                        asset,
                        prior,
                        history,
                        support,
                        root.join(&relative).to_string_lossy().into_owned(),
                        error.to_string(),
                    ));
                    continue;
                }
            }
        } else {
            None
        };
        let (local_hash, upstream_fingerprint) = match effective_local_hash(
            app_state,
            &connection,
            app_type,
            &asset,
            prior,
            &states,
            disk_hash.clone(),
            limits,
        ) {
            Ok(value) => value,
            Err(error) => {
                items.push(failed_plan_item(
                    &connection,
                    app_type,
                    asset,
                    prior,
                    history,
                    support,
                    root.join(&relative).to_string_lossy().into_owned(),
                    error.to_string(),
                ));
                continue;
            }
        };
        let drift = classify_drift(
            prior.map(|state| state.local_hash.as_str()),
            local_hash.as_deref(),
            &asset.content_hash,
        );
        items.push(SyncPlanItem {
            install_path: root.join(&relative).to_string_lossy().into_owned(),
            activation_path: if asset.kind == AssetKind::Rule && rules::supported(app_type) {
                Some(
                    rules::activation_path(app_type, &connection.id, asset.asset_id)
                        .map_err(TeamSyncError::Local)?,
                )
            } else {
                None
            },
            previous_revision: prior.map(|state| state.revision),
            subscribed: prior.is_some_and(|state| state.subscribed),
            has_backup: history.is_some_and(|state| {
                state.backup_root_path.is_some() || state.backup_path.is_some()
            }),
            last_synced_at: prior.and_then(|state| state.last_synced_at.clone()),
            decision_token: sync_decision_token(
                &connection.id,
                app_type.as_str(),
                &asset,
                drift,
                local_hash.as_deref(),
            ),
            disk_fingerprint: disk_hash,
            upstream_fingerprint,
            inspection_error_code: None,
            inspection_error_message: None,
            asset,
            drift,
            support,
        });
    }
    Ok(SyncPlan {
        connection,
        conflicts: manifest.conflicts,
        items,
        withdrawn,
        last_successful_sync,
    })
}

#[allow(clippy::too_many_arguments)]
fn failed_plan_item(
    connection: &TeamConnection,
    app: &AppType,
    asset: ManifestItem,
    prior: Option<&ManagedAssetState>,
    history: Option<&ManagedAssetState>,
    support: SyncSupport,
    install_path: String,
    message: String,
) -> SyncPlanItem {
    let marker = local_state_marker(&format!("inspection_error:{message}"), None);
    SyncPlanItem {
        activation_path: None,
        decision_token: sync_decision_token(
            &connection.id,
            app.as_str(),
            &asset,
            DriftStatus::LocalModified,
            Some(&marker),
        ),
        asset,
        drift: DriftStatus::LocalModified,
        support,
        install_path,
        previous_revision: prior.map(|state| state.revision),
        subscribed: prior.is_some_and(|state| state.subscribed),
        has_backup: history
            .is_some_and(|state| state.backup_root_path.is_some() || state.backup_path.is_some()),
        last_synced_at: prior.and_then(|state| state.last_synced_at.clone()),
        disk_fingerprint: None,
        upstream_fingerprint: None,
        inspection_error_code: Some("local_state".into()),
        inspection_error_message: Some(message),
    }
}

#[allow(clippy::too_many_arguments)]
fn effective_local_hash(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
    prior: Option<&ManagedAssetState>,
    all_states: &[ManagedAssetState],
    disk_hash: Option<String>,
    limits: ContentLimits,
) -> Result<(Option<String>, Option<String>), TeamSyncError> {
    if item.kind == AssetKind::Rule && rules::supported(app) {
        let current =
            rules::read(app, &connection.id, item.asset_id).map_err(TeamSyncError::Local)?;
        let fingerprint = rules::fingerprint(&current);
        let recorded = prior.and_then(|state| state.upstream_fingerprint.as_deref());
        if recorded == Some(fingerprint.as_str()) && current.body.is_some() {
            return Ok((disk_hash, Some(fingerprint)));
        }
        if (recorded.is_none() || recorded == Some(fingerprint.as_str()))
            && current.body.is_none()
            && (disk_hash.is_none()
                || disk_hash.as_deref()
                    == Some(
                        prior
                            .map(|state| state.local_hash.as_str())
                            .unwrap_or(&item.content_hash),
                    ))
        {
            return Ok((None, Some(fingerprint)));
        }
        return Ok((
            Some(local_state_marker(&fingerprint, disk_hash.as_deref())),
            Some(fingerprint),
        ));
    }
    if !matches!(item.kind, AssetKind::Prompt | AssetKind::Skill) {
        return Ok((disk_hash, None));
    }
    let upstream_fingerprint = current_upstream_fingerprint(state, connection, app, item, limits)?
        .ok_or_else(|| TeamSyncError::Local("missing upstream fingerprint".into()))?;
    let missing = if item.kind == AssetKind::Prompt {
        local_state_marker("missing_prompt", None)
    } else {
        missing_skill_upstream_fingerprint()
    };
    let (current_shared_skill, current_skill_enabled) = if item.kind == AssetKind::Skill {
        let (fingerprint, enabled, _) =
            current_shared_skill_fingerprint(state, connection, app, item)?;
        (Some(fingerprint), enabled)
    } else {
        (None, false)
    };
    let known_shared_skill = current_shared_skill.as_ref().is_some_and(|fingerprint| {
        all_states.iter().any(|state| {
            state.connection_id == connection.id
                && state.asset_id == item.asset_id
                && state.shared_upstream_fingerprint.as_ref() == Some(fingerprint)
        })
    });
    let has_recorded_shared_skill = item.kind == AssetKind::Skill
        && all_states.iter().any(|state| {
            state.connection_id == connection.id
                && state.asset_id == item.asset_id
                && state.shared_upstream_fingerprint.is_some()
        });
    if item.kind == AssetKind::Skill
        && current_shared_skill.as_deref()
            != Some(local_state_marker("missing_skill", None).as_str())
        && !known_shared_skill
        && (has_recorded_shared_skill || prior.is_none())
    {
        return Err(TeamSyncError::Local(
            "the shared Skill catalog contains unrecorded local changes; preserve or resolve them before Team sync"
                .into(),
        ));
    }
    let current_app_skill_disabled =
        item.kind == AssetKind::Skill && prior.is_some() && !current_skill_enabled;
    let clean = prior
        .and_then(|state| state.upstream_fingerprint.clone())
        .or_else(|| {
            prior.and_then(|state| {
                (item.kind == AssetKind::Prompt)
                    .then(|| prompt_state_fingerprint(&state.local_hash, false))
            })
        })
        .unwrap_or_else(|| {
            if known_shared_skill || prior.is_some() {
                // A legacy Skill state has no upstream baseline. Adopt the
                // current value once; subsequent previews compare it strictly.
                upstream_fingerprint.clone()
            } else {
                missing
            }
        });
    if current_app_skill_disabled || (upstream_fingerprint != clean && !known_shared_skill) {
        return Ok((
            Some(local_state_marker(
                &upstream_fingerprint,
                disk_hash.as_deref(),
            )),
            Some(upstream_fingerprint),
        ));
    }
    Ok((disk_hash, Some(upstream_fingerprint)))
}

fn current_upstream_fingerprint(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
    limits: ContentLimits,
) -> Result<Option<String>, TeamSyncError> {
    match item.kind {
        AssetKind::Prompt => {
            let prompts = PromptService::get_prompts(state, app.clone())
                .map_err(|error| TeamSyncError::Local(error.to_string()))?;
            let id = managed_item_id(connection, app, item.asset_id);
            let Some(prompt) = prompts.get(&id) else {
                return Ok(Some(local_state_marker("missing_prompt", None)));
            };
            let normalized = super::content::normalize_text(prompt.content.as_bytes(), limits)
                .map_err(|error| TeamSyncError::Local(error.to_string()))?;
            Ok(Some(prompt_state_fingerprint(
                &super::content::sha256_hex(&normalized),
                prompt.enabled,
            )))
        }
        AssetKind::Skill => {
            let (shared, enabled, _) =
                current_shared_skill_fingerprint(state, connection, app, item)?;
            Ok(Some(local_state_marker(
                &format!("skill:{shared}:enabled={enabled}"),
                None,
            )))
        }
        AssetKind::Rule if rules::supported(app) => Ok(Some(rules::fingerprint(
            &rules::read(app, &connection.id, item.asset_id).map_err(TeamSyncError::Local)?,
        ))),
        AssetKind::Rule | AssetKind::Workflow | AssetKind::Agent => Ok(None),
    }
}

fn current_shared_skill_fingerprint(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
) -> Result<(String, bool, Option<String>), TeamSyncError> {
    let id = managed_skill_id(connection, item.asset_id);
    let (_, relative, _) = target_for(connection, app, item)?;
    let directory = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(TeamSyncError::Path)?;
    let Some(skill) = state
        .db
        .get_installed_skill(&id)
        .map_err(|error| TeamSyncError::Local(error.to_string()))?
    else {
        return Ok((local_state_marker("missing_skill", None), false, None));
    };
    if skill.id != id || skill.directory != directory {
        return Ok((
            local_state_marker("invalid_skill_identity", None),
            false,
            None,
        ));
    }
    let ssot = SkillService::get_ssot_dir()
        .map_err(|error| TeamSyncError::Local(error.to_string()))?
        .join(directory);
    let actual_hash = if ssot.exists() {
        Some(strict_skill_path_hash(
            item,
            &ssot,
            ContentLimits::default(),
        )?)
    } else {
        None
    };
    Ok((
        local_state_marker(
            &format!(
                "skill:{id}:{directory}:db={}:disk={}",
                skill.content_hash.as_deref().unwrap_or("missing"),
                actual_hash.as_deref().unwrap_or("missing")
            ),
            None,
        ),
        skill.apps.is_enabled_for(app),
        actual_hash,
    ))
}

fn missing_skill_upstream_fingerprint() -> String {
    let shared = local_state_marker("missing_skill", None);
    local_state_marker(&format!("skill:{shared}:enabled=false"), None)
}

fn skill_target_after_remove(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
) -> Result<String, TeamSyncError> {
    let id = managed_skill_id(connection, item.asset_id);
    let Some(_skill) = state
        .db
        .get_installed_skill(&id)
        .map_err(|error| TeamSyncError::Local(error.to_string()))?
    else {
        return Ok(missing_skill_upstream_fingerprint());
    };
    let shared = current_shared_skill_fingerprint(state, connection, app, item)?.0;
    Ok(local_state_marker(
        &format!("skill:{shared}:enabled=false"),
        None,
    ))
}

fn target_upstream_fingerprint(
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
    install_path: &Path,
    local_hash: &str,
) -> Result<Option<String>, TeamSyncError> {
    match item.kind {
        AssetKind::Prompt => Ok(Some(prompt_state_fingerprint(local_hash, false))),
        AssetKind::Skill => {
            let shared =
                target_shared_upstream_fingerprint(connection, item, install_path, local_hash)?
                    .ok_or_else(|| {
                        TeamSyncError::Local("missing Skill target fingerprint".into())
                    })?;
            Ok(Some(local_state_marker(
                &format!("skill:{shared}:enabled=true"),
                None,
            )))
        }
        AssetKind::Rule if rules::supported(app) => Ok(Some(rules::fingerprint(
            &rules::desired(
                app,
                &connection.id,
                item.asset_id,
                Some(rules::payload(install_path).map_err(TeamSyncError::Local)?),
            )
            .map_err(TeamSyncError::Local)?,
        ))),
        AssetKind::Rule | AssetKind::Workflow | AssetKind::Agent => Ok(None),
    }
}

fn target_shared_upstream_fingerprint(
    connection: &TeamConnection,
    item: &ManifestItem,
    install_path: &Path,
    local_hash: &str,
) -> Result<Option<String>, TeamSyncError> {
    if item.kind != AssetKind::Skill {
        return Ok(None);
    }
    let id = managed_skill_id(connection, item.asset_id);
    let directory = install_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(TeamSyncError::Path)?;
    let actual_hash = strict_skill_path_hash(item, install_path, ContentLimits::default())?;
    Ok(Some(local_state_marker(
        &format!("skill:{id}:{directory}:db={local_hash}:disk={actual_hash}"),
        None,
    )))
}

fn strict_skill_path_hash(
    item: &ManifestItem,
    path: &Path,
    limits: ContentLimits,
) -> Result<String, TeamSyncError> {
    let root = path.parent().ok_or(TeamSyncError::Path)?;
    let relative = Path::new(path.file_name().ok_or(TeamSyncError::Path)?);
    inspect_skill_physical_with_limits(item, root, relative, limits)
        .map_err(|error| TeamSyncError::Local(error.to_string()))?
        .ok_or_else(|| TeamSyncError::Local("the Team Skill directory is missing".into()))
}

fn local_state_marker(reason: &str, disk_hash: Option<&str>) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!(
            "nexusops-team-local-v1|{reason}|{}",
            disk_hash.unwrap_or("missing")
        ))
    )
}

fn prompt_state_fingerprint(content_hash: &str, enabled: bool) -> String {
    local_state_marker(&format!("prompt:{content_hash}:enabled={enabled}"), None)
}

fn sync_decision_token(
    connection_id: &str,
    app: &str,
    item: &ManifestItem,
    drift: DriftStatus,
    local_hash: Option<&str>,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "nexusops-team-sync-decision-v1|{connection_id}|{app}|{}|{}|{}|{:?}|{}",
                item.asset_id,
                item.revision,
                item.content_hash,
                drift,
                local_hash.unwrap_or("missing")
            )
            .as_bytes()
        )
    )
}

fn reconcile_removed(
    team: &TeamService,
    connection: &TeamConnection,
    manifest: &Manifest,
    app: &str,
) -> Result<(), TeamSyncError> {
    let authorized = manifest
        .assets
        .iter()
        .map(|item| item.asset_id)
        .collect::<HashSet<_>>();
    for state in team.state.asset_states(&connection.id)? {
        if state.app == app
            && !state.restored_unmanaged
            && state.subscribed
            && !authorized.contains(&state.asset_id)
        {
            let mut withdrawn = state;
            withdrawn.subscribed = false;
            team.state.save_asset_state(&withdrawn)?;
        }
    }
    Ok(())
}

fn target_for(
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
) -> Result<(PathBuf, PathBuf, SyncSupport), TeamSyncError> {
    target_for_kind(connection, app, item.asset_id, &item.kind)
}

fn target_for_kind(
    connection: &TeamConnection,
    app: &AppType,
    asset_id: i64,
    kind: &AssetKind,
) -> Result<(PathBuf, PathBuf, SyncSupport), TeamSyncError> {
    let connection_prefix = connection.id.chars().take(12).collect::<String>();
    if *kind == AssetKind::Skill {
        let root = SkillService::get_app_skills_dir(app)
            .map_err(|error| TeamSyncError::Local(error.to_string()))?;
        let ssot = SkillService::get_ssot_dir()
            .map_err(|error| TeamSyncError::Local(error.to_string()))?;
        if paths_resolve_equal(&root, &ssot) {
            return Err(TeamSyncError::Local(
                "the Team Skill target must be distinct from the shared Skill catalog directory"
                    .into(),
            ));
        }
        return Ok((
            root,
            PathBuf::from(format!("nexusops-{connection_prefix}-asset-{asset_id}")),
            SyncSupport::ToolSkill,
        ));
    }
    let root = crate::config::get_app_config_dir()
        .join("team")
        .join("managed")
        .join(&connection_prefix)
        .join(app.as_str());
    let directory = match kind {
        AssetKind::Prompt => "prompts",
        AssetKind::Rule => "rules",
        AssetKind::Workflow => "workflows",
        AssetKind::Agent => "agents",
        AssetKind::Skill => unreachable!(),
    };
    let support = if *kind == AssetKind::Prompt {
        SyncSupport::InactivePrompt
    } else if *kind == AssetKind::Rule && rules::supported(app) {
        SyncSupport::ToolRule
    } else {
        SyncSupport::ManagedDownload
    };
    Ok((
        root,
        PathBuf::from(directory).join(format!("asset-{asset_id}.md")),
        support,
    ))
}

#[allow(clippy::too_many_arguments)]
fn integrate_with_upstream(
    state: &AppState,
    connection: &TeamConnection,
    app: &AppType,
    item: &ManifestItem,
    install_path: &std::path::Path,
    local_hash: &str,
    limits: ContentLimits,
    operation: Option<&str>,
    expected_upstream_fingerprint: Option<&str>,
) -> Result<(), String> {
    let id = if item.kind == AssetKind::Skill {
        managed_skill_id(connection, item.asset_id)
    } else {
        managed_item_id(connection, app, item.asset_id)
    };
    match item.kind {
        AssetKind::Skill => {
            let directory = install_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "invalid Team skill path".to_string())?;
            SkillService::register_team_managed(
                &state.db,
                app,
                &id,
                directory,
                &item.name,
                local_hash,
                operation.ok_or_else(|| "missing Team Skill operation id".to_string())?,
                || {
                    let current = current_upstream_fingerprint(
                        state,
                        connection,
                        app,
                        item,
                        limits,
                    )
                    .map_err(|error| error.to_string())?;
                    let (_, _, actual_ssot_hash) =
                        current_shared_skill_fingerprint(state, connection, app, item)
                            .map_err(|error| error.to_string())?;
                    let ssot_path = SkillService::get_ssot_dir()
                        .map_err(|error| error.to_string())?
                        .join(directory);
                    let same_path = matches!(
                        (ssot_path.canonicalize(), install_path.canonicalize()),
                        (Ok(ssot), Ok(installed)) if ssot == installed
                    );
                    if same_path {
                        return Err(
                            "The Team Skill target must be distinct from the shared Skill catalog directory"
                                .into(),
                        );
                    }
                    if current.as_deref() != expected_upstream_fingerprint {
                        return Err(
                            "The Skill catalog changed after sync preview; review it again"
                                .into(),
                        );
                    }
                    Ok(actual_ssot_hash)
                },
                |path| {
                    strict_skill_path_hash(item, path, limits).map_err(|error| error.to_string())
                },
            )
            .map_err(|error| error.to_string())?;
        }
        AssetKind::Prompt => {
            let existing = PromptService::get_prompts(state, app.clone())
                .map_err(|error| error.to_string())?;
            let current_fingerprint = if let Some(prompt) = existing.get(&id) {
                let normalized = super::content::normalize_text(prompt.content.as_bytes(), limits)
                    .map_err(|error| error.to_string())?;
                prompt_state_fingerprint(&super::content::sha256_hex(&normalized), prompt.enabled)
            } else {
                local_state_marker("missing_prompt", None)
            };
            if expected_upstream_fingerprint != Some(current_fingerprint.as_str()) {
                return Err("The Prompt changed after sync preview; review it again".into());
            }
            if existing.get(&id).is_some_and(|prompt| prompt.enabled) {
                return Err(
                    "Disable the active Team prompt in the Prompt page before updating it".into(),
                );
            }
            let content = fs::read_to_string(install_path).map_err(|error| error.to_string())?;
            let now = Utc::now().timestamp();
            let created_at = existing
                .get(&id)
                .and_then(|prompt| prompt.created_at)
                .or(Some(now));
            PromptService::upsert_inactive(
                state,
                app.clone(),
                &id,
                Prompt {
                    id: id.clone(),
                    name: item.name.clone(),
                    content,
                    description: Some(format!(
                        "NexusOps Team asset {} revision {}",
                        item.asset_id, item.revision
                    )),
                    enabled: false,
                    created_at,
                    updated_at: Some(now),
                },
            )
            .map_err(|error| error.to_string())?;
        }
        AssetKind::Rule if rules::supported(app) => {
            let target = rules::desired(
                app,
                &connection.id,
                item.asset_id,
                Some(rules::payload(install_path)?),
            )?;
            rules::apply(
                app,
                &connection.id,
                item.asset_id,
                &target,
                expected_upstream_fingerprint,
            )?;
        }
        AssetKind::Rule | AssetKind::Workflow | AssetKind::Agent => {}
    }
    Ok(())
}

fn managed_item_id(connection: &TeamConnection, app: &AppType, asset_id: i64) -> String {
    format!(
        "nexusops-team:{}:{asset_id}:{}",
        connection.id,
        app.as_str()
    )
}

fn managed_skill_id(connection: &TeamConnection, asset_id: i64) -> String {
    format!("nexusops-team:{}:{asset_id}:skill", connection.id)
}

fn supported_app(value: &str) -> Result<AppType, TeamSyncError> {
    let app = AppType::from_str(value).map_err(|_| TeamSyncError::UnsupportedApp(value.into()))?;
    if matches!(
        app,
        AppType::Claude | AppType::Codex | AppType::Gemini | AppType::GrokBuild | AppType::OpenCode
    ) {
        Ok(app)
    } else {
        Err(TeamSyncError::UnsupportedApp(value.into()))
    }
}

fn result_for(
    item: &SyncPlanItem,
    outcome: SyncOutcome,
    backup: Option<&PathBuf>,
    error: Option<(&str, &str)>,
) -> SyncItemResult {
    SyncItemResult {
        asset_id: item.asset.asset_id,
        revision: item.asset.revision,
        outcome,
        drift: item.drift,
        install_path: item.install_path.clone(),
        backup_path: backup.map(|path| path.to_string_lossy().into_owned()),
        error_code: error.map(|(code, _)| code.into()),
        error_message: error.map(|(_, message)| message.into()),
    }
}

fn install_error(error: &InstallError) -> (&'static str, String) {
    let code = match error {
        InstallError::Conflict(_) => "local_modified",
        InstallError::PreviewStale => "preview_stale",
        InstallError::Commit(_) => "commit_failed",
        InstallError::UnsafePath | InstallError::UnsafeFilesystem => "unsafe_path",
        InstallError::RecoveryConflict(_) => "recovery_conflict",
        InstallError::Recovery => "recovery_failed",
        InstallError::Content(_) => "content_invalid",
        InstallError::InvalidRoot => "install_path",
        InstallError::Io(_) | InstallError::InvalidJournal => "install_failed",
    };
    (code, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serial_test::serial;

    use super::*;
    use crate::{
        database::Database,
        services::team::{
            credentials::fixtures::MemoryCredentialStore,
            types::{fixture_profile, ConnectionStatus},
        },
    };

    fn connection() -> TeamConnection {
        TeamConnection {
            id: "0123456789abcdef".into(),
            gateway_url: "https://gateway.example/".into(),
            profile: fixture_profile(),
            status: ConnectionStatus::Connected,
            last_checked_at: "2026-09-07T00:00:00Z".into(),
            last_error: None,
        }
    }

    fn text_item(asset_id: i64, kind: AssetKind, body: &[u8]) -> ManifestItem {
        let normalized = super::super::content::normalize_text(body, ContentLimits::default())
            .expect("normalize fixture");
        ManifestItem {
            asset_id,
            kind,
            slug: "../../remote-name-is-not-a-path".into(),
            name: format!("Asset {asset_id}"),
            revision: 1,
            content_hash: super::super::content::sha256_hex(&normalized),
            archive_sha256: None,
            download_url: format!("/api/v1/assets/{asset_id}/revisions/1/download"),
            content_type: "text/plain; charset=utf-8".into(),
            byte_size: body.len() as u64,
            files: Vec::new(),
        }
    }

    #[test]
    #[serial]
    fn remote_slug_never_controls_the_local_path() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        let item = text_item(17, AssetKind::Rule, b"rule");
        let (root, relative, support) = target_for(&connection(), &AppType::Codex, &item).unwrap();
        assert!(root.starts_with(temp.path()));
        assert_eq!(relative, PathBuf::from("rules/asset-17.md"));
        assert_eq!(support, SyncSupport::ToolRule);
        assert!(!root
            .join(relative)
            .to_string_lossy()
            .contains("remote-name"));
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[tokio::test]
    #[serial]
    async fn rules_upgrade_legacy_payloads_activate_and_restore_native_instructions() {
        use axum::{http::HeaderMap, routing::get, Json, Router};
        use serde_json::json;
        struct RestoreHome(Option<std::ffi::OsString>);
        impl Drop for RestoreHome {
            fn drop(&mut self) {
                if let Some(previous) = self.0.as_ref() {
                    std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", previous);
                } else {
                    std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
                }
                let _ = crate::settings::reload_settings();
            }
        }
        let _restore = RestoreHome(std::env::var_os("NEXUSOPS_CLIENT_TEST_HOME"));
        for app in ["codex", "claude"] {
            let temp = tempfile::tempdir().unwrap();
            std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
            crate::settings::reload_settings().unwrap();
            let body = b"# Team standard\nUse checked arithmetic.\n";
            let mut item = text_item(17, AssetKind::Rule, body);
            item.slug = "team-rule".into();
            let served = item.clone();
            let router=Router::new()
                .route("/api/v1/me/team-profile",get(|headers:HeaderMap|async move{let mut profile=fixture_profile();profile.gateway_url=format!("http://{}/",headers.get("host").unwrap().to_str().unwrap());profile.base_url=format!("{}v1",profile.gateway_url);Json(json!({"code":0,"data":profile}))}))
                .route("/api/v1/me/assets/manifest",get(move ||{let served=served.clone();async move{Json(json!({"code":0,"data":{"schema_version":1,"assets":[served],"conflicts":[]}}))}}))
                .route("/api/v1/assets/17/revisions/1/download",get(move ||async move{body.to_vec()}));
            let (gateway, server) = super::super::api::tests::serve(router).await;
            let team = TeamService::with_credentials(
                &temp.path().join("state"),
                Arc::new(MemoryCredentialStore::default()),
            )
            .unwrap();
            let app_state = AppState::new(Arc::new(Database::memory().unwrap()));
            let cancel = Cancellation::default();
            let connection = team
                .connect(&gateway, "nx_rule_fixture", &cancel)
                .await
                .unwrap();
            let app_type = supported_app(app).unwrap();
            let (root, relative, _) = target_for(&connection, &app_type, &item).unwrap();
            fs::create_dir_all(&root).unwrap();
            let receipt = super::super::install::install_verified(
                &item,
                body,
                &root,
                &relative,
                None,
                InstallPolicy::default(),
            )
            .unwrap();
            let metadata:ManagedAssetState=serde_json::from_value(json!({"connection_id":connection.id,"app":app,"asset_id":17,"asset_kind":"rule","revision":1,"content_hash":item.content_hash,"local_hash":receipt.local_hash,"install_root":root,"relative_path":relative,"install_path":receipt.install_path,"subscribed":true,"last_synced_at":Utc::now().to_rfc3339()})).unwrap();
            team.state.save_asset_state(&metadata).unwrap();
            let personal = if app == "codex" {
                crate::codex_config::get_codex_config_dir().join("AGENTS.md")
            } else {
                crate::config::get_claude_config_dir().join("rules/personal.md")
            };
            fs::create_dir_all(personal.parent().unwrap()).unwrap();
            fs::write(&personal, "Personal guidance\r\n").unwrap();
            let plan = preview_sync(&team, &app_state, app, &cancel).await.unwrap();
            assert_eq!(plan.items[0].support, SyncSupport::ToolRule);
            assert_eq!(plan.items[0].drift, DriftStatus::NotInstalled);
            assert!(plan.items[0].activation_path.is_some());
            let installed = sync_all(&team, &app_state, app, &[], &cancel)
                .await
                .unwrap();
            assert_eq!(installed.items[0].outcome, SyncOutcome::Installed);
            assert_eq!(
                rules::read(&app_type, &connection.id, 17)
                    .unwrap()
                    .body
                    .as_deref(),
                Some(std::str::from_utf8(body).unwrap())
            );
            assert!(fs::read_to_string(&personal)
                .unwrap()
                .starts_with("Personal guidance\r\n"));
            let unchanged = sync_all(&team, &app_state, app, &[], &cancel)
                .await
                .unwrap();
            assert_eq!(unchanged.items[0].outcome, SyncOutcome::Unchanged);
            let active =
                PathBuf::from(rules::activation_path(&app_type, &connection.id, 17).unwrap());
            let edited = fs::read_to_string(&active)
                .unwrap()
                .replace(std::str::from_utf8(body).unwrap(), "Local override.\n");
            fs::write(&active, &edited).unwrap();
            let conflict = sync_all(&team, &app_state, app, &[], &cancel)
                .await
                .unwrap();
            assert_eq!(conflict.items[0].outcome, SyncOutcome::Conflict);
            assert_eq!(fs::read_to_string(&active).unwrap(), edited);
            let plan = preview_sync(&team, &app_state, app, &cancel).await.unwrap();
            let replaced = sync_all(
                &team,
                &app_state,
                app,
                &[SyncOverwriteDecision {
                    asset_id: 17,
                    decision_token: plan.items[0].decision_token.clone(),
                }],
                &cancel,
            )
            .await
            .unwrap();
            assert_eq!(replaced.items[0].outcome, SyncOutcome::Installed);
            let restored = restore_local_backup(&team, &app_state, app, 17)
                .await
                .unwrap();
            assert!(restored.restored);
            let intent:super::super::state::teamai::AckIntent=serde_json::from_value(json!({
                "project":{"id":1,"name":"Fixture","organization_id":"local","workspace_id":"local","member_id":11},
                "command":{"command_id":"a".repeat(64),"asset_id":17,"revision_id":1,"kind":"rule","type":"install_rule","runtime":if app=="claude"{"claude-code"}else{"codex"},"content_hash":item.content_hash,"download_url":"/api/v1/assets/17/revisions/1/download","issued_at":Utc::now(),"expires_at":Utc::now()+chrono::Duration::minutes(29),"outcome":null}
            })).unwrap();
            team.state
                .prepare_teamai_ack(&connection.id, app, &intent)
                .unwrap();
            team.state
                .record_teamai_outcome(
                    &connection.id,
                    &intent.command.command_id,
                    super::super::api::teamai::Outcome::Conflict,
                )
                .unwrap();
            let mut interrupted = team.state.asset_states(&connection.id).unwrap().remove(0);
            interrupted.upstream_pending = true;
            interrupted.upstream_backup_path = Some(
                Path::new(interrupted.backup_root_path.as_ref().unwrap())
                    .join(UPSTREAM_BACKUP_FILE)
                    .to_string_lossy()
                    .into_owned(),
            );
            interrupted.pending_disk_state =
                Some(PendingDiskState::Hash(interrupted.local_hash.clone()));
            interrupted.pending_target_upstream_fingerprint = current_upstream_fingerprint(
                &app_state,
                &connection,
                &app_type,
                &item,
                ContentLimits::default(),
            )
            .unwrap();
            interrupted.pending_previous_upstream_fingerprint =
                interrupted.pending_target_upstream_fingerprint.clone();
            team.state.save_asset_state(&interrupted).unwrap();
            repair_one_pending(
                &team,
                &app_state,
                &connection,
                interrupted,
                ContentLimits::default(),
            )
            .unwrap();
            assert_eq!(
                team.state.pending_teamai_acks(&connection.id, app).unwrap()[0].outcome,
                Some(super::super::api::teamai::Outcome::Conflict)
            );
            assert_eq!(
                rules::read(&app_type, &connection.id, 17)
                    .unwrap()
                    .body
                    .as_deref(),
                Some("Local override.\n")
            );
            assert!(fs::read_to_string(&personal)
                .unwrap()
                .starts_with("Personal guidance\r\n"));
            server.abort();
        }
    }

    #[test]
    fn backup_restore_uses_creation_limits_after_remote_limits_shrink() {
        let stored = fixture_profile().limits;
        let mut current = stored.clone();
        current.max_text_bytes = 1;
        current.max_unpacked_bytes = 1;
        current.max_files = 1;
        let backup = UpstreamBackup {
            version: 1,
            limits: Some(stored.clone()),
            previous_state: None,
            previous_prompt: None,
            previous_rule: None,
        };
        let selected = restore_content_limits(&backup, &current).unwrap();
        assert_eq!(selected.max_text_bytes, stored.max_text_bytes);
        assert_eq!(selected.max_unpacked_bytes, stored.max_unpacked_bytes);
        assert_eq!(selected.max_files as u64, stored.max_files);
    }

    #[test]
    fn upstream_backup_alias_is_rejected_even_when_canonical_parents_match() {
        let temp = tempfile::tempdir().unwrap();
        let control = temp.path().join(".nexusops-team");
        fs::create_dir(&control).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let link = control.join("backups");
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
        let name = uuid::Uuid::new_v4().to_string();
        fs::create_dir(outside.join(&name)).unwrap();
        fs::write(
            outside.join(&name).join(UPSTREAM_BACKUP_FILE),
            r#"{"version":1,"previous_state":null,"previous_prompt":null}"#,
        )
        .unwrap();
        assert!(read_upstream_backup(temp.path(), &link.join(name)).is_err());
    }

    #[test]
    fn withdrawn_asset_is_marked_without_deleting_its_file() {
        let temp = tempfile::tempdir().unwrap();
        let team =
            TeamService::with_credentials(temp.path(), Arc::new(MemoryCredentialStore::default()))
                .unwrap();
        let mut connection = connection();
        connection.id = super::super::connection_id(&connection.gateway_url, &connection.profile)
            .expect("derive connection id");
        team.state.save_connection(&connection).unwrap();
        let file = temp.path().join("managed.md");
        fs::write(&file, "keep local copy").unwrap();
        team.state
            .save_asset_state(&ManagedAssetState {
                connection_id: connection.id.clone(),
                app: "codex".into(),
                asset_id: 19,
                asset_name: "Managed".into(),
                asset_kind: Some(AssetKind::Rule),
                revision: 2,
                content_hash: "a".repeat(64),
                archive_sha256: None,
                asset_files: Vec::new(),
                local_hash: "a".repeat(64),
                install_path: file.to_string_lossy().into_owned(),
                install_root: temp.path().to_string_lossy().into_owned(),
                relative_path: "managed.md".into(),
                backup_path: None,
                backup_root_path: None,
                backup_created_at: None,
                install_operation: None,
                upstream_pending: false,
                upstream_operation: None,
                upstream_backup_path: None,
                upstream_fingerprint: None,
                shared_upstream_fingerprint: None,
                pending_previous_upstream_fingerprint: None,
                pending_target_upstream_fingerprint: None,
                restored_unmanaged: false,
                pending_disk_state: None,
                pending_limits: None,
                subscribed: true,
                last_synced_at: None,
            })
            .unwrap();

        reconcile_removed(
            &team,
            &connection,
            &Manifest {
                schema_version: 1,
                assets: vec![],
                conflicts: vec![],
            },
            "codex",
        )
        .unwrap();

        let state = team.state.asset_states(&connection.id).unwrap().remove(0);
        assert!(!state.subscribed);
        assert_eq!(fs::read_to_string(file).unwrap(), "keep local copy");
        let history = local_history(&team, "codex").unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].local_file_present);
    }

    #[test]
    fn prompt_integration_stays_inactive() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("prompt.md");
        fs::write(&path, "team prompt").unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        let item = text_item(23, AssetKind::Prompt, b"team prompt");
        let expected = local_state_marker("missing_prompt", None);

        integrate_with_upstream(
            &state,
            &connection(),
            &AppType::Codex,
            &item,
            &path,
            &item.content_hash,
            ContentLimits::default(),
            None,
            Some(&expected),
        )
        .unwrap();

        let prompts = PromptService::get_prompts(&state, AppType::Codex).unwrap();
        let prompt = prompts.values().next().expect("Team prompt registered");
        assert_eq!(prompt.content, "team prompt");
        assert!(!prompt.enabled);
        let mut active = prompt.clone();
        active.enabled = true;
        state.db.save_prompt("codex", &active).unwrap();
        assert!(ensure_managed_prompt_inactive(
            &state,
            &connection(),
            &AppType::Codex,
            item.asset_id,
        )
        .is_err());
    }

    #[test]
    #[serial]
    fn pending_prompt_is_repaired_from_committed_team_state() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        crate::settings::reload_settings().unwrap();
        let team = TeamService::with_credentials(
            &temp.path().join("team-state"),
            Arc::new(MemoryCredentialStore::default()),
        )
        .unwrap();
        let mut connection = connection();
        connection.id = super::super::connection_id(&connection.gateway_url, &connection.profile)
            .expect("derive connection id");
        team.state.save_connection(&connection).unwrap();
        let app_state = AppState::new(Arc::new(Database::memory().unwrap()));
        let item = text_item(31, AssetKind::Prompt, b"committed prompt");
        let (root, relative, _) = target_for(&connection, &AppType::Codex, &item).unwrap();
        fs::create_dir_all(&root).unwrap();
        let receipt = super::super::install::install_verified(
            &item,
            b"committed prompt",
            &root,
            &relative,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        team.state
            .save_asset_state(&ManagedAssetState {
                connection_id: connection.id.clone(),
                app: "codex".into(),
                asset_id: item.asset_id,
                asset_name: item.name.clone(),
                asset_kind: Some(item.kind.clone()),
                revision: item.revision,
                content_hash: item.content_hash.clone(),
                archive_sha256: None,
                asset_files: Vec::new(),
                local_hash: receipt.local_hash.clone(),
                install_path: receipt.install_path.to_string_lossy().into_owned(),
                install_root: root.to_string_lossy().into_owned(),
                relative_path: relative.to_string_lossy().into_owned(),
                backup_path: receipt
                    .backup
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                backup_root_path: receipt
                    .backup_dir
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                backup_created_at: None,
                install_operation: receipt.operation.clone(),
                upstream_pending: true,
                upstream_operation: receipt.operation.clone(),
                upstream_backup_path: None,
                upstream_fingerprint: None,
                shared_upstream_fingerprint: None,
                pending_previous_upstream_fingerprint: Some(local_state_marker(
                    "missing_prompt",
                    None,
                )),
                pending_target_upstream_fingerprint: Some(prompt_state_fingerprint(
                    &receipt.local_hash,
                    false,
                )),
                restored_unmanaged: false,
                pending_disk_state: Some(PendingDiskState::Hash(receipt.local_hash.clone())),
                pending_limits: Some(connection.profile.limits.clone()),
                subscribed: true,
                last_synced_at: None,
            })
            .unwrap();

        let prompt_id = managed_item_id(&connection, &AppType::Codex, item.asset_id);
        PromptService::upsert_inactive(
            &app_state,
            AppType::Codex,
            &prompt_id,
            Prompt {
                id: prompt_id.clone(),
                name: "Local edit".into(),
                content: "changed while recovery was pending".into(),
                description: None,
                enabled: false,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();
        let blocked = repair_pending_upstream(&team, &app_state).unwrap();
        assert_eq!(blocked.repaired, 0);
        assert!(blocked
            .errors
            .contains_key(&("codex".into(), item.asset_id)));
        assert_eq!(
            PromptService::get_prompts(&app_state, AppType::Codex)
                .unwrap()
                .get(&prompt_id)
                .unwrap()
                .content,
            "changed while recovery was pending"
        );
        PromptService::delete_prompt(&app_state, AppType::Codex, &prompt_id).unwrap();
        assert_eq!(
            repair_pending_upstream(&team, &app_state).unwrap().repaired,
            1
        );
        let prompts = PromptService::get_prompts(&app_state, AppType::Codex).unwrap();
        let prompt = prompts
            .get(&managed_item_id(
                &connection,
                &AppType::Codex,
                item.asset_id,
            ))
            .unwrap();
        assert_eq!(prompt.content, "committed prompt");
        assert!(!prompt.enabled);
        assert!(
            !team
                .state
                .asset_states(&connection.id)
                .unwrap()
                .remove(0)
                .upstream_pending
        );
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[test]
    #[serial]
    fn shared_skill_catalog_does_not_create_cross_app_local_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        crate::settings::reload_settings().unwrap();
        let app_state = AppState::new(Arc::new(Database::memory().unwrap()));
        let mut connection = connection();
        connection.id = super::super::connection_id(&connection.gateway_url, &connection.profile)
            .expect("derive connection id");
        let mut item = ManifestItem {
            asset_id: 55,
            kind: AssetKind::Skill,
            slug: "shared-skill".into(),
            name: "Shared Skill".into(),
            revision: 1,
            content_hash: "hash-a".into(),
            archive_sha256: Some("0".repeat(64)),
            download_url: String::new(),
            content_type: "application/gzip".into(),
            byte_size: 0,
            files: Vec::new(),
        };
        let id = managed_skill_id(&connection, item.asset_id);
        let (codex_root, relative, _) = target_for(&connection, &AppType::Codex, &item).unwrap();
        let codex_path = codex_root.join(&relative);
        fs::create_dir_all(&codex_path).unwrap();
        fs::write(codex_path.join("SKILL.md"), "A").unwrap();
        SkillService::register_team_managed(
            &app_state.db,
            &AppType::Codex,
            &id,
            relative.file_name().unwrap().to_str().unwrap(),
            &item.name,
            "hash-a",
            "00000000-0000-4000-8000-000000000001",
            || {
                current_shared_skill_fingerprint(&app_state, &connection, &AppType::Codex, &item)
                    .map(|(_, _, hash)| hash)
                    .map_err(|error| error.to_string())
            },
            |path| {
                strict_skill_path_hash(&item, path, ContentLimits::default())
                    .map_err(|error| error.to_string())
            },
        )
        .unwrap();
        let codex_upstream = current_upstream_fingerprint(
            &app_state,
            &connection,
            &AppType::Codex,
            &item,
            ContentLimits::default(),
        )
        .unwrap();
        let codex_shared =
            current_shared_skill_fingerprint(&app_state, &connection, &AppType::Codex, &item)
                .unwrap()
                .0;
        let mut codex_state = ManagedAssetState {
            connection_id: connection.id.clone(),
            app: "codex".into(),
            asset_id: item.asset_id,
            asset_name: item.name.clone(),
            asset_kind: Some(AssetKind::Skill),
            revision: 1,
            content_hash: "hash-a".into(),
            archive_sha256: item.archive_sha256.clone(),
            asset_files: item.files.clone(),
            local_hash: "hash-a".into(),
            install_path: codex_path.to_string_lossy().into_owned(),
            install_root: codex_root.to_string_lossy().into_owned(),
            relative_path: relative.to_string_lossy().into_owned(),
            backup_path: None,
            backup_root_path: None,
            backup_created_at: None,
            install_operation: None,
            upstream_pending: false,
            upstream_operation: None,
            upstream_backup_path: None,
            upstream_fingerprint: codex_upstream,
            shared_upstream_fingerprint: Some(codex_shared),
            pending_previous_upstream_fingerprint: None,
            pending_target_upstream_fingerprint: None,
            restored_unmanaged: false,
            pending_disk_state: None,
            pending_limits: None,
            subscribed: true,
            last_synced_at: None,
        };

        let (second_app_local, _) = effective_local_hash(
            &app_state,
            &connection,
            &AppType::Claude,
            &item,
            None,
            &[codex_state.clone()],
            None,
            ContentLimits::default(),
        )
        .unwrap();
        assert_eq!(second_app_local, None);
        assert_eq!(
            classify_drift(None, second_app_local.as_deref(), "hash-a"),
            DriftStatus::NotInstalled
        );

        let (claude_root, claude_relative, _) =
            target_for(&connection, &AppType::Claude, &item).unwrap();
        let claude_path = claude_root.join(&claude_relative);
        fs::create_dir_all(&claude_path).unwrap();
        fs::write(claude_path.join("SKILL.md"), "A").unwrap();
        SkillService::register_team_managed(
            &app_state.db,
            &AppType::Claude,
            &id,
            claude_relative.file_name().unwrap().to_str().unwrap(),
            &item.name,
            "hash-a",
            "00000000-0000-4000-8000-000000000002",
            || {
                current_shared_skill_fingerprint(&app_state, &connection, &AppType::Claude, &item)
                    .map(|(_, _, hash)| hash)
                    .map_err(|error| error.to_string())
            },
            |path| {
                strict_skill_path_hash(&item, path, ContentLimits::default())
                    .map_err(|error| error.to_string())
            },
        )
        .unwrap();
        let mut claude_state = codex_state.clone();
        claude_state.app = "claude".into();
        claude_state.install_path = claude_path.to_string_lossy().into_owned();
        claude_state.install_root = claude_root.to_string_lossy().into_owned();
        claude_state.relative_path = claude_relative.to_string_lossy().into_owned();
        claude_state.upstream_fingerprint = current_upstream_fingerprint(
            &app_state,
            &connection,
            &AppType::Claude,
            &item,
            ContentLimits::default(),
        )
        .unwrap();
        claude_state.shared_upstream_fingerprint = Some(
            current_shared_skill_fingerprint(&app_state, &connection, &AppType::Claude, &item)
                .unwrap()
                .0,
        );

        fs::write(codex_path.join("SKILL.md"), "B").unwrap();
        SkillService::register_team_managed(
            &app_state.db,
            &AppType::Codex,
            &id,
            relative.file_name().unwrap().to_str().unwrap(),
            &item.name,
            "hash-b",
            "00000000-0000-4000-8000-000000000003",
            || {
                current_shared_skill_fingerprint(&app_state, &connection, &AppType::Codex, &item)
                    .map(|(_, _, hash)| hash)
                    .map_err(|error| error.to_string())
            },
            |path| {
                strict_skill_path_hash(&item, path, ContentLimits::default())
                    .map_err(|error| error.to_string())
            },
        )
        .unwrap();
        item.revision = 2;
        item.content_hash = "hash-b".into();
        codex_state.revision = 2;
        codex_state.content_hash = "hash-b".into();
        codex_state.local_hash = "hash-b".into();
        codex_state.upstream_fingerprint = current_upstream_fingerprint(
            &app_state,
            &connection,
            &AppType::Codex,
            &item,
            ContentLimits::default(),
        )
        .unwrap();
        codex_state.shared_upstream_fingerprint = Some(
            current_shared_skill_fingerprint(&app_state, &connection, &AppType::Codex, &item)
                .unwrap()
                .0,
        );
        let (claude_local, _) = effective_local_hash(
            &app_state,
            &connection,
            &AppType::Claude,
            &item,
            Some(&claude_state),
            &[codex_state.clone(), claude_state.clone()],
            Some("hash-a".into()),
            ContentLimits::default(),
        )
        .unwrap();
        assert_eq!(claude_local.as_deref(), Some("hash-a"));
        assert_eq!(
            classify_drift(Some("hash-a"), claude_local.as_deref(), "hash-b"),
            DriftStatus::RemoteUpdate
        );
        let ssot_path = SkillService::get_ssot_dir()
            .unwrap()
            .join(relative.file_name().unwrap());
        assert_eq!(fs::read_to_string(ssot_path.join("SKILL.md")).unwrap(), "B");
        apply_upstream_backup(
            &app_state,
            &connection,
            &AppType::Codex,
            item.asset_id,
            &AssetKind::Skill,
            &codex_path,
            &UpstreamBackup {
                version: 1,
                limits: None,
                previous_state: Some(codex_state.clone()),
                previous_prompt: None,
                previous_rule: None,
            },
            ContentLimits::default(),
            None,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(ssot_path.join("SKILL.md")).unwrap(),
            "B",
            "a per-app backup restore must not roll back the shared SSOT"
        );

        fs::write(ssot_path.join(".notes"), "hidden local note").unwrap();
        assert!(effective_local_hash(
            &app_state,
            &connection,
            &AppType::Claude,
            &item,
            Some(&claude_state),
            &[codex_state.clone(), claude_state.clone()],
            Some("hash-a".into()),
            ContentLimits::default(),
        )
        .is_err());
        fs::remove_file(ssot_path.join(".notes")).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = ssot_path.join("SKILL.md");
            let mut permissions = path.metadata().unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
            assert!(effective_local_hash(
                &app_state,
                &connection,
                &AppType::Claude,
                &item,
                Some(&claude_state),
                &[codex_state.clone(), claude_state.clone()],
                Some("hash-a".into()),
                ContentLimits::default(),
            )
            .is_err());
            let mut permissions = path.metadata().unwrap().permissions();
            permissions.set_mode(0o644);
            fs::set_permissions(&path, permissions).unwrap();
        }

        fs::write(ssot_path.join("SKILL.md"), "unrecorded local C").unwrap();
        let protected = effective_local_hash(
            &app_state,
            &connection,
            &AppType::Claude,
            &item,
            Some(&claude_state),
            &[codex_state.clone(), claude_state.clone()],
            Some("hash-a".into()),
            ContentLimits::default(),
        );
        assert!(protected.is_err());
        assert_eq!(
            fs::read_to_string(ssot_path.join("SKILL.md")).unwrap(),
            "unrecorded local C"
        );
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[tokio::test]
    #[serial]
    #[ignore = "requires the explicit isolated NexusOps HTTP fixture"]
    async fn real_gateway_sync_is_idempotent_and_protects_local_edits() {
        let state_path = std::env::var_os("NEXUSOPS_TEAM_HTTP_STATE_FILE")
            .expect("set isolated HTTP fixture state file");
        let fixture: serde_json::Value =
            serde_json::from_slice(&fs::read(state_path).unwrap()).unwrap();
        let key = fixture["key"]["key"].as_str().unwrap();
        let gateway = fixture["gateway_url"]
            .as_str()
            .unwrap_or("http://127.0.0.1:18191");
        let temporary = if std::env::var_os("NEXUSOPS_TEAM_SYNC_ROOT").is_none() {
            Some(tempfile::tempdir().unwrap())
        } else {
            None
        };
        let root = std::env::var_os("NEXUSOPS_TEAM_SYNC_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.as_ref().unwrap().path().to_path_buf());
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", &root);
        crate::settings::reload_settings().unwrap();
        let team = TeamService::with_credentials(
            &root.join("team-state"),
            Arc::new(MemoryCredentialStore::default()),
        )
        .unwrap();
        let app_state = AppState::new(Arc::new(Database::memory().unwrap()));
        let cancel = Cancellation::default();
        team.connect(gateway, key, &cancel).await.unwrap();

        let first = sync_all(&team, &app_state, "codex", &[], &cancel)
            .await
            .unwrap();
        assert!(first.items.len() >= 5);
        assert!(
            first.items.iter().all(|item| {
                matches!(
                    item.outcome,
                    SyncOutcome::Installed
                        | SyncOutcome::Downloaded
                        | SyncOutcome::ImportedInactive
                        | SyncOutcome::Unchanged
                )
            }),
            "unexpected first-sync results: {:#?}",
            first.items
        );
        let second = sync_all(&team, &app_state, "codex", &[], &cancel)
            .await
            .unwrap();
        assert!(second
            .items
            .iter()
            .all(|item| item.outcome == SyncOutcome::Unchanged));

        let editable = first
            .items
            .iter()
            .find(|item| std::path::Path::new(&item.install_path).is_file())
            .expect("find managed text asset")
            .clone();
        fs::write(&editable.install_path, "local edit").unwrap();
        let protected = sync_all(&team, &app_state, "codex", &[], &cancel)
            .await
            .unwrap();
        assert_eq!(
            protected
                .items
                .iter()
                .find(|item| item.asset_id == editable.asset_id)
                .unwrap()
                .outcome,
            SyncOutcome::Conflict
        );
        assert_eq!(
            fs::read_to_string(&editable.install_path).unwrap(),
            "local edit"
        );
        let overwrite_plan = preview_sync(&team, &app_state, "codex", &cancel)
            .await
            .unwrap();
        let decision = overwrite_plan
            .items
            .iter()
            .find(|item| item.asset.asset_id == editable.asset_id)
            .map(|item| SyncOverwriteDecision {
                asset_id: item.asset.asset_id,
                decision_token: item.decision_token.clone(),
            })
            .unwrap();
        let restored = sync_all(&team, &app_state, "codex", &[decision], &cancel)
            .await
            .unwrap();
        let restored = restored
            .items
            .iter()
            .find(|item| item.asset_id == editable.asset_id)
            .unwrap();
        assert!(matches!(
            restored.outcome,
            SyncOutcome::Downloaded | SyncOutcome::ImportedInactive
        ));
        assert!(restored.backup_path.is_some());
        assert_ne!(
            fs::read_to_string(&editable.install_path).unwrap(),
            "local edit"
        );
        team.disconnect().await.unwrap();
        assert!(std::path::Path::new(&editable.install_path).exists());
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }
}
