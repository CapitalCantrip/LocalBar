use std::path::Path;
use std::time::Duration;

use crate::executable::TargetOs;

pub const MLX_LM_SERVER_SCRIPT: &str = "mlx_lm.server";
pub const MLX_LM_DISTRIBUTION: &str = "mlx-lm";
pub const MLX_LM_IMPORT_CHECK_ARGS: [&str; 2] = ["-c", "import mlx_lm"];
pub const UV_COMMAND: &str = "uv";
pub const UVX_COMMAND: &str = "uvx";
pub const BREW_COMMAND: &str = "brew";
pub const UV_TOOL_INSTALL_ARGS: [&str; 3] = ["tool", "install", MLX_LM_DISTRIBUTION];
pub const UV_OFFICIAL_INSTALLER_SCRIPT: &str = "curl -LsSf https://astral.sh/uv/install.sh | sh";
pub const OLLAMA_DOWNLOAD_URL: &str = "https://ollama.com/download";
pub const POSIX_SHELL: &str = "/bin/sh";
pub const POSIX_SHELL_COMMAND_FLAG: &str = "-c";
pub const MAX_ENTRIES_PER_GLOB: usize = 16;

const PYTHON_MODULE_FLAG: &str = "-m";
const UVX_FROM_FLAG: &str = "--from";
const UV_TOOL_RUN_ARGS: [&str; 2] = ["tool", "run"];
const PYTHON3_COMMAND: &str = "python3";
const SYSTEM_PYTHON: &str = "/usr/bin/python3";
pub const MAX_DETECTION_TIME: Duration = Duration::from_secs(30);
const HOMEBREW_PYTHON_FORMULAE_DIR: &str = "/opt/homebrew/opt";
const PYTHON_FORMULA_PREFIX: &str = "python";
const BIN_PYTHON3: &str = "bin/python3";
const HOME_FIXED_PYTHONS: [&str; 4] = [
    ".local/pipx/venvs/mlx-lm/bin/python",
    ".venv/bin/python",
    "venv/bin/python",
    "env/bin/python",
];
const HOME_ENV_PARENTS: [&str; 5] = [
    ".pyenv/versions",
    "miniconda3/envs",
    "miniforge3/envs",
    "mambaforge/envs",
    "anaconda3/envs",
];
const MACOS_FIXED_PYTHONS: [&str; 2] = ["/opt/homebrew/bin/python3", "/usr/local/bin/python3"];
const LINUX_FIXED_PYTHONS: [&str; 2] = ["/usr/local/bin/python3", "/home/linuxbrew/.linuxbrew/bin/python3"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlxLauncherKind {
    ServerScript,
    Uv,
    Uvx,
    Python,
}

pub fn mlx_launcher_kind(executable: &str) -> MlxLauncherKind {
    let name = Path::new(executable).file_name().and_then(|n| n.to_str()).unwrap_or(executable);
    match name {
        MLX_LM_SERVER_SCRIPT => MlxLauncherKind::ServerScript,
        UV_COMMAND => MlxLauncherKind::Uv,
        UVX_COMMAND => MlxLauncherKind::Uvx,
        _ => MlxLauncherKind::Python,
    }
}

pub fn mlx_launch_prefix(executable: &str) -> Vec<String> {
    let from_mlx_lm = [UVX_FROM_FLAG, MLX_LM_DISTRIBUTION, MLX_LM_SERVER_SCRIPT];
    let parts: Vec<&str> = match mlx_launcher_kind(executable) {
        MlxLauncherKind::ServerScript => Vec::new(),
        MlxLauncherKind::Uv => UV_TOOL_RUN_ARGS.iter().chain(from_mlx_lm.iter()).copied().collect(),
        MlxLauncherKind::Uvx => from_mlx_lm.to_vec(),
        MlxLauncherKind::Python => vec![PYTHON_MODULE_FLAG, MLX_LM_SERVER_SCRIPT],
    };
    parts.into_iter().map(str::to_string).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonCandidate {
    Fixed(String),
    EachSubdir { parent: String, name_prefix: Option<String>, child: String },
}

fn subdir_candidate(parent: String, name_prefix: Option<&str>) -> PythonCandidate {
    PythonCandidate::EachSubdir { parent, name_prefix: name_prefix.map(str::to_string), child: BIN_PYTHON3.to_string() }
}

pub fn python_candidates(home: Option<&str>, os: TargetOs) -> Vec<PythonCandidate> {
    let home = home.map(|h| h.trim_end_matches('/')).filter(|h| !h.is_empty());
    let system_fixed: &[&str] = match os {
        TargetOs::MacOs => &MACOS_FIXED_PYTHONS,
        TargetOs::Linux => &LINUX_FIXED_PYTHONS,
    };
    let mut out: Vec<PythonCandidate> = Vec::new();
    if let Some(h) = home {
        out.extend(HOME_FIXED_PYTHONS.iter().map(|rel| PythonCandidate::Fixed(format!("{h}/{rel}"))));
    }
    out.extend(system_fixed.iter().map(|p| PythonCandidate::Fixed(p.to_string())));
    if os == TargetOs::MacOs {
        out.push(subdir_candidate(HOMEBREW_PYTHON_FORMULAE_DIR.to_string(), Some(PYTHON_FORMULA_PREFIX)));
    }
    if let Some(h) = home {
        out.extend(HOME_ENV_PARENTS.iter().map(|rel| subdir_candidate(format!("{h}/{rel}"), None)));
    }
    out.push(PythonCandidate::Fixed(SYSTEM_PYTHON.to_string()));
    out
}

pub fn expand_subdir_candidate(parent: &str, name_prefix: Option<&str>, child: &str, mut subdirs: Vec<String>) -> Vec<String> {
    subdirs.retain(|name| !name.starts_with('.') && name_prefix.is_none_or(|p| name.starts_with(p)));
    subdirs.sort();
    subdirs.truncate(MAX_ENTRIES_PER_GLOB);
    subdirs.into_iter().map(|name| format!("{}/{name}/{child}", parent.trim_end_matches('/'))).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionHost {
    pub os: TargetOs,
    pub developer_tools_installed: bool,
}

impl DetectionHost {
    fn may_run(self, python: &str) -> bool {
        self.os != TargetOs::MacOs || self.developer_tools_installed || python != SYSTEM_PYTHON
    }
}

pub fn detect_best_mlx_launcher(
    search: &[String],
    expanded_pythons: &[String],
    host: DetectionHost,
    is_executable: impl Fn(&Path) -> bool,
    within_deadline: impl Fn() -> bool,
    imports_mlx_lm: impl Fn(&str) -> bool,
) -> Option<String> {
    let on_search_path = |name: &str| search.iter().any(|dir| is_executable(&Path::new(dir).join(name)));
    for command in [MLX_LM_SERVER_SCRIPT, UV_COMMAND, UVX_COMMAND] {
        if on_search_path(command) {
            return Some(command.to_string());
        }
    }
    let path_pythons = search.iter().map(|dir| format!("{}/{PYTHON3_COMMAND}", dir.trim_end_matches('/')));
    let mut tried: Vec<String> = Vec::new();
    for candidate in path_pythons.chain(expanded_pythons.iter().cloned()) {
        if tried.contains(&candidate) {
            continue;
        }
        tried.push(candidate.clone());
        if !host.may_run(&candidate) || !is_executable(Path::new(&candidate)) {
            continue;
        }
        if !within_deadline() {
            return None;
        }
        if imports_mlx_lm(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallProgram {
    Path(String),
    UvAfterInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallStep {
    pub program: InstallProgram,
    pub args: Vec<String>,
}

impl InstallStep {
    pub fn display(&self) -> String {
        let program = match &self.program {
            InstallProgram::Path(p) => p.as_str(),
            InstallProgram::UvAfterInstall => UV_COMMAND,
        };
        let mut parts = vec![program.to_string()];
        parts.extend(self.args.iter().map(|a| if a.contains(' ') { format!("'{a}'") } else { a.clone() }));
        parts.join(" ")
    }
}

pub fn mlx_install_plan(uv: Option<&str>, brew: Option<&str>) -> Vec<InstallStep> {
    let strings = |args: &[&str]| args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    let mut steps = Vec::new();
    let uv_program = match (uv, brew) {
        (Some(uv), _) => InstallProgram::Path(uv.to_string()),
        (None, Some(brew)) => {
            steps.push(InstallStep { program: InstallProgram::Path(brew.to_string()), args: strings(&["install", UV_COMMAND]) });
            InstallProgram::UvAfterInstall
        }
        (None, None) => {
            steps.push(InstallStep {
                program: InstallProgram::Path(POSIX_SHELL.to_string()),
                args: strings(&[POSIX_SHELL_COMMAND_FLAG, UV_OFFICIAL_INSTALLER_SCRIPT]),
            });
            InstallProgram::UvAfterInstall
        }
    };
    steps.push(InstallStep { program: uv_program, args: strings(&UV_TOOL_INSTALL_ARGS) });
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launcher_kind_is_decided_by_the_executable_basename() {
        assert_eq!(mlx_launcher_kind("mlx_lm.server"), MlxLauncherKind::ServerScript);
        assert_eq!(mlx_launcher_kind("/Users/me/.local/bin/mlx_lm.server"), MlxLauncherKind::ServerScript);
        assert_eq!(mlx_launcher_kind("uv"), MlxLauncherKind::Uv);
        assert_eq!(mlx_launcher_kind("/opt/homebrew/bin/uv"), MlxLauncherKind::Uv);
        assert_eq!(mlx_launcher_kind("uvx"), MlxLauncherKind::Uvx);
        assert_eq!(mlx_launcher_kind("/Users/me/.local/bin/uvx"), MlxLauncherKind::Uvx);
        assert_eq!(mlx_launcher_kind("/opt/homebrew/bin/python3"), MlxLauncherKind::Python);
        assert_eq!(mlx_launcher_kind("/Users/me/.venv/bin/python"), MlxLauncherKind::Python);
    }

    #[test]
    fn each_launcher_kind_gets_its_own_argument_prefix() {
        assert!(mlx_launch_prefix("/Users/me/.local/bin/mlx_lm.server").is_empty());
        assert_eq!(mlx_launch_prefix("uv"), strings(&["tool", "run", "--from", "mlx-lm", "mlx_lm.server"]));
        assert_eq!(mlx_launch_prefix("uvx"), strings(&["--from", "mlx-lm", "mlx_lm.server"]));
        assert_eq!(mlx_launch_prefix("/usr/bin/python3"), strings(&["-m", "mlx_lm.server"]));
    }

    const MAC_WITH_TOOLS: DetectionHost = DetectionHost { os: TargetOs::MacOs, developer_tools_installed: true };
    const MAC_WITHOUT_TOOLS: DetectionHost = DetectionHost { os: TargetOs::MacOs, developer_tools_installed: false };
    const LINUX_HOST: DetectionHost = DetectionHost { os: TargetOs::Linux, developer_tools_installed: false };

    #[test]
    fn the_macos_python_stub_is_never_run_without_developer_tools() {
        let search = strings(&["/usr/bin/", "/opt/bin"]);
        let pythons = strings(&["/usr/bin/python3"]);
        let found = detect_best_mlx_launcher(&search, &pythons, MAC_WITHOUT_TOOLS, exists_in(&["/usr/bin/python3", "/opt/bin/python3"]), || true, |p| {
            assert_ne!(p, "/usr/bin/python3", "ran the developer tools stub");
            p == "/opt/bin/python3"
        });
        assert_eq!(found.as_deref(), Some("/opt/bin/python3"));
    }

    #[test]
    fn the_system_python_is_tried_when_developer_tools_are_installed_or_off_macos() {
        let pythons = strings(&["/usr/bin/python3"]);
        for host in [MAC_WITH_TOOLS, LINUX_HOST] {
            let found = detect_best_mlx_launcher(&[], &pythons, host, exists_in(&["/usr/bin/python3"]), || true, |p| p == "/usr/bin/python3");
            assert_eq!(found.as_deref(), Some("/usr/bin/python3"));
        }
    }

    #[test]
    fn detection_gives_up_once_the_deadline_passes() {
        let pythons = strings(&["/a/python3", "/b/python3", "/c/python3"]);
        let checks = std::cell::Cell::new(0);
        let found = detect_best_mlx_launcher(&[], &pythons, MAC_WITH_TOOLS, exists_in(&["/a/python3", "/b/python3", "/c/python3"]), || checks.get() < 2, |p| {
            checks.set(checks.get() + 1);
            p == "/c/python3"
        });
        assert_eq!(found, None);
        assert_eq!(checks.get(), 2);
    }

    fn exists_in(present: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p: &Path| present.iter().any(|q| Path::new(q) == p)
    }

    #[test]
    fn an_installed_server_script_beats_uv_and_python() {
        let search = strings(&["/a", "/b"]);
        let found = detect_best_mlx_launcher(&search, &[], MAC_WITH_TOOLS, exists_in(&["/a/uv", "/b/mlx_lm.server", "/a/python3"]), || true, |_| true);
        assert_eq!(found.as_deref(), Some("mlx_lm.server"));
    }

    #[test]
    fn uv_is_preferred_over_uvx_and_both_are_stored_as_bare_names() {
        let search = strings(&["/a"]);
        assert_eq!(detect_best_mlx_launcher(&search, &[], MAC_WITH_TOOLS, exists_in(&["/a/uv", "/a/uvx"]), || true, |_| true).as_deref(), Some("uv"));
        assert_eq!(detect_best_mlx_launcher(&search, &[], MAC_WITH_TOOLS, exists_in(&["/a/uvx"]), || true, |_| true).as_deref(), Some("uvx"));
    }

    #[test]
    fn a_python_is_chosen_only_when_it_imports_mlx_lm() {
        let search = strings(&["/a"]);
        let pythons = strings(&["/env1/bin/python3", "/env2/bin/python3"]);
        let present = exists_in(&["/a/python3", "/env1/bin/python3", "/env2/bin/python3"]);
        let found = detect_best_mlx_launcher(&search, &pythons, MAC_WITH_TOOLS, present, || true, |p| p == "/env2/bin/python3");
        assert_eq!(found.as_deref(), Some("/env2/bin/python3"));
    }

    #[test]
    fn search_path_pythons_are_tried_before_scanned_pythons_and_each_only_once() {
        let search = strings(&["/a"]);
        let pythons = strings(&["/a/python3", "/env/bin/python3"]);
        let calls = std::cell::RefCell::new(Vec::new());
        let present = exists_in(&["/a/python3", "/env/bin/python3"]);
        let found = detect_best_mlx_launcher(&search, &pythons, MAC_WITH_TOOLS, present, || true, |p| { calls.borrow_mut().push(p.to_string()); p == "/env/bin/python3" });
        assert_eq!(found.as_deref(), Some("/env/bin/python3"));
        assert_eq!(*calls.borrow(), strings(&["/a/python3", "/env/bin/python3"]));
    }

    #[test]
    fn missing_pythons_are_never_run() {
        let found = detect_best_mlx_launcher(&[], &strings(&["/nope/python3"]), MAC_WITH_TOOLS, |_| false, || true, |_| panic!("ran a missing python"));
        assert_eq!(found, None);
    }

    #[test]
    fn python_candidates_cover_pipx_venvs_homebrew_pyenv_and_conda_and_end_with_system_python() {
        let candidates = python_candidates(Some("/Users/me"), TargetOs::MacOs);
        assert!(candidates.contains(&PythonCandidate::Fixed("/Users/me/.local/pipx/venvs/mlx-lm/bin/python".into())));
        assert!(candidates.contains(&PythonCandidate::Fixed("/Users/me/.venv/bin/python".into())));
        assert!(candidates.contains(&PythonCandidate::Fixed("/opt/homebrew/bin/python3".into())));
        for parent in ["/Users/me/.pyenv/versions", "/Users/me/miniconda3/envs", "/Users/me/mambaforge/envs"] {
            assert!(candidates.iter().any(|c| matches!(c, PythonCandidate::EachSubdir { parent: p, .. } if p == parent)), "{parent}");
        }
        assert!(candidates.iter().any(|c| matches!(c, PythonCandidate::EachSubdir { parent, name_prefix: Some(_), .. } if parent == "/opt/homebrew/opt")));
        assert_eq!(candidates.last(), Some(&PythonCandidate::Fixed("/usr/bin/python3".into())));
    }

    #[test]
    fn python_candidates_without_a_home_never_scan_home_folders() {
        let candidates = python_candidates(None, TargetOs::Linux);
        assert!(!candidates.iter().any(|c| matches!(c, PythonCandidate::EachSubdir { .. })));
        assert!(!candidates.contains(&PythonCandidate::Fixed("/opt/homebrew/bin/python3".into())));
    }

    #[test]
    fn subdir_expansion_skips_hidden_and_unmatched_names_and_is_capped() {
        let names: Vec<String> = (0..40).map(|i| format!("python@3.{i:02}")).chain([".hidden".into(), "node".into()]).collect();
        let paths = expand_subdir_candidate("/opt/homebrew/opt/", Some("python"), "bin/python3", names);
        assert_eq!(paths.len(), MAX_ENTRIES_PER_GLOB);
        assert_eq!(paths[0], "/opt/homebrew/opt/python@3.00/bin/python3");
        assert!(paths.iter().all(|p| !p.contains("node") && !p.contains(".hidden")));
    }

    #[test]
    fn with_uv_present_the_plan_is_only_the_tool_install_using_that_uv() {
        let plan = mlx_install_plan(Some("/Users/me/.local/bin/uv"), Some("/opt/homebrew/bin/brew"));
        assert_eq!(plan, vec![InstallStep {
            program: InstallProgram::Path("/Users/me/.local/bin/uv".into()),
            args: strings(&["tool", "install", "mlx-lm"]),
        }]);
    }

    #[test]
    fn without_uv_but_with_brew_the_plan_installs_uv_through_brew_first() {
        let plan = mlx_install_plan(None, Some("/opt/homebrew/bin/brew"));
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].display(), "/opt/homebrew/bin/brew install uv");
        assert_eq!(plan[1].program, InstallProgram::UvAfterInstall);
        assert_eq!(plan[1].display(), "uv tool install mlx-lm");
    }

    #[test]
    fn without_uv_or_brew_the_plan_runs_the_official_uv_installer_first() {
        let plan = mlx_install_plan(None, None);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].program, InstallProgram::Path("/bin/sh".into()));
        assert_eq!(plan[0].display(), "/bin/sh -c 'curl -LsSf https://astral.sh/uv/install.sh | sh'");
        assert_eq!(plan[1].program, InstallProgram::UvAfterInstall);
    }
}
