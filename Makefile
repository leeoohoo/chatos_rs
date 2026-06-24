SHELL := /bin/bash

.PHONY: help dev restart status stop build test smoke smoke-user-service-flow
.PHONY: restart-wsl status-wsl stop-wsl bootstrap-wsl
.PHONY: restart-user-service-wsl status-user-service-wsl stop-user-service-wsl
.PHONY: restart-task-runner-wsl status-task-runner-wsl stop-task-runner-wsl
.PHONY: restart-memory-engine-wsl status-memory-engine-wsl stop-memory-engine-wsl
.PHONY: restart-all-wsl status-all-wsl stop-all-wsl
.PHONY: restart-all-win status-all-win stop-all-win
.PHONY: restart-user-service status-user-service stop-user-service
.PHONY: restart-task-runner status-task-runner stop-task-runner
.PHONY: restart-memory-engine status-memory-engine stop-memory-engine
.PHONY: restart-db-hub status-db-hub stop-db-hub
.PHONY: restart-all status-all stop-all
.PHONY: build-chat-app-server build-chat-app build-db-hub build-user-service
.PHONY: test-chat-app-server test-chat-app test-db-hub test-user-service
.PHONY: smoke-repo smoke-chat-app-server smoke-chat-app smoke-db-hub smoke-user-service
.PHONY: type-check-db-hub-frontend lint-db-hub-frontend type-check-user-service-frontend

help:
	@echo "Chatos RS root tasks:"
	@echo "  make dev                 # same as restart"
	@echo "  make restart             # restart main backend + frontend via restart_services.sh"
	@echo "  make status              # show main backend + frontend status"
	@echo "  make stop                # stop main backend + frontend"
	@echo "  make restart-user-service # restart user_service backend + frontend"
	@echo "  make status-user-service  # show user_service status"
	@echo "  make stop-user-service    # stop user_service backend + frontend"
	@echo "  make restart-task-runner  # restart task_runner backend + frontend"
	@echo "  make status-task-runner   # show task_runner status"
	@echo "  make stop-task-runner     # stop task_runner backend + frontend"
	@echo "  make restart-memory-engine # restart memory_engine backend + frontend"
	@echo "  make status-memory-engine  # show memory_engine status"
	@echo "  make stop-memory-engine    # stop memory_engine backend + frontend"
	@echo "  make restart-db-hub       # restart db_connection_hub backend + frontend"
	@echo "  make status-db-hub        # show db_connection_hub status"
	@echo "  make stop-db-hub          # stop db_connection_hub backend + frontend"
	@echo "  make restart-all          # restart memory_engine + user_service + chatos + task_runner"
	@echo "  make status-all           # show full stack status"
	@echo "  make stop-all             # stop full stack"
	@echo "  make build               # build key subprojects"
	@echo "  make test                # run repo checks + subproject tests"
	@echo "  make smoke               # repo governance + lightweight cross-subproject probes"
	@echo "  make smoke-user-service-flow # call the live user_service API flow end-to-end"
	@echo "  make bootstrap-wsl       # bootstrap Ubuntu/WSL dependencies for Rust + Node dev"
	@echo "  make restart-wsl         # run root restart_services.sh inside WSL"
	@echo "  make status-wsl          # show root service status inside WSL"
	@echo "  make stop-wsl            # stop root services inside WSL"
	@echo "  make restart-user-service-wsl # run user_service restart inside WSL"
	@echo "  make status-user-service-wsl  # show user_service status inside WSL"
	@echo "  make stop-user-service-wsl    # stop user_service inside WSL"
	@echo "  make restart-task-runner-wsl  # run task_runner restart inside WSL"
	@echo "  make status-task-runner-wsl   # show task_runner status inside WSL"
	@echo "  make stop-task-runner-wsl     # stop task_runner inside WSL"
	@echo "  make restart-memory-engine-wsl # run memory_engine restart inside WSL"
	@echo "  make status-memory-engine-wsl  # show memory_engine status inside WSL"
	@echo "  make stop-memory-engine-wsl    # stop memory_engine inside WSL"
	@echo "  make restart-all-wsl      # restart the full stack inside WSL"
	@echo "  make status-all-wsl       # show full stack status inside WSL"
	@echo "  make stop-all-wsl         # stop full stack inside WSL"
	@echo "  make restart-all-win      # restart the validated Windows local stack"
	@echo "  make status-all-win       # show Windows local stack status"
	@echo "  make stop-all-win         # stop the Windows local stack"

dev: restart

restart:
	@./restart_services.sh restart

status:
	@./restart_services.sh status

stop:
	@./restart_services.sh stop

restart-user-service:
	@bash user_service/restart_services.sh restart

status-user-service:
	@bash user_service/restart_services.sh status

stop-user-service:
	@bash user_service/restart_services.sh stop

restart-task-runner:
	@./restart_task_runner_service.sh restart

status-task-runner:
	@./restart_task_runner_service.sh status

stop-task-runner:
	@./restart_task_runner_service.sh stop

restart-memory-engine:
	@bash memory_engine/restart_services.sh restart

status-memory-engine:
	@bash memory_engine/restart_services.sh status

stop-memory-engine:
	@bash memory_engine/restart_services.sh stop

restart-db-hub:
	@./db_connection_hub/restart_services.sh restart

status-db-hub:
	@./db_connection_hub/restart_services.sh status

stop-db-hub:
	@./db_connection_hub/restart_services.sh stop

restart-all:
	@./restart_all_services.sh restart

status-all:
	@./restart_all_services.sh status

stop-all:
	@./restart_all_services.sh stop

build: build-chat-app-server build-chat-app build-db-hub build-user-service

build-chat-app-server:
	@cd chat_app_server_rs && cargo build

build-chat-app:
	@cd chat_app && npm run build

build-db-hub:
	@cd db_connection_hub/backend && cargo build
	@cd db_connection_hub/frontend && npm run build

build-user-service:
	@cd user_service/backend && cargo build
	@cd user_service/frontend && npm run build

test: smoke test-chat-app-server test-chat-app test-db-hub test-user-service

smoke: smoke-repo smoke-chat-app-server smoke-chat-app smoke-db-hub smoke-user-service

smoke-repo:
	@bash scripts/check_api_surface.sh
	@bash scripts/check_api_path_baseline.sh
	@bash scripts/check-hotspot-line-budgets.sh
	@bash -n restart_services.sh
	@bash -n db_connection_hub/restart_services.sh
	@bash -n user_service/restart_services.sh
	@bash scripts/check-large-files.sh --fail

smoke-chat-app-server:
	@cd chat_app_server_rs && cargo check

smoke-chat-app:
	@cd chat_app && npm run type-check

smoke-db-hub:
	@cd db_connection_hub/backend && cargo check
	@cd db_connection_hub/frontend && npm run type-check

smoke-user-service:
	@cd user_service/backend && cargo check
	@cd user_service/frontend && npm run type-check

smoke-user-service-flow:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/smoke-user-service-flow.ps1

bootstrap-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action bootstrap -Target main

restart-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target main

status-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action status -Target main

stop-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action stop -Target main

restart-user-service-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target user-service

status-user-service-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action status -Target user-service

stop-user-service-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action stop -Target user-service

restart-task-runner-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target task-runner

status-task-runner-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action status -Target task-runner

stop-task-runner-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action stop -Target task-runner

restart-memory-engine-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target memory-engine

status-memory-engine-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action status -Target memory-engine

stop-memory-engine-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action stop -Target memory-engine

restart-all-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target all

status-all-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action status -Target all

stop-all-wsl:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action stop -Target all

restart-all-win:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action restart

status-all-win:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action status

stop-all-win:
	@powershell.exe -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action stop

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

test-user-service:
	@cd user_service/backend && cargo test -q
	@cd user_service/frontend && npm run type-check
	@cd user_service/frontend && npm run build
