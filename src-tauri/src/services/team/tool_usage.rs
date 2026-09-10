//! Native subset of SkillOps lifecycle collection (MIT, Gjts).
//! Only the typed metadata below can cross the local queue / network boundary.
use super::{
    api::{Cancellation, TeamApi},
    types::TeamError,
    TeamService,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

#[cfg(test)]
mod cli_tests;
pub mod history;
pub mod hooks;
#[cfg(test)]
mod http_tests;
pub mod observation;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UsageEvent {
    pub id: String,
    pub runtime: String,
    pub event: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct UsageStatus {
    pub enabled: bool,
    pub pending: i64,
    pub uploaded: i64,
    pub last_uploaded_at: Option<String>,
    pub last_error: Option<String>,
}

pub struct UsageStore {
    db: Mutex<Connection>,
    pub directory: PathBuf,
}

impl UsageStore {
    pub fn open(directory: &Path) -> Result<Self, TeamError> {
        std::fs::create_dir_all(directory).map_err(|_| TeamError::Storage)?;
        let db =
            Connection::open(directory.join("tool-usage.db")).map_err(|_| TeamError::Storage)?;
        db.busy_timeout(Duration::from_millis(500))
            .map_err(|_| TeamError::Storage)?;
        db.execute_batch("PRAGMA journal_mode=WAL;
          CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), connection_id TEXT NOT NULL DEFAULT '', enabled INTEGER NOT NULL DEFAULT 0, uploaded INTEGER NOT NULL DEFAULT 0, last_uploaded_at TEXT, last_error TEXT);
          INSERT OR IGNORE INTO settings(id) VALUES(1);
          CREATE TABLE IF NOT EXISTS events(id TEXT PRIMARY KEY, body TEXT NOT NULL, created_at TEXT NOT NULL);").map_err(|_| TeamError::Storage)?;
        history::initialize(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            directory: directory.to_path_buf(),
        })
    }

    pub fn configure(&self, connection_id: &str, enabled: bool) -> Result<(), TeamError> {
        let mut db = self.db.lock().map_err(|_| TeamError::Storage)?;
        let tx = db.transaction().map_err(|_| TeamError::Storage)?;
        let old: String = tx
            .query_row("SELECT connection_id FROM settings WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(|_| TeamError::Storage)?;
        if !enabled || old != connection_id {
            tx.execute("DELETE FROM events", [])
                .map_err(|_| TeamError::Storage)?;
        }
        if old != connection_id {
            tx.execute(
                "UPDATE settings SET uploaded=0,last_uploaded_at=NULL WHERE id=1",
                [],
            )
            .map_err(|_| TeamError::Storage)?;
        }
        tx.execute(
            "UPDATE settings SET connection_id=?1,enabled=?2,last_error=NULL WHERE id=1",
            params![connection_id, enabled],
        )
        .map_err(|_| TeamError::Storage)?;
        tx.commit().map_err(|_| TeamError::Storage)
    }

    pub fn bind_history(&self, connection_id: &str, member_id: i64) -> Result<(), TeamError> {
        let owner = history::owner(connection_id, member_id)?;
        let mut db = self.db.lock().map_err(|_| TeamError::Storage)?;
        let tx = db.transaction().map_err(|_| TeamError::Storage)?;
        tx.execute("UPDATE settings SET enabled=0,uploaded=0,last_uploaded_at=NULL,last_error=NULL WHERE NOT EXISTS(SELECT 1 FROM usage_history_binding WHERE id=1 AND owner=?1)", [&owner]).map_err(|_| TeamError::Storage)?;
        tx.execute("DELETE FROM events WHERE NOT EXISTS(SELECT 1 FROM usage_history_binding WHERE id=1 AND owner=?1)", [&owner]).map_err(|_| TeamError::Storage)?;
        tx.execute("INSERT INTO usage_history_binding(id,connection_id,owner) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET connection_id=excluded.connection_id,owner=excluded.owner", params![connection_id,owner]).map_err(|_| TeamError::Storage)?;
        tx.commit().map_err(|_| TeamError::Storage)
    }

    pub fn unbind_history(&self) -> Result<(), TeamError> {
        let mut db = self.db.lock().map_err(|_| TeamError::Storage)?;
        let tx = db.transaction().map_err(|_| TeamError::Storage)?;
        tx.execute_batch("UPDATE settings SET enabled=0,uploaded=0,last_uploaded_at=NULL,last_error=NULL WHERE id=1; DELETE FROM events; DELETE FROM usage_history_binding;").map_err(|_| TeamError::Storage)?;
        tx.commit().map_err(|_| TeamError::Storage)
    }

    pub fn history(&self, connection_id: &str) -> Result<Vec<history::DailyUsage>, TeamError> {
        let db = self.db.lock().map_err(|_| TeamError::Storage)?;
        history::read(&db, connection_id, Utc::now())
    }

    pub fn clear_history(&self, connection_id: &str) -> Result<(), TeamError> {
        let mut db = self.db.lock().map_err(|_| TeamError::Storage)?;
        let tx = db.transaction().map_err(|_| TeamError::Storage)?;
        for table in ["usage_history_daily", "usage_history_seen"] {
            tx.execute(&format!("DELETE FROM {table} WHERE owner IN(SELECT b.owner FROM usage_history_binding b JOIN settings s ON b.id=s.id AND b.connection_id=s.connection_id WHERE b.connection_id=?1)"), [connection_id]).map_err(|_| TeamError::Storage)?;
        }
        tx.commit().map_err(|_| TeamError::Storage)
    }

    pub fn status(&self, connection_id: &str) -> Result<UsageStatus, TeamError> {
        self.db.lock().map_err(|_| TeamError::Storage)?.query_row(
            "SELECT enabled AND connection_id=?1,CASE WHEN connection_id=?1 THEN (SELECT count(*) FROM events) ELSE 0 END,CASE WHEN connection_id=?1 THEN uploaded ELSE 0 END,CASE WHEN connection_id=?1 THEN last_uploaded_at END,CASE WHEN connection_id=?1 THEN last_error END FROM settings WHERE id=1", [connection_id],
            |r| Ok(UsageStatus { enabled: r.get(0)?, pending: r.get(1)?, uploaded: r.get(2)?, last_uploaded_at: r.get(3)?, last_error: r.get(4)? })
        ).map_err(|_| TeamError::Storage)
    }

    pub fn bind_connection(&self, connection_id: &str) -> Result<(), TeamError> {
        let old: String = self
            .db
            .lock()
            .map_err(|_| TeamError::Storage)?
            .query_row("SELECT connection_id FROM settings WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(|_| TeamError::Storage)?;
        if old != connection_id {
            self.configure(connection_id, false)?;
        }
        Ok(())
    }

    pub fn capture(&self, runtime: &str, input: &Value) -> Result<(), TeamError> {
        if !matches!(runtime, "codex" | "claude-code") {
            return Ok(());
        }
        // Match SkillOps native lifecycle hooks, never inventory/discovery.
        let Some(event) = input
            .get("hook_event_name")
            .and_then(Value::as_str)
            .and_then(|name| match name {
                "SessionStart" => Some("session.started"),
                "SessionEnd" => Some("session.completed"),
                "UserPromptSubmit" => Some("prompt.submitted"),
                "Stop" | "StopFailure" => Some("turn.completed"),
                "PostToolUse" | "PostToolUseFailure" => Some("tool.completed"),
                _ => None,
            })
        else {
            return Ok(());
        };
        let mut guard = self.db.lock().map_err(|_| TeamError::Storage)?;
        let db = guard
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| TeamError::Storage)?;
        let enabled: bool = db
            .query_row("SELECT enabled FROM settings WHERE id=1", [], |r| r.get(0))
            .map_err(|_| TeamError::Storage)?;
        if !enabled {
            return Ok(());
        }
        let count: i64 = db
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
            .map_err(|_| TeamError::Storage)?;
        if count >= 10000 {
            db.execute("UPDATE settings SET last_error='queue_full' WHERE id=1", [])
                .map_err(|_| TeamError::Storage)?;
            return db.commit().map_err(|_| TeamError::Storage);
        }
        let id = format!("{:x}", Sha256::digest(uuid::Uuid::new_v4().as_bytes()));
        let usage = UsageEvent {
            id,
            runtime: runtime.into(),
            event: event.into(),
            timestamp: Utc::now().to_rfc3339(),
        };
        let body = serde_json::to_string(&usage).map_err(|_| TeamError::Storage)?;
        db.execute(
            "INSERT INTO events(id,body,created_at) VALUES(?1,?2,?3)",
            params![usage.id, body, usage.timestamp],
        )
        .map_err(|_| TeamError::Storage)?;
        history::record(&db, &usage, Utc::now())?;
        db.commit().map_err(|_| TeamError::Storage)
    }

    fn pending(&self) -> Result<Vec<UsageEvent>, TeamError> {
        let db = self.db.lock().map_err(|_| TeamError::Storage)?;
        // Match the server retry window; expired events must not poison later batches.
        db.execute("DELETE FROM events WHERE julianday(created_at)<julianday('now','-30 days','+5 minutes')", []).map_err(|_| TeamError::Storage)?;
        let mut query = db
            .prepare("SELECT body FROM events ORDER BY created_at,id LIMIT 200")
            .map_err(|_| TeamError::Storage)?;
        let rows = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| TeamError::Storage)?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|_| TeamError::Storage)?)
                .map_err(|_| TeamError::Storage)
        })
        .collect()
    }

    fn acknowledge(&self, events: &[UsageEvent]) -> Result<(), TeamError> {
        let mut db = self.db.lock().map_err(|_| TeamError::Storage)?;
        let tx = db.transaction().map_err(|_| TeamError::Storage)?;
        let mut acknowledged = 0;
        for e in events {
            acknowledged += tx
                .execute("DELETE FROM events WHERE id=?1", [&e.id])
                .map_err(|_| TeamError::Storage)?;
        }
        if acknowledged > 0 {
            tx.execute("UPDATE settings SET uploaded=uploaded+?1,last_uploaded_at=?2,last_error=NULL WHERE id=1", params![acknowledged, Utc::now().to_rfc3339()]).map_err(|_| TeamError::Storage)?;
        }
        tx.commit().map_err(|_| TeamError::Storage)
    }
}

impl TeamService {
    pub async fn tool_usage_history(&self) -> Result<Vec<history::DailyUsage>, TeamError> {
        let _operation = self.operation.lock().await;
        let connection = self.status()?.ok_or(TeamError::NotConnected)?;
        self.tool_usage.history(&connection.id)
    }

    pub async fn clear_tool_usage_history(&self) -> Result<(), TeamError> {
        let _operation = self.operation.lock().await;
        let connection = self.status()?.ok_or(TeamError::NotConnected)?;
        self.tool_usage.clear_history(&connection.id)
    }

    pub async fn configure_tool_usage(&self, enabled: bool) -> Result<UsageStatus, TeamError> {
        let _operation = self.operation.lock().await;
        let connection = self.status()?.ok_or(TeamError::NotConnected)?;
        if enabled {
            let key = self.member_key()?;
            let project = TeamApi::new(&connection.gateway_url)?
                .teamai_project(&key, &connection.profile, &Cancellation::default())
                .await?;
            self.tool_usage
                .bind_history(&connection.id, project.member_id)?;
        }
        // Disable collection before removing hooks, including when a config is unreadable.
        if !enabled {
            self.tool_usage.configure(&connection.id, false)?;
        }
        hooks::install(&self.tool_usage.directory, enabled)?;
        self.tool_usage.configure(&connection.id, enabled)?;
        self.tool_usage.status(&connection.id)
    }

    pub async fn upload_tool_usage(&self) -> Result<UsageStatus, TeamError> {
        let _operation = self.operation.lock().await;
        let connection = self.status()?.ok_or(TeamError::NotConnected)?;
        if !self.tool_usage.status(&connection.id)?.enabled {
            return self.tool_usage.status(&connection.id);
        }
        let events = self.tool_usage.pending()?;
        if !events.is_empty() {
            let key = self.member_key()?;
            let result = TeamApi::new(&connection.gateway_url)?
                .upload_tool_usage(&key, &events, &Cancellation::default())
                .await;
            if let Err(error) = result {
                self.tool_usage
                    .db
                    .lock()
                    .map_err(|_| TeamError::Storage)?
                    .execute(
                        "UPDATE settings SET last_error=?1 WHERE id=1",
                        [error.code()],
                    )
                    .map_err(|_| TeamError::Storage)?;
                return Err(error);
            }
            self.tool_usage.acknowledge(&events)?;
        }
        self.tool_usage.status(&connection.id)
    }
}

/// Runs before GUI/logging/single-instance startup. Hook failures never block the runtime.
pub fn run_hook(args: &[String]) -> bool {
    if args.get(1).map(String::as_str) != Some("--nexusops-skillops-hook") {
        return false;
    }
    if let (Some(runtime), Some(directory)) = (args.get(2), args.get(3)) {
        let mut bytes = Vec::new();
        if std::io::stdin()
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_ok()
            && bytes.len() <= 1024 * 1024
        {
            if let (Ok(input), Ok(store)) = (
                serde_json::from_slice::<Value>(&bytes),
                UsageStore::open(Path::new(directory)),
            ) {
                let _ = store.capture(runtime, &input);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_acknowledgements_count_only_deleted_events() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::open(dir.path()).unwrap();
        store.configure("connection-a", true).unwrap();
        for _ in 0..20 {
            store
                .capture("codex", &serde_json::json!({"hook_event_name":"Stop"}))
                .unwrap();
        }
        let batch = store.pending().unwrap();
        store.acknowledge(&batch[..5]).unwrap();
        store.acknowledge(&batch).unwrap();
        let status = store.status("connection-a").unwrap();
        assert_eq!(status.uploaded, 20);
        assert_eq!(status.pending, 0);
        store.acknowledge(&batch).unwrap();
        let repeated = store.status("connection-a").unwrap();
        assert_eq!(repeated.uploaded, 20);
        assert_eq!(repeated.last_uploaded_at, status.last_uploaded_at);
        drop(store);
        assert_eq!(
            UsageStore::open(dir.path())
                .unwrap()
                .status("connection-a")
                .unwrap()
                .uploaded,
            20
        );
    }

    #[test]
    fn status_for_another_connection_does_not_expose_queue_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::open(dir.path()).unwrap();
        store.configure("connection-a", true).unwrap();
        store
            .capture("codex", &serde_json::json!({"hook_event_name":"Stop"}))
            .unwrap();
        store.acknowledge(&store.pending().unwrap()).unwrap();
        store
            .capture("codex", &serde_json::json!({"hook_event_name":"Stop"}))
            .unwrap();
        store
            .db
            .lock()
            .unwrap()
            .execute("UPDATE settings SET last_error='test_error' WHERE id=1", [])
            .unwrap();
        let other = store.status("connection-b").unwrap();
        assert!(!other.enabled);
        assert_eq!(other.pending, 0);
        assert_eq!(other.uploaded, 0);
        assert!(other.last_uploaded_at.is_none());
        assert!(other.last_error.is_none());
        let own = store.status("connection-a").unwrap();
        assert_eq!(own.pending, 1);
        assert_eq!(own.uploaded, 1);
        assert_eq!(own.last_error.as_deref(), Some("test_error"));
    }
    #[test]
    fn capture_is_opt_in_minimal_and_disconnect_drops_pending() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::open(dir.path()).unwrap();
        let input = serde_json::json!({"hook_event_name":"UserPromptSubmit","prompt":"PRIVATE_CODE","session_id":"PRIVATE_ID","cwd":"PRIVATE_PATH"});
        store.capture("codex", &input).unwrap();
        assert!(store.pending().unwrap().is_empty());
        store.configure("employee-one", true).unwrap();
        store.capture("codex", &input).unwrap();
        store
            .capture(
                "claude-code",
                &serde_json::json!({"hook_event_name":"SessionStart"}),
            )
            .unwrap();
        store.capture("cursor", &input).unwrap();
        store
            .capture(
                "codex",
                &serde_json::json!({"hook_event_name":"skill.discovered"}),
            )
            .unwrap();
        let batch = store.pending().unwrap();
        assert_eq!(batch.len(), 2);
        let serialized = serde_json::to_string(&batch).unwrap();
        assert!(!serialized.contains("PRIVATE"));
        assert_eq!(batch, store.pending().unwrap(), "retry keeps exact IDs");
        store.acknowledge(&batch).unwrap();
        assert!(store.pending().unwrap().is_empty());
        assert_eq!(store.status("employee-one").unwrap().uploaded, 2);
        store.capture("codex", &input).unwrap();
        store.configure("employee-one", false).unwrap();
        assert!(store.pending().unwrap().is_empty());
        store.configure("employee-two", true).unwrap();
        assert!(!store.status("employee-one").unwrap().enabled);
        assert_eq!(store.status("employee-two").unwrap().uploaded, 0);
    }
}
