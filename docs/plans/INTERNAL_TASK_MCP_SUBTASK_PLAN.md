# 内部 task MCP 子任务挂载方案

## 核心需求

TaskRunner 在执行某个任务时，AI 如果通过内部 `task_manager` MCP 创建任务，这些任务要落到 TaskRunner 自己的 `tasks` 数据里，并自动挂到当前正在执行的任务下面。

除此之外，内部 task MCP 的主要使用方式保持不变：AI 还是按原来的 `add/list/update/complete/delete` 工具维护这些任务，不需要知道父任务、运行来源、权限归属、租户、memory 等程序内部信息。

唯一新增的 AI 可见能力：`add_task` 可以可选传入一个已存在的前置任务 ID。这个 ID 用于表达内部子任务之间有明确先后顺序；不需要时不传。

## 边界

- AI 只描述要创建或更新的业务任务内容。
- AI 可以在确有先后关系时传一个已存在的前置任务 ID。
- 父任务、来源运行、owner、tenant、subject、memory thread 等都由服务端根据当前执行上下文和认证上下文自动处理。
- 子任务不需要模型配置、MCP 工具选择，也不需要独立 memory 初始化；它们不会作为独立可运行任务去调模型。
- MCP schema、prompt、工具说明里不出现这些内部字段。
- 工具返回给 AI 的内容也不带父任务 id、来源运行 id、owner、tenant 等内部字段。
- 子任务展示给人看时，可以通过 TaskRunner/Chatos 的页面看到它属于哪个当前任务，但这属于 UI/API 展示，不属于 AI 输入。

## 当前代码现状

现有代码已经接近这个目标：

- `crates/chatos_builtin_tools/src/task_manager.rs` 定义内部 task MCP 工具。
- `task_runner_service/backend/src/services/builtin_providers/builders.rs` 给 TaskRunner 运行时注册了内置 TaskManager。
- `task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs` 创建 `TaskRunSpec` 时把当前 `task.id` 和 `run.id` 放进运行上下文。
- `task_runner_service/backend/src/services/task_manager_bridge/store_adapter.rs` 把 task MCP 的创建请求接到 TaskRunner 的任务服务。
- `task_runner_service/backend/src/services/task_manager_bridge/task_ops.rs` 的 `create_followup_task_for_tool` 已经会用当前上下文写入 `parent_task_id` 和 `source_run_id`，并继承父任务 owner/tenant。

所以结论是：可以做，而且不需要动 AI 调用的任何东西。

需要检查和收紧的点：

- 不改 `crates/chatos_builtin_tools/src/task_manager.rs` 现有 MCP 工具字段语义和调用方式。
- 只给 `add_task` draft 追加可选前置任务 ID，不影响旧调用。
- 确认所有 TaskRunner 执行期的内部 task MCP 创建都只走 `create_followup_task_for_tool`。
- `task_to_manager_value` 这类返回给 AI 的视图不能包含父任务、来源运行、owner、tenant 等内部字段。
- `create_followup_task_for_tool` 不应该继续复制父任务的 `default_model_config_id` 和 `mcp_config`，也不应该为子任务初始化独立 memory thread；子任务应保存为空模型、禁用/空工具配置，memory 字段只做数据库结构需要的占位。
- Chatos 流程图现在主要看前置依赖，后续要显示“子任务”时，需要从 TaskRunner 按父任务查询。

实际需要动的核心代码：

- `crates/chatos_builtin_tools/src/task_manager.rs`、`schema.rs`、`parsing.rs`：给 `add_task` draft 增加可选前置任务 ID。
- `task_runner_service/backend/src/services/task_manager_bridge/task_ops.rs`：调整子任务落库字段。
- `task_runner_service/backend/src/services/task_manager_bridge/support.rs`：调整返回给 AI 的任务视图。
- `task_runner_service/frontend/src/pages/tasks`：任务列表加“子任务”按钮和子任务 Drawer。

## 最小改造方案

### 1. 创建时自动挂当前任务

保留现有 `create_followup_task_for_tool(root_task_id, run_id, draft)` 作为唯一入口：

- `root_task_id` 来自当前执行上下文。
- `run_id` 来自当前运行上下文。
- 子任务保存到 `tasks` 表。
- 服务端内部写入父任务和来源运行关系。
- 如果 draft 带了前置任务 ID，服务端写入现有前置任务关系。
- owner/tenant/subject/creator 从父任务继承。
- 模型配置留空。
- MCP 工具配置禁用或留空，不复制父任务工具选择。
- 不初始化独立 memory thread；如数据库字段必填，只保存占位值。
- AI 不传、也看不到这些字段。

### 2. 工具行为保持不变

内部 task MCP 仍然是：

- `add_task` 创建子任务。
- `list_tasks` 查看当前工具上下文里的任务。
- `update_task` 更新任务内容和状态。
- `complete_task` 完成任务。
- `delete_task` 删除任务。

不让 AI 理解 TaskRunner 的父子关系模型；只允许它在需要顺序关系时传已存在的前置任务 ID。

### 3. 返回给 AI 的字段做瘦身

给 AI 的任务视图只保留工具后续操作和业务判断需要的字段，例如：

- `id`
- `title`
- `details`
- `priority`
- `status`
- `tags`
- `due_at`
- `outcome_summary`
- `outcome_items`
- `resume_hint`
- `blocker_*`
- `created_at`
- `updated_at`

不返回：

- 父任务 id
- 来源运行 id
- owner/tenant/subject
- memory thread
- 模型配置
- MCP 工具配置
- 其他程序透传字段

### 4. 任务列表展示子任务

TaskRunner 任务列表每行加一个“子任务”按钮。

点击后打开 Drawer，通过现有 `parent_task_id` 查询当前任务下面的子任务，展示标题、状态、摘要、更新时间和前置关系数量。

### 5. Chatos 流程图后续展示

后续在流程图节点上加“子任务”按钮时，不需要改变 AI 工具逻辑。

后端提供一个按当前任务查子任务的内部能力即可，例如：

```text
GET /internal/chatos/message-tasks/:task_id/tool-subtasks
```

接口内部校验当前消息来源，然后按父任务关系查子任务，返回给前端展示标题、状态、摘要、更新时间等人类可读字段。

### 6. 验证点

- TaskRunner 执行任务时调用内部 `task_manager_add_task`，数据库里出现子任务。
- 子任务在数据库里能查到父任务关系和来源运行关系。
- draft 传入前置任务 ID 时，子任务能写入现有前置任务关系。
- AI 看到的工具结果不包含父任务、来源运行、owner、tenant 等内部字段。
- `list/update/complete/delete` 仍然按原来的 task MCP 方式工作。
- 任务列表点击“子任务”可以看到当前任务下的子任务。
- admin 能看到全部数据，普通用户只能看到自己的数据。
- Chatos 后续可按父任务查询并展示这些子任务。

## 不做

- 不新建一套子任务系统。
- 不新增 AI 必须理解的父子任务参数。
- 不把程序内部字段写进 prompt 或 MCP schema。
- 不改变现有 task MCP 的业务使用方式。
