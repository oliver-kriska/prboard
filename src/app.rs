//! Root view: header (repo, counts, sync + rate-limit status) over the board
//! table, the auto-refresh loop, and the keyboard/mouse actions.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Local};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, AppContext, ClipboardItem, Context, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
};
use gpui_component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::table::{Table, TableEvent, TableState};
use gpui_component::{h_flex, v_flex, ActiveTheme, IndexPath, Sizable, TitleBar};
use prboard_core::board::Mode;

use crate::state::{relative, AppState};
use crate::table::BoardTableDelegate;
use crate::theme::ThemePref;

/// Startup decisions resolved in `main` (CLI + env + config file).
pub struct Launch {
    pub theme: ThemePref,
    pub refresh: Duration,
    /// Repo-picker entries; the active repo is always among them.
    pub repos: Vec<String>,
}

/// Persist the current window size so the next launch opens the same way.
/// Only the plain-windowed size — maximized/fullscreen store their restore
/// size, which is what we'd want back anyway.
fn save_window_size(window: &Window) {
    let (gpui::WindowBounds::Windowed(bounds)
    | gpui::WindowBounds::Maximized(bounds)
    | gpui::WindowBounds::Fullscreen(bounds)) = window.window_bounds();
    crate::config::persist_window(bounds.size.width.into(), bounds.size.height.into());
}

pub struct RootView {
    state: Entity<AppState>,
    table: Entity<TableState<BoardTableDelegate>>,
    /// Present only when there is more than one repo to pick from.
    repo_select: Option<Entity<SelectState<SearchableVec<String>>>>,
    focus_handle: FocusHandle,
    seen_generation: u64,
    theme_pref: ThemePref,
    refresh: Duration,
    /// Last selected display-row per queue, restored on switch-back so a queue
    /// keeps its place (critique #1). Bounded by the number of modes.
    selections: HashMap<Mode, usize>,
    /// Set on a mode switch and consumed by the generation observer once the
    /// switched rows land, so selection restore happens after the rows exist.
    pending_restore: Option<Mode>,
    /// Last real (non-header) selected row, so header bounces know which way
    /// the caret was travelling.
    last_selected: usize,
}

impl RootView {
    pub fn new(
        state: Entity<AppState>,
        launch: Launch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = state.read(cx).mode;
        let table = cx.new(|cx| {
            TableState::new(BoardTableDelegate::new(mode), window, cx)
                .sortable(false)
                .col_movable(false)
                .col_resizable(true)
                .row_selectable(true)
        });

        let repo_select = (launch.repos.len() > 1).then(|| {
            let current = state.read(cx).repo.clone();
            let selected = launch
                .repos
                .iter()
                .position(|r| *r == current)
                .map(IndexPath::new);
            let select = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(launch.repos.clone()),
                    selected,
                    window,
                    cx,
                )
            });
            cx.subscribe(
                &select,
                |this: &mut Self, _, event: &SelectEvent<SearchableVec<String>>, cx| {
                    let SelectEvent::Confirm(Some(repo)) = event else {
                        return;
                    };
                    let repo = repo.clone();
                    crate::config::persist_str("repo", &repo);
                    this.state.update(cx, |s, cx| s.switch_repo(repo, cx));
                },
            )
            .detach();
            select
        });

        // Theme: apply the configured preference, and while in System mode
        // follow macOS appearance changes live.
        let theme_pref = launch.theme;
        theme_pref.apply(window, cx);
        let this_handle = cx.entity().downgrade();
        window
            .observe_window_appearance(move |window, cx| {
                if let Some(view) = this_handle.upgrade() {
                    if view.read(cx).theme_pref == ThemePref::System {
                        ThemePref::System.apply(window, cx);
                    }
                }
            })
            .detach();

        // Push new rows into the table only when a fetch actually landed —
        // gate the observer on generation (the PRFlow infinite-observer trap).
        // On a mode switch, restore that queue's remembered selection + scroll
        // once its rows are in place; on a plain refresh, keep the current
        // selection but clamp it if the row count shrank.
        cx.observe(&state, |this: &mut Self, state, cx| {
            let generation = state.read(cx).generation;
            if generation != this.seen_generation {
                this.seen_generation = generation;
                let rows = state.read(cx).rows.clone();
                let restore = this
                    .pending_restore
                    .take()
                    .and_then(|m| this.selections.get(&m).copied());
                this.table.update(cx, |table, cx| {
                    table.delegate_mut().set_rows(rows);
                    table.refresh(cx);
                    let len = table.delegate().display_len();
                    match restore {
                        Some(sel) if sel < len && !table.delegate().is_header(sel) => {
                            table.set_selected_row(sel, cx);
                            table.scroll_to_row(sel, cx);
                        }
                        _ => {
                            if table.selected_row().is_some_and(|s| s >= len) {
                                table.clear_selection(cx);
                            }
                        }
                    }
                });
            }
            cx.notify();
        })
        .detach();

        cx.subscribe(&table, |this, _table, event: &TableEvent, cx| match event {
            TableEvent::DoubleClickedRow(row_ix) => this.open_row(*row_ix, cx),
            TableEvent::SelectRow(row_ix) => this.on_select_row(*row_ix, cx),
            _ => {}
        })
        .detach();

        // Arrow keys belong to the table's own key context.
        table.focus_handle(cx).focus(window);

        // Remember the window size across sessions (red traffic light path;
        // the `q` key saves too).
        window.on_window_should_close(cx, |window, _cx| {
            save_window_size(window);
            true
        });

        // The "synced Xm ago" label renders only on notify — without a slow
        // tick it can claim "just now" for a whole refresh interval. One
        // frame a minute; nothing animates.
        let ticker = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(60))
                .await;
            let Some(view) = ticker.upgrade() else { break };
            if view.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();

        let this = Self {
            state,
            table,
            repo_select,
            focus_handle: cx.focus_handle(),
            seen_generation: 0,
            theme_pref,
            refresh: launch.refresh,
            selections: HashMap::new(),
            pending_restore: None,
            last_selected: 0,
        };
        this.start_refresh_loop(cx);
        this
    }

    fn start_refresh_loop(&self, cx: &mut Context<Self>) {
        let state = self.state.clone();
        state.update(cx, |s, cx| s.refresh(cx));
        let interval = self.refresh;
        cx.spawn(async move |_this, cx| {
            loop {
                cx.background_executor().timer(interval).await;
                // Rate-limit gating and in-flight dedup live inside refresh().
                if state.update(cx, |s, cx| s.refresh(cx)).is_err() {
                    break; // app is shutting down
                }
            }
        })
        .detach();
    }

    fn selected_row_url(&self, cx: &App) -> Option<String> {
        let table = self.table.read(cx);
        let row_ix = table.selected_row()?;
        table.delegate().row(row_ix).map(|r| r.url.clone())
    }

    fn open_row(&self, row_ix: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.table.read(cx).delegate().row(row_ix) {
            cx.open_url(&row.url.clone());
        }
    }

    /// Switch the visible queue from either mouse or keyboard, keeping the
    /// data query, column set, and persisted preference in lockstep. The
    /// outgoing queue's selection is remembered and the incoming queue's is
    /// restored by the generation observer once its rows land.
    fn select_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        let current = self.state.read(cx).mode;
        if current == mode {
            return;
        }
        if let Some(sel) = self.table.read(cx).selected_row() {
            self.selections.insert(current, sel);
        }
        self.pending_restore = Some(mode);
        self.state.update(cx, |s, cx| s.set_mode(mode, cx));
        self.table.update(cx, |table, cx| {
            table.delegate_mut().set_mode(mode);
            table.refresh(cx);
        });
        crate::config::persist_str(
            "view",
            match mode {
                Mode::Authored => "authored",
                Mode::Review => "review",
            },
        );
    }

    /// Keep keyboard/mouse selection off the section-header pseudo-rows: when a
    /// header gets selected, bounce to the nearest PR in the direction of
    /// travel (`set_selected_row` re-emits `SelectRow`, but the bounced-to row
    /// is a real PR, so it settles in one hop).
    fn on_select_row(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        let delegate_is_header =
            |i: usize, this: &Self, cx: &Context<Self>| this.table.read(cx).delegate().is_header(i);
        if !delegate_is_header(row_ix, self, cx) {
            self.last_selected = row_ix;
            return;
        }
        let len = self.table.read(cx).delegate().display_len();
        let going_down = row_ix >= self.last_selected;
        let down = (row_ix + 1..len).find(|&i| !delegate_is_header(i, self, cx));
        let up = (0..row_ix)
            .rev()
            .find(|&i| !delegate_is_header(i, self, cx));
        let target = if going_down { down.or(up) } else { up.or(down) };
        if let Some(t) = target {
            self.last_selected = t;
            self.table
                .update(cx, |table, cx| table.set_selected_row(t, cx));
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let platform = event.keystroke.modifiers.platform;
        match key {
            "q" => {
                save_window_size(window);
                cx.quit();
            }
            "r" => self.state.update(cx, |s, cx| s.refresh(cx)),
            "v" if !platform => {
                let mode = match self.state.read(cx).mode {
                    Mode::Authored => Mode::Review,
                    Mode::Review => Mode::Authored,
                };
                self.select_mode(mode, cx);
            }
            "1" if !platform => self.select_mode(Mode::Authored, cx),
            "2" if !platform => self.select_mode(Mode::Review, cx),
            "t" if !platform => {
                self.theme_pref = self.theme_pref.next();
                self.theme_pref.apply(window, cx);
                crate::config::persist_str("theme", self.theme_pref.label());
                cx.notify();
            }
            "enter" | "o" if !platform => {
                if let Some(url) = self.selected_row_url(cx) {
                    cx.open_url(&url);
                }
            }
            "y" if !platform => {
                if let Some(url) = self.selected_row_url(cx) {
                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                }
            }
            _ => {}
        }
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let theme = cx.theme();
        let (action, awaiting, drafts) = state.counts();
        let draft_label = if drafts == 1 { "draft" } else { "drafts" };
        let counts = match state.mode {
            prboard_core::board::Mode::Authored => format!(
                "{} open · {action} need action · {awaiting} awaiting review · {drafts} {draft_label}",
                state.rows.len()
            ),
            prboard_core::board::Mode::Review => format!(
                "{} open · {action} pending · {awaiting} reviewed · {drafts} {draft_label}",
                state.rows.len()
            ),
        };
        // Static text only — a spinner animation would defeat the idle-GPU
        // half of the spike gate (zed#55949). Copy is queue-specific and keeps
        // the "synced Xm ago" anchor visible while a background refresh runs,
        // so the control reads as navigation, not a re-run (critique #6/#1).
        let sync = queue_sync_text(state.mode, state.syncing, state.last_synced);
        let budget = state
            .rate
            .as_ref()
            .map(|r| format!("API {}/{}", r.remaining, r.limit));
        let selected_mode = match state.mode {
            Mode::Authored => 0,
            Mode::Review => 1,
        };
        // The queue is a primary scope, not a hidden preference. A compact
        // toolbar tab view keeps both choices visible and the selected state
        // persistent (Apple HIG); equal widths prevent either queue from
        // appearing subordinate. Keyboard 1/2 and v remain accelerators.
        let view_switcher = TabBar::new("view-switcher")
            .small()
            .segmented()
            .selected_index(selected_mode)
            .child(Tab::new().label("My PRs").w(px(104.)).font_weight(
                if state.mode == Mode::Authored {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                },
            ))
            .child(Tab::new().label("Review queue").w(px(104.)).font_weight(
                if state.mode == Mode::Review {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                },
            ))
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                let mode = match index {
                    0 => Mode::Authored,
                    _ => Mode::Review,
                };
                this.select_mode(mode, cx);
            }));

        // One-line toolbar living INSIDE the transparent titlebar: repository
        // identity, primary queue scope, flexible counts, then sync status.
        // An error replaces the sync text — it IS the sync status then.
        h_flex()
            .flex_1()
            .pr(px(crate::design::HEADER_PAD_X))
            .gap_3()
            .items_center()
            .map(|this| match &self.repo_select {
                Some(select) => this.child(
                    div()
                        .min_w(px(200.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Select::new(select).small().menu_width(px(320.))),
                ),
                None => this.child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(state.repo.clone()),
                ),
            })
            .child(view_switcher)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(theme.muted_foreground)
                    .child(counts),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_3()
                    .text_size(px(12.))
                    .text_color(theme.muted_foreground)
                    .map(|this| match state.error.clone() {
                        Some(err) => this.child(div().text_color(theme.danger).child(err)),
                        None => this.child(sync),
                    })
                    .when_some(budget, |this, b| this.child(b)),
            )
    }

    fn render_footer(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let theme_label = format!("theme ({})", self.theme_pref.label());
        let hints: Vec<(&str, String)> = vec![
            ("↑↓", "select".into()),
            ("⏎", "open".into()),
            ("y", "copy".into()),
            ("1/2", "views".into()),
            ("r", "refresh".into()),
            ("t", theme_label),
            ("q", "quit".into()),
        ];
        // Keycap legend (spec §6): reference material lives at the bottom,
        // status at the top — the gh-dash/native pattern.
        let mut bar = h_flex()
            .flex_shrink_0()
            .px(px(crate::design::HEADER_PAD_X))
            .py(px(crate::design::FOOTER_PAD_Y))
            .gap_3()
            .items_center()
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(theme.title_bar_border);
        for (key, label) in hints {
            bar = bar.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .px_1()
                            .rounded(px(3.))
                            .bg(theme.muted)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.secondary_foreground)
                            .child(key),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.muted_foreground)
                            .child(label),
                    ),
            );
        }
        bar
    }
}

/// The header's right-side status line, specific to the active queue. Keeps
/// the "synced Xm ago" anchor visible during a background refresh so switching
/// feels like navigation, not a command re-run.
fn queue_sync_text(mode: Mode, syncing: bool, last_synced: Option<DateTime<Local>>) -> String {
    let loading = match mode {
        Mode::Authored => "Loading your open PRs…",
        Mode::Review => "Loading requested reviews…",
    };
    match (syncing, last_synced) {
        (true, None) | (false, None) => loading.to_string(),
        (true, Some(t)) => {
            let verb = match mode {
                Mode::Authored => "Updating your PRs…",
                Mode::Review => "Updating review requests…",
            };
            format!("{verb} · synced {}", relative(t))
        }
        (false, Some(t)) => format!("synced {}", relative(t)),
    }
}

/// The centered body copy shown before a queue's first rows ever arrive.
fn queue_loading_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Authored => "Loading your open PRs…",
        Mode::Review => "Loading requested reviews…",
    }
}

impl Focusable for RootView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let loaded = self.state.read(cx).generation > 0;
        let loading_text = queue_loading_text(self.state.read(cx).mode);

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(TitleBar::new().child(self.render_header(cx)))
            // Full-bleed table (spec §5): the window IS the table; 13 px
            // cells at Size::Small density.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .text_size(px(crate::design::TABLE_TEXT_PX))
                    .map(|this| {
                        if loaded {
                            // bordered defaults to TRUE — at full bleed the
                            // outer border + rounded corners fight the
                            // window edge (spec §5: header border is the
                            // only separator).
                            this.child(Table::new(&self.table).small().stripe(true).bordered(false))
                        } else {
                            this.child(
                                h_flex()
                                    .size_full()
                                    .justify_center()
                                    .text_color(theme.muted_foreground)
                                    .child(loading_text),
                            )
                        }
                    }),
            )
            .child(self.render_footer(cx))
    }
}
