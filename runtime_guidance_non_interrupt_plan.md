# 运行中用户引导（不打断执行）方案

## 1. 目标

在会话处于执行态（流式输出、工具调用、迭代推理）时，允许用户发送“引导信息”，并满足：

1. 不中断当前执行链路（不触发 stop，不重开新 turn）。
2. 引导信息可在当前 turn 后续步骤被模型消费。
3. 用户能看到“已提交/已应用”的反馈。
4. 与现有 `task_review` / `ui_prompt` 机制共存，不互相污染。

---

## 2. 现状与问题定位

### 2.1 前端阻断点（当前无法中途引导）

- `chat_app/src/components/ChatInterface.tsx`
  - `inputDisabled={chatIsLoading || chatIsStreaming || chatIsStopping}`，执行中直接禁输入。
- `chat_app/src/components/InputArea.tsx`
  - 发送按钮 `disabled={disabled || isStreaming || isStopping}`。
- `chat_app/src/lib/store/actions/sendMessage.ts`
  - 若 `chatState.isLoading || chatState.isStreaming || chatState.isStopping`，直接 return。

结论：执行态的任何用户输入都被拦截。

### 2.2 后端当前模型

- `chat_app_server_rs/src/api/chat_v3.rs`
  - 当前只有 `stream` 和 `stop` 两类控制。
- `chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
  - 主循环只检查 `abort_registry`，没有“非中断引导队列”的消费点。

结论：运行时只有“继续执行”或“中断执行”两个状态，没有“温和引导”通道。

---

## 3. 设计原则

1. **双通道**：把“正常消息”与“运行时引导”分离。
2. **非抢占**：引导不打断正在进行的单次工具调用，只在安全点注入。
3. **可追踪**：每条引导有 `id/时间/状态`，可查看是否已应用。
4. **最小侵入**：优先复用现有 SSE、store、history 展示能力。
5. **可降级**：开关化，异常时可退回现状（执行态禁输入）。

---

## 4. 推荐方案（MVP）

## 4.1 新增运行时引导队列（服务端）

新增服务：`chat_app_server_rs/src/services/runtime_guidance_manager/`

核心结构（按 `session_id + turn_id` 维度）：

- `RuntimeGuidanceItem`
  - `guidance_id`
  - `session_id`
  - `turn_id`
  - `content`
  - `status`（`queued | applied | dropped`）
  - `created_at` / `applied_at`
- `ActiveTurnState`
  - `queue: VecDeque<RuntimeGuidanceItem>`
  - `max_queue_size`（建议 20）

核心接口：

- `register_active_turn(session_id, turn_id)`
- `enqueue_guidance(session_id, turn_id, content) -> guidance_id`
- `drain_guidance(session_id, turn_id, limit) -> Vec<RuntimeGuidanceItem>`
- `mark_applied(...)`
- `close_turn(session_id, turn_id)`

说明：

- MVP 先以内存队列为主。
- 同时把引导记录写入消息表（`role=user`, `message_mode=runtime_guidance`）做审计与可恢复展示。

## 4.2 新增引导提交 API

在 `chat_v3` 增加：

- `POST /api/agent_v3/chat/guide`

请求体：

```json
{
  "session_id": "xxx",
  "turn_id": "turn_xxx",
  "content": "先优先修复登录失败，不要先做UI优化"
}
```

返回：

```json
{
  "success": true,
  "guidance_id": "gd_xxx",
  "status": "queued"
}
```

校验规则：

- `session_id/turn_id/content` 必填。
- `content` 建议上限 500~1000 字符。
- 非 active turn 可返回 `409 turn_not_running`（前端提示“本轮已结束”）。

## 4.3 执行循环注入点（不打断）

在 `execution_loop.rs` 增加“安全注入点”：

1. 每轮模型请求前（最关键）：
  - `drain_guidance(session_id, turn_id)`。
  - 将引导转成系统约束片段，追加到本轮 `input`（或 `prefixed_input_items`）。
2. 每轮工具结果回收后、下一轮请求前：
  - 再次 drain，保证工具执行期间提交的引导能尽快生效。

注入模板建议：

```text
[Runtime Guidance]
- time: 2026-03-25T...
- source: user guidance during running turn
- instruction: <guidance content>
- rule: treat this as high-priority preference unless conflicts with safety
```

这样保证：

- 当前正在跑的步骤不被打断。
- 后续步骤能吸收用户新意图。

## 4.4 SSE 回执事件（可观测）

新增事件类型（建议）：

- `runtime_guidance_queued`
- `runtime_guidance_applied`

前端可在 Workbar 上展示：

- “引导待应用 2”
- “已应用 5”

---

## 5. 前端改造方案

## 5.1 输入区改为“执行态可引导”

目标行为：

- 非执行态：按钮语义 = `发送消息`（现状）。
- 执行态：按钮语义 = `发送引导`（新）。

建议改造：

- `ChatInterface.tsx`
  - `inputDisabled` 从“执行态禁用”改为“仅 stopping 或无会话时禁用”。
  - 传入当前 `activeTurnId` 和 `onGuide` 回调。
- `InputArea.tsx`
  - 新增 `mode: normal | guiding` 或根据 `isStreaming` 自动切换。
  - 执行态下点击发送调用 `onGuide(message)`，不触发 `onSend`。
  - 引导模式禁附件（文本即可），降低复杂度。

## 5.2 Store 与 API

新增 action：

- `submitRuntimeGuidance(content, { sessionId, turnId })`

新增状态：

- `sessionRuntimeGuidanceState[sessionId]`
  - `pendingCount`
  - `appliedCount`
  - `lastGuidanceAt`
  - `lastAppliedAt`

API 客户端新增：

- `apiClient.submitRuntimeGuidance(...)`

## 5.3 展示位置

建议优先放在 `TaskWorkbar` 附近：

- 执行态显示小状态条：
  - `引导待应用: N`
  - `最近应用: xx:xx:xx`

---

## 6. 与现有 UI Prompt 的关系

现有 `ui_prompt` 是“工具主动发问、等待用户确认（阻塞）”。

本方案是“用户主动补充偏好、不阻塞执行（非阻塞）”。

两者关系：

- 机制层可复用（存储、事件、面板容器、history）。
- 语义层必须分离（`ui_prompt` != `runtime_guidance`）。

---

## 7. 分阶段落地

### Phase 1（MVP，建议先做）

1. 后端 `runtime_guidance_manager`（内存队列）。
2. 新增 `POST /api/agent_v3/chat/guide`。
3. `execution_loop` 在“每轮请求前”注入引导。
4. 前端执行态发送“引导”而非“新消息”。
5. Workbar 显示简单计数。

### Phase 2（增强）

1. 引导入库检索（按 turn 历史）。
2. SSE 事件细化（queued/applied/dropped）。
3. 引导优先级与去重策略（例如相似内容覆盖）。

### Phase 3（可选）

1. 支持 v2 对齐。
2. 引导模板化（“优先级/范围/约束”结构化输入）。

---

## 8. 验收标准

1. 执行中输入框可提交引导，不会触发 stop。
2. 提交引导后，当前流不中断，后续工具/回复体现新引导意图。
3. 引导过长、空内容、turn 已结束时有明确错误提示。
4. 并发提交多条引导时顺序可控（FIFO）。
5. 强制 stop 后，不再接受当前 turn 的新引导。

---

## 9. 风险与规避

1. 风险：引导注入过频导致 prompt 膨胀。
  - 规避：每轮最多消费 N 条；超量合并摘要。
2. 风险：用户连续改口导致策略抖动。
  - 规避：后写优先；追加“latest guidance wins”说明。
3. 风险：引导与安全策略冲突。
  - 规避：系统模板中明确“安全策略优先于引导”。

---

## 10. 默认假设

1. 当前优先支持 v3（`/api/agent_v3/chat/stream`）链路。
2. 引导是纯文本，不带附件。
3. 引导不立即强制重算当前正在执行的工具调用，只影响后续步骤。

