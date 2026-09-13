# Local Agent protocol v24 fixtures

These files are the shared Rust, Swift, and C# contract fixtures for Task retry,
exact Run-bound tool approval, version-bound Run control, Task snapshots,
source-turn Task Graph projections, historical Run detail, Run-bound Memory
Sync status, owner-scoped revision-bound Project CRUD, Clipboard metadata, and
Media history metadata with project, status and integrity fields, plus
owner-scoped Story Project/Agent Run/Media Batch state, and typed Notepad
folder/note records, owner-scoped revision-bound Client Settings, typed Terminal
History, and durable native Approval History records stored by the selected
Client Storage Provider. Binary payload bytes never enter this protocol.

Protocol v24 is the only supported native-client contract. Older fixture sets
are intentionally removed rather than retained as compatibility inputs.
