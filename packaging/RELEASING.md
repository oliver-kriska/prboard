# Releasing prboard (macOS)

Step-by-step runbook for shipping a signed, notarized `prboard.app` via a
Homebrew cask in a personal tap. Synthesized from
`.claude/research/2026-07-24-desktop-packaging.md` — see that doc for sources
and reasoning.

**Status legend:**
- ✅ works today
- 🔒 **BLOCKED on Apple Developer Program enrollment** ($99/yr,
  developer.apple.com). Until enrolled there is no Developer ID certificate
  and no notarization access, so nothing past step 2 can ship to other people.

**Why the blocked steps are mandatory, not optional:** Homebrew applies the
`com.apple.quarantine` attribute to cask downloads, `--no-quarantine` is being
removed, and **from 2026-09-01 Homebrew stops supporting casks that fail
Gatekeeper checks**. A personal tap does not dodge this — quarantine happens on
the user's machine regardless of tap. An un-notarized `.app` is only usable on
the machine that built it.

---

## 0. One-time setup (🔒 blocked on enrollment)

1. Enroll in the Apple Developer Program ($99/yr). Note your **Team ID**.
2. Create a **Developer ID Application** certificate; export as `.p12`.
3. Create an **app-specific password** for notarization (or, better for CI, an
   App Store Connect **API key** — `notarytool` accepts
   `--key/--key-id/--issuer`).
4. Create the tap repo **`oliver-kriska/homebrew-tap`** with a `Casks/`
   directory. Copy `packaging/homebrew/prboard.rb` there as the live cask.
5. For CI later: store the 7 secrets from the research doc §3.3
   (`MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PWD`, `MACOS_CERTIFICATE_NAME`,
   `MACOS_NOTARIZATION_APPLE_ID`, `MACOS_NOTARIZATION_TEAM_ID`,
   `MACOS_NOTARIZATION_PWD`, `MACOS_CI_KEYCHAIN_PWD`).

## 1. Build ✅

```bash
cargo build --release
```

CI later: build **universal** (arm64 + x86_64) on a `macos-14` runner. Cold
GPUI builds are 15–30+ min — cache cargo registry/git/target keyed on
`Cargo.lock`.

## 2. Bundle ✅ (local script now; Zed cargo-bundle fork in CI later)

Today, locally:

```bash
scripts/bundle-app.sh    # hand-rolled .app → ad-hoc sign → ~/Applications
```

This is enough for *your own machine* (Spotlight launch). For releases, CI
should instead use **Zed's cargo-bundle fork** (built for GPUI apps, produces
universal binaries):

```bash
cargo install cargo-bundle --git https://github.com/zed-industries/cargo-bundle.git --branch zed-deploy
cargo bundle --release
```

Requires `[package.metadata.bundle]` in `Cargo.toml` (name, `identifier =
"dev.oliverkriska.prboard"`, icon, `category = "public.app-category.developer-tools"`)
— not added yet.

## 3. Codesign with Developer ID 🔒

Ad-hoc (`-s -`) is NOT sufficient for distribution. Sign with the Developer ID
cert and the **hardened runtime** (mandatory for notarization):

```bash
codesign --deep --force --timestamp --options runtime \
  --entitlements packaging/prboard.entitlements \
  -s "Developer ID Application: Oliver Kriška (TEAMID)" \
  target/release/bundle/osx/prboard.app -v
```

A minimal entitlements plist is fine — prboard talks to the network only via
the `gh` subprocess and needs no special entitlements.

## 4. Notarize 🔒

```bash
xcrun notarytool store-credentials "notarytool-profile" \
  --apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APP_SPECIFIC_PWD"
ditto -c -k --keepParent prboard.app prboard-notarize.zip
xcrun notarytool submit prboard-notarize.zip \
  --keychain-profile "notarytool-profile" --wait
```

`--wait` blocks until Apple returns `Accepted` (usually minutes). On
`Invalid`, run `xcrun notarytool log <submission-id>` for the reasons.

## 5. Staple 🔒

```bash
xcrun stapler staple prboard.app
```

Staples the notarization ticket into the bundle so Gatekeeper passes offline.

## 6. Zip the distributable 🔒

```bash
ditto -c -k --keepParent prboard.app prboard-<version>-universal.zip
shasum -a 256 prboard-<version>-universal.zip   # needed for the cask
```

Zip the **stapled** app (step 5 first, then zip). A `.dmg` is optional polish
for later; casks handle both.

## 7. GitHub release 🔒 (mechanics work today, artifact is blocked)

```bash
git tag v<version> && git push origin v<version>
gh release create v<version> prboard-<version>-universal.zip \
  --title "prboard v<version>" --generate-notes
```

## 8. Update the cask in the tap 🔒

In `oliver-kriska/homebrew-tap`, edit `Casks/prboard.rb`: bump `version`,
paste the new `sha256` from step 6, commit, push. (CI later: a ~15-line
release.yml step computes the sha and commits to the tap; `livecheck` keeps
brew aware of new versions.)

## 9. Install / upgrade ✅ (once a release exists)

```bash
brew install --cask oliver-kriska/tap/prboard
brew upgrade --cask prboard
```

Acceptance test: on a **colleague's** Mac, the app must open with **no
Gatekeeper warning**. That is the whole point of steps 3–5.

---

## Caveats

- **gh CLI is a hard runtime dependency.** prboard shells out to `gh` for all
  GitHub access; the cask declares `depends_on formula: "gh"`, but users still
  need `gh auth login` once. Document this in the release notes / README.
- **Launch context:** Spotlight launches provide no shell env and `cwd=/`.
  The app resolves its repo from CLI args → `PRBOARD_REPO` → 
  `~/.config/prboard/config.toml` → `gh repo view` in cwd. A config file is
  effectively required for Spotlight launches.
- **CI pipeline:** hand-rolled `release.yml`, not cargo-dist — cargo-dist
  cannot produce a `.app` or a cask (research doc §2). Espanso's GitHub
  Actions recipe is the template for the keychain/signing steps (§3.3).
- **Linux** is a separate path (tarball + .desktop; Vulkan runtime required) —
  research doc §5. Not covered by this runbook.
