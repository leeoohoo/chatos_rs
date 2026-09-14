# Local Agent protocol v27 fixtures

These files are the shared Rust, Swift, and C# contract fixtures for Task retry,
exact Run-bound tool approval, version-bound Run control, Task snapshots,
source-turn Task Graph projections, historical Run detail, Run-bound Memory
Sync status, owner-scoped revision-bound Project CRUD, Clipboard metadata, and
Media history metadata with project, status and integrity fields, plus
owner-scoped Story Project/Agent Run/Media Batch state, and typed Notepad
folder/note records, owner-scoped revision-bound Client Settings, typed Terminal
History, and durable native Approval History records stored by the selected
Client Storage Provider. Binary payload bytes never enter this protocol.

Protocol v27 is the only supported native-client contract. Older fixture sets
are intentionally removed rather than retained as compatibility inputs.

v27 adds the typed `create_approval_review` command. It accepts only a model
configuration ID and a bounded frozen review request; provider endpoints and
credentials are not wire fields. It also retains account- and device-scoped
installed Plugin CRUD backed by the shared
`PluginStateRepository`. The installation payload is bounded structured JSON;
legacy connector `state.json` values are never migrated or accepted.
