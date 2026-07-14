# ChatOS Skill Bundle Creator

Use this Skill when the user asks to create or update a ChatOS Local Connector Skill Bundle.

The bundle must remain local-first: scripts, files, tools, credentials, browser actions, and desktop actions execute through Local Connector. Plugin Management stores only metadata, policy, version, hash, availability, and user preference.

Required workflow:

1. Inspect the target repository and reuse its existing conventions.
2. Create a versioned immutable bundle manifest with a stable bundle ID.
3. Declare only the permissions and dependencies the implementation actually needs.
4. Never expose arbitrary shell execution as a Skill operation.
5. Add deterministic checksums and verify the bundle before reporting it available.
6. Mark incomplete adapters unavailable instead of simulating success.
7. Add tests for manifest validation, dependency detection, and execution boundaries.

The current product only accepts Admin-provided internal bundles. User installation is reserved for a future release.
