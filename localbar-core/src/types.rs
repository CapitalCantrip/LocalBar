use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ParamValue {
    Double(f64),
    Int(i64),
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CanonicalParam {
    Temperature,
    TopP,
    TopK,
    MinP,
    MaxTokens,
    RepeatPenalty,
    PresencePenalty,
    Seed,
    ContextLength,
    SystemPrompt,
}

#[derive(Debug, Clone)]
pub struct ParamDescriptor {
    pub param: CanonicalParam,
    pub server_flag_name: &'static str,
    pub modelfile_param_name: Option<&'static str>,
    pub default_value: Option<ParamValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParamValues {
    #[serde(default)]
    pub values: HashMap<CanonicalParam, ParamValue>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedParams(pub ParamValues);

impl ParamValues {
    pub fn resolve(
        profile: Option<&ParamValues>,
        memory: Option<&ModelMemory>,
        driver_defaults: &[ParamDescriptor],
    ) -> ResolvedParams {
        let mut resolved = ParamValues::default();
        for desc in driver_defaults {
            if desc.param == CanonicalParam::SystemPrompt {
                continue;
            }
            if let Some(default) = &desc.default_value {
                resolved.values.insert(desc.param, default.clone());
            }
        }
        if let Some(mem) = memory {
            resolved.merge_from(&mem.last_used_params, MergeMode::AutoMemory);
        }
        if let Some(profile_params) = profile {
            resolved.merge_from(profile_params, MergeMode::Profile);
        }
        ResolvedParams(resolved)
    }

    fn merge_from(&mut self, source: &ParamValues, mode: MergeMode) {
        for (param, value) in &source.values {
            if *param == CanonicalParam::SystemPrompt {
                continue;
            }
            self.values.insert(*param, value.clone());
        }
        match mode {
            MergeMode::Profile => {
                self.system_prompt = source.system_prompt.clone();
            }
            MergeMode::AutoMemory => {
                if self.system_prompt.is_none() {
                    self.system_prompt = source.system_prompt.clone();
                }
            }
        }
    }
}

enum MergeMode {
    AutoMemory,
    Profile,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelMemoryKey {
    pub server_type: ServerType,
    pub model_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMemory {
    pub server_type: ServerType,
    pub model_key: String,
    pub last_used_params: ParamValues,
    pub restart_duration_samples: Vec<f64>,
}

impl ModelMemory {
    pub fn new(server_type: ServerType, model_key: impl Into<String>) -> Self {
        Self {
            server_type,
            model_key: model_key.into(),
            last_used_params: ParamValues::default(),
            restart_duration_samples: Vec::new(),
        }
    }

    pub fn key(&self) -> ModelMemoryKey {
        ModelMemoryKey { server_type: self.server_type, model_key: self.model_key.clone() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRef {
    pub key: String,
    pub display_name: String,
    pub publisher: Option<String>,
    pub architecture: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerType {
    MlxLm,
    Ollama,
    External,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DiscoveryConfig {
    #[serde(default)]
    pub mlx_lm_search_paths: Vec<String>,
    #[serde(default)]
    pub ollama_executable_path: Option<String>,
}

impl DiscoveryConfig {
    pub fn resolved_mlx_paths(&self, override_path: Option<&str>) -> Vec<String> {
        match override_path {
            Some(p) => vec![p.to_owned()],
            None => self.mlx_lm_search_paths.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInstanceConfig {
    pub id: Uuid,
    pub name: String,
    pub server_type: ServerType,
    pub host: String,
    pub port: u16,
    pub executable_path: String,
    pub selected_model_key: Option<String>,
    pub instance_params: ParamValues,
    pub active_profile_id: Option<Uuid>,
    pub managed_model_tag: Option<String>,
    pub start_on_launch: bool,
    pub was_running_when_quit: bool,
    #[serde(default)]
    pub model_search_path_override: Option<String>,
}

impl ServerInstanceConfig {
    pub fn new(
        name: impl Into<String>,
        server_type: ServerType,
        port: u16,
        executable_path: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            server_type,
            host: "127.0.0.1".to_string(),
            port,
            executable_path: executable_path.into(),
            selected_model_key: None,
            instance_params: ParamValues::default(),
            active_profile_id: None,
            managed_model_tag: None,
            start_on_launch: false,
            was_running_when_quit: false,
            model_search_path_override: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedProfile {
    pub id: Uuid,
    pub name: String,
    pub params: ParamValues,
    pub server_type: Option<ServerType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstancePhase {
    Stopped,
    Starting,
    Running,
    Stopping,
    SwitchingModel,
    Error(InstanceError),
}

impl InstancePhase {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            InstancePhase::Running | InstancePhase::Starting | InstancePhase::SwitchingModel
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceError {
    pub kind: InstanceErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstanceErrorKind {
    LaunchFailed,
    PortConflict { port: u16 },
    HealthCheckFailed,
    StopFailed,
    ModelSwitchFailed,
    Unexpected,
}

#[cfg(test)]
mod discovery_tests {
    use super::DiscoveryConfig;

    #[test]
    fn global_paths_only_when_no_override() {
        let cfg = DiscoveryConfig {
            mlx_lm_search_paths: vec!["/a".into(), "/b".into()],
            ollama_executable_path: None,
        };
        assert_eq!(cfg.resolved_mlx_paths(None), vec!["/a", "/b"]);
    }

    #[test]
    fn override_replaces_global_list() {
        let cfg = DiscoveryConfig {
            mlx_lm_search_paths: vec!["/a".into(), "/b".into()],
            ollama_executable_path: None,
        };
        assert_eq!(cfg.resolved_mlx_paths(Some("/override")), vec!["/override"]);
    }

    #[test]
    fn empty_global_paths_returns_empty_vec_for_driver_fallback() {
        let cfg = DiscoveryConfig::default();
        assert!(cfg.resolved_mlx_paths(None).is_empty());
    }

    #[test]
    fn default_deserialises_from_empty_json_object() {
        let cfg: DiscoveryConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, DiscoveryConfig::default());
    }
}
