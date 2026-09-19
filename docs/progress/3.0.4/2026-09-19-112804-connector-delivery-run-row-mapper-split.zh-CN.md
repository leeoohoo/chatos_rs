# 3.0.4 进度：拆分 Delivery 与 Run 行映射

- 时间：2026-09-19 11:28:04（Asia/Shanghai）
- 本轮目标：保持 Delivery 列解码和 Run JSON 校验语义不变，将 Delivery 与 Run 的纯持久化映射移出 Store facade。
- 起始提交：`a67ff2ec9`
- 代码提交：`2c2973827`

## 实际改动

- 将 `ProjectAgentDelivery` 的 SQLite 行解码迁入 `AgentGroupChatRowMapper`。
- 将 `LocalAgentGroupChatRun` 的 JSON 解码、模型校验和错误转换迁入 mapper。
- Delivery 列表、dedup 查询与按 ID 查询直接引用 mapper；Run 单条和列表查询统一复用 mapper。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Delivery 的 SELECT 列、列索引、可空字段、trigger/status raw value 解析和构造参数顺序不变。
- Run 继续用同一 Codable 模型解码并调用 `validate()`；领域校验错误原样传播，其他错误仍转换为 `invalid Agent run record`。
- SQL、过滤与排序、参数绑定、事务边界、prepared-statement 数和 Store 公共 API 不变。

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
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- Message 主记录仍与 mention、附件子查询耦合，尚未形成纯 mapper/repository 边界。
- facade 仍包含大量领域 SQL 与事务编排，下一阶段需要按聚合根拆 repository。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 先抽取无文件系统依赖的 Profile 或 Conversation repository 读路径，保持 facade、SQL 和事务调用顺序不变，再逐步覆盖写路径。
