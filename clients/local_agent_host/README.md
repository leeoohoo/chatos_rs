# ChatOS Local Agent Host

`chatos_local_agent_host` is the client-owned durable execution host introduced for the 3.0.8 local-runtime migration.

The current milestone provides:

- a versioned cross-platform protocol;
- client-owned SQLite storage and schema migration;
- idempotent mutating commands;
- Run claim leases and compare-and-swap transitions;
- a shared `LocalAgentProfile` registry and one-step Host scheduler;
- a durable Tool Invocation Ledger with per-call claims and results;
- a `chatos_ai_runtime` single-step Profile adapter and conservative tool-safety policy;
- a local tool registry and one-invocation Tool Scheduler;
- an event-driven Host Coordinator that drains model and tool work to quiescence;
- monotonic, replayable event cursors;
- conservative crash recovery to `needs_review`;
- length-prefixed JSON over Unix sockets, Windows named pipes, or stdio.

The standalone binary does not yet ship a control-plane model planner or platform tool adapters, so it does not replace the production conversation path by itself. Main Chat and Task Runner planners resolve each model request without persisting credentials, while native tool workers consume the durable Tool Invocation Ledger.

The library-level schedulers execute one registered Profile step or one tool invocation at a time and commit through the same durable protocol. `LocalAgentHostCoordinator` combines them into a long-running loop: successful IPC commands wake it immediately, each wake drains model and tool work until no durable progress remains, and `retry_scheduled` Runs arm a timer for the earliest persisted retry deadline. Idle operation does not poll. All stdio, Unix socket and Windows named-pipe transports accept the same `HostRequestHandler`, so an embedded client can route IPC directly through the Coordinator. The standalone binary still uses an empty runtime because production Profile and platform-tool registration belongs to the native client integration.

`ChatosAiRuntimeStepExecutor` executes a prepared `chatos_ai_runtime` request exactly once. `DurableAiProfile` converts final responses, continuations, retries and tool calls into durable Host outcomes. Tools are considered side-effecting unless an explicit `ToolSafetyPolicy` classifies them as read-only.

`ControlPlaneLocalAiStepPlanner` provides the shared Main Chat and Task Runner planning boundary. It resolves the exact model and capability revisions for each step, keeps the resolved API key in transient runtime objects only, and reconstructs Responses history from the durable checkpoint plus tool or user continuation payload.

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
  "protocol_version": 2,
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

A successful claim moves one runnable Run to `model_running`, increments its iteration and version, and returns a random claim token. `commit_step` requires the exact token and version. If the Host stops before commit, an expired `model_running` claim is moved to `needs_review`; it is never silently replayed.

`wait_for_tool` persists every call before execution. Read-only calls whose lease expires return to `pending`; side-effecting calls with an unknown result move both the invocation and Run to `needs_review`. A completed batch moves the Run to `continuation_ready` only after every call has a durable result.

Each Run also stores an opaque Profile checkpoint plus a one-shot continuation payload. Tool results and explicit `resume_run` input survive Host restarts and are cleared only after the next claimed model step commits.

Transient model retries persist `model_attempt` in the Run itself and do not consume a pending tool/user continuation. A restarted Host therefore continues with the exact request inputs and next attempt, without resetting the provider retry budget.

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
