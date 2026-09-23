# Agent 连续拆消息观测

- 时间：2026-09-20 11:41:38 CST（Asia/Shanghai）
- 本轮目标：完成 M4 的连续短消息规避长文附件策略观测，且不记录消息正文或真实 Agent/Run 标识。
- 起始提交：`7afb7518b`
- 代码提交：`812aa4ea0`

## 实际改动

- 新增 SQLite migration 25 与 `local_agent_message_sequence_events`，保存账户范围的 Run 指纹、序号、字符数、附件数和时间。
- 四个消息发送入口仅在持久化成功后记录一次观测事件；已有 replay 快速路径不重复计数。
- 在 5 分钟窗口中，同一 Run 的多条无附件消息累计首次超过 2,000 字时，聚合记录 `message_sequence / possible_limit_bypass`。
- 事件保留 24 小时；窗口统计过期后清零，但保留期内序号持续递增，避免主键冲突。
- 新增观测、隐私边界、窗口过期和历史迁移测试。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatMigrations.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/SQLiteAgentGroupChatStore+CommunicationSequence.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/SQLiteAgentGroupChatStore+MessageWrite.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatMessageCompletionTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatSendTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatDirectMessageTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatInboxTools.swift`
- `clients/macos/Tests/ChatOSConnectorTests/AgentCommunicationSequenceTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatHistoricalMigrationTests.swift`

## 业务不变量

- 不保存消息正文、真实 Agent ID 或真实 Run ID；仅保存 SHA-256 指纹和数值统计。
- 观测失败不得影响已成功持久化的消息发送。
- 消息仍在原事务中写入，文档引用仍只在成功后消费。
- replay 调用不新增消息，也不重复记录观测事件。
- 附带文档的消息序列不标记为可能绕过长文附件策略。

## 验证结果

- `swift test --package-path clients/macos --filter AgentCommunicationSequenceTests`：2 通过，0 失败。
- `swift test --package-path clients/macos --filter LocalAgentChatToolProviderTests`：5 通过，0 失败。
- `swift test --package-path clients/macos --filter AgentGroupChatHistoricalMigrationTests`：1 通过，0 失败。
- `swift test --package-path clients/macos`：全量通过；现有环境门禁测试按设计跳过。
- `git diff --check`：通过。

## 剩余风险与下一步

- 补齐 `chat_send_message`、`chat_team_send`、`chat_direct_send`、`chat_inbox_send` 四个入口携带附件的成功执行测试矩阵。
- 增加账户级 artifact 列表与 macOS 远端附件浏览/预览入口；本地群聊消息本身仍不宣称已完成跨设备同步。
- PostgreSQL 路由/鉴权集成测试、真实第二设备 E2E 和 100 条附件消息 UI/性能验证仍依赖可用运行时环境。
- 完成上述附件方案门禁后，再继续两份 macOS 大文件重构及有基准的性能修复。
