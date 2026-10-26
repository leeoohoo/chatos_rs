# Local Agent Host protocol v49 fixtures

This directory is the client-neutral wire-contract baseline for Local Agent Host protocol 49.

- Rust owns the serialized protocol types.
- Swift and TypeScript clients must decode the same JSON fixtures.
- Fixtures use the real snake_case wire keys; client-language naming conventions are adapter details.
- A protocol change must update the versioned directory and all three contract tests together.

The first frozen slice is the lifecycle health handshake used before either client exposes Host-backed features.
