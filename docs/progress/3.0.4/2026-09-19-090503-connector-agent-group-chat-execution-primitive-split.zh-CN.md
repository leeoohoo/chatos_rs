# 3.0.4 进度：拆分 Agent Group Chat SQLite 执行原语

- 时间：2026-09-19 09:05:03（Asia/Shanghai）
- 本轮目标：保持 SQL、错误语义、事务边界与调试计数不变，将 SQLite execute、scalar 与 transaction 原语移入数据库内核。
- 起始提交：`96450da8c`
- 代码提交：`a561451a6`

## 实际改动

- 在 `AgentGroupChatDatabase` 中新增 `execute`、`scalarInt64` 与 `transaction` 内核实现。
- `SQLiteAgentGroupChatStore` 保留薄转发方法，现有业务调用点无需改动。
- 通过显式 `recordPreparedStatement` 回调保留 DEBUG prepared-statement 计数，包括 BEGIN、COMMIT 与 ROLLBACK。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatDatabase.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SQL 文本、参数绑定顺序、查询返回值和 SQLite 错误转换不变。
- 事务仍使用 `BEGIN IMMEDIATE`，成功时 COMMIT，失败时尽力 ROLLBACK 并重新抛出原始错误。
- execute 与 scalar 每次仍只计入一个 prepared statement；事务控制语句计数路径不变。
- 排序、索引、持久化格式、actor 隔离和 facade 公共 API 不变。

## 验证结果

- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败。
- 可重复 Store 基准：1 个测试通过，三次 statement 计数与冻结基线完全一致：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat poll：`500 / 500 / 500`，数据库写入均为 0
  - idle account drain：`103 / 103 / 103`，数据库写入均为 0
- `swift test --package-path clients/macos`：退出码 0，全部测试通过。
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- 数据库 handle 仍由 facade 持有，数据库内核尚未封装为独立生命周期对象。
- 领域 SQL、repository 编排与 row mapper 仍集中在大型 facade 中，需继续按聚合根拆分。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 选择一个低耦合聚合根，将对应 SQL、repository 编排与 row mapper 作为独立可回滚单元从 facade 拆出，并继续以冻结契约测试和 statement 基线锁定行为。
