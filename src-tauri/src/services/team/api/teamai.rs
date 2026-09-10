use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    decode_json, response_bytes, transport_error, Cancellation, HeaderValue, TeamApi, TeamError,
};
use crate::services::team::types::TeamProfile;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Runtime {
    Codex,
    ClaudeCode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Skill,
    Rule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallType {
    InstallSkill,
    InstallRule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Applied,
    AlreadyCurrent,
    Conflict,
    Skipped,
    Cancelled,
    Failed,
}

impl Outcome {
    pub fn succeeded(self) -> bool {
        matches!(self, Self::Applied | Self::AlreadyCurrent)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: u32,
    pub name: String,
    pub organization_id: String,
    pub workspace_id: String,
    pub member_id: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Projects {
    schema_version: u32,
    projects: Vec<Project>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub project_id: u32,
    pub runtimes: Vec<Runtime>,
    pub resource_types: Vec<ResourceKind>,
    pub operations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub accepted: bool,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub command_id: String,
    pub asset_id: i64,
    pub revision_id: i64,
    pub kind: ResourceKind,
    #[serde(rename = "type")]
    pub install_type: InstallType,
    pub runtime: Runtime,
    pub content_hash: String,
    pub download_url: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub outcome: Option<Outcome>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sync {
    schema_version: u32,
    commands: Vec<Command>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ack {
    pub schema_version: u32,
    pub command_id: String,
    pub outcome: Outcome,
}

impl TeamApi {
    pub async fn teamai_project(
        &self,
        key: &str,
        profile: &TeamProfile,
        cancel: &Cancellation,
    ) -> Result<Project, TeamError> {
        let mut response: Projects = self
            .json("api/v1/me/teamai/projects", key, 16384, cancel)
            .await
            .map_err(upgrade_if_missing)?;
        if response.schema_version != 1 {
            return Err(TeamError::UpgradeRequired);
        }
        if response.projects.len() != 1 {
            return Err(TeamError::InvalidResponse);
        }
        let project = response.projects.remove(0);
        if project.id != 1 || project.member_id <= 0 || project.name.chars().count() > 160 {
            return Err(TeamError::InvalidResponse);
        }
        if project.organization_id != profile.organization_id
            || project.workspace_id != profile.workspace_id
        {
            return Err(TeamError::DifferentIdentity);
        }
        Ok(project)
    }

    pub async fn teamai_config(
        &self,
        key: &str,
        cancel: &Cancellation,
    ) -> Result<Config, TeamError> {
        let response: Config = self
            .json("api/v1/me/teamai/config?project_id=1", key, 16384, cancel)
            .await
            .map_err(upgrade_if_missing)?;
        if response.schema_version != 1 || response.project_id != 1 {
            return Err(TeamError::UpgradeRequired);
        }
        let required = ["projects", "config", "report", "sync", "ack"];
        if response.operations.len() != required.len()
            || required
                .iter()
                .any(|required| !response.operations.iter().any(|value| value == required))
        {
            return Err(TeamError::UpgradeRequired);
        }
        if response.runtimes.len() != 2
            || !response.runtimes.contains(&Runtime::Codex)
            || !response.runtimes.contains(&Runtime::ClaudeCode)
            || response.resource_types.len() != 2
            || !response.resource_types.contains(&ResourceKind::Skill)
            || !response.resource_types.contains(&ResourceKind::Rule)
        {
            return Err(TeamError::InvalidResponse);
        }
        Ok(response)
    }

    pub async fn teamai_report(
        &self,
        key: &str,
        runtime: Runtime,
        cancel: &Cancellation,
    ) -> Result<Report, TeamError> {
        let response: Report = self.teamai_post("report", key, &json!({"schema_version":1,"project_id":1,"runtime":runtime,"bridge_version":env!("CARGO_PKG_VERSION")}), 16384, cancel).await?;
        if response.schema_version != 1 || !response.accepted {
            return Err(TeamError::InvalidResponse);
        }
        Ok(response)
    }

    pub async fn teamai_sync(
        &self,
        key: &str,
        project: &Project,
        runtime: Runtime,
        selected: Option<&[i64]>,
        cancel: &Cancellation,
    ) -> Result<Vec<Command>, TeamError> {
        if project.id != 1 || project.member_id <= 0 {
            return Err(TeamError::DifferentIdentity);
        }
        let mut body = json!({"schema_version":1,"project_id":1,"runtime":runtime});
        if let Some(ids) = selected {
            if ids.len() > 200 {
                return Err(TeamError::TooLarge);
            }
            let mut unique = HashSet::new();
            if ids.iter().any(|id| *id <= 0 || !unique.insert(*id)) {
                return Err(TeamError::InvalidResponse);
            }
            body["asset_ids"] = json!(ids);
        }
        let response: Sync = self
            .teamai_post("sync", key, &body, 4 << 20, cancel)
            .await?;
        if response.schema_version != 1 || response.commands.len() > 200 {
            return Err(TeamError::InvalidResponse);
        }
        let mut unique = HashSet::new();
        for command in &response.commands {
            validate_command(self, project, runtime, command, Utc::now())?;
            if !unique.insert(command.asset_id) {
                return Err(TeamError::InvalidResponse);
            }
        }
        if selected.is_some_and(|ids| {
            ids.len() != unique.len() || ids.iter().any(|id| !unique.contains(id))
        }) {
            return Err(TeamError::InvalidResponse);
        }
        Ok(response.commands)
    }

    /// Call only after the installation layer has established the local outcome.
    pub async fn teamai_ack(
        &self,
        key: &str,
        project: &Project,
        command: &Command,
        outcome: Outcome,
        cancel: &Cancellation,
    ) -> Result<Ack, TeamError> {
        validate_command(self, project, command.runtime, command, Utc::now())?;
        let response: Ack = self.teamai_post("ack", key, &json!({"schema_version":1,"project_id":1,"command_id":command.command_id,"runtime":command.runtime,"outcome":outcome}), 16384, cancel).await?;
        if response.schema_version != 1
            || response.command_id != command.command_id
            || (response.outcome != outcome
                && !(response.outcome.succeeded() && outcome.succeeded()))
        {
            return Err(TeamError::InvalidResponse);
        }
        Ok(response)
    }

    async fn teamai_post<T: serde::de::DeserializeOwned>(
        &self,
        operation: &str,
        key: &str,
        body: &serde_json::Value,
        limit: usize,
        cancel: &Cancellation,
    ) -> Result<T, TeamError> {
        if !matches!(operation, "report" | "sync" | "ack") {
            return Err(TeamError::InvalidResponse);
        }
        if key.is_empty() || key.len() > 128 || key.trim() != key {
            return Err(TeamError::AuthenticationRequired);
        }
        let mut credential =
            HeaderValue::from_str(key).map_err(|_| TeamError::AuthenticationRequired)?;
        credential.set_sensitive(true);
        let payload = serde_json::to_vec(body).map_err(|_| TeamError::InvalidResponse)?;
        if payload.len() > 16384 {
            return Err(TeamError::TooLarge);
        }
        let request = async {
            let response = self
                .client
                .post(self.endpoint(&format!("api/v1/me/teamai/{operation}"))?)
                .header("X-Nexus-Member-Key", credential)
                .header("Content-Type", "application/json")
                .body(payload)
                .send()
                .await
                .map_err(transport_error)?;
            let bytes = response_bytes(response, limit).await?;
            decode_json(&bytes, key)
        };
        // POST retries are owned by the durable sync/ACK state, never by this transport.
        tokio::select! { biased; _ = cancel.cancelled() => Err(TeamError::Cancelled), result=request => result }
    }
}

fn upgrade_if_missing(error: TeamError) -> TeamError {
    if error == TeamError::NotFound {
        TeamError::UpgradeRequired
    } else {
        error
    }
}

fn command_id(project: &Project, asset: i64, revision: i64, runtime: Runtime) -> String {
    let canonical = json!([
        project.organization_id,
        project.workspace_id,
        project.member_id,
        asset,
        revision,
        runtime
    ])
    .to_string()
    .replace('&', "\\u0026")
    .replace('<', "\\u003c")
    .replace('>', "\\u003e")
    .replace('\u{2028}', "\\u2028")
    .replace('\u{2029}', "\\u2029");
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

fn validate_command(
    api: &TeamApi,
    project: &Project,
    runtime: Runtime,
    command: &Command,
    now: DateTime<Utc>,
) -> Result<(), TeamError> {
    if command.asset_id <= 0
        || command.revision_id <= 0
        || command.runtime != runtime
        || command.content_hash.len() != 64
        || !command
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || command.command_id != command_id(project, command.asset_id, command.revision_id, runtime)
        || !matches!(
            (command.kind, command.install_type),
            (ResourceKind::Skill, InstallType::InstallSkill)
                | (ResourceKind::Rule, InstallType::InstallRule)
        )
        || command.expires_at <= command.issued_at
        || command.expires_at - command.issued_at > chrono::Duration::minutes(30)
        || command.issued_at > now + chrono::Duration::minutes(5)
    {
        return Err(TeamError::InvalidResponse);
    }
    if command.expires_at <= now {
        return Err(TeamError::Conflict);
    }
    api.checked_reference(
        &command.download_url,
        &format!(
            "api/v1/assets/{}/revisions/{}/download",
            command.asset_id, command.revision_id
        ),
    )?;
    Ok(())
}
