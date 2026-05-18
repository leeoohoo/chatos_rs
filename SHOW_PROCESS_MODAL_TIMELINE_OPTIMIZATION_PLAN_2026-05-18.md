# Show Process 弹框时间轴优化方案

## 1. 目标

当前 `Show process` 的交互是把过程消息直接插回主消息流，挂在用户消息下面展开。这个方案在数据上能工作，但在体验上有三个明显问题：

- 主消息流被过程噪音打断，用户很难继续顺着“提问 -> 最终回答”阅读。
- 过程内容和最终回答混在同一条垂直流里，展开后高度暴涨，滚动定位很差。
- 执行中与历史态的展示方式不一致，用户对“当前正在发生什么”和“历史过程长什么样”没有统一心智。

本方案目标是把 `Show process` 改成：

- 点击按钮后打开独立弹框，不再在用户消息下方内嵌展开过程明细。
- 弹框内部使用时间轴展示 thinking / tool call / tool stream / tool result / unavailable tool。
- 历史态与执行中态共用同一套过程视图模型。
- 主消息流始终只保留：用户消息 + 最终 assistant 消息。

## 2. 现状结论

我看完代码后，当前实现的关键事实如下。

### 2.1 现在的 `toggleTurnProcess` 不是“打开面板”，而是“把过程消息 merge 回 messages”

核心逻辑在：

- `chat_app/src/lib/store/actions/messagesTurnProcess.ts`
- `chat_app/src/lib/store/helpers/messages/turnProcessState.ts`

当前点击 `Show process` 后，会：

- 读取或请求该 turn 的 process messages
- 调用 `mergeTurnProcessMessages(...)`
- 把 assistant/tool 过程消息重新插回 `state.messages`
- 再通过 `hidden / historyProcessExpanded` 控制显示与收起

这意味着当前主消息流本身就是过程展示容器，所以体验上天然会“挤在用户消息下面”。

### 2.2 主消息列表已经围绕“内嵌过程消息”做了很多派生

核心在：

- `chat_app/src/components/messageList/derivedData.ts`
- `chat_app/src/components/MessageList.tsx`
- `chat_app/src/components/MessageItem.tsx`

`MessageList` 现在接收的是完整 `messages`，里面既有用户消息、最终回答，也可能包含被 merge 进来的 process assistant/tool message。  
`derivedData.ts` 会根据 `historyProcessUserMessageId / historyProcessTurnId / historyFinalForUserMessageId` 把它们重新关联回用户消息。

这也是为什么当前虽然 UI 上只点了一个按钮，但底层其实是在“重写主列表内容”。

### 2.3 项目里已经有一套 `TurnProcessDrawer`，但现在没有挂到页面树

相关文件：

- `chat_app/src/components/TurnProcessDrawer.tsx`
- `chat_app/src/components/turnProcessDrawer/useTurnProcessDrawerModel.ts`

这套组件已经具备：

- 独立读取某个 userMessageId 对应的 process messages
- 汇总 tool result / assistant tool call
- fallback 到 final assistant 里的 thinking/tool_call segment

但它目前没有在聊天主页面或团队成员页面真正挂载。  
也就是说，仓库里已经有“把过程从消息流里抽离出来”的半成品思路，但当前产品路径仍然走的是 inline expand。

### 2.4 历史态后端接口已经够用

后端相关：

- `chat_app_server_rs/src/api/sessions/message_handlers.rs`
- `chat_app_server_rs/src/api/sessions/history_process.rs`

已有接口：

- `GET /api/conversations/:conversation_id/turns/:user_message_id/process`
- `GET /api/conversations/:conversation_id/turns/by-turn/:turn_id/process`

这些接口已经能按 turn 拉过程记录。  
所以这次优化的主战场不是后端接口缺失，而是前端状态模型与挂载方式。

### 2.5 执行中态的过程信息其实已经在 streaming assistant message 里持续累积

相关文件：

- `chat_app/src/lib/store/actions/sendMessage/streamEventHandler.ts`
- `chat_app/src/lib/store/actions/sendMessage/streamLifecycleEvents.ts`
- `chat_app/src/lib/store/actions/sendMessage/toolStreamEvents.ts`
- `chat_app/src/lib/store/actions/sendMessage/streamingState.ts`

执行中时，以下内容会持续写入当前 streaming assistant message：

- `contentSegments[].type === 'thinking'`
- `contentSegments[].type === 'tool_call'`
- `metadata.toolCalls`
- `metadata.unavailableTools`

同时用户消息上的 `metadata.historyProcess` 也会持续累计：

- `thinkingCount`
- `toolCallCount`
- `unavailableToolCount`
- `processMessageCount`

这意味着执行中态并不需要新增后端接口，也不需要等 turn 结束后才能展示过程时间轴。

## 3. 当前问题的根因

从实现上看，当前体验差不是因为样式不够好，而是因为架构层做了这几个选择：

1. `Show process` 的状态语义被设计成“展开消息流”。
2. 过程数据和主消息流共用同一个 `messages` 数组承载。
3. 历史态与执行中态没有统一成一个“过程视图模型”，而是分别散落在：
   - compact history process messages
   - fetched turn process messages
   - streaming assistant draft segments/toolCalls
4. `HistoryProcessSummary` 只是一个 toggle 按钮，不是一个真正的 process viewer entry。

所以这次优化如果只改按钮样式、只把 `TurnProcessDrawer` 改成 modal，而不调整状态语义，后面还是会继续出现：

- 主列表污染
- 历史和 streaming 不一致
- 切页/切 session/切团队成员时 process 状态难维护

## 4. 推荐方案总览

推荐把 `Show process` 重构成“过程查看器”模式，而不是“消息展开”模式。

### 核心原则

- 主消息流只显示用户消息和最终 assistant。
- 过程数据独立缓存，但不再 merge 回 `messages`。
- 点击 `Show process` 只负责打开弹框，并指定当前 turn。
- 弹框里的时间轴统一渲染历史态和执行中态。
- 执行中时，按钮和弹框都要能实时反映新增的 thinking/tool 过程。

## 5. 交互设计

### 5.1 用户入口

保留用户消息下面的 `Show process` 摘要条，但行为改为：

- 点击后打开 modal
- 不再插入内嵌 assistant/tool 节点
- 按钮文案从 `Show process / Hide process` 改成更稳定的查看语义

建议文案：

- 空闲历史态：`查看过程`
- 执行中：`查看执行过程`
- 加载中：`加载过程...`

按钮右侧保留摘要 chip：

- `Tools: n`
- `Thinking: n`
- `Unavailable: n`

执行中可以再加一个状态 chip：

- `Running`

### 5.2 弹框形态

建议使用居中 modal，而不是侧边抽屉。

原因：

- 用户当前的诉求就是“点击后弹出一个弹框”
- 过程内容是辅助阅读，不应永久占据横向空间
- 聊天页面和团队成员页面都更容易共享一套 modal overlay

弹框建议结构：

1. 顶部标题区
2. 过程概览区
3. 时间轴内容区
4. 底部操作区（关闭、必要时刷新）

### 5.3 弹框头部信息

建议展示：

- 标题：`过程详情`
- 副标题：当前 turn 的用户问题首行摘要
- 状态：
  - `执行中`
  - `已完成`
  - `已停止`
  - `失败`
- 统计：
  - thinking 数
  - tool 数
  - unavailable 数
  - 总耗时（如果能拿到）

### 5.4 时间轴结构

时间轴按事件顺序渲染，建议支持这些节点类型：

- `thinking`
- `tool_call_started`
- `tool_stream_chunk`
- `tool_result`
- `tool_unavailable`
- `final_answer_ready`（可选，作为收尾节点）

展示建议：

- `thinking`
  - 折叠卡片
  - 默认只展示摘要前 2 行
  - 点击展开全文
- `tool_call_started`
  - 展示工具名、参数摘要、开始时间
- `tool_stream_chunk`
  - 如果工具有持续输出，折叠为“实时日志”卡片
  - 默认只显示最新若干行，可展开全文
- `tool_result`
  - 成功与失败状态清晰区分
  - 可复用现有 tool card / tool result 渲染能力
- `tool_unavailable`
  - 高亮显示原因，不和 tool_result 混淆

## 6. 状态与数据模型改造

### 6.1 不再把 process messages merge 回 `messages`

这是本次优化最重要的改动。

当前：

- `toggleTurnProcess()` 会修改 `messages`

目标：

- `openTurnProcessViewer()` 只修改 viewer UI state
- process 数据放在独立 cache
- `messages` 始终保持主会话消息流干净

建议新增独立状态：

- `sessionTurnProcessViewerState[sessionId]`

建议结构：

```ts
type TurnProcessViewerState = {
  open: boolean;
  userMessageId: string | null;
  turnId: string | null;
  loading: boolean;
  loaded: boolean;
  pinnedWhileStreaming: boolean;
};
```

现有的：

- `sessionTurnProcessCache`
- `sessionTurnProcessState`

可以保留，但语义需要弱化为“数据缓存/加载状态”，不再承担“控制主消息流显示隐藏”的职责。

### 6.2 新增统一的“时间轴视图模型”

建议新增一层 selector / mapper，例如：

- `chat_app/src/components/turnProcessViewer/buildTurnProcessTimeline.ts`

输入来源：

- 历史态：
  - `sessionTurnProcessCache`
  - 或者按 turn 拉到的 process messages
- 执行中态：
  - 当前 streaming assistant message 的 `contentSegments`
  - 当前 streaming assistant message 的 `toolCalls`
  - `unavailableTools`

输出统一为：

```ts
type TurnProcessTimelineItem =
  | { id: string; kind: 'thinking'; createdAt?: string; text: string; isStreaming?: boolean }
  | { id: string; kind: 'tool_call_started'; createdAt?: string; toolCallId: string; toolName: string; arguments: unknown }
  | { id: string; kind: 'tool_stream_chunk'; createdAt?: string; toolCallId: string; text: string; isStreaming?: boolean }
  | { id: string; kind: 'tool_result'; createdAt?: string; toolCallId: string; result: unknown; error?: string; completed: boolean }
  | { id: string; kind: 'tool_unavailable'; createdAt?: string; serverName: string; toolName: string; reason: string }
  | { id: string; kind: 'final_answer_ready'; createdAt?: string; assistantMessageId: string };
```

这样历史态和执行中态就可以走同一套 UI。

### 6.3 当前 `TurnProcessDrawer` 的能力建议复用，但不要直接原样复用

可复用部分：

- `useTurnProcessDrawerModel.ts` 里找 user message / turnId / final assistant 的逻辑
- fallback 逻辑
- toolResultById / assistantToolCallsById 的构建

不建议直接复用部分：

- 抽屉宽度逻辑 `useResizableTurnProcessPanel`
- 直接用 `MessageItem` 渲染过程消息

原因：

- `MessageItem` 是主会话消息卡片，不是时间轴节点
- 时间轴需要更细粒度地把 thinking、tool start、tool stream、tool result 拆开
- modal 也不需要现有的 resize 逻辑

## 7. 执行中态方案

这是这次方案里必须单独照顾的部分。

### 7.1 执行中入口表现

当当前 turn 正在执行时，用户消息下方摘要条建议显示为：

- 主按钮：`查看执行过程`
- 状态点：绿色/蓝色脉冲
- 统计 chip 持续递增

如果用户已经打开弹框：

- 弹框保持打开
- 时间轴实时 append 新节点
- 自动滚动到底部，但当用户主动向上滚动后暂停自动跟随

### 7.2 执行中时间轴数据来源

优先直接从内存中的 streaming message 构建，不等后端。

数据源：

- `sessionChatState[sessionId].isStreaming`
- 当前 streaming assistant message
- 该 assistant message 的：
  - `metadata.contentSegments`
  - `metadata.toolCalls`
  - `metadata.unavailableTools`
- 对应用户消息的 `metadata.historyProcess`

这样能做到：

- thinking 一出来就能看到
- tool start 一出来就能进时间轴
- tool stream 持续更新日志
- tool end 立刻变成完成态

### 7.3 执行完成后的衔接

turn 完成后，viewer 需要从“流式草稿态”平滑切到“持久化态”。

建议策略：

1. streaming 中优先展示本地内存时间轴
2. turn 结束后触发一次 turn process fetch
3. 用服务端返回结果刷新 cache
4. 尽量保留滚动位置和 viewer open 状态

这样可以避免：

- 流式过程与历史过程断层
- 关闭再打开后内容顺序变化太大

### 7.4 停止/失败态

执行中还要兼顾：

- 用户 stop
- tool error
- conversation cancel
- 后端异常结束

建议在 viewer 顶部状态明确区分：

- `执行中`
- `已停止`
- `执行失败`
- `已完成`

同时在时间轴里保留终止节点，例如：

- `执行已停止`
- `工具执行失败`
- `本轮在生成最终答案前终止`

## 8. 组件设计建议

### 8.1 新组件建议

建议新增：

- `chat_app/src/components/TurnProcessModal.tsx`
- `chat_app/src/components/turnProcessViewer/useTurnProcessViewerModel.ts`
- `chat_app/src/components/turnProcessViewer/buildTurnProcessTimeline.ts`
- `chat_app/src/components/turnProcessViewer/TurnProcessTimeline.tsx`
- `chat_app/src/components/turnProcessViewer/TurnProcessTimelineItem.tsx`

### 8.2 现有组件改造点

#### `HistoryProcessSummary.tsx`

从 toggle 行为改成 viewer trigger：

- 不再依赖 `expanded`
- 改为 `open / loading / running`
- 按钮语义改成“查看”

#### `MessageItem.tsx`

保留摘要条，但移除对 inline 展开语义的耦合。

#### `MessageList.tsx`

不再需要 process message 混入列表。  
主列表只渲染真正的会话消息。

#### `useMessageListDerivedState.ts / derivedData.ts`

需要缩减针对 inline process message 的派生逻辑，重点保留：

- process 统计摘要
- user 与 final assistant 的关联

可以逐步下线这些内嵌展开相关逻辑：

- `linkedUserExpandedByAssistantId`
- process message visibility 依赖 `hidden`
- inline process insertion 相关推导

#### `ChatInterfaceOverlays.tsx`

建议把 `TurnProcessModal` 作为全局 overlay 挂在这里。  
原因是主聊天页已经把其它 overlay 放在这里，结构最一致。

#### `TeamMembersPane.tsx`

团队成员页也需要同样挂载 `TurnProcessModal`，或者共享一套更上层 overlay mount。  
不要只在主聊天页实现，否则团队成员页的行为会和聊天页分裂。

## 9. 推荐实施步骤

### 第 1 步：先把 viewer 状态独立出来

- 新增 `openTurnProcessViewer / closeTurnProcessViewer`
- 保持 `toggleTurnProcess` 兼容一小段时间，但内部逐步转调 viewer open
- 不马上删除旧 cache

### 第 2 步：接入 modal，但先复用现有 process 数据

- 挂载 `TurnProcessModal`
- 点击摘要条打开 modal
- modal 内先显示现有 process messages 列表
- 此阶段先不做时间轴拆分，只验证交互路径和状态隔离

### 第 3 步：把 modal 内容升级为时间轴

- 引入 timeline view model
- thinking / tool call / tool result / unavailable tool 分节点渲染
- streaming 中实时更新

### 第 4 步：从主消息流里移除 inline process message 依赖

- 停止 `mergeTurnProcessMessages()` 影响主 `messages`
- `MessageList` 不再渲染 process assistant/tool message
- 收敛 `derivedData.ts` 的复杂关联逻辑

### 第 5 步：清理遗留状态

- 删除仅服务于 inline expand 的字段和逻辑
- 若 `TurnProcessDrawer` 不再使用，替换或删除

## 10. 风险与注意点

### 10.1 最大风险不是 UI，而是状态双轨期

在过渡阶段，仓库里可能同时存在：

- inline process 旧语义
- modal viewer 新语义

如果不控制好，容易出现：

- 同一 turn 同时在主列表和弹框重复展示
- `expanded` 状态残留
- 切 session 后 viewer 指向错误 turn

建议在第 2 步开始时就明确：

- 主路径是 modal
- inline expand 不再对外可见

### 10.2 streaming 与 persisted 数据切换要避免闪烁

执行完成后，如果服务端回填顺序与本地流式顺序不同，时间轴可能跳动。  
建议 timeline item 设计稳定 ID，优先按：

- `toolCallId`
- `message.id`
- `segmentIndex`

做节点归并。

### 10.3 团队成员页和主聊天页必须统一

当前这两处都会复用 `MessageList`：

- `chat_app/src/components/chatInterface/ChatConversationPane.tsx`
- `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspaceContent.tsx`

所以 viewer 方案不能只改主聊天页，否则团队成员页仍会保留糟糕体验。

## 11. 验收标准

### 交互验收

1. 点击用户消息下方过程按钮后，打开 modal，不再把过程消息插回主消息流。
2. 关闭 modal 后，消息流高度和滚动位置保持稳定。
3. 聊天页和团队成员页行为一致。

### 历史态验收

1. 历史 turn 打开过程弹框后，能正确展示 thinking、tool、unavailable tool。
2. 最终 assistant 仍只在主消息流中展示，不在过程视图里重复污染阅读链路。
3. 没有 process records 但 final assistant 内含 reasoning/tool_call segment 时，fallback 仍可展示。

### 执行中态验收

1. 执行中点击按钮可以立即打开弹框。
2. thinking、tool start、tool stream、tool end 能实时进入时间轴。
3. turn 完成后，弹框内容从流式态平滑切到历史持久化态。

### 技术验收

1. `state.messages` 不再因为查看过程而被插入/移除 process messages。
2. process viewer 有独立 open state。
3. 主消息列表派生逻辑明显简化，不再依赖 inline process visibility。

## 12. 最终建议

这次优化建议不要做成“给现有 inline 展开套一个 modal 壳”，而应该明确升级为：

- 主消息流负责结果阅读
- process modal 负责过程追踪
- timeline 负责统一历史态和执行中态

从仓库现状看，最佳路径不是从零设计，而是：

1. 复用现有 `turn process fetch/cache` 能力
2. 吸收 `TurnProcessDrawer` 里已有的 turn 解析与 fallback 逻辑
3. 停止把 process merge 回 `messages`
4. 在 overlay 层正式挂载 `TurnProcessModal`

这样改完之后，`Show process` 才会从“打断阅读的展开按钮”真正变成“可单独查看的执行过程时间轴”。

## 13. 当前落地状态（2026-05-18）

本方案目前已经完成的实现如下：

- `Show process` 已切换为点击打开弹框，不再在用户消息下方直接内联展开。
- 主聊天页与团队成员页都已经挂载同一套 `TurnProcessModal` 过程查看器。
- 主消息流已经过滤 inline process assistant message，只保留用户消息和最终 assistant 消息。
- 弹框内部已经改为时间轴展示，并统一兼容历史态与执行中态。
- 执行中支持实时更新、自动跟随、暂停跟随后的“回到最新”入口，以及活跃步骤高亮。
- 旧的 `TurnProcessDrawer` 入口和前端 `toggleTurnProcess` action 已经移除，仓库主路径收敛为 modal viewer。
- 主消息流不再把 turn process cache 重新 merge 回 `state.messages`，viewer 直接读取实时消息与 process cache。
- `sessionTurnProcessState` 以及 `historyProcessExpanded / historyProcessLoaded / historyProcessInlineMessages` 这套旧展示状态主干已移除。
- 已补充测试覆盖主路径，并通过类型检查和生产构建验证。

当前仍然刻意保留的兼容层如下：

- `sessionTurnProcessCache` 仍然保留，作为 turn recovery 与 viewer 的共享数据源。
- 后续如果还要继续极限收敛，可以再评估是否把 process cache 也收口到更独立的 viewer/domain 层，而不是继续放在 chat store 主状态里。
