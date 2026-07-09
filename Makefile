# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

SHELL := /bin/bash

.PHONY: help dev docker-up docker-fast docker-dev docker-rebuild docker-restart docker-restart-fast docker-restart-dev docker-build docker-clean-images docker-down docker-reset docker-logs docker-ps docker-config
.PHONY: local-dev local-dev-stop local-dev-status local-dev-logs
.PHONY: local-connector-client local-connector-client-status local-connector-client-stop
.PHONY: build build-rust build-frontends test smoke smoke-repo code-size-report hotspot-line-warnings
.PHONY: test-chat-app-server test-chat-app test-user-service
.PHONY: type-check-user-service-frontend

help:
	@echo "Chatos RS tasks:"
	@echo "  make dev                    # build/start the Docker stack from local source"
	@echo "  make local-dev              # start host-side local dev stack for fast testing"
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
	@echo "  make local-connector-client # run the host-side Local Connector client"
	@echo "  make build                  # build Rust services and frontends"
	@echo "  make test                   # run repo checks and focused tests"
	@echo "  make smoke                  # run lightweight repo checks"

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
	@docker compose -f docker/compose.yml config >/dev/null
	@docker compose -f docker/compose.yml -f docker/compose.build.yml config >/dev/null

local-connector-client:
	@bash local_connector_client/restart_services.sh restart

local-connector-client-status:
	@bash local_connector_client/restart_services.sh status

local-connector-client-stop:
	@bash local_connector_client/restart_services.sh stop

build: build-rust build-frontends

build-rust:
	@cargo build
	@cd user_service/backend && cargo build
	@cd memory_engine/backend && cargo build

build-frontends:
	@cd chatos/frontend && npm run build
	@cd user_service/frontend && npm run build
	@cd task_runner_service/frontend && npm run build
	@cd memory_engine/frontend && npm run build
	@cd project_management_service/frontend && npm run build
	@cd sandbox_manager_service/frontend && npm run build
	@cd official_website_service/frontend && npm run build

test: smoke test-chat-app-server test-chat-app test-user-service

smoke: smoke-repo

smoke-repo:
	@bash scripts/check_api_surface.sh
	@bash scripts/check_api_path_baseline.sh
	@bash scripts/check-hotspot-line-budgets.sh
	@bash -n docker/deploy.sh
	@bash -n scripts/local-dev-stack.sh
	@docker compose -f docker/compose.yml config >/dev/null
	@docker compose -f docker/compose.yml -f docker/compose.build.yml config >/dev/null
	@bash scripts/check-large-files.sh --fail

test-chat-app-server:
	@cargo test -p chat_app_server_rs -q

test-chat-app:
	@cd chatos/frontend && npm run test -- --run
	@cd chatos/frontend && npm run lint
	@cd chatos/frontend && npm run type-check

test-user-service:
	@cd user_service/backend && cargo test -q
	@cd user_service/frontend && npm run type-check
	@cd user_service/frontend && npm run build

code-size-report:
	@bash scripts/code-size-report.sh

hotspot-line-warnings:
	@bash scripts/check-hotspot-line-budgets.sh --warn-planned

type-check-user-service-frontend:
	@cd user_service/frontend && npm run type-check
