//! The board table: a `TableDelegate` over `Vec<BoardRow>` for
//! gpui-component's virtualized `Table`. Cells render the same glyph language
//! as the prototype's markdown dashboard (SKILL.md).
//!
//! Rows are grouped by category with a section-header row before each band
//! ("Needs action (14)"). Because published gpui-component 0.5.1 renders every
//! cell inside a fixed-width `overflow_hidden` box, a full-width header label
//! cannot live in a cell — it is drawn as an absolute overlay from `render_tr`
//! (the row container is NOT clipped) while that row's cells render empty.
//!
//! Nothing here may animate: the table sits idle between refreshes and any
//! continuous animation would defeat the idle-GPU half of the spike gate.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::table::{Column, TableDelegate, TableState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, ActiveTheme};
use prboard_core::board::{BoardRow, Category, Ci, Mode};

use crate::design::{CHIP_HEIGHT, CHIP_PAD_X, CHIP_RADIUS, STATUS_DOT};

/// One rendered line of the table: either a category section header or a PR
/// (an index into `rows`). Headers are pseudo-rows — `row()` returns `None`
/// for them, and keyboard selection bounces off them (see `app.rs`).
enum DisplayRow {
    Header { label: &'static str, count: usize },
    Pr(usize),
}

pub struct BoardTableDelegate {
    rows: Vec<BoardRow>,
    display: Vec<DisplayRow>,
    columns: Vec<Column>,
    mode: Mode,
}

impl BoardTableDelegate {
    pub fn new(mode: Mode) -> Self {
        Self {
            rows: Vec::new(),
            display: Vec::new(),
            columns: Self::columns_for(mode),
            mode,
        }
    }

    /// Rebuild the column set AND the section grouping on an in-app mode switch
    /// (rows arrive with the switched fetch via the generation observer).
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.columns = Self::columns_for(mode);
        self.rebuild_display();
    }

    fn columns_for(mode: Mode) -> Vec<Column> {
        // Human scanning order (critique #3): identity first (PR + draft
        // badge), then WHAT it is (Title), then health (CI), then who/what is
        // blocking, then Labels, then the Note. The always-"ready" Status
        // column is gone — draft is a compact badge in the PR cell instead.
        // Note stays last and widest: it is the product (visual-design doc),
        // and Title degrades gracefully where the Note must not. The width
        // sum fits the default 1440px window.
        match mode {
            Mode::Authored => vec![
                // 100px fits "#NNNNN" plus the draft badge without clipping.
                Column::new("pr", "PR").width(px(100.)),
                Column::new("title", "Title").width(px(392.)),
                Column::new("ci", "CI").width(px(72.)),
                Column::new("requested", "Requested").width(px(140.)),
                Column::new("reviewed", "Reviewed by").width(px(150.)),
                Column::new("labels", "Labels").width(px(96.)),
                Column::new("note", "Note").width(px(440.)),
            ],
            Mode::Review => vec![
                Column::new("pr", "PR").width(px(100.)),
                Column::new("title", "Title").width(px(516.)),
                Column::new("ci", "CI").width(px(72.)),
                Column::new("author", "Author").width(px(120.)),
                Column::new("unresolved", "Unres").width(px(56.)),
                Column::new("labels", "Labels").width(px(96.)),
                Column::new("note", "Note").width(px(440.)),
            ],
        }
    }

    pub fn set_rows(&mut self, rows: Vec<BoardRow>) {
        self.rows = rows;
        self.rebuild_display();
    }

    /// Interleave a section header before each contiguous category band. Rows
    /// arrive already sorted by category rank (core `derive_rows`), so a band
    /// is just a run of equal categories.
    fn rebuild_display(&mut self) {
        let mut display = Vec::with_capacity(self.rows.len() + 3);
        let mut i = 0;
        while i < self.rows.len() {
            let cat = self.rows[i].category;
            let start = i;
            while i < self.rows.len() && self.rows[i].category == cat {
                i += 1;
            }
            display.push(DisplayRow::Header {
                label: group_label(self.mode, cat),
                count: i - start,
            });
            for j in start..i {
                display.push(DisplayRow::Pr(j));
            }
        }
        self.display = display;
    }

    /// The `BoardRow` at a display index, or `None` if it is a section header.
    pub fn row(&self, display_ix: usize) -> Option<&BoardRow> {
        match self.display.get(display_ix)? {
            DisplayRow::Pr(i) => self.rows.get(*i),
            DisplayRow::Header { .. } => None,
        }
    }

    /// True when the display index is a non-selectable section header.
    pub fn is_header(&self, display_ix: usize) -> bool {
        matches!(
            self.display.get(display_ix),
            Some(DisplayRow::Header { .. })
        )
    }

    pub fn display_len(&self) -> usize {
        self.display.len()
    }
}

/// The section label for a category within a mode. Action/Await differ from
/// Todo/Done even though they share a sort rank.
fn group_label(mode: Mode, cat: Category) -> &'static str {
    match (mode, cat) {
        (Mode::Authored, Category::Action) => "Needs action",
        (Mode::Authored, Category::Await) => "Awaiting review",
        (Mode::Review, Category::Todo) => "Needs review",
        (Mode::Review, Category::Done) => "Reviewed",
        (_, Category::Draft) => "Drafts",
        // Unreachable pairings (Action in Review etc.) — a calm fallback.
        _ => "Other",
    }
}

/// Calm reviewer-state glyph + its color token, per the design spec (§8).
/// Rendered BEFORE the login: on truncation the glyph is the information.
fn review_glyph(state: &str, theme: &gpui_component::theme::Theme) -> (&'static str, Hsla) {
    match state {
        "APPROVED" => ("✓", theme.success),
        "COMMENTED" => ("·", theme.muted_foreground),
        "CHANGES_REQUESTED" => ("±", theme.danger),
        // ✕, not "–": a bare dash reads as "nothing" — dismissed is an
        // invalidated review, which is information.
        "DISMISSED" => ("✕", theme.muted_foreground),
        _ => ("·", theme.muted_foreground),
    }
}

/// Notes come from core with the prototype's emoji language (the SKILL spec);
/// on this board a themed status dot carries that signal instead, so the
/// emoji are presentation noise — strip them, never change them in core.
fn strip_note_glyphs(note: &str) -> String {
    const GLYPHS: &[&str] = &[
        "⚠️ ", "🔴 ", "❌ ", "✋ ", "🟡 ", "🟢 ", "✅ ", "💬 ", "🔵 ",
    ];
    let mut s = note.to_string();
    for g in GLYPHS {
        s = s.replace(g, "");
    }
    s
}

fn status_dot(color: Hsla) -> Div {
    div()
        .size(px(STATUS_DOT))
        .rounded_full()
        .flex_shrink_0()
        .bg(color)
}

fn review_state_word(state: &str) -> &'static str {
    match state {
        "APPROVED" => "approved",
        "COMMENTED" => "commented",
        "CHANGES_REQUESTED" => "requested changes",
        "DISMISSED" => "dismissed",
        _ => "reviewed",
    }
}

impl TableDelegate for BoardTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.display.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let tr = div().id(("board-row", row_ix));
        let theme = cx.theme();
        match self.display.get(row_ix) {
            // Section header: a subtle band with a stronger top rule, and the
            // label drawn as an absolute overlay (cells render empty on this
            // row so nothing paints over it — see the module note).
            Some(DisplayRow::Header { label, count }) => tr
                .relative()
                .bg(theme
                    .secondary
                    .opacity(if theme.mode.is_dark() { 0.5 } else { 0.7 }))
                .border_t_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .absolute()
                        .left(px(crate::design::HEADER_PAD_X))
                        .top_0()
                        .bottom_0()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.secondary_foreground)
                                .child(label.to_uppercase()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.muted_foreground)
                                .child(count.to_string()),
                        ),
                ),
            Some(DisplayRow::Pr(i)) => match self.rows.get(*i).map(|r| r.category) {
                // A whisper of a tint keeps "needs me" rows findable at a
                // glance; the red note dot does the pointing, position does the
                // sorting. Perceptual loudness, not numeric alpha, must match
                // across modes: 4% over white reads salmon, over near-black it
                // vanishes.
                Some(Category::Action | Category::Todo) => {
                    let a = if theme.mode.is_dark() { 0.05 } else { 0.02 };
                    tr.bg(theme.danger.opacity(a))
                }
                // Drafts are dimmed (inactive), not tinted (different) — see
                // the per-cell text colors.
                _ => tr,
            },
            None => tr,
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        // Header rows carry no cell content — the label is an overlay from
        // render_tr; empty (transparent) cells let it show through.
        let row = match self.display.get(row_ix) {
            Some(DisplayRow::Pr(i)) => match self.rows.get(*i) {
                Some(r) => r,
                None => return div().into_any_element(),
            },
            _ => return div().into_any_element(),
        };
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // Draft rows dim their text — inactive, not merely different.
        let dim = row.draft;

        let cell = match self.columns[col_ix].key.as_ref() {
            "pr" => {
                // Single-click link (critique #5): the blue #number opens the
                // PR. A compact "draft" badge replaces the old always-"ready"
                // Status column.
                let url = row.url.clone();
                let number = h_flex()
                    .id(("pr-link", row_ix))
                    .cursor_pointer()
                    .text_color(if dim { muted } else { theme.link })
                    .hover(|this| this.underline())
                    .child(format!("#{}", row.number))
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url)));
                let mut cell = h_flex().gap_1().items_center().child(number);
                if row.draft {
                    cell = cell.child(
                        div()
                            .px(px(4.))
                            .rounded(px(CHIP_RADIUS))
                            .bg(theme.muted)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(10.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted)
                            .child("draft"),
                    );
                }
                return cell.into_any_element();
            }
            "labels" => {
                if row.labels.is_empty() {
                    div().text_color(muted.opacity(0.5)).child("—")
                } else {
                    // Chips; "bug" is the loud one. Cap the visible count —
                    // the tooltip carries the full list.
                    const VISIBLE: usize = 2;
                    let full = row.labels.join(", ");
                    // "bug" must never hide behind the +n overflow.
                    let mut ordered: Vec<String> = row.labels.clone();
                    ordered.sort_by_key(|l| l != "bug");
                    // One neutral chip style (spec §7) — GitHub's arbitrary
                    // label hues would out-shout the status system. 🐛 is the
                    // single permitted emoji: semantic, not decorative.
                    let chip_base = || {
                        h_flex()
                            .h(px(CHIP_HEIGHT))
                            .px(px(CHIP_PAD_X))
                            .items_center()
                            .rounded(px(CHIP_RADIUS))
                            .bg(theme.muted)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                    };
                    let mut chips = h_flex().gap_1().overflow_hidden();
                    for label in ordered.iter().take(VISIBLE) {
                        let text = if label == "bug" {
                            format!("🐛 {label}")
                        } else {
                            label.clone()
                        };
                        chips = chips.child(
                            chip_base()
                                .text_color(theme.secondary_foreground)
                                .child(text),
                        );
                    }
                    if row.labels.len() > VISIBLE {
                        chips = chips.child(
                            chip_base()
                                .text_color(muted)
                                .child(format!("+{}", row.labels.len() - VISIBLE)),
                        );
                    }
                    return chips
                        .id(("labels", row_ix))
                        .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                        .into_any_element();
                }
            }
            // The calm rule (spec §8): bad states get colored text, good
            // states get only a colored dot with muted text.
            "ci" => match row.ci {
                Ci::Pass => h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(status_dot(theme.success))
                    .child(div().text_color(muted).child("pass")),
                Ci::Fail => h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(status_dot(theme.danger))
                    .child(div().text_color(theme.danger).child("fail")),
                Ci::Running => h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(status_dot(theme.warning))
                    .child(div().text_color(muted).child("running")),
                Ci::None => h_flex().child(div().text_color(muted.opacity(0.5)).child("—")),
            },
            "author" => div().child(row.author.clone().unwrap_or_else(|| "?".into())),
            "unresolved" => {
                if row.unresolved > 0 {
                    div()
                        .text_color(theme.warning)
                        .child(row.unresolved.to_string())
                } else {
                    div().text_color(muted.opacity(0.5)).child("—")
                }
            }
            "requested" => {
                if row.requested.is_empty() {
                    // The red no-reviewers note already carries this signal;
                    // anything louder than a dim dash is noise.
                    div().text_color(muted.opacity(0.5)).child("—")
                } else {
                    div().child(row.requested.join(", "))
                }
            }
            "reviewed" => {
                if row.reviews.is_empty() {
                    div().text_color(muted.opacity(0.5)).child("—")
                } else {
                    // Glyph first: when the column truncates, the state glyph
                    // is the information — the login is recoverable from the
                    // tooltip, the ±/– is not.
                    let mut cell = h_flex().gap_2().overflow_hidden();
                    for r in &row.reviews {
                        let (glyph, color) = review_glyph(&r.state, theme);
                        cell = cell.child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .whitespace_nowrap()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(color)
                                        .child(glyph),
                                )
                                .child(r.login.clone().unwrap_or_else(|| "?".into())),
                        );
                    }
                    let full = row
                        .reviews
                        .iter()
                        .map(|r| {
                            format!(
                                "{} {}",
                                r.login.as_deref().unwrap_or("?"),
                                review_state_word(&r.state)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · ");
                    return cell
                        .id(("reviewed", row_ix))
                        .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                        .into_any_element();
                }
            }
            "title" => {
                let full = match &row.issue {
                    Some(issue) => format!("{issue} · {}", row.title),
                    None => row.title.clone(),
                };
                let tag_color = if dim { muted } else { theme.accent_foreground };
                // Ellipsis, not a mid-word clip — it's the affordance that
                // says "there's more, hover" (critique #3).
                let inner = match (&row.issue, &row.issue_url) {
                    // A linked issue is a single-click link of its own
                    // (critique #5): the tag opens the tracker, not the PR.
                    (Some(issue), Some(issue_url)) => {
                        let issue_url = issue_url.clone();
                        h_flex()
                            .gap_1()
                            .overflow_hidden()
                            .when(dim, |t| t.text_color(muted))
                            .child(
                                h_flex()
                                    .id(("issue-link", row_ix))
                                    .flex_shrink_0()
                                    .cursor_pointer()
                                    .text_color(if dim { muted } else { theme.link })
                                    .hover(|this| this.underline())
                                    .child(issue.clone())
                                    .on_click(
                                        cx.listener(move |_, _, _, cx| cx.open_url(&issue_url)),
                                    ),
                            )
                            .child(div().min_w_0().truncate().child(row.title.clone()))
                    }
                    (Some(issue), None) => h_flex()
                        .gap_1()
                        .overflow_hidden()
                        .when(dim, |t| t.text_color(muted))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(tag_color)
                                .child(issue.clone()),
                        )
                        .child(div().min_w_0().truncate().child(row.title.clone())),
                    (None, _) => h_flex().overflow_hidden().child(
                        div()
                            .truncate()
                            .when(dim, |t| t.text_color(muted))
                            .child(row.title.clone()),
                    ),
                };
                return inner
                    .id(("title", row_ix))
                    .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    .into_any_element();
            }
            "note" => {
                // Dot carries the state; text stays calm — colored only when
                // the state needs action (spec §8).
                let text = strip_note_glyphs(&row.note);
                let full = text.clone();
                let (dot, text_color) = match row.category {
                    Category::Action | Category::Todo => (Some(theme.danger), theme.danger),
                    Category::Await | Category::Done => (Some(theme.success), muted),
                    Category::Draft => (None, muted),
                };
                // pr_2: the terminal column needs an optical margin the
                // 6px cell pad doesn't give (critique #4).
                let mut cell = h_flex().gap_1p5().items_center().overflow_hidden().pr_2();
                if let Some(color) = dot {
                    cell = cell.child(status_dot(color));
                }
                // Two-tone action notes (critique #1): the problem clause in
                // danger, the remedy after " — " muted — 14 identical red
                // rows stop flattening into one red wall.
                let action = matches!(row.category, Category::Action | Category::Todo);
                match text.split_once(" — ").filter(|_| action) {
                    Some((head, tail)) => {
                        cell = cell
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(text_color)
                                    .child(head.to_string()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(muted)
                                    .child(format!("— {tail}")),
                            );
                    }
                    None => {
                        cell = cell.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(text_color)
                                .child(text),
                        );
                    }
                }
                return cell
                    .id(("note", row_ix))
                    .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    .into_any_element();
            }
            _ => div(),
        };
        cell.into_any_element()
    }

    fn render_last_empty_col(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        // The default renders a 12px filler that reads as a stray empty
        // column header after Note. The Note column is elastic; no filler.
        div()
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        // Queue-specific empty copy (critique #6): each scope says its own
        // "nothing here" so an empty Review queue never reads as no authored
        // PRs. Text only — the default empty view pulls an SVG from an asset
        // bundle this app does not ship.
        let msg = match self.mode {
            Mode::Authored => "You have no open PRs",
            Mode::Review => "Nothing is waiting for your review",
        };
        h_flex()
            .size_full()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(msg)
    }
}
