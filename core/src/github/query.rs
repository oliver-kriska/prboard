//! The one GraphQL query (verbatim from the prototype, plus the free
//! `rateLimit{}` field) and the raw response model.
//!
//! Never fan out per-PR REST calls — this single query is the whole data
//! layer (~3 rate-limit points per repo refresh).

use serde::Deserialize;
use serde_json::Value;

use super::rate_limit::RateLimitInfo;
use super::GhError;

/// Search query string for a view. `who` must be a resolved login, not `@me`
/// (GraphQL search does not expand `@me`; the prototype resolves it first).
pub fn search_string(mode: crate::board::Mode, repo: &str, who: &str) -> String {
    match mode {
        crate::board::Mode::Authored => format!("repo:{repo} is:pr is:open author:{who}"),
        crate::board::Mode::Review => {
            format!("repo:{repo} is:pr is:open review-requested:{who} -author:{who}")
        }
    }
}

/// Verbatim from `pr-board.sh`, with `rateLimit{}` appended (costs nothing,
/// lets the UI show the live budget).
pub const PR_SEARCH_QUERY: &str = r#"query($q:String!){
  search(query:$q, type:ISSUE, first:60){
    nodes{ ... on PullRequest {
      number title isDraft reviewDecision mergeable createdAt
      author{ login }
      labels(first:20){ nodes{ name } }
      reviewRequests(first:15){ nodes{ requestedReviewer{ __typename ... on User{login} ... on Team{slug} } } }
      reviews(first:60){ nodes{ author{login} state submittedAt } }
      reviewThreads(first:100){ nodes{ isResolved } }
      commits(last:1){ nodes{ commit{ statusCheckRollup{ state } } } }
    } }
  }
  rateLimit { limit cost remaining resetAt }
}"#;

#[derive(Debug, Clone, Deserialize)]
pub struct Nodes<T> {
    #[serde(default = "Vec::new")]
    pub nodes: Vec<T>,
}

impl<T> Default for Nodes<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Login {
    pub login: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelNode {
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestedReviewer {
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequestNode {
    #[serde(default)]
    pub requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewNode {
    #[serde(default)]
    pub author: Option<Login>,
    pub state: String,
    #[serde(default)]
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadNode {
    pub is_resolved: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCheckRollup {
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    #[serde(default)]
    pub status_check_rollup: Option<StatusCheckRollup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitNode {
    pub commit: Commit,
}

/// One PR as returned by the search query. Field names follow GitHub's schema;
/// the product view-model lives in [`crate::board::BoardRow`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPr {
    pub number: u64,
    pub title: String,
    pub is_draft: bool,
    #[serde(default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub mergeable: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub author: Option<Login>,
    #[serde(default)]
    pub labels: Nodes<LabelNode>,
    #[serde(default)]
    pub review_requests: Nodes<ReviewRequestNode>,
    #[serde(default)]
    pub reviews: Nodes<ReviewNode>,
    #[serde(default)]
    pub review_threads: Nodes<ThreadNode>,
    #[serde(default)]
    pub commits: Nodes<CommitNode>,
}

/// Parse the full `gh api graphql` response body.
///
/// GraphQL errors are classified (RATE_LIMITED vs the rest); nodes that are
/// not PullRequests (empty inline-fragment objects) are skipped.
pub fn parse_search_response(body: &Value) -> Result<(Vec<RawPr>, Option<RateLimitInfo>), GhError> {
    if let Some(errors) = body.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            let rate_limited = errors
                .iter()
                .any(|e| e.get("type").and_then(Value::as_str) == Some("RATE_LIMITED"));
            if rate_limited {
                return Err(GhError::RateLimited { reset_epoch: None });
            }
            if body.get("data").is_none_or(Value::is_null) {
                let msgs = errors
                    .iter()
                    .map(|e| {
                        e.get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error")
                            .to_string()
                    })
                    .collect();
                return Err(GhError::GraphqlErrors(msgs));
            }
        }
    }

    let nodes = body
        .pointer("/data/search/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::Parse("missing data.search.nodes".into()))?;

    let mut prs = Vec::with_capacity(nodes.len());
    for node in nodes {
        // Non-PR search hits surface as `{}` through the inline fragment.
        if node.as_object().is_some_and(|o| o.is_empty()) {
            continue;
        }
        let pr: RawPr = serde_json::from_value(node.clone())
            .map_err(|e| GhError::Parse(format!("bad PullRequest node: {e}")))?;
        prs.push(pr);
    }

    let rate = body
        .pointer("/data/rateLimit")
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value::<RateLimitInfo>(v.clone()).ok());

    Ok((prs, rate))
}
