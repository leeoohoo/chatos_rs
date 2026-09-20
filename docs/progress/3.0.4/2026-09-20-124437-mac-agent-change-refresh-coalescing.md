# macOS Agent 聊天变更刷新合并

- 时间：2026-09-20 12:44:37 CST（Asia/Shanghai）
- 本轮目标：基于既有可重复基准，消除同批 Agent 房间/Run 事件触发的重复完整刷新。
- 起始提交：`482bcbd66`
- 代码提交：`b00828fde`

## 实际改动

- 新增 MainActor 隔离的 `AgentChangeRefreshCoalescer`，在原有 120ms 窗口内把突发变更合并成一次刷新。
- 群聊与 Agent 私聊共用该合并器，不再让每个缓冲事件各自等待后触发一次完整 `load()`。
- 若刷新执行期间又收到变更，合并器在当前刷新后安排一次有界后续刷新，避免持续事件导致饥饿或丢失最终状态。
- 更新 UI 性能基准冻结值，并新增 500 次突发只刷新一次、刷新中变更触发一次后续刷新的回归测试。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentChangeRefreshCoalescer.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentGroupChatManagement.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentDirectChatView.swift`
- `clients/macos/Tests/ChatOSAppTests/AgentChangeRefreshCoalescerTests.swift`
- `clients/macos/Tests/ChatOSAppTests/AgentGroupChatViewModelPerformanceBaselineTests.swift`

## 业务不变量

- 变更仍通过原账户/房间 change stream 驱动，刷新仍读取 SQLite 权威快照。
- 合并窗口保持原来的 120ms，没有用延长 debounce 换取表面性能。
- 刷新期间的新变更不会静默丢弃；最多合并为一次紧随其后的刷新。
- 消息排序、未读、附件、提案、Todo、Run 与 scheduler 语义不变。

## 测试与基准结果

- 修改前 500-event burst：246 条 prepared statement、42 次 UI publish。
- 修改后连续三次：均为 123 条 prepared statement、21 次 UI publish；查询和发布放大稳定减少 50%。
- 三次修改后 settled 时间：约 552.27ms、572.80ms、587.42ms；未通过降低数据新鲜度或省略业务数据实现。
- `swift test --package-path clients/macos --filter AgentChangeRefreshCoalescerTests`：2 通过，0 失败。
- `CHATOS_RUN_AGENT_UI_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatViewModelPerformanceBaselineTests`：连续三次通过。
- `swift test --package-path clients/macos`：全量通过；现有环境门禁测试按设计跳过。
- `git diff --check`：通过。

## 剩余风险与下一步

- 当前优化减少同批重复刷新，单次完整刷新仍为 123 条查询；进一步做增量刷新必须先增加事件载荷语义与独立前后基准，不在本轮扩大行为变化。
- Agent Workspace 的批量 Delivery/Message 读取、Story 布局和 SQLite 测试仍属于工作区既有并发修改，本轮未纳入提交。
- 真实登录账号高频 Run 场景仍需运行时环境验证，但自动化基准已锁定查询与 UI 发布上限。
