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
    ├── infrastructure/            database/control-plane/local MCP adapters
    ├── interface/                 stdio, Unix socket, and Windows named-pipe IPC
    ├── lib.rs                     public composition root
    └── main.rs                    standalone process entry point
```

The application runtime depends on `ports` and `interface`; it does not depend on the SQLite implementation. The Host composition root selects SQLite and wires it into the runtime.

Conversation, Turn, Message and Run state follows the same dependency direction:

```text
IPC interface → application runtime → LocalConversationStore port → local SQLite adapter
```

The local database is the authoritative Main Chat fact source. `start_conversation_turn` commits the queued Run, running Turn, user Message, conversation version change, durable event and idempotency receipt as one transaction. Terminal Run transitions reconcile the owning Turn and, on success only, append the assistant Message inside that Run transaction. The runtime does not import or call the old `chatos` conversation backend, and no historical server data migration is part of this boundary.

The local MCP adapter owns Plugin process startup, MCP session initialization, tool discovery and invocation. Marketplace metadata and signed artifacts may still come from the retained Plugin control plane, but no tool execution request is routed through `mcp_management_service`.

Installed Plugin/MCP snapshots are application data behind a storage port and are implemented by the local SQLite adapter. Only credential-store references are durable; native Keychain/Credential Manager adapters resolve secret values into the child-process environment at launch time.

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
