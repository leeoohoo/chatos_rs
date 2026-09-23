# 3.0.4 推进记录：macOS 阶段 0 退出审计

- 时间：2026-09-19 01:41:42 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线，已完成。
- 本轮目标：汇总并重跑阶段 0 的契约、行为和性能证据，确认是否可以进入阶段 1 纯拆分。
- 起始提交：`89f16ae7c`
- 实施提交：`c766e208c`

## 实际改动

1. 新增 `docs/baselines/3.0.4-macos-stage-0-verification.zh-CN.md`，形成阶段 0 的独立退出审计。
2. 把职责/API、历史迁移、Core Codable、Store、Scheduler、Tool Provider、固定规模 Store 和 App/UI 基线映射到原方案门禁。
3. 记录当前三个可复现性能问题和阶段 4 才允许修改的边界。
4. 明确 Tool Provider 与 UI 在各自拆分前仍需增强的专项 characterization，避免把阶段 0 完成误解为后续可以跳过组件门禁。
5. 指定阶段 1 首个工作单元为 Core error/validation 纯移动拆分。

## 涉及文件

- `docs/baselines/3.0.4-macos-stage-0-verification.zh-CN.md`

## 业务不变量

- 本轮只审计和记录证据，不修改生产代码、测试、SQLite、协议或产品行为。
- 性能问题只进入阶段 4 队列，本轮不做优化或策略调整。
- 工作区三处用户并行改动没有被暂存、提交、覆盖或纳入权威 API 快照。

## 验证与测量结果

1. Core contract 与 Connector 历史迁移、Tool Provider、Scheduler、Store 筛选门禁共 53 个测试通过、0 失败。
2. Store 与 UI 两组 opt-in 固定规模基线同时通过，查询和发布硬计数未漂移。
3. 本轮再次确认：20 消息 42 条 SQL、500 消息快照 1008 条 SQL、空闲 drain 103 条 SQL且 0 写入、500 change burst 246 条 SQL和 42 次 UI 发布。
4. 上一代码提交 `f2976a6d7` 的完整 `swift test --package-path clients/macos` 已通过；其后至本轮只有 Markdown 文档变更。
5. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 阶段 0 已满足进入阶段 1 Core/持久化纯拆分的条件，但不授权改变 SQL、JSON Schema、错误文本、Codable 或 UI 行为。
- Tool Provider 的全量 canonical golden 必须在阶段 2 拆分前补齐；ViewModel 的分页、选择、错误恢复和 generation 取消测试必须在阶段 3 拆分前补齐。
- 下一轮从 `AgentGroupChat.swift` 仅移动 `AgentGroupChatError` 与 `AgentGroupChatValidation`，运行 Core、Connector 和完整 macOS 门禁后独立提交。
