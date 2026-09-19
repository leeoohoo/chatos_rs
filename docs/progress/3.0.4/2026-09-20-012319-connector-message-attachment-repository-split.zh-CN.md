# 3.0.4 进度：拆分 Message Attachment 读取 Repository

- 时间：2026-09-20 01:23:19（Asia/Shanghai）
- 本轮目标：保持每条消息的 attachment 查询、位置排序、默认状态和行映射不变，将纯读取与映射迁出 facade。
- 起始提交：`26b6cb8b0`
- 代码提交：`4be649a57`

## 实际改动

- 为 `AgentMessageRepository` 增加按 owner/message ID 读取 attachments 的入口。
- 在 `AgentGroupChatRowMapper` 新增 `ProjectAgentMessageAttachment` 行映射。
- 消息行组装改为依次委托 repository 读取 mentions 和 attachments，再按原字段构造消息。
- 删除 facade 内仅用于读取 message attachments 的 SQL 和行映射辅助方法。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentGroupChatRowMapper.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SELECT 的 15 个列及顺序保持不变，查询仍限定 `owner_user_id` 与 `message_id`。
- attachments 仍按 `position` 正序返回，空列表语义不变。
- 无效 kind/origin 仍抛出 `invalid message attachment` storage 错误。
- 无法识别的 sync status 仍回退为 `localOnly`，所有可空远端字段与同步时间映射不变。
- 每条消息仍先读取 mentions、再读取 attachments；未批处理、缓存或合并查询。
- 每次 attachment 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 附件文件读取、上传队列、写入和同步状态更新行为均未改变。

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

- 附件上传队列与下一重试时间 SELECT 仍由 facade 持有，需按业务边界独立拆分。
- 消息行组装仍为每条消息执行 mention 和 attachment 查询；没有可复现基准前不优化该路径。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 重新审计 facade 剩余纯 SELECT；优先以单个有界工作单元拆分附件上传队列或下一重试时间读取，不移动上传状态写入。
