# 3.0.4 进度：拆分 Artifact Sync Due 读取 Repository

- 时间：2026-09-20 02:33:03（Asia/Shanghai）
- 本轮目标：保持附件同步下一到期时间的状态筛选、MIN 聚合与空值语义不变，将纯读取迁入 `AgentAttachmentRepository`。
- 起始提交：`7534a3d9b`
- 代码提交：`7a50410d0`

## 实际改动

- 为 `AgentAttachmentRepository` 增加按 owner 读取下一附件同步到期时间的入口。
- facade 在完成 owner 校验后改为委托 repository，不再直接持有该聚合 SELECT。
- repository 保留可空 Int64 行映射与“聚合行存在但值为 NULL”的双层 optional 展平语义。
- 上传 claim、状态更新、退避和重试写入均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentAttachmentRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍限定当前 owner，并只统计 `queued`、`failed`、`uploading` 状态。
- 仍使用 `MIN(next_retry_at_unix_ms)`，参数顺序保持不变。
- 没有待同步记录时聚合 NULL 仍映射为 nil；存在记录时原样返回最小 Int64。
- owner ID 校验仍在查询前由 facade 执行。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- artifact 调度、claim 事务、文件校验和上传状态写入均未改变。

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

- 附件 payload 元数据 SELECT 仍由 facade 持有，并与受保护本地文件恢复编排相邻。
- 上传状态 UPDATE、重试退避和文件完整性校验是业务编排行为，本轮未移动。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分附件 payload 元数据定点读取与行映射，保留 room 归属检查、本地文件恢复和完整性校验编排。
