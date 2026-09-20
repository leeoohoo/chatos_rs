# macOS 阶段 5 退出审计

- 审计时间：2026-09-20 10:20 CST（Asia/Shanghai）
- 审计范围：`LOCAL_AGENT_COMPACT_MESSAGES_AND_DOCUMENT_ATTACHMENTS.zh-CN.md` 完成定义，以及 `3.0.4-native-client-refactor-and-parity-plan.zh-CN.md` 的 macOS 阶段 0～5。
- 审计基线：`374e9a37d43dd4a94ddcef4f8258c9dfba0655bc`
- 当前已推送提交：`db23ea5a86a08286908fc1a3698df78ce5a4c085`

## 已完成

1. 精简消息与文档附件 M0～M4 的代码闭环、阈值权威来源、离线本地文档、Agent artifact、统一预览、观测指标与回归测试已完成。
2. macOS Core 与 SQLite 已按领域拆分；历史迁移、Codable、事务与冻结数据量基准持续通过。
3. Tool Provider 已拆为 registry、codec、response、reference vault、proposal、inbox、messaging、Todo、Todo execution 和 asset 工具组。
4. Scheduler 已拆为 drain、lifecycle、execution 和 prompt；账号租约与 manager/executor lane 语义保持不变。
5. UI 与组合根热点已拆分：团队工作区、Workspace ViewModel、Agent 管理、主 Chat ViewModel、AppModel、Media Studio 和 Story Studio。
6. 打包门禁发现的 Markdown 预览本地化缺口已修复；中英文 catalog 审计为 0 缺失。

## 自动化证据

- `swift build --package-path clients/macos`：退出码 0。
- `swift test --package-path clients/macos`：退出码 0；主要 XCTest 汇总 111 项 0 失败；Swift Testing 分组 22、98、50 项均通过。
- `make test-macos-client`：退出码 0。
- `clients/macos/scripts/package-debug-app.sh`：退出码 0。
- 产物：`clients/macos/.build/ChatOS.app`。
- `codesign --verify --deep --strict`：通过。
- 本地化审计：412 个 UI 中文 literal，缺失英文 0；575 个中文 identity 条目，缺失/不一致 0。
- 冻结 Store 基准最近结果：workspace `1008/1008/1008`；recent messages `42/42/42`；image attachment `1/1/1`；idle heartbeat `500/500/500` 且 0 写；idle account drain `103/103/103` 且 0 写。

## 未解决风险与外部阻塞

1. 工作区仍有 4 份在本轮开始前或执行期间出现的并行改动，已完整保留且未提交：
   - `AgentGroupChatViewModel.swift`：移除未使用的 `members` 参数；
   - `AgentGroupChatWorkspaceViewModel.swift`：trigger Run 的 Delivery/Message 批量读取；
   - `SQLiteAgentGroupChatStoreTests.swift`：批量读取覆盖；
   - `StoryWorkbenchView.swift`：解除嵌套 ScrollView 的循环高度 proposal。
2. 这些改动随当前工作区一起通过了最终 build、test、make 和 package，但其所有权不属于本轮，不能由本轮擅自提交或丢弃。
3. 当前环境未注入测试账号 Secret，无法自动完成登录后普通聊天、真实附件上传/跨设备恢复、插件、终端、文件/Git、宠物与 Visual Session 的人工关键路径。
4. `/Applications/ChatOS.app` 已有用户实例运行；为避免第二实例争抢 SQLite、Connector 和 Agent scheduler，本轮未强制启动 debug 包。
5. debug 包使用 ad-hoc 签名，`spctl` 拒绝未公证包属于开发产物的预期限制，不代表 Release 公证验收通过。

## 结论

macOS 代码重构与自动化/打包门禁已完成。阶段 5 不能标记为“无条件完成”，直到并行改动由其所有者决定是否提交，并在安全注入运行时测试账号后完成登录后关键路径。Windows 阶段未开始。
