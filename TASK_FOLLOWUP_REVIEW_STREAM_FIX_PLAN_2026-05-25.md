# Task Follow-up / Review 阶段流式可见性修复与优化方案（2026-05-25）

## 1. 结论先行（当前实现是否健壮）

当前实现的“同一轮继续执行 + 复查”主逻辑是存在且可工作的，但在前端可见性与状态表达上不够健壮，表现为：

1. 功能逻辑层面：基本可用（有 follow-up 与 review 分支、有限重试、单轮内闭环）。
2. 用户体验层面：不稳定（主总结可能不及时可见，review 阶段无明确 UI 阶段态）。
3. 协议层面：语义不完整（只有 `thinking/chunk/complete`，缺少“当前在 review 阶段”的显式事件）。

结论：**业务逻辑“在”，但端到端体验可靠性不足，需要补齐事件语义与前端阶段状态机。**

---

## 2. 现状核对（与你的问题逐条对应）

### 2.1 你问的“这个逻辑还在不在”

逻辑仍在，主要入口：

1. 后端任务判定与 follow-up 指令：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/modules/conversation_runtime/task_board.rs`
2. v3 执行循环：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
3. v2 执行循环：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_client/mod.rs`

### 2.2 你问的“前端是不是 SSE”

你的判断是对的：**前端主链路是 realtime 长连接（WebSocket）+ `chat_stream` 事件，不是 SSE。**

1. 前端发起：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/api/client/stream.ts`（`sendChatCommand`）
2. 前端消费 realtime：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/realtime/client.ts`
   - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
3. 后端仍保留 `SseSender` 兼容通道，但当前路径 `sender=None`，事件通过 realtime 发布：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/modules/conversation_runtime/chat_usecase.rs`
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/core/chat_stream/events.rs`

---

## 3. 根因分析（为什么会“看不到总结/复查总结”）

### 根因 A：Review 阶段被设计为隐藏流式

在 `ReviewExecution` 分支，后端把可见流回调关闭：

1. `on_chunk: None`
2. `on_thinking: None`

位置：

1. `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
2. `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_client/mod.rs`

这会导致 review 过程本身在前端“无流式可见信号”。

### 根因 B：前端只有 `AI is thinking`，没有 `AI is reviewing`

当前 loading 文案固定读 `messageList.aiThinking`，没有阶段态区分。

位置：

1. `/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/MessageList.tsx`
2. `/Users/lilei/project/my_project/chatos_rs/chat_app/src/i18n/messages.ts`

### 根因 C：缺少“阶段事件”导致前端无法准确切换状态

现有事件类型里没有“turn phase = execution/review”。前端只能从 chunk/thinking 被动推断，review 隐藏流式时就无法推断。

位置：

1. `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/utils/events.rs`
2. `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/streamEventHandler.ts`

---

## 4. 修复目标（按你的期望落地）

目标行为：

1. 主执行完成后，当前轮总结先可见（先刷主总结）。
2. 进入复查阶段时，前端 loading 文案显示 `AI is reviewing`（中文同理）。
3. 复查结束后，前端能收到并呈现“复查结果”（至少状态可见；可选附加说明）。

---

## 5. 修复方案（低风险、可分阶段上线）

## 阶段 1（必须做，最小闭环）

### 5.1 后端：新增阶段事件（realtime）

新增一个轻量事件类型，建议命名：

1. `turn_phase`

事件 payload 建议：

```json
{
  "type": "turn_phase",
  "timestamp": "...",
  "data": {
    "phase": "execution" | "review",
    "reason": "task_follow_up",
    "turn_id": "..."
  }
}
```

改造点：

1. 常量定义：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/utils/events.rs`
2. 发送函数：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/core/chat_stream/events.rs`
3. 触发时机（关键）：
   - 在 follow-up 判定为 `ReviewExecution` 时发送 `phase=review`
   - 在回到 `ContinueExecution` 时发送 `phase=execution`
   - 文件：
     - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
     - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_client/mod.rs`

### 5.2 前端：增加会话级流式阶段状态

在 `SessionChatState` 增加字段（建议）：

1. `streamingPhase?: 'thinking' | 'reviewing' | null`

改造点：

1. 类型与默认值：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/types.ts`
   - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/sessionState.ts`
2. 事件消费：
   - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/streamEventHandler.ts`
   - 收到 `turn_phase`:
     - `phase=review` -> `streamingPhase='reviewing'`
     - `phase=execution` -> `streamingPhase='thinking'`
3. 流结束/失败/取消时清理：
   - `finalizeStreamingSessionState` / `failSendMessageState` 重置为 `null`

### 5.3 前端：loading 文案切换

1. `MessageList.tsx` 读取 `streamingPhase`
2. `reviewing` 时显示 `messageList.aiReviewing`
3. 否则沿用 `messageList.aiThinking`
4. i18n 新增键：
   - 中文：`AI 正在复查...`
   - 英文：`AI is reviewing...`
   - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/i18n/messages.ts`

---

## 阶段 2（建议做，提升“总结不丢失”可靠性）

### 5.4 后端：complete 结果保持“可恢复一致性”

当前 `complete` 事件虽已在最终链路做 persisted message enrich，但建议再加一层结构化 review 元数据，避免前端只能猜：

```json
{
  "review": {
    "attempted": true,
    "outcome": "pass" | "needs_more_work" | "unknown",
    "rounds": 1
  }
}
```

目标：即使 review 流式隐藏，前端也能在 terminal 事件拿到“本轮复查发生过且结果如何”。

建议位置：

1. v3:
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
2. v2:
   - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_client/mod.rs`

### 5.5 前端：终态补充展示 review 结果（可选）

可先做轻量版（不改变消息正文）：

1. 在 streaming assistant metadata 写入 `review.outcome`
2. UI 可在消息尾部或 process 侧边栏展示 “复查通过/复查后继续执行”

涉及：

1. `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/streamLifecycleEvents.ts`
2. `/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/TurnProcessModal.tsx`（可选）

---

## 阶段 3（优化项：可观测性 + 防回归）

### 5.6 日志与埋点

为每次 turn 增加 phase 迁移日志（execution->review->execution...）：

1. 有助于定位“前端无总结/无阶段提示”类问题。
2. 可统计 review 触发率、平均轮次。

### 5.7 测试补齐

后端测试：

1. v3/v2：触发 review 时发送 `turn_phase(review)`
2. review fail 回退 continue 时发送 `turn_phase(execution)`
3. complete payload 包含 review 元数据（若实施 5.4）

前端测试：

1. `streamEventHandler.test.ts`：`turn_phase` 事件驱动 `streamingPhase` 变化
2. `chatStreamRealtimeCompletion.test.ts`：review 阶段到 complete 终态不丢主总结
3. `MessageList` 组件测试：`thinking` vs `reviewing` 文案切换

---

## 6. 兼容性与风险控制

### 6.1 向后兼容

1. 新增事件类型为增量，不影响旧事件消费。
2. 前端未知事件默认忽略，先后端上线不会导致崩溃。

### 6.2 风险点

1. 若 phase 事件发送时机过早/过晚，会出现文案闪烁。
2. 若终态清理漏掉 `streamingPhase`，可能残留“AI is reviewing”状态。

### 6.3 回滚策略

1. 前端可先容忍无 `turn_phase`（fallback to `aiThinking`）。
2. 后端 phase 发送可受 feature flag 控制（推荐）。

---

## 7. 实施顺序建议

1. 先做阶段 1（事件 + 前端 phase + 文案），快速修复你当前最直观的体验 bug。
2. 再做阶段 2（complete 携带 review 结构化结果），提升“总结可见性”兜底能力。
3. 最后做阶段 3（日志与测试），防止回归。

---

## 8. 我对“是否可靠”的最终判断

当前实现在“模型行为控制”上可靠性中等，在“用户可见反馈链路”上可靠性偏弱。  
按本方案完成阶段 1 + 2 后，整体可提升到生产可依赖水平，尤其是你关心的：

1. 主总结先出现
2. review 阶段明确可见
3. review 结束结果可追踪

