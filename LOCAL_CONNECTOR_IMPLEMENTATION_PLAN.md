# Local Connector 实施方案

## 1. 目标

Local Connector 是 ChatOS 的本地执行客户端。它运行在用户自己的电脑上，用 Rust + React 实现，让云端 ChatOS / Task Runner 能在用户授权的本地项目目录、终端环境和本地沙箱环境里工作。

本版方案先解决“路打通”：

1. ChatOS 添加项目和运行终端时，可以选择已配对的本地环境，并只看到用户授权的本地路径。
2. Task Runner 执行任务时，可以通过 Local Connector 暴露的 MCP 能力操作对应本地项目。
3. 用户开启本地沙箱配对后，Task Runner 使用的沙箱 relay facade 地址按用户映射到 Local Connector Service，再由 Service 通过本地主动建立的长连接转发到对应 Connector，由 Connector 在本机 Docker 内独立完成沙箱能力。
4. Local Connector 必须登录并绑定 ChatOS 用户，否则无法建立 user -> device -> workspace -> sandbox 的映射。

危险命令精细化识别、复杂命令策略和高风险操作审批先不作为 MVP 阻塞项；MVP 只做最小边界：用户登录、本地路径显式授权、workspace 内执行、可断开、可审计。

## 2. 当前代码可复用点

现有系统已经有几个适合接 Local Connector 的扩展点：

1. 项目模型：`chat_app_server_rs` 的 `Project.root_path` 来自 `project_management_service`，目前是普通路径字符串。Local Connector 需要把它扩展为可表示本地授权路径的 project binding。
2. 终端模型：主后端已有 `/api/terminals` 和 workspace 内目录限制。Local Connector 可以提供远端终端会话，ChatOS 侧只需要按项目 execution target 路由到本地终端。
3. Task Runner MCP：`TaskMcpConfig` 已有 `workspace_dir`、`default_remote_server_id`、`external_mcp_config_ids`。MVP 可以把 Local Connector 注册成 HTTP MCP，加入任务的 `external_mcp_config_ids`。
4. Task Runner 沙箱：`sandbox_runtime` 已经抽象出 sandbox-manager-compatible HTTP 协议。Local Connector 不能提供让云端直连的本机 endpoint；应由 Local Connector Service 提供兼容 relay facade，内部通过 Connector 的出站长连接转发到本机 Docker 沙箱实现。这个 facade 不是云端 Sandbox Manager，也不运行任何云端沙箱实例。
5. 用户身份：`user_service` 已有登录、JWT、`/api/auth/verify`、agent token 和 owner_user_id 语义。Local Connector 应复用这套身份，而不是自建账号系统。
6. 沙箱鉴权：Local Connector relay facade 使用服务间密钥、owner_user_id、device_id、workspace_id 做路由和归属校验；真正的本机 agent token 只在 Connector 和本机容器之间使用。

## 3. 总体架构

```text
ChatOS Web / Cloud Backend
  -> user_service
  -> chat_app_server_rs
  -> project_management_service
  -> task_runner_service
  -> local_connector_service
       - device registry
       - workspace grants
       - project bindings
       - sandbox pairings
       - connector relay
       - sandbox protocol facade

Local Connector Desktop App (Rust + React)
  -> React UI
  -> Rust core service
       - login and device key store
       - outbound cloud connection
       - local workspace registry
       - local terminal sessions
       - local MCP server
       - local sandbox protocol handler
       - audit log
```

连接方向保持“本地连云端”：

1. Local Connector 登录后主动连接云端。
2. 云端不直接扫描用户机器，也不保存本地绝对路径的可执行凭据。
3. 云端只保存用户授权后的 workspace metadata 和 opaque workspace_id。
4. 真正的本地绝对路径只在 Connector 本机使用；云端展示时可以显示脱敏路径或用户设置的别名。

## 4. Local Connector 客户端形态

推荐使用 Tauri 形态：

1. UI：React + TypeScript，负责登录、配对、授权目录、状态展示、任务确认、日志查看。
2. Core：Rust，负责本地服务、MCP、终端、文件系统、Docker / sandbox adapter、系统安全存储。
3. 本地 HTTP 监听：仅监听 `127.0.0.1`，供本机 UI 调用。
4. 云端连接：Rust core 主动建立 WebSocket 或长轮询连接到 `local_connector_service`，所有云端到本地的调用都复用这条出站通道。

本地客户端主要页面：

1. 登录页：使用 ChatOS 账号登录，保存 refresh/device credential。
2. 设备状态页：显示当前连接的云端、用户、device_id、连接状态。
3. 工作区授权页：选择本地目录，设置别名、允许项目绑定、是否允许终端、是否允许 MCP。
4. 项目绑定页：把本地授权目录绑定到 ChatOS project。
5. 沙箱配对页：开启/关闭本地 sandbox handler，展示配对状态、Relay 状态和本地后端状态。
6. 活动日志页：展示云端请求、本地执行、失败原因。

## 5. 身份与配对

Local Connector 必须登录，原因是所有映射都依赖用户身份：

```text
user_id
  -> device_id
    -> local_workspace_id
      -> chatos_project_id
      -> task_runner_workspace_binding
      -> sandbox_pairing
```

登录流程：

1. Connector UI 调用 `user_service /api/auth/login`。
2. Connector 调用独立 `local_connector_service` 的 `POST /api/local-connectors/devices` 注册设备。
3. 本机生成 device key pair，云端保存 public key 和 device metadata。
4. 云端返回 device credential，Connector 用系统安全存储保存。
5. Connector 建立出站连接：`GET /api/local-connectors/devices/{device_id}/connect`。

云端需要新增数据：

```text
local_connector_devices
  id
  owner_user_id
  display_name
  public_key
  status
  last_seen_at
  revoked_at

local_connector_sessions
  id
  owner_user_id
  device_id
  connection_id
  status
  connected_at
  last_heartbeat_at
```

## 6. 工作区授权与项目绑定

MVP 的授权对象不是“整台电脑”，而是用户主动选择的目录。

```text
local_connector_workspaces
  id
  owner_user_id
  device_id
  display_name
  local_path_alias
  local_path_fingerprint
  capabilities
  status
  created_at
  updated_at
```

本地保存：

```text
workspace_id -> absolute_local_root
```

云端保存：

```text
workspace_id
owner_user_id
device_id
display_name
path_alias
capabilities
```

不要把本地绝对路径作为云端执行依据。云端可以存脱敏展示值，例如 `~/project/chatos_rs` 或用户自定义别名，但真正路径解析必须在 Connector 本地完成。

项目绑定：

```text
local_connector_project_bindings
  id
  owner_user_id
  project_id
  device_id
  workspace_id
  mode: local_mcp | local_terminal | local_sandbox
  enabled
```

ChatOS 添加项目时的变化：

1. 用户选择“本地 Connector 项目”。
2. ChatOS 向云端查询当前用户在线设备和已授权 workspace。
3. 用户选择 workspace 后创建 project。
4. project_management_service 仍保存项目记录，但增加 execution binding，而不是把本地绝对路径当普通 root_path 使用。

## 7. 终端路由

目标：用户在 ChatOS 项目里打开终端时，实际终端运行在 Local Connector 对应的本地 workspace 内。

当前落地采用逻辑 cwd 方案，后续仍可抽象成统一终端 target：

```text
TerminalTarget
  type: server_local | remote_connection | local_connector
  project_id
  owner_user_id
  device_id
  workspace_id
```

实现路径：

1. ChatOS 创建本地终端时使用逻辑 cwd：`local://connector/{device_id}/{workspace_id}`。
2. `/api/terminals` 识别该逻辑 cwd 后只创建终端记录，不在云服务器本机 spawn shell。
3. `/api/terminals/{id}/ws` 识别本地 Connector 终端后，代理到 `local_connector_service` 的 terminal websocket facade。
4. `local_connector_service` 通过 Connector 主动建立的出站 WebSocket 下发 `terminal_session_create_request`、`terminal_input`、`terminal_resize`、`terminal_snapshot_request`、`terminal_close`。
5. Local Connector Rust core 在授权 workspace root 内启动 PTY shell，并把 `terminal_output`、`terminal_snapshot`、`terminal_state`、`terminal_exit` 事件通过同一条长连接回传。
6. Connector 只允许 session cwd 来自授权 workspace；危险命令穷举暂不做。

终端消息：

```json
{
  "type": "terminal_create",
  "request_id": "req_1",
  "project_id": "project_1",
  "workspace_id": "workspace_1",
  "shell": "default"
}
```

```json
{
  "type": "terminal_output",
  "terminal_id": "term_1",
  "chunk": "..."
}
```

同时保留一个更小的 `terminal exec` 能力，用于 Local MCP 工具和调试链路：

```text
POST /api/local-connectors/relay/{device_id}/terminal/exec
```

请求体：

```json
{
  "workspace_id": "workspace_1",
  "command": "cargo",
  "args": ["check"],
  "cwd": ".",
  "timeout_ms": 30000
}
```

Connector 客户端不默认走 shell 展开，而是直接执行 `command + args`；默认 cwd 是授权 workspace 根目录，可选 `cwd` 也必须仍在授权 workspace 内。响应返回 `exit_code`、`success`、`stdout`、`stderr`、`timed_out` 和输出截断标记。交互式终端使用 PTY websocket relay，`terminal exec` 不再作为 ChatOS 终端 UI 的主要路径。

## 8. Task Runner 通过 MCP 操作本地项目

MVP 不需要先重写 Task Runner 执行器。当前落地路径是把 Local Connector 作为“任务级临时 HTTP MCP”注入 Task Runner，而不是写入长期 external MCP config。这样可以避免把用户 token、本地路径或每个 workspace 的会话凭据持久化到通用 MCP 配置表。

Local Connector 提供：

```text
POST /mcp
GET  /mcp/tools
```

ChatOS 创建本地 Connector 项目时，项目 root 使用逻辑路径：

```text
local://connector/{device_id}/{workspace_id}
```

Task Runner 创建任务时：

1. ChatOS 在项目执行任务创建处识别 `local://connector/{device_id}/{workspace_id}`。
2. 不把该逻辑路径写入 `TaskMcpConfig.workspace_dir`，避免 Task Runner 当作云端服务器路径校验。
3. 移除 `CodeMaintainerRead`、`CodeMaintainerWrite`、`TerminalController` 这类服务器本机文件/终端 builtin，避免任务误操作云端默认工作目录。
4. 在 `TaskMcpConfig.ephemeral_http_servers` 注入一个 `local_connector` HTTP MCP：

```json
{
  "name": "local_connector",
  "url": "https://cloud.example.com/api/local-connectors/relay/{device_id}/mcp?workspace_id={workspace_id}",
  "auth_mode": "local_connector_internal"
}
```

5. Task Runner 运行阶段根据 `auth_mode = local_connector_internal` 动态注入服务间 header：

```text
x-local-connector-internal-secret: <TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET>
x-local-connector-owner-user-id: <task.owner_user_id>
x-task-runner-task-id: <task_id>
```

这些 header 不进入任务数据库。Local Connector service 校验 secret 后按 owner_user_id 继续校验 device/workspace 归属，然后通过 Connector 主动建立的长连接把 MCP 请求转发给本地客户端。relay 会过滤内部鉴权 header，避免 secret 被透传到用户本机。

相关环境变量：

```text
TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET
LOCAL_CONNECTOR_INTERNAL_API_SECRET
```

两边必须一致。Task Runner 也兼容从 `TASK_RUNNER_INTERNAL_API_SECRET` 兜底读取；Local Connector service 兼容 `CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET` / `TASK_RUNNER_INTERNAL_API_SECRET`。

MVP 工具先做这些：

1. `local_environment_probe`：返回 OS、shell、git、docker、node、python、rust、可用包管理器。
2. `local_fs_list`：列目录。
3. `local_fs_read`：读文件。
4. `local_fs_search`：搜索文件内容。
5. `local_fs_write`：写入 UTF-8 文件。
6. `local_git_status`：查看 git 状态。
7. `local_git_diff`：查看 diff。
8. `local_terminal_exec`：在 workspace 内执行命令并返回 stdout/stderr/exit_code。

当前 `terminal exec` / Local MCP MVP 的安全边界：

1. 先不做高危命令穷举，也不尝试靠字符串规则判断所有危险操作。
2. 默认不走 shell 展开，只执行明确的 `command + args`。
3. 强制 cwd 在授权 workspace 内。
4. 设置命令超时和 stdout/stderr 回传截断，避免单次响应无限膨胀。
5. 本地 MCP 当前已支持 `local_environment_probe`、`local_fs_list`、`local_fs_read`、`local_fs_search`、`local_fs_write`、`local_git_status`、`local_git_diff`、`local_terminal_exec`；patch 级增量写入继续补。

下一阶段再补：

1. 本地 UI 审批：`sudo`、系统目录写入、Docker socket、SSH key、凭据文件等敏感能力触发人工确认。
2. 本地 audit log：记录云端请求、工作区、命令摘要、退出码和风险标签。
3. Capability policy：按 workspace/project 配置是否允许 terminal、Docker、网络、文件写入、沙箱启动等能力。
4. Path-aware guard：对命令参数里的绝对路径、挂载路径和重定向目标做结构化解析，而不是靠穷举命令名。

## 9. 本地沙箱配对

用户开启本地沙箱后，Local Connector 必须完整具备沙箱能力：本机检查 Docker、本机构建沙箱镜像、本机创建 lease、本机运行 MCP agent、本机 release 并导出结果。它不能再去调用云上的 sandbox manager，也不要求用户额外启动一个本机 sandbox_manager_service。

同时，云端仍然不能直接访问用户本机，因为大多数用户电脑在 NAT、防火墙、家庭网络或公司网络之后，没有可被服务器访问的公网入口。因此连接模型仍然是“本地 Connector 主动连云端”。

正确模型是：

```text
Task Runner
  -> Cloud Local Connector Sandbox Facade
    -> Connector outbound WebSocket / long connection
      -> Local Connector sandbox handler
        -> local Docker sandbox container
          -> sandbox MCP agent
```

也就是说，Task Runner 看到的仍然是一个兼容现有沙箱协议的 `base_url`。代码字段名可以继续叫 `sandbox_manager_base_url`，但这里的值不是云端 sandbox manager，也不是用户电脑的 localhost，而是云端 Local Connector Facade URL。Facade 只负责协议适配和长连接 relay，真正的沙箱由本机 Connector 创建。

云端 Facade 暴露一组兼容现有沙箱协议的 HTTP 接口：

```text
POST /api/sandboxes/leases
GET  /api/sandboxes
GET  /api/sandboxes/:sandbox_id
GET  /api/sandboxes/:sandbox_id/health
GET  /api/sandboxes/:sandbox_id/mcp/tools
POST /api/sandboxes/:sandbox_id/mcp
POST /api/sandboxes/:sandbox_id/mcp/call
POST /api/sandboxes/:sandbox_id/release
```

这些请求进入 Facade 后被转换成长连接 RPC：

```json
{
  "type": "sandbox_request",
  "request_id": "req_...",
  "owner_user_id": "user_...",
  "device_id": "device_...",
  "workspace_id": "workspace_...",
  "method": "POST",
  "path": "/api/sandboxes/leases",
  "body": {
    "tenant_id": "user_...",
    "project_id": "project_...",
    "run_id": "run_..."
  }
}
```

Connector 在本地处理后，通过同一条长连接返回：

```json
{
  "type": "sandbox_response",
  "request_id": "req_...",
  "status": 200,
  "body": {
    "lease_id": "lease_...",
    "sandbox_id": "sandbox_...",
    "status": "ready"
  }
}
```

因此 Task Runner 访问的是云端 Facade URL：

```text
https://cloud.example.com/api/local-connectors/sandbox-facade/{pairing_id}
```

Task Runner 侧采用任务级覆盖，而不是让云服务直连用户电脑：

```text
TaskMcpConfig.sandbox_enabled = true
TaskMcpConfig.sandbox_manager_base_url = cloud local_connector sandbox facade base_url
```

ChatOS 在为 `local://connector/{device_id}/{workspace_id}` 项目创建 Task Runner 任务时，查询当前用户该 workspace 是否存在启用中的 sandbox pairing。存在时注入以上两个字段；不存在时不启用本地沙箱。Task Runner 仍然按原协议创建 lease 和调用 MCP，但请求会被 facade 包装成 `sandbox_request`，通过已在线的 Connector 长连接发到用户本机。

本地沙箱的 workspace 复制不能在云端完成，因为 run workspace 位于用户电脑。处理方式是：

1. Task Runner 识别 `sandbox_manager_base_url` 是 Local Connector facade 后，跳过云端 `effective_workspace_dir -> run_workspace` 复制。
2. Local Connector core 在处理 `POST /api/sandboxes/leases` 时，把 `workspace_root` 改写成本地授权 workspace 的 `.chatos/task-runner`。
3. Local Connector core 在本机创建 `baseline/workspace`、`input/workspace`、`output/workspace`。
4. Local Connector core 把授权 workspace 复制到 baseline 和 input/run workspace，跳过 `.chatos`，避免递归复制运行目录。
5. Local Connector core 启动 Docker 容器，挂载 run workspace 到 `/workspace`，容器内运行 `chatos-sandbox-mcp-server`。
6. Task Runner 的 MCP 请求经 facade -> Connector -> 本机容器 agent。
7. release 时 Connector 把 run workspace 导出到 output workspace，生成 `change_manifest.json`，再按请求销毁容器。

新增数据：

```text
local_connector_sandbox_pairings
  id
  owner_user_id
  device_id
  workspace_id
  enabled
  sandbox_mode: docker | local_process
  facade_base_url
  access_client_id
  created_at
  updated_at
```

MVP sandbox adapter 先支持 `docker` 模式。`local_process` 只是保留给后续非 Docker 环境的扩展占位，不作为当前实现路径。

本机 Docker 镜像：

1. 默认镜像名：`chatos-sandbox-agent:latest`。
2. UI 支持用户创建镜像，core 复用 `sandbox_manager_service/sandbox_agent/Dockerfile` 作为构建模板，但构建动作在 Connector 本机执行。
3. 构建成功后，core 把 `selected_image_ref` 保存到本机 state，后续 lease 默认使用这个镜像。
4. 可通过 `LOCAL_CONNECTOR_SANDBOX_DOCKER_IMAGE` 覆盖默认镜像。

这个设计的关键边界：

1. 云端永远不直连用户本机。
2. Connector 必须先在线并保持出站长连接。
3. Facade 每个请求都必须有 request_id、timeout、owner_user_id、device_id、workspace_id。
4. Connector 断开时，Facade 返回明确的 offline / timeout，而不是继续重试本地地址。
5. 本机 Docker 容器只挂载授权 workspace 的 run copy，不挂载用户整个文件系统。
6. MCP 流式输出后续需要走同一条长连接的 stream message，不能依赖云端访问本地 WebSocket。

## 10. 云端新增服务边界

必须新增独立的 `local_connector_service`，不要放进 `chat_app_server_rs`，也不要散落在各服务里。原因是 Connector 控制面和 Relay 是基础设施能力，不属于 ChatOS 主后端的会话/项目 API；后续 Task Runner、ChatOS Web、Sandbox Facade 都需要通过统一服务调用它。

```text
local_connector_service/backend
  devices
  sessions
  workspaces
  project_bindings
  sandbox_pairings
  relay
  sandbox_facade
```

本服务拥有自己的配置、数据库和迁移：

```text
LOCAL_CONNECTOR_SERVICE_HOST
LOCAL_CONNECTOR_SERVICE_PORT
LOCAL_CONNECTOR_DATABASE_URL
LOCAL_CONNECTOR_USER_SERVICE_BASE_URL
LOCAL_CONNECTOR_PUBLIC_BASE_URL
LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS
LOCAL_CONNECTOR_INTERNAL_API_SECRET
```

`chat_app_server_rs` 以后只做客户端调用和 UI 聚合，不直接保存 Connector device/workspace/session 状态。

当前 MVP API：

```text
POST   /api/local-connectors/devices
GET    /api/local-connectors/devices
GET    /api/local-connectors/devices/{id}
POST   /api/local-connectors/devices/{id}/heartbeat
POST   /api/local-connectors/devices/{id}/revoke
GET    /api/local-connectors/devices/{id}/connect
POST   /api/local-connectors/workspaces
GET    /api/local-connectors/workspaces
PUT    /api/local-connectors/workspaces/{id}
DELETE /api/local-connectors/workspaces/{id}
POST   /api/local-connectors/project-bindings
GET    /api/local-connectors/project-bindings?project_id=...
PUT    /api/local-connectors/project-bindings/{id}
DELETE /api/local-connectors/project-bindings/{id}
POST   /api/local-connectors/sandbox-pairings
GET    /api/local-connectors/sandbox-pairings
PUT    /api/local-connectors/sandbox-pairings/{id}
DELETE /api/local-connectors/sandbox-pairings/{id}
POST   /api/local-connectors/relay/{device_id}/mcp?workspace_id=...
POST   /api/local-connectors/relay/{device_id}/terminal/exec
POST   /api/local-connectors/sandbox-facade/{pairing_id}
ANY    /api/local-connectors/sandbox-facade/{pairing_id}/{*path}
```

其中 `sandbox-facade` 是云端 HTTP facade。它负责把 HTTP 请求包装成 `sandbox_request`，通过已在线的 Connector 长连接发出去，并等待 `sandbox_response`。

Connector 长连接消息约定：

```json
{
  "type": "mcp_request",
  "request_id": "req_uuid",
  "owner_user_id": "user_1",
  "device_id": "device_1",
  "workspace_id": "workspace_1",
  "method": "POST",
  "path": "/mcp",
  "headers": {},
  "body": {}
}
```

```json
{
  "type": "mcp_response",
  "request_id": "req_uuid",
  "status": 200,
  "headers": {},
  "body": {}
}
```

`terminal_exec_request/terminal_response` 和 `sandbox_request/sandbox_response` 使用同样的 envelope，只是 `type` 不同；terminal 的 `path` 是 `/terminal/exec`，sandbox 的 `path` 是 sandbox-manager-compatible path。

当前已落地的第一步是 `local_connector_service/backend`：

1. 独立 Rust 服务，加入 workspace。
2. SQLite migrations 管理 device/workspace/project binding/sandbox pairing/session。
3. 复用 `user_service /api/auth/verify` 做鉴权。
4. `GET /api/local-connectors/devices/{id}/connect` 接受 Connector 主动发起的 WebSocket 长连接，并维护在线/心跳/断开状态。
5. 内存 relay registry 管理在线 device session、pending request、timeout 和 offline 错误。
6. `POST /api/local-connectors/relay/{device_id}/mcp?workspace_id=...` 已经能把 MCP HTTP 请求包装成 `mcp_request` 并等待 `mcp_response`。
7. `POST /api/local-connectors/relay/{device_id}/terminal/exec` 已经能把命令执行请求包装成 `terminal_exec_request` 并等待 `terminal_response`。
8. `sandbox-facade` 已经能把 HTTP 请求包装成 `sandbox_request` 并等待 `sandbox_response`。
9. 支持内部服务鉴权：Task Runner 可使用 `x-local-connector-internal-secret` 和 `x-local-connector-owner-user-id` 调用 relay / facade，避免把用户 token 持久化到任务配置。

当前已落地的本地客户端是 `local_connector_client`：

1. `core` 是 Rust 本机 daemon，启动后监听 `127.0.0.1:39232`。
2. `frontend` 是 React 本地客户端 UI，放在 `local_connector_client/frontend`，用于用户登录/注册、开放目录、本地终端测试、沙箱开关和镜像创建。
3. UI 登录后，core 调用 `user_service /api/auth/login` 或 `/api/auth/register` 获取 token，再调用 `local_connector_service` 注册 device。
4. UI 目录授权默认关闭；用户点击开放目录后，通过 core 的本机目录浏览 API 多选目录，并由 core 注册 cloud workspace，同时在本机 state file 保存 `workspace_id -> absolute_local_root`。
5. core 主动连接 `GET /api/local-connectors/devices/{id}/connect` WebSocket。
6. 已能处理 `mcp_request`，支持 `tools/list`、`local_environment_probe`、`local_fs_list`、`local_fs_read`、`local_fs_search`、`local_fs_write`、`local_git_status`、`local_git_diff`、`local_terminal_exec` MVP 工具。
7. 已能处理 `terminal_exec_request`，在授权 workspace 内直接执行 `command + args`，返回退出码、stdout/stderr、超时和截断信息。
8. UI 的终端测试会调用 core，再经 `local_connector_service` relay 回到本机执行，用于验证 ChatOS 后续接入路径。
9. UI 的本地沙箱开关默认关闭；开启时 core 会检查 Docker 是否安装和运行，未运行时会尝试启动 Docker Desktop，然后同步 Local Connector sandbox pairing 元数据。
10. UI 的镜像创建由 core 在本机执行 Docker build，默认复用 `sandbox_manager_service/sandbox_agent/Dockerfile` 作为镜像模板，但不代理到任何 sandbox manager 服务。
11. 下一步需要补完整 PTY 终端、审计日志、Tauri 原生目录选择/安全存储，以及更完整的 MCP 文件写入/patch 能力。

## 11. 与现有模块的改造点

`project_management_service`：

1. Project 增加 execution binding 查询能力。
2. root_path 保持兼容，但本地 Connector 项目不依赖云端 root_path 做真实路径。

`chat_app_server_rs`：

1. 不保存 Local Connector 设备、工作区、session 状态。
2. 新增 `local_connector_service` API client，用于查询当前用户设备、工作区、项目绑定。
3. 添加终端 target 路由；target 为 `local_connector` 时转发到 `local_connector_service` relay，不在服务器本机 spawn shell。
4. 添加项目创建/更新时的 local workspace binding UI/调用。
5. 聊天 runtime settings 可以增加 `execution_target_id` 或 `local_connector_workspace_id`。

`task_runner_service`：

1. `TaskMcpConfig` 新增 `ephemeral_http_servers`，支持任务级临时 HTTP MCP 注入。
2. Task 创建时由 ChatOS 按 `local://connector/{device_id}/{workspace_id}` 自动注入 Local Connector MCP config。
3. 本地 Connector 项目不设置 `workspace_dir` 为逻辑路径，避免触发服务器本机目录校验；本地文件/命令通过 `local_connector` MCP 完成。
4. 本地 Connector 项目会过滤服务器本机 code/terminal builtin，并在模型输入中提示必须使用 `local_connector_*` 工具操作项目。
5. Task Runner 运行时根据 `auth_mode = local_connector_internal` 注入服务间 secret 和 owner_user_id。
6. `TaskMcpConfig` 新增任务级 `sandbox_enabled` / `sandbox_manager_base_url` 覆盖字段；ChatOS 为本地 Connector 项目查询启用中的 sandbox pairing 后注入云端 sandbox facade URL。
7. Run metadata 记录实际 execution target。

`sandbox protocol / local Docker sandbox`：

1. Task Runner 继续复用现有 lease/health/mcp/release 协议。
2. Local Connector Service 提供 relay facade，负责把协议请求包装成长连接 `sandbox_request`；它不创建云端沙箱，不调用云端 Sandbox Manager。
3. Local Connector client 在本机 Docker 内创建容器、运行 MCP agent、导出 release 结果。
4. 对本地 Connector 模式，不要求用户本机公网可访问；只要求 Connector 能主动连上云端 Relay，云端 sandbox facade 能通过这条长连接完成请求/响应。

`user_service`：

1. 复用现有登录与 `/api/auth/verify`。
2. 后续可增加 device credential 专用 token audience。

## 12. 安全底线

MVP 不做复杂危险命令识别，但必须有这些底线：

1. Connector 必须登录并绑定 owner_user_id。
2. 只有用户显式授权的 workspace 可以被云端引用。
3. 所有文件、终端、MCP 请求必须带 workspace_id。
4. Connector 本地校验 workspace_id -> absolute path 映射。
5. 所有本地执行 cwd 必须在授权 workspace root 内。
6. 断开连接后云端任务不能继续执行本地操作。
7. 设备可以从云端 UI 撤销。
8. 本地 UI 可以一键暂停所有云端请求。
9. 云端不保存用户本机 SSH key、Docker socket 凭据、系统密码。
10. 所有 relay 请求都要记录 request_id、user_id、device_id、workspace_id、project_id、run_id。

## 13. 分阶段实施

### Phase 0：协议和数据模型

1. 定义 Local Connector device/workspace/project binding/sandbox pairing 数据模型。
2. 定义 connector relay 消息协议。
3. 定义 Local MCP tool schema。
4. 明确 ChatOS project 和 Task Runner task 如何解析 execution target。

### Phase 1：客户端骨架

1. 新建 Rust core CLI/daemon 原型。当前已完成。
2. 实现登录、设备注册、设备凭据本地安全存储。当前已支持 UI 登录/注册和设备注册，安全存储待 Tauri 阶段接入。
3. 实现本地目录选择和 workspace 授权。当前已支持本机目录浏览、多选目录和 workspace 注册。
4. 实现出站连接和 heartbeat。当前已完成。
5. 新建 React/Tauri UI，调用 Rust core 的登录、授权目录和连接状态能力。当前已完成 React UI，Tauri 打包待后续接入。

### Phase 2：项目和终端打通

1. ChatOS 添加本地项目入口。
2. 创建 project binding。
3. ChatOS 终端 target 支持 `local_connector`。
4. Connector 实现本地 terminal create/input/output/kill。

### Phase 3：Task Runner MCP 打通

1. Connector 实现 HTTP MCP。
2. Task Runner 支持任务级 `ephemeral_http_servers`。
3. ChatOS 为本地 Connector 项目自动注入 `local_connector` MCP relay URL。
4. Local Connector service 支持 Task Runner 内部服务鉴权。
5. 实现读文件、搜索、patch、git status、terminal exec。

### Phase 4：本地沙箱配对

1. Connector UI 增加“开启本地沙箱配对”。
2. Connector core 检查 Docker、构建/选择本地沙箱镜像。
3. Connector core 支持本机 Docker lease、health、mcp、release。
4. Task Runner 按 owner_user_id/project_id 选择云端 sandbox facade base_url。
5. 验证 lease、health、mcp、release 全链路。

### Phase 5：安全增强

1. 引入 capability token。
2. 增加本地审批队列。
3. 增加命令效果分析。
4. 增加 Docker 策略。
5. 增加完整审计与回放。

## 14. MVP 验收标准

1. 用户可以在 Local Connector 登录 ChatOS 账号。
2. 用户可以授权一个本地目录，并在 ChatOS 创建绑定项目。
3. ChatOS 打开该项目终端时，终端实际运行在用户本机授权目录内。
4. Task Runner 任务可以通过 MCP 读取、搜索、patch 本地项目文件。
5. Task Runner 任务可以在本地项目目录运行命令并看到输出。
6. 用户开启本地沙箱配对后，同一用户的 Task Runner 沙箱请求能路由到该 Connector。
7. Connector 断开后，云端能正确显示离线，任务不再误以为本地环境可用。
8. 云端和本地都能看到基本操作日志。

## 15. 当前建议决策

1. 客户端使用 Tauri：Rust core + React UI。
2. MVP 使用 WebSocket relay，不先做 QUIC/HTTP2。
3. 本地能力先以 HTTP MCP 形式接 Task Runner，不先重写 Task Runner worker。
4. 沙箱协议保持兼容现有 Task Runner 接口，但实现必须在 Local Connector 本机 Docker 内完成。
5. 危险命令策略后置，但 workspace 边界、登录、设备撤销必须首版就有。
