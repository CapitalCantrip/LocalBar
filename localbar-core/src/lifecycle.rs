use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use uuid::Uuid;

use crate::driver::{HealthStatus, LaunchPlan, ServerDriver};
use crate::registry::InstanceRegistry;
use crate::types::{InstanceError, InstanceErrorKind, InstancePhase, ModelMemoryKey, ModelRef, ParamValues};

// ─── Public types ─────────────────────────────────────────────────────────────

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

// ─── start ────────────────────────────────────────────────────────────────────

/// Precondition: instance is in Starting phase with start time recorded.
pub fn start(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
) -> (Option<LaunchPlan>, Vec<LifecycleEvent>) {
    let Some(config) = reg.get_config(id).cloned() else {
        return (None, vec![]);
    };

    let health = driver.health_check(&config);

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

    match driver.launch(&config, None, &config.instance_params) {
        Ok(plan) => (Some(plan), vec![]),
        Err(e) => (None, error_events(reg, id, InstanceErrorKind::LaunchFailed, &e)),
    }
}

// ─── stop ─────────────────────────────────────────────────────────────────────

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

// ─── switch_model ─────────────────────────────────────────────────────────────

/// Precondition: config already updated with new model key and managed model ensured.
pub fn switch_model(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
) -> (Option<LaunchPlan>, Vec<LifecycleEvent>) {
    let events = phase_events(reg, id, InstancePhase::SwitchingModel);
    reg.record_start_time(id);
    let Some(config) = reg.get_config(id).cloned() else { return (None, events) };
    if driver.switch_requires_restart() {
        return switch_model_restart(reg, id, driver, &config, events);
    }
    switch_model_sync(reg, id, driver, &config, events)
}

fn switch_model_restart(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    config: &crate::types::ServerInstanceConfig,
    mut events: Vec<LifecycleEvent>,
) -> (Option<LaunchPlan>, Vec<LifecycleEvent>) {
    match driver.launch(config, None, &config.instance_params) {
        Ok(plan) => (Some(plan), events),
        Err(e) => {
            events.extend(error_events(reg, id, InstanceErrorKind::ModelSwitchFailed, &e));
            (None, events)
        }
    }
}

fn switch_model_sync(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    config: &crate::types::ServerInstanceConfig,
    mut events: Vec<LifecycleEvent>,
) -> (Option<LaunchPlan>, Vec<LifecycleEvent>) {
    let key = config.selected_model_key.clone().unwrap_or_default();
    let model = ModelRef { key: key.clone(), display_name: key, publisher: None, architecture: None, size_bytes: None, modified_secs: None };
    match driver.switch_model(&model, &config.instance_params, config) {
        Ok(()) => { events.extend(phase_events(reg, id, InstancePhase::Running)); (None, events) }
        Err(e) => { events.extend(error_events(reg, id, InstanceErrorKind::ModelSwitchFailed, &e)); (None, events) }
    }
}

// ─── adopt ────────────────────────────────────────────────────────────────────

/// Mark an already-running external/adopted instance as Running.
///
/// The Tauri layer spawns `run_adopted_health_poll` after calling this.
pub fn adopt(reg: &mut InstanceRegistry, id: Uuid) -> Vec<LifecycleEvent> {
    phase_events(reg, id, InstancePhase::Running)
}

// ─── poll_once ────────────────────────────────────────────────────────────────

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Precondition: elapsed is wall-clock time since the instance entered Starting/SwitchingModel.
pub fn poll_once(
    reg: &mut InstanceRegistry,
    id: Uuid,
    driver: &dyn ServerDriver,
    health: bool,
    process_alive: bool,
    elapsed: Duration,
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
        persist_startup_metrics(reg, id, driver, elapsed);
        reg.consume_start_time(id);
        return (PollOutcome::Done, phase_events(reg, id, InstancePhase::Running));
    }

    if elapsed >= STARTUP_TIMEOUT {
        return done_with_error(
            reg, id, &ctx,
            InstanceErrorKind::HealthCheckFailed, "startup timed out after 30 s",
            InstanceErrorKind::HealthCheckFailed, "model switch timed out after 30 s",
        );
    }

    (PollOutcome::Continue, vec![])
}

// ─── poll_adopted ─────────────────────────────────────────────────────────────

/// Ongoing health monitor for an adopted/external instance (every ~10 s).
///
/// Running → Error when `health` is false; Error → Running when `health` is true.
/// Returns an empty vec when no phase change is needed.
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

// ─── Private helpers ──────────────────────────────────────────────────────────

fn port_is_open(host: &str, port: u16) -> bool {
    let Ok(mut addrs) = (host, port).to_socket_addrs() else { return false };
    let Some(addr) = addrs.next() else { return false };
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::persistence::InMemoryPersistence;
    use crate::registry::InstanceRegistry;
    use crate::testing::MockDriver;
    use crate::types::{InstancePhase, ServerInstanceConfig, ServerType};

    use super::{adopt, poll_adopted, poll_once, start, stop, switch_model, LifecycleEvent, PollContext, PollOutcome};

    fn make_registry() -> InstanceRegistry {
        InstanceRegistry::new(Box::new(InMemoryPersistence::default()))
    }

    fn ollama_config(name: &str) -> ServerInstanceConfig {
        let mut cfg = ServerInstanceConfig::new(name, ServerType::Ollama, 11434, "/usr/bin/ollama");
        cfg.selected_model_key = Some("llama3:8b".into());
        cfg
    }

    fn external_config(name: &str) -> ServerInstanceConfig {
        let mut cfg = ServerInstanceConfig::new(name, ServerType::External, 11434, "");
        cfg.host = "127.0.0.1".into();
        cfg
    }

    fn phase_of(events: &[LifecycleEvent]) -> Option<InstancePhase> {
        events.iter().rev().map(|e| match e {
            LifecycleEvent::PhaseChanged(_, p) => p.clone(),
        }).next()
    }

    // ── start ─────────────────────────────────────────────────────────────────

    #[test]
    fn start_returns_launch_plan_for_unhealthy_managed_instance() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new_unhealthy(ServerType::Ollama);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_some(), "should return a LaunchPlan");
        assert!(events.is_empty(), "no phase events yet when spawning");
    }

    #[test]
    fn start_adopts_healthy_instance_as_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new(ServerType::Ollama); // healthy by default

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
    }

    #[test]
    fn start_external_not_reachable_gives_health_check_failed() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new_unmanaged_unhealthy(ServerType::External);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::HealthCheckFailed)
        ));
    }

    // ── stop ──────────────────────────────────────────────────────────────────

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

    // ── switch_model ──────────────────────────────────────────────────────────

    #[test]
    fn switch_model_sync_driver_transitions_to_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new(ServerType::Ollama); // switch_requires_restart = false

        let (plan, events) = switch_model(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
    }

    #[test]
    fn switch_model_sets_switching_model_phase_first() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new(ServerType::Ollama);

        let (_, events) = switch_model(&mut reg, id, &driver);
        let phases: Vec<_> = events.iter().map(|e| match e {
            LifecycleEvent::PhaseChanged(_, p) => p.clone(),
        }).collect();
        assert!(phases.contains(&InstancePhase::SwitchingModel), "must pass through SwitchingModel");
    }

    #[test]
    fn switch_model_restart_driver_returns_launch_plan() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new_restart(ServerType::MlxLm);

        let (plan, events) = switch_model(&mut reg, id, &driver);
        assert!(plan.is_some(), "restart driver must return a LaunchPlan");
        assert_eq!(phase_of(&events), Some(InstancePhase::SwitchingModel));
    }

    #[test]
    fn switch_model_records_start_time() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        let driver = MockDriver::new_restart(ServerType::MlxLm);

        switch_model(&mut reg, id, &driver);
        assert!(reg.consume_start_time(id).is_some(), "start time must be recorded");
    }

    // ── adopt ─────────────────────────────────────────────────────────────────

    #[test]
    fn adopt_sets_running() {
        let mut reg = make_registry();
        let id = reg.add_instance(external_config("ext"));
        reg.set_phase(id, InstancePhase::Starting).unwrap();

        let events = adopt(&mut reg, id);
        assert_eq!(phase_of(&events), Some(InstancePhase::Running));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));
    }

    // ── poll_once ─────────────────────────────────────────────────────────────

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

    // ── poll_adopted ──────────────────────────────────────────────────────────

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

    // ── poll_once phase guard (M1) ────────────────────────────────────────────

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

    // ── start port conflict (M2) ──────────────────────────────────────────────

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
        reg.set_phase(id, InstancePhase::Starting).unwrap();
        reg.record_start_time(id);
        let driver = MockDriver::new_unhealthy(ServerType::Ollama);

        let (plan, events) = start(&mut reg, id, &driver);
        assert!(plan.is_none());
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::PortConflict { .. })
        ));
    }

    // ── switch_model_sync failure (M3) ────────────────────────────────────────

    struct SwitchFailDriver;
    impl crate::driver::ServerDriver for SwitchFailDriver {
        fn server_type(&self) -> crate::types::ServerType { crate::types::ServerType::Ollama }
        fn param_schema(&self) -> Vec<crate::types::ParamDescriptor> { vec![] }
        fn launch(&self, c: &crate::types::ServerInstanceConfig, _: Option<&crate::types::ModelRef>, _: &crate::types::ParamValues) -> Result<crate::driver::LaunchPlan, String> {
            Ok(crate::driver::LaunchPlan { executable: c.executable_path.clone(), arguments: vec![], environment: vec![], working_directory: None })
        }
        fn stop(&self, _: &crate::types::ServerInstanceConfig) -> crate::driver::ShutdownPlan {
            crate::driver::ShutdownPlan { grace_period_secs: 0.0 }
        }
        fn list_models(&self, _: &crate::types::ServerInstanceConfig) -> Result<Vec<crate::types::ModelRef>, String> { Ok(vec![]) }
        fn switch_model(&self, _: &crate::types::ModelRef, _: &crate::types::ParamValues, _: &crate::types::ServerInstanceConfig) -> Result<(), String> {
            Err("driver switch failed".into())
        }
        fn health_check(&self, _: &crate::types::ServerInstanceConfig) -> crate::driver::HealthStatus {
            crate::driver::HealthStatus::Unhealthy("not ready".into())
        }
    }

    #[test]
    fn switch_model_sync_driver_failure_gives_model_switch_failed() {
        let mut reg = make_registry();
        let id = reg.add_instance(ollama_config("a"));
        reg.set_phase(id, InstancePhase::Running).unwrap();

        let (plan, events) = switch_model(&mut reg, id, &SwitchFailDriver);
        assert!(plan.is_none());
        assert!(matches!(
            phase_of(&events),
            Some(InstancePhase::Error(e)) if matches!(e.kind, crate::types::InstanceErrorKind::ModelSwitchFailed)
        ));
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Error(crate::types::InstanceError {
            kind: crate::types::InstanceErrorKind::ModelSwitchFailed,
            message: "driver switch failed".into(),
        })));
    }

    // ── start_times ───────────────────────────────────────────────────────────

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
