# 3.0.4 进度：拆分 Todo Progress Sequence 读取 Repository

- 时间：2026-09-20 07:45:37（Asia/Shanghai）
- 本轮目标：保持 progress 追加事务、owner/Todo 隔离和 sequence 分配语义不变，将下一事件序号读取迁入 `AgentTodoRepository`。
- 起始提交：`0f63a16f0`
- 代码提交：`02d107ff9`

## 实际改动

- 为 `AgentTodoRepository` 增加按 owner 与 Todo 读取下一 progress sequence 的入口。
- `appendAgentTodoProgress(...)` 在原事务中的同一位置委托 repository，再继续构造并插入 progress 事件。
- ID、文本、时间校验，Todo 归属校验和事件 INSERT 均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SQL 仍为当前 owner 与 Todo 范围内 `MAX(sequence)` 加一。
- 空事件列表仍通过 `COALESCE(MAX(sequence), 0) + 1` 得到 1，聚合查询异常无行时仍回退 1。
- 参数顺序保持 owner、Todo。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 查询仍执行在原 progress 追加事务内，位于 Todo 归属校验之后、事件构造与 INSERT 之前。
- progress ID、kind、run ID、stage、detail 与时间戳写入语义均未改变。

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

- facade 仍有 heartbeat/Todo 调度候选、递归循环检测和依赖状态聚合等事务内读取。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分 Todo dependency 未完成计数读取，保持 ready Delivery 创建事务与判断顺序不变。
