use crate::driver::{HealthStatus, LaunchPlan, ServerDriver, ShutdownPlan};
use crate::drivers::http::base_url;
use crate::types::{ModelRef, ParamDescriptor, ParamValues, ServerInstanceConfig, ServerType};

pub struct ExternalDriver;

impl ServerDriver for ExternalDriver {
    fn server_type(&self) -> ServerType {
        ServerType::External
    }

    fn manages_lifecycle(&self) -> bool {
        false
    }

    fn param_schema(&self) -> Vec<ParamDescriptor> {
        Vec::new()
    }

    fn launch(
        &self,
        _config: &ServerInstanceConfig,
        _model: Option<&ModelRef>,
        _params: &ParamValues,
    ) -> Result<LaunchPlan, String> {
        Ok(LaunchPlan {
            executable: String::new(),
            arguments: Vec::new(),
            environment: Vec::new(),
            working_directory: None,
        })
    }

    fn stop(&self, _config: &ServerInstanceConfig) -> ShutdownPlan {
        ShutdownPlan { grace_period_secs: 0.0 }
    }

    fn health_check(&self, config: &ServerInstanceConfig) -> HealthStatus {
        let agent = super::http::quick_agent();
        match probe_result(&agent, &models_url(config)) {
            ProbeOutcome::Ok => return HealthStatus::Healthy,
            ProbeOutcome::Unreachable => return HealthStatus::Unreachable,
            ProbeOutcome::BadStatus(_) => {}
        }
        match probe_result(&agent, &health_url(config)) {
            ProbeOutcome::Ok => HealthStatus::Healthy,
            ProbeOutcome::BadStatus(msg) => HealthStatus::Unhealthy(msg),
            ProbeOutcome::Unreachable => HealthStatus::Unreachable,
        }
    }

    fn list_models(&self, config: &ServerInstanceConfig) -> Result<Vec<ModelRef>, String> {
        let resp = super::http::quick_agent()
            .get(&models_url(config))
            .call()
            .map_err(|e| e.to_string())?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        Ok(parse_openai_models(&json))
    }

    fn switch_model(
        &self,
        _model: &ModelRef,
        _params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Result<(), String> {
        Ok(())
    }
}

fn models_url(config: &ServerInstanceConfig) -> String {
    format!("{}/v1/models", base_url(config))
}

fn health_url(config: &ServerInstanceConfig) -> String {
    format!("{}/health", base_url(config))
}

enum ProbeOutcome {
    Ok,
    BadStatus(String),
    Unreachable,
}

fn probe_result(agent: &ureq::Agent, url: &str) -> ProbeOutcome {
    match agent.get(url).call() {
        Ok(_) => ProbeOutcome::Ok,
        Err(ureq::Error::Status(code, _)) => {
            ProbeOutcome::BadStatus(format!("unexpected {} from {}", code, url))
        }
        Err(_) => ProbeOutcome::Unreachable,
    }
}

fn parse_openai_models(json: &serde_json::Value) -> Vec<ModelRef> {
    let Some(data) = json["data"].as_array() else { return Vec::new() };
    data.iter().filter_map(model_ref_from_openai).collect()
}

fn model_ref_from_openai(m: &serde_json::Value) -> Option<ModelRef> {
    let key = m["id"].as_str()?.to_owned();
    Some(ModelRef { display_name: key.clone(), key, publisher: None, architecture: None, size_bytes: None, modified_secs: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_config() -> ServerInstanceConfig {
        ServerInstanceConfig::new("external-test", ServerType::External, 8080, "")
    }

    #[test]
    fn launch_is_noop_returns_ok() {
        let driver = ExternalDriver;
        let config = dummy_config();
        let result = driver.launch(&config, None, &ParamValues::default());
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert!(plan.executable.is_empty(), "launch plan must be empty for external driver");
    }

    #[test]
    fn stop_returns_zero_grace() {
        let driver = ExternalDriver;
        let config = dummy_config();
        let plan = driver.stop(&config);
        assert_eq!(plan.grace_period_secs, 0.0);
    }

    #[test]
    fn fetch_model_metadata_returns_none() {
        let driver = ExternalDriver;
        let config = dummy_config();
        assert!(driver.fetch_model_metadata("any-key", &config).is_none());
    }

    #[test]
    fn param_schema_is_empty() {
        let driver = ExternalDriver;
        assert!(driver.param_schema().is_empty());
    }

    #[test]
    fn server_type_is_external() {
        assert_eq!(ExternalDriver.server_type(), ServerType::External);
    }

    #[test]
    fn manages_lifecycle_is_false() {
        assert!(!ExternalDriver.manages_lifecycle());
    }

    #[test]
    fn parse_openai_models_standard_response() {
        let json = serde_json::json!({
            "object": "list",
            "data": [
                {"id": "gpt-4", "object": "model"},
                {"id": "llama3:8b", "object": "model"}
            ]
        });
        let models = parse_openai_models(&json);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].key, "gpt-4");
        assert_eq!(models[1].key, "llama3:8b");
        assert_eq!(models[0].display_name, models[0].key);
    }

    #[test]
    fn parse_openai_models_empty_data() {
        let json = serde_json::json!({"object": "list", "data": []});
        assert!(parse_openai_models(&json).is_empty());
    }

    #[test]
    fn parse_openai_models_missing_data_returns_empty() {
        let json = serde_json::json!({"error": "not found"});
        assert!(parse_openai_models(&json).is_empty());
    }

    #[test]
    fn parse_openai_models_entry_missing_id_skipped() {
        let json = serde_json::json!({
            "data": [
                {"object": "model"},
                {"id": "valid-model", "object": "model"}
            ]
        });
        let models = parse_openai_models(&json);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].key, "valid-model");
    }

    #[test]
    fn switch_model_is_noop() {
        let driver = ExternalDriver;
        let config = dummy_config();
        let model = ModelRef { key: "m".to_string(), display_name: "m".to_string(), publisher: None, architecture: None, size_bytes: None, modified_secs: None };
        assert!(driver.switch_model(&model, &ParamValues::default(), &config).is_ok());
    }
}
