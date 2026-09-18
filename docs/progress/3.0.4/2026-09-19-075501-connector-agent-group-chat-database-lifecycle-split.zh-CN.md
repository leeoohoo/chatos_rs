# 3.0.4 进度：拆分 Agent Group Chat 数据库生命周期

- 时间：2026-09-19 07:55:01（Asia/Shanghai）
- 本轮目标：保持 SQLite 打开参数、Schema/迁移顺序、错误文本和 handle 生命周期不变，抽出最小数据库连接生命周期内核。
- 起始提交：`c593bbc2b`
- 代码提交：`c010dad1f`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatDatabase.swift`。
- 将数据库父目录创建、`sqlite3_open_v2`、busy timeout、初始 Schema、历史迁移、失败关闭和正常关闭封装到 `AgentGroupChatDatabase`。
- `SQLiteAgentGroupChatStore` 继续持有同一个 `OpaquePointer?`，仅通过新内核完成打开与关闭；查询、写入和事务代码尚未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatDatabase.swift`

## 业务不变量

- SQLite flags 仍为 `CREATE | READWRITE | FULLMUTEX`。
- busy timeout 仍为 5 秒。
- 初始 Schema 与历史迁移的执行顺序、SQL 和错误传播不变。
- 打开失败、Schema/迁移失败时仍关闭临时 handle；Store 析构时仍关闭已发布 handle。
- 附件根目录、远端 Artifact service、actor 隔离和 facade 公共 API 不变。
- statement prepare/finalize、bind、查询和事务实现本轮未改动。

## 验证结果

- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败；数据库创建、消息、Todo、Delivery、Run 与历史迁移路径均通过。
- `swift test --package-path clients/macos`：退出码 0，未出现失败输出。
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- `AgentGroupChatDatabase` 当前只负责连接生命周期；query/bind/transaction 原语仍在 facade，需后续小步迁移。
- handle 仍以 `OpaquePointer?` 形式由 actor 持有，以避免本轮同时改变所有调用点；后续数据库原语收拢完成后再评估是否将 handle 完全封装。
- 测试资源泄漏问题仍待独立修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 将 `Value`、query/execute/scalar/transaction 与 SQLite 错误转换移动到数据库内核，通过最小 facade 转发保持现有调用点、statement 计数和事务边界不变。
