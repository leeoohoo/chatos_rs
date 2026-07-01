# Chatos RS

Cross-platform installation guide: [INSTALL_GUIDE.zh-CN.md](./INSTALL_GUIDE.zh-CN.md)

Chatos RS is an AI platform for engineering workflows.
It combines conversational collaboration, tool orchestration, and long-term memory in one system.

Chatos RS 是一个面向工程协作场景的 AI 平台，统一了对话协作、工具编排和长期记忆能力。

## Why This System
- Keep context across sessions with memory and summarization.
- Reduce context cost with layered summaries and scheduled processing.
- Make tool execution observable and operable in chat workflows.
- Keep engineering workflows integrated through MCP-style tool orchestration.

## Architecture
- `chat_app/`: Frontend interaction layer
- `chat_app_server_rs/`: Main orchestration backend
- `user_service/`: Unified identity service for real users, agent accounts, and Task Runner delegation tokens
- `task_runner_service/`: Task execution and agent runtime service
- `memory_engine/`: Independent long-term memory microservice

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

## WSL Rust Dev Flow
If Windows Smart App Control / Code Integrity blocks `cargo run` or `cargo test`,
prefer the WSL-based Rust dev flow instead of running Rust binaries directly on Windows.

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

Default logs:
- `/tmp/chatos_rs_dev_<repo-hash>/backend.log`
- `/tmp/chatos_rs_dev_<repo-hash>/frontend.log`
- `/tmp/chatos_rs_user_service_<repo-hash>/backend.log`
- `/tmp/chatos_rs_user_service_<repo-hash>/frontend.log`

## Language Docs
- [中文](./README.zh-CN.md)
- [English](./README.en.md)

## Subproject READMEs
- [chat_app English](./chat_app/README.en.md)
- [chat_app 中文](./chat_app/README.zh-CN.md)
- [chat_app_server_rs English](./chat_app_server_rs/README.en.md)
- [chat_app_server_rs 中文](./chat_app_server_rs/README.zh-CN.md)
- [db_connection_hub backend](./db_connection_hub/backend/README.md)
- [db_connection_hub frontend](./db_connection_hub/frontend/README.md)

## Note
Development plan documents may live in root-level historical files or local `docs/plans/` archives.

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
