# Plugin 用户与运维手册

本文说明单轨 Marketplace npm MCP 的用户、管理员和本机运维流程。第三方 package 和 Release 的制作要求见 [第三方 Plugin 发布与接入手册](./third-party-plugin-publishing.zh-CN.md)。

Browser CDP Plugin 的专项开发、权限、安装和端到端验收规则见 [Browser CDP MCP 开发、发布与安装规范](./browser-cdp-mcp-development-and-publishing-spec.zh-CN.md)。

## 1. 系统边界

- Plugin Management Service：维护 Marketplace、publisher、签名 Catalog、不可变 Release、权限、安装状态投影和审计，是插件信任控制面。
- ChatOS：创建任务前读取当前用户在线 Local Connector Client 已安装且可用的插件列表，根据用户选择和任务需要保存 `selected_plugins`。
- Task Runner 和 MCP Management：固定并校验任务的 Release/component snapshot，把 MCP 调用路由到指定 Local Connector Client。
- Local Connector Service：维护客户端出站连接并转发请求和结果，只承担 Relay，不安装也不执行插件。
- Local Connector Client：从可信 Catalog 下载和验证 npm `.tgz`，管理安装、权限和凭据，在用户设备上启动 stdio MCP 或请求 HTTP MCP。

所有 MCP 都在客户端一侧执行。HTTP MCP 的“本地执行”表示 HTTP 请求由 Local Connector Client 发起；目标服务可以是外部 HTTPS endpoint，但服务端组件不能代替客户端调用。

系统没有 Cloud、Portable、Hybrid、ZIP Plugin、bundled Plugin、内置 Skill/native adapter、直接 MCP 配置或任何 fallback 路径。

## 2. 用户流程

### 2.1 安装插件

1. 启动并登录 Local Connector Client，确认设备在线。
2. 在 Plugin Marketplace 查看 publisher、版本、权限、平台、许可证和 Release 状态。
3. 点击安装。客户端从可信 Catalog 获得签名 Release 和 npm artifact URL。
4. 客户端下载 `.tgz` 并验证 Catalog/Release 签名、npm SHA-512 integrity、artifact SHA-256、package name/version、`package.json.bin` 和安全解包规则。
5. 根据插件声明完成本地权限确认、OAuth 或 credential 连接。
6. 客户端上报安装和组件可用性。只有 installed、active、未撤销且权限/凭据满足的插件可供任务选择。

用户不能通过 URL、文件上传或手写配置安装未上架 MCP，也不能从客户端包中启用预置能力。

### 2.2 创建任务

ChatOS 创建任务时应执行：

1. 查询当前用户在线 Local Connector Client 的已安装插件和组件能力。
2. 根据任务内容给出插件建议，或使用用户显式选择的插件。
3. 把 plugin、Release、component、device 和 workspace snapshot 写入任务。
4. 如果需要的插件未安装、客户端离线或组件不可用，创建/启动阶段明确报错，不静默换版本或绕过本地执行。

ChatOS 自身的 Agent 不直接调用插件；插件由它创建的任务在 Task Runner 执行期间使用。

### 2.3 运行任务

```text
Task Runner
  -> MCP Management
  -> Local Connector Service
  -> Local Connector Client
  -> stdio MCP 子进程 / HTTP MCP 请求
  -> Local Connector Service
  -> MCP Management
  -> Task Runner
```

Run 开始后始终使用创建任务时固定的 snapshot。Marketplace 发布新版本不会改变正在运行的任务。客户端断线、安装漂移、签名或 hash 不一致、Release 被撤销、权限拒绝、凭据失效时，调用失败关闭。

### 2.4 OAuth 和凭据

- OAuth token、refresh token、API key 和 webhook secret 只保存在受管理的凭据边界中，不进入 Manifest、Catalog、Release、任务 snapshot、审计或诊断导出。
- 前端只展示 provider、scope、connected、expires_at、account_display 等必要状态。
- 服务端只下发经过授权的临时参数或 credential reference，客户端在执行前解析并注入。
- 连接失效后必须重新授权，不能自动借用其他用户、设备或账号的凭据。

## 3. 管理员流程

### 3.1 Marketplace 和 publisher

- Marketplace Catalog URL 必须是无 credential、无 fragment 的 HTTPS URL。
- trusted Marketplace 至少保留一个未撤销的 Ed25519 Catalog signing key。
- `super_admin` 审核 publisher identity、网站、许可证和供应链信息；平台在首次发布时为 Marketplace + publisher 创建托管的 Release-only signing key。
- 平台托管 key 的轮换先增加 successor key，再撤销旧 key；Catalog key 和 Release key 不得混用。
- Catalog sync 验证签名、revision/issued_at 单调性、publisher identity、Release 不可变性和 revocation。

### 3.2 Release 审核

管理员不再分别手工创建 Catalog 和填写 Release JSON。标准上架流程是：

1. 在“Marketplace”确认目标 Marketplace 已启用，类型为 `admin_registry`，信任级别为 `trusted`。
2. 在“Publisher”确认发布者状态为 `approved`，并且属于同一个 Marketplace。
3. 进入“Plugin Catalog”，点击“上架 Plugin”。
4. 上传 `npm pack` 生成的 `.tgz`；包内没有 `chatos.plugin.json` 时，再上传独立 Manifest JSON。
5. 点击“校验并预览”。平台自动解析 package name/version/bin、Manifest、组件和权限，并自动计算 npm SHA-512 integrity 与 artifact SHA-256。
6. 选择 publisher、许可证、是否允许再分发、可见性和 Release channel 后点击发布。
7. 平台保存不可变 Artifact，自动创建或复用 Catalog，使用平台托管 Ed25519 key 签署 Release，并打开 Release 列表供复核。

上架页面不要求填写 artifact URL、hash、integrity、Manifest JSON 文本或 Signature JSON。Artifact 下载地址由平台生成，客户端仍会独立复验所有签名和 hash。

每个 Release 必须确认：

- artifact 是标准 npm `.tgz`，不是 ZIP 或任意目录包。
- Manifest `schemaVersion` 为 3。
- stdio MCP 的 `bin` 存在于 `package.json.bin`，没有任意 command 或运行时 `npx @latest`。
- npm package name/version、SHA-512 integrity 和 artifact SHA-256 完整且一致。
- 权限按组件声明，网络、文件、进程、屏幕和外部服务访问范围合理。
- package 无路径穿越、危险链接、设备文件、未审核二进制下载和秘密材料。
- 支持平台和最低客户端版本真实可用。

Release 发布后不可覆盖。修复必须发布新版本；供应链事件使用 revoke Release 或 suspend publisher。

## 4. 安装与调用诊断

安装诊断至少应展示：

- plugin ID、Release ID、version、npm package name/version。
- device、platform、install/availability/active 状态。
- artifact SHA-256 和已验证 Catalog revision。
- dependency、permission、auth 状态。
- component key、kind、runtime kind、最后检查时间和脱敏错误状态。

脱敏导出必须移除 device 原值、账号展示名、错误原文、token、cookie、API key、用户文件、屏幕/浏览器内容和工具 payload。

常见故障：

| 现象 | 优先检查 |
| --- | --- |
| Marketplace sync 失败 | Catalog HTTPS、DNS/TLS、Catalog key、revision/issued_at、大小限制 |
| Release 无法发布 | publisher 审核、Release key usage、schema v3、npm identity/integrity、artifact SHA-256 |
| 客户端无法安装 | Release 是否 revoked、平台/版本、下载 URL、磁盘空间、安全解包或 package bin 校验 |
| ChatOS 创建任务时看不到插件 | 客户端是否在线、插件是否 installed/active、组件可用性是否已同步、用户和 device 是否匹配 |
| Run 调用插件失败 | pinned snapshot、Relay 连接、workspace、权限、凭据、MCP initialize 和超时 |
| HTTP MCP 失败 | URL 是否 HTTPS/loopback、TLS、header 策略、OAuth、`tools/list`/`tools/call` 支持 |
| 插件版本与预期不同 | 检查任务固定的 Release snapshot；已开始 Run 不接受中途升级 |

## 5. 部署和监控

### 5.1 服务端

- Plugin Management Service 使用 MongoDB 保存控制面数据，客户端不能直连数据库。
- Plugin Artifact 和平台托管签名密钥保存在 `PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR`；生产部署必须使用持久卷并备份，不能使用容器临时文件系统。
- `PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL` 必须是客户端可访问的 HTTPS 地址；默认通过 `https://plugin.jgoool.com/api/plugin-artifacts/<sha256>` 提供不可变 `.tgz` 下载。
- `PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES` 控制单包上传上限；反向代理的 body size 上限不得小于该值。
- Local Connector Service 必须只接受已认证客户端的出站连接，并验证 Relay 消息的平台签名和设备绑定。
- MCP Management 和 Task Runner 不得直接启动 npm MCP、请求外部 HTTP MCP 或访问用户机器 localhost。
- Catalog sync、Release revoke 和 publisher suspend 应产生可审计事件。

### 5.2 客户端

- 安装目录必须由客户端管理，拒绝 symlink traversal、archive bomb、hash/signature drift 和安装目录外执行。
- stdio 子进程只能从已验证 npm package root 和声明的 bin 启动。
- HTTP MCP 只能由客户端 runtime 请求允许的 HTTPS 或 loopback HTTP URL。
- 操作系统权限、用户确认、credential 注入和 workspace 边界都在客户端强制执行。
- 客户端升级不能预置或恢复 Plugin、Skill、Computer Use helper、Office/PDF runtime。

### 5.3 最小指标

- Catalog sync 与签名验证成功率。
- npm 下载、SHA-512/SHA-256、解包和安装成功率。
- 客户端在线率、插件 availability 同步延迟。
- Relay、prepare、execute、cancel 成功率和超时。
- stdio MCP 启动/退出与 HTTP MCP 请求失败原因。
- 权限拒绝、OAuth 失效、Release revoked 和 snapshot drift 命中次数。

指标不得包含凭据、用户文件、屏幕/浏览器内容、MCP 参数或结果原文。

## 6. 最小验收清单

- 只有已审核并签名的 Marketplace npm MCP Release 能被客户端安装。
- 客户端能复验签名、SHA-512 integrity、SHA-256、package identity 和 `package.json.bin`。
- ChatOS 创建任务时能看到目标客户端已安装插件，并把正确 snapshot 固定到任务。
- stdio 和 HTTP MCP 都通过 Relay 到 Local Connector Client 执行，结果原路返回 Task Runner。
- 客户端离线、未安装、撤销、漂移、权限拒绝或凭据失效时失败关闭。
- 系统不存在旧插件包、客户端预置能力、直接 MCP 配置或其他执行旁路。
