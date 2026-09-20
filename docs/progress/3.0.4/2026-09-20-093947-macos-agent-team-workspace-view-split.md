# macOS Agent 团队工作区视图拆分

- 时间：2026-09-20 09:39:47 CST（Asia/Shanghai）
- 本轮目标：将项目 Agent 团队页的 Todo、资产、Run 和编辑 Sheet 从主视图分离。
- 起始提交：`c74450a718e5eabd6ace99efc87ac7b762651bb6`
- 代码提交：`5e68f66ec9b9163032757522ec19541f713a381b`

## 实际改动

- `ProjectAgentGroupChatView.swift` 从 2,089 行缩减为 724 行，保留团队导航、聊天和成员展示。
- 新增独立 Todo、团队资产、Run/Inspector 和 Agent/团队编辑 Sheet 文件。
- 仅移动顶层 SwiftUI 类型和状态显示扩展；跨文件类型从文件私有调整为模块内部可见。

## 业务不变量

- 团队页的 Chat、Todo、Assets、Runs、Members 导航与交互不变。
- Todo 状态、资产 revision/history、Run checkpoint/Delivery 状态展示不变。
- Agent 编辑、邀请、建队、创建 Agent 和提案确认流程不变。

## 验证结果

- `swift build --package-path clients/macos`：通过。
- Store 冻结基准：查询计数全部保持冻结值，0 空闲写入。
- `swift test --package-path clients/macos`：退出码 0。

## 剩余风险与下一步

- `ProjectAgentGroupChatView` 已进入单一 UI 文件建议范围；下一步拆分 `AppModel` 长生命周期协调职责。
- 未覆盖并行修改：`AgentGroupChatViewModel.swift`、`AgentGroupChatWorkspaceView.swift`、`StoryWorkbenchView.swift`、`SQLiteAgentGroupChatStoreTests.swift`。
