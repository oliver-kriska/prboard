#!/usr/bin/env bash
# Regenerate the golden files that pin core/src/board.rs to the prototype's
# output. The jq programs in scripts/prototype-jq/ are verbatim copies of
# ~/.claude/skills/pr-board/scripts/pr-board.sh — the behavioral spec.
# Run from the repo root after changing a fixture; commit the results.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="acme/widgets"
ME="oliver"

mkdir -p core/tests/golden
jq --arg repo "$REPO" --arg me "$ME" -f scripts/prototype-jq/authored.jq \
  core/tests/fixtures/authored_response.json > core/tests/golden/authored.json
jq --arg repo "$REPO" --arg me "$ME" -f scripts/prototype-jq/review.jq \
  core/tests/fixtures/review_response.json > core/tests/golden/review.json

echo "regenerated core/tests/golden/{authored,review}.json"
