# Agent 四类发送工具附件执行矩阵

- 时间：2026-09-20 11:46:44 CST（Asia/Shanghai）
- 本轮目标：补齐四类消息发送工具携带文档的实际执行与持久化验证。
- 起始提交：`859074218`
- 代码提交：`192fc9a41`

## 实际改动

- `chat_send_message` 继续验证当前会话发送、一次性文档引用消费、重放幂等和附件读取。
- `chat_team_send` 新增 Markdown 文档创建、发送和团队消息附件持久化断言。
- `chat_direct_send` 新增私聊引用解析、Markdown 文档发送和私聊消息附件持久化断言。
- `chat_inbox_send` 新增跨会话收件箱回复携带 Markdown 文档及附件持久化断言。

## 涉及文件

- `clients/macos/Tests/ChatOSConnectorTests/LocalAgentChatToolProviderTests.swift`

## 业务不变量

- 每个文档引用只由一次成功发送消费，其他工具不能重复使用。
- 工具响应和 Agent 可见数据继续只暴露不透明引用，不暴露 SQLite 主键或本地绝对路径。
- 附件与消息在同一持久化操作中写入，测试从存储层重新读取验证，而不是只检查工具响应。
- 四个工具保持原有会话权限、成员约束、路由和 replay 行为。

## 验证结果

- `swift test --package-path clients/macos --filter LocalAgentChatToolProviderTests`：5 通过，0 失败。
- `swift test --package-path clients/macos`：全量通过；现有环境门禁测试按设计跳过。
- `git diff --check`：通过。

## 剩余风险与下一步

- 仍需实现账户级 artifact 列表和 macOS 远端附件浏览/预览入口，解决已知 artifact ID 之外的跨设备发现问题。
- 完整消息跨设备同步不在本轮伪装完成；群聊消息仍以设备本地 SQLite 为权威。
- 真实第二设备 E2E、100 条附件消息 UI/性能验证及 PostgreSQL 路由鉴权集成测试仍需相应运行时环境。
