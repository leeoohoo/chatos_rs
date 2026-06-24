# Chatos Plan 模式接入 TaskRunner 规划任务确定方案

## 固定结论

- Chatos Plan 模式只装载 `task_runner_service` MCP。
- Chatos Plan 模式不再直接装载 `project_management_service` MCP。
- Project Management MCP 由 TaskRunner 规划任务在后台运行期装载。
- Project Management MCP 在规划任务中全量开放读写工具。
- 内部 MCP 在规划任务中按固定清单装载，并按固定工具清单限制写入能力。
- `CodeMaintainerWrite` 不加入规划任务的 Active Builtin Kinds。
- `AgentBuilder` 不加入规划任务的 Active Builtin Kinds。
- Plan 模式的 TaskRunner 任务工具只查询、读取、更新、取消规划任务，不能看到普通任务。
- 前端已经通过 `hasConcreteProjectContext(effectiveProjectId)` 保证 Plan 模式带 concrete `project_id`。后端保留非法请求 guard。

## Chatos 请求协议

Chatos 在 Plan 模式下仍调用 `build_contact_task_runner_runtime`，并向 TaskRunner MCP 注入以下 headers：

```http
X-Task-Runner-Tool-Profile: chatos_async_planner
X-Task-Runner-Task-Profile: chatos_plan
X-Chatos-Plan-Mode: true
X-Chatos-Project-Id: <current concrete project id>
X-Chatos-Session-Id: <session id>
X-Chatos-Turn-Id: <turn id>
X-Chatos-User-Message-Id: <user message id>
X-Chatos-User-Authorization: Bearer <real user token>
```

Plan 模式获取 TaskRunner skill 使用固定接口：

```http
GET /api/skills/task-runner?lang=zh-CN&profile=chatos_plan
GET /api/skills/task-runner?lang=en-US&profile=chatos_plan
```

普通模式继续使用：

```http
GET /api/skills/task-runner?lang=zh-CN
GET /api/skills/task-runner?lang=en-US
```

## TaskRunner 请求上下文

`McpRequestContext` 增加字段：

```rust
pub task_profile: Option<String>,
pub chatos_plan_mode: bool,
```

`mcp_request_context_from_headers` 读取：

```rust
task_profile: header_text(headers, "x-task-runner-task-profile"),
chatos_plan_mode: header_bool(headers, "x-chatos-plan-mode"),
```

判定函数固定为：

```rust
pub(super) fn is_chatos_plan_task_profile(&self) -> bool {
    self.task_profile
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("chatos_plan"))
        || self.chatos_plan_mode
}
```

Plan 模式请求缺少 concrete `project_id` 时，TaskRunner MCP 返回错误：

```text
Chatos Plan mode requires concrete project_id
```

## 任务数据模型

`TaskRecord` 增加字段：

```rust
pub task_profile: String,
```

固定取值：

- `default`
- `chatos_plan`

SQLite 迁移：

```sql
ALTER TABLE tasks ADD COLUMN task_profile TEXT NOT NULL DEFAULT 'default';
CREATE INDEX IF NOT EXISTS idx_tasks_task_profile ON tasks(task_profile);
CREATE INDEX IF NOT EXISTS idx_tasks_chatos_source_profile
ON tasks(source_session_id, source_user_message_id, task_profile);
```

Mongo 索引：

```text
tasks.task_profile
tasks.source_session_id + tasks.source_user_message_id + tasks.task_profile
```

InMemory store 增加同名字段，默认值为 `default`。

## Plan 模式任务工具隔离

当 `McpRequestContext::is_chatos_plan_task_profile()` 为 true，TaskRunner MCP 任务工具统一叠加：

```rust
task_profile = "chatos_plan"
```

工具行为固定如下：

- `list_tasks` 只返回规划任务。
- `get_task` 只读取规划任务。普通任务 ID 返回无权访问。
- `get_task_stats` 只统计规划任务。
- `create_task` 创建规划任务。
- `create_tasks_with_prerequisites` 创建规划任务。
- `update_task` 只更新规划任务。
- `set_task_prerequisites` 只允许规划任务依赖规划任务。
- `cancel_task` 只取消规划任务。
- `get_task_dependency_graph` 只读取规划任务图。
- `wait_for_task_completion` 只表示本轮规划任务编排结束。

实现位置：

- `task_runner_service/backend/src/mcp_server/task_tools.rs`
- `task_runner_service/backend/src/mcp_server/prerequisite_creation.rs`
- `task_runner_service/backend/src/mcp_server/access.rs`

必须在 `require_task_for_user_in_context` 和 `require_tasks_for_user_in_context` 中校验 `task_profile`。模型手工传入普通任务 ID 也会被拒绝。

## 规划任务创建规则

Plan 模式调用 `create_task` 时，TaskRunner 强制写入：

```rust
task_profile = "chatos_plan"
status = TaskStatus::Ready
schedule.mode = TaskScheduleMode::ContactAsync
mcp_config.enabled = true
```

Plan 模式调用 `create_tasks_with_prerequisites` 时，每个新任务都写入同样规则。

重复请求复用范围固定为：

```text
source_session_id + source_user_message_id + task_profile
```

普通任务不参与 Plan 模式复用。

## Project Management MCP 运行期装载

TaskRunner 运行 `task_profile = "chatos_plan"` 的任务时，必须注入 Project Management MCP HTTP server。

server 固定配置：

```rust
McpHttpServer {
    name: "project_management_service",
    url: format!("{}/mcp", project_service_base_url),
    headers: {
        "X-Project-Service-Sync-Secret": project_service_sync_secret,
        "X-Task-Runner-Owner-User-Id": task.owner_user_id,
        "X-Task-Runner-Owner-Username": task.owner_username,
        "X-Task-Runner-Owner-Display-Name": task.owner_display_name,
        "X-Chatos-Project-Id": task.project_id,
        "X-Task-Runner-Task-Profile": "chatos_plan"
    }
}
```

Project Management 服务必须在 `/mcp` 增加 TaskRunner 内部认证分支：

- 校验 `X-Project-Service-Sync-Secret`。
- 读取 `X-Task-Runner-Owner-User-Id`。
- 读取 `X-Chatos-Project-Id`。
- 按 owner 和 project scope 执行原有项目权限校验。
- 公共 agent token 认证路径保持原状。

规划任务中的 Project Management MCP 全量开放以下工具：

- `get_project_overview`
- `initialize_project`
- `list_requirements`
- `create_requirement`
- `update_requirement`
- `set_requirement_dependencies`
- `upsert_requirement_technical_overview`
- `get_requirement_technical_overview`
- `list_project_tasks`
- `create_project_task`
- `update_project_task`
- `set_project_task_dependencies`
- `get_project_dependency_graph`

## 内部 MCP Active Builtin Kinds

规划任务 Active Builtin Kinds 固定为：

- `CodeMaintainerRead`
- `TerminalController`
- `TaskManager`
- `Notepad`
- `AskUser`
- `RemoteConnectionController`
- `WebTools`
- `BrowserTools`
- `MemorySkillReader`
- `MemoryCommandReader`
- `MemoryPluginReader`

以下 builtin 永不加入规划任务：

- `CodeMaintainerWrite`
- `AgentBuilder`

## 内部 MCP 工具策略

TaskRunner 为规划任务构建 builtin registry 时只执行 builtin 级过滤。

固定规则：

- `CodeMaintainerWrite` 不加入 Active Builtin Kinds。
- `AgentBuilder` 不加入 Active Builtin Kinds。
- Active Builtin Kinds 中存在的其它 MCP 全部开放工具。
- 不对 `TerminalController` 做工具级过滤。
- 不对 `TaskManager` 做工具级过滤。
- 不对 `Notepad` 做工具级过滤。
- 不对 `RemoteConnectionController` 做工具级过滤。
- 不对 `WebTools` 做工具级过滤。
- 不对 `BrowserTools` 做工具级过滤。
- 不对 `MemorySkillReader`、`MemoryCommandReader`、`MemoryPluginReader` 做工具级过滤。
- 不对 `AskUser` 做工具级过滤。

手工调用 `CodeMaintainerWrite` 所属写工具必须返回错误：

```text
Tool is disabled in Chatos Plan task profile
```

`CodeMaintainerRead` 开放工具：

- `read_file_raw`
- `read_file_range`
- `list_dir`
- `search_text`
- `read_file`
- `search_files`

`CodeMaintainerWrite` 禁止工具：

- `write_file`
- `edit_file`
- `append_file`
- `delete_path`
- `apply_patch`
- `patch`

## TaskRunner Skill

新增文件：

- `task_runner_service/TASK_RUNNER_PLAN_TASK_SKILL.zh-CN.md`
- `task_runner_service/TASK_RUNNER_PLAN_TASK_SKILL.en-US.md`

`GET /api/skills/task-runner?profile=chatos_plan` 返回规划任务 skill。

普通 skill 接口保持当前内容。

规划任务 skill 必须表达以下规则：

- 当前对话模型通过 TaskRunner MCP 创建规划任务。
- 规划任务通过 Project Management MCP 写入项目规划。
- 内部 MCP 按 Active Builtin Kinds 全量开放。
- `CodeMaintainerWrite` 不存在。
- `AgentBuilder` 不存在。
- Plan 模式任务工具只看规划任务。
- 普通实现任务不在 Plan 模式里创建。
- 实现范围、验收标准、拆分结果写入 Project Management 项目任务。

## TaskRunner 设置页

设置页新增 tab：

```text
Plan Task Skill
```

修改文件：

- `task_runner_service/frontend/src/pages/SettingsPage.tsx`
- `task_runner_service/frontend/src/api/client.ts`
- `task_runner_service/frontend/src/i18n/messages/zhCN.ts`
- `task_runner_service/frontend/src/i18n/messages/enUS.ts`

tab 展示：

- endpoint：`/api/skills/task-runner?lang=<locale>&profile=chatos_plan`
- skill name
- locale
- content

`api.getTaskRunnerSkill` 签名固定为：

```ts
getTaskRunnerSkill(lang: string, profile?: string)
```

设置页普通 skill tab 传 `profile` 空值。

设置页规划 skill tab 传 `profile = "chatos_plan"`。

## 后端落地步骤

1. 修改 Chatos runtime。
   - `plan_mode = true` 走 `build_contact_task_runner_runtime`。
   - 删除 Plan 模式直接装载 Project Management MCP 的分支。
   - 注入 Plan 模式 headers。
   - fetch skill 时带 `profile=chatos_plan`。

2. 修改 TaskRunner skill API。
   - query 增加 `profile`。
   - profile 为 `chatos_plan` 时返回规划任务 skill。

3. 修改 TaskRunner MCP context。
   - 读取 task profile header。
   - 增加 `is_chatos_plan_task_profile`。

4. 修改 TaskRunner 任务模型。
   - 增加 `task_profile`。
   - 增加 SQLite 迁移。
   - 增加 Mongo 索引。
   - 更新 DTO。
   - 更新前端类型。

5. 修改 TaskRunner MCP 任务工具。
   - Plan profile 下创建规划任务。
   - Plan profile 下查询规划任务。
   - Plan profile 下读取规划任务。
   - Plan profile 下更新规划任务。
   - Plan profile 下取消规划任务。
   - Plan profile 下建立规划任务之间的前置关系。

6. 修改 TaskRunner 运行期 MCP builder。
   - Plan profile 下注入 Project Management MCP。
   - Plan profile 下注入固定内部 builtin 清单。
   - Plan profile 下只排除 `CodeMaintainerWrite` 和 `AgentBuilder`。

7. 修改 Project Management `/mcp`。
   - 增加 TaskRunner sync secret 内部认证分支。
   - 保持现有 agent token 认证分支。

8. 修改 TaskRunner 设置页。
   - 增加 Plan Task Skill tab。
   - 增加 profile 参数调用。

## 验收清单

### Chatos backend

- Plan 模式只注册 `task_runner_service` MCP。
- Plan 模式不注册 `project_management_service` MCP。
- Plan 模式请求包含 `X-Task-Runner-Task-Profile: chatos_plan`。
- Plan 模式 skill 请求包含 `profile=chatos_plan`。
- 非法 Plan 请求缺少 concrete project id 时，TaskRunner 返回固定错误。

### TaskRunner MCP

- `list_tasks` 在 Plan profile 下只返回规划任务。
- `get_task` 在 Plan profile 下拒绝普通任务 ID。
- `get_task_dependency_graph` 在 Plan profile 下拒绝普通任务 ID。
- `set_task_prerequisites` 在 Plan profile 下拒绝普通任务 ID。
- `cancel_task` 在 Plan profile 下拒绝普通任务 ID。
- `create_task` 在 Plan profile 下写入 `task_profile = "chatos_plan"`。
- `create_tasks_with_prerequisites` 在 Plan profile 下写入 `task_profile = "chatos_plan"`。
- 重复请求复用只命中 `task_profile = "chatos_plan"` 的任务。

### TaskRunner runtime

- 规划任务运行期加载 Project Management MCP。
- Project Management MCP 写工具调用成功。
- Active Builtin Kinds 不包含 `CodeMaintainerWrite`。
- Active Builtin Kinds 不包含 `AgentBuilder`.
- `CodeMaintainerWrite` 工具不出现在 `list_tools`。
- `AgentBuilder` 工具不出现在 `list_tools`。
- `execute_command` 出现在 `list_tools`。
- `run_command` 出现在 `list_tools`。
- `add_task` 出现在内部 TaskManager `list_tools`。
- `create_note` 出现在 Notepad `list_tools`。
- `browser_click` 出现在 BrowserTools `list_tools`。
- 手工调用 `CodeMaintainerWrite` 写工具返回 `Tool is disabled in Chatos Plan task profile`。

### Project Management

- TaskRunner sync secret 认证能访问 `/mcp`。
- sync secret 错误时 `/mcp` 拒绝请求。
- owner scope 错误时 `/mcp` 拒绝请求。
- project scope 错误时 `/mcp` 拒绝请求。
- agent token 认证路径保持原行为。

### TaskRunner frontend

- Settings 页面出现 Plan Task Skill tab。
- Plan Task Skill tab 使用 `profile=chatos_plan` endpoint。
- 普通 skill tab 不传 profile。
- 中英文切换返回对应 skill。

### 端到端

1. Chatos 在具体项目内打开 Plan 模式。
2. 当前对话只看到 TaskRunner MCP。
3. 当前对话获取规划任务 skill。
4. 当前对话创建规划任务。
5. TaskRunner 保存 `task_profile = "chatos_plan"`。
6. 规划任务运行期加载 Project Management MCP。
7. 规划任务加载除 `CodeMaintainerWrite` 和 `AgentBuilder` 之外的内部 MCP。
8. 规划任务写入 Project Management 需求、技术总体文档、项目任务、依赖。
9. `CodeMaintainerWrite` 和 `AgentBuilder` 没有暴露。
10. TaskRunner 回调 Chatos 展示规划结果。
