# 联系人会话从 SSE 改为 WS 长连接的异步任务方案

## 1. 这次需求的目标

结合你的描述，我把目标拆成 5 个明确能力：

1. 聊天主实时链路从 `SSE` 改成 `WebSocket` 长连接。
2. 用户给联系人发消息后，不要求后端立刻把完整处理过程持续流给前端。
3. 联系人可以先立刻回一个“已收到/我开始处理”的回复，再把真正耗时的工具执行放到后台任务里逐个执行。
4. 联系人忙碌时，用户仍然可以继续发消息；新消息不能被硬拦住。
5. 联系人忙碌时，如果新消息会形成新的后台任务，需要主动向用户确认“插队”还是“排队”。

我建议把这次改造定义为：

`聊天实时层改造 + 联系人异步任务编排 + 用户优先级交互`

## 2. 我看完代码后的现状判断

### 2.1 当前聊天主链路

前端当前是标准的“发一个请求，拿一个流，再结束”模式：

- 前端发送消息入口在 `chat_app/src/lib/store/actions/sendMessage.ts`
- 前端流式请求封装在 `chat_app/src/lib/api/client/stream.ts`
- 前端按 `SSE` 文本块解析事件在 `chat_app/src/lib/store/actions/sendMessage/sse.ts`
- 前端把流事件写回消息草稿在 `chat_app/src/lib/store/actions/sendMessage/streamEventHandler.ts`

后端当前聊天主链路也是围绕 `SSE` 设计的：

- `chat_app_server_rs/src/api/chat_v3.rs`
- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/utils/sse.rs`
- `chat_app_server_rs/src/core/chat_stream/events.rs`
- `chat_app_server_rs/src/utils/events.rs`

`agent_v3/chat/stream` 现在的语义是：

1. HTTP POST 发起一次 turn
2. 后端立刻进入本次 AI 调用
3. 工具流、thinking、chunk、complete 持续经 `SSE` 往前推
4. turn 结束后才真正收口

这个模型不适合“任务先挂后台，稍后再把最终消息推给客户端”。

### 2.2 当前前端存在的硬限制

`chat_app/src/lib/store/actions/sendMessage.ts` 里有这段逻辑：

- 当前 session 如果 `isLoading / isStreaming / isStopping`，直接拒绝再次发送

这和新需求直接冲突。新需求下，联系人即使忙，用户也要继续发消息。

### 2.3 当前后端已经有两块可直接复用的基础设施

#### A. WebSocket 样板已经存在

终端链路已经是成熟的 `WS` 模式：

- `chat_app_server_rs/src/api/terminals/ws_handlers.rs`
- `chat_app/src/components/terminal/useTerminalSocketLifecycle.ts`

这说明：

- 后端已经有 `axum ws` 实战代码
- 前端已经有带 token 的 `ws` URL 构造和生命周期管理模式

聊天链路切 `WS` 不需要从零搭骨架。

#### B. “主动向用户索要决策”的基础设施已经存在

项目里已经有：

- `ui_prompt_manager`
- `builtin/ui_prompter`
- 前端 `UiPromptPanel`
- 后端 `api/ui_prompts.rs`

关键点是：

- 后端已经能创建 prompt 记录
- 前端已经能展示选择面板
- 现在的 prompt 事件虽然主要是通过工具流事件触发，但数据结构本身足够通用

所以“联系人忙时问用户：插队还是排队”这件事，不需要重新发明交互协议，直接扩展现有 `ui_prompt` 体系就够了。

### 2.4 当前联系人能力的现状

联系人上下文、联系人命令、联系人技能加载已经在用：

- `chat_app_server_rs/src/api/chat_stream_common.rs`
- `chat_app_server_rs/src/core/chat_runtime.rs`
- `chat_app_server_rs/src/core/mcp_runtime.rs`

也就是说，联系人身份、命令、技能、插件这些“智能体人格和工具边界”已经有了。

缺的是：

- 后端任务队列
- 联系人忙碌状态机
- 多消息并发到达时的调度策略
- 一个和 `turn` 解耦的实时推送总线

## 3. 需求落地时，我建议的总架构

我建议把聊天链路拆成两层：

### 3.1 命令层

负责“客户端告诉后端要做什么”。

推荐改成 `WebSocket` 双向消息：

- `client -> server`: 发用户消息、提交 prompt 决策、心跳、重连恢复
- `server -> client`: 回执、状态更新、prompt、最终消息、错误、心跳

### 3.2 数据层

继续保留现有 HTTP 查询接口做：

- 拉历史消息
- 拉 pending ui prompt
- 拉任务列表
- 拉会话详情

也就是：

- 实时交互走 `WS`
- 查询/恢复走 `HTTP`

这是最稳的组合，不建议把所有事情都硬塞进 `WS`。

## 4. 核心设计建议

## 4.1 不要再以“单次 turn 请求”作为实时连接单位

当前 `SSE` 本质是“一个 turn 对应一个连接”。

新方案应该变成“一个 session 对应一个长连接订阅”：

- 建议新接口：`GET /api/chat/ws?session_id=...`
- 一个聊天 session 打开后，客户端持续订阅这个 session 的所有实时事件
- 用户每次发消息，只是通过这个 socket 发一条 frame，而不是再新建一次流请求

这样后端才能在“几分钟之后”把后台任务最终结果继续推回同一个会话。

## 4.2 引入联系人运行时 Actor

我建议新增一个后端运行时模块，先叫：

- `contact_runtime_hub`
- `contact_runtime_actor`
- `contact_job_scheduler`

核心职责：

1. 为每个联系人维护运行时状态
2. 为每个联系人维护待执行任务队列
3. 按顺序逐个消费任务
4. 把状态变化和最终结果推给所有订阅该 session 的客户端

### 运行时 key 不建议只用 session_id

更合理的是：

`runtime_key = (user_id, contact_agent_id, project_scope_id)`

理由：

- 你的需求说的是“每个智能体维护任务表”
- 同一个用户如果在同一项目里对同一个联系人开多个 session，本质上应共享忙碌状态和任务队列

同时每个 job 仍然要记录 `source_session_id`，因为最终消息要推回具体会话。

如果你想先降低实现复杂度，第一阶段也可以先退化成：

`runtime_key = session_id`

但这只是过渡方案，不是最终形态。

## 4.3 引入后台 Job，而不是把耗时工作绑死在前台请求里

建议新增一类“联系人异步作业”：

- `queued`
- `awaiting_priority`
- `running`
- `waiting_user_input`
- `completed`
- `failed`
- `cancelled`

每个 job 至少记录：

- `id`
- `runtime_key`
- `session_id`
- `conversation_turn_id`
- `user_message_id`
- `contact_agent_id`
- `status`
- `priority`
- `queue_position`
- `job_kind`
- `summary`
- `payload`
- `result_message_id`
- `created_at`
- `started_at`
- `finished_at`

这里我不建议复用当前 `task_manager_tasks` 表直接承载后台 job。

原因：

- 当前 `task_manager` 更像用户可编辑任务清单
- 这次新增的是运行时调度实体
- 两者生命周期、字段、状态机都不同

建议新建独立存储，例如：

- `contact_runtime_jobs`
- `contact_runtime_states`

`task_manager` 继续用于“AI 提炼出的任务卡片”，不要和“后台执行作业”混淆。

## 4.4 联系人收到消息后的处理模式

新流程建议改成两段式：

### 第一段：立即回应

用户一发消息，联系人立即给一个短回复。

建议第一阶段先用“系统生成短回执”，不要再为这一步专门跑一次完整 LLM：

- 空闲时：`收到，我开始处理，完成后给你结果。`
- 忙碌时：`我这边正在处理上一项任务，你这条我也收到了。`

原因：

- 稳定
- 延迟低
- 不额外增加模型调用
- 和“立即响应”需求完全匹配

后面如果你想更像真人，再升级成“轻量模型生成 ack”。

### 第二段：决定是否入后台任务

不是每条消息都必须进后台任务。

建议先做一个调度判定器：

- 简单消息：直接同步回答，立即结束
- 耗时消息：生成后台 job，异步执行

判定方式第一阶段建议务实一点：

1. 显式命令/工具型请求直接异步
2. 命中联系人命令、插件、技能、文件/远端/MCP 调用时异步
3. 纯闲聊、小回复、确认类消息同步

不要一开始就做复杂智能分类器，否则改造面会失控。

## 4.5 忙碌时的新消息处理策略

这是本次需求的核心。

### 推荐状态机

当联系人 `running` 时，新消息进入：

1. 立即给用户一个短 ack
2. 后端评估这条消息是否需要形成新 job
3. 如果不需要 job，直接同步回复
4. 如果需要 job，创建一个 `ui_prompt`
5. 询问用户：
   - `插队执行`
   - `排队等待`
   - `取消本次任务`

### 这部分直接复用现有 ui_prompt

建议不要新造“优先级弹窗协议”，而是直接用现有 `ui_prompt_manager`：

- 后端直接创建一个 `choice` prompt
- 选项就是 `insert_front` / `enqueue_tail` / `cancel`
- 前端继续用 `UiPromptPanel`

差异只在于：

- 现在 `ui_prompt` 多从工具流里冒出来
- 新方案里它也可以由调度器直接触发

## 4.6 实时事件协议建议

建议保留现有事件风格，但从 `SSE event` 改成 `WS json frame`。

### client -> server

```json
{ "type": "hello", "session_id": "sess_1", "last_event_id": "evt_120" }
{ "type": "message.send", "request_id": "req_1", "session_id": "sess_1", "content": "帮我排查一下这个问题", "attachments": [] }
{ "type": "ui_prompt.submit", "prompt_id": "prompt_1", "status": "ok", "selection": "enqueue_tail" }
{ "type": "ping" }
```

### server -> client

```json
{ "type": "ready", "session_id": "sess_1", "connection_id": "conn_1" }
{ "type": "message.accepted", "request_id": "req_1", "user_message_id": "msg_u_1", "turn_id": "turn_1" }
{ "type": "agent.state", "runtime_key": "rt_1", "busy": true, "current_job_id": "job_1", "queue_size": 2 }
{ "type": "message.created", "message": { "...": "..." } }
{ "type": "job.queued", "job_id": "job_2", "position": 2 }
{ "type": "job.started", "job_id": "job_2" }
{ "type": "ui_prompt.required", "prompt": { "...": "..." } }
{ "type": "ui_prompt.resolved", "prompt_id": "prompt_1", "status": "ok" }
{ "type": "job.completed", "job_id": "job_2", "result_message_id": "msg_a_9" }
{ "type": "error", "code": "xxx", "message": "xxx" }
{ "type": "pong", "ts": "2026-04-01T12:00:00Z" }
```

这里建议新增 `event_id`，支持断线恢复和去重。

## 4.7 前端 store 需要从“单流模式”改成“会话订阅模式”

当前前端 store 里的状态仍然是以“当前正在 stream 的一条消息”为中心：

- `isStreaming`
- `streamingMessageId`
- `activeTurnId`

这在新模型下不够。

建议新增两层状态：

### 会话级连接状态

- `socketConnected`
- `socketConnecting`
- `lastEventId`

### 联系人运行状态

- `agentBusy`
- `runtimeKey`
- `currentJobId`
- `queuedJobCount`
- `pendingPriorityPrompt`

发送消息时也不能再用“当前 streaming 就拒绝”的逻辑。

建议改成：

- 只有 socket 未连接或正在恢复时禁止发送
- 联系人 busy 只影响提示文案，不禁止发送

## 4.8 消息持久化策略

建议所有真正对用户可见的内容都先落库，再推送。

顺序建议是：

1. 保存用户消息
2. 推 `message.accepted`
3. 保存联系人立即 ack 消息
4. 推 `message.created`
5. 任务完成后保存最终 assistant 消息
6. 推 `message.created`

这样：

- 刷新后历史一致
- 多端同步更简单
- 断线恢复时只要补事件和拉历史即可

## 5. 我建议的改造范围

## 5.1 后端新增模块

建议新增：

- `chat_app_server_rs/src/api/chat_ws.rs`
- `chat_app_server_rs/src/services/contact_runtime_hub.rs`
- `chat_app_server_rs/src/services/contact_runtime_actor.rs`
- `chat_app_server_rs/src/services/contact_job_scheduler.rs`
- `chat_app_server_rs/src/services/contact_job_store.rs`

如果想更贴近现有目录结构，也可以放到：

- `chat_app_server_rs/src/services/chat_runtime_ws/...`

## 5.2 后端需要改造的现有模块

- `chat_app_server_rs/src/api/mod.rs`
  - 注册新 `WS` 路由
- `chat_app_server_rs/src/api/chat_v3.rs`
  - 逐步降级为兼容接口或内部复用逻辑
- `chat_app_server_rs/src/services/v3/ai_server.rs`
  - 拆出“同步回复”和“后台 job 执行”都可复用的入口
- `chat_app_server_rs/src/services/v3/message_manager.rs`
  - 增加异步 job 结果落库辅助
- `chat_app_server_rs/src/utils/events.rs`
  - 扩展 `WS` 事件类型常量
- `chat_app_server_rs/src/services/ui_prompt_manager/*`
  - 支持调度器直接发起 prompt

## 5.3 前端需要改造的现有模块

- `chat_app/src/lib/api/client/stream.ts`
  - 逐步迁移为 `chatSocket.ts`
- `chat_app/src/lib/store/actions/sendMessage.ts`
  - 改为通过 socket 发消息，不再等待 `ReadableStream`
- `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
  - 大概率会退场或被大幅收缩
- `chat_app/src/lib/store/actions/sendMessage/sse.ts`
  - 可废弃
- `chat_app/src/lib/store/actions/sendMessage/streamEventHandler.ts`
  - 保留事件归并逻辑，但输入源改成 WS frame
- `chat_app/src/lib/store/types.ts`
  - 新增 socket、agent runtime、job queue 状态
- `chat_app/src/components/chat/SessionBusyBadge.tsx`
  - 从“是否正在流式输出”改为“联系人运行状态”

## 6. 分阶段实施建议

我建议不要一次性把 `SSE + 调度 + prompt + 重连恢复` 全部一起上。

### Phase 1: 先搭聊天 WS 总线，保持现有同步回答逻辑

目标：

- 客户端建立 session 级 `WS`
- 发消息走 `WS`
- 后端仍然立即执行 AI，再把事件通过 `WS` 推回

收益：

- 先替换掉 `SSE`
- 风险最低
- 前后端事件协议能先稳定

### Phase 2: 引入联系人后台 job 和忙碌状态

目标：

- 联系人收到消息先回 ack
- 耗时任务异步化
- 完成后推最终消息
- 前端允许 busy 时继续发送

### Phase 3: 加入忙碌时优先级 prompt

目标：

- 联系人忙时收到新任务，主动弹 `插队/排队/取消`
- 后端按决策重排队列

### Phase 4: 做断线恢复、多端同步、幂等

目标：

- `event_id` 重放
- reconnect resume
- 多端订阅同一 session
- 防重复消息

## 7. 关键风险和处理建议

## 7.1 单机内存 Hub 只能先跑单实例

如果 `WS` 连接和联系人运行时状态只放内存：

- 单机没问题
- 多实例部署会有路由和广播问题

如果后面会多实例部署，建议预留：

- Redis pub/sub
- 或者统一事件总线
- 或者 sticky session

第一阶段可以先单实例实现，但文档和代码边界要预留。

## 7.2 现有 runtime guidance 只适合“当前 running turn 的侧向输入”

`runtime_guidance_manager` 现在更像：

- 正在执行的 turn 的临时补充输入

它不适合直接承担“后台 job 排队和插队”。

建议：

- `runtime_guidance` 保留原用途
- 新增独立的 `contact job scheduler`

不要把两套状态机混在一起。

## 7.3 立即 ack 如果也走模型，会让整体复杂度翻倍

所以我建议第一阶段先走系统固定回执。

这是一个明确的工程取舍，不是能力不足。

## 8. 我给你的最终建议

如果按工程成本和成功率排序，我建议这次这样做：

1. 先把聊天实时层从 `SSE` 切到 `WS`
2. 再把“耗时工具执行”从同步 turn 中抽成后台 job
3. 复用现有 `ui_prompt` 做“插队/排队/取消”决策
4. 新建独立 `contact_runtime_jobs`，不要直接复用 `task_manager_tasks`
5. 第一阶段忙碌态按 `session_id` 落地，第二阶段再升级成 `(user_id, contact_agent_id, project_scope_id)` 级别

## 9. 我建议你确认的两个产品决策

这两个点会直接影响实现方式：

### A. 任务队列的归属范围

二选一：

- 方案 A: 以 `session` 为单位维护队列
- 方案 B: 以 `联系人` 为单位维护队列

我更推荐 `方案 B`，但第一阶段可以先按 `方案 A` 实现。

### B. 立即回复的风格

二选一：

- 方案 A: 固定模板 ack
- 方案 B: 轻量模型生成 ack

我更推荐先上 `方案 A`。

## 10. 下一步实施顺序

如果你认可这个方案，我建议下一步直接进入实现，不再继续停留在设计层：

1. 先建聊天 `WS` 路由和前端 socket 生命周期
2. 把现有 `SSE event` 协议映射为 `WS frame`
3. 去掉前端“busy 就禁止 send”的限制
4. 新建联系人后台 job 表和调度器
5. 接入 `ui_prompt` 做插队/排队选择

