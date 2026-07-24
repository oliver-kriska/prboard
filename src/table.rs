//! The board table: a `TableDelegate` over `Vec<BoardRow>` for
//! gpui-component's virtualized `Table`. Cells render the same glyph language
//! as the prototype's markdown dashboard (SKILL.md).
//!
//! Nothing here may animate: the table sits idle between refreshes and any
//! continuous animation would defeat the idle-GPU half of the spike gate.

use gpui::{
    div, px, App, Context, Div, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, Window,
};
use gpui_component::table::{Column, TableDelegate, TableState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, ActiveTheme};
use prboard_core::board::{BoardRow, Category, Ci, Mode};

pub struct BoardTableDelegate {
    rows: Vec<BoardRow>,
    columns: Vec<Column>,
}

impl BoardTableDelegate {
    pub fn new(mode: Mode) -> Self {
        // Column sets mirror the prototype dashboard's two tables (SKILL.md).
        let columns = match mode {
            // The Note is the highest-value column — it wins width over
            // Title; both carry hover tooltips with the full text, and every
            // column is drag-resizable.
            Mode::Authored => vec![
                Column::new("pr", "PR").width(px(76.)),
                Column::new("status", "Status").width(px(64.)),
                Column::new("bug", "Bug").width(px(44.)),
                Column::new("ci", "CI").width(px(64.)),
                Column::new("requested", "Requested").width(px(140.)),
                Column::new("reviewed", "Reviewed by").width(px(170.)),
                Column::new("title", "Title").width(px(360.)),
                Column::new("note", "Note").width(px(460.)),
            ],
            Mode::Review => vec![
                Column::new("pr", "PR").width(px(76.)),
                Column::new("author", "Author").width(px(120.)),
                Column::new("ci", "CI").width(px(64.)),
                Column::new("bug", "Bug").width(px(44.)),
                Column::new("unresolved", "Unres").width(px(56.)),
                Column::new("title", "Title").width(px(380.)),
                Column::new("note", "Note").width(px(460.)),
            ],
        };
        Self {
            rows: Vec::new(),
            columns,
        }
    }

    pub fn set_rows(&mut self, rows: Vec<BoardRow>) {
        self.rows = rows;
    }

    pub fn row(&self, ix: usize) -> Option<&BoardRow> {
        self.rows.get(ix)
    }
}

fn review_glyph(state: &str) -> &'static str {
    match state {
        "APPROVED" => "✅",
        "COMMENTED" => "💬",
        "CHANGES_REQUESTED" => "✋",
        "DISMISSED" => "🚫",
        _ => "·",
    }
}

impl TableDelegate for BoardTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
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
        match self.rows.get(row_ix).map(|r| r.category) {
            // A faint tint keeps "needs me" rows findable at a glance.
            Some(Category::Action | Category::Todo) => tr.bg(cx.theme().danger.opacity(0.06)),
            Some(Category::Draft) => tr.bg(cx.theme().muted.opacity(0.3)),
            _ => tr,
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let cell = match self.columns[col_ix].key.as_ref() {
            "pr" => div()
                .text_color(theme.link)
                .child(format!("#{}", row.number)),
            "status" => {
                if row.draft {
                    div().text_color(muted).child("DRAFT")
                } else {
                    div().child("ready")
                }
            }
            "bug" => {
                if row.bug {
                    div().child("🐛")
                } else {
                    div().text_color(muted).child("—")
                }
            }
            "ci" => match row.ci {
                Ci::Pass => div().text_color(theme.success).child("✓ pass"),
                Ci::Fail => div().text_color(theme.danger).child("✗ fail"),
                Ci::Running => div().text_color(theme.warning).child("⏳"),
                Ci::None => div().text_color(muted).child("—"),
            },
            "author" => div().child(row.author.clone().unwrap_or_else(|| "?".into())),
            "unresolved" => {
                if row.unresolved > 0 {
                    div()
                        .text_color(theme.warning)
                        .child(row.unresolved.to_string())
                } else {
                    div().text_color(muted).child("—")
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
                    div().text_color(muted).child("—")
                } else {
                    let text = row
                        .reviews
                        .iter()
                        .map(|r| {
                            format!(
                                "{} {}",
                                r.login.as_deref().unwrap_or("?"),
                                review_glyph(&r.state)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("  ");
                    div().child(text)
                }
            }
            "title" => {
                let full = match &row.issue {
                    Some(issue) => format!("{issue} · {}", row.title),
                    None => row.title.clone(),
                };
                let inner = match &row.issue {
                    Some(issue) => h_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_color(theme.accent_foreground)
                                .child(issue.clone()),
                        )
                        .child(row.title.clone()),
                    None => div().child(row.title.clone()),
                };
                return inner
                    .id(("title", row_ix))
                    .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    .into_any_element();
            }
            "note" => {
                let color = match row.category {
                    Category::Action | Category::Todo => theme.danger,
                    Category::Await | Category::Done => theme.success,
                    Category::Draft => muted,
                };
                let full = row.note.clone();
                return div()
                    .text_color(color)
                    .child(row.note.clone())
                    .id(("note", row_ix))
                    .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    .into_any_element();
            }
            _ => div(),
        };
        cell.into_any_element()
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        // Text only — the default empty view pulls an SVG icon from an asset
        // bundle this app does not ship.
        h_flex()
            .size_full()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No open PRs — nothing needs you 🎉")
    }
}
