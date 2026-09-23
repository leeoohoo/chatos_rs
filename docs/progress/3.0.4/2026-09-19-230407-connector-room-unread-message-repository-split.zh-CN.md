# 3.0.4 进度：拆分 Room 未读 Message 读取 Repository

- 时间：2026-09-19 23:04:07（Asia/Shanghai）
- 本轮目标：保持 room 范围未读消息的成员校验、read cursor、过滤、排序和返回页语义不变，将消息 SQL 收拢到 `AgentMessageRepository`。
- 起始提交：`d61c6a220`
- 代码提交：`9a9f94656`

## 实际改动

- 为 `AgentMessageRepository` 增加 room 范围未读消息读取入口，并复用已有消息游标值对象。
- 将 read cursor 后的 SQL 条件、系统内部消息过滤、排除 Agent 自身回复、正序查询和 `limit + 1` 读取迁出 facade。
- facade 继续负责 owner/room/agent、limit、active room 与 active member 校验，并继续读取持久化 read cursor。
- facade 继续负责截取当前页、计算 next cursor、`hasMore` 和 `readThroughMessageID`。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 无 read cursor 时仍从 room 的首条可见且非自身消息开始读取。
- 有 read cursor 时仍使用 `created_at_unix_ms > ? OR (created_at_unix_ms = ? AND id > ?)`，时间参数仍重复绑定两次。
- 仍过滤 causation 为 `heartbeat`、`todo`、`todo_status` 的 system 消息。
- 仍排除 `sender_kind = 'agent' AND sender_id = 当前 Agent` 的自身持久化回复。
- SQL 仍按 `created_at_unix_ms, id` 正序查询并读取 `limit + 1` 条；limit 仍限制在 1 到 100。
- room 或成员非 active 时仍返回 `notMember`，返回页仍以最后一条 ID 作为 next cursor。
- `readThroughMessageID` 仍来自查询前读取的持久化 cursor；消息行组装及 prepared-statement 计数路径不变。

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

- Message 全局未读 SELECT 及 read cursor 本身的读取仍由 facade 持有。
- 消息行组装仍会为每条消息读取 mentions 和 attachments；没有可复现基准前不改变此行为。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 审计并以单个有界工作单元迁移 Message 全局未读读取，精确保留 membership/read cursor JOIN、排除自身回复、跨 room 排序和分页语义。
