# 3.0.4 进度：拆分全局未读 Message 读取 Repository

- 时间：2026-09-19 23:39:10（Asia/Shanghai）
- 本轮目标：保持跨 room 未读消息的 membership/read cursor JOIN、过滤、排序及原子标记已读语义不变，将消息 SELECT 收拢到 `AgentMessageRepository`。
- 起始提交：`0e4ccfe05`
- 代码提交：`224c8ecd8`

## 实际改动

- 为 `AgentMessageRepository` 增加跨 active room 的全局未读消息读取入口。
- 将 room/member/read cursor JOIN、未读判定、系统内部消息过滤、排除自身回复、跨 room 正序排序和 limit 迁出 facade。
- facade 继续负责 owner/agent、limit、时间和 active Agent 校验，以及整个读取、分组、cursor 写入和 room 组装事务。
- facade 继续按首次出现顺序组织 conversation，并按每个 room 的最后一条消息更新 read cursor。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- JOIN 仍限定同 owner、目标 Agent、active room 和 active member。
- read cursor 缺失时仍读取全部可见消息；存在时仍按时间与 ID 复合游标严格向后读取。
- 仍过滤 causation 为 `heartbeat`、`todo`、`todo_status` 的 system 消息，并排除当前 Agent 自身回复。
- SQL 参数顺序仍为 member agent、cursor agent、owner、sender agent、limit。
- 仍按 `msg.created_at_unix_ms, msg.id` 跨 room 正序排序，limit 仍限制在 1 到 500。
- 空结果仍不写 cursor；非空结果的分组、各 room 最后消息选择、cursor UPSERT 与时间计算不变。
- SELECT 仍处于原事务内；消息行组装、mentions、attachments 与 prepared-statement 计数路径不变。

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

- read cursor 的定点读取仍由 facade 持有，Message mentions 与 attachments 读取也尚未进入独立 repository。
- 消息行组装仍会为每条消息读取 mentions 和 attachments；没有可复现基准前不改变此行为。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 重新审计 facade 中剩余的 Message/attachment/read cursor SELECT，选择下一个有界读取边界；优先拆 read cursor 定点读取或消息附件读取，不合并写入行为。
