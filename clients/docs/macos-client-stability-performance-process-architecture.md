# ChatOS macOS 客户端稳定性、性能与进程架构方案

更新时间：2026-10-22

状态：强化版提案，待架构决策冻结、分阶段实施与 Release 验收

适用范围：`clients/macos`、`clients/local_agent_host` 以及 macOS 客户端启动的 Plugin/Companion 进程

## 一、执行结论

当前问题不适合用“每个一级功能各启一个进程”解决。推荐采用**选择性多进程 + 单一数据所有者 + 小型 UI 状态域**：

1. `ChatOSSwift` 继续作为唯一原生 UI 进程，负责窗口、导航、轻量展示状态和用户交互。
2. 现有 Rust `chatos_local_agent_host` 继续拥有项目聊天、Task/Run/Event/Tool Invocation 与 `local-agent.sqlite3`，不得再增加第二个直接写入者。
3. 现有 Plugin Application/MCP 子进程和 Global Utilities Companion 保持隔离，不合并回主进程。
4. 媒体链路必须先完成“文件原生化”：下载流式落盘、上传流式读文件、请求 DTO 不再承载大 Base64/`Data`。在此基础上新增 `ChatOSMediaWorker`，将下载、校验、解码、缩略图、帧提取和落盘移出主进程。不能把现有大对象实现原样搬进 Worker。
5. Agent 团队当前不立即迁入 Rust Host。先把 Swift 调度器、Store、Memory 和 Tool Provider 从 `AppModel` 抽成版本化服务边界；只有达到本文定义的进程化门槛后，才落地独立 `ChatOSAgentRuntimeService`。
6. Story Studio 的 Combine 崩溃、Local Agent 数据库写放大、后台并发和 SwiftUI 观察边界必须先修。进程拆分不能替代这些修复。
7. 正式验收只使用 Release 包；Debug 只用于开发定位，不能作为最终性能结论。
8. 新增账户级 `ResourceGovernor`，统一限制 Rust Host、Swift Agent Runtime、Media Worker 和可控 Plugin 的并发与 I/O 压力。每个模块各自设置并发上限仍可能形成乘法式资源争抢。
9. 辅助进程默认只提供故障和内存隔离。只有完成对端身份验证、最小权限、文件句柄授权、代码签名与沙箱设计后，才能称为安全隔离边界。

目标不是追求更多进程，而是让每个高风险工作负载有清楚的所有者、预算、取消语义和故障边界。

## 二、问题范围与非目标

### 2.1 本方案处理的问题

- 项目聊天仍有后台 Run 时，切换 Agent 团队或 AI 创作出现卡顿。
- AI 创作初始化期间发生 `EXC_BAD_ACCESS`。
- Local Agent 数据库与命令回执快速增长，产生 JSON、内存和 SQLite 写放大。
- `AppModel`、Media Studio 和 Story Studio 的观察范围过大。
- 后台调度、页面生命周期、主线程交互优先级缺少统一资源策略。
- 未来引入辅助进程时，避免把当前的大 JSON/Data 复制问题原样搬进 IPC。
- Story 视频参考输入读取完整文件、转换 Base64 并构造大 JSON 请求体造成的上传侧内存放大。
- Media job 在提交、计费、返回 provider job ID、下载和最终落盘之间的崩溃一致性。
- 多个后台系统各自并发合理、合计并发失控的问题。
- Helper、Companion 和本地 IPC 的对端身份、路径授权及同机恶意进程边界。

### 2.2 明确非目标

- 不把每个侧边栏菜单做成一个独立 App。
- 不把 SwiftUI 页面通过远程 View 嵌入主窗口。
- 不为多进程而重写已经稳定的 Rust/Swift 业务逻辑。
- 不允许多个进程直接写同一个 SQLite 数据库。
- 不通过取消用户已经提交的后台 Run 换取表面上的切页流畅。
- 不在本方案中改变 Agent、Plugin 或媒体生成的产品语义。
- 不把“移入独立进程”视为降低总 CPU、总内存或总磁盘写入的证明。
- 不默认承诺 App 退出后媒体或 Agent job 继续运行；该产品语义必须通过本文的生命周期决策明确选择。

## 三、当前源码架构盘点

### 3.1 当前进程拓扑

```text
ChatOSSwift（主进程）
├── SwiftUI/AppKit 窗口、导航、AppModel
├── 会话 ViewModel、MediaStudioViewModel、StoryStudioViewModel
├── Agent 团队 SQLite Store、Scheduler、Heartbeat、Recovery
├── Local Connector 与部分 Platform Tool Worker
├── 启动 chatos_local_agent_host（Rust，stdio IPC）
├── 启动 Plugin Application / MCP 子进程或进程组
└── 再次启动自身 --global-utilities-companion
    └── 截图、录屏、剪贴板、快捷键、快速搜索等全局工具
```

这已经是部分多进程架构，并非单体应用：

- `ChatOSApp.swift` 根据 `--global-utilities-companion` 决定运行主 App 还是 Companion。
- `GlobalUtilityCompanionClient` 使用同一可执行文件创建独立进程，通过小型快照文件和 Distributed Notification 通讯。
- `ManagedLocalAgentHostProcess` 创建 Rust Host，使用非阻塞 stdio、请求 ID 和长度帧通讯。
- `NativePluginApplicationProcessRegistry` 记录 Plugin 进程组，退出时先 `SIGTERM`、超时再 `SIGKILL`，并能清理历史孤儿进程。

### 3.2 主进程当前承担过多长期责任

`AppModel` 是 `@MainActor ObservableObject`，同时持有或协调：

- 导航、语言、字体、项目、联系人、远程连接和 Plugin Application；
- 会话缓存、Realtime、Local Agent Host 生命周期和恢复；
- Agent 团队服务、调度器、Heartbeat、Communication Recovery、Executor Recovery、Artifact Storage；
- Media Studio、Story Studio、Visual Session、Pet、Global Utilities；
- 多组常驻或账户级 Task。

根视图观察整个 `AppModel`。Media Studio 页面又同时观察 `AppModel`、`MediaStudioViewModel` 和 `StoryStudioViewModel`。因此一个与当前页面无关的全局发布也可能扩大为根视图或复杂子树重新求值。

### 3.3 项目聊天与 Local Agent Host

项目聊天的长期事实已主要在 Rust Host：

- Host 由 `NativeLocalAgentHostLifecycle` 管理；
- Swift 使用 `ManagedLocalAgentHostProcess` 通过 stdio 请求/响应；
- 单帧最大 4 MiB，单消息最大 64 MiB；
- App 启动 Host 时传入 Swift 自适应并发：逻辑处理器数量的一半，限制在 1–16；独立运行 Rust Host 且未指定参数时，其默认值限制在 2–8。两者必须分别记录，不能混称一个默认值；
- `local-agent.sqlite3` 保存 Run、Event、Tool Invocation、Command Receipt 等数据。

页面离开时，`ConversationSessionViewModel.deactivate()` 会取消 Realtime、历史刷新、分页和视口任务，但不会取消已经持久化并提交给 Host 的后台 Run。这符合“切页不丢任务”的产品语义，却要求后台任务降低优先级并受资源预算约束。

### 3.4 Agent 团队仍属于主进程后台运行时

当前 Agent 团队实现是 Swift 原生运行时：

- `SQLiteAgentGroupChatStore` 是 actor，直接拥有 `AgentGroupChat.sqlite3` 连接；
- `LocalAgentGroupChatScheduler` 在 macOS 客户端内消费 durable delivery queue；
- `AppModelAgentRuntime` 启动 Communication、Executor Recovery、Heartbeat 和 Artifact Storage Task；
- `NativeAgentGroupChatService.changes()` 只是进程内有界失效通知，SQLite 才是事实源；
- Agent 团队 UI 进入页面后创建 Workspace ViewModel，并订阅变化后重新读取 Store。

另外，`AppModelAgentRuntime` 的 Communication recovery 使用 `batchSize = 64`，`LocalAgentGroupChatSchedulerDrain.runClaimedWork()` 对已 claim 的 work 分别创建 `Task`。批次大小不能充当执行并发上限：必须另设全局 in-flight 限额，并限制提前 claim 数量，防止任务等待 permit 时 lease 已过期。只降低 Rust Host 并发无法约束这条 Swift 执行链路。

这部分虽然不一定运行在 MainActor，但 CPU、内存、数据库页缓存和崩溃边界仍与主 App 共享。另一方面，它依赖 Swift `AgentRuntime`、Memory、Local Projects、Plugin Capability Provider 和 Local Connector。直接迁入 Rust Host 会形成高成本重写，不适合作为第一阶段修复。

### 3.5 AI 创作和媒体链路仍属于主进程

当前媒体服务具有以下特点：

- `AppModel` 构造时立即创建 `MediaStudioViewModel` 和 `StoryStudioViewModel`；
- 登录成功时调用 `mediaStudio.activate(userID:)`，而不是进入 AI 创作页面时才激活；
- `StoryStudioViewModel` 同时管理项目目录、规划、历史、图片、首尾帧、视频、批处理和帧提取；
- Story Studio 当前有 30 余个 `@Published` 状态和多组 Task 字典；
- 视频下载上限为 512 MiB，结果以 `VideoGenerationResult.videoData: Data` 返回主进程后再落盘；
- 上传侧 `StoryVideoGenerationAdapter` 也会读取最大 512 MiB 的完整参考视频，并调用 `base64EncodedString()`；`MediaGenerationModels` 的参考媒体 DTO 持有 Base64 字符串，服务层还会解码和组装请求。512 MiB 输入仅 Base64 就约 683 MiB，与原始 `Data` 并存已超过 1 GiB，尚未计入 JSON 和网络缓冲；
- 图片 provider 响应、Base64 解码、媒体历史和多个文件 Store 也位于当前 App 生命周期内；
- 部分图片解码、导入和帧提取已经使用 detached work，这是正确方向，但仍不能提供进程级内存回收和故障隔离。

媒体链路具备独立进程的典型条件：大内存、长任务、文件型输出、可恢复、可以使用粗粒度协议，并且不需要直接访问 SwiftUI 状态。

## 四、已确认的故障与性能事实

### 4.1 已确认崩溃位置，根因仍需验证

2026-10-21 收集到 11 份 `ChatOSSwift` 崩溃报告，均为：

- `EXC_BAD_ACCESS` / `SIGSEGV`；
- `KERN_INVALID_ADDRESS at 0x0`；
- 主线程 `objc_msgSend`；
- `Published.withMutation`；
- `StoryStudioViewModel.isLoading.setter` 或 `activeAssetGenerations.setter`。

典型调用链：

```text
AppModel.applyAuthenticationPhase
→ MediaStudioViewModel.activate
→ StoryStudioViewModel.activate
→ @Published setter
→ Published.withMutation
→ objc_msgSend
→ EXC_BAD_ACCESS
```

这些报告证明故障发生在 Story Studio 的 Combine/`@Published` 发布路径；发布器或对象生命周期是优先验证的假设。仅凭栈不能排除更早的内存破坏、竞争或框架问题，也不能宣称根因已经确认。报告不属于直接的 OOM 终止；内存压力仍可能通过时序和资源争抢参与触发。

当前工作区已有两处止血修改：显式保存稳定 `ObservableObjectPublisher`，以及首次激活不再对全新对象执行冗余 reset。这些修改仍需 Release 压力验证，不能以源码存在替代验收。

调查保留原始 `.ips`、可执行 UUID、dSYM、系统版本、复现步骤和账户切换时序。分别使用 ASan、TSan、Zombies/Malloc Scribble 的兼容配置定位越界、竞争及失效对象；工具运行和优化构建复现互为补充，不能把 sanitizer 性能当作 Release 性能。对止血修改做单变量对照，并检查 Task、订阅和异步回调是否越过账户 generation。

### 4.2 Local Agent 存储存在确定的写放大

2026-10-22 原方案记录的只读样本如下。原记录缺少精确采集时刻及完整查询归档，暂用于说明量级，不作为正式验收基线：

| 项目 | 结果 |
| --- | ---: |
| `local-agent.sqlite3` 文件大小 | 2,757,988,352 bytes，约 2.57 GiB |
| SQLite freelist | 257,998 页，约 1,008 MiB |
| freelist 比例 | 38.3% |
| Command Receipt 表 | 约 795.5 MiB |
| Event 表 | 约 294.7 MiB |
| Run 表 | 约 233.0 MiB |
| Tool Invocation 表 | 约 187.1 MiB |
| 最近 24 小时 Command Receipt JSON | 2,211 条、约 751.5 MiB |
| 大于 1 MiB 的 Receipt | 302 条 |

`LocalAgentApplication::try_handle()` 默认使用 `persist_receipt = true`。`record_receipt()` 会再次把完整响应序列化为 JSON 写入 SQLite。相同业务数据还可能存在于 Run checkpoint/outcome、Event payload 和 Tool result 中。

现有保留策略只按 24 小时和 5,000 条裁剪，没有总字节预算；`VACUUM` 只在要求压缩的 Schema migration 后执行，不是日常维护策略。

因此卡顿链路不是“SQLite 文件大”这一句，而是：

```text
后台 Run 并发执行
→ 大结果在 Run/Event/Tool/Receipt 多处复制
→ JSON 编解码与大对象分配
→ stdio 帧复制和 SQLite WAL/事务写入
→ 同时进入复杂 Agent/Media 页面
→ 主进程争夺 CPU、内存带宽和磁盘
→ SwiftUI 首帧与交互延迟上升
```

数据库处于活动写入和保留裁剪中，同日复查可得到不同的 freelist、Receipt 条数和表占用；不得把不同采样时刻拼成一个快照。表占用包括页结构，JSON 正文字节也不等于 SQLite 文件或 WAL 增长。该因果链是待 trace 验证的性能解释，尚不能从文件大小单独推出每次卡顿的根因。

### 4.3 证据与可复现性

- 源码复核锚点：`6078f0cd88ec8453930f953648e0bb4a40717b08`，时间 `2026-10-22T02:02:01Z`；本工作区存在未提交变更，HEAD 不能独自代表所检查源码，也不能代表原崩溃包。
- 新采样必须保存 UTC 时间、Git SHA、dirty diff/未跟踪输入的 hash、App/Host UUID、构建 profile、macOS/硬件、数据集 hash、Schema version 和执行命令。
- SQLite 使用只读事务或 SQLite backup 得到一致样本；文件/WAL 外部大小另记采集时刻。记录 `page_size`、`page_count`、`freelist_count`、`auto_vacuum`、`dbstat` 和 Receipt 聚合 SQL；只保存脱敏统计，禁止把媒体、消息或密钥导出进报告。
- 崩溃事实、源码事实、运行测量和待验证假设分开标注。没有构建和采样来源的数字只能作为线索，不能升级为验收结论。

### 4.4 Debug 是放大器，不是根因

此前安装包确认为真正的 Swift Debug + Rust Debug 构建，现已从 `/Applications` 移除。Debug 会放大布局、状态发布、JSON 和数据库开销，但以下事实与构建类型独立：

- 11 份崩溃栈均指向 Story Studio/Combine；
- Receipt 默认保存完整响应；
- 数据库已经达到 GiB 级并包含大量空闲页；
- Agent Runtime 和 Media Studio 在页面不可见时仍有账户级工作；
- SwiftUI 观察范围过大。

后续不得把“换成 Release”当作问题已经解决。

## 五、架构决策原则

### 5.1 进程边界按风险和工作负载划分

功能满足以下条件中的至少两项，才考虑独立进程：

- 执行不可信或第三方代码；
- 单次可能占用大量 CPU、内存或文件 I/O；
- 有独立、可恢复的任务生命周期；
- 可以通过小型、稳定、粗粒度协议工作；
- 不需要高频访问 SwiftUI 细粒度状态；
- 需要独立权限、沙箱、QoS 或故障隔离；
- 退出进程能显著释放不可控缓存或媒体内存。

菜单大小、代码行数和页面层级本身不是拆进程理由。

### 5.2 单一事实源与单一写入者

每一份持久化状态必须有唯一拥有者：

| 数据 | 当前/目标拥有者 | 禁止事项 |
| --- | --- | --- |
| `local-agent.sqlite3` | Rust Local Agent Host | 主 App、Media Worker、Agent Runtime 不得直接打开写入 |
| `AgentGroupChat.sqlite3` | 当前 Swift Store；未来可迁入 Agent Runtime Service | 主 App 与辅助进程不得并行写入 |
| `AgentMemoryCache.sqlite3` | 与 Agent Runtime 同一所有者 | 不跨进程共享 SQLite 连接语义 |
| Story Studio 项目和媒体文件 | Media/Story Service，通过原子文件协议管理 | UI 不直接持有未落盘的超大媒体结果 |
| Plugin 安装与进程登记 | Local Connector/Plugin Runtime | Worker 不绕过权限与安装记录 |

如果模块进程化，数据库所有权必须整体迁移，而不是让两个进程同时访问同一文件。

单一写入者必须由操作系统级独占锁与 owner epoch 落实，不能只靠代码约定；主 App 多实例、旧 Helper 尚未退出和升级交接也必须被阻止。锁覆盖迁移、维护和正常运行，失去所有权后拒绝新写入。SQLite 自身写锁不能替代业务层单一所有者。

### 5.3 大数据走文件，小数据走 IPC

禁止通过 IPC 传输完整视频、超大图片 Base64、完整 Run 历史或无限列表。

- 小于预算的控制消息使用版本化 Codable/JSON 或 XPC DTO；
- 媒体和大工具结果先原子落盘，再返回受控文件引用、大小、MIME、SHA-256 和生命周期 token；
- 列表必须分页；事件必须返回版本/游标，通知只表示“有变化”；
- IPC 不得再次复制数据库中已经存在的完整 JSON。

### 5.4 隔离效果必须分别验收

进程拆分分别验证故障隔离、主线程延迟、进程组总资源和权限隔离。单独降低主进程 RSS 不代表总内存下降；Worker 的额外运行时、页缓存和 IPC 也计入预算。普通子进程继承同一用户权限，未经沙箱和凭据隔离不能宣称安全隔离。

## 六、目标进程拓扑

### 6.1 近期目标

```text
ChatOSSwift（UI 与编排）
├── Window / Navigation / 小型 Feature State
├── Conversation UI / Agent UI / Media UI
├── Local Connector 控制面
├── LocalAgentHostClient ───── stdio（保持现有传输）────────┐
├── MediaWorkerClient ─────── authenticated IPC（ADR 冻结）─┼─ helper processes
├── Plugin Runtime Client ─── stdio/process groups ────────┤
└── Global Utility Client ─── small snapshot/commands ─────┘

chatos_local_agent_host（Rust）
└── Project Chat / Task / Run / Event / Tool / local-agent.sqlite3

ChatOSMediaWorker（新增）
└── Upload / Download / Decode / Thumbnail / Frame / Validate / Blob Persist

Plugin Processes（现有）
└── Untrusted or isolated Plugin execution

Global Utilities Companion（现有）
└── Capture / Clipboard / Hotkeys / Quick Search
```

### 6.2 Agent 团队的条件式目标

第一阶段只抽象服务，不新增进程：

```text
AgentGroupRuntimeServicing
├── list/read/write room and message
├── submit/cancel/retry delivery
├── paged run/todo/artifact queries
├── change stream with sequence cursor
└── lifecycle: start(owner), suspend, resume, stop
```

UI 和 `AppModel` 只能依赖该协议，不能持有 `SQLiteAgentGroupChatStore` 或直接调用 Scheduler 内部对象。

完成协议化后，使用相同契约在两种实现间切换：

- `InProcessAgentGroupRuntime`：迁移期默认实现；
- `AgentGroupRuntimeClient`：未来连接 `ChatOSAgentRuntimeService`。

只有满足第十一节门槛，才启用独立 Agent Runtime。这样可避免现在就把 Swift AgentRuntime、Memory、Plugin Provider 和项目工具链重写为 Rust。

### 6.3 账户级 ResourceGovernor

主进程内先实现统一资源控制接口，进程化后保持同一契约；它负责准入和优先级，不成为第二个任务事实源。运行时仍拥有 durable queue 和 checkpoint。

| 资源类别 | 起始策略（待固定机器基线校准） | 必须保证 |
| --- | --- | --- |
| 后台逻辑 job | Host 与 Swift Agent 合计起始上限 3；交互窗口可降至 2 | 分配给两者的 permit 总数不超过上限，批次 64 不等于并发 64 |
| 媒体重解码/帧提取 | 整个 App 进程组同时 1 个重任务 | 按预计解码工作集准入，超预算拒绝或 downsample |
| 大文件上传/下载 | 起始总并发 2，流式缓冲有硬上限 | 文件大小与峰值内存不形成线性关系 |
| 数据库压缩/大型扫描 | 同时 1 个；空闲且有磁盘余量 | 与交互写路径错峰，不在 MainActor 等待 |

permit 必须带 `owner/session_epoch`、任务 ID、资源类别和 lease，支持释放、取消、续期及断连回收。准入发生在 claim 前或与 claim 协调；等待许可不能消耗 durable claim 的执行时限。父 job 同步等待子工作时禁止持有子工作需要的同类 permit，需定义借用/转移规则，避免嵌套 Host/Plugin 调用死锁。

lease 过期或 IPC 断连不证明旧执行已结束：回收前需确认进程退出/任务终止，或用 fencing epoch 拒绝旧执行提交，并确认其资源已释放。外部副作用不能仅靠本地 fencing 保证停止；在完成对账前不得另起同一副作用任务。

多个 Helper 不得各自复制一份“全局上限”；Host 先通过启动配置、有限 claim 和保守配额接入，再补动态额度协议。不可控 Plugin 不算已被限额：其派生进程、资源和调用链另行统计。Governor 故障时停止新准入，已接受的 durable 工作保留并可恢复。

前台交互加权、账户公平、排队老化、热状态/低电量降级和恢复迟滞必须明确；优先降 claim 速度和暂停可暂停阶段，不自动取消已提交副作用。记录排队时长、permit 使用、拒绝原因和总 in-flight，并提供手动配置覆盖。

### 6.4 Media durable job 与应用生命周期

新增 `MediaJobStore` 作为媒体 job 状态的唯一写入者：进程内阶段由 Media Service 拥有，Worker 启用后整体交接，UI 只通过服务访问。job 保存 owner、session epoch、稳定 job/idempotency key、输入 artifact 引用、provider ID、重试次数和状态版本；密钥不落 job 表。

```text
prepared → submitting → submitted(provider_job_id) → running
running → downloading → validating → finalizing → completed

submitting 遇到结果不明 → submission_uncertain → 查询/对账后继续
非终态 → cancellation_requested → cancelled / completed / failed
任一失败 → failed（带阶段、可重试性与恢复动作）
```

提交前持久化稳定提交身份，provider 支持时传递 idempotency key。发生“provider 已接受但本地未记下 job ID”的窗口时，按提交身份查询；不支持幂等或查询的 provider 必须保留 `submission_uncertain`，提示对账，禁止自动再次提交导致重复计费。取消是幂等请求：不承诺 provider 已停止或退款，完成与取消竞争时保留 provider 最终结果及费用事实。

provider 支持时优先查询/续签过期下载 URL；不以下载链接失效为理由重新生成。无独立远端 job ID 的同步 provider 也沿用提交不确定语义，不能伪造可恢复承诺。

| 生命周期事件 | 本方案默认语义 |
| --- | --- |
| 切页/关单个窗口 | 解除 UI 订阅；账户仍活跃时 durable job 继续 |
| 退出整个 App | 停止准入并保存 checkpoint，限时退出；不承诺本地任务继续执行，远端任务可能继续 |
| 睡眠/唤醒、断网 | 记录暂停原因，唤醒后重新鉴权与查询状态，不能把超时直接变成重新提交 |
| 登出/切账户 | 撤销旧账户凭据和新准入；隔离旧结果，重新登录原账户后对账恢复 |
| App/Helper 升级 | 先停旧写入者、检查兼容性并迁移，再转交独占锁；不兼容时明确停止 |

默认采用 App 管理生命周期；选择 XPC 并不自动提供退出 App 后持续工作的承诺。如产品需要退出后继续本地下载/生成，必须单独批准 LaunchAgent/`SMAppService` 的注册、用户关闭入口、后台凭据和升级语义，再更改本表。

### 6.5 文件与元数据的一致提交

Worker 拥有不可变 blob 和临时文件；Story Service 拥有项目业务元数据，项目通过 artifact ID/hash 引用 blob。两者不对同一可变文件交替写入，也不假定跨文件系统与 SQLite 存在一个原子事务。

1. 在目标同一卷创建受控临时文件，流式写入，校验大小、hash、解码预算及可用空间。
2. 按所需持久性 flush 文件及目录，以原子 rename 发布不可变 blob；跨卷导入先复制到目标卷临时文件。
3. Story Service 幂等提交 artifact 引用和项目 revision，再确认 job 完成。崩溃后可按 job ID 重试 finalize。
4. reconciliation 扫描“blob 已发布但元数据未提交”“元数据引用缺失”及未完成 job；未引用 blob 经权威元数据确认、宽限期和活跃 lease 检查后回收。Story Service 不可用时延后 GC，不能只因 UI 没确认就删除。

定义临时、待引用、已提交、删除待回收四类所有权。账户/项目配额、最小磁盘余量、保留期和删除规则由服务执行；UI lease 只控制临时交付，不能成为永久业务文件的生存依据。

## 七、各进程职责

### 7.1 ChatOSSwift 主进程

只负责：

- AppKit/SwiftUI Scene、窗口、导航和输入；
- 小型、页面级状态；
- 请求编排、权限确认和可见进度；
- 订阅辅助进程的状态快照；
- 用户显式打开文件和结果。

不得负责：

- 512 MiB 视频完整下载到内存；
- 大图片/视频解码和帧提取；
- 第三方 Plugin 实际执行；
- Local Agent 数据库写入；
- 在页面不可见时维持不必要的历史加载或预览任务；
- 用一个 ObservableObject 承载全应用所有可变状态。

### 7.2 Rust Local Agent Host

保持现有业务所有权，同时治理数据面：

- 按服务端命令分类决定 Receipt，副作用防重记录与大响应保留分离；
- 查询、轮询、健康检查和纯读取使用 ephemeral policy；
- Receipt 设置单条和总字节预算；
- 大结果保存一次，其他表保存引用、摘要或稳定 ID；
- 后台并发纳入账户级总预算；2–3 是 Host 与 Swift Agent 合计的起始预算，不能分别各开 3 个后称为全局受控；
- 提供安全、可观测、可中断的数据库维护入口；
- 维持 crash recovery 与幂等语义，不能为省空间破坏副作用防重。

### 7.3 ChatOSMediaWorker

新增 Worker 负责：

- Provider 媒体内容下载或接收受控下载任务；
- 上传从已授权文件句柄流式读取；DTO 使用 artifact 引用而非大 Base64 字符串；
- 流式写入临时文件，不构造 512 MiB `Data`；
- MIME、大小、像素、时长和 SHA-256 校验；
- 图片 downsample、缩略图、视频 poster/frame 提取；
- 最终 blob 原子提交至账户作用域目录，项目元数据按第 6.5 节幂等 finalize；
- 返回小型 `MediaArtifactDescriptor`；
- 支持任务取消、进度、超时、Worker 重启和可恢复 job ID。

Media Worker 不拥有 UI 状态，不直接修改 Story Studio ViewModel，不接收长期 API Secret 明文快照。需要凭据时使用短生命周期授权或凭据代理的受限请求；代理自身也必须从文件流式上传/下载，不能重新在主进程聚合完整媒体。

### 7.4 Agent Group Runtime

迁移期仍在主进程，但必须移出 `AppModel` 的具体实现依赖：

- Runtime 自己拥有 Store、Scheduler 和账户级协调器；
- UI 只请求分页快照和动作；
- 页面消失只停止 UI 订阅，不停止 durable delivery；
- Runtime 根据前台交互状态降低后台 QoS；
- 变化流包含单调 sequence，丢通知后可按 cursor 补齐；若 cursor 过期或现有机制仅支持失效通知，则返回 reset-required 并重取分页快照，不能承诺不存在的持久事件日志；
- 将来进程化时整体迁移 Store 与数据库所有权。

### 7.5 Plugin 与 Global Utilities

保持现有隔离并补齐统一管理：

- 每个进程有 owner、PID/start-time、任务、最后心跳和退出原因；
- 父进程退出时可可靠清理；
- stdout/stderr 有界；
- 重启有退避和熔断，禁止一秒无限拉起；
- Plugin 权限、路径和密钥边界不因新 Worker 放宽；
- Companion 继续只交换小快照和命令。

## 八、IPC 与数据契约

### 8.1 通用消息信封

P2 编码前以 ADR 冻结 Media Worker 的一种正式传输：App 生命周期方案优先评估 XPC；若沙箱、文件句柄或打包约束不满足，再选择受认证 local socket/继承管道并记录理由。不同时维护两套正式传输。现有 Rust Host 保留 stdio，不为统一形式额外迁移。

连接先交换 `hello`：协议 min/max、构建版本、capabilities、最大 inline bytes、支持的 schema/操作以及 session epoch；按共同能力协商，不兼容则明确拒绝。握手未完成前不得提交业务请求。

所有新增本地 IPC 至少包含：

```text
protocol_version
request_id
owner_user_id / account_session_id
session_epoch / helper_instance_id
operation
deadline
idempotency_key（仅需要时）
payload
```

响应至少包含：

```text
request_id
ok
result | error
retryable
server_sequence / snapshot_version（适用时）
```

### 8.2 Payload 预算

- 新协议默认 inline payload 不超过 512 KiB；
- 超过 512 KiB 必须分页、流式或文件化；
- 单页建议不超过 100 条且必须有游标；
- 进度事件使用最新值合并，不能无界排队；
- 同时限制消息总数、连接缓冲字节和积压请求数；进度按 job 合并并限频，终态单独可靠保存。解码前检查长度，不能先分配再拒绝；
- Local Agent 现有 64 MiB 消息上限只是安全上限，不是目标载荷大小；
- 不允许使用 Base64 把媒体塞进进程间 JSON。

### 8.3 取消与超时

- UI 取消需要区分“停止观察”和“取消业务任务”；
- 切页默认停止观察，不取消 durable Agent/Media job；
- 用户点击取消才发送业务取消命令；
- 每个请求有 deadline；超时只结束等待，不自动假定远端副作用未发生；
- 写命令用 idempotency key 查询最终结果，避免重复执行；
- 用户取消必须可重复调用；返回“已请求/已取消/已完成/结果不明”，不能用 RPC 成功推断 provider 已停止。断连后重建订阅并拉取持久化快照。

### 8.4 文件交付

大文件返回 descriptor：

```text
artifact_id
scoped_path_or_handle
mime_type
byte_count
sha256
created_at
lease_expiry / ownership
```

Worker 在受控目录写临时文件，按第 6.5 节提交 blob 与元数据。优先传递只读文件描述符或受限 capability；只有受信任内部路径模式才允许 scoped path，并校验账户根目录、文件类型、符号链接和打开后的文件身份。取消不能删除已提交业务文件。

### 8.5 对端身份、凭据与权限

- `owner_user_id` 是授权作用域字段，不是身份凭证。XPC 使用 audit token 与代码签名 requirement 验证对端；local socket/stdio 采用受保护端点、系统 peer 信息和继承连接能力等对应机制，不能只信任 payload、PID 或可猜测 session ID。
- 主进程把已认证连接绑定到账户 epoch，Helper 拒绝旧 epoch 消息和回调。UI 状态更新也验证 epoch，防止旧账户结果污染新账户。
- 当前普通子进程打包不能自动形成 App Sandbox 边界。ADR 明确 Helper 类型、bundle 布局、entitlements、可读写目录、网络权限及 TCC 能力；截图、录屏等权限继续由 Global Utilities 对应主体负责。
- 文件访问优先使用已打开的受限句柄。路径模式需防 traversal、symlink/TOCTOU，输出只允许在指定根目录创建；不能把任意字符串路径视为授权。
- 当前 Media Service 的 `include_secret=true` 路径需要收口：长期 provider secret 留在凭据代理，Worker 只获取一次任务范围的授权或流式代理能力；限制目标 host、方法、重定向及有效期，不接受任意 URL/任意 header 代理。日志、Receipt、job DTO 和崩溃诊断中禁止保存 secret 或带凭据 URL。
- Global Utilities 现有 Distributed Notification 和公共快照文件不能视为可信命令通道；补充私有会话端点、目录权限及可信来源校验，或迁移到认证 IPC。通知只作为不可信失效提示，未鉴权请求不得触发录屏、剪贴板读取等敏感动作。

### 8.6 最小进程监督（随 P2 交付）

监督记录 process identity（PID + start-time/instance ID）、session epoch、版本、最后心跳及退出原因。异常重启采用指数退避和抖动，例如 1/2/4/8/16/30 秒，5 分钟内连续 5 次异常则隔离该 Helper 并显示可恢复错误；稳定运行后才重置计数。不得沿用 Companion 当前固定 1 秒无限拉起策略。

明确父进程死亡策略、限时优雅退出、不可中断解码超时后的进程终止、日志轮转及孤儿临时文件回收。仅检查 PID 存在不足以识别原进程。被终止的 job 从持久化状态恢复；非幂等 provider 提交不得盲目重放。P4 统一监督实现，但这些基本约束不延后到 P4。

## 九、UI 状态与生命周期治理

### 9.1 AppModel 拆分

`AppModel` 逐步退化为路由和依赖容器，至少拆出：

- `WorkspaceNavigationState`
- `AccountSessionState`
- `ConversationPresentationState`
- `AgentWorkspacePresentationState`
- `MediaStudioPresentationState`
- `GlobalOverlayState`

View 只能观察实际使用的状态域。不得为了调用一个动作而观察整个 `AppModel`。

### 9.2 Media Studio 生命周期

- 登录时只绑定账户身份，不加载项目、历史和模型；
- 进入 Media Studio 时 activate 当前 section；
- 图片、视频、故事、历史分别按 section 延迟加载；
- 离开页面停止 UI 订阅、分页、预览和非 durable Task；
- 已提交的 durable 媒体 job 按第 6.4 节生命周期继续，UI 返回后按 job ID 恢复；P0 由进程内 Media Service 执行，P2 再交接 Worker；
- Story Studio 拆成 Project Catalog、Planning Session、Media Jobs 和 Selection 四个小状态域；
- 缩小发布域，并验证异步任务、订阅及发布器的隔离与生命周期；`@Published` 数量本身不是已证实的崩溃根因。

### 9.3 Agent 团队生命周期

- 账户登录后 Runtime 可以执行 durable heartbeat/recovery；
- Agent 页面不可见时，不构造 Workspace ViewModel 的展示快照；
- UI 订阅取消不影响 Scheduler；
- 高频 run checkpoint 不触发完整 rooms/todos/history 重载；
- 使用 sequence/cursor 合并刷新，只更新当前可见 Agent/Room/Run；
- 切页时主线程工作优先于后台恢复扫描。

### 9.4 QoS 与并发

- 主线程优先处理输入与导航；用户直接请求的后台工作按需用 `userInitiated`，不把整条模型/媒体管线标成 `userInteractive`；
- 后台 Agent 执行：按执行器能力映射为 `utility` 或较低 Task priority，受第 6.3 节总预算约束；Swift actor/Task priority 不等于操作系统硬资源上限；
- 数据库压缩、历史维护、缩略图预生成：`utility`/`background`；
- App 激活和切页期间临时降低后台 claim 速度，但不丢 durable work；
- 所有队列必须有界，并暴露排队数、运行数和拒绝/退避原因。

## 十、数据库治理方案

### 10.1 Command Receipt 分类

将命令分为：

1. `read_ephemeral`：读取、查询、轮询、健康检查，不保存 Receipt；
2. `write_compact_receipt`：保存命令 ID、指纹、状态和必要标识；
3. `write_replayable_result`：只有调用方重试必须得到完全相同小结果时使用；
4. `large_result_reference`：结果正文单独保存，Receipt 只保存引用和摘要。

不得继续让所有 IPC 调用默认保存完整响应。

分类由 Rust application 根据 `HostCommand` 的穷尽匹配决定，不能允许 IPC 调用方传 `persist_receipt=false` 绕过副作用防重。新增命令未分类时编译/CI 失败；内部可信 ephemeral 入口也要有适用范围。

| 命令例子 | 目标策略 | 重试正确性要求 |
| --- | --- | --- |
| `Health`、`GetRun`、`ListRuns`、`ListEvents`、`WaitEvents` | `read_ephemeral` | 允许读取最新快照；确认实现不存在隐藏业务写入 |
| `CreateRun`、`StartConversationTurn`、`CreateTaskGraph` | compact dedupe + 稳定实体 ID | 同 owner/key/fingerprint 只创建一次；同 key 不同内容拒绝 |
| `ClaimNextRun`、`ClaimNextTool`、`RenewToolClaim` | claim/lease 事务专用策略 | 返回原 claim 或明确过期，不能把一次重试变成领取第二份 work |
| `CommitStep`、`CommitTool`、`DecideToolApproval` | compact dedupe + 版本约束 | 提交与防重在同一事务；重复不再次产生事件或副作用 |
| `GetArtifactData`、大型历史/结果 | 读取文件引用/分页 | 不复制大正文到 Receipt；需要重放时引用固定 revision |

此表为重点示例，实施时必须覆盖全部 enum。每类写命令补充 `authoritative_key`、重放期限、过期后行为、结果重建方法及副作用位置。返回实体的当前状态不等于重放原响应：要求精确重放时保存不可变小结果或固定版本引用。

把可淘汰的完整响应与精简防重 tombstone 分开。tombstone 包含 owner/key、请求摘要、终态、实体/版本标识，至少覆盖最大客户端离线重试期和业务副作用生命周期。若期限无法有界定义，使用长期业务唯一约束；禁止仅因达到 24 小时或字节预算就重新允许副作用。超出重放期限返回明确的 expired/需要对账状态。摘要采用带版本的规范化表示，避免序列化格式变动破坏幂等身份。

SQLite 事务只覆盖本地事实，不覆盖 Plugin、文件系统和 provider 的外部副作用。涉及外部执行时先持久化执行意图/outbox，再携带稳定 invocation key 调用，最后记录结果；不支持幂等的外部系统出现结果不明时对账或等待人工处理，不能把 `CommitTool` 可重放等同于 Tool 恰好执行一次。

### 10.2 预算与维护

- 可淘汰 Receipt 响应正文总预算起始为 128 MiB，按 owner/时间和字节共同淘汰；防重 tombstone 单独统计与容量规划，不能被正文预算驱逐；
- 新增 Receipt inline 正文目标不超过 512 KiB，与 IPC 目标一致；旧记录超过 1 MiB 优先治理，超限转固定结果引用；请求 fingerprint 也改为有界摘要，不能保留一份大请求正文；
- 维护 Job 在无活跃写事务、接电且 App 空闲时运行；
- 首次治理先检查空间、备份并校验，完成分批逻辑清理后，在独占维护窗口 checkpoint/安全压缩并再次检查完整性；
- 记录实际 `PRAGMA auto_vacuum`：若为 `NONE/0`，直接调用 `incremental_vacuum` 无法回收文件，必须先经一次性重建/`VACUUM` 转换。是否转为 `INCREMENTAL` 由写入与碎片基线决定，避免每次启动完整 `VACUUM`；
- 数据库暴露 page count、freelist、WAL bytes、写入 bytes、最大 row 和维护时间指标。

### 10.3 正确性约束

- 不能删除仍承担幂等防重的 Receipt；
- 不能让清理任务与活跃写事务竞争导致启动或发送路径长暂停；
- 压缩失败必须保留原库和可恢复备份；
- Schema migration、数据清理、物理压缩是三个不同阶段，分别记录结果。

### 10.4 维护执行协议

维护入口由 Host 单一所有者执行，流程为 `preflight → quiesce → backup → cleanup → compact → verify → resume`。停止新 claim 并排空或 checkpoint 活跃写入，使用 maintenance lease 和独占锁；长读取者未退出、其他 App 实例占用或无法安全暂停时延期，不能强行在交互路径压缩。

preflight 按备份输出、重写后的数据库、WAL 峰值和安全余量估算所需空间，并核对实际目标卷容量；不得只用“剩余空间大于原库大小”作为保证。空间不足只做已证明安全的有界清理或延期，不能先删除唯一备份换空间。批量清理分事务限时执行，支持阶段间取消；单次不可安全中断的操作不承诺即时取消。

维护中止、磁盘满或 `kill -9` 后恢复必须能识别完整旧库、完成的新库及不完整临时文件，不能仅按文件修改时间选库。备份采用 SQLite backup/`VACUUM INTO`，不能裸复制活跃的 `.sqlite3` 而忽略 WAL。备份保留数量、校验状态、保留时长和删除条件进入策略，至少保留最后一个验证通过的可恢复版本，覆盖新版本的回滚窗口。

## 十一、Agent Runtime 是否独立进程的门槛

完成服务抽象后，只有满足以下任一条件，才实施 `ChatOSAgentRuntimeService`：

- 固定机器、数据集和后台负载下，预热 2 分钟后采样 10 分钟，Agent 团队可归因 CPU 均值超过 10%（Activity Monitor 口径：单逻辑核满载为 100%），且降低并发/刷新后仍超预算，并有 trace 显示其与 UI 延迟相关；
- 相对同数据集未启用 Agent Runtime 的对照，主进程 physical footprint 增量超过 300 MiB，停止可释放任务/缓存并等待 2 分钟后仍持续；排除正常持久工作集后，再评估 helper 是否能隔离和回收；
- Agent Scheduler/Store 发生独立崩溃并带走主 App；
- Agent 团队需要独立权限、更新节奏或无 UI 运行能力；
- 性能采样确认主要热点来自 Agent Runtime，而不是 SwiftUI 展示或 Local Agent Host。

如果进程化，必须同时满足：

- `AgentGroupChat.sqlite3` 和 `AgentMemoryCache.sqlite3` 整体迁移为 Helper 单一所有；
- 主进程不再直接获得 `SQLiteAgentGroupChatStore`；
- 变化流有 sequence/cursor 和断线补偿；
- Plugin/项目工具调用有清楚的代理边界，不形成双向无限 RPC；
- Helper 被强制终止后可从 durable checkpoint 恢复且不重复副作用；
- A/B 同时报告 App、Host、Helper、Plugin/Companion 进程组的总 CPU、physical footprint、峰值内存和磁盘写入；共享页按工具口径说明，不能把 RSS 简单求和当作精确物理内存。若仅转移资源而没有改善交互或可靠性，不以进程化作为性能优化完成条件。

在门槛未满足前，优先通过状态拆分、查询分页、事件合并、QoS 和并发预算解决问题。

## 十二、实施阶段

### P0-A：恢复可信 Release 基线

- 固定本次实施提交，从干净 checkout 构建 Release App；当前工作区的未提交改动先保留并明确纳入范围。`package-app.sh` 默认拒绝 dirty Release，不能为省事绕过后仍称正式基线；
- 验证 App 与 Rust Host 都是 Release；
- 产出机器可读 benchmark manifest：Git SHA、dirty 输入摘要（诊断构建适用）、Swift/Rust profile、编译器/SDK、可执行 UUID/dSYM、签名、协议/Schema 版本、OS/硬件、数据集 hash、脚本版本、UTC 起止时间与电源/热状态；
- 固定复现数据集与导航脚本；
- 增加 signpost：页面首帧、主线程 hang、Host round-trip、SQLite write bytes、JSON bytes、RSS 和 Worker job。

完成条件：基线与候选包均可由 manifest 重建，在同一机器、数据集、脚本和环境重复对比；每份 trace 可关联到具体包。

### P0-B：关闭 Story Studio 崩溃

- 验证稳定 `objectWillChange` 与首次激活不 reset；
- 对发布器生命周期假设做对照，保留 ASan/TSan/Zombies 等定位证据；若复现仍存在，继续查更早内存破坏，不能把止血补丁当根因闭环；
- 将 Media Studio 从登录 eager activation 改为账户绑定 + 页面 lazy activation；
- 拆分 Story Studio 发布状态；
- 增加登录/退出、账户切换、AI 创作进入/离开和生成中导航测试；
- 对每个新崩溃按 UUID 符号化，不能用旧报告代替新构建结果。

完成条件：Release 连续 20 次登录/退出和 200 次跨模块切换无崩溃、无新增对应 `.ips`。

### P0-C：治理 Local Agent 写放大

- 建立命令 Receipt 分类表；
- 只读 IPC 全部 ephemeral；
- 大结果改引用；
- 增加字节预算与迁移；
- 经空间预检与独占维护协议备份、治理和压缩现有数据库；
- 补齐副作用 tombstone、业务唯一约束和过期重试行为。

完成条件：固定 Run 负载下 Receipt 增长不超过预算；只读轮询产生 0 条 Receipt；幂等写重试仍返回一致结果且不重复副作用。

### P0-D：统一后台并发与准入

- 实现第 6.3 节 ResourceGovernor，先用有限总配额约束 Host 与 Swift Agent；
- 将 Agent batch/claim 数量与运行 in-flight 分开，处理 permit、lease 与嵌套调用；
- 增加前台切页、账户公平、热状态与低电量策略，以及有界队列指标；
- 用 Host Run + Agent recovery + 媒体任务组合负载验证，不能只单测每个模块。

完成条件：组合负载下后台逻辑 job 不超过配置总上限，重解码不超过 1；取消、断连和重启后 permit 可回收，无饥饿、死锁或 claim 过期重复执行。

### P0-E：媒体上传/下载文件原生化

- 将参考图片/视频/音频 DTO 和生成结果逐步改为 artifact/file 引用，清除大 Base64/String/Data 往返；
- 下载消费 `HTTPTransport.stream` 并直接写文件；确认实际 transport 是真正流式实现，默认 `stream` fallback 仍会完整调用 `send`，不能误当流式；
- 现有 `HTTPRequest.body: Data?` 不支持文件原生上传，补文件/流式上传接口与取消、长度校验。provider 仅接受 Base64 JSON 时从文件增量编码到有界流或临时请求文件，不在内存构造完整 JSON，并评估 provider 的输入上限；
- 先实现 MediaJobStore、提交不确定态和第 6.5 节 finalize；P2 整体交接给 Worker，避免同时改变大数据形态与执行位置。

完成条件：512 MiB 输入/输出路径没有全文件 `Data` 与 Base64 字符串共存；固定缓冲预算在 trace 中可见；强杀提交窗口不会自动重复计费。下载、上传和 provider 响应均有硬大小、超时和磁盘预算。

P0 依赖：A 是所有验收前提；B、C、D、E 可按模块推进，但 P2 的准入门槛是 B/C/D/E 验收通过且相关 ADR 冻结。P1/P3 的进程内服务抽象可提前推进，无须等待新 Helper。

### P1：拆分 UI 状态与页面生命周期

- AppModel 拆状态域；
- Conversation、Agent、Media 页面只观察局部状态；
- Media Studio section 延迟加载；
- Agent Workspace 查询分页并合并失效通知；
- 不可见页面停止非 durable Task。

完成条件：输入草稿、Agent checkpoint、媒体进度不会触发无关根视图重算；切页 p95 交互延迟小于 100 ms，单次不得出现超过 250 ms 的主线程停顿。

### P2：新增 ChatOSMediaWorker

- 定义版本化 Worker 协议；
- 冻结传输、凭据代理、签名/沙箱与文件授权 ADR；基本监督、握手、退避、隔离和父进程死亡处理必须同批交付；
- 先迁移缩略图、图片解码和视频帧提取；
- 再迁移已文件化的上传、下载与 blob 落盘；
- 整体交接媒体 job/文件所有权，项目业务元数据仍由 Story Service 幂等提交；
- 加入 crash/restart、取消、磁盘不足和账号切换测试。

完成条件：上传/下载大视频时主进程不持有完整视频 `Data`；杀死 Worker 不会结束主 App；重启后能恢复、明确失败或进入待对账态；文件可登记或由 reconciliation 回收。未通过对端鉴权/文件授权测试不得启用生产 Worker。

### P3：Agent Group Runtime 服务化

- 提取 `AgentGroupRuntimeServicing`；
- UI 移除 Store/Scheduler 具体类型依赖；
- 引入分页 DTO、sequence/cursor 与统一 lifecycle；
- 在进程内实现上完成测试和性能基线；
- 根据第十一节门槛决定是否创建 Helper。

完成条件：即使不启新进程，Agent 团队也已成为可测、可替换、单一所有者的后台服务。

### P4：统一进程监督和发布门禁

- 统一 Host/Worker/Plugin/Companion 的 PID、start-time、心跳、退避、退出原因和日志；
- 增加诊断页，显示每个进程的版本、RSS、CPU、队列和最后错误；
- Release CI 验证 Helper 版本、协议版本、签名和资源完整性；
- 对外分发时补齐 Developer ID、hardened runtime、嵌套 Helper 从内到外签名、公证及 stapling 验证；当前 DMG 脚本明确声明未公证，不能把现状当成发布门禁已满足。App 生命周期 Helper 与 XPC Service 的 bundle 布局按 ADR 分别校验；
- 灰度启用 Media Worker，保留单版本回滚开关；
- 明确当前/前一版本兼容矩阵与 Schema 回滚路径；不兼容允许明确拒绝，不承诺旧版本可读取已升级数据库。

## 十三、验证矩阵

| 场景 | 必须验证 |
| --- | --- |
| 项目聊天 Run 执行中切 Agent 团队 | 输入、滚动和首帧可交互；后台 Run 不丢失 |
| 项目聊天 Run 执行中切 AI 创作 | 无 Combine 崩溃；AI 页面按需加载 |
| 反复切换三大模块 200 次 | 无崩溃、无 Task/observer 单调增长、无数据库连接泄漏 |
| 登录/退出或切换账户 20 次 | 旧账号任务和媒体结果不能写入新账号状态 |
| Local Agent Host 被终止 | 主 App 存活；Host 有界退避恢复；幂等写不重复 |
| Media Worker 被终止 | 主 App 存活；临时文件可清理；job 可恢复或明确失败 |
| Media 每个状态强杀并重启 App/Worker | 提交前/接受后未记 ID/下载中/blob rename 后/元数据提交前分别注入；可恢复或待对账，无重复计费、无悬空引用 |
| Plugin 子进程生成孙进程 | 退出 App 后整个登记进程组被清理 |
| 512 MiB 上限视频上传与下载 | 主进程不持有完整视频 Data/Base64；大小超限、磁盘预算、流式背压和取消有效 |
| 文件完整但解码代价极高 | 超大像素、压缩炸弹、畸形媒体、超长时长有解码/工作集上限；Worker 退出不带走 UI |
| 网络切换/断网、睡眠/唤醒 | 凭据重新验证；超时不重复提交；断点续传仅在 ETag/内容身份匹配时使用 |
| 磁盘满、只读卷、目标跨卷 | 写入失败保留有效旧数据；临时文件可回收；跨卷不会误称原子 rename |
| 协议/Schema 不匹配、旧 Helper 存活 | hello 拒绝/迁移路径清楚；不产生双写入者，不解析未知消息 |
| 进度风暴/损坏帧/消费者变慢 | 缓冲有界、终态可查询、长度检查先于分配，主线程不被进度挤占 |
| 同机伪造命令/越权路径 | 未认证进程、旧 epoch、其他账户、symlink/路径逃逸均拒绝 |
| 同时启动两份 App/升级交接 | OS 锁与 epoch 保证单一写入者；不抢占仍存活所有者 |
| SQLite 维护 | 先备份与 integrity check；失败可恢复；活跃任务不被长时间阻塞 |
| 维护阶段 kill -9/备份盘空间耗尽 | 能恢复已验证数据库；不丢 WAL；不删除唯一可恢复备份 |
| Receipt 保留策略 | 字节预算有效；必要幂等结果仍可重放 |
| Receipt 淘汰后离线重试/并发同 key | 同 key 不同 fingerprint 拒绝；副作用不重复；过期行为明确 |
| Host + Agent + Media 组合负载 | 总准入限制生效，无嵌套 permit 死锁；前台切页仍达标 |
| 8 小时 soak 与 Helper 连续崩溃 | Task/observer/FD/临时文件不单调泄漏；达到阈值后熔断，不无限重启 |

Release 验收建议记录：

- 主进程与每个 Helper 的 CPU、RSS、physical footprint；
- 页面切换到可交互时间；
- Main Thread hang 次数和最长时间；
- Host/Worker IPC 请求数、字节数和 p50/p95/p99；
- SQLite transaction、WAL bytes、freelist 和维护时间；
- SwiftUI body/signpost 次数；
- 运行中 Task、observer、文件描述符和子进程数量。

验收统一口径：冷启动、暖切页、前台单任务、后台组合负载分别运行；固定机器/OS、数据集、供电及热状态，记录缓存与网络条件。每个主要场景至少 3 轮，报告每轮与合并分位数、样本数和最坏值；不能把不同硬件结果混成一个 p95。

| 指标 | 起始门槛/判定方法 |
| --- | --- |
| 页面切换交互延迟 | 暖切页从导航输入到目标首个可响应交互帧，p95 < 100 ms；冷加载单独统计，不隐藏在暖切页均值中 |
| 主线程 hang | 本验收脚本中无超过 250 ms 的停顿，保存最长栈与 signpost；网络等待与主线程阻塞分开 |
| 内存 | 同时报告主进程和进程组 physical footprint、峰值/稳定值；媒体文件尺寸增大时不得出现完整文件级内存放大，另设解码像素预算 |
| 总资源 | 相同工作量报告总 CPU 时间、磁盘写字节、完成时长和能耗/热状态；候选包较基线恶化超过 10% 时需 trace 解释及明确的架构取舍记录，不能仅凭 UI RSS 降低通过 |
| 可恢复性 | 每个持久化窗口故障注入后，无重复副作用、跨账户结果和已完成文件丢失；无法确认 provider 结果时进入待对账态 |
| 并发/泄漏 | in-flight 不超过配置额度；稳定工作负载下 Task/FD/observer 等在回收窗口后回到预算，不随循环单调增长 |

数值是实施起始 SLO，需在 P0-A 固定硬件等级和产品可接受负载后冻结；调整必须记录理由与对照数据。崩溃测试的 20 次账户循环、200 次切页和 8 小时 soak 是发布门禁样本，不是“不会再崩”的统计证明。

## 十四、回滚策略

- P0 数据库变更必须在迁移前创建校验通过的备份；
- Receipt 策略以 Feature Flag 灰度，可恢复旧读取兼容，但不得同时恢复无界写入；
- Media Worker 初期保留受控的 in-process fallback，仅用于 Worker 不兼容，仍须遵守文件原生化、资源预算和媒体校验；未经校验的高风险解码不自动退回主进程；
- Helper 协议不兼容时应明确提示并停止对应功能，不能尝试解析未知 payload；
- Agent Group Runtime 在未满足进程化门槛前保持 in-process 实现，避免一次改动同时改变业务逻辑、数据库所有权和 IPC；
- 任何阶段出现数据正确性问题时，优先回滚该阶段，不回退无关的稳定性修复；
- Schema 已升级时先核对旧包读取能力；恢复旧快照会丢弃快照之后的新写入，必须走明确的数据恢复流程，不能静默用旧备份覆盖。Feature Flag 本身不构成数据库降级方案。

## 十五、源码落点

主要现状与后续改造入口：

- App/进程入口：`clients/macos/Sources/ChatOSApp/ChatOSApp.swift`
- 全局生命周期：`clients/macos/Sources/ChatOSApp/AppModel.swift`
- 登录激活：`clients/macos/Sources/ChatOSApp/AppModelAuthentication.swift`
- Agent 常驻协调器：`clients/macos/Sources/ChatOSApp/AppModelAgentRuntime.swift`
- 根视图观察边界：`clients/macos/Sources/ChatOSApp/RootView.swift`
- Agent 团队服务：`clients/macos/Sources/ChatOSConnector/NativeAgentGroupChatService.swift`
- Agent 团队 Store：`clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- Agent 团队 Scheduler：`clients/macos/Sources/ChatOSConnector/LocalAgentGroupChatScheduler*.swift`
- Local Agent Host 生命周期：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostLifecycle.swift`
- Local Agent Host IPC：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostProcess.swift`
- Media API：`clients/macos/Sources/ChatOSAPI/ChatOSMediaGenerationService.swift`
- 流式下载与上传接口缺口：`clients/macos/Sources/ChatOSAPI/HTTPTransport.swift`
- 参考媒体 DTO：`clients/macos/Sources/ChatOSCore/MediaGenerationModels.swift`
- Story 参考视频读取/Base64：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryVideoGenerationAdapter.swift`
- Media Studio 状态：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/MediaStudioViewModel.swift`
- Story Studio 状态：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryStudioViewModel.swift`
- Global Utilities Companion：`clients/macos/Sources/ChatOSApp/Features/GlobalUtilities/GlobalUtilityCompanion.swift`
- Plugin 进程监督：`clients/macos/Sources/ChatOSConnector/NativePluginApplicationProcessRegistry.swift`
- Receipt 默认策略：`clients/local_agent_host/layers/application/src/lib.rs`
- Receipt 持久化：`clients/local_agent_host/layers/infrastructure/database/src/lib.rs`
- Receipt 保留策略：`clients/local_agent_host/layers/infrastructure/database/src/schema.rs`
- SQLite 维护：`clients/local_agent_host/layers/infrastructure/database/src/maintenance.rs`
- Host 协议事实源：`clients/local_agent_host/layers/interface/src/lib.rs`
- Swift Host 版本与并发：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostLifecycle.swift`
- Rust 独立启动并发：`clients/local_agent_host/src/main.rs`
- 打包、签名及发布：`clients/macos/scripts/package-app.sh`、`clients/macos/scripts/package-release-dmg.sh`

## 十六、实施前需冻结的架构决策

以下为当前建议与冻结时点，避免把待验证选择写成已实现能力。职责列用于拆分工程责任，不代表已指派人员。

| 决策 | 当前建议 | 冻结时点/责任职责 |
| --- | --- | --- |
| 生命周期 | App 退出后停止本地执行、重启恢复；provider 远端作业按实际状态对账 | P0-E 前；产品 + macOS runtime |
| 媒体 job 唯一所有者 | Media Service 起步，P2 整体交接 Worker，Story 元数据保持独立业务所有者 | P0-E 前；媒体/存储 |
| Worker IPC 与沙箱 | App 生命周期优先评估 XPC，按真实签名/文件/网络约束选定一种传输 | P2 前；macOS runtime/打包 |
| 凭据 | 长期 secret 留在代理，任务授权/流式代理且限制目标与操作 | P0-E/P2 前；API/凭据 |
| 并发预算 | Host + Swift Agent 后台总额度起始 3；媒体重任务 1；大传输 2 | P0-D 验收；runtime/性能 |
| 防重与维护 | 全 enum 命令策略、重放期限、tombstone 保留和自动压缩模式 | P0-C migration 前；Host/存储 |
| 分发与升级 | 定义内部测试签名和正式外部分发两条验收路径、旧版本兼容范围 | P2 打包前、P4 发布前；发布工程 |

每份 ADR 必须记录问题、证据、选项、选择理由、失败/回滚行为和验证方式。未冻结的决策不阻止其他独立阶段推进，但不能跨过对应生产启用门禁。

### 16.1 源码与文档漂移治理

- 以协议代码常量与 Schema migration 为事实源。当前 Rust/Swift Host 常量为 v44，而 `clients/local_agent_host/ARCHITECTURE.md` 仍有“validate protocol v28”陈述，属于待修的文档漂移；本方案引用版本不能替代实际握手。
- CI 比较 Swift/Rust 协议版本、payload 边界、支持能力和打包后的真实可执行版本；Schema 兼容矩阵、进程布局及 ADR 随实现更新。
- 为每个阶段归档 manifest、测试脚本、脱敏统计、故障注入结果与 trace，结论标注“现状/目标/已验证”。本次仅强化方案，新增 Governor、Worker、job 状态机与门禁均为待实施项。

## 十七、最终决策摘要

本项目需要的是选择性进程隔离，不是“每个大功能一个进程”：

- 已有 Rust Host、Plugin 和 Global Utilities 隔离继续保留；
- 媒体先完成上传/下载文件原生化与 durable job 一致性，再新增 Media Worker 隔离解码和文件 I/O；
- Agent 团队先服务化，再按真实指标决定是否独立进程；
- 主 UI 进程必须先解决 Combine 生命周期、状态观察范围和 eager activation；
- Local Agent 必须先解决 Receipt 与大 JSON 写放大；
- Swift Agent 与 Rust Host 必须共享总并发预算，并处理 claim/lease 与嵌套调用，不能各自限流后相加失控；
- 所有跨进程架构都以单一数据所有者、小消息、文件化大结果、可取消和可恢复为前提；
- 身份认证、文件授权、最小权限和基本监督随新 Helper 同期交付，故障隔离与安全隔离分别验收。

只有按这个顺序实施，多进程才会成为稳定性边界，而不是新的复制、同步和数据库竞争来源。
