# 3.0.4 推进记录：历史 Agent Schema 行为基线

- 时间：2026-09-18 20:08:55 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：增加真实历史 Agent Group Chat SQLite fixture 与迁移 golden test，在拆分 Store 前锁定兼容行为。
- 起始提交：`99778d458`
- 本轮代码提交：`c32cd73dd`

## 实际改动

1. 从引入 migration 12 之前的真实仓库提交 `1ca6f3467` 提取 schema v11，保存为只含合成数据的 SQL fixture。
2. fixture 覆盖 Agent、项目团队、成员、消息、Markdown 附件、Delivery 和 Run 的历史行，并带完整 v11 迁移标记。
3. 新增 golden test，验证 v11 数据库原地升级至 schema 24 后：
   - Agent 身份、角色、思考等级及 heartbeat 默认值保持正确；
   - 房间、消息、附件、Delivery 和 Run 行不丢失；
   - 附件远端同步字段按兼容默认值补齐；
   - migration 1～24 完整存在；
   - 附件列顺序与当前读取契约一致；
   - `PRAGMA foreign_key_check` 无错误；
   - 数据库二次打开不改变消息快照。
4. 将历史 fixture 作为 `ChatOSConnectorTests` 测试资源打包，不引入生产运行时依赖。

## 业务不变量

- 本轮只增加测试和 fixture，没有修改 Store、SQL、迁移、领域模型或业务行为。
- fixture 中的 owner、Agent、房间、消息和内容均为合成值，不含真实账号或本机数据。
- 用户正在修改的 ViewModel、Workspace 和既有 Store 测试文件未纳入本轮提交。

## 验证结果

1. `swift test --package-path clients/macos --filter AgentGroupChatHistoricalMigrationTests`：1 test，0 failed。
2. `swift test --package-path clients/macos`：Core、Connector、App、AgentRuntime、API 与 Swift Testing 全部 0 failed；1 个真实浏览器环境用例按预期跳过，仅有既有 SwiftTerm 构建缓存 warning。
3. `git diff --check` 与 `git diff --cached --check` 通过。

## 剩余风险与下一步

- v11 fixture 锁定最早一组完整本地 Agent 表；migration 17、21、22、23、24 仍有各自的专项回归测试。
- 下一轮继续阶段 0，建立待拆大文件的职责/API 快照和可重复 Store 性能数据集，再开始 Core 纯拆分。
