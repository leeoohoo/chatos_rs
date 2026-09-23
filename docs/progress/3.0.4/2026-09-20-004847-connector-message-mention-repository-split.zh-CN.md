# 3.0.4 进度：拆分 Message Mention 读取 Repository

- 时间：2026-09-20 00:48:47（Asia/Shanghai）
- 本轮目标：保持每条消息的 mention 查询、位置排序和行组装顺序不变，将纯读取迁入 `AgentMessageRepository`。
- 起始提交：`9c6dfe3c6`
- 代码提交：`dfa8b4bf1`

## 实际改动

- 为 `AgentMessageRepository` 增加按 owner/message ID 读取 mention Agent ID 的入口。
- 消息行组装改为委托 repository 获取 mentions，再按原顺序读取 attachments 并构造消息。
- repository 内增加最小字符串列读取辅助函数，不改变空值到空字符串的既有行为。
- 未合并、批处理或缓存逐消息 mention 查询，避免改变冻结 statement 基线。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍限定 `owner_user_id` 与 `message_id`，参数顺序保持不变。
- mention 仍按 `position` 正序返回，重复或空列表语义不变。
- 每条消息仍先查询 mentions，再查询 attachments，然后构造 `ProjectAgentMessageDraft`。
- `mentionedAgentIDs` 仍直接使用查询结果，不做额外去重或排序。
- 每次 mention 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 消息主查询、附件读取、写入、路由和游标行为均未改变。

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

- Message attachments 的逐消息读取仍由 facade 持有；附件上传队列和重试时间 SELECT 也尚未拆分。
- 消息行组装仍为每条消息执行 mention 和 attachment 查询；没有可复现基准前不优化该路径。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分每条消息的 attachments 纯读取与行映射，保留列、默认 sync status、position 排序和 statement 计数。
