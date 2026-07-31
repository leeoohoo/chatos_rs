# Ponytail ChatOS Release 发布手册

Ponytail 适配包位于 `plugins/ponytail`，版本为 `4.8.4-chatos.1`。它是 schema v2 Portable Prompt Plugin：包含一个 Skill、四个 Command 和 Cloud/Local 各三档 Agent Profile，不包含 Hook、MCP、Node 进程、状态文件、OAuth 或运行时权限。

## 0. 默认 `chatos-bundled` 交付

ChatOS 默认启动 seed 会把 Ponytail 作为 `bundled-plugin-ponytail` 写入现有 `chatos-bundled` Marketplace，而不是创建第二个 Marketplace：

- 编译时嵌入并校验 Manifest、精确 checksums、SPDX SBOM 和全部 Prompt 文件；
- 生成不可变 Release、11 个 component snapshot 和 11 个 canonical Cloud Bundle；
- 先以 `ready=false` 落库，全部校验和持久化完成后再切换为 `ready=true`；
- 绑定到 `task_runner_run_phase` 和 `task_runner_local_run_phase`，因此普通云端/本地执行任务可以发现它；
- 首次为 seed System Admin 创建启用偏好，尊重后续人工禁用，不在重启时重新开启；
- 其他用户在“插件目录”启用后，即可在 ChatOS 普通任务的 Plugin Picker 中选择。

内置 Release 使用只适用于编译时 bundled content 的确定性 attestation key，不能给网络下载 artifact 或第三方 Release 继承。默认本地环境重启 Plugin Management Service 后即可完成 seed，不需要外部生产私钥。

## 1. 发布前检查

确认以下内容：

- `CHATOS-ADAPTATION.md` 说明与上游差异；
- `licenses/ponytail-LICENSE` 保留 Dietrich Gebert 的 MIT attribution；
- `.chatos-plugin/plugin.json` 的默认执行位置为 `portable`；
- `checksums.json` 精确覆盖除自身以外的全部文件；
- `sbom.spdx.json` 合法；
- Cloud 与 Local Profile 的 `baseAgent` 分别匹配对应 Task Runner Agent plane；
- 包内不存在 Hook、MCP、脚本、二进制、凭据或本地持久状态。

生成并校验 metadata：

```bash
node scripts/package-plugin-release.mjs --write-metadata plugins/ponytail
node scripts/package-plugin-release.mjs --verify plugins/ponytail
```

`--verify` 会调用 Rust SDK 解析 Manifest 并计算 normalized Manifest SHA-256，不能用原始 JSON 字节或 `JSON.stringify` hash 替代。

## 2. 生成确定性 artifact

```bash
node scripts/package-plugin-release.mjs --package plugins/ponytail \
  --output /tmp/ponytail-4.8.4-chatos.1.zip
```

脚本会固定 ZIP 文件时间和排序，输出：

- ZIP 路径；
- artifact SHA-256；
- 同路径的 `.release.json` 待签名模板。

模板中的 `plugin_id` 必须替换为 Plugin Management Catalog 中的真实 ID，不能使用 Manifest name 代替。`artifact_ref` 必须替换为生产 HTTPS 地址。

## 3. 网络 Catalog 的生产签名

以下流程只适用于把 Ponytail 作为网络 artifact 发布到独立生产 Catalog；默认 `chatos-bundled` 编译时交付不走该流程。仓库不保存网络 Catalog 的生产 Ed25519 私钥，这是正确且必须保持的状态。网络 Release 必须由外部生产 signer 完成：

1. 使用 approved publisher 的 active release-only key；
2. 填写真实 `key_id`、`publisher_id`、`marketplace_id` 和 RFC3339 `signed_at`；
3. 使用 SDK 定义的 Release signing payload 签名；
4. 写入 Base64 signature；
5. 保持 `.release.json` 中的 artifact SHA-256 和 normalized Manifest SHA-256 与刚生成的包完全一致。

不要在仓库、CI 日志、Artifact metadata、Catalog、审计或聊天记录中粘贴私钥。测试 fixture key 不能用于生产 Catalog。

## 4. 发布顺序

1. 先部署 `plugin_cloud_component_bundles` 和 `plugin_release_publication_states` 的索引；
2. 部署支持 schema v2/readiness 的 Plugin Management；
3. 部署同版本 Task Runner、ChatOS 与 Local Connector；
4. 上传确定性 ZIP 到生产 HTTPS artifact 地址；
5. 由外部 signer 生成正式 Release signature；
6. 写入 signed Catalog 或调用管理员 Release 发布接口；
7. 等待 Release publication state 变为 ready；
8. 验证 Cloud Bundle identity、component snapshot 和 Bundle hash 一致；
9. 执行云端与本地 E2E。

发布失败时不要修改同一个不可变 Release 的内容。修正构建输入后生成新版本；Catalog materialization 的可重试 staging 只能用于补完尚未 ready、内容仍完全相同的 Release。

## 5. 验收

Cloud：

- 没有 Local Connector 仍能发现和选择 Ponytail；
- Run snapshot 的 `device_id` 为空；
- 不创建 Relay；
- Skill、Command、Agent Prompt 按稳定顺序注入；
- UI 显示 Task Runner Cloud provider 和 content SHA-256。

Local：

- 安装的是同一 artifact SHA-256 的 Release；
- Local Connector 从安装目录重建 canonical Bundle；
- raw `SKILL.md` hash 不能替代 Bundle hash；
- Profile 只显示与 Local Agent plane 兼容的 lite/full/ultra；
- 不启动 Node、MCP、Hook 或其他进程。

供应链与快照：

- artifact、Manifest、checksum、SBOM 或 Bundle hash 任一漂移均失败关闭；
- 新 Release 不改变已开始 Run；
- 撤销后新 Run 拒绝，历史 Run 仍可审计；
- 日志不包含 Ponytail 完整 Prompt 正文。

## 6. 发布门禁

```bash
cargo test -p chatos_plugin_management_sdk
cargo test -p chatos_plugin_package
cargo test -p plugin_management_service_backend --lib
cargo test -p task_runner_service_backend --lib
cargo test -p local_connector_client_core --lib
cargo test -p chat_app_server_rs --lib
npm run type-check --prefix chatos/frontend
node --test local_connector_client/tests/prepare-plugin-bundles.test.mjs
node scripts/package-plugin-release.mjs --verify plugins/ponytail
git diff --check
```
