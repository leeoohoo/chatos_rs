# 3.0.4 进度：拆分 Message 最近消息分页读取 Repository

- 时间：2026-09-19 22:30:06（Asia/Shanghai）
- 本轮目标：保持最近消息反向分页的游标校验、过滤、排序与返回页语义不变，将分页 SQL 收拢到 `AgentMessageRepository`。
- 起始提交：`54f3d681e`
- 代码提交：`38672f79f`

## 实际改动

- 为 `AgentMessageRepository` 增加反向分页读取入口，并复用已有消息游标值对象。
- 将反向游标 SQL 条件、参数组装、系统内部消息过滤、倒序查询和 `limit + 1` 读取迁出 facade。
- facade 继续负责 owner、room、limit、游标 ID 校验，以及游标消息存在且属于目标 room 的权限边界。
- facade 继续负责截取倒序结果、反转为时间正序、计算首页游标和 `hasMore`。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentMessageRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 无游标时仍从 room 的最新可见消息向前读取。
- 有游标时仍使用 `created_at_unix_ms < ? OR (created_at_unix_ms = ? AND id < ?)`，时间参数仍重复绑定两次。
- 仍过滤 causation 为 `heartbeat`、`todo`、`todo_status` 的 system 消息。
- SQL 仍按 `created_at_unix_ms DESC, id DESC` 查询并读取 `limit + 1` 条。
- 游标消息缺失或不属于目标 room 时仍返回 `notFound`；limit 仍限制在 1 到 500。
- 返回页仍截取前 limit 条后反转为正序，以页面首条 ID 作为 next cursor，并以额外一条判断 `hasMore`。
- 消息行组装、mentions、attachments 及 prepared-statement 计数路径不变。

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

- Message room 未读和全局未读 SELECT 仍由 facade 持有。
- 消息行组装仍会为每条消息读取 mentions 和 attachments；没有可复现基准前不改变此行为。
- macOS 阶段 1 尚未满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元迁移 room 范围的未读 Message 读取，精确保留 read cursor、排除自身回复、`limit + 1` 和返回页语义。
