# 3.0.4 进度：拆分未完成 Todo Dependency 计数读取

- 时间：2026-09-20 08:20:43（Asia/Shanghai）
- 本轮目标：保持 ready Delivery 创建事务、依赖 JOIN 与状态判断不变，将未完成依赖计数迁入 `AgentTodoRepository`。
- 起始提交：`9a8b53377`
- 代码提交：`3b762d073`

## 实际改动

- 为 `AgentTodoRepository` 增加按 owner 与 Todo 统计未完成 prerequisite 的入口。
- `enqueueAgentTodoReady(...)` 在原事务中的同一位置委托 repository，并继续以计数为零作为创建 ready Delivery 的门禁。
- Todo 二次读取、幂等 Delivery 检查、消息与事件收件人写入均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍限定当前 owner 与 Todo，参数顺序保持 owner、Todo。
- 仍按 owner 与 prerequisite Todo ID JOIN，并统计状态不为 `completed` 的 prerequisite。
- 仍使用 `COUNT(*)` Int64 结果，只有计数为 0 才继续创建 ready Delivery。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 查询仍执行在原事务内，位于 pending Todo 二次确认之后、幂等 Delivery 检查及任何写入之前。
- Delivery 去重键、消息内容、事件键和事务回滚语义均未改变。

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

- facade 仍有 heartbeat/Todo 调度候选、递归循环检测以及消息路由事务内读取。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分 Todo dependency 递归循环检测读取，保持依赖替换事务、DELETE/INSERT 顺序与错误语义不变。
