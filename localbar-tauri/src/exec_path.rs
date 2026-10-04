use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use localbar_core::executable::{
    self, CommandResolver, ExecutableNotFound, ResolvedCommand, ShellFlavor, TargetOs,
    INTERACTIVE_LOGIN_SHELL_ARGS, LOGIN_SHELL_ARGS, PATH_ENV_VAR, SHELL_ENV_VAR,
};

const SHELL_READ_TIMEOUT: Duration = Duration::from_secs(4);
const SHELL_POLL_INTERVAL: Duration = Duration::from_millis(25);
const HOME_ENV_VAR: &str = "HOME";
#[cfg(unix)]
const ANY_EXECUTE_PERMISSION_BITS: u32 = 0o111;

static SHELL_PATH: OnceLock<Option<Vec<String>>> = OnceLock::new();

pub fn start_shell_path_read() {
    std::thread::spawn(|| {
        let _ = SHELL_PATH.set(read_login_shell_path());
    });
}

fn read_login_shell_path() -> Option<Vec<String>> {
    let shell = executable::login_shell(std::env::var(SHELL_ENV_VAR).ok().as_deref(), TargetOs::current());
    let flavor = executable::shell_flavor(&shell);
    run_shell_for_path(&shell, &INTERACTIVE_LOGIN_SHELL_ARGS, flavor, SHELL_READ_TIMEOUT)
        .or_else(|| run_shell_for_path(&shell, &LOGIN_SHELL_ARGS, flavor, SHELL_READ_TIMEOUT))
}

fn run_shell_for_path(shell: &str, args: &[&str], flavor: ShellFlavor, timeout: Duration) -> Option<Vec<String>> {
    let mut child = Command::new(shell)
        .args(args)
        .arg(executable::path_print_script(flavor))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    let deadline = Instant::now() + timeout;
    let exited = wait_until(&mut child, deadline);
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let buf = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok()?;
    executable::parse_marked_path(&String::from_utf8_lossy(&buf), flavor)
}

fn wait_until(child: &mut std::process::Child, deadline: Instant) -> bool {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(SHELL_POLL_INTERVAL),
            _ => return false,
        }
    }
}

pub fn search_path() -> Vec<String> {
    let shell = SHELL_PATH.get().and_then(|p| p.as_deref());
    let process = std::env::var(PATH_ENV_VAR).map(|p| executable::split_path(&p)).unwrap_or_default();
    let home = std::env::var(HOME_ENV_VAR).ok();
    let fallbacks = executable::fallback_dirs(home.as_deref(), TargetOs::current());
    executable::merge_search_path(shell, &process, &fallbacks)
}

pub fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & ANY_EXECUTE_PERMISSION_BITS != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn resolve_command(configured: &str) -> Result<ResolvedCommand, ExecutableNotFound> {
    let search = search_path();
    let program = executable::resolve_executable(configured, &search, is_executable_file)?;
    Ok(ResolvedCommand { program, path_env: executable::join_path(&search) })
}

pub fn resolver() -> CommandResolver {
    Arc::new(resolve_command)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn write_fake_shell(body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("localbar-fake-shell-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_real_login_shell_reports_its_path_through_the_markers() {
        let dirs = run_shell_for_path("/bin/sh", &LOGIN_SHELL_ARGS, ShellFlavor::Posix, SHELL_READ_TIMEOUT).unwrap();
        assert!(dirs.iter().any(|d| d == "/bin" || d == "/usr/bin"), "{dirs:?}");
    }

    #[test]
    fn a_shell_that_hangs_is_killed_at_the_timeout() {
        let shell = write_fake_shell("exec sleep 30");
        let started = Instant::now();
        let result = run_shell_for_path(shell.to_str().unwrap(), &LOGIN_SHELL_ARGS, ShellFlavor::Posix, Duration::from_millis(300));
        std::fs::remove_file(&shell).ok();
        assert_eq!(result, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_shell_that_fails_yields_no_path() {
        let shell = write_fake_shell("exit 1");
        let result = run_shell_for_path(shell.to_str().unwrap(), &INTERACTIVE_LOGIN_SHELL_ARGS, ShellFlavor::Posix, SHELL_READ_TIMEOUT);
        std::fs::remove_file(&shell).ok();
        assert_eq!(result, None);
    }

    #[test]
    fn a_bare_command_resolves_to_an_absolute_path_on_the_search_path() {
        let resolved = resolve_command("sh").unwrap();
        assert!(resolved.program.is_absolute());
        assert!(resolved.path_env.contains("/bin"));
    }

    #[test]
    fn a_missing_command_is_reported_with_the_folders_searched() {
        let err = resolve_command("localbar-definitely-missing-tool").unwrap_err();
        let home = std::env::var(HOME_ENV_VAR).ok();
        for dir in executable::fallback_dirs(home.as_deref(), TargetOs::current()) {
            assert!(err.searched.contains(&dir), "{dir} missing from {:?}", err.searched);
        }
    }

    #[test]
    fn a_directory_is_not_treated_as_an_executable() {
        assert!(!is_executable_file(Path::new("/bin")));
        assert!(is_executable_file(Path::new("/bin/sh")));
    }
}
