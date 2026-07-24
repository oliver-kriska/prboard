#!/usr/bin/env bash
# capture-window.sh <pid> <out.png> — screenshot ONE prboard window, correctly.
#
# GPUI does not repaint occluded windows, so the window is first brought
# frontmost (by PID — several prboard instances may run at once), given a
# moment to repaint, then captured by CGWindowID so only that window lands
# in the image. No keystrokes are ever sent.
set -euo pipefail

PID=${1:?usage: capture-window.sh <pid> <out.png>}
OUT=${2:?usage: capture-window.sh <pid> <out.png>}
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

kill -0 "$PID" 2>/dev/null || { echo "error: no process $PID" >&2; exit 1; }

osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $PID) to true"
sleep 0.7  # let GPUI paint a fresh frame after activation

WID="$(swift "$HERE/window-id.swift" "$PID")"
screencapture -x -o -l"$WID" "$OUT"
echo "captured window $WID of pid $PID -> $OUT"
