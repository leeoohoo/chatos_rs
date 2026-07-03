# 项目任务支持规划任务类型实施方案

## 背景

当前系统里已经有一层 TaskRunner 规划任务：

- Chatos Plan 模式通过 TaskRunner MCP 创建 `task_profile = "chatos_plan"` 的 TaskRunner 任务。
- 这个 TaskRunner 规划任务运行时加载 Project Management MCP，并通过 `create_project_task` 写入 Project Management 的项目任务。
- TaskRunner 的规划任务运行期已经根据 `task_profile = "chatos_plan"` 固定加载 Project Management、TaskManager、AskUser 等规划所需 MCP。

现在缺的是第二层语义：Project Management 里的 `project_task` / `project_work_item` 本身也可能是“继续规划/拆解”的任务，而不是普通实现任务。后续执行项目任务时，必须根据这个类型把 TaskRunner 任务创建成规划任务，否则它会按普通执行任务运行，拿不到规划任务的 Project Management MCP 写入能力。

## 目标

1. Project Management MCP 创建项目任务时可以声明“是否是规划任务”。
2. Project Management 的项目任务记录持久化该字段，并在列表、详情、依赖图、Project Plan 返回里暴露。
3. Chatos 执行项目任务时读取该字段；如果为规划任务，创建 TaskRunner 任务时写入 `task_profile = "chatos_plan"`。
4. 规划型项目任务执行时必须带齐项目上下文、来源消息上下文、异步调度和 Project Management MCP 内部认证所需上下文。
5. 普通项目任务行为保持不变。

## 字段设计

新增字段：

```rust
is_planning_task: bool
```

对外 JSON 使用同名 snake_case：`is_planning_task`。

含义：

- `false`：普通实现/执行项目任务，执行时创建 TaskRunner `task_profile = "default"`。
- `true`：规划型项目任务，执行时创建 TaskRunner `task_profile = "chatos_plan"`，其目标是继续细化需求、补充技术文档、创建或调整项目任务和依赖。

默认值必须是 `false`，保证历史数据兼容。

## Project Management MCP 改动

这是主入口，必须优先改。

### 1. MCP 参数模型

文件：

- `crates/chatos_project_mcp_contract/src/args.rs`

修改：

- `CreateProjectTaskArgs` 增加：

```rust
#[serde(default)]
pub is_planning_task: bool,
```

- `UpdateProjectTaskPatch` 增加：

```rust
pub is_planning_task: Option<bool>,
```

### 2. MCP schema

文件：

- `crates/chatos_project_mcp_contract/src/schemas.rs`

修改：

- 增加 `boolean_field` helper。
- `create_project_task` 的 input schema 增加 `is_planning_task`。
- `update_project_task.patch` 目前是 `additionalProperties: true` 的宽 patch，建议改成显式 patch schema，至少要在描述里声明可更新 `is_planning_task`；更好的做法是补齐 patch properties，避免模型传错字段。
- `list_project_tasks` 可以增加可选过滤：

```json
{ "is_planning_task": true | false | null }
```

这样 AI 能只查规划型或普通项目任务。

字段描述建议：

> Whether this project task is itself a planning/decomposition task. Set true only when executing it should continue project planning through TaskRunner chatos_plan profile; leave false for implementation work.

中文 skill 里也要明确：

> 如果项目任务的目标是继续拆解需求、补充技术方案、创建更多项目任务或调整依赖，创建时必须设置 `is_planning_task: true`；如果目标是实现、修复、测试、交付代码或文档，则保持 `false`。

### 3. MCP 工具实现

文件：

- `project_management_service/backend/src/mcp_tools.rs`

修改：

- `create_project_task` 调 `CreateProjectWorkItemRequest` 时写入 `is_planning_task: args.is_planning_task`。
- `update_project_task` 的 `UpdateProjectWorkItemRequest::from(UpdateProjectTaskPatch)` 带上 `is_planning_task`。
- 若新增 `list_project_tasks.is_planning_task` 过滤，传入 store 查询条件。

建议约束：

- 已 `done` 的项目任务仍不可改。
- 已有关联 TaskRunner 执行任务且处于 active 状态时，不允许切换 `is_planning_task`。
- 没有关联或尚未执行的任务允许修正类型。

## Project Management 数据模型和存储

### 1. Rust 模型

文件：

- `project_management_service/backend/src/models/work_items.rs`

修改：

- `ProjectWorkItemRecord` 增加：

```rust
#[serde(default)]
pub is_planning_task: bool,
```

- `CreateProjectWorkItemRequest` 增加 `#[serde(default)] pub is_planning_task: bool`。
- `UpdateProjectWorkItemRequest` 增加 `pub is_planning_task: Option<bool>`。

### 2. SQLite

文件：

- `project_management_service/backend/migrations/0001_init.sql`
- `project_management_service/backend/src/store/sqlite.rs`
- `project_management_service/backend/src/store/sqlite_rows.rs`
- `project_management_service/backend/src/store/sqlite/work_items.rs`

修改：

- `project_work_items` 增加：

```sql
is_planning_task INTEGER NOT NULL DEFAULT 0
```

- `run_migrations()` 里增加兼容迁移，已有库通过 `ALTER TABLE` 补列。
- `work_item_from_row` 读取该列；使用 `try_get` 兼容旧库。
- `save_work_item` insert/update 带上该列。
- 如做列表过滤，SQL 增加 `AND (? IS NULL OR is_planning_task = ?)`。

### 3. Mongo

文件：

- `project_management_service/backend/src/store/mongo/work_items.rs`

修改：

- 创建记录时写入 `is_planning_task`。
- Mongo 历史记录依赖 `#[serde(default)]` 自动兼容缺失字段。
- 如做列表过滤，Mongo filter 增加 `{ "is_planning_task": true/false }`。

## 创建规划型项目任务时除了类型还需要什么

只加 `is_planning_task` 不够。规划型项目任务后续执行时，必须能创建一个完整可运行的 TaskRunner 规划任务。

### Project Management MCP 创建项目任务时仍需保存

`create_project_task` 当前已有的执行配置仍然需要：

- `task_runner_default_model_config_id`：规划任务执行模型。
- `task_runner_enabled_tool_ids`：执行配置中选择的工具；对于 `chatos_plan` 运行期，TaskRunner 会固定注入规划所需 builtin，但该字段仍用于权限校验、UI 展示和外部工具选择。
- `task_runner_skill_ids`：可选，用于规划任务运行时加载特定 skill。
- `requirement_id`、`title`、`description`：规划任务要知道它正在规划哪个需求下的哪部分内容。
- `prerequisite_project_task_ids`：如果这个规划任务必须等其他项目任务完成后才能继续拆解，需要保留依赖。

### 执行时创建 TaskRunner 任务必须带

当 `project_work_item.is_planning_task == true`：

- `task_profile = "chatos_plan"`。
- `project_id = <当前 concrete project id>`。
- `status = "ready"`。
- `schedule.mode = "contact_async"`。
- `schedule.run_at = now`。
- `source_session_id`、`source_user_message_id`、`source_turn_id`。
- 请求 header 继续带：
  - `X-Chatos-Session-Id`
  - `X-Chatos-User-Message-Id`
  - `X-Chatos-Turn-Id`
  - `X-Chatos-User-Authorization`
- `input_payload` 至少包含：
  - `source = "chatos_project_requirement_execution"`
  - `project_id`
  - `project_root`
  - `requirement_id`
  - `project_task_id`
  - `is_planning_task = true`
  - `source_session_id`
  - `source_user_message_id`
  - `source_turn_id`
- `default_model_config_id` 使用项目任务保存的模型。
- `mcp_config.workspace_dir` 使用项目根目录。
- `mcp_config.builtin_prompt_locale` 使用当前会话 locale。
- `mcp_config.skill_ids` 使用项目任务保存的 skill ids。
- `prerequisite_task_ids` 使用项目任务依赖转换出的 TaskRunner 前置任务。

这样 TaskRunner 执行时才能识别为规划任务，并在运行期注入 Project Management MCP。Project Management MCP 内部认证分支依赖 TaskRunner 运行期带：

- `X-Project-Service-Sync-Secret`
- `X-Task-Runner-Owner-User-Id`
- `X-Task-Runner-Owner-Username`
- `X-Task-Runner-Owner-Display-Name`
- `X-Chatos-Project-Id`
- `X-Task-Runner-Task-Profile: chatos_plan`

这些 header 已由 TaskRunner 的规划任务 Project Management provider 负责构造，所以消费方关键是把 TaskRunner task 的 `task_profile` 创建正确。

## Chatos 执行链路改动

文件：

- `chat_app_server_rs/src/api/projects/requirement_execution/types.rs`
- `chat_app_server_rs/src/api/projects/requirement_execution/plan.rs`
- `chat_app_server_rs/src/api/projects/requirement_execution/tasks.rs`

修改：

- `WorkItemPlanItem` 增加 `is_planning_task: bool`。
- `parse_work_items` 同时读取：
  - `is_planning_task`
  - `isPlanningTask`
- `create_and_start_execution_tasks` 创建 TaskRunner task 时：

```rust
let task_profile = if work_item.is_planning_task {
    "chatos_plan"
} else {
    "default"
};
```

- 当前代码固定写：

```rust
task_profile: Some("default".to_string()),
```

需要改成按字段判断。

- `input_payload` 增加 `is_planning_task`。
- tag 可以额外追加：
  - 普通任务：`project_requirement_execution`
  - 规划任务：`project_requirement_execution`, `project_planning_task`

## Project Management 普通 API 改动

虽然 MCP 是主入口，但普通 API 也要兼容，否则 UI 或脚本创建的项目任务无法表达类型。

文件：

- `project_management_service/backend/src/api/work_items.rs`
- `project_management_service/backend/src/api/task_runner_links.rs`
- `project_management_service/backend/src/task_runner_api_client.rs`

修改：

- `CreateProjectWorkItemRequest` / `UpdateProjectWorkItemRequest` 已带字段后，普通 API 自动可写。
- `/api/work-items/:work_item_id/task-runner-task` 这条直接从项目任务创建 TaskRunner task 的接口也要读取 `work_item.is_planning_task`。
- `project_management_service/backend/src/task_runner_api_client.rs` 内部的 `CreateTaskRunnerTaskRequest` 增加：
  - `task_profile`
  - `status`
  - `schedule`
  - 必要时补 `source_turn_id`
- 如果 `is_planning_task = true`，该接口也要传 `task_profile = "chatos_plan"`、`status = ready`、`schedule.mode = contact_async`。

如果确认所有执行都只走 Chatos 的需求执行接口，这条可以稍后做；但为了避免隐藏入口行为不一致，建议同一轮补齐。

## 前端展示和创建

文件：

- `project_management_service/frontend/src/types.ts`
- `project_management_service/frontend/src/pages/projectDetail/ProjectDetailOverlays.tsx`
- `project_management_service/frontend/src/pages/projectDetail/utils.ts`
- `project_management_service/frontend/src/pages/projectDetail/columns.tsx`
- `chat_app/src/lib/api/client/types/project.ts`
- `chat_app/src/components/projectExplorer/ProjectPlanPane.tsx`

修改：

- 类型增加 `is_planning_task?: boolean`。
- 新建项目任务弹窗增加开关“规划任务”。
- 项目任务列表/详情展示一个标签，例如“规划”。
- ChatOS 项目 Plan 面板展示规划型项目任务，避免用户误以为它是实现任务。

## Skill 文档改动

文件：

- `project_management_service/PROJECT_MANAGEMENT_MCP_SKILL.zh-CN.md`
- `project_management_service/PROJECT_MANAGEMENT_MCP_SKILL.en-US.md`
- 如 TaskRunner 规划任务 skill 中也描述 Project Management 写入规则，则同步更新：
  - `task_runner_service/TASK_RUNNER_PLAN_TASK_SKILL.zh-CN.md`
  - `task_runner_service/TASK_RUNNER_PLAN_TASK_SKILL.en-US.md`

需要补充规则：

- 创建项目任务时先判断它是“实现/执行”还是“继续规划/拆解”。
- 如果任务目标是继续规划、拆分、补技术方案、生成更多项目任务、调整依赖，设置 `is_planning_task: true`。
- 如果任务目标是编码、测试、修复、文档落地、部署等执行工作，设置 `false`。
- 规划型项目任务也必须挂在具体 requirement 下，也要有明确目标和验收口径，不能用模糊标题如“继续规划”。

## 测试计划

后端单测：

- `crates/chatos_project_mcp_contract`：
  - `create_project_task` schema 暴露 `is_planning_task`。
  - args 能反序列化缺省值为 `false`，显式 `true` 生效。
- `project_management_service/backend`：
  - MCP `create_project_task` 写入 `is_planning_task = true`。
  - MCP `update_project_task` 可在未执行前切换字段。
  - SQLite 旧库缺列时迁移后读取默认 `false`。
  - Mongo 缺字段记录序列化为 `false`。
- `chat_app_server_rs`：
  - `parse_work_items` 能读取 `is_planning_task` / `isPlanningTask`。
  - 创建执行任务时，规划型项目任务传 `task_profile = "chatos_plan"`。
  - 普通项目任务仍传 `task_profile = "default"`。

集成验证：

1. Chatos Plan 模式创建一个普通项目任务，确认 `is_planning_task = false`，执行时 TaskRunner task 为 `default`。
2. Chatos Plan 模式创建一个规划型项目任务，确认 Project Management 记录 `is_planning_task = true`。
3. 执行该项目任务，确认 TaskRunner task：
   - `task_profile = "chatos_plan"`
   - `project_id` 为当前项目
   - `source_session_id/source_user_message_id/source_turn_id` 存在
   - `schedule.mode = contact_async`
4. 运行中的规划型项目任务能调用 Project Management MCP 的 `create_project_task` / `update_project_task`。
5. 回调后 Project Management 的项目任务状态和 link 正常更新。

## 实施顺序

1. 先改 Project Management MCP contract：args、schema、skill 文档。
2. 改 Project Management backend 模型、SQLite/Mongo 存储、MCP 工具实现。
3. 改 Project Plan/列表返回，确保 Chatos 能读到 `is_planning_task`。
4. 改 Chatos 需求执行链路，按字段映射 TaskRunner `task_profile`。
5. 补 Project Management 普通 API 直连 TaskRunner 的一致性。
6. 补前端展示和创建开关。
7. 跑后端单测和一次端到端手工验证。

