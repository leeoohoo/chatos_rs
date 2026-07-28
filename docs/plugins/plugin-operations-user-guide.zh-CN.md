# Plugin 用户与运维手册

本文面向 ChatOS/Okra 的最终用户、管理员和本机运行时运维人员，说明 Plugin Marketplace、安装、OAuth、诊断、审计和部署边界如何协同工作。第三方发布者如何产出 signed Catalog/Release 见 [第三方 Plugin 发布与接入手册](./third-party-plugin-publishing.zh-CN.md)。

## 1. 组件和数据边界

Plugin 体系分为四层：

- Plugin Management Service：保存 Marketplace、Catalog Entry、不可变 Release、publisher review、用户偏好、安装/OAuth 状态投影和审计。它是服务端控制面，需要 MongoDB。
- Local Connector Client / Service：运行在用户设备侧，负责下载、验签、安装、依赖检查、权限/OAuth、prepare/execute/cancel 和本机 Adapter 调用。
- Task Runner：在创建和执行任务时固定 Plugin snapshot，保证一次 Run 不被中途升级或回滚影响。
- ChatOS/Okra 客户端：展示插件市场、选择插件、承载 Plugin UI/Artifact，并把用户选择传给 Task Runner。

客户端本身不需要直接连接 MongoDB。MongoDB 只属于后端服务的持久化依赖；桌面客户端和 Local Connector 通过认证 API 同步状态，不应嵌入数据库账号、连接串或直连查询逻辑。

## 2. 用户侧流程

### 2.1 查看和安装 Plugin

1. 在客户端打开 Plugin Marketplace。
2. 搜索或按分类查看公开、个人或 featured Plugin。
3. 打开详情页，确认 publisher、版本、权限、OAuth/connected app 要求、支持平台和 release 状态。
4. 点击安装。Local Connector 会下载 artifact、校验 SHA-256、验证 Release 签名、检查平台和依赖。
5. 如果需要权限或 OAuth，客户端应逐项展示原因并让用户确认；失败时保持未启用或 unavailable，不做云端 fallback。
6. 安装完成后，用户可以启用 Plugin，并在 ChatOS 输入区或任务创建流程中选择它。

### 2.2 在任务中使用 Plugin

- 新任务会保存 `selected_plugins`，其中包含 plugin/release/component snapshot。
- Run 开始后，即使 Marketplace 里有新 Release，本次 Run 仍使用创建时固定的 snapshot。
- Plugin disabled、revoked、安装失效或本机设备离线时，新 Run 不能静默启用该 Plugin；旧 Run 按 snapshot 和策略失败关闭或只读保留状态。

### 2.3 OAuth 和 connected app

- OAuth token、refresh token、API key 和 webhook secret 不进入 Manifest、Catalog、Release、审计、Artifact 或诊断导出。
- 前端只展示 provider、scope、connected、expires_at、account_display 等必要状态；脱敏导出会移除 account_display。
- 连接失效时，用户需要重新授权；系统不能自动切换到其他账号或其他设备的凭据。

## 3. 管理员流程

### 3.1 Marketplace 管理

`super_admin` 可以创建和维护 trusted Admin Marketplace：

- Catalog URL 必须是无 credential、无 fragment 的 HTTPS URL。
- trusted network Marketplace 至少保留一个未撤销 Catalog signing key。
- key rotation 采用两阶段：先添加 successor key，再撤销旧 key；非 revoked key 不可直接删除。
- Marketplace 更新使用 compare-and-replace，遇到并发 Catalog sync 或其他管理员修改时应刷新后重试。

### 3.2 Publisher 审核

普通用户可以提交 publisher ID、名称、HTTPS 网站和 1-32 个 active Ed25519 release-only key。`super_admin` 审核时：

- approve 会把审核过的 Release key 合并进 Marketplace trust root。
- reject 和 suspend 必须写审核备注。
- 手工 Admin Catalog Entry 和 Release 发布必须匹配 approved publisher identity 与 approved release key。
- 审计只记录 key ID、状态和决策，不记录公钥内容或私钥材料。

### 3.3 Release 和撤销

- Release 是不可变的：plugin_id、version、artifact_sha256、signature、components 和 permissions 一旦发布不能被原地覆盖。
- 撤销 Release 后，客户端不应再激活新安装；已安装状态会在下一次同步/检查时变为 unavailable 或 revoked。
- 发生供应链事件时，优先 revoke Release 或 suspend publisher，再由客户端和 Local Connector 同步状态。

## 4. 诊断和排障

### 4.1 安装诊断

Plugin Management 前端的“安装诊断”页面面向普通用户和管理员开放，但只查询当前登录用户的数据。输入 `device_id` 后可查看：

- Plugin / Release / version / platform。
- install_status、availability_status。
- dependency_status、permission_status、auth_status。
- active、installed_at、last_checked_at、last_error 是否存在。
- 组件级 availability、kind、last_checked_at 和错误状态。
- 选中 Plugin 后的 OAuth provider、scope、connected、expires_at 和 updated_at。

页面提供“导出脱敏诊断”，默认移除：

- owner_user_id。
- device_id 原值。
- OAuth account_display。
- last_error 原文。
- token、cookie、API key、屏幕内容、浏览器页面内容、用户文件内容和 Plugin UI 私有数据。

导出只保留状态、版本、组件、provider/scope、时间戳和 `has_last_error` / `has_account_display` 布尔值，适合发给运维或开发者定位问题。

### 4.2 审计诊断

`super_admin` 可在“审计诊断”页面按 event、plugin_id、owner_user_id、device_id 查询 Marketplace、Catalog、Release、publisher review、安装来源、OAuth 和偏好变更事件。该页面用于回答：

- 哪个管理员修改了 Marketplace trust root。
- 某个 publisher 何时被批准、拒绝、暂停或恢复。
- Catalog sync、Release 发布或撤销是否成功。
- 用户 preference、安装来源或 OAuth 状态何时同步。

审计页面不用于查看用户文件、Hook stdout/stderr、工具 payload、OAuth token 或 Plugin UI 私有数据。

## 5. 部署和运维检查

### 5.1 服务端

- Plugin Management Service 需要 MongoDB 和 User Service。
- 后端环境变量包括 MongoDB URL、User Service URL、super admin seed、Catalog sync interval、Catalog request timeout 和最大 Catalog bytes。
- 生产环境应使用独立数据库、最小权限数据库账号、HTTPS reverse proxy、固定服务身份和集中日志。
- Catalog sync 可开启后台定时任务；高风险变更仍建议由管理员手动触发并查看审计结果。

### 5.2 客户端和 Local Connector

- Local Connector 只应保存本机必要状态、凭据引用和安装目录，不直连 MongoDB。
- 安装目录必须拒绝 symlink traversal、ZIP Slip、archive bomb、设备文件和 hash/signature drift。
- Browser、Chrome、Computer Use、Office、PDF 等本机能力必须按平台权限和逐次审批合同运行，不能由服务端强行执行。
- 真实 Windows、Linux、Chrome installed-session、Office live session、DNS/TLS/reverse-proxy 验收不能用离线单元测试替代。

### 5.3 指标建议

最小指标集合：

- Catalog sync 成功/失败和失败原因。
- Release 验签失败、artifact hash 失败、revoked release 命中。
- 安装成功率、安装耗时、回滚次数和 crash recovery。
- 依赖缺失、权限拒绝、OAuth 失效。
- prepare/execute/cancel 成功率和超时。
- Plugin disabled/revoked 后的新 Run 拒绝次数。

指标中不得包含用户文件内容、屏幕内容、Chrome 页面内容、OAuth token、Hook stdout/stderr 原文或 Plugin UI 私有 payload。

## 6. 常见故障定位

| 现象 | 优先检查 |
| --- | --- |
| Marketplace sync 失败 | Catalog URL 是否 HTTPS、DNS/TLS、Catalog key、revision/issued_at 单调性、Catalog size limit |
| Release 无法发布 | publisher 是否 approved、release key usage 是否正确、artifact SHA-256 和 signature payload 是否匹配 |
| 用户看不到 Plugin | Marketplace visibility、Catalog Entry enabled、用户 preference、Release revoked、Agent binding |
| 安装后不可用 | 安装诊断中的 availability_status、dependency_status、permission_status、auth_status |
| OAuth 工具失败 | 安装诊断 OAuth 连接状态、scope、expires_at、connected app grant |
| Run 中插件版本不对 | Task/Run 的 pinned snapshot；Run 中升级不会影响已开始的 Run |
| 审计中缺少敏感内容 | 这是预期行为；敏感原文不应进入审计或诊断导出 |

## 7. 最小验收清单

- 普通用户可以查看 Marketplace、申请 publisher、安装/启用个人可见 Plugin，并导出脱敏安装诊断。
- `super_admin` 可以编辑 trusted Marketplace、审核 publisher、发布/撤销 Release、查看审计诊断。
- 新任务能固定 selected Plugin snapshot，旧 Run 不受新 Release 影响。
- 离线、签名失败、版本错配、依赖缺失、权限拒绝、OAuth 失效时失败关闭，没有云端 fallback。
- 脱敏导出和审计均不包含凭据、屏幕/浏览器内容、用户文件内容或工具 payload 原文。
