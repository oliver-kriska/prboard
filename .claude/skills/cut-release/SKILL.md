---
name: cut-release
description: Cut and publish a prboard GitHub release (tag + ad-hoc-signed .app tarball) and verify the curl install path end-to-end. Use this whenever asked to release, publish, ship, tag a version, update the GitHub release, or rebuild the release asset — and also when only the installer or release asset needs re-testing. Do not improvise release steps from memory; this pipeline has non-obvious traps (running-app SIGKILL, pre-release API visibility, CDN caching) that this skill encodes.
---

# Cut a prboard release

Target: `github.com/oliver-kriska/prboard`, asset `prboard-v<X>-macos-arm64.tar.gz`
(ad-hoc-signed .app until Apple Developer enrollment unblocks notarization —
see `packaging/RELEASING.md` for the eventual notarized pipeline).

## Traps this pipeline exists to avoid

- **Never install over `~/Applications/prboard.app`.** macOS can SIGKILL a
  running app whose signed binary is replaced in place — and an instance may be
  under an overnight memory measurement. Stage and tar instead
  (`bundle-app.sh --stage-only`); only the *user* relaunches their installed app.
- **`/releases/latest` hides pre-releases** — `install.sh` reads the releases
  *list*; keep it that way while releases are marked `--prerelease`.
- **raw.githubusercontent.com caches ~5 min.** After pushing an `install.sh`
  change, a curl of the raw URL can serve the old script; test the local file
  first, re-verify the raw URL after the cache expires.
- A hook blocks bare `git push --force`; if history was rewritten, use
  `--force-with-lease=<ref>:<expected-remote-sha>` with an explicit SHA.

## Pipeline

1. Preflight: working tree clean, `cargo test --workspace` and
   `cargo clippy --all-targets -- -D warnings` pass (only warning allowed:
   the dependency future-incompat notice), version bumped in `Cargo.toml`
   (workspace root `[package]`) if this is a new version.
2. Build + stage (never installs):
   ```sh
   cargo build --release
   scripts/bundle-app.sh --stage-only        # -> target/bundle/prboard.app
   ```
3. Tar with the version from Cargo.toml:
   ```sh
   V=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
   tar czf "$SCRATCHPAD/prboard-v$V-macos-arm64.tar.gz" -C target/bundle prboard.app
   ```
4. Publish (new tag) or refresh an existing release's asset:
   ```sh
   gh release create "v$V" "$SCRATCHPAD/prboard-v$V-macos-arm64.tar.gz" --prerelease --title "prboard v$V" --notes "..."
   # or: gh release upload "v$V" <tarball> --clobber
   ```
   Release notes: what changed, the curl install command, the "curl not
   browser" quarantine explanation, Apple-silicon-only caveat.
   Note: `gh release create` makes the tag on the remote only — `git fetch --tags`
   if local work needs it.
5. **Verify end-to-end** — the exact README command, into a scratch dir so the
   user's installed app is untouched:
   ```sh
   curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh -s -- --dir "$SCRATCHPAD/install-test"
   codesign --verify --deep "$SCRATCHPAD/install-test/prboard.app"
   ```
   A release is not "done" until this passes against the live release.
6. If the user should pick up the new build: tell them to rerun the curl
   installer (or `scripts/bundle-app.sh`) and relaunch — never do it for them
   while their instance runs.
