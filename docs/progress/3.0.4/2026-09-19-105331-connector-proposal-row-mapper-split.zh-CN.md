# 3.0.4 进度：拆分 Proposal 行映射

- 时间：2026-09-19 10:53:31（Asia/Shanghai）
- 本轮目标：保持 Proposal draft JSON、状态解析、列索引、校验和错误文本不变，将五类 Proposal 的纯 SQLite 行映射移出 Store facade。
- 起始提交：`bbb4bbc9e`
- 代码提交：`5f6ab1fae`

## 实际改动

- 将 Agent 创建、Project 创建、Agent 移除、Team 创建与 Membership Proposal 的行解码迁入 `AgentGroupChatRowMapper`。
- 所有列表查询、按 ID 查询和幂等 request-key 查询直接引用 mapper。
- facade 不再持有五类 Proposal 的 draft JSON 解码、状态解析与模型构造实现。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 五类 Proposal 的 SELECT 列、列索引、可空结果字段和构造参数顺序不变。
- 每类 draft 的 JSON 类型、失败错误文本和 persisted status raw value 解析不变。
- 解码后仍调用对应模型的 `validate()`，校验时点与错误传播不变。
- SQL、过滤条件、排序、参数绑定、事务边界、prepared-statement 数和 Store 公共 API 不变。

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

- Delivery 的纯行映射仍位于 facade，适合下一轮独立迁移。
- Message 映射依赖附件和 mention 子查询，需等待 repository 查询边界明确后再拆。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 迁移 Delivery 行映射，并评估 Run JSON 解码是否可与其一起形成独立且无查询依赖的映射单元。
