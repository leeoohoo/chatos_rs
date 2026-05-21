# 动态任务看板与回合 Review 续跑方案

## 现状

- 看板当前主要在 `chat_app/src/components/TaskWorkbar.tsx`、`chat_app/src/components/taskWorkbar/TaskHistoryDrawer.tsx` 展示。
- 任务状态目前只有 `todo / doing / blocked / done`，但前端历史抽屉只做了“全部 / 已处理”两类展示，没有明确的“未完成”视图。
- 后端任务看板文案在 `chat_app_server_rs/src/services/task_board_prompt.rs`，当前只输出 `current / blocked / completed`，没有单独的未完成任务区。
- AI 回合执行主链路在 `chat_app_server_rs/src/modules/conversation_runtime/chat_execution.rs`、`chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`、`openai-codex-gateway/gateway_runtime/turn_loop.py`。

## 目标

1. 看板增加“未完成任务”区，明确展示 `todo + doing`，同时保留 `blocked`、`done` 和 `current`。
2. AI 在输出总结后，不直接结束，而是进入任务收敛检查：
   - 若仍有未完成任务且不是 blocked，则自动续上一轮，继续完成任务。
   - 若任务都完成，则在当前轮额外发起 review，会话检查“是否真的完成”。

## 实施路径

### 1. 看板分组重构

- 前端把任务分成 4 组：`current / unfinished / blocked / done`。
- `unfinished` 定义为 `todo + doing`，排除 `blocked` 和 `done`。
- `TaskWorkbar.tsx` 继续保留当前轮任务条，但新增“未完成”概览区，避免只能看到历史和完成结果。
- `TaskHistoryDrawer.tsx` 新增独立筛选项，至少补一个“未完成”标签；默认展示全部时也要能看见未完成任务。
- `taskWorkbar/helpers.ts` 增加分组工具函数，避免组件里散落状态判断。
- `chat_app/src/components/taskWorkbar/types.ts` 同步扩展过滤类型，避免 UI 分组和类型定义脱节。

### 2. 看板 prompt 调整

- 在 `chat_app_server_rs/src/services/task_board_prompt.rs` 增加 `unfinished` 段。
- 展示顺序建议为：
  - current
  - unfinished
  - blocked
  - completed
- 对 AI 的行为约束改成：
  - 看到 `unfinished` 时优先继续清理未完成项。
  - `blocked` 只记录阻塞原因，不要把它误当成需要立刻完成的任务。
  - `done` 仅作为已完成结果和复用上下文。

### 3. 任务收敛检查器

新增一个“turn completion checker”，放在后端回合结束后的收口层，而不是前端。

实现位置：
- `chat_app_server_rs/src/modules/conversation_runtime/chat_execution.rs`
- `chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
- 同步到 `v2` 链路，保证两套模型路径一致。

检查流程：
1. AI 返回总结后，读取当前 turn 的任务板快照。
2. 计算是否存在未完成任务：
   - `todo` 或 `doing` 视为未完成。
   - `blocked` 不算失败项，但要单独保留。
3. 若存在未完成项：
   - 在**当前轮对话**里追加一段 `task_review_followup` 系统消息。
   - 复用同一 session / thread / turn 上下文，并把下一次模型请求串到上一条响应的 `previous_response_id` 上，继续当前轮，不新开用户轮次，也不新开会话。
   - 让 AI 直接回复“继续执行”，并把剩余未完成项作为当前轮续跑目标。
4. 若不存在未完成项：
   - 在**当前轮对话**里追加一段 `task_review_check` 系统消息。
   - 同样沿用上一条响应的 `previous_response_id`，保证 review 仍然挂在当前轮链路里。
   - 让 AI 在同一 turn 内执行 review，检查“是否真的完成，是否漏改、漏验、漏说明”。
   - review 结果若发现问题，直接回到续跑分支，并继续留在当前轮。
5. 每次续跑或 review 结束后，都重新拉取最新任务列表，再次计算 `unfinished`，直到任务收敛或达到上限。

### 3.1 当前轮对话写回方式

- `task_review_followup` 和 `task_review_check` 都作为当前 turn 的后置系统上下文注入。
- 它们进入同一条消息链路，和原始总结处在同一轮上下文里。
- 不创建新的 conversation，不创建新的 top-level turn，不把 review 单独落成一轮独立对话。
- 当前轮的最终 assistant 消息先承载总结，再承载 review 结果或补跑结果，直到任务收敛。
- 这一串 follow-up/review 请求全部复用同一条响应链，靠 `previous_response_id` 维持上下文连续性。

### 3.2 前端执行中状态

- 进入 `task_review_followup` 或 `task_review_check` 后，前端继续保持“执行中”。
- 前端不切换到完成态，不关闭当前轮进度展示，不把 review 当成新的空闲轮次。
- 只有当 review 返回 `pass` 且未完成任务清空后，前端才切换到完成态。
- 如果 review 触发续跑，前端继续沿用同一条执行中的状态，直到最后一次 review 结束。
- 这个状态由 `useChatInterfaceController.ts` / `useConversationPaneProps.ts` 继续向下传递，`inputDisabled`、`isStreaming`、`chatIsStopping` 和任务面板状态都要保持一致。

### 4. review 子轮协议

新增一个简单的内部协议，避免模型自由发挥：

- 输入：
  - 当前任务板快照
  - 当前轮 AI 总结
  - 当前轮关键变更/工具结果摘要
- 输出：
  - `pass`
  - `needs_more_work`
  - `needs_human_confirmation`

执行规则：
- `needs_more_work` 直接触发续跑。
- `pass` 才允许当前轮收束。
- review 结果必须写回当前 turn 的过程记录，而不是单独生成一个新的会话记录。
- review 阶段使用同一条对话链路的后续 assistant 消息完成，用户看到的是当前轮内部的补跑和复审过程。
- `needs_more_work` 与 review 触发的续跑需要有上限，超过上限后转为 `needs_human_confirmation`，避免无限补跑。

## 实现切点

- UI：
  - `chat_app/src/components/TaskWorkbar.tsx`
  - `chat_app/src/components/taskWorkbar/TaskHistoryDrawer.tsx`
  - `chat_app/src/components/taskWorkbar/helpers.ts`
  - `chat_app/src/components/chatInterface/workbarTransforms.ts`
- 后端任务板：
  - `chat_app_server_rs/src/services/task_board_prompt.rs`
  - `chat_app_server_rs/src/modules/conversation_runtime/task_board.rs`
- 回合控制：
  - `chat_app_server_rs/src/modules/conversation_runtime/chat_execution.rs`
  - `chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
  - `chat_app_server_rs/src/services/v2/ai_client/*`（如需要兼容）
  - `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
  - `chat_app/src/components/chatInterface/useConversationPaneProps.ts`

## 验收标准

1. 看板能明确看到 `未完成` 分组，且 `todo / doing` 不会再只混在 current/history 里。
2. AI 输出总结后，如果还有未完成任务，会自动继续同一轮，直到未完成项清空。
3. 若任务都已完成，会自动进入 review；review 发现问题会自动回滚到续跑。
4. `blocked` 任务不会导致死循环续跑，但会被持续保留和展示。

## 实施顺序

1. 先改任务分组与 prompt 输出。
2. 再加 turn 结束后的任务检查器。
3. 最后补 review 子轮和回归测试。
