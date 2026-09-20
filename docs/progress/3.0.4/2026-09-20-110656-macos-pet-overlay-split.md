# macOS Pet Overlay 组件拆分

- 时间：2026-09-20 11:06:56 CST（Asia/Shanghai）
- 本轮目标：将 Pet 主视图与窗口控制器中的角色渲染、任务进度、用户问答和窗口辅助职责拆为独立组件，使生产文件回到 900 行门槛以内。
- 起始提交：`9edd7ea20e5ae21b71fd471f09d7399cf745465b`
- 代码提交：`79a428abef43e6fbca9ebed266852671737e2eeb`

## 实际改动

1. `PetOverlayView.swift` 从 1,394 行缩减为 872 行，只保留 Overlay 的主要布局、状态和交互编排。
2. 新增 `PetCharacterView.swift`，承载宠物角色渲染与无障碍文本。
3. 新增 `PetTaskProcessInlineView.swift`，承载任务进度、阶段和步骤展示。
4. 新增 `PetAskUserInlineView.swift`，承载用户问答、选项和提交交互。
5. `PetOverlayWindowController.swift` 从 1,101 行缩减为 882 行。
6. 新增 `PetOverlayWindowSupport.swift`，承载窗口级辅助类型和支持逻辑。
7. 拆分前后声明数量一致：`PetOverlayView` 相关声明 66/66，窗口控制器相关声明 53/53。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/Pet/PetOverlayView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Pet/PetCharacterView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Pet/PetTaskProcessInlineView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Pet/PetAskUserInlineView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Pet/PetOverlayWindowController.swift`
- `clients/macos/Sources/ChatOSApp/Features/Pet/PetOverlayWindowSupport.swift`

## 业务不变量

- 宠物角色渲染、拖动、悬停与无障碍语义不变。
- 任务阶段、步骤、审批与取消交互不变。
- 用户问答的单选、多选、自由输入和提交行为不变。
- 窗口层级、定位、显示/隐藏及事件转发行为不变。
- 未修改 Pet 状态模型、持久化格式、Agent 请求或网络协议。

## 验证结果

- `swift test --package-path clients/macos --filter Pet`：通过。
- `swift test --package-path clients/macos`：通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。
- `git diff --cached --check`：通过。

## 并行改动与剩余风险

两个 Agent Group Chat ViewModel、两个 Story Workbench 布局文件及 `SQLiteAgentGroupChatStoreTests.swift` 的并行修改均保持未提交，没有进入本轮代码提交。登录后真实账号关键路径仍受运行时 Secret 与用户 App 实例占用阻塞。

## 下一步

继续处理仍超过 900 行的 `ChatOSMediaGenerationService.swift` 和 `NativeLocalConnectorService.swift`，随后重新执行 macOS 生产文件体量审计和两份方案的完成定义复核。
