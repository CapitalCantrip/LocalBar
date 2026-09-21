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
    pub health_status: HealthStatus,
    /// When false, `manages_lifecycle()` returns false (External-style driver).
    pub manages_lifecycle: bool,
    /// When true, `switch_requires_restart()` returns true (mlx-lm-style driver).
    pub switch_requires_restart: bool,
    /// Captures (tag, content) pairs from `apply_managed_config` calls.
    pub managed_config_calls: Arc<Mutex<Vec<(String, String)>>>,
    /// Captures tag strings from `delete_managed_config` calls.
    pub delete_config_calls: Arc<Mutex<Vec<String>>>,
}

impl MockDriver {
    pub fn new(server_type: ServerType) -> Self {
        Self {
            server_type,
            models: Self::default_models(),
            health_status: HealthStatus::Healthy,
            manages_lifecycle: true,
            switch_requires_restart: false,
            managed_config_calls: Arc::new(Mutex::new(Vec::new())),
            delete_config_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn default_models() -> Vec<ModelRef> {
        fn mk(key: &str, name: &str) -> ModelRef {
            ModelRef { key: key.into(), display_name: name.into(), publisher: None, architecture: None, size_bytes: None, modified_secs: None }
        }
        vec![mk("model-a", "Model A"), mk("model-b", "Model B")]
    }

    /// A driver that reports Unhealthy but still manages its own process.
    pub fn new_unhealthy(server_type: ServerType) -> Self {
        Self {
            health_status: HealthStatus::Unhealthy("not ready".into()),
            ..Self::new(server_type)
        }
    }

    /// A driver that reports Unhealthy AND does not manage its own lifecycle
    /// (External-style: LocalBar cannot spawn it).
    pub fn new_unmanaged_unhealthy(server_type: ServerType) -> Self {
        Self {
            health_status: HealthStatus::Unhealthy("not ready".into()),
            manages_lifecycle: false,
            ..Self::new(server_type)
        }
    }

    /// A driver that requires a full process restart for model switches (mlx-lm-style).
    pub fn new_restart(server_type: ServerType) -> Self {
        Self { switch_requires_restart: true, ..Self::new(server_type) }
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
        self.health_status.clone()
    }

    fn manages_lifecycle(&self) -> bool {
        self.manages_lifecycle
    }

    fn switch_requires_restart(&self) -> bool {
        self.switch_requires_restart
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
