# 3.0.4 进度：拆分 Delivery claim 候选读取

- 时间：2026-09-19 17:54:53（Asia/Shanghai）
- 本轮目标：在不改变 claim 原子性、队列优先级和并发门禁的前提下，将下一条 pending Delivery 的候选选择收拢到 repository。
- 起始提交：`a071d5d65`
- 代码提交：`1b7922447`

## 实际改动

- 为 `AgentDeliveryRepository` 增加下一条 pending Delivery ID 的候选查询。
- 将 `claimNextDelivery` 事务内的候选 SELECT 接入 repository。
- facade 继续持有参数校验、`BEGIN IMMEDIATE` 事务、条件 UPDATE、受影响行数检查和更新后实体读取。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentDeliveryRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 候选查询的 Delivery、Room、Member 三表 JOIN、owner/agent/status 条件和 SQL 参数顺序不变。
- 活跃房间、活跃成员以及同一 Agent 同类执行槽互斥规则不变；Todo 与非 Todo 仍分别串行。
- 可选 room 过滤仍只在传值时追加。
- 候选排序仍为 `created_at_unix_ms, id`，仍使用 `LIMIT 1` 和首行语义。
- 候选读取仍位于同一个 `BEGIN IMMEDIATE` 事务内，claim UPDATE 的条件、字段、冲突检查和后续读取不变。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。

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

- Delivery 的 heartbeat、Todo 和通知调度计数查询仍位于 facade。
- claim UPDATE 与事务编排有明确原子性要求，暂不下沉到纯读取 repository。
- Run 聚合根的读取仍位于 facade。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 盘点 Delivery 的 outstanding/active 计数查询，按语义分组后选择一个不触碰写事务的有界读取单元继续拆分。
