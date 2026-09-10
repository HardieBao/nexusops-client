use super::{
    history::DailyUsage,
    hooks::{self, Registration},
    TeamError, TeamService,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    Unobserved,
    Observed,
    Stale,
}

#[derive(Debug, Serialize)]
pub struct ToolObservation {
    pub runtime: String,
    pub registration: Registration,
    pub collection_enabled: bool,
    pub observation: Observation,
    pub last_observed_at: Option<String>,
}

fn observation(
    rows: &[DailyUsage],
    runtime: &str,
    now: DateTime<Utc>,
) -> (Observation, Option<String>) {
    let last = rows
        .iter()
        .filter(|row| row.runtime == runtime)
        .filter_map(|row| DateTime::parse_from_rfc3339(&row.last_observed_at).ok())
        .map(|time| time.with_timezone(&Utc))
        .filter(|time| *time <= now)
        .max();
    match last {
        Some(time) => (
            if now.signed_duration_since(time) > Duration::hours(24) {
                Observation::Stale
            } else {
                Observation::Observed
            },
            Some(time.to_rfc3339()),
        ),
        None => (Observation::Unobserved, None),
    }
}

impl TeamService {
    pub async fn tool_observation(&self) -> Result<Vec<ToolObservation>, TeamError> {
        let _operation = self.operation.lock().await;
        let connection = self.status()?.ok_or(TeamError::NotConnected)?;
        let enabled = self.tool_usage.status(&connection.id)?.enabled;
        let rows = self.tool_usage.history(&connection.id)?;
        let now = Utc::now();
        Ok(hooks::inspect(&self.tool_usage.directory)?
            .into_iter()
            .map(|(runtime, registration)| {
                let (observation, last_observed_at) = observation(&rows, runtime, now);
                ToolObservation {
                    runtime: runtime.into(),
                    registration,
                    collection_enabled: enabled,
                    observation,
                    last_observed_at,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observation_is_per_runtime_and_expires_without_claiming_tool_failure() {
        let now = "2026-09-11T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let mut rows = vec![DailyUsage {
            day: "2026-09-10".into(),
            runtime: "codex".into(),
            event: "turn.completed".into(),
            count: 1,
            last_observed_at: "2026-09-10T12:00:00Z".into(),
        }];
        assert_eq!(observation(&rows, "codex", now).0, Observation::Observed);
        assert_eq!(
            observation(&rows, "claude-code", now),
            (Observation::Unobserved, None)
        );
        assert_eq!(
            observation(&rows, "codex", now + Duration::seconds(1)).0,
            Observation::Stale
        );
        rows[0].last_observed_at = "2026-09-12T12:00:00Z".into();
        assert_eq!(
            observation(&rows, "codex", now),
            (Observation::Unobserved, None)
        );
    }
}
