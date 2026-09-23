# 3.0.4 进度：拆分 Pending Dependent Todo 读取 Repository

- 时间：2026-09-20 06:01:28（Asia/Shanghai）
- 本轮目标：保持依赖 Todo 的 owner 隔离、pending 筛选与调度顺序不变，将 `enqueueReadyDependentAgentTodos(...)` 的候选读取迁入 `AgentTodoRepository`。
- 起始提交：`80256844f`
- 代码提交：`047f7bd1e`

## 实际改动

- 为 `AgentTodoRepository` 增加按 owner 与 prerequisite Todo 读取 pending dependent 候选的入口。
- facade 保留 ID、时间校验及逐个调用 `enqueueAgentTodoReady(...)` 的调度编排，仅委托候选 SELECT。
- readiness 二次校验、Delivery 创建和 Todo 事件写入均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍限定当前 owner 与 prerequisite Todo，参数顺序保持 owner、prerequisite Todo。
- 仍按 owner 与 dependent Todo ID JOIN `local_agent_todos`，只返回 pending Todo。
- SELECT 仍返回 Agent ID 与 Todo ID 两列。
- 调度顺序仍为 priority 降序、sort order 升序、Todo ID 升序。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- facade 仍按候选顺序逐个执行 readiness/依赖状态复核；创建 Delivery、事件收件人和消息的事务边界均未改变。

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
- 全量测试结束后精确清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- facade 中仍有 heartbeat/todo 写事务内的候选读取、递归循环检测、序号计算和依赖状态聚合；拆分时必须保持事务边界。
- communication metric snapshot 的独立纯 SELECT 仍在 facade。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分 communication metric snapshot 的纯读取与行映射。
