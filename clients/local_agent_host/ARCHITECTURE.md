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

The local database is the authoritative Main Chat fact source. `start_conversation_turn` commits the queued Run, running Turn, user Message, conversation version change, durable event and idempotency receipt as one transaction. `guide_conversation_turn` persists an in-flight user instruction in a dedicated queue and invalidates an active model claim, preventing its late response from winning the race; pending guidance is attached exactly once to the next claim and can coexist with durable tool results. `resume_conversation_turn` atomically validates Conversation/Run versions, appends the user's continuation Message and attachment references, and makes the Run claimable again. `cancel_conversation_turn` validates Turn ownership and atomically cancels the Run, invalidates open tool claims, closes the Turn and advances the Conversation version. Terminal Run transitions reconcile the owning Turn and, on success only, append the assistant Message inside that Run transaction. History reads use an exclusive Message ordinal cursor and a bounded page; related Turns and attachments are selected from the same local database connection so native clients never need an unbounded transcript response. The runtime does not import or call the old `chatos` conversation backend, and no historical server data migration is part of this boundary.

Message attachments follow a reference-only boundary. The database owns immutable attachment metadata, ordering, byte size and SHA-256, while the file body remains in the client filesystem behind an opaque authorization reference. The Planner may disclose the bounded manifest to the model, but only a capability-approved native tool may resolve the reference and read content. Large file bytes and Base64 never enter IPC, SQLite, Run input checkpoints, logs or the retained server control planes.

Task completion also stays inside the local database boundary. A Task Graph sourced from a local `conversation_turn` writes each distinct terminal generation back as a structured assistant Message. The terminal Task transition, writeback ledger row, Conversation version update and durable event share one SQLite transaction. Retrying or restarting a terminal graph produces another generation; replaying the same terminal state does not duplicate the Message. Graphs from non-Conversation sources do not use this projection.

The local MCP adapter owns Plugin process startup, MCP session initialization, tool discovery and invocation. Marketplace metadata and signed artifacts may still come from the retained Plugin control plane, but no tool execution request is routed through `mcp_management_service`.

Installed Plugin/MCP snapshots are application data behind a storage port and are implemented by the local SQLite adapter. Only credential-store references are durable; native Keychain/Credential Manager adapters resolve secret values into the child-process environment at launch time.

Control-plane revisions follow the same dependency direction through `LocalCapabilitySnapshotStore` and `LocalModelConfigSnapshotStore`. SQLite v11 stores immutable, bounded instructions, prefixed input items and tool definitions keyed by Profile plus revision. SQLite v12 stores the bounded model/provider request configuration and a native credential reference keyed by model config plus revision. The model DTO structurally has no API-key field and excludes request-specific cache keys, response IDs and working directories.

After restart, the control-plane resolver loads both revision types from SQLite. `LocalModelCredentialResolver` resolves the persisted reference from Keychain/Credential Manager only when a model step is prepared, then combines the secret with a process-local `ContextualTurnRunner`. The credential value and runner are never serialized or written to SQLite.

IPC v15 exposes idempotent publication and exact-revision reads for both snapshot types. Their wire DTOs live in the interface layer, the application runtime routes validated commands into storage ports, and SQLite implements those ports with the common command-receipt transaction. The IPC DTOs contain credential references only; credential values are not accepted by these commands.

The standalone composition root now wires SQLite, both control-plane stores, a process-local AI runner, both Profiles, the reserved Rust Task tools, model/tool Schedulers and the Coordinator. Platform tools stay in the native process and use the external Tool Worker IPC path. For child-process deployments, the native launcher resolves a model secret from Keychain/Credential Manager and injects it into a dedicated environment variable referenced as `env:NAME`; the standalone Host never accepts a secret CLI argument. Embedded clients may provide a native credential resolver instead.

Retained Memory is isolated in an infrastructure adapter and is enabled only by an explicit base URL plus source ID. Profiles own the business mapping from a Run to tenant/thread/turn and stable record IDs; the infrastructure adapter owns HTTP authentication and the concrete Memory SDK client. This keeps the direction `Profile policy → AI runtime Memory port → retained Memory adapter`. Main Chat scopes a Memory thread to the local Conversation, Task Runner scopes it to the local Task, and record routing derives the tenant from bounded Profile metadata so one Host can serve multiple users without a fixed-tenant writer. Access tokens and internal signing secrets are child-process environment inputs only and never cross IPC or SQLite.

Memory writes follow `AI runtime record port → LocalMemoryOutboxStore → SQLite v13`. Enqueue is immutable and idempotent by source plus stable record ID. The Coordinator owns a lease/CAS sync worker that sends one pending record at a time, marks success, and persists bounded failure diagnostics plus an exponential retry deadline; restarts reclaim expired leases. Network failure is therefore sync state, not Run failure. Compose follows `MemoryContextComposer → retained Memory → LocalMemoryContextCacheStore`: SQLite v14 stores a successful response under the exact serialized scope. A remote failure uses that snapshot, and a first-run miss produces an empty context instead of stopping local execution.

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
