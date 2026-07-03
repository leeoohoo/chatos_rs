# Chatos RS

Cross-platform installation guide: [INSTALL_GUIDE.zh-CN.md](./INSTALL_GUIDE.zh-CN.md)

## Positioning
`Chatos RS` is an AI platform for engineering and collaborative workflows.  
It combines conversational interaction, tool execution, and long-term memory in one system so AI can run as a reliable ongoing worker, not only a one-shot chatbot.

## What Problems It Solves
Typical issues in engineering-grade chat AI systems:
- Context is trapped in a single session and is hard to carry forward.
- Token cost keeps increasing as history grows.
- Tool integration is fragmented and expensive to maintain.
- Engineering workflows are hard to operate when tool execution is not observable.

`Chatos RS` addresses these with a layered architecture:
main chat service + external memory platform integration + MCP-style tool orchestration.

## Core Advantages
1. Long-term memory by design
- Supports session summaries, rollups, and memory consolidation to preserve facts, decisions, and TODOs across sessions.

2. Controlled context cost
- Uses layered summarization and scheduled jobs to compress context while maintaining continuity.

3. Tool-friendly orchestration
- Built for tool calls and MCP-like workflows, making it practical for real engineering pipelines.

4. Scalable architecture
- Frontend, backend, and external memory platform are decoupled and can scale independently.

5. Operable engineering workflows
- Keeps tool calls, task execution, and memory-backed context visible and maintainable.

## Architecture Layers
- `chat_app/`: frontend interaction layer
- `chat_app_server_rs/`: main orchestration backend (sessions, messages, tools, streaming)
- `user_service/`: unified identity service for real users, agent accounts, and Task Runner delegation tokens
- `task_runner_service/`: task execution and agent runtime service
- `memory_engine/`: independent long-term memory microservice
- `official_website_service/`: official product website microservice

## Quick Start
Run from repository root:

```bash
./restart_services.sh restart
```

Run the full local stack:

```bash
./restart_all_services.sh restart
```

Unified root tasks:

```bash
make help
make build
make test
make smoke
```

`make smoke` runs repo governance checks plus lightweight cross-subproject probes.
It also validates root startup script syntax and the Git-relevant large-file policy.

Shared local configuration entrypoint:

- repository root [`.env.example`](./.env.example)
- `./restart_services.sh` loads root `.env` before applying defaults
- if `chat_app_server_rs/.env` exists, backend-specific keys there still override the shared root defaults

Useful commands:

```bash
./restart_services.sh status
./restart_services.sh stop
./restart_all_services.sh status
./restart_all_services.sh stop
```

Run the official website microservice:

```bash
make restart-official-website
```

Official website URLs:

- backend/static site: `http://localhost:39250`
- frontend dev server: `http://localhost:39251`

The default `OFFICIAL_WEBSITE_MODE=dev` runs both Rust and Vite. For static
production-style serving, run `make build-official-website` first, then start
with `OFFICIAL_WEBSITE_MODE=prod make restart-official-website`.
Use `make restart-official-website-prod` for an isolated production-style port
and `make docker-build-official-website` to build the Docker image.

The website backend also exposes `GET /api/site/status` for local core
microservice health shown on the website page.
`robots.txt` and `sitemap.xml` are generated from
`OFFICIAL_WEBSITE_PUBLIC_BASE_URL`; use `make smoke-official-website-live` to
probe a running website. For public deployments, set
`OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS=false` to disable internal service probes.

To include it in the full local stack, set `START_OFFICIAL_WEBSITE=1` before
running `./restart_all_services.sh restart`.

## WSL Rust Dev Flow
If Windows Smart App Control / Code Integrity blocks `cargo run` or `cargo test`,
use the WSL-based Rust dev flow instead of executing Rust artifacts directly on Windows.

Bootstrap WSL once:

```powershell
wsl.exe --install -d Ubuntu
make bootstrap-wsl
```

Run ChatOS inside WSL from Windows:

```powershell
make restart-wsl
make status-wsl
make stop-wsl
```

Run the full stack inside WSL from Windows:

```powershell
make restart-all-wsl
make status-all-wsl
make stop-all-wsl
```

Run only `user_service` inside WSL from Windows:

```powershell
make restart-user-service-wsl
make status-user-service-wsl
make stop-user-service-wsl
```

Optional root `.env` keys for the WSL helper:

- `WSL_DEV_DISTRO`
- `WSL_CARGO_TARGET_DIR`
- `WSL_RUNTIME_DIR`
- `WSL_USER_SERVICE_RUNTIME_DIR`
- `WSL_TASK_RUNNER_RUNTIME_DIR`
- `WSL_MEMORY_ENGINE_RUNTIME_DIR`

Unified user-service local run:

```bash
bash user_service/restart_services.sh restart
make status-user-service
make stop-user-service
```

If root `.env` keeps `START_USER_SERVICE=1` and
`CHATOS_USER_SERVICE_BASE_URL=http://127.0.0.1:39190`, then
`./restart_services.sh restart` will also start the local `user_service`.

Current limitation:
- On the current Windows machine, Smart App Control / Code Integrity can block Rust-generated EXE/DLL artifacts during `cargo run` or `cargo test`; use the WSL flow above to avoid that execution-policy issue.

Default runtime logs:
- `/tmp/chatos_rs_dev_<repo-hash>/backend.log`
- `/tmp/chatos_rs_dev_<repo-hash>/frontend.log`
- `/tmp/chatos_rs_user_service_<repo-hash>/backend.log`
- `/tmp/chatos_rs_user_service_<repo-hash>/frontend.log`

## Development Plans Archive
Historical plans/assessments/contracts may live in root-level historical files or local `docs/plans/` archives.

## Per-Project READMEs
- [chat_app English](./chat_app/README.en.md)
- [chat_app 中文](./chat_app/README.zh-CN.md)
- [chat_app_server_rs English](./chat_app_server_rs/README.en.md)
- [chat_app_server_rs 中文](./chat_app_server_rs/README.zh-CN.md)
- [db_connection_hub backend](./db_connection_hub/backend/README.md)
- [db_connection_hub frontend](./db_connection_hub/frontend/README.md)
- [official website](./official_website_service/README.md)

## Unified User Service Docs
- [user_service](./user_service/README.md)
- [unified user-service status](./CHATOS_UNIFIED_USER_SERVICE_STATUS_20260619.md)
- [user_service local runbook](./USER_SERVICE_LOCAL_RUNBOOK_20260619.md)
- [WSL Rust dev flow](./WSL_RUST_DEV_FLOW_20260619.md)

## License
This project is source-available under the [PolyForm Noncommercial License 1.0.0](./LICENSE).
Commercial use is not permitted without a separate written license from the copyright holder.
Check first-party source headers with `python3 scripts/apply_license_headers.py`.
Add missing headers with `python3 scripts/apply_license_headers.py --write`.
