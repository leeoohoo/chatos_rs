---
name: task-runner-ai-agent-zh-cn
description: 中文指南，指导 AI agent 通过 Task Runner MCP 创建、查询、执行任务，选择内置 MCP 能力，配置前置任务依赖，并读取运行结果。
---

# Task Runner AI Agent Skill

当当前会话暴露了 Task Runner MCP 工具时，使用本指南创建、执行和维护任务。

## 核心原则

- 只操作当前 agent 有权限看到的任务。`list_tasks` / `get_task` 查不到的任务，不要假设自己可以引用。
- 不要编造 `task_id`、`run_id`、`model_config_id` 或前置任务 ID。只能使用工具返回过的真实 ID。
- 创建任务时只传工具 schema 要求或允许的业务字段。
- 创建任务不等于立即执行。需要马上执行时，再调用 `start_task_run` 或 `batch_start_task_runs`。
- 有破坏性或不可逆的操作，例如删除任务、批量改状态、取消运行，先确认用户意图。

## 常用流程

### 1. 查看当前任务

先用 `list_tasks` 搜索，再用 `get_task` 看详情。

```json
{
  "status": "pending",
  "keyword": "deploy",
  "limit": 20
}
```

可用过滤条件包括 `status`、`keyword`、`tag`、`model_config_id`、`scheduled_only`、`parent_task_id`、`source_run_id`、`limit`。

### 2. 创建普通任务

`create_task` 必填字段只有 `title` 和 `objective`。

```json
{
  "title": "检查订单同步失败原因",
  "objective": "定位订单同步失败的直接原因，输出证据、影响范围和建议修复方案。",
  "description": "用户反馈最近 2 小时部分订单未同步到下游系统。",
  "priority": 50,
  "tags": ["orders", "incident"],
  "input_payload": {
    "time_range": "last_2_hours",
    "systems": ["order-service", "downstream-sync"]
  }
}
```

字段说明：

- `title`: 给人看的短标题。
- `objective`: 任务完成时必须达成的结果，尽量可验收。
- `description`: 背景、限制、补充信息。
- `input_payload`: 结构化输入、日志片段、业务参数、外部引用。
- `priority`: 数字越大优先级越高。
- `tags`: 后续检索和分组用。
- `default_model_config_id`: 只有明确要指定模型时才传，值必须来自 `list_model_configs` / `get_model_config`。
- `schedule`: 只有用户明确要求定时、延迟或周期执行时才传。
- `enabled_builtin_kinds`: 当前任务执行时允许加载的内置 MCP 能力。
- `prerequisite_task_ids`: 当前任务执行前必须先成功完成的真实任务 ID 列表。

不要在 `create_task` 里尝试补充工具 schema 之外的系统内部字段。

### 3. 选择任务执行时可用的 MCP 能力

不确定有哪些能力时，先调用 `list_mcp_builtin_catalog`。创建任务时通过 `enabled_builtin_kinds` 传多选列表。

```json
{
  "title": "修复登录页按钮错位",
  "objective": "修复登录页移动端按钮错位问题，并给出验证结果。",
  "enabled_builtin_kinds": [
    "CodeMaintainerWrite",
    "TerminalController",
    "BrowserTools"
  ]
}
```

可选能力指南：

- `CodeMaintainerRead`: 只读代码仓库，适合理解代码、搜索实现、审查问题。
- `CodeMaintainerWrite`: 修改仓库文件、生成补丁、修复缺陷。需要实际改代码时使用。
- `TerminalController`: 运行命令、编译检查、脚本、读取终端输出。
- `TaskManager`: 在执行过程中拆分子任务、跟踪待办。
- `Notepad`: 长任务中保存计划、观察结果和中间结论。
- `AgentBuilder`: 维护 agent 配置、能力描述或构建材料。
- `UiPrompter`: 执行中需要用户输入、选择或确认时使用。
- `RemoteConnectionController`: 操作 Task Runner 服务器清单里的远程机器。
- `WebTools`: 搜索外部资料、读取网页、核对公开信息。
- `BrowserTools`: 打开和操作网页、截图检查 UI、读取页面状态。

选择建议：

- 只读排查用 `CodeMaintainerRead`。
- 要改代码用 `CodeMaintainerWrite`，通常再加 `TerminalController` 做验证。
- 前端 UI 问题加 `BrowserTools`。
- 需要远程日志或远程部署环境时加 `RemoteConnectionController`。
- 需要用户中途决策时加 `UiPrompter`。
- 不要为了“可能用到”而开启所有能力。

### 4. 创建有前置任务的任务

如果前置任务已经存在，先用 `list_tasks` 或 `get_task` 拿到真实 `task_id`，再创建当前任务。

```json
{
  "title": "生成发布风险结论",
  "objective": "基于前置检查结果，输出是否可以发布、风险点和回滚建议。",
  "prerequisite_task_ids": ["task_real_id_from_list_or_create"]
}
```

规则：

- `prerequisite_task_ids` 只能填真实任务 ID。
- 一个任务可以有多个前置任务，所有前置任务成功完成后当前任务才会执行。
- 不能依赖自己，不能形成循环依赖。
- 执行当前任务时，系统会先执行或等待前置任务，并把前置任务结果和过程记录注入当前任务的全局 prompt。
- 如果任一前置任务失败，当前任务会被阻塞或失败，不要假装当前任务已经完成。

### 5. 一次创建一组新任务并建立依赖

当新的前置任务还没有真实 ID 时，使用 `create_tasks_with_prerequisites`。每个任务用 `client_ref` 做本次请求内的临时引用，用 `prerequisite_refs` 引用同次创建的其他任务。

```json
{
  "tasks": [
    {
      "client_ref": "collect_logs",
      "title": "收集同步链路日志",
      "objective": "收集最近 2 小时订单同步链路的错误日志并总结关键异常。",
      "enabled_builtin_kinds": ["TerminalController", "RemoteConnectionController"]
    },
    {
      "client_ref": "inspect_code",
      "title": "检查订单同步代码",
      "objective": "阅读订单同步相关代码，找出可能导致漏同步的逻辑点。",
      "enabled_builtin_kinds": ["CodeMaintainerRead"]
    },
    {
      "client_ref": "diagnose",
      "title": "形成订单同步故障诊断",
      "objective": "结合日志和代码检查结果，输出根因、证据和修复建议。",
      "prerequisite_refs": ["collect_logs", "inspect_code"]
    }
  ]
}
```

工具返回后会给出每个 `client_ref` 对应的真实 `task_id`。后续只能使用真实 `task_id`，不要继续把 `client_ref` 当作任务 ID。

### 6. 修改或检查前置任务

替换某个任务的直接前置任务：

```json
{
  "task_id": "task_current",
  "prerequisite_task_ids": ["task_a", "task_b"]
}
```

清空前置任务时传空数组：

```json
{
  "task_id": "task_current",
  "prerequisite_task_ids": []
}
```

修改后用 `get_task_dependency_graph` 检查依赖图、传递前置任务、阻塞项和 `ready` 状态。

### 7. 执行任务并查看结果

立即执行单个任务：

```json
{
  "task_id": "task_current"
}
```

指定模型执行：

```json
{
  "task_id": "task_current",
  "model_config_id": "model_config_id_from_list_model_configs"
}
```

如果任务有前置任务，启动当前任务即可。系统会按依赖顺序处理前置任务，当前任务的 prompt 会自动包含前置任务执行结果和过程记录。

过程记录由 Task Runner 内部执行器维护。外部 agent 不需要也不能主动写过程记录。

查看运行：

- `list_runs`: 按任务、状态或模型过滤运行记录。
- `get_run`: 查看单次运行详情和输出。
- `list_run_events`: 查看执行事件、工具调用和失败点。
- `cancel_run`: 取消排队或运行中的任务。
- `retry_run`: 基于旧 run 创建重试。

### 8. 处理任务执行中的 UI prompt

如果任务启用了 `UiPrompter`，执行过程中可能产生待用户处理的 prompt。

- `list_prompts`: 查找当前任务或 run 的 prompt。
- `get_prompt`: 查看 prompt 详情。
- `submit_prompt`: 提交用户选择或输入。
- `cancel_prompt`: 在允许取消时取消 prompt。

不要替用户编造确认结果。需要用户选择时，应把选项和影响说明清楚。

### 9. 读取任务记忆

- `get_task_memory_context`: 查看任务组合后的 Memory Engine 上下文和线程摘要。
- `list_task_memory_records`: 查看任务线程持久化记录。
- `summarize_task_memory`: 触发一次 repair summary。它是修复或手动整理用途，不是普通执行流程的一部分。只有用户明确要求，或上下文明显需要修复时才调用。

## 模型配置

普通 agent 可以：

- `list_model_configs`
- `get_model_config`

管理员才应使用：

- `create_model_config`
- `update_model_config`
- `delete_model_config`
- `test_model_config`

如果用户要求“用某个模型”，先用 `list_model_configs` 找到真实 `model_config_id`，再传给 `create_task.default_model_config_id` 或 `start_task_run.model_config_id`。不要猜 ID。

## 决策模板

收到用户请求后按这个顺序判断：

1. 用户是在查已有任务吗？先 `list_tasks` / `get_task`。
2. 用户是在创建待办吗？调用 `create_task`，只填必要字段。
3. 任务执行需要代码、终端、浏览器、远程服务器或网页能力吗？需要时选择最小的 `enabled_builtin_kinds`。
4. 任务是否依赖其他任务结果？已有任务用 `prerequisite_task_ids`，同批新任务用 `create_tasks_with_prerequisites`。
5. 用户是否要求现在执行？创建后调用 `start_task_run`。
6. 执行后是否需要回报结果？用 `get_run` 和 `list_run_events` 汇总事实，不要凭空补全。

## 输出给用户时

尽量说明：

- 已创建的任务标题和真实 `task_id`。
- 是否配置了前置任务，以及前置任务 ID。
- 是否已启动执行，以及真实 `run_id`。
- 当前状态、失败原因或下一步需要用户确认的事项。
