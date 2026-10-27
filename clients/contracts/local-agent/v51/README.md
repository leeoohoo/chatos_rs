# Local Agent Host protocol v51 fixtures

This directory is the client-neutral wire-contract baseline for Local Agent Host protocol 51.

- Rust owns the serialized protocol types.
- Swift and TypeScript clients must decode the same JSON fixtures.
- Fixtures use the real snake_case wire keys; client-language naming conventions are adapter details.
- A protocol change must update the versioned directory and all three contract tests together.

Protocol 51 embeds a bounded latest-Run presentation in each message-task-graph node. A graph refresh therefore has one Host request and one batched SQLite lookup instead of one request and query per task. The projection deliberately excludes Run input, checkpoint, continuation, Tool batch, claim and complete terminal payloads so a large graph cannot monopolize the multiplexed IPC channel. Terminal display strings are capped at 4,096 characters and inline acceptance data at 16 KiB; full Run state remains available from the single-Run detail command. An explicit `null` is authoritative for tasks without Runs, while clients may treat a missing field as a legacy response during rolling upgrades.
