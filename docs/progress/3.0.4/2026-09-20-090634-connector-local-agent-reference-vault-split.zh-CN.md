# 3.0.4 进度：Local Agent Run Reference Vault 拆分

## 本轮目标

从 `LocalAgentChatToolProvider.swift` 独立 Run 级 opaque reference 与文档草稿/发送幂等权威状态。

## 起始提交

- `208a97a5c`

## 实际改动

- 新增 `LocalAgentRunReferenceVault.swift`。
- 原样迁移 conversation、message、Todo、team、Agent、assignee、plugin、attachment、team asset 与 document reference 映射。
- 原样迁移文档大小/数量配额、SHA-256 完整性验证、发送 reservation/consume/release 和 call ID receipt 幂等记录。
- Vault 从文件私有调整为模块内部可见，仅供独立 Provider 文件引用，没有新增 public API。
- Provider 文件从 4,047 行降至 3,733 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentRunReferenceVault.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`

## 业务不变量

- 所有 opaque reference 前缀、复用条件与真实 ID 隔离规则不变。
- 文档 UTF-8 数据、单文档/单 Run 配额和 0600 文件权限不变。
- 文档发送前仍重新读取并验证大小和 SHA-256；消费后仍删除临时文件。
- 同一个 call ID 的相同签名仍重放结果，不同签名仍冲突。
- Vault 释放时仍清理整个 Run 草稿目录。
- 工具名称、Schema、错误码、响应 JSON 和权限行为不变。

## 验证结果

- `LocalAgentChatToolProviderTests`：5 个通过。
- `LocalAgentGroupChatSchedulerTests`：12 个通过。
- Store 性能基准：通过，冻结 prepared statement 计数保持 `1008 / 42 / 1 / 500 / 103`，空闲场景 0 写入。
- `swift test --package-path clients/macos`：全量通过；数据量基准按预期在未设置环境变量时跳过。
- 全量测试产生的两个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `c39440fcf974902c7eb1120ef54497adc54934ea` `refactor(connector): extract local agent reference vault`

## 剩余风险

- Provider 仍包含工具注册、领域执行、参数解析与响应 DTO，阶段 2 继续拆分。
- Vault 的模块内部可见性是跨文件所需的最小范围，没有暴露到包外。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 将工具定义/注册从执行 Provider 中独立，使用 golden tests 验证工具名称和 JSON Schema 逐字兼容。
