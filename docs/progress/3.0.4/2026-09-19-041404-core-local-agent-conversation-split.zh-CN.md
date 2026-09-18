# 3.0.4 进度：拆分 Local Agent Conversation 领域模型

- 时间：2026-09-19 04:14:04（Asia/Shanghai）
- 本轮目标：在不改变业务行为的前提下，将会话、房间、成员、消息、附件和未读游标相关声明从 `AgentGroupChat.swift` 纯移动到独立文件。
- 起始提交：`59f4f1b0d`
- 代码提交：`1331673dd`

## 实际改动

- 新建 `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentConversation.swift`。
- 移入 Room、Room Member、Conversation Kind、Message、Message Attachment、Message Page、Read Cursor、Unread Page 与 Unread Conversation 类型。
- `AgentGroupChat.swift` 减少 509 行；新文件包含 510 行（额外一行为独立文件所需的 `Foundation` 导入）。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentConversation.swift`

## 业务不变量

- 房间、成员、消息和附件的属性、初始化器、默认值与验证逻辑不变。
- 会话类型、状态与发送者类型 raw value 不变。
- 消息附件的 Codable 兼容默认值与字段集合不变。
- 分页游标、未读游标的单调性语义和账号/房间隔离不变。
- Store、SQLite、Tool Provider、Scheduler 和 UI 行为不变。
- 除新文件所需的 `Foundation` 导入外，移动声明与原声明逐字一致。

## 验证结果

- 声明移动前后逐字比对：通过（`BYTE_FOR_BYTE_CONVERSATION_DECLARATIONS_MATCH`）。
- AgentGroupChat 领域公开声明数量：仍为 74。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败：
  - `AgentGroupChatCodableContractTests`
  - `LocalAgentDraftTests`
  - `SQLiteAgentGroupChatStoreTests`
  - `LocalAgentChatToolProviderTests`
- `swift test --package-path clients/macos`：退出码 0；完整 macOS Swift 测试通过，App target 105 个测试中 1 个按设计 opt-in 跳过、0 失败。

## 剩余风险

- 本轮为声明纯移动；后续新增会话模型应继续放入独立领域文件，避免聚合文件重新膨胀。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续阶段 1 的纯拆分：将 Todo 与 Asset 领域模型移入独立 Core 文件，并执行定向测试与完整 macOS 测试。
