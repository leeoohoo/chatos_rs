# 3.0.4 进度：拆分 Profile 与 Conversation 行映射

- 时间：2026-09-19 09:42:49（Asia/Shanghai）
- 本轮目标：保持列顺序、解码、校验和错误文本不变，从 SQLite Store facade 抽出 Profile、Room 与 Member 行映射。
- 起始提交：`cf500d426`
- 代码提交：`d473b2e87`

## 实际改动

- 新增 `AgentGroupChatRowMapper`，集中承载 Agent Profile、Conversation Room 与 Room Member 的 SQLite 行解码。
- 将字符串数组 JSON 解码和这些模型所需的 SQLite 可空值读取封装在 mapper 内部。
- facade 查询直接引用 mapper，不再保留对应的行映射实现或薄转发方法。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Agent、Room 与 Member 的 SELECT 列、列索引和构造参数顺序不变。
- persisted enum 的 raw value 解析、可空字段处理和字符串数组 JSON 错误文本不变。
- 解码后仍调用各模型原有 `validate()`，校验时点和错误传播不变。
- SQL、参数绑定、排序、索引、事务边界、prepared-statement 数和公共 Store API 不变。

## 验证结果

- `git diff --check`：通过。
- 定向契约与 Store 测试：43 个测试，0 失败；最终 mapper 直连形态另跑 Store 30 个测试，0 失败。
- 可重复 Store 基准：1 个测试通过，三次 statement 计数与冻结基线完全一致：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat poll：`500 / 500 / 500`，数据库写入均为 0
  - idle account drain：`103 / 103 / 103`，数据库写入均为 0
- 最终代码执行 `swift test --package-path clients/macos`：退出码 0，全部测试通过。
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- Message 依赖附件与 mention 查询，尚不能作为纯行映射直接搬移。
- Todo、Asset、Proposal、Delivery 等纯映射仍位于 facade，后续需分批迁移。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 将 Todo 与 Team Asset 的纯行映射迁入同一 mapper；保留涉及嵌套查询的 Message 映射在 facade，直至 repository 边界可显式注入查询能力。
