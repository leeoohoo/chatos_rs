# 3.0.4 进度：拆分成员加入提案读取 Repository

- 时间：2026-09-19 15:33:20（Asia/Shanghai）
- 本轮目标：保持成员加入提案的列表、定点读取、幂等读取和 DEBUG statement 计数不变，继续收拢 Proposal 聚合根的 SQLite 读取边界。
- 起始提交：`e656a92ae`
- 代码提交：`7e4dc96d1`

## 实际改动

- 扩展 `AgentProposalRepository`，承载成员加入提案列表、按 proposal ID 定点读取以及按 proposer/delivery/request key 幂等读取。
- 将 `listMembershipProposals` 和两个内部 `readMembershipProposal` 路径接入 repository。
- 将成员加入提案 SELECT 列清单收拢到 repository。
- facade 继续负责参数校验、来源房间存在性检查、成员与团队校验以及创建、审批和拒绝事务编排。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentProposalRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 成员加入提案的 SELECT 列、表名、owner/source-room/proposal/proposer/delivery/request 条件和 SQL 参数顺序不变。
- 可选 status 过滤仍只在传值时追加，列表排序仍为 `created_at_unix_ms, id`。
- 定点与幂等查询仍使用 `LIMIT 1` 和 `.first` 语义。
- 来源房间存在性、目标团队与成员状态校验、错误类型、事务边界和写入顺序不变。
- 每次 repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。

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

- Team 与 Project proposal 的读取仍位于 facade。
- 所有 Proposal 写事务及权限校验仍由 facade 编排，尚未评估进一步拆分边界。
- Delivery 与 Run 聚合根尚未建立完整 repository 读取边界。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 分批将 Team 与 Project proposal 的列表、定点和幂等读取接入同一 repository，继续冻结列清单、排序、参数顺序和 statement 计数。
