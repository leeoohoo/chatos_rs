# ChatOS macOS 客户端稳定性、性能与进程架构方案

更新时间：2026-10-22

状态：强化版方案，P0 部分实施中；阶段记录见 12.1—12.39，完整 Release 验收待完成

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
- App 启动 Host 时使用 12.12、12.84 的进程组逻辑 job 预算：未配置时同时按 CPU 和物理内存计算 2–16 的静态启动额度，为 App/OS 保留 2 个逻辑核与 2 GiB，每个后台 job 预算 2 GiB；Host 默认取得约四分之一且至少 1 个额度，其余交给独立 Swift Agent Run。可通过 `CHATOS_BACKGROUND_JOB_BUDGET` 显式配置 2–16 的总额，旧 Host 单项覆盖也会被总额和 Swift 保留位约束。独立运行 Rust Host 且未指定参数时，其 Rust 默认值仍限制在 2–8；两者必须分别记录，不能混称一个默认值；
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
| 后台逻辑 job | Host 与 Swift Agent 合计按 CPU/物理内存取较小值，默认 2–16；显式覆盖仍为 2–16 | 为 App/OS 保留 2 核/2 GiB、每 job 2 GiB；分配给两者的 permit 总数不超过上限，批次 64 不等于并发 64 |
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

### 12.36 第三十六批实施记录：视频下载临时 artifact 崩溃对账（2026-10-23）

本批收口生产视频响应流在进程被 SIGKILL 后遗留的下载临时文件。原实现直接创建 `chatos-video-download-<UUID>.tmp`，正常取消和对象释放能够删除，但崩溃后既没有活跃所有权凭据，也不能安全地区分仍被另一 App 使用的文件；现在每次下载拥有独立的小型 artifact 目录和由内核释放的 lease。

已实施：

- 新下载使用严格 UUID 命名的 `.chatos-video-download-<UUID>/`，目录强制 `0700`，内部只包含 `0600` 的 `.lease` 与 `payload.tmp`。lease 和 payload 均以 `O_NOFOLLOW | O_CLOEXEC | O_EXCL` 创建并校验为当前 euid 的普通单硬链接文件；下载仍按 chunk 流式写入，不重新引入整段视频 `Data`。
- writer 从创建目录开始持有 `.lease` 的 `flock(LOCK_EX)`，成功返回后由 `GeneratedMediaFile` 的共享清理所有者继续持有。显式消费完成、最后引用释放、取消、超限、流错误或写入失败都会幂等删除整个目录，并在删除完成后才释放 lease；SIGKILL 则由内核自动释放锁，保留可对账结构。
- 每次新下载前执行 best-effort 有界维护：仅检查超过 24 小时的严格 UUID 目录，每轮最多 64 个。清理方只有在目录属于当前 euid、内容严格限定为 lease/payload、文件不是 symlink/硬链接，并能非阻塞取得独占 lease 后才删除；取得锁后再次核验目录 inode、内容集合和文件身份，无法证明安全时全部保留。
- 新鲜 orphan、活跃 lease、畸形 UUID、symlink 目录、symlink payload、硬链接 payload、未知文件和无 lease 的非空旧结构均保留。只允许回收崩溃发生在 `mkdir` 与 lease 创建之间留下的过期空目录；维护错误不阻断当前下载或改变原磁盘预算语义。
- `GeneratedMediaFile` 增加可选的线程安全自定义清理动作，默认初始化器和既有单文件删除行为保持兼容；生产视频下载用它同时管理 artifact 目录与 lease descriptor，而调用方仍只看到可直接复制/读取的 payload URL。

验证：`HTTPTransportTests` 新增目录/文件权限与正常释放、活跃 lease 保护及释放后回收、新鲜与异常结构保守策略、64 候选上限、取消和流失败无残留；原有流式内容、超限 partial、Content-Length 截断和磁盘准入测试继续覆盖。Debug 与 arm64 Release 均选择性运行 13 项 HTTP transport 测试，Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批只管理新版目录化的视频下载结果，不猜测或删除旧版 lease-less 的 `chatos-video-download-*.tmp`，也尚未给上传侧 `chatos-video-request-*.json` / multipart request body 建立同等崩溃对账。测试覆盖真实 `flock` 竞争但尚未用独立下载 writer 子进程执行 SIGKILL；真实网络中断、睡眠/唤醒、写入中 ENOSPC、App→Worker lease 所有权交接、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.37 第三十七批实施记录：视频下载 writer 真实 SIGKILL 回归（2026-10-23）

本批把 12.36 的下载 artifact lease 从同进程 descriptor 竞争提升为真实进程死亡验证。父测试启动独立 xctest 子进程，子进程运行生产 `ChatOSResponseBodyFileWriter`，写入部分视频响应后故意让 `AsyncThrowingStream` 保持未完成；只有在真实 payload 已出现且字节数可见后才发布 checkpoint，随后由父进程发送 signal 9。生产 writer 没有增加测试环境分支、暂停点或崩溃钩子。

已验证：

- 子进程仍存活时，父进程把 artifact 修改时间设为超过 24 小时并执行生产维护；跨进程 `.lease` 独占锁使维护返回零删除，真实 partial payload 和目录保持存在，证明年龄不能越过活跃 writer 所有权。
- 父进程发送 `SIGKILL` 后使用有界异步 `Process.isRunning` 观察退出，不依赖 Release 下可能无界等待的同步 `waitUntilExit()`；终止原因和 signal 均必须精确匹配 `SIGKILL`。
- 进程死亡没有执行 Swift `deinit`、取消或错误清理，但内核释放 `flock`；全新的维护调用随即取得同一 lease，校验目录/文件结构并删除过期 artifact，证明 partial 下载不会在强杀后永久泄漏。
- 每个场景使用独立临时根目录并在测试结束清理，不启动/安装 ChatOS App、不发真实网络请求，也不读取活动用户数据。checkpoint 超时、子进程提前退出和 kill 后未及时退出均有明确失败路径。

验证：Debug 与 arm64 Release 各选择性运行 1 项父进程 XCTest；该测试内部再精确选择 1 项 probe 入口运行独立 xctest 子进程，均通过。Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批验证的是生产 writer、真实文件锁和真实 signal 9，但响应源仍是受控内存 stream，不覆盖 URLSession 正在接收内核网络数据、真实断网/睡眠、写入中 ENOSPC、断电或文件系统损坏。上传 request body 临时文件对账、App→Worker lease 所有权交接、Media Worker、完整签名 App 强杀及 Release soak 仍待完成，P0-E 不能标记完成。

### 12.38 第三十八批实施记录：上传 request body 临时 artifact 对账（2026-10-23）

本批补齐 12.36/12.37 明确保留的上传侧临时文件缺口。视频参考输入生成的 JSON request body 和图片编辑生成的 multipart body 此前都是 `/tmp` 下的单文件，正常 provider 请求结束会删除，但 writer 或 App 被 SIGKILL 后无法证明文件是否仍被另一上传使用；现在两条生产 writer 共用同一个 lease-aware artifact 协议。

已实施：

- 视频请求写入 `.chatos-video-request-<UUID>/request.json`，图片 multipart 写入 `.chatos-image-request-<UUID>/request.multipart`；目录强制 `0700`，`.lease` 与 payload 强制 `0600`，创建使用 `O_EXCL | O_NOFOLLOW | O_CLOEXEC` 并校验当前 euid、普通文件和单硬链接。
- writer 从目录创建到请求消费完成持续持有 `.lease` 的独占 `flock`。`HTTPRequestBodyFile` 增加共享、线程安全、幂等的清理所有者；值在 request、service 与测试 transport 之间复制时共享同一 owner，任一显式完成清理或最后一份副本释放只执行一次目录删除，并在删除后释放 descriptor。
- `sendProviderRequest` 不再只删除 payload URL，而是调用 body file 的所有权清理，从而同时删除目录并关闭 lease。写入失败、取消、provider 拒绝、账户 session 校验失败和正常完成沿用同一清理路径；现有不带自定义 owner 的 `HTTPRequestBodyFile` 仍保留原单文件删除兼容语义。
- 每次新建上传 body 前执行 best-effort 维护，跨视频 JSON 与图片 multipart 共用每轮 64 个、24 小时的上限。只有严格 UUID、真实目录、当前 euid、内容集合精确匹配、lease/payload 均为普通单硬链接并能非阻塞取得 lease 的候选才删除；锁后再次核验 inode、文件身份和内容集合。
- 新鲜或活跃 artifact、畸形名称、symlink 目录/payload、硬链接 payload、未知文件和无 lease 的非空旧结构全部保留；维护失败不阻断新请求，上传仍按既有 chunk 流式生成、磁盘准入与 64/161 MiB request 上限执行。

验证：新增 4 项 `UploadBodyArtifactTests`，覆盖两种 writer 的实际目录/文件权限、payload 名称与显式清理，过期但活跃的 lease 保护，视频/图片混合 65 个 orphan 的每轮 64 上限，以及畸形 UUID、symlink、硬链接、未知文件和 lease-less 非空目录的保守策略。既有 `ChatOSMediaGenerationServiceTests` 29 项继续验证视频/音频/首尾帧增量 Base64、图片 multipart、多输入、低磁盘、provider 拒绝和正常请求后删除。Debug 与 arm64 Release 选择性运行上述 33 项测试，Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批只管理新版目录化 request body，不猜测或删除旧版 lease-less 的 `chatos-video-request-*.json` 与 `chatos-image-request-*.multipart`。尚未用独立 writer/上传子进程对 request body 执行真实 SIGKILL，也不覆盖 URLSession 正在从文件读取时的睡眠/断网、真实 ENOSPC、断电或文件系统损坏。App→Worker lease 所有权交接、Media Worker、图片/音频响应侧的大对象治理和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.39 第三十九批实施记录：上传 request body 真实 SIGKILL 回归（2026-10-23）

本批用真实进程死亡验证 12.38 的两种上传 artifact。父测试分别启动独立 xctest 子进程，子进程运行生产视频 JSON writer 或图片 multipart writer，完整生成 request body 并持续持有返回的 `HTTPRequestBodyFile`；checkpoint 记录真实 artifact/payload 身份和精确字节数后，父进程先竞争维护，再发送 signal 9。生产 writer 和所有权模型没有增加测试暂停分支或崩溃钩子。

已验证：

- 视频 JSON 与图片 multipart 两种子进程都能生成符合各自命名协议、字节数与 payload 文件的 artifact。父进程把目录修改时间设为超过 24 小时后调用生产维护，跨进程活跃 lease 仍使删除数为零，证明完成写入但尚在上传/消费的 body 不会仅因年龄被回收。
- 父进程真实 `SIGKILL` 后不执行 `HTTPRequestBodyFile` 的显式清理或 deinit；有界异步退出观察确认 termination reason/signal，随后新的维护进程取得由内核释放的 lease，分别安全删除两种过期 artifact。
- 每个 mode 使用独立临时根目录，不启动/安装正式 App、不请求 provider、不读取活动用户数据。checkpoint 缺失、子进程提前退出、SIGKILL 后未及时退出、payload 字节数不一致或活跃 lease 被误删都会明确失败。

验证：Debug 与 arm64 Release 各选择性运行 2 项父进程 XCTest；每项内部精确选择同一个 probe 入口并以不同 mode 运行独立 xctest 子进程，全部通过。Swift 6 编译与 `git diff --check` 同步作为本批提交门禁。

边界：本批覆盖“request body 已生成、上传尚未结束”窗口的真实进程强杀，不注入 writer 每个 chunk 的中间指令窗口，也不证明 URLSession 内核/daemon 已停止读取文件。真实 provider 网络、睡眠/唤醒、断网、写入中 ENOSPC、断电、App→Worker lease 交接、Media Worker 和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.40 第四十批实施记录：Story 图片参考输入文件原生化（2026-10-23）

本批消除 Story 首帧/尾帧生成在应用层仍保留的确定性图片内存放大。审计确认，单次生成适配器和持久 Media Batch 都会先把每张项目图片完整读成 `Data`，再生成约 4/3 体积的 Base64 字符串；API 层随后又把 Base64 解码并写入 multipart 临时文件，使同一参考图在提交前同时存在项目文件、`Data`、Base64 和 multipart 副本。

已实施：

- 单次 Story 首帧/尾帧生成对已确认人物/场景/道具图、上一段已确认尾帧和当前段已确认首帧，均直接构造带项目 `fileURL` 的 `ImageGenerationInputImage`；不再调用 `MediaStudioImageLoader` 预读输入，也不再在 Story 层生成 Base64。
- 持久 Media Batch 的 frames/lastFrames 路径采用同一文件引用方式，删除异步整图读取 helper；已有参考图顺序、文件名、MIME、prompt reference index、continuation fencing 和 Batch intent/完成提交协议保持不变。
- 全部为本地文件引用且 Base64 为空时，既有 `ChatOSImageGenerationWire` 会自动进入 12.38 已加固的流式 multipart writer，因此大图只按 chunk 复制到受 lease 保护的 request artifact，不重新拼出整份内存 multipart。
- 回归分别验证单次首帧与尾帧、持久 Batch 首帧与尾帧请求：参考资源、上一段尾帧和当前段首帧都保留正确顺序与真实文件内容，`base64Data` 为空且 `fileURL` 为本地文件；连续性 prompt index 和生成后的 Story/Batch 状态仍正确。

验证：Debug 与 arm64 Release 各运行完整 `StoryStudioTests` 93 项，全部通过；新增的单次生成与持久 Batch 文件引用回归包含在内。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。测试只使用独立临时目录和受控内存 provider，不启动/安装正式 App，不读取活动用户数据。

边界：本批只消除 Story 图片生成**输入参考图**的 `Data + Base64` 峰值；provider 返回图片目前仍可能是完整 JSON/Base64，输出解码、AppKit 归一化和项目落盘仍在主进程。真实大图 memory/physical-footprint trace、响应侧流式协议、恶意/超大像素解码隔离、网络/睡眠/ENOSPC 故障矩阵、App→Worker lease 交接、Media Worker 和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.41 第四十一批实施记录：图片响应 Base64 零输出分配校验（2026-10-23）

本批继续收敛 12.40 明确保留的图片响应峰值。`generateImage` 在 JSON 解码得到 Base64 字符串后，此前会先调用 `Data(base64Encoded:)` 生成最多 20 MiB 的完整图片副本，只用它检查字节数便立即丢弃；Story 或历史持久化随后还会再解码同一个字符串。这是与图片大小线性增长、且对每次 Base64 图片响应必然发生的一份冗余缓冲。

已实施：

- API 层新增只遍历 `String.UTF8View` 的严格 Base64 校验器，不构造解码输出；校验标准字母表、四字符分组、只允许末尾一至两个 padding，并通过编码长度和 padding 精确计算解码字节数。
- `generateImage` 仍在返回 Core DTO 之前拒绝空内容、畸形 Base64 和解码后超过 20 MiB 的图片，既有错误类型、provider 身份、MIME、revised prompt 与后续消费协议不变；合法响应不再为预校验额外分配最多 20 MiB 的 `Data`。
- 新回归覆盖精确字节数、空值、缺失 padding、中间 padding、非法字符和上限越界，并通过完整 service 调用确认畸形 provider 响应仍在 API boundary 返回 `invalidProviderResponse`；原有合法 `b64_json` 路径继续通过。

验证：Debug 与 arm64 Release 各运行 `ChatOSMediaGenerationServiceTests` 31 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。测试使用受控 transport，不启动/安装正式 App、不请求真实 provider、不读取活动用户数据。

边界：本批只删除“预校验后丢弃”的重复解码副本；HTTP 响应 `Data`、JSONDecoder 产生的 Base64 `String` 和业务消费者真正需要的单次图片解码仍同时存在，尚未实现 JSON/Base64 流式落盘。图片像素解码与 PNG 归一化仍在主进程；真实大响应 memory trace、响应 artifact/崩溃对账、Media Worker、睡眠/断网/ENOSPC 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.42 第四十二批实施记录：图片响应 Base64 增量文件化（2026-10-23）

本批继续治理 12.41 明确保留的业务层图片解码和跨层 Base64 传递。此前 API 层把 provider 的 `b64_json` 原样装入 `GeneratedMediaAsset`；History 或 Story 随后再次把完整字符串解码为 `Data`。除 HTTP 响应体和 JSONDecoder 的字符串外，业务消费期间还会常驻一份完整图片字节，并允许大 Base64 继续穿过 API/Core/App 边界。

已实施：

- API 层新增 Base64 响应文件 writer：先复用严格长度校验，再按 64K quartets（约 192 KiB 解码输出）增量解码，直接写入 `.chatos-image-response-<UUID>/payload.tmp`，不再构造完整 decoded `Data`。空值、畸形 padding、非法字符和超过 20 MiB 的响应仍在 provider boundary 失败。
- 图片响应 artifact 复用视频下载 writer 的文件安全和生命周期协议：目录权限 `0700`、payload/lease 权限 `0600`，写入前执行磁盘预算检查，取消或失败删除 partial，成功结果由 `GeneratedMediaFile` 持有；24 小时孤儿清理按独立图片前缀扫描，避免和视频 artifact 互相误判。
- `GeneratedMediaAsset` 新增仅进程内的 `mediaFile` owner，并把其 URL 作为消费入口；owner 不进入 Codable，旧 Base64/HTTPS 资产保持兼容。API 返回的 Base64 图片现在 `base64Data == nil`，释放生成结果会自动删除临时 artifact。
- Media History 对 file-backed 图片执行 20 MiB 上限的有界复制，复制期间不读取整文件；持久副本建立后即与临时响应 artifact 解耦。Story 原有文件 URL 消费路径保持兼容，没有重新引入 Base64。
- 回归覆盖跨多个解码 chunk 的 400,000 字节内容、真实文件字节、目录/文件权限、lease 防误删和显式清理；完整 service 回归覆盖文件化结果及释放删除；History 回归覆盖持久副本在临时 owner 释放后仍可读取。

验证：Debug 与 arm64 Release 各选择性运行 `ChatOSMediaGenerationServiceTests`、`Base64PayloadFileWriterTests`、`HTTPTransportTests`、`MediaStudioHistoryStoreTests` 和 `StoryStudioTests` 共 150 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。测试只使用受控 transport 和独立临时目录，不启动/安装正式 App、不请求真实 provider、不读取活动用户数据。

边界：provider 当前仍以单个 JSON body 返回 Base64，因此 URLSession 的 HTTP response `Data` 和 JSONDecoder 产生的完整 Base64 `String` 会存在到 API 解析结束；本批消除的是完整 decoded `Data` 常驻和解析后的跨层 Base64 传递，不是网络层 JSON 流式解析。Story/AppKit 的图片像素解码与 PNG 归一化仍在主进程；真实大响应 physical-footprint trace、writer 中途 SIGKILL、ENOSPC/睡眠/断网故障矩阵、Media Worker 文件授权和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.43 第四十三批实施记录：图片响应 artifact 真实 SIGKILL 对账（2026-10-23）

本批为 12.42 的图片响应文件所有权补真实进程死亡门禁。普通单元测试释放 Swift owner 只能覆盖正常析构，不能证明 App/未来 Worker 被 `SIGKILL` 时内核会释放 lease，也不能证明下一次启动式对账会回收孤儿而不误删另一个仍在使用文件的进程。

已实施：

- 扩展既有 response-body crash harness，以显式 mode 分别运行视频下载和图片响应生产 writer；每个 mode 使用独立临时根目录和独立 xctest 子进程。
- 图片 probe 通过真实 `ChatOSBase64PayloadFileWriter` 解码跨多个 chunk 的 400,000 字节 payload，保留返回的 `GeneratedMediaFile` owner 并写出 checkpoint；父进程把 artifact 时间改为 48 小时前，确认跨进程 `flock` 仍阻止清理。
- 父进程向 probe 发送真实 `SIGKILL`，等待内核终止后调用图片专属 `.imageResponse` 对账；验证 lease 随进程死亡释放，过期 artifact 恰好回收一次，payload 不再残留。既有视频“写入中被强杀”场景继续作为同一 harness 的回归。

验证：Debug 与 arm64 Release 各运行 2 项父进程 XCTest；每项内部精确选择同一个 probe 入口并运行独立 xctest 子进程，全部通过。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。测试不启动/安装正式 App、不请求 provider、不读取活动用户数据。

边界：图片 probe 的强杀点位于增量解码完成、结果 owner 正在使用的窗口；本批没有向 Base64 writer 每个 chunk 注入停点，因此不单独证明图片 partial 写入的每个指令窗口，但底层通用 response writer 的视频 mode 已覆盖“payload 写入中”强杀与同一清理协议。真实 App/Worker 启动接线、断电、写入中 ENOSPC、HTTP/JSON 流式解析、主进程像素解码隔离和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.44 第四十四批实施记录：Story 文件图片直接解码（2026-10-23）

本批继续消除 12.42 文件化之后残留的应用层源文件副本。此前 Story 的资源图、首尾帧和 Batch 收到 file-backed provider 结果后，仍先把最多 20 MiB 的临时文件完整读成 `Data`，随后才交给 ImageIO 解码和 PNG 归一化；因此响应文件与同尺寸源 `Data` 会在 App 进程内并存。

已实施：

- 本地图片解码器改为以 `O_NOFOLLOW | O_CLOEXEC` 打开文件，对已打开 descriptor 执行 `fstat`，只接受非空、未超过 20 MiB 的普通文件，再通过 `/dev/fd/<descriptor>` 直接交给 ImageIO；不再为 file-backed 输入先创建完整源 `Data`，符号链接在解码前拒绝。
- Data 与文件两种入口复用同一像素预算和缩略解码核心：源像素仍限制为 64 MP，归一化最长边仍限制为 8,192 px，ImageIO 立即生成有界像素面后再编码 PNG；本地缩略图加载也同步复用文件入口，避免预览路径重新引入整文件副本。
- 新增 `normalizedPNGData(for:)`，file-backed `GeneratedMediaAsset` 在 detached 工作中直接解码，并用显式 owner lifetime 保证 lease-owned 临时文件直到 ImageIO 和 PNG 编码结束才可释放。旧 Base64 与 HTTPS 结果继续走有界兼容路径。
- Story 单次资源图、单次首尾帧、持久 Media Batch 图片步骤和生成图片导入统一使用 asset 级归一化入口；既有 attempt fencing、账户 continuation check、provider identity 和 Store 完成提交协议保持不变。
- 回归覆盖 file-backed owner 在归一化期间存活、输出尺寸正确、完成后源文件随 owner 释放，以及 symlink 在 ImageIO 前被拒绝；完整 Story、图片交互和缩略图回归继续验证旧 Base64/本地历史兼容。

验证：Debug 与 arm64 Release 各选择性运行 `StoryStudioTests` 93 项、`MediaStudioImageInteractionTests` 12 项、`AppThumbnailPixelRepresentationTests` 3 项和 `AppBoundedFileReaderTests` 16 项，共 124 项，全部通过。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁；未启动/安装正式 App，未请求 provider，未读取活动用户数据。

边界：本批删除的是 file-backed 输入的完整**压缩源字节**副本；ImageIO 的有界解码像素面和最终 PNG `Data` 仍位于 App 进程，Store 也仍以 `Data` 接收归一化结果。HTTPS/旧 Base64 兼容资产仍会产生源 `Data`。将恶意图片解码、PNG 编码和最终文件提交整体迁入 Media Worker，真实 20 MiB/64 MP physical-footprint trace、内存压力、取消风暴和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.45 第四十五批实施记录：Story 本地图片授权内直接解码（2026-10-23）

本批关闭 12.44 尚未覆盖的用户本地导入路径。Story 上传资源图或首尾帧时，此前会在 security-scoped 授权窗口内把最多 20 MiB 文件完整读成 `Data`，结束授权后再做 ImageIO 解码；即使文件已在磁盘上，也会产生一份确定性的压缩源字节副本。

已实施：

- 将 12.44 的安全文件归一化抽成明确的 `fromFile` 入口；生成结果继续通过 asset owner 保持 lease，本地导入则在 security-scoped 访问仍有效时直接等待该入口完成，并在归一化结束、项目 Store 提交前释放授权。
- 本地文件由后台任务以 `O_NOFOLLOW | O_CLOEXEC` 打开并对 descriptor 执行 `fstat`，20 MiB、普通文件、64 MP 和 8,192 px 归一化边界与生成图片完全一致；Story adapter 不再调用 `AppBoundedFileReader.read` 构造完整源 `Data`。
- 正常导入后的 PNG 保存、用户选中首帧、确认关系、账户 continuation fencing 和项目持久化顺序不变；新增端到端回归确认 symlink 导入在解码前失败，且不会向 Story 挂入图片或确认帧。

验证：Debug 与 arm64 Release 各选择性运行 `StoryStudioTests` 94 项、`MediaStudioImageInteractionTests` 12 项和 `AppBoundedFileReaderTests` 16 项，共 122 项，全部通过。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁；未启动/安装正式 App，未读取活动用户数据。

边界：security-scoped 授权覆盖异步文件打开、解码和 PNG 编码，归一化结束后即释放，不延伸到项目 Store 提交；未来 Worker 文件授权协议仍需定义 descriptor/lease 成功交接点。本批仍在 App 进程内持有解码像素面和 PNG 输出 `Data`，没有实现跨进程图片解码、文件式 Store 提交或真实内存压力/签名 Release soak，P0-E 不能标记完成。

### 12.46 第四十六批实施记录：file-backed 图片预览直接解码（2026-10-23）

本批补齐 12.44 后仍绕回整文件读取的两个高频 UI 消费点。媒体卡片/Story 缩略图缓存与全屏图片预览此前统一调用 `MediaStudioImageLoader.data(for:)`，导致 file-backed provider 响应和本地历史图片仍会先复制为最多 20 MiB 的完整 `Data`，再进行下采样解码。

已实施：

- `MediaStudioImageLoader` 新增统一 thumbnail 入口：file URL 直接调用 12.44 的 `O_NOFOLLOW`/`fstat`/`/dev/fd` 解码路径，Base64 与 HTTPS 资产继续通过既有有界下载/解码兼容路径；64 MP 源像素预算和调用方显示像素上限保持不变。
- file-backed asset 在异步缩略解码期间显式保留 `GeneratedMediaFile` owner，避免响应 artifact 在 ImageIO 仍读取 descriptor 时提前释放；解码完成后缓存只持有有界像素表示，不延长源文件生命周期。
- 媒体卡片共享缩略图缓存与全屏预览改用同一入口，删除各自“整文件 Data + 再次 detached decode”的重复实现；既有 waiter-aware 取消、NSCache 数量/成本限制和 Retina 像素边界保持不变。
- 新回归使用 lease-owned file-backed 图片通过真实共享缓存下采样，确认 owner 在解码期间存活、输出不超过目标 24 px，调用方释放 asset 后源文件立即删除；旧 Base64、历史文件和远端无凭据下载回归继续通过。

验证：Debug 与 arm64 Release 各运行 `MediaStudioImageInteractionTests` 13 项和 `AppThumbnailPixelRepresentationTests` 3 项，共 16 项，全部通过。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁；未启动/安装正式 App，未访问活动用户数据库。

边界：本批消除的是预览/缩略图 file-backed 输入的完整压缩源 `Data`；ImageIO 解码工作集和缓存的有界位图仍属于 App 进程，Base64/HTTPS 兼容输入仍需要源 `Data`。Story PNG 输出到 Store 的文件式提交、Media Worker 解码隔离、真实滚动/快速切图 memory trace 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.47 第四十七批实施记录：Story 归一化 PNG 输出文件化（2026-10-23）

本批关闭 12.44–12.46 仍保留的最终 PNG 内存副本。此前 Story 即使已经从文件直接解码，ImageIO 仍先把归一化 PNG 完整编码进 `NSMutableData`，随后 Store 再把该 `Data` 写入项目目录；高分辨率图片会让有界像素面、完整 PNG 内存和磁盘源/目标在主进程内同时存在。

已实施：

- 新增归一化图片 artifact writer，复用 response writer 的安全目录、预打开文件、`0700/0600` 权限、lease、磁盘预算、失败清理和 24 小时孤儿对账协议，并用独立 `.chatos-image-normalized-<UUID>` 前缀隔离视频下载和 provider 图片响应。
- ImageIO 改由 `CGDataConsumer` 回调直接向 writer 持有的文件 descriptor 增量写入，不再创建完整 PNG `Data`。consumer 在每次写入前执行 20 MiB 累计上限，正确处理短写、`EINTR` 和 `ENOSPC`；writer 完成后再次用 `fstat` 校验同一个已打开普通文件的 owner、link count、非空和最终大小。
- Story 的资源图生成、首尾帧生成、持久 Batch 图片步骤、生成图片导入和 security-scoped 本地图片导入全部改为接收 lease-owned `GeneratedMediaFile`。本地授权只覆盖源文件解码和输出 artifact 建立，随后项目复制不再依赖原始授权。
- `StoryProjectStore` 新增 20 MiB file-backed 图片保存入口，通过 `O_NOFOLLOW`/`fstat` 的有界后台复制写入项目目录；复制期间显式保留临时 owner。资源图与帧完成路径在 actor 跨 `await` 前后都核对 attempt ID，manifest 合并失败或 attempt 已变化时删除刚建立的项目副本，避免悬空文件和迟到结果串位。
- 保留原 `Data` overload 作为旧调用与既有回归兼容边界，生产 Story 路径不再调用它。新增回归覆盖 artifact 前缀、权限、lease 防误删、低磁盘/超限无残留、真实 PNG 可解码及 owner 释放删除，以及 Store 复制后临时源删除而项目副本仍可读取、file-backed 资源图/帧 attempt 正确完成。

验证：Debug 与 arm64 Release 各选择性运行 `Base64PayloadFileWriterTests` 1 项、`ResponseBodyFileWriterCrashTests` 2 项、`NormalizedImageArtifactWriterTests` 2 项、`MediaStudioImageInteractionTests` 14 项和 `StoryStudioTests` 96 项，共 115 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。测试只使用独立临时目录和内存生成图片，不启动/安装正式 App、不读取活动用户数据库。

边界：本批消除的是 Story 归一化阶段的完整 PNG `Data`，不是解码像素面；ImageIO 解码/编码仍在 App 进程，旧 Base64/HTTPS 输入仍可能有完整压缩源 `Data`。归一化 artifact 尚未补独立真实 `SIGKILL` probe，当前复用同一 lease/对账实现并由普通回归验证；Media Worker、恶意图片进程隔离、真实 20 MiB/64 MP physical-footprint trace、内存压力/取消风暴、写入中强杀与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.48 第四十八批实施记录：归一化图片写入中 SIGKILL 对账（2026-10-23）

本批补齐 12.47 明确保留的进程死亡门禁。普通成功/失败回归只能证明 Swift 正常析构会删除临时 artifact，不能证明 ImageIO/未来 Media Worker 正在向预打开 descriptor 写入时遭遇 `SIGKILL`，内核会释放 lease 且下次启动式对账能安全回收 partial。

已实施：

- 扩展既有 response-body crash harness，新增独立 `normalizedImage` mode，并继续使用 disposable xctest 子进程、独立临时根目录和生产 `ChatOSMediaArtifactFileWriter`。
- probe 在 writer 的同步 populate 窗口内写入并同步 128,000 字节 payload，建立 checkpoint 后停在尚未返回 `GeneratedMediaFile` 的真实写入栈上；该停点同时持有预打开 payload descriptor 与跨进程 `flock` lease。
- 父进程把 artifact 目录时间改为 48 小时前，先验证活跃子进程仍阻止归一化图片专属对账；随后发送真实 `SIGKILL`，确认进程无 Swift defer/deinit 机会、lease 由内核释放，`.normalizedImage` 对账恰好回收一个过期 partial 且 payload/目录均不再残留。
- 视频下载写入中强杀和 provider 图片结果 owner 强杀继续在同一枚举式 harness 中运行，防止新增 artifact kind 破坏既有前缀隔离与清理行为。

验证：Debug 与 arm64 Release 各运行 `ResponseBodyFileWriterCrashTests` 3 项；每项内部精确选择同一个 probe 入口并启动独立 xctest 子进程，全部通过。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁；未启动/安装正式 App，未访问活动用户数据库。

边界：本批证明的是进程级 `SIGKILL` 与 lease/orphan 协议，不模拟断电后文件系统持久化顺序，也不验证真实 ENOSPC 发生在 ImageIO callback 的每个短写位置。Media Worker 尚未建立，主进程仍承担像素解码/编码；真实大图 physical-footprint、睡眠/唤醒、断电、磁盘耗尽矩阵和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.49 第四十九批实施记录：内存 PNG 归一化入口退役（2026-10-23）

本批在 12.47/12.48 文件链路稳定后收紧代码边界。审计确认生产 Story 已全部使用 `normalizedPNGFile`，但 `MediaStudioImageLoader` 仍保留三组无生产调用者的 `normalizedPNGData` 方法和 `CGImageDestinationCreateWithData` 编码器；未来维护者仍可能绕过文件协议，重新引入完整 PNG 内存副本。

已实施：

- 删除 Data 输入、asset 输入和 file URL 输入三组归一化 `Data` 返回入口，以及基于 `NSMutableData` 的 PNG 编码实现；Media Studio 归一化现在只有 lease-owned 文件输出这一条应用层路径。
- file-backed owner 生命周期、坏图片拒绝和尺寸正确性测试全部切到 `normalizedPNGFile`，继续验证源 owner 在直接解码期间存活、输出 artifact 可解码、源/输出 owner 分别释放后各自清理。
- 保留 `MediaStudioImageLoader.data(for:)` 作为预览及 Base64/HTTPS 兼容输入边界，也保留 `StoryProjectStore` 的旧 `Data` overload 供既有非生产测试和小型兼容调用；本批只移除已经无调用者的“完整归一化 PNG 输出”能力，不扩大行为变更。

验证：Debug 与 arm64 Release 各运行 `MediaStudioImageInteractionTests` 13 项，全部通过；源码检索确认 Media Studio 生产目录与该测试文件均不再出现 `normalizedPNGData` 或 `CGImageDestinationCreateWithData`。Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁；未启动/安装正式 App，未访问活动用户数据库。

边界：Base64/HTTPS 兼容资产在输入阶段仍可能形成最多 20 MiB 的压缩源 `Data`，预览 API 也按调用契约返回 `Data`；本批不把所有小图片 API 一刀切成文件。像素面仍在主进程，Media Worker、真实大图 physical-footprint、内存压力/取消风暴和签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.50 第五十批实施记录：HTTPS 图片消费流式文件化（2026-10-23）

本批继续收紧 12.49 保留的 HTTPS 兼容边界。审计确认 provider/CDN 以 URL 返回图片时，Media History 持久化和“把生成图加入参考图”仍调用 `transport.send`，先在 App 内聚合最多 20 MiB `Data`，随后再写入 History 或输入 artifact；这两条路径的最终消费者本来就是文件，不需要中间完整响应缓冲。

已实施：

- `ChatOSMediaArtifactFileWriter` 新增公开的下载图片入口，复用生产 `ChatOSResponseBodyFileWriter` 的 pull stream、20 MiB 上限、磁盘预算、`.chatos-image-response-<UUID>` lease artifact、取消/失败清理和 24 小时孤儿对账协议。
- `MediaStudioImageLoader.downloadedFile` 只接受无凭据的 HTTPS URL，不发送模型 Authorization；生产 `URLSessionHTTPTransport.stream` 的块直接写入临时 artifact。若响应提供 `Content-Length`，先验证其为 1...20 MiB，并在流结束后要求实际字节精确一致；无长度响应仍由累计写入上限约束。
- Media History 的 URL 图片保存改为“网络流 → lease artifact → `O_NOFOLLOW`/`fstat` 有界复制 → manifest”，不再把完整远端图片响应保存为 `Data`。HTTP 非成功状态继续映射为历史下载失败，磁盘不足等 writer 错误保留原始诊断。
- 生成图片作为通用参考图时，HTTPS 资产同样先流式落 lease artifact，再经现有像素/格式验证复制到 ViewModel 独占的 `0600` 输入文件；复制完成后响应 owner 立即释放。file-backed、Base64 和本地 security-scoped 兼容路径保持原行为。
- 回归 transport 把 payload 拆成两段，并分别记录 `stream` 与 `send` 调用；History 离线恢复和远端参考图测试均确认只走一次 stream、从未调用聚合 `send`、不携带 Authorization，最终文件字节与输入一致。

验证：Debug 与 arm64 Release 各选择性运行 `HTTPTransportTests` 13 项、`Base64PayloadFileWriterTests` 1 项、`MediaStudioHistoryStoreTests` 11 项和 `MediaStudioImageInteractionTests` 14 项，共 39 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。未启动/安装正式 App，未请求真实 provider，未访问活动用户数据库。

边界：全屏/缩略预览仍按 UI 解码契约使用有界 `Data` 或像素面，旧 Base64-only 资产仍需一次源解码；本批只移除最终消费者为文件的 HTTPS 聚合。非 identity `Content-Encoding` 不把传输长度误当解码后长度，仍由 20 MiB 累计写入约束最终字节。URLSession 真实断网/睡眠、响应中途 ENOSPC、CDN 重定向策略、真实 20 MiB physical-footprint 与 Media Worker 仍待完成，P0-E 不能标记完成。

### 12.51 第五十一批实施记录：Story HTTPS 图片归一化源文件化（2026-10-23）

本批继续消除最终消费者为文件的 HTTPS 图片聚合。12.50 已把 History 持久化和“加入参考图”切到 response artifact，但 Story 资源图、首尾帧与 Batch 消费 provider URL 时仍经 `normalizedPNGFile → thumbnail → transport.send` 先形成最多 20 MiB 的完整压缩源 `Data`，随后才交给 ImageIO；这会让网络响应、解码像素面和归一化输出在主进程叠加。

已实施：

- `MediaStudioImageLoader.normalizedPNGFile` 对 HTTPS 资产先复用 12.50 的无凭据 pull stream 下载器，响应块直接进入 lease-owned `.chatos-image-response-*` artifact；随后 `AppImageThumbnailLoader.loadLocalFile` 从文件 URL 直接创建增量 ImageIO source，继续执行 20 MiB、64 MP 输入与 8192 像素归一化边界。
- response owner 覆盖完整的文件打开与 ImageIO 解码周期，归一化 PNG 写入独立 lease artifact 成功或失败后立即释放；正常返回时临时 response artifact 已删除，只把归一化文件 owner 交给 Story Store。
- Base64、已有 file-backed 资产和 UI 全屏/缩略预览保持原契约；本批不为 UI 解码制造额外磁盘往返。
- 新增远端归一化回归：拆分 payload 的 transport 必须只调用一次 `stream`、从不调用聚合 `send`，输出像素尺寸正确，函数返回时下载 artifact 已清理，释放最终 owner 后归一化 artifact 同样消失。

验证：Debug 与 arm64 Release 各选择性运行 `HTTPTransportTests` 13 项、`Base64PayloadFileWriterTests` 1 项、`MediaStudioHistoryStoreTests` 11 项和 `MediaStudioImageInteractionTests` 15 项，共 40 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。未启动/安装正式 App，未请求真实 provider，未访问活动用户数据库。

边界：本批移除的是 Story 最终文件化路径的 HTTPS 压缩源 `Data`，ImageIO 像素解码/PNG 编码仍在主进程；旧 Base64-only 结果仍有一次源解码，UI 预览仍按调用契约返回有界像素面。真实 20 MiB/64 MP physical-footprint、内存压力/取消风暴、恶意图片进程隔离、URLSession 断网/睡眠/ENOSPC、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.52 第五十二批实施记录：HTTPS 图片预览源文件化（2026-10-23）

本批继续收紧远端图片的主进程内存峰值。媒体网格与全屏预览的业务输出是受像素上限约束的 `AppDecodedThumbnail`，但 HTTPS 输入仍经 `MediaStudioImageLoader.data` 聚合完整响应，再用 `CGImageSourceCreateWithData` 解码；当网格加载多张远端结果时，每个共享任务仍会叠加一份压缩源 `Data`。

已实施：

- `MediaStudioImageLoader.thumbnail` 对 HTTPS 资产复用 response artifact 下载器，再调用 `AppImageThumbnailLoader.loadLocalFile` 从安全预打开的 descriptor 创建 ImageIO source；网络响应不再作为完整 `Data` 进入网格/全屏预览路径。
- 下载 owner 覆盖文件解码周期；ImageIO 已生成独立的有界位图后立即释放 owner，临时 response artifact 在 `thumbnail` 返回前删除。现有共享 task pool 与 `NSCache` 继续合并同 key 并发请求并按解码像素成本记账。
- `thumbnail` 增加默认化的临时目录与可用容量注入参数，生产调用不变；归一化的非 HTTPS 分支同步传递同一预算，避免测试/故障注入绕开准入约束。
- 新增远端预览回归：拆分 payload 只能调用一次 `stream`、不得调用 `send`，128×64 输入在 32 像素边界下输出 32×16，返回时临时目录为空。

验证：Debug 与 arm64 Release 各选择性运行 `HTTPTransportTests` 13 项、`Base64PayloadFileWriterTests` 1 项、`MediaStudioHistoryStoreTests` 11 项和 `MediaStudioImageInteractionTests` 16 项，共 41 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。未启动/安装正式 App，未请求真实 provider，未访问活动用户数据库。

边界：`MediaStudioImageLoader.data` 仍保留给确实要求字节值的兼容调用，Agent Avatar 选择器的归一化仍以 `Data` 为输入；旧 Base64-only 预览也仍需一次源解码。位图解码和缓存继续位于主进程，真实多图滚动 physical-footprint、内存压力/取消风暴、恶意图片进程隔离、断网/睡眠/ENOSPC、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.53 第五十三批实施记录：Agent Avatar 文件原生归一化（2026-10-23）

本批关闭 12.52 明确保留的头像选择峰值。用户本地选图与生成记录选图此前都会先形成最多 20 MiB 的完整 `Data`，再创建 ImageIO source、2048 像素解码面和 256×256 JPEG；HTTPS 生成图还会先经历网络响应聚合。

已实施：

- `AgentAvatarImageProcessor.normalizeFile` 复用 `AppImageThumbnailLoader.decodeFile`，以 `O_NOFOLLOW` 打开并对同一 descriptor 执行 `fstat`，只接受 1...20 MiB 普通文件；ImageIO 经 `/dev/fd` 直接解码，继续执行 64 MP 源像素与 2048 解码像素上限。
- 本地 security-scoped 头像选择在授权范围内直接调用文件归一化，不再执行 `AppBoundedFileReader.read`；授权覆盖安全打开、属性读取与完整解码，归一化结束后立即释放。
- 新增 `AgentAvatarImageLoader` 统一生成资产入口：file-backed 资产保持 owner 到解码完成，HTTPS 资产使用 12.50 response artifact 流式下载后文件归一化，只有旧 Base64-only 兼容结果保留有界 `Data` 路径。
- `Data` 与文件入口共享同一个 `AppDecodedThumbnail → 居中裁剪 → 256×256 JPEG` 后半段，避免像素边界和输出策略漂移；输出继续限制在 512 KiB。
- 新增远端生成头像回归：拆分 payload 只允许一次 `stream`、禁止 `send` 且不携带 Authorization，输出为 256×256 有界 JPEG，返回时 response artifact 已清理。

验证：Debug 与 arm64 Release 各选择性运行 `HTTPTransportTests` 13 项、`Base64PayloadFileWriterTests` 1 项、`MediaStudioHistoryStoreTests` 11 项、`MediaStudioImageInteractionTests` 16 项和 `AgentAvatarImageProcessorTests` 4 项，共 45 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。未启动/安装正式 App，未请求真实 provider，未访问活动用户数据库。

边界：旧 Base64-only 头像仍需一次源 `Data`，最终 512 KiB JPEG 按 UI/持久化契约返回 `Data`；ImageIO 解码与 JPEG 编码仍在主进程。本批没有改变头像存储格式或账户模型；真实大图 physical-footprint、内存压力/取消风暴、恶意图片进程隔离、断网/睡眠/ENOSPC、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.54 第五十四批实施记录：聊天附件图片校验与预览文件原生化（2026-10-23）

本批扩展文件原生图片消费到普通聊天附件。此前打开已持久化的本地图片附件时，会先读取最多 20 MiB 完整 `Data`，对该副本计算 SHA-256，再从同一 `Data` 创建最多 2048 像素的 ImageIO 位图；压缩源与解码面会同时驻留。

已实施：

- `ConversationAttachmentImageLoader` 以 `O_NOFOLLOW | O_CLOEXEC` 打开附件并对同一 descriptor 执行 `fstat`，要求普通文件、记录尺寸精确一致且不超过 20 MiB；64 KiB 分块增量计算 SHA-256，不再保留完整压缩源 `Data`。
- `AppImageThumbnailLoader` 提取预打开 descriptor 解码入口；摘要通过后将同一 descriptor rewind，经 `/dev/fd` 创建 ImageIO source，继续执行 64 MP 源像素和 2048 显示像素上限。
- 解码完成后再次检查同一 descriptor 的 device、inode、尺寸与 mtime，检测校验和解码窗口内的替换或改写；取消在摘要分块、解码前后均传播，不能被误报成损坏。
- `LocalAttachmentImage` 直接消费有界位图，不再读取完整附件 `Data`；附件文件与持久化引用格式保持不变。
- 新增 3 项回归，覆盖正确摘要的 128×64 → 64×32 文件解码、摘要不匹配拒绝以及 `O_NOFOLLOW` 拒绝符号链接，失败均不删除原附件。

验证：Debug 与 arm64 Release 各选择性运行既有图片/HTTP 门禁 45 项和 `ConversationAttachmentImageLoaderTests` 3 项，共 48 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。未启动/安装正式 App，未访问活动用户数据库。

边界：本批只优化已持久化聊天附件的图片预览，不改变发送前 `ConversationAttachmentDraft` 的内存 DTO，也不处理 Markdown/文本附件按契约读取正文的路径。ImageIO 仍在主进程；真实 20 MiB/64 MP physical-footprint、快速开关预览取消风暴、进程隔离、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.55 第五十五批实施记录：Agent 群聊持久化图片预览文件原生化（2026-10-23）

本批把 12.54 的附件文件校验与直接解码边界扩展到 Agent 群聊。此前群聊消息中的持久化图片在用户点按预览时仍通过通用附件读取器聚合最多 20 MiB `Data`，然后再由 `AppAsyncDataImage` 创建解码面；已经随当前消息保留在 `dataByID` 的发送期内存图片则不存在重复文件读取问题。

已实施：

- 新增 `AgentMessageAttachmentImageLoader`，要求附件类型为图片、记录尺寸位于 1...20 MiB 且存在 64 字符 SHA-256；随后复用 `ConversationAttachmentImageLoader`，在同一 `O_NOFOLLOW` descriptor 上增量校验摘要并通过 `/dev/fd` 执行 64 MP/2048 像素有界 ImageIO 解码。
- 持久化图片的点按加载不再经过 `AgentMessageAttachmentDataLoader`，预览项直接持有已解码的有界 `NSImage`，打开 sheet 时不再次读取压缩源或重复解码。
- 当前消息已有 `dataByID` 的图片继续走原有内存快速路径；Markdown、另存为及 Agent 执行确实需要字节值的路径仍保留通用有界 `Data` 读取契约。
- 缺少摘要的旧/异常持久化记录明确拒绝图片预览，不以未校验文件换取兼容；Store 的缺失文件恢复仍先校验尺寸与摘要。
- 新增 4 项回归，覆盖正确文件解码且保留源文件、记录尺寸不一致、摘要不一致以及缺少摘要拒绝。

验证：Debug 与 arm64 Release 各选择性运行既有图片/HTTP 门禁 48 项和 `AgentMessageAttachmentImageLoaderTests` 4 项，共 52 项，全部通过；Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。未启动/安装正式 App，未访问活动用户数据库。

边界：发送期 `dataByID` 仍按现有消息 DTO 保留完整压缩图片，Markdown、另存为和 Agent 输入仍按业务契约读取最多 20 MiB 正文；ImageIO 仍位于主进程。本批没有改变附件存储格式或恢复协议；真实群聊大图 physical-footprint、快速反复开关预览、恶意图片隔离、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.56 第五十六批实施记录：Agent 图片热路径去压缩字节缓存（2026-10-23）

本批修正 12.55 之后仍存在的主路径偏差。两个 Agent 聊天 ViewModel 原先会 eager 读取最近最多 24 张图片、累计最多 48 MiB 的完整压缩 `Data` 到 `attachmentDataByID`；因此热消息仍优先绕过 12.55 的文件原生预览入口，并与全局解码缓存中的位图同时驻留。12.55 实际只覆盖缓存外图片，不能作为群聊图片主路径已经文件化的结论。

已实施：

- 将 `AgentAttachmentDataCachePolicy` 替换为 `AgentAttachmentThumbnailCachePolicy`：仍以最近消息、最多 48 MiB 源文件读取计划限制 eager I/O，但内存只保留从持久化文件直接校验并下采样的 `AppDecodedThumbnail`，不再保留压缩源字节。
- 群聊与私聊共享 `AgentMessageAttachmentThumbnailLoader`，逐项复用 12.55 的 size/SHA-256/`O_NOFOLLOW` descriptor 校验和 ImageIO 文件解码；行内缩略图最长边为 1024 像素，缓存同时限制为 16 项和 64 MiB 实际 `bytesPerRow × height` 成本。
- 已缓存缩略图按附件 ID、记录尺寸和摘要保持稳定身份，消息刷新只保留当前计划内且仍符合内存预算的像素面；新加载只消费剩余预算。损坏或缺摘要的旧图片不进入缩略图缓存，但继续以附件 chip 呈现，不能阻断提案、任务板等补充元数据加载。
- 行内图片直接渲染有界 `CGImage`；用户点按时仍从持久化文件按 2048 像素上限重新校验和解码，避免把 1024 像素缩略图冒充全屏预览。
- 删除群聊/私聊重复的 `loadAttachmentData` 整文件读取循环，并把 native timeline 的 row state 从压缩 `Data` 字典改为缩略图字典；Markdown、另存为和 Agent 模型输入所需的字节路径不在本批改变。
- 新增解码内存成本保留回归，并更新性能策略回归验证最近优先、图片类型筛选、源读取预算及缺失项计划。

验证：Debug 与 arm64 Release 各选择性运行既有图片/HTTP 门禁 53 项及 `PerformancePolicyTests` 31 项，共 84 项，全部通过；Swift 6 编译、优化编译、源码检索确认 Agent UI eager 路径不再调用 `AgentMessageAttachmentDataLoader`，`git diff --check` 同步作为本批门禁。未启动/安装正式 App，未访问活动用户数据库。

边界：发送草稿和 Agent 模型输入仍按现有 DTO/模型协议持有图片 `Data`，另存为仍会先读取完整附件；缩略图 ImageIO 解码仍在主进程。64 MiB 是两个 ViewModel 各自的缓存上限，不是整个 App 的统一图片预算；全局资源 Governor、跨页面共享缓存、真实快速滚动/账户切换 physical-footprint、恶意图片进程隔离、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.57 第五十七批实施记录：Agent 附件另存为流式原子导出（2026-10-23）

本批关闭 12.56 明确保留的文件型消费者峰值。Agent 消息附件“另存为”此前会先调用 `AgentMessageAttachmentDataLoader` 把最多 20 MiB 文件完整读入 `Data`，而且在用户确认保存位置之前就开始读取；随后再由 `Data.write(.atomic)` 形成输出文件。最终消费者本来就是磁盘文件，不需要这份完整内存副本。

已实施：

- 保存面板先取得用户目标；确认后才查询消息归属下的持久化 payload，取消面板不触发附件读取。
- 新增 `AgentMessageAttachmentFileExporter`：以 `O_NOFOLLOW | O_CLOEXEC` 打开源文件，要求普通文件、记录尺寸精确一致、范围为 1...20 MiB 且存在 64 字符 SHA-256；使用固定 256 KiB 缓冲分块读取、增量计算摘要并处理短写、`EINTR` 与任务取消，不再构造完整附件 `Data`。
- 导出在目标同目录创建 `0600`、`O_EXCL | O_NOFOLLOW` 临时文件，写入前执行磁盘容量门禁，写完 `fsync`；只有摘要和源 descriptor 的 device/inode/size/mtime 复查全部通过后才以 `rename(2)` 原子替换目标。
- 摘要不符、源符号链接、短读或并发改写会删除临时文件并保留已有目标；源附件始终保留。Markdown 预览仍因文本渲染契约读取有界正文，预览内的“另存为”保存当前已加载文本，不重复读取源文件。
- 新增 3 项回归，覆盖正确导出覆盖已有目标且保留源、摘要不匹配保持旧目标以及拒绝符号链接且无临时文件残留。

验证：Debug 与 arm64 Release 各选择性运行 12.56 的 84 项图片/HTTP/性能门禁及 `AgentMessageAttachmentFileExporterTests` 3 项，共 87 项，全部通过；Swift 6 编译、优化编译、`git diff --check` 和源码检索确认消息附件保存路径不再调用完整 `Data` 写出。未启动/安装正式 App，未访问活动用户数据库。

边界：附件上限当前只有 20 MiB，导出仍在 App 进程内执行文件 I/O；源文件缺少摘要的旧/异常记录明确拒绝导出。Agent Artifact Library 仍通过 service 返回完整文档 `Data` 后另存为，模型输入和 Markdown 正文也仍需要字节值；全局文件 I/O Governor、真实外置卷/只读卷/ENOSPC/睡眠与进程终止矩阵、签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.58 第五十八批实施记录：Agent Artifact Library 文件原生导出（2026-10-23）

本批关闭 12.57 明确保留的 Artifact Library 导出峰值。此前“另存为”会在保存面板出现前让 Host 从文件聚合最多 2 MiB `Vec<u8>`，Base64 放大后写入 framed JSON；App 再解码为 `Data` 并执行原子写出。最终消费者仍是文件，但跨进程和两个进程内都保留了完整正文副本。

已实施：

- 新增账户绑定的只读 `get_artifact_file` Host 命令。存储层只查询当前 owner 与 artifact ID 对应的受控相对文件名、大小和 SHA-256，并以 `symlink_metadata` 要求现有普通文件及记录尺寸一致；文件名必须精确等于 `<artifact_id>.artifact`。
- IPC 只返回 opaque 单文件名和完整性元数据，不返回绝对路径、文件正文或用户选择的目标路径；Swift 仅能在由 Host 数据库 URL 确定的 `.artifacts` 根目录下解析，重复校验 artifact ID、单文件名、2 MiB 上限和摘要格式。不存在 Host 任意路径读取或替用户写任意目标的接口。
- Artifact Library 保存面板现在先取得用户目标，取消不触发 Host 查询或文件读取；确认后才请求文件引用。Markdown 预览仍使用原有有界正文读取，因为文本渲染最终需要 UTF-8 字符串。
- 新增 `AgentArtifactFileExporter`：以 `O_NOFOLLOW | O_CLOEXEC` 打开 Host-owned 源文件，使用固定 256 KiB 缓冲增量校验 SHA-256，目标同目录以 `0600` 临时文件写入，处理短写、`EINTR`、取消、容量门禁与 `fsync`，最后 `rename(2)` 原子替换。源缺失、符号链接、摘要不符或并发改写均保留已有目标并清理临时文件。
- Local Agent Host 协议升级到 v45；Rust、Swift 与 Windows React 的共享 fixture/常量同步迁移，避免旧客户端把新增协议当作兼容 v44 解析。
- 新增回归覆盖 owner 隔离、受控文件引用、路径逃逸拒绝、正确导出、摘要不符保留目标和符号链接拒绝；Windows 6 个受影响契约测试文件共 17 项通过。

验证：Debug 与 arm64 Release 各选择性运行 12.57 的 87 项图片/HTTP/性能门禁，以及 `NativeLocalAgentArtifactClientTests` 2 项、v45 `NativeLocalAgentHostContractFixtureTests` 3 项和 `AgentArtifactFileExporterTests` 3 项，共 95 项，全部通过；Host protocol/runtime/storage 相关 298 项 Rust 测试通过，`cargo fmt --check`、Swift 6 编译、优化编译与 `git diff --check` 同步作为本批门禁。完整 Rust workspace 额外检查只被用户当前未提交的 `plugins/browser/npm/chatos.plugin.json` 与其既有 published-manifest hash 不一致阻断；未修改该目录。未启动/安装正式 App，未访问活动用户数据库。

边界：Artifact 创建与 Markdown 预览仍按现有 2 MiB 文本契约传递 Base64/`Data`；本批只消除“另存为”路径的完整正文跨进程复制。文件引用只适用于 App 与其直接子 Host 共享、由 App 配置的私有存储根，不扩展为网络 IPC 或第三方客户端通用路径能力。真实外置卷/只读卷/ENOSPC/睡眠和进程终止矩阵、跨进程 FD 传递、全局文件 I/O Governor、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.59 第五十九批实施记录：Agent Artifact 缺失附件文件原生恢复（2026-10-23）

本批继续复用 12.58 的账户绑定文件引用，关闭 Agent 消息附件在本地副本丢失后的恢复峰值。此前 Store 会让 Host 读取最多 2 MiB artifact 为 `Vec<u8>`、经 Base64 framed JSON 返回，再在 App 中解码为完整 `Data` 并执行原子写入；因此一次恢复同时产生跨进程编码放大和两端完整正文副本。

已实施：

- `SQLiteAgentGroupChatStore.messageAttachment` 在生产 `AgentArtifactFileServing` 可用时只请求 owner 与 artifact ID 绑定的受控文件引用，不再调用 `read`；不具备文件能力的内存/兼容实现仍保留原有有界 `Data` 后备，不改变协议抽象和既有测试替身。
- 新增 `AgentArtifactFileRestorer`，把恢复 I/O 放到可取消的 utility detached task；严格复验文件引用与消息记录的 size、SHA-256 和 2 MiB 文档上限，以 `O_NOFOLLOW | O_CLOEXEC` 打开 Host-owned 源文件，并要求 descriptor 指向精确尺寸的普通文件。
- 恢复使用固定 256 KiB 缓冲分块读取、增量 SHA-256 和完整短写/`EINTR` 处理；目标目录执行容量门禁，在同目录创建 `0600`、`O_EXCL | O_NOFOLLOW` 临时文件，写完 `fsync`，仅在摘要及源 descriptor 的 device/inode/size/mtime/ctime 前后复查全部通过后才用 `rename(2)` 原子安装。
- Host-owned artifact 源始终保留；摘要不符、引用元数据不符、符号链接、短读、并发改写或取消均不安装附件并清理临时文件。文件型服务失败不会静默降级到完整 `Data` 下载，避免安全校验失败重新引入内存峰值或绕过路径约束。
- 新增回归覆盖文件原生恢复且源文件保留、生产路径不调用 `read`、摘要不符不安装、符号链接拒绝、引用元数据不符拒绝、临时文件清理，以及非文件型服务继续使用兼容后备。

验证：Debug 与 arm64 Release 各选择性运行 12.58 的 95 项图片/HTTP/性能/文件导出/Host 契约门禁及 `AgentArtifactStorageTests` 6 项，共 101 项，全部通过；Swift 6 编译、优化编译、`git diff --check` 与源码审查同步作为本批门禁。测试只使用独立临时目录和受控替身；未启动/安装正式 App，未访问活动用户数据库。

边界：Artifact 创建/上传和 Markdown 正文预览仍按现有 2 MiB 文本契约持有 `Data`；非文件型 `AgentArtifactServing` 的兼容恢复也仍走完整字节值。本批不改变 Host v45 协议，只消费 12.58 已有的只读能力。真实磁盘满/只读卷/睡眠与强杀矩阵、跨进程 FD 传递、全局文件 I/O Governor、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.60 第六十批实施记录：Global Utility 上下文快照主线程去阻塞（2026-10-23）

本批处理主进程与 Global Utility Companion 之间的上下文快照热点。此前项目、联系人或界面语言任一变化都会在主进程 MainActor 上重新映射全部搜索项、JSON 编码并同步 `Data.write(.atomic)`；Companion 收到通知后又在自己的 MainActor 上通过无上限 `Data(contentsOf:)` 读取和 JSON 解码。连续资源刷新会重复执行整套写盘，两个 UI 进程都可能被文件 I/O 和编解码阻塞。

已实施：

- 主进程 Combine 链先对只含不可变 `Sendable` 资源值的输入去重，并以 100 ms debounce 合并同一轮 projects/contacts/language 更新；数组到搜索 DTO 的映射、JSON 编码和文件写入全部移到可取消的 utility detached work，不再占用 MainActor。
- 新增 `GlobalUtilityContextSnapshotPersistence` actor，任一写入进行时只保留最新 pending 输入；相同已持久化内容不重复写盘和通知。停止客户端会丢弃 pending、取消 drain task，完成写盘后才向 Companion 发布 reload 通知。
- 快照建立 4 MiB 明确上限。写入在目标同目录创建 `0600`、`O_EXCL | O_NOFOLLOW` 临时文件，处理短写、`EINTR`、取消和 `fsync`，最后以 `rename(2)` 原子安装；编码失败、超限或取消保留上一份有效快照并清理临时文件。
- Companion reload 改为 latest-task-wins 的异步有界读取和后台 JSON 解码；读取使用 `O_NOFOLLOW | O_CLOEXEC`、要求普通文件且 descriptor 尺寸在 1...4 MiB 内，旧 reload task 会在新通知或进程终止时取消。MainActor 只接收最终的小型值数组并更新上下文。
- 新增回归覆盖私有权限原子 round-trip、超限编码保留旧快照、符号链接/超限文件拒绝，以及写入中多次提交只落首份与最后一份。

验证：Debug 与 arm64 Release 各选择性运行 12.59 的 101 项图片/HTTP/性能/文件恢复门禁及 `GlobalUtilityContextSnapshotStoreTests` 4 项，共 105 项，全部通过；Swift 6 编译、优化编译、`git diff --check` 与源码审查同步作为本批门禁。测试只使用独立临时目录和受控写入探针；未启动/安装正式 App，未访问活动用户数据库。

边界：快照最终消费者仍需要完整 JSON，因此后台任务内仍会持有一次有上限的编码/解码 `Data`；本批不把 Companion IPC 改成增量数据库或共享内存。Distributed Notification、真实万级资源压力、低磁盘/只读卷、睡眠/强杀、全局 I/O Governor、Media Worker 与签名 Release soak 仍待完成，P0-E 不能标记完成。

### 12.61 第六十一批实施记录：本机终端有界输出截断可观测（2026-10-23）

本批修正本机终端短命令执行器的结果语义。执行器原先已经持续排空 stdout/stderr 管道并把每路内存限制在 512 KiB，但超限字节会静默丢弃；即使命令产生 3 MiB 输出，结果仍只有 `success = true` 和一段看似完整的 512 KiB 字符串。调用方无法区分“命令成功且完整”与“命令成功但观测不完整”，容易基于残缺日志作出错误判断。

已实施：

- `LockedDataBuffer` 在持锁追加时同时记录该路是否丢弃过字节，继续保持 stdout/stderr 各 512 KiB 的严格内存上限；结束时以单次锁快照取得文本与截断状态，避免文本和标志来自两个不同瞬间。
- `LocalConnectorTerminalResult` 新增 `stdoutTruncated`、`stderrTruncated` 及聚合 `outputTruncated`。进程 exit code 与 `success` 仍只表达命令是否成功退出，截断标志独立表达结果是否完整，不把成功命令伪装成执行失败。
- 新字段进入 Codable/Relay 结果；自定义解码对旧 payload 缺失字段默认 `false`，避免旧缓存或旧对端结果因模型扩展而无法读取。启动失败、stdin 打开失败、超时和正常结束路径都返回同一组可观测字段。
- 本机终端 UI 在任一路超限时明确显示“仅展示开头 512 KiB”的中英文警告，不再把前缀当作完整输出；原始 stdout/stderr 文本不混入哨兵字符串，现有精确解析调用方仍可按独立字段判断。
- 新增回归覆盖 3 MiB stdout、3 MiB stderr 的独立截断标志、聚合状态、成功 exit 语义、普通小输出不误报，以及旧 JSON 缺字段兼容解码。

验证：Debug 与 arm64 Release 各选择性运行 12.60 的 105 项图片/HTTP/性能/快照门禁及 `NativeTerminalExecutorTests` 8 项，共 113 项，全部通过；Swift 6 编译、优化编译、`git diff --check` 与结果 DTO 源码审查同步作为本批门禁。测试只执行受控临时目录命令；未启动/安装正式 App，未访问活动用户数据库。

边界：本批不增加 512 KiB 保留预算，也不把输出落盘；超限后的尾部内容不可恢复，需要完整大日志的消费者仍应使用长生命周期 Terminal Store 的分页/轮询路径。真实进程树风暴、磁盘日志方案、全局进程并发 Governor、签名 Release soak 与 P0-D/P0-E 综合压力门禁仍待完成。

### 12.62 第六十二批实施记录：Agent Artifact 上传读取脱离 Store actor（2026-10-23）

本批处理 Agent Artifact 创建侧在进入 Host IPC 前的本地文件读取。`claimNextAgentArtifactStorage` 原先在 SQLite Store actor 内同步读取最多 2 MiB 附件并整块计算 SHA-256；协调器每轮最多连续领取 8 份文档，期间同一 actor 上的房间、消息和附件查询都必须等待。该路径还通过 `try?` 吞掉取消，把用户退出/任务取消误记为“本地文档完整性失败”并进入失败退避。

已实施：

- `claimNextAgentArtifactStorage` 改为 async；事务内只完成候选领取和 5 分钟 recoverable lease，随后把文件读取及摘要校验交给可取消的 utility detached task。等待磁盘期间 actor 可重入服务其他数据库请求，不再被同步文件 I/O 占住。
- 新增 `AgentArtifactUploadDataLoader`：以 `O_NOFOLLOW | O_CLOEXEC` 打开来源，要求普通文件、记录尺寸精确一致且不超过 2 MiB；用固定 256 KiB 缓冲分块读取、增量 SHA-256、短读/`EINTR` 处理，并在结束时复查 descriptor 的 device/inode/size/mtime/ctime，拒绝符号链接与并发改写。
- 该阶段最终仍需构造协议要求的有界 `Data`，但不再额外对完整 `Data` 做第二次 SHA 遍历，且读取/哈希均不占 Store actor。后续 Host Base64 IPC 文件化仍作为独立协议阶段处理。
- 取消在进入 detached work 前和每个读取分块都会检查；取消向上传播为 `CancellationError`，不调用 `markAgentArtifactStorageFailed`。已经领取的记录保持 `uploading` lease，超时后可由既有队列安全重领，避免把生命周期取消污染为业务失败及指数退避。
- 新增回归覆盖跨多个分块的正确读取、摘要不符、符号链接拒绝，以及预取消 claim 保持 uploading lease、无 upload error 且 next due 精确指向租约到期。

验证：Debug 与 arm64 Release 各选择性运行 12.61 的 113 项图片/HTTP/性能/快照/终端门禁及本批新增 4 项 Agent Artifact 上传读取回归，共 117 项，全部通过；Swift 6 编译、优化编译及文件读取边界源码审查同步作为本批门禁。测试只使用独立临时目录和测试数据库；不启动/安装正式 App，不访问活动用户数据库。

边界：`AgentArtifactWriteRequest` 与 Host v45 的 `store_artifact` 仍携带最多 2 MiB `Data`/Base64，跨进程放大尚未消除；要继续文件化必须设计 App-owned staging、opaque token、Host 根目录校验、崩溃回收及协议升级，不能把任意绝对路径直接暴露给 Host。全局 I/O Governor、真实慢盘/磁盘故障、签名 Release soak 与 P0-D/P0-E 综合压力门禁仍待完成。

### 12.63 第六十三批实施记录：Agent Artifact 创建文件原生 IPC（2026-10-23）

本批关闭 12.62 保留的 Agent Artifact 创建侧跨进程内存放大。此前最多 2 MiB Markdown 会先在附件 Store 外读取成 Swift `Data`，再编码为约 2.67 MiB Base64 JSON，Host 解码成完整 `Vec<u8>` 后才写入 artifact 文件；同一正文会同时出现在源文件、App 堆、IPC 消息与 Host 堆中。

已实施：

- `AgentArtifactWriteRequest` 改为只持有受控源文件 URL、精确大小、SHA-256 和幂等键；领取队列不再读取正文。`NativeLocalAgentArtifactClient` 在 utility detached task 中以固定 256 KiB 缓冲、`O_NOFOLLOW | O_CLOEXEC`、descriptor 前后身份复验、增量摘要、短读/短写/`EINTR`、取消和 `fsync` 把源文件发布到数据库旁的私有 `.artifact-staging` 根，文件与目录权限分别为 `0600/0700`。
- IPC `create_artifact` 不再携带 `data_base64` 或任意绝对路径，只传 lowercase UUID staging token、大小、SHA-256 与原有 owner/名称/MIME/幂等键。App 在请求成功、失败或取消后都尝试清理 token 文件；Host 进程启动时只回收名称严格匹配且超过 24 小时的普通 staging 文件，未知项和 symlink 保留，避免误删。
- Host 从配置数据库路径独立派生同一个 staging 根；协议层限制 token 语法和 2 MiB 大小，存储层再在该根下构造单文件名，以 `O_NOFOLLOW` 打开并要求普通文件、单硬链接、私有权限、精确大小、稳定 device/inode/size/mtime/ctime。正文使用固定缓冲增量校验 SHA-256 与 UTF-8，不聚合 `Vec<u8>`；校验完成后先移动为 artifact 根内隐藏 import 文件，再在短 SQLite 写事务中原子发布最终文件和元数据。
- command receipt 与 artifact 幂等键都在读取 staging 前预检，并在短事务内再次复验；响应丢失重放不要求已被消费的 token 仍存在，并发同幂等键只发布一个 artifact。校验失败、事务失败和竞争输家会清理隐藏 import，数据库不会引用未验证或半发布文件。
- Local Agent Host 协议升级到 v46；Rust、Swift 与 Windows React 的常量、共享 fixture 和契约测试同步迁移。新增回归覆盖多分块 staging、权限、摘要失败无残留、symlink 拒绝、IPC 不含 Base64、Host 摘要/symlink 双重拒绝、逻辑幂等重放，以及 24 小时崩溃遗留只清理受控名称。

验证：Debug 与 arm64 Release 各运行 `AgentArtifactStorageTests` 7 项、`NativeLocalAgentArtifactClientTests` 2 项、v46 Host contract 3 项和 `AgentArtifactUploadStagerTests` 3 项，共 15 项，全部通过；Host protocol/runtime/storage/process 全量 428 项 Rust 测试通过，Windows 6 个受影响契约测试文件共 17 项通过。Swift 6 编译、优化编译、`cargo fmt --check`、TypeScript 类型检查与 `git diff --check` 同步作为本批门禁。测试只使用独立临时目录和内存/临时数据库；不启动/安装正式 App，不访问活动用户数据库。

边界：本批只文件化 Agent Artifact 创建；Markdown 预览的最终消费者仍需要有界字符串，非文件型兼容读取仍可能返回完整 `Data`。staging 与 Host artifact 根必须位于同一数据库派生位置并由同一登录用户访问，不扩展为网络客户端上传协议；跨进程 FD 传递、全局文件 I/O Governor、真实 ENOSPC/只读卷/睡眠与强杀矩阵、签名 Release soak 和 P0-D/P0-E 综合压力门禁仍待完成。

### 12.64 第六十四批实施记录：Agent 文本附件有界前缀读取（2026-10-23）

本批处理 Agent 模型请求组装中的普通文件/音频附件读取。此前 Chat Completions 与 Responses 两个适配器都会先把每份附件完整读取为最多 20 MiB `Data`，即使非图片、非 PDF 最终只检查并使用前 512 KiB 文本；多附件请求会形成与原文件大小线性增长的无效读取和短时堆占用。

已实施：

- `AgentAttachmentDataLoader` 新增 512 KiB 文本预览入口；普通文件与音频只读取该前缀，并从 descriptor 已知的原始大小判断是否添加既有截断提示，不再为了判断 `data.count` 读取剩余最多 19.5 MiB。
- 文件访问统一改为 `O_NOFOLLOW | O_CLOEXEC` 与 `fstat`：只接受 1 字节—20 MiB 的单链接普通文件，循环处理短读、`EINTR` 和任务取消，读取后复验 device/inode/link count/size/mtime/ctime，拒绝 symlink、读取期间替换或变化的输入。
- 图片与 Responses PDF 仍保留完整有界读取和现有 data URL wire 语义；普通二进制文件仍只向模型提供 Relay 读取提示，不把二进制塞入文本上下文。两个模型适配器对文本包装、截断提示和 fallback 的外部行为不变。

验证：Debug 与 arm64 Release 各运行 `AgentChatModelClientTests` 14 项和 `AgentAttachmentDataLoaderTests` 4 项，共 18 项，全部通过。新增回归覆盖大文本只返回固定前缀且标记截断、两个模型适配器都不生成 data URL、symlink 双入口拒绝，以及原有 20 MiB 上限；Swift 6 Debug/Release 编译和 `git diff --check` 通过。测试只使用临时文件，不启动/安装正式 App，不访问活动用户数据库。

边界：本批消除的是普通文本/音频附件的无效尾部读取，不是 Agent 模型请求的完全文件原生化。图片和 PDF 仍会形成完整 `Data`、Base64 字符串与 JSON body，多个 20 MiB 输入仍可能叠加；后续需把 Agent 请求改为增量 Base64 临时 body/真正的 file upload，并覆盖 SSE 上传、取消、重定向、磁盘预算和崩溃回收。512 KiB 文本字符串是模型上下文的必要有界消费者，不计为可消除的文件正文副本；全局文件 I/O Governor 和组合压力门禁仍未完成。

### 12.65 第六十五批实施记录：Agent 二进制附件请求文件原生化（2026-10-23）

本批关闭 12.64 保留的 Agent 图片与 Responses PDF 请求组装峰值。此前每份最多 20 MiB 的二进制附件会先完整读取为 `Data`，再生成约 4/3 大小的 Base64 `String`，最后与整份 JSON `Data` 同时驻留；多附件请求会把这些副本叠加在主进程堆中，SSE 路径也没有可直接上传请求文件的入口。

已实施：

- Chat Completions 图片及 Responses 图片/PDF 先在结构化 payload 中写入不可预测的单次 placeholder；`AgentModelRequestBodyFileWriter` 再以固定 48 KiB（可被 3 整除）分块读取源文件、增量 Base64 并直接写出请求文件，不再构造完整附件 `Data`、完整 Base64 `String` 或最终 JSON `Data`。单源继续限制 20 MiB，编码后的整份请求硬限制 64 MiB，并在创建文件前完成溢出安全的精确尺寸预算。
- 源文件以 `O_NOFOLLOW | O_CLOEXEC` 打开，只接受当前用户所有、单硬链接、1 字节—20 MiB 的普通文件；读写循环处理短读/短写、`EINTR` 与任务取消，结束时复验 device/inode/link/size/mtime/ctime，拒绝 symlink、并发替换与读取期间改写。
- 请求文件进入系统临时根下私有 `.chatos-agent-model-requests` 目录，目录/文件权限固定为 `0700/0600`，写完执行 `fsync`；发送成功、错误或取消均通过共享 cleanup owner 幂等删除。启动新写入时最多检查 64 个严格 UUID 文件名，只回收超过 24 小时且仍为当前用户单链接普通文件的崩溃遗留，未知文件和 symlink 不删除。
- 非流式生产路径使用 `BoundedURLSessionDataLoader.upload(fromFile:)`，响应继续受 2 MiB 上限；SSE 生产路径使用私有 `URLSessionUploadTask(fromFile:)`，不会把请求体重新物化。响应桥接采用 16 KiB 分片、256 KiB 高水位/64 KiB 低水位暂停恢复 URLSession task，并设 2 MiB 待消费硬上限；消费者取消或提前释放响应流会取消 task、结束私有 session，不静默丢弃 SSE 字节。
- 为保持现有单元测试和自定义 provider transport 的注入接口兼容，只有显式注入的兼容 transport 会映射并物化临时请求文件；默认生产 transport 始终走 file upload。没有二进制附件的纯文本请求继续沿用原内存 JSON 路径，避免为小请求引入磁盘往返。

验证：Debug 与原生 arm64 Release 各选择性运行 `AgentChatModelClientTests` 15 项、`AgentURLSessionFileUploadTests` 2 项、`AgentModelRequestBodyFileWriterTests` 4 项及 `AgentAttachmentDataLoaderTests` 4 项，共 25 项，全部通过。新增回归覆盖多附件增量替换、64 MiB 编码预算前置拒绝且零残留、权限与 24 小时保守回收、symlink 拒绝、普通与 SSE 的真实文件上传、512 KiB 快速响应在有界背压下字节无损，以及取消响应消费会停止底层请求；Swift 6 Debug/Release 编译、优化编译与 `git diff --check` 同步作为门禁。测试只使用独立临时文件和受控 URLProtocol，不启动/安装正式 App，不请求真实 provider，不访问活动用户数据库。

边界：本批限制的是 Agent 模型请求附件和 SSE 待消费队列，不替代全局文件 I/O Governor；最多 64 MiB 请求文件仍会占用临时磁盘，容量检查不是空间预留。真实 ENOSPC/只读卷、provider 重定向或认证挑战重放、SIGKILL 各写入/上传窗口、网络切换与签名 Release soak 尚未完成，P0-D/P0-E 组合压力门禁仍不能标记完成。

### 12.66 第六十六批实施记录：大文件网络传输共享准入（2026-10-23）

本批关闭第 6.3 节“大文件上传/下载总并发 2”在主进程生产网络路径中的首个缺口。此前 Media 文件上传、视频下载和 Agent 模型请求文件上传各自直接启动；即使单条链路已经流式化，组合负载仍可能同时占用网络、文件 I/O、临时磁盘与内存带宽。

已实施：

- 在公共 `ChatOSNetworking` 增加进程内共享 `ClientLargeTransferGovernor`，默认并发 2、FIFO 等待、等待队列硬上限 256；队列满时停止新准入。permit 记录 task ID、可选 owner session ID 和上传/下载方向，但不认领或改写 durable job 状态。
- 等待任务取消会从 FIFO 队列移除；取消与 permit 移交并发时，已移交 permit 会同步归还。permit 的显式释放与析构释放均幂等，避免错误、取消或调用方提前退出泄漏容量。
- `URLSessionHTTPTransport` 的生产 file-backed upload 在网络 I/O 前统一取得上传 permit。当前 Media multipart/视频请求均走带响应上限的 delegate 上传，在 `didSendBodyData` 确认正文全部发出后即可释放；没有发送完成回调、错误或取消时由 completion/defer 兜底。
- `AgentModelRequestBodyTransport` 的普通响应与 SSE 文件上传接入同一共享 Governor。SSE loader 同样在请求正文发完时提前释放，响应侧既有 256 KiB/2 MiB 有界背压继续独立生效；Agent 当前没有可安全透传的账户 session 上下文，因此只登记每次请求生成的 task ID，不伪造 owner。
- `URLSessionHTTPTransport.stream` 仅把声明响应上限大于 2 MiB 的流视为大下载；Media 视频内容请求登记稳定 `video:<provider-job-id>:content` task ID，并把绑定后的认证 session UUID 带入 transfer context。permit 直接由既有 pull-based `URLSessionChunkStream.Reader` 持有，在 EOF、错误、取消或 reader 析构时释放，不新增会积压或丢字节的 push queue。
- 共享准入只放在默认生产 URLSession transport。显式注入的测试/兼容 transport 仍可物化请求文件，以保持协议和测试替身兼容；它们不代表生产 Governor 已被绕过，也不能用来证明真实网络组合压力。

验证：Debug 与原生 arm64 Release 选择性运行 `AgentChatModelClientTests` 15 项、`AgentURLSessionFileUploadTests` 2 项、`ChatOSMediaGenerationServiceTests` 32 项、`ClientLargeTransferGovernorTests` 2 项和 `HTTPTransportTests` 14 项，共 65 项，全部通过。新增回归覆盖 FIFO、排队取消、队列满 fail-closed、共享上传/下载额度、下载消费完成释放、Media 账户上下文和稳定视频 task ID；Swift 6 编译、优化编译与 `git diff --check` 同步作为门禁。测试只使用临时文件和受控 URLProtocol/transport，不启动或安装正式 App，不请求真实 provider，不访问活动用户数据库。

边界：这是主进程内 Media/Agent 大网络传输的静态总额度，不是第 6.3 节完整动态 `ResourceGovernor`。Media 重解码的同时 1 个额度、Host/Swift Agent 逻辑 job 动态共享、Plugin、数据库维护和通用文件 I/O 尚未统一；也未实现账户公平、排队老化、前台交互降额、热状态/低电量迟滞、跨进程 lease/断连回收或诊断指标。Agent owner/session 尚未接入，未来 Media Worker 也不能复制一份独立“全局 2”。真实 Media + Agent 组合压力、认证挑战/重定向重放、网络切换、取消风暴、固定 Release trace 与长时间 soak 仍待验收，因此 P0-D/P0-E 和客户端总体 SLO 均不能标记完成。

### 12.67 第六十七批实施记录：重媒体解码共享准入（2026-10-23）

本批关闭第 6.3 节“媒体重解码/帧提取同时 1 个”在 Media Studio 主进程生产路径中的首个缺口。此前图片归一化、大尺寸预览和 Story 视频末帧提取可以并行执行；即使各入口已有源文件大小、像素数或任务取消边界，多个解码工作集和 PNG 编码仍可能在主进程内叠加。

已实施：

- 在公共 `ChatOSNetworking` 增加独立的进程内共享 `ClientHeavyMediaWorkGovernor`，默认并发 1、FIFO 等待、等待队列硬上限 256；它与大网络传输额度分离，permit 记录任务 ID、可选 owner session ID、图片解码/视频帧提取类别和预计解码字节，但不认领或改写 durable job/lease。
- 单任务预计解码工作集默认最多 256 MiB；零值、负值和超预算请求在解码前 fail-closed。等待取消会从队列移除，取消与 permit 移交竞争时已移交额度同步归还；成功、错误和显式/析构释放均幂等，避免泄漏单一通道。
- Media Studio 图片归一化和边长请求大于 2,048 像素的大预览接入共享额度。归一化 permit 覆盖有界解码及 PNG 写出，防止上一份大解码图仍参与编码时放行下一份；远程资源继续先按既有流式、磁盘预算路径下载到临时文件，再等待解码 permit，避免网络等待占住重媒体通道。
- 边长不超过 2,048 像素的缓存缩略图继续使用既有 64,000,000 源像素门禁和有界 downsample，不进入重媒体队列，以免后台归一化阻塞列表与预览交互。归一化最大边长仍为 8,192，按 `min(64,000,000, target²) × 4` 估算，单任务最大估算为 256,000,000 字节；本批通过串行化防止多任务峰值叠加，不宣称降低了该单任务峰值。
- Story 视频末帧提取接入同一额度，生产调用使用 provider job ID 形成稳定 task ID；可取得认证账户时同时携带 session UUID。`AVAssetImageGenerator` 输出最大限制为 4,096 × 4,096，生成后再次校验宽高，按 64 MiB 预计工作集准入，再执行 PNG 编码。

验证：Debug 与原生 arm64 Release 各选择性运行 `MediaStudioImageInteractionTests` 19 项及 `ClientHeavyMediaWorkGovernorTests` 2 项，共 21 项，全部通过。新增回归覆盖 FIFO、排队取消、queue full、超估算预算拒绝、operation 错误后的同步释放、图片归一化串行化、小缩略图不等待重媒体通道，以及视频帧提取携带稳定 task/session context 并在解码错误后回收 permit；Swift 6 Debug/Release 编译、优化编译与 `git diff --check` 同步作为门禁。测试只使用独立临时目录、生成的小图片及故意无效的视频输入；不启动或安装正式 App，不访问活动用户数据库。

边界：这仍是主进程内 Media Studio 部分重媒体路径的静态额度，不是整个 App 进程组的动态/跨进程 `ResourceGovernor`。普通 UI 图片解码、Plugin 图片归一化、Local Agent 图片模型输入、粘贴图片归一化和其他 AVFoundation 消费者尚未接入；未来 Media Worker 不能复制一份独立“全局 1”，仍需跨进程 lease、断连回收和 fencing 协议。尚未实现账户公平、排队老化、前台交互降额、热状态/低电量迟滞或诊断指标，也没有用真实超大图片、有效 4K/8K 视频、固定 Release trace、组合负载与长时间 soak 验证峰值和切页 SLO，因此 P0-D/P0-E 和客户端总体 SLO 均不能标记完成。

### 12.68 第六十八批实施记录：模型与粘贴图片共享重解码额度（2026-10-23）

本批扩展 12.67 的主进程重媒体准入，关闭 Plugin 图片结果、Local Agent 图片模型输入和聊天粘贴图片三条 4K 解码链路仍可与 Media Studio/Story 并行的缺口。三者已有 20 MiB、64 MP、4,096 px 等单任务边界，但此前各自创建 detached 解码任务，组合负载仍会叠加像素面和 PNG/JPEG 编码工作集。

已实施：

- Plugin 返回的 PNG 图片块在 Base64 解码、ImageIO downsample 和 JPEG 编码前取得同一个 `ClientHeavyMediaWorkGovernor.shared` permit；一次工具结果最多 8 个图片块并在一个 permit 内顺序处理，按 4,096² × 4，即 64 MiB 预计工作集准入。task ID 使用稳定 plugin call ID；不含待归一化 PNG 的纯文本/JPEG/WebP 结果直接返回，不占重媒体通道。
- Local Agent 附件向模型提供图片前接入同一 permit，既有 20 MiB 输入、64 MP 源像素、4,096 px downsample、2 MiB 模型输出和有效解码校验不变。task ID 由 run ID 与 attachment ID 组成，避免同一 run 的多个图片无法区分；permit 覆盖 ImageIO 解码、必要的 JPEG 重编码及模型输入结果组装。
- 聊天粘贴的非 JPEG/PNG/WebP 图片归一化接入同一 permit，使用本次粘贴 UUID 作为 task ID；预计工作集按 `min(sourcePixelLimit, decodePixelSize²) × 4` 计算，并对无效限制及整数乘法溢出 fail-closed。可直接上传的 JPEG/PNG/WebP 继续绕过归一化，不为无需解码的粘贴引入排队。
- 三条路径继续在 detached task 中执行实际 ImageIO/编码工作，排队与执行取消沿用 12.67 的同步 permit 回收。当前调用链没有可证明等价于认证 session epoch 的值，因此不把 owner user ID、聊天 session ID 或 workspace ID 冒充 `ownerSessionID`；上下文保留为空，后续随统一账户 ResourceGovernor 契约接入。

验证：Debug 与原生 arm64 Release 各选择性运行 `NativePluginModelImageNormalizerTests` 6 项、`NativeLocalAgentAttachmentImageTests` 7 项、`AppPastedImageNormalizerTests` 6 项、`MediaStudioImageInteractionTests` 19 项和 `ClientHeavyMediaWorkGovernorTests` 2 项，共 40 项，全部通过。新增回归使用同一 Governor blocker 验证三条解码路径真实排队、task ID/类别/64 MiB 或动态估算上下文正确、完成后 permit 回收，以及 Plugin JPEG 直通不等待重媒体通道；Swift 6 Debug/Release 编译、优化编译与 `git diff --check` 同步作为门禁。测试只使用内存生成的小图片、独立临时目录和受控替身；不启动或安装正式 App，不访问活动用户数据库。

边界：本批控制的是像素解码/编码工作集，不倒退为完整文件 I/O Governor；Local Agent 已验证的附件 `Data` 和 Plugin 已返回的 Base64 字符串仍可能在等待 permit 时驻留。项目文件 4K 预览和其他普通 UI 图片消费者仍未统一接入；头像与常规缩略图目前限制在 2,048 px 或更小，为保护交互延迟继续不占重媒体通道。Plugin 派生进程自身的解码不受主进程 permit 控制，Media Worker 也尚未建立；跨进程 lease/fencing、账户公平、排队老化、热状态/低电量迟滞、真实多源 4K/64 MP 组合压力、固定 Release trace 与长时间 soak 仍待完成，因此 P0-D/P0-E 和客户端总体 SLO 均不能标记完成。

### 12.69 第六十九批实施记录：项目文件 4K 预览共享重解码额度（2026-10-23）

本批关闭 12.68 保留的项目文件图片预览缺口。项目编辑器允许最多约 20 MiB 原始或 28 MiB Base64 内容，并以 64 MP 源像素门禁解码到最长边 4,096 px；此前该 detached 解码会绕过 Media/Plugin/Agent 共用的单额度，在用户切换文件时与后台重媒体任务叠加。

已实施：

- 抽出可测试的 `ProjectImagePreviewLoader`，将 Base64/文本图片的有界物化、ImageIO 源校验和 4K downsample 整体置于 `ClientHeavyMediaWorkGovernor.shared` permit 内；按 4,096² × 4，即 64 MiB 预计解码工作集准入，既有 28 MiB Base64 字符串、20 MiB 原始内容和 64 MP 源像素限制保持不变。
- SwiftUI 文件预览任务在进入 loader 前生成同进程稳定的 opaque task ID：哈希输入包含项目内路径、文件大小和修改时间，但 context 不保留或暴露真实路径。切换文件时既有 `.task(id:)` 取消继续向 Governor 排队和 detached 解码传播，permit 由统一取消/错误路径同步回收。
- 头像、Markdown 图片、消息缩略图和 Media Studio 不超过 2,048 px 的缓存缩略图继续走低延迟有界旁路；本批只接入明确请求 4K 像素面的项目预览，避免单一后台归一化把所有小型 UI 图片排队。

验证：Debug 与原生 arm64 Release 各选择性运行 `ProjectImagePreviewLoaderTests` 2 项和 `ClientHeavyMediaWorkGovernorTests` 2 项，共 4 项，全部通过。新增回归验证项目预览在共享通道被占用时真实排队、64 MiB context 正确、释放后可解码且 permit 无泄漏，并确认同一文件身份得到相同 opaque task ID 且不包含路径正文；Swift 6 Debug/Release 编译、优化编译与 `git diff --check` 同步作为门禁。测试只使用内存生成的小图片，不启动或安装正式 App，不访问活动用户数据库。

边界：项目文件内容在上游 IPC/DTO 中仍是字符串，Base64 解码会在取得 permit 后形成压缩源 `Data`；本批限制像素工作集并发，不解决项目文件内容的文件原生化。2K 及以下 UI 图片仍可彼此并行，其上限和缓存预算继续独立生效；截图合成等已持有 `CGImage` 的路径不属于本批压缩媒体解码门禁。Plugin 派生进程、未来 Media Worker 和其他 Helper 仍不受这份主进程静态额度控制，跨进程 lease/fencing、账户公平、前台权重、热状态/低电量迟滞、真实项目大图切换 trace 与长时间 soak 尚未完成，因此 P0-D/P0-E 和客户端总体 SLO 均不能标记完成。

### 12.70 第七十批实施记录：共享资源 Governor 进程内诊断指标（2026-10-23）

本批补齐 12.66–12.69 两类共享准入的首组可观测信号。此前 snapshot 只能看到当前 active/queued context，无法区分正常排队、容量拒绝和取消，也不能量化排队时间与进程生命周期内的峰值，组合压力问题仍缺少可核对证据。

已实施：

- `ClientLargeTransferGovernor` 与 `ClientHeavyMediaWorkGovernor` 的 snapshot 增加当前 `totalInFlight`，并提供进程内累计的成功准入、排队后准入、真实 permit 释放、排队取消、queue full 拒绝、总/最大排队纳秒、最大 active 和最大 queued。重媒体 Governor 另外区分非法估算与单任务超工作集预算拒绝，避免把输入错误和容量压力混为一类。
- waiter 在进入 FIFO 时记录单调 uptime，只有真正从队列移交 permit 才累计等待时间；立即准入不伪造排队样本。释放计数只在 active permit 首次真实移除时增加，重复显式释放或析构不会重复计数；取消与移交竞争继续沿用既有 actor 串行语义。
- 长期累计的 `UInt64` 计数和等待时间采用饱和加法，避免极长进程生命周期发生回绕。指标只保留数值聚合，不复制 task ID、路径、owner session 或媒体正文；现有 snapshot 中用于实时诊断的 active/queued context 语义不变。

验证：Debug 与原生 arm64 Release 各选择性运行 `ClientLargeTransferGovernorTests` 4 项及 `ClientHeavyMediaWorkGovernorTests` 4 项，共 8 项，全部通过。新增回归覆盖立即与排队准入、非零排队时间、当前总 in-flight、active/queued 峰值、幂等释放、排队取消、queue full，以及重媒体非法估算和超预算的独立拒绝计数；Swift 6 Debug/Release 编译、优化编译与 `git diff --check` 同步作为门禁。不启动或安装正式 App，不访问活动用户数据库。

边界：这些指标只存在于各自主进程 actor 的内存中，进程重启即清零；本批没有持久化、OS signpost、诊断 UI、采样导出、账户维度或跨进程聚合。它们也不提供动态调额、前台权重、排队老化、热状态/低电量迟滞、Helper lease/断连回收或 fencing，因此不能把第 6.3 节动态/跨进程 `ResourceGovernor`、P0-D/P0-E 或客户端总体 SLO 标记完成。

### 12.71 第七十一批实施记录：Swift Agent 执行准入有界排队与诊断快照（2026-10-23）

本批继续收紧 12.12 的 Swift Agent 逻辑执行额度。此前 `LocalAgentExecutionGovernor` 虽限制 active execution，但来自不同账户、窗口、communication 和 recovery drain 的 `acquire()` waiter 可无界增长；大量并发 drain 不会提前 claim durable delivery，却仍可积累 Task、continuation 和调度压力，也没有办法从 Scheduler 核对实际使用与拒绝原因。

已实施：

- `LocalAgentExecutionGovernor` 增加默认 256 的等待队列硬上限；active 已满且队列也满时立即 fail-closed 返回，不创建 continuation。上层 drain 将其视为本轮没有执行额度并停止继续 claim，durable queue 保持权威且任务留待后续 drain，不把容量压力误写成业务失败。
- 增加只读 `LocalAgentExecutionGovernorSnapshot`，由 `LocalAgentGroupChatScheduler.executionGovernorSnapshot()` 暴露 limit、队列上限、当前 active/queued/总 in-flight，以及成功准入、排队后准入、真实释放、排队取消、queue full、`tryAcquire` 即时容量不足、总/最大排队时间和 active/queued 峰值。累计值使用饱和加法，permit 重复释放不重复计数。
- waiter 只在真正进入 FIFO 时记录单调 uptime，移交 permit 时才累计等待；release 向 waiter 直接转交既有 active slot，不制造 active 先降后升的虚假采样。snapshot 不保存 owner、消息、Run 或 prompt 正文。

验证：Debug 与原生 arm64 Release 各选择性运行 `LocalAgentGroupChatSchedulerTests` 20 项及 `NativeClientResourceBudgetPolicyTests` 6 项，共 26 项，全部通过。新增回归覆盖 queue full fail-closed、即时容量拒绝、排队取消、非零等待、active/queued 峰值、总 in-flight 和幂等释放；原调度回归继续验证预算 2 时执行峰值精确为 2、claim 不超额、跨入口共享、communication fast lane、恢复与取消语义。Swift 6 Debug/Release 编译、优化编译与 `git diff --check` 同步作为门禁；不启动或安装正式 App，不访问活动用户数据库。

边界：这是 Swift Agent 主进程内、claim 前的有界等待，不是 Host + Swift 的动态共享 permit；Host 额度仍在启动时静态切分，独立 Host 进程的 in-flight 也未汇入该 snapshot。由于 permit 在 durable claim 前取得，本层没有可登记的 delivery/task ID；账户公平、排队老化、前台降额、热状态/低电量迟滞、手动诊断 UI/导出、跨进程 lease 与 fencing 仍未完成，不能据此把第 6.3 节、P0-D/P0-E 或客户端总体 SLO 标记完成。

### 12.72 第一完整里程碑：迁移恢复空间有界与成功后收敛（2026-10-23）

本里程碑关闭实际大库迁移反复启动时恢复副本持续占盘的问题。现场样本的单库约 2.7 GB；Swift 生命周期侧已在 `3f9c49379` 串行化同账户启动、等待旧 Host 释放数据库锁并将大库启动窗口延长至 10 分钟。本批补齐 Rust 存储侧的独立硬门禁和成功后保留协议，使任一侧再次异常都不会只依赖固定 8 份的数量上限。

已实施：

- 迁移预检除最多 8 个恢复文件外，新增恢复文件逻辑总字节门禁。预算为“当前数据库逻辑大小的两倍”和 4 GiB 中的较大值；没有恢复点时始终允许创建第一份数据库大小的安全快照，已有恢复点时则在创建前计算预期总量并 fail-closed。计数、加法和预算乘法均检查溢出；匹配恢复文件名的 symlink 或非普通文件直接拒绝迁移。
- 迁移、迁移后完整性校验及必要压缩全部成功后，保留当次新发布且已验证的快照；若本次从 v35/v36 等中间版本恢复，额外保留一份最低旧 Schema 的 v34 回滚点。相同起始版本的重复完整快照、已被替代的中间快照、`.incomplete` 和对应 journal 才会回收，并同步父目录。失败或强杀路径不执行清理。
- 无法识别命名版本的完整恢复文件、手工备份、目录和 symlink 永不自动删除；清理为成功后的 best-effort hygiene，删除失败会保留文件，并由下次迁移的数量/字节门禁阻止继续放大。当前 Schema 的普通重开不扫描删除历史恢复点。

验证：存储包 Debug 与 Release 各 202 项全量通过；13 个迁移强杀场景覆盖备份占位、写完未校验、发布后、v35/v36/v37 提交前后及三类回填中断，确认失败时源库和恢复点不变，成功重试后无 incomplete、同版本副本收敛，同时 v35/v36 恢复仍保留一份 v34 回滚点。`chatos_client_storage --all-targets -D warnings` 严格 Clippy、全仓 Rust 格式和 `git diff --check` 通过。

边界：本里程碑治理的是迁移恢复文件的生产空间上界与保留策略，不自动处理被人工移到任意子目录或改名的历史文件，也不替代在线维护 lease、旧 Host 跨版本交接、真实 ENOSPC/断电、数据库物理缩容和正式 Release 大库耗时门禁。已完成 v37 的普通启动不会为清理历史快照重新进入迁移；既有重复文件需经过明确人工核对后清理。P0-C 的生产迁移门禁仍未全部关闭。

### 12.73 第二完整里程碑：Media Worker 图片归一化垂直切片（2026-10-23）

本里程碑开始落实 P2，但只把 Story 最终文件化路径中的 PNG 归一化作为第一条完整生产切片。此前 12.47 已消除最终 PNG `Data`，像素解码和编码仍留在主 App；本批将 ImageIO 解码、downsample、PNG 编码、输出同步和 SHA-256 全部移入独立 `ChatOSMediaWorker`，主进程只准备/持有受限文件 artifact 和小型 descriptor。

已实施：

- 新增独立协议、Core 与 executable target。wire v1 先 hello 协商版本、`media-worker-v1` capability 和 512 KiB inline 上限，再处理一个 `normalize_png_v1` 请求；媒体正文不进入 JSON。传输冻结为 App 直接创建的单用途子进程和 inherited stdin/stdout，理由、XPC/socket 取舍、安全边界及回滚见 `clients/macos/docs/23-media-worker-transport-adr.md`。
- App 预创建既有 mode `0700/0600`、lease-owned 的输出 artifact；Worker 对输入/输出使用 `O_NOFOLLOW`，打开后复核 device/inode/size、普通文件、owner、单链接空输出，并拒绝 symlink、路径替换和输入输出同 inode。输入最多 20 MiB/64 MP，输出长边最多 8192 px/20 MiB。
- Worker 不联网、不接收 provider secret、不打开业务数据库或修改 Story 元数据。Base64 兼容输入先落入已有有界 image-response artifact，HTTPS 继续流式落盘；像素面和 PNG 编码缓冲不再进入主 App。
- Worker 要求直接父 PID 与 `getppid()` 一致，并监听父进程退出。App 取消/超时先 `SIGTERM`，250 ms 后 `SIGKILL`；异常新启动采用 1/2/4/8/16/30 秒退避，5 分钟内连续 5 次异常时隔离 30 秒。invalid-image 业务拒绝不计为进程崩溃，失败请求不自动重放。
- 打包脚本独立构建 Worker、纳入 Release dSYM/build evidence，复制到 `Contents/MacOS` 并从内到外签名；验证脚本检查存在性、arm64-only、Release 路径、嵌套签名、bundle 深度签名及无父能力启动失败。

验证：Debug 与原生 arm64 Release 各 28 项通过。Worker Core 4 项覆盖真实 PNG 输出、symlink、输出 inode 替换和 deadline；真实子进程 5 项覆盖握手/归一化、Story lease artifact 整条链路、取消、超时，以及父进程被 `SIGKILL` 且 stdin 仍由旁路进程保持打开时 Worker 自行退出；Media Studio 图片回归 19 项覆盖文件/Base64/HTTPS、Governor、invalid image 和 lease 清理。Debug/Release 的 Worker 与主 App product 均完成优化编译，Worker 与主 App 的 warnings-as-errors 构建通过；打包/证据脚本回归 25 项、shell 语法和 `git diff --check` 通过。未提交源码的原生 arm64 Release 诊断包完成全量组装，`ChatOSMediaWorker` 位于 `Contents/MacOS`，App/Worker 嵌套 ad-hoc 签名、arm64-only、Release 路径、四份 dSYM UUID、bundle 内容哈希及 build-evidence 全部验证通过；未执行 App 启动 smoke，也不把该 dirty diagnostic build 作为 Developer ID 分发验收。

边界：这不是 P2 全部完成。缩略图、聊天附件预览、视频 frame/poster、已文件化上传/下载和 durable media job 仍未移交 Worker；普通子进程提供崩溃/内存隔离但不是 App Sandbox。真实 20 MiB/64 MP physical-footprint、恶意解码崩溃、ENOSPC、睡眠/断网、账户切换、连续崩溃 soak、Developer ID/hardened runtime/notarization 及完整 Release SLO 仍是后续门禁，不能据此宣称 P0-E/P2/P4 或客户端总体性能优化已经完成。

### 12.74 第三完整里程碑：缩略图与聊天附件预览迁入 Media Worker（2026-10-23）

本里程碑关闭 12.45、12.54、12.56 和 12.73 明确保留的 UI 原始图片解码边界。Media Studio 卡片/Story 缩略图/全屏预览、普通聊天持久图片，以及 Agent 群聊/私聊缩略图与点按大图不再由主 App 直接用 ImageIO 解析原始文件；原始 20 MiB/64 MP 输入统一先交给单用途 `ChatOSMediaWorker` 下采样并编码成受限 PNG，App 只为最终显示解码该受限输出。

已实施：

- 复用已冻结的 `normalize_png_v1`、hello、文件身份、deadline、取消/强杀和 lease artifact 协议，不新增平行 IPC。Base64 和 HTTPS 资产先进入既有有界临时文件；Worker 输出在 App 成功构造有界 `CGImage` 后立即随 lease 释放，不形成长期重复媒体副本。
- Media Studio 的 512/640/2,048 像素缓存缩略图和 8,192 像素全屏预览全部携带真实账户 ID；账户 ID 进入共享缓存键，避免切换账户后复用旧账户像素面。小于等于 2,048 像素的交互缩略图继续绕过重媒体排队，但不再绕过 Worker 隔离。
- 普通聊天附件继续先验证记录 size 与 SHA-256。验证阶段冻结 device/inode/byte count/mtime，Worker 客户端只接受同一授权身份，完成后再次用 `O_NOFOLLOW` 打开并复核；digest 后路径替换、inode/大小或 mtime 变化均 fail-closed。Agent 聊天复用同一实现，真实 `ownerUserID` 从 ViewModel/消息调用链传入。
- 主 App 解码的最终 PNG 已由 Worker 限制像素边长、源像素数和输出字节数；这保留 SwiftUI/AppKit 显示所需像素面，同时把恶意原图解析、方向变换、下采样和清洗编码的崩溃/峰值边界移出 UI 进程。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 38 项全部通过，覆盖 Media Studio 19 项、普通聊天附件 4 项、Agent 附件 5 项、Worker Core 4 项及真实 Worker 进程 6 项。新增真实进程用例覆盖 Media Studio 缩略图完整链路；新增附件回归覆盖 digest 校验后源路径被替换仍必须拒绝。Release 测试与 Worker product 使用 `warnings-as-errors` 优化编译通过，真实 Worker 的热态小缩略图链路在该测试机约 35--37 ms；该单点数据只用于发现明显回退，不作为正式性能 SLO。未提交源码的完整 arm64 Release 诊断包再次通过 Worker/App 嵌套签名、dSYM UUID、bundle 哈希和 build-evidence 校验；未启动、安装或替换正式 App。

边界：App 为渲染仍必须持有最终受限 `CGImage`，本批不宣称 UI 像素面为零；草稿内存图片、头像、Markdown、项目文件预览和其他普通图片消费者尚未全部迁移。当前一次请求启动一个 Worker，尚未以大量列表冷启动、20 MiB/64 MP、快速滚动/取消风暴和组合内存压力决定是否需要受控的批处理或常驻执行模型。视频 frame/poster、上传/下载、durable media job、ENOSPC/睡眠/断网、长期 soak 及 Developer ID/hardened runtime/notarization 仍未完成，不能据此宣称 P0-E/P2/P4 或总体性能优化完成。

### 12.75 第四完整里程碑：Story 视频尾帧提取迁入 Media Worker（2026-10-23）

本里程碑迁移主 App 中唯一的生产 `AVAssetImageGenerator` 路径。Story 视频完成后的连续性尾帧此前在 UI 进程加载最高 512 MiB 视频、解码最高 4096 像素帧，再编码为 PNG `Data` 后复制到 Story Store；现在 AVFoundation 解码、方向变换、下采样、PNG 编码、同步和 SHA-256 全部由 Media Worker 完成。

已实施：

- wire v1 增加 `extract_last_video_frame_png_v1` capability 和专用请求，固定语义为“有效时长最后一个 1/600 秒 tick、向前容忍 1/30 秒”，不暴露任意 seek 或格式转换接口。输入普通文件/owner/device/inode/byte count 和预创建单链接空输出继续复用既有授权，视频上限 512 MiB，输出长边 4096 px/20 MiB。
- AVFoundation 不能可靠识别 `/dev/fd` 容器，因此 Worker 保持授权 descriptor 打开，并在 AVURLAsset 创建前和提取完成后对 App-owned 路径执行 `lstat` 身份复核；symlink、路径替换、inode/大小变化或输入输出同文件均 fail-closed。
- `StoryVideoFrameExtractor` 现在返回 lease-owned `GeneratedMediaFile`，真实账户 ID 和任务/session context 一并传入 Worker/Governor。Batch、直接生成和恢复式尾帧路径调用 Store 的新文件重载，采用有界复制与重入后二次业务 fencing；失败会删除候选，成功后临时 artifact 自动释放，4K PNG 不再聚合为主进程 `Data`。
- hello response 同时发布图片与视频 capability；新客户端按实际 operation 校验 capability，旧图片客户端仍可只声明 `normalize_png_v1`，避免不必要的握手破坏。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 43 项全部通过，包括原有图片/附件/生命周期 38 项、视频 deadline/symlink/真实进程/lease artifact 4 项，以及 Story Store 文件提交 1 项。真实 Worker 使用 AVAssetWriter 生成的两帧 H.264 MOV 验证尾帧提取，并覆盖 Story extractor 的 lease artifact 完整链路；首版 `/dev/fd` 输入被 AVFoundation 拒绝，门禁未放行，切换为 descriptor 保活 + 路径双重身份 fencing 后同一用例通过。Release 测试、Worker product 和 App product 均以 `warnings-as-errors`/优化配置编译通过；打包脚本语法、build-evidence 的 25 项回归和差异空白检查通过。未提交源码的完整 arm64 Release 诊断包通过 5 个二进制架构检查、Worker/App 嵌套签名、4 个 dSYM UUID 匹配、bundle 哈希和 build-evidence 校验，manifest 标记为 `diagnostic-build` / `uuid-matched`；未运行 launch smoke，未启动、安装或替换正式 App。

边界：本批只迁移 Story 已落盘视频的尾帧，不新增 UI poster 缓存，也不处理任意时间 frame 或正在下载的 partial。AVFoundation 仍必须用 App-owned 路径打开媒体；普通子进程不是 sandbox，同用户恶意进程若能在两次身份检查之间替换并恢复完全相同 inode/size 的内容，不属于当前信任模型。真实 512 MiB/4K physical-footprint、损坏容器/多轨/HDR/旋转矩阵、连续取消/崩溃、上传下载所有权、睡眠/断网/ENOSPC、长期 soak 和发布签名仍待完成，不能据此宣称 P0-E/P2/P4 完成。

### 12.76 第五完整里程碑：项目文件 4K 图片预览迁入 Media Worker（2026-10-23）

本里程碑关闭 12.69、12.74 保留的项目文件原始图片解码边界。项目文件服务当前仍以字符串 DTO 返回内容；此前预览路径在主 App 将最多约 28 MiB Base64 解成最多 20 MiB 压缩图片，再直接用 ImageIO 解析最高 64 MP 原图并生成 4K 像素面。现在 DTO 内容先有界落成私有 lease 文件，原图解析、方向变换、下采样和清洗 PNG 编码交给 `ChatOSMediaWorker`，App 只解码最长边 4096 的受限输出。

已实施：

- `ProjectImagePreviewLoader` 复用 `normalize_png_v1` 与既有 artifact writer，不新增 IPC capability。Base64 字符数、解码后 20 MiB、64 MP、输出 4096 px/20 MiB 门禁保持不变；输入和 Worker 输出均由 lease 管理，成功、失败或取消后自动删除临时目录。
- 真实认证账户 ID 进入 Worker 请求；SwiftUI `.task(id:)` 同时包含账户身份，异步返回后再次核对当前账户，切换账户或退出登录会取消旧预览，旧账户像素面不会写入新账户页面状态。
- 4K 预览继续占用既有单通道 `ClientHeavyMediaWorkGovernor` permit，permit 覆盖 Worker 启动、解码、输出和 App 最终受限 PNG 解码；task ID 仍为不暴露项目路径的进程内 opaque hash。

验证：Debug 与原生 arm64 Release 的单元/真实 Worker 集成各 3 项全部通过，覆盖共享重媒体通道排队、真实账户/4K 参数透传、独立进程解码、最终像素尺寸，以及输入/输出 artifact 全部回收；Release 目标以 `warnings-as-errors` 优化编译通过。打包脚本语法、build-evidence 的 25 项回归和差异空白检查通过。完整 arm64 Release 诊断包通过 5 个二进制架构检查、Worker/App 嵌套签名、4 个 dSYM UUID 匹配、bundle 哈希和 build-evidence 校验，manifest 为 `diagnostic-build` / `uuid-matched`；首次打包因并行 Windows 源码在构建期间变化被 evidence 正确拒绝，输入稳定后的增量重跑通过。未运行 launch smoke，未启动、安装或替换正式 App。

边界：项目文件上游 DTO 仍持有完整字符串；本批会在主进程短暂物化最多 20 MiB 的压缩源 `Data` 并立即落盘，只迁移高风险/高峰值的 64 MP ImageIO 解码，不宣称项目文件传输已经文件原生化。复制到剪贴板时既有 `imagePreviewData` 仍会按用户操作物化压缩数据；Markdown 网络图片和其他普通 UI 图片消费者尚未全部迁移。真实 20 MiB/64 MP physical-footprint、快速切文件取消风暴、ENOSPC、长期 soak、上传下载与 durable media job 仍待完成，不能据此宣称 P0-E/P2/P4 完成。

### 12.77 第六完整里程碑：Agent 头像原图解码迁入 Media Worker（2026-10-23）

本里程碑迁移 Agent 头像编辑器的本机文件、生成记录和远程生成图片三条生产路径。此前头像处理在主 App 直接用 ImageIO 解析最多 20 MiB/64 MP 原图到最长边 2048 的像素面，再裁剪并编码 256×256 JPEG；现在原图解析、方向变换、下采样和清洗 PNG 编码由 Media Worker 完成，App 只对受限 2K 输出执行固定 256×256 裁剪与 JPEG 编码。

已实施：

- 本机 security-scoped 文件、已落盘生成结果、HTTPS 流式下载和 inline Base64 均收敛到 `AgentAvatarImageLoader` 的文件路径；inline 压缩数据有界落成 lease artifact 后立即释放主进程源 `Data`，不把媒体字节送入 IPC。
- 头像解码复用 `normalize_png_v1`、20 MiB/64 MP 输入门禁、2K 输出门禁和既有单通道重媒体 Governor。最终 256×256 裁剪仅处理 Worker 清洗后的受限 PNG，不再接触原始容器。
- 编辑器和生成记录选择器传入真实认证账户 ID；账户切换或退出后会拒绝旧任务回写。取消继续终止 Worker，输入和输出 artifact 在成功、错误或取消时由 lease 回收。
- 高频 Markdown 图片与 Visual Session 帧没有直接套用一次一进程模型：前者单文档最多 32 张、后者持续刷新，必须先冻结多输出 artifact 所有权、部分失败和批量取消语义，避免进程启动成本造成新的性能回退。

验证：Debug 与原生 arm64 Release 的单元/真实 Worker 集成各 5 项全部通过，覆盖裁剪/输出预算、无效输入、HTTPS 流式落盘、真实账户与 2K 参数透传、独立进程原图解码，以及临时 artifact 回收；Release 目标以 `warnings-as-errors` 优化编译通过。打包脚本语法、build-evidence 的 25 项回归和差异空白检查通过。完整 arm64 Release 诊断包通过 5 个二进制架构检查、Worker/App 嵌套签名、4 个 dSYM UUID 匹配、bundle 哈希和 build-evidence 校验，manifest 为 `diagnostic-build` / `uuid-matched`；首次打包因并行 Windows/Plugin 源码在构建期间变化被 evidence 正确拒绝，输入稳定后的增量重跑通过。未运行 launch smoke，未启动、安装或替换正式 App。

边界：App 仍必须为最终头像持有一个受限 2K `CGImage` 和 256×256 编码面；本批不迁移已经持久化的 256×256 头像显示解码。inline/Base64 上游 DTO 仍短暂持有压缩字符串/`Data`。Markdown、Visual Session、剪贴板历史、Plugin 图标和 Pet 图片等高频或低风险消费者需要按调用频率决定批处理、缓存或继续留在进程内，不能机械地逐张启动 Worker。真实 20 MiB/64 MP physical-footprint、取消风暴、ENOSPC、长期 soak、上传下载和 durable media job 仍待完成，不能据此宣称 P0-E/P2/P4 完成。

### 12.78 第七完整里程碑：解除 Agent 跨会话与跨项目执行阻塞（2026-10-23）

本里程碑处理的不是会话 ID 串线，而是独立会话在执行层被错误地归入同一全账户串行域。此前同一 Agent 的 manager/executor 唯一索引、pending claim 和 Todo 启动判断均按 `(owner_user_id, target_agent_id)` 限制；更严重的是 `drainAccount` 从领取工作一直到模型 Run 结束都持有账户级 lease。项目 A 的长 executor 因而会让稍后到达的项目 B drain 在领取前等待，即使两者房间、Todo、取消域均独立且全局执行额度仍有空闲。

已实施：

- manager/executor 的 running 唯一索引与 pending claim 改为 `(owner_user_id, room_id, target_agent_id)` 范围；Schema migration 33 在单个 immediate transaction 内替换旧索引。同一房间内同一 Agent 仍各自保持 manager/executor 单通道，不同房间不再互相占用通道。
- Todo executor 的占用范围缩小到 `team_room_id`。调度状态可以同时报告某团队中正在运行的 Todo 和另一团队可独立启动的 ready Todo；manager 在仍有可启动工作时继续调用 `todo_start_next`，而不是被全账户 busy 状态提前结束。
- 删除覆盖整个模型 Run 生命周期的 account drain lease。并发 drain 依靠 SQLite 原子 claim、room-scoped 唯一索引、进程内 active-delivery registry 和共享 execution governor 防止重复；后到的独立任务可以使用剩余额度立即领取和执行。
- 账户队列改为按 Agent round-robin 多轮领取：同一 Agent 可以在不同房间领取多个独立任务，但每轮先给其他 Agent 领取机会，避免单个 Agent 抢完全部 permit。共享 Governor 的默认总并发上限仍为 2，不把“可并行”误解为无限制并发。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 34 项全部通过，包括调度器 23 项、聊天工具 9 项及 Todo schedule/migration 2 项；Release 使用 `warnings-as-errors` 优化编译。新增回归证明同一 Agent 可跨项目会话与私聊达到并发 2、可跨两个项目团队并发执行 Todo、后创建的项目 B executor 能在项目 A executor 尚未释放时启动并完成；同时验证同一房间仍严格串行、并发 drain 不重复领取、全局执行额度不被突破，以及取消、恢复、manager/executor fast lane 语义不回退。测试仅使用独立临时数据库，不启动或安装正式 App，不访问活动用户数据库。

边界：本里程碑消除了已确认的跨会话软件锁与数据库 lane 阻塞，但 Agent 模型 Run 仍执行在主 App 进程内；等待、崩溃、CPU/内存失控和进程级取消尚未形成每任务故障边界。用户已明确要求所有可并行任务拥有独立进程，因此 Agent Runtime 进程化现在是后续硬性里程碑，不再仅由旧方案中的性能阈值决定。下一阶段必须保持 Store 单一写所有者，由主 App 调度并通过版本化 IPC 启动每 Run worker；worker 不直接并发写共享 SQLite，并需补齐独立取消/超时、资源额度、durable checkpoint、工具代理、副作用 fencing、崩溃回收和真实多任务压力门禁。

### 12.79 第八完整里程碑：Agent Run 一任务一 Worker 进程（2026-10-23）

本里程碑把生产 Agent 团队调度器的状态机从主 App 迁入按 Run 创建的 `ChatOSAgentWorker`。每个 manager/executor Run 各自启动一个子进程，具有独立 PID、标准输入输出通道、墙钟超时和取消/终止域；项目 A 的 worker 等待、退出或协议失败不会占用项目 B 的 worker 生命周期。App 仍以共享 Governor 控制总进程准入，避免把独立进程误解为无限并发。

已实施：

- 新增 `ChatOSAgentWorkerProtocol` v1、`ChatOSAgentWorker` 和 `AgentWorkerProcessClient`。握手绑定 parent PID、Run UUID、随机 nonce 和协议区间；单条 NDJSON 消息硬限制 64 MiB，未知顺序、错误 Run/scope、协议不兼容或无父进程启动均 fail closed。
- Worker 独占 `AgentRuntime` 的模型/工具循环、检查点状态转换、重试/无进展计数和 Run 总时限；每次模型、工具、完成校验、Memory compose/sync 与 checkpoint record 通过父进程 capability 代理。App 只保留数据库、凭据、Memory 和工具授权的单一所有权，Worker 不打开也不写共享 SQLite。
- 生产 `AppModel` 显式注入进程执行器；没有“Worker 找不到就静默退回主进程”的生产 fallback。测试仍可注入 in-process `AgentRuntime`，但不改变已打包 App 的进程边界。
- 取消或墙钟超时立即终止所属 Worker；两秒后仍未退出会升级 `SIGKILL`。Worker 同时监视父 PID，App 退出后自行终止，不能成为孤儿常驻进程。
- checkpoint 在每个模型/工具边界仍由 App 原子持久化。若 Worker 在 `inFlightCallID` 清除前中断，Run 转为 `needsReview`，Todo 进入 blocked，不能把结果未知的写入/计费工具自动重放；没有 in-flight 副作用的故障按普通失败处理。
- 打包、签名、架构、Debug 路径、dSYM UUID 和 build-evidence 清单纳入新 Worker；无父能力启动检查加入既有 smoke 逻辑。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 38 项全部通过，包括真实 Agent Worker 进程 3 项、调度器 24 项、聊天工具 9 项及 Todo schedule/migration 2 项；Release 使用 `warnings-as-errors` 优化编译。真实进程测试覆盖 Memory/record/model/tool/terminal 完整代理链路，并证明两个并发 Run 获得不同 PID：第一个 Run 被故意保持等待时，第二个 Run 可先完成；取消其中一个 Run 只终止所属 Worker PID，父进程对已关闭 pipe 的并发写入被转换为普通错误而不会收到 `SIGPIPE`。故障回归证明存在 durable `inFlightCallID` 时进程中断会进入 `needsReview` 和 blocked Todo，而不是误标失败或自动重放。打包/证据脚本 25 项、shell 语法和差异检查通过；完整 arm64 Release 诊断包通过 6 个二进制架构、App/Host/Media Worker/Agent Worker 嵌套签名、5 份 dSYM UUID、bundle 哈希和 build-evidence 校验，manifest 为 `diagnostic-build` / `uuid-matched`。未运行 launch smoke，未启动、安装或替换正式 App，不访问活动用户数据库。

边界：当前独立的是 Agent orchestration/状态机进程；provider 网络客户端、Memory 客户端和受权工具仍由父进程代理，因此本批建立了 Run 生命周期与状态机崩溃边界，但不宣称第三方 SDK 解析崩溃、主进程内工具实现或全部 CPU/内存工作集已经迁出。后续要把可安全序列化的模型传输迁入 Worker，并为 Worker 增加跨进程动态 CPU/内存预算、连续崩溃熔断、真实 `SIGKILL` 每个 IPC 窗口、睡眠/断网/账户切换及长时间组合 soak；Plugin/Host 等已有独立进程继续使用各自监督协议，不能再套一层无所有权设计的 Worker。

### 12.80 第九完整里程碑：生产 Agent 模型传输迁入所属 Worker（2026-10-23）

本里程碑关闭 12.79 明确保留的 provider 主进程边界。生产 Agent 的 OpenAI Responses 请求体构造、文件上传、HTTP/SSE 读取、增量解析和终态组装现在都在该 Run 的 `ChatOSAgentWorker` 内执行；App 不再为这条路径执行第三方模型 transport 或解析响应。一个会话的 provider 卡死、畸形流或解析器退出只能终止所属 Worker，不会把另一个会话拖入同一模型执行栈。

已实施：

- `ChatOSAgentWorkerProtocol` 升至 v2，Start capability 可携带严格类型化的 Responses 配置。旧 v1 App/Worker 与 v2 版本区间不重叠时 fail closed，不允许静默退回主进程模型调用。
- App 仍从当前账户会话读取模型配置并决定是否授权；只有生产 `URLSessionHTTPTransport` 路径生成 Worker 配置，注入测试 transport 和其他未声明可迁移的模型继续走显式 callback，不猜测任意闭包可以跨进程序列化。
- endpoint、model、推理参数、输出预算、prompt cache key 与上游 key 仅经继承匿名 pipe 交付给单 Run Worker，不进入 argv、环境变量、checkpoint、日志或 build manifest。Worker 不获得账户 API client、SQLite 路径或任意配置查询能力。
- 凭据交付前、每次 provider 请求前和响应完成后均由 App 对原 authentication session 做授权检查。账户切换后的 provider 结果会在进入 durable checkpoint 或工具调用前被拒绝；取消仍只终止所属 Worker 和它拥有的 URLSession。
- Worker 本地直接产生模型流活动和解析状态，不再让每个 SSE 事件在 App 与 Worker 之间往返。工具、Memory 与 checkpoint 仍走父进程 capability，保持单一数据库写所有者和已有副作用 fencing。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 49 项，包括真实 Agent Worker 进程 4 项、生产模型选择/会话授权 10 项、调度器 24 项、聊天工具 9 项及 Todo schedule/migration 2 项；Release 使用 `warnings-as-errors` 优化编译。真实 Worker 用例启动本机 HTTP fixture，确认 `/v1/responses` 和 Authorization 请求确实来自 Worker、父进程模型调用为 0，并完成 terminal tool；同时确认凭据交付前/请求前/响应后共三次授权。打包/证据、签名和 dSYM 结果见本里程碑提交前的最终验证记录。

边界：当前迁移的是生产 Agent 团队调度器使用的 Responses provider，不自动改变 Story Studio 的一次性规划调用，也不把自定义注入 transport、Memory 或工具实现伪装成已进程化。上游 key 在 Run 存活期间存在于可信 Worker 内存；Worker 尚无独立 App Sandbox/seatbelt，因此这是故障与工作集隔离，不是对本机恶意进程的秘密隔离。后续仍需动态 CPU/内存预算、连续崩溃熔断、真实强杀授权/网络/工具各窗口、睡眠/断网/账户切换及长时间组合 soak，并逐项迁移仍在 App 内且适合跨进程的工具负载。

### 12.81 第十完整里程碑：剩余图片归一化解除主进程单槽阻塞（2026-10-24）

本里程碑关闭生产代码中最后两条依赖 `ClientHeavyMediaWorkGovernor.shared` 的图片链路。剪贴板 TIFF 等非直传格式和 Plugin 模型 PNG 此前仍在 App/Connector 进程内执行 ImageIO/AppKit 解码，并被同一个全局单槽 Governor 串行化；任何一张慢图都会让无关会话、项目和媒体功能排队。

已实施：

- 粘贴图片先写入 mode `0700/0600` 的随机私有临时目录，再通过既有 `normalize_png_v1` 启动专属 `ChatOSMediaWorker`。每次粘贴拥有独立 PID、deadline、取消和故障边界；Worker 输出经路径、MIME、字节数、SHA-256、像素和 PNG 签名校验后才读回有界附件字节，成功/失败/取消均删除临时目录。
- Media Worker wire/build contract 升至 v2，新增 `normalize_jpeg_v1`。JPEG 请求沿用输入/输出 inode 授权、25 MiB/64 MP 协议上限、最长边、deadline、同步和摘要校验，并额外验证有限的 `0...1` 压缩质量及 JPEG 首尾签名；v1/v2 不兼容组合 fail closed。
- Plugin 模型图片最多接受 8 个 image block；合格 PNG 不再在 Connector 内解码，而是每块各自并行启动一个 JPEG Worker，完成后按原索引归并。单块损坏只降级为该块的显式 unavailable 文本，取消仍向上传播，不拖住或污染其他会话。
- 会话粘贴路径和 Plugin 执行路径显式传入各自 session/owner 范围；测试注入协议与生产 `MediaWorkerProcessClient.shared` 使用同一调用契约。生产源码已无 `ClientHeavyMediaWorkGovernor.shared` 调用，旧类型暂留作历史测试与后续删除，不再成为运行时准入点。
- ADR 23 同步冻结 `media-worker-v2`、JPEG capability、并行图片块和当前覆盖范围；打包仍只维护 inherited stdio 这一套正式 IPC。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 23 项全部通过，包括 Worker Core 9 项、Plugin 归一化/并发 6 项、粘贴图片 6 项和两条真实 Worker 进程集成。并发回归证明同一 Plugin 结果的 3 张图片达到 Worker 最大并发 3；真实 Release Worker 完成 TIFF→PNG 与 PNG→JPEG。Release 全测试目标以 `warnings-as-errors` 完成优化编译；build-evidence 脚本 25 项和 `git diff --check` 通过。未运行 launch smoke，未启动、安装或替换正式 App，不访问活动用户数据库。

边界：最终 PNG/JPEG 仍需作为有界 `Data` 进入聊天附件或模型请求 DTO，本批消除的是原始容器的主进程解码/编码和跨功能单槽等待，不是网络层 JSON/Base64 流式化。单个 Plugin 结果最多可并行 8 个 Worker，后续账户级动态 CPU/内存预算必须做跨进程观测与公平调度，不能重新用全局单槽锁实现。Media Worker 当前仍共享异常退避状态，连续 Helper 基础设施故障可能影响后续启动；按会话/请求隔离熔断、ENOSPC、内存压力、取消风暴和长期 soak 仍是后续门禁。

### 12.82 第十一完整里程碑：Media Worker 故障不再跨任务传染（2026-10-24）

本里程碑移除 `MediaWorkerProcessClient` 的全局静态熔断器。此前任一 Worker 的 launch、握手、输出或协议异常都会写入同一进程级失败序列；下一张完全无关的图片即使使用健康 Worker，也会先等待 1/2/4/8/16/30 秒，累计五次后所有媒体请求共同隔离 30 秒。这与一次请求一次进程的故障边界冲突，也是跨会话“没有串 ID 但仍互相阻塞”的隐蔽来源。

已实施：

- PNG、JPEG 和视频尾帧请求不再查询或修改共享异常日期、退避时间和 quarantine；每个请求只受自己的 Worker PID、deadline、取消和强杀状态约束。
- 业务级拒绝、异常退出、无效 Helper 和超时仍按原错误返回，输出身份/摘要/签名校验不放宽，也不增加 in-process fallback 或自动重放。
- 新增真实进程回归：先启动立即异常退出的独立 Helper，再立刻启动正式健康 Worker；健康请求必须在 900 ms 内完成，证明没有继承一秒退避。测试目标继续使用随机私有文件和独立 PID，不访问用户数据。
- ADR 23 明确一次性 Worker 不共享故障退避；未来总 CPU/内存额度由跨进程资源 Governor 负责，但不得以全局单槽或共享失败状态实现。

边界：移除共享熔断避免故障传染，也意味着高频调用方不能依赖全局 quarantine 抑制错误风暴。当前每个入口已有输入数量、大小、deadline 与任务生命周期上限；账户级跨 Helper 动态资源预算、连续恶意请求速率限制和发布 soak 仍需后续交付。Agent Worker 使用按可执行身份共享的独立熔断实现，尚需另行核对它是否会让无关 Run 互相隔离，不能把本批结论外推到 Agent/Plugin/Host。

### 12.83 第十二完整里程碑：Agent Worker 故障不再阻塞其他 Run（2026-10-24）

本里程碑完成对 `AgentWorkerProcessClient` 共享准入状态的审计并移除按 Worker 可执行文件身份共享的静态熔断器。此前任一 Run 的 launch、握手、协议、无效响应或资源超限都会写入同一失败序列；下一个不同会话、不同项目甚至不同 Agent 的健康 Run 会在 durable Delivery 领取前被共同退避 1/2/4/8/16/30 秒，累计五次后全部隔离 30 秒。会话 ID 和 checkpoint 即使完全正确，这个执行层共享状态仍会造成用户观察到的“另一个任务一直不执行”。

已实施：

- `checkExecutionAdmission()` 继续在领取 durable Delivery 前验证 Worker 存在且是当前用户拥有、单硬链接、可执行的普通文件，但不读取任何其他 Run 的故障历史。
- 每个 `executeRun` 仍各自启动独立 `ChatOSAgentWorker` PID，并保留本 Run 的墙钟 timeout、取消、资源采样、`SIGTERM`/`SIGKILL`、版本握手、Run UUID/nonce 绑定和输出校验；失败不再写入进程级共享 backoff/quarantine。
- 删除已无生产语义的 `temporarilyUnavailable` 错误和熔断器注入测试；调度器的 fail-closed admission 回归改为验证 Worker 确实不可用时仍不得提前领取 Delivery。
- 新增真实 Worker 回归：一个 Run 在模型授权 IPC 窗口被 `SIGKILL` 后，复用同一个生产 client 和同一个 Worker executable 立即启动独立 Run；第二个 Run 必须获得不同 PID 并完成，不能继承一秒退避。既有强杀副作用窗口继续验证 durable `inFlightCallID` 会进入 `needsReview` 且禁止自动重放。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 38 项全部通过，包括真实 Agent Worker 13 项和调度器并发/恢复/副作用门禁 25 项；Release 使用 `warnings-as-errors` 优化编译。真实回归覆盖授权、网络和写工具三个 `SIGKILL` 窗口、16 个独立 Run/独立 PID、断网重连、账户切换、资源超限和取消隔离。Release 首次紧随九分钟全量链接执行时，16 Worker 压力项受瞬时机器负载影响命中 Run timeout；该项隔离重跑在 0.214 秒通过，随后完整 38 项热态重跑在 8.157 秒全部通过，未发现可重复功能失败。未启动、安装或替换正式 App，不访问活动用户数据库。

边界：本批消除的是共享故障状态，不取消安全监督，也不允许 in-process fallback。移除全局 quarantine 后，错误风暴不能再靠暂停所有会话来抑制；后续账户级 CPU/内存 lease、来源限速与连续异常告警必须跨进程观测并公平分配，不能使用全局单槽锁，也不能让一个 Run 的失败成为其他 Run 的准入条件。Memory 和部分工具 capability 仍由 App 代理，适合迁移的重负载仍需逐项进程化。

### 12.84 第十三完整里程碑：独立 Agent Run 按机器资源扩展并行度（2026-10-24）

本里程碑修正静态保守默认值造成的剩余跨会话排队。12.12 为防止 Host 与 Swift Agent 两套并发默认值相乘，把常规机器固定为总额 3、Swift Agent 2；在进程隔离、每 Run 资源采样与真实 16 Worker 回归均已建立后，这个固定值会让第三个独立会话即使在高配机器上仍等待，与“可并行任务拥有独立 PID”的产品要求不符。

已实施：

- 未显式配置时，后台总逻辑 job 同时受 CPU 与物理内存约束：为 App/UI/OS 保留 2 个逻辑核和 2 GiB，每个 Host/Agent 逻辑 job 分配 2 GiB 预算，取两种容量的较小值并限制在 2–16。
- Host 默认取得总额约四分之一且至少 1 个额度，剩余额度用于 Swift Agent 独立 Run；当前 12 核、96 GiB 开发机从 Host 1 + Swift Agent 2 提升为 Host 2 + Swift Agent 8。8 GiB/12 核机器仍保持总额 3，8 核/充足内存机器为总额 6、Swift Agent 5，避免仅按 CPU 数盲目放大工作集。
- `CHATOS_BACKGROUND_JOB_BUDGET` / `ChatOSBackgroundJobBudget` 的显式 2–16 覆盖保持不变；Host 单项覆盖仍不能突破总额，且始终为 Swift Agent 保留至少一条 lane。
- claim 前 Governor、队列上限、取消归还、即时容量拒绝和并发峰值指标全部保留。额度内每个 Run 仍由自己的 `ChatOSAgentWorker` PID 承担；超出安全预算时只在 durable claim 前等待，不占用数据库 lease。

验证：Debug 与原生 arm64 Release 的针对性矩阵各 32 项全部通过，包括调度器并发/恢复/副作用门禁 25 项和资源预算/Governor 7 项；Release 使用 `warnings-as-errors` 优化编译。确定性硬件夹具覆盖 12 核/96 GiB、12 核/8 GiB、8 核/96 GiB、2 核/4 GiB，以及无效/显式覆盖；继续验证预算 2 时执行峰值精确为 2、claim 不超额、排队取消与 permit 幂等归还。未启动、安装或替换正式 App，不访问活动用户数据库。

边界：这是基于启动时硬件容量的静态自适应，不是运行中的热状态、内存压力或低电量动态调额，也没有把 Host 实际 in-flight、Media、Plugin 和数据库维护汇入同一个跨进程 lease 服务。显式配置仍可选择更高额度，运维应结合机型与负载设置。后续仍需账户公平、前台权重、动态迟滞、组合负载和长时间 soak；这些保护不得重新退化为全局单槽。

## 十三、验证矩阵

| 场景 | 必须验证 |
| --- | --- |
| 项目聊天 Run 执行中切 Agent 团队 | 输入、滚动和首帧可交互；后台 Run 不丢失 |
| 同一 Agent 在两个独立会话/项目执行长任务 | 可在共享总资源预算内并行；后到任务不等待先到任务结束；同一会话仍保持因果顺序；取消或失败只影响所属任务 |
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
- Agent Group Runtime 的生产路径固定为一 Run 一 `ChatOSAgentWorker`；协议不兼容或 Worker 缺失时暂停/失败并保留 durable checkpoint，不静默退回主进程；
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
- Agent Run Worker 协议/入口/父进程客户端：`clients/macos/Sources/ChatOSAgentWorkerProtocol/`、`clients/macos/Sources/ChatOSAgentWorker/`、`clients/macos/Sources/ChatOSConnector/AgentWorkerProcessClient.swift`
- Agent Worker 模型传输 capability：`clients/macos/Sources/ChatOSAgentRuntime/AgentWorkerModelTransport.swift`、`clients/macos/Sources/ChatOSAPI/ChatOSStoryPlanningService.swift`
- Agent Run Worker 传输 ADR：`clients/macos/docs/24-agent-run-worker-transport-adr.md`
- Local Agent Host 生命周期：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostLifecycle.swift`
- Local Agent Host IPC：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostProcess.swift`
- Media API：`clients/macos/Sources/ChatOSAPI/ChatOSMediaGenerationService.swift`
- Media Worker 协议/解码核心/入口：`clients/macos/Sources/ChatOSMediaWorkerProtocol/`、`ChatOSMediaWorkerCore/`、`ChatOSMediaWorker/`
- Media Worker 主进程监督客户端与传输 ADR：`clients/macos/Sources/ChatOSApp/Infrastructure/MediaWorkerProcessClient.swift`、`clients/macos/docs/23-media-worker-transport-adr.md`
- 文件请求上传与有界响应：`clients/macos/Sources/ChatOSAPI/HTTPTransport.swift`、`clients/macos/Sources/ChatOSNetworking/BoundedURLSessionDataLoader.swift`
- 大文件网络传输共享准入：`clients/macos/Sources/ChatOSNetworking/ClientLargeTransferGovernor.swift`、`clients/macos/Sources/ChatOSNetworking/URLSessionChunkStream.swift`
- 重媒体解码共享准入：`clients/macos/Sources/ChatOSNetworking/ClientHeavyMediaWorkGovernor.swift`、`clients/macos/Sources/ChatOSApp/Features/MediaStudio/MediaStudioImageLoader.swift`、`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryVideoFrameExtractor.swift`
- 参考视频增量 JSON/Base64 请求文件：`clients/macos/Sources/ChatOSAPI/ChatOSVideoRequestBodyFileWriter.swift`
- 视频响应流式临时文件：`clients/macos/Sources/ChatOSAPI/ChatOSResponseBodyFileWriter.swift`
- 参考媒体 DTO：`clients/macos/Sources/ChatOSCore/MediaGenerationModels.swift`
- Story 参考视频文件引用：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryVideoGenerationAdapter.swift`、`StoryMediaBatchSession.swift`
- Media Studio 状态：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/MediaStudioViewModel.swift`
- Story Studio 状态：`clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryStudioViewModel.swift`
- 项目文件 4K 图片预览：`clients/macos/Sources/ChatOSApp/Features/Project/ProjectFileEditorView.swift`
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
| 凭据 | 持久配置与账户 session 留在 App；v2 仅经继承 pipe 向单 Run Worker 交付 provider 凭据，请求前后重验 session；未来优先替换为短期 Run token | P0-E/P2 前；API/凭据 |
| 并发预算 | Host + Swift Agent 后台总额度起始 3；媒体重任务 1；大传输 2 | P0-D 验收；runtime/性能 |
| 防重与维护 | v35 防重见 16.2，v36 正文预算见 16.3，v37 不可变引用见 16.4，新版 Host 进程锁见 16.5；owner 配额、旧 Host 交接及在线维护仍待完成 | P0-C 生产迁移前；Host/存储 |
| 分发与升级 | 定义内部测试签名和正式外部分发两条验收路径、旧版本兼容范围 | P2 打包前、P4 发布前；发布工程 |

每份 ADR 必须记录问题、证据、选项、选择理由、失败/回滚行为和验证方式。未冻结的决策不阻止其他独立阶段推进，但不能跨过对应生产启用门禁。

### 16.1 源码与文档漂移治理

- 以协议代码常量与 Schema migration 为事实源。当前 Rust/Swift/Windows Host 常量与共享 fixture 为 v46；本方案引用版本不能替代实际握手。
- CI 比较 Swift/Rust 协议版本、payload 边界、支持能力和打包后的真实可执行版本；Schema 兼容矩阵、进程布局及 ADR 随实现更新。
- 为每个阶段归档 manifest、测试脚本、脱敏统计、故障注入结果与 trace，结论标注“现状/目标/已验证”。首批 P0 改动见 12.1；大传输静态 Governor 见 12.66，重媒体静态 Governor 及其主进程 4K 解码扩展见 12.67–12.69，跨资源动态/跨进程 Governor、Worker 和剩余门禁仍为待实施项。

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

### 16.6 ADR：Agent Run Worker 单一数据所有者与 capability 代理

- 问题/选择：跨会话 lane 并行只能解决软件锁等待，不能阻止一个 Agent 状态机崩溃或卡死影响主 App。生产调度器因此为每个 manager/executor Run 启动一个短生命周期 Worker，而不是为账户、Agent 或会话复用常驻进程。
- 数据所有权：SQLite、Delivery/Todo claim、Memory 服务、持久模型配置来源和工具授权只存在于 App；Worker 不接收数据库路径。v2 仅在 App 重新验证账户 session 后经继承 pipe 交付该 Run 的 provider 配置与凭据，并在请求前后再次授权；这不会引入多个 SQLite 写所有者，也不允许 Worker 查询其他模型或绕过 Todo 身份 fencing。
- 协议/生命周期：v2 握手绑定父 PID、Run UUID、nonce 和版本区间；父进程取消/超时先 `SIGTERM`、两秒后 `SIGKILL`，Worker 监视父 PID。每个请求严格顺序、消息有 64 MiB 硬上限，最终 checkpoint 必须匹配起始 Run/scope。旧 v1 不支持模型传输，版本不匹配必须拒绝。
- 副作用：`tool_started` checkpoint 必须先由 App 持久化再执行 capability。Worker/IPC 在结果确认前中断时保留 `inFlightCallID` 并转 `needsReview`，不能通过启动新 Worker自动重放；只有 Human 明确走既有 retry/reconcile 流程才能清理该 fencing。
- 回滚/边界：打包生产路径不允许静默 in-process fallback；回滚必须回到上一完整 App 包，并遵守当前 Schema/Run checkpoint 兼容性。生产 Responses provider 已在 Worker，Memory/tool 仍在 App capability 端；下一阶段继续跨进程动态资源预算、熔断、强杀矩阵并逐项审查工具迁移。详细传输约束见 `clients/macos/docs/24-agent-run-worker-transport-adr.md`。

## 十七、最终决策摘要

本项目需要的是选择性进程隔离，不是“每个大功能一个进程”：

- 已有 Rust Host、Plugin 和 Global Utilities 隔离继续保留；
- 媒体先完成上传/下载文件原生化与 durable job 一致性，再新增 Media Worker 隔离解码和文件 I/O；
- Agent 团队已按 Run 独立进程；后续继续把安全可迁移的模型传输与资源预算移入该边界，不回退到账户级长锁或共享常驻 Agent 进程；
- 主 UI 进程必须先解决 Combine 生命周期、状态观察范围和 eager activation；
- Local Agent 必须先解决 Receipt 与大 JSON 写放大；
- Swift Agent 与 Rust Host 必须共享总并发预算，并处理 claim/lease 与嵌套调用，不能各自限流后相加失控；
- 所有跨进程架构都以单一数据所有者、小消息、文件化大结果、可取消和可恢复为前提；
- 身份认证、文件授权、最小权限和基本监督随新 Helper 同期交付，故障隔离与安全隔离分别验收。

只有按这个顺序实施，多进程才会成为稳定性边界，而不是新的复制、同步和数据库竞争来源。
