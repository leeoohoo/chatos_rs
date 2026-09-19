# 3.0.4 进度：拆分 Message 基础列表读取 Repository

- 时间：2026-09-19 21:22:11（Asia/Shanghai）
- 本轮目标：保持基础消息列表的校验、过滤、排序和限制不变，将其 SQL 读取收拢到 `AgentMessageRepository`。
- 起始提交：`4dcf92e5d`
- 代码提交：`d43baa322`

## 实际改动

- 为 `AgentMessageRepository` 增加按 owner/room 读取基础消息列表的入口。
- 将可选 `afterUnixMs` 条件、SQL 参数组装、系统内部消息过滤和时间正序查询迁出 facade。
- facade 继续负责 owner、room、limit、`afterUnixMs` 合法性校验和房间存在性检查。
- repository 继续复用原有消息行组装闭包，未改变 mentions 与 attachments 的读取路径。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- owner/room 参数仍位于 SQL 参数首位，可选时间参数仍仅在存在时追加，limit 仍最后绑定。
- 仍过滤 causation 为 `heartbeat`、`todo`、`todo_status` 的 system 消息。
- 仍按 `created_at_unix_ms, id` 正序排序并使用相同 limit。
- `afterUnixMs` 仍为严格大于条件，负数仍在查询前拒绝。
- limit 仍限制在 1 到 500，目标房间不存在仍返回 `notFound`。
- 每次列表查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 消息分页、未读、写入、路由和附件持久化均未改变。

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

- Message 前后向分页、room 未读和全局未读 SELECT 仍由 facade 持有。
- 消息行组装仍会为每条消息读取 mentions 和 attachments；没有可复现基准前不改变此行为。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元迁移 Message 的前向或最近消息分页读取，精确保留游标、排序、`limit + 1` 和返回页语义。
