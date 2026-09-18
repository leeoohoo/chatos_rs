# 3.0.4 推进记录：精简沟通与文档附件 M0

- 时间：2026-09-18 18:26:26 CST（Asia/Shanghai）
- 本轮目标：先实施《本地 Agent 精简消息与长文附件》方案的 M0，为后续本地文档闭环建立统一策略和稳定指令快照。
- 起始提交：`8810b8e6f`
- 本轮代码提交：`7fe568b53`

## 实际改动

1. 将新方案文档纳入版本控制，明确 M0～M4 的协议、离线优先边界、安全边界与验收标准。
2. 新增集中式 `AgentCommunicationPolicy.standard`，统一维护消息建议/最大长度、单消息文档数、单文档大小和单 Run 文档总大小。
3. 新增中英文、manager/executor 分层的产品级 `chatos-compact-communication` Skill。
4. 新 Run 根据语言与执行通道注入对应 Skill；Run checkpoint 固化 Skill 名称、版本、语言、受众和内容 SHA-256。
5. 旧 checkpoint 的新字段保持可选，恢复已存在 Run 时不重新注入或改写持久化消息。
6. 补充策略阈值、Skill 稳定哈希、双语/受众差异、Prompt 注入、checkpoint 元数据、旧 checkpoint 解码和恢复不漂移测试。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentCommunicationPolicy.swift`
- `clients/macos/Sources/ChatOSCore/Resources/Skills/chatos-compact-communication/`
- `clients/macos/Sources/ChatOSAgentRuntime/AgentTypes.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentGroupChatScheduler.swift`
- `clients/macos/Sources/ChatOSCore/LocalAgentPromptCatalog.swift`
- `clients/macos/Sources/ChatOSCore/Resources/AgentPrompt.GroupChat.System.md`
- 对应 AgentRuntime、Core 和 Connector 测试
- `docs/plans/LOCAL_AGENT_COMPACT_MESSAGES_AND_DOCUMENT_ATTACHMENTS.zh-CN.md`

## 业务与安全不变量

- 未改变既有 manager/executor 业务流程和消息投递语义。
- M0 Skill 使用过渡性条件文案：只有当前 Run 提供 `chat_document_create` 时才要求创建文档，避免 M1 尚未落地时误导模型。
- 模型未获得真实用户、房间、消息、对象存储或本机路径信息。
- 用户提供的测试账号未写入代码、文档、日志或提交。
- 既有 ViewModel、Workspace、Store 测试和 document plugin 元数据并行改动未纳入本轮提交。

## 验证结果

1. 定向测试通过：
   - `swift test --package-path clients/macos --filter 'AgentRuntimeTests|LocalAgentSkillCatalogTests|LocalAgentGroupChatSchedulerTests'`
2. 首次完整测试发现 `LocalAgentPromptCatalogTests` 的 GroupChat 测试值未补充新占位符，进程按设计触发模板完整性断言；补齐 `compact_communication_skill` 测试值后相关测试通过。
3. 修复后完整测试通过：
   - `swift test --package-path clients/macos`
4. `git diff --check` 通过。

## 下一步

进入 M1 本地文档闭环：扩展 `LocalAgentRunReferenceVault` 的文档权威记录，实现 `chat_document_create`，为四个发送工具增加 `document_refs` 和统一正文上限，在本地 SQLite 消息事务中绑定 Markdown 附件，并补齐跨 Run/Agent、伪造、重复消费、大小边界和离线读取测试。
