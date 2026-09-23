# macOS 媒体生成服务线缆层拆分

- 时间：2026-09-20 12:19:14 CST（Asia/Shanghai）
- 本轮目标：在不改变媒体生成业务行为的前提下，拆分 `ChatOSMediaGenerationService.swift` 中的图片请求线缆逻辑和传输模型。
- 起始提交：`227101edb`
- 代码提交：`9851c7f50`

## 实际改动

- 将图片生成请求构建、multipart 编码、错误正文解析和模型排序移动到独立的 `ChatOSImageGenerationWire.swift`。
- 将媒体 runtime/wire DTO、视频任务传输模型和客户端错误模型移动到独立的 `ChatOSMediaGenerationWireModels.swift`。
- `ChatOSMediaGenerationService.swift` 从 1,062 行降到 773 行；本轮仅调整同一模块内的代码边界与访问级别，没有修改公开接口或请求语义。
- 避开并未提交本轮发现的 Agent 群聊、Story 视图和 SQLite 测试并发修改。

## 涉及文件

- `clients/macos/Sources/ChatOSAPI/ChatOSMediaGenerationService.swift`
- `clients/macos/Sources/ChatOSAPI/ChatOSImageGenerationWire.swift`
- `clients/macos/Sources/ChatOSAPI/ChatOSMediaGenerationWireModels.swift`

## 业务不变量

- 图片和视频生成的公开 API、模型选择、请求字段、multipart 边界内容与错误映射保持不变。
- 认证、provider endpoint 选择、轮询策略和任务完成语义保持不变。
- 新拆出的类型仍为 `ChatOSAPI` 模块内部实现，不扩大公开 API 面。

## 验证结果

- `swift test --package-path clients/macos --filter ChatOSMediaGenerationServiceTests`：25 通过，0 失败。
- `swift test --package-path clients/macos --filter NativeProjectGitServiceTests`：2 通过，0 失败。
- `swift test --package-path clients/macos`：全量通过；现有环境门禁测试按设计跳过。
- 首次全量测试中 `NativeProjectGitServiceTests.testLoadsChangesStagesCommitsAndBuildsDiff` 曾出现一次 `notRepository`；隔离复测和随后全量复测均通过，未发现与本轮纯拆分相关的稳定故障。
- `git diff --check`：通过。

## 剩余风险与下一步

- `ChatOSMediaGenerationService.swift` 已显著缩小，但仍包含媒体编排主流程；后续仅在职责边界清晰且有相关测试时继续拆分。
- 下一有界单元审计并拆分约 1,006 行的 `NativeLocalConnectorService.swift`，保持连接器行为不变。
- 真实账号媒体生成和外部 provider E2E 仍依赖运行时凭证与网络环境，未在本轮注入或记录任何 Secret。
