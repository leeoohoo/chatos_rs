# Local Connector 命令审批 Agent 实施方案

## 1. 目标

在 `local_connector_client/core` 中新增一个本地命令审批 Agent。远端下发 shell 命令后，Local Connector Client 在真正执行前调用审批 Agent，结合本地项目上下文判断是否允许执行。

审批模式：

1. 请求审批：每次执行前创建本地待审批项，用户批准后执行。
2. 自动审批：每次执行前由 AI 判断，AI 可调用本地项目读文件和搜索工具，最后必须调用 `approval_decision` 返回审批结果。
3. 完全控制：直接放行命令执行，同时保留现有 workspace / path guard。

项目级白名单：

- 白名单按项目隔离。
- 用户或 AI 选择“始终允许”时，将命令加入当前项目白名单。
- 命中白名单的命令直接放行。

项目定义：

- Local Connector 的开放目录只是本机授权边界。
- ChatOS 在开放目录下选择的文件或路径形成项目锚点。
- 审批设置、白名单、Memory scope 都按项目锚点计算。

沙箱命令保持现有 sandbox 执行链路。

## 2. 需要覆盖的命令入口

所有本机 shell 执行入口在 spawn 进程或写入 shell stdin 前调用统一审批入口：

```rust
approval_service
    .approve(CommandApprovalRequest { ... })
    .await?
```

覆盖入口：

1. `terminal_exec_request`
   - `local_connector_client/core/src/terminal/exec/runner.rs`
2. ChatOS 交互式 PTY 输入
   - `local_connector_client/core/src/terminal/session/input.rs`
   - `local_connector_client/core/src/terminal/relay/control.rs`
3. Local MCP shell 命令
   - `local_connector_client/core/src/terminal/controller/store.rs`
   - `local_connector_client/core/src/terminal/controller/store/standalone.rs`
   - `local_connector_client/core/src/terminal/controller/store/reused.rs`
4. MCP `process_write`
   - `local_connector_client/core/src/terminal/controller/store/process/control.rs`

PTY 行为：

- 在用户提交完整命令行时拦截。
- 待审批期间向终端输出等待提示。
- 审批通过后把原始命令写入 PTY。
- 审批拒绝后输出拒绝原因。

## 3. 项目身份

新增项目级审批 key：

```rust
pub(crate) struct ApprovalProjectKey {
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) project_root_relative_path: String,
    pub(crate) project_anchor_relative_path: Option<String>,
}
```

远端命令上下文需要携带：

```text
request_id
owner_user_id
device_id
workspace_id
project_id
project_root_relative_path
project_anchor_relative_path
cwd
source
```

项目 key 计算：

- `project_id` 有值时作为主键的一部分。
- `project_root_relative_path` 表示项目根相对开放目录的位置。
- `project_anchor_relative_path` 表示 ChatOS 侧选择的项目锚点文件或路径。
- 缺少项目 metadata 时使用 `workspace_id + cwd` 推导临时 project key，并在 UI 中标记为未绑定项目。

## 4. 本地模块

新增模块：

```text
local_connector_client/core/src/approval/
  mod.rs
  types.rs
  settings.rs
  whitelist.rs
  fingerprint.rs
  command_context.rs
  risk.rs
  ai_agent.rs
  tools.rs
  pending.rs
  service.rs
```

模块职责：

- `service.rs`：统一审批入口，串联设置、白名单、人工审批、AI 审批、历史和 Memory。
- `types.rs`：审批请求、审批结果、项目 key、命令来源、审批模式等类型。
- `settings.rs`：项目级和全局审批设置。
- `whitelist.rs`：项目白名单匹配和写入。
- `fingerprint.rs`：命令规范化、hash、白名单 key。
- `command_context.rs`：解析 cwd、命令参数、影响路径、项目内相对路径。
- `risk.rs`：静态风险规则。
- `pending.rs`：本地待审批队列和 UI 事件。
- `ai_agent.rs`：调用 `chatos_ai_runtime` 发起自动审批，并通过 runtime 已有 Memory Engine 接入读取上下文、写入审批记录。
- `tools.rs`：实现 AI 可调用的读/搜工具和 `approval_decision`。

核心对象：

```rust
pub(crate) struct CommandApprovalService {
    state_path: PathBuf,
    state: Arc<RwLock<LocalState>>,
    pending: Arc<RwLock<BTreeMap<String, PendingApproval>>>,
    events: broadcast::Sender<ApprovalEvent>,
    ai_agent: ApprovalAiAgent,
}
```

返回值：

```rust
pub(crate) enum ApprovalDecision {
    Approved {
        source: ApprovalSource,
        whitelist_entry_id: Option<String>,
    },
    Denied {
        source: ApprovalSource,
        reason: String,
    },
    AskUser {
        reason: String,
    },
}
```

## 5. 本地状态

扩展 `LocalState`：

```rust
pub(crate) struct ApprovalState {
    pub(crate) default_mode: ApprovalMode,
    pub(crate) projects: Vec<ProjectApprovalState>,
    pub(crate) whitelist: Vec<CommandWhitelistEntry>,
    pub(crate) history: Vec<ApprovalHistoryEntry>,
    pub(crate) ai: ApprovalAiSettings,
    pub(crate) memory: ApprovalMemorySettings,
}
```

审批设置：

```rust
pub(crate) enum ApprovalMode {
    RequestApproval,
    AutoApproval,
    FullControl,
}

pub(crate) struct ProjectApprovalState {
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) mode: Option<ApprovalMode>,
    pub(crate) ai_enabled: bool,
    pub(crate) updated_at: String,
}
```

白名单：

```rust
pub(crate) struct CommandWhitelistEntry {
    pub(crate) id: String,
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) command_fingerprint: String,
    pub(crate) command_display: String,
    pub(crate) normalized_command: String,
    pub(crate) cwd_scope: WhitelistCwdScope,
    pub(crate) created_by: ApprovalSource,
    pub(crate) created_at: String,
    pub(crate) enabled: bool,
}
```

第一版白名单匹配策略：

- `match_type = exact`
- 命令规范化后精确匹配。
- `cwd_scope = project` 时在同项目内生效。
- `cwd_scope = cwd` 时只在同一项目相对 cwd 下生效。

## 6. 审批主流程

```text
收到远端命令
  -> 解析 CommandApprovalRequest
  -> 计算 ApprovalProjectKey
  -> 执行 workspace / cwd guard
  -> 规范化命令并计算 fingerprint
  -> 采集命令上下文和静态风险
  -> 查询项目白名单
  -> 按审批模式决策
  -> 写本地审批历史
  -> best-effort 写入 Memory Engine
  -> 返回 Approved / Denied / AskUser
```

模式分支：

```text
RequestApproval
  -> 创建 PendingApproval
  -> 等待本地用户批准或拒绝

AutoApproval
  -> 调用 ApprovalAiAgent
  -> approve: 执行
  -> deny: 拒绝
  -> ask_user: 创建 PendingApproval

FullControl
  -> 通过 workspace / cwd guard 后直接执行
```

“始终允许”处理：

- 用户选择“始终允许”时写入白名单。
- AI 返回 `remember_allow = true` 且风险为低风险稳定命令时写入白名单。
- 白名单写入后记录 `whitelist_entry_id`。

## 7. 自动审批 Agent

自动审批 Agent 使用 `crates/chatos_ai_runtime` 已抽象好的能力：

- 低温模型调用。
- 多轮工具调用循环。
- 自定义审批工具执行器。
- Memory Engine context 注入。
- Memory Engine 审批记录写入。
- 用户模型配置解析。

模型参数：

```text
temperature = 0.0 或 0.1
max_output_tokens = 600
max_iterations = 8
timeout_ms = 8000 到 15000
stream = false
```

AI 调用流程：

```text
构建审批 prompt
  -> 构造 ApprovalAgentMemory
  -> 注册 ApprovalToolExecutor
  -> 运行 AI turn
  -> 等待 approval_decision
  -> 读取 ApprovalDecisionSink
```

伪代码：

```rust
let model_config = model_config
    .with_temperature(Some(0.0))
    .with_max_output_tokens(Some(600))
    .with_instructions(Some(APPROVAL_SYSTEM_PROMPT.to_string()));

let tool_executor = ApprovalToolExecutor::new(
    project_root.clone(),
    read_only_code_tools_for_project(project_root.as_path())?,
    decision_sink.clone(),
);

let memory = build_approval_agent_memory(
    settings,
    owner_user_id,
    project_key,
    auth_state.access_token.as_deref(),
)?;

let decision = run_approval_ai_turn(ApprovalAiTurnInput {
    model_config,
    memory,
    tool_executor,
    request_id,
    prompt: approval_prompt,
    approval_record: build_approval_record(...),
}).await?;
```

## 8. AI 工具

`ApprovalToolExecutor.available_tools()` 暴露固定工具集：

```text
read_file_raw
read_file_range
list_dir
search_text
approval_decision
```

读/搜工具复用：

- `crates/chatos_builtin_tools/src/code_maintainer/registration_read.rs`
- `local_connector_client/core/src/mcp/tools/code.rs`
- `code_maintainer_service_for_root(project_root, ..., allow_writes=false, enable_read_tools=true, enable_write_tools=false)`

工具 root：

- root 固定为当前项目 root。
- path 参数按项目 root 归一化。
- 工具结果设置大小预算。
- 敏感文件路径返回风险摘要。

最终决策工具：

```json
{
  "name": "approval_decision",
  "description": "Return the final command approval decision.",
  "input_schema": {
    "type": "object",
    "properties": {
      "decision": { "type": "string", "enum": ["approve", "deny", "ask_user"] },
      "risk": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
      "reason": { "type": "string" },
      "affected_paths": { "type": "array", "items": { "type": "string" } },
      "remember_allow": { "type": "boolean" },
      "whitelist": {
        "type": "object",
        "properties": {
          "match_type": { "type": "string", "enum": ["exact"] },
          "command": { "type": "string" },
          "cwd_scope": { "type": "string", "enum": ["project", "cwd"] }
        },
        "additionalProperties": false
      }
    },
    "required": ["decision", "risk", "reason"],
    "additionalProperties": false
  }
}
```

AI prompt 要点：

```text
你是 Local Connector 的命令审批 agent。
只判断本次命令是否可以在用户本机项目目录执行。
你可以调用本地只读文件工具和搜索工具理解项目。
最后必须调用 approval_decision 返回 approve / deny / ask_user。
不确定时返回 ask_user。
高风险命令返回 deny 或 ask_user。
```

用户 prompt 输入：

- 命令原文和规范化命令。
- cwd、workspace、项目根、项目锚点。
- 静态风险摘要。
- 命令显式影响路径。
- 已读取的 manifest 摘要。
- Memory Engine 返回的项目审批偏好和历史摘要。

## 9. Memory Engine 接入

Approval Agent 通过 `chatos_ai_runtime` 现有 Memory Engine 接入能力完成记忆读取和审批记录写入。实现结构参考 `project_management_service/backend/src/services/environment_agent.rs` 里的 `ProjectAgentMemory`：业务层只封装 Memory 连接、项目记忆范围和当前会话 id，具体 compose / record writer 交给 runtime。

建议新增业务封装：

```rust
struct ApprovalAgentMemory {
    composer: MemoryContextComposer,
    writer: MemoryEngineRecordWriter,
    scope: MemoryScope,
    conversation_id: String,
}
```

构造逻辑：

```text
build_approval_agent_memory(settings, owner_user_id, project_key, access_token)
  -> 创建 Memory Engine client
  -> 构造 composer
  -> 构造 writer
  -> 构造项目级 scope
  -> 返回 ApprovalAgentMemory
```

项目记忆范围包含：

```text
owner_user_id
source_id = local_connector_approval
thread_id = local_connector_approval:{project_key_hash}
subject_id = local_connector_approval_project:{project_id_or_project_key_hash}
related_subject_ids = [project:{project_id}, workspace:{workspace_id}]
```

审批记录输入包含：

```json
{
  "record_type": "local_connector_approval_event",
  "event_type": "approval_decision",
  "request_id": "req_1",
  "project_key": {
    "project_id": "project_1",
    "workspace_id": "workspace_1",
    "project_root_relative_path": "apps/web",
    "project_anchor_relative_path": "apps/web/package.json"
  },
  "mode": "auto_approval",
  "decision": "approved",
  "decision_source": "ai",
  "risk": "low",
  "reason": "Runs project tests.",
  "command_redacted": "npm test",
  "command_sha256": "...",
  "cwd_relative_to_project": ".",
  "affected_paths": ["package.json"],
  "remember_allow": false,
  "whitelist_entry_id": null,
  "created_at": "2026-07-08T10:00:00Z"
}
```

记录内容限定：

- 脱敏命令摘要。
- 命令 hash。
- 审批模式、结果、来源、原因。
- 项目 key 和项目内相对 cwd。
- 白名单变更摘要。
- 设置变更摘要。

Memory 接入行为：

- 审批完成后先写本地 `history`。
- 项目审批记忆注入由 `chatos_ai_runtime` runtime 完成。
- 审批记录写入由 `chatos_ai_runtime` runtime 完成。
- 写入失败按 runtime 的 best-effort 策略处理。

## 10. Core API 与 UI

Core API：

```text
GET  /api/local/approval/settings
POST /api/local/approval/settings
GET  /api/local/approval/pending
POST /api/local/approval/pending/{id}/approve
POST /api/local/approval/pending/{id}/deny
GET  /api/local/approval/whitelist
POST /api/local/approval/whitelist
DELETE /api/local/approval/whitelist/{id}
GET  /api/local/approval/history
GET  /api/local/approval/events
```

前端新增“审批”页：

- 全局默认审批模式。
- 项目级审批模式。
- AI 自动审批开关和模型配置状态。
- Memory Engine 配置状态。
- 待审批队列。
- 白名单列表。
- 审批历史。

待审批项展示：

- 来源：ChatOS terminal / Local MCP / PTY。
- 项目、workspace、cwd。
- 命令原文和规范化命令。
- 静态风险等级和原因。
- 影响路径。
- 操作：允许一次、始终允许、拒绝。

## 11. 命令入口接入方案

### terminal exec

文件：`local_connector_client/core/src/terminal/exec/runner.rs`

接入点：

```rust
let approval = approval_service.approve(request).await?;
match approval {
    ApprovalDecision::Approved { .. } => run_command().await,
    ApprovalDecision::Denied { reason, .. } => return approval_denied(reason),
    ApprovalDecision::AskUser { .. } => wait_for_pending_result().await,
}
```

### Local MCP execute command

文件：`local_connector_client/core/src/terminal/controller/store.rs`

做法：

- 在 `execute_command` 统一审批。
- 审批通过后再进入 standalone / reused 执行。
- 历史记录附加审批字段。

### 交互式 PTY

文件：

- `local_connector_client/core/src/terminal/session/input.rs`
- `local_connector_client/core/src/terminal/relay/control.rs`

做法：

- 维护一行 pending buffer。
- 回车提交时解析命令。
- 调用审批服务。
- 审批通过后写入 PTY。
- 审批拒绝后输出提示并清理 buffer。

### process_write

文件：`local_connector_client/core/src/terminal/controller/store/process/control.rs`

做法：

- `submit=true` 或 data 含换行时按交互式命令处理。
- 普通输入继续写入 process。

## 12. 静态风险规则

低风险候选：

- `pwd`
- `ls`
- `cat` 项目内普通文件
- `git status`
- `git diff`
- `npm test`
- `cargo test`
- `pnpm lint`

中高风险候选：

- 删除、覆盖、批量改权限。
- 系统目录写入。
- 读取密钥、token、证书。
- 网络下载后执行。
- 上传文件或远程执行。
- Docker / kubectl / sudo 等高权限命令。

静态风险用于：

- 人工审批 UI 展示。
- AI prompt 输入。
- 自动审批前置拦截。
- `remember_allow` 安全校验。

## 13. 测试计划

单元测试：

- project key 计算。
- 命令规范化和 fingerprint。
- 白名单 exact match。
- cwd scope 隔离。
- 静态风险分类。
- `ApprovalToolExecutor.available_tools()` 固定工具集。
- `approval_decision` 参数校验。
- 审批记录输入脱敏和稳定 record id。

集成测试：

- `terminal_exec_request` 审批通过后执行。
- `terminal_exec_request` 审批拒绝后返回 `approval_denied`。
- Local MCP `execute_command` 命中统一审批入口。
- PTY 待审批期间命令未写入 shell。
- “始终允许”后同项目同命令命中白名单。
- 同一开放目录下多个项目白名单隔离。
- 自动审批可调用 read/search 工具并通过 `approval_decision` 返回结果。
- 自动审批未返回最终决策时进入人工审批。
- Memory Engine 不可用时审批流程按原结果完成。
- Memory Engine 可用时写入 `local_connector_approval_event`。

前端测试：

- 切换审批模式。
- 批准、始终允许、拒绝待审批项。
- 删除或禁用白名单。
- 查看审批历史。
- 查看 Memory Engine 配置状态。

## 14. 分阶段实施

### Phase 1：基础模型和人工审批

- 新增 `approval` Rust 模块。
- 扩展 `LocalState`。
- 实现 `ApprovalProjectKey`、审批设置、白名单、历史。
- 新增 Core API。
- 接入 `terminal_exec_request`。
- 前端新增“审批”页和待审批队列。
- 完成基础单元测试。

### Phase 2：覆盖本机 shell 入口

- 接入 Local MCP `execute_command`。
- 接入交互式 PTY。
- 接入 `process_write submit=true`。
- 命令历史增加审批字段。
- 增加入口覆盖测试。

### Phase 3：项目锚点和白名单隔离

- relay 命令上下文携带 project metadata。
- 按 `ApprovalProjectKey` 隔离项目设置和白名单。
- 支持未绑定项目的临时 scope。
- 覆盖同一开放目录多项目测试。

### Phase 4：Memory Engine

- 在 `ApprovalAiAgent` 中接入 AI runtime 的 Memory Engine 配置。
- 按项目 key 构造项目记忆范围。
- 按审批事件构造脱敏审批记录输入。
- 让 AI runtime 在自动审批 turn 中注入 Memory context 并写入审批记录。
- 自动审批前读取项目审批记忆摘要。

### Phase 5：AI 自动审批

- 实现 `ApprovalAiAgent`。
- 实现 `ApprovalToolExecutor`。
- 复用只读 code maintainer 工具。
- 实现 `approval_decision` 最终决策工具。
- 接入低温模型配置。
- 增加 mock AI 集成测试。

### Phase 6：审计和体验完善

- 审批历史筛选。
- 白名单禁用、删除、来源追踪。
- Memory Engine 状态展示。
- 自动审批统计和风险分布。
