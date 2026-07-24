# VERBATIM from the prototype ~/.claude/skills/pr-board/scripts/pr-board.sh
# (authored / Mode A branch), 2026-07-24. This is the behavioral spec the Rust
# port in core/src/board.rs must reproduce — do not "improve" it here.
# Args: --arg repo owner/name --arg me login
def bots: ["chatgpt-codex-connector","github-actions"];
[ .data.search.nodes[]
  | ([.labels.nodes[].name] | index("bug")) as $bug
  | ([.reviewThreads.nodes[] | select(.isResolved==false)] | length) as $unres
  | (.commits.nodes[0].commit.statusCheckRollup.state // "NONE") as $ci
  | ([.reviewRequests.nodes[].requestedReviewer | (.login // .slug) | select(.!=null)]) as $req
  | ([.reviews.nodes[]
        | select(.author.login as $a | ($a!=$me) and (bots|index($a)|not))]
      | group_by(.author.login)
      | map({login:.[0].author.login, state:(max_by(.submittedAt).state)})) as $rv
  | ($rv | map(select(.state=="APPROVED")) | length) as $appr
  | ($rv | any(.state=="COMMENTED")) as $cmt
  | ($rv | any(.state=="CHANGES_REQUESTED")) as $chg
  | (.mergeable=="CONFLICTING") as $conflict
  | (($ci=="FAILURE") or ($ci=="ERROR")) as $cifail
  | (if $chg then "changes" elif $appr>0 then "approved" elif $cmt then "commented"
     elif ($req|length)>0 then "waiting" else "none" end) as $rflag
  | (if .isDraft then "draft"
     elif ($cifail or $conflict or (.reviewDecision=="CHANGES_REQUESTED") or ($unres>0) or ($rflag=="none")) then "action"
     else "await" end) as $cat
  | ((.title | capture("(?<e>ENA-[0-9]+)").e) // null) as $ena
  | {
      number: .number,
      url: "https://github.com/\($repo)/pull/\(.number)",
      title: (.title | gsub("^WIP\\s*";"") | gsub("^\\[ENA-[0-9]+\\]\\s*";"")),
      issue: $ena,
      issueUrl: (if $ena then "https://linear.app/enaia-dev/issue/\($ena)" else null end),
      draft: .isDraft,
      category: $cat,
      bug: ($bug != null),
      ci: (if $ci=="SUCCESS" then "pass" elif $cifail then "fail" elif $ci=="NONE" then "none" else "running" end),
      conflict: $conflict,
      reviewDecision: (.reviewDecision // null),
      reviewState: $rflag,
      requested: $req,
      reviews: $rv,
      unresolved: $unres
    }
]
| sort_by( (if .category=="action" then 0 elif .category=="await" then 1 else 2 end), (- .number) )
