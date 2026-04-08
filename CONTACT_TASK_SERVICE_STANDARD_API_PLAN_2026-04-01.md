# 联系人任务服务标准接口方案

## 1. 最终边界

按你最新确认后的方案，边界应该固定成这样：

### `task service`

只负责：

- 任务标准接口
- 任务状态机
- 任务优先级
- 任务归属上下文固化
- 调度结论返回
- 任务看板

不负责：

- AI 执行
- 工具调用
- 聊天消息落库
- WebSocket 推送

### `agent_orchestrator` 后端

继续负责：

- 用户发消息入口
- AI 对话执行
- 工具调用
- 定时扫描任务
- 任务实际执行
- 聊天历史写入
- `WS` 推送给前端

### `memory`

继续负责：

- 智能体元数据
- 普通聊天历史
- 任务执行历史
- 联系人会话
- 模型配置

## 2. 核心流程

### 2.1 用户发消息创建任务

1. 用户消息进入 `agent_orchestrator`
2. `agent_orchestrator` 写普通聊天历史
3. `agent_orchestrator` 调 AI
4. AI 通过内置 `task MCP` 调 `task service`
5. `task service` 创建任务，初始状态：
   - `pending_confirm`
6. AI 返回面向用户的回复
7. `agent_orchestrator` 把回复写普通聊天历史并返回前端

### 2.2 用户确认执行

1. 用户再次确认
2. `agent_orchestrator` 调 AI
3. AI 通过 `task MCP` 调 `task service`
4. 任务从：
   - `pending_confirm`
   - 进入 `pending_execute`
5. AI 返回确认类回复
6. `agent_orchestrator` 写普通聊天历史并返回前端

### 2.3 定时任务执行

1. `agent_orchestrator` 后端定时任务运行
2. 对每个 `(user_id, contact_agent_id, project_id)` 执行槽向 `task service` 请求一个调度结论
3. `task service` 返回三种之一：
   - `task`
   - `pass`
   - `all_done`
4. `agent_orchestrator` 根据结论执行：
   - `task`: 执行该任务
   - `pass`: 本轮什么都不做
   - `all_done`: 请求一次 AI 生成结词，写入普通聊天历史并推送给前端

## 3. 执行槽主键

最重要的租户和串行主键是：

`(user_id, contact_agent_id, project_id)`

这个键同时用于：

- 租户隔离
- 串行执行约束
- 定时调度分桶
- `all_done` 批次判断

## 4. 任务硬约束

所有任务都必须固化这些字段：

- `user_id`
- `contact_agent_id`
- `project_id`
- `session_id`
- `conversation_turn_id`
- `source_message_id`
- `model_config_id`

其中：

- `user_id / contact_agent_id / project_id`
  - 不能让模型自由填写
  - 必须由 `agent_orchestrator` 后端从当前上下文注入

## 5. 智能体必须绑定模型

后台定时执行任务时，没有“当前前端选中的模型”这个上下文，所以必须在智能体里显式配置默认模型。

建议在 `memory` 的智能体模型里新增：

- `model_config_id`

涉及：

- [memory_server/backend/src/models/agents.rs](./memory_server/backend/src/models/agents.rs)
- [memory_server/backend/src/api/agents_api.rs](./memory_server/backend/src/api/agents_api.rs)
- [memory_server/frontend/src/pages/AgentsPage.tsx](./memory_server/frontend/src/pages/AgentsPage.tsx)

规则建议固定：

1. 创建任务时读取 `agent.model_config_id`
2. 固化写入 `task.model_config_id`
3. 若为空，任务允许创建
4. 但不能进入 `pending_execute`

推荐错误文案：

- `当前联系人未配置执行模型，无法进入待执行状态`

## 6. 任务状态机

建议固定：

- `pending_confirm`
- `pending_execute`
- `running`
- `completed`
- `failed`
- `cancelled`

## 7. 优先级规则

优先级在创建任务时就必须体现：

- `high`
- `medium`
- `low`

排序规则固定：

1. 先按 `priority`
2. 再按 `created_at`
3. 再按 `id`

也就是：

- `high` 优先
- 同优先级 FIFO

## 8. 串行执行规则

同一个：

`(user_id, contact_agent_id, project_id)`

在同一时刻：

- 只能有一个 `running`
- 绝不能并行执行

## 9. task service 数据模型

## 9.1 `contact_tasks`

建议字段：

- `id`
- `user_id`
- `contact_agent_id`
- `project_id`
- `session_id`
- `conversation_turn_id`
- `source_message_id`
- `model_config_id`
- `title`
- `content`
- `priority`
- `status`
- `confirm_note`
- `execution_note`
- `created_by`
- `created_at`
- `updated_at`
- `confirmed_at`
- `started_at`
- `finished_at`
- `last_error`
- `result_summary`
- `result_message_id`

## 9.2 `contact_task_runtime_locks`

建议字段：

- `user_id`
- `contact_agent_id`
- `project_id`
- `running_task_id`
- `lease_until`
- `updated_at`

唯一键：

- `(user_id, contact_agent_id, project_id)`

## 9.3 `contact_task_runs`

用于记录某个任务的执行批次。

## 9.4 `contact_task_run_logs`

用于记录：

- 开始
- 结束
- 错误
- 执行摘要

## 10. 普通聊天历史和任务执行历史必须分表

这是硬约束，不是优化项。

### 原因

如果定时任务调用 AI 和用户正常对话共用同一张消息历史表，会导致：

1. 后续模型上下文互相污染
2. 用户聊天里混入后台执行轨迹
3. 历史压缩和总结语义变脏
4. 调试与展示边界混乱

所以必须分成两套历史。

## 10.1 普通聊天历史

用于：

- 用户和联系人正常对话
- 前端聊天界面
- 普通聊天上下文

## 10.2 任务执行历史

用于：

- 定时任务执行时的内部 user/assistant/tool 消息
- 工具调用轨迹
- 任务执行审计

这套历史不能默认进入普通聊天上下文。

另外，这套历史不能只是“单独存表”，还必须同步具备：

- 定时总结能力
- 上下文拼装能力

否则任务执行历史一旦变长，后续定时任务请求 AI 时一样会失控。

## 10.3 建议表

除了现有普通聊天消息表，新增：

- `task_execution_sessions`
- `task_execution_messages`

`task_execution_messages` 至少带：

- `id`
- `task_id`
- `task_run_id`
- `user_id`
- `contact_agent_id`
- `project_id`
- `model_config_id`
- `role`
- `content`
- `tool_call_id`
- `tool_calls`
- `reasoning`
- `metadata`
- `created_at`

建议同时为任务执行历史新增对应的 summary 作用域，例如：

- `task_execution_summaries`

或者在现有 summary 建模里增加：

- `history_scope = task_execution`

原则是：

- 普通聊天 summary 和任务执行 summary 不能混算

## 10.4 上下文拼装规则

### 用户正常对话

只读取：

- 普通聊天历史

### 定时任务执行

只读取：

- 任务执行历史

这样两条链路天然隔离。

## 10.4.1 任务执行请求 AI 时，历史拼装要和现在一样

你这个补充必须落成正式约束：

- 定时任务请求 AI 时
- 历史消息的拼装方式

要尽量和现在普通用户对话链路一致，只是数据源换成 `task_execution_messages`。

也就是说，不能退化成：

- 直接把任务执行历史全量拼接后扔给模型

而应该继续复用现有策略：

- 最近消息窗口
- summary 参与
- tool 边界保持
- token 控制
- 必要时上下文压缩

相关可复用位置：

- [agent_orchestrator/src/services/message_manager_common.rs](./agent_orchestrator/src/services/message_manager_common.rs)
- [agent_orchestrator/src/services/summary/engine.rs](./agent_orchestrator/src/services/summary/engine.rs)
- [memory_server/backend/src/services/context.rs](./memory_server/backend/src/services/context.rs)

## 10.5 给用户看的结果消息怎么处理

虽然内部执行历史要分表，但任务完成后面向用户的消息仍然应该写入普通聊天历史。

也就是：

- 内部执行轨迹：
  - 写 `task_execution_messages`
- 面向用户的完成消息：
  - 写普通聊天历史

## 10.6 `all_done` 结词怎么处理

`all_done` 时生成的结词本质上也是给用户看的消息，所以：

- 写普通聊天历史
- 不写任务执行主轨迹表

## 10.7 定时总结也要对任务执行历史生效

这是新的硬约束：

- 普通聊天历史有自己的定时总结
- 任务执行历史也要有自己的定时总结

两套总结逻辑可以共用同一套总结引擎，但必须分开作用域：

### 普通聊天历史

- 面向普通聊天消息表

### 任务执行历史

- 面向 `task_execution_messages`

这样后续定时任务再次执行时，仍然能像现在一样基于：

- summary
- 最近消息

来拼装上下文，而不是从头读取全部历史。

## 11. task service 标准接口

统一前缀：

`/api/contact-task/v1`

## 11.1 给 task MCP 用的接口

### 创建任务

`POST /api/contact-task/v1/tasks`

模型可填字段：

```json
{
  "title": "排查前端白屏",
  "content": "检查首页白屏原因，优先定位最近变更",
  "priority": "high",
  "confirm_note": "建议确认后执行"
}
```

由 `agent_orchestrator` 后端自动注入：

```json
{
  "user_id": "ctx.user_id",
  "contact_agent_id": "ctx.contact_agent_id",
  "project_id": "ctx.project_id",
  "session_id": "ctx.session_id",
  "conversation_turn_id": "ctx.turn_id",
  "source_message_id": "ctx.message_id",
  "model_config_id": "ctx.agent_default_model_config_id"
}
```

### 列表查询

`GET /api/contact-task/v1/tasks`

### 任务详情

`GET /api/contact-task/v1/tasks/:task_id`

### 编辑任务

`PATCH /api/contact-task/v1/tasks/:task_id`

允许改：

- `title`
- `content`
- `priority`
- `confirm_note`

### 删除任务

`DELETE /api/contact-task/v1/tasks/:task_id`

### 确认任务

`POST /api/contact-task/v1/tasks/:task_id/confirm`

状态：

- `pending_confirm -> pending_execute`

### 取消任务

`POST /api/contact-task/v1/tasks/:task_id/cancel`

### 重试任务

`POST /api/contact-task/v1/tasks/:task_id/retry`

状态：

- `failed -> pending_execute`

## 11.2 给 agent_orchestrator 定时执行器用的接口

### 获取调度结论

`POST /api/contact-task/v1/internal/execution/next`

请求体：

```json
{
  "user_id": "user_1",
  "contact_agent_id": "agent_1",
  "project_id": "proj_1"
}
```

返回三种之一：

#### A. `task`

```json
{
  "decision": "task",
  "task": {
    "id": "task_1",
    "priority": "high",
    "status": "pending_execute"
  }
}
```

#### B. `pass`

```json
{
  "decision": "pass",
  "reason": "running_task_exists",
  "running_task_id": "task_9"
}
```

#### C. `all_done`

```json
{
  "decision": "all_done",
  "completion_batch_id": "batch_20260401_01",
  "recent_completed_task_ids": ["task_1", "task_2"]
}
```

规则：

- 如果已有 `running`，返回 `pass`
- 如果无 `running` 且有 `pending_execute`，返回当前应执行的 `task`
- 如果无 `running` 且无 `pending_execute`，并且当前批次已全部完成，返回 `all_done`

### `all_done` 必须是一次性事件

否则定时任务每扫一次都会重复给用户发总结。

所以需要补一个确认消费接口：

`POST /api/contact-task/v1/internal/execution/ack-all-done`

请求体：

```json
{
  "user_id": "user_1",
  "contact_agent_id": "agent_1",
  "project_id": "proj_1",
  "completion_batch_id": "batch_20260401_01"
}
```

### 开始执行

`POST /api/contact-task/v1/internal/tasks/:task_id/start`

状态：

- `pending_execute -> running`

### 心跳

`POST /api/contact-task/v1/internal/tasks/:task_id/heartbeat`

### 完成任务

`POST /api/contact-task/v1/internal/tasks/:task_id/complete`

### 标记失败

`POST /api/contact-task/v1/internal/tasks/:task_id/fail`

## 12. task MCP 改造

当前内置 task MCP 需要从“本地任务存储”改成“标准 client 调 task service”。

入口：

- [agent_orchestrator/src/builtin/task_manager/mod.rs](./agent_orchestrator/src/builtin/task_manager/mod.rs)

建议新增：

- `agent_orchestrator/src/services/contact_task_client.rs`

`task MCP` 的职责应固定为：

1. 取当前上下文
2. 自动注入：
   - `user_id`
   - `contact_agent_id`
   - `project_id`
   - `session_id`
   - `conversation_turn_id`
   - `source_message_id`
3. 调 task service
4. 把结果返回给 AI

模型不能自由决定：

- `user_id`
- `contact_agent_id`
- `project_id`

## 13. 定时执行器为什么必须在 agent_orchestrator 后端

因为这样：

1. 不需要 task service 反向通知 `agent_orchestrator`
2. 不需要跨服务回推 `WS`
3. 不需要跨服务处理聊天历史
4. 最大化复用现有 AI 执行链路

## 14. agent_orchestrator 内部公共模块

现在 AI 执行已经明确放在 `agent_orchestrator` 后端，所以公共逻辑不需要独立成跨服务包，抽到 `agent_orchestrator` 内部即可。

建议抽一个内部公共模块，例如：

- `agent_orchestrator/src/services/contact_runtime_common/`

抽这些内容：

### A. 联系人上下文解析

参考：

- [agent_orchestrator/src/api/chat_stream_common.rs](./agent_orchestrator/src/api/chat_stream_common.rs)

### B. 单轮联系人执行入口

例如：

`run_contact_turn(input) -> result`

输入至少包含：

- `session_id`
- `user_id`
- `contact_agent_id`
- `project_id`
- `model_config_id`
- `content`

### C. 持久化 trait

- `ConversationStore`
- `TaskExecutionStore`
- `RuntimeContextProvider`
- `ToolExecutorFactory`

### D. 历史上下文拼装器

建议再抽：

- `ConversationHistoryContextBuilder`
- `TaskExecutionHistoryContextBuilder`

两者尽量共用同一套 token、summary、tool-boundary 逻辑，只是底层消息源不同。

这样：

- 用户正常聊天可以复用
- 定时任务执行也可以复用

## 15. 任务执行的消息写入方式

### 普通用户对话

写：

- 普通聊天历史

### 定时任务执行过程

写：

- `task_execution_messages`

### 任务完成给用户的结果

写：

- 普通聊天历史

### `all_done` 结词

写：

- 普通聊天历史

## 16. 任务服务看板

任务服务要有自己的前端看板。

建议目录：

- `contact_task_service/frontend/`

### 最少页面

- 任务列表
- 任务详情
- 待确认任务
- 待执行任务
- 运行中任务
- 失败任务
- 执行日志

### 最少操作

- 查看任务
- 筛选任务
- 确认任务
- 取消任务
- 重试任务

### 权限规则

- `admin`
  - 看所有任务
- 普通用户
  - 只看自己的任务

服务端必须强制：

- 非 admin 查询自动加：
  - `where user_id = current_user_id`

## 17. 数据库索引建议

推荐表：

- `contact_tasks`
- `contact_task_runtime_locks`
- `contact_task_runs`
- `contact_task_run_logs`
- `contact_task_idempotency_keys`
- `task_execution_sessions`
- `task_execution_messages`
- `task_execution_summaries`

建议索引：

### `contact_tasks`

- `(user_id, contact_agent_id, project_id, status, priority, created_at)`
- `(user_id, status, created_at)`
- `(session_id, created_at)`
- `(source_message_id)`

### `contact_task_runtime_locks`

- 唯一键：`(user_id, contact_agent_id, project_id)`

### `task_execution_messages`

- `(task_id, created_at)`
- `(task_run_id, created_at)`
- `(user_id, contact_agent_id, project_id, created_at)`

## 18. 仓库调整建议

新增：

- `contact_task_service/`
- `contact_task_service/frontend/`

重点改造：

- [agent_orchestrator/src/builtin/task_manager/mod.rs](./agent_orchestrator/src/builtin/task_manager/mod.rs)
  - 改为使用 `contact_task_client`
- [agent_orchestrator/src/api/task_manager.rs](./agent_orchestrator/src/api/task_manager.rs)
  - 可逐步代理到新服务
- [agent_orchestrator/src/services/v3/ai_server.rs](./agent_orchestrator/src/services/v3/ai_server.rs)
  - 抽公共执行入口
- [agent_orchestrator/src/api/chat_stream_common.rs](./agent_orchestrator/src/api/chat_stream_common.rs)
  - 抽公共上下文构建逻辑
- `memory_server`
  - 新增任务执行历史表、总结表和对应接口
- [memory_server/backend/src/models/agents.rs](./memory_server/backend/src/models/agents.rs)
  - 智能体增加 `model_config_id`
- [memory_server/backend/src/api/agents_api.rs](./memory_server/backend/src/api/agents_api.rs)
  - 智能体创建/更新支持模型字段
- [memory_server/frontend/src/pages/AgentsPage.tsx](./memory_server/frontend/src/pages/AgentsPage.tsx)
  - 智能体管理增加模型列和模型选择

## 19. memory_server 需要新增的表和接口

这部分是这次方案能不能真正复用现有聊天链路的关键。

因为当前 `agent_orchestrator` 的上下文拼装不是简单查消息表，而是依赖：

- 历史消息
- summary
- compose_context

所以 task execution scope 也必须有对应能力。

## 19.1 建议新增表

### 1. `task_execution_sessions`

作用：

- 作为任务执行历史的逻辑 session
- 承载 `(user_id, contact_agent_id, project_id)` 级别的执行上下文

建议字段：

- `id`
- `user_id`
- `contact_agent_id`
- `project_id`
- `model_config_id`
- `status`
- `created_at`
- `updated_at`
- `metadata`

建议唯一键：

- `(user_id, contact_agent_id, project_id)`

### 2. `task_execution_messages`

作用：

- 存储定时任务执行时的 user / assistant / tool 消息

建议字段：

- `id`
- `task_execution_session_id`
- `task_id`
- `task_run_id`
- `role`
- `content`
- `tool_call_id`
- `tool_calls`
- `reasoning`
- `metadata`
- `created_at`

### 3. `task_execution_summaries`

作用：

- 专门为任务执行历史服务的 summary

建议字段风格尽量和现有 summary 保持一致：

- `id`
- `task_execution_session_id`
- `level`
- `summary_text`
- `summary_model`
- `source_start_message_id`
- `source_end_message_id`
- `status`
- `created_at`
- `updated_at`

如果你更想少建表，也可以复用现有 summary 表结构，但必须增加作用域字段：

- `history_scope = chat | task_execution`

但我更建议独立表，后续更清楚。

## 19.2 建议新增接口

当前普通聊天依赖 `compose_context`，所以任务执行历史也要有对应接口。

### 1. 创建或获取 task execution session

`POST /api/memory/v1/task-execution/sessions/ensure`

请求体：

```json
{
  "user_id": "user_1",
  "contact_agent_id": "agent_1",
  "project_id": "proj_1",
  "model_config_id": "model_cfg_1"
}
```

### 2. 写入 task execution message

`POST /api/memory/v1/task-execution/sessions/:session_id/messages`

### 3. 批量查询 task execution messages

`GET /api/memory/v1/task-execution/sessions/:session_id/messages`

### 4. task execution compose_context

`POST /api/memory/v1/task-execution/context/compose`

语义应尽量和现有：

- [context_api.rs](./memory_server/backend/src/api/context_api.rs)

一致，只是数据源改成 task execution scope。

### 5. task execution summary 接口

至少要有：

- `GET /api/memory/v1/task-execution/sessions/:session_id/summaries`
- `POST /api/memory/v1/task-execution/sessions/:session_id/summaries/run-once`

如果后面任务执行 summary 也要进入 worker，则再扩展统一 job 配置。

## 19.3 为什么不能只建 message 表

因为当前普通聊天链路里，真正参与模型上下文的是：

- `compose_context` 的返回

而 `compose_context` 又依赖：

- summary
- raw pending history

所以如果 task execution 只有 message 表，没有 summary/compose_context，那么你就无法保证：

- 工具返回后的重拼历史
- token 控制
- 和当前聊天一致的上下文行为

## 20. agent_orchestrator 内部公共模块建议

因为 AI 执行已经回到 `agent_orchestrator` 后端，所以公共能力应该抽在 `agent_orchestrator` 内部。

建议新模块：

- `agent_orchestrator/src/services/contact_runtime_common/`

## 20.1 建议的模块拆分

### A. `context_scope.rs`

定义统一 scope：

- `ChatConversation`
- `TaskExecution`

### B. `context_builder.rs`

统一的上下文构建入口，例如：

`build_runtime_context(scope, session_id, history_limit, ...)`

内部按 scope 选择：

- 普通聊天 `compose_context`
- 任务执行 `task_execution compose_context`

### C. `turn_runner.rs`

统一的单轮执行入口，例如：

`run_contact_turn(input: ContactTurnExecutionInput) -> ContactTurnExecutionResult`

### D. `message_store.rs`

统一 trait：

- `ConversationStore`
- `TaskExecutionStore`

### E. `summary_scope.rs`

统一 summary scope，避免以后逻辑散掉。

## 20.2 为什么要先抽模块再写任务执行器

因为现在普通聊天链路里，很多逻辑是隐含耦合的：

- 请求 AI 前拼历史
- 工具返回后重拼历史
- prev_response_id / stateless 模式切换

如果不先抽，后面定时任务执行器大概率会复制粘贴一套，最后两边行为漂移。

## 21. 两种 history scope 差异矩阵

下面这个矩阵可以作为后续实现时的硬对照表。

| 维度 | 普通聊天 | 任务执行 |
|---|---|---|
| 入口来源 | 用户前端消息 | `agent_orchestrator` 定时任务 |
| 作用域主键 | `session_id` | `task_execution_session_id` |
| 逻辑归属 | 用户会话 | `(user_id, contact_agent_id, project_id)` 执行槽 |
| 消息表 | 普通聊天消息表 | `task_execution_messages` |
| summary 表 | 普通 summary 表 | `task_execution_summaries` |
| compose_context | 现有 `/context/compose` | task execution 版 compose_context |
| 可见性 | 用户可见 | 默认用户不可见 |
| 是否参与普通聊天上下文 | 是 | 否 |
| 是否参与任务执行上下文 | 否 | 是 |
| 工具返回后是否重拼历史 | 是 | 也必须是 |
| `all_done` 结词落点 | 普通聊天消息表 | 普通聊天消息表 |
| 任务完成可见结果落点 | 普通聊天消息表 | 普通聊天消息表 |
| 中间执行轨迹落点 | 不适用 | `task_execution_messages` |

## 22. 基于现有代码的整体评估

我看完当前代码后的判断是：

### 22.1 现有链路可以复用很多，但不是“轻改”

可以复用的核心：

- `v3 ai_client` 的多轮执行模式
- 工具调用后续轮逻辑
- stateless context rebuild
- summary engine
- memory compose_context

但这不是“加个新表就行”的改动量。

### 22.2 真正难点不在 task service，而在 history scope 的完整复制

最大工作量其实是：

1. task execution scope 的 message store
2. task execution scope 的 summary
3. task execution scope 的 compose_context
4. 抽 `agent_orchestrator` 内部公共 runtime 模块

### 22.3 如果这四件事不做完整，任务执行链路会和用户聊天链路行为分叉

最常见的问题会是：

- 工具返回后上下文不一致
- 任务执行越跑越长，token 失控
- 总结缺失导致性能变差
- 定时任务和普通聊天的模型行为不一致

所以这里要提高标准，不能只做“能跑通”。

## 23. 最终结论

最终应固定成：

1. `task service` 只是任务域服务，不执行 AI
2. `agent_orchestrator` 后端才是任务执行器
3. 任务创建默认：
   - `pending_confirm`
4. 任务确认后进入：
   - `pending_execute`
5. 定时执行时 task service 返回三种结论：
   - `task`
   - `pass`
   - `all_done`
6. `pass`
   - 什么都不做，等下次扫描
7. `task`
   - `agent_orchestrator` 按 `(user_id, contact_agent_id, project_id)` 串行执行
8. `all_done`
   - `agent_orchestrator` 请求一次 AI 生成结词并推给用户
9. `all_done` 必须是一次性事件
10. 智能体必须新增：
   - `model_config_id`
11. 普通用户聊天历史和定时任务执行历史必须分表
12. 任务执行历史也必须有自己的定时总结能力
13. 定时任务请求 AI 时，历史拼装要和当前普通聊天链路一致，只是数据源换成任务执行历史
14. 任务完成后的可见结果仍然写回普通聊天历史
15. 任务服务要有自己的看板：
   - admin 看全部
   - 普通用户只看自己
