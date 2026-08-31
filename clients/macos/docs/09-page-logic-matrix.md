# 页面真实逻辑矩阵

更新时间：2026-08-24

本文档是 SwiftUI 页面设计与实现的业务基线。任何设计稿进入评审前，都必须能从这里回答五个问题：页面由哪个真实组件负责、读取哪些接口、维护哪些状态、允许哪些操作、失败时怎样恢复。

## 证据规则

- “当前逻辑”只来自 `chatos_rs` 源码、已运行客户端的可访问界面和后端接口类型。
- `docs/design` 中的旧 SVG 不是需求来源。附件中“多源文件夹 / 主要目录”即属于旧概念稿，源码没有对应模型，已明确废弃。
- Apple 风格只改变布局、层级、控件和交互表达，不改变实体关系、权限边界和操作顺序。
- 设计新增能力必须单独标记为“建议新增”，不能混入“当前已支持”。
- 产品裁剪：新版不实现原项目的 `ApplicationsPanel` 和应用快捷方式入口；这项裁剪优先于“保留现有入口”的一般规则。

## 0. 全局信息架构

| 区域 | 当前源码/组件 | 真实职责 | SwiftUI 设计约束 |
| --- | --- | --- | --- |
| 顶部栏 | `chatInterface/HeaderBar.tsx` | 当前联系人或项目标题；原项目可打开记事本、应用列表、主题、用户菜单 | 使用原生 toolbar；保留记事本、主题、用户菜单，按产品决定移除应用列表；不能替换成虚构的全局 Dashboard 指标 |
| 左侧资源栏 | `SessionList.tsx`、`sessionList/Sections.tsx` | 四个可折叠资源分区：联系人、项目、终端、远端；各自有刷新、创建、选择、行内菜单 | 原生 `List` + `Section`；资源类型和顺序保持一致 |
| 主内容路由 | `ChatInterfaceMainContent.tsx` | 根据当前选择显示联系人会话、项目工作区、本地终端、远端终端或 SFTP | 不是固定三栏 IDE；每类资源有自己的工作面 |
| 全局浮层 | `ChatInterfaceOverlays.tsx` | 原项目包含记事本、应用列表、智能体管理、用户偏好、任务抽屉等 | 新版不实现应用列表；其余可改为 sheet、inspector 或独立 window，但功能边界不能合并丢失 |
| 项目二级导航 | `projectExplorer/WorkspaceTabs.tsx` | 项目目录、用户消息、Plan、项目设置 | 项目内固定四个工作面，不把 DAG 直接当作 Plan 首页 |

范围说明：本矩阵用于 macOS 客户端原生重写。浏览器 Web 端继续由现有 Web 工程维护；Browser MCP 与 Computer Use Plugin 本体也不在 Swift 重写范围内。

## 1. 登录与设备配对

| 项目 | 真实逻辑 |
| --- | --- |
| 主前端组件 | `AuthPanel.tsx` |
| Local Connector 当前壳 | `local_connector_client/frontend/src/main.tsx` 的 `ShellApp` / `SettingsApp` |
| 核心状态 | 未登录、登录中、Token 刷新、登录过期；Connector 未配置、等待连接、在线、开发模式 |
| 关键接口 | 主前端认证接口；Connector `/api/local/status`。旧壳通过一次性桌面票据完成配对 |
| SwiftUI 页面 | 主窗口原生登录；登录成功后直接进入全局资源壳。Connector 设置窗口展示设备配对和本机边界 |
| 禁止想当然 | 不把旧 `WebContentsView` 登录壳复刻到 Swift；不把用户名密码写入文档、fixture 或快照 |

## 2. 联系人与联系人会话

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `SessionList.tsx`、`sessionList/CreateContactModal.tsx`、`ChatConversationPane.tsx` |
| 数据接口 | `getContacts`、`createContact`、`deleteContact`、`getSessions`；联系人可配置 Task Runner Agent Account |
| 创建规则 | “添加联系人”实际从尚未成为联系人的可用 Agent 中选择；若全部已添加，提交按钮禁用 |
| 选择结果 | 打开联系人级会话，不带项目范围；项目联系人会话是另一条业务路径 |
| 页面状态 | 无联系人、刷新中、联系人已有会话、空会话、执行中、总结/上下文/复盘可用状态 |
| SwiftUI 页面 | 左侧保留联系人资源；主工作面使用同一 Conversation 组件，但明确显示“无项目范围”或当前项目范围 |

## 3. 项目创建

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `sessionList/CreateResourceModals.tsx`、`useSessionListActions.ts` |
| 关键接口 | `listLocalConnectorWorkspaces`、`listLocalConnectorDirectory`、`createLocalConnectorDirectory`、`createLocalConnectorProject` |
| 请求模型 | `CreateLocalConnectorProjectRequest`：单个 `device_id`、单个 `workspace_id`、可选单个 `relative_path`；项目名由所选路径推导 |
| 用户流程 | 选择 Local Connector 工作区 → 浏览目录 → 可在当前目录新建文件夹 → 选择当前目录 → 创建项目 → 自动选中新项目 → 后台刷新项目列表 |
| 页面状态 | Connector 工作区加载中/为空/离线；目录加载/为空/失败；未选工作区；创建中；创建失败 |
| SwiftUI 页面 | 一个原生 sheet，左上选择设备工作区，中部目录浏览器，底部显示推导出的项目名和最终路径 |
| 明确不支持 | 不支持多源目录，不存在“主要目录”，当前 UI 也不要求创建时选择联系人 |

## 4. 项目目录

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `ProjectExplorerFilesWorkspace.tsx`、`TreePane.tsx`、`PreviewPane.tsx`、`projectExplorer/git/*` |
| 文件接口 | `listFsEntries`、`searchFsContent`、`readFsFile`、`createFsDirectory`、`createFsFile`、`writeFsFile`、`moveFsEntry`、`deleteFsEntry`、`openFsPathExternally` |
| Git 能力 | 状态、分支、Diff、提交、比较等通过项目顶部 Git 下拉和相关弹层进入，不是永久 Changes Inspector |
| 工作面结构 | 左：项目信息、Connector 提示、全文搜索、文件树；右：文件预览/编辑。树宽可调整 |
| 搜索状态 | 区分大小写、全词匹配、结果截断、上/下一个命中、命中锚点 |
| 文件状态 | 未选择、目录不可预览、加载、文本、Markdown 预览、二进制、可编辑、只读、未保存、保存失败 |
| 其他操作 | 拖拽移动、冲突覆盖/重命名、右键创建/下载/复制路径/.gitignore/默认程序/Finder/VS Code/删除 |
| SwiftUI 页面 | 保留“树 + 预览/编辑”两区结构；Git 用 toolbar menu/sheet；只有发生选中任务或 Git 比较时才打开 inspector |

## 5. 项目用户消息与聊天历史（P0）

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `ConversationUserMessagesSidebar.tsx`、`useConversationUserMessages.ts`、`MessageList.tsx`、`useMessageListWindowing.ts` |
| Turn 列表接口 | `getConversationUserMessageTurns` / compact history；按 `before` 游标分页 |
| Turn 详情接口 | `getConversationTurnMessagesByTurn`，旧数据回退到 `getConversationTurnMessages(userMessageId)` |
| 活跃任务接口 | `getConversationTaskRunnerActiveMessageTasks`，并结合 live message metadata 合并运行状态 |
| Turn 语义 | 一个用户 Turn 可包含用户消息、最终 Assistant 消息、过程消息统计、多个 Task Runner callback；不能按“一问一答两条消息”建模 |
| 页面结构 | 单一全宽时间线，不再复制左侧 Turn 列表；加载更早消息与新消息提示都在时间线内完成 |
| 任务入口 | 仅真实包含任务运行的 Turn 显示任务卡片；点击“查看任务”后按 `turn_id` 加载并展示任务/工具/推理节点 |
| 消息级入口 | 任务回调消息保留“查看过程 / 查看详情 / 任务图”；过程来自 Turn messages，详情与任务图来自 Task Runner message graph/task/run 接口 |
| 任务图源关联 | 历史消息必须保留 `task_runner_async.source_user_message_id` 与 `source_turn_id`；查询图时优先使用这组 Task Runner 源标识，不能一律把当前聊天消息 ID 当成任务批次 ID |
| 已取消任务 | `task_runner_async.event = task.cancelled/task.canceled` 或状态为 cancelled/canceled 的回调从聊天最终回复、过程列表和过程计数中排除；失败与阻塞回调继续显示 |
| 多任务语义 | 一条用户消息可由 AI 通过 Task Runner MCP 创建一个或多个任务节点；每个节点都可独立查看过程、详情、Run 和阻塞处理 |
| 规划模式 | 仅项目会话可开启；更新 `/conversations/{id}/runtime-settings`，发送 `/agent/chat/send` 时携带 `plan_mode`，开启后先规划并等待确认执行 |
| 规划确认 | 从消息 `project_requirement_execution` / `task_runner_async` 元数据恢复项目、需求、执行批次和确认状态；完整 DAG 且节点尚无 `last_run_id` 时允许确认 |
| 确认执行 | `POST /projects/{projectId}/requirements/{requirementId}/confirm-execution`；Body 为 `execution_group_id`、`conversation_id` 和可选 `contact_id` |
| 放弃计划 | `POST /projects/{projectId}/requirements/{requirementId}/stop`；在相同执行标识基础上明确发送 `discard_tasks: true`，UI 先显示破坏性确认框 |
| Composer | 模型、附件、Task Plugin 偏好、规划模式、推理开关；执行中仍可发送 runtime guidance |
| 用户可见状态 | 加载更早消息、inline retry、离线/重连、发送失败、任务状态、离开底部后的新消息按钮 |
| 禁止展示 | TurnID、cursor、revision、generation、缓存数量、加载范围、锚点与偏移、Realtime/HTTP merge 等内部诊断字段 |
| 当前实现风险 | 首屏页小、HTTP 与 realtime 竞态、历史页被刷新覆盖、会话切换清空、滚动依赖多组 RAF 和估算行高 |
| SwiftUI 权威模型 | 以 `ConversationHistoryStore` actor 管理 per-session Turn、message fragment、callback、cursor、generation 和 in-flight 请求；SQLite 先显示缓存，网络只 merge 不 replace |
| 滚动约束 | 旧页 prepend 保持可见锚点；用户离开底部后不得抢滚动；新消息用提示按钮；切换会话恢复独立锚点 |

详细状态机和验收矩阵见 [聊天历史专项架构](./06-chat-history-architecture.md)。

## 6. Plan / Requirement

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `ProjectPlanPane.tsx`、`PlanRequirementColumns.tsx`、`PlanRequirementDetail.tsx` |
| 初始接口 | `getProjectPlan(projectId, includeWorkItems: false)`；任务和技术文档按选中 Requirement 延迟加载 |
| 左侧结构 | Requirement 按层级展开为多列；顶部统计需求数、完成任务数、阻塞任务数 |
| Requirement 卡片 | 标题、状态、类型、优先级、任务数、前置/后续/子需求、摘要 |
| 右侧详情 Tab | 需求、技术文档、任务 |
| 需求内容 | 需求关系、执行范围、摘要、详细说明、业务价值、验收标准 |
| 任务内容 | 前置关系排序、未完成数、状态、依赖；大列表增量渲染 |
| 执行入口 | 预览流程；打开执行工作台；恢复已有执行计划/过程；冲突时尝试读取未结束批次 |
| 执行工作台源码 | `RequirementExecutionStartingModal.tsx`、`RequirementExecutionProcessModal.tsx`、`RequirementExecutionProcessView.tsx`、`MessageTaskGraphPanel.tsx` |
| 执行工作台结构 | 左侧显示规划阶段、详细过程入口与重新规划反馈；右侧主工作面必须是实时任务 DAG；底部根据阶段显示执行、暂停后续任务、取消、重试或重新生成 |
| 关键接口 | `listProjectRequirementWorkItems`、`listProjectRequirementDocuments`、`executeProjectRequirement`、`getProjectRequirementExecutionPlan`、confirm/pause/resume/stop/rerun |
| SwiftUI 页面 | 默认仍是 Requirement 浏览与详情；DAG 只出现在“预览流程”或“执行工作台”，不能占据 Plan 首页；执行工作台不能退化成线性任务列表 |

## 7. Requirement 执行工作台与任务流程图

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `RequirementExecutionProcessModal.tsx`、`MessageTaskDrawer.tsx`、`MessageTaskGraphPanel.tsx` |
| 图数据 | 消息级任务、Run、prerequisite 边、context 非阻塞边；支持 Project Task 阶段合并和传递约简 |
| 图模式 | 精简图/完整图；当前消息、直接前置、间接前置；聚焦节点时保留上下游并弱化无关节点 |
| 运行表达 | 运行节点和边动画；顶部显示当前任务数、展开前置数、依赖连线数 |
| 节点动作 | 查看执行过程、任务详情、处理阻塞、Run 详情；可重试、集成重试或豁免等 |
| 画布操作 | 缩放、适应窗口、清除聚焦 |
| SwiftUI 页面 | `Canvas` 绘边 + SwiftUI 节点；右侧 inspector 展示选中节点；另提供无障碍大纲列表 |

详细图语义见 [任务流程图专项架构](./08-task-graph-architecture.md)。

## 8. 项目设置（实际为运行设置）

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `ProjectRunSettingsPanel.tsx`、`RunEnvironmentDetails.tsx` |
| 初始化接口 | `analyzeProjectRun`、`getProjectRunCatalog`、`getProjectRunState`、`getProjectRunEnvironment` |
| 顶部状态 | 项目路径、运行状态、检测到的目标数、语言、错误/消息/最近退出诊断 |
| 运行前检查 | 当前目标问题、其他目标问题、缺失文件/环境/工具链及修复建议 |
| 运行目标 | 下拉选择检测到的 target；显示来源、cwd、entrypoint、manifest、最终命令 |
| 实例操作 | 启动新实例、停止当前、重启、删除、刷新；选择运行实例 |
| 运行终端 | 项目设置中显示独立运行终端；环境和工具链为可折叠详情 |
| 环境编辑 | 自定义工具链、配置文件和环境变量；保存后重新分析 |
| SwiftUI 页面 | 设计成“运行控制台”，不是通用项目偏好，也不是 Local Connector 全局终端页 |

## 9. 本地终端

| 项目 | 真实逻辑 |
| --- | --- |
| 创建 | 与项目创建共用目录选择器：选择一个 Connector 工作区和 cwd，名称由路径推导 |
| 接口 | `createTerminal`、`getTerminal`、`interruptTerminal`、`deleteTerminal`、`listTerminalLogs` |
| 主工作面 | `EmbeddedTerminalView.tsx` 与 `terminal/*` 管理 websocket、输入、viewport、历史、状态和主题 |
| 状态 | 连接中、就绪、命令运行、stdin 可写、断线、重连、已结束、日志分页 |
| 已有增强能力 | Xterm resize、snapshot/日志历史分页、命令解析与命令历史、per-terminal 缓存、主题同步、重新连接 |
| SwiftUI 页面 | 终端属于左侧“终端”资源；项目运行终端属于项目设置，两者不能混成一个 Runtime Dashboard |
| 原生布局 | 跟随系统主题的全宽 PTY viewport + Terminal Tab + 精简 Toolbar + 底部细状态栏；不使用大状态卡或命令历史侧栏 |
| 明确裁剪 | 不迁移 `TerminalCommandHistoryPanel` 的独立右侧面板；保留 shell history、终端输出 snapshot/日志分页和系统级搜索能力 |
| 交互底线 | 文本选择、复制粘贴、IME、ANSI、搜索、链接/路径识别、resize 和快捷键至少达到 macOS Terminal；删除进入菜单，运行中才显示中断 |

## 10. 远端连接、远端终端与 SFTP

| 项目 | 真实逻辑 |
| --- | --- |
| 创建组件 | `RemoteConnectionModal.tsx` |
| 必填字段 | host、port、username；可选名称、默认远端目录 |
| 安全字段 | 主机校验策略；认证支持私钥、密码等分支；私钥/证书文件从选定 Connector 设备读取 |
| 跳板机 | 可启用；可复用已有远端连接或填写独立 jump host 凭据 |
| 操作 | 测试连接、验证码/二次验证、创建/编辑、快速测试、删除、打开终端、打开 SFTP |
| SFTP 接口 | 列目录、上传、下载、传输状态、取消、建目录、重命名、删除 |
| SwiftUI 页面 | 创建 sheet 使用分组 Form；远端资源行提供 Terminal/SFTP 两个明确入口 |

## 11. 记事本

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `NotepadPanel.tsx`、`notepad/*` |
| 接口 | init、文件夹列表/创建/重命名/递归删除；笔记列表/创建/读取/更新/删除；标签和搜索 |
| 左侧 | 文件夹树、展开状态、标题/文件夹搜索、新建文件夹、新建笔记、上下文菜单 |
| 右侧 | 编辑、预览、分栏、刷新、复制文本、复制为 Markdown、保存、删除；标题、标签、内容和 dirty 状态 |
| SwiftUI 页面 | 使用独立可复用窗口，保持“文件夹/笔记导航 + 文档编辑器”结构；标题直接作为文档标题，标签进入轻量元信息栏或 Inspector |
| 视觉约束 | 不携带主资源侧栏，不放在居中大卡片里，不把标题、标签和正文全部画成表单输入框；编辑/预览/分栏/复制进入 Toolbar |

## 12. 明确裁剪：应用列表

原项目的 `ApplicationsPanel.tsx`、`Application` 实体及其创建、编辑、删除接口不进入 Swift 版范围。主 Toolbar 不提供应用列表入口，也不生成对应页面设计稿。Local Connector 的 Plugin Marketplace 是独立能力，继续保留。

## 13. 智能体管理

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `AgentManager.tsx`、`agentManager/*` |
| 列表信息 | 名称、启用状态、分类、描述、插件数、技能数 |
| 创建字段 | 名称、分类、描述、角色定义、启用 |
| 操作 | 新建、AI 创建、编辑、删除 |
| 关系 | Agent 可成为联系人；联系人是用户侧会话入口，不应与 Agent 配置合并成同一列表 |

## 14. 系统上下文

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `SystemContextEditor.tsx`、`systemContextEditor/*` |
| 左侧 | 提示词列表、搜索、新建、选择、删除 |
| 右侧 | 名称、AI 场景、风格、语言、输出格式、约束、禁止项、优化目标、提示词正文 |
| AI 操作 | AI 生成、优化、评估；使用选择的云端模型配置 |
| 接口 | list/create/update/delete/activate System Context；generate/optimize/evaluate draft |
| SwiftUI 页面 | 独立全屏工作区；不能缩成设置首页的一张入口卡片 |

## 15. 用户偏好

| 项目 | 真实逻辑 |
| --- | --- |
| 源码组件 | `UserSettingsPanel.tsx`、`settings/*` |
| Tab | 常规设置、云端 AI |
| 常规设置 | 界面语言、内部上下文语言；说明不会改写用户内容、工具输出和压缩记忆原文 |
| 云端 AI | 云端默认模型与 Task 模型设置，属于账号配置，不属于 Local Connector 本机模型页 |
| 接口 | `getUserSettings`、`updateUserSettings` 以及云端模型配置接口 |
| SwiftUI 页面 | 使用独立 Settings Window；左侧为“常规 / 云端 AI”，右侧为连续 Form，不携带主资源侧栏 |
| 视觉约束 | 不放在灰色大画布中的居中巨型卡片里，不使用卡片套卡片 |

## 15A. Plugin 实时画面与画中画

| 项目 | 真实逻辑 |
| --- | --- |
| 客户端组件 | `local_connector_client/frontend/src/components/PluginVisualPreview.tsx`、`styles-visual-preview.css` |
| 数据接口 | `api.pluginRuntimeVisualSession()`；Core `plugin_visual_sessions.rs` 当前从 Plugin `visual-sessions` 目录选择全局最新有效会话 |
| 会话字段 | `session_id`、`adapter_session_id`、`plugin_id`、`component_key`、`title`、`target_app`、`status`、`frame_sequence`、`captured_at`、尺寸和可选帧 |
| 有效性 | 仅接受 `running`；帧只允许 JPEG/PNG、最大 2 MiB；超过 15 秒或时间异常即失效；界面当前约每 450 ms 刷新 |
| 展开状态 | 新 `session_id` 自动展开；标题、实时点、目标应用、帧、Plugin 标识和“仅在本机显示” |
| 收起状态 | 168×44 的实时按钮，点击恢复；展开约 376×276，右下角 18 pt 边距 |
| 等待/结束 | 没有首帧时显示“正在建立隔离画面”；没有有效 session 时完全隐藏 |
| 当前缺口 | 返回模型没有 conversation/turn/task run 归属；Swift 协议必须通过 `adapter_session_id` 增加或解析归属，不能照搬“全局最新会话” |
| 可见规则 | 只在当前选中的 conversation 与 visual session owner 匹配时显示；同会话内切换聊天/过程/详情不消失，切到其他会话只隐藏不取消执行 |
| 窗口层级 | 浮层在客户端主工作区之上；命令/权限审批浮层在它之上；不能被消息滚动裁剪 |
| Swift 范围 | 只实现通用原生 `VisualSessionOverlay` 和本机数据消费；不重写 Computer Use、Browser MCP、Chrome 或浏览器 Web 端 |
| Browser MCP 现状 | 平台迁移文档要求 Plugin 提供结构化页面状态、截图和流式 frame；Swift 客户端只依赖统一 visual-session 契约，不绑定 Browser MCP 内部实现 |
| 聊天历史约束 | Visual session 状态与 `ConversationHistoryStore` 分离，不插入或替换消息数组，不改变会话滚动锚点 |

## 16. Local Connector 设置

### 16.1 设备配对

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `ConnectionPanels.tsx` |
| 信息 | Core 地址、在线状态、用户、设备名、Device ID、本机边界、文件路由、运行方式 |
| 操作 | 刷新、退出本机配对 |
| 关键事实 | 当前产品明确写明“无需另行登记目录”；文件、终端和权限在当前设备执行 |

### 16.2 Plugin Marketplace

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `plugins/PluginMarketplacePanel.tsx`、`PluginDetailDrawer.tsx` |
| 数据 | Catalog、已安装、可更新；公开/个人范围；分类、搜索、publisher、release、签名、组件、权限 |
| 操作 | 安装、更新检查、事务恢复、启停组件、自动更新、回滚、卸载、权限授权、OAuth、文件 grant |
| 接口 | `/api/local/plugins/*` 系列；状态事件长轮询 |

### 16.3 本机终端与命令历史

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `TerminalPanel.tsx` |
| 页面 | 命令/args 测试执行；输出；按来源筛选命令历史；展开单条记录；清空历史 |
| 接口 | `/api/local/terminal/exec`、`/api/local/commands` |

### 16.4 模型配置

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `ModelConfigPanel.tsx` |
| 云端模型 | 只读同步副本，展示启用和凭据同步状态；供应商、凭据和运行参数不在本机编辑 |
| 本机可改 | 模型请求最大重试次数、命令审批模型、审批 Thinking |
| 接口 | `/api/local/model-configs`、refresh、`/api/local/model-settings` |

### 16.5 命令审批

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `ApprovalPanel.tsx`、`GlobalApprovalTray.tsx` |
| 模式 | 请求审批、AI 自动审批、从不询问 |
| 内容 | 待审批/正在审查；按项目分组白名单；分页审批历史；命令和 Plugin/Computer Use 操作审计 |
| 决策 | 允许、拒绝、可记住允许；专用确认响应；风险确认 |
| 接口 | `/api/local/approval/settings`、pending、approve、deny |

### 16.6 运行与系统权限

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `RuntimeSettingsPanel.tsx`、`systemPermissions.ts` |
| 运行配置 | 开发者模式及 ChatOS/Connector/User Service/MinIO 端点；切换时断开旧环境 relay |
| 本机审批 Agent | Prompt 与能力策略版本、检查更新、更新 |
| 系统权限 | 本地目录、终端、HTTPS、辅助功能、屏幕录制、Office 自动化；插件权限与 Connector 自身权限分开 |

### 16.7 权限控制

| 项目 | 真实逻辑 |
| --- | --- |
| 组件 | `SandboxPanel.tsx`、`SandboxPolicySettings.tsx` |
| 默认策略 | 本机进程隔离；文件仅授权项目；网络默认关闭；AI 可审批联网和项目外临时访问 |
| 高级信息 | capability、settings、当前运行 lease；开启后每 6 秒刷新 lease |
| 接口 | `/api/local/sandbox/capabilities`、settings、leases |

## 17. 设计评审 Gate

每张页面稿必须附带以下检查结果：

1. 页面名称与真实路由一致。
2. 主要实体能在本矩阵或接口类型中找到。
3. 主操作与现有操作顺序一致。
4. 空、加载、失败、离线、权限不足和运行中状态已覆盖。
5. 没有把 Plugins、Agents、Contacts、Cloud Models、Local Approval Model 混为同一实体；已裁剪的 Applications 不得重新出现在入口或页面中。
6. 没有把 Project Plan、Requirement 执行工作台和消息任务 DAG 混为同一页面。
7. 聊天历史使用稳定 Turn 身份和 merge 语义，不使用“整体替换消息数组”。
8. Local Connector 页面不嵌入主前端；主前端也不依赖 Connector 设置页提供导航。
9. 设置页没有使用居中巨型卡片，也没有无意义的大面积空白。
10. Plugin 实时画面不参与消息列表布局；审批层级高于画中画；可见性由当前会话归属决定，而不是由进入某个专用页面决定。
