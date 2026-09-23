# 3.0.4 推进记录：macOS 重构职责与契约基线

- 时间：2026-09-18 20:44:01 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：在移动大文件代码前冻结职责、依赖方向、公共 API 和拆分顺序。
- 起始提交：`47989e237`
- 本轮提交：`92ef3e2c1`

## 实际改动

1. 新增 `docs/baselines/3.0.4-macos-native-client-refactor-baseline.zh-CN.md`。
2. 基于 Git `HEAD` 而不是脏工作区，记录 10 个 macOS 热点文件的准确行数与混合职责。
3. 固化 Core/Connector/API/App 的允许依赖方向，明确禁止 Core 反向依赖和 Store/Scheduler 读取 UI 组合根。
4. 冻结以下外部兼容面：
   - Core 的 74 个公开领域声明；
   - `AgentGroupChatStore` 的 76 个方法；
   - Relay MCP 的 37 个工具名及 JSON/错误兼容要求；
   - schema 24、SQL/事务/索引/迁移约束；
   - Scheduler 的账号、Agent 与 lane 并发不变量；
   - UI 的分页、未读、选择、滚动、附件和错误恢复行为。
5. 给出 Core、Store、Tool Provider、Scheduler、UI 的文件级拆分顺序与逐提交门禁。

## 涉及文件

- `docs/baselines/3.0.4-macos-native-client-refactor-baseline.zh-CN.md`

## 业务不变量

- 本轮为文档基线，不修改生产代码、测试、SQLite 或产品行为。
- 行数与 API 数量从提交 `47989e237` 读取，未把用户并行改动固化为权威契约。
- 用户正在修改的 ViewModel、Workspace 和既有 Store 测试文件没有被暂存或提交。

## 验证结果

1. 使用 `git show HEAD:<path>`、`wc -l` 和 `rg` 复核 10 个热点文件、74 个公开 Core 声明、76 个 Store 方法和 37 个工具名。
2. `git diff --cached --check` 通过。
3. 上一代码提交 `c32cd73dd` 的完整 `swift test --package-path clients/macos` 已通过；本轮只有 Markdown 文档，不触发重新编译。

## 剩余风险与下一步

- 阶段 0 尚需建立可重复的 Store 数据规模与查询/耗时基线，性能候选问题仍不能直接认定为故障。
- 下一轮优先增加不进入普通测试耗时路径的性能数据集与测量入口；完成阶段 0 门禁后，再开始 Core 纯移动拆分。
