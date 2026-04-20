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

## Memory Backend (`memory_server/backend`)

| Path prefix | Primary owner scope | Contract file |
| --- | --- | --- |
| `/api/memory/v1/auth/*` | Memory auth & user management | `memory_server.openapi.yaml` |
| `/api/memory/v1/sessions*`, `/api/memory/v1/messages*` | Memory chat/session lifecycle | `memory_server.openapi.yaml` |
| `/api/memory/v1/agents*` | Memory agent runtime domain | `memory_server.openapi.yaml` |
| `/api/memory/v1/contacts*`, `/api/memory/v1/projects*`, `/api/memory/v1/project-agent-links/*` | Contact/project memory topology | `memory_server.openapi.yaml` |
| `/api/memory/v1/configs/*` | Model/job configuration | `memory_server.openapi.yaml` |
| `/api/memory/v1/jobs/*` | Background memory jobs | `memory_server.openapi.yaml` |
| `/api/memory/v1/skills*` | Skills & plugin management | `memory_server.openapi.yaml` |

## Review Rule

For PRs that add, remove, or behavior-change API paths:

1. Update OpenAPI contract in the mapped file.
2. Update baseline snapshots when endpoint topology changes.
3. Request review from mapped owner scope.
4. Keep `.github/CODEOWNERS.openapi` aligned (validated by `scripts/check_openapi_ownership_map_consistency.sh`).
5. Keep `.github/api-contract/ownership/manifest.yaml` aligned as ownership source-of-truth.
