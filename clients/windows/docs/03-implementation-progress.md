# Windows 客户端实施进度

本文只记录已经落到代码并经过对应层级验证的能力。完整范围和顺序见 `01-windows-client-implementation-plan.md`，产品对齐状态见 `02-capability-parity-matrix.md`。

## 已完成并可跨平台验证

- `.NET 8` Core/API 分层和 xUnit 测试工程。
- 认证：登录、Token 保存接口、会话恢复、退出、401 自动清理。
- Workspace：并行读取项目、联系人、会话；metadata 兼容对象和 JSON 字符串。
- 不使用 `-1` 作为无项目会话的哨兵值；项目身份保持 nullable，由资源路由显式处理。
- Realtime：WebSocket ticket、HTTP 到 WS/WSS 地址转换、用户/会话订阅、分片消息、安全大小限制、指数退避。
- Pet Inbox：列表、处理/忽略/确认、Realtime 更新、sequence 去旧、TTL 和优先级 reducer。
- 宠物处理防复活：先写本地 stable-identity suppression，再提交服务端；失败时回滚；旧 Run 的忽略状态不会压制新 Run。
- Ask User：字段、密文、多行、单选、多选、取消和提交协议。
- 聊天命令：权威运行配置、模型选择、Reasoning、Plan、新轮次、追加指导和停止。
- 会话历史：compact-history 分页、Turn 归并、最终回复、任务回调、附件和过程摘要映射。
- 附件：预签名目标获取、PUT 上传、header allow handling，网关 Bearer Token 不转发到对象存储。
- 会话状态机：revision/generation 防旧页覆盖、跨页排序、Realtime 去重、未读新动态和 Project Execution replacement。
- 会话 SQLite 缓存：启动先展示缓存，再以服务端最新页校正；损坏缓存行不会阻塞在线历史。
- Presentation：聊天、Ask User、Realtime 过程、附件草稿和项目文件状态均可脱离 WinUI 做自动化测试。
- 项目默认会话：项目无会话时按 macOS 规则绑定默认联系人并复用或创建会话，不使用 `-1` 哨兵。
- 项目文件 API：目录、文件名搜索、内容搜索、读取、写入、创建、删除和外部打开。
- 项目文件安全变更：对齐后端 `/fs/move` 做原子重命名，不使用复制后删除；创建、重命名和删除后强制刷新权威目录，名称在请求前做基础边界校验。
- Project Plan：需求层级、Work Item、依赖边、文档、Execution Plan 查询和创建。
- Message Task Graph：真实任务标题和状态、任务层级、Run identity、Run 详情、事件分页、结果/错误、MCP/tool payload、取消和重试。
- Project Run：Catalog、分析、运行状态、环境预检、工具链、环境变量、配置文件、默认目标、启动、停止和删除 API。
- Project Run 状态机：项目 session、加载 generation、mutation generation 和 state request generation 分离；旧轮询和旧操作不能覆盖新项目或新状态。
- Project Run 环境草稿：保存失败时保留用户尚未提交的工具链和环境变量输入。
- Project Execution：使用 project、requirement、execution group、conversation 和 contact 完整身份确认或停止执行；刷新携带精确 query，避免读到同一需求的其他执行批次。
- Project Execution 状态机：确认、停止后重新读取权威状态；旧项目操作不能覆盖新项目；停止明确发送 `discard_tasks: true`。
- Connector 连接状态机：连接 generation、停止、睡眠挂起、恢复、失败和重连状态分离；旧 socket 的连接、pong、失败和清理不能覆盖新连接。
- Connector 重连与心跳：1/2/4/8/16/30 秒有界退避，15 秒心跳，连续 3 次缺少 pong 主动断线；长 Relay 请求不阻塞接收 pong。
- Connector Relay dispatcher：先校验 owner/device 和平台签名，再按消息类型分发；错误响应保留 `request_id`，单连接最多并行处理 8 个长请求。
- Relay 平台签名：对齐服务端 canonical JSON 和 Ed25519 协议，校验时间窗口、可信 key id、签名和 nonce 防重放；验签失败不消耗 nonce。
- Workspace 文件 Relay：目录、读取、文件名搜索、内容搜索、创建、写入、删除和移动均返回原地详情；搜索有 2 MB 文件限制、20,000 次访问限制、3 秒超时和结果上限。
- Workspace 路径安全：同时拒绝绝对路径、盘符、UNC、`..`、NTFS ADS、非法 Windows 文件名、symlink 和 junction/reparse point；不能修改根目录或把目录移入自身。
- Workspace 覆盖移动：目标先转为同目录备份，源移动成功后再清理，失败时恢复目标，不使用“先删除目标再移动”。
- Connector SQLite 权威状态：配对用户、设备、工作区和远控 trust 原子保存；Runtime Context 只发布已持久化状态，未登录或未配对时不建立 WebSocket。
- Connector pairing：ticket exchange 使用独立 `1.0.0-windows` 客户端身份；设备注册校验 owner 和本机公钥；工作区按设备私钥加盐 fingerprint 复用、迁移或创建。
- Connector pairing 本地提交：远端设备、工作区和 managed trust 全部成功后才提交 Token 与 SQLite 状态；本地持久化失败会恢复之前的 Token。
- Connector managed config：每 60 秒刷新远控签名 trust，失败保留已持久化快照；trust 更新即时用于后续 Relay 验签，但不重启健康的 WebSocket。
- Terminal 会话状态：同一 session id 只创建一个原生会话，不能切换到其他 workspace/cwd；已退出会话可安全重建，关闭和 Connector 断线都会回收全部会话。
- Terminal 输出：UTF-8 增量解码、512 KB 有界 ring buffer、按行 snapshot；output/snapshot/exit/state/error 使用服务端现有终端事件协议。
- Terminal Relay：`terminal_session_create_request`、input、command metadata、resize、snapshot 和 close 已接入；控制消息同样先做 owner/device/Ed25519 校验，另一工作区不能控制已有 session。
- Terminal Exec Relay：`terminal_exec_request` 已接入 workspace/project/cwd 三层边界、命令/参数大小限制、精确 Relay identity、超时 clamp 和 `terminal_response` 完整结果；超时使用 Relay 408。
- 命令审批状态机：默认 `request_approval`；支持本次允许、本会话允许和拒绝，重复 resolve 不生效；`full_control` 必须显式确认风险；`auto_approval` 使用 Windows AI reviewer，模型、密钥或插件 Prompt 不可用时安全回退用户。
- 审批 identity：pending approval 使用 owner、device、workspace、request 的稳定身份；本会话 allowlist 使用 workspace、project、cwd 和完整命令参数哈希，允许 `git status` 不会顺带放行同目录下的 `git reset --hard`。
- 审批断线清理：Connector 断线会拒绝全部 pending approval、清空会话 allowlist 并释放所有等待中的 Relay task；审计写入异常也不会留下悬挂 continuation。
- 审批与命令历史：默认模式、审批 reviewer/risk/reason 和命令 exit/timeout/stdout/stderr/truncation 元数据分别进入独立 SQLite 表，最多按 1,000 条读取。
- 一次性命令输出：stdout/stderr 始终并发 drain，总字节数与 512 KB preview 分开记录，超过预览限制不会堵塞子进程管道。
- Windows 命令进程：`CreateProcessW + CREATE_SUSPENDED`，加入 Kill-on-close Job 后才恢复；加入 Job 失败时也显式终止挂起进程，超时/取消终止完整进程树，stdin 启动后立即 EOF。
- Windows 句柄边界：一次性命令使用 `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`，子进程只继承 stdin/stdout/stderr 三个指定句柄，不会继承客户端内其他可继承文件、凭据或同步句柄。
- Windows 可执行文件解析：裸命令只从绝对 PATH 目录解析，不使用当前项目目录的同名可执行文件；相对可执行路径必须仍在 workspace 且不能穿过 symlink/junction。
- 插件安装与设置：Catalog/Release、完整性与平台校验、安装/升级/卸载补偿、启停、权限、Secret 和 OAuth 均接入真实服务。
- HTTP MCP：HTTPS/loopback、network domain 权限、Secret header、OAuth Bearer、initialize/tools/list/tools/call、取消、超时和大响应边界。
- 插件 Artifact：Host 重写 descriptor，拒绝绝对路径、穿越、ADS、reparse point、MIME/扩展名不符、超限和篡改文件。
- Visual Session 数据层：conversation/turn/message/task/run owner、多 adapter、精确 host identity、选中帧按需加载和生命周期清理。
- Windows Visual Session 画中画：650 ms 元数据刷新、只为选中 adapter 读取帧、generation 防旧帧覆盖、置顶不激活主窗口、无会话自动隐藏和单画面关闭抑制。
- Windows Artifact 中心：全局或按 adapter 列表、原始插件路径不出 UI、受控缓存打开、FileSavePicker 另存、同目录临时文件原子替换和失败清理。
- Windows 审批设置：三种模式、升级风险确认、等待处理原地决策、最近审计记录、AI reviewer readiness 和安全回退状态。
- Windows 模型设置：同步服务端已启用模型，本机 SQLite 保存审批模型 ID 和请求重试次数，支持清除选择并识别已失效模型。
- Windows AI reviewer：从插件管理读取 Prompt/Capability，校验 Prompt SHA-256，调用已选择的 OpenAI-compatible 模型并只接受结构化 approve/deny/ask_user/remember_allow；请求不包含绝对工作区路径，API Key 和供应商错误正文不进入 UI/日志。
- Windows AppContainer 沙箱：ReadOnly / WorkspaceWrite / FullAccess、Disabled / Host 网络、工作区 ACL、环境变量白名单和隔离临时目录；一次性命令与 ConPTY 都在挂起状态加入 AppContainer 和 Kill-on-close Job。
- Windows 本机终端资源：支持从多个已配对 Connector 工作区创建独立终端、真实 ConPTY 输入/输出、停止/关闭、200,000 字符界面上限和退出登录进程树回收。
- Windows 全量国际化：全部 Desktop XAML 中文硬编码已收口到 `LocalizationViewModel`；DataTemplate 使用应用级本地化源，联系人/Connector/终端资源标题和状态随语言即时切换。
- Windows CI：Windows Server 2022 上串行运行五套测试，并分别构建 x64/ARM64 Desktop Release 与 unsigned NetworkGuard WDK 包；驱动 job 必须产出 SYS/CAT/INF、Service、SHA-256 manifest 和 schema v2 `signing_mode=unsigned` build report，构建脚本验证真实 exe/MSIX/AppX，防止空成功。
- MSIX：单项目 Package manifest、x64/ARM64 未签名/签名打包、构建时 PNG 品牌资源和 CI 产物上传已接入；证书从 CI Secrets 临时导入，校验 Publisher/Subject、Authenticode、Thumbprint 和时间戳后清理。
- Windows UI smoke：76 个稳定 Automation ID 已覆盖登录、Shell、设置、聊天、项目导航、本机终端、全局审批和宠物；静态测试校验 ID 唯一性、关键 ID 完整性和显式 accessible name。x64 CI 默认验证匿名登录页，也可从进程环境安全读取测试凭据，清除后再启动客户端并验证真实登录、Shell、账号菜单和设置页。
- WindowsNative 验收：真实运行一次性 Win32 命令、超时后验证 Job Object 没有子进程逃逸、验证 AppContainer 工作区可写/外部拒绝/只读拒绝和零网络 capability、真实 ConPTY 输入输出、Credential Manager 随机 Secret 写入读取删除。
- Windows 验收证据：`build/native-acceptance.ps1` 输出 TRX 与 `acceptance-report.json`，记录系统/架构、实际检查项和 exe/MSIX SHA-256，并重新解析 TRX，要求 9 个指定 WindowsNative 测试和至少 19 个 NetworkGuard Service 测试实际执行且全部通过；不记录 Token、Secret 或异常正文，CI 自动上传。
- MSIX 生命周期证据：`build/msix-lifecycle.ps1` 读取包内 manifest，拒绝碰触已有用户安装，验证签名或显式 unsigned 模式、可选旧版升级、AppUserModelId 启动、打包 UI smoke 和卸载清理并输出 JSON；`ui-smoke.ps1` 同时支持 unpackaged exe 与 packaged app。
- Windows 证据防伪：MSIX 使用逐阶段 schema v2，native/NetworkGuard 使用 schema v3，记录 requested/passed 和 failure stage；MSIX 清理残留会回写失败，native/NetworkGuard 保存 TRX 相对路径、测试名、执行/通过/失败数和 hash，最终校验器会重新读取 TRX；NetworkGuard 另记录 SYS/CAT hash、签名状态与 signer thumbprint。
- Windows 生产驱动签名：`networkguard-submission.ps1` 生成并签名 Hardware Dev Center CAB；`networkguard-import-production.ps1` 导入 Microsoft 返回结果，核对 INF、Microsoft Hardware Compatibility Publisher、时间戳并重建 manifest/build report。签名模式区分 `unsigned`、`local_test`、`microsoft_production`，生命周期默认拒绝前两种。
- Windows 最终验收：`verify-windows-acceptance.ps1` 同时校验 native、MSIX、NetworkGuard 和 x64/ARM64 WDK 报告，并对实际当前/旧版 MSIX、SYS、CAT、manifest 与所有 WDK artifacts 重新计算长度和 SHA-256；正式模式只接受 x64/ARM64 `microsoft_production` 包，`windows-final-acceptance.ps1` 在明确确认的一次性管理员测试机上执行当前架构原生/认证 UI、MSIX 生命周期、disruptive NetworkGuard 与卸载，并把交付物复制到独立 deliverables 后生成两份汇总。
- MSIX 输出隔离：`build/package.ps1` 每次只清理并生成当前 `AppPackages/<Platform>`，不会用其他架构或旧运行留下的包伪造成功。
- Windows 启动入口：正式脚本为 `scripts/start-client.ps1`，启动前分别检查 ChatOS API 与 Local Connector `/api/health`、按架构构建并注入 API/Connector 地址；历史误命名的 `start-server.ps1` 只保留兼容转发。
- Controlled 服务端签发：Local Connector 使用独立 Ed25519 私钥签发 owner/device/workspace/Windows SID 绑定策略，TTL、私钥路径和 Key ID 已进入 Config Center；Windows SID 只能由设备私钥签名的 v2 WebSocket 连接注册，已绑定设备若切换 SID 必须重新配对。
- Controlled 权威策略来源：ChatOS terminal exec/session 只发送空的策略申请，不接受普通 UI 传入 SID、域名或签名；Local Connector 按 global → role → user 合并托管权限层，从默认 permission profile 的 allow 域名生成 80/443 策略。缺少 signer、SID、默认 profile 或 allowlist 时不生成策略，Windows Controlled 执行保持 fail closed，Host/Disabled 不受影响。
- Controlled 设置门禁：Windows 设置页同时检查本机 NetworkGuard 和设备级服务端 readiness；未配对、SID 尚未注册、signer/allowlist 缺失或策略不可编译时会直接给出可操作提示，不再允许用户先保存、等运行时才发现失败。
- Controlled Relay/执行接线：terminal exec 与 ConPTY 接受服务端 `network_policy`，在审批前校验 Relay identity；进程保持 suspended，加入 Kill-on-close Job、成功 acquire lease 后才恢复，续租失败立即终止完整进程树。
- NetworkGuard Connector 侧：v1.0 版本协议、256 KB length-prefix JSON 分帧、correlation 校验、健康/driver/self-test/active lease readiness、lease 获取/续租/释放和续租失败 fail-closed 回调已实现。
- NetworkGuard pipe 防冒充：连接后读取服务端 PID/Token，只接受安装脚本固定使用的 LocalSystem SID；LocalService、NetworkService 和普通用户进程均不能通过抢占同名 pipe 接收策略。
- NetworkGuard broker 判定核心：HTTP/1 Host、absolute-form/重复 Host/非 ASCII 拒绝，TLS ClientHello SNI、跨 TLS record 重组、无 SNI/ECH fail closed、精确域名和单层通配符判定已实现。
- AppContainer 隔离与回收：普通沙箱按 workspace/permission 隔离；Controlled 每次 exec/ConPTY 使用独立 SID，且不授予 Internet/Private Network capability，只由 WFP 显式放行 broker。最后一个进程退出后删除临时 profile、撤销 workspace ACL 和临时目录；崩溃遗留由受保护元数据做 26 小时有界 GC。
- NetworkGuard 重启一致性：驱动健康协议暴露 active lease count；服务启动先执行 reset IOCTL，残留不为零则 broker 不启动。服务依赖驱动启动，升级会移除旧 OEM INF，安装前校验 SHA-256 manifest 与 SYS/CAT 签名一致性。
- NetworkGuard 验收：`build/networkguard-acceptance.ps1` 生成独立测试密钥、安装/升级组件、执行真实签名策略与不可绕过探测，并要求两个指定 E2E 测试都出现在 TRX 且通过；输出 TRX、服务状态和 SHA-256 证据，私钥在报告生成前删除。
- Windows 宠物状态机：Inbox/Reatime/suppression 与窗口连接，完成和阻塞不会在用户查看前消失；运行中支持精确取消。
- Windows 宠物快捷聊天：叽咕狸固定首项、常用项目本地持久化、项目会话准备、独立 Conversation Session、历史/Realtime/发送。

## 已完成 Windows C# 编译，等待真机运行

- Windows Credential Manager Token Store。
- Windows Credential Manager 设备私钥存储；Ed25519 私钥不进入 SQLite，损坏密钥自动废弃并生成新设备身份。
- `ClientWebSocket` Connector transport：设备签名 headers、4 MB 单消息上限、发送串行化、独立 Relay 任务和可取消关闭。
- Connector Gateway HTTP：pairing ticket、设备、工作区迁移/创建、离线标记和 runtime trust 使用真实 `/api/local-connectors/*` 协议。
- Connector 后台宿主：SQLite 初始化先于 Hosted Service；无配对/无 Token 时等待状态变化，不进行无效重连。
- Windows ConPTY：`CreatePseudoConsole`、扩展启动属性、暂停创建进程、加入 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` Job Object 后再恢复线程，避免子进程抢跑逃逸。
- Windows ConPTY 生命周期：输入/输出管道、resize、显式终止 Job、等待 exit、关闭 pseudo console 和 SafeHandle；Connector 断线时终止完整终端进程树。
- Windows 全局审批浮层：主窗口直接展示待处理命令、cwd、来源、风险原因和排队数量，可原地拒绝、本次允许或本会话允许，不需要跳回设置页面。
- SQLite WAL 初始化和 schema version。
- `ui_state`、`conversation_cache`、`conversation_cursor`、`pet_preferences`、`pet_activity_suppression`、Connector/插件/终端/诊断状态表。
- WinUI 主窗口、登录卡片、固定 Toolbar、联系人和项目侧栏、设计 token。
- WinUI 聊天页：历史、过程、Ask User 原地处理、Composer、停止、模型、Reasoning 和 Plan。
- WinUI 附件：文件选择、拖放、剪贴板图片、长文本转附件、20 MB 限制、失败恢复。
- WinUI 项目工作区：聊天和文件作为平级顶部入口；文件默认只读预览，显式进入编辑后才可保存。
- WinUI 项目文件操作：可新建文件/文件夹、逐项重命名和删除；删除文件夹前明确说明递归删除且要求确认。
- WinUI 项目计划：需求层级、任务、文档和执行计划作为独立平级入口展示。
- WinUI 消息任务图：从聊天任务回调原地打开右侧详情面板，不跳离当前应用页面；支持事件分页、取消和重试。
- WinUI 项目运行：作为聊天、文件、计划的平级顶部入口，支持目标分析、预检、默认目标、实例日志、停止/删除、工具链和环境变量配置。
- WinUI 项目计划：等待确认的执行计划可原地确认；运行中、失败或未启动的计划可原地停止/放弃，并在清理任务前显示确认对话框。
- WinUI 插件设置：安装/更新/卸载、启停、权限、Secret 保存/清除和 OAuth 授权/断开。
- WinUI Visual Session：独立置顶 ToolWindow、多视觉会话切换、PNG/JPEG 内存帧解码和 Artifact 入口。
- WinUI Artifact 中心：主 Toolbar 全局入口与 Visual Session 过滤入口。
- WinUI 审批设置：模式、待处理和审计历史。
- WinUI 模型设置：同步、审批模型选择/清除、0–10 次请求重试和保存反馈。
- WinUI 全局宠物：透明置顶、多显示器物理坐标拖动、左右动画、通知详情、审批、Ask User、任务取消和快捷聊天。

## 当前验证结果

- Core tests：20 项通过。
- API tests：49 项通过。
- Presentation tests：52 项通过。
- Connector tests：242 项通过，其中 9 项 WindowsNative 和 2 项显式启用的 NetworkGuard 端到端测试在非 Windows 环境只验证编译和测试发现；新增 Desktop Automation ID 静态契约测试。本轮通过隔离的 .NET 8 SDK 容器重新编译并运行，242 项全部通过。
- NetworkGuard tests：19 项通过。
- 共 382 项自动化测试通过；Core/API/Presentation/Connector/NetworkGuard：0 个失败。
- 20 个 Desktop XAML 已通过 XML 静态校验，76 个 Automation ID 无重复，XAML 中文硬编码扫描为 0；GitHub Actions YAML 已通过静态解析，20 个 `build/*.ps1` 已使用 PowerShell 7.4 parser 全量通过语法检查。NetworkGuard manifest 正常/篡改/路径逃逸及 TRX 正常/目标缺项/失败结果测试均符合预期；最终验收校验器覆盖普通成功、完整升级、缺少升级证据、MSIX 篡改、local_test 正式拒绝/显式开发允许、缺少 NetworkGuard E2E 测试和 WDK 路径逃逸。WinUI 依赖已成功 restore；macOS 上 Windows App SDK XAML 编译器会因缺少 `kernel32.dll` 停止，必须在 Windows 构建机继续 XAML 编译和运行验收。

## 下一实施批次

1. 触发新增的 Windows 2022 WDK x64/ARM64 job，取得首轮无签名编译证据；通过 Microsoft Hardware Dev Center 分别取得 x64/ARM64 `microsoft_production` 包，再在 Windows 10/11 x64 和 Windows 11 ARM64 执行 NetworkGuard disruptive 端到端验收，通过前 Controlled 不提升为 `Windows 验收`。
2. 在 Windows 11 x64/ARM64 完成 WinUI、AppContainer/ACL、ConPTY、Credential Manager、SSH/SFTP、宠物多 DPI、MSIX 安装升级卸载和签名信任链验收。
3. 触发 Windows CI 首次远端运行；为仓库配置 `CHATOS_UI_TEST_USERNAME` / `CHATOS_UI_TEST_PASSWORD` 后手动启用 authenticated smoke，收集 Shell → 设置关键路径证据。
