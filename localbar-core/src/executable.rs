use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const PATH_MARKER_START: &str = "__LOCALBAR_PATH_START__";
pub const PATH_MARKER_END: &str = "__LOCALBAR_PATH_END__";
pub const PATH_ENV_VAR: &str = "PATH";
pub const SHELL_ENV_VAR: &str = "SHELL";
pub const INTERACTIVE_LOGIN_SHELL_ARGS: [&str; 3] = ["-i", "-l", "-c"];
pub const LOGIN_SHELL_ARGS: [&str; 2] = ["-l", "-c"];

const MACOS_DEFAULT_SHELL: &str = "/bin/zsh";
const UNIX_DEFAULT_SHELL: &str = "/bin/sh";
const FISH_SHELL_NAME: &str = "fish";
const PATH_SEPARATOR: char = ':';

const HOME_RELATIVE_FALLBACKS: [&str; 5] = [
    ".local/bin",
    ".cargo/bin",
    ".pyenv/shims",
    ".asdf/shims",
    ".local/share/mise/shims",
];
const MACOS_SYSTEM_FALLBACKS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
const LINUX_SYSTEM_FALLBACKS: [&str; 2] = ["/usr/local/bin", "/home/linuxbrew/.linuxbrew/bin"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    MacOs,
    Linux,
}

impl TargetOs {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") { TargetOs::MacOs } else { TargetOs::Linux }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFlavor {
    Posix,
    Fish,
}

pub fn shell_flavor(shell: &str) -> ShellFlavor {
    let name = Path::new(shell).file_name().and_then(|n| n.to_str()).unwrap_or(shell);
    if name == FISH_SHELL_NAME { ShellFlavor::Fish } else { ShellFlavor::Posix }
}

pub fn login_shell(env_shell: Option<&str>, os: TargetOs) -> String {
    match env_shell.map(str::trim).filter(|s| !s.is_empty()) {
        Some(shell) => shell.to_string(),
        None => match os {
            TargetOs::MacOs => MACOS_DEFAULT_SHELL.to_string(),
            TargetOs::Linux => UNIX_DEFAULT_SHELL.to_string(),
        },
    }
}

pub fn path_print_script(flavor: ShellFlavor) -> String {
    match flavor {
        ShellFlavor::Posix => format!("printf '%s%s%s' '{PATH_MARKER_START}' \"$PATH\" '{PATH_MARKER_END}'"),
        ShellFlavor::Fish => format!("printf '%s%s%s' '{PATH_MARKER_START}' (string join : $PATH) '{PATH_MARKER_END}'"),
    }
}

pub fn parse_marked_path(output: &str, flavor: ShellFlavor) -> Option<Vec<String>> {
    let after_start = &output[output.rfind(PATH_MARKER_START)? + PATH_MARKER_START.len()..];
    let body = &after_start[..after_start.find(PATH_MARKER_END)?];
    let body = body.trim();
    let entries: Vec<String> = if flavor == ShellFlavor::Fish && !body.contains(PATH_SEPARATOR) {
        body.split_whitespace().map(str::to_string).collect()
    } else {
        split_path(body)
    };
    (!entries.is_empty()).then_some(entries)
}

pub fn split_path(value: &str) -> Vec<String> {
    value.split(PATH_SEPARATOR).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

pub fn join_path(dirs: &[String]) -> String {
    dirs.join(&PATH_SEPARATOR.to_string())
}

pub fn fallback_dirs(home: Option<&str>, os: TargetOs) -> Vec<String> {
    let home = home.map(|h| h.trim_end_matches('/')).filter(|h| !h.is_empty());
    let system: &[&str] = match os {
        TargetOs::MacOs => &MACOS_SYSTEM_FALLBACKS,
        TargetOs::Linux => &LINUX_SYSTEM_FALLBACKS,
    };
    let home_dirs = home.into_iter().flat_map(|h| HOME_RELATIVE_FALLBACKS.iter().map(move |rel| format!("{h}/{rel}")));
    let mut dirs: Vec<String> = Vec::new();
    let mut home_iter = home_dirs.peekable();
    if let Some(first) = home_iter.next() {
        dirs.push(first);
    }
    dirs.extend(system.iter().map(|s| s.to_string()));
    dirs.extend(home_iter);
    dirs
}

pub fn merge_search_path(shell: Option<&[String]>, process: &[String], fallbacks: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    let all = shell.unwrap_or_default().iter().chain(process).chain(fallbacks);
    for dir in all {
        if !dir.is_empty() && !merged.contains(dir) {
            merged.push(dir.clone());
        }
    }
    merged
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableNotFound {
    pub command: String,
    pub searched: Vec<String>,
}

impl ExecutableNotFound {
    pub fn message(&self) -> String {
        format!("Couldn't find `{}`. Looked in: {}", self.command, self.searched.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
    pub program: PathBuf,
    pub path_env: String,
}

pub type CommandResolver = Arc<dyn Fn(&str) -> Result<ResolvedCommand, ExecutableNotFound> + Send + Sync>;

pub fn resolve_executable(
    configured: &str,
    search: &[String],
    is_executable: impl Fn(&Path) -> bool,
) -> Result<PathBuf, ExecutableNotFound> {
    if configured.contains('/') {
        let path = PathBuf::from(configured);
        return if is_executable(&path) {
            Ok(path)
        } else {
            Err(ExecutableNotFound { command: configured.to_string(), searched: vec![configured.to_string()] })
        };
    }
    search
        .iter()
        .map(|dir| Path::new(dir).join(configured))
        .find(|candidate| is_executable(candidate))
        .ok_or_else(|| ExecutableNotFound { command: configured.to_string(), searched: search.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn marked_path_is_extracted_from_surrounding_profile_noise() {
        let output = format!("Welcome!\nnvm loaded\n{PATH_MARKER_START}/a/bin:/b/bin{PATH_MARKER_END}\nbye\n");
        assert_eq!(parse_marked_path(&output, ShellFlavor::Posix), Some(strings(&["/a/bin", "/b/bin"])));
    }

    #[test]
    fn the_last_start_marker_wins_when_profile_echoes_the_marker() {
        let output = format!("{PATH_MARKER_START}junk {PATH_MARKER_START}/x:/y{PATH_MARKER_END}");
        assert_eq!(parse_marked_path(&output, ShellFlavor::Posix), Some(strings(&["/x", "/y"])));
    }

    #[test]
    fn output_without_markers_yields_no_path() {
        assert_eq!(parse_marked_path("/usr/bin:/bin", ShellFlavor::Posix), None);
        assert_eq!(parse_marked_path(&format!("{PATH_MARKER_START}/usr/bin"), ShellFlavor::Posix), None);
    }

    #[test]
    fn an_empty_marked_path_yields_no_path() {
        assert_eq!(parse_marked_path(&format!("{PATH_MARKER_START}{PATH_MARKER_END}"), ShellFlavor::Posix), None);
    }

    #[test]
    fn fish_space_separated_path_is_split_on_whitespace() {
        let output = format!("{PATH_MARKER_START}/Users/me/.local/bin /opt/homebrew/bin /usr/bin{PATH_MARKER_END}");
        assert_eq!(
            parse_marked_path(&output, ShellFlavor::Fish),
            Some(strings(&["/Users/me/.local/bin", "/opt/homebrew/bin", "/usr/bin"]))
        );
    }

    #[test]
    fn fish_colon_joined_path_keeps_entries_containing_spaces() {
        let output = format!("{PATH_MARKER_START}/Users/me/My Tools/bin:/usr/bin{PATH_MARKER_END}");
        assert_eq!(parse_marked_path(&output, ShellFlavor::Fish), Some(strings(&["/Users/me/My Tools/bin", "/usr/bin"])));
    }

    #[test]
    fn fish_script_joins_path_with_colons_and_posix_script_quotes_path() {
        assert!(path_print_script(ShellFlavor::Fish).contains("(string join : $PATH)"));
        assert!(path_print_script(ShellFlavor::Posix).contains("\"$PATH\""));
        for flavor in [ShellFlavor::Fish, ShellFlavor::Posix] {
            let script = path_print_script(flavor);
            assert!(script.contains(PATH_MARKER_START) && script.contains(PATH_MARKER_END));
        }
    }

    #[test]
    fn shell_flavor_is_fish_only_for_a_fish_binary() {
        assert_eq!(shell_flavor("/opt/homebrew/bin/fish"), ShellFlavor::Fish);
        assert_eq!(shell_flavor("fish"), ShellFlavor::Fish);
        assert_eq!(shell_flavor("/bin/zsh"), ShellFlavor::Posix);
        assert_eq!(shell_flavor("/bin/bash"), ShellFlavor::Posix);
        assert_eq!(shell_flavor("/usr/bin/selfish"), ShellFlavor::Posix);
    }

    #[test]
    fn login_shell_falls_back_to_the_platform_default_when_shell_is_unset() {
        assert_eq!(login_shell(Some("/opt/homebrew/bin/fish"), TargetOs::MacOs), "/opt/homebrew/bin/fish");
        assert_eq!(login_shell(None, TargetOs::MacOs), "/bin/zsh");
        assert_eq!(login_shell(Some("  "), TargetOs::Linux), "/bin/sh");
    }

    #[test]
    fn merged_path_puts_shell_entries_first_then_process_then_fallbacks_without_duplicates() {
        let shell = strings(&["/s1", "/usr/bin", "/s2"]);
        let process = strings(&["/usr/bin", "/bin", "/s1"]);
        let fallbacks = strings(&["/f1", "/bin", "/s2"]);
        assert_eq!(
            merge_search_path(Some(&shell), &process, &fallbacks),
            strings(&["/s1", "/usr/bin", "/s2", "/bin", "/f1"])
        );
    }

    #[test]
    fn fallbacks_are_still_searched_when_the_shell_read_failed() {
        let process = strings(&["/usr/bin", "/bin"]);
        let fallbacks = strings(&["/home/me/.local/bin"]);
        assert_eq!(merge_search_path(None, &process, &fallbacks), strings(&["/usr/bin", "/bin", "/home/me/.local/bin"]));
    }

    #[test]
    fn macos_fallbacks_include_local_bin_and_homebrew_but_not_linuxbrew() {
        let dirs = fallback_dirs(Some("/Users/me/"), TargetOs::MacOs);
        assert_eq!(dirs[0], "/Users/me/.local/bin");
        for d in ["/opt/homebrew/bin", "/usr/local/bin", "/Users/me/.cargo/bin", "/Users/me/.pyenv/shims", "/Users/me/.asdf/shims", "/Users/me/.local/share/mise/shims"] {
            assert!(dirs.contains(&d.to_string()), "{d} missing from {dirs:?}");
        }
        assert!(!dirs.iter().any(|d| d.contains("linuxbrew")));
    }

    #[test]
    fn linux_fallbacks_include_linuxbrew_but_not_opt_homebrew() {
        let dirs = fallback_dirs(Some("/home/me"), TargetOs::Linux);
        assert!(dirs.contains(&"/home/linuxbrew/.linuxbrew/bin".to_string()));
        assert!(!dirs.contains(&"/opt/homebrew/bin".to_string()));
    }

    #[test]
    fn fallbacks_without_a_home_contain_only_system_folders() {
        assert_eq!(fallback_dirs(None, TargetOs::MacOs), strings(&["/opt/homebrew/bin", "/usr/local/bin"]));
    }

    #[test]
    fn bare_command_resolves_to_the_first_search_folder_that_has_it() {
        let search = strings(&["/a", "/b", "/c"]);
        let found = resolve_executable("uvx", &search, |p| p == Path::new("/b/uvx") || p == Path::new("/c/uvx"));
        assert_eq!(found, Ok(PathBuf::from("/b/uvx")));
    }

    #[test]
    fn bare_command_not_in_any_folder_reports_every_folder_searched() {
        let search = strings(&["/a", "/b"]);
        let err = resolve_executable("uvx", &search, |_| false).unwrap_err();
        assert_eq!(err, ExecutableNotFound { command: "uvx".into(), searched: search.clone() });
        assert_eq!(err.message(), "Couldn't find `uvx`. Looked in: /a, /b");
    }

    #[test]
    fn value_containing_a_slash_is_used_as_is_without_searching() {
        let search = strings(&["/a"]);
        let found = resolve_executable("/opt/x/ollama", &search, |p| p == Path::new("/opt/x/ollama"));
        assert_eq!(found, Ok(PathBuf::from("/opt/x/ollama")));
        let relative = resolve_executable("bin/uvx", &search, |p| p == Path::new("/a/bin/uvx"));
        assert!(relative.is_err());
    }

    #[test]
    fn missing_slash_value_reports_only_that_path() {
        let err = resolve_executable("/nope/uvx", &strings(&["/a", "/b"]), |_| false).unwrap_err();
        assert_eq!(err.searched, strings(&["/nope/uvx"]));
    }

    #[test]
    fn join_and_split_path_round_trip() {
        let dirs = strings(&["/a", "/b c"]);
        assert_eq!(split_path(&join_path(&dirs)), dirs);
        assert_eq!(split_path("/a::/b:"), strings(&["/a", "/b"]));
    }
}
