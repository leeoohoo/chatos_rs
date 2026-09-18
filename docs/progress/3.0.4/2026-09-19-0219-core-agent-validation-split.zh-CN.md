# 3.0.4 推进记录：Core Agent 错误与校验拆分

- 时间：2026-09-19 02:19:46 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 1——macOS Core 与持久化纯拆分。
- 本轮目标：从 `AgentGroupChat.swift` 纯移动错误类型与通用校验，不改变任何业务或公共 API。
- 起始提交：`653fb78a7`
- 代码提交：`ba730cdfd`

## 实际改动

1. 新建 `ChatOSCore/AgentGroupChat/AgentGroupChatValidation.swift`。
2. 将 `AgentGroupChatError` 和 `AgentGroupChatValidation` 从 2,586 行的聚合文件移入独立文件。
3. 原文件减少 61 行；新文件 62 行，其中新增内容只有独立编译单元所需的 `Foundation` import。
4. 用基线提交原文与新文件声明区做 diff，确认两个声明逐字一致。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/AgentGroupChatValidation.swift`

## 业务不变量

- `AgentGroupChatError` 的 case、关联值、协议遵循和中文错误文本不变。
- identifier、identifiers、text、optionalText、timestamps 的签名、长度限制、trim、控制字符和时间规则不变。
- 不修改 Codable、Store protocol、SQLite、Scheduler、Tool Provider 或 UI。
- Core 公开领域声明仍为 74 个，Store protocol 方法仍为 76 个。
- 工作区三处用户并行改动未被暂存、提交或覆盖。

## 验证结果

1. 原声明与新文件声明区逐字 diff 为空。
2. `swift test --package-path clients/macos --filter AgentGroupChatCodableContractTests` 通过：5 个测试、0 失败。
3. `swift test --package-path clients/macos` 完整通过；App target 105 个测试中 1 个 opt-in 基线按设计跳过，其余 targets 无失败。
4. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 本轮只完成第一个 61 行纯移动，`AgentGroupChat.swift` 仍包含 Profile、Proposal、Conversation、Todo、Asset、Delivery、Run 和 Store contract。
- 新增 Swift 文件触发了一次完整增量重编译，耗时明显高于普通测试；这是构建缓存重建，不是运行时性能结论。
- 下一轮继续按依赖顺序拆分 Profile/Builder Draft 领域声明；仍只移动代码，并用 Codable contract 与完整 macOS 测试守住兼容性。
