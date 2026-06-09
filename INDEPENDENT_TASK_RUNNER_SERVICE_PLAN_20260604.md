# 独立 Task Runner 服务方案

日期：2026-06-04

## 1. 目标

新建一个独立的任务执行服务，先不接入现有 `chatos server`。

第一阶段做到：

1. Rust 后端。
2. React 前端。
3. UI 风格使用 Ant Design。
4. 支持任务增删改查。
5. 支持模型配置管理。
6. 用户在前端点击“执行任务”后，后端调用 AI 执行任务。
7. 任务执行期间产生的输入、输出、过程记录、上下文压缩交给 Memory Engine 管理。
8. 后端直接使用 `memory_engine_sdk` 对接 Memory Engine。

这版产品可以先理解为一个独立的“AI 任务执行台”，后续稳定后再考虑和 chatos 联系人、项目、MCP 体系打通。

## 2. 总体结论

建议新建目录：

```text
task_runner_service/
  backend/
  frontend/
```

服务边界：

1. `task_runner_service/backend`
   - Rust + Axum。
   - 本地数据库保存任务、模型配置、执行运行记录和 Memory Engine 指针。
   - 通过 `memory_engine_sdk` 写入任务执行聊天数据，并从 Memory Engine 读取压缩上下文。
   - 通过 OpenAI-compatible API 调用模型。

2. `task_runner_service/frontend`
   - React + Vite + TypeScript。
   - UI 使用 Ant Design。
   - 提供任务列表、任务编辑、模型配置、执行面板、运行历史。

3. Memory Engine
   - 保存任务执行过程中的 thread 和 records。
   - 负责 recent records、thread summary、subject memory 组合上下文。
   - 负责后续的上下文压缩和总结任务。

第一阶段不要依赖 chatos server 的会话、联系人、MCP、模型配置表，避免把新服务原型绑到现有主系统复杂度里。

## 3. 技术选型

### 3.1 后端

建议：

```text
Rust
Axum
Tokio
SQLx
SQLite first, PostgreSQL later
Reqwest
memory_engine_sdk
tracing
```

数据库策略：

1. MVP 使用 SQLite，方便本地启动。
2. 表结构和 SQLx repository 设计时预留 PostgreSQL 迁移空间。
3. AI API Key MVP 可以先明文保存到本地 SQLite；正式版再加加密存储。

### 3.2 前端

建议：

```text
React
TypeScript
Vite
Ant Design
TanStack Query
Zustand optional
```

AntD 组件：

1. `Layout`：整体框架。
2. `Table`：任务列表、模型列表、运行历史。
3. `Form`：任务编辑、模型配置。
4. `Drawer`：任务详情和运行详情。
5. `Modal`：删除确认、执行确认。
6. `Steps` / `Timeline`：任务执行过程。
7. `Tag` / `Badge` / `Progress`：状态展示。
8. `Segmented`：任务状态筛选。

## 4. 核心领域模型

### 4.1 Task

任务是用户可维护的执行单元。

字段建议：

```text
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
```

`status` 建议：

```text
draft
ready
running
succeeded
failed
blocked
cancelled
archived
```

说明：

1. `description` 给用户看。
2. `objective` 给模型执行用。
3. `input_payload_json` 保存结构化输入。
4. `memory_thread_id` 指向 Memory Engine 里的 thread。
5. `tenant_id` MVP 可以固定为 `default_tenant`，后续再接用户体系。
6. `subject_id` 可以先固定为 `task_runner_user_default`，后续用于用户/项目/团队级记忆。

### 4.2 ModelConfig

模型配置独立保存，不复用 chatos 的模型表。

字段建议：

```text
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
enabled
created_at
updated_at
```

MVP 支持：

1. OpenAI-compatible Chat Completions。
2. 可选支持 Responses API。
3. 后续再扩展 Anthropic、Gemini、本地模型网关。

### 4.3 TaskRun

每次点击执行任务都会生成一条运行记录。

字段建议：

```text
id
task_id
model_config_id
memory_thread_id
memory_user_record_id
memory_assistant_record_id
status
started_at
finished_at
input_snapshot_json
context_snapshot_json
result_summary
error_message
usage_json
created_at
updated_at
```

`status` 建议：

```text
queued
running
succeeded
failed
cancelled
blocked
```

说明：

1. Task 保存“当前任务状态”。
2. TaskRun 保存“每次执行历史”。
3. 过程消息不在本地重复存完整内容，主要写入 Memory Engine records。
4. 本地只保存 Memory Engine record id 和结果摘要，方便列表展示。

## 5. Memory Engine 对接设计

### 5.1 SDK 初始化

使用 direct 模式：

```rust
let client = MemoryEngineClient::new_direct(
    memory_base_url,
    Duration::from_secs(30),
    "task_runner_service",
)?
.with_operator_token(operator_token);
```

配置项：

```text
MEMORY_ENGINE_BASE_URL=http://127.0.0.1:7081
MEMORY_ENGINE_SOURCE_ID=task_runner_service
MEMORY_ENGINE_OPERATOR_TOKEN=
```

### 5.2 每个任务对应一个 Memory Thread

创建任务时：

1. 后端生成 `task_id`。
2. 创建或复用 Memory Engine thread：

```text
thread_id = task-{task_id}
tenant_id = default_tenant
subject_id = task_runner_user_default
thread_type = task
title = task.title
labels = ["task_runner", task.status]
metadata = { task_id, service: "task_runner_service" }
```

调用：

```rust
client.upsert_thread(thread_id, &SdkUpsertThreadRequest { ... }).await
```

### 5.3 执行前组装上下文

点击执行时：

1. 根据 task 找到 `memory_thread_id`。
2. 调用 `compose_context`：

```rust
client.compose_context(&SdkComposeContextRequest {
    tenant_id,
    subject_id: Some(subject_id),
    related_subject_ids: None,
    thread_id,
    policy: Some(ComposeContextPolicy {
        include_recent_records: Some(true),
        include_thread_summary: Some(true),
        include_subject_memory: Some(true),
        recent_record_limit: Some(20),
        summary_limit: Some(5),
    }),
}).await
```

3. 把返回的 `blocks` 和 `recent_records` 转为模型上下文。

模型 system prompt 建议包含：

```text
你是 Task Runner 服务中的任务执行代理。
你需要根据任务目标、历史执行记录和 Memory Engine 提供的上下文完成任务。
如果任务可以完成，给出清晰结果。
如果任务无法完成，说明阻塞原因、已尝试内容和下一步需要什么。
```

### 5.4 执行期间写入 records

每次执行至少写入两类 record：

1. user record：本次任务执行请求。
2. assistant record：AI 执行结果。

record 示例：

```text
role = user
record_type = task_run_input
content = task objective + input snapshot
metadata = { task_id, task_run_id, model_config_id }
```

```text
role = assistant
record_type = task_run_result
content = AI output
metadata = { task_id, task_run_id, status, usage }
```

如果后续支持流式、工具调用或多轮执行，可以继续写：

```text
record_type = thought
record_type = tool_call
record_type = tool_result
record_type = checkpoint
```

### 5.5 执行后触发上下文压缩

执行完成后调用：

```rust
client.run_thread_repair_summary(thread_id, tenant_id).await
```

语义：

1. Memory Engine 接收后异步总结。
2. Task Runner 不需要等待总结完成。
3. 本地 TaskRun 可以记录 `summary_job_run_id`。

后续可做：

1. 在运行详情里展示 Memory Engine summaries。
2. 提供“手动压缩上下文”按钮。
3. 后台定时调用 `run_pending_summaries_once` / `run_pending_rollups_once`。

## 6. AI 执行流程

点击“执行任务”后的流程：

1. 前端调用：

```text
POST /api/tasks/{task_id}/runs
```

2. 后端校验任务、模型配置。
3. 创建 `TaskRun(status=queued)`。
4. 将任务状态更新为 `running`。
5. 写入 Memory Engine user record。
6. 调用 Memory Engine `compose_context`。
7. 拼装模型请求。
8. 调用 AI provider。
9. 写入 Memory Engine assistant record。
10. 更新 `TaskRun(status=succeeded|failed|blocked)`。
11. 更新 Task 当前状态。
12. 触发 Memory Engine thread repair summary。
13. 通过 SSE 或轮询让前端看到执行结果。

MVP 可以先非流式：

```text
点击执行 -> 按钮 loading -> 后端同步返回 run 结果
```

第二阶段再做：

```text
点击执行 -> 后端立即返回 run_id -> 前端订阅 SSE -> 实时显示执行过程
```

## 7. 后端 API 设计

### 7.1 Task API

```text
GET    /api/tasks
POST   /api/tasks
GET    /api/tasks/{task_id}
PATCH  /api/tasks/{task_id}
DELETE /api/tasks/{task_id}
```

列表支持：

```text
status
keyword
tag
limit
offset
```

### 7.2 Run API

```text
POST /api/tasks/{task_id}/runs
GET  /api/tasks/{task_id}/runs
GET  /api/runs/{run_id}
POST /api/runs/{run_id}/cancel
```

MVP `cancel` 可以先只支持未开始或本地标记取消；真正中断模型请求第二阶段补。

### 7.3 Model Config API

```text
GET    /api/model-configs
POST   /api/model-configs
GET    /api/model-configs/{id}
PATCH  /api/model-configs/{id}
DELETE /api/model-configs/{id}
POST   /api/model-configs/{id}/test
```

`test` 用于验证 base_url、api_key、model 是否可用。

### 7.4 Memory API

```text
GET  /api/tasks/{task_id}/memory/context
GET  /api/tasks/{task_id}/memory/records
POST /api/tasks/{task_id}/memory/summarize
```

用途：

1. 调试上下文。
2. 查看任务执行历史 records。
3. 手动触发 Memory Engine 总结。

## 8. 前端页面设计

### 8.1 主布局

AntD `Layout`：

```text
左侧 Sider:
  - Tasks
  - Runs
  - Models
  - Settings

顶部 Header:
  - 当前服务状态
  - Memory Engine 连接状态

内容 Content:
  - 当前页面
```

### 8.2 任务列表页

功能：

1. 任务表格。
2. 状态筛选。
3. 关键词搜索。
4. 新建任务。
5. 编辑任务。
6. 删除任务。
7. 点击执行。
8. 查看运行历史。

表格列：

```text
标题
状态
优先级
默认模型
最近执行时间
最近结果
更新时间
操作
```

操作：

```text
执行
编辑
历史
删除
```

### 8.3 任务编辑页/Drawer

AntD `Drawer + Form`：

字段：

```text
title
description
objective
input_payload_json
priority
tags
default_model_config_id
```

`objective` 使用大文本框。

`input_payload_json` 可以先用 `TextArea`，后续接 JSON editor。

### 8.4 执行详情页

展示：

1. 当前 run 状态。
2. 执行输入。
3. Memory Engine context blocks。
4. AI 输出。
5. 错误信息。
6. Token usage。
7. Memory records。

AntD 组件：

```text
Descriptions
Timeline
Collapse
Typography.Paragraph
Alert
```

### 8.5 模型配置页

功能：

1. 新增模型。
2. 编辑模型。
3. 删除模型。
4. 启用/禁用模型。
5. 测试模型。

字段：

```text
name
provider
base_url
api_key
model
temperature
max_output_tokens
thinking_level
supports_responses
enabled
```

### 8.6 设置页

配置：

```text
Memory Engine Base URL
Memory Engine Source ID
默认 tenant_id
默认 subject_id
执行超时时间
```

MVP 也可以先只走后端 `.env`，前端只展示连接状态。

## 9. 后端模块划分

建议：

```text
task_runner_service/backend/src/
  main.rs
  config.rs
  db/
    mod.rs
    sqlite_schema.rs
  api/
    mod.rs
    tasks.rs
    runs.rs
    model_configs.rs
    memory.rs
  models/
    task.rs
    task_run.rs
    model_config.rs
  repositories/
    tasks.rs
    task_runs.rs
    model_configs.rs
  services/
    task_execution.rs
    ai_client.rs
    memory_engine.rs
    prompt_builder.rs
  errors.rs
```

核心服务：

1. `task_execution`
   - 编排执行流程。
   - 更新 task/run 状态。

2. `memory_engine`
   - 包装 `memory_engine_sdk`。
   - 提供 `ensure_task_thread`、`write_task_record`、`compose_task_context`、`request_summary`。

3. `ai_client`
   - OpenAI-compatible provider 调用。
   - 后续支持 streaming。

4. `prompt_builder`
   - 将 task、Memory Engine context、用户输入组合成模型请求。

## 10. 数据表草案

### 10.1 tasks

```sql
CREATE TABLE tasks (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  objective TEXT NOT NULL DEFAULT '',
  input_payload_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'draft',
  priority TEXT NOT NULL DEFAULT 'medium',
  tags_json TEXT NOT NULL DEFAULT '[]',
  default_model_config_id TEXT,
  memory_thread_id TEXT,
  tenant_id TEXT NOT NULL DEFAULT 'default_tenant',
  subject_id TEXT NOT NULL DEFAULT 'task_runner_user_default',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
);
```

### 10.2 task_runs

```sql
CREATE TABLE task_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  model_config_id TEXT,
  memory_thread_id TEXT,
  memory_user_record_id TEXT,
  memory_assistant_record_id TEXT,
  status TEXT NOT NULL DEFAULT 'queued',
  started_at TEXT,
  finished_at TEXT,
  input_snapshot_json TEXT NOT NULL DEFAULT '{}',
  context_snapshot_json TEXT NOT NULL DEFAULT '{}',
  result_summary TEXT NOT NULL DEFAULT '',
  error_message TEXT,
  usage_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);
```

### 10.3 model_configs

```sql
CREATE TABLE model_configs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL DEFAULT 'openai',
  base_url TEXT NOT NULL,
  api_key TEXT NOT NULL,
  model TEXT NOT NULL,
  temperature REAL NOT NULL DEFAULT 0.7,
  max_output_tokens INTEGER,
  thinking_level TEXT,
  supports_responses INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

## 11. 与 Memory Engine 的数据分工

本地 Task Runner 保存：

1. 任务定义。
2. 模型配置。
3. 运行状态。
4. 运行摘要。
5. Memory Engine thread/record 指针。

Memory Engine 保存：

1. 任务执行期间的聊天数据。
2. 执行过程记录。
3. 历史上下文。
4. thread summaries。
5. subject memories。
6. 上下文压缩结果。

原则：

1. 不在 Task Runner 本地重复维护完整聊天历史。
2. 所有可长期复用的执行上下文都进入 Memory Engine。
3. 本地 DB 只服务 UI 查询和执行状态机。

## 12. MVP 分阶段

### 阶段一：项目骨架

1. 新建 `task_runner_service/backend`。
2. 新建 `task_runner_service/frontend`。
3. 后端健康检查。
4. 前端 AntD Layout。
5. 本地 SQLite schema。

### 阶段二：任务和模型 CRUD

1. 任务增删改查。
2. 模型配置增删改查。
3. 模型连通性测试。
4. 前端任务列表和模型配置页。

### 阶段三：Memory Engine 接入

1. 后端封装 `memory_engine_sdk`。
2. 创建任务时 upsert Memory Thread。
3. 执行前 compose context。
4. 执行输入/结果写入 records。
5. 执行后触发 summary。

### 阶段四：点击执行任务

1. `POST /api/tasks/{id}/runs`。
2. 后端调用 AI。
3. 更新 task/run 状态。
4. 前端展示执行结果。
5. 支持运行历史。

### 阶段五：执行体验增强

1. SSE 实时输出。
2. 取消执行。
3. 执行超时。
4. 重试。
5. 上下文预览。
6. Memory records 展示。

## 13. 后续再接 chatos 的方式

独立服务稳定后，再考虑和 chatos 对接。

可选方式：

1. chatos 只把任务创建请求转发给 Task Runner。
2. Task Runner 提供 MCP server，让 chatos 联系人通过 MCP 创建/修改/执行任务。
3. chatos 前端嵌入 Task Runner 页面。
4. Task Runner 执行完成后通过 webhook 或 API 回写 chatos 联系人会话。

但这些都不进入第一阶段。

第一阶段重点是把独立任务服务自己的闭环做好：

```text
任务 CRUD -> 模型配置 -> 点击执行 -> AI 结果 -> Memory Engine 记录和压缩 -> 前端可查看
```

## 14. 复用 chatos 内置 MCP

### 14.1 结论

可以复用，但推荐“抽共享内置 MCP runtime”，不要让新 Task Runner 直接依赖整个 `chat_app_server_rs`。

当前 chatos 内置 MCP 的形态不是独立外部 MCP server，而是进程内 builtin function tools：

1. `chat_app_server_rs/src/services/builtin_mcp.rs` 定义内置 MCP id、server name、kind。
2. `chat_app_server_rs/src/services/mcp_loader.rs` 把内置 MCP 配置转成 `McpBuiltinServer`。
3. `chat_app_server_rs/src/services/agent_runtime/mcp_tool_execute.rs` 支持 `init_builtin_only()`。
4. `chat_app_server_rs/src/core/mcp_tools/builtin.rs` 根据 `BuiltinMcpKind` 创建对应工具服务。

这说明新 Task Runner 可以复用同一套“内置工具注册 + function tool schema + tool call 执行”机制，不一定要启动 chatos server。

### 14.2 推荐拆分方式

新增共享 crate：

```text
crates/chatos_builtin_mcp/
```

或先放在新服务下：

```text
task_runner_service/backend/crates/builtin_mcp_runtime/
```

对外提供：

```rust
BuiltinMcpKind
BuiltinMcpServerConfig
BuiltinToolRuntime
BuiltinToolContext
BuiltinToolService
list_builtin_tools(...)
execute_builtin_tool(...)
```

新服务后端只依赖这个共享 crate，而不是依赖 `chat_app_server_rs`。

### 14.3 需要做适配层

内置 MCP 里有些工具本身比较通用，有些强绑定 chatos。推荐用 trait adapter 把依赖隔离：

```rust
trait TaskStore {
    async fn list_tasks(...);
    async fn update_task(...);
    async fn complete_task(...);
}

trait NotepadStore {
    async fn list_notes(...);
    async fn upsert_note(...);
}

trait UiPromptSink {
    async fn request_choice(...);
}

trait RemoteConnectionStore {
    async fn get_connection(...);
}
```

Task Runner 实现自己的 adapter：

```text
TaskRunnerTaskStore -> task_runner_service.tasks
TaskRunnerMemoryStore -> memory_engine_sdk
TaskRunnerWorkspace -> task_runner workspace root
TaskRunnerPromptSink -> 前端 SSE / run confirmation UI
```

这样内置 MCP 的工具逻辑可以复用，但数据源切到新服务自己的表和 Memory Engine。

### 14.4 工具复用难度分级

第一批建议复用：

1. `builtin_web_tools`
   - 难度：低。
   - 主要依赖 `reqwest` 和 workspace dir。
   - 适合任务执行时联网检索和网页抽取。

2. `builtin_code_maintainer_read`
   - 难度：低到中。
   - 只要 Task Runner 有 workspace root，就可以读文件、搜索文件。
   - 写工具先不要默认打开。

3. `builtin_code_maintainer_write`
   - 难度：中。
   - 需要 Task Runner 明确 workspace sandbox、写入权限和变更审计。
   - 建议 MVP 先只启用 read，第二阶段再启用 write。

4. `builtin_terminal_controller`
   - 难度：中。
   - 需要本地进程管理、日志截断、超时、工作目录隔离。
   - 可以复用，但默认关闭，按任务显式启用。

第二批再考虑：

1. `builtin_notepad`
   - 难度：中。
   - 现有 notepad 存储偏 chatos 本地用户目录。
   - 新系统可以改成 Task Runner 自己的 notes 表，或直接把 note records 写 Memory Engine。

2. `builtin_task_manager`
   - 难度：中。
   - 现有实现绑定 chatos conversation/task table。
   - 新系统需要做 Task Runner 版 task tool：`list_tasks`、`update_task`、`complete_task`、`cancel_task`。
   - 工具 schema 和提示可以复用，store 要换。

3. `builtin_browser_tools`
   - 难度：中到高。
   - 取决于是否复用 chatos 的 browser runtime。
   - MVP 可先只上 `web_tools`，浏览器自动化后置。

不建议第一阶段复用：

1. `builtin_ui_prompter`
   - 难度：高。
   - 依赖前端实时确认、等待用户 decision、超时处理。
   - 新服务应先用自己的 run confirmation / SSE 机制。

2. `builtin_agent_builder`
   - 难度：高。
   - 强绑定 chatos agent / memory agent 体系。

3. `memory_skill_reader` / `memory_command_reader` / `memory_plugin_reader`
   - 难度：高。
   - 强绑定 chatos 联系人、技能、插件。
   - 新系统没有联系人模型时不需要先接。

4. `builtin_remote_connection_controller`
   - 难度：高。
   - 依赖远端连接配置、认证、SFTP/SSH 能力。
   - 等 Task Runner 自己有 remote connection 模型后再接。

### 14.5 新服务里的 MCP 配置页面

前端增加 `Tools` 页面或放在任务编辑页里：

1. 全局内置工具开关。
2. 每个任务选择允许的工具集。
3. 显示工具风险级别。
4. 对写文件、终端、远端连接这类高风险工具增加显式确认。

任务字段增加：

```text
enabled_builtin_mcp_ids_json
workspace_root
tool_policy_json
```

模型执行时：

1. 根据任务允许的工具创建 `BuiltinToolRuntime`。
2. 把工具 schema 放入 AI 请求。
3. 模型返回 tool calls 后，后端执行工具。
4. 工具结果写入 Memory Engine records。
5. 继续下一轮模型调用，直到无工具调用或达到最大轮数。

### 14.6 对 AI 执行流程的补充

如果启用内置 MCP，`task_execution` 需要从单次模型调用升级为 tool loop：

```text
compose memory context
build prompt + tools
call model
if tool_calls:
  execute builtin tools
  write tool_call/tool_result records to Memory Engine
  call model again with tool results
else:
  write final assistant record
  finish run
```

需要限制：

```text
max_tool_rounds
max_tool_calls_per_round
max_tool_result_chars
max_run_seconds
```

### 14.7 推荐落地顺序

1. 先实现无工具的 Task Runner MVP。
2. 抽出最小 builtin runtime，只接 `web_tools` 和 `code_maintainer_read`。
3. 把工具调用过程写入 Memory Engine。
4. 前端增加任务级工具开关。
5. 接 `task_manager` 的 Task Runner 版本。
6. 再评估 terminal、browser、notepad。

这样做能复用 chatos 内置 MCP 的价值，但不会把新系统变成 chatos server 的影子进程。

## 15. 验收标准

MVP 完成时应满足：

1. 可以在前端创建一个任务。
2. 可以配置一个模型。
3. 可以点击执行任务。
4. 后端会调用 AI，并把结果展示在前端。
5. 执行输入和输出会写入 Memory Engine。
6. 再次执行同一任务时，会从 Memory Engine compose context 拿到历史上下文。
7. 可以查看任务运行历史。
8. 可以删除、编辑任务。
9. 可以编辑、禁用、测试模型配置。

这版服务先独立跑通，不污染现有 chatos server，也给后续产品化留出清晰边界。
