# 3.0.4 进度：独立 AgentGroupChatStore 协议

- 时间：2026-09-19 06:11:21（Asia/Shanghai）
- 本轮目标：在不改变业务行为的前提下，将 `AgentGroupChatStore` protocol 从旧聚合文件移入独立 Core 文件，完成阶段 1 的领域声明拆分。
- 起始提交：`5f8fa1083`
- 代码提交：`a4707c010`

## 实际改动

- 将 `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift` 100% 重命名为 `clients/macos/Sources/ChatOSCore/AgentGroupChat/AgentGroupChatStore.swift`。
- 文件内容与协议 API 未发生任何字节变化。
- 原聚合文件已清空并由 Git 记录为纯重命名，不再保留无意义的空壳入口。

## 涉及文件

- `clients/macos/Sources/ChatOSCore/AgentGroupChat.swift`（移除）
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/AgentGroupChatStore.swift`（新增）

## 业务不变量

- `AgentGroupChatStore` 的方法集合、参数、返回值、并发与错误语义全部不变。
- 所有 Core 领域公开声明数量仍为 74。
- Store 实现、SQLite、Tool Provider、Scheduler 和 UI 行为不变。
- Git 检测结果为 100% rename，0 insertions、0 deletions。

## 验证结果

- 协议文件移动前后逐字比对：通过（`BYTE_FOR_BYTE_STORE_PROTOCOL_MATCH`）。
- `git diff --check`：通过。
- 定向测试：43 个测试，0 失败：
  - `AgentGroupChatCodableContractTests`
  - `LocalAgentDraftTests`
  - `SQLiteAgentGroupChatStoreTests`
  - `LocalAgentChatToolProviderTests`
- 初次全量测试受到此前测试遗留的 84 个孤儿 `fixture.zsh` 进程影响，出现 `NativeProjectGitServiceTests` 快照竞态与 `NativeTerminalTests` PTY 超时；相关业务定向测试和 Git 单测独立重跑均通过。
- 精确清理这些父进程已退出、且命令路径严格匹配测试临时目录 `fixture.zsh` 的孤儿进程后，再次完整执行 `swift test --package-path clients/macos`：退出码 0，无失败输出。
- 通过的全量测试仍会遗留 2 个同类孤儿 `fixture.zsh`，本轮已在验证后清理。

## 剩余风险

- macOS 测试套件中存在可复现的测试资源泄漏：每次全量运行遗留 2 个忽略 SIGTERM 的 `fixture.zsh` 进程；累计后会耗尽 CPU，并诱发 Git 与 PTY 用例失败。该问题不涉及本轮协议纯移动，但已取得稳定复现证据，后续应作为独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 阶段 1 的 Core 领域声明拆分已完成。下一轮先按阶段门禁审计拆分结果与文件规模，再进入 SQLite facade/database/schema/migration/repository 的有界拆分；测试资源泄漏另列独立修复单元，不与结构移动混合。
