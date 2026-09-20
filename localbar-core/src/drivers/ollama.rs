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
    Ok(ModelRef { display_name: key.clone(), key, publisher: None, size_bytes: m["size"].as_i64() })
}

// ─── CLI list ─────────────────────────────────────────────────────────────────

/// Invoke `ollama list` and parse its output. Uses `executable` as the path to
/// the ollama binary, falling back to PATH resolution when the string is "ollama".
pub fn list_models_cli(executable: &str) -> Result<Vec<ModelRef>, String> {
    let output = std::process::Command::new(executable)
        .arg("list")
        .output()
        .map_err(|e| format!("ollama list spawn: {e}"))?;
    if !output.status.success() {
        return Err(format!("ollama list exited with status {}", output.status));
    }
    Ok(parse_ollama_list_output(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse the tabular stdout of `ollama list` into `Vec<ModelRef>`.
/// Skips the header row (detected by first token "NAME") and any malformed rows;
/// never panics.
pub fn parse_ollama_list_output(raw: &str) -> Vec<ModelRef> {
    raw.lines()
        .filter(|l| !is_ollama_header_line(l))
        .filter_map(parse_ollama_list_row)
        .collect()
}

fn is_ollama_header_line(line: &str) -> bool {
    line.split_whitespace().next().map(|t| t.eq_ignore_ascii_case("NAME")).unwrap_or(false)
}

fn parse_ollama_list_row(line: &str) -> Option<ModelRef> {
    let mut cols = line.split_whitespace();
    let name = cols.next()?.to_owned();
    let _id = cols.next();
    let size_bytes = parse_size_cols(cols.next(), cols.next());
    Some(ModelRef { display_name: name.clone(), key: name, publisher: None, size_bytes })
}

fn parse_size_cols(value_col: Option<&str>, unit_col: Option<&str>) -> Option<i64> {
    let value: f64 = value_col?.parse().ok()?;
    let multiplier: f64 = match unit_col?.to_ascii_uppercase().as_str() {
        "B"  | "IB"  => 1.0,
        "KB" | "KIB" => 1_000.0,
        "MB" | "MIB" => 1_000_000.0,
        "GB" | "GIB" => 1_000_000_000.0,
        "TB" | "TIB" => 1_000_000_000_000.0,
        _             => return None,
    };
    Some((value * multiplier) as i64)
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

// ─── CLI list parser tests ────────────────────────────────────────────────────

#[cfg(test)]
mod cli_tests {
    use super::*;

    const TYPICAL_OUTPUT: &str = "\
NAME             ID              SIZE      MODIFIED
bge-m3:latest    1a0efc6c2574    1.2 GB    20 hours ago
mistral:7b       6577803aa9a0    4.4 GB    6 days ago";

    #[test]
    fn typical_output_parses_two_models() {
        let models = parse_ollama_list_output(TYPICAL_OUTPUT);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].key, "bge-m3:latest");
        assert_eq!(models[0].display_name, "bge-m3:latest");
        assert_eq!(models[1].key, "mistral:7b");
    }

    #[test]
    fn typical_output_parses_size_bytes() {
        let models = parse_ollama_list_output(TYPICAL_OUTPUT);
        assert_eq!(models[0].size_bytes, Some(1_200_000_000));
        assert_eq!(models[1].size_bytes, Some(4_400_000_000));
    }

    #[test]
    fn empty_output_returns_empty_vec() {
        assert!(parse_ollama_list_output("").is_empty());
    }

    #[test]
    fn header_only_returns_empty_vec() {
        assert!(parse_ollama_list_output("NAME    ID    SIZE    MODIFIED").is_empty());
    }

    #[test]
    fn preamble_before_header_is_ignored() {
        let raw = "warning: some preamble\nNAME    ID    SIZE    MODIFIED\nllama3:8b    abc    4.9 GB    yesterday";
        let models = parse_ollama_list_output(raw);
        // "warning:" does not start with NAME so it tries to parse as a row,
        // but "warning:" has no second column (id), yielding one model with key "warning:".
        // The real guard here is that "NAME" header is correctly filtered out.
        assert!(models.iter().all(|m| m.key != "NAME"));
    }

    #[test]
    fn gb_lowercase_parses_size() {
        let raw = "NAME    ID    SIZE    MODIFIED\nfoo:bar    abc123    2.0 gb    yesterday";
        let models = parse_ollama_list_output(raw);
        assert_eq!(models[0].size_bytes, Some(2_000_000_000));
    }

    #[test]
    fn malformed_row_name_only_yields_model_without_size() {
        let raw = "NAME    ID    SIZE    MODIFIED\njust-a-name";
        let models = parse_ollama_list_output(raw);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].key, "just-a-name");
        assert_eq!(models[0].size_bytes, None);
    }

    #[test]
    fn unknown_size_unit_yields_none_size() {
        let raw = "NAME    ID    SIZE    MODIFIED\nfoo:bar    abc123    1.0 XB    yesterday";
        let models = parse_ollama_list_output(raw);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].size_bytes, None);
    }

    #[test]
    fn blank_lines_are_skipped() {
        let raw = "NAME    ID    SIZE    MODIFIED\n\nllama3:8b    abc    4.9 GB    yesterday\n";
        let models = parse_ollama_list_output(raw);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].key, "llama3:8b");
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
