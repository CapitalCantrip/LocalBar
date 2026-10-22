use uuid::Uuid;

use crate::driver::ServerDriver;
use crate::registry::InstanceRegistry;
use crate::types::{CanonicalParam, ModelMemoryKey, ParamDescriptor, ParamValues};

pub fn effective_params(reg: &InstanceRegistry, id: Uuid, driver: &dyn ServerDriver) -> Result<ParamValues, String> {
    let config = reg.get_config(id).ok_or_else(|| format!("effective_params: no instance {id}"))?;
    let memory = config.selected_model_key.as_ref().and_then(|model_key| {
        reg.get_model_memory(&ModelMemoryKey { server_type: config.server_type, model_key: model_key.clone() })
    });
    Ok(layer(
        &driver.param_schema(),
        memory.map(|m| &m.last_used_params),
        &config.instance_params,
        reg.get_active_profile_params(id),
    ))
}

fn layer(
    driver_defaults: &[ParamDescriptor],
    memory: Option<&ParamValues>,
    instance: &ParamValues,
    profile: Option<&ParamValues>,
) -> ParamValues {
    let mut effective = defaults_of(driver_defaults);
    if let Some(memory) = memory {
        overlay(&mut effective, memory, SystemPromptRule::FallBackWhenUnset);
    }
    overlay(&mut effective, instance, SystemPromptRule::FallBackWhenUnset);
    if let Some(profile) = profile {
        overlay(&mut effective, profile, SystemPromptRule::Replace);
    }
    effective
}

enum SystemPromptRule {
    FallBackWhenUnset,
    Replace,
}

fn defaults_of(driver_defaults: &[ParamDescriptor]) -> ParamValues {
    let values = driver_defaults
        .iter()
        .filter(|desc| desc.param != CanonicalParam::SystemPrompt)
        .filter_map(|desc| desc.default_value.clone().map(|value| (desc.param, value)))
        .collect();
    ParamValues { values, system_prompt: None }
}

fn overlay(effective: &mut ParamValues, layer: &ParamValues, rule: SystemPromptRule) {
    for (param, value) in &layer.values {
        if *param != CanonicalParam::SystemPrompt {
            effective.values.insert(*param, value.clone());
        }
    }
    match rule {
        SystemPromptRule::Replace => effective.system_prompt = layer.system_prompt.clone(),
        SystemPromptRule::FallBackWhenUnset => {
            if layer.system_prompt.is_some() {
                effective.system_prompt = layer.system_prompt.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::persistence::{InMemoryPersistence, Persistence};
    use crate::registry::InstanceRegistry;
    use crate::testing::MockDriver;
    use crate::types::{
        CanonicalParam, ModelMemory, NamedProfile, ParamDescriptor, ParamValue, ParamValues, ServerInstanceConfig,
        ServerType,
    };

    use super::effective_params;

    const MODEL: &str = "model-a";
    const MOCK_DEFAULT_TEMPERATURE: f64 = 0.8;
    const MOCK_DEFAULT_CONTEXT_LENGTH: i64 = 4096;

    struct Layers {
        memory: Option<ParamValues>,
        instance: ParamValues,
        profile: Option<ParamValues>,
        selected_model: Option<&'static str>,
    }

    impl Default for Layers {
        fn default() -> Self {
            Self { memory: None, instance: ParamValues::default(), profile: None, selected_model: Some(MODEL) }
        }
    }

    fn temperature(value: f64) -> ParamValues {
        let mut params = ParamValues::default();
        params.values.insert(CanonicalParam::Temperature, ParamValue::Double(value));
        params
    }

    fn prompt(text: Option<&str>) -> ParamValues {
        ParamValues { system_prompt: text.map(str::to_string), ..ParamValues::default() }
    }

    fn effective(layers: Layers) -> ParamValues {
        let mut persistence = InMemoryPersistence::default();
        let mut config = ServerInstanceConfig::new("i", ServerType::MlxLm, 8080, "/usr/bin/mlx");
        config.selected_model_key = layers.selected_model.map(str::to_string);
        config.instance_params = layers.instance;
        if let Some(params) = layers.profile {
            let profile = NamedProfile { id: Uuid::new_v4(), name: "p".into(), params, server_type: None };
            config.active_profile_id = Some(profile.id);
            persistence.save_profiles(&[profile]).unwrap();
        }
        let mut reg = InstanceRegistry::new(Box::new(persistence));
        reg.load_profiles().unwrap();
        if let Some(last_used_params) = layers.memory {
            reg.upsert_model_memory(ModelMemory { last_used_params, ..ModelMemory::new(ServerType::MlxLm, MODEL) }).unwrap();
        }
        let id = reg.add_instance(config);
        effective_params(&reg, id, &MockDriver::new(ServerType::MlxLm)).unwrap()
    }

    fn temperature_of(params: &ParamValues) -> Option<&ParamValue> {
        params.values.get(&CanonicalParam::Temperature)
    }

    #[test]
    fn driver_defaults_apply_when_no_other_layer_sets_a_param() {
        let params = effective(Layers::default());
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(MOCK_DEFAULT_TEMPERATURE)));
        assert_eq!(params.values.get(&CanonicalParam::ContextLength), Some(&ParamValue::Int(MOCK_DEFAULT_CONTEXT_LENGTH)));
    }

    #[test]
    fn model_memory_overrides_driver_defaults() {
        let params = effective(Layers { memory: Some(temperature(0.3)), ..Layers::default() });
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(0.3)));
        assert_eq!(params.values.get(&CanonicalParam::ContextLength), Some(&ParamValue::Int(MOCK_DEFAULT_CONTEXT_LENGTH)));
    }

    #[test]
    fn instance_params_override_model_memory() {
        let params = effective(Layers { memory: Some(temperature(0.3)), instance: temperature(0.7), ..Layers::default() });
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(0.7)));
    }

    #[test]
    fn the_active_profile_overrides_instance_params() {
        let params = effective(Layers {
            memory: Some(temperature(0.3)),
            instance: temperature(0.7),
            profile: Some(temperature(1.2)),
            ..Layers::default()
        });
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(1.2)));
    }

    #[test]
    fn a_param_the_active_profile_leaves_unset_keeps_the_instance_value() {
        let mut instance = temperature(0.7);
        instance.values.insert(CanonicalParam::MaxTokens, ParamValue::Int(256));
        let params = effective(Layers { instance, profile: Some(temperature(1.2)), ..Layers::default() });
        assert_eq!(params.values.get(&CanonicalParam::MaxTokens), Some(&ParamValue::Int(256)));
    }

    #[test]
    fn a_cleared_instance_field_falls_back_to_model_memory() {
        let mut instance = ParamValues::default();
        instance.values.insert(CanonicalParam::MaxTokens, ParamValue::Int(256));
        let params = effective(Layers { memory: Some(temperature(0.3)), instance, ..Layers::default() });
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(0.3)));
    }

    #[test]
    fn a_cleared_instance_field_falls_back_to_the_driver_default_without_memory() {
        let params = effective(Layers { instance: ParamValues::default(), ..Layers::default() });
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(MOCK_DEFAULT_TEMPERATURE)));
    }

    #[test]
    fn without_a_selected_model_no_model_memory_applies() {
        let params = effective(Layers { memory: Some(temperature(0.3)), selected_model: None, ..Layers::default() });
        assert_eq!(temperature_of(&params), Some(&ParamValue::Double(MOCK_DEFAULT_TEMPERATURE)));
    }

    #[test]
    fn the_model_memory_system_prompt_applies_when_no_higher_layer_sets_one() {
        let params = effective(Layers { memory: Some(prompt(Some("from memory"))), ..Layers::default() });
        assert_eq!(params.system_prompt.as_deref(), Some("from memory"));
    }

    #[test]
    fn the_instance_system_prompt_overrides_the_model_memory_prompt() {
        let params = effective(Layers {
            memory: Some(prompt(Some("from memory"))),
            instance: prompt(Some("from instance")),
            ..Layers::default()
        });
        assert_eq!(params.system_prompt.as_deref(), Some("from instance"));
    }

    #[test]
    fn a_cleared_instance_system_prompt_falls_back_to_the_model_memory_prompt() {
        let params = effective(Layers {
            memory: Some(prompt(Some("from memory"))),
            instance: prompt(None),
            ..Layers::default()
        });
        assert_eq!(params.system_prompt.as_deref(), Some("from memory"));
    }

    #[test]
    fn the_active_profile_system_prompt_overrides_the_instance_prompt() {
        let params = effective(Layers {
            instance: prompt(Some("from instance")),
            profile: Some(prompt(Some("from profile"))),
            ..Layers::default()
        });
        assert_eq!(params.system_prompt.as_deref(), Some("from profile"));
    }

    #[test]
    fn an_active_profile_without_a_system_prompt_clears_the_lower_prompts() {
        let params = effective(Layers {
            memory: Some(prompt(Some("from memory"))),
            instance: prompt(Some("from instance")),
            profile: Some(prompt(None)),
            ..Layers::default()
        });
        assert_eq!(params.system_prompt, None);
    }

    #[test]
    fn a_system_prompt_default_in_the_driver_schema_is_ignored() {
        let schema = vec![ParamDescriptor {
            param: CanonicalParam::SystemPrompt,
            server_flag_name: "--system",
            modelfile_param_name: None,
            default_value: Some(ParamValue::String("default system".into())),
        }];
        let defaults = super::defaults_of(&schema);
        assert!(!defaults.values.contains_key(&CanonicalParam::SystemPrompt));
        assert_eq!(defaults.system_prompt, None);
    }

    #[test]
    fn an_unknown_instance_is_an_error() {
        let reg = InstanceRegistry::new(Box::new(InMemoryPersistence::default()));
        assert!(effective_params(&reg, Uuid::new_v4(), &MockDriver::new(ServerType::MlxLm)).is_err());
    }
}
