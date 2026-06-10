# Task Runner 前置任务方案

## 背景

当前 Task Runner 已有 `parent_task_id` / `source_run_id`，但这组字段表示“某次任务运行中自动产生的后续任务”，不适合承载“执行当前任务前必须先完成哪些任务”的调度约束。

本方案新增独立的“前置任务”概念：

- 一个任务可以配置多个前置任务。
- 前置任务可以继续拥有自己的前置任务。
- 执行任务时，系统先递归执行未完成的前置任务。
- 只有所有直接和间接前置任务都成功完成，才执行当前任务。
- 任务依赖图不能成环。
- 当前任务请求 AI 时，需要把前置任务的执行结果注入到全局任务 prompt 中。

## 数据模型

新增任务依赖边表，不直接在 `tasks` 表里存数组，方便查询、去重和环检测。

SQLite migration:

```sql
CREATE TABLE IF NOT EXISTS task_prerequisites (
  task_id TEXT NOT NULL,
  prerequisite_task_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(task_id, prerequisite_task_id),
  FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY(prerequisite_task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_prerequisites_task_id
ON task_prerequisites(task_id);

CREATE INDEX IF NOT EXISTS idx_task_prerequisites_prerequisite_task_id
ON task_prerequisites(prerequisite_task_id);
```

MongoDB:

- Collection: `task_prerequisites`
- Unique index: `{ task_id: 1, prerequisite_task_id: 1 }`
- Index: `{ prerequisite_task_id: 1 }`

新增后端模型：

- `TaskPrerequisiteRecord`
  - `task_id`
  - `prerequisite_task_id`
  - `created_at`
- `TaskDependencyGraph`
  - `task_id`
  - `prerequisites`
  - `transitive_prerequisites`
  - `blocked_by`
  - `ready`

`TaskRecord` 可以增加只读展示字段：

- `prerequisite_task_ids: Vec<String>`
- `prerequisite_tasks: Option<Vec<TaskSummaryRecord>>`

存储层仍以边表为准，`TaskRecord` 上的字段只作为 API/页面展示聚合结果。

## API 设计

### 创建 / 更新任务

在 `CreateTaskRequest` / `UpdateTaskRequest` 中增加：

```json
{
  "prerequisite_task_ids": ["task-id-1", "task-id-2"]
}
```

保存时做校验：

- 任务 ID 必须存在。
- 不能依赖自己。
- 不能重复。
- agent 用户只能引用自己有权限访问的任务。
- 更新依赖后不能形成环。

### 独立依赖接口

建议新增：

- `GET /api/tasks/:id/prerequisites`
- `PUT /api/tasks/:id/prerequisites`
- `GET /api/tasks/:id/dependency-graph`

`PUT /api/tasks/:id/prerequisites` 覆盖当前任务的直接前置任务列表，适合页面多选保存。

### MCP 工具

`create_task` / `update_task` schema 增加：

```json
{
  "prerequisite_task_ids": {
    "type": "array",
    "items": { "type": "string" },
    "uniqueItems": true,
    "description": "当前任务执行前必须先成功完成的任务 ID 列表。只有这些任务及它们的前置任务全部成功后，当前任务才会执行。不要填写当前任务自身，也不要构造循环依赖。"
  }
}
```

新增 MCP 工具：

- `get_task_dependency_graph`
- `set_task_prerequisites`
- `create_tasks_with_prerequisites`

AI 在创建任务前如果需要引用现有任务，可以先 `list_tasks`，再设置 `prerequisite_task_ids`。

### AI 如何创建带前置任务的数据

模型调用工具时不能凭空填写任务 ID。所有 `prerequisite_task_ids` 必须来自以下来源：

- `list_tasks` / `get_task` 查询到的已有任务 ID。
- `create_task` 刚创建成功后返回的任务 ID。
- `create_tasks_with_prerequisites` 一次性创建任务组后由后端生成并返回的任务 ID。

模型绝不能自己编造 ID。工具 description 里需要明确写：

```text
prerequisite_task_ids 只能填写通过 list_tasks/get_task/create_task/create_tasks_with_prerequisites 返回的真实任务 ID。
如果你需要同时创建前置任务和当前任务，不要猜 ID，请使用 create_tasks_with_prerequisites，通过 client_ref 建立临时引用。
```

#### 场景 1：前置任务已经存在

AI 调用流程：

```text
list_tasks(keyword/status/tag)
  -> 找到已有前置任务 ID
create_task({
  title,
  objective,
  prerequisite_task_ids: ["existing-task-id-1", "existing-task-id-2"]
})
```

适用场景：

- 用户明确说“等任务 X 完成后再做 Y”。
- AI 可以通过任务列表找到 X。
- X 已经存在于任务系统。

#### 场景 2：前置任务不存在，需要先创建

可以用两步调用：

```text
create_task({ title: "先收集日志", objective: "..." })
  -> 返回 task_id = B
create_task({
  title: "分析日志并给出结论",
  objective: "...",
  prerequisite_task_ids: [B]
})
```

这个方式简单，但当一次要创建多层依赖时，模型需要多次调用工具，并且中途要记住返回 ID。

#### 场景 3：一次创建一组任务和依赖

建议新增 MCP 工具 `create_tasks_with_prerequisites`，让 AI 用临时引用创建任务图，后端负责生成真实 ID 并写入依赖边。

工具 schema 示例：

```json
{
  "tasks": [
    {
      "client_ref": "collect_logs",
      "title": "收集服务日志",
      "objective": "收集最近一次异常相关日志",
      "description": "从目标服务中整理错误日志和时间线",
      "prerequisite_refs": [],
      "prerequisite_task_ids": []
    },
    {
      "client_ref": "analyze_logs",
      "title": "分析日志并定位原因",
      "objective": "根据日志判断异常原因并给出修复建议",
      "description": "需要基于 collect_logs 的结果执行",
      "prerequisite_refs": ["collect_logs"],
      "prerequisite_task_ids": []
    },
    {
      "client_ref": "write_fix_plan",
      "title": "输出修复方案",
      "objective": "形成可执行的修复步骤",
      "prerequisite_refs": ["analyze_logs"],
      "prerequisite_task_ids": ["existing-task-id-from-list_tasks"]
    }
  ]
}
```

字段说明：

- `client_ref`: 本次工具调用内的临时任务引用，由 AI 自己起名，只在本次请求内有效。
- `prerequisite_refs`: 引用同一次请求中其它新建任务的 `client_ref`。
- `prerequisite_task_ids`: 引用系统里已经存在的真实任务 ID。

后端处理流程：

1. 校验 `client_ref` 必填且本次请求内唯一。
2. 校验 `prerequisite_refs` 都能在本次请求中找到。
3. 校验 `prerequisite_task_ids` 都是真实存在、当前用户有权限访问的任务。
4. 把 `client_ref` 图和已有任务 ID 图合并做环检测。
5. 在数据库事务中创建所有新任务。
6. 拿到真实 task id 后写入 `task_prerequisites` 边表。
7. 返回 `client_ref -> task_id` 映射和创建出的任务列表。

返回示例：

```json
{
  "created_tasks": [
    { "client_ref": "collect_logs", "task_id": "task-a", "title": "收集服务日志" },
    { "client_ref": "analyze_logs", "task_id": "task-b", "title": "分析日志并定位原因" },
    { "client_ref": "write_fix_plan", "task_id": "task-c", "title": "输出修复方案" }
  ],
  "dependency_edges": [
    { "task_id": "task-b", "prerequisite_task_id": "task-a" },
    { "task_id": "task-c", "prerequisite_task_id": "task-b" },
    { "task_id": "task-c", "prerequisite_task_id": "existing-task-id-from-list_tasks" }
  ]
}
```

这个工具能解决“AI 创建当前任务时还不知道前置任务真实 ID”的问题，也能减少多次工具调用造成的上下文丢失。

#### 推荐 MCP 工具选择规则

- 只创建一个任务，且前置任务已存在：使用 `create_task.prerequisite_task_ids`。
- 需要先创建前置任务，再创建当前任务：优先使用 `create_tasks_with_prerequisites`。
- 要给已有任务补前置任务：使用 `set_task_prerequisites`。
- 不确定有哪些任务可引用：先用 `list_tasks` / `get_task_dependency_graph` 查询。

## 环检测

依赖边方向定义：

```text
task_id -> prerequisite_task_id
```

表示 `task_id` 执行前必须先完成 `prerequisite_task_id`。

新增或更新依赖时执行 DFS：

1. 以待保存的边集构建邻接表。
2. 从当前任务出发遍历所有前置任务。
3. 如果遍历过程中再次遇到当前任务，拒绝保存。
4. 同时维护 `visiting` / `visited`，发现任意回边也拒绝。

错误示例：

```text
A -> B
B -> C
C -> A
```

保存 `C -> A` 时应返回：

```text
前置任务不能形成循环依赖: C -> A -> B -> C
```

建议依赖链最大深度先限制为 32，防止异常数据导致递归过深。

## 执行流程

当前 `RunService::start_run` 会直接为目标任务创建 run 并异步调用 `execute_run`。需要在创建当前任务 run 之前加入依赖解析。

推荐流程：

```text
start_run(task_id)
  acquire task start lock
  load task
  resolve prerequisite graph
  validate no cycle
  execute prerequisites in topological order
  if any prerequisite failed/cancelled/blocked:
    mark current task Blocked
    return blocked result without starting current AI run
  create current run
  execute current run with prerequisite result context
```

### 前置任务完成判定

第一版建议只有 `TaskStatus::Succeeded` 算完成。

- `Succeeded`: 跳过执行，读取最近一次成功 run 结果。
- `Draft` / `Ready` / `Failed` / `Cancelled` / `Blocked`: 需要尝试执行。
- `Running`: 等待正在运行的 run 完成，避免重复执行。
- `Archived`: 不能作为有效前置任务，当前任务进入 Blocked。

如果前置任务执行失败：

- 当前任务不请求 AI。
- 当前任务状态置为 `Blocked`。
- 写入 run event 或 task event：
  - `dependency_failed`
  - 包含失败的前置任务 ID、标题、状态、run_id、错误摘要。

### 多前置任务执行策略

第一版建议使用拓扑顺序串行执行，逻辑最清晰：

```text
C is prerequisite of B
B is prerequisite of A
D is prerequisite of A

execute C
execute B
execute D
execute A
```

后续可以优化为“同层并发执行”，但需要更复杂的并发锁、取消传播和错误聚合。第一版先保证正确性。

### 避免重复执行

执行某个前置任务前先检查：

- 如果任务状态是 `Succeeded` 且存在最近一次成功 run，则直接复用结果。
- 如果有 active run，则等待该 run 完成。
- 如果状态不是 `Succeeded`，调用内部 `start_run_internal` 同步等待完成。

为避免当前 `start_run` 只返回 queued run 后后台执行，建议拆出内部方法：

- `start_run`: HTTP/MCP 入口，保留现有异步返回 queued 行为。
- `run_task_with_dependencies`: 新增核心流程，可同步等待依赖完成。
- `execute_single_task_run`: 创建并执行单个任务 run，返回最终 run。

外部调用 `start_run(A)` 时，返回的 run 可以仍然是 A 的 run，但 A 的 run 在真正请求 AI 前先记录依赖准备事件。

## Prompt 注入

当前 prompt 在 `build_task_prompt(task, prompt_override)` 中生成。需要扩展为：

```rust
build_task_prompt(
    task,
    prompt_override,
    prerequisite_context,
)
```

即使传了 `prompt_override`，也必须注入前置任务结果。建议结构：

```text
前置任务执行结果:

1. [Succeeded] task-id / 标题
   目标:
   ...
   最近运行:
   run-id
   结果摘要:
   ...
   关键输出:
   ...

2. [Succeeded] ...

当前任务:

任务标题:
...

任务目标:
...
```

注入范围建议：

- 包含所有直接和间接前置任务。
- 按拓扑执行顺序排列。
- 每个前置任务最多注入：
  - `title`
  - `objective`
  - `result_summary`
  - `last_run_id`
  - run 的 `result_summary`
  - run 的 `report.content` 截断内容
- 默认每个前置任务最多 4000 字符，总长度最多 20000 字符。

如果总长度超限：

- 优先保留直接前置任务。
- 间接前置任务只保留摘要。
- 在 prompt 中标记 `内容已截断`。

同时把依赖上下文写入当前 run 的 `input_snapshot`：

```json
{
  "prerequisite_task_ids": [],
  "resolved_prerequisites": [
    {
      "task_id": "...",
      "run_id": "...",
      "status": "succeeded",
      "result_summary": "..."
    }
  ]
}
```

## 状态和事件

建议新增 run event 类型：

- `dependency_graph_resolved`
- `dependency_run_started`
- `dependency_run_finished`
- `dependency_waiting_active_run`
- `dependency_failed`
- `dependency_context_attached`

当前任务被依赖阻塞时：

- 如果还没创建当前 run：可以只更新 task 状态为 `Blocked` 并返回错误。
- 如果已创建当前 run：run 状态置为 `Blocked`，task 状态置为 `Blocked`。

为了前端体验，建议创建当前 run 后再执行依赖准备，这样页面能看到当前任务处于 queued/running，并看到依赖事件。

## Store 层改动

新增 trait 方法：

- `list_task_prerequisites(task_id) -> Vec<TaskPrerequisiteRecord>`
- `set_task_prerequisites(task_id, prerequisite_ids)`
- `list_tasks_depending_on(prerequisite_task_id)`
- `get_task_dependency_edges(task_ids)`
- `get_latest_successful_run(task_id)`
- `get_active_run_for_task(task_id)`

SQLite 需要补：

- 边表 CRUD。
- 依赖图查询。
- 最新成功 run 查询：

```sql
SELECT * FROM task_runs
WHERE task_id = ? AND status = 'succeeded'
ORDER BY datetime(finished_at) DESC, datetime(created_at) DESC
LIMIT 1;
```

Mongo 做同等实现。

## 前端改动

任务创建/编辑抽屉增加“前置任务”多选：

- 支持搜索任务标题。
- 已选择项显示状态 tag。
- 禁止选择自己。
- 如果后端返回会成环，保存时报错并展示错误。

任务详情页增加：

- 直接前置任务列表。
- 依赖图入口。
- 最近一次前置执行结果摘要。

列表页可增加：

- `前置` 列，显示数量。
- hover 展示前几个任务标题。

运行详情增加依赖事件流，方便看到当前任务为什么还没请求 AI。

## 调度任务行为

定时任务到期时也走同一套依赖流程：

- 如果前置任务未完成，先执行前置任务。
- 如果前置失败，当前任务 blocked，本次调度完成。
- 如果前置成功，再执行当前任务。

注意：如果前置任务自身也是定时任务，本次作为依赖执行时仍应按“立即执行一次”处理，不等待它自己的 schedule。

## 权限

管理员：

- 可依赖任意任务。

Agent 用户：

- 只能依赖自己创建的任务。
- 只能看到自己可访问任务的依赖图。
- 如果管理员给 agent 的任务配置了 agent 无权访问的前置任务，执行时后端仍可执行，但 MCP/API 返回给 agent 的依赖详情需要脱敏。第一版建议禁止这种配置，保持规则简单。

## 迁移兼容

已有任务默认没有前置任务，不影响当前执行。

新增字段和表均为增量迁移：

- SQLite migration `0011_task_prerequisites.sql`
- Mongo 启动时 ensure indexes
- 前端类型增加可选字段，旧数据兼容空数组。

## 测试建议

后端单元/集成测试：

- 创建任务 A 依赖 B。
- A 依赖 B、C，B 依赖 D，执行顺序为 D/B/C/A。
- B 已 succeeded 时，执行 A 不重复执行 B。
- B running 时，执行 A 等待 B 完成。
- B failed 时，A blocked 且不请求 AI。
- 环检测：
  - A -> A
  - A -> B -> A
  - A -> B -> C -> A
- agent 不能依赖别人的任务。
- `create_task.prerequisite_task_ids` 只能接受真实存在且有权限的任务 ID。
- `create_tasks_with_prerequisites` 可以用 `client_ref` 一次创建多层依赖任务。
- `create_tasks_with_prerequisites` 拒绝重复 `client_ref`、未知 `prerequisite_refs` 和循环依赖。
- prompt 中包含前置任务结果。
- prompt_override 下仍包含前置任务结果。

前端 type-check：

- 任务表单保存前置任务。
- 任务详情展示前置任务。

## 实施顺序

1. 后端模型和迁移：新增 `task_prerequisites` 表/collection、索引、store trait 方法。
2. API/MCP：创建/更新任务支持 `prerequisite_task_ids`，新增依赖查询和设置接口。
3. MCP 批量建图：新增 `create_tasks_with_prerequisites`，支持 `client_ref` / `prerequisite_refs` / `prerequisite_task_ids`，解决 AI 创建新前置任务时没有真实 ID 的问题。
4. 环检测：保存依赖前统一校验。
5. 执行核心：抽出 `run_task_with_dependencies`，递归解析依赖并按拓扑顺序执行。
6. Prompt 注入：扩展 `build_task_prompt`，把依赖结果写入 prompt 和 input snapshot。
7. 前端：任务创建/编辑多选前置任务，详情页展示依赖信息。
8. 测试和编译检查。

## 待确认点

- “完成”是否只认 `Succeeded`，还是允许人工标记为某种 `Done` 状态后跳过执行。建议第一版只认 `Succeeded`。
- 多个无依赖关系的前置任务是否需要并发执行。建议第一版串行，后续再优化。
- 前置任务结果注入是否包含完整 report.content。建议第一版截断注入，完整内容保留在 run/report 中。
- 当前任务依赖失败时，是创建一条 blocked run，还是只把 task 置为 Blocked。建议创建 run，便于页面展示原因和事件流。
