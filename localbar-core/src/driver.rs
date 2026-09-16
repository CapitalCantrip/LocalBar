use crate::types::{ModelRef, ParamDescriptor, ParamValues, ServerInstanceConfig, ServerType};

// ─── Health ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy(String),
    Unreachable,
}

// ─── Model metadata ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetadata {
    pub parameter_count: Option<String>,
    pub quantization: Option<String>,
}

// ─── Launch / shutdown plans ─────────────────────────────────────────────────

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

// ─── ServerDriver ────────────────────────────────────────────────────────────

/// The seam between LocalBar's generic control plane and each server's behaviour.
/// Implementations must be stateless — all mutable state lives in InstanceRegistry.
/// No code outside a driver implementation may downcast `dyn ServerDriver` (C4).
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

    /// True when this driver owns the server process (full driver).
    /// False for external drivers — callers must not invoke `launch`/`stop` to spawn processes.
    fn manages_lifecycle(&self) -> bool {
        true
    }

    /// Returns best-effort metadata for a model. Default: None (C4).
    fn fetch_model_metadata(
        &self,
        _model_key: &str,
        _config: &ServerInstanceConfig,
    ) -> Option<ModelMetadata> {
        None
    }

    /// Generate a managed config artifact (e.g. an Ollama Modelfile).
    /// Returns None for drivers that don't use managed configs.
    fn generate_managed_config(
        &self,
        _base_key: &str,
        _params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Option<String> {
        None
    }
}
