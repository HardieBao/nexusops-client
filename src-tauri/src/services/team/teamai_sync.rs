use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{
    api::{
        teamai::{Command, Outcome, Project, Runtime},
        Cancellation, TeamApi,
    },
    state::teamai::AckIntent,
    sync::{
        self, SyncBatchResult, SyncOutcome, SyncOverwriteDecision, SyncPlan, SyncPlanItem,
        SyncSupport, TeamSyncError,
    },
    types::{AssetKind, ConnectionStatus, TeamConnection, TeamError},
    TeamService,
};
use crate::store::AppState;

#[cfg(test)]
mod tests;

#[derive(Serialize)]
pub struct Preview {
    pub project: Project,
    pub commands: Vec<Command>,
    pub plan: SyncPlan,
    pub legacy_items: Vec<SyncPlanItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedAsset {
    pub asset_id: i64,
    pub revision: i64,
    pub content_hash: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedCommand {
    pub asset_id: i64,
    pub command_id: String,
}

#[derive(Default, Serialize)]
pub struct AckSummary {
    pub acknowledged: usize,
    pub superseded: usize,
    pub waiting: usize,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
pub struct ApplyResult {
    pub install: SyncBatchResult,
    pub acknowledgements: AckSummary,
}

fn app(runtime: Runtime) -> &'static str {
    match runtime {
        Runtime::Codex => "codex",
        Runtime::ClaudeCode => "claude",
    }
}

pub fn ack_status(
    team: &TeamService,
    runtime: Runtime,
) -> Result<(Option<TeamConnection>, super::state::teamai::AckQueueStatus), TeamError> {
    let connection = team.status()?;
    let status = match connection.as_ref() {
        Some(connection) => team.state.teamai_ack_status(&connection.id, app(runtime))?,
        None => Default::default(),
    };
    Ok((connection, status))
}

/// Background work never installs or repairs files. Foreground operations own those actions.
pub async fn retry_background(
    team: &TeamService,
    cancel: &Cancellation,
) -> Result<(), TeamSyncError> {
    let Ok(_guard) = team.sync_operation.try_lock() else {
        return Ok(());
    };
    if cancel.is_cancelled() {
        return Err(TeamError::Cancelled.into());
    }
    let Some(current) = team.status()? else {
        return Ok(());
    };
    if current.status != ConnectionStatus::Connected {
        return Ok(());
    }
    for runtime in [Runtime::Codex, Runtime::ClaudeCode] {
        let pending = team.state.pending_teamai_acks(&current.id, app(runtime))?;
        if !pending
            .iter()
            .any(|item| item.ready && item.retry_at.is_none_or(|date| date <= Utc::now()))
        {
            continue;
        }
        match protocol(team, runtime, cancel).await {
            Ok((connection, api, key, project)) => {
                if connection.id != current.id {
                    return Err(TeamError::DifferentIdentity.into());
                }
                let summary =
                    retry_ready(team, &api, &key, &project, &connection.id, runtime, cancel)
                        .await?;
                if summary.last_error.is_some() {
                    break;
                }
            }
            Err(TeamError::Cancelled) => return Err(TeamError::Cancelled.into()),
            Err(error) => {
                defer_pending(team, &current.id, &error)?;
                break;
            }
        }
    }
    Ok(())
}

fn defer_pending(
    team: &TeamService,
    connection_id: &str,
    error: &TeamError,
) -> Result<(), TeamError> {
    for runtime in [Runtime::Codex, Runtime::ClaudeCode] {
        for item in team
            .state
            .pending_teamai_acks(connection_id, app(runtime))?
        {
            if item.ready {
                team.state.defer_teamai_ack(
                    connection_id,
                    &item.intent.command.command_id,
                    error,
                )?;
            }
        }
    }
    if let Some(mut connection) = team.status()? {
        if connection.id == connection_id
            && matches!(
                error,
                TeamError::AuthenticationRequired | TeamError::AccessDenied
            )
        {
            connection.status = if *error == TeamError::AuthenticationRequired {
                ConnectionStatus::AuthenticationRequired
            } else {
                ConnectionStatus::AccessDenied
            };
            connection.last_error = Some(error.code().into());
            team.state.save_connection(&connection)?;
        }
    }
    Ok(())
}

async fn protocol(
    team: &TeamService,
    runtime: Runtime,
    cancel: &Cancellation,
) -> Result<(TeamConnection, TeamApi, String, Project), TeamError> {
    let connection = team.status()?.ok_or(TeamError::NotConnected)?;
    let api = TeamApi::new(&connection.gateway_url)?;
    let key = team.member_key()?;
    let project = api
        .teamai_project(&key, &connection.profile, cancel)
        .await?;
    api.teamai_config(&key, cancel).await?;
    api.teamai_report(&key, runtime, cancel).await?;
    Ok((connection, api, key, project))
}

async fn prepare(
    team: &TeamService,
    app_state: &AppState,
    runtime: Runtime,
    selected: Option<&[i64]>,
    cancel: &Cancellation,
) -> Result<(Preview, TeamApi, String), TeamSyncError> {
    sync::recover_known_installs(&team.state)?;
    let repairs = sync::repair_pending_upstream(team, app_state)?;
    let plan = sync::load_plan(team, app_state, app(runtime), cancel, &repairs.errors).await?;
    let (connection, api, key, project) = protocol(team, runtime, cancel).await?;
    if connection.id != plan.connection.id {
        return Err(TeamError::DifferentIdentity.into());
    }
    let commands = api
        .teamai_sync(&key, &project, runtime, selected, cancel)
        .await?;
    let mut plan = plan;
    let legacy_items = plan
        .items
        .iter()
        .filter(|item| !matches!(item.asset.kind, AssetKind::Skill | AssetKind::Rule))
        .cloned()
        .collect();
    plan.items.retain(|item| {
        commands
            .iter()
            .any(|command| command.asset_id == item.asset.asset_id)
    });
    if plan.items.len() != commands.len()
        || plan.items.iter().any(|item| {
            !commands.iter().any(|command| {
                command.asset_id == item.asset.asset_id
                    && command.revision_id == item.asset.revision
                    && command.content_hash == item.asset.content_hash
                    && matches!(
                        (command.kind, &item.asset.kind),
                        (
                            super::api::teamai::ResourceKind::Skill,
                            super::types::AssetKind::Skill
                        ) | (
                            super::api::teamai::ResourceKind::Rule,
                            super::types::AssetKind::Rule
                        )
                    )
            })
        })
    {
        return Err(TeamError::Conflict.into());
    }
    Ok((
        Preview {
            project,
            commands,
            plan,
            legacy_items,
        },
        api,
        key,
    ))
}

pub async fn preview(
    team: &TeamService,
    app_state: &AppState,
    runtime: Runtime,
    selected: Option<&[i64]>,
    cancel: &Cancellation,
) -> Result<Preview, TeamSyncError> {
    let _guard = tokio::select! {_ = cancel.cancelled()=>return Err(TeamError::Cancelled.into()),guard=team.sync_operation.lock()=>guard};
    prepare(team, app_state, runtime, selected, cancel)
        .await
        .map(|(preview, _, _)| preview)
}

pub async fn apply(
    team: &TeamService,
    app_state: &AppState,
    runtime: Runtime,
    reviewed: &[ReviewedCommand],
    legacy_reviewed: &[ReviewedAsset],
    overwrite: &[SyncOverwriteDecision],
    cancel: &Cancellation,
) -> Result<ApplyResult, TeamSyncError> {
    let _guard = tokio::select! {_ = cancel.cancelled()=>return Err(TeamError::Cancelled.into()),guard=team.sync_operation.lock()=>guard};
    if reviewed.len() > 200 {
        return Err(TeamError::TooLarge.into());
    }
    let selected = reviewed
        .iter()
        .map(|command| command.asset_id)
        .collect::<Vec<_>>();
    let (mut preview, api, key) =
        prepare(team, app_state, runtime, Some(&selected), cancel).await?;
    if preview.commands.iter().any(|command| {
        !reviewed.iter().any(|expected| {
            expected.asset_id == command.asset_id && expected.command_id == command.command_id
        })
    }) {
        return Err(TeamError::Conflict.into());
    }
    // A managed download is not an installation into a tool.
    if preview
        .plan
        .items
        .iter()
        .any(|item| !matches!(item.support, SyncSupport::ToolSkill | SyncSupport::ToolRule))
    {
        return Err(TeamSyncError::Local(
            "This resource cannot be installed into the selected tool".into(),
        ));
    }
    if legacy_reviewed.len() as u64 > preview.plan.connection.profile.limits.max_assets {
        return Err(TeamError::TooLarge.into());
    }
    let mut legacy_ids = std::collections::HashSet::new();
    for expected in legacy_reviewed {
        if !legacy_ids.insert(expected.asset_id) {
            return Err(TeamError::Conflict.into());
        }
        let item = preview
            .legacy_items
            .iter()
            .find(|item| {
                item.asset.asset_id == expected.asset_id
                    && item.asset.revision == expected.revision
                    && item.asset.content_hash == expected.content_hash
            })
            .ok_or(TeamError::Conflict)?;
        preview.plan.items.push(item.clone());
    }
    preview.plan.items.sort_by_key(|item| item.asset.asset_id);
    let connection_id = preview.plan.connection.id.clone();
    for command in &preview.commands {
        team.state.prepare_teamai_ack(
            &connection_id,
            app(runtime),
            &AckIntent {
                project: preview.project.clone(),
                command: command.clone(),
            },
        )?;
    }
    let deadline = preview
        .commands
        .iter()
        .map(|command| command.expires_at)
        .min();
    let mut install = sync::apply_plan(
        team,
        app_state,
        app(runtime),
        overwrite,
        cancel,
        preview.plan,
        deadline,
    )
    .await?;
    for result in &install.items {
        if result.error_code.as_deref() == Some("upstream_pending") {
            continue;
        }
        let outcome = match result.outcome {
            SyncOutcome::Installed => Outcome::Applied,
            SyncOutcome::Unchanged => Outcome::AlreadyCurrent,
            SyncOutcome::Conflict => Outcome::Conflict,
            SyncOutcome::Failed => Outcome::Failed,
            _ => Outcome::Skipped,
        };
        if let Some(command) = preview
            .commands
            .iter()
            .find(|command| command.asset_id == result.asset_id)
        {
            team.state
                .record_teamai_outcome(&connection_id, &command.command_id, outcome)?;
        }
    }
    let acknowledgements = retry_ready(
        team,
        &api,
        &key,
        &preview.project,
        &connection_id,
        runtime,
        cancel,
    )
    .await?;
    if let Some(current) = team.status()? {
        if current.id == connection_id {
            install.connection = current;
        }
    }
    Ok(ApplyResult {
        install,
        acknowledgements,
    })
}

pub async fn retry(
    team: &TeamService,
    app_state: &AppState,
    runtime: Runtime,
    cancel: &Cancellation,
) -> Result<AckSummary, TeamSyncError> {
    let _guard = tokio::select! {_ = cancel.cancelled()=>return Err(TeamError::Cancelled.into()),guard=team.sync_operation.lock()=>guard};
    sync::recover_known_installs(&team.state)?;
    sync::repair_pending_upstream(team, app_state)?;
    let current = team.status()?.ok_or(TeamError::NotConnected)?;
    let pending = team.state.pending_teamai_acks(&current.id, app(runtime))?;
    if !pending
        .iter()
        .any(|item| item.ready && item.retry_at.is_none_or(|date| date <= Utc::now()))
    {
        return Ok(AckSummary {
            waiting: pending.len(),
            ..AckSummary::default()
        });
    }
    let (connection, api, key, project) = protocol(team, runtime, cancel).await?;
    retry_ready(team, &api, &key, &project, &connection.id, runtime, cancel).await
}

async fn retry_ready(
    team: &TeamService,
    api: &TeamApi,
    key: &str,
    project: &Project,
    connection_id: &str,
    runtime: Runtime,
    cancel: &Cancellation,
) -> Result<AckSummary, TeamSyncError> {
    let pending = team
        .state
        .pending_teamai_acks(connection_id, app(runtime))?;
    let mut summary = AckSummary::default();
    for item in &pending {
        if !item.ready || item.retry_at.is_some_and(|date| date > Utc::now()) {
            continue;
        }
        if cancel.is_cancelled() {
            return Err(TeamError::Cancelled.into());
        }
        if item.intent.project.member_id != project.member_id
            || item.intent.project.organization_id != project.organization_id
            || item.intent.project.workspace_id != project.workspace_id
        {
            team.state
                .remove_teamai_ack(connection_id, &item.intent.command.command_id)?;
            summary.superseded += 1;
            continue;
        }
        let result = async {
            let mut renewed = api
                .teamai_sync(
                    key,
                    project,
                    runtime,
                    Some(&[item.intent.command.asset_id]),
                    cancel,
                )
                .await?;
            let command = renewed.pop().ok_or(TeamError::NotFound)?;
            if command.command_id != item.intent.command.command_id
                || command.content_hash != item.intent.command.content_hash
            {
                return Err(TeamError::Conflict);
            }
            team.state.prepare_teamai_ack(
                connection_id,
                app(runtime),
                &AckIntent {
                    project: project.clone(),
                    command: command.clone(),
                },
            )?;
            api.teamai_ack(
                key,
                project,
                &command,
                item.outcome.ok_or(TeamError::Storage)?,
                cancel,
            )
            .await?;
            Ok::<(), TeamError>(())
        }
        .await;
        match result {
            Ok(()) => {
                team.state
                    .remove_teamai_ack(connection_id, &item.intent.command.command_id)?;
                summary.acknowledged += 1;
            }
            Err(TeamError::NotFound | TeamError::Conflict) => {
                team.state
                    .remove_teamai_ack(connection_id, &item.intent.command.command_id)?;
                summary.superseded += 1;
            }
            Err(TeamError::Cancelled) => return Err(TeamError::Cancelled.into()),
            Err(error) => {
                defer_pending(team, connection_id, &error)?;
                summary.last_error = Some(error.code().into());
                break;
            }
        }
    }
    summary.waiting = team
        .state
        .pending_teamai_acks(connection_id, app(runtime))?
        .len();
    Ok(summary)
}
