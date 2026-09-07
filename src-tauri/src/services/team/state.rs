use std::{fs, path::Path, sync::Mutex, time::Duration};

use rusqlite::{params, Connection, OptionalExtension};

use super::types::{ManagedAssetState, ManagedProviderLink, TeamConnection, TeamError};

pub struct TeamState {
    connection: Mutex<Connection>,
}

impl TeamState {
    pub fn open(data_dir: &Path) -> Result<Self, TeamError> {
        fs::create_dir_all(data_dir).map_err(|_| TeamError::Storage)?;
        let connection =
            Connection::open(data_dir.join("team.db")).map_err(|_| TeamError::Storage)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|_| TeamError::Storage)?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| TeamError::Storage)?;
        if version > 2 {
            return Err(TeamError::Storage);
        }
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS team_connection (
                 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
                 metadata TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS team_assets (
                 connection_id TEXT NOT NULL,
                 app TEXT NOT NULL,
                 asset_id INTEGER NOT NULL,
                 metadata TEXT NOT NULL,
                 PRIMARY KEY(connection_id,app,asset_id)
             );
             CREATE TABLE IF NOT EXISTS team_provider_links (
                 connection_id TEXT NOT NULL,
                 app TEXT NOT NULL,
                 provider_id TEXT NOT NULL,
                 managed_model TEXT,
                 PRIMARY KEY(connection_id,app)
             );",
            )
            .map_err(|_| TeamError::Storage)?;
        if version < 2 {
            let has_managed_model = {
                let mut statement = connection
                    .prepare("PRAGMA table_info(team_provider_links)")
                    .map_err(|_| TeamError::Storage)?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))
                    .map_err(|_| TeamError::Storage)?;
                columns
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| TeamError::Storage)?
                    .iter()
                    .any(|column| column == "managed_model")
            };
            if !has_managed_model {
                connection
                    .execute_batch("ALTER TABLE team_provider_links ADD COLUMN managed_model TEXT;")
                    .map_err(|_| TeamError::Storage)?;
            }
            connection
                .pragma_update(None, "user_version", 2)
                .map_err(|_| TeamError::Storage)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn connection(&self) -> Result<Option<TeamConnection>, TeamError> {
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let text: Option<String> = connection
            .query_row(
                "SELECT metadata FROM team_connection WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| TeamError::Storage)?;
        text.map(|value| serde_json::from_str(&value).map_err(|_| TeamError::Storage))
            .transpose()
    }

    pub fn save_connection(&self, metadata: &TeamConnection) -> Result<(), TeamError> {
        let text = serde_json::to_string(metadata).map_err(|_| TeamError::Storage)?;
        self.connection
            .lock()
            .map_err(|_| TeamError::Storage)?
            .execute(
                "INSERT INTO team_connection(singleton,metadata) VALUES(1,?1)
             ON CONFLICT(singleton) DO UPDATE SET metadata=excluded.metadata",
                [text],
            )
            .map_err(|_| TeamError::Storage)?;
        Ok(())
    }

    pub fn disconnect(&self, connection_id: &str) -> Result<(), TeamError> {
        let mut connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let transaction = connection.transaction().map_err(|_| TeamError::Storage)?;
        // Keep installation baselines and provider links; disconnect does not delete files/providers.
        transaction.execute(
            "UPDATE team_assets SET metadata=json_set(metadata,'$.subscribed',json('false')) WHERE connection_id=?1", [connection_id],
        ).map_err(|_| TeamError::Storage)?;
        transaction
            .execute("DELETE FROM team_connection WHERE singleton=1", [])
            .map_err(|_| TeamError::Storage)?;
        transaction.commit().map_err(|_| TeamError::Storage)
    }

    pub fn asset_states(&self, connection_id: &str) -> Result<Vec<ManagedAssetState>, TeamError> {
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let mut statement = connection
            .prepare(
                "SELECT metadata FROM team_assets WHERE connection_id=?1 ORDER BY app,asset_id",
            )
            .map_err(|_| TeamError::Storage)?;
        let rows = statement
            .query_map([connection_id], |row| row.get::<_, String>(0))
            .map_err(|_| TeamError::Storage)?;
        rows.map(|row| {
            let text = row.map_err(|_| TeamError::Storage)?;
            serde_json::from_str(&text).map_err(|_| TeamError::Storage)
        })
        .collect()
    }

    pub fn save_asset_state(&self, metadata: &ManagedAssetState) -> Result<(), TeamError> {
        let text = serde_json::to_string(metadata).map_err(|_| TeamError::Storage)?;
        self.connection
            .lock()
            .map_err(|_| TeamError::Storage)?
            .execute(
                "INSERT INTO team_assets(connection_id,app,asset_id,metadata) VALUES(?1,?2,?3,?4)
             ON CONFLICT(connection_id,app,asset_id) DO UPDATE SET metadata=excluded.metadata",
                params![
                    metadata.connection_id,
                    metadata.app,
                    metadata.asset_id,
                    text
                ],
            )
            .map_err(|_| TeamError::Storage)?;
        Ok(())
    }

    pub fn provider_links(
        &self,
        connection_id: &str,
    ) -> Result<Vec<ManagedProviderLink>, TeamError> {
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let mut statement = connection.prepare("SELECT app,provider_id,managed_model FROM team_provider_links WHERE connection_id=?1 ORDER BY app").map_err(|_| TeamError::Storage)?;
        let rows = statement
            .query_map([connection_id], |row| {
                Ok(ManagedProviderLink {
                    connection_id: connection_id.into(),
                    app: row.get(0)?,
                    provider_id: row.get(1)?,
                    managed_model: row.get(2)?,
                })
            })
            .map_err(|_| TeamError::Storage)?;
        rows.map(|row| row.map_err(|_| TeamError::Storage))
            .collect()
    }

    pub fn all_asset_states(&self) -> Result<Vec<ManagedAssetState>, TeamError> {
        let connection = self.connection.lock().map_err(|_| TeamError::Storage)?;
        let mut statement = connection
            .prepare("SELECT metadata FROM team_assets ORDER BY connection_id,app,asset_id")
            .map_err(|_| TeamError::Storage)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| TeamError::Storage)?;
        rows.map(|row| {
            let text = row.map_err(|_| TeamError::Storage)?;
            serde_json::from_str(&text).map_err(|_| TeamError::Storage)
        })
        .collect()
    }

    pub fn save_provider_link(&self, link: &ManagedProviderLink) -> Result<(), TeamError> {
        self.connection
            .lock()
            .map_err(|_| TeamError::Storage)?
            .execute(
                "INSERT INTO team_provider_links(connection_id,app,provider_id,managed_model) VALUES(?1,?2,?3,?4)
             ON CONFLICT(connection_id,app) DO UPDATE SET provider_id=excluded.provider_id,managed_model=excluded.managed_model",
                params![link.connection_id, link.app, link.provider_id, link.managed_model],
            )
            .map_err(|_| TeamError::Storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_link_schema_migrates_once_and_preserves_managed_model() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("team.db");
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE team_provider_links (
                    connection_id TEXT NOT NULL,
                    app TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    PRIMARY KEY(connection_id,app)
                 );
                 PRAGMA user_version=1;",
            )
            .unwrap();
        drop(legacy);

        let state = TeamState::open(temp.path()).unwrap();
        state
            .save_provider_link(&ManagedProviderLink {
                connection_id: "connection".into(),
                app: "grokbuild".into(),
                provider_id: "provider".into(),
                managed_model: Some("team-model-a".into()),
            })
            .unwrap();
        drop(state);

        let reopened = TeamState::open(temp.path()).unwrap();
        let link = reopened.provider_links("connection").unwrap().remove(0);
        assert_eq!(link.managed_model.as_deref(), Some("team-model-a"));
        let version: u32 = reopened
            .connection
            .lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn pending_absent_disk_state_survives_sqlite_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let state = TeamState::open(temp.path()).unwrap();
        state
            .save_asset_state(&ManagedAssetState {
                connection_id: "connection".into(),
                app: "codex".into(),
                asset_id: 7,
                asset_name: "Prompt".into(),
                asset_kind: Some(super::super::types::AssetKind::Prompt),
                revision: 2,
                content_hash: "a".repeat(64),
                archive_sha256: None,
                asset_files: Vec::new(),
                local_hash: "b".repeat(64),
                install_path: "C:/managed/prompt.md".into(),
                install_root: "C:/managed".into(),
                relative_path: "prompt.md".into(),
                backup_path: None,
                backup_root_path: Some("C:/managed/.nexusops-team/backups/op".into()),
                backup_created_at: None,
                install_operation: None,
                upstream_pending: true,
                upstream_operation: Some("operation".into()),
                upstream_backup_path: None,
                upstream_fingerprint: None,
                shared_upstream_fingerprint: None,
                pending_previous_upstream_fingerprint: None,
                pending_target_upstream_fingerprint: None,
                restored_unmanaged: true,
                pending_disk_state: Some(super::super::types::PendingDiskState::Absent),
                pending_limits: Some(super::super::types::fixture_profile().limits),
                subscribed: false,
                last_synced_at: None,
            })
            .unwrap();
        drop(state);
        let reopened = TeamState::open(temp.path()).unwrap();
        let restored = reopened.asset_states("connection").unwrap().remove(0);
        assert_eq!(
            restored.pending_disk_state,
            Some(super::super::types::PendingDiskState::Absent)
        );
        assert_eq!(restored.pending_limits.unwrap().max_text_bytes, 1 << 20);
    }
}
