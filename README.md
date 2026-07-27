# prboard

A GitHub PR-review dashboard as a native desktop app (Rust, GPUI +
[gpui-component](https://github.com/longbridge/gpui-component)) for macOS and
Linux. One dense, always-open table of your pull requests with a computed
**Note** per row saying what to do next — so you glance instead of tab-cycling
through github.com.

- **Every signal in one row:** PR number, draft/ready, CI state, unresolved
  threads, merge conflicts, labels, linked issue, reviewers and their verdicts
  — plus the Note ("CI red — fix before ping", "2 unresolved threads", …).
- **Two queues, one click apart:** **My PRs** (needs action → awaiting review
  → drafts) and your incoming **Review queue**.
- **Cheap and calm:** auto-refresh (default 5 min) costs one GraphQL search
  per cycle; the header always shows "synced Xm ago" and your live API budget.
  No animations, ~0 idle CPU/GPU, flat memory.
- **Read-only by design:** it opens PRs in your browser; it never merges,
  assigns, or comments. Auth is your existing [`gh`](https://cli.github.com)
  CLI login — prboard never touches a token itself.

**Status: early (Step-0 walking skeleton).** The framework choice is gated on
an overnight memory measurement (see below). Roadmap and evidence:
`HANDOFF.md` and `.claude/research/`.

## Install

Requires an authenticated [`gh`](https://cli.github.com) (`gh auth login`).

**macOS (Apple silicon) — latest release:**

```sh
curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh
```

On a first install, the script detects the current checkout's repo or asks for
`owner/name`, then writes the minimal config needed for Spotlight launches. For
a non-interactive install, pass it explicitly:

```sh
curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh \
  | sh -s -- --repo owner/name
```

**Any platform — build current `main` from source** (needs git + stable
Rust ≥ 1.88; macOS also needs Xcode's Metal toolchain):

```sh
curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh -s -- --from-source
```

Both install `prboard.app` into `~/Applications` (Spotlight-launchable);
on Linux the source build installs the binary to `~/.local/bin`. Why curl and
not a browser download: until notarization lands, the installer strips download
metadata and ad-hoc signs and verifies the app locally. Homebrew (`brew install
--cask …`) is templated in `packaging/` and blocked on an Apple Developer ID —
see `packaging/RELEASING.md`.

## Running

Requires an authenticated [`gh`](https://cli.github.com) (`gh auth login`) —
prboard inherits its credentials and never touches a token itself.

```sh
cargo build --release
./target/release/prboard --repo owner/name    # or run inside a repo checkout
./target/release/prboard --review             # your incoming review queue
```

### Install as a Mac app from a checkout

```sh
cargo build --release
scripts/bundle-app.sh     # assembles + ad-hoc-signs ~/Applications/prboard.app
```

Spotlight launches carry no shell env and no working directory, so configure
via the config file below (at minimum `repo`).

### Configuration

`~/.config/prboard/config.toml` (or `$XDG_CONFIG_HOME/prboard/config.toml`);
every value optional, env vars override the file, CLI args override both:

```toml
repo = "acme/widgets"              # default repo
repos = ["acme/widgets", "acme/api"]  # >1 entries -> repo picker in the header
refresh_secs = 300                 # hard floor 30
theme = "system"                   # system | light | dark
default_reviewers = ["alice", "bob"]

[issue_link]
pattern = "PROJ-[0-9]+"            # ticket id regex matched in PR titles
url_template = "https://linear.app/acme/issue/{id}"
```

Issue links are optional and tracker-agnostic. `pattern` can match any ticket
format and `url_template` can point to Linear, Jira, or a custom tracker; `{id}`
is replaced with the matched identifier. No project prefix or tracker URL is
built into prboard.

Env vars: `PRBOARD_REPO`, `PRBOARD_REFRESH_SECS`, `PRBOARD_THEME`,
`PRBOARD_ISSUE_PATTERN` + `PRBOARD_ISSUE_URL_TEMPLATE`,
`PRBOARD_DEFAULT_REVIEWERS` (comma-separated).

Use the visible **My PRs / Review queue** control in the titlebar, or press `1`/`2` to switch
directly (`v` still toggles between them). Other keys: `↑`/`↓` select · `⏎`/`o` open PR in
browser · `y` copy PR URL · `r` refresh now ·
`t` cycle theme (system → light → dark) · `q` quit.
Double-click a row to open it; drag column edges to resize; hover the Note,
Title, Labels, or Reviewed-by cell for the full text.

## Building

- Latest **stable** Rust (the dependency graph requires ≥ 1.88).
- macOS: full Xcode with the **Metal Toolchain** component — if the build fails
  with `cannot execute tool 'metal'`, run
  `xcodebuild -downloadComponent MetalToolchain`.
- One GraphQL search per refresh (~3 rate-limit points); the categorization
  logic in `core/` is pinned to the shell-prototype spec by golden tests
  (`scripts/gen-golden.sh`, `core/tests/parity.rs`).

```sh
cargo test -p prboard-core   # categorization/Note parity + unit tests
cargo clippy --all-targets
```

## Development

`make help` lists everything. The quality gate is fast because `prboard-core`
has no GPUI/Metal dependency — only the app binary does.

```sh
make check     # fmt --check + clippy(core) + test(core) — exactly what CI runs
make fix       # auto-format and apply the auto-fixable clippy lints
make build     # debug build of the GPUI app (needs the Metal Toolchain)
make hooks     # install the git pre-commit + commit-msg hooks (one-time)
```

- **Pre-commit hook** runs the same `check` gate, but only when Rust/Cargo files
  are staged (docs-only commits stay instant). Bypass with `git commit
  --no-verify` or `PRBOARD_SKIP_HOOKS=1`.
- **Pre-push hook** runs `make verify` — the full-workspace gate incl. the GPUI
  app binary (`fmt --all --check` + `clippy --all-targets -D warnings` +
  `test --workspace`). This is the one automated check the app binary gets, since
  CI only compiles `prboard-core`; it needs the Metal Toolchain and is slower.
  Bypass with `git push --no-verify` or `PRBOARD_SKIP_HOOKS=1`.
- **CI** (`.github/workflows/ci.yml`) runs `fmt --check` and clippy+test on
  `prboard-core` for every push and PR to `main`. The GPUI binary is not built
  in CI — build it locally with `make build`.
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `chore:`, …) are grouped
  into [`CHANGELOG.md`](CHANGELOG.md) by [git-cliff](https://git-cliff.org); the
  `commit-msg` hook nudges (never blocks) toward them. A **draft "Unreleased"
  GitHub release** is kept in sync with everything on `main` that isn't in the
  latest tag — or run `make unreleased` to see it locally. `make changelog`
  regenerates `CHANGELOG.md` when cutting a release.

## The Step-0 memory gate

GPUI is validated, not assumed: the skeleton must hold **flat RSS < ~150 MB
and ~0 idle GPU over 12–24 h** before anything gets built on top of it.

```sh
scripts/spike-run.sh owner/repo     # app + per-minute RSS sampler + caffeinate
scripts/measure-summary.sh          # next day: verdict against the gate
```

Deeper probes during the run (optional): `leaks <pid>` after a few refresh
cycles (growing count = fail), `sudo powermetrics --samplers gpu_power -i 1000`
(idle GPU should be ~0 between refreshes).

## License

MIT — see `LICENSE`.
