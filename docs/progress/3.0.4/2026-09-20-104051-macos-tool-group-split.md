# macOS Messaging 与 Todo 工具组细分

- 时间：2026-09-20 10:40:51 CST（Asia/Shanghai）
- 本轮目标：重新审计方案结构门槛后，将仍超过 600 行的 Messaging 与 Todo 工具组按职责继续拆分。
- 起始提交：`7f40b1872f93010d46c1c4e2ac1f1e3497721fad`
- 代码提交：`b509b6b881b705db7b28a44f596c5943010fa276`

## 实际改动

1. 将 722 行的 `LocalAgentChatMessagingTools.swift` 拆为消息读取、Markdown 文档、私聊、发送、完成/回执与通信指标文件。
2. 将 669 行的 `LocalAgentChatTodoTools.swift` 拆为 Todo 查询/选项、创建、更新和排序文件。
3. 拆分后相关工具文件最大 420 行；Messaging 的 16 个方法和 Todo 的 8 个方法在拆分前后逐项一致。
4. 本轮只移动现有实现，没有修改工具名、JSON Schema、参数解析、错误码、中文错误文本、响应 DTO、权限检查或调用顺序。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatMessagingTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatDocumentTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatDirectMessageTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatSendTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatMessageCompletionTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoCreateTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoUpdateTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoReorderTools.swift`

## 业务不变量

- 消息引用、附件读取、文档草稿完整性校验与发送幂等回执不变。
- Human/Agent/团队房间权限与 Agent 引用隔离不变。
- Todo 的项目经理权限、依赖校验、ready 入队与团队排序语义不变。
- 没有性能行为修改，也没有引入新共享状态。

## 验证结果

- `swift test --package-path clients/macos --filter LocalAgentChatToolProvider`：5 项，0 失败。
- `swift test --package-path clients/macos`：通过；主要 XCTest 分组 43、111、105、45 项均 0 失败；Swift Testing 分组 22、98、50 项均通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- 本地化审计：缺失英文 0，缺失/不一致中文 identity 0。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。

## 并行改动与剩余风险

4 份既有并行改动继续保持未提交：两个 Agent Group Chat ViewModel、`StoryWorkbenchView.swift` 和 `SQLiteAgentGroupChatStoreTests.swift`。登录后真实账号关键路径仍需要运行时 Secret，且必须避免与当前用户实例争抢本地运行时资源。

## 下一步

继续按原方案热点清单审计 macOS 文件的实际职责；600～900 行的单一领域文件按职责而非机械行数判定，超过 900 行或继续混合多个领域的文件不得标记完成。
