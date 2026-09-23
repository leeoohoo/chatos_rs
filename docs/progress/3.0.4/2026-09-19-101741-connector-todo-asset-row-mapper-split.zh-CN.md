# 3.0.4 进度：拆分 Todo 与 Team Asset 行映射

- 时间：2026-09-19 10:17:41（Asia/Shanghai）
- 本轮目标：保持列索引、JSON 解码、校验和错误文本不变，将 Todo、Team Asset 与 Todo Asset Snapshot 的纯 SQLite 行映射移出 Store facade。
- 起始提交：`e826acd9c`
- 代码提交：`b79641c09`

## 实际改动

- 将 `LocalAgentTodo` 行解码迁入 `AgentGroupChatRowMapper`，包括 execution plan、execution contract 与 team binding 校验。
- 将 `LocalAgentTeamAsset` 和 `LocalAgentTodoTeamAssetSnapshot` 行解码迁入同一 mapper。
- 所有对应查询直接引用 mapper，facade 不再持有这些纯映射实现。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Todo、Asset 与 Snapshot 的 SELECT 列、列索引、可空字段和构造参数顺序不变。
- execution plan 与 contract 的 JSON 解码、contract normalization 和错误文本不变。
- Todo team binding、persisted enum 解析和模型 `validate()` 调用时点不变。
- SQL、参数绑定、排序、索引、事务边界、prepared-statement 数和 Store 公共 API 不变。

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

- Proposal 与 Delivery 的纯映射仍位于 facade，适合后续独立迁移。
- Message 映射依赖附件和 mention 子查询，不能按当前纯 mapper 边界直接搬移。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 分批迁移 Proposal 与 Delivery 的纯行映射；保持 draft JSON 解码、兼容默认值和错误文本逐字一致。
