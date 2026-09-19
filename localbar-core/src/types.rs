use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── ParamValue ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ParamValue {
    Double(f64),
    Int(i64),
    String(String),
    Bool(bool),
}

// ─── CanonicalParam ───────────────────────────────────────────────────────────

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
    // Never stored in ParamValues.values — lives in ParamValues.system_prompt.
    // Included for exhaustive descriptor switches only.
    SystemPrompt,
}

// ─── ParamDescriptor ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParamDescriptor {
    pub param: CanonicalParam,
    /// The server's CLI flag name shown in hover tooltips.
    pub server_flag_name: &'static str,
    /// The Modelfile PARAMETER directive name (e.g. "temperature").
    /// None for params not baked into an Ollama Modelfile. (C6)
    pub modelfile_param_name: Option<&'static str>,
    pub default_value: Option<ParamValue>,
}

// ─── ParamValues ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParamValues {
    #[serde(default)]
    pub values: HashMap<CanonicalParam, ParamValue>,
    pub system_prompt: Option<String>,
}

/// The result of `ParamValues::resolve`. Newtype prevents callers from
/// accidentally passing unresolved params where resolved params are expected.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedParams(pub ParamValues);

impl ParamValues {
    /// Sole implementation of the param resolution contract (C1).
    /// Priority (lowest → highest): driver defaults → model memory → active profile.
    /// `CanonicalParam::SystemPrompt` is never inserted into `values`; it is
    /// filtered out of driver defaults here to enforce the invariant at the boundary.
    pub fn resolve(
        profile: Option<&ParamValues>,
        memory: Option<&ModelMemory>,
        driver_defaults: &[ParamDescriptor],
    ) -> ResolvedParams {
        let mut resolved = ParamValues::default();
        for desc in driver_defaults {
            // SystemPrompt must never live in values — it belongs in system_prompt.
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
            // Profile is highest priority: always wins, including clearing the prompt.
            MergeMode::Profile => {
                self.system_prompt = source.system_prompt.clone();
            }
            // Auto-memory only sets the prompt when the profile did not supply one.
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

// ─── ModelMemoryKey ───────────────────────────────────────────────────────────

/// Composite key for ModelMemory. Entries are per-(server_type, model_key) so
/// that MlxLm and Ollama entries for a key string "llama3" never clobber each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelMemoryKey {
    pub server_type: ServerType,
    pub model_key: String,
}

// ─── ModelMemory ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMemory {
    pub server_type: ServerType,
    /// ModelRef.key — driver-scoped; same weights → separate entries per server type.
    pub model_key: String,
    pub last_used_params: ParamValues,
    /// Rolling window of measured stop→healthy durations (newest first, max 10).
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

// ─── ModelRef ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRef {
    pub key: String,
    pub display_name: String,
    pub size_bytes: Option<i64>,
}

// ─── ServerType ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerType {
    MlxLm,
    Ollama,
    External,
}

// ─── ServerInstanceConfig ────────────────────────────────────────────────────

/// One configured server instance. Pure config — no runtime state (no PID, no phase).
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
    /// Ollama only: localbar/<model>-<instanceId> managed model tag.
    pub managed_model_tag: Option<String>,
    pub start_on_launch: bool,
    pub was_running_when_quit: bool,
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
        }
    }
}

// ─── NamedProfile ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedProfile {
    pub id: Uuid,
    pub name: String,
    pub params: ParamValues,
    /// None = applicable to any server type.
    pub server_type: Option<ServerType>,
}

// ─── InstancePhase ───────────────────────────────────────────────────────────

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

// ─── InstanceError ───────────────────────────────────────────────────────────

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
