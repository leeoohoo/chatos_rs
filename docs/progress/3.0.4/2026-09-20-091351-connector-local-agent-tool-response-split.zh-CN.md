# 3.0.4 进度：Local Agent Tool Response 拆分

## 本轮目标

从工具执行 Provider 中独立所有响应 DTO 与文档草稿解析结果类型，保持响应 JSON key 和编码行为不变。

## 起始提交

- `f578a0011`

## 实际改动

- 新增 `LocalAgentChatToolResponses.swift`。
- 将 Chat、Inbox、Workspace、Todo、Team Asset、Proposal、Direct Message、Send 和结构化错误响应模型原样迁入 Provider 扩展。
- 将 `DocumentDraftResolution` 与响应模型一并迁移。
- 类型从文件私有调整为模块内部的 Provider 嵌套类型，仅供 Provider 各扩展使用，没有新增 public API。
- Provider 主文件从 3,514 行降至 2,993 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolResponses.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`

## 业务不变量

- 所有响应属性、可空性、CodingKeys 和 snake_case JSON 字段不变。
- Todo progress 映射和结构化失败 `ok/error/next_tool` 格式不变。
- 文档草稿 ready/failure 分支及附件 draft 类型不变。
- 工具定义、执行路由、权限、参数解析和错误映射不变。

## 验证结果

- `LocalAgentChatToolProviderTests`：5 个通过。
- `LocalAgentGroupChatSchedulerTests`：12 个通过。
- Store 性能基准：通过，冻结 prepared statement 计数保持 `1008 / 42 / 1 / 500 / 103`，空闲场景 0 写入。
- `swift test --package-path clients/macos`：`FULL_STATUS=0`；主要 XCTest target 均为 0 failures，Swift Testing 98/50 测试 suites 通过。
- 数据量基准按预期在普通全量测试中跳过。
- 全量测试产生的两个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `f58b30292f485d3949b9e80f907b668aa3b3157f` `refactor(connector): extract local agent tool responses`

## 剩余风险

- Provider 仍有 2,993 行，领域执行函数和参数/结果编解码尚待拆分。
- 响应嵌套类型的模块内部可见性是跨文件扩展所需的最小范围。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 独立通用参数解析、结构化错误与结果编码辅助函数，再按 Chat/Proposal/Todo/Asset 工具组拆分执行方法。
