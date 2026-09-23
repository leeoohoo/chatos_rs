# 3.0.4 进度：拆分 Profile 读取 Repository

- 时间：2026-09-19 12:02:33（Asia/Shanghai）
- 本轮目标：保持 Profile 查询 SQL、过滤、排序和 statement 计数不变，建立首个按聚合根划分的 SQLite repository 读取边界。
- 起始提交：`1aa5fdf9d`
- 代码提交：`fd65a6fbc`

## 实际改动

- 新增 `AgentProfileRepository`，承载账号 Agent 列表和按 Agent ID 读取两个查询。
- 将 Profile SELECT 列清单收拢到 repository，复用既有 `AgentGroupChatRowMapper.agent`。
- facade 保留输入校验和协议入口，通过显式 prepared-statement 回调维持调试计数。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentProfileRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SELECT 列文本、owner 过滤、archived 条件拼接和 `ORDER BY name, id` 不变。
- 按 ID 查询的过滤条件、返回首行语义和 Profile 行映射不变。
- 每次 repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- actor facade、输入校验、SQL 参数顺序、错误传播和 Store 公共 API 不变。

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

- Profile 创建、更新和 heartbeat 调度仍由 facade 编排，尚未进入 repository。
- 其他聚合根尚未采用同样的 repository 边界。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以相同的显式 statement 计数边界抽取 Conversation 的 Room/Member 纯读取 repository，再评估写路径事务编排的最小迁移单元。
