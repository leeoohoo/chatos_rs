# macOS 源码事实矩阵

> 状态：持续更新；Host lifecycle、Shell 与 Authentication 已进入 Windows React
> 分支：`3.0.9.2`
> 事实源：`clients/macos` 当前源码与测试、`clients/local_agent_host` 当前契约
> 用途：约束 macOS 重构与 Windows React 实现，不是界面截图说明书

## 1. 强制规则

Windows React 的业务逻辑只能从 macOS 当前源码、测试及其调用的 Rust Host 契约中提取。截图只回答“长什么样”，不能用来推导“怎么运行”。

每个页面必须依次通过以下状态：

1. `未映射`：只有页面名称或截图，不允许写 React 业务代码。
2. `源码映射中`：正在核对完整调用链，仍不允许写 React 页面。
3. `已映射，可重构`：View、状态机、Port、Adapter、Host 和测试证据齐全，可以开始 macOS 分层。
4. `macOS 分层完成，可进入 React`：Application/Port 已稳定且有测试，才允许创建对应 React feature。

“已映射”至少必须具备：

- 页面入口与导航来源；
- View 和所有参与业务决策的 ViewModel/Feature Model；
- Domain model、Application Port 与错误类型；
- Connector adapter、Host command/event、分页和并发语义；
- loading、空、成功、失败、取消、重试、断线和恢复状态；
- 对应自动测试；
- Windows 平台差异记录。没有差异时必须明确写“无”。

禁止事项：

- 根据截图发明默认值、状态迁移、重试策略、错误文案或数据结构；
- 因 React/Tauri 使用方便而绕开 macOS 的账号、版本、权限或幂等约束；
- 在 Port 尚未稳定时直接把 Swift Connector 调用翻译为 Tauri `invoke`；
- 将旧 `clients/windows` 的行为作为参考。

## 2. 首批垂直切片总览

| 切片 | 当前状态 | macOS UI | macOS 状态/用例 | Domain/Port | Connector/Host | 主要测试 | React 准入 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 项目会话准备 | Application 用例、账号绑定 per-project lifecycle、强类型展示阶段与 Conversation Session composition factory 已提取；Windows 已按同一四态接入 | `ProjectMessagesView.swift` | `AppModelResources.swift`、`PrepareProjectConversationUseCase.swift`、`ProjectConversationPreparationState.swift`、`ConversationSessionFactory.swift` | `ProjectConversationModels.swift` | `NativeLocalAgentWorkspaceService.swift`、Windows Registry + Host adapter | Preparation state/use case、Session factory、Connector 竞态/绑定、React latest-wins 与 Rust Registry 测试 | `macOS 分层完成，可进入 React`；项目会话准备已实现，不放行 Conversation 时间线 |
| 项目工作区列表 | 本地优先两阶段 loader、账号绑定 latest-wins lifecycle 与 await ownership 已提取；Windows 只读索引已按同一顺序接入 | `ResourceSidebar.swift` | `AppModelWorkspace.swift`、`ClientOwnedWorkspaceLoader.swift`、`WorkspaceLoadState.swift` | `ProjectRegistry.swift`、workspace models | `NativeLocalProjectsService.swift`、`NativeLocalAgentWorkspaceService.swift`、`NativeProjectRunService.swift`；Windows SQLite Registry + Host relation adapter | Load state、两阶段 loader、React state/projection 与真实 Host 请求测试 | `macOS 分层完成，可进入 React`；只读索引已实现，创建/编辑仍禁止 |
| 项目目录与文件操作 | macOS 目录、预览、搜索和 mutation 契约与 Windows workspace grant 边界已映射；Windows 项目工作面、相对路径目录树、文件读写与 lifecycle 已接入 | `ProjectWorkspaceView.swift`、`ProjectDirectoryView.swift`、`ProjectFileEditorView.swift` | `ProjectDirectoryViewModel.swift`、`ProjectDirectoryExpansionStateStore.swift` | `ProjectFilesystemModels.swift`、`ProjectFilesystemMutationUseCase.swift` | `NativeProjectFilesystemService.swift`、`NativeWorkspaceFilesystem.swift`；Windows Registry/workspace resolver + Tauri filesystem adapter | macOS mutation lifecycle、Windows Rust path/revision/mutation 与 React domain/store/UI 测试 | 目录、预览、搜索、编辑、新建、删除、Git 与项目设置均已进入 React |
| 项目运行设置 | macOS View/Feature/Application/Port 已完整分层；Windows analyzer、typed process、实例状态、偏好/Secret、Removal、Favorite 与 React 项目设置闭环已完成 | `ProjectRunSettingsView.swift`、`ProjectRunInstancesSection.swift`、`ProjectRunEnvironmentSection.swift` | `ProjectRunSettingsViewModel.swift`、`ProjectRunLifecycleState.swift`、`AppModelResources.swift`；Windows `projectRunStore` | Project Run/Removal/Favorite models 与 use cases；Windows `features/project-run` Domain/Port | macOS Run/Registry/Pet adapters；Windows `project_run`、`project_removal`、`project_favorite` + workspace resolver + Tauri Adapter | macOS Run 15、Removal 15、Favorite 8；Windows Rust Project Run 13、Removal 1、Favorite 1；React Domain/Store/UI/跨页测试 | `macOS 分层完成，可进入 React`；Windows 完整纵向切片已实现 |
| 远程连接资源 | 列表、管理、MFA、SFTP 与交互式 Terminal 均已通过 Application use case、Core byte-stream Port 和 typed lifecycle 分层；导航 latest-wins，mutation single-flight，连接/session generation 隔离迟到结果，凭据与原生进程留在 Connector | `ResourceSidebar.swift`、Remote Connection Editor/Detail/SFTP/Terminal Views | Remote Connection/SFTP/Terminal ViewModels 与三类 lifecycle state | Remote Connection/File models/use cases、`RemoteTerminalSessionPort`/use case | `NativeRemoteConnectionService.swift`、`NativeRemoteFileService.swift`、`NativeRemoteTerminalPortAdapter.swift` | 管理、SFTP、Terminal Core/App/Connector 释放/竞态/MFA/PTY 测试 | `macOS 分层完成，可进入 React`；Windows metadata/Credential Manager、SSH/MFA/host-key、SFTP、ConPTY/OpenSSH typed session 与 xterm.js 工作区均已完成 |
| 本机终端 | 多标签、搜索、清屏、中断、OSC title/目录与 PTY 生命周期已通过 Core byte-stream Port、Connector session 和 Feature ViewModel 分层；Shell、环境、进程句柄与信号不再进入 UI | `TerminalWorkspaceView.swift`、`TerminalComponents.swift` | `TerminalWorkspaceViewModel.swift`、`NativeLocalTerminalViewModel` | `LocalTerminalUseCase.swift` | `NativeLocalTerminalSession.swift` | Core lifecycle/use case、真实 PTY、输出预算、App renderer/workspace 测试 | `macOS 分层完成，可进入 React`；Windows 独立 owner/presentation-scoped ConPTY session、cwd/环境、进程树回收、typed Port/Store 与 Apple 主题 UI 已完成 |
| 插件应用列表 | Listing 与 Launch 已分别提取窄 Port/Application Use Case；列表和 Host 页面均使用账号绑定的 typed lifecycle，项目根路径仅在 Core/Connector 间流转，WebView/iframe 只接收脱敏 bridge context；Windows 完整纵向切片已完成 | `PluginApplicationsView.swift`、`ResourceSidebar.swift`；Windows `features/plugin-applications` | `AppModelWorkspace.swift`、`PluginApplicationsLoadState.swift`、`PluginApplicationHostViewModel.swift`；Windows `pluginApplicationStore` | `LocalConnectorCatalogModels.swift`、`PluginApplicationLaunchUseCase.swift`；Windows Domain/Port | `NativeLocalConnectorService+Plugins.swift`、`NativeLocalProjectsService.swift`；Windows Tauri catalog/runtime/bridge | Listing/Launch state/use case、Host ViewModel 释放/竞态、Connector、Windows Rust 与 React lifecycle/bridge 测试 | `macOS 分层完成，可进入 React`；Windows catalog/runtime/bridge、typed Store/Adapter、context picker 与 sandbox iframe 已完成 |
| Conversation 时间线与发送 | macOS 分层与三端共享 fixture 已完成；Windows History、Composer、显式能力、附件闭环、runtime controls 与联系人入口已接入 | `ConversationTimelineView.swift`、`ContactConversationView`、`ComposerView.swift`、`ReasoningLevelControl.swift` | `ConversationSessionViewModel*.swift`、`ConversationSessionFactory.swift`、Conversation Application 类型 | `ConversationModels.swift`、`ConversationRuntimeSettings.swift`、Conversation ports | `NativeLocalAgentConversationService.swift`、`NativeLocalAgentConversationRuntimeSettingsService.swift`、Host conversation/attachment commands | Conversation Core policy/use case、附件导入、runtime settings load/mutation state、Session factory、ViewModel 与 React 联系人组合测试等 | `macOS 分层完成，可进入 React`；Windows 项目与联系人核心发送闭环已实现 |
| Task Graph / Run Inspector | Graph、realtime、Run、section 刷新、Inspector request identity、Workspace 基础详情→section 顺序加载、内嵌 Inspector Task 复用/section polling 恢复/presentation/retry identity 与 Application use case 路由、polling session 与 Workspace presentation session 已提取；Windows 已接入基础详情、过程、Run、重试、停止、恢复和 polling | `MessageTaskWorkspaceSheet.swift`、`MessageTaskInspectorView.swift`、`TaskProcessTimelineView.swift` | `MessageTaskWorkspaceViewModel*.swift`、Task Graph/Run/Inspector Application 类型 | `MessageTaskRunModels.swift` 及 task graph models | `NativeLocalAgentMessageTaskGraphService.swift`、Host task/run/event commands | Core state/use case、ViewModel、Connector、共享 fixture、React latest-wins 与 Rust projection 测试 | `macOS 分层完成，可进入 React`；Windows 主检查器纵向切片已实现 |
| Ask User | 查询/提交/取消、排序投影、操作门禁、表单状态、refresh/per-prompt 异步生命周期与 activation/action recovery/focus refresh await ownership 已提取；Windows 已按同一语义接入安全投影、表单、提交、取消和失败对账 | `AskUserPromptCardView.swift`、Conversation 内嵌入口 | `ConversationSessionViewModel+AskUser.swift`、Ask User Application 类型 | `AskUserPromptModels.swift`；Windows `features/ask-user` Domain/Port/Store | `NativeLocalAgentAskUserPromptService.swift`、Host run/conversation commands；Windows Tauri typed commands | Ask User Core、Connector、ViewModel、React domain/store/UI 与 Rust projection/validation 测试 | `macOS 分层完成，可进入 React`；Windows 完整纵向切片已实现 |
| 工具审批 / 恢复 | Human Approval Port、加载/强类型决策用例、全局队列展示、监控 session、独立页面加载/决定 Task、Host durable/Human 两阶段端口与决定写回恢复 journal 已提取并通过干净快照验收；Windows 已接入 durable queue/单次决策、全局浮层与原决定恢复 journal | `GlobalApprovalOverlayHost`、完整审批页；Windows `GlobalToolApprovalOverlay` | `LocalConnectorControlCenterViewModel.swift`、Approval Application 类型；Windows `toolApprovalStore` | Local Connector approval models/ports；Windows `features/tool-approval` Domain/Port | platform tool worker、approval handler、Local Connector approval service；Windows Tauri typed commands/journal | Core use case/state、worker、approval handler/agent、state store、ViewModel tests；Windows projection/store/UI/journal tests | `macOS 分层完成，可进入 React`；Windows durable decision/recovery 已实现，真实 platform worker 完成前不发布对应执行能力 |
| Host lifecycle / Shell | Runtime ready、local-first launch、authenticated bootstrap retry、health/restart、service configuration 与 shutdown orchestration 均已进入 Application state/use cases；v44 health 契约已由 Rust/Swift/TypeScript 共享 fixture 冻结；React/Tauri 已接入双 Host control-plane bootstrap、Credential Manager 模型恢复、完整快照离线回退、最终 health ready gate 和附件 platform worker | `RootView.swift` 与各页面 ready gate | `AppModelAuthentication.swift`、Runtime/Bootstrap/Recovery state、Host launch/bootstrap/configuration/shutdown use cases | `LocalAgentHostLifecycle.swift`、Host launch/bootstrap/configuration/shutdown Ports | Native Host process、App composition adapters、Windows Tauri lifecycle/control-plane/attachment worker adapter | Runtime/bootstrap/recovery/launch/configuration/shutdown、共享 fixture、React gate、凭据环境净化、附件 vault/worker 与 Tauri tests | `macOS 分层完成，可进入 React`；Windows 只发布已真实执行的 `local_attachment_read` platform 能力 |
| Authentication | 四阶段 Gate、登录/注册、恢复重试、注册验证码倒计时、logout/login 排序与 latest-wins 已由 ViewModel 和稳定 Core Port 隔离；Windows 已按同一契约实现 | `AuthenticationGateView.swift`、`LoginView.swift` | `AuthenticationViewModel.swift` | `AuthenticationModels.swift`、`CredentialStore.swift` | `ChatOSAuthenticationService.swift`、`ChatOSAPIClient.swift`、`KeychainCredentialStore.swift`；Windows Tauri auth adapter + Credential Manager | `AuthenticationViewModelTests.swift`、Rust DTO/URL tests、React application state tests | `macOS 分层完成，可进入 React`；Windows 首切片已实现 |
| Pet 翻译 | 模型、历史、附件与翻译 operation identity 已提取；Host/磁盘等待不再强持有窗口 ViewModel | Pet 翻译浮窗 | `PetTranslationViewModel.swift` | `PetTranslationAgent` 请求/结果、历史记录模型 | Agent model provider、`PetTranslationHistoryStore` | Agent、历史持久化、ViewModel 生命周期测试 | 禁止；等待完整源码映射与 Port 验收 |
| Media / Story Studio | Media Studio 的账号 session、模型 generation、图片/视频 operation identity、历史、导入、生成、取消与保存已完成分层并进入 Windows；Story 项目目录、创建/设置、规划 Agent runtime、历史分页、安全制作批次投影、持久化批次执行、仅视频顺序队列、素材/首尾帧图片、单段视频生成恢复、通用编辑保存、图片导入、视频实际末帧重提取及完成视频归档已进入 Core Port/Use Case/typed lifecycle，媒体 URL 投影继续按调用链收口 | AI 图片、AI 视频、AI 故事、AI 历史 | `MediaStudioViewModel.swift`、`MediaStudioVideoWorkspace.swift`、`StoryStudioViewModel*.swift`；Windows `mediaStudioStore` | Media 图片/视频 Port 与 Application use case；Story Project Catalog/Mutation/Planning/History/Media Batch/Image Import/Asset Image/Frame Image/Video/Tail Extraction Port 与 Direct Video Queue Use Case；Windows strict Domain/Port | macOS Media service/adapters、Story project store/provider/Agent runtime/history/media batch/import/asset/frame/video/tail adapter；Windows provider、历史、preview、reference/audio vault 与 job registry | macOS Media 43 项；Windows Media Studio Rust 36 项、React Domain/Adapter/Store/UI 31 项；Story Core 54 项、Story Studio 66 项、Story Agent 21 项 | AI 图片与视频已完成 Windows React 纵向切片；剧情模式继续禁止，等待 Story media display 与平台门禁 |
| Agent Group Chat | Team/Direct change stream 保持弱 UI 订阅；补充卡片、附件 hydration、account/communication scheduler drain 与 Run refresh 均已冻结请求上下文，存储/Agent 等待不再强持有 ViewModel | Agent 私聊、团队聊天与团队工作区 | `AgentDirectChatViewModel.swift`、`AgentGroupChatViewModel.swift` | Agent room/message/proposal/todo/run models | `SQLiteAgentGroupChatStore`、Local Agent scheduler | Codable、迁移、Store、Scheduler、UI presentation/performance tests | 禁止；等待完整页面状态与操作映射 |

以上“React 准入”不是进度建议，而是代码审查门禁。

### 2.1 项目工作区列表刷新

- `ClientOwnedWorkspaceLoader` 保持 macOS 的本地优先顺序：先只从 Project Registry 组合项目并立即发布，再解析设备身份、修复 root workspace binding，最后刷新远端联系人和 Conversation 关系；
- 未配对时项目仍可见；远端关系失败时保留最后可信联系人与 Conversation，不能用空响应覆盖；远端关系不得创建、重命名或复活项目；
- `WorkspaceLoadState` 将 owner、account generation 与 request identity 固定为一个 typed token，新刷新、项目删除、账号切换、登出与应用退出都会使旧 token 失效；
- 刷新 Task 在开始时冻结 Projects、Workspace Relations 与 Project Run 服务，各段 registry/Connector/Host await 不强持有 AppModel；每次快照准备、展示写回、错误和 loading 收尾都重新验收 token；
- `NativeProjectRunService` 的 project root 映射跟随每次已验收快照更新；UI 继续只消费 `WorkspaceResourceResolver` 输出，不直接读取 Connector/Host 状态。

Windows 只读索引实现（2026-10-20）：

- Tauri/Rust 中建立与 macOS 同 schema 的账号隔离 `Projects.sqlite3`，第一阶段只读取 `status = active` 的项目并立即返回；数据库损坏或字段无效时失败关闭，不伪装成空列表；
- 第二阶段通过账号绑定的 Host v47 `ListConversations` 分页读取关系，并按 macOS 规则保证“叽咕狸”和“插件安装助手”两个系统联系人会话存在；
- Host 中出现 project conversation 但本地 Registry 没有对应项目时，只保留关系记录，绝不凭远端关系创建或复活项目；
- React `WorkspaceIndexPort`、Application store 和 resolver 保留 local-first 顺序、账号切换 latest-wins、远端失败保留项目及显式错误/重试；
- Host 业务请求统一使用有界 framed transport。协议、IO、EOF 或超时会终止假健康进程；普通业务拒绝不会误杀健康 Host；
- 侧栏索引仍以本地 Registry 为权威；项目创建与显式文件夹授权已在独立切片通过后开放。导入、重命名、删除及其他 project context 变更仍须等待各自完整切片。

### 2.1.1 联系人会话入口

- macOS `ContactConversationView` 不建立第二套消息服务：选中联系人后从 `WorkspaceResourceResolver` 取得公开 Conversation ID，激活同一 `ConversationSessionViewModel` 并展示 `ConversationTimelineView`；切换资源时停用旧 session；
- 联系人没有公开 Conversation 时只展示“还没有公开会话”空状态，不从 UI 发明 Conversation。Workspace Index 已按 Host 真值确保“叽咕狸”和“插件安装助手”两个系统联系人及其会话存在；
- “插件安装助手”唯一的入口差异是时间线上方的隔离提示，明确 MCP、npm 包或本地 Plugin 文件夹安装能力只在该对话可用；能力本身仍由 Conversation capability snapshot 与 Host policy 决定，React 不因联系人 ID 私自发布工具；
- Windows 新增薄 `ContactConversationView` 组合层，直接复用已准入的 History、Composer、Attachment、Realtime、Runtime Settings、Run Inspector 与 Ask User Ports；AppShell 不再显示占位卡。3 项入口测试覆盖普通联系人复用、安装助手提示和无会话空态；React 全量为 47 个测试文件、188 项通过。

### 2.2 项目目录只读树、文件预览与搜索

macOS 事实源：

- `ProjectWorkspaceView` 默认项目 tab 为“用户消息”，并在“项目目录 / 用户消息 / 项目设置”之间切换；新建项目成功后才主动切到目录；
- `ProjectDirectoryViewModel` 持有目录缓存、展开路径、选中项、搜索与文件内容；根目录加载、子树展开、折叠淘汰和 connector retry 是独立生命周期；
- `ProjectFilesystemServicing` 固定 list/search/read/write/create/delete/open 端口；首个 Windows 子切片只采用 `listEntries`，不能因接口存在就提前开放其他操作；
- `NativeWorkspaceFilesystem.list` 单目录最多 5,000 项、文件夹优先、忽略符号链接、返回相对路径与 truncated 标记；
- expansion store 只保存展示偏好，不是文件授权；workspace grant 与每次路径 canonicalization 才是权限事实。

Windows React 已完成只读目录树：

- Tauri 通过账号隔离 `Projects.sqlite3` 解析项目的 workspace/relative root，再与当前 owner 的 Connector grant、device identity 和 canonical root 联合验收；
- 调用只接受规范化便携相对路径，拒绝绝对路径、盘符、反斜杠、控制字符、`.`/`..` 和越界 canonical path；绝对根目录从不发送给 React；
- 目录列表与 macOS 一致最多 5,000 项、文件夹优先排序并跳过符号链接，只投影名称、相对路径、类型、可写位、大小和修改时间；
- React Store 以 owner/project generation 和 per-path request identity 管理根目录、展开与刷新；折叠目录会使其子树的迟到请求失效，切换项目或离开页面清空展示状态；
- 项目工作面默认显示用户消息；目录 tab 已开放，项目设置保持禁用并明确等待对应页面准入；
- 目录树与文件元数据之外，项目创建/授权、内容预览与搜索、编辑及文件创建/删除已由后续独立切片开放；Git 与项目设置仍继续禁止；
- Windows 定向验收为 Rust 88 项通过、2 项 sidecar 环境测试忽略，React 37 个文件 132 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过。

### 2.2.1 项目创建与 Windows 文件夹授权

macOS 事实源与 Windows 平台适配：

- `CreateProjectSheet` / `CreateProjectViewModel` 固定项目仅保存在本机、当前所选目录本身成为项目根、创建不会自动新建同名目录、成功结果在弹窗离开后发布；Windows 用原生文件夹选择器产生显式目录授权，不复制 macOS 的已配对根目录浏览器；
- `NativeLocalProjectsService` / `SQLiteProjectRegistry` 固定 owner-scoped Registry、创建前目录复验、同目录 active/archived 防重，以及成功 Registry 写入不因后续 pairing 变化伪装成失败；Windows 使用稳定 workspace ID/fingerprint 和跨 Connector JSON/SQLite 的恢复 journal 实现同一边界；
- React 只持有 owner + presentation 绑定的一次性 selection ID 和目录显示名；绝对路径、canonical root 与 workspace grant 永不进入 WebView。弹窗关闭会在 React 和 Rust 两层淘汰迟到选择器结果，账号替换和保存结果也采用独立 identity；
- 原生提交再次拒绝文件、符号链接、Windows reparse point、失效目录和 owner 不匹配；活动或归档项目不能重复注册相同 canonical root，removed 记录不阻止 Human 重新创建；
- 27 个项目类型的 key、中文标签、说明和分类由 Rust 测试逐项对照 macOS `ChatOSSkillCatalog.json`，React 不维护第二份类型真值；
- 成功后先关闭弹窗，再加入当前账号的 Workspace Index、选中新项目并保持 macOS 默认“用户消息”工作面；不隐式创建 Conversation、Git 仓库或 remote；
- Windows 定向验收为 Rust 95 项通过、2 项 sidecar 环境测试忽略，React 40 个文件 141 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过。

### 2.2.2 项目文件内容预览与只读搜索

macOS 事实源与 Windows 实现：

- `ProjectDirectoryViewModel` 的文件选择与 280 ms 搜索防抖分别映射为 preview/search request identity；切项目、离开目录页、清空查询、选择新文件或发起更新查询会立即淘汰旧结果；
- `NativeWorkspaceFilesystem.read` 的边界保持一致：文本最多 2 MiB、图片最多 25 MiB，前 8,000 字节含 NUL 视为二进制，非二进制按 UTF-8 容错解码；图片、普通二进制与文本使用明确的强类型投影，React 不接收绝对路径；
- 文件名与相对路径搜索、内容搜索并行执行，分别最多返回 100/200 项；Rust 原生层统一限制任一调用最多 500 项、20,000 次访问和 3 秒，内容搜索单文件最多 2 MiB，行号和列号从 1 开始，单行摘要最多 2,000 字符；
- 搜索与 macOS 一致大小写/变音符号不敏感，并跳过 `.git`、`.build`、`.cache`、`.next`、`.idea`、`.vscode`、`node_modules`、`DerivedData`、`Pods`、`target`、`dist`、`build`、`vendor`；目录枚举本身也按剩余访问预算截断，不能先把超大目录完整读入内存；
- 每次读文件或搜索都重新从 owner-scoped Registry 与 workspace grant 解析 canonical root，并逐级拒绝符号链接和 Windows reparse point；React schema 再次约束相对路径、大小、编码、数量与摘要长度；
- Apple 风格侧栏新增搜索框、分组结果、截断提示和局部 loading；右侧提供文本目标行、图片棋盘背景、二进制不可预览与可恢复错误状态。此切片只读，不开放写入、创建、删除、外部打开、Git 或项目设置；
- Windows 全量验收为 Rust 97 项通过、2 项 sidecar 环境测试忽略，React 40 个文件 144 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过。

### 2.2.3 项目文件写操作准入

macOS 已完成写操作分层与生命周期收口：

- Core `ProjectFilesystemMutationUseCase` 统一保存后重读、创建后刷新父目录和删除后刷新父目录；`ProjectDirectoryViewModel` 不再自行串联多段 filesystem service 调用；
- 保存冻结目标路径和草稿，创建冻结父目录/名称/类型，删除冻结目标/父目录/类型；同一 ViewModel 只允许一个 mutation，迟到结果必须同时匹配 mutation identity 与当前选择；
- 用户在保存等待期间切换文件时，旧文件仍按明确动作完成写入，但迟到重读不能替换新文件的预览、草稿或编辑状态；
- 写入、创建或删除已成功，而随后的 read/list 刷新失败时，Core 返回 committed-without-refresh 结果；UI 明确显示“操作已完成但刷新失败”，不能把已发生的副作用伪装成整体失败；
- 新增 3 项 `ProjectDirectoryMutationLifecycleTests`，覆盖迟到保存、创建/删除父目录刷新，以及提交后刷新失败。该边界现允许 Windows React 实施；Windows 还必须补齐 canonical path/reparse point、原子写、内容 revision 冲突和 owner/project/mutation identity。

Windows React/Tauri 已完成对应垂直切片：

- Rust 的 `write_project_file/create_project_entry/delete_project_entry` 每次都从 owner-scoped Registry 与 workspace grant 重新解析项目根，逐级拒绝 symlink 和 Windows reparse point；React 只提交规范化相对路径；
- 文件读取投影携带 SHA-256 opaque revision；保存与删除文件必须复验 revision，外部修改产生明确冲突并保留 React 草稿，不能静默覆盖。写入最多 2 MiB，使用同目录临时文件、同步与 Windows `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`/Unix rename 原子替换；
- 新建文件使用 create-new，新建目录只创建单层且不覆盖；删除目录不携带文件 revision、不能删除项目根，并按 macOS 语义递归删除所选目录；
- Zustand Store 冻结 owner、project、generation、目标路径、revision、草稿、父目录、名称和类型；同一展示 session 只允许一个 mutation。保存期间切换文件不会取消已授权副作用，但迟到结果不能覆盖新选择；切项目、刷新或离开页面会淘汰旧 UI 写回；
- 保存后的 read 或新建/删除后的 list 失败均作为 committed-without-refresh 展示。保存内容保留在当前预览并暂时标为只读，防止继续使用旧 revision；创建/删除保留最后可信目录并提示刷新，不把已完成副作用误报为失败；
- 创建父目录严格复用 macOS 规则：选择目录时在该目录创建，选择文件时在其父目录创建，无选择时在项目根创建。编辑仅对 `isWritable && !isBinary` 的内容开放；删除通过 Apple 风格确认弹框执行；
- React UI 使用苹果主题的编辑器工具栏、新建 sheet、分段类型选择器、危险操作确认框和局部错误反馈，没有开放尚未验收的 Git 或项目设置；
- Windows 全量验收为 Rust 98 项通过、2 项 sidecar 环境测试忽略，React 40 个文件 153 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过。

### 2.2.4 项目 Git 工作面准入

macOS 事实源与已完成分层：

- 页面事实源为 `ProjectGitWorkbenchView`、`ProjectGitChangeListView`、`ProjectGitHistoryView`、`ProjectGitDiffView` 与三个 branch/remote sheet；状态事实源为 `ProjectGitViewModel`，契约为 `ProjectGitModels.ProjectGitServicing`，原生实现为 `NativeProjectGitService` 与 `NativeProjectGitParser`；
- snapshot 固定读取 repository root、HEAD/当前或 detached branch、upstream、ahead/behind、porcelain v1 `-z` changes、本地 branches、最多 80 条全分支提交历史和 remotes。变更同时按 index/worktree 状态进入 staged 与 working-tree 分组，冲突优先、其余按路径排序；
- diff 只允许 snapshot 中的项目相对路径，普通/暂存差异最多 4 MiB；未跟踪文件使用 `--no-index /dev/null`。UI 最多解析 8,000 行并保留 hunk、旧/新行号、增删和上下文语义；
- mutation 包含 init、stage/unstage、commit、switch/create/merge branch、remote add/rename/set-url/remove、`pull --ff-only` 和 push；无 upstream 时 push 使用首个 remote 发布当前 branch。单次普通 Git 命令默认 120 秒，pull/push 15 分钟，普通输出最多 16 MiB；原生进程超时会终止整个进程组；
- repository root 必须位于 Connector workspace root 内，文件路径与 snapshot absolute path 必须同时通过 repository 边界复验；branch 由 `git check-ref-format --branch` 验证，commit message、remote name/url 去除首尾空白后不得为空；
- 多步“mutation→snapshot refresh”已从 ViewModel 移入 Core `ProjectGitUseCase`。副作用成功但 snapshot 刷新失败返回 committed-without-refresh，仍清理已提交的旧提交说明、发布 working-tree revision，并明确提示刷新失败，不把成功 mutation 误报为失败；
- ViewModel 的 load、diff 和 mutation 分别使用 request identity。新 load/diff 淘汰旧请求；mutation 与 load/diff 隔离且同一时间只允许一个。迟到状态/差异不得覆盖当前展示，提交期间新输入的 message 不得被旧提交完成清空；mutation 失败后尝试恢复最新 snapshot，但保留原始操作错误；
- 新增 5 项 `ProjectGitViewModelTests`，并复验 4 项 `NativeProjectGitServiceTests` 与 3 项项目文件 mutation lifecycle 测试。macOS Git 已达到 Windows 原生边界实施准入。

Windows 原生边界（2026-10-20）：

- `project_git.rs` 已提供 snapshot、diff、init、stage/unstage、commit、switch/create/merge branch、remote add/rename/set-url/remove、`pull --ff-only` 与 push 的独立 Tauri command；没有开放通用 Git arguments 或 Shell 入口；
- 每个入口先按 owner + active Project Registry + Connector workspace grant 重新解析 canonical project/workspace root；`rev-parse --show-toplevel` 得到的 repository root 必须仍位于 workspace 内。stage/unstage/diff 会重新读取 snapshot，并只接受当前 snapshot 中具有对应 index/worktree 状态的规范化便携相对路径；
- React 安全投影不包含 repository root、absolute path 或 raw stderr。remote URL 会移除所有 URI userinfo、query、fragment，SCP 风格疑似密码，并隐藏 `file://`、Unix/Windows 绝对本地路径；这是 Windows 为 Secret 与本机路径隔离增加的安全投影，原始 URL 只留在 Rust 命令边界。投影同时携带 `urlIsRedacted`，编辑已脱敏 remote 时可发送 `url: null` 显式保留原始地址，避免仅改名就破坏凭据；
- Git 子进程固定 `GIT_TERMINAL_PROMPT=0`、`GCM_INTERACTIVE=Never`、`LC_ALL=C`，stdout/stderr 并发 drain 且合计有界；普通命令 120 秒、pull/push 15 分钟、diff 4 MiB、其余输出 16 MiB。Unix 使用独立 process group，Windows 使用 kill-on-close Job Object；超时或 Git 主进程退出后回收整个进程树，避免 hook/helper 保留 pipe 造成假存活；
- branch 和 remote name 经 `git check-ref-format` 复验；commit message、remote name/url 具有空值、首尾空白、控制字符和长度门禁。所有失败只返回固定安全分类，不把命令 stderr、绝对路径或凭据带到 WebView；
- mutation 成功后返回 nullable snapshot 与显式 `committedWithoutRefresh`，刷新失败不会把已经发生的副作用误报成失败；commit log 使用 NUL 字段解析，最多 80 条，并对 object ID、字段长度、UTF-8 和控制字符执行 schema 验证；
- 新增 8 项原生测试，覆盖真实仓库 status/stage/commit/diff、branch/merge/remote、snapshot 路径成员关系、workspace 越界、输出上限、超时进程组、remote Secret 净化、committed-without-refresh 与 DTO 无绝对路径。Windows Rust 全量为 106 项通过、2 项 sidecar 环境测试忽略，`clippy -D warnings` 通过；
- React 已建立独立 Domain schema、`ProjectGitPort` 与 Tauri Adapter。Zustand Store 与 macOS `ProjectGitViewModel` 一致地隔离 load/diff/mutation identity：mutation 淘汰旧 load/diff，同一时间只允许一个 mutation，项目切换/页面离开淘汰迟到写回，失败恢复 snapshot 时保留原操作错误，committed-without-refresh 不误报副作用失败，提交期间新输入的 message 不被旧提交完成清空；
- 项目工作面新增 Git 分段页，并以 Apple 主题实现 branch/merge/remote sheet、提交 composer、暂存区/工作区双分组、最多 80 条历史、ahead/behind、pull/push 和 8,000 行语义 diff；从 diff “打开文件”会切回目录并按项目相对路径读取。新增 Domain、Store、UI 与跨页测试后，React 全量为 43 个测试文件、170 项通过，TypeScript、Vite production build 与架构审计通过；项目 Git 已进入 React，项目设置继续禁止。

### 2.2.5 项目运行设置源码映射与 macOS 分层

macOS 事实源与当前行为：

- `ProjectWorkspaceView` 的“项目设置”实际承载项目运行设置；主页面由 `ProjectRunSettingsView`、`ProjectRunInstancesSection` 与 `ProjectRunEnvironmentSection` 组成，另包含宠物快捷访问和“从 ChatOS 删除项目”两个跨功能区块；
- 首次展示先订阅 `ProjectRunServicing.changes(projectID:)`，再并发读取 catalog、state 和 environment，避免进程在初始读取与订阅之间退出而令 UI 永久陈旧；显式刷新重新读取完整 snapshot，进程 change stream 只刷新 state；
- catalog 由 `NativeProjectRunAnalyzer` 扫描项目入口和清单产生，固定投影 target 的 label/kind/language/cwd/command/source/entrypoint/manifest/required toolchains；默认 target 按项目偏好恢复，无效旧选择回退到首个目标；
- environment 展示工具链候选、配置文件预览、运行前 validation issues、自定义工具链选择和环境变量。偏好只保存在本机；启动时将选择的可执行文件目录加入 `PATH`，并把项目环境变量合并进子进程环境；
- 每次启动创建独立实例。当前 macOS Adapter 使用 `/bin/zsh -lc <target.command>`，合并 stdout/stderr，单实例日志只保留最后 512 KiB；start/stop/delete 与进程退出通过 change stream 发布，退出实例默认最多保留 100 个；
- Core `ProjectRunUseCase` 已把三路 snapshot、analyze、默认目标、环境保存、start/stop/delete 编排从 ViewModel 移出，并明确区分“副作用提交失败”和“副作用已完成但展示刷新失败”；后者保留最后可信状态并显示刷新警告，不能误报启动或停止失败；
- `ProjectRunLifecycleState` 为完整 load、realtime state refresh 和互斥 mutation 建立 request identity。新 load 淘汰旧 load；较新的 realtime state 可只淘汰旧 snapshot 中的 state，不影响 catalog/environment；mutation 淘汰在途读取并拒绝并发 mutation；离开页面 reset 后所有迟到结果均不可写回；
- 环境保存冻结提交时的工具链与变量。保存期间用户继续编辑的新草稿不会被旧请求完成覆盖；mutation 失败后的恢复 snapshot 保留原始操作错误，并同样不能覆盖更新后的环境草稿；实例选择在刷新后尽量保持，仅在实例已经不存在时回退到最新实例；
- 定向验收覆盖 7 项 Core Use Case/Lifecycle、4 项 ViewModel 竞态/草稿测试，以及 4 项 Native analyzer/service 测试，共 15 项通过。

准入前必须收口的同页边界与 Windows 差异（现均已按下述约束实现）：

- 项目删除已提取 `ProjectRemovalServicing` 与 `ProjectRemovalUseCase`：先按 owner/project 读取 revision，再以 optimistic revision 标记 Registry 记录为 removed；返回 receipt 冻结 owner、account generation、project 与 revision，只有账号上下文仍一致时 `AppModel` 才失效 workspace load，并清理 Conversation cache、宠物收藏、Project Run roots 与 selection；本机项目目录始终不删除。Core 3 项与 `NativeLocalProjectsServiceTests` 12 项通过；
- 宠物快捷访问已提取窄 `ProjectFavoritePreferences` Port 与 `ProjectFavoriteUseCase`；`ProjectRunSettingsView` 只通过 Feature ViewModel 读取和切换当前项目收藏，不再直接操作 `PetPreferencesStore`。当前 macOS 产品事实仍是设备级 `UserDefaults` 集合而非账号级存储；展示时与当前账号项目列表取交集，因此不会把另一账号项目展示出来。Windows 必须保留该语义，除非另行批准账号级迁移；
- Windows 不复制 `/bin/zsh -lc`。Tauri Adapter 必须从 owner-scoped Project Registry 与 workspace grant 重新解析根目录，只接受由受信任 analyzer/配置产生的 typed target，不向 React 暴露任意 shell、绝对路径、原始环境 Secret 或进程句柄；
- Windows 进程必须使用 Job Object 回收整棵进程树，stdout/stderr 并发有界 drain，并具有启动/停止超时、退出事件和账号/项目 teardown；启用交互终端时通过独立 ConPTY typed session 实现，不能把通用命令执行桥伪装成项目设置能力；
- macOS 同页 View/Feature State/Application/Port/Adapter 已完整映射并通过准入；可以开始 Windows 原生 Adapter，但在其路径、进程树、日志上限、Secret、超时和恢复测试完成前，不得创建 React 项目设置 UI。

Windows analyzer 第一阶段（2026-10-20）：

- 新增唯一 typed `analyze_project_run(ownerUserID, projectID)` command；每次调用先经认证 owner 校验，再从 active Project Registry 与 Connector workspace grant 重新解析 canonical project root，不接受 React 传入路径、命令、manifest 或工具链；
- 扫描预算与 macOS 保持一致：最多 2,500 个目录、深度 6、每目录 10,000 项、100 个 target、60 个配置文件；忽略 `.git/.chatos/node_modules/target/build/dist/vendor` 等目录并拒绝 symlink/Windows reparse point；已知清单文件独立探测，不受目录枚举预算影响；
- 已映射 Node 固定脚本名 `dev/start/serve/test/build`、Rust、Python、Go、Maven/Gradle 与 Swift target；target 内部保存 typed executable kind + arguments，React 只接收固定 command 展示文本和 opaque target ID，不接收可执行文件或任意参数入口；
- catalog 的 cwd、entrypoint、manifest 与 configuration path 全部是规范化项目相对路径；配置文件读取上限 128 KiB、预览最多 12,000 字符，manifest 解析上限 2 MiB，二进制/超限文件不预览；
- 工具链只从原生进程 `PATH` 发现，DTO 返回 opaque executable ID、文件名标签和 kind，不返回绝对路径。环境变量 DTO 采用 `key + hasValue` 安全投影，初始 analyzer 不读取或返回值；
- 4 项原生测试覆盖 macOS 优先级/多语言识别、超限 manifest、symlink 逃逸和 owner/project/workspace 边界，并断言序列化 DTO 不含项目绝对路径；`clippy -D warnings` 通过。此阶段尚未开放 start/stop/delete、偏好 mutation、state long-poll 或 React UI。

Windows typed process 第二阶段（2026-10-20）：

- 新增 `load/start/stop/delete/poll_project_run_*` typed commands；start 只接收 analyzer 生成的 opaque target ID，每次启动前重新解析 owner → active Project Registry → Connector workspace grant 并重新分析，React 不能提交命令、参数、cwd 或 executable；
- 启动器按 analyzer 内部 `ProjectRunExecutable + arguments` 直接创建进程，不经过通用 Shell；项目 wrapper 逐级拒绝 symlink/reparse 并限制在 canonical project root，工具链从原生进程 `PATH` 解析且不向 WebView 返回路径；
- Windows 使用带 `KILL_ON_JOB_CLOSE` 的 Job Object，Unix 测试使用独立 process group；stop、delete、账号切换、登出、应用退出和 workspace grant/root 变化均终止整棵进程树；stdout/stderr 并发读取，单实例组合日志最多 512 KiB，退出实例最多保留 100 个；
- state 返回 macOS 同构的 running/stopping/exited/failed、busy/running、倒序实例、日志、开始时间与退出码，并增加原生单调 revision 和最长 30 秒 bounded long-poll；账号、项目和 canonical root 三重 scope 防止跨账号/跨项目读取或操作实例；
- 新增 typed target freshness、owner scope、日志/退出状态、long-poll timeout、teardown 唤醒与 Unix 进程组回收测试；项目运行定向 8 项和 `clippy -D warnings` 通过。偏好、Secret 环境变量和 React UI 仍未开放。

Windows preferences / Secret 第三阶段（2026-10-21）：

- 新增 `set_project_run_default_target` 与 `update_project_run_environment` typed commands；默认目标必须来自本次 fresh analyzer，工具链只能提交 analyzer 返回的 opaque option ID，失效或跨 kind 的 ID 会在原生层拒绝；
- owner/project-scoped preferences 使用 4 MiB 有界、版本化、拒绝未知字段的 JSON，并通过私有临时文件 + fsync + rename 原子替换；JSON 只保存目标 ID、opaque 工具链 ID、环境变量键与随机 Secret 引用，不保存环境变量值、绝对 executable 或项目路径；
- 环境变量值写入 Windows Credential Manager / macOS Keychain；更新采用“retired reference write-ahead → 新凭据写入 → preferences 原子切换 → 旧凭据回收”的版本化引用流程，文件提交失败不会切换到半写入 Secret，失败清理引用可在后续 mutation 重试；
- 环境变量读取只返回 `key + hasValue`，新增键必须带值，`value: null` 仅表示保留已有 Secret；键名、单值 64 KiB、总量 512 KiB、数量 128 均有原生上限。启动时才从系统凭据库取值，并与所选工具链目录合成进程环境；
- 运行日志的 DTO 投影会对当前所有环境 Secret 做整段脱敏，因此即使子进程主动打印变量，React 也不能取得明文；偏好文件无明文、masked projection、保留/删除、无效输入与运行日志脱敏进入定向测试。Project Run 定向 11 项和 `clippy -D warnings` 通过；React UI 仍等待 Removal/Favorite Adapter。

Windows Removal / Favorite 第四阶段（2026-10-21）：

- 新增 `remove_local_project`：先校验当前认证 owner，再与项目创建/恢复共用 Registry transaction lock，在 `BEGIN IMMEDIATE` 事务中读取当前 revision 并以 revision + status 条件更新为 `removed`；receipt 明确返回旧/新 revision 与 `directoryPreserved = true`，实现不读取、更不删除本机项目目录；
- Registry 提交后立即终止该 owner/project 的全部 typed process、唤醒 state long-poll，再清理该项目的运行偏好和系统凭据 Secret；偏好清理失败通过 receipt 单独标记，不能把已经提交的 Registry removal 误报成未发生；Connector workspace grant 和历史会话保持 macOS 原语义，不随项目移除删除；
- 新增设备级 `project_favorite` store，使用 2 MiB 有界、版本化、拒绝未知字段、私有临时文件 + fsync + rename 的原子 JSON；存储本身不按账号分区，但每次 load/set 都与当前认证 owner 的 active Registry 项目取交集，保持 macOS “设备级偏好、账号项目投影”的真实语义；
- 原生测试证明 removal 的 owner/revision scope、removed 状态、目录保留与幂等拒绝，favorite 的设备级存储和 owner/active-project 投影，以及项目 teardown 对进程树和 Secret 引用的清理。至此第 03 页 Windows 原生准入门禁完成，可以建立 React 项目设置 UI。

Windows React 项目设置第五阶段（2026-10-21）：

- 新增独立 `features/project-run` Domain、Port、Tauri Adapter、Zustand Store 与 UI；Zod schema 严格验证项目相对路径、opaque target/toolchain ID、状态派生关系、512 KiB 日志上限、Removal receipt 和 Favorite snapshot，React 不接收绝对路径、可执行路径、Secret 或通用命令入口；
- load、bounded long-poll 与 mutation 使用 owner/project/generation/request identity。切项目、离开页面和项目移除会拒绝迟到写回；mutation 互斥并淘汰旧 load，poll 只接受不倒退的 revision；命令失败后重新读取原生真值，同时保留原始操作错误；
- 环境保存冻结提交时的工具链与变量；保存期间的新草稿不会被迟到结果覆盖。已有 Secret 默认发送 `value: null` 保留凭据引用，只有用户明确输入替换值时才把明文交给 Tauri Adapter，页面永不回显 Secret；
- 项目工作面已开放“项目设置”标签，并按 macOS 页面顺序实现页头、宠物快捷访问、提示、运行预检、默认目标/启动、实例/日志、工具链、环境变量、配置预览及项目移除确认；全部使用 Apple 主题卡片、分段标签、switch、sheet 与明暗色 Token；
- 移除成功后由 AppShell 从当前 owner 的 Workspace Index 删除项目并回到工作台；原生 receipt 必须证明 `directoryPreserved = true`，页面明确说明本机项目文件夹不会删除。
- React 全量验收为 46 个测试文件、185 项通过，TypeScript、Vite production build 与架构审计通过；本切片未修改 Rust 原生代码，继续沿用前四阶段的 121 项 Rust 通过、2 项 sidecar 环境测试忽略与零 Clippy 警告门禁。

### 2.2.6 远程连接管理分层

macOS 连接管理切片（2026-10-21）：

- Core 新增 `RemoteConnectionManagementUseCase`，统一创建、更新、草稿测试、已保存连接测试和删除；SwiftUI 与 ViewModel 不再直接编排具体 Connector；
- 编辑器和详情页共用 `RemoteConnectionOperationState` single-flight identity。保存、测试和删除互斥，页面或弹窗退出会取消具体 Task 并失效 token，底层 Connector 即使忽略取消也不能写回已经离开的页面；
- 编辑器在远程等待期间不再强持有 ViewModel；输入表单在涉及凭据的操作期间冻结，避免测试或保存的是旧草稿而界面继续展示新输入；SSH 二次验证仍沿用原有 challenge/retry 语义；
- 删除成功后才通过 App composition 更新列表并释放对应 Terminal/SFTP workspace；删除失败保留当前连接和页面。新增 6 项 Core/App 测试覆盖 use case 路由、single-flight、ViewModel 释放、迟到删除和二次验证；
- 本切片只证明 macOS 连接管理面；其后的 SFTP 与 Terminal 分层见 2.2.7、2.2.8。Windows 后续已补齐 SSH 测试、SFTP、Terminal 原生门禁及对应 React 工作区，当前结论以 2.2.8 的完整远程资源验收为准。

Windows 原生管理与凭据切片（2026-10-21）：

- Tauri 新增账号校验后的 `list/get/create/update/delete` typed commands，继续复用 Host 的远程连接 metadata CRUD；更新和删除在写入前重新读取目标并联合复验 owner、connection ID 与 expected version，不信任 WebView 缓存；
- 本机 Connector route 只从 owner-scoped `state.json` 读取稳定 `deviceID` 与 workspace ID，React 不能提交或读取这些内部路由字段；引用已有跳板机时由原生层重新读取同账号记录并冻结 host/port/username，拒绝自引用和失效引用；
- 登录密码、私钥路径、证书路径及三类手工跳板机凭据使用独立 Windows Credential Manager/macOS Keychain 条目；磁盘索引只保存 owner 与 connection ID，系统账户名使用 SHA-256 派生值。Host metadata、Tauri 返回值和 React projection 均不包含 Secret 或本机路径，只返回六个 `has*` 状态；
- 创建时凭据写入失败会按 Host 返回版本删除刚创建的 metadata 并清理凭据；更新先保存下一份凭据，Host 拒绝或版本冲突时恢复上一份；删除先撤销本机凭据，Host 删除失败时恢复。列表会按 Host 真值回收已删除连接的残留 Credential Manager 引用；
- 14 项新增原生测试覆盖 Secret 隔离、Credential Manager 容量门禁、owner scope、route 冻结、expected-version 拒绝、CRUD 成功路径、创建/更新/删除回滚及残留凭据清理。该阶段 Windows Rust 全量为 135 项通过、2 项 sidecar 环境测试忽略，`clippy -D warnings` 通过。

Windows OpenSSH 连接测试切片（2026-10-21）：

- 新增草稿与已保存连接两条 typed 测试命令；已保存连接在启动进程前重新读取 owner-scoped target 与 jump metadata，并只在原生层合并当前设备的 Credential Manager 内容。草稿测试不写 Host metadata，两条命令都与保存/删除共用 single-flight mutation 边界；
- Windows 只允许由 `GetWindowsDirectoryW` 解析的 `%WINDIR%\\System32\\OpenSSH\\ssh.exe`，不接受 React、PATH 或任意可执行文件。生成的私有临时 config 与 AskPass response 位于 AppData 随机目录，严格/accept-new host-key、password/private-key/certificate、ProxyJump 和 connect timeout 均复用 macOS 当前配置语义；
- App 自身以隐藏 AskPass helper 模式响应密码和 MFA，SSH 子进程只继承显式 allowlist 环境。首次 MFA prompt 不重启连接，而是保留同一 OpenSSH/ProxyJump 进程并等待 verification file；后续验证码提交恢复原会话，新的 challenge 可再次返回，5 分钟未处理自动回收；
- stdout 64 KiB、stderr 1 MiB、prompt 64 KiB 且持续 drain；所有用户错误均按 host-key、认证、拒绝连接、网络、DNS、key-file 等固定类别投影，不返回 raw stderr、Secret 或绝对路径。Windows 使用 kill-on-close Job Object，Unix 验收使用独立 process group；超时、取消、账号切换、登出和应用退出均回收整棵 OpenSSH/AskPass 进程树；
- 14 项新增原生测试覆盖 config/host-key/ProxyJump、AskPass 路由、同会话 MFA 续接、过期验证码拒绝重连、完成后临时目录释放、prompt Secret 脱敏、固定诊断分类、私有临时目录、超时进程树和 lifecycle reset。Windows Rust 全量为 149 项通过、2 项 sidecar 环境测试忽略，`clippy -D warnings` 通过；下一切片是 SFTP typed commands 与路径/传输边界。

### 2.2.7 SFTP 工作区分层

macOS SFTP 切片（2026-10-21）：

- Core 新增 `RemoteFileUseCase`，统一认证→初始目录、目录读取、上传、下载、创建目录、重命名和删除；副作用成功后的目录刷新改为 best-effort typed result，刷新失败不再把已经提交的远端操作误报为失败；
- `RemoteFileLifecycleState` 为远端导航、本机目录读取和 side-effecting mutation 建立独立 token。连续目录导航只接受最新结果；mutation 会淘汰旧目录请求，并在完成前拒绝新的导航或并发 mutation；
- `RemoteSFTPViewModel` 持有具体 Remote/Local/Mutation Tasks，远程等待期间只冻结 use case、连接和操作参数，不强持有 ViewModel。连接编辑、删除、登出或 workspace 清理会 `deactivate` 并拒绝所有迟到结果；
- 上传/下载/创建/重命名/删除期间冻结冲突操作，并分别展示成功、真实操作错误和 `committed-without-refresh` 警告；本机目录继续采用可取消 detached work 与 10,000 项上限；
- 新增 6 项 Core/App 生命周期与回归测试，覆盖 load lane 隔离、mutation invalidation、认证顺序、导航 latest-wins、忽略取消时释放，以及删除已提交但刷新失败。现有 Remote File Service、SFTP MFA 和 workspace 隔离测试继续通过；
- 该切片完成时交互式 Remote Terminal 仍直接依赖原生 session provider；其后已在 2.2.8 完成 session lifecycle/Port。Windows 后续也已完成固定 SSH 可执行边界、连接测试、路径/传输安全投影、Terminal 原生 session 与 React 工作区，当前结论以 2.2.8 为准。

Windows 远端目录与 mutation 原生切片（2026-10-21）：

- 新增初始/指定目录读取、创建目录、重命名和删除四类 typed Tauri commands；每次操作重新读取 owner-scoped target/jump metadata 与设备本机凭据，React 只能提交连接 ID、操作 ID、远端路径和可选验证码；
- 文件操作复用连接测试已经验收的固定 OpenSSH、AskPass、Job Object/process group 和 MFA retained-session runtime。操作 ID 与连接/凭据/完整参数 fingerprint 绑定，验证码只能恢复原操作；参数变化会立即取消旧进程，不能把验证码交给替换后的命令；
- 远端路径最多 4,096 bytes、名称最多 1,024 bytes，拒绝控制字符、NUL、`.`/`..` 名称和根目录 mutation；所有 shell 参数单引号转义。目录输出以 NUL 分隔协议解析，最多 1,000 项/2 MiB，拒绝非法 UTF-8、未知类型和畸形元组，WebView 不接收凭据、本机路径、命令、进程或 raw stderr；
- 创建/重命名/删除与父目录刷新在同一个已认证 SSH 进程中完成；只有收到进程内提交 marker 才返回 `committed: true`，其后目录读取失败、超时或超限会投影为 committed-without-refresh，而非把已经发生的副作用误报为失败；
- 9 项新增原生测试覆盖安全投影、排序/上限、banner 隔离、非法路径/名称、shell quoting、错误脱敏、同会话刷新、committed-without-refresh 和 MFA 操作替换回收。该阶段 Windows Rust 全量为 158 项通过、2 项 sidecar 环境测试忽略，`clippy -D warnings` 通过。

Windows 本机选择与流式传输原生切片（2026-10-21）：

- 上传和下载分别使用 Windows 原生打开/保存面板，Rust 只向 React 返回绑定 owner + presentation + kind 的一次性 opaque selection ID、文件名与可选大小；绝对路径仅保存在进程内，15 分钟过期，账号切换、登出、连接 mutation、应用退出或页面 discard 会清理选择并终止等待中的 SSH/MFA 进程；
- 上传启动前重新 canonicalize 并拒绝 symlink/reparse point、类型或大小变化，通过有界 stdin 流式写入远端同目录随机临时文件；远端复验精确字节数后才以覆盖 rename 或 no-clobber hard-link 提交，再在同一 SSH 会话刷新目录。副作用与 committed-without-refresh 继续保持强类型区分；
- 下载只允许普通非 symlink 远端文件，远端先在 stderr NUL frame 中声明大小，Rust 将 stdout 流式写入目标目录 `create_new` 临时文件并同时执行 8 GiB 硬上限；退出后必须复验声明大小与实际字节数一致，才使用 Windows `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`/Unix rename 覆盖，未确认覆盖则用同卷 hard-link 原子拒绝竞态覆盖；
- 临时下载文件由原生 RAII guard 管理：认证失败、超限、取消、超时、调用方丢弃结果或保存失败都会清理；`create_new` 失败绝不删除已有路径。WebView 不接收本机路径、远端 shell、raw stderr、Secret、进程或文件句柄；
- 7 项新增原生测试覆盖 selection scope/reset、上传临时文件与精确大小、symlink 拒绝、stdin 流式传输、未领取下载清理、`create_new` 失败保留原文件，以及无覆盖确认的原子落盘。该切片完成时 Windows Rust 全量为 165 项通过、2 项 sidecar 环境测试忽略，`clippy -D warnings` 通过；其后的交互式 ConPTY/OpenSSH session 已在 2.2.8 完成。

### 2.2.8 交互式 Remote Terminal 分层

macOS Terminal 切片（2026-10-21）：

- Core 新增 `RemoteTerminalSessionPort`、`RemoteTerminalSessionProviding` 与 `RemoteTerminalUseCase`。UI 只消费有界的 byte-stream `output/terminated` 事件和 start/send/resize/interrupt/close 能力，不再依赖 Connector 的 SSH session、delegate、进程句柄、启动参数或凭据路径；
- `NativeRemoteTerminalPortAdapter` 在 Connector 内把原生 PTY delegate 转为 Core `AsyncStream`，关闭时解绑 delegate、终止 OpenSSH session 并结束事件流；密码、私钥路径、SSH config、AskPass 和临时运行目录不跨出 Connector；
- `RemoteTerminalLifecycleState` 分离 connect token 与 active session token。reconnect/disconnect 同时失效旧连接和事件流；旧 provider 忽略取消并迟到返回 session 时必须立即 close，旧 session 的 output/terminated 事件不能结束或污染新 session；
- `NativeRemoteTerminalViewModel` 持有 connect/event Tasks，但远程 open 等待期间不强持有自身；workspace 删除、连接编辑或登出沿既有 store teardown 调用 `disconnect`，保证 Task、session、PTY 与临时凭据目录一起回收；
- 新增 6 项 Core/App/Connector 测试，覆盖 connect/session generation、verification code 路由、MFA 重连、旧 session 事件隔离、provider 忽略取消时 ViewModel 释放，以及原生 session 到 Core Port 的适配。现有 TTY、AskPass、MFA、ControlPath 与 runtime cleanup 测试继续通过；
- 至此 macOS 远程连接的管理、SFTP 与 Terminal 产品行为均完成源码分层。

Windows ConPTY/OpenSSH 原生切片（2026-10-21）：

- Tauri 新增 start/send/resize/interrupt/poll/close/discard 七类 typed commands；每次 start 都重新读取 owner-scoped 连接 metadata、跳板机与 Credential Manager 内容，React 不提交 SSH executable、config、Secret、本机路径或进程句柄；
- Windows 使用 `portable-pty`/ConPTY 启动固定 OpenSSH，仍复用受限环境、私有临时 config、AskPass、host-key、jump host 和 MFA 语义。进程由 kill-on-close Job Object 管理；Unix 验收使用独立 process group；关闭、账号切换、登出、应用退出或 presentation discard 会回收整棵进程树；
- session identity 同时绑定 owner、presentation、connection、epoch 与 generation；每个 owner 最多 4 个 active/in-flight session。旧 connect 迟到完成会立即关闭，旧 session output/terminated 事件不能污染新连接；
- 输入每次最多 64 KiB，输出 ring 最多 512 个事件/2 MiB，单次 poll 最多 128 个事件/256 KiB，long-poll 最长 30 秒；游标截断显式返回，resize 限定 1–1,000 行列；
- 9 项原生测试覆盖 MFA 路由、真实 PTY 有序输出与退出、连接 generation、in-flight session 上限、reset 回收、输入/identity 校验、输出事件/字节预算和尺寸边界。Windows Rust 全量为 179 项通过、2 项 sidecar 环境测试忽略，`clippy -D warnings` 通过。

Windows React 远程资源工作区（2026-10-21）：

- 连接管理、编辑、草稿/已保存连接测试、MFA 与删除已通过独立 Domain、Application Store、Port 和 Tauri Adapter 接入；删除成功后才释放当前连接的 SFTP/Terminal 展示，失败保留页面与连接；
- SFTP 已提供远端/本机双栏浏览、原生文件选择、上传、下载、新建目录、重命名与删除，并保持远端导航、本机导航、mutation 三条 identity 隔离和 committed-without-refresh 语义；
- Terminal 使用 xterm.js 标准终端仿真，并以懒加载独立 chunk 接入；UI 支持输入、resize、中断、断开、重连、MFA、搜索、清屏、OSC 7 工作目录与 OSC title。Store 串行输入并联合校验 owner/presentation/session/generation，保留最多 2 MiB 输出，游标截断会显示明确提示；
- React 不读取凭据、本机绝对路径、SSH 参数或进程句柄；`invoke` 仅存在于 Adapter，Adapter 只在 `App.tsx` composition root 构造。远程资源页面保持 macOS tab 顺序与 Apple 主题；
- 当前全量验收为 React 65 个测试文件、235 项通过，TypeScript、Vite production build、架构审计与生产依赖审计通过；Rust 179 项通过、2 项 sidecar 环境测试忽略，格式检查及 `clippy -D warnings` 通过。远程连接管理、SFTP 与 Terminal 已全部进入 React。

### 2.2.9 插件应用列表与启动分层

macOS 插件应用切片（2026-10-21）：

- 列表继续通过独立 `LocalConnectorPluginApplicationListing`、列表 Use Case 与 `PluginApplicationsLoadState` 加载；账号、request identity、刷新和离页 lifecycle 不与具体启动过程混用；
- Core 新增 `LocalConnectorPluginApplicationLaunching` 与 `PluginApplicationLaunchUseCase`，统一校验 owner、应用声明的 context scope、device fallback 和项目选择；SwiftUI/AppModel 不再直接编排“解析项目上下文 → 启动 Connector 应用”；
- 项目选择只把 project ID 交给 Use Case，`PluginApplicationContextProviding` 在 Core/Connector 边界解析完整上下文。包含项目根路径的 context 只传给原生 launcher；返回 UI 和受限 WebView bridge 的 `PluginApplicationBridgeContext` 仅包含 project ID/name；
- `PluginApplicationLaunchState` 将 owner、account generation、application、context selection 与 request identity 冻结为 token。新启动、切换 context、离页和账号导致的页面重建都会淘汰旧 token；Connector 即使忽略 cancellation 并迟到返回，也不能覆盖当前页面；
- `PluginApplicationHostViewModel` 独立持有页面 phase、选择、launch/retry/deactivate Task。原生启动 await 期间不强持有 ViewModel，页面释放不再被悬挂的 Connector 请求阻止；
- `RestrictedPluginWebView` 保留既有 origin/navigation/bridge 限制，只消费 launch 结果和脱敏 bridge context，不接收项目根路径、Connector service 或原生进程状态；
- Core 6 项、App 3 项及既有 Native Plugin Runtime 定向测试通过；完整 `swift test --package-path clients/macos --no-parallel` 退出码为 0，其中 ChatOSApp XCTest 292 项通过、1 项按环境跳过。macOS 应用页分层已完成；
- Windows 已建立第一段原生 catalog：Tauri 只按当前认证 owner 分页读取 Host 安装记录，逐项复验 summary/record identity 与 revision，并只选择 enabled 安装；executable、manifest 与 UI source 必须 canonicalize 到 `<appData>/NativeConnector/Plugins` 下同一安装根，manifest 限制为 2 MiB、schema v3、Windows platform 与 workbench surface；
- `local_http` 声明只接受安全 basename、有界参数/health path/timeout，并要求组件级必需 `process.spawn`。React DTO 只投影应用身份、展示、context、bridge capabilities 与 release identity，不包含 executable、安装目录、source 路径或 Secret；
- Windows local HTTP runtime 已按 macOS `NativePluginApplicationRuntime` 接入：每次 launch 重新读取 Host 安装真值与 manifest，`package.json` 限制 1 MiB，bin 必须是安装根内无 symlink/reparse 的声明文件；Node 入口由原生层解析本机 Node，React 不提交 executable 或 arguments；
- runtime context 保持 macOS 的 user/plugin/project/workspace/device scope 和 SHA-256 存储隔离；项目绝对根只在原生子进程环境中按 manifest required/optional 字段提供，返回 React 的 context 只有 project ID/name。进程环境使用 allowlist 且不解析安装记录中的 Secret references；
- 每个账号最多 8 个 runtime，服务只监听原生分配的 `127.0.0.1` 随机端口，健康检查禁止 redirect/proxy 并受 700 ms 单次请求与 100–120,000 ms 总超时约束；stdout/stderr 固定缓冲持续 drain。Windows Job Object 与 Unix process group 在启动失败、账号恢复/登录/注册/登出、Host 停止/失联/替换、应用退出和 reset 时回收进程树；
- launch identity 绑定 owner、presentation、generation、plugin/component 与 context scope；discard tombstone 和 runtime epoch 会拒绝在账号/Host/页面 teardown 后才完成的旧请求。健康 runtime 仅在 release ID/digest/version 全部一致时复用；
- Windows 当前真实隔离能力为受限环境加 Job Object 生命周期，不宣称具备 macOS Seatbelt 的文件/网络沙箱；该差异沿用第 31 页 `limited` 访问控制事实，环境显式标记 `windows-job-lifecycle`；
- 21 项 Plugin 原生测试覆盖 catalog、context/generation、路径/permission、存储隔离、Secret 环境、真实 loopback 健康子进程、复用、失败清理和 reset 回收；Windows Rust 全量 205 项通过、2 项真实 sidecar 环境测试忽略；
- Windows 原生 bridge 只准入 macOS 当前真实实现的 `host.context.read`；manifest 即使声明 `artifact.*` 也不会提前成为可调用能力。请求上限 256 KiB，并同时绑定 owner、presentation、generation、session、64 位随机 nonce、精确 `http://127.0.0.1:<port>` origin 与当前存活 runtime；discard/reset 后旧 bridge 立即失效，返回 context 仍只有 project ID/name；
- Plugin 定向 22 项及 Windows Rust 全量 206 项通过、2 项真实 sidecar 环境测试忽略，Clippy 通过。第 05 页原生准入完成，可以建立 React Domain/Port/Store/UI；React iframe 必须继续验证 `event.source` 和 exact origin、使用 sandbox，并仅把原生返回的 admitted capabilities 写入 ready 消息。
- Windows React 已建立独立 `plugin-applications` Domain、Application Store、Port、Tauri Adapter 与 UI；原生 DTO 由 strict schema 再次复验，未知安装路径/executable 字段、非 `127.0.0.1` HTTP 地址、release identity 错配和未声明 capability 均在 Adapter 边界失败关闭。`invoke` 仍只存在于 Adapter，且只在 `App.tsx` composition root 构造；
- 列表保持 macOS 的 loading/error/empty/retry、Apple 风格应用卡片和本地服务标记。侧栏增加“应用”入口；Host 页面保持返回、重新载入、切换上下文、启动失败重试和公共项目语义，项目选择只使用 Workspace Index 的 ID/name，不把根目录带进 React；
- Store 将 owner、Host availability、account generation、presentation、application、context、launch generation 与 request identity 联合验收。切账号、Host 失联/恢复、切项目、重试、返回列表和页面 teardown 都先淘汰旧结果并向原生层写入 generation tombstone，迟到 launch 不得恢复旧 iframe；
- `RestrictedPluginFrame` 使用不含 popup/顶层导航权限的 sandbox，只允许精确 loopback frame。消息必须同时匹配 `event.source === iframe.contentWindow`、exact origin、protocol、session、64 位 nonce、唯一有界 request ID 与 launch admitted capability；ready 消息只发布原生返回的 capability，Bridge 仍通过 typed Port 调用原生命令，页面重载/卸载后不再回传旧请求；
- CSP 仅新增 `frame-src http://127.0.0.1:*`，没有放宽 `connect-src`、远端脚本或 Tauri IPC。React 全量验收为 74 个测试文件、258 项通过，TypeScript、Vite production build 与架构审计通过；至此第 05 页插件应用已完整进入 Windows React。

### 2.2.10 本机交互式 Terminal 分层

macOS 本机 Terminal 切片（2026-10-21）：

- 既有产品语义保持多标签、关闭最后一个标签后立即补建空标签、搜索上下匹配、清屏、Ctrl-C 中断、OSC title/当前目录、10,000 行 scrollback，以及登出/账号替换时关闭全部会话；
- Core 新增 `LocalTerminalSessionPort`、`LocalTerminalSessionProviding` 与 `LocalTerminalUseCase`，只暴露 byte-stream output/termination/failure、Shell 展示名以及 start/send/resize/interrupt/close；
- `NativeLocalTerminalSession` 移入 Connector，独占登录 Shell 解析、`TERM/COLORTERM` 环境、SwiftTerm `LocalProcess`、PTY viewport、进程组 SIGHUP 和进程句柄。App/SwiftUI 不再读取 `ProcessInfo`、调用 `Darwin.kill` 或直接持有 `LocalProcessTerminalView`；
- `NativeLocalTerminalViewModel` 只持有 Core Port、renderer 与 `LocalTerminalLifecycleState`。标签关闭/reset 会先失效 session token，再取消事件 Task 并关闭 Port；旧进程的 output/terminated 不能写回已关闭或替换的标签；
- 本机与远程终端共用无进程能力的 `NativeByteStreamTerminalView` renderer，输入与 resize 只转发给 typed Port，OSC title/目录、搜索、链接白名单和剪贴板仍由展示层负责；
- Connector 事件缓冲最多 256 个 chunk。消费者追不上时不静默丢失控制序列，而是发布显式 failure、终止进程组并结束 stream，防止高速输出重新形成无界内存；
- 定向验收为 Core 2 项、Connector 本机 PTY 3 项、App 4 项通过；既有 Native Terminal Executor 6 项同时通过。完整 macOS 回归仍作为提交门禁；
- Windows 第 19 页已建立独立 owner + presentation + generation 绑定的 ConPTY typed session；现有 task terminal worker 与远程终端仅提供进程回收技术参考，没有复用其 Host durable tool invocation 或 SSH 权限；
- React 只提交 owner、opaque presentation/session identity、输入字节、viewport 和 poll cursor，不提交 executable、arguments、environment 或 cwd。Windows 原生层从系统目录解析 Windows PowerShell，选择当前用户目录并构造不含应用 Secret 的 allowlist 环境；
- 原生层限制每 owner 最多 4 个会话、64 KiB 单次输入、1–1000 viewport、512 event/2 MiB retained output、128 event/256 KiB 单次 poll 与 30 秒 long-poll；Windows Job Object 在 close/reset/drop 时回收进程树；
- React `LocalTerminalPort`、Tauri Adapter、Zustand workspace Store 与 Apple 主题 UI 保持 macOS 的多标签、关闭最后标签立即补建、10,000 行 scrollback、搜索、清屏、中断、OSC title/cwd；每标签冻结 owner/workspace generation/presentation/session/tab generation，关闭、离页、账号或 Host 变化后的迟到 start/poll 不得写回；
- Authentication restore/login/register/logout、Host stop/失联/替换、页面 discard 与应用退出均接入原生 reset。Rust 5 项本机终端定向测试、React 12 项切片测试及全量门禁通过，第 19 页已进入 Windows React。

### 2.3 Authentication：完整调用链

入口和分层：

```text
AuthenticationGateView / LoginView
  → AuthenticationViewModel
  → AuthenticationServicing
  → ChatOSAuthenticationService
  → ChatOSAPIClient + CredentialStoring
  → /api/user/auth/* + platform credential storage
```

冻结的服务契约：

- 恢复：读取 access token 后请求 `GET /api/user/auth/me`；仅 `401` 清除凭据并返回未登录，瞬时网络错误保留凭据并允许应用再次 active 时重试；
- 登录：`POST /api/user/auth/login`，请求为 `username/password`，响应同时兼容 `token` 与 `access_token`；
- 注册验证码：`POST /api/user/auth/register/send-code`，请求使用 `email/invite_code`，倒计时必须读取服务端 `resend_after_seconds`；
- 注册：`POST /api/user/auth/register`，请求使用 `email/password/invite_code/verification_code`；
- 用户模型固定为 `id/username/display_name/role`；客户端展示 DTO 使用 camelCase，但 Adapter 负责转换；
- 所有请求使用 `X-Chatos-Client-Surface: local-connector-desktop`；只有 user service 的 `401` 有权使全局登录失效；`x-access-token` 可刷新现有凭据；
- Windows 认证连接超时固定 10 秒、整次请求最长 30 秒，失败必须回到带错误的可重试状态，禁止无限 loading；
- 密码只允许 12–128 字符，验证码严格为 6 位数字；注册错误继续使用 macOS 的固定中文映射；
- logout 立即将业务 UI 置为 unavailable，但必须等待已发出的认证操作结束，再停止 Host、清除凭据；新 login 必须等待旧 logout 完成，迟到 login 不能恢复已退出的 UI；
- 登录成功后才用 `session.user.id` 启动 Host；账号退出时先停止 Host，再删除 access token。

Windows 平台差异：

- Token 由 Tauri/Rust 持有并写入 Windows Credential Manager；React 不接触 Token，禁止 `localStorage`、IndexedDB、URL、诊断或日志保存；
- API 基址与 macOS profile 语义一致：debug 默认 `http://127.0.0.1:9080`，release 默认 `https://gateway.jgoool.com`，测试/部署可通过 `CHATOS_API_BASE_URL` 或 `CHATOS_DEPLOYMENT_PROFILE` 覆盖；
- React 只持有不含 Token 的 `AuthSession`，并通过 `AuthenticationPort` 调用；Tauri command 是唯一网络/凭据 Adapter；
- 系统 Credential Manager 界面属于操作系统安全界面，不受应用自绘苹果主题约束；其余登录与注册 UI 必须沿用 macOS 双栏结构与苹果主题。

## 3. 项目会话准备：完整调用链

### 3.1 页面入口与状态展示

- 项目工作区入口：`clients/macos/Sources/ChatOSApp/Features/Project/ProjectWorkspaceView.swift`
- 消息页面：`clients/macos/Sources/ChatOSApp/Features/Chat/ProjectMessagesView.swift`
- 页面在 `.task(id: projectID)` 中先 `Task.yield()`，再调用 `prepareProjectChat(projectID:)`，避免同一 SwiftUI 更新周期内短暂错误/进度分支闪现。
- UI 必须保留四个互斥状态：
  - 已有 Conversation：展示 `ConversationTimelineView`；
  - 正在准备：明确的连接进度；
  - 准备失败：展示错误并允许重试；
  - 尚无可用会话：等待默认项目联系人。

### 3.2 状态所有权与并发规则

源码：`clients/macos/Sources/ChatOSApp/AppModelResources.swift`

调用链：

```text
ProjectMessagesView.task
  → AppModel.prepareProjectChat
  → prepareProjectConversationIfNeeded
  → ensureProjectConversation
  → ProjectConversationPreparing.ensureConversation
  → applyPreparedConversation
  → conversation(for:).activate
```

必须原样保留的规则：

- 仅登录账号与 Local Agent workspace owner 一致时才准备会话；
- 项目与默认联系人必须存在；
- 同一项目已有准备任务时复用，不重复创建；
- per-project preparation token 与 `workspaceAccountGeneration` 在应用 Conversation 前共同验收，阻止过期异步结果覆盖当前账号/项目状态；
- 强制 reconcile 可校正重装、重新配对或 workspace 移动后的陈旧执行目标；
- 失败保存在项目级错误状态，用户显式重试才清除并重新准备；
- Conversation ViewModel 按 session ID 缓存，激活新会话时停用旧会话。

### 3.3 Conversation 绑定与幂等

Port：`clients/macos/Sources/ChatOSCore/ProjectConversationModels.swift`

Adapter：`NativeLocalAgentProjectConversationService`，位于 `clients/macos/Sources/ChatOSConnector/NativeLocalAgentWorkspaceService.swift`。

- `project.projectContext.projectId` 必须等于项目 ID，否则拒绝创建；
- resource binding 固定为 `.project(project.id)`；
- 创建前先按 binding 查询现有 Conversation；
- 创建发生并发竞争并失败后再次查询，查到即返回，保证幂等；
- Windows 不得设计另一套 project → conversation 关系。

### 3.4 目标 Application/Port

macOS 分层从现有行为提取，不增加业务含义：

- `PrepareProjectConversationUseCase`
- `ProjectConversationPreparationState`（已完成）
- `ProjectConversationPort`（已完成）
- `ConversationSessionFactory`（已完成）

React 以后只能消费上述稳定用例输出；Tauri adapter 只负责传输与 Windows 平台能力。

实施进度（2026-10-19）：

- 已新增 `ProjectContextLoading` Port，由现有 `NativeLocalProjectsService` adapter 实现；
- 已将项目 Conversation 创建/复用边界固定为 `ProjectConversationPort`，现有 `NativeLocalAgentProjectConversationService` 是 Host adapter，Application use case 不依赖 Connector 类型；
- 已新增 `PrepareProjectConversationUseCase`，按 owner + project 复用并发准备任务；
- 已新增 `ProjectConversationPreparationState`，为每个项目分配 latest-wins token；token 内含 owner 与 account generation，完成写回不再由 UI 手工拼接账号验收。AppModel 显式持有对应 Task，项目删除只取消目标项目，账号切换/登出/退出统一取消全部任务；
- `ProjectConversationPreparationState` 同时统一 `idle / preparing / failed` 展示阶段；失败只在显式重试时清除，项目删除或账号 reset 则同时清理 token 和展示状态；`ProjectMessagesView` 只消费该强类型阶段，不再从独立 Set/Dictionary 推导界面；
- 项目 Conversation 准备 Task 会在进入 Application use case 前冻结 owner、project、contact、account generation 与 port；Context/Host await 期间不再强持有 AppModel，返回后仍联合验收账号、项目存在性和 per-project token；
- 已新增 App 层 `ConversationSessionFactory`，集中把 Core conversation ports 组合为 SwiftUI session；AppModel 只保留按 session ID 缓存、LRU 淘汰与 activate/deactivate 策略，runtime ready 在每次创建时读取当前账号状态而非冻结在 factory 中；
- Use Case 仍严格执行“先加载当前 project context，再调用 `ProjectConversationPreparing`”的 macOS 顺序；
- AppModel 继续负责登录账号、account generation、项目是否仍存在和四种 UI 状态，迟到结果仍不能写回；
- 登出/账号切换与项目删除会取消对应 Application 任务；
- 已覆盖同账号同项目复用、不同账号绝不复用、不同项目独立 lifecycle、项目级取消/账号级 reset，以及 context 注入后才创建 Conversation 的测试。
- Connector 已补齐定向测试：复用既有 project binding、在 IPC 前拒绝错配 project context、创建时冻结认证 owner 与 project resource binding，以及并发创建失败后二次查询并接受胜出 Conversation。

Windows 平台差异：

- 项目真值仍来自账号隔离的本地 `Projects.sqlite3`；Tauri adapter 负责读取并验证 project context，React 不直接读 SQLite；
- Conversation 创建/复用仍通过 Rust Host 的 project resource binding，包括并发失败后二次查询；与 macOS 业务语义无差异；
- Windows 的目录授权与设备/workspace 绑定由 Tauri 平台层提供，不得把绝对路径或平台凭据暴露给 React；
- 本次 React 准入仅覆盖“选中项目→复用/准备 Conversation→显式四态”。Conversation 时间线、发送、项目创建/编辑/删除仍按各自门禁禁止。

Windows 实现（2026-10-20）：

- React 已建立独立 project-conversation Domain/Application/Port/Adapter/UI，以 per-project request identity 和 owner reset 拒绝迟到结果；失败保留到用户显式重试，不会因组件重渲染自动重放；
- Tauri 每次准备都重新从账号隔离 Registry 读取 active 项目上下文，验证 workspace/relative root/revision；React 只传 owner 和 project ID；
- Adapter 先分页查询 project binding，未找到才创建；创建失败后再查一次 Host 真值，接受并发胜出会话；
- 项目点击已驱动 `idle / preparing / failed / available` 苹果主题展示；`available` 仅说明 binding 就绪，不伪造未准入的 Conversation 时间线。

## 4. Conversation：完整调用链

### 4.1 UI 与 Feature State

UI：

- `clients/macos/Sources/ChatOSApp/Features/Chat/ConversationTimelineView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Chat/ConversationTimelineItem.swift`

状态与行为：

- `ConversationSessionViewModel.swift`
- `ConversationSessionViewModel+Sending.swift`
- `ConversationSessionViewModel+Attachments.swift`
- `ConversationSessionViewModel+AskUser.swift`

独立状态不可合并成一个 `loading`：

- `isRefreshing`
- `isLoadingOlder`
- `isSending`
- `isUpdatingRuntimeSettings`
- `historyError`
- `runtimeSettingsError`
- `sendError`

历史默认每页 10 条；realtime 刷新 debounce 为 250ms。Windows 必须保留二者，除非先修改 macOS 产品行为与测试。

当前分层进度：

- `clients/macos/Sources/ChatOSCore/Application/ConversationHistoryUseCase.swift` 已统一 latest/older 查询、HistoryStore 合并和 snapshot 读取；
- 固定 10 条分页与 2/5/10/20 秒有界重试策略已移入 Application；
- ViewModel 只应用合并后的 snapshot，并继续分别维护 visible refresh 与 older loading；
- `ConversationViewportUseCase` 已统一底部锚点、未读清零和变更后的 snapshot 返回；底部状态是否发布的判定也已从 SwiftUI 移入 Core；
- `ConversationLatestRefreshState` 已固定 250ms debounce、silent/visible 优先级、in-flight 合并和单次 pending follow-up；
- `ConversationHistoryRetryState` 已固定 2/5/10/20 秒有界重试预算并绑定当前 activation identity；停用或重新激活后旧 sleep 不得发起新刷新；
- `ConversationViewportMutationState` 已把 timeline anchor 与 mark-read 建模为两条独立 latest-wins lane；ViewModel 持有对应 Task，Conversation 停用会统一取消并失效 token，迟到的本地 snapshot 不得写回不可见页面，而 History Store 真值仍可在下次 snapshot 中恢复。

### 4.2 发送状态机

源码事实：`ConversationSessionViewModel+Sending.swift`

```text
sendDraft
  ├─ 空文本且无附件 / 正在发送 → 忽略
  ├─ command service 不可用 → sendError
  ├─ 存在 streaming Turn
  │    ├─ 没有显式 capability → sendGuidance
  │    └─ 有显式 capability → 阻止并提示等待或停止
  └─ 不存在 streaming Turn → sendNewTurn
```

- Guidance 发送前后都以现有 Turn 为目标；若 Host 判断目标已结束，捕获 `guidanceTargetInactive` 并自动降级为新 Turn。
- 新 Turn 先插入 optimistic Turn，并使时间线观察状态失效；发送失败时删除 optimistic Turn，恢复草稿、附件和显式 capability。
- `beginSending` 会清空当前草稿、附件、附件错误、能力选择和发送错误；失败恢复必须避免重复附件。
- 停止运行不是前端改状态，而是调用 Host cancel。

当前分层进度：

- `ConversationSendPlanner` 已集中空请求忽略、active Turn guidance、新 Turn 和显式 capability 冲突判定；
- 文本 trim、附件与 capability 的提交快照由 Application plan 固定；
- `ConversationSendUseCase` 已集中 optimistic Turn 构造与 History Store 写入、typed guidance/new-turn command、revision 0 optimistic 回滚和 snapshot 返回；
- 失败草稿恢复规则已移入 Application：仅在当前草稿为空时恢复原文、附件按 ID 去重且失败附件置前、当前 capability 为空时恢复显式选择；
- `ConversationSendLifecycleState` 已集中发送 request identity；具体 send/recovery Task 由 ViewModel 持有，`guidanceTargetInactive` 在同一 token 内自动降级为新 Turn；
- `ConversationAttachmentImportState` 允许同一 composer generation 内多个文件/图片并发导入；发送开始会失效并取消全部旧导入，迟到的文件读取、图片归一化结果或错误不能写入发送后形成的新草稿；
- runtime unavailable 会立即失效并取消旧发送、恢复 composer，再通过本地 History Store 回滚 optimistic Turn；旧 Host 成功/失败响应不得覆盖恢复后的草稿、时间线或 `isSending`。
- Conversation feature state 已成为 Turn Process、Task Workspace 与内嵌 Task Inspector 的子功能 composition owner；时间线和桌面宠物 SwiftUI View 只接收已构造的子 ViewModel，不再读取或转发 `TurnProcessServicing`、`MessageTaskGraphServicing`、`ConversationRealtimeStreaming` ports。

Windows React 对齐状态：

- Composer 已按同一语法识别草稿末尾 `/query`；路径、URL、已结束的 `/选择 后续正文` 和超过 80 字符的查询不会打开能力菜单；
- 已实现过滤、首项高亮、上下键循环、Escape 暂时关闭、Enter/Tab 选择、选中 chip 移除，以及发送失败时恢复显式能力；owner 或 Conversation 切换会淘汰旧选择和旧发送响应；
- Requirement Survey 作为固定首项并通过既有 `explicit_capability_selection` Host 协议发送；活动 Turn 期间继续执行与 macOS 相同的冲突阻止规则；
- Windows platform plugin worker 尚未实现，因此当前只展示可真实执行的 Requirement Survey。已安装插件不能仅凭目录记录进入候选列表；待执行 worker、capability snapshot 与 plugin release binding 全部打通后再开放。

### 4.3 Host ready、刷新与 realtime

源码事实：`ConversationSessionViewModel.swift`

- Host 未 ready 时不启动业务刷新；
- Conversation bootstrap Task 由 ViewModel 显式持有，并冻结创建时的 runtime-ready 决策；初始化为 unavailable 后即使 Task 延迟执行，也不得与后续 ready recovery 重复加载 settings/prompts。runtime 转换会失效 bootstrap 的最终 Host 恢复动作，但本地 History Store hydration 仍可独立完成；
- ready 后重新加载 runtime settings、Ask User、历史并启动 realtime；
- unavailable 时取消 realtime、retry、debounce 与发送等任务，回滚 optimistic 发送并复位 loading 状态；
- 每次 Conversation 可见激活都具有独立 activation session；停用后旧 realtime 事件、错误与结束回调不能写回或影响下一次激活；
- activation 触发的 Ask User refresh 外层 Task 由 ViewModel 持有并绑定当前 activation；停用会取消 refresh Task 并只失效 refresh token，不会误取消已经提交的 per-prompt action，迟到 prompt snapshot 不得写回不可见 Conversation；
- latest refresh 的 debounce 与具体 in-flight Task 均由 ViewModel 持有，UI 写回绑定 activation；in-flight closure 不跨远程等待强持有 ViewModel，停用后迟到快照仍可完成 History Store 合并，但不得修改当前 UI、错误、retry 或 loading；新 activation 的合并刷新仍由现有 pending 规则接续；
- older-page 请求由 ViewModel 显式持有并绑定 activation、runtime ready 与 cursor；停用或 unavailable 后即使请求迟到返回，也不得写回时间线、错误或清理新请求的 loading；
- ready 恢复由 `ConversationRuntimeRecoveryState` 分配 latest-wins identity，具体 Task 由 ViewModel 持有；连续 ready 会淘汰旧恢复，异步加载期间若 runtime 再次 unavailable 也会统一取消并失效 identity，迟到恢复不得重新触发历史刷新或 realtime；
- 时间线的 Task Graph 可用性轻量探测由 `ConversationTaskGraphAvailabilityState` 为每个 Turn 分配独立 token；新 revision、Conversation 停用或 runtime unavailable 会取消并失效未完成探测，迟到 Host 响应不得改变入口可见性，ready 后会重试被中断且尚未形成可信缓存的探测；
- 模型、推理等级与远程连接的 optimistic 更新共享 latest-wins mutation identity；新操作会淘汰旧操作，旧成功/失败响应不得覆盖或回滚新选择，也不得结束新操作的 loading；runtime unavailable 会统一取消并失效 mutation；
- runtime settings snapshot load 使用独立 latest-wins identity；新的 bootstrap/recovery load、任一 optimistic mutation 或 runtime unavailable 都会失效旧 load，迟到 settings/models snapshot 不得覆盖新选择或断线后的状态；
- runtime settings 与模型目录的并发加载，以及模型、推理等级、远程连接 mutation 已统一通过 `ConversationRuntimeSettingsUseCase` 进入 port；ViewModel 不再直接编排 `ConversationRuntimeSettingsServicing` 调用；
- Task Graph 可用性轻量探测已复用 `MessageTaskGraphUseCase`，Conversation ViewModel 不再直接调用 Graph Host port；
- realtime 事件经 history store 合并，不允许组件自行拼接出另一套事实；
- 历史分页游标是 ordinal，非法游标必须报错；limit 被限制在 1–100；
- 本地 realtime 来自 Host event hub/reconcile，不是服务端 WebSocket。

`ConversationRealtimePolicy` 已集中 Ask User 优先级、终态/history 刷新、失败信息与忽略事件的判定；`ConversationSessionActivationState` 已集中可见 activation identity；ViewModel 只执行对应动作、校验 runtime ready 并管理 stream Task。

`ConversationRealtimeUseCase` 已统一 Conversation、内嵌 Inspector 与 Task Workspace 的 typed stream 订阅入口，并复用上述事件动作映射；三个 Feature State 均不再直接调用 `ConversationRealtimeStreaming` port。

### 4.4 Adapter 与 Host 契约

Port：

- `clients/macos/Sources/ChatOSCore/ConversationCommandService.swift`
- `clients/macos/Sources/ChatOSCore/ConversationRemoteService.swift`
- `clients/macos/Sources/ChatOSCore/ConversationRealtimeService.swift`
- `clients/macos/Sources/ChatOSCore/ConversationHistoryStoring.swift`

Adapter：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationService.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationClient.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationCommands.swift`

不可丢失的契约：

- `sendNewTurn` 先解析 Conversation runtime selection；
- 插件安装助手 Conversation 使用专用 capability snapshot，普通会话使用标准 snapshot；
- 附件先由 attachment vault 授权，IPC 只传 `authorized_local_ref`；
- `startTurn` 携带 expected Conversation version、model config revision 和 capability policy revision；
- Host 接受后唤醒 platform tool worker；
- `sendGuidance` 先确认目标 Turn 仍为 `running`；
- history 使用 `before_ordinal`，返回 `next_before_ordinal`；
- Host 返回 `not_found` 时，当前 adapter 创建空 Conversation 后重试历史查询。

### 4.5 测试证据

- `clients/macos/Tests/ChatOSAppTests/ConversationSessionViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationGuidanceTimelineTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationTimelineBottomSyncPolicyTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentConversationCommandTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentConversationRuntimeSettingsServiceTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentAskUserPromptServiceTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationHistoryStoreTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationHistoryUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationInteractionPolicyTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationLatestRefreshStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationRealtimeUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationSessionActivationStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationRuntimeSettingsLoadStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationRuntimeSettingsMutationStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationRuntimeSettingsUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationAttachmentImportStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationSendLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationSendUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationViewportUseCaseTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationTimelineBottomSyncPolicyTests.swift`

共享 Host v47 fixture 已固定 Conversation history、Message Task Graph、最新 Run events 与 Host scheduler 资源快照，并由 Rust interface、Swift Connector、TypeScript Zod contract 同时读取验证；Windows 后续实现必须继续复用这些 fixture，并覆盖上述状态分支，不能只做组件快照测试。

准入结论：Conversation 核心时间线与发送/恢复状态机已达到 `macOS 分层完成，可进入 React`；React 实现仍必须按 History→Composer→Realtime 的垂直顺序逐片落地。

Windows History 实现（2026-10-20）：

- React 已建立 `conversation-history` Domain/Application/Port/Adapter/UI，页面不直接调用 Tauri；
- Tauri 固定以 macOS 相同的 10 条 page size 调用 Host `get_conversation_history`，并在返回前校验 owner 与 Conversation identity；
- latest 与 older loading/error 独立，older 使用 `next_before_ordinal`，消息、Turn 与附件按稳定 ID 合并，消息按 ordinal 展示；
- owner/Conversation 切换会使旧 request identity 失效，失败保持到显式重试，React StrictMode 重渲染不会重复请求；
- 当前只读 History 已接入项目会话 available 页面；Composer、Realtime 与 Run Inspector 仍按后续子切片实施，未提前伪造。

Windows Composer 发送生命周期（2026-10-20）：

- React 已建立独立 Domain/Application/Port/Adapter/UI，UI 不直接调用 Tauri；
- 发送规划复用 macOS `ConversationSendPlanner`：运行中 Turn 接收普通 guidance，显式能力选择在活动 Turn 上被阻止，否则创建新 Turn；
- Tauri 在 guidance 前后校验真实 Turn 状态；若竞态中 Turn 已结束，保持同一发送生命周期并回退为新 Turn；
- 新 Turn 使用 Host 中已持久化的 runtime settings、精确 model revision 和最新 `main_chat` capability revision，不在 React 发明运行配置；
- 失败恢复已提交草稿但不覆盖用户等待期间的新输入；账号/Conversation 切换使迟到响应失效；
- 显式能力选择严格复用 macOS 规则：只识别草稿末尾 `/query`，路径、URL、关闭后的选择和超过 80 字符的查询不触发；Requirement Survey 固定首项，键盘循环、选择、移除和 active Turn 冲突门禁均由独立 Domain/Application 状态控制；Windows 未实现的 Plugin 候选不会伪装为可用。
- 附件通过独立 `conversation-attachments` Domain/Application/Port/Adapter 接入；支持原生文件选择、剪贴板图片/长文本、Tauri 原生拖放、附件-only 消息、guidance 附件、草稿 chips、图片/文本/普通文件预览和历史消息附件回看。
- React 永远不接触绝对路径、完整文件内容或 Secret；Rust vault 按 owner + Conversation 哈希隔离，只把 opaque `local-attachment:<uuid>` 交给 Host。每条消息最多 20 个附件、总计最多 20 MB，发送前重新校验文件大小、修改状态和 SHA-256；图片预览通过受控 `chatos-attachment` 协议，文本预览最多 16 KB。
- 附件导入绑定 draft generation；发送、账号/Conversation 切换和页面卸载都会拒绝并清理迟到导入。发送失败按 ID 去重恢复附件，发送成功后附件从草稿所有权提交到历史所有权，后续 history refresh 失败不会把已发送附件重新塞回草稿。
- Windows `local_attachment_read` worker 只领取同名 Platform invocation；先从 Run 取得 source Conversation，再以 `authorized_local_ref` 向 Host 精确查询授权记录，最后读取并复验对应 vault 文件。`content` 每次最多返回 2 KB 并提供 `next_offset`；`model` 仅接受验证后的图片，执行像素/2 MB 传输限制并在需要时缩放转 JPEG。
- Host 的 `tool_execution_route` 对非共享工具默认使用 `Platform`，因此 `local_attachment_read` 会进入该 worker；control-plane capability schema 已升级为 `windows-host-attachment-v2`，且只发布这项真实可执行的平台能力。

Windows Realtime 实现（2026-10-20）：

- React 已建立独立 Realtime Domain/Application/Port/Adapter/Observer，Conversation 页面只装配 typed Port；
- Tauri 首次订阅读取 owner-scoped 最新 durable event cursor，之后以有界 page 读取 routing payload，并按已知 Run ID 或 `conversation_id` 判断是否影响当前会话；
- 即使事件页为空，也以 Conversation version 变化触发 reconcile，避免漏掉游标初始化和事件过滤边界内的持久化变化；
- reconcile 使用静默 latest refresh 并按稳定 ID 合并，保留已加载的 older cursor 和旧消息；失败保留现有时间线并有界重试，不出现全屏 loading；
- 当前 stdio adapter 使用短周期非阻塞事件页读取，避免单连接长等待占住业务命令；后续若 Tauri transport 增加真正多路复用，可替换为与 macOS Event Hub 相同的 Host `wait_events` 长轮询，Application 层无需变化。

## 5. Task Graph / Run Inspector：完整调用链

### 5.1 UI 与 Port

UI：

- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskWorkspaceSheet.swift`
- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskInspectorView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Execution/TaskProcessTimelineView.swift`

状态：

- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskWorkspaceViewModel.swift`
- `clients/macos/Sources/ChatOSApp/Features/Execution/MessageTaskWorkspaceViewModel+Loading.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskGraphUseCase.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskGraphPollingState.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskInspectorRefreshUseCase.swift`
- `clients/macos/Sources/ChatOSCore/Application/MessageTaskRunUseCase.swift`

Port 与模型：

- `clients/macos/Sources/ChatOSCore/MessageTaskRunModels.swift`
- `MessageTaskGraphServicing.fetchGraph/fetchTask/fetchRun/retryRun/cancelTask`

Adapter：`clients/macos/Sources/ChatOSConnector/NativeLocalAgentMessageTaskGraphService.swift`

### 5.2 最新执行记录与分页

这是产品语义，不是排序样式：

- 打开 Run Inspector 时请求最新 40 条事件：`eventLimit: 40, eventOffset: 0`；
- Adapter 调 Host events 时固定 `newestFirst: true`；
- 加载更多每次 50 条，以当前事件数为 offset；
- 合并分页时按 event ID 去重；
- `eventLimit` 在 adapter 内限制为 1–100，并多取 1 条判断是否还有更旧记录；
- realtime reconcile 与 `completed/failed/cancelled/persisted` 都刷新 Graph、当前 Task 和当前 Inspector；
- 因此 Windows 运行详情首屏必须展示最新执行记录，不能停留在任务刚开始的记录。

当前分层进度：

- `MessageTaskRunUseCase.loadLatestRun` 已集中首屏最新 40 条规则；
- `MessageTaskRunUseCase.loadOlderEvents` 已集中 50 条分页、offset 与 event ID 去重规则；
- `MessageTaskRunUseCase.loadModelOutput` 已固定不读取事件历史的轻量查询；
- `MessageTaskInspectorRefreshUseCase` 已固定 process/detail/run 三个 section 的刷新查询与陈旧 Run 清理语义；
- Conversation 内嵌 Inspector 的 Task 读取、detail 模型输出与 Run 重试已改由 `MessageTaskGraphUseCase` / `MessageTaskInspectorRefreshUseCase` 执行，ViewModel 不再直接调用 Host service；当前 Task 尚无 Run 时沿用 macOS callback Run 的回退规则，并已固化为用例契约；
- Conversation 内嵌 Inspector 从 process 切换到 detail 时复用当前 Task，仅发起轻量模型输出查询；realtime、polling、显式刷新和目标变化仍先重新查询 Task，以跟随最新 `lastRunID`；
- Conversation 执行过程通过 Host `process` payload mode 读取 durable events；SQLite 只投影 mapper 所需的 tool call identity、状态、原因与错误摘要，移除附件内容和工具参数。macOS 固定每页 25 条、最多 400 页，保持原有 10,000 事件上限且让每页稳定低于 IPC frame 限制；
- 旧版 Turn Process 弹层已改由 `TurnProcessUseCase` 执行 typed session/turn 查询，并用 `TurnProcessLifecycleState` 绑定当前可见请求；弹层关闭会取消具体 Task、立即失效 token，忽略取消的迟到结果不得回写或保活 ViewModel；
- `MessageTaskInspectorRequestState` 已统一 Inspector 最新快照与 Run 事件分页的 request identity；同一 Task 重载、section 切换和刷新都会拒绝旧结果，新的快照也会淘汰旧分页；
- `MessageTaskGraphPollingState` 已持有 polling session identity，`MessageTaskWorkspaceLifecycleState` 已持有弹层 presentation session；SwiftUI ViewModel 只保留具体 Task 的取消、realtime 降级协调与 loading 映射。

### 5.3 并发、空结果与降级

- 切换 Task 时取消旧 inspector、model output、run 和 load-more 请求；
- 选中带 Run 的 Task 时，必须先加载基础 Task 详情，再由完成后的同一快照按当前 section 加载模型输出或 Run；禁止预先启动 section 请求并淘汰基础详情；
- 每次异步写回同时检查 request token 与 `requestedTaskID`；即使相同 Task ID 被重新加载，旧请求也不能覆盖新结果或清理新请求的 loading；
- 已经观察到的稳定 Graph 不得被瞬时空响应擦除；
- 初始空 Graph 最多按现有策略重试，不能无限 loading；
- realtime 中断时展示“实时进度连接已中断”错误，并回退 polling；
- polling 只在存在 active task 或正在等待初始空 Graph 时运行；没有活动任务后停止。
- polling 停止或重启会使旧 session 立即失效；即使底层请求忽略取消并迟到返回，也不得写回 Graph、错误或继续 Inspector 刷新。
- Workspace 弹层关闭会使 presentation session 立即失效，并统一取消 initial load、manual refresh、Run retry、Inspector、realtime 与 polling；旧 session 不得写回或重新启动后台任务。
- initial load 与 manual refresh 会在 Task 启动前冻结 Graph use case、message/lookup、稳定快照、workspace/polling session 与 refresh generation；Graph Host await 不再强持有 Workspace ViewModel，迟到结果只在完整 identity 验收后写回。
- manual refresh 的 Inspector detail→section 请求同样冻结 task、section、target、request token 与 use cases；Task/Run Host await 期间不强持有 Workspace，返回后同时验收 Workspace session、Inspector token、selected task 与 section。
- realtime stream 等待与 polling sleep/Graph/Inspector await 只保留 weak Workspace 引用；每一轮冻结独立请求，弹层释放会立即 deinit/cancel，空闲 stream 或忽略 cancellation 的 adapter 不能把 Workspace 留在内存中。
- Conversation 内嵌 Inspector 的 load、refresh、retry、realtime 与 polling 统一绑定 `MessageTaskInspectorLifecycleState` presentation session；消失时统一取消，重新显示时创建新 generation 并主动 reconcile。切换 Task/Run 会淘汰旧 retry identity、清理旧目标的说明和错误，任何旧 session 的迟到结果都不得写回或为新目标触发刷新。
- Conversation 内嵌 Inspector 在 detail 请求进行中切回 process 时，会取消旧 load generation 并按当前 active Task 重新安排 polling；section 切换不能让活跃任务失去后续刷新。
- Conversation 内嵌 Inspector 在请求开始前冻结 Task/Run use case、message/lookup、selection identity、section、presentation session 与 load generation；Task 和 Run Host await 期间不再强持有 Inspector ViewModel，返回后逐段验收完整 identity。Task 成功而模型输出失败时仍保留 Task 并单独展示模型输出错误。
- 内嵌 Inspector 的 realtime stream 在订阅和空闲等待期间只保留弱 UI 引用；每条信号按 presentation session、Conversation session 与 realtime subscription identity 验收，永久空闲 stream 不再阻止 Inspector 释放。

当前分层进度：

- `MessageTaskGraphUseCase.loadGraph` 已保证瞬时空响应不能覆盖稳定 Graph；
- `MessageTaskGraphUseCase.shouldRefreshWorkspace` 已固化 session reconcile 与当前 Turn 终态/持久化事件的刷新规则；
- Task 详情加载与 Run 重试已通过 `MessageTaskGraphUseCase` 暴露，Workspace ViewModel 不再直接调用 `MessageTaskGraphServicing`；
- Task 活跃状态规范化与 polling 判定所需语义已移入 Core；
- `MessageTaskGraphPollingState` 已集中空 Graph 三次恢复预算、是否继续 polling、600ms/2s/60s 间隔规则及 polling session identity；
- Graph 刷新在请求前后同时校验 workspace refresh generation 与 polling session；停止/重启后的旧结果不会写回，realtime 中断后的降级 polling 仍由当前 session 控制；
- `MessageTaskWorkspaceLifecycleState` 已集中 presentation session identity；初次加载、手动刷新、Run 重试、realtime 与 polling 都必须同时通过当前 session 验收；
- Workspace Run retry 复用 `MessageTaskRetryState` 并绑定 session、Task 与 Run identity；切换 Task 或关闭 Workspace 会取消并失效旧 retry，清理旧目标说明，迟到 Host 结果不得重新加载旧 Inspector、覆盖错误或清理新 retry 的 loading；
- Inspector snapshot/分页验收规则已移入 `MessageTaskInspectorRequestState`，具体 cancellable Task 仍由 macOS adapter 状态层持有；
- Workspace Inspector 冻结请求目标与 Application use case 后再进入 Host await；详情、模型输出、Run 与事件分页请求只在返回后弱引用恢复成功且 token/目标仍有效时写回，因此忽略取消的底层请求不会反向延长 ViewModel 生命周期；
- polling Task 的具体实例与 realtime 降级触发仍由 macOS UI adapter 持有。

### 5.4 测试证据

- `clients/macos/Tests/ChatOSCoreTests/MessageTaskRunUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorRefreshUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorRequestStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskInspectorLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphPollingStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskWorkspaceLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskRetryStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/TurnProcessLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/TurnProcessUseCaseTests.swift`
- `clients/macos/Tests/ChatOSAppTests/MessageTaskWorkspaceViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/TaskReplyInspectorViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/TurnProcessViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/TaskGraphDeliveryPresentationTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentMessageTaskGraphServiceTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphNormalizerTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentHostContractFixtureTests.swift`
- `clients/local_agent_host/layers/interface/src/protocol_tests.rs`
- `clients/windows-react/src/conversation-run-contract.test.ts`

准入结论：Run Inspector、Task Graph 与最新事件分页已达到 `macOS 分层完成，可进入 React`。

Windows Task Workspace / Run Inspector 纵向切片（2026-10-20）：

- React 已建立独立 Run Inspector Domain/Application/Port/Adapter/UI；Conversation 只装配 Port；
- 每条用户消息以 Conversation/Turn/Message 精确关联查询真实 Message Task Graph；`not_found` 或空 Graph 不展示入口；
- 打开详情先查询 Task 基础详情和最新 `GetTaskRuns` 结果，再按 detail/process/run 当前 section 加载；Task、section、弹层、分页和 mutation 均使用独立 request identity，迟到结果不得写回；
- detail 展示目标、说明、模型输出、结果摘要、模型配置和错误；process 只投影 `task_run_process_record_process` 的 append/replace/clear 语义；run 展示最新 Run、错误和事件；
- 最新事件固定 40 条、`newest_first=true`，更早记录固定 50 条并以当前事件数作为 offset，合并时按 event ID 去重；
- Conversation Realtime reconcile 会刷新 Graph、Task 和当前 section；检查器存在 active Task 时另以 1.5 秒有界 polling 降级，终态后停止；已经观察到的非空稳定 Graph 不会被瞬时空响应擦除；
- Retry 使用 Task 当前 version 和可选 instruction；Cancel 与 Restart 同样保留 optimistic concurrency version，操作结束后重新读取持久化 Graph/Task/Run，前端不伪造任务状态；
- Tauri 对普通 Run events 使用 Host `process` payload mode；构建过程时间线时仅在 Rust adapter 内读取受限的完整 `tool_batch_completed` 事件，再向 React 投影 heading/content/time，不暴露其他工具参数、结果或附件内容；
- UI 采用苹果式任务侧栏、segmented control、卡片、确认停止和恢复面板，不使用旧 Windows C# 行为或视觉。
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskGraphFocusTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/MessageTaskAcceptanceTests.swift`

## 6. Host lifecycle：完整调用链

### 6.1 当前状态所有权

UI 与应用协调：

- `clients/macos/Sources/ChatOSApp/AppModelAuthentication.swift`
- `clients/macos/Sources/ChatOSApp/AppModel.swift`
- `clients/macos/Sources/ChatOSApp/RootView.swift`

Port：`clients/macos/Sources/ChatOSCore/LocalAgentHostLifecycle.swift`

Adapter 与进程传输：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostLifecycle.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostProcess.swift`

当前 macOS 已用 `LocalAgentRuntimeState` 显式表达业务就绪阶段，但进程任务、账号 generation、owner 标记和 error 仍共同参与协调。Windows 不得据此简化成“进程存在即 ready”。现有关键事实为：

- `localAgentWorkspaceOwnerUserID == authenticatedUserID` 只表示 workspace/project Conversation 服务已配置；
- `localAgentServicesOwnerUserID == authenticatedUserID` 才是 Conversation 等业务服务的 runtime-ready gate；
- `localAgentControlPlaneOwnerUserID` 表示控制面 bootstrap 完成；
- `LocalAgentControlPlaneBootstrapState` 将 owner、account generation、Host lifecycle generation 与 request ID 固定为一个 token；账号切换、Host 重启、强制刷新或新请求会淘汰旧 bootstrap；
- `LocalAgentHostRecoveryState` 为 health check 与延迟重启绑定 owner、account generation、Host lifecycle generation 和 request ID；取消只用于提前结束工作，迟到结果还必须通过 token 验收；
- `localAgentHostError` 在 `RootView` 顶部显示，不应以无限 loading 隐藏错误。

### 6.2 启动与账号隔离

```text
authenticated
  → 清空上一个账号的异步任务与缓存
  → startLocalAgentHost
  → 等待上一轮 shutdown
  → reset event hub
  → startForBootstrap(workers disabled)
  → 尝试恢复本地 control-plane snapshot
  → 配置 workspace / project conversation 服务
  → 刷新 workspace
  → authenticated gateway control-plane bootstrap
  → 以 Keychain 凭据启动/复用 worker Host
  → configureLocalAgentServices
  → Conversation runtime ready
```

必须保留：

- 每次启动递增 lifecycle generation，所有异步写回同时校验账号与 generation；
- 本地 snapshot 恢复与 authenticated gateway bootstrap 共用冻结 adapter 的服务配置函数；access token、Connector bootstrap 和各业务服务 configure await 不得持有 AppModel，每一段返回后都重新验收 lifecycle/token；
- bootstrap Host 禁用 workers，避免凭据恢复完成前领取 durable work；
- 凭据只进入子进程环境，不出现在命令行、持久化或日志；
- 相同凭据复用运行中的 worker；凭据变化时不擅自杀死正在领取 durable work 的 worker，而是返回 deferred；
- 登出/终止时先让所有 account-scoped service reset，再停止 Host；
- stop 使所有 Conversation 立即进入 runtime unavailable，并取消 realtime/retry/loading。

### 6.3 握手、请求与假健康防护

- 协议版本当前为 `44`；request/response 必须同时匹配 protocol version 与 command ID；
- 启动必须通过 `health`，且 `storage_ready == true` 后才返回 managed process；
- stdio 物理 frame 使用 4 字节大端长度前缀，单帧最大 4 MiB；高位为 continuation 标志，自动重组的逻辑消息最大 64 MiB；
- Host 响应编码超过 64 MiB 逻辑消息上限时返回有界、可重试的 `response_too_large` 错误，并保持传输继续服务后续请求；这属于单请求降级，不等同于 writer 失败；
- 多路复用请求按 command ID 保存 continuation，各自有取消与超时；
- reader 退出或 writer 失败会关闭 transport 并失败所有 pending response；lifecycle 收到传输失败后移除并终止对应 Host，不能留下“进程还在但无法回包”的假健康状态；
- 单次长轮询超时先以 1 秒 health probe 检查同一进程：健康则只返回该请求超时，不连坐其他并发请求；health 失败才使 Host 失效；
- Host 不在运行时发送 recovery notification，并在 startup timeout 内等待恢复；owner 不匹配直接拒绝。

### 6.4 崩溃、睡眠与恢复

- 意外退出发布 `nativeLocalAgentHostDidExit`，清空 active owner 与凭据 fingerprint；
- AppModel 按 250ms、1s、2s、5s、10s 最多重试 5 次；超过上限展示明确错误；
- 系统唤醒会取消睡眠前遗留重试并执行完整 bootstrap，重新从安全存储取凭据；
- health recovery 与延迟 crash retry 同时校验 owner、账号 generation、Host lifecycle generation 和请求 identity，旧结果不能重启新账号或新 generation 的 Host；
- shutdown 串行等待前一轮 shutdown，防止新 Host 与旧清理交叉。

### 6.5 测试证据与目标分层

测试：`clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentHostLifecycleTests.swift`

已覆盖 recovery signal、凭据复用/延迟替换、真实 Rust Host 切换、进程异常退出、健康/失效超时、并发长轮询、停止卡死进程、frame 编码与取消等行为。

macOS 重构应把现有分散事实收敛为可测试的 `HostLifecycleState` 与 `HostLifecycleUseCase`，但迁移后的状态和转换必须逐项对应本节，不能另造一套启动流程。Windows Tauri sidecar 使用同一 Application 状态语义，只替换进程与平台 adapter。

实施进度（2026-10-20）：

- 已新增 `clients/macos/Sources/ChatOSCore/Application/LocalAgentRuntimeState.swift`；
- 已显式建模 `stopped / launching / handshaking / synchronizing / ready / degraded / restarting`；
- AppModel 已按现有启动节点驱动状态，Conversation 创建只在同账号 `.ready` 时放行业务刷新；
- 控制面 service 配置完成前再次校验账号与 lifecycle generation，迟到结果不能把新账号标记为 ready，也不能停止新 generation 的 Host；
- health check 与 crash retry 的并发 identity、五级有界退避、取消和重置已进入独立 Application state；不再只依赖 Task cancellation 或散落整数计数；
- Local Approval、Runtime Settings、Pet、Conversation、Task Graph、Ask User、Turn Process 与 Platform Tool Worker 的固定配置顺序已进入 `LocalAgentServiceConfigurationUseCase`；Connector adapter 持有 bootstrap 细节与瞬时 external MCP credential，AppModel 不再逐项调用具体服务；
- Application use case 会在首个 stage 前及每个 Host/actor await 后重新验收当前 lifecycle，adapter 忽略 cancellation 时也不能继续配置旧账号；
- account-scoped 服务 reset 到 Host stop 的十二阶段固定顺序已进入 `LocalAgentServiceShutdownUseCase`；Host 必须最后停止，AppModel 只负责把冻结的 shutdown adapter 串行接在上一轮 shutdown 之后；
- event hub reset、bootstrap Host 启动、本地 control-plane 恢复、workspace/project/notepad/remote metadata 配置已进入 `LocalAgentHostLaunchUseCase`；API token 与 Connector actors 的组合收口在 App composition adapter；
- launch use case 会在每个 await 后重新验收 lifecycle，本地 snapshot 缺失仍继续 workspace 配置，而 Host 启动失败保持 fatal；
- authenticated gateway bootstrap 的 `立即 / 1s / 2s / 5s / 10s` 有界重试已进入 `LocalAgentControlPlaneBootstrapUseCase`；credential refresh deferred 通过 Application non-retryable error contract 明确短路，其他错误只在最后一次失败后上抛；
- API credential、Connector bootstrap 和 service configuration 组合收口在 App composition adapter，token/lifecycle 校验覆盖 sleep、token 读取、Host bootstrap 与逐服务配置后的每个 await；
- 已增加 generation、owner、恢复、停止、degraded、health check、重启退避、配置/launch/bootstrap 顺序、迟到取消、non-retryable 短路、adapter 失败和 Host-last shutdown 测试；
- `clients/contracts/local-agent/v47` 已提供 canonical health request/success/failure fixture；Rust interface、Swift Connector 与 TypeScript 均读取同一份 fixture，固定协议版本 47、4 MiB frame、64 MiB message、scheduler snapshot 和 `chatos-local-agent-host` service identity；
- `clients/windows-react` 已建立 typed `HostLifecyclePort`、Tauri adapter、Zustand presentation store 和 `HostReadyGate`；Rust Tauri command 使用真实 framed health handshake，不以子进程存在代替 ready；
- Host lifecycle 与 Shell 已升级为 `macOS 分层完成，可进入 React`。当前准入只覆盖生命周期和 Shell；账号 bootstrap、Conversation 以及其他业务页面仍按各自行状态门禁执行。

## 7. Ask User：完整调用链

### 7.1 UI、状态与排序

UI 与状态：

- `clients/macos/Sources/ChatOSApp/Features/Chat/AskUserPromptCardView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Chat/ConversationSessionViewModel+AskUser.swift`

Domain/Port：`clients/macos/Sources/ChatOSCore/AskUserPromptModels.swift`

Adapter：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentAskUserPromptService.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentAskUserPromptMapper.swift`

必须保留的 UI/Feature 语义：

- Prompt 状态为 `pending / ok / canceled / timeout / failed`；只有 pending 可提交；
- Prompt 按 createdAt、updatedAt、ID 稳定升序；未加载到对应 Turn 的 pending Prompt 仍单独展示，不能丢失；
- 每个 Prompt 单独维护 submitting 与 error，提交一个 Prompt 不得锁死全部 Conversation；
- required field 去除首尾空白后不能为空；choice 数量必须位于 minimum/maximum 之间；
- 单选保持一个值，多选达到 maximum 后不能再添加；
- cancel 只有在 `allowsCancel`、pending 且未提交中时可用；
- 状态离开 pending 后立即清空内存中的 secret field；
- 提交/取消失败后保存该 Prompt 错误并重新拉取 Host 真值。

### 7.2 Host 查询、提交与取消

- 仅查询 scope `active` 且 status `waiting_user` 的 Run；服务 limit 归一化到 1–500，单次 Host page 最大 100；
- 只保留 source Conversation 与当前 session ID 相同的 Run；
- Prompt ID 必须为 `local-ask:<run-id>`；resolve 时重新确认 Run 仍为 `waiting_user`，防止过期 UI 提交；
- Prompt 内容来自最新一条 `user_input_requested` event，Host 查询固定 `newestFirst: true`；
- main chat 提交通过 `resumeTurn`，同时携带 expected Conversation version、expected Run version 与状态 `waiting_user`；
- task profile 提交通过 `resumeWaitingRun`；
- main chat 取消通过 `cancelTurn`；非 main chat 只有 owner entity 为 task 时才能 cancel task；
- 提交 metadata 固定包含 `source=ask_user`、prompt ID、values、selection，存在 tool call ID 时一并携带；
- secret 字段或名称含 api key/access token/password/secret/credential 的非空值禁止进入 IPC 或数据库。这一安全限制高于界面控件形态，Windows 必须相同。

### 7.3 测试证据

- `clients/macos/Tests/ChatOSCoreTests/AskUserPromptFormStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationAskUserPromptUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/ConversationAskUserPromptLifecycleStateTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentAskUserPromptServiceTests.swift`
- `clients/macos/Tests/ChatOSAppTests/ConversationSessionViewModelTests.swift`

当前分层进度：

- `ConversationAskUserPromptUseCase` 已固定 Conversation 查询上限 100，并提供 typed fetch/submit/cancel；
- Prompt 按 Turn 投影、未挂载 pending 投影、createdAt/updatedAt/ID 稳定排序、submit/cancel 门禁和原位 upsert 已移入 Application；
- `AskUserPromptFormState` 已统一默认值、required trim 校验、choice 数量边界、单选/多选切换、按 option 顺序提交和 resolved 后 secret 清理；
- `ConversationAskUserPromptLifecycleState` 已统一 refresh request identity、同 Prompt 操作互斥、跨 Prompt 独立并发，以及 mutation 对旧 snapshot 的淘汰规则；
- ViewModel 只持有具体 action Task 与展示状态；runtime unavailable/deinit 会取消操作，迟到 submit/cancel 结果不能写回；失败仍按 Prompt 保存错误并重新拉取 Host 真值，但对账等待期间不强持有 Conversation，返回后必须同时通过 action 与 refresh token 验收；
- 全局活动的 Prompt 定位使用独立 typed focus token 和 ViewModel 持有的 Task；连续定位只接受最新请求，Prompt Host 等待期间不强持有 Conversation；
- Connector 测试已覆盖 main chat 查询/提交、task source Conversation 与取消、secret 在 IPC 前拒绝；
- UI 只绑定 Application form/lifecycle state 并负责视觉展示；macOS 完整切片已通过跨层验收并放行 React。

Windows 实现（2026-10-20）：

- `features/ask-user` 按 Domain → Application Store → Port → Tauri Adapter → UI 分层；React 不读取 Host 原始事件，也不直接调用 Tauri；
- Tauri 仅投影 Prompt 展示字段，查询固定为 active + `waiting_user`、最多 100 个 Run，并逐个读取最新 `user_input_requested`；敏感字段默认值不会进入 React；
- 提交和取消前重新 `GetRun`，联合验收 owner、Conversation、Prompt ID、Run ID 与 `waiting_user`；main chat/task 分别复用 macOS 的 Conversation/Run 和 Conversation/Task 命令；
- React Store 使用 Conversation refresh identity 与 per-prompt action identity；切换会话、旧 refresh、同 Prompt 重复操作和 runtime 迟到结果均不能覆盖当前状态，不同 Prompt 可独立并发；
- 操作失败保留 Prompt 级错误并重新读取 Host 真值；实时事件与初次进入 Conversation 都会刷新 Prompt；未挂到当前分页 Turn 的 pending Prompt 单独展示；
- required trim、单选、多选 min/max、maximum 禁止追加、cancel 门禁及 resolved/unmount Secret 清理均有 Domain/UI 测试；Rust 在 Host IPC 前再次拒绝 Secret 或敏感名称的非空值；
- UI 使用与 macOS 一致的蓝色强调、半透明材质、圆角卡片、分层阴影与浅/深色苹果主题，没有引入 Fluent/WinUI 风格分支。

## 8. 工具审批与恢复：完整调用链

### 8.1 两层事实与调用链

UI：

- `clients/macos/Sources/ChatOSApp/Features/LocalConnector/GlobalApprovalOverlay.swift`
- `clients/macos/Sources/ChatOSApp/Features/LocalConnector/LocalConnectorApprovalsView.swift`
- `clients/macos/Sources/ChatOSApp/Features/LocalConnector/LocalConnectorControlCenterViewModel.swift`

Domain/Port：

- `clients/macos/Sources/ChatOSCore/LocalConnectorApprovalModels.swift`
- `clients/macos/Sources/ChatOSCore/LocalConnectorApprovalStreaming.swift`
- `clients/macos/Sources/ChatOSCore/LocalConnectorControlServicing.swift`

Adapter/worker：

- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentPlatformToolWorker.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentToolApprovalHandler.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+Approval.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentToolClient.swift`

审批链路有两层，Windows 不能合并或跳过：

```text
Rust Host durable tool invocation (approval_status = pending)
  → NativeLocalAgentPlatformToolWorker reconciliation
  → NativeLocalAgentToolApprovalHandler
  → project/conversation execution context + safe cwd
  → NativeLocalConnectorService.approvalDecision
  → policy / AI reviewer / session allowlist / Human UI
  → Host decide_tool_approval(expected invocation version)
  → approved claim 才能执行平台工具
```

Host 中的 durable invocation 是恢复事实源；Local Connector 的弹层队列和 continuation 是当前进程内的交互状态。应用重启后必须由 worker 重新扫描 Host pending approvals 并重建 UI，不能只恢复一份前端 loading。

### 8.2 可进入审批的工具与安全上下文

当前 handler 只处理以下平台写操作：

- 项目代码写入工具；
- `execute_command / process_write / process_kill`；
- 远程命令与远程文件上传。

必须先从 run 解析绑定的项目或直接联系人 execution context，再得到 `executionRootURL` 与稳定 workspace scope。`execute_command` 的审批 cwd 来自命令参数 `path`；相对路径以 execution root 解析，绝对路径也必须仍位于 root 内，并拒绝越界、指向 root 外的 symlink、不存在路径和普通文件。其他工具以 root 为 cwd。

缺失有效运行目的、无法生成展示信息或 cwd 不安全时，不弹出一个可被用户误批的空白审批，而是直接以明确原因拒绝 Host invocation。

### 8.3 策略状态机

`LocalConnectorApprovalMode` 的行为必须保持：

- `request_approval`：进入 Human pending queue；
- `auto_approval`：使用本机审批 Agent；模型未配置、runtime 不可用、Agent 要求问用户或执行失败时降级到 Human queue；
- `full_control`：由 policy 自动允许，不进入 Human queue。

切换到非 `request_approval` 模式必须显式确认风险；切到 `auto_approval` 前必须已选择审批 Agent 模型。

Human 决策仅接受当前 approval 的 `availableDecisions`：

- `decline`：本次拒绝；
- `accept`：仅本次允许；
- `acceptForSession`：当前本机会话内允许相同 scope。

session allowlist key 绑定账号、设备、workspace、project root、cwd、声明 scope、purpose 与权限描述。终端等 exact-request scope 还绑定 command/arguments；稳定的 host-declared capability 才能使用 declared-scope 复用。应用/网关 session 结束时清空 allowlist，不能持久化成永久授权。

### 8.4 UI、stream 与并发语义

- `RootView` 全局承载审批浮层，用户切换项目或页面时 pending approval 仍可见；
- 新 approval 到达时自动展开；多个 approval 可前后切换，active index 在队列变化时归一化；
- 命令、purpose、cwd、source、risk、reason 与时间必须完整展示；
- ViewModel 优先消费 snapshot/event stream；有 stream 时每 60 秒做一致性校验，没有 stream 时每 2 秒轮询；
- 一致性查询短暂失败时保留最后可信队列，不闪空；
- 所有异步结果受 lifecycle/action generation 约束，登出后的迟到审批结果不能写回新账号；
- resolve 后重新并发读取 pending queue 与 approval settings；处理中禁用重复操作；
- Local Connector 决策先移除 pending、发布 snapshot、记录最多 1,000 条历史并发布 event，再恢复等待中的 continuation；
- 最终仍由 Host `decide_tool_approval` 以 invocation identity/version 落 durable 状态，UI 本身不能宣称工具已经执行。

### 8.5 worker 恢复与测试证据

worker 在配置时立即订阅 Host event，并开启短期 reconciliation；`wake()`、新的 tool batch event 和启动恢复都会再次触发 approval resolution。运行中的 approval reconciliation 会合并新的 wake，并在退出后补跑一次，不能丢失临界事件。审批等待独立于已批准 claim 的执行，不得阻塞其他可运行任务。空闲退避从 250ms 增至最多 2s，同时受 activity window 与 generation 限制。

当前分层进度：

- 已从宽泛的 `LocalConnectorControlServicing` 拆出 `LocalConnectorApprovalServicing`，只暴露 settings、pending queue、mode update 与 resolve；
- `LocalConnectorApprovalUseCase` 已统一 settings + pending 并行加载、mode 风险确认透传、`LocalConnectorApprovalDecision` allowlist 门禁，以及 resolve 后重新读取 durable presentation truth；
- 60 秒 stream 一致性校验与 2 秒无 stream fallback 已移入 Core policy；
- `LocalConnectorApprovalQueuePresentationState` 已固定新审批自动展开、删除时不误展开、active index 边界和前后导航；
- `LocalConnectorApprovalMonitoringState` 已统一 snapshot/event/polling 的监控 session token、迟到更新验收、立即刷新和校验间隔；
- 全局浮层、完整审批页与桌面宠物只展示当前 approval 提供的决定，处理中统一禁用；Native service 对 typed decision 再做一次 allowlist 校验；
- `NativeLocalAgentToolApprovalHandler` 已通过独立 Port 区分 Rust Host durable invocation、任务执行上下文与 Human presentation queue，并验证 Human 结果使用原 invocation identity/version 回写 Host；
- Human/policy 决定在 Host 写回前进入本地有界 journal，键绑定账号、invocation identity 与 version；IPC 写回失败时 reconciliation 只重放原决定，不重复询问用户，应用重启后也可继续写回；Host 成功后立即清理，残留记录不能匹配其他 version；
- `LocalConnectorApprovalActionState` 已将每条 approval 的互斥 request identity 从通用控制操作中分离；不同审批和其他设置操作互不取消，重复点击同一审批只提交一次；
- ViewModel 在登出时取消并等待未完成的审批决定，迟到结果不能污染新账号；完整审批页、全局浮层与桌面宠物按 approval 展示独立进度和错误；
- `LocalConnectorApprovalLoadState` 已为审批页显式 snapshot 加载提供独立 request identity；新刷新淘汰旧结果，切离页面和登出都会取消并拒绝迟到写回，短暂失败保留最后可信 settings/queue；
- ViewModel 继续独立持有实际 snapshot/event stream Task；页面加载取消不会停止全局审批 stream/monitoring，页面错误和重试也不会污染全局浮层。

测试：

- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalUseCaseTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalQueuePresentationStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalMonitoringStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalActionStateTests.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalConnectorApprovalLoadStateTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentPlatformToolWorkerTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentExecutionDirectoryResolverTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentToolApprovalHandlerTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeApprovalAgentTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/NativeConnectorStateStoreTests.swift`
- `clients/macos/Tests/ChatOSAppTests/LocalConnectorControlCenterViewModelTests.swift`
- `clients/macos/Tests/ChatOSAppTests/PerformancePolicyTests.swift`

已覆盖等待审批不阻塞已批准 claim、启动/事件恢复、durable approval typed command、同审批重复提交门禁、独立审批并发状态、决定中登出、Human 决定写回瞬时失败与重启恢复、journal 存储失败的进程内恢复、cwd/traversal/symlink 边界、AI reviewer 保守降级、session scope 隔离和登出迟到结果隔离。2026-10-20 在干净 `HEAD` 快照中完成准入复验：Core/Application 20 项、Connector/worker/handler/state 62 项、App ViewModel 21 项、Native XCTest 兼容层 1 组，共 104 项通过。该切片现允许进入 React；Windows 必须复用相同的两阶段状态语义，不能把 presentation queue 当成 durable approval 事实源，也不得在真实 platform worker 就绪前发布对应执行能力。

Windows 已完成 Host durable queue/decision、全局 Human 浮层、terminal session allowlist 与命令审批设置页：

- Tauri 只投影 invocation identity/version、Run、tool name、白名单 command/purpose/cwd/source/risk/reason、时间和可用决定；完整 arguments、claim token、result/error 永不进入 React；
- 每次决定前重新查询 Host pending queue，并再次确认 invocation identity/version、pending 状态与 approval 门禁；React 只展示 Rust 安全投影发布的决定，terminal 写操作提供 `decline / accept / acceptForSession`，其他工具不得借用 session 决定；
- React Store 绑定 owner 与 Host ready session，2 秒 fallback polling 短暂失败时保留最后可信队列；Host unavailable、登出或账号切换会拒绝全部迟到 refresh/decision 结果；
- 同一审批重复点击被拒绝，不同审批可并发；决定完成后重新读取 durable queue，失败保存审批级错误并对账；
- Human 决定在 Host writeback 前写入私有、有界、逐记录原子发布的 journal，键绑定 owner + invocation ID + version；IPC 失败或重启后列表 reconciliation 只重放原决定，冲突决定被拒绝，Host 已不再保留相同 pending identity 时清理陈旧记录；
- `execute_command` 允许前会重读 Run 与 Conversation，严格解析 macOS `tool_options`/`owned_paths` 形状，并从账号隔离的 Registry + Connector grant 解析项目或联系人执行根目录；cwd 仅允许根目录内已存在的目录，路径穿越、文件路径和逃逸软链接全部失败关闭；
- 浮层挂在 `AppShell`，不依赖当前项目或页面；新审批自动展开、移除只归一化 active index，展示语义与 macOS 全局浮层一致；
- Windows terminal platform worker 已接入 Host durable claim/lease/commit：前台/后台命令、poll/log/wait、stdin、kill、有界日志、超时与进程树回收均由 Rust/Tauri 原生层执行；React 不接触 Shell、文件系统或 Host DTO；
- worker 最多并发处理 4 个 claim，30 秒 lease 每 10 秒续约，原样提交 preflight outcome；side-effecting 的未知执行结果进入 `NeedsReview`，已知 pre-effect failure 进入 Failed；Run 完成事件、任务 retry/restart/cancel、Host 失联、登出和应用退出都会释放所属进程；
- `task_execution` 现在只发布附件及 macOS 同源六项 terminal schema，Task policy 开放 `TerminalController`；旧 attachment-only capability snapshot 被离线恢复门禁拒绝；Host 将附件读取与 `process_poll/log/wait` 标为 read-only，`execute_command/process_write/process_kill` 保持需审批的 side-effecting；
- Windows 启动会准备 owner-scoped Connector state，同账号保留经过验证的 workspace grants，账号变化清除旧 grants；项目执行仍必须同时命中账号隔离 Registry 与当前 grant，联系人 workspace 使用 owner + Conversation 哈希隔离；
- Connector state 同时持有稳定 `deviceID`；session key 按 macOS `v2` 算法绑定 owner、device、workspace、canonical root/cwd、`local-agent-terminal:<conversation>`、purpose、权限描述及 PowerShell command/arguments。allowlist 只存在于进程内，Host 新建/替换/失联/停止、登出和应用退出都会清空；generation 门禁阻止旧决定在新 Host session 回填；
- `acceptForSession` 仍先将原 typed decision 写入恢复 journal，再完成 Host durable approve，成功后才写入 allowlist；journal 重放只恢复原 Host 决定，不重建 session 权限。后续命中 scope 的 pending terminal invocation 也先进入 journal，再自动 durable approve，并重新读取 Host queue；
- Tauri/React 设置端口完整投影 macOS `request_approval / auto_approval / full_control` 三态、当前账号模型与推理等级、最多 1,000 条审批历史；账号切换重置高权限模式并清除旧账号模型选择，Host unavailable 时页面给出明确恢复状态；
- 非 `request_approval` 模式必须通过确认对话框传递显式风险确认；`auto_approval` 没有模型时 Rust 拒绝保存。Windows 审批 Agent runtime 尚未完成准入，因此页面明确显示 fallback，pending 操作继续进入 Human queue，不能用规则结果冒充 AI reviewer；
- `full_control` 当前只自动批准已发布并通过 Run/Conversation execution context、账号 grant 与安全 cwd 复验的 terminal 写操作；非 terminal 或上下文不合格操作仍保留在 Human queue；
- 审批决定先写不可变恢复 journal，再写 owner-scoped、有界、原子发布且幂等的审计历史，最后写回 Host；恢复重放会先补齐历史再提交原决定。journal 保存原审批模式与决定时间，并兼容未含 mode 的上一版记录；
- React 设置 Store 将 owner、Host availability、load 与 mutation identity 分离；账号/Host 变化拒绝迟到结果，刷新不能越过进行中的 mutation。设置页复用全局 durable pending Store，支持模式确认、模型/推理选择、逐项决定、错误重试和历史分页；
- Windows 定向验收为 Rust 80 项通过、2 项 sidecar 环境测试忽略，React 28 个文件 103 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过；
- 该阶段已证明 Human durable decision、session/policy 复用、设置持久化、审计恢复与真实 terminal 执行结果写回形成闭环。运行时权限和项目创建/授权 UI 仍是后续独立切片，不能伪造成已完成。

### 8.6 运行与系统权限的 Windows 平台映射

macOS 事实源：

- `LocalConnectorRuntimePermissionsView.swift` 只展示系统权限，不编辑 `LocalConnectorSandboxView.swift` 中的访问控制策略；
- `LocalConnectorSystemPermission(s)` 固定平台、权限项、状态、是否可请求、说明和最近错误的传输结构；
- `NativeSystemPermissions.swift` 由原生层检测并请求权限，SwiftUI 不自行判断系统状态；
- ready 语义包含 `ready / not_applicable / on_demand`，`needs_attention / missing_dependency / action_required` 使用注意状态。

Windows React 已完成对应独立切片：

- Rust/Tauri 发布有界、强类型的 `windows` 原生能力快照，命令要求当前 owner 的 Host session 就绪；React 只解析安全 DTO，不读取系统或 Host 内部状态；
- `terminal` 对应已经准入的 Rust/Tauri PowerShell worker，状态为 `ready`，但明确保留账号、工作区、审批和进程树约束；
- Windows 没有 macOS“完全磁盘访问”的等价总开关；`filesystem` 状态为 `on_demand`，准确说明当前账号 ACL、Registry 与 workspace grant 的联合边界；
- Computer Use/UI Automation 与屏幕采集已接入受信 Plugin Worker：Rust 会读取 Host 安装记录，复验安装完整性、Windows 平台声明、stdio component、必需权限、启动参数、可执行文件和工作目录；重复、越界或不一致记录全部失败关闭；
- `open-computer-use` 0.9.0 的统一 Marketplace 包声明并携带 macOS arm64、Windows x64 与 Windows arm64 payload；Windows 安装器优先解析当前 CPU 对应的 self-contained single-file `.exe`，目标机器无需另装 .NET；打包阶段拒绝错误架构或伪造 PE，安装后仍受签名与逐文件完整性复验保护；
- 独立 Computer Use package workflow 会构建三端 payload、运行 Skill gate 与架构校验，并上传 `.tgz` 和 SHA-256 清单作为后续可信 Release 签名输入；源码提交本身不会绕过 Marketplace 签名发布；
- 权限状态来自 Worker 的有界 `--doctor` 诊断，不再固定伪装为缺失：未安装为 `missing_dependency`、停用或检查失败为 `action_required`、当前交互桌面不可用为 `needs_attention`、真实截图与输入能力均通过后才为 `ready`；
- 只有诊断确认能力尚不可用时才发布 `canRequest`；request command 对未知或不可请求 ID 失败关闭，并通过已验证的同一 Worker 打开受控 `--onboarding` 指引。Windows Marketplace artifact 尚未发布时，正常安装路径仍会诚实显示 `missing_dependency`；
- React Store 绑定 owner 与 Host availability，refresh/action 使用独立 request identity；短暂刷新失败保留最后可信快照，账号替换或 Host suspension 拒绝迟到结果；
- Windows 全量验收为 Rust 445 项通过、2 项 sidecar 环境测试忽略，React 130 个文件 472 项通过；Rustfmt、TypeScript、Vite production build、架构审计、bundle 预算和 `clippy -D warnings` 通过；Computer Use 另有 18 项 Swift 测试、3 项跨平台打包校验与完整 Skill gate 审计通过。

### 8.7 访问控制与执行后端的 Windows 平台映射

macOS 事实源：

- `LocalConnectorSandboxView.swift` 固定“默认任务权限 + 运行后端”两张卡片，展示启用状态、文件权限、审批策略/审批人、网络访问、策略版本及后端隔离能力；
- `LocalConnectorSandboxSettings` 与 `LocalConnectorSandboxBackend` 是页面传输契约，SwiftUI 不自行探测进程隔离能力；
- `NativeLocalConnectorService.fetchSandboxBackends/fetchSandboxSettings/updateSandboxSettings` 与 `NativeConnectorPersistentState` 提供当前原生实现；现有 macOS View 选项值与持久化默认值仍有命名差异，Windows 不复制未经执行链路证明的枚举；
- 第 31 页与第 29 页命令审批共享审批语义，但不得形成两份互相漂移的审批配置。

Windows 真实能力核验：

- terminal worker 在执行前重读 Host Run/Conversation 与 `tool_options`，解析 owner-scoped Registry/workspace grant，只允许规范化后的项目 cwd；
- PowerShell 命令仍继承当前 Windows 账号 ACL 和本机网络，cwd 边界不等于操作系统级文件沙箱，审批前置校验也不等于网络隔离；
- Windows Job Object 使用 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 并在 Run/Host/应用生命周期结束时回收进程树，只有 `processTreeControl` 可准确标记为已提供；
- 当前没有可安全写入并由 worker 执行的文件/网络策略字段，因此不能照搬 macOS 可编辑 Toggle/Picker。

Windows React 已完成对应独立切片：

- Rust/Tauri 从 owner-scoped Connector state 读取现有 `approvalMode`，发布 `windows_local_process` 后端与强类型策略快照；未知审批模式失败关闭；
- 后端状态固定为 `limited`、不可选择，准确发布 `filesystemIsolation = false`、`networkIsolation = false`、`processTreeControl = true`；
- 默认文件权限显示 `host_account` / Windows 账号权限，网络显示 `host`；策略页面只读，直到真实隔离机制进入执行链路后才允许编辑；
- 审批策略由现有 `request_approval / auto_approval / full_control` 映射为 macOS 页面语义，仍以第 29 页持久化设置为唯一真值；
- React Store 绑定 owner、Host availability 与 request identity；刷新失败保留最后可信快照，账号替换、Host suspension 和页面离开拒绝迟到写回；
- Apple 主题页面保留 macOS 两张卡片的信息层级，明确展示部分保护和缺失能力，不提供无法生效的控件；
- Windows 定向验收为 Rust 85 项通过、2 项 sidecar 环境测试忽略，React 34 个文件 123 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过。

## 9. 次级模块生命周期审计

### 9.1 Pet 翻译

- 模型加载、历史读取/清空、附件导入与翻译均由 ViewModel 显式持有并在销毁时取消；不再创建无法管理的裸 Task。
- 每次翻译使用独立 `translationOperationID`。流式 delta、最终文本、历史落盘和动画收尾都必须匹配当前 operation；取消后迟到 delta 或完成结果不得写回。
- 翻译 Agent 和历史 Store 在请求开始前冻结；模型、Agent 与磁盘 await 期间只保留弱 UI 引用。提交翻译时会失效并取消仍在读取的附件，避免发送后附件迟到回填到新草稿。
- 已验证延迟模型请求不会阻止翻译窗口释放，并保留“一键截图在模型就绪后自动提交”、成功后清空 composer、历史 newest-first 与容量限制语义。

### 9.2 Media Studio

- 账号切换继续以 `sessionID` 隔离全部结果；模型目录、图片生成、视频生成和历史分页分别使用 owner/account/request typed lifecycle，输入图片/视频首帧/参考音频继续使用独立 selection identity。
- 第 06 页图片生成已完成第一段 Application 分层：Core 新增窄 `MediaImageGenerating`、`MediaStudioImageHistoryPersisting`、`MediaStudioImageGenerationUseCase` 与 `MediaStudioImageGenerationState`。ViewModel 不再直接编排“调用 provider → 保存本地历史”，只冻结 request/owner、持有 Task 并消费 `saved / generatedWithoutPersistence` 强类型结果；账号 session、owner 与 request token 共同拒绝迟到写回。
- “生成成功但保存失败仍展示结果”的既有语义现在由 Use Case 明确表达；取消在 provider 返回后、历史保存前重新检查，不会把旧账号结果写入磁盘。3 项 Core 测试覆盖 Port 路由、持久化失败和 lifecycle token，既有 Media Studio ViewModel/History/Image Interaction 22 项通过。模型目录、历史分页、参考图导入和 Windows 平台媒体/凭据边界仍需继续完成，当前不得进入 React。
- 模型目录已收口到 `MediaStudioModelCatalogUseCase`：Gateway/Connector 继续实现窄 `MediaGenerationModelCatalogProviding` Port，Application 层保留当前完整图片候选，并精确复现仅允许 `MiniMax-H3` 进入视频候选的 macOS 规则。`MediaStudioModelCatalogState` 联合绑定 owner、account session 与 request，ViewModel 只消费目录快照、协调选中项和 loading/error 展示。Core 2 项及 ViewModel 5 项回归覆盖规则、连续刷新与跨账号迟到结果。
- 历史首页/分页已收口到 `MediaStudioHistoryLoading`、`MediaStudioHistoryUseCase` 与 `MediaStudioHistoryLoadState`：Core 拥有图片/视频历史 DTO、游标、页面及合并规则，磁盘 Store 不再反向依赖 ViewModel 类型。首屏替换、下一页去重/排序、损坏记录累计、游标推进和 1...200 页大小上限由 Application 层定义；owner/account session/request/page kind 联合拒绝迟到结果。Core 3 项、Store 8 项及 ViewModel 5 项回归通过。
- 参考图导入已收口到 `MediaStudioImageReferenceImporting`、`MediaStudioImageReferenceUseCase` 与 `MediaStudioImageReferenceState`：Application 层拥有 8 张上限、当前账号历史精确授权、去重合并与 owner/account/request/source lifecycle；macOS Adapter 独占 security-scoped URL、文件类型/20 MB/6400 万像素检查、MIME/base64 转换，以及本机/内联/无凭据 HTTPS 生成图片读取。ViewModel 不再执行文件 I/O、图片解析或编码。Core 4 项、图片交互 11 项、历史引用 8 项及性能策略 31 项回归通过。
- Windows 原生媒体边界已完成：Tauri 模型目录和生成命令只接受 owner-scoped typed request，模型 Secret 从 Gateway 解析后写入系统凭据库并只在 Rust 内参与 provider Authorization；文生图、多参考图编辑、48 MiB 有界响应、20 MiB 单图、无凭据 HTTPS 远端结果、owner hash 历史目录、相对 manifest、随机 content token 和原子提交均已实现。账号 generation 在网络与落盘边界复验；落盘失败保留 `generatedWithoutPersistence`，序列化结果不含 Secret 或绝对路径。历史首页/分页保持 macOS 默认 10 条、1...200 上限、modified-at + record ID 游标、损坏记录隔离和当前页 manifest 解码；`chatos-media` 预览协议绑定 owner + authentication generation 随机 session，逐次复验 manifest 成员和非链接有界文件。原生 picker 与历史引用统一进入 presentation-scoped 临时 vault，真实内容只接受 PNG/JPEG/WebP，最多 8 张、每张 20 MiB/6400 万像素，逐级拒绝 symlink/reparse 并以 SHA-256 复验；生成 IPC 只接受 opaque reference ID，不接受路径或 base64。页面、账号、应用 lifecycle 和下次启动清理 vault，迟到 picker 由 epoch 拒绝。Rust 全量 222 项通过、2 项 sidecar 环境测试忽略，Clippy 无警告；原生准入完成。
- Windows React 已建立独立 `media-studio` Domain/Application/Port/Adapter/UI：Zod 严格限制模型、历史页、游标、参考图、生成结果和受控预览 URL，Tauri `invoke` 只存在于 Adapter 并在 `App.tsx` composition root 构造；React 不读取路径、Secret、原始文件字节或系统凭据。
- Store 分离模型、首屏历史、旧页、参考图和生成 request identity，绑定 owner/account generation/presentation；生成期间冻结模型、提示词、尺寸、数量与参考图请求，账号/页面切换拒绝迟到写回。分页按 ID 去重、newest-first、累计 unreadable count；迟到 picker 返回的 vault 资源会主动 discard，页面 teardown 必定调用 presentation discard。
- `saved` 结果进入本机历史；`generatedWithoutPersistence` 以 transient canvas item 保留图片并明确显示未落盘警告，不会伪装成历史记录。Apple 主题 UI 保持 macOS 的创作类型 header、左画布、右生成设置、模型刷新、多参考图、本机 picker、当前账号生成记录 picker、提示词建议、尺寸、1...4 数量和历史分页；视频/剧情模式在各自准入前禁用。
- CSP 仅为受控媒体预览扩展 `img-src` 的 `chatos-media`、Windows 映射 origin 与 HTTPS 图片，不放宽 `connect-src`、脚本或 Tauri IPC。React 全量 78 个测试文件、279 项通过，TypeScript、production build 和架构审计通过；Windows Media Studio 原生定向 15 项通过。第 06 页 AI 图片已完整进入 Windows React。
- 第 07 页视频生成的 macOS Application 分层已完成：Core 新增窄 `MediaStudioVideoHistoryPersisting`、`MediaStudioVideoGenerationUseCase`、`MediaStudioVideoGenerationOutcome` 与 `MediaStudioVideoGenerationState`。Use Case 统一编排 provider progress、取消复验、`saving` 阶段和历史保存；ViewModel 只冻结 request/owner、持有 Task、消费 progress/outcome，不再直接调用视频 provider 或磁盘 Store。生成成功但保存失败以强类型结果保持 failed canvas 语义，provider 忽略取消并迟到返回时也不得落盘；owner/account session/request token 共同拒绝旧账号和旧请求写回。新增 Core 4 项及既有 ViewModel/History/Video Canvas 16 项通过。
- Windows 视频原生门禁已完成：Rust 精确实现当前 macOS 仅允许 `MiniMax-H3`、768P/2K、4...15 秒和既定比例的 `/videos` 契约，typed start/poll/cancel job 投影 `queued/in_progress/unknown/downloading/saving/failed/completed`，创建/查询 JSON 各限制 2 MiB，视频内容限制 512 MiB；signed HTTPS CDN 不携带模型凭据，legacy content endpoint 才使用 Rust 内的 bearer token。首帧复用图片 token vault，参考音频使用独立 owner/authentication generation/presentation-scoped 原生 picker vault，校验真实 MP3/WAV/M4A/AAC 内容、20 MiB、SHA-256 与非链接文件。job 绑定 owner/account generation/presentation/request/poll lease，限制单账号 4 个活动任务和 180 次轮询；取消、账号/页面/应用 teardown 使迟到 provider 响应、下载和 1 MiB 分块写盘全部失败关闭。视频历史使用独立 `VideoAccounts` 物理目录以保持已上线图片游标稳定，仍通过同一 owner/session `chatos-media` 协议投影，不向 IPC 暴露路径、媒体字节或 Secret。新增 20 项，Media Studio 定向 35 项、Windows Rust 全量 242 项通过、2 项 sidecar 环境测试忽略，Clippy 零警告；现在允许进入 React 视频切片。
- Windows React 视频切片已完成：strict Zod Domain/typed Port/Tauri Adapter 只接受 opaque 首帧、音频、job 与 content token，并联合校验 preview URL、job ID 和 client request ID；路径、视频字节、base64 视频、Secret 与未知字段在 Adapter 前后均失败关闭。Store 将图片与视频状态隔离，冻结 MiniMax-H3 模型与全部生成参数，start/poll 绑定 owner/account generation/presentation/request identity 并约 10 秒轮询；取消、换账号和页面 teardown 会停止轮询、清理原生 presentation 并拒绝迟到结果。停止等待恢复旧视频，provider/保存失败保持 failed canvas，completed 结果 newest-first 合并到视频历史。Apple 主题 UI 已映射 macOS 视频画布、进度、停止等待、首帧、参考音频、分辨率、比例、时长及历史播放；剧情模式保持禁用。审查同时补齐原生 reset 对视频 audio vault/job registry 的清理。Media Studio React 31 项及全量 78 个测试文件、289 项通过，Rust Media Studio 36 项、Windows Rust 全量 243 项通过、2 项 sidecar 环境测试忽略，TypeScript、production build、架构审计和 Clippy 均通过。
- 历史首页/分页、图片生成→本地保存、视频生成→进度→本地保存，以及所有媒体读取任务都先完成 adapter/磁盘工作，再以 weak ViewModel 恢复并联合验收 identity。
- 图片生成成功但本地保存失败时仍显示生成结果并给出历史错误；视频保存失败继续保留失败画布语义；取消视频必须拒绝服务端迟到完成。这些均为现有 macOS 行为，不得由 Windows 自行修改。
- Media Studio 43 项 macOS 回归覆盖历史分页/损坏记录隔离、账号切换、图片引用、视频画布取消/重试、保存失败及延迟生成释放。
- Story Studio 已完成项目首屏/分页、Agent history、media batch、图片/首尾帧/视频生成、进度恢复、末帧提取、顺序批次、Agent planning/runtime、编辑/导入/保存与镜头细化的 await ownership 审计；所有请求先冻结 Store/service/project snapshot，I/O 完成后才弱恢复 UI 并验收 identity。
- Story provider 图片/视频生成悬挂时 ViewModel 可释放；Story Studio 54 项与 Story Agent 相关测试通过。完整 UI/Port 映射与跨平台媒体能力差异仍未完成，因此只有剧情模式继续不具备 React 准入资格。
- 第 08 页项目目录读取已先行进入 Application 边界：Core `StoryProjectCatalogLoading`、`StoryProjectCatalogUseCase` 和 typed load state 定义首页/下一页、1...200 上限、newest-first、ID 去重、损坏记录累计及 owner/account session/request/page kind identity；`StoryProjectStore` 作为 macOS Adapter 映射其私有文件 cursor，ViewModel 不再直接读取 Store snapshot。Core 3 项与 Story Studio 55 项通过，包含跨账号延迟首页拒绝。该切片完成时创建/设置 mutation、规划/Agent、素材/帧/视频批次与 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页创建/设置已进入第二段 Application 边界：Core `StoryProjectMutationPersisting`、`StoryProjectMutationUseCase`、settings DTO 与 typed mutation state 统一 owner、schema v2、模型 enabled/API Key 门禁、视频能力快照、未决任务模型冻结及纯持久化副作用。设置从 canonical project 重建，过期表单不能覆盖原文、计划、媒体、任务 identity 或版本元数据；Store Adapter 保留连续性与活动视频合并。Core 5 项与 Story Studio 56 项通过，包含跨账号迟到创建拒绝。规划/Agent、素材/帧/视频批次、历史安全投影与 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页规划 Agent 已进入第三段 Application 边界：Core `StoryPlanningExecuting`、Use Case、start/resume/abandon command、safe run/stream projection、typed operation state 和兼容 capability snapshot 的 project fingerprint，统一 owner/project/run identity 及 2,000 字补充想法上限。macOS Adapter 独占 AgentRuntime、provider/Memory、checkpoint、tool receipt、暂停/恢复/应用/放弃；ViewModel/UI 不再接触模型消息、工具参数/receipt、memory scope、usage 或配置 Secret。Core 5 项、Story Agent 21 项与 Story Studio 56 项通过，既有迟到模型账号切换测试继续有效。Agent 历史分页、制作批次、素材/帧/视频生成与 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页历史与制作批次投影已进入第四段 Application 边界：Core `StoryHistoryLoading`、Use Case、typed cursor/page/initial recovery result 及 initial/planning/media 三条 load lane 统一 owner/project/session/request identity。macOS Adapter 独占 Store 文件游标、completed planning draft 启动恢复和旧视频批次 reconcile；较早规划分页保持只读。UI 只消费不含模型目录、provider job identity、generation intent、持久化摘要、凭据及本机媒体文件名的 `StoryMediaBatchSnapshot`，ViewModel 不再直接调用 Story history Store。Core 4 项、Story Studio 57 项与 Story Agent 21 项通过，包含跨账号迟到历史拒绝。制作批次执行、素材/帧/视频生成及 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页持久化制作批次执行已进入第五段 Application 边界：Core `StoryMediaBatchExecuting`、Use Case、safe preparation/command/event/outcome 与 typed execution state 统一 owner/account session/request/project/batch identity；候选规则、补充想法上限和 preview project fingerprint 由 Core 固定。macOS Adapter 独占模型目录、完整 Store 批次、provider job/intent、Session、恢复和放弃持久化，UI 不再接触原始 `StoryMediaBatch`。Core 5 项、Story Studio 59 项与 Story Agent 21 项通过。既有仅视频操作仍走独立顺序队列，等待下一片提取；素材/首尾帧/视频生成、编辑保存和 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页仅视频顺序队列已进入第六段 Application 边界：Core `StoryDirectVideoQueueUseCase`、冻结 plan/step 与 typed lifecycle 统一 preview/project fingerprint、故事顺序、模型能力、参考视频门禁和 owner/account session/project/queue/request identity。每段结束后才读取最新 canonical project 并进入下一段，继续保证实际尾帧连续性；账号、实例或项目替换会在下一次可计费提交前失败关闭。ViewModel 不再解释顺序和 preflight，只桥接仍待提取的单段视频执行入口。Core 5 项与 Story Studio 60 项通过，包含离开项目页后队列继续按顺序完成。素材/首尾帧/单段视频生成恢复、编辑保存及 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页素材图片生成已进入第七段 Application 边界：Core `StoryAssetImageGenerating`、Use Case、安全 command/event/outcome 与 per-resource typed lifecycle 统一 owner/account session/project/resource/attempt/request identity，补充想法限制 2,000 字。不同素材仍可并发，同一素材 single-flight；外来 intent 被过滤、错配最终图片被拒绝，旧任务不能清理替换后的 token。macOS Adapter 独占 Store durable intent、session-bound provider、prompt、provider identity、媒体字节、PNG 归一化与完成持久化；ViewModel 不再持有 provider payload 或文件名。ambiguous provider failure 继续保留 attempt 并要求用户核对后解锁。Core 5 项、Story Studio 61 项与 Story Agent 21 项通过，包含跨账号迟到 outcome 拒绝。首尾帧、单段视频生成/恢复、编辑保存及 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页首帧/尾帧生成已进入第八段 Application 边界：Core `StoryFrameImageGenerating`、Use Case、安全 command/event/outcome 与 global single-flight typed lifecycle 统一 owner/account session/project/segment/role/attempt/request identity，冻结按分段声明排序的已确认素材引用并限制补充想法为 2,000 字。macOS Adapter 独占文件名与媒体字节、session-bound provider、prompt/reference index、Store durable intent、PNG 归一化及完成持久化；上一段尾帧到下一段首帧、当前首帧到当前尾帧的连续性输入保持原调用顺序。外来 role intent 与错配终态均失败关闭，ambiguous failure 继续保留 role-specific attempt。Core 5 项、Story Studio 61 项与 Story Agent 21 项通过。单段视频生成/恢复、编辑保存及 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页单段视频生成/恢复已进入第九段 Application 边界：Core `StoryVideoGenerating`、Use Case、安全 command/progress/outcome 与 per-segment typed lifecycle 统一 owner/account session/project/segment/attempt/request identity，冻结模型、分辨率、时长、prompt 与 start/resume 模式。macOS Adapter 独占 Store intent、首尾帧/参考视频媒体字节、session-bound provider、job identity、进度持久化、视频保存和实际末帧提取。preflight 未提交失败会释放 attempt，ambiguous submission 保留 attempt，resume 只轮询原 job；不同分段并发与仅视频队列逐段等待语义保持不变。Core 5 项、Story Studio 61 项与 Story Agent 21 项通过。编辑/导入/保存及 Windows 平台差异仍未完成，剧情模式继续禁止。
- 第 08 页通用编辑保存已进入第十段 Application 边界：Core `StoryProjectEditCommand` 冻结 canonical fingerprint、schema/version、创建时间、基线 revision、模型组合、编辑结果与活动视频 segment 集合；`StoryProjectMutationUseCase` 统一校验 owner、项目身份、不变量和 schema。普通剧情/分段/素材确认编辑及非 Agent outline/refine 不再从 ViewModel 直连 Store，Store Adapter 独占连续性 reconcile、更新时间与 provider-owned 视频状态合并。`runCommit` 绑定 owner/account session/request/edit token，换账号后的迟到编辑不能发布或清理新状态。Core mutation 7 项、Story Studio 62 项与 Story Agent 21 项通过。图片导入 attachment 与视频实际末帧重提取仍待独立 Port，剧情模式继续禁止。
- 第 08 页图片导入已进入第十一段 Application 边界：Core `StoryImageImporting`、Use Case、target/source command 与 typed lifecycle 冻结 owner/project/resource-or-frame/确认策略/活动视频保留集合，并拒绝错配 Adapter outcome。macOS Adapter 独占 generated/local security-scoped 输入、20 MiB 读取、解码/PNG 归一化、Store 文件写入、attachment、自动确认、用户首帧标记和连续性失效。ViewModel 不再读取媒体字节或直连 Store，跨账号/项目迟到结果失败关闭。Core 5 项、Story Studio 65 项与 Story Agent 21 项通过。视频实际末帧重提取及剩余 archive/display Store 边界仍待提取，剧情模式继续禁止。
- 第 08 页视频实际末帧重提取已进入第十二段 Application 边界：Core `StoryVideoTailExtracting`、Use Case、冻结 owner/project/segment/video job/file identity 的 command 与 per-segment lifecycle 统一目标和结果校验；actual tail 必须精确关联冻结 job。macOS Adapter 独占视频路径、AVFoundation 解码、PNG 与 Store apply，ViewModel 不再直接访问文件或持久化。同段 single-flight、不同段并发，跨账号迟到结果失败关闭。Core 5 项、Story Studio 66 项与 Story Agent 21 项通过。完成视频 archive 持久化、媒体 URL 投影和最终平台差异审计仍待收口，剧情模式继续禁止。
- 第 08 页完成视频重新生成前的 archive 已进入第十三段 Application 边界：ViewModel 构造 `StoryProjectEditCommand` 并通过现有 mutation Port 持久化旧视频、attempt 与指导方式；Store 直连已移除。归档结果只有通过 owner/account session/request edit token 后才允许进入单段视频 generation，因此迟到/取消结果不能触发 provider 计费。Story Studio 66 项通过。媒体 URL 投影与最终平台差异审计仍待收口，剧情模式继续禁止。
- 第 08 页媒体访问已进入第十四段 Application 边界：Core `StoryMediaAccessResolving` 与 `StoryMediaAccessUseCase` 只投影 display-safe `GeneratedMediaAsset` 或本机 file URL，并拒绝 Adapter 返回非 file URL。macOS `StoryMediaAccessAdapter` 独占 owner/project/filename 到 `StoryProjectStore.fileURL` 的映射，ViewModel 不再解释磁盘布局。Core 4 项、Story Studio 66 项与 Story Agent 21 项通过，Story Core 累计 58 项。最终 Store 直连、敏感字段及 Windows opaque media token 平台差异审计仍待收口，剧情模式继续禁止。
- 第 08 页媒体确认已进入第十五段 Application 边界：纯 Core `StoryMediaConfirmationUseCase` 统一 resource/frame attachment、导入自动确认、手动/批量素材确认、首尾帧确认/取消、用户首帧标记及素材变化后的依赖帧失效。macOS `StoryImageImportAdapter` 只处理 security scope、媒体字节、PNG、Store 文件及持久化，ViewModel 不再实现这些确认规则。Core 新增 5 项、图片导入 Core 5 项、Story Studio 66 项与 Story Agent 21 项通过，Story Core 累计 63 项。普通剧情/分段编辑中的确认失效策略与最终平台差异审计仍待收口，剧情模式继续禁止。
- 第 08 页编辑连续性规则已进入第十六段 Application 边界：分段重新细化、分段保存，以及新增/删除/排序分段时的当前/相邻确认与计划失效统一由 `StoryMediaConfirmationUseCase` 计算。ViewModel 不再直接写任何确认、用户首帧、继承来源或尾帧指导字段。Core 确认规则 8 项、Story Studio 66 项与 Story Agent 21 项通过，Story Core 累计 66 项。最终 Store/路径/Secret/媒体字节与 Windows opaque token 审计仍待收口，剧情模式继续禁止。
- 第 08 页最终 macOS 准入审计已完成：`StoryStudioAssembly` 成为具体 Store/provider/Adapter 的唯一 presentation composition root，ViewModel 不再持有 `StoryProjectStore` 或 `MediaGenerationServicing`。源码架构门禁覆盖全部 StoryStudio/Workbench/Views，禁止 Store、本机文件读取、base64/security scope 和确认字段写入回流。Story Core 66 项、Story Studio 66 项、Story Agent 21 项及架构门禁 1 项通过。macOS Story 已允许进入 Windows 原生 Domain/Port/Adapter；Rust 原生门禁完成前 React 剧情入口继续禁止。

#### 9.2.1 Windows Story 强制平台契约

| 关注点 | macOS 当前事实源 | Windows 实施要求 |
| --- | --- | --- |
| 项目持久化 | `StoryProjectStore` 负责账号目录、原子 manifest/media batch 与文件名安全 | Rust 原生层 owner/session-scoped 存储；React 不读 SQLite、manifest 或本机目录 |
| 本机图片导入 | macOS security-scoped URL、20 MiB 有界读取、解码与 PNG 归一化 | Windows 原生 picker 生成 presentation/session-scoped opaque import token；拒绝 symlink/reparse、越界大小与迟到 token |
| 媒体展示 | Swift 原生层可投影受验证的 file URL | React 只能持有 owner/session-scoped opaque media token 和受控协议 URL；DTO 不含路径、文件名或 base64 |
| provider 调用 | Adapter/Session 持有模型、Secret、请求媒体字节与 provider job identity | Rust 原生 Adapter 持有 Credential Manager Secret、媒体字节、attempt/job；IPC 只发布安全状态、进度与可恢复 opaque operation ID |
| attachment/确认 | Core `StoryMediaConfirmationUseCase` 是 resource/frame attachment、确认和连续性失效唯一规则 | Rust Domain 必须逐项复刻同一输入/输出与失败语义；React 只发 typed command，不自行改确认字段 |
| Agent/批次恢复 | macOS Adapter/Session 持有 checkpoint、tool receipt、完整 batch 与 job intent | Windows 原生层持久化完整私有记录；React 只接收已定义的 safe snapshot/event/outcome |
| lifecycle | owner/account session/project/segment/attempt/request identity 拒绝迟到写回 | Tauri command、Store 和媒体 token 全部绑定 owner + authentication generation + presentation；账号/页面/应用 teardown 失败关闭 |

### 9.3 Agent Group Chat

- Team 与 Direct 的 change stream 在永久等待期间只通过 weak ViewModel 读取当前选择并记录刷新计划，不让不可见页面被事件流保活。
- Team workspace 的 proposals、todos、assets、dashboard、running Runs、附件数据等补充快照，以及 Direct chat 的 proposals、teams 与附件数据，都在 Task 启动前冻结 owner/room/project/store 和附件计划；SQLite/附件 await 期间不再强持有 ViewModel。
- 补充快照只有在任务未取消且当前 room identity 仍匹配时才整体写回；旧 room 的迟到结果不能清空或覆盖新 room。消息主体先展示、补充卡片失败仍解除 Direct timeline 首次定位等待的既有语义保持不变。
- Team/Direct 的 account drain 与 communication fast lane 在每一轮开始时冻结 scheduler、service、owner 和 room，Agent 执行与 delivery 查询期间只保留 weak UI；返回后按当前 room 再计算 `AgentSchedulerIssue`。全局 drain 仍会发布 room change 通知，不可见页面释放不会中断 durable scheduler 事实。
- 高频 Run refresh 现在冻结 run/room/owner/store 请求，在 SQLite 查询和 todo presentation 构建结束后一次性按 room identity 写回；同 Run 的新 refresh 会取消旧请求，旧请求不能移除或覆盖新任务。
- Agent Group Chat 相关 89 项契约、迁移、Store、Scheduler 与 UI 测试通过（2 项显式性能基线按环境变量跳过），继续作为 Windows 行为事实源。
- Windows 已按这条调用链完成 Agent 档案、头像、Skills、私聊、团队创建/成员、概览、Human 团队消息、需求调研、共享资产、Todo、Run 历史与运行详情的 Domain → Store → Port → Tauri Adapter → React 垂直切片；React 不读取团队 SQLite、Host payload、项目绝对路径或模型 Secret。
- 第 18 页全局需求调研已按 macOS `RequirementSurveyCenterView`、`ProjectRequirementSurveysView`、`RequirementSurveyFollowUpView` 与 Host client 行为完成独立准入：侧栏提供一级入口，按当前 owner 的 Workspace Index 项目聚合真实 Host Survey，支持项目总览、待填写/已提交分组、四种题型、版本化提交、原 Run 恢复、后续状态、方案查看/确认及历史成功回复保存确认。Domain schema、Port、Tauri Adapter 与聚合 Store 从 Agent 团队 UI 解耦并由团队页复用同一回答校验；Rust 原生层重新读取并逐项复验 Survey/Run/Plan owner、project、conversation、task、revision 与终态，只投影最终用户回复和方案 Markdown，不投影 trace/tool payload。owner/项目/页面 teardown identity 拒绝迟到结果，版本冲突后重读 Host 真值，无全局 event subscription 时使用可卸载的可见页面 polling；确认只认可当前方案，不自动创建或执行任务。
- 第 20 页记事本已按 macOS `NotepadViewModel`、`NotepadSheet`、`NotepadMarkdownEditor` 与 `NativeLocalAgentNotepadService` 完成独立准入：复用 Host v47 Notepad 协议，提供文件夹树、搜索、创建/重命名/递归删除目录、创建/选择/版本化保存与删除笔记、标签及 Markdown 编辑/预览/分栏。React 不接收 owner、`file` 或本机路径；Rust 在读写前后复验账号 generation、owner、note ID 与 expected version，mutation 冲突后回读 Host 真值。选择、搜索、账号与页面 teardown 均拒绝迟到结果；页面可见期间执行可卸载一致性刷新。图片选择和剪贴板读取只在原生层进行，限制 512 KiB 并复验 MIME、签名、大小与 SHA-256 后插入 Markdown，再通过同一版本化保存链落盘。
- 第 22 页设置 / 通用已按 macOS `SettingsView`、`AppModelAuthentication`、`AgentManagementView` 与 `LocalAgentGroupChatSchedulerExecution` 完成独立准入：owner-scoped JSON 偏好由 Rust 原子持久化并以 revision 防止并发覆盖；界面语言、内部上下文语言、12...18 pt 全局缩放、Windows 防空闲休眠、账号信息与退出登录已进入 Apple 主题 React 设置页。界面语言由共享 locale domain 驱动 Authentication、App Shell、Workspace Sidebar、设置入口及顶级 Host 空态/错误；Skill 管理器从全局上下文语言初始化并像 macOS 一样反向保存。Agent 私聊、团队 Delivery 与 Agent 间通讯创建 Run 时只从 Rust `GeneralSettingsStore` 读取当前 owner 的可信上下文语言，再选择对应双语 Skill 快照；React 不能随消息提交覆盖语言、Skill 正文或摘要。
- 第 23 页设置 / 微信已按 macOS `WeChatCompanionSettingsView`、`WeChatCompanionSettingsViewModel` 与 `ChatOSWeChatCompanionService` 完成独立准入：绑定状态与小程序 session 并行读取，短期单次二维码、扫码后的桌面二次确认、单 session 撤销与整体解绑均进入 Apple 主题 React 页面。Windows Rust 使用当前 owner 的系统凭据 access token 访问同源 User Service；authentication generation 在网络响应后复验。bind ticket、scene、二维码字节与服务端身份只留在 Rust，React 只接收 `issued/claimed/expired`、过期时间和随机 opaque 预览 URL；自定义协议限制 PNG/JPEG、2 MiB、400 万像素及 no-store/nosniff，页面离开、账号恢复/退出和应用重置都会销毁预览能力。轮询保持 2/4/8 秒有界退避，迟到 load/mutation/poll 由 owner、epoch、operation 与 preview identity 拒绝。
- Todo 使用独立 executor Conversation 和 Run；执行能力由创建时固化的计划决定，团队资产按启动 revision 冻结。`started/progress/completed/blocked` 都是持久化回执；Host 成功但缺少终态工具回执，或 `project_write` 缺少提交后读取证据时，任务必须转为 blocked，禁止用模型文本伪装完成。
- Windows 不暴露 macOS 的 `todo_schedule_state`、`todo_start_next` 和 `agent_cycle_complete` 给模型：同等调度语义由账号/团队去重的原生 drain 承担，并在 Agent 工具 mutation、Human mutation、Todo 终态和应用恢复时唤醒。原生事务原子选择每个 Agent 的最高优先级 ready Todo，依赖与单 executor lane 由 SQLite 复验；再开放模型调度入口会形成第二写入者和竞态。
- `agent_workspace_snapshot`、`chat_list_members`、`chat_read_all_unread`、`chat_read_attachment`、`chat_document_create`、`chat_send_message`、`chat_heartbeat_complete` 与 `agent_cycle_complete` 已接入：账户内 Agent、项目团队、成员、消息和附件只以当前 Run 的 opaque ref 暴露；`team_ref` 支持事务内复验当前 Agent 成员关系后主动向另一个项目团队发送，`reply_to_message_ref` 可精确回复其他项目团队或 Agent 私聊中的已签发消息，`target_agent_ref` 会按排序后的 Agent pair 创建或复用唯一双成员 `agent_agent_direct` room。项目团队与 Agent 私聊的未读查询、引用签发、游标推进和读取回执在同一 SQLite 事务完成，Host 提交丢包后按 invocation 重放原结果，不会吞掉未读；消息写入继续按 invocation 幂等，团队 @ 与私聊对端都会生成真实 Delivery。提交 Host ToolCommit 后由原生 drain 唤醒所有存在新 Delivery 的团队和私聊；私聊使用独立 Host conversation、schema v3 Run 上下文、应用恢复扫描、终态对账和孤立 Turn 失败关闭，不会留下“消息已写但目标 Agent 不执行”的伪支持。Agent 路由与 macOS 一致限制为最多 4 跳、同一 root 最多 12 个 Run；达到边界时保留消息和 @ 事实但不再生成 Delivery，并返回稳定的 `routing_stop_reason`。Agent 文档按 macOS 同源限制创建为当前 Run 私有 UTF-8 Markdown 草稿，最多 20 份、单份 2 MiB、总计 8 MiB；每条消息最多一次性消费 5 个 `document_ref`，Team/Direct 未读批量投影附件引用，正文按最多 12,000 字符分页读取并复验成员关系、消息归属、大小与 SHA-256。heartbeat 已按 profile 的时间、间隔和自定义提示在事务内创建 Human Direct 系统消息与真实 Delivery；存在未完成 heartbeat 时不重复入队但继续推进下次计划，原生 worker 每轮扫描到期任务并复用通讯 drain、Host Turn、恢复和终态对账，cycle 完成工具只在已绑定运行中验收，Host 提交失败不会提前伪完成 Delivery。Windows Todo executor 已冻结并执行可信 stdio MCP Plugin 计划，包含 run-scoped capability disclosure、Skill gate、人工审批、调用 journal 与恢复；纯 skills-only Plugin 仍等待 Host 安装模型提供无 executable component 的可信快照。
- Human 团队消息已接入图片、音频和文件附件：React 只持有受控草稿 ID，Rust 在入队事务内重新校验 20 个/合计 20 MiB 上限、字节数和 SHA-256，再将随机私有文件与附件记录原子关联到消息。每个目标 Agent 的 Host Turn 都从持久化原件重新授权 conversation-scoped opaque ref，当前触发消息的二进制附件直接进入多模态输入；历史二进制附件只返回安全元数据和重新附带提示，不能经文本工具泄露 base64。团队 UI 仅展示名称、类型和大小，不接收路径、媒体字节或 `authorized_local_ref`。
- `agent_propose_member`、`agent_propose_existing_member` 与 `agent_propose_member_removal` 已按 macOS 提案边界接入：模型只能生成 pending proposal，不能直接修改成员；Human 批准或拒绝时，提案状态、档案/成员变更、系统回执消息和 `mention` Delivery 在同一 SQLite 事务完成。Direct 批准新 Agent 只创建可复用档案，不加入 Direct room；Team 批准才加入当前团队；移出只改变团队成员关系，不删除档案，当前项目经理必须先完成交接。普通 Human Direct UI Delivery 不会被后台 scheduler 领取，审批回执则会进入 communication drain 并真实唤醒提案发起 Agent。React 仅展示安全提案 DTO 和批准/拒绝操作，完整角色提示词、职责、理由与交接计划均来自原生事实。

## 10. 31 个页面的准入状态

| # | 页面 / 截图 | 状态 | React 准入 |
| --- | --- | --- | --- |
| 01 | 项目目录 | macOS 工作面、目录 list/expand/refresh/search/read/mutation、项目创建、Git 与运行设置语义及 Windows workspace 路径边界已映射；Windows 完整项目工作面已完成 | 目录、创建/授权、预览/搜索、编辑、新建/删除、Git 与项目设置均已进入 React |
| 02 | 项目消息 | macOS 分层与三端 fixture 已完成 | 允许；按 History→Composer→Realtime→Run Inspector 分片实施 |
| 03 | 项目设置 | macOS Application/Port 分层完成；Windows analyzer、typed process、Job Object/process group、bounded log/state long-poll、偏好/系统凭据 Secret、Removal、Favorite 与 Apple 主题 React UI 已完成 | 已进入 React |
| 04 | 联系人聊天 | macOS 直接复用 Conversation session/timeline、空态与插件安装助手隔离提示已映射；Windows 薄组合层已完成 | 已进入 React；不创建第二套会话协议 |
| 05 | 应用 | macOS Listing/Launch Use Case、Feature ViewModel、项目上下文安全投影及异步 lifecycle 已完成分层；Windows catalog/runtime/bridge、typed Store/Adapter、Apple 主题 UI 与 sandbox iframe 已完成 | 已进入 React；exact origin/source/session/nonce 与 CSP 门禁已进入测试 |
| 06 | AI 图片 | macOS 生成/保存、模型目录、历史分页及参考图已进入 Core 窄 Port/Use Case 与 typed lifecycle；Windows provider/Secret、原子历史/分页、安全预览、参考图 token vault、strict React Domain/Port/Adapter/Store 与 Apple 主题 UI 已完成 | 已进入 React；剧情模式继续按独立准入门禁禁用 |
| 07 | AI 视频 | macOS Core Port/Use Case/typed lifecycle 与 Windows provider/job/progress/history/首帧及音频 token vault、strict React Domain/Port/Adapter/Store、轮询/取消语义及 Apple 主题视频工作面均已完成 | 已进入 React；剧情模式仍保持禁用 |
| 08 | AI 故事 | macOS 全部 Story 页面准入完成；Windows 已完成 schema v2 Domain、owner-scoped 原子 Store、opaque 媒体预览、Planning/Memory/历史、编辑确认、素材与首尾帧、视频生成恢复、制作批次和安全历史投影 | 已进入 React；项目/规划/素材/分段/图片版本/单段视频/批量制作/历史分页/连续播放均按 Apple 主题工作面开放 |
| 09 | AI 历史 | macOS 图片/视频历史分页与账号隔离已收口并完成 Windows 原生/React 映射；Story 的规划、制作批次、图片版本和视频修订历史也已在第 08 页接入 | 图片与视频历史已进入 React；剧情历史由 Story 工作面按其 canonical project scope 展示 |
| 10 | Agent 管理 | macOS 档案/模型/职业/职责/头像/Skills 与私聊来源已映射；Windows typed 原生层、Apple 主题 React 管理页及可信 Todo Plugin executor 已完成 | 已进入 React；Plugin stdio executor 与 Skill gate 已开放，skills-only package 和未解析 credential 的安装继续失败关闭 |
| 11 | Agent 文档 | macOS Run-scoped 文档草稿、单次消费、附件读取与数量/字节限制已映射；Windows UTF-8 Markdown 创建、Team/Direct 附加、opaque 引用与分页读取已完成 | 文本文档已进入原生链路；Human 团队消息二进制附件已按当前触发消息多模态、历史安全元数据规则进入原生链路 |
| 12 | Agent 团队概览 | macOS 团队/成员/默认 Agent/项目经理语义已映射；Windows 创建、邀请、职责与负责人管理已完成 | 已进入 React |
| 13 | Agent 团队聊天 | macOS Delivery/Run/heartbeat 主链已映射；Windows Human 消息、@成员、默认 Agent、真实 Host Turn、响应持久化、Run-scoped 账户/团队目录、团队与 Agent 私聊全部未读、指定消息回复、跨团队主动消息、双成员 Agent 私聊、文档/多模态附件、heartbeat、cycle、人员提案及可信 Todo Plugin executor 已完成 | 已进入 React 与原生运行时；Plugin 选择只来自当前 Run 的冻结可信计划 |
| 14 | Agent 团队任务 | macOS Todo 工作契约、依赖、独立 executor、资产快照、工具回执与调度约束已映射；Windows 持久任务板、manager/executor 工具、原生 drain、停止/重试、实时进度、stdio MCP Plugin 与统一 Product/Plugin Skill 路由已完成 | 已进入 React；Plugin 调用保持人工审批、冻结 Release 复验与 uncertain-outcome 禁止自动重放 |
| 15 | Agent 团队调研 | macOS Ask User/Requirement Survey 恢复语义已映射；Windows 真实 Host Survey 查询、回答、取消与 Run 恢复已接入团队页 | 已进入 React |
| 16 | Agent 团队资产 | macOS 项目经理权限、版本、归档与 executor 冻结读取已映射；Windows manager 工具、Human 编辑、历史与 opaque Run 引用已完成 | 已进入 React |
| 17 | Agent 团队运行 | macOS Delivery/Turn/Run 关系已映射；Windows manager/executor lane、终态对账、最新事件详情、Human 停止与 needs-review 重试已完成 | 已进入 React |
| 18 | 需求调研 | macOS 全局中心、项目分组、详情/回答、提交恢复、后续回复、方案确认与刷新生命周期已映射；Windows 独立 Domain/Port/Adapter/Store、原生 Survey/Run/Plan 复验、一级入口和 Apple 主题工作面已完成 | 已进入 React；确认仅固定当前方案版本，不自动开始任务 |
| 19 | 本地终端 | macOS Core byte-stream Port、Connector PTY session、Feature lifecycle 与多标签 renderer 已完成分层 | 已进入 React；Windows 使用独立 owner/presentation-scoped ConPTY、typed Port/Store 和 Apple 主题 UI |
| 20 | Notebook | macOS 文件夹树、搜索、Markdown 编辑/预览/分栏、标签、版本并发、外部刷新和图片粘贴语义已映射；Windows Host 复用、安全投影、原生图片导入、typed Port/Store lifecycle 与 Apple 主题工作面已完成 | 已进入 React |
| 21 | 项目 Git | macOS View/ViewModel/Core Port/Native service/parser、命令边界和异步 lifecycle 已映射并完成准入；Windows owner/project/workspace、Git 进程树、输出/超时、路径成员、Secret 安全投影、typed Port/Store lifecycle 与 Apple 风格工作面已完成 | 已进入 React |
| 22 | 设置 / 通用 | macOS 界面/上下文语言、显示、休眠和账号语义已映射；Windows owner-scoped 原子偏好、共享界面 locale、可信 Agent Skill 语言消费与 Apple 主题设置页已完成 | 已进入 React；内部上下文语言只由 Rust 当前账号偏好进入新 Run，消息输入不得覆盖 |
| 23 | 设置 / 微信 | macOS 绑定、二维码二次确认、session 撤销与解绑语义已映射；Windows User Service 原生适配、opaque 二维码预览、typed Store lifecycle 与 Apple 主题页面已完成 | 已进入 React；token/ticket/scene/二维码字节不跨 Rust 边界，页面与账号 teardown 后预览失败关闭 |
| 24 | 设置 / 宠物 | macOS 设备级偏好、透明浮窗、资源、尺寸、位置恢复与 Run 活动映射语义已映射；Windows 原子偏好、可信资源协议、双原生 Tauri overlay、Host Run 安全投影、typed polling/event lifecycle 与 Apple 主题页面已完成 | 已进入 React；Windows 公开 API 不支持把单一窗口固定到所有虚拟桌面，当前以置顶浮窗实现并在页面明确说明平台差异；Run ID、input 与 terminal outcome 不跨 Rust/窗口边界 |
| 25 | 设置 / 全局工具 | macOS 总开关、四项动作、设备级偏好、快捷键冲突/回退、剪贴板历史、Quick Search 数据源/范围/排序与隐私语义已映射；Windows 原子偏好、真实系统级注册、截图/录屏入口、剪贴板原生监听/存储/恢复、Quick Search 原生搜索执行、两个独立浮层、typed lifecycle 与 Apple 主题设置页已完成 | 已进入 React；文件/应用路径、原生执行目标、使用记录与系统动作继续只由 Rust 持有 |
| 26 | 设置 / AI 模型 | macOS 模型目录、供应商、云端默认、本机审批默认、重试与任务模型语义已映射；Windows owner/authentication-generation-scoped Rust Domain/Adapter、API Key 脱敏、Apple 主题页面及供应商管理弹层已完成 | 已进入 React；API Key、网络请求、本机审批真值和 mutation 校验继续只由 Rust 持有 |
| 27 | 设置 / 设备网关 | macOS 连接状态、本机身份、网关信息与本机边界已映射；Windows owner/authentication-generation-scoped 安全投影、真实 Host 连接控制、平台差异说明及 Apple 主题页面已完成 | 已进入 React；Windows 使用账号认证控制面，不宣称具备 macOS 设备签名长连接 Relay；路径与 workspace fingerprint 继续只由 Rust 持有 |
| 28 | 设置 / 插件与 Skills | macOS Marketplace、本机安装记录、Skill、权限声明、启停、卸载、平台兼容与安装助手语义已映射；Windows owner/authentication-generation-scoped Rust 投影、Host 乐观并发 mutation、运行时终止与 Apple 主题页面已完成 | 已进入 React；Marketplace 安装/更新及手动导入在 Windows 原生层具备签名下载、受限解包、逐文件完整性、依赖隔离和回滚链前保持未准入，UI 明确展示差异且不会伪装成功 |
| 29 | 设置 / 命令审批 | macOS 分层与干净快照准入验收已完成；Windows durable queue/decision、全局浮层、writeback journal、execution context/cwd、terminal platform worker、进程内 session allowlist、三态策略/模型设置与审计历史页已完成 | 已进入 React；审批 Agent runtime 未准入时明确降级 Human |
| 30 | 设置 / 运行时权限 | macOS 系统权限契约与 Windows 平台差异已映射；Windows 原生能力快照、Host/owner lifecycle、刷新与 Apple 主题页面已完成 | 已进入 React；未准入 worker 明确显示为能力缺失 |
| 31 | 设置 / 访问控制 | macOS 信息结构与 Windows 执行边界已映射；Windows owner-scoped 只读策略、真实后端能力、审批真值投影、Host lifecycle 与 Apple 主题页面已完成 | 已进入 React；文件/网络隔离未实现前不发布可编辑策略 |

Run Inspector 是项目消息中的业务弹层，不对应单独的一级截图；它已在第 5 节完成 macOS 分层，并已进入 Windows React 纵向实现。

## 11. 当前准入与下一步

Host lifecycle / Shell 已完成第一轮跨层验收并进入 React。其余业务页面仍按以下顺序实施，不能越过各自行准入门禁：

1. 完成 Windows Host sidecar 打包、退出联动、有界诊断和账号 bootstrap adapter；
2. [已完成] 让 macOS Conversation 与 Run Inspector 仅依赖已提取 ports/use cases，并完成 UI Task/service orchestration 收口；
3. [已完成] 用现有测试验证行为未变化，并补齐生命周期与竞态测试；
4. [已完成] 固定 Conversation history、Message Task Graph、最新 Run events 的 Rust/Swift/TypeScript 共享 fixture；
5. [已完成] 将项目消息切片状态改为 `macOS 分层完成，可进入 React`；
6. [已完成] `clients/windows-react` 已完成账号 bootstrap、History、Composer 文本/附件/显式能力/runtime controls、Realtime、Run Inspector 与 Ask User 核心闭环；后续页面继续按准入矩阵逐个垂直切片实施，禁止一次性复制整个 SwiftUI 页面。
7. [已完成] 工具审批 / 恢复已在干净 macOS 快照通过 104 项定向测试并达到 React 准入；Windows Host durable queue/decision、安全投影、2 秒 fallback polling、全局 Human 浮层与决定 writeback journal 已完成。
8. [已完成] Windows 已按 macOS 边界完成 execution context/cwd、账号 Connector state、批准前置复验和 terminal platform worker；真实命令/进程、Host sidecar、claim/lease/approval/recovery 与资源回收测试通过后，`task_execution` 才发布附件及六项 terminal 能力。
9. [已完成] 实现 account/device/workspace/exact-request 绑定的 terminal session allowlist，扩展 Human 决定为 `acceptForSession`，并在应用/Host session 结束时清空；journal 恢复不重建进程内授权。
10. [已完成] 按 macOS 契约实现命令审批三态策略、模型/推理设置、风险确认、pending 决定和审计历史页；未准入的 Windows 审批 Agent 明确降级 Human。
11. [已完成] 按 macOS 系统权限契约实现 Windows 原生能力快照、刷新/lifecycle 与运行权限页；没有把未准入 worker 伪装成系统授权问题。
12. [已完成] 映射第 31 页访问控制/沙箱策略；Windows 当前仅进程树控制得到执行链路证明，因此策略只读，文件/网络隔离明确显示缺失，审批复用第 29 页唯一真值。
13. [已完成] 第 01 页项目工作面、目录树、预览/搜索、文件写操作、Git 与项目设置已按 workspace grant、规范化相对路径、typed process 与异步 lifecycle 进入 React。
14. [已完成] 项目创建与 Windows 文件夹授权由原生选择器产生 owner/presentation-scoped opaque ticket，通过恢复 journal 收口 owner-scoped Connector state 与 Project Registry，并在账号/弹窗/选择器/保存 lifecycle 中拒绝迟到结果。
15. [已完成] 按 `ProjectDirectoryViewModel`、`ProjectFilesystemServicing.search/readFile`、`NativeWorkspaceFilesystem` 与 `ProjectFileEditorView` 完成项目文件内容预览和只读搜索；路径、大小、编码、二进制、搜索预算与迟到结果门禁均已进入 Rust/React 测试。
16. [已完成] `writeFile/createFile/createDirectory/deleteEntry` 已提取到 macOS Core mutation use case；Windows Tauri 已实现 owner/project/canonical path、symlink/reparse 拒绝、原子写入、内容 revision 冲突、create-new 和删除复验。React Port/Store 已冻结 mutation identity 与请求参数，拒绝文件/项目切换后的迟到写回，并以 Apple 风格编辑、新建和删除确认交互区分副作用失败与 committed-without-refresh；Git 与项目设置继续独立实施。
17. [已完成] 项目 Git 的 snapshot/diff/mutation 行为、进程与路径安全边界已完成源码映射，多步 mutation orchestration 已进入 macOS Core use case。Windows Tauri 已完成 owner/project/workspace/repository 双重边界、snapshot path 成员复验、无通用参数的 typed commands、有界并发输出、process group/Job Object 超时回收、remote Secret 净化与安全保留、committed-without-refresh 投影；React Port/Adapter、Store identity、Apple 风格工作面、8,000 行 diff 和打开项目文件闭环均已完成。
18. [已完成] 第 03 页项目运行设置已完成 catalog/state/environment、analyze/default target/environment mutation、实例 start/stop/delete 与 change stream 的源码映射；macOS Core Use Case/Lifecycle 和 ViewModel request identity 已收口，Windows typed process、Secret、Removal、Favorite、React Feature/Store 与 Apple 主题 UI 已形成闭环。
19. [已完成] 项目删除已从 `AppModel` 的 Registry 直连提取为 owner/project/revision-scoped Port 与 Use Case；删除 receipt 冻结 account generation，账号切换后的迟到完成不得清理新账号展示，本机目录永不删除。Core 3 项与 Native 项目服务 12 项通过。
20. [已完成] 宠物快捷访问已从项目设置 View 对 `PetPreferencesStore` 的直连提取为窄 Port、Application Use Case 和 Feature ViewModel 状态；明确冻结当前设备级、非账号级但与当前项目列表取交集的产品语义。Core、Store 与 ViewModel 8 项定向测试通过，第 03 页 macOS 分层达到 Windows 原生 Adapter 实施准入。
21. [已完成] Windows Project Settings 原生与 React 门禁已完成：owner/project/workspace-scoped analyzer、typed invocation、Job Object/process group、512 KiB 日志、100 条退出实例、start/stop/delete、账号/应用/项目/root teardown、bounded long-poll、原子偏好、系统凭据 Secret、日志脱敏、revision-scoped Registry removal、device-wide favorite 投影、React request identity 与完整 UI 均进入测试。
22. [已完成] 第 04 页联系人聊天按 macOS 直接复用已准入 Conversation surface；普通联系人、插件安装助手能力隔离提示及无公开会话空态已完成 React 映射，不新增协议或 Host 权限。
23. [已完成] 远程资源已完成 Windows metadata/Credential Manager、固定 OpenSSH、host-key、jump host/MFA、目录读取/mutation、committed-without-refresh、opaque 本机选择、8 GiB 流式上传/下载与交互式 ConPTY/OpenSSH session；React 连接管理、SFTP 和 xterm Terminal 工作区已按 macOS 事实源接入并通过全量门禁。
24. [已完成] 第 05 页应用列表与启动已完成 macOS 分层：Listing/Launch 使用独立 Port/Application Use Case，Feature ViewModel 管理 context/launch identity，项目根路径不进入 UI/WebView；Windows 已完成 catalog/runtime/bridge、typed Port/Store/Adapter、项目 context picker、Apple 主题页面和 sandbox iframe，迟到结果与错误 source/origin/session/nonce 均失败关闭。
25. [已完成] 第 19 页本机终端已完成 macOS 分层，并以独立 ConPTY session、typed Tauri Port、多标签 Store 和 Apple 主题 UI 进入 Windows React；task terminal worker 与远程终端权限未被复用，账号/Host/页面/退出 teardown、预算和迟到结果门禁均进入测试。
26. [已完成] 第 05 页 Windows 原生 catalog 已按 Host 当前账号安装记录和 `<appData>/NativeConnector/Plugins` 边界接入；manifest/platform/permission/path、summary/record identity、分页 cursor 与 DTO 脱敏均进入测试。下一片继续完成 local HTTP runtime、进程回收与受限 bridge，达到原生准入前不创建 React 页面。
27. [已完成] 第 05 页 Windows local HTTP runtime 已按 macOS context/storage/health/reuse 语义接入；package bin、受限环境、loopback、owner/presentation/generation、Windows Job Object、失败/reset/账号/Host/退出回收均进入真实子进程测试。下一片只推进 origin/navigation/session 绑定的受限 bridge，完成前继续关闭 React 页面。
28. [已完成] 第 05 页 Windows 原生 bridge 只开放当前真实实现的 `host.context.read`，并绑定 owner/presentation/generation/session/nonce/exact loopback origin/存活 runtime；其他 manifest capability、错误 origin、越限 envelope 与 teardown 后请求均失败关闭。原生准入完成，下一片开始 React Domain/Port/Store、项目 context picker 与 sandbox iframe Host。
29. [已完成] 第 05 页 Windows React 已完成 strict Domain DTO、typed Port/Tauri Adapter、latest-wins Store、Apple 风格列表/Host/context picker 与 sandbox iframe；CSP 只增加 loopback `frame-src`，React 全量 74 个测试文件、258 项通过，production build、类型检查和架构审计通过。
30. [已完成] 第 06 页 AI 图片开始按 macOS 事实源分层；图片生成与本地历史保存已收口到 Core 窄 Port/Use Case/typed lifecycle，生成成功但保存失败及跨账号迟到结果均有明确语义和回归覆盖。
31. [已完成] 第 06 页模型目录读取、`MiniMax-H3` 视频准入规则及 owner/account/request lifecycle 已进入 Core Use Case；连续刷新和跨账号迟到响应不会覆盖当前目录。下一片继续历史首页/分页边界。
32. [已完成] 第 06 页历史首页/分页的 DTO、Port、Use Case、合并规则与 owner/account/request/page lifecycle 已进入 Core，磁盘 Store 不再暴露 ViewModel 类型。下一片继续参考图导入边界。
33. [已完成] 第 06 页参考图的本机文件/生成记录入口、账号授权、数量/字节/像素边界、macOS security scope Adapter 与 typed lifecycle 已完成分层。下一片明确并实现 Windows provider、Secret、本机文件和历史存储差异。
34. [已完成] 第 06 页 Windows 原生模型目录、provider 生成、系统凭据 Secret、账号 generation 复验和 owner-scoped 原子历史写入已完成；生成成功但保存失败保留结果，IPC 不暴露 Secret 或绝对路径。下一片继续原生参考图 token vault、历史分页与安全预览协议，完成前不进入 React。
35. [已完成] 第 06 页 Windows 图片历史首页/分页、损坏记录隔离、owner/session-scoped content token 预览协议和账号/应用 teardown 已完成；分页 DTO 不含绝对路径或媒体字节，旧登录 session 的 URL 失败关闭。下一片完成原生参考图选择 token vault，完成前不进入 React。
36. [已完成] 第 06 页 Windows 本机 picker 与当前账号历史引用已统一进入 owner/authentication generation/presentation-scoped token vault；真实图片格式、8 张/20 MiB/6400 万像素、逐级 symlink/reparse、SHA-256、迟到 picker 和 teardown 均进入测试，生成 IPC 只接受 opaque ID。Windows 原生门禁完成，下一片开始 React Domain/Port/Adapter/Store 和 Apple 主题 UI。
37. [已完成] 第 06 页 Windows React 已完成 strict Domain/Port/Tauri Adapter、独立 request identity Store、生成参数冻结、历史分页/去重、迟到参考图清理、保存失败 transient 结果、页面 teardown 和 Apple 主题图片创作/历史 UI；React 全量 78 个测试文件、279 项通过，production build、类型检查和架构审计通过。
38. [已完成] 第 07 页视频生成已按 macOS 事实源完成第一段 Application 分层：provider、progress、取消复验、`saving` 和历史保存收口到 Core Use Case，磁盘 Store 通过窄 Port 接入；保存失败、provider 自发取消、用户停止等待、迟到 provider 完成及 owner/account/request lifecycle 均保持原语义并进入测试。下一片开始 Windows 视频 provider/job/progress/history/token vault，原生门禁完成前不创建 React 视频页面。
39. [已完成] 第 07 页 Windows 原生 provider、typed start/poll/cancel job、进度、512 MiB 有界下载、取消感知原子历史、分页、安全预览、首帧图片 token 与参考音频 picker/token vault 已形成闭环；Secret、路径和媒体字节不进入 IPC，账号/页面/应用 teardown 与 poll lease 拒绝迟到写回。Windows 使用独立 `VideoAccounts` 物理目录保持现有图片分页游标稳定，产品语义仍与 macOS 一致。下一片开始 React Domain/Port/Adapter/Store 和 Apple 主题视频工作面。
40. [已完成] 第 07 页 Windows React 已在既有 `media-studio` feature 内完成视频 strict Domain/Port/Tauri Adapter、独立历史/输入/生成状态、冻结参数、start/poll identity、约 10 秒轮询、取消/迟到/账号切换、failed canvas、旧视频恢复、newest-first 历史和 Apple 主题视频工作面；原生 reset 同步清理 audio vault/job registry。React 全量 78 个测试文件、289 项，Windows Rust 全量 243 项通过、2 项环境测试忽略，production build、类型检查、架构审计与 Clippy 通过。下一片进入第 08 页 AI 故事的完整 macOS UI/Port 与平台差异映射。
41. [已完成] 第 08 页 macOS Story 项目目录首页/分页已提取 Core `StoryProjectCatalogLoading`、Use Case、page/cursor/collection、合并规则和 owner/account session/request/page kind lifecycle；`StoryProjectStore` 只实现 Port，ViewModel 不再直接消费其 Snapshot/Cursor。Core 3 项、Story Studio 55 项通过，跨账号延迟首页不能覆盖当前列表。下一片继续创建/设置 mutation 与版本/副作用语义。
42. [已完成] 第 08 页 macOS Story 创建/设置已提取 Core `StoryProjectMutationPersisting`、Use Case、settings DTO 与 owner/account session/request/kind lifecycle；创建只校验模型、冻结视频能力并持久化，设置从 canonical project 重建并保留 schema/原文/计划/媒体/任务 identity，未决图片/视频冻结对应模型。Store Adapter 负责时间戳、连续性与活动视频合并，迟到 mutation 不进入新账号。Core 5 项、Story Studio 56 项通过。下一片继续规划/Agent 的 request/runtime/checkpoint/application Port。
43. [已完成] 第 08 页 macOS Story 规划 Agent 已提取 Core `StoryPlanningExecuting`、Use Case、start/resume/abandon command、safe run/stream projection、project fingerprint 与 typed operation lifecycle；macOS Adapter 独占 AgentRuntime、模型/Memory、checkpoint/tool receipt 和草稿应用。owner/project/run identity 对中间事件与最终结果均失败关闭，UI 不再持有运行时私有载荷。Core 5 项、Story Agent 21 项、Story Studio 56 项通过。下一片继续 Agent 历史分页和制作批次 Port。
44. [已完成] 第 08 页 macOS Story 历史首页/分页已提取 Core `StoryHistoryLoading`、Use Case、typed cursor/page/initial recovery result 与三条 load lane；macOS Adapter 独占 Store cursor、规划草稿启动恢复和视频批次 reconcile。UI 的制作记录改为安全 `StoryMediaBatchSnapshot`，不再持有完整模型配置、provider job/intent、持久化摘要或本机文件信息；跨账号/项目迟到结果失败关闭。Core 4 项、Story Studio 57 项、Story Agent 21 项通过。下一片继续制作批次 preview/start/resume/abandon/execution Port。
45. [已完成] 第 08 页 macOS Story 持久化制作批次已提取 Core `StoryMediaBatchExecuting`、Use Case、safe preparation/command/event/outcome 和 typed execution lifecycle；preview 冻结 batch identity 与 project fingerprint，macOS Adapter 独占 Store、模型、provider job/intent、Session、恢复与放弃。UI/ViewModel 不再持有原始批次记录。Core 5 项、Story Studio 59 项、Story Agent 21 项通过。下一片单独提取仅视频顺序队列，再继续素材/首尾帧/视频生成 Port。
46. [已完成] 第 08 页 macOS Story 仅视频队列已提取 Core `StoryDirectVideoQueueUseCase`、冻结 queue plan/step 与 typed lifecycle；preview/project fingerprint、故事顺序、模型/时长/参考视频 preflight 及账号/会话/项目/队列 identity 由 Core 统一。每段完成后才读取最新 canonical project 并启动下一段，保留实际尾帧连续性；ViewModel 只桥接尚待提取的单段视频执行。Core 5 项、Story Studio 60 项通过。下一片继续素材图片生成 Port。
47. [已完成] 第 08 页 macOS Story 素材图片生成已提取 Core `StoryAssetImageGenerating`、`StoryAssetImageGenerationUseCase`、安全 command/event/outcome 与按素材隔离的 typed lifecycle；macOS Adapter 独占 Store intent、session-bound provider、prompt、provider identity、媒体字节与 PNG 持久化。不同素材并发和 ambiguous submission 锁定语义保持不变，owner/account session/project/resource/attempt/request identity 拒绝跨账号迟到结果。Core 5 项、Story Studio 61 项、Story Agent 21 项通过。下一片继续首帧/尾帧生成 Port。
48. [已完成] 第 08 页 macOS Story 首帧/尾帧生成已提取 Core `StoryFrameImageGenerating`、`StoryFrameImageGenerationUseCase`、安全 command/event/outcome 与全局 single-flight typed lifecycle；macOS Adapter 独占 Store intent、session-bound provider、参考媒体字节、prompt/reference index 与 PNG 持久化。素材引用顺序、上一段尾帧连续性、当前首帧到尾帧的参考关系及 role-specific ambiguous attempt 语义保持不变。Core 5 项、Story Studio 61 项、Story Agent 21 项通过。下一片继续单段视频生成/恢复 Port。
49. [已完成] 第 08 页 macOS Story 单段视频生成/恢复已提取 Core `StoryVideoGenerating`、`StoryVideoGenerationUseCase`、安全 command/progress/outcome 与 per-segment typed lifecycle；macOS Adapter 独占 Store intent、provider request/job、参考媒体字节、进度、视频持久化和实际末帧提取。preflight、ambiguous submission、resume 与顺序队列的原有计费/恢复语义保持不变。Core 5 项、Story Studio 61 项、Story Agent 21 项通过。下一片继续 Story 编辑、导入与保存边界。
50. [已完成] 第 08 页 macOS Story 通用编辑保存已扩展 Core `StoryProjectMutationUseCase` 与 `StoryProjectEditCommand`，冻结 canonical fingerprint、项目不变量、基线 revision、编辑结果和活动视频保留集合；普通剧情/分段/确认编辑及非 Agent 规划落盘不再从 ViewModel 直连 Store。Store Adapter 保持连续性和 provider-owned 视频状态合并，typed edit lifecycle 拒绝跨账号迟到结果。Core mutation 7 项、Story Studio 62 项、Story Agent 21 项通过。下一片继续图片导入 Port。
51. [已完成] 第 08 页 macOS Story 图片导入已提取 Core `StoryImageImporting`、`StoryImageImportUseCase`、resource/frame target、generated/local source 与 typed lifecycle；macOS Adapter 独占 security-scoped 文件、媒体字节、PNG 归一化、Store 文件、attachment 与确认/连续性规则。Core 5 项、Story Studio 65 项、Story Agent 21 项通过，包含真实 generated/local 输入及跨账号迟到导入拒绝。下一片继续视频实际末帧重新提取 Port。
52. [已完成] 第 08 页 macOS Story 视频实际末帧重提取已提取 Core `StoryVideoTailExtracting`、`StoryVideoTailExtractionUseCase`、冻结 video identity 与 per-segment typed lifecycle；macOS Adapter 独占视频路径解析、AVFoundation 解码、PNG 和 Store apply。Core 5 项、Story Studio 66 项、Story Agent 21 项通过，包含跨账号迟到结果拒绝。下一片继续完成视频 archive 与媒体 URL 投影边界。
53. [已完成] 第 08 页 macOS Story 完成视频重新生成前的 archive 已改走 `StoryProjectMutationUseCase` 与 typed edit lifecycle；只有归档持久化成功且账号/session/request 仍匹配才会启动后续计费 generation。Story Studio 66 项通过。下一片继续媒体 URL 投影与最终 macOS Story 准入审计。
54. [已完成] 第 08 页 macOS Story 媒体 URL 投影已提取 Core `StoryMediaAccessResolving`、`StoryMediaAccessUseCase` 与 macOS `StoryMediaAccessAdapter`；ViewModel 不再解释本机路径，非 file URL 失败关闭。Core 4 项、Story Studio 66 项、Story Agent 21 项通过，Story Core 累计 58 项。下一片执行最终 Store/敏感字段及 Windows opaque media token 平台差异审计。
55. [已完成] 第 08 页 macOS Story resource/frame attachment 与显式媒体确认已统一进入 Core `StoryMediaConfirmationUseCase`；图片导入 Adapter 复用同一规则，ViewModel 不再实现手动/批量素材确认或首尾帧确认/取消。Core 新增 5 项、图片导入 Core 5 项、Story Studio 66 项与 Story Agent 21 项通过，Story Core 累计 63 项。下一片继续收口普通剧情/分段编辑中的确认失效策略。
56. [已完成] 第 08 页 macOS Story 分段重新细化、保存及结构变化的确认/相邻计划失效已统一进入 `StoryMediaConfirmationUseCase`；ViewModel 对确认与首帧继承字段的直接写入降为 0。Core 确认规则 8 项、Story Studio 66 项与 Story Agent 21 项通过，Story Core 累计 66 项。下一片执行最终 macOS Story 准入与 Windows 平台差异审计。
57. [已完成] 第 08 页 macOS Story 最终准入审计通过：具体 Store/provider/Adapter 组装集中到 `StoryStudioAssembly`，presentation 源码架构门禁禁止 Store、路径、媒体字节/base64、security scope 与确认写入回流。Story Core 66 项、Story Studio 66 项、Story Agent 21 项及架构门禁 1 项通过。下一片开始 Windows Rust 原生 Story Domain/Port/Adapter；原生门禁完成前不创建 React 剧情入口。
58. [已完成] 第 08 页 Windows Rust 私有 Story Domain 已按 macOS schema v2 建立模型、资源、分段、关系、媒体、attempt/video identity 与严格验证；未知字段和不安全原生文件名失败关闭，resource/frame attachment 与首批确认规则保持同源。Rust 定向 6 项及 Clippy `-D warnings` 通过；没有注册 IPC 或 React 入口。下一片建立 owner-scoped 原子 Store、目录分页与安全 DTO。
59. [已完成] 第 08 页 Windows Rust 已建立 owner SHA-256 隔离的 Story 原子 Store、8 MiB 严格 schema v2 manifest、逐级 symlink/reparse 拒绝、私有目录/文件、`tmp + fsync + replace + directory sync` 提交，以及默认 40/上限 200、modified time + project ID 游标、newest-first、损坏记录累计的目录分页。只可序列化的安全 DTO 不包含文件名、路径、provider result/asset/job identity 或媒体字节，媒体仅保留待 vault 填充的 opaque `contentToken`/`previewUrl`。Story Rust 定向 10 项及 Clippy `-D warnings` 通过；没有注册 IPC 或 React 入口。下一片建立 owner/authentication generation/presentation-scoped 媒体 token vault 与受控预览协议。
60. [已完成] 第 08 页 Windows Story 媒体已进入 owner/authentication generation/presentation/project-scoped 内存 token vault 与 `chatos-story` 受控预览协议；安全项目投影只发布随机 opaque `contentToken`/`previewUrl`。每次读取重新复验最新 manifest、项目引用、媒体种类、20 MiB 图片/512 MiB 视频边界和 symlink/reparse，页面/账号/登录代次/应用 teardown 后旧 URL 失败关闭，响应固定 no-store/nosniff；CSP 只增加精确 scheme/host。图片 MIME 与视频 storage filename 校验同步收紧。Story Rust 定向 11 项及 Clippy `-D warnings` 通过；没有 Story 业务 IPC 或 React 入口。下一片建立 authentication generation 绑定的目录/项目读取及安全创建/设置 typed commands。
61. [已完成] 第 08 页 Windows Story 已开放目录分页、单项目读取、创建、设置和 presentation discard 五个 typed commands；每条调用绑定当前 owner/authentication generation，并在磁盘/模型目录 await 后复验。创建只接收安全 creation input，由 Rust 构造 canonical schema v2；设置在 Store 原子 mutation 内从最新 manifest 重建，只改变标题、描述和模型组合。原生模型目录统一复验 owner/enabled/credential，保留 `taskEnabled=false` 的 macOS 手工选择语义并快照视频时长；未决图片/视频提交冻结对应模型。持久化字段、provider identity、Secret 与媒体字节不进入输入或输出。Story Rust 定向 14 项及 Clippy `-D warnings` 通过；React 入口继续关闭。下一片实现原生 Story Planning Agent request/runtime/checkpoint 和 safe snapshot。
62. [已完成] 第 08 页 Windows Story 已建立私有 Planning run/checkpoint 与 64 MiB owner/project-scoped 原子存储：policy/context、完整 messages、tool pending/in-flight/receipt、Responses output、usage、Memory scope、instruction bundle、事件与 draft 都只留在 Rust；strict decode 的损坏文件保留。fingerprint、outline/refine target、完成视频仅在 draft 归档及 detail/确认失效规则对齐 macOS。safe snapshot 不含 prompt、provider output、tool arguments/receipts、Memory checkpoint 或 Secret。Story Rust 定向 18 项及 Clippy `-D warnings` 通过；没有 Planning IPC。下一片接入文本模型、原生 Story tool registry 与 start/resume/abandon runtime。
63. [已完成] 第 08 页 Windows Story 已接入当前账号文本模型、共享 Responses 传输、macOS 同源提示词和 stage-scoped 原生工具 registry，并开放 start/resume/abandon typed commands。owner/authentication generation/presentation/project/run single-flight、teardown 取消、pending/in-flight 恢复、模型/工具边界原子 checkpoint、retry/failed/paused/limit/no-progress、fingerprint 条件应用及放弃后迟到写回拒绝均已进入原生层；安全 DTO/事件不含 prompt、tool arguments/receipts、provider output、Secret 或媒体原生身份。Story Rust 定向 24 项及 Clippy `-D warnings` 通过。平台差异已登记：macOS 还通过 Memory Engine 执行 thread 建立、消息同步/对账与 compose；Windows 尚无验收过的 owner-scoped Story Memory Port，因此明确返回 `usesCloudMemory=false`，不直连 Host SQLite、不伪造不完整 Memory 接入。React 剧情入口继续关闭；下一片先补正式 Memory Adapter/契约，再推进历史、编辑和媒体 provider commands。
64. [已完成] 第 08 页 Windows Story 已建立当前认证 bearer 驱动的 owner-scoped Memory Adapter，复用共享 SDK 与 gateway `/api/memory`，不访问 Host SQLite。thread/subject identity、32 条批量同步、写前 `syncInFlightEnd`、恢复逐条对账、固定消息与同步 digest、tenant/source/thread/record 严格校验、summary/近期记录 compose、孤立工具调用过滤及稳定 Responses continuation 均按 macOS Story Memory 语义进入 Rust 原生层；Memory 网络请求参与 presentation/账号/应用取消，错误投影不泄漏 URL/token/原始响应。绑定成功后安全快照返回 `usesCloudMemory=true`。Story Rust 定向 25 项及 Clippy `-D warnings` 通过。React 剧情入口继续关闭；下一片推进 Story 历史与安全编辑 commands，再处理媒体导入、生成和恢复门禁。
65. [已完成] 第 08 页 Windows Story Planning 历史已开放 owner/authentication generation/presentation/project-scoped typed command；原生 Store 按默认 20/上限 200、文件修改时间 + run UUID 稳定分页，strict decode 的损坏记录只计数保留，DTO 仅发布 safe run snapshot。首页会按 macOS 历史 Adapter 规则恢复通过本地完成校验且 canonical/draft fingerprint 仍匹配的中断终态，较早分页严格只读；恢复项目重新取得 presentation-scoped opaque media token。Story Rust 定向 26 项及 Clippy `-D warnings` 通过。Media batch 历史等待其原生模型，React 剧情入口继续关闭；下一片实现 canonical Story 编辑 command。
66. [已完成] 第 08 页 Windows Story canonical 编辑第一片已开放 `update_story_project_source`：live safe snapshot 发布原生 fingerprint，command 只接收 source/style/ratio typed intent，Store 在原子 mutation lock 内重读并拒绝 stale baseline；已有分段时按 macOS `saveSource` 语义禁止修改。schema、模型、资源、媒体 filename、provider identity、attempt/video 与创建时间完全保留；presentation/account generation teardown 后的迟到提交失败关闭。Story Rust 定向 28 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现素材 prompt typed edit 与确认/依赖帧失效规则。
67. [已完成] 第 08 页 Windows Story canonical 编辑第二片已开放 `update_story_resource_prompt`：typed intent 在锁内按 fingerprint 定位人物/场景/道具，未知字段/目标失败关闭；未决依赖视频禁止修改。成功后按 macOS 确认规则保留媒体与 provider/generation identity，只取消素材确认并清除依赖分段首尾帧确认；素材 name/prompt 上限同步收紧为 120/4,000 字符。Story Rust 定向 30 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现分段文本/detail/relations typed edit与相邻连续性失效。
68. [已完成] 第 08 页 Windows Story canonical 编辑第三片已开放 `update_story_segment`：typed intent 不包含 source range、frame/media、attempt/job、video/archive、error 或 filename；Rust 从最新 canonical 项目重建授权字段。当前段 attempt/video 门禁、当前确认清除和仅对无 attempt 相邻段执行连续性失效均与 macOS `applyingSegmentEdit` 一致，原生媒体/provider 历史状态完整保留。Story Rust 定向 32 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现 add/delete/reorder segment 与全计划失效规则。
69. [已完成] 第 08 页 Windows Story canonical 编辑第四片已开放 `update_story_segment_structure`：add/delete/move 使用严格窄 intent，Rust 生成新增分段 identity；未决资源/帧/视频任务或已完成视频存在时全部失败关闭。删除同步清理关系、移动只允许相邻位置，所有成功结构变化按 macOS 规则失效全量 detail 与首尾帧确认。Story Rust 定向 34 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现 resource/frame confirmation typed commands。
70. [已完成] 第 08 页 Windows Story 已开放 fingerprint/presentation/generation-scoped 单项 resource/frame 图片确认及尾帧取消；资源确认的未决视频门禁、依赖帧失效、显式首帧选择/继承清除，以及 first+last guidance 降级规则均与 macOS Core 一致。目标 DTO 严格拒绝歧义组合，不接受媒体对象或 filename。Story Rust 定向 35 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片补批量 latest 确认与图片导入边界。
71. [已完成] 第 08 页 Windows Story 已开放最新素材图和最新首尾帧批量确认 typed commands；只对实际变化目标写盘，资源未决视频门禁、依赖帧失效、显式首帧 identity、继承清除及可选 first+last guidance 均与 macOS Core 一致。Story Rust 定向 36 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片进入图片导入 token/文件落盘边界。
72. [已完成] 第 08 页 Windows Story 本机图片导入已通过原生 picker、20 MiB/PNG-JPEG-WebP/6,400 万像素/读取竞态校验与 PNG 归一化接入；路径不进入 React。随机私有文件与 fingerprint-guarded canonical attachment 在同一 Store mutation lock 内提交，失败孤儿清理与不明确提交复验进入实现。Story Rust 定向 37 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片补 generated/history 图片导入。
73. [已完成] 第 08 页 Windows Story 已开放 generated/history 图片导入：IPC 仅接受当前账号图片历史的 `recordId/contentToken` 与窄 Story target intent；Media Studio Adapter owner-scoped 读取私有历史字节，Story Adapter 重新校验格式/大小/像素并归一化 PNG，历史路径、filename、provider result/asset identity 均不跨边界。fingerprint、同锁媒体/manifest 提交、孤儿清理及 resource/frame 确认与连续性规则与本机导入一致。Story Rust 定向 38 项、跨 owner/token 桥接测试及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现素材图片生成 Port/commands。
74. [已完成] 第 08 页 Windows Story 已开放素材图片生成：fingerprint-guarded canonical mutation 先持久化 per-resource attempt 并发布安全事件；Media Studio 窄 Port 独占当前账号模型/Secret、provider wire 与媒体下载，素材 prompt 对齐 macOS 的 user ideas/style/resource prompt 组合。完成阶段按 project/resource/attempt identity 合并最新 manifest、PNG 归一化并同锁提交文件；首图自动确认和依赖帧失效规则同源，失败/取消保留 ambiguous attempt。React 不接收 filename、媒体字节、provider result/asset identity 或 Secret。Story Rust 定向 39 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现首帧/尾帧图片生成 Port/commands。
75. [已完成] 第 08 页 Windows Story 已开放首帧/尾帧图片生成：Rust 从 canonical project 按分段顺序解析已确认素材图，首帧追加上一段确认尾帧、尾帧追加本段确认首帧，并生成与 macOS frame context/role prompt/reference index 同源的请求。媒体读取完成后才持久化 role-scoped attempt；provider/Secret/base64/path/filename 不跨 IPC。完成按 project/segment/role/attempt 合并最新 manifest、统一 PNG并自动确认首张结果；AI 首帧不写用户显式选择，实际视频尾帧继承规则同步对账，错误身份无孤儿文件。Story Rust 定向 40 项及 Clippy `-D warnings` 通过。React 入口继续关闭；下一片实现单段视频生成、轮询、取消与恢复。
76. [已完成] Windows Todo Plugin executor 已按 macOS 可信计划完成 stdio MCP 闭环：通讯线程只能从当前账号已验证安装签发 run-scoped `plugin_ref`，Todo 冻结 installation/component/Release/digest/manifest identity，executor 重新复验后通过 capability broker 延迟启动。`capability_search/describe`、真实 MCP initialize/list/call、人工审批、append-only invocation journal、进程树回收与 Host durable disclosure 恢复均已完成；Plugin `SKILL.md`/`references/**` 固定快照、`chatos/skillGate`、opaque `skill_ref`、activate/read-resource 与 argument selector gate 同步进入测试。Windows Host 安装记录当前仍以 executable component 为单位，因此没有 stdio component 的 skills-only package 保持禁用，不伪装完整迁移。
77. [已完成] 第 08 页 Windows Story 已完成单段视频提交/轮询/取消/恢复、实际尾帧解码与继承、ambiguous submission 人工核对、完成视频修订归档、制作批次 preview/start/resume/abandon、仅视频顺序队列、Planning/批次历史分页、项目分页、素材/分段/时长/模型设置编辑、图片版本选择、事实查看和连续播放；React 通过 strict Domain → Store → Port → Tauri Adapter 接入 Apple 主题工作面。定向验收为 Story Rust 54 项、React Story 19 项全部通过。
78. [已完成] Windows Task Skill control plane 已把 Product 与 Plugin 统一分流：Product ref 每次从当前 Run 冻结 bundle 解析并校验 revision 与内容摘要，Plugin 只接受 run-scoped opaque digest；Host durable activation 可恢复，bundle fingerprint 变化不继承旧内存状态。空 Plugin 计划的 `capability_search` 返回空目录而非损坏计划，且无 Plugin 的 Todo 仍可正常激活 Product Skill。
79. [已完成] 第 22 页设置 / 通用已完成 owner-scoped 原子偏好、revision 冲突回读、界面语言/内部上下文语言、12...18 pt 全局缩放、Windows 长驻线程防空闲休眠、账号卡片与退出登录；共享 locale 已接入 Authentication、App Shell、Workspace Sidebar 和 Host 顶级状态。Agent Skill 管理器按 macOS `initialLanguage/onLanguageChange` 同步全局上下文语言；Agent 私聊、团队与 Agent 通讯的新 Run 只由 Rust `GeneralSettingsStore` 选择中英文 Skill 快照，不信任 React 回传。Rust 通用设置 3 项、Agent Skill 4 项、React 全量门禁及 Clippy `-D warnings` 通过。
80. [已完成] 第 23 页设置 / 微信已完成当前账号绑定状态、短期小程序码、扫码后二次确认、登录 session 撤销和整体解绑。Rust 独占 access token、ticket/scene 与二维码字节，以随机预览 token、自定义协议、PNG/JPEG 签名、2 MiB/400 万像素、no-store/nosniff 和 authentication generation 形成失败关闭边界；React 只消费 typed 安全投影并以 2/4/8 秒轮询退避、owner/epoch/operation identity 拒绝迟到结果，页面 teardown 主动销毁二维码。Rust 微信定向 3 项、React 微信 7 项、全量门禁及 Clippy `-D warnings` 通过。
81. [已完成] 第 25 页设置 / 全局工具第一阶段已按 macOS 默认关闭、四动作默认开启、首次冲突确认、单项注册状态和 350ms 触发门禁完成 Windows 映射。设备级原子 JSON 由 Rust 持有 revision，登录加载时注册、退出登录立即注销；React 只提交结构化快捷键 intent，不持有原生 key code。Windows 将 macOS Command 语义适配为 Control，Quick Search 冲突时回退 `Control+Shift+Space`；截屏/录屏由 Rust 直接打开 Windows 系统入口，Clipboard History/Quick Search 在各自原生浮层完成前只安全唤醒主窗口并继续标记未准入。React 全量 117 个测试文件、450 项，Rust 定向 3 项、production build、架构/Bundle 审计与 Clippy `-D warnings` 通过。下一片实现 Clipboard History 原生存储/监听/安全投影与独立浮层。
82. [已完成] 第 25 页 Clipboard History 已按 macOS 事实源完成 Windows 原生轮询、敏感格式及私钥/token 对过滤、25 MiB 上限、SHA-256 去重、500 条/30 天清理、置顶/删除/清空和恢复。Rust SQLite 与私有 payload 目录不跨 IPC；React 只接收 360 字预览、时间、大小、固定类型和受控图片 URL，图片协议重新验证记录、格式、像素预算并固定 `no-store/nosniff`。快捷键打开独立透明 Tauri 浮层，查询/选择/快捷键/图片预览采用 Apple 主题；账号退出会注销热键、停止监听并隐藏浮层，后台刷新失败保留最后可信列表。React 全量 120 个测试文件、454 项，Clipboard Rust 3 项、production build、架构/Bundle 审计与 Clippy `-D warnings` 通过。下一片实现 Quick Search 原生数据源、排序和独立浮层。
83. [已完成] 第 25 页 Quick Search 已按 macOS 事实源完成项目、联系人、Windows 应用、文件和五项内置操作数据源，支持 `>` 操作、`@` ChatOS、`/` 文件范围前缀、150ms 文件搜索 debounce、3 秒/50 条边界、同源文本评分及最多 512 条最近/频率加权。Rust 以 presentation/request identity 拒绝迟到查询，独占项目注册表、应用/文件路径、原生执行动作和设备级原子 usage；React 只消费最多 50 条安全结果及 opaque result ID。文件扫描跳过 hidden/system、symlink/reparse 和依赖/构建目录，打开时重新复验目标；快捷键使用独立 760×520 透明 Apple 主题浮层，项目/联系人/设置只通过 owner-scoped typed navigation event 返回主窗口，退出登录、关闭页面、禁用功能和账号代次变化均使旧结果失败关闭。React 全量 123 个测试文件、459 项，Quick Search Rust 4 项、production build、类型检查、架构/Bundle 审计与 Clippy `-D warnings` 通过。第 25 页完成；下一片进入第 26 页设置 / AI 模型。
84. [已完成] 第 26 页设置 / AI 模型已按 macOS 事实源完成模型目录与供应商并行读取、供应商新增/修改/删除/模型刷新、Memory 总结默认、本机审批默认、0–10 次重试及逐模型任务用途/Thinking/Temperature/Max Tokens/任务开关。Rust 将全部读取与 mutation 绑定 owner 和 authentication generation，API Key 只进入原生请求且 DTO 仅发布 `hasApiKey`；新增要求密钥、修改留空保留、显式清除、默认模型必须启用且有凭据、Temperature 0–2 和 Max Tokens 正整数均在原生边界复验。本机审批默认复用第 29 页唯一真值，React 仅持有安全草稿和 opaque ID。Apple 主题页面、供应商弹层、Domain/Port/Store/Adapter 已接入；React 全量 126 个测试文件、464 项，Rust 定向 4 项、production build、类型检查、架构/Bundle 审计与 Clippy `-D warnings` 通过。第 26 页完成；下一片进入第 27 页设置 / 设备网关。
85. [已完成] 第 27 页设置 / 设备网关已按 macOS 连接状态、设备身份、网关信息和本机边界完成 Windows 适配。Rust 只投影设备名、opaque Device ID、已验证网关 URL、workspace 数量、运行时和开发模式，绝对路径与 fingerprint 不跨 IPC；读取绑定 owner 与 authentication generation，Host 活性由真实进程状态复验。Windows 当前使用账号认证控制面与 Rust Local Agent Host，页面明确标记 macOS 设备签名长连接 Relay 不可用，不虚报能力；连接/断开复用唯一 Host lifecycle。React Domain/Port/Store/Adapter 和 Apple 主题页面已接入，定向 React 3 项、Rust 2 项、production build、类型检查、架构/Bundle 审计与 Clippy `-D warnings` 通过。第 27 页完成；下一片进入第 28 页设置 / 插件与 Skills。
86. [已完成] 第 28 页设置 / 插件与 Skills 已按 macOS Marketplace 与本机安装目录合并、名称/Skill/发布者/分类搜索、启停、卸载、安装助手、应用标签、版本、平台兼容、MCP/Skill 和权限声明语义完成 Windows 适配。Rust 并行读取真实 Marketplace 与 Host 安装记录，所有读取与 mutation 绑定 owner 和 authentication generation；启停同步设备级云端 preference 与全部 Host component，失败时尝试回滚，卸载先以 expected version 删除 Host 元数据、终止 Plugin runtime，再删除经 canonical confinement 验证的安装目录。React 只接收安全展示字段，绝对路径、可执行文件、参数、环境与 release digest 不跨 IPC；运行健康未实际握手时只显示“等待检查”，权限未检查时只显示“已声明”，不虚报 ready/granted。Windows 尚无与 macOS 等价的签名下载、受限解包、逐文件完整性、依赖隔离和安装回滚链，因此 Marketplace 安装/更新与手动导入保持 fail-closed，页面明确说明平台差异并保留安装助手入口。下一步进入 Windows 真机 package/install/smoke 验收。
87. [已完成] Windows package/install/smoke 已收口为可重复运行的真机 PowerShell 门禁：CI 构建 NSIS 后安装到全新临时目录，验证唯一主程序、Local Agent Host、Agent Skill 示例与宠物资源，比较安装后 sidecar 与构建时二进制的 SHA-256，启动真实安装版并等待应用自身完成 graceful exit，最后静默卸载并确认主程序及 sidecar 已移除。每次运行都会上传不含路径和 Secret 的结构化 JSON 报告；实际通过结论只由 `windows-latest` runner 产生，macOS 开发机不得伪报真机结果。

任何新的平台差异必须在本文件新增“差异原因、macOS 原行为、Windows 适配方式、测试与批准记录”后才能实现。
