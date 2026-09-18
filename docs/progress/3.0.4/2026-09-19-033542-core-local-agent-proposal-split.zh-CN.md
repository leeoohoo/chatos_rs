# 3.0.4 进度：拆分 Local Agent Proposal 领域模型

- 时间：2026-09-19 03:35:42（Asia/Shanghai）
- 本轮目标：在不改变业务行为的前提下，将 Local Agent Proposal 相关领域声明从 `AgentGroupChat.swift` 纯移动到独立文件。
- 起始提交：`bd8fdfc953f526fc312fd9b4c5af6164b3720370`
- 代码提交：`5d023f5e9`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentProposal.swift`。
- 将 Creation、Removal、Membership、Team Creation、Project Creation Proposal，以及相关权限与审批结果类型移入新文件。
- `AgentGroupChat.swift` 减少 561 行；新文件包含 562 行（额外一行为独立文件所需的 `Foundation` 导入）。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentProposal.swift`

## 业务不变量

- Proposal 状态 raw value 不变。
- draft/entity 的属性、初始化器、默认值、验证、审批结果和 Codable 契约不变。
- `LocalAgentPermission` 规则不变。
- Store、SQLite、Tool Provider、Scheduler 和 UI 行为不变。
- 除新文件所需的 `Foundation` 导入外，移动声明与原声明逐字一致。

## 验证结果

- 声明移动前后逐字比对：通过（`BYTE_FOR_BYTE_PROPOSAL_DECLARATIONS_MATCH`）。
- Core 公开声明数量：仍为 74。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败：
  - `AgentGroupChatCodableContractTests`
  - `LocalAgentDraftTests`
  - `SQLiteAgentGroupChatStoreTests`
  - `LocalAgentChatToolProviderTests`
- `swift test --package-path clients/macos`：退出码 0；完整 macOS Swift 测试通过，App target 105 个测试中 1 个按设计 opt-in 跳过、0 失败。

## 剩余风险

- 本轮为声明纯移动，风险主要是后续新增类型仍写回聚合文件；需要在后续拆分中保持领域边界。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续阶段 1 的纯拆分：将 Conversation、Room、Message、Unread 领域模型移入独立 Core 文件，并执行定向测试与完整 macOS 测试。
