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
| 项目会话准备 | Application 用例、per-project lifecycle 与 Conversation Session composition factory 已提取，继续收口 UI 协调 | `ProjectMessagesView.swift` | `AppModelResources.swift`、`PrepareProjectConversationUseCase.swift`、`ProjectConversationPreparationState.swift`、`ConversationSessionFactory.swift` | `ProjectConversationModels.swift` | `NativeLocalAgentWorkspaceService.swift` | Preparation state/use case、Session factory 及 Connector/ViewModel 测试 | 禁止；等待完整切片验收 |
| 项目工作区列表 | 本地优先两阶段 loader、账号绑定 latest-wins lifecycle 与 await ownership 已提取 | `ResourceSidebar.swift` | `AppModelWorkspace.swift`、`ClientOwnedWorkspaceLoader.swift`、`WorkspaceLoadState.swift` | `ProjectRegistry.swift`、workspace models | `NativeLocalProjectsService.swift`、`NativeLocalAgentWorkspaceService.swift`、`NativeProjectRunService.swift` | Load state 与两阶段 loader 测试 | 禁止；等待项目工作区完整切片 |
| 远程连接列表 | 列表查询/排序用例、账号绑定 latest-wins lifecycle 与 await ownership 已提取 | `ResourceSidebar.swift` | `AppModelResources.swift`、`RemoteConnectionsLoadState.swift` | `RemoteConnectionModels.swift` | `NativeRemoteConnectionService.swift` | Load state/use case 及 Connector 测试 | 禁止；等待远程资源完整切片 |
| 插件应用列表 | 窄 listing Port、列表用例、账号绑定 latest-wins lifecycle 与 await ownership 已提取 | `PluginApplicationsView.swift`、`ResourceSidebar.swift` | `AppModelWorkspace.swift`、`PluginApplicationsLoadState.swift` | `LocalConnectorCatalogModels.swift` | `NativeLocalConnectorService+Plugins.swift` | Load state/use case 及 Connector 测试 | 禁止；等待插件应用完整切片 |
| Conversation 时间线与发送 | History、发送、send/attachment import/runtime settings load/mutation lifecycle、bootstrap/recovery/older-page/viewport/unread/realtime await ownership、刷新合并、realtime 动作、activation session 与 Session composition factory 已提取，其余 UI Task 生命周期待收口 | `ConversationTimelineView.swift` | `ConversationSessionViewModel*.swift`、`ConversationSessionFactory.swift`、Conversation Application 类型 | `ConversationModels.swift`、Conversation ports | `NativeLocalAgentConversationService.swift`、`NativeLocalAgentConversationClient.swift`、Host conversation commands | Conversation Core policy/use case、Session factory、ViewModel 测试等 | 禁止；等待完整切片验收 |
| Task Graph / Run Inspector | Graph、realtime、Run、section 刷新、Inspector request identity、Workspace 基础详情→section 顺序加载、内嵌 Inspector Task 复用/section polling 恢复/presentation/retry identity 与 Application use case 路由、polling session 与 Workspace presentation session 已提取；具体 Task 仍由 UI adapter 持有 | `MessageTaskWorkspaceSheet.swift`、`MessageTaskInspectorView.swift`、`TaskProcessTimelineView.swift` | `MessageTaskWorkspaceViewModel*.swift`、Task Graph/Run/Inspector Application 类型 | `MessageTaskRunModels.swift` 及 task graph models | `NativeLocalAgentMessageTaskGraphService.swift`、Host task/run/event commands | Core state/use case、ViewModel 与 Connector 测试 | 禁止；等待完整切片验收 |
| Ask User | 查询/提交/取消、排序投影、操作门禁、表单状态、refresh/per-prompt 异步生命周期与 activation/action recovery/focus refresh await ownership 已提取 | `AskUserPromptCardView.swift`、Conversation 内嵌入口 | `ConversationSessionViewModel+AskUser.swift`、Ask User Application 类型 | `AskUserPromptModels.swift` | `NativeLocalAgentAskUserPromptService.swift`、Host run/conversation commands | Ask User Core、Connector 与 ViewModel 测试 | 禁止；等待完整切片验收 |
| 工具审批 / 恢复 | Human Approval Port、加载/强类型决策用例、全局队列展示、监控 session、独立页面加载/决定 Task、Host durable/Human 两阶段端口与决定写回恢复 journal 已提取 | `GlobalApprovalOverlayHost`、完整审批页 | `LocalConnectorControlCenterViewModel.swift`、Approval Application 类型 | Local Connector approval models/ports | platform tool worker、approval handler、Local Connector approval service | Core use case/state、worker、approval handler/agent、state store、ViewModel tests | 禁止；等待完整切片验收 |
| Host lifecycle | Runtime ready、控制面 bootstrap、health check 与崩溃重启均已提取 typed lifecycle/await identity，Host service orchestration 继续收口 | `RootView.swift` 与各页面 ready gate | `AppModelAuthentication.swift`、`LocalAgentRuntimeState.swift`、`LocalAgentControlPlaneBootstrapState.swift`、`LocalAgentHostRecoveryState.swift` | `LocalAgentHostLifecycle.swift` | `NativeLocalAgentHostLifecycle.swift`、`NativeLocalAgentHostProcess.swift` | Runtime/bootstrap/recovery state、Host lifecycle 与服务配置测试 | 禁止；先完成 Application use case orchestration |

以上“React 准入”不是进度建议，而是代码审查门禁。

### 2.1 项目工作区列表刷新

- `ClientOwnedWorkspaceLoader` 保持 macOS 的本地优先顺序：先只从 Project Registry 组合本地项目并立即发布，再解析设备身份、修复 root workspace binding，最后刷新远端联系人和 Conversation 关系；
- 未配对时本地项目仍可见；远端关系失败时保留最后可信联系人与 Conversation，不能用空响应覆盖；远端关系不得创建、重命名或复活项目；
- `WorkspaceLoadState` 将 owner、account generation 与 request identity 固定为一个 typed token，新刷新、项目删除、账号切换、登出与应用退出都会使旧 token 失效；
- 刷新 Task 在开始时冻结 Local Projects、Workspace Relations 与 Project Run 服务，各段 registry/Connector/Host await 不强持有 AppModel；每次快照准备、展示写回、错误和 loading 收尾都重新验收 token；
- `NativeProjectRunService` 的 project root 映射跟随每次已验收快照更新；UI 继续只消费 `WorkspaceResourceResolver` 输出，不直接读取 Connector/Host 状态。

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
- per-project preparation token 与 `workspaceAccountGeneration` 在应用 Conversation 前共同验收，阻止过期异步结果覆盖当前账号/项目状态；
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

macOS 分层从现有行为提取，不增加业务含义：

- `PrepareProjectConversationUseCase`
- `ProjectConversationPreparationState`（已完成）
- `ProjectConversationPort`（已完成）
- `ConversationSessionFactory`（已完成）

React 以后只能消费上述稳定用例输出；Tauri adapter 只负责传输与 Windows 平台能力。

实施进度（2026-10-19）：

- 已新增 `ProjectContextLoading` Port，由现有 `NativeLocalProjectsService` adapter 实现；
- 已将项目 Conversation 创建/复用边界固定为 `ProjectConversationPort`，现有 `NativeLocalAgentProjectConversationService` 是 Host adapter，Application use case 不依赖 Connector 类型；
- 已新增 `PrepareProjectConversationUseCase`，按 owner + project 复用并发准备任务；
- 已新增 `ProjectConversationPreparationState`，为每个项目分配 latest-wins token；AppModel 显式持有对应 Task，项目删除只取消目标项目，账号切换/登出/退出统一取消全部任务；
- 项目 Conversation 准备 Task 会在进入 Application use case 前冻结 owner、project、contact、account generation 与 port；Context/Host await 期间不再强持有 AppModel，返回后仍联合验收账号、项目存在性和 per-project token；
- 已新增 App 层 `ConversationSessionFactory`，集中把 Core conversation ports 组合为 SwiftUI session；AppModel 只保留按 session ID 缓存、LRU 淘汰与 activate/deactivate 策略，runtime ready 在每次创建时读取当前账号状态而非冻结在 factory 中；
- Use Case 仍严格执行“先加载当前 project context，再调用 `ProjectConversationPreparing`”的 macOS 顺序；
- AppModel 继续负责登录账号、account generation、项目是否仍存在和四种 UI 状态，迟到结果仍不能写回；
- 登出/账号切换与项目删除会取消对应 Application 任务；
- 已覆盖同账号同项目复用、不同账号绝不复用、不同项目独立 lifecycle、项目级取消/账号级 reset，以及 context 注入后才创建 Conversation 的测试。

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
- `ConversationHistoryRetryState` 已固定 2/5/10/20 秒有界重试预算并绑定当前 activation identity；停用或重新激活后旧 sleep 不得发起新刷新；
- `ConversationViewportMutationState` 已把 timeline anchor 与 mark-read 建模为两条独立 latest-wins lane；ViewModel 持有对应 Task，Conversation 停用会统一取消并失效 token，迟到的本地 snapshot 不得写回不可见页面，而 History Store 真值仍可在下次 snapshot 中恢复；因此尚未放行 React。

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
- `ConversationSendLifecycleState` 已集中发送 request identity；具体 send/recovery Task 由 ViewModel 持有，`guidanceTargetInactive` 在同一 token 内自动降级为新 Turn；
- `ConversationAttachmentImportState` 允许同一 composer generation 内多个文件/图片并发导入；发送开始会失效并取消全部旧导入，迟到的文件读取、图片归一化结果或错误不能写入发送后形成的新草稿；
- runtime unavailable 会立即失效并取消旧发送、恢复 composer，再通过本地 History Store 回滚 optimistic Turn；旧 Host 成功/失败响应不得覆盖恢复后的草稿、时间线或 `isSending`。

### 4.3 Host ready、刷新与 realtime

源码事实：`ConversationSessionViewModel.swift`

- Host 未 ready 时不启动业务刷新；
- Conversation bootstrap Task 由 ViewModel 显式持有，并冻结创建时的 runtime-ready 决策；初始化为 unavailable 后即使 Task 延迟执行，也不得与后续 ready recovery 重复加载 settings/prompts。runtime 转换会失效 bootstrap 的最终 Host 恢复动作，但本地 History Store hydration 仍可独立完成；
- ready 后重新加载 runtime settings、Ask User、历史并启动 realtime；
- unavailable 时取消 realtime、retry、debounce 与发送等任务，回滚 optimistic 发送并复位 loading 状态；
- 每次 Conversation 可见激活都具有独立 activation session；停用后旧 realtime 事件、错误与结束回调不能写回或影响下一次激活；
- activation 触发的 Ask User refresh 外层 Task 由 ViewModel 持有并绑定当前 activation；停用会取消 refresh Task 并只失效 refresh token，不会误取消已经提交的 per-prompt action，迟到 prompt snapshot 不得写回不可见 Conversation；
- latest refresh 的 debounce 与具体 in-flight Task 均由 ViewModel 持有，UI 写回绑定 activation；in-flight closure 不跨远程等待强持有 ViewModel，停用后迟到快照仍可完成 History Store 合并，但不得修改当前 UI、错误、retry 或 loading；新 activation 的合并刷新仍由现有 pending 规则接续；
- older-page 请求由 ViewModel 显式持有并绑定 activation、runtime ready 与 cursor；停用或 unavailable 后即使请求迟到返回，也不得写回时间线、错误或清理新请求的 loading；
- ready 恢复由 `ConversationRuntimeRecoveryState` 分配 latest-wins identity，具体 Task 由 ViewModel 持有；连续 ready 会淘汰旧恢复，异步加载期间若 runtime 再次 unavailable 也会统一取消并失效 identity，迟到恢复不得重新触发历史刷新或 realtime；
- 时间线的 Task Graph 可用性轻量探测由 `ConversationTaskGraphAvailabilityState` 为每个 Turn 分配独立 token；新 revision、Conversation 停用或 runtime unavailable 会取消并失效未完成探测，迟到 Host 响应不得改变入口可见性，ready 后会重试被中断且尚未形成可信缓存的探测；
- 模型、推理等级与远程连接的 optimistic 更新共享 latest-wins mutation identity；新操作会淘汰旧操作，旧成功/失败响应不得覆盖或回滚新选择，也不得结束新操作的 loading；runtime unavailable 会统一取消并失效 mutation；
- runtime settings snapshot load 使用独立 latest-wins identity；新的 bootstrap/recovery load、任一 optimistic mutation 或 runtime unavailable 都会失效旧 load，迟到 settings/models snapshot 不得覆盖新选择或断线后的状态；
- realtime 事件经 history store 合并，不允许组件自行拼接出另一套事实；
- 历史分页游标是 ordinal，非法游标必须报错；limit 被限制在 1–100；
- 本地 realtime 来自 Host event hub/reconcile，不是服务端 WebSocket。

`ConversationRealtimePolicy` 已集中 Ask User 优先级、终态/history 刷新、失败信息与忽略事件的判定；`ConversationSessionActivationState` 已集中可见 activation identity；ViewModel 只执行对应动作、校验 runtime ready 并管理 stream Task。

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
- `clients/macos/Tests/ChatOSCoreTests/ConversationSessionActivationStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationRuntimeSettingsLoadStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationRuntimeSettingsMutationStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationAttachmentImportStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationSendLifecycleStateTests.swift`
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
- Conversation 内嵌 Inspector 的 Task 读取、detail 模型输出与 Run 重试已改由 `MessageTaskGraphUseCase` / `MessageTaskInspectorRefreshUseCase` 执行，ViewModel 不再直接调用 Host service；当前 Task 尚无 Run 时沿用 macOS callback Run 的回退规则，并已固化为用例契约；
- Conversation 内嵌 Inspector 从 process 切换到 detail 时复用当前 Task，仅发起轻量模型输出查询；realtime、polling、显式刷新和目标变化仍先重新查询 Task，以跟随最新 `lastRunID`；
- Conversation 执行过程通过 Host `process` payload mode 读取 durable events；SQLite 只投影 mapper 所需的 tool call identity、状态、原因与错误摘要，移除附件内容和工具参数。macOS 固定每页 25 条、最多 400 页，保持原有 10,000 事件上限且让每页稳定低于 IPC frame 限制；
- `MessageTaskInspectorRequestState` 已统一 Inspector 最新快照与 Run 事件分页的 request identity；同一 Task 重载、section 切换和刷新都会拒绝旧结果，新的快照也会淘汰旧分页；
- `MessageTaskGraphPollingState` 已持有 polling session identity，`MessageTaskWorkspaceLifecycleState` 已持有弹层 presentation session；SwiftUI ViewModel 只保留具体 Task 的取消、realtime 降级协调与 loading 映射，因此尚未放行 React。

### 5.3 并发、空结果与降级

- 切换 Task 时取消旧 inspector、model output、run 和 load-more 请求；
- 选中带 Run 的 Task 时，必须先加载基础 Task 详情，再由完成后的同一快照按当前 section 加载模型输出或 Run；禁止预先启动 section 请求并淘汰基础详情；
- 每次异步写回同时检查 request token 与 `requestedTaskID`；即使相同 Task ID 被重新加载，旧请求也不能覆盖新结果或清理新请求的 loading；
- 已经观察到的稳定 Graph 不得被瞬时空响应擦除；
- 初始空 Graph 最多按现有策略重试，不能无限 loading；
- realtime 中断时展示“实时进度连接已中断”错误，并回退 polling；
- polling 只在存在 active task 或正在等待初始空 Graph 时运行；没有活动任务后停止。
- polling 停止或重启会使旧 session 立即失效；即使底层请求忽略取消并迟到返回，也不得写回 Graph、错误或继续 Inspector 刷新。
- Workspace 弹层关闭会使 presentation session 立即失效，并统一取消 initial load、manual refresh、Run retry、Inspector、realtime 与 polling；旧 session 不得写回或重新启动后台任务。
- Conversation 内嵌 Inspector 的 load、refresh、retry、realtime 与 polling 统一绑定 `MessageTaskInspectorLifecycleState` presentation session；消失时统一取消，重新显示时创建新 generation 并主动 reconcile。切换 Task/Run 会淘汰旧 retry identity、清理旧目标的说明和错误，任何旧 session 的迟到结果都不得写回或为新目标触发刷新。
- Conversation 内嵌 Inspector 在 detail 请求进行中切回 process 时，会取消旧 load generation 并按当前 active Task 重新安排 polling；section 切换不能让活跃任务失去后续刷新。

当前分层进度：

- `MessageTaskGraphUseCase.loadGraph` 已保证瞬时空响应不能覆盖稳定 Graph；
- `MessageTaskGraphUseCase.shouldRefreshWorkspace` 已固化 session reconcile 与当前 Turn 终态/持久化事件的刷新规则；
- Task 详情加载与 Run 重试已通过 `MessageTaskGraphUseCase` 暴露，Workspace ViewModel 不再直接调用 `MessageTaskGraphServicing`；
- Task 活跃状态规范化与 polling 判定所需语义已移入 Core；
- `MessageTaskGraphPollingState` 已集中空 Graph 三次恢复预算、是否继续 polling、600ms/2s/60s 间隔规则及 polling session identity；
- Graph 刷新在请求前后同时校验 workspace refresh generation 与 polling session；停止/重启后的旧结果不会写回，realtime 中断后的降级 polling 仍由当前 session 控制；
- `MessageTaskWorkspaceLifecycleState` 已集中 presentation session identity；初次加载、手动刷新、Run 重试、realtime 与 polling 都必须同时通过当前 session 验收；
- Workspace Run retry 复用 `MessageTaskRetryState` 并绑定 session、Task 与 Run identity；切换 Task 或关闭 Workspace 会取消并失效旧 retry，清理旧目标说明，迟到 Host 结果不得重新加载旧 Inspector、覆盖错误或清理新 retry 的 loading；
- Inspector snapshot/分页验收规则已移入 `MessageTaskInspectorRequestState`，具体 cancellable Task 仍由 macOS adapter 状态层持有；
- Workspace Inspector 冻结请求目标与 Application use case 后再进入 Host await；详情、模型输出、Run 与事件分页请求只在返回后弱引用恢复成功且 token/目标仍有效时写回，因此忽略取消的底层请求不会反向延长 ViewModel 生命周期；
- polling Task 的具体实例与 realtime 降级触发仍由 macOS UI adapter 持有。

### 5.4 测试证据

- `clients/macos/Tests/ChatOSCoreTests/MessageTaskRunUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorRefreshUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorRequestStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphPollingStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskWorkspaceLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskRetryStateTests.swift`
- `clients/macos/Tests/ChatOSAppTests/MessageTaskWorkspaceViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/TaskReplyInspectorViewModelTests.swift`
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

当前 macOS 已用 `LocalAgentRuntimeState` 显式表达业务就绪阶段，但进程任务、账号 generation、owner 标记和 error 仍共同参与协调。Windows 不得据此简化成“进程存在即 ready”。现有关键事实为：

- `localAgentWorkspaceOwnerUserID == authenticatedUserID` 只表示 workspace/project Conversation 服务已配置；
- `localAgentServicesOwnerUserID == authenticatedUserID` 才是 Conversation 等业务服务的 runtime-ready gate；
- `localAgentControlPlaneOwnerUserID` 表示控制面 bootstrap 完成；
- `LocalAgentControlPlaneBootstrapState` 将 owner、account generation、Host lifecycle generation 与 request ID 固定为一个 token；账号切换、Host 重启、强制刷新或新请求会淘汰旧 bootstrap；
- `LocalAgentHostRecoveryState` 为 health check 与延迟重启绑定 owner、account generation、Host lifecycle generation 和 request ID；取消只用于提前结束工作，迟到结果还必须通过 token 验收；
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
- 本地 snapshot 恢复与 authenticated gateway bootstrap 共用冻结 adapter 的服务配置函数；access token、Connector bootstrap 和各业务服务 configure await 不得持有 AppModel，每一段返回后都重新验收 lifecycle/token；
- bootstrap Host 禁用 workers，避免凭据恢复完成前领取 durable work；
- 凭据只进入子进程环境，不出现在命令行、持久化或日志；
- 相同凭据复用运行中的 worker；凭据变化时不擅自杀死正在领取 durable work 的 worker，而是返回 deferred；
- 登出/终止时先让所有 account-scoped service reset，再停止 Host；
- stop 使所有 Conversation 立即进入 runtime unavailable，并取消 realtime/retry/loading。

### 6.3 握手、请求与假健康防护

- 协议版本当前为 `44`；request/response 必须同时匹配 protocol version 与 command ID；
- 启动必须通过 `health`，且 `storage_ready == true` 后才返回 managed process；
- stdio 物理 frame 使用 4 字节大端长度前缀，单帧最大 4 MiB；高位为 continuation 标志，自动重组的逻辑消息最大 64 MiB；
- Host 响应编码超过 64 MiB 逻辑消息上限时返回有界、可重试的 `response_too_large` 错误，并保持传输继续服务后续请求；这属于单请求降级，不等同于 writer 失败；
- 多路复用请求按 command ID 保存 continuation，各自有取消与超时；
- reader 退出或 writer 失败会关闭 transport 并失败所有 pending response；lifecycle 收到传输失败后移除并终止对应 Host，不能留下“进程还在但无法回包”的假健康状态；
- 单次长轮询超时先以 1 秒 health probe 检查同一进程：健康则只返回该请求超时，不连坐其他并发请求；health 失败才使 Host 失效；
- Host 不在运行时发送 recovery notification，并在 startup timeout 内等待恢复；owner 不匹配直接拒绝。

### 6.4 崩溃、睡眠与恢复

- 意外退出发布 `nativeLocalAgentHostDidExit`，清空 active owner 与凭据 fingerprint；
- AppModel 按 250ms、1s、2s、5s、10s 最多重试 5 次；超过上限展示明确错误；
- 系统唤醒会取消睡眠前遗留重试并执行完整 bootstrap，重新从安全存储取凭据；
- health recovery 与延迟 crash retry 同时校验 owner、账号 generation、Host lifecycle generation 和请求 identity，旧结果不能重启新账号或新 generation 的 Host；
- shutdown 串行等待前一轮 shutdown，防止新 Host 与旧清理交叉。

### 6.5 测试证据与目标分层

测试：`clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentHostLifecycleTests.swift`

已覆盖 recovery signal、凭据复用/延迟替换、真实 Rust Host 切换、进程异常退出、健康/失效超时、并发长轮询、停止卡死进程、frame 编码与取消等行为。

macOS 重构应把现有分散事实收敛为可测试的 `HostLifecycleState` 与 `HostLifecycleUseCase`，但迁移后的状态和转换必须逐项对应本节，不能另造一套启动流程。Windows Tauri sidecar 使用同一 Application 状态语义，只替换进程与平台 adapter。

实施进度（2026-10-20）：

- 已新增 `clients/macos/Sources/ChatOSCore/Application/LocalAgentRuntimeState.swift`；
- 已显式建模 `stopped / launching / handshaking / synchronizing / ready / degraded / restarting`；
- AppModel 已按现有启动节点驱动状态，Conversation 创建只在同账号 `.ready` 时放行业务刷新；
- 控制面 service 配置完成前再次校验账号与 lifecycle generation，迟到结果不能把新账号标记为 ready，也不能停止新 generation 的 Host；
- health check 与 crash retry 的并发 identity、五级有界退避、取消和重置已进入独立 Application state；不再只依赖 Task cancellation 或散落整数计数；
- 已增加 generation、owner、恢复、停止、degraded、health check 和重启退避状态单元测试；
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
- `clients/macos/Tests/ChatOSCoreTests/ConversationAskUserPromptLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentAskUserPromptServiceTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationSessionViewModelTests.swift`

当前分层进度：

- `ConversationAskUserPromptUseCase` 已固定 Conversation 查询上限 100，并提供 typed fetch/submit/cancel；
- Prompt 按 Turn 投影、未挂载 pending 投影、createdAt/updatedAt/ID 稳定排序、submit/cancel 门禁和原位 upsert 已移入 Application；
- `AskUserPromptFormState` 已统一默认值、required trim 校验、choice 数量边界、单选/多选切换、按 option 顺序提交和 resolved 后 secret 清理；
- `ConversationAskUserPromptLifecycleState` 已统一 refresh request identity、同 Prompt 操作互斥、跨 Prompt 独立并发，以及 mutation 对旧 snapshot 的淘汰规则；
- ViewModel 只持有具体 action Task 与展示状态；runtime unavailable/deinit 会取消操作，迟到 submit/cancel 结果不能写回；失败仍按 Prompt 保存错误并重新拉取 Host 真值，但对账等待期间不强持有 Conversation，返回后必须同时通过 action 与 refresh token 验收；
- 全局活动的 Prompt 定位使用独立 typed focus token 和 ViewModel 持有的 Task；连续定位只接受最新请求，Prompt Host 等待期间不强持有 Conversation；
- Connector 测试已覆盖 main chat 查询/提交、task source Conversation 与取消、secret 在 IPC 前拒绝；
- UI 目前只绑定 Application form/lifecycle state 并负责视觉展示；完整切片通过跨层验收前仍不放行 React。

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

worker 在配置时立即订阅 Host event，并开启短期 reconciliation；`wake()`、新的 tool batch event 和启动恢复都会再次触发 approval resolution。运行中的 approval reconciliation 会合并新的 wake，并在退出后补跑一次，不能丢失临界事件。审批等待独立于已批准 claim 的执行，不得阻塞其他可运行任务。空闲退避从 250ms 增至最多 2s，同时受 activity window 与 generation 限制。

当前分层进度：

- 已从宽泛的 `LocalConnectorControlServicing` 拆出 `LocalConnectorApprovalServicing`，只暴露 settings、pending queue、mode update 与 resolve；
- `LocalConnectorApprovalUseCase` 已统一 settings + pending 并行加载、mode 风险确认透传、`LocalConnectorApprovalDecision` allowlist 门禁，以及 resolve 后重新读取 durable presentation truth；
- 60 秒 stream 一致性校验与 2 秒无 stream fallback 已移入 Core policy；
- `LocalConnectorApprovalQueuePresentationState` 已固定新审批自动展开、删除时不误展开、active index 边界和前后导航；
- `LocalConnectorApprovalMonitoringState` 已统一 snapshot/event/polling 的监控 session token、迟到更新验收、立即刷新和校验间隔；
- 全局浮层、完整审批页与桌面宠物只展示当前 approval 提供的决定，处理中统一禁用；Native service 对 typed decision 再做一次 allowlist 校验；
- `NativeLocalAgentToolApprovalHandler` 已通过独立 Port 区分 Rust Host durable invocation、任务执行上下文与 Human presentation queue，并验证 Human 结果使用原 invocation identity/version 回写 Host；
- Human/policy 决定在 Host 写回前进入本地有界 journal，键绑定账号、invocation identity 与 version；IPC 写回失败时 reconciliation 只重放原决定，不重复询问用户，应用重启后也可继续写回；Host 成功后立即清理，残留记录不能匹配其他 version；
- `LocalConnectorApprovalActionState` 已将每条 approval 的互斥 request identity 从通用控制操作中分离；不同审批和其他设置操作互不取消，重复点击同一审批只提交一次；
- ViewModel 在登出时取消并等待未完成的审批决定，迟到结果不能污染新账号；完整审批页、全局浮层与桌面宠物按 approval 展示独立进度和错误；
- `LocalConnectorApprovalLoadState` 已为审批页显式 snapshot 加载提供独立 request identity；新刷新淘汰旧结果，切离页面和登出都会取消并拒绝迟到写回，短暂失败保留最后可信 settings/queue；
- ViewModel 继续独立持有实际 snapshot/event stream Task；页面加载取消不会停止全局审批 stream/monitoring，页面错误和重试也不会污染全局浮层。

测试：

- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalQueuePresentationStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalMonitoringStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalActionStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalLoadStateTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentPlatformToolWorkerTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentExecutionDirectoryResolverTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentToolApprovalHandlerTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeApprovalAgentTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeConnectorStateStoreTests.swift`
- `clients/macos/Tests/ChatOSAppTests/LocalConnectorControlCenterViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/PerformancePolicyTests.swift`

已覆盖等待审批不阻塞已批准 claim、启动/事件恢复、durable approval typed command、同审批重复提交门禁、独立审批并发状态、决定中登出、Human 决定写回瞬时失败与重启恢复、journal 存储失败的进程内恢复、cwd/traversal/symlink 边界、AI reviewer 保守降级、session scope 隔离和登出迟到结果隔离。Windows 后续必须复用相同的两阶段状态语义，不能把 presentation queue 当成 durable approval 事实源。

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
