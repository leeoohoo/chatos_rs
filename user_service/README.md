# User Service

`user_service` is the server-side identity and model-configuration service.

It owns:

- human user registration, login, sessions, and administration;
- user-owned model providers and concrete model configurations;
- the model settings used by the local Main Chat and Task Runner profiles;
- signed internal model-runtime lookup for ChatOS and Memory Engine;
- Harness account provisioning.

Conversation and task Agent loops do not run in this service. The retired
server Task Runner token exchange, agent execution identities, model catalog,
and downstream synchronization endpoints have been physically removed.

## Stack

- Backend: Rust + Axum + MongoDB + JWT
- Administration UI: the repository-level React admin console

## Model configuration

`user_service` is the source of truth for user-owned model credentials and
capabilities. Creating a model configuration may omit `model`; the service
then queries the provider-compatible `/models` endpoint and creates one
concrete configuration per returned model ID.

Memory Engine resolves its summary model through a signed, user-scoped internal
runtime lookup. Native clients obtain the current user's model catalog through
the ChatOS API and freeze the selected model revision into each local Agent run.

Required internal-call settings include:

- `USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET`
- `CHATOS_USER_SERVICE_INTERNAL_API_SECRET`
- `USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS`

## Harness provisioning

When enabled, User Service provisions the user's Harness identity and project
access. Configure:

- `USER_SERVICE_HARNESS_PROVISIONING_ENABLED=true`
- `USER_SERVICE_HARNESS_BASE_URL=http://harness:3000`

## Local development

From the repository root, start the server dependencies required by the native
local-agent client:

```bash
./scripts/local-client-stack.sh up
./scripts/local-client-stack.sh status
```

The User Service public API listens on its configured
`USER_SERVICE_HOST:USER_SERVICE_PORT`; internal model-runtime lookup uses the
separate mTLS listener configured by the local stack.
