# 3.0.4 推进记录：空闲 heartbeat 轮询基线

- 时间：2026-09-18 23:46:20 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：用可重复数据证明没有到期 Agent 时 heartbeat Store 检查的查询成本和写入行为。
- 起始提交：`8b9ec9ad1`
- 代码提交：`8299b2d0a`

## 实际改动

1. 在 Debug 测试诊断中增加 SQLite `total_changes` 读取，仅供测试计算前后差值；Release 不包含该诊断入口。
2. 扩展 opt-in Store 基线，对 heartbeat 全部关闭的 20 Agent fixture 连续执行 500 次 `nextAgentHeartbeatDue`。
3. 同时记录 500 次轮询耗时、prepared statement 数量和数据库 change 数量，并冻结三轮一致结果。
4. 断言每轮只执行 500 条只读查询且产生 0 次数据库变更，防止未来空闲轮询意外写盘。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- `clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift`

## 业务不变量

- 不修改 heartbeat 到期计算、间隔、唤醒、Delivery 创建、账号隔离或调度策略。
- 不修改 SQLite schema、SQL 或事务；新增诊断只读取 SQLite 自带累计 change 计数。
- 不以延长轮询周期换取更低耗时，也不把本轮 Store 微基准冒充完整 App 空闲 CPU 结论。
- 工作区中原有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证与测量结果

1. `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 通过。
2. 500 次空闲 heartbeat 到期查询三轮耗时：`8.290 / 8.582 / 8.509 ms`。
3. 三轮 prepared statement 数量：`500 / 500 / 500`；数据库 changes：`0 / 0 / 0`。
4. 同轮继续确认消息工作区 `1008` 条语句、最近 20 条消息 `42` 条语句以及高频 change stream 最终事件合并语义未漂移。
5. `swift test --package-path clients/macos` 全量通过；默认门禁中的重型基线按设计跳过，其他测试无失败。
6. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 本轮说明单次 Store 查询无写入且成本稳定，但 App heartbeat coordinator 的计时器唤醒频率、进程级 CPU 和账号级 drain 仍需单独测量。
- ViewModel 并行改动仍未归属到自动任务，因此主线程刷新与 UI generation 基线继续避让。
- 阶段 0 下一步可补 coordinator 的可注入时钟/空闲唤醒 characterization；若需改动 AppModel，则应先确认不与并行工作重叠。
