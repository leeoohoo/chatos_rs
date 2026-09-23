# 3.0.4 进度：拆分 Todo Dependency 读取 Repository

- 时间：2026-09-20 04:52:17（Asia/Shanghai）
- 本轮目标：保持 Todo 依赖归属校验、JOIN 与排序不变，将 `listAgentTodoDependencies(...)` 的纯读取及行映射迁入 `AgentTodoRepository`。
- 起始提交：`417b0fc8e`
- 代码提交：`fae3371b9`

## 实际改动

- 为 `AgentTodoRepository` 增加按 owner 与 Todo 读取依赖列表的入口。
- 将 prerequisite Todo 的 Agent ID JOIN 与依赖行映射迁入 repository。
- facade 继续完成 owner、Agent、Todo ID 校验以及 Todo 归属存在性校验，再委托 repository 返回依赖列表。
- 依赖替换、循环检测和写事务均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍限定当前 owner 与 Todo，参数顺序保持 owner、Todo。
- SELECT 的四个列及列顺序保持不变。
- 仍按 owner 与 prerequisite Todo ID JOIN `local_agent_todos`，返回 prerequisite 的 Agent ID。
- 排序仍为创建时间、prerequisite Todo ID 升序。
- Todo 不存在或不属于指定 Agent 时仍在查询前抛出 not found。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- dependency 删除/插入、循环检测、原子替换及调用列表读取的事务边界均未改变。

## 验证结果

- `git diff --check`：通过。
- 定向契约与 Store 测试：43 个测试，0 失败。
- 可重复 Store 基准：1 个测试通过，三次 statement 计数与冻结基线完全一致：
  - workspace snapshot：`1008 / 1008 / 1008`
  - recent messages：`42 / 42 / 42`
  - image attachment read：`1 / 1 / 1`
  - idle heartbeat poll：`500 / 500 / 500`，数据库写入均为 0
  - idle account drain：`103 / 103 / 103`，数据库写入均为 0
- `swift test --package-path clients/macos`：退出码 0，全部测试通过。
- 全量测试结束后精确清理了 2 个已知孤儿 `fixture.zsh` 测试进程。

## 剩余风险

- facade 中仍有 Todo progress 与调度相关读取；部分读取嵌在写事务中，需要逐项保留事务边界。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 以单个有界工作单元拆分 Todo progress 列表纯读取与行映射，保留 Todo 归属校验、limit 语义和事件追加事务。
