#!/usr/bin/env bash
# Summarize a measure.sh CSV against the Step-0 gate:
# PASS shape = RSS flat (no monotonic climb) and < ~150 MB over the whole run.
# Usage: scripts/measure-summary.sh [csv]   (default: newest in measurements/)
set -euo pipefail

CSV=${1:-$(ls -t measurements/rss-*.csv 2>/dev/null | head -1)}
[ -n "${CSV:-}" ] && [ -f "$CSV" ] || { echo "no CSV found — run scripts/measure.sh first" >&2; exit 1; }

awk -F, 'NR==2 {first=$3; min=$3; max=$3}
     NR>=2 {last=$3; if ($3<min) min=$3; if ($3>max) max=$3; n++}
     END {
       if (n==0) { print "no samples yet"; exit 1 }
       hours = n/60.0
       printf "%s\n  samples: %d (~%.1f h)\n  rss MB: first %.1f · last %.1f · min %.1f · max %.1f\n", FILENAME, n, hours, first, last, min, max
       drift = last-first
       printf "  drift: %+.1f MB over the run\n", drift
       verdict = "LOOKS FLAT"
       if (max >= 150) verdict = "OVER the ~150 MB gate ceiling"
       else if (drift > 20 && last > first*1.2) verdict = "CLIMBING — inspect before calling the gate"
       printf "  read: %s (gate: flat and < ~150 MB over 12-24 h + ~0 idle GPU)\n", verdict
     }' "$CSV"
