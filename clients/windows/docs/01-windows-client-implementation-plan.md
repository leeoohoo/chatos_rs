# ChatOS Windows 原生客户端实施方案

## 1. 目标与边界

本仓库实现 ChatOS 的 Windows 原生客户端，并与现有 macOS Swift 客户端保持产品能力、信息架构、视觉语言和交互语义一致。

核心原则：

- 不为共享源码而把 Swift 客户端重构为 Rust。
- Windows 客户端使用独立 C# 实现，后端 HTTP、Realtime、Connector 和插件协议保持一致。
- 主工作区、设置、审批、终端、文件、任务图和桌面宠物均使用 Windows 原生界面。
- 只有插件提供的受限网页内容允许进入 WebView2；一级产品页面不得用 Web 页面代替。
- 业务事实以服务端数据为准，本地数据库只保存缓存、UI 状态和可恢复的设备状态。
- macOS 与 Windows 的产品行为通过协议 fixture、状态机测试和能力矩阵对齐，而不是复制平台 UI 代码。

目标系统：

- Windows 11 为主要目标。
- Windows 10 版本 2004（10.0.19041）作为最低系统版本。
- x64 为首个发布架构，随后补齐 ARM64。

## 2. 技术选型

| 领域 | Windows 实现 |
| --- | --- |
| UI | C#、WinUI 3、Windows App SDK |
| 架构 | MVVM + 依赖注入 + async/await + `IAsyncEnumerable<T>` |
| HTTP | `HttpClient` + typed service clients |
| Realtime | `ClientWebSocket` + 单连接事件中心 |
| JSON | `System.Text.Json` source generation |
| 本地数据库 | SQLite (`Microsoft.Data.Sqlite`) |
| 凭据 | Windows Credential Manager / PasswordVault |
| 终端 | ConPTY + 原生终端渲染控件 |
| SSH/SFTP | SSH.NET，凭据仅从系统凭据库读取 |
| 文件监控 | `FileSystemWatcher` + 有界去抖队列 |
| 插件进程 | `System.Diagnostics.Process` + Job Object |
| 插件 UI | WebView2，严格 origin、CSP、bridge allowlist |
| 日志 | `Microsoft.Extensions.Logging`，统一敏感字段清理 |
| 打包 | MSIX；开发环境支持 unpackaged 启动 |
| 测试 | xUnit、协议 fixture、状态机测试、WinAppDriver/Windows Application Driver 替代方案 |

## 3. 解决方案结构

```text
ChatOS.Win.sln
├── src/ChatOS.Core
│   ├── Domain
│   ├── State
│   ├── Abstractions
│   └── Localization
├── src/ChatOS.Api
│   ├── Http
│   ├── Realtime
│   ├── Dtos
│   ├── Mapping
│   └── Services
├── src/ChatOS.Connector
│   ├── Device
│   ├── Workspace
│   ├── Terminal
│   ├── Git
│   ├── Remote
│   ├── Plugins
│   ├── Approval
│   ├── Persistence
│   └── Security
├── src/ChatOS.Desktop
│   ├── AppShell
│   ├── DesignSystem
│   ├── Features
│   ├── Pet
│   ├── Settings
│   └── Assets
├── tests/ChatOS.Core.Tests
├── tests/ChatOS.Api.Tests
└── fixtures
```

### 3.1 ChatOS.Core

纯 `.NET 8` 项目，不引用 WinUI 或 Windows API，可在 macOS/Linux CI 执行测试。

责任：

- 联系人、项目、会话、消息、任务、Run、审批、Ask User、插件和宠物活动领域模型。
- 会话历史合并、Realtime 去重、Task Graph 归一化。
- 宠物活动优先级、展示、忽略、处理和过期状态机。
- 资源路由、错误分类、分页和状态恢复规则。
- 所有平台无关的 service interface。

### 3.2 ChatOS.Api

纯 `.NET 8` 项目，直接调用现有 ChatOS 网关。

责任：

- 认证、Workspace、Conversation、Attachment、Notepad、Plan、Task、Run、Project Run、Remote、Pet Inbox API。
- WebSocket ticket、用户级 Realtime、会话级 Realtime。
- HTTP 认证重试、401 失效、错误映射、correlation ID。
- DTO 与 Core domain model 隔离。
- 使用 Swift/Rust 已验证的 JSON fixture 做协议一致性测试。

默认网关：

```text
http://127.0.0.1:9080/api/chatos
```

支持通过 `CHATOS_API_BASE_URL` 覆盖。

### 3.3 ChatOS.Connector

目标框架为 Windows `.NET 8`，封装所有平台能力。

责任：

- Connector 设备注册、配置同步、心跳、重连和 Relay dispatcher。
- 工作区路径校验、文件读写、搜索、Git 和代码导航。
- ConPTY 会话、后台命令、进程树终止和输出 ring buffer。
- SSH/SFTP、远程终端和二次认证。
- 插件安装、哈希校验、manifest、stdio MCP、HTTP MCP、OAuth、artifact 和 visual session。
- 本机审批、风险识别、自动审批策略和审批历史。
- SQLite、Windows Credential Manager、诊断日志和升级迁移。

### 3.4 ChatOS.Desktop

WinUI 3 可执行项目。

责任：

- 主窗口、资源侧栏、页面路由和全局 Toolbar。
- 与 macOS 客户端一致的设计 token、排版、颜色、卡片、状态和空页面。
- 聊天、项目、文件、Plan、任务图、终端、远端和设置页面。
- 全局审批浮层、Visual Session 浮层和桌面宠物窗口。

## 4. 主窗口信息架构

Windows 版保持当前 macOS 客户端的资源模型，不增加虚构 Dashboard：

```text
NavigationView / 自定义 Split Shell
├── Resource Sidebar
│   ├── Contacts
│   ├── Projects
│   ├── Local Terminal
│   └── Remote Connections
└── Resource Workspace
    ├── Contact Conversation
    ├── Project
    │   ├── Files
    │   ├── Messages
    │   ├── Plan / Requirements
    │   └── Run Settings
    ├── Local Terminal
    └── Remote Terminal / SFTP
```

全局 Toolbar 始终提供：

- 刷新资源。
- 创建资源。
- 展开/收起侧栏。
- 打开记事本。
- 账号、设置和退出登录。

## 5. 视觉与交互一致性

设计基线来自当前 macOS 客户端，而不是照搬 macOS 系统控件外观。

### 5.1 设计 token

- 背景：浅灰工作区、白色内容层、弱边框。
- 强调色：ChatOS 蓝色；AI/推理使用紫色；成功绿色；警告橙色；失败红色。
- 圆角：紧凑控件 6–8，卡片 10–12，浮层 14–18。
- 间距：4/8/12/16/20/24 的离散体系。
- 字体：Segoe UI Variable；中文回退 Microsoft YaHei UI。
- 正文、辅助文字、标题和代码字体分别建立 token，页面不得自行写死字号。
- 动画遵循系统 Reduce Motion 设置。

### 5.2 页面规则

- 侧栏选中态只覆盖资源行，不覆盖分组标题。
- 设置页面使用与常规页面一致的分组、留白、字号和卡片体系。
- 聊天输入区固定在底部，历史区域独立虚拟化滚动。
- 单任务详情按内容自适应高度，不使用固定大空白。
- 状态变化必须同时有文字、颜色和图标，不依赖单一颜色表达。
- 中英文切换覆盖全部一级页面、菜单、空状态、错误和辅助功能标签。

## 6. 功能实施顺序

### 阶段 A：工程和设计基线

- 建立解决方案、项目依赖、统一编译属性和测试项目。
- 建立设计 token、基础控件和窗口 Shell。
- 实现登录壳、资源侧栏和页面路由。
- 建立中英文资源文件与语言切换。
- 建立应用配置、日志和依赖注入。

完成标准：Windows 上可启动、可登录、可切换空资源页面；Core 测试可跨平台执行。

### 阶段 B：API 与 Realtime 核心

- 实现认证和凭据保存。
- 实现 Workspace、联系人、项目和会话 API。
- 实现统一 Realtime Event Center。
- 实现会话历史分页、发送、停止、附件和 Ask User。
- 实现错误映射、断线重连、事件去重和认证过期。

完成标准：联系人和项目聊天可完整收发，重启后恢复正确，断线不会产生重复消息。

### 阶段 C：项目工作区

- 文件目录、搜索、预览、编辑和保存。
- Git 状态、Diff、历史、分支和提交工作流。
- Plan、Requirement、范围预览、启动和停止。
- Task Graph、Task Detail、Run 过程和阻塞处理。
- 项目运行分析、环境、目标和实例管理。

完成标准：项目四个工作面与 macOS 行为一致。

### 阶段 D：Windows Connector

- 设备注册、配置同步和本机连接状态。
- 工作区安全边界和路径操作。
- ConPTY 本机终端和 Relay terminal execution。
- SSH/SFTP 与远程终端。
- 插件下载、安装、校验、stdio/HTTP MCP、OAuth 和 artifact。
- Visual Session、Browser 和 Computer Use 画面桥接。

完成标准：后端能够通过 Windows Connector 执行与 macOS 相同的本机工具能力。

### 阶段 E：设置与审批

- Connector 连接、模型、插件、沙箱、权限和审批设置。
- 全局审批浮层和审批历史。
- Windows 权限诊断与可操作引导。
- 记事本、界面语言、字号和宠物设置。

完成标准：设置能力和审批闭环完整，不需要跳回 Web 客户端。

### 阶段 F：全局桌面宠物

- 透明、无边框、置顶、跨虚拟桌面的宠物窗口。
- v2 spritesheet 九行动画、左右方向和拖动跑步。
- 宠物消息面板、任务列表和内容自适应尺寸。
- 审批、Ask User、阻塞、失败、完成和运行中任务直接处理。
- Pet Inbox 作为云端消息唯一事实来源。
- 忽略、已处理、完成反馈、时效性和重启恢复。
- 单击宠物打开叽咕狸与常用项目快捷聊天。

完成标准：任务处理后不会复活；完成和阻塞保留查看机会；运行中任务可取消；审批和 Ask User 可原地完成。

### 阶段 G：发布与验收

- x64/ARM64 构建。
- MSIX 签名、安装、升级和卸载。
- 崩溃恢复、睡眠唤醒、多显示器、DPI 和触控测试。
- 协议 fixture、Core/API、Connector 集成和 UI 自动化。
- 与 macOS 能力矩阵逐项验收。

## 7. 状态与数据原则

### 7.1 会话

- 服务端消息和 Realtime 是聊天事实来源。
- 本地 SQLite 保存分页缓存、草稿、滚动位置和未完成 UI 状态。
- optimistic message 必须通过稳定 identity 与服务端消息合并。
- 任务回调不得覆盖真实 assistant reply。

### 7.2 宠物活动

- 云端宠物消息只读取 `pet_activity_inbox`。
- 不从聊天历史或任务图重新生成已处理消息。
- 本地处理时先做活动版本级抑制，接口失败再恢复。
- `activity_key + activity_version` 是展示去重身份。
- 本机审批不经过云端 inbox，但使用相同展示模型。

### 7.3 本地存储

SQLite 表至少包括：

- `conversation_cache`
- `conversation_cursor`
- `ui_state`
- `pet_preferences`
- `connector_state`
- `plugin_runtime_state`
- `terminal_session_snapshot`
- `diagnostic_event`

Token、私钥、SSH 密码和插件 Secret 不进入 SQLite。

## 8. Windows 平台关键实现

### 8.1 桌面宠物窗口

- 使用 WinUI 3 `AppWindow` 创建独立无边框窗口。
- 通过 Win32 interop 设置透明、置顶、工具窗口和非激活行为。
- 空白透明区域启用 click-through，宠物和面板区域保留命中测试。
- 拖动使用屏幕物理像素计算，统一 DPI 转换，避免重影和鼠标滞后。
- 宠物与消息面板共享同一位置状态，移动时同帧更新。

### 8.2 ConPTY

- 每个终端独立 pseudo console、输入管道、输出管道和 Job Object。
- resize 使用 `ResizePseudoConsole`。
- 关闭时终止完整进程树，不能遗留 shell 或插件子进程。
- 输出使用有界 buffer，并保留截断状态。

### 8.3 插件隔离

- 插件安装目录按 plugin/release/content hash 隔离。
- 启动前校验 manifest、文件哈希、平台和架构。
- 使用 Job Object 限制进程生命周期。
- workspace 权限绑定到 Connector 注册的 workspace，不接受插件自行声明路径。
- device-only 插件不得获得 workspace 环境。

## 9. 测试与验收

每个能力至少包含：

- Domain 状态机单元测试。
- Swift/Rust JSON fixture 兼容测试。
- HTTP request path、method、body 和错误映射测试。
- Realtime 断线、乱序、重复和恢复测试。
- Connector 路径逃逸、symlink、进程终止和权限测试。
- Windows UI 可访问性与关键流程自动化。

提交门禁：

- `dotnet test` 全部通过。
- Windows x64 Debug/Release 构建通过。
- 无未本地化的用户可见字符串。
- 无明文凭据或绝对用户路径。
- 能力矩阵对应项更新。

## 10. 当前限制

当前开发机器是 macOS，已通过仓库外临时目录安装 .NET 8 SDK 用于验证：

- `ChatOS.Core`、`ChatOS.Api` 自动化测试已可在 macOS 执行。
- `ChatOS.Connector` 已使用 Windows targeting pack 在 macOS 完成 C# 编译。
- WinUI、ConPTY、Credential Manager、MSIX 和真实桌面宠物必须在 Windows 11 构建机验证。
- WinUI 在 macOS 会停在 Windows 专用 `XamlCompiler.exe`，这不是源代码构建通过的证据。
- 仓库已提供 Windows bootstrap、build、test 和 package 脚本，避免依赖手工配置。
