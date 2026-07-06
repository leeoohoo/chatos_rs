# Local Connector 实施方案

## 1. 目标

Local Connector 是 ChatOS 的“用户授权本地执行运行时”。

云服务继续负责身份、对话编排、任务调度、模型调用、记忆、审计索引和 UI 状态；Local Connector 运行在用户自己的机器上，只在用户明确授权的工作区内执行本地开发操作。

产品目标是恢复本地 AI 编程助手的真实可用性，同时避免把云服务变成不受限制的远程控制通道。

## 2. 核心原则

1. 连接永远由本地 Connector 主动向云端发起。
2. 云端不能获得用户本机的长期凭据。
3. 每一个本地动作都必须绑定短期 capability token。
4. 每个 capability 都必须限定 user、device、workspace、project、run、tool 和 TTL。
5. 未知 shell 命令默认不可信。
6. 高影响操作必须经过本地策略和用户确认。
7. 文件写入优先走 patch 和 diff，而不是直接改真实文件。
8. Docker 是高权限本地能力，不能当作普通 shell 命令处理。
9. 用户必须能随时暂停、断开、吊销和查看 Connector 状态。
10. 策略、身份、作用域或审计检查不完整时，Connector 必须 fail closed。

## 3. 目标架构

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

上层不应该关心任务实际运行在云端沙箱、用户自管远端机器，还是本地 Connector。它们应该调用同一套 execution runtime 接口。

## 4. Execution Target 模型

新增统一执行目标抽象：

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

对于 Local Connector，一台物理设备可以暴露多个工作区，但每个工作区都必须由用户显式注册。

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

云端 API 应该使用 `workspace_id` 和相对路径。本地绝对路径只存在于 Connector 内部映射层。

## 5. 配对与信任建立

配对流程：

1. 用户在 ChatOS 中点击“连接本地机器”。
2. 云端创建短期 pairing code。
3. 用户启动 Local Connector，输入 code 或扫描二维码。
4. Connector 完成用户和设备认证。
5. 云端签发 device-bound refresh credential，只存储在 Connector 本地。
6. Connector 注册设备信息和支持的能力。
7. 用户在本机授权可访问的 workspace。

安全要求：

1. pairing code 的 TTL 要短，例如 5 分钟。
2. device credential 必须能在云端 UI 吊销。
3. Connector 应尽量使用系统安全存储。
4. device identity 应包含本地生成的密钥对。
5. 运行时 capability token 必须短期、限定 audience。

## 6. 数据通道

Connector 维护一条出站连接：

```text
GET /api/local-connectors/connect
Authorization: Bearer <device_token>
```

推荐传输方式：

1. MVP 使用 WebSocket。
2. 后续如需更强多路复用和背压，可以升级到 HTTP/2 或 QUIC stream。

请求消息示例：

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

响应消息示例：

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

Connector 必须支持长任务的取消和流式输出。

## 7. Capability Token

capability token 应该按操作签发，或按很短的 run window 签发。

Claims 示例：

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

Connector 执行任何操作前必须在本地校验 token。云端请求只是一个“执行提案”，最终是否执行由本地策略决定。

## 8. 工具模型

优先使用结构化工具，避免让 AI 默认发送自由文本 shell。

初始工具：

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

自由 shell 只能作为高级工具存在：

```text
terminal_exec
```

`terminal_exec` 默认策略：

1. 只读探测命令如果命中 allowlist，可以自动执行。
2. 未知命令必须弹出本地确认。
3. 触碰 workspace 之外路径的命令默认阻断，除非用户显式授权。
4. 需要提权的命令默认阻断。

## 9. 风险策略

不要依赖危险命令黑名单。应使用基于“操作效果”的策略。

风险等级：

| 等级 | 例子 | 默认策略 |
| --- | --- | --- |
| L0 | 版本检查、环境探测、`git status` | 自动允许 |
| L1 | 读取项目文件、搜索项目文件 | workspace 授权后自动允许 |
| L2 | 通过 patch 写项目文件 | 需要 diff 确认或项目级授权 |
| L3 | 在 workspace 内跑测试或构建 | 按项目策略允许 |
| L4 | 启动服务、暴露本地端口、Docker Compose | 展示影响并确认 |
| L5 | 安装全局包、修改系统配置、访问外部路径 | 每次确认 |
| L6 | privileged Docker、挂载宿主根、访问密钥、大规模破坏性操作 | 默认阻断 |

策略输入：

1. 工具类型。
2. capability token scopes。
3. workspace 路径作用域。
4. 命令解析结果。
5. 文件系统影响。
6. 网络和端口影响。
7. Docker 影响。
8. 用户确认状态。

## 10. Docker 策略

Docker 必须作为一等策略域处理。

可自动允许的候选操作：

1. `docker version`
2. 脱敏后的 `docker info`
3. `docker ps`
4. `docker images`
5. `docker compose config`

需要确认的操作：

1. `docker compose up`
2. `docker compose build`
3. 拉取镜像。
4. 暴露端口。
5. 创建 named volume。
6. 使用环境变量文件。

默认阻断：

1. `--privileged`
2. `--pid=host`
3. `--ipc=host`
4. `--network=host`
5. `--device`
6. `--cap-add`
7. 挂载 `/`
8. 挂载用户 home 目录。
9. 挂载 `.ssh`
10. 挂载系统凭据目录。
11. 挂载 `/var/run/docker.sock`
12. Compose 文件中出现以上能力。

运行 Docker Compose 前，Connector 应解析 compose 文件并展示摘要：

```text
Services: backend, frontend, mongo
Images to pull/build: ...
Ports: 3997, 8088, 27018
Mounts: ./data -> /data
Privileged features: none
Environment files: .env
```

用户可以选择：仅允许一次、允许本项目、拒绝。

## 11. 本地确认 UX

Connector 需要一个可见的本地控制界面。

必须提供：

1. 当前连接状态。
2. 当前云端账号。
3. 已授权 workspaces。
4. 正在运行的任务和命令。
5. 待审批请求。
6. 暂停所有执行。
7. 断开设备。
8. 吊销 workspace 授权。
9. 查看近期审计日志。

确认弹窗应展示：

1. 谁发起了请求。
2. 影响哪个项目和 workspace。
3. 将运行哪个工具。
4. 涉及哪些文件、端口、Docker 资源或命令。
5. 是否会写入、删除、启动服务或触碰密钥。

## 12. 审计与脱敏

审计记录：

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

脱敏规则：

1. 默认不向云端发送原始本地环境变量。
2. 脱敏 token 形态字符串。
3. 脱敏私钥和证书块。
4. `.env` 值除非明确批准，否则脱敏。
5. 云端可见输出尽量脱敏 home 目录路径。

## 13. 与现有服务集成

### `task_runner_service`

增加 execution target 选择：

1. 判断 run 应使用云端沙箱、远端 runner，还是 Local Connector。
2. 将选中的 target 写入 `run.input_snapshot`。
3. 将 MCP 工具路由到选中的 runtime。
4. 复用现有 sandbox output manifest 流程记录文件变更。

### `sandbox_manager_service`

继续保留现有 sandbox lease 模型给云端沙箱使用。

新增 sibling lease 或广义 runtime lease：

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

新增设备和 workspace 管理 API：

1. 配对 Connector。
2. 列出设备。
3. 吊销设备。
4. 列出 Connector workspaces。
5. 为 project 或 task 选择 execution target。

### 前端

新增 UI：

1. Local Connector 引导页。
2. 设备列表。
3. workspace 授权状态。
4. execution target 选择器。
5. 任务运行详情里的本地审批状态。

## 14. MVP 范围

MVP 应避免 unrestricted shell。

MVP 能力：

1. 配对和吊销 Connector。
2. 注册本地 workspace。
3. 探测本地开发环境。
4. 读取和搜索项目文件。
5. 查看 Git status 和 diff。
6. 经本地确认后写入 patch。
7. 经确认后运行项目测试和构建命令。
8. Docker Compose 只通过结构化 `docker_compose_*` 工具执行。
9. 将命令输出流式返回 Task Runner。
10. 本地记录审计日志，并向云端发送脱敏摘要。

MVP 不应包含：

1. 全系统 shell 自动执行。
2. privileged Docker。
3. 宿主根目录挂载。
4. 访问 SSH key 或系统凭据存储。
5. 未经明确确认的全局软件安装。

## 15. 分阶段交付

### 阶段 1：基础能力

1. Execution target 数据模型。
2. 设备配对和吊销。
3. Connector 出站 WebSocket。
4. capability token 签发和校验。
5. 本地 workspace registry。
6. 基础审计和脱敏。

### 阶段 2：读取与 Diff

1. 环境探测。
2. 文件列表、读取和搜索。
3. Git status 和 diff。
4. 云端 UI 展示已连接设备和 workspace 状态。

### 阶段 3：受控写入

1. 仅允许 patch 写入。
2. 本地 diff 审批。
3. 集成 run output change manifest。
4. Task Runner 结果预览。

### 阶段 4：本地命令

1. 结构化项目命令执行。
2. 测试和构建命令 profile。
3. 端口检测。
4. 流式输出和取消。

### 阶段 5：Docker

1. Docker 可用性探测。
2. Compose 解析和影响摘要。
3. `docker_compose_config`。
4. 经确认的 `docker_compose_up`。
5. 经确认的 `docker_compose_down`。
6. Docker 策略执行。

### 阶段 6：高级模式

1. 可选自由 shell。
2. 项目级 allowlist。
3. 团队策略 profile。
4. 企业审计导出。
5. 管理员托管设备策略。

## 16. 验收标准

1. 用户可以从 ChatOS UI 配对本地 Connector。
2. 用户可以授权一个本地项目 workspace。
3. ChatOS 可以针对该 workspace 运行 Task Runner 任务。
4. 任务可以读取文件并查看 Git 状态，但不能访问 workspace 外路径。
5. 文件写入以 diff 展示，并需要本地确认。
6. Docker Compose 执行前会展示影响摘要。
7. privileged Docker 选项默认阻断。
8. 用户可以在执行中暂停或断开 Connector。
9. 被吊销设备不能再接收或执行请求。
10. 审计日志能展示请求、允许、拒绝和阻断的操作。

## 17. 待确认问题

1. Connector 应优先做成桌面应用，还是先做独立 CLI？
2. device credential 是否必须只存 OS secure storage，还是允许加密文件 fallback？
3. 团队是否需要组织级 Connector policy profile？
4. Docker Compose 审批是否按 compose 文件 hash 记住？
5. 本地命令 profile 应由项目文件配置、云端配置，还是二者都支持？
6. 自由 shell 是否进入 MVP，还是等策略遥测成熟后再开放？
