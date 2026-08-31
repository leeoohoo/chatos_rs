# ChatOS Browser MCP 平台迁移与客户端改造请求

文档状态：技术沟通提案
目标读者：ChatOS Plugin Management、MCP Management、Task Runner、Local Connector Client 团队
提案方：Browser CDP MCP 项目
日期：2026-08-21

## 1. 执行摘要

Browser CDP MCP 已按照 ChatOS Plugin Marketplace 的 npm Plugin、Manifest v3 和本地 stdio MCP 路线开发，目标是用一个可独立安装、签名、升级和撤销的 Marketplace Plugin，完整替代 ChatOS 任务执行链中的内置 `BuiltinMcpKind::BrowserTools`。

本提案请求 ChatOS 完成以下架构调整：

1. 删除 Browser Plugin 对 `BuiltinMcpKind::BrowserTools` 的特殊注入和隐式 fallback。
2. Browser 任务能力统一通过已安装、已固定 Release 的 Marketplace Plugin 提供。
3. 保留并补齐通用 Plugin 权限、审批、数据目录、生命周期、审计和 MCP 标准生命周期能力。
4. 不再要求 ChatOS 原生 Chrome 扩展为 Browser CDP Plugin 提供 Chrome 权限；现有 Chrome 的 `debugger` 权限、标签页共享、Native Messaging 和 Browser Bridge 由 Browser CDP Plugin 自己的独立扩展负责。
5. 在客户端浏览器预览 UI 完成 Plugin 化迁移后，删除旧 BrowserTools system route、builtin catalog、builtin prompt、provider 和相关兼容代码。

目标生产链路为：

```text
Task Runner
  -> MCP Management
  -> Local Connector Service Relay
  -> Local Connector Client
  -> 已安装的 Browser CDP Plugin stdio MCP
  -> Managed Chrome 或独立 Browser CDP Extension
```

当 Browser CDP Plugin 未安装、被禁用、Release 被撤销、权限不足或本机不可用时，应明确失败关闭，不得回退到内置 BrowserTools。

## 2. 背景与迁移动机

当前 ChatOS 同时存在三套浏览器相关实现：

1. Task Runner/MCP Runtime 中的内置 `BuiltinMcpKind::BrowserTools`。
2. Local Connector 中基于 `BrowserToolsService` 的 managed browser 和浏览器预览 UI。
3. `ChatOS Chrome` 扩展及 `com.chatos.chrome` Native Messaging Host。

Browser CDP MCP 则提供：

- Managed Chrome isolated/persistent profile。
- 用户现有 Chrome 的独立 Extension backend。
- 高层页面操作。
- Network、Console、HAR、WebSocket 能力。
- 有界 raw CDP command 和 CDP event subscription。
- 文件 grant、artifact、结果截断和敏感字段脱敏。
- Manifest 权限与 MCP tool `_meta.chatos/*` 风险元数据。

继续保留两套任务级 Browser MCP 会带来以下问题：

- 相同能力存在两套工具目录和路由选择逻辑。
- Plugin 被选中后仍隐式注入 BrowserTools，无法验证真实 Plugin 可用性。
- Plugin 故障可能被 builtin fallback 掩盖。
- 权限、审批和审计可能落到不同实现。
- Browser 功能升级必须同时修改 ChatOS 核心和 Plugin。
- raw CDP 的高风险权限会污染普通 ChatOS Chrome 扩展的安全定位。

因此建议把 Browser 能力从 ChatOS 内核迁移为独立 Marketplace Plugin。

## 3. 目标架构

### 3.1 Managed Chrome

```text
Browser CDP Plugin
  -> 启动本机已安装的 Chrome / Chromium / Edge
  -> 创建隔离或持久化 Plugin profile
  -> 仅使用 loopback CDP endpoint/debugging pipe
  -> MCP 退出时清理自己启动的进程树
```

该模式不依赖 ChatOS Chrome 扩展，也不访问用户现有 Chrome 的登录状态。

### 3.2 用户现有 Chrome

```text
Browser CDP Plugin
  -> Plugin 自己的 loopback Browser Bridge
  -> Plugin 自己的 Native Messaging Host
  -> 独立 Browser CDP Chrome Extension
  -> 用户明确共享的标签页
```

Chrome 扩展负责：

- 请求 `chrome.debugger`、`nativeMessaging`、`storage` 和 `tabs` 权限。
- 只共享用户在扩展弹窗中明确选择的 HTTP(S) 标签页。
- attach/detach `chrome.debugger`。
- 执行 session-scoped CDP command。
- 转发明确订阅的 CDP event。
- 在撤销、扩展禁用、Bridge 断开或 token 过期时立即失败关闭。

ChatOS 不需要替该扩展申请 Chrome 权限，但仍应负责 Plugin 的安装信任、进程生命周期、通用权限审批和审计。

## 4. 双方职责边界

| 能力 | ChatOS 平台负责 | Browser CDP Plugin 负责 |
| --- | --- | --- |
| Release 信任 | Publisher、Release、Catalog 签名和撤销 | 提交签名发布材料 |
| 安装 | 下载、验签、安全解包、版本固定和回滚 | 提供完整 npm `.tgz` |
| MCP 进程 | 启动、取消、超时、进程树清理 | 标准 stdio MCP 实现 |
| Plugin 权限 | Manifest permission grant、tool policy、审批和审计 | 声明最小权限和 tool `_meta` |
| Managed Chrome | 提供受约束的本机执行环境 | Chrome 发现、profile、CDP 和生命周期 |
| Existing Chrome | 提供允许注册外部集成的通用受审计能力 | 独立扩展、Native Host、Bridge 和配对 |
| 文件访问 | file grant 和 artifact store | 只消费 grant，只返回 artifact descriptor |
| 敏感数据 | 本机审批和审计边界 | CDP policy、脱敏、大小限制和安全错误 |
| 浏览器 UI | 提供 Plugin session/artifact 展示入口 | 提供结构化页面状态、截图和流式 frame |

## 5. ChatOS 必须完成的 P0 改造

### 5.1 删除 Browser Plugin 的 builtin 特殊注入

删除或重构：

```text
task_runner_service/backend/src/services/tool_runtime/
plugin_management_policy/task_config_application.rs
```

当前类似逻辑：

```rust
if plugin.catalog.name == "browser" {
    dependencies.push(BuiltinMcpKind::BrowserTools);
}
```

目标行为：

- 选中 Browser Plugin 时只准备该 Plugin 的 MCP component。
- 不附加 `BuiltinMcpKind::BrowserTools`。
- Plugin 不可用时返回 `task_plugin_unavailable` 或等价明确错误。
- 不允许根据 Plugin 名称、分类或工具名隐式回退到 builtin。

### 5.2 Plugin MCP 使用标准 MCP 生命周期

修改：

```text
crates/chatos_mcp_runtime/src/rpc/stdio.rs
```

每个新 stdio session 必须：

1. 启动进程。
2. 发送 `initialize`。
3. 校验协议版本和 server capabilities。
4. 发送 `notifications/initialized`。
5. 之后才允许 `tools/list` 和 `tools/call`。
6. session cache 复用时不得重复 initialize。
7. 取消、超时、Release 撤销和 Plugin 禁用时关闭整个进程树。

不得长期依赖某个 MCP 对“未 initialize 直接 tools/list”的兼容行为。

### 5.3 注入正式 Plugin 数据目录

Local Connector 为每个 installation/session 创建并注入：

```text
CHATOS_PLUGIN_ROOT
CHATOS_PLUGIN_DATA_DIR
CHATOS_PLUGIN_CACHE_DIR
CHATOS_PLUGIN_ARTIFACT_DIR
```

要求：

- Plugin root 视为只读发布内容。
- Browser profile、Bridge state 和非敏感持久状态只能写入 data 目录。
- 可重建内容只能写入 cache 目录。
- HAR、download、截图等结果只能写入 session-scoped artifact 目录。
- artifact 注册前校验路径、普通文件、非 symlink、大小、SHA-256 和 MIME。
- 禁止把用户机器绝对路径返回给服务端或模型。

### 5.4 执行 MCP tool policy metadata

Local Connector 在 `tools/list` prepare 阶段验证并固定：

```json
{
  "_meta": {
    "chatos/policyVersion": 1,
    "chatos/requiredPermissions": ["browser.cdp.raw"],
    "chatos/riskLevel": "critical",
    "chatos/approvalMode": "per_call",
    "chatos/timeoutMs": 15000,
    "chatos/toolResultMaxChars": 1000000
  }
}
```

执行前必须：

1. 校验 tool snapshot/hash 未变化。
2. 校验 `requiredPermissions` 是 Manifest 已声明且用户已授权的子集。
3. 根据 `riskLevel` 和 `approvalMode` 触发本机审批。
4. 应用 tool-specific timeout。
5. 应用结果大小限制。
6. 在审批、权限、超时或 snapshot 不一致时失败关闭。

至少以下能力不得默认无审批执行：

- `browser_cdp_send`
- raw CDP event subscription
- Network response/request body 读取
- Storage、Cookie 或 credential-like 数据访问
- Network interception/mock
- 文件上传和下载导出
- 向页面输入敏感文本

### 5.5 把权限状态改为真实 grant 状态

当前 Local Connector 不应在安装成功后无条件上报：

```text
permission_status = Satisfied
```

建议引入 installation-scoped permission grant：

```text
owner_user_id
device_id
plugin_id
release_id
permission
granted/denied
granted_at
revoked_at
```

规则：

- 必需权限被拒绝时 Plugin 状态为 `NeedsPermission`。
- 可选权限被拒绝时 Plugin 仍可启动，但对应工具不可发布或调用失败。
- Release 新增权限时不能沿用旧 Release 的宽泛授权。
- 用户可在 Plugin 详情页逐项查看和撤销权限。
- MCP Management 的 permission snapshot 只能包含实际 grant，不能直接复制 Manifest 全量声明。

### 5.6 增加 Plugin 外部集成注册权限

建议增加通用权限标识：

```text
browser.native_host.register
```

或更通用的：

```text
external_integration.register
```

Browser CDP Plugin 第一次设置 Existing Chrome 时，请求本地确认：

```text
该 Plugin 将在当前用户范围注册 Chrome Native Messaging Host，
只允许固定 ID 的 Browser CDP 扩展连接。是否继续？
```

平台不必代替 Plugin 编写 Native Host manifest，但应：

- 审计用户确认。
- 限制为用户级注册。
- 禁止覆盖不属于该 Plugin 的同名注册。
- 记录 Plugin/Release/设备和注册目标。
- 在 Plugin 禁用或卸载时触发清理流程。

### 5.7 增加卸载前生命周期

当前 `PluginDisabled` 不足以覆盖直接卸载。建议增加：

```text
BeforePluginUninstall
```

或者确保卸载流程先执行等价的不可跳过 cleanup callback。

Browser CDP Plugin 使用该生命周期：

- 关闭 managed Chrome 和 Bridge 进程。
- 撤销 Browser Bridge token。
- 删除只属于自己的 Native Host 注册。
- 删除临时 rendezvous 和 credential 文件。
- 保留是否删除用户 profile/data 的明确用户选择。

如果 cleanup 失败，平台应显示明确错误，并允许用户选择保留诊断后继续卸载；不得静默遗留仍可执行的后台入口。

## 6. ChatOS 建议完成的 P1 改造

### 6.1 客户端浏览器预览 UI Plugin 化

旧 Electron 客户端曾使用：

```text
local_connector_client/core/src/api/handlers/browser_sessions.rs（3.0.0 已移除）
```

直接构造 `BrowserToolsService`，用于浏览器预览、截图和交互。

建议迁移为：

```text
Local Connector UI
  -> Plugin adapter session
  -> Browser CDP Plugin tools/artifacts
  -> screenshot/frame descriptor
```

迁移完成前，可以暂时保留 `BrowserToolsService` 作为 UI-only legacy implementation，但它不得再出现在 Agent/Task 的 MCP 工具路由中。

### 6.2 停用旧 ChatOS Chrome provider

现有 `ChatOS Chrome` 扩展使用：

```text
activeTab + scripting + optional_host_permissions
```

Browser CDP 扩展使用：

```text
debugger + nativeMessaging + tabs
```

二者安全级别不同，不建议把 raw CDP 合并进普通 ChatOS Chrome 扩展。

建议：

1. Browser CDP Plugin 不依赖 `com.chatos.chrome`。
2. 使用独立 Native Host 名称 `ai.chatos.browser_bridge`。
3. 使用独立 Chrome Web Store Extension ID。
4. 旧 ChatOS Chrome 集成标记 deprecated。
5. 确认没有其他产品功能依赖后再删除旧扩展、Native Host 和 command bridge。

## 7. `BuiltinMcpKind::BrowserTools` 删除范围

### 7.1 第一阶段立即删除的行为

- Browser Plugin 触发 `BuiltinMcpKind::BrowserTools` 注入。
- Browser Plugin 不可用时的 builtin fallback。
- Task 配置中基于 Plugin 名称猜测 BrowserTools 依赖。

### 7.2 完成 UI 迁移后删除的类型和路由

- `BuiltinMcpKind::BrowserTools`
- `SystemMcpKey::BrowserTools`
- BrowserTools builtin catalog entry
- BrowserTools builtin prompt section
- BrowserTools system MCP descriptor
- ChatOS provider 中 BrowserTools prepare/call 特判
- MCP Management BrowserTools route
- Task Runner BrowserTools 默认选择和 guide
- Local Connector builtin BrowserTools runtime registry
- 对应配置兼容和测试 fixture

涉及目录包括但不限于：

```text
crates/chatos_mcp_runtime
crates/chatos_mcp_service
crates/chatos_plugin_management_sdk
mcp_management_service
task_runner_service
clients/macos
clients/windows
plugins/browser
```

### 7.3 不应删除的通用能力

- Plugin Marketplace 和签名验证。
- `process.spawn` 权限检查。
- Plugin permission grant。
- Local Connector 本机审批。
- Plugin data/cache/artifact 目录。
- file grant。
- Plugin disable/uninstall 生命周期。
- MCP timeout、cancel 和进程树清理。
- 审计、诊断、脱敏和 Release 撤销。

目标是删除 Browser 专用 builtin，不是取消 Plugin 安全边界。

## 8. Browser CDP Plugin 权限建议

建议 Manifest 权限：

| 权限 | 必需 | 用途 |
| --- | --- | --- |
| `process.spawn` | 是 | 启动已安装 stdio MCP |
| `browser.managed.launch` | Managed 模式必需 | 启动隔离 Chrome |
| `browser.page.read` | 是 | 页面结构、快照和可见内容 |
| `browser.page.control` | 是 | 导航、点击、输入和滚动 |
| `browser.chrome.attach` | Existing Chrome 可选 | 连接用户明确共享的 Chrome 标签页 |
| `browser.native_host.register` | Existing Chrome 可选 | 注册用户级 Native Messaging Host |
| `browser.network.observe` | 可选 | Network、Console、HAR 和 WebSocket |
| `browser.network.intercept` | 可选 | 请求 abort/mock |
| `browser.file.transfer` | 可选 | file grant 上传和 artifact 下载 |
| `browser.cdp.raw` | 可选、高风险 | raw CDP command/event |

建议默认安装只授予 Managed Browser 的最小权限；Existing Chrome、Network interception 和 raw CDP 由用户后续显式启用。

## 9. 迁移计划

### 阶段 A：Plugin-only Agent 路由

- 删除 Browser builtin 自动注入。
- Browser Agent 工具只来自 Marketplace Plugin。
- Plugin 缺失时明确不可用。
- 旧 BrowserToolsService 暂时只服务客户端旧 UI。

完成标准：任务快照和工具目录中不再出现 builtin BrowserTools。

### 阶段 B：Managed Browser 正式发布

- 发布六平台签名二进制。
- 完成标准 MCP initialize。
- 完成 data/cache/artifact 目录。
- 完成权限 grant 和 tool policy。
- 默认仅开放 Managed Browser。

完成标准：干净设备通过 Marketplace 安装后可执行页面读取、交互、截图和关闭，并且无 builtin fallback。

### 阶段 C：Existing Chrome 独立扩展

- Browser CDP Extension 上架 Chrome Web Store。
- 固定 Extension ID。
- 完成 macOS、Linux、Windows 用户级 Native Host 注册。
- 完成显式 setup/unsetup、配对和撤销。
- 完成卸载前 cleanup。

完成标准：只能控制用户明确共享的标签页，扩展或 Plugin 禁用后立即断开。

### 阶段 D：客户端 UI 迁移和旧实现删除

- 浏览器预览 UI 接入 Plugin session/artifact。
- 删除 BrowserTools system route 和 builtin 类型。
- 停用旧 ChatOS Chrome provider。
- 删除无消费者的旧实现与测试。

完成标准：全仓不再需要 `BuiltinMcpKind::BrowserTools` 或 `SystemMcpKey::BrowserTools`。

## 10. 验收标准

### 10.1 路由与可用性

- Browser Plugin 未安装时不出现 Browser 工具。
- Browser Plugin 安装并授权后只出现 Plugin tools。
- Release 撤销后新的 prepare/call 立即失败。
- 不存在 builtin fallback。
- 任务固定使用创建时的 Plugin Release/component snapshot。

### 10.2 权限与审批

- Manifest 声明不等于用户 grant。
- 必需权限拒绝后 Plugin 状态为 `NeedsPermission`。
- 可选权限拒绝后相关 tool 不可调用。
- raw CDP 和敏感 Network/Storage 操作触发本机逐次审批。
- 审批记录不包含完整 CDP params/result 或凭据。

### 10.3 Existing Chrome

- Extension ID 和 Native Host `allowed_origins` 固定匹配。
- Native Host 只注册到当前用户范围。
- 用户未共享标签页时 target catalog 为空。
- 撤销标签页立即 detach debugger 并清理 subscription。
- 非 HTTP(S) 特权页面不可共享。
- Extension、Native Host、Bridge 任意一端断开时 pending call 失败关闭。

### 10.4 生命周期和清理

- Task cancel 终止 MCP 和其启动的 managed Chrome 进程树。
- Plugin disable 取消所有 adapter session。
- Plugin uninstall 清理 Native Host 注册和临时 credential。
- Plugin 更新后 Native Host path 能安全迁移。
- 不删除不属于该 Plugin 的文件或注册项。

### 10.5 供应链

- npm package、Manifest 和 Release version 完全一致。
- 六平台二进制包含在 `.tgz` 内，安装时不下载可执行代码。
- 通过 npm SHA-512 integrity 和 artifact SHA-256 复验。
- macOS 和 Windows 二进制完成平台签名。
- SBOM、provenance、Release signature 和 Catalog signature 可验证。

## 11. Browser CDP 团队交付项

Browser CDP 团队负责提供：

- 六平台 release 二进制和 Node launcher。
- 完整 Manifest v3。
- 正式 CycloneDX SBOM 和 provenance。
- Managed Chrome backend。
- 独立 Chrome Extension、Native Host 和 Bridge。
- tool `_meta.chatos/*` 元数据。
- CDP method policy、参数大小限制、结果截断和敏感字段脱敏。
- file grant 和 artifact contract。
- setup/status/unpair/cleanup 诊断工具。
- Chrome Web Store 发布材料和隐私说明。
- Marketplace Release 签名材料和端到端验收报告。

## 12. ChatOS 团队交付项

ChatOS 团队负责提供：

- 删除 Browser builtin 特殊注入和 fallback。
- 标准 MCP initialize 生命周期。
- 实际 permission grant 和 `NeedsPermission` 状态。
- tool `_meta` policy 执行与本机审批。
- Plugin data/cache/artifact 目录。
- Plugin uninstall 前 cleanup 生命周期。
- 用户级外部集成注册的权限与审计入口。
- Plugin session/artifact 到客户端浏览器预览 UI 的通用接口。
- Publisher/Release key 审核和 Marketplace 发布协作。

## 13. 需要双方确认的决策

1. `browser.native_host.register` 使用 Browser 专用权限，还是平台通用的 `external_integration.register`。
2. Managed Browser 是否作为首个 stable Release 的唯一默认模式。
3. Existing Chrome 和 raw CDP 是否作为独立可选组件/权限组发布。
4. Plugin 卸载 cleanup 失败时采用阻止卸载还是“明确确认后继续并显示手工清理步骤”。
5. 旧 ChatOS Chrome 扩展是否还有 Browser MCP 之外的生产消费者。
6. 客户端浏览器预览 UI 迁移到 Plugin 的时间表。
7. Browser CDP Publisher 名称、品牌授权、Release key 和 Marketplace 审核负责人。

## 14. 请求的最终确认

希望 ChatOS 团队确认以下目标架构：

> Browser CDP 能力由签名 Marketplace Plugin 独立提供，任务执行链不再注入或回退到 `BuiltinMcpKind::BrowserTools`。Managed Chrome 由 Plugin 自己管理；用户现有 Chrome 的 `debugger` 权限、扩展、Native Host 和 Browser Bridge 由独立 Browser CDP Extension 管理。ChatOS 保留并完善通用 Plugin 权限、审批、生命周期、数据目录、审计和供应链安全能力。在客户端浏览器 UI 完成 Plugin 化迁移后，彻底删除旧 BrowserTools builtin 路由和实现。

如该方向确认，建议双方先以“阶段 A：Plugin-only Agent 路由”和“阶段 B：Managed Browser stable Release”为第一批联合改造范围。
