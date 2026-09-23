# macOS Story Workbench 展示层拆分

- 时间：2026-09-20 10:52:16 CST（Asia/Shanghai）
- 本轮目标：补齐 Story Studio 展示层遗漏，将 1,476 行的 Story Workbench 按剧情概览、分段工作区和公共组件拆分，同时保留并隔离并行布局修复。
- 起始提交：`712c450a39e355b62ea9271406090ff237ca8402`
- 代码提交：`b9931951a3a69054cad167435907c180e499f27b`

## 实际改动

1. 将 `StoryWorkbenchView.swift` 从 1,476 行缩减到提交态 856 行，保留主容器、分段/首尾帧/视频工作区和批处理栏。
2. 将顶部导航、Agent 草稿提示、剧情原文、画面风格、规划卡片和角色/场景画像迁至 `StoryWorkbenchOverview.swift`（587 行）。
3. 将 Story surface modifier 与缩略图组件迁至 `StoryWorkbenchComponents.swift`（43 行）。
4. 拆分前后 54 个 View 属性/方法逐项一致；只放宽跨文件扩展所需的模块内访问级别。
5. 使用独立索引版本提交主文件，未把工作区已有的 ScrollView 循环高度 proposal 修复纳入本轮提交；该并行修改在提交后仍完整保留为未提交 diff。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchView.swift`
- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchOverview.swift`
- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchComponents.swift`

## 业务不变量

- Story 项目导航、设置、暂停、重试和确认弹窗保持不变。
- 剧情原文、风格优化、角色/场景画像与关系图入口保持不变。
- 分段选择、镜头计划、首尾帧、视频生成和批处理逻辑保持不变。
- 没有更改模型请求、媒体扣费确认、持久化 Schema 或任务恢复语义。

## 验证结果

- `swift test --package-path clients/macos --filter StoryStudioTests`：50 项，0 失败。
- `swift test --package-path clients/macos`：通过；主要 XCTest 分组 43、111、105、45 项均 0 失败；Swift Testing 分组 22、98、50 项均通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- 本地化审计：缺失英文 0，缺失/不一致中文 identity 0。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。

## 并行改动与剩余风险

`StoryWorkbenchView.swift` 的 ScrollView 布局修复以及两个 Agent Group Chat ViewModel、`SQLiteAgentGroupChatStoreTests.swift` 的并行修改均继续保持未提交。登录后真实账号关键路径仍需要运行时 Secret，并需避免与用户正在运行的 App 实例争抢本地资源。

## 下一步

完成最终结构热点审计；对 600～900 行文件仅在其保持单一状态机或单一完整渲染职责且有测试覆盖时接受，原方案点名的混合职责大文件不得继续遗留。
