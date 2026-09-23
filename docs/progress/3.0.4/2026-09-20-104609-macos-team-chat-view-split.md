# macOS 团队聊天消息区与成员区拆分

- 时间：2026-09-20 10:46:09 CST（Asia/Shanghai）
- 本轮目标：将仍混合团队主容器、聊天时间线、输入框和成员管理的 `ProjectAgentGroupChatView.swift` 按 UI 职责继续拆分。
- 起始提交：`fe7cea6585b79409bd023fb5143cf7ac9912555d`
- 代码提交：`531d9b620d897dfe65153b8d6a6b9dfa09ce3236`

## 实际改动

1. 将 `ProjectAgentGroupChatView.swift` 从 724 行缩减到 446 行，保留团队工作区主容器、导航、提案和顶部状态。
2. 将聊天时间线、消息气泡、附件展示、提及候选和输入框迁至 `ProjectAgentGroupChatMessages.swift`。
3. 将成员列表、Agent/项目经理标记、运行筛选和成员操作迁至 `ProjectAgentGroupChatMembers.swift`。
4. 拆分前后 15 个 View 属性/方法逐项一致；只放宽扩展所需的类型内访问级别，没有改变用户可见 API。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/ProjectAgentGroupChatView.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/ProjectAgentGroupChatMessages.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/ProjectAgentGroupChatMembers.swift`

## 业务不变量

- 团队 Chat/Todo/Assets/Runs 导航和所选 Agent 保持逻辑不变。
- 提案审批、停止全部 Agent、成员编辑和项目经理显示不变。
- 消息分页、附件展示、提及选择和发送动作不变。
- SwiftUI sheet、alert、confirmation dialog 和任务生命周期不变。

## 验证结果

- `swift test --package-path clients/macos --filter AgentGroupChatViewModel`：编译通过；性能基准测试按环境开关预期跳过。
- `swift test --package-path clients/macos`：通过；主要 XCTest 分组 43、111、105、45 项均 0 失败；Swift Testing 分组 22、98、50 项均通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- 本地化审计：缺失英文 0，缺失/不一致中文 identity 0。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。

## 并行改动与剩余风险

两个 Agent Group Chat ViewModel、`StoryWorkbenchView.swift` 和 `SQLiteAgentGroupChatStoreTests.swift` 的既有并行修改继续保持未提交。本轮未触碰这些修改。登录后真实账号关键路径仍受运行时 Secret 与用户实例占用阻塞。

## 下一步

完成原方案 macOS 热点清单复审，明确区分可接受的 600～900 行单一状态/渲染单元与仍然混合职责的文件；任何超过 900 行的 Story/Pet 等文件若属于方案范围，必须先避开或解决并行修改所有权再拆分。
