# ChatOS Local Agent Host

`chatos_local_agent_host` is the client-owned durable execution host introduced for the 3.0.8 local-runtime migration.

The source, protocol, application state machine, storage ports, SQLite adapter, Profiles, and native IPC adapters are owned under this directory. See [ARCHITECTURE.md](./ARCHITECTURE.md) for the layer boundaries that keep the already-removed `chatos/`, `mcp_management_service/`, and `task_runner_service/` roots from returning as dependencies.

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
- a client-owned MCP stdio process/session adapter with local tool discovery and execution;
- durable installed-Plugin/MCP snapshots in the client-owned SQLite database;
- durable non-secret model and capability control-plane revisions;
- a client-owned Conversation/Turn/Message fact source with atomic Main Chat Run creation and control;
- versioned per-Conversation model, reasoning and remote-connection runtime settings;
- durable local Message attachment references without storing file bodies in SQLite;
- idempotent Task Graph terminal summaries written back to their source Conversation;
- an event-driven Host Coordinator that drains model and tool work to quiescence;
- monotonic, replayable event cursors;
- conservative crash recovery to `needs_review`;
- length-prefixed JSON over Unix sockets, Windows named pipes, or stdio.

The standalone binary now assembles both production Profiles, the model Scheduler, the two Rust Task tools, the Tool Scheduler and the event-driven Coordinator. Authenticated native code publishes non-secret control-plane revisions through IPC and claims platform tools from the durable Tool Invocation Ledger. The macOS and Windows clients now package this binary, launch it over protected stdio after authentication, verify IPC v28 health, restart it when the account changes, and stop it on sign-out or application exit. Both lifecycle adapters also expose serialized typed request/response clients over the same framed process, validate response identity and protocol version, map structured Host errors, and reject native requests whose owner differs from the active account. Windows production data-source switching remains required before all old runtime paths can be retired.

The macOS connector additionally exposes a native Conversation gateway for create, detail, stable list pagination, bounded history pagination, and versioned start/guidance/resume/cancel Turn mutations. Its DTOs mirror the Rust protocol explicitly, including opaque JSON content and metadata, attachment authority references, immutable control-plane revisions and optimistic versions. Main Chat now uses this gateway for commands and history, while an account-scoped local polling stream observes Conversation versions and drives UI reconciliation without a server WebSocket. Attachments are copied into a client-owned `0600` vault and only opaque authority tokens cross IPC. Integration tests execute the flow against the actual Host transport, including send, SQLite history and local realtime reconciliation.

Main Chat runtime settings now use the same local Host instead of `/conversations/*/runtime-settings` or `/ai-model-configs` during a Conversation. IPC v27 and SQLite v21 persist an owner-scoped exact model revision, reasoning state, selected thinking level and opaque remote-connection identifier with optimistic versioning. The application layer verifies that the selected model revision exists in the owner-scoped control-plane cache before storage accepts it. macOS builds its model menu from the authenticated bootstrap snapshot, resolves that durable selection before every new Turn, and the Planner applies the frozen reasoning level to that Run's transient model request without mutating a shared model snapshot.

IPC v28 and SQLite v22 attach an optional typed Contact or Project resource binding to each local Conversation. The binding is owner-scoped and unique in SQLite, is returned by detail and paged index reads, and is accepted only as part of idempotent Conversation creation. Native clients can now build their workspace Conversation relation locally without identifier conventions or the old `/contacts` and `/conversations` endpoints.

The macOS pet overlay now derives chat and task activity from owner-scoped local Run pages and refreshes through Host `wait_events`. It no longer opens the server realtime WebSocket or reads the server `/pet-activities` inbox. Terminal success/cancellation cards expire locally, while failures and review states remain visible; dismissing a projected card changes UI state only and never deletes durable Run facts.

The macOS Task Graph/Inspector data source now discovers graphs by their local `conversation_turn` source, loads DAG/task/run/event detail from the Host, and performs versioned local task cancellation and retry. Conversation history marks turns as graph candidates and lets an empty local result hide the inspector, so no server message-to-task proxy is required. IPC v26 adds an optional bounded retry instruction; SQLite appends it to the Task's durable input in the same retry transaction, and the next materialized Run receives it as model input instead of losing it in UI-only state.

A native control-plane client publishes and reads immutable model and capability snapshots without allowing an API-key field in its model. Model snapshots carry only an explicit child-environment credential reference; capability snapshots carry instructions and JSON tool/input definitions. Real-Host integration tests verify durable round trips for both snapshot families.

The macOS connector owns a dedicated per-user/model Keychain credential store and Windows owns the equivalent Password Vault/Credential Manager adapter. Both lifecycles can restart the Host with a tightly validated `CHATOS_LOCAL_AGENT_MODEL_*` environment map and the current user's `CHATOS_MEMORY_ACCESS_TOKEN`; these values are merged only into the child environment, are never retained by lifecycle configuration, and remain absent from process arguments, IPC, and SQLite. Both launchers derive the public `/api/memory` endpoint from deployment configuration and use the fixed `local_agent` source. The macOS real-Host test verifies that an environment-injected restart preserves the durable non-secret control plane.

After the independent configuration connector is authenticated, the macOS bootstrap refreshes enabled model configuration, saves each returned secret into Keychain, immediately reloads it for child-only injection, restarts the account-scoped Host, and publishes deterministic non-secret model revisions plus the built-in `main_chat` capability revision. Secret material is excluded from revision hashes and discarded with the temporary environment map after launch. Bootstrap failure is surfaced to the application instead of falling back to a remote execution runtime.

The library-level schedulers execute one registered Profile step or one tool invocation at a time and commit through the same durable protocol. `LocalAgentHostCoordinator` combines them into a long-running loop: successful IPC commands wake it immediately, each wake drains model and tool work until no durable progress remains, and `retry_scheduled` Runs arm a timer for the earliest persisted retry deadline. Idle operation does not poll. All stdio, Unix socket and Windows named-pipe transports use the Coordinator as their `HostRequestHandler`. If the execution loop fails, the standalone process now terminates instead of continuing to accept work it cannot run.

`ChatosAiRuntimeStepExecutor` executes a prepared `chatos_ai_runtime` request exactly once. `DurableAiProfile` converts final responses, continuations, retries and tool calls into durable Host outcomes. Tools are considered side-effecting unless an explicit `ToolSafetyPolicy` classifies them as read-only.

`ControlPlaneLocalAiStepPlanner` provides the shared Main Chat and durable Task Execution planning boundary. It resolves the exact model and capability revisions for each step, keeps the resolved API key in transient runtime objects only, and reconstructs Responses history from the durable checkpoint plus tool or user continuation payload. When retained Memory is explicitly enabled, the Planner also assigns a tenant/source/thread scope, stable user/assistant/tool record IDs, and profile-specific record metadata. Main Chat uses its local Conversation as the Memory thread; Task Execution uses its local Task. Initial user records are omitted on model retry or checkpoint continuation, while completed native tool results are persisted before the next model step.

`LocalAgentHostAssembly` is the native-client composition root. Given one initialized Runtime plus concrete model, capability and platform-tool adapters, it registers both production Profile keys and constructs the model Scheduler, tool Scheduler and Coordinator with one shared safety policy.

The in-process assembly reserves `create_task` and `create_tasks_with_prerequisites` and routes them directly into the local Task DAG repository. IDs are derived from the durable tool invocation, so replay returns the original graph. New Tasks inherit the parent Run's exact model and capability revisions; an unresolved model switch and dependencies on Tasks outside the submitted graph are rejected instead of storing an ambiguous execution snapshot.

`LocalControlPlaneSnapshot` is the default Resolver backing for native integration. Authenticated configuration code publishes exact model and capability revisions into it. Non-secret revisions are immutable and retained through the `LocalModelConfigSnapshotStore` and `LocalCapabilitySnapshotStore` ports in client SQLite, so a restarted Host can resolve the configuration frozen by an existing Run. The model snapshot stores a native `credential_ref`, never the credential value. At execution time `LocalModelCredentialResolver` reads that reference from Keychain/Credential Manager and constructs a transient `ModelRuntimeConfig` around the process-local runner. API keys never enter the Run or control-plane database; request-specific cache keys, response IDs, and working directories are also excluded from the durable configuration DTO.

The standalone process uses `ChildEnvironmentModelCredentialResolver`. A persisted reference such as `env:CHATOS_MODEL_KEY` reads only that variable from the Host process environment when a model step starts. The native launcher should resolve Keychain/Credential Manager itself and set the variable only in the child environment. Secret values are never accepted as CLI arguments or snapshot fields. Embedded integrations can supply their own `LocalModelCredentialResolver` directly.

Retained Memory is an explicit, optional standalone adapter. `--memory-base-url` and `--memory-source-id` must be supplied together; without them the Host performs no Memory network requests. The authenticated user's Bearer credential is read only from `CHATOS_MEMORY_ACCESS_TOKEN` in the child environment. Internal-service credentials are intentionally unsupported by the client Host. Credentials are not accepted by CLI or IPC and are not serialized to SQLite. One process-local client safely routes records for multiple signed-in users by the tenant ID generated by the Profile, while the configured source ID remains fixed for the Host instance. SQLite v13 stores immutable Memory records in a local outbox before model execution continues. A lease/CAS sync worker retries network failures with bounded exponential backoff and never turns a retained-service outage into a failed local model step. SQLite v14 caches successful compose responses by the exact serialized Memory scope; compose failures use that cache, or an empty context on the first offline request, so local execution remains available.

IPC v16 adds `get_memory_sync_status`. Native UI supplies the signed-in tenant and configured source and receives only aggregate pending/syncing/retry/synced counts, retry and age timestamps, plus the most recent bounded error. Memory record payloads and credentials are never returned by this diagnostic command.

IPC v17 adds `list_runs` for native recovery and inspector screens. Queries are always scoped to one owner account, may select active, terminal, or all Runs, and use the stable `(updated_at_unix_ms, run_id)` descending cursor. SQLite v15 adds the matching owner/update index; pages never depend on an in-memory scheduler view.

IPC v18 and SQLite v16 add the durable tool-approval gate. Side-effecting model tool calls are inserted as approval-pending and are excluded from every worker claim until an owner-scoped `decide_tool_approval` approves them. Native UI discovers them through `list_pending_tool_approvals`. Rejection is persisted as a deterministic failed tool result and lets the batch continue back to the model; approval and rejection are CAS/version protected and idempotent. The two built-in local Task creation tools are explicitly approval-exempt because they only create durable local work, while their unknown-result recovery remains conservative.

IPC v19 adds `list_task_graphs` as the account-scoped Task Inspector index. Active, terminal, and complete views use a stable descending `(updated_at_unix_ms, graph_id)` cursor and return bounded summaries instead of complete DAGs; `get_task_graph` loads one selected DAG. Task Graph detail, Task Run history, cancel, retry, and restart commands now all require the owner account and treat a cross-account identifier as not found.

IPC v20 makes `list_conversations` a stable account-scoped page instead of a one-shot bounded list. Pages use `(updated_at_unix_ms DESC, conversation_id ASC)` and expose an explicit paired cursor. Conversation detail, history, Turn start, guidance, resume, and cancel commands now require the owner account; a Conversation identifier from another signed-in account is treated as not found before any Run or Message state can change.

IPC v21 and SQLite v17 make local Plugin installation discovery account-scoped and cursor-paged. List responses are deliberately safe summaries: executable paths, process arguments, working directories and credential references are available only from an owner-scoped detail read. Installation ownership is immutable after creation, and update, detail and remove operations cannot address another account's installation by identifier.

IPC v22 closes the remaining account boundary on generic Run inspection. `get_run`, `resume_run`, `cancel_run`, `list_events` and `wait_events` all require the owner account; SQLite owner predicates protect detail reads and mutations, while event queries join through the owning Run. Trusted in-process schedulers retain a separate non-IPC lookup for claim reconciliation, so native callers cannot bypass the account-scoped contract.

IPC v23 binds execution workers to the signed-in account. Model claims, Tool claims and ready-Task materialization all carry the active owner into their SQLite selection predicates, so a Host launched for one account cannot execute queued work left by another local account. The optional Memory outbox worker uses the same owner as its tenant filter for record claims, expired-lease recovery and retry timers; it cannot upload or mutate another account's pending records with the active account's credential. SQLite v18 adds the matching tenant-first runnable index. The standalone process therefore requires `--owner-user-id`; native clients restart it with the newly authenticated owner when accounts change, while account data remains in the shared client-owned database.

IPC v24 and SQLite v19 make the authenticated owner the first key of both control-plane snapshot types. Model configs, capability policies, process-local caches and credential resolution are selected by `owner_user_id` plus their existing reference/revision, and the Planner always supplies the owner frozen into the claimed Run. Two accounts may therefore publish identical references and revisions without collision, while a cross-account lookup is not found. Ownerless v11/v12 control-plane cache rows are intentionally discarded during v19 migration and are republished after authentication; no secret value is added to IPC or SQLite.

IPC v25 binds the complete standalone IPC surface to the `--owner-user-id` selected at process launch. Every stateful command exposes one account scope, including Memory tenant requests and nested snapshot/install specifications; the Coordinator rejects a mismatch before database dispatch or long polling. `commit_step` and `commit_tool` now carry the owner explicitly, and the application/storage layers verify it before accepting a claim token. A stale worker from the previously signed-in account therefore cannot finish work through a Host restarted for another account.

IPC v26 extends versioned Task retry with an optional 8,000-character user instruction. The storage adapter appends each instruction to the Task input's `retry_instructions` array, rechecks the total input bound, resets the Task and its blocked descendants, and records the idempotent command receipt in one transaction. Task-to-Run materialization then copies that augmented input into the new Run.

SQLite v20 extends the active-owner boundary to lifecycle maintenance. Startup recovery and the implicit recovery performed before model/Tool claims only inspect expired leases owned by the account passed to `LocalAgentRuntime::initialize`; another account's unknown work is left untouched. The Coordinator's Run retry timer also selects the earliest deadline for its active owner only, so an overdue retry belonging to a signed-out account cannot create a zero-delay wake loop. Owner-first runnable and expired-Tool indexes keep these scoped paths bounded in the shared client database.

File-backed storage performs `PRAGMA quick_check(1)` before and after schema migration. When an existing database needs an upgrade, the adapter first creates a transactionally consistent SQLite snapshot beside the primary file using the name pattern `<database>.pre-migration-v<from>-to-v<to>-<id>.backup.sqlite`, verifies that snapshot independently, and only then applies migrations. A failed or interrupted migration therefore leaves a verified pre-migration recovery point. On Unix, both the primary database and generated backups are forced to mode `0600`; Windows deployments rely on the native client's protected per-user data directory ACL. Backups are retained intentionally and native lifecycle code may remove them only after its release rollback window closes.

Native clients may choose `LocalAgentHostAssembly::with_external_tool_worker`. In that mode Swift or C# claims and commits platform tools through IPC, while Rust still owns model scheduling plus the two Task creation tools and wakes immediately after each native tool receipt. This keeps platform permissions and UI-bound tools in the native process without duplicating the Agent loop.

Protocol v15 retains the optional `include_tool_names` and `exclude_tool_names` Tool claim filters. The Assembly's Rust worker includes only the two reserved Task tools, and Coordinator IPC automatically excludes them from native claims. Explicit overlapping filters are rejected.

Protocol v15 also lets authenticated native-client configuration code publish and read immutable model and capability revisions through the same protected Host IPC connection. These commands contain only the non-secret DTOs defined by the interface layer; storage ports and SQLite remain below the application boundary. Command receipts make publication replay-safe and reject reuse of one command ID with different content.

Main Chat conversations are written to the local SQLite fact source rather than a remote conversation runtime. Starting a Turn atomically creates the Turn, its user Message, and one queued `main_chat` Run owned by that Turn. A conversation version compare-and-swap rejects stale composers, while a partial unique index permits only one active Turn per conversation. Successful Runs append a deterministic assistant Message and close the Turn in the same transaction; failed or cancelled Runs close the Turn without inventing an assistant response. Historical server conversations are intentionally not migrated.

Ask User continuation and user-initiated stop use Conversation-specific commands rather than bypassing the Conversation repository with generic Run mutations. `resume_conversation_turn` checks exact Conversation and Run versions, appends the reply Message and attachment references, advances the Run to `continuation_ready`, and increments the Conversation version in one transaction. `cancel_conversation_turn` verifies Turn ownership, cancels its Run, invalidates open tool invocations, closes the Turn, and increments the Conversation version atomically. Durable command receipts make both operations replay-safe.

Mid-turn guidance uses `guide_conversation_turn` and the SQLite v10 guidance queue. The user Message, attachment references, queue row, Run transition, Conversation version and event commit atomically. Guidance received during a model request invalidates that claim, so its late result cannot overwrite the new instruction; the scheduler treats this deliberate supersession as progress and claims the Run again. Guidance received while tools are outstanding remains queued and is merged with their durable results at the next model claim. Delivered guidance is retained for audit but attached to model input only once.

Conversation history clients use `get_conversation_history` instead of loading an unbounded transcript. The command returns at most 100 Messages plus only their related Turns and attachments. Pages are selected newest-first with an exclusive `before_ordinal` cursor, then returned in chronological order for direct UI merging; `next_before_ordinal` is present only when an older page exists.

User Messages can include up to 32 local attachment records. SQLite stores only display metadata, byte size, canonical SHA-256 and an opaque `authorized_local_ref` in the `local-attachment:<token>` namespace; raw paths, URLs, file bodies and unrestricted Base64 are rejected or excluded from database and IPC attachment fields. Turn creation commits attachment rows in the same transaction as the Message and Run. The Main Chat planner sends the model a bounded attachment manifest so a capability-approved native tool can resolve the opaque reference and verify its hash locally. Arbitrary attachment metadata is retained for the client UI but is not forwarded to the model.

Task Graphs created by a Main Chat Turn retain `conversation_turn` as their source. When the graph becomes terminal, SQLite appends one structured assistant Message containing the graph status and per-Task terminal outcome to that source Conversation in the same transaction as the final Task mutation. A terminal signature prevents duplicate delivery, while a later retry or restart creates a new generation and therefore a new summary. Pending Task cancellation follows the same path without requiring a Task Run. No server callback or `chatos/backend` message proxy participates.

Installed Plugin MCP configurations are created, queried, updated and removed through version-protected Host commands and stored by the Local Agent SQLite repository. Records freeze Plugin release/component revisions, executable location and the allowed tool set. Environment entries store native credential references only; `LocalPluginSecretResolver` resolves their values transiently from Keychain or Credential Manager immediately before local process launch.

`LocalToolScheduler` claims one persisted invocation, routes it through `LocalToolRegistry`, and commits the result. Executor infrastructure errors on side-effecting calls become `needs_review`; read-only executor errors become ordinary failed tool results that the next model step can inspect.

`LocalMcpStdioSession` launches installed Plugin MCP processes directly on the client, performs the MCP initialize handshake, follows paginated `tools/list`, prefixes tool names per local server, and registers `tools/call` executors in `LocalToolRegistry`. It never routes Plugin execution through a server. The default process environment is empty; native integration must explicitly provide the minimum environment required by the signed Plugin release. Transport loss or timeout invalidates the session so an unknown side-effect result cannot be silently replayed, while an MCP `isError` response is committed as a known failed tool result.

## Run locally

On macOS or Linux:

```bash
cargo run -p chatos_local_agent_host -- \
  --database /absolute/path/to/local-agent.sqlite \
  --owner-user-id authenticated-user-id \
  --memory-base-url https://memory.example.com \
  --memory-source-id local_agent \
  --read-only-tool read_file \
  --socket /absolute/path/to/local-agent.sock
```

The Memory flags are optional. A native launcher that enables them should inject credentials only into the child environment, for example `CHATOS_MEMORY_ACCESS_TOKEN`; never put a credential in process arguments. Outbox payloads and compose caches contain Memory data but never credentials.

For an embedded child process on any platform:

```bash
cargo run -p chatos_local_agent_host -- \
  --database /absolute/path/to/local-agent.sqlite \
  --owner-user-id authenticated-user-id \
  --stdio
```

On Windows, use a per-user pipe in the reserved namespace:

```powershell
cargo run -p chatos_local_agent_host -- `
  --database C:\absolute\path\local-agent.sqlite `
  --owner-user-id authenticated-user-id `
  --read-only-tool read_file `
  --pipe \\.\pipe\chatos-local-agent-USER-SCOPE
```

The native client must place the socket or pipe in a user-only security boundary. Unix socket permissions are set to `0600`. Windows pipes reject remote clients and use a protected DACL that grants access only to LocalSystem, administrators, and the creating object owner.

Repeat `--read-only-tool <name>` for native tools whose interrupted execution is safe to retry. Tools not listed remain conservatively side-effecting. This option carries tool names only; never place credentials on the command line.

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
  "protocol_version": 39,
  "command_id": "health-019",
  "command": {
    "type": "health"
  }
}
```

Mutating commands use `command_id` as an idempotency key. Reusing a key with different command content returns `command_mismatch`.

## Supported commands

- `health`
- `put_model_config_snapshot`
- `get_model_config_snapshot`
- `put_capability_policy_snapshot`
- `get_capability_policy_snapshot`
- `create_run`
- `get_run`
- `list_runs`
- `claim_next_run`
- `commit_step`
- `claim_next_tool`
- `renew_tool_claim`
- `commit_tool`
- `list_pending_tool_approvals`
- `decide_tool_approval`
- `resume_run`
- `cancel_run`
- `get_event_cursor`
- `list_events`
- `wait_events`
- `create_task_graph`
- `list_task_graphs`
- `get_task_graph`
- `get_task_runs`
- `cancel_task`
- `retry_task`
- `restart_task`
- `put_plugin_installation`
- `get_plugin_installation`
- `list_plugin_installations`
- `remove_plugin_installation`
- `create_conversation`
- `get_conversation`
- `get_conversation_history`
- `list_conversations`
- `get_conversation_runtime_settings`
- `put_conversation_runtime_settings`
- `start_conversation_turn`
- `guide_conversation_turn`
- `resume_conversation_turn`
- `cancel_conversation_turn`

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
