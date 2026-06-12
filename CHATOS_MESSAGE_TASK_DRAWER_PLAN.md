# Chatos 消息任务抽屉方案

## 目标

在 Chatos 的每条可见聊天消息上增加一个「任务」按钮。用户点击后，Chatos 前端只请求 Chatos 后端，由 Chatos 后端内部调用 Task Runner，拉取与这条消息关联的任务并在右侧抽屉展示。

核心边界：

- Chatos 前端不直接调用 Task Runner。
- Chatos 后端到 Task Runner 属于内部只读调用，不走 Task Runner 用户鉴权。
- 数据归属必须由 Chatos 后端和 Task Runner 内部接口共同保证：只能返回与当前消息对应的任务。
- UI 默认保持简洁：任务卡片只展示任务名和任务描述；详情、运行详情按需打开，并默认折叠长内容。

## 用户体验

### 消息上的「任务」按钮

落点：`chat_app/src/components/messageItem/MessageActions.tsx` 或消息头部附近。

建议规则：

- 普通用户消息、Task Runner 规划回复、Task Runner 回调结果消息都可以显示「任务」按钮。
- 点击时再拉取任务，不在消息列表渲染时批量预拉取，避免消息多时产生额外请求。
- 没有关联任务时，右侧抽屉展示空状态：「这条消息暂无关联任务」。

### 右侧任务抽屉

新增组件建议：

- `MessageTaskDrawer`
- `MessageTaskCard`
- `MessageTaskDetailModal`
- `MessageTaskRunDetailModal`

抽屉内容：

- 顶部展示当前消息的简短信息：消息时间、角色、关联源消息 ID。
- 主体用卡片列表展示任务。
- 每张任务卡默认只展示：
  - 任务名：`title`
  - 任务描述：`description`，为空时展示「暂无描述」
- 卡片按钮：
  - 「详情」：打开任务详情弹窗。
  - 「运行详情」：打开最近一次运行详情弹窗；没有 `last_run_id` 时按钮置灰或显示「暂无运行」。

### 任务详情弹窗

参考 Task Runner 任务列表详情页：

- 现有位置：`task_runner_service/frontend/src/pages/TasksPage.tsx`
- 需要移植字段展示思路，但不要把 Task Runner 前端页面直接耦合进 Chatos。

展示内容做简化和折叠：

- 基础信息：任务 ID、状态、创建人、模型、优先级、计划方式、创建时间、更新时间。
- 任务内容：目标、描述。
- 执行结果：`result_summary`。
- 执行过程：`process_log`，默认折叠。
- 前置任务：`prerequisite_task_ids`，默认折叠。
- MCP / 工作区 / 服务器配置：只展示摘要，默认折叠。
- 来源信息：`source_session_id`、`source_turn_id`、`source_user_message_id`，默认折叠。
- 原始 JSON / input payload：默认折叠，不作为主要内容。

弹窗内不要提供编辑、立即运行、取消、重试、跳转 Task Runner 页面等操作。这里是 Chatos 内的只读查看入口。

### 运行详情弹窗

参考 Task Runner 运行列表详情页：

- 现有位置：`task_runner_service/frontend/src/pages/RunsPage.tsx`

展示内容做简化和折叠：

- 基础信息：运行 ID、任务名、状态、模型、开始时间、结束时间。
- 最终结果：`result_summary`。
- 错误信息：`error_message`，有错误时展开显示。
- Report 内容：从 `report.content` 提取，默认展开；完整 `report` JSON 默认折叠。
- 工具调用摘要：只展示数量和简短列表，默认折叠。
- 模型请求 / 事件流 / input_snapshot / context_snapshot / usage：默认折叠。

运行详情的目标是帮助用户看「这个任务具体怎么跑完的」，不是完整复刻 Task Runner 后台。

## 消息到任务的关联规则

Chatos 后端收到前端请求时，先根据 `message_id` 读取消息，并校验消息所在会话属于当前登录用户。

必须保证：用户在某条消息上点击「任务」时，只能看到由这条源用户消息创建出来的任务。这里不能按会话、联系人、轮次做宽泛匹配。

源消息 ID 解析顺序：

1. 如果当前消息 `role=user`，使用当前消息自己的 `id`。
2. 如果存在 `metadata.task_runner_async.source_user_message_id`，使用它。
3. 如果存在 `metadata.historyFinalForUserMessageId`，使用它。
4. 如果都没有，返回空列表，不请求 Task Runner。

会话 ID：

- 使用当前消息的 `conversation_id` 作为 `source_session_id`。

这样可以覆盖三类常见消息：

- 用户原始消息：直接用自己的消息 ID。
- AI 创建任务后的即时规划回复：通过 `historyFinalForUserMessageId` 找回用户消息。
- Task Runner 完成后的回调结果消息：通过 `task_runner_async.source_user_message_id` 找回用户消息。

### 创建任务时的 source_user_message_id 透传要求

为了保证新任务一定能被消息准确查回，Chatos 调用 Task Runner MCP 工具时必须由程序透传源消息上下文，AI 不需要知道这些字段。

透传内容：

- `source_session_id`：当前会话 ID。
- `source_turn_id`：当前轮次 ID。
- `source_user_message_id`：本轮用户消息 ID。

Task Runner MCP HTTP 入口已经有类似上下文头的读取方式，落地时需要确认并补齐：

- `x-chatos-session-id`
- `x-chatos-turn-id`
- `x-chatos-user-message-id`

实现要求：

- Chatos 在进入 task runner 模式并调用 Task Runner MCP 时，必须把当前用户消息 ID 写入 `x-chatos-user-message-id`。
- Task Runner 创建任务时必须把 `x-chatos-user-message-id` 落到 `TaskRecord.source_user_message_id`。
- 如果是 Chatos async planner 这类专用工具画像，Task Runner 收到创建任务请求但缺少 `source_user_message_id` 时，应直接拒绝创建或返回明确错误，不能创建无来源任务。
- 子任务、前置任务、由任务拆解出的后续任务都必须继承同一个 `source_session_id` 和 `source_user_message_id`。
- 老历史数据如果缺少 `source_user_message_id`，消息抽屉只展示空状态，不做会话级兜底查询。

## Task Runner 内部只读接口

新增 Task Runner 内部接口，放在不走 `require_auth` 的路由层，但只做 Chatos 内部读取，不暴露给 Chatos 前端。

建议接口：

```text
GET /internal/chatos/message-tasks?source_session_id=...&source_user_message_id=...
GET /internal/chatos/message-tasks/:task_id?source_session_id=...&source_user_message_id=...
GET /internal/chatos/message-runs/:run_id?source_session_id=...&source_user_message_id=...
```

接口校验规则：

- `source_session_id` 和 `source_user_message_id` 必须同时存在。
- 查询任务列表时必须同时过滤：
  - `task.source_session_id == source_session_id`
  - `task.source_user_message_id == source_user_message_id`
- 查询任务详情时不能只按 `task_id` 返回，必须额外校验任务的 `source_session_id` 和 `source_user_message_id`。
- 查询运行详情时先查 run，再查 run 对应 task，必须校验 task 的 `source_session_id` 和 `source_user_message_id`。
- 校验失败统一返回 404，避免泄露任务是否存在。

可复用现有数据结构：

- `TaskRecord`
- `TaskRunRecord`
- `TaskRunEventRecord`

但对外建议定义内部 DTO，避免把以后不想展示的字段无意透出。

### 列表 DTO

```json
{
  "items": [
    {
      "id": "task_id",
      "title": "任务名",
      "description": "任务描述",
      "status": "ready",
      "last_run_id": "run_id",
      "result_summary": "最近结果摘要",
      "created_at": "...",
      "updated_at": "..."
    }
  ]
}
```

### 任务详情 DTO

返回任务详情所需字段：

- `id`
- `title`
- `description`
- `objective`
- `status`
- `priority`
- `tags`
- `default_model_config_id`
- `creator_user_id`
- `creator_username`
- `creator_display_name`
- `result_summary`
- `process_log`
- `last_run_id`
- `schedule`
- `parent_task_id`
- `source_run_id`
- `source_session_id`
- `source_turn_id`
- `source_user_message_id`
- `prerequisite_task_ids`
- `task_tool_state`
- `mcp_config`
- `input_payload`
- `created_at`
- `updated_at`

### 运行详情 DTO

返回运行详情所需字段：

- `run`
  - `TaskRunRecord` 的主要字段。
- `task`
  - 当前运行所属任务的简要信息：`id`、`title`、`status`。
- `events`
  - `TaskRunEventRecord[]`，用于折叠查看运行过程。

## Chatos 后端代理接口

新增 Chatos 后端接口，前端只调这些接口：

```text
GET /api/messages/:message_id/task-runner/tasks
GET /api/messages/:message_id/task-runner/tasks/:task_id
GET /api/messages/:message_id/task-runner/runs/:run_id
```

接口行为：

1. 使用当前登录用户鉴权。
2. 读取 `message_id` 对应消息。
3. 用 `ensure_owned_session(message.conversation_id, auth)` 校验会话归属。
4. 解析 `source_session_id` 和 `source_user_message_id`。
5. 找到当前会话/联系人对应的 Task Runner 配置。
6. 调用 Task Runner 内部接口。
7. Chatos 后端再次做防御性过滤：
   - 返回任务必须匹配 `source_session_id` 和 `source_user_message_id`。
   - 返回运行必须属于匹配的任务。
8. 返回给前端。

Task Runner 配置来源：

- 优先通过消息所在会话关联的联系人配置拿到 Task Runner base URL。
- 当前已有联系人配置读取能力：`chat_app_server_rs/src/services/chatos_memory_mappings.rs` 中的 `get_contact_task_runner_runtime_config(...)`。
- 如果某些历史消息缺少联系人元数据，可以先返回明确错误：「当前消息没有可用的任务系统配置」，不要猜默认 Task Runner 地址。

Task Runner API client 扩展：

- 现有文件：`chat_app_server_rs/src/services/task_runner_api_client.rs`
- 增加内部只读方法：
  - `list_message_tasks(base_url, source_session_id, source_user_message_id)`
  - `get_message_task(base_url, task_id, source_session_id, source_user_message_id)`
  - `get_message_run(base_url, run_id, source_session_id, source_user_message_id)`

这些方法不带 agent token，不走 Task Runner 鉴权，只调用 `/internal/chatos/...`。

## Chatos 前端实现方案

### API client

在 Chatos 前端 API client 增加：

```ts
getMessageTaskRunnerTasks(messageId: string)
getMessageTaskRunnerTask(messageId: string, taskId: string)
getMessageTaskRunnerRun(messageId: string, runId: string)
```

DTO 类型放在现有 `chat_app/src/lib/api/client/types.ts` 或相邻类型文件。

### 状态管理

建议局部状态即可，不引入全局 store：

- 当前打开的 `messageId`
- 当前抽屉任务列表
- 列表 loading / error
- 当前任务详情 modal 状态
- 当前运行详情 modal 状态

原因：

- 这是消息的附加查看能力，不影响主聊天状态。
- 数据可按需请求，关闭抽屉后可以释放。

### UI 样式

沿用 Chatos 当前 Tailwind / 自定义组件风格，不引入 Task Runner 的 Ant Design 依赖。

可参考已有抽屉：

- `chat_app/src/components/chatInterface/TurnRuntimeContextDrawer.tsx`

UI 要求：

- 右侧抽屉宽度：桌面 `max-w-2xl` 或 `max-w-3xl`，移动端全宽。
- 卡片 radius 保持 8px 以内。
- 长文本统一使用可展开组件：
  - 默认最大高度。
  - 超出后显示「展开 / 收起」。
- JSON、事件、工具调用默认折叠。
- 只读入口，不放「运行」「编辑」「删除」等会改变任务状态的操作。

## 文件落点

Task Runner 后端：

- `task_runner_service/backend/src/api/mod.rs`
- `task_runner_service/backend/src/services.rs`
- 如 DTO 较多，可新增 `task_runner_service/backend/src/api/chatos_internal.rs`

Chatos 后端：

- `chat_app_server_rs/src/api/messages.rs`：挂 `/api/messages/:message_id/task-runner/...`
- `chat_app_server_rs/src/services/task_runner_api_client.rs`：增加内部只读调用
- 必要时新增 `chat_app_server_rs/src/services/message_task_runner.rs` 承担消息解析和代理逻辑

Chatos 前端：

- `chat_app/src/components/messageItem/MessageActions.tsx`
- `chat_app/src/components/MessageItem.tsx`
- 新增 `chat_app/src/components/messageTasks/MessageTaskDrawer.tsx`
- 新增 `chat_app/src/components/messageTasks/MessageTaskCard.tsx`
- 新增 `chat_app/src/components/messageTasks/MessageTaskDetailModal.tsx`
- 新增 `chat_app/src/components/messageTasks/MessageTaskRunDetailModal.tsx`
- 新增 `chat_app/src/components/messageTasks/CollapsibleText.tsx`
- `chat_app/src/lib/api/client/...`
- `chat_app/src/i18n/messages.ts`

## 实施步骤

1. Task Runner 增加内部只读接口。
   - 列表按 `source_session_id + source_user_message_id` 查。
   - 详情和运行详情按 ID 查后再校验归属。

2. 补强 Chatos 调用 Task Runner MCP 的上下文透传。
   - 调用工具时程序透传 `source_session_id`、`source_turn_id`、`source_user_message_id`。
   - 确认 Task Runner 创建任务、子任务、前置任务都会保存并继承 `source_user_message_id`。
   - Chatos async planner 模式下缺少 `source_user_message_id` 时禁止创建任务。

3. Chatos 后端增加代理接口。
   - 校验当前用户拥有消息所在会话。
   - 从消息 metadata 解析源用户消息 ID。
   - 找联系人 Task Runner 配置。
   - 调 Task Runner 内部接口并再次过滤。

4. Chatos 前端增加 API 和类型。
   - 所有请求只打 Chatos 后端 `/api/messages/.../task-runner/...`。

5. Chatos 前端增加「任务」按钮和抽屉。
   - 点击按钮拉取列表。
   - 卡片默认只展示任务名和描述。

6. Chatos 前端增加详情和运行详情弹窗。
   - 复刻必要信息，不复刻后台操作按钮。
   - 长内容、JSON、事件全部默认折叠。

7. 编译检查。
   - `cargo check -p task_runner_service_backend`
   - `cargo check -p chat_app_server_rs`
   - `npm run type-check`（在 `chat_app`）

## 风险和处理

- 历史消息缺少 `source_user_message_id`：
  - 返回空列表，不做模糊匹配。

- 新创建任务缺少 `source_user_message_id`：
  - 这是创建链路 bug，不允许通过消息抽屉兜底修复。
  - 需要在 Chatos 调用 Task Runner MCP 时补齐透传，并在 Task Runner async planner 创建入口强校验。

- 一条消息创建多个任务：
  - 抽屉展示多张卡片。
  - 按 `created_at` 或 `updated_at` 倒序排列。

- 子任务 / 前置任务也需要展示：
  - 只要它们继承了同一个 `source_user_message_id`，列表会一起展示。
  - 详情里额外显示 `parent_task_id` 和 `prerequisite_task_ids`。

- Task Runner 内部接口被误传 task_id：
  - 详情接口必须校验 `source_session_id + source_user_message_id`，失败返回 404。

- 内容太多影响 Chatos 体验：
  - 默认只展示摘要。
  - 详情内容分组折叠。
  - 原始 JSON 永远默认折叠。

## 不做的事

- 不让 Chatos 前端直连 Task Runner。
- 不在 Chatos 消息任务抽屉里提供编辑、运行、取消、重试、删除任务。
- 不用 Task Runner 用户 token 做这组只读内部查询。
- 不通过只传 `task_id` 或 `run_id` 的方式返回数据。
- 不把 Task Runner 完整后台页面嵌进 Chatos。
