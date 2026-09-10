use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::TeamState;
use crate::services::team::{
    api::teamai::{Command, Outcome, Project},
    types::TeamError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AckIntent {
    pub project: Project,
    pub command: Command,
}

#[derive(Debug)]
pub struct PendingAck {
    pub intent: AckIntent,
    pub ready: bool,
    pub outcome: Option<Outcome>,
    pub retry_at: Option<DateTime<Utc>>,
}

#[derive(Default, Serialize)]
pub struct AckQueueStatus {
    pub pending: usize,
    pub last_error: Option<String>,
}

impl TeamState {
    pub fn teamai_ack_status(
        &self,
        connection_id: &str,
        app: &str,
    ) -> Result<AckQueueStatus, TeamError> {
        use rusqlite::OptionalExtension;
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let pending = connection
            .query_row(
                "SELECT count(*) FROM teamai_ack_queue WHERE connection_id=?1 AND app=?2",
                params![connection_id, app],
                |row| row.get(0),
            )
            .map_err(|_| TeamError::Storage)?;
        let last_error=connection.query_row("SELECT last_error FROM teamai_ack_queue WHERE connection_id=?1 AND app=?2 AND last_error IS NOT NULL ORDER BY retry_at DESC LIMIT 1",params![connection_id,app],|row|row.get(0)).optional().map_err(|_|TeamError::Storage)?;
        Ok(AckQueueStatus {
            pending,
            last_error,
        })
    }
    pub fn prepare_teamai_ack(
        &self,
        connection_id: &str,
        app: &str,
        intent: &AckIntent,
    ) -> Result<(), TeamError> {
        let metadata = serde_json::to_string(intent).map_err(|_| TeamError::Storage)?;
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        connection.execute("INSERT INTO teamai_ack_queue(connection_id,command_id,app,asset_id,revision,content_hash,metadata)
            VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(connection_id,command_id) DO UPDATE SET metadata=excluded.metadata",
            params![connection_id,intent.command.command_id,app,intent.command.asset_id,intent.command.revision_id,intent.command.content_hash,metadata]).map_err(|_|TeamError::Storage)?;
        Ok(())
    }

    pub fn pending_teamai_acks(
        &self,
        connection_id: &str,
        app: &str,
    ) -> Result<Vec<PendingAck>, TeamError> {
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let mut statement=connection.prepare("SELECT metadata,ready,outcome,retry_at FROM teamai_ack_queue WHERE connection_id=?1 AND app=?2 ORDER BY command_id").map_err(|_|TeamError::Storage)?;
        let rows = statement
            .query_map(params![connection_id, app], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|_| TeamError::Storage)?;
        rows.map(|row| {
            let (metadata, ready, outcome, retry_at) = row.map_err(|_| TeamError::Storage)?;
            Ok(PendingAck {
                intent: serde_json::from_str(&metadata).map_err(|_| TeamError::Storage)?,
                ready,
                outcome: outcome
                    .map(|value| {
                        serde_json::from_value(serde_json::Value::String(value))
                            .map_err(|_| TeamError::Storage)
                    })
                    .transpose()?,
                retry_at: retry_at
                    .map(|value| {
                        DateTime::parse_from_rfc3339(&value)
                            .map(|date| date.with_timezone(&Utc))
                            .map_err(|_| TeamError::Storage)
                    })
                    .transpose()?,
            })
        })
        .collect()
    }

    pub fn record_teamai_outcome(
        &self,
        connection_id: &str,
        command_id: &str,
        outcome: Outcome,
    ) -> Result<(), TeamError> {
        let value = serde_json::to_value(outcome).map_err(|_| TeamError::Storage)?;
        self.connection.lock().map_err(|_|TeamError::Storage)?.execute(
            "UPDATE teamai_ack_queue SET ready=1,outcome=?3 WHERE connection_id=?1 AND command_id=?2
             AND (?4=0 OR local_committed=1) AND (outcome IS NULL OR outcome NOT IN('applied','already_current'))",
            params![connection_id,command_id,value.as_str(),outcome.succeeded()]).map_err(|_|TeamError::Storage)?;
        Ok(())
    }

    pub fn defer_teamai_ack(
        &self,
        connection_id: &str,
        command_id: &str,
        error: &TeamError,
    ) -> Result<(), TeamError> {
        let seconds = match error {
            TeamError::RateLimited {
                retry_after_seconds,
            } => *retry_after_seconds,
            _ => 60,
        };
        let next = Utc::now() + chrono::Duration::seconds(seconds.min(3600) as i64);
        self.connection.lock().map_err(|_|TeamError::Storage)?.execute(
            "UPDATE teamai_ack_queue SET retry_at=?3,last_error=?4 WHERE connection_id=?1 AND command_id=?2",
            params![connection_id,command_id,next.to_rfc3339(),error.code()]).map_err(|_|TeamError::Storage)?;
        Ok(())
    }

    pub fn remove_teamai_ack(
        &self,
        connection_id: &str,
        command_id: &str,
    ) -> Result<(), TeamError> {
        self.connection
            .lock()
            .map_err(|_| TeamError::Storage)?
            .execute(
                "DELETE FROM teamai_ack_queue WHERE connection_id=?1 AND command_id=?2",
                params![connection_id, command_id],
            )
            .map_err(|_| TeamError::Storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::team::types::ManagedAssetState;
    use serde_json::json;

    fn intent() -> AckIntent {
        serde_json::from_value(json!({"project":{"id":1,"name":"Fixture","organization_id":"local","workspace_id":"local","member_id":11},"command":{"command_id":"fixture","asset_id":17,"revision_id":23,"kind":"skill","type":"install_skill","runtime":"codex","content_hash":"a".repeat(64),"download_url":"/api/v1/assets/17/revisions/23/download","issued_at":Utc::now(),"expires_at":Utc::now()+chrono::Duration::minutes(30),"outcome":null}})).unwrap()
    }
    fn installed() -> ManagedAssetState {
        serde_json::from_value(json!({"connection_id":"connection","app":"codex","asset_id":17,"revision":23,"content_hash":"a".repeat(64),"local_hash":"b".repeat(64),"install_path":"fixture/SKILL.md","subscribed":true,"last_synced_at":Utc::now().to_rfc3339()})).unwrap()
    }

    #[test]
    fn local_commit_is_required_and_survives_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let state = TeamState::open(temp.path()).unwrap();
        state
            .prepare_teamai_ack("connection", "codex", &intent())
            .unwrap();
        state
            .record_teamai_outcome("connection", "fixture", Outcome::Applied)
            .unwrap();
        assert!(!state.pending_teamai_acks("connection", "codex").unwrap()[0].ready);
        let mut metadata = installed();
        metadata.upstream_pending = true;
        state.save_installed_asset_state(&metadata).unwrap();
        assert!(!state.pending_teamai_acks("connection", "codex").unwrap()[0].ready);
        metadata.upstream_pending = false;
        state.save_installed_asset_state(&metadata).unwrap();
        drop(state);
        let state = TeamState::open(temp.path()).unwrap();
        let pending = state.pending_teamai_acks("connection", "codex").unwrap();
        assert!(pending[0].ready);
        assert_eq!(pending[0].outcome, Some(Outcome::Applied));
        assert!(state
            .pending_teamai_acks("other", "codex")
            .unwrap()
            .is_empty());
        state
            .record_teamai_outcome("connection", "fixture", Outcome::Failed)
            .unwrap();
        assert_eq!(
            state.pending_teamai_acks("connection", "codex").unwrap()[0].outcome,
            Some(Outcome::Applied)
        );
        state.disconnect("connection").unwrap();
        assert!(state
            .pending_teamai_acks("connection", "codex")
            .unwrap()
            .is_empty());
        assert_eq!(state.asset_states("connection").unwrap().len(), 1);
    }

    #[test]
    fn ack_marker_failure_rolls_back_asset_metadata_too() {
        let temp = tempfile::tempdir().unwrap();
        let state = TeamState::open(temp.path()).unwrap();
        state
            .prepare_teamai_ack("connection", "codex", &intent())
            .unwrap();
        state.connection.lock().unwrap().execute_batch("CREATE TRIGGER fixture_ack_failure BEFORE UPDATE ON teamai_ack_queue BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert_eq!(
            state.save_installed_asset_state(&installed()).unwrap_err(),
            TeamError::Storage
        );
        assert!(state.asset_states("connection").unwrap().is_empty());
        assert!(!state.pending_teamai_acks("connection", "codex").unwrap()[0].ready);
    }

    #[test]
    fn another_revision_cannot_make_an_intent_ready() {
        let temp = tempfile::tempdir().unwrap();
        let state = TeamState::open(temp.path()).unwrap();
        state
            .prepare_teamai_ack("connection", "codex", &intent())
            .unwrap();
        let mut metadata = installed();
        metadata.revision = 24;
        state.save_asset_state(&metadata).unwrap();
        assert!(!state.pending_teamai_acks("connection", "codex").unwrap()[0].ready);
        state
            .defer_teamai_ack(
                "connection",
                "fixture",
                &TeamError::RateLimited {
                    retry_after_seconds: 17,
                },
            )
            .unwrap();
        assert!(
            state.pending_teamai_acks("connection", "codex").unwrap()[0]
                .retry_at
                .unwrap()
                > Utc::now()
        );
    }

    #[test]
    fn metadata_maintenance_cannot_promote_a_failed_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let state = TeamState::open(temp.path()).unwrap();
        state
            .prepare_teamai_ack("connection", "codex", &intent())
            .unwrap();
        state
            .record_teamai_outcome("connection", "fixture", Outcome::Conflict)
            .unwrap();
        state.save_asset_state(&installed()).unwrap();
        state
            .record_teamai_outcome("connection", "fixture", Outcome::Applied)
            .unwrap();
        assert_eq!(
            state.pending_teamai_acks("connection", "codex").unwrap()[0].outcome,
            Some(Outcome::Conflict)
        );
    }
}
