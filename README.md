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

Configuration (env vars, for now):

| Variable | Meaning |
|---|---|
| `PRBOARD_REPO` | `owner/name` when not passing `--repo` |
| `PRBOARD_REFRESH_SECS` | refresh interval; default 300, hard floor 30 |
| `PRBOARD_THEME` | `system` (default, follows macOS appearance live) \| `light` \| `dark` |
| `PRBOARD_ISSUE_PATTERN` | regex extracting a ticket id from PR titles, e.g. `ENA-[0-9]+` |
| `PRBOARD_ISSUE_URL_TEMPLATE` | ticket URL with `{id}`, e.g. `https://linear.app/acme/issue/{id}` |
| `PRBOARD_DEFAULT_REVIEWERS` | comma-separated logins suggested in the "no reviewers" note |

Keys: `↑`/`↓` select · `⏎`/`o` open PR in browser · `y` copy PR URL ·
`r` refresh now · `t` cycle theme (system → light → dark) · `q` quit.
Double-click a row to open it; drag column edges to resize; hover the Note or
Title cell for the full text.

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
