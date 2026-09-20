# macOS Local Agent Todo 与团队资产工具组拆分

- 时间：2026-09-20 09:26:26 CST（Asia/Shanghai）
- 本轮目标：将 `LocalAgentChatToolProvider` 中 Todo 与团队资产工具实现迁移到独立领域扩展，保持工具协议和运行语义不变。
- 起始提交：`374e9a37d43dd4a94ddcef4f8258c9dfba0655bc`
- 代码提交：`7730f194aea2ed76436c6c9914163e3d56071ab4`

## 实际改动

- 新增 `LocalAgentChatTodoTools.swift`，承载 Todo 查询、调度、创建、更新、依赖、排序、进度、完成/阻塞，以及团队资产查询、维护和归档实现。
- 从 `LocalAgentChatToolProvider.swift` 删除同一实现块；主 Provider 继续负责工具分发及其余聊天生命周期逻辑。
- 跨文件方法仅由 `private` 收窄调整为模块内部可见；未改变公开 API、工具名、JSON Schema、错误码或响应字段。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoTools.swift`

## 业务不变量

- Todo 项目经理权限、依赖环校验、执行 lane、取消、完成、阻塞与唤醒语义不变。
- 团队资产成员可见性、项目经理写权限、revision 并发校验与 executor 快照边界不变。
- Store 调用、排序、时间戳、引用保险库和结构化错误内容保持原样。

## 验证结果

- `swift test --package-path clients/macos --filter 'LocalAgentChatToolProviderTests|LocalAgentGroupChatSchedulerTests'`：17 项通过，0 失败。
- `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests`：1 项通过，0 失败。
- 冻结查询计数：workspace snapshot `1008/1008/1008`；recent messages `42/42/42`；image attachment `1/1/1`；idle heartbeat `500/500/500` 且 0 写；idle account drain `103/103/103` 且 0 写。
- `swift test --package-path clients/macos`：退出码 0；XCTest 与 Swift Testing 全部通过。

## 剩余风险与下一步

- 本轮是纯结构迁移，风险主要是后续领域扩展间的内部可见性继续扩大；下一轮继续按职责拆分聊天/收件箱工具组并保持最小依赖。
- 工作区中 3 个既有并行修改未纳入本轮提交：`AgentGroupChatViewModel.swift`、`AgentGroupChatWorkspaceView.swift`、`SQLiteAgentGroupChatStoreTests.swift`。
