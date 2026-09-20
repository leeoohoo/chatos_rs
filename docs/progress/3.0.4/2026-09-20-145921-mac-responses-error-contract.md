# 3.0.4 macOS Responses 错误契约与私聊反馈修复

## 本轮目标

- 依据 OpenAI 官方 Responses 文档和本机运行日志，明确区分模型配置失效、Responses 流式终态错误和工具调用结构错误。
- 保持所有 Agent 只使用 OpenAI Responses API，不增加 Chat Completions 或厂商私有协议兜底。
- 修复私聊错误提示一闪而过及短消息气泡横向铺满的问题。

## 起始提交

- `4491d06f13ecef096352ad3b21573eef92124aa5`

## 官方协议与日志证据

- OpenAI 官方 Streaming API Responses 文档确认流式响应使用 `text/event-stream`，成功终态为 `response.completed`：<https://developers.openai.com/api/docs/guides/streaming-responses>。
- OpenAI 官方 Function calling 文档确认 `function_call` 使用 `call_id` 关联工具输出：<https://developers.openai.com/api/docs/guides/function-calling>。
- OpenAI Responses 流式事件参考明确列出 `response.completed`、`response.incomplete`、`response.failed` 和 `error`：<https://developers.openai.com/api/reference/resources/responses/streaming-events>。
- 14:02 的失败日志显示旧模型配置请求返回 HTTP 404，错误为 `load ai model config via user_service failed`。
- 14:06 的失败日志显示当前模型请求已发送至 `/v1/responses` 并收到 HTTP 200；旧客户端随后把具体 Responses 协议问题折叠为“模型没有返回有效且完整的工具调用”。

## 实际改动

- 对 HTTP 200 但不是 SSE 的响应、缺少 `response.completed` 的提前结束、`response.incomplete`、`response.failed`、`error`、非法 completed envelope 和字段不完整的 `function_call` 分别返回明确错误。
- 错误只展示经过白名单约束的协议 code/reason，不回显上游原始 message 或响应正文。
- 将模型配置 404 映射为明确的“绑定配置已失效、已删除或不可用”，并说明客户端不会自动换用其他模型。
- 私聊错误改为可手动关闭的持久红色内联提示，不再被后台刷新或补充加载清除。
- 为 Markdown 消息增加 `.fitContent` 宽度模式，使短消息气泡按内容收缩。
- 新增 Responses 失败终态、缺失 completion、非法工具调用、非 SSE 成功响应及模型配置失效测试。

## 涉及文件

- `clients/macos/Sources/ChatOSAPI/ChatOSStoryPlanningService.swift`
- `clients/macos/Sources/ChatOSAgentRuntime/AgentResponsesModelClient.swift`
- `clients/macos/Sources/ChatOSAgentRuntime/AgentTypes.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentDirectChatView.swift`
- `clients/macos/Sources/ChatOSApp/Features/Shared/MarkdownDocumentView.swift`
- `clients/macos/Support/Localization/en.lproj/Localizable.strings`
- `clients/macos/Support/Localization/zh-Hans.lproj/Localizable.strings`
- `clients/macos/Tests/ChatOSAPITests/ChatOSStoryPlanningServiceTests.swift`
- `clients/macos/Tests/ChatOSAgentRuntimeTests/AgentChatModelClientTests.swift`

## 业务不变量

- 所有 Agent 仍通过配置的 `/v1/responses` 端点调用 OpenAI Responses API。
- 不添加 Chat Completions、供应商原生消息协议或自动换模型回退。
- 只有收到完整、合法的 `response.completed` 和必需工具调用字段后才允许执行工具。
- 模型配置、API key 和其他 Secret 不写入源码、文档、测试输出或 Git。
- 其他用户或并发进程正在修改的 macOS 与官网文件不纳入本轮提交。

## 验证结果

- 官方文档页面重新联网获取成功，共获取约 3.38 MB HTML，并检索到上述协议事件与字段。
- `swift test --package-path clients/macos --filter 'AgentChatModelClientTests|ChatOSStoryPlanningServiceTests'`：21 项通过（Responses 12 项、模型配置与故事规划 9 项）。
- 首次 `make test-macos-client` 仅有 `NativeTerminalTests` 的 PTY 用例发生一次 5 秒超时；单独复跑该测试 4 项通过。
- 第二次 `make test-macos-client`：全量通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- 新调试 App 已安装并启动于 `/Applications/ChatOS.app`；旧版本备份于 `/Applications/ChatOS.app.before-responses-fix-20260920-1450`。
- 实际私聊视觉检查确认“？”短消息显示为内容自适应气泡。
- `git diff --check`：通过。

## 代码提交

- `3e5c69f596d482ba9eb73334ced2b6b2df2ed499` (`fix(macos): surface strict Responses failures`)

## 剩余风险

- 上游网关若发送不符合 OpenAI Responses 规范的 200 响应，客户端现在会明确报出具体协议类别；仍需结合该错误推动网关修正实际返回内容。
- PTY 测试曾出现一次时间相关抖动，但定向复跑和第二次全量测试均通过。

## 下一步

- 单独提交本进度文件并与代码提交一同推送至 `origin/3.0.4`。
- 由用户在已安装的新 App 中重新发送消息，确认网关真实返回的具体错误或成功工具调用。
