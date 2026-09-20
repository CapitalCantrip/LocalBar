/// Mock ServerDriver for use in tests. Stateless; returns canned values.
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::driver::{HealthStatus, LaunchPlan, ServerDriver, ShutdownPlan};
use crate::types::{
    CanonicalParam, ModelRef, ParamDescriptor, ParamValue, ParamValues, ServerInstanceConfig,
    ServerType,
};

pub struct MockDriver {
    pub server_type: ServerType,
    pub models: Vec<ModelRef>,
    /// Captures (tag, content) pairs from `apply_managed_config` calls.
    pub managed_config_calls: Arc<Mutex<Vec<(String, String)>>>,
    /// Captures tag strings from `delete_managed_config` calls.
    pub delete_config_calls: Arc<Mutex<Vec<String>>>,
}

impl MockDriver {
    pub fn new(server_type: ServerType) -> Self {
        Self {
            server_type,
            models: vec![
                ModelRef {
                    key: "model-a".to_string(),
                    display_name: "Model A".to_string(),
                    publisher: None,
                    architecture: None,
                    size_bytes: None,
                    modified_secs: None,
                },
                ModelRef {
                    key: "model-b".to_string(),
                    display_name: "Model B".to_string(),
                    publisher: None,
                    architecture: None,
                    size_bytes: None,
                    modified_secs: None,
                },
            ],
            managed_config_calls: Arc::new(Mutex::new(Vec::new())),
            delete_config_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn default_schema() -> Vec<ParamDescriptor> {
        vec![
            ParamDescriptor {
                param: CanonicalParam::Temperature,
                server_flag_name: "--temperature",
                modelfile_param_name: Some("temperature"),
                default_value: Some(ParamValue::Double(0.8)),
            },
            ParamDescriptor {
                param: CanonicalParam::MaxTokens,
                server_flag_name: "--max-tokens",
                modelfile_param_name: None,
                default_value: Some(ParamValue::Int(2048)),
            },
            ParamDescriptor {
                param: CanonicalParam::ContextLength,
                server_flag_name: "--ctx-size",
                modelfile_param_name: Some("num_ctx"),
                default_value: Some(ParamValue::Int(4096)),
            },
        ]
    }
}

impl ServerDriver for MockDriver {
    fn server_type(&self) -> ServerType {
        self.server_type
    }

    fn param_schema(&self) -> Vec<ParamDescriptor> {
        Self::default_schema()
    }

    fn launch(
        &self,
        config: &ServerInstanceConfig,
        _model: Option<&ModelRef>,
        _params: &ParamValues,
    ) -> Result<LaunchPlan, String> {
        Ok(LaunchPlan {
            executable: config.executable_path.clone(),
            arguments: vec!["--port".to_string(), config.port.to_string()],
            environment: Vec::new(),
            working_directory: None,
        })
    }

    fn stop(&self, _config: &ServerInstanceConfig) -> ShutdownPlan {
        ShutdownPlan { grace_period_secs: 5.0 }
    }

    fn list_models(&self, _config: &ServerInstanceConfig) -> Result<Vec<ModelRef>, String> {
        Ok(self.models.clone())
    }

    fn switch_model(
        &self,
        _model: &ModelRef,
        _params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Result<(), String> {
        Ok(())
    }

    fn health_check(&self, _config: &ServerInstanceConfig) -> HealthStatus {
        HealthStatus::Healthy
    }

    // ── Managed-config methods (Ollama-like behaviour when server_type == Ollama) ──

    fn generate_managed_config(
        &self,
        base_key: &str,
        _params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Option<String> {
        if self.server_type == ServerType::Ollama {
            Some(format!("FROM {base_key}"))
        } else {
            None
        }
    }

    fn managed_config_tag(&self, model_key: &str, instance_id: Uuid) -> Option<String> {
        if self.server_type == ServerType::Ollama {
            let sanitized = model_key
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                .collect::<String>();
            Some(format!("localbar/{sanitized}-{}", &instance_id.to_string()[..8]))
        } else {
            None
        }
    }

    fn apply_managed_config(
        &self,
        _config: &ServerInstanceConfig,
        tag: &str,
        content: &str,
    ) -> Result<(), String> {
        self.managed_config_calls.lock().unwrap().push((tag.to_string(), content.to_string()));
        Ok(())
    }

    fn delete_managed_config(
        &self,
        _config: &ServerInstanceConfig,
        tag: &str,
    ) -> Result<(), String> {
        self.delete_config_calls.lock().unwrap().push(tag.to_string());
        Ok(())
    }
}
