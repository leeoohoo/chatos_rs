# 3.0.4 进度：Local Agent Proposal Tool 拆分

## 本轮目标

将 Agent 创建、成员邀请、成员移除提案和相关权限判定从主 Provider 独立为 Proposal 工具组。

## 起始提交

- `01f270e02`

## 实际改动

- 新增 `LocalAgentChatProposalTools.swift`。
- 迁移 `proposeMember`、`proposeExistingMember`、`proposeMemberRemoval`。
- 迁移人员管理、项目经理和团队成员权限辅助判断。
- Provider 的 9 个依赖属性从文件私有调整为模块内部只读，以支持领域扩展；没有新增 public API，也没有暴露可写共享状态。
- Provider 主文件从 2,842 行降至 2,661 行。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatProposalTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`

## 业务不变量

- 人员管理 Skill、当前 Agent、room/delivery 与项目经理权限检查不变。
- 提案 request key、真实 ID 解析、Human 审批边界和错误文案不变。
- 新 Agent 的模型配置继承、thinking level 与 profession 校验不变。
- 现有 Agent 邀请的 team/agent opaque reference 校验和结构化失败不变。
- Store、Context、Vault 与回调仍为初始化后只读依赖。

## 验证结果

- `LocalAgentChatToolProviderTests`：5 个通过，包含人员权限与提案路径。
- `LocalAgentGroupChatSchedulerTests`：12 个通过。
- Store 性能基准：通过，冻结 prepared statement 计数保持 `1008 / 42 / 1 / 500 / 103`，空闲场景 0 写入。
- `swift test --package-path clients/macos`：`FULL_STATUS=0`；主要 XCTest target 均为 0 failures，Swift Testing 98/50 测试 suites 通过。
- 数据量基准按预期在普通全量测试中跳过。
- 全量测试产生的两个孤立 fixture shell 进程已按精确 PID 核对并清理。

## 代码提交

- `c56622de992a680320ffd963e93aaf03aee2b3ca` `refactor(connector): extract local agent proposal tools`

## 剩余风险

- Provider 仍有 2,661 行，Chat、Todo、Asset 与发送生命周期函数尚待拆分。
- 模块内部只读依赖是跨文件领域扩展所需的最小范围；包外调用方仍不可见。
- 工作区中 3 个既有用户并行修改文件未纳入本轮提交。

## 下一步

- 独立 Todo 与 Team Asset 工具组，再拆 Chat/Inbox/Document/发送工具组。
