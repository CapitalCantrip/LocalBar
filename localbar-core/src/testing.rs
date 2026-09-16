/// Mock ServerDriver for use in tests. Stateless; returns canned values.
use crate::driver::{HealthStatus, LaunchPlan, ServerDriver, ShutdownPlan};
use crate::types::{
    CanonicalParam, ModelRef, ParamDescriptor, ParamValue, ParamValues, ServerInstanceConfig,
    ServerType,
};

pub struct MockDriver {
    pub server_type: ServerType,
    pub models: Vec<ModelRef>,
}

impl MockDriver {
    pub fn new(server_type: ServerType) -> Self {
        Self {
            server_type,
            models: vec![
                ModelRef {
                    key: "model-a".to_string(),
                    display_name: "Model A".to_string(),
                    size_bytes: None,
                },
                ModelRef {
                    key: "model-b".to_string(),
                    display_name: "Model B".to_string(),
                    size_bytes: None,
                },
            ],
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
}
