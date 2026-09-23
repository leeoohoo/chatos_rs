# 3.0.4 进度：拆分 Artifact Upload Candidate 读取 Repository

- 时间：2026-09-20 01:58:40（Asia/Shanghai）
- 本轮目标：保持附件上传候选的 JOIN、筛选、排序、attempt 计算和原子 claim 语义不变，将候选 SELECT 迁出 facade。
- 起始提交：`41504aa85`
- 代码提交：`3835d95f2`

## 实际改动

- 新增 `AgentAttachmentRepository` 与内部 `AgentArtifactUploadCandidate` 值对象。
- 将下一个附件上传候选的 message JOIN、状态/重试时间筛选、排序、限制和行映射迁入 repository。
- facade 继续在原事务内读取候选并执行 uploading 状态 UPDATE，以 `sqlite3_changes == 1` 决定是否成功 claim。
- 本地文件读取、大小/SHA-256 校验、失败回写和上传请求构造均保持在 facade。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentAttachmentRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 候选仍只来自当前 owner 的 Agent 消息附件，并限定 `queued`、`failed`、`uploading` 状态。
- 仍要求 `next_retry_at_unix_ms <= now` 且 SHA-256 非空。
- 仍按 `next_retry_at_unix_ms, id` 正序取第一条，参数顺序仍为 owner、now。
- attempt 仍由持久化 `upload_attempt + 1` 计算。
- SELECT 与 claim UPDATE 仍处于同一 `BEGIN IMMEDIATE` 事务，UPDATE 条件和五分钟 lease 不变。
- 竞争失败仍返回 nil；文件缺失或完整性失败仍调用原失败回写路径。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。

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

- 下一附件同步到期时间和附件 payload 元数据 SELECT 仍由 facade 持有。
- 上传状态 UPDATE、重试退避和文件完整性校验是业务编排行为，本轮未移动。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分下一附件同步到期时间读取，保持空聚合结果和 statement 计数不变。
