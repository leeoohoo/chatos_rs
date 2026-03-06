# UI Prompt MCP 内化方案（仅 Agent 流程）

## 1. 结论先说

你这个需求**不建议直接复用 Task 的领域模型**（TaskReviewHub / TaskDraftPanel）来承载。

原因：
- Task 是“任务草稿确认”单一场景；
- UI Prompt 是“人机交互输入”通用场景，数据形态包含：
  - 仅表单（kv）
  - 仅选择（single/multi choice）
  - 表单 + 选择混合（mixed）
- 如果硬塞到 task 体系，会让 Task 语义被污染，后续可维护性会快速下降。

**推荐做法**：
- 复用“Task 这套机制”里的**交互模式**（SSE 事件 + Hub 等待 + 前端面板 + 审批提交）
- 但新建独立域：`ui_prompt_manager`（数据、事件、API、前端状态都独立）

---

## 2. 本次范围（按你要求，仅 Agent）

### In Scope
- 仅接入 Agent 聊天流：
  - v2 agent: `/api/agents/chat/stream`
  - v3 agent: `/api/agent_v3/agents/chat/stream`
- 新增内置 MCP：`builtin_ui_prompter`
- 支持三类交互：
  1. `prompt_key_values`
  2. `prompt_choices`
  3. `prompt_mixed_form`（同一次弹窗同时含输入和选择）
- 前端在聊天页展示“待确认输入面板”，用户提交后继续 tool 调用

### Out of Scope
- 非 Agent 的 model 直聊（v2/v3 model chat）
- 独立 admin server（不再单独起 tcp admin-ui）

---

## 3. 目标能力

1. Agent 在 tool 调用阶段触发 UI prompt
2. 前端实时弹出交互面板
3. 用户提交/取消/超时后，tool 继续并返回结果给模型
4. 支持并发多条 prompt（按 session 排队）
5. 支持刷新恢复（pending 列表恢复）
6. `secret` 字段不落明文日志与持久化

---

## 4. 总体架构

## 4.1 后端组件（新增）
- `services/ui_prompt_manager/`
  - `types.rs`：Prompt/Response 结构、状态枚举
  - `normalizer.rs`：参数校验、mixed 规范化
  - `hub.rs`：内存等待与唤醒（类似 review_hub，但独立）
  - `store.rs`：DB 持久化（sqlite + mongo）
- `builtin/ui_prompter/mod.rs`
  - 暴露 3 个 MCP tools
  - 发出 `ui_prompt_required` 流式事件
  - 阻塞等待 hub 决策（confirm/cancel/timeout）

## 4.2 路由（新增）
- `api/ui_prompts.rs`
  - `GET /api/ui-prompts/pending?session_id=...`
  - `POST /api/ui-prompts/:prompt_id/respond`

并入 `api/mod.rs` protected api。

## 4.3 前端组件（新增）
- `chat_app/src/components/UiPromptPanel.tsx`
  - 渲染 kv / choice / mixed
- store 增加
  - `uiPromptPanelsBySession`
  - `upsertUiPromptPanel/removeUiPromptPanel`
- SSE 解析 `tools_stream` 中的 `ui_prompt_required`

---

## 5. 为什么不和 task 共用“一个”

可以共用的层：
- 事件总线（`tools_stream`）
- “发事件 -> 前端面板 -> API回传 -> 唤醒等待”的控制流

不该共用的层：
- 数据结构（TaskDraft vs PromptPayload）
- API 语义（task review decision vs prompt response）
- UI 组件（任务编辑器 vs 表单/选择器）

**结论**：
- 复用模式，不复用领域对象。
- 这样不会影响现有 task 稳定性，也利于后续扩展更多输入控件。

---

## 6. Prompt 数据模型（支持混合）

建议统一主结构：

```json
{
  "prompt_id": "up_...",
  "session_id": "...",
  "conversation_turn_id": "...",
  "tool_call_id": "...",
  "kind": "kv|choice|mixed",
  "title": "...",
  "message": "...",
  "allow_cancel": true,
  "timeout_ms": 120000,
  "payload": {
    "fields": [],
    "choice": {},
    "sections": []
  }
}
```

### mixed 形态
`sections` 按顺序渲染，每段 `type` 可为：
- `text_input`
- `textarea`
- `single_choice`
- `multi_choice`

返回统一为：

```json
{
  "status": "ok|canceled|timeout",
  "values": {
    "field_a": "...",
    "pick_b": ["x", "y"]
  }
}
```

---

## 7. 事件设计

新增事件名（放到 `utils/events.rs`）：
- `UI_PROMPT_REQUIRED = "ui_prompt_required"`
- `UI_PROMPT_RESOLVED = "ui_prompt_resolved"`（可选）

`tools_stream` data.content 内传递 JSON 字符串（沿用现有 task 流式事件做法）：

```json
{
  "event": "ui_prompt_required",
  "data": { ...prompt_payload... }
}
```

---

## 8. 存储设计

### SQLite 新表：`ui_prompt_requests`
字段建议：
- `id` (prompt_id)
- `session_id`
- `conversation_turn_id`
- `tool_call_id`
- `kind`
- `status` (`pending|ok|canceled|timeout`)
- `prompt_json`（脱敏）
- `response_json`（脱敏）
- `expires_at`
- `created_at`
- `updated_at`

索引：
- `(session_id, status, updated_at desc)`
- `(conversation_turn_id)`

Mongo 同步 collection：`ui_prompt_requests`。

---

## 9. secret 字段策略

- Prompt 定义中标记 `secret=true` 的输入项：
  - 持久化只存 `******`
  - 日志只打掩码
- 内存中保留明文仅用于当次 tool 返回
- tool 结果落库（消息表）时，默认也做脱敏版本（避免历史泄露）

> 若后续业务必须把明文再次给模型，可加开关，但默认不建议。

---

## 10. 后端改造点（文件级）

1. `src/services/builtin_mcp.rs`
- 增加 `builtin_ui_prompter` id/command/display name
- 纳入 `BuiltinMcpKind`

2. `src/core/mcp_tools.rs`
- `BuiltinToolService` 增加 `UiPrompter(...)`
- `build_builtin_tool_service` 分支新增

3. `src/builtin/mod.rs`
- 导出 `ui_prompter`

4. `src/services/mod.rs`
- 导出 `ui_prompt_manager`

5. 新增 `src/services/ui_prompt_manager/*`
- hub/store/types/normalizer

6. 新增 `src/builtin/ui_prompter/mod.rs`
- 三个 tool + wait/resolve

7. `src/utils/events.rs`
- 新增 `UI_PROMPT_REQUIRED/UI_PROMPT_RESOLVED`

8. `src/api/mod.rs`
- merge `ui_prompts::router()`

9. 新增 `src/api/ui_prompts.rs`
- pending + respond API（含鉴权 session ownership）

10. `src/db/sqlite.rs`
- 建表 `ui_prompt_requests`

11. （可选）`sub_agent_router` 流回传处补 ui_prompt 事件透传

---

## 11. 前端改造点（文件级）

1. `chat_app/src/lib/store/types.ts`
- 新增 `UiPromptPanelState`
- 新增 `uiPromptPanelsBySession`

2. `chat_app/src/lib/store/createChatStoreWithBackend.ts`
- 新增 upsert/remove action

3. `chat_app/src/lib/store/actions/sendMessage.ts`
- 在 `tools_stream` 分支解析 `ui_prompt_required`
- 入队到 `uiPromptPanelsBySession[sessionId]`

4. 新增 `chat_app/src/components/UiPromptPanel.tsx`
- 动态渲染 kv/choice/mixed
- 提交/取消按钮

5. `chat_app/src/components/ChatInterface.tsx`
- 在输入框上方挂载 `UiPromptPanel`
- 处理 confirm/cancel API 调用

6. `chat_app/src/lib/api/client.ts`
- 新增：
  - `getPendingUiPrompts(sessionId)`
  - `submitUiPromptResponse(promptId, payload)`

7. `chat_app/src/components/SessionList.tsx`
- 会话 pending 标识合并 task + ui_prompt 数量

---

## 12. 交互时序

1. Agent 触发 `builtin_ui_prompter_prompt_*`
2. 后端创建 pending 记录 + hub 注册
3. 后端通过 `tools_stream` 发 `ui_prompt_required`
4. 前端弹面板
5. 用户提交 -> `POST /api/ui-prompts/:id/respond`
6. hub resolve，tool 继续执行
7. `tools_end` 返回最终结果，面板关闭

---

## 13. 分阶段落地建议

### Phase 1（MVP，2~3 天）
- kv + choice
- pending 恢复
- agent v2/v3 流程打通

### Phase 2（1~2 天）
- mixed_form 一次多控件
- 更细粒度校验（min/max/regex）

### Phase 3（可选）
- rich controls（date/time/file/path picker）
- 审计追踪与导出

---

## 14. 验收清单

- Agent 使用 MCP 时，能弹出 kv prompt 并继续执行
- Agent 使用 MCP 时，能弹出 choice prompt 并继续执行
- mixed prompt 可同时提交输入与选择
- cancel/timeout 行为正确
- secret 字段不出现在 DB 明文和日志
- 页面刷新后 pending 仍可恢复并提交

---

## 15. 风险与规避

- 风险：多 prompt 并发导致面板错位
  - 规避：以 `prompt_id + session_id + tool_call_id` 唯一定位
- 风险：用户长期不处理导致 tool 阻塞
  - 规避：强制 timeout + 超时自动 `timeout`
- 风险：敏感信息泄露
  - 规避：统一脱敏管线，默认不持久化明文

