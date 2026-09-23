# 3.0.4 进度：Todo 调度读取拆分

## 本轮目标

将 Todo 调度路径中重复的候选与运行状态读取统一迁移到 `AgentTodoRepository`，继续完成阶段 1 的 SQLite facade 收口。

## 起始提交

- `1fa099d1f`

## 实际改动

- 新增 pending Todo Agent ID 候选读取。
- 新增当前运行 Todo、运行 Todo 数量和最高优先级 ready Todo 读取。
- `enqueuePendingAgentTodos(...)`、`agentTodoScheduleState(...)`、`startNextReadyAgentTodo(...)` 统一复用 repository 查询。
- 删除 facade 中三份重复的 ready Todo SQL。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- pending Agent 候选仍只包含 active Agent、团队 Todo，并按 Agent ID 排序和限制数量。
- ready Todo 仍要求 active room/member 且全部依赖已完成。
- ready 排序仍为优先级降序、sort order、创建时间和 ID。
- running Todo 排序、running count 以及 outstanding Delivery 门禁不变。
- 所有读取仍在各自原事务中，后续消息、Delivery、资产快照和 Todo 状态写入顺序不变。
- 未修改 Schema、索引、错误文本、公开协议或 Codable 字段。

## 验证结果

- 相关固定门禁：43 个测试通过，0 失败。
- Store 性能基准：通过，冻结 prepared statement 计数保持：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat：`500 / 500 / 500`，0 写入
  - idle account drain：`103 / 103 / 103`，0 写入
- `swift test --package-path clients/macos`：全量通过；数据量基准按预期在未设置环境变量时跳过。
- 测试期间发现的四个孤立 fixture shell 进程均已确认 PPID 为 1，并按精确 PID 清理。

## 代码提交

- `c3f916ef9db8236ab5e4d5ba234a46f9d6a029af` `refactor(connector): extract todo scheduler reads`

## 剩余风险

- 本轮只统一读取职责，没有改变调度行为或做性能优化。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 迁移成员移除后的默认 Agent 替换读取与 direct room active member 路由读取，然后执行阶段 1 退出审计。
