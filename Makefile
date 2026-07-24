# prboard developer tasks. `make check` mirrors CI exactly.
#
# Fast targets (fmt/lint-core/test-core) need no GPU/Metal and are what CI runs.
# The GPUI binary (build/run/release) compiles Metal shaders and needs the Xcode
# Metal Toolchain locally — see CLAUDE.md.

.PHONY: help fmt fmt-check lint lint-all test build release run install check ci fix \
        hooks changelog unreleased clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

## ---- Quality gate (fast, no Metal — matches CI) ------------------------------

fmt: ## Format the whole workspace
	cargo fmt

fmt-check: ## Check formatting (CI mode)
	cargo fmt --check

lint: ## Clippy on prboard-core, warnings as errors (matches CI)
	cargo clippy -p prboard-core --all-targets -- -D warnings

test: ## Run the prboard-core spec + golden suite (matches CI)
	cargo test -p prboard-core

check: fmt-check lint test ## Full local gate — run before every commit/push

ci: check ## Alias: simulate CI locally

fix: ## Auto-fix formatting and the clippy lints that are auto-fixable
	cargo fmt
	cargo clippy -p prboard-core --fix --allow-dirty --allow-staged

## ---- GPUI binary (needs the Metal Toolchain locally) -------------------------

lint-all: ## Clippy on the ENTIRE workspace incl. the GPUI binary (Metal required)
	cargo clippy --all-targets -- -D warnings

build: ## Debug build of the app
	cargo build

release: ## Release build (LTO) — used for measurements and shipping
	cargo build --release

run: ## Run the debug app
	cargo run

install: ## Release-build and install ~/Applications/prboard.app (refuses if running)
	@pgrep -f 'prboard.app/Contents/MacOS/prboard' >/dev/null \
		&& { echo "prboard.app is running — quit it first (macOS SIGKILLs an app whose binary is swapped; this also protects a live memory-gate run)"; exit 1; } \
		|| true
	cargo build --release
	scripts/bundle-app.sh

## ---- Release notes / changelog -----------------------------------------------

changelog: ## Regenerate CHANGELOG.md from the commit history (git-cliff)
	git cliff --config cliff.toml -o CHANGELOG.md

unreleased: ## Print what's on main but not in the latest tag
	@git cliff --config cliff.toml --unreleased --strip all

## ---- Setup -------------------------------------------------------------------

hooks: ## Install the repo git hooks (pre-commit + commit-msg)
	git config core.hooksPath .githooks
	@echo "core.hooksPath -> .githooks (pre-commit + commit-msg active)"

clean: ## Remove build artifacts
	cargo clean
