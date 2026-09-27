use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::process_match;
use crate::driver::{HealthStatus, LaunchPlan, ModelMetadata, ServerDriver, ShutdownPlan};
use crate::types::{
    CanonicalParam, ModelRef, ParamDescriptor, ParamValue, ParamValues, ServerInstanceConfig,
    ServerType,
};

const OLLAMA_EXECUTABLE: &str = "ollama";
const OLLAMA_SERVE_SUBCOMMAND: &str = "serve";
pub const OLLAMA_MODELS_ENV_VAR: &str = "OLLAMA_MODELS";
const OLLAMA_HOME_DIR_NAME: &str = ".ollama";
const OLLAMA_MODELS_DIR_NAME: &str = "models";
const MANIFESTS_DIR_NAME: &str = "manifests";
const DEFAULT_REGISTRY: &str = "registry.ollama.ai";
const DEFAULT_NAMESPACE: &str = "library";

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

    fn recognises_process(&self, command_line: &str) -> bool {
        let tokens = process_match::tokens(command_line);
        process_match::executable_is(&tokens, OLLAMA_EXECUTABLE) && tokens[1..].contains(&OLLAMA_SERVE_SUBCOMMAND)
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
        super::http::warm_load_agent().post(&url).send_json(body).map_err(|e| e.to_string())?;
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

pub fn managed_tag(model_key: &str, instance_id: Uuid) -> String {
    let sanitized = sanitize_model_key_for_tag(model_key);
    let short_id = &instance_id.to_string()[..8];
    format!("localbar/{sanitized}-{short_id}")
}

fn sanitize_model_key_for_tag(key: &str) -> String {
    let mut result = String::new();
    let mut last_was_sep = true;
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

fn tags_url(config: &ServerInstanceConfig) -> String {
    format!("{}/api/tags", super::http::base_url(config))
}

pub fn generate_modelfile(schema: &[ParamDescriptor], base_key: &str, params: &ParamValues) -> String {
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

fn parse_tags_response(json: &serde_json::Value) -> Result<Vec<ModelRef>, String> {
    let models = json["models"].as_array().ok_or("missing 'models' array in /api/tags response")?;
    models.iter().map(model_ref_from_json).collect()
}

fn model_ref_from_json(m: &serde_json::Value) -> Result<ModelRef, String> {
    let key = m["name"].as_str().ok_or("missing model name")?.to_owned();
    Ok(ModelRef { display_name: key.clone(), key, publisher: None, architecture: None, size_bytes: m["size"].as_i64(), modified_secs: None })
}

pub fn resolve_models_dir(env_value: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = env_value {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    let home = home?;
    if home.is_empty() {
        return None;
    }
    Some(Path::new(home).join(OLLAMA_HOME_DIR_NAME).join(OLLAMA_MODELS_DIR_NAME))
}

pub fn list_models_from_manifests(models_dir: &Path) -> Result<Vec<ModelRef>, String> {
    let manifests_root = models_dir.join(MANIFESTS_DIR_NAME);
    if !manifests_root.is_dir() {
        return Err(format!("no manifests directory at {}", manifests_root.display()));
    }
    let mut models = Vec::new();
    scan_registries(&manifests_root, &mut models);
    Ok(models)
}

fn scan_registries(manifests_root: &Path, models: &mut Vec<ModelRef>) {
    for entry in read_subdirs(manifests_root) {
        let registry = entry_name(&entry);
        scan_namespaces(&entry.path(), &registry, models);
    }
}

fn scan_namespaces(registry_dir: &Path, registry: &str, models: &mut Vec<ModelRef>) {
    for entry in read_subdirs(registry_dir) {
        let namespace = entry_name(&entry);
        scan_models(&entry.path(), registry, &namespace, models);
    }
}

fn scan_models(namespace_dir: &Path, registry: &str, namespace: &str, models: &mut Vec<ModelRef>) {
    for entry in read_subdirs(namespace_dir) {
        let model = entry_name(&entry);
        scan_tags(&entry.path(), registry, namespace, &model, models);
    }
}

fn scan_tags(model_dir: &Path, registry: &str, namespace: &str, model: &str, models: &mut Vec<ModelRef>) {
    let Ok(entries) = std::fs::read_dir(model_dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(tag) = entry.file_name().to_str().map(str::to_owned) else { continue };
        if let Some(model_ref) = parse_manifest_file(registry, namespace, model, &tag, &path) {
            models.push(model_ref);
        }
    }
}

fn read_subdirs(dir: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).collect())
        .unwrap_or_default()
}

fn entry_name(entry: &std::fs::DirEntry) -> String {
    entry.file_name().to_string_lossy().into_owned()
}

pub fn manifest_key(registry: &str, namespace: &str, model: &str, tag: &str) -> String {
    if registry == DEFAULT_REGISTRY && namespace == DEFAULT_NAMESPACE {
        return format!("{model}:{tag}");
    }
    if registry == DEFAULT_REGISTRY {
        return format!("{namespace}/{model}:{tag}");
    }
    format!("{registry}/{namespace}/{model}:{tag}")
}

fn parse_manifest_file(registry: &str, namespace: &str, model: &str, tag: &str, path: &Path) -> Option<ModelRef> {
    let bytes = std::fs::read(path).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let key = manifest_key(registry, namespace, model, tag);
    Some(ModelRef {
        display_name: key.clone(),
        key,
        publisher: None,
        architecture: None,
        size_bytes: Some(manifest_size_bytes(&json)),
        modified_secs: manifest_mtime_secs(path),
    })
}

pub fn manifest_size_bytes(json: &serde_json::Value) -> i64 {
    let config_size = json["config"]["size"].as_i64().unwrap_or(0);
    let layers_size: i64 = json["layers"]
        .as_array()
        .map(|layers| layers.iter().filter_map(|l| l["size"].as_i64()).sum())
        .unwrap_or(0);
    config_size + layers_size
}

fn manifest_mtime_secs(path: &Path) -> Option<i64> {
    std::fs::metadata(path).ok()?.modified().ok()?
        .duration_since(std::time::UNIX_EPOCH).ok()
        .map(|d| d.as_secs() as i64)
}

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

#[cfg(test)]
mod manifest_tests {
    use super::*;
    use tempfile::TempDir;

    fn write_manifest(models_dir: &Path, registry: &str, namespace: &str, model: &str, tag: &str, body: &str) {
        let dir = models_dir.join(MANIFESTS_DIR_NAME).join(registry).join(namespace).join(model);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(tag), body).unwrap();
    }

    const SAMPLE_MANIFEST: &str = r#"{
        "config": {"size": 490},
        "layers": [
            {"size": 8149180896},
            {"size": 358},
            {"size": 8432}
        ]
    }"#;

    #[test]
    fn key_default_registry_and_namespace() {
        assert_eq!(manifest_key("registry.ollama.ai", "library", "gemma3", "latest"), "gemma3:latest");
    }

    #[test]
    fn key_default_registry_other_namespace() {
        assert_eq!(manifest_key("registry.ollama.ai", "someuser", "gemma3", "latest"), "someuser/gemma3:latest");
    }

    #[test]
    fn key_other_registry() {
        assert_eq!(manifest_key("my.registry.example", "someuser", "gemma3", "latest"), "my.registry.example/someuser/gemma3:latest");
    }

    #[test]
    fn size_bytes_sums_config_and_layers() {
        let json: serde_json::Value = serde_json::from_str(SAMPLE_MANIFEST).unwrap();
        assert_eq!(manifest_size_bytes(&json), 490 + 8149180896 + 358 + 8432);
    }

    #[test]
    fn size_bytes_missing_fields_default_to_zero() {
        let json: serde_json::Value = serde_json::from_str("{}").unwrap();
        assert_eq!(manifest_size_bytes(&json), 0);
    }

    #[test]
    fn missing_manifests_dir_is_an_error() {
        let tmp = TempDir::new().unwrap();
        assert!(list_models_from_manifests(tmp.path()).is_err());
    }

    #[test]
    fn scans_manifests_into_model_refs() {
        let tmp = TempDir::new().unwrap();
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "gemma3", "latest", SAMPLE_MANIFEST);
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "qwen3-hermes", "latest", SAMPLE_MANIFEST);
        let models = list_models_from_manifests(tmp.path()).unwrap();
        let mut keys: Vec<_> = models.iter().map(|m| m.key.clone()).collect();
        keys.sort();
        assert_eq!(keys, vec!["gemma3:latest", "qwen3-hermes:latest"]);
    }

    #[test]
    fn scanned_model_display_name_matches_key() {
        let tmp = TempDir::new().unwrap();
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "gemma3", "latest", SAMPLE_MANIFEST);
        let models = list_models_from_manifests(tmp.path()).unwrap();
        assert_eq!(models[0].display_name, models[0].key);
    }

    #[test]
    fn scanned_model_size_bytes_is_summed_from_manifest() {
        let tmp = TempDir::new().unwrap();
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "gemma3", "latest", SAMPLE_MANIFEST);
        let models = list_models_from_manifests(tmp.path()).unwrap();
        assert_eq!(models[0].size_bytes, Some(490 + 8149180896 + 358 + 8432));
    }

    #[test]
    fn unparseable_manifest_is_skipped_not_fatal() {
        let tmp = TempDir::new().unwrap();
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "broken", "latest", "not json");
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "gemma3", "latest", SAMPLE_MANIFEST);
        let models = list_models_from_manifests(tmp.path()).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].key, "gemma3:latest");
    }

    #[test]
    fn non_file_entry_at_tag_level_is_skipped() {
        let tmp = TempDir::new().unwrap();
        let model_dir = tmp.path().join(MANIFESTS_DIR_NAME).join("registry.ollama.ai").join("library").join("gemma3");
        std::fs::create_dir_all(model_dir.join("latest")).unwrap();
        let models = list_models_from_manifests(tmp.path()).unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn stray_file_at_registry_level_is_skipped() {
        let tmp = TempDir::new().unwrap();
        let manifests_root = tmp.path().join(MANIFESTS_DIR_NAME);
        std::fs::create_dir_all(&manifests_root).unwrap();
        std::fs::write(manifests_root.join(".DS_Store"), b"").unwrap();
        write_manifest(tmp.path(), "registry.ollama.ai", "library", "gemma3", "latest", SAMPLE_MANIFEST);
        let models = list_models_from_manifests(tmp.path()).unwrap();
        assert_eq!(models.len(), 1);
    }

    #[test]
    fn resolve_models_dir_prefers_env_override() {
        let dir = resolve_models_dir(Some("/custom/models"), Some("/Users/someone")).unwrap();
        assert_eq!(dir, PathBuf::from("/custom/models"));
    }

    #[test]
    fn resolve_models_dir_falls_back_to_home() {
        let dir = resolve_models_dir(None, Some("/Users/someone")).unwrap();
        assert_eq!(dir, PathBuf::from("/Users/someone/.ollama/models"));
    }

    #[test]
    fn resolve_models_dir_empty_env_falls_back_to_home() {
        let dir = resolve_models_dir(Some(""), Some("/Users/someone")).unwrap();
        assert_eq!(dir, PathBuf::from("/Users/someone/.ollama/models"));
    }

    #[test]
    fn resolve_models_dir_no_env_no_home_is_none() {
        assert!(resolve_models_dir(None, None).is_none());
    }

    #[test]
    fn resolve_models_dir_empty_home_is_none() {
        assert!(resolve_models_dir(None, Some("")).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn recognises_ollama_serve_processes() {
        for command_line in [
            "ollama serve",
            "/Applications/Ollama.app/Contents/Resources/ollama serve",
            "/usr/local/bin/ollama serve",
        ] {
            assert!(OllamaDriver.recognises_process(command_line), "{command_line}");
        }
    }

    #[test]
    fn does_not_recognise_non_ollama_serve_processes() {
        for command_line in [
            "python3 -m http.server 8090",
            "ollama run llama3",
            "/usr/bin/ollama-helper serve",
            "/Applications/Ollama.app/Contents/MacOS/Ollama",
            "grep ollama serve",
            "",
        ] {
            assert!(!OllamaDriver.recognises_process(command_line), "{command_line}");
        }
    }

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
    fn modelfile_newline_in_base_key_cannot_inject_directive() {
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
        assert!(out.contains("SYSTEM \"\"\""));
        assert!(!out.contains("\\\\\\\\"), "should not double-escape backslashes");
    }
}
