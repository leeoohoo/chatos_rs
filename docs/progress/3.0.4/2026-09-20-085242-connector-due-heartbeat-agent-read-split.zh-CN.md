# 3.0.4 进度：到期 heartbeat Agent 读取拆分

## 本轮目标

将 `enqueueDueAgentHeartbeats(...)` 中的到期 Agent 候选读取迁移到 `AgentProfileRepository`，继续收窄 SQLite facade 的读取职责。

## 起始提交

- `e4a72899a`

## 实际改动

- 新增 repository 内部值类型 `DueHeartbeatAgent`。
- 新增 `AgentProfileRepository.dueHeartbeatAgents(...)`。
- 原样迁移候选查询、行映射、到期条件、排序和数量限制。
- facade 继续在原事务内消费候选并创建 heartbeat 消息、Delivery 与下一次到期时间。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentProfileRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 仅选择 active、启用 heartbeat 且已经到期的 Agent。
- 排序仍为 `next_heartbeat_at_unix_ms, id`，`agentLimit` 语义不变。
- 每个 Agent 的 outstanding Delivery 去重、Human direct room 选择、消息文本和 heartbeat 更新逻辑不变。
- 整体仍在原 `BEGIN IMMEDIATE` 事务内运行。
- 未修改 SQL 参数顺序、Schema、索引、公开协议或错误文本。

## 验证结果

- 相关固定门禁：43 个测试通过，0 失败。
- Store 性能基准：通过，冻结 prepared statement 计数保持：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat：`500 / 500 / 500`，0 写入
  - idle account drain：`103 / 103 / 103`，0 写入
- `swift test --package-path clients/macos`：全量通过；数据量基准按预期在未设置环境变量时跳过。
- 全量测试产生的两个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `2a021219aea6a3802f71f0048948730d00eeef75` `refactor(connector): extract due heartbeat agent read`

## 剩余风险

- 本轮只迁移读取边界，没有改变 heartbeat 行为或优化查询。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 迁移 pending Todo Agent ID 候选读取与 ready Todo 候选读取，继续保持原事务、查询和调度语义。
