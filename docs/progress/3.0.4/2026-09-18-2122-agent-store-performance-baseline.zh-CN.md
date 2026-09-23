# 3.0.4 推进记录：Agent Store 可重复数据规模基线

- 时间：2026-09-18 21:22:43 CST（Asia/Shanghai）
- 阶段：原生客户端重构阶段 0——行为冻结与可测基线。
- 本轮目标：为 Agent 工作区 Store 热路径建立显式启用、可重复运行且不拖慢常规测试的固定规模性能入口。
- 起始提交：`f09c35b0c`
- 代码提交：`71af8ad79`

## 实际改动

1. 新增 opt-in XCTest 性能基线，固定构造 20 个 Agent、10 个团队、500 条消息、500 个 Run、100 个 Todo 和一个 128 KiB 图片附件。
2. 默认测试套件明确跳过数据构造；设置 `CHATOS_RUN_AGENT_STORE_BASELINE=1` 后才执行，避免把性能 fixture 的成本带入普通正确性门禁。
3. 每次基线连续测量 3 轮 Store 重新打开、完整工作区快照、最近消息读取和图片附件读取，并输出一行排序后的 JSON，便于后续重构前后对比。
4. 测量只读取现有 Store 公共契约，没有给生产代码加入测试开关、缓存或新状态。

## 涉及文件

- `clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift`

## 业务不变量

- 不修改生产代码、SQLite schema、SQL、索引、事务边界、排序或产品 UI。
- fixture 使用独立临时目录和专用虚构 owner，结束后删除，不读取账号 Secret 或用户数据。
- 性能结果只作为后续前后对比样本；当前数据不构成“已有性能故障”结论，也不据此提前修改业务路径。
- 工作区中已有的 ViewModel、Workspace 和 Store 测试并行改动未被暂存、提交或覆盖。

## 验证结果

1. `swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 通过；基线用例按设计跳过。
2. `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos --filter AgentGroupChatStorePerformanceBaselineTests` 通过，fixture 构造与 3 轮测量总计约 1.69 秒。
3. 本机样本：
   - Store 打开：`1.895 / 0.935 / 0.887 ms`
   - 工作区快照：`30.878 / 42.475 / 158.939 ms`
   - 最近 20 条消息：`0.330 / 0.338 / 0.338 ms`
   - 128 KiB 图片附件读取：`0.263 / 0.173 / 0.201 ms`
4. `swift test --package-path clients/macos` 全量通过；Connector 111 项（1 项为新增 opt-in 跳过）、App 104 项，其余 Core、API、AgentRuntime 与 Swift Testing 套件均通过。
5. `git diff --cached --check` 通过。

## 剩余风险与下一步

- 工作区快照第 3 轮出现明显抖动；在获得查询次数、主线程长任务和更多隔离重复样本前，不据此宣称回退或收益。
- 本轮覆盖 Store 打开与主要工作区读取，不覆盖启动全链路、Run 高频 change stream、空闲 CPU 或 UI 主线程排版；阶段 0 尚未完成。
- 下一轮应在不触碰并行修改文件的前提下补齐查询计数或高频事件基线，再判断阶段 0 是否达到退出条件。
