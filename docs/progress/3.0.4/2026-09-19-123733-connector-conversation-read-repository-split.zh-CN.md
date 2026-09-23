# 3.0.4 进度：拆分 Conversation 读取 Repository

- 时间：2026-09-19 12:37:33（Asia/Shanghai）
- 本轮目标：保持 Room/Member 查询 SQL、过滤、排序和 statement 计数不变，建立 Conversation 聚合根的 SQLite repository 读取边界。
- 起始提交：`86ac3f1d7`
- 代码提交：`7c7b29999`

## 实际改动

- 新增 `AgentConversationRepository`，承载项目房间列表、直聊列表、成员列表以及 Room/Member 定点读取。
- 将 active project room 与 direct-key room 查询连同 Room/Member 列清单收拢到 repository。
- facade 保留输入校验、not-found 判定和协议入口，通过显式 prepared-statement 回调维持调试计数。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentConversationRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- Room/Member 的 SELECT 列、owner/room/project/direct-key 条件与 active/project-team 过滤不变。
- 项目房间和直聊仍按 `updated_at_unix_ms DESC, id DESC` 排序，成员仍按 `joined_at_unix_ms, agent_id` 排序。
- 单条读取仍使用 `.first`，每次 repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- actor facade、输入校验、not-found 行为、参数顺序、错误传播和 Store 公共 API 不变。

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

- Conversation 创建、成员写入和默认 Agent/Manager 更新仍由 facade 编排。
- Message 读取仍同时依赖主记录、mention 与附件查询，需单独设计 repository 边界。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 抽取 Team Asset 或 Todo 的纯读取 repository，继续保持 statement 计数和全部 SQL 文本不变，再逐步处理写路径事务编排。
