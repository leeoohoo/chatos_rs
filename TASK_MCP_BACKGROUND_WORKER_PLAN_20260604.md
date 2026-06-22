# 内置任务 MCP 后台执行 Worker 强化方案

日期：2026-06-04

## 1. 结论

建议新开一个独立的后端 worker 进程来执行任务，但不要把任务、联系人、模型配置、MCP 上下文重新做成一套新业务系统。

推荐形态：

1. `chat_app_server_rs` 继续作为主 API 服务，负责联系人会话、任务 MCP、任务表、模型配置、实时推送和消息持久化。
2. 新增一个独立运行的 `chatos_task_worker` 后端进程，负责定时扫描待执行任务、抢占任务、调用模型、更新任务状态，并把执行结果作为联系人消息发回会话。
3. worker 初期可以放在 `chat_app_server_rs/src/bin/chatos_task_worker.rs`，共享现有 Rust 模块和仓储；部署时作为独立进程或独立容器启动。这样既能隔离复杂度，又避免复制一套联系人上下文和 MCP 调用逻辑。

不建议一开始就做完全独立的新业务后端。完全独立会立刻遇到这些重复建设：

1. 联系人 runtime context 解析。
2. task board prompt 注入。
3. 模型配置解析和权限校验。
4. MCP server selection 与内置 MCP 注册。
5. 消息落库、turn runtime snapshot、实时事件。

这些逻辑现在都已经在 `chat_app_server_rs` 里有成熟入口，worker 应该复用它们。

## 2. 当前项目里可复用的基础

现有代码已经具备大部分地基：

1. 内置任务 MCP：`chat_app_server_rs/src/builtin/task_manager/mod.rs`
   - 已有 `add_task`、`list_tasks`、`update_task`、`complete_task`、`delete_task`。
   - `add_task` 在 `auto_create_task=true` 时会自动持久化任务。

2. 任务持久化：`chat_app_server_rs/src/services/task_manager/*`
   - SQLite 表：`task_manager_tasks`
   - Mongo collection：`task_manager_tasks`
   - 创建、更新、完成、删除都会发布 `conversation.task_board.updated`。

3. 模型配置：`ai_model_configs` 与 `session_runtime_settings`
   - `session_runtime_settings.selected_model_id` 已经保存当前会话选择的模型。
   - 后端请求也支持 `model_config_id`。
   - `resolve_model_runtime_for_request(...)` 已经支持按 `model_config_id` 解析模型运行配置。

4. 联系人会话上下文：`modules/conversation_runtime`
   - `load_common_chat_bootstrap(...)`
   - `resolve_runtime_context(...)`
   - `run_chat_usecase(...)`
   - 这里已经会加载联系人 prompt、项目、远端连接、MCP、技能、记忆摘要、任务看板。

5. 任务看板注入模型：`modules/conversation_runtime/task_board.rs`
   - `build_task_board_prompt(...)`
   - `build_runtime_prefixed_input_items_for_turn(...)`
   - 任务看板已经能作为 runtime prefixed system prompt 进入模型上下文。

## 3. 目标行为

### 3.1 创建任务后的行为

当联系人通过内置任务 MCP 创建任务后：

1. 当前联系人会话这一轮立即结束。
2. 联系人正常回复用户，例如：
   - “任务已创建，正在后台执行。”
   - “我会在后台继续处理，完成后会把结果发回来。”
3. 后续任务执行不再占用当前用户对话 turn。
4. 后台 worker 定时领取任务并执行。
5. 任务完成、失败、阻塞或取消后，由联系人自动向该会话发送一条可见消息说明执行情况。

### 3.2 用户后续仍可正常聊天

任务创建后，用户可以继续给联系人发送普通消息。

如果联系人判断用户是在改变任务、补充任务条件、放弃任务或取消任务，就继续调用现有内置任务 MCP 操作任务看板：

1. 改任务：调用 `task_manager_update_task`。
2. 完成任务：调用 `task_manager_complete_task`。
3. 放弃/取消任务：建议新增 `task_manager_cancel_task`，或扩展 `update_task` 支持 `status=cancelled`。
4. 删除误建任务：继续保留 `task_manager_delete_task`。

用户消息优先级高于后台执行。worker 每次开始执行、写回结果前都必须重新检查任务状态和版本。

## 4. 服务拆分方案

### 4.1 主服务职责

`chat_app_server_rs` 保持以下职责：

1. 提供普通聊天 API。
2. 提供任务 MCP 工具。
3. 任务增删改查和权限校验。
4. 保存任务创建时的运行快照字段。
5. 提供 worker 领取任务、写回结果所需的内部函数或内部 API。
6. 负责消息落库与实时事件发布。

### 4.2 Worker 职责

新增 `chatos_task_worker`：

1. 定时扫描待执行任务。
2. 原子抢占任务，写入 lease，避免多 worker 重复执行。
3. 使用任务创建时记录的模型配置 ID 调用模型。
4. 使用与联系人普通聊天相同的上下文构造逻辑。
5. 允许模型继续调用内置任务 MCP 更新任务状态。
6. 将执行结果写为联系人会话消息。
7. 失败时重试或标记阻塞。
8. 支持取消和用户修改任务后的中止/重排。

## 5. 数据模型调整

### 5.1 扩展 `task_manager_tasks`

建议在现有任务表上新增字段：

```text
created_by_user_id TEXT
created_by_contact_agent_id TEXT
project_id TEXT
project_root TEXT
remote_connection_id TEXT

model_config_id TEXT
model_config_name_snapshot TEXT

mcp_enabled INTEGER NOT NULL DEFAULT 1
enabled_mcp_ids_json TEXT NOT NULL DEFAULT '[]'
auto_create_task INTEGER NOT NULL DEFAULT 0

execution_mode TEXT NOT NULL DEFAULT 'manual'
execution_status TEXT NOT NULL DEFAULT 'idle'
next_run_at TEXT
run_after TEXT
attempt_count INTEGER NOT NULL DEFAULT 0
max_attempts INTEGER NOT NULL DEFAULT 3

lease_owner TEXT
lease_expires_at TEXT
last_run_id TEXT
last_run_turn_id TEXT
last_error TEXT

task_version INTEGER NOT NULL DEFAULT 1
cancel_requested_at TEXT
cancel_reason TEXT
```

字段说明：

1. `execution_mode`
   - `manual`：只做任务看板，不后台执行。
   - `background`：由 worker 后台执行。

2. `execution_status`
   - `idle`
   - `queued`
   - `running`
   - `completed`
   - `blocked`
   - `failed`
   - `cancelled`

3. `model_config_id`
   - 创建任务时从当前会话运行设置读取。
   - 优先来源：`session_runtime_settings.selected_model_id`
   - 兜底来源：`sessions.selected_model_id`
   - 不建议后台执行时再猜默认模型。

4. `task_version`
   - 用户每次修改任务递增。
   - worker 抢占任务时记录版本；写回时发现版本变化则停止写旧结果，改为重新排队或标记需要复查。

### 5.2 新增 `task_manager_task_runs`

建议新增任务运行表，记录每次后台执行：

```text
id TEXT PRIMARY KEY
task_id TEXT NOT NULL
conversation_id TEXT NOT NULL
conversation_turn_id TEXT NOT NULL
model_config_id TEXT
worker_id TEXT
attempt INTEGER NOT NULL
status TEXT NOT NULL
started_at TEXT
completed_at TEXT
error TEXT
result_message_id TEXT
created_at TEXT
updated_at TEXT
```

用途：

1. 查看任务执行历史。
2. 排查失败原因。
3. 避免只靠 task 当前状态丢失过程信息。
4. 后续可以给前端任务历史抽屉展示。

## 6. 任务创建链路

### 6.1 内置 MCP 创建任务

当前创建入口在：

```text
chat_app_server_rs/src/builtin/task_manager/review_flow.rs
chat_app_server_rs/src/services/task_manager/store/create_ops.rs
```

需要调整：

1. `create_tasks_for_turn(...)` 增加一个 runtime capture 参数，或内部根据 `conversation_id` 读取会话 runtime settings。
2. 创建任务时冻结以下字段：
   - `model_config_id`
   - `created_by_user_id`
   - `created_by_contact_agent_id`
   - `project_id`
   - `project_root`
   - `remote_connection_id`
   - `mcp_enabled`
   - `enabled_mcp_ids`
   - `auto_create_task`
3. 如果任务需要后台执行，写入：
   - `execution_mode=background`
   - `execution_status=queued`
   - `next_run_at=now`

### 6.2 联系人当前 turn 结束

任务创建成功后，不再让现有同一 turn 的自动 follow-up 继续执行未完成任务。

需要调整：

1. `modules/conversation_runtime/task_board.rs` 里现有同 turn follow-up 逻辑保留给非后台模式。
2. 当本轮创建了 `execution_mode=background` 的任务时，后端注入一条 runtime guidance，让模型简短告知用户任务已创建并后台执行，然后结束当前回复。
3. 不再在当前 turn 内循环执行这些任务。

## 7. Worker 执行链路

### 7.1 领取任务

worker 每隔一小段时间扫描：

```text
execution_mode='background'
execution_status in ('queued', 'failed')
next_run_at <= now
cancel_requested_at is null
```

领取时必须使用事务或原子更新：

1. 设置 `execution_status=running`
2. 设置 `lease_owner`
3. 设置 `lease_expires_at`
4. 递增 `attempt_count`
5. 创建 `task_manager_task_runs` 记录

### 7.2 构造后台模型请求

worker 不应该自己拼一套上下文，而是复用现有聊天 usecase。

推荐构造内部 `ChatStreamRequest`：

```text
conversation_id = task.conversation_id
content = 后台任务执行指令 + 当前 task 信息
model_config_id = task.model_config_id
user_id = task.created_by_user_id
turn_id = 新生成的 background turn id
contact_agent_id = task.created_by_contact_agent_id
project_id = task.project_id
project_root = task.project_root
remote_connection_id = task.remote_connection_id
mcp_enabled = task.mcp_enabled
enabled_mcp_ids = task.enabled_mcp_ids + builtin_task_manager
auto_create_task = false
```

后台执行指令示例：

```text
你正在后台执行一个由联系人会话创建的任务。

要求：
1. 使用当前联系人身份、会话记忆、项目上下文和任务看板继续工作。
2. 优先执行 task_id=... 的任务。
3. 如果完成，调用 task_manager_complete_task 写入 outcome_summary 和关键结果。
4. 如果遇到缺少权限、缺少用户输入、外部依赖等阻塞，调用 task_manager_update_task 标记 blocked，并写清 blocker_reason。
5. 不要向用户提问，除非任务确实无法继续。
6. 最终给用户一条简洁的执行结果消息。
```

### 7.3 消息落库

后台执行需要避免把内部执行指令直接展示给用户。

推荐新增内部运行选项：

1. 后台 user message 落库但 `metadata.hidden=true`。
2. assistant 结果消息可见。
3. assistant message metadata 写入：

```json
{
  "background_task": true,
  "task_id": "...",
  "task_run_id": "...",
  "conversation_turn_id": "...",
  "message_source": "task_worker"
}
```

这样用户看到的是联系人自动发来的执行结果，而不是 worker 的内部 prompt。

### 7.4 实时推送

现有 `chat_stream` realtime 主要服务于前端正在 active streaming 的 turn。后台任务完成时，用户可能没有 active stream。

建议新增或补强：

1. 新增 `conversation.messages.updated` realtime 事件，payload 带新消息。
2. 或扩展前端 `chat_stream` bridge：当收到非 active turn 的 `chat.turn.completed`，如果带 `persisted_assistant_message`，直接追加到对应会话。

推荐方案：新增 `conversation.messages.updated`，语义更清楚。

worker 完成后发布：

1. `conversation.messages.updated`
2. `conversation.task_board.updated`
3. 必要时发布 `sessions.updated`，用于左侧列表最新消息时间刷新。

## 8. 用户修改、放弃、取消任务

用户任务创建后可以继续正常聊天。

联系人如果判断用户要修改任务，就调用任务 MCP：

1. `update_task` 修改标题、详情、优先级、状态、due_at 等。
2. `cancel_task` 或 `update_task status=cancelled` 放弃任务。
3. `delete_task` 只用于误建或用户明确要求删除记录。

需要补充取消语义：

1. 新增 `cancelled` 状态。
2. 新增 `cancel_reason`、`cancel_requested_at`。
3. 如果任务还在 `queued`，worker 不再领取。
4. 如果任务正在 `running`：
   - 主服务记录取消请求。
   - 如果有 `last_run_turn_id`，调用 abort registry 取消对应后台 turn。
   - worker 在模型调用前后和工具调用间检查取消状态。

用户修改任务时：

1. `task_version += 1`
2. 如果任务在 `queued`，直接更新待执行内容。
3. 如果任务在 `running`，标记 `needs_restart` 或重新排队。
4. worker 写回结果前检查版本，不允许旧版本覆盖新任务。

## 9. 模型配置策略

创建任务时必须记录 `model_config_id`。

读取顺序：

1. 当前请求显式 `model_config_id`
2. `session_runtime_settings.selected_model_id`
3. `sessions.selected_model_id`

执行时：

1. worker 使用记录的 `model_config_id` 调用 `resolve_model_runtime_for_request(...)`。
2. 如果模型配置被删除、禁用或无权限，不静默 fallback 到其它模型。
3. 任务标记为 `blocked` 或 `failed`，并向联系人会话发送说明：
   - “任务未能执行：原模型配置不可用，请重新选择模型后继续。”

可选增强：

1. 同时保存 `model_config_name_snapshot`，方便 UI 展示。
2. 不保存 API Key 快照，避免复制敏感信息。

## 10. 为什么不把执行器直接塞进主服务

不建议直接在主 API 服务里 `tokio::spawn` 一个长期任务执行循环作为最终形态，原因：

1. 聊天请求和后台任务会争抢同一个进程资源。
2. 模型调用、MCP 工具、终端/文件操作都可能耗时，容易影响普通聊天响应。
3. 任务重试、lease、取消、并发控制会让主服务启动逻辑变复杂。
4. 后续需要独立扩容 worker 时会更难拆。

但可以保留一个开发期 fallback：

```text
TASK_WORKER_EMBEDDED=1
```

用于本地单进程调试。生产和正式开发推荐独立进程。

## 11. 推荐落地步骤

### 阶段一：数据与状态

1. 扩展 `task_manager_tasks`。
2. 新增 `task_manager_task_runs`。
3. 补齐 Mongo/SQLite 双存储映射。
4. 新增 `cancelled` 状态和 `cancel_task` 工具。

### 阶段二：任务创建后结束当前 turn

1. 创建任务时记录 runtime capture。
2. 后台任务创建后禁止同 turn 自动 follow-up 继续执行这些任务。
3. 联系人简短回复“任务已创建，后台执行中”。

### 阶段三：Worker MVP

1. 新增 `chatos_task_worker` 独立 binary。
2. 实现任务领取、lease、重试。
3. 复用 `run_chat_usecase` 或抽出 `run_background_task_turn`。
4. 后台 user message 隐藏，assistant 结果消息可见。

### 阶段四：实时与前端

1. 新增 `conversation.messages.updated` 实时事件。
2. 前端收到后台 assistant 消息后追加到当前会话。
3. 任务工作栏展示 `queued/running/blocked/failed/cancelled`。
4. 支持用户在 UI 上取消后台任务。

### 阶段五：并发与可靠性

1. 支持 worker 并发上限。
2. 支持 lease 过期恢复。
3. 支持 task version 防旧结果覆盖。
4. 支持任务执行超时。
5. 增加运行历史 UI 和日志排查。

## 12. 关键测试

后端测试：

1. 创建任务时正确保存 `model_config_id`。
2. worker 只领取 `queued` 且未取消任务。
3. 两个 worker 不会重复领取同一个任务。
4. 用户修改任务后 `task_version` 递增，旧 worker 结果不会覆盖新任务。
5. 取消 running 任务会触发 abort。
6. 模型配置不存在时任务进入 blocked/failed 并发消息说明。

前端测试：

1. 创建后台任务后当前聊天 turn 正常结束。
2. 用户可继续发消息。
3. 后台完成消息能自动出现在联系人会话里。
4. 用户取消任务后任务工作栏状态更新。
5. 用户要求改变任务时，模型可通过内置任务 MCP 修改任务。

## 13. 最小实现判断

最小可用版本只需要做到：

1. 任务创建时保存 `model_config_id`。
2. 任务进入 `queued`。
3. 独立 worker 领取并调用现有聊天 usecase。
4. worker 完成后写入可见 assistant 消息。
5. 用户可以继续聊天，并可通过任务 MCP 修改或取消任务。

这条路径既满足需求，又不会把主联系人聊天链路改成一个大型任务调度系统。
