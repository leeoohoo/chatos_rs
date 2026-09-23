# macOS Todo 与团队资产工具组细分

- 时间：2026-09-20 10:16:09 CST（Asia/Shanghai）
- 本轮目标：完成阶段 2 工具组边界审计，将 Todo、Todo 执行和团队资产分为独立文件。
- 起始提交：`3e5778990c140e0655b72949e93e60be3c0916de`
- 代码提交：`060ad4a312481ff3cc55726156738f80c069b4e6`

## 实际改动

- `LocalAgentChatTodoTools.swift` 缩减至 669 行，保留 Todo 调度、创建、更新、依赖和排序。
- 新增 `LocalAgentChatAssetTools.swift`，承载团队资产 list/get/upsert/archive。
- 新增 `LocalAgentChatTodoExecutionTools.swift`，承载 executor context、进度、完成/阻塞和 Todo 响应组装。
- 全部为原实现机械迁移，无协议或行为调整。

## 业务不变量

- 工具名、Schema、响应 JSON、权限、revision、依赖与 executor lane 语义不变。
- Store 调用顺序、错误码、引用作用域和时间戳不变。

## 验证结果

- Tool Provider 与 Scheduler 定向测试：17 项通过，0 失败。
- Store 冻结基准查询计数保持冻结值，空闲场景 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- Tool Provider 所有领域组现均处于建议文件规模；下一步重新执行阶段 5 最终门禁并归档未解决外部阻塞。
- 4 个既有并行修改保持未提交。
