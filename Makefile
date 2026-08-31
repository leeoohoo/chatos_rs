# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

SHELL := /bin/bash

.PHONY: help dev docker-up docker-fast docker-dev docker-rebuild docker-restart docker-restart-fast docker-restart-dev docker-build docker-clean-images docker-down docker-reset docker-logs docker-ps docker-config
.PHONY: local-dev local-dev-stop local-dev-status local-dev-logs
.PHONY: build build-rust build-frontends build-macos-client build-windows-client build-browser-plugin build-computer-use-plugin build-document-plugin build-plugins
.PHONY: test smoke smoke-repo smoke-local-project-entry verify verify-fast test-rust-workspaces check-frontends code-size-report hotspot-line-warnings
.PHONY: test-chat-app-server test-user-service test-task-runner-service test-local-connector-service test-mcp-management-service test-memory-engine
.PHONY: test-macos-client test-windows-client test-browser-plugin test-computer-use-plugin test-document-plugin test-plugins
.PHONY: type-check-user-service-frontend

help:
	@echo "Chat OS tasks:"
	@echo "  make dev                    # build/start the Docker stack from local source"
	@echo "  make local-dev              # start host-side cloud services and administration frontends"
	@echo "  make local-dev-stop         # stop host-side local dev stack"
	@echo "  make local-dev-status       # show host-side local dev stack status"
	@echo "  make docker-up              # pull/start the prebuilt Docker stack"
	@echo "  make docker-fast            # start/reconcile existing Docker images without pulling"
	@echo "  make docker-dev             # build/start Docker images from local source"
	@echo "  make docker-rebuild         # rebuild selected services: SERVICES=\"task-runner-backend\""
	@echo "  make docker-restart         # recreate the prebuilt Docker stack"
	@echo "  make docker-restart-fast    # recreate existing Docker images without pulling"
	@echo "  make docker-restart-dev     # recreate with local image builds"
	@echo "  make docker-build           # build Docker images without starting"
	@echo "  make docker-clean-images    # remove dangling <none>:<none> Docker images"
	@echo "  make docker-down            # stop Docker services"
	@echo "  make docker-reset           # stop Docker services and remove volumes"
	@echo "  make docker-logs            # follow Docker service logs"
	@echo "  make docker-ps              # show Docker service status"
	@echo "  make build                  # build Rust services and frontends"
	@echo "  make build-macos-client     # build the native macOS client"
	@echo "  make build-windows-client   # build the native Windows client"
	@echo "  make build-plugins          # build the three first-party plugins"
	@echo "  make test                   # run repo checks and core backend/frontend tests"
	@echo "  make smoke                  # run lightweight repo checks"
	@echo "  make smoke-local-project-entry # verify Config Center -> ChatOS local-project UI switch"
	@echo "  make verify-fast            # run repository quality policies and Rust lint"
	@echo "  make verify                 # run full Rust and frontend verification"

dev: docker-dev

local-dev:
	@bash scripts/local-dev-stack.sh up

local-dev-stop:
	@bash scripts/local-dev-stack.sh down

local-dev-status:
	@bash scripts/local-dev-stack.sh status

local-dev-logs:
	@bash scripts/local-dev-stack.sh logs $(SERVICE)

docker-up:
	@docker/deploy.sh up

docker-fast:
	@docker/deploy.sh fast

docker-dev:
	@docker/deploy.sh dev

docker-rebuild:
	@docker/deploy.sh rebuild $(SERVICES)

docker-restart:
	@docker/deploy.sh restart

docker-restart-fast:
	@docker/deploy.sh restart-fast

docker-restart-dev:
	@docker/deploy.sh restart-dev

docker-build:
	@docker/deploy.sh build

docker-clean-images:
	@docker/deploy.sh clean-images

docker-down:
	@docker/deploy.sh down

docker-reset:
	@docker/deploy.sh reset

docker-logs:
	@docker/deploy.sh logs

docker-ps:
	@docker/deploy.sh ps

docker-config:
	@docker compose -f docker/compose.yml -f docker/compose.platform.yml config >/dev/null
	@docker compose -f docker/compose.yml -f docker/compose.platform.yml -f docker/compose.build.yml config >/dev/null

build: build-rust build-frontends

build-rust:
	@cargo build
	@cd user_service/backend && cargo build
	@cd memory_engine/backend && cargo build

build-frontends:
	@cd config_center_service/frontend && npm run build
	@cd user_service/frontend && npm run build
	@cd task_runner_service/frontend && npm run build
	@cd memory_engine/frontend && npm run build
	@cd project_management_service/frontend && npm run build
	@cd plugin_management_service/frontend && npm run build
	@cd official_website_service/frontend && npm run build

build-macos-client:
	@swift build --package-path clients/macos

build-windows-client:
	@dotnet build clients/windows/ChatOS.Win.sln --configuration Release

build-browser-plugin:
	@cargo build --manifest-path plugins/browser/Cargo.toml --workspace

build-computer-use-plugin:
	@case "$$(uname -s)" in \
		Darwin) swift build --package-path plugins/computer-use ;; \
		MINGW*|MSYS*|CYGWIN*) dotnet build plugins/computer-use/windows/VisualComputerUse.Windows/VisualComputerUse.Windows.csproj --configuration Release ;; \
		*) echo "Computer Use plugin builds are supported on macOS and Windows" >&2; exit 2 ;; \
	esac

build-document-plugin:
	@npm --prefix plugins/document run build

build-plugins: build-browser-plugin build-computer-use-plugin build-document-plugin

test: smoke test-chat-app-server test-user-service test-task-runner-service test-local-connector-service test-mcp-management-service test-memory-engine

smoke: smoke-repo

verify-fast:
	@bash scripts/verify-repository.sh fast

verify:
	@bash scripts/verify-repository.sh full

test-rust-workspaces:
	@bash scripts/verify-repository.sh rust-test

check-frontends:
	@bash scripts/verify-repository.sh frontends

smoke-repo:
	@bash scripts/check_api_surface.sh
	@bash scripts/check_api_path_baseline.sh
	@python3 scripts/check-agent-tool-plane-boundaries.py
	@bash scripts/check-hotspot-line-budgets.sh
	@bash -n docker/deploy.sh
	@bash -n docker/deploy-harness-ci.sh
	@bash -n scripts/local-dev-stack.sh scripts/local-dev-stack/environment.sh scripts/local-dev-stack/services.sh
	@docker compose -f docker/compose.yml -f docker/compose.platform.yml -f docker/compose.local-dev.yml config >/dev/null
	@docker compose -f docker/compose.yml -f docker/compose.platform.yml config >/dev/null
	@docker compose -f docker/compose.yml -f docker/compose.platform.yml -f docker/compose.build.yml config >/dev/null
	@bash scripts/check-large-files.sh --fail

test-chat-app-server:
	@cargo test -p chat_app_server_rs -q

test-user-service:
	@cd user_service/backend && cargo test -q
	@cd user_service/frontend && npm run type-check
	@cd user_service/frontend && npm run build

test-task-runner-service:
	@cargo test -p task_runner_service_backend -q

test-local-connector-service:
	@cargo test -p local_connector_service_backend -q

test-mcp-management-service:
	@cargo test -p mcp_management_service_backend -q

test-memory-engine:
	@cd memory_engine/backend && cargo test -q

test-macos-client:
	@swift test --package-path clients/macos

test-windows-client:
	@dotnet test clients/windows/ChatOS.Win.sln --configuration Release

test-browser-plugin:
	@cargo test --manifest-path plugins/browser/Cargo.toml --workspace

test-computer-use-plugin:
	@case "$$(uname -s)" in \
		Darwin) swift test --package-path plugins/computer-use ;; \
		MINGW*|MSYS*|CYGWIN*) dotnet build plugins/computer-use/windows/VisualComputerUse.Windows/VisualComputerUse.Windows.csproj --configuration Release ;; \
		*) echo "Computer Use plugin tests are supported on macOS and Windows" >&2; exit 2 ;; \
	esac

test-document-plugin:
	@npm --prefix plugins/document run vendor:fetch:current
	@npm --prefix plugins/document test

test-plugins: test-browser-plugin test-computer-use-plugin test-document-plugin

code-size-report:
	@bash scripts/code-size-report.sh

hotspot-line-warnings:
	@bash scripts/check-hotspot-line-budgets.sh --warn-planned

type-check-user-service-frontend:
	@cd user_service/frontend && npm run type-check
