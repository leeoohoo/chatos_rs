# 现有系统审计

> 本文记录原生客户端立项时对旧 React/Electron 客户端的历史审计。所列旧目录已在 ChatOS 3.0.0 中移除，不是当前实现入口。

## 审计范围

本次读取了以下主要区域：

- `chatos/frontend`：ChatOS 主 React 前端。
- `local_connector_client/frontend`：Electron 内的本机 React 设置界面。
- `local_connector_client/core`：Rust 本机 Core。
- `local_connector_service`、`user_service` 以及前端 API facade：确认登录、设备注册和云端编排边界。
- `docs/ui-redesign`：现有产品页清单，仅用于功能覆盖核对，不作为新视觉的直接模板。

本机开发服务确认正在监听：

| 服务 | 地址 | 作用 |
| --- | --- | --- |
| ChatOS Web | `127.0.0.1:8088` | 当前主前端 |
| User Service | `127.0.0.1:39190` | 登录与用户服务 |
| Local Connector Service | `127.0.0.1:39230` | 云端到设备的连接编排 |
| Local Connector Core | `127.0.0.1:39232` | 本机设置 API 与设备运行时 |

## 当前产品结构

现有桌面产品实际上有两层界面：

1. Electron 使用独立 `WebContentsView` 加载 ChatOS Web 主页面。
2. Electron 自己展示 Local Connector 本地设置页。

这两个界面背后的状态所有权不同：

| 领域 | 当前所有者 | Swift 重写后 |
| --- | --- | --- |
| Project / Session / Message / Task / Memory / Agent | 云端服务 | 保持云端事实源 |
| 工作区绝对路径与目录授权 | Local Connector | 保持设备本地 |
| 文件、Git、终端、SFTP | Local Connector | 保持设备本地 |
| 插件安装、MCP/Skill 进程、OAuth、凭据 | Local Connector | 保持设备本地 |
| 命令审批、白名单、审批历史 | Local Connector | 保持设备本地 |
| 系统权限 | macOS + Local Connector | 保持设备本地 |

## 桌面客户端需要原生复刻的 Web 功能面

`chatos/frontend` 不是单一聊天页。源码中已经形成以下工作面：

- 登录与注册。
- 会话列表、项目联系人、项目与终端入口。
- Agent 对话、流式消息、工具调用、任务卡片与 Ask User。
- 会话摘要、Memory 时间线、Runtime Context。
- 项目文件树、搜索、预览、编辑、Git 状态、Diff、提交与分支。
- Requirement、Project Plan、任务 DAG、任务 Run 详情与阻塞恢复。
- 本地和远程终端、SSH/SFTP。
- Agent、应用、模型、System Context、Notes、用户设置。
- 插件命令、插件 UI Workbench 和运行事件。

因此“客户端主工作区 Swift 化”应被当作完整桌面 IDE/协作工具，而不是聊天 UI 工程。现有浏览器 Web 端不在 Swift 项目范围内。

## Local Connector 功能面

Rust Core 当前承担的职责包括：

- 用户登录、桌面票据交换、设备注册与单设备租约。
- 工作区添加、移除、路径归一化、边界校验与项目配置信任。
- 只向外建立 WebSocket，15 秒心跳，并同步设备状态。
- 校验平台 Ed25519 签名、时间戳与 nonce，拒绝不可信远程控制消息。
- PTY 终端会话、一次性命令、输出截断、历史记录与远程终端。
- 本地文件、Git、目录、SFTP 请求路由。
- Plugin Marketplace 安装、校验、回滚、自动更新、Skill/MCP/Command/Hook/UI 加载。
- MCP stdio/HTTP 运行时、插件 Artifact 与文件授权。
- OAuth、插件凭据和平台 Keychain/DPAPI 安全存储。
- 命令风险识别、AI 审批 Agent、人工审批、白名单和 Session Approval。
- Sandbox/permission layer、租约、本机进程能力与托管要求。
- Accessibility、Screen Recording、Automation、网络和工作区权限检查。
- 本地 SQLite 状态库和迁移。

Local API 已覆盖状态、认证、工作区、命令历史、运行设置、Agent Prompt、系统权限、Sandbox、终端、模型、插件、OAuth/凭据和审批等路由。Swift 版必须对这些路由和 WebSocket 消息做逐项兼容测试。

## 必须保留的安全不变量

1. 云端永远不保存本机绝对路径，只保存逻辑 workspace id、alias 和 fingerprint。
2. 远程命令必须落在已授权工作区内，不能通过 `..`、符号链接或路径编码逃逸。
3. 设备控制消息必须验证平台签名、算法、key id、时间戳和 nonce。
4. Access Token、OAuth Refresh Token 和插件 Secret 不写入普通 JSON 或 UserDefaults。
5. Local Connector 离线时明确失败或等待，不能静默回退到云端文件系统。
6. 插件进程必须按授权暴露文件、网络、环境变量和凭据。
7. 高风险命令必须经过本地策略，不能由云端直接绕过审批。
8. Local API 只监听 loopback 或受保护的 IPC，并要求桌面鉴权 token。

## 重写难点

| 难点 | 原因 | 应对 |
| --- | --- | --- |
| WebSocket 协议兼容 | 消息类型多且存在异步回包 | 建立 JSON fixture 和双实现回放测试 |
| PTY 与终端语义 | 不是简单 `Process` 输出 | Darwin `forkpty` 小型 C shim + Swift actor |
| 插件运行时 | 包含 npm、MCP、OAuth、文件授权 | 独立 `PluginRuntimeKit`，最后阶段迁移 |
| 安全路径处理 | symlink、volume、bookmark、scope 复杂 | Security-scoped bookmark + `realpath` 双校验 |
| 富文本和代码展示 | Markdown、KaTeX、Mermaid、Diff、终端 | 原生 SwiftUI 为主，必要时封装 AppKit 控件 |
| 大会话性能 | 消息、工具事件、任务时间线很长 | 分页、虚拟化、增量解析、actor 隔离 |
| 跨平台差异 | Swift 方案天然偏 Apple | 明确 macOS 产品线；Windows/Linux 保留 Rust 客户端 |

## 当前建议

不要直接删除 Rust Core。先把它固定为“行为 Oracle”：同一组 fixture 同时喂给 Rust 和 Swift，结果一致后再替换对应模块。每完成一个垂直切片，就让真实服务在 Swift 客户端上跑一遍端到端流程。
