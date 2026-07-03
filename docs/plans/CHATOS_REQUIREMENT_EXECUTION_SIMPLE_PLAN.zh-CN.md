# Chatos 需求执行简化方案实施计划

## 目标

点击项目 Plan 里的需求“执行”按钮后，由 Chatos 程序化创建执行任务，而不是把一段上下文注入给 AI 再让 AI 规划。

核心约束：

- 只有点击需求卡片“执行”才触发该链路；普通用户聊天消息不注入需求执行上下文。
- Chatos 只新增一条用户消息，不调用聊天发送接口，不触发 AI 回复。
- 每个项目任务只对应一个 Task Runner 执行任务，一对一关联，不做批次/多执行任务聚合。
- 项目任务创建时必须保存 Task Runner 执行模型和统一工具集；执行时直接读取这些字段。
- 需求级和项目任务级前置关系都必须被执行链路尊重。
- 项目管理服务获取 Task Runner 模型和工具集时必须使用真实用户 token。
- Task Runner 回调 Chatos 后，由 Chatos 根据消息 metadata 找到项目任务，并回写项目管理任务状态。

## 最终执行链路

1. 用户在 Chatos 项目 Plan 里点击某个需求的“执行”按钮。
2. 前端先关闭当前会话规划模式，然后调用：

   ```http
   POST /api/projects/:project_id/requirements/:requirement_id/execute
   ```

3. Chatos 校验项目权限，读取项目管理里的需求、子需求、项目任务、依赖图和需求技术文档。
4. Chatos 选择项目联系人；如果请求未指定联系人，则使用该项目最近的联系人。联系人必须配置 Task Runner runtime。
5. Chatos 通过真实用户 token 兑换联系人对应的 Task Runner agent token。
6. Chatos 复用或创建联系人会话，并只落库一条 `project_requirement_execution` 用户消息。
7. Chatos 校验需求级前置；如果目标需求依赖的并列前置需求未完成，则拒绝执行。
8. Chatos 把执行范围内的需求级前置折算成项目任务级前置，并按完整拓扑顺序创建 Task Runner 任务。
9. 每个 Task Runner 任务创建后，Chatos 调项目管理 link 接口写入一对一关联，并同步项目任务为进行中。
10. Chatos 把项目任务 ID、需求 ID、Task Runner task ID、run ID 写回消息 metadata。
11. Task Runner 回调 Chatos 时，Chatos 根据回调 `task_id` 在消息 metadata 中找到项目任务 ID，再调用项目管理同步接口更新项目任务状态。

## 项目管理服务改动

### 项目任务执行配置

`ProjectWorkItemRecord` 和 `CreateProjectWorkItemRequest` 新增必填字段：

```rust
task_runner_default_model_config_id: String
task_runner_enabled_tool_ids: Vec<String>
```

含义：

- `task_runner_default_model_config_id`：Task Runner 模型配置单选。
- `task_runner_enabled_tool_ids`：统一工具集多选，不区分内部/外部工具。

这些字段在项目任务创建时必须存在；不考虑历史任务补齐流程。

### MCP 工具变更

项目管理 MCP 的 `create_project_task` 增加两个必填参数：

```json
{
  "task_runner_default_model_config_id": "model_config_id",
  "task_runner_enabled_tool_ids": ["filesystem", "terminal"]
}
```

MCP 执行 `create_project_task` 时会用真实用户 token 调 Task Runner：

- `GET /api/model-configs`
- `GET /api/mcp/tools`
- `GET /api/external-mcp-configs`

然后校验模型 ID 和工具 ID 是否对当前真实用户可用。这里不能使用项目管理服务 token，也不能使用联系人 agent token。

### 管理页面入口

项目管理前端“新建项目任务”也增加：

- 执行模型单选。
- 工具集多选。

页面通过项目管理服务新增接口读取可选项：

```http
GET /api/task-runner/execution-options
```

该接口同样使用当前登录用户 token 去 Task Runner 拉模型和工具。

### 执行关联表

`project_work_item_task_runner_links` 收敛为一对一：

- `work_item_id` 唯一。
- `task_runner_task_id` 保留索引，用于排查和回调定位。
- SQLite/Mongo 启动建索引前会清理同一 `work_item_id` 的重复 link，只保留一条。

link 记录增加执行来源和回调状态字段：

```rust
source_session_id: Option<String>
source_user_message_id: Option<String>
task_runner_status: Option<String>
last_callback_event: Option<String>
last_callback_at: Option<String>
last_error_message: Option<String>
```

### 状态同步接口

新增 Chatos 内部同步接口：

```http
POST /api/chatos-sync/work-items/:work_item_id/task-runner-status
```

该接口使用项目管理 sync secret 鉴权。状态映射：

- `queued` / `running` / `processing` / `in_progress` -> `in_progress`
- `succeeded` / `success` / `completed` / `done` -> `done`
- `failed` / `error` / `blocked` -> `blocked`
- `cancelled` / `canceled` -> `cancelled`

## Task Runner 改动

`POST /api/tasks` 支持 Chatos 来源 header：

```http
X-Chatos-Session-Id: ...
X-Chatos-User-Message-Id: ...
X-Chatos-Turn-Id: ...
```

Task Runner 创建任务时把这些值写入 source context，后续回调 Chatos 时可以带回来源信息。

Task Runner 内置项目管理 provider 调项目管理 MCP 时，需要透传当前真实用户 token：

```http
X-Chatos-User-Authorization: Bearer <真实用户 token>
```

这样项目管理 MCP 才能按真实用户权限获取模型和工具集。

## Chatos 改动

### 后端编排接口

新增：

```http
POST /api/projects/:project_id/requirements/:requirement_id/execute
```

请求体可选：

```json
{
  "contact_id": "可选联系人 ID"
}
```

接口职责：

- 校验项目归属。
- 收集目标需求及所有子需求。
- 找到这些需求下的项目任务，过滤归档任务。
- 校验每个项目任务都有执行模型和工具集。
- 校验目标需求及子需求的外部前置需求已完成。
- 校验每个相关需求都有实现技术总体文档。
- 选择项目联系人并兑换 Task Runner agent token。
- 创建只落库、不发 AI 的执行消息。
- 按需求前置和项目任务前置计算完整拓扑顺序，先完成前置校验，再创建并启动 Task Runner 执行任务。
- 创建执行任务时，把对应需求的技术设计文档写入 Task Runner 任务说明，并在 `input_payload.technical_overview` 保留结构化原文。
- 写入项目管理一对一 link。
- 更新消息 metadata。

### 消息 metadata

执行消息 metadata 维护项目任务和执行任务的一对一映射：

```json
{
  "message_mode": "project_requirement_execution",
  "project_requirement_execution": {
    "project_id": "project_id",
    "requirement_id": "requirement_id",
    "status": "tasks_created",
    "task_links": [
      {
        "project_task_id": "project_task_id",
        "requirement_id": "requirement_id",
        "task_runner_task_id": "task_runner_task_id",
        "task_runner_run_id": "task_runner_run_id",
        "task_runner_status": "queued"
      }
    ]
  }
}
```

普通聊天消息不会生成这段 metadata。

### 回调同步

Task Runner 回调 Chatos 的既有入口保留：

```http
POST /api/agent/chat/task-runner/callback
```

新增逻辑：

1. 从回调读取 `task_id`、`run_id`、`status`。
2. 从原始用户消息 metadata 的 `project_requirement_execution.task_links` 中匹配 `task_runner_task_id`。
3. 找到对应 `project_task_id` 后，调用项目管理同步接口回写项目任务状态。
4. 如果 metadata 中没有匹配，按普通 Task Runner 回调处理，不影响聊天消息原有流程。

## 前端改动

Chatos 项目 Plan：

- 需求详情头部增加“执行”按钮。
- 点击后调用 Chatos 后端执行接口。
- 点击前关闭当前规划模式。
- 成功后展示已创建的执行任务数量，并刷新 Plan。

项目管理前端：

- 项目任务类型增加执行模型和工具集字段。
- 新建项目任务弹窗增加模型单选和工具集多选。
- 项目任务详情展示保存的执行模型和工具集。

## 失败处理

执行接口在以下情况直接失败，不创建执行任务：

- 需求不存在或无项目权限。
- 目标需求依赖的外部前置需求未完成。
- 需求及子需求下没有可执行项目任务。
- 项目任务存在循环前置关系。
- 范围外前置项目任务未完成，且没有可等待的 Task Runner 执行任务 link。
- 项目任务缺少执行模型或工具集。
- 需求技术文档为空。
- 项目联系人不存在或未配置 Task Runner runtime。
- 真实用户无法兑换 Task Runner agent token。
- 模型或工具集对 Task Runner agent 不可用。

如果 Task Runner 任务创建/启动过程中失败，接口返回错误；已经创建出的 link 和消息 metadata 会保留可追踪信息，便于后续排查。

## 验收标准

- 点击需求“执行”后，Chatos 新增一条消息，但不会触发 AI 回复。
- 普通用户聊天消息不会注入项目需求执行上下文。
- Task Runner 中创建出的任务使用联系人 agent 身份。
- Task Runner 任务带有项目 ID、需求 ID、项目任务 ID、来源 session ID、来源 message ID、模型配置和工具集。
- 项目管理里每个项目任务最多关联一个 Task Runner 执行任务。
- Task Runner 回调 Chatos 后，Chatos 消息状态和项目管理任务状态都能同步更新。
- 项目管理 MCP 创建项目任务时必须传模型和工具集，并且用真实用户 token 校验可用性。
