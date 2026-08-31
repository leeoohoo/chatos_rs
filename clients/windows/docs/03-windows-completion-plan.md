# Windows 客户端完整能力实施清单

本清单以 `02-capability-parity-matrix.md` 为验收基线。每个条目只有同时满足“真实能力接入、错误与取消路径、自动化测试、Windows 验收说明”后，才能标记为完成。

## 实施顺序

1. 公共基础
   - SQLite 持久化设置、运行时语言切换、字号和主题即时生效。
   - 全局敏感信息脱敏日志、统一错误展示、设计 token 收口。
2. 项目生产力
   - Git 状态、Diff、历史、暂存、提交、分支和远端操作。
   - 记事本目录、搜索、编辑、预览、导出与稳定选择状态。
3. 本机与远端
   - Connector 配对管理、睡眠唤醒恢复、符号导航。
   - SSH 凭据、二次认证、SFTP、远程终端。
4. 插件运行时
   - Catalog 安装/升级/卸载、hash/平台/架构/权限校验。
   - stdio/HTTP MCP Relay、OAuth、Secret、artifact 和 Visual Session。
5. 全局宠物
   - 透明置顶窗口、DPI/多显示器、拖动动画。
   - Pet Inbox、完成/阻塞/执行中、审批、Ask User、快捷聊天。
6. 交付质量
   - x64/ARM64 MSIX、Windows CI、UI 自动化、可访问性和真机验收脚本。

## 边界

- Core/API/Presentation/Connector 继续保持平台无关或可独立测试。
- WinUI、Credential Manager、ConPTY、DPI、多显示器和 MSIX 必须在 Windows 真机验收。
- 不为了共享 macOS 代码引入 Rust 公共层；Windows 使用 .NET 与 Windows 原生能力独立实现。
- 不把 macOS 的 UI 逐像素复制到 Windows，但颜色、密度、层级、交互语义必须一致。

## 当前第一批

- [x] 设置持久化模型与 SQLite 存储
- [x] 全部 Desktop 页面、DataTemplate、资源标题、状态和操作反馈的中英文运行时切换
- [x] 字号、主题、宠物开关即时生效
- [x] 设置页面与主窗口入口
- [x] 公共基础自动化测试

## 已完成的项目生产力能力

- [x] Git：安全本地路径解析、`git.exe` 有界执行、取消/超时、状态/Diff/历史/暂存/提交/分支/远端/拉取/推送。
- [x] Git：项目原生页面和操作后权威刷新；不会通过 reset 或 checkout 强制覆盖外部修改，拉取固定为 `--ff-only`。
- [x] 记事本：服务端目录和笔记 API、搜索、稳定选择、切换前保存、默认预览、编辑/分栏、删除和 Markdown 导出。
- [x] Git 与记事本自动化测试。

Git 和记事本当前均进入 `Windows 验收`：非 UI 层已编译和测试，XAML 已通过 XML 静态校验；WinUI XAML 编译器依赖 Windows，验收步骤见 `04-windows-git-notepad-acceptance.md`。

## 已完成的本机 Connector 能力

- [x] 主应用 API 登录凭据与 Connector Gateway 凭据彻底隔离，配对和断开不会覆盖或清除 ChatOS 登录态。
- [x] 设置页真实请求 pairing ticket，支持 Gateway、设备名、多工作区文件夹选择、重复路径过滤、配对、刷新和确认断开。
- [x] 侧栏展示本机 Connector 的未配对、连接中、已连接、等待重连和睡眠暂停状态，并可进入管理页。
- [x] 后台连接在系统睡眠时终止旧 lease，唤醒后恢复连接循环，重复电源通知不会触发重连风暴。
- [x] 文件页定义/引用导航，包含项目边界、结果上限、取消、超时和短缓存。
- [x] API、Presentation 和 Connector 配对管理自动化测试。

Connector 配对、睡眠恢复和代码导航当前进入 `Windows 验收`，步骤见 `05-windows-connector-acceptance.md`。

## 正在实施的远端能力

- [x] 远端连接云端元数据列表、新建、编辑和删除协议，所有本机密码、私钥路径、证书路径和跳板机凭据在 API 层再次强制剔除。
- [x] Windows Credential Manager 本机凭据存储；云端创建成功但本机凭据提交失败时回滚云端记录。
- [x] SSH.NET 密码、私钥和 OpenSSH 证书认证，连接取消/超时与可操作错误。
- [x] `strict` / `accept_new` 主机密钥指纹策略，首次接受的 SHA-256 指纹进入本机凭据库。
- [x] keyboard-interactive 二次验证挑战模型，支持验证码重试。
- [x] 跳板机先认证并建立本地端口转发，再对目标服务器独立进行主机指纹和认证校验。
- [x] 远端连接编辑/测试/删除页面和 Shell 远端资源列表，凭据字段不回显。
- [x] SFTP 浏览、文本预览、上传下载、覆盖确认、目录创建、重命名和递归删除。
- [x] 远程命令终端保留工作目录、支持取消和二次验证；远端临时输出受文件大小限制，客户端最多接收 200,000 字符。

远端 SSH、SFTP 和命令终端当前进入 `Windows 验收`，步骤见 `06-windows-remote-acceptance.md`。

## 已完成的插件运行时与设置能力

- [x] 插件 Catalog 安装、Release 升级、SHA-256、平台/架构、权限和包内路径校验。
- [x] stdio 与 HTTP MCP initialize、tools/list、tools/call、超时、取消、大响应和进程/会话回收。
- [x] Secret 模板只从 Windows Credential Manager 注入，设置页不回显且草稿禁止序列化。
- [x] OAuth PKCE、refresh、needs-auth、取消语义、卸载/升级 purge 和调用时 Bearer 注入。
- [x] Artifact 权威 descriptor、路径/MIME/大小/hash/篡改校验和有界流式下载。
- [x] Visual Session owner、conversation/turn/task/run 绑定、多 adapter metadata 和选中帧读取。
- [x] 插件设置 UI：安装/更新/卸载、启停、权限、Secret、OAuth 与中英文状态。
- [x] Visual Session 原生置顶画中画：多会话选择、选中帧加载、旧帧防覆盖、无会话自动隐藏和关闭单个画面。
- [x] Artifact 全局产物中心：按 adapter 过滤、受控缓存打开、FileSavePicker 另存、临时文件原子替换和失败清理。

插件运行时与 UI 当前进入 Windows 验收；新增验收项已合并到 `07-windows-plugin-pet-acceptance.md`。

## 已完成的设置能力

- [x] 审批设置 UI：默认模式、升级风险确认、等待处理、原地决策和最近 30 条审计记录。
- [x] Windows AI reviewer 使用已选择的本机审批模型和插件管理下发 Prompt 执行结构化审核；支持 approve/deny/ask_user/remember_allow。
- [x] reviewer 请求校验 Prompt SHA-256，不发送绝对工作区路径；API Key、Secret 和供应商错误正文不进入 UI/日志。
- [x] 模型或 Prompt 不可用时明确安全回退用户，不把用户确认伪装成 AI 决策。
- [x] 同步 ChatOS 已启用模型，本机持久化审批模型选择和 0–10 次请求重试；失效模型要求用户明确重选。
- [x] Windows AppContainer 沙箱 backend：ReadOnly / WorkspaceWrite / FullAccess 文件边界、Disabled / Host 网络、环境变量白名单和沙箱临时目录。
- [x] 一次性命令和 ConPTY 均在挂起状态加入 AppContainer 与 Kill-on-close Job，失败、取消、超时和退出时回收完整进程树。
- [x] 受控域名网络威胁模型、WFP callout driver + NetworkGuard 服务 + HTTP/TLS broker 架构、安装门禁和验收标准，见 `09-windows-controlled-network-architecture.md`。
- [x] NetworkGuard driver/service/broker、后端签发、Connector exec/ConPTY wiring、readiness 门禁、每进程 SID、profile/ACL 回收和一键真机验收脚本已实现，不使用 `HTTP_PROXY` 等可绕过方案。
- [x] Controlled 上游可信链路：设备签名注册 Windows SID，ChatOS 后端不接收 UI 自报 SID/域名，Local Connector 仅从托管 permission profile 推导 allowlist；无权威策略时 Host/Disabled 正常，Controlled 明确失败。
- [ ] 在 Windows 10/11 x64 与 Windows 11 ARM64 使用 VS2022 + WDK 编译，通过 Microsoft Hardware Dev Center 取得 x64/ARM64 `microsoft_production` 包，执行 `build/networkguard-acceptance.ps1 -Disruptive`，证据通过后才把 Controlled 状态提升为 `Windows 验收`。

## 已完成代码、等待 Windows 验收的交付能力

- [x] Windows Server 2022 CI：串行测试、x64/ARM64 Release 构建和未打包产物上传。
- [x] Windows Server 2022 WDK matrix：x64/ARM64 无签名驱动与 Service 编译，校验 SYS/CAT/INF、manifest、schema v2、`signing_mode=unsigned` 和含 WDK 版本的 build report 后上传。
- [x] Windows 生产驱动签名流水线：生成并签名 Hardware Dev Center submission CAB，导入 Microsoft 返回包，核对 INF 未变化、SYS/CAT 为 `Microsoft Windows Hardware Compatibility Publisher`、可信时间戳并重建自包含 manifest/build report；本机证书只能标记为 `local_test`。
- [x] build/package 脚本验证真实 exe 或 MSIX/AppX 输出，禁止命令成功但没有交付物。
- [x] 单项目 MSIX manifest、构建时品牌资源生成、x64/ARM64 未签名包和 CI 上传。
- [x] 发布证书通过 CI Secrets 注入，使用 Thumbprint 签名，并校验 Publisher/Subject、Authenticode、Thumbprint 与时间戳；临时 PFX 和证书在任务结束时清理。
- [x] 关键路径 UI Automation：76 个稳定 ID 覆盖登录、Shell、设置、聊天、项目导航、本机终端、全局审批和宠物；静态唯一性/必备项/accessible name 契约测试已接入。
- [x] `ui-smoke.ps1` 支持默认匿名登录页检查，以及通过进程环境凭据执行真实登录、Shell、账号菜单和设置页检查；凭据在启动客户端前从进程环境清除且不写日志。
- [x] WindowsNative 原生验收测试：一次性命令、超时后 Job Object 子进程回收、AppContainer 工作区 ACL/禁网 capability、ConPTY 输入输出和 Credential Manager 随机 Secret 生命周期。
- [x] `build/native-acceptance.ps1` 生成 TRX、系统版本、构建/包 SHA-256 和无敏感正文的 JSON 验收报告；报告会重新解析 TRX，要求 9 个指定 WindowsNative 测试全部真实出现并通过，NetworkGuard Service 至少执行当前 19 项测试，CI 保留 14 天。
- [x] `build/msix-lifecycle.ps1` 在干净测试账号自动校验包身份/签名、可选旧版升级、打包 AppUserModelId 启动、UI Automation 和卸载清理；拒绝覆盖已有安装。
- [x] `build/package.ps1` 按 x64/ARM64 清理并隔离输出目录，禁止旧包被误当作本次构建产物。
- [x] MSIX 报告使用逐阶段 schema v2，native 与 NetworkGuard 报告升级为 schema v3；失败报告不再把未执行检查写成通过，MSIX 卸载残留会直接把报告改为失败，native/NetworkGuard 会保存并校验 TRX 测试名、执行数、结果和 hash，NetworkGuard 同时记录 SYS/CAT hash、签名状态、签名证书和失败阶段。
- [x] `build/verify-windows-acceptance.ps1` 统一校验 native、MSIX、NetworkGuard、x64/ARM64 WDK 五份证据；除检查报告字段外，还会对证据目录中的实际 MSIX、SYS、CAT、manifest 和双架构 WDK 产物重新计算长度与 SHA-256，拒绝路径逃逸、缺项、残留或伪通过。
- [x] `build/windows-final-acceptance.ps1` 提供一次性真机入口：接收 x64/ARM64 Microsoft 生产签名驱动包、当前架构签名 MSIX 构建、原生测试、认证 UI、MSIX 安装/升级/卸载、NetworkGuard disruptive/卸载和最终证据汇总；且必须确认一次性验收机。最终目录保存当前/旧版 MSIX 与两套驱动交付物；unsigned/local_test 必须显式降级开关且不能形成正式证据。
- [x] 验收校验器的普通成功、完整升级成功、缺少升级证据、产物篡改、缺少目标 E2E 测试、local_test 冒充生产和 WDK 路径逃逸夹具测试已加入 `build/test.ps1`；另有独立 TRX 正常/缺项/失败结果测试，防止零测试或提前返回重新产生伪通过。
- [ ] x64/ARM64 MSIX 安装、升级、卸载和签名信任链真机验收。
- [ ] Windows CI 首次远端运行、authenticated smoke 和 WDK x64/ARM64 build report 证据归档。

## 已完成的本机终端与国际化收口

- [x] 侧栏可按已配对 Connector 工作区创建多个独立本机终端资源。
- [x] 本机终端使用真实 `TerminalSessionManager` / ConPTY，支持输入、输出、停止、关闭和 200,000 字符界面上限。
- [x] 退出登录回收全部终端进程树；Connector 状态刷新失败不会清空已打开终端资源。
- [x] 全部 Desktop XAML 中文硬编码扫描为零；DataTemplate 使用应用级本地化资源，运行时语言切换无需重启。

## 已完成的全局宠物能力

- [x] 独立透明置顶 ToolWindow，不依赖主窗口前台；关闭登录态或总开关后停止 Realtime 并隐藏。
- [x] 使用系统物理坐标拖动整个窗口，消息框与宠物保持同一原生窗口；按方向翻转并播放脚步动画。
- [x] 多显示器工作区约束、动态尺寸保持宠物锚点、SQLite 持久化位置和离屏恢复。
- [x] 完成、失败、阻塞和运行中消息展示；终态不会自动丢失，可忽略或标记处理。
- [x] 运行中任务按 message/task/conversation/turn identity 取消，不使用模糊名称或哨兵值。
- [x] 本机审批直接拒绝、本次允许或本会话允许；Ask User 在宠物内展示完整字段和选项。
- [x] 点击宠物打开快捷聊天；叽咕狸固定首项，常用项目与其平级，最近消息/Realtime/发送复用正式聊天状态机。
- [x] 项目运行页提供“设为常用项目”开关，项目 ID 单独存入 `pet_preferences`。

Windows 真机验收步骤见 `07-windows-plugin-pet-acceptance.md`。
