# Local Connector Implementation Plan

## 1. Goal

Local Connector is a user-authorized local execution runtime for ChatOS.

The cloud service remains responsible for identity, chat orchestration, task scheduling, model calls, memory, audit indexing, and UI state. The Local Connector runs on the user's machine and performs local development actions only inside explicitly authorized workspaces.

The product goal is to recover the usefulness of a local-first AI coding assistant without turning the cloud service into an unrestricted remote-control channel.

## 2. Core Principles

1. The connector always initiates the outbound connection to the cloud.
2. The cloud never receives raw, long-lived local machine credentials.
3. Every local action is bound to a short-lived capability token.
4. Every capability is scoped to a user, device, workspace, project, run, tool, and TTL.
5. Unknown shell commands are not automatically trusted.
6. High-impact effects are gated by local policy and user confirmation.
7. File writes prefer patch and diff workflows over direct mutation.
8. Docker is treated as a privileged local capability, not as a normal shell command.
9. The local user can pause, disconnect, revoke, or inspect the connector at any time.
10. The connector must fail closed when policy, identity, scope, or audit checks are incomplete.

## 3. Target Architecture

```text
ChatOS Web App
  -> chat_app_server_rs
    -> task_runner_service
      -> execution target router
        -> cloud sandbox runtime
        -> remote SSH runner
        -> local connector runtime

Local Connector
  -> outbound secure websocket or HTTP/2 stream
  -> local policy engine
  -> workspace registry
  -> structured tool executor
  -> shell executor with confirmation gates
  -> Docker policy adapter
  -> audit and redaction pipeline
```

The upper layers should not need to know whether execution happens in a cloud sandbox, a user-owned remote runner, or a local connector. They should call a common execution runtime interface.

## 4. Execution Target Model

Introduce an execution target abstraction:

```text
ExecutionTarget
  id
  type: cloud_sandbox | remote_ssh | local_connector
  owner_user_id
  device_id
  display_name
  status
  capabilities
  workspace_scopes
  created_at
  last_seen_at
```

For Local Connector, one physical device can expose multiple workspaces, but each workspace must be registered explicitly.

```text
LocalWorkspaceScope
  workspace_id
  project_id
  local_root
  allowed_tools
  allowed_ports
  docker_policy_profile
  confirmation_profile
```

Cloud APIs should use `workspace_id` and relative paths. The connector maps them to local absolute paths internally.

## 5. Pairing And Trust

Pairing flow:

1. User opens ChatOS and chooses "Connect local machine".
2. Cloud creates a short-lived pairing code.
3. User starts the Local Connector and enters the code or scans a QR code.
4. Connector authenticates the user and device.
5. Cloud issues a device-bound refresh credential stored only by the connector.
6. Connector registers device metadata and supported capabilities.
7. User grants workspace scopes from the local machine.

Security requirements:

1. Pairing code TTL should be short, for example 5 minutes.
2. Device credentials must be revocable from the cloud UI.
3. Connector should use OS secure storage when available.
4. Device identity should include a locally generated key pair.
5. Runtime capability tokens should be short-lived and audience-bound.

## 6. Data Plane

The connector should maintain an outbound connection:

```text
GET /api/local-connectors/connect
Authorization: Bearer <device_token>
```

Recommended transport:

1. WebSocket for MVP.
2. HTTP/2 or QUIC stream later if multiplexing and backpressure become important.

Message model:

```json
{
  "type": "tool_request",
  "request_id": "req_...",
  "run_id": "run_...",
  "workspace_id": "ws_...",
  "capability_token": "cap_...",
  "tool": "docker_compose_up",
  "args": {
    "compose_file": "docker-compose.yml",
    "detach": true
  }
}
```

Connector response:

```json
{
  "type": "tool_response",
  "request_id": "req_...",
  "ok": true,
  "result": {
    "summary": "Started 3 services",
    "ports": [3997, 8088, 27018]
  },
  "audit_ref": "audit_..."
}
```

The connector must support request cancellation and streaming output for long-running commands.

## 7. Capability Tokens

Capability tokens must be issued per operation or per short run window.

Claims:

```json
{
  "sub": "user_...",
  "aud": "local_connector:device_...",
  "device_id": "device_...",
  "workspace_id": "ws_...",
  "project_id": "project_...",
  "run_id": "run_...",
  "scopes": [
    "fs.read",
    "fs.write.patch",
    "git.status",
    "terminal.exec",
    "docker.compose.up"
  ],
  "path_prefixes": ["."],
  "expires_at": "..."
}
```

The connector must validate the token locally before executing anything. The cloud's request is only a proposal until local policy accepts it.

## 8. Tool Model

Prefer structured tools over free-form shell.

Initial tools:

1. `environment_probe`
2. `fs_list`
3. `fs_read`
4. `fs_search`
5. `fs_write_patch`
6. `git_status`
7. `git_diff`
8. `git_branch`
9. `git_commit`
10. `run_project_command`
11. `docker_info`
12. `docker_compose_config`
13. `docker_compose_up`
14. `docker_compose_down`
15. `service_port_status`

Free-form shell should exist only as an advanced tool:

```text
terminal_exec
```

Default policy for `terminal_exec`:

1. Read-only inspection commands can be auto-approved if they match an allowlist.
2. Unknown commands require local confirmation.
3. Commands touching paths outside the workspace are blocked unless explicitly granted.
4. Commands requiring elevated privileges are blocked by default.

## 9. Risk Policy

Do not rely on a blacklist of dangerous commands. Use an effect-based policy.

Risk levels:

| Level | Examples | Default Policy |
| --- | --- | --- |
| L0 | Version checks, environment probe, `git status` | Auto-allow |
| L1 | Read project files, search project files | Auto-allow after workspace grant |
| L2 | Write project files via patch | Require diff approval or project grant |
| L3 | Run tests or build inside workspace | Allow by project policy |
| L4 | Start services, expose local ports, Docker Compose | Confirm impact |
| L5 | Install global packages, modify system config, access external paths | Confirm every time |
| L6 | Privileged Docker, host root mounts, secrets access, destructive bulk operations | Block by default |

Policy inputs:

1. Tool type.
2. Capability token scopes.
3. Workspace path scope.
4. Command parse result.
5. File system effects.
6. Network and port effects.
7. Docker effects.
8. User confirmation state.

## 10. Docker Policy

Docker must be a first-class policy domain.

Auto-allow candidates:

1. `docker version`
2. `docker info` with redaction
3. `docker ps`
4. `docker images`
5. `docker compose config`

Require confirmation:

1. `docker compose up`
2. `docker compose build`
3. Pulling images
4. Exposing ports
5. Creating named volumes
6. Using environment files

Block by default:

1. `--privileged`
2. `--pid=host`
3. `--ipc=host`
4. `--network=host`
5. `--device`
6. `--cap-add`
7. Mounting `/`
8. Mounting the user's home directory
9. Mounting `.ssh`
10. Mounting OS credential stores
11. Mounting `/var/run/docker.sock`
12. Compose services with the same effects above

Before running Docker Compose, the connector should parse the compose file and show a summary:

```text
Services: backend, frontend, mongo
Images to pull/build: ...
Ports: 3997, 8088, 27018
Mounts: ./data -> /data
Privileged features: none
Environment files: .env
```

The user can allow once, allow for this project, or deny.

## 11. Local Confirmation UX

The connector needs a visible local control surface.

Required controls:

1. Current connection status.
2. Active cloud account.
3. Authorized workspaces.
4. Running tasks and commands.
5. Pending approval prompts.
6. Pause all executions.
7. Disconnect device.
8. Revoke workspace authorization.
9. View recent audit log.

Approval prompt should show:

1. Who requested the action.
2. Which project and workspace are affected.
3. What tool will run.
4. What files, ports, Docker resources, or commands are involved.
5. Whether the action writes, deletes, starts services, or touches secrets.

## 12. Audit And Redaction

Audit record:

```text
audit_id
device_id
workspace_id
project_id
run_id
request_id
tool
risk_level
decision: allowed | denied | confirmed | blocked
started_at
finished_at
summary
redacted_args
redacted_output_preview
```

Redaction rules:

1. Never send raw local environment variables by default.
2. Redact token-like strings.
3. Redact private keys and certificate blocks.
4. Redact `.env` values unless explicitly approved.
5. Redact home directory paths in cloud-visible output when possible.

## 13. Integration With Current Services

### `task_runner_service`

Add execution target selection:

1. Resolve whether a run should use cloud sandbox, remote runner, or local connector.
2. Store selected target in `run.input_snapshot`.
3. Route MCP tools to the selected runtime.
4. Preserve existing sandbox output manifest flow for file changes.

### `sandbox_manager_service`

Keep the existing sandbox lease model for cloud sandboxes.

Add a sibling lease type or generalized runtime lease:

```text
runtime_lease
  runtime_type
  target_id
  workspace_id
  project_id
  run_id
  capability_scopes
  expires_at
```

### `chat_app_server_rs`

Add device and workspace management APIs:

1. Pair connector.
2. List devices.
3. Revoke device.
4. List connector workspaces.
5. Select execution target for a project or task.

### Frontend

Add UI surfaces:

1. Local Connector onboarding.
2. Device list.
3. Workspace authorization status.
4. Execution target selector.
5. Local approval state in task run details.

## 14. MVP Scope

MVP should avoid unrestricted shell.

MVP capabilities:

1. Pair and revoke connector.
2. Register a local workspace.
3. Probe local development environment.
4. Read/search project files.
5. Show Git status and diff.
6. Write patch with local confirmation.
7. Run project test/build commands with confirmation.
8. Run Docker Compose only through structured `docker_compose_*` tools.
9. Stream command output back to Task Runner.
10. Record audit logs locally and send redacted summaries to cloud.

MVP should not include:

1. Full system shell auto-execution.
2. Privileged Docker.
3. Host root mounts.
4. Access to SSH keys or OS credential stores.
5. Global package installation without explicit confirmation.

## 15. Phased Delivery

### Phase 1: Foundation

1. Execution target data model.
2. Device pairing and revocation.
3. Connector outbound websocket.
4. Capability token issuing and validation.
5. Local workspace registry.
6. Basic audit and redaction.

### Phase 2: Read And Diff

1. Environment probe.
2. File list/read/search.
3. Git status and diff.
4. Cloud UI for connected device and workspace status.

### Phase 3: Controlled Writes

1. Patch-only file writes.
2. Local diff approval.
3. Run output change manifest integration.
4. Task Runner result preview.

### Phase 4: Local Commands

1. Structured project command execution.
2. Test/build command profiles.
3. Port detection.
4. Streaming output and cancellation.

### Phase 5: Docker

1. Docker availability probe.
2. Compose parser and impact summary.
3. `docker_compose_config`.
4. Confirmed `docker_compose_up`.
5. Confirmed `docker_compose_down`.
6. Docker policy enforcement.

### Phase 6: Advanced Mode

1. Optional free-form shell.
2. Per-project allowlists.
3. Team policy profiles.
4. Enterprise audit export.
5. Admin-managed device policy.

## 16. Acceptance Criteria

1. A user can pair a local connector from the ChatOS UI.
2. A user can grant one local project workspace.
3. ChatOS can run a Task Runner job against that workspace.
4. The job can read files and inspect Git status without accessing paths outside the workspace.
5. File writes are shown as diffs and require local approval.
6. Docker Compose execution shows an impact summary before running.
7. Privileged Docker options are blocked by default.
8. User can pause or disconnect the connector during execution.
9. Revoked devices can no longer receive or execute requests.
10. Audit logs show what was requested, allowed, denied, or blocked.

## 17. Open Questions

1. Should the connector be bundled into a desktop app or shipped as a standalone CLI first?
2. Should device credentials be stored in OS secure storage only, or should encrypted file fallback be allowed?
3. Should teams be able to define organization-level connector policy profiles?
4. Should Docker Compose approvals be remembered per compose file hash?
5. Should local command profiles be configured by project files, cloud settings, or both?
6. Should free-form shell exist in MVP, or wait until policy telemetry is mature?
