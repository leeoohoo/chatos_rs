# 长链接剩余缺口执行计划

## 当前结论

目前 realtime / WebSocket 主骨架已经落地，`review-repair`、任务面板、终端列表、项目运行态、项目变更摘要这些链路已经不再是“纯轮询”。

但从代码现状看，仍然有几条高价值链路没有完全收口，主要不是“没有事件”，而是：

1. 聊天主链仍保留真实 SSE fallback，但发送入口已经进一步收成“websocket 命令优先，SSE 仅在连接窗口超时、网络/服务端错误时降级”。
2. 聊天异常断流与部分失败终态仍会主动 HTTP reload messages；成功态、取消态、一部分失败态，以及断线恢复已经进一步收成“turn snapshot + turn 级恢复优先”，但仍保留少量 whole-session 兜底。
3. `sessions / contacts / projects / remote_connections` 虽然都已开始支持 payload patch，而且事件后的 delete/update 回源也已进一步压成“单条 refresh / 404 本地删除优先”，但首次冷启动、少数 invalidation-only reason、以及 created 场景 detail 缺失时仍会回退到 HTTP refresh。
4. `project runner catalog` 等少数列表态里，虽然主链已经开始携带 snapshot，但仍有少数 payload 缺字段时会回退 HTTP snapshot。
5. memory 后台管理前端的 `JobRunsPage` 已切到 SSE 优先，但其他独立后台页仍值得继续排查是否残留轮询。

## 代码证据

### 1. 聊天主链仍未完全收成 WebSocket 主路径

- 发送入口仍保留二选一：
  - 先给 WebSocket 一个更长的短等待窗口，并优先尝试 `sendChatCommand`
  - 只有连接窗口内始终没连上，或 `sendChatCommand` 遇到网络/服务端错误，才降级到 `streamChat` SSE
- 位置：
  - [sendMessage.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage.ts#L241)
  - [stream.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/api/client/stream.ts)
  - [state.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/realtime/state.ts)

现状判断：

- 这已经比以前好很多，但还不是“统一长链接主路径”。
- 现在已经从“发送前直接二选一”往“WebSocket 命令优先，SSE 条件降级”收了一层。
- 本轮又收掉了一个高价值误回退：
  - `sendChatCommand` 的 `4xx` 现在不会再自动降级 SSE
  - 这意味着“请求本身非法”的场景不会再沿两条发送链各打一遍
- SSE 仍未彻底退出主链，但剩余问题已经更集中在“连接可靠性”和“终态回执完整性”，而不是前端误判。

### 2. 聊天异常断流与部分失败终态仍有 message reload 回源

- SSE fallback 成功态已不再无条件 `loadMessages(currentSessionId)`：
  - 收口点：
    - [streamExecution.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/streamExecution.ts)
    - [persistedTurnMessages.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.ts)
- WebSocket `cancelled` 终态现在也会优先消费持久化消息回执，再决定是否回退 `loadMessages(active.sessionId)`：
  - [useChatStreamRealtimeBridge.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts)
- 当前更核心的剩余点，主要落在异常断流里“后端终态事件本身都没来齐”或“既没有持久化 user 回执、也无法稳定本地归属”的失败场景：
  - [useChatStreamRealtimeBridge.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts)
  - [streamExecution.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/streamExecution.ts)
- 最新已收掉的一层：
  - [persistedTurnMessages.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/persistedTurnMessages.ts) 现在允许“temp user 仍残留，但同 turn 已经存在可靠 local final assistant”时直接本地收口
  - [turnRecovery.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts) 现在允许“runtime snapshot 已终态，但 turn 级消息接口暂时为空”时优先用本地 terminal assistant 收口
  - 这意味着剩余 whole-session fallback 已经进一步收缩到“既没有可靠 persisted 回执，也没有可靠 local terminal assistant”的场景

现状判断：

- 聊天正文增量已经可以靠 realtime 消费。
- 成功态与取消态已经改成“终态 payload 对账优先 + 条件兜底”。
- 一部分失败态也已改成“持久化 user message 对账成功后允许本地错误 assistant 保留”，不再强制整段回源。
- realtime 断线恢复也已经从“一断就整段 sync”收成了“仅 realtime 主链 + 短重连窗口 + 条件恢复”。
- 当前剩下最核心的回源点，是 realtime / SSE 都缺少足够终态信息，并且本地 terminal assistant 也不够可靠时的最后一层 whole-session fallback。

### 3. 列表链路仍有剩余 HTTP fallback

- `sessions / contacts / projects / remote_connections` 现在都已经支持 create/update 事件 payload patch：
  - [useSessionListController.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/sessionList/useSessionListController.ts)
- 但以下场景仍然会回退 HTTP：
  - created 场景里 payload 缺失且 detail 刷取也失败时的整表刷新
  - 少数 invalidation-only reason 的整表刷新
  - 冷启动时的初始 snapshot
- 最新已收掉的一层：
  - 四类列表在 realtime 事件里只要拿到实体 id，就会先走单条 `refresh*ById()`
  - 单条 detail 返回 `404` 时，会优先本地删除，不再继续整表刷新
  - 这意味着 delete / update 场景下的整表 `load*()` 已经明显减少，剩余主要集中在 create race 和无 id invalidation

现状判断：

- 这条不是 bug，而是还存在兼容型 fallback。
- 当前四类列表已经从“纯失效通知”走到“payload patch 优先 + detail patch / 404 本地删除 + HTTP 兜底”。

## 优先级建议

### P1. 聊天主链彻底减少回源

目标：

1. 保留 SSE 兜底，但进一步压缩进入 SSE 的概率。
2. 继续减少 turn 级恢复失败后的 whole-session fallback，重点补足“turn 切片拿到了但 tool/final-assistant 信息仍不完整”以及“终态回执没来齐、但本地 draft 已基本完整”这类边界。
3. 能从 realtime payload 或 turn 级恢复直接补齐最终 assistant message / tool state，就尽量不再整段 reload。
4. 后续如果继续做聊天主链优化，优先看真实日志里还有没有大量 websocket 连接超时或 `5xx`，不要再围绕 `4xx` 降级做重复工作。

原因：

- 这是用户体感最强的一条链路。
- 也是“看起来已经 WebSocket 化，但实际上还在大量回拉”的核心点。

### P1. 列表链路继续去 fallback

目标：

1. 继续把 `sessions / contacts / projects / remote_connections` 的 delete / archiving / stale 场景也往 payload patch 方向补齐。
2. 能从事件直接判定删除/归档的，不再顺手标脏整表。
3. 把冷启动后的补拉次数继续压缩，尽量减少事件到达后的 `refresh*ById()`。

原因：

- 四类高频列表的 create/update 已经打通，剩下主要是兼容性回退收口。
- 这条改造收益稳定，风险也可控。

## 不再列为“未完成”的项

以下内容不应该再放在“剩余缺口”里反复追：

1. `review-repair` 前端持续轮询：
   - 主前端已改成 realtime 优先 + 状态接口兜底，不再是持续 polling。
2. `project.run.catalog.updated` 触发成员与 runner 全量双刷：
   - 这块已经拆开。
3. `project change summary` 固定轮询：
   - 这块已经事件化。
4. `conversation summaries` 还是事件触发 HTTP reload：
   - 这块已经改成后端推 summary 快照、前端直接 patch 本地状态。
5. `contacts / projects / remote_connections` 还是纯 invalidation：
   - 这块已经改成后端推实体快照、前端优先本地 patch。
6. `terminal.list.invalidated` 还是纯 invalidation：
   - 这块已经改成后端可推 terminal 快照，前端优先本地 patch。
7. `project.run.catalog.updated` 完全只能靠 HTTP 刷脚本存在性：
   - 这块已经升级成“payload 快照优先，缺字段时再 fallback”。
8. `memory_server/frontend` 的 `JobRunsPage` 还是固定 `10s` 轮询：
   - 这块已经改成 SSE snapshot + upsert 增量，保留手动刷新与 resync 兜底。

## 推荐执行顺序

1. 聊天主链结束态去回源。
2. 收紧四类列表的 delete / archiving fallback。
3. 继续清理聊天异常断流 / 少量缺失回执的终态回源，重点覆盖真正拿不到终态 assistant 正文、本地 tool 状态不全、或本地映射不稳的边界。
4. 排查 memory 后台其他页面是否还残留高频定时刷新。
