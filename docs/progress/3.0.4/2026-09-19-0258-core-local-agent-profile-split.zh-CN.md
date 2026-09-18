# 3.0.4 推进记录：Core Local Agent Profile 拆分

- 时间：2026-09-19 02:58:25 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 1——macOS Core 与持久化纯拆分。
- 本轮目标：把 Local Agent Profile、Builder Draft 与相关资源模型移出领域聚合文件，保持声明逐字兼容。
- 起始提交：`2373993d1`
- 代码提交：`124828277`

## 实际改动

1. 新建 `ChatOSCore/AgentGroupChat/LocalAgentProfile.swift`。
2. 纯移动 Profile status/draft/entity、Builder 产生的 `LocalAgentDraft`、模型/插件选项、Thinking Level catalog、Builder resources 和仅供这些类型使用的 `String.nilIfEmpty`。
3. `AgentGroupChat.swift` 减少 327 行，由 2,525 行降至 2,198 行；新文件为 328 行，其中新增内容只有独立编译单元需要的 `Foundation` import。
4. 用起始提交中的两个原始声明区与新文件声明区做 diff，确认全部移动内容逐字一致。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentProfile.swift`

## 业务不变量

- 不改变 Profile、Draft、Builder 资源的类型名、属性、初始化器、默认值、验证、CodingKeys 或旧数据解码。
- 不改变 Thinking Level 的 provider 归一化、允许值和回退规则。
- 不改变 `LocalAgentDraft` 到 profile/member draft 的映射或创建权限边界。
- Core 公开领域声明仍为 74 个，Store protocol 方法仍为 76 个。
- 工作区三处用户并行改动未被暂存、提交或覆盖。

## 验证结果

1. 起始提交原声明与新文件声明区逐字 diff 为空。
2. `swift test --package-path clients/macos --filter '(AgentGroupChatCodableContractTests|LocalAgentDraftTests|LocalAgentBuilderToolProviderTests)'` 通过：10 个测试、0 失败。
3. `swift test --package-path clients/macos` 完整通过；App target 105 个测试中 1 个 opt-in 基线按设计跳过，其余 targets 无失败。
4. `git diff --cached --check` 通过。

## 剩余风险与下一步

- `LocalAgentProfile.swift` 现在是 328 行单一 Profile/Builder 模型边界，低于阶段 1 的普通文件目标。
- `AgentGroupChat.swift` 仍有 2,198 行，下一轮按计划纯移动 Creation、Removal、Membership、Team 和 Project proposal 类型到 `LocalAgentProposal.swift`。
- Proposal 拆分必须继续保持状态 raw value、审批结果、权限检查、验证错误和 Codable 兼容，不顺带调整模型结构。
