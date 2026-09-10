use super::{TeamError, UsageEvent};
use chrono::{DateTime, Days, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
pub struct DailyUsage {
    pub day: String,
    pub runtime: String,
    pub event: String,
    pub count: i64,
    pub last_observed_at: String,
}

pub fn initialize(db: &Connection) -> Result<(), TeamError> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS usage_history_binding(id INTEGER PRIMARY KEY CHECK(id=1),connection_id TEXT NOT NULL,owner TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS usage_history_seen(owner TEXT NOT NULL,event_id TEXT NOT NULL,day TEXT NOT NULL,PRIMARY KEY(owner,event_id));
      CREATE TABLE IF NOT EXISTS usage_history_daily(owner TEXT NOT NULL,day TEXT NOT NULL,runtime TEXT NOT NULL,event TEXT NOT NULL,count INTEGER NOT NULL,last_observed_at TEXT NOT NULL,PRIMARY KEY(owner,day,runtime,event));
      CREATE INDEX IF NOT EXISTS usage_history_seen_day ON usage_history_seen(day);").map_err(|_| TeamError::Storage)
}

pub fn owner(connection_id: &str, member_id: i64) -> Result<String, TeamError> {
    if connection_id.is_empty() || member_id <= 0 {
        return Err(TeamError::DifferentIdentity);
    }
    let bytes = serde_json::to_vec(&(connection_id, member_id)).map_err(|_| TeamError::Storage)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn prune(db: &Connection, now: DateTime<Utc>) -> Result<(), TeamError> {
    let cutoff = now
        .date_naive()
        .checked_sub_days(Days::new(29))
        .ok_or(TeamError::Storage)?
        .to_string();
    for table in ["usage_history_seen", "usage_history_daily"] {
        db.execute(&format!("DELETE FROM {table} WHERE day<?1"), [&cutoff])
            .map_err(|_| TeamError::Storage)?;
    }
    Ok(())
}

// Called inside the same transaction as outbox insertion; ACK never touches it.
pub fn record(db: &Connection, event: &UsageEvent, now: DateTime<Utc>) -> Result<(), TeamError> {
    prune(db, now)?;
    let owner: Option<String> = db.query_row("SELECT b.owner FROM usage_history_binding b JOIN settings s ON s.id=b.id AND s.connection_id=b.connection_id WHERE s.enabled=1", [], |row| row.get(0)).optional().map_err(|_| TeamError::Storage)?;
    let Some(owner) = owner else {
        return Ok(());
    };
    let timestamp = DateTime::parse_from_rfc3339(&event.timestamp)
        .map_err(|_| TeamError::Storage)?
        .with_timezone(&Utc);
    let day = timestamp.date_naive();
    let cutoff = now
        .date_naive()
        .checked_sub_days(Days::new(29))
        .ok_or(TeamError::Storage)?;
    if day < cutoff || timestamp > now {
        return Ok(());
    }
    let inserted = db
        .execute(
            "INSERT OR IGNORE INTO usage_history_seen(owner,event_id,day) VALUES(?1,?2,?3)",
            params![owner, event.id, day.to_string()],
        )
        .map_err(|_| TeamError::Storage)?;
    if inserted == 1 {
        db.execute("INSERT INTO usage_history_daily(owner,day,runtime,event,count,last_observed_at) VALUES(?1,?2,?3,?4,1,?5) ON CONFLICT(owner,day,runtime,event) DO UPDATE SET count=count+1,last_observed_at=MAX(last_observed_at,excluded.last_observed_at)", params![owner,day.to_string(),event.runtime,event.event,timestamp.to_rfc3339()]).map_err(|_| TeamError::Storage)?;
    }
    Ok(())
}

pub fn read(
    db: &Connection,
    connection_id: &str,
    now: DateTime<Utc>,
) -> Result<Vec<DailyUsage>, TeamError> {
    prune(db, now)?;
    let mut query = db.prepare("SELECT d.day,d.runtime,d.event,d.count,d.last_observed_at FROM usage_history_daily d JOIN usage_history_binding b ON d.owner=b.owner JOIN settings s ON s.id=b.id AND s.connection_id=b.connection_id WHERE b.connection_id=?1 ORDER BY d.day,d.runtime,d.event").map_err(|_| TeamError::Storage)?;
    let rows = query
        .query_map([connection_id], |r| {
            Ok(DailyUsage {
                day: r.get(0)?,
                runtime: r.get(1)?,
                event: r.get(2)?,
                count: r.get(3)?,
                last_observed_at: r.get(4)?,
            })
        })
        .map_err(|_| TeamError::Storage)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| TeamError::Storage)
}

#[cfg(test)]
mod tests {
    use super::super::UsageStore;
    use super::*;

    #[test]
    fn history_survives_ack_disable_and_identity_switch() {
        let directory = tempfile::tempdir().unwrap();
        let store = UsageStore::open(directory.path()).unwrap();
        store.bind_history("workspace", 11).unwrap();
        store.configure("workspace", true).unwrap();
        for _ in 0..20 {
            store
                .capture("codex", &serde_json::json!({"hook_event_name":"Stop"}))
                .unwrap();
        }
        let events = store.pending().unwrap();
        store.acknowledge(&events).unwrap();
        assert_eq!(store.history("workspace").unwrap()[0].count, 20);
        store.configure("workspace", false).unwrap();
        store
            .capture("codex", &serde_json::json!({"hook_event_name":"Stop"}))
            .unwrap();
        assert_eq!(store.history("workspace").unwrap()[0].count, 20);
        store.unbind_history().unwrap();
        assert!(store.history("workspace").unwrap().is_empty());
        store.bind_history("workspace", 22).unwrap();
        assert!(store.history("workspace").unwrap().is_empty());
        store.bind_history("workspace", 11).unwrap();
        assert_eq!(store.history("workspace").unwrap()[0].count, 20);
        assert!(!store.status("workspace").unwrap().enabled);
        drop(store);
        assert_eq!(
            UsageStore::open(directory.path())
                .unwrap()
                .history("workspace")
                .unwrap()[0]
                .count,
            20
        );
    }

    #[test]
    fn repeated_ids_count_once_and_retention_uses_utc_days() {
        let directory = tempfile::tempdir().unwrap();
        let store = UsageStore::open(directory.path()).unwrap();
        store.bind_history("workspace", 11).unwrap();
        store.configure("workspace", true).unwrap();
        let now = "2026-09-11T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let db = store.db.lock().unwrap();
        for (id, timestamp) in [
            ("a", "2026-08-13T00:00:00Z"),
            ("a", "2026-08-13T00:00:00Z"),
            ("old", "2026-08-12T23:59:59Z"),
            ("b", "2026-09-11T01:00:00+08:00"),
        ] {
            record(
                &db,
                &UsageEvent {
                    id: id.into(),
                    runtime: "codex".into(),
                    event: "turn.completed".into(),
                    timestamp: timestamp.into(),
                },
                now,
            )
            .unwrap();
        }
        let rows = read(&db, "workspace", now).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].day, "2026-08-13");
        assert_eq!(rows[1].day, "2026-09-10");
        assert_eq!(rows.iter().map(|row| row.count).sum::<i64>(), 2);
        let tomorrow = now + chrono::Duration::days(1);
        assert_eq!(read(&db, "workspace", tomorrow).unwrap().len(), 1);
        let remaining: i64 = db
            .query_row("SELECT count(*) FROM usage_history_seen", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn clearing_history_is_scoped_and_does_not_delete_uploads() {
        let directory = tempfile::tempdir().unwrap();
        let store = UsageStore::open(directory.path()).unwrap();
        for member in [11, 22] {
            store.bind_history("workspace", member).unwrap();
            store.configure("workspace", true).unwrap();
            store
                .capture("codex", &serde_json::json!({"hook_event_name":"Stop"}))
                .unwrap();
        }
        store.clear_history("other-workspace").unwrap();
        assert_eq!(store.history("workspace").unwrap()[0].count, 1);
        store.clear_history("workspace").unwrap();
        assert!(store.history("workspace").unwrap().is_empty());
        assert_eq!(store.pending().unwrap().len(), 1);
        store.bind_history("workspace", 11).unwrap();
        assert_eq!(store.history("workspace").unwrap()[0].count, 1);
        assert!(!store.status("workspace").unwrap().enabled);
    }
}
