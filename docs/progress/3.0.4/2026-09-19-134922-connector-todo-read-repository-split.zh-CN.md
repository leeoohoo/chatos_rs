# 3.0.4 进度：拆分 Todo 读取 Repository

- 时间：2026-09-19 13:49:22（Asia/Shanghai）
- 本轮目标：保持 Todo 查询、幂等创建和 DEBUG statement 计数不变，建立 Todo 聚合根的基础 SQLite repository 读取边界。
- 起始提交：`6cee1b955`
- 代码提交：`5fa93f9ba`

## 实际改动

- 新增 `AgentTodoRepository`，承载按 Agent 列表、按 Team 列表、按 Todo ID 定点读取和按 request key 幂等读取。
- 让 `listAgentTodos`、`listTeamTodos`、内部 `readTodo` 与 Todo 创建事务的幂等命中复用 repository。
- facade 继续负责参数校验、Team 房间类型校验、事务边界和 Todo 写入编排。
- 顶部 scheduler 的复杂 Todo JOIN 查询、Todo 写事务、依赖、来源和 progress 查询均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Todo SELECT 列清单、SQL 参数顺序与 `.first` 返回语义不变。
- terminal 状态过滤仍为排除 `completed`、`cancelled`。
- 列表排序仍为 `priority DESC, sort_order, created_at_unix_ms, id`。
- Team Todo 列表前置的 project-team 类型校验及错误语义不变。
- 创建事务仍先按 owner/agent/request key 查询幂等结果，事务边界和后续写入顺序不变。
- 每次 repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。

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

- scheduler 使用的复杂 Todo JOIN 读取仍位于 facade，需在单独工作单元中评估边界。
- Todo 写事务、依赖、来源、progress 与事件收件人逻辑仍由 facade 编排。
- Proposal、Delivery 与 Run 聚合根尚未建立完整 repository 读取边界。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 选择下一组纯读取路径建立 repository 边界，继续冻结 SQL、排序、事务时点与 statement 计数。
