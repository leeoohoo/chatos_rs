# 第三方 Plugin 发布与接入手册

本手册面向需要把 Computer Use、文档处理、PDF、CircleCI、Sentry 等能力接入 ChatOS 的 MCP 发布者和平台运维人员。ChatOS Plugin 只有一条交付路径：发布 npm package，由 Plugin Management Marketplace 审核并生成可信 Release，最终由 Local Connector Client 下载、安装和执行。

Browser CDP MCP 的专项架构、权限、Chrome Extension Bridge、完整 CDP 和验收规范见 [Browser CDP MCP 开发、发布与安装规范](./browser-cdp-mcp-development-and-publishing-spec.zh-CN.md)。

不存在 Cloud、Portable、Hybrid、ZIP Plugin、bundled Plugin 或未安装时的 fallback。没有在线且已安装对应 Release 的 Local Connector Client，任务就不能使用该插件。

## 1. 端到端发布路径

1. MCP 发布者开发 Node.js package，并在 `package.json.bin` 中声明 stdio MCP 可执行入口；远程 HTTP MCP 也必须通过一个 Marketplace npm package 描述和交付。
2. package 使用 `npm pack` 生成标准 `.tgz`。发布者不需要手工计算 integrity、SHA-256 或填写 artifact URL。
3. Plugin Manifest 使用 `schemaVersion: 3`，声明 MCP、权限及可选的 Command、Agent、Hook、UI 等组件，推荐放在 package 根目录的 `chatos.plugin.json` 中。
4. 平台在 Package 校验后从 Manifest `author` 自动建议 publisher identity。已有 approved publisher 会直接复用；不存在时，`super_admin` 可在确认发布时自动创建并审核。独立 Publisher 申请入口仍可用于提前占用和审核 identity。
5. 管理员在“Plugin Catalog → 上架 Plugin”上传 `.tgz`。平台安全解析 package、校验 Manifest 和 `package.json.bin`，自动计算 npm SHA-512 integrity 与 artifact SHA-256，并显示组件和权限预览。
6. 管理员确认 Marketplace、publisher、许可证、可再分发状态和 Release channel 后发布。平台保存不可变 Artifact，自动创建或复用 Publisher/Catalog，为 Marketplace + publisher 自动生成托管 Release signing key，并签署不可变 Release。发布者不上传私钥或 signing key JSON。
7. Local Connector Client 只从可信 Marketplace Catalog 获取安装来源，下载 `.tgz`，验证 Catalog/Release 签名、npm SHA-512 integrity、artifact SHA-256、package identity、`package.json.bin` 和安全解包规则。
8. 安装成功后，客户端上报可用性。ChatOS 创建任务时才能把该 Release 的不可变组件 snapshot 固定到任务中。

不得提供直接 URL 安装、手工 MCP 配置、任意 `npx package@latest`、ZIP 上传、客户端预置包或服务端执行入口。

## 2. npm package 要求

stdio MCP package 至少应满足：

- package name 和 version 与 Release 中的 `npm_package` 完全一致。
- `package.json.bin` 包含 Manifest 中 `mcpServers.<key>.bin` 指向的可执行名称。
- bin 文件位于 package 内，不能通过软链接、路径穿越或安装后下载替换执行内容。
- 运行所需 JavaScript 和生产依赖完整包含在 `.tgz` 中，安装过程不能依赖任意 install script 获取未审核代码。
- package 可在声明支持的 macOS、Windows、Linux 架构上启动并完成 MCP initialize、`tools/list` 和 `tools/call`。
- package 根目录应包含 `chatos.plugin.json`；如暂时无法随包交付，管理员也可以在上架时单独上传 Manifest JSON。

建议在发布前运行：

```bash
npm ci
npm test
npm pack
npm view ./your-package.tgz name version bin --json
```

生成后，在 Plugin Management 管理后台执行：

```text
Plugin Catalog
  → 上架 Plugin
  → 选择可信 admin_registry Marketplace
  → 上传 npm .tgz
  → 可选上传 Manifest JSON
  → 校验并预览
  → 确认 Manifest author 自动建议的 Publisher（已有则复用，不存在则自动创建）
  → 填写许可证与 Release channel
  → 发布
```

后台上传接口是 `POST /api/admin/plugin-package/analyze`，发布接口是 `POST /api/admin/plugin-package/publish`。日常上架应使用管理页面，不再手填 Catalog JSON、Release JSON、integrity、artifact URL 或 Signature JSON。

本地开发栈会把 Artifact public base 固定为本机 Plugin Management 地址。Artifact 代理只为 `localhost`、`127.0.0.1` 和 `::1` 接受 HTTP，并在 DNS 解析后再次要求所有地址都是 loopback；任何非 loopback Artifact 仍必须使用 HTTPS 并解析到公网地址。

`.tgz` 是 npm package artifact，不是旧式 Plugin ZIP。普通桌面客户端安装包使用的 DMG、DEB 或 Windows ZIP 与 Plugin artifact 无关。

## 3. Manifest schema v3

stdio MCP 只声明 package 内的 bin，不接受任意 shell command：

```json
{
  "schemaVersion": 3,
  "name": "open-computer-use",
  "version": "1.0.0",
  "description": "Control the local computer through MCP.",
  "author": { "name": "Example Publisher" },
  "mcpServers": {
    "computer-use": {
      "type": "stdio",
      "bin": "open-computer-use",
      "args": ["mcp"]
    }
  },
  "interface": {
    "displayName": "Open Computer Use",
    "shortDescription": "Local computer control",
    "longDescription": "Runs the reviewed computer-use MCP on the Local Connector Client.",
    "developerName": "Example Publisher",
    "category": "Developer Tools"
  },
  "permissions": [
    {
      "permission": "process.spawn",
      "required": true,
      "reason": "Launch the installed stdio MCP executable.",
      "components": ["computer-use"]
    },
    {
      "permission": "computer.control",
      "required": true,
      "reason": "Control applications selected by the user.",
      "components": ["computer-use"]
    }
  ]
}
```

HTTP MCP 使用 `type: "http"` 和 `url`。它仍由 Local Connector Client 发起网络请求，不会在 Plugin Management、MCP Management、Task Runner 或 Local Connector Service 中执行。生产 URL 必须使用 HTTPS；HTTP 只允许 loopback 开发地址。当前 HTTP runtime 只开放 MCP `tools/list` 和 `tools/call`，并拒绝不安全或由运行时管理的 header。

仓库中的可解析示例：

- `docs/plugins/examples/circleci-plugin.manifest.json`
- `docs/plugins/examples/sentry-plugin.manifest.json`
- `docs/plugins/examples/build-web-plugin.manifest.json`

## 4. 凭据和权限

- token、cookie、API key、refresh token 和 webhook secret 不得写入 npm package、Manifest、Catalog、Release、审计或日志。
- stdio MCP 环境变量只能引用受管理的 credential；不能在 Manifest 中固定秘密值。
- 需要向用户展示本地隔离桌面或其他实时画面的 MCP，可使用 Host 注入的 `CHATOS_PLUGIN_VISUAL_SESSION_DIR`。Host 在 turn-scoped 私有目录写入 `host.json`，Plugin 以原子替换方式更新 `session.json` 和 `frame.jpg` / `frame.png`；元数据最大 16 KiB、单帧最大 2 MiB，running 会话至少每 15 秒更新一次 `captured_at`，结束时写入 `status: "ended"`。视觉帧不得通过 Relay 或远端 HTTP 上传。
- HTTP MCP 的 OAuth/credential 由客户端在执行时注入，服务端只下发经过策略过滤的临时调用参数。
- 权限必须声明到组件粒度。读取、写入、执行、部署、屏幕控制、文件访问和网络访问应分别声明。
- 高风险能力必须在客户端完成操作系统授权、用户确认和本地策略检查。

### 4.1 桌面系统权限 onboarding 约定

声明 `computer.accessibility` 或 `computer.screen-recording` 的 stdio MCP，必须在同一个、已由 `package.json.bin` 发布的可执行入口上实现：

```text
<manifest mcpServers.*.bin> doctor
```

`doctor` 由用户在 Local Connector 权限页点击“启动插件权限引导”后显式启动。Local Connector 只执行已安装 Release 中经过签名、文件 hash 校验且属于相关 MCP 组件的 bin，不执行 Manifest 外路径或 shell 字符串。Local Connector 不替 Plugin 请求 TCC 权限，也不使用自己的 Electron 权限覆盖 Plugin 状态。该命令应：

- 检查实际 Plugin App / 原生进程的辅助功能和屏幕录制状态；
- 缺失时启动该 Plugin 自己的 macOS onboarding，使 TCC 绑定到实际执行主体；
- 输出中不得包含屏幕内容、窗口内容、凭据或本机敏感路径；
- 完成引导启动后及时退出，退出码 `0` 表示检查/引导命令已正常执行；
- 不得把 Local Connector Electron 进程的权限当成 Plugin 权限。

系统权限按实际 executable、App bundle 和代码签名身份授予。Manifest permission 与本机 `granted_permissions` 只表示用户允许 Plugin 使用这项能力，不代表 macOS TCC 已经授权；Plugin 在每次敏感工具调用前仍须自行检查并在缺失时失败关闭。

## 5. 签名与不可变性

- Catalog key 只签 Catalog，Release key 只签 Release，不能跨用途复用。
- publisher identity、Marketplace、Release key 和签名 payload 必须属于同一条已审核信任链。
- Catalog revision 和 `issued_at` 必须单调前进，客户端拒绝重放和降级。
- Release 的 plugin ID、version、npm package identity、integrity、artifact SHA-256、Manifest、组件和权限发布后不可原地覆盖。
- key 轮换先加入 successor key，确认新 Release 可验证后再撤销旧 key。
- 供应链事件通过 revoke Release 或 suspend publisher 处理，客户端同步后停止新的安装和调用。

## 6. 执行链路

任务调用插件时固定使用以下链路：

```text
Task Runner
  -> MCP Management
  -> Local Connector Service
  -> Local Connector Client
  -> 本地启动 stdio MCP / 本地请求 HTTP MCP
  -> 原路返回工具结果
```

Local Connector Service 是双向 Relay，不执行插件，也不访问用户机器的 localhost。Plugin Management 是控制面和信任根，不执行 MCP。Task Runner 只使用任务创建时固定的 Release/component snapshot，不临时选择其他版本或其他执行位置。

## 7. 发布验收

至少验证：

```bash
cargo test -p chatos_plugin_management_sdk --test third_party_plugin_examples
cargo test -p plugin_management_service_backend --lib
cargo test -p local_connector_client_core plugins
```

真实 Release 还必须覆盖：

- `.tgz` 的 SHA-512 integrity 和 SHA-256 均能复验。
- package name、version 和 `package.json.bin` 与 Manifest/Release 一致。
- 非法路径、软链接、设备文件、archive bomb 和额外可执行入口被拒绝。
- 客户端离线、未安装、Release revoked、权限拒绝或凭据失效时失败关闭。
- stdio 子进程取消、超时、退出和清理正确；HTTP MCP 超时、TLS 和 URL 策略正确。
- 日志、审计和诊断中不包含凭据、用户文件内容、屏幕内容或完整工具 payload。

生产环境必须持久化 Plugin Management Artifact volume，并限制平台托管签名密钥目录权限。私钥不得进入源码、npm package、数据库返回、浏览器响应或 CI 日志。
