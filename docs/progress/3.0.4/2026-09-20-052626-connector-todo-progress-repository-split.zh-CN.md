# 3.0.4 进度：拆分 Todo Progress 读取 Repository

- 时间：2026-09-20 05:26:26（Asia/Shanghai）
- 本轮目标：保持 Todo progress 的归属校验、limit、倒序截取与正序返回语义不变，将列表读取及行映射迁入 `AgentTodoRepository`。
- 起始提交：`b3ea4d78e`
- 代码提交：`20f8272ce`

## 实际改动

- 为 `AgentTodoRepository` 增加按 owner、Agent、Todo 与 limit 读取 progress 的入口。
- 将 progress kind 校验、可空 run ID 映射和“数据库倒序取最近 N 条后正序返回”逻辑迁入 repository。
- facade 继续校验三个 ID、limit 范围和 Todo 归属存在性，再委托 repository 返回事件列表。
- progress 事件追加、序号分配和写事务均未移动。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentTodoRepository.swift`
- `clients/macos/Sources/ChatOSConnector/SQLiteAgentGroupChatStore.swift`

## 业务不变量

- 查询仍限定当前 owner 与 Todo，参数顺序保持 owner、Todo、limit。
- SELECT 的八个列及列顺序保持不变。
- 数据库仍按 sequence 降序并以原 limit 截取，返回前仍反转为升序。
- 无效 progress kind 仍抛出 `invalid Agent Todo progress kind` 存储错误；可空 run ID 映射保持不变。
- limit 仍限制为 1 至 500；Todo 不存在或不属于指定 Agent 时仍产生原有 invalid field 语义。
- repository 查询仍在 prepare 前精确增加一次 DEBUG statement 计数。
- progress 事件序号计算、插入与调用列表读取的事务边界均未改变。

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

- facade 中仍有 Todo 调度候选、递归循环检测、事件序号和依赖状态等读取；其中多项嵌在写事务中，需要逐项保留事务边界。
- macOS 阶段 1 尚未确认满足退出条件，不得进入阶段 2。
- 测试资源泄漏问题仍待独立工作单元修复。
- 用户并行修改的 ViewModel、WorkspaceView 与 SQLite 测试文件未纳入本轮提交。

## 下一步

- 继续审计 facade 剩余 SELECT，优先拆分不改变写事务边界的独立 Todo 调度读取。
