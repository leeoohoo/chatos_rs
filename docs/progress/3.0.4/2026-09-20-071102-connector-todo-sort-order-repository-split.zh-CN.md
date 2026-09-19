# 3.0.4 进度：拆分 Todo Sort Order 读取 Repository

- 时间：2026-09-20 07:11:02（Asia/Shanghai）
- 本轮目标：保持 Todo 创建事务、owner/Agent 隔离与 sort order 分配语义不变，将下一排序值读取迁入 `AgentTodoRepository`。
- 起始提交：`122043e5d`
- 代码提交：`e56b23119`

## 实际改动

- 为 `AgentTodoRepository` 增加按 owner 与 Agent 读取下一 Todo sort order 的入口。
- Todo 创建流程在原事务中的同一位置委托 repository，再继续构造、校验并插入 Todo。
- Todo 创建前的权限、成员、来源和团队边界校验以及后续来源写入均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SQL 仍为当前 owner 与 Agent 范围内 `MAX(sort_order)` 加一。
- 空列表仍通过 `COALESCE(MAX(sort_order), -1) + 1` 得到 0，聚合查询异常无行时仍回退 0。
- 参数顺序保持 owner、Agent。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 查询仍执行在原 `createAgentTodo(...)` 事务内部，位于权限/归属校验之后、Todo 构造和 INSERT 之前。
- Todo ID、状态、时间戳、execution plan/contract 与来源链接写入语义均未改变。

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

- facade 仍有 heartbeat/Todo 调度候选、递归循环检测、progress 序号和依赖状态等事务内读取。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分 Todo progress 事件下一 sequence 的事务内读取，保持追加事务和 INSERT 顺序不变。
