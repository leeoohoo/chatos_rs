# macOS Agent Workspace 管理视图拆分

- 时间：2026-09-20 10:00:31 CST（Asia/Shanghai）
- 本轮目标：将 Agent Workspace 的 ViewModel、管理页与编辑 Sheet 从导航主视图中拆出。
- 起始提交：`55c9ed5409122ae52d2efbd41808478cb82ebaef`
- 代码提交：`b11ceb11f056f40b13d7e91deb8d0e801d089b8e`

## 实际改动

- `AgentGroupChatWorkspaceView.swift` 从 1,591 行缩减至 260 行，只保留导航、列表和详情路由。
- 新增独立 `AgentGroupChatWorkspaceViewModel.swift`、Agent 管理视图和 Profile/建队 Sheet 文件。
- 拆分提交严格基于原行为；测试前已存在的 trigger Run 批量读取并行改动随后原样恢复到新 ViewModel 文件，保持为未提交状态，不纳入本轮代码提交。

## 业务不变量

- Workspace 导航、Agent/私聊/团队选择、编辑、创建和模型加载行为不变。
- Run 恢复/重试/放弃、Store 生命周期和通知观察语义不变。
- 本轮不声明或提交并行性能修改的所有权。

## 验证结果

- macOS Swift build：通过。
- Store 冻结基准查询计数保持冻结值，空闲场景 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- Agent Workspace 结构拆分完成；并行批量读取改动已迁移到新文件但仍未提交，待其所有者或后续获授权单元处理。
- 其余并行修改仍保留：`AgentGroupChatViewModel.swift`、`StoryWorkbenchView.swift`、`SQLiteAgentGroupChatStoreTests.swift`。
