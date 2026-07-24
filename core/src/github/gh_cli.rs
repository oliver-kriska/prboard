//! v1 transport: shell out to `gh api graphql` (the prototype's and gh-dash's
//! model). `gh` owns auth, token refresh, enterprise hosts. Calls block — run
//! them on a background thread/executor, never the UI thread.

use std::io;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::{GhError, GithubTransport, TokenSource};

/// A hung `gh` (network black hole) must never freeze the data layer: with
/// no timeout, `syncing` stays true forever and the refresh dedup silently
/// blocks every future fetch including the `r` key. Kill and surface it.
const GRAPHQL_TIMEOUT: Duration = Duration::from_secs(60);
const QUICK_TIMEOUT: Duration = Duration::from_secs(30);

/// `Command::output()` with a watchdog: a helper thread SIGKILLs the child
/// (via `kill -9 <pid>`, no extra deps) if it outlives `timeout`. Output is
/// still collected by `wait_with_output`, so pipes drain normally.
fn output_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<Output, GhError> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| classify_spawn_error(&e))?;
    let pid = child.id();
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();
    let watchdog = std::thread::spawn(move || {
        let step = Duration::from_millis(100);
        let mut waited = Duration::ZERO;
        while waited < timeout {
            if done_flag.load(Ordering::Relaxed) {
                return false;
            }
            std::thread::sleep(step);
            waited += step;
        }
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        true
    });
    let out = child.wait_with_output();
    done.store(true, Ordering::Relaxed);
    let killed = watchdog.join().unwrap_or(false);
    if killed {
        return Err(GhError::Network(format!(
            "gh timed out after {}s — killed",
            timeout.as_secs()
        )));
    }
    out.map_err(|e| GhError::Network(e.to_string()))
}

/// Locate `gh`. Apps launched from Spotlight/Finder inherit a minimal PATH
/// (`/usr/bin:/bin`) that misses Homebrew, so a bare "gh" only works from a
/// terminal — fall back to the standard install locations.
pub fn resolve_gh_path() -> String {
    if which_on_path("gh") {
        return "gh".into();
    }
    for candidate in [
        "/opt/homebrew/bin/gh", // macOS arm64 Homebrew
        "/usr/local/bin/gh",    // macOS x86_64 Homebrew / manual installs
        "/home/linuxbrew/.linuxbrew/bin/gh",
    ] {
        if std::path::Path::new(candidate).is_file() {
            return candidate.into();
        }
    }
    "gh".into() // let spawn fail with NotInstalled
}

fn which_on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

pub struct GhCliTransport {
    gh_path: String,
}

impl GhCliTransport {
    pub fn new() -> Self {
        Self {
            gh_path: resolve_gh_path(),
        }
    }
}

impl Default for GhCliTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl GithubTransport for GhCliTransport {
    fn graphql(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, GhError> {
        let mut cmd = Command::new(&self.gh_path);
        cmd.args(["api", "graphql", "-f"])
            .arg(format!("query={query}"));
        for (k, v) in variables {
            cmd.arg("-f").arg(format!("{k}={v}"));
        }
        let out = output_with_timeout(&mut cmd, GRAPHQL_TIMEOUT)?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);

        // gh prints the GraphQL response body to stdout even on non-zero exit
        // (e.g. errors[] present). Prefer parsing the body over exit codes.
        if let Ok(body) = serde_json::from_str::<Value>(stdout.trim()) {
            return Ok(body);
        }

        if !out.status.success() {
            return Err(classify_failure(&stderr));
        }
        Err(GhError::Parse(format!(
            "gh returned non-JSON output: {}",
            stdout.chars().take(200).collect::<String>()
        )))
    }
}

fn classify_spawn_error(e: &io::Error) -> GhError {
    if e.kind() == io::ErrorKind::NotFound {
        GhError::NotInstalled
    } else {
        GhError::Network(e.to_string())
    }
}

fn classify_failure(stderr: &str) -> GhError {
    let s = stderr.to_lowercase();
    if s.contains("gh auth login") || s.contains("not logged in") || s.contains("authentication") {
        GhError::NotAuthenticated
    } else if s.contains("rate limit") || s.contains("rate_limited") {
        GhError::RateLimited { reset_epoch: None }
    } else {
        GhError::Network(stderr.trim().chars().take(300).collect())
    }
}

/// Token via `gh auth token`. Unused by `GhCliTransport` (auth stays inside
/// `gh`); the future direct-HTTP transport is seeded from this.
pub struct GhCliTokenSource {
    gh_path: String,
}

impl GhCliTokenSource {
    pub fn new() -> Self {
        Self {
            gh_path: resolve_gh_path(),
        }
    }
}

impl Default for GhCliTokenSource {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenSource for GhCliTokenSource {
    fn token(&self) -> Result<String, GhError> {
        let out = output_with_timeout(
            Command::new(&self.gh_path).args(["auth", "token"]),
            QUICK_TIMEOUT,
        )?;
        if !out.status.success() {
            return Err(GhError::NotAuthenticated);
        }
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if token.is_empty() {
            return Err(GhError::NotAuthenticated);
        }
        Ok(token)
    }
}

/// `owner/name` of the repo the given directory belongs to, via `gh repo view`.
pub fn detect_repo(dir: &std::path::Path) -> Result<String, GhError> {
    run_gh_line(Command::new(resolve_gh_path()).current_dir(dir).args([
        "repo",
        "view",
        "--json",
        "nameWithOwner",
        "--jq",
        ".nameWithOwner",
    ]))
}

/// The authenticated user's login (resolves what the prototype calls `@me`).
pub fn current_login() -> Result<String, GhError> {
    run_gh_line(Command::new(resolve_gh_path()).args(["api", "user", "--jq", ".login"]))
}

/// Repos the user can access, newest-activity first — feeds the repo picker.
pub fn list_repos(limit: u32) -> Result<Vec<String>, GhError> {
    let out = run_gh_line(Command::new(resolve_gh_path()).args([
        "repo",
        "list",
        "--limit",
        &limit.to_string(),
        "--json",
        "nameWithOwner",
        "--jq",
        ".[].nameWithOwner",
    ]))?;
    Ok(out.lines().map(|l| l.trim().to_string()).collect())
}

fn run_gh_line(cmd: &mut Command) -> Result<String, GhError> {
    let out = output_with_timeout(cmd, QUICK_TIMEOUT)?;
    if !out.status.success() {
        return Err(classify_failure(&String::from_utf8_lossy(&out.stderr)));
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() {
        return Err(GhError::Parse("empty gh output".into()));
    }
    Ok(line)
}
