# User Service

`user_service` is the unified identity service for this repository.

It owns:

- Chat OS human users
- User and device authentication
- User-owned model configs shared by local clients and memory_engine

## Stack

- Backend: Rust + Axum + PostgreSQL + JWT
- Frontend: React + Vite + Ant Design

## Current Integration Status

The service is now integrated into the repository flow:

- Local clients call `user_service` directly for registration, login, session restore, and model configuration
- Local clients use the signed-in user's identity and fetch that user's model runtime config through the authenticated control plane
- `user_service` exposes signed internal model runtime endpoints to approved callers and retains Memory Engine model settings
- There is no Task Runner token audience, token exchange, internal caller, or task-model catalog API

## Unified Model Configs

- `user_service` is now the source of truth for user-owned model configs.
- A real user can keep provider credentials here for local execution and memory processing.
- Creating a model config may omit `model`; `user_service` will call the provider-compatible `/models` endpoint and create one concrete config per returned model id.
- Local clients and `memory_engine` use those concrete model names from the shared configs.
- `memory_summary_model_config_id` must point to a config with a concrete `model`.
- Memory summary thinking level is stored in model settings; local execution usage and thinking level are stored per model config.

## Downstream Sync Environment

If you want model config changes in `user_service` to sync into the other services, configure these environment variables:

- `MEMORY_ENGINE_BASE_URL=http://127.0.0.1:7081/api/memory-engine/v1`
- `USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET=...`
- `USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS=5000`

## Harness Provisioning

In the Docker stack, Harness runs as the `harness` service and `user_service` points to it with:

- `HARNESS_PROVISIONING_ENABLED=true`
- `HARNESS_BASE_URL=http://harness:3000`

Harness provisioning does not follow HTTP redirects, including redirects on the same origin. Configure the Harness base URL to serve the API directly; a 3xx response is treated as a failed Harness request without forwarding passwords or tokens or triggering the existing-account login fallback.

User summaries expose any recorded Harness provisioning error as the fixed message `harness provisioning failed`; an absent error remains `null`. Historical error text may contain credentials echoed by downstream services, so it is never copied into user-list or user-detail summaries. Status, attempt count and timestamps remain available. This response projection does not rewrite existing provisioning records or change retry behavior.

Harness source lives in a separate ignored Git checkout at repository root `harness/`; the Chat OS parent repository does not track it.

Important behavior:

- `model` is optional on create. If omitted, `user_service` imports provider models from `/models`.
- `model` is required on each concrete stored config and cannot be cleared on update.
- Downstream sync problems are returned as `sync_warnings` on the save response.
- Docker deployment projects `USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET` from Configuration Center for signed service calls.

## Docker Stack

From the repository root:

```bash
docker/deploy.sh up
```

Default URLs:

- Frontend: `http://127.0.0.1:39191`
- Backend: `http://127.0.0.1:39190`

## Email Registration

Public registration now uses email as the login username and requires both an invite code and an email verification code.

- Super admins generate invite codes from the User Service users page.
- Invite code plaintext is shown only once when generated; the database stores only a hash.
- Registration email codes are 6 digits, expire after 10 minutes by default, and are rate-limited per email address.
- Do not commit SMTP authorization codes. Publish `USER_SERVICE_SMTP_PASSWORD` through Configuration Center.

Required SMTP environment variables:

- `USER_SERVICE_SMTP_HOST=smtp.qq.com`
- `USER_SERVICE_SMTP_PORT=587`
- `USER_SERVICE_SMTP_USERNAME=...`
- `USER_SERVICE_SMTP_PASSWORD=...`
- `USER_SERVICE_EMAIL_FROM=...`
- `USER_SERVICE_EMAIL_FROM_NAME=Chat OS`

## Backend-Only Development

```bash
cd user_service/backend
cargo run
```

## Unified Admin Console Development

```bash
cd admin_console
npm install
npm run dev
```

The user and model pages live in the unified admin console. Its `/api/admin/user-service` prefix is proxied through APISIX to this backend.

## Default Admin

On first startup the service creates a default `super_admin` account:

- username: `admin`
- password: `admin123456`

Change the default password and JWT secret before production use.

## Main API Areas

- `POST /api/auth/register`
- `POST /api/auth/register/send-code`
- `POST /api/auth/login`
- `GET /api/auth/me`
- `POST /api/auth/logout`
- `GET /api/invite-codes`
- `POST /api/invite-codes`
- `POST /api/invite-codes/:id/revoke`
- `GET /api/users`
- `POST /api/users`
- `PATCH /api/users/:id`
- `GET /api/model-configs`
- `POST /api/model-configs`
- `PATCH /api/model-configs/:id`
- `DELETE /api/model-configs/:id`
- `GET /api/model-configs/settings`
- `PUT /api/model-configs/settings`

## Validation Notes

Recommended checks:

- `cd user_service/backend && cargo test`
- `cd admin_console && npm run type-check`
- `cd admin_console && npm run build`
