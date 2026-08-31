# 全局宠物悬浮与事件通知实施方案

## 目标

在 ChatOS 进程运行期间提供一个脱离主窗口的 macOS 全局宠物：

- 主窗口最小化、关闭或被其他应用遮挡时仍可见。
- 可跨桌面 Space 展示，并可作为全屏应用的辅助窗口出现。
- 宠物状态由 ChatOS 的真实事件驱动，而不是页面是否打开驱动。
- 能提示本机命令审批、Ask User、计划确认、AI 执行过程、任务完成、失败和阻塞。
- 点击宠物或消息气泡可以回到对应的 ChatOS 上下文。
- 第一阶段只消费已有宠物资源，不包含宠物生成能力。

本方案中的“脱离应用”指脱离 ChatOS 主窗口，但仍由 ChatOS 进程托管。若未来要求 ChatOS 完全退出后宠物仍驻留，则单独增加 Login Item Helper 与进程间通信，不放入当前实现范围。

## 当前代码约束

1. `GlobalApprovalOverlayHost` 和 `VisualSessionOverlayHost` 都挂在 `RootView` 的主窗口层级中，无法在主窗口之外持续展示。
2. `ChatOSRealtimeClient` 当前为每个 conversation 建立一条 WebSocket，并只订阅一个 `conversation` topic。
3. Realtime 消费逻辑分散在 `ConversationSessionViewModel` 和 `MessageTaskWorkspaceViewModel`，未打开相应页面时不能作为可靠的全局状态来源。
4. Swift Realtime DTO 当前只完整处理 `chat_stream` 和 `ask_user_prompt`，尚未把 `task_board`、`task_runner_callback` 映射为稳定的应用领域事件。
5. 本机审批在 `NativeLocalConnectorService` actor 内立即产生，但 UI 目前通过两秒轮询获取 pending approvals。
6. 后端 Realtime 已按登录用户过滤事件，也支持单连接订阅多个 topic；缺少的只是“当前用户全部事件”的 topic。

## 总体结构

```text
Cloud Realtime ──────────────┐
Native Connector approvals ─┤
Recovery snapshots ─────────┤
                             ▼
                  PetOverlayCoordinator
                             ▼
                       PetStateReducer
                   ┌─────────┴─────────┐
                   ▼                   ▼
             PetOverlayStore      AppNavigationRoute
                   ▼                   ▼
        NSPanel sprite + bubble    主窗口精确跳转
```

当前版本由 App 生命周期级 `PetOverlayCoordinator` 直接消费用户级 Realtime 和本机审批流。后续在页面级 Realtime 也迁移到统一总线时，再把该职责下沉为独立 `PetEventCenter actor`，不阻塞全局宠物先投入使用。

## 当前实施状态

已完成：

- `PetActivity`、`PetStateReducer`、事件去重、优先级、多任务计数和临时状态过期。
- 两个透明 `NSPanel`：宠物窗口和消息气泡窗口；主窗口关闭、最小化或切换应用后仍由独立窗口展示。
- 跨 Space、全屏辅助窗口、拖动、位置持久化、显示器变化夹紧和尺寸设置。
- 本机审批 `AsyncStream`：新增、处理、断开时立即推送，原两秒轮询保留为恢复兜底。
- 云端用户级 Realtime topic，以及 AI 过程、Ask User、Task Board、Task Runner 状态到宠物活动的映射。
- 用户级 Realtime 首次连接和每次重连后，从所有已知 conversation 恢复 pending Ask User、运行中任务、阻塞/失败任务和计划确认状态。
- 恢复请求最多并发处理 6 个 conversation，并使用来源版本戳防止旧快照覆盖查询期间到达的新 Realtime 事件。
- 设置页中的总开关、过程通知、完成通知、跨 Space、尺寸和位置重置。
- 点击本机审批提醒打开审批设置；点击云端事件可定位 prompt、turn、task 和 run。
- Swift 与 Rust 单元测试、Swift 应用构建和格式检查。

当前边界：

- 宠物视觉暂用 SF Symbols 占位，真实 v1/v2 atlas 加载器和逐帧动画尚未接入。
- 深链目标不在最新历史页时会继续加载更早消息；如果服务端已经删除对应历史对象，则只返回 conversation 上下文。
- Realtime 断线会自动重连并恢复状态；恢复依据当前 workspace 中已知 conversation 的最近 compact history，尚未进入 workspace 列表的孤立 conversation 不在恢复范围内。
- 当前全局展示依赖 ChatOS 进程存活；ChatOS 完全退出后继续显示需要独立 Login Item Helper。

## 持久化活动 Inbox 与状态机

宠物展示不再把 `UserDefaults` 中的忽略时间当作事实来源。云端活动统一写入 Rust 后端的 MongoDB `pet_activity_inbox` collection，Swift 只能通过 HTTP/WebSocket 访问：

```text
Task Board / Ask User / Task Runner
              │
              ▼
Rust activity projection ── upsert ──► MongoDB pet_activity_inbox
              │                              │
              ▼                              ▼
pet_activity_inbox.updated WebSocket    GET /api/pet-activities
              └──────────────► Swift PetOverlayStore
```

每条记录使用 `(user_id, activity_key, activity_version)` 唯一索引：

- `activity_key` 标识业务对象，例如 `task-runner:<taskID>`、`ask-user:<promptID>`。
- `activity_version` 标识本次运行；Task Runner 优先使用 `runID`。同一任务重新运行会创建新版本，不会被旧版本的忽略或确认状态吞掉。
- `business_status` 保存任务或审批本身的状态；`inbox_status` 只保存用户与消息的交互状态，两者不得混用。

Inbox 状态：

| 状态 | 含义 | 默认展示 |
|---|---|---|
| `unread` | 新活动，尚未被客户端取回 | 是 |
| `displayed` | 至少一个客户端已经展示 | 是 |
| `acknowledged` | 用户点击“知道了” | 否 |
| `ignored` | 用户明确忽略该版本 | 否 |
| `handled` | 用户已在宠物框完成处理、重试或取消 | 否 |
| `resolved` | 业务对象已在其他位置解决 | 否 |
| `expired` | 仅适用于短时动态且已过时 | 否 |

状态规则：

- 完成、失败和阻塞没有自动展示 TTL，保持到用户确认、忽略或处理。
- Ask User、计划确认等业务完成后标记 `resolved`。
- AI/策略自动审批结果属于短时动态，仍按约 5 秒展示，不进入长期待处理队列。
- Swift 获取开放活动时，后端将 `unread` 原子更新为 `displayed`。
- Swift 点击“知道了 / 忽略 / 已处理”分别调用 `acknowledge / ignore / handled` API；本地立即隐藏，写回失败时重新同步恢复。
- 本地缓存只用于断网体验；后端 Inbox 是多设备一致性的唯一事实来源。

## 模块设计

### 1. PetActivity 领域模型

在 `ChatOSCore` 新增稳定的宠物事件模型，不让 UI 直接依赖后端字符串：

```swift
public enum PetActivityKind {
    case idle
    case working
    case reviewing
    case waitingForApproval
    case waitingForUser
    case succeeded
    case failed
    case blocked
    case cancelled
}
```

每条活动必须包含：

- 稳定 activity key：`approvalID` 或 `sessionID + turnID + taskID/runID`。
- 来源：local approval、chat、task board、task runner、project execution。
- 标题和安全摘要。
- conversation、project、turn、task、run 等可用路由标识。
- 是否需要用户处理。
- 是否是终态。
- 事件 ID、sequence 和时间戳，用于去重与排序。

### 2. PetStateReducer

Reducer 保存所有仍然有效的活动，而不是只保存最后一条事件。

聚合优先级：

1. 等待审批、等待用户输入、等待计划确认。
2. 失败或阻塞。
3. 正在执行、思考、调用工具、检查结果。
4. 短暂的完成反馈。
5. idle。

行为规则：

- 一个任务完成但其他任务仍在运行时，短暂播放完成反馈后恢复 working。
- 多个等待项显示数量，不让后到的低优先级事件覆盖审批。
- 高频 delta/tool 事件合并，只保留用户可理解的阶段摘要。
- 审批、Ask User 和阻塞状态不自动消失。
- 完成和普通进度按展示期限自动清理。
- 不展示原始 reasoning、完整敏感命令或凭据。

### 3. PetEventCenter（后续统一总线）

在当前 Coordinator 直连方案稳定后，新增 App 生命周期级 actor，负责：

- 维护一条用户级 Realtime WebSocket。
- 将云端 envelope 解码为 `PetActivityEvent`。
- 接收 Native Connector 审批快照。
- 根据 event ID 去重，并保留有限大小的近期事件集合。
- Realtime 重连后执行恢复查询，重新获取 pending approvals、pending Ask User 和活动任务状态。
- 向页面 ViewModel 和 Pet Coordinator 提供各自过滤后的 AsyncStream。

页面 ViewModel 后续不再自行创建 WebSocket，而是订阅 Event Center 的 conversation stream。迁移期允许旧接口包装在 Event Center 之上，避免一次性重写页面代码。

### 4. 后端 user topic

在 `RealtimeTopicScope` 增加 `User`：

- `User` 不需要 id。
- 每个 envelope 都附加 `User` topic。
- WebSocket 层原有的 `envelope.user_id == auth.user_id` 校验保持不变。
- Swift App Event Center 连接成功后只需订阅 `{"scope":"user"}`。

这样可以覆盖尚未打开的 conversation、新建会话、后台 Task Runner 回调以及项目事件，避免客户端维护大量 conversation topic。

### 5. Realtime DTO 补齐

Swift 解码层增加：

- envelope `project_id`。
- `task_board` 的 `review_id/task_id/action/task/timeout_ms`。
- `chat.task_runner.updated` 中 raw 的 `event`、持久化消息标识和任务元数据。
- Ask User 的 title、message、prompt kind 和 project ID。
- project execution 状态变化所需字段。

所有后端事件先转换为稳定 domain event，再提供给宠物和页面；不在 `PetOverlayView` 中判断字符串。

### 6. 本机审批事件流

Native Connector 同时提供审批快照和审批结果 AsyncStream：

- 新审批 append 后立即 yield。
- resolve、disconnect、重置后立即 yield。
- 新订阅者先收到当前快照。
- 原来的周期 fetch 保留为恢复兜底。
- 用户允许/拒绝、AI 自动批准/拒绝、完全控制策略放行、会话授权复用均产生独立结果事件。
- 结果事件不进入 pending 队列，由宠物映射为约 5 秒的“最近动态”，到期自动移除。

`LocalConnectorControlCenterViewModel` 消费该 stream 并继续发布 `pendingApprovals`。Pet Coordinator 观察同一份状态，不复制审批队列。

### 7. 全局窗口

使用 AppKit 管理两个相关窗口：

#### PetOverlayPanel

- `NSPanel`，`borderless + nonactivatingPanel`。
- 透明背景、无标题栏、非 opaque。
- `level = .floating`。
- `collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]`。
- `hidesOnDeactivate = false`，主应用失焦后仍显示。
- 只在宠物实际区域接收点击，避免透明区域挡住其他应用。
- 支持拖动、屏幕边缘吸附和显示器切换。

#### PetMessagePanel

- 独立透明 nonactivating panel，定位在宠物上方或侧边。
- 普通进度只显示文本；审批和 Ask User 可直接在悬浮框内处理，也保留打开 ChatOS 详情的入口。
- 多个审批、Ask User 和执行中任务以平级事件呈现，当前选中的待处理项展开操作，其余项可切换处理。
- 根据屏幕可见区域自动调整左右方向，避免气泡超出屏幕。

窗口位置以显示器标识和可见区域归一化坐标持久化。显示器拔插、分辨率变化和唤醒时重新夹紧位置。

### 8. 宠物动画资源

复用 Codex v2 宠物图集语义：单元格 `192x208`，8 列，标准动画行如下：

| 行 | 动画 | ChatOS 语义 |
|---|---|---|
| 0 | idle | 无活动 |
| 1 | running-right | 向右拖动 |
| 2 | running-left | 向左拖动 |
| 3 | waving | 单项完成 |
| 4 | jumping | 全部完成 |
| 5 | failed | 失败或阻塞 |
| 6 | waiting | 审批、Ask User、计划确认 |
| 7 | running | AI/任务执行中 |
| 8 | review | 规划、检查、审阅 |
| 9–10 | look directions | 可选的鼠标方向反馈 |

资源加载规则：

- 首版允许没有图集资源，使用系统图标占位，保证事件和窗口链路可独立开发。
- 图集加载一次后切帧缓存，不在每个动画 tick 重新解码。
- pixel art 使用 nearest-neighbor；其他风格允许高质量插值。
- idle 降低帧率，窗口不可见、屏幕锁定或系统睡眠时暂停动画。
- 遵循 Reduce Motion，必要时只切换静态关键帧。

### 9. 导航

当前由 `PetOverlayCoordinator` 把活动交给 `AppModel.openPetActivity`，再转换为一次性 `ConversationFocusRequest`：

- approval → 打开 Settings/Approvals。
- ask user → 选择对应 conversation，并定位到 prompt 卡片。
- task/execution → 打开对应 project/message，定位 turn，自动打开并选中 task/run。
- 无法解析精确目标时至少打开主窗口及对应 project/conversation。

Pet Window Controller 不直接修改 `AppModel.selection`，所有导航仍由 AppModel 和 conversation 页面统一执行。

### 10. 设置

在 Settings 增加“宠物”页面：

- [x] 总开关。
- [ ] 选择已安装宠物。
- [x] 尺寸。
- [x] 展示 AI 执行过程。
- [x] 展示任务完成通知；失败和阻塞始终作为重要提醒展示。
- [x] 跨 Space/全屏显示。
- [ ] 勿扰模式。
- [x] 恢复默认位置。

设置保存在独立 `PetPreferencesStore`，不要继续扩大 `AppModel` 的 UserDefaults 属性数量。

### 11. 常用项目快捷聊天

宠物单击与事件通知使用同一个消息 Panel，但保持两套独立状态，不把聊天入口伪装成 `PetActivity`：

- 项目设置提供“设为常用项目”开关，项目 ID 由 `PetPreferencesStore` 本地持久化。
- 单击宠物打开快捷聊天；拖动超过阈值只移动宠物，不触发单击行为。
- 一级列表固定优先展示联系人“叽咕狸”，其后展示当前工作区内已设为常用的项目。
- 列表只读取当前工作区存在的项目；工作区暂时未加载时不删除本地保存的常用项目 ID。
- 详情复用 `AppModel` 的 `ConversationSessionViewModel` 缓存、历史分页、Realtime 和发送命令，不建立第二套会话状态。
- 快捷详情只展示最近消息和轻量输入框，不复制完整聊天页的附件、模型、Plan、任务图等控制项。
- 快捷聊天打开时任务、审批和 Ask User 活动继续保留；关闭后恢复原通知，不改变其已读、忽略或处理状态。
- 常用项目尚无会话时沿用现有项目会话准备链路，准备完成后自动变为可聊天状态。

## 文件落点

```text
Sources/ChatOSCore/
  PetActivityModels.swift
  PetActivityRecoveryMapper.swift
  PetStateReducer.swift
  LocalConnectorApprovalStreaming.swift

Sources/ChatOSAPI/
  PetRealtimeDTOs.swift
  ChatOSRealtimeClient.swift（接入用户级宠物事件流）

Sources/ChatOSApp/Features/Pet/
  PetOverlayCoordinator.swift
  PetOverlayWindowController.swift
  PetOverlayStore.swift
  PetOverlayView.swift
  PetPreferencesStore.swift
  PetSettingsView.swift
  PetQuickChatView.swift
  PetNavigationRequest.swift

Tests/
  ChatOSCoreTests/PetActivityRecoveryMapperTests.swift
  ChatOSCoreTests/PetStateReducerTests.swift
  ChatOSAPITests/RealtimeDTOTests.swift

Rust backend:
  chatos/backend/src/services/realtime/session_scope.rs
  chatos/backend/src/models/pet_activity_inbox.rs
  chatos/backend/src/repositories/pet_activity_inbox.rs
  chatos/backend/src/services/pet_activity_inbox.rs
  chatos/backend/src/api/pet_activities.rs
```

## 实施顺序

### 阶段 A：可运行的窗口与状态骨架

- [x] 新增 PetActivity 模型和 reducer。
- [x] 新增透明 NSPanel 与占位宠物视图。
- [x] App 启动时创建，认证后显示；退出登录时清空活动。
- [x] 支持拖动和位置持久化。
- [x] 用 reducer 测试验证 idle、working、waiting、success、failed。

### 阶段 B：本机审批闭环

- [x] Native Connector 增加 approval snapshots AsyncStream。
- [x] ViewModel 改为流式更新，保留轮询恢复。
- [x] Pet Coordinator 将 pending approvals 转成持久 waiting 状态。
- [x] 点击气泡可打开现有完整审批页面，也可在悬浮框内直接允许或拒绝。
- [x] AI 自动审批、完全控制模式、会话授权和用户处理结果产生约 5 秒的审批动态。

### 阶段 C：云端全局事件

- [x] Rust Realtime 增加 user topic 及测试。
- [x] Swift 建立 App 生命周期级全局宠物 Realtime 消费链路。
- [x] 补齐 task board、task runner、ask user、AI process 解码。
- [ ] 将旧 conversation realtime API 迁移到统一 Event Center，逐步移除重复 socket。

### 阶段 D：真实动画与路由

- [ ] 实现 v1/v2 atlas loader 与帧缓存。
- [x] 完成所有业务状态到动画语义的映射，当前由系统占位图标呈现。
- [x] 点击事件精确跳转 prompt/turn/task/run；找不到历史目标时回退到 project/conversation。
- [x] 增加 Settings 宠物页面。

### 阶段 E：恢复与质量

- [x] WebSocket 首次连接和重连后的 Ask User、AI turn、Task Runner 与计划确认状态恢复。
- [x] 多任务并发、事件去重、TTL 和优先级测试。
- [ ] 多显示器、全屏 Space、主窗口关闭、睡眠唤醒的人工验收。
- [x] Reduce Motion 和辅助功能标签适配。
- [ ] 使用真实 atlas 后重新检查 CPU 与内存占用。

### 阶段 F：悬浮框内直接处理

- [x] 紧凑消息明确显示后台执行中的任务数量，即使当前主消息是审批或阻塞提醒。
- [x] 展开消息展示全部并行执行任务及最近更新时间。
- [x] 展开消息中的实际任务采用平级任务卡；执行计划/对话过程仅作状态来源，不作为任务父节点重复展示。
- [x] Task Runner 任务可在悬浮框内直接取消，并通过 ChatOS 后端校验消息归属后转发取消。
- [x] 普通 AI 对话执行可在悬浮框内直接停止当前轮次。
- [x] 取消期间禁用重复操作并显示进度；失败时在对应任务旁展示错误。
- [x] 审批、阻塞重试、失败/阻塞忽略均可直接在悬浮框内完成。
- [x] Ask User 可在悬浮框内直接填写普通/密文/多行字段，完成单选或多选并提交或取消。

### 阶段 G：持久化活动 Inbox

- [x] 新增 `pet_activity_inbox` collection 和用户/业务版本唯一索引。
- [x] 新增开放活动查询及 `displayed / acknowledge / ignore / handled` 状态 API。
- [x] Task Board、Ask User、Task Runner 和普通 Chat Stream 通过统一投影写入 Inbox。
- [x] Inbox 更新通过用户级 WebSocket 通知 Swift 重新同步。
- [x] Swift 新增 Inbox DTO/service，恢复时优先读取后端活动，并保留旧历史恢复作为迁移兜底。
- [x] 完成消息提供“知道了”，阻塞提供“忽略/重新处理”，操作写回服务端。
- [ ] 将 project execution 聚合过程也投影进 Inbox，完全移除 compact history 恢复兜底。
- [ ] 为本机审批增加设备事件同步策略；当前本机审批仍由 Native Connector 的本地流负责。
- [x] 多个审批和 Ask User 作为平级待处理项切换，不建立父子包含关系。
- [x] 非待处理审批结果只短暂展示约 5 秒，即使当前策略无需用户审批也能看到。
- [x] 宠物本体不再承载状态角标；事件数量和状态只在消息框中呈现。
- [x] Task Runner 状态以任务图节点当前状态为权威值；运行日志只展示过程，不能把已取消/已完成任务恢复成执行中。
- [x] 存在执行中任务时每 20 秒做一次后台状态校准，修正漏收终态事件造成的陈旧状态。
- [x] 执行组只作为任务容器，不再生成“执行计划正在运行”这类伪任务；悬浮框从任务图节点读取真实任务名与状态。
- [x] 任务流程图总状态同样以节点当前状态优先；节点已阻塞/取消时，旧的运行记录和执行组 metadata 不再显示为运行中。

### 阶段 H：常用项目快捷聊天

- [x] 项目设置增加常用项目开关并本地持久化。
- [x] 宠物单击与拖动手势分流，拖动不打开快捷聊天。
- [x] 快捷列表固定包含“叽咕狸”，并追加当前工作区的常用项目。
- [x] 复用主会话 ViewModel 展示最近消息、Realtime 更新并直接发送。
- [x] 快捷聊天与任务、审批、Ask User 通知状态隔离，关闭后恢复原通知。

## 测试要求

### ChatOSCore

- reducer 优先级。
- 多任务并发与终态清理。
- 完成反馈后恢复 working/idle。
- approval 不被普通 progress 覆盖。
- event ID 去重。

### ChatOSAPI

- user topic subscription payload。
- task board fixture。
- task runner completed/failed/blocked fixture。
- Ask User pending/resolved fixture。
- thinking 不暴露 raw reasoning。

### ChatOSConnector

- 新审批立即发出 snapshot。
- resolve 后 snapshot 移除对应审批。
- disconnect 清空并通知。
- 多订阅者和取消订阅不泄漏 continuation。

### ChatOSApp

- panel 配置包含跨 Space 和 full-screen auxiliary。
- 主窗口 orderOut/关闭时宠物 panel 保持可见。
- 气泡不会在透明区域吞掉其他应用点击。
- 显示器变化后位置仍在 visibleFrame 内。
- 点击事件产生正确导航请求。
- 单击宠物打开快捷列表，拖动宠物不打开列表。
- 常用项目开关持久化，工作区刷新后列表顺序和内容保持正确。
- 快捷会话发送后主聊天页能看到同一条消息，反向 Realtime 更新也能进入快捷会话。

## 验收标准

- 主窗口最小化、关闭和切换应用后宠物仍显示。
- 普通 AI 过程、审批、Ask User、任务完成和失败都能驱动正确宠物状态。
- 未打开对应 conversation 页面时仍可收到任务事件。
- 多任务同时运行时不会因单个完成事件错误进入 idle。
- 审批提醒保持到审批被处理。
- AI 审批、无需审批和已处理审批结果出现约 5 秒后自动消失，不会长期占用悬浮框。
- 不展示原始 reasoning、Secret 或未脱敏命令内容。
- Realtime 重连后能恢复真实 pending 状态。
- 宠物窗口不要求辅助功能或屏幕录制权限。
- 宠物关闭时 ChatOS 主工作区功能不受影响。
- “叽咕狸”始终作为快捷聊天第一项；常用项目按项目列表顺序平级展示。
- 快捷聊天打开期间到达的任务、审批和 Ask User 不丢失，关闭聊天后可继续处理。
