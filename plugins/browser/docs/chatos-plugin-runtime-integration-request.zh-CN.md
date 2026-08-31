# Browser CDP MCP 与 ChatOS 通用 Plugin Runtime 对接确认单

文档日期：2026-08-21
适用双方：Browser CDP MCP 开发团队、ChatOS / Local Connector / MCP Management 团队

## 1. 目的

Browser CDP MCP 已按照新的职责边界自行负责 Managed Chrome、Existing Chrome、Chrome
Extension、Native Messaging Host、loopback Bridge、配对 token 和浏览器权限交互。

本文只请求 ChatOS 完成所有 Marketplace Plugin MCP 共用的通用能力，不请求恢复任何
BrowserTools、浏览器专用 Bridge、浏览器 session API、浏览器权限 UI 或 fallback。

## 2. Browser MCP 当前已完成的调整

- `browser_session_open` 的 `approvalMode` 已从平台不支持的 `per_session` 改为 `per_call`。
- Existing Chrome attach 权限已失败关闭；当前静态策略暂时声明
  `browser.managed.launch` 与 `browser.chrome.attach` 的并集。
- MCP 参数不再接受 `executable_path`，调用方不能借浏览器工具指定任意本机可执行文件。
- 每个工具均输出：
  - `chatos/policyVersion`
  - `chatos/requiredPermissions`
  - `chatos/riskLevel`
  - `chatos/approvalMode`
  - `chatos/parallelSafe`
  - `chatos/timeoutMs`
  - `chatos/toolResultMaxChars`
- Browser MCP 主二进制自行启动和清理 loopback Bridge。
- Browser MCP 主二进制自身可以作为 `ai.chatos.browser_bridge` Native Messaging Host。
- Native Host manifest 由 Browser MCP 自行安装；macOS、Linux 和 Windows 路径均由 Browser
  MCP 处理。
- Bridge credential 每个 MCP 进程重新生成、单次使用、到期失效，MCP 退出时清理。
- 生产 Chrome Extension ID 可在构建时固定进 Browser MCP；开发环境支持显式 ID。
- 上传只接受 `file_grant_id`，不接受本机文件路径。
- 截图、HAR 和下载只写入 `CHATOS_PLUGIN_ARTIFACT_DIR`，返回相对路径、大小和 hash，不返回
  绝对路径。
- Artifact 候选同时通过 MCP CallToolResult `_meta["chatos/artifacts"]` 返回，等待 Host 完成
  通用注册。

## 3. 请求 ChatOS 完成的 P0 通用能力

### 3.1 MCP `per_call` 必须在发送 `tools/call` 前真正审批

当前要求：

1. Local Connector 在调用前读取不可变 tool snapshot 中的 `riskLevel` 和 `approvalMode`。
2. `approvalMode=per_call` 时调用通用本机审批 UI。
3. 用户拒绝或审批超时后，不得向 Plugin MCP 发送 `tools/call`。
4. 审批必须绑定：
   - owner user
   - device
   - task/run/adapter session
   - plugin/release/component
   - tool name
   - invocation ID
   - arguments hash
5. 对 `browser_cdp_send`，审批摘要可额外展示 CDP method 和 target，但审计中只保存 params
   hash，不保存 raw params/result。
6. raw CDP、Existing Chrome attach、上传、下载导出、HAR 导出和新增 route 均应逐次审批。

验收：用户拒绝 `browser_cdp_send` 后，Chrome 端不得收到对应 CDP command。

### 3.2 permission snapshot 必须来自用户实际 grant

当前要求：

1. Manifest permission 是“能力声明”，不能自动等同于“用户已授权”。
2. Task 固定的 permission snapshot 必须来自目标设备上的实际用户 grant。
3. optional permission 未授权时必须从 snapshot 排除。
4. prepare 和每次 tools/call 前都重新验证 permission snapshot 未漂移。
5. permission 被撤销后，已经准备的 Plugin Runtime Session 必须失败关闭。
6. Local Connector 安装状态不得无条件报告 `permission_status=Satisfied`。

#### 需要双方确认：参数条件权限

`browser_session_open` 同时支持：

```text
mode=managed          -> browser.managed.launch
mode=chrome_extension -> browser.chrome.attach
```

现有 Tool Policy 只有静态 `requiredPermissions`，无法根据参数选择权限。请 ChatOS 在以下两种
方案中确认一种：

1. 推荐：增加通用、可快照和可签名的参数条件 permission rule；或
2. 允许 Browser MCP 拆成 `browser_session_open_managed` 和
   `browser_session_open_existing` 两个工具。

在协议确认前，Browser MCP 使用两个权限的并集失败关闭，安全但会使 Managed 模式也要求
Existing Chrome attach grant。

### 3.3 通用 Plugin File Grant

Browser MCP 已支持以下目录传输契约：

```text
CHATOS_PLUGIN_FILE_GRANT_DIR/<file_grant_id>.json
```

descriptor：

```json
{
  "path": "/absolute/path/known-only-to-local-host",
  "expires_at_unix_ms": 1787241600000,
  "size": 1234,
  "sha256": "lowercase-sha256"
}
```

请求 ChatOS：

1. 用户通过本机 UI 显式选择文件。
2. Local Connector 创建 adapter-session-private grant 目录并注入
   `CHATOS_PLUGIN_FILE_GRANT_DIR`。
3. descriptor 只能由受信任 Host 写入，目录和文件使用私有权限。
4. grant 必须绑定当前 user/device/plugin/release/component/adapter session。
5. grant 必须短期有效、不可跨 session 使用，并在 session 结束后清理。
6. 文件变化、大小变化或 SHA-256 不一致时失败关闭。
7. Task Runner、MCP Management 和远端服务不得看到本机绝对路径。

未来可以用通用本机 RPC 替换目录传输，但 `browser_upload(file_grant_ids)` MCP schema 不需要
变化。

### 3.4 通用 MCP Artifact 注册

Browser MCP 写入：

```text
CHATOS_PLUGIN_ARTIFACT_DIR/<relative_path>
```

并在 CallToolResult 中返回：

```json
{
  "_meta": {
    "chatos/artifacts": [
      {
        "producer_artifact_id": "artifact_<opaque>",
        "relative_path": "artifact_<opaque>-screenshot.png",
        "display_name": "screenshot.png",
        "media_type": "image/png",
        "size_bytes": 12345,
        "sha256": "lowercase-sha256"
      }
    ]
  }
}
```

请求 Local Connector：

1. 只允许在当前 adapter session 的 artifact 目录内解析 `relative_path`。
2. 拒绝绝对路径、`..`、symlink、非普通文件和越界文件。
3. 重新计算 size、SHA-256 和 MIME。
4. 从当前 Runtime Session 添加 owner identity，不能信任 Plugin 自报 owner。
5. 注册为 ChatOS 正式 Plugin Artifact，并生成权威 `pa_...` ID。
6. 将正式 descriptor 返回或投影给 Task Runner/UI，使文件可以预览和下载。
7. session 关闭时清理未注册或无人持有的文件。

`producer_artifact_id` 只是 MCP 进程本地关联 ID，不得直接当作平台 Artifact ID。

## 4. 请求 ChatOS 完成的 P1 项目

### 4.1 `chatos/parallelSafe`

请通用 Runtime 解析并执行 `chatos/parallelSafe`，不要继续依赖硬编码工具名称 allowlist。
同一 browser session 的 mutating 工具仍应串行；不同只读 session 可以按 metadata 并行。

### 4.2 Windows ARM64 平台状态

Local Connector 的 platform status 必须识别 `windows-arm64`，不能报告 `unsupported`。Browser
MCP Release 会提供 Windows ARM64 原生二进制。

### 4.3 Artifact 和 File Grant 自动化测试

建议加入通用 Plugin fixture，不以 `browser_*` 名称实现特殊逻辑，覆盖：

- grant 未授权、过期、跨 session、hash 漂移时拒绝；
- artifact path traversal、symlink、hash 不一致时拒绝；
- 注册成功后生成 `pa_...` ID；
- Release/permission/adapter session 漂移后失败关闭。

## 5. 双方端到端验收

完成标准：

```text
Marketplace 安装 Browser Plugin
-> Task 固定 Release/component/tool/permission snapshot
-> Local Connector 标准 initialize/tools/list
-> browser_session_open(mode=managed)
-> screenshot 注册并可下载
-> 用户选择文件并生成 file_grant_id
-> browser_upload 成功
-> browser_session_open(mode=chrome_extension) 触发 attach 审批
-> browser_cdp_send 触发逐次审批
-> 拒绝审批时 Chrome 不收到 command
-> cancel/close 清理 Chrome、Bridge、token、grant 和未注册 artifact
```

## 6. 明确不请求 ChatOS 实现的内容

- 不恢复 `BuiltinMcpKind::BrowserTools`。
- 不恢复 BrowserTools fallback。
- 不提供 Browser Bridge 或 Chrome Native Host。
- 不内置 Chrome Extension。
- 不增加浏览器 session API 或浏览器预览 UI。
- 不代替 Chrome 获取 Extension 权限。
- 不按 `browser_*` 工具名称硬编码权限或审批。

ChatOS 只需要完成上述通用 Plugin Runtime 契约，Browser MCP 负责全部浏览器业务实现。
