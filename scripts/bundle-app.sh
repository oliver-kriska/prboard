#!/usr/bin/env bash
# bundle-app.sh — assemble a local, ad-hoc-signed prboard.app and install it
# to ~/Applications so Spotlight can launch it.
#
# This is the LOCAL/dev bundle path (hand-rolled .app skeleton). The real
# release pipeline (Zed's cargo-bundle fork + Developer ID + notarization)
# is documented in packaging/RELEASING.md.
#
# Usage: scripts/bundle-app.sh
# Requires: target/release/prboard (cargo build --release), stock macOS tools.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="$REPO_ROOT/target/release/prboard"
STAGE="$REPO_ROOT/target/bundle"
APP="$STAGE/prboard.app"
INSTALL_DIR="$HOME/Applications"
BUNDLE_ID="dev.oliverkriska.prboard"

step() { printf '\n==> %s\n' "$*"; }

[[ -x "$BINARY" ]] || {
  echo "error: $BINARY not found or not executable — run 'cargo build --release' first" >&2
  exit 1
}

# --- version ---------------------------------------------------------------
# Parse the [package] version straight from Cargo.toml (read-only; cargo
# pkgid is only a fallback because it may refresh Cargo.lock).
VERSION="$(awk -F'"' '
  /^\[package\]/ { in_pkg = 1; next }
  /^\[/          { in_pkg = 0 }
  in_pkg && /^version[[:space:]]*=/ { print $2; exit }
' "$REPO_ROOT/Cargo.toml")"
if [[ -z "$VERSION" ]]; then
  VERSION="$(cd "$REPO_ROOT" && cargo pkgid 2>/dev/null | sed -n 's/.*[#@]\([0-9][0-9A-Za-z.+-]*\)$/\1/p')"
fi
[[ -n "$VERSION" ]] || { echo "error: could not determine package version" >&2; exit 1; }
step "prboard v$VERSION"

# --- stage skeleton --------------------------------------------------------
step "Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BINARY" "$APP/Contents/MacOS/prboard"

# --- icon (best-effort; skipped gracefully on any failure) -----------------
# Draws a simple pull-request glyph via JXA/Cocoa, then sips + iconutil.
make_icon() {
  local workdir master iconset
  workdir="$(mktemp -d "${TMPDIR:-/tmp}/prboard-icon.XXXXXX")"
  master="$workdir/master.png"
  iconset="$workdir/prboard.iconset"

  cat > "$workdir/icon.jxa" <<'JXA'
ObjC.import('Cocoa')
const out = $.NSProcessInfo.processInfo.environment.objectForKey('ICON_OUT').js
const S = 1024
const img = $.NSImage.alloc.initWithSize($.NSMakeSize(S, S))
img.lockFocus
// dark rounded-square background
$.NSColor.colorWithSRGBRedGreenBlueAlpha(0.11, 0.14, 0.21, 1).setFill
$.NSBezierPath.bezierPathWithRoundedRectXRadiusYRadius(
  $.NSMakeRect(64, 64, 896, 896), 200, 200).fill
// pull-request glyph, stroked in accent blue
$.NSColor.colorWithSRGBRedGreenBlueAlpha(0.42, 0.63, 0.98, 1).setStroke
function strokePath(p) { p.setLineWidth(58); p.setLineCapStyle(1); p.setLineJoinStyle(1); p.stroke }
// circles: top-left, bottom-left, bottom-right
;[[392, 700], [392, 324], [632, 324]].forEach(function (c) {
  const o = $.NSBezierPath.bezierPathWithOvalInRect(
    $.NSMakeRect(c[0] - 74, c[1] - 74, 148, 148))
  o.setLineWidth(58); o.stroke
})
// left rail: top circle down to bottom-left circle
let p = $.NSBezierPath.bezierPath
p.moveToPoint($.NSMakePoint(392, 398)); p.lineToPoint($.NSMakePoint(392, 626))
strokePath(p)
// branch: out of top-left circle, elbow down into bottom-right circle
p = $.NSBezierPath.bezierPath
p.moveToPoint($.NSMakePoint(500, 700))
p.appendBezierPathWithArcFromPointToPointRadius(
  $.NSMakePoint(632, 700), $.NSMakePoint(632, 398), 90)
p.lineToPoint($.NSMakePoint(632, 398))
strokePath(p)
img.unlockFocus
const rep = $.NSBitmapImageRep.imageRepWithData(img.TIFFRepresentation)
const png = rep.representationUsingTypeProperties(4 /* PNG */, $.NSDictionary.dictionary)
if (!png.writeToFileAtomically(out, true)) throw new Error('png write failed')
JXA

  ICON_OUT="$master" osascript -l JavaScript "$workdir/icon.jxa" >/dev/null

  mkdir -p "$iconset"
  local size double
  for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$master" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$master" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$iconset" -o "$APP/Contents/Resources/prboard.icns"
  rm -rf "$workdir"
}

ICON_KEY=""
if make_icon 2>/dev/null; then
  step "Icon generated (Contents/Resources/prboard.icns)"
  ICON_KEY="	<key>CFBundleIconFile</key>
	<string>prboard</string>"
else
  step "Icon generation failed — continuing without an icon"
fi

# --- Info.plist ------------------------------------------------------------
step "Writing Info.plist"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>$BUNDLE_ID</string>
	<key>CFBundleName</key>
	<string>prboard</string>
	<key>CFBundleDisplayName</key>
	<string>prboard</string>
	<key>CFBundleExecutable</key>
	<string>prboard</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>NSHighResolutionCapable</key>
	<true/>
$ICON_KEY
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# --- ad-hoc codesign -------------------------------------------------------
step "Codesigning (ad-hoc)"
codesign --force --deep -s - "$APP"
codesign --verify --deep "$APP"

# --- install ---------------------------------------------------------------
step "Installing to $INSTALL_DIR/prboard.app"
mkdir -p "$INSTALL_DIR"
rm -rf "$INSTALL_DIR/prboard.app"
ditto "$APP" "$INSTALL_DIR/prboard.app"

# Nudge LaunchServices so Spotlight picks it up promptly (best-effort).
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
[[ -x "$LSREGISTER" ]] && "$LSREGISTER" -f "$INSTALL_DIR/prboard.app" >/dev/null 2>&1 || true

step "Done: $INSTALL_DIR/prboard.app (v$VERSION, ad-hoc signed)"
echo "Launch with Spotlight ('prboard') or: open ~/Applications/prboard.app"
