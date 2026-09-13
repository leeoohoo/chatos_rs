# Local Agent protocol v20 fixtures

These files are the shared Rust, Swift, and C# contract fixtures for Task retry,
exact Run-bound tool approval, version-bound Run control, Task snapshots,
source-turn Task Graph projections, historical Run detail, Run-bound Memory
Sync status, owner-scoped revision-bound Project CRUD, Clipboard metadata, and
Media history metadata with project, status and integrity fields, plus
owner-scoped Story Project/Agent Run/Media Batch state. Binary payload bytes
never enter this protocol.

Protocol v20 is the only supported native-client contract. Older fixture sets
are intentionally removed rather than retained as compatibility inputs.
