# 3.0.4 进度：Local Agent Tool Registry 拆分

## 本轮目标

从执行 Provider 中独立静态工具注册表和 JSON Schema，保持工具契约逐字兼容。

## 起始提交

- `e7bf8bb3a`

## 实际改动

- 新增 `LocalAgentChatToolRegistry.swift`。
- 将 35 个静态工具定义、描述、Schema 和 effect 原样迁入 `LocalAgentChatToolProvider` 扩展。
- 动态职业目录驱动的 `memberProposalDefinition()` 仍留在 Provider 实例侧。
- `toolDefinitions` 从文件私有调整为模块内部可见，仅供 Provider 主文件读取，没有新增 public API。
- Provider 主文件从 3,733 行降至 3,514 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolRegistry.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`

## 业务不变量

- 工具名常量、定义顺序、描述、JSON Schema、required/additionalProperties 和 effect 不变。
- 消息与文档上限仍从 `AgentCommunicationPolicy.standard` 单一来源插值。
- manager/executor/项目经理权限过滤与动态职业 Schema 不变。
- 工具执行分发、参数解析、错误码和响应 JSON 不变。

## 验证结果

- `LocalAgentChatToolProviderTests`：5 个通过。
- `LocalAgentGroupChatSchedulerTests`：12 个通过。
- Store 性能基准：通过，冻结 prepared statement 计数保持 `1008 / 42 / 1 / 500 / 103`，空闲场景 0 写入。
- 首次全量测试的尾部汇总出现测试框架提示，但命令状态无法单独证明；随后重新捕获完整日志与退出码：`FULL_STATUS=0`，所有 XCTest 汇总均为 0 failures，Swift Testing suites 全绿。
- `swift test --package-path clients/macos`：确认全量通过；数据量基准按预期在未设置环境变量时跳过。
- 测试产生的四个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `7bd0a6e13e32aa168f8be24f6167e6bb7bde1553` `refactor(connector): extract local agent tool registry`

## 剩余风险

- Provider 仍包含多个领域工具组、参数解析和响应 DTO，阶段 2 尚未完成。
- Registry 的模块内部可见性是跨文件扩展所需的最小范围。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 独立 Provider 的响应 DTO 与通用参数/结果编解码，随后按 Chat、Todo、Proposal、Asset 工具组拆分执行函数。
