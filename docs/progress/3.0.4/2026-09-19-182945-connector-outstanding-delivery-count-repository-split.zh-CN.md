# 3.0.4 进度：拆分 outstanding Delivery 计数读取

- 时间：2026-09-19 18:29:45（Asia/Shanghai）
- 本轮目标：保持 heartbeat 与 Todo 调度的占用判定不变，将按 Agent 和触发类型统计 pending/running Delivery 的查询收拢到 repository。
- 起始提交：`7a6aff17f`
- 代码提交：`22413ba75`

## 实际改动

- 为 `AgentDeliveryRepository` 增加 outstanding Delivery 计数查询。
- 将 heartbeat 调度、账户级 Todo 调度和单 Agent Todo 启动三个占用检查接入同一 repository 读取路径。
- facade 继续负责 Agent/Todo 校验、事务、是否入队判断以及消息和 Delivery 写入编排。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentDeliveryRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 计数仍限定相同 owner、target Agent、trigger kind 以及 `pending`/`running` 状态。
- heartbeat 与 Todo 的触发类型原始值不变，owner 与 Agent 的 SQL 参数顺序不变。
- 三条调用路径仍以计数为 0 作为继续调度的门禁。
- 查询仍位于各自原有事务和循环位置，后续 Todo 状态检查、消息写入与 Delivery 写入顺序不变。
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

- root message 的 Delivery 数量与房间内 active Delivery ID 列表仍位于 facade。
- Delivery 写事务、claim UPDATE 和调度业务编排仍由 facade 持有。
- Run 聚合根的读取仍位于 facade。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 将 root message Delivery 计数与房间 active Delivery ID 列表作为下一个只读单元接入 repository，同时保持路由预算和取消顺序不变。
