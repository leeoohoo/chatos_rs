SHELL := /bin/bash

.PHONY: help dev restart status stop build test smoke
.PHONY: build-chat-app-server build-chat-app build-db-hub
.PHONY: test-chat-app-server test-chat-app test-db-hub
.PHONY: smoke-repo smoke-chat-app-server smoke-chat-app smoke-db-hub
.PHONY: type-check-db-hub-frontend lint-db-hub-frontend

help:
	@echo "Chatos RS root tasks:"
	@echo "  make dev                 # same as restart"
	@echo "  make restart             # restart main backend + frontend via restart_services.sh"
	@echo "  make status              # show main backend + frontend status"
	@echo "  make stop                # stop main backend + frontend"
	@echo "  make build               # build key subprojects"
	@echo "  make test                # run repo checks + subproject tests"
	@echo "  make smoke               # repo governance + lightweight cross-subproject probes"

dev: restart

restart:
	@./restart_services.sh restart

status:
	@./restart_services.sh status

stop:
	@./restart_services.sh stop

build: build-chat-app-server build-chat-app build-db-hub

build-chat-app-server:
	@cd chat_app_server_rs && cargo build

build-chat-app:
	@cd chat_app && npm run build

build-db-hub:
	@cd db_connection_hub/backend && cargo build
	@cd db_connection_hub/frontend && npm run build

test: smoke test-chat-app-server test-chat-app test-db-hub

smoke: smoke-repo smoke-chat-app-server smoke-chat-app smoke-db-hub

smoke-repo:
	@bash scripts/check_api_surface.sh
	@bash scripts/check_api_path_baseline.sh
	@bash scripts/check-hotspot-line-budgets.sh
	@bash -n restart_services.sh
	@bash -n db_connection_hub/restart_services.sh
	@bash scripts/check-large-files.sh --fail

smoke-chat-app-server:
	@cd chat_app_server_rs && cargo check

smoke-chat-app:
	@cd chat_app && npm run type-check

smoke-db-hub:
	@cd db_connection_hub/backend && cargo check
	@cd db_connection_hub/frontend && npm run type-check

test-chat-app-server:
	@cd chat_app_server_rs && cargo test -q

test-chat-app:
	@cd chat_app && npm run test -- --run
	@cd chat_app && npm run lint
	@cd chat_app && npm run type-check

test-db-hub:
	@cd db_connection_hub/backend && cargo test -q
	@cd db_connection_hub/frontend && npm run type-check
	@cd db_connection_hub/frontend && npm run build
