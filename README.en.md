# Chatos RS

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

## Quick Start
Run from repository root:

```bash
./restart_services.sh restart
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
```

Default runtime logs:
- `/tmp/chatos_rs_dev_<repo-hash>/backend.log`
- `/tmp/chatos_rs_dev_<repo-hash>/frontend.log`

## Development Plans Archive
Historical plans/assessments/contracts may live in root-level historical files or local `docs/plans/` archives.

## Per-Project READMEs
- [chat_app English](./chat_app/README.en.md)
- [chat_app 中文](./chat_app/README.zh-CN.md)
- [chat_app_server_rs English](./chat_app_server_rs/README.en.md)
- [chat_app_server_rs 中文](./chat_app_server_rs/README.zh-CN.md)
- [db_connection_hub backend](./db_connection_hub/backend/README.md)
- [db_connection_hub frontend](./db_connection_hub/frontend/README.md)

## License
This project is licensed under the [MIT License](./LICENSE).
