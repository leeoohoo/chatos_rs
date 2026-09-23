# macOS Local Agent Scheduler 拆分

- 时间：2026-09-20 09:35:07 CST（Asia/Shanghai）
- 本轮目标：按账号 drain、生命周期、Run 执行和 Prompt 组装拆分本地 Agent Scheduler。
- 起始提交：`6d74c3a31afc61b23f0bb3c5b9a8647f253d323c`
- 代码提交：`d0f2dba64088cecd148b61a65516529e829b3297`

## 实际改动

- `LocalAgentGroupChatScheduler.swift` 保留取消注册表、账号租约协调器、公开类型、依赖和初始化，缩减到 224 行。
- 新增 `LocalAgentGroupChatSchedulerDrain.swift`：项目/会话/账号 drain、claim 规划、并发轮次与自动恢复。
- 新增 `LocalAgentGroupChatSchedulerLifecycle.swift`：项目停止、恢复、重试、放弃与失败包装。
- 新增 `LocalAgentGroupChatSchedulerExecution.swift`：单次 Run 组装、Memory、工具执行、checkpoint 与失败通知。
- 新增 `LocalAgentGroupChatSchedulerPrompt.swift`：初始系统/用户消息和有界失败摘要。
- 只调整跨文件所需的模块内部可见性，未改变算法或状态转换。

## 业务不变量

- 账号级 drain 仍串行；不同 Agent 可并发；同一 Agent 的 manager/executor 两条 lane 仍可并发。
- claim 次序、最大运行数、取消注册时机、崩溃恢复资格和 needs-review 语义不变。
- Memory 强连续性、Todo 终态、checkpoint、失败通知和 Prompt 内容不变。

## 验证结果

- `LocalAgentGroupChatSchedulerTests`：12 项通过，0 失败。
- Store 冻结基准：workspace `1008/1008/1008`、recent messages `42/42/42`、image attachment `1/1/1`、idle heartbeat `500/500/500` 且 0 写、idle account drain `103/103/103` 且 0 写。
- `swift test --package-path clients/macos`：退出码 0。

## 剩余风险与下一步

- 阶段 2 的 Provider 与 Scheduler 职责拆分完成；下一步进入阶段 3，先审计 UI/组合根热点及并行改动，再选择不覆盖用户修改的拆分单元。
- 未纳入并行修改：Agent Group Chat 两个 UI 文件、`SQLiteAgentGroupChatStoreTests.swift`、`StoryWorkbenchView.swift`。
