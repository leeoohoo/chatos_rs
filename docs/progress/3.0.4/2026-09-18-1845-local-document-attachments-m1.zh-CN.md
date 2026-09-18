# 3.0.4 推进记录：本地文档附件 M1

- 时间：2026-09-18 18:45:28 CST（Asia/Shanghai）
- 本轮目标：完成本地 Agent 精简消息与长文附件方案的 M1 离线闭环。
- 起始提交：`52dc53fd3`
- 本轮代码提交：`c3f2f8d79`

## 实际改动

1. 新增 `chat_document_create`，在现有 `AgentGroupChatAttachments/.drafts` 受控目录生成 UTF-8 Markdown 草稿。
2. 文档工具自动清洗展示文件名、补齐 `.md`、计算字节数和 SHA-256，只向模型返回 Run 内临时 `document_ref`。
3. `LocalAgentRunReferenceVault` 增加文档权威记录、预留/释放/消费状态、完整性复核、Run 总量限制和发送结果幂等回放。
4. `chat_inbox_send`、`chat_direct_send`、`chat_team_send`、`chat_send_message` 全部支持最多 5 个唯一 `document_refs`。
5. 四个发送工具在 JSON Schema 和运行时统一使用 `AgentCommunicationPolicy.standard.maximumMessageCharacters = 2_000`，超限返回 `message_too_long` 和 `chat_document_create` 修复指引，不截断正文。
6. 发送前重新读取草稿并校验大小与 SHA-256；现有 `postMessage` 在同一个 SQLite 事务内持久化消息与附件，失败释放引用，成功消费引用并删除草稿。
7. Agent 生成的 Markdown 复用现有附件模型、卡片和 `chat_read_attachment` 分段读取路径，不引入第二套读取协议。
8. 共享沟通 Skill 升级到 v2；manager 新 Run 使用已经可用的强制文档规则，旧 Run 保留 checkpoint 中的既有指令文本和版本。
9. 产品策略增加单 Run 最多 20 个文档，继续受 8 MiB 总量上限约束。

## 安全与业务不变量

- 模型不能指定真实用户、Agent、Run、房间、消息、磁盘路径、bucket、object key 或 URL。
- `document_ref` 只存在于当前 provider/Run vault；伪造、已消费、并发占用或其他 Run 引用统一拒绝且不泄露目标是否存在。
- 草稿磁盘文件使用程序生成 opaque 文件名和 0600 权限，目录使用 0700 权限；Run vault 释放时清理未发送草稿。
- MinIO 不进入 M1 主链路；完全离线时消息和 Markdown 附件仍可创建、发送、显示并分段读取。
- Human 原有 64,000 字消息能力未改变；2,000 字限制只施加于四个 Agent 聊天发送工具。
- 原有 manager/executor、Todo、delivery、@ 路由和已读语义未改变。
- 用户提供的测试账号未写入代码、测试、日志或提交。
- 工作区原有 ViewModel、Workspace 和 Store 测试并行改动未纳入本轮提交。

## 验证结果

1. M1 定向测试通过：
   - `swift test --package-path clients/macos --filter 'LocalAgentChatToolProviderTests|LocalAgentSkillCatalogTests|LocalAgentGroupChatSchedulerTests'`
2. 覆盖内容包括四个发送 Schema、2,000/2,001 字边界、文件名清洗、2 MiB 超限、SHA-256、附件事务、分段读取、引用消费、相同发送调用幂等和调用 ID 参数冲突。
3. Connector 全套复跑通过：
   - `swift test --package-path clients/macos --filter ChatOSConnectorTests`
4. 完整测试两次均只出现一个非本轮波动：`NativeProjectGitServiceTests.testLoadsChangesStagesCommitsAndBuildsDiff` 在并行全套运行时临时仓库 upstream 偶发为 `nil`；该用例单独复跑通过，后续 Connector 全套也通过。其余目标和测试通过。
5. `git diff --check` 通过。

## 下一步

进入 M2：新增账户级 Agent artifact API 与鉴权元数据，扩展 macOS API service 和本地附件远端字段，加入同步 outbox、退避重试、orphan 清理及远端恢复，同时保证上传失败不阻塞本地消息事务。
