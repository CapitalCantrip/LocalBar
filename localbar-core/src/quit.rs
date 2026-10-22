use uuid::Uuid;

use crate::registry::InstanceRegistry;
use crate::types::{AppSettings, InstancePhase, ServerInstanceConfig, ServerType};

pub const MAX_QUIT_GRACE_SECS: f64 = 60.0;
pub const QUIT_KILL_SLACK_SECS: f64 = 2.0;
pub const MAX_STARTUP_LAUNCHES_WITHOUT_ASKING: usize = 1;

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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StartupRestore {
    pub launch_now: Vec<Uuid>,
    pub ask_user: Vec<Uuid>,
}

pub fn startup_restore_for(configs: &[ServerInstanceConfig], settings: &AppSettings) -> StartupRestore {
    let ids = ids_to_start_on_launch(configs, settings);
    if ids.len() > MAX_STARTUP_LAUNCHES_WITHOUT_ASKING {
        StartupRestore { launch_now: Vec::new(), ask_user: ids }
    } else {
        StartupRestore { launch_now: ids, ask_user: Vec::new() }
    }
}

pub fn decline_startup_restore(reg: &mut InstanceRegistry, ids: &[Uuid]) {
    for &id in ids {
        if reg.get_phase(id).is_some_and(InstancePhase::is_active) {
            continue;
        }
        reg.set_phase(id, InstancePhase::Stopped).ok();
        reg.update_config(id, |c| c.was_running_when_quit = false).ok();
    }
    reg.save().ok();
}

pub fn ids_to_forget_reconnect_on_launch(
    configs: &[ServerInstanceConfig],
    settings: &AppSettings,
) -> Vec<Uuid> {
    configs
        .iter()
        .filter(|c| c.was_running_when_quit && !should_start_on_launch(c, settings))
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
    use crate::persistence::InMemoryPersistence;
    use crate::types::{InstanceError, InstanceErrorKind};

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
    fn unrestored_reconnect_flag_is_forgotten_so_a_later_restore_does_not_revive_it() {
        let c = launch_config(false, true);
        let settings = AppSettings { restore_running_servers_on_launch: false, ..AppSettings::default() };
        assert_eq!(ids_to_forget_reconnect_on_launch(std::slice::from_ref(&c), &settings), vec![c.id]);
    }

    #[test]
    fn restored_reconnect_flag_is_kept_until_stop() {
        let c = launch_config(false, true);
        assert!(ids_to_forget_reconnect_on_launch(&[c], &AppSettings::default()).is_empty());
    }

    #[test]
    fn neither_flag_does_not_start() {
        let c = launch_config(false, false);
        assert!(!should_start_on_launch(&c, &AppSettings::default()));
    }

    fn registry_with(configs: &[ServerInstanceConfig]) -> InstanceRegistry {
        let mut reg = InstanceRegistry::new(Box::new(InMemoryPersistence::default()));
        for c in configs {
            reg.add_instance(c.clone());
        }
        reg
    }

    #[test]
    fn single_startup_candidate_launches_without_asking() {
        let c = launch_config(false, true);
        let restore = startup_restore_for(std::slice::from_ref(&c), &AppSettings::default());
        assert_eq!(restore, StartupRestore { launch_now: vec![c.id], ask_user: vec![] });
    }

    #[test]
    fn several_startup_candidates_are_all_held_for_the_user_to_choose() {
        let a = launch_config(true, false);
        let b = launch_config(false, true);
        let restore = startup_restore_for(&[a.clone(), b.clone()], &AppSettings::default());
        assert_eq!(restore, StartupRestore { launch_now: vec![], ask_user: vec![a.id, b.id] });
    }

    #[test]
    fn instances_not_restored_by_setting_do_not_count_towards_asking() {
        let a = launch_config(true, false);
        let b = launch_config(false, true);
        let settings = AppSettings { restore_running_servers_on_launch: false, ..AppSettings::default() };
        let restore = startup_restore_for(&[a.clone(), b], &settings);
        assert_eq!(restore, StartupRestore { launch_now: vec![a.id], ask_user: vec![] });
    }

    #[test]
    fn declined_candidate_forgets_reconnect_flag_so_next_launch_does_not_offer_it_again() {
        let c = launch_config(false, true);
        let mut reg = registry_with(std::slice::from_ref(&c));
        decline_startup_restore(&mut reg, &[c.id]);
        let configs: Vec<_> = reg.all_configs().cloned().collect();
        assert!(!configs[0].was_running_when_quit);
        assert!(ids_to_start_on_launch(&configs, &AppSettings::default()).is_empty());
    }

    #[test]
    fn declined_candidate_is_marked_stopped() {
        let c = launch_config(false, true);
        let mut reg = registry_with(std::slice::from_ref(&c));
        reg.set_phase(c.id, InstancePhase::Error(InstanceError {
            kind: InstanceErrorKind::Unexpected,
            message: "x".into(),
        })).unwrap();
        decline_startup_restore(&mut reg, &[c.id]);
        assert_eq!(reg.get_phase(c.id), Some(&InstancePhase::Stopped));
    }

    #[test]
    fn declined_candidate_keeps_its_start_on_launch_setting() {
        let c = launch_config(true, true);
        let mut reg = registry_with(std::slice::from_ref(&c));
        decline_startup_restore(&mut reg, &[c.id]);
        assert!(reg.get_config(c.id).unwrap().start_on_launch);
    }

    #[test]
    fn declining_a_candidate_the_user_already_started_leaves_it_running() {
        let c = launch_config(false, true);
        let mut reg = registry_with(std::slice::from_ref(&c));
        reg.set_phase(c.id, InstancePhase::Running).unwrap();
        decline_startup_restore(&mut reg, &[c.id]);
        assert_eq!(reg.get_phase(c.id), Some(&InstancePhase::Running));
        assert!(reg.get_config(c.id).unwrap().was_running_when_quit);
    }

    #[test]
    fn declining_leaves_other_instances_untouched() {
        let declined = launch_config(false, true);
        let other = launch_config(false, true);
        let mut reg = registry_with(&[declined.clone(), other.clone()]);
        decline_startup_restore(&mut reg, &[declined.id]);
        assert!(reg.get_config(other.id).unwrap().was_running_when_quit);
    }

    #[test]
    fn legacy_state_without_restore_key_loads_with_restore_true() {
        let settings: AppSettings = serde_json::from_str(r#"{"keep_servers_running_on_quit":true}"#).unwrap();
        assert!(settings.restore_running_servers_on_launch);
    }
}
