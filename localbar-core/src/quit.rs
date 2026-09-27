use uuid::Uuid;

use crate::types::{AppSettings, ServerType};

pub const MAX_QUIT_GRACE_SECS: f64 = 60.0;
pub const QUIT_KILL_SLACK_SECS: f64 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub struct QuitCandidate {
    pub id: Uuid,
    pub has_child: bool,
    pub server_type: ServerType,
}

pub fn should_stop_on_quit(candidate: &QuitCandidate, settings: &AppSettings) -> bool {
    !settings.keep_servers_running_on_quit
        && candidate.has_child
        && candidate.server_type != ServerType::External
}

pub fn ids_to_stop_on_quit(candidates: &[QuitCandidate], settings: &AppSettings) -> Vec<Uuid> {
    candidates
        .iter()
        .filter(|c| should_stop_on_quit(c, settings))
        .map(|c| c.id)
        .collect()
}

pub fn quit_shutdown_budget_secs(grace_periods: &[f64]) -> f64 {
    let largest = grace_periods
        .iter()
        .map(|g| g.clamp(0.0, MAX_QUIT_GRACE_SECS))
        .fold(0.0, f64::max);
    largest + QUIT_KILL_SLACK_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(has_child: bool, server_type: ServerType) -> QuitCandidate {
        QuitCandidate { id: Uuid::new_v4(), has_child, server_type }
    }

    fn stop_by_default() -> AppSettings {
        AppSettings::default()
    }

    fn keep_running() -> AppSettings {
        AppSettings { keep_servers_running_on_quit: true }
    }

    #[test]
    fn quit_stops_spawned_running_server() {
        let c = candidate(true, ServerType::MlxLm);
        assert_eq!(ids_to_stop_on_quit(std::slice::from_ref(&c), &stop_by_default()), vec![c.id]);
    }

    #[test]
    fn quit_keeps_adopted_server_running() {
        let c = candidate(false, ServerType::Ollama);
        assert!(ids_to_stop_on_quit(&[c], &stop_by_default()).is_empty());
    }

    #[test]
    fn quit_keeps_external_instance_running() {
        let c = candidate(true, ServerType::External);
        assert!(ids_to_stop_on_quit(&[c], &stop_by_default()).is_empty());
    }

    #[test]
    fn quit_with_keep_running_setting_stops_nothing() {
        let c = candidate(true, ServerType::MlxLm);
        assert!(ids_to_stop_on_quit(&[c], &keep_running()).is_empty());
    }

    #[test]
    fn quit_budget_is_largest_grace_plus_slack() {
        assert_eq!(quit_shutdown_budget_secs(&[1.0, 5.0, 3.0]), 5.0 + QUIT_KILL_SLACK_SECS);
    }

    #[test]
    fn quit_budget_caps_grace_so_quit_cannot_hang() {
        assert_eq!(quit_shutdown_budget_secs(&[f64::INFINITY]), MAX_QUIT_GRACE_SECS + QUIT_KILL_SLACK_SECS);
    }
}
