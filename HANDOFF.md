# prboard — Project Handoff

**Working name:** prboard (rename-friendly — see below). **Owner:** Oliver Kriška. **License:** MIT (intended), public, open source. **Platforms:** macOS + Linux. **Created from research on:** 2026-07-24.

> **Read this first.** It's the executive summary of a research spike for a new project: an open-source **GitHub PR-review dashboard in Rust**. It distills five detailed research files (linked at the bottom) into a recommendation, a risk list, a v1 scope cut, and a build roadmap. A fresh session should be able to start building from this file alone. The deeper docs are there when you need the evidence behind a claim.
>
> Everything is tagged **FACT** (verified, sourced), **ASSESSMENT** (my judgement — challenge it), or **OPEN QUESTION** (unverified — with a way to answer it). Oliver said explicitly: don't fabricate. Where a number couldn't be verified, it's an OPEN QUESTION, not a guess.

---

## 1. What we're building

A dashboard of GitHub pull requests you keep open all day. A dense table — PR number/link, draft/ready, CI state, requested reviewers, completed reviews, unresolved-thread count, merge-conflict flag, bug label, linked issue, and a computed **"Note"** saying what to do next / what it's blocked on. **Three views:** (1) all open PRs in a repo with filters; (2) *my authored PRs*, triaged action → awaiting-review → drafts; (3) *my review queue*. **Multiple named tabs**, each a repo or group of repos. **Auto-refresh** (default 5 min). **Read-only + open-in-browser only** — no AI, no reviewer-assignment, no merging in v1. Installable via **Homebrew**.

There is a **working shell prototype** (`~/.claude/skills/pr-board/`) that already does the data + categorization + note logic in one GitHub GraphQL call. **prboard v1 = port that prototype to Rust behind a refreshing UI.** The prototype is the behavioral spec; it's transcribed in full in the data-layer doc.

---

## 2. The recommendation (distilled)

> **PRODUCT DECISION (Oliver, 2026-07-24, same day as the research):** prboard is a **windowed native desktop app, not a TUI**. The research below originally recommended ratatui as the default; Oliver ruled the TUI out as a product form ("we want a desktop app with a dashboard, not a terminal"). The recommendation table and roadmap reflect the updated decision; the ratatui analysis in the UI-framework doc stands as evidence and as the record of what was traded away.

| Decision | Recommendation | Confidence |
|---|---|---|
| **UI framework** | **GPUI + longbridge/gpui-component** (windowed desktop app — product decision above). Use gpui-component, NOT Guise, for the component layer (Guise lacks the table). Gated by the 1-day memory spike (§5 Step 0): idle RSS must stay flat and bounded. If the gate fails there is **no pre-committed fallback** — revisit with Oliver (egui, Tauri, or accept-and-mitigate), since the TUI escape hatch is ruled out. | Decided (product); memory gate mandatory |
| **Data transport** | **Shell out to `gh api graphql`** (like the prototype and gh-dash), behind a `GithubTransport` trait so direct-HTTP is a later swap. | High |
| **Query** | Reproduce the prototype's single `search(type:ISSUE, first:60)` GraphQL query (~3 rate-limit points/repo). Never fan out per-PR REST like the previous app did. | High (FACT prototype) |
| **Refresh floor** | Default **5 min**; **hard minimum 30 s** (not the guessed 5 s); adaptive budget warning across tabs. | High (derived from GitHub's published limits) |
| **Auth** | Reuse `gh` (zero-config for anyone who's run `gh auth login`); OAuth Device Flow + PAT later, behind a `TokenSource` trait. | High |
| **Distribution** | macOS: **signed + notarized `.app` — hard-required, not just accepted**: Homebrew quarantines cask downloads, is removing `--no-quarantine`, and enforces Gatekeeper checks on casks from **2026-09-01**; a personal tap does NOT dodge quarantine (r2 packaging doc §3). Pipeline: **Zed's cargo-bundle fork** → codesign `--options runtime` → `notarytool submit --wait` → `stapler staple` in CI → hand-authored **cask** in own tap + direct download. **cargo-dist is formula/CLI-only — it cannot produce `.app`/cask** (axodotdev #850); the macOS pipeline is hand-rolled. Linux: **`.deb` preferred over raw tarball** — GPUI needs Vulkan/fontconfig/xkbcommon at runtime, and a package manager resolves those. | High (r2-verified) |
| **Name** | `prboard` is free on crates.io (the namespace that matters) but crowded on GitHub. Consider `prwall`/`pullboard` for distinctiveness. | FACT (checked) |

**Why GPUI + gpui-component, given the desktop-app decision:** among windowed options it is the only one that fits both hard requirements — the native "Guise-family" aesthetic Oliver wants AND a production-proven **virtualized data table** (gpui-component powers Longbridge Pro, a trading app with exactly our always-open-dense-table workload). Tauri reintroduces webview idle memory (the failure class Oliver is escaping), and egui/iced/Slint are weak precisely on heavy tables while looking less native. PRFlow's memory crash was primarily an **app-level unbounded-cache bug** that is now understood and avoidable (guardrails in §5) — but GPUI carries its own documented leak/overdraw history in Zed, which is why the memory spike below is a **mandatory gate, not a formality**.

**What this decision consciously accepts** (the costs the original ratatui recommendation avoided — accept them with eyes open): Apple Developer ID + notarization ($99/yr + CI signing pipeline) for a distributable `.app` — and per round 2 this is **strictly required**, since Homebrew enforces Gatekeeper on casks from 2026-09-01 and quarantine applies regardless of tap; pre-1.0 `gpui` API churn and long cold builds — though round 2 corrected a round-1 belief: the **published `gpui-component 0.5.1` depends on crates.io `gpui ^0.2.2`, no git pin needed** (pin `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`, stable Rust confirmed by the gpui README); Linux renderer churn (Blade→wgpu Feb 2026) plus runtime deps (Vulkan/fontconfig/xkbcommon). The idle-GPU worry is **milder than round 1 feared**: GPUI renders on-demand via a `WindowInvalidator`, so idle GPU is ~0 *as long as nothing animates continuously* — design rule: **no perpetual refresh-spinner animation**. None of these is disqualifying; all of them are real. The ratatui analysis remains in the UI doc as the record of the alternative.

---

## 3. The five biggest risks / open questions

1. **OPEN QUESTION — GPUI idle memory is unmeasured, and it is now the go/no-go gate.** No public number exists for a minimal gpui-component app's idle RSS over a day. **Answer it in the spike** (build the walking skeleton, leave running 12–24 h, sample RSS + energy). Pass = flat and bounded (<~150 MB proposed); fail = climbs. This re-tests exactly what stalled PRFlow.
2. **RISK — if the memory gate fails, there is no good windowed fallback.** The TUI escape hatch is ruled out by the product decision; the remaining windowed options are egui (weak on heavy tables, tool-UI look), Tauri (webview idle memory — the failure class being escaped), or accept-and-mitigate GPUI. A gate failure means going back to Oliver with numbers, not silently picking one.
3. **RISK — GPUI's Linux support is real but churning, and it's now on our path.** The Linux renderer was rewritten off Blade onto wgpu in Feb 2026 (`zed#46758`) to fix NVIDIA/Wayland freezes. A cross-platform GPUI app inherits that surface (Wayland/X11 feature flags, GPU drivers). Budget Linux testing time; NVIDIA+Wayland is the historical trouble spot.
4. **RISK — shared rate-limit budget, not the "5 s floor."** The token is shared with your own `gh`/git/CI usage. A too-fast refresh across many tabs can rate-limit your *actual work*, not just the dashboard. Mitigation: default 5 min, hard floor 30 s, show the live `rateLimit{}` budget, adaptive warning (math in the data-layer doc).
5. **RISK — Guise (the specific lib Oliver linked) is missing our core widget and is bus-factor-1.** Guise has Tabs/Select/Modal but **no Table, List, or virtual scrolling** — the 80% of prboard — and is an unversioned single-author git dependency. If you go GPUI, use **gpui-component** (mature, has the virtualized table), not Guise. Wanting Guise's *components* specifically is hard to justify.

**Round-2 additions (2026-07-24, later same day — details in the r2 research docs):**

6. **RISK — Homebrew's Gatekeeper enforcement makes notarization non-optional.** Casks are quarantined regardless of tap, `--no-quarantine` is being removed, and Gatekeeper-failing casks are blocked from 2026-09-01. The $99/yr + CI signing pipeline is a hard prerequisite for shipping the `.app` to colleagues at all.
7. **RISK — dependency-variant footgun:** the *published* `gpui-component 0.5.1` uses crates.io `gpui ^0.2.2` (no git), while the *git-main* variant pulls `gpui`/`gpui_platform` via git with different bootstrap code. Pin the crates.io pair (`gpui = "=0.2.2"`, `gpui-component = "=0.5.1"`) and do NOT copy git-main Getting-Started snippets.
8. **RISK — Linux is not a drop-in binary:** GPUI needs a working Vulkan stack + fontconfig/xkbcommon on the user's machine (NVIDIA+Wayland worst). Prefer a `.deb` so the package manager resolves deps; document the requirements.
9. **NOTE — the table is greenfield:** PRFlow hand-rolled its rows and never used the virtualized table. Use gpui-component's **`DataTable`** (delegate `TableDelegate`; only `render_td` required; virtual scroll, sort, selection + keyboard nav, fixed columns, per-row/cell colors) — NOT the simple stateless `Table`. Budget learning time for the imperative delegate API.

**Two things that contradict what Oliver currently believes** (he asked to be told):
- **The "5 s or something" refresh floor is wrong as a floor.** The real GraphQL cost is ~3 points/repo-refresh, and 5 s is *survivable* for one repo (43% of the hourly budget) but imprudent and bad across tabs. Replace with **30 s hard minimum / 60 s recommended / 5 min default** — with the math shown.
- **The `gh`-CLI dependency is the right call, not a limitation to design around.** It's what gh-dash (12.1k stars) does, it gives true zero-config onboarding, and calling the API directly buys little in v1. Keep `gh` for v1; put it behind a trait so direct-HTTP (seeded by `gh auth token`) is a clean later swap. (Also corrected: PRFlow is *on disk* at `~/Projects/pr_flow`, and GPUI reportedly builds on *stable* Rust now, not the nightly PRFlow needed.)

---

## 4. Proposed v1 scope cut

**In v1:**
- The three views (authored default, review queue, all-open) as modes within a tab.
- One GraphQL query, `gh api graphql` transport behind a trait.
- The full categorization + Note logic from the prototype (action/await/draft; todo/done).
- Multiple named tabs (config-driven), each one or more repos.
- Repo selection: pick a GitHub repo (via `gh repo list` + free-text) **or** point at a local folder (derive owner/repo from git remote). Zero-config first run infers the repo from `cwd`.
- Auto-refresh (default 5 min, floor 30 s), refresh indicator + "last updated" + live rate-limit budget, back-off on rate-limit.
- Filters (author/CI/review-state/draft/label), client-side, persisted per tab.
- Full mouse support + keyboard shortcuts (`?` help); open PR / linked issue in browser; copy PR URL.
- Bounded caches from line one (explicit `MAX_*` + FIFO — the #1 lesson from PRFlow).
- Config in TOML at `$XDG_CONFIG_HOME/prboard/config.toml`.
- Distribution: macOS signed + notarized `.app` via own brew tap + direct download; Linux tarball / shell installer / brew formula; CI with clippy `-D warnings` (clean first).

**Explicitly NOT in v1:** AI/summaries; reviewer assignment / dismiss / merge / submit-review (any write action beyond open-in-browser); custom keybindings; shell-out custom actions (gh-dash-style); self-updating binary; OAuth Device Flow; homebrew-core; Windows.

---

## 5. Build roadmap

**Step 0 — Spike (≈1 day, the memory gate — do this before building on top).**
Build ONE walking skeleton in **GPUI + gpui-component** (the framework is decided; the spike validates it): stable Rust, crates.io pins `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`, the **`DataTable`** widget (not the stateless `Table`), no continuously-animating spinner (it would defeat the idle-GPU half of the gate) — one tab, one repo, the real `gh api graphql` call, render the 60-row virtualized table + Note, auto-refresh 5 min, open-in-browser. The r2 build doc (`2026-07-24-gpui-desktop-build.md`) has the skeleton sketch — start from it. Then **measure idle RSS over 12–24 h** (sample `ps -o rss=` per minute; on mac also `leaks`/Instruments and `powermetrics` for idle GPU/energy — GPUI has a history of idle frame-presentation burn) + build time + binary size. **Gate rule:** proceed if idle RSS is flat and < ~150 MB over a day and idle GPU is ~0 when nothing changes; if it climbs, STOP and bring the numbers back to Oliver — the fallbacks (egui, Tauri, accept-and-mitigate) are all compromised and need a conscious re-decision. Mine `~/Projects/pr_flow` first — it already used gpui-component, so its table/state code is a reference for both patterns and pitfalls. (Original two-skeleton protocol: UI-framework doc §7 — the ratatui half is obsolete per the product decision.)

> **STEP-0 STATUS (2026-07-24, evening).** The skeleton is built and the overnight gate run is in progress (release build + RSS sampler via `scripts/spike-run.sh`; verdict via `scripts/measure-summary.sh`). Build-time corrections to the research are in `.claude/research/2026-07-24-step0-build-findings.md` — notably: the published gpui-component 0.5.1 has no `DataTable` (its `Table`+`TableState` IS the virtualized one), MSRV is effectively 1.88, macOS needs the Xcode Metal Toolchain, and cold builds are ~4 min, not 15–30.
>
> **Step 0.5 — first-feedback round (same evening, done):** Note column wins width over Title and both carry full-text hover tooltips (truncation was hiding compound notes — composition itself is correct and golden-tested); drag-resizable columns confirmed on (0.5.1 default); Requested "none" dimmed to "—" (redundant with the red note); **dark/light/system theme modes** (`PRBOARD_THEME`, `t` key cycles, System follows macOS appearance changes live via `observe_window_appearance`).
>
> **Queued for later steps, learned in the build:** an asset bundle (icons) only when tabs/filters need it (Step 3); a details row on select as a richer note-survival strategy (Step 2 candidate); "copy board as markdown" via delegate cell-text (post-v1); the SKILL.md example JSON is internally inconsistent with the prototype script — the script is the spec (parity tests pin to it).
>
> **Step 0.6 — second-feedback round (2026-07-24, late evening, done):** (a) **config file** `~/.config/prboard/config.toml` (repo/repos/refresh/theme/reviewers/issue_link; env > file precedence) so Spotlight launches work, plus `gh` resolved from Homebrew paths when PATH is the LaunchServices minimum; (b) **repo picker** — header Select fed from config `repos`, switching clears + refetches; (c) **Bug column → Labels** — neutral chips for all labels (`labels` now on `BoardRow`), 🐛 marks the bug label, `+n` overflow, tooltip; (d) **Reviewed-by is glyph-first** (the state glyph survives truncation — the DISMISSED case on #10710 was the proof) with a spelled-out tooltip; (e) stray last-empty-column filler removed; (f) **visual overhaul** per the Guise/Mantine-derived spec in `.claude/research/2026-07-24-visual-design.md` — `src/design.rs` token overrides (both modes, WCAG-AA-checked), status **dots replace emoji** in CI/Note cells (core note strings unchanged — UI strips the glyph language), one-line header, keycap footer, full-bleed `Size::Small` table, draft rows dimmed not tinted; (g) **packaging** — `scripts/bundle-app.sh` hand-rolls an ad-hoc-signed `~/Applications/prboard.app` (Spotlight-launchable, generated icon), Homebrew cask template + notarization runbook in `packaging/` (distribution blocked on Apple Developer enrollment; Homebrew enforces Gatekeeper from 2026-09-01). A tiny embedded `AssetSource` (4 Lucide SVGs) now exists for the Select chevron — the "asset bundle when needed" moment arrived early.
> Also landed same round: **custom transparent titlebar** (user-requested) — gpui-component's `TitleBar` wraps the header row, traffic lights overlay the app's own chrome (drag + double-click-zoom included).
>
> The Step-0 memory gate still decides Step 1+. Verification gotcha for future sessions: GPUI skips repainting occluded windows — screenshot checks MUST bring the window frontmost first or they capture a stale first frame (looks like a hung fetch; it isn't).
>
> **Discovered gaps to fix (2026-07-24 retro, not yet implemented):**
> 1. **No timeout on the `gh` subprocess** — a network-hung `gh` leaves `syncing=true` forever; the dedup in `refresh()` then blocks all future refreshes AND the `r` key. Silent permanent freeze of the data layer; only restart recovers. Fix: spawn+poll with a ~60 s kill → visible error. (Same failure class as the post-mortem's unclamped waits, one layer down.)
> 2. **"synced Xm ago" staleness** — the label renders only on notify, so between refreshes it can be wrong by up to a full interval ("just now" for 5 min). Fix: ~60 s `cx.notify()` tick — but this changes idle behavior, so land it only AFTER the gate verdict and re-check idle GPU/CPU.
> 3. **In-app state doesn't persist** (Spotlight-first ergonomics): no `mode` key in config; `t`/`v` choices and window bounds reset on relaunch; config edits need a relaunch (consider re-reading config on refresh).

**Step 1 — Walking skeleton (framework chosen).**
Data layer first, framework-independent: `GithubTransport` trait + `GhCliTransport`, the GraphQL query, serde structs, and the `Pr → BoardRow` derive (category, ci, reviewState, unresolved, note). Unit-test the categorization against the prototype's output. Then a static one-tab render of a `Vec<BoardRow>`.

**Step 2 — Interactivity.**
Auto-refresh loop (interval + floor + back-off + `rateLimit{}` budget), the three view modes, keyboard nav, open-in-browser, refresh/last-updated/stale/error/empty states, bounded caches.

**Step 3 — Tabs, filters, config.**
Multiple tabs, per-tab repos + persisted filters, TOML config + zero-config first run (cwd git remote), repo picker (GitHub select + local folder).

**Step 4 — v1 release.**
`.app` bundling with **Zed's cargo-bundle fork** (Info.plist + .icns), Apple Developer ID enrollment, codesign `--options runtime` + `notarytool submit --wait` + `stapler staple` on a macos-14 CI job, hand-authored **cask** in own tap for macOS, **`.deb` + tarball** for Linux, tag `v0.1.0`, install on a colleague's machine, README. Ship. (Full pipeline incl. required CI secrets: r2 packaging doc.)

**Step 5 (post-v1, optional):** direct-HTTP transport, OAuth Device Flow, action layer (assign reviewers, re-run checks), custom keybindings, in-app update check, AUR/deb.

**Guardrails to encode from day one** (all from the prior-art post-mortem):
- Bound every cache (`MAX_*` + FIFO); no unbounded `HashMap`.
- One GraphQL call, never per-PR REST.
- Clamp every wait `max(60).min(900)`; never freeze on a far-future reset.
- No runtime you don't use (maybe no full tokio — a thread+channel may suffice).
- Ship "last synced Xm ago" + visible rate-limit state.
- Keep transport, token-source, and issue-link behind traits/config (no hard-coded `ENA-`/Linear).

---

## 6. The research files (evidence behind all of the above)

- **`.claude/research/2026-07-24-prior-art.md`** — PRFlow post-mortem (the memory root cause: app-level unbounded caches + GPUI's own leak history), Agentrix lessons, gh-dash + the Rust-TUI landscape. **Start here after this handoff** — it's why the recommendation is what it is.
- **`.claude/research/2026-07-24-github-data-layer.md`** — the behavioral spec: verbatim GraphQL query + JSON shape + categorization + note rules, the transport trait, the real rate-limit math, the auth story. **This is what v1 must reproduce.**
- **`.claude/research/2026-07-24-ui-framework-evaluation.md`** — the full framework comparison (ratatui / GPUI+gpui-component / Guise / Tauri / egui / iced / Slint), the decision matrix, and the spike protocol.
- **`.claude/research/2026-07-24-product-ux-spec.md`** — layout, the three views, filters, tabs, states, keyboard model, config, repo selection, visual direction.
- **`.claude/research/2026-07-24-distribution.md`** — Homebrew via cargo-dist, macOS notarization (and why the CLI path avoids it), CI matrix, update story, name availability. *Partly superseded by the r2 packaging doc below.*

**Round 2 (same day, after the desktop-app decision):**

- **`.claude/research/2026-07-24-gpui-desktop-build.md`** — building with GPUI + gpui-component in practice: dependency pins, `DataTable` API, standalone-app architecture (async executor, gh subprocess, refresh timers), PRFlow code lessons, sharpened memory/energy measurement protocol, and the **skeleton sketch the spike starts from**.
- **`.claude/research/2026-07-24-desktop-packaging.md`** — the real macOS `.app` pipeline (Zed's cargo-bundle fork → codesign → notarytool → staple → cask), why notarization is hard-required (Homebrew 2026-09-01 enforcement), Linux `.deb`/tarball, CI secrets and matrix. Supersedes the CLI-path parts of the round-1 distribution doc.
- **`.claude/research/2026-07-24-desktop-ux-capabilities.md`** — feasibility map for tray/menubar, Dock badge, native notifications, launch-at-login, window-state persistence, app menu — with v1/v2 markers.

---

## 7. Key facts to not re-derive

- Prototype: `~/.claude/skills/pr-board/scripts/pr-board.sh` + `SKILL.md` (the spec).
- Previous app: `~/Projects/pr_flow` (Rust+GPUI, on disk) — read `SOLUTIONS_EXTRACTED.md` for the memory fix.
- Prior TUI Oliver shipped: `~/Projects/agentrix` (Rust+ratatui) and `vibenalytics` (Rust+ratatui, GitHub-Releases distribution).
- GraphQL: 5,000 points/hr; our query ≈ 3 points/repo-refresh; 500k-node cap; secondary 2,000 pts/min.
- gpui-component: crates.io, Apache-2.0, 12.2k stars. **r2 corrections:** published 0.5.1 depends on **crates.io `gpui ^0.2.2` (no git pin)** — pin `gpui = "=0.2.2"` + `gpui-component = "=0.5.1"`, stable Rust (gpui README). Use **`DataTable`** (delegate API: virtual scroll, sort, selection, fixed columns, per-row/cell colors), not the stateless `Table`.
- Guise: git-only, MIT, 120+ components but **no table/list/virtual-scroll**.
- crates.io `prboard` is free; GitHub `prboard` is taken by others (use `oliver-kriska/prboard` or rename).
- Distribution: cargo-dist v0.32.0 (2026-05), alive but **formula/CLI-only — no `.app`/cask** (axodotdev #850); `.app` = $99/yr Developer ID + notarization, **hard-required** (Homebrew quarantines casks regardless of tap, `--no-quarantine` going away, Gatekeeper enforcement on casks from 2026-09-01). **DECIDED 2026-07-24: the `.app` path** — pipeline: Zed cargo-bundle fork → codesign → notarytool → staple → cask in own tap; Linux `.deb` (Vulkan/fontconfig/xkbcommon runtime deps).
- DECISION 2026-07-24: windowed desktop app (GPUI + gpui-component), TUI ruled out as product form. Second research round (desktop build/packaging specifics) in `.claude/research/2026-07-24-gpui-desktop-*.md` if present.
