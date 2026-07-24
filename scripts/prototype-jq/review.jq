# VERBATIM from the prototype ~/.claude/skills/pr-board/scripts/pr-board.sh
# (--review / Mode B branch), 2026-07-24. This is the behavioral spec the Rust
# port in core/src/board.rs must reproduce — do not "improve" it here.
# Args: --arg repo owner/name --arg me login
[ .data.search.nodes[]
  | ([.labels.nodes[].name] | index("bug")) as $bug
  | ([.reviewThreads.nodes[] | select(.isResolved==false)] | length) as $unres
  | (.commits.nodes[0].commit.statusCheckRollup.state // "NONE") as $ci
  | (($ci=="FAILURE") or ($ci=="ERROR")) as $cifail
  | (.mergeable=="CONFLICTING") as $conflict
  | ([.reviews.nodes[] | select(.author.login==$me)]
      | (if length>0 then (max_by(.submittedAt).state) else "NONE" end)) as $mine
  | (if .isDraft then "draft"
     elif ($mine=="APPROVED" or $mine=="COMMENTED" or $mine=="CHANGES_REQUESTED") then "done"
     else "todo" end) as $cat
  | ((.title | capture("(?<e>ENA-[0-9]+)").e) // null) as $ena
  | {
      number: .number,
      url: "https://github.com/\($repo)/pull/\(.number)",
      title: (.title | gsub("^WIP\\s*";"") | gsub("^\\[ENA-[0-9]+\\]\\s*";"")),
      issue: $ena,
      issueUrl: (if $ena then "https://linear.app/enaia-dev/issue/\($ena)" else null end),
      author: .author.login,
      draft: .isDraft,
      category: $cat,
      bug: ($bug != null),
      ci: (if $ci=="SUCCESS" then "pass" elif $cifail then "fail" elif $ci=="NONE" then "none" else "running" end),
      conflict: $conflict,
      myReview: $mine,
      unresolved: $unres,
      createdAt: .createdAt
    }
]
| sort_by( (if .category=="todo" then 0 elif .category=="done" then 1 else 2 end), .number )
