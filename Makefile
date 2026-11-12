SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO ?= cargo
NPM ?= npm
WEB := apps/web
COVERAGE_MIN ?= 80
# The traceability ratchet: how many requirements may still be uncovered. It only goes down.
# M0 = 50 (8 covered), M1 tightens to 40, and so on; docs/STATE.md records the value.
TRACE_MAX_MISSING ?= 50

.PHONY: help install deps-lock run build fmt fmt-check lint typecheck test coverage deny bench \
        migrate trace secrets check ui-install ui-run ui-build ui-typecheck ui-lint ui-test ui-a11y

help: ## Show the available targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

install: ## Fetch Rust crates and install the web dependencies
	$(CARGO) fetch
	$(NPM) --prefix $(WEB) ci

deps-lock: ## Generate Cargo.lock and package-lock.json once, then commit both
	$(CARGO) generate-lockfile
	$(NPM) --prefix $(WEB) install --package-lock-only --ignore-scripts

run: ## Serve the API on 127.0.0.1:3000 (needs a database)
	$(CARGO) run --bin boma -- serve

build: ## Build the release binary
	$(CARGO) build --release --bin boma

fmt: ## Format the workspace
	$(CARGO) fmt --all

fmt-check: ## Fail if formatting drifted
	$(CARGO) fmt --all -- --check

lint: ## Clippy, warnings are errors
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

typecheck: ## Check Rust types and the web TypeScript
	$(CARGO) check --workspace --all-targets --all-features --locked
	$(NPM) --prefix $(WEB) run typecheck

test: ## Run every Rust test, golden and contract included
	$(CARGO) nextest run --workspace --all-features

coverage: ## Line coverage with the floor from docs/PLAN.md
	$(CARGO) llvm-cov --workspace --all-features --summary-only --fail-under-lines $(COVERAGE_MIN)

deny: ## Licences, advisories, duplicates, unused dependencies
	$(CARGO) deny check
	$(CARGO) machete

bench: ## Benchmarks (arrive with M1 hot paths)
	$(CARGO) bench --workspace

migrate: ## Apply migrations against BOMA_DATABASE_URL
	$(CARGO) run --bin boma -- migrate

trace: ## Traceability ratchet: REQ ids in test names
	python3 tools/trace_check.py spec . --max-missing $(TRACE_MAX_MISSING)

secrets: ## Refuse a credential in the tree
	python3 tools/secret_scan.py .

check: fmt-check lint typecheck test trace secrets ## Everything CI runs, in order

ui-install: ## Install the web dependencies
	$(NPM) --prefix $(WEB) ci

ui-run: ## Serve the web shell on 127.0.0.1:3100 (dev server)
	$(NPM) --prefix $(WEB) run dev

ui-build: ## Static export into apps/web/out
	$(NPM) --prefix $(WEB) run build

ui-typecheck: ## Check the web TypeScript
	$(NPM) --prefix $(WEB) run typecheck

ui-lint: ## Lint the web app, warnings are errors
	$(NPM) --prefix $(WEB) run lint

ui-test: ## Playwright end-to-end and UI golden replay
	$(NPM) --prefix $(WEB) run test

ui-a11y: ## axe on every screen, critical and serious fail
	$(NPM) --prefix $(WEB) run test:a11y
