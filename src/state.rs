//! App state entity: the board data plus refresh/sync/rate-limit status.
//! All GitHub work happens on the background executor; results hop back to
//! the UI thread via `this.update`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};
use gpui::Context;
use prboard_core::board::{derive_rows, BoardConfig, BoardRow, Mode};
use prboard_core::github::query::{parse_search_response, search_string, PR_SEARCH_QUERY};
use prboard_core::github::rate_limit::{backoff_secs, should_back_off, RateLimitInfo};
use prboard_core::github::{GhError, GithubTransport};

pub struct AppState {
    pub repo: String,
    pub me: String,
    pub mode: Mode,
    pub config: BoardConfig,
    pub transport: Arc<dyn GithubTransport>,
    pub rows: Vec<BoardRow>,
    pub last_synced: Option<DateTime<Local>>,
    pub syncing: bool,
    pub error: Option<String>,
    pub rate: Option<RateLimitInfo>,
    /// Bumped on every successful fetch; observers use it to detect new rows
    /// without diffing (and to gate their reactions — the PRFlow observer-loop
    /// lesson).
    pub generation: u64,
    /// Do not fetch before this epoch second (set after a rate-limit hit,
    /// always clamped 60..900s ahead).
    backoff_until: Option<u64>,
}

impl AppState {
    pub fn new(
        repo: String,
        me: String,
        mode: Mode,
        config: BoardConfig,
        transport: Arc<dyn GithubTransport>,
    ) -> Self {
        Self {
            repo,
            me,
            mode,
            config,
            transport,
            rows: Vec::new(),
            last_synced: None,
            syncing: false,
            error: None,
            rate: None,
            generation: 0,
            backoff_until: None,
        }
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        use prboard_core::board::Category;
        let count = |c: Category| self.rows.iter().filter(|r| r.category == c).count();
        match self.mode {
            Mode::Authored => (
                count(Category::Action),
                count(Category::Await),
                count(Category::Draft),
            ),
            Mode::Review => (
                count(Category::Todo),
                count(Category::Done),
                count(Category::Draft),
            ),
        }
    }

    /// Kick off one fetch unless one is in flight or we are backing off.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.syncing {
            return;
        }
        let now = Local::now().timestamp().max(0) as u64;
        if let Some(until) = self.backoff_until {
            if now < until {
                return; // quietly wait out the backoff window
            }
            self.backoff_until = None;
        }
        // Preserve the reserve for the user's own gh/git usage.
        if let Some(rate) = &self.rate {
            if should_back_off(rate) {
                let reset = rate.reset_epoch();
                if reset.is_some_and(|r| now < r) {
                    self.backoff_until = Some(now + backoff_secs(reset, now));
                    self.error = Some(format!(
                        "GitHub budget low ({} left) — pausing refresh",
                        rate.remaining
                    ));
                    cx.notify();
                    return;
                }
            }
        }

        self.syncing = true;
        self.error = None;
        cx.notify();

        let transport = self.transport.clone();
        let repo = self.repo.clone();
        let me = self.me.clone();
        let mode = self.mode;
        let config = self.config.clone();

        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    // The `gh` subprocess blocks; that is fine on the
                    // background pool for a call made every few minutes.
                    let search = search_string(mode, &repo, &me);
                    let body = transport.graphql(PR_SEARCH_QUERY, &[("q", &search)])?;
                    let (prs, rate) = parse_search_response(&body)?;
                    Ok::<_, GhError>((derive_rows(&prs, mode, &repo, &me, &config), rate))
                })
                .await;

            let _ = this.update(cx, |state, cx| {
                state.syncing = false;
                match fetched {
                    Ok((rows, rate)) => {
                        state.rows = rows;
                        state.rate = rate;
                        state.last_synced = Some(Local::now());
                        state.generation += 1;
                    }
                    Err(GhError::RateLimited { reset_epoch }) => {
                        let now = Local::now().timestamp().max(0) as u64;
                        let wait = backoff_secs(reset_epoch, now);
                        state.backoff_until = Some(now + wait);
                        state.error = Some(format!(
                            "GitHub rate limited — retrying in {}m",
                            wait.div_ceil(60)
                        ));
                    }
                    Err(e) => {
                        state.error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// "just now" / "3m ago" / "2h 15m ago" — static text, recomputed on notify.
pub fn relative(since: DateTime<Local>) -> String {
    let secs = (Local::now() - since).num_seconds().max(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        _ => format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60),
    }
}

/// Refresh interval from `PRBOARD_REFRESH_SECS`, clamped to the hard floor.
pub fn refresh_interval() -> Duration {
    use prboard_core::github::rate_limit::{DEFAULT_REFRESH_SECS, MIN_REFRESH_SECS};
    let secs = std::env::var("PRBOARD_REFRESH_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_REFRESH_SECS)
        .max(MIN_REFRESH_SECS);
    Duration::from_secs(secs)
}
