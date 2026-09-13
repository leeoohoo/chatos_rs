# Shared client architecture

This directory owns every platform-independent part of the ChatOS desktop clients.

## Ownership

- `rust/chatos_local_agent_protocol`: the only Local Agent wire and state contract.
- `rust/chatos_client_storage`: the only structured client storage contract and its SQLite/PostgreSQL providers.
- `rust/chatos_local_agent_runtime`: the only durable Agent state machine, model step, context, memory synchronization, and tool transaction implementation.
- `rust/chatos_agent_profiles`: Main Chat, Task Runner, approval, story, and future business profiles.
- `rust/local_agent_host`: the long-lived local process, typed IPC, projections, lifecycle, and local plugin/MCP integration.
- `contracts`, `fixtures`, `conformance`, and `codegen`: generated Swift/C# contracts and the common tests that prevent native clients from drifting.

## Platform boundary

`clients/macos` and `clients/windows` may implement only native UI and operating-system adapters: process launch, Unix Socket/Named Pipe transport, Keychain/DPAPI, notifications, window lifecycle, and platform packaging.

They must not implement Agent loops, Task Graph projection, retry semantics, context compaction, Memory Engine composition, tool state reduction, SQLite/PostgreSQL business repositories, or fallback execution paths.

## Legacy dependency rule

Code under `clients/shared` may call retained server APIs through explicit client contracts. The server Cloud Agent, Task Runner Service, and all direct dependencies from `clients/shared` to top-level legacy execution crates have been removed; they must not be restored. New shared runtime, host, storage, protocol, projection, and provider code belongs under `clients/shared`, while platform directories contain only UI and operating-system adapters.
