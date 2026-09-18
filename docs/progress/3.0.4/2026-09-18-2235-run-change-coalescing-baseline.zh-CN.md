# 3.0.4 推进记录：Run 高频变更合并基线

- 时间：2026-09-18 22:35:06 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：为高频 Run 更新建立可重复的 invalidation stream 基线，并验证慢消费者不会积压全部中间事件。
- 起始提交：`d62305965`
- 代码提交：`39ee4121c`

## 实际改动

1. 扩展现有 opt-in Agent Store 基线，在每轮创建一个房间限定的 `NativeAgentGroupChatService.changes` 订阅。
2. 连续发布 500 个 `runUpdated` 事件，记录发布耗时。
3. 消费者在全部发布完成后才读取事件，并断言收到的是最后一个 Run ID，从测试层冻结 `bufferingNewest(1)` 的合并语义。
4. 将三轮高频发布耗时写入同一份排序 JSON，供后续 ViewModel generation/coalescing 重构做前后对比。

## 涉及文件

- `clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift`

## 业务不变量

- 不修改生产代码、SQLite、事件类型、事件过滤、UI 刷新策略或业务数据。
- 只验证 invalidation wake-up 合并；SQLite 仍是唯一事实源，消费者仍需在收到事件后重新读取。
- 不用延长 debounce、吞掉最后状态或降低数据新鲜度换取测量结果。
- 工作区中原有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证与测量结果

1. `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 通过。
2. 500 个 `runUpdated` 发布三轮耗时：`0.483 / 1.767 / 0.461 ms`。
3. 三轮慢消费者都只收到最后一个 Run ID，确认缓冲区有界且最终状态不会被更早事件覆盖。
4. `swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 通过；默认门禁按设计跳过重型 fixture。
5. `swift test --package-path clients/macos` 全量通过，新增用例没有造成其他测试失败。
6. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 本轮只证明 Connector 的事件缓冲有界；尚未测量 App ViewModel 收到事件后的完整 `load()` 次数、主线程耗时或后台空闲 CPU。
- 工作区现有 ViewModel 文件带有并行改动，当前自动任务继续避让，不能在该改动归属明确前加入刷新计数或拆分。
- 下一轮优先补 Core Codable/错误契约 characterization，或在不触碰并行文件的前提下补空闲 heartbeat 读写基线。
