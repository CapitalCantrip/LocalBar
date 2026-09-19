use uuid::Uuid;

use crate::driver::{HealthStatus, LaunchPlan, ModelMetadata, ServerDriver, ShutdownPlan};
use crate::types::{
    CanonicalParam, ModelRef, ParamDescriptor, ParamValue, ParamValues, ServerInstanceConfig,
    ServerType,
};

pub struct OllamaDriver;

impl ServerDriver for OllamaDriver {
    fn server_type(&self) -> ServerType {
        ServerType::Ollama
    }

    fn param_schema(&self) -> Vec<ParamDescriptor> {
        vec![
            ParamDescriptor { param: CanonicalParam::ContextLength,   server_flag_name: "options.num_ctx",          modelfile_param_name: Some("num_ctx"),          default_value: Some(ParamValue::Int(2048)) },
            ParamDescriptor { param: CanonicalParam::Temperature,     server_flag_name: "options.temperature",       modelfile_param_name: Some("temperature"),      default_value: Some(ParamValue::Double(0.8)) },
            ParamDescriptor { param: CanonicalParam::MaxTokens,       server_flag_name: "options.num_predict",       modelfile_param_name: Some("num_predict"),      default_value: Some(ParamValue::Int(-1)) },
            ParamDescriptor { param: CanonicalParam::TopK,            server_flag_name: "options.top_k",             modelfile_param_name: Some("top_k"),            default_value: Some(ParamValue::Int(40)) },
            ParamDescriptor { param: CanonicalParam::RepeatPenalty,   server_flag_name: "options.repeat_penalty",    modelfile_param_name: Some("repeat_penalty"),   default_value: Some(ParamValue::Double(1.1)) },
            ParamDescriptor { param: CanonicalParam::PresencePenalty, server_flag_name: "options.presence_penalty",  modelfile_param_name: Some("presence_penalty"), default_value: None },
            ParamDescriptor { param: CanonicalParam::TopP,            server_flag_name: "options.top_p",             modelfile_param_name: Some("top_p"),            default_value: Some(ParamValue::Double(0.9)) },
            ParamDescriptor { param: CanonicalParam::MinP,            server_flag_name: "options.min_p",             modelfile_param_name: Some("min_p"),            default_value: None },
            ParamDescriptor { param: CanonicalParam::Seed,            server_flag_name: "options.seed",              modelfile_param_name: Some("seed"),             default_value: None },
        ]
    }

    fn launch(
        &self,
        config: &ServerInstanceConfig,
        _model: Option<&ModelRef>,
        _params: &ParamValues,
    ) -> Result<LaunchPlan, String> {
        Ok(LaunchPlan {
            executable: config.executable_path.clone(),
            arguments: vec!["serve".to_string()],
            environment: vec![("OLLAMA_HOST".to_string(), format!("{}:{}", config.host, config.port))],
            working_directory: None,
        })
    }

    fn stop(&self, _config: &ServerInstanceConfig) -> ShutdownPlan {
        ShutdownPlan { grace_period_secs: 10.0 }
    }

    fn health_check(&self, config: &ServerInstanceConfig) -> HealthStatus {
        let url = tags_url(config);
        match super::http::quick_agent().get(&url).call() {
            Ok(_) => HealthStatus::Healthy,
            Err(ureq::Error::Status(_, _)) => {
                HealthStatus::Unhealthy("unexpected status from /api/tags".to_string())
            }
            Err(_) => HealthStatus::Unreachable,
        }
    }

    fn list_models(&self, config: &ServerInstanceConfig) -> Result<Vec<ModelRef>, String> {
        let resp = super::http::quick_agent().get(&tags_url(config)).call().map_err(|e| e.to_string())?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        parse_tags_response(&json)
    }

    fn switch_model(
        &self,
        model: &ModelRef,
        _params: &ParamValues,
        config: &ServerInstanceConfig,
    ) -> Result<(), String> {
        let tag = config.managed_model_tag.as_deref().unwrap_or(&model.key);
        let url = format!("{}/api/generate", super::http::base_url(config));
        let body = serde_json::json!({"model": tag, "prompt": "", "stream": false});
        // 5-minute timeout: model warm-load into VRAM can be slow on large models.
        super::http::load_agent().post(&url).send_json(body).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn fetch_model_metadata(
        &self,
        model_key: &str,
        config: &ServerInstanceConfig,
    ) -> Option<ModelMetadata> {
        let url = format!("{}/api/show", super::http::base_url(config));
        let body = serde_json::json!({"name": model_key});
        let resp = super::http::quick_agent().post(&url).send_json(body).ok()?;
        let json: serde_json::Value = resp.into_json().ok()?;
        Some(ModelMetadata {
            parameter_count: json["details"]["parameter_size"].as_str().map(str::to_owned),
            quantization: json["details"]["quantization_level"].as_str().map(str::to_owned),
        })
    }

    fn generate_managed_config(
        &self,
        base_key: &str,
        params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Option<String> {
        Some(generate_modelfile(&self.param_schema(), base_key, params))
    }

    fn managed_config_tag(&self, model_key: &str, instance_id: Uuid) -> Option<String> {
        Some(managed_tag(model_key, instance_id))
    }

    fn apply_managed_config(
        &self,
        config: &ServerInstanceConfig,
        tag: &str,
        content: &str,
    ) -> Result<(), String> {
        let path = std::env::temp_dir()
            .join(format!("localbar-{}.modelfile", Uuid::new_v4()));
        std::fs::write(&path, content)
            .map_err(|e| format!("write temp Modelfile: {e}"))?;
        let status = std::process::Command::new(&config.executable_path)
            .args(["create", tag, "-f"])
            .arg(&path)
            .status()
            .map_err(|e| format!("ollama create spawn: {e}"))?;
        std::fs::remove_file(&path).ok();
        if status.success() { Ok(()) } else { Err(format!("ollama create {tag} failed")) }
    }

    fn delete_managed_config(
        &self,
        config: &ServerInstanceConfig,
        tag: &str,
    ) -> Result<(), String> {
        std::process::Command::new(&config.executable_path)
            .args(["rm", tag])
            .status()
            .map_err(|e| format!("ollama rm spawn: {e}"))?;
        Ok(())
    }
}

// ─── Managed tag ─────────────────────────────────────────────────────────────

/// Build the managed model tag: `localbar/<sanitised-model-key>-<first-8-chars-of-instance-id>`.
pub fn managed_tag(model_key: &str, instance_id: Uuid) -> String {
    let sanitized = sanitize_model_key_for_tag(model_key);
    let short_id = &instance_id.to_string()[..8];
    format!("localbar/{sanitized}-{short_id}")
}

/// Lowercase the key and replace runs of non-alphanumeric characters with a single hyphen.
fn sanitize_model_key_for_tag(key: &str) -> String {
    let mut result = String::new();
    let mut last_was_sep = true; // suppress leading hyphens
    for ch in key.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            result.push('-');
            last_was_sep = true;
        }
    }
    if result.ends_with('-') {
        result.pop();
    }
    result
}

// ─── HTTP helpers ─────────────────────────────────────────────────────────────

fn tags_url(config: &ServerInstanceConfig) -> String {
    format!("{}/api/tags", super::http::base_url(config))
}

// ─── Modelfile generation (Seam 3 — pure function) ───────────────────────────

/// Build an Ollama Modelfile from schema + params.
/// Param names come from `ParamDescriptor.modelfile_param_name` (C6).
/// Params with `modelfile_param_name = None` are silently skipped.
pub fn generate_modelfile(schema: &[ParamDescriptor], base_key: &str, params: &ParamValues) -> String {
    // Strip newlines to prevent Modelfile directive injection via base_key.
    let safe_key = base_key.replace(['\n', '\r'], "");
    let mut lines = vec![format!("FROM {}", safe_key)];
    for desc in schema {
        let Some(param_name) = desc.modelfile_param_name else { continue };
        let Some(value) = params.values.get(&desc.param) else { continue };
        if matches!(value, ParamValue::Bool(_)) { continue }
        lines.push(format!("PARAMETER {} {}", param_name, fmt_param_value(value)));
    }
    append_system_block(&mut lines, &params.system_prompt);
    lines.join("\n")
}

fn append_system_block(lines: &mut Vec<String>, system_prompt: &Option<String>) {
    if let Some(system) = system_prompt {
        if !system.is_empty() {
            // Escape backslashes first, then triple-quotes, to avoid double-escape.
            let escaped = system.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
            lines.push(format!("SYSTEM \"\"\"\n{}\n\"\"\"", escaped));
        }
    }
}

fn fmt_param_value(value: &ParamValue) -> String {
    match value {
        ParamValue::Double(v) => v.to_string(),
        ParamValue::Int(v) => v.to_string(),
        ParamValue::String(v) => v.clone(),
        ParamValue::Bool(v) => v.to_string(),
    }
}

// ─── Tags response parser ─────────────────────────────────────────────────────

fn parse_tags_response(json: &serde_json::Value) -> Result<Vec<ModelRef>, String> {
    let models = json["models"].as_array().ok_or("missing 'models' array in /api/tags response")?;
    models.iter().map(model_ref_from_json).collect()
}

fn model_ref_from_json(m: &serde_json::Value) -> Result<ModelRef, String> {
    let key = m["name"].as_str().ok_or("missing model name")?.to_owned();
    Ok(ModelRef { display_name: key.clone(), key, size_bytes: m["size"].as_i64() })
}

// ─── sanitize_model_key_for_tag tests ────────────────────────────────────────

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn sanitize_simple_name() {
        assert_eq!(sanitize_model_key_for_tag("llama3"), "llama3");
    }

    #[test]
    fn sanitize_colon_becomes_hyphen() {
        assert_eq!(sanitize_model_key_for_tag("llama3:8b"), "llama3-8b");
    }

    #[test]
    fn sanitize_runs_collapsed_to_single_hyphen() {
        assert_eq!(sanitize_model_key_for_tag("a::b"), "a-b");
    }

    #[test]
    fn sanitize_leading_trailing_non_alnum_stripped() {
        assert_eq!(sanitize_model_key_for_tag(":llama3:"), "llama3");
    }

    #[test]
    fn sanitize_uppercase_lowercased() {
        assert_eq!(sanitize_model_key_for_tag("Llama3:8B"), "llama3-8b");
    }

    #[test]
    fn managed_tag_format() {
        let id = uuid::Uuid::parse_str("12345678-1234-1234-1234-123456789012").unwrap();
        let tag = managed_tag("llama3:8b", id);
        assert_eq!(tag, "localbar/llama3-8b-12345678");
    }
}

// ─── Seam 3 tests — pure Modelfile generation, no process needed ──────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_schema() -> Vec<ParamDescriptor> {
        vec![
            ParamDescriptor {
                param: CanonicalParam::Temperature,
                server_flag_name: "options.temperature",
                modelfile_param_name: Some("temperature"),
                default_value: Some(ParamValue::Double(0.8)),
            },
            ParamDescriptor {
                param: CanonicalParam::ContextLength,
                server_flag_name: "options.num_ctx",
                modelfile_param_name: Some("num_ctx"),
                default_value: Some(ParamValue::Int(2048)),
            },
            // MaxTokens intentionally has no modelfile name to test C6 skip.
            ParamDescriptor {
                param: CanonicalParam::MaxTokens,
                server_flag_name: "options.num_predict",
                modelfile_param_name: None,
                default_value: Some(ParamValue::Int(-1)),
            },
        ]
    }

    fn params_with(entries: &[(CanonicalParam, ParamValue)]) -> ParamValues {
        ParamValues {
            values: entries.iter().cloned().collect::<HashMap<_, _>>(),
            system_prompt: None,
        }
    }

    #[test]
    fn modelfile_all_params_set() {
        let params = params_with(&[
            (CanonicalParam::Temperature, ParamValue::Double(0.5)),
            (CanonicalParam::ContextLength, ParamValue::Int(4096)),
        ]);
        let out = generate_modelfile(&test_schema(), "llama3:8b", &params);
        assert!(out.starts_with("FROM llama3:8b"), "FROM line must be first");
        assert!(out.contains("PARAMETER temperature 0.5"));
        assert!(out.contains("PARAMETER num_ctx 4096"));
    }

    #[test]
    fn modelfile_partial_params_only_set_ones_emitted() {
        let params = params_with(&[(CanonicalParam::Temperature, ParamValue::Double(0.7))]);
        let out = generate_modelfile(&test_schema(), "llama3:8b", &params);
        assert!(out.contains("PARAMETER temperature 0.7"));
        assert!(!out.contains("PARAMETER num_ctx"), "unset param must not appear");
    }

    #[test]
    fn modelfile_system_prompt_present() {
        let params = ParamValues {
            system_prompt: Some("You are a helpful assistant.".to_string()),
            ..Default::default()
        };
        let out = generate_modelfile(&test_schema(), "llama3:8b", &params);
        assert!(out.contains("SYSTEM \"\"\""));
        assert!(out.contains("You are a helpful assistant."));
    }

    #[test]
    fn modelfile_system_prompt_absent() {
        let out = generate_modelfile(&test_schema(), "llama3:8b", &ParamValues::default());
        assert!(!out.contains("SYSTEM"), "no SYSTEM block when prompt is absent");
    }

    #[test]
    fn modelfile_skips_param_with_no_modelfile_name() {
        let params = params_with(&[(CanonicalParam::MaxTokens, ParamValue::Int(100))]);
        let out = generate_modelfile(&test_schema(), "llama3:8b", &params);
        assert!(!out.contains("PARAMETER num_predict"), "param with None modelfile_param_name must be skipped");
        assert!(!out.contains("PARAMETER max_tokens"));
    }

    #[test]
    fn modelfile_newline_in_base_key_is_stripped() {
        let out = generate_modelfile(&[], "llama3:8b\nSYSTEM injected", &ParamValues::default());
        assert_eq!(out, "FROM llama3:8bSYSTEM injected");
    }

    #[test]
    fn modelfile_backslash_before_triple_quote_escaped_once() {
        let params = ParamValues {
            system_prompt: Some("a \\\"\"\" b".to_string()),
            ..Default::default()
        };
        let out = generate_modelfile(&[], "base", &params);
        // backslash → \\ then """ → \"\"\", so the combined \\\"\"\" should
        // produce \\\\\"\"\", not double-escape.
        assert!(out.contains("SYSTEM \"\"\""));
        assert!(!out.contains("\\\\\\\\"), "should not double-escape backslashes");
    }
}
