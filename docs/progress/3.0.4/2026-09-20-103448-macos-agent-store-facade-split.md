# macOS Agent Group Chat Store facade 拆分

- 时间：2026-09-20 10:34:48 CST（Asia/Shanghai）
- 本轮目标：纠正 macOS 阶段 1 的遗漏，将仍有 5,122 行的 `SQLiteAgentGroupChatStore.swift` 完整拆为小型 actor facade 和按领域组织的实现文件。
- 起始提交：`4af1369c78facdb5bdc23c327c746020556c3dd3`
- 代码提交：`f313b5e6e65934134a478eaafaa60d2ea5fb9532`

## 实际改动

1. 将 `SQLiteAgentGroupChatStore.swift` 从 5,122 行缩减到 80 行，只保留公开上传任务模型、actor 生命周期、数据库连接、附件根目录、测试计数器和文档草稿目录创建。
2. 新增 18 个领域扩展文件，分别承载 Profile、Todo 调度、五类 Proposal、Conversation、消息读写、附件支持、Team Asset、Todo、Delivery、Run、查询映射和数据库支持。
3. 新扩展文件最大 516 行，全部低于方案建议的普通生产文件 600 行门槛。
4. 保留既有 `AgentGroupChatDatabase`、Schema、Migrations、RowMapper 和各聚合根 Repository；没有修改 SQL、索引、排序、事务边界、错误文本或协议公开方法。
5. 拆分前后 Store 方法清单均为 151 个，方法名及重载数量逐项一致。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`
- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/SQLiteAgentGroupChatStore+*.swift`（18 个新文件）

## 业务不变量

- `SQLiteAgentGroupChatStore` 继续满足 `AgentGroupChatStore` 与 `LocalAgentGroupChatRunStoring`。
- Schema 版本、历史迁移、Codable 字段、账号隔离、权限校验和附件目录规则不变。
- Agent heartbeat、Todo 调度、Proposal 审批、消息/未读、Delivery claim、Run checkpoint 的事务语义不变。
- 未引入 statement cache 或其他未经基准证明的性能行为修改。

## 验证结果

- `swift test --package-path clients/macos --filter SQLiteAgentGroupChatStoreTests`：30 项，0 失败。
- `swift build --package-path clients/macos`：通过。
- `swift test --package-path clients/macos`：通过；主要 XCTest 分组 43、111、105、45 项均 0 失败；Swift Testing 分组 22、98、50 项均通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过。
- 本地化审计：412 个 UI 中文 literal，缺失英文 0；575 个中文 identity 条目，缺失/不一致 0。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。
- 打包产物：`clients/macos/.build/ChatOS.app`。

## 并行改动与剩余风险

以下 4 份并行改动未纳入本轮提交，均保持原状：

- `AgentGroupChatViewModel.swift`
- `AgentGroupChatWorkspaceViewModel.swift`
- `StoryWorkbenchView.swift`
- `SQLiteAgentGroupChatStoreTests.swift`

本轮纠正了此前退出审计中过早宣称 Store 重构完成的问题。macOS 自动化和打包门禁已重新通过；登录后真实账号关键路径仍需要运行时 Secret，并需在不与正在运行的用户实例争抢 SQLite/Connector/Scheduler 的条件下执行。

## 下一步

重新审计原方案列出的 macOS 结构热点及阶段退出条件，不再仅依据测试通过判定大文件重构完成；对仍超出门槛但属于并行改动的文件，先确认所有权再处理。
