# Ask User 默认注入与 TaskRunner 回流 Chatos 方案

## 目标行为

- 在 Chatos 联系人异步任务入口里，`TaskManager` 和 `AskUser` 都作为系统默认工具自动注入，不让 AI 在 `enabled_builtin_kinds` 里选择它们。
- 任务执行模型仍然能直接调用 `ask_user_*` 工具；只是“是否启用这个工具”由程序决定。
- TaskRunner 后台任务调用 `AskUser` 时，用户能在 Chatos 当前会话里看到待处理交互，并能提交或取消。
- 用户在 Chatos 处理后，结果回写 TaskRunner 的 prompt API，唤醒等待中的工具调用，任务继续执行。
- 非 Chatos 来源的 TaskRunner 任务保持现状：prompt 记录留在 TaskRunner 自己的 Prompts/运行事件里处理。

## 当前代码观察

- TaskRunner 已经实现 `AskUser` builtin provider：
  - `task_runner_service/backend/src/services/builtin_providers/builders.rs`
  - `task_runner_service/backend/src/services/builtin_providers/provider.rs`
  - `task_runner_service/backend/src/ask_user_prompt_service.rs`
- TaskRunner `AskUserPromptService::execute_prompt` 已经会保存 `AskUserPromptRecord`、追加 run event、等待 submit/cancel/timeout。
- TaskRunner 已有 prompt REST/MCP API，可 `list/get/submit/cancel`：
  - `task_runner_service/backend/src/api/prompts.rs`
  - `task_runner_service/backend/src/mcp_server/prompt_tools.rs`
- Chatos 本地内置 `AskUser` 已经能创建本地 prompt 记录，并通过工具流发 `ask_user_prompt_required/ask_user_prompt_resolved`：
  - `chat_app_server_rs/src/services/shared_builtin_ask_user.rs`
  - `chat_app_server_rs/src/services/ask_user_prompt_manager/*`
- Chatos realtime backend 已有 `AskUserPromptRealtimePayload` 和 `publish_ask_user_prompt_updated`，但前端 realtime 类型还没有接入 `conversation.ask_user_prompt.updated`。
- Chatos 当前 TaskRunner HTTP callback 只处理任务事件，并落成 `task_runner_callback` 助手消息：
  - `chat_app_server_rs/src/api/agent_chat.rs`
- Chatos 发起 TaskRunner async planner 时已经透传：
  - `X-Chatos-Project-Id`
  - `X-Chatos-Session-Id`
  - `X-Chatos-Turn-Id`
  - `X-Chatos-User-Message-Id`
  - `X-Chatos-User-Authorization`
- TaskRunner async planner 目前只把 `TaskManager` 作为后端自动注入工具：
  - `task_runner_service/backend/src/mcp_server/chatos_async_planner/request_guards.rs`
  - `task_runner_service/backend/src/mcp_server/chatos_async_planner/schema.rs`

## 核心设计

### 1. 系统默认 builtin 工具集

在 TaskRunner async planner 内定义统一的系统默认工具集：

```rust
const SYSTEM_INJECTED_BUILTIN_KINDS: &[&str] = &[
    "TaskManager",
    "AskUser",
];
```

行为：

- `create_task`、`create_tasks_with_prerequisites` 在 Chatos async planner profile 下创建任务时，后端强制把这两个 builtin 加入 `TaskMcpConfig.enabled_builtin_kinds`。
- `update_task` 更新 `mcp_config` 时也强制保留这两个 builtin。
- async planner 的 tool schema 从 `enabled_builtin_kinds` enum 和描述里移除 `TaskManager`、`AskUser`。
- async planner 调 `list_mcp_builtin_catalog` 时隐藏 `TaskManager`、`AskUser`，避免 AI 再把它们当成可选项。
- 执行期再做一次兜底：对 `schedule.mode = contact_async` 或带 Chatos source context 的任务，在 `build_mcp_builder_parts` 合并系统默认 builtin，防止历史任务或手工 patch 丢失默认工具。

这不是过渡兼容，而是长期模型：Chatos async 任务有一组程序默认工具，AI 只选择任务特定能力，例如代码、终端、浏览器、外部 MCP。

### 2. TaskRunner 发出 Ask user prompt 生命周期 callback

新增 TaskRunner 到 Chatos 的 prompt callback，复用现有 callback URL/secret 配置：

- `TASK_RUNNER_CHATOS_CALLBACK_URL`
- `TASK_RUNNER_CHATOS_CALLBACK_SECRET`

建议事件名：

- `ask_user_prompt.required`
- `ask_user_prompt.resolved`

payload 建议：

```json
{
  "event": "ask_user_prompt.required",
  "task_id": "task-123",
  "run_id": "run-456",
  "task_title": "执行部署检查",
  "task_status": "running",
  "project_id": "project-1",
  "source_session_id": "chatos-session-id",
  "source_turn_id": "turn-id",
  "source_user_message_id": "message-id",
  "prompt": {
    "prompt_id": "prompt-789",
    "kind": "prompt_choices",
    "title": "请选择部署环境",
    "message": "任务需要确认部署目标",
    "allow_cancel": true,
    "timeout_ms": 86400000,
    "payload": {},
    "status": "pending",
    "expires_at": "2026-06-24T12:00:00Z"
  },
  "callback_at": "2026-06-24T10:00:00Z"
}
```

实现位置：

- 在 `AskUserPromptService` 增加 `AppConfig` 依赖，或拆一个 `ChatosAskUserPromptCallbackService` 注入进去。
- `execute_prompt` 保存 pending 后发送 `ask_user_prompt.required`。
- `submit_prompt`、`cancel_prompt`、`timeout_prompt` 更新状态后发送 `ask_user_prompt.resolved`。
- callback 是 best-effort：失败只记录 warn，不阻塞 TaskRunner 本地 prompt 处理。
- 只有能从 `prompt.task_id/run_id` 找到 Chatos source context 的 prompt 才发送；普通 TaskRunner 本地任务不发。

### 3. Chatos 接收并展示远程 prompt

在现有 `/api/agent/chat/task-runner/callback` 入口里增加 prompt 事件分支。

`ask_user_prompt.required`：

- 校验 callback secret。
- 根据 `source_session_id` 和 `source_user_message_id` 校验会话与消息存在。
- 在 Chatos `ask_user_prompt_requests` 中 upsert 一条本地可展示记录。
- 本地记录使用同一个 `prompt_id`，并标记来源：
  - `source = "task_runner"`
  - `external_prompt_id = TaskRunner prompt_id`
  - `external_task_id`
  - `external_run_id`
  - `external_project_id`
- 发布 `conversation.ask_user_prompt.updated`，`action = "prompt_required"`。
- 可以同步更新源 user message metadata，例如 `pending_ask_user_prompt_ids`，用于 Workbar/消息角标。

`ask_user_prompt.resolved`：

- 根据 `prompt_id` 更新本地记录状态。
- 状态映射建议统一扩展 Chatos 状态枚举：
  - `pending`
  - `ok/submitted`
  - `canceled/cancelled`
  - `timeout/timed_out`
  - `failed`
- 发布 `conversation.ask_user_prompt.updated`，`action = "prompt_resolved"`。
- 从源 user message metadata 中移除 pending prompt，保留历史。

不要为 prompt required 生成 `task_runner_callback` 助手终态消息；它是任务中间态，应该走 Ask user prompt 体验。

### 4. Chatos 用户提交/取消时代理回 TaskRunner

新增 Chatos 受保护 API：

- `GET /api/ask-user-prompts?conversation_id=...&include_pending=true`
- `POST /api/ask-user-prompts/:prompt_id/submit`
- `POST /api/ask-user-prompts/:prompt_id/cancel`

提交请求示例：

```json
{
  "values": { "env": "prod" },
  "selection": ["prod"],
  "reason": "用户确认"
}
```

处理逻辑：

- 先校验当前用户拥有 `conversation_id` 对应会话。
- 读取本地 prompt 记录，判断来源。
- `source = "chatos"`：走本地 `ask_user_prompt_manager` hub，唤醒 Chatos 本地工具调用。
- `source = "task_runner"`：调用 TaskRunner prompt API：
  - `POST /api/prompts/:id/submit`
  - `POST /api/prompts/:id/cancel`
- TaskRunner API 认证优先使用当前前端请求里的真实用户 Authorization token。TaskRunner 已按 owner scope 校验 task/prompt 访问；如后续需要 agent token，可复用 `task_runner_api_client::exchange_task_runner_token_via_user_service`。
- TaskRunner 成功后，Chatos 可以先本地更新为已提交/已取消；最终仍以 TaskRunner 后续 `ask_user_prompt.resolved` callback 做幂等确认。

需要补齐的现有能力：

- `ask_user_prompt_manager::submit_ask_user_prompt_response` 目前只在测试编译下公开，需要改成生产可用。
- `ask_user_prompt_manager` 需要提供 `get_ask_user_prompt_record` 或按 `prompt_id` 读取记录，供提交 API 判断来源。
- `task_runner_api_client` 增加 prompt submit/cancel 方法。

### 5. Chatos 前端体验

前端接入点：

- `chat_app/src/lib/realtime/types.ts` 增加 `RealtimeAskUserPromptPayloadWrapper`，并把它加入 `RealtimeEventEnvelope.payload` union。
- 新增 `useConversationAskUserPromptRealtime`，监听 `conversation.ask_user_prompt.updated`。
- 在当前会话状态里维护 pending prompt 队列。
- 复用现有 `askUserPrompt.*` i18n 文案，补 `failed` 状态文案。
- 在 Workbar 或消息区域显示 pending prompt；点击打开交互面板。
- 根据 prompt payload 渲染三类基础交互：
  - 确认/取消
  - key-value 输入
  - choices 选择
- 提交/取消调用新的 `/api/ask-user-prompts/:prompt_id/submit|cancel`。
- prompt resolved 后关闭弹窗，保留历史入口。

如果暂时只做最小闭环，优先支持 choices 和 key-value；未知 kind 显示通用 JSON 输入或只读提示并允许取消。

## 数据模型建议

### TaskRunner

`AskUserPromptRecord` 当前已有核心字段，不一定需要迁移。callback payload 里的 `project_id/source_*` 可以从关联 task/run 派生。

可选增强：

- `AskUserPromptRecord` 增加 `project_id`，方便 prompt 列表直接按项目过滤。
- `PromptListFilters` 增加 `project_id`，避免每次通过 task 过滤。

### Chatos

给 `ask_user_prompt_requests` 增加来源字段，避免把远程来源塞在 prompt JSON 里：

```sql
ALTER TABLE ask_user_prompt_requests ADD COLUMN source TEXT NOT NULL DEFAULT 'chatos';
ALTER TABLE ask_user_prompt_requests ADD COLUMN external_prompt_id TEXT;
ALTER TABLE ask_user_prompt_requests ADD COLUMN external_task_id TEXT;
ALTER TABLE ask_user_prompt_requests ADD COLUMN external_run_id TEXT;
ALTER TABLE ask_user_prompt_requests ADD COLUMN external_project_id TEXT;
CREATE INDEX IF NOT EXISTS idx_ask_user_prompt_requests_source_external
ON ask_user_prompt_requests(source, external_prompt_id);
```

Mongo 对应在 document 中写入同名字段。

## 实施步骤

1. TaskRunner async planner 默认工具注入
   - 抽 `SYSTEM_INJECTED_BUILTIN_KINDS`。
   - 替换 `ensure_builtin_task_manager_*` 为 `ensure_system_builtin_*`。
   - schema/catalog 隐藏 `TaskManager`、`AskUser`。
   - 执行期对 Chatos async 任务合并默认 builtin。

2. TaskRunner Ask user prompt callback
   - 新增 prompt callback payload/build/delivery。
   - `AskUserPromptService` pending/resolved/timeout 后触发 callback。
   - 保持 callback 失败不影响本地 prompt 状态。

3. Chatos callback handler
   - 扩展 `TaskRunnerCallbackRequest` 支持 prompt payload。
   - `ask_user_prompt.required/resolved` 走 prompt 分支。
   - upsert 本地 prompt record，发布 realtime，不生成终态助手消息。

4. Chatos prompt 操作 API
   - 暴露 list/get/submit/cancel。
   - 本地 prompt 走 hub；TaskRunner prompt 走 TaskRunner API proxy。
   - 增加 task_runner_api_client prompt submit/cancel。

5. Chatos 前端
   - 接入 `conversation.ask_user_prompt.updated`。
   - pending prompt 队列、弹窗/面板、提交/取消。
   - Workbar/history 数量刷新。

6. 收口与文案
   - 更新 async planner schema 描述：系统默认工具由后端带上，AI 不要选择。
   - TaskRunner builtin catalog 可继续展示 `AskUser` 给人看；只有 Chatos async planner 入口隐藏。

## 测试清单

### TaskRunner backend

- async planner schema 不暴露 `TaskManager`、`AskUser`。
- `create_task` 在 Chatos async profile 下自动注入两个系统 builtin。
- `create_tasks_with_prerequisites` 每个创建的任务都自动注入两个系统 builtin。
- `update_task` patch 后仍保留系统 builtin。
- `list_mcp_builtin_catalog` 在 Chatos async profile 下不返回 `TaskManager`、`AskUser`。
- contact_async 任务运行期即使 stored config 缺少 `AskUser`，执行 builder 仍注入。
- `AskUserPromptService::execute_prompt` pending 后发送 `ask_user_prompt.required` callback。
- submit/cancel/timeout 后发送 `ask_user_prompt.resolved` callback。
- 无 Chatos source context 的 prompt 不发送 callback。

### Chatos backend

- callback secret 校验继续生效。
- `ask_user_prompt.required` 创建/更新本地 prompt record，并发布 `conversation.ask_user_prompt.updated`。
- 重复 `ask_user_prompt.required` 幂等 upsert。
- `ask_user_prompt.resolved` 更新本地状态并发布 resolved realtime。
- prompt submit API 对本地 prompt 唤醒 hub。
- prompt submit/cancel API 对 TaskRunner prompt 调用 TaskRunner API，并校验当前用户拥有会话。
- TaskRunner API 失败时返回可读错误，不提前把本地 prompt 标成成功。

### Chatos frontend

- realtime required 后当前会话出现待处理 prompt。
- 用户提交 choices/key-value 后 pending prompt 消失，历史可见。
- cancel/timeout/resolved realtime 能关闭弹窗或刷新状态。
- 当前会话以外的 prompt 不弹到错误会话。

### 端到端

1. Chatos 发起联系人异步任务。
2. planner 不选择 `AskUser`，TaskRunner 存储任务仍包含系统默认 `AskUser`。
3. 任务执行模型调用 `ask_user_prompt_choices`。
4. TaskRunner 保存 pending prompt 并 callback Chatos。
5. Chatos 当前会话展示 prompt。
6. 用户提交选择。
7. Chatos proxy 到 TaskRunner `/api/prompts/:id/submit`。
8. TaskRunner tool call 返回用户选择，任务继续执行并最终发送 `task.completed/failed` callback。

## 风险与约束

- 如果 Chatos callback 不可达，TaskRunner 任务会继续等待 prompt；用户仍可在 TaskRunner Prompts 页面处理，这是保底路径。
- prompt submit 必须走真实用户权限校验，不能只靠 prompt_id，避免跨用户提交。
- callback 和 submit 都要幂等：重复 required/resolved 或重复 submit 应返回当前状态，不制造多条记录。
- TaskRunner 和 Chatos prompt status 需要统一映射，否则前端历史会出现状态不一致。
- 远程 TaskRunner prompt 的等待者在 TaskRunner 进程内，Chatos 本地 hub 不能作为权威等待机制。
