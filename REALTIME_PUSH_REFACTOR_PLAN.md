# 前端轮询接口与对话流实时化改造方案

## 1. 目标与结论

这次改造是可行的，而且值得做，但不建议把所有高频接口都改成“后端不断推全量数据”。

更合理的目标是：

1. 把前端当前的轮询接口，尽量收敛成后端实时推送。
2. 把当前对话流从 `fetch + ReadableStream + SSE 文本解析`，迁移到统一实时通道。
3. 保留 HTTP 快照接口负责“首次加载、按需详情、重连兜底”。
4. 对重接口使用“失效通知 / 增量事件”，而不是持续推送完整 payload。

推荐技术路线：

1. 新增一个全局实时通道，采用 `WebSocket-first`。
2. 终端原始输出继续保留现有专用 WebSocket。
3. 对话流、复盘状态、项目运行状态、列表失效通知、变更摘要更新，统一走新的全局实时通道。
4. 现有 SSE 对话接口先兼容保留，等前端切完后再下线。

## 2. 现状盘点

### 2.1 当前已有的长连接能力

当前项目并不是“完全没有实时能力”，而是能力分散在三套链路里：

1. 对话流：
   - 前端：`chat_app/src/lib/api/client/stream.ts`
   - 前端解析：`chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
   - 前端解析：`chat_app/src/lib/store/actions/sendMessage/streamReader.ts`
   - 前端 SSE 切片：`chat_app/src/lib/store/actions/sendMessage/sse.ts`
   - 后端：`chat_app_server_rs/src/api/chat_v2.rs`
   - 后端：`chat_app_server_rs/src/api/chat_v3.rs`
   - 后端 SSE 工具：`chat_app_server_rs/src/utils/sse.rs`

2. 本地终端 WebSocket：
   - 前端：`chat_app/src/components/terminal/useTerminalSocketLifecycle.ts`
   - 后端：`chat_app_server_rs/src/api/terminals.rs`
   - 后端：`chat_app_server_rs/src/api/terminals/ws_handlers.rs`

3. 远端终端 WebSocket：
   - 前端调用链已接入
   - 后端：`chat_app_server_rs/src/api/remote_connections/terminal_ws_api.rs`

结论很明确：

1. 你们已经有稳定的 WebSocket 认证和连接模式。
2. 当前问题不是“不会做长连接”，而是“不同域各自一套，前端还在补轮询”。
3. 这次更适合做“统一实时层”，不是继续局部补丁。

### 2.2 当前主要轮询 / 高频请求点

下表只列当前最值得治理的热点。

| 类别 | 前端位置 | 当前接口 | 当前行为 | 问题 | 建议目标 |
| --- | --- | --- | --- | --- | --- |
| 复盘状态 | `chat_app/src/components/chatInterface/useChatInterfaceController.ts` | `GET /api/conversations/:conversation_id/review-repair` | 执行复盘后持续轮询直到结束 | 同一会话状态被反复拉取 | 改成 `conversation.review_repair.*` 实时事件 |
| 项目成员面板复盘状态 | `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts` | `GET /api/conversations/:conversation_id/review-repair` | 同上 | 同一个状态在两个 UI 面板重复轮询 | 同上，统一订阅同一会话事件 |
| 项目变更摘要 | `chat_app/src/components/projectExplorer/useProjectExplorerProjectLifecycle.ts` | `GET /api/projects/:id/changes/summary` | 每 6 秒轮询 | 摘要类接口适合按变更触发，不适合固定轮询 | 改成 `project.change_summary.updated` 事件 + 按需刷新 |
| 项目运行终端发现 | `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.ts` | `GET /api/terminals` | 每 2 秒轮询 | 终端列表被高频全量拉取 | 改成 `terminal.*` / `project.run.state_changed` 事件 |
| 项目运行终端 busy 状态 | `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.ts` | `GET /api/terminals/:id` | 每 1.5 秒轮询 | 单终端状态被密集轮询 | 改成 `terminal.state_changed` 事件 |
| 运行脚本存在性检查 | `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts` | 间接调用 `GET /api/fs/entries` | 每 2.5 秒轮询直到 ready | 通过文件系统接口做状态轮询，成本高 | 改成 `project.runner.catalog.updated` 或 `project.fs.invalidated` |
| 终端面板列表刷新 | `chat_app/src/components/sessionList/useSessionListController.ts` | `GET /api/terminals` | 终端面板打开时每 2 秒轮询 | 终端列表元数据重复拉取 | 改成终端列表失效事件 |
| 侧边栏项目运行状态 | `chat_app/src/components/sessionList/useProjectRunState.ts` | `GET /api/projects/:id/contacts` + runner script state | 每 5 秒刷新非 ready 项目 | 多项目下会放大请求量 | 改成项目运行目录事件 / 成员变更事件 |
| 预览运行终端发现 | `chat_app/src/components/projectExplorer/previewRunController/useProjectPreviewTerminalPolling.ts` | `GET /api/terminals` | 每 2 秒轮询 | 与运行面板重复 | 改成共享终端 / 项目运行事件 |
| 预览运行终端 busy 状态 | `chat_app/src/components/projectExplorer/previewRunController/useProjectPreviewTerminalPolling.ts` | `GET /api/terminals/:id` | 每 1.5 秒轮询 | 同上 | 改成 `terminal.state_changed` |

### 2.3 当前高频但不适合直接改成“持续推全量数据”的接口

这些接口会频繁出现，但不建议做成“后端不断推完整结果”：

1. `GET /api/fs/entries`
2. `GET /api/fs/read`
3. `GET /api/fs/search`
4. `GET /api/fs/search-content`
5. `GET /api/conversations/:conversation_id/messages`
6. `GET /api/terminals/:id/history`
7. `GET /api/projects/:id/contacts`

原因：

1. 这些接口 payload 可能较重。
2. 这些接口更适合“首次快照 + 失效后按需重拉”。
3. 如果直接变成持续推送，很容易把实时通道变成新的重负载瓶颈。

## 3. 改造原则

### 3.1 不是所有频繁请求都应该变成全量推送

建议按三类来拆：

1. 适合实时推送状态：
   - running / busy / progress / completed / failed
   - 当前会话流式输出
   - 列表增删改元数据

2. 适合实时推送失效通知：
   - 文件树变化
   - 项目变更摘要失效
   - 会话列表、联系人列表、项目列表失效

3. 继续保留 HTTP 快照：
   - 文件树内容
   - 文件正文
   - 搜索结果
   - 历史分页数据
   - 终端历史

### 3.2 实时层要统一，但高吞吐终端流可以保留专用通道

建议把“应用状态实时事件”和“终端原始 PTY 输出”分开：

1. 新增一个全局实时通道，负责应用状态和聊天流。
2. 现有 `/api/terminals/:id/ws` 和 `/api/remote-connections/:id/ws` 继续保留，负责高频交互式终端 I/O。

这样做的好处：

1. 不用冒险重写已经跑通的终端链路。
2. 应用状态事件和终端海量输出不会互相影响。
3. 前端模型更清晰：一个全局 app realtime socket，零到多个 terminal socket。

### 3.3 实时协议要做“作用域订阅”

前端不应该连上后收全量广播，而应该显式声明当前关注范围。

建议最小订阅粒度：

1. `conversation:{id}`
2. `project:{id}`
3. `terminal:{id}`
4. `remote_connection:{id}`
5. `contacts`
6. `projects`
7. `sessions`

## 4. 目标架构

### 4.1 新增全局实时通道

建议新增：

1. `GET /api/realtime/ws`

认证方式：

1. 沿用现有 WebSocket 模式，支持 `?access_token=...`
2. 这条链路项目里已经有中间件支持，不需要新造鉴权体系

单页连接策略：

1. 每个浏览器页面维护一个全局 WebSocket
2. 页面内各个模块向它注册订阅
3. 由统一的 realtime store 分发到 chat、sessionList、projectExplorer 等子模块

### 4.2 客户端消息协议

建议客户端向服务端发送这些消息：

```json
{ "type": "subscribe", "topics": [{ "scope": "conversation", "id": "conv_1" }] }
{ "type": "unsubscribe", "topics": [{ "scope": "project", "id": "proj_1" }] }
{ "type": "ping" }
```

建议服务端返回这些消息：

```json
{
  "type": "event",
  "event": "chat.turn.delta",
  "scope": { "conversation_id": "conv_1", "turn_id": "turn_1" },
  "ts": "2026-04-29T12:00:00Z",
  "seq": 123,
  "payload": { "delta": "hello" }
}
```

```json
{ "type": "ack", "acked": "subscribe" }
{ "type": "pong", "ts": "2026-04-29T12:00:00Z" }
{ "type": "error", "code": "invalid_topic", "message": "..." }
```

### 4.3 前端数据流模型

推荐统一成：

1. HTTP 负责“命令”和“首次快照”
2. WebSocket 负责“后续实时变化”

也就是：

1. 页面打开时先 `GET` 一次基础数据
2. 然后订阅对应 scope
3. 后端一有变化就推事件
4. 前端收到事件后：
   - 要么直接 patch 本地 store
   - 要么只标记某个 scope dirty，再按需重拉快照

### 4.4 重连策略

第一阶段不强求做复杂事件回放，建议先做简单可靠版：

1. WebSocket 断开后自动重连
2. 重连成功后重新订阅当前 scope
3. 对关键 scope 重新拉一次 HTTP 快照兜底

这比一开始就做全量 event replay 更稳。

## 5. 事件模型设计

### 5.1 对话流事件

当前 SSE 里的事件名是：

1. `start`
2. `chunk`
3. `thinking`
4. `tools_start`
5. `tools_stream`
6. `tools_end`
7. `complete`
8. `cancelled`
9. `error`
10. `runtime_guidance_queued`
11. `runtime_guidance_applied`

这套命名用于 SSE 还可以，但如果并入全局实时总线，建议做命名空间化：

1. `chat.turn.started`
2. `chat.turn.delta`
3. `chat.turn.thinking`
4. `chat.tool.started`
5. `chat.tool.delta`
6. `chat.tool.completed`
7. `chat.turn.completed`
8. `chat.turn.cancelled`
9. `chat.turn.failed`
10. `chat.runtime_guidance.queued`
11. `chat.runtime_guidance.applied`
12. `chat.task_board.updated`
13. `chat.ui_prompt.required`
14. `chat.ui_prompt.resolved`

这样以后不容易和别的业务事件冲突。

### 5.2 复盘与总结事件

建议新增：

1. `conversation.review_repair.started`
2. `conversation.review_repair.progress`
3. `conversation.review_repair.completed`
4. `conversation.review_repair.failed`
5. `conversation.summaries.updated`

关键 payload 字段至少要有：

1. `conversation_id`
2. `running`
3. `pending_message_count`
4. `job_id`
5. `error`

这样前端就可以正确控制：

1. 复盘按钮 loading
2. 复盘按钮禁用
3. 输入框禁用
4. memory / summary 面板刷新

### 5.3 项目与运行态事件

建议新增：

1. `project.change_summary.updated`
2. `project.run.state_changed`
3. `project.run.catalog.updated`
4. `project.members.updated`
5. `project.fs.invalidated`

其中：

1. `project.change_summary.updated` 只表示摘要失效或已更新，不要直接推完整大摘要
2. `project.run.state_changed` 负责 running / busy / terminal_id / last_active_at 这类轻量状态
3. `project.run.catalog.updated` 负责运行脚本是否存在、默认 target 是否变化
4. `project.fs.invalidated` 只推受影响路径，不推整棵目录树

补充约束：

1. `project.members.updated` 到前端后，优先走项目成员共享 cache 的局部 patch / 单域刷新
2. 不把它重新退化成每次都整批重拉 `/projects/:id/contacts`
3. HTTP `listProjectContacts` 继续保留为首次快照、跨入口兜底和 cache miss 回源

### 5.4 终端元数据事件

建议新增：

1. `terminal.created`
2. `terminal.updated`
3. `terminal.deleted`
4. `terminal.state_changed`
5. `terminal.list.invalidated`

说明：

1. `terminal.output` 仍建议留在现有专用终端 WS，不强行并到全局实时通道
2. 侧边栏、项目运行面板、预览运行面板消费的是元数据事件，不是原始终端流

### 5.5 全局列表失效事件

建议新增：

1. `contacts.updated`
2. `projects.updated`
3. `sessions.updated`
4. `remote_connections.updated`

这些事件默认只做“失效提醒”，前端收到后可以：

1. 如果当前面板正在展示该列表，则静默刷新一次
2. 如果当前面板不在前台，则先标记 dirty，等用户打开时再刷新

## 6. 分域改造建议

### 6.1 复盘状态改造

当前问题：

1. 同一个 `review-repair` 状态被两个面板重复轮询
2. loading 结束时机依赖轮询结果
3. memory 刷新也依赖轮询收敛

建议方案：

1. `POST /api/conversations/:conversation_id/review-repair` 保留，仍作为“发起命令”
2. 发起成功后，后端立即推 `conversation.review_repair.started`
3. 执行过程中推 `progress`
4. 结束时推 `completed` / `failed`
5. 完成后再推 `conversation.summaries.updated`

前端行为改成：

1. 点击复盘后立即进入 loading
2. loading 直到收到 `completed` / `failed`
3. 期间禁用重复点击和输入
4. `pending_message_count === 0` 时按钮置灰

### 6.2 项目变更摘要改造

当前问题：

1. `GET /api/projects/:id/changes/summary` 每 6 秒固定轮询
2. 但真正需要刷新摘要的时机，其实是项目文件 / git 状态变化之后

建议方案：

1. 项目页首次打开时拉一次摘要
2. 服务端在以下情况发布 `project.change_summary.updated`：
   - 应用内文件增删改
   - 终端执行导致工作区变化
   - 外部文件系统变化被 watcher 捕获
   - 外部 git 变化被 watcher / scanner 捕获
3. 前端收到事件后再静默调用一次 `GET /api/projects/:id/changes/summary`

重点：

1. 推的是“摘要已变”，不是“完整摘要全文”
2. 这样可以避免把摘要 payload 变成高频广播

### 6.3 项目运行与预览运行改造

当前问题：

1. 项目运行面板和预览运行面板都在轮询终端列表与终端 busy 状态
2. 这两个面板读的是同一批底层事实

建议方案：

1. 后端统一发布 `project.run.state_changed`
2. 后端统一发布 `terminal.state_changed`
3. 运行面板、预览面板、侧边栏共享同一份 store

具体落点：

1. 是否存在运行中的 terminal：由 `project.run.state_changed` 决定
2. terminal busy：由 `terminal.state_changed` 决定
3. 打开 terminal 详情时，仍然连接现有 `/api/terminals/:id/ws`

### 6.4 runner script / catalog 改造

这是本次方案里一个容易忽略但很关键的点。

当前 `runnerScriptExists` 轮询，本质上是在前端用文件系统接口反复判断某个脚本文件是否出现。这说明“状态源头不在前端”。

更合理的责任划分是：

1. 前端不再轮询文件系统
2. 后端负责监控项目根目录下 runner script 的变化
3. 检测到变化后推 `project.run.catalog.updated`

建议实现：

1. 后端新增 project watcher 服务
2. watcher 只关注：
   - 项目根目录变化
   - runner script 路径变化
   - 项目运行配置相关文件变化
3. watcher 做 debounce，避免一次保存触发大量事件

### 6.5 文件树与文件内容改造

不建议把文件树改成“后端持续推整棵树”。

建议方案：

1. `GET /api/fs/entries` 保留
2. `GET /api/fs/read` 保留
3. 新增 `project.fs.invalidated`

`project.fs.invalidated` payload 建议包含：

1. `project_id`
2. `paths`
3. `reason`
4. `kind`

前端收到后：

1. 如果受影响路径当前在展开目录内，则刷新该目录
2. 如果受影响路径是当前打开文件，则按策略提示或自动刷新
3. 如果无关，则只更新脏标记

### 6.6 联系人、项目、会话列表改造

这些接口不一定高频轮询，但它们适合统一进入实时层：

1. `GET /api/contacts`
2. `GET /api/projects`
3. `GET /api/conversations`
4. `GET /api/remote-connections`

建议模式：

1. 首次进入页面时 HTTP 拉取
2. 列表发生增删改时后端推 `contacts.updated` / `projects.updated` / `sessions.updated`
3. 前端只在需要时重拉对应列表

## 7. 对话流从 SSE 迁到统一实时层

### 7.1 当前情况

当前对话流虽然看起来像 SSE，但实际上不是浏览器 `EventSource` 模式，而是：

1. 前端 `fetch(POST /api/agent_v2/chat/stream)` 或 `fetch(POST /api/agent_v3/chat/stream)`
2. 拿到 `ReadableStream`
3. 手工解析 SSE 文本片段

这套方式的问题：

1. 它只解决“当前一次对话流式返回”
2. 不能顺手承载其它实时事件
3. 前端需要维护一套专门的 SSE 文本解析器
4. 后续如果还要再把复盘、任务、UI prompt 等并入实时链路，会继续分裂

### 7.2 推荐目标

建议把聊天改成：

1. HTTP 负责发起命令
2. WebSocket 负责返回流式事件

推荐新增命令接口：

1. `POST /api/agent_v2/chat/send`
2. `POST /api/agent_v3/chat/send`

命令返回只需要：

```json
{
  "accepted": true,
  "conversation_id": "conv_1",
  "turn_id": "turn_1"
}
```

随后流式内容通过全局 realtime socket 推送：

1. `chat.turn.started`
2. `chat.turn.delta`
3. `chat.turn.thinking`
4. `chat.tool.*`
5. `chat.turn.completed`
6. `chat.turn.cancelled`
7. `chat.turn.failed`

### 7.3 为什么不建议第一阶段直接把“发消息”也改成 WS 指令

从工程风险上看，第一阶段保留 HTTP 命令更稳，原因有三点：

1. 现有消息发送链路、附件参数、鉴权、错误码都已经稳定
2. HTTP 更适合做幂等提交和明确的同步错误返回
3. 真正的性能问题在“持续读取流”和“额外轮询”，不在“发起命令这一个 POST”

所以推荐顺序是：

1. 先把 SSE 返回流迁到 WebSocket 事件
2. 等这层稳定后，再评估是否需要把发送命令本身也并入 WS

### 7.4 后端改造方式

建议把当前聊天回调从“只会发 SSE”抽象成统一 sink：

1. `SseSender` 继续保留
2. 新增 `RealtimeSender`
3. 聊天流回调面向一个统一 `StreamEventSink` trait

这样：

1. 迁移期间可以同时支持 SSE 与 WS
2. 前端可以逐步切换
3. 回滚也简单

### 7.5 前端改造方式

`sendMessage` 链路建议改成：

1. 先确保全局 realtime socket 已连接
2. 订阅当前 `conversation:{id}`
3. `POST /api/agent_v2/chat/send` 或 `/api/agent_v3/chat/send`
4. 本地先插入 pending user / assistant 占位消息
5. 后续内容完全由 `chat.turn.*` 事件驱动

这样可以下掉这些前端专用 SSE 解析模块：

1. `chat_app/src/lib/store/actions/sendMessage/streamReader.ts`
2. `chat_app/src/lib/store/actions/sendMessage/sse.ts`

## 8. 后端实现建议

### 8.1 新增 realtime hub

建议新增模块，例如：

1. `chat_app_server_rs/src/services/realtime_hub/`
2. `chat_app_server_rs/src/api/realtime.rs`

核心职责：

1. 管理用户级连接
2. 管理 topic subscription
3. 向匹配 scope 的连接 fanout 事件
4. 提供统一 publish API 给各业务模块调用

### 8.2 业务事件发布接入点

建议逐步接到这些地方：

1. `review_handlers`
2. `summary_handlers`
3. `projects` 相关 API
4. `fs` mutate handlers
5. terminal manager state 变更点
6. runtime guidance manager
7. chat stream callbacks

### 8.3 watcher / scanner 服务

如果变化不是通过应用 API 产生，而是外部直接改动本地文件、git 状态或脚本文件，仅靠 API 发布事件不够。

因此建议后端补一层 watcher / scanner：

1. 文件系统 watcher：
   - 监听项目根目录相关变更
   - 触发 `project.fs.invalidated`
   - 触发 `project.run.catalog.updated`

2. 项目变更摘要 scanner：
   - 对 watcher 事件做 debounce
   - 必要时重新计算项目变更摘要
   - 发布 `project.change_summary.updated`

3. 终端状态事件：
   - 复用现有 terminal manager 的订阅机制

### 8.4 事件聚合与节流

要避免实时层变成“另一种洪水”。

建议后端加两层保护：

1. 对同一 project 的文件变化事件做 300 到 1000 毫秒 debounce
2. 对列表失效事件做 coalesce，例如同一秒内多个联系人变化只发一次 `contacts.updated`

## 9. 前端实现建议

### 9.1 新增统一 realtime client

建议新增例如：

1. `chat_app/src/lib/realtime/client.ts`
2. `chat_app/src/lib/realtime/provider.tsx`
3. `chat_app/src/lib/realtime/topics.ts`

职责：

1. 建立全局 WebSocket
2. 自动重连
3. topic 引用计数订阅
4. 事件分发
5. 前后台面板切换时动态订阅 / 退订

### 9.2 用 subscription 替换 polling hooks

建议按下面顺序替换：

1. `useChatInterfaceController.ts`
2. `useTeamMembersRuntimeResources.ts`
3. `useProjectExplorerProjectLifecycle.ts`
4. `useProjectRunnerTerminalPolling.ts`
5. `useProjectRunnerCatalogState.ts`
6. `useSessionListController.ts`
7. `useProjectRunState.ts`
8. `useProjectPreviewTerminalPolling.ts`

### 9.3 Store 更新策略

建议分两种：

1. 轻量状态直接 patch：
   - busy
   - running
   - pending count
   - current turn delta

2. 重数据收到 invalidation 后再拉：
   - 文件树
   - 变更摘要全文
   - 列表快照
   - 历史消息分页

## 10. 分阶段落地计划

### Phase 0：打基线

目标：

1. 统计当前高频接口 QPS
2. 确认各页面真实订阅范围
3. 为改造设置 feature flag

产出：

1. `realtime_enabled`
2. `chat_realtime_stream_enabled`
3. 埋点：轮询次数、WS 连接数、重连数、事件量

### Phase 1：上线全局 realtime hub

目标：

1. 打通 `/api/realtime/ws`
2. 支持 subscribe / unsubscribe / ping
3. 先接简单失效事件

优先事件：

1. `contacts.updated`
2. `projects.updated`
3. `sessions.updated`
4. `remote_connections.updated`

### Phase 2：替换复盘状态与项目摘要轮询

目标：

1. 下掉 `review-repair` 状态轮询
2. 下掉项目页固定 6 秒摘要轮询

优先事件：

1. `conversation.review_repair.*`
2. `conversation.summaries.updated`
3. `project.change_summary.updated`

### Phase 3：替换项目运行与终端元数据轮询

目标：

1. 下掉 `listTerminals()` 发现轮询
2. 下掉 `getTerminal()` busy 状态轮询
3. 下掉 runner script 轮询

优先事件：

1. `project.run.state_changed`
2. `project.run.catalog.updated`
3. `terminal.state_changed`
4. `terminal.list.invalidated`
5. `project.fs.invalidated`

当前落地顺序补充：

1. 先收掉“动作成功后立刻强刷”的显式重复请求
2. 再清理 terminal socket 生命周期里与全局 realtime 重叠的列表刷新
3. 始终保留 `HTTP snapshot` 作为 realtime 断线时的兜底

### Phase 4：迁移对话流

目标：

1. 新增 chat send 命令接口
2. chat delta 改走 realtime socket
3. 前端不再手工解析 SSE 文本

迁移策略：

1. 先让后端同时支持 SSE 与 WS
2. 前端灰度切 realtime
3. 观察稳定后再移除旧 SSE 依赖

### Phase 5：清理旧链路

目标：

1. 移除不再需要的 polling hooks
2. 移除 chat SSE 解析模块
3. 保留 SSE endpoint 一段时间作为回滚兜底
4. 稳定后正式下线旧接口

## 11. 风险与控制点

### 11.1 风险：把重 payload 直接推送，导致实时通道拥塞

控制：

1. 文件树、摘要全文、历史分页不做持续推送
2. 推轻量状态和 invalidation

### 11.2 风险：外部文件变化不经过应用 API，事件发不出来

控制：

1. 补后端 watcher / scanner
2. 重连后强制快照刷新兜底

### 11.3 风险：事件顺序错乱导致 UI 状态异常

控制：

1. 每条事件带 `seq` 和 `ts`
2. chat 事件带 `conversation_id` 和 `turn_id`
3. 前端忽略旧序号事件

### 11.4 风险：WebSocket 断开后状态丢失

控制：

1. 自动重连
2. 重连后重新订阅
3. 关键 scope 自动补一次 HTTP 快照

### 11.5 风险：一次性改太多，难以回滚

控制：

1. 全部通过 feature flag 灰度
2. 每个域独立切换
3. SSE 与轮询链路在迁移期保留

## 12. 最终建议

这次改造建议不要简单理解成“把轮询换成长连接”，而是做成一套明确分层：

1. 全局 WebSocket 负责应用实时状态
2. 终端专用 WebSocket 继续负责高吞吐终端流
3. HTTP 快照接口继续负责首次加载与重连兜底
4. chat SSE 迁到 WebSocket 事件，但第一阶段保留 HTTP 发命令

如果按性价比排序，我建议优先做：

1. `review-repair` 状态实时化
2. `project.change_summary` 由轮询改成 invalidation 驱动
3. 项目运行 / 预览运行 / 终端列表元数据实时化
4. 对话流从 SSE 迁到统一 realtime socket

这样改完以后，前端请求图会明显干净很多，而且后续再做任务、UI prompt、memory、team member runtime 等联动时，也不会继续到处补轮询。

## 13. 实施补充约束

### 13.1 `force refresh` 不能复用旧 inflight

在这轮实施里又确认了一条很重要的约束：

1. 共享 snapshot cache 可以做 `inflight` 去重，但 `force: true` 不能继续复用旧 inflight。
2. 否则前端虽然传了强制刷新，实际等到的还是变更前那次请求结果，会出现：
   - realtime 事件到了但 UI 还是旧数据
   - 手动刷新看起来成功，实际没有真正回源
   - mutation 后补刷被上一轮请求“吞掉”
3. 后续继续改造剩余快照链时，需要统一遵守：
   - 普通加载：允许 cache / inflight 去重
   - 强制刷新：绕过旧 inflight，发起新的 HTTP 请求
