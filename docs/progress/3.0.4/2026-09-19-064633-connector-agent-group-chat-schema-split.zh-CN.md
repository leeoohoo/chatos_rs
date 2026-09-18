# 3.0.4 进度：拆分 Agent Group Chat Schema

- 时间：2026-09-19 06:46:33（Asia/Shanghai）
- 本轮目标：保持 SQL、索引、事务和迁移行为不变，将 Agent Group Chat 初始 Schema 从 SQLite actor facade 纯拆分到独立文件。
- 起始提交：`f3a32bc65`
- 代码提交：`a627daa93`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatSchema.swift`。
- 将完整初始 Schema SQL 移入 `AgentGroupChatSchema.definition`。
- `SQLiteAgentGroupChatStore` 初始化时改为引用独立 Schema 定义；数据库打开参数、busy timeout、执行顺序、错误处理与随后迁移调用均不变。
- `SQLiteAgentGroupChatStore.swift` 从 7,311 行降至 6,805 行；独立 Schema 文件为 513 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatSchema.swift`

## 业务不变量

- Schema SQL 字符串内容逐字不变。
- 表、列、约束、索引、外键、默认值和初始 migration marker 不变。
- `PRAGMA journal_mode`、`foreign_keys` 与初始 `BEGIN IMMEDIATE` / `COMMIT` 边界不变。
- 数据库创建、已有数据库打开和后续迁移调用顺序不变。
- facade 的公共 API 与 actor 隔离不变。

## 验证结果

- Schema SQL 移动前后逐字比对：通过（`BYTE_FOR_BYTE_SCHEMA_SQL_MATCH`）。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败：
  - `AgentGroupChatCodableContractTests`
  - `LocalAgentDraftTests`
  - `SQLiteAgentGroupChatStoreTests`
  - `LocalAgentChatToolProviderTests`
- `swift test --package-path clients/macos`：退出码 0，未出现失败输出。
- 全量测试结束后确认仍遗留 2 个已知孤儿 `fixture.zsh` 测试进程，本轮已精确清理，未将其计入产品行为变更。

## 剩余风险

- 迁移逻辑仍留在 facade 文件中，下一有界单元需要单独抽出并保持旧库升级路径逐字/逐版本一致。
- 测试资源泄漏问题仍待独立修复；在修复前，每次全量验证后需清理其遗留进程，避免污染下一轮门禁。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 将 `migrateConversationSchema` 及其内部迁移辅助逻辑纯移动到 `AgentGroupChatMigrations.swift`，保持 migration SQL、版本 marker、事务和失败回滚语义不变，并运行历史迁移用例及完整 macOS 测试。
