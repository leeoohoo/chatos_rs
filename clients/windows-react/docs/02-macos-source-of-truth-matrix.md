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
| 项目会话准备 | Application 用例已提取，继续收口 UI 协调 | `ProjectMessagesView.swift` | `AppModelResources.swift`、`PrepareProjectConversationUseCase.swift` | `ProjectConversationModels.swift` | `NativeLocalAgentWorkspaceService.swift` | `PrepareProjectConversationUseCaseTests.swift` 及 Connector/ViewModel 测试 | 禁止；等待完整切片验收 |
| Conversation 时间线与发送 | History、发送、viewport/unread、刷新合并与 realtime 动作规则已提取，UI Task 生命周期待收口 | `ConversationTimelineView.swift` | `ConversationSessionViewModel*.swift`、Conversation Application 类型 | `ConversationModels.swift`、Conversation ports | `NativeLocalAgentConversationService.swift`、`NativeLocalAgentConversationClient.swift`、Host conversation commands | Conversation Core policy/use case tests、ViewModel 测试等 | 禁止；等待完整切片验收 |
| Task Graph / Run Inspector | Graph、realtime、Run、section 刷新与 polling 决策已提取，UI 异步任务生命周期待收口 | `MessageTaskWorkspaceSheet.swift`、`MessageTaskInspectorView.swift`、`TaskProcessTimelineView.swift` | `MessageTaskWorkspaceViewModel*.swift`、Task Graph/Run/Inspector Application 类型 | `MessageTaskRunModels.swift` 及 task graph models | `NativeLocalAgentMessageTaskGraphService.swift`、Host task/run/event commands | 四组 Core state/use case tests、ViewModel 测试等 | 禁止；等待完整切片验收 |
| Ask User | 查询/提交/取消、排序投影、操作门禁与表单状态已提取，UI Task 生命周期待迁移 | `AskUserPromptCardView.swift`、Conversation 内嵌入口 | `ConversationSessionViewModel+AskUser.swift`、Ask User Application 类型 | `AskUserPromptModels.swift` | `NativeLocalAgentAskUserPromptService.swift`、Host run/conversation commands | Ask User Core、Connector 与 ViewModel 测试 | 禁止；等待完整切片验收 |
| 工具审批 / 恢复 | 已映射，可重构 | `GlobalApprovalOverlayHost`、完整审批页 | `LocalConnectorControlCenterViewModel.swift` | Local Connector approval models/ports | platform tool worker、approval handler、Local Connector approval service | worker、approval agent、state store、ViewModel tests | 禁止；先完成 macOS Application/Port 分层 |
| Host lifecycle | 已映射，可重构 | `RootView.swift` 与各页面 ready gate | `AppModelAuthentication.swift` | `LocalAgentHostLifecycle.swift` | `NativeLocalAgentHostLifecycle.swift`、`NativeLocalAgentHostProcess.swift` | `NativeLocalAgentHostLifecycleTests.swift` | 禁止；先完成显式 Application 状态 |

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

实施进度（2026-10-19）：

- 已新增 `ProjectContextLoading` Port，由现有 `NativeLocalProjectsService` adapter 实现；
- 已新增 `PrepareProjectConversationUseCase`，按 owner + project 复用并发准备任务；
- Use Case 仍严格执行“先加载当前 project context，再调用 `ProjectConversationPreparing`”的 macOS 顺序；
- AppModel 继续负责登录账号、workspace generation、项目是否仍存在和四种 UI 状态，迟到结果仍不能写回；
- 登出/账号切换与项目删除会取消对应 Application 任务；
- 已覆盖同账号同项目复用、不同账号绝不复用，以及 context 注入后才创建 Conversation 的测试。

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

当前分层进度：

- `clients/macos/Sources/ChatOSCore/Application/ConversationHistoryUseCase.swift` 已统一 latest/older 查询、HistoryStore 合并和 snapshot 读取；
- 固定 10 条分页与 2/5/10/20 秒有界重试策略已移入 Application；
- ViewModel 只应用合并后的 snapshot，并继续分别维护 visible refresh 与 older loading；
- `ConversationViewportUseCase` 已统一底部锚点、未读清零和变更后的 snapshot 返回；底部状态是否发布的判定也已从 SwiftUI 移入 Core；
- `ConversationLatestRefreshState` 已固定 250ms debounce、silent/visible 优先级、in-flight 合并和单次 pending follow-up；
- ViewModel 继续用 generation 阻止迟到的 viewport Task 覆盖新 UI 状态，并持有实际 Task 的创建、取消和 loading 展示；因此尚未放行 React。

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

当前分层进度：

- `ConversationSendPlanner` 已集中空请求忽略、active Turn guidance、新 Turn 和显式 capability 冲突判定；
- 文本 trim、附件与 capability 的提交快照由 Application plan 固定；
- `ConversationSendUseCase` 已集中 optimistic Turn 构造与 History Store 写入、typed guidance/new-turn command、revision 0 optimistic 回滚和 snapshot 返回；
- 失败草稿恢复规则已移入 Application：仅在当前草稿为空时恢复原文、附件按 ID 去重且失败附件置前、当前 capability 为空时恢复显式选择；
- `guidanceTargetInactive` 自动降级为新 Turn、`isSending`、刷新触发和 UI Task 取消继续由 ViewModel 管理，避免 Application 层持有展示生命周期。

### 4.3 Host ready、刷新与 realtime

源码事实：`ConversationSessionViewModel.swift`

- Host 未 ready 时不启动业务刷新；
- ready 后重新加载 runtime settings、Ask User、历史并启动 realtime；
- unavailable 时取消 realtime、retry、debounce 等任务，并复位 loading 状态；
- realtime 事件经 history store 合并，不允许组件自行拼接出另一套事实；
- 历史分页游标是 ordinal，非法游标必须报错；limit 被限制在 1–100；
- 本地 realtime 来自 Host event hub/reconcile，不是服务端 WebSocket。

`ConversationRealtimePolicy` 已集中 Ask User 优先级、终态/history 刷新、失败信息与忽略事件的判定；ViewModel 只执行对应动作并管理 stream Task。

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
- `clients/macos/Tests/ChatOSCoreTests/ConversationHistoryUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationInteractionPolicyTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationLatestRefreshStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationSendUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationViewportUseCaseTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationTimelineBottomSyncPolicyTests.swift`

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
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskGraphUseCase.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskGraphPollingState.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskInspectorRefreshUseCase.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskRunUseCase.swift`

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

当前分层进度：

- `MessageTaskRunUseCase.loadLatestRun` 已集中首屏最新 40 条规则；
- `MessageTaskRunUseCase.loadOlderEvents` 已集中 50 条分页、offset 与 event ID 去重规则；
- `MessageTaskRunUseCase.loadModelOutput` 已固定不读取事件历史的轻量查询；
- `MessageTaskInspectorRefreshUseCase` 已固定 process/detail/run 三个 section 的刷新查询与陈旧 Run 清理语义；
- SwiftUI ViewModel 只保留选中 Task 的取消、loading 与异步写回校验；
- Inspector 初次加载的请求生命周期与 polling task 生命周期仍在 ViewModel，因此尚未放行 React。

### 5.3 并发、空结果与降级

- 切换 Task 时取消旧 inspector、model output、run 和 load-more 请求；
- 每次异步写回前检查 `requestedTaskID`，旧请求不能覆盖新选择；
- 已经观察到的稳定 Graph 不得被瞬时空响应擦除；
- 初始空 Graph 最多按现有策略重试，不能无限 loading；
- realtime 中断时展示“实时进度连接已中断”错误，并回退 polling；
- polling 只在存在 active task 或正在等待初始空 Graph 时运行；没有活动任务后停止。

当前分层进度：

- `MessageTaskGraphUseCase.loadGraph` 已保证瞬时空响应不能覆盖稳定 Graph；
- `MessageTaskGraphUseCase.shouldRefreshWorkspace` 已固化 session reconcile 与当前 Turn 终态/持久化事件的刷新规则；
- Task 详情加载与 Run 重试已通过 `MessageTaskGraphUseCase` 暴露，Workspace ViewModel 不再直接调用 `MessageTaskGraphServicing`；
- Task 活跃状态规范化与 polling 判定所需语义已移入 Core；
- `MessageTaskGraphPollingState` 已集中空 Graph 三次恢复预算、是否继续 polling 及 600ms/2s/60s 间隔规则；
- cancellable polling Task、Task 选择和 Inspector 请求取消仍由 macOS UI 状态层协调。

### 5.4 测试证据

- `clients/macos/Tests/ChatOSCoreTests/MessageTaskRunUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorRefreshUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphPollingStateTests.swift`
- `clients/macos/Tests/ChatOSAppTests/MessageTaskWorkspaceViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/TaskGraphDeliveryPresentationTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentMessageTaskGraphServiceTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphNormalizerTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphFocusTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskAcceptanceTests.swift`

## 6. Host lifecycle：完整调用链

### 6.1 当前状态所有权

UI 与应用协调：

- `clients/macos/Sources/ChatOSApp/AppModelAuthentication.swift`
- `clients/macos/Sources/ChatOSApp/AppModel.swift`
- `clients/macos/Sources/ChatOSApp/RootView.swift`

Port：`clients/macos/Sources/ChatOSCore/LocalAgentHostLifecycle.swift`

Adapter 与进程传输：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostLifecycle.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostProcess.swift`

当前 macOS 没有单一 lifecycle enum，而是通过 lifecycle task、generation、owner 标记和 error 共同表达状态。Windows 不得据此简化成“进程存在即 ready”。现有关键事实为：

- `localAgentWorkspaceOwnerUserID == authenticatedUserID` 只表示 workspace/project Conversation 服务已配置；
- `localAgentServicesOwnerUserID == authenticatedUserID` 才是 Conversation 等业务服务的 runtime-ready gate；
- `localAgentControlPlaneOwnerUserID` 表示控制面 bootstrap 完成；
- `localAgentHostError` 在 `RootView` 顶部显示，不应以无限 loading 隐藏错误。

### 6.2 启动与账号隔离

```text
authenticated
  → 清空上一个账号的异步任务与缓存
  → startLocalAgentHost
  → 等待上一轮 shutdown
  → reset event hub
  → startForBootstrap(workers disabled)
  → 尝试恢复本地 control-plane snapshot
  → 配置 workspace / project conversation 服务
  → 刷新 workspace
  → authenticated gateway control-plane bootstrap
  → 以 Keychain 凭据启动/复用 worker Host
  → configureLocalAgentServices
  → Conversation runtime ready
```

必须保留：

- 每次启动递增 lifecycle generation，所有异步写回同时校验账号与 generation；
- bootstrap Host 禁用 workers，避免凭据恢复完成前领取 durable work；
- 凭据只进入子进程环境，不出现在命令行、持久化或日志；
- 相同凭据复用运行中的 worker；凭据变化时不擅自杀死正在领取 durable work 的 worker，而是返回 deferred；
- 登出/终止时先让所有 account-scoped service reset，再停止 Host；
- stop 使所有 Conversation 立即进入 runtime unavailable，并取消 realtime/retry/loading。

### 6.3 握手、请求与假健康防护

- 协议版本当前为 `42`；request/response 必须同时匹配 protocol version 与 command ID；
- 启动必须通过 `health`，且 `storage_ready == true` 后才返回 managed process；
- stdio frame 使用 4 字节大端长度前缀，最大 4 MiB；
- 多路复用请求按 command ID 保存 continuation，各自有取消与超时；
- reader 退出或 writer 失败会关闭 transport 并失败所有 pending response；lifecycle 收到传输失败后移除并终止对应 Host，不能留下“进程还在但无法回包”的假健康状态；
- 单次长轮询超时先以 1 秒 health probe 检查同一进程：健康则只返回该请求超时，不连坐其他并发请求；health 失败才使 Host 失效；
- Host 不在运行时发送 recovery notification，并在 startup timeout 内等待恢复；owner 不匹配直接拒绝。

### 6.4 崩溃、睡眠与恢复

- 意外退出发布 `nativeLocalAgentHostDidExit`，清空 active owner 与凭据 fingerprint；
- AppModel 按 250ms、1s、2s、5s、10s 最多重试 5 次；超过上限展示明确错误；
- 系统唤醒会取消睡眠前遗留重试并执行完整 bootstrap，重新从安全存储取凭据；
- health recovery 同时校验账号 generation，旧账号检查不能重启新账号 Host；
- shutdown 串行等待前一轮 shutdown，防止新 Host 与旧清理交叉。

### 6.5 测试证据与目标分层

测试：`clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentHostLifecycleTests.swift`

已覆盖 recovery signal、凭据复用/延迟替换、真实 Rust Host 切换、进程异常退出、健康/失效超时、并发长轮询、停止卡死进程、frame 编码与取消等行为。

macOS 重构应把现有分散事实收敛为可测试的 `HostLifecycleState` 与 `HostLifecycleUseCase`，但迁移后的状态和转换必须逐项对应本节，不能另造一套启动流程。Windows Tauri sidecar 使用同一 Application 状态语义，只替换进程与平台 adapter。

实施进度（2026-10-19）：

- 已新增 `clients/macos/Sources/ChatOSCore/Application/LocalAgentRuntimeState.swift`；
- 已显式建模 `stopped / launching / handshaking / synchronizing / ready / degraded / restarting`；
- AppModel 已按现有启动节点驱动状态，Conversation 创建只在同账号 `.ready` 时放行业务刷新；
- 控制面 service 配置完成前再次校验账号与 lifecycle generation，迟到结果不能把新账号标记为 ready，也不能停止新 generation 的 Host；
- 已增加 generation、owner、恢复、停止和 degraded 状态单元测试；
- 当前仍处于“已映射，可重构”，需要继续把 service orchestration 从 AppModel 提取到 Application use case 后才能升级 React 准入状态。

## 7. Ask User：完整调用链

### 7.1 UI、状态与排序

UI 与状态：

- `clients/macos/Sources/ChatOSApp/Features/Chat/AskUserPromptCardView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Chat/ConversationSessionViewModel+AskUser.swift`

Domain/Port：`clients/macos/Sources/ChatOSCore/AskUserPromptModels.swift`

Adapter：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentAskUserPromptService.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentAskUserPromptMapper.swift`

必须保留的 UI/Feature 语义：

- Prompt 状态为 `pending / ok / canceled / timeout / failed`；只有 pending 可提交；
- Prompt 按 createdAt、updatedAt、ID 稳定升序；未加载到对应 Turn 的 pending Prompt 仍单独展示，不能丢失；
- 每个 Prompt 单独维护 submitting 与 error，提交一个 Prompt 不得锁死全部 Conversation；
- required field 去除首尾空白后不能为空；choice 数量必须位于 minimum/maximum 之间；
- 单选保持一个值，多选达到 maximum 后不能再添加；
- cancel 只有在 `allowsCancel`、pending 且未提交中时可用；
- 状态离开 pending 后立即清空内存中的 secret field；
- 提交/取消失败后保存该 Prompt 错误并重新拉取 Host 真值。

### 7.2 Host 查询、提交与取消

- 仅查询 scope `active` 且 status `waiting_user` 的 Run；服务 limit 归一化到 1–500，单次 Host page 最大 100；
- 只保留 source Conversation 与当前 session ID 相同的 Run；
- Prompt ID 必须为 `local-ask:<run-id>`；resolve 时重新确认 Run 仍为 `waiting_user`，防止过期 UI 提交；
- Prompt 内容来自最新一条 `user_input_requested` event，Host 查询固定 `newestFirst: true`；
- main chat 提交通过 `resumeTurn`，同时携带 expected Conversation version、expected Run version 与状态 `waiting_user`；
- task profile 提交通过 `resumeWaitingRun`；
- main chat 取消通过 `cancelTurn`；非 main chat 只有 owner entity 为 task 时才能 cancel task；
- 提交 metadata 固定包含 `source=ask_user`、prompt ID、values、selection，存在 tool call ID 时一并携带；
- secret 字段或名称含 api key/access token/password/secret/credential 的非空值禁止进入 IPC 或数据库。这一安全限制高于界面控件形态，Windows 必须相同。

### 7.3 测试证据

- `clients/macos/Tests/ChatOSCoreTests/AskUserPromptFormStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationAskUserPromptUseCaseTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentAskUserPromptServiceTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationSessionViewModelTests.swift`

当前分层进度：

- `ConversationAskUserPromptUseCase` 已固定 Conversation 查询上限 100，并提供 typed fetch/submit/cancel；
- Prompt 按 Turn 投影、未挂载 pending 投影、createdAt/updatedAt/ID 稳定排序、submit/cancel 门禁和原位 upsert 已移入 Application；
- `AskUserPromptFormState` 已统一默认值、required trim 校验、choice 数量边界、单选/多选切换、按 option 顺序提交和 resolved 后 secret 清理；
- ViewModel 继续持有 refresh generation、per-prompt submitting/error、失败后重新拉取与 UI Task 生命周期；
- Connector 测试已覆盖 main chat 查询/提交、task source Conversation 与取消、secret 在 IPC 前拒绝；
- UI 目前只绑定 Application form state 并负责视觉展示；完成 per-prompt 异步生命周期提取前仍不放行 React。

## 8. 工具审批与恢复：完整调用链

### 8.1 两层事实与调用链

UI：

- `clients/macos/Sources/ChatOSApp/Features/LocalConnector/GlobalApprovalOverlay.swift`
- `clients/macos/Sources/ChatOSApp/Features/LocalConnector/LocalConnectorApprovalsView.swift`
- `clients/macos/Sources/ChatOSApp/Features/LocalConnector/LocalConnectorControlCenterViewModel.swift`

Domain/Port：

- `clients/macos/Sources/ChatOSCore/LocalConnectorApprovalModels.swift`
- `clients/macos/Sources/ChatOSCore/LocalConnectorApprovalStreaming.swift`
- `clients/macos/Sources/ChatOSCore/LocalConnectorControlServicing.swift`

Adapter/worker：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentPlatformToolWorker.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentToolApprovalHandler.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+Approval.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentToolClient.swift`

审批链路有两层，Windows 不能合并或跳过：

```text
Rust Host durable tool invocation (approval_status = pending)
  → NativeLocalAgentPlatformToolWorker reconciliation
  → NativeLocalAgentToolApprovalHandler
  → project/conversation execution context + safe cwd
  → NativeLocalConnectorService.approvalDecision
  → policy / AI reviewer / session allowlist / Human UI
  → Host decide_tool_approval(expected invocation version)
  → approved claim 才能执行平台工具
```

Host 中的 durable invocation 是恢复事实源；Local Connector 的弹层队列和 continuation 是当前进程内的交互状态。应用重启后必须由 worker 重新扫描 Host pending approvals 并重建 UI，不能只恢复一份前端 loading。

### 8.2 可进入审批的工具与安全上下文

当前 handler 只处理以下平台写操作：

- 项目代码写入工具；
- `execute_command / process_write / process_kill`；
- 远程命令与远程文件上传。

必须先从 run 解析绑定的项目或直接联系人 execution context，再得到 `executionRootURL` 与稳定 workspace scope。`execute_command` 的审批 cwd 来自命令参数 `path`；相对路径以 execution root 解析，绝对路径也必须仍位于 root 内，并拒绝越界、指向 root 外的 symlink、不存在路径和普通文件。其他工具以 root 为 cwd。

缺失有效运行目的、无法生成展示信息或 cwd 不安全时，不弹出一个可被用户误批的空白审批，而是直接以明确原因拒绝 Host invocation。

### 8.3 策略状态机

`LocalConnectorApprovalMode` 的行为必须保持：

- `request_approval`：进入 Human pending queue；
- `auto_approval`：使用本机审批 Agent；模型未配置、runtime 不可用、Agent 要求问用户或执行失败时降级到 Human queue；
- `full_control`：由 policy 自动允许，不进入 Human queue。

切换到非 `request_approval` 模式必须显式确认风险；切到 `auto_approval` 前必须已选择审批 Agent 模型。

Human 决策仅接受当前 approval 的 `availableDecisions`：

- `decline`：本次拒绝；
- `accept`：仅本次允许；
- `acceptForSession`：当前本机会话内允许相同 scope。

session allowlist key 绑定账号、设备、workspace、project root、cwd、声明 scope、purpose 与权限描述。终端等 exact-request scope 还绑定 command/arguments；稳定的 host-declared capability 才能使用 declared-scope 复用。应用/网关 session 结束时清空 allowlist，不能持久化成永久授权。

### 8.4 UI、stream 与并发语义

- `RootView` 全局承载审批浮层，用户切换项目或页面时 pending approval 仍可见；
- 新 approval 到达时自动展开；多个 approval 可前后切换，active index 在队列变化时归一化；
- 命令、purpose、cwd、source、risk、reason 与时间必须完整展示；
- ViewModel 优先消费 snapshot/event stream；有 stream 时每 60 秒做一致性校验，没有 stream 时每 2 秒轮询；
- 一致性查询短暂失败时保留最后可信队列，不闪空；
- 所有异步结果受 lifecycle/action generation 约束，登出后的迟到审批结果不能写回新账号；
- resolve 后重新并发读取 pending queue 与 approval settings；处理中禁用重复操作；
- Local Connector 决策先移除 pending、发布 snapshot、记录最多 1,000 条历史并发布 event，再恢复等待中的 continuation；
- 最终仍由 Host `decide_tool_approval` 以 invocation identity/version 落 durable 状态，UI 本身不能宣称工具已经执行。

### 8.5 worker 恢复与测试证据

worker 在配置时立即订阅 Host event，并开启短期 reconciliation；`wake()`、新的 tool batch event 和启动恢复都会再次触发 approval resolution。审批等待独立于已批准 claim 的执行，不得阻塞其他可运行任务。空闲退避从 250ms 增至最多 2s，同时受 activity window 与 generation 限制。

测试：

- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentPlatformToolWorkerTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentExecutionDirectoryResolverTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeApprovalAgentTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeConnectorStateStoreTests.swift`
- `clients/macos/Tests/ChatOSAppTests/LocalConnectorControlCenterViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/PerformancePolicyTests.swift`

已覆盖等待审批不阻塞已批准 claim、启动/事件恢复、durable approval typed command、cwd/traversal/symlink 边界、AI reviewer 保守降级、session scope 隔离和登出迟到结果隔离。macOS 分层需将“Host durable approval”与“Human presentation queue”明确建模成两个 port，React 才能复用而不复制状态机。

## 9. 31 个页面的准入状态

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
| 29 | 设置 / 命令审批 | 已映射，可重构 | 禁止；等待 macOS 分层 |
| 30 | 设置 / 运行时权限 | 源码映射中 | 禁止 |
| 31 | 设置 / 访问控制 | 未映射 | 禁止 |

Run Inspector 是项目消息中的业务弹层，不对应单独的一级截图；它已在第 5 节完成第一轮源码映射，但仍需先完成 macOS 分层。

## 10. 第一条切片的下一步

按以下顺序实施，不能跳过到 React 页面：

1. 从现有 Swift 行为提取 Host lifecycle、Conversation、Run Inspector、Ask User 与 Approval Application use cases/ports，不改变产品语义；
2. 让 macOS Conversation 与 Run Inspector 仅依赖这些 ports；
3. 用现有测试验证行为未变化，并补齐生命周期与竞态测试；
4. 固定 Rust/Swift/TypeScript fixture；
5. 将本矩阵状态改为 `macOS 分层完成，可进入 React`；
6. 才创建 `clients/windows-react` 下对应的 React feature。

任何新的平台差异必须在本文件新增“差异原因、macOS 原行为、Windows 适配方式、测试与批准记录”后才能实现。
