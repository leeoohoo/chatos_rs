# Mac Todo 来源会话路由恢复

- 时间：2026-09-20 17:05:37 +0800（Asia/Shanghai）
- 本轮目标：修复 Todo 负责人并非来源私聊成员时，`chat_inbox_send` 抛出 `Agent 不是当前群聊成员` 并令通讯 Run 进入人工审查的问题；同时保留旧 Run 引用失效的可恢复处理。
- 起始提交：`e7797ae58f41979cc79a7ffc711bc4df1bff7ffe`
- 代码提交：`74b91bc3f2d86b9f37a427487aa4e8bbbcb60fe3`

## 实际改动

- 把旧版随机 inbox 引用或无效引用转换为 `invalid_inbox_reference` 结构化错误，要求重新调用 `todo_list` 获取当前 Run 的引用，不再把无副作用的输入错误升级为写工具中断。
- 当 Todo 来源会话有效、但当前负责人不是该私聊参与者时，`chat_inbox_send` 返回 `source_conversation_not_accessible`，并明确引导调用 `agent_workspace_snapshot` 后使用 `chat_team_send` 向 Todo 所属团队公开汇报。
- 失败前已经预留的 Markdown 文档引用会被释放，可在团队群汇报时继续使用，不会丢失交付物。
- 更新 `chat_inbox_send`、`todo_list` 工具说明和 Todo 状态通知提示词，明确来源会话不等于负责人可访问会话，禁止对不可访问私聊反复重试。
- 新增端到端回归：项目经理从自己的 Human 私聊创建来源、把 Todo 分配给其他团队成员；负责人向来源汇报得到可恢复错误，随后携带同一文档在团队群成功汇报并精确通知项目经理。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/LocalAgentChatInboxTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolRegistry.swift`
- `clients/macos/Sources/ChatOSCore/Resources/AgentPrompt.Cycle.TodoStatus.md`
- `clients/macos/Tests/ChatOSConnectorTests/LocalAgentChatToolProviderTests.swift`

## 业务不变量

- Agent 仍不能读取或写入自己不是活跃成员的私聊；本轮没有放宽成员权限。
- Todo 来源仍忠实记录最初的来源会话和消息，不迁移、不伪造回复关系。
- 只有真正具备来源会话成员资格的 Agent 才能通过 `chat_inbox_send` 回复来源。
- 团队进度、阻塞与交付仍默认在项目团队群公开；项目经理通过显式 `mention_agent_refs` 被唤醒。
- 写入失败时不消费尚未成功附加的文档引用；成功发送后仍按原规则一次性消费。
- 所有模型可见引用仍是当前 Run 签发的临时引用，不暴露真实数据库 ID。

## 验证结果

- `swift test --package-path clients/macos --filter LocalAgentChatToolProviderTests`：6 个测试通过，0 失败。
- `make test-macos-client`：全量 Mac Swift 测试通过；各测试目标 0 失败，既有 1 个跳过保持不变。
- `clients/macos/scripts/package-debug-app.sh`：构建、资源复制和签名成功。
- `codesign --verify --deep --strict /Applications/ChatOS.app`：通过。
- 安装后二进制与本轮构建产物逐字节一致；安装资源已包含 `source_conversation_not_accessible` 路由提示。
- 新应用已启动；旧应用备份为 `/Applications/ChatOS.app.before-source-routing-20260920-170440`。

## 剩余风险

- 已经处于 `needsReview` 的旧 Run 不会自动清除历史错误状态，需要用户在新客户端中点击“重试中断步骤”；重试后新工具实现会把错误返回给模型并允许其改投团队群。
- 回归覆盖了本地 SQLite、Relay 工具、文档引用和团队投递链路；真实模型是否严格遵循 `next_tool` 仍取决于模型输出，因此工具说明与周期提示词同时加了强约束。
- 工作树仍有用户或其他进程的并行修改，本轮未暂存、提交或覆盖这些文件。

## 下一步

- 在已安装的新客户端中对当前 `needsReview` Run 点击“重试中断步骤”，确认不再弹出成员权限中断框，并在“三国横版闯关游戏团队”看到公开汇报。
- 若模型仍偏离 `next_tool`，保留对应 Run 和工具调用记录，再针对真实输出补充确定性路由约束；不要放宽私聊权限。
