# Local Agent Host protocol v50 fixtures

This directory is the client-neutral wire-contract baseline for Local Agent Host protocol 50.

- Rust owns the serialized protocol types.
- Swift and TypeScript clients must decode the same JSON fixtures.
- Fixtures use the real snake_case wire keys; client-language naming conventions are adapter details.
- A protocol change must update the versioned directory and all three contract tests together.

Protocol 50 adds owner- and plugin-scoped, cursor-paginated Hook invocation diagnostics. The diagnostic contract intentionally excludes Hook payloads, subjects, stdout/stderr contents and hashes, and model context.
