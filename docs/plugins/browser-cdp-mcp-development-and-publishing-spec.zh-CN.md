# Browser CDP MCP 开发、发布与安装规范

本文定义 Chatos Browser CDP MCP 的开发边界、参考实现、运行架构、权限模型、npm 交付格式、Marketplace 发布流程和端到端验收标准。目标产物必须能够像 Open Computer Use 一样，以签名 npm Release 发布到 Chatos Plugin Marketplace，由 Local Connector Client 下载、验证、安装并作为本地 stdio MCP 启动。

本文是 Browser CDP Plugin 的实施基线。实现不得以 Chatos 内置 BrowserTools、直接 MCP 配置、运行时 `npx @latest`、ZIP 包或未安装 fallback 作为生产执行路径。

通用 Plugin 发布规则见 [第三方 Plugin 发布与接入手册](./third-party-plugin-publishing.zh-CN.md)，用户和运维流程见 [Plugin 用户与运维手册](./plugin-operations-user-guide.zh-CN.md)。

## 1. 技术决策摘要

Browser CDP MCP 采用以下固定技术路线：

- 使用 Rust 开发跨平台原生 stdio MCP。
- 使用 `chromiumoxide` 作为受管 Chrome 的主要异步 CDP 实现基础。
- 参考 Playwright MCP 的工具设计、可访问性快照、会话 profile 和 Extension 连接模式。
- 参考 Puppeteer 的 Connection、CDP Session、Target Manager 和 Extension Transport 设计。
- 仅把 `rust-headless-chrome` 用作进程管理、同步事件模型、请求拦截和测试案例的补充参考。
- npm package 只包含 Node 跨平台 launcher 和已审核的原生二进制，不在安装后下载 Chrome 或其他代码。
- 同时支持“受管隔离浏览器”和“用户现有 Chrome”两种后端。
- 完整 CDP 通过一个通用 raw command 工具和有界事件订阅工具开放，不为每个 CDP method 创建一个 MCP tool。
- 高风险能力由签名 Manifest permission、工具级 policy 和 Local Connector 本机审批共同强制执行。
- 最终删除 Task Runner 对 `plugin.catalog.name == "browser"` 的内置 BrowserTools 注入逻辑。

## 2. 已克隆项目的参考优先级

以下数据在 2026-08-21 通过 GitHub API 和本地 clone 核对。Star 数只用于判断社区成熟度，不作为依赖选择的唯一依据。

| 优先级 | 项目 | GitHub | Star | 许可证 | 在本项目中的定位 |
| --- | --- | --- | ---: | --- | --- |
| P0 | Chromiumoxide | [mattsse/chromiumoxide](https://github.com/mattsse/chromiumoxide) | 1,368 | MIT OR Apache-2.0 | Rust 直接 CDP 主依赖 |
| P0 | Playwright MCP | [microsoft/playwright-mcp](https://github.com/microsoft/playwright-mcp) | 36,317 | Apache-2.0 | MCP 工具、快照、profile、Extension 产品设计参考 |
| P0 | Puppeteer | [puppeteer/puppeteer](https://github.com/puppeteer/puppeteer) | 95,474 | Apache-2.0 | CDP connection/session/target/extension transport 协议参考 |
| P2 | Rust Headless Chrome | [rust-headless-chrome/rust-headless-chrome](https://github.com/rust-headless-chrome/rust-headless-chrome) | 2,945 | MIT | 进程、WebSocket、事件和拦截的补充参考 |

本地研究目录：

```text
mcps/chromiumoxide
mcps/playwright-mcp
mcps/puppeteer
mcps/rust-headless-chrome
```

这些 clone 只作为研究材料，不应直接打进 Browser Plugin npm 包，也不应作为 Chatos 主仓库的嵌套 Git 依赖提交。

### 2.1 Chromiumoxide：主实现基础

应直接参考和使用的内容：

- `src/browser/mod.rs`：浏览器启动、连接和生命周期。
- `src/browser/config.rs`：Chrome executable、profile 和启动参数构造。
- `src/conn.rs`：WebSocket 连接。
- `src/handler/browser.rs`：浏览器级命令和事件分发。
- `src/handler/session.rs`：CDP session 管理。
- `src/handler/target.rs`：target 生命周期。
- `src/page.rs`：页面级高层 API 和任意 typed CDP command 执行。
- `chromiumoxide_cdp`、`chromiumoxide_pdl`：Chrome PDL 生成的完整 typed CDP 协议。
- `chromiumoxide_types::CdpEvent`：统一事件类型。

采用理由：

- 与 Chatos 主技术栈一致，都是 Rust/Tokio。
- 支持启动 Chrome，也支持连接已运行的 CDP endpoint。
- 提供完整生成式 CDP command/event 类型。
- handler stream 天然适合 MCP 进程内的后台事件泵。

禁止照搬的部分：

- 不启用 `fetcher` 在运行时下载 Chromium。
- 不直接把 library 的 Page/Element handle 暴露给 MCP 调用方。
- 不把 Chrome debugger endpoint 返回给 Agent。
- 不依赖从 Chrome 英文 stderr 解析调试端口作为唯一发现方式；优先使用 `DevToolsActivePort` 或 debugging pipe。

### 2.2 Playwright MCP：MCP 产品和工具设计参考

应参考的内容：

- 以 accessibility snapshot 为主要交互表面，而不是默认依赖截图。
- 页面元素使用 snapshot ref 定位，页面变化后刷新 ref。
- `browser_find` 只返回命中节点附近上下文，降低 token 使用。
- managed persistent profile、isolated profile 和 Chrome Extension 三种模式。
- action timeout、navigation timeout、settle timeout 分离。
- output directory、output size 上限、workspace file boundary。
- `browser_click`、`browser_type`、`browser_fill_form` 等高层工具的参数设计。
- read-only 和 mutating 工具分类。
- raw/unsafe 能力必须显式 opt-in，不能混入默认工具集。

本地 clone 是 npm 包包装仓库；其实际 MCP 源码由 `playwright-core` 提供，clone 中的 `src/README.md` 指向 Playwright monorepo的 `packages/playwright-core/src/tools/mcp`。

禁止照搬的部分：

- 不使用 `npx @playwright/mcp@latest` 作为 Chatos Runtime。
- 不在插件安装或首次调用时运行 `playwright install`。
- 不接受 unrestricted absolute file path。
- 不把“allowed origins”误当成完整安全边界；最终权限仍由 Local Connector 强制执行。
- 不开放类似 `browser_run_code_unsafe` 的 MCP 进程内任意 JavaScript/RCE 工具。

### 2.3 Puppeteer：CDP 内核和 Extension Transport 参考

应重点参考：

- `packages/puppeteer-core/src/cdp/Connection.ts`：请求 ID、回调注册、浏览器级消息和 session 消息复用同一 transport。
- `packages/puppeteer-core/src/cdp/CdpSession.ts`：session scoped command、detach、pending callback 清理。
- `packages/puppeteer-core/src/cdp/TargetManager.ts`：target discover、auto attach、target 生命周期。
- `packages/puppeteer-core/src/common/CallbackRegistry.ts`：请求超时、成功、协议错误和连接关闭处理。
- `packages/puppeteer-core/src/common/ConnectionTransport.ts`：把上层 Connection 与 WebSocket/Extension transport 解耦。
- `packages/puppeteer-core/src/cdp/ExtensionTransport.ts`：通过 `chrome.debugger` 转发 CDP，并为 Extension 缺失的 browser/target 行为提供兼容层。

采用方式：

- 只借鉴状态机、消息路由和错误处理结构。
- 在 Rust 中重新实现 transport trait、callback registry、session registry 和 target registry。
- Extension transport 必须接入 Chatos Local Connector Browser Bridge，不允许浏览器扩展直连 Task Runner 或 MCP Management。

禁止照搬的部分：

- 不把 Puppeteer/Node 运行时作为生产 Browser MCP 的依赖。
- 不复制 Puppeteer 内部未稳定 API。
- 不假设 `chrome.debugger` 能完整实现所有 browser-level CDP method；Extension backend 必须公开 capability differences。

### 2.4 Rust Headless Chrome：补充参考

适合参考：

- `src/browser/process.rs`：Chrome 进程和调试地址发现。
- `src/browser/transport/web_socket_connection.rs`：WebSocket 生命周期。
- `src/browser/transport/waiting_call_registry.rs`：同步 pending call registry。
- `src/browser/tab/mod.rs`：target 事件线程、request interception、键鼠输入、截图、Cookie 和文件选择器操作。

不作为主依赖的原因：

- 核心模型以同步 API 和线程为主，不如 Chromiumoxide 适合 Chatos 的 Tokio Runtime。
- README 明确列出 frames、file chooser、网络 timing、WebSocket inspection 等缺失项。
- 完整 CDP 和持久事件订阅仍需要大量额外封装。

## 3. 目标运行架构

```text
Task Runner
  -> MCP Management Runtime Session
  -> Local Connector Service Relay
  -> Local Connector Client PluginRuntimeHost
  -> npm bin/chatos-browser-cdp
  -> Rust Browser CDP MCP
       -> DirectCdpBackend -> managed Chrome CDP WebSocket
       -> ExtensionCdpBackend -> Local Connector Browser Bridge
                              -> Chrome Extension
                              -> chrome.debugger
```

职责边界：

- Plugin Management 是 Manifest、Catalog、Release、签名和撤销的控制面，不执行浏览器调用。
- Task Runner 只使用任务创建时固定的 plugin/release/component/tool snapshot。
- MCP Management 只准备和路由调用，不直接连接 Chrome。
- Local Connector Service 只做双向 Relay。
- Local Connector Client 验证、安装、授权、审批并启动 stdio MCP。
- Rust Browser MCP 管理 browser session、target、CDP session、事件队列和工具行为。
- Chrome Extension 只能和本机 Local Connector Browser Bridge 通信。

## 4. Browser Backend 抽象

Rust 代码必须通过统一 trait 隔离两种浏览器连接模式：

```rust
#[async_trait]
pub trait BrowserBackend: Send + Sync {
    async fn open(&self, request: OpenBrowserRequest) -> Result<BrowserDescriptor>;
    async fn list_targets(&self) -> Result<Vec<TargetDescriptor>>;
    async fn attach_target(&self, target_id: &str) -> Result<BackendSessionId>;
    async fn detach_target(&self, session_id: &BackendSessionId) -> Result<()>;
    async fn send_command(
        &self,
        session_id: Option<&BackendSessionId>,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value>;
    async fn subscribe(&self, filter: EventFilter) -> Result<EventStream>;
    async fn close(&self) -> Result<()>;
}
```

实现：

- `DirectCdpBackend`：封装 Chromiumoxide。
- `ExtensionCdpBackend`：封装 Browser Bridge transport。

上层 session、tool 和 policy 层不得依赖 Chromiumoxide 的具体 `Page` 或 Puppeteer 风格对象。

## 5. 推荐仓库结构

```text
chatos-browser-cdp/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── crates/
│   ├── browser-cdp-cli/
│   ├── browser-cdp-mcp/
│   ├── browser-cdp-core/
│   ├── browser-cdp-direct/
│   ├── browser-cdp-extension/
│   ├── browser-cdp-policy/
│   └── browser-cdp-protocol/
├── extension/
├── npm/
│   ├── package.json
│   ├── bin/chatos-browser-cdp
│   ├── dist/
│   ├── manifest/chatos-plugin.json
│   ├── sbom.cdx.json
│   ├── THIRD_PARTY_NOTICES.md
│   ├── README.md
│   └── LICENSE
├── scripts/
└── tests/
    ├── contract/
    ├── managed-browser/
    ├── extension-browser/
    ├── marketplace-install/
    └── security/
```

crate 职责：

| crate | 职责 |
| --- | --- |
| `browser-cdp-cli` | `mcp`、`doctor`、`version` 命令和进程入口 |
| `browser-cdp-mcp` | JSON-RPC、MCP lifecycle、tools/list、tools/call |
| `browser-cdp-core` | browser/session/tab/CDP session/event queue/element ref |
| `browser-cdp-direct` | Chromiumoxide direct CDP backend |
| `browser-cdp-extension` | Browser Bridge 和 Chrome Extension backend |
| `browser-cdp-policy` | URL policy、权限、审批摘要、敏感信息和日志脱敏 |
| `browser-cdp-protocol` | 各 crate 和 Extension Bridge 共享的稳定数据结构 |

## 6. npm 交付结构

发布后的 npm `.tgz` 解包内容必须类似：

```text
package.json
bin/chatos-browser-cdp
dist/macos/arm64/chatos-browser-cdp
dist/macos/x64/chatos-browser-cdp
dist/linux/arm64/chatos-browser-cdp
dist/linux/x64/chatos-browser-cdp
dist/windows/arm64/chatos-browser-cdp.exe
dist/windows/x64/chatos-browser-cdp.exe
manifest/chatos-plugin.json
sbom.cdx.json
THIRD_PARTY_NOTICES.md
README.md
LICENSE
```

规范 `package.json`：

```json
{
  "name": "chatos-browser-cdp",
  "version": "1.0.0",
  "description": "Full Chrome DevTools Protocol browser control MCP for Chatos.",
  "license": "Apache-2.0",
  "homepage": "https://github.com/chatos-ai/chatos-browser-cdp",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/chatos-ai/chatos-browser-cdp.git"
  },
  "bugs": {
    "url": "https://github.com/chatos-ai/chatos-browser-cdp/issues"
  },
  "keywords": ["browser", "cdp", "chrome", "chromium", "mcp", "chatos"],
  "type": "commonjs",
  "engines": {
    "node": ">=18"
  },
  "publishConfig": {
    "access": "public"
  },
  "bin": {
    "chatos-browser-cdp": "bin/chatos-browser-cdp"
  },
  "files": [
    "bin/",
    "dist/",
    "manifest/",
    "sbom.cdx.json",
    "THIRD_PARTY_NOTICES.md",
    "README.md",
    "LICENSE"
  ]
}
```

硬性要求：

- 不允许 `postinstall`、`install`、`prepare` 等安装阶段脚本。
- 不允许安装时下载 Chrome、Node package、动态库或其他可执行文件。
- Node launcher 只选择原生二进制、透传参数和信号。
- `mcp` 模式下 launcher 和原生二进制都不能向 stdout 输出非 JSON-RPC 内容。
- 日志只写 stderr，并默认不包含 CDP params/result 原文。
- npm package version、Manifest version、Release version 必须完全一致。

## 7. Node 跨平台 launcher

`bin/chatos-browser-cdp` 使用 `#!/usr/bin/env node`，根据下表选择二进制：

| Node key | 二进制 |
| --- | --- |
| `darwin-arm64` | `dist/macos/arm64/chatos-browser-cdp` |
| `darwin-x64` | `dist/macos/x64/chatos-browser-cdp` |
| `linux-arm64` | `dist/linux/arm64/chatos-browser-cdp` |
| `linux-x64` | `dist/linux/x64/chatos-browser-cdp` |
| `win32-arm64` | `dist/windows/arm64/chatos-browser-cdp.exe` |
| `win32-x64` | `dist/windows/x64/chatos-browser-cdp.exe` |

launcher 必须：

- 使用 package root 内的固定相对路径。
- 拒绝未知 `platform-arch`。
- 使用 `stdio: "inherit"`。
- 转发 `SIGINT`、`SIGTERM`。
- 使用子进程退出码退出。
- 错误写 stderr。
- 不执行 shell command 拼接。
- 不搜索或执行 package root 外的同名二进制。

## 8. Chatos Manifest v3

生产 Manifest 基线：

```json
{
  "schemaVersion": 3,
  "name": "chatos-browser-cdp",
  "version": "1.0.0",
  "description": "Operate managed Chromium sessions or an explicitly connected Chrome browser through the Chrome DevTools Protocol.",
  "author": {
    "name": "Chatos",
    "url": "https://github.com/chatos-ai"
  },
  "homepage": "https://github.com/chatos-ai/chatos-browser-cdp",
  "repository": "https://github.com/chatos-ai/chatos-browser-cdp",
  "license": "Apache-2.0",
  "keywords": ["browser", "chrome", "cdp", "automation", "mcp"],
  "mcpServers": {
    "browser-cdp": {
      "type": "stdio",
      "bin": "chatos-browser-cdp",
      "args": ["mcp"],
      "env": {}
    }
  },
  "interface": {
    "displayName": "Browser CDP",
    "shortDescription": "Full browser control through Chrome DevTools Protocol",
    "longDescription": "Launch an isolated managed Chrome browser or connect to an explicitly paired existing Chrome session. Supports high-level browser automation and raw CDP commands with local permission enforcement.",
    "developerName": "Chatos",
    "category": "Developer Tools",
    "capabilities": [
      "Managed browser sessions",
      "Existing Chrome connection",
      "Page interaction",
      "Network inspection",
      "Full CDP commands"
    ],
    "websiteURL": "https://github.com/chatos-ai/chatos-browser-cdp",
    "defaultPrompt": [
      "Inspect the current page before interacting with it.",
      "Use raw CDP commands only when high-level browser tools are insufficient."
    ],
    "brandColor": "#4285F4"
  },
  "dependencies": {
    "minimumHostVersion": ">=2.1.0",
    "supportedPlatforms": ["macos", "windows", "linux"]
  },
  "permissions": [
    {
      "permission": "process.spawn",
      "required": true,
      "reason": "Launch the installed Browser CDP stdio MCP executable.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.managed.launch",
      "required": true,
      "reason": "Launch an isolated local Chrome or Chromium process.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.page.read",
      "required": true,
      "reason": "Read page structure, accessibility snapshots and visible content.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.page.control",
      "required": true,
      "reason": "Navigate, click, type, scroll and control browser tabs.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.chrome.attach",
      "required": false,
      "reason": "Connect to an explicitly paired existing Chrome browser.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.network.observe",
      "required": false,
      "reason": "Inspect console, network, WebSocket and HAR activity.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.network.intercept",
      "required": false,
      "reason": "Abort or mock explicitly selected network requests.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.file.transfer",
      "required": false,
      "reason": "Upload user-selected files and expose downloaded files as managed artifacts.",
      "components": ["browser-cdp"]
    },
    {
      "permission": "browser.cdp.raw",
      "required": false,
      "reason": "Execute explicitly approved raw Chrome DevTools Protocol commands.",
      "components": ["browser-cdp"]
    }
  ]
}
```

`minimumHostVersion` 必须等于首个完整支持本文第 13 节平台改造的 Local Connector 产品版本。若实际产品发版号不是 `2.1.0`，必须在发布前同时更新本文示例、Manifest 和发布测试，不能删除最低版本限制。

## 9. MCP 协议和进程生命周期

### 9.1 stdio framing

Chatos 当前 stdio runtime 使用一行一个 JSON-RPC 2.0 对象，不使用 `Content-Length` framing。Browser MCP 必须兼容：

- JSON-RPC request/response 各占一行。
- stdout 只承载 JSON-RPC。
- stderr 承载脱敏日志。
- 单行响应不得超过 Local Connector 的 4 MiB 限制。
- `tools/list` 不超过 200 个工具和 512 KiB。
- 工具结果必须读取并遵守 `_meta["chatos/toolResultMaxChars"]`。

### 9.2 lifecycle

必须实现：

- `initialize`
- `notifications/initialized`
- `ping`
- `tools/list`
- `tools/call`
- `notifications/cancelled`
- 兼容性 `shutdown` / `exit`
- `SIGINT` / `SIGTERM` 清理

在 Chatos stdio runtime 完成标准 initialize 改造前，MCP 还必须兼容“进程启动后直接收到 `tools/list`”。该兼容路径只用于 Host 迁移，不能改变 tools schema 或权限行为。

取消和关闭：

- 单次工具取消先取消对应 Rust future。
- 无法安全取消的 direct CDP command 应 detach 对应 CDP session。
- Local Connector 强制取消 stdio session 时，MCP 必须关闭 Chrome 进程树、WebSocket、后台事件 task 和临时 profile。
- `browser_session_close` 关闭浏览器会话，但不要求 MCP 进程立即退出。

### 9.3 timeout

当前 Chatos stdio RPC 默认 15 秒。平台改造后，工具定义可以声明受控 timeout；Browser MCP 内部仍必须有更短的子步骤 timeout。

建议上限：

| 操作 | 默认 | 最大 |
| --- | ---: | ---: |
| 普通 CDP command | 5 秒 | 15 秒 |
| click/type/press | 5 秒 | 10 秒 |
| navigation | 15 秒 | 60 秒 |
| snapshot | 10 秒 | 20 秒 |
| event poll | 0 秒 | 5 秒 |

长时间录制或导出不得保持一个无限期 `tools/call`；必须使用 start/status/stop 或 start/poll 模式。

## 10. Runtime 数据模型

```text
Runtime
└── BrowserSession
    ├── id
    ├── mode: managed | chrome_extension
    ├── owner_adapter_session_id
    ├── browser_instance_id
    ├── tabs: Map<TabId, BrowserTarget>
    ├── cdp_sessions: Map<CdpSessionId, CdpSession>
    ├── subscriptions: Map<SubscriptionId, EventQueue>
    ├── element_refs: ElementRefGeneration
    ├── routes: Map<RouteId, RouteRule>
    └── artifacts: Map<ArtifactRef, ArtifactDescriptor>
```

约束：

- 所有公开 ID 使用 opaque random ID，不使用可猜测自增 ID。
- backend target ID 和 CDP session ID 不直接作为公开 ID 返回。
- session 必须绑定 Chatos adapter session，禁止跨 task/user/device 使用。
- 页面 navigation、frame replacement 或 document generation 变化后，旧 element ref 失效。
- browser/page/worker 等 target 均由统一 Target Registry 管理。
- pending CDP callback 在 timeout、detach、browser crash 和 session close 时全部失败完成。
- 每个 subscription 维护单调递增 sequence。
- 每个事件队列最多 10,000 条或 8 MiB，超限丢弃最旧项并增加 `dropped_event_count`。

## 11. MCP 工具目录

### 11.1 会话和标签页

- `browser_session_open`
- `browser_session_status`
- `browser_session_close`
- `browser_tabs`
- `browser_tab_new`
- `browser_tab_switch`
- `browser_tab_close`

### 11.2 高层页面工具

- `browser_navigate`
- `browser_snapshot`
- `browser_find`
- `browser_click`
- `browser_type`
- `browser_fill_form`
- `browser_press`
- `browser_scroll`
- `browser_wait`
- `browser_handle_dialog`
- `browser_screenshot`

高层工具优先使用 accessibility snapshot/ref。不得默认要求模型依据截图坐标点击。

### 11.3 Console、Network、HAR 和 WebSocket

- `browser_console`
- `browser_network`
- `browser_network_request`
- `browser_har_start`
- `browser_har_stop`
- `browser_websocket_start`
- `browser_websocket_events`
- `browser_websocket_stop`

### 11.4 请求拦截

- `browser_route_add`
- `browser_route_list`
- `browser_route_remove`
- `browser_route_clear`

第一版 route action 只允许：

- `abort`
- `mock_json`

不得允许 caller 注入任意 shell、任意本机文件、任意 credential header 或执行脚本。

### 11.5 文件

- `browser_upload`
- `browser_downloads`

上传接受 Local Connector 生成的 `file_grant_id`，不接受任意绝对路径。下载、HAR 和大截图写入 `CHATOS_PLUGIN_ARTIFACT_DIR`，返回 artifact descriptor，不返回用户机器绝对路径。

### 11.6 完整 CDP

- `browser_cdp_targets`
- `browser_cdp_attach`
- `browser_cdp_detach`
- `browser_cdp_send`
- `browser_cdp_subscribe`
- `browser_cdp_events`
- `browser_cdp_unsubscribe`

`browser_cdp_send` 接受任意合法 `Domain.method`：

```json
{
  "browser_session_id": "bs_...",
  "cdp_session_id": "cs_...",
  "target": "page",
  "method": "Runtime.evaluate",
  "params": {
    "expression": "document.title"
  },
  "timeout_ms": 10000
}
```

完整 CDP 不维护静态 method allowlist，但必须强制：

- `browser.cdp.raw` permission。
- 每次本机审批。
- session/target 所有权。
- method 格式和参数大小。
- timeout 和返回大小。
- 不暴露 debugger endpoint。
- params/result 不进入审计和普通日志。

事件使用 subscribe/poll/unsubscribe，不依赖 MCP host 实时消费 stdio notification：

```text
browser_cdp_subscribe -> subscription_id
browser_cdp_events(subscription_id, after_sequence, max_events, wait_ms)
browser_cdp_unsubscribe(subscription_id)
```

## 12. Tool Policy 和本机审批

Browser Plugin 不得通过工具名称硬编码接入审批。每个 MCP tool 在 `tools/list` 中携带通用 metadata：

```json
{
  "name": "browser_cdp_send",
  "description": "Execute a raw CDP command.",
  "inputSchema": {
    "type": "object",
    "properties": {},
    "additionalProperties": false
  },
  "_meta": {
    "chatos/requiredPermissions": ["browser.cdp.raw"],
    "chatos/riskLevel": "critical",
    "chatos/approvalMode": "per_call",
    "chatos/parallelSafe": false,
    "chatos/timeoutMs": 15000
  }
}
```

Local Connector 必须：

1. 在 prepare 阶段验证 tool metadata。
2. 验证 tool 所需 permission 已存在于签名 Manifest 对应 component。
3. 把原始 tool definition 和 metadata 纳入 tool snapshot hash。
4. 在 tools/call 前检查 permission snapshot。
5. 根据 approval mode 发起本机审批。
6. 审批日志只记录 tool、risk、CDP method 和 params hash。
7. 拒绝旧 Host 无法理解的 policy version。

审批建议：

| 操作 | 审批方式 |
| --- | --- |
| snapshot、tabs、普通 console | permission 满足后无需逐次审批 |
| navigate、click、type | browser session 内无需逐次审批 |
| 连接用户现有 Chrome | 每次 attach 审批 |
| route interception | 每条新增规则审批 |
| 文件上传、下载导出 | 每次审批 |
| raw CDP | 每次调用审批 |
| Cookie、Storage、credential-like CDP | 高敏感提示并逐次审批 |

## 13. Chatos 平台改造

以下平台改造完成前，Browser Plugin 不能被认定为生产可发布。

### 13.1 删除 Browser 内置注入

删除以下文件中基于 catalog name 的特殊逻辑：

[Task Runner Plugin 应用逻辑](../../task_runner_service/backend/src/services/tool_runtime/plugin_management_policy/task_config_application.rs)

当前逻辑：

```text
plugin.catalog.name == "browser"
-> inject BuiltinMcpKind::BrowserTools
```

目标逻辑：

```text
selected signed Plugin MCP component
-> pinned Plugin MCP tools
```

生产环境不保留“插件不可用时自动切回内置 BrowserTools”的 fallback。

### 13.2 通用 Plugin Tool Policy

修改 Local Connector Plugin MCP prepare/execute：

- 保存并校验 `_meta.chatos/*`。
- permission 必须属于 signed component inventory。
- policy metadata 纳入 snapshot hash。
- execute 前执行 permission 和 approval 检查。
- 增加 tool-level timeout 上限。

主要实现入口：

- [Plugin MCP preparation](../../local_connector_client/core/src/plugins/runtime/mcp/preparation.rs)
- [Plugin MCP runtime shell](../../local_connector_client/core/src/plugins/runtime/mcp/runtime_shell.rs)
- [Plugin Runtime Host](../../local_connector_client/core/src/plugins/runtime/host.rs)

### 13.3 标准 MCP initialize

修改 `chatos_mcp_runtime` stdio session：

- spawn 后执行一次 `initialize`。
- 发送 `notifications/initialized`。
- 之后才执行 `tools/list`/`tools/call`。
- session cache 复用时不重复 initialize。
- cancel/evict 时完成进程清理。

当前实现入口：

[stdio MCP runtime](../../crates/chatos_mcp_runtime/src/rpc/stdio.rs)

### 13.4 Plugin data 和 artifact 目录

Local Connector 为每个 installation/adapter session 创建并注入：

```text
CHATOS_PLUGIN_ROOT
CHATOS_PLUGIN_DATA_DIR
CHATOS_PLUGIN_CACHE_DIR
CHATOS_PLUGIN_ARTIFACT_DIR
```

规则：

- Plugin root 只读。
- profile 和 cache 只能写 data/cache 目录。
- artifact 目录按 adapter session 隔离。
- session 关闭后清理未注册 artifact。
- artifact 注册前验证路径、size、SHA-256 和 MIME。

### 13.5 Browser Extension Bridge

Local Connector 新增设备级 Browser Bridge：

- 只监听 loopback 或使用 Native Messaging。
- 校验固定 Chrome Extension ID。
- 用户显式配对。
- 每个 adapter session 颁发短期 token。
- token 绑定 user/device/plugin ID/release ID/session ID。
- token 不进入服务端任务 snapshot、Catalog、Manifest 或日志。
- MCP 通过 `CHATOS_BROWSER_BRIDGE_ENDPOINT` 和临时 credential 获取连接。

## 14. 两种浏览器模式

### 14.1 Managed Chrome

`browser_session_open(mode="managed")`：

1. 查找本机已安装 Chrome、Chromium 或 Edge。
2. 创建 Chatos 管理的 isolated 或 persistent profile。
3. 只在 loopback 启动 debugger。
4. 使用随机端口、`DevToolsActivePort` 或 debugging pipe。
5. DirectCdpBackend 连接后不再向上层暴露 endpoint。
6. MCP 退出时终止自己启动的 Chrome 进程树。

不得启用 Chromiumoxide fetcher 或 Rust Headless Chrome fetch feature 在运行时下载浏览器。

### 14.2 用户现有 Chrome

`browser_session_open(mode="chrome_extension")`：

```text
Chrome Extension
-> Local Connector Browser Bridge
-> ExtensionCdpBackend
-> Browser CDP MCP tools
```

Extension 使用：

- `chrome.debugger.attach`
- `chrome.debugger.sendCommand`
- `chrome.debugger.onEvent`
- `chrome.tabs`

Extension backend 应参考 Puppeteer `ExtensionTransport` 对 browser/target command 的兼容处理，但必须向 MCP 返回明确 capability 信息。Extension API 无法实现的 CDP method 必须返回 `unsupported_by_backend`，不能静默伪造成功。

## 15. 发布流水线

发布产物：

```text
chatos-browser-cdp-<version>.tgz
chatos-plugin.manifest.json
sbom.cdx.json
provenance.json
artifact.sha256
npm-integrity-sha512.txt
release-signature.json
```

流水线：

1. 编译 macOS arm64/x64、Linux arm64/x64、Windows arm64/x64。
2. macOS 二进制 codesign 并验证。
3. Windows 二进制做 Authenticode 签名并验证。
4. 执行 Rust 单元、contract 和跨平台 launcher 测试。
5. 组装 npm package。
6. 执行 `npm pack`。
7. 验证 package name/version/bin/files。
8. 复验 `.tgz` 不含 symlink、路径穿越和未声明文件。
9. 计算 npm SHA-512 integrity。
10. 计算 artifact SHA-256。
11. 生成 CycloneDX SBOM 和 provenance。
12. 使用独立 Ed25519 Release key 签名。
13. Plugin Management 创建不可变 Release。
14. Marketplace Catalog 使用独立 Catalog key 签名。
15. 从真实测试 Marketplace 在干净设备完成安装和运行验收。

包限制必须进入 CI：

- `.tgz` 不超过 256 MiB。
- 单文件不超过 128 MiB。
- 解包总大小不超过 768 MiB。
- 文件数不超过 8,192。
- 路径长度、深度满足 Local Connector 限制。
- package bin 被 package file SHA-256 覆盖。

## 16. 安装和运行预期

安装目录由 Local Connector 分配：

```text
~/.chatos/local_connector/plugins/installed/
└── chatos-browser-cdp--<stable-suffix>/
    └── <version>/
        ├── package.json
        ├── bin/chatos-browser-cdp
        ├── dist/...
        ├── manifest/chatos-plugin.json
        ├── sbom.cdx.json
        └── .chatos-installation.json
```

本地 registry 必须记录：

- plugin ID、marketplace ID、Release ID。
- active version。
- npm package name/version。
- artifact SHA-256、Manifest SHA-256。
- Release signing key ID。
- package file SHA-256 map。
- `browser-cdp` component 和 `npm_stdio` runtime kind。
- permissions、supported platforms 和 active/availability 状态。

运行链固定为：

```text
Task Runner
-> MCP Management
-> Local Connector Service Relay
-> Local Connector Client PluginRuntimeHost
-> verified npm bin
-> Rust Browser CDP MCP
-> Chrome/CDP
-> 原路返回
```

## 17. 自动化验收

### 17.1 Marketplace 安装

- 签名 Catalog/Release 可以安装。
- SHA-512、SHA-256、package identity、bin 全部复验。
- 修改 artifact 任意一个字节后安装失败。
- 撤销 Release 后新的 prepare/call 失败关闭。
- `state.json` 和 `.chatos-installation.json` 与 Release 一致。

### 17.2 MCP 合约

- 六个平台 launcher 选择正确二进制。
- stdout 无非 JSON-RPC 内容。
- `initialize`、`tools/list`、`tools/call` 成功。
- 兼容路径直接 `tools/list` 成功。
- tool 顺序和 schema 重启后稳定。
- tool catalog 小于 200 项和 512 KiB。
- 取消、timeout、MCP crash 和 Host close 能正确清理。

### 17.3 Managed browser

- 启动已安装 Chrome。
- 打开本地测试站点。
- snapshot/find/click/type/fill/navigation 成功。
- 新建、切换、关闭标签页成功。
- console/network/WebSocket/HAR 成功。
- route abort/mock 成功。
- raw `Runtime.evaluate`、`Page.*`、`Network.*` 等 CDP command 成功。
- Chrome crash 后返回结构化错误并清理 session。

### 17.4 Existing Chrome

- Extension 配对必须用户确认。
- 只能访问用户明确 attach 的 Chrome/tab。
- 可以使用现有登录态。
- token 过期、Extension disabled/uninstalled 后立即失败。
- 其他进程不能冒充 Extension 或复用旧 token。

### 17.5 安全

- 未授予 permission 时工具调用被 Local Connector 拒绝。
- raw CDP 未审批时命令不发送。
- 不能访问其他 adapter session 的 browser/tab/CDP session。
- Cookie、Authorization、表单密码不进入日志和审计。
- 不暴露 debugger endpoint、Browser Bridge token。
- upload 只能使用 file grant。
- download/HAR 只能写 artifact 目录。
- oversized command/event/result 被有界拒绝或截断。

### 17.6 真实端到端链

下列流程必须在测试环境全链路通过：

```text
Marketplace 安装
-> ChatOS 选择 Browser Plugin
-> Task 固定 Release/component/tool snapshot
-> MCP Management prepare
-> Local Connector Relay
-> npm launcher
-> Rust MCP tools/list
-> browser_session_open
-> browser_navigate
-> browser_snapshot
-> browser_cdp_send
-> cancel/close
```

只有这条链完整通过，才能认定 Browser CDP MCP 可以像 Open Computer Use 一样发布、安装和运行。

## 18. 明确不采用的方案

- 不直接把 Playwright MCP 的 Node package 重新打包上架。
- 不把 Puppeteer 作为运行时依赖。
- 不同时使用 Chromiumoxide 和 Rust Headless Chrome 控制同一个 direct browser session。
- 不在插件中包含所有平台的 Chrome for Testing。
- 不在运行时下载浏览器或执行 `npx @latest`。
- 不允许调用方传入 debugger WebSocket URL。
- 不接受任意 absolute upload/download path。
- 不通过插件名称硬编码注入内置 BrowserTools。
- 不保留未安装 Plugin 时的内置 Browser fallback。
- 不把 raw CDP params/result、Cookie、Header 或页面内容写入普通日志和审计。

## 19. Definition of Done

Browser CDP Plugin 只有同时满足以下条件才算完成：

- 独立 Rust stdio MCP 和六平台 npm launcher 完成。
- DirectCdpBackend 和 ExtensionCdpBackend 通过同一 trait 工作。
- 高层工具和完整 raw CDP 工具可用。
- Tool Policy、permission 和本机审批由 Local Connector 强制执行。
- Browser Extension Bridge 完成设备绑定和短期 token。
- Task Runner 的 `browser` 名称硬编码已删除。
- 最终生产路径不再使用内置 BrowserTools fallback。
- npm `.tgz`、SBOM、hash、Release signature、Catalog signature 完整。
- Marketplace 全新安装成功。
- Task Runner 到 Chrome 的完整调用链通过自动化测试。
- Release revoke、权限拒绝、Client 离线和 artifact 漂移均失败关闭。
