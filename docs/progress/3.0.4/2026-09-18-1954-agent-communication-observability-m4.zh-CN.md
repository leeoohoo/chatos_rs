# 3.0.4 推进记录：Agent 沟通观测与灰度验收 M4

- 时间：2026-09-18 19:54:53 CST（Asia/Shanghai）
- 本轮目标：完成精简消息与长文附件方案 M4 的隐私安全观测、阈值边界验证和完整 macOS 回归。
- 起始提交：`a93ed0882`
- 本轮代码提交：`ceb46860b`

## 实际改动

1. 新增账户隔离的 `local_agent_communication_metrics` 聚合表和 SQLite schema 24 迁移。
2. 记录四个发送工具的正文字符数分布，分桶为 0～300、301～800、801～2,000 和 2,001 以上；超过 2,000 字符时同时记录 `message_too_long` 拒绝原因。
3. 记录 `chat_document_create` 的成功、空内容、过大、数量上限、Run 总量上限、名称/标题无效和本地存储失败结果，并聚合文档字节数。
4. 记录 artifact 上传成功/失败及字节数，保留离线 outbox 和重试语义不变。
5. 记录 Markdown 预览成功/失败、累计耗时和最大耗时。
6. 记录文档引用过多、重复、无效和本地完整性变化等工具拒绝原因。
7. 新增独立 `AgentCommunicationMetricsTests`，覆盖 schema 24 从旧库升级、300/301/800/801/2,000/2,001 边界、聚合计数/总值/最大值、账户隔离和固定维度白名单。

## 隐私与业务不变量

- 指标只保存固定 metric name、固定枚举维度、事件数、总值、最大值和更新时间。
- 不保存消息正文、Markdown 内容、文档名、真实用户/Agent/房间/消息 ID、本机路径、artifact ID、bucket、object key 或授权 URL。
- 指标写入失败不会改变消息发送、文档创建、上传、预览或重试的业务结果。
- 2,000 字硬限制、Run 临时引用权限、本地优先附件事务和 MinIO 后台同步语义未改变。
- 用户提供的测试账号未写入代码、测试、文档、日志或提交。
- 工作区原有 ViewModel、Workspace 和 Store 测试并行改动未纳入本轮提交。

## 验证结果

1. 观测性定向测试通过：
   - `swift test --package-path clients/macos --filter AgentCommunicationMetricsTests`
   - 3 tests，0 failed。
2. M1～M3 关键回归通过：
   - `swift test --package-path clients/macos --filter 'LocalAgentChatToolProviderTests|AgentArtifactSyncTests|MarkdownRenderCacheTests'`
   - XCTest 7 tests、Swift Testing 3 tests，0 failed。
3. macOS 完整测试通过：
   - `swift test --package-path clients/macos`
   - Core、Connector、App、AgentRuntime、API 及 Swift Testing 全部 0 failed；仅有既有 SwiftTerm 构建缓存 warning。
4. `git diff --cached --check` 通过。

## 结论与下一步

M0～M4 的代码闭环已经完成，已具备用聚合数据校准 800/2,000 字阈值和识别连续短消息规避倾向的基础。下一轮先按方案完成后端、macOS、离线恢复和真实交互的整体验收；验收通过后进入 `3.0.4-native-client-refactor-and-parity-plan.zh-CN.md` 的 macOS 大文件拆分与架构重构。
