# prboard

A GitHub PR review dashboard as a native desktop app (Rust, GPUI +
[gpui-component](https://github.com/longbridge/gpui-component)). One dense,
always-open table of your pull requests with a computed **Note** per row saying
what to do next — read-only, refreshing itself, opening PRs in the browser.

**Status: Step-0 walking skeleton.** The framework choice is gated on an
overnight memory measurement (see below). Roadmap and evidence: `HANDOFF.md`
and `.claude/research/`.

## Running

Requires an authenticated [`gh`](https://cli.github.com) (`gh auth login`) —
prboard inherits its credentials and never touches a token itself.

```sh
cargo build --release
./target/release/prboard --repo owner/name    # or run inside a repo checkout
./target/release/prboard --review             # your incoming review queue
```

### Install as a Mac app (Spotlight)

```sh
cargo build --release
scripts/bundle-app.sh     # assembles + ad-hoc-signs ~/Applications/prboard.app
```

Then launch it from Spotlight like any app. Spotlight launches carry no shell
env and no working directory, so configure via the config file below (at
minimum `repo`). Distribution via Homebrew (`brew install --cask
oliver-kriska/tap/prboard`) is templated in `packaging/` but blocked on an
Apple Developer ID for notarization — see `packaging/RELEASING.md`.

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
pattern = "ENA-[0-9]+"             # ticket id regex matched in PR titles
url_template = "https://linear.app/acme/issue/{id}"
```

Env vars: `PRBOARD_REPO`, `PRBOARD_REFRESH_SECS`, `PRBOARD_THEME`,
`PRBOARD_ISSUE_PATTERN` + `PRBOARD_ISSUE_URL_TEMPLATE`,
`PRBOARD_DEFAULT_REVIEWERS` (comma-separated).

Keys: `↑`/`↓` select · `⏎`/`o` open PR in browser · `y` copy PR URL ·
`r` refresh now · `t` cycle theme (system → light → dark) · `q` quit.
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
