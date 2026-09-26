use crate::drivers::external::{models_url, parse_openai_models};
use crate::types::{ServerInstanceConfig, ServerType};

pub fn reports_loaded_model(server_type: ServerType) -> bool {
    matches!(server_type, ServerType::MlxLm)
}

pub fn detect_loaded_model(config: &ServerInstanceConfig) -> Option<String> {
    let resp = crate::drivers::http::quick_agent()
        .get(&models_url(config))
        .call()
        .ok()?;
    let json: serde_json::Value = resp.into_json().ok()?;
    parse_openai_models(&json).into_iter().next().map(|m| m.key)
}

pub fn model_matches(candidate: &str, reported: &str) -> bool {
    if candidate == reported {
        return true;
    }
    if path_boundary_match(candidate, reported) {
        return true;
    }
    hf_repo_from_cache_path(reported).as_deref() == Some(candidate)
        || hf_repo_from_cache_path(candidate).as_deref() == Some(reported)
}

fn path_boundary_match(a: &str, b: &str) -> bool {
    let (longer, shorter) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    longer.ends_with(shorter) && longer[..longer.len() - shorter.len()].ends_with('/')
}

fn hf_repo_from_cache_path(s: &str) -> Option<String> {
    s.split('/').find_map(hf_repo_from_cache_component)
}

fn hf_repo_from_cache_component(component: &str) -> Option<String> {
    let rest = component.strip_prefix("models--")?;
    let sep = rest.find("--")?;
    let (org, repo) = (&rest[..sep], &rest[sep + 2..]);
    Some(format!("{org}/{repo}"))
}

pub fn best_matching_key(reported: &str, candidates: &[String]) -> Option<String> {
    candidates.iter().find(|c| model_matches(c, reported)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_loaded_model_true_for_mlx_lm() {
        assert!(reports_loaded_model(ServerType::MlxLm));
    }

    #[test]
    fn reports_loaded_model_false_for_ollama_and_external() {
        assert!(!reports_loaded_model(ServerType::Ollama));
        assert!(!reports_loaded_model(ServerType::External));
    }

    #[test]
    fn model_matches_exact_key() {
        assert!(model_matches(
            "mlx-community/Qwen3.5-4B-OptiQ-4bit",
            "mlx-community/Qwen3.5-4B-OptiQ-4bit"
        ));
    }

    #[test]
    fn model_matches_when_reported_is_local_path_ending_in_key() {
        let candidate = "mlx-community/Qwen3.5-4B-OptiQ-4bit";
        let reported = "/Users/x/SharedModels/mlx-community/Qwen3.5-4B-OptiQ-4bit";
        assert!(model_matches(candidate, reported));
    }

    #[test]
    fn model_matches_when_candidate_is_path_and_reported_is_suffix() {
        let candidate = "/Users/x/SharedModels/mlx-community/Qwen3.5-4B-OptiQ-4bit";
        let reported = "mlx-community/Qwen3.5-4B-OptiQ-4bit";
        assert!(model_matches(candidate, reported));
    }

    #[test]
    fn model_matches_hf_repo_id_against_resolved_cache_snapshot_path() {
        let candidate = "mlx-community/Qwen3.5-4B-OptiQ-4bit";
        let reported = "/Users/x/.cache/huggingface/hub/models--mlx-community--Qwen3.5-4B-OptiQ-4bit/snapshots/abc123";
        assert!(model_matches(candidate, reported), "must recognise the HF cache models--org--repo naming convention");
    }

    #[test]
    fn model_matches_false_for_unrelated_models() {
        assert!(!model_matches("mlx-community/Qwen3.5-4B-OptiQ-4bit", "mlx-community/Other-Model-4bit"));
    }

    #[test]
    fn model_matches_false_for_unrelated_cache_paths() {
        let candidate = "mlx-community/Qwen3.5-4B-OptiQ-4bit";
        let reported = "/Users/x/.cache/huggingface/hub/models--mlx-community--Other-Model-4bit/snapshots/abc123";
        assert!(!model_matches(candidate, reported));
    }

    #[test]
    fn model_matches_false_for_substring_that_is_not_on_a_path_boundary() {
        assert!(!model_matches("Qwen3-4B", "mlx-community/Some-Qwen3-4B"));
        assert!(!model_matches("tinyllama", "llama"));
    }

    #[test]
    fn best_matching_key_finds_the_right_candidate() {
        let candidates = vec![
            "mlx-community/Llama-3-8B-4bit".to_string(),
            "mlx-community/Mistral-7B-4bit".to_string(),
        ];
        let reported = "/models/mlx-community/Mistral-7B-4bit";
        assert_eq!(
            best_matching_key(reported, &candidates),
            Some("mlx-community/Mistral-7B-4bit".to_string())
        );
    }

    #[test]
    fn best_matching_key_none_when_nothing_matches() {
        let candidates = vec!["mlx-community/Llama-3-8B-4bit".to_string()];
        assert_eq!(best_matching_key("totally-unrelated", &candidates), None);
    }
}
