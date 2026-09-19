# 3.0.4 进度：拆分 Communication Metric Snapshot Repository

- 时间：2026-09-20 06:36:34（Asia/Shanghai）
- 本轮目标：保持通信指标的 owner 隔离、字段映射与排序不变，将 `agentCommunicationMetricSnapshot(...)` 的纯读取迁入独立 repository。
- 起始提交：`18d5a4c03`
- 代码提交：`dbc9d48f1`

## 实际改动

- 新增 `AgentCommunicationMetricRepository`，承载指标快照 SELECT 与 `AgentCommunicationMetricRow` 行映射。
- facade 在完成 owner 校验后改为委托 repository 返回快照。
- 指标记录、聚合更新和校验逻辑仍留在 facade，本轮未改动写入。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentCommunicationMetricRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍仅限定当前 owner，参数顺序不变。
- SELECT 的六个列及列顺序保持不变。
- 仍按 metric name、dimension 升序返回快照。
- count、total value、maximum value 与 updated time 仍使用原 Int64 映射。
- owner ID 校验仍在查询前由 facade 执行。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 指标名称、dimension、value 校验及 UPSERT 聚合语义均未改变。

## 验证结果

- `git diff --check`：通过。
- 通信指标、契约与 Store 定向测试：46 个测试，0 失败。
- 可重复 Store 基准：1 个测试通过，三次 statement 计数与冻结基线完全一致：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat poll：`500 / 500 / 500`，数据库写入均为 0
  - idle account drain：`103 / 103 / 103`，数据库写入均为 0
- `swift test --package-path clients/macos`：退出码 0，全部测试通过。
- 全量测试结束后精确清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- facade 剩余 SELECT 均位于 heartbeat/Todo/消息写入或调度事务附近，需要继续按事务边界审计，不能机械移动。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 审计剩余事务内 SELECT，选择一个可在同一数据库事务内委托 repository、且不改变写入顺序的有界拆分单元。
