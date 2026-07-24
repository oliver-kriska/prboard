//! The board view-model: `RawPr` → `BoardRow` (category, CI, review state,
//! unresolved count, Note). Ported from the shell prototype's `jq` programs
//! (categorization) and `SKILL.md` (Note composition); the golden tests in
//! `tests/parity.rs` pin this module to the prototype's actual output.

use regex::Regex;

use crate::github::query::RawPr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// PRs the user authored — the outgoing queue.
    Authored,
    /// PRs awaiting the user's review — the incoming queue.
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    // Authored mode
    Action,
    Await,
    // Review mode
    Todo,
    Done,
    // Both
    Draft,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Action => "action",
            Category::Await => "await",
            Category::Todo => "todo",
            Category::Done => "done",
            Category::Draft => "draft",
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Category::Action | Category::Todo => 0,
            Category::Await | Category::Done => 1,
            Category::Draft => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ci {
    Pass,
    Fail,
    None,
    Running,
}

impl Ci {
    pub fn as_str(&self) -> &'static str {
        match self {
            Ci::Pass => "pass",
            Ci::Fail => "fail",
            Ci::None => "none",
            Ci::Running => "running",
        }
    }
}

/// Aggregate of completed human reviews on an authored PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Changes,
    Approved,
    Commented,
    Waiting,
    None,
}

impl ReviewState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewState::Changes => "changes",
            ReviewState::Approved => "approved",
            ReviewState::Commented => "commented",
            ReviewState::Waiting => "waiting",
            ReviewState::None => "none",
        }
    }
}

/// Latest review per author. `login` is `None` for reviews whose author no
/// longer exists (the prototype keeps them too).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSummary {
    pub login: Option<String>,
    pub state: String,
}

/// Optional "linked ticket" extraction: a pattern matched against the PR
/// title and a URL template with an `{id}` placeholder. Never hard-code a
/// tracker (the prototype's `ENA-`/Linear pair becomes user config).
#[derive(Debug, Clone)]
pub struct IssueLinkRule {
    pattern: Regex,
    url_template: String,
    /// Strips a leading `[<match>] ` from the displayed title.
    strip_prefix: Regex,
}

impl IssueLinkRule {
    pub fn new(pattern: &str, url_template: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            pattern: Regex::new(pattern)?,
            url_template: url_template.to_string(),
            strip_prefix: Regex::new(&format!(r"^\[(?:{pattern})\]\s*"))?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BoardConfig {
    /// Review authors that never count as human review (prototype default).
    pub bots: Vec<String>,
    /// Suggested reviewers for the "no reviewers — assign …" note.
    pub default_reviewers: Vec<String>,
    pub issue_link: Option<IssueLinkRule>,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            bots: vec!["chatgpt-codex-connector".into(), "github-actions".into()],
            default_reviewers: Vec::new(),
            issue_link: None,
        }
    }
}

/// One row of the dashboard. Fields not applicable to the row's mode are
/// empty/None (`review_*`/`requested`/`reviews` are authored-mode; `author`/
/// `my_review` are review-mode).
#[derive(Debug, Clone)]
pub struct BoardRow {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub issue: Option<String>,
    pub issue_url: Option<String>,
    pub author: Option<String>,
    pub draft: bool,
    pub category: Category,
    pub bug: bool,
    pub ci: Ci,
    pub conflict: bool,
    pub review_decision: Option<String>,
    pub review_state: ReviewState,
    pub requested: Vec<String>,
    pub reviews: Vec<ReviewSummary>,
    pub my_review: Option<String>,
    pub unresolved: usize,
    pub created_at: String,
    pub note: String,
}

/// Defensive bound on the board size. The query already caps at `first:60`;
/// this keeps the bound explicit at the data boundary (bounded-everything
/// guardrail from the PRFlow post-mortem).
pub const MAX_BOARD_ROWS: usize = 60;

/// Derive and sort the full board. `me` must be the resolved login.
pub fn derive_rows(
    prs: &[RawPr],
    mode: Mode,
    repo: &str,
    me: &str,
    cfg: &BoardConfig,
) -> Vec<BoardRow> {
    let mut rows: Vec<BoardRow> = prs
        .iter()
        .take(MAX_BOARD_ROWS)
        .map(|pr| derive_row(pr, mode, repo, me, cfg))
        .collect();
    match mode {
        // action → await → draft, newest first within each.
        Mode::Authored => rows.sort_by_key(|r| (r.category.rank(), std::cmp::Reverse(r.number))),
        // todo → done → draft, oldest first — clear the backlog.
        Mode::Review => rows.sort_by_key(|r| (r.category.rank(), r.number)),
    }
    rows
}

fn derive_row(pr: &RawPr, mode: Mode, repo: &str, me: &str, cfg: &BoardConfig) -> BoardRow {
    let bug = pr.labels.nodes.iter().any(|l| l.name == "bug");
    let unresolved = pr
        .review_threads
        .nodes
        .iter()
        .filter(|t| !t.is_resolved)
        .count();
    let ci = derive_ci(pr);
    let conflict = pr.mergeable.as_deref() == Some("CONFLICTING");
    let (issue, issue_url, title) = derive_title(&pr.title, cfg);
    let url = format!("https://github.com/{repo}/pull/{}", pr.number);

    let mut row = BoardRow {
        number: pr.number,
        url,
        title,
        issue,
        issue_url,
        author: pr.author.as_ref().and_then(|a| a.login.clone()),
        draft: pr.is_draft,
        category: Category::Draft, // set below
        bug,
        ci,
        conflict,
        review_decision: pr.review_decision.clone(),
        review_state: ReviewState::None,
        requested: Vec::new(),
        reviews: Vec::new(),
        my_review: None,
        unresolved,
        created_at: pr.created_at.clone(),
        note: String::new(),
    };

    match mode {
        Mode::Authored => {
            row.requested = requested_reviewers(pr);
            row.reviews = latest_reviews_excluding(pr, me, &cfg.bots);
            let appr = row.reviews.iter().filter(|r| r.state == "APPROVED").count();
            let cmt = row.reviews.iter().any(|r| r.state == "COMMENTED");
            let chg = row.reviews.iter().any(|r| r.state == "CHANGES_REQUESTED");
            row.review_state = if chg {
                ReviewState::Changes
            } else if appr > 0 {
                ReviewState::Approved
            } else if cmt {
                ReviewState::Commented
            } else if !row.requested.is_empty() {
                ReviewState::Waiting
            } else {
                ReviewState::None
            };
            row.category = if pr.is_draft {
                Category::Draft
            } else if ci == Ci::Fail
                || conflict
                || pr.review_decision.as_deref() == Some("CHANGES_REQUESTED")
                || unresolved > 0
                || row.review_state == ReviewState::None
            {
                Category::Action
            } else {
                Category::Await
            };
            row.note = authored_note(&row, cfg);
        }
        Mode::Review => {
            let mine = my_latest_review(pr, me);
            row.category = if pr.is_draft {
                Category::Draft
            } else if matches!(
                mine.as_str(),
                "APPROVED" | "COMMENTED" | "CHANGES_REQUESTED"
            ) {
                Category::Done
            } else {
                Category::Todo
            };
            row.my_review = Some(mine);
            row.note = review_note(&row);
        }
    }
    row
}

fn derive_ci(pr: &RawPr) -> Ci {
    let state = pr
        .commits
        .nodes
        .first()
        .and_then(|c| c.commit.status_check_rollup.as_ref())
        .and_then(|r| r.state.as_deref())
        .unwrap_or("NONE");
    match state {
        "SUCCESS" => Ci::Pass,
        "FAILURE" | "ERROR" => Ci::Fail,
        "NONE" => Ci::None,
        _ => Ci::Running, // PENDING / EXPECTED
    }
}

fn requested_reviewers(pr: &RawPr) -> Vec<String> {
    pr.review_requests
        .nodes
        .iter()
        .filter_map(|n| n.requested_reviewer.as_ref())
        .filter_map(|r| r.login.clone().or_else(|| r.slug.clone()))
        .collect()
}

/// Latest review state per author, excluding the PR author and bots — the
/// prototype's `$rv`. Ordered by login (`group_by` sorts; null first).
fn latest_reviews_excluding(pr: &RawPr, me: &str, bots: &[String]) -> Vec<ReviewSummary> {
    let mut latest: Vec<(Option<String>, &str, Option<&str>)> = Vec::new(); // (login, state, submitted_at)
    for review in &pr.reviews.nodes {
        let login = review.author.as_ref().and_then(|a| a.login.clone());
        if let Some(l) = &login {
            if l == me || bots.iter().any(|b| b == l) {
                continue;
            }
        }
        let submitted = review.submitted_at.as_deref();
        match latest.iter_mut().find(|(l, _, _)| *l == login) {
            // Later-or-equal submittedAt wins, like jq's max_by.
            Some(entry) => {
                if submitted >= entry.2 {
                    entry.1 = &review.state;
                    entry.2 = submitted;
                }
            }
            None => latest.push((login, &review.state, submitted)),
        }
    }
    latest.sort_by(|a, b| a.0.cmp(&b.0));
    latest
        .into_iter()
        .map(|(login, state, _)| ReviewSummary {
            login,
            state: state.to_string(),
        })
        .collect()
}

/// The user's own latest review state, or "NONE" — the prototype's `$mine`.
fn my_latest_review(pr: &RawPr, me: &str) -> String {
    pr.reviews
        .nodes
        .iter()
        .filter(|r| r.author.as_ref().and_then(|a| a.login.as_deref()) == Some(me))
        .max_by(|a, b| a.submitted_at.cmp(&b.submitted_at))
        .map(|r| r.state.clone())
        .unwrap_or_else(|| "NONE".to_string())
}

fn derive_title(raw_title: &str, cfg: &BoardConfig) -> (Option<String>, Option<String>, String) {
    let (issue, issue_url) = match &cfg.issue_link {
        Some(rule) => match rule.pattern.find(raw_title) {
            Some(m) => {
                let id = m.as_str().to_string();
                let url = rule.url_template.replace("{id}", &id);
                (Some(id), Some(url))
            }
            None => (None, None),
        },
        None => (None, None),
    };

    let mut title = raw_title.to_string();
    if let Some(rest) = title.strip_prefix("WIP") {
        title = rest.trim_start().to_string();
    }
    if let Some(rule) = &cfg.issue_link {
        title = rule.strip_prefix.replace(&title, "").to_string();
    }
    (issue, issue_url, title)
}

/// Mode A Note (SKILL.md): action rows combine every applicable blocker,
/// most-blocking first; await/draft rows are single-state.
fn authored_note(row: &BoardRow, cfg: &BoardConfig) -> String {
    match row.category {
        Category::Action => {
            let mut parts: Vec<String> = Vec::new();
            if row.review_state == ReviewState::None {
                if cfg.default_reviewers.is_empty() {
                    parts.push("⚠️ no reviewers".to_string());
                } else {
                    parts.push(format!(
                        "⚠️ no reviewers — assign {}",
                        cfg.default_reviewers.join(" + ")
                    ));
                }
            }
            if row.conflict {
                parts.push("🔴 merge conflict — rebase".to_string());
            }
            if row.ci == Ci::Fail {
                parts.push("❌ CI failing".to_string());
            }
            if row.review_decision.as_deref() == Some("CHANGES_REQUESTED") {
                parts.push("✋ changes requested".to_string());
            }
            if row.unresolved > 0 {
                parts.push(format!("🟡 {} unresolved comments", row.unresolved));
            }
            parts.join(" · ")
        }
        Category::Await => match row.review_state {
            ReviewState::Approved => "🟢 approved — mergeable".to_string(),
            ReviewState::Commented => "🟢 commented — awaiting approval".to_string(),
            _ => "✅ awaiting review".to_string(),
        },
        Category::Draft => {
            if row.conflict {
                "🔴 draft · merge conflict".to_string()
            } else if row.unresolved > 0 {
                format!("🟡 draft · {} unresolved comments", row.unresolved)
            } else if row.ci == Ci::Fail {
                "🔴 draft · CI failing".to_string()
            } else {
                "· draft".to_string()
            }
        }
        _ => String::new(),
    }
}

/// Mode B Note (SKILL.md).
fn review_note(row: &BoardRow) -> String {
    match row.category {
        Category::Todo => {
            if row.ci == Ci::Fail {
                "⚠️ CI red — maybe wait for green".to_string()
            } else if row.conflict {
                "⚠️ has conflicts".to_string()
            } else {
                "🔵 needs your review".to_string()
            }
        }
        Category::Done => match row.my_review.as_deref() {
            Some("APPROVED") => "✅ you approved".to_string(),
            Some("CHANGES_REQUESTED") => "✋ you requested changes — on the author now".to_string(),
            Some("COMMENTED") => "💬 you commented".to_string(),
            _ => String::new(),
        },
        Category::Draft => "· draft (not ready)".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> BoardConfig {
        BoardConfig {
            default_reviewers: vec!["mkurkov".into(), "abs".into()],
            issue_link: Some(
                IssueLinkRule::new("ENA-[0-9]+", "https://linear.app/enaia-dev/issue/{id}")
                    .unwrap(),
            ),
            ..Default::default()
        }
    }

    fn pr(v: serde_json::Value) -> RawPr {
        serde_json::from_value(v).unwrap()
    }

    fn base(number: u64) -> serde_json::Value {
        json!({
            "number": number,
            "title": "Some change",
            "isDraft": false,
            "reviewDecision": null,
            "mergeable": "MERGEABLE",
            "createdAt": "2026-07-20T10:00:00Z",
            "author": {"login": "oliver"},
            "labels": {"nodes": []},
            "reviewRequests": {"nodes": []},
            "reviews": {"nodes": []},
            "reviewThreads": {"nodes": []},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "SUCCESS"}}}]}
        })
    }

    fn derive_one(v: serde_json::Value, mode: Mode) -> BoardRow {
        derive_rows(&[pr(v)], mode, "acme/widgets", "oliver", &cfg())
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn no_reviewers_is_action_with_assign_note() {
        let row = derive_one(base(1), Mode::Authored);
        assert_eq!(row.category, Category::Action);
        assert_eq!(row.review_state, ReviewState::None);
        assert_eq!(row.note, "⚠️ no reviewers — assign mkurkov + abs");
    }

    #[test]
    fn approved_is_await_mergeable() {
        let mut v = base(2);
        v["reviews"]["nodes"] = json!([
            {"author": {"login": "alice"}, "state": "APPROVED", "submittedAt": "2026-07-21T10:00:00Z"}
        ]);
        let row = derive_one(v, Mode::Authored);
        assert_eq!(row.category, Category::Await);
        assert_eq!(row.review_state, ReviewState::Approved);
        assert_eq!(row.note, "🟢 approved — mergeable");
    }

    #[test]
    fn action_note_combines_in_blocking_order() {
        let mut v = base(3);
        v["mergeable"] = json!("CONFLICTING");
        v["reviewDecision"] = json!("CHANGES_REQUESTED");
        v["commits"]["nodes"] = json!([{"commit": {"statusCheckRollup": {"state": "FAILURE"}}}]);
        v["reviewThreads"]["nodes"] = json!([
            {"isResolved": false}, {"isResolved": false}, {"isResolved": true}
        ]);
        let row = derive_one(v, Mode::Authored);
        assert_eq!(row.category, Category::Action);
        assert_eq!(
            row.note,
            "⚠️ no reviewers — assign mkurkov + abs · 🔴 merge conflict — rebase · \
             ❌ CI failing · ✋ changes requested · 🟡 2 unresolved comments"
        );
    }

    #[test]
    fn approved_with_unresolved_is_still_action() {
        let mut v = base(4);
        v["reviews"]["nodes"] = json!([
            {"author": {"login": "grace"}, "state": "APPROVED", "submittedAt": "2026-07-21T10:00:00Z"}
        ]);
        v["reviewThreads"]["nodes"] = json!([{"isResolved": false}, {"isResolved": false}]);
        let row = derive_one(v, Mode::Authored);
        assert_eq!(row.category, Category::Action);
        assert_eq!(row.review_state, ReviewState::Approved);
        assert_eq!(row.note, "🟡 2 unresolved comments");
    }

    #[test]
    fn bot_and_own_reviews_are_excluded() {
        let mut v = base(5);
        v["reviews"]["nodes"] = json!([
            {"author": {"login": "github-actions"}, "state": "COMMENTED", "submittedAt": "2026-07-21T09:00:00Z"},
            {"author": {"login": "chatgpt-codex-connector"}, "state": "COMMENTED", "submittedAt": "2026-07-21T09:05:00Z"},
            {"author": {"login": "oliver"}, "state": "COMMENTED", "submittedAt": "2026-07-21T09:10:00Z"}
        ]);
        let row = derive_one(v, Mode::Authored);
        assert!(row.reviews.is_empty());
        assert_eq!(row.review_state, ReviewState::None);
        assert_eq!(row.category, Category::Action);
    }

    #[test]
    fn latest_review_per_author_wins() {
        let mut v = base(6);
        v["reviews"]["nodes"] = json!([
            {"author": {"login": "eve"}, "state": "COMMENTED", "submittedAt": "2026-07-21T09:00:00Z"},
            {"author": {"login": "eve"}, "state": "APPROVED", "submittedAt": "2026-07-22T09:00:00Z"}
        ]);
        let row = derive_one(v, Mode::Authored);
        assert_eq!(
            row.reviews,
            vec![ReviewSummary {
                login: Some("eve".into()),
                state: "APPROVED".into()
            }]
        );
        assert_eq!(row.review_state, ReviewState::Approved);
    }

    #[test]
    fn team_slug_counts_as_requested_reviewer() {
        let mut v = base(7);
        v["reviewRequests"]["nodes"] = json!([
            {"requestedReviewer": {"__typename": "Team", "slug": "platform"}},
            {"requestedReviewer": null}
        ]);
        let row = derive_one(v, Mode::Authored);
        assert_eq!(row.requested, vec!["platform"]);
        assert_eq!(row.review_state, ReviewState::Waiting);
        assert_eq!(row.category, Category::Await);
        assert_eq!(row.note, "✅ awaiting review");
    }

    #[test]
    fn title_issue_extraction_and_stripping() {
        let mut v = base(8);
        v["title"] = json!("WIP [ENA-1234] Fix the crash");
        let row = derive_one(v, Mode::Authored);
        assert_eq!(row.issue.as_deref(), Some("ENA-1234"));
        assert_eq!(
            row.issue_url.as_deref(),
            Some("https://linear.app/enaia-dev/issue/ENA-1234")
        );
        assert_eq!(row.title, "Fix the crash");
    }

    #[test]
    fn draft_notes_first_match_wins() {
        let mut v = base(9);
        v["isDraft"] = json!(true);
        v["commits"]["nodes"] = json!([{"commit": {"statusCheckRollup": {"state": "FAILURE"}}}]);
        assert_eq!(
            derive_one(v.clone(), Mode::Authored).note,
            "🔴 draft · CI failing"
        );
        v["mergeable"] = json!("CONFLICTING");
        assert_eq!(
            derive_one(v.clone(), Mode::Authored).note,
            "🔴 draft · merge conflict"
        );
        v["mergeable"] = json!("MERGEABLE");
        v["commits"]["nodes"] = json!([{"commit": {"statusCheckRollup": {"state": "SUCCESS"}}}]);
        v["reviewThreads"]["nodes"] = json!([{"isResolved": false}]);
        assert_eq!(
            derive_one(v, Mode::Authored).note,
            "🟡 draft · 1 unresolved comments"
        );
    }

    #[test]
    fn review_mode_categories_and_notes() {
        // Not yet reviewed, green.
        let mut v = base(20);
        v["author"] = json!({"login": "petra"});
        let row = derive_one(v.clone(), Mode::Review);
        assert_eq!(row.category, Category::Todo);
        assert_eq!(row.my_review.as_deref(), Some("NONE"));
        assert_eq!(row.note, "🔵 needs your review");

        // CI red beats conflicts.
        v["commits"]["nodes"] = json!([{"commit": {"statusCheckRollup": {"state": "ERROR"}}}]);
        v["mergeable"] = json!("CONFLICTING");
        assert_eq!(
            derive_one(v.clone(), Mode::Review).note,
            "⚠️ CI red — maybe wait for green"
        );

        // I approved → done.
        v["commits"]["nodes"] = json!([{"commit": {"statusCheckRollup": {"state": "SUCCESS"}}}]);
        v["mergeable"] = json!("MERGEABLE");
        v["reviews"]["nodes"] = json!([
            {"author": {"login": "oliver"}, "state": "APPROVED", "submittedAt": "2026-07-21T10:00:00Z"}
        ]);
        let row = derive_one(v.clone(), Mode::Review);
        assert_eq!(row.category, Category::Done);
        assert_eq!(row.note, "✅ you approved");

        // Draft trumps everything.
        v["isDraft"] = json!(true);
        assert_eq!(derive_one(v, Mode::Review).note, "· draft (not ready)");
    }

    #[test]
    fn sort_orders_differ_by_mode() {
        let mk = |n: u64, draft: bool, reviewed: bool| {
            let mut v = base(n);
            v["isDraft"] = json!(draft);
            if reviewed {
                v["reviews"]["nodes"] = json!([
                    {"author": {"login": "alice"}, "state": "APPROVED", "submittedAt": "2026-07-21T10:00:00Z"}
                ]);
            }
            pr(v)
        };
        // 10=action (no reviewers), 11=await (approved), 12=draft, 13=action
        let prs = vec![
            mk(10, false, false),
            mk(11, false, true),
            mk(12, true, false),
            mk(13, false, false),
        ];
        let authored = derive_rows(&prs, Mode::Authored, "acme/widgets", "oliver", &cfg());
        let order: Vec<u64> = authored.iter().map(|r| r.number).collect();
        assert_eq!(order, vec![13, 10, 11, 12]); // action desc, then await, then draft

        // Review mode: 10,13 todo (asc), 11 done (oliver didn't review → todo!, alice did)
        // — for review-mode sorting use my own reviews instead:
        let mk_r = |n: u64, mine: bool| {
            let mut v = base(n);
            v["author"] = json!({"login": "petra"});
            if mine {
                v["reviews"]["nodes"] = json!([
                    {"author": {"login": "oliver"}, "state": "APPROVED", "submittedAt": "2026-07-21T10:00:00Z"}
                ]);
            }
            pr(v)
        };
        let prs = vec![mk_r(31, true), mk_r(30, false), mk_r(28, false)];
        let review = derive_rows(&prs, Mode::Review, "acme/widgets", "oliver", &cfg());
        let order: Vec<u64> = review.iter().map(|r| r.number).collect();
        assert_eq!(order, vec![28, 30, 31]); // todo asc, then done
    }

    #[test]
    fn rows_are_bounded() {
        let prs: Vec<RawPr> = (0..100).map(|n| pr(base(n))).collect();
        let rows = derive_rows(&prs, Mode::Authored, "acme/widgets", "oliver", &cfg());
        assert_eq!(rows.len(), MAX_BOARD_ROWS);
    }
}
