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

pub mod hooks;
#[cfg(test)]
mod http_tests;

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

    pub fn status(&self, connection_id: &str) -> Result<UsageStatus, TeamError> {
        self.db.lock().map_err(|_| TeamError::Storage)?.query_row(
            "SELECT enabled AND connection_id=?1,(SELECT count(*) FROM events),uploaded,last_uploaded_at,last_error FROM settings WHERE id=1", [connection_id],
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
        for e in events {
            tx.execute("DELETE FROM events WHERE id=?1", [&e.id])
                .map_err(|_| TeamError::Storage)?;
        }
        tx.execute("UPDATE settings SET uploaded=uploaded+?1,last_uploaded_at=?2,last_error=NULL WHERE id=1", params![events.len(), Utc::now().to_rfc3339()]).map_err(|_| TeamError::Storage)?;
        tx.commit().map_err(|_| TeamError::Storage)
    }
}

impl TeamService {
    pub async fn configure_tool_usage(&self, enabled: bool) -> Result<UsageStatus, TeamError> {
        let _operation = self.operation.lock().await;
        let connection = self.status()?.ok_or(TeamError::NotConnected)?;
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
