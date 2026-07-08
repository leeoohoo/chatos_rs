# User Service

`user_service` is the unified identity service for this repository.

It owns:

- ChatOS human users
- Task Runner agent accounts
- The ownership relation from a real user to that user's agent accounts
- Task Runner delegation token exchange
- User-owned model configs shared by ChatOS, Task Runner, and memory_engine

## Stack

- Backend: Rust + Axum + MongoDB + JWT
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
- Creating a model config may omit `model`; `user_service` will call the provider-compatible `/models` endpoint and create one concrete config per returned model id.
- ChatOS, `task_runner_service`, and `memory_engine` use those concrete model names from the shared configs.
- `task_runner_service` and `memory_engine` receive synced runnable configs when downstream sync is configured.
- `memory_summary_model_config_id` must point to a config with a concrete `model`.
- Memory summary thinking level is stored in model settings; Task Runner usage and thinking level are stored per model config.

## Downstream Sync Environment

If you want model config changes in `user_service` to sync into the other services, configure these environment variables:

- `MEMORY_ENGINE_BASE_URL=http://127.0.0.1:7081/api/memory-engine/v1`
- `MEMORY_ENGINE_OPERATOR_TOKEN=...`
- `TASK_RUNNER_BASE_URL=http://127.0.0.1:39090`
- `TASK_RUNNER_CHATOS_CALLBACK_SECRET=...`
- `USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS=5000`

## Harness Provisioning

In the Docker stack, Harness runs as the `harness` service and `user_service` points to it with:

- `HARNESS_PROVISIONING_ENABLED=true`
- `HARNESS_BASE_URL=http://harness:3000`

Harness source lives in a separate ignored Git checkout at repository root `harness/`; the Chatos parent repository does not track it.

Important behavior:

- `model` is optional on create. If omitted, `user_service` imports provider models from `/models`.
- `model` is required on each concrete stored config and cannot be cleared on update.
- Downstream sync problems are returned as `sync_warnings` on the save response.
- Docker deployment defaults `MEMORY_ENGINE_OPERATOR_TOKEN` in `docker/.env.example` for local development.

## Docker Stack

From the repository root:

```bash
docker/deploy.sh up
```

Default URLs:

- Frontend: `http://127.0.0.1:39191`
- Backend: `http://127.0.0.1:39190`

## Backend-Only Development

```bash
cd user_service/backend
cargo run
```

## Frontend-Only Development

```bash
cd user_service/frontend
npm install
npm run dev
```

The frontend uses `/api` proxying to the backend during local development.

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

Recommended checks:

- `cd user_service/backend && cargo test`
- `cd user_service/frontend && npm run type-check`
- `cd user_service/frontend && npm run build`
