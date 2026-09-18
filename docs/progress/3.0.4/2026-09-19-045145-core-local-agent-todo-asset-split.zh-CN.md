# 3.0.4 进度：拆分 Local Agent Todo 与 Asset 领域模型

- 时间：2026-09-19 04:51:45（Asia/Shanghai）
- 本轮目标：在不改变业务行为的前提下，将 Todo、执行计划、依赖、进度与团队资产相关声明从 `AgentGroupChat.swift` 纯移动到独立文件。
- 起始提交：`64839b10f`
- 代码提交：`b77258108`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentTodo.swift`。
- 移入 Todo 状态、内建能力、插件选择、执行计划/契约、来源、依赖、调度状态、进度，以及 Team Asset、Revision、Todo Asset Snapshot 类型。
- `AgentGroupChat.swift` 减少 596 行；新文件包含 597 行（额外一行为独立文件所需的 `Foundation` 导入）。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentTodo.swift`

## 业务不变量

- Todo、能力、资产与进度状态的 raw value 不变。
- Todo draft/update/entity、执行计划和执行契约的属性、初始化器、默认值与 Codable 契约不变。
- 依赖关系、来源引用、调度状态和优先级语义不变。
- 团队资产版本、状态和 Todo 启动时快照语义不变。
- Store、SQLite、Tool Provider、Scheduler 和 UI 行为不变。
- 除新文件所需的 `Foundation` 导入外，移动声明与原声明逐字一致。

## 验证结果

- 声明移动前后逐字比对：通过（`BYTE_FOR_BYTE_TODO_ASSET_DECLARATIONS_MATCH`）。
- AgentGroupChat 领域公开声明数量：仍为 74。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败：
  - `AgentGroupChatCodableContractTests`
  - `LocalAgentDraftTests`
  - `SQLiteAgentGroupChatStoreTests`
  - `LocalAgentChatToolProviderTests`
- `swift test --package-path clients/macos`：退出码 0；完整 macOS Swift 测试通过，App target 105 个测试中 1 个按设计 opt-in 跳过、0 失败。

## 剩余风险

- 本轮为声明纯移动；后续 Todo/Asset 模型应继续进入独立领域文件，避免聚合文件重新膨胀。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续阶段 1 的纯拆分：将 Delivery 与 Run 相关领域模型移入独立 Core 文件，并执行定向测试与完整 macOS 测试。
