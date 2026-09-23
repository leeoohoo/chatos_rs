# macOS Local Agent 聊天工具组拆分

- 时间：2026-09-20 09:30:27 CST（Asia/Shanghai）
- 本轮目标：完成 `LocalAgentChatToolProvider` 剩余聊天领域方法的纯结构拆分。
- 起始提交：`fd0f8a4ec916cfea3478d7a12fabb1d02017bbcf`
- 代码提交：`2f35b27070d873e15a630c2e7111da298a415152`

## 实际改动

- 新增 `LocalAgentChatInboxTools.swift`，承载 bootstrap、workspace snapshot、触发消息、成员、全局未读和 inbox 回复。
- 新增 `LocalAgentChatMessagingTools.swift`，承载消息/附件读取、文档创建、已读、私聊/团队发送、heartbeat 完成、文档引用解析、幂等发送和沟通指标记录。
- 主 Provider 缩减到工具注册、构造、分发和动态成员提案定义，行数从 1,383 降至 254。
- 迁移方法仅将跨文件所需的 `private` 调整为模块内部可见，未改写实现。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatInboxTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatMessagingTools.swift`

## 业务不变量

- 工具名称、JSON Schema、权限判断、引用作用域、结构化错误码和响应 JSON 不变。
- 文档附件、本地优先发送、幂等重放、消息路由和 heartbeat 完成语义不变。
- 未读标记、消息分页、附件读取和团队/私聊边界不变。

## 验证结果

- Tool Provider 与 Scheduler 定向测试：17 项通过，0 失败。
- Store 冻结基准：1 项通过；查询计数仍为 workspace `1008/1008/1008`、recent messages `42/42/42`、image attachment `1/1/1`、idle heartbeat `500/500/500` 且 0 写、idle account drain `103/103/103` 且 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- Tool Provider 拆分已达到主文件职责收敛目标；下一步拆分 `LocalAgentGroupChatScheduler.swift` 的租约/drain、执行和恢复策略。
- 未纳入并行修改：Agent Group Chat 两个 UI 文件、`SQLiteAgentGroupChatStoreTests.swift`，以及本轮测试期间新出现的 `StoryWorkbenchView.swift`。
