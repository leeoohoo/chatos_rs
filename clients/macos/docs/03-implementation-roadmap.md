# 实施路线与里程碑

## 总策略

采用“协议冻结 → SwiftUI 壳 → 客户端主工作区垂直切片 → Connector Core 分模块替换 → 全量切换”的路线。浏览器 Web 端保持独立，不进入 Swift 迁移范围。每一阶段都必须能运行、能测试、能回退，避免一次性重写造成长期不可用分支。

## Phase 0：基线与协议冻结（1–2 周）

交付：

- 建立 Xcode workspace、Swift Package 边界、CI 和签名配置。
- 从现有服务 OpenAPI/TypeScript facade 生成接口清单。
- 收集登录、会话、任务、文件、终端、Connector Relay 的脱敏 JSON fixture。
- 建立 Rust/Swift 双实现回放测试。
- 固定页面信息架构、设计 token 和关键交互原型。

退出标准：Swift 测试可以读取 fixture，CI 能构建空壳 App，协议变更有 diff gate。

## Phase 1：原生 App Shell 与认证（2–3 周）

交付：

- 登录/注册、Keychain Token、自动续期和登出。
- `NavigationSplitView` 主窗口、联系人/项目/终端/远端资源壳、全局错误与离线状态。
- Local Connector 设置窗口和菜单栏状态。
- 建立 `ConversationHistoryStore`、稳定 Turn 模型、磁盘缓存与滚动锚点，但暂不接入完整富内容。
- Light/Dark、Dynamic Type、键盘导航、VoiceOver 基线。

退出标准：用户能登录、看到真实项目/会话列表、打开空的功能工作面；无明文 Token。

## Phase 2：会话与任务主路径（4–6 周）

交付：

- 会话创建、选择、分页消息、发送、流式响应和重连。
- 聊天历史必须先通过 [专项架构与验收矩阵](./06-chat-history-architecture.md)，再允许接入任务卡片和复杂工具事件。
- Markdown、代码块、工具调用、Ask User、任务卡片。
- Computer Use / Browser MCP 的客户端实时画面浮层；只消费 Plugin visual-session，不重写浏览器或 Plugin 本体。
- 会话摘要、Runtime Context Inspector。
- 任务列表、DAG、Run 详情、停止、补充引导、重试和阻塞恢复。

退出标准：真实项目可完成一次“提问 → 工具执行 → 人工确认 → 最终回答”的闭环。

## Phase 3：项目工作区（4–6 周）

交付：

- 项目创建与 Local Connector 工作区选择。
- 文件树、搜索、预览、编辑、保存、创建、移动、删除。
- Git 状态、Diff、stage、commit、branch、compare。
- Requirement、Project Plan、Team、运行设置。
- 本地终端第一版。

退出标准：可以在 Swift 客户端中完成一个小型代码修改并提交 Git。

## Phase 4：Local Connector Swift Core 基础（5–7 周）

交付：

- 设备注册、桌面票据、状态存储、WebSocket、心跳和重连。
- 平台签名校验与 replay protection。
- Connector 工作区句柄、路径边界、文件/Git relay；macOS bookmark 仅在 App Sandbox 需要时由 Core 内部维护。
- PTY、terminal exec、历史和审批挂钩。
- 真实服务下与 Rust Core shadow comparison。

退出标准：文件、Git、终端协议 fixture 100% 通过；真实服务可使用 Swift Connector 完成主路径。

## Phase 5：权限、审批与远程能力（4–6 周）

交付：

- 系统权限检查与引导。
- Permission layer、Sandbox lease、本地进程限制。
- 风险识别、白名单、Session Approval、待审批通知。
- Remote Terminal、SSH/SFTP。

退出标准：高风险命令不会绕过审批；离线、超时、拒绝和权限不足均有可恢复 UI。

## Phase 6：插件运行时（6–9 周）

交付：

- Marketplace、安装、校验、回滚和自动更新。
- npm MCP/Skill/Command/Hook/UI 加载。
- MCP stdio/HTTP、OAuth、Keychain Secret、文件授权和 Artifact。
- 隔离的插件 Web UI bridge。

退出标准：选定的官方插件矩阵与 Rust 客户端行为一致，崩溃插件不会拖垮主 App。

## Phase 7：切换、打磨与发布（3–5 周）

交付：

- 数据迁移、升级/降级策略、崩溃恢复。
- 性能、内存、能耗、长会话和多终端压力测试。
- notarization、Sparkle/系统更新渠道、隐私说明。
- 删除 macOS 发行包中的 Electron 和 Rust Core 依赖。

退出标准：连续两个发布候选版本达到功能矩阵，P0/P1 缺陷清零，可从旧客户端平滑迁移。

## 优先级

### P0

- 登录、项目、会话、流式消息。
- 聊天历史的顺序、分页、切换恢复、实时合并与滚动稳定性。
- 任务执行与人工确认。
- 工作区文件/Git/终端。
- Connector 连接、安全签名、路径边界、Keychain。

### P1

- Project Plan、DAG、Memory、Notes、Agent/Model 设置。
- Remote Terminal/SFTP。
- 命令审批 Agent 和高级权限策略。

### P2

- 全量插件 Marketplace、插件 UI、复杂 Mermaid/数学内容。
- iPadOS 客户端主工作区。

## 团队切分建议

| 轨道 | 主要责任 | 与其他轨道的契约 |
| --- | --- | --- |
| App/UI | DesignSystem、Shell、页面 Feature | 只依赖 domain service protocol |
| Cloud API | OpenAPI、Auth、Session、Task、Realtime | 提供 Sendable domain model |
| Connector Core | WebSocket、relay、workspace、terminal | 提供 actor API 与 fixture |
| Security/Plugin | Keychain、签名、权限、插件 | 建立威胁模型与专项测试 |
| QA/Release | fixture、快照、E2E、性能、签名 | 维护迁移功能矩阵 |

## 首个 30 天执行清单

1. 建立 `ChatOS.xcworkspace` 和上述 Package 空骨架。
2. 录制 20 组核心 API fixture，禁止包含真实 Token、密码和绝对路径。
3. 实现 Auth、真实资源 Sidebar、项目四 Tab 壳和 Connector 状态 Mock。
4. 接入真实项目/会话只读列表。
5. 实现一条真实会话的分页消息和 SSE/WebSocket 流。
6. 建立视觉快照、无障碍审计和协议兼容 CI。
7. 输出 Phase 2 详细 backlog，再开始写 Connector Core。

## 暂不做

- 不在第一阶段承诺 Windows/Linux Swift 客户端。
- 不在 SwiftUI 壳完成前迁移插件运行时。
- 不用 WebView 重新包装现有 ChatOS 页面冒充 Swift 重写。
- 不保留 Electron `WebContentsView` 的远端主页面模式，也不引入等价的 `WKWebView` 主壳。
- 不修改或用 Swift 重写现有浏览器 Web 端，也不重写 Browser MCP / Computer Use Plugin 本体。
- 不把现有 Rust 状态文件直接绑定到 Swift model；通过显式迁移器导入。
