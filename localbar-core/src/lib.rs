// localbar-core: platform-agnostic domain logic, drivers, and persistence.
// No Tauri dependency. All Tauri-specific code lives in localbar-tauri.
#![deny(clippy::cognitive_complexity)]
#![deny(clippy::too_many_lines)]

pub mod driver;
pub mod drivers;
pub mod persistence;
pub mod registry;
pub mod testing;
pub mod types;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::driver::ServerDriver;
    use crate::persistence::{InMemoryPersistence, Persistence};
    use crate::registry::InstanceRegistry;
    use crate::testing::MockDriver;
    use crate::types::{
        CanonicalParam, InstanceError, InstanceErrorKind, InstancePhase, ModelMemory, ParamValue,
        ParamValues, ServerInstanceConfig, ServerType,
    };

    struct SharedPersistence(Arc<Mutex<InMemoryPersistence>>);
    impl Persistence for SharedPersistence {
        fn save_instances(&mut self, configs: &[ServerInstanceConfig]) -> Result<(), String> {
            self.0.lock().unwrap().save_instances(configs)
        }
        fn load_instances(&self) -> Result<Vec<ServerInstanceConfig>, String> {
            self.0.lock().unwrap().load_instances()
        }
        fn save_profiles(&mut self, profiles: &[crate::types::NamedProfile]) -> Result<(), String> {
            self.0.lock().unwrap().save_profiles(profiles)
        }
        fn load_profiles(&self) -> Result<Vec<crate::types::NamedProfile>, String> {
            self.0.lock().unwrap().load_profiles()
        }
        fn load_model_memory(&self) -> Result<std::collections::HashMap<crate::types::ModelMemoryKey, ModelMemory>, String> {
            self.0.lock().unwrap().load_model_memory()
        }
        fn upsert_model_memory(&mut self, entry: ModelMemory) -> Result<(), String> {
            self.0.lock().unwrap().upsert_model_memory(entry)
        }
    }
    unsafe impl Send for SharedPersistence {}
    unsafe impl Sync for SharedPersistence {}

    fn make_registry() -> InstanceRegistry {
        InstanceRegistry::new(Box::new(InMemoryPersistence::default()))
    }

    fn make_config(name: &str) -> ServerInstanceConfig {
        ServerInstanceConfig::new(name, ServerType::MlxLm, 8080, "/usr/bin/mlx")
    }

    fn make_memory(model_key: &str) -> ModelMemory {
        ModelMemory::new(ServerType::MlxLm, model_key)
    }

    // ── Phase transitions ─────────────────────────────────────────────────────

    #[test]
    fn phase_transition_happy_path() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));

        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopped));

        reg.set_phase(id, InstancePhase::Starting).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Starting));

        reg.set_phase(id, InstancePhase::Running).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Running));

        reg.set_phase(id, InstancePhase::Stopping).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopping));

        reg.set_phase(id, InstancePhase::Stopped).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Stopped));
    }

    #[test]
    fn phase_transition_error_to_retry() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));

        reg.set_phase(id, InstancePhase::Error(InstanceError {
            kind: InstanceErrorKind::LaunchFailed,
            message: "process exited immediately".to_string(),
        })).unwrap();
        assert!(matches!(reg.get_phase(id), Some(InstancePhase::Error(_))));

        reg.set_phase(id, InstancePhase::Starting).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::Starting));
    }

    #[test]
    fn switching_model_phase() {
        let mut reg = make_registry();
        let id = reg.add_instance(make_config("mlx"));
        reg.set_phase(id, InstancePhase::Running).unwrap();
        reg.set_phase(id, InstancePhase::SwitchingModel).unwrap();
        assert_eq!(reg.get_phase(id), Some(&InstancePhase::SwitchingModel));
        assert!(reg.get_phase(id).unwrap().is_active());
    }

    #[test]
    fn set_phase_unknown_id_returns_err() {
        let mut reg = make_registry();
        let unknown = uuid::Uuid::new_v4();
        assert!(reg.set_phase(unknown, InstancePhase::Running).is_err());
    }

    // ── ParamValues::resolve priority ordering ────────────────────────────────

    fn temp_desc() -> crate::types::ParamDescriptor {
        crate::types::ParamDescriptor {
            param: CanonicalParam::Temperature,
            server_flag_name: "--temperature",
            modelfile_param_name: Some("temperature"),
            default_value: Some(ParamValue::Double(0.8)),
        }
    }

    fn ctx_desc() -> crate::types::ParamDescriptor {
        crate::types::ParamDescriptor {
            param: CanonicalParam::ContextLength,
            server_flag_name: "--ctx-size",
            modelfile_param_name: Some("num_ctx"),
            default_value: Some(ParamValue::Int(4096)),
        }
    }

    #[test]
    fn resolve_driver_defaults_only() {
        let schema = vec![temp_desc(), ctx_desc()];
        let resolved = ParamValues::resolve(None, None, &schema);
        assert_eq!(resolved.0.values[&CanonicalParam::Temperature], ParamValue::Double(0.8));
        assert_eq!(resolved.0.values[&CanonicalParam::ContextLength], ParamValue::Int(4096));
    }

    #[test]
    fn resolve_memory_overrides_driver_defaults() {
        let schema = vec![temp_desc(), ctx_desc()];
        let mut memory_params = ParamValues::default();
        memory_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(0.3));
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        let resolved = ParamValues::resolve(None, Some(&memory), &schema);
        assert_eq!(resolved.0.values[&CanonicalParam::Temperature], ParamValue::Double(0.3));
        assert_eq!(resolved.0.values[&CanonicalParam::ContextLength], ParamValue::Int(4096));
    }

    #[test]
    fn resolve_profile_overrides_memory_and_driver_defaults() {
        let schema = vec![temp_desc(), ctx_desc()];

        let mut memory_params = ParamValues::default();
        memory_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(0.3));
        memory_params.values.insert(CanonicalParam::ContextLength, ParamValue::Int(8192));
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        let mut profile_params = ParamValues::default();
        profile_params.values.insert(CanonicalParam::Temperature, ParamValue::Double(1.0));

        let resolved = ParamValues::resolve(Some(&profile_params), Some(&memory), &schema);
        assert_eq!(resolved.0.values[&CanonicalParam::Temperature], ParamValue::Double(1.0));
        assert_eq!(resolved.0.values[&CanonicalParam::ContextLength], ParamValue::Int(8192));
    }

    #[test]
    fn resolve_system_prompt_propagates_through_layers() {
        let profile_params = ParamValues { system_prompt: Some("You are a helpful assistant.".into()), ..Default::default() };
        let resolved = ParamValues::resolve(Some(&profile_params), None, &[]);
        assert_eq!(resolved.0.system_prompt.as_deref(), Some("You are a helpful assistant."));
    }

    #[test]
    fn resolve_profile_system_prompt_overrides_memory_prompt() {
        let memory_params = ParamValues { system_prompt: Some("from memory".into()), ..Default::default() };
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        let profile_params = ParamValues { system_prompt: Some("from profile".into()), ..Default::default() };

        let resolved = ParamValues::resolve(Some(&profile_params), Some(&memory), &[]);
        assert_eq!(resolved.0.system_prompt.as_deref(), Some("from profile"));
    }

    #[test]
    fn resolve_profile_no_prompt_clears_memory_prompt() {
        // Profile with None system_prompt should override (clear) the memory-set prompt.
        let memory_params = ParamValues { system_prompt: Some("from memory".into()), ..Default::default() };
        let mut memory = make_memory("m");
        memory.last_used_params = memory_params;

        // Profile has no system_prompt
        let profile_params = ParamValues::default();
        let resolved = ParamValues::resolve(Some(&profile_params), Some(&memory), &[]);
        assert_eq!(resolved.0.system_prompt, None);
    }

    #[test]
    fn resolve_system_prompt_variant_in_driver_defaults_is_ignored() {
        // A driver that mistakenly includes SystemPrompt in its schema must not
        // end up with it in the values map.
        let schema = vec![
            crate::types::ParamDescriptor {
                param: CanonicalParam::SystemPrompt,
                server_flag_name: "--system",
                modelfile_param_name: None,
                default_value: Some(ParamValue::String("default system".into())),
            },
        ];
        let resolved = ParamValues::resolve(None, None, &schema);
        assert!(!resolved.0.values.contains_key(&CanonicalParam::SystemPrompt));
    }

    // ── start_warning (C3) ────────────────────────────────────────────────────

    #[test]
    fn start_warning_none_when_all_stopped() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("A"));
        let b = reg.add_instance(make_config("B"));
        assert_eq!(reg.start_warning(a), None);
        assert_eq!(reg.start_warning(b), None);
    }

    #[test]
    fn start_warning_some_when_another_is_running() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        let b = reg.add_instance(make_config("beta"));
        reg.set_phase(a, InstancePhase::Running).unwrap();

        let warning = reg.start_warning(b);
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("alpha"));
    }

    #[test]
    fn start_warning_none_for_self_when_running() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        reg.set_phase(a, InstancePhase::Running).unwrap();
        assert_eq!(reg.start_warning(a), None);
    }

    #[test]
    fn start_warning_some_when_another_is_starting() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        let b = reg.add_instance(make_config("beta"));
        reg.set_phase(a, InstancePhase::Starting).unwrap();
        assert!(reg.start_warning(b).is_some());
    }

    #[test]
    fn start_warning_none_when_other_is_stopping() {
        let mut reg = make_registry();
        let a = reg.add_instance(make_config("alpha"));
        let b = reg.add_instance(make_config("beta"));
        reg.set_phase(a, InstancePhase::Stopping).unwrap();
        assert_eq!(reg.start_warning(b), None);
    }

    // ── Persistence round-trip ────────────────────────────────────────────────

    #[test]
    fn persistence_round_trip_produces_identical_configs() {
        let store = Arc::new(Mutex::new(InMemoryPersistence::default()));
        let mut reg = InstanceRegistry::new(Box::new(SharedPersistence(Arc::clone(&store))));
        let mut config = make_config("ollama");
        config.server_type = ServerType::Ollama;
        config.port = 11434;
        config.selected_model_key = Some("llama3:8b".into());
        let id = reg.add_instance(config.clone());
        reg.save().expect("save should succeed");

        let mut reg2 = InstanceRegistry::new(Box::new(SharedPersistence(Arc::clone(&store))));
        reg2.load().expect("load should succeed");

        let loaded = reg2.get_config(id).expect("config should exist after load");
        assert_eq!(loaded, &config);
    }

    // ── Mock driver sanity ────────────────────────────────────────────────────

    #[test]
    fn mock_driver_list_models() {
        let driver = MockDriver::new(ServerType::MlxLm);
        let config = make_config("mlx");
        let models = driver.list_models(&config).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].key, "model-a");
    }

    #[test]
    fn mock_driver_fetch_model_metadata_default_none() {
        let driver = MockDriver::new(ServerType::MlxLm);
        let config = make_config("mlx");
        assert_eq!(driver.fetch_model_metadata("model-a", &config), None);
    }

    #[test]
    fn param_descriptor_c6_modelfile_names() {
        let schema = MockDriver::default_schema();
        let temp = schema.iter().find(|d| d.param == CanonicalParam::Temperature).unwrap();
        let max_tokens = schema.iter().find(|d| d.param == CanonicalParam::MaxTokens).unwrap();
        let ctx = schema.iter().find(|d| d.param == CanonicalParam::ContextLength).unwrap();

        assert_eq!(temp.modelfile_param_name, Some("temperature"));
        assert_eq!(max_tokens.modelfile_param_name, None);
        assert_eq!(ctx.modelfile_param_name, Some("num_ctx"));
    }
}
