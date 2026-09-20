# Agent artifact 内容与大小边界修复

- 时间：2026-09-20 11:25:05 CST（Asia/Shanghai）
- 本轮目标：补齐方案复核发现的 artifact MIME/UTF-8 完成校验，并解除 2 MiB Markdown 与整个工具 JSON 同为 2 MiB 导致的合法边界冲突。
- 起始提交：`18f9620528e9f3855857bb8046639336016a70a3`
- 代码提交：`902a1698780f4ec65c3bf7b1687096242118a360`

## 实际改动

1. 服务端 `complete_upload` 在大小和 SHA-256 之外，继续校验对象存储返回的 MIME、charset 和 UTF-8 正文。
2. artifact 最大字节数改由 `CHATOS_AGENT_ARTIFACT_MAX_BYTES` 配置，默认仍为 2 MiB，并受对象存储总上传上限约束。
3. macOS artifact 客户端校验创建、完成和下载响应的 Markdown MIME；下载同时拒绝无效 UTF-8。
4. 工具参数 envelope 上限与文档正文上限解耦为 16 MiB；字段 Schema 和执行层仍分别执行 2 MiB 文档硬限制。
5. 增加最坏转义情况下 2 MiB Markdown JSON envelope 的回归测试，以及服务端 MIME/charset/UTF-8 拒绝测试。

## 涉及文件

- `chatos/backend/src/api/agent_artifacts.rs`
- `docker/compose.yml`
- `clients/macos/Sources/ChatOSAPI/ChatOSAgentArtifactService.swift`
- `clients/macos/Sources/ChatOSAgentRuntime/AgentSchemaValidator.swift`
- `clients/macos/Tests/ChatOSAgentRuntimeTests/AgentSchemaValidatorTests.swift`

## 业务不变量

- 单文档产品上限仍为 2 MiB，单 Run 总量仍为 8 MiB。
- 上传签名、账户归属、幂等键、大小和 SHA-256 校验不变。
- 模型仍不能看到 bucket、object key、预签名 URL 或本机路径。
- 本地离线消息与 artifact 后台上传行为不变。

## 验证结果

- `swift test --package-path clients/macos --filter AgentSchemaValidatorTests`：4 项通过。
- `swift test --package-path clients/macos --filter ChatOSAgentArtifactServiceTests`：1 项通过。
- `cargo test -p chat_app_server_rs agent_artifact`：4 项通过。
- `cargo fmt --all -- --check`：通过。
- `swift test --package-path clients/macos`：完整通过，0 失败；opt-in 性能基线按设计跳过。

## 并行改动与剩余风险

两个 Agent Group Chat ViewModel、两个 Story Workbench 布局文件及 `SQLiteAgentGroupChatStoreTests.swift` 的并行修改均保持未提交。artifact staged TTL、删除 outbox、跨设备发现、连续拆分消息观测和完整四发送工具附件矩阵仍需后续有界单元补齐。

## 下一步

实现服务端 staged artifact TTL 与可重试删除 outbox，再接入 macOS 本地删除队列和对应恢复测试。
