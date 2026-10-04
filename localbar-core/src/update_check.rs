use serde::{Deserialize, Serialize};

pub const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;
pub const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/CapitalCantrip/LocalBar/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    numbers: [u64; 3],
    is_release: bool,
}

pub fn parse_version(text: &str) -> Option<Version> {
    let trimmed = text.trim();
    let bare = trimmed.strip_prefix('v').or_else(|| trimmed.strip_prefix('V')).unwrap_or(trimmed);
    let bare = bare.split('+').next().unwrap_or(bare);
    let (core, pre) = match bare.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (bare, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut numbers = [0u64; 3];
    for (slot, part) in numbers.iter_mut().zip(&parts) {
        *slot = part.parse().ok()?;
    }
    Some(Version { numbers, is_release: pre.is_none() })
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate.is_release && candidate > current,
        _ => false,
    }
}

pub fn is_check_due(last_checked_secs: Option<i64>, now_secs: i64) -> bool {
    match last_checked_secs {
        None => true,
        Some(last) => now_secs < last || now_secs - last >= CHECK_INTERVAL_SECS,
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LatestRelease {
    pub tag_name: String,
    pub html_url: String,
}

pub fn parse_latest_release(json: &str) -> Result<LatestRelease, String> {
    serde_json::from_str(json).map_err(|e| format!("release JSON: {e}"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UpdateCheckOutcome {
    UpToDate,
    Available { version: String, url: String },
    Failed,
}

pub fn check_for_update(current: &str, fetch: impl FnOnce() -> Result<String, String>) -> UpdateCheckOutcome {
    let release = match fetch().and_then(|body| parse_latest_release(&body)) {
        Ok(release) => release,
        Err(_) => return UpdateCheckOutcome::Failed,
    };
    if parse_version(&release.tag_name).is_none() {
        return UpdateCheckOutcome::Failed;
    }
    if is_newer(&release.tag_name, current) {
        let version = release.tag_name.trim().trim_start_matches(['v', 'V']).to_owned();
        UpdateCheckOutcome::Available { version, url: release.html_url }
    } else {
        UpdateCheckOutcome::UpToDate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_JSON: &str = r#"{"tag_name":"v0.3.3","html_url":"https://github.com/CapitalCantrip/LocalBar/releases/tag/v0.3.3","draft":false}"#;

    #[test]
    fn a_v_prefix_is_ignored_when_comparing() {
        assert!(is_newer("v0.3.3", "0.3.2"));
        assert!(!is_newer("v0.3.2", "0.3.2"));
    }

    #[test]
    fn numeric_parts_compare_as_numbers_not_text() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.9.9", "0.10.0"));
    }

    #[test]
    fn a_major_bump_outranks_larger_minor_and_patch() {
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn missing_parts_count_as_zero() {
        assert!(!is_newer("v0.4", "0.4.0"));
        assert!(is_newer("v0.4", "0.3.9"));
    }

    #[test]
    fn a_pre_release_is_never_newer_than_a_release() {
        assert!(!is_newer("v0.4.0-beta.1", "0.3.2"));
    }

    #[test]
    fn a_release_is_newer_than_its_own_pre_release() {
        assert!(is_newer("0.4.0", "0.4.0-rc.1"));
    }

    #[test]
    fn unparseable_versions_are_never_newer() {
        assert!(!is_newer("latest", "0.3.2"));
        assert!(!is_newer("v0.3.3", "dev"));
        assert!(!is_newer("1.2.3.4", "0.3.2"));
    }

    #[test]
    fn a_check_is_due_when_never_checked() {
        assert!(is_check_due(None, 1_000));
    }

    #[test]
    fn a_check_is_not_due_within_a_day_of_the_last_one() {
        assert!(!is_check_due(Some(1_000), 1_000 + CHECK_INTERVAL_SECS - 1));
    }

    #[test]
    fn a_check_is_due_a_day_after_the_last_one() {
        assert!(is_check_due(Some(1_000), 1_000 + CHECK_INTERVAL_SECS));
    }

    #[test]
    fn a_check_is_due_when_the_clock_moved_backwards() {
        assert!(is_check_due(Some(1_000), 999));
    }

    #[test]
    fn release_json_yields_the_tag_and_page_url() {
        let release = parse_latest_release(RELEASE_JSON).unwrap();
        assert_eq!(release.tag_name, "v0.3.3");
        assert_eq!(release.html_url, "https://github.com/CapitalCantrip/LocalBar/releases/tag/v0.3.3");
    }

    #[test]
    fn release_json_without_a_tag_is_an_error() {
        assert!(parse_latest_release(r#"{"html_url":"https://github.com/CapitalCantrip/LocalBar/releases"}"#).is_err());
    }

    #[test]
    fn a_newer_release_reports_the_version_without_the_v_and_its_page() {
        let outcome = check_for_update("0.3.2", || Ok(RELEASE_JSON.to_owned()));
        assert_eq!(outcome, UpdateCheckOutcome::Available {
            version: "0.3.3".into(),
            url: "https://github.com/CapitalCantrip/LocalBar/releases/tag/v0.3.3".into(),
        });
    }

    #[test]
    fn the_same_release_reports_up_to_date() {
        assert_eq!(check_for_update("0.3.3", || Ok(RELEASE_JSON.to_owned())), UpdateCheckOutcome::UpToDate);
    }

    #[test]
    fn a_fetch_error_reports_failed() {
        assert_eq!(check_for_update("0.3.2", || Err("timeout".into())), UpdateCheckOutcome::Failed);
    }

    #[test]
    fn a_garbled_response_reports_failed() {
        assert_eq!(check_for_update("0.3.2", || Ok("<html>".into())), UpdateCheckOutcome::Failed);
    }

    #[test]
    fn an_unparseable_tag_reports_failed_rather_than_up_to_date() {
        let json = r#"{"tag_name":"nightly","html_url":"https://example.com"}"#;
        assert_eq!(check_for_update("0.3.2", || Ok(json.to_owned())), UpdateCheckOutcome::Failed);
    }
}
