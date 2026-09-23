# 3.0.4 进度：Local Agent Relay Server 拆分

## 本轮目标

完成阶段 1 退出审计，并开始阶段 2：从 `LocalAgentChatToolProvider.swift` 中独立 Relay MCP 入口、Run Context 与安全插件选项模型。

## 起始提交

- `3c03a9d72`

## 阶段 1 退出审计

- Core 的 Profile、Proposal、Conversation、Todo、Asset、Run/Delivery、Store Contract 与 Validation 已按职责独立。
- Connector 的 Database、Schema、Migration、RowMapper 和各聚合 repository 已独立。
- SQLite facade 中已无直接返回业务数据的 `SELECT`；唯一剩余 `SELECT` 是 Todo 开始时写入资产快照使用的 `INSERT ... SELECT`。
- 原数据库迁移、契约测试、Store 测试、冻结查询基准和全量测试持续通过。
- 因此阶段 1 达到“调用方不感知、持久化兼容、读取职责收窄、无循环依赖”的退出条件。

## 实际改动

- 新增 `LocalAgentRelayMCPServer.swift`。
- 将 `LocalAgentTodoPluginOption`、`LocalAgentChatRunContext` 和 `LocalAgentRelayMCPServer` 原样迁入新文件。
- `LocalAgentChatToolProvider.swift` 保留 Reference Vault、Provider 路由、领域执行和响应模型，后续继续拆分。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentRelayMCPServer.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`

## 业务不变量

- Run Context 字段、Codable key、默认 manager lane 与 hop count 校验不变。
- Relay Server 的 Store 获取、文档草稿目录创建、room change 发布和依赖注入不变。
- 插件选项仍只把真实 plugin ID 保留在 Provider 内部。
- 工具名称、Schema、错误码、响应 JSON、权限和调度行为不变。

## 验证结果

- `LocalAgentChatToolProviderTests`：5 个通过。
- `LocalAgentGroupChatSchedulerTests`：12 个通过。
- `AgentGroupChatCodableContractTests`：5 个通过。
- Store 性能基准：通过，冻结 prepared statement 计数保持 `1008 / 42 / 1 / 500 / 103`，空闲场景 0 写入。
- `swift test --package-path clients/macos`：全量通过；数据量基准按预期在未设置环境变量时跳过。
- 全量测试产生的两个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `341c3137ebd5d8edd69c0efc92918538e1ab7b67` `refactor(connector): extract local agent relay server`

## 剩余风险

- Tool Provider 仍包含 Reference Vault、工具定义、领域执行与大量响应 DTO，阶段 2 尚未完成。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 独立 `LocalAgentRunReferenceVault`，保持 opaque reference、文档完整性校验、单次消费和发送幂等语义不变。
