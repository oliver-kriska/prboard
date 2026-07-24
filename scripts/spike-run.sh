#!/usr/bin/env bash
# Launch the Step-0 overnight run: release prboard + RSS sampler + a
# caffeinate scoped to the app so the Mac's idle sleep doesn't pause the
# measurement. Everything is nohup'd — safe to close the terminal.
#
# Usage: scripts/spike-run.sh <owner/repo> [extra prboard args]
# Stop:  quit prboard (q) — the sampler and caffeinate follow it down.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO=${1:?usage: spike-run.sh <owner/repo> [args...]}
shift || true

BIN=target/release/prboard
[ -x "$BIN" ] || { echo "no release binary — run: cargo build --release" >&2; exit 1; }
mkdir -p measurements

nohup "$BIN" --repo "$REPO" "$@" > measurements/app.log 2>&1 &
APP_PID=$!
sleep 2
kill -0 "$APP_PID" 2>/dev/null || { echo "prboard exited at startup:"; cat measurements/app.log; exit 1; }

nohup scripts/measure.sh "$APP_PID" > measurements/measure.log 2>&1 &
SAMPLER_PID=$!
nohup caffeinate -i -w "$APP_PID" >/dev/null 2>&1 &

echo "prboard PID $APP_PID · sampler PID $SAMPLER_PID (caffeinate tied to the app)"
echo "leave the app open overnight; tomorrow run: scripts/measure-summary.sh"
