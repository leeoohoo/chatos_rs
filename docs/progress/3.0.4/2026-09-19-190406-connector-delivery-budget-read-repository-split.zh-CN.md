# 3.0.4 进度：完成 Delivery 纯读取边界

- 时间：2026-09-19 19:04:06（Asia/Shanghai）
- 本轮目标：保持路由预算与房间批量停止行为不变，将剩余的 root message 计数和房间 active Delivery ID 读取收拢到 repository。
- 起始提交：`d0cdb8369`
- 代码提交：`c13e41d78`

## 实际改动

- 为 `AgentDeliveryRepository` 增加按 root message 统计 Delivery 数量的查询。
- 为 `AgentDeliveryRepository` 增加按房间读取 pending/running Delivery ID 的有序查询。
- 将消息路由预算检查与 `stopOutstandingDeliveries` 的目标 ID 读取接入 repository。
- facade 中已不再保留 `project_agent_deliveries` 的 SELECT；只保留 INSERT、UPDATE、事务和业务编排，Delivery 基础纯读取边界至此完成。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentDeliveryRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- root message 计数仍限定 owner 与 root message，SQL 参数顺序和零值语义不变。
- 最大 hop 与每个 root message 最大 Agent run 数的判定顺序、阈值比较和停止原因不变。
- 房间 active ID 查询仍限定 `pending`/`running`，排序仍为 `created_at_unix_ms, id`。
- `stopOutstandingDeliveries` 仍在同一个 `BEGIN IMMEDIATE` 事务内先锁定 ID 顺序，再关闭 Run，最后批量更新 Delivery 并检查受影响行数。
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

- Delivery 写事务、claim UPDATE、完成/失败/重试/取消状态迁移继续由 facade 持有；本轮未移动这些业务行为。
- Run 聚合根的读取仍位于 facade。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 盘点 `local_agent_group_chat_runs` 的按 delivery、按 ID 和列表读取路径，选择一个有界查询组建立 Run repository，并保持 JSON 解码、排序和事务边界不变。
