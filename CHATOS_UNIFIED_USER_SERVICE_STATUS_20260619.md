# ChatOS Unified User Service Status

This file records the implementation status of the unified user-service architecture.

## What Was Implemented

### 1. New Unified User Service

Added a standalone `user_service`:

- Backend: `user_service/backend`
- Frontend: `user_service/frontend`

It manages:

- real ChatOS users
- Task Runner agent accounts
- user-to-agent ownership
- Task Runner delegation token exchange

### 2. Real User Owns Agent Accounts

The ownership model is now explicit:

- one real user can create multiple agent accounts
- each agent account belongs to exactly one real user
- the current user can manage only that user's own agent accounts
- `super_admin` can manage all users and reassign ownership when needed

This is the concrete answer to the business requirement:

- ChatOS users and task users are unified
- every real user can create and own agent accounts

### 3. ChatOS Integration

`chat_app_server_rs` was updated so that, when `user_service` is configured:

- `/api/auth/register` proxies to `user_service`
- `/api/auth/login` proxies to `user_service`
- `/api/auth/me` proxies to `user_service`
- `/api/auth/agent-accounts` loads the current user's agent accounts from `user_service`

ChatOS token parsing now supports JWTs issued by `user_service` for human users.

### 4. Contact Task Runner Config Change

ChatOS contact configuration now supports:

- `task_runner_base_url`
- `task_runner_agent_account_id`

Instead of requiring stored Task Runner username/password for the main flow.

Old username/password fields are still kept as a compatibility fallback.

### 5. Runtime Token Exchange Flow

The new preferred runtime flow is:

1. a human user logs into ChatOS
2. ChatOS gets the current human user's access token
3. ChatOS reads the contact's `task_runner_agent_account_id`
4. ChatOS calls `user_service /api/token/exchange/task-runner`
5. `user_service` verifies that the current human user owns that agent account
6. `user_service` issues a short-lived Task Runner JWT for that agent account
7. ChatOS uses that JWT to call Task Runner

This removes the need for ChatOS to keep long-lived Task Runner agent passwords in the normal flow.

### 6. Task Runner Integration

`task_runner_service/backend` now accepts `user_service` JWTs with:

- issuer from `TASK_RUNNER_USER_SERVICE_JWT_ISSUER`
- audience from `TASK_RUNNER_USER_SERVICE_TASK_RUNNER_AUDIENCE`
- principal type `agent_account`

That allows Task Runner to trust delegated agent identity issued by the unified user service.

## Frontend Scope

The `user_service` frontend is implemented with:

- React
- Vite
- Ant Design

It provides UI for:

- login
- user management
- agent account management
- token exchange inspection
- settings

## Validation

Validated successfully:

- `cargo check --manifest-path user_service/backend/Cargo.toml`
- `cargo check --tests --manifest-path user_service/backend/Cargo.toml`
- `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
- `cargo check --tests --manifest-path chat_app_server_rs/Cargo.toml`
- `cargo check --manifest-path task_runner_service/backend/Cargo.toml`
- `cargo check --tests --manifest-path task_runner_service/backend/Cargo.toml`
- `npm.cmd install` in `user_service/frontend`
- `npm.cmd run build` in `user_service/frontend`
- `npm.cmd run type-check` and `npm.cmd run build` in `chat_app`
- `docker compose config`
- live `user_service` smoke flow: register human user -> create owned agent account -> exchange Task Runner token

Additional implementation detail completed during validation:

- added in-repo `crates/memory_engine_sdk` to replace the previous external path dependency
- removed `ssh2` vendored OpenSSL usage in `chat_app_server_rs` and `task_runner_service/backend` so Windows builds no longer depend on a separate Perl/OpenSSL toolchain
- added [scripts/smoke-user-service-flow.ps1](/C:/project/learn/chatos_rs/scripts/smoke-user-service-flow.ps1) plus `make smoke-user-service-flow` for repeatable local API verification
- added compile-checked JWT trust tests for ChatOS `human_user` parsing and Task Runner `agent_account` parsing
- fixed several pre-existing `cargo check --tests` blockers in `chat_app_server_rs` that were unrelated to the user-service business logic but prevented test code from compiling
- added Windows -> WSL Rust development helpers: [scripts/chatos-wsl.ps1](/C:/project/learn/chatos_rs/scripts/chatos-wsl.ps1), [scripts/bootstrap-wsl-dev.sh](/C:/project/learn/chatos_rs/scripts/bootstrap-wsl-dev.sh), and matching `make *-wsl` targets

Additional environment limitation during this turn:

- Docker daemon is currently unavailable on this machine, so container config was validated but image build/run was not executed here
- `chat_app_server_rs` runtime proxy could not be started on this machine because Windows application control blocks Cargo build-script executables during `cargo run`; compile-time validation still passed

## Required Environment Variables

ChatOS side:

- `CHATOS_USER_SERVICE_BASE_URL`
- `CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS`
- `CHATOS_USER_SERVICE_JWT_SECRET`
- `CHATOS_USER_SERVICE_JWT_ISSUER`
- `CHATOS_USER_SERVICE_USER_AUDIENCE`

Task Runner side:

- `TASK_RUNNER_USER_SERVICE_JWT_SECRET`
- `TASK_RUNNER_USER_SERVICE_JWT_ISSUER`
- `TASK_RUNNER_USER_SERVICE_TASK_RUNNER_AUDIENCE`

These entries were also added to the repository `.env.example`.

## Remaining Follow-Up

- remove the old Task Runner direct username/password flow after migration is complete
- decide whether to move to JWKS instead of shared JWT secret configuration
- clean up remaining non-blocking Rust warnings in existing backend modules
- once Docker daemon is available, verify `docker compose build user-service-backend user-service-frontend`
