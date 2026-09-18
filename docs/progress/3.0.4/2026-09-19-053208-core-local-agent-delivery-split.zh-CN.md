# 3.0.4 进度：拆分 Local Agent Delivery 与 Run 领域模型

- 时间：2026-09-19 05:32:08（Asia/Shanghai）
- 本轮目标：在不改变业务行为的前提下，将 Delivery、Run Lane 与消息路由结果相关声明从 `AgentGroupChat.swift` 纯移动到独立文件。
- 起始提交：`76bcf50c1`
- 代码提交：`519869485`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentDelivery.swift`。
- 移入 Delivery Trigger、Run Lane、Delivery Status、Delivery entity、Routing Limits 与 Post Result 类型。
- `AgentGroupChat.swift` 减少 103 行；新文件包含 104 行（额外一行为独立文件所需的 `Foundation` 导入）。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentDelivery.swift`

## 业务不变量

- Delivery trigger/status 与 Run lane 的 raw value 不变。
- Delivery entity 的属性、初始化器、默认值、lane 推导与 Codable 契约不变。
- 路由 hop/run 限制的默认值和验证范围不变。
- Post Result 中消息、投递与停止原因的语义不变。
- Store、SQLite、Tool Provider、Scheduler 和 UI 行为不变。
- 除新文件所需的 `Foundation` 导入外，移动声明与原声明逐字一致。

## 验证结果

- 声明移动前后逐字比对：通过（`BYTE_FOR_BYTE_DELIVERY_RUN_DECLARATIONS_MATCH`）。
- AgentGroupChat 领域公开声明数量：仍为 74。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败：
  - `AgentGroupChatCodableContractTests`
  - `LocalAgentDraftTests`
  - `SQLiteAgentGroupChatStoreTests`
  - `LocalAgentChatToolProviderTests`
- 第一次完整 `swift test --package-path clients/macos` 中，未涉及本轮代码的 `NativeTerminalTests/localPTYSmokeTest` 因 PTY 五秒超时失败；该测试随后单独重跑通过。
- 再次完整执行 `swift test --package-path clients/macos`：退出码 0；App target 105 个测试中 1 个按设计 opt-in 跳过、0 失败，其余测试目标全部通过。

## 剩余风险

- `NativeTerminalTests/localPTYSmokeTest` 存在偶发 PTY 启动超时；本轮未改终端代码，单测重跑和完整套件重跑均通过，后续阶段继续观察。
- 本轮为声明纯移动；后续 Delivery/Run 模型应继续进入独立领域文件。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续阶段 1 的纯拆分：将 `AgentGroupChatStore` protocol 移入独立 Core 文件，并执行定向测试与完整 macOS 测试。
