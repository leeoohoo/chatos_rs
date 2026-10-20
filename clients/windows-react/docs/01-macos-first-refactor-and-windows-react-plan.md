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

远端连接等尚未在当前 macOS 形成完整可验收页面的能力，不凭空补做；先等待 macOS 产品行为确定，再进入 Windows。

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

Workspace Inspector 的详情、模型输出、Run 与事件分页 Task 已冻结请求目标和 Application use case，Host await 期间不再强持有 ViewModel；底层请求即使忽略取消也不会阻止弹层状态释放，返回后仍须通过现有 request token 与目标 identity 才能写回。

内嵌 Task Inspector 在 detail 加载中切回 process 时会重新挂起当前活跃 Task 的 polling；取消模型输出请求不能同时让执行过程停止刷新。

Conversation 执行过程读取已使用 Host `process` 事件载荷投影：SQLite 查询阶段只保留映射 UI 所需的 tool call identity、状态、原因与错误摘要，移除附件内容和工具参数；macOS 以每页 25 条、最多 400 页的既有 10,000 事件上限读取以控制内存与解码成本，超过 4 MiB 的逻辑消息由 IPC 自动分片，同时仍完整合并工具生命周期。

项目 Conversation 准备现已由 Core `ProjectConversationPort`、per-project token state 与 AppModel 持有的具体 Task 共同管理；项目删除只取消对应准备，账号切换、登出和应用退出取消全部准备，迟到结果不能恢复旧 loading 或错误。准备 Task 会在进入 Application use case 前冻结 owner、project、contact、account generation 与 port；Context/Host await 期间不再强持有 AppModel，返回后仍须联合验收账号、项目存在性和 per-project token。

项目 Workspace 刷新已用 Core `WorkspaceLoadState` 取代裸 generation，并把 registry、本地快照、设备配对、根目录修复、远端关系与 Run project 映射保持为既有的两阶段加载顺序。刷新 Task 在开始时冻结账号、account generation 与服务依赖，各段 IO 期间不再强持有 AppModel；每次快照准备、展示写回、错误和 loading 收尾都必须通过同一个 typed token，项目删除、账号切换、登出或新刷新会同时淘汰旧请求。

Local Agent 控制面 bootstrap 已新增绑定 owner、account generation、Host lifecycle generation 与 request ID 的 `LocalAgentControlPlaneBootstrapState`。本地快照恢复和 authenticated gateway bootstrap 共用无 AppModel 所有权的服务配置函数；API/Connector/各业务服务 await 只持有冻结 adapter，并在每一段配置后重新验收当前生命周期，只有最终同步提交会短暂恢复 AppModel。

Conversation Session 的服务组合已收口到 App 层 `ConversationSessionFactory`；AppModel 不再逐项构造 ViewModel 依赖，只负责 session 缓存、LRU 淘汰与激活策略，并在每次创建时传入当前账号对应的 runtime ready 状态。

Conversation 的模型、推理等级与远程连接 mutation Task 已冻结 session/请求参数，Host await 期间不再强持有 ViewModel；底层忽略取消时仍可释放会话状态，返回后继续以 mutation token 验收。

Conversation older-history 分页与 viewport/unread mutation 也已采用相同持有边界；远程 History 或本地 Store await 不会延长 ViewModel 生命周期，返回后仍须通过 activation、cursor 或 mutation token 才能更新展示。

Conversation 激活触发的 Ask User refresh 已拆分为请求句柄、无 ViewModel 等待与主线程 token 验收三段；Prompt Host 请求忽略取消时不再阻止 Conversation 释放。

Conversation bootstrap 与 runtime-ready recovery 现复用 runtime settings/Prompt 请求句柄并并发等待；模型或 Prompt Host 阻塞时不再整段持有 ViewModel，结果仅在对应 load/recovery token 仍有效时应用。

Conversation realtime Task 不再在整个 stream 生命周期内强持有 ViewModel，只在单个事件同步写回时恢复引用；Prompt realtime 事件等待 Host 时也使用请求句柄，因此失去最后一个 UI owner 后长连接会正常随 deinit 终止。

Ask User 提交或取消失败后的 Host 对账刷新已拆为请求句柄、无 ViewModel 等待与 action/refresh token 联合验收；底层 Prompt 请求忽略取消时不再留住 Conversation，也不能以迟到快照结束新操作。

全局活动定位 Ask User 前的 Prompt refresh 已改为 Conversation 显式持有的 latest-wins focus Task 与 typed token；后一次定位会淘汰前一次，Host 忽略取消时也不会因实例 async 方法而留住 Conversation。

远程连接列表已由 Core `RemoteConnectionListUseCase` 统一查询与排序，并使用绑定账号 generation 的 typed latest-wins token；AppModel 在 IO 期间不再强持有自身，账号切换或更新请求会拒绝旧列表、错误与 loading 写回。

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

当前状态：执行中。`clients/windows-react` 已建立 React 19 + TypeScript + Vite + Tauri 2 工程，Host lifecycle、Shell 与 Authentication 三个 P0 切片均通过 typed Port/Adapter 接入。Shell 只读取显式 lifecycle snapshot；Host 未就绪或握手失败时展示可恢复状态，不以无限 loading 隐藏错误。Tauri 已打包真实 Local Agent Host sidecar，完成 v44 framed health handshake、4 MiB frame/64 MiB message 限制、有界无 Secret 诊断及应用退出前的 Host 回收。自定义窗口框架保留拖拽、缩放、最小化、最大化和关闭能力，并完成浏览器视觉回归。Authentication 复用 macOS 的恢复、登录、注册、验证码倒计时与 logout/login 排序语义；Token 仅由 Rust 持有并写入 Windows Credential Manager，登录成功后才以 `user.id` 启动 Host。Windows CI 已覆盖前端架构审计、类型检查、测试、构建、Rust 测试、真实 Host 握手和 Tauri debug executable。Conversation 等尚未达到矩阵准入条件的业务页面仍未创建。

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
6. [进行中] 迁移 Conversation + Run Inspector 作为第一条完整垂直切片；Conversation bootstrap/send/attachment import/activation/realtime/runtime recovery/runtime settings load/viewport mutation/per-turn Graph availability、Graph、Run、section 刷新、Inspector request identity、Workspace 基础详情→section 顺序加载、内嵌 Inspector Task 复用、section polling 恢复与 presentation/retry identity、Application use case 路由、polling 决策、polling session 与 Workspace presentation session 生命周期已提取，其余 service orchestration 与 typed lifecycle port 尚未完成。
7. [进行中] 已创建 Tauri 2 + React 19 工程骨架并接入 Host lifecycle；继续完善 sidecar 打包、诊断和恢复，Conversation 在准入前不实现。
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
