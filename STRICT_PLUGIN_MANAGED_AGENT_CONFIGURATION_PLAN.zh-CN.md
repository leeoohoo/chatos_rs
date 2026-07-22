# 系统 Agent 严格插件配置实施计划

## 目标

- Plugin Management 是所有系统 Agent 的唯一 MCP、Skill、Agent 启停和权限策略来源。
- 未配置、空配置、快照缺失、身份不匹配、依赖缺失或同步失败时一律失败关闭。
- 不允许普通 Agent、规划 Agent、Task Runner 或环境 Agent 互相继承工具权限。
- Local Connector 的“系统 Agent 配置”把 Prompt 与插件配置作为一个原子更新资源包。

## 实施项

- [x] 本地普通对话按 `chatos_conversation_agent` 解析权限。
- [x] 本地规划对话按 `chatos_planning_agent` 解析权限。
- [x] 本地 Task Runner 分别按 `task_runner_plan_phase` / `task_runner_run_phase` 解析权限。
- [x] 空 MCP 选择不再解释为启用全部本地工具。
- [x] 禁止主对话继承 Task Runner 的文件读取权限。
- [x] 文件写入依赖读取时，要求同一 Agent 的插件配置明确包含读取资源。
- [x] 本地 Project Environment 扫描文件前要求显式配置 `CodeMaintainerRead`。
- [x] 系统配置更新覆盖 12 个系统 Agent 的完整能力快照。
- [x] Local Connector Service 向客户端开放全部 12 个系统 Agent 配置资源。
- [x] Prompt 48 条与插件配置 12 条在同一 SQLite 事务中替换。
- [x] 任意一条 Prompt 或插件配置缺失时拒绝安装并保留上一份有效配置。
- [x] 设置页分别显示 `Prompt 48/48` 与 `插件配置 12/12`，两者完整才算初始化。
- [x] 检查更新同时比较 Prompt Bundle Version 和插件配置完整内容。
- [x] 云端 Task Runner 缺少 Plugin Management 能力客户端时停止执行，不进入旧权限路径。
- [x] 完成受并行 MCP crate 重构影响的相关服务编译、完整单测与客户端实机验证。

## 验收标准

1. 普通/规划 Agent 未在插件管理配置文件读取 MCP 时，模型请求中不存在文件工具。
2. 仅给 Task Runner 配置文件 MCP 时，普通/规划 Agent 仍无文件权限。
3. 给目标 Agent 显式配置后，对应工具才会出现。
4. `CodeMaintainerWrite` 缺少同 Agent 的 `CodeMaintainerRead` 时直接报配置错误。
5. 初始化或更新时必须同时获得 48 条 Prompt 和 12 条插件配置；任何一项失败均不修改本机现有配置。
6. 设置页只有在 `Prompt 48/48`、`插件配置 12/12` 时显示已安装。
