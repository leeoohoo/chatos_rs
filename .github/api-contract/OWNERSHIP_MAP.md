# API Path Ownership Map

This map defines who is responsible for endpoint changes and OpenAPI contract updates.

## Main Backend (`chat_app_server_rs`)

| Path prefix | Primary owner scope | Contract file |
| --- | --- | --- |
| `/api/auth/*` | Auth & user identity flows | `chat_app_server_rs.openapi.yaml` |
| `/api/sessions*`, `/api/messages*`, `/api/chat/*` | Chat runtime/session domain | `chat_app_server_rs.openapi.yaml` |
| `/api/agent_v2/*`, `/api/agent_v3/*`, `/api/memory-agents*` | Agent orchestration domain | `chat_app_server_rs.openapi.yaml` |
| `/api/projects*`, `/api/contacts*` | Project/contact collaboration domain | `chat_app_server_rs.openapi.yaml` |
| `/api/terminals*`, `/api/task-manager/*` | Terminal/task execution domain | `chat_app_server_rs.openapi.yaml` |
| `/api/system-context*`, `/api/ui-prompts/*` | System context & prompt UX domain | `chat_app_server_rs.openapi.yaml` |
| `/api/remote-connections*`, `/api/fs/*` | Remote I/O and file operations | `chat_app_server_rs.openapi.yaml` |

## Review Rule

For PRs that add, remove, or behavior-change API paths:

1. Update OpenAPI contract in the mapped file.
2. Update baseline snapshots when endpoint topology changes.
3. Request review from mapped owner scope.
4. Keep `.github/CODEOWNERS.openapi` aligned (validated by `scripts/check_openapi_ownership_map_consistency.sh`).
5. Keep `.github/api-contract/ownership/manifest.yaml` aligned as ownership source-of-truth.
