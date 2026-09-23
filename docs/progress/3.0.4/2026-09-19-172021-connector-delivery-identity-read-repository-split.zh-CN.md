# 3.0.4 进度：拆分 Delivery 标识读取 Repository

- 时间：2026-09-19 17:20:21（Asia/Shanghai）
- 本轮目标：保持 Delivery 的按 ID、去重键和 ID 集合读取语义及 DEBUG statement 计数不变，建立 Delivery 聚合根的首个纯读取边界。
- 起始提交：`4d2cf6cf2`
- 代码提交：`995e18520`

## 实际改动

- 新增 `AgentDeliveryRepository`，统一承载按 delivery ID、按 deduplication key 以及按 delivery ID 集合读取 Delivery 的 SQL。
- 将 facade 内部 `readDelivery`、两个 Todo 通知幂等读取路径和公开批量 `deliveries` 路径接入 repository。
- 将 Delivery SELECT 列清单收拢到 repository。
- facade 继续负责参数校验、ID 去重与排序、500 条上限、字典映射、事务、调度和写入编排。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentDeliveryRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Delivery 的 SELECT 列、表名、owner/delivery/deduplication 条件和 SQL 参数顺序不变。
- 单条 ID 读取继续保持原有无 `LIMIT` 查询加 `.first` 语义；去重键读取继续保持 `LIMIT 1` 和 `.first`。
- 批量读取仍先由 facade 去重和排序，空集合不访问数据库，最大数量仍为 500，IN 占位符和绑定顺序不变。
- Todo ready/status 通知的事件键、去重键、幂等分支、事务边界和写入顺序不变。
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
- 全量测试结束后精确清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- Delivery 队列计数、候选选择和 claim 相关读取仍与事务编排位于 facade，需要先区分纯查询与原子更新边界。
- Run 聚合根的读取仍位于 facade。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 盘点 Delivery 队列计数与候选选择路径，选择不改变 claim 原子性、排序和锁语义的纯读取单元继续拆分。
