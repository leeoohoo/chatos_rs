# macOS Media Studio 工作区拆分

- 时间：2026-09-20 09:44:16 CST（Asia/Shanghai）
- 本轮目标：按图片、视频、历史与播放预览职责拆分 Media Studio 大视图。
- 起始提交：`ed5e971be53686157109a1a333ed954a4a66d241`
- 代码提交：`c45dd71283625f8d4172fb5d5ef356b511d67006`

## 实际改动

- `MediaStudioView.swift` 从 1,633 行缩减至 162 行，保留顶层导航、Sheet 生命周期和工作区路由。
- 新增图片生成工作区、视频生成工作区、历史工作区和播放器/预览组件文件。
- 跨文件需要的视图状态与子视图从文件私有调整为模块内部可见；生成和展示实现保持原样。

## 业务不变量

- 图片/视频/剧情/历史四个入口、表单字段、模型选择、参考图和生成按钮行为不变。
- 图片与视频历史、剧情批次预览、播放器及登录切换时清理 Sheet 的行为不变。
- 未改变网络请求、生成参数、缓存、文件读取或 UI 文案。

## 验证结果

- macOS Swift build：通过。
- Store 冻结基准查询计数保持冻结值，空闲场景 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- Media Studio 主视图职责已收敛；下一步处理 Story Studio ViewModel 和 AppModel。
- 未纳入并行修改：Agent Group Chat 两个 UI 文件、`StoryWorkbenchView.swift`、`SQLiteAgentGroupChatStoreTests.swift`。
