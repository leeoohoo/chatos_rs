# 3.0.4 推进记录：空闲账号 drain 基线

- 时间：2026-09-19 00:24:50 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：测量无任何可执行工作的账号级 `drainAccount` 查询成本，并确认重复空闲 drain 不产生数据库写入。
- 起始提交：`af4914724`
- 代码提交：`5c5b7aee5`

## 实际改动

1. 扩展 opt-in Agent Store 性能基线，在既有 20 Agent、10 团队、500 消息、500 Run、100 Todo fixture 上新建 scheduler，并连续执行三轮空闲账号级 `drainAccount`。
2. 为每轮记录耗时、prepared statement 数量和 SQLite 累计 change 差值。
3. 冻结三轮一致的查询数量 `103 / 103 / 103`，并断言返回结果为空且数据库 changes 为 `0 / 0 / 0`。
4. 用拒绝任何模型或 Memory 构造的测试服务保证空闲路径不会意外进入 Agent 运行时。

## 涉及文件

- `clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift`

## 业务不变量

- 不修改 scheduler 的账号串行、claim、lane、公平性、取消、恢复、heartbeat 或执行规则。
- 不修改 SQLite schema、SQL、事务、排序或持久化数据。
- 不通过降低轮询频率、吞掉工作或放宽错误处理来改善数字；本轮只建立可复现基线。
- 工作区中原有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证与测量结果

1. `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 通过，1 个目标测试通过、0 失败。
2. 空闲账号 drain 三轮耗时：`17.466 / 17.335 / 17.011 ms`。
3. 三轮 prepared statement 数量：`103 / 103 / 103`；数据库 changes：`0 / 0 / 0`。
4. 同轮继续确认：最近 20 条消息读取 `42` 条语句、500 条消息工作区快照 `1008` 条语句、500 次 heartbeat 到期查询 `500` 条语句且 0 写入。
5. `swift test --package-path clients/macos` 完整通过；默认门禁中的 opt-in 重型性能基线按设计跳过，其他测试无失败。
6. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 空闲账号 drain 没有写入，但每轮固定执行 103 条查询，已形成有证据的性能候选；按方案留到阶段 4，在保持到期唤醒和调度语义的前提下修复并做前后对比。
- 当前 Store 微基准不能证明 App 主线程刷新、UI generation 或进程级空闲 CPU 情况；阶段 0 仍需补足可重复的 UI/主线程证据。
- 工作区三处用户并行改动仍未归属自动任务，后续需要继续避让；若基线必须触及同一文件，应先选择无冲突的观察入口。
