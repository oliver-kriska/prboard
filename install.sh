#!/bin/sh
# prboard installer — designed to be curl-piped:
#
#   curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/oliver-kriska/prboard/main/install.sh | sh -s -- --from-source
#
# Default: download the latest GitHub release .app and install it (macOS).
# Piping through curl matters: the tarball never gets the com.apple.quarantine
# xattr a browser download would add, so the ad-hoc-signed app runs without a
# Gatekeeper fight (proper notarization is on the roadmap — packaging/RELEASING.md).
#
#   --from-source   clone main and build (needs git + stable Rust >= 1.88;
#                   macOS additionally needs Xcode's Metal toolchain)
#   --dir DIR       install destination (default: ~/Applications on macOS,
#                   ~/.local/bin on Linux; env PRBOARD_INSTALL_DIR also works)
set -eu

REPO="oliver-kriska/prboard"
FROM_SOURCE=0
DIR="${PRBOARD_INSTALL_DIR:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    --from-source) FROM_SOURCE=1 ;;
    --dir) DIR="${2:?--dir needs a path}"; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

OS="$(uname -s)"
ARCH="$(uname -m)"
say() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

post_install_notes() {
  cat <<'EOF'

prboard needs an authenticated GitHub CLI:  gh auth login
Spotlight launches carry no shell env, so set your repo in
~/.config/prboard/config.toml:

    repo = "owner/name"

EOF
}

install_release_macos() {
  [ "$ARCH" = "arm64" ] || die "prebuilt releases are Apple-silicon only for now; use --from-source"
  DIR="${DIR:-$HOME/Applications}"
  say "Finding the latest release of $REPO"
  URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -o '"browser_download_url": *"[^"]*macos-arm64[^"]*\.tar\.gz"' \
    | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')
  [ -n "$URL" ] || die "no macos-arm64 asset found in the latest release; use --from-source"
  TMP=$(mktemp -d "${TMPDIR:-/tmp}/prboard-install.XXXXXX")
  trap 'rm -rf "$TMP"' EXIT
  say "Downloading ${URL##*/}"
  curl -fL --progress-bar "$URL" | tar xz -C "$TMP"
  [ -d "$TMP/prboard.app" ] || die "unexpected archive layout (no prboard.app)"
  # curl leaves no quarantine xattr, but clear any just in case.
  xattr -dr com.apple.quarantine "$TMP/prboard.app" 2>/dev/null || true
  say "Installing to $DIR/prboard.app"
  mkdir -p "$DIR"
  rm -rf "$DIR/prboard.app"
  mv "$TMP/prboard.app" "$DIR/prboard.app"
  LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
  [ -x "$LSREG" ] && "$LSREG" -f "$DIR/prboard.app" >/dev/null 2>&1 || true
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
  post_install_notes
}

if [ "$FROM_SOURCE" = 1 ]; then
  install_from_source
elif [ "$OS" = "Darwin" ]; then
  install_release_macos
else
  die "no prebuilt Linux packages yet — rerun with --from-source"
fi
