# 3.0.4 进度：拆分附件 Payload 元数据读取 Repository

- 时间：2026-09-20 03:09:32（Asia/Shanghai）
- 本轮目标：保持附件归属、行映射和文件恢复行为不变，将 `messageAttachment(...)` 的 payload 元数据定点读取迁入 `AgentAttachmentRepository`。
- 起始提交：`f83345334`
- 代码提交：`b86245e83`

## 实际改动

- 将 facade 私有的 `StoredMessageAttachment` 值对象迁至附件 repository 文件。
- 为 `AgentAttachmentRepository` 增加按 owner、message、attachment、room 定点读取 payload 元数据的入口。
- facade 在完成四个 ID 校验后委托 repository 读取元数据；本地文件路径校验、远端下载恢复、大小与 SHA-256 完整性检查仍留在 facade。
- 未改动附件写入、上传 claim、同步状态更新或重试退避逻辑。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentAttachmentRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- SELECT 的 16 个列及列顺序保持不变。
- 参数顺序仍为 owner、message、attachment、room。
- 查询仍 JOIN message，并通过 `m.room_id` 限定附件的 room 归属。
- 无匹配记录仍返回 nil；无效 kind 或 origin 仍抛出 `invalid message attachment` 存储错误。
- 未知 sync status 仍回退为 `.localOnly`，可空远端元数据的映射保持不变。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- 受保护相对路径解析、缺失文件处理、远端恢复条件、大小/SHA-256 校验、目录与文件权限均未改变。

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

- facade 中可能仍有未迁出的纯 SELECT，需要继续审计后才能判断 macOS 阶段 1 是否达到退出条件。
- 上传状态 UPDATE、重试退避和文件完整性校验属于业务编排，本轮未移动。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 审计 facade 剩余纯 SELECT；若仍存在，以单个有界工作单元继续拆分，否则按阶段门禁验证阶段 1 退出条件。
