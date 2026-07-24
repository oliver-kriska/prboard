//! Root view: header (repo, counts, sync + rate-limit status) over the board
//! table, the auto-refresh loop, and the keyboard/mouse actions.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, App, AppContext, ClipboardItem, Context, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
};
use gpui_component::table::{Table, TableEvent, TableState};
use gpui_component::{h_flex, v_flex, ActiveTheme};

use crate::state::{refresh_interval, relative, AppState};
use crate::table::BoardTableDelegate;

pub struct RootView {
    state: Entity<AppState>,
    table: Entity<TableState<BoardTableDelegate>>,
    focus_handle: FocusHandle,
    seen_generation: u64,
}

impl RootView {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mode = state.read(cx).mode;
        let table = cx.new(|cx| {
            TableState::new(BoardTableDelegate::new(mode), window, cx)
                .sortable(false)
                .col_movable(false)
                .row_selectable(true)
        });

        // Push new rows into the table only when a fetch actually landed —
        // gate the observer on generation (the PRFlow infinite-observer trap).
        cx.observe(&state, |this: &mut Self, state, cx| {
            let generation = state.read(cx).generation;
            if generation != this.seen_generation {
                this.seen_generation = generation;
                let rows = state.read(cx).rows.clone();
                this.table.update(cx, |table, cx| {
                    table.delegate_mut().set_rows(rows);
                    table.refresh(cx);
                });
            }
            cx.notify();
        })
        .detach();

        cx.subscribe(&table, |this, _table, event: &TableEvent, cx| {
            if let TableEvent::DoubleClickedRow(row_ix) = event {
                this.open_row(*row_ix, cx);
            }
        })
        .detach();

        // Arrow keys belong to the table's own key context.
        table.focus_handle(cx).focus(window);

        let this = Self {
            state,
            table,
            focus_handle: cx.focus_handle(),
            seen_generation: 0,
        };
        this.start_refresh_loop(cx);
        this
    }

    fn start_refresh_loop(&self, cx: &mut Context<Self>) {
        let state = self.state.clone();
        state.update(cx, |s, cx| s.refresh(cx));
        let interval = refresh_interval();
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

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let platform = event.keystroke.modifiers.platform;
        match key {
            "q" => cx.quit(),
            "r" => self.state.update(cx, |s, cx| s.refresh(cx)),
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
        let counts = match state.mode {
            prboard_core::board::Mode::Authored => format!(
                "{} open · {action} need action · {awaiting} awaiting review · {drafts} drafts",
                state.rows.len()
            ),
            prboard_core::board::Mode::Review => format!(
                "{} to review · {action} todo · {awaiting} done · {drafts} drafts",
                state.rows.len()
            ),
        };
        // Static text only — a spinner animation would defeat the idle-GPU
        // half of the spike gate (zed#55949).
        let sync = if state.syncing {
            "syncing…".to_string()
        } else {
            match state.last_synced {
                Some(t) => format!("synced {}", relative(t)),
                None => "loading…".to_string(),
            }
        };
        let budget = state
            .rate
            .as_ref()
            .map(|r| format!("API {}/{}", r.remaining, r.limit));

        v_flex()
            .flex_shrink_0()
            .px_4()
            .py_2()
            .gap_1()
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_3()
                    .items_baseline()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(state.repo.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(counts),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(sync)
                    .when_some(budget, |this, b| this.child(b))
                    .when_some(state.error.clone(), |this, err| {
                        this.child(div().text_color(theme.danger).child(err))
                    })
                    .child(div().flex_1())
                    .child("↑↓ select · ⏎ open · y copy · r refresh · q quit"),
            )
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

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(self.render_header(cx))
            .child(div().flex_1().min_h_0().p_2().map(|this| {
                if loaded {
                    this.child(Table::new(&self.table).stripe(true).bordered(true))
                } else {
                    this.child(
                        h_flex()
                            .size_full()
                            .justify_center()
                            .text_color(theme.muted_foreground)
                            .child("Loading board…"),
                    )
                }
            }))
    }
}
