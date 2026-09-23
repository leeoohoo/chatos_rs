# 3.0.4 推进记录：Agent Store 语句计数基线

- 时间：2026-09-18 21:59:48 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：给上一轮数据规模基线补上可重复的 SQLite prepared statement 数量证据，避免只凭耗时判断热点。
- 起始提交：`a125ddbf5`
- 代码提交：`d5f30a086`

## 实际改动

1. 在 Debug 构建中为 `SQLiteAgentGroupChatStore` 增加 actor 隔离的 prepared statement 计数器；Release 构建不包含计数状态或递增开销。
2. 测试通过 `@testable` 读取计数，不扩大 Store 的公开生产 API。
3. 扩展 opt-in 性能基线，同时输出并冻结工作区快照、最近消息和单附件读取的语句数量。
4. 首次按“预期批量读取”的 6/2/1 次断言运行时稳定得到 1008/42/1 次；据此将现状冻结为基线，后续只有携带前后测量的修复才能更新断言。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- `clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift`

## 业务不变量

- 不修改 SQLite schema、SQL、索引、事务、查询结果、排序或 UI 行为。
- 计数器仅在 `DEBUG` 编译条件下存在；Release 生产路径保持原样。
- 不把测量钩子公开给 App 或模型，不记录 SQL 参数、消息内容、路径或账号信息。
- 工作区中原有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证与测量结果

1. `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 最终通过。
2. 三轮 prepared statement 数量完全一致：
   - 500 条消息的工作区快照：`1008 / 1008 / 1008`
   - 最近 20 条消息：`42 / 42 / 42`
   - 单个图片附件读取：`1 / 1 / 1`
3. 同轮耗时样本：
   - Store 打开：`1.043 / 1.071 / 1.012 ms`
   - 工作区快照：`304.155 / 122.232 / 131.559 ms`
   - 最近 20 条消息：`0.419 / 0.384 / 0.381 ms`
   - 128 KiB 图片附件读取：`0.348 / 0.334 / 0.216 ms`
4. `swift test --package-path clients/macos` 全量通过；新增性能用例在默认门禁中按设计跳过，其余测试无失败。
5. `git diff --cached --check` 通过。

## 已证实问题、风险与下一步

- 已证实消息列表存在线性语句放大：读取 20 条消息执行 42 条语句，读取 500 条消息所在工作区执行 1008 条语句。代码证据是 `readMessage(_:)` 对每条消息分别读取 mentions 和 attachments。
- 该问题已从“候选”升级为有稳定复现数据的性能/设计问题；当前只建立基线，尚未修改查询或业务行为。
- 按实施方案，阶段 0 还需补齐高频 change stream、空闲 CPU 和主线程长任务证据；完成纯拆分门禁后，在性能阶段批量读取 mentions/attachments，并用本基线证明语句数量和耗时改善。
