# Homebrew cask TEMPLATE for prboard.
#
# This file lives in the app repo as a template only. The live copy belongs in
# the personal tap:  oliver-kriska/homebrew-tap  →  Casks/prboard.rb
# (install name becomes: brew install --cask oliver-kriska/tap/prboard)
#
# Per-release fills (see packaging/RELEASING.md):
#   - version:  the release tag without the leading "v"
#   - sha256:   shasum -a 256 prboard-<version>-universal.zip
# Everything else stays as-is between releases.
#
# PRECONDITION: the .app inside the zip MUST be Developer-ID-signed, notarized,
# and stapled. Homebrew quarantines cask downloads, and from 2026-09-01 casks
# that fail Gatekeeper are unsupported — an ad-hoc-signed zip will not install
# cleanly for anyone.
cask "prboard" do
  version "0.1.0"                                    # <- FILL per release
  sha256 "REPLACE_WITH_SHA256_OF_RELEASE_ZIP"        # <- FILL per release

  url "https://github.com/oliver-kriska/prboard/releases/download/v#{version}/prboard-#{version}-universal.zip"
  name "prboard"
  desc "GitHub pull-request review dashboard"
  homepage "https://github.com/oliver-kriska/prboard"

  livecheck do
    url :url
    strategy :github_latest
  end

  # prboard shells out to the GitHub CLI for all API access; without gh
  # (authenticated via `gh auth login`) the app cannot load any data.
  depends_on formula: "gh"
  depends_on macos: ">= :monterey"

  app "prboard.app"

  zap trash: [
    "~/.config/prboard",
    "~/Library/Application Support/prboard",
    "~/Library/Caches/dev.oliverkriska.prboard",
    "~/Library/Saved Application State/dev.oliverkriska.prboard.savedState",
  ]
end
