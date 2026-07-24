//! prboard — a GitHub PR review dashboard as a native desktop app.
//! Step-0 walking skeleton: one window, one repo, the authored view.

mod app;
mod assets;
mod config;
mod design;
mod state;
mod table;
mod theme;

use std::sync::Arc;

use gpui::{px, size, App, AppContext, Application, WindowBounds, WindowKind, WindowOptions};
use prboard_core::board::{BoardConfig, IssueLinkRule, Mode};
use prboard_core::github::gh_cli::{current_login, detect_repo, GhCliTransport};

use crate::app::RootView;
use crate::state::AppState;

const USAGE: &str = "usage: prboard [--repo owner/name] [--review]

Repo resolution: --repo, else $PRBOARD_REPO, else `repo` in
~/.config/prboard/config.toml, else the git remote of the current directory
(via `gh repo view`). Requires an authenticated `gh`.

Config file (~/.config/prboard/config.toml): repo, repos = [..] for the
repo picker, refresh_secs, theme, default_reviewers, [issue_link].
Env vars override the file:
  PRBOARD_REPO                 owner/name
  PRBOARD_REFRESH_SECS         refresh interval (default 300, floor 30)
  PRBOARD_THEME                system | light | dark (default system; `t` cycles)
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

fn board_config(file: &config::FileConfig) -> BoardConfig {
    let mut config = BoardConfig::default();
    if !file.default_reviewers.is_empty() {
        config.default_reviewers = file.default_reviewers.clone();
    }
    if let Some(reviewers) = std::env::var("PRBOARD_DEFAULT_REVIEWERS")
        .ok()
        .filter(|v| !v.is_empty())
    {
        config.default_reviewers = reviewers.split(',').map(|s| s.trim().to_string()).collect();
    }
    let issue_rule = match (
        std::env::var("PRBOARD_ISSUE_PATTERN"),
        std::env::var("PRBOARD_ISSUE_URL_TEMPLATE"),
    ) {
        (Ok(pattern), Ok(template)) => Some((pattern, template)),
        _ => file
            .issue_link
            .as_ref()
            .map(|l| (l.pattern.clone(), l.url_template.clone())),
    };
    if let Some((pattern, template)) = issue_rule {
        match IssueLinkRule::new(&pattern, &template) {
            Ok(rule) => config.issue_link = Some(rule),
            Err(e) => eprintln!("prboard: ignoring bad issue-link pattern: {e}"),
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

    let file = config::load();

    // Resolve repo + login up front (CLI phase, before any window exists) so
    // auth/setup problems surface as plain terminal messages.
    let repo = repo_arg
        .or_else(|| std::env::var("PRBOARD_REPO").ok().filter(|v| !v.is_empty()))
        .or_else(|| file.repo.clone())
        .map(Ok)
        .unwrap_or_else(|| {
            detect_repo(&std::env::current_dir().expect("cwd")).map_err(|e| {
                format!(
                    "no repo configured and none detectable from the current directory ({e}); \
                     pass --repo owner/name or set `repo` in {}",
                    config::config_path().display()
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
    let config = board_config(&file);

    // Repo-picker entries: config list with the active repo always present.
    let mut repos = file.repos.clone();
    if !repos.contains(&repo) {
        repos.insert(0, repo.clone());
    }
    let launch = app::Launch {
        theme: crate::theme::ThemePref::resolve(file.theme.as_deref()),
        refresh: crate::state::refresh_interval(file.refresh_secs),
        repos,
    };

    Application::new().with_assets(assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        cx.activate(true);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
            // Transparent native chrome: the app's own header row IS the
            // titlebar (traffic lights overlay it), like Zed/modern Mac apps.
            titlebar: Some(gpui_component::TitleBar::title_bar_options()),
            window_min_size: Some(size(px(900.), px(560.))),
            kind: WindowKind::Normal,
            app_id: Some("dev.oliverkriska.prboard".into()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            let state =
                cx.new(|_| AppState::new(repo, me, mode, config, Arc::new(GhCliTransport::new())));
            cx.new(|cx| RootView::new(state, launch, window, cx))
        })
        .expect("failed to open window");
    });
}
