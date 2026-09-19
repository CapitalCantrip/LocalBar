use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::driver::{HealthStatus, LaunchPlan, ModelMetadata, ServerDriver, ShutdownPlan};
use crate::types::{
    CanonicalParam, ModelRef, ParamDescriptor, ParamValue, ParamValues, ServerInstanceConfig,
    ServerType,
};

/// Driver for mlx_lm.server.
/// `search_paths` is the ordered list of directories to scan for HF-cached models.
/// Empty = derive from HF_HOME or HOME at list time (consistent with ADR D10: global, not per-instance).
pub struct MLXLMDriver {
    pub search_paths: Vec<String>,
}

impl MLXLMDriver {
    pub fn new(search_paths: Vec<String>) -> Self {
        Self { search_paths }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for MLXLMDriver {
    fn default() -> Self {
        Self { search_paths: Vec::new() }
    }
}

impl ServerDriver for MLXLMDriver {
    fn server_type(&self) -> ServerType {
        ServerType::MlxLm
    }

    fn param_schema(&self) -> Vec<ParamDescriptor> {
        vec![
            ParamDescriptor { param: CanonicalParam::ContextLength,   server_flag_name: "--max-kv-size",         modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::Temperature,     server_flag_name: "--temp",                modelfile_param_name: None, default_value: Some(ParamValue::Double(0.0)) },
            ParamDescriptor { param: CanonicalParam::MaxTokens,       server_flag_name: "--max-tokens",          modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::TopK,            server_flag_name: "--top-k",               modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::RepeatPenalty,   server_flag_name: "--repetition-penalty",  modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::PresencePenalty, server_flag_name: "--presence-penalty",    modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::TopP,            server_flag_name: "--top-p",               modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::MinP,            server_flag_name: "--min-p",               modelfile_param_name: None, default_value: None },
            ParamDescriptor { param: CanonicalParam::Seed,            server_flag_name: "--seed",                modelfile_param_name: None, default_value: None },
        ]
    }

    fn launch(
        &self,
        config: &ServerInstanceConfig,
        model: Option<&ModelRef>,
        params: &ParamValues,
    ) -> Result<LaunchPlan, String> {
        let model_key = model
            .map(|m| m.key.as_str())
            .or(config.selected_model_key.as_deref())
            .ok_or("mlx-lm requires a model to launch")?;
        let arguments = build_launch_args(config, model_key, params, &self.param_schema());
        Ok(LaunchPlan {
            executable: config.executable_path.clone(),
            arguments,
            environment: Vec::new(),
            working_directory: None,
        })
    }

    fn stop(&self, _config: &ServerInstanceConfig) -> ShutdownPlan {
        ShutdownPlan { grace_period_secs: 5.0 }
    }

    fn health_check(&self, config: &ServerInstanceConfig) -> HealthStatus {
        let url = format!("{}/health", super::http::base_url(config));
        match super::http::quick_agent().get(&url).call() {
            Ok(_) => HealthStatus::Healthy,
            Err(ureq::Error::Status(_, _)) => {
                HealthStatus::Unhealthy("unexpected status from /health".to_string())
            }
            Err(_) => HealthStatus::Unreachable,
        }
    }

    fn list_models(&self, _config: &ServerInstanceConfig) -> Result<Vec<ModelRef>, String> {
        let search_paths = effective_search_paths(&self.search_paths);
        let mut models = Vec::new();
        let mut seen_keys = std::collections::HashSet::new();
        for root in &search_paths {
            scan_hf_root(Path::new(root), &mut models, &mut seen_keys);
        }
        Ok(models)
    }

    fn switch_model(
        &self,
        _model: &ModelRef,
        _params: &ParamValues,
        _config: &ServerInstanceConfig,
    ) -> Result<(), String> {
        Err("mlx-lm requires a server restart to switch models".to_string())
    }

    fn fetch_model_metadata(
        &self,
        model_key: &str,
        _config: &ServerInstanceConfig,
    ) -> Option<ModelMetadata> {
        let config_json = locate_config_json(model_key, &self.search_paths);
        let (parameter_count, quantization) = if let Some(path) = config_json {
            parse_metadata_from_config(model_key, &path)
        } else {
            (parse_parameter_count(model_key), parse_quantization(model_key))
        };
        Some(ModelMetadata { parameter_count, quantization })
    }
}

// ─── Launch helpers ───────────────────────────────────────────────────────────

fn build_launch_args(
    config: &ServerInstanceConfig,
    model_key: &str,
    params: &ParamValues,
    schema: &[ParamDescriptor],
) -> Vec<String> {
    let is_uvx = config.executable_path.ends_with("/uvx") || config.executable_path == "uvx";
    let mut args: Vec<String> = if is_uvx {
        vec!["--from".into(), "mlx-lm".into(), "mlx_lm.server".into()]
    } else {
        vec!["-m".into(), "mlx_lm.server".into()]
    };
    args.extend(["--model".into(), model_key.to_string(), "--host".into(), config.host.clone(), "--port".into(), config.port.to_string()]);
    for desc in schema {
        let Some(value) = params.values.get(&desc.param) else { continue };
        append_param_arg(desc.server_flag_name, value, &mut args);
    }
    args
}

fn append_param_arg(flag: &str, value: &ParamValue, args: &mut Vec<String>) {
    match value {
        ParamValue::Bool(b) => { if *b { args.push(flag.to_string()); } }
        ParamValue::Double(d) => { args.push(flag.to_string()); args.push(d.to_string()); }
        ParamValue::Int(i) => { args.push(flag.to_string()); args.push(i.to_string()); }
        ParamValue::String(s) => { args.push(flag.to_string()); args.push(s.clone()); }
    }
}

// ─── List-models helper ───────────────────────────────────────────────────────

fn scan_hf_root(
    root_path: &Path,
    models: &mut Vec<ModelRef>,
    seen_keys: &mut std::collections::HashSet<String>,
) {
    if !root_path.is_dir() { return; }
    let Ok(entries) = fs::read_dir(root_path) else { return };
    for entry in entries.flatten() {
        let dir_name = entry.file_name();
        let dir_name_str = dir_name.to_string_lossy();
        if !dir_name_str.starts_with("models--") { continue; }
        let model_dir = entry.path();
        let snapshots_dir = model_dir.join("snapshots");
        if !snapshots_dir.is_dir() { continue; }
        let Some(snapshot_dir) = canonical_snapshot(&model_dir, &snapshots_dir) else { continue };
        if !snapshot_dir.join("config.json").exists() { continue; }
        let model_key = hf_dir_to_model_key(&dir_name_str);
        if seen_keys.contains(&model_key) { continue; }
        seen_keys.insert(model_key.clone());
        models.push(ModelRef { display_name: model_key.clone(), key: model_key, size_bytes: None });
    }
}

// ─── HF cache helpers ─────────────────────────────────────────────────────────

/// `models--org--name` → `org/name`
pub fn hf_dir_to_model_key(dir_name: &str) -> String {
    dir_name
        .strip_prefix("models--")
        .unwrap_or(dir_name)
        .replacen("--", "/", 1)
        .replace("--", "/")
}

/// Pick the canonical snapshot directory for a given model dir.
/// Prefers the hash written in `refs/main`; falls back to latest mtime.
pub fn canonical_snapshot(model_dir: &Path, snapshots_dir: &Path) -> Option<PathBuf> {
    let refs_main = model_dir.join("refs").join("main");
    if refs_main.exists() {
        if let Ok(hash) = fs::read_to_string(&refs_main) {
            let hash = hash.trim();
            if !hash.is_empty() {
                let candidate = snapshots_dir.join(hash);
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }
    // Fall back to snapshot with latest mtime.
    fs::read_dir(snapshots_dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .max_by_key(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
        .map(|e| e.path())
}

fn effective_search_paths(configured: &[String]) -> Vec<String> {
    let mut paths = configured.to_vec();
    let hf_default = hf_cache_default();
    if let Some(default) = hf_default {
        if !paths.contains(&default) {
            paths.push(default);
        }
    }
    paths
}

/// Returns `$HF_HOME` if set, or `$HOME/.cache/huggingface/hub` if HOME is set,
/// or None if neither is set (e.g. daemon context without HOME).
fn hf_cache_default() -> Option<String> {
    if let Ok(hf) = std::env::var("HF_HOME") {
        return Some(hf);
    }
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() { return None; }
    Some(format!("{}/.cache/huggingface/hub", home))
}

fn locate_config_json(model_key: &str, search_paths: &[String]) -> Option<PathBuf> {
    let dir_name = format!("models--{}", model_key.replace('/', "--"));
    for root in effective_search_paths(search_paths) {
        let model_dir = Path::new(&root).join(&dir_name);
        let snapshots_dir = model_dir.join("snapshots");
        if !snapshots_dir.is_dir() {
            continue;
        }
        if let Some(snapshot) = canonical_snapshot(&model_dir, &snapshots_dir) {
            let p = snapshot.join("config.json");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

// ─── Metadata parsing ─────────────────────────────────────────────────────────

fn parse_metadata_from_config(model_key: &str, config_path: &Path) -> (Option<String>, Option<String>) {
    let Ok(data) = fs::read_to_string(config_path) else {
        return (parse_parameter_count(model_key), parse_quantization(model_key));
    };
    let json: Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(_) => return (parse_parameter_count(model_key), parse_quantization(model_key)),
    };
    // config.json rarely has a "quantization" field; derive from name when absent.
    let quantization = json["quantization_config"]["quant_type"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| parse_quantization(model_key));
    // num_parameters is non-standard; use name-based heuristic.
    let parameter_count = parse_parameter_count(model_key);
    (parameter_count, quantization)
}

/// Matches tokens like "7B", "1.5B", "0.5b" in a path string.
pub fn parse_parameter_count(text: &str) -> Option<String> {
    for token in split_tokens(text) {
        let lower = token.to_lowercase();
        if !lower.ends_with('b') || lower.len() < 2 {
            continue;
        }
        let digits = &lower[..lower.len() - 1];
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
            continue;
        }
        if digits.parse::<f64>().is_ok() {
            return Some(format!("{}B", digits.to_uppercase()));
        }
    }
    None
}

/// Matches "4bit", "8bit", "4-bit", GGUF-style "Q4_K_M", or float dtypes.
pub fn parse_quantization(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    // N-bit variants
    for bits in ["4", "8", "2", "3", "5", "6"] {
        if lower.contains(&format!("{}bit", bits)) || lower.contains(&format!("{}-bit", bits)) {
            return Some(format!("{}bit", bits));
        }
    }

    // Float dtypes — check longer tokens first
    let tokens: Vec<&str> = split_tokens(&lower).collect();
    for dtype in ["bf16", "fp16", "f16"] {
        if tokens.contains(&dtype) {
            return Some(dtype.to_string());
        }
    }

    // GGUF-style: q4_k_m etc.
    for token in split_tokens(&lower) {
        if token.starts_with('q')
            && token.len() >= 2
            && token.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
            && token.contains('_')
        {
            return Some(token.to_uppercase());
        }
    }

    None
}

fn split_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric() && c != '.')
        .filter(|s| !s.is_empty())
}


// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Display name derivation ───────────────────────────────────────────────

    #[test]
    fn hf_dir_to_key_standard_org_name() {
        assert_eq!(
            hf_dir_to_model_key("models--mlx-community--Qwen2.5-7B-Instruct-4bit"),
            "mlx-community/Qwen2.5-7B-Instruct-4bit"
        );
    }

    #[test]
    fn hf_dir_to_key_simple() {
        assert_eq!(hf_dir_to_model_key("models--org--name"), "org/name");
    }

    #[test]
    fn hf_dir_to_key_three_component_repo() {
        // models--a--b--c: first -- is the separator, rest stay as /
        assert_eq!(hf_dir_to_model_key("models--a--b--c"), "a/b/c");
    }

    // ── Snapshot deduplication ────────────────────────────────────────────────

    fn make_snapshot(root: &Path, hash: &str) -> PathBuf {
        let dir = root.join(hash);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), r#"{"model_type":"llama"}"#).unwrap();
        dir
    }

    #[test]
    fn canonical_snapshot_prefers_refs_main() {
        let tmp = TempDir::new().unwrap();
        let model_dir = tmp.path().join("models--org--name");
        let snapshots_dir = model_dir.join("snapshots");
        let refs_dir = model_dir.join("refs");
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::create_dir_all(&refs_dir).unwrap();

        make_snapshot(&snapshots_dir, "aaa111");
        make_snapshot(&snapshots_dir, "bbb222");
        fs::write(refs_dir.join("main"), "bbb222\n").unwrap();

        let result = canonical_snapshot(&model_dir, &snapshots_dir).unwrap();
        assert_eq!(result.file_name().unwrap(), "bbb222");
    }

    #[test]
    fn canonical_snapshot_falls_back_when_no_refs_main() {
        // Tests that a valid snapshot is returned when refs/main is absent.
        // Mtime ordering between snapshots is not asserted here because
        // sub-second mtime resolution is not guaranteed on all filesystems.
        let tmp = TempDir::new().unwrap();
        let model_dir = tmp.path().join("models--org--name");
        let snapshots_dir = model_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir).unwrap();

        make_snapshot(&snapshots_dir, "abc123");

        let result = canonical_snapshot(&model_dir, &snapshots_dir).unwrap();
        // The sole snapshot must be returned when no refs/main exists.
        assert_eq!(result.file_name().unwrap(), "abc123");
    }

    // ── Two snapshots → one entry ─────────────────────────────────────────────

    #[test]
    fn list_models_two_snapshots_returns_one_entry() {
        let tmp = TempDir::new().unwrap();
        let hub_root = tmp.path().to_str().unwrap().to_string();

        let model_dir = tmp.path().join("models--mlx-community--Qwen2.5-7B-Instruct-4bit");
        let snapshots_dir = model_dir.join("snapshots");
        let refs_dir = model_dir.join("refs");
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::create_dir_all(&refs_dir).unwrap();
        make_snapshot(&snapshots_dir, "hash1111");
        make_snapshot(&snapshots_dir, "hash2222");
        fs::write(refs_dir.join("main"), "hash2222").unwrap();

        let driver = driver_for(hub_root);
        let config = dummy_config();
        let models = driver.list_models(&config).unwrap();
        assert_eq!(models.len(), 1, "two snapshots must yield one model entry");
        assert_eq!(models[0].key, "mlx-community/Qwen2.5-7B-Instruct-4bit");
        assert_eq!(models[0].display_name, "mlx-community/Qwen2.5-7B-Instruct-4bit");
    }

    // ── Non-model directories with config.json are excluded ──────────────────

    #[test]
    fn list_models_spurious_config_json_excluded() {
        let tmp = TempDir::new().unwrap();
        let hub_root = tmp.path().to_str().unwrap().to_string();

        // A directory that has config.json but is NOT inside models--*/snapshots/*
        let spurious = tmp.path().join("my-project");
        fs::create_dir_all(&spurious).unwrap();
        fs::write(spurious.join("config.json"), r#"{"model_type":"llama"}"#).unwrap();

        // A proper HF model alongside
        let model_dir = tmp.path().join("models--org--model");
        let snapshots_dir = model_dir.join("snapshots");
        let refs_dir = model_dir.join("refs");
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::create_dir_all(&refs_dir).unwrap();
        make_snapshot(&snapshots_dir, "abc123");
        fs::write(refs_dir.join("main"), "abc123").unwrap();

        let driver = driver_for(hub_root);
        let config = dummy_config();
        let models = driver.list_models(&config).unwrap();
        assert_eq!(models.len(), 1, "spurious dir must not appear as a model");
        assert_eq!(models[0].key, "org/model");
    }

    // ── Metadata parsing ──────────────────────────────────────────────────────

    #[test]
    fn parse_parameter_count_standard_tokens() {
        assert_eq!(parse_parameter_count("Qwen2.5-7B-Instruct"), Some("7B".to_string()));
        assert_eq!(parse_parameter_count("Llama-3.1-70B"), Some("70B".to_string()));
        assert_eq!(parse_parameter_count("phi-1.5b"), Some("1.5B".to_string()));
    }

    #[test]
    fn parse_quantization_nbit() {
        assert_eq!(parse_quantization("Qwen2.5-7B-4bit"), Some("4bit".to_string()));
        assert_eq!(parse_quantization("model-8-bit"), Some("8bit".to_string()));
        assert_eq!(parse_quantization("model-bf16"), Some("bf16".to_string()));
    }

    #[test]
    fn parse_quantization_absent() {
        assert_eq!(parse_quantization("plain-model-7B"), None);
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn driver_for(search_path: String) -> MLXLMDriver {
        MLXLMDriver::new(vec![search_path])
    }

    fn dummy_config() -> ServerInstanceConfig {
        ServerInstanceConfig::new("test", ServerType::MlxLm, 8080, "/usr/bin/mlx")
    }
}
