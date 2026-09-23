# 3.0.4 推进记录：沟通指标阈值权威源修正

- 时间：2026-09-18 20:00:23 CST（Asia/Shanghai）
- 本轮目标：修复 M4 整体验收发现的指标分桶阈值重复定义。
- 起始提交：`1cdcd22b0`
- 本轮代码提交：`ca741949e`

## 实际改动

1. 在 `AgentCommunicationPolicy` 增加推荐区间下界的派生属性，保留 Codable 数据结构兼容性。
2. 消息长度指标分桶统一读取 `AgentCommunicationPolicy.standard` 的推荐下界、推荐上界和发送硬上限。
3. 指标维度由包含数字的名称改为稳定语义值：`concise`、`recommended`、`extended`、`over_limit`，避免策略调整后维度名称与真实边界不一致。
4. 边界测试同步改为读取统一策略，仍覆盖边界值及相邻值，不再在实现和测试中复制 800/2,000 阈值。

## 验证结果

1. `git diff --check` 通过。
2. `swift test --package-path clients/macos --filter 'AgentCommunicationMetricsTests|LocalAgentSkillCatalogTests'` 通过：6 tests，0 failed。
3. 修正前同一工作树的完整 macOS 测试已通过；本轮只调整策略读取和指标维度，相关 Core/Connector 定向回归已重新执行。
4. 后端 artifact 定向测试通过：`cargo test -p chat_app_server_rs agent_artifact`，3 passed，0 failed。

## 安全与并行改动

- 指标仍不保存消息正文、文档内容、路径或对象存储信息。
- 发送限制和业务逻辑未改变，只消除了观测层的重复阈值来源。
- 工作区原有 ViewModel、Workspace 和 Store 测试并行改动未纳入本轮提交。

## 下一步

继续完成精简消息与长文附件方案的跨层整体验收，然后进入 3.0.4 原生客户端重构计划。
