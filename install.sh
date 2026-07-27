#!/bin/sh
# prboard installer — designed to be curl-piped:
#
#   curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh -s -- --from-source
#
# Default: download the latest GitHub release .app and install it (macOS).
# The installer strips download/build metadata, then ad-hoc signs and verifies
# the app on the destination Mac. Proper Developer ID signing and notarization
# remain on the roadmap — see packaging/RELEASING.md.
#
#   --from-source   clone main and build (needs git + stable Rust >= 1.88;
#                   macOS additionally needs Xcode's Metal toolchain)
#   --dir DIR       install destination (default: ~/Applications on macOS,
#                   ~/.local/bin on Linux; env PRBOARD_INSTALL_DIR also works)
#   --repo OWNER/NAME
#                   save the default repo so Finder/Spotlight launches work
set -eu

REPO="oliver-kriska/prboard"
FROM_SOURCE=0
DIR="${PRBOARD_INSTALL_DIR:-}"
DEFAULT_REPO="${PRBOARD_REPO:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    --from-source) FROM_SOURCE=1 ;;
    --dir) DIR="${2:?--dir needs a path}"; shift ;;
    --repo) DEFAULT_REPO="${2:?--repo needs owner/name}"; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

OS="$(uname -s)"
ARCH="$(uname -m)"
say() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

config_path() {
  case "${XDG_CONFIG_HOME:-}" in
    /*) printf '%s/prboard/config.toml\n' "$XDG_CONFIG_HOME" ;;
    *) printf '%s/.config/prboard/config.toml\n' "$HOME" ;;
  esac
}

configure_repo() {
  CONFIG=$(config_path)
  # Never rewrite a user's existing config from a shell installer. The app's
  # toml_edit persistence owns updates once a config exists.
  [ ! -e "$CONFIG" ] || return 0

  CANDIDATE="$DEFAULT_REPO"
  if [ -z "$CANDIDATE" ] && command -v gh >/dev/null 2>&1; then
    # This makes installing while inside a checkout zero-config.
    CANDIDATE=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)
  fi
  if [ -z "$CANDIDATE" ] && [ -t 1 ] && [ -r /dev/tty ]; then
    printf 'Default GitHub repo for prboard (owner/name, or Enter to skip): ' >/dev/tty
    IFS= read -r CANDIDATE </dev/tty || CANDIDATE=""
  fi
  [ -n "$CANDIDATE" ] || return 0

  command -v gh >/dev/null 2>&1 || {
    printf 'warning: cannot validate repo without the GitHub CLI; config not written\n' >&2
    return 0
  }
  CANONICAL=$(gh repo view "$CANDIDATE" --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)
  [ -n "$CANONICAL" ] || {
    printf 'warning: cannot access GitHub repo %s; config not written\n' "$CANDIDATE" >&2
    return 0
  }

  mkdir -p "${CONFIG%/*}"
  (umask 077 && printf 'repo = "%s"\n' "$CANONICAL" > "$CONFIG")
  say "Configured $CANONICAL in $CONFIG"
}

post_install_notes() {
  CONFIG=$(config_path)
  if ! command -v gh >/dev/null 2>&1; then
    cat <<'EOF'

prboard needs the GitHub CLI. Install it, then authenticate:

    brew install gh
    gh auth login

EOF
  elif ! gh auth status >/dev/null 2>&1; then
    cat <<'EOF'

Authenticate the GitHub CLI before launching prboard:

    gh auth login

EOF
  fi
  if [ ! -e "$CONFIG" ]; then
    cat <<EOF
Spotlight launches need a default repo. Create $CONFIG:

    repo = "owner/name"

EOF
  fi
}

install_release_macos() {
  [ "$ARCH" = "arm64" ] || die "prebuilt releases are Apple-silicon only for now; use --from-source"
  command -v codesign >/dev/null 2>&1 || die "macOS codesign tool not found"
  DIR="${DIR:-$HOME/Applications}"
  say "Finding the latest release of $REPO"
  # The list endpoint (newest first) rather than /releases/latest, which
  # skips pre-releases — and early prboard releases are all pre-releases.
  RELEASES=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=5") \
    || die "could not query GitHub releases"
  URL=$(printf '%s\n' "$RELEASES" \
    | grep -o '"browser_download_url": *"[^"]*macos-arm64[^"]*\.tar\.gz"' \
    | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')
  [ -n "$URL" ] || die "no macos-arm64 release asset found; use --from-source"
  TMP=$(mktemp -d "${TMPDIR:-/tmp}/prboard-install.XXXXXX")
  trap 'rm -rf "$TMP"' EXIT
  say "Downloading ${URL##*/}"
  curl -fL --progress-bar "$URL" -o "$TMP/prboard.tar.gz" || die "release download failed"
  tar xzf "$TMP/prboard.tar.gz" -C "$TMP" || die "could not extract release archive"
  rm -f "$TMP/prboard.tar.gz"
  [ -d "$TMP/prboard.app" ] || die "unexpected archive layout (no prboard.app)"
  # Release tarballs can carry build-machine metadata in AppleDouble entries.
  # Strip it, then sign on the destination Mac just like --from-source does.
  # This is the temporary pre-notarization path; a Developer ID release will
  # replace it once the signing pipeline is available.
  find "$TMP/prboard.app" -name '._*' -exec rm -f {} + 2>/dev/null || true
  xattr -cr "$TMP/prboard.app" 2>/dev/null || true
  say "Signing and verifying the app for this Mac"
  if ! SIGN_ERROR=$(codesign --force --deep -s - "$TMP/prboard.app" 2>&1); then
    [ -z "$SIGN_ERROR" ] || printf '%s\n' "$SIGN_ERROR" >&2
    die "could not sign the downloaded app"
  fi
  if ! VERIFY_ERROR=$(codesign --verify --deep --strict "$TMP/prboard.app" 2>&1); then
    [ -z "$VERIFY_ERROR" ] || printf '%s\n' "$VERIFY_ERROR" >&2
    die "downloaded app failed code-signature verification"
  fi
  say "Installing to $DIR/prboard.app"
  mkdir -p "$DIR"
  rm -rf "$DIR/prboard.app"
  mv "$TMP/prboard.app" "$DIR/prboard.app"
  LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
  [ -x "$LSREG" ] && "$LSREG" -f "$DIR/prboard.app" >/dev/null 2>&1 || true
  configure_repo
  say "Done — launch 'prboard' from Spotlight, or: open '$DIR/prboard.app'"
  post_install_notes
}

install_from_source() {
  command -v git >/dev/null 2>&1 || die "git is required for --from-source"
  command -v cargo >/dev/null 2>&1 || die "Rust (cargo) is required for --from-source — https://rustup.rs"
  TMP=$(mktemp -d "${TMPDIR:-/tmp}/prboard-src.XXXXXX")
  trap 'rm -rf "$TMP"' EXIT
  say "Cloning $REPO (main)"
  git clone --depth 1 "https://github.com/$REPO" "$TMP/prboard"
  say "Building release binary (a few minutes; LTO)"
  (cd "$TMP/prboard" && cargo build --release)
  if [ "$OS" = "Darwin" ]; then
    DIR="${DIR:-$HOME/Applications}"
    PRBOARD_INSTALL_DIR="$DIR" "$TMP/prboard/scripts/bundle-app.sh"
  else
    DIR="${DIR:-$HOME/.local/bin}"
    say "Installing binary to $DIR/prboard"
    mkdir -p "$DIR"
    install -m 755 "$TMP/prboard/target/release/prboard" "$DIR/prboard"
    say "Done — make sure $DIR is on your PATH"
  fi
  configure_repo
  post_install_notes
}

if [ "$FROM_SOURCE" = 1 ]; then
  install_from_source
elif [ "$OS" = "Darwin" ]; then
  install_release_macos
else
  die "no prebuilt Linux packages yet — rerun with --from-source"
fi
