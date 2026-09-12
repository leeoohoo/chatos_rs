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
- Windows 已有 Host 进程、Named Pipe IPC 和 DPAPI 凭据基础，但尚未达到 macOS 业务接入和原生 UI 等价。
- macOS Task Graph、Run Detail、取消和重试已切换到本地 Host，远程 Service 与 DTO 已删除。
- Task 已完成真正的多 Run/Retry 权威模型，并保留全部历史 Run。
- 旧 Swift `ChatOSAgentRuntime`、服务端 Cloud Agent、Task Runner Service 及其队列和状态基础设施仍存在于生产代码。
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
- 验证记录：`cargo test -p chatos_local_agent_protocol -p chatos_local_agent_host`、`swift test --no-parallel --skip NativePluginRuntimeTests`、客户端存储边界审计、Cargo metadata 与静态远程路径搜索全部通过。macOS 全量回归期间发现并修复终端退出状态早于尾部 stdout 落库的竞态，定向连续执行 10 次及全量回归均通过。当前 macOS 主机未安装 .NET SDK，Windows v12 代码和共享夹具测试尚未在 Windows/.NET 环境执行，不能记为通过。
- 当前在制：阶段 3，完成 macOS Host 与全部原生交互。
- 下一切片：审计账户切换、Host 崩溃重启、后台继续、更新与登录/退出隔离的生产生命周期，补齐缺失的失败状态和恢复测试。
- 完成状态：阶段 1、阶段 2 已达到完成门槛；阶段 3—8 尚未达到完整门槛。
