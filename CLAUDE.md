# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

prboard is an open-source GitHub PR-review dashboard: a windowed native desktop app in Rust (GPUI + gpui-component), macOS + Linux, MIT. **There is no code yet** — the repo holds a completed research spike (2026-07-24). Git is not initialized.

**Read `HANDOFF.md` first.** It is the executive summary of the research, written so a fresh session can start building from it alone. Evidence behind every claim is in `.claude/research/*.md` (indexed in HANDOFF §6). Claims are tagged FACT / ASSESSMENT / OPEN QUESTION — keep that discipline; never fabricate a number where an OPEN QUESTION belongs.

## Decisions already made — do not re-litigate

- **Windowed desktop app, NOT a TUI** (Oliver's product decision, 2026-07-24). The ratatui analysis in the UI-framework doc is a record of the alternative, not an option.
- **UI: GPUI + gpui-component.** Pin crates.io `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`, stable Rust. Use the delegate-based **`DataTable`**, not the stateless `Table`. Do NOT use Guise (no table/list/virtual scroll) and do NOT copy git-main gpui-component snippets (different bootstrap than the published crate).
- **Data: shell out to `gh api graphql`** behind a `GithubTransport` trait; one `search(type:ISSUE, first:60)` query per refresh (~3 rate-limit points/repo); never fan out per-PR REST.
- **Refresh: default 5 min, hard floor 30 s** (the "5 s floor" idea is wrong — math in the data-layer doc).
- **Auth: reuse `gh` CLI login**, behind a `TokenSource` trait; OAuth/PAT is post-v1.
- **v1 is read-only + open-in-browser.** No AI, no reviewer assignment, no merging, no Windows (full in/out list: HANDOFF §4).
- **Distribution:** macOS signed + notarized `.app` — hard-required (Homebrew enforces Gatekeeper on casks from 2026-09-01; quarantine applies regardless of tap). Pipeline: Zed's cargo-bundle fork → `codesign --options runtime` → `notarytool submit --wait` → `stapler staple` → hand-authored cask in own tap. cargo-dist cannot produce `.app`/cask. Linux: `.deb` preferred (Vulkan/fontconfig/xkbcommon runtime deps).

## Next step: the memory gate (blocks everything else)

Step 0 of the roadmap (HANDOFF §5): build the GPUI walking skeleton and measure **idle RSS over 12–24 h** (`ps -o rss=` per minute; `leaks`/`powermetrics` on macOS). Pass = flat and < ~150 MB with idle GPU ~0. Skeleton sketch to start from: `.claude/research/2026-07-24-gpui-desktop-build.md`. **If the gate fails, STOP and bring the numbers to Oliver** — there is no pre-committed fallback framework.

## Behavioral spec

- The shell prototype at `~/.claude/skills/pr-board/scripts/pr-board.sh` (+ its `SKILL.md`) is the spec for the GraphQL query, categorization (action/await/draft), and Note logic. It is transcribed in full in `.claude/research/2026-07-24-github-data-layer.md`. v1 = port it to Rust behind a refreshing UI; unit-test categorization against the prototype's output.
- Reference codebases on disk: `~/Projects/pr_flow` (previous GPUI attempt — read its `SOLUTIONS_EXTRACTED.md` for the unbounded-cache memory bug that killed it, and mine its gpui-component table/state code), `~/Projects/agentrix` and `~/Projects/vibenalytics` (shipped Rust TUIs, for patterns and distribution).

## Guardrails (from the PRFlow post-mortem — encode from line one)

- Bound every cache: explicit `MAX_*` + FIFO eviction; no unbounded `HashMap`.
- Clamp every rate-limit wait to `max(60).min(900)`; never sleep until a far-future reset.
- No perpetually animating spinner — GPUI renders on demand, and continuous animation defeats the idle-GPU property the framework was chosen for.
- Don't pull in a runtime you don't use (a thread + channel may suffice over full tokio).
- Always show "last synced Xm ago" + the live `rateLimit{}` budget.
- Keep transport, token source, and issue-link detection behind traits/config — no hard-coded issue prefixes.

## Build conventions (once code exists)

- Standard cargo workflow; CI must run `cargo clippy -- -D warnings` and stay clean from the first commit.
- Config lives in TOML at `$XDG_CONFIG_HOME/prboard/config.toml`.
- Name note: `prboard` is free on crates.io but taken on GitHub — publish as `oliver-kriska/prboard` or rename (`prwall`/`pullboard` were candidates).
