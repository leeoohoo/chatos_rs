# Local Agent Host architecture

`clients/local_agent_host` is a self-contained client product, not an adapter over the server Task Runner.

## Layers

```text
local_agent_host/
├── layers/
│   ├── interface/                 IPC commands, results, and stable DTOs
│   ├── ports/                     storage interfaces used by the application core
│   ├── application/               durable Run/Task state machine
│   ├── profiles/                  Main Chat and Task Runner business profiles
│   └── infrastructure/database/   SQLite implementation of the storage ports
└── src/
    ├── application/               schedulers, coordinator, assembly, local Task tools
    ├── infrastructure/            database/control-plane composition adapters
    ├── interface/                 stdio, Unix socket, and Windows named-pipe IPC
    ├── lib.rs                     public composition root
    └── main.rs                    standalone process entry point
```

The application runtime depends on `ports` and `interface`; it does not depend on the SQLite implementation. The Host composition root selects SQLite and wires it into the runtime.

## Server-removal boundary

The completed client localization must allow these directories to be physically deleted:

- `chatos/`
- `mcp_management_service/`
- `task_runner_service/`

Local Agent Host code must not import packages from those directories, directly or transitively. Memory, configuration/model control plane, authentication required by those retained control planes, and Plugin publication/Marketplace remain server-owned.

Run the enforced dependency audit with:

```bash
python3 scripts/check_local_agent_host_dependency_boundary.py
```

The production Rust verification entry point runs the same audit before compiling services.
