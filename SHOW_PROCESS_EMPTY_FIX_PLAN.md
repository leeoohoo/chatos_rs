# Show Process 为空问题修复方案（会话/终端切换后）

## 1) 问题现象

在会话正在执行（流式返回中）时：

1. 切到其他会话或终端页面；
2. 再回到该会话点击 **Show process**；
3. 右侧过程面板出现“暂无可展示的过程内容”，但对话仍在继续执行。

## 2) 代码排查结论（根因）

### 根因 A：前端临时 user id 与后端真实 user id 不一致，导致过程关联断裂

- 发送消息时前端先创建 `temp_user_*`，并把临时 assistant 的
  `historyFinalForUserMessageId` 指向这个临时 id：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage.ts:260`
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage.ts:343`
- 切换会话后会重新从后端拉消息，后端返回的是**真实 user message id**；
  同时前端会把 streaming draft 重新塞回消息列表：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sessions.ts:214`
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sessions.ts:229`
- 但 draft 里的 `historyFinalForUserMessageId` 仍是临时 id，未重绑到真实 id，
  导致过程面板 fallback 查找失败：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/TurnProcessDrawer.tsx:164`

### 根因 B：`loaded` 状态过早置 true，且空结果会被缓存为“已加载”

- 发送时就把该轮 `sessionTurnProcessState[userMessageId].loaded` 设为 `true`：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage.ts:307`
- `toggleTurnProcess` 在 `loaded=true` 时不会再请求后端过程数据：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/messages.ts:146`
- 而后端 `turn process` 接口只返回已落库的中间消息；执行中的一段时间可能确实为空：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/sessions.rs:706`
  - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/sessions.rs:733`
- 结果：一次空加载后，前端后续不再刷新，面板持续空白。

## 3) 修复目标

1. 会话切回后，streaming draft 与真实 user message 重新正确关联；
2. “空过程”不再被永久缓存为已加载；
3. 在执行中（isStreaming=true）时，Show process 能持续看到过程内容（fallback 或后端过程消息）。

## 4) 修复方案（分阶段）

## 阶段 1：ID 重绑（必须）

在 `selectSession` 里，恢复 streaming draft 时新增“按 turn_id 重绑”：

- 读取 draft 的 `metadata.conversation_turn_id`；
- 在刚拉回的 `nextMessages` 中找到同 turn 的 user message（真实 id）；
- 将 restored draft 的：
  - `historyFinalForUserMessageId` 改为真实 user id；
  - （可选）`historyProcessUserMessageId`、`historyProcessExpanded` 同步修正；
- 同步迁移 `sessionTurnProcessState/cache` 的 key（临时 user id -> 真实 user id）。

涉及文件：

- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sessions.ts`

## 阶段 2：loaded 语义修正（必须）

### 2.1 发送初始状态改为“未加载”

将 sendMessage 初始化：

- `sessionTurnProcessState[currentSessionId][userMessage.id].loaded` 从 `true` 改为 `false`；
- `historyProcess.loaded` 同步改为 `false`。

涉及文件：

- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage.ts`

### 2.2 `toggleTurnProcess` 增加“已加载但空数据”的二次拉取

新增 helper：`hasTurnProcessInMemory(messages, userMessageId)`，判断：

- 是否已有 `historyProcessUserMessageId === userMessageId` 的过程消息；
- 或是否存在 `historyFinalForUserMessageId === userMessageId` 且含 thinking/tool_call 的 assistant fallback。

若 `loaded=true` 但内存中无过程内容，则允许重新请求 `getSessionTurnProcessMessages`。

涉及文件：

- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/messages.ts`
- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/TurnProcessDrawer.tsx`（仅用于统一判定逻辑时可抽 helper）

## 阶段 3：执行中空结果不固化（建议）

当 `isStreaming=true` 且后端返回 `processMessages.length === 0` 时：

- 不将该轮标记为“loaded=true”；
- 设为 `loaded=false, loading=false, expanded=true`，允许后续重试/自动刷新。

涉及文件：

- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/messages.ts`
- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sessions.ts`（读取会话 streaming 状态）

## 阶段 4：终端页返回体验修正（建议）

`selectSession` 当前在“同会话且 streaming”时直接 return：

- `/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sessions.ts:202`

建议改为：

- 保留“避免重复拉取”的逻辑；
- 但仍设置 `activePanel='chat'`，确保从 terminal 返回会话时 UI 行为稳定。

## 5) 验收用例（必须覆盖）

1. **会话切换回流式会话**
   - A 会话执行中 -> 切到 B -> 回 A -> Show process 可见。
2. **终端页往返**
   - A 会话执行中 -> terminal -> 回 A -> Show process 可见。
3. **首次点击时后端仍无过程消息**
   - 第一次点击可能空；后续工具开始后再次点击/自动刷新应出现内容。
4. **已完成会话回放**
   - 历史会话 Show process 行为不回归（可折叠、可复开）。
5. **并发安全**
   - 多会话并发流式时，A/B 会话过程不串线。

## 6) 风险与注意点

1. 以 `conversation_turn_id` 做重绑时要防止同 turn 多消息误配（取该 turn 最新 user）。
2. 迁移 `sessionTurnProcessState/cache` key 时需原子更新，避免状态残留。
3. 不能破坏现有 compact 历史结构（`historyFinalForUserMessageId` / `historyProcessUserMessageId`）。

## 7) 交付顺序

1. 先做阶段 1 + 2（核心修复）；
2. 再做阶段 3（提升执行中稳定性）；
3. 最后做阶段 4（交互体验修正）。

