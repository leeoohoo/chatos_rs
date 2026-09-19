# 3.0.4 进度：拆分 Message 定点与批量读取 Repository

- 时间：2026-09-19 20:48:20（Asia/Shanghai）
- 本轮目标：保持消息行组装及其 mentions、attachments 嵌套读取不变，将按 ID 定点读取和 ID 集合批量读取收拢到独立 repository。
- 起始提交：`7d0fcf4c8`
- 代码提交：`66f487209`

## 实际改动

- 新增 `AgentMessageRepository`，承接按 owner/message ID 定点读取和按 owner/message ID 集合批量读取。
- facade 继续负责 owner、room、message ID 与批量上限校验，以及房间存在性和消息归属判断。
- repository 接收原有消息行组装闭包，因此 mentions、attachments 读取、错误语义和 DEBUG prepared-statement 计数保持不变。
- 空 ID 集合仍直接返回空字典，不 prepare SQL。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 定点读取仍限定 `owner_user_id` 和 `id`，并保持首行/无记录语义。
- 批量读取仍先去重、排序并逐项校验 ID，最多接受 500 个 ID；SQL 的 owner 参数与 ID 参数顺序不变。
- 两类查询的 SELECT 列及列顺序保持不变。
- 每条消息仍按原路径读取有序 mentions 和 attachments，没有顺带消除 N+1 查询或改变 statement 基线。
- 公共 `message` API 仍校验 room 存在，并在读取后验证消息属于目标 room。
- 消息写入、路由、分页、未读游标和附件持久化均未改变。

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

- 基础时间顺序、前后向分页、room 未读和全局未读等 Message 列表 SELECT 仍由 facade 持有。
- 消息行组装仍会为每条消息读取 mentions 和 attachments；没有可复现的性能退化证据前保持现状。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续以单个有界工作单元迁移 Message 基础列表或分页读取，精确保留系统消息过滤、游标、排序、`limit + 1` 和参数顺序。
