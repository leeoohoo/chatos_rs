# 3.0.4 进度：活跃成员 ID 读取拆分

## 本轮目标

迁移 SQLite facade 中最后两段直接 `SELECT`：成员移除后的默认 Agent 替换读取，以及 direct room 的活跃成员路由读取。

## 起始提交

- `aeb4ca695`

## 实际改动

- 新增 `AgentConversationRepository.firstActiveMemberID(...)`。
- 新增 `AgentConversationRepository.activeMemberIDs(...)`。
- removal proposal 审批与 direct room 消息路由改用 repository 读取。
- facade 不再直接执行返回业务数据的 `SELECT`；仅保留写入用的 `INSERT ... SELECT` 和数据库原语转发。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentConversationRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 默认 Agent 替换仍选择最早加入、同时间按 Agent ID 排序的首个 active 成员。
- direct room 路由仍按加入时间和 Agent ID 返回全部 active 成员。
- Agent 发送时仍过滤发送者自身；团队房间的 mention/default Agent 路由不变。
- 两个读取仍位于原消息或提案事务内。
- SQL、参数顺序、Schema、错误文本、公开协议和 Codable 字段不变。

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

- `53eeb1c4b62eb2d957afbbc7ac6b03814fe3ec2d` `refactor(connector): extract active member ID reads`

## 剩余风险

- 本轮为纯读取职责迁移，没有改变路由或成员选择行为。
- 阶段 1 是否退出仍需进行文件边界、直接查询和完整门禁审计。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 执行阶段 1 退出审计；确认 facade 不再承担业务读取、Core/Schema/Migration/repository 边界完整且无循环依赖，再进入阶段 2。
