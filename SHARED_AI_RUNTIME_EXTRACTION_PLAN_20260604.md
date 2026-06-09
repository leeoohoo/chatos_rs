# AI 对话与内置 MCP 共享运行时拆分方案

日期：2026-06-04

## 1. 结论

可以把“和模型对话 + 工具循环 + MCP 执行 + 内置 MCP 注册”拆成独立 Rust crate，让 chatos 和新的 Task Runner 都通过 `Cargo.toml` 引入。

本次需求口径调整为：一次改到位，不做最小闭环，不按 MVP 分批上线。实现策略采用“复制优先、边界改造”的方式：

1. 大部分现有 AI runtime、MCP executor、builtin tool 代码可以直接复制到 shared crates。
2. 对 `crate::models`、`crate::services`、`crate::repositories`、实时事件、前端交互这类 chatos 专属依赖，改成 trait / adapter / provider 注入。
3. chatos 和 Task Runner 都接最终共享 runtime，不保留两套 AI 对话逻辑。
4. Task Runner 第一版就按完整运行形态接入：模型配置、任务 CRUD、手动执行、Memory Engine 上下文、AI 多轮工具调用、内置 MCP、执行事件、运行记录。

这里不是说“少抽一点”，而是不要把 `chat_app_server_rs` 整个作为依赖拖进新系统。原因是现在代码里混有三类职责：

1. 可共享的 AI runtime：
   - OpenAI-compatible 请求组包。
   - Responses / Chat Completions 流式解析。
   - 工具调用循环。
   - tool call 输入输出拼接。
   - MCP HTTP / stdio / builtin 执行核心。

2. chatos 专属业务：
   - 联系人、项目、session、runtime snapshot。
   - 用户设置、实时 SSE / websocket 事件。
   - task board follow-up。
   - abort registry。
   - chatos 数据表与 repository。

3. 可共享但需要适配的边界：
   - Memory Engine scope 解析。
   - Memory Engine records 写入。
   - 内置 MCP 工具注册。
   - 模型配置解析。
   - 任务执行事件回调。

所以推荐方案是：

```text
crates/
  chatos_ai_runtime/
  chatos_mcp_runtime/
  chatos_builtin_tools/

chat_app_server_rs/
task_runner_service/backend/
```

其中 `chat_app_server_rs` 和 `task_runner_service/backend` 都依赖这些 shared crates，但 shared crates 不反向依赖 chatos server。

## 2. 当前项目现状

### 2.1 AI runtime 入口

当前 AI 对话入口主要在：

```text
chat_app_server_rs/src/services/agent_runtime/ai_server.rs
chat_app_server_rs/src/services/agent_runtime/ai_client/mod.rs
chat_app_server_rs/src/services/agent_runtime/ai_client/request_flow.rs
chat_app_server_rs/src/services/agent_runtime/ai_client/execution_loop.rs
chat_app_server_rs/src/services/agent_runtime/ai_request_handler/mod.rs
chat_app_server_rs/src/services/agent_runtime/ai_request_handler/stream_request.rs
```

`AiServer` 当前组合了：

```text
MessageManager
AiRequestHandler
McpToolExecute
AiClient
```

这说明现在运行时已经有比较清晰的内部结构，但 `MessageManager` 和 `McpToolExecute` 仍然是 chatos 里的具体实现。

### 2.2 上下文依赖

`MessageManager` 依赖：

```text
crate::models::message::Message
crate::models::session_summary_v2::SessionSummaryV2
crate::services::message_manager_common::MessageManagerCore
crate::services::chatos_sessions
crate::services::chatos_memory_engine
```

`build_stateless_items(...)` 已经在使用 Memory Engine 组合上下文，当前只是入口还先通过 chatos 的 `Session` 推导 Memory Engine scope。

这里不需要把上下文抽象成“给我 summary + pending messages”的接口。更好的做法是让共享 runtime 直接使用 Memory Engine SDK 的 `compose_context`，两个系统只负责换自己的系统身份和 scope key：

```text
tenant_id
source_id 或 system key 对应的系统身份
thread_id
subject_id
related_subject_ids
policy
```

也就是说，共享 runtime 不重新定义上下文存储模型，只消费 Memory Engine 已经返回的 `blocks` 和 `recent_records`。

### 2.3 MCP 执行核心

当前 MCP 相关代码：

```text
chat_app_server_rs/src/services/agent_runtime/mcp_tool_execute.rs
chat_app_server_rs/src/services/mcp_tool_execute_shared.rs
chat_app_server_rs/src/services/mcp_execution_core/*
chat_app_server_rs/src/core/mcp_tools/*
chat_app_server_rs/src/core/mcp_runtime.rs
chat_app_server_rs/src/core/mcp_tools/builtin.rs
```

其中 `McpExecutorCore` 已经相对独立，支持：

1. HTTP MCP。
2. stdio MCP。
3. builtin MCP。
4. 工具 schema 构建。
5. 工具调用。
6. 并行工具调用策略。
7. Codex gateway MCP passthrough 工具描述。

这是适合抽出去的部分。

### 2.4 内置 MCP 现状

当前内置 MCP 包括：

```text
CodeMaintainerRead
CodeMaintainerWrite
TerminalController
TaskManager
Notepad
AgentBuilder
UiPrompter
RemoteConnectionController
WebTools
BrowserTools
MemorySkillReader
MemoryCommandReader
MemoryPluginReader
```

问题是 `BuiltinToolService` 枚举直接引用了 chatos 的十几个 builtin service。这些 service 里有些比较通用，有些强依赖 chatos 的数据表、会话和实时交互。

因此内置 MCP 可以共享，但需要拆成“注册协议 + 工具实现包 + app 注入适配器”，不能让新的 Task Runner 被迫依赖整个 chatos server。

### 2.5 复制优先迁移规则

这次不按重写思路做，按复制迁移做：

1. 能直接复制的代码直接复制到 shared crates，保持文件结构和函数行为尽量不变。
2. 编译失败时优先处理 `crate::...` 路径和可见性，不做业务重写。
3. 遇到 chatos 专属依赖时，只在依赖边界抽 trait：
   - 数据库 repository。
   - Memory Engine scope 推导。
   - realtime / SSE / websocket。
   - UI prompt 等前端交互。
   - 用户、联系人、项目、agent、task board 等业务对象。
4. 原有测试能迁的迁到 shared crate，不能迁的留在 chatos adapter 层。
5. shared crate 的 public API 以当前 chatos 调用方式为基准，先保证 chatos 行为不变，再让 Task Runner 接同一 API。

复制目录优先级：

```text
直接复制：
  services/agent_runtime/ai_request_handler/*
  services/agent_runtime/ai_client/*
  services/mcp_execution_core/*
  services/mcp_tool_execute_shared.rs
  core/mcp_tools/*
  core/tool_call.rs
  core/tool_io.rs
  services/ai_common/request_support/*
  services/ai_common/stream_support.rs

复制后做 adapter：
  builtin/*
  services/agent_runtime/message_manager.rs
  core/mcp_runtime.rs
  core/builtin_mcp_prompt.rs
  modules/conversation_runtime/task_board.rs

留在 chatos adapter：
  modules/conversation_runtime/*
  repositories/*
  models/*
  services/realtime/*
  api/*
```

## 3. 推荐 crate 拆分

### 3.1 `chatos_ai_runtime`

职责：

1. AI 请求组包。
2. Responses / Chat Completions transport。
3. SSE 流式解析。
4. tool call 解析和回填。
5. AI + tool 多轮循环。
6. retry / backpressure / empty response recovery。
7. callback 事件模型。
8. Memory Engine context compose 结果到模型 input items 的转换。

建议迁入：

```text
services/agent_runtime/ai_request_handler/*
services/agent_runtime/ai_client/*
services/ai_common/request_support/*
services/ai_common/stream_support.rs
core/chat_stream/*
core/tool_call.rs
core/tool_io.rs
core/messages.rs 中与 runtime 无关的纯函数
utils/model_config.rs 中 provider normalize / thinking mode 纯函数
```

需要改造：

1. `MessageManager` 的读上下文逻辑改为 Memory Engine scope + `compose_context`。
2. `McpToolExecute` 改为 trait 或 `Arc<dyn ToolExecutor>`。
3. `abort_registry` 改为 `CancellationProvider` trait。
4. task board follow-up 迁入共享 runtime 的 `TurnReviewPolicy` 扩展点，chatos 默认启用原策略。
5. `TaskBoardRefreshContextStore` 改为 `RuntimeGuidanceProvider` trait。

### 3.2 `chatos_mcp_runtime`

职责：

1. MCP server 类型定义。
2. MCP 工具 schema 构建。
3. MCP HTTP 调用。
4. MCP stdio 调用。
5. builtin tool registry。
6. tool metadata。
7. tool result 结构。
8. 并行执行策略。

建议迁入：

```text
services/mcp_execution_core/*
services/mcp_tool_execute_shared.rs
core/mcp_tools/*
core/mcp_args.rs
```

需要改造：

1. `BuiltinToolService` 从 enum 改为 trait object。
2. `build_builtin_tool_service(server)` 改成外部注入 registry。
3. `McpBuiltinServer` 不直接依赖 chatos 的 `BuiltinMcpKind`，改成字符串 kind 或共享 enum。
4. 内置工具按 feature 或独立模块注册。

推荐接口：

```rust
#[async_trait::async_trait]
pub trait BuiltinToolProvider: Send + Sync {
    fn server_name(&self) -> &str;
    fn list_tools(&self) -> Vec<serde_json::Value>;

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        context: ToolCallContext,
        stream: Option<ToolStreamChunkCallback>,
    ) -> Result<serde_json::Value, String>;

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}
```

### 3.3 `chatos_builtin_tools`

职责：

把现有内置 MCP 工具实现一次性沉淀出来。不是只迁一部分；本轮目标是让 chatos 当前所有内置 MCP 都能通过 shared registry 注册。

建议迁入：

```text
web_tools
browser_tools
code_maintainer_read
code_maintainer_write
terminal_controller
remote_connection_controller
notepad
task_manager
ui_prompter
agent_builder
memory_skill_reader
memory_command_reader
memory_plugin_reader
```

迁移方式：

1. 纯能力工具直接复制到 `chatos_builtin_tools`：
   - web tools
   - code maintainer
   - browser tools 的通用部分
   - terminal controller 的进程控制核心

2. 依赖 chatos 业务数据的工具也迁入，但把业务访问改成 adapter：
   - `task_manager` 通过 `TaskToolBackend` 访问任务仓储。
   - `notepad` 通过 `NotepadBackend` 访问笔记仓储。
   - `agent_builder` 通过 `AgentBackend` 访问 agent 仓储。
   - `memory_skill_reader` / `memory_command_reader` / `memory_plugin_reader` 通过 `AgentMemoryBackend` 访问联系人能力。
   - `ui_prompter` 通过 `UiPromptBackend` 访问实时交互通道。
   - `remote_connection_controller` 通过 `RemoteConnectionBackend` 访问远程连接。

3. Task Runner 不复用 chatos 的任务表实现，但复用同一个 `task_manager` 工具协议和 provider 框架：
   - chatos 注入 `ChatosTaskToolBackend`。
   - Task Runner 注入 `TaskRunnerTaskToolBackend`。
   - 对模型暴露的工具语义保持一致，背后落库不同。

## 4. 共享 runtime 的关键 trait 边界

### 4.1 Memory Engine scope 与消息写入

上下文读取不建议走 `load_context -> summary + pending messages` 这种二次抽象。Memory Engine SDK 已经提供：

```rust
client.compose_context(&SdkComposeContextRequest {
    tenant_id,
    subject_id,
    related_subject_ids,
    thread_id,
    policy,
})
```

返回结果里有：

```text
blocks
recent_records
meta.summary_count
meta.recent_record_count
```

所以 shared runtime 的上下文边界应该是 Memory Engine scope resolver：

```rust
#[async_trait::async_trait]
pub trait MemoryScopeResolver: Send + Sync {
    async fn resolve_scope(&self, runtime_id: &str) -> Result<MemoryScope, String>;
}

pub struct MemoryScope {
    pub tenant_id: String,
    pub source_id: Option<String>,
    pub system_id: Option<String>,
    pub thread_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Vec<String>,
}
```

如果使用 direct 模式，`source_id` 对应：

```text
chatos
task_runner_service
```

如果使用 system key 模式，则由各自系统配置 `system_id / secret_key`，scope 里仍然保留 `tenant_id / thread_id / subject_id`。

shared runtime 负责：

1. 用 resolver 得到 Memory Engine scope。
2. 调 `compose_context`。
3. 把 `blocks` 转成 system/developer input items。
4. 把 `recent_records` 转成 user/assistant/tool input items。

chatos resolver：

```text
session_id
  -> build_thread_mapping(session)
  -> tenant_id = session.user_id
  -> source_id = chatos
  -> thread_id = session.id
  -> subject_id = session:{session_id}
  -> related_subject_ids = contact / agent / project / contact_project / agent_project
```

Task Runner resolver：

```text
task_run_id 或 task_thread_id
  -> tenant_id = task.tenant_id
  -> source_id = task_runner_service
  -> thread_id = task.memory_thread_id
  -> subject_id = task:{task_id} 或 actor:{user_id}
  -> related_subject_ids = project / contact / agent / workspace 等可选 scope
```

另外，AI 循环仍然需要写入 user、assistant、tool records，因此保留一个写入接口：

```rust
#[async_trait::async_trait]
pub trait MemoryRecordWriter: Send + Sync {
    async fn save_user_record(&self, input: SaveRecordInput) -> Result<String, String>;
    async fn save_assistant_record(&self, input: SaveAssistantRecordInput) -> Result<String, String>;
    async fn save_tool_record(&self, input: SaveToolRecordInput) -> Result<String, String>;
    async fn save_tool_results(&self, scope: &MemoryScope, results: &[ToolResult]);
}
```

chatos 实现：

```text
ChatosMemoryScopeResolver
ChatosMemoryRecordWriter
  -> chatos_memory_engine::sync_chatos_session(...)
  -> memory_engine_sdk::batch_sync_records(...)
```

Task Runner 实现：

```text
TaskRunnerMemoryScopeResolver
TaskRunnerMemoryRecordWriter
  -> task_runner_service local DB
  -> memory_engine_sdk::upsert_thread(...)
  -> memory_engine_sdk::batch_sync_records(...)
```

这样 AI runtime 不再关心 chatos session 表，也不需要 Task Runner 自己实现一套 summary/history 拼接逻辑。上下文能力统一由 Memory Engine 提供，两个系统只换系统身份和 scope key。

### 4.2 工具执行

核心 trait：

```rust
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    fn available_tools(&self) -> Vec<serde_json::Value>;
    fn unavailable_tools(&self) -> Vec<serde_json::Value>;
    fn tool_metadata(&self) -> ToolMetadataMap;

    async fn execute_tools_stream(
        &self,
        tool_calls: &[serde_json::Value],
        context: ToolExecutionContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult>;
}
```

`chatos_mcp_runtime::McpExecutor` 实现这个 trait。

### 4.3 运行时事件

核心 trait：

```rust
pub trait RunEventSink: Send + Sync {
    fn on_chunk(&self, text: String);
    fn on_thinking(&self, text: String);
    fn on_phase(&self, event: serde_json::Value);
    fn on_tools_start(&self, event: serde_json::Value);
    fn on_tools_stream(&self, event: serde_json::Value);
    fn on_tools_end(&self, event: serde_json::Value);
    fn on_model_request(&self, payload: serde_json::Value);
}
```

chatos 用它转 SSE / websocket 事件。

Task Runner 用它写入 task run event 表和 Memory Engine records。

### 4.4 运行时增强上下文

当前 `runtime_guidance`、task board prompt、builtin MCP prompt 都是 chatos conversation runtime 的一部分。

共享 runtime 只保留扩展点：

```rust
#[async_trait::async_trait]
pub trait RuntimeInputProvider: Send + Sync {
    async fn prefixed_input_items(&self, ctx: &RuntimeTurnContext) -> Vec<serde_json::Value>;
    async fn dynamic_guidance_items(&self, ctx: &RuntimeTurnContext) -> Vec<serde_json::Value>;
}
```

chatos provider：

```text
联系人 prompt
command prompt
builtin MCP prompt
task board prompt
runtime guidance
```

Task Runner provider：

```text
任务目标
任务输入
任务执行约束
可用 MCP 提示
```

## 5. Cargo 引入方式

本轮直接把仓库调整成 workspace，不采用临时 path 引入过渡方案。

根目录新增：

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

`chat_app_server_rs/Cargo.toml`：

```toml
[dependencies]
chatos_ai_runtime = { path = "../crates/chatos_ai_runtime" }
chatos_mcp_runtime = { path = "../crates/chatos_mcp_runtime" }
chatos_builtin_tools = { path = "../crates/chatos_builtin_tools" }
```

`task_runner_service/backend/Cargo.toml`：

```toml
[dependencies]
chatos_ai_runtime = { path = "../../crates/chatos_ai_runtime" }
chatos_mcp_runtime = { path = "../../crates/chatos_mcp_runtime" }
chatos_builtin_tools = { path = "../../crates/chatos_builtin_tools" }
memory_engine_sdk = { path = "/Users/lilei/project/my_project/memory_engine/sdk" }
```

`chatos_builtin_tools` 默认编译完整 provider 集合。具体某个任务能不能使用某个工具，由运行时 capability 和任务配置决定，不靠 Cargo feature 裁剪能力。

## 6. 新 Task Runner 如何使用共享 runtime

### 6.1 任务执行链路

```text
用户点击执行任务
  -> Task Runner 创建 TaskRun
  -> 读取 Task + ModelConfig
  -> TaskRunnerMemoryScopeResolver 解析 Memory Engine scope key
  -> shared runtime 调 Memory Engine compose_context
  -> TaskRunnerBuiltinToolRegistry 注册可用内置 MCP
  -> chatos_ai_runtime::AiRuntime::run_turn(...)
  -> 过程事件写 TaskRun events
  -> 用户/助手/工具消息写 Memory Engine
  -> 写回 TaskRun 状态和结果摘要
```

### 6.2 Task Runner 自己实现的内置 MCP

Task Runner 需要自己的任务工具 provider：

```text
task_runner_list_tasks
task_runner_get_task
task_runner_update_task
task_runner_complete_task
task_runner_cancel_task
task_runner_create_followup_task
```

这些工具复用 `chatos_mcp_runtime` 的注册协议，也复用共享后的 `task_manager` provider 框架；区别是 Task Runner 注入自己的 `TaskToolBackend`，不写 chatos 的联系人任务表。

这样模型在 Task Runner 里也能通过 MCP 修改任务，但不会污染 chatos 的联系人任务表。

### 6.3 复用内置 MCP 的策略

本轮一次性支持完整内置 MCP 集合，不做“第一阶段只开放部分工具”。Task Runner 后端需要能注册以下 provider：

```text
web_tools
browser_tools
code_maintainer_read
code_maintainer_write
terminal_controller
remote_connection_controller
ui_prompter
agent_builder
task_manager
notepad
memory_skill_reader
memory_command_reader
memory_plugin_reader
```

但“支持注册”和“默认授权给某个任务执行”分开：

1. 所有 provider 都进入 shared crate。
2. 所有 provider 都能在 chatos 注册并保持原行为。
3. Task Runner 可以按任务配置启用/禁用 provider。
4. 有副作用的工具必须受 capability 控制：
   - `allow_writes`
   - `allow_terminal`
   - `allow_browser`
   - `allow_remote_connection`
   - `allow_ui_prompt`
5. `ui_prompter` 在 Task Runner 里可以实现为前端待确认事件，不应该阻塞后台 worker 无限等待。
6. `memory_skill_reader` / `memory_command_reader` / `memory_plugin_reader` 在 Task Runner 里如果没有联系人 agent，就返回明确不可用原因，而不是缺失工具定义。

## 7. chatos 如何迁移

迁移目标是“不改变现有聊天行为”。

步骤：

1. 新建 shared crates，但先复制代码，不动调用链。
2. 在 shared crates 里建立 trait 和通用类型。
3. chatos 内实现：
   - `ChatosMemoryScopeResolver`
   - `ChatosMemoryRecordWriter`
   - `ChatosToolExecutor`
   - `ChatosRuntimeInputProvider`
   - `ChatosRunEventSink`
4. 将 `AiServer` 内部替换为 shared `AiRuntime`。
5. 保持 `modules/conversation_runtime` 的 API 不变。
6. 迁移内置 MCP executor 到 `chatos_mcp_runtime`。
7. 将 chatos 原有 builtin tools 用 provider registry 注册回来。
8. 跑原有聊天、工具、任务看板、summary、MCP 测试。

这样前端和 API 层基本不需要感知 runtime 被抽走。

## 8. 一次性交付实施拆解

下面不是 MVP 分阶段上线，而是一次完整改造里的工程拆解。所有条目都属于同一轮交付范围；只有代码提交顺序不同，最终验收必须同时覆盖 chatos 和 Task Runner。

### 工作流 0：依赖边界盘点

产出：

1. 列出 AI runtime 对 `crate::` 的所有依赖。
2. 标记为 shared / adapter / chatos-only。
3. 给现有聊天、工具调用、task manager MCP、内置工具注册加回归测试清单。

验收：

1. 能跑通普通聊天。
2. 能跑通带 builtin MCP 的聊天。
3. 能跑通工具调用后继续模型回答。

### 工作流 1：抽 `chatos_mcp_runtime`

产出：

1. `McpHttpServer`、`McpStdioServer`、`McpBuiltinServer` 迁入 shared crate。
2. `McpExecutorCore` 迁入 shared crate。
3. `BuiltinToolService` enum 改为 `BuiltinToolProvider` trait。
4. chatos 内置工具通过 registry 注入。

验收：

1. chatos 现有 MCP 列表不变。
2. HTTP / stdio MCP 可用。
3. builtin MCP 可用。
4. Codex gateway passthrough 工具描述可用。

### 工作流 2：抽 `chatos_ai_runtime`

产出：

1. AI request handler 迁入 shared crate。
2. AI client execution loop 迁入 shared crate。
3. `MessageManager` 的读上下文逻辑替换为 `MemoryScopeResolver + compose_context`。
4. `MessageManager` 的写消息逻辑替换为 `MemoryRecordWriter`。
5. `McpToolExecute` 替换为 `ToolExecutor`。
6. callbacks 替换为通用 `RunEventSink` 或保留 callback struct，但不依赖 chatos。

验收：

1. chatos 普通聊天行为不变。
2. assistant 消息、tool 消息仍落库。
3. Memory Engine 上下文仍参与 prompt。
4. 流式输出、thinking、tools events 正常。

### 工作流 3：抽完整 builtin tools

产出：

1. web tools provider。
2. code maintainer provider。
3. browser tools provider。
4. terminal provider。
5. remote connection provider。
6. task manager provider + backend trait。
7. notepad provider + backend trait。
8. agent builder provider + backend trait。
9. UI prompter provider + backend trait。
10. memory skill/command/plugin reader provider + backend trait。

验收：

1. chatos 可以通过 shared builtin provider 注册工具。
2. Task Runner 可以选择启用这些 provider。
3. 权限策略由 app 注入。
4. chatos 当前所有内置 MCP 工具名、schema、调用行为保持兼容。

### 工作流 4：Task Runner 接入

产出：

1. Task Runner 后端引入 shared crates。
2. 实现 `TaskRunnerMemoryScopeResolver`。
3. 实现 `TaskRunnerMemoryRecordWriter`。
4. 实现 `TaskRunnerRuntimeInputProvider`。
5. 实现 `TaskRunnerTaskBuiltinProvider`。
6. 实现任务执行 API。

验收：

1. 前端可配置模型。
2. 可创建任务。
3. 点击执行后调用模型。
4. 工具调用过程可见。
5. Memory Engine 内能看到任务 thread、user record、assistant record、tool record。
6. 执行结果写回 TaskRun。
7. Task Runner 可以通过同一套 MCP runtime 调用完整 provider 集合。
8. Task Runner 的 task manager MCP 写自己的任务表，不写 chatos 任务表。

## 9. 风险与处理方式

### 9.1 直接抽整块会拖出太多 chatos 依赖

风险：

`conversation_runtime` 依赖联系人、项目、技能、命令、系统上下文、task board、runtime snapshot、SSE。

处理：

只抽 AI runtime 核心，把联系人运行时留在 chatos，作为 `RuntimeInputProvider` 实现。

### 9.2 task board follow-up 混在 AI loop 里

风险：

当前 `execution_loop.rs` 里已经有 task turn review / follow-up 逻辑。Task Runner 也需要任务执行复查能力，但不应该硬绑定 chatos 联系人任务看板实现。

处理：

把这段改为可插拔 `TurnReviewPolicy`：

```rust
pub trait TurnReviewPolicy {
    fn should_review(&self, ctx: &TurnReviewContext) -> bool;
    fn build_review_items(&self, ctx: &TurnReviewContext) -> Vec<Value>;
    fn parse_review_outcome(&self, response: &str) -> ReviewOutcome;
}
```

chatos 保留原 task board policy。Task Runner 同一轮交付实现自己的 `TaskRunReviewPolicy`，用于判断任务是否完成、是否需要继续执行、是否需要标记 blocked。

### 9.3 内置 MCP 工具权限

风险：

code write、terminal、browser、remote connection 都可能产生副作用。

处理：

1. provider 注册时必须带 capability。
2. 每个 task run 保存启用工具列表。
3. 所有工具都实现注册能力，但任务默认模板可以只启用 read-only 工具。
4. 写工具需要前端显式授权。
5. 执行前保存 workspace root、allow_writes、max bytes、timeout。

### 9.4 Memory Engine 双系统数据边界

风险：

chatos 和 Task Runner 都写 Memory Engine，容易 thread / tenant / subject 混乱。

处理：

1. source_id 区分：
   - `chatos`
   - `task_runner_service`
2. thread type 区分：
   - `conversation`
   - `task`
3. metadata 必须写：
   - `app`
   - `task_id`
   - `task_run_id`
   - `conversation_id`
   - `contact_agent_id`

### 9.5 Cargo workspace 改造影响现有构建

风险：

当前根目录没有 workspace `Cargo.toml`，直接引入 workspace 会改变 cargo 行为。

处理：

1. 本轮直接补根 workspace `Cargo.toml`。
2. `chat_app_server_rs`、shared crates、Task Runner backend 都进入 workspace。
3. 保留原有启动脚本入口，但底层改用 workspace package name 运行。
4. CI 或本地脚本同步切换到 workspace cargo 命令。

## 10. 建议最终目录

```text
chatos_rs/
  Cargo.toml

  crates/
    chatos_ai_runtime/
      Cargo.toml
      src/
        lib.rs
        ai_request_handler/
        ai_client/
        context.rs
        callbacks.rs
        traits.rs

    chatos_mcp_runtime/
      Cargo.toml
      src/
        lib.rs
        executor/
        tools/
        registry.rs
        transport/
        types.rs

    chatos_builtin_tools/
      Cargo.toml
      src/
        lib.rs
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

  chat_app_server_rs/
    src/
      adapters/
        ai_runtime_memory_scope.rs
        ai_runtime_memory_writer.rs
        ai_runtime_events.rs
        mcp_builtin_registry.rs

  task_runner_service/
    backend/
      Cargo.toml
      src/
        adapters/
          memory_scope.rs
          memory_record_writer.rs
          task_runtime_input_provider.rs
          task_builtin_tools.rs
```

## 11. 一次改到位需求清单

这次需求不按“最小闭环”推进，最终交付必须同时包含以下能力。

### 11.1 共享 AI runtime

1. `AiRequestHandler`、stream parser、Responses / Chat Completions transport 迁入 `chatos_ai_runtime`。
2. `AiClient` 多轮工具调用循环迁入 `chatos_ai_runtime`。
3. retry、backpressure、empty response recovery、prompt cache、reasoning/thinking 支持保持原行为。
4. task follow-up / review 迁入 `TurnReviewPolicy`，chatos 和 Task Runner 都能注入自己的 policy。
5. runtime guidance / prefixed input items 迁入 `RuntimeInputProvider`，chatos 原联系人 prompt、task board prompt、builtin MCP prompt 都通过 provider 注入。

### 11.2 共享 Memory Engine 上下文

1. shared runtime 直接调用 Memory Engine SDK `compose_context`。
2. chatos 实现 `ChatosMemoryScopeResolver`，使用现有 `build_thread_mapping(session)` 逻辑。
3. Task Runner 实现 `TaskRunnerMemoryScopeResolver`，使用自己的 `tenant_id / source_id / thread_id / subject_id`。
4. shared runtime 把 `blocks` 和 `recent_records` 转为模型 input items。
5. AI 产生的 user、assistant、tool records 都通过 `MemoryRecordWriter` 写入 Memory Engine。

### 11.3 共享 MCP runtime

1. HTTP MCP、stdio MCP、builtin MCP executor 全部迁入 `chatos_mcp_runtime`。
2. tool schema、tool metadata、tool result、parallel policy、Codex gateway passthrough 保持兼容。
3. `BuiltinToolService` enum 改为 provider registry。
4. chatos 和 Task Runner 都通过同一套 executor 调用工具。

### 11.4 共享内置 MCP 工具

本轮迁移完整集合：

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

要求：

1. chatos 当前内置 MCP 工具 schema 和行为保持兼容。
2. Task Runner 能注册同一套工具 provider。
3. 依赖 chatos 数据表的工具改为 backend trait。
4. Task Runner 注入自己的 backend，不反向依赖 chatos server。
5. 工具权限由 capability 控制，任务执行时保存启用工具快照。

### 11.5 Task Runner 完整接入

1. Rust 后端进入 workspace。
2. React + Ant Design 前端进入 `task_runner_service/frontend`。
3. 支持任务 CRUD。
4. 支持模型配置 CRUD。
5. 支持点击执行任务。
6. 支持 AI 多轮工具调用。
7. 支持 Memory Engine 上下文和 records 写入。
8. 支持 TaskRun 事件、状态、日志、结果摘要。
9. 支持通过内置 MCP 修改、完成、取消任务。
10. 支持前端查看执行过程和工具调用过程。

### 11.6 chatos 完整替换

1. chatos 原聊天入口切换到 shared `AiRuntime`。
2. chatos 原 MCP executor 切换到 shared `McpExecutor`。
3. chatos 原内置 MCP 通过 shared provider registry 注册。
4. 前端 API 和用户体验保持不变。
5. 原有联系人、项目、技能、任务看板、runtime snapshot、实时事件继续由 chatos adapter 实现。

## 12. 对新需求的回答

可以做到“两个系统通过 `Cargo.toml` 引入同一套 AI 对话 + 内置 MCP 能力”。

更准确地说，建议共享的是：

1. AI 对话运行时核心。
2. MCP 执行核心。
3. 可复用的内置工具 provider。
4. Memory Engine scope、消息写入、事件、权限的 trait 协议。

不直接放进 shared core 的是：

1. chatos 联系人会话编排的具体数据访问。
2. chatos 任务看板的具体仓储实现。
3. chatos 的 repository / models / realtime 事件实现。

这些不是不做，而是通过 adapter 接入 shared runtime。shared crate 提供协议和执行框架，chatos / Task Runner 各自提供 backend。

这样拆以后：

1. chatos 保持现有产品行为。
2. Task Runner 作为完整新系统独立运行。
3. 两边真正共享模型对话和 MCP 执行能力。
4. 后续要把联系人创建的任务交给 Task Runner 后台执行，也会有清晰的协议边界。
