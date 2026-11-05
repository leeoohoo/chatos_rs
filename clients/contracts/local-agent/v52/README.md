# Local Agent Host protocol v52 fixtures

This directory is the client-neutral wire-contract baseline for Local Agent Host protocol 52.

- Rust owns the serialized protocol types.
- Swift and TypeScript clients must decode the same JSON fixtures.
- Fixtures use the real snake_case wire keys; client-language naming conventions are adapter details.
- A protocol change must update the versioned directory and all three contract tests together.

Protocol 52 adds the owner-scoped `resolve_agent_team_hook_context` command so Agent teams can run the installed Hook bundles against the team's real project workspace. The result contains only bounded additional-context strings; it does not grant tools, approvals, Todo scope, or acceptance authority. Protocol 51's bounded latest-Run task-graph presentation remains unchanged.
