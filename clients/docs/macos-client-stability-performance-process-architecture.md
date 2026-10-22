# ChatOS macOS 客户端稳定性、性能与进程架构方案

更新时间：2026-10-22

状态：强化版方案，P0 部分实施中；阶段记录见 12.1—12.13，完整 Release 验收待完成

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
- App 启动 Host 时使用 12.12 的进程组逻辑 job 预算：默认总额为 3（两核及以下为 2），在常规机器上分为 Rust Host 1、Swift Agent 2；可通过 `CHATOS_BACKGROUND_JOB_BUDGET` 配置 2–16 的总额，旧 Host 单项覆盖也会被总额和 Swift 保留位约束。独立运行 Rust Host 且未指定参数时，其 Rust 默认值仍限制在 2–8；两者必须分别记录，不能混称一个默认值；
- `local-agent.sqlite3` 保存 Run、Event、Tool Invocation、Command Receipt 等数据。

页面离开时，`ConversationSessionViewModel.deactivate()` 会取消 Realtime、历史刷新、分页和视口任务，但不会取消已经持久化并提交给 Host 的后台 Run。这符合“切页不丢任务”的产品语义，却要求后台任务降低优先级并受资源预算约束。

### 3.4 Agent 团队仍属于主进程后台运行时

当前 Agent 团队实现是 Swift 原生运行时：

- `SQLiteAgentGroupChatStore` 是 actor，直接拥有 `AgentGroupChat.sqlite3` 连接；
- `LocalAgentGroupChatScheduler` 在 macOS 客户端内消费 durable delivery queue；
- `AppModelAgentRuntime` 启动 Communication、Executor Recovery、Heartbeat 和 Artifact Storage Task；
- `NativeAgentGroupChatService.changes()` 只是进程内有界失效通知，SQLite 才是事实源；
- Agent 团队 UI 进入页面后创建 Workspace ViewModel，并订阅变化后重新读取 Store。

另外，`AppModelAgentRuntime` 的 Communication recovery 使用 `batchSize = 64`，该值只表示单次 drain 的吞吐上限。12.12 已增加 Scheduler 共享的执行 Governor：room/account/communication/recovery 路径在 durable claim 前取得 permit，同一轮最多 claim 当前可执行的数量，不能让任务持有数据库 lease 等待额度；`batchSize = 64` 不再等于并发 64。动态前台降额、账户公平、热状态与 Media/Plugin 接入仍未完成。

这部分虽然不一定运行在 MainActor，但 CPU、内存、数据库页缓存和崩溃边界仍与主 App 共享。另一方面，它依赖 Swift `AgentRuntime`、Memory、Local Projects、Plugin Capability Provider 和 Local Connector。直接迁入 Rust Host 会形成高成本重写，不适合作为第一阶段修复。

### 3.5 AI 创作和媒体链路仍属于主进程

当前媒体服务具有以下特点：

- `AppModel` 构造时立即创建 `MediaStudioViewModel` 和 `StoryStudioViewModel`；
- 实施前登录成功会立即调用 `mediaStudio.activate(userID:)`；首批改为只绑定账户，进入媒体页面才加载模型与历史，Story 目录延后至故事/历史 section；
- `StoryStudioViewModel` 同时管理项目目录、规划、历史、图片、首尾帧、视频、批处理和帧提取；
- Story Studio 当前有 30 余个 `@Published` 状态和多组 Task 字典；
- 基线审计时视频下载上限为 512 MiB，结果以 `VideoGenerationResult.videoData: Data` 返回主进程后再落盘；12.15 已将生产下载改成有界流写临时文件，兼容 Data 初始化器仅留给旧调用和测试；
- 基线审计时上传侧 `StoryVideoGenerationAdapter` 会读取最大 512 MiB 的完整参考视频并调用 `base64EncodedString()`；512 MiB 输入仅 Base64 就约 683 MiB，与原始 `Data` 并存超过 1 GiB。12.14 已将 Story 参考视频按 provider 的 47 MiB 实际上限改为文件引用和增量请求文件；图片/音频 Base64 仍待治理；
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

审查基线中，`LocalAgentRuntime::try_handle()` 默认构造 `persist_receipt = true` 的命令上下文；纯查询实际上未调用 `record_receipt()`，不能把所有查询算作 Receipt 写入。对持久化写命令，`record_receipt()` 会再次把完整响应序列化为 JSON 写入 SQLite，相同业务数据还可能存在于 Run checkpoint/outcome、Event payload 和 Tool result 中。已实施的指纹与服务端分类优化见 12.1、12.2，完整写响应去重仍待完成。

响应正文按插入时的 24 小时业务时间窗口和 5,000 条裁剪；12.4 已将长期防重身份独立保存，12.5 再增加全库 128 MiB 正文预算，不再随正文裁剪防重身份。该预算不限制数据库/WAL 文件大小或长期身份总量，也尚未实现 owner 公平配额。`VACUUM` 只在要求压缩的 Schema migration 后执行，不是日常维护策略。

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

迁移不能以旁路双写开始。当前 Story `segment.attempt`、持久化 Media Batch `jobs` 和通用 Media Studio 内存 token 是三套不同生命周期：前两套已经保存 provider 身份和防重意图，后一套没有 durable 状态。实施顺序固定为：先让通用 Studio 的新视频任务由 `MediaJobStore` 独占写入；再用可重复的导入迁移把 Story/Batch 非终态 job 搬入同一 Store，并在同一次项目/Batch 原子提交中只保留业务关联 ID；读路径完成切换并验证恢复后，才停止旧字段写入。迁移期间允许旧格式只读兼容，不允许 Store 与旧字段各自推进状态。若任一步无法提供原子切换或稳定关联 ID，则停止迁移，不能新增第四套可运行任务源。

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
- 写连接必须覆盖 BEGIN 派发前、业务执行与事务收尾的完整生命周期；调用方取消不代表 SQLite 后台语句已停止。未确认清理完成的连接不能回池，不能只在业务成功/失败的显式返回路径回滚。当前实现与验证见 12.9、12.10。

### 10.4 维护执行协议

维护入口由 Host 单一所有者执行，流程为 `preflight → quiesce → backup → cleanup → compact → verify → resume`。停止新 claim 并排空或 checkpoint 活跃写入，使用 maintenance lease 和独占锁；长读取者未退出、其他 App 实例占用或无法安全暂停时延期，不能强行在交互路径压缩。

preflight 按备份输出、重写后的数据库、WAL 峰值和安全余量估算所需空间，并核对实际目标卷容量；不得只用“剩余空间大于原库大小”作为保证。空间不足只做已证明安全的有界清理或延期，不能先删除唯一备份换空间。批量清理分事务限时执行，支持阶段间取消；单次不可安全中断的操作不承诺即时取消。

维护中止、磁盘满或 `kill -9` 后恢复必须能识别完整旧库、完成的新库及不完整临时文件，不能仅按文件修改时间选库。备份采用 SQLite backup/`VACUUM INTO`，不能裸复制活跃的 `.sqlite3` 而忽略 WAL。备份保留数量、校验状态、保留时长和删除条件进入策略，至少保留最后一个验证通过的可恢复版本，覆盖新版本的回滚窗口。

当前已落地启动迁移的备份保护与最低空间准入（12.3），以及新版 Host 从打开数据库前直到进程退出的 OS 独占锁（12.7）；后者只约束遵守同一协议的新 Host，旧版进程仍须在升级前停止。v34 → v37 的备份发布、回填与版本提交前后已增加临时库 SIGKILL 回归（12.8），不代表 COMMIT 内部写窗口、旧版本压缩、掉电或真实磁盘耗尽已验收。尚未实现上述完整在线维护协议，不能据此启用活动数据库治理。

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

### 12.1 首批实施记录（2026-10-22）

基于分支 `3.0.9.3`、实施前 HEAD `763f5f399eaa` 的工作区修改，尚未完成正式打包与发布级性能基线；下述固定数据量工程基准不替代交互延迟、组合负载和长时间稳定性验收：

| 项目 | 已实现 | 边界/仍待完成 |
| --- | --- | --- |
| P0-B 媒体生命周期 | 登录只 `bindAccount`；进入页面加载媒体模型/历史；Story 仅在故事/历史 section 激活；账户变化触发可见页面重新加载 | 未完成 Story 状态域拆分、sanitizer 定位及 200 次导航/账户循环，不能宣称崩溃根因已修复 |
| P0-C 请求指纹 | Host 使用流式 JSON → `sha256:v1:` 摘要，持久化指纹固定 74 字节；可信 ephemeral 调用跳过指纹序列化；后续全命令分类见 12.2 | 完整响应仍按原策略保存；响应引用/tombstone、总字节预算与维护协议尚未实施；不缩小已有数据库 |
| 旧回执兼容 | 新 Runtime 可重放已有完整 JSON 指纹的 Receipt，同 ID 不同内容仍拒绝；新回执防重仍有效 | 无 Schema 迁移；旧版整包不认识新指纹时可能明确报 command mismatch，禁止用清空 Receipt 处理回滚 |

修复后验证（未排除下述两个 Rust 失败用例）：

- Rust：`cargo test -p chatos_client_storage -p chatos_local_agent_runtime`，存储层全套 121 项、Runtime 全套 53 项通过。
- Swift Debug：相关回归 97 项通过，包括媒体 31 项、验收分类 3 项、Agent 存储 61 项、显式启用的存储基准与 UI 刷新诊断各 1 项。
- Swift Release：媒体 31 项、验收分类 3 项、Agent 存储 61 项和显式启用的存储基准 1 项通过；此前独立的 `swift build -c release --product ChatOSSwift` 也成功。这里只覆盖相关测试，不宣称整个 Swift 测试集已全量运行。
- 固定存储基准包含 20 Agent、10 房间、500 消息、500 Run、100 Todo，重复 3 轮。Debug 仍验证原有 SQL 数量；Debug/Release 均验证实际返回数据、附件内容，以及空闲心跳/调度零数据库写入。耗时仅是工程测量，不据此宣称发布 SLO 已达标。

失败成因与修复：

- `failed_task_display_distinguishes_execution_from_unverified_acceptance`：Rust `task_writeback_summary::task_display_status` 的 succeeded 分支先把报告总状态 failed 映射为 acceptance_failed，后面的 not_verified 检查不可达；Swift 则先检查逐项 failed/not_verified。已将 Rust 展示优先级对齐为 blocked → 逐项 failed → 逐项 not_verified → 总状态 failed 兜底，并补充新字段 `result_status` 优先于兼容字段、混合检查结果及空检查列表的回归；不改 Task 调度状态。问题实现来自 `904d136fa`，暴露该问题的断言在 `bdb37d1cb` 增补。
- `failed_branch_reports_partial_success_and_blocked_dependents_then_retry_generation`：`904d136fa` 将成功任务的展示标签改为 completed，测试仍断言旧标签 succeeded。已同步展示断言，保留执行成功/失败/依赖阻塞与重试的业务校验，不更改任务执行结果。
- Release 编译阻塞：多个测试无条件引用 `#if DEBUG` 中才定义的 SQL 计数 API，`--filter` 不会免除其他测试 target 的编译。已仅条件化计数采样与断言，功能校验仍在 Release 运行；存储基准保留 Release 耗时和 SQLite 写入检查，JSON 标明构建配置与计数能力，未测量的计数字段省略而非伪造为 0。仅按需读取的 SQLite 累计写入指标不增加逐查询计数开销。UI 刷新诊断依赖 Debug 计数器判断稳定状态，因此在 Release 显式跳过，不能视为 Release UI 性能验收通过。

复现 Swift 回归可使用下面的命令；Release 时增加 `-c release` 并去掉 UI 诊断过滤项，避免把预期跳过计入通过数量：

```sh
CHATOS_RUN_AGENT_STORE_BASELINE=1 CHATOS_RUN_AGENT_UI_BASELINE=1 \
swift test --package-path clients/macos \
  --filter 'MediaStudio(ViewModel|HistoryStore|ImageInteraction|VideoCanvas)Tests|MessageTaskAcceptanceTests|SQLiteAgentGroupChatStoreTests|AgentGroupChat(Store|ViewModel)PerformanceBaselineTests'
```

上述首批改动不代表 P0 或整个多进程方案已完成。全 enum 分类的后续实施见 12.2；响应去重、Host/Swift Agent 总准入预算与文件原生媒体传输仍需推进。

### 12.2 第二批实施记录：服务端命令分类（2026-10-22）

在 application 层新增 `command_receipt_policy.rs`，穷尽匹配当前全部 78 个 `HostCommand`，无默认分支；新增命令未选择策略时编译失败。策略不依赖命令名字前缀，也不接受 IPC 调用方传入的 `persist_receipt` 选项。

| 当前实现类别 | 数量 | 普通请求入口 | 内部一次性入口 |
| --- | ---: | --- | --- |
| `ReadEphemeral` | 38 | 不计算指纹，不保存 Receipt，返回最新查询结果 | 同左 |
| `ReplayableWrite` | 36 | 计算有界摘要，保留原精确重放 | 分发前拒绝，不允许创建/取消/审批等任意写绕过回执 |
| `SchedulerReplayableWrite` | 4 | `ClaimNextRun`、`ClaimNextTool`、`CommitStep`、`CommitTool` 仍保留原精确重放 | 仅现有可信进程内调度器可免回执；仍受 claim token、版本与对账约束 |

`InitializeNotepad` 名称虽像初始化写操作，当前存储实现仅执行现有笔记数量的 SELECT，因此按只读处理；若将来增加导入/修复写入，必须重新审计分类。`RenewToolClaim` 仍走普通持久化写策略；内部心跳现有专用方法未改变。

本批的直接优化是省去只读请求原有的无用 JSON 指纹计算，不是减少此前不存在的查询 Receipt。与此同时收紧内部 `try_handle_ephemeral` 的适用范围，不能将创建 Run/Task 等普通写命令交给这个入口免除防重。

验证：`cargo test -p chatos_local_agent_runtime -p chatos_client_storage -p chatos_local_agent_protocol -p chatos_local_agent_host`，Runtime 61 项、存储层 121 项、协议 37 项、Host 库 112 项和可执行入口 5 项，共 336 项全部通过，无忽略或排除。新增 8 项测试覆盖读取免指纹、内部白名单、拒绝后无状态变化、伪造 wire 字段不能关闭防重、读取返回新状态、写重放不重复事件、Run 已完成后旧 claim 重试不领取下一份 work，以及内部 claim/commit/对账仍可运行。

附加严格静态检查 `cargo clippy -p chatos_local_agent_runtime --all-targets -- -D warnings` 通过。检查发现的两处阻塞已作等价整理：`LocalTaskGraphNotificationPolicy` 改为派生 `Default`，默认值仍为 `ProgressSummary`；`skill_runtime.rs` 的测试模块原样移至文件末尾，未改技能运行实现和测试内容。

本批未改 Swift、IPC 协议版本或数据库 Schema，也未安装/重启 App 或清理活动数据库。当前三类是读取/写入/内部例外的入口治理，**不是** 10.1 中 compact/reference 结果存储的全部实现：普通写仍保存完整响应，旧 24 小时/5,000 条裁剪及防重期限问题仍存在。后续需将不可变结果与长期防重 tombstone 分离，配合明确的重放过期行为和字节预算再迁移；不能删除回执或读取当前实体状态来冒充历史响应。

### 12.3 第三批实施记录：迁移恢复点保护（2026-10-22）

推进回执结果/tombstone 分离前，审计发现原迁移流程会在创建新备份前删除所有匹配文件，并在迁移成功或再次打开当前数据库时删除备份。首次迁移失败虽然留下备份，下一次启动却会先删掉它；随后若备份失败或空间耗尽，已有恢复点也丢失。因此本批先修补迁移安全前置条件，不引入新 Schema。

已实施：

- 移除三处启动路径的备份自动删除；成功迁移、失败重试和当前 Schema 重启都保留原恢复点，不自动恢复或覆盖主库。
- 新快照在同目录以 `.backup.sqlite.incomplete` 创建，Unix 从创建起即使用 `0600`；通过 SQLite `VACUUM INTO` 纳入 WAL 中已提交的数据，执行完整 `integrity_check(1)` 和来源 Schema 版本校验，再同步文件、同目录 rename，Unix 同步目录。通过后才使用 `.backup.sqlite` 名称。旧版本遗留的同名后缀不自动认定有效，也不自动清理。
- 空间预检使用目标目录所在卷的实际可用空间，数据库估算取文件尺寸与 `page_count × page_size` 的较大者；最低空闲门槛为 `3 × 数据库估算 + 当前 WAL 文件尺寸 + 64 MiB`，覆盖备份、一次重写、数据库量级 WAL 增长和安全余量，计算溢出拒绝。
- 每个数据库累计 8 个恢复/未完成文件时，暂停创建新备份与新迁移，返回非自动重试的 `storage_maintenance_required`，要求先验证并归档恢复文件；不为腾空间自动删除。当前 Schema 的正常重启不消耗槽位，也不受该迁移门槛阻塞。

验证：新增 7 项测试，加固原迁移回归，覆盖空间边界与溢出、槽位耗尽时 Schema 不变、显式释放测试槽位后可重试、WAL 已提交数据进入快照、恢复点跨重启及失败重试保留、来源版本不符拒绝、无迁移表旧库的恢复快照。存储层 128 项、Runtime 61 项、协议 37 项、Host 库 112 项与可执行入口 5 项，共 343 项全量通过；存储层与 Runtime 的全目标严格 Clippy 检查通过。

边界：这是启动迁移保护，不是 10.4 的完整维护协议。最低空间估算不等于空间预留或任意迁移增长的上界；尚无跨进程独占维护 lease、迁移专用容量估算、自动归档/保留时长策略及 `kill -9`/真实磁盘耗尽故障注入。旧 `<33` 迁移仍有既有清理/压缩行为，本批未改变；回执长期防重、不可变结果引用和正文总字节预算仍未落地，不宣称 P0-C 已完成。本批新增 `fs2` 仅用于跨平台查询可用空间；未升级 Schema、安装/重启 App 或触碰活动用户库。

### 12.4 第四批实施记录：防重身份与历史响应分离（2026-10-22）

Schema 升至 v35，新增 `local_agent_command_tombstones`，保存全局 command ID、请求指纹和本地事务完成时间；表中存在记录即表示该本地写事务已完成，不附着实体外键，也不设置自动 TTL。历史响应正文仍在 `local_agent_command_receipts`，原插入触发的时间/数量淘汰仅作用于该正文表。

已实施的行为：

- `record_receipt` 的 BEFORE INSERT trigger 在同一业务事务内先写防重身份，即使新正文因旧时间戳立即被淘汰，身份仍保留；业务写/事件/回执失败则一并回滚。
- 重试先查询身份并校验指纹，通过后才加载可能较大的历史响应；同 ID 不同输入始终返回 `command_mismatch`，不因正文已过期而放宽。正文仍在时精确重放，正文已淘汰时返回 `command_replay_expired`、`retryable=false`，要求对账，不自动换新 ID 重做，不拿当前实体状态伪造历史响应。
- v35 在单个迁移事务内按主键游标逐条回填尚存身份，不加载历史响应正文；旧完整 JSON 指纹转换为兼容的 `sha256:v1:` 摘要，回执表的同份旧指纹同步收紧；新版 Host 指纹仍固定 74 字节，可信端口历史 opaque 指纹不擅自改变含义。回填失败时 Schema、身份和指纹更新整体回滚，保留迁移前恢复点。
- 修正尚未执行的 v33 迁移路径：不再 DROP/重建回执表，保证从 `<33` 升级时尚存的身份能到达 v35 回填。已经由旧版本删除的回执无法凭空恢复；不扫描活动用户库或假定存在可恢复记录。

验证：新增 12 项回归。存储层 139 项、Runtime 62 项、协议 37 项、Host 库 112 项与可执行入口 5 项，共 355 项全量通过，无忽略或排除；存储层与 Runtime 全目标严格 Clippy 通过。覆盖时间/数量淘汰、相同时间戳排序、立即淘汰、不同指纹拒绝、跨数据库重启保留、业务实体删除后不能重建、两个独立连接并发同 key 只写一次、回执写入失败回滚业务/事件/身份、可信 ephemeral 零身份且不序列化、v34 新旧指纹精确重放、回填中途失败与重试，以及 IPC 旧 claim 过期后不回收/领取其他 work、错误码与非重试标记。

边界：本批是本地 SQLite 写防重，不保证 Plugin/文件/provider 的外部副作用恰好一次；内部可信调度器一次性入口仍按 token/version/对账协议运行，未改为持久化回执。正文保留仍是基于写入时间高水位的机会式清理，不承诺空闲时满 24 小时立即过期。长期身份不受正文配额驱逐，会持续增长，独立容量统计/准入与归档策略待完成；128 MiB 正文预算、512 KiB inline 目标和不可变大结果引用也仍未实现。旧 `<33` 大库的迁移耗时、空间峰值、既有压缩、跨进程独占维护和强杀恢复仍是发布前门禁，不能把 v35 单事务回归当成这些验收已通过。本批没有安装/重启 App 或升级活动用户库；未改 wire DTO 或协议版本，旧包按原版本上限检查明确拒绝 Schema v35，不能用旧快照自动覆盖新业务数据。具体子决策与回滚语义见 16.2。

### 12.5 第五批实施记录：响应正文总字节预算（2026-10-22）

Schema 升至 v36，在防重身份保持独立的基础上，新增 `local_agent_receipt_sizes` 和单行 `local_agent_receipt_cache_usage`。预算默认且最多为全库共享的 **128 MiB UTF-8 JSON 正文字节**；内部数据库策略可下调，IPC 调用方没有预算或关闭防重的选项。

已实施：

- 每份正文的真实字节数使用 `length(CAST(response_json AS BLOB))` 计量，不能用 SQLite 文本字符数代替。插入/更新/删除、外键级联、时间/数量淘汰和事务回滚均同步维护总字节与条数；`receipt_cache_usage()` 只读单行计数，不在诊断时汇总全部正文。
- 超预算时按 `(created_at_unix_ms DESC, command_id DESC)` 保留最新可容纳的连续前缀，淘汰较旧正文；单份超预算正文先排除。淘汰扫描尺寸/排序元数据，不反复读取大正文。数量与总字节淘汰的扫描均由单行用量先行判断，未超限不做该轮扫描。
- `record_receipt` 改用有界序列化缓冲，不先构造超大完整 JSON String。单份响应超过整个缓存预算时释放缓冲，继续序列化验证剩余内容但不再保留正文，仅在原业务事务中保存防重身份；正常首次调用仍返回业务结果，重试明确得到 `command_replay_expired`。序列化错误即使出现在超过预算之后也仍使整个写事务失败，不把错误结果当作成功身份。
- v36 迁移在同一事务内由 SQLite 回填尺寸并执行字节裁剪，不将历史响应加载到 Rust；v35 已先保全身份。元数据回填失败回滚 v36 表、触发器与尺寸变更，保留原响应和迁移前快照。v35 → v36 不新增自动 `VACUUM`；正文删除仅释放逻辑空间，不承诺主库/WAL 立即缩小。

验证：新增 16 项测试。存储层 155 项、Runtime 62 项、协议 37 项、Host 库 112 项及可执行入口 5 项，共 371 项全量通过，无忽略或排除；存储层与 Runtime 全目标严格 Clippy 通过。新增覆盖 UTF-8/转义的真实字节计量、预算恰好等于边界、缓冲增长上限与溢出释放、相同时间戳排序、超额淘汰后的身份与 mismatch、更新/删除/级联计数、回滚恢复被淘汰正文、溢出后的序列化错误、下调内部配额、迁移回填/中途失败/重新打开、两个独立连接共享同一预算，以及真实 Run 在超大响应时只执行一次、计数写失败时业务/事件/身份/计数整体回滚。淘汰行为使用缩小的测试配额验证同一 SQL/序列化路径，另校验生产默认值为 128 MiB；这不是 128 MiB 真实磁盘耗尽或性能压测。

边界：这是全库正文总量硬限制，不是每 owner 公平配额；长期 tombstone、请求摘要、SQLite 页/索引、WAL 与其他业务表不计入该正文预算。本批时尚未实现 512 KiB inline 目标与不可变大结果引用，后续由 12.6 补齐存储边界；整个 IPC/业务 DTO 的内存与单帧大小仍不由此得到保证。预算淘汰意味着历史结果可能更早不可重放，调用方必须保留原 key 并对账，不能自动换 key 重做。启动迁移的空间/耗时与独占维护、实际大库故障注入、物理回收、长期身份容量规划仍未验收。本批未改 Swift、wire DTO/协议版本，未安装或重启 App，也未迁移、压缩或清理活动用户库。v36 子决策见 16.3。

### 12.6 第六批实施记录：512 KiB inline 边界与不可变结果引用（2026-10-22）

Schema 升至 v37。序列化后 UTF-8 JSON 正文 **不超过 512 KiB（含等号）** 时仍内联；超过此边界且能装入整个缓存预算时，回执的 `response_json` 为空，仅用独立的 `result_digest` 列引用 `local_agent_receipt_results` 中的不可变原始 JSON。引用为固定 74 字节的 `sha256:v1:` 摘要，不借用业务实体的当前状态重建历史响应，也不通过猜测普通 JSON 字段来区分引用。

已实施：

- 相同序列化字节在不同命令间共享一份结果正文；复用既有摘要时在 SQLite 内比较原始正文和长度，发现损坏/碰撞使整个业务事务失败。结果表拒绝 UPDATE；回执引用发布后也不允许改指向。每次重放先验证命令身份/指纹，再读取并校验引用正文的长度和 SHA-256，缺失或损坏返回不可重试的 `storage_corrupt`，绝不按“未执行”继续业务写入。
- 引用结果与 inline 共用 v36 的 128 MiB 正文预算、时间高水位与 5,000 条限制。即使共享一份物理正文，也按每个命令的完整响应字节保守计费，不能只计 74 字节引用。触发器用尺寸元数据维护计数；只有最后一个引用被删除才回收正文，引用索引避免按正文扫描所有者。时间/数量/字节淘汰、下调配额与立即淘汰均在同一事务内回收，tombstone 不删除；结果超过整个预算时仍只记身份，不创建结果正文。
- 正文、引用、身份、业务与事件全部写在同一个 SQLite 事务中，迁移快照也包含正文，不引入跨文件/数据库的原子性假设。业务或计数写失败时，连同新正文和本事务内的旧正文淘汰一起回滚，不留下孤儿结果或悬空引用。
- v37 利用 v36 尺寸元数据筛选旧大回执，按主键逐条转存原始字节；不解析重排 JSON，不改变逻辑用量，也不同时把全部结果加载到 Rust。转换与新表/列/触发器/版本标记同一迁移事务，失败整体恢复 v36，已有迁移前恢复快照保留；重试可继续完成升级，不自动降级或覆盖新数据。

验证：新增 14 项回归。存储层 169 项、Runtime 62 项、协议 37 项、Host 库 112 项及可执行入口 5 项，共 385 项全量通过，无忽略或排除；存储层与 Runtime 全目标严格 Clippy 通过。覆盖真实 512 KiB 边界（含 UTF-8/转义）、原始字节迁移、普通 JSON 不误识别为引用、SQL inline 限制、正文/引用不可变与外键、相同结果共享与最后引用回收、混合正文配额/下调/超限、时间/数量/立即淘汰、缺失/摘要损坏与 mismatch 优先、回滚恢复已回收正文、跨连接并发/重启、v36 快照及迁移中途失败重试，以及真实大 Run 更新后精确重放、损坏时不再产生事件、同 key 并发只提交一份业务/事件/结果、计数失败时全部回滚。此处是临时库正确性验证，不是生产数据迁移或性能压测。

边界：这是数据库内的不可变结果引用，不是 wire 文件句柄或分页协议。首次不同的大结果仍需完整序列化、计算摘要并落库；仅相同结果之间减少重复的缓存正文，不消除 Run/Event/Tool 等业务表中的重复数据，也没有证明 WAL、RSS 或实际延迟已下降。缓冲仍受整个缓存预算而非 512 KiB 限制，IPC 返回值/业务 DTO 仍可能很大；引用重放还增加摘要校验成本。128 MiB 约束提交后保留的逻辑正文，不是插入后淘汰前的事务瞬时空间、主库/WAL 或迁移峰值限制；旧正文转存会产生迁移写入，不自动物理缩库。公平配额、长期身份容量、独占维护、真实大库/磁盘满/强杀和发布性能门禁仍待完成，不能将 P0-C 标为全部完成。本批未改 Swift、wire DTO 或协议版本，未安装/重启 App，未访问或修改活动用户数据库。v37 子决策见 16.4。

### 12.7 第七批实施记录：Host 数据库进程级独占所有权（2026-10-22）

审查生产调用路径后，Local Agent 数据库的实际打开入口是 Host `main.rs`；Swift Agent 团队、记忆及项目注册使用其他存储。此前 `BEGIN IMMEDIATE` 只能串行化单个 SQLite 写事务，不能阻止两份 Host 分别进行启动备份、迁移、压缩和调度。本批在 Host 创建 SQLite 连接池之前取得数据库专属 OS 非阻塞独占锁，并覆盖整个进程生命周期。

已实施：

- 按实际数据库路径派生持久的 `<database>.owner.lock`。已有数据库文件和父目录的 symlink 别名先解析为同一锁身份；数据库尚不存在时也先解析父目录，不能让两份首次启动分别建库。Unix 拒绝数据库硬链接、悬空数据库 symlink、锁文件 symlink/硬链接与非普通文件，并在获取锁后再次核对锁文件 inode；锁文件权限为 `0600`。锁身份与诊断使用解析后的路径，SQLite 与 `.artifacts` 继续沿用配置路径，避免悄悄改变旧 artifact 根目录。
- 获取锁失败直接从启动入口退出，stderr 明确要求先停旧 Host，不污染 IPC stdout；此时不打开 SQLite、不创建业务库/连接池/备份或执行迁移，也不截断当前所有者的诊断内容。不同账户仍不能对同一库各开一份 Host；不同数据库各有独立锁。
- 锁句柄由进程级 `OnceLock` 持有，不在备份或迁移后提前释放，也不依赖异步 `main` 栈帧的析构顺序；Tokio/SQLite 后台线程关闭期间仍持锁，由操作系统在进程真正退出或被强杀后释放。退出不 unlink 锁文件，防止旧 inode 仍被持有时，新进程锁住另一个 inode。
- 成功取得内核锁后才覆盖诊断，记录格式版本、随机 `owner_epoch`、PID 和解析后的数据库路径。残留/损坏 JSON、过时 PID 或 epoch 不决定所有权，不设置“超时后抢锁”，也不通过删除锁文件强行接管。epoch 目前只是此次持锁进程的诊断身份，不是已经接入事务/IPC 的 fencing token。

排查记录：首轮路径断言因 macOS `/var` → `/private/var` 别名失败；明确区分解析后的锁身份与必须保留的配置/Artifact 路径后补齐断言。扩大严格 Clippy 范围后，`planner.rs` 的 `!Option::is_some_and(...)` 触发 `nonminimal_bool`，改为等价的 `Option::is_none_or(...)`，不改变 pending-review 阻止异步交接收尾的条件。随之纳入的 Profiles 全量测试又暴露 `structured_final_delivers_checked_text_not_a_later_model_rewrite` 的旧断言与当前协议矛盾：协议/Host 已允许执行正常结束并独立报告 `failed`/`not_verified` 验收发现，旧断言却要求 `not_verified` 变成执行错误。未改生产验收语义；将该断言补成保留原 final_response 与两类验收发现的验证，并保留未知 verdict 必须拒绝的负例，不排除失败项或放宽警告。

验证：新增 6 项锁/路径单测和 4 项真实 Host 子进程回归。存储层 169 项、Runtime 62 项、协议 37 项、Host 库 112 项、Host 可执行入口 11 项、子进程集成 4 项及 Profiles 59 项，共 454 项全量通过，无忽略或排除。存储层、Runtime、Host 与 Profiles 全目标严格 Clippy 通过；本批修改文件格式检查及 `git diff --check` 通过。覆盖同库/跨账户互斥、不同数据库独立、未建库先持锁、活 owner 诊断不被截断、残留诊断可重获锁、symlink 别名/硬链接/不安全路径/权限/inode、锁竞争时不打开数据库或创建备份、第二 Host 被拒后首个仍健康、正常 EOF 退出/空闲 SIGKILL/启动失败后的新进程与新 epoch。所有真实进程均使用本次构建的开发 Host 和临时库，非已安装 App，也非 Release 压力验收。

边界：这是生产 Host 入口的协作式进程锁，不是强制阻止所有 SQLite 工具写入的安全沙箱；存储适配器低层 `connect_file()` 仍允许工程测试/受信内部代码建立多个连接，任何未来独立维护工具都必须先加入同一所有权协议。不认识此锁的旧包、绕过协议的进程，以及持锁后恶意替换锁文件或数据库路径，都不由它自动防住；路径及父目录必须受信且稳定，锁文件不得手工删除，发布升级仍需停旧 Host 并确认退出。未实现在线维护的 quiesce/排空/取消/恢复状态机、epoch fencing、跨版本受控交接、迁移各提交窗口强杀或真实磁盘耗尽验收；本批真实强杀只覆盖临时测试库中已启动且空闲的 Host。未升级 Schema（仍为 v37）或 wire 协议，未改 Swift、未安装/重启 App、未访问活动用户数据库；不能据此宣称 P0-C 生产迁移门禁已通过。子决策见 16.5。

### 12.8 第八批实施记录：迁移期间 SIGKILL 与恢复回归（2026-10-22）

本批补齐迁移异常退出的可重复验证，不新增 Schema 或生产暂停开关。所有停点及故障驱动均限定在 `#[cfg(all(test, unix))]`：正常库、Host 及 Release 构建不包含这些分支，也不读取测试暂停环境变量。测试启动存储包自身的单测可执行文件，在子进程内调用真实 `connect_file()`；父进程等待带 PID/停点身份的标记，再发送真实 SIGKILL 并检查退出信号为 9，不以普通报错或 Rust 析构模拟强杀。等待有超时，子进程提前退出或未到停点时报告 stdout/stderr，且丢弃子进程句柄会终止它。

临时 v34 测试库含一份真实已提交的 Run/事件，以及两份各约 1 MiB、内容不同且带原始空白的历史 JSON 回执；预先保留一份校验通过的恢复快照。新增 12 个强杀场景：

| 停点 | 强杀后必须保留的已提交状态 |
| --- | --- |
| 私有备份文件占位；备份已写入但尚未校验；备份已校验、同步并发布 | 源库仍为 v34；前两种只有 `.incomplete`，第三种为完整 v34 快照 |
| 第一条防重身份回填后；v35 COMMIT 前；v35 COMMIT 后 | 前两种恢复 v34，不留部分身份/版本标记；提交后完整保留 v35 |
| v36 尺寸回填后；v36 COMMIT 前；v36 COMMIT 后 | 前两种恢复 v35，不留部分尺寸/计数表；提交后完整保留 v36 |
| 第一条大结果转存后；v37 COMMIT 前；v37 COMMIT 后 | 前两种恢复 v36，不留部分结果表/引用；提交后完整保留 v37 |

每次强杀后先直接只读检查源库，再允许正常重试迁移：完整 `integrity_check`、`foreign_key_check`、连续版本标记、业务/事件/回执数量、防重指纹与身份一致、原始 JSON 字节、对应版本的尺寸计数/预算及结果表必须正确。重试到 v37 后精确重放原业务及两个大结果，确认 Run/事件没有重复，逻辑响应字节与数量精确。原已验证快照、当次已发布或未完成文件都做 SHA-256 前后比较，重试不能改写它们；重试新建的快照也按重启时最后已提交的 v34/v35/v36 状态做完整检查。已完成 v37 后重开不额外制造迁移快照。

排查记录：存储包单独编译首先暴露 Tokio 未声明 `process` 测试特性，已在该包 dev-dependencies 显式增加 `process`/`time`，不依赖与 Host 一起构建时的特性合并；另修正测试中两处弃用 API 警告，没有跳过失败项或放宽警告。

验证：新增 12 项强杀回归和 1 项子进程辅助入口测试。存储层 182 项、Runtime 62 项、协议 37 项、Host 库 112 项、Host 可执行入口 11 项、Host 子进程集成 4 项及 Profiles 59 项，共 467 项全量通过，无忽略或排除。存储层、Runtime、Host 与 Profiles 全目标严格 Clippy、正常非测试 Host `cargo check`、本批文件格式检查及 `git diff --check` 通过。独立复验可执行 `cargo test --locked -p chatos_client_storage migration_crash_tests`；全量仍使用 12.7 所列七组测试对应的五个包，不将辅助入口算作第 13 个强杀场景。

边界：本批强杀的是执行真实存储路径的测试子进程，不是实际 Host；12.7 的真实 Host 锁生命周期回归与本批迁移回归是两组证据，尚未证明两者组合的所有窗口。停点覆盖备份前后、部分回填及 COMMIT 调用前后，不覆盖 `VACUUM INTO` 执行内部、SQLite COMMIT 内部任意 I/O 窗口、断电/系统崩溃、真实 ENOSPC、旧 `<33` 路径的自动压缩、生产大库空间/耗时、旧 Host 升级交接、在线维护状态机或 Release 性能。测试进程 SIGKILL 不等于断电持久性证明。未安装/重启 App，未访问或修改活动用户数据库；P0-C 生产迁移门禁仍未全部通过。

### 12.9 第九批实施记录：COMMIT 失败回滚与连接收尾隔离（2026-10-22）

本批发现并修复真实事务泄漏，不是单纯增加测试。原 `finish_write()` 在业务语句成功后执行 `COMMIT`，提交报错即直接返回；但 SQLite 的延迟外键等提交错误会保留事务。当前使用裸 SQL `BEGIN IMMEDIATE`，未建立 SQLx `Transaction` RAII 对象，不能假设池连接归还时会自动回滚。新增三个回归首先在原实现上失败：提交被拒后，同一个池连接仍能读到未提交的 Run；事务内已经淘汰的旧大结果也暂时不可重放。这证明失败请求的中间状态可能泄漏给后续请求。

已实施：

- 将写事务辅助逻辑集中到 `transaction.rs`。业务成功且 COMMIT 确认成功才返回结果；COMMIT 失败与业务失败都执行 ROLLBACK，始终返回原始业务/提交错误，不用清理错误覆盖原错误分类与信息。不自动换 key、不自动重新执行外部副作用，也不将提交失败当作确定的“未执行”。
- 收尾函数显式接收池连接，并以同步析构守卫保护 COMMIT/ROLLBACK 的异步等待。只有确认提交或回滚完成才允许该句柄继续回池；回滚失败或收尾 future 被取消时标记 `close_on_drop`，连接释放后关闭而非复用，由连接池按原上限补建。正常提交/回滚不关闭健康句柄，避免给普通业务拒绝增加连接重建开销。
- 守卫不伪造取消的事务语义：SQLite 后台线程可能在调用方取消后继续完成 COMMIT。测试用真实 SQLite commit hook 暂停后台线程，先取消 Rust future，再允许 SQLite 提交，确认提交数据保留但原句柄不被复用。调用方仍须使用原命令 key 重试/对账，不能因取消另起一次操作。

验证：新增 7 项回归，覆盖 inline/引用大结果在真实 COMMIT 失败时的 Run/事件/正文/身份/计数整体回滚、用原 key 成功重试且只产生一份事件、恢复被淘汰的大结果与用量、健康提交/回滚保留同一连接、真实 SQLite progress interruption 导致清理失败时保留原业务错误并更换连接，以及 COMMIT/ROLLBACK 执行期间取消等待。单连接内存池证明读到的是原句柄的状态；单连接临时文件池配合 TEMP 表证明失败/取消后的物理句柄确实被替换，并确认已提交基线仍在、下一事务能启动。

全量结果：存储层 189 项、Runtime 62 项、协议 37 项、Host 库 112 项、Host 可执行入口 11 项、Host 子进程集成 4 项及 Profiles 59 项，共 474 项通过，无忽略或排除；存储层、Runtime、Host 与 Profiles 全目标严格 Clippy、正常非测试 Host `cargo check`、本批文件格式检查和 `git diff --check` 通过。定向复验：`cargo test --locked -p chatos_client_storage transaction_tests`。

边界：守卫从进入 `finish_write()` 才开始保护；BEGIN 等待或业务 future 在调用收尾前被取消的完整事务生命周期治理仍待补齐，不能宣称所有取消路径都已安全。关闭不确定句柄不等于回滚已提交数据，也不保证外部文件/provider 副作用回滚。本批使用延迟外键与 SQLite 回调在临时库中制造真实错误/等待，不等于真实 ENOSPC、掉电或生产大库验收；异步关闭与补建的压力/资源指标仍需 Release 测量。未改 Schema v37、wire 协议或 Swift，未安装/重启 App、未访问活动用户数据库，P0-C 生产门禁仍未全部通过。

### 12.10 第十批实施记录：写事务全生命周期取消保护（2026-10-22）

本批补齐 12.9 明确保留的 BEGIN/业务阶段缺口。四个新回归先在旧实现上失败：BEGIN 执行中取消、业务 SQL 执行中取消、业务已写完但未进入收尾时取消，以及业务 panic unwind，都会让原池连接未经清理就复用。此前收尾守卫尚未创建，不能保护这些窗口；SQLite 工作线程还可能在 Rust future 消失后继续执行语句。

已实施：

- 引入拥有池连接的 `WriteConnection`，所有现有生产写入口和逐版本迁移统一经 `acquire_write()` 获取。`begin_immediate()` 与 `finish_write()` 的参数类型要求该守卫，裸池连接无法直接传给这两个入口；普通只读路径仍沿用原连接池。BEGIN 派发之前就标记待清理，不能等到 BEGIN 返回成功后再开始保护。
- BEGIN/业务阶段取消、panic unwind 或提前返回时，守卫将连接连同原 pool permit 交给独立回滚任务；permit 在回滚确认前保持占用。回滚排在同一 SQLite 线程已经派发的语句之后，确认成功才允许句柄回池，不向后来请求暴露待清理状态。不靠立即关闭健康的单连接内存库来通过测试，正常取消回滚后其既有 Schema 与已提交数据仍保留。
- 回滚任务进入执行前就持有失败即关闭的所有权守卫，且由消费整个守卫的 future 承载；即使 future 从未轮询便被丢弃、执行中被取消或真实回滚报错，也关闭句柄而非回池。保存原 Tokio runtime handle，析构发生在 runtime enter 范围外时仍能正确调度清理/释放；显式声明 `rt` 依赖，不靠其他包合并特性。
- 正常显式提交/回滚仍直接完成，不为每次业务写新增后台任务。12.9 的更严格收尾策略保留：COMMIT/ROLLBACK 等待被取消或清理失败时直接退休句柄，不能因为 SQLite 最终可能提交成功就猜测结果。调用方仍使用原 key 重试/对账，不把取消转成另一份副作用。

验证：新增 10 项回归，覆盖上述四个复现窗口、内存库取消后的已提交基线、绕过显式收尾的提前错误、后台回滚实际中断、清理 future 未轮询被丢弃、清理执行中取消，以及 runtime enter 范围外析构。单连接临时库用 TEMP 表验证成功回滚后健康句柄可复用、异常清理后确实换句柄；SQLite 回调在真实执行期间阻塞，先等待 Rust 任务取消完成再释放后台线程，并检查清理期间 permit 不可重新领取。业务取消还验证 Run 版本、Event、长期身份、大正文、用量共同回滚，未提交的原 key 可以正常重试。原迁移 SIGKILL、提交失败及收尾取消回归继续保留。

全量结果：存储层 199 项、Runtime 62 项、协议 37 项、Host 库 112 项、Host 可执行入口 11 项、Host 子进程集成 4 项及 Profiles 59 项，共 484 项通过，无忽略或排除。存储层、Runtime、Host 与 Profiles 全目标严格 Clippy、非测试存储包/Host `cargo check`、本批文件格式检查及 `git diff --check` 通过。可执行 `cargo test --locked -p chatos_client_storage transaction` 复验事务相关场景；该筛选也包含两项原有事务/并发读取回归，不将筛选总数当新增测试数。

边界：这是本地 SQLite 写连接状态隔离与回收，不是立即中断 SQLite 工作线程的硬时限；慢语句仍可能延迟回滚和释放 permit，取消风暴、长查询、关闭时资源回收及 Release 指标仍需测量。panic 用例为 unwind，不是 panic=abort、系统掉电或实际 App 退出验收。测试覆盖的是清理任务被丢弃的机制，不声称所有真实 Host 关闭窗口已验证。若清理失败必须关闭唯一 `:memory:` 连接，内存库本身不具备跨连接恢复能力；本批验证其成功回滚路径，生产 Host 使用文件库。SQLite 回滚不撤销已发生的文件/provider 副作用，外部一致性仍按 outbox/对账协议治理。新增手写裸事务若绕过这两个受类型约束的入口，仍须另行审查。未改 Schema v37、wire 或 Swift，未安装/重启 App、未访问活动用户库；真实 ENOSPC、生产大库、掉电、在线维护与 Release 门禁仍待完成。

### 12.11 第十一批实施记录：Release 构建证据与固定工程基准入口（2026-10-22）

本批回到 P0-A，补构建与测量的可追溯性，不继续扩展存储 Schema。新增 `clients/macos/scripts/build_evidence.py` 与 `run_performance_baseline.py`，使用 Python 标准库；具体用法和证据边界见 `clients/macos/scripts/README-performance.md`。

- 打包前记录源码清单、内容/可执行位/symlink/缺失文件摘要、Git index、Swift/Rust profile、工具链/SDK、协议/Schema 源码版本、UTC 与硬件/供电/原始热状态；收尾再次校验输入未改变。Swift 固定 resolved 依赖，Cargo 使用 `--locked`。现有 dirty Release 门禁保留；允许的 dirty 诊断包不能被分类为干净 Release。
- Release 显式保留调试信息并生成 App、Host、ps 的 dSYM，要求 UUID/架构对应唯一成品；签名后记录完整 bundle 与符号清单。外置 manifest 不修改已签名 bundle；验证入口检查签名、成品和符号，防止旧 manifest 配新二进制或资源。此为本机诊断证据，不是供应链签名、可移植符号归档或公证验收。
- 提供三轮 Release 合成存储工程基准，每轮沿用现有三次重复；保留原始数据、测试二进制哈希/UUID、日志、环境和合并样本分位数。未测量指标不补零；未运行 Rust 的 profile 为 null。夹具规则/源码哈希不冒充数据库逐字节哈希，正式数据集冻结仍待完成。跳过、缺失/重复输出、样本异常、执行失败、超时或输入/产物变化均拒绝通过，保留失败证据。
- 首次真实执行暴露运行器架构问题：`/usr/local/bin/python3` 是 x86_64，经 Rosetta 拉起要求 x86_64 的 XCTest，但 Swift 已生成 arm64 bundle，导致 dlopen 架构不兼容，尚未运行测试。这不是功能用例断言失败，也不是有效性能样本。已显式选择本机 Swift/XCTest 架构，并区分本机与采集器架构；保留首轮 failed manifest/日志，不将其算作基准通过。

架构约束纠正：本项目 macOS 已明确只支持 arm64。首轮运行器错误选择 Intel Python 后，曾加入 Intel 兼容选择，属于本批实现偏离约束，现已撤掉；统一入口固定系统原生 Python 与 arm64 Swift/XCTest，不支持 Intel/universal 或 x86_64 回退。打包和验证拒绝非原生 arm64 shell，成品四个可执行均须 arm64-only。架构修正后另暴露 Release XCTest 资源定位错误，尚未取得有效三轮测量，不与 Windows 打包混为一谈。

此前脚本回归 23 项通过，覆盖源码/index/资源变化、dirty 分类、元数据不符、dSYM UUID 对应、缺失/重复/畸形测量、原始样本分位数和失败/超时留证；收紧 arm64-only 后补充拒绝 Intel/未知硬件及三个 shell 入口提前拒绝的回归，统一用系统原生 Python 复验。实际三轮基准及完整打包链路尚未通过；不能用这些工具回归宣称 P0-A 已验收。

剩余 P0-A：固定干净实施提交、正式数据集与导航脚本、页面/IPC/SQLite/内存 signpost/trace、同机基线与候选对照。当前工作区未提交，不自动提交、清理或安装 App；正式发布基线仍不能建立。后续依次推进 Story 崩溃/状态域、统一 Governor 与媒体文件化，再按前置门槛决定 Worker；其余阶段和生产数据库门禁未因本批工具落地而完成。

### 12.12 第十二批实施记录：Host/Swift Agent 共享预算与 claim 前准入（2026-10-22）

本批纠正实施重心长期停留在 SQLite 边界的问题，开始推进 P0-D。此前 App 为 Rust Host 单独按逻辑处理器一半配置 1–16 个 work item，而 Swift Agent 的 `batchSize = 64` 会先 claim 一轮 work、再为每项创建无独立上限的 Task；两个局部默认值可能相乘争抢 CPU、内存带宽和磁盘，且提前 claim 的工作可能在等待执行时消耗 lease。

已实施：

- 新增进程组逻辑 job 预算策略。未配置时严格采用第 6.3 节的保守起始总额：常规机器总额 3，默认 Rust Host 1、Swift Agent 2；两核及以下总额 2，各 1。保留两条 Swift lane 是为了让 manager 通讯能与长 executor Run 并行。`CHATOS_BACKGROUND_JOB_BUDGET`/`ChatOSBackgroundJobBudget` 可将总额配置为 2–16；旧 `CHATOS_LOCAL_AGENT_MAX_CONCURRENCY`/Info.plist Host 覆盖被夹在总额内，至少给 Swift Agent 保留 1 个额度，不能再独立放大两套默认值。
- `LocalAgentGroupChatScheduler` 新增实例级共享执行 Governor，覆盖 project、conversation、account、communication fast lane 与 interrupted-run recovery。所有 drain 副本共享同一 actor，不因窗口或入口不同复制额度。
- permit 在 `claimNextDelivery` 之前取得；一轮已有可运行 work 时只尝试立即取得剩余额度，额度已满便立刻启动现有 work，不持 permit 等待更多 permit。某 Agent 没有第二 lane work、claim 报错、取消、注册冲突及正常完成都会归还额度；异常析构保留幂等兜底。
- 首轮回归暴露“空 claim 异步归还导致同轮并行度从 2 降到 1”，已改为正常路径确认后同步归还，再启动下一个 Agent。测试证明上限为 2 时四个 Agent 全部完成且模型调用峰值精确为 2，并继续保留 manager/executor fast lane。

边界：本批是 P0-D 的第一段，不是完整 `ResourceGovernor`。额度目前只约束 Rust Host 启动配置与 Swift Agent 执行；Media、Plugin、数据库维护、大文件 I/O 尚未接入同一动态 Governor。没有实现前台交互时从 3 降到 2、热状态/低电量迟滞、跨账户公平、permit 指标/诊断页、进程间动态 lease 或嵌套 Plugin 借用协议。Host 配额只在进程启动时固定，修改配置需重启；外部独立启动的 Host 仍使用 Rust 自身默认。不能据此宣称组合负载和切页 SLO 已通过。

验证：Debug 下 Scheduler 调度、恢复、取消和 fast-lane 20 项，加预算/取消回收 4 项，共 24 项通过；arm64 Release 将上述 24 项与资源定位 7 项合并复验，共 31 项通过。此前 P0-C 所涉 Rust 五个包全量复验：Profiles 59、Storage 199、Host lib 112、Host main 11、真实 Host 子进程 4、Protocol 37、Runtime 62，共 484 项通过，无失败、忽略或排除。严格 Clippy 首轮在 Protocol 的既有测试代码发现 `items_after_test_module` 与单元素 `useless_vec` 两项当前工具链警告；等价移动 helper、改用数组后，五包 `--all-targets -D warnings` 通过，Protocol 37 项复验通过。构建/基准工具 25 项 Python 回归、三个 shell 语法检查、Rust 全量格式检查和 `git diff --check` 通过。

同时关闭 12.11 留下的 Release XCTest 资源定位阻塞：SwiftPM XCTest 能从产品目录兄弟 bundle 加载资源，打包 App 仍只允许密封的 `Contents/Resources`，不回退到构建机路径。基准 shell 首次直接执行又暴露未设置 executable bit，已补齐；随后第一次真实采集期间本共享工作区的 `plugins/browser` 被另一条工作流修改，输入门禁正确将整轮标为 failed，未混入结果。输入稳定后完成三轮 arm64 Release 合成存储基准，每轮内部重复 3 次、测试二进制 SHA-256/UUID 一致；合并 9 个原始样本结果如下：

| 工程测量 | p50 | p95 / 最大值 |
| --- | ---: | ---: |
| Workspace snapshot | 10.363 ms | 12.251 ms |
| 500 次空闲 heartbeat poll | 13.520 ms | 19.022 ms |
| 500 次 Run change 发布 | 0.618 ms | 1.350 ms |
| 最近消息查询 | 0.366 ms | 0.822 ms |
| 128 KiB 图片附件读取 | 0.649 ms | 1.585 ms |
| Store 打开 | 1.356 ms | 4.410 ms |
| 空闲 account drain | 0.224 ms | 0.562 ms |

三轮空闲 heartbeat/account drain 的 SQLite change 数均为 0。证据位于本地 ignored 目录 `clients/macos/.build/performance/store-release-9zb5yb7d/`，manifest 状态为 `completed`、`performance_acceptance` 为 `not-evaluated`，`source_input_sha256` 为 `5802dfb6c50b001bf5760d272e5723c5b6b05fdcb7c11f8b45ae9522d0f1e027`；该路径可用于本工作区复核，不是应提交或长期归档的位置。采集完成后的本文记录和两处 Protocol Clippy 清理不属于该输入摘要。以上只证明合成存储热路径，不替代同机候选/基线对照、真实 App 导航、Host + Agent + Media 组合负载、内存/能耗 trace、干净打包或发布 SLO。

### 12.13 第十三批实施记录：Story 高频状态域与进度写放大收敛（2026-10-22）

本批回到 P0-B/P1 的 Story Studio。此前 `StoryStudioViewModel` 在显式稳定 `objectWillChange` 之外仍直接发布视频百分比、批处理进度和模型 token 流；一次 provider 轮询会更新大项目数组，模型每个 token 又会重复发布相同的 `operation`。一键制作路径还会把相同 `queued`/`in_progress` 阶段下的每个百分比回调追加为历史事件，并通过批次/项目清单产生写放大。

已实施：

- 新增独立的 `StoryGenerationActivityState` 与 `StoryPlanningActivityState`。视频百分比、批处理瞬时进度、模型文本流和工具名不再是根 ViewModel 的 `@Published`；根模型保留兼容只读访问和业务方法，但不会转发子状态的 `objectWillChange`。账户 reset 同时清理两个状态域。
- Agent 实时输出面板直接观察 planning 状态；视频百分比由分段详情中的小型叶子 View 直接观察 generation 状态。根 Story 工作台不观察这两个对象，不能把嵌套对象换个名字后继续让整棵树随 token/percent 重算。
- 文本 token 流仅在“分析/文本输出/工具组装/完成”阶段真实切换时更新根 `operation`，不再为连续 token 重复发布同一个字符串。回归用 200 个连续 token 证明局部状态发布 200 次而根 ViewModel 发布 0 次。
- 单段视频适配器增加 attempt 内的持久化门：只有 provider 状态阶段或 job ID 改变才进入 Story store；百分比变化仍实时送到局部 UI。Store 自身继续做第二层幂等检查，相同 status/job ID 不更新 `updatedAt`、不重写项目清单。
- 收到单段视频事件时，只有 attempt 身份、job ID、最终视频或错误等项目结构变化才发布大项目数组；纯 status/percent 走 activity 状态。任务开始/结束、durable claim、恢复与导航锁定语义未改变。
- 持久化 Media Batch 同样只在状态阶段或 job ID 改变时追加事件和提交清单。100 个模拟回调（前 50 个 `queued`、后 50 个 `in_progress`）最终只产生各 1 条阶段事件，不再形成 100 轮批次/项目写入。

验证：Debug 与 arm64 Release 分别运行 Story Studio 71 项，均全部通过；其中新增 5 项覆盖根/局部 publisher 隔离、重复 activity 合并、reset 清理、相同视频状态不重写项目清单，以及 100 次 Batch 进度风暴收敛为两个持久化阶段。`git diff --check` 继续作为最终工作区门禁。

边界：本批没有把开始/结束才变化的 asset/frame/video active 集合搬出根模型，以免为低频状态扩大编辑锁定与跨页面导航重构；后续应基于 Instruments 的失效图决定是否继续拆分。Provider 状态字符串变化仍会持久化，这是崩溃恢复所需的低频事实；没有新增丢进度定时器。尚未完成真实 App 的 200 次导航、进度风暴 signpost、内存/能耗 trace 或 Combine 崩溃设备复现，因此不能据单元测试宣称 P0-B/SLO 已全部验收。媒体 `Data`/Base64 文件化和 Worker 仍属后续 P0-E；本批未安装/重启 App、未访问活动用户数据库，也未清理共享工作区的既有改动。

### 12.14 第十四批实施记录：Story 参考视频文件原生上传（2026-10-22）

本批推进 P0-E 的第一段，消除 Story 续写/重绘参考视频在提交阶段同时驻留“完整视频 `Data` + 约 4/3 大小 Base64 `String` + 完整 JSON `Data`”的确定性内存放大。单段适配器和持久化 Media Batch 现在只传递受控项目文件 URL，不再以 512 MiB 上限整段读取后编码。

已实施：

- `VideoGenerationInputVideo` 增加文件引用模式；原 Base64 初始化器保留给已有调用和测试。文件模式要求 Base64 为空，禁止两种所有权同时存在。
- 新增请求文件写入器：以 `O_NOFOLLOW` 打开源文件并对已打开描述符执行 `fstat`，只接受普通文件；源文件限制为 1–47 MiB，临时请求文件以 `O_EXCL` 和 `0600` 创建。视频按 48 KiB（三的倍数）分块 Base64 编码并直接写入 JSON 请求文件，过程中检查取消，最终请求严格限制在 64 MiB 内；失败或取消会删除未完成文件。
- `HTTPRequest`/`HTTPTransport` 增加显式 file-backed body。生产 `URLSessionHTTPTransport` 使用 `uploadTask(...fromFile:)`/`upload(for:fromFile:)`，并继续通过有界 delegate 接收响应，不把请求文件重新物化为 `Data`。测试和旧 transport 的默认实现保留兼容性内存 fallback，但它不属于生产 URLSession 路径。
- Media Service 在提交前后继续校验绑定的认证 session；临时请求文件在成功、provider 拒绝、网络错误、取消和账户切换错误路径上均由统一 `defer` 回收。普通 Base64 视频调用保持原协议行为。
- Story 的 previous-video 续写和 source-video 重绘两条路径均改为文件引用；现有测试同时验证 URL 指向的字节仍是预期视频，避免仅验证字段形态而漏掉业务内容。

验证：Debug 下 Media Service 24 项、HTTP transport 7 项、Story Studio 71 项通过；arm64 Release 合并运行同一组 102 项，全部通过。覆盖增量请求内容与 data URL 完整性、发送时请求文件可读、成功/失败后清理、symlink 拒绝、47 MiB 超限在 POST 前拒绝，以及 file upload 响应继续受上限约束。测试 transport 为验证内容会物化临时 JSON；生产 URLSession 路径另有 `fromFile` 回归，不能把测试 fallback 描述成生产内存行为。

边界：这是 P0-E 的 Story 参考视频上传切片，不是整个媒体管线文件化完成。视频下载结果仍以 `VideoGenerationResult.videoData` 返回，图片和参考音频输入仍可能保留 Base64，Provider 真实 47 MiB 边界、取消风暴、磁盘满、长时间网络上传、主进程 physical footprint 与能耗仍需在固定 Release 场景实测。尚未新增 Media Worker、durable 下载 sidecar、断点续传或跨进程文件授权；未安装/重启 App、未访问活动用户数据库，也未宣称 P0-E 或客户端 SLO 已全部验收。

### 12.15 第十五批实施记录：视频结果流式下载与文件化落盘（2026-10-22）

本批继续 P0-E，关闭 12.14 明确保留的下载峰值。生产 Media Service 不再通过 `send` 接收完整视频 `Data`，而是消费 `HTTPTransport.stream`，将响应块直接写入权限 `0600` 的临时文件；`URLSessionHTTPTransport.stream` 基于 `URLSession.AsyncBytes` 的 pull reader，每块最多 4 KiB，不使用会丢字节的 eager push 队列。

已实施：

- `VideoGenerationResult` 增加 `GeneratedMediaFile` 结果形态，生产返回的 `videoData` 为空。文件记录精确字节数并共享一个线程安全的清理所有者；显式消费完成、取消/失败退出或最后一个结果引用释放时均幂等删除临时下载。原 Data 初始化器保留，避免一次性破坏测试和旧 service adapter，但生产 NewAPI 下载不再走该路径。
- 下载先检查 HTTP 状态和可用的 `Content-Length`，响应流累计硬限制为 512 MiB；空响应、越界、流错误、取消和账户 session 改变都会关闭句柄并删除 partial。非 2xx 正文只收集最多 2 MiB 用于脱敏错误，不能借错误页绕开视频上限。
- Story Store 与 Media History Store 同时接受 Data/file 两种结果且要求二选一。文件结果通过 `O_NOFOLLOW` + `fstat` 校验源描述符、核对精确长度，再以 256 KiB 固定缓冲复制到 `O_EXCL`/`0600` 的最终媒体文件；复制在可取消 detached work 中执行，不在 Store actor 或主线程物化完整视频。Story 仍保持“媒体复制完成后才提交项目 manifest”，History 仍保持“媒体完成后才写 record manifest”。
- 单段 Story、持久化 Batch 和通用 Media Studio use case 都在落盘后主动释放临时结果；RAII 清理作为遗漏/异常路径的第二层保障。测试/legacy transport 的默认 `stream` fallback 仍可能先完整 `send`，文档和测试均不把它冒充生产 URLSession 行为。

验证：Debug 针对性运行 API/transport 33 项、Story/History/ViewModel 90 项、Core Media use case 4 项和文件 I/O 14 项，共 141 项通过；arm64 Release 运行同一组 141 项也全部通过。新增覆盖分块内容完整性、超限 partial 清理、结果释放清理、History/Story 文件复制、symlink 拒绝、固定长度核验，以及原 Story 69 项和高频状态 3 项回归。`git diff --check` 通过。

全量 Debug 也已执行，用于审计非媒体回归，但当前共享工作区并非全绿：`ToolSkillCoverageCatalogTests.testProductCatalogCoversRegisteredFamiliesWithProviderScopedOverlap` 的固定数量比实际少 1，`TaskReplyInspectorViewModelTests.testStalledToolTaskCanBeRestartedFromProcessInspector` 的策略/夹具不一致，认证恢复与 Memory Engine 的两个用例仍期待 401/403 不触发当前刷新/重试行为。四项单独重跑均稳定失败，相关测试及业务源码不在本批改动范围内；因此不能把针对性 141 项通过描述为整个 Swift package 全绿，后续需单独核对这些既有契约漂移。

边界：大视频上传和下载的确定性全文件内存往返已从生产 Story/NewAPI 路径移除，但 P0-E 仍未完成。图片/音频 DTO 仍可能持有 Base64，临时文件在进程被 SIGKILL 后尚无 durable reconciliation，提交接受后未记 job ID 的不确定态、MediaJobStore、磁盘空间预检、真实 512 MiB trace、断点续传和 Worker 文件授权仍待实施。当前结果文件仍在主 App 进程内写入；这降低峰值内存，不提供进程级故障隔离。

### 12.16 第十六批实施记录：媒体文件磁盘预算准入（2026-10-22）

本批关闭 12.15 留下的磁盘空间预检缺口。新增共享 `MediaDiskCapacityPolicy`，按目标目录所在文件系统的可用字节执行溢出安全的准入计算；大媒体写入完成后必须至少保留 **256 MiB**。容量无法确认时停止新写入，不把未知容量当作无限空间。

已实施：

- 参考视频的 Base64 JSON 临时请求在创建目标文件前，按已经精确计算的完整请求字节数加 256 MiB 余量准入；空间不足属于 provider POST 前的确定性失败，保持 `requestMayHaveBeenSubmitted=false`，目录中不会留下 partial 请求文件。
- 视频流式下载优先采用合法 `Content-Length` 作为计划大小；缺失时按 512 MiB 硬上限预算。下载临时文件通常还要与 Story/History 最终副本短时共存，因此准入同时保留一份同等大小的后续副本空间和 256 MiB 余量：未知长度响应的保守门槛为 1.25 GiB。声明了长度的响应在流结束时必须与该长度精确相等，截断响应删除 partial 并拒绝交付。
- Story/History 从临时结果复制到最终目录前，在目标卷按精确文件大小再次准入；检查发生在 `O_EXCL` 创建目标之前，空间不足不会留下 partial 媒体。History 的唯一未提交记录目录在复制或 manifest 提交失败时一并回收，不再留下无 `record.json` 的空目录。
- 统一错误保留提交阶段语义：请求文件空间不足是提交前失败；provider 已完成后的下载空间不足标记为“请求可能已经提交”，调用方不得因此自动换 key 再生成并重复计费。

验证：Debug 与 arm64 Release 分别运行 Media Service/HTTP transport 35 项、磁盘策略 3 项、文件 I/O 15 项、History 9 项及 Story 文件结果落盘 1 项，共 63 项，全部通过。新增故障注入覆盖精确容量边界、少 1 字节、整数溢出、请求/下载/最终复制空间不足时零 partial，以及 `Content-Length` 截断清理；Swift 6 编译与并发检查随 Release 构建通过。

边界：这是准入检查，不是空间预留。检查后其他进程或第二个媒体任务仍可能消耗容量，真实写入继续依赖 ENOSPC 错误和 partial 清理；尚未完成并发媒体任务的账户/项目配额或容量 lease。测试通过注入容量值验证同一决策和清理路径，没有真的填满用户磁盘，也不替代只读卷、跨卷、APFS purgeable 空间和 512 MiB Release trace。12.15 中记录的 SIGKILL 临时文件对账、MediaJobStore、提交不确定态、图片/音频文件化、断点续传及 Worker 文件授权仍未完成，因此 P0-E 仍不能标记完成。

### 12.17 第十七批实施记录：Story 视频首尾帧文件原生化（2026-10-22）

本批继续收敛 P0-E 的请求侧峰值。此前 Story 的上一段视频已经是文件引用，但首帧/尾帧仍先读取完整图片 `Data`，再生成 Base64 `String`，最后与 JSON `Data` 同时驻留。单段生成和持久化 Media Batch 现均只把受控项目图片 URL 交给 API 层。

已实施：

- `ImageGenerationInputImage` 增加进程内 `fileURL` 形态，继续保留原 Base64 初始化器与 Codable wire 兼容；本地文件 URL 不进入旧 JSON DTO。Story 单段视频和 Batch 的视频步骤使用文件形态，图片生成 multipart 的既有调用暂不改变。
- 原单视频占位写入器扩展为多媒体占位写入：可在同一个 64 MiB JSON 请求中按出现顺序替换首帧、尾帧和参考视频。每个源以 `O_NOFOLLOW` 打开并对 descriptor 执行 `fstat`，图片各限制 20 MiB、视频限制 47 MiB；所有源和 Base64 膨胀后的总请求在创建目标文件前统一计算并执行 12.16 的磁盘准入。
- 每个源仍以 48 KiB（三的倍数）分块编码，整个请求直接写入权限 `0600` 的临时文件，再由生产 URLSession `fromFile` 上传。取消、校验失败、总请求超限、provider 拒绝和成功路径继续统一清理；不会为两个帧图分别构造完整 Base64 字符串。
- 文件帧的像素范围与宽高比校验通过 `O_NOFOLLOW` 打开的描述符和 `/dev/fd` 交给 ImageIO，在已验证 descriptor 仍打开时取得属性；随后真正编码阶段再次以相同文件安全规则打开。旧 Base64 帧仍走原校验与协议，未被静默改写。

验证：Debug 与 arm64 Release 分别运行 Story 69 项、Media History/ViewModel 18 项、Media Service/HTTP transport 36 项、Core 视频用例与磁盘策略及 wire 兼容 8 项、文件 I/O 15 项，共 **146 项**，全部通过。新增端到端回归确认两个文件帧进入同一请求、data URL 字节与原文件完全一致、临时请求上传后删除；Story 回归确认捕获到的生产请求 `base64Data` 为空且文件内容正确；Core wire 回归确认文件 URL 不会进入旧 Codable DTO。Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：这一批仅消除 Story 视频首/尾帧的 Base64 峰值。通用 Media Studio 用户选择的图片和参考音频、Story 图片生成 multipart、provider 返回图片及旧兼容调用仍可能持有完整 Data/Base64；不能据此宣称所有图片/音频 DTO 已文件化。文件内容在属性校验与编码之间仍可能被同一账户下有写权限的外部进程替换，编码阶段会重新检查普通文件与大小，但尚未实现不可变 artifact hash/descriptor 跨阶段传递。MediaJobStore 的全局唯一事实源仍需先迁移 Story/Batch 现有 intent，不能旁路新增第四套 job 状态；SIGKILL 对账、真实内存 trace 和 Worker 仍未完成。

### 12.18 第十八批实施记录：通用视频图片与参考音频文件原生化（2026-10-22）

本批继续 P0-E，但不旁路创建 MediaJobStore。审计确认 Story attempt 与 Media Batch 已经是 durable provider 状态源，通用 Media Studio 仍只有内存 token；因此先在 6.4 冻结接管顺序和禁止双写规则，再推进不改变任务事实源的输入文件化。

已实施：

- 通用 Media Studio 选择视频首帧或历史图片时，不再长期持有完整图片 `Data` 与 Base64 字符串。新增每个 ViewModel 独占的临时 artifact 目录；在 security-scoped 访问仍有效时完成格式、像素和 20 MiB 上限校验，并以 `O_NOFOLLOW` 有界复制成权限 `0600` 的 App 控制文件。替换、移除、账户重置和 ViewModel 释放都会清理受管文件。
- `VideoGenerationInputAudio` 增加文件引用形态，旧 Base64 初始化器继续兼容。参考音频选择不再读取完整文件后执行 `base64EncodedString()`，而是复制为受管文件；模型只保存 URL、名称与 MIME。
- NewAPI 请求构造将文件图片、首尾帧、参考视频和参考音频统一放入同一个多源临时 JSON。音频沿用 20 MiB 源上限，所有媒体 Base64 膨胀后的合计仍受 64 MiB 请求上限和 12.16 磁盘预算约束；writer 在编码时重新执行 `O_NOFOLLOW`/`fstat`，成功、拒绝、取消和失败路径继续删除请求临时文件。
- 视频首帧预览改为从本地 artifact 异步下采样，使用现有有界缩略图缓存，不为预览重新生成 Base64 或全尺寸 `NSImage`。

验证：新增 App artifact 回归覆盖图片/音频私有复制、原文件删除后仍可用、无 Base64、`0600` 权限和显式清理；API 端到端回归覆盖视频与音频两个文件进入同一请求、data URL 字节一致及上传后临时请求删除。Debug 与 arm64 Release 相关 Core/App/API 分别运行 56 项，全部通过；Swift 6 编译与并发检查及 `git diff --check` 同步作为门禁。

边界：通用图片创作的最多 8 张参考图仍沿用 Base64 DTO，Story 图片生成 multipart 和 provider 图片结果也尚未文件化。本批 artifact 目录是短生命周期输入缓存，正常退出路径会清理，但进程被 SIGKILL 后仍需启动 reconciliation；不能把它当作 durable job 输入仓库。MediaJobStore、提交不确定态、重启恢复和 Worker 仍未实现，P0-E 不能标记完成。

### 12.19 第十九批实施记录：通用图片参考图与 multipart 文件原生化（2026-10-22）

本批关闭 12.18 明确保留的通用图片创作参考图峰值。此前本机图片和历史图片进入通用图片创作时，会读取完整 `Data`、转换为 Base64 字符串，再由 API 层同时解码最多 8 份图片并拼接完整 multipart `Data`；峰值随图片总大小线性放大且存在多份并存。

已实施：

- 通用图片参考图与视频输入共用 ViewModel 独占的受控 artifact 目录。本机图片在 security-scoped 访问期间完成校验和有界复制；历史中的本地文件直接流式复制，远端/Base64 兼容资产只在确实没有本地文件时才落一份受控副本。生产请求中的 `ImageGenerationInputImage` 只持有文件 URL，不再持有 Base64。
- 新增图片 multipart 文件 writer：最多 8 张、单张 20 MiB，总请求硬上限 161 MiB；创建目标前计算精确 Content-Length 并执行 12.16 磁盘准入。每个源通过 `O_NOFOLLOW` 与 `fstat` 验证为受限普通文件，按 256 KiB 分块复制到权限 `0600` 的临时 multipart，再由生产 URLSession `fromFile` 上传。
- 上传成功、provider 拒绝、取消和本地写失败都会删除临时 multipart；旧 Base64 请求继续走兼容内存路径，避免静默改变 Story 图片生成协议。本批没有把测试 transport 的内存 fallback 当作生产流式实现。
- 通用图片参考图缩略图改为从 artifact 异步有界下采样；移除单张、全部清除、账户重置与 ViewModel 释放会回收受管文件。生成期间 UI 与 ViewModel 均阻止删除正在上传的输入。

验证：Debug 与 arm64 Release 相关 Core/App/API 分别运行 54 项，全部通过。新增端到端回归确认两张文件参考图进入同一 multipart、字段数量和原始字节一致、上传完成后临时文件删除；App 回归确认选择后 `base64Data` 为空且文件 URL 有效。Swift 6 编译与并发检查及 `git diff --check` 同步作为门禁。

边界：Story 图片生成 multipart 仍沿用 Base64，provider 返回的图片结果仍可能是完整 Base64 响应；远端历史资产首次下载不可避免需要受响应上限约束的一份 `Data`，但不再二次编码为 Base64 输入。短生命周期 artifact 的 SIGKILL 启动清理、MediaJobStore、提交不确定态、真实多图峰值 trace 与 Worker 仍待完成，P0-E 仍不能标记完成。

### 12.20 第二十批实施记录：通用视频 MediaJobStore 第一阶段（2026-10-22）

本批按 6.4 冻结的迁移顺序，只接管通用 Media Studio 的新视频任务。原 `MediaStudioVideoGenerationState` 仅保留 UI 订阅/防止旧回调污染的内存 token，不再保存 provider 任务事实；Story `segment.attempt` 与 Media Batch `jobs` 没有双写到新 Store，仍等待后续原子迁移。

已实施：

- 新增账户隔离的 `MediaStudioVideoJobStore` 和进程内 `MediaStudioVideoJobService`。提交前原子保存本地 job ID、owner、session epoch、稳定 idempotency key、请求参数、输入 artifact 引用、provider job ID、retry 次数、状态版本、失败阶段、可重试性和更新时间；manifest 与 artifact 使用私有目录/`0600` 文件，凭据不进入记录。
- 首帧、尾帧、参考视频和参考音频在 intent 阶段复制到 durable job 目录，记录大小与 SHA-256；恢复创建前用 `O_NOFOLLOW`、普通文件/长度和摘要重新校验。provider job ID 首次出现时先单独提交这个关键事实，再回收已经不再需要的提交输入，避免清理与状态写失败同时丢失恢复身份。
- `VideoGenerationRequest` 增加稳定 `clientRequestID`，NewAPI 视频创建请求以 `Idempotency-Key` 发送同一值。provider 是否真正实现幂等不能由客户端假定：重启发现已越过提交边界但没有 provider ID 时，状态改为 `submission_uncertain`，UI 明确提示核对且恢复服务绝不调用 create；已有 provider ID 时只调用 `resumeVideo` 查询/下载。
- ViewModel 在历史分页初次加载完成后再恢复 job，避免恢复结果被稍晚返回的旧 history snapshot 覆盖。恢复、进度、finalize 和取消写入都携带 session epoch；新会话接管后，旧会话的迟到写入被拒绝。模型服务也绑定到创建时的认证 session，切换账户后不能继续使用新账号凭据推进旧任务。
- 通用视频 history finalize 使用本地 durable job ID 作为稳定 record ID。若视频与 `record.json` 已原子提交但 job 尚未来得及标为 completed，重启重试会验证并复用同一记录；没有 manifest 的 partial folder 才会被确定性清理重写，因此不会生成重复历史项。
- 损坏的已提交 manifest 不再被当作“没有任务”跳过；Store 会停止恢复和新自动动作。无 provider ID 的确定性前置失败不参与启动恢复；带 provider ID 的可重试失败最多自动恢复 3 次，之后保留记录等待人工处理。

验证：Debug 与 arm64 Release 选择性运行 History/Video Job/Video Canvas/Image Interaction/ViewModel/API 共 71 项，全部通过。新增覆盖 durable 输入复制与摘要篡改、损坏 manifest、稳定幂等头、提交不确定态禁止 create、provider ID 仅 resume、session epoch fencing、启动自动恢复，以及 history 已提交/job 未完成窗口的幂等 finalize；Swift 6 编译与并发检查及 `git diff --check` 同步作为门禁。

边界：当前 Store 只替换通用 Media Studio 视频的任务事实源，Story/Media Batch 的非终态任务尚未执行 6.4 的原子导入；图片任务仍没有进入 MediaJobStore。App 内由单个 service actor 写入并有 epoch fencing，但尚未增加两份 App/未来 Worker 之间的 OS 文件锁与交接协议。`Idempotency-Key` 已发送但没有真实 provider 沙箱对账证据；没有 provider ID 的不确定提交仍需人工核对。取消没有 provider cancel/退款契约，只能保留 `cancellation_requested` 后继续查询最终事实。terminal job 的保留/清理策略、真实逐状态强杀矩阵、睡眠/断网恢复、下载断点续传、Worker 文件授权与独立 Media Worker 仍待完成，因此 P0-E 仍不能标记完成。

### 12.21 第二十一批实施记录：MediaJobStore 跨进程操作锁与 epoch fencing（2026-10-22）

本批补齐 12.20 明确保留的跨 Store/跨进程写竞态。单个 Swift actor 只能串行化自身调用，两份 App 或未来 App/Worker 若同时打开同一 root，仍可在 `load → epoch 校验 → transition → save` 之间覆盖对方。

已实施：

- Store root 下新增持久 `.writer.lock`，以 `O_NOFOLLOW | O_CLOEXEC` 打开，校验为当前用户拥有、单硬链接的普通文件，并强制权限 `0600`；symlink、非普通文件或无法安全打开时拒绝继续操作。锁文件不通过 unlink “解锁”，崩溃/强杀后由内核随 descriptor 释放 `flock`。
- `prepare`、`markSubmitting`、进度/终态/失败/取消写入、`recover` 和 `snapshot` 都在一次独占操作锁内完成读取、epoch 校验、状态转换与原子 manifest 写入。两个独立 Store 竞争同一 prepared job 时只有一个可以进入 submitting，另一个看到已提交的状态后失败，不再基于旧 snapshot 重复创建。
- 并发恢复会在锁内串行接管 epoch；持久记录只保留最后一个 epoch，前一个恢复会话的迟到进度写入会被 `staleSession` 拒绝。这一 fencing 跨越不同 Store actor，不再只是单进程内的约定。

验证：新增锁文件类型/权限、symlink 拒绝、descriptor 释放后等待方继续、双 Store 并发 submitting 单成功，以及双 Store 并发 recovery 后旧 epoch 被 fencing 的回归。Debug 与 arm64 Release 按 History/Video Job/Video Canvas/Image Interaction/ViewModel/API 各选择性运行 76 项，全部通过；Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：这是单次 Store 操作锁，不是跨进程的长期 lease、heartbeat 或 App → Worker 升级交接协议；两份同时存活的客户端可以依次接管 epoch，只有最后接管者能提交后续状态，但本批未阻止前一个进程已发起的 provider 查询/下载。为先保证正确性，`prepare` 最多 47 MiB 的 artifact 复制与 SHA-256 仍在锁内；Story/Batch 迁移前应改为锁外私有 staging、短锁内 rename + manifest commit，并补齐长期所有者/交接协议。Story/Media Batch 仍未迁移，图片 job、终态清理和真实多进程强杀矩阵仍待完成，P0-E 仍不能标记完成。

### 12.22 第二十二批实施记录：缩短 MediaJobStore prepare 写锁临界区（2026-10-22）

本批收敛 12.21 为正确性暂时保留的大文件持锁问题。原实现会在全局 `.writer.lock` 内复制并两次计算最多 47 MiB 参考视频的 SHA-256，使其他账号和其他 job 的 snapshot/进度/取消写入也被大文件 I/O 阻塞。

已实施：

- `prepare` 先在随机 job ID 的私有目录内复制输入、校验长度并计算 SHA-256，此时不存在 `job.json`；并发 `recover` 依旧只能看到已提交 manifest 的 job，不会把正在 staging 的输入当成可执行任务。
- 仅在最终 `job.json` 原子写入/替换和返回已发布文件 URL 时获取跨进程写锁。新鲜 staging 输入已在同一次 prepare 中验证摘要，发布时不再重复扫描整个大文件；重启 `recover` 仍会使用 `O_NOFOLLOW`、普通文件/长度和 SHA-256 全量复验，不降低持久输入的恢复门禁。
- manifest 写入或发布后的本地组装失败时，在释放写锁前删除该未完成 job 目录，避免其他进程在错误清理窗口内读到半提交状态。

验证：新增回归在人工持有 `.writer.lock` 时启动含 2 MiB 参考视频的 `prepare`；artifact 可在持锁期间完成 staging，但第二份 `job.json` 不会提前可见；释放锁后任务才发布，持久文件字节与源文件一致。Debug 与 arm64 Release 按 History/Video Job/Video Canvas/Image Interaction/ViewModel/API 各选择性运行 77 项，全部通过；Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：本批缩短的是 Store 全局互斥时间，不会减少单个输入必需的一次复制与摘要计算；这些 I/O 仍在 App 进程内，尚未迁往 Media Worker。进程在 manifest 提交前被 `SIGKILL` 仍可留下无 manifest 的私有 job 目录；它不会被恢复或提交，但启动对账/安全清理策略仍待实现。长期 lease/交接协议、Story/Media Batch 迁移和真实多进程强杀矩阵仍未完成，P0-E 仍不能标记完成。

### 12.23 第二十三批实施记录：MediaJobStore workload 分区与兼容读取（2026-10-22）

本批在迁移 Story/Batch 前先固化业务关联和恢复路由，不改变任何 Story 写入者。审计确认：当前新建的批量视频已通过 `directVideos` 逐段执行，任务事实仍在 `segment.attempt`；持久 `StoryMediaBatch.jobs` 的视频执行路径主要承担旧批次恢复兼容。因此后续应先迁移新 Story 单段/直连队列，再对旧 Batch 做可重复导入，不能把两者当成同一种存储结构直接覆盖。

已实施：

- MediaJobStore manifest 新增受校验的 workload 关联：`generic_studio`、`story_video(projectID, segmentID, attemptID)` 和 `story_batch_video(projectID, batchID, segmentID, attemptID)`。关联不包含凭据，Story/Batch 的 attempt/intent ID 作为未来原子切换与重复导入的稳定业务键。
- 新任务在 prepare 时持久 workload，snapshot 显式返回该关联；非法的字段组合、空 segment ID 或超界 segment ID 被拒绝，避免恢复路径猜测任务归属。
- 通用 Media Studio 的既有 `recover(owner, epoch)` 只筛选 `generic_studio`，不能消费或推进 Story/Batch 分区中的 job；新增 workload 精确恢复入口，未来 Story 只能用完整业务关联取回自己的 job。
- 新 manifest 升为 format-v2，避免旧客户端忽略未知 workload 字段后把 Story job 误当通用 Studio job 执行；旧客户端会明确拒绝 v2。12.20–12.22 已写入的 format-v1 manifest 必须缺少 workload，新客户端读取时只将其解释为 `generic_studio`，无需就地重写，也不会被误归类为 Story 任务。v1 却携带 workload、或 v2 缺少 workload 都按损坏记录拒绝。

验证：新增 Story workload 对默认恢复不可见、只能被精确 workload 恢复，以及删除 workload 字段的旧 manifest 仍可按通用 Studio job 恢复的回归。Debug 与 arm64 Release 按 History/Video Job/Video Canvas/Image Interaction/ViewModel/API 各选择性运行 79 项，全部通过；Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：本批只建立 schema 与恢复隔离，Story `segment.attempt` 和旧 Batch `jobs` 仍是各自的唯一写入者，没有向 MediaJobStore 旁路双写。下一阶段还必须提供可重复的稳定本地 job ID、项目 manifest 与 job manifest 之间的可恢复切换协议，以及旧 provider ID 的只读导入；在这些条件完成前，Story/Batch 不能改由新 Store 推进，P0-E 仍不能标记完成。

### 12.24 第二十四批实施记录：稳定 job ID、幂等发布与所有权切换协议（2026-10-22）

本批只完成迁移协议的 MediaJobStore 一侧，仍不修改 Story/Batch 写入者。直接给原 `prepare` 增加固定 UUID 并不安全：输入复制发生在 writer lock 外，同一 ID 的两个进程可能互相覆盖最终目录。因此发布模型改为每次先在账户目录创建不可恢复的隐藏 staging，完成受限复制、SHA-256 和临时 manifest 后，再在 writer lock 内校验稳定 ID 冲突并通过同卷目录 rename 一次发布。

已实施：

- Story/Batch 的稳定本地 job ID 强制等于现有 attempt UUID；`stagePrepared` 对同 ID、同 workload、同请求内容可重复执行并返回同一记录，prompt、模型、参数或 artifact 摘要不同则以 conflict 拒绝，不能覆盖第一次已持久化的输入。通用 Studio 仍生成随机 ID，但也改走隐藏 staging 发布，继续保持大文件复制在全局锁外。
- format-v3 新增 `staged/active` 所有权状态。staged job 已持久但不参与任何 recovery，所有状态推进 API 也拒绝它；只有业务 manifest 已原子改为引用该 local job ID 后，协调者才可调用幂等 `activate`。因此正确切换顺序固定为“stage job → 原子提交 Story/Batch 业务关联并停止旧状态推进 → activate”；若第二步后崩溃，业务关联可在重启时重复 activate，若第二步前崩溃，旧写入者仍是唯一所有者。
- 旧任务导入只接受非空 provider job ID，且不复制 create 输入；激活后唯一执行模式是 resume。没有 provider ID 的旧 attempt 不能证明远端未接单，继续留在旧事实源或进入人工对账，禁止通过导入重新 create。
- 同稳定 ID 的并发 staging 在锁内比较不可变 workload、idempotency key、provider ID、请求字段与 artifact 摘要；完全一致才视为成功重放。隐藏 staging 即使被 `SIGKILL` 留下也不会被扫描或提交。v1/v2 manifest 没有 activation 字段，按当时语义只读为 active；新 v3 对旧客户端仍以未知版本明确拒绝，避免旧版本绕过 staged 隔离。

验证：新增稳定 staging 重放、冲突输入不覆盖、staged recovery/状态推进隔离、激活恢复、旧 provider ID 仅 resume、重复激活，以及 v1/v2 兼容读取回归。Debug 与 arm64 Release 按 History/Video Job/Video Canvas/Image Interaction/ViewModel/API 各选择性运行 82 项，全部通过；Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：本批尚未把 `localJobID` 写入 Story project schema，也未接入单段适配器或启动扫描器，因此不会形成双写。下一批必须先实现 Story manifest 的关联字段、上述切换顺序和崩溃恢复，再让新 Story attempt 使用 MediaJobStore；旧 Batch 仍需单独分页导入。隐藏 staging 的启动限龄清理、长期 lease/App→Worker 交接、真实多进程逐状态强杀和最终 job 保留策略仍未完成，P0-E 不能标记完成。

### 12.25 第二十五批实施记录：Story manifest 原子所有权切换（2026-10-22）

本批完成迁移协议的 Story project 一侧，但尚不让生成适配器调用该协议。兼容性审查确认：若只给 v2 segment 增加可选 local job ID，旧客户端会忽略未知字段，并因 legacy attempt 已清除而把同一段误判成“可生成”，存在重复计费风险。因此项目只在实际 handoff 时从 schema-v2 升为 v3；当前客户端同时读取 v2/v3，旧客户端会明确拒绝 v3。

已实施：

- `StorySegment.videoJobRecordID` 保存稳定的 MediaJobStore job ID，并与 legacy `attempt` 强制互斥；`hasVideoJob`、项目未完成任务判断、生成/图片/编辑/Agent/Batch 准入和 UI 锁定同时识别两种所有权。迁移后的段落不会因为 `attempt == nil` 被错误开放编辑、重绘或再次提交。
- `handoffVideoGenerationToMediaJob` 从最新 project manifest 校验完整 expected attempt，在一次原子项目保存中升级 v3、写入与 attempt UUID 相同的 job record ID、清除 legacy attempt。完全相同的崩溃重试不更新时间；attempt、segment 或 job ID 不匹配均拒绝切换。
- handoff 一旦提交，旧 `updateVideoGenerationProgress` 因 attempt 已清除而被 fencing，不能继续写 provider ID/status。MediaJobStore 仍须在项目提交之后显式 activate，符合 12.24 的“stage → project handoff → activate”顺序。
- 普通项目编辑现在从最新 manifest 自动合并所有 durable `videoJobRecordID` 和 schema 版本，即使调用方没有进程内 active-task token，也不能用旧 v2 UI snapshot 擦除已交接任务；删除仍拥有 durable job 的 segment 会被拒绝。完成视频重新编辑时会同时清除该关联，为下一次 attempt 生成新稳定 ID。

验证：新增 handoff 升级 v3、清除 legacy writer、幂等重试、普通旧快照编辑自动保留关联、legacy progress fencing 和 v2 不得携带新关联的回归。Debug 与 arm64 Release 选择性运行 Story/Core/MediaJobStore 共 195 项，全部通过；Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：本批仅提供 project schema 与事务边界，尚未在 `StoryVideoGenerationAdapter` 接入 stage/handoff/activate，也没有启动时按 `videoJobRecordID` 恢复任务，因此正常产品路径还不会写 v3 关联。下一批应迁移新 Story 单段/直连视频并补 stage 后、project handoff 后、activate 后三个崩溃点的端到端测试；旧 Batch provider job 导入继续保持独立阶段。Worker、长期 lease、staging 限龄清理和真实强杀矩阵仍未完成，P0-E 不能标记完成。

### 12.26 第二十六批实施记录：Story 单段视频接入 durable job 生命周期（2026-10-22）

本批让新 Story 单段/直连视频首次实际执行 12.24–12.25 冻结的所有权协议。原适配器会把 provider job ID/status、业务结果和错误全部写回 `segment.attempt`；现在 handoff 之后只有 MediaJobStore 推进 provider 状态，Story project 只保留稳定关联、最终视频和业务错误。

已实施：

- 新任务固定执行 `begin legacy intent → 组装完整请求 → stagePrepared(attempt UUID) → Story manifest handoff → activate → markSubmitting → provider create`。provider progress 只写 MediaJobStore，百分比仍通过轻量事件实时显示；相同 status/job ID 的轮询由 gate 合并，不再为百分比风暴重复改写 manifest。
- 旧 Story attempt 仅在已经持有非空 provider job ID 时可导入；导入记录不保存 create 输入，handoff/activate 后只调用 `resumeVideo`，不会回退到 create。顶层 Media Studio 与 Story composition 显式共享同一个 MediaJobStore；隔离测试和独立 Story composition 则把 job root 放在对应 Story root 下，不接触真实用户目录。
- 账号 session UUID 现在穿过 Core use case 和 adapter，stage/activate/progress/finalize/failure/cancel 都携带同一 epoch；Store 抢占后的旧 writer 会收到 `staleSession`，适配器在失败写 Story 业务字段前也重新执行 continuation fencing。
- provider 返回后先 `markFinalizing`，再按 `videoJobRecordID` 幂等保存 Story 视频，最后 `markCompleted`。若崩溃发生在 Story 保存之后、job completed 之前，同一 provider result 会复用已提交的 `StoryVideo` 和文件；actor 在异步文件复制后重新读取最新 manifest，并删除竞态中落败的候选文件，避免 finalize 重放产生重复 mp4。
- handoff 后的错误只更新 durable job 状态和 Story 业务错误，不重建 legacy attempt。provider identity 第一次持久化后不可被后续 progress/result 改成另一个 ID，空 ID 和冲突 ID 都拒绝进入 finalizing/completed。
- Core 事件/终态身份校验同时接受迁移前 attempt UUID 和迁移后的 `videoJobRecordID`，但完成、失败和提交前失败仍分别校验视频/错误及所有权字段组合，外来 segment/job 结果不能通过。

验证：新增新 Story create 只执行一次且最终 durable record 为 completed、旧 provider task 导入只 resume 不 create、Story finalize 重放不重复文件、provider identity 不可改绑，以及 v3 事件/结果身份过滤回归；现有顺序队列、页面离开、参考视频、重新生成、preflight 失败和输入副本回收测试同步适配 durable 生命周期。Debug 与 arm64 Release 选择性运行 Story/Core/MediaJobStore 各 95 项，全部通过；Swift 6 编译与并发检查、`git diff --check` 同步作为门禁。

边界：本批只迁移当前进程内主动发起的 Story 单段/直连任务，尚未在项目加载后扫描 `videoJobRecordID` 并执行 staged activate、prepared create、已有 provider ID resume 或 submission-uncertain 人工核对。因此 handoff 后进程退出的任务必须由下一批启动恢复协调器接管，本批不能宣称 Story 恢复闭环完成。旧持久 Batch 的分页导入、明确的 durable failed-job 人工释放/重试入口、staging 限龄清理、长期 lease/App→Worker 交接、真实强杀矩阵和 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.27 第二十七批实施记录：Story 项目加载恢复协调器（2026-10-22）

本批补齐 12.26 明确保留的进程退出恢复窗口。恢复以已加载的 Story project manifest 为路由事实：只有 segment 中的完整 `videoJobRecordID` 能构造精确 `story_video(projectID, segmentID, attemptID)` workload，不能枚举 MediaJobStore 后猜测归属或消费其他业务分区。

已实施：

- Story 项目首页和后续分页加载完成后，按页面/segment 顺序把 durable video 关联加入单一恢复队列；同一 target 已在主动生成、已排队或正在恢复时不会重复入队。恢复任务计入现有视频活动状态，但不占用全局 Story 编辑 busy，离开项目页后仍可继续完成。
- staged job 说明业务 manifest 已经 handoff、但 activate 尚未落盘：加载后幂等 activate；prepared 才允许进入 create。active job 统一经 workload 精确 `recover`：已有 provider ID 只 resume，`submitting` 且无 provider ID 会先持久化为 `submission_uncertain` 并返回人工核对提示，绝不重新 create；failed/cancelled/completed 但缺少 Story 结果等不能自动证明安全的组合也只提示核对。
- Story 视频已提交而 MediaJobStore 仍处于 finalizing 的崩溃窗口不再请求 provider：恢复器校验两边 provider job ID 一致后，只补 `markCompleted`。若两边 ID 不一致则保留文件和记录并要求人工核对；已经双边 completed 的任务为只读成功，不在每次项目加载时重写 epoch/manifest。
- 恢复沿用 12.26 的幂等 Story finalize、progress gate、provider identity fencing 和 session epoch。ViewModel 切换账号/reset 会取消队列并清空 pending target；所有事件和终态同时校验 owner、session、project、segment 与 record ID，旧账号恢复即使稍后返回也不能发布到新账号 UI 或继续写被新 epoch 接管的 job。
- Core application boundary 新增显式 recovery outcome：`finished` 仍须满足 completed/failed 的业务字段组合，`needsReview` 必须保留同一个 durable record 且带非空说明；外来 project/segment/record 的进度和终态继续被过滤。

验证：新增 handoff 后未 activate 的 staged create 恢复、active provider job 只 resume、无 provider ID 的 uncertain submission 禁止 create、Story 已保存时本地补 completed、恢复中切账号丢弃迟到结果，以及 Core durable record 身份过滤回归。Debug 选择性运行 MediaJobStore/Story/Core 共 XCTest 100 项与 Swift Testing 6 项，全部通过；arm64 Release 同矩阵、Swift 6 编译与 `git diff --check` 作为本批提交门禁。

边界：恢复在 Story workspace 实际加载项目页时启动，不把尚未分页读取的 project 全量扫入内存，也不是 App 登录主路径的无界磁盘扫描。当前两份同时存活的 App 仍可能依次接管 epoch 并并发查询同一 provider job；不会重复 create，但长期 lease/heartbeat 与 App→Worker 独占交接尚未实现。明确的 failed/uncertain 人工释放或核对后重试 UI、旧持久 Batch 分页导入、隐藏 staging 限龄清理、终态 job 保留清理、真实强杀/睡眠/断网矩阵和 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.28 第二十八批实施记录：Story durable 任务核对后安全释放（2026-10-22）

本批补齐 failed、`submission_uncertain` 及其他只能人工核对的 Story durable video job 释放闭环。用户确认只表示放弃旧关联并允许后续手动重新生成，不会在确认动作中调用 provider create/resume；警告文案继续明确潜在重复扣费风险。

已实施：

- Core `StoryVideoGenerating`/use case 新增显式 verified-release application boundary，冻结 owner、account session、project、segment 与 `videoJobRecordID`；返回项目必须清除同一 durable 关联且仍无 attempt/video，外来或错配结果失败关闭。ViewModel 不直接访问具体 Store。
- Adapter 以完整 `story_video(projectID, segmentID, attemptID)` workload 校验 durable record；staged job 先幂等 activate，active/terminal record 接管到当前 session epoch。MediaJobStore 随后把记录写成 `cancelled` 终态，保存 `verified_retry_release` 审计原因、原 provider ID 和 manifest，并回收尚存提交输入；重复释放为只读幂等。
- 提交顺序固定为“job 终态先落盘，Story 关联后清除”。若在两次提交之间崩溃，下一次用户确认会识别已终态记录并只补 Story 清除；不同 workload/record ID 不能清除业务关联。Story 清除本身允许 nil 关联的幂等重放，但拒绝新 attempt、已保存 video 或另一个 record。
- `cancelled` 之后迟到的 finalizing/completed 写入被拒绝，避免旧 provider callback 重新打开用户已明确释放的任务。账号切换/reset 会取消释放 task；continuation 与最终发布都再次校验 owner/session/record，旧账号迟到结果不能进入新账号项目。
- durable record 的非运行 UI 现在复用“核对后允许重试”确认框，释放期间显示独立状态；旧 legacy attempt 的归档逻辑保持不变。两条路径都只解锁 segment，不立即发起生成。

验证：覆盖 staged/failed/`submission_uncertain` 释放、provider ID 审计保留、workload 错配拒绝、重复释放、释放后迟到 completed fencing、终态已写但 Story 未清的崩溃重放、点击确认不调用 provider、Core 输出身份过滤及切换账号丢弃迟到结果。Debug 与 arm64 Release 选择性运行 MediaJobStore/Story/Core，各 105 项 XCTest 与 7 项 Swift Testing 全部通过；Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：这是人工解锁闭环，不是 provider cancel/退款协议；用户确认前仍不会自动重发不确定任务，确认后也无法证明服务商没有继续执行旧任务。durable terminal job 的长期保留/配额清理、旧持久 Batch 分页导入、隐藏 staging 限龄清理、长期 lease/App→Worker 独占交接、真实逐状态强杀/睡眠/断网矩阵与 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.29 第二十九批实施记录：Story Media Batch durable handoff 协议（2026-10-22）

本批先建立旧持久 Batch 视频任务切换唯一写入者所需的 schema 与可恢复提交协议，尚不让生产 Batch session 同时写旧字段和 MediaJobStore。审计确认旧路径会把 provider ID/status 同时推进到 `StoryMediaBatch.jobs` 与 `segment.attempt`；如果在没有 handoff 的情况下直接旁路写 Store，会违反 6.4 的单一事实源约束。

已实施：

- `StoryMediaBatch.Job` 新增可选 `videoJobRecordID`，旧 JSON 缺失字段继续解码为 nil。非空关联必须等于稳定 `intentID`、只属于对应 videos step，并与 Batch draft 中同 segment 的 `videoJobRecordID` 精确一致；关联后 legacy attempt 必须为空，损坏或交叉绑定的 batch 拒绝加载。
- StoryProjectStore 新增 Batch 视频 handoff：校验 owner/project/batch/step/intent、旧 job 与 attempt 的 provider ID 一致后，在 Batch draft 内以 `intentID` 替换 legacy attempt，并把 project schema 升到 v3。相同 handoff 可重复执行，不同 intent 不能接管。
- 两文件顺序继续采用 Batch manifest write-ahead、canonical Story project second。handoff 前把旧 draft digest 固化为 expected digest；若进程在 Batch 文件提交后、Story 文件提交前退出，重放会接受精确旧 canonical 状态并补第二次写入，而不会恢复旧 attempt writer。
- MediaJobStore 新增 `stageSubmissionUncertain`，用于导入没有 provider ID、但不能证明 POST 未发生的旧 Batch intent。该记录不复制 create 输入、激活后只能返回 `needsReview`，重复 stage 幂等，绝不会被恢复成 create；已有 provider ID 继续使用只-resume 的 imported-provider 路径。

验证：覆盖 Batch workload 的无 provider ID 不确定态导入、无输入副本、激活后只人工核对、legacy provider identity 保留、Batch/draft/canonical 三方关联切换、重复 handoff、外来 intent 拒绝，以及 Batch-first 崩溃窗口重放修复。Debug 与 arm64 Release 选择性运行 MediaJobStore/Story，各 108 项 XCTest 全部通过；Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批只交付下一阶段接管所需的持久格式与 handoff 原语，生产 `StoryMediaBatchSession` 仍是 legacy 唯一 writer，没有旁路双写，也不会扫描或改写用户现有 batch。下一批必须把 account session epoch、stage/import → Batch handoff → activate、progress/finalize/recovery 全部在同一切换中接入后，才能停止写 `batch.jobs.jobID` 与 `segment.attempt`。分页迁移协调器、终态清理、staging 限龄、长期 lease、真实强杀矩阵与 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.30 第三十批实施记录：生产 Story Media Batch 切换 durable 单写（2026-10-22）

本批完成 12.29 预备协议的生产接管。`StoryMediaBatchUseCase`、macOS adapter、ViewModel 与 session 现在贯穿同一个 account session epoch；Story composition 与直接视频队列共享同一 `MediaStudioVideoJobStore`，不再由 Batch session 自行维护另一套 provider 状态机。

已实施：

- 新 Batch 视频严格执行 `stagePrepared → 临时 intent checkpoint → Batch handoff → activate → markSubmitting`，provider POST 只能发生在 handoff 与激活之后；输入素材先复制并校验到 durable record，账号切换后的旧 epoch 无法继续写 progress/finalize/completed。
- 生产 progress callback 只写 MediaJobStore。`batch.jobs.jobID` 与 `segment.attempt.jobID/status` 不再随轮询推进，重复 provider phase 在 Store 前合并；UI 进度通过非持久 event projection 发布，不把瞬态百分比重新写回 Story/Batch manifest。
- 旧 Batch 有 provider ID 时导入为 resume-only；没有 provider ID 时导入 `submission_uncertain` 并只进入人工核对，绝不 create。若进程死在 durable stage 已提交、Batch handoff 尚未提交之间，恢复会先核对原 staged request/workload，再沿用 prepared 记录 create 一次，不会误降级或覆盖成另一生命周期。
- 成片结果采用 `MediaJobStore.finalizing → Batch/Story write-ahead commit → MediaJobStore.completed`。恢复会在不调用 provider 的情况下对账“成片已提交、job 仍 finalizing”的窗口；尾帧提取继续是成片终态后的可失败增强。
- “核对后结束批次”先把每个未完成 durable 视频记录写成带 `verified_retry_release` 的 `cancelled` 审计终态，再清除 Batch 与 Story 关联。两步可重放，不能因中途崩溃重新开放旧任务。

验证：覆盖新任务 durable-only progress 与重复 phase 合并、legacy provider resume-only、无 provider identity review-only、prepared-before-handoff 崩溃重放、finalizing-after-Story-commit 对账，以及 verified abandon 的 terminal-first 顺序；同时保留原有 Story 账号隔离、直接视频恢复和 Batch write-ahead 回归。Debug 与 arm64 Release 选择性运行 MediaJobStore/Story/Core，各 113 项 XCTest 与 5 项 Swift Testing 全部通过；Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批迁移的是用户显式启动/恢复的生产 Batch session，不会启动应用、扫描或批量改写静态历史。全量历史 Batch 分页迁移协调器、durable terminal job 配额/清理、隐藏 staging 限龄、App→Worker 长期 lease 交接、真实逐状态强杀/睡眠/断网矩阵与 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.31 第三十一批实施记录：durable video 隐藏 staging 有界回收（2026-10-22）

本批收口 MediaJobStore 在大输入复制或 manifest 发布前异常退出留下的隐藏 staging。回收只处理新版显式 lease-aware 目录，不根据“目录够旧”猜测另一个 App writer 已死亡，也不把终态 job 与临时目录混为同一保留策略。

已实施：

- 每个新 `.staging-<UUID>` 在复制输入前创建权限为 `0600` 的 `.staging.lease`，校验 regular file、单硬链接与当前 euid，并从 staging 创建一直持有 `flock(LOCK_EX)` 到 publish/discard 完成。进程退出由内核释放锁；清理方只用非阻塞独占锁判断当前是否无人持有。
- owner 首次 prepare/stage/recover 时执行一次 best-effort 维护；大输入仍先在 writer lock 外复制并取得 lease，维护不会把文件复制重新串行化到全局写锁之前。清理按 account owner 目录隔离，只接受严格 UUID 名称、真实目录、非 symlink、超过 24 小时且存在合格 lease 的候选。
- 每次最多检查 64 个 staging 候选。活跃 lease、新鲜目录、旧版本无 lease 目录、畸形名称和 symlink 全部保留；无法安全确认所有权时失败关闭。本次维护失败不阻断视频提交，且同一 Store 后续操作仍可重试。
- publish 在原子目录 rename 前移除隐藏 lease 文件，descriptor 保持打开到 publish/discard 收尾；正式 job 目录不携带 staging 标记。候选删除后立即 unlock/close，不把最多 64 个文件描述符积压到整轮枚举结束。

验证：新增过期 lease-aware staging 回收、新鲜目录保留、跨 descriptor 活跃 lease 保护及释放后回收、旧版无 lease/畸形名称/symlink 保留、正式发布不含 `.staging.lease`、每轮候选上限，以及“writer lock 被占用时仍先完成大输入 staging”的性能回归。Debug 与 arm64 Release 选择性运行 MediaJobStore 测试，Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批不删除正式 durable job。终态记录仍可能被 Story/Batch manifest 引用，在交付引用对账、owner 字节/数量配额和可重放保留协议前，不能仅凭年龄删除。旧版本遗留的无 lease staging 也不会被自动猜测为孤儿；需要后续显式迁移/核对工具处理。全量历史 Batch 分页迁移、终态 job 配额/清理、App→Worker 长期 lease 交接、真实逐状态强杀/睡眠/断网矩阵与 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.32 第三十二批实施记录：durable video 跨 App 执行 lease（2026-10-22）

本批关闭 12.21 epoch fencing 仍允许的并发执行窗口。单次 writer lock 只能串行化 manifest 读写；两份同时存活的 App 此前仍可依次 recover 同一个 job、后者覆盖 session epoch，而前者已经在查询或下载同一个 provider 任务。现在 runnable execution 额外持有 job 级内核 lease，竞争者在改写 epoch 前失败关闭。

已实施：

- 每个 job 使用持久 `.execution.lock`，按 `O_NOFOLLOW | O_CLOEXEC` 打开，校验当前 euid、regular file、单硬链接并强制 `0600`。锁文件不 unlink；`Execution` 持有非阻塞 `flock(LOCK_EX)` descriptor，整个 create/resume、progress、finalize 和失败收尾期间保持存活，正常释放或进程退出时由内核解锁。
- 通用 Studio 的 lease 在 `markSubmitting` 与状态提交的同一个 writer-lock 临界区取得；纯 `prepared` intent 不长期占锁。Story/Batch 在 `activate` 时取得，恢复路径在更新 session epoch/retry 计数之前取得。竞争进程收到明确 busy，不会先抢 epoch 再让原执行者的 progress/finalize 变成 stale。
- `markSubmitting` 新增 execution-preserving 路径，状态从 prepared 切到 submitting 时转移同一个 lease，不因替换 `Execution` 值提前 close。生产通用 Studio、Story 单段/直连与持久 Batch 全部使用该路径。
- recovery 先验证活跃 lease，再把“submitting 且无 provider ID”归入 `submission_uncertain`；仍存活的 POST 不会被第二份 App 误判为崩溃残留。另修正 prepared recovery 只在内存替换 session epoch、未保存便返回的问题，接管 epoch 现在先 durable 提交，随后 submitting writer 才能通过 fencing。

验证：覆盖两个独立 Store 同时 recover 只能有一个 execution owner、失败方不能改写胜方 epoch、lease 穿过 submitting 转换、释放后可恢复、prepared 接管 epoch 可继续提交，以及 execution lock 类型/权限与 symlink 拒绝；原有 staging、provider resume、submission-uncertain、Story workload 和 ViewModel 恢复回归继续通过。Debug 与 arm64 Release 选择性运行 MediaJobStore/Story 测试，Swift 6 编译与 `git diff --check` 同步作为本批门禁。

边界：这是同一文件系统上协作式 App/App 互斥，不是 provider cancel、heartbeat、超时抢占或“lease 过期即证明旧副作用结束”。不遵守锁协议的旧客户端仍无法被 advisory lock 强制隔离；升级前仍需停止旧版本。App→Worker 必须显式完成 lease 所有权交接，不能先释放再靠 TTL 猜测；真实双 App `SIGKILL`、睡眠/断网及 Worker 组合矩阵仍待执行。全量历史 Batch 分页迁移、终态 job 引用对账/配额清理和 Media Worker 也仍待完成，P0-E 不能标记完成。

### 12.33 第三十三批实施记录：旧 Story Batch 分页迁移协调器（2026-10-22）

本批补齐 12.30 保留的静态历史迁移缺口。迁移绑定到用户实际读取的每一页 Batch 历史，不在登录或启动路径全盘扫描项目目录；首屏和“加载更早制作记录”使用同一协调器，因此落在第二页及更早页的未完成旧视频任务也能逐页切换到 MediaJobStore 单一事实源。

已实施：

- Story history application boundary 贯穿当前 account session epoch，默认 macOS adapter 共享生产 `MediaStudioVideoJobStore`。每页最多沿用既有 1～200 条上限，逐 Batch、逐 video step 执行 `stage/import → Batch/Story handoff → activate`，并在每个提交边界检查取消；切换账号后的旧分页任务不能用新 session 继续迁移。
- 已有 provider ID 的 legacy job 只导入为 resume-only；没有 provider identity 的旧 intent 只导入 `submission_uncertain`，协调器不调用 provider，也不会因打开历史页重新 create。新版进程若死在输入已复制的 `prepared` 与 Batch handoff 之间，迁移会保留原输入副本和 create mode，而不会误降级成不确定提交。
- 已经完成 handoff 的 staged record 会补 activate；active record 只校验 workload，不抢写 epoch。激活只发布 durable 可恢复所有权，返回的 execution lease 随即释放，真正的 provider 查询/提交仍只发生在用户显式恢复 Batch 时。
- 单条迁移冲突不会让整页消失：原 Batch 文件保留，页面返回独立 migration issue 计数并提示用户；多 step 在后续步骤失败时会重新加载已提交的最新 manifest，避免把部分成功迁移显示成旧快照。迁移后的 canonical project 同页返回并更新 ViewModel，UI 不继续持有 legacy attempt 假象。
- 分页 cursor 仍来自迁移前选出的稳定文件页；迁移导致当前页文件 mtime 更新不会触发无界重扫，也不会跳过 cursor 之后的更早候选。finished Batch 不创建无意义的 durable job，终态长期保留仍交给后续引用对账/配额策略。

验证：覆盖旧 provider task 只在其历史页真正加载时导入、第二页 cursor 继续可达、无 provider ID 进入 review-only、prepared pre-handoff 崩溃保留输入/create mode、workload 冲突保留原 Batch 并计数，以及 Core 对迁移后 project 的身份校验。Debug 选择性运行 Story 91 项 XCTest 与 Story history 4 项 Swift Testing 全部通过；arm64 Release 同矩阵、Swift 6 编译与 `git diff --check` 作为本批提交门禁。

边界：分页读取只完成 durable ownership 迁移，不自动恢复 provider 工作，也不迁移已完成历史任务。durable terminal job 的 Story/Batch 引用对账、owner 数量/字节配额与安全清理、App→Worker lease 显式交接、真实双 App/`SIGKILL`/睡眠/断网矩阵及 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.34 第三十四批实施记录：durable video 终态引用对账与有界保留（2026-10-23）

本批补齐 12.31 明确禁止提前实现的正式 durable job 回收。清理不扫描全部历史或全部 Story 项目，而是使用 workload 内已经持久化的精确 owner/project/segment/batch 标识定位业务 manifest；只有 completed/cancelled、超过 owner 预算、达到最短保留期、确认没有业务引用且未被执行 lease 占用的记录才可进入删除。

已实施：

- 每个 owner 默认最多保留 512 条终态记录和 16 MiB 终态 manifest/lease 字节，最短保留 30 天，每轮删除（包括上轮 tombstone 续删）最多 64 条。数量或字节任一超限即按 `updatedAt + UUID` 最老优先回收；未超限不因年龄单独删除，failed、submission-uncertain、prepared/running 等非完成审计态不在候选集合。
- Generic Studio 只核对同 UUID 的 history folder/`record.json` 与单一视频资源；Story 只核对 workload 指向的 project/segment，并在 Batch workload 下额外核对精确 batch/segment job，覆盖“Batch 已提交 handoff、canonical project 尚未切换”的崩溃窗口。缺失表示未引用，畸形、路径逃逸、symlink、解码/校验失败一律抛错并保留，不把损坏数据猜成无引用。
- 候选正式删除前重新取得全局 writer lock，核对 owner、终态、stateVersion 与 updatedAt，再非阻塞取得该 job 的 `.execution.lock`；维护在引用查询 await 期间允许 actor 重入，但并发恢复/状态变化会被二次核对或 lease 阻止。正在 create/resume/finalize 的终态收尾对象不能被另一 App 的维护任务移走。
- 删除先在同一 owner 目录原子 rename 为 `.terminal-trash-<UUID>`，再移除目录。进程死在 rename 后时，下次维护会校验 owner、终态、允许文件集合与 lease 后继续删除；tombstone 续删和新候选共享每轮上限。删除意图已经由 rename 持久化，因此续删不重新依赖业务 manifest。
- 正式 job 仅接受真实 UUID 目录、当前 owner/id 匹配的有界 manifest、当前 euid 的普通单硬链接 `job.json`/`.execution.lock`，以及兼容旧版本的空 `inputs/`。未知文件、非空残留输入、symlink、硬链接、损坏 manifest 或引用解析失败全部保留并计数。终态收尾现在即使 request 本来没有输入，也会删除旧的空 `inputs/`，避免正常记录永久被误判为残留。
- Media Studio 可见工作区每个登录 session 只启动一次 best-effort 后台维护，不阻塞页面、不向用户发布维护噪声；退出账号和 ViewModel 销毁会取消任务。维护与 Story ViewModel 共享同一个显式 `StoryProjectStore`，避免引用解析读取另一份默认实例。

验证：覆盖数量与字节预算、最短保留期、最老优先、引用保留与解析失败保守策略、completed/cancelled 与非终态边界、owner 隔离、每轮上限、活跃 execution lease、未知文件/残留输入/硬链接、tombstone 崩溃续删，以及 Generic 精确历史引用、Story canonical project 与 Batch-first 崩溃窗口引用。Debug 与 arm64 Release 各选择性运行 MediaJobStore 39 项、History 10 项、Story 92 项，共 141 项 XCTest 全部通过；Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批只管理 App 内现有 MediaJobStore 的本地终态审计记录，不删除视频历史成片或 Story 业务文件，不把本地删除解释为 provider cancel，也不引入 TTL 抢占。App→Worker 必须显式交接 execution lease；真实双 App/`SIGKILL`/睡眠/断网矩阵、Media Worker、高风险媒体解码隔离和发布级 soak 仍待完成，P0-E 不能标记完成。

### 12.35 第三十五批实施记录：MediaJobStore 真实 SIGKILL 状态矩阵（2026-10-23）

本批把 12.20～12.34 的 durable 状态与 execution lease 语义提升为真实进程死亡回归。测试父进程启动独立 xctest 子进程，子进程运行生产 `MediaStudioVideoJobStore` 路径并持有真实文件锁；到达持久化停点后用 checkpoint 文件通知父进程，父进程发送 `SIGKILL`（signal 9），再由全新的 Store 实例恢复。没有在生产代码中加入测试环境变量、暂停点或模拟崩溃分支，也没有启动/安装 ChatOS App。

已验证：

- `prepared` 在 provider 提交边界之前被强杀，重启仍以相同 stable job ID 和 idempotency key 返回 create mode；该状态按协议尚未长期持 execution lease，不能把可接管误写成 busy。
- `submitting` 尚未持久化 provider ID 时被强杀，重启先进入 `submission_uncertain`/needs-review，绝不退回 create；这直接覆盖最危险的“服务商可能已接单但本地未记 ID”窗口。
- provider ID 已持久化后被强杀，重启只返回同一 provider ID 的 resume mode；`finalizing` 被强杀也保留 provider identity 并进入既有恢复/业务 manifest 对账路径，不生成新 provider task。
- `completed` 被强杀后仍是不可恢复终态，普通 recover 返回 nil，不会因为进程死亡重新开放 provider 工作。
- 从 submitting 开始，子进程存活期间父进程的竞争 recover 明确得到 `jobBusy`，不能先改写 session epoch；signal 9 后内核释放 lease，新 Store 随即按上述状态恢复。每个场景使用独立临时 owner root，退出后删除，不触碰活动用户数据。
- 子进程启动、checkpoint 等待和 SIGKILL 后退出观察均有明确超时；Release 验证发现 Foundation 的同步 `Process.waitUntilExit()` 可在异步 XCTest worker 上无界等待，因此门禁改为有界异步 `isRunning` 观察，避免测试驱动自身制造假挂死。

验证：Debug 与 arm64 Release 均运行 5 个真实 SIGKILL 场景和 1 个仅供显式子进程选择的 probe 入口，共 6 项 XCTest，全部通过；子进程提前退出会回传 stdout/stderr，未到 checkpoint 或 kill 后未及时退出都会明确失败。`git diff --check` 同步作为本批提交门禁。

边界：这是独立 xctest 子进程上的真实内核强杀与生产 Store 文件协议，不是签名后的完整 App/Media Worker 组合验收，也不发真实 provider 请求。它不覆盖 POST 正在内核/网络栈传输、下载 partial、Story/History 文件 rename 任意指令窗口、睡眠/唤醒、断网、ENOSPC、断电或两份正式 App 包并存；这些仍需固定 Release 场景和 Worker 进程矩阵。App→Worker lease 所有权交接与 Media Worker 尚未实现，P0-E 仍不能标记完成。

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
| 维护阶段 kill -9/备份盘空间耗尽 | 能恢复已验证数据库；不丢 WAL；不删除唯一可恢复备份。12.8 已覆盖临时 v34 → v37 库的 12 个备份/迁移停点；COMMIT/VACUUM 内部、旧路径压缩、掉电、真实空间耗尽及实际 Host 组合验收仍待完成 |
| Receipt 保留策略 | 字节预算有效；必要幂等结果仍可重放 |
| Receipt 淘汰后离线重试/并发同 key | 同 key 不同 fingerprint 拒绝；副作用不重复；过期行为明确 |
| COMMIT 报错/写事务生命周期取消 | 原始错误保留；失败提交的业务/回执/淘汰回滚，不能复用状态不明的句柄；取消不能推断未提交。12.9 覆盖收尾，12.10 覆盖 BEGIN/业务与后台清理；真实 I/O、关闭压力及慢语句仍待验收 |
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

- 启动迁移备份保护：`clients/local_agent_host/layers/infrastructure/database/src/maintenance.rs`、`migration_backup_policy.rs`、`maintenance_backup_tests.rs`、`migration_tests.rs`

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
- 文件请求上传与有界响应：`clients/macos/Sources/ChatOSAPI/HTTPTransport.swift`、`clients/macos/Sources/ChatOSNetworking/BoundedURLSessionDataLoader.swift`
- 参考视频增量 JSON/Base64 请求文件：`clients/macos/Sources/ChatOSAPI/ChatOSVideoRequestBodyFileWriter.swift`
- 视频响应流式临时文件：`clients/macos/Sources/ChatOSAPI/ChatOSResponseBodyFileWriter.swift`
- 参考媒体 DTO：`clients/macos/Sources/ChatOSCore/MediaGenerationModels.swift`
- Story 参考视频文件引用：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryVideoGenerationAdapter.swift`、`StoryMediaBatchSession.swift`
- Media Studio 状态：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/MediaStudioViewModel.swift`
- Story Studio 状态：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryStudioViewModel.swift`
- Global Utilities Companion：`clients/macos/Sources/ChatOSApp/Features/GlobalUtilities/GlobalUtilityCompanion.swift`
- Plugin 进程监督：`clients/macos/Sources/ChatOSConnector/NativePluginApplicationProcessRegistry.swift`
- Receipt 策略入口：`clients/local_agent_host/layers/application/src/lib.rs`
- 全命令穷尽分类与内部一次性白名单：`clients/local_agent_host/layers/application/src/command_receipt_policy.rs`
- 分类/重放/防绕过回归：`clients/local_agent_host/layers/application/src/command_receipt_policy_tests.rs`
- 流式请求指纹：`clients/local_agent_host/layers/application/src/command_fingerprint.rs`
- 新旧指纹重放兼容：`clients/local_agent_host/layers/infrastructure/database/src/receipt_fingerprint.rs`
- Receipt 持久化、重放与 v35 回填：`clients/local_agent_host/layers/infrastructure/database/src/receipt_store.rs`
- Receipt 淘汰/重启/迁移回滚回归：`clients/local_agent_host/layers/infrastructure/database/src/receipt_store_tests.rs`
- 正文尺寸计数、预算淘汰、有界序列化与 v36：`clients/local_agent_host/layers/infrastructure/database/src/receipt_cache.rs`
- 正文预算/计数/回滚/共享配额回归：`clients/local_agent_host/layers/infrastructure/database/src/receipt_cache_tests.rs`
- 不可变结果引用、共享回收、摘要校验与 v37：`clients/local_agent_host/layers/infrastructure/database/src/receipt_result.rs`
- inline 边界/引用完整性/事务回收/迁移回滚回归：`clients/local_agent_host/layers/infrastructure/database/src/receipt_result_tests.rs`
- Receipt 保留策略：`clients/local_agent_host/layers/infrastructure/database/src/schema.rs`
- SQLite 维护：`clients/local_agent_host/layers/infrastructure/database/src/maintenance.rs`
- 写事务提交/回滚与连接收尾隔离：`clients/local_agent_host/layers/infrastructure/database/src/transaction.rs`
- 提交失败/清理中断/收尾取消与连接复用回归：`clients/local_agent_host/layers/infrastructure/database/src/transaction_tests.rs`
- BEGIN/业务取消、panic/提前退出与异步回滚回归：`clients/local_agent_host/layers/infrastructure/database/src/transaction_cancellation_tests.rs`；清理 future 的所有权边界回归另见 `transaction.rs` 的测试模块。
- 迁移停点强杀/快照不改写/重启与业务防重回归（仅 Unix 单测）：`clients/local_agent_host/layers/infrastructure/database/src/migration_crash_tests.rs`
- Host 进程级数据库所有权与路径校验：`clients/local_agent_host/src/database_owner.rs`
- 锁竞争/路径别名/诊断不覆盖回归：`clients/local_agent_host/src/database_owner/tests.rs`
- 真实 Host 重复启动/退出/强杀/失败重开回归：`clients/local_agent_host/tests/database_owner_process.rs`
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
| 防重与维护 | v35 防重见 16.2，v36 正文预算见 16.3，v37 不可变引用见 16.4，新版 Host 进程锁见 16.5；owner 配额、旧 Host 交接及在线维护仍待完成 | P0-C 生产迁移前；Host/存储 |
| 分发与升级 | 定义内部测试签名和正式外部分发两条验收路径、旧版本兼容范围 | P2 打包前、P4 发布前；发布工程 |

每份 ADR 必须记录问题、证据、选项、选择理由、失败/回滚行为和验证方式。未冻结的决策不阻止其他独立阶段推进，但不能跨过对应生产启用门禁。

### 16.1 源码与文档漂移治理

- 以协议代码常量与 Schema migration 为事实源。当前 Rust/Swift Host 常量为 v44，而 `clients/local_agent_host/ARCHITECTURE.md` 仍有“validate protocol v28”陈述，属于待修的文档漂移；本方案引用版本不能替代实际握手。
- CI 比较 Swift/Rust 协议版本、payload 边界、支持能力和打包后的真实可执行版本；Schema 兼容矩阵、进程布局及 ADR 随实现更新。
- 为每个阶段归档 manifest、测试脚本、脱敏统计、故障注入结果与 trace，结论标注“现状/目标/已验证”。首批 P0 改动见 12.1；新增 Governor、Worker、job 状态机与门禁仍为待实施项。

### 16.2 ADR：v35 本地命令防重身份（实现范围冻结，生产迁移门禁未通过）

- 问题/证据：原回执行兼任历史正文与副作用防重；24 小时/5,000 条淘汰后 `replay` 返回未见过该命令。对 `ClaimNextRun`，原响应被淘汰可能变成领取另一份 work。v33 的 DROP TABLE 也会在升级时删除尚存身份。
- 备选：永久保留完整响应会延续大 JSON 写放大；淘汰后读取当前实体不等于历史响应；仅依赖实体 ID/版本不能覆盖 claim、删除等所有写命令。本批选择独立长期身份 + 可淘汰历史响应，尚不选择大结果引用形态。
- 权威 key/账户：兼容当前全局 `command_id` 命名空间，不在本次将同 ID 变为“每 owner 可各执行一次”；Host 指纹包含完整命令及 owner，其他账户同 ID 不同内容仍拒绝。旧回执无独立 owner 列，不从摘要猜 owner，也不假称已实现按 owner 的正文预算。完成标记由同一本地事务提交产生，不需要靠实体当前版本重建。
- 保留/过期：防重身份在数据库生命周期内保留，不随正文的时间、条数或未来字节配额删除；尚无业务安全证明前不自动归档。历史响应在缓存中则精确重放，淘汰则 `command_replay_expired`，不得把换新 key 当自动恢复方法。成功且无业务变化的特殊路径（如空 claim 轮询）沿用原先不记回执的行为，不承诺永久缓存空查询结果。
- 迁移/失败：v35 表与逐条回填同一事务，迁移失败不能留下半套身份；迁移前保留已验证快照，未来经过 v33 的路径保留尚存回执。已被旧版清掉的身份无法恢复，需依赖业务约束/对账，不能宣称历史上全部写命令都已获得长期防重。
- 回滚：旧 Host 的 Schema 上限检查拒绝 v35；不通过清空身份表或降低迁移标记使其强行读取。回滚二进制须先明确数据恢复窗口；恢复旧快照会丢失之后的新写入，只能通过明确的数据恢复流程执行。生产迁移前仍需停旧 Host、独占所有者/维护协议和大库/磁盘耗尽/强杀验收，本批源码与临时测试库验证不等于已启用该生产迁移。
- 验证：12.4 的事务、并发、过期 claim、旧指纹与回填失败测试是本 ADR 的最小正确性证据；不替代整套 P0-C 性能及迁移门禁。

### 16.3 ADR：v36 全库响应缓存预算（实现范围冻结，生产迁移门禁未通过）

- 问题/证据：只有 24 小时/5,000 条不能限制大 JSON 总量；每次 `SUM(length(response_json))` 则会把容量控制本身变成正文全扫描。v35 已保留独立防重身份，允许仅淘汰正文而不重新放行写副作用。
- 备选与选择：不采用每次写入扫描全部正文或超限后先完整序列化再丢弃。选择独立尺寸元数据 + 同事务增量计数 + 超限时排序淘汰，全库默认/上限为 128 MiB；现有回执没有 owner 列，暂不伪造按 owner 预算，公平份额与结果引用另行决定。该上限保护逻辑正文，不是整个数据库的容量承诺。
- 单份超限：不写入巨量正文/WAL 后立即删除；有界缓冲超限后仅落成功身份，首次返回业务结果，之后重试按过期处理。继续序列化剩余内容以保留错误/回滚语义。它不等于 512 KiB 以上转不可变引用，不保证超大 IPC 结果能交付。
- 迁移/失败：v36 尺寸回填与新计数/触发器在一个事务内，字节裁剪永不删除 tombstone。v35 若已作为前序迁移提交，不因 v36 失败而被偷偷降级；保留快照并允许再次完成 v36，不自动恢复旧库覆盖业务数据。
- 回滚/门禁：旧包按 Schema 上限拒绝 v36，不能仅改版本号绕过。独占所有者、生产大库空间峰值与耗时、强杀/真实磁盘耗尽和维护恢复仍是生产启用前置条件；暂未交付自动归档或物理压缩。验证证据见 12.5，不能以逻辑字节限额替代进程 RSS、WAL 或发布性能验收。

### 16.4 ADR：v37 数据库内不可变结果引用（实现范围冻结，生产迁移门禁未通过）

- 问题/证据：v36 虽然限制总正文，但单条回执仍可能携带接近 128 MiB 的 JSON；引用可变 Run 的当前值无法满足精确重放。固定摘要应指向原始历史字节，而不能把返回实体最新状态冒充原响应。
- 备选与选择：选择同一 SQLite 中的内容寻址结果表与显式外键列，超过 512 KiB 才引用，相同字节共享。暂不新增文件 sidecar，以免在尚无 durable 文件所有权/孤儿对账协议时引入跨存储提交窗口；也不把普通业务 JSON 中看似引用的字段当存储格式。该选择提供回执正文隔离和相同结果缓存去重，不承诺已解决不同业务表的重复写入。
- 所有权/预算：回执持有强引用，最后一个引用删除时同事务回收正文。全库配额按每命令完整逻辑字节保守计费，物理共享不扩大可重放集合。tombstone 继续独立保留；正常淘汰为 `command_replay_expired`，仍被引用但正文缺失或校验失败为不可重试 `storage_corrupt`，两者均不能重新放行副作用。
- 迁移/失败：v36 → v37 在单事务内逐条转换原始大结果，逻辑字节数保持不变；失败回滚本版本，前序已提交版本不倒退，保留恢复快照。新正文与业务事实同事务，避免文件 rename/数据库提交之间的悬空引用；这不扩展为外部 provider 或文件操作的恰好一次保证。
- 回滚/门禁：旧包按 Schema 上限拒绝 v37，禁止通过清版本标记、删引用或清身份强行降级。引用格式未改变 wire 协议，但并不代表旧 Host 能读新 Schema；生产迁移仍须独占维护与大库空间/耗时/故障验收。首次大结果写入及重放都需实测，不能用小型正确性回归替代性能结果；证据和限制见 12.6。

### 16.5 ADR：Host 数据库协作式进程所有权（实现范围冻结，生产迁移门禁未通过）

- 问题/证据：SQLite 写事务锁不覆盖启动备份/迁移/压缩之间的间隙，也不赋予唯一业务调度所有权。生产入口当前仅 Host `main.rs` 打开这份数据库，因而在这里先取得锁可同时保护启动阶段和正常运行，而不把测试用的多连接能力伪装成多 Host 所有权。
- 选择：采用数据库专属持久 sidecar 上的 OS 非阻塞独占锁，先加锁再创建池，持锁至操作系统销毁进程。获取失败不等待、不接管、不改当前所有者诊断；新 owner 的随机 epoch 在取得锁后产生，仅用于诊断。不以 PID/修改时间/TTL 推断旧执行已经终止，不允许 unlink 锁文件来“解锁”。
- 路径与生命周期：解析数据库/父目录别名形成共同锁身份，Unix 拒绝硬链接与不安全锁路径；保留 SQLite 配置路径与 Artifact 根目录兼容。进程级静态持有避免异步入口返回时早于后台资源销毁释放所有权，崩溃或 SIGKILL 交给内核释放，不把残留文件当成仍持锁。
- 兼容/失败：不改变 Schema/wire。新版与新版互斥，旧版与新版不保证互斥；旧 Host 未确认停止就禁止生产升级。持锁路径必须受信且稳定，绕过协议/替换 inode 的写入者不受 advisory lock 强制隔离；当前 epoch 未实现提交 fencing，未来独立维护入口和跨版本交接需遵守同一协议。
- 门禁：macOS 临时库上的真实子进程测试证明重复启动拒绝、原所有者健康、正常退出/启动失败/空闲强杀后可重开，不证明迁移中途强杀、跨版本 App 双开、磁盘满、Windows 行为或在线维护完成。验证与边界见 12.7；不得据此启用无人值守的活动库治理。

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
