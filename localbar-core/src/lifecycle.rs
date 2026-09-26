use std::time::Duration;

use uuid::Uuid;

use crate::driver::{HealthStatus, LaunchPlan, ServerDriver};
use crate::net::port_is_open;
use crate::registry::InstanceRegistry;
use crate::types::{InstanceError, InstanceErrorKind, InstancePhase, ModelMemoryKey, ModelRef, ParamValues, ServerType};

#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    PhaseChanged(Uuid, InstancePhase),
}

#[derive(Debug, Clone)]
pub struct StopPlan {
    pub grace_secs: f64,
}

#[derive(Debug, Clone)]
pub enum PollContext {
    Startup,
    ModelSwitch { old_key: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PollOutcome {
    Continue,
    Done,
}

pub fn start(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
) -> (Option<LaunchPlan>, Vec<LifecycleEvent>) {
    let Some(config) = reg.get_config(id).cloned() else {
        return (None, vec![]);
    };
    if !start_should_proceed(reg, id, &config) {
        return (None, vec![]);
    }

    reg.record_start_time(id);
    let mut events = phase_events(reg, id, InstancePhase::Starting);

    let (plan, rest) = start_from_stopped_config(reg, id, driver, &config);
    events.extend(rest);
    (plan, events)
}

fn start_should_proceed(reg: &InstanceRegistry, id: Uuid, config: &crate::types::ServerInstanceConfig) -> bool {
    match reg.get_phase(id) {
        Some(InstancePhase::Starting | InstancePhase::Stopping) => false,
        Some(InstancePhase::Running) => config.server_type == ServerType::External,
        _ => true,
    }
}

fn start_from_stopped_config(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    config: &crate::types::ServerInstanceConfig,
) -> (Option<LaunchPlan>, Vec<LifecycleEvent>) {
    let health = driver.health_check(config);

    if health == HealthStatus::Healthy {
        return (None, phase_events(reg, id, InstancePhase::Running));
    }

    if port_is_open(&config.host, config.port) {
        return (None, error_events(
            reg, id,
            InstanceErrorKind::PortConflict { port: config.port },
            &format!("port {} is in use by another process", config.port),
        ));
    }

    if !driver.manages_lifecycle() {
        return (None, error_events(
            reg, id,
            InstanceErrorKind::HealthCheckFailed,
            &format!("no server detected at {}:{}", config.host, config.port),
        ));
    }

    match driver.launch(config, None, &config.instance_params) {
        Ok(plan) => (Some(plan), vec![]),
        Err(e) => (None, error_events(reg, id, InstanceErrorKind::LaunchFailed, &e)),
    }
}

pub fn stop(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
) -> (StopPlan, Vec<LifecycleEvent>) {
    let grace_secs = reg
        .get_config(id)
        .map(|c| driver.stop(c).grace_period_secs)
        .unwrap_or(0.0);
    let events = phase_events(reg, id, InstancePhase::Stopping);
    (StopPlan { grace_secs }, events)
}

#[derive(Debug, Clone)]
pub enum SwitchPlan {
    KeyOnly,
    Restart { plan: LaunchPlan, old_key: Option<String> },
    WarmLoad { model: ModelRef, old_key: Option<String> },
    Failed { message: String },
}

pub fn switch_model(
    reg: &mut InstanceRegistry,
    id: Uuid,
    model_key: &str,
    driver: &dyn ServerDriver,
) -> (SwitchPlan, Vec<LifecycleEvent>) {
    let old_key = reg.get_config(id).and_then(|c| c.selected_model_key.clone());

    if let Err(e) = crate::select_model(reg, id, model_key, driver) {
        return (SwitchPlan::Failed { message: e.clone() }, error_events(reg, id, InstanceErrorKind::ModelSwitchFailed, &e));
    }

    if !matches!(reg.get_phase(id), Some(InstancePhase::Running)) {
        return (SwitchPlan::KeyOnly, vec![]);
    }

    reg.record_start_time(id);
    let events = phase_events(reg, id, InstancePhase::SwitchingModel);

    let Some(config) = reg.get_config(id).cloned() else { return (SwitchPlan::KeyOnly, events) };

    if driver.switch_requires_restart() {
        switch_model_restart_plan(reg, id, driver, &config, old_key, events)
    } else {
        switch_model_warm_load_plan(&config, old_key, events)
    }
}

fn switch_model_restart_plan(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    config: &crate::types::ServerInstanceConfig,
    old_key: Option<String>,
    mut events: Vec<LifecycleEvent>,
) -> (SwitchPlan, Vec<LifecycleEvent>) {
    match driver.launch(config, None, &config.instance_params) {
        Ok(plan) => (SwitchPlan::Restart { plan, old_key }, events),
        Err(e) => {
            reg.consume_start_time(id);
            restore_old_key(reg, id, &old_key);
            events.extend(error_events(reg, id, InstanceErrorKind::ModelSwitchFailed, &e));
            (SwitchPlan::Failed { message: e }, events)
        }
    }
}

fn switch_model_warm_load_plan(
    config: &crate::types::ServerInstanceConfig,
    old_key: Option<String>,
    events: Vec<LifecycleEvent>,
) -> (SwitchPlan, Vec<LifecycleEvent>) {
    let key = config.selected_model_key.clone().unwrap_or_default();
    let model = ModelRef { key: key.clone(), display_name: key, publisher: None, architecture: None, size_bytes: None, modified_secs: None };
    (SwitchPlan::WarmLoad { model, old_key }, events)
}

fn restore_old_key(reg: &mut InstanceRegistry, id: Uuid, old_key: &Option<String>) {
    reg.update_config(id, |c| c.selected_model_key = old_key.clone()).ok();
    reg.save().ok();
}

pub fn finish_warm_load(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    result: Result<(), String>,
    old_key: Option<String>,
) -> Vec<LifecycleEvent> {
    match result {
        Ok(()) => finish_warm_load_ok(reg, id, driver),
        Err(e) => finish_warm_load_err(reg, id, &old_key, &e),
    }
}

fn finish_warm_load_ok(reg: &mut InstanceRegistry, id: Uuid, driver: &dyn ServerDriver) -> Vec<LifecycleEvent> {
    let elapsed = reg.consume_start_time(id).map(|t| t.elapsed()).unwrap_or_default();
    persist_startup_metrics(reg, id, driver, elapsed);
    phase_events(reg, id, InstancePhase::Running)
}

fn finish_warm_load_err(reg: &mut InstanceRegistry, id: Uuid, old_key: &Option<String>, msg: &str) -> Vec<LifecycleEvent> {
    restore_old_key(reg, id, old_key);
    reg.consume_start_time(id);
    error_events(reg, id, InstanceErrorKind::ModelSwitchFailed, msg)
}

pub fn adopt(reg: &mut InstanceRegistry, id: Uuid) -> Vec<LifecycleEvent> {
    phase_events(reg, id, InstancePhase::Running)
}

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn poll_once(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    health: bool,
    process_alive: bool,
    since_phase_entered: Duration,
    ctx: PollContext,
) -> (PollOutcome, Vec<LifecycleEvent>) {
    let phase = reg.get_phase(id).cloned();
    if !matches!(phase, Some(InstancePhase::Starting) | Some(InstancePhase::SwitchingModel)) {
        return (PollOutcome::Done, vec![]);
    }

    if !process_alive {
        return done_with_error(
            reg, id, &ctx,
            InstanceErrorKind::LaunchFailed, "process exited unexpectedly",
            InstanceErrorKind::ModelSwitchFailed, "process exited unexpectedly during model switch",
        );
    }

    if health {
        persist_startup_metrics(reg, id, driver, since_phase_entered);
        reg.consume_start_time(id);
        return (PollOutcome::Done, phase_events(reg, id, InstancePhase::Running));
    }

    if since_phase_entered >= STARTUP_TIMEOUT {
        return done_with_error(
            reg, id, &ctx,
            InstanceErrorKind::HealthCheckFailed, "startup timed out after 30 s",
            InstanceErrorKind::HealthCheckFailed, "model switch timed out after 30 s",
        );
    }

    (PollOutcome::Continue, vec![])
}

pub fn poll_adopted(reg: &mut InstanceRegistry, id: Uuid, health: bool) -> Vec<LifecycleEvent> {
    let phase = reg.get_phase(id).cloned();
    match (health, phase) {
        (false, Some(InstancePhase::Running)) => error_events(
            reg, id,
            InstanceErrorKind::HealthCheckFailed,
            "server is no longer reachable",
        ),
        (true, Some(InstancePhase::Error(_))) => phase_events(reg, id, InstancePhase::Running),
        _ => vec![],
    }
}

fn phase_events(reg: &mut InstanceRegistry, id: Uuid, phase: InstancePhase) -> Vec<LifecycleEvent> {
    reg.set_phase(id, phase.clone()).ok();
    vec![LifecycleEvent::PhaseChanged(id, phase)]
}

fn error_events(
    reg: &mut InstanceRegistry,
    id: Uuid,
    kind: InstanceErrorKind,
    msg: &str,
) -> Vec<LifecycleEvent> {
    phase_events(reg, id, InstancePhase::Error(InstanceError { kind, message: msg.to_string() }))
}

fn done_with_error(
    reg: &mut InstanceRegistry,
    id: Uuid,
    ctx: &PollContext,
    startup_kind: InstanceErrorKind,
    startup_msg: &str,
    switch_kind: InstanceErrorKind,
    switch_msg: &str,
) -> (PollOutcome, Vec<LifecycleEvent>) {
    let events = match ctx {
        PollContext::Startup => error_events(reg, id, startup_kind, startup_msg),
        PollContext::ModelSwitch { old_key } => {
            reg.update_config(id, |c| c.selected_model_key = old_key.clone()).ok();
            reg.save().ok();
            error_events(reg, id, switch_kind, switch_msg)
        }
    };
    reg.consume_start_time(id);
    (PollOutcome::Done, events)
}

fn persist_startup_metrics(reg: &mut InstanceRegistry, id: Uuid, driver: &dyn ServerDriver, elapsed: Duration) {
    let Some(config) = reg.get_config(id).cloned() else { return };
    let schema = driver.param_schema();
    let profile = reg.get_active_profile_params(id).cloned();
    let mem_key = ModelMemoryKey {
        server_type: config.server_type,
        model_key: config.selected_model_key.clone().unwrap_or_default(),
    };
    let memory = reg.get_model_memory(&mem_key).cloned();
    let resolved = ParamValues::resolve(profile.as_ref(), memory.as_ref(), &schema);
    crate::update_model_memory(reg, id, resolved.0).ok();
    crate::push_restart_duration_sample(reg, id, elapsed.as_secs_f64()).ok();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::persistence::InMemoryPersistence;
    use crate::registry::InstanceRegistry;
    use crate::testing::MockDriver;
    use crate::types::{InstancePhase, ServerInstanceConfig, ServerType};

    use super::{
        adopt, finish_warm_load, poll_adopted, poll_once, start, stop, switch_model, LifecycleEvent,
        PollContext, PollOutcome, SwitchPlan,
    };

    fn make_registry() -> InstanceRegistry {
        InstanceRegistry::new(Box::new(InMemoryPersistence::default()))
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn ollama_config(name: &str) -> ServerInstanceConfig {
        let mut cfg = ServerInstanceConfig::new(name, ServerType::Ollama, free_port(), "/usr/bin/ollama");
        cfg.selected_model_key = Some("llama3:8b".into());
        cfg
    }

    fn external_config(name: &str) -> ServerInstanceConfig {
        let mut cfg = ServerInstanceConfig::new(name, ServerType::External, free_port(), "");
        cfg.host = "127.0.0.1".into();
        cfg
    }

    fn phase_of(events: &[LifecycleEvent]) -> Option<InstancePhase> {
        events.iter().rev().map(|e| match e {
            LifecycleEvent::PhaseChanged(_, p) => p.clone(),
        }).next()
    }

    #[test]
    fn start_returns_launch_plan_for_unhealthy_managed_instance() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        let driver = MockDriver::new_unhealthy(ServerType::Ollama);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_some(), "should return a LaunchPlan");
        assert_eq!(events.first().map(|e| match e {
            LifecycleEvent::PhaseChanged(_, p) => p.clone(),
        }), Some(InstancePhase::Starting), "must emit Starting first");
    }

    #[test]
    fn start_adopts_healthy_instance_as_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        let driver = MockDriver::new(ServerType::Ollama);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
    }

    #[test]
    fn start_external_not_reachable_gives_health_check_failed() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        let driver = MockDriver::new_unmanaged_unhealthy(ServerType::External);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::HealthCheckFailed)
        ));
    }

    #[test]
    fn start_from_stopped_never_leaves_stopped_and_records_start_time() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopped));
        let driver = MockDriver::new_unhealthy(ServerType::Ollama);

        start(&mut reg, id, &driver);

        assert_ne!(reg.get_phase(id), Some(&InstancePhase::Stopped), "start must move the instance out of Stopped");
        assert!(reg.consume_start_time(id).is_some(), "start must record the start time");
    }

    #[test]
    fn start_noop_when_starting_or_stopping() {
        for in_progress_phase in [InstancePhase::Starting, InstancePhase::Stopping] {
            let mut reg = make_registry();
            let id = reg.add_instance(ollama_config("a"));
            reg.set_phase(id, in_progress_phase.clone()).unwrap();
            let driver = MockDriver::new(ServerType::Ollama);

            let (plan, events) = start(&mut reg, id, &driver);

            assert!(plan.is_none());
            assert!(events.is_empty(), "must not emit events for {in_progress_phase:?}");
            assert_eq!(reg.get_phase(id), Some(&in_progress_phase));
            assert!(reg.consume_start_time(id).is_none(), "must not record a start time for {in_progress_phase:?}");
        }
    }

    #[test]
    fn start_noop_when_managed_instance_already_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new(ServerType::Ollama);

        let (plan, events) = start(&mut reg, id, &driver);

        assert!(plan.is_none());
        assert!(events.is_empty());
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
        assert!(reg.consume_start_time(id).is_none());
    }

    #[test]
    fn start_proceeds_when_external_instance_already_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("lm-studio"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new_unhealthy(ServerType::External);

        let (_, events) = start(&mut reg, id, &driver);

        assert_eq!(phase_of(&events), Some(InstancePhase::Starting));
    }

    #[test]
    fn stop_sets_stopping_and_returns_grace_secs() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new(ServerType::Ollama);

        let (plan, events) = stop(&mut reg, id, &driver);
        assert!(plan.grace_secs >= 0.0);
        assert_eq!(phase_of(&events), Some(InstancePhase::Stopping));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopping));
    }

    struct FailLaunchDriver;
    impl crate::driver::ServerDriver for FailLaunchDriver {
        fn server_type(&self) -> crate::types::ServerType { crate::types::ServerType::MlxLm }
        fn param_schema(&self) -> Vec<crate::types::ParamDescriptor> { vec![] }
        fn switch_requires_restart(&self) -> bool { true }
        fn launch(&self, _: &crate::types::ServerInstanceConfig, _: Option<&crate::types::ModelRef>, _: &crate::types::ParamValues) -> Result<crate::driver::LaunchPlan, String> {
            Err("launch failed".into())
        }
        fn stop(&self, _: &crate::types::ServerInstanceConfig) -> crate::driver::ShutdownPlan {
            crate::driver::ShutdownPlan { grace_period_secs: 0.0 }
        }
        fn list_models(&self, _: &crate::types::ServerInstanceConfig) -> Result<Vec<crate::types::ModelRef>, String> { Ok(vec![]) }
        fn switch_model(&self, _: &crate::types::ModelRef, _: &crate::types::ParamValues, _: &crate::types::ServerInstanceConfig) -> Result<(), String> { Ok(()) }
        fn health_check(&self, _: &crate::types::ServerInstanceConfig) -> crate::driver::HealthStatus {
            crate::driver::HealthStatus::Unhealthy("not ready".into())
        }
    }

    #[test]
    fn switch_model_not_running_returns_key_only() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        let driver = MockDriver::new(ServerType::Ollama);

        let (plan, events) = switch_model(&mut reg, id, "new-model", &driver);

        assert!(matches!(plan, SwitchPlan::KeyOnly));
        assert!(events.is_empty(), "no phase change when instance is not running");
        assert_eq!(reg.get_config(id).unwrap().selected_model_key.as_deref(), Some("new-model"));
    }

    #[test]
    fn switch_model_restart_driver_returns_restart_plan() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new_restart(ServerType::MlxLm);

        let (plan, events) = switch_model(&mut reg, id, "new-model", &driver);

        match plan {
            SwitchPlan::Restart { old_key, .. } => assert_eq!(old_key.as_deref(), Some("llama3:8b")),
            other => panic!("expected Restart, got {other:?}"),
        }
        assert_eq!(phase_of(&events), Some(InstancePhase::SwitchingModel));
        assert!(reg.consume_start_time(id).is_some(), "start time must be recorded before a restart");
    }

    #[test]
    fn switch_model_sync_driver_returns_warm_load_plan() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new(ServerType::Ollama);

        let (plan, events) = switch_model(&mut reg, id, "new-model", &driver);

        match plan {
            SwitchPlan::WarmLoad { model, old_key } => {
                assert_eq!(model.key, "new-model");
                assert_eq!(old_key.as_deref(), Some("llama3:8b"));
            }
            other => panic!("expected WarmLoad, got {other:?}"),
        }
        assert_eq!(phase_of(&events), Some(InstancePhase::SwitchingModel));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::SwitchingModel), "must not resolve to Running until finish_warm_load");
    }

    #[test]
    fn switch_model_restart_launch_failure_restores_old_key_and_fails() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();

        let (plan, events) = switch_model(&mut reg, id, "new-model", &FailLaunchDriver);

        assert!(matches!(plan, SwitchPlan::Failed { .. }));
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::ModelSwitchFailed)
        ));
        assert_eq!(reg.get_config(id).unwrap().selected_model_key.as_deref(), Some("llama3:8b"), "old key must be restored");
        assert!(reg.consume_start_time(id).is_none(), "restart launch failure must not leak start time");
    }

    #[test]
    fn finish_warm_load_ok_sets_running_and_persists_metrics() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::SwitchingModel).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        let events = finish_warm_load(&mut reg, id, &driver, Ok(()), None);

        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
        assert!(reg.consume_start_time(id).is_none(), "start time must be consumed");
    }

    #[test]
    fn finish_warm_load_err_restores_old_key_and_sets_error() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::SwitchingModel).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);
        let old_key = Some("old-model".to_string());

        let events = finish_warm_load(&mut reg, id, &driver, Err("driver switch failed".into()), old_key.clone());

        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::ModelSwitchFailed)
        ));
        assert_eq!(reg.get_config(id).unwrap().selected_model_key, old_key, "must roll back to the old key");
        assert!(reg.consume_start_time(id).is_none(), "start time must be consumed");
    }

    #[test]
    fn adopt_sets_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();

        let events = adopt(&mut reg, id);
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
    }

    #[test]
    fn poll_once_continue_when_not_yet_healthy() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        let (outcome, events) = poll_once(
            &mut reg, id, &driver,
            false, true, Duration::from_secs(1), PollContext::Startup,
        );
        assert_eq!(outcome, PollOutcome::Continue);
        assert!(events.is_empty());
    }

    #[test]
    fn poll_once_done_when_healthy() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        let (outcome, events) = poll_once(
            &mut reg, id, &driver,
            true, true, Duration::from_secs(2), PollContext::Startup,
        );
        assert_eq!(outcome, PollOutcome::Done);
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
    }

    #[test]
    fn poll_once_done_on_process_exit() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        let (outcome, events) = poll_once(
            &mut reg, id, &driver,
            false, false, Duration::from_secs(1), PollContext::Startup,
        );
        assert_eq!(outcome, PollOutcome::Done);
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::LaunchFailed)
        ));
    }

    #[test]
    fn poll_once_done_on_timeout() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        let (outcome, events) = poll_once(
            &mut reg, id, &driver,
            false, true, Duration::from_secs(30), PollContext::Startup,
        );
        assert_eq!(outcome, PollOutcome::Done);
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::HealthCheckFailed)
        ));
    }

    #[test]
    fn poll_once_model_switch_timeout_rolls_back_key() {
        let mut reg = make_registry();
        let mut cfg = ollama_config("a");
        cfg.selected_model_key = Some("new-model".into());
        let id = reg.add_instance(cfg);
        reg.set_phase(id, InstancePhase::SwitchingModel).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        let old_key = Some("old-model".to_string());
        let (outcome, _) = poll_once(
            &mut reg, id, &driver,
            false, true, Duration::from_secs(30),
            PollContext::ModelSwitch { old_key: old_key.clone() },
        );
        assert_eq!(outcome, PollOutcome::Done);
        assert_eq!(reg.get_config(id).unwrap().selected_model_key, old_key, "must roll back");
    }

    #[test]
    fn poll_once_healthy_consumes_start_time() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama);

        poll_once(&mut reg, id, &driver, true, true, Duration::from_secs(2), PollContext::Startup);
        assert!(reg.consume_start_time(id).is_none(), "start time must be consumed");
    }

    #[test]
    fn poll_adopted_running_to_error_when_unhealthy() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        reg.set_phase(id, InstancePhase::Running).unwrap();

        let events = poll_adopted(&mut reg, id, false);
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::HealthCheckFailed)
        ));
    }

    #[test]
    fn poll_adopted_error_to_running_when_healthy() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        reg.set_phase(id, InstancePhase::Error(crate::types::InstanceError {
            kind: crate::types::InstanceErrorKind::HealthCheckFailed,
            message: "gone".into(),
        })).unwrap();

        let events = poll_adopted(&mut reg, id, true);
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
    }

    #[test]
    fn poll_adopted_no_event_when_healthy_and_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        reg.set_phase(id, InstancePhase::Running).unwrap();

        let events = poll_adopted(&mut reg, id, true);
        assert!(events.is_empty());
    }

    #[test]
    fn poll_once_noop_when_phase_is_terminal() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Stopped).unwrap();
        let driver = MockDriver::new(ServerType::Ollama);

        let (outcome, events) = poll_once(
            &mut reg, id, &driver,
            true, true, Duration::from_secs(1), PollContext::Startup,
        );
        assert_eq!(outcome, PollOutcome::Done);
        assert!(events.is_empty(), "terminal phase must produce no events");
    }

    #[test]
    fn start_port_in_use_gives_port_conflict() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut reg = make_registry();
        let mut cfg = ServerInstanceConfig::new("a", ServerType::Ollama, port, "/usr/bin/ollama");
        cfg.host = "127.0.0.1".into();
        cfg.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(cfg);
        let driver = MockDriver::new_unhealthy(ServerType::Ollama);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::PortConflict { .. })
        ));
    }

    #[test]
    fn record_and_consume_start_time() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        assert!(reg.consume_start_time(id).is_none(), "none before record");
        reg.record_start_time(id);
        let t = reg.consume_start_time(id);
        assert!(t.is_some(), "should have instant after record");
        assert!(reg.consume_start_time(id).is_none(), "consumed once means gone");
    }
}
