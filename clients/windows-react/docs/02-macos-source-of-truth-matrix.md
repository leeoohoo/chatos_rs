# macOS 源码事实矩阵

> 状态：阶段 0 基线，持续更新
> 分支：`3.0.9.1`
> 事实源：`clients/macos` 当前源码与测试、`clients/local_agent_host` 当前契约
> 用途：约束 macOS 重构与 Windows React 实现，不是界面截图说明书

## 1. 强制规则

Windows React 的业务逻辑只能从 macOS 当前源码、测试及其调用的 Rust Host 契约中提取。截图只回答“长什么样”，不能用来推导“怎么运行”。

每个页面必须依次通过以下状态：

1. `未映射`：只有页面名称或截图，不允许写 React 业务代码。
2. `源码映射中`：正在核对完整调用链，仍不允许写 React 页面。
3. `已映射，可重构`：View、状态机、Port、Adapter、Host 和测试证据齐全，可以开始 macOS 分层。
4. `macOS 分层完成，可进入 React`：Application/Port 已稳定且有测试，才允许创建对应 React feature。

“已映射”至少必须具备：

- 页面入口与导航来源；
- View 和所有参与业务决策的 ViewModel/Feature Model；
- Domain model、Application Port 与错误类型；
- Connector adapter、Host command/event、分页和并发语义；
- loading、空、成功、失败、取消、重试、断线和恢复状态；
- 对应自动测试；
- Windows 平台差异记录。没有差异时必须明确写“无”。

禁止事项：

- 根据截图发明默认值、状态迁移、重试策略、错误文案或数据结构；
- 因 React/Tauri 使用方便而绕开 macOS 的账号、版本、权限或幂等约束；
- 在 Port 尚未稳定时直接把 Swift Connector 调用翻译为 Tauri `invoke`；
- 将旧 `clients/windows` 的行为作为参考。

## 2. 首批垂直切片总览

| 切片 | 当前状态 | macOS UI | macOS 状态/用例 | Domain/Port | Connector/Host | 主要测试 | React 准入 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 项目会话准备 | 已映射，可重构 | `ProjectMessagesView.swift` | `AppModelResources.swift` | `ProjectConversationModels.swift` | `NativeLocalAgentWorkspaceService.swift` | Connector 与 ViewModel 相关测试 | 禁止；先完成 macOS Application/Port 分层 |
| Conversation 时间线与发送 | 已映射，可重构 | `ConversationTimelineView.swift` | `ConversationSessionViewModel*.swift` | `ConversationModels.swift`、`ConversationCommandService.swift`、`ConversationRemoteService.swift`、`ConversationRealtimeService.swift` | `NativeLocalAgentConversationService.swift`、`NativeLocalAgentConversationClient.swift`、Host conversation commands | `ConversationSessionViewModelTests.swift` 等 | 禁止；先完成 macOS Application/Port 分层 |
| Task Graph / Run Inspector | 已映射，可重构 | `MessageTaskWorkspaceSheet.swift`、`MessageTaskInspectorView.swift`、`TaskProcessTimelineView.swift` | `MessageTaskWorkspaceViewModel*.swift` | `MessageTaskRunModels.swift` 及 task graph models | `NativeLocalAgentMessageTaskGraphService.swift`、Host task/run/event commands | `MessageTaskWorkspaceViewModelTests.swift` 等 | 禁止；先完成 macOS Application/Port 分层 |
| Ask User / 审批 / 恢复 | 源码映射中 | Conversation 与执行详情内嵌入口 | `ConversationSessionViewModel+AskUser.swift` 及审批状态 | Ask User / approval protocols | Native Local Agent Ask User 与 approval adapters | `NativeLocalAgentAskUserPromptServiceTests.swift` 等 | 禁止 |
| Host lifecycle | 源码映射中 | App Shell 与各页面 ready gate | AppModel/Host lifecycle state | Host client/service protocols | Host process、transport、handshake | lifecycle 与 transport 测试待汇总 | 禁止 |

以上“React 准入”不是进度建议，而是代码审查门禁。

## 3. 项目会话准备：完整调用链

### 3.1 页面入口与状态展示

- 项目工作区入口：`clients/macos/Sources/ChatOSApp/Features/Project/ProjectWorkspaceView.swift`
- 消息页面：`clients/macos/Sources/ChatOSApp/Features/Chat/ProjectMessagesView.swift`
- 页面在 `.task(id: projectID)` 中先 `Task.yield()`，再调用 `prepareProjectChat(projectID:)`，避免同一 SwiftUI 更新周期内短暂错误/进度分支闪现。
- UI 必须保留四个互斥状态：
  - 已有 Conversation：展示 `ConversationTimelineView`；
  - 正在准备：明确的连接进度；
  - 准备失败：展示错误并允许重试；
  - 尚无可用会话：等待默认项目联系人。

### 3.2 状态所有权与并发规则

源码：`clients/macos/Sources/ChatOSApp/AppModelResources.swift`

调用链：

```text
ProjectMessagesView.task
  → AppModel.prepareProjectChat
  → prepareProjectConversationIfNeeded
  → ensureProjectConversation
  → ProjectConversationPreparing.ensureConversation
  → applyPreparedConversation
  → conversation(for:).activate
```

必须原样保留的规则：

- 仅登录账号与 Local Agent workspace owner 一致时才准备会话；
- 项目与默认联系人必须存在；
- 同一项目已有准备任务时复用，不重复创建；
- `requestID` 与 `workspaceAccountGeneration` 阻止过期异步结果覆盖当前账号/项目状态；
- 强制 reconcile 可校正重装、重新配对或 workspace 移动后的陈旧执行目标；
- 失败保存在项目级错误状态，用户显式重试才清除并重新准备；
- Conversation ViewModel 按 session ID 缓存，激活新会话时停用旧会话。

### 3.3 Conversation 绑定与幂等

Port：`clients/macos/Sources/ChatOSCore/ProjectConversationModels.swift`

Adapter：`NativeLocalAgentProjectConversationService`，位于 `clients/macos/Sources/ChatOSConnector/NativeLocalAgentWorkspaceService.swift`。

- `project.projectContext.projectId` 必须等于项目 ID，否则拒绝创建；
- resource binding 固定为 `.project(project.id)`；
- 创建前先按 binding 查询现有 Conversation；
- 创建发生并发竞争并失败后再次查询，查到即返回，保证幂等；
- Windows 不得设计另一套 project → conversation 关系。

### 3.4 目标 Application/Port

macOS 分层时从现有行为提取，不增加业务含义：

- `PrepareProjectConversationUseCase`
- `ProjectConversationPreparationState`
- `ProjectConversationPort`
- `ConversationSessionFactory`

React 以后只能消费上述稳定用例输出；Tauri adapter 只负责传输与 Windows 平台能力。

## 4. Conversation：完整调用链

### 4.1 UI 与 Feature State

UI：

- `clients/macos/Sources/ChatOSApp/Features/Chat/ConversationTimelineView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Chat/ConversationTimelineItem.swift`

状态与行为：

- `ConversationSessionViewModel.swift`
- `ConversationSessionViewModel+Sending.swift`
- `ConversationSessionViewModel+Attachments.swift`
- `ConversationSessionViewModel+AskUser.swift`

独立状态不可合并成一个 `loading`：

- `isRefreshing`
- `isLoadingOlder`
- `isSending`
- `isUpdatingRuntimeSettings`
- `historyError`
- `runtimeSettingsError`
- `sendError`

历史默认每页 10 条；realtime 刷新 debounce 为 250ms。Windows 必须保留二者，除非先修改 macOS 产品行为与测试。

### 4.2 发送状态机

源码事实：`ConversationSessionViewModel+Sending.swift`

```text
sendDraft
  ├─ 空文本且无附件 / 正在发送 → 忽略
  ├─ command service 不可用 → sendError
  ├─ 存在 streaming Turn
  │    ├─ 没有显式 capability → sendGuidance
  │    └─ 有显式 capability → 阻止并提示等待或停止
  └─ 不存在 streaming Turn → sendNewTurn
```

- Guidance 发送前后都以现有 Turn 为目标；若 Host 判断目标已结束，捕获 `guidanceTargetInactive` 并自动降级为新 Turn。
- 新 Turn 先插入 optimistic Turn，并使时间线观察状态失效；发送失败时删除 optimistic Turn，恢复草稿、附件和显式 capability。
- `beginSending` 会清空当前草稿、附件、附件错误、能力选择和发送错误；失败恢复必须避免重复附件。
- 停止运行不是前端改状态，而是调用 Host cancel。

### 4.3 Host ready、刷新与 realtime

源码事实：`ConversationSessionViewModel.swift`

- Host 未 ready 时不启动业务刷新；
- ready 后重新加载 runtime settings、Ask User、历史并启动 realtime；
- unavailable 时取消 realtime、retry、debounce 等任务，并复位 loading 状态；
- realtime 事件经 history store 合并，不允许组件自行拼接出另一套事实；
- 历史分页游标是 ordinal，非法游标必须报错；limit 被限制在 1–100；
- 本地 realtime 来自 Host event hub/reconcile，不是服务端 WebSocket。

### 4.4 Adapter 与 Host 契约

Port：

- `clients/macos/Sources/ChatOSCore/ConversationCommandService.swift`
- `clients/macos/Sources/ChatOSCore/ConversationRemoteService.swift`
- `clients/macos/Sources/ChatOSCore/ConversationRealtimeService.swift`
- `clients/macos/Sources/ChatOSCore/ConversationHistoryStoring.swift`

Adapter：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationService.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationClient.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationCommands.swift`

不可丢失的契约：

- `sendNewTurn` 先解析 Conversation runtime selection；
- 插件安装助手 Conversation 使用专用 capability snapshot，普通会话使用标准 snapshot；
- 附件先由 attachment vault 授权，IPC 只传 `authorized_local_ref`；
- `startTurn` 携带 expected Conversation version、model config revision 和 capability policy revision；
- Host 接受后唤醒 platform tool worker；
- `sendGuidance` 先确认目标 Turn 仍为 `running`；
- history 使用 `before_ordinal`，返回 `next_before_ordinal`；
- Host 返回 `not_found` 时，当前 adapter 创建空 Conversation 后重试历史查询。

### 4.5 测试证据

- `clients/macos/Tests/ChatOSAppTests/ConversationSessionViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationGuidanceTimelineTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationTimelineBottomSyncPolicyTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentConversationCommandTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentConversationRuntimeSettingsServiceTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentAskUserPromptServiceTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationHistoryStoreTests.swift`

Windows 对应契约测试必须复用相同 fixture，并覆盖上述分支；不能只做组件快照测试。

## 5. Task Graph / Run Inspector：完整调用链

### 5.1 UI 与 Port

UI：

- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskWorkspaceSheet.swift`
- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskInspectorView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Execution/TaskProcessTimelineView.swift`

状态：

- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskWorkspaceViewModel.swift`
- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskWorkspaceViewModel+Loading.swift`

Port 与模型：

- `clients/macos/Sources/ChatOSCore/MessageTaskRunModels.swift`
- `MessageTaskGraphServicing.fetchGraph/fetchTask/fetchRun/retryRun/cancelTask`

Adapter：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentMessageTaskGraphService.swift`

### 5.2 最新执行记录与分页

这是产品语义，不是排序样式：

- 打开 Run Inspector 时请求最新 40 条事件：`eventLimit: 40, eventOffset: 0`；
- Adapter 调 Host events 时固定 `newestFirst: true`；
- 加载更多每次 50 条，以当前事件数为 offset；
- 合并分页时按 event ID 去重；
- `eventLimit` 在 adapter 内限制为 1–100，并多取 1 条判断是否还有更旧记录；
- realtime reconcile 与 `completed/failed/cancelled/persisted` 都刷新 Graph、当前 Task 和当前 Inspector；
- 因此 Windows 运行详情首屏必须展示最新执行记录，不能停留在任务刚开始的记录。

### 5.3 并发、空结果与降级

- 切换 Task 时取消旧 inspector、model output、run 和 load-more 请求；
- 每次异步写回前检查 `requestedTaskID`，旧请求不能覆盖新选择；
- 已经观察到的稳定 Graph 不得被瞬时空响应擦除；
- 初始空 Graph 最多按现有策略重试，不能无限 loading；
- realtime 中断时展示“实时进度连接已中断”错误，并回退 polling；
- polling 只在存在 active task 或正在等待初始空 Graph 时运行；没有活动任务后停止。

### 5.4 测试证据

- `clients/macos/Tests/ChatOSAppTests/MessageTaskWorkspaceViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/TaskGraphDeliveryPresentationTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentMessageTaskGraphServiceTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphNormalizerTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphFocusTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskAcceptanceTests.swift`

## 6. 31 个页面的准入状态

| # | 页面 / 截图 | 状态 | React 准入 |
| --- | --- | --- | --- |
| 01 | 项目目录 | 源码映射中 | 禁止 |
| 02 | 项目消息 | 已映射，可重构 | 禁止；等待 macOS 分层 |
| 03 | 项目设置 | 未映射 | 禁止 |
| 04 | 联系人聊天 | 源码映射中（复用 Conversation，入口差异待完成） | 禁止 |
| 05 | 应用 | 未映射 | 禁止 |
| 06 | AI 图片 | 未映射 | 禁止 |
| 07 | AI 视频 | 未映射 | 禁止 |
| 08 | AI 故事 | 未映射 | 禁止 |
| 09 | AI 历史 | 未映射 | 禁止 |
| 10 | Agent 管理 | 未映射 | 禁止 |
| 11 | Agent 文档 | 未映射 | 禁止 |
| 12 | Agent 团队概览 | 未映射 | 禁止 |
| 13 | Agent 团队聊天 | 未映射 | 禁止 |
| 14 | Agent 团队任务 | 未映射 | 禁止 |
| 15 | Agent 团队调研 | 未映射 | 禁止 |
| 16 | Agent 团队资产 | 未映射 | 禁止 |
| 17 | Agent 团队运行 | 源码映射中（Run 模型复用关系待确认） | 禁止 |
| 18 | 需求调研 | 未映射 | 禁止 |
| 19 | 本地终端 | 未映射 | 禁止 |
| 20 | Notebook | 未映射 | 禁止 |
| 21 | 项目 Git | 未映射 | 禁止 |
| 22 | 设置 / 通用 | 未映射 | 禁止 |
| 23 | 设置 / 微信 | 未映射 | 禁止 |
| 24 | 设置 / 宠物 | 未映射 | 禁止 |
| 25 | 设置 / 全局工具 | 未映射 | 禁止 |
| 26 | 设置 / AI 模型 | 未映射 | 禁止 |
| 27 | 设置 / 设备网关 | 未映射 | 禁止 |
| 28 | 设置 / 插件与 Skills | 未映射 | 禁止 |
| 29 | 设置 / 命令审批 | 源码映射中 | 禁止 |
| 30 | 设置 / 运行时权限 | 源码映射中 | 禁止 |
| 31 | 设置 / 访问控制 | 未映射 | 禁止 |

Run Inspector 是项目消息中的业务弹层，不对应单独的一级截图；它已在第 5 节完成第一轮源码映射，但仍需先完成 macOS 分层。

## 7. 第一条切片的下一步

按以下顺序实施，不能跳过到 React 页面：

1. 完成 Host lifecycle、Ask User/审批/恢复的源码映射；
2. 从现有 Swift 行为提取 Application use cases 与 ports，不改变产品语义；
3. 让 macOS Conversation 与 Run Inspector 仅依赖这些 ports；
4. 用现有测试验证行为未变化，并补齐生命周期与竞态测试；
5. 固定 Rust/Swift/TypeScript fixture；
6. 将本矩阵状态改为 `macOS 分层完成，可进入 React`；
7. 才创建 `clients/windows-react` 下对应的 React feature。

任何新的平台差异必须在本文件新增“差异原因、macOS 原行为、Windows 适配方式、测试与批准记录”后才能实现。
