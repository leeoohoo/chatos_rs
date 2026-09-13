# ChatOS 3.0.2 本地 Agent 最终架构执行计划

## 1. 计划地位

本文是 `3.0.2` 分支的剩余实施清单，目标架构以
`docs/plans/LOCAL_AGENT_RUNTIME_FINAL_ARCHITECTURE.zh-CN.md` 为唯一规范。

本文不引入兼容层、旧数据自动适配、双写、双轨、fallback、feature flag 或临时远程代理。每个阶段必须让最终架构更接近完成，并删除被替代的生产路径；不能只增加新抽象而保留旧执行面。

## 2. 当前真实状态

截至本计划建立时：

- Rust Local Agent 协议、Durable Runtime、Profile、Model Gateway、Provider Context、Memory Sync 和本地 Plugin/MCP 执行主体已经存在。
- SQLite/PostgreSQL 公共 Storage Provider、契约测试、连接配置、导入导出主体已经存在，但尚未覆盖全部客户端业务数据。
- macOS 已接入本地 Host、IPC、Main Chat/Task 创建、事件恢复、Ask User、工具授权、暂停、继续和取消。
- Windows 已完成 Host、账户生命周期、原子恢复、账户级事件泵和 Task Presentation 本地化，但 Main Chat 创建、Ask User、授权、人工复核及 Pet 的剩余远程状态源尚未达到 macOS 等价。
- macOS Task Graph、Run Detail、取消和重试已切换到本地 Host，远程 Service 与 DTO 已删除。
- Task 已完成真正的多 Run/Retry 权威模型，并保留全部历史 Run。
- 旧服务端 Cloud Agent、Task Runner Service、Agent 专用 RabbitMQ consumer/outbox/queue、Mongo Run/状态库、远程 callback/resume 桥、Agent Account/JWT 体系及其管理台和部署配置已经物理删除；Memory Engine 已直接接管无工具模型请求、摘要分块/合并、上下文溢出收缩、瞬时错误重试退避、摘要锁续租和可观测性。
- 旧 Swift `ChatOSAgentRuntime` 仍需在阶段 6 随对应业务 Profile 迁移后物理删除。
- 会话、项目、Notepad、剧情、媒体、插件和设置等现有客户端存储尚未全部迁入统一 Provider。

这些未完成项决定本分支不能被描述为“接近收尾”。后续进度只按本文件的删除与验收门槛计算，不按提交数或新增文件数计算。

## 3. 不可变实施规则

1. `project_id` 从创建到重试、工具调用、结果回传全程冻结透传，不从 UI 当前选择重新推断。
2. Main Chat 与 Task Runner 只能调用同一个 Rust Runtime；Swift/C# 不实现第二套模型循环。
3. Retry 必须创建新 Run 并保留历史 Run，不得实现为 resume、覆盖旧 Run 或调用远程重试。
4. 当前 Storage Provider 是唯一权威执行状态；禁止 SQLite/PostgreSQL 双写或错误时回退。
5. 本地插件/MCP 不经过远程 Task Runner 或服务端 Agent 工具链。
6. 每个垂直切片完成时，同时删除被替代路径、测试、DTO、配置和部署项。
7. 每个切片必须通过定向测试和受影响平台全量测试，独立提交并推送 `origin/3.0.2`。
8. 发现与最终架构冲突的历史实现时直接删除或替换，不新增适配器长期保留旧语义。
9. macOS/Windows 共用的 Rust Runtime、Host、Storage、客户端契约、状态投影规范、黄金夹具和生成逻辑只能放在 `clients/shared`；顶层旧 `crates` 不能成为最终客户端边界，平台目录只保存 UI 与系统 API 适配。

### 当前交付优先级

macOS 是当前首要可运行交付面。阶段 4 的 Windows 原生发行验收保持未完成，
但在继续 Windows 工作前，必须先保证 macOS 可以由一条明确命令启动最终架构
需要的最小后台拓扑、构建并运行签名 App、启动内嵌 Local Agent Host，并完成
登录后的 Main Chat/Task 原生冒烟验收。随后优先推进阶段 5、阶段 6 中的 macOS
存储和旧 Swift Agent Runtime 删除；这不会缩减 Windows 最终等价目标。

## 4. 强制执行顺序

### 阶段 1：关闭 macOS Task 远程闭环

- 将 Task schema 定义为 `initial_run_id + current_run_id + run_ids`。
- 新增类型化本地 Retry Task IPC；同一请求幂等，冲突可检测。
- Retry 原子创建新 Run、更新 Task 当前 Run、保留旧 Run，并复用冻结的项目、模型、Prompt 和 Capability 快照。
- 本地 Task Graph Service 从 Storage/Host 投影 Task、全部 Run、步骤、工具、交互和终态。
- Task Graph/Run Detail 由公共 Rust Host 通过类型化 IPC 输出；`clients/shared` 保存共同契约与黄金夹具，Swift/C# 不各自实现投影逻辑。
- 取消调用本地 Run Control；重试调用 Rust Retry Task。
- Main Chat、Task Workspace、Reply Inspector、Pet UI 全部改用本地服务。
- 删除 `ChatOSMessageTaskGraphService`、远程 retry/cancel/read DTO、API 测试和生产注入。

完成门槛：macOS 生产代码不存在远程 Message Task Graph 调用；旧 Run 可查、当前 Run 唯一、重试后 `project_id` 与全部冻结快照完全不变；Rust、Swift 定向测试和 macOS 全量测试通过。

### 阶段 2：完成 Main Chat 与 Task 的本地结果闭环

- Main Chat turn、创建的 Task、Task 当前/历史 Run 和最终结果使用稳定 ID 建立本地关联。
- Task 进度与最终结果只从本地事件和权威快照渲染。
- 页面关闭、客户端重启和事件重放后关联保持一致。
- 删除 macOS 主聊天中的 Cloud Agent/Task Realtime 状态源、远程任务回调解析和回退路径。

完成门槛：主聊天普通问答与创建项目任务均只经过 Local Host；Task 完成后结果准确回到来源 turn；macOS 生产代码不存在服务端 Agent 状态订阅。

### 阶段 3：完成 macOS Host 与全部原生交互

- 验证并补齐账户切换、Host 崩溃重启、后台继续、更新、登录/退出隔离和事件 cursor。
- 所有 Ask User、工具授权、人工复核、暂停、继续、取消、Memory Sync 状态都精确绑定 Run/Interaction/Invocation。
- Keychain 仅按已知 service/account 精确读写；禁止枚举钥匙串。
- 原生 UI 明确展示失败、待复核、存储不可用和同步待处理状态。

完成门槛：生命周期与恢复用例全部通过；不存在 UI 猜测“最近一条事件”的路由；不出现批量钥匙串授权。

### 阶段 4：完成 Windows 与 macOS 等价接入

- Windows Main Chat 和 Task Runner 接入同一 Rust Host/协议/Profile。
- 补齐 Task/Run 投影、事件订阅、Ask User、授权、人工复核和 Run Control UI。
- 完成 Host 安装、升级、账户隔离、崩溃重启、Named Pipe 身份验证和 DPAPI 精确凭据读写。
- 删除 Windows 远程 Agent/Task Runner 状态与执行路径，不新增 C# Agent Loop。

完成门槛：同一协议用例在 macOS/Windows 得到等价状态与工具行为；Windows 全量测试通过；生产代码无第二套 Loop。

### 阶段 5：统一全部客户端业务存储

- 对 macOS、Windows 全部结构化业务存储建立机器可审计清单。
- 会话、消息索引、附件引用、草稿、Task、项目、Notepad、计划、剧情、媒体元数据、插件状态、MCP 回执、Memory 游标与设置全部改用领域 Repository。
- 删除业务层直接 SQLite/PostgreSQL Driver、JSON 文件业务库和平台专属数据库实现。
- 高级设置完整实现 SQLite 默认与 PostgreSQL 自选；凭据只进入 Keychain/DPAPI。
- 补齐双后端 migration、事务、排序、并发 lease、备份、显式导入导出和故障测试。
- 为 Agent Event、Message 与 Tool Execution 增加按 `owner_user_id + run_id` 的领域查询及双后端索引，Run Detail 禁止长期依赖 owner 全量扫描。

完成门槛：存储审计器对生产代码零违规；全部业务在两个 Provider 上通过同一契约；PostgreSQL 故障明确失败且不创建 SQLite 替代数据。

### 阶段 6：统一审批、剧情及其他旧本地 Agent

- 将审批、剧情和其他使用 `ChatOSAgentRuntime` 的业务改成 Rust Profile/类型化工具回调。
- 删除 Swift `ChatOSAgentRuntime` target、模型客户端、Memory 拼装、重试和 `while true` Loop。
- 确认 Windows 不存在对应 C# Loop 或重复实现。

完成门槛：客户端生产依赖图中不存在旧 Agent Runtime；所有 Agent 业务只注册 Profile。

### 阶段 7：删除服务端 Agent 执行面

- 删除主聊天 Cloud Agent consumer/outbox、专用协议和状态库。
- 删除 Task Runner Service、RabbitMQ Agent 队列、Mongo Agent 状态、worker、模型 phase 和远程 MCP 调度入口。
- 删除相关服务发现、配置键、管理 UI、部署清单、镜像、指标、告警和测试。
- 保留 User Service、Memory Engine、插件管理、模型配置、无状态单步 Model Gateway 和服务端运行配置。

完成门槛：服务端构建与部署图中没有 Agent Loop、Task Runner worker、Agent RabbitMQ/Mongo；Model Gateway 无 Run 状态且一次请求只执行一个 Step。

### 阶段 8：最终验收与发布审计

- 逐项执行最终架构文档第 17 节全部验收矩阵。
- 执行 Rust workspace、macOS、Windows、服务端测试与 lint/build。
- 使用代码搜索和依赖图证明无兼容路由、fallback、双写、旧 Loop、远程插件执行和秘密落盘。
- 验证 OpenAI 原生 compaction、DeepSeek Memory Engine Strategy、600 次长任务、崩溃恢复、副作用未知结果、双数据库并发与账户隔离。
- 更新架构文档、运维文档和发布说明，提交并推送最终结果。

完成门槛：最终架构文档第 18 节 15 条完成定义均有直接证据；任何证据缺失都视为未完成。

## 5. 每次持续执行协议

每次继续工作必须执行以下步骤：

1. 检查 `3.0.2` 工作区、当前在制修改和远端状态。
2. 从尚未通过完成门槛的最早阶段选择一个可验证垂直切片。
3. 实现生产路径与测试，同时删除该切片被替代的旧路径。
4. 运行定向测试；修复后运行受影响范围全量测试。
5. 记录搜索/测试证据，提交并推送 `origin/3.0.2`。
6. 更新本文件的实施记录，再进入下一切片。

不得因为单次执行时间、上下文长度或测试耗时而缩小最终目标。除非需要新的外部授权、用户选择或外部系统恢复，否则持续推进，不以状态汇报代替实现。

## 6. 实施记录

- 2026-09-12：客户端公共 Rust 协议、Storage、Runtime、Profile 与 Host 已从顶层旧目录迁入 `clients/shared/rust`；Cargo workspace 与直接引用已切换到新边界。
- 2026-09-12：Rust Task schema v2 与协议 v12 已实现 `initial_run_id + current_run_id + run_ids`，Retry 原子创建新 Run、保留历史、冻结复用项目/模型/Prompt/Capability 身份，并通过 Runtime/Host 全量测试。
- 2026-09-12：Rust、Swift、C# 已统一到协议 v12；共享 Retry、Task Snapshot、Task Graph 与 Run Detail 黄金夹具由 Rust、Swift 和 Windows 测试共同读取。macOS 当前 Run 投影已支持 Retry，并拒绝历史 Run 迟到事件污染当前状态。
- 2026-09-13：公共 Rust Host 已成为 Task Graph 与 Run Detail 的唯一投影实现；Run Detail 合并 Agent Event、Message 与 Tool Execution，并使用来源前缀生成跨表唯一事件 ID。macOS 的 Task Workspace、Reply Inspector、Pet、Retry 与 Cancel 已全部切到本地 Service；远程 `ChatOSMessageTaskGraphService`、DTO 和测试已删除。
- 2026-09-13：从 `clients/shared` 切断对服务端 `chatos_service_runtime` 的直接依赖；新增客户端公共 `chatos_client_http`，只提供有界 HTTP 响应、错误分类和带进度超时的 SSE 解析，不携带服务发现、内部服务令牌或服务端生命周期。新增边界测试锁定剩余 4 条旧顶层 crate 依赖，后续清单只能缩减、不得扩张。
- 2026-09-13：新增客户端专用 `chatos_memory_client`，只保留 Bearer Token 下的 compose、active summary 和 batch sync，删除 Local Runtime/Host 对旧 `memory_engine_sdk` 的依赖；契约测试锁定无 system key、内部 JWT、隐藏重试和无限响应读取。
- 2026-09-13：新增客户端专用 `chatos_mcp_client` 与项目级 stdio 会话执行器，使用冻结工具白名单完成 initialize、tools/list、tools/call、取消、进程树回收和有界 I/O；删除 Local Host 对旧 `chatos_mcp_runtime` 的依赖，远程 HTTP、队列、内置服务目录和服务端工具路由不进入客户端边界。当前 `clients/shared` 对顶层旧 `crates` 只剩 `chatos_plugin_management_sdk` 一条直接依赖。
- 2026-09-13：新增客户端专用 `chatos_plugin_capability`，Local Host 只消费并验证 exact manifest bytes、Release/Artifact 身份、Ed25519 签名、发布者/Marketplace 身份、平台、权限和就绪组件；目录、偏好、安装工作流、服务 Client 与缓存 DTO 不进入 Host。删除 Local Host 对旧 `chatos_plugin_management_sdk` 的依赖，存储能力契约直接升级到 schema v2，不提供 schema v1 fallback；`clients/shared` 对顶层旧 `crates` 的直接依赖已清零。
- 2026-09-13：macOS Task 入口改为直接使用 Local Host 恢复的 `source_thread_id + source_turn_id + task_id + run_id` 关联；删除 `MessageTaskLookup`、历史消息元数据映射和逐 turn 的远程任务图存在性探测。会话时间线、聚焦路由与任务画布只根据本地 Task 投影决定关联，不再从 `source_user_message_id` 或当前 UI 状态猜测。
- 2026-09-13：删除 macOS 对 `task_runner_callback` / `task_runner_async` 历史消息的解析、模型、排序、状态推断和专用 Reply Inspector；会话历史只映射正式 user/final assistant 消息。Task 进度、终态、结果、重试与详情统一由 Local Host 的 Task/Run 权威投影提供，Pet Quick Chat 不再从历史 callback 恢复任务状态或打开第二套 inspector。
- 2026-09-13：删除 macOS 远程 Conversation/Task Realtime WebSocket、Pet Activity Inbox Client/DTO、WebSocket ticket 和云端 activity disposition；Task Workspace 不再展示服务端实时过程副本。`LocalAgentTaskStateStore` 新增账户级全局更新流，Pet 直接把本地 Task/Run、终态结果和 Ask User 投影为活动，相关来源只保留本地审批、Ask User 与 Task Runner。
- 2026-09-13：删除 macOS `ChatOSTurnProcessService`、远程过程 DTO/Mapper、`TurnProcessViewModel` 及其测试；主聊天“查看过程”直接展示 Local Host 已持久化并恢复到 `ConversationTurn.processEvents` 的模型、工具、人工交互、记忆与 Run 事件。服务端 compact history 的 `processMessageCount` 不再伪造可点击过程节点。
- 2026-09-13：新增 Host 通用、分页、owner-scoped 的 Run Detail 投影，统一从持久 Agent Event、语义 Message 与 Tool Execution 构造 Run 时间线。macOS 登录后先通过 `NativeLocalAgentMainChatRestorer` 恢复所有 Main Chat Turn、精确最终文本、思考/工具过程、Run 控制与待处理人工交互，再启动账户级增量 cursor；已恢复的本地 Turn 不再被服务端 compact history 以更高 revision 覆盖，也不通过从零重放 UI 事件恢复。Run Detail 携带 Host 事务内的 `snapshot_event_sequence`，恢复层跳过已被权威快照覆盖的旧 UI 事件，同时继续接收水位后的增量，避免活跃 Run 在恢复/补放交界处重复追加文本。Task IPC 补齐必传 `initial_run_id`，Rust、macOS、Windows 与共享 v12 夹具统一验证 `initial_run_id == run_ids.first` 且 `current_run_id` 属于完整有序历史；通用 Run Detail 的命令、响应和共享黄金夹具也已在 Rust、Swift、C# 三端对齐。
- 2026-09-13：删除 macOS `/compact-history` 请求、`ChatOSConversationService`、远程 History DTO/Mapper、分页游标、刷新重试和历史同步 UI；`ConversationHistoryStore` 不再接受远程 page/cache/realtime Turn，只接受 Local Host 权威恢复/增量事件与发送前 `revision == 0` 的本地乐观 Turn。发送成功后的用户消息、过程与最终结果均由同一个本地 Run 绑定覆盖，远程 compact history 不再是主聊天加载或结果恢复的数据源。
- 2026-09-13：增加完整重启一致性用例，使用同一个 Local Host 权威快照同时重建来源 Main Chat Turn 与 Task Runner 投影，锁定 `source_thread_id + source_turn_id → task_id → initial_run_id + current_run_id + run_ids → 当前 Run 终态结果` 全链路；历史 Run 缺失、当前 Run 错配或最终结果变化都会直接使恢复失败。会话与任务 ViewModel 的激活顺序统一为“先订阅、再读取快照”，消除订阅窗口内已经落 Store 但 UI 未刷新的竞态。生产路径审计确认 macOS 不存在服务端 Agent/Task 状态订阅、远程 compact history、远程任务回调或结果 fallback，阶段 2 达到完成门槛。
- 2026-09-13：修复 macOS Host 崩溃恢复后的 IPC 端点失效问题：账户级 Event Hub 不再持有启动时的固定 Client，而是在每轮 drain 从 Account Session 解析 Supervisor 当前端点；替换 Host 后从持久 cursor 继续消费并确认事件。Supervisor 新增真实状态流，AppModel 按账户 generation 观察 `.starting/.restarting/.running/.failed/.stopped`，账户切换或退出立即取消旧观察，工作区用明确状态条展示启动、恢复和失败，不再把失联误报为健康。故障切换测试锁定旧/new Client 分别确认连续 cursor `1 → 2`。
- 2026-09-13：补齐 macOS 账户切换隔离：每次绑定新账户 Host 前和退出完成时，强制清空 Main Chat 与 Task Runner 的全部内存投影；`ConversationHistoryStore.reset()` 同时移除已恢复 Turn、未持久化乐观消息、viewport 和交互状态，并通知仍存活的视图订阅立即清屏。账户切换测试锁定旧 Host 停止、旧 access token 删除、旧持久密钥保留、新 token 独立以及旧账户 IPC 拒绝访问；Keychain 生产实现审计确认所有查询均包含 exact service + account 且 `kSecMatchLimitOne`，不存在枚举读取。
- 2026-09-13：新增 `NativeLocalAgentStartupRecovery`，Main Chat 与 Task 在事件消费前必须从同一个 Host Client/生命周期完成原子投影恢复；Host 在分页恢复中关闭连接、socket/write 暂时不可用时，整次恢复等待 Supervisor 当前端点后重做，不混合两代 Host 快照。协议、身份、响应结构和 Host 拒绝等数据错误立即 fail-closed，不作为重试条件。Main Chat Restorer 与 Task Event Sink 改为动态 Client Provider，Task 在 Host 重启后发现新 Run 时也从当前端点读取身份；故障注入测试覆盖旧端点中断、新端点完整恢复，以及无效协议只尝试一次。
- 2026-09-13：移除 macOS Keychain 生产访问对已废弃 `SecKeychainCopyDefault/SecKeychainGetStatus` 的预检查；Authentication 与 Local Agent 凭据现在都直接执行带 `LAContext.interactionNotAllowed` 的 exact SecItem 请求，以单次原子结果判断成功、缺失或锁定，消除检查后再访问的竞态和编译警告。Local Agent 可用性探针也改为 exact service/account 的非交互 SecItem 查询，不枚举 Keychain。
- 2026-09-13：工具授权改为强绑定命令，原生 UI 必须从待授权投影取得同一条 `run_id + invocation_id` 并一起发送；Rust Runtime 在同一事务内读取 Invocation 后再次校验其持久 `run_id`，跨 Run 授权直接失败且不改变状态。macOS Main Chat 与 Task Runner 都通过同一原生控制服务发送该身份对。
- 2026-09-13：原生客户端协议先升级到 v14；暂停、继续和取消命令必须携带 `run_id + expected_version`，Rust Runtime 在写入控制事件前于同一事务内执行 Run version CAS，拒绝陈旧 UI 对后续版本 Run 的控制。Main Chat、Task Runner 和 Ask User 取消入口都先读取权威 Run Snapshot；Task 同时核对 owner identity，Ask User 同时核对当前 `interaction_id`。Rust、Swift、C# 共同消费 Run Control 黄金夹具；v13 夹具已删除，不保留旧协议兼容。
- 2026-09-13：修复 macOS Host 子进程退出观察竞态：启动前注册唯一 `terminationHandler`，由线程安全 continuation 分发退出状态，不再对已退出进程调用可能永久阻塞的 `waitUntilExit()`；Host Process 定向测试连续执行 10 次通过。
- 2026-09-13：唯一原生客户端协议继续升级到 v15；Memory Sync UI 事件的 `run_id` 改为必填，Rust 在同一批次一次扫描权威 `AgentMessage.memory_sync_status`，按受影响 Run 分别聚合 pending/failed 并产生独立事件，不再发送账户级无 Run 事件。Main Chat 与 Task Sink 只按事件携带的确切 Run 路由，跨 Run 聚合测试锁定计数隔离；Task 卡片明确展示待同步和失败数量及错误码，全部同步后隐藏状态。Rust、Swift、C# 共同消费唯一 v15 黄金夹具，v14 不保留。
- 2026-09-13：完成阶段 3 收口审计。人工复核由权威 Run `needs_review` 状态、持久原因和版本绑定 Resume 命令驱动，Main Chat 与 Task UI 均明确显示；Storage Provider 致命不可用由公共 Rust Host 使用稳定进程退出码 `75` 对外表达，macOS Supervisor 不解析 stderr 文案即可在重启期间发布强类型 `.storageUnavailable`，全局状态条明确显示存储不可用与重连次数。共享 Host 测试覆盖 Worker 与 Memory Sync Worker 两条存储失败路径，macOS 真实子进程测试覆盖一般崩溃与存储不可用的不同状态。
- 验证记录：`cargo test -p chatos_local_agent_protocol -p chatos_local_agent_runtime -p chatos_local_agent_host` 全部通过；`swift test --no-parallel --filter LocalAgent` 通过 84 项测试；`swift test --no-parallel --skip NativePluginRuntimeTests` 通过全部选中回归（Swift Testing 186 项 / 56 suites）。旧协议/可选 Memory Sync Run 静态搜索无残留；该切片完成时 Windows v15 尚未在 .NET 环境执行，后续阶段 4 的 Windows 验证记录取代这一限制。
- 2026-09-13：阶段 4 首个切片完成 Windows Host 启动契约升级。删除旧 v3 单帧启动语义，Windows 现在与公共 Rust Host 唯一 v4 契约一致：普通启动帧只含 Credential Manager/DPAPI 引用，第二个有界且同 `launch_id` 的一次性 secret frame 携带精确三项凭据；两帧均在写入后清零，缺失、多余、空值或超限凭据在进程启动前失败。Windows 构建基线同时固定 `Microsoft.Windows.SDK.NET.Ref 10.0.19041.53`，并把 5 处有重载歧义的 collection expression 改为显式 `char[]`；共享 v15 JSON 测试改用 .NET 8 可用的结构比较且不再错误比较随机 `request_id`。
- Windows 验证记录：在隔离临时目录安装 .NET SDK 8.0.402 后，`ChatOS.Connector.Tests` Local Agent 34 项通过、Connector 全量 322 项通过；公共 Rust Host bootstrap/binary 10 项契约测试通过。Windows 原生进程、Named Pipe 和 DPAPI 的 OS 专属行为仍需在 Windows runner 验收，不能由 macOS 测试替代。
- 2026-09-13：阶段 4 第二个切片完成 Windows 账户生命周期生产接入。新增唯一 `WindowsLocalAgentAccountSession`，恢复登录和新登录必须先从当前安全 Token Store 取得凭据、创建或精确读取账户级设备 ID 与 DPAPI 持久密钥、构造 v4 Host 配置并启动 Supervisor，成功后桌面才发布已登录状态；Token 更新重启同一账户 Host，账户切换先停止旧 Host并只删除可替换 access token，退出与主窗口关闭均先停止 Host。IPC Client 不缓存启动时端点，每次从 Supervisor 当前 Named Pipe endpoint 创建。启动或 Client 建立失败时清除半激活 Session 和 Host access token，不保留可执行的残缺状态。
- Windows 类型化运行配置固定 bundled Host 路径、由发行配置提供的 `sha256:` 摘要、账户哈希目录、附件授权目录、平台状态目录、Model Gateway、Memory Engine 与 SQLite 默认 Profile；缺失可信摘要时明确失败，不计算当前磁盘文件作为信任来源，也不 fallback。所有 Credential Manager 删除改为 exact `ResourceName + UserName`，生产代码已不存在 `FindAllByResource` 枚举清理。
- Windows 生命周期验证记录：Local Agent 与 Shell 定向 27 项通过；`ChatOS.Connector.Tests` 全量 331 项、API 45 项、Core 19 项、NetworkGuard 19 项、Presentation 47 项全部通过。macOS 上 Desktop C# 编译已进入 WinUI XAML 阶段，随后因 Windows 专用 `XamlCompiler.exe` 不能在 macOS 执行而停止；Windows 原生 `.exe`、Named Pipe、DPAPI、WinUI 打包与关闭事件仍必须在 Windows runner 最终验收，不能声称已由 macOS 验证。
- 2026-09-13：阶段 4 第三个切片建立 Windows 唯一 Client Runtime 事务边界：登录后的生产顺序固定为 Host 启动、同一 IPC Client/Host lifetime 完整分页恢复、账户级 Event Hub 启动，任一步失败都会停止事件泵、清空投影并退出 Host Session。恢复层同时读取全部 Task、Run、每个 Run 的完整 Run Detail、Main Chat 持久消息绑定和已确认 UI cursor，严格验证 owner、Profile、Task 全历史 Run、冻结 `project_id`、分页推进、事件唯一性及 Main Chat 消息身份后才一次性发布不可变快照。
- Windows Event Hub 独立于聊天页面生命周期；每轮 drain 都从 Account Session 解析 Supervisor 当前 Named Pipe endpoint，页面校验和全部身份解析成功后才原子应用并确认 cursor。Host 在恢复中换代时整个恢复重做，不拼接两代快照；同端点持续不可用有界失败，协议/数据错误立即 fail-closed。每个 Run 的 `snapshot_event_sequence` 阻止权威恢复快照被旧 UI 事件覆盖，确认失败后的同页重放在原生投影内幂等。
- Windows 恢复与事件泵验证记录：新增 6 项定向用例，覆盖 Main Chat/Task/Run/cursor 原子恢复、冻结项目拒绝、当前 Client 换端点、应用后确认、无效事件不确认、恢复换端点整轮重试及非法快照整体回滚；`ChatOS.Connector.Tests` 全量 337 项通过。
- 2026-09-13：Windows Task Presentation 已直接消费唯一 Local Agent Task/Run/Graph 类型，不再映射回远程 `MessageTaskGraph` 领域模型。任务图只按冻结的 `source_thread_id + source_turn_id` 查询；Task Detail 可选择当前及全部历史 Run；Retry 只允许当前终态 Run，并在返回后逐项验证账户、`project_id`、模型配置及修订、Prompt、Capability、Context Strategy 和 Model Runtime Snapshot 均未改变，同时验证新 Run 成为当前 Run且旧 Run 仍在历史中；Cancel 只发送当前 `run_id + expected_version`。账户投影清空时任务面板立即关闭并清屏，Pet 的 Task 取消入口也已切换到同一精确本地 Run Control。
- 本切片同时删除 Windows `IMessageTaskGraphService`、`MessageTaskGraphService`、全部远程 Task Graph DTO/领域模型、API 注入及对应 API 测试，不保留兼容适配器。新增 Connector 契约测试 4 项与 Presentation 行为测试 4 项；`ChatOS.Connector.Tests` 全量 341 项、`ChatOS.Presentation.Tests` 全量 48 项、`ChatOS.Api.Tests` 全量 41 项、`ChatOS.Core.Tests` 全量 19 项通过。macOS 上 Desktop 构建进入 WinUI XAML Compiler 后因 Windows `XamlCompiler.exe` 无法执行而停止，WinUI XAML 和原生行为仍必须由 Windows runner 验收。
- 2026-09-13：修正 Windows 账户级 Event Hub 的权威内容投影：增量事件不再只替换 Run 状态并遗留启动恢复时的旧 Detail，而是从当轮当前 Named Pipe Client 分页取得完整 Run Detail，校验 Run 身份/版本后，将文本、推理、工具过程、终态和 `snapshot_event_sequence` 与 Run Snapshot 一起原子写入账户投影；这样 Main Chat 与 Task UI 消费同一份增量权威内容，不依赖远程 History 或页面级补拉。确认 cursor 仍严格发生在完整 Detail 解析和投影提交之后。Connector 全量 341 项通过，事件泵用例新增断言证明新内容和快照水位已实际替换。
- 2026-09-13：完成 Windows Main Chat 本地闭环。桌面主会话与 Pet Quick Chat 都必须先从当前账户、权威 Workspace Conversation 和本地 Active Project 构造不可变 `account_id + thread_id + project_id + contact_agent_id` Scope；项目首次建会话时立即把同一 Contact/Project 身份写入当前 Workspace 投影，后续发送不再根据 UI 当前选择猜测 `project_id`。发送入口读取服务端模型配置与 Contact Runtime、本地 Project Record，生成与 macOS 语义一致且能通过共享 Rust canonical UTF-8 digest 校验的 Prompt/Capability/Project 冻结快照；附件只写入账户私有 Grant 目录，IPC 仅传 opaque `attachment-grant`、MIME、大小和 SHA-256。Host 创建后必须取得完整 Run Detail 与 Main Chat Binding 并原子写入账户投影后才能向 UI 返回；Host 调用失败精确删除暂存授权，Host 已接管后的 Run 不误删附件。服务层和 UI 都拒绝同 Thread 并发创建第二个 Turn，取消严格绑定 `thread_id + turn_id + run_id + expected_version`。
- 本切片删除 Windows 远程 Conversation History、旧 SQLite Conversation Cache、Conversation WebSocket、远程 Send/Guidance、远程附件上传及其 DTO、领域类型、注入和测试，不保留 fallback 或双写。Pet 尚未迁移的旧聊天活动取消被收窄为只服务 Pet Activity 的 `IPetConversationControl`，Main Chat 无法引用该远程路径。行为测试覆盖冻结 Scope、即时创建投影、最终文本/过程/Task 本地回填、账户清空、附件失败恢复、跨账户/跨 Thread 拒绝、附件授权安全、Unicode canonical digest、会话切换后的迟到失败隔离与精确版本取消。验证：`ChatOS.Connector.Tests` 356 项、`ChatOS.Presentation.Tests` 51 项、`ChatOS.Api.Tests` 34 项、`ChatOS.Core.Tests` 11 项、`ChatOS.NetworkGuard.Tests` 19 项全部通过。macOS Desktop 构建完成 Core/API/Connector/Presentation 编译后进入 WinUI XAML Compiler，因 Windows `XamlCompiler.exe` 无法在 macOS 执行而以 code 126 停止；不据此声称 Windows 原生验收完成。
- 2026-09-13：完成 Windows Ask User 本地闭环。唯一 `WindowsLocalAgentAskUserPromptService` 只从账户级 Local Agent Projection 的 paused Run `pending_interaction` 恢复问题；Main Chat 通过持久 `thread_id + turn_id + message_id` Binding 定位，Task Runner 只接受唯一所属 Task 的当前 Run，并全程冻结账户、来源 Thread、Run、Interaction 与 `project_id`。问题标题、类型、取消/多选约束、选项和视觉引用标识完整投影到 Presentation；提交严格校验空答案、非法/重复选项和选择数量后发送精确 `run_id + interaction_id + answer`，取消发送精确 `run_id + expected_version`。
- 提交或取消后不在 Windows UI 内推测状态，而是通过公共 `WindowsLocalAgentRunProjectionRefresher` 从当前 Named Pipe Host 分页读取完整权威 Run Detail；Main Chat 同时重读并校验 Binding，随后原子替换投影。替换拒绝账户、Profile、Owner、`project_id`、模型配置及修订、Model Runtime Snapshot、Context Strategy、Prompt、Capability、创建时间或 Main Chat 来源变化，并忽略迟到旧版本。会话 Projection 更新会同步刷新 Prompt；账户清空立即清屏；旧会话迟到的 Prompt Fetch 不能污染新会话。
- 本切片删除 Windows 远程 Ask User API Service、注入与 API 测试，不保留 fallback、双写或兼容适配。新增 24 项 Connector/Presentation 定向行为覆盖 Main Chat 与 Task 恢复、字段映射、视觉引用、提交、取消、Host 后刷新、重复 Interaction、跨账户/跨会话拒绝、冻结身份拒绝、同版本非法变化、迟到版本/水位与迟到会话隔离。验证：`ChatOS.Connector.Tests` 377 项、`ChatOS.Presentation.Tests` 54 项、`ChatOS.Api.Tests` 31 项、`ChatOS.Core.Tests` 11 项、`ChatOS.NetworkGuard.Tests` 19 项全部通过。macOS Desktop 构建完成 Core/API/Connector/Presentation 编译后进入 WinUI XAML Compiler，因 Windows `XamlCompiler.exe` 无法在 macOS 执行而以 code 126 停止；Windows 原生 WinUI、Named Pipe 与 DPAPI 行为仍必须由 Windows runner 验收。
- 2026-09-13：完成 Windows Local Agent 工具授权本地闭环。Ask User 与 Tool Approval 现在共同使用唯一 `WindowsLocalAgentRunSourceResolver`，Main Chat 只信任持久 Binding，Task Runner 只信任唯一所属 Task 的当前 Run，不再各自复制来源推断。`WindowsLocalAgentToolApprovalService` 只读取完整权威 Run Detail 中 `awaiting_approval` 的 Tool Snapshot，严格验证账户、来源会话、Run、Invocation、Tool 所属 Run 和重复 Invocation；批准/拒绝仅发送一次性 `run_id + invocation_id + decision + reason`，随后复用公共 Run Refresher 重新读取 Host 权威状态，不在 UI 内伪造成功。
- Main Chat Presentation 新增有界可滚动的工具授权区，明确显示工具名、Effect 风险语义和参数摘要指纹；操作完成后随权威 Projection 消失，账户清空和会话切换都会清除，旧会话迟到查询不能污染新会话。Local Agent Tool Approval 与既有 Connector Command Approval 保持为两个独立安全边界：前者决定 Agent Run 内单次工具调用，后者保护用户/Connector 发起的本机命令；没有删除或复用后者，也没有增加兼容层。此前 Windows 不存在远程 Local Agent Tool Approval 实现，因此本切片没有虚构“旧远程路径删除”。
- 工具授权切片新增 11 项 Connector/Presentation 定向行为，覆盖 Main Chat/Task 映射、批准/拒绝、权威刷新、未知 Decision、已完成/跨会话调用拒绝、重复 Invocation、Tool/Run 身份不一致、UI 风险元数据、操作后移除和迟到会话隔离。验证：`ChatOS.Connector.Tests` 385 项、`ChatOS.Presentation.Tests` 57 项、`ChatOS.Api.Tests` 31 项、`ChatOS.Core.Tests` 11 项、`ChatOS.NetworkGuard.Tests` 19 项全部通过；XAML 已通过 XML 结构校验。macOS Desktop 构建完成 Core/API/Connector/Presentation 后仍因 Windows `XamlCompiler.exe` 无法在 macOS 执行而以 code 126 停止，不能据此声称 Windows 原生 UI 已验收。
- 2026-09-13：完成 Windows 人工复核与统一 Run Control。`WindowsLocalAgentRunControlService` 从同一账户级 Projection 同时投影 Main Chat 与当前 Task Run，明确计算 Pause/Resume/Cancel 权限；`needs_review` 和非 Ask User paused Run 保留 Interaction Kind，并把未知工具结果的 `batch_id` 或 Runtime blocked 原因展示给用户。所有控制在操作前重新解析 `account + conversation + run + version`，发送精确 CAS 命令后复用公共 Run Refresher，不推测下一状态；Ask User paused Run 禁止由通用 Resume 绕过回答。
- Main Chat 新增有界 Run 状态/复核卡，Task Detail 直接展示复核原因及 Pause/Review-and-Resume/Cancel；主聊天原有独立 `CancelTurnAsync` 接口、实现和测试已删除，停止按钮改走唯一 Run Control。Task Detail 的取消也已改走统一服务；Pet 尚未迁移的 Task 控制将在下一切片随 Pet 远程活动源一起删除。新增 9 项 Connector/Presentation 行为覆盖 Main Chat/当前 Task 映射、复核原因、三类精确命令、非法 Ask User Resume、跨会话拒绝、Main Chat/Task UI Resume 与账户清屏；同时删除 1 项旧独立取消测试。验证：`ChatOS.Connector.Tests` 390 项、`ChatOS.Presentation.Tests` 60 项、`ChatOS.Api.Tests` 31 项、`ChatOS.Core.Tests` 11 项、`ChatOS.NetworkGuard.Tests` 19 项全部通过，XAML XML 结构校验通过。Desktop 仍只在 Windows XAML Compiler 的 macOS code 126 平台边界停止。
- 2026-09-13：完成 Windows Pet 全本地闭环。新增唯一 `WindowsLocalAgentInteractionProjection`，Main Chat、Task、Ask User、Tool Approval、Run Control 与 Pet 共用同一套账户/来源/当前 Task Run 映射，不从窗口当前选择推断身份；Pet 直接从账户级 Local Agent Projection 生成 Main Chat、当前 Task、Ask User、Tool Approval、Needs Review 和终态活动，Route 精确保留冻结的 `project_id + thread_id + turn_id + message_id + task_id + run_id + interaction/invocation_id`。Projection 更新或清空会直接刷新/清空 Pet；历史 Task Run 不生成活动；重复 Run、Interaction 或 Invocation fail-closed。
- Pet 的忽略/已处理只写本地、按 Run version 失效的 suppression；Ask User、Tool Approval 与 Needs Review 详情复用唯一原生交互服务，操作后重读 Host 权威投影；所有取消统一走 `ILocalAgentRunControlService`。删除远程 Pet Activity Inbox、Disposition API、Pet WebSocket/Decoder/Ticket、`IPetConversationControl`、`IRealtimeClient`、`PetActivityCoordinator`、旧 Realtime 领域状态及 Task Service 独立 Cancel 接口，不保留兼容、fallback 或双写。
- Windows Pet 切片验证：`ChatOS.Connector.Tests` 393 项、`ChatOS.Presentation.Tests` 60 项、`ChatOS.Api.Tests` 23 项、`ChatOS.Core.Tests` 4 项、`ChatOS.NetworkGuard.Tests` 19 项全部通过；Pet XAML 通过 XML 结构校验，远程类型/注入静态搜索零残留。Desktop 的 Core/API/Connector/Presentation 均完成编译，随后只在 macOS 无法执行 Windows `XamlCompiler.exe` 的 code 126 平台边界停止，未冒充 Windows 原生验收。
- 2026-09-13：按 macOS 首要可运行目标新增 `scripts/local-client-stack.sh`。公共 runner、服务目录和 profile 选择取代第二套复制编排；最小 profile 只启动 Config Center、User Service、Memory Engine API/Worker、Plugin Management、Model Gateway/API shell，以及 Consul、MongoDB、MinIO、RabbitMQ 和 APISIX。启动前停止全部仓库旧进程和未选基础设施，启动后 fail-closed 验证远程 Task Runner、MCP 调度、Local Connector 云端执行、官网及管理台端口均为空，并验证统一网关健康。macOS Debug App 打包审计缺失的两条本地化资源已补齐，App、内嵌 Rust Host 和签名链构建成功。
- macOS 可运行验证：最小栈六个 Host 进程和五个基础设施容器实际运行，`/api/chatos/health` 经 APISIX 返回 200，六个禁用服务端口均无监听；工作区 `.build/ChatOS.app` 进程路径已确认。Swift 回归通过 XCTest 226 项及 Swift Testing 186 项/56 suites；本机随后锁屏，登录后的真实 Host/Main Chat/Task UI 冒烟仍待解锁后完成，不能用进程存活替代。
- 2026-09-13：完成 macOS 登录后内嵌 Host 的真实启动修复。服务端签发的当前 Bearer Token 为 684 字节，旧实现错误复用最多 512 字符的账户身份校验，导致主客户端 `/auth/me` 与模型目录均返回 200 时 Local Agent 仍报告“登录凭据无效”；Token 校验现已独立为非空、无首尾空白、无控制字符且最大 64 KiB，并用 684 字节生产形态回归锁定。随后发现原账户目录生成的 Unix Socket 完整路径为 185 字节，超过 macOS `sockaddr_un.sun_path`；仅将临时 IPC 目录迁到当前用户私有的 `/tmp/chatos-la-<uid>/<32位账户哈希>/`，SQLite、附件授权和平台状态仍保留在账户级 Application Support 目录。Bootstrap 错误同时补齐可读本地化，不再显示无语义“错误 2”。
- macOS Host 现场证据：工作区 Debug App 与内嵌 Host 签名链验证通过；只运行工作区 App 后，`chatos_local_agent_host` 成为其真实子进程，账户级加密 SQLite/WAL 已打开，短路径 Unix Socket 正在监听，Swift Event Hub 已通过 IPC 读取权威事件；UI 中“登录凭据无效”和“正在启动本地 Agent”状态均已消失。Token 本身通过本机 APISIX `/auth/me` 和模型目录双 200 验证且未输出。新增 Token 与 Runtime Configuration 定向测试 9 项通过；完整 Swift 回归首轮 231 项通过、2 项 Plugin Runtime 并发抖动，独立重跑该 Suite 42 项全部通过。
- 2026-09-13：修复 macOS 真实冒烟暴露的三组系统边界问题。APISIX 新增受保护且不改写的 `/api/model-gateway/*` 与 `/api/memory-engine/v1/*` 公网客户端路由；本地栈把运行时 APISIX 配置生成到 `/tmp/chatos-local-dev-<uid>/apisix`，避免外置工作区单文件 bind mount 在容器重建时失效。Local Host 的 Memory 来源固定为 Memory Engine 已正式注册的 `chatos`，真实 `batch-sync` 已返回 HTTP 200。Memory Synchronizer 的所有 clone 通过同一个异步互斥锁协调 claim 到完成的临界区，模型上下文同步不会再把后台 Worker 几毫秒内即成功的 `in_flight` 记录误判成失败暂停。
- 2026-09-13：macOS Debug App 不再默认使用每次构建变化的 ad-hoc 身份，也不会选用仅在钥匙串中“有效”但 OCSP 已吊销的 Apple 证书。新增一次性本地开发签名初始化：证书私钥保存在独立、脚本可解锁且只授权 `codesign` 的开发钥匙串，登录钥匙串只保存用户确认过一次的代码签名信任。稳定自签名身份本身并不能保证重建后的主 App 或包内 Helper 继续满足旧式 Keychain ACL，因此 Authentication 与 Local Agent 生产凭据已分别切到全新 v5/v6 命名空间，并统一由版本化、首次原子安装到用户私有 Application Support 的稳定 `chatos_keychain_broker` 非交互访问；Swift 主进程与 Rust Host 不直接访问 Keychain，后续 App 打包和替换不会覆盖这个 Keychain 访问主体。客户端每次调用前校验 Helper 为当前用户所有、不可被组/其他用户写入、签名有效、identifier 精确且与主 App 使用同一叶证书；Broker 反向校验父进程路径结构、有效签名、主 App identifier 和同一叶证书。生产 service 只接受满足该身份的主进程，测试 service 只接受 `.build` 内测试 Broker，不保留旧凭据读取、fallback 或双写。旧 Keychain 项和旧 Local Agent 数据保持原样但不再读取。
- 本地签名流程同时修复长编译后签名钥匙串被定时重新锁定的问题：脚本在真正 `codesign` 前重新使用本机受限密码文件非交互解锁并固化 partition list，签名结束或失败后主动锁回。现场清理了 `/Applications` 下十一个同 Bundle ID 的并存 ChatOS 包，只保留当前 `/Applications/ChatOS.app` 的 LaunchServices 注册；旧包移入废纸篓的独立可恢复目录，避免 Dock/Spotlight 启动旧实现。
- 本轮验证：Memory Sync 并发契约 6 项、macOS Host Bootstrap 2 项、Windows Memory 来源定向 4 项及 Connector 全量 393 项全部通过；Authentication/Local Agent Keychain 定向测试、Broker 父进程与测试命名空间隔离测试、本地签名 Python 契约通过。从主动锁定签名钥匙串开始的完整 App 打包成功且未启动 `SecurityAgent`。现场把包内 Broker 从 CDHash `5c885e…` 改签为 `da21b5…` 后重新安装，Application Support 中稳定 Helper 仍保持 `5c885e…`，按 Bundle ID 冷启动自动恢复登录并启动内嵌 Host；恢复标准产物再次替换 App 后稳定 Helper 仍未变化，全程没有密码提示。登录后的新 Main Chat/Task 真实模型冒烟仍需完成，不能把本轮基础设施修复记作该验收已通过。
- 2026-09-13：用户现场复测推翻了上一条“无密码提示”的结论。macOS 26 的系统日志明确记录旧式 Keychain 在 ACL partition action `65538` 处为新 Broker 启动 `SecurityAgent`；`LAContext.interactionNotAllowed` 只保证 Data Protection Keychain 静默失败，而本地自签名二进制无法在没有 Apple provisioning profile 的情况下合法携带 Data Protection Keychain 所需受限 entitlement。Broker 现在同时设置旧式 Keychain 的 `kSecUseAuthenticationUIFail`，并切到首次安装后永不覆盖的 `KeychainBrokerV3`。为避免新 Broker 触碰任何旧 partition ACL，Authentication、Local Agent 凭据与本地数据分别直接使用全新 v6、v7 与 `LocalAgentV7`；旧条目和目录不读取、不迁移、不 fallback、不双写。升级后只需重新登录一次 ChatOS 账号，新凭据由 V3 自身创建。现场完成登录后两次冷启动均自动恢复登录并启动内嵌 Host，`securityd` 没有新增 ChatOS/Broker keychain prompt；签名契约 6 项、Swift Authentication/Local Agent/Runtime Configuration 定向测试 7 项和深度签名验证通过。
- 2026-09-13：再次现场复测发现，Authentication v6 的默认 `access-token` account 仍可能命中同一 service 下由历史可执行文件创建的旧 ACL 项，导致用户点击“始终允许”后仍看到下一个旧条目的密码请求。最终实现不读取或迁移该条目：认证 Token 改用同一受限生产 service 下全新的 exact account `access-token-v2`，继续由已经固定且不覆盖的 `KeychainBrokerV3` 创建；Local Agent v7 的设备 ID、持久上下文密钥和 SQLite 加密密钥保持原 account，不通过切换 service 破坏现有 `LocalAgentV7` 数据。真实安装包完成一次登录及随后两次 GUI 冷启动：均未启动 `SecurityAgent`，第二次启动自动恢复登录；Host 始终为同一 PID `43291`，GUI 退出后由 PID 1 接管，重启后通过原 Unix socket 重新附着，未启动第二个 Host、未再次争抢 SQLite。签名契约 6 项、Authentication/Local Agent Keychain 定向测试 6 项、深度签名及 `git diff --check` 均通过。
- 2026-09-13：阶段 7 的旧服务端执行面已完成物理删除。删除 `chatos_cloud_agent_protocol`、`chatos_cloud_agent_runtime`、`task_runner_service/backend`、ChatOS 服务端 Agent Loop/工具执行/远程 Task callback、Memory Engine Cloud Agent bridge、MCP Management Task Runner provider、User Service Agent Account 与 Task Runner/Agent Token、管理台 Task Runner/Agent Account 页面，以及 Compose/APISIX/Prometheus/mTLS/配置中心对应项。Memory Engine 改为直接执行无工具摘要模型请求，保留分块、多轮合并、上下文溢出缩减、按尝试次数递增的瞬时错误退避、Thread/Subject 摘要锁续租和结构化日志；Memory Engine 专属运行实现也已从通用 `chatos_agent` 包迁回自身，仅复用 Plugin Management 发布的稳定 Prompt Key，不再依赖旧 Agent Runtime。不保留兼容、双写、fallback、callback 或 resume 桥。边界审计脚本、服务端构建和管理台构建通过。
- 2026-09-13：补齐阶段 7 的遗漏清理。删除 Docker 镜像发布矩阵中仍会构建已不存在 `task_runner_service_backend` 的目标；顶层 `chatos_agent` 收缩为纯系统 Agent 目录契约，物理删除无人调用的旧 Main Chat、Task Runner、Command Approval 执行器和 Prompt/Memory Loop；删除仓库中遗留的 `.task_runner` 业务状态。Memory Engine 把摘要请求实际需要的网络/流解析错误分类、重试类型与有界指数退避收回 `memory_engine/backend/src/ai`，并移除对旧 `chatos_ai_runtime` 的依赖。MongoDB 仍是 Memory Engine、User Service、Plugin Management 等保留服务的业务存储，RabbitMQ 仍是 Memory Engine 自身摘要任务与其他保留服务的队列，不属于已删除的 Cloud Agent/Task Runner 执行面，不能按名称误删。边界审计新增规则，禁止旧 Agent runtime、旧 Task Runner 镜像和 Memory Engine 对旧 Agent runtime 的依赖重新出现。
- 2026-09-13：继续物理收缩顶层旧 `chatos_ai_runtime`。生产依赖审计确认 ChatOS Model Gateway 与 Plugin Management 只使用无状态模型请求、Provider payload、SSE 解析、错误策略和 Prompt 生成；因此删除旧 Task Loop、Memory Context、Tool Runtime、MCP 执行、持久化回调、Builder 与多轮 Runtime 共 13,800 余行，并移除 `chatos_mcp_runtime`、`memory_engine_sdk`、`async-trait`、`chrono`、`uuid` 等执行面依赖。该 crate 现在只提供无状态模型传输能力，客户端 Agent Loop 仍唯一位于 `clients/shared/rust/chatos_local_agent_runtime`。边界审计禁止这些旧模块和依赖恢复。
- 2026-09-13：物理删除最后的顶层 `agent/` 旧壳及已失效的 Task Runner Service MCP 使用指南。系统 Agent 目录契约迁入 `chatos_plugin_management_sdk`：Main Chat、客户端 Task Runner 和本地命令审批明确标记为 `ClientEmbedded`；五个 Memory Engine Prompt Key 明确标记为 `MemoryEngineOwned` 且无工具能力，仅供 Memory Engine 自己的摘要请求与 Plugin Management Prompt 发布使用。ChatOS、Plugin Management、MCP Management 不再依赖顶层 Agent crate，边界审计禁止该目录和旧服务指南恢复。
- 2026-09-13：清除发布面最后两处旧 Task Runner Service 残留：删除 Drone 中仍会检查已不存在 package 的独立 pipeline，并从官网服务清单、默认端口、展示图和在线状态探针中删除旧服务。客户端本地 Task Runner Profile 保留，它是 `clients/shared` 内的新执行面，不是服务端 Task Runner Service。
- 2026-09-13：把已收缩为纯 Provider HTTP/SSE、payload、解析与重试能力的 `chatos_ai_runtime` 直接改名为 `chatos_model_transport`，并同步 ChatOS Model Gateway、Plugin Management、依赖审计和源码体积审计；不保留旧 package、路径别名或兼容 re-export。Memory Engine 明确禁止依赖该传输包，继续使用自身无工具摘要请求实现。
- 2026-09-13：修复账户 Access Token 轮换会重启 Local Agent Host、从而中断正在执行模型 Step 的生命周期缺陷。唯一原生协议升级到 v16，新增仅通过受保护本地 IPC 传递的 `update_access_token` 命令；Rust Host 使用 `Arc<RwLock<Zeroizing<String>>>` 保存账户 Token，Model Gateway 与 Memory Engine Client 共享同一来源。每个 HTTP 请求在第一次 await 前取得短期零化快照，因此轮换前已发出的请求继续完成，轮换后的请求立即使用新 Token；命令 Debug、Host 错误与平台日志永久脱敏，Token 更新不创建 Run/Event/Outbox，也不写入业务数据库。macOS 与 Windows 更新 Token 后不再重启 Host、Event Hub 或 Projection；IPC 交付失败时停止 Host、清除内存会话与已替换凭据，禁止新旧 Token 执行面并存。
- v16 验证：Rust `chatos_memory_client`、`chatos_local_agent_protocol`、`chatos_local_agent_host` 全部通过，覆盖真实并发 HTTP 请求在轮换边界分别观察旧/新 Token、IPC 精确 JSON、Host 原子更新与 durable mutation 隔离。macOS `NativeLocalAgentIPCClientTests` 11 项、账户 Session 9 项、v16 Fixture 6 项、Host Process 7 项、Supervisor 4 项通过；全量 237 项仅出现既有 Plugin Runtime 并发超时，独立重跑该 Suite 42 项全部通过。Windows 在官方 .NET 8 容器中完成编译并运行 396 项，395 项通过，唯一失败为既有 Network Guard 计时测试；本切片相关 40 项全部通过。现场安装包冷启动后 ChatOS 与 Host PID 在观察期保持不变，且不存在 `SecurityAgent` 密码请求。
- 2026-09-13：阶段 5 的 macOS Project Registry 垂直切片完成。删除 Swift `SQLiteProjectRegistry`、独立 `Projects.sqlite3`、sqlite3 链接与对应测试；唯一原生协议升级到 v17，并在 `clients/shared` 增加 owner-scoped Project CRUD。项目 ID 由客户端创建后冻结，Host 在当前 SQLite/PostgreSQL Provider 内执行查询、创建和 revision CAS 更新，removed 项目不可复活，Provider 失败不回退。macOS 项目列表、重命名、删除、工作区恢复、插件/Agent Project Context 全部只通过类型化 Host IPC，不保留旧 schema、迁移、fallback 或双写。Task Planner 读取同一份 Project Record，从 `workspace_id + relative_root` 冻结目录权限，并继承父 Main Chat Run 已冻结的模型配置，不再维护第二套 Task 项目状态。Rust 协议/Host/Storage、Swift 项目/工作区/Task Graph/v17 夹具全部通过；macOS 全量 246 项只有既有 Plugin HTTP readiness 并发超时，独立重跑通过。
- 2026-09-13：阶段 5 的 macOS Clipboard History 垂直切片完成。删除 Swift 对 `SQLite3`、`clipboard.sqlite` 和 `clipboard_entries` 的全部访问；唯一协议升级到 v18，Host 在当前 Provider 的 `ClipboardRepository` 内完成 owner-scoped 查询、按内容摘要原子去重、revision CAS 置顶/删除以及未置顶记录 30 天/500 条清理。客户端“清空”也只枚举 Host 快照并按 revision 逐条删除，避免无界批量结果撑爆 IPC 帧。文本、URL、文件列表和图片的 payload 字节不进入 IPC 或数据库，仍写入账户哈希隔离的客户端私有 `ClipboardHistoryV2/Payloads/`，Provider 只保存相对引用、摘要、预览和大小；Host 返回已失效引用后由 Swift 删除文件。账户切换不能读取另一账户记录、读取另一账户 payload 或构造跨账户引用，Storage/IPC 失败不回退到本地 SQLite。不读取、不迁移旧剪贴板库。Rust Protocol/Host/Storage 与 Swift Clipboard/IPC/v18 夹具定向测试全部通过；macOS 全量 248 项仅出现既有 Plugin HTTP readiness 并发超时，独立复跑该 Suite 42 项全部通过。
- 2026-09-13：阶段 5 的 macOS Media Studio History 垂直切片完成。唯一原生协议升级到 v19；图片/视频生成记录、可选 `project_id`、`pending/completed/failed` 状态、生成时间、模型、摘要、MIME、字节数和 revision CAS 全部通过当前 Provider 的 `MediaStateRepository` 保存。媒体字节只存在账户哈希隔离的客户端 `MediaStudioV2/Payloads/<owner>/<record>/` 文件边界，Host 只接受 owner + record 目录内的相对引用，Swift 恢复前校验普通文件、非符号链接、大小和 SHA-256。生成请求前先持久化 pending，成功或失败后按 revision 原子更新；更新/删除返回失效引用供客户端回收文件。删除旧 `record.json` 业务库，不读取、不迁移旧 `ChatOSSwift/MediaStudio` 目录，不 fallback、不双写。省略大型 payload 引用的 archive 会整体排除含本地文件引用的 Media 记录，避免导入无法解码的残缺状态。Rust Protocol/Host/Storage、Swift Media/IPC/v19 夹具定向测试全部通过。
- 2026-09-13：阶段 7 最终仓库审计完成。删除遗漏的 Harness Task Runner Service 独立镜像流水线、官网旧服务截图与采集目标，并重新生成 ChatOS API surface/path 基线，使已删除的 Cloud Agent、Task Runner、远程文件/Git/Project 执行路由 CI 真实反映，不再用失效基线掩盖。边界脚本现在直接禁止旧 Harness 流水线、截图和采集地址回归。Memory Engine 仍只在自身目录内拥有无工具模型请求、Responses/Chat Completions 解析、瞬时错误分类、有界退避、分块/合并摘要和记忆提炼；不依赖任何旧 Agent Runtime，不迁入 Agent Loop、工具调用、Run 状态或队列兼容层。Memory Engine 的 MongoDB 记忆存储与 RabbitMQ 摘要任务调度继续保留。
- 2026-09-14：阶段 5 的 macOS Story Studio 垂直切片完成。唯一原生协议直接升级到 v20，Story Project、Story Agent Run 与 Story Media Batch 的权威状态全部进入当前 SQLite/PostgreSQL Provider 的共享 `StoryRepository`；macOS 生产代码只能从账户级 Local Agent Host 注入 `StoryProjectStore`，不再创建默认文件 Store。删除旧 `project.json`、Run JSON、Media Batch JSON 的读取和宽松旧 Story Codable，不读取、不迁移、不 fallback、不双写。Story 修改通过客户端 mutation gate 串行提交短本地事务并使用 Host revision CAS，素材与视频网络请求仍可并发，反向完成不会覆盖状态。媒体字节继续留在账户隔离文件目录；省略大型 payload 的 archive 只排除含本地媒体引用的 Story 记录，保留纯结构化 Story。Rust 合同覆盖 owner isolation、kind/record identity、分页、CRUD 与 revision 冲突；Swift 覆盖账户隔离、重启恢复、并发素材/视频和损坏记录。Rust Protocol/Storage/Host 全量测试、macOS 全量测试 252 项/58 suites、Story Studio 35 项及边界审计全部通过。
- 2026-09-14：阶段 5 的 macOS Notepad 垂直切片完成。唯一原生协议直接升级到 v21，文件夹与笔记全部进入当前 SQLite/PostgreSQL Provider 的共享 `NotepadRepository`；macOS `NativeLocalNotepadService` 只通过账户级 Local Agent Host 执行分页查询、创建、读取、revision CAS 更新与删除。文件夹改名、非递归安全删除和递归删除均由 Host 在单个 Storage 事务内更新整棵路径，失败时整体回滚。删除远程 `ChatOSNotepadService`、`/notepad/*` API 和对应测试，不读取、不迁移、不 fallback、不双写。Rust 合同覆盖 owner isolation、kind/record identity、分页、CRUD、CAS、整树改名、目标冲突回滚及递归删除原子性；Swift 合同覆盖领域映射、查询、精确 revision、文件夹命令和未登录账户隔离。Rust Protocol/Storage/Host 全量测试和架构边界审计通过；macOS 全量 255 项中本切片全部通过，唯一既有 Terminal 异步轮询用例首次抖动后独立复跑 2 项全部通过；Windows v21 IPC 定向 26 项在 .NET 8 容器中全部通过。
- 2026-09-14：阶段 5 的 Project Run Settings 垂直切片完成。唯一原生协议直接升级到 v22，并新增可复用、owner-scoped、64 KiB 有界 JSON 的 Client Setting CRUD；Host 使用当前 `ClientSettingRepository` 执行 revision CAS，不接受非规范 key、陈旧更新或跨账户访问。macOS Project Run 的默认目标、工具链选择、自定义工具链和环境变量只通过账户级 Local Agent Host 保存；删除 `ProjectRunSettings.json` 的生产读取、写入和路径注入，不读取、不迁移、不 fallback、不双写。共享夹具、Rust Host 合同和 Swift 重启恢复合同锁定首次创建、CAS 更新、错误显式返回及账户隔离。验证：Rust Protocol/Storage/Host 全量测试通过；macOS 全量 257 项通过；Windows v22 IPC 定向 26 项在官方 .NET 8 容器中全部通过；Agent/Tool Plane 边界审计、Rust 格式与 diff 检查通过，旧 `ProjectRunSettings.json` 和 v21 协议引用静态搜索零残留。
- 2026-09-14：阶段 7 的远程 MCP 执行面完成物理删除。删除独立 MCP Management Service、`chatos_mcp_management_sdk`、ChatOS 内部 MCP/mTLS 入口、Local Connector 的远程 MCP/Plugin Hook/Terminal Execution/Sandbox Facade handler，以及配置中心、部署、APISIX、Prometheus、管理台和 CI 中对应的服务、路由、密钥、配置与队列投影；不保留旧 URL、调用方、deprecated 配置、兼容别名或数据迁移。Local Connector 只保留客户端插件产物、工作区文件、远程连接和终端连接所需的明确 relay 契约。Memory Engine 中现有的无工具模型请求、Responses/Chat Completions 解析、摘要分块/合并、上下文溢出收缩、瞬时错误分类与有界重试继续作为唯一服务端 AI 总结实现；没有迁入 Agent Loop、Run 状态、工具循环或 RabbitMQ/Mongo 远程执行驱动。MongoDB 仍用于保留服务的业务持久化，RabbitMQ 仍用于 Memory Engine 摘要与 Plugin Management catalog 调度，不属于被删除的远程 Agent/MCP 执行面。
- 2026-09-14：阶段 5 的 macOS Pet 与 Global Utility Preferences 垂直切片完成。Pet 开关、尺寸、通知、跨 Space、收藏项目，以及全局工具开关、隐私项、快捷键和冲突确认被编码为两份类型化、owner-scoped Client Setting，只通过账户级 Local Agent Host 的 `ClientSettingRepository` 读写，当前选择的 SQLite/PostgreSQL Provider 因而自动获得完全相同的行为。登录后才加载，切换账户和退出前提交最新 mutation，未登录、加载失败或持久化失败时 fail-closed 并向设置页显示可重试错误；不读取、不迁移、不 fallback、不双写旧 UserDefaults。新增通用 `NativeLocalClientSettingStore`，以 revision CAS 和 mutation gate 防止旧异步保存覆盖新值，并增加静态边界测试禁止这两个 Store 回归 UserDefaults；同时删除 macOS 已无调用方的 sqlite3 链接。
- 2026-09-14：阶段 5 的 macOS Quick Search 使用历史垂直切片完成。搜索结果的最近使用时间与频次只写入 owner-scoped `quick_search.usage` Client Setting，随当前 SQLite/PostgreSQL Provider、登录账户、切换账户和退出生命周期加载与提交；记录上限固定为最近 512 项，避免无限增长或突破 Client Setting 帧限制。删除旧 `ChatOS.quickSearch.usage` UserDefaults 读写，不读取、不迁移、不 fallback、不双写；存储失败时丢弃未持久化 boost 并在 Quick Search 面板显示明确错误。静态存储边界测试禁止该生产文件重新引用 UserDefaults。
- 2026-09-14：阶段 5 的 macOS Native Connector 秘密存储切片完成。Gateway access token、设备 Ed25519 私钥及远程连接密码/私钥引用等敏感值统一通过受签名身份约束、非交互访问的 `MacOSKeychainBrokerClient` 精确读写独立 Keychain service/account；删除 Application Support 下 `NativeConnector/Secrets` 明文目录、文件权限伪装与测试文件 Store，不读取、不迁移、不 fallback、不双写。远程连接测试改用显式内存 Secret Store，仅测试注入可见；真实 Keychain 合同覆盖保存、替换、删除、锁定时 fail-closed 及非法 account 拒绝。静态存储边界测试禁止明文秘密目录和 POSIX 文件权限实现回归。
- 2026-09-14：完成 Memory Engine 旧 Agent 抽象收口。模型配置解析、已发布摘要 Prompt 校验和无工具模型 Client 被收归独立 `MemoryModelJobRuntime`；删除内部 `ManagedMemoryAgentRuntime` 命名及其与策略控制面的混合职责。架构边界脚本明确禁止旧 Managed Agent Runtime 抽象、Agent Loop、工具执行器和远程执行依赖重新进入 Memory Engine；保留的 MongoDB 记忆存储及 RabbitMQ 摘要队列只负责 Memory Engine 自身业务，不承载客户端 Run 或工具循环。
- 2026-09-14：macOS 终端命令历史退出 `NativeConnectorPersistentState/state.json`。公共协议 v23 新增账户隔离的追加、分页、单条删除和整库清空命令；Local Agent Host 直接使用 `TerminalHistoryRepository`，统一受当前 SQLite/PostgreSQL Provider、事务、归档和 revision CAS 约束，并保留最多 1000 条。`NativeTerminalHistoryStore` 成为 macOS 唯一入口，原生终端、Terminal Relay 与 MCP Terminal 共用，不读取、不迁移、不 fallback、不双写旧 JSON 历史；Rust/Swift 共享 v23 夹具、Host 契约和 macOS Store 测试锁定该边界。
- 2026-09-14：删除 macOS Remote Connection 的旧 Local Connector 路由兼容层。AppModel 只创建一个线程安全 `NativeConnectorRouteStore` 并同时注入 Local Connector 与 Remote Connection；新建或更新连接必须取得当前真实 `device_id + workspace_id`，路由未激活时明确失败。读取、测试和执行已有连接严格使用其已冻结路由，不自动迁移、不静默改写，也不再回退到 `chatos-swift-native-client/local-machine`。静态边界测试禁止旧常量、迁移函数及 fallback 回归。
- 2026-09-14：删除旧 Swift `AgentSettingsStore` 对 v1 重试默认值的自动修补与迁移标记。仍在迁移期使用该 Store 的审批、剧情运行只读取并校验当前保存值，不再把 `2` 静默改写为 `5`，也不写迁移哨兵；静态边界测试禁止该兼容分支回归。该 UserDefaults Store 仍属于阶段 5/6 待删除路径，不因此标记存储统一或旧 Runtime 完成。
- 2026-09-14：完成 macOS Agent Runtime 偏好存储切片。删除 `ChatOSAgentRuntime` 内的 `AgentSettingsStore` 和全部 UserDefaults 读写；新增账户隔离的 `NativeAgentRuntimeSettingsStore`，审批 Agent、剧情 Agent 与设置页共享 `agent_runtime.preferences` Client Setting，随当前 SQLite/PostgreSQL Provider 和账户生命周期加载、提交、清空缓存。旧 UserDefaults 值不读取、不迁移、不 fallback、不双写；存储审计清单移除这一已完成违规项。
- 2026-09-14：完成 macOS Local Connector Runtime/Sandbox 偏好存储切片。开发者模式、Sandbox 开关、权限 Profile、审批 Policy/Reviewer、网络访问策略与 Policy Revision 已从 `NativeConnectorPersistentState/state.json` 删除，统一编码为账户隔离的 `local_connector.runtime_preferences` Client Setting，并只通过当前 SQLite/PostgreSQL Provider 读写。控制中心必须等账户 Host 启动并完成权威加载后才激活；退出、换号或写入失败会清空内存缓存并显式失败，不读取、不迁移、不 fallback、不双写旧 JSON 值。静态边界和 Swift 合同覆盖旧字段永久缺席、重启恢复、账户隔离及未激活 fail-closed。Client Storage/Agent Tool Plane 边界检查与定向测试通过；macOS 全量 271 项仅原有 Plugin HTTP readiness 并发超时，独立重跑该 Suite 42/42 通过。
- 2026-09-14：完成 macOS Local Connector 审批存储切片。审批模式、审批模型选择和 Thinking Level 已从 `NativeConnectorPersistentState/state.json` 删除，统一进入账户隔离的 `local_connector.approval_preferences` Client Setting；审批审计使用新增的 `ApprovalHistoryRepository`，SQLite/PostgreSQL 同步升级到 schema v6，Archive 升级到 v5，Local Agent Protocol 升级到 v24。macOS、Windows 与 Rust Host 使用同一套 append/list 契约；旧 JSON 值不读取、不迁移、不 fallback、不双写。审批偏好或审计存储不可用时 fail-closed，批准操作不得执行，等待中的 continuation 会收到明确拒绝。Rust Client Storage/Protocol/Host、真实 PostgreSQL 16 合同、Swift 协议与 Store、Windows Connector 全量 397 项、静态边界检查通过；macOS 全量 274 项只有原有 Plugin HTTP readiness 并发超时，独立重跑 Native Plugin Runtime 42/42 通过。
- 当前在制：迁移插件安装、版本与启用状态到共享 `PluginStateRepository`，随后删除 `NativeConnectorPersistentState`、`NativeConnectorStateStore` 与 `state.json`；并完成 Main Chat 创建与 Task 创建的真实签名 App 冒烟。阶段 4 的 Windows Host 打包和原生 runner 验收保持未完成，不得标记完成。
- 后续删除旧 Swift `ChatOSAgentRuntime` 时，只允许把仍有价值的单次模型请求、Responses/Chat Completions 解析、摘要分块/合并、上下文溢出收缩、错误分类和有限重试能力收敛到 Memory Engine 自有的 tool-less AI Pipeline；Agent Loop、工具循环、Run 状态和审批不得进入 Memory Engine。完成公共 Rust Profile 接管审批与剧情后，旧 Runtime 目录和测试必须物理删除，不保留兼容适配器。
- 完成状态：阶段 1、阶段 2、阶段 3、阶段 7 已达到完成门槛；阶段 4、阶段 5、阶段 6、阶段 8 尚未达到完整门槛。
