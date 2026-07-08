# 沙箱管理微服务实施计划

## 目标

新增一个独立的沙箱管理微服务，专门管理 AI 任务运行时使用的隔离环境。当任务启用了终端、文件读写、搜索、补丁应用等会操作文件系统或执行命令的内置工具时，Task Runner 不再直接在宿主机执行这些工具，而是向沙箱管理服务申请一个沙箱环境。

任务运行期间，所有文件和终端类内置 MCP 调用都进入沙箱内的固定服务 `sandbox-agent`。`sandbox-agent` 只操作沙箱内的项目副本。任务结束后，沙箱内的项目副本被导出回 `.chatos` 下的 run 工作区，再由外部程序做 diff、审计、展示或后续受控写回。

这个方案的核心价值：

- AI 不能直接操作真实项目目录。
- AI 不能直接访问宿主机其他用户、其他项目、平台凭证。
- 终端命令和脚本都只在沙箱项目副本内运行。
- 沙箱生命周期可被统一调度、限流、回收和审计。
- 后续可以从简单 Docker 后端平滑升级到 gVisor、Kata、microVM 等更强隔离后端。

## 总体架构

```text
Task Runner
  -> Sandbox Manager 申请/释放沙箱
  -> Workspace Materializer 复制项目到 .chatos run 工作区
  -> Sandbox Backend 创建隔离环境
  -> sandbox-agent 在沙箱内提供文件/终端 MCP 能力
  -> Task Runner 执行模型循环，工具调用代理到 sandbox-agent
  -> Result Exporter 导出沙箱结果到 .chatos run 工作区
  -> Sandbox Manager 销毁或回收沙箱
```

建议服务命名：

```text
sandbox_manager_service/
  backend/
    src/
      main.rs
      api/
      config.rs
      manager/
      pool/
      backend/
      agent_client/
      workspace/
      store/
```

沙箱内固定服务命名：

```text
sandbox_agent/
  src/
    main.rs
    mcp/
    filesystem/
    terminal/
    diff/
```

也可以先放在同一个 Rust workspace 里：

```text
crates/chatos_sandbox_protocol/
crates/chatos_sandbox_agent/
sandbox_manager_service/backend/
```

## 核心原则

1. 沙箱池只能复用干净环境，不能复用带用户数据的环境。
2. 任务开始前复制项目副本，沙箱只看到副本，不看到真实项目根目录。
3. 任务结束后不由沙箱直接写真实项目，只导出到 `.chatos` 下的 run 工作区。
4. 所有文件和终端工具都通过 `sandbox-agent` 执行。
5. 严格模式不可用时，不能自动降级到宿主机执行。
6. 每个沙箱绑定 `tenant_id/user_id/project_id/run_id`，日志和审计都以 run 为中心。
7. 沙箱必须有 TTL、心跳、资源限制和异常清理。

## 数据目录设计

以项目根目录为：

```text
/projects/demo
```

每次 run 创建：

```text
/projects/demo/.chatos/sandboxes/runs/{run_id}/
  metadata.json
  input/
    workspace/            # 任务开始前复制出来的项目副本
  output/
    workspace/            # 任务结束后从沙箱导出的最终副本
  diff/
    summary.json
    changes.patch
    changed_files.json
  logs/
    sandbox-manager.log
    sandbox-agent.log
    terminal.ndjson
  state/
    lease.json
    heartbeat.json
```

复制项目时必须排除：

```text
.chatos/sandboxes/
.chatos/runtime/
.git/hooks/
target/
node_modules/
dist/
build/
.DS_Store
```

其中 `target/ node_modules/ dist/ build/` 是否排除可以配置。MVP 可以默认排除大目录，后续允许项目级配置。

重要：复制时不能递归复制 `.chatos/sandboxes`，否则会把历史 run 和当前 run 自己复制进去。

## 服务划分

### 1. Sandbox Manager

职责：

- 管理沙箱租约。
- 创建、启动、停止、销毁沙箱。
- 维护沙箱池和并发上限。
- 维护沙箱状态机。
- 为 Task Runner 提供租用接口。
- 代理或签发访问 `sandbox-agent` 的短期 token。
- 在 run 超时、心跳丢失、任务取消时清理沙箱。
- 记录审计日志和资源使用。

不负责：

- 不直接执行 AI 命令。
- 不直接读写真实项目业务文件，除非作为 materializer/exporter 的显式步骤。
- 不把平台长期密钥注入沙箱。

### 2. Workspace Materializer

职责：

- 根据 `workspace_root` 和 `run_id` 创建 `.chatos/sandboxes/runs/{run_id}`。
- 把项目复制到 `input/workspace`。
- 生成 `metadata.json`，记录复制时间、源路径、排除规则、文件数量、总大小。
- 校验 symlink，避免复制结果指向项目外。
- 可以把 `input/workspace` 打包成 tar，上传给沙箱后端。

MVP 可以放在 Task Runner 中实现，后续也可以下沉到 Sandbox Manager。

### 3. sandbox-agent

运行在每个沙箱内部，是固定 server。它提供内置 MCP 能力：

- 文件读取。
- 文件写入。
- 目录列表。
- 文本搜索。
- 应用 patch。
- 执行终端命令。
- 后台进程管理。
- 获取终端日志。
- 生成 diff。
- 导出最终 workspace。

它只允许访问沙箱内的 `/workspace`。

### 4. Result Exporter

职责：

- 从沙箱导出最终 workspace 到 `.chatos/sandboxes/runs/{run_id}/output/workspace`。
- 对比 `input/workspace` 和 `output/workspace`。
- 生成 diff summary、patch、changed files。
- 检查非法输出：
  - 超大文件。
  - 软链接逃逸。
  - socket、fifo、设备文件。
  - `.ssh`、`.env`、token 文件。
  - 路径逃逸。
- MVP 阶段只写回 `.chatos` run 工作区，不自动覆盖真实项目。

后续如果要把结果应用到真实项目，必须单独做 `apply_result` 流程，经过用户确认或策略审批。

## 沙箱生命周期

### 状态机

```text
IdleClean
  -> Leasing
  -> PreparingWorkspace
  -> Starting
  -> Ready
  -> Running
  -> Exporting
  -> Releasing
  -> Destroyed

任何状态 -> Failed -> Destroying -> Destroyed
Ready/Running 心跳超时 -> Expired -> Destroying -> Destroyed
```

状态说明：

- `IdleClean`：干净沙箱或容量槽位，可以租用。不能带用户数据。
- `Leasing`：已分配给某个 run，正在创建租约。
- `PreparingWorkspace`：正在准备项目副本。
- `Starting`：后端正在启动容器/VM，agent 尚未 ready。
- `Ready`：agent ready，可以接收工具调用。
- `Running`：任务正在调用工具。
- `Exporting`：任务结束，正在导出结果。
- `Releasing`：释放资源。
- `Destroyed`：资源已销毁。
- `Failed`：创建、运行或导出失败。

### 租约字段

```json
{
  "lease_id": "lease_...",
  "sandbox_id": "sandbox_...",
  "tenant_id": "tenant_1",
  "user_id": "user_1",
  "project_id": "project_1",
  "run_id": "run_1",
  "workspace_root": "/projects/demo",
  "run_workspace": "/projects/demo/.chatos/sandboxes/runs/run_1/input/workspace",
  "agent_endpoint": "http://127.0.0.1:39001",
  "agent_token_ref": "token_ref_or_inline_mvp",
  "status": "Ready",
  "created_at": "2026-06-30T00:00:00Z",
  "expires_at": "2026-06-30T02:00:00Z",
  "resource_limits": {
    "cpu": 2,
    "memory_mb": 4096,
    "disk_mb": 10240,
    "max_processes": 128
  }
}
```

## Sandbox Manager API

MVP 先使用 HTTP JSON。后续如果工具调用吞吐较高，可以把 agent 调用改成 gRPC 或 WebSocket。

### 创建租约

```http
POST /api/sandboxes/leases
```

请求：

```json
{
  "tenant_id": "tenant_1",
  "user_id": "user_1",
  "project_id": "project_1",
  "run_id": "run_1",
  "workspace_root": "/projects/demo",
  "tools": ["filesystem", "terminal"],
  "ttl_seconds": 7200,
  "resource_limits": {
    "cpu": 2,
    "memory_mb": 4096,
    "disk_mb": 10240,
    "max_processes": 128
  },
  "network": {
    "mode": "none"
  }
}
```

响应：

```json
{
  "lease_id": "lease_123",
  "sandbox_id": "sandbox_123",
  "status": "ready",
  "agent_endpoint": "http://127.0.0.1:39001",
  "agent_token": "short_lived_token",
  "run_workspace": "/projects/demo/.chatos/sandboxes/runs/run_1/input/workspace",
  "expires_at": "2026-06-30T02:00:00Z"
}
```

### 心跳

```http
POST /api/sandboxes/{sandbox_id}/heartbeat
```

请求：

```json
{
  "lease_id": "lease_123",
  "run_id": "run_1"
}
```

响应：

```json
{
  "ok": true,
  "status": "running",
  "expires_at": "2026-06-30T02:00:00Z"
}
```

### 释放沙箱

```http
POST /api/sandboxes/{sandbox_id}/release
```

请求：

```json
{
  "lease_id": "lease_123",
  "run_id": "run_1",
  "export_result": true,
  "destroy": true
}
```

响应：

```json
{
  "ok": true,
  "status": "destroyed",
  "output_workspace": "/projects/demo/.chatos/sandboxes/runs/run_1/output/workspace",
  "diff_summary": "/projects/demo/.chatos/sandboxes/runs/run_1/diff/summary.json"
}
```

### 强制销毁

```http
DELETE /api/sandboxes/{sandbox_id}
```

用于异常清理、超时、管理员操作。

### 查询状态

```http
GET /api/sandboxes/{sandbox_id}
GET /api/sandboxes?tenant_id=...&project_id=...&run_id=...
GET /api/sandbox-pool/status
```

## sandbox-agent API

`sandbox-agent` 可以实现一个 MCP server，也可以先实现 HTTP API，再由 Task Runner 的 MCP provider 做适配。建议内部仍然保持 MCP tool 语义，便于未来直接暴露 MCP。

### 健康检查

```http
GET /health
```

响应：

```json
{
  "ok": true,
  "workspace": "/workspace",
  "agent_version": "0.1.0"
}
```

### MCP JSON-RPC

```http
POST /mcp
```

列出工具请求：

```json
{
  "jsonrpc": "2.0",
  "id": "tools-1",
  "method": "tools/list",
  "params": {}
}
```

调用工具请求：

```json
{
  "jsonrpc": "2.0",
  "id": "call-1",
  "method": "tools/call",
  "params": {
    "name": "execute_command",
    "arguments": {
      "command": "npm test",
      "cwd": "."
    }
  }
}
```

响应：

```json
{
  "result": {
    "content": [{ "type": "text", "text": "..." }]
  },
  "jsonrpc": "2.0",
  "id": "call-1"
}
```

### 路径规则

所有文件路径都必须：

- 相对 `/workspace`。
- canonicalize 后仍在 `/workspace` 内。
- 默认不跟随指向 `/workspace` 外部的 symlink。
- 不允许绝对宿主路径。

## 沙箱池设计

### 池的职责

沙箱池类似线程池，但要注意用户数据安全。池可以管理容量和干净基础环境，不能把已经跑过用户数据的沙箱直接放回给下一个用户。

MVP 推荐：

- 池只控制最大并发数。
- 每次租约创建一个新沙箱。
- 每次释放都销毁沙箱。
- 暂不复用带 workspace 的容器。

第二阶段优化：

- 维护 `IdleClean` 预热沙箱。
- 预热沙箱只包含镜像、agent、空 workspace。
- 租用后注入 workspace 副本。
- 释放后仍然销毁，不回到 `IdleClean`。
- 后续如要复用，必须做强清理和证明，但不建议早期做。

### 池配置

```env
SANDBOX_POOL_MAX_ACTIVE=20
SANDBOX_POOL_MAX_PENDING=100
SANDBOX_POOL_IDLE_CLEAN_TARGET=5
SANDBOX_LEASE_TTL_SECONDS=7200
SANDBOX_CREATE_TIMEOUT_SECONDS=60
SANDBOX_AGENT_READY_TIMEOUT_SECONDS=30
SANDBOX_RELEASE_TIMEOUT_SECONDS=120
SANDBOX_FORCE_DESTROY_AFTER_SECONDS=300
```

### 排队策略

- 按租户限制并发。
- 按用户限制并发。
- 按项目限制并发。
- 超过 `max_pending` 直接拒绝。
- pending 超时返回 `sandbox_capacity_exceeded`。

建议错误码：

```text
sandbox_capacity_exceeded
sandbox_create_timeout
sandbox_agent_unhealthy
sandbox_lease_expired
sandbox_destroy_failed
sandbox_workspace_materialize_failed
sandbox_result_export_failed
```

## 后端实现选择

### MVP 后端

如果目标是“先把微服务做出来，实现简单创建、运行、销毁”，MVP 可以先用 Docker 或 containerd 实现后端，但接口必须抽象出来：

```rust
#[async_trait]
pub trait SandboxBackend: Send + Sync {
    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, SandboxError>;
    async fn start(&self, id: &str) -> Result<(), SandboxError>;
    async fn stop(&self, id: &str) -> Result<(), SandboxError>;
    async fn destroy(&self, id: &str) -> Result<(), SandboxError>;
    async fn inspect(&self, id: &str) -> Result<SandboxInstance, SandboxError>;
    async fn copy_into(&self, id: &str, source: &Path, dest: &str) -> Result<(), SandboxError>;
    async fn copy_out(&self, id: &str, source: &str, dest: &Path) -> Result<(), SandboxError>;
}
```

这样以后可以替换成：

- Docker。
- containerd。
- gVisor RuntimeClass。
- Kata RuntimeClass。
- Firecracker/microVM。

### MVP Docker 启动示例

```bash
docker run --rm -d \
  --name chatos-sandbox-${sandbox_id} \
  --network none \
  --cpus 2 \
  --memory 4g \
  --pids-limit 128 \
  --read-only \
  --tmpfs /tmp:rw,noexec,nosuid,size=512m \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  -v ${run_workspace}:/workspace:rw \
  chatos-sandbox-agent:latest
```

MVP 可以先用本机 bind mount `run_workspace`。生产化时建议改成：

- 临时卷。
- 对象存储 snapshot。
- gVisor/Kata runtime。
- 不直接挂真实项目。

## Task Runner 接入点

当前相关入口：

- `task_runner_service/backend/src/terminal_store/ops/controller_api/execute.rs`
- `task_runner_service/backend/src/terminal_store/ops/session_ops.rs`
- `task_runner_service/backend/src/services/builtin_providers/builders.rs`
- `task_runner_service/backend/src/services/run_model_phase/setup/preparation/mcp_builder.rs`
- `crates/chatos_builtin_tools/src/code_maintainer/`
- `crates/chatos_builtin_tools/src/terminal_controller.rs`

建议接入方式：

### 1. Run 开始前申请沙箱

在 run setup 阶段判断任务是否启用了这些 builtin：

- `TerminalController`
- `CodeMaintainerRead`
- `CodeMaintainerWrite`
- 未来任何本地文件/进程工具

如果启用了，则：

1. 创建 `.chatos/sandboxes/runs/{run_id}`。
2. 复制项目到 `input/workspace`。
3. 调用 Sandbox Manager `POST /api/sandboxes/leases`。
4. 把 `sandbox_id`、`lease_id`、`agent_endpoint`、`agent_token` 保存到 run runtime state。
5. 向 run event 写入 `sandbox_started`。

### 2. 构建 MCP provider 时代理到 sandbox-agent

新增：

```text
task_runner_service/backend/src/services/sandbox_tool_proxy.rs
```

它实现：

- `TerminalControllerStore`
- CodeMaintainer 文件读写 store 或一个新的 `SandboxCodeMaintainerProvider`

调用时不再访问本地文件系统，而是 HTTP 调用 `sandbox-agent /mcp` JSON-RPC。

### 3. Run 结束时释放沙箱

在 completion/cancel/failure 路径中：

1. 调用 agent 生成 diff。
2. 调用 Sandbox Manager release，要求 export result。
3. 输出到 `.chatos/sandboxes/runs/{run_id}/output/workspace`。
4. 生成 `.chatos/sandboxes/runs/{run_id}/diff/summary.json`。
5. 写 run event：`sandbox_exported`、`sandbox_destroyed`。

如果 release 失败：

- 写 `sandbox_release_failed`。
- 后台 cleanup job 继续尝试销毁。

## Chatos 主服务接入点

如果 Chatos 主服务也有内置终端：

- `chatos/backend/src/builtin/terminal_controller/actions/actions_execute.rs`
- `chatos/backend/src/services/terminal_manager/io_runtime.rs`

建议不要再启动宿主 shell。改为：

- 如果会话绑定了 Task Runner run，则复用 run 的 sandbox。
- 如果是独立聊天终端，则由 Chatos 主服务向 Sandbox Manager 申请独立 sandbox。
- UI 展示“沙箱终端”，而不是“宿主终端”。

MVP 可以先只改 Task Runner 任务执行路径，Chatos 主服务终端保留但标记为 unsafe/local-only，等任务路径稳定后再改。

## Workspace 复制策略

MVP 可以用 Rust 实现递归复制：

```rust
copy_workspace(source_root, run_workspace, CopyOptions {
    exclude_patterns: vec![
        ".chatos/sandboxes/**",
        "target/**",
        "node_modules/**",
        "dist/**",
        "build/**",
    ],
    preserve_git: true,
    preserve_symlinks: false,
    max_file_size_mb: 100,
    max_total_size_mb: 2048,
})
```

建议规则：

- 普通文件复制内容。
- 目录递归创建。
- symlink 默认复制为 symlink 元数据，但导入沙箱前校验目标是否在 workspace 内。
- 指向 workspace 外的 symlink 默认跳过，并记录 warning。
- 超大文件默认跳过，并记录 warning。
- `.git` 是否复制可配置：
  - 如果需要 git diff、git status，可以复制 `.git`。
  - 如果担心泄露 remote/token，可以只复制工作树，不复制 `.git/config` 中的敏感信息。

## Diff 生成策略

MVP 可用两种方式：

1. 如果复制了 `.git`，在 `output/workspace` 里运行 `git diff --stat` 和 `git diff`。
2. 通用方式：基于 `input/workspace` 和 `output/workspace` 做文件 hash 对比。

建议生成：

```json
{
  "run_id": "run_1",
  "created_at": "2026-06-30T00:00:00Z",
  "added": [
    { "path": "src/new.rs", "size": 1200, "sha256": "..." }
  ],
  "modified": [
    { "path": "src/main.rs", "old_sha256": "...", "new_sha256": "...", "size": 3400 }
  ],
  "deleted": [
    { "path": "old.txt" }
  ],
  "warnings": []
}
```

先不要自动写回真实项目。等 diff 展示和审核流程稳定后，再实现：

```http
POST /api/sandbox-results/{run_id}/apply
```

## 安全控制

MVP 必须做到：

- sandbox-agent 只监听沙箱内部或 manager 可访问的私有地址。
- agent 每个请求都校验 run token。
- token 有过期时间，绑定 `sandbox_id/run_id`。
- 沙箱无平台服务环境变量。
- 沙箱不挂 Docker socket。
- 沙箱不挂宿主 `/home`、`/root`、`/var/run`。
- 沙箱默认禁网，或者只允许访问代理。
- 命令执行有超时、输出上限、进程数上限。
- release 时必须 kill 进程树。
- 异常退出必须销毁沙箱。

后续增强：

- gVisor/Kata/microVM。
- egress proxy。
- 文件内容扫描。
- 租户级配额。
- 完整审计日志。
- sandbox 镜像签名和版本锁定。

## 日志和审计

每个 run 记录：

- 沙箱创建时间。
- 沙箱 backend。
- 镜像版本。
- agent 版本。
- workspace copy 规则。
- 文件数量和大小。
- 每次 MCP tool 调用名称、耗时、结果状态。
- 终端命令文本。
- 终端输出摘要。
- 资源使用。
- release/export 结果。
- diff summary。

日志路径：

```text
.chatos/sandboxes/runs/{run_id}/logs/
  manager.ndjson
  agent.ndjson
  tools.ndjson
  terminal.ndjson
```

注意：日志需要做敏感信息脱敏，避免模型或命令输出把 token 打进长期日志。

## 分阶段实施

### Phase 1：协议和目录结构

目标：先把数据结构和边界定下来。

任务：

1. 新增 `crates/chatos_sandbox_protocol`。
2. 定义：
   - `SandboxLeaseRequest`
   - `SandboxLeaseResponse`
   - `SandboxStatus`
   - `SandboxReleaseRequest`
   - `SandboxToolCallRequest`
   - `SandboxToolCallResponse`
   - `SandboxDiffSummary`
3. 新增 `.chatos/sandboxes/runs/{run_id}` 目录管理工具。
4. 实现 workspace copy exclude 规则。
5. 写单元测试：
   - 不复制 `.chatos/sandboxes`。
   - symlink 逃逸被跳过。
   - 超大文件被跳过。

### Phase 2：sandbox-agent MVP

目标：沙箱里有一个固定 server，可以操作 `/workspace`。

任务：

1. 新增 `crates/chatos_sandbox_agent` 或 `sandbox_agent` binary。
2. 实现 `/health`。
3. 实现 `/mcp` JSON-RPC。
4. 实现 `initialize`、`ping`、`tools/list`、`tools/call`。
5. 实现文件工具：
   - read_file
   - write_file
   - list_dir
   - search_text
   - apply_patch
6. 实现终端工具：
   - execute_command
   - process_list
   - process_poll
   - process_log
   - process_write
   - process_kill
7. 所有路径限制在 `/workspace`。
8. 所有命令 current_dir 限制在 `/workspace` 内。

### Phase 3：Sandbox Manager MVP

目标：能创建、运行、销毁一个沙箱。

任务：

1. 新增 `sandbox_manager_service/backend`。
2. 实现 HTTP API：
   - `POST /api/sandboxes/leases`
   - `POST /api/sandboxes/{id}/heartbeat`
   - `POST /api/sandboxes/{id}/release`
   - `DELETE /api/sandboxes/{id}`
   - `GET /api/sandboxes/{id}`
   - `GET /api/sandbox-pool/status`
3. 实现 `SandboxBackend` trait。
4. 实现 Docker backend。
5. 启动容器后等待 agent `/health`。
6. release 时导出 workspace 到 output。
7. destroy 时强制删除容器。

### Phase 4：沙箱池

目标：像线程池一样控制容量。

任务：

1. 实现 `SandboxPool`。
2. 支持配置：
   - max_active
   - max_pending
   - per_tenant_limit
   - per_user_limit
   - lease_ttl
3. 支持 pending queue。
4. 支持 heartbeat 过期清理。
5. 支持后台 cleanup worker。
6. 支持 pool status metrics。

MVP 不复用带用户数据的沙箱，只做并发池。

### Phase 5：Task Runner 接入

目标：任务启用终端/文件工具时自动走沙箱。

任务：

1. 在 run setup 判断是否需要沙箱。
2. 创建 run 工作区并复制项目。
3. 调 Sandbox Manager lease。
4. 保存 sandbox runtime state。
5. 新增 `SandboxTerminalControllerStore`。
6. 新增 `SandboxCodeMaintainerStore` 或 `SandboxBuiltinProvider`。
7. MCP builder 根据 runtime state 改用 sandbox provider。
8. run completion/cancel/failure 调 release。
9. run events 展示 sandbox 生命周期。

### Phase 6：结果导出和 diff

目标：任务结束能看到沙箱里的改动。

任务：

1. release 时 copy out `/workspace` 到 `output/workspace`。
2. 对比 input/output。
3. 生成：
   - `diff/summary.json`
   - `diff/changes.patch`
   - `diff/changed_files.json`
4. API 提供查看 diff。
5. UI 后续可以展示改动。

这一阶段仍不自动覆盖真实项目。

### Phase 7：安全加固

目标：从 MVP 走向云生产可用。

任务：

1. gVisor/Kata backend。
2. 禁网或 egress proxy。
3. token broker。
4. 镜像签名。
5. seccomp/AppArmor/SELinux profile。
6. 租户级资源配额。
7. 日志脱敏。
8. 异常沙箱隔离和节点清理。

## 验收标准

MVP 必须通过：

1. 能创建沙箱。
2. 能在沙箱中启动 `sandbox-agent`。
3. 能通过 manager 获得 agent endpoint。
4. 能在沙箱中读写项目副本文件。
5. 能在沙箱中执行终端命令。
6. 命令写出的文件只出现在副本中，不影响真实项目。
7. 任务结束能导出 output workspace。
8. 能生成 diff summary。
9. release 后容器/进程被销毁。
10. 心跳超时后自动清理。
11. 并发超过池上限时排队或拒绝。
12. symlink 指向项目外时不能读取外部文件。
13. 沙箱中无法访问宿主 `/home`、Docker socket、平台环境变量。

## 推荐 MVP 任务拆分

第一批 PR：

- `crates/chatos_sandbox_protocol`
- run 工作区目录和 workspace copy
- copy/exclude/symlink 测试

第二批 PR：

- `sandbox-agent`
- 文件工具
- 终端工具
- agent 路径安全测试

第三批 PR：

- `sandbox_manager_service`
- Docker backend
- lease/release/destroy API
- pool max active

第四批 PR：

- Task Runner 接入 lease
- terminal MCP 代理到 sandbox-agent
- run 结束 release

第五批 PR：

- code maintainer 文件 MCP 代理到 sandbox-agent
- diff/export
- run event 和 UI 展示基础信息

## 关键风险

- 项目复制可能很慢：需要 exclude、增量复制或对象存储 snapshot。
- Docker bind mount 只是 MVP：云生产不能只依赖普通 Docker 隔离。
- 沙箱池不能复用用户数据：否则会有跨租户泄漏风险。
- 自动写回真实项目风险高：先只导出 diff，不自动 apply。
- 长期后台进程必须可靠 kill：release/destroy 要杀整个进程树。
- 日志可能泄漏 token：需要脱敏和 token 最小化。

## 结论

这个方案可行，并且适合当前项目逐步落地。建议先做一个最小闭环：

1. `sandbox-agent`。
2. `sandbox-manager`。
3. Docker backend。
4. run 工作区复制到 `.chatos`。
5. Task Runner 终端 MCP 代理到 agent。
6. release 后导出 output 和 diff。
7. 沙箱池只做并发控制，不复用用户数据。

等这个闭环稳定后，再把 backend 从 Docker 升级到 gVisor/Kata，把结果从“只导出到 `.chatos`”升级到“可审核后写回真实项目”。
