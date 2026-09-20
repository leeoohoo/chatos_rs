# 3.0.4 macOS Agent auto 思考等级与云端文档布局修复

## 本轮目标

- 修复确认 Agent 创建提案时出现“无效的 Agent 群聊字段：thinkingLevel”的错误。
- 修复云端 Agent 文档空状态把标题栏整体推到页面中部、顶部出现大块空白的问题。

## 起始提交

- `e2ddf77606ae5872f9d820742f203b390cdb1f7b`

## 实际改动

- 将 `auto` 纳入所有 Responses 模型供应商的本地思考等级目录。
- 保持 `auto` 的既有语义：不强制指定具体推理强度，由模型配置决定；不是自动换模型或协议兜底。
- 新增目录回归测试，覆盖 GPT、DeepSeek、GLM 与 Kimi 分支。
- 让云端 Agent 文档根视图始终填满工作区并顶部对齐，标题栏不再随空状态的固有高度垂直居中。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentProfile.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentRemoteArtifactLibraryView.swift`
- `clients/macos/Tests/ChatOSCoreTests/LocalAgentDraftTests.swift`

## 业务不变量

- Agent 仍只使用所绑定的模型配置，不因 `auto` 切换模型、供应商或协议。
- 明确选择 `none`、`minimal`、`low`、`medium`、`high`、`xhigh` 或供应商允许的其他等级时行为不变。
- Agent 提案仍需用户确认；本轮视觉验证没有替用户确认或创建真实 Agent。
- 云端文档加载、刷新、预览、另存为和完整性校验逻辑不变。
- 未提交工作区中其他用户或并发进程的改动。

## 验证结果

- `swift test --package-path clients/macos --filter LocalAgentDraftTests`：4 项通过。
- `make test-macos-client`：全量通过；数据量基准按既有环境开关跳过 1 项，其余测试无失败。
- `clients/macos/scripts/package-debug-app.sh`：通过，包含本轮 Core 与 App 重新编译、SDK 一致性检查和签名。
- 新 App 已安装并启动于 `/Applications/ChatOS.app`。
- 实际 UI 检查确认云端文档标题栏位于内容区顶部，空状态居中于剩余区域，原截图顶部大块空白已消失。
- 原 App 已备份至 `/Applications/ChatOS.app.before-agent-auto-style-20260920-151204`。
- `git diff --check`：通过。

## 代码提交

- `2abf760e74a9bead8d0f44ebd442e58cbeec7dd7` (`fix(macos): repair agent proposal and document layout`)

## 剩余风险

- 为避免修改用户真实数据，本轮没有在生产数据上点击“确认创建”；该路径通过目录回归测试和全量客户端测试验证。
- 已存在的 `auto` 提案无需重建，重启后的新版客户端会按修复后的目录重新校验。

## 下一步

- 单独提交本进度文件并与代码提交一起推送至 `origin/3.0.4`。
- 用户可直接对现有提案点击“确认创建”，验证真实数据路径不再出现 `thinkingLevel` 错误。
