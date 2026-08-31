# Windows / macOS 能力对齐矩阵

macOS 后续 Bug 修复、功能更新和协议变化先进入 `10-macos-change-sync-register.md`，完成 Windows 自动化与真机验收后再回写本矩阵。

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
| 认证 | 登录、恢复、退出、401 失效 | Windows 验收 | API 和 Credential Manager 已实现并测试，等待 Windows UI 验收 |
| Shell | 联系人、项目、终端、远端侧栏 | Windows 验收 | 联系人、项目、本机 Connector、独立多工作区本机终端和远端连接均已真实加载并进入对应页面 |
| Shell | 全局记事本和设置 Toolbar | Windows 验收 | 两个 Toolbar 均已接真实页面，记事本支持原地关闭返回工作区 |
| 聊天 | 历史分页与缓存 | Windows 验收 | 状态机、SQLite cache-first 和服务端校正已实现并测试 |
| 聊天 | 流式消息与工具过程 | Windows 验收 | Realtime 过程即时展示，终态刷新和未读新动态已实现 |
| 聊天 | Composer、附件、粘贴和拖放 | Windows 验收 | 选择、拖放、图片/长文本粘贴、限制和失败恢复已实现 |
| 聊天 | 模型、推理和 Plan 模式 | Windows 验收 | 权威配置读写、发送参数和 WinUI 控件已接入 |
| 聊天 | Ask User 直接处理 | Windows 验收 | 字段、密文、单选、多选、多行和原地提交/忽略已接入 |
| 任务 | 任务卡、任务图和详情 | Windows 验收 | 消息 Task Graph、真实任务名/状态、Run 详情和事件分页已接入聊天右侧原地面板 |
| 任务 | 执行过程、取消、重试和阻塞处理 | Windows 验收 | 任务详情面板支持原地取消、重试和权威 Graph 刷新，等待 Windows UI 验收 |
| 项目 | 文件目录、搜索、预览和编辑 | Windows 验收 | 真实 API、状态机和 WinUI 页面完成；默认预览，支持创建、原子重命名和确认删除，二进制不进入文本编辑器 |
| 项目 | Git 状态、Diff、历史和提交 | Windows 验收 | 安全路径、状态、双层 Diff、历史、暂存、提交、分支、远端、取消和 ff-only 拉取已完成并测试，不覆盖外部修改 |
| 项目 | Plan、Requirement 和执行范围 | Windows 验收 | 需求层级、任务、文档、Execution Plan 查询/创建、精确 identity 确认、放弃和停止已实现 |
| 项目 | 运行环境、目标和实例 | Windows 验收 | Catalog/State/Environment、目标分析、工具链与环境变量、启动/停止/删除、日志轮询和 WinUI 页面已完成 |
| 记事本 | 目录、搜索、编辑、预览和导出 | Windows 验收 | 服务端目录/笔记 API、稳定选择、切换前保存、默认预览、编辑/分栏、基础 Markdown 渲染和原生导出已完成 |
| 本机 | Connector 注册、心跳和重连 | Windows 验收 | ticket、设备/多工作区配对、独立 Connector 凭据、断开、状态轮询、managed trust、签名 WebSocket、心跳、有界重连和睡眠/唤醒恢复已完成并测试 |
| 本机 | Workspace 文件 Relay | Windows 验收 | 全部文件操作、原地响应、目录/盘符/UNC/ADS/symlink/junction 边界和可恢复覆盖移动已实现并测试 |
| 本机 | 代码导航与搜索 | Windows 验收 | 文件名/内容搜索与定义/引用启发式导航已完成；均限制当前项目边界、结果数量、超时、取消和短缓存，文件页可点击结果跳转 |
| 本机 | ConPTY 终端 | Windows 验收 | 原生 pseudo console 和一次性 terminal exec 均使用暂停启动 + Kill-on-close Job Object；超时、取消、完整进程树回收、有界输出及 Relay 408 已完成，等待 Windows ANSI/IME/复制和原生进程验收 |
| 远端 | SSH 测试与凭据 | Windows 验收 | 云端元数据 CRUD、本机 Credential Manager 凭据隔离、密码/私钥/证书认证、双段跳板机、SHA-256 主机指纹、取消/超时、二次验证码和 WinUI 编辑/测试已完成 |
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
| 设置 | 常规、连接、模型、插件、沙箱、审批 | Windows 验收 | 插件、模型同步、AI 审批 reviewer、审批策略、待处理/审计和 AppContainer 文件/网络边界设置已完成 |
| 设置 | 宠物、语言和字号 | Windows 验收 | SQLite 持久化和运行时即时生效已完成，等待 Windows 视觉与重启验收 |
| 安全 | 受控域名网络 | 实现中 | Windows SID 由设备私钥签名连接上报并服务端绑定，域名只从托管权限配置推导；后端策略签发、Relay、exec/ConPTY 挂起进程 lease、每进程 SID、Service/broker、WFP 驱动和端到端脚本均已完成；已增加 Hardware Dev Center CAB/微软签名结果导入，严格区分 unsigned、local_test、microsoft_production，正式验收只接受 Microsoft Hardware Compatibility Publisher；仅剩实际 WDK 编译、微软生产签名及不可绕过真机证据 |
| 发布 | x64 MSIX | Windows 验收 | manifest、品牌资源、隔离的 x64 输出目录、证书签名校验和自动安装/升级/打包启动/UI smoke/卸载证据脚本已接入；待干净 Windows 账号执行 |
| 发布 | ARM64 MSIX | Windows 验收 | ARM64 构建、隔离的未签名/签名包和同一生命周期验收脚本已接入；需 ARM64 Windows 真机执行 |
| 质量 | Core/API 自动化测试 | Windows 验收 | 当前 Core 20、API 49、Presentation 52、Connector 242、NetworkGuard 19，共 382 项测试通过；其中 9 项 WindowsNative 和 2 项显式启用的 NetworkGuard 端到端测试需在 Windows 执行真实系统 API |
| 质量 | Connector 集成测试 | Windows 验收 | 当前 Connector 242 项，覆盖插件、AI 审批、AppContainer profile/ACL 回收、NetworkGuard 策略/协议/lease/broker、服务端 Controlled readiness、宠物、终端和 Desktop Automation ID 静态契约；WindowsNative 还验证 suspended-before-lease、ConPTY acquire 失败不恢复进程和 Credential Manager |
| 质量 | NetworkGuard 自动化测试 | 实现中 | 当前 19 项单元/服务测试通过；端到端验收要求两个指定测试真实出现在 TRX，覆盖 Microsoft 生产签名门禁、同 IP denied SNI、HTTP/TLS、IP literal、DNS/DoH/QUIC/UDP、无 SNI、子进程、服务/驱动重启和 lease residue=0，等待 Windows 专用验收机生成证据 |
| 质量 | Windows CI | Windows 验收 | Windows 2022 x64/ARM64 restore、串行测试、WindowsNative TRX/JSON、Desktop Release、未签名/签名 MSIX，以及 x64/ARM64 unsigned WDK SYS/CAT/INF/Service 编译与 schema v2 hash 报告上传均已接入；生产驱动签名由 Hardware Dev Center 外部流程完成，统一校验器拒绝 local_test 冒充生产、零测试、缺项、残留或伪通过，等待远端首次运行 |
| 质量 | UI 自动化与可访问性 | Windows 验收 | 76 个稳定 Automation ID 覆盖登录、Shell、设置、聊天、项目导航、本机终端、全局审批和宠物关键路径；静态测试校验唯一性/必备项/显式 accessible name，smoke 支持匿名登录页和 Secret 驱动的真实登录后 Shell → 设置路径，等待 Windows CI 首次运行 |
