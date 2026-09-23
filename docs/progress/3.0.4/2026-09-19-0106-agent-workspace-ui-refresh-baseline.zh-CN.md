# 3.0.4 推进记录：Agent 工作区 UI 刷新基线

- 时间：2026-09-19 01:06:07 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：在 App 层固定数据集上测量工作区激活、主线程响应和高频 change burst 的刷新放大成本。
- 起始提交：`49e18cb3a`
- 代码提交：`f2976a6d7`

## 实际改动

1. 新增 opt-in 的 `AgentGroupChatViewModel` 性能 characterization test，使用 20 Agent、10 团队、500 消息、100 Todo 的可重复本地 fixture。
2. 覆盖真实 `activate()` 路径，包括首次主快照、补充数据、空闲 scheduler pass、change stream 订阅和后续刷新。
3. 用 MainActor 1ms 探针记录测试期间的最大调度间隔，同时记录工作区发布次数、prepared statement 数量和端到端稳定耗时。
4. 连续发布 500 个无持久化变更的 Run/房间事件，冻结 change stream 当前合并后的 SQL 与 UI 发布放大结果。
5. 测试依赖拒绝任何模型、Memory 或配对请求，确保基线不会访问网络或进入 Agent 执行。

## 涉及文件

- `clients/macos/Tests/ChatOSAppTests/AgentGroupChatViewModelPerformanceBaselineTests.swift`

## 业务不变量

- 不修改 ViewModel、Store、scheduler、change stream、debounce、消息分页或 UI 状态逻辑。
- fixture 的 500 条消息不创建 Agent Delivery，避免性能测试执行或改变业务任务。
- 不用吞事件、延长 debounce 或降低数据新鲜度换取更低数字；本轮只记录当前行为。
- 工作区中原有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证与测量结果

1. `CHATOS_RUN_AGENT_UI_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatViewModelPerformanceBaselineTests` 通过。
2. 四次有效重复中，计数均稳定：首次激活 `349` 条 SQL、`45` 次 UI 发布；500 事件 burst 合并后仍产生 `246` 条 SQL、`42` 次 UI 发布。
3. 计数可与既有基线互相解释：空闲 scheduler drain 为 `103` 条 SQL；burst 的 `246` 条 SQL 对应两次完整刷新，即单次刷新当前约 `123` 条 SQL。
4. 最终样本首次 `load()` 返回约 `5.986 ms`，补充加载与空闲 scheduler 全部稳定约 `137.921 ms`；500 事件发布约 `22.011 ms`，刷新全部稳定约 `571.929 ms`。
5. 有效样本 MainActor 最大调度间隔约 `4.755–25.899 ms`；该值作为观察数据记录，不设易受机器负载影响的硬阈值。
6. `swift test --package-path clients/macos` 完整通过；App target 为 105 个测试、1 个 opt-in 基线按设计跳过、0 失败，其余 targets 也无失败。
7. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 500 个事件已被有界 stream 合并成两次刷新，并非 500 次；但两次无数据变化的刷新仍执行 246 条 SQL 并发布 42 次 UI 更新，这是阶段 4 可直接做前后对比的性能问题证据。
- MainActor 探针显示个别样本超过一帧预算，但 XCTest 调度会受机器负载影响；在声称主线程卡顿改善前仍应增加 Instruments 或 signpost 真机证据。
- 阶段 0 的固定规模 Store、SQL、heartbeat、空闲 drain 与 UI 刷新证据现已可重复；下一轮应审计剩余 characterization 覆盖，再决定进入阶段 1 的第一个纯拆分单元。
