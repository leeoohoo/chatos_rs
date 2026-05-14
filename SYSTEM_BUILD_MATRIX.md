# Chatos RS System Build Matrix

This file is the root build/test/smoke contract for the repository in its current multi-subproject form.

## Subprojects

| Subproject | Role | Language | Primary Local Run Command | Default Local Port | Covered By Root `make build` | Covered By Root `make smoke` | Covered By Root `make test` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `chat_app/` | Main user-facing frontend | TypeScript / React | `npm run dev` | `8088` when started via root script | Yes | Yes | Yes |
| `chat_app_server_rs/` | Main orchestration backend | Rust | `cargo run --bin chat_app_server_rs` | `3997` | Yes | Yes | Yes |
| `openai-codex-gateway/` | OpenAI-compatible gateway | Python | `python server.py --host 127.0.0.1 --port 8089` | `8089` | Yes | Yes | Yes |
| `db_connection_hub/backend/` | Database connection backend | Rust | `cargo run` | `8099` | Yes | Yes | Yes |
| `db_connection_hub/frontend/` | Database connection frontend | TypeScript / React | `npm run dev` | `5174` | Yes | Yes | Yes |

## Root Task Contract

Use these commands from repository root:

```bash
make help
make build
make test
make smoke
make restart
make status
make stop
```

### Meaning

- `make build`
  - Builds the main backend, main frontend, gateway, and db-connection-hub subprojects.
- `make test`
  - Runs repo-level smoke gates plus subproject verification commands.
- `make smoke`
  - Runs fast repository governance checks plus lightweight subproject probes.
  - Also validates root startup script syntax and the tracked-file large artifact policy.
- `make restart`
  - Starts the main Chatos RS frontend/backend pair managed by `restart_services.sh`.

## Shared Local Configuration

- Repository-root `.env.example` is the shared local configuration template for startup scripts.
- Root `restart_services.sh` loads repository-root `.env` before applying built-in defaults.
- `db_connection_hub/restart_services.sh` loads repository-root `.env` first, then `db_connection_hub/.env` if present.
- If `chat_app_server_rs/.env` exists, backend-only keys there still override the shared root defaults during root startup.

## Runtime Boundaries

### `chat_app`

- Talks to `chat_app_server_rs` APIs during the main user flow.
- Is part of the default root startup path.

### `chat_app_server_rs`

- Serves the main API surface for sessions, chat runtime, tools, files, git, terminals, and related workflows.
- Integrates with the memory engine platform.

### `openai-codex-gateway`

- Exposes OpenAI-compatible endpoints for external clients.
- Is not part of the default `restart_services.sh` startup flow today.
- Current smoke probe: `python server.py --help`

### `db_connection_hub`

- Forms a separate database-tooling subsystem inside the same repository.
- Is covered by CI and root build/test contracts, but is not launched by the main root startup flow today.
- Current smoke probes:
  - backend: `cargo check`
  - frontend: `npm run type-check`

## CI Mapping

Current `.github/workflows/ci.yml` covers:

- API surface / OpenAPI governance
- `chat_app_server_rs`
- `chat_app`
- `openai-codex-gateway`
- `db_connection_hub/backend`
- `db_connection_hub/frontend`

## Known Current Gaps

- The main root startup script does not launch `openai-codex-gateway`.
- The main root startup script does not launch `db_connection_hub`.
- The deferred absolute-path SDK dependency in `chat_app_server_rs` remains a separate follow-up item and is intentionally outside this document’s cleanup scope.

## Update Rule

If you change root tasks, CI coverage, startup orchestration, or default ports, update this file together with:

- `Makefile`
- `README.md`
- `README.en.md`
- `README.zh-CN.md`
