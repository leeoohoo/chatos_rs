# 3.0.4 推进记录：Agent Core Codable 契约基线

- 时间：2026-09-18 23:10:48 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：在拆分 `AgentGroupChat.swift` 前，用测试冻结核心领域模型的序列化字段、枚举 raw value、兼容默认值和错误文案。
- 起始提交：`b973a920b`
- 代码提交：`bb809608c`

## 实际改动

1. 新增 5 组 Core characterization tests，覆盖 Profile、团队/私聊、消息、远端同步附件、Todo、执行计划、团队资产和 Delivery。
2. 对代表性完整对象同时断言顶层 JSON key 集合和 Codable round-trip 等价，嵌套 Profile draft、Todo execution contract/plan 也单独冻结字段集合。
3. 冻结会写入持久化或跨层传递的枚举 raw value，包括会话类型、Todo 状态、团队资产类别、Delivery trigger 和附件同步状态。
4. 冻结 `AgentGroupChatError` 六类本地化文案。
5. 增加旧版 Profile draft、旧版附件和空 Todo execution contract 的解码测试，确认新增字段仍按既有默认值兼容。

## 涉及文件

- `clients/macos/Tests/ChatOSCoreTests/AgentGroupChatCodableContractTests.swift`

## 业务不变量

- 本轮只新增测试，不修改 Core 生产类型、初始化器、CodingKeys、校验、Store 或 UI。
- 测试固定的是当前公开/持久化契约，不引入新的字段、默认值或错误文案。
- 不读取真实账号、数据库或文件；所有 ID 和内容均为内存中的虚构 fixture。
- 工作区中原有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证结果

1. `swift test --package-path clients/macos --filter AgentGroupChatCodableContractTests` 通过：5 项测试、0 失败。
2. `swift test --package-path clients/macos` 全量通过；Core 现在包含新增的 5 项契约测试，其他测试无失败。
3. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 当前已冻结主要聚合根，但不逐个快照 74 个公开声明；较小 DTO 继续由编译器和现有 Store/Tool 测试覆盖。
- 阶段 0 仍缺 App 主线程长任务和空闲 CPU 的可重复证据；受并行 ViewModel 改动影响，当前自动任务继续避让相关文件。
- 若后续 Core 纯拆分导致任一 key、raw value、默认值或错误文本漂移，本测试应阻止提交；不得为了通过测试直接改快照，除非有独立业务授权。
