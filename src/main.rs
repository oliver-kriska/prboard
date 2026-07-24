//! prboard — a GitHub PR review dashboard as a native desktop app.
//! Step-0 walking skeleton: one window, one repo, the authored view.

mod app;
mod state;
mod table;

use std::sync::Arc;

use gpui::{
    point, px, size, App, AppContext, Application, TitlebarOptions, WindowBounds, WindowKind,
    WindowOptions,
};
use prboard_core::board::{BoardConfig, IssueLinkRule, Mode};
use prboard_core::github::gh_cli::{current_login, detect_repo, GhCliTransport};

use crate::app::RootView;
use crate::state::AppState;

const USAGE: &str = "usage: prboard [--repo owner/name] [--review]

Repo resolution: --repo, else $PRBOARD_REPO, else the git remote of the
current directory (via `gh repo view`). Requires an authenticated `gh`.

Environment:
  PRBOARD_REPO                 owner/name
  PRBOARD_REFRESH_SECS         refresh interval (default 300, floor 30)
  PRBOARD_ISSUE_PATTERN        e.g. ENA-[0-9]+
  PRBOARD_ISSUE_URL_TEMPLATE   e.g. https://linear.app/acme/issue/{id}
  PRBOARD_DEFAULT_REVIEWERS    comma-separated logins for the no-reviewer note";

fn parse_args() -> Result<(Option<String>, Mode), String> {
    let mut repo = None;
    let mut mode = Mode::Authored;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo = Some(args.next().ok_or("--repo needs owner/name")?),
            "--review" => mode = Mode::Review,
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown arg: {other}\n\n{USAGE}")),
        }
    }
    Ok((repo, mode))
}

fn board_config() -> BoardConfig {
    let mut config = BoardConfig::default();
    if let Some(reviewers) = std::env::var("PRBOARD_DEFAULT_REVIEWERS")
        .ok()
        .filter(|v| !v.is_empty())
    {
        config.default_reviewers = reviewers.split(',').map(|s| s.trim().to_string()).collect();
    }
    if let (Ok(pattern), Ok(template)) = (
        std::env::var("PRBOARD_ISSUE_PATTERN"),
        std::env::var("PRBOARD_ISSUE_URL_TEMPLATE"),
    ) {
        match IssueLinkRule::new(&pattern, &template) {
            Ok(rule) => config.issue_link = Some(rule),
            Err(e) => eprintln!("prboard: ignoring bad PRBOARD_ISSUE_PATTERN: {e}"),
        }
    }
    config
}

fn main() {
    let (repo_arg, mode) = match parse_args() {
        Ok(parsed) => parsed,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    // Resolve repo + login up front (CLI phase, before any window exists) so
    // auth/setup problems surface as plain terminal messages.
    let repo = repo_arg
        .or_else(|| std::env::var("PRBOARD_REPO").ok().filter(|v| !v.is_empty()))
        .map(Ok)
        .unwrap_or_else(|| {
            detect_repo(&std::env::current_dir().expect("cwd")).map_err(|e| {
                format!(
                    "cannot detect repo from the current directory ({e}); pass --repo owner/name"
                )
            })
        });
    let repo = match repo {
        Ok(repo) => repo,
        Err(msg) => {
            eprintln!("prboard: {msg}");
            std::process::exit(1);
        }
    };
    let me = match current_login() {
        Ok(login) => login,
        Err(e) => {
            eprintln!("prboard: {e}");
            std::process::exit(1);
        }
    };
    let config = board_config();

    Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        cx.activate(true);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("prboard".into()),
                appears_transparent: false,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            window_min_size: Some(size(px(900.), px(560.))),
            kind: WindowKind::Normal,
            app_id: Some("dev.oliverkriska.prboard".into()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            let state =
                cx.new(|_| AppState::new(repo, me, mode, config, Arc::new(GhCliTransport::new())));
            cx.new(|cx| RootView::new(state, window, cx))
        })
        .expect("failed to open window");
    });
}
