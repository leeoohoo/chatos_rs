# Swift 目标架构

## 平台与工程基线

- 平台：macOS 14+，优先适配当前 macOS；不在本项目中改写浏览器 Web 端。
- 语言：Swift 6 严格并发。
- UI：SwiftUI + Observation；AppKit 仅用于系统没有等价 SwiftUI 控件的底层承载。
- 并发：async/await、AsyncSequence、actor；禁止把全局状态塞进单例。
- 网络：URLSession、URLSessionWebSocketTask；OpenAPI 生成 DTO，手写 domain adapter。
- 存储：SQLite 保存聊天历史与可恢复 UI 状态；Keychain 保存 Token 和 Secret；若 App Sandbox 需要访问本机路径，security-scoped bookmark 只作为 Connector Core 内部实现细节，不新增“工作区授权”产品页面。
- 测试：Swift Testing、XCTest UI、SnapshotTesting、协议 fixture、端到端 smoke test。

## 应用形态

一个 `ChatOS.app`，包含三个 Scene：

1. `MainWorkspaceScene`：ChatOS 主窗口。
2. `LocalConnectorSettingsScene`：本机连接、插件、权限与审批。
3. `MenuBarExtra`：连接状态、待审批数量、快速暂停、打开窗口。

macOS 客户端主工作区不加载现有 `127.0.0.1:8088` 或线上 ChatOS Web 页面。Swift 客户端直接调用同一套后端 HTTP/Realtime API，并用自己的 domain model 与 SwiftUI Feature 展示。浏览器 Web 端继续由现有 Web 工程维护。

主窗口忠于现有资源路由，不设置虚构 Dashboard：

```text
NavigationSplitView
├── Resource Sidebar
│   ├── Contacts
│   ├── Projects
│   ├── Terminals
│   └── Remotes
└── Resource Workspace
    ├── Contact Conversation
    ├── Project
    │   ├── Files
    │   ├── User Messages
    │   ├── Plan / Requirement
    │   └── Run Settings
    ├── Local Terminal
    └── Remote Terminal / SFTP

Inspector 仅在 Task Graph、Run、Git 比较等明确的主从详情场景按需出现。
```

## Package 划分

建议 Xcode workspace 内使用本地 Swift Package：

| Package | 责任 |
| --- | --- |
| `ChatOSApp` | App Scene、菜单、窗口、路由 |
| `DesignSystem` | Token、组件、图标、动效、空状态 |
| `AppShellFeature` | 联系人/项目/终端/远端资源栏、Toolbar、资源路由 |
| `AuthFeature` | 登录、注册、Token 刷新、桌面票据 |
| `ConversationFeature` | 会话、消息、Composer、流式事件、工具卡片 |
| `ProjectFeature` | 项目、文件、搜索、编辑、Git、运行设置 |
| `TaskFeature` | Requirement、Plan、DAG、Run、审批与阻塞状态 |
| `MemoryFeature` | Summary、Recall、Runtime Context、Notes |
| `SettingsFeature` | Agent、Model、System Context、用户设置 |
| `ConnectorFeature` | Local Connector 设置 UI 与状态编排 |
| `VisualSessionFeature` | Computer Use、Browser MCP 等 Plugin 实时画面、画中画状态与窗口层级 |
| `ConnectorCore` | 设备注册、WebSocket、relay dispatcher、本机状态 |
| `WorkspaceKit` | Connector 工作区句柄、路径安全、文件、Git、目录操作 |
| `TerminalKit` | PTY、会话、终端渲染、远程终端 |
| `PluginRuntimeKit` | 插件安装、校验、MCP、OAuth、Secret、Artifact |
| `ApprovalKit` | 风险识别、规则、人工审批、历史、白名单 |
| `APIClient` | HTTP、WebSocket、DTO、认证重试、错误映射 |
| `PersistenceKit` | Keychain、SQLite/SwiftData、迁移、缓存 |
| `ProtocolFixtures` | Rust/Swift 共用 JSON fixture 和回放工具 |

## 状态与依赖注入

- Feature 级状态使用 `@Observable @MainActor` model。
- 网络、数据库、文件、终端、插件等副作用通过 protocol 注入。
- 长生命周期连接放入 actor，例如 `ConnectorSessionActor`、`ChatStreamActor`。
- View 不直接构造 URL、读 Keychain 或启动 Process。
- 所有服务返回 domain model，避免 UI 依赖 OpenAPI 生成结构。

### 第一版代码组织约束

当前 Swift Package 先按 `ChatOSCore` 与 `ChatOSApp/Features/*` 落地，后续功能稳定后再拆为独立 Package。拆包前仍执行以下约束：

- 页面容器只组合布局和路由，不直接维护历史合并、终端进程或网络状态。
- 会话草稿、选中 Turn、未读状态与 Store 快照由 `ConversationSessionViewModel` 管理。
- 聊天事实只经 `ConversationHistoryStoring` 写入 `ConversationHistoryStore` actor，View 不直接增删消息数组。
- IO 实现通过 protocol 注入；当前终端的 `ShellCommandExecutor` 只是可替换原型。
- 复杂页面按 Feature/组件拆文件，Swift 源文件原则上控制在约 200 行以内；超过时优先拆状态、组件或服务，而不是继续扩展页面文件。

## Local Connector Swift 内核

### 连接状态机

```text
signedOut
  -> exchangingTicket
  -> registeringDevice
  -> syncingConfiguration
  -> connectingWebSocket
  -> online
       -> reconnecting(backoff + jitter)
       -> authenticationExpired
       -> leaseRejected
```

`ConnectorSessionActor` 负责：

- 单一 WebSocket 生命周期。
- 15 秒 heartbeat。
- 指数退避和网络恢复。
- 平台配置定时同步。
- Relay request 并发调度和 response correlation。
- 插件/OAuth 状态上报。

### 本机 IPC

最终 App 内部优先使用 actor/service 直接调用，不再依赖浏览器式 HTTP。为兼容调试和迁移，可保留一个只监听 Unix Domain Socket 的 `CompatibilityAPI`，TCP loopback 默认关闭。

### PTY

Swift 标准库没有完整 PTY API。建议增加极小的 C target：

- 封装 `forkpty`、`ioctl(TIOCSWINSZ)`、signal 和 fd 生命周期。
- Swift `TerminalSessionActor` 只接收类型安全事件。
- 交互终端启动用户 shell 并原样转发 PTY 输入；一次性 terminal exec 才以 argv 传递，默认不经过 shell expansion。
- 输出采用有界 ring buffer，保留截断标志。
- 每个终端独立保存 viewport、snapshot/history cursor、命令历史和连接状态；切换资源或断线重连不能创建重复会话。
- 渲染使用 AppKit-backed glyph view 或 Metal，不用 `TextEditor` 模拟终端；必须支持 ANSI、IME、文本选择、复制粘贴、搜索、链接/路径识别和 resize。
- SwiftUI 负责 Terminal Tab、精简 Toolbar、inline 错误恢复和系统主题同步；不实现独立命令历史侧栏。

### 工作区安全

- 产品层沿用当前单个 Connector workspace + relative path 模型；创建项目和终端时由云端列出 Connector 可见目录。
- 若 Swift/macOS 沙箱要求 bookmark，由 Connector Core 在设备内透明维护，不能把实现细节误画成现有业务能力。
- 每次操作对 root 和 target 做 standardized URL、`realpath` 与 volume 检查。
- create 场景校验真实父目录；symlink 目标必须仍在 root 内。
- Git 与 Process 的 cwd 只能来自已验证的 workspace handle。

### 凭据

- Access Token、OAuth Refresh Token、插件 Secret：Keychain access group。
- 非敏感状态：SQLite/SwiftData。
- 不在日志、崩溃报告、通知或 UI 快照中展示 Secret。

## 客户端主工作区网络层

将现有 TypeScript facade 按领域变成 Swift protocol，而不是复制一个巨型 ApiClient：

```swift
protocol SessionService: Sendable {
    func sessions(projectID: Project.ID?) async throws -> [Session]
    func messages(sessionID: Session.ID, cursor: String?) async throws -> Page<Message>
    func send(_ draft: MessageDraft, to sessionID: Session.ID) async throws -> SendReceipt
    func events(sessionID: Session.ID) -> AsyncThrowingStream<ConversationEvent, Error>
}
```

相同方式拆分 `ProjectService`、`TaskService`、`MemoryService`、`AgentService`、`ModelService` 和 `LocalConnectorService`。

## 富内容策略

| 内容 | 实现建议 |
| --- | --- |
| Markdown/GFM | 基于 AST 的 Swift Markdown 渲染，不注入任意 HTML |
| 代码高亮 | tree-sitter 或 Splash，缓存 token |
| Diff | 原生 SwiftUI 行模型 + 虚拟化 |
| Mermaid | 第一阶段服务端/隔离 WebView 渲染，后续原生常用图 |
| KaTeX/数学 | 隔离 WKWebView 或预渲染 SVG |
| Terminal | SwiftUI 容器 + AppKit-backed glyph view/Metal 渲染 |
| Plugin UI | 隔离 WKWebView，严格 origin、bridge 与权限清单 |

“SwiftUI 展示层”不等于禁止任何系统承载视图；重点是导航、状态、布局、交互和设计系统归 SwiftUI 所有，WebView 只承载不可安全原生化的受限内容。

这里的 WebView 例外不包含 macOS 客户端主工作区。客户端主工作区、Local Connector 设置和所有一级页面都必须原生实现；该限制不适用于独立存在的浏览器 Web 产品。

## 可观测性

- `Logger` 按 subsystem/category 分类。
- 每个 HTTP、WebSocket request 使用 correlation id。
- 敏感字段统一 redact。
- Connector 状态页展示最近心跳、重连次数、当前 workspace、插件进程和待审批数量。
- Debug 构建可导出匿名诊断包，Release 默认不采集命令正文。
