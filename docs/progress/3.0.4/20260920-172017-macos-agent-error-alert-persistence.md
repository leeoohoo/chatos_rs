# Mac Agent 错误弹窗持久显示修复

- 时间：2026-09-20 17:20:17 +0800（Asia/Shanghai）
- 本轮目标：诊断“运行历史或记忆范围不一致”提示，并修复 Agent 错误弹窗被后台刷新自动关闭的问题。
- 起始提交：`6871835aa429e2ab1beb5013f8b427c428a8e516`
- 代码提交：`5b8e18403d6e350bcac9b1565334ce49b292ae8e`

## 诊断证据

- 客户端进程在用户报告后仍然存在，未发现新的 macOS 崩溃报告；自动消失的是 SwiftUI 错误弹窗，不是应用退出。
- 数据库中唯一包含“运行历史或记忆范围不一致”的 Run 为 `8717cc7d-b6cd-4bff-b92a-2f4b09bb9710`：它在同步第 6 条 Memory 记录时触发完整性保护，共留下 33 次 `context_paused`；同一 Run 后续在不丢弃记录、不重放副作用的前提下完成同步，并最终以 46 条消息、19 次模型调用正常完成。
- 新安装后的活跃 Run 持续处于 `running`，近期其他 Run 均为 `completed`，没有当前未处理的 Memory 不一致暂停。
- 两个 Agent 页面 ViewModel 的被动 `load()` 成功路径都会无条件执行 `errorMessage = nil`。运行状态更新会触发自动刷新，因此刚出现的错误弹窗会在约 120 ms 后被刷新关闭。

## 实际改动

- Agent 总工作区的后台房间/投递刷新不再清除当前错误。
- 单个团队页面的后台刷新不再清除当前错误。
- 错误现在只会由用户点击弹窗按钮明确关闭，或由后续明确的成功用户操作按原有逻辑清除。
- 新增两条回归测试，分别覆盖总工作区与团队页：设置运行错误后执行成功刷新，错误内容必须保持不变。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentGroupChatWorkspaceViewModel.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentGroupChatViewModel.swift`
- `clients/macos/Tests/ChatOSAppTests/AgentGroupChatErrorPresentationTests.swift`

## 业务不变量

- 后台刷新仍正常更新房间、成员、消息、任务、运行与投递状态。
- Memory 完整性保护保持不变：不一致时仍暂停，不丢记录，也不自动重放副作用工具。
- 本轮没有修改或清理用户的 Agent、聊天、Todo、Run 或 Memory 数据。
- 用户或其他进程已存在于两个 ViewModel 文件中的并行性能改动未被本轮暂存或提交。

## 验证结果

- `swift test --package-path clients/macos --filter AgentGroupChatErrorPresentationTests`：2 个测试通过，0 失败。
- `make test-macos-client`：全量 Mac Swift 测试通过；ChatOSAppTests 为 109 个测试、1 个跳过、0 失败，其余目标 0 失败。
- `clients/macos/scripts/package-debug-app.sh`：构建、资源复制与签名成功。
- `codesign --verify --deep --strict /Applications/ChatOS.app`：通过。
- 安装后二进制与构建产物一致，新客户端进程已启动。
- 旧应用备份：`/Applications/ChatOS.app.before-persistent-agent-errors-20260920-171759`。

## 剩余风险

- 历史 Run 中的完整性保护事件仍会保留在运行时间线中，这是审计记录，不应删除。
- 如果未来再次出现新的 Memory 不一致，新版会保持弹窗直到用户确认；应根据新 Run 的同步阶段继续定位，而不会再因界面刷新丢失错误文本。
- 工作树仍含用户或其他进程的并行修改，本轮只以交互式暂存提交了错误保留相关的两个小 hunk 与新测试文件。

## 下一步

- 继续观察当前活跃 Run；若再次出现 Memory 提示，记录新 Run ID、同步记录序号和错误持续时间，区分远端暂态确认与真实不可变记录冲突。
- 用户可以继续测试；任何新错误弹窗现在都应保持显示，直到主动点击“好”。
