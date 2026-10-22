pub mod external;
pub mod http;
pub mod mlx_lm;
pub mod ollama;
pub mod process_match;

use crate::driver::ServerDriver;
use crate::executable::CommandResolver;
use crate::registry::InstanceRegistry;
use crate::types::{DiscoveryConfig, ServerInstanceConfig, ServerType};

pub fn for_instance(
    registry: &InstanceRegistry,
    config: &ServerInstanceConfig,
    resolver: CommandResolver,
) -> Box<dyn ServerDriver> {
    for_discovery(config, registry.get_discovery_config(), resolver)
}

pub fn for_discovery(
    config: &ServerInstanceConfig,
    discovery: &DiscoveryConfig,
    resolver: CommandResolver,
) -> Box<dyn ServerDriver> {
    match config.server_type {
        ServerType::Ollama => Box::new(
            ollama::OllamaDriver::new(ollama::explicit_models_dir(&explicit_ollama_candidates(config, discovery)))
                .with_resolver(resolver),
        ),
        ServerType::MlxLm => {
            let paths = discovery.resolved_mlx_paths(config.model_search_path_override.as_deref());
            Box::new(mlx_lm::MLXLMDriver::new(paths))
        }
        ServerType::External => Box::new(external::ExternalDriver),
    }
}

pub fn explicit_ollama_candidates<'a>(
    config: &'a ServerInstanceConfig,
    discovery: &'a DiscoveryConfig,
) -> ollama::ModelsDirCandidates<'a> {
    ollama::ModelsDirCandidates {
        instance_override: config.model_search_path_override.as_deref(),
        discovery_setting: discovery.ollama_models_dir.as_deref(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::executable::ExecutableNotFound;
    use crate::persistence::InMemoryPersistence;
    use crate::types::ParamValues;

    const SHARED_MODELS_DIR: &str = "/Users/someone/SharedModels/Ollama";

    fn no_resolver() -> CommandResolver {
        Arc::new(|name: &str| Err(ExecutableNotFound { command: name.into(), searched: vec![] }))
    }

    fn registry_with_ollama_dir(dir: &str) -> InstanceRegistry {
        let mut reg = InstanceRegistry::new(Box::new(InMemoryPersistence::default()));
        reg.set_discovery_config(DiscoveryConfig { ollama_models_dir: Some(dir.into()), ..DiscoveryConfig::default() }).unwrap();
        reg
    }

    #[test]
    fn ollama_driver_from_registry_uses_the_discovery_models_folder() {
        let reg = registry_with_ollama_dir(SHARED_MODELS_DIR);
        let config = ServerInstanceConfig::new("o", ServerType::Ollama, 11434, "ollama");
        let plan = for_instance(&reg, &config, no_resolver()).launch(&config, None, &ParamValues::default()).unwrap();
        assert!(
            plan.environment.contains(&(ollama::OLLAMA_MODELS_ENV_VAR.to_string(), SHARED_MODELS_DIR.to_string())),
            "{:?}",
            plan.environment
        );
    }
}
