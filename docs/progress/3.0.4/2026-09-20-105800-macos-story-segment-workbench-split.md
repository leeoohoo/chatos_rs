# macOS Story 分段工作区拆分

- 时间：2026-09-20 10:58:00 CST（Asia/Shanghai）
- 本轮目标：将上一轮仍有 856 行的 Story 分段工作区继续拆为小型主容器、时间线、详情/帧/视频操作和批处理辅助渲染。
- 起始提交：`1bc087745199742790035ca484af386c74baa394`
- 代码提交：`8a5799b87e78ccc300f30e047161c78bd34da9eb`

## 实际改动

1. `StoryWorkbenchView.swift` 缩减为 129 行，只保留状态、主容器与弹窗/确认流程。
2. 新增 `StoryWorkbenchSegments.swift`，承载分段时间线与选择交互，提交态 114 行。
3. 新增 `StoryWorkbenchSegmentDetail.swift`，承载分段详情、镜头计划、首尾帧和视频操作，提交态 468 行。
4. 新增 `StoryWorkbenchSupport.swift`，承载批处理栏和共享渲染辅助，166 行。
5. 完整 Story Workbench 的 54 个 View 属性/方法在最初拆分前后逐项一致。
6. 已有 ScrollView 循环高度 proposal 修复迁移到两个新文件的工作区版本，但通过独立索引内容排除在本轮提交之外。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchView.swift`
- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchSegments.swift`
- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchSegmentDetail.swift`
- `clients/macos/Sources/ChatOSApp/Features/MediaStudio/Story/StoryWorkbenchSupport.swift`

## 业务不变量

- 分段选择、排序、编辑、删除与连续性重新规划语义不变。
- 首帧、尾帧、视频引导模式、重试和扣费确认语义不变。
- 批处理、暂停、全剧连播和项目保存语义不变。
- 没有修改媒体请求、持久化格式或任务恢复行为。

## 验证结果

- `swift test --package-path clients/macos --filter StoryStudioTests`：50 项，0 失败。
- `swift test --package-path clients/macos`：通过；主要 XCTest 分组 43、111、105、45 项均 0 失败；Swift Testing 分组 22、98、50 项均通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- 本地化审计：缺失英文 0，缺失/不一致中文 identity 0。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。

## 并行改动与剩余风险

原 `StoryWorkbenchView.swift` 的并行布局修复现完整保留在 `StoryWorkbenchSegments.swift` 与 `StoryWorkbenchSegmentDetail.swift` 的未提交 diff 中。两个 Agent Group Chat ViewModel 和 `SQLiteAgentGroupChatStoreTests.swift` 的并行修改也保持未提交。登录后真实账号关键路径仍受运行时 Secret 与用户实例占用阻塞。

## 下一步

执行最终 macOS 热点清单与文件体量审计，确认原方案点名的混合职责文件均已拆分，并归档仍然属于外部验收条件的项目。
