# Windows React 与 macOS 功能拉齐关闭清单

更新时间：2026-11-06

分支：`3.0.9.3.1`

Windows 产品版本：`3.0.9`

本清单区分“页面已出现”“业务闭环已完成”“平台能力真实可用”和“发布环境已验证”。只有四项同时满足，才算完成；不再把只读占位、失败关闭或测试未运行写成已拉齐。

## 当前复核（2026-11-03）

以下结论以当前分支 `3.0.9.3.1` 的最新代码、测试和最终 NSIS 安装包为准。文档后面的编号条目保留为历史交付记录，其中较早的“部分完成”措辞不应覆盖本节的最新状态。

- Hook 链路当前已完成：最终 NSIS 安装包能够真实启动 AppContainer runner，写入并激活受信 Bundle，触发 `UserPromptSubmit`，回传上下文，按 `continue=false` 阻断 Prompt，并在 Host 重启后恢复 active Bundle。证据：`%TEMP%\\chatos-windows-installed-hook-restart.json`。
- 登录入口当前已完成：原生安装版首屏进入 owner-scoped 项目工作区，不再默认落到设置页。证据：`%TEMP%\\chatos-windows-workspace-entry-nsis.json` 与本机原生窗口观察记录。
- 安装包原生启动 smoke 已增加硬门禁：每次启动都必须观察到安装目录中的 `chatos-windows-react.exe` 自己创建的非零窗口句柄，且窗口标题必须精确为 `ChatOS`，不能只靠进程退出码把浏览器、空进程或错误壳层误判为客户端。最新报告 `%TEMP%\\chatos-windows-native-title-smoke.json` 通过（安装版启动 `3,172 ms`、窗口标题 `ChatOS`、卸载成功、隔离用户数据已清理）。这仍不替代 31 张 macOS 截图的逐页视觉走查。
- 插件应用 Bridge 当前与 macOS 的真实实现一致：两端只实现 `host.context.read`；`artifact.*` 等尚未闭环的能力不会在 Windows 中伪装成可调用能力。
- 静态 Plugin 页面当前补齐 `navigate-to 'self'` CSP，页面不能自行导航到外部地址；`local_http` 后端仍保持现有 loopback/origin 门禁，未把尚未解决的 AppContainer loopback 代理问题伪装成完成。
- Windows Local Agent Host 当前已读取并应用 `/api/local-connectors/config/runtime` 的受管运行时配置：模型重试次数、上下文压缩阈值、Task 最大迭代次数均写入 Host 的模型/能力快照；请求/运行超时和无进展轮数也会做范围校验，网络失败时继续使用已持久化快照进行离线恢复。代码提交 `6de98e76f`，Windows Tauri Rust 全量测试 `541 passed, 6 ignored`。
- 任务终端已接入独立 AppContainer profile、项目目录读写租约、默认禁网、交互式 stdin/stdout/stderr、超时与进程树回收；绝对路径读写和租约恢复已有真实 Windows 测试。租约现在会在运行期间为工作目录父级临时授予仅 `FILE_TRAVERSE`，并在正常释放、启动失败和 journal 恢复时按 AppContainer SID 撤销，父级不获得列举、读写权限。
- Plugin 应用后端的生产启动路径已接入独立 AppContainer：安装目录只读、数据与缓存目录读写，并按 manifest 的 `network.client` 选择 `loopback`/`controlled` capability；本机实测 `CheckNetIsolation LoopbackExempt` 返回 Win32 5（需要管理员权限），不能放进普通用户启动路径，Windows loopback exemption/proxy 尚未闭环，健康检查仍可能超时，测试构建继续使用 Host runtime 以保留既有健康检查覆盖。
- Agent Plugin invocation journal 现在在每次校验私有 journal 根目录时，额外有界清理超过 24 小时的 canonical `.<小写 UUID>.tmp` 残留；清理复用普通非 reparse/单硬链接所有权检查，最多扫描 2,000 项，未知名称和近期临时文件保留。定向 journal 测试覆盖 stale、fresh、malformed 和扫描上限。
- OpenSSH MFA 验证码提交与 AskPass 消费现在都通过同目录私有临时文件原子替换，读取方不会看到半截验证码；临时运行目录的既有有界清理会回收崩溃遗留。
- Computer Use Windows worker 的本机协议门禁已补齐：`npm run smoke:windows` 现在同时验证 `--doctor`、真实 stdio `initialize`、`tools/list`、11 个工具的 Skill gate 映射、`scroll` 的 `-1200..1200` 像素边界和只读 `tools/call(check_permissions)`，严格确认 `visual-computer-use` 身份、工具契约及权限快照字段；仓库自带 worker 已完成真实 Notepad 截图、鼠标移动、按键和关闭链路，客户端触发的诊断与引导进程也通过进程组/Job Object 边界回收整棵进程树，跨平台 `.tgz` 与 Marketplace 签名发布仍未完成。
- Computer Use Windows worker 现在与 macOS 一样为 11 个 MCP 工具发布 `_meta.chatos/skillGate`，并由跨平台 `npm run audit:skill-gates` 在 Windows 走本机 wrapper、在 macOS 走 Swift worker；Windows 本机 Skill gate 审计、双架构构建与屏幕 smoke 均通过。跨平台 `.tgz` 与 Marketplace 签名发布仍未完成。
- 任务审查窗口与桌面宠物状态已补齐键盘/无障碍语义：打开审查窗口后焦点落到关闭按钮，Tab 焦点保持在 modal 内，页签使用 roving focus 并支持方向键/Home/End，`Escape` 关闭并还原触发控件焦点，任务与检查页声明当前项及关联面板，宠物状态使用有界的 `polite` live region；这改善了键盘导航和屏幕阅读器投影，但不替代原生窗口逐态视觉取证。
- 当前仍未完全拉齐的基础差异有三类：细粒度目标/目的地出站规则仍未完成；Plugin 应用后端的 Windows loopback 健康代理/exemption 尚未闭环；任务审查的原生窗口逐态视觉取证尚未完成。这些差异继续保持为不可编辑/不可用状态，不对用户虚报为已支持。
- 当前 Windows React 前端复核通过：Vitest `138` 个测试文件、`516` 项测试全部通过；TypeScript 类型检查和 Vite 生产构建均通过。构建产物仅用于本地验证，不作为签名发布证据。

## A. 本轮已关闭

- [x] 已拉取并合并 `origin/3.0.9.4`（远端头 `fa8a80477`，合并提交 `1a2bede43`）；Local Agent 协议升级到 v49，Windows React 的共享契约、Tauri transport 与打包 sidecar 已同步。
- [x] Windows 机器资源压力已接入 Host durable-claim 准入：原生层每 5 秒读取 `GlobalMemoryStatusEx`、供电、电池与系统节能状态，映射 `normal/elevated/serious/critical`；升级立即生效，恢复需持续稳定 30 秒。当前等级在 Host 未运行时保留，新 worker Host 完成 v52 握手后必须先应用压力；压力 IPC 异常按 Host transport 故障回收，不让未知调度状态继续领取任务。真实 sidecar 已验证 `serious` 把有效 worker 降到配置并发的一半，`critical` 仍保留一个交互 worker。
- [x] Windows App 现在通过 `machine_resource_snapshot` 暴露共享资源池和 Host scheduler 诊断：App 的配置/有效容量、后台上限、已用/排队数量、压力级别、Media/Code Read 分域活动数，以及 Host 的有效并发、活动/空闲/排队、内存同步和 admission 状态；快照只读、字段有界并由 React schema 复验。
- [x] Windows App 共享资源准入基础已完成：CPU/物理内存/Host 保留共同计算预算，Media 与 Code Read 共享 permit、各 kind 保留容量、交互优先但每 4 次强制给后台公平机会、队列有界且取消安全；同一稳定系统压力会同步缩减 App 与 Host。用户附件、Artifact 导出、Media/Story 导入、项目文件任务、项目创建事务和记事本图片读取已在磁盘/worker 工作前取得 permit，附件阻塞 I/O 不再占用 async executor；已被 Host durable claim 领取的 tool work 不重复排队。
- [x] 快速搜索的文件/应用索引构建现在在进入 `spawn_blocking` 前取得 `CodeReadWorker + Interactive` permit；容量不足时受控失败，不再绕过共享资源池启动重型本地扫描。
- [x] 项目 Git 的查询、差异、暂存、提交、分支、远端和同步命令现在共享同一个 `CodeReadWorker + Interactive` permit，permit 覆盖完整 Git 子进程及刷新流程，避免并发 Git 操作绕过容量治理。
- [x] 远程文件的 OpenSSH 会话现在共享 `CodeReadWorker + Interactive` permit；permit 随 SSH 会话持有到完成、取消或超时，二次验证提示不会提前释放容量，避免长时间传输绕过本机资源池。
- [x] 本机终端的 PTY 启动现在共享 `CodeReadWorker + Interactive` permit；permit 覆盖等待、PowerShell/PTY 创建和整个会话生命周期，并在自然退出、启动失败、关闭、页面 discard 与应用退出时释放，避免本机终端绕过共享容量池。
- [x] Plugin 应用后端的动态 runtime 现在共享 `CodeReadWorker + Interactive` permit；permit 覆盖受限进程启动、健康检查和整个子进程生命周期，并在自然退出、启动失败、超时、停止和应用 reset 时释放；静态 Plugin 页面继续使用独立的有界 in-process server 路径。
- [x] Agent Plugin 的 MCP stdio 会话现在共享 `CodeReadWorker + Interactive` permit；skills-only 组件不启动进程也不申请容量，真实 MCP 子进程的 permit 随连接持有，并在连接失效、调用超时、会话替换、reset 和应用退出时释放。
- [x] Computer Use 的诊断和 onboarding 子进程现在共享 `CodeReadWorker + Interactive` permit；诊断 permit 覆盖检查及超时回收，onboarding permit 持有到一小时生命周期或提前退出，启动失败和进程树回收都会释放容量。
- [x] 项目运行 runtime 的用户目标进程现在共享 `CodeReadWorker + Interactive` permit；permit 覆盖启动、输出读取和运行生命周期，并在自然退出、停止、项目释放、账号切换、应用 reset 与启动失败时释放。
- [x] Plugin 安装器的 Git/npm/MCP 预检与安装子进程现在共享 `CodeReadWorker + Interactive` permit；普通 Registry 元数据查询不占用容量，设置页安装与 Agent 安装助手均覆盖 npm/Git 子进程及超时回收。
- [x] 剩余 process-spawn/重型 in-process 入口审计已完成：本机终端、项目运行、Plugin 应用、Agent Plugin MCP、Computer Use、Plugin 安装器、项目 Git 与 OpenSSH 均进入共享 permit；Host 进程继续使用自己的 Host scheduler durable claim，已领取 durable claim 的附件/终端 tool work 不重复进入 App pool，Explorer/系统 onboarding 只保留用户明确触发的外部短进程。
- [x] Agent Artifact 与记事本 Markdown 导出已使用公共 app-owned 原子事务：同目录独占临时文件完整写入并刷盘后再替换目标，失败自动清理；仅回收当前导出目录内最多 2,000 项、严格 canonical 命名、超过 24 小时且非 reparse/多硬链接的 ChatOS 残留。Windows 不会按 `.tmp` 后缀扫描或删除任意用户文件。
- [x] 附件 vault、Media Studio 图片/视频历史与 Story Studio 确定性 manifest 已接入所有权明确的启动后台清理：共享资源 governor 的 `CodeReadWorker` 后台 permit，只处理超过 24 小时的 canonical 临时文件；目录层级/目录项有界，账号 hash、记录 UUID、文件名、普通文件、reparse 与多硬链接均失败关闭，不碰已提交记录、媒体素材或任意后缀相同文件。
- [x] 固定 AppData 元数据目录的事务临时文件已接入同一后台清理链：General Settings、Global Utilities、Quick Search、Pet 偏好/收藏、Project Run、Remote Connection credential index、Clipboard payload 与 Tool Approval journal 共 6 个固定 scope、9 种受审计前缀；每 scope 最多 2,000 项，只删除超过 24 小时的 canonical 小写 UUID 临时文件。
- [x] Agent Plugin per-run invocation journal 与 Plugin 安装完整性事务已接入同一后台清理链：只进入 `NativeConnector/PluginRuntime/InvocationJournal` 和 `NativeConnector/Plugins/.integrity` 两个固定目录，分别只接受 `.<小写 UUID>.tmp` 与 `.<64 位小写 SHA-256>-<小写 UUID>.tmp`；继续拒绝未满 24 小时、symlink/reparse、多硬链接、非 canonical 名称和超出 2,000 项上限的对象。
- [x] Plugin 来源获取与安装提交的 staging 目录已接入启动后台清理：只进入 `.source-staging/<小写 UUID>` 与 `.staging/<小写 UUID>`，整棵树最多 20,000 项且所有节点均超过 24 小时才回收；任一 symlink、Windows reparse、多硬链接、特殊文件、近期写入或扫描错误都会保留整棵树，已提交版本目录和 `.backups` 不在清理范围。
- [x] Plugin `.backups` 已完成基于 Host 当前 `release_digest` 的崩溃恢复：设置页加载与所有安装来源共享串行事务锁；Host 仍指向旧 digest 时恢复完整性精确匹配的唯一备份，Host 已提交新 digest 时保留新目录。被替换的新目录或已失效旧备份只移动到 canonical `.staging/<UUID>` 隔离，随后复用受限的 24 小时树清理；无法唯一匹配、链接、越界或损坏的备份保持不动。
- [x] `NativeConnector/state.json` 与 `approval-activity.json` 已迁移到公共 AppData 原子替换边界：同目录 canonical UUID 临时文件完整写入、flush/fsync、复验普通非 reparse 文件及长度后，以 Windows write-through replace 提交；启动清理只识别 `.connector-state-<小写 UUID>.tmp` 与 `.approval-history-<小写 UUID>.tmp`，超过 24 小时才回收。
- [x] 31 个 macOS 一级页面均有 Windows React 路由和对应业务入口；默认登录后进入用户消息，而不是设置页。
- [x] Local Agent Host 打包资源完成真实 v52 健康握手，并通过账号隔离业务请求测试；此前两个 ignored 集成测试已实际运行通过。
- [x] 重新同步 `origin/3.0.9.4` 最新远端头 `5095fb146` 并合并到本分支；Windows React 的协议常量、共享 v52 fixtures、Tauri transport 和诊断分类已同步。NSIS 安装包 `%TEMP%\\chatos-windows-v52-nsis.json` 通过：安装 `3,191 ms`、原生窗口启动 `3,227 ms`、窗口标题 `ChatOS`、卸载 `1,171 ms`，退出码均为 `0`，sidecar SHA-256 为 `78a611c08eb7c26db118e39889144198aa7d33bcdbf39bb7f40ecc98f5428697`。
- [x] Story H.264 实际尾帧回归修复。旧 `16×16` 夹具低于 Windows Media Foundation H.264 有效尺寸，已替换为 `64×64` Baseline/YUV420P 红到蓝视频，继续断言最后一帧为蓝色。
- [x] Windows Rust 条件编译警告收口，避免发布测试输出掩盖新问题。
- [x] Windows 客户端、Cargo、Tauri 和 npm 产品版本从占位的 `0.1.0` 统一为 `3.0.9`。
- [x] Windows Computer Use Worker 的双架构构建规则和打包校验通过；x64 Worker 已真实启动并返回有效诊断。
- [x] Plugin 静态应用页面已准入：Windows 仅通过进程内 loopback 服务暴露清单声明的 HTML/静态资源，限制单文件与总大小，拒绝链接、越界和未声明资源，并保留 iframe sandbox、CSP、同源和 bridge 会话绑定。
- [x] Plugin 安装助手不再只是提示文案：专用 `main_chat` capability、精确联系人/Run/capability 复验以及 `plugin_installer_inspect_source/install_source` worker 已接通。本机 ChatOS Plugin 目录执行稳定内容指纹、显式安装意图、重新复制复验、逐文件完整性、Host 元数据事务与目录回滚；链接、reparse、特殊文件、越界/重叠目录、超限内容和未准入环境变量均失败关闭。
- [x] Plugin 安装助手的 Git 来源已闭环：只接受用户当前消息明确给出的无凭据 HTTPS URL 和安全 ref/subdirectory；使用隔离 bare fetch，禁用交互、Hook、文件协议和子模块，不 checkout 工作树或运行仓库脚本；archive、文件数、解包体积、路径和文件类型均有界，安装时重新获取并复验预检指纹后进入同一原子提交/回滚链。
- [x] Plugin 安装助手的固定版本 npm 来源已闭环：`search_npm/resolve_npm` 读取公共 Registry 的有界不可信元数据并固定精确语义版本；source 获取只接受 `registry.npmjs.org` HTTPS tarball，限制 512 MiB 并复验 sha512 integrity，不读取用户 npm 配置、凭据或运行生命周期脚本；安装时重新下载并复验内容指纹后进入同一原子提交/回滚链。
- [x] 普通 npm MCP 的 `inspect_npm/install_npm` 已闭环：使用隔离 npm 配置和私有 cache，禁用 lifecycle scripts/audit/fund，审计 v3 lockfile 的 Registry HTTPS + sha512 固定依赖；移除链接/shim、冻结 Node 运行时并生成 ChatOS 清单后，在 Job Object 中真实执行 initialize/tools/list。内容、启动参数、access profile 与完整工具 schema 共同形成指纹，安装时全部重做并复验后进入 Host 元数据事务和目录回滚链；公网官方 MCP Everything 已完成预检与重复安装 smoke。
- [x] 常见 stdio MCP JSON 兼容导入已接入 source 指纹与原子安装链：支持 `mcp.json`、`.mcp.json`、Cursor、VS Code、Gemini、Claude Desktop、Cline 和 OpenCode 布局；只准入可冻结的 Node 或包内 Windows `.exe`、空环境变量和 stdio transport，转换只发生在隔离 staging，不修改用户源目录。`npx`/shell、远端 transport、明文 Secret 和越界命令均失败关闭。
- [x] portable Agent Plugin、Codex Plugin、Claude Plugin、npm Agent Plugin 与独立 `SKILL.md` 已接入同一隔离 staging、内容指纹、原子安装和回滚链。根 Skill 会安全归入 `skills/<name>` 并携带 `scripts/references/assets`；纯 Skills 包使用完整性保护的 `skills-only` Host 记录，Agent runtime 不启动进程，只有 `capability_describe` 已披露的 Skill 才能激活并分页读取资源。
- [x] Windows Codex Hook 的安全预检与诚实状态投影已接入：兼容导入不再丢弃 Codex/Claude Plugin 的 Hook 路径，弱根清单不再遮住有效 `.codex-plugin` 清单；原生层按 macOS 同源边界校验 512 KiB Bundle、64 个 Hook、事件、同步 `command`、正则 matcher、命令长度、timeout、additional context、路径归属及 reparse/link，并且只向 React 投影组件名、事件和数量，不暴露命令或绝对路径。混合 MCP/Skill + Hook 包可安装，但设置页明确显示 `runner_unavailable`；Hook-only 包会说明已通过预检但因 Windows AppContainer runner 尚未接入而拒绝安装，绝不把 Hook 标成已运行。
- [x] Plugin 设置页已开放本机目录 typed 导入：React 只持有原生选择句柄与目录名，不接收绝对路径；预检显示清单、MCP/Skill 组件和内容指纹，确认安装时由原生端复验账号、页面、会话代次、选择租约及完全相同的指纹，再进入既有原子安装和回滚链。Git/npm 来源仍由安装助手处理。
- [x] Plugin Secret 已完成 Windows 原生闭环：`${credential:<name>}` 按账号、Plugin 与 Secret 名生成不透明 Host 引用，明文只写入 Windows Credential Manager；设置页仅投影 Secret 名称与配置状态，已有值永不回读。Agent 启动前会重新核对清单、Host 绑定与安装完整性，再把明文短暂注入目标进程环境；缺值、跨账号、引用篡改或清单漂移均失败关闭，卸载会同步清理当前清单声明的凭据。兼容 MCP JSON 仍拒绝环境变量，避免把无法区分的明文 Secret 导入系统。
- [x] 访问控制页的“审批策略/审批人”已接入现有真实命令门禁：按需审批、审批模型和完全控制分别映射到 `request_approval`、`auto_approval`、`full_control`；自动审批与完全控制必须二次确认，自动审批仍由 Rust 后端强制要求已配置审批模型。文件权限、网络访问和本机权限总开关继续锁定，并明确标注尚无操作系统级隔离。
- [x] Windows MSI 覆盖升级闭环已实测：从仓库保留的 `0.1.0` 安装到隔离目录并使用隔离的真实 AppData，先用截断的 `3.0.9` MSI 验证失败前置不改变旧程序和用户数据哨兵，再完成 `0.1.0 → 3.0.9` Major Upgrade、两代原生启动退出、数据保留和卸载。`3.0.9` 起显式设置 `allowDowngrades=false`，Bundle 审计和 MSI `WIX_DOWNGRADE_DETECTED`/LaunchCondition 双重门禁防止本安装包覆盖更高版本。
- [x] Windows MSI 事务中途故障回滚闭环已实测：仓库内受审计 WiX 模板把 `RemoveExistingProducts` 固定在 `InstallExecute` 之后（执行序号 `6501`）；在复制新文件后注入 Type 19 故障，两轮均返回 `1603`，并完整恢复旧 ProductCode、旧 EXE 哈希和 AppData 哨兵。随后正常升级和卸载成功，旧版、故障版与当前版 ProductCode 均为未安装状态，主 EXE 组件无残留 client。
- [x] Windows MSI 在旧客户端保持原生窗口运行时的覆盖升级闭环已实测：两轮均由 Windows Installer 正常关闭 `0.1.0` 进程，升级返回 `0`，`3.0.9` 原生启动和最终卸载成功，AppData 哨兵保留；测试逐轮对比系统 `PendingFileRenameOperations`，确认未新增待重启文件操作。
- [x] Windows NSIS 原生启动 smoke 已与真实用户数据隔离：每次运行使用独立的新 AppData/LocalAppData，完成后断言测试数据已清理，不再读取或污染当前登录账号的数据。当前安装包完成两轮、每轮 5 次隔离冷启动；第二轮安装 `1,970 ms`、启动中位数 `9,040 ms`、P95 `17,882 ms`、卸载 `619 ms`，全部在 20 秒门限内正常退出。
- [x] Windows AppContainer 基础启动器与真实隔离探测已落地：使用临时 profile、`PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES` 和 `CREATE_SUSPENDED` 启动系统进程，先校验子进程 token 再恢复执行，策略不匹配时在用户代码运行前终止。`disabled` 档位实测 token 不含 capability、无法读取宿主临时文件且无法连接宿主 loopback；`loopback`/`controlled` 档位生成并注入对应 Windows capability SID，随后从子进程 token 反查确认策略精确生效，`host` 明确拒绝映射到 AppContainer。租约 manager 会以不跟随 reparse point 的 Windows 句柄遍历非空目录树，逐项保存原 DACL、卷号和文件 ID，再以 v3 journal 原子落盘后通过同一对象句柄授权。正常释放通过保留句柄恢复原对象，因此支持租约内重命名；启动恢复会在写 ACL 前验证对象身份，同路径替换会失败关闭。运行期新增项的 AppContainer SID 会撤销，v1/v2 journal 继续兼容恢复，目录树数量与 journal 大小有界。可复用运行时启动器现已补齐显式工作目录、最小环境块、仅三个 stdio 句柄继承、有界 stdout/stderr 捕获、超时与 Job Object 进程树回收；真实 Git 仓库已在读写租约中完成 `git add`。每次运行使用独立 AppContainer SID/profile，profile journal 在创建前落盘，正常退出删除，模拟崩溃后启动恢复也已实测，避免并行执行链共享 SID 或遗留 profile。长驻运行时现可安全移交 stdin/stdout/stderr、保持 profile 与多 workspace 租约，并在等待超时或句柄释放时回收整棵进程树；重叠租约会在启动前失败关闭。Agent Plugin MCP 已迁移到独立 AppContainer，任务终端也已迁移；Plugin 应用后端已接入生产受限启动路径，但 loopback 健康代理/exemption 尚未完成，不能把全部链路标成已完成。

## B. 本次交付前必须关闭

- [x] Windows React 前端全量测试、类型检查、生产构建、架构审计和 Bundle 审计全部通过（137 个测试文件、510 项测试）。
- [x] Windows 宠物任务门禁摘要完成后，Tauri Rust 全量测试、Rustfmt、Clippy 和 Bundle 审计重新通过：516 项测试被发现，510 项通过、0 项失败、6 项环境集成测试按预期 ignored；其中 2 项 sidecar 集成测试此前已以新 v49 Release Host 实际运行通过。Local Agent Host 全量 135 项测试及 Host 自身 Clippy `-D warnings` 保持通过；Windows 对锁中文件的新开句柄读取限制已进入跨平台测试语义，不再误报数据库所有权回归。
- [x] 重新生成版本为 `3.0.9` 的 Release EXE、MSI、NSIS，并记录 SHA-256：
  - `chatos-windows-react.exe`：`23e20965dee8b6e7b5260014d69a17a8b1cf1a01dd706ba8052e0dbd97d51c03`
  - `chatos_local_agent_host.exe`：`b6737311ae2b9739d496503007a635c2e97a38a8bfc4a9e17bd87a00520e6187`
  - `ChatOS_3.0.9_x64_en-US.msi`：`768aee082d7b7dd2156425e86bca04237fde1b012b3e41b067ac4635b521c6e8`
  - `ChatOS_3.0.9_x64-setup.exe`：`1d222d048926b0306b7f17b26745ffd30053b6e1c1caa0cb10e4c352bf3f1682`
- [x] 用新 NSIS 安装到全新临时目录，验证主程序、Host、资源、隔离 AppData 原生启动、正常退出、卸载和残留清理；当前事务安全构建产物先连续两次通过单次启动门限，再完成两轮各 5 次冷启动基线，最新报告为 `%TEMP%\chatos-windows-nsis-startup-performance-1.json` 和 `%TEMP%\chatos-windows-nsis-startup-performance-2.json`。
- [x] MSI 升级 smoke 在既有四轮基础上，以事务安全模板再连续通过两轮；报告 `%TEMP%\chatos-windows-msi-upgrade-transactional-safe-1.json` 和 `%TEMP%\chatos-windows-msi-upgrade-transactional-safe-2.json` 同时记录 `0.1.0 → 3.0.9`、两代原生启动退出、截断包 `1620` 失败、事务中途故障 `1603` 回滚、AppData 哨兵保留、卸载以及降级检测规则。
- [x] MSI 运行中客户端升级 smoke 再连续通过两轮；报告 `%TEMP%\chatos-windows-msi-running-upgrade-1.json` 和 `%TEMP%\chatos-windows-msi-running-upgrade-2.json` 记录旧版原生窗口进程、Installer 正常关闭、无重启升级、新版原生启动、数据保留、卸载和系统待重启队列不变。
- [x] 本批“Agent Plugin MCP AppContainer 迁移”已重新生成到 MSI/NSIS，并通过 `%TEMP%\chatos-windows-agent-plugin-appcontainer-functional-nsis.json` 的全新目录安装、隔离 AppData 原生启动、正常退出、卸载和清理验证：安装 `1,947 ms`、原生启动 `27,398 ms`、卸载 `614 ms`，三者退出码均为 `0`；Probe 与 Runtime AppContainer profile 注册表残留合计为 `0`。同一哈希的 20 秒门限报告 `%TEMP%\chatos-windows-agent-plugin-appcontainer-final-nsis.json` 以 `3,099 ms` 通过，但另一全新安装路径超过 20 秒；该性能不稳定项继续保留，不伪报为稳定通过。
- [x] 合并 `3.0.9.4` 后的新 NSIS 已通过 `%TEMP%\chatos-windows-3.0.9.4-merge-nsis.json` 的全新目录安装、隔离 AppData 原生启动、应用自身退出、卸载与清理：安装 `2,181 ms`、原生启动 `3,184 ms`、卸载 `624 ms`，全部退出码为 `0`，安装后 sidecar 哈希与构建资源一致。
- [x] 资源压力治理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-resource-pressure-final-nsis.json`：全新目录安装 `2,052 ms`、安装版原生启动 `3,144 ms`、卸载 `610 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] App 共享资源准入版本的新 NSIS 已通过 `%TEMP%\chatos-windows-machine-governor-final-nsis.json`：全新目录安装 `2,174 ms`、安装版隔离冷启动并由应用自身退出 `16,086 ms`、卸载 `621 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] 原子导出版本的新 NSIS 已通过 `%TEMP%\chatos-windows-atomic-export-final-nsis.json`：全新目录安装 `2,220 ms`、安装版隔离冷启动并由应用自身退出 `15,070 ms`、卸载 `612 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] 附件 vault 启动清理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-attachment-temp-cleanup-final-nsis.json`：全新目录安装 `2,220 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,167 ms`、卸载 `631 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Media History 临时 manifest 清理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-media-history-temp-cleanup-final-nsis.json`：全新目录安装 `2,199 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,197 ms`、卸载 `629 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Story manifest 临时文件清理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-story-temp-cleanup-final-nsis.json`：全新目录安装 `2,194 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,161 ms`、卸载 `657 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Agent 对话附件写入路径已补齐启动清理：`NativeConnector/AgentMessageAttachments` 只扫描固定的 `messages/<bucket>/attachment_<小写 32 位 hex>` 与 `drafts/<bucket>/document-draft_<小写 32 位 hex>` 两层对象，按数据库引用保护仍在使用的文件，孤儿文件须超过 24 小时且为普通单硬链接文件才回收；目录越界、reparse、未知名称、数据库引用异常和超过 20,000 项均失败关闭。
- [x] OpenSSH 会话运行目录已接入同一启动后台清理：只处理 `remote-connections/runtime/connection-test-<带连字符的小写 UUID>` 的完整目录树，所有节点均超过 24 小时且通过普通文件、reparse 和硬链接检查才回收；正常会话仍由 Drop 即时清理，近期目录和未知名称保留。
- [x] Media Studio 图片/视频历史清理已覆盖崩溃中断的记录目录：账号 hash 和记录 UUID 通过固定层级校验后，缺失 `record.json` 且整棵树超过 24 小时、无 reparse/多硬链接的目录才回收；已有 manifest 的记录只清理 canonical `record.json.tmp`，未知名称和近期写入保留。
- [x] Story Studio 项目媒体清理已覆盖生成资产孤儿：加载并验证 `project.json` 后，只回收未被当前项目引用的 `image-<小写 32 位 hex>.png` / `video-<小写 32 位 hex>.mp4` 旧普通文件；用户资源名、异常 manifest、reparse、硬链接和近期写入均保留。
- [x] 远程下载目标目录已接入按需残留清理：每次新下载前只在用户刚确认的目标目录内，扫描最多 2,000 项 canonical `.chatos-download-<24 位小写 hex>.tmp`，仅回收超过 24 小时的普通单硬链接文件；其他后缀、未知名称、近期文件和 reparse 对象保留。
- [x] 终端项目文件批量编辑已接入崩溃残留清理：每次提交前只在本次编辑涉及的父目录内，扫描最多 2,000 项 canonical `.chatos-edit-<小写 UUID>` / `.chatos-backup-<小写 UUID>`，仅回收超过 24 小时的普通单硬链接文件；未知名称、近期文件、硬链接和 reparse 对象保留。
- [x] App-owned 残留清理在 Unix 兼容构建上也拒绝多硬链接文件：共享 `try_remove_stale_owned_file` 在执行删除前验证 `nlink == 1`，避免回收仍被其他目录引用的文件。
- [x] 项目创建恢复 journal 加载改为有界枚举：最多检查 128 个目录项、接受最多 64 个 JSON journal，超过上限立即失败，不再先把整个目录收集到内存。
- [x] Plugin 完整性树快照改为流式遍历：最多检查 40,000 个文件/目录条目，并继续限制文件数与总字节数，避免大量空目录造成无界条目收集。
- [x] Agent Plugin Skill 目录发现与资源快照改为有界流式遍历：每个 Skill 目录及其资源树最多检查 512 项，同时保留 256 个资源和 16 MiB 总内容上限。
- [x] 兼容 Plugin 导入的 Skill 根目录与资源复制也改为有界遍历：根目录最多 256 项，递归资源树最多 20,000 项，超过限制立即拒绝，不再先收集整个目录。
- [x] npm Plugin 安装后的 shim 清理与安装目录 sanitization 共享 20,000 项全树上限；递归清理遇到超限立即拒绝，避免恶意 npm 包通过大量目录或 `.bin` 条目造成无界扫描。
- [x] Media Studio 图片/视频历史与 Story Studio 项目、规划运行和媒体批次分页读取均限制每个目录最多扫描 20,000 项，超限立即失败关闭，避免旧产物堆积导致无界历史遍历。
- [x] AppContainer workspace/profile journal 恢复改为流式有界枚举：每个 journal 根最多检查 1,024 项，超过上限立即失败，不再先把整个目录收集到内存。
- [x] AppContainer workspace ACL 租约树改为流式有界枚举：单目录条目在加入排序缓冲前受全树 100,000 项上限约束，超过上限立即失败，不再先把超大目录全部收集到内存。
- [x] 快速搜索的应用索引与文件搜索目录队列最多保留 10,000 个待处理目录；文件搜索在达到队列上限时显式标记源数据不完整，避免大目录树无界占用内存。
- [x] 项目运行分析器的待处理目录队列与 2,500 个目录访问上限保持一致，避免每个目录批量发现 10,000 个子目录时队列瞬间膨胀到百万级路径。
- [x] 项目文件名称搜索现在把待处理目录栈计入全局 20,000 项访问预算，达到预算时显式截断，避免多个大目录反复扩张搜索栈。
- [x] AppData 临时目录树清理在入队前同时约束已访问和待处理节点，超过 20,000 项立即保留整棵树，避免超大目录一次性膨胀清理队列。
- [x] Agent Plugin journal、Agent Skill overrides 与 AppContainer workspace/profile journal 均改为句柄上的 `take(limit + 1)` 流式读取，metadata 检查后的文件膨胀不会造成无界分配。
- [x] Plugin 设置页加载已安装 manifest 改为句柄限长读取，metadata 检查后的文件替换或膨胀不会触发无界 `read_to_string` 分配。
- [x] 附件 vault metadata JSON 改为 64 KiB 句柄限长读取，损坏或竞态文件不会通过无界 `fs::read` 占用内存。
- [x] 终端项目 `list_dir` 与文本搜索改为有界目录枚举：目录列表最多读取请求上限加一项，递归搜索每个目录最多排队 10,000 个子项且全局待处理栈计入 100,000 项预算，并显式标记截断，避免大目录一次性占满内存。
- [x] Agent Plugin Skill manifest 目录清单在处理前限制为最多 64 项加一项，超限立即拒绝，不再先收集不可信清单的全部目录。
- [x] 固定 AppData 临时文件清理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-appdata-temp-cleanup-final-nsis.json`：全新目录安装 `2,281 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,214 ms`、卸载 `667 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Plugin 事务临时文件清理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-plugin-temp-cleanup-final-nsis.json`：全新目录安装 `2,181 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,187 ms`、卸载 `638 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Plugin staging 崩溃残留清理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-plugin-staging-cleanup-final-nsis.json`：全新目录安装 `2,187 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,173 ms`、卸载 `647 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Plugin backup 崩溃恢复版本的新 NSIS 已通过 `%TEMP%\chatos-windows-plugin-backup-recovery-final-nsis.json`：全新目录安装 `2,298 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,204 ms`、卸载 `635 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Connector 固定元数据原子替换版本的新 NSIS 已通过 `%TEMP%\chatos-windows-connector-transaction-cleanup-final-nsis.json`：全新目录安装 `2,192 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,185 ms`、卸载 `626 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Project Creation journal 与 Agent Skill overrides 原子事务版本的新 NSIS 已通过 `%TEMP%\chatos-windows-project-skill-transaction-final-nsis.json`：全新目录安装 `2,182 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,160 ms`、卸载 `646 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] 项目文件保存原子事务版本的新 NSIS 已通过 `%TEMP%\chatos-windows-project-file-transaction-final-nsis.json`：全新目录安装 `2,177 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,216 ms`、卸载 `619 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] AppContainer workspace/profile journal 原子事务版本的新 NSIS 已通过 `%TEMP%\chatos-windows-appcontainer-journal-final-nsis.json`：全新目录安装 `2,183 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,151 ms`、卸载 `636 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Story 尾帧临时媒体治理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-story-tail-temp-final-nsis.json`：全新目录安装 `2,196 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `16,970 ms`、卸载 `619 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。首次启动耗时继续计入未签名包性能波动，不据此声称稳定达到 20 秒基线。
- [x] AppContainer 自检临时目录治理版本的新 NSIS 已通过 `%TEMP%\chatos-windows-appcontainer-probe-temp-final-nsis.json`：全新目录安装 `2,244 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `3,194 ms`、卸载 `635 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] 任务审查优先级投影版本的新 NSIS 已通过 `%TEMP%\chatos-windows-task-priority-final-nsis.json`：全新目录安装 `2,205 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `15,637 ms`、卸载 `654 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] 对话 Timeline 任务状态摘要版本的新 NSIS 已通过 `%TEMP%\chatos-windows-task-timeline-final-nsis.json`：全新目录安装 `2,207 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `11,096 ms`、卸载 `635 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] 宠物任务门禁摘要版本的新 NSIS 已通过 `%TEMP%\chatos-windows-pet-task-gates-final-nsis.json`：全新目录安装 `2,245 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `11,025 ms`、卸载 `701 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。
- [x] Codex Hook 预检与状态投影版本的新 NSIS 已通过 `%TEMP%\chatos-windows-hook-preflight-final-nsis.json`：全新目录安装 `2,239 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `9,444 ms`、卸载 `640 ms`，全部退出码为 `0`；安装后 v49 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。当前产物 SHA-256：EXE `b7cb166d0c92d90b6d9b2fc54de1293708e9aa519d7ccdc5e47ff9c66ecc34ee`、Sidecar `b6737311ae2b9739d496503007a635c2e97a38a8bfc4a9e17bd87a00520e6187`、MSI `11b49b5cdb0c7f2755a0758515cd3fce810abf2ae9cd7abb8b45fc577bddd20f`、NSIS `6d0371b17f1c2320ad486e4a226958f709f25380d2517ff844ec0c76d1e8fbf2`。
- [x] Codex/Ponytail Hook 信任 Bundle 发布版本的新 NSIS 已通过 `%TEMP%\chatos-windows-hook-trust-final-nsis.json`：全新目录安装 `2,109 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `10,189 ms`、卸载 `625 ms`，全部退出码为 `0`；安装后 sidecar SHA-256 与构建资源一致，隔离用户数据已清理。当前 Bundle 只发布并保持 inactive，Windows runner 尚未接入。当前产物 SHA-256：EXE `aa2154074bdbca527fa90e635207ea644b5fc49c6b760e604b192a06855ee09e`、Sidecar `d092495a2f4ed65a0569733c3f2c2832186d3a060425d0a78af24c62d80f198b`、MSI `bbc5a574eec8088da788f3b8776f4f75bc23eec5a87771ab52a6695d87630b7d`、NSIS `e17fd2810b5d09b901ac4f673a006b8b0f27aef73d05aa3672be07d5a4f2e3ae`。
- [x] Windows Hook AppContainer runner 基础版本的新 NSIS 已通过 `%TEMP%\chatos-windows-hook-runner-final-nsis.json`：全新目录安装 `2,261 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `9,827 ms`、卸载 `634 ms`，全部退出码为 `0`；Host 真实测试覆盖 Hook stdout、超时和进程树回收。Bundle 仍保持 inactive，尚未开放安全启用。当前产物 SHA-256：EXE `b2ef541d211795f1d975e1c154455d49977acc9b563768b97d9796db2db18d07`、Sidecar `4949243637760522783258c06871d859f1b55b0a29dde1db79a86217cc82ee0d`、MSI `16e25dd93325658dc67a1e5c38d148f6cfbea3bf2efe73d2e9d3c527e3c4216a`、NSIS `c1f9d0019629aad7bb2743d230d42805b8ef0f52159b40a8269416689a4a0279`。
- [x] Windows Hook AppContainer runner 并发 lease 安全版本的新 NSIS 已通过 `%TEMP%\chatos-windows-hook-runner-final-nsis.json`：全新目录安装 `2,263 ms`、安装版隔离 AppData 原生启动并由应用自身退出 `9,055 ms`、卸载 `629 ms`，全部退出码为 `0`；Host 真实测试覆盖 Hook stdout、超时、进程树回收和重叠安装目录的串行 lease。Bundle 仍保持 inactive，尚未开放安全启用。当前产物 SHA-256：EXE `37a6aabddf17e596fc10b006953e5f5571d799929d7fd6131c2faf9224f5df57`、Sidecar `4159ed68b438a3d5e28dbd342138e4476011efb1e69f7dc38f73c828a5671ecb`、MSI `9160225c79b52e155d79dac342c2f034c6a0310ab8b8c0ac96bc9ff4031c857a`、NSIS `b0687915d458f95d2ce49efd4fc7aa1489dccdff090375fcb4140893d9fd942d`。
- [x] 启动新 Release 原生 EXE，并从 Windows 进程信息确认运行目标为 `target-shared\release\chatos-windows-react.exe`，不是 Vite 或浏览器进程。
- [x] 在新 Release 原生窗口中确认登录后的默认页仍为用户消息：最新 NSIS 安装版原生启动已进入 `chatos` 项目的“用户消息”工作区，未落入设置页；报告为 `%TEMP%\\chatos-windows-workspace-entry-nsis.json`，本机窗口观察记录已归档在临时目录。
- [ ] 按 31 张 macOS 截图完成原生窗口逐页视觉走查。当前 Computer Use 原生管道不可用，需在工具恢复后补验；不能用浏览器截图替代。

## C. 仍存在的真实功能差距

### P1：用户能直接感知

- [x] Codex/Ponytail Hook 的 Windows 端完整闭环。最终 NSIS 安装包已真实启动 AppContainer runner，写入并激活受信 Bundle，触发 `UserPromptSubmit`，回传上下文，按 `continue=false` 阻断 Prompt，并在 Host 重启后恢复 active Bundle；报告为 `%TEMP%\\chatos-windows-installed-hook-restart.json`。原生窗口逐态视觉走查仍单独保留为未完成项。
- [ ] `3.0.9.4` 新增的任务审查优先级展示仍需原生窗口逐态核对。Windows 原生任务图 DTO 现同时保留 Host 原始状态和 `reviewing_followup` / `waiting_confirmation` 展示状态；Task Graph、Run Inspector、对话 Timeline 状态胶囊与宠物活动摘要均让新门禁覆盖旧 Run 活动并保持轮询，`needs_review` 的恢复操作继续保留。各入口的原生视觉优先级仍需取证，当前不声称完全一致。
- [ ] Computer Use / 屏幕采集的受信发布闭环。Windows Worker 源码、x64/arm64 构建和 smoke 已通过，但当前用户 Plugin 目录为空；还需产出跨平台 `.tgz`、进入签名 Marketplace Release，并从 Windows 客户端完成安装、启用、`doctor`、屏幕采集和输入实测。
- [ ] 访问控制的文件与网络隔离。AppContainer 文件/网络拒绝、`disabled`/`loopback`/`controlled` capability token、句柄绑定的非空目录逐项 DACL 快照、父目录临时 `FILE_TRAVERSE` 与按 SID 恢复、运行期新增项 SID 清理、v1/v2/v3 journal 恢复、对象身份校验、reparse point 拒绝，带最小环境、一次性输出捕获、长驻交互 stdio、多 workspace 租约、超时与 Job Object 回收的可复用启动器，以及独立 profile SID/journal 崩溃恢复已经通过；Agent Plugin MCP 与任务终端已迁移到独立 AppContainer，Agent Plugin 的 `network.client` 已按 manifest 映射到 `controlled`，Plugin 应用后端已接入生产受限启动路径。普通用户调用 `CheckNetIsolation LoopbackExempt` 会被系统拒绝（Win32 5），仍需实现不依赖管理员权限的细粒度目标/目的地出站规则，并完成 Plugin loopback 健康代理/exemption 后才能开放第 31 页开关。迁移完成前继续只发布账号/项目边界、审批策略和现有 Job Object 进程树回收。

### P2：平台/发布质量

- [x] Windows App 的统一容量治理已完成基础共享 permit、机器资源诊断 snapshot、快速搜索、项目 Git、远程文件、本机终端、Plugin 应用 runtime、Agent Plugin MCP、Computer Use、项目运行 runtime 和 Plugin 安装器接入；已完成剩余 process-spawn/重型 in-process 入口审计。Host v49 与 App pool 已响应同一系统压力；已经领取 durable claim 的附件/终端 tool work 继续排除在外层等待之外。
- [ ] `3.0.9.4` 的文件 worker 隔离和临时文件生命周期仍需 Windows 继续拉齐：公共原子导出与安全的 24 小时残留回收已覆盖 Agent Artifact、记事本 Markdown、项目文件保存、Agent 对话附件、Media Studio 历史记录、Story Studio 生成媒体和远程下载目标目录的按需残留；附件 vault、Media History、Story manifest/尾帧临时媒体、固定 AppData 元数据、Connector state/审批历史、Project Creation journal、Agent Skill overrides、AppContainer workspace/profile journal 与隔离自检目录、Plugin 安装完整性事务、Agent Plugin per-run journal、Plugin source/install staging 树，以及 `.backups` 的 Host-authoritative 崩溃恢复已完成，远程下载已有独立原子提交。运行路径中的无归属 `NamedTempFile` 与匿名 `tempdir` 已清零；其余用户目录及 App-owned 直接写入点仍需按实际提交语义逐项审计，禁止用全盘后缀清理冒充统一治理。
- [ ] Windows 安装包代码签名、发布证书、签名校验和 SmartScreen 信誉验证。仓库可生成安装包，但本机未配置正式签名身份。
- [ ] Windows Installer 无法由 Restart Manager 正常关闭的强制文件锁及需重启场景仍需在一次性 Windows VM 中补验；客户端正常运行造成的文件占用、事务中途故障回滚均已关闭。本机已有其他软件留下的待重启文件操作，不能为了测试篡改系统队列。历史 `0.1.0` 安装包自身允许降级，已发布二进制无法由新版本追溯修改。
- [ ] 固定分辨率与 100%/125%/150% 缩放的原生截图对照、键盘导航、焦点顺序、屏幕阅读器标签和对比度验收。
- [ ] Host ready、首屏会话、万级消息、大目录、Git Diff、终端大输出和 8 小时运行的 Windows 性能/稳定性基线；安装包隔离冷启动基线已经完成。
- [ ] 本批未签名 EXE/sidecar 在全新安装路径的首次启动性能不稳定：同一最终哈希曾在 `3,099 ms` 内通过 20 秒门限，也曾两次超过门限；60 秒功能 smoke 在 `27,398 ms` 正常退出，同路径复跑为 `2,783 ms`。需结合正式代码签名与一次性干净 Windows VM 复验首次扫描/冷启动门限。

## D. 已确认的平台差异，不伪装为缺陷已修复

- 宠物浮窗当前使用 Windows 置顶窗口；Tauri 公共 API 不提供 macOS 同等的“固定到所有虚拟桌面”。
- Windows 设备网关使用账号认证控制面与本地 Rust Host，不宣称具有 macOS 设备签名长连接 Relay。
- Windows 文件访问遵循当前账号 ACL 和用户显式授权工作区，不存在 macOS“完全磁盘访问”的同名总开关。
- Plugin 应用静态页面与 `local_http` UI 已在 Windows 准入；Windows 和 macOS UI Host 目前都只真正实现 `host.context.read`。Manifest 中尚未闭环的 `artifact.list/read/download/create/update` 是两端共同待办，不列为 Windows 落后于 macOS 的差距。

## E. 执行顺序

1. [已完成] 关闭 B 组中不依赖 Computer Use 原生管道的构建、测试、重新打包、安装 smoke 和原生进程启动门禁。
2. [已完成] Plugin 安装助手专用 capability、受限平台工具与原生 ChatOS Plugin 本地目录 inspect/install 闭环。
3. [已完成] Git、固定版本 npm 来源、普通 npm MCP、常见 stdio MCP JSON、portable Agent、Codex、Claude、独立 Skill、skills-only、设置页本机目录 typed 导入与 Windows Credential Manager Secret resolver 均已完成。
4. [外部依赖] 完成 Computer Use `.tgz`、Marketplace 签名发布/客户端安装闭环，以及 Windows 安装包正式代码签名。
5. [进行中] Windows AppContainer 启动、文件/网络拒绝、disabled/loopback/controlled 网络 capability、句柄绑定的非空目录逐项 ACL 租约、父目录临时 `FILE_TRAVERSE`、v1/v2/v3 journal 恢复和可复用受限进程启动器已完成；任务终端已迁移，Agent Plugin 的 `network.client` 已按 manifest 选择 `controlled`/`disabled`，Plugin 应用后端已接入生产受限启动路径。下一步实现细粒度目标/目的地出站规则、完成 Plugin loopback 健康代理/exemption，再开放第 31 页开关。审批策略和审批人已经接入真实执行门禁。
6. [待验证] 工具恢复后完成 31 页原生逐页视觉、缩放和无障碍验收；升级、失败前置保护、事务中途故障回滚、客户端运行中升级及安装包冷启动基线已完成，后续在一次性 VM 补强制锁/需重启场景并继续 Host ready、首屏与长稳性能基线。
