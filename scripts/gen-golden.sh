#!/usr/bin/env bash
# Regenerate the golden files that pin core/src/board.rs to the prototype's
# output. The jq programs in scripts/prototype-jq/ preserve the shell
# prototype's behavior with fictional, configurable tracker data.
# Run from the repo root after changing a fixture; commit the results.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="acme/widgets"
ME="me"
ISSUE_PATTERN='PROJ-[0-9]+'
ISSUE_URL_TEMPLATE='https://tracker.example.test/issues/{id}'

mkdir -p core/tests/golden
jq --arg repo "$REPO" --arg me "$ME" \
  --arg issue_pattern "$ISSUE_PATTERN" --arg issue_url_template "$ISSUE_URL_TEMPLATE" \
  -f scripts/prototype-jq/authored.jq \
  core/tests/fixtures/authored_response.json > core/tests/golden/authored.json
jq --arg repo "$REPO" --arg me "$ME" \
  --arg issue_pattern "$ISSUE_PATTERN" --arg issue_url_template "$ISSUE_URL_TEMPLATE" \
  -f scripts/prototype-jq/review.jq \
  core/tests/fixtures/review_response.json > core/tests/golden/review.json

echo "regenerated core/tests/golden/{authored,review}.json"
