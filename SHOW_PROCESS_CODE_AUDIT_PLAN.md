# Show Process 代码审计后方案（基于当前前后端实现）

> 这是基于**当前仓库代码**给出的方案，不是泛化建议。先列结论，再给分阶段落地路径。

## 1) 代码审计结论（当前真实问题）

### 问题 A：前端请求 process 仍以 `user_message_id` 为主键

- 前端接口：`chat_app/src/lib/api/client.ts:119`-`chat_app/src/lib/api/client.ts:121`
- 后端路由：`chat_app_server_rs/src/api/sessions.rs:90`
- 后端查询实现：`chat_app_server_rs/src/api/sessions.rs:745`-`chat_app_server_rs/src/api/sessions.rs:763`

现状是只要传入的不是库里的真实 user message id（例如前端 `temp_user_*`），就必然 404：
`"user message not found in session"`。

---

### 问题 B：前端流式阶段使用临时 user id，且 assistant 绑定到临时 user id

- 临时 user id 创建：`chat_app/src/lib/store/actions/sendMessage.ts:260`
- assistant 绑定：`chat_app/src/lib/store/actions/sendMessage.ts:343`
- 切会话恢复时仅恢复 assistant 草稿，不保证 user 草稿恢复：`chat_app/src/lib/store/actions/sessions.ts:232`-`chat_app/src/lib/store/actions/sessions.ts:255`

结果：
- 会话重建后可能出现“assistant 在，user 不在”；
- `Show process` 的按钮入口在 user 消息卡片上，user 不在就直接没按钮。

---

### 问题 C：按钮显示与请求触发都耦合 `user_message_id`，且 `loaded` 过早为 true

- sendMessage 初始就把该轮 `loaded=true`：
  - `chat_app/src/lib/store/actions/sendMessage.ts:288`
  - `chat_app/src/lib/store/actions/sendMessage.ts:309`
  - `chat_app/src/lib/store/actions/sendMessage.ts:539`
- ChatInterface 里 `turnState.loaded` 直接短路，不再请求：
  - `chat_app/src/components/ChatInterface.tsx:879`
- toggle 只在 `!loaded` 时请求：
  - `chat_app/src/lib/store/actions/messages.ts:146`

这会导致“状态显示已加载，但实际没有 process 数据”，面板长期空白。

---

### 问题 D：后端虽然给 user 保存了 `conversation_turn_id`，但 assistant/tool 元数据未统一携带 turn_id

- user metadata 含 turn_id：`chat_app_server_rs/src/services/ai_common.rs:20`-`chat_app_server_rs/src/services/ai_common.rs:41`
- assistant metadata 构建不含 turn_id：`chat_app_server_rs/src/services/ai_common.rs:156`-`chat_app_server_rs/src/services/ai_common.rs:173`
- tool metadata 构建不含 turn_id：`chat_app_server_rs/src/services/ai_common.rs:303`-`chat_app_server_rs/src/services/ai_common.rs:308`

因此系统无法稳定做到“按 turn_id 直接聚合整轮过程”，只能反复靠 user id 和区间推断。

---

## 2) 治理目标（明确终态）

1. process 查询主键从 `user_message_id` 迁移为 `conversation_turn_id`；
2. 前端过程状态/cache 主键从 `userMessageId` 迁移为 `turnId`；
3. streaming 场景下切会话，不再依赖“临时 id 迁移”；
4. 历史会话与运行中会话，`Show process` 行为一致。

---

## 3) 分阶段落地（可灰度、可回滚）

## 阶段 1（止血，低风险）

### 1.1 后端新增 turn 维度接口（保留旧接口）

新增：
- `GET /api/sessions/:session_id/turns/by-turn/:turn_id/process`

逻辑：
1. 先按 `user.metadata.conversation_turn_id == turn_id` 找 user；
2. 找不到时返回空数组（不要 404），避免前端直接报错；
3. 找到后沿用现有区间提取逻辑返回 process 消息。

旧接口 `/turns/:user_message_id/process` 保留，但内部复用新逻辑（先映射到 turn_id）。

---

### 1.2 前端 process 请求改为“优先 turn_id”

改动点：
- 从 user 消息 metadata 读取 `conversation_turn_id`；
- 有 turn_id 就调新接口；
- 无 turn_id 才降级旧接口。

同时把“process 查询失败”降级为可重试状态，不要写死 `loaded=true`。

---

### 1.3 修正 loaded 语义

- sendMessage 初始化 `loaded=false`；
- 只有拿到非空 process 或明确历史 inline process 时才置 `loaded=true`；
- streaming 且本次返回空时保持 `loaded=false`，允许下次重试。

---

## 阶段 2（结构修复，彻底去 user_id 耦合）

### 2.1 后端统一写入 turn_id 到 assistant/tool metadata

改造：
- `build_assistant_message_metadata(...)` 增加 `turn_id` 入参并写入 `conversation_turn_id`；
- `build_tool_result_metadata(...)` 增加 `turn_id`（或在保存 tool message 时补 metadata）。

这样 process 能直接按 turn 聚合，不再依赖“某条 user 消息存在且可定位”。

---

### 2.2 前端 store 主键迁移为 turnId

将以下结构从 `userMessageId` 改为 `turnId`：
- `sessionTurnProcessState`
- `sessionTurnProcessCache`
- `activeTurnProcess...`

UI 上 user message 仅作为“入口卡片”，不作为 process 主键。

---

### 2.3 切会话恢复改为按 turn 合并

切回会话时：
1. 先恢复该 session 的 streaming assistant 草稿；
2. 用草稿里的 `turn_id` 找对应 user（真实/临时均可）；
3. 按 turn 聚合过程面板内容。

不再做 temp_user_id 到 real_user_id 的复杂 key 迁移。

---

## 阶段 3（清理与收口）

1. 下线旧的 user_message_id process 接口（先灰度）；
2. 删除前端围绕 `historyFinalForUserMessageId` 的兼容分支；
3. 补充自动化测试：
   - 流式中切会话/切 terminal；
   - 历史回放；
   - 并发会话不串线；
   - 空过程重试。

---

## 4) 回滚策略

加 feature flag：`process_by_turn_id`。

- 开启：走新 turn 方案；
- 关闭：回到旧 user_message_id 方案。

任何线上异常都可秒级回滚，不阻塞主流程。

---

## 5) 建议的实施顺序（按天）

Day 1:
- 后端新接口 + 前端优先 turn_id 请求 + loaded 修正。

Day 2:
- assistant/tool turn_id 持久化 + 前端 byTurnId store 改造。

Day 3:
- 回归测试 + 灰度 + 清理兼容分支。

