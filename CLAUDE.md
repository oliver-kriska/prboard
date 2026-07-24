# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

prboard is an open-source GitHub PR-review dashboard: a windowed native desktop app in Rust (GPUI + gpui-component), macOS + Linux, MIT. **The Step-0 walking skeleton is built and running** (2026-07-24): workspace with `core/` (`prboard-core` — categorization/Note/transport, no GPUI dep, golden-tested against the shell prototype) and the root `prboard` GPUI binary. The framework gate (overnight memory measurement) may still be pending — check `measurements/` and the git log before assuming it passed.

**Read `HANDOFF.md` first.** It is the executive summary of the research, written so a fresh session can start building from it alone. Evidence behind every claim is in `.claude/research/*.md` (indexed in HANDOFF §6). Claims are tagged FACT / ASSESSMENT / OPEN QUESTION — keep that discipline; never fabricate a number where an OPEN QUESTION belongs.

## Decisions already made — do not re-litigate

- **Windowed desktop app, NOT a TUI** (Oliver's product decision, 2026-07-24). The ratatui analysis in the UI-framework doc is a record of the alternative, not an option.
- **UI: GPUI + gpui-component.** Pin crates.io `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`, stable Rust. The virtualized delegate-based table in the published 0.5.1 is **`Table` + `TableState` + `TableDelegate`** — the `DataTable` name exists only on git main (build-findings doc #1). Do NOT use Guise (no table/list/virtual scroll) and do NOT copy git-main gpui-component snippets (different bootstrap than the published crate).
- **Data: shell out to `gh api graphql`** behind a `GithubTransport` trait; one `search(type:ISSUE, first:60)` query per refresh (~3 rate-limit points/repo); never fan out per-PR REST.
- **Refresh: default 5 min, hard floor 30 s** (the "5 s floor" idea is wrong — math in the data-layer doc).
- **Auth: reuse `gh` CLI login**, behind a `TokenSource` trait; OAuth/PAT is post-v1.
- **v1 is read-only + open-in-browser.** No AI, no reviewer assignment, no merging, no Windows (full in/out list: HANDOFF §4).
- **Distribution:** macOS signed + notarized `.app` — hard-required (Homebrew enforces Gatekeeper on casks from 2026-09-01; quarantine applies regardless of tap). Pipeline: Zed's cargo-bundle fork → `codesign --options runtime` → `notarytool submit --wait` → `stapler staple` → hand-authored cask in own tap. cargo-dist cannot produce `.app`/cask. Linux: `.deb` preferred (Vulkan/fontconfig/xkbcommon runtime deps).

## The memory gate (blocks Step 1+)

The framework choice is validated by an overnight measurement, not assumed: **idle RSS flat and < ~150 MB over 12–24 h, ~0 idle GPU**. Tooling exists: `scripts/spike-run.sh <owner/repo>` (app + per-minute RSS sampler + caffeinate), `scripts/measure-summary.sh` for the verdict, `leaks`/`sudo powermetrics --samplers gpu_power` for depth. **If the gate fails, STOP and bring the numbers to Oliver** — there is no pre-committed fallback framework. Do not start tabs/filters/config (Step 3) before the gate passes.

## Behavioral spec

- The shell prototype at `~/.claude/skills/pr-board/scripts/pr-board.sh` (+ its `SKILL.md`) is the spec for the GraphQL query, categorization (action/await/draft), and Note logic. It is transcribed in full in `.claude/research/2026-07-24-github-data-layer.md`. The Rust port lives in `core/src/board.rs` and is **pinned by golden tests**: `scripts/prototype-jq/` holds the prototype's verbatim jq, `scripts/gen-golden.sh` regenerates `core/tests/golden/`, and `core/tests/parity.rs` asserts field-for-field equality. If parity fails, fix the port, never the golden.
- Reference codebases on disk: `~/Projects/pr_flow` (previous GPUI attempt — read its `SOLUTIONS_EXTRACTED.md` for the unbounded-cache memory bug that killed it, and mine its gpui-component table/state code), `~/Projects/agentrix` and `~/Projects/vibenalytics` (shipped Rust TUIs, for patterns and distribution).

## Guardrails (from the PRFlow post-mortem — encode from line one)

- Bound every cache: explicit `MAX_*` + FIFO eviction; no unbounded `HashMap`.
- Clamp every rate-limit wait to `max(60).min(900)`; never sleep until a far-future reset.
- No perpetually animating spinner — GPUI renders on demand, and continuous animation defeats the idle-GPU property the framework was chosen for.
- Don't pull in a runtime you don't use (a thread + channel may suffice over full tokio).
- Always show "last synced Xm ago" + the live `rateLimit{}` budget.
- Keep transport, token source, and issue-link detection behind traits/config — no hard-coded issue prefixes.

## Building and testing

- **Toolchain:** latest stable Rust — the dep graph hard-requires ≥ 1.88 (1.97 in use). macOS needs the Xcode **Metal Toolchain** (`xcodebuild -downloadComponent MetalToolchain`) or gpui's shader build fails.
- `cargo test -p prboard-core` — the fast spec suite (no GPUI compile); run after any change to `core/`.
- `cargo clippy --all-targets` must stay clean (CI will add `-D warnings`); `cargo fmt` before committing.
- `cargo build` for the dev binary; `cargo build --release` (LTO) for anything measured or shipped.
- Run: `./target/debug/prboard --repo owner/name` (`--review` for the incoming queue); config is env-vars for now (see README), TOML at `$XDG_CONFIG_HOME/prboard/config.toml` comes in Step 3.
- `.claude/research/2026-07-24-step0-build-findings.md` records where reality corrected the research (no `DataTable` in 0.5.1, MSRV 1.88, Metal Toolchain, ~4 min cold build) — read it before trusting the r2 docs on those points.
- Name note: `prboard` is free on crates.io but taken on GitHub — publish as `oliver-kriska/prboard` or rename (`prwall`/`pullboard` were candidates).
