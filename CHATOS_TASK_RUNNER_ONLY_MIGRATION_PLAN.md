# Chatos Task Runner Only 改造计划

## 1. 决策

本次改造采用硬切方案：

- Chatos 后续只保留 Task Runner 模式。
- 普通实时对话模式、普通 MCP 直连模式、为兼容旧模式保留的分支逻辑，全部删除。
- 不再考虑“同一套前后端同时兼容 task runner 和普通模式”。

这不是继续打补丁，而是把当前会话链路、消息模型、前端状态机一次性收敛成一条单轨流程。

## 2. 代码现状确认

我已经按当前代码看过主要分叉点，普通模式和 task runner 模式目前是混在一起跑的。

### 2.1 前端分叉点

核心文件：

- `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
- `chat_app/src/components/chatInterface/ChatConversationPane.tsx`
- `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`
- `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
- `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberWorkspaceProps.ts`
- `chat_app/src/lib/store/actions/sendMessage.ts`
- `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- `chat_app/src/lib/domain/messages.ts`
- `chat_app/src/components/messageList/derivedData.ts`

当前问题：

- 前端大量依赖 `isTaskRunnerAsyncContactMode` 做双分支渲染。
- `sendMessage.ts` 仍然会创建临时 assistant streaming draft。
- `useChatStreamRealtimeBridge.ts` 仍然保留实时流、断线恢复、snapshot recovery 这一整套普通模式状态机。
- 消息列表和消息语义层同时兼容普通 assistant、history process、task runner plan、task runner callback，导致渲染和状态判断很容易互相污染。
- 团队成员页和普通聊天页分别维护了一套相似但不完全一致的模式判断。

### 2.2 后端分叉点

核心文件：

- `chat_app_server_rs/src/modules/conversation_runtime/runtime_context.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/chat_execution.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/chat_runner.rs`
- `chat_app_server_rs/src/api/agent_chat.rs`
- `chat_app_server_rs/src/api/sessions/history_process.rs`
- `chat_app_server_rs/src/api/sessions/history_process_support.rs`

当前问题：

- `runtime_context.rs` 里通过 `task_runner_async_contact_mode` 决定到底走普通 MCP bundle 还是只挂 Task Runner。
- `chat_execution.rs` 里 `message_mode` 仍然在 `task_runner_async_plan` 和 `model` 之间切换。
- `chat_execution.rs` 里的 task board runtime 在普通模式下继续生效。
- `chat_runner.rs` 虽然对 task runner 模式关闭了工具流回调，但整体执行骨架还是为“双模式共存”设计的。
- `agent_chat.rs` 同时承载普通对话链路和 task runner callback 补消息链路。
- `history_process` 相关接口和归一化逻辑，仍在兼容普通工具过程展示。

## 3. 目标态

最终只保留这一条链路：

1. 用户发送消息。
2. Chatos 后端调用模型。
3. 模型只能使用 Task Runner 提供的任务工具。
4. 模型创建完任务后立即给出规划总结。
5. Chatos 结束本轮模型调用，不继续给模型任何工具。
6. Task Runner 定时调度执行任务。
7. Task Runner 完成某个任务后回调 Chatos。
8. Chatos 把回调结果写进消息历史并推送给前端。

也就是说：

- Chatos 不再展示普通实时工具调用过程。
- Chatos 不再维护普通 streaming assistant 草稿流。
- Chatos 不再向联系人暴露除 Task Runner 之外的任何 MCP。
- 前端消息状态只围绕“待处理 / 正在处理 / 已处理”和任务回调结果展开。

## 4. 改造原则

### 4.1 单轨运行

- 所有会话发送入口统一走 task runner planner 流程。
- 不再按联系人、团队成员、页面位置去区分“这个入口是不是普通模式”。

### 4.2 程序透传与模型可见字段分离

- workspace、remote server、source session id、source turn id、source user message id、服务鉴权信息，仍由程序透传。
- 这些程序内部透传信息不写进 skill，不暴露成 AI 需要理解的参数。

### 4.3 前端只认持久化消息

- 用户消息可先本地 optimistic 展示。
- assistant 侧不再维护普通 streaming draft。
- assistant 的计划总结和任务完成结果，以后都以“后端持久化后的正式消息”为准。

## 5. 删除范围

### 5.1 前端要删除的普通模式逻辑

1. 删除 `isTaskRunnerAsyncContactMode` 这一整条模式判断链。
2. 删除普通模式下的 streaming / stopping / thinking / tool timeline UI 分支。
3. 删除普通模式的 turn process viewer、history process summary、legacy task panels 开关逻辑。
4. 删除普通模式下的 `onGuide`、`onStop`、工具流相关展示行为。
5. 删除 `sendMessage.ts` 中为普通流式回复准备的临时 assistant draft 机制。
6. 删除 `useChatStreamRealtimeBridge.ts` 中围绕普通流式回复的 completion、disconnect recovery、snapshot recovery 逻辑。
7. 删除消息语义层里为普通 assistant + tool trace 做的兼容归类，只保留 task runner 规划消息、task runner 回调消息、普通用户消息三类可见语义。

### 5.2 后端要删除的普通模式逻辑

1. 删除联系人/会话运行时里“普通 MCP bundle”和“Task Runner MCP bundle”二选一的分支。
2. 删除普通 MCP 服务器注入：
   - `load_mcp_servers_by_selection(...)`
   - `contact_agent_skill_reader_server(...)`
   - `contact_agent_command_reader_server(...)`
   - `contact_agent_plugin_reader_server(...)`
3. 删除 `message_mode = "model"` 这类普通对话模式分支，统一只保留 task runner 规划消息模式和 task runner callback 消息模式。
4. 删除普通工具流事件输出：
   - `chat.tool.started`
   - `chat.tool.delta`
   - `chat.tool.completed`
   - 普通 thinking/tool chunk 展示链路
5. 删除普通 task board prompt 和其对应刷新上下文。
6. 删除普通模式下的工具过程持久化和展示兼容代码。

## 6. 具体实施方案

### 6.1 后端先收口成单一运行时

目标文件：

- `chat_app_server_rs/src/modules/conversation_runtime/runtime_context.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/chat_execution.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/chat_runner.rs`
- `chat_app_server_rs/src/api/agent_chat.rs`

实施内容：

1. `ResolvedConversationRuntimeContext` 去掉 `task_runner_async_contact_mode` 这类模式标记。
2. 运行时默认只构建 Task Runner server，不再加载普通 MCP。
3. 若当前联系人没有 Task Runner 配置，直接失败返回，不再回落到普通聊天。
4. `chat_execution.rs` 里：
   - 删除 `build_task_board_runtime_context(...)` 相关普通路径。
   - `message_mode` 固定为 task runner 规划消息模式。
   - 保留 workspace prompt 注入。
5. `chat_runner.rs` 里：
   - 不再构造普通流式工具展示回调。
   - 不再向前端推送普通 chunk/tool 事件。
   - 仅保留消息创建、消息更新、任务回调相关的实时事件。
6. `agent_chat.rs` 里把发送消息主链路简化成：
   - 写 user message
   - 调模型创建任务
   - 写 assistant 规划总结
   - 更新 user message 的 task runner 状态
   - 结束本轮

### 6.2 前端消息链路改成“持久化消息驱动”

目标文件：

- `chat_app/src/lib/store/actions/sendMessage.ts`
- `chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts`
- `chat_app/src/components/chatInterface/useChatInterfaceModel.ts`
- `chat_app/src/components/chatInterface/ChatConversationPane.tsx`
- `chat_app/src/components/chatInterface/useChatInterfaceSessionResources.ts`

实施内容：

1. `sendMessage.ts`
   - 保留用户消息 optimistic 插入。
   - 去掉普通 assistant draft 创建。
   - 发送后仅维护该用户消息的 task runner 状态。
2. `useChatStreamRealtimeBridge.ts`
   - 保留 task runner callback 的正式消息 upsert。
   - 删除普通流式 completion、disconnect recovery、snapshot recovery。
3. `useChatInterfaceModel.ts`
   - 删除 `isTaskRunnerAsyncContactMode` 计算和下游传递。
   - 会话页只暴露 task runner 所需状态。
4. `ChatConversationPane.tsx`
   - 删除普通 streaming/loading/stopping 分支。
   - 删除 turn process 和普通 workbar 的展示开关。
   - 保留消息状态展示、任务按钮、任务抽屉。
5. `useChatInterfaceSessionResources.ts`
   - 删除只给普通模式使用的资源加载逻辑。
   - 保留任务抽屉、任务结果、必要的运行时配置读取。

### 6.3 团队成员页和聊天页统一成同一套模式

目标文件：

- `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
- `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberWorkspaceProps.ts`
- `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspace.tsx`
- `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspaceContent.tsx`
- `chat_app/src/components/projectExplorer/teamMembers/TeamMemberSummaryView.tsx`

实施内容：

1. 删除团队成员页里残留的普通模式显示分支。
2. 团队成员会话与主聊天会话使用同样的 task runner-only 消息状态模型。
3. 删除普通模式下才显示的控制项和过程视图。

### 6.4 消息语义层重定义

目标文件：

- `chat_app/src/lib/domain/messages.ts`
- `chat_app/src/components/messageList/derivedData.ts`
- `chat_app/src/components/messageItem/useMessageItemModel.ts`

保留的可见消息类型只剩：

1. `user`
2. `assistant` 的任务规划总结消息
3. `assistant` 的任务完成回调消息

对应约束：

- 不再把普通工具调用过程作为可见消息展示。
- 不再为普通流式 assistant 做补齐和去重兼容。
- 列表聚合规则只围绕：
  - 源 user message
  - 规划总结消息
  - 回调结果消息
  - 消息级任务状态

### 6.5 Session Runtime / 设置项收缩

要重新梳理哪些运行时设置在 task runner-only 下仍然有意义。

建议保留：

- planner 使用的模型选择
- workspace root
- remote connection
- 联系人对应的 task runner 凭证和开关

建议删除或停止透出：

- 只服务于普通 MCP 直连的开关
- 只服务于普通工具流展示的状态
- 只服务于普通 auto-create-task 兼容逻辑的字段

这里需要连带检查：

- `chat_app/src/features/sessionRuntime/useSessionRuntimeSettings.ts`
- `chat_app/src/lib/store/helpers/sessionRuntime.ts`
- `/conversations/:id/runtime-settings` 对应后端持久化逻辑

目标不是简单隐藏，而是把已经无意义的字段从交互和持久化上一起收掉。

### 6.6 History Process 相关代码收缩

目标文件：

- `chat_app_server_rs/src/api/sessions/history_process.rs`
- `chat_app_server_rs/src/api/sessions/history_process_support.rs`

处理原则：

- 如果任务系统方案下前端不再使用普通 history process 面板，就删除普通工具过程提取逻辑。
- 若仍需要用户消息上的状态展示，则只保留 task runner 状态归一化和必要的辅助字段补齐。

## 7. 实施顺序

建议按下面顺序做，避免前后端一起改时状态更乱：

1. 先改后端运行时，彻底禁止普通模式继续进入主链路。
2. 再改前端发送和实时桥接，去掉普通 streaming draft 机制。
3. 再删聊天页和团队成员页上的普通模式 UI 与状态分支。
4. 最后收缩消息语义层、history process 和 runtime settings。
5. 每一阶段都执行编译检查，确保删分支时不会漏引用。

## 8. 风险点

### 8.1 消息历史污染

以前普通模式留下来的 metadata、history process、tool call 字段，可能影响新的列表聚合和消息显隐。

应对：

- 统一重新定义 task runner-only 下的可见消息判定。
- 对旧字段只做兼容读取，不再继续生成新数据。

### 8.2 刷新后状态回退

当前“大刷新后状态回退”的根因，本质上就是前端本地状态和后端持久化状态双轨并存。

应对：

- 状态以消息 metadata 为准。
- 前端本地只做短暂 optimistic，不再自己维护一套普通 streaming 终态。

### 8.3 回调补消息后的历史连续性

Task Runner 回调写入 assistant 结果后，必须保证：

- 能进入 memory engine 历史
- 下轮请求模型时不会因为消息结构异常导致 400
- 前端刷新后仍能正常展示

### 8.4 设置页与实际运行不一致

如果页面还保留普通模式设置，但后端已经不消费，会继续制造误解。

应对：

- 页面、接口、后端消费逻辑同步收口。

## 9. 验收标准

完成后应满足：

1. Chatos 所有发送消息入口都只走 Task Runner 模式。
2. 模型侧看不到普通 MCP，只能看到 Task Runner 提供的任务工具。
3. 创建完任务后，当轮模型调用立即结束，不再继续普通工具流循环。
4. 前端不再出现普通 streaming draft、普通 tool trace、普通 thinking 面板。
5. 刷新页面后，用户消息状态不回退。
6. 任务完成回调能正确写入消息历史，并推送到前端。
7. 团队成员页与主聊天页不再各自维护不同的模式分支。
8. 代码中不再存在 `isTaskRunnerAsyncContactMode` 这类兼容旧模式的判断链。
9. 前后端编译通过。

## 10. 这次改造完成后保留下来的核心能力

保留：

- 创建任务
- 查询任务
- 任务回调补消息
- 消息任务抽屉
- workspace / server 配置程序透传
- 任务状态展示

删除：

- 普通实时聊天模式
- 普通 MCP 直连
- 普通工具流展示
- 普通 history process 展示链路
- 普通模式兼容状态机

这份方案的目标很明确：把 Chatos 从“双模式拼装体”收成“只为 Task Runner 服务的一条对话链路”，后面再做功能迭代时，代码会稳很多。
