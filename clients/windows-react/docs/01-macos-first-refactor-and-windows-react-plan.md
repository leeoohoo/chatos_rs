# ChatOS 3.0.9：macOS 深度重构与 Windows React 客户端实施方案

> 状态：执行中（阶段 0–3 并行；Host lifecycle 技术样板已落地）
> 日期：2026-10-20
> 适用分支：`3.0.9.2` 及后续开发分支
> 唯一产品基准：当前实际运行的 macOS 客户端
> 源码事实矩阵：[`02-macos-source-of-truth-matrix.md`](./02-macos-source-of-truth-matrix.md)

## 1. 决策与边界

本方案取代此前所有以旧 Windows C# 客户端为基础的实现方案、补齐计划、能力矩阵和验收口径。

以下约束不可变更：

1. `clients/windows` 的代码、架构、页面、交互和文档均不是参考资料，实施时整体删除。
2. Windows 新客户端从零建设于 `clients/windows-react`，不从旧 C# 工程复制代码或设计。
3. macOS 当前真实运行行为是产品事实源；仓库截图和源码用于补充、解释和自动验证。
4. 先完成 macOS 的 UI/业务逻辑分层和跨平台契约，再实现 Windows React，避免复制当前耦合。
5. 可复用、与操作系统无关的业务能力优先归入 Rust Local Agent Host；平台能力保留在原生适配层。
6. React UI 不得直接访问 SQLite、密钥、文件系统、进程、PTY 或通用 Shell。
7. 不迁移旧 Windows 的 WFP、NetworkGuard、驱动、服务或安装器；如未来确认需要，单独立项并从零设计。
8. **所有产品自有 UI 必须统一采用苹果主题与设计语言**；包括 macOS 与 Windows React 的主窗口、导航、页面、弹窗、菜单、表单、卡片、状态、动效和反馈，不允许混入 Fluent、Material、WinUI 默认视觉或普通后台模板风格。
9. **所有具体实现逻辑必须逐项来源于 macOS 当前源码、测试及其调用的 Rust Host 契约**；截图只用于视觉核对，禁止根据页面外观自行发明业务流程、状态机、数据结构、默认值、错误恢复或交互语义。

本方案只定义新的实施路线，不以旧 Windows 的“已有能力”决定范围或优先级。

## 2. 产品事实源

功能与视觉判断按以下优先级处理：

1. **当前运行的 macOS 客户端**：实际页面、状态、交互和异常表现。
2. **macOS 当前源码与测试**：确认业务规则、数据来源、生命周期和边界条件。
3. **macOS 界面截图**：`clients/macos/docs/screenshots` 中的 31 个一级界面快照。
4. **macOS 设计与架构文档**：仅在与当前运行结果一致时有效。

旧 `clients/windows` 不出现在事实源列表中。

在开始重构每个页面前，必须形成该页面的“行为基线包”：

- 当前运行界面的整页截图及关键弹窗/菜单截图；
- 页面入口、导航关系、键盘行为和窗口行为；
- 加载、空状态、成功、失败、离线、权限不足和恢复状态；
- 输入、输出、任务执行、审批、取消和重试流程；
- 页面依赖的用例、Host 命令、原生能力和云端接口；
- macOS 对应 View、Feature Model、Domain/Port、Connector adapter、Host command 和测试文件的精确源码映射；
- 可自动化的验收步骤与稳定测试数据。

如果截图、文档与运行行为冲突，以运行行为和实际源码为准，并更新基线资料。

没有完成源码映射的页面不得进入 React 实现。Windows 端需要平台差异时，必须先保留 macOS 的业务语义，再把差异限制在 Tauri 平台适配器内，并在差异登记中说明原因和验收方式。

## 3. 当前系统判断

### 3.1 macOS

当前 macOS 客户端已经具备完整产品形态，但展示层与实现层耦合较深：

- `ChatOSApp` 约 260 个 Swift 文件、69,969 行；
- `ChatOSConnector` 约 227 个 Swift 文件、61,795 行；
- `ChatOSApp` 中有 51 个文件直接 `import ChatOSConnector`；
- 大量页面同时承担布局、状态、数据拼装、副作用调用和错误恢复；
- 平台能力、Local Agent Host 接入、云端接口与 UI 状态的边界尚未完全稳定。

因此不能直接把 Swift 页面逐文件翻译成 React。先建立用例和端口，再迁移页面，才能让两个客户端共享一致行为。

### 3.2 Rust Local Agent Host

`clients/local_agent_host` 已有约 61,014 行 Rust 代码，并具备清晰的 `interface / application / ports / infrastructure / profiles` 分层。它已经适合作为跨平台本地业务后端的基础，不需要另建一套 Node.js 后端。

现有 Host 已覆盖或正在承载：

- Conversation、Turn、Message、Run 与 Task Graph；
- 本地 SQLite 事实存储和恢复；
- 模型与能力快照；
- 插件/MCP 生命周期；
- 工具审批和外部 Tool Worker；
- 事件等待、任务检查器和账号隔离；
- macOS/Windows 可用的 stdio、Unix socket、named pipe 传输基础。

后续重点是补齐稳定的客户端契约和平台端口，而不是重写 Host。

### 3.3 旧 Windows

旧 C# 客户端不做质量修复、不做功能盘点、不做迁移映射。它只进入删除清单：

- `clients/windows` 全目录；
- 只服务旧 Windows 的构建脚本、CI、解决方案引用和发布配置；
- 仓库中以旧 Windows 为对象的同步清单、验收文档和架构说明；
- 旧安装包、驱动、服务、证书和发布任务。

删除前只做“引用扫描”，目的是避免删后构建残留，不是读取其实现作为参考。

## 4. 目标架构

```text
macOS SwiftUI                         Windows React
┌──────────────────────┐              ┌────────────────────────┐
│ View / Components    │              │ React View / Components│
│ Feature State        │              │ Feature Store / Query  │
│ Application UseCases │              │ Application UseCases   │
└──────────┬───────────┘              └───────────┬────────────┘
           │ typed ports                            │ typed bridge
┌──────────▼───────────┐              ┌───────────▼────────────┐
│ macOS adapters       │              │ Tauri commands/adapters│
│ Swift native APIs    │              │ Rust + Windows APIs    │
└──────────┬───────────┘              └───────────┬────────────┘
           └──────────────┬────────────────────────┘
                          │ versioned Host protocol
               ┌──────────▼──────────┐
               │ Rust Local Agent Host│
               │ domain/application   │
               │ SQLite/MCP/model/run │
               └──────────────────────┘
```

依赖方向必须保持单向：

```text
UI → Feature State → Application Use Case → Port → Adapter
```

禁止反向依赖，也禁止 UI 绕过 Use Case 直接调用 Adapter。

### 4.1 跨平台 Rust Host 负责

- Conversation、Message、Turn、Run、Task 和 Task Graph 状态机；
- 本地事实存储、事务、幂等、分页、恢复和事件流；
- 模型配置、能力目录和账号边界；
- Plugin/MCP 安装记录、进程协议和工具调用编排；
- 审批状态机、任务取消/重试和崩溃恢复；
- 与平台无关的项目元数据、Git 用例和文件操作策略；
- 跨平台 DTO、错误码、协议版本和兼容性规则。

### 4.2 macOS 原生层负责

- SwiftUI/AppKit 窗口、菜单栏、Dock、快捷键和拖放；
- Keychain、系统文件选择器、通知、权限请求；
- ScreenCaptureKit 截图、长截图、标注和快速翻译入口；
- Accessibility、AVFoundation、WebKit 等 Apple 平台能力；
- PTY 与 SwiftTerm 的显示适配；
- Rust Host 子进程生命周期和 typed client adapter。

### 4.3 Windows Tauri 原生层负责

- Tauri 2 窗口、托盘、系统通知、文件选择器和深链；
- Windows Credential Manager/Password Vault；
- Rust Host sidecar 生命周期、健康检查、重启和账号切换；
- ConPTY 会话和 xterm.js 所需的有界事件桥；
- Windows 权限、路径、进程、协议注册与安装更新；
- 仅暴露白名单 typed command，不暴露任意文件、进程或 Shell API。

### 4.4 客户端不应负责

- 复制 Host 的任务或会话状态机；
- 自己查询或修改 Host SQLite；
- 将 API key、Token 或插件 Secret 放入前端状态或日志；
- 用 UI loading 状态代替持久化任务状态；
- 使用通用 `invoke(command, args)` 承载未经约束的本机能力。

## 5. macOS 重构设计

### 5.1 目标模块

在现有 Swift Package 中渐进形成以下边界，先拆源码目录和 Target，稳定后再考虑独立 Package：

```text
ChatOSApp
├── AppShell                 # Scene、Window、导航与依赖装配
├── Features
│   ├── Conversations
│   ├── Projects
│   ├── Requirements
│   ├── Tasks
│   ├── Agents
│   ├── Creation
│   ├── Terminal
│   ├── Notebook
│   └── Settings
├── Application             # Use Case、Feature input/output
├── Domain                  # UI 使用的稳定实体和值对象
└── DesignSystem            # Token、基础组件、状态组件

ChatOSConnector
├── LocalAgentAdapter       # Rust Host typed client
├── CloudAdapter            # 保留的云端能力
├── PersistenceAdapter
├── PluginAdapter
└── PlatformAdapters        # Apple 原生能力
```

最终约束：

- SwiftUI `View` 只依赖 Feature Model、Domain Model 和 Design System；
- Feature Model 只调用 Application Use Case；
- Use Case 只依赖 protocol；
- `ChatOSConnector` 只实现 protocol，不被页面文件直接导入；
- App 入口是唯一 composition root；
- 跨页面共享状态必须有明确所有者，不放入无边界单例；
- 所有持续任务必须有取消、超时、错误和账号切换语义。

### 5.2 渐进式迁移方式

不做一次性“大爆炸”重写。每个垂直切片按同一流程迁移：

1. 从当前运行界面采集行为基线；
2. 阅读并登记该页面现有 View、ViewModel、Domain、Connector、Host 和测试的完整调用链；
3. 从该调用链提取 Domain Model、Use Case、Port、状态机和错误模型，不从截图反推逻辑；
4. 用现有 Connector 实现 Adapter；
5. 将 ViewModel 从具体服务迁移到 Use Case；
6. 让 View 去除 IO、持久化和进程控制；
7. 补充单元、契约、快照和端到端测试；
8. 对照 macOS 真实客户端与源码逐状态验收；
9. 删除该切片的旧入口和兼容代码。

迁移顺序优先选择公共依赖最多、能建立模板的切片：

1. AppShell、账号与 Host 生命周期；
2. 项目/联系人导航与 Conversation；
3. Run、Task Graph、执行过程、审批与恢复；
4. 项目目录、Git、Notebook；
5. Terminal；
6. Agent、团队、需求调研；
7. AI 创作；
8. 设置与全局工具；
9. 截图、宠物、菜单栏等平台特性。

## 6. 跨平台契约

### 6.1 契约形式

建立单一版本化协议来源，至少包含：

- Host command/request/response DTO；
- 事件 envelope、游标、分页和增量更新语义；
- 结构化错误码及可恢复性；
- capability/feature flags；
- 协议版本、最低兼容版本和升级规则；
- Rust、Swift、TypeScript 共用的 JSON fixture；
- Secret、文件内容和大对象的禁止字段规则。

优先从 Rust interface DTO 生成 JSON Schema，再生成或校验 Swift/TypeScript 类型。无法生成的类型必须通过 fixture 契约测试保证一致。

### 6.2 Host 生命周期

两个客户端遵循同一状态机：

```text
stopped
  → launching
  → handshaking
  → synchronizing
  → ready
  → degraded / restarting
  → stopped
```

UI 只能在 `ready` 后发业务命令。启动阶段允许展示应用 Shell 和骨架，但必须将 Host 未就绪显示为明确状态，不能产生红色错误闪现、无限 loading 或“进程存活但不能回包”的假健康。

必须具备：

- 启动握手和协议版本检查；
- 请求超时、取消和 correlation id；
- 单次响应超过 64 MiB 逻辑消息上限时返回有界 `response_too_large` 错误并保持健康传输，调用方必须改用更小分页、精简 payload 或 Artifact 引用；
- reader/writer 任一退出即终止整条传输并回收 Host；
- 重启后的 Conversation/Run/Task 恢复；
- 账号切换时先终止旧账号 Host，再启动新账号 Host；
- 有界日志与匿名诊断包，默认脱敏。

## 7. Windows React 技术方案

### 7.1 技术栈

- 桌面宿主：Tauri 2；
- UI：React 19 + TypeScript + Vite；
- 路由：React Router；
- 服务端状态：TanStack Query；
- 轻量客户端状态：Zustand，仅存窗口/选择/草稿等 UI 状态；
- 表单：React Hook Form + schema validation；
- Terminal：xterm.js + Rust ConPTY adapter；
- 测试：Vitest、React Testing Library、Playwright、Rust tests；
- 样式：CSS variables + CSS Modules/明确约束的组件样式，不引入决定产品视觉的整套后台模板。

最终依赖在技术验证阶段锁定版本，避免在方案阶段把未验证的库写死。

### 7.2 目录结构

```text
clients/windows-react/
├── docs/
├── src/
│   ├── app/                    # composition、router、providers
│   ├── features/               # 按产品能力组织
│   ├── application/            # use cases 与 ports
│   ├── domain/                 # 稳定业务类型
│   ├── infrastructure/         # typed Tauri/Host adapters
│   ├── platform/               # Windows-only UI capability wrappers
│   ├── design-system/          # tokens、primitives、states
│   └── test-support/
├── src-tauri/
│   ├── src/
│   │   ├── host/               # sidecar lifecycle + IPC
│   │   ├── terminal/           # ConPTY
│   │   ├── credentials/
│   │   ├── filesystem/
│   │   └── commands/           # typed allowlisted commands
│   └── capabilities/           # Tauri 最小权限配置
├── tests/
│   ├── contract/
│   ├── visual/
│   └── e2e/
└── package.json
```

### 7.3 全局苹果主题约束

Windows React 必须复现 macOS 的信息架构、功能、状态和苹果主题视觉语言。此要求是全局产品约束，不是可选的页面优化：

- 左侧资源导航、中心工作区、右侧运行/任务详情保持同一信息层级；
- 页面标题、操作位置、状态颜色、空状态和反馈语义保持一致；
- 产品自有标题栏、工具栏、侧栏、菜单、弹窗、Popover、Sheet、按钮、输入框、列表、卡片、标签、Toast、Tooltip、骨架屏和进度状态全部使用同一套苹果主题；
- Windows 主窗口采用自定义窗口框架还原苹果风格，同时保留系统要求的拖拽区、缩放、最小化、最大化、关闭、键盘操作和无障碍语义；
- 设计 Token 以当前 macOS 实际界面为来源，统一浅色/深色颜色、材质、透明度、模糊、间距、圆角、阴影、描边、字体层级、图标尺寸和动效曲线；
- 图标优先建立与 SF Symbols 语义和线条风格一致的跨平台图标集，不混用 Fluent、Material 或风格不一致的图标；
- 第三方 React 组件只能作为无样式能力层使用，必须完整覆盖其默认主题，不允许默认 Material、Fluent、Ant Design 或浏览器控件样式直接进入产品；
- Windows 快捷键、IME、屏幕阅读器、焦点环和高 DPI 行为遵循系统可用性规范，但视觉呈现仍服从苹果主题；
- 操作系统拥有且应用无法重绘的安全界面（例如系统权限确认、系统文件选择器）是唯一视觉例外，应用自有的前置说明和结果反馈仍使用苹果主题；
- 所有关键页面必须有 macOS 基线截图与 Windows 对照截图；
- 不因 React 组件库、Tauri 或 Windows 默认样式改变产品结构和视觉语言。

## 8. 页面实施顺序

### P0：可运行骨架

- 应用启动、登录态和账号切换；
- Rust Host sidecar 打包、握手、诊断和恢复；
- 主窗口 Shell、左侧导航、主题与基础设计系统；
- 项目/联系人索引的只读加载；
- Windows 安装包和最小 CI。

### P1：核心会话闭环

- 联系人会话、项目用户消息；
- 历史分页、附件、Composer、流式状态；
- 模型/推理选择；
- 发送、引导、继续、取消和错误恢复；
- 任务动态与执行过程显示最新事件，而不是初始快照。

### P2：任务与项目工作区

- Requirement、Plan、Task Graph、Run Inspector；
- Ask User、命令审批、阻塞/失败/重试；
- 项目目录、预览、编辑和搜索；
- Git 状态、Diff 和基础操作；
- Notebook/System Context。

### P3：本机生产力能力

- ConPTY Terminal；
- Agent 管理与团队页面；
- 插件/Skills、模型、运行权限与访问控制设置；
- 应用页与全局工具。

### P4：创作与平台增强

- AI 图片、视频、故事和创作历史；
- 截图、长截图、标注和快速翻译的 Windows 方案；
- 宠物、托盘、全局快捷键和通知；
- 更新、崩溃恢复、诊断导出和发布签名。

远端连接已在 macOS 完成管理、SFTP 与交互式 Terminal 的分层验收；Windows 已完成 metadata/Credential Manager、OpenSSH 连接测试、绑定 MFA 会话的远端目录 mutation、owner/presentation-scoped opaque 本机文件选择、8 GiB 流式上传/下载、下载完整性复验与原子落盘，以及有界的交互式 ConPTY/OpenSSH session。React 已完成连接管理、SFTP 和基于 xterm.js 的真实 Terminal 工作区，继续遵守 Domain → Application Store → Port → Tauri Adapter 边界。其余尚未形成完整可验收 macOS 页面或 Port 的能力仍不得凭空补做。

## 9. 实施阶段与退出条件

### 阶段 0：废弃旧 Windows 与冻结基线

当前状态：执行中。旧 `clients/windows` 与专属构建引用已经删除；macOS 源码事实矩阵正在逐页建立。

工作：

- 删除 `clients/windows`；
- 清理旧 C#、WFP、驱动、服务和安装器的仓库/CI 引用；
- 记录当前 macOS 页面、弹窗、菜单、状态和关键流程；
- 建立 31 张现有截图与当前运行界面的差异清单；
- 建立页面—用例—接口—原生能力矩阵。

退出条件：旧 Windows 不再参与构建；仓库没有悬空引用；每个首批页面都有可复验基线。

### 阶段 1：契约与 macOS 架构地基

当前状态：执行中。`LocalAgentRuntimeState` 已接入 Conversation ready gate；项目 Conversation 准备、Conversation History 查询/合并、发送路径规划、optimistic Turn/失败恢复、send lifecycle identity、viewport/unread mutation identity、bootstrap identity、latest refresh debounce/in-flight 合并、older-page lifecycle、realtime 动作策略、Conversation activation session、runtime recovery identity、runtime settings mutation identity、per-turn Task Graph availability identity、Ask User 查询/提交/取消/投影/表单与 per-prompt 异步生命周期、Human Approval 加载/强类型决策、全局队列展示、监控 session、独立页面加载与决定 Task、Host durable/Human 两阶段端口与决定写回恢复 journal，以及 Task Graph/Run Inspector 核心规则、Inspector request identity、Graph polling session identity、Workspace presentation session、内嵌 Inspector presentation session 和 Run 重试 identity 已从 UI 协调层提取。审批页加载已与通用 tab loader、全局 stream/monitoring 分离，具备刷新淘汰、切页/登出迟到结果拒绝和失败保留最后可信快照；Ask User 已具备刷新 request identity、同 Prompt 操作互斥、跨 Prompt 独立并发和 runtime unavailable 迟到结果拒绝；activation 触发的 Prompt refresh 外层 Task 也由 ViewModel 持有并绑定当前 session，停用只取消 refresh lane，不影响已提交的 per-prompt action；Conversation bootstrap Task 会冻结创建时的 ready 决策，runtime 转换只淘汰其 Host 恢复动作而不阻断本地 History Store hydration，延迟执行的 bootstrap 不得与 ready recovery 重复加载 runtime 服务；Conversation realtime、debounce、in-flight latest history 与 older-page presentation 只接受当前可见 activation，停用时立即复位对应 loading；latest-history in-flight Task 已显式持有且远程等待期间不强持有 ViewModel，停用后仍允许完成 Store 合并但拒绝 presentation 写回；timeline anchor 与 mark-read 使用独立 latest-wins token，停用后迟到的本地 snapshot 不得写回不可见页面；ready 恢复 Task 由 ViewModel 显式持有并绑定 latest-wins token，连续 ready 或 runtime 再次 unavailable 都会淘汰旧恢复，迟到结果不能重新刷新历史或启动 realtime；时间线的 Task Graph 可用性探测按 Turn 绑定 token，停用或 unavailable 会失效未完成探测，ready 后只重试尚未形成可信缓存的 revision；发送 Task 由 ViewModel 显式持有并绑定 token，runtime unavailable 会恢复 composer、回滚 optimistic Turn 和拒绝旧 Host 响应；模型、推理等级与远程连接更新采用 latest-wins token，旧响应不能覆盖新选择、回滚新状态或关闭新 loading；Task Inspector 已覆盖同一 Task 重载、section 切换和 Run 事件分页的迟到结果隔离，内嵌 Inspector 与 Workspace 的 Run retry 均绑定目标 identity，切换目标后旧 Host 结果不能恢复旧 Inspector 或干扰新 loading；Workspace 选中 Task 时先完成基础详情请求，再按当前 section 加载模型输出或 Run，section 请求不再反向取消基础详情；内嵌 Inspector 的 process→detail 切换复用已加载 Task，只补取模型输出，realtime/polling 刷新则仍重新查询最新 Task；其 load、refresh、retry、realtime 与 polling 全部绑定当前 presentation session，任务读取、模型输出与 Run 重试已统一通过 Core Application use case；Graph polling 在停止或重启后会拒绝旧 session 的迟到结果，realtime 中断后的降级 polling 也只能由当前 session 驱动；Workspace 关闭后，初次加载、手动刷新、Run 重试、realtime 与 polling 的迟到工作都不能写回或重启后台任务。Conversation 与 Inspector 的其余 cancellable Task 生命周期、service orchestration 与 typed lifecycle port 继续迁移。

Conversation 附件文件读取和粘贴图片归一化现已绑定 composer draft generation；同一草稿可并发导入，发送清空 composer 时会失效并取消全部旧导入，底层任务即使迟到也不能把附件写入下一条消息。Runtime settings snapshot load 也已改用显式 latest-wins identity；新加载、设置 mutation 或 runtime unavailable 都会拒绝旧 Host snapshot。

Windows React Composer 已完成 macOS `/query` 显式能力选择闭环，包括语法解析、候选过滤、键盘导航、可移除 chip、活动 Turn 冲突和失败恢复。当前仅发布 Host 可执行的 Requirement Survey；插件候选明确等待 Windows platform plugin worker 与真实 capability binding，不以安装记录伪装可执行能力。

Workspace Inspector 的详情、模型输出、Run 与事件分页 Task 已冻结请求目标和 Application use case，Host await 期间不再强持有 ViewModel；底层请求即使忽略取消也不会阻止弹层状态释放，返回后仍须通过现有 request token 与目标 identity 才能写回。

内嵌 Task Inspector 在 detail 加载中切回 process 时会重新挂起当前活跃 Task 的 polling；取消模型输出请求不能同时让执行过程停止刷新。

Conversation 执行过程读取已使用 Host `process` 事件载荷投影：SQLite 查询阶段只保留映射 UI 所需的 tool call identity、状态、原因与错误摘要，移除附件内容和工具参数；macOS 以每页 25 条、最多 400 页的既有 10,000 事件上限读取以控制内存与解码成本，超过 4 MiB 的逻辑消息由 IPC 自动分片，同时仍完整合并工具生命周期。

项目 Conversation 准备现已由 Core `ProjectConversationPort`、账号绑定的 per-project token state 与 AppModel 持有的具体 Task 共同管理；`idle / preparing / failed` 强类型展示阶段也已并入同一 Application state，SwiftUI 不再通过独立 loading Set 和 error Dictionary 推导状态。项目删除只取消对应准备，账号切换、登出和应用退出取消全部准备，迟到结果不能恢复旧 loading 或错误。准备 Task 会在进入 Application use case 前冻结 owner、project、contact、account generation 与 port；Context/Host await 期间不再强持有 AppModel，返回后仍须联合验收账号、项目存在性和 per-project token。Connector 已通过既有 binding 复用、context 错配拒绝、创建 binding 与并发胜出恢复测试，该子切片已允许进入 React，但不包含 Conversation 时间线。

项目 Workspace 刷新已用 Core `WorkspaceLoadState` 取代裸 generation，并把 registry、本地快照、设备配对、根目录修复、远端关系与 Run project 映射保持为既有的两阶段加载顺序。刷新 Task 在开始时冻结账号、account generation 与服务依赖，各段 IO 期间不再强持有 AppModel；每次快照准备、展示写回、错误和 loading 收尾都必须通过同一个 typed token，项目删除、账号切换、登出或新刷新会同时淘汰旧请求。

Local Agent 控制面 bootstrap 已新增绑定 owner、account generation、Host lifecycle generation 与 request ID 的 `LocalAgentControlPlaneBootstrapState`。本地快照恢复和 authenticated gateway bootstrap 共用无 AppModel 所有权的服务配置函数；API/Connector/各业务服务 await 只持有冻结 adapter，并在每一段配置后重新验收当前生命周期，只有最终同步提交会短暂恢复 AppModel。

Conversation Session 的服务组合已收口到 App 层 `ConversationSessionFactory`；AppModel 不再逐项构造 ViewModel 依赖，只负责 session 缓存、LRU 淘汰与激活策略，并在每次创建时传入当前账号对应的 runtime ready 状态。Conversation feature state 进一步统一构造 Turn Process、Task Workspace 与内嵌 Task Inspector 子 ViewModel，时间线和桌面宠物 SwiftUI View 不再读取或转发底层 ports。

Conversation 的模型、推理等级与远程连接 mutation Task 已冻结 session/请求参数，Host await 期间不再强持有 ViewModel；底层忽略取消时仍可释放会话状态，返回后继续以 mutation token 验收。

Conversation older-history 分页与 viewport/unread mutation 也已采用相同持有边界；远程 History 或本地 Store await 不会延长 ViewModel 生命周期，返回后仍须通过 activation、cursor 或 mutation token 才能更新展示。

Conversation 激活触发的 Ask User refresh 已拆分为请求句柄、无 ViewModel 等待与主线程 token 验收三段；Prompt Host 请求忽略取消时不再阻止 Conversation 释放。

Conversation bootstrap 与 runtime-ready recovery 现复用 runtime settings/Prompt 请求句柄并并发等待；模型或 Prompt Host 阻塞时不再整段持有 ViewModel，结果仅在对应 load/recovery token 仍有效时应用。

Conversation realtime Task 不再在整个 stream 生命周期内强持有 ViewModel，只在单个事件同步写回时恢复引用；Prompt realtime 事件等待 Host 时也使用请求句柄，因此失去最后一个 UI owner 后长连接会正常随 deinit 终止。

Ask User 提交或取消失败后的 Host 对账刷新已拆为请求句柄、无 ViewModel 等待与 action/refresh token 联合验收；底层 Prompt 请求忽略取消时不再留住 Conversation，也不能以迟到快照结束新操作。

全局活动定位 Ask User 前的 Prompt refresh 已改为 Conversation 显式持有的 latest-wins focus Task 与 typed token；后一次定位会淘汰前一次，Host 忽略取消时也不会因实例 async 方法而留住 Conversation。

远程连接列表已由 Core `RemoteConnectionListUseCase` 统一查询与排序，并使用绑定账号 generation 的 typed latest-wins token；AppModel 在 IO 期间不再强持有自身，账号切换或更新请求会拒绝旧列表、错误与 loading 写回。创建/编辑、草稿与已保存连接测试、SSH 二次验证和删除进一步收口到 `RemoteConnectionManagementUseCase` 与 single-flight operation state；编辑器和详情页退出会失效具体请求，Connector 忽略取消时也不能保活 ViewModel 或把迟到结果写回不可见页面。SFTP 已通过 `RemoteFileUseCase` 与远端/本机/mutation 三条 typed lifecycle 分层，连续导航 latest-wins，传输和文件 mutation 区分提交失败与 committed-without-refresh，workspace 清理会拒绝迟到结果。Remote Terminal 已抽取 Core byte-stream Port、connect/session generation 和 Connector adapter；SwiftUI 不再依赖原生 SSH session、delegate、进程或凭据实现，重连/断开会拒绝旧 provider 结果和旧 session 事件。macOS 远程资源纵向切片已完成分层。Windows 已完成 metadata/Credential Manager、固定 OpenSSH、host-key、jump host/MFA、目录读取/mutation、committed-without-refresh、opaque 本机文件选择、流式上传/下载、大小/完整性门禁和原子落盘，并完成交互式 ConPTY/OpenSSH session。React 连接管理、SFTP 与 Terminal 已按同一事实源形成完整远程资源工作区；Terminal 输出、输入、resize、中断、MFA、重连、断开、搜索、清屏和 OSC 7/title 均通过 typed Port 接入，旧 presentation/session 的迟到结果不得污染当前工作区。

Conversation History 失败后的 2/5/10/20 秒有界重试已收口到 `ConversationHistoryRetryState`；每次 sleep 绑定具体 activation token，停用、重新激活、手动刷新或成功响应都会拒绝旧重试唤醒，不再只依赖 Task cancellation。

插件应用列表已拆出 `LocalConnectorPluginApplicationListing` 窄 Port 与 `PluginApplicationListUseCase`，并使用绑定账号 generation 的 typed latest-wins token；AppModel 不再自行组织 Connector IO，旧账号或旧刷新不得覆盖应用列表、错误和 loading。

工作：

- 固定 Host 协议版本、schema、fixture 和兼容策略；
- 建立 Domain、Application、Ports、Composition Root；
- 统一 Host lifecycle、错误、loading 和恢复模型；
- 增加架构依赖检查，阻止 View 新增 Connector 直接依赖。

退出条件：新切片能在不依赖具体 Connector 类型的情况下完成单元测试；Swift/Host 契约测试稳定。

### 阶段 2：macOS 核心垂直切片

工作：依次迁移 AppShell、Conversation、Run/Task/Approval、Project/Git/Notebook、Terminal。

退出条件：核心流程全部通过行为回归；页面层无 IO；重启、断线、账号切换和任务恢复可测试。

### 阶段 3：Windows React 技术样板

当前状态：执行中。`clients/windows-react` 已建立 React 19 + TypeScript + Vite + Tauri 2 工程，Host lifecycle、Shell、Authentication、Workspace Index、项目会话准备、项目创建/文件夹授权与 Conversation 核心闭环均通过 typed Port/Adapter 接入。Shell 只读取显式 lifecycle snapshot；Host 未就绪或握手失败时展示可恢复状态，不以无限 loading 隐藏错误。Tauri 已打包真实 Local Agent Host sidecar，完成 v44 framed health/business request、4 MiB frame/64 MiB message限制、有界无 Secret 诊断及应用退出前的 Host 回收。账号 bootstrap 已按 macOS 的双 Host 顺序接入：先以 `--disable-workers` 恢复持久化快照和 Credential Manager 模型密钥，再并发刷新 `/api/model-configs`、模型 Secret 与 `local_agent_execution_agent`，发布模型、`main_chat`、`task_execution` 和 Task policy 快照，最后仅通过净化后的子进程环境启动 workers Host；网络不可用时只允许完整本地快照离线恢复，最终 health 完成前绝不返回 `ready`。Windows 已实现附件读取 worker 与真实 terminal platform worker：`task_execution` 仅发布附件及 macOS 同源的 `execute_command / process_poll / process_log / process_wait / process_write / process_kill`，Task policy 只开放 `TerminalController`；旧 attachment-only capability snapshot 不得离线恢复。Host 启动参数把附件读取和三项终端查询标为 read-only，命令执行、stdin 写入和终止进程保持 side-effecting 并由 durable approval 门禁。服务端 MCP/Plugin 工具仍不会被伪装为可用。自定义窗口框架保留拖拽、缩放、最小化、最大化和关闭能力，并完成浏览器视觉回归。Authentication 复用 macOS 的恢复、登录、注册、验证码倒计时与 logout/login 排序语义；Token 仅由 Rust 持有并写入 Windows Credential Manager，登录成功后才以 `user.id` 启动 Host。Workspace Index 复用 macOS 的客户端本地项目权威与 local-first 两阶段加载：本地 SQLite 项目先展示，Host 联系人/会话关系随后刷新，远端关系不得发明项目。项目点击后以 Registry context 校验、Host project binding 幂等复用/创建和并发失败二次查询完成四态准备。History 固定 macOS 的 10 条分页、ordinal 游标、latest/older 独立状态、ID 去重与迟到结果隔离；React 不读 SQLite，也不直接调用 Tauri。Composer 已完成文本、附件、显式能力选择和运行时控件：复用 macOS 的 new Turn/guidance 规划、guidance 竞态回退、失败草稿/附件恢复、模型/推理等级/远端连接语义、共享模型推理目录、乐观更新及 latest-wins 迟到结果隔离。附件只在 Rust 原生层读取，按 owner + Conversation 隔离并以 SHA-256 复验，React 只持有 opaque ID 和受控预览 URL；20 个/20 MB 限制、draft generation、历史授权预览和模型读取 worker 已形成闭环。Realtime 已按 Host durable event cursor 接入，只在 Conversation version 或相关 Run/Conversation 事件变化时静默合并最新页，保留已加载旧分页且不以全屏 loading 打断输入。Run Inspector 已按真实 Message Task Graph 提供入口，首屏固定最新 40 条 newest-first 事件，更早记录每页 50 条且按 event ID 去重；Tauri 仅投影安全的状态/摘要字段，不把完整工具 payload 暴露给 React。Ask User 已按 macOS 查询、表单、提交、取消和并发恢复语义接入：只查询当前 Conversation 的 waiting Run，主会话和任务分别走原有 Host 命令，每个 Prompt 独立 mutation identity，失败后重新读取 Host 真值，Secret 在 Rust Host IPC 前拒绝。Human Approval 已完成 durable 与 session 两阶段：Tauri 从 Host pending queue 生成无原始 arguments 的安全投影，决定前重读并校验 invocation identity/version；React 全局浮层采用 2 秒 fallback polling、最后可信队列、owner/Host session 迟到隔离、同审批互斥与跨审批并发，并为已准入 terminal 写操作提供单次允许、会话允许和拒绝。Human/会话自动决定在写 Host 前进入绑定 owner + invocation identity/version 的私有有界 journal；IPC 失败或应用重启后只重放原决定，冲突决定不会覆盖，且绝不从 journal 恢复进程内 session allowlist。allowlist 的 macOS `v2` exact-request key 绑定 owner/device/workspace/root/cwd/Conversation/purpose/权限/PowerShell command arguments，Host 新建、替换、失联、停止、登出和应用退出都会清空。Windows CI 已覆盖前端架构审计、类型检查、测试、构建、Rust 测试、真实 Host 业务请求、Tauri release build 和 NSIS 安装包产物上传。后续继续按准入矩阵推进项目文件预览/搜索和其他核心页面。

状态补充（2026-10-20）：`execute_command` 的允许操作会在写回前重读 Run/Conversation，验证 macOS 同源 `tool_options`，并从账号隔离的 Registry/Connector grant 解析执行根目录；cwd traversal、文件路径及越界软链接均失败关闭。Windows 启动时原子初始化 owner-scoped Connector state，账号切换会清除旧 workspace grants。terminal runtime 支持前台/后台命令、有界 stdout/stderr、poll/log/wait、stdin、kill、前台超时、Windows Job Object 与 Unix process group 回收；worker 支持最多 4 个并发 claim、30 秒 lease、10 秒 heartbeat、preflight outcome、Run 完成事件回收，以及 Host 失联/登出/退出/reset 的 fail-closed 清理。terminal `acceptForSession` 已按 macOS exact-request 语义接入，仅保存在当前 Host/应用进程内。审批设置切片进一步接入 macOS 同源的 `request_approval / auto_approval / full_control` 三态、账号模型与推理等级、风险确认、等待队列和最多 1,000 条审计历史；Windows 审批 Agent runtime 尚未准入时明确降级 Human。决定按 journal→审计历史→Host writeback 顺序持久化，恢复重放先补齐幂等历史，旧 journal schema 可兼容读取；`full_control` 只自动批准通过 execution context/cwd 复验的已准入 terminal 写操作，非 terminal 工具继续进入 Human queue。当前验收为 Windows Rust 80 项通过、2 项 sidecar 环境测试忽略，React 28 个文件 103 项通过，TypeScript、production build、架构审计与 Clippy 通过。真实 sidecar health/account request 2 项、Host claim/lease/approval/recovery 10 项和 Host tool scheduler 49 项继续有效；本机缺少 MinGW sysroot，因此 Windows GNU cross-check 由 Windows CI 继续承担。下一步是运行时权限设置页。

状态补充（2026-10-20，运行与权限）：已按 macOS `LocalConnectorRuntimePermissionsView` 的系统权限契约建立独立 Domain/Port/Tauri/UI 切片，没有把第 31 页访问控制策略混入本页。Windows 原生快照只陈述已验证事实：terminal worker 已就绪；文件访问遵循当前账号 ACL 与 ChatOS workspace grant、按需生效；Computer Use 与屏幕采集 worker 尚未准入，因此显示“尚未提供”而非伪造“需要用户授权”。当前没有任何可由应用内触发的 Windows 授权项，UI 不展示无效按钮；未来只有原生层发布 `canRequest` 才允许调用 request port。Host/账号切换、refresh 与 permission action 均有迟到结果隔离。当前验收为 Windows Rust 82 项通过、2 项 sidecar 环境测试忽略，React 31 个文件 114 项通过，TypeScript、production build、架构审计与 Clippy 通过。下一步是第 31 页访问控制设置。

状态补充（2026-10-20，访问控制）：已按 macOS `LocalConnectorSandboxView` 的“默认任务权限 + 运行后端”信息结构建立独立 Domain/Port/Tauri/UI 切片，并按 Windows 当前真实执行边界收窄为只读状态。PowerShell worker 具备 owner/project context、workspace grant、规范化 cwd、审批门禁与 Windows Job Object 进程树回收，但尚未具备操作系统级文件或网络隔离，因此后端明确显示 `limited`，文件权限显示 Windows 账号 ACL，网络显示跟随本机，不发布无效开关或下拉框。审批摘要直接投影既有 `approvalMode`，没有创建第二套策略真值。Host/owner refresh 使用 request identity 拒绝迟到结果。当前验收为 Windows Rust 85 项通过、2 项 sidecar 环境测试忽略，React 34 个文件 123 项通过，TypeScript、production build、架构审计与 Clippy 通过。下一步回到第 01 页项目目录，完成项目创建、授权和目录展示的 macOS 源码映射。

状态补充（2026-10-20，项目目录只读树）：已按 macOS `ProjectWorkspaceView`、`ProjectDirectoryViewModel`、`ProjectFilesystemServicing` 与 `NativeWorkspaceFilesystem` 建立项目工作面和只读目录树垂直切片。Windows 默认仍打开用户消息；项目目录从 owner-scoped Registry/workspace grant 解析真实根目录，React 只接收规范化相对路径。单目录最多 5,000 项，文件夹优先排序，符号链接和非便携路径失败关闭；展开/折叠、刷新、项目切换与页面离开均用 identity 拒绝迟到结果。当前只显示文件元数据，不提前开放尚未验收的内容预览、搜索、编辑、删除、Git 或项目设置。当前验收为 Windows Rust 88 项通过、2 项 sidecar 环境测试忽略，React 37 个文件 132 项通过，TypeScript、production build、架构审计与 Clippy 通过。下一步是项目创建与 Windows 文件夹授权切片。

状态补充（2026-10-20，项目创建与 Windows 文件夹授权）：已按 macOS `CreateProjectSheet`、`CreateProjectViewModel`、`NativeLocalProjectsService` 与 `SQLiteProjectRegistry` 固定“本地项目、所选目录本身为根、成功后再发布选择”的语义，并以 Windows 原生文件夹选择器替代 macOS 已配对根目录浏览。React 仅接收绑定 owner + presentation 的一次性 opaque selection ID 与目录显示名，绝对路径只存在 Rust；弹窗关闭、账号替换、重复选择和保存分别以 identity 拒绝迟到结果，原生 tombstone 同时拒绝关闭后才返回的选择器结果。提交会重新 canonicalize 目录并拒绝文件、符号链接和 Windows reparse point；稳定 workspace ID/fingerprint、owner-scoped grant 与 Project Registry 记录通过有界恢复 journal 收口，跨 JSON/SQLite 中途退出可幂等补齐。活动或归档项目不能重复注册相同 canonical root，removed 项目可重新注册；Connector state 全部读写增加进程内串行化，避免审批设置与文件夹 grant 相互覆盖。27 个项目类型字段由测试逐项对照 macOS `ChatOSSkillCatalog.json`。创建成功先关闭弹窗，再原子加入当前账号侧栏、选中新项目并进入默认“用户消息”工作面。当前验收为 Windows Rust 95 项通过、2 项 sidecar 环境测试忽略，React 40 个文件 141 项通过，TypeScript、production build、架构审计与 Clippy 通过。下一步是项目文件内容预览与只读搜索切片。

状态补充（2026-10-20，项目文件内容预览与只读搜索）：已按 macOS `ProjectDirectoryViewModel`、`ProjectFilesystemServicing.search/readFile`、`NativeWorkspaceFilesystem` 与 `ProjectFileEditorView` 接入文本、图片、二进制状态预览，以及文件名/相对路径和文件内容并行搜索。每次请求仍从 owner-scoped Registry 与 workspace grant 解析项目根，逐级拒绝符号链接和 Windows reparse point，React 只接收规范化相对路径。文本预览上限 2 MiB、图片 25 MiB、内容搜索单文件 2 MiB；二进制取前 8,000 字节检测 NUL，文本按 UTF-8 容错解码。搜索最多访问 20,000 项、最长 3 秒、最多返回 500 项，大小写与变音符号不敏感，跳过 macOS 同源的 13 个生成/依赖目录，摘要最多 2,000 字符。搜索采用 280 ms 防抖并并发执行文件名与内容查询；项目切换、页面离开、清空查询、选择新文件和后续查询都会以 request identity 淘汰迟到结果，单路失败仍保留另一条可信结果。当前仍只读，不开放编辑、创建/删除、Git 或项目设置。当前验收为 Windows Rust 97 项通过、2 项 sidecar 环境测试忽略，React 40 个文件 144 项通过，TypeScript、production build、架构审计与 Clippy 通过。下一步是项目文件写操作的审批与竞态边界切片。

状态补充（2026-10-20，macOS 项目文件 mutation 准入）：`writeFile/createFile/createDirectory/deleteEntry` 的多步 service orchestration 已从 `ProjectDirectoryViewModel` 提取到 Core `ProjectFilesystemMutationUseCase`。保存会冻结文件路径和草稿，创建/删除会冻结父目录与目标；单个 ViewModel 只允许一个 mutation，刷新、选择新文件或迟到的重读不得覆盖当前展示。写入、创建或删除已经提交但后续 read/list 失败时返回“已提交但刷新失败”的强类型结果，UI 不再把成功的副作用误报为失败。macOS 新增 3 项生命周期测试，覆盖保存期间切换文件、创建/删除后以 mutation 结果刷新父目录，以及创建已提交但刷新失败的恢复语义。该子切片已达到 React 写操作准入；Windows 仍需独立实现 canonical path、原子写入、冲突检测与 mutation identity 后才可开放 UI。

状态补充（2026-10-20，Windows 项目文件原生 mutation 边界）：Tauri 已建立保存、创建文件/目录和递归删除命令，但尚未向 React Port/UI 开放。保存要求读取返回的 SHA-256 revision，写入前重新解析 owner/project grant、逐级拒绝 symlink/reparse point 并复验当前内容；旧 revision 必须冲突失败，不能覆盖外部修改。写入内容最多 2 MiB，先在同目录写入临时文件并同步，再以 Windows `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` 或 Unix rename 原子替换。新建使用 create-new/单层 create-dir，绝不覆盖同名目标；删除文件要求 revision，删除目录拒绝文件 revision且不能删除项目根。副作用提交后的 read/list 失败会返回 committed-without-refresh 投影，不伪装成副作用失败。React 文件内容 schema 已接收 opaque revision，但编辑、新建和删除 UI 仍保持关闭，等待 Port/Store mutation identity 与确认交互完成。

状态补充（2026-10-20，Windows 项目文件 mutation React 闭环）：已按 macOS `ProjectDirectoryViewModel` 与 `ProjectFileEditorView` 将保存、新建文件/目录和删除接入 typed React Port/Adapter、Zustand Store 与 Apple 风格 UI。Store 冻结 owner、project、generation、目标、revision、草稿和创建父目录；同一展示 session 只允许一个 mutation。保存期间切换文件不撤销已开始的副作用，但迟到结果不得覆盖新文件；切项目、刷新和离开页面会淘汰旧 UI 写回。revision 冲突保留草稿与编辑状态；保存已提交但重读失败时展示已提交提示、保留新内容并暂时禁用再次写入；创建/删除已提交但目录刷新失败时保留最后可信树并提示刷新。创建位置遵循 macOS 的“所选目录/所选文件父目录/项目根”规则，二进制和只读文件不开放编辑；删除必须经过苹果主题确认弹框，文件删除携带读取时 revision。当前验收为 Windows Rust 98 项通过、2 项 sidecar 环境测试忽略，React 40 个文件 153 项通过，TypeScript、production build、架构审计与 Clippy 通过。项目目录下一独立切片为 Git 工作面；项目设置仍保持关闭，等待各自的 macOS 分层准入。

状态补充（2026-10-20，macOS 项目 Git 准入）：已按 `ProjectGitWorkbenchView`、`ProjectGitViewModel`、`ProjectGitServicing`、`NativeProjectGitService` 与 parser 建立完整源码事实映射。init、stage/unstage、commit、branch、merge、remote、pull/push 后的 snapshot 刷新已收口到 Core `ProjectGitUseCase`；副作用成功但刷新失败返回 committed-without-refresh，不再误报 Git 操作失败。ViewModel 的 load、diff 和 mutation 均有独立 request identity，同一时间只允许一个 mutation，迟到 snapshot/diff 不得覆盖当前展示，mutation 会淘汰旧 load/diff；提交期间新输入的 message 不会被旧提交完成清空，失败恢复 snapshot 时保留原始操作错误。5 项新 ViewModel 生命周期测试、4 项既有 Native Git 集成测试和 3 项文件 mutation 回归通过。该页面现只允许进入 Windows 原生 Git 边界实施；Windows React Git 入口继续关闭，直到 owner/project/workspace 解析、进程/输出/超时限制和 typed lifecycle 全部完成。

状态补充（2026-10-20，Windows 项目 Git 原生边界）：已新增无通用参数入口的 typed Tauri Git commands，覆盖 snapshot/diff/init、stage/unstage、commit、branch/merge、remote、`pull --ff-only` 与 push。每次入口均从 owner-scoped active Project Registry 和 Connector workspace grant 解析 canonical project/workspace，repository root 不得越界；path mutation/diff 必须命中刚读取 snapshot 的对应 staged/worktree 状态。WebView DTO 不包含 repository root、absolute path、raw stderr 或带凭据/本地绝对路径的 remote URL。Git stdout/stderr 并发有界 drain，diff 上限 4 MiB、其余 16 MiB；普通命令 120 秒、网络命令 15 分钟，Unix process group 与 Windows Job Object 会在超时及主进程退出后回收整个进程树。mutation 后 snapshot 失败以显式 committed-without-refresh 返回。8 项新增原生测试及 Windows Rust 全量 106 项通过，2 项 sidecar 环境测试忽略，Clippy 通过。下一切片接入 React Port/Store lifecycle 和 Apple 风格 Git 工作面，完成前入口继续关闭。

状态补充（2026-10-20，Windows React 项目 Git 闭环）：已按 macOS `ProjectGitViewModel`、Workbench、Change List、History、Diff 与 branch/remote sheets 接入 Domain schema、typed Port/Tauri Adapter、Zustand lifecycle 和 Apple 风格工作面。load/diff/mutation 使用独立 identity，mutation 淘汰旧 load/diff 且互斥；切项目和离开页面拒绝迟到写回，失败恢复最新 snapshot 但保留原始操作错误，committed-without-refresh 显式提示，提交期间新输入不会被旧提交清空。UI 提供 init、暂存/取消暂存、提交、branch switch/create/merge、remote add/edit/remove、`pull --ff-only`、push、ahead/behind、最多 80 条 history 与最多 8,000 行语义 diff，并能从 diff 按相对路径切回文件。remote URL 的脱敏标记允许仅改名时由 Rust 保留原始 Secret，不会把凭据带回 WebView。当前验收为 Windows Rust 106 项通过、2 项 sidecar 环境测试忽略，React 43 个文件 170 项通过，TypeScript、production build、架构审计与 Clippy 通过。项目 Git 已进入 React；项目设置仍等待独立 macOS 分层准入。

状态补充（2026-10-21，Windows React 项目设置闭环）：已按 macOS `ProjectRunSettingsView`、Instances、Environment 与 `ProjectRunSettingsViewModel` 接入独立 Domain schema、typed Port/Tauri Adapter、Zustand lifecycle 和 Apple 风格项目设置页。React 只提交 opaque target/toolchain ID；运行状态通过最长 30 秒 long-poll 更新，owner/project/generation/request identity 拒绝切项目和离页后的迟到写回，mutation 互斥且失败后读取原生真值并保留原始错误。环境编辑只取得 `key + hasValue`，已有 Secret 默认以 `value: null` 保留，用户明确输入替换值时才将明文交给原生层，保存期间新草稿不会被旧结果覆盖。页面按 macOS 顺序提供宠物快捷访问、预检、目标与启动、实例与日志、工具链、环境变量、配置预览和项目移除；移除成功后同步 Workspace Index、清理该项目 Conversation cache 并返回工作台，本机目录始终保留。React 全量 46 个测试文件、185 项、TypeScript、production build 与架构审计通过，项目设置已进入 React。

状态补充（2026-10-21，Windows React 联系人聊天入口）：已按 macOS `ContactConversationView` 将联系人直接组合到既有 Conversation History/Composer/Attachment/Realtime/Runtime Settings/Run Inspector/Ask User 闭环，不新建联系人专属协议。无公开 Conversation 时保持 macOS 空态；`plugin-installer` 仅增加“安装能力只在此对话可用”的隔离提示，真实能力仍服从 Host snapshot 与 policy。AppShell 已移除联系人占位卡，3 项入口测试通过，React 全量为 47 个测试文件、188 项。

状态补充（2026-10-21，Windows React 本机终端闭环）：已按 macOS `TerminalWorkspaceViewModel`、`NativeLocalTerminalViewModel` 与 `LocalTerminalUseCase` 建立独立的 Domain/Port/Tauri/React 切片，没有复用 task terminal worker 或远程连接权限。Rust 原生层以 owner + presentation + generation 绑定最多 4 个交互会话；React 不提交 executable、arguments、environment 或绝对 cwd，Windows 只从系统目录解析 Windows PowerShell，并由原生层选择当前用户目录和构造不含应用 Secret 的 allowlist 环境。每个 ConPTY 会话具备 64 KiB 输入、1–1000 viewport、512 event/2 MiB 输出 ring、128 event/256 KiB poll、30 秒 long-poll 和 Windows Job Object kill-on-close；账号恢复/登录/注册/登出、Host 停止/失联/替换、页面离开和应用退出都会回收。React Store 保持 macOS 的多标签、关闭最后标签立即补建、每标签 identity/输出/标题/cwd/phase、串行输入、迟到 start/poll 隔离、搜索、清屏、Ctrl-C、OSC title/cwd 与 10,000 行 scrollback；Adapter 只在 `App.tsx` composition root 构造，UI 无 `invoke`。当前验收为 Windows Rust 184 项通过、2 项 sidecar 环境测试忽略，React 69 个测试文件、246 项通过，TypeScript、production build、架构审计与 Clippy 通过。下一步是第 05 页插件应用的 Windows 原生 runtime/bridge 安全边界。

状态补充（2026-10-21，Windows Plugin 应用原生 catalog）：已按 macOS `NativeLocalConnectorService.fetchPluginApplications` 与 Host 当前账号安装记录建立 Tauri catalog 前置切片。原生层分页读取并交叉复验 installation summary/record，只接受 enabled、schema v3、Windows platform、workbench surface 和组件级必需 `process.spawn`；executable、manifest、UI source 必须 canonicalize 到 `<appData>/NativeConnector/Plugins` 下同一安装根。manifest、参数、health path、timeout 和 bridge capability 均有界，React DTO 不包含 executable、安装目录、source 路径或 Secret。11 项定向测试及 Windows Rust 全量 195 项通过、2 项真实 sidecar 环境测试忽略。该切片只完成 catalog，不代表第 05 页进入 React；下一步仍是 local HTTP runtime、进程树回收、受限 bridge 与 launch lifecycle。

状态补充（2026-10-21，Windows Plugin 应用 local HTTP runtime）：已按 macOS `NativePluginApplicationRuntime`、`NativePluginRuntimeContextResolver` 与 `PluginApplicationLaunchUseCase` 建立 launch 前置切片。每次启动重新读取 Host 安装真值；package bin、manifest source 和 executable 均锁定安装根且拒绝 symlink/reparse。原生层解析 Node/参数，构造不含应用 Secret 的 allowlist 环境，按 user/plugin/project/workspace/device 生成 SHA-256 隔离 data/cache，只向 UI 返回 project ID/name。服务使用 `127.0.0.1` 随机端口、有界 health timeout、无 redirect/proxy 客户端和固定缓冲输出 drain；相同 release/digest/version 才复用。owner/presentation/generation/context 与 runtime epoch 拒绝 teardown 后迟到启动；每账号最多 8 个进程，Windows Job Object/Unix process group 在失败、账号、Host 和应用退出时回收。Windows 当前只具备受限环境与进程生命周期边界，不伪装成 macOS Seatbelt 文件/网络沙箱。Plugin 定向 21 项及 Rust 全量 205 项通过、2 项真实 sidecar 环境测试忽略，Clippy 通过。第 05 页仍等待 origin/navigation/session 绑定的受限 bridge，完成前不创建 React UI。

状态补充（2026-10-21，Windows Plugin 应用原生 bridge 准入）：已按 macOS `RestrictedPluginWebView` 当前真实能力将原生 bridge 收窄为 `host.context.read`，不因 manifest 声明而提前开放 `artifact.*`。launch 产生独立 session 与 64 位随机 nonce，bridge 请求同时复验认证 owner、presentation、generation、session、nonce、精确 `http://127.0.0.1:<port>` origin、存活 runtime 和 256 KiB envelope；discard、账号/Host reset 或进程退出后旧请求失败关闭。返回 context 只有 project ID/name 和已准入 capability，不含项目根、安装路径或 Secret。Plugin 定向 22 项、Rust 全量 206 项通过、2 项真实 sidecar 环境测试忽略，Clippy 通过。至此第 05 页达到 React 准入；下一片建立 typed Port/Adapter、latest-wins Store、项目 context picker、exact `event.source`/origin 校验和 sandbox iframe Host。

状态补充（2026-10-21，Windows React Plugin 应用闭环）：已按 macOS `PluginApplicationsView`、`PluginApplicationHostViewModel`、`PluginApplicationsLoadState` 与 `PluginApplicationLaunchUseCase` 建立独立 Domain/Port/Tauri Adapter/Zustand Store/UI 切片。侧栏开放“应用”，列表具备 loading/error/empty/retry 和 Apple 风格应用卡片；Host 页面支持返回、重新载入、失败重试、项目/工作区 context picker、公共项目 fallback 与切换上下文。Store 联合绑定 owner、Host availability、account generation、presentation、application、context、launch generation 和 request identity，账号/Host/页面 teardown、重试及连续切项目都会拒绝迟到结果并向原生层写入 generation tombstone。iframe 只加载原生返回的精确 `http://127.0.0.1:<port>`，sandbox 不含 popup/顶层导航权限；bridge 同时校验 `event.source`、exact origin、protocol、session、nonce、唯一有界 request ID 和 admitted capability，页面重载/卸载后拒绝旧响应。CSP 仅增加 loopback `frame-src`，不放宽 `connect-src` 或 Tauri IPC。React 全量 74 个测试文件、258 项通过，TypeScript、production build 与架构审计通过；第 05 页已完整进入 Windows React。

工作：创建 Tauri/React 工程，接通 Host、账号、导航和一个完整 Conversation 垂直切片。

退出条件：安装后的 Windows 应用可启动、登录、恢复会话、发送消息、查看执行过程并安全退出；没有开发机路径依赖。

### 阶段 4：Windows 核心功能覆盖

工作：按 P1、P2、P3 顺序实现，逐页使用 macOS 基线验收。

退出条件：核心页面、状态和任务闭环与 macOS 行为一致；所有平台差异都有明确记录，而非静默缺失。

### 阶段 5：发布硬化

工作：性能、内存、长时间运行、升级、签名、无障碍、安全审计、诊断和灾难恢复。

退出条件：通过发布门禁、升级/回滚测试和真实 Windows 设备验收。

## 10. 测试与质量门禁

### 10.1 架构门禁

- macOS View 文件不得新增 `ChatOSConnector` 直接依赖；
- React 页面只有在源码事实矩阵达到“macOS 分层完成，可进入 React”后才允许创建；
- 源码事实矩阵必须记录 macOS View/ViewModel、Domain/Port、Connector/Host、测试与平台差异，截图不能作为业务逻辑来源；
- React feature 不得直接调用 Tauri `invoke`，只能使用 typed application port；
- UI 不得依赖数据库结构或 Host 内部 DTO；
- 跨平台 DTO 必须通过 schema/fixture 契约测试；
- 平台命令必须在 allowlist 中并有输入大小、路径、账号和权限校验。

### 10.2 行为测试

- 每个 Use Case 的成功、失败、取消、超时和重试；
- 每个 Windows Use Case 都必须能追溯到 macOS 对应源码行为或明确批准的平台差异记录；不存在来源映射的逻辑视为验收失败；
- 启动时 UI 快于 Host、Host 快于 UI、Host 崩溃和 IPC writer/reader 退出；
- 任务运行中切换项目、切换页面和切换账号；
- 冷启动恢复 Conversation、Run、Task Graph 和审批；
- 分页、并发新事件、滚动锚点和最新执行记录；
- 附件授权引用、丢失文件、超大文件和取消上传；
- 终端断线、进程退出、resize、大输出与句柄失效。

### 10.3 视觉验收

- macOS 运行界面截图作为 goldens；
- Windows 在固定分辨率、100%/125%/150% 缩放下截图；
- 对布局、层级、材质、颜色、字体、图标、圆角、阴影、状态、动效、可读性和交互热点逐项对照；
- 产品自有 UI 中出现 Fluent、Material、WinUI 默认样式、浏览器默认控件样式或普通后台模板风格，直接判定视觉验收失败；
- 除操作系统无法重绘的安全界面外，Windows 页面必须保持与 macOS 一致的苹果主题；不同分辨率和缩放比例只允许响应式布局差异，不允许主题分叉；
- 弹窗必须有可辨识边界、遮罩、阴影、焦点和键盘关闭行为；
- loading 必须有终止条件，禁止无限转圈。

### 10.4 性能基线

在阶段 0 记录 macOS 当前数据，Windows 以不劣于该体验为目标：

- 冷启动到 Shell 可交互；
- Host ready 时间；
- 首屏 Conversation 可见时间；
- 万级消息滚动与内存；
- 项目切换、Run Inspector 更新延迟；
- 大目录、Git Diff 和终端大输出；
- 8 小时空闲与持续任务稳定性。

具体阈值必须由实测基线确定，不在没有测量前虚构数字。

## 11. 可行性评估

### 11.1 结论

方案可行，且比继续修补旧 C# 客户端风险更低。关键原因：

- Rust Host 已承担大量跨平台业务和持久化能力；
- macOS 已提供完整产品行为与真实页面基准；
- Tauri 可以复用 Rust 能力，并把原生权限控制放在前端之外；
- React 适合快速重建复杂工作区、时间线、任务详情和多面板 UI；
- 最大工作量是 macOS 解耦、契约稳定和逐页行为复现，不是基础技术不可行。

### 11.2 主要风险

| 风险 | 影响 | 控制措施 |
| --- | --- | --- |
| macOS 行为未形成契约 | Windows 出现“看起来像但行为不同” | 每页先建行为基线包和用例契约 |
| 一次性重构范围过大 | 长期无可交付版本 | 垂直切片迁移，每片完成即删旧入口 |
| IPC/Host 生命周期不稳定 | 启动报错、无限 loading、假健康 | 统一状态机、握手、超时、传输联动退出和恢复测试 |
| React 获得过多本机权限 | 安全边界失控 | Tauri 最小 capability + typed allowlist |
| Terminal/大消息性能 | 卡顿和内存增长 | 有界缓冲、虚拟列表、增量事件、压力测试 |
| 双客户端同步演进 | 功能逐渐分叉 | 共享 Host 契约、fixture 和页面行为矩阵 |
| 过早追求全部功能 | 核心会话迟迟不可用 | 严格按 P0→P4，先交付核心闭环 |

### 11.3 工作量判断

这是一次产品级客户端重建，不应按“把 Swift 页面翻译成 React”估算。合理拆分是：

- macOS 架构地基与核心切片：高工作量、高优先级；
- Host 契约与 Windows lifecycle 样板：中高工作量、最高技术风险；
- Windows 核心页面：高工作量，可在契约稳定后并行推进；
- Terminal、截图、宠物和全局能力：中高工作量，平台差异明显；
- 发布、安装、更新与签名：中等工作量，但必须预留真实设备验证时间。

只有在阶段 0 的页面矩阵和性能基线完成后，才能给出可信的人日/迭代排期。任何在此之前的精确工期都是猜测。

## 12. 首批执行清单

1. [已完成] 删除旧 `clients/windows` 及所有构建引用，并以独立提交完成。
2. [进行中] 为当前 macOS 主窗口、创建项目弹窗、Conversation、任务动态和执行详情采集最新基线。
3. [进行中] 输出页面—用例—接口—平台能力矩阵；Conversation 与 Run Inspector 为首批源码映射。
4. [已完成] 建立 Host v44 health fixture 与 Rust/Swift/TypeScript 契约验证原型。
5. [进行中] 在 macOS 建立 Application Port 和 Composition Root；Host runtime 状态、项目 Conversation 准备用例/per-project lifecycle、Conversation Session factory 与 Run Inspector 分页规则已提取。
6. [已完成] Conversation + Run Inspector 的 macOS 分层、Application use case 路由、typed lifecycle、子功能 composition 与 Rust/Swift/TypeScript 共享 fixture 已完成，并通过最终 React 准入验收。
7. [已完成] 已创建 Tauri 2 + React 19 工程骨架并接入 Host lifecycle；账号 control-plane bootstrap、Conversation History、Composer 文本/附件/显式能力与运行时控件、Realtime、Run Inspector、Ask User 与 Human Approval durable queue/decision 核心闭环已完成。
8. 通过真实 macOS/Windows 对照验收后，再进入下一批页面。

## 13. 完成定义

项目只有同时满足以下条件才算完成：

- 旧 `clients/windows` 及专属基础设施已从仓库删除；
- macOS UI 与副作用实现之间存在可测试的 Application/Port 边界；
- Windows React 不包含旧 C# 代码或以旧客户端为依据的行为；
- macOS 与 Windows React 的全部产品自有 UI 使用同一套苹果主题 Token、组件规范和动效语言；
- 两个平台通过同一 Rust Host 契约完成核心 Conversation、Run、Task、Approval 和恢复流程；
- Windows 核心页面通过当前 macOS 真实运行行为的逐页验收；
- 启动、断线、崩溃、取消、重试、账号切换和升级均有自动测试；
- 没有无限 loading、假健康 Host、过期执行详情或 UI 直接访问敏感资源；
- 安装包可在干净 Windows 设备上完成安装、升级、运行和卸载。
