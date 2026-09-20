# 3.0.4 进度：Todo 依赖循环检测读取拆分

## 本轮目标

将 `replaceAgentTodoDependencies(...)` 中仍直接留在 SQLite facade 的递归依赖循环检测读取迁移到 `AgentTodoRepository`，继续收窄阶段 1 的持久化职责边界。

## 起始提交

- `a14db84fd5cf`（`origin/3.0.4` 同步点）

## 实际改动

- 新增 `AgentTodoRepository.dependencyCreatesCycle(...)`。
- 原样迁移递归 CTE、绑定参数顺序和 `COUNT(*) > 0` 判断。
- facade 在原依赖替换事务内调用 repository，保留先删除旧依赖、逐项校验/检测/插入的顺序。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Todo、前置 Todo 与团队范围校验不变。
- 递归祖先查询 SQL、参数顺序和判断语义不变。
- 循环依赖仍返回 `invalidField("todoDependencyCycle")`。
- 查询仍运行在同一依赖替换事务内，失败时整体回滚。
- 未修改 Schema、迁移、索引、排序、公开协议或 Codable 字段。

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

- `12255dc553a1c2a2a269b965a54f4f2827f79a95` `refactor(connector): extract todo dependency cycle read`

## 剩余风险

- 这是纯职责迁移，没有改变业务行为或性能；阶段 1 facade 中仍有少量候选读取需要继续迁移。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 继续迁移 `enqueueDueAgentHeartbeats(...)` 的 due Agent 候选读取，保持查询、顺序、限制和事务边界不变。
