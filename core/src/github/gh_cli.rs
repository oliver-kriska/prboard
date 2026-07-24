//! v1 transport: shell out to `gh api graphql` (the prototype's and gh-dash's
//! model). `gh` owns auth, token refresh, enterprise hosts. Calls block — run
//! them on a background thread/executor, never the UI thread.

use std::io;
use std::process::Command;

use serde_json::Value;

use super::{GhError, GithubTransport, TokenSource};

pub struct GhCliTransport {
    gh_path: String,
}

impl GhCliTransport {
    pub fn new() -> Self {
        Self {
            gh_path: "gh".into(),
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
        let out = cmd.output().map_err(|e| classify_spawn_error(&e))?;

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
            gh_path: "gh".into(),
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
        let out = Command::new(&self.gh_path)
            .args(["auth", "token"])
            .output()
            .map_err(|e| classify_spawn_error(&e))?;
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
    run_gh_line(Command::new("gh").current_dir(dir).args([
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
    run_gh_line(Command::new("gh").args(["api", "user", "--jq", ".login"]))
}

fn run_gh_line(cmd: &mut Command) -> Result<String, GhError> {
    let out = cmd.output().map_err(|e| classify_spawn_error(&e))?;
    if !out.status.success() {
        return Err(classify_failure(&String::from_utf8_lossy(&out.stderr)));
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() {
        return Err(GhError::Parse("empty gh output".into()));
    }
    Ok(line)
}
