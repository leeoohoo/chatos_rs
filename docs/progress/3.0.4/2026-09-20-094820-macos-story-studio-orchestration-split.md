# macOS Story Studio 编排拆分

- 时间：2026-09-20 09:48:20 CST（Asia/Shanghai）
- 本轮目标：按素材、帧、视频、Agent 规划和媒体批处理拆分 Story Studio ViewModel。
- 起始提交：`e10cc1aa61add53b61a8457551b4c69360cb33c7`
- 代码提交：`43373e3d8e78054178cd01c92d92383d927aafc6`

## 实际改动

- `StoryStudioViewModel.swift` 从 1,798 行缩减至 639 行，保留状态、项目编辑和基础生命周期。
- 新增素材生成、首末帧生成、视频生成、Agent 规划和媒体批处理五个领域扩展。
- 为跨文件扩展将内部状态和辅助方法从文件私有调整为模块内部可见；未改写执行流程。

## 业务不变量

- 项目持久化、generation attempt、确认/重试、任务取消和 session token 校验不变。
- 视频并发、进度、末帧提取、Agent checkpoint/恢复和媒体批次状态不变。
- 模型能力校验、文件安全、Story continuity 和历史记录行为不变。

## 验证结果

- macOS Swift build：通过。
- Store 冻结基准查询计数保持冻结值，空闲场景 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- Story Studio ViewModel 已进入建议行数范围；下一步收敛 `AppModel` 组合根与长生命周期职责。
- 未纳入并行修改：Agent Group Chat 两个 UI 文件、`StoryWorkbenchView.swift`、`SQLiteAgentGroupChatStoreTests.swift`。
