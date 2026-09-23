# 3.0.4 进度：拆分 Agent Heartbeat Due 读取 Repository

- 时间：2026-09-20 03:44:01（Asia/Shanghai）
- 本轮目标：保持 heartbeat 到期筛选、排序和空值语义不变，将 `nextAgentHeartbeatDue(...)` 的纯读取迁入 `AgentProfileRepository`。
- 起始提交：`66161eb04`
- 代码提交：`40a19ccfe`

## 实际改动

- 为 `AgentProfileRepository` 增加按 owner 读取下一 heartbeat 到期时间的入口。
- facade 在完成 owner 校验后改为委托 repository，不再直接持有该 SELECT。
- heartbeat 入队事务、Agent 状态更新、Delivery 创建与调度行为均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentProfileRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍只匹配当前 owner、active 状态且启用 heartbeat 的 Agent。
- 仍排除 `next_heartbeat_at_unix_ms` 为 NULL 的记录，并按该字段升序取第一条。
- 参数顺序与 `LIMIT 1` 保持不变；无匹配记录仍返回 nil。
- owner ID 校验仍在查询前由 facade 执行。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 到期 Agent 入队、下一次调度时间推进与相关事务边界均未改变。

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

- facade 中仍有 Todo 调度、来源、依赖、进度等读取，其中部分嵌在写事务中；需要继续按事务边界逐项审计，不能机械迁移。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续审计 facade 剩余 SELECT，优先选择不改变事务边界的单一纯读取拆分工作单元。
