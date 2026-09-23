# 3.0.4 进度：Local Agent Tool Codec 拆分

## 本轮目标

从执行 Provider 中独立无状态的参数解析、消息/文档校验、结果编码和错误映射辅助函数。

## 起始提交

- `0b5e5d287`

## 实际改动

- 新增 `LocalAgentChatToolCodec.swift`。
- 迁移 JSON arguments、必填/可选字符串、整数、布尔值、字符串数组和对象数组解析。
- 迁移消息长度失败、Markdown 文件名清洗、Encodable 结果编码、结构化失败与 Store 错误映射。
- 辅助函数从文件私有调整为模块内部的 Provider 静态方法，仅供 Provider 扩展调用，没有新增 public API。
- 保留需要访问 Store、Context、Vault 或指标记录的实例函数在主 Provider。
- Provider 主文件从 2,993 行降至 2,842 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolCodec.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`

## 业务不变量

- JSON 类型判定、NSNumber/Bool 区分、整数精度校验和字段错误名不变。
- 2,000 字消息上限、`message_too_long` 结构化引导和 `next_tool` 不变。
- Markdown 文件名清洗、`.md` 补齐和 240 字符限制不变。
- 响应 JSON 仍使用 sorted keys 与不转义斜杠。
- Store 错误码和 invalid field 映射不变。

## 验证结果

- `LocalAgentChatToolProviderTests`：5 个通过。
- `LocalAgentGroupChatSchedulerTests`：12 个通过。
- Store 性能基准：通过，冻结 prepared statement 计数保持 `1008 / 42 / 1 / 500 / 103`，空闲场景 0 写入。
- `swift test --package-path clients/macos`：`FULL_STATUS=0`；主要 XCTest target 均为 0 failures，Swift Testing 98/50 测试 suites 通过。
- 数据量基准按预期在普通全量测试中跳过。
- 全量测试产生的两个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `40467d9684d718249f724c0ee8210fa593eec6f7` `refactor(connector): extract local agent tool codec`

## 剩余风险

- Provider 仍有 2,842 行，主要由 Chat、Todo、Proposal 和 Asset 工具执行函数构成。
- Codec 方法的模块内部可见性是跨文件扩展所需的最小范围。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 按领域将 Chat/Inbox/Document、Todo/Asset、Proposal 工具执行函数移动到独立 Provider 扩展文件，保持执行 switch 与权限过滤不变。
