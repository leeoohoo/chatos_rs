# 第三方 Plugin 发布与接入手册

本手册面向需要把 CircleCI、Sentry、Build Web 或其他外部服务接入 ChatOS/Codex 风格 Plugin Runtime 的发布者和运维人员。目标是：新增第三方插件只需要发布合规 Plugin Release 和必要 Adapter，不再修改 ChatOS、Task Runner 或 Local Connector 主流程代码。

## 1. 发布路径

1. 管理员创建或选择一个公开、启用、可信的 `admin_registry` Plugin Marketplace，并配置 HTTPS Catalog URL 与至少一个 active Catalog signing key。
2. 发布者在 Plugin Management 的“发布者审核”页面提交 publisher ID、名称、HTTPS 网站和 1-32 个 active Ed25519 release-only key。
3. super admin 审核通过发布者。通过时，release key 会并入 Marketplace trust root；拒绝、暂停和恢复都会进入审计。
4. 发布者生成 Plugin artifact、Manifest、SBOM、Release signature，并把 Release 写入 Marketplace signed Catalog。
5. 管理员或 owner 触发 Catalog sync。服务端会验证 Catalog/Release 签名、key usage、revision 单调性、artifact hash、release immutability、publisher identity 和 revocation。
6. 用户安装或选择 Plugin。Task Runner 和 Local Connector 只使用已安装的 immutable Release snapshot。

## 2. 身份与密钥边界

- `publisher.id`、`publisher.name`、`publisher.website` 必须和 approved publisher record 完全一致。
- Release signature 的 `publisher_id`、`marketplace_id`、`key_id` 必须指向同一 approved publisher 和未撤销 release key。
- Catalog key 只能签 Catalog；Release key 只能签 Release。不要复用同一 key ID 或 key material 跨 usage。
- 轮换 release key 时先发布 successor key，待新 Release 可验证后再撤销旧 key；非 revoked key 不能直接从 trust root 消失。
- 审计只记录 key ID、决策和状态，不记录 public key material 或用户凭据。

## 3. Runtime 组件选择

第三方服务通常优先使用以下组件组合：

- `mcpServers`：适合 CircleCI/Sentry 这类远程 API 操作。生产建议使用 HTTPS MCP endpoint；开发可使用 loopback HTTP。
- `commands`：适合把高层工作流暴露为用户可显式调用的 Markdown 命令，例如“诊断失败构建”。
- `agents`：适合多步排障、归因、修复建议等受限工具链任务。`baseAgent` 只能是 `task_runner_plan_phase` 或 `task_runner_run_phase`。
- `ui`：适合只读状态面板、Artifact viewer 或 Workbench。入口必须在 `./ui/` 下，bridge capability 必须逐项声明。
- `apps`：适合需要 OAuth 或本机 connected app 绑定的服务。凭据不要写入 Manifest，使用 credential reference 或 connected app grant。

## 4. 凭据与权限

- stdio MCP 的环境变量值只能使用 `${credential:<name>}` 模板。
- 不要在 Manifest、Catalog、Release signature、审计、Artifact 或 UI URL 中放 token、cookie、API key 或 webhook secret。
- HTTP MCP 应使用 Host 注入的 OAuth/credential grant；如果服务需要 webhook，webhook secret 由部署环境或 connected app 保存。
- `permissions` 必须写到组件粒度。Build、deploy、issue mutation、artifact write 等能力要分开声明，便于审批和审计。

## 5. 示例 Manifest

仓库提供三个可解析示例：

- `docs/plugins/examples/circleci-plugin.manifest.json`
- `docs/plugins/examples/sentry-plugin.manifest.json`
- `docs/plugins/examples/build-web-plugin.manifest.json`

这些示例会被 `chatos_plugin_management_sdk` 测试直接解析，确保字段名、组件类型、权限和 URL 合同不会随 schema 演进而过期。

## 6. 运维检查清单

- Marketplace Catalog URL 是无 credential、无 fragment 的 HTTPS URL。
- Catalog revision 和 issued_at 单调前进，不能重放旧 Catalog。
- Artifact URL 是无 credential 的 HTTPS URL，下载后 SHA-256 与 Release 完全一致。
- 每个 Release 都有 SBOM 或明确的 SBOM 缺失记录；许可审查未完成时使用 pending redistribution 状态。
- Plugin UI resource origin 已部署并通过离线检查；公网 DNS/TLS 检查必须在真实部署后单独执行。
- Mongo driver、Windows installed-app、Linux host、Chrome/Computer Use 等真实环境验收不能用离线测试替代。

## 7. 最小发布验收

发布前至少运行：

```bash
cargo test -p chatos_plugin_management_sdk --test third_party_plugin_examples
cargo test -p plugin_management_service_backend --lib
```

如果发布包包含本机 stdio MCP、Hook、UI 或 Artifact 写入，还必须补充对应 Local Connector packaged E2E，并确认没有启动未授权 listener、没有占用固定端口、没有把凭据或用户文件内容写入审计。
