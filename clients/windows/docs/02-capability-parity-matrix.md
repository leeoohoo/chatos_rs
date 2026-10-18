# Windows / macOS 能力对齐矩阵

## P0 覆盖门禁（2026-10-18）

| 能力 | 严格状态 | 当前证据 / 缺口 |
| --- | --- | --- |
| 从真实空目录创建项目并在项目聊天操作 | 阻塞，未验收 | `C:\ChatOS-ProjectChat-Acceptance-20261018-01` 复核为 0 项、非 reparse point，但尚未在最终 x64 Debug UI 中选择；没有 project/conversation identity、真实 list/read/search/write/approval/diff/restart 证据。Computer Use `node_repl/@oai/sky` 未提供。创建项目关键状态与动作、绝对路径、真实目录项计数 AutomationId 已补；刷新失败后的陈旧路径创建已 fail-closed。Create Project 34/34、桌面合同 29/29、审批可观测/焦点 2/2，均仅算自动化准备。 |

此门禁通过前，下方 W308/Plugin/Skill/MCP/Media/Pet/远程/打包等矩阵只保留历史状态，不继续实施或验收。

2026-10-18 10:35 +08:00：新一轮互斥/目录/产物/进程检查通过；权威目录仍为 0 项、普通目录、非 reparse point，最终 Debug 产物存在，相关进程为 0。完整 Computer Use 指令可读，但可调用目录仍无 `node_repl/@oai/sky`，因此未操作客户端，也未用替代 UIA、数据库/API、fixture/mock 或已有项目。读审未发现无真实复现即可确认的新缺陷，未做猜测性修改或重复构建。P0 三行严格状态不变；证据 `.local/parity/2026-10-18-project-chat-real/21-computer-use-retry-audit.md`。

macOS 后续 Bug 修复、功能更新和协议变化先进入 `10-macos-change-sync-register.md`，完成 Windows 自动化与真机验收后再回写本矩阵。

本地代码完成审计：2026-09-29。已发现并补齐 macOS 近期增加的邀请码邮箱注册；其余登记范围内未发现仍可由本地代码关闭的 Windows/macOS 差距。

状态定义：

- `未开始`：尚无 Windows 实现。
- `骨架`：已有接口或页面结构，但未接真实能力。
- `实现中`：已有主要代码，仍缺测试或端到端验收。
- `已实现`：代码和自动化测试完成。
- `Windows 验收`：代码完成，等待 Windows 真机验收。

| 领域 | 能力 | Windows 状态 | 验收要求 |
| --- | --- | --- | --- |
| 工程 | .NET 分层解决方案 | Windows 验收 | Core/API/Connector 已构建，Desktop 等待 Windows XAML 编译 |
| 工程 | 依赖注入、配置、日志 | Windows 验收 | DI、环境配置和统一敏感日志审计已完成；异常日志只记录失败类型，不记录 URL、Token、Secret、API Key 或供应商错误正文 |
| 设计 | 全局颜色、字号、圆角、间距 token | Windows 验收 | 基础 token 已统一接入，等待 Windows 多 DPI 与 macOS 视觉对照验收 |
| 设计 | 中英文切换 | Windows 验收 | 全部 Desktop XAML 已移除中文硬编码；页面、DataTemplate、资源标题、状态和操作反馈均绑定统一运行时本地化源 |
| 认证 | 登录、邀请码邮箱注册、恢复、退出、401 失效 | Windows 验收 | 注册 API、邮箱验证码、服务端重发倒计时、两次密码校验、自动登录和 Credential Manager 已接入并通过 Windows 原生测试与安装包，等待真实邮箱与高 DPI UI 验收 |
| Shell | 联系人、项目、终端、远端侧栏 | Windows 验收 | 联系人、项目、本机 Connector、独立多工作区本机终端和远端连接均已真实加载并进入对应页面；2026-10-17 补齐新项目首次聊天会话回写、加载/错误/重试和标签重入自动化，真实新项目读/搜/改与重启仍待 UI 验收 |
| Shell | 全局记事本和设置 Toolbar | Windows 验收 | 两个 Toolbar 均已接真实页面，记事本支持原地关闭返回工作区 |
| 聊天 | 历史分页与缓存 | Windows 验收 | 状态机、SQLite cache-first 和服务端校正已实现并测试 |
| 聊天 | 流式消息与工具过程 | Windows 验收 | Realtime 过程即时展示，终态刷新和未读新动态已实现 |
| 聊天 | Composer、附件、粘贴和拖放 | 已通过 | 真实选择/取消、预览、删除、资源管理器拖放、20 个及累计 20 MiB、短/长文本粘贴、PNG 规范化、权限拒绝、发送失败正文+附件恢复、成功模型消费、历史/vault 持久化和重启恢复均通过；已修复异步粘贴重复、伪 PNG、读入预算及登录恢复 runtime 竞态，见 docs/16 |
| 聊天 | 模型、推理和 Plan 模式 | Windows 验收 | 权威配置读写、发送参数和 WinUI 控件已接入 |
| 聊天 | Ask User 直接处理 | Windows 验收 | 2026-10-16 真实 Task 已通过必填、单选默认值、多行、普通提交继续、敏感掩码与安全拒绝、取消清理、760 DIP/Pet 可达性和重启；多选点击、提交中断网、timeout、Pet 内嵌卡及审批仍待验收，见 docs/19 |
| 任务 | 任务卡、任务图和详情 | Windows 验收 | 消息 Task Graph、真实任务名/状态、Run 详情和事件分页已接入；2026-10-16 已用真实单 Task 验证 running、Ask User、完整事件链、cancelled 与重启恢复，多节点/分页仍待验收 |
| 任务 | 执行过程、取消、重试和阻塞处理 | Windows 验收 | 2026-10-16 实机取消、实时终态、取消按钮禁用、重启恢复以及 Ask User 操作区避开 Pet 均通过；重试、失败任务和断流轮询回退仍待验收 |
| 项目 | 文件目录、搜索、预览和编辑 | Windows 验收 | 真实 API、状态机和 WinUI 页面完成；默认预览，支持创建、原子重命名和确认删除，二进制不进入文本编辑器。2026-10-16 已实机通过中英文无命中、README 搜索/打开、清空恢复目录及重启持久化；慢/失败请求、权限、排序、完整树和刷新恢复继续待验收 |
| 项目 | Git 状态、Diff、历史和提交 | Windows 验收 | 安全路径、状态、双层 Diff、历史、暂存、提交、分支、远端、取消和 ff-only 拉取已完成并测试，不覆盖外部修改 |
| 项目 | Plan、Requirement 和执行范围 | Windows 验收 | 需求层级、任务、文档、Execution Plan 查询/创建、精确 identity 确认、放弃和停止已实现 |
| 项目 | 运行环境、目标和实例 | Windows 验收 | Catalog/State/Environment、目标分析、工具链与环境变量、启动/停止/删除、日志轮询和 WinUI 页面已完成 |
| Agent 团队 | Agent 配置、团队、成员、项目经理和默认 Agent | Windows 验收 | 本机账号隔离的 Agent Profile、模型/推理/职业/插件/Skill/心跳配置，项目团队、成员职责、项目经理、默认路由和 Human/Agent 私聊已接入；具有双重人员权限的 Agent 可发起新建成员、已有成员入队和移出成员提案，Human 在成员提案页批准/拒绝后由 SQLite v15 事务原子生效，移出保留 Profile，等待 Windows 真机 UI 验收 |
| Agent 团队 | 团队消息、@ 路由、附件和 Agent 间私聊 | Windows 验收 | durable message/delivery、精确 @、默认 Agent、4-hop/12-run、附件 payload 分表和 UTF-8 正文按需分段读取已实现；账号级全会话 Inbox 支持成员范围过滤、排除自身回复、单调已读游标、跨会话原子读取/标记和 v16 迁移，批量读取不加载 payload 且消除逐消息 N+1；每次真实模型 Run 使用独立 opaque reference vault，Agent/会话/消息/附件/Todo/资产/调研引用不可跨 Run 复用或用持久 ID 猜测，并提供账号工作区快照；触发消息和冻结 Todo 来源中的 PNG/JPEG/GIF/WebP/PDF 以 Responses 多模态 part 输入，限制最多 8 项、单项 8 MiB、合计 16 MiB，二次校验 MIME、Kind、实际字节数、文件签名及会话+消息归属，二进制不会进入 UTF-8 读取；Run 内 Markdown 文档草稿可作为一次性附件发送，团队/私聊/Inbox 回复具备 call receipt 幂等防重；等待 Windows 真机模型验收 |
| Agent 团队 | Todo、依赖、进展、调度和共享资产 | Windows 验收 | Todo 不可变执行合同、跨会话来源关系、任务级 builtin/Plugin 能力快照及 allowlist 交集校验、原子 schedule state/start-next、单 Agent 单执行槽、优先级选取、revision、依赖释放、排序、进展、manager 状态唤醒和资产建议已实现；团队与项目查询固定按执行中、就绪、等待依赖、阻塞、完成、取消展示，终态历史不会压住活跃工作；列表 SQL 默认最多 200 条、服务端硬上限 1000 条，模型 `todo_list` 默认 100 条且二次限制最多 200 条，避免历史增长形成无界数据库读取、来源附加和模型/UI payload；executor 不扫描整块任务板，而按 delivery 精确读取当前 Todo，以单次有界查询加载最多 100 个冻结依赖，并用同一连接上的三条批量 SQL 按合同顺序加载最多 64 条跨会话来源消息、提及和附件元数据，不逐来源查询且不加载附件 payload；Todo 完成后的依赖释放使用单条集合 SQL，只推进直接引用该 Todo 且全部前置已完成的待处理项，不再无界物化全账号 Pending Todo 或逐项查询依赖；Plugin 通过 run-scoped opaque ref 选择且执行时只加载冻结子集；executor 输入已隔离为冻结合同/来源/依赖/能力/资产，启动时由 v18 持久化团队资产 revision 快照，拒绝经理聊天历史、实时资产漂移和快照外工具；communication lane 只允许项目经理重排/取消，executor lane 的 complete/block/progress 均二次核对正在运行的 owning delivery、room、Agent 和当前 Todo，不能借同一 Agent 的其他 run-scoped Todo ref 越权写进展，且 progress 只接受非终态 Update；隐藏工具调用仍由服务端拒绝；两条 lane 使用独立账号锁、定向原子 claim 和定向崩溃恢复，2 秒 communication 后台循环不会等待长 Todo executor；瞬时模型超时、408、429 与 5xx 在同一 run 和 16 次总预算内以 1/2/4/8/16 秒退避最多重试 5 次，显式重试失败 Todo 会原子复活同一 delivery、兼容旧 revision 键并延续同一 durable run identity/累计调用数，不复制触发消息；来源与合同由 v17 无损迁移，等待 Windows 真机模型与崩溃恢复验收 |
| Agent 团队 | 项目级需求调研 | Windows 验收 | 调研已从房间归属迁移为项目归属，v13→v14 无损回填；项目经理或获授 `requirement.survey.manage` 的成员可创建/读取/解决，支持统一 `skill_activate/list_resources/read_resource` 渐进协议、跨团队项目 Todo 核对、Human 填写和 durable 回唤；独立项目入口、按阶段排序的列表、详情、问卷提交、只读权限状态、解决方案/执行步骤、空态、错误态与刷新均已接入 WinUI，单次查询限制 1–500 条且默认 200 条并优先保留待处理项，等待 Windows 真机视觉和交互验收 |
| Agent 团队 | 模型循环、插件、项目工具和终端 | Windows 验收 | OpenAI Responses 工具循环已改为读取配置中心托管的调用、重试、请求和整轮超时预算并按 Run 冻结，Windows 原生测试和安装包已通过；供应商错误脱敏、默认插件 MCP、成员 allowlist、权限/审批、OAuth/Secret、Artifact、项目工具、后台心跳和 durable run 均已实现；等待真机长 Run、插件进程和命令审批验收 |
| 记事本 | 目录、搜索、编辑、预览和导出 | Windows 验收 | 服务端目录/笔记 API、稳定选择、切换前保存、默认预览、编辑/分栏、基础 Markdown 渲染和原生导出已完成 |
| 本机 | Connector 注册、心跳和重连 | Windows 验收 | ticket、设备/多工作区配对、独立 Connector 凭据、断开、状态轮询、managed trust、签名 WebSocket、心跳、有界重连和睡眠/唤醒恢复已完成并测试 |
| 本机 | Workspace 文件 Relay | Windows 验收 | 全部文件操作、原地响应、目录/盘符/UNC/ADS/symlink/junction 边界和可恢复覆盖移动已实现并测试 |
| 本机 | 代码导航与搜索 | Windows 验收 | 文件名/内容搜索与定义/引用启发式导航已完成；均限制当前项目边界、结果数量、超时、取消和短缓存，文件页可点击结果跳转 |
| 本机 | ConPTY 终端 | Windows 验收 | 原生 pseudo console 和一次性 terminal exec 均使用暂停启动 + Kill-on-close Job Object；超时、取消、完整进程树回收、有界输出及 Relay 408 已完成，等待 Windows ANSI/IME/复制和原生进程验收 |
| 远端 | SSH 测试与凭据 | Windows 验收 | 云端元数据 CRUD、本机 Credential Manager 凭据隔离、密码/私钥/证书认证、双段跳板机、SHA-256 主机指纹、取消/超时、二次验证码和 WinUI 编辑/测试已完成；SSH.NET 会保留原认证 attempt 并在同一会话内续交 session-bound MFA，待 Windows 真机验证跳板机与目标机的单次/连续挑战；对应 macOS 工作区变化仍未提交、未编号 |
| 远端 | SFTP 与远程终端 | Windows 验收 | SFTP 浏览、UTF-8 预览、上传下载、覆盖、目录/重命名/递归删除，以及保留 cwd、取消、二次验证和 200k 有界输出的远程命令终端已完成 |
| 插件 | 安装、校验、状态和卸载 | Windows 验收 | Catalog、hash、平台、架构、权限、升级补偿、禁用和卸载清理已实现并测试 |
| 插件 | stdio/HTTP MCP Relay | Windows 验收 | stdio/HTTP、取消、超时、大响应、进程回收、Secret 模板和 OAuth Bearer 已实现并测试 |
| 插件 | OAuth、Secret 和 artifact | Windows 验收 | OAuth/Secret 生命周期、安全 Artifact 注册/流式下载、全局产物中心、受控缓存打开和原子另存均已完成 |
| 插件 | Browser/Computer Use Visual Session | Windows 验收 | 多 adapter 选择、选中帧按需读取、generation 防旧帧覆盖、置顶 ToolWindow 和单会话关闭抑制已完成 |
| 审批 | 风险评估与默认模式 | Windows 验收 | 默认逐次审批、显式确认的全控制、精确命令会话 allowlist、SQLite 模式/历史和结构化 Windows AI reviewer 已完成；模型不可用时安全回退用户 |
| 审批 | 全局浮层直接允许/拒绝 | Windows 验收 | 主窗口直接展示命令、目录、来源、风险和排队数量，可拒绝、本次允许或本会话允许，等待 Windows 视觉与焦点验收 |
| 宠物 | 透明置顶窗口与多显示器 | Windows 验收 | 原生透明 ToolWindow、Always-on-top、物理坐标拖动、工作区约束和位置持久化已实现 |
| 宠物 | 左右跑步和业务动画 | Windows 验收 | 拖动方向翻转、原生窗口同步移动和脚步动画已实现，等待多 DPI 真机观察重影 |
| 宠物 | Pet Inbox 状态流 | Windows 验收 | Inbox、Realtime、suppression、TTL、优先级和动态窗口已接入并测试 |
| 宠物 | 完成、阻塞和运行中任务 | Windows 验收 | 完成/阻塞保留详情，可忽略、标记处理；运行中按精确任务或会话 identity 取消 |
| 宠物 | 审批与 Ask User 原地处理 | Windows 验收 | 本机审批可拒绝/本次/本会话允许；Ask User 完整字段、选项、密文和提交在宠物窗口内处理 |
| 宠物 | 叽咕狸与常用项目快捷聊天 | Windows 验收 | 叽咕狸固定首项、项目常用开关、本地持久化、项目会话准备、最近消息、Realtime 和发送已接入 |
| 设置 | 常规、连接、模型、插件、权限控制、审批 | Windows 验收 | 插件、模型同步、AI 审批 reviewer、审批策略、待处理/审计和 AppContainer 文件/网络边界设置已完成 |
| 设置 | 宠物、语言和字号 | Windows 验收 | SQLite 持久化和运行时即时生效已完成，等待 Windows 视觉与重启验收 |
| 全局效率 | 失效模型清理 | Windows 验收 | 权威目录移除模型后同步清除 SQLite 审批模型 ID，自动化覆盖刷新和 ViewModel 重建；等待设置页与聊天页真机验收 |
| 全局效率 | 全局快速搜索 | Windows 验收 | ChatOS、应用、Indexed Library 文件与内建动作四类 provider、模式前缀、排序、频次和快捷键冲突回退已完成；等待焦点与启动行为真机验收 |
| 全局效率 | 剪贴板历史 | Windows 验收 | 文本、URL、文件和图片采集/恢复、SHA-256 去重、固定、搜索、500 条/30 天清理、20MB 限制与敏感内容过滤已完成；等待跨应用真机验收 |
| 全局效率 | 原生屏幕录制 | Windows 验收 | Windows 原生录屏负责显示器/窗口选择、系统音频、H.264 MP4 与停止条；ChatOS 状态机自动检测完成文件并归档到 `Videos/ChatOS`，等待 Windows 版本和媒体参数真机验收 |
| 安全 | 受控域名网络 | 实现中 | Windows SID 由设备私钥签名连接上报并服务端绑定，域名只从托管权限配置推导；后端策略签发、Relay、exec/ConPTY 挂起进程 lease、每进程 SID、Service/broker、WFP 驱动和端到端脚本均已完成；已增加 Hardware Dev Center CAB/微软签名结果导入，严格区分 unsigned、local_test、microsoft_production，正式验收只接受 Microsoft Hardware Compatibility Publisher；仅剩实际 WDK 编译、微软生产签名及不可绕过真机证据 |
| 发布 | x64 MSIX | Windows 验收 | manifest、品牌资源、隔离的 x64 输出目录、证书签名校验和自动安装/升级/打包启动/UI smoke/卸载证据脚本已接入；待干净 Windows 账号执行 |
| 发布 | ARM64 MSIX | Windows 验收 | ARM64 构建、隔离的未签名/签名包和同一生命周期验收脚本已接入；需 ARM64 Windows 真机执行 |
| 质量 | Core/API 自动化测试 | Windows 验收 | 当前 Core 25、API 45、Presentation 50、Connector 363、NetworkGuard 19，共 502 项测试通过；WindowsNative 和显式启用的 NetworkGuard 端到端测试仍需在 Windows 执行真实系统 API |
| 质量 | Connector 集成测试 | Windows 验收 | Connector 自动化新增覆盖 Agent 团队账号隔离、路由限流、附件 payload/正文按需读取、图片/PDF 多模态 allowlist/签名/数量/字节边界/触发与 Todo 来源上下文、跨会话 Inbox/单调游标/v16 重启、run-scoped opaque refs/跨 Run 失效/持久 ID 拒绝、Markdown 文档草稿/一次性消费/发送 receipt 防重/Inbox 引用回复、Todo 不可变执行合同/来源关系/来源消息有界批量读取/builtin 与 Plugin 能力快照/allowlist 交集/v17 迁移/单执行槽原子调度/优先级/活跃优先有界展示/精确批量依赖读取/集合式依赖释放/隔离 executor/lane 状态写入边界/跨 Todo progress 拒绝/终态 progress kind 拒绝/communication-executor 并发隔离/失败 delivery 原子复活与 durable run 延续/瞬时模型重试/依赖/进展、资产建议/权限/维护唤醒、项目级需求调研/v14 迁移/有界查询、成员提案双权限/幂等/原子审批/v15 持久化、心跳、Responses API、模型工具循环、插件 MCP run session，以及 SSH session-bound MFA 同会话续交/一次性验证码消费；WindowsNative 还验证系统 API、Credential Manager、WinUI 和原生命令执行 |
| 质量 | NetworkGuard 自动化测试 | 实现中 | 当前 19 项单元/服务测试通过；端到端验收要求两个指定测试真实出现在 TRX，覆盖 Microsoft 生产签名门禁、同 IP denied SNI、HTTP/TLS、IP literal、DNS/DoH/QUIC/UDP、无 SNI、子进程、服务/驱动重启和 lease residue=0，等待 Windows 专用验收机生成证据 |
| 质量 | Windows CI | Windows 验收 | Windows 2022 x64/ARM64 restore、串行测试、WindowsNative TRX/JSON、Desktop Release、未签名/签名 MSIX，以及 x64/ARM64 unsigned WDK SYS/CAT/INF/Service 编译与 schema v2 hash 报告上传均已接入；生产驱动签名由 Hardware Dev Center 外部流程完成，统一校验器拒绝 local_test 冒充生产、零测试、缺项、残留或伪通过，等待远端首次运行 |
| 质量 | UI 自动化与可访问性 | Windows 验收 | 87 个稳定 Automation ID 覆盖登录、Shell、设置、聊天、项目导航、需求调研、本机终端、全局审批和宠物关键路径；静态测试校验唯一性/必备项/显式 accessible name，smoke 支持匿名登录页和 Secret 驱动的真实登录后 Shell → 设置路径，等待 Windows CI 首次运行 |

## macOS 3.0.8 新增能力扩展矩阵（2026-10-17）

历史 51/111 只表示合并前旧矩阵基线。以下 17 行是新增跟踪项，目前没有任何一行满足完整 Windows 真实验收条件；子链路通过只写在备注中。受影响的旧行（模型/推理、Task 重试/图、需求调研、插件、附件、本地 Host、设置/打包）在对应 W308 完成前不得用旧状态宣布当前版本完成。

| 编号 | 能力 | 严格状态 | Windows 实现 / 缺口与证据 |
| --- | --- | --- | --- |
| W308-01 | 模型推理能力与 Task/Run 冻结 | 实现中 | Windows 已接 `gpt-5.6-sol` 的 `low/medium/high/xhigh/max` 能力，模型/推理/`medium` 持久化和两次重启恢复通过；重复 runtime-ready 不再重建不变集合，程序应用用 `IsApplyingSettings` 门控。真实陈旧 revision 现在保持 fail-closed 但显示当前目录和中英文恢复指引，只有 Human 显式选择才以乐观版本写入；最终客户端选择 `gpt-5.6-sol` 后重启恢复 `medium`，SQLite version/timestamp 未再次变化。Presentation 57/57、Connector 27/27、Debug x64 0 警告/错误；历史冻结 model ref/revision 展示通过。真实当前 Task/Run thinking-level 冻结和刷新/过滤/禁用/关闭/重试完整矩阵仍缺，见 `.local/parity/2026-10-17-0645/`、`.local/parity/2026-10-17-1115/` |
| W308-02 | 需求补充问答、方案版本/确认 | 实现中 | Windows 已按共享协议读取/校验方案 identity、revision、Markdown 和准确源 Run 状态，UI 只允许 Human 对生成成功的精确 revision 确认；旧成功 final reply 也可由显式 Human UI 保存为 v1 后确认，读取不写入，失败/不匹配来源在保存前拒绝，全链路不创建 Task。Connector/桌面契约 30/30、Presentation 862/862、Debug x64 0 警告/错误，汇总/项目空态、退出和认证重启通过。当前账号无真实调研，真实成功/失败/取消/冲突/确认/持久化仍待验收，见 `.local/parity/2026-10-17-0615/` |
| W308-03 | 方案与 Main Chat/Task 冻结绑定 | 实现中 | Windows 现只从 Task 自身输入解码已确认 snapshot，独立复算/核对 Markdown SHA-256，校验 frozen task scope/title，并在 Task 图展示 revision、正文、prompt、预期交付、验收范围和 hash；不会用当前项目方案替换历史或重试版本。Core 39、Presentation 862、Connector/桌面合同定向 31、Host 冻结 4、最终 Debug 构建、旧 Task 兼容和认证重启通过。当前账号无 plan-bound Task，真实 v1→v2 历史/重试/重启样本仍待验收，见 `.local/parity/2026-10-17-0625/` |
| W308-04 | Task 验收合同/证据/原文/字符约束 | 实现中 | Windows 已严格解析冻结合同和 Run 报告，独立复算 Unicode scalar，映射并展示逐项状态/evidence/最终原文/字符上限，缺失或非法 v1 报告 fail-closed；Core 6/6、Connector 6/6、Presentation 113/113 和最终 Debug 构建通过。历史 Task 兼容与重启真机通过，但无真实 v1 报告样本，成功/失败/not_verified/超限与重启矩阵仍待验收，见 `.local/parity/2026-10-17-0530/` |
| W308-05 | follow-up、补充指令重试与恢复 | 实现中 | 补充指令/无旧写/无虚假 blocker/退出重启子链路真机通过；完整 follow-up/租约/取消仍缺，见 `.local/parity/2026-10-17-0340/` |
| W308-06 | 去重任务图摘要与历史引用 | 实现中 | 终态摘要去重自动化通过；完整 Windows 受影响回归未完成 |
| W308-07 | pluginKey + componentKey 选择/冻结 | 实现中 | 精确 pair schema、Main Chat 显式选择、Task component/Release 快照、漂移 fail-closed、executor 单 component 启动已实现；自动化/构建和无已安装插件空态通过，真实双 component 启动/调用/重启仍待安装确认，见 `.local/parity/2026-10-17-0455/` |
| W308-08 | Solution Studio 双 MCP | 实现中 | 共享包有双 MCP且 Windows component 链已接入；Solution Studio 当前未安装，authoring/execution 真机分离调用仍阻塞于安装确认 |
| W308-09 | 自定义 MCP/Plugin 来源安装与预检 | 实现中 | 本地目录、固定版本 npm MCP 与无凭据 HTTPS Git（明确 branch/tag/commit ref、可选安全 monorepo 子目录）均已接入同一隔离预检/二次获取/真实 tools-list 指纹管线。Git 使用结构化参数、无 shell/无交互、禁 hooks/file protocol/子模块，进程树归入 Job；只读 `.git` 元数据也能不跟随 reparse 安全清理。设置页和独立安装助手均已暴露 Git 字段，普通 Main Chat/Task 仍无安装工具。定向 47/47、Connector 可执行集 691 通过/5 条件跳过、Debug x64 0 警告错误；真实公开 HTTPS fetch 曾成功，后续权威重跑受 `github.com:443` 不可达阻塞并保留失败证据。真实 UI 确认安装/重启、生产 AppContainer、卸载恢复仍缺，见 `.local/parity/2026-10-18-0100/` |
| W308-10 | 独立安装助手和工具隔离 | 实现中 | 独立 `plugin-installer` 联系人/持久会话、immutable installer capability、仅安装工具目录、每次调用 Run+conversation 重授权、边界横幅和普通 Plugin picker 隐藏已实现；普通 Main Chat/Task 无安装工具。Presentation 867、定向合同 55、Host planner 隔离 1、最终 Debug 通过；真机切换/重启和不存在路径 `plugin_installer_inspect_source` 负向路由通过，未安装且无进程/sandbox 残留。21:01 已对齐 macOS 安全错误摘要：仅受信任的 `PluginPackageException` 产品校验原因可返回模型，原生异常/敏感标记强制脱敏并截断；定向 34/34、Debug 0 警告错误及最终产物启动/退出通过。真实 UI 可见文案和成功生产预检/安装仍缺，后者受 W308-09/13 卷根前置阻塞，见 `.local/parity/2026-10-17-1048/`、`.local/parity/2026-10-17-2055/` |
| W308-11 | 主流 Agent Plugin/Skill/MCP 导入 | 实现中 | Windows 已复用 macOS 兼容契约，支持 ChatOS/Portable/Codex/Claude/独立 Skill 与主流 MCP JSON 的 staging-only 归一化、Skill snapshot、真实 stdio `initialize/tools/list`，并对 HTTP/SSE/UI/明文环境/Git/npm fail-closed；真实 Portable Skill 预检、Human 安装、文件完整性与退出残留通过。20:42 修复重启后只枚举 marketplace 的缺口：legacy `custom:` 与新来源记录合并进设置/runtime catalog，checksum-covered manifest 恢复准确元数据，本地启用状态持久化且不调用远端 preference，篡改包保持可见但不可执行，Plugin application 列表/启动同语义；设置返回会取消预检并清除旧指纹。定向 32/32、Connector 可执行子集 631 通过/5 条件跳过、Job 5 次、Debug 0 警告错误和最终产物两次启动/正常退出/包保留/残留归零通过。真实设置页重启可见/启停/刷新、Main Chat Skill 选择、Claude 独立夹具与 Git/npm 仍缺，见 `.local/parity/2026-10-17-1140/`、`.local/parity/2026-10-17-1158/`、`.local/parity/2026-10-17-2025/` |
| W308-12 | canonical REA 包真实 stdio 调用 | 实现中 | Windows 已实现固定版本 npm MCP 获取、禁 scripts、精确包/bin 校验、双重指纹和安装助手专用工具。真实 `rea-agents@6.5.0` 预检/复检、临时安装后重开、207 工具和 `binary_session({})` 调用通过；生产 AppContainer/UI、目录/Main Chat 选择、完整客户端重启及缺分析引擎诊断仍待验收，见 `.local/parity/2026-10-17-2110/` |
| W308-13 | Windows 安装路径/ACL/Job/凭据 | 实现中 | canonical child/reparse/隔离 staging/hash/Job 已有；ACL 更新使用 `RawSecurityDescriptor` + `SetFileSecurity` 写入精确不传播 ACE，支持长路径，避开 `C:\Users`，创建 profile 前校验 workspace，并在 ACL 前写 cleanup journal。停滞、取消和失败残留已修复；最终客户端从设置页走生产 launcher，快速、可见地因本机卷根未预配 traverse ACE fail-closed，正常关闭后 Desktop/Host/Connector/testhost/vstest 和两个 sandbox roots 均为 0。安全安装时卷根前置、npm scripts、Git hooks/file、凭据及完整取消矩阵仍开放，见 `.local/parity/2026-10-17-0945/` |
| W308-14 | Skill 门控/渐进披露/schema 校验 | 实现中 | Windows 已有 schema preflight、disclosure v2 与 Plugin 精确 Skill gate。外部 HTTP MCP 现直接嵌入 canonical `chatos-external-mcp` Product Skill/reference，发布 schema-v2 frozen bundle、capability revision/catalog 及 `on_demand` binding；稳定 SHA-256 identity 覆盖 endpoint/tool/schema/远端 gate 并排除凭据 header。HTTP 前验证 bundle/binding/activation，漂移、缺激活和无可信文档的远端业务 Skill fail-closed，JSON-RPC 错误保留结构化诊断。定向 20/20、受影响 52/52、Connector 可执行集 674 通过/5 跳过、Debug 0 警告错误；真实 Plugin/HTTP MCP 成功、拒绝、分页、取消/错误、持久化/重启仍缺，见 `.local/parity/2026-10-17-2155/`、`.local/parity/2026-10-17-2240/`、`.local/parity/2026-10-18-0005/` |
| W308-15 | 活跃任务配置/凭据刷新和 Host 生命周期 | 实现中 | IPC 42 与历史重启子链路真机通过；Windows 现以 `--disable-workers` 启动 bootstrap Host，并以不保留秘密的 SHA-256 指纹判断凭据集合。同 owner/同凭据刷新不替换 worker，模型或 memory token 变化、跨 owner 刷新均保留活跃 worker并延后，停止后的正常启动才应用新值。生命周期/参数 7/7、合同 21/21、受影响定向 58/58、Connector 可执行子集 639 通过/5 条件跳过、Debug 0 警告错误；真实长 Run/历史凭据/延后提示/重启矩阵仍缺，见 `.local/parity/2026-10-17-2140/` |
| W308-16 | 附件与插件文件交付 | 实现中 | 聊天附件旧主链真机通过；Windows 已对齐 macOS 产物持久化合同，要求 `artifact.create` + 项目 workspace，严格解析候选 schema，校验后复制到项目受管目录并注入精确 Markdown/结构化交付链接，Task/Agent Team/Relay 三入口均 fail-closed 传递真实权限和项目。定向 17/17、受影响 165/165、Connector 可执行集 666 通过/5 跳过、Debug 0 警告错误；真实插件产物点击、预览/导出、项目文件 UI、篡改/取消/重启仍缺，见 `.local/parity/2026-10-17-2255/` |
| W308-17 | 本地化与打包资源定位 | 实现中 | 修复无 NASM 的 x64 Release 工具链、MSIX SDK 工作目录和 arbitrary-CWD Host content root；Debug/Release 0 警告错误，真实无签名 MSIX 含 runtime/Host/PRI/88 帧 Pet/中英文 manifest，独立复制 publish 从 `C:\Windows\Temp` 启动、正常退出和残留归零通过。合同 17/17；签名安装/升级、packaged AUMID、中英文 UI、ARM64、干净账号与符号包仍缺，见 `.local/parity/2026-10-17-2325/` |

2026-10-18 00:36 W308-09 增量：Settings 自定义导入已增加固定版本 npm stdio MCP 模式，结构化收集包/版本/bin/component/参数并复用禁 scripts、真实 `initialize/tools/list`、二次获取和指纹漂移拒绝管线；来源变化取消进行中的预检并作废旧指纹。定向 52/52、x64 Debug 0 警告错误、启动退出归零。真实 UI npm 安装/重启、HTTPS Git、生产 AppContainer 成功和卸载恢复仍缺，状态保持“实现中”，见 `.local/parity/2026-10-18-0025/`。

## P0 空目录项目聊天门禁（2026-10-18 06:39）

| 能力 | 严格状态 | Windows 实现 / 缺口与证据 |
| --- | --- | --- |
| 从真实空目录创建项目并绑定聊天 | 阻塞/真实验收未通过 | `C:\ChatOS-ProjectChat-Acceptance-20261018-01` 已证明 0 项且非 reparse point，但 Computer Use `node_repl/@oai/sky` 未暴露，最终客户端尚未选择该目录；无 project/conversation identity、截图或 UI tree。不得用数据库/API、fixture、mock、已有项目或编译替代。 |
| 项目聊天 list/read/search/write/diff/复制/移动/重命名 | 实现中 | 既有 list/read/search/write 继续绑定来源 conversation→project→workspace。`project_copy/project_move` 现均通过共享 Host 的 `CodeMaintainerWrite + requires_execution` 过滤，Windows 侧要求 Host 审批并复用 path guard；复制增加 staging、覆盖备份恢复、取消清理和 reparse 子项拒绝。revision 为 `native-windows-main-chat-v11-project-copy-move`，C# 权威子集 36/36、Host 1/1、最终 x64 Debug 0 警告/错误；但真实模型调用、审批三分支、磁盘一致性、diff 和重启均未验收。 |
| P0 总体 | 阻塞/真实验收未通过 | 仍缺空目录 UI 创建、首次/已有项目同会话、文件页往返、关闭重启恢复、取消/超时/冲突/重试无重复副作用及清理证据。证据 `.local/parity/2026-10-18-project-chat-real/acceptance-summary.md`；P0 前所有扩展项冻结。 |

2026-10-18 06:50 +08:00：再次确认权威目录 0 项、普通目录、非 reparse point，最终 x64 Debug 产物存在且无产品/测试/构建残留；但工具目录仍无 `node_repl/@oai/sky`。未用替代 UIA 或业务数据注入，以上三行状态与真实验收缺口均不变。

2026-10-18 07:08 +08:00：创建项目动态确认/取消动作增加稳定 `ChatOS.CreateProject.Submit` / `ChatOS.CreateProject.Cancel` AutomationId 和中英文 accessible name；合同 2/2、Desktop x64 Debug 与最终 x64 Debug 均 0 警告/错误。Computer Use 入口仍缺，真实空目录项目尚未创建/操作；目录复核仍 0 项、普通 Directory、非 reparse。最终构建的 4 个 MSBuild 复用节点已正常 shutdown，07:09 残留进程为 0，因此三行严格状态仍为“阻塞/真实验收未通过”“实现中”“阻塞/真实验收未通过”。
## 2026-10-18 07:21 P0 project-chat acceptance note

- Approval UI observability/focus: **implemented + automated (2/2), not real-UI accepted**. Source, risk, exact command, working directory, reason and queue state are targetable; new approvals focus Deny.
- Empty-directory project chat: **blocked / not accepted**. The authoritative directory is still unused and empty/non-reparse; no real project/conversation/tool/file/diff/restart evidence exists because Computer Use `node_repl/@oai/sky` was unavailable.

## 2026-10-18 07:38 P0 create-directory verification note

- Fixed stale-path project creation after a selected-directory refresh failure; Create/Save now require a successfully verified current listing and recover only after retry.
- Exact absolute path and entry/folder/file counts are exposed through stable AutomationIds. Focused results: 34/34 Create Project, 29/29 Desktop automation, final x64 Debug 0 warnings/errors.
- Strict status is unchanged: real empty-directory selection, project/conversation identities, chat tools, approvals, disk/diff, navigation, restart and failure matrix are all unaccepted.

## 2026-10-18 07:53 P0 project-conversation binding note

- Project-chat binding remains **implementation/automation passed, real acceptance not passed**. Remote/stale `LatestConversationId` cannot bypass the Host: first entry per authenticated lifecycle verifies exact `owner + project resource`; invalid identities fail closed, and only verified identities may be reused for files↔chat return.
- `ChatOS.Workspace.Root` exposes selected project and opened conversation identities through UIA. Shell 22/22, combined binding/automation 52/52 and final x64 Debug 0 warnings/errors passed.
- The authoritative directory remains unused and empty/non-reparse because Computer Use is unavailable. No real project identity, Host model/tool operation, approval, file/diff, restart or error-matrix evidence exists; the P0 table statuses remain unchanged and all extensions stay frozen.

## 2026-10-18 08:10 P0 workspace UIA evidence note

- Workspace absolute path, project identity and conversation identity are now separately observable on `ChatOS.Workspace.Root`; navigation buttons publish deterministic selected state. Desktop automation 31/31 and final x64 Debug 0 warnings/errors passed.
- This does not change the matrix: the authoritative directory is still unused, empty and non-reparse because Computer Use is unavailable. Real final-client creation and the complete chat/tool/approval/disk/diff/navigation/restart/error matrix remain unaccepted; extensions stay frozen.

## 2026-10-18 08:22 P0 project-files UIA evidence note

- Project files now expose project/root/current directory, selected file path/content and operation state through stable UIA properties. Desktop automation 32/32 and final x64 Debug 0 warnings/errors passed.
- This is implementation/automation preparation only. The matrix remains unchanged: the authoritative directory is unused, empty/non-reparse, and the complete real create/chat/tool/approval/disk/diff/navigation/restart/error matrix remains unaccepted; extensions stay frozen.

## 2026-10-18 08:37 P0 project-chat UIA evidence note

- Project chat identity/state/turn/message/process/action/error observability is now implemented and automation-verified (Presentation 63/63, Desktop automation 33/33, final Debug 0 warnings/errors).
- Strict state remains **blocked / real acceptance not passed**: the authoritative empty directory is still unused and the real final-client create, Host tool, approval, disk/diff, navigation, restart and error matrix has not run. P1/P2 remain frozen.

## 2026-10-18 08:51 P0 project Git/diff UIA evidence note

- Project Git/diff now exposes stable project/workspace/repository, branch/sync, change-list, diff path/content/empty-state, action-result and error UIA properties with bilingual accessible names. Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed.
- This is implementation/automation preparation only. The authoritative directory remains unused, empty and non-reparse; the complete real create/chat/tool/approval/disk/diff/navigation/restart/error matrix remains unaccepted. P1/P2 remain frozen.

## 2026-10-18 09:25 P0 project-chat idempotent retry note

- Exact resend after an ambiguous project-chat Host failure now preserves the original turn identity, Connector-frozen start command and turn-scoped IPC receipt. Text/attachment/reasoning/capability drift cannot silently reuse the identity. Targeted Connector/Host 16/16, Presentation 59/59 and final x64 Debug 0 warnings/errors passed.
- This is implementation/automation preparation only. Computer Use remains unavailable, the authoritative directory is still unused/empty/non-reparse, and the real create/chat/tool/approval/disk/diff/navigation/restart/error/timeout-retry matrix remains missing. P1/P2 stay frozen.

## 2026-10-18 09:07 P0 project-files empty-directory evidence note

- Project files now expose a stable localized empty-directory state only for an open, completed, successful zero-entry listing, with the exact current path available to UIA. Loading, search, error and closed states do not masquerade as empty.
- Project-files Presentation 141/141, Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed. This remains implementation/automation preparation only: the authoritative directory is still unused, empty and non-reparse, and all real create/chat/Host/tool/approval/disk/diff/navigation/restart/error evidence remains missing. P1/P2 stay frozen.

## 2026-10-18 09:37 P0 cancelled-approval note

- Project write/copy/move approval now carries a live exact-Run gate: only `waiting_tool_result` remains actionable. Stop/cancel, state transition or Host identity failure cancels and removes the pending approval fail-closed, preventing stale decisions and post-cancel filesystem effects. Focused regressions passed 33/33 and final x64 Debug passed with 0 warnings/errors.
- Strict matrix state remains **implementation/automation passed, real acceptance not passed**. Computer Use is unavailable; the authoritative directory is still unused, empty and non-reparse, and the real create/chat/tool/approval/disk/diff/navigation/restart/error matrix remains missing. Evidence: `.local/parity/2026-10-18-project-chat-real/17-project-chat-cancelled-approval.md`. P1/P2 stay frozen.

## 2026-10-18 09:51 P0 project-conversation ambiguity note

- Project-chat restore now reuses exactly one owner/project-bound Host conversation, creates only when there are no matches, and rejects duplicate bindings before creating or opening a chat. Create-conflict recovery uses the same exact-one rule. Workspace/Shell regressions passed 26/26 and final x64 Debug passed with 0 warnings/errors.
- Strict matrix state remains **implementation/automation passed, real acceptance not passed**. Computer Use is unavailable; the authoritative directory remains unused, empty and non-reparse, and the real create/chat/Host-tool/approval/disk/diff/navigation/restart/error matrix remains missing. Evidence: `.local/parity/2026-10-18-project-chat-real/18-project-conversation-ambiguity-guard.md`. P1/P2 stay frozen.

## 2026-10-18 10:07 P0 project-conversation response identity note

- Owner-scoped conversation pages now fail closed on a foreign owner, missing/duplicate conversation identity or repeated pagination cursor. Contact/project create responses must match the exact requested owner, conversation identity and resource binding. Workspace/Shell 32/32 and final x64 Debug 0 warnings/errors passed.
- Strict matrix state remains **implementation/automation passed, real acceptance not passed**. Computer Use is unavailable; the authoritative directory remains unused, 0-entry and non-reparse, and the real create/chat/Host-tool/approval/disk/diff/navigation/restart/error matrix remains missing. Evidence: `.local/parity/2026-10-18-project-chat-real/19-project-conversation-response-identity.md`. P1/P2 stay frozen.

## 2026-10-18 10:25 P0 project workspace failure note

- Workspace path/file failures now carry stable, path-free code/status through the Local Agent project tool result. Deterministic pre-commit rejection is `failed` and correctable; arbitrary side-effecting I/O/native failure remains redacted `needs_review`. Focused 43/43, Connector 715 passed/5 conditional skips/0 failures after excluding three named symbol-link privilege fixtures, final x64 Debug 0 warnings/errors.
- Strict matrix state remains **implementation/automation passed, real acceptance not passed**. Computer Use is unavailable; the authoritative directory remains unused, 0-entry and non-reparse, and the full real create/chat/Host-tool/approval/disk/diff/navigation/restart/failure matrix remains missing. Evidence: `.local/parity/2026-10-18-project-chat-real/20-project-workspace-error-results.md`. P1/P2 stay frozen.

## 2026-10-18 10:50 P0 timeout/cancellation note

- Approval cancellation, Host claim renewal/no-commit on loss, copy staging cancellation, deterministic write/move preflight, and unknown-side-effect `needs_review` were re-audited without finding a new reproducible defect. This is implementation evidence only; no new test/build result is claimed.
- Strict matrix state remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory remains unused and 0-entry/non-reparse, and the real create/chat/Host-tool/approval/disk/diff/navigation/restart/timeout-retry matrix is missing. Evidence: `.local/parity/2026-10-18-project-chat-real/22-project-tool-timeout-audit.md`. P1/P2 stay frozen.

## 2026-10-18 11:05 P0 Computer Use gate note

- The full Computer Use instructions were readable, but no callable `node_repl/@oai/sky` entry was exposed; therefore the final client was not operated and no substitute UIA/data mutation was used. The authoritative directory remains unused, 0-entry, ordinary and non-reparse; final Debug exists with zero relevant process/sandbox residue.
- Matrix state remains **blocked / real acceptance not passed**. No project/workspace/conversation identity or real Host-tool/approval/disk/diff/navigation/restart/failure evidence exists. Evidence: `.local/parity/2026-10-18-project-chat-real/23-computer-use-gate-recheck.md`. P1/P2 stay frozen.

## 2026-10-18 11:23 P0 project-search completeness note

- Project-chat search is now **implemented/automation passed, real acceptance not passed** for incomplete access: enumeration, metadata and read failures set `incomplete=true` and `truncated=true`; capability revision `native-windows-main-chat-v12-project-search-completeness` prevents an exhaustive absence claim. Focused authoritative subset 39/39 and final Debug 0 warnings/errors passed; two broad failures are named pre-product symlink privilege fixtures.
- Overall matrix remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory is still unused/0-entry/non-reparse, and the real create/chat/Host-tool/approval/disk/diff/navigation/restart/failure matrix is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/24-project-search-completeness.md`. P1/P2 remain frozen.

## 2026-10-18 11:36 P0 project-list reparse completeness note

- Project-chat directory listing is now **implemented/automation passed, real acceptance not passed** for reparse-only contents: withheld reparse entries set `incomplete=true` and publish `omitted_reparse_points`, and revision `native-windows-main-chat-v13-project-list-completeness` prevents an exhaustive empty-directory claim. Focused authoritative subset 39/39 and final Debug 0 warnings/errors passed; the direct symlink fixture is blocked by the host privilege prerequisite.
- Overall P0 remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory remains unused/0-entry/non-reparse, and all real final-client create/identity/Host-tool/approval/disk/diff/navigation/restart/failure artifacts are absent. P1/P2 stay frozen.

## 2026-10-18 11:52 P0 project-read invalid UTF-8 note

- Project-chat read/search is **implemented/automation passed, real acceptance not passed** for invalid UTF-8 truthfulness: reads return exact base64 with `is_binary=true`, while content search excludes binary bytes and cannot match decoder-invented replacement text. Authoritative subset 19/19 and final x64 Debug 0 warnings/errors passed; two broader failures are machine-level symbolic-link fixture prerequisites.
- Overall P0 remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory remains unused/0-entry/non-reparse, and all real final-client create/identity/Host-tool/approval/disk/diff/navigation/restart/failure artifacts are absent. Evidence: `.local/parity/2026-10-18-project-chat-real/26-project-read-invalid-utf8.md`. P1/P2 stay frozen.

## 2026-10-18 12:08 P0 project-write diff note

- Project-chat write change evidence is now **implemented/automation passed, real acceptance not passed**: v14 results include bounded unified diff, before/after SHA-256, changed/available/truncated state and stable unavailable reasons. Create, update, no-op, invalid UTF-8 and truncation paths have focused coverage; authoritative subset 43/43 and final Debug 0 warnings/errors passed.
- Overall P0 remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory remains unused/0-entry/non-reparse, and every real final-client create/identity/Host-tool/approval/disk/UI-diff/navigation/restart/failure artifact is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/27-project-write-diff-result.md`. P1/P2 stay frozen.

## 2026-10-18 12:24 P0 project-search reparse completeness note

- Project-chat search is now **implemented/automation passed, real acceptance not passed** for withheld reparse entries: both name and content search set incomplete/truncated and the v15 frozen capability contract prevents an exhaustive absence claim. Non-machine-dependent filesystem/control-plane regressions passed 43/43 and final Debug passed with 0 warnings/errors; the direct symlink fixture remains a machine privilege prerequisite before product execution.
- Overall P0 remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory remains unused/0-entry/non-reparse, and every real final-client create/identity/Host-tool/approval/disk/UI-diff/navigation/restart/failure artifact is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/28-project-search-reparse-completeness.md`. P1/P2 stay frozen.

## 2026-10-18 12:37 P0 project_move case-only rename note

- Project-chat move/rename is **implemented/automation passed, real acceptance not passed** for Windows case-only file and directory names: v16 performs a rollback-safe intermediate move and preserves an exact-retry no-op. Filesystem/control-plane regressions passed 44/44 and final Debug passed with 0 warnings/errors.
- Overall P0 remains **blocked / real acceptance not passed**. Computer Use is unavailable, the authoritative directory remains unused/0-entry/non-reparse, and every real final-client create/identity/Host-tool/approval/disk copy/move/rename/UI-diff/navigation/restart/failure artifact is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/29-project-move-case-only-rename.md`. P1/P2 stay frozen.

## 2026-10-18 P0 project-mutation approval identity note

- Project mutation approvals are now **implemented/automation passed, real acceptance not passed** for exact visible intent: write approvals bind create-only mode, UTF-8 byte count and content SHA-256; copy/move approvals expose replacement intent, with replacement-enabled requests elevated to high risk.
- Approval/Control Plane regressions passed 41/41 and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/30-project-mutation-approval-identity.md`.
- Overall P0 remains **blocked / real acceptance not passed**. No callable Computer Use `node_repl/@oai/sky` exists; the authoritative directory remains unused/0-entry/non-reparse, and real identity/Host/approval allow-deny-cancel/disk/UI-diff/navigation/restart/failure evidence is absent. P1/P2 stay frozen.

## 2026-10-18 13:09 P0 project_write empty-content note

- Project-chat write is now **implemented/automation passed, real acceptance not passed** for present empty/whitespace-only content: v17 approval and execution preserve the exact value and fingerprint instead of treating it as a missing field. Missing/non-string content remains rejected. Affected regressions passed 66/66 and final Debug passed with 0 warnings/errors.
- Overall P0 remains **blocked / real acceptance not passed**. No callable Computer Use `node_repl/@oai/sky` exists; the authoritative directory remains unused/0-entry/non-reparse, and real identity/Host/approval allow-deny-cancel/disk/UI-diff/navigation/restart/failure evidence is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/31-project-write-empty-content.md`. P1/P2 stay frozen.

## 2026-10-18 13:25 P0 Create Project listing identity note

- Empty-directory project creation is **implemented/automation passed, real acceptance not passed** for directory-response identity: a usable listing now proves the selected workspace, exact requested absolute path and derived project relative root are the same identity. Focused regressions passed 36/36 and final Debug passed with 0 warnings/errors.
- Overall P0 remains **blocked / real acceptance not passed**. No callable Computer Use `node_repl/@oai/sky` exists; the authoritative directory remains unused/0-entry/non-reparse, and real identity/Host/approval allow-deny-cancel/disk/UI-diff/navigation/restart/failure evidence is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/32-create-project-listing-identity.md`. P1/P2 stay frozen.

## 2026-10-18 13:38 P0 Create Project folder-item automation identity note

- Empty-directory project creation is **implemented/automation passed, real acceptance not passed** for actionable folder-row identity: each row exposes its directory name and exact path to UI Automation under the stable folder-list anchor. Focused contract 1/1, Desktop automation 34/34 and final Debug 0 warnings/errors passed.
- Overall P0 remains **blocked / real acceptance not passed**. No callable Computer Use `node_repl/@oai/sky` exists; the authoritative directory remains unused/0-entry/non-reparse, and real identity/Host/approval allow-deny-cancel/disk/UI-diff/navigation/restart/failure evidence is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/33-create-project-folder-item-automation.md`. P1/P2 stay frozen.

## 2026-10-18 13:53 P0 Create Project submit revalidation note

- Empty-directory project creation is **implemented/automation passed, real acceptance not passed** for submit-time freshness: the selected directory is force-refreshed and exact workspace/path/relative-root identity is revalidated before registration. Refresh failure, drift or disposal has no create side effect. Focused 40/40, Presentation 878/878 and final Debug 0 warnings/errors passed.
- Overall P0 remains **blocked / real acceptance not passed**. No callable Computer Use `node_repl/@oai/sky` exists; the authoritative directory remains unused/0-entry/non-reparse, and real identity/Host/approval allow-deny-cancel/disk/UI-diff/navigation/restart/failure evidence is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/34-create-project-submit-revalidation.md`. P1/P2 stay frozen.

## 2026-10-18 14:08 P0 Create Project failure/empty-state note

- Empty-directory project creation is **implemented/automation passed, real acceptance not passed** for state truthfulness: `ChatOS.CreateProject.Empty` requires a successful usable listing, so loading/no-permission/Host/listing failures cannot be reported as empty. Desktop automation 34/34, focused Presentation 40/40 and final Debug 0 warnings/errors passed.
- Overall P0 remains **blocked / real acceptance not passed**. No callable Computer Use `node_repl/@oai/sky` exists; the authoritative directory remains unused/0-entry/non-reparse, and real identity/Host/approval allow-deny-cancel/disk/UI-diff/navigation/restart/failure evidence is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/35-create-project-failure-empty-state.md`. P1/P2 stay frozen.

## 2026-10-18 14:19 P0 status clarification

- Implementation and automation preparation do not count as the mandatory real gate: final-client empty-directory project acceptance remains **0 passed**.
- Computer Use still provides no callable `node_repl/@oai/sky`; no substitute UIA/database shortcut was used. Real project/workspace/conversation identities and all Host tool, approval, disk/UI-diff, navigation, restart and failure-matrix evidence remain absent. P1/P2 stay frozen.

## 2026-10-18 14:38 P0 Computer Use native pipe note

- Computer Use runtime registration is now **available but native service blocked**: callable `node_repl` imports `@oai/sky`, while initial, delayed-retry and reset/reinitialize `list_apps()` calls all fail because the app-managed native pipe is absent (`os error 2`).
- Current runtime/helper files are present and signed, but no helper process or `codex-computer-use-*` pipe exists. No manual helper start, alternate UIA or business-data shortcut was used.
- The capability matrix remains **real acceptance not passed**: the empty directory is unused/0-entry/non-reparse and every real identity, Host-tool, approval, disk/UI-diff, navigation, restart and boundary/failure artifact is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/36-computer-use-native-pipe-unavailable.md`. P1/P2/W308 stay frozen.

## 2026-10-18 19:51 P0 real Project Files/approval checkpoint

- Project Files mutual-exclusive rendering: **real acceptance passed for the observed tree state**. The final client shows one TreeView and one actionable row per current disk file; the former bright/dim duplicate layer is gone.
- Approval allow-once/readability: **real acceptance passed for this allow-once sample**. All three decision labels are readable, the overlay does not cover the composer, and the exact approval-bound 40-byte write completed once with matching SHA-256.
- Overall empty-directory project-chat P0: **real acceptance in progress, not passed**. Cancellation cascade, copy/move/rename, the full path/permission/reparse/conflict/cancel/timeout/retry/persistence/restart matrix and final cleanup remain open. Evidence: `.local/parity/2026-10-18-project-chat-real/37-project-files-approval-ui-real.md`.

## 2026-10-18 20:49 P0 real Stop checkpoint

- Project-chat Stop/cancellation: **real acceptance passed for pre-Task and active-child-Task timing states**. Pre-Task cancellation produced no Task graph, Task, tool or file. Active-child cancellation atomically cancelled the parent Run, Task and child Run, cleared the active Run identity and produced no child tool/file side effect.
- UI/persistence/disk/cleanup evidence is recorded in `.local/parity/2026-10-18-project-chat-real/38-project-chat-stop-real.md`; supporting storage 108/108, Desktop automation 35/35 and final x64 Debug 0 warnings/errors remain green.
- Overall empty-directory project-chat P0 is still **real acceptance in progress, not passed**. Copy/move/rename, complete boundary/permission/reparse/missing/conflict/timeout/retry/idempotency, remaining restart/persistence and final cleanup coverage remain open. P1/P2/W308 stay frozen.

## 2026-10-18 21:10 P0 real copy/move/rename checkpoint

- Project copy and same-directory move/rename are now **real acceptance passed for the observed non-overwriting paths** in the same final-client project/conversation. Both exact `replace-existing=false` approvals were visible, approved once and persisted as succeeded invocations.
- Disk truth is exact: the original source remains, the intermediate copy is absent, and the renamed file is 40 bytes with the expected unique marker and SHA-256. The repaired Send layout is also really verified clear of the Pet window. Evidence: `.local/parity/2026-10-18-project-chat-real/39-project-chat-copy-move-send-ui-real.md`.
- Overall P0 is still **real acceptance in progress, not passed**. Permission/absolute-path/drive/UNC/ADS/reparse/missing/conflict, approval deny/cancel, timeout/failure retry without duplicate effects, remaining persistence/restart, affected regressions and final cleanup are open. Delete was not requested; P1/P2/W308 stay frozen.

## 2026-10-18 21:30 P0 real read-boundary status

- Project read boundaries are **real acceptance passed for traversal, absolute path, alternate drive, UNC, ADS and missing-entry cases** in the same final-client project conversation. Five unsafe path shapes returned HTTP 400 `workspace_path_unsafe`; the missing in-project entry returned HTTP 404 `workspace_entry_missing`.
- All six persisted invocations are read-only, require no approval and have no side effect. The Task/Run/Turn succeeded, and normal close left no product/build/test or sandbox residue. Evidence: `.local/parity/2026-10-18-project-chat-real/40-project-chat-boundary-read-real.md`.
- Overall P0 remains **real acceptance in progress, not passed**. No-permission, reparse, conflict, approval deny/cancel distinction, timeout/failure retry with no duplicate side effect, remaining persistence/restart, affected regressions and final cleanup are open. P1/P2/W308 stay frozen.

## 2026-10-18 22:17 P0 real conflict and approval-cancel status

- Non-overwrite conflict is **real acceptance passed for the observed project-copy case**: one approved `replace_existing=false` call returned 409 `workspace_copy_target_exists` and left both exact 40-byte files unchanged.
- Approval-waiting cancellation is **real acceptance passed for the observed Task Graph cancel case**: the Task/Run/callback/node reached `cancelled`, the sole `project_write` failed before execution with no approval decision, the pending approval UI cleared and the target remained absent. This is distinct from approval rejection and ordinary Stop.
- Evidence is `.local/parity/2026-10-18-project-chat-real/41-project-chat-conflict-real.md` and `43-project-chat-approval-cancel-real.md`; normal close left no process/sandbox residue. Overall P0 remains **real acceptance in progress, not passed**. Strict decline, no-permission, reparse, timeout/failure retry without duplicate side effect, remaining persistence/restart, affected regressions and final cleanup are open. P1/P2/W308 stay frozen.

## 2026-10-18 22:40 P0 real strict approval rejection status

- Strict local rejection is **real acceptance passed for the observed `project_write` case**: one exact create-only approval was rejected through `ChatOS.Approval.Deny`; persistence records one failed write with `approval_status=rejected`, the Windows local decision actor and the exact decline error. No retry or alternate mutation occurred and the target remains absent.
- Restart persistence is also passed for this sample: the final client restored the same project/conversation and rejected callback, then closed cleanly with zero relevant processes and no sandbox residue. Evidence: `.local/parity/2026-10-18-project-chat-real/44-project-chat-approval-deny-real.md`.
- Overall P0 remains **real acceptance in progress, not passed**. No-permission, reparse, timeout/failure retry without duplicate side effects, remaining persistence/regression coverage and final full-matrix cleanup remain open. P1/P2/W308 stay frozen.
## 2026-10-19 00:28 P0 project-chat timeout/retry matrix update

| Capability | Windows status | Evidence / remaining gap |
|---|---|---|
| Host timeout before task persistence | Real acceptance passed for observed sample | Timed-out submission returned `A task was canceled.`, retained composer text and left zero Turn/graph/Task/tool/file side effects. |
| Exact retry / idempotent mutation | Real acceptance passed for observed sample | One Turn/graph/Task/Run, one approved `project_write`, one `project_read`, zero alternate mutations; exact 37-byte content, SHA-256 and create diff. |
| Succeeded callback display | Implemented + automation + real UI/restart passed | Negated timeout/failure text is preserved; no false “服务暂时不可用”. Storage 109/109, final Debug 0 warnings/errors, same callback after restart. |
| Complete P0 project-chat baseline | In progress, not passed | macOS Main Chat/Product Skill/internal bundles/Task invariants, remaining Host-not-ready/loading/permission/reparse coverage, affected regressions and final cleanup remain open. |

Evidence: `.local/parity/2026-10-18-project-chat-real/46-project-chat-timeout-retry-real.md`.
