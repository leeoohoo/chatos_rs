# 3.0.4 进度：完成 Run 纯读取 Repository

- 时间：2026-09-19 19:39:07（Asia/Shanghai）
- 本轮目标：保持 Run JSON 解码、筛选、排序和限制不变，将 Run 的定点与列表读取收拢到独立 repository。
- 起始提交：`ec58a90f1`
- 代码提交：`316ed3d0b`

## 实际改动

- 新增 `AgentRunRepository`。
- 迁移按 delivery ID 定点读取 Run 的路径。
- 迁移按 project 查询未结束 Run、按 Agent 查询 Run 历史以及按 Room 查询 Run 历史的路径。
- facade 继续负责参数校验、limit 范围、Delivery/Run 一致性检查、事务和 Run 写入编排。
- facade 中已不再保留 `local_agent_group_chat_runs` 的 SELECT，Run 基础纯读取边界完成。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentRunRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 按 delivery 读取仍限定 owner/delivery，保持 `LIMIT 1` 与首行语义。
- 未结束列表仍排除 `completed`/`failed`，三类列表仍按 `updated_at_unix_ms DESC, id DESC` 排序。
- owner、project/agent/room 与 limit 的 SQL 参数顺序不变，limit 仍限制在 1 到 500。
- 查询仍先读取 `run_json` 字符串，再按原顺序调用 `AgentGroupChatRowMapper.run` 解码；无记录与无效 JSON 的语义不变。
- `saveRun`、房间批量停止和所有 Run UPDATE/UPSERT 事务边界不变。
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

- Run 写入、状态迁移和 Delivery 一致性检查继续由 facade 持有；本轮未移动这些业务行为。
- macOS 阶段 1 仍需进行退出条件审计，确认职责清单、历史数据库读取、快照和循环依赖证据完整后才能进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 对 macOS 阶段 1 做只读退出审计：核对 Core/持久化文件职责、facade 剩余 SQL 类型、历史迁移与快照证据，并据审计结果选择最后一个纯拆分单元或正式转入阶段 2。
