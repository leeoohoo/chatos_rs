# 3.0.4 进度：拆分 Read Cursor 定点读取 Repository

- 时间：2026-09-20 00:14:09（Asia/Shanghai）
- 本轮目标：保持消息 read cursor 的 owner/room/agent 定点读取与首行语义不变，将 SELECT 和行映射迁出 facade。
- 起始提交：`dfe3a9734`
- 代码提交：`fd988ae87`

## 实际改动

- 新增 `AgentReadCursorRepository`，承接 owner/room/agent 范围的 read cursor 定点读取。
- 在 `AgentGroupChatRowMapper` 新增 `ProjectAgentReadCursor` 行映射。
- facade 的 `readCursor` 辅助入口改为委托 repository，调用方和事务位置保持不变。
- read cursor 的 INSERT/UPSERT、单调性判断和已读业务编排仍保留在 facade。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentReadCursorRepository.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SELECT 列、列顺序及 owner/room/agent 参数顺序保持不变。
- 查询仍使用 `LIMIT 1`，无记录仍返回 nil，存在记录仍取首行。
- `messageCreatedAtUnixMs` 与 `updatedAtUnixMs` 仍按 SQLite Int64 原样映射。
- room 范围未读读取和 `markMessagesRead` 仍通过同一 facade 辅助入口读取 cursor。
- cursor 的防回退判断、更新时间计算和 UPSERT 事务边界不变。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。

## 验证结果

- `git diff --check`：通过。
- 定向契约与 Store 测试：43 个测试，0 失败。
- 可重复 Store 基准：1 个测试通过，三次 statement 计数与冻结基线完全一致：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat poll：`500 / 500 / 500`，数据库写入均为 0
  - idle account drain：`103 / 103 / 103`，数据库写入均为 0
- `swift test --package-path clients/macos`：退出码 0，全部测试通过。
- 全量测试结束后精确清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- Message mentions 与 attachments 的逐消息读取仍由 facade 持有；附件上传队列和重试时间 SELECT 也尚未拆分。
- 消息行组装仍会为每条消息读取 mentions 和 attachments；没有可复现基准前不改变此行为。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分消息 mentions 或消息 attachments 的纯读取，保留逐消息查询顺序和冻结 statement 计数。
