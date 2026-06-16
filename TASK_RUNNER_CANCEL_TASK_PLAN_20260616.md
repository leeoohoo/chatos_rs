# Task Runner 任务取消能力方案

## 背景

当前 Chatos 联系人异步模式会把用户消息拆成 Task Runner 后台任务。比如用户先说“加两个接口”，联系人会通过任务系统建立对应任务；如果用户随后改口说“其中一个接口不要了，换成另外两个接口”，或者“两项都不要了”，联系人需要能及时终止已经安排但与当前意图冲突的任务。

现在的问题是：联系人异步 profile 里没有任务级取消工具。已有 `cancel_run` 只能取消某一次运行记录，而且没有暴露给 Chatos async planner；`delete_task` 也不是正确语义，因为删除会丢掉审计和回调上下文。

目标是新增一个独立的任务取消工具：必须带取消理由，取消理由要回调给 Chatos；被取消的任务不能再次启动，也不能再作为前置任务。

## 当前 Task Runner 暴露给 Chatos 的工具

### Chatos async planner profile 当前可用工具

当 MCP 请求带有 Chatos message source context 时，会走 `McpToolProfile::ChatosAsyncPlanner`，当前允许工具来自 `task_runner_service/backend/src/mcp_server/chatos_async_planner/access.rs`：

- `list_tasks`
- `get_task`
- `get_task_stats`
- `create_task`
- `list_mcp_builtin_catalog`
- `create_tasks_with_prerequisites`
- `update_task`
- `set_task_prerequisites`
- `wait_for_task_completion`
- `get_task_dependency_graph`

这组工具可以创建、查询、更新任务内容、设置依赖、等待异步接收，但不能取消任务。

### 默认 agent profile 还有的任务/运行工具

默认 agent profile 还允许：

- 任务侧：`delete_task`、`batch_update_task_status`、`batch_delete_tasks`
- 运行侧：`list_runs`、`get_run`、`start_task_run`、`batch_start_task_runs`、`cancel_run`、`retry_run`、`list_run_events`
- 记忆和 prompt 相关工具：`get_task_memory_context`、`list_task_memory_records`、`summarize_task_memory`、`list_prompts`、`get_prompt`、`submit_prompt`、`cancel_prompt`

但这些不是 Chatos async planner 当前能用的能力；并且 `cancel_run` 不是任务级取消，不能表达“这个任务本身已经不符合用户最新意图”。

## 设计目标

1. 新增独立 MCP 工具 `cancel_task`，暴露给 Chatos async planner。
2. `cancel_task` 必须要求 `task_id` 和非空 `reason`。
3. 只有待执行/执行中的任务可以取消。第一版明确限定为 `ready`、`queued`、`running`。
4. 已经成功的任务不允许作废、不允许取消；其他非待执行状态也不能通过 `cancel_task` 改成取消。
5. 取消任务后，任务进入终态 `cancelled`，保留审计信息，不删除记录。
6. 取消理由随 `task.cancelled` 回调传给 Chatos。
7. 已取消任务不能再启动、不能被 retry、不能被 scheduler 启动、不能被作为前置任务引用。
8. 如果任务已有 queued/running run，取消任务时同步请求取消 active run。
9. 取消一个被其他任务依赖的任务时，程序自动级联取消所有依赖它且仍处于待执行/执行中的下游任务。
10. 如果 running run 后续才结束，不能覆盖任务的 `cancelled` 终态。
11. `update_task` 和 `batch_update_task_status` 不允许绕过 `cancel_task` 写入 `cancelled`。
12. 对外 skill 要明确引导联系人：用户意图改变时，只需要取消与新意图冲突的任务；依赖它的下游任务会由程序级联取消。

## 新增工具契约

### MCP tool: `cancel_task`

输入：

```json
{
  "task_id": "真实任务 ID",
  "reason": "取消原因，必须说明为什么该任务已不符合用户最新意图"
}
```

建议 schema：

- `task_id`: string, required, minLength 1
- `reason`: string, required, minLength 1, maxLength 1000
- `replacement_task_ids`: string[], optional，后续如果联系人先创建替代任务，可以记录“由哪些任务替代”

输出：

```json
{
  "cancelled": true,
  "task_id": "...",
  "status": "cancelled",
  "reason": "...",
  "active_run_ids": ["..."],
  "cascade_cancelled_task_ids": ["..."],
  "callback_event": "task.cancelled"
}
```

### REST API

新增：

```text
POST /api/tasks/:id/cancel
```

body:

```json
{
  "reason": "用户改为只需要另外两个接口，原接口不再需要",
  "replacement_task_ids": []
}
```

REST API 和 MCP 工具走同一个 service 方法，避免语义分叉。

## 数据模型

`TaskStatus` 已经有 `Cancelled`，不需要新增状态枚举。

建议扩展 `TaskToolState`，避免新增任务表列：

```rust
pub struct TaskToolState {
    ...
    pub cancel_reason: Option<String>,
    pub cancelled_at: Option<String>,
    pub cancelled_by_user_id: Option<String>,
    pub cancelled_by_username: Option<String>,
    pub cancelled_by_display_name: Option<String>,
    pub replacement_task_ids: Vec<String>,
    pub cancelled_because_task_id: Option<String>,
    pub cascade_root_task_id: Option<String>,
}
```

取消时写入：

- `task.status = TaskStatus::Cancelled`
- `task.result_summary = Some(format!("任务已取消：{reason}"))`
- `task.task_tool_state.cancel_reason = Some(reason)`
- `task.task_tool_state.cancelled_at = Some(now)`
- `task.task_tool_state.cancelled_by_* = current_user`
- 级联取消的下游任务额外写入 `cancelled_because_task_id` 和 `cascade_root_task_id`
- `task.updated_at = now`

如果未来需要按取消时间筛选，再把 `cancelled_at` 提升为独立列；第一版可以放在 `task_tool_state_json`。

## 服务层行为

新增 `TaskService::cancel_task(id, request, current_user)`，核心规则：

1. 校验任务存在、当前用户有权访问。
2. 校验 `reason.trim()` 非空。
3. 如果任务已经 `cancelled`：
   - 若已有取消理由，返回当前任务，保持幂等。
   - 不允许覆盖原取消理由，避免审计被冲掉。
4. 只允许 `ready`、`queued`、`running`。
5. 如果任务已经 `succeeded`：
   - 返回错误，明确说明成功任务不允许作废或取消。
6. 如果任务是 `draft`、`failed`、`blocked`、`archived` 或其他非待执行状态：
   - 返回错误，说明只有待执行/执行中的任务允许取消。
7. 查找该任务 active run：
   - queued run：直接置为 run `cancelled`，写 run event。
   - running run：写 `cancel_requested = true`，让执行循环尽快中止。
8. 保存 task 为 `cancelled`。
9. 反向查找所有直接或间接依赖该任务的下游任务。
10. 对每个下游任务：
    - 如果状态是 `ready`、`queued`、`running`，用程序级联取消。
    - 如果状态是 `succeeded`，保持不变，成功任务绝不作废。
    - 如果状态是 `draft`、`failed`、`blocked`、`archived`，保持不变。
    - 级联取消理由使用根任务理由派生，例如：`前置任务 {task_id} 已取消：{reason}`。
    - 如果下游任务有 queued/running run，同样取消或请求取消 active run。
11. 对根任务和被级联取消的下游任务分别发送 `task.cancelled` callback 到 Chatos。

## 必须加的硬校验

### 1. 禁止再次启动 cancelled 任务

在这些入口统一加 guard：

- `RunService::start_run_with_trigger`
- `RunService::queue_dependency_run`
- `RunService::retry_run` 依赖 `start_run` guard 即可，但测试要覆盖
- `batch_start_runs` 通过 `start_run` 得到逐项失败
- scheduler 的 `list_due_scheduled_tasks` 查询要排除 `cancelled`

错误示例：

```text
任务已取消，不能再次启动: {task_id}
```

### 2. 禁止 cancelled 任务作为前置任务

在 `TaskService::validate_task_prerequisites` 中，读取每个 prerequisite 后检查：

```rust
if prerequisite.status == TaskStatus::Cancelled {
    return Err(format!("已取消任务不能作为前置任务: {prerequisite_task_id}"));
}
```

覆盖入口：

- `create_task.prerequisite_task_ids`
- `create_tasks_with_prerequisites.prerequisite_task_ids`
- `update_task.patch.prerequisite_task_ids`
- `set_task_prerequisites`

如果某个任务取消前已经被其他任务引用，不强行删除边；取消入口会通过反向依赖解析自动级联取消待执行/执行中的下游任务。若因为历史数据或并发窗口仍出现“待启动任务引用了 cancelled prerequisite”，执行入口必须拒绝启动并说明“前置任务已取消”。

### 3. 取消被依赖任务时级联取消下游任务

新增反向依赖解析能力：

- 根据 `task_prerequisites.prerequisite_task_id = cancelled_task_id` 找到直接下游任务。
- 继续递归找到所有间接下游任务。
- 限制最大遍历数量，例如 500，避免异常依赖图拖垮取消请求。
- 按拓扑顺序或 BFS 顺序逐个处理，保证每个下游任务只处理一次。

级联取消规则：

- `ready`、`queued`、`running`：取消。
- `succeeded`：不取消、不作废。
- `draft`、`failed`、`blocked`、`archived`：不取消。
- 下游任务如果已经有 active run，按根任务同样的 run 取消规则处理。
- 级联取消的 task callback 应带上 `cancelled_because_task_id` 和 `cascade_root_task_id`，方便 Chatos 展示“因为前置任务取消而取消”。

### 4. 禁止绕过 cancel_task 改成 cancelled

修改 `TaskService::update_task`：

- 继续禁止 `queued` / `running`
- 新增禁止 `cancelled`
- 提示“请使用 cancel_task 并提供取消原因”

`batch_update_status` 如果目标状态是 `cancelled`，直接失败，不逐项写状态。

### 5. 保护取消终态不被运行结束覆盖

当前 `finalize_model_phase` 会根据 run 结果回写 task status。需要在回写前重新读取 task：

- 如果当前 task 已经是 `cancelled`，保持 `cancelled`。
- 不覆盖 `cancel_reason`、`cancelled_at` 等字段。
- run 可以按实际状态保存为 `cancelled` 或失败，但 task 不能被改回 `succeeded` / `failed`。
- Chatos `task.cancelled` 已经由 `cancel_task` 发送过时，后续 run terminal callback 不再重复发送同一个取消通知。

## Chatos 回调

当前回调 payload 在 `task_runner_service/backend/src/services/chatos_callbacks.rs`，事件名已有：

- `task.completed`
- `task.failed`
- `task.cancelled`
- `task.blocked`

建议复用 `task.cancelled`，扩展 payload：

```rust
struct ChatosTaskCallbackPayload {
    ...
    cancel_reason: Option<String>,
    cancelled_at: Option<String>,
    cancelled_by_user_id: Option<String>,
    cancelled_by_username: Option<String>,
    cancelled_by_display_name: Option<String>,
    replacement_task_ids: Vec<String>,
    cancelled_because_task_id: Option<String>,
    cascade_root_task_id: Option<String>,
}
```

取消任务时立即发送：

```json
{
  "event": "task.cancelled",
  "task_id": "...",
  "status": "cancelled",
  "task_title": "...",
  "cancel_reason": "用户最新消息取消了该接口，改为实现另外两个接口",
  "cancelled_because_task_id": null,
  "cascade_root_task_id": null,
  "source_session_id": "...",
  "source_turn_id": "...",
  "source_user_message_id": "...",
  "callback_at": "..."
}
```

Chatos 收到后应能把对应用户消息上的任务状态更新为 cancelled，并展示/记录取消理由。

## 对外 skill 更新

需要更新：

- `task_runner_service/TASK_RUNNER_AI_SKILL.zh-CN.md`
- `task_runner_service/TASK_RUNNER_AI_SKILL.en-US.md`

新增规则建议：

1. 用户追问、改口、缩小范围、替换需求时，先用 `list_tasks` / `get_task` / `get_task_dependency_graph` 找到已有安排。
2. 如果已有任务与用户最新意图冲突，调用 `cancel_task`，并写清楚取消理由。
3. 取消理由要面向审计和回调可读，例如：“用户最新消息明确不再需要接口 A，改为实现接口 C、D。”
4. 取消冲突任务后，再创建替代任务或调整剩余任务依赖。
5. 只需要取消用户明确不再需要的根任务；如果其他待执行/执行中任务依赖它，程序会自动级联取消，不能为了级联再额外编造取消理由。
6. 不要用 `delete_task` 表达用户改意图；删除只用于误建且尚未形成有效安排的记录。
7. 不要用 `update_task` 修改执行状态；取消必须走 `cancel_task`。
8. `wait_for_task_completion` 要放在取消、更新、新建、依赖关系都处理完之后调用。

示例补充：

> 用户说“刚才那个用户导出接口不要了，换成订单导出和账单导出”。先定位“用户导出接口”任务，调用 `cancel_task`，理由写明用户已取消该接口；如果其他待执行/执行中任务依赖它，系统会级联取消。然后创建“订单导出接口”和“账单导出接口”的新任务，并按需要安排 review 任务。

## 代码改动位置

预计涉及：

- `task_runner_service/backend/src/models/task/config.rs`
  - 扩展 `TaskToolState`
- `task_runner_service/backend/src/models/task/requests.rs`
  - 新增 `CancelTaskRequest`
- `task_runner_service/backend/src/services/task_service/tasks/mutations/`
  - 新增 `cancellation.rs`
  - `updates.rs` 禁止 `status=cancelled`
- `task_runner_service/backend/src/services/task_dependencies.rs`
  - 禁止 cancelled prerequisite
  - 新增反向依赖/下游任务解析
- `task_runner_service/backend/src/store/*/tasks/prerequisites.rs`
  - 新增按 `prerequisite_task_id` 查询 dependent task ids 的存储方法
- `task_runner_service/backend/src/services/run_control/start.rs`
  - start guard
- `task_runner_service/backend/src/services/run_prerequisites/dependency_runs/queueing.rs`
  - prerequisite run guard
- `task_runner_service/backend/src/services/run_model_phase/completion.rs`
  - 防止运行结束覆盖 cancelled task
- `task_runner_service/backend/src/store/*/tasks/listing*`
  - due scheduled query 排除 cancelled
- `task_runner_service/backend/src/services/chatos_callbacks.rs`
  - payload 增加取消字段
- `task_runner_service/backend/src/services/chatos_callbacks/payload.rs`
  - 从 `TaskToolState` 填充取消字段
- `task_runner_service/backend/src/mcp_server/entrypoints/tool_definitions/tasks.rs`
  - 新增 `cancel_task` tool schema
- `task_runner_service/backend/src/mcp_server/task_tools.rs`
  - dispatch `cancel_task`
- `task_runner_service/backend/src/mcp_server/dispatch.rs`
  - 注册 `cancel_task`
- `task_runner_service/backend/src/mcp_server/support/access.rs`
  - default profile allowlist 增加 `cancel_task`
- `task_runner_service/backend/src/mcp_server/chatos_async_planner/access.rs`
  - async planner allowlist 增加 `cancel_task`
- `task_runner_service/backend/src/api/tasks/mutations.rs`
  - 新增 REST handler
- `task_runner_service/backend/src/api/router.rs`
  - 新增 `POST /api/tasks/:id/cancel`
- `task_runner_service/TASK_RUNNER_AI_SKILL.zh-CN.md`
- `task_runner_service/TASK_RUNNER_AI_SKILL.en-US.md`

## 测试清单

### MCP / profile

- `cancel_task` 出现在 async planner profile。
- 非 admin agent 只能取消自己创建的任务。
- `cancel_task` 缺少 reason 返回错误。
- `update_task status=cancelled` 返回错误。
- `batch_update_task_status status=cancelled` 返回错误。

### 状态与运行

- ready 任务取消后变成 cancelled。
- draft/failed/blocked/archived/succeeded 任务调用 `cancel_task` 失败，其中 succeeded 明确提示“不允许作废或取消”。
- queued run 对应任务取消后，run 立即变成 cancelled。
- running run 对应任务取消后，run `cancel_requested=true`，task 立即 cancelled。
- running run 晚结束时不能把 task 从 cancelled 覆盖成 succeeded/failed。
- cancelled task 调 `start_task_run` 失败。
- cancelled task 调 `retry_run` 失败。
- cancelled scheduled task 不会被 scheduler 捞起。

### 依赖

- create/update/set prerequisites 引用 cancelled task 失败。
- 取消被依赖的 ready/queued/running 任务时，所有 ready/queued/running 下游任务被级联取消。
- 级联取消不会取消 succeeded 下游任务，也不会作废任何已经成功的任务。
- 级联取消下游 running 任务时，对 active run 写入取消请求。
- 如果历史数据或并发窗口导致启动时遇到 cancelled prerequisite，下游执行被拒绝，错误说明前置任务已取消。
- `get_task_dependency_graph` 能显示 cancelled prerequisite 在 blocked_by 中。

### 回调

- Chatos 来源任务取消时发送 `task.cancelled`。
- payload 包含 `cancel_reason`、`cancelled_at`、`cancelled_by_*`。
- 没有 `source_user_message_id` 的普通任务不发 Chatos callback，但状态仍保存。
- task-level cancel 已发 callback 后，run 终止不重复生成第二条 Chatos 取消消息。

### skill

- zh-CN/en-US skill 都包含“用户改意图时使用 `cancel_task`”规则。
- skill 明确不建议用 `delete_task` 表达正常取消。
- skill 明确取消后再创建替代任务，并在最后调用 `wait_for_task_completion`。

## 实施顺序

1. 加模型/request 类型和 `TaskService::cancel_task`。
2. 加 REST API 和 MCP `cancel_task`。
3. 加反向依赖解析和级联取消。
4. 加 start、scheduler、dependency、update status 的硬校验。
5. 加 Chatos callback payload 字段和取消回调。
6. 更新 async planner allowlist、schema enrich 和相关 tests。
7. 更新对外 skill。
8. 跑 `cargo test -p task_runner_service_backend`，重点补齐取消和依赖相关单测。

## 已定规则

1. 已经 `succeeded` 的任务不允许被作废，不允许被取消。
2. `cancel_task` 只允许取消 `ready`、`queued`、`running`。
3. 取消一个被其他任务依赖的任务时，程序自动级联取消依赖它的 `ready`、`queued`、`running` 下游任务。
4. AI 只需要取消用户明确不要的那个任务；skill 说明系统会级联取消下游任务。
5. 不需要 `batch_cancel_tasks`。
