use uuid::Uuid;

use crate::types::{ModelRef, ParamDescriptor, ParamValues, ServerInstanceConfig, ServerType};

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy(String),
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelMetadata {
    pub parameter_count: Option<String>,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub executable: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ShutdownPlan {
    pub grace_period_secs: f64,
}

pub trait ServerDriver: Send + Sync {
    fn server_type(&self) -> ServerType;
    fn param_schema(&self) -> Vec<ParamDescriptor>;

    fn launch(
        &self,
        config: &ServerInstanceConfig,
        model: Option<&ModelRef>,
        params: &ParamValues,
    ) -> Result<LaunchPlan, String>;

    fn stop(&self, config: &ServerInstanceConfig) -> ShutdownPlan;

    fn list_models(&self, config: &ServerInstanceConfig) -> Result<Vec<ModelRef>, String>;

    fn switch_model(
        &self,
        model: &ModelRef,
        params: &ParamValues,
        config: &ServerInstanceConfig,
    ) -> Result<(), String>;

    fn health_check(&self, config: &ServerInstanceConfig) -> HealthStatus;

    fn manages_lifecycle(&self) -> bool {
        true
    }

    fn switch_requires_restart(&self) -> bool {
        false
    }

    fn fetch_model_metadata(
        &self,
        _model_key: &str,
        _config: &ServerInstanceConfig,
    ) -> Option<ModelMetadata> {
        None
    }

    fn generate_managed_config(
        &self,
        _base_key: &str,
        _params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Option<String> {
        None
    }

    fn managed_config_tag(&self, _model_key: &str, _instance_id: Uuid) -> Option<String> {
        None
    }

    fn apply_managed_config(
        &self,
        _config: &ServerInstanceConfig,
        _tag: &str,
        _content: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    fn delete_managed_config(
        &self,
        _config: &ServerInstanceConfig,
        _tag: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}
