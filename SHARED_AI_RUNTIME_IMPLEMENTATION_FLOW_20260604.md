# AI Runtime 与内置 MCP 共享化完整实施流程

日期：2026-06-04

## 0. 交付口径

本流程按“一次改到位”执行，不做最小闭环。

最终必须同时完成：

1. `chatos_ai_runtime` shared crate。
2. `chatos_mcp_runtime` shared crate。
3. `chatos_builtin_tools` shared crate。
4. `chat_app_server_rs` 切换到 shared runtime。
5. `task_runner_service/backend` 接入 shared runtime。
6. `task_runner_service/frontend` 完成 React + Ant Design 管理界面。
7. 两套系统都通过 Memory Engine SDK 管理上下文和执行记录。
8. 两套系统都通过同一套 MCP runtime 调用内置 MCP。

## 1. 实施顺序总览

按下面顺序做：

1. 建 workspace。
2. 新建 shared crates。
3. 复制 AI runtime 代码。
4. 复制 MCP runtime 代码。
5. 复制完整 builtin tools。
6. 抽 shared traits。
7. 写 chatos adapters。
8. chatos 切换 shared runtime。
9. 新建 Task Runner 后端。
10. Task Runner 接 Memory Engine。
11. Task Runner 接 shared AI runtime。
12. Task Runner 接 shared MCP runtime。
13. 新建 Task Runner 前端。
14. 联调完整任务执行链路。
15. 回归 chatos 原聊天和 MCP。
16. 清理旧重复代码。

## 2. Step 1：建立 Cargo Workspace

新增根目录 `Cargo.toml`：

```toml
[workspace]
members = [
  "chat_app_server_rs",
  "crates/chatos_ai_runtime",
  "crates/chatos_mcp_runtime",
  "crates/chatos_builtin_tools",
  "task_runner_service/backend"
]

resolver = "2"
```

调整：

1. 保留 `chat_app_server_rs/Cargo.toml` 的 package 信息。
2. 后续所有 cargo 命令使用 `-p` 指定 package。
3. 启动脚本继续可用，但内部命令改成 workspace package。

验收：

```text
cargo metadata
cargo check -p chat_app_server_rs
```

## 3. Step 2：新建 Shared Crates

新增目录：

```text
crates/chatos_ai_runtime/
crates/chatos_mcp_runtime/
crates/chatos_builtin_tools/
```

每个 crate 建：

```text
Cargo.toml
src/lib.rs
```

`chatos_ai_runtime` 依赖：

```toml
tokio
serde
serde_json
reqwest
futures
bytes
tokio-util
tracing
sha2
uuid
memory_engine_sdk
async-trait
```

`chatos_mcp_runtime` 依赖：

```toml
tokio
serde
serde_json
reqwest
futures
tracing
async-trait
```

`chatos_builtin_tools` 依赖：

```toml
tokio
serde
serde_json
tracing
regex
walkdir
reqwest
scraper
portable-pty
async-trait
chatos_mcp_runtime
```

验收：

```text
cargo check -p chatos_ai_runtime
cargo check -p chatos_mcp_runtime
cargo check -p chatos_builtin_tools
```

## 4. Step 3：复制 AI Runtime

从 `chat_app_server_rs` 复制到 `crates/chatos_ai_runtime/src/`：

```text
services/agent_runtime/ai_request_handler/*
services/agent_runtime/ai_client/*
services/ai_common/request_support/*
services/ai_common/stream_support.rs
services/ai_client_common.rs
core/tool_call.rs
core/tool_io.rs
core/messages.rs
utils/model_config.rs
```

目标结构：

```text
crates/chatos_ai_runtime/src/
  lib.rs
  request_handler/
  client/
  request_support/
  stream_support.rs
  callbacks.rs
  tool_call.rs
  tool_io.rs
  messages.rs
  model_support.rs
  traits.rs
  memory_context.rs
  runtime.rs
```

改名规则：

1. `AiRequestHandler` 保留名称。
2. `AiClient` 保留名称。
3. `ProcessOptions` 保留名称。
4. `AiClientCallbacks` 保留名称或迁为 `RuntimeCallbacks`。
5. `AiServer` 改成 shared `AiRuntime`。

必须替换的 `crate::` 依赖：

```text
crate::services::agent_runtime::...
crate::services::ai_common::...
crate::core::...
crate::utils::...
crate::modules::conversation_runtime::...
```

处理方式：

1. shared 内部模块直接改路径。
2. chatos-only 依赖改 trait。
3. task board follow-up 改 `TurnReviewPolicy`。
4. runtime guidance 改 `RuntimeInputProvider`。
5. abort registry 改 `CancellationProvider`。

验收：

```text
cargo check -p chatos_ai_runtime
```

## 5. Step 4：定义 AI Runtime Traits

在 `crates/chatos_ai_runtime/src/traits.rs` 定义：

```rust
pub trait MemoryScopeResolver;
pub trait MemoryRecordWriter;
pub trait ToolExecutor;
pub trait RuntimeInputProvider;
pub trait RunEventSink;
pub trait CancellationProvider;
pub trait TurnReviewPolicy;
pub trait ModelRuntimeResolver;
```

关键数据结构：

```rust
pub struct MemoryScope;
pub struct RuntimeTurnContext;
pub struct SaveRecordInput;
pub struct SaveAssistantRecordInput;
pub struct SaveToolRecordInput;
pub struct ToolExecutionContext;
pub struct RuntimeModelConfig;
pub struct AiRuntimeOptions;
pub struct AiRuntimeResult;
```

实现要求：

1. `AiRuntime` 不依赖 chatos models。
2. `AiRuntime` 不依赖 chatos repositories。
3. `AiRuntime` 不依赖 SSE/websocket。
4. `AiRuntime` 直接调用 Memory Engine SDK `compose_context`。
5. `AiRuntime` 只通过 `MemoryRecordWriter` 写 records。

验收：

```text
cargo check -p chatos_ai_runtime
```

## 6. Step 5：实现 Memory Engine Context Compose

在 `crates/chatos_ai_runtime/src/memory_context.rs` 实现：

1. `compose_runtime_context(...)`
2. `memory_blocks_to_input_items(...)`
3. `engine_records_to_input_items(...)`
4. `engine_record_to_runtime_message_item(...)`
5. tool call / tool output record 转换。

调用：

```rust
MemoryEngineClient::compose_context(&SdkComposeContextRequest {
    tenant_id,
    subject_id,
    related_subject_ids,
    thread_id,
    policy,
})
```

默认 policy：

```text
include_recent_records = true
include_thread_summary = true
include_subject_memory = true
recent_record_limit = None
summary_limit = None
```

验收：

1. 输入 `MemoryScope` 能得到模型 input items。
2. `blocks` 进入 system/developer items。
3. `recent_records` 进入 user/assistant/tool items。
4. tool call 和 tool output 能保持上下文连续。

## 7. Step 6：复制 MCP Runtime

从 `chat_app_server_rs` 复制到 `crates/chatos_mcp_runtime/src/`：

```text
services/mcp_execution_core/*
services/mcp_tool_execute_shared.rs
core/mcp_tools/*
core/mcp_args.rs
services/mcp_loader.rs 中的 server 类型
services/builtin_mcp.rs 中的 kind/常量
```

目标结构：

```text
crates/chatos_mcp_runtime/src/
  lib.rs
  executor/
  registry.rs
  transport/
  tools/
  builtin.rs
  args.rs
  types.rs
```

改造：

1. `McpExecutorCore` 改为 public `McpExecutor`。
2. `McpHttpServer`、`McpStdioServer`、`McpBuiltinServer` 迁入 `types.rs`。
3. `BuiltinToolService` enum 改为 `Arc<dyn BuiltinToolProvider>`。
4. `build_builtin_tool_service` 删除，改为 registry 注入。
5. 保留 HTTP MCP、stdio MCP、builtin MCP 三类执行。
6. 保留 parallel policy。
7. 保留 Codex gateway passthrough tool schema。

验收：

```text
cargo check -p chatos_mcp_runtime
```

## 8. Step 7：定义 Builtin Tool Provider Registry

在 `chatos_mcp_runtime` 中实现：

```rust
pub trait BuiltinToolProvider;
pub struct BuiltinToolRegistry;
pub struct BuiltinToolDefinition;
pub struct ToolCallContext;
pub type ToolStreamChunkCallback;
```

registry 必须支持：

1. 按 server name 注册 provider。
2. list tools。
3. call tool。
4. unavailable tools。
5. tool metadata。
6. capability metadata。
7. stream chunk callback。

验收：

1. provider 可注册。
2. provider 工具可出现在 `available_tools`。
3. provider 工具可被 `execute_tools_stream` 调用。
4. 不存在 provider 时返回明确错误。

## 9. Step 8：复制完整 Builtin Tools

从 `chat_app_server_rs/src/builtin` 复制到 `crates/chatos_builtin_tools/src/`：

```text
agent_builder/
browser_tools/
code_maintainer/
memory_command_reader/
memory_plugin_reader/
memory_skill_reader/
notepad/
remote_connection_controller/
task_manager/
terminal_controller/
ui_prompter/
web_tools/
browser_runtime.rs
browser_page_state_view.rs
browser_page_insights.rs
browser_command_support.rs
research_*.rs
```

目标结构：

```text
crates/chatos_builtin_tools/src/
  lib.rs
  registry.rs
  backends.rs
  web_tools/
  browser_tools/
  code_maintainer/
  terminal_controller/
  remote_connection_controller/
  task_manager/
  notepad/
  agent_builder/
  ui_prompter/
  memory_skill_reader/
  memory_command_reader/
  memory_plugin_reader/
```

复制后处理：

1. 工具 schema 保持不变。
2. 工具名称保持不变。
3. 参数结构保持不变。
4. 输出结构保持不变。
5. chatos-only 依赖替换为 backend trait。

验收：

```text
cargo check -p chatos_builtin_tools
```

## 10. Step 9：定义 Builtin Tool Backends

在 `crates/chatos_builtin_tools/src/backends.rs` 定义：

```rust
pub trait TaskToolBackend;
pub trait NotepadBackend;
pub trait AgentBackend;
pub trait AgentMemoryBackend;
pub trait UiPromptBackend;
pub trait RemoteConnectionBackend;
pub trait TerminalBackend;
pub trait BrowserBackend;
pub trait WorkspaceBackend;
```

映射：

```text
task_manager -> TaskToolBackend
notepad -> NotepadBackend
agent_builder -> AgentBackend
memory_*_reader -> AgentMemoryBackend
ui_prompter -> UiPromptBackend
remote_connection_controller -> RemoteConnectionBackend
terminal_controller -> TerminalBackend / WorkspaceBackend
browser_tools -> BrowserBackend / WorkspaceBackend
code_maintainer -> WorkspaceBackend
```

要求：

1. chatos 提供完整 backend 实现。
2. Task Runner 提供自己的 backend 实现。
3. Task Runner 暂无业务能力的 backend 也要实现，返回 unavailable reason。
4. 不允许 shared builtin tools 直接访问 chatos repositories。

验收：

```text
cargo check -p chatos_builtin_tools
```

## 11. Step 10：实现 Chatos Adapters

在 `chat_app_server_rs/src/adapters/` 新增：

```text
ai_runtime_memory_scope.rs
ai_runtime_memory_writer.rs
ai_runtime_input_provider.rs
ai_runtime_events.rs
ai_runtime_cancellation.rs
ai_runtime_turn_review.rs
mcp_builtin_registry.rs
builtin_backends/
```

实现：

```text
ChatosMemoryScopeResolver
ChatosMemoryRecordWriter
ChatosRuntimeInputProvider
ChatosRunEventSink
ChatosCancellationProvider
ChatosTaskTurnReviewPolicy
ChatosBuiltinRegistryFactory
ChatosTaskToolBackend
ChatosNotepadBackend
ChatosAgentBackend
ChatosAgentMemoryBackend
ChatosUiPromptBackend
ChatosRemoteConnectionBackend
ChatosTerminalBackend
ChatosBrowserBackend
ChatosWorkspaceBackend
```

关键映射：

```text
session_id -> build_thread_mapping(session)
messages -> memory_engine_sdk batch_sync_records
runtime guidance -> 原 guidance 模块
task board prompt -> 原 task_board 模块
SSE/websocket -> 原 realtime/event sink
abort -> 原 abort_registry
```

验收：

```text
cargo check -p chat_app_server_rs
```

## 12. Step 11：Chatos 切换 AI Runtime

修改入口：

```text
chat_app_server_rs/src/services/agent_runtime/ai_server.rs
chat_app_server_rs/src/modules/conversation_runtime/chat_execution.rs
chat_app_server_rs/src/modules/conversation_runtime/chat_runner.rs
chat_app_server_rs/src/modules/conversation_runtime/chat_usecase.rs
```

处理：

1. 原 `AiServer` 包装 shared `AiRuntime`。
2. 原 `AiClient` 调用替换为 shared `AiRuntime::run_turn(...)`。
3. 原 `McpToolExecute` 替换为 shared `McpExecutor`。
4. 原 callbacks 转 `ChatosRunEventSink`。
5. 原 task board refresh context 转 `ChatosRuntimeInputProvider`。
6. 原 message manager 写消息转 `ChatosMemoryRecordWriter`。

保持：

1. API request/response 不变。
2. SSE event 格式不变。
3. message metadata 不变。
4. tool message 持久化不变。
5. runtime snapshot 不变。

验收：

1. 普通联系人聊天可用。
2. 带 Memory Engine 上下文聊天可用。
3. 带内置 MCP 聊天可用。
4. tool call 后能继续模型回答。
5. SSE thinking/chunk/tool events 正常。

## 13. Step 12：Chatos 切换 MCP Runtime

修改：

```text
chat_app_server_rs/src/services/agent_runtime/mcp_tool_execute.rs
chat_app_server_rs/src/services/mcp_tool_execute_shared.rs
chat_app_server_rs/src/core/mcp_runtime.rs
chat_app_server_rs/src/core/mcp_tools/builtin.rs
```

处理：

1. 删除本地 `McpExecutorCore` 逻辑，改 re-export shared executor。
2. `load_mcp_servers_by_selection(...)` 继续留在 chatos。
3. `McpBuiltinServer` 使用 shared 类型。
4. `BuiltinMcpKind` 使用 shared 类型或字符串 kind。
5. `mcp_builtin_registry.rs` 构造所有 chatos builtin providers。

验收：

1. MCP 面板工具列表不变。
2. HTTP MCP 可用。
3. stdio MCP 可用。
4. 内置 MCP 可用。
5. unavailable tools 原因可见。

## 14. Step 13：新建 Task Runner Backend

目录：

```text
task_runner_service/backend/
  Cargo.toml
  src/
    main.rs
    config.rs
    state.rs
    api/
    db/
    models/
    repositories/
    services/
    adapters/
```

依赖：

```toml
axum
tokio
serde
serde_json
sqlx
uuid
chrono
tracing
tower-http
memory_engine_sdk
chatos_ai_runtime
chatos_mcp_runtime
chatos_builtin_tools
```

API：

```text
GET    /api/tasks
POST   /api/tasks
GET    /api/tasks/:id
PATCH  /api/tasks/:id
DELETE /api/tasks/:id

GET    /api/model-configs
POST   /api/model-configs
GET    /api/model-configs/:id
PATCH  /api/model-configs/:id
DELETE /api/model-configs/:id

POST   /api/tasks/:id/runs
GET    /api/tasks/:id/runs
GET    /api/runs/:id
GET    /api/runs/:id/events
POST   /api/runs/:id/cancel

GET    /api/mcp/tools
PATCH  /api/tasks/:id/mcp
```

验收：

```text
cargo check -p task_runner_service_backend
```

## 15. Step 14：Task Runner 数据表

新增迁移：

```text
task_runner_tasks
task_runner_model_configs
task_runner_runs
task_runner_run_events
task_runner_task_mcp_configs
task_runner_tool_snapshots
```

核心字段：

```text
task_runner_tasks:
  id
  title
  description
  objective
  input_payload_json
  status
  priority
  tags_json
  default_model_config_id
  memory_thread_id
  tenant_id
  subject_id
  created_at
  updated_at
  deleted_at

task_runner_model_configs:
  id
  name
  provider
  base_url
  api_key
  model
  temperature
  max_output_tokens
  thinking_level
  supports_responses
  supports_images
  enabled
  created_at
  updated_at

task_runner_runs:
  id
  task_id
  model_config_id
  memory_thread_id
  status
  started_at
  finished_at
  input_snapshot_json
  tool_snapshot_json
  result_summary
  error_message
  usage_json
  created_at
  updated_at

task_runner_run_events:
  id
  run_id
  event_type
  payload_json
  created_at
```

验收：

1. migrations 可执行。
2. CRUD repository 可用。
3. run event 可写入和分页读取。

## 16. Step 15：Task Runner 接 Memory Engine

实现：

```text
TaskRunnerMemoryScopeResolver
TaskRunnerMemoryRecordWriter
TaskRunnerMemoryThreadService
```

创建任务时：

1. 生成 `task_id`。
2. 生成 `memory_thread_id = task:{task_id}`。
3. 调 Memory Engine `upsert_thread`。
4. 写 metadata：

```json
{
  "app": "task_runner_service",
  "task_id": "...",
  "thread_type": "task"
}
```

执行任务时：

1. resolver 返回：

```text
tenant_id = task.tenant_id
source_id = task_runner_service
thread_id = task.memory_thread_id
subject_id = task.subject_id
related_subject_ids = task labels/project/contact/agent/workspace
```

2. writer 写入：

```text
user record
assistant record
tool record
process record
```

验收：

1. Memory Engine 中能看到 task thread。
2. 能 compose_context。
3. 能看到 run 产生的 records。
4. 上下文压缩后下一次执行可用。

## 17. Step 16：Task Runner 接 Shared AI Runtime

实现：

```text
TaskRunnerRuntimeInputProvider
TaskRunnerRunEventSink
TaskRunnerCancellationProvider
TaskRunnerTaskRunReviewPolicy
TaskRunnerModelRuntimeResolver
```

执行服务：

```text
TaskExecutionService::run_task(task_id, requested_by)
```

流程：

1. 创建 `TaskRun`。
2. 读取任务。
3. 读取模型配置。
4. 创建 Memory Engine user record。
5. 构造 shared `AiRuntime`。
6. 注入：
   - model config
   - memory scope resolver
   - memory record writer
   - runtime input provider
   - MCP executor
   - event sink
   - review policy
7. 调 `run_turn(...)`。
8. 写 assistant record。
9. 写 run events。
10. 更新 run status。
11. 更新 task status。

验收：

1. 点击执行能调用模型。
2. chunk/thinking/tool events 写入 run events。
3. 模型能调用 MCP。
4. 执行完成写 result summary。
5. 执行失败写 error_message。

## 18. Step 17：Task Runner 接 Shared MCP Runtime

实现：

```text
TaskRunnerBuiltinRegistryFactory
TaskRunnerTaskToolBackend
TaskRunnerNotepadBackend
TaskRunnerUiPromptBackend
TaskRunnerWorkspaceBackend
TaskRunnerBrowserBackend
TaskRunnerTerminalBackend
TaskRunnerRemoteConnectionBackend
TaskRunnerAgentMemoryBackend
```

注册 provider：

```text
web_tools
browser_tools
code_maintainer_read
code_maintainer_write
terminal_controller
remote_connection_controller
task_manager
notepad
agent_builder
ui_prompter
memory_skill_reader
memory_command_reader
memory_plugin_reader
```

权限：

```text
allow_writes
allow_terminal
allow_browser
allow_remote_connection
allow_ui_prompt
allow_agent_memory
```

验收：

1. `/api/mcp/tools` 返回完整工具列表和 unavailable reason。
2. task run tool snapshot 保存启用工具。
3. task_manager 工具能修改 Task Runner 自己的任务。
4. code/web/browser/terminal 类工具按权限执行。
5. 未授权工具返回明确错误。

## 19. Step 18：新建 Task Runner Frontend

目录：

```text
task_runner_service/frontend/
  package.json
  src/
    main.tsx
    App.tsx
    api/
    pages/
    components/
```

技术：

```text
React
TypeScript
Vite
Ant Design
TanStack Query
```

页面：

```text
/tasks
/tasks/:id
/model-configs
/runs/:id
/settings/mcp
```

功能：

1. 任务列表。
2. 新建/编辑/删除任务。
3. 模型配置列表。
4. 新建/编辑/删除模型配置。
5. 任务 MCP 权限配置。
6. 点击执行任务。
7. 运行历史。
8. 运行事件 Timeline。
9. 工具调用详情。
10. 执行结果摘要。

验收：

```text
npm run type-check
npm run build
```

## 20. Step 19：联调完整任务执行

联调脚本：

1. 创建模型配置。
2. 创建任务。
3. 配置任务 MCP 权限。
4. 点击执行。
5. 观察 run status。
6. 观察 run events。
7. 确认 Memory Engine thread。
8. 确认 user/assistant/tool records。
9. 再次执行同一任务。
10. 确认第二次执行能读取第一次上下文。

验收：

1. `queued -> running -> succeeded` 正常。
2. 模型调用正常。
3. 工具调用正常。
4. Memory Engine 上下文正常。
5. 前端展示正常。

## 21. Step 20：回归 Chatos

必须回归：

1. 普通联系人聊天。
2. 联系人带项目聊天。
3. 联系人带 Memory Engine 上下文聊天。
4. 联系人带系统上下文聊天。
5. 联系人启用技能聊天。
6. 联系人启用命令聊天。
7. 内置 task_manager MCP。
8. 内置 code_maintainer MCP。
9. 内置 terminal MCP。
10. 内置 browser MCP。
11. 内置 web_tools MCP。
12. 内置 notepad MCP。
13. 内置 ui_prompter MCP。
14. 内置 agent_builder MCP。
15. remote connection controller MCP。
16. memory skill/command/plugin reader。
17. HTTP MCP。
18. stdio MCP。
19. Codex gateway MCP passthrough。
20. tool call 后继续模型回答。

验收：

```text
cargo check -p chat_app_server_rs
cargo test -p chat_app_server_rs
cargo test -p chatos_ai_runtime
cargo test -p chatos_mcp_runtime
cargo test -p chatos_builtin_tools
```

## 22. Step 21：清理旧重复代码

清理原则：

1. chatos 本地旧 AI runtime 只保留 adapter/wrapper。
2. chatos 本地旧 MCP executor 只保留 adapter/wrapper。
3. chatos 本地 builtin 工具实现迁入 shared 后，原路径改 re-export 或删除。
4. 不删除 API、repository、models。
5. 不删除 chatos conversation runtime 编排。

清理目标：

```text
services/agent_runtime/*
services/mcp_execution_core/*
services/mcp_tool_execute_shared.rs
core/mcp_tools/*
builtin/*
```

处理方式：

1. 如果前端/API 仍引用旧路径，旧路径保留薄 wrapper。
2. 如果没有引用，删除旧实现。
3. 测试同步迁移到 shared crates。

验收：

1. `rg` 确认没有重复核心实现。
2. workspace 全量 check 通过。
3. chatos 和 Task Runner 都能启动。

## 23. 最终验收清单

### 23.1 Workspace

```text
cargo metadata
cargo check --workspace
cargo test --workspace
```

### 23.2 Chatos

```text
chat_app_server_rs 原聊天能力可用
原内置 MCP 全部可用
HTTP/stdio MCP 可用
Memory Engine 上下文可用
前端 SSE/工具事件可用
```

### 23.3 Task Runner Backend

```text
任务 CRUD 可用
模型配置 CRUD 可用
任务执行可用
Memory Engine records 可用
MCP 工具调用可用
TaskRun events 可用
```

### 23.4 Task Runner Frontend

```text
任务列表可用
任务编辑可用
模型配置可用
点击执行可用
运行详情可用
工具调用详情可用
```

### 23.5 共享代码

```text
AI 对话逻辑只有 chatos_ai_runtime 一份
MCP 执行逻辑只有 chatos_mcp_runtime 一份
内置 MCP 工具实现只有 chatos_builtin_tools 一份
chatos 和 Task Runner 都通过 Cargo.toml 引入 shared crates
```

