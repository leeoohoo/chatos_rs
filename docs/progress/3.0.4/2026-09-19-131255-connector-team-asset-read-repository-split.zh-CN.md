# 3.0.4 进度：拆分 Team Asset 读取 Repository

- 时间：2026-09-19 13:12:55（Asia/Shanghai）
- 本轮目标：保持 Team Asset、Revision 与 Todo Snapshot 查询行为和 statement 计数不变，建立 Team Asset 聚合根的 SQLite repository 读取边界。
- 起始提交：`81da2619e`
- 代码提交：`4446ad5b5`

## 实际改动

- 新增 `AgentTeamAssetRepository`，承载资产列表、单资产读取、Revision 列表以及 Todo Asset Snapshot 列表与定点读取。
- 将 Team Asset SELECT 列清单收拢到 repository，并让 upsert 的 existing-asset 查询复用同一读取入口。
- 将 `LocalAgentTeamAssetRevision` 行映射加入 `AgentGroupChatRowMapper`。
- facade 保留输入校验、团队类型校验、not-found 判定和写事务编排。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTeamAssetRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Asset、Revision 与 Snapshot 的 SELECT 列、owner/team/todo/asset/revision 条件和可空字段不变。
- 资产排序仍为 `category, updated_at_unix_ms DESC, id`；Revision 与 Snapshot 排序不变。
- upsert 中 existing asset 的读取时点、事务边界与返回首行语义不变。
- 每次 repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- actor facade、校验、参数顺序、错误传播和 Store 公共 API 不变。

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
- 全量测试结束后清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- Team Asset 的 upsert/archive SQL 和 Revision 写入仍由 facade 编排。
- Todo、Proposal、Delivery 与 Run 聚合根尚未建立 repository 读取边界。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 抽取 Todo 的列表、定点读取与 delivery lookup 读取路径；继续冻结 Todo 排序、terminal 过滤和 statement 计数。
