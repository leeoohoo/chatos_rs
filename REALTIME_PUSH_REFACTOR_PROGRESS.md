# 实时化改造进度

## 当前目标

按照 `docs/plans/REALTIME_PUSH_REFACTOR_PLAN.md` 分阶段推进，把高频轮询和零散流式链路逐步收敛到统一 realtime 通道。

## 剩余高价值缺口

- 已补根目录执行计划：
  - `docs/plans/REALTIME_REMAINING_GAPS_EXECUTION_PLAN.md`
- 当前确认的高价值剩余项：
  - 聊天主链仍保留真实 SSE fallback，尚未完全收成“长链接主路径”
  - `project runner catalog` 仍以 invalidation 后 HTTP snapshot 为主，局部 patch 还有提升空间
  - 聊天终态里 `cancelled / failed` 仍未像 `completed` 一样携带可直接对账的持久化回执，前端仍保留少量 `loadMessages()` 兜底
  - 四类高频列表虽然已经支持 payload patch，但冷启动和少量无 id invalidation 仍保留整表 HTTP fallback

## 已完成

### 1. 全局 realtime 基础设施

- 后端新增全局 WebSocket 入口：
  - `chat_app_server_rs/src/api/realtime.rs`
  - 路径：`/api/realtime/ws`
- 后端新增 realtime hub：
  - `chat_app_server_rs/src/services/realtime/mod.rs`
  - `chat_app_server_rs/src/services/realtime/hub.rs`
  - `chat_app_server_rs/src/services/realtime/types.rs`
- 前端新增统一 realtime client / provider：
  - `chat_app/src/lib/realtime/client.ts`
  - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
  - `chat_app/src/lib/realtime/buildWsUrl.ts`
  - `chat_app/src/lib/realtime/types.ts`
- 应用根部已接入 provider：
  - `chat_app/src/App.tsx`

### 2. review-repair 实时化

- 后端 `review-repair` 发起后会推送：
  - `conversation.review_repair.started`
  - `conversation.review_repair.completed`
  - `conversation.review_repair.failed`
  - `conversation.summaries.updated`
- 主要接入点：
  - `chat_app_server_rs/src/api/sessions/review_handlers.rs`
- 前端新增复盘专用 realtime hook：
  - `chat_app/src/lib/realtime/useReviewRepairRealtime.ts`
- 前端两处主入口已去掉原来的持续轮询：
  - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
  - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
- WebSocket 不可用时仍保留短轮询兜底，避免体验退化。

### 3. 终端列表 / 项目运行态 实时化

- 后端新增终端与项目运行相关 realtime 事件：
  - `terminal.state_changed`
  - `terminal.list.invalidated`
  - `project.run.state_changed`
  - `project.run.catalog.updated`
- 主要后端接入点：
  - `chat_app_server_rs/src/services/terminal_manager/session.rs`
  - `chat_app_server_rs/src/services/terminal_manager/manager.rs`
  - `chat_app_server_rs/src/api/projects/contact_handlers.rs`
  - `chat_app_server_rs/src/builtin/code_maintainer/storage.rs`
- 前端新增终端 / 项目运行 realtime hooks：
  - `chat_app/src/lib/realtime/useTerminalListRealtime.ts`
  - `chat_app/src/lib/realtime/useTerminalStateRealtime.ts`
  - `chat_app/src/lib/realtime/useProjectRunRealtime.ts`
- 会话列表终端面板已去掉固定 2 秒轮询，改为终端列表失效事件驱动：
  - `chat_app/src/components/sessionList/useSessionListController.ts`
- 项目运行面板已去掉终端发现 / busy 状态轮询，改为终端与项目运行事件驱动：
  - `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.ts`
- 预览运行面板已去掉终端发现 / busy 状态轮询，改为终端与项目运行事件驱动：
  - `chat_app/src/components/projectExplorer/previewRunController/useProjectPreviewTerminalPolling.ts`
- 侧边栏项目运行目录状态已去掉 5 秒自旋，改为初始化快照 + realtime 刷新：
  - `chat_app/src/components/sessionList/useProjectRunState.ts`
- 项目页 runner catalog 已去掉 2.5 秒自旋，改为 `project.run.catalog.updated` 触发刷新：
  - `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts`
- 项目运行目录与侧边栏运行态的 realtime 刷新粒度又收紧了一层：
  - `project.run.catalog.updated` 现在只刷新 runner 脚本侧，不再顺手重拉项目成员
  - `project.members.updated` 现在只刷新成员侧，不再顺手重查 runner 脚本
  - 侧边栏 `useProjectRunState` 已拆成 members/script 两条定向刷新，减少事件到达后的整块 HTTP 回拉
- runner 脚本存在性查询补上了 client 侧缓存，并在 `project.run.catalog.updated` / 手动分析时失效
- 这样项目页与侧边栏同时关注同一项目时，不会重复打文件系统接口检查 `.chatos/project_runner.sh`
- `project.run.catalog.updated` 现在又往前收了一层：
  - 后端 payload 会直接附带 `runner_script_exists / root_missing` 快照
  - 前端项目页与侧边栏运行态收到该事件后，会优先本地 patch runner script 状态
  - 只有 payload 没带快照字段时，才继续回退到旧的文件系统 HTTP 检查

## 已验证

- 前端：`chat_app` `npm run build` 通过
- 后端：`chat_app_server_rs` `cargo check` 通过
- 本轮新增验证：
  - 前端：`chat_app` `npm run build` 再次通过（包含 `RealtimeClient` 生命周期修复与 `useWorkbarMutations` 本地 patch 修复）
  - 前端：`chat_app` `npm run build` 再次通过（包含 `RealtimeProvider` 订阅上下文拆分与 topic 订阅稳定化修复）

## 正在进行

### 4. project change summary 事件化

已完成：

- 后端新增 `project.change_summary.updated`
- 当前已接入的事件发布点：
  - 确认变更 `POST /api/projects/:id/changes/confirm`
  - 代码维护器写入 `mcp_change_logs`
- 主要修改文件：
  - `chat_app_server_rs/src/api/projects/change_handlers.rs`
  - `chat_app_server_rs/src/builtin/code_maintainer/storage.rs`
  - `chat_app_server_rs/src/services/realtime/types.rs`
  - `chat_app_server_rs/src/services/realtime/hub.rs`
- 前端项目页摘要刷新已改成事件驱动：
  - 新增 `chat_app/src/lib/realtime/useProjectChangeSummaryRealtime.ts`
  - `chat_app/src/components/projectExplorer/useProjectExplorerProjectLifecycle.ts`
  - `chat_app/src/components/projectExplorer/useProjectExplorerEffects.ts`
  - `chat_app/src/components/projectExplorer/workspaceModelBuilders.ts`
- 原来的固定 6 秒轮询已移除：
  - `useProjectExplorerSummaryPolling` 已被替换

当前仍待补强：

- 远程终端内部产生的细粒度 workspace 脏路径，后续还可以继续补更精准的事件源，降低 watcher 全盘扫描频率
- 剩余一些高频轮询接口还没收口到 realtime，例如部分 Git 视图

本轮已完成：

- 已接入后端 `workspace_realtime_watcher`，服务启动时自动运行
- 工作区外部文件变化现在会被后台扫描并合成写入 `mcp_change_logs`
- 这条链路会复用现有 `project.change_summary.updated`，让 `GET /projects/:id/changes/summary` 刷新后真正出现变化
- 当命中 `.chatos/project_runner.sh` 时，会额外推送 `project.run.catalog.updated`
- `FS mutate` 与 `code maintainer` 受控写路径已接入 watcher suppression / dirty-path 唤醒，避免同一次受控写被 watcher 重复记账
- 远程 SFTP 传输状态已接入 realtime 事件 `remote.sftp.transfer.updated`
- 前端 `useRemoteSftpTransfer` 已改成“realtime 优先 + 断线回退轮询”，正常连接下不再依赖 `350ms` 轮询拉取传输状态
- Git 面板 summary 已去掉固定 `15s` 刷新，改成 `project.change_summary.updated` 事件驱动，保留 focus 与手动刷新兜底
- `task board / ui prompt` 已新增会话级 realtime 事件：
  - `conversation.task_board.updated`
  - `conversation.ui_prompt.updated`
- 后端任务确认、任务增删改、UI Prompt 创建/解决 现在会统一推送 realtime 事件：
  - `chat_app_server_rs/src/services/task_manager/review_hub.rs`
  - `chat_app_server_rs/src/services/task_manager/store/create_ops.rs`
  - `chat_app_server_rs/src/services/task_manager/store/write_ops.rs`
  - `chat_app_server_rs/src/services/ui_prompt_manager/store/write_ops.rs`
- 前端工作台和 UI Prompt 面板已接入会话级 realtime：
  - `chat_app/src/lib/realtime/useConversationTaskBoardRealtime.ts`
  - `chat_app/src/lib/realtime/useConversationUiPromptRealtime.ts`
  - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
- 聊天主模型已接入全局 panel realtime，同一用户下非当前会话也能同步 review / ui prompt 面板状态：
  - `chat_app/src/components/chatInterface/useGlobalConversationPanelsRealtime.ts`
  - `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
- 这轮不是持续推全量任务列表，而是“面板状态同步 + 事件驱动失效刷新”：
  - review required / resolved 用 realtime 直接同步 panel
  - task created / updated / deleted 用 realtime 触发当前 turn 与历史任务刷新
  - ui prompt required / resolved 用 realtime 同步 panel，并刷新 history
- 当前会话的 task / ui prompt 变更已进一步收敛成“realtime 优先，断线回退”：
  - WebSocket 已连接时，workbar mutation、review confirm/cancel、ui prompt submit/cancel 不再额外主动强刷
  - `review_required / review_confirmed / review_cancelled` 不再误触发任务列表刷新，只保留 panel 同步
  - 会话级 task/ui-prompt realtime 增加了最小 inflight / queued 去重，避免同一批事件连发时重复拉取
  - 相关文件：
    - `chat_app/src/components/chatInterface/useWorkbarMutations.ts`
    - `chat_app/src/components/chatInterface/usePanelActions.ts`
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
- 聊天主链路的 realtime 后端镜像层已落地：
  - 新增 `chat stream` 专用 realtime payload，开始把原来只发 SSE 的流式事件镜像到全局 realtime 通道
  - 后端聊天流事件发送已抽象成统一 `ChatEventSink`，同一份流式事件现在可以同时发给：
    - 现有 SSE 响应
    - 新的 `/api/realtime/ws`
  - 当前已镜像的聊天流事件类型包括：
    - `chat.turn.started`
    - `chat.turn.delta`
    - `chat.turn.thinking`
    - `chat.tool.started`
    - `chat.tool.delta`
    - `chat.tool.completed`
    - `chat.tools.unavailable`
    - `chat.context_summarized.started`
    - `chat.context_summarized.delta`
    - `chat.context_summarized.completed`
    - `chat.runtime_guidance.applied`
    - `chat.turn.completed`
    - `chat.turn.cancelled`
    - `chat.turn.failed`
  - 这一步还没有切前端消费，只是先把后端“双发兼容层”搭起来，方便下一轮把 `sendMessage` 从 SSE 逐步迁到 realtime
  - 相关文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/core/chat_stream/events.rs`
    - `chat_app_server_rs/src/core/chat_stream/callbacks.rs`
    - `chat_app_server_rs/src/api/chat_v2.rs`
    - `chat_app_server_rs/src/api/chat_v3.rs`
- 聊天主链路的前端消费已开始切到“realtime 优先，SSE 兜底收尾”：
  - `sendMessage` 已接入 `preferRealtimeStream`
  - WebSocket 已连接时，SSE 只再处理 `error / cancelled / done / complete` 等终态事件，不再重复消费正文 delta / thinking / tools
  - 聊天 realtime bridge 已从“只监听当前会话”改成“全局监听后按 `conversation_id` 路由到对应正在流式的会话”
  - 这样可以避免：
    - 同一轮聊天被 SSE + realtime 双重写入
    - loading 因 SSE 提前收尾而过早解除
    - 用户切到别的联系人后，原会话 realtime 事件丢失
  - 相关文件：
    - `chat_app/src/lib/api/client/stream.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamReader.ts`
    - `chat_app/src/lib/realtime/useConversationChatStreamRealtime.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
- 聊天完成态又往前收了一步，减少了 WebSocket 主路径结束后的 HTTP 回源：
  - 后端 `chat.turn.completed` 事件现在会补带当前 turn 的持久化 user/assistant message 回执
  - 前端 `useChatStreamRealtimeBridge` 收到完成事件后，会优先用这份持久化消息替换当前 turn 的临时消息
  - 只有本地仍残留 temp user / temp assistant、无法完成持久化对账时，才继续调用 `loadMessages()`
  - 这一步把“完成后无条件整段 reload messages”收成了“基于真实持久化回执的条件兜底”
  - 相关文件：
    - `chat_app_server_rs/src/core/chat_stream/events.rs`
    - `chat_app_server_rs/src/core/chat_stream/mod.rs`
    - `chat_app_server_rs/src/api/chat_v2.rs`
    - `chat_app_server_rs/src/api/chat_v3.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- SSE fallback 成功态也已经收掉了外层无条件回源：
  - `sendMessage` 不再在 SSE 完成后直接 `loadMessages(currentSessionId)`
  - `runStreamingAssistantTurn` 现在会在消费 `complete` 事件时提取：
    - `persisted_user_message`
    - `persisted_assistant_message`
  - 然后和 WebSocket 主路径复用同一套本地 reconcile 逻辑，优先把临时 user / assistant 消息就地替换成真正持久化消息
  - 只有替换后仍残留 temp message 时，才继续做 `loadMessages()` 兜底
  - 这一步把聊天成功态从“WS 条件兜底 + SSE 无脑回源”统一收成了“终态 payload 对账优先，必要时才回源”
  - 相关文件：
    - `chat_app/src/lib/store/actions/sendMessage.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
    - `chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.ts`
    - `chat_app/src/lib/store/actions/sendMessage/types.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- 聊天 `cancelled` 终态也往前收了一步：
  - 后端 `chat.turn.cancelled` 现在会尽量补带当前 turn 的持久化 user / assistant message 回执
  - 前端 realtime bridge 收到取消事件后，会先尝试用这份回执替换当前 turn 的临时消息
  - 只有本地仍残留 temp user / temp assistant 时，才继续做 `loadMessages()` 兜底
  - 这一步把取消态也从“事件到达后直接整段回源”收成了“终态 payload 对账优先 + 条件兜底”
  - 相关文件：
    - `chat_app_server_rs/src/core/chat_stream/events.rs`
    - `chat_app_server_rs/src/api/chat_v2.rs`
    - `chat_app_server_rs/src/api/chat_v3.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- 聊天失败态 / 本地错误保留场景的回源判断也继续收紧了一层：
  - 新增 `shouldReloadMessagesAfterTerminalState(...)`
  - 当当前 turn 的 temp user 已经成功对账成持久化 user message，而 assistant 只是本地错误气泡时，不再为了“替换一个本来就可能不存在的持久化 assistant”去强制回源
- 聊天 realtime 断线恢复也继续收紧了一层：
  - 现在会区分当前 streaming turn 是走 `realtime` 还是 `sse`
  - 只有 `realtime` 主链的 streaming turn 才会进入断线恢复逻辑
  - 且断线后会先给 WebSocket 一个短暂重连窗口，不再一闪断就立刻 `syncSessionMessagesInBackground`
  - 如果本地 draft / 持久化映射已经足够安全收口，也不会再额外整段同步消息
  - 相关文件：
    - `chat_app/src/lib/store/types.ts`
    - `chat_app/src/lib/store/actions/sendMessage/sessionState.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`
    - `chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- 聊天失败态又往前收了一层：
  - realtime `chat.turn.failed` 进入前端错误路径时，现在会先消费后端补带的 `persisted_user_message / persisted_assistant_message`
  - SSE fallback 的 `error / cancelled` 终态也会先尝试读取同一份 persisted 回执
  - 这样失败时如果 user message 已经成功持久化，前端会先把 temp user 对账掉，再决定 assistant error 气泡是否需要额外回源
  - 这一步继续减少了“失败后为了替换 temp user 而整段 `loadMessages()`”的概率
- 聊天 turn 级恢复又往前收了一层：
  - `shouldReloadMessagesAfterTerminalState(...)` 现在不再只看“temp user / temp assistant 还在不在”
  - 当同一 turn 已经有可用的 local final assistant，而且正文 / tool / error payload 足够完整时，即使 temp user 还残留，也不会再默认触发 whole-session `loadMessages()`
  - `recoverStreamingTurnBySnapshot(...)` 现在在 runtime snapshot 已经终态、但 turn 级消息接口暂时还没返回数据时，会先尝试直接用本地 terminal assistant 收口，而不是立刻判失败再整段回源
  - 这一步继续压缩了 realtime 断流和 SSE/realtime 终态收口中的 whole-session fallback
  - 相关文件：
    - `chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.ts`
    - `chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts`
    - `chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.test.ts`
- 聊天发送入口也继续向长链接主路径收了一层：
  - realtime 连接等待窗口从 `1200ms` 提高到 `2200ms`
  - `waitForRealtimeConnectedSnapshot(...)` 现在把 `error` 也视为可短暂等待的重连态，不再一看到 `error` 就立即放弃 websocket
  - `sendMessage` 现在不是“发送前先硬分 websocket / SSE 两条路”，而是：
    - 先尽力等待 websocket 连接
    - 连接上后优先发送 `sendChatCommand`
    - 只有 `sendChatCommand` 明确失败，或者等待窗口内始终没连上，才降级到 SSE
  - 这一步把 SSE 从“常规分支”继续压成了“长链接命令失败后的降级兜底”
  - `sendChatCommand` 的错误现在也开始分型：
    - `4xx` 请求非法会直接报错，不再静默降级 SSE 重打一遍
    - `5xx` / 网络错误仍允许降级 SSE
  - 这一步继续减少了“同一条坏请求沿 websocket + SSE 双打一次”的无效流量
  - 相关文件：
    - `chat_app/src/lib/realtime/state.ts`
    - `chat_app/src/lib/api/client/stream.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`
- 四类高频列表的事件后回源也继续收紧了一层：
  - `useSessionListController` 现在对 `sessions / contacts / projects / remote_connections` 的 realtime 事件不再默认“reason 不认识就整表 `load*()`”
  - 只要 payload 里带了实体快照，就直接本地 patch
  - 只要事件里带了对象 id，就优先走单条 `refresh*ById()`，并复用已有的 `404 => 本地删除` 语义
  - 只有 create race 导致 detail 暂时查不到，或者事件本身既没有 snapshot 也没有 id，才继续回退整表刷新
  - 这一步把 delete / update 场景下很多原本会打到整表 `loadSessions/loadContacts/loadProjects/loadRemoteConnections` 的回源压成了定点收口
  - 相关文件：
    - `chat_app/src/components/sessionList/useSessionListController.ts`

## 本轮验证

- 前端：`chat_app` `npm run test -- --run src/lib/store/actions/sendMessage/persistedTurnMessages.test.ts` 通过
- 前端：`chat_app` `npm run build` 通过
- 前端：`chat_app` `npm run build` 再次通过（包含四类列表 realtime fallback 收紧）
- 前端：`chat_app` `npm run build` 再次通过（包含发送入口 websocket 优先策略收紧）
- 聊天断流 / 终态兜底又收紧了一层，开始按 turn 粒度恢复：
  - 后端新增 `GET /api/conversations/:conversation_id/turns/by-turn/:turn_id/messages`
  - 该接口返回当前 turn 的完整展示切片：user + process/tool + final assistant
  - 前端 `useChatStreamRealtimeBridge` 的 realtime 断线恢复不再优先整段 `syncSessionMessagesInBackground(sessionId)`
  - 现在会先：
    - 读取 `turn runtime snapshot`
    - 判断当前 turn 是否仍在 `running` / 已进入终态
    - 然后只拉当前 turn 的消息切片并 merge 回本地
  - `cancelled / completed / failed` 三类终态在本地仍需兜底时，也会优先走这套 turn 级恢复，而不是直接整段 `loadMessages()`
  - 只有 turn snapshot 缺失、或 turn 级恢复拿不到足够数据时，才回退到旧的 whole-session HTTP 同步
  - 相关文件：
    - `chat_app_server_rs/src/api/sessions/history_process.rs`
    - `chat_app_server_rs/src/api/sessions/message_handlers.rs`
    - `chat_app_server_rs/src/api/sessions.rs`
    - `chat_app/src/lib/api/client/workspace/sessions.ts`
    - `chat_app/src/lib/api/client/facades/workspace/sessionsFacade.ts`
    - `chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- SSE fallback 的结束态也接入了同一套 turn 级恢复：
  - `streamExecution.ts` 在 SSE 流结束后，如果本地仍需兜底，不再直接整段 `loadMessages(currentSessionId)`
  - 现在会先复用 `turnRecovery`：
    - 读取当前 turn 的 runtime snapshot
    - 只拉当前 turn 的完整消息切片
    - merge 回本地后再决定是否需要 whole-session fallback
  - 这样 SSE fallback 主链也从“结束即整段回源”进一步收成了“turn 级恢复优先，整段回源最后兜底”
  - 相关文件：
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`
    - `chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts`
- `turnRecovery` 本身又补了一层，减少 snapshot 失败时的 whole-session fallback：
  - 后端新增 `GET /api/conversations/:conversation_id/turns/:user_message_id/messages`
  - 前端 `turnRecovery` 现在不再是“必须先拿到 by-turn runtime snapshot 才能恢复”
  - 当前恢复顺序变成：
    - 先尝试 `turn runtime snapshot by-turn`
    - 再尝试 `turn messages by-turn`
    - 如果当前 turn 仍拿不到，再按 `preferredUserMessageId` 走 `turn messages by user_message_id`
  - 这样即使 snapshot 接口报错、snapshot 缺失、或者 turn_id 对不齐，只要当前 user message 还在，本地仍有机会按这一轮定点恢复，而不是直接 whole-session fallback
  - 相关文件：
    - `chat_app_server_rs/src/api/sessions/message_handlers.rs`
    - `chat_app_server_rs/src/api/sessions.rs`
    - `chat_app/src/lib/api/client/workspace/sessions.ts`
    - `chat_app/src/lib/api/client/facades/workspace/sessionsFacade.ts`
    - `chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
- `turnRecovery` 又继续补了一层 merge 收口边界：
  - 如果 `by-turn snapshot` 缺失，现在还会再尝试 `latest runtime snapshot`，只要 latest 仍指向当前 turn，就继续走这轮恢复
  - 如果 turn 切片里没有 final assistant，但已经拿到了当前轮的 persisted user message，本地临时 assistant 也会被正确挂到新的 persisted user 上，而不是继续因为 user/assistant 映射不稳掉回 whole-session fallback
  - 这一步主要处理两类高频边界：
    - snapshot 接口短暂查不到当前 turn，但 latest 仍是这轮
    - 当前轮只持久化了 user，assistant 仍以本地 terminal 气泡收口
  - 相关文件：
    - `chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts`

## 本轮验证补充

- 前端：`chat_app` `npm run build` 通过
- 后端：`chat_app_server_rs` `cargo check` 通过
  - 相关文件：
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
- 终端列表的 realtime patch 也已经补上第一轮：
  - 后端 `terminal.list.invalidated` 现在可附带 `terminal` 实体快照
  - 前端会优先 `removeTerminalLocally / applyRealtimeTerminalSnapshot / refreshTerminalById`
  - 只有 created 且 detail 也拿不到时，才整表 `loadTerminals()`
- `memory_server` 管理后台的 job runs 页面也已去掉固定 `10s` 轮询：
  - 后端新增 `/api/memory/v1/jobs/runs/stream` SSE
  - 首包会推当前过滤条件下的 snapshot，后续推 `upsert` 增量事件
  - 前端 `JobRunsPage` 已改成 SSE 优先，保留手动刷新与 `resync/error` 时的 HTTP 兜底
  - 相关文件：
    - `memory_server/backend/src/api/jobs_api.rs`
    - `memory_server/backend/src/services/realtime.rs`
    - `memory_server/backend/src/repositories/jobs.rs`
    - `memory_server/frontend/src/lib/jobRunsStream.ts`
    - `memory_server/frontend/src/pages/JobRunsPage.tsx`
  - 这样 SSE fallback 和 WebSocket realtime bridge 在一部分失败态下也会优先本地收口，只在 temp user 仍残留、或本地终态 assistant 无法稳定归属到持久化 user 时才回源
  - 这一步继续减少了聊天主链终态里的 `loadMessages()` 触发频率
  - 相关文件：
    - `chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- 四类高频列表的 detail fallback 也继续收紧了一层：
  - `refreshSessionById / refreshProjectById / refreshRemoteConnectionById` 现在补上了和 `contact` 一样的 `404 => 本地删除` 语义
  - `useSessionListController` 里 realtime 事件后的 fallback 也从“detail 刷不到就整表 load*()`”收成了：
    - created 场景刷不到 detail 时，才回退整表刷新
    - updated / deleted 场景优先靠本地 patch 或 `404 => 本地删除` 收口
  - 这一步减少了 `sessions / contacts / projects / remote_connections` 在 payload 缺失、对象已删或 detail 接口返回 404 时的整表回源频率
  - 相关文件：
    - `chat_app/src/lib/store/actions/sessions/loadSessions.ts`
    - `chat_app/src/lib/store/actions/projects.ts`
    - `chat_app/src/lib/store/actions/remoteConnections.ts`
    - `chat_app/src/components/sessionList/useSessionListController.ts`
- `review-repair` 前端状态拉取又收紧了一层，减少重复请求：
  - `useReviewRepairRealtime` 现在增加了：
    - 同会话状态请求 inflight 去重
    - 1 秒短 TTL 缓存
    - 旧会话异步结果防串写保护
  - 这样可以减少：
    - 聊天主视图和团队成员视图同时挂载时对同一 `GET /review-repair` 的重复请求
    - 断线兜底轮询与首次状态探测之间的重复拉取
    - 快速切换联系人时旧请求回写当前界面状态
  - 相关文件：
    - `chat_app/src/lib/realtime/useReviewRepairRealtime.ts`
- 复盘按钮“始终不可点击”的一类后端误判已补兼容修复：
  - `review-repair` scope 解析不再只依赖 session 顶层 `project_id`，现在会兜底读取 `metadata.chat_runtime.project_id`
  - memory 侧项目 scope 查询也同步兼容 `metadata.chat_runtime.project_id/projectId`
  - 未总结消息统计不再只认 `summary_status = pending`，现在兼容老消息缺失该字段、为空或为 `null` 的情况
  - 复盘后标记 summarized 也同步兼容这类老消息，避免按钮状态长期不回落
  - 同时修掉了消息聚合里默认项目 scope 前缀化 `$or` 条件的结构问题，避免 `project_id = 0` 时统计失真
  - 相关文件：
    - `chat_app_server_rs/src/api/sessions/support.rs`
    - `chat_app_server_rs/src/api/sessions/review_handlers.rs`
    - `memory_server/backend/src/repositories/session_support.rs`
    - `memory_server/backend/src/repositories/messages/aggregate_ops.rs`
    - `memory_server/backend/src/repositories/messages/read_ops.rs`
    - `memory_server/backend/src/repositories/messages/write_ops.rs`
- 项目运行态 / 预览运行态又去掉了一层“事件到达后再重拉”的冗余：
  - `project.run.state_changed` 现在前端优先直接消费 realtime payload，先 patch 本地运行态
  - 不再把每次运行态事件都转成额外的 `listTerminals()` / `getTerminal()` 查询
  - `project.run.catalog.updated` 仍然保留为 HTTP 快照刷新，因为它本质上是目录/脚本存在性失效通知
  - 这样可以减少：
    - 侧边栏项目运行态收到事件后整批重拉终端列表
    - 项目页运行面板收到事件后再次查询终端详情
    - 预览运行面板收到事件后再次查询终端详情
  - 相关文件：
    - `chat_app/src/components/sessionList/useProjectRunState.ts`
    - `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.ts`
    - `chat_app/src/components/projectExplorer/previewRunController/useProjectPreviewTerminalPolling.ts`
    - `chat_app/src/components/projectExplorer/previewRunController/previewRunControllerTypes.ts`
    - `chat_app/src/components/projectExplorer/useProjectPreviewRunController.ts`
- `review-repair` 命令入口已补齐“真正异步化”：
  - `POST /api/conversations/:conversation_id/review-repair` 现在改成 `202 Accepted`
  - 点击后会立即推送 `conversation.review_repair.started`
  - 复盘执行改为后台任务，不再同步等待 memory server 的长任务返回
  - 这样修掉了“前端先提示执行失败、后端任务其实已经 running”的假失败问题
  - 同时将 review-repair 对 memory server 的长任务调用从默认 `5s` 请求超时中摘出，改为后台长任务专用超时，避免后台链路自己再次误报失败
  - 完成态优先以实时/状态接口的真实结果收口，状态接口取不到时才回落到本地推导，减少 loading 提前解除和错误 completed/failed
  - 相关文件：
    - `chat_app_server_rs/src/api/sessions/review_handlers.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
- `conversation summaries` 已从“事件触发 HTTP reload”收成“后端推 summary 快照、前端直接 patch”：
  - 后端 `conversation.summaries.updated` 现在发送真实 summary 列表 payload，不再复用 `review_repair` 壳
  - 聊天主面板的 memory summary 状态新增 `applyRealtimeSessionMemorySummaries(...)`
  - 团队成员 summary 面板新增直接消费 `applyRealtimeSessionSummaries(...)`
  - 两处前端消费端现在都会优先把推送的 `items` 直接落本地 cache/store，仅在 payload 缺失时才回退旧的标脏 + HTTP reload
  - 这样可以减少：
    - `review_repair.completed` 之后紧跟的 summary reload
    - 团队成员 summary 面板收到事件后的 `getConversationSummaries`
    - 聊天主 summary pane 收到事件后的 `getConversationSummaries`
  - 相关文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
    - `chat_app_server_rs/src/api/sessions/review_handlers.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/lib/realtime/useConversationSummariesRealtime.ts`
    - `chat_app/src/lib/sessionSummaries/cache.ts`
    - `chat_app/src/features/sessionSummary/useSessionSummaryPanel.ts`
    - `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersContactResources.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
- `sessions.updated` 已开始从 invalidation 走向实体 patch：
  - 后端 `sessions.updated` 现在对 `session_created / session_updated` 直接附带 session 快照
  - 前端 store 新增 `applyRealtimeSessionSnapshot(...)`，统一复用会话归一化、cache upsert 和当前会话选择同步逻辑
  - 会话列表 realtime 现在优先本地 patch；只有 payload 缺失时才回退 `refreshSessionById()`
  - 这样可以减少：
    - 会话创建后的单条会话详情回拉
    - 会话更新后的单条会话详情回拉
    - `sessions.updated` 到达后列表层的多余 HTTP 刷新
  - 相关文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/api/sessions/session_handlers.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/lib/store/types.ts`
    - `chat_app/src/lib/store/actions/sessions/mutations.ts`
    - `chat_app/src/components/sessionList/useSessionListStoreState.ts`
    - `chat_app/src/components/sessionList/useSessionListController.ts`
- `contacts / projects / remote_connections` 三条列表也已开始从 invalidation 走向实体 patch：
  - 后端 `contacts.updated / projects.updated / remote_connections.updated` 现在对 create/update 事件直接附带实体快照
  - 前端 store 新增：
    - `applyRealtimeContactSnapshot(...)`
    - `applyRealtimeProjectSnapshot(...)`
    - `applyRealtimeRemoteConnectionSnapshot(...)`
  - `useSessionListController` 收到对应 realtime 事件时，现在优先本地 patch；只有 payload 缺失时才回退：
    - `refreshContactById()`
    - `refreshProjectById()`
    - `refreshRemoteConnectionById()`
  - 这样可以减少：
    - 联系人创建/更新后的单条详情回拉
    - 项目创建/更新后的单条详情回拉
    - 远端连接创建/更新后的单条详情回拉
  - 相关文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/api/contacts.rs`
    - `chat_app_server_rs/src/api/projects/crud_handlers.rs`
    - `chat_app_server_rs/src/api/remote_connections/handlers.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/lib/store/types.ts`
    - `chat_app/src/lib/store/actions/contacts.ts`
    - `chat_app/src/lib/store/actions/projects.ts`
    - `chat_app/src/lib/store/actions/remoteConnections.ts`
    - `chat_app/src/components/sessionList/useSessionListStoreState.ts`
    - `chat_app/src/components/sessionList/useSessionListController.ts`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
    - `chat_app_server_rs/src/services/memory_server_client/http.rs`
    - `chat_app_server_rs/src/services/memory_server_client/session_ops.rs`
- 已定位并修复一类更深层的 `review-repair` 卡死问题：
  - 现象：
    - 任务面板里 `manual_review_repair` 长时间保持 `running`
    - 同一个 session 持续被普通 summary worker 打印 `skip session lock busy`
    - 前端 `review-repair` 状态会被僵尸 `running job` 一直拖住
  - 根因：
    - `memory_server` 的 summary/review-repair 执行会占用 session 级 job lock
    - 当 AI 调用走本地 `127.0.0.1:8089` 网关时，之前代码会关闭请求超时
    - 一旦本地网关流式调用卡住，任务就可能无限等待，既不 finish job_run，也不释放 lock
    - `review_repair_status` 又只看 `job_runs.status=running`，不会自动清理这类超时僵尸任务
  - 当前修复：
    - 本地 `8089` 网关不再无限等待，恢复为“较长但有限”的 AI 请求超时
    - summary/review-repair 单 session 执行新增显式 job timeout，超时后直接失败收尾
    - `review_repair_status` 查询前会清理当前 scope 下超时过久的僵尸 `running job`
    - `review_repair_status` 查询前还会顺手清理当前 scope 下已过期的 `summary_l0:*` session lock，避免旧卡死任务继续把会话锁住
  - 直接收益：
    - 复盘任务不会再因为本地网关流式卡住而无限 `running`
    - 同一会话不会被 stale lock 长时间拖住，普通 summary worker 也不会半小时一直 `lock busy`
    - 前端 `running` 状态更容易正确回落，不会长期被陈旧 job_run 卡住
  - 相关文件：
    - `memory_server/backend/src/ai/mod.rs`
    - `memory_server/backend/src/jobs/job_support.rs`
    - `memory_server/backend/src/jobs/summary.rs`
    - `memory_server/backend/src/jobs/summary_generation.rs`
    - `memory_server/backend/src/repositories/jobs.rs`
    - `memory_server/backend/src/repositories/locks.rs`
- 聊天发送入口又往统一 realtime 主路径收了一层：
  - `sendMessage` 不再依赖调用方透传 `preferRealtimeStream`
  - 现在统一由发送层内部读取全局 realtime 连接状态，决定：
    - `connected` 时走 `sendChatCommand`
    - 非 `connected` 时走旧 `streamChat` fallback
  - 主聊天、团队成员会话、runner 脚本生成三个入口已删掉手工 `preferRealtimeStream` 透传
  - `SendMessageRuntimeOptions` 已去掉 `preferRealtimeStream`
  - 这样可以避免外层组件各自判断连接态，进一步减少聊天主链的分叉
  - 相关文件：
    - `chat_app/src/lib/realtime/state.ts`
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
    - `chat_app/src/lib/store/actions/sendMessage.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberConversation.ts`
    - `chat_app/src/components/projectExplorer/useProjectRunnerScriptGenerator.ts`
    - `chat_app/src/types/runtime.ts`
- 已定位并修复“聊天发送仍持续回退 SSE”的一处关键前端问题：
  - 后端 `/api/realtime/ws` 路由与鉴权链路本身是通的，日志里已确认存在 `101 Switching Protocols`
  - 真正的根因在前端 `RealtimeClient` 生命周期：
    - `RealtimeProvider` 在开发态会经历 `React.StrictMode` 的 effect cleanup
    - 之前 cleanup 会调用 `client.destroy()`，把 client 永久标记成 `destroyed`
    - 之后同一个 provider 生命周期里即使重新设置 token / topic，client 也不会再执行 `connect()`
    - 结果就是页面表面还在，但全局 realtime 连接态回不到 `connected`，`sendMessage` 只能继续 fallback 到 `/api/agent_v3/chat/stream`
  - 当前修复：
    - 去掉 `RealtimeClient` 的永久锁死语义
    - `destroy()` 现在只负责关闭连接、清理监听器和重连定时器，不再阻止后续重连
  - 相关文件：
    - `chat_app/src/lib/realtime/client.ts`
- 已定位并修复一处导致前端 console 疯狂报错、WS 抖动的 realtime 订阅循环问题：
  - 现象：
    - 前端持续出现 `Maximum update depth exceeded`
    - 控制台反复出现 `WebSocket is closed before the connection is established`
  - 根因：
    - 订阅类 hook 原先直接消费包含 `connectionState/debugSnapshot` 的整块 realtime context
    - `debugSnapshot` 每次订阅/同步 topic 时都会变化，导致 provider 频繁 rerender
    - rerender 后，多个 `useRealtimeTopic/useRealtimeTopics` 因依赖 topic 对象/数组引用变化而重复退订/重订
    - 进而形成“订阅 -> debug state 更新 -> rerender -> 再订阅”的循环
  - 当前修复：
    - 将 `RealtimeProvider` 拆成稳定的 `client context` 与独立的 `state context`
    - `useRealtimeEvent/useRealtimeTopic/useRealtimeTopics` 只依赖稳定 `client`
    - `useRealtimeTopic/useRealtimeTopics` 改为按稳定 key/signature 驱动 effect，避免仅因对象引用变化而重复订阅
  - 相关文件：
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
- 已定位并修复“团队成员页复盘按钮显示正常但无法点击”的一处前端交互问题：
  - 现象：
    - 复盘按钮不是灰态，但在团队成员工作区里会出现“看得见、点不动”
    - 体感上更像被透明层挡住，而不是业务状态真的禁用
  - 根因：
    - 输入区附件拖拽浮层 `InputAreaDragOverlay` 使用 `absolute inset-0`
    - 外层没有把浮层作用域严格限制在输入区内部
    - 同时拖拽态在部分中断路径下缺少兜底复位，导致 `isDragging` 偶发残留
    - 两者叠加后，会出现浮层继续存在并吞掉上方 Workbar 点击的情况
  - 当前修复：
    - 给输入区 composer 外层增加 `relative`，把拖拽浮层作用域收紧在输入区内部
    - 给拖拽浮层增加 `pointer-events-none`，避免覆盖层拦截按钮点击
    - 给附件拖拽状态增加统一 `clearDraggingState()`，并在 `drop / dragend / window blur / visibilitychange / disabled` 等路径统一复位
    - 组件内 `dragleave` 仅在真正离开 composer 时才清理，减少抖动
  - 相关文件：
    - `chat_app/src/components/InputArea.tsx`
    - `chat_app/src/components/inputArea/InputAreaDragOverlay.tsx`
    - `chat_app/src/components/inputArea/useAttachmentsInput.ts`
- Git 面板这条剩余高频链又往前收了一层：
  - `useProjectGit` 现在补了跨实例共享的 client cache：
    - `git client info` cache / inflight 去重
    - `git summary` cache / stale / inflight 去重
    - `git details(branches + status)` cache / stale / inflight 去重
  - Git 面板初始化时会先 hydrate 共享快照：
    - 优先回填已有 summary / details
    - 非 stale 时不再因为组件重挂载重复请求
  - `getGitClientInfo()` 不再在项目页 Git hook 挂载时立刻请求：
    - 现在改成真正打开 Git 面板后再按需加载
    - 同时复用共享 inflight，避免短时间重复打开面板时并发多打
  - Git action 如果后端直接返回最新 summary，也会同步回写共享 summary cache，避免当前实例和共享快照脱节
  - 这样可以继续减少：
    - 项目页重渲染/重挂载时的重复 `/git/summary`
    - 打开 Git 面板前就预热 `/git/client-info`
    - 短时间多次打开 Git 面板或多处复用时的重复 `/git/branches`、`/git/status`
  - 相关文件：
    - `chat_app/src/components/projectExplorer/git/cache.ts`
    - `chat_app/src/components/projectExplorer/git/useProjectGit.ts`
    - `chat_app/src/components/projectExplorer/git/useProjectGitLifecycle.ts`
- `workbar / ui prompt history` 这两条会话内高频快照链也开始共享缓存了：
  - `useWorkbarState` 现在补了会话级共享 cache / inflight 去重：
    - current turn tasks 按 `session + turn` 维度共享
    - history tasks 按 `session` 维度共享
    - 继续保留现有的 stale 标记与 UI 层 request 序号保护
  - 这样主聊天区与团队成员工作区如果命中同一会话：
    - 不会再各自重复拉一遍当前 turn 任务
    - 不会再各自重复拉一遍历史任务列表
  - `useUiPromptHistory` 现在也补了会话级共享 cache / inflight 去重：
    - 同一会话的 UI Prompt 历史会跨入口复用
    - 面板切换时会优先 hydrate 已有快照，再按 stale 状态决定是否回源
  - 这一步仍然坚持原有策略：
    - 交互层不做激进改动
    - 只收口底下的 HTTP snapshot 层
    - realtime 事件仍然负责标 stale 或触发按需刷新
  - 这样可以继续减少：
 - 终端列表这条剩余高频链已补成共享快照层，开始从根上减少 `GET /api/terminals`：
  - 新增 `chat_app/src/lib/store/actions/terminalsCache.ts`
  - 终端列表现在具备：
    - per-user list cache / inflight 去重
    - per-terminal detail cache / inflight 去重
    - `markTerminalsStale / refreshTerminalById / removeTerminalLocally`
  - `sessionList` 收到 `terminal.list.invalidated` 后不再只能全量重拉：
    - `deleted` 直接本地剔除
    - 有 `terminal_id` 的非创建事件优先局部 refresh
    - 只有兜底场景才全量 `loadTerminals()`
  - 手动刷新终端列表现在显式 `force: true`，不会被本地 cache 吃掉
  - 项目页运行态的 `handleListTerminals()` 也改成复用同一份终端共享快照，而不是直接再打一遍 `client.listTerminals()`
  - 后端删除终端后会额外补发一次 `terminal.list.invalidated(reason=deleted)`，减少跨页面旧终端残留
  - `pending ui prompt` 这条未闭环的快照链本轮也补齐到了“会话级共享”：
    - pending prompt 的首包加载职责已从主聊天页 `useChatSessionEffects` 下沉到 `useSessionWorkbarPanels`
    - 主聊天与团队成员工作区现在会复用同一份 pending prompt cache / inflight，不再各自重复拉取
    - realtime `prompt_required / prompt_resolved` 已同步维护共享 pending cache，而不只是改本地 panel store
    - 本地 `ui prompt submit / cancel` 成功后也会同步清理 pending cache，避免接口成功但待处理提示残留
    - 全局 conversation panel realtime 入口也已同步回写 pending cache，跨会话切换时不会丢这份快照
    - 这样可以减少：
      - 主聊天与团队成员对同一会话重复请求 `/ui-prompts/pending`
      - `prompt resolved` 后 panel 消失了但共享快照还挂着，导致切回来又“复活”
      - 当前入口有数据、团队成员入口没有数据的分叉状态
  - 当前这条链仍有一个后续可优化点：
    - 团队成员侧边栏里“未选中的成员 session”如果需要首屏直接显示已有 pending count，后续可以再加一层静默预热
    - 这一步要继续坚持 cache/inflight 去重，避免重新引入批量请求噪音
  - 上面这层静默预热本轮也已经补上：
    - 团队成员侧边栏现在会按成员 session 复用共享 pending prompt cache / inflight 做一次轻量对齐
    - 因此即使某个成员当前并未被点开，只要该 session 已存在 pending ui prompt，侧边栏的待处理计数也更容易首屏直接显示
    - 这一步仍然没有回到轮询，而是继续复用：
      - 会话级共享 cache
      - inflight 去重
      - 单次快照对齐
    - 主聊天区和团队成员工作区对同一 session 的重复 `/task-manager/tasks`
    - 同一 session 的重复 `/ui-prompt/history`
    - 会话来回切换时明明已有快照却还要各入口重新回源
  - 相关文件：
    - `chat_app/src/components/chatInterface/workbarCache.ts`
    - `chat_app/src/components/chatInterface/uiPromptHistoryCache.ts`
    - `chat_app/src/components/chatInterface/useWorkbarState.ts`
    - `chat_app/src/components/chatInterface/useUiPromptHistory.ts`
- `workbar / ui-prompt` 又收紧了一轮“无意义回源”：
  - 聊天会话切换时不再默认拉取整段 `workbar history`，默认只加载当前 turn 任务
  - `conversation.task_board.updated` 到达后，默认只刷新当前 turn 任务
  - 只有任务历史抽屉真的打开时，才会补拉 `workbar history`
  - 未打开历史抽屉时，前端只把该会话 history 标记为 stale，等用户真正打开时再补最新快照
  - `conversation.ui_prompt.updated` 也是同样策略：
    - UI Prompt 历史抽屉打开时才强制刷新 history
    - 抽屉关闭时仅标 stale，不再每次实时事件都回源
  - `TaskWorkbar` 已兼容受控/非受控两种历史抽屉状态，聊天主视图可以精准利用“是否可见”来控制回源，团队成员工作区不会被这次改坏
  - 会话切换或离开聊天面板时，会自动关闭任务历史抽屉，避免旧会话打开状态残留
  - 团队成员工作区这条复用链也已接成相同策略：
    - 打开成员 Workbar 历史抽屉时才拉 history
    - 实时事件默认只刷当前 turn，历史未打开时只标 stale
  - 相关文件：
    - `chat_app/src/components/chatInterface/useWorkbarState.ts`
    - `chat_app/src/components/chatInterface/useUiPromptHistory.ts`
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`
    - `chat_app/src/components/chatInterface/useChatSessionEffects.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
    - `chat_app/src/components/chatInterface/ChatConversationPane.tsx`
    - `chat_app/src/components/chatInterface/ChatComposerPanel.tsx`
    - `chat_app/src/components/TaskWorkbar.tsx`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberWorkspaceProps.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspace.tsx`
    - `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspaceComposer.tsx`
    - `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspaceTypes.ts`
- `summary / memory` 这条链也开始收口到“realtime 失效通知 + 可见时刷新”：
  - 新增会话总结 realtime 消费 hook：
    - `chat_app/src/lib/realtime/useConversationSummariesRealtime.ts`
- 聊天主链这轮又继续往“默认长链接主路径”收了一层：
  - `sendMessage` 在决定是否回退 SSE 前，新增了一个短暂的 realtime 建连等待窗口
  - 这样可以减少：
    - websocket 已经在 `connecting`，但发送瞬间尚未切到 `connected` 时的误回退
    - 页面刚恢复、provider 刚挂起后第一条消息又走回 `/chat/stream`
  - 当前策略仍然保留 SSE 兜底，但不再对“正在建连”的 realtime 过早判失败
  - 相关文件：
    - `chat_app/src/lib/realtime/state.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`
- `review-repair` 后端 bridge polling 这轮已继续收口：
  - chat backend 不再为了维持前端 loading 持续轮询 memory `review_repair_status`
  - 当前改成更直接的异步长任务链路：
    - `POST /review-repair` 立即返回 `202`
    - 同步推 `conversation.review_repair.started`
    - 后台直接 await memory 长任务
    - 成功后只在终态补查一次真实 status 收口，再推 `completed + summaries.updated`
    - 失败时直接推 `failed`
  - 这样可以减少：
    - backend 内部重复 bridge polling memory status
    - 长时间 running 任务期间的无意义状态探测噪音
  - 当前取舍：
    - 运行中的 `pending_message_count` 不再依赖持续 progress 事件更新
    - 前端 loading 主要靠 `started/completed/failed` 与状态接口兜底维持
  - 相关文件：
    - `chat_app_server_rs/src/api/sessions/review_handlers.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
- `task / review / ui prompt` 本轮又补了一层“真实强刷语义 + 非可见历史不回源”：
  - `current turn task` 现在支持显式 `force` 刷新：
    - `loadCurrentTurnWorkbarTasks(sessionId, turnId, true)` 不再直接命中旧 cache
    - 同时也不再复用旧 inflight，避免“看似强刷，实际等到的是上一轮请求”
  - `workbar history`、`ui prompt history`、`conversation summaries` 这几条共享快照链也同步修正了：
    - `force: true` 时不再复用旧 inflight
    - 这样 realtime 事件、手动刷新、提交后补刷，都会真正发起新请求
  - `review confirm/cancel` 成功后：
    - 仍会刷新当前 turn 任务
    - 但只有历史抽屉真的打开时才刷新历史任务
    - 历史未打开时只标记 history stale，不再顺手强刷
    - 同时去掉了原来额外的 `loadWorkbarSummaries(..., true)` 强刷
  - `ui prompt submit/cancel` 成功后：
    - 只有 UI Prompt 历史抽屉真的打开时才刷新 history
    - 抽屉未打开时只标 stale，不再无条件回源
  - `task complete/edit/delete` 成功后：
    - 非 realtime 模式下不再无脑 `refreshWorkbarTasks()`
    - 改成：
      - 强刷当前 turn
      - 历史抽屉打开时才拉 history
      - 否则仅标 history stale
  - `Workbar` 打开历史任务时，聊天主界面与团队成员界面都去掉了附带的 summary 强刷：
    - 现在只打开 history 自己的快照链
    - 不再因为看任务历史顺手触发 `conversation summaries` 请求
  - 相关文件：
    - `chat_app/src/components/chatInterface/useWorkbarState.ts`
    - `chat_app/src/components/chatInterface/useWorkbarMutations.ts`
    - `chat_app/src/components/chatInterface/usePanelActions.ts`
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
    - `chat_app/src/components/chatInterface/useUiPromptHistory.ts`
    - `chat_app/src/lib/sessionSummaries/cache.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`

### 5. sessions 列表失效通知补齐

本轮已完成：

- 后端新增 `sessions.updated` realtime 事件：
  - 会话创建、更新、归档时都会统一推送 `reason + session_id + project_id`
  - 主要修改文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/api/sessions/session_handlers.rs`
- 前端新增会话列表 realtime hook：
  - `chat_app/src/lib/realtime/useSessionsRealtime.ts`
- `session list` 主入口已接入 `sessions.updated`：
  - 事件到达后执行 `loadSessions({ silent: true })`
  - 保持现有 UI 不闪 loading，但能把联系人会话的增删改统一收口到同一条失效通知链路
  - 主要修改文件：
    - `chat_app/src/components/sessionList/useSessionListController.ts`
    - `chat_app/src/components/sessionList/useSessionListStoreState.ts`
- `session list bootstrap` 已补初始会话快照加载：
  - 首次挂载时会执行一次 `loadSessions({ silent: true })`
  - 避免会话列表完全依赖联系人补建/局部选择副作用
  - 主要修改文件：
    - `chat_app/src/components/sessionList/useSessionListBootstrap.ts`

这轮改造的取舍：

- 仍然坚持 `HTTP snapshot + realtime invalidation`
- 暂时没有把 `sessions.updated` 做成局部 patch，而是先采用 low-risk 的 silent refresh
- 这样能先补齐列表一致性，再决定后续是否继续细化成按 `session_id` 的局部更新

### 6. project members 事件解耦

本轮已完成：

- 后端新增 `project.members.updated` realtime 事件：
  - 项目成员添加、移除时会单独推送 `project_id + reason + contact_id`
  - 不再只借 `project.run.catalog.updated` 的 `reason` 侧带成员变化语义
  - 主要修改文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
    - `chat_app_server_rs/src/api/projects/contact_handlers.rs`
- 前端项目级 realtime hook 已补 `project.members.updated` 消费能力：
  - `chat_app/src/lib/realtime/useProjectRunRealtime.ts`
- 团队成员面板已优先消费独立成员事件：
  - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
  - 收到成员增删事件后，会继续沿用现有 `mark stale + silent reload` 模式

兼容策略：

- 旧的 `project.run.catalog.updated(reason=project_contact_added/removed)` 兼容逻辑暂时保留
- 这样做可以先平滑迁移，避免已有依赖这两个 reason 的其它面板被一次性打断
- 后续如果确认没有别的消费者依赖这层借道，再继续收口旧触发逻辑

### 7. realtime subscribe/unsubscribe 第一版

本轮已完成：

- 后端 `/api/realtime/ws` 已支持客户端控制消息：
  - `subscribe`
  - `unsubscribe`
  - `ping`
- 新增 realtime topic / subscription 模型：
  - 支持的 scope 第一版包括：
    - `contacts`
    - `projects`
    - `sessions`
    - `remote_connections`
    - `conversation:{id}`
    - `project:{id}`
    - `terminal:{id}`
    - `remote_connection:{id}`
  - 主要修改文件：
    - `chat_app_server_rs/src/api/realtime.rs`
    - `chat_app_server_rs/src/services/realtime/session_scope.rs`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
- 服务端事件分发已从“连接后全收”升级为：
  - 默认兼容模式：客户端从未发过 subscribe/unsubscribe 时，仍允许全量事件通过
  - 显式订阅模式：一旦客户端开始声明 topics，就只下发匹配 topic 的事件
- 前端 realtime client 已支持 topic 引用计数与自动同步：
  - 多个 hooks 订阅同一 topic 时会自动合并
  - 断线重连后会自动重发当前活跃 topics
  - 主要修改文件：
    - `chat_app/src/lib/realtime/client.ts`
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
    - `chat_app/src/lib/realtime/types.ts`
- 已接入 topic 声明的前端 hooks：
  - 全局列表：
    - `useContactsRealtime`
    - `useProjectsRealtime`
    - `useSessionsRealtime`
    - `useRemoteConnectionsRealtime`
  - conversation scope：
    - `useConversationChatStreamRealtime`
    - `useConversationSummariesRealtime`
    - `useConversationTaskBoardRealtime`
    - `useConversationUiPromptRealtime`
    - `useReviewRepairRealtime`
  - project / terminal / remote connection scope：
    - `useProjectChangeSummaryRealtime`
    - `useProjectRunRealtime`
    - `useTerminalListRealtime`
    - `useTerminalStateRealtime`
    - `useRemoteSftpTransferRealtime`

这一轮的边界：

- 先做“兼容式协议升级”，还没有把所有 realtime 消费点都强制改成显式 topic
- 当前还保留“未订阅客户端默认全收”的兼容分支，避免一次性切换把现网行为打断
- 下一步可以继续补剩余消费者，然后再考虑移除默认全收兼容

本轮继续补齐：

- 新增 `useRealtimeTopics()`，支持动态批量 topic 注册：
  - 适合“多会话 / 多项目”这类不能只靠单个 `useRealtimeTopic()` 覆盖的入口
  - 主要修改文件：
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
- 聊天主界面的全局 panel realtime 已改成按已知 sessions 批量声明 conversation topics：
  - 不再只能依赖默认全收兼容
  - 主要修改文件：
    - `chat_app/src/components/chatInterface/useGlobalConversationPanelsRealtime.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceStoreBridge.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
- 侧边栏多项目运行态已改成按当前 projects 批量声明 project topics：
  - `project.run.state_changed / project.run.catalog.updated` 不再需要走全量广播兜底
  - 主要修改文件：
    - `chat_app/src/components/sessionList/useProjectRunState.ts`

当前判断：

- 前端主要 realtime 消费点已经基本都挂上了 topic 声明
- 接下来就可以开始评估“未订阅默认全收”兼容分支的下线条件

### 8. 默认全收兼容下线

本轮已完成：

- 服务端 `RealtimeSubscriptionSet` 已切到真正的“显式订阅制”：
  - 当前 socket 如果没有任何 active topics，将不会收到业务事件
  - 不再保留“客户端从未订阅过就默认全收”的兼容行为
  - 主要修改文件：
    - `chat_app_server_rs/src/services/realtime/session_scope.rs`

这一步成立的原因：

- 前端现有主要 realtime 消费入口都已经接入了 `useRealtimeTopic()` 或 `useRealtimeTopics()`
- 包括：
  - 全局列表
  - conversation scope
  - project scope
  - terminal scope
  - remote connection scope

当前意义：

- realtime 通道现在已经从“带过滤能力的全量广播”进入“真正按订阅下发”的阶段
- 后续继续扩展事件面时，不需要再担心所有连接都被动吃到无关事件

### 9. realtime 订阅可观测性补强

本轮已完成：

- 前端 `RealtimeClient` 新增 debug snapshot 能力：
  - 当前连接状态
  - active topics
  - 最近一次 `ack`
  - 最近一次 `error`
  - 最近一次 `pong.ts`
  - 最近一次控制消息发送时间
  - 主要修改文件：
    - `chat_app/src/lib/realtime/client.ts`
    - `chat_app/src/lib/realtime/types.ts`
- `RealtimeProvider` 已暴露 `useRealtimeDebugSnapshot()`：
  - 后续如果要做开发态调试面板、状态角标或故障排查 UI，可以直接消费这份状态
  - 主要修改文件：
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
- 控制消息与 ack/error 现在会通过 `debugLog` 记录：
  - `subscribe`
  - `unsubscribe`
  - ack
  - error

这一轮的价值：

- 后续如果出现“为什么这个页面没收到 realtime 事件”，可以直接先看 active topics 和最近 ack/error
- 排查成本会比之前只看网络请求和肉眼猜测低很多

### 10. fallback 轮询继续收紧

本轮已完成：

- `review-repair` 断线兜底轮询进一步收紧：
  - 轮询间隔从 `1200ms` 放宽到 `1500ms`
  - 页面不可见时不再继续高频轮询，只在恢复可见后再续跑
  - 主要修改文件：
    - `chat_app/src/lib/realtime/useReviewRepairRealtime.ts`
- `remoteSftp` 断线兜底轮询从 `setInterval(350ms)` 改成串行 `setTimeout`：
  - 避免慢请求时多次重叠查询状态
  - 页面不可见时暂停轮询，恢复可见后由后续状态驱动重新继续
  - 同时把兜底轮询节奏放宽到 `500ms`
  - 主要修改文件：
    - `chat_app/src/components/remoteSftp/useRemoteSftpTransfer.ts`

这一轮的取舍：

- 还没有完全删除 fallback 轮询
- 但已经把“断线兜底”从积极自旋收紧成更克制的后台行为
- 这样可以先降低重复请求和隐藏页噪音，再继续评估是否能完全去掉

本轮继续补了一层：

- `review-repair` 初始状态探测进一步去重：
  - 如果本地已有短 TTL cache，优先直接用缓存，不再立刻重复请求
  - 如果当前 websocket 已连接且该会话状态已经被 realtime hydrate 过，则跳过额外的首次状态探测
  - 主要修改文件：
    - `chat_app/src/lib/realtime/useReviewRepairRealtime.ts`

这一步的意义：

- 减少聊天主视图/团队成员视图切换时对 `GET /review-repair` 的不必要首包探测
- 让 `review-repair` 更接近“realtime 为主，HTTP 只做真正兜底”的目标

本轮继续收了一步：

- `review-repair` 已移除“断线后持续轮询直到完成”的后台自旋逻辑
- 当前改成更弱的恢复策略：
  - realtime 正常时完全靠事件驱动
  - 断线状态下，如果页面重新变为可见，才补一次 `refreshReviewRepairStatus`
  - 不再在后台持续追着同一个 running job 高频轮询
  - 主要修改文件：
    - `chat_app/src/lib/realtime/useReviewRepairRealtime.ts`

当前效果：

- `review-repair` 这条链已经非常接近“realtime 主导 + 少量恢复探测”
- HTTP 状态接口更多只承担：
  - 首次轻量状态探测
  - 用户回到页面时的一次恢复校准

本轮继续推进 `remoteSftp`：

- `remoteSftp` 断线兜底已从“持续轮询”收紧成“按需一次性恢复探测”模型：
  - 传输开始后若 websocket 未连接，不再持续自旋追状态
  - 改成在这些时机安排一次 `getRemoteSftpTransferStatus`：
    - 传输启动后
    - 页面重新可见
    - 窗口重新 focus
    - 网络恢复 `online`
    - 发起取消后（断线场景）
  - 正常 websocket 已连接时，仍然完全依赖 `remote.sftp.transfer.updated`
  - 主要修改文件：
    - `chat_app/src/components/remoteSftp/useRemoteSftpTransfer.ts`

这一轮的意义：

- `remoteSftp` 这条链也开始从“断线时持续查状态”转向“断线时少量恢复校准”
- 到这一步，主要 fallback 已经都从持续轮询收紧成了更弱、更克制的恢复型探测
  - 后端已有的 `conversation.summaries.updated` 现在前端会真正消费：
    - 主聊天区收到事件后：
      - 先把 memory summary 标记为 stale
      - 只有记忆视图当前可见时，才会 silent refresh
      - 视图关闭时不再无脑回源
    - 团队成员工作区收到事件后：
      - 先把 session summary 列表标记为 stale
      - 只有 summary 面板真的打开时，才会 silent refresh
      - 面板关闭时不再因为 realtime 事件重复请求
  - `useSessionSummaryPanel` 已补齐：
    - session 级 cache
    - stale 标记
    - pending load cancel
    - 打开 summary 时优先回填缓存，再按需刷新
  - `useContactMemoryContext` 已补齐：
    - session 级 cache
    - stale 标记
    - 打开 summary 时优先回填缓存，再按需刷新
    - 修掉了“已加载同 key 时直接短路，导致 stale 后仍不刷新”的问题
  - 团队成员 summary 打开/切换链路也收紧了一层：
    - 去掉了 `sessionSummaryPaneVisible` 后额外的 silent refresh effect
    - 切换联系人时会取消旧的 summary pending 请求，避免旧回包串到新联系人
    - 手动刷新按钮现在显式走 `force: true`
  - 这样可以减少：
    - `review_repair.completed` 与 `conversation.summaries.updated` 相关的重复 summary/memory 回源
    - 打开 summary 面板后每次重渲染都 silent refresh
    - 团队成员快速切联系人时旧 summary 请求回包污染当前界面
  - 相关文件：
    - `chat_app/src/lib/realtime/useConversationSummariesRealtime.ts`
    - `chat_app/src/features/sessionSummary/useSessionSummaryPanel.ts`
    - `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`
    - `chat_app/src/components/chatInterface/useChatSessionEffects.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersContactResources.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberConversation.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberWorkspaceProps.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
- Git 面板这条剩余高频链又收了一层：
  - `useProjectGit` 现在补了：
    - summary cache / stale
    - details cache / stale
    - `force` 刷新能力
  - `project.change_summary.updated` 到达后：
    - 先标记 Git summary / details 为 stale
    - Git 面板未打开时，只刷新顶部 summary
    - 只有 Git 面板当前打开时，才会补拉 branches / status 详情
  - 原来窗口 `focus` 就直接刷新 Git summary 的行为已改成：
    - 先标 stale
    - 只有 Git 面板当前打开时，才真正请求
  - 这样可以减少：
    - 切回应用窗口时无条件触发的 `/git/summary` 请求
    - 项目有文件变化时，Git 面板关闭状态下仍回源 `/git/branches`、`/git/status`
    - 重复打开 Git 面板时对未变化详情的重复拉取
  - 相关文件：
    - `chat_app/src/components/projectExplorer/git/projectGitTypes.ts`
    - `chat_app/src/components/projectExplorer/git/useProjectGit.ts`
    - `chat_app/src/components/projectExplorer/git/useProjectGitLifecycle.ts`
    - `chat_app/src/components/projectExplorer/git/useGitBranchButtonModel.ts`
- Notepad 面板这条打开即回源的链路也先收了一轮：
  - 修掉了首开时 `refreshAll()` 与额外 `loadNotes()` effect 叠加造成的 notes 双请求
  - `useNotepadData` 现在补了：
    - 基于 `apiClient` 隔离的模块级 cache
    - folders cache / stale
    - notes 按 `searchQuery` 维度 cache / stale
    - init / folders / notes 的 inflight 去重
    - `force` 刷新能力
  - 打开 notepad 时现在会：
    - 先回填已有 folders / notes 缓存
    - 再按 stale 状态决定是否真正请求
    - 关闭再打开时，不再默认把 `/notepad/init`、`/notepad/folders`、`/notepad/notes` 全打一遍
  - 搜索词变化时：
    - 只刷新当前 query 对应的 notes
    - 不再和首开刷新叠成重复请求
  - CRUD 后这一轮仍保留“HTTP 快照强刷”策略，但已统一走 `mark stale + force refresh`：
    - 先保证行为正确，不在这一步冒进改成局部 patch
  - 这样可以减少：
    - 打开 notepad 面板时的重复 `listNotepadNotes`
    - 关闭后再次打开时对未变化 notes/folders 的重复回源
    - 搜索切换与初始化并发时的相同请求重入
  - 相关文件：
    - `chat_app/src/components/notepad/useNotepadData.ts`
    - `chat_app/src/components/notepad/useNotepadPanelEffects.ts`
    - `chat_app/src/components/notepad/useNotepadPanelController.ts`
    - `chat_app/src/components/notepad/useNotepadCrudActions.ts`
- Notepad CRUD 又往前收了一层，开始从“强刷快照”转向“本地 patch 优先”：
  - `useNotepadData` 新增了可写缓存能力：
    - `upsertCachedNote`
    - `removeCachedNote`
    - `applyFolderToCache`
    - `removeFolderFromCache`
  - 当前已切到本地 patch 的场景：
    - 新建 note 后不再额外全量 `listNotepadNotes`
    - 保存 note 后优先用接口返回的 note 回写缓存，不再默认全量 `listNotepadNotes`
    - 删除 note 后不再额外全量 `listNotepadNotes`
    - 新建 folder / 删除 folder 后优先 patch 本地 folders 与已缓存 notes
  - 当前仍保留的保守策略：
    - `saveNote` 如果后端没回 note，仍会回退到 `mark stale + force loadNotes`
    - folder 相关这轮只是安全 patch，本质还不是 realtime 驱动
  - 这样可以进一步减少：
    - note CRUD 后的重复 `/notepad/notes`
    - folder CRUD 后为了更新目录树触发的整表 notes 回源
    - 纯本地可推导状态却仍走 HTTP 快照的冗余请求
  - 相关文件：
    - `chat_app/src/components/notepad/useNotepadData.ts`
    - `chat_app/src/components/notepad/useNotepadPanelController.ts`
    - `chat_app/src/components/notepad/useNotepadCrudActions.ts`
- Notepad 详情链路也开始收口到“详情缓存优先”：
  - `useNotepadData` 现在补了 note detail cache / inflight 去重
  - `openNote` 已改成优先消费 `loadNoteDetail`，不再每次点击都直接打 `getNotepadNote`
  - `copy text / copy as md` 已改成：
    - 当前正在编辑的笔记直接复用本地 editor 内容
    - 已缓存详情的笔记直接复用 detail cache
    - 只有真正未命中缓存时才回退到 `getNotepadNote`
  - `saveNote` 现在除了更新列表 cache，也会同步回写当前 note 的 detail cache
  - 这样可以进一步减少：
    - 重复打开同一笔记时的 `/notepad/notes/:id`
    - 右键复制/导出非当前笔记时的重复详情请求
    - 刚保存完当前笔记后再次打开/导出又去拉详情的冗余请求
  - 相关文件：
    - `chat_app/src/components/notepad/useNotepadData.ts`
    - `chat_app/src/components/notepad/useNotepadOpenNote.ts`
    - `chat_app/src/components/notepad/useNotepadExportActions.ts`
    - `chat_app/src/components/notepad/useNotepadPanelController.ts`
    - `chat_app/src/components/notepad/useNotepadCrudActions.ts`
- Runtime context 这条链也开始收口到“共享缓存 + 首开不双拉”：
  - 新增共享 runtime context cache：
    - session 级 cache
    - stale 标记
    - inflight 去重
  - 聊天主入口 `useRuntimeContextState` 已改成：
    - 打开 drawer 时先 hydrate cache
    - 不再 `handleOpenRuntimeContext()` 先拉一次、`useEffect` 再补拉一次
    - `runtimeContextRefreshNonce` 变化时改成 `mark stale + silent refresh`
  - 团队成员 `useTeamMemberRuntimeContext` 也已接成相同策略：
    - 打开成员 runtime context 时先回填 cache
    - 去掉打开时的重复详情请求
    - 刷新 nonce 到达时只做 stale + silent refresh
  - 这样可以减少：
    - 打开 runtime context drawer 时的重复 `/conversation/latest-turn-runtime-context`
    - 主聊天区与团队成员对同一 session runtime context 的重复回源
    - nonce 驱动刷新时无意义的 loading 抖动
  - 相关文件：
    - `chat_app/src/lib/runtimeContext/cache.ts`
    - `chat_app/src/components/chatInterface/useRuntimeContextState.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberRuntimeContext.ts`
- 远程连接列表/详情这条链也先收了一轮，优先解决多入口重复加载和 fallback 详情重复请求：
  - `remoteConnections` 动作层现在补了：
    - list cache / inflight 去重
    - detail cache / inflight 去重
    - 手动刷新 `force` 能力
  - `selectRemoteConnection` / `openRemoteSftp` 在本地列表未命中时：
    - 现在会复用共享 detail inflight
    - 不再各自重复打 `/remote-connections/:id`
  - `create / update / delete remote connection` 后：
    - 会同步 patch list/detail cache
    - 避免刚修改完又被旧快照覆盖
  - 会话列表里的“刷新远端连接”按钮已显式走 `force: true`
  - 这样可以减少：
    - 聊天区与侧边栏多入口挂载时的重复 `/remote-connections`
    - 切换远端终端 / 打开 SFTP 时对同一连接详情的重复回源
    - 手动刷新之外对未变化远端连接列表的重复请求
  - 相关文件：
    - `chat_app/src/lib/store/actions/remoteConnections.ts`
    - `chat_app/src/lib/store/types.ts`
    - `chat_app/src/components/sessionList/useSessionListActions.ts`
- `contacts / projects` 这两条多入口首屏加载链也开始统一收口到共享缓存：
  - `contacts` 动作层现在补了：
    - user 级 list cache / inflight 去重
    - 手动刷新 `force` 能力
    - create / delete 后同步 patch cache
  - `projects` 动作层现在补了：
    - user 级 list cache / inflight 去重
    - detail cache / inflight 去重
    - 手动刷新 `force` 能力
    - create / update / delete / select 后同步 patch list/detail cache
  - 这样可以减少：
    - 聊天主入口与 session list 同时初始化时的重复 `/contacts`、`/projects`
    - 打开项目面板时本地列表未命中导致的重复 `/projects/:id`
    - 手动刷新之外对未变化联系人/项目列表的重复回源
  - 相关文件：
    - `chat_app/src/lib/store/actions/contacts.ts`
    - `chat_app/src/lib/store/actions/projects.ts`
    - `chat_app/src/lib/store/types.ts`
    - `chat_app/src/components/sessionList/useSessionListActions.ts`
- `contacts / projects / remote_connections` 这三条缓存链已经开始接上后端 realtime 失效通知：
  - 后端新增列表失效事件：
    - `contacts.updated`
    - `projects.updated`
    - `remote_connections.updated`
  - 当前已接入的发布点：
    - 联系人 `create / delete`
    - 项目 `create / update / delete`
    - 远端连接 `create / update / delete`
  - 前端新增对应 realtime hooks：
    - `chat_app/src/lib/realtime/useContactsRealtime.ts`
    - `chat_app/src/lib/realtime/useProjectsRealtime.ts`
    - `chat_app/src/lib/realtime/useRemoteConnectionsRealtime.ts`
  - `sessionList` 当前已经接入三类事件驱动刷新：
    - 收到事件后走动作层 silent refresh
    - 动作层会优先复用这轮新补的 cache / inflight 去重
    - 同一批事件连发时 hook 内还有最小 inflight 节流，避免短时间重复回源
  - 这样可以进一步减少：
    - 其他入口增删改联系人/项目/远端连接后，前端仍依赖手动刷新或重新打开面板
    - 多个入口同时收到相同失效后各自连发重复请求
    - 刚接上 realtime 后却因为没有动作层去重而把回源放大
  - 相关文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
    - `chat_app_server_rs/src/api/contacts.rs`
    - `chat_app_server_rs/src/api/projects/crud_handlers.rs`
    - `chat_app_server_rs/src/api/remote_connections/handlers.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/lib/realtime/useContactsRealtime.ts`
    - `chat_app/src/lib/realtime/useProjectsRealtime.ts`
    - `chat_app/src/lib/realtime/useRemoteConnectionsRealtime.ts`
    - `chat_app/src/components/sessionList/useSessionListController.ts`
- 团队成员项目成员刷新链也开始并入 realtime，而不再只靠本地 `window` 事件：
  - `useProjectMembersManager` 现在会消费 `project.run.catalog.updated`
  - 当后端推送的 `reason` 是：
    - `project_contact_added`
    - `project_contact_removed`
    就会触发项目成员列表 silent reload
  - 当前策略是：
    - 保留原有 `project-contact-changed` 本地事件作为同页兜底
    - 新增 realtime 消费保证跨入口/跨面板也能同步
    - 增加了最小 reload 节流，避免同一次成员增删被本地事件和 realtime 双重触发
  - 这样可以减少：
    - 团队成员面板仅在当前页面本地操作后才更新，跨入口变更不同步
    - 项目成员增删后必须手动重新打开面板才能看到变化
    - 同一次成员增删操作触发两次 reload
  - 相关文件：
    - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
    - `chat_app/src/lib/realtime/useProjectRunRealtime.ts`
- `projectRunner` 项目成员行数据也开始共享缓存，不再只是并发去重：
  - `loadProjectRunnerContactRows()` 现在补了：
    - project 级 cache
    - stale 标记
    - 原有 inflight 去重继续保留
  - 当前接入点：
    - 团队成员项目成员面板
    - 侧边栏项目运行态
  - 项目成员增删时：
    - 本地操作后会先 `markProjectRunnerContactRowsStale`
    - 收到 `project_contact_added / removed` realtime 事件后也会标 stale
    - 后续 reload 会复用共享快照，避免两个入口各自重新打一遍 `listProjectContacts`
  - 这样可以减少：
    - 项目成员面板和侧边栏项目运行态对同一项目重复回源 `/projects/:id/contacts`
    - realtime 接入后因为没有 stale/cache 协议而导致的重复 HTTP 快照
  - 相关文件：
    - `chat_app/src/lib/domain/projectRunner.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
    - `chat_app/src/components/sessionList/useProjectRunState.ts`

## 下一步候选

按优先级建议：

1. 继续评估 team members / 非聊天主入口里是否还存在 task/ui-prompt / runtime-context 相关的手动刷新冗余
2. 继续把 notepad 往“更细粒度事件/局部 patch”再推进，尤其是 folder rename 后的跨 query 精确同步与详情冲突提示
3. 视需要把 `contacts.updated / projects.updated / remote_connections.updated` 再细化成更精准的 payload，减少前端无差别 silent refresh
4. 补远程终端内部更细粒度 workspace 脏路径事件，降低 watcher 全盘扫描频率
5. 继续清理 `project.run.catalog.updated(reason=project_contact_added/removed)` 这层成员兼容逻辑
6. 再评估是否需要引入更细粒度 subscribe / unsubscribe 协议

## 本轮新增

- realtime 调试状态已真正暴露到开发态全局对象：
  - `window.__CHATOS_REALTIME_DEBUG__`
  - 当前可直接查看：
    - `snapshot`
    - `getSnapshot()`
    - `getTopics()`
    - `getConnectionState()`
  - 主要修改文件：
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`
- 这一步的价值：
  - 排查“为什么某个页面没收到 realtime”时，不再只能靠 Network 面板盲猜
  - 可以直接核对：
    - 当前连接状态
    - 活跃订阅 topics
    - 最近一次 ack / error / pong / control message

- notepad 已补上第一版 realtime 变更通知：
  - 后端新增 `notepad.updated`
  - 当前已接入的发布点：
    - folder create
    - folder rename
    - folder delete
    - note create
    - note update
    - note delete
  - 主要修改文件：
    - `chat_app_server_rs/src/api/notepad.rs`
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/realtime/mod.rs`
    - `chat_app_server_rs/src/services/realtime/session_scope.rs`

- 前端 notepad 已接成“realtime 标脏 + 面板打开时按需刷新”：
  - 新增 hook：
    - `chat_app/src/lib/realtime/useNotepadRealtime.ts`
  - 打开 notepad 面板时：
    - 仍然优先使用已有 cache / local patch
    - 收到外部变更后会先标记 folders / notes / detail stale
    - 面板关闭时只累计 `refreshNonce`，不做后台无意义回源
    - 面板重新打开时会强制做一次对齐刷新
  - 面板打开且当前笔记未脏时：
    - 如果收到当前 note 对应的 `note_updated`，会直接重新拉详情对齐编辑器内容
  - 主要修改文件：
    - `chat_app/src/components/notepad/useNotepadData.ts`
    - `chat_app/src/components/notepad/useNotepadPanelEffects.ts`
    - `chat_app/src/components/notepad/useNotepadPanelController.ts`

当前这一轮的边界：

- notepad 这次先做的是“低风险失效通知”，还没有把所有 CRUD 都彻底改成纯 realtime 增量 patch
- folder rename / delete 目前仍然优先走 stale + refresh，而不是复杂的跨 query 精确重写
- 这是刻意取舍，先保证：
  - 外部改动能同步
  - 面板关闭时不重复打请求
  - 当前编辑中的 note 不会被无脑覆盖

- `project members` 借道 `project.run.catalog.updated` 的兼容链又拆掉了一层：
  - 后端项目成员增删现在只发布：
    - `project.members.updated`
  - 不再额外混发：
    - `project.run.catalog.updated(reason=project_contact_added/removed)`
  - 团队成员面板也已移除这层 catalog fallback 消费，只保留独立成员事件与本地 `project-contact-changed` 兜底
  - 主要修改文件：
    - `chat_app_server_rs/src/api/projects/contact_handlers.rs`
    - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`

这一步的意义：

- 项目成员变更语义终于和 runner catalog 变更彻底解耦了一步
- 后续如果排查“为什么项目运行态被刷新”，不会再混入成员增删带来的噪音
- 还保留的兼容层已经只剩前端本地 `project-contact-changed` 事件，范围小很多

- 前端本地 `project-contact-changed` 事件也已经下线：
  - 团队成员面板本地 add/remove 后，不再额外广播 `window` 事件
  - 改成：
    - mutation 成功后直接 `mark stale + local reload`
    - 侧边栏项目运行态直接消费 `project.members.updated` 来重算 `no_member / script_missing / ready`
  - 主要修改文件：
    - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
    - `chat_app/src/components/sessionList/useProjectRunState.ts`

这一步的意义：

- 项目成员这条链现在只剩两套机制：
  - 本地 mutation 后的直接 reload
  - 后端 realtime 的统一失效通知
- 原来那条同页专用的 `window` 事件桥已经不需要了

- `contacts / projects / remote_connections` 这三条列表失效通知又细化了一轮：
  - 这次先不改后端 payload 协议，直接利用现有：
    - `reason`
    - `contact_id / project_id / connection_id`
  - 前端动作层新增了局部 cache 能力：
    - `markContactsStale`
    - `removeContactLocally`
    - `markProjectsStale`
    - `removeProjectLocally`
    - `refreshProjectById`
    - `markRemoteConnectionsStale`
    - `removeRemoteConnectionLocally`
    - `refreshRemoteConnectionById`
  - `sessionList` realtime 消费现在优先走：
    - delete 类事件：直接本地移除
    - create / update 类事件：优先标记对应 detail stale，再按 id 刷新单条
    - 只有 reason 不明确时，才回退整表 silent refresh
  - 主要修改文件：
    - `chat_app/src/lib/store/actions/contacts.ts`
    - `chat_app/src/lib/store/actions/projects.ts`
    - `chat_app/src/lib/store/actions/remoteConnections.ts`
    - `chat_app/src/lib/realtime/useContactsRealtime.ts`
    - `chat_app/src/lib/realtime/useProjectsRealtime.ts`
    - `chat_app/src/lib/realtime/useRemoteConnectionsRealtime.ts`
    - `chat_app/src/components/sessionList/useSessionListController.ts`
    - `chat_app/src/components/sessionList/useSessionListStoreState.ts`
    - `chat_app/src/lib/store/types.ts`

这一步的意义：

- 不再把这三类事件一律转成整表 HTTP 回源
- 对 create / update / delete 这类明确场景，前端已经能先做更细粒度 patch / single-item refresh

- `project members` 这条链又往“局部 patch + 共享 cache”收了一层：
  - `projectRunner` 项目成员共享缓存新增了局部能力：
    - `getProjectRunnerContactRowsSnapshot`
    - `upsertProjectRunnerContactRow`
    - `removeProjectRunnerContactRow`
  - 团队成员面板本地 add/remove 后现在优先：
    - 直接 patch 本地 `projectMembers`
    - 同步更新共享 project-runner contact rows cache
    - 不再自己立刻整批 reload `/projects/:id/contacts`
  - 对 `project.members.updated` realtime：
    - 继续保留 HTTP 快照兜底
    - 但本地 mutation 会短时吞掉自己刚触发的对应 realtime 回声，避免“刚本地 patch 完又被同一事件再打一遍 reload”
  - 项目页 runner catalog 现在也开始直接消费：
    - `project.members.updated(project_contact_added|project_contact_removed)`
    - 收到后只刷新成员列表，不再必须等 catalog 事件或手动 refresh
  - 主要修改文件：
    - `chat_app/src/lib/domain/projectRunner.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
    - `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts`

这一步的意义：

- 项目成员本地操作的主路径已经从“mutation 成功 -> 标 stale -> 整批 reload”收成“mutation 成功 -> 本地 patch + 共享 cache 对齐”
- 团队成员面板和项目运行页对同一项目成员数据的复用又更进一步
- `project.members.updated` 现在更像真正的跨入口兜底同步，而不是每次本地操作都必然触发的一次额外全量回源

- 团队成员入口的 pending review / pending ui-prompt 预热也收了一层：
  - `useTeamMembersRuntimeResources` 现在会先读：
    - `peekPendingTaskReviewCacheEntry`
    - `peekPendingUiPromptCacheEntry`
  - 命中非 stale 共享快照时：
    - 直接同步到面板 store
    - 不再重复回源 `getPendingTaskReviews / getPendingUiPrompts`
  - 同时把 effect 对 `taskReviewPanelsBySession / uiPromptPanelsBySession` 的直接依赖拿掉了：
    - 改成 ref 读取当前面板快照
    - 避免 panel state 自己变化又反过来触发这一轮预热 effect
  - 主要修改文件：
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`

这一步的意义：

- 团队成员面板在同一批会话间切换时，会更多复用主聊天区已经打过的 pending-panel 快照
- 减少因为团队成员 pane 挂载、panel state 同步而产生的重复 pending review / ui-prompt HTTP 请求

- `getConversationSummaries` 现在开始跨入口共享 snapshot / inflight：
  - 新增统一 summaries cache：
    - `chat_app/src/lib/sessionSummaries/cache.ts`
  - 团队成员总结面板 `useSessionSummaryPanel` 已改成复用这层：
    - 不再维护自己独立的一份 summaries HTTP cache
  - 主聊天 memory 面板 `useContactMemoryContext` 里的 summaries 读取也已接入同一层：
    - `loadSessionMemorySummaries`
    - `loadContactMemoryContext`
    现在都会复用 `loadConversationSummaryItems(...)`
  - stale 标记也对齐到了共享层：
    - 团队成员 summary pane 标 stale
    - 主聊天 memory 标 stale
    会一起命中同一份 session summaries 失效状态
  - 主要修改文件：
    - `chat_app/src/lib/sessionSummaries/cache.ts`
    - `chat_app/src/features/sessionSummary/useSessionSummaryPanel.ts`
    - `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`

这一步的意义：

- 主聊天 memory 面板和团队成员总结面板，不再对同一会话各自重复请求 `getConversationSummaries`
- 同一会话 summaries 在两个入口之间开始真正共享 HTTP snapshot 与 inflight 去重
- 后续如果后端再补更丰富 payload，这一层可以继续顺着扩，不需要再返工基础结构

- notepad 这条链又往“事件后本地 patch 优先”推进了一层：
  - 当前已经改成优先本地 patch 的 realtime 场景：
    - `folder_created`
    - `folder_deleted`
    - `folder_renamed`
    - `note_deleted`
  - 现在行为变成：
    - `folder_created`：直接把目录补进本地 folder cache
    - `folder_deleted`：直接从本地 folders / notes / detail cache 删除整棵目录树
    - `folder_renamed`：先本地改写 folders / notes / detail cache 里的 folder path，再做一次保守 refresh 对齐
    - `note_deleted`：直接从本地 notes / detail cache 删除，若当前正在看这条 note 就直接清空 editor
  - 当前仍保留“stale + refresh”兜底的场景：
    - `note_created`
    - `note_updated`
    - `folder_renamed` 的最终对齐
  - 主要修改文件：
    - `chat_app/src/components/notepad/useNotepadData.ts`
    - `chat_app/src/components/notepad/useNotepadPanelController.ts`

这一步的意义：

- notepad 不再把这些高确定性事件都转成整面板刷新
- 面板打开时，删除类和部分目录类变化已经可以直接在本地视图生效
- 后续如果继续补 `folder_renamed` / `note_updated` 的更精准 patch，这一层也已经有了基础设施

- notepad 的 refresh 粒度又往下压了一轮：
  - `note_created / note_updated` 现在不再默认 `refreshAll()`
  - 当前改成：
    - 先 `mark detail stale`
    - 再按 `note_id` 单条 `loadNoteDetail(force: true)`
    - 用 detail 回写本地 notes/detail cache
  - `folder_renamed` 也不再默认整面板 refresh：
    - 先本地改 folders / notes / detail cache 里的 folder path
    - 如果影响当前列表，再只做一次 `loadNotes({ force: true })`
    - 不再顺手把 folders + notes + init 全打一遍
  - `folder_deleted` 现在还会顺手处理当前 UI 状态：
    - 如果当前选中目录落在被删树下，就清掉 `selectedFolder`
    - 如果当前打开笔记落在被删树下，就直接清空 editor
  - 主要修改文件：
    - `chat_app/src/components/notepad/useNotepadPanelController.ts`

这一步的意义：

- notepad 的 `realtime -> HTTP` 回源粒度已经从“整面板”进一步收缩到“单条详情 / 当前列表”
- 对用户最容易频繁操作的 note create/update/delete 场景，页面抖动和无意义请求都会更少
- 这样后续继续做成员列表局部 patch 或更细粒度 payload，会简单很多

- realtime debug 快照补强了一层，方便直接排查“事件没到前端”还是“事件到了但消费没生效”：
  - `RealtimeDebugSnapshot` 新增：
    - `lastEventAt`
    - `recentEvents`
  - `recentEvents` 当前会保留最近 30 条业务事件简表，包含：
    - `event`
    - `conversation_id`
    - `project_id`
    - `payloadKind`
    - `payloadReason`
    - `payloadAction`
    - `streamType`
    - `ts`
  - `RealtimeClient` 在收到 `type=event` 时，会先把这条事件写入调试 ring buffer，再分发给业务 listener
  - 开发态下的 `window.__CHATOS_REALTIME_DEBUG__` 也补了：
    - `getRecentEvents()`
    - `snapshot.recentEvents`
  - 主要修改文件：
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/lib/realtime/client.ts`
    - `chat_app/src/lib/realtime/RealtimeProvider.tsx`

这一步的意义：

- 我们现在可以直接在浏览器控制台确认最近业务事件是否真实到达前端
- 即使 topic 订阅没问题，也能继续区分是：
  - event name 不对
  - payload kind / reason / action 不对
  - 还是前端消费侧 patch 没命中
- 这对后面继续清理“看起来 WS 连上了，但局部 UI 还是没更新”的问题会非常有帮助

- `sessions.updated` 这条链又从“整表 silent refresh”继续收紧了一轮：
  - 前端 store 新增：
    - `markSessionsStale`
    - `removeSessionLocally`
    - `refreshSessionById`
  - `sessions.updated` 现在改成按 `reason + session_id` 分流：
    - `session_deleted`：直接本地移除会话
    - `session_created / session_updated`：优先按 `session_id` 单条 `getSession()` 刷新
    - 只有 reason 不明确时，才回退整表 `loadSessions({ silent: true })`
  - `useSessionsRealtime` 也补了一个很小的 trailing-queue：
    - 正在处理一条会话事件时，不再把后续事件直接丢掉
    - 会在当前处理完成后，至少再补跑最后一条 pending payload
  - 主要修改文件：
    - `chat_app/src/lib/realtime/useSessionsRealtime.ts`
    - `chat_app/src/components/sessionList/useSessionListController.ts`
    - `chat_app/src/lib/store/actions/sessions/cache.ts`
    - `chat_app/src/lib/store/actions/sessions/loadSessions.ts`
    - `chat_app/src/lib/store/actions/sessions/mutations.ts`
    - `chat_app/src/lib/store/actions/sessions/createSession.ts`
    - `chat_app/src/lib/store/types.ts`

- `loadSessions` 内部也顺手去掉了一层重复联系人回源：
  - 过去 `loadSessions` 自己会直接再打一遍 `getContacts`
  - 现在改成复用 store 里的 `loadContacts()`，可以直接吃现有 contacts cache / inflight 去重
  - 这意味着：
    - session list bootstrap 时，`loadContacts + loadSessions` 不会再天然各自打两遍联系人请求
    - 后续会话列表失效重载时，也能复用联系人快照，而不是每次重新直连 contacts API

- 会话 store 现在也补上了第一版 list/detail cache：
  - 会话列表主快照按 user scope 缓存
  - 单条 `getSession(id)` 详情也有了 detail cache / inflight 去重
  - 本地 create / update / delete / realtime refresh 会同步更新这层 cache

这一步的意义：

- 联系人聊天这条链路里，会话增删改不再默认整表回源
- `loadSessions` 不会再顺手重复打联系人接口
- 后续如果继续细化 `sessions.updated` payload，前端这一层已经具备了单条增量同步能力

- 聊天主链路的 SSE 依赖又往下拆了一层：
  - 新增前端 `sendChatCommand()` 命令提交路径
  - 当 `preferRealtimeStream=true` 且全局 realtime 已连接时：
    - `sendMessage` 不再打开 SSE `ReadableStream`
    - 改成只发 HTTP 命令提交
    - 聊天正文、thinking、tool、完成/失败/取消 全部等待 realtime 事件收尾
  - 当 realtime 不可用时，仍然保留原有 `streamChat()` SSE/ReadableStream 兜底
  - 主要修改文件：
    - `chat_app/src/lib/api/client/stream.ts`
    - `chat_app/src/lib/api/client/types/runtime.ts`
    - `chat_app/src/lib/api/client/facades/runtimeFacade.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`

这一步的意义：

- websocket 已连接时，聊天主路径终于不再额外打开一条 SSE body 去“陪跑”
- 前端聊天流从“realtime 优先但仍依赖 SSE 通道存在”推进到了“realtime 主通道，SSE 仅断线兜底”
- 距离彻底移除旧 `ReadableStream + SSE parser` 又近了一大步

- 聊天命令提交语义也已经和旧 `stream` 彻底拆开：
  - 后端新增显式命令接口：
    - `/api/agent_v2/chat/send`
    - `/api/agent_v3/chat/send`
  - 这两个接口返回：
    - `accepted`
    - `conversation_id`
    - `turn_id`
  - 后端原有聊天执行逻辑现在支持：
    - `SSE + realtime` 双发
    - `realtime only` 执行
  - 前端 `sendChatCommand()` 已正式切到新的 `/chat/send`
  - 主要修改文件：
    - `chat_app_server_rs/src/api/chat_v2.rs`
    - `chat_app_server_rs/src/api/chat_v3.rs`
    - `chat_app_server_rs/src/api/chat_stream_common/types.rs`
    - `chat_app/src/lib/api/client/stream.ts`

这一步的意义：

- 聊天主链路终于从协议语义上完成了：
  - `send` 负责命令提交
  - `realtime/ws` 负责流式事件
  - `stream` 只剩历史兼容 / 断线 fallback
- 后面如果继续下掉前端 SSE reader，这一层已经不会再和命令提交耦在一起

- realtime-only 聊天收尾链也已经补齐：
  - `useChatStreamRealtimeBridge` 现在在收到 terminal realtime 事件时会直接负责：
    - `completed` 后 finalize 本地 streaming state
    - `cancelled` 后也直接 finalize，不再依赖 SSE 连接自然关闭
    - 当前会话仍停留在该 session 时，再补一次 `loadMessages()` 做最终对齐
  - `sendMessage` 在 realtime 命令提交路径下，已经移除了“提交后立刻 `loadMessages()`”这层过早回源
  - 这样可以避免：
    - realtime 模式下因为没有 SSE close 信号而导致 loading 卡住
    - 聊天刚 accepted 就立刻回源消息列表，结果过早拿到未完成快照
  - 主要修改文件：
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
    - `chat_app/src/lib/store/actions/sendMessage.ts`

这一步的意义：

- websocket 已连接时，聊天主路径已经具备完整的“提交 -> 流式事件 -> terminal 收尾 -> 最终快照对齐”闭环
- 旧 SSE reader 现在更接近真正只剩断线 fallback，而不是暗中承担 terminal 收尾职责

- 聊天 realtime bridge 又补了一层“会话订阅职责回收 + turn 对齐保护”：
  - `useChatStreamRealtimeBridge` 不再只依赖“当前聊天页上其它 hook 顺带订阅当前会话”
  - 现在它会主动订阅所有 `sessionChatState.isStreaming === true` 的会话 topic
  - 这样即使用户切走当前联系人，只要该会话还在生成，bridge 也能继续收到对应 realtime 事件
  - 同时 bridge 现在会按 `conversation_turn_id / turn_id` 和本地 active draft 的 turn 严格匹配
  - 这样可以避免：
    - 同一会话里上一轮迟到事件串到下一轮草稿上
    - 流式事件订阅职责隐式分散在多个聊天/面板 hook 里
  - 主要修改文件：
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`

这一步的意义：

- 聊天 realtime 主路径不再暗依赖别的 conversation hook 来“顺手保活订阅”
- 流式事件已经从“按 session 粗路由”进一步收紧到“按 session + turn 精确路由”

- 前端聊天旧 SSE 执行器又去掉了一层已失效的双轨兼容：
  - `sendMessage` 在 fallback 路径下，传给 `streamExecution` 的已经只剩纯 SSE/ReadableStream 语义
  - `streamExecution` 不再保留“realtime 模式下只消费 terminal 事件”的分支
  - `streamChat()` 请求体也不再携带 `prefer_realtime_stream`
  - 相关 runtime option / payload 类型已同步收敛
  - 主要修改文件：
    - `chat_app/src/lib/store/actions/sendMessage.ts`
    - `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`
    - `chat_app/src/lib/store/actions/sendMessage/requestPayload.ts`
    - `chat_app/src/lib/store/actions/sendMessage/types.ts`
    - `chat_app/src/lib/api/client/stream.ts`
    - `chat_app/src/lib/api/client/types/runtime.ts`

这一步的意义：

- 现在前端语义上更清楚地变成：
  - `chat/send` + `realtime/ws` 是主路径
  - `chat/stream` + `ReadableStream/SSE parser` 是纯 fallback
- 旧 `streamExecution / streamReader` 已经不再夹带主路径的 realtime 兼容分支，后续继续清理会更安全

- 聊天主路径的“断线后快照收尾”也补了一层：
  - `mergeMessagesWithStreamingDraft()` 现在会先检查：
    - 当前会话是否仍标记为 `isStreaming`
    - 服务器最新消息快照里是否已经存在同一 `turn_id` 的最终 assistant 消息
  - 如果后端快照已经有该 turn 的最终 assistant：
    - 本地 `sessionStreamingMessageDrafts[sessionId]` 会被清空
    - `sessionChatState[sessionId]` 会直接结束 loading / streaming / stopping
    - 当前会话全局 `isStreaming` / `streamingMessageId` 也会同步复位
    - 不再把旧的本地 streaming 草稿继续 merge 回消息列表
  - 同时补了对应前端单测，覆盖“服务端已持久化最终消息时，本地 stale draft 不应回灌”的场景
  - 主要修改文件：
    - `chat_app/src/lib/store/actions/messagesState.ts`
    - `chat_app/src/lib/store/actions/messagesState.test.ts`

这一步的意义：

- 即使 realtime 主路径中途断线，后续一旦通过 HTTP 快照拿到了最终消息，前端也能自动完成聊天收尾
- 这让 `chat/send + realtime/ws` 不再过度依赖“必须亲自收到 terminal realtime 事件才能解除 loading”

- 聊天 realtime bridge 的“中途断线受控恢复”已经正式接通：
  - `useChatStreamRealtimeBridge` 现在会监听 realtime 连接状态
  - 当连接状态从 `connected` 掉到 `disconnected/error` 时：
    - 只会对当前仍处于 `sessionChatState.isStreaming === true` 的会话触发恢复
    - 每个会话都会走一次 `syncSessionMessagesInBackground(sessionId)`
    - 增加了最小 inflight 去重和 `4s` 冷却，避免 websocket 抖动时对同一会话疯狂回源
  - 这样如果聊天主路径在生成中途掉了 realtime 连接，前端会主动拉一次该会话消息快照，尝试尽快把已落库的最终 assistant 收尾回来
  - 主要修改文件：
    - `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
    - `chat_app/src/lib/store/actions/messagesLoading.ts`

- 后台消息同步现在已经真正做到“静默对齐，不误伤当前界面 loading”：
  - `messagesLoading` 里的快照应用逻辑已拆成前台/后台两种模式
  - `loadMessages()` 仍会正常接管可见消息列表并清理全局 `isLoading`
  - `syncSessionMessagesInBackground()` 则只同步目标会话消息、streaming draft 和 turn process cache
  - 后台同步不会再误清当前界面的全局 `isLoading` / `error`
  - 这样可以保证：
    - 其它会话断线恢复时，不会把当前会话的 loading 提前解除
    - 复用快照合并逻辑时，也不会把“后台回源”伪装成“当前页面请求完成”
  - 同时补了一个前端单测，专门覆盖“后台同步不能清掉全局 loading/error”这个场景
  - 主要修改文件：
    - `chat_app/src/lib/store/actions/messagesLoading.ts`
    - `chat_app/src/lib/store/actions/messagesLoading.test.ts`

- 本轮验证结果：
  - 前端单测：`chat_app` `npm run test -- src/lib/store/actions/messagesLoading.test.ts src/lib/store/actions/messagesState.test.ts` 通过
  - 前端构建：`chat_app` `npm run build` 通过
  - 后端检查：`chat_app_server_rs` `cargo check` 通过

- `summary / memory` 这条链又去掉了一层无意义回源：
  - 主聊天页的 memory 上下文现在拆成了两类刷新：
    - 全量 memory 刷新：`session summaries + agent recalls`
    - summary-only 刷新：只拉 `session summaries`，保留已有 recall cache
  - 因此以下场景不再顺手重复请求 `getContactAgentRecalls()`：
    - `conversation.summaries.updated` realtime 到达
    - `review-repair completed` 后的 summary 面板刷新
    - workbar 打开历史时触发的 summaries 刷新
  - 手动点击“刷新记忆”仍然保留全量刷新，不影响用户主动拿最新 recall
  - 这样可以减少 summary 变更场景下的双接口回源，把“只改 summaries，却顺带重拉 recalls”的噪音收掉
  - 主要修改文件：
    - `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`

- 本轮补充验证结果：
  - 前端单测：`chat_app` `npm run test -- src/lib/store/actions/messagesLoading.test.ts src/lib/store/actions/messagesState.test.ts` 通过
  - 前端构建：`chat_app` `npm run build` 通过

- `review-repair completed` 与 `conversation.summaries.updated` 的双重刷新也去重了：
  - 后端在复盘完成时会连续推两条事件：
    - `conversation.review_repair.completed`
    - `conversation.summaries.updated`
  - 主聊天页与团队成员工作区现在都统一只依赖 `conversation.summaries.updated` 来刷新 summaries
  - 不再在 `review_repair.completed` 的 `onCompleted` 回调里额外再刷一次 summary
  - 这样可以避免复盘完成瞬间同一会话 summary 面板被连续回源两次
  - 主要修改文件：
    - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`

- 本轮再次验证结果：
  - 前端构建：`chat_app` `npm run build` 通过

- 聊天旧 SSE fallback 执行器已经按需加载，不再常驻主包：
  - `sendMessage` 现在只有在明确走 fallback `chat/stream` 路径时，才会动态 `import('./sendMessage/streamExecution')`
  - 因此以下旧模块不再被 realtime 主路径首包静态带入：
    - `streamExecution.ts`
    - `streamReader.ts`
    - `sse.ts`
  - 这一步没有改 fallback 语义，只是把旧 SSE 执行器变成真正的“按需兜底”
  - 与当前主架构更加一致：
    - `HTTP send + realtime/ws` 是默认主路径
    - `chat/stream + SSE parser` 只在断线 fallback 时才加载执行
  - 构建结果也已经反映出这点：
    - 新增了独立 chunk `dist/assets/streamExecution-*.js`
    - 主包 `index-*.js` 约从 `605.68 kB` 降到 `603.23 kB`
  - 主要修改文件：
    - `chat_app/src/lib/store/actions/sendMessage.ts`

- 本轮补充验证结果：
  - 前端构建：`chat_app` `npm run build` 通过
  - 后端检查：`chat_app_server_rs` `cargo check` 通过

- 团队成员侧边栏的“待处理数”这条链这轮又补齐了一块关键缺口：
  - 之前 `ui prompt` 已经有：
    - 会话级共享 cache / inflight 去重
    - 当前会话首包加载
    - 团队成员 session 静默预热
  - 这轮把 `task review` 也补成了同样的首屏快照链路：
    - 后端新增 `GET /api/task-manager/reviews/pending?conversation_id=...`
    - `TaskReviewHub` 新增按 conversation 导出当前 pending reviews 的能力
    - 前端新增 `pendingTaskReviewCache.ts`
    - 当前聊天会话会先 hydrate / load pending task review 快照
    - 团队成员侧边栏会对各成员 session 静默预热 pending task review
    - realtime `review_required / review_confirmed / review_cancelled` 与本地 confirm/cancel 现在也会同步维护这层共享 cache
  - 同时顺手把 `task_board` realtime payload 的 `timeout_ms` 前后端字段也补齐，避免 review panel 走不同通道时结构不一致
  - 另外，当前会话从 `pending ui prompt` cache hydrate 时也改成了“快照同步”，后端已不存在的旧 panel 会被一并清掉，减少脏计数残留
  - 主要修改文件：
    - `chat_app_server_rs/src/services/task_manager/review_hub.rs`
    - `chat_app_server_rs/src/api/task_manager.rs`
    - `chat_app_server_rs/src/services/task_manager/mod.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app/src/lib/api/client/tasks.ts`
    - `chat_app/src/lib/api/client/facades/runtimeFacade.ts`
    - `chat_app/src/lib/api/client/types/runtime.ts`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/components/chatInterface/helpers.ts`
    - `chat_app/src/components/chatInterface/pendingTaskReviewCache.ts`
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
    - `chat_app/src/components/chatInterface/useGlobalConversationPanelsRealtime.ts`
    - `chat_app/src/components/chatInterface/usePanelActions.ts`
    - `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`

- realtime invalidation 这一层这轮也补了一次“串行队列 + 不丢后续事件”的收口：
  - 之前多条列表 / 摘要 realtime hook 里有同一类问题：
    - 某次事件触发的 reload 还在 inflight 时，后续事件会被直接丢掉
    - 或者不同 hook 各自写一套 inflight/timer，小行为并不一致
  - 现在新增了通用 helper：
    - `chat_app/src/lib/realtime/invalidationQueue.ts`
  - 并统一接入了这些 hook：
    - `useSessionsRealtime`
    - `useContactsRealtime`
    - `useProjectsRealtime`
    - `useRemoteConnectionsRealtime`
    - `useTerminalListRealtime`
    - `useProjectChangeSummaryRealtime`
    - `useNotepadRealtime`
    - `useConversationSummariesRealtime`
  - 统一后的语义是：
    - 正在处理一条失效事件时，不再把后续事件直接丢掉
    - 只保留“最新一次待处理失效”，当前处理结束后立刻补跑一次
    - 避免高频 burst 事件下既重复回源、又漏掉最终一次状态
  - 这一步的收益：
    - `contacts / projects / remote connections / sessions` 这类全局列表在 burst 更新下更稳
    - `project change summary / terminal list / notepad / conversation summaries` 这种失效驱动刷新不会再因为 inflight 窗口漏一拍
    - 继续减少“前端看起来请求很多，但某次变化又没同步上”的不稳定体验

- 本轮补充验证结果：
  - 前端构建：`chat_app` `npm run build` 通过

- 启动期 / 面板打开时的基础配置请求这轮又去掉了两条重复加载源：
  - `loadProjects / loadSessions / loadContacts` 之前已经有共享 cache / inflight
  - 这轮把 `AI model configs` 和 `agents` 也补成了同样的模型：
    - `chat_app/src/lib/store/actions/aiModels.ts`
    - `chat_app/src/lib/store/actions/agents.ts`
  - 现在这些场景会自动复用共享结果，而不是各自再打一遍：
    - 主聊天页初始化
    - 侧边栏 `useSessionListBootstrap`
    - `AiModelManager`
    - `UserSettingsPanel`
    - `SessionSummaryJobConfigPanel`
  - 统一后的语义：
    - 首次加载时共享 inflight 去重
    - 后续非 stale 命中共享 cache
    - `updateAiModelConfig()` 后会只把对应 cache 标成 stale，再走一次强制刷新
    - 删除模型配置时会同步回写共享 cache，减少后续面板 reopen 的回源
  - 这一步主要减少的是：
    - 启动期 `loadAiModelConfigs()` 重复请求
    - 打开设置类面板时再次重复拉同一份模型配置
    - 主聊天页和侧边栏同时存在时的 `loadAgents()` 重复请求

- 本轮再次验证结果：
  - 前端构建：`chat_app` `npm run build` 通过

- 侧边栏项目运行态这轮又去掉了动作成功后的显式终端全量强刷：
  - `chat_app/src/components/sessionList/useProjectRunState.ts`
  - `handleRunProject / handleStopProject / handleRestartProject` 现在会读取全局 realtime 连接快照
  - 当 `/api/realtime/ws` 已处于 `connected` 时：
    - 不再在命令成功后额外 `loadTerminals()`
    - 直接依赖已有的 `terminal.list.invalidated` 与 `project.run.state_changed` 来更新列表和运行态
  - 当 realtime 未连接时：
    - 仍然保留 `loadTerminals()` 作为 HTTP 兜底
  - 这一步继续减少了项目 start / stop / restart 后紧跟的一次整批 `/api/terminals` 请求
  - 也让 sidebar 项目运行态与前面已经接入的共享终端快照策略保持一致

- 当前 Phase 3 剩余的一条高价值链路更清晰了：
  - 终端面板自己的专用 socket 生命周期里，`state / exit / close` 原本也会触发 `loadTerminals()`
  - 这轮已经继续收了一层：
    - `chat_app/src/components/terminal/useTerminalSocketLifecycle.ts`
    - 现在当全局 realtime 已连接时，不再因为 terminal 专用 socket 的 `state / exit / close` 再补打一轮 `/api/terminals`
    - 仍然保留 realtime 未连接时的 `loadTerminals()` 兜底
  - 这样可以继续减少：
    - 当前打开终端窗口时，因为 busy 切换触发的列表回源
    - 终端退出时 `terminal ws event + 全局 realtime` 的重复刷新

- `terminal.list.invalidated(reason=created)` 这条链也继续从“整批刷新”收成了“局部优先”：
  - `chat_app/src/components/sessionList/useSessionListController.ts`
  - 之前只要收到 `created`，前端就会直接 `markTerminalsStale()` 然后 `loadTerminals()`
  - 现在改成：
    - 如果事件里带了 `terminal_id`，先 `refreshTerminalById(terminalId)`
    - 只有局部刷新没有拿到终端时，才回退到整批 `loadTerminals()`
  - 这一步继续减少了：
    - 新建终端后的整批 `/api/terminals`
    - 项目 runner 启动过程中因 terminal created 触发的列表全量回源

- 终端面板挂载时的入口级列表读取也收紧了一层：
  - `chat_app/src/components/terminal/useTerminalRuntime.ts`
  - 之前每次打开 Terminal 面板都会调用一次 `loadTerminals()`
  - 现在改成只有在 `currentTerminal` 为空时，才用这次加载去补：
    - 终端列表初始化
    - 上次终端选择恢复
  - 当当前终端已经存在时，不再因为重新打开 Terminal 面板重复触发这次入口级列表加载

- 联系人列表这条 realtime invalidate 链这轮也补上了“局部 refresh 优先”能力：
  - 后端新增单联系人读取：
    - `chat_app_server_rs/src/api/contacts.rs`
    - `chat_app_server_rs/src/services/memory_server_client/contact_ops.rs`
  - 前端新增单联系人读取 / 刷新：
    - `chat_app/src/lib/api/client/workspace/contacts.ts`
    - `chat_app/src/lib/api/client/facades/workspace/contactsFacade.ts`
    - `chat_app/src/lib/store/actions/contacts.ts`
  - `sessionList` 的 `contacts.updated` 现在对以下事件优先局部刷新：
    - `contact_created`
    - `contact_updated`
    - `contact_upserted`
  - 只有局部刷新拿不到联系人时，才回退到整批 `loadContacts()`
  - 这样继续减少了联系人侧边栏因为单个联系人变化而触发的整批 `/api/contacts?limit=2000`

- `projects / remote connections / sessions` 这三条列表 realtime 主链，这轮也把“局部刷新失败后的整批兜底”补齐了：
  - `chat_app/src/components/sessionList/useSessionListController.ts`
  - 之前这些链路在 `created / updated` 事件上虽然已经优先：
    - `refreshProjectById(projectId)`
    - `refreshRemoteConnectionById(connectionId)`
    - `refreshSessionById(sessionId)`
  - 但如果局部刷新失败，前端不会立即回退整批列表刷新
  - 现在改成：
    - 局部刷新成功：继续只 patch 单条
    - 局部刷新失败：自动标 stale 并回退到整批 `loadProjects / loadRemoteConnections / loadSessions`
  - 这样可以减少局部刷新偶发失手时的列表残缺，也让几条主链的 realtime 行为更一致

- `workbar task mutation` 这轮继续从“成功后强刷当前 turn”往“本地 patch 优先”推进了一步：
  - `chat_app/src/components/chatInterface/workbarCache.ts`
  - `chat_app/src/components/chatInterface/useWorkbarState.ts`
  - `chat_app/src/components/chatInterface/useWorkbarMutations.ts`
  - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
  - 当前已经切到本地 patch 优先的场景：
    - task complete
    - task edit
    - task delete
  - 具体行为调整：
    - mutation 成功后优先用接口返回的 `TaskManagerTaskResponse` 直接 patch 当前 turn 共享 cache 与本地 state
    - 删除任务时优先本地剔除当前 turn / history 已加载快照
    - 只有本地快照没有命中时，才回退到对应 scope 的 HTTP 强刷
    - history 抽屉未打开时仍然只标 `history stale`，不把非可见历史重新打回整批回源
  - 这一步继续减少了：
    - 非 realtime 模式下 task edit / complete 后紧跟的一次 `GET /task-manager/tasks?conversation_turn_id=...`
    - task delete 后为了同步当前 turn 再打一轮同 turn 快照
    - history 已经打开时，先显示旧值再等 reload 覆盖的一次视觉抖动

- `conversation.task_board.updated` 这条 realtime 链这轮又补成了“task snapshot 优先”：
  - 后端现在在以下事件里附带完整 `task` 快照，而不再只有 `task_id`：
    - `task_created`
    - `task_updated`
  - `task_deleted` 仍然只带 `task_id`，前端按本地 remove 处理
  - 主要修改文件：
    - `chat_app_server_rs/src/services/realtime/types.rs`
    - `chat_app_server_rs/src/services/realtime/hub.rs`
    - `chat_app_server_rs/src/services/task_manager/store/create_ops.rs`
    - `chat_app_server_rs/src/services/task_manager/store/write_ops.rs`
    - `chat_app/src/lib/realtime/types.ts`
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
  - 前端收到 `task_created / task_updated` realtime 时，现在会优先：
    - patch 当前 turn workbar cache / state
    - history 抽屉已打开时同步 patch history
    - history 抽屉未打开时只标 `history stale`
  - 如果收到的是旧事件或 payload 里没有 `task`，仍然回退到原来的 reload 路径，保证兼容
  - 这一步的直接收益是：
    - `review confirm` 之后由异步 tool 流程创建出来的任务，前端更容易直接就地出现，不必再等一次 `/task-manager/tasks`
    - 普通 `task_updated` realtime 也开始从“事件到达后 reload”收成“事件到达后本地 patch”
    - chat 主界面与团队成员界面的 task board 同步会更顺，减少事件后的一次回源抖动

- `task board` 本地 mutation 与 realtime 回声这轮也补上了短时去重：
  - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
  - `chat_app/src/components/chatInterface/useWorkbarMutations.ts`
  - 现在当全局 realtime 已连接时：
    - 本地 `task edit / complete / delete` 成功后，会先登记一条短时 mutation guard
    - 随后收到同一条 `task_updated / task_deleted` realtime 回声时，前端会直接消费掉，不再重复进入 reload / patch 分支
  - guard key 目前按：
    - `action`
    - `taskId`
    - `turnId`
    组合，并带 4 秒短 TTL
  - 这一步继续减少了：
    - 本地 mutation 刚成功，又被同一条 realtime 回声再次触发的重复处理
    - connected 模式下 `task_updated / task_deleted` 事件造成的多余 HTTP 兜底
    - workbar 在本地 patch 后又被相同事件二次刷新带来的轻微抖动

- 本轮验证结果：
  - 前端构建：`chat_app` `npm run build` 通过
  - 后端检查：`chat_app_server_rs` `cargo check` 通过

- `复盘` 统计与选数口径这轮重新校正回“未被总结的消息”：
  - 根因复盘：
    - 之前一度把 `review-repair` 的待处理集合改成了“尚未被 `manual_review_repair*` summary 覆盖的消息”
    - 这会导致：
      - 只要当前 session 还没有任何 review-repair summary
      - 即使大量历史消息早就被普通 summary 处理过
      - 手动复盘仍可能把整段历史消息重新卷进去
  - 当前修复：
    - `review-repair` 的 `pending_message_count` 与实际选数，重新对齐回 `messages.summary_status = pending`
    - 也就是只处理“当前 scope 下仍未被总结”的消息
    - 不再按“是否做过 review-repair 覆盖”来计算候选
    - 同时移除了 `review-repair` 在 scope 级别对 `max_sessions_per_tick` 的复用限制
    - 手动复盘现在会覆盖当前联系人 + 项目范围内所有仍未总结的 session/message，而不是只处理被定时任务配置截出来的一部分
  - 直接收益：
    - 手动复盘不会再把已经总结过的整段历史消息重新拿去处理
    - `selected / marked / pending_message_count` 的语义会重新一致
    - 手动复盘不会再被定时总结配置里的 session 限流策略悄悄截断
    - 更符合最初产品定义：复盘总结当前联系人对应项目下“所有未总结的消息”
  - 相关文件：
    - `memory_server/backend/src/repositories/messages/read_ops.rs`
    - `memory_server/backend/src/jobs/summary.rs`
- `review-repair` 任务诊断字段这轮也补齐了失败态：
  - 之前如果复盘在 LLM / summary 落库 / memory sync / mark 阶段失败
  - `job_runs` 往往只会留下 `status=failed`
  - 但 `pending / selected / marked / pending_after` 这些诊断列仍然是空的
  - 当前修复后：
    - 失败任务也会尽量写入当时的 `pending_before_count`
    - `selected_count`
    - `marked_count`
    - `pending_after_count`
  - 这样任务面板里即使失败，也更容易看出：
    - 本次原本打算处理多少条
    - 实际有没有动到任何消息
    - 失败前后 pending 是否变化
  - 相关文件：
    - `memory_server/backend/src/jobs/job_support.rs`
    - `memory_server/backend/src/jobs/summary_support.rs`
    - `memory_server/backend/src/jobs/summary_generation.rs`
- `review-repair` 的 scope 统计这轮也改成真正的聚合查询：
  - 之前 scope 级 `pending_message_count` 与可处理 session 列表，是先列出 scope 下所有 session，再逐个 session 计数累加
  - 当前改成直接复用 messages 聚合层：
    - `list_session_ids_with_pending_messages_by_scope`
    - `count_pending_messages_by_scope`
  - 这样可以让复盘 scope 统计：
    - 更直接表达“当前联系人 + 项目范围内还有哪些 session 存在未总结消息”
    - 减少逐 session 计数循环带来的额外噪音
    - 顺手把之前几个已经实现但没真正接上的 scope 聚合函数接回实际链路
  - 相关文件：
    - `memory_server/backend/src/repositories/messages.rs`
    - `memory_server/backend/src/jobs/summary.rs`

## 注意事项

- 重接口仍然保持“HTTP 快照 + realtime invalidation”，不要改成持续推送全量数据。
- 现有 terminal 专用 WebSocket 先不动，继续保留。
- 每完成一个阶段，都要同步更新这个文件，避免后面改造状态失真。
