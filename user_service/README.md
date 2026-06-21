# User Service

`user_service` is the unified identity service for this repository.

It owns:

- ChatOS human users
- Task Runner agent accounts
- The ownership relation from a real user to that user's agent accounts
- Task Runner delegation token exchange
- User-owned model configs shared by ChatOS, Task Runner, and memory_engine

## Stack

- Backend: Rust + Axum + SQLite + JWT
- Frontend: React + Vite + Ant Design

## Ownership Model

- A `human_user` is the real ChatOS user.
- An `agent_account` is a Task Runner execution identity.
- Every `agent_account` belongs to exactly one `human_user`.
- A real user can create and manage that user's own agent accounts.
- `super_admin` can manage all users and reassign agent ownership when needed.

## Current Integration Status

The service is now integrated into the repository flow:

- `chat_app_server_rs` can proxy `register`, `login`, and `me` to `user_service`
- ChatOS can load the current user's agent accounts from `user_service`
- ChatOS contact Task Runner config now uses `task_runner_agent_account_id`
- ChatOS runtime can exchange the current human user's token plus `task_runner_agent_account_id` for a short-lived Task Runner token
- `task_runner_service/backend` can validate Task Runner audience JWTs issued by `user_service`
- ChatOS model config CRUD can proxy to `user_service`
- `user_service` can sync concrete model configs into `task_runner_service` and `memory_engine`

Backward compatibility is still kept for the old contact-level Task Runner username/password flow when `user_service` is not configured.

## Unified Model Configs

- `user_service` is now the source of truth for user-owned model configs.
- A real user can keep provider credentials here and create that user's own agent accounts here.
- ChatOS may save a provider-only config with `provider + base_url + api_key` and leave `model` blank.
- When ChatOS actually sends a request, the concrete runtime model is still selected later from the fetched provider model list.
- `task_runner_service` and `memory_engine` only receive synced runnable configs when `model` is concrete and non-empty.
- `memory_summary_model_config_id` must point to a config with a concrete `model`.

## Downstream Sync Environment

If you want model config changes in `user_service` to sync into the other services, configure these environment variables:

- `MEMORY_ENGINE_BASE_URL=http://127.0.0.1:7081/api/memory-engine/v1`
- `MEMORY_ENGINE_OPERATOR_TOKEN=...`
- `TASK_RUNNER_BASE_URL=http://127.0.0.1:39090`
- `TASK_RUNNER_CHATOS_CALLBACK_SECRET=...`
- `USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS=5000`

Important behavior:

- If `model` is blank, ChatOS can still use the config as a provider credential entry.
- The downstream sync will skip runnable-model creation for `task_runner_service` and `memory_engine`.
- That skip is returned as `sync_warnings` instead of failing the save request.
- The repository startup scripts now default `MEMORY_ENGINE_OPERATOR_TOKEN` to `chatos-memory-engine-dev-operator-token` for local development.

## Run Backend

```bash
cd user_service/backend
cargo run
```

Default backend address:

- `http://127.0.0.1:39190`

## Run Frontend

```bash
cd user_service/frontend
npm install
npm run dev
```

Default frontend address:

- `http://127.0.0.1:39191`

The frontend uses `/api` proxying to the backend during local development.

On Windows, prefer Git Bash when using the provided `.sh` startup script.

If Windows Smart App Control / Code Integrity blocks Rust execution, prefer WSL:

```powershell
make bootstrap-wsl
make restart-user-service-wsl
make status-user-service-wsl
make stop-user-service-wsl
```

When the repository root `.env` keeps `START_USER_SERVICE=1` and
`CHATOS_USER_SERVICE_BASE_URL=http://127.0.0.1:39190`, the root
`./restart_services.sh restart` flow will also start the local `user_service`.

## Docker Compose

Repository root `docker-compose.yml` now includes:

- `user-service-backend`
- `user-service-frontend`

Start only the unified user service:

```bash
docker compose up -d user-service-backend user-service-frontend
```

Start it together with ChatOS:

```bash
docker compose up -d user-service-backend user-service-frontend backend frontend
```

Current limitation:

- `docker compose config` is validated
- actual Docker image build was not executed here because the local Docker daemon is currently unavailable
- on the current Windows machine, direct `cargo run` can be blocked by Smart App Control / Code Integrity, so the WSL flow is the preferred Rust runtime path

## Default Admin

On first startup the service creates a default `super_admin` account:

- username: `admin`
- password: `admin123456`

Change the default password and JWT secret before production use.

## Main API Areas

- `POST /api/auth/register`
- `POST /api/auth/login`
- `GET /api/auth/me`
- `POST /api/auth/logout`
- `GET /api/users`
- `POST /api/users`
- `PATCH /api/users/:id`
- `GET /api/agent-accounts`
- `POST /api/agent-accounts`
- `PATCH /api/agent-accounts/:id`
- `POST /api/agent-accounts/:id/reset-password`
- `POST /api/token/exchange/task-runner`
- `GET /api/model-configs`
- `POST /api/model-configs`
- `PATCH /api/model-configs/:id`
- `DELETE /api/model-configs/:id`
- `GET /api/model-configs/settings`
- `PUT /api/model-configs/settings`

## Validation Notes

Validated in this workspace:

- `cargo check --manifest-path user_service/backend/Cargo.toml`
- `npm.cmd run build` in `user_service/frontend`

Repository-wide Rust validation is still blocked by an existing missing external dependency under `C:\project\learn\memory_engine\sdk`.
