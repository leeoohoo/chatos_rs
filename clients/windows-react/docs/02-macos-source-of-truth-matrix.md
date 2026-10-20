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
| 项目目录与文件操作 | macOS 目录、预览、搜索和 mutation 契约与 Windows workspace grant 边界已映射；Windows 项目工作面、相对路径目录树、文件读写与 lifecycle 已接入 | `ProjectWorkspaceView.swift`、`ProjectDirectoryView.swift`、`ProjectFileEditorView.swift` | `ProjectDirectoryViewModel.swift`、`ProjectDirectoryExpansionStateStore.swift` | `ProjectFilesystemModels.swift`、`ProjectFilesystemMutationUseCase.swift` | `NativeProjectFilesystemService.swift`、`NativeWorkspaceFilesystem.swift`；Windows Registry/workspace resolver + Tauri filesystem adapter | macOS mutation lifecycle、Windows Rust path/revision/mutation 与 React domain/store/UI 测试 | 目录、预览、搜索、编辑、新建和删除已进入 React；Git、项目设置仍禁止 |
| 远程连接列表 | 列表查询/排序用例、账号绑定 latest-wins lifecycle 与 await ownership 已提取 | `ResourceSidebar.swift` | `AppModelResources.swift`、`RemoteConnectionsLoadState.swift` | `RemoteConnectionModels.swift` | `NativeRemoteConnectionService.swift` | Load state/use case 及 Connector 测试 | 禁止；等待远程资源完整切片 |
| 插件应用列表 | 窄 listing Port、列表用例、账号绑定 latest-wins lifecycle 与 await ownership 已提取 | `PluginApplicationsView.swift`、`ResourceSidebar.swift` | `AppModelWorkspace.swift`、`PluginApplicationsLoadState.swift` | `LocalConnectorCatalogModels.swift` | `NativeLocalConnectorService+Plugins.swift` | Load state/use case 及 Connector 测试 | 禁止；等待插件应用完整切片 |
| Conversation 时间线与发送 | macOS 分层与三端共享 fixture 已完成；Windows History、Composer、显式能力、附件闭环和 runtime controls 已接入 | `ConversationTimelineView.swift`、`ComposerView.swift`、`ReasoningLevelControl.swift` | `ConversationSessionViewModel*.swift`、`ConversationSessionFactory.swift`、Conversation Application 类型 | `ConversationModels.swift`、`ConversationRuntimeSettings.swift`、Conversation ports | `NativeLocalAgentConversationService.swift`、`NativeLocalAgentConversationRuntimeSettingsService.swift`、Host conversation/attachment commands | Conversation Core policy/use case、附件导入、runtime settings load/mutation state、Session factory、ViewModel 测试等 | `macOS 分层完成，可进入 React`；Windows 核心发送闭环已实现 |
| Task Graph / Run Inspector | Graph、realtime、Run、section 刷新、Inspector request identity、Workspace 基础详情→section 顺序加载、内嵌 Inspector Task 复用/section polling 恢复/presentation/retry identity 与 Application use case 路由、polling session 与 Workspace presentation session 已提取；Windows 已接入基础详情、过程、Run、重试、停止、恢复和 polling | `MessageTaskWorkspaceSheet.swift`、`MessageTaskInspectorView.swift`、`TaskProcessTimelineView.swift` | `MessageTaskWorkspaceViewModel*.swift`、Task Graph/Run/Inspector Application 类型 | `MessageTaskRunModels.swift` 及 task graph models | `NativeLocalAgentMessageTaskGraphService.swift`、Host task/run/event commands | Core state/use case、ViewModel、Connector、共享 fixture、React latest-wins 与 Rust projection 测试 | `macOS 分层完成，可进入 React`；Windows 主检查器纵向切片已实现 |
| Ask User | 查询/提交/取消、排序投影、操作门禁、表单状态、refresh/per-prompt 异步生命周期与 activation/action recovery/focus refresh await ownership 已提取；Windows 已按同一语义接入安全投影、表单、提交、取消和失败对账 | `AskUserPromptCardView.swift`、Conversation 内嵌入口 | `ConversationSessionViewModel+AskUser.swift`、Ask User Application 类型 | `AskUserPromptModels.swift`；Windows `features/ask-user` Domain/Port/Store | `NativeLocalAgentAskUserPromptService.swift`、Host run/conversation commands；Windows Tauri typed commands | Ask User Core、Connector、ViewModel、React domain/store/UI 与 Rust projection/validation 测试 | `macOS 分层完成，可进入 React`；Windows 完整纵向切片已实现 |
| 工具审批 / 恢复 | Human Approval Port、加载/强类型决策用例、全局队列展示、监控 session、独立页面加载/决定 Task、Host durable/Human 两阶段端口与决定写回恢复 journal 已提取并通过干净快照验收；Windows 已接入 durable queue/单次决策、全局浮层与原决定恢复 journal | `GlobalApprovalOverlayHost`、完整审批页；Windows `GlobalToolApprovalOverlay` | `LocalConnectorControlCenterViewModel.swift`、Approval Application 类型；Windows `toolApprovalStore` | Local Connector approval models/ports；Windows `features/tool-approval` Domain/Port | platform tool worker、approval handler、Local Connector approval service；Windows Tauri typed commands/journal | Core use case/state、worker、approval handler/agent、state store、ViewModel tests；Windows projection/store/UI/journal tests | `macOS 分层完成，可进入 React`；Windows durable decision/recovery 已实现，真实 platform worker 完成前不发布对应执行能力 |
| Host lifecycle / Shell | Runtime ready、local-first launch、authenticated bootstrap retry、health/restart、service configuration 与 shutdown orchestration 均已进入 Application state/use cases；v44 health 契约已由 Rust/Swift/TypeScript 共享 fixture 冻结；React/Tauri 已接入双 Host control-plane bootstrap、Credential Manager 模型恢复、完整快照离线回退、最终 health ready gate 和附件 platform worker | `RootView.swift` 与各页面 ready gate | `AppModelAuthentication.swift`、Runtime/Bootstrap/Recovery state、Host launch/bootstrap/configuration/shutdown use cases | `LocalAgentHostLifecycle.swift`、Host launch/bootstrap/configuration/shutdown Ports | Native Host process、App composition adapters、Windows Tauri lifecycle/control-plane/attachment worker adapter | Runtime/bootstrap/recovery/launch/configuration/shutdown、共享 fixture、React gate、凭据环境净化、附件 vault/worker 与 Tauri tests | `macOS 分层完成，可进入 React`；Windows 只发布已真实执行的 `local_attachment_read` platform 能力 |
| Authentication | 四阶段 Gate、登录/注册、恢复重试、注册验证码倒计时、logout/login 排序与 latest-wins 已由 ViewModel 和稳定 Core Port 隔离；Windows 已按同一契约实现 | `AuthenticationGateView.swift`、`LoginView.swift` | `AuthenticationViewModel.swift` | `AuthenticationModels.swift`、`CredentialStore.swift` | `ChatOSAuthenticationService.swift`、`ChatOSAPIClient.swift`、`KeychainCredentialStore.swift`；Windows Tauri auth adapter + Credential Manager | `AuthenticationViewModelTests.swift`、Rust DTO/URL tests、React application state tests | `macOS 分层完成，可进入 React`；Windows 首切片已实现 |
| Pet 翻译 | 模型、历史、附件与翻译 operation identity 已提取；Host/磁盘等待不再强持有窗口 ViewModel | Pet 翻译浮窗 | `PetTranslationViewModel.swift` | `PetTranslationAgent` 请求/结果、历史记录模型 | Agent model provider、`PetTranslationHistoryStore` | Agent、历史持久化、ViewModel 生命周期测试 | 禁止；等待完整源码映射与 Port 验收 |
| Media / Story Studio | 账号 session、模型 generation、图片/视频/Story operation identity 已固定；历史、生成、保存、导入、Agent planning/runtime、批次与编辑等待不再强持有 ViewModel | AI 图片、AI 视频、AI 故事、AI 历史 | `MediaStudioViewModel.swift`、`StoryStudioViewModel*.swift` | Media/Story generation models/service | Media service、Story project store、provider/Agent adapters | Media 25 项、Story Studio 54 项及 Story Agent 回归 | 禁止；等待完整页面映射与 Port 验收 |
| Agent Group Chat | Team/Direct change stream 保持弱 UI 订阅；补充卡片、附件 hydration、account/communication scheduler drain 与 Run refresh 均已冻结请求上下文，存储/Agent 等待不再强持有 ViewModel | Agent 私聊、团队聊天与团队工作区 | `AgentDirectChatViewModel.swift`、`AgentGroupChatViewModel.swift` | Agent room/message/proposal/todo/run models | `SQLiteAgentGroupChatStore`、Local Agent scheduler | Codable、迁移、Store、Scheduler、UI presentation/performance tests | 禁止；等待完整页面状态与操作映射 |

以上“React 准入”不是进度建议，而是代码审查门禁。

### 2.1 项目工作区列表刷新

- `ClientOwnedWorkspaceLoader` 保持 macOS 的本地优先顺序：先只从 Project Registry 组合本地项目并立即发布，再解析设备身份、修复 root workspace binding，最后刷新远端联系人和 Conversation 关系；
- 未配对时本地项目仍可见；远端关系失败时保留最后可信联系人与 Conversation，不能用空响应覆盖；远端关系不得创建、重命名或复活项目；
- `WorkspaceLoadState` 将 owner、account generation 与 request identity 固定为一个 typed token，新刷新、项目删除、账号切换、登出与应用退出都会使旧 token 失效；
- 刷新 Task 在开始时冻结 Local Projects、Workspace Relations 与 Project Run 服务，各段 registry/Connector/Host await 不强持有 AppModel；每次快照准备、展示写回、错误和 loading 收尾都重新验收 token；
- `NativeProjectRunService` 的 project root 映射跟随每次已验收快照更新；UI 继续只消费 `WorkspaceResourceResolver` 输出，不直接读取 Connector/Host 状态。

Windows 只读索引实现（2026-10-20）：

- Tauri/Rust 中建立与 macOS 同 schema 的账号隔离 `Projects.sqlite3`，第一阶段只读取 `status = active` 的本地项目并立即返回；数据库损坏或字段无效时失败关闭，不伪装成空列表；
- 第二阶段通过账号绑定的 Host v44 `ListConversations` 分页读取关系，并按 macOS 规则保证“叽咕狸”和“插件安装助手”两个系统联系人会话存在；
- Host 中出现 project conversation 但本地 Registry 没有对应项目时，只保留关系记录，绝不凭远端关系创建或复活项目；
- React `WorkspaceIndexPort`、Application store 和 resolver 保留 local-first 顺序、账号切换 latest-wins、远端失败保留本地项目及显式错误/重试；
- Host 业务请求统一使用有界 framed transport。协议、IO、EOF 或超时会终止假健康进程；普通业务拒绝不会误杀健康 Host；
- 侧栏索引仍以本地 Registry 为权威；项目创建与显式文件夹授权已在独立切片通过后开放。导入、重命名、删除及其他 project context 变更仍须等待各自完整切片。

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
- 新增 5 项 `ProjectGitViewModelTests`，并复验 4 项 `NativeProjectGitServiceTests` 与 3 项项目文件 mutation lifecycle 测试。macOS Git 已达到 Windows 原生边界实施准入；Windows React 仍保持 Git 入口关闭，直到 owner/project/workspace 解析、Git 进程边界、typed Port/Store lifecycle 与 Apple 风格 UI 分别完成。

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
- 本次 React 准入仅覆盖“选中本地项目→复用/准备 Conversation→显式四态”。Conversation 时间线、发送、项目创建/编辑/删除仍按各自门禁禁止。

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

共享 Host v44 fixture 已固定 Conversation history、Message Task Graph 与最新 Run events，并由 Rust interface、Swift Connector、TypeScript Zod contract 同时读取验证；Windows 后续实现必须继续复用这些 fixture，并覆盖上述状态分支，不能只做组件快照测试。

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
- `clients/contracts/local-agent/v44` 已提供 canonical health request/success/failure fixture；Rust interface、Swift Connector 与 TypeScript 均读取同一份 fixture，固定协议版本 44、4 MiB frame、64 MiB message 和 `chatos-local-agent-host` service identity；
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
- Computer Use/UI Automation 与屏幕采集 worker 尚未准入，状态为 `missing_dependency` 且 `canRequest = false`。这代表产品能力缺失，不得伪装成用户尚未授权；
- 当前快照没有应用内可请求项；request command 对未知或不可请求 ID 失败关闭。将来只有原生层发布 `canRequest` 后，React 才显示授权动作；
- React Store 绑定 owner 与 Host availability，refresh/action 使用独立 request identity；短暂刷新失败保留最后可信快照，账号替换或 Host suspension 拒绝迟到结果；
- Windows 定向验收为 Rust 82 项通过、2 项 sidecar 环境测试忽略，React 31 个文件 114 项通过；TypeScript、Vite production build、架构审计和 `clippy -D warnings` 通过。

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

- 账号切换继续以 `sessionID` 隔离全部结果；模型目录使用独立 generation，图片与视频分别使用 operation ID，输入图片/视频首帧/参考音频继续使用 selection ID。
- 历史首页/分页、图片生成→本地保存、视频生成→进度→本地保存，以及所有媒体读取任务都先完成 adapter/磁盘工作，再以 weak ViewModel 恢复并联合验收 identity。
- 图片生成成功但本地保存失败时仍显示生成结果并给出历史错误；视频保存失败继续保留失败画布语义；取消视频必须拒绝服务端迟到完成。这些均为现有 macOS 行为，不得由 Windows 自行修改。
- Media Studio 25 项相关回归覆盖历史分页/损坏记录隔离、账号切换、图片引用、视频画布取消/重试及延迟生成释放。
- Story Studio 已完成项目首屏/分页、Agent history、media batch、图片/首尾帧/视频生成、进度恢复、末帧提取、顺序批次、Agent planning/runtime、编辑/导入/保存与镜头细化的 await ownership 审计；所有请求先冻结 Store/service/project snapshot，I/O 完成后才弱恢复 UI 并验收 identity。
- Story provider 图片/视频生成悬挂时 ViewModel 可释放；Story Studio 54 项与 Story Agent 相关测试通过。完整 UI/Port 映射与跨平台媒体能力差异仍未完成，因此 AI Studio 页面仍不具备 React 准入资格。

### 9.3 Agent Group Chat

- Team 与 Direct 的 change stream 在永久等待期间只通过 weak ViewModel 读取当前选择并记录刷新计划，不让不可见页面被事件流保活。
- Team workspace 的 proposals、todos、assets、dashboard、running Runs、附件数据等补充快照，以及 Direct chat 的 proposals、teams 与附件数据，都在 Task 启动前冻结 owner/room/project/store 和附件计划；SQLite/附件 await 期间不再强持有 ViewModel。
- 补充快照只有在任务未取消且当前 room identity 仍匹配时才整体写回；旧 room 的迟到结果不能清空或覆盖新 room。消息主体先展示、补充卡片失败仍解除 Direct timeline 首次定位等待的既有语义保持不变。
- Team/Direct 的 account drain 与 communication fast lane 在每一轮开始时冻结 scheduler、service、owner 和 room，Agent 执行与 delivery 查询期间只保留 weak UI；返回后按当前 room 再计算 `AgentSchedulerIssue`。全局 drain 仍会发布 room change 通知，不可见页面释放不会中断 durable scheduler 事实。
- 高频 Run refresh 现在冻结 run/room/owner/store 请求，在 SQLite 查询和 todo presentation 构建结束后一次性按 room identity 写回；同 Run 的新 refresh 会取消旧请求，旧请求不能移除或覆盖新任务。
- Agent Group Chat 相关 89 项契约、迁移、Store、Scheduler 与 UI 测试通过（2 项显式性能基线按环境变量跳过）。页面加载、发送、提案操作与导航差异仍需完整映射，因此对应 React 页面继续禁止实现。

## 10. 31 个页面的准入状态

| # | 页面 / 截图 | 状态 | React 准入 |
| --- | --- | --- | --- |
| 01 | 项目目录 | macOS 工作面、目录 list/expand/refresh/search/read/mutation、项目创建语义与 Windows workspace 路径边界已映射；Windows 目录树、原生文件夹授权、本地项目创建、预览/搜索和文件写操作已完成 | 目录树、项目创建/授权、预览/搜索、编辑、新建与删除已进入 React；Git 与项目设置禁止 |
| 02 | 项目消息 | macOS 分层与三端 fixture 已完成 | 允许；按 History→Composer→Realtime→Run Inspector 分片实施 |
| 03 | 项目设置 | 未映射 | 禁止 |
| 04 | 联系人聊天 | 源码映射中（复用 Conversation，入口差异待完成） | 禁止 |
| 05 | 应用 | 未映射 | 禁止 |
| 06 | AI 图片 | 源码映射中（generation/session 生命周期已收口） | 禁止 |
| 07 | AI 视频 | 源码映射中（progress/保存/取消生命周期已收口） | 禁止 |
| 08 | AI 故事 | 源码映射中（项目/历史/生成/批次/Agent/编辑生命周期已收口） | 禁止；等待完整 Port 与平台差异验收 |
| 09 | AI 历史 | 源码映射中（本地分页与账号隔离已收口） | 禁止 |
| 10 | Agent 管理 | 未映射 | 禁止 |
| 11 | Agent 文档 | 未映射 | 禁止 |
| 12 | Agent 团队概览 | 源码映射中（补充快照生命周期已收口） | 禁止 |
| 13 | Agent 团队聊天 | 源码映射中（change stream/附件 hydration 已收口） | 禁止 |
| 14 | Agent 团队任务 | 未映射 | 禁止 |
| 15 | Agent 团队调研 | 未映射 | 禁止 |
| 16 | Agent 团队资产 | 未映射 | 禁止 |
| 17 | Agent 团队运行 | 源码映射中（Run 模型复用关系待确认） | 禁止 |
| 18 | 需求调研 | 未映射 | 禁止 |
| 19 | 本地终端 | 未映射 | 禁止 |
| 20 | Notebook | 未映射 | 禁止 |
| 21 | 项目 Git | macOS View/ViewModel/Core Port/Native service/parser、命令边界和异步 lifecycle 已映射并完成准入；Windows 原生边界尚未实施 | 禁止进入 React，下一步先实现 Windows owner/project/workspace 与 Git 进程边界 |
| 22 | 设置 / 通用 | 未映射 | 禁止 |
| 23 | 设置 / 微信 | 未映射 | 禁止 |
| 24 | 设置 / 宠物 | 未映射 | 禁止 |
| 25 | 设置 / 全局工具 | 未映射 | 禁止 |
| 26 | 设置 / AI 模型 | 未映射 | 禁止 |
| 27 | 设置 / 设备网关 | 未映射 | 禁止 |
| 28 | 设置 / 插件与 Skills | 未映射 | 禁止 |
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
13. [部分完成] 第 01 页项目工作面、目录树、预览/搜索和文件写操作已按 workspace grant、规范化相对路径、5,000 项上限及异步 lifecycle 进入 React；Git 与项目设置仍未提前开放。
14. [已完成] 项目创建与 Windows 文件夹授权由原生选择器产生 owner/presentation-scoped opaque ticket，通过恢复 journal 收口 owner-scoped Connector state 与 Project Registry，并在账号/弹窗/选择器/保存 lifecycle 中拒绝迟到结果。
15. [已完成] 按 `ProjectDirectoryViewModel`、`ProjectFilesystemServicing.search/readFile`、`NativeWorkspaceFilesystem` 与 `ProjectFileEditorView` 完成项目文件内容预览和只读搜索；路径、大小、编码、二进制、搜索预算与迟到结果门禁均已进入 Rust/React 测试。
16. [已完成] `writeFile/createFile/createDirectory/deleteEntry` 已提取到 macOS Core mutation use case；Windows Tauri 已实现 owner/project/canonical path、symlink/reparse 拒绝、原子写入、内容 revision 冲突、create-new 和删除复验。React Port/Store 已冻结 mutation identity 与请求参数，拒绝文件/项目切换后的迟到写回，并以 Apple 风格编辑、新建和删除确认交互区分副作用失败与 committed-without-refresh；Git 与项目设置继续独立实施。
17. [macOS 准入已完成] 项目 Git 的 snapshot/diff/mutation 行为、进程与路径安全边界已完成源码映射；多步 mutation orchestration 已进入 Core use case，ViewModel load/diff/mutation identity 与 committed-without-refresh 已由 5 项生命周期测试覆盖。下一步实现 Windows 原生 Git 边界，React 入口继续关闭。

任何新的平台差异必须在本文件新增“差异原因、macOS 原行为、Windows 适配方式、测试与批准记录”后才能实现。
