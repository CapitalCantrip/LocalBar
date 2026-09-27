use uuid::Uuid;

use crate::types::{AppSettings, ServerInstanceConfig, ServerType};

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

pub fn should_start_on_launch(config: &ServerInstanceConfig, settings: &AppSettings) -> bool {
    config.start_on_launch
        || (config.was_running_when_quit && settings.restore_running_servers_on_launch)
}

pub fn ids_to_start_on_launch(
    configs: &[ServerInstanceConfig],
    settings: &AppSettings,
) -> Vec<Uuid> {
    configs
        .iter()
        .filter(|c| should_start_on_launch(c, settings))
        .map(|c| c.id)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitSignal {
    Requested,
    Terminating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitSequence {
    AlreadyRan,
    InBackground,
    Inline,
}

pub fn quit_sequence_for(signal: ExitSignal, already_quitting: bool) -> QuitSequence {
    match (already_quitting, signal) {
        (true, _) => QuitSequence::AlreadyRan,
        (false, ExitSignal::Requested) => QuitSequence::InBackground,
        (false, ExitSignal::Terminating) => QuitSequence::Inline,
    }
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
        AppSettings { keep_servers_running_on_quit: true, ..AppSettings::default() }
    }

    fn launch_config(start_on_launch: bool, was_running_when_quit: bool) -> ServerInstanceConfig {
        let mut c = ServerInstanceConfig::new("test", ServerType::MlxLm, 8080, "/bin/mlx");
        c.start_on_launch = start_on_launch;
        c.was_running_when_quit = was_running_when_quit;
        c
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
    fn exit_request_runs_quit_sequence_in_background_so_exit_can_wait() {
        assert_eq!(quit_sequence_for(ExitSignal::Requested, false), QuitSequence::InBackground);
    }

    #[test]
    fn termination_without_exit_request_still_runs_quit_sequence_inline() {
        assert_eq!(quit_sequence_for(ExitSignal::Terminating, false), QuitSequence::Inline);
    }

    #[test]
    fn quit_sequence_runs_once_even_when_both_exit_signals_arrive() {
        assert_eq!(quit_sequence_for(ExitSignal::Requested, true), QuitSequence::AlreadyRan);
        assert_eq!(quit_sequence_for(ExitSignal::Terminating, true), QuitSequence::AlreadyRan);
    }

    #[test]
    fn quit_budget_is_largest_grace_plus_slack() {
        assert_eq!(quit_shutdown_budget_secs(&[1.0, 5.0, 3.0]), 5.0 + QUIT_KILL_SLACK_SECS);
    }

    #[test]
    fn quit_budget_caps_grace_so_quit_cannot_hang() {
        assert_eq!(quit_shutdown_budget_secs(&[f64::INFINITY]), MAX_QUIT_GRACE_SECS + QUIT_KILL_SLACK_SECS);
    }

    #[test]
    fn start_on_launch_always_starts() {
        let c = launch_config(true, false);
        assert!(should_start_on_launch(&c, &stop_by_default()));
        let settings = AppSettings { restore_running_servers_on_launch: false, ..AppSettings::default() };
        assert!(should_start_on_launch(&c, &settings));
    }

    #[test]
    fn was_running_and_restore_on_starts() {
        let c = launch_config(false, true);
        assert!(should_start_on_launch(&c, &AppSettings::default()));
    }

    #[test]
    fn was_running_and_restore_off_does_not_start() {
        let c = launch_config(false, true);
        let settings = AppSettings { restore_running_servers_on_launch: false, ..AppSettings::default() };
        assert!(!should_start_on_launch(&c, &settings));
    }

    #[test]
    fn neither_flag_does_not_start() {
        let c = launch_config(false, false);
        assert!(!should_start_on_launch(&c, &AppSettings::default()));
    }

    #[test]
    fn legacy_state_without_restore_key_loads_with_restore_true() {
        let settings: AppSettings = serde_json::from_str(r#"{"keep_servers_running_on_quit":true}"#).unwrap();
        assert!(settings.restore_running_servers_on_launch);
    }
}
