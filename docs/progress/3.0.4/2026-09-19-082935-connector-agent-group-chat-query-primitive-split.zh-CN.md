# 3.0.4 进度：拆分 Agent Group Chat SQLite 查询原语

- 时间：2026-09-19 08:29:35（Asia/Shanghai）
- 本轮目标：保持 statement 生命周期、绑定顺序、错误文本和测试计数不变，将 SQLite query/bind/step 原语移入数据库内核。
- 起始提交：`03d92b0bf`
- 代码提交：`16ef627d8`

## 实际改动

- 将 SQL 参数 `Value`、prepare、bind、step、finalize 与 SQLite 错误转换移动到 `AgentGroupChatDatabase`。
- `SQLiteAgentGroupChatStore` 保留一个薄 `query` 转发层，以维持现有所有调用点和 DEBUG prepared-statement 计数。
- `execute`、`scalarInt64` 与 `transaction` 仍通过该转发层工作，未改变调用拓扑或事务实现。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatDatabase.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 每次查询仍执行一次 `sqlite3_prepare_v2`，并在作用域结束时 `sqlite3_finalize`。
- text/integer/null 参数的绑定顺序、索引和 destructor 语义不变。
- row callback、`SQLITE_ROW` / `SQLITE_DONE` 处理和错误文本不变。
- prepared-statement 计数仍由 facade 在每次 query 前递增，BEGIN/COMMIT/ROLLBACK 的计数路径不变。
- SQL、排序、索引、事务边界和 facade 公共 API 不变。

## 验证结果

- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败。
- 可重复 Store 基准：1 个测试通过，三次 statement 计数与冻结基线完全一致：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat poll：`500 / 500 / 500`，数据库写入均为 0
  - idle account drain：`103 / 103 / 103`，数据库写入均为 0
- `swift test --package-path clients/macos`：退出码 0，未出现失败输出。
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- `execute`、`scalarInt64` 和 `transaction` 的薄编排仍位于 facade；后续可在保持计数回调与事务边界的前提下继续收拢。
- handle 仍由 facade 持有，尚未完全封装到数据库对象。
- 测试资源泄漏问题仍待独立修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 抽出 execute/scalar/transaction 内核并显式保留 prepared-statement 计数回调；随后再按 Profile/Conversation/Proposal/Todo/Asset/Run 聚合根拆 repository 与 row mapper。
