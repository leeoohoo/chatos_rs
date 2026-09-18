# 3.0.4 进度：拆分 Agent Group Chat Migrations

- 时间：2026-09-19 07:20:31（Asia/Shanghai）
- 本轮目标：保持历史数据库升级路径、migration SQL、版本 marker 和错误语义不变，将迁移逻辑从 SQLite actor facade 纯拆分到独立文件。
- 起始提交：`f17065bed`
- 代码提交：`4f41f57ac`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatMigrations.swift`。
- 将 `migrateConversationSchema` 及其内部列检测、版本检测和 SQL 执行辅助逻辑移入 `AgentGroupChatMigrations`。
- `SQLiteAgentGroupChatStore` 初始化改为调用独立迁移器；调用时机仍位于初始 Schema 成功执行之后、数据库 handle 发布之前。
- `SQLiteAgentGroupChatStore.swift` 从 6,805 行降至 5,999 行；独立迁移文件为 811 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatMigrations.swift`

## 业务不变量

- 迁移函数主体逐字不变。
- Schema 版本检测、缺列修复、migration marker 写入顺序和 SQL 内容不变。
- 迁移事务与失败传播语义不变；打开失败时仍关闭 SQLite handle。
- 版本 17、21、22 等已有修复路径及版本 24 通信指标表创建行为不变。
- facade 的公共 API、actor 隔离和数据库 handle 生命周期不变。

## 验证结果

- 迁移函数主体移动前后逐字比对：通过（`BYTE_FOR_BYTE_MIGRATION_BODY_MATCH`）。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败，其中覆盖历史迁移 17、21、22 的 fixture 用例全部通过。
- `swift test --package-path clients/macos`：退出码 0，未出现失败输出。
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程，避免污染后续验证。

## 剩余风险

- SQLite connection、query、bind、transaction 和错误封装仍与 facade 混合，下一步需要最小数据库内核；拆分时不得扩大可见性或改变 statement 生命周期。
- `AgentGroupChatMigrations.swift` 为单一完整迁移状态机，当前 811 行处于方案允许的 900 行上限内，不再混入 repository 逻辑。
- 测试资源泄漏问题仍待独立修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 抽出最小 `AgentGroupChatDatabase` connection/query/transaction 内核；保持 prepare/finalize、bind 顺序、busy timeout、错误文本和事务边界不变，再逐步让 facade 委托数据库原语。
