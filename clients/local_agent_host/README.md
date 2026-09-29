# ChatOS Local Agent Host

`chatos_local_agent_host` is the client-owned durable execution host introduced for the 3.0.8 local-runtime migration.

The source, protocol, application state machine, storage ports, SQLite adapter, Profiles, and native IPC adapters are owned under this directory. See [ARCHITECTURE.md](./ARCHITECTURE.md) for the layer boundaries and the enforced requirement that `chatos/`, `mcp_management_service/`, and `task_runner_service/` can ultimately be removed.

The current milestone provides:

- a versioned cross-platform protocol;
- client-owned SQLite storage and schema migration;
- idempotent mutating commands;
- Run claim leases and compare-and-swap transitions;
- a shared `LocalAgentProfile` registry and one-step Host scheduler;
- a durable Tool Invocation Ledger with per-call claims and results;
- a durable Task DAG repository with atomic, idempotent graph creation;
- query-derived Task Graph lifecycle status and per-Task Run history;
- local `create_task` and `create_tasks_with_prerequisites` tool executors;
- a `chatos_ai_runtime` single-step Profile adapter and conservative tool-safety policy;
- a local tool registry and one-invocation Tool Scheduler;
- an event-driven Host Coordinator that drains model and tool work to quiescence;
- monotonic, replayable event cursors;
- conservative crash recovery to `needs_review`;
- length-prefixed JSON over Unix sockets, Windows named pipes, or stdio.

The standalone binary does not yet wire authenticated production control-plane feeds or platform tool adapters, so it does not replace the production conversation path by itself. Main Chat and Task Runner planners resolve each model request without persisting credentials, while native tool workers consume the durable Tool Invocation Ledger.

The library-level schedulers execute one registered Profile step or one tool invocation at a time and commit through the same durable protocol. `LocalAgentHostCoordinator` combines them into a long-running loop: successful IPC commands wake it immediately, each wake drains model and tool work until no durable progress remains, and `retry_scheduled` Runs arm a timer for the earliest persisted retry deadline. Idle operation does not poll. All stdio, Unix socket and Windows named-pipe transports accept the same `HostRequestHandler`, so an embedded client can route IPC directly through the Coordinator. The standalone binary still uses an empty runtime because production Profile and platform-tool registration belongs to the native client integration.

`ChatosAiRuntimeStepExecutor` executes a prepared `chatos_ai_runtime` request exactly once. `DurableAiProfile` converts final responses, continuations, retries and tool calls into durable Host outcomes. Tools are considered side-effecting unless an explicit `ToolSafetyPolicy` classifies them as read-only.

`ControlPlaneLocalAiStepPlanner` provides the shared Main Chat and Task Runner planning boundary. It resolves the exact model and capability revisions for each step, keeps the resolved API key in transient runtime objects only, and reconstructs Responses history from the durable checkpoint plus tool or user continuation payload.

`LocalAgentHostAssembly` is the native-client composition root. Given one initialized Runtime plus concrete model, capability and platform-tool adapters, it registers both production Profile keys and constructs the model Scheduler, tool Scheduler and Coordinator with one shared safety policy.

The in-process assembly reserves `create_task` and `create_tasks_with_prerequisites` and routes them directly into the local Task DAG repository. IDs are derived from the durable tool invocation, so replay returns the original graph. New Tasks inherit the parent Run's exact model and capability revisions; an unresolved model switch and dependencies on Tasks outside the submitted graph are rejected instead of storing an ambiguous execution snapshot.

`LocalControlPlaneSnapshot` is the default in-process Resolver backing for native integration. Authenticated configuration code publishes exact model and capability revisions into it; model credentials remain in non-serializable process memory and can be removed or atomically replaced without altering durable Runs.

Native clients may choose `LocalAgentHostAssembly::with_external_tool_worker`. In that mode Swift or C# claims and commits platform tools through IPC, while Rust still owns model scheduling plus the two Task creation tools and wakes immediately after each native tool receipt. This keeps platform permissions and UI-bound tools in the native process without duplicating the Agent loop.

Protocol v8 retains the optional `include_tool_names` and `exclude_tool_names` Tool claim filters. The Assembly's Rust worker includes only the two reserved Task tools, and Coordinator IPC automatically excludes them from native claims. Explicit overlapping filters are rejected.

`LocalToolScheduler` claims one persisted invocation, routes it through `LocalToolRegistry`, and commits the result. Executor infrastructure errors on side-effecting calls become `needs_review`; read-only executor errors become ordinary failed tool results that the next model step can inspect.

## Run locally

On macOS or Linux:

```bash
cargo run -p chatos_local_agent_host -- \
  --database /absolute/path/to/local-agent.sqlite \
  --socket /absolute/path/to/local-agent.sock
```

For an embedded child process on any platform:

```bash
cargo run -p chatos_local_agent_host -- \
  --database /absolute/path/to/local-agent.sqlite \
  --stdio
```

On Windows, use a per-user pipe in the reserved namespace:

```powershell
cargo run -p chatos_local_agent_host -- `
  --database C:\absolute\path\local-agent.sqlite `
  --pipe \\.\pipe\chatos-local-agent-USER-SCOPE
```

The native client must place the socket or pipe in a user-only security boundary. Unix socket permissions are set to `0600`. Windows pipes reject remote clients and use a protected DACL that grants access only to LocalSystem, administrators, and the creating object owner.

## Frame format

Each request and response is:

```text
4-byte unsigned big-endian JSON length
JSON payload bytes
```

Frames are limited to 1 MiB. The connection can carry multiple request/response pairs.

Example health request:

```json
{
  "protocol_version": 8,
  "command_id": "health-019",
  "command": {
    "type": "health"
  }
}
```

Mutating commands use `command_id` as an idempotency key. Reusing a key with different command content returns `command_mismatch`.

## Supported commands

- `health`
- `create_run`
- `get_run`
- `claim_next_run`
- `commit_step`
- `claim_next_tool`
- `commit_tool`
- `resume_run`
- `cancel_run`
- `list_events`
- `wait_events`
- `create_task_graph`
- `get_task_graph`
- `get_task_runs`
- `cancel_task`
- `retry_task`
- `restart_task`

`create_task_graph` validates the complete acyclic graph and writes it in one SQLite transaction. Tasks without prerequisites start as `ready`; dependent tasks start as `pending`. Graph creation uses the same command receipt mechanism as Run mutations, so an identical `command_id` replay returns the original graph and a mismatched replay is rejected.

The model scheduler atomically materializes each `ready` Task as one Run. A terminal Run updates its owning Task in the same transaction: success unlocks newly satisfied dependents, while failure or cancellation transitively marks downstream Tasks `blocked`. `cancel_task` also terminates an active Run atomically. `retry_task` requires the exact failed/cancelled Task version and satisfied prerequisites, then resets derived downstream blocks before scheduling a fresh Run.

`get_task_graph` derives its aggregate status from the authoritative Task rows instead of maintaining a second mutable status column. A graph is `pending` before work starts, `running` once any Task has started while runnable work remains, `succeeded` when every Task succeeds, `failed` when terminal execution contains a failure, and `cancelled` when cancellation ends the graph without a failure. Retrying a Task immediately re-derives the graph status from the reset DAG.

`get_task_runs` returns up to 100 Runs owned by one Task, newest first. This includes superseded failed or cancelled Runs after retry, allowing Task Inspector to show the complete local execution history. Unknown Task IDs return `not_found`.

`restart_task` is an explicit, version-protected force restart for a running or terminal Task. It atomically cancels any active Run for that Task and all running transitive descendants, invalidates their outstanding tool claims, resets the target to `ready`, and rewinds every transitive descendant to `pending`. Previous Runs remain in local history. Tasks that have never started and blocked Tasks cannot be force-restarted, and the target's prerequisites must still be satisfied.

A successful claim moves one runnable Run to `model_running`, increments its iteration and version, and returns a random claim token. `commit_step` requires the exact token and version. If the Host stops before commit, an expired `model_running` claim is moved to `needs_review`; it is never silently replayed.

`wait_for_tool` persists every call before execution. Read-only calls whose lease expires return to `pending`; side-effecting calls with an unknown result move both the invocation and Run to `needs_review`. A completed batch moves the Run to `continuation_ready` only after every call has a durable result.

Each Run also stores an opaque Profile checkpoint plus a one-shot continuation payload. Tool results and explicit `resume_run` input survive Host restarts and are cleared only after the next claimed model step commits.

Transient model retries persist `model_attempt` in the Run itself and do not consume a pending tool/user continuation. A restarted Host therefore continues with the exact request inputs and next attempt, without resetting the provider retry budget.

`wait_events` performs a bounded long poll (at most 60 seconds) from a durable event cursor. External native Tool Workers wait for `tool_batch_requested`, then use `claim_next_tool` and `commit_tool`; timeout responses preserve the supplied cursor and contain an empty event page.

## Verification

```bash
cargo test \
  -p chatos_local_agent_protocol \
  -p chatos_client_storage \
  -p chatos_local_agent_runtime \
  -p chatos_local_agent_host

cargo clippy \
  -p chatos_local_agent_protocol \
  -p chatos_client_storage \
  -p chatos_local_agent_runtime \
  -p chatos_local_agent_host \
  --all-targets -- -D warnings
```
