# TaskRunner 统一 Project 源方案

## 实现状态

已按长期方向落地首版实现：

- TaskRunner 已新增 `task_projects`、Project API、`tasks.project_id`、project 过滤和 MCP header 透传。
- ChatOS `ProjectService` 已改为 TaskRunner Project API adapter，不再编译本地 `repositories/projects.rs`，新库初始化也不再创建本地 `projects` 表/collection。
- ChatOS 创建/更新项目支持可选 `git_url`，前端 API 类型已透传。
- ChatOS TaskRunner MCP runtime 会写入 `X-Chatos-Project-Id`；无项目上下文默认 `-1`，旧 `"0"` public scope 归一为 `-1`。
- TaskRunner 受保护 Project API 会按当前真实用户合成 owner-scoped Public 项目；同一个 `project_id = "-1"` 在不同 owner 下代表各自独立的 public 空间。
- TaskRunner 内置 task manager 创建的子任务继承父任务 `project_id`。
- TaskRunner 前置任务约束已收紧：创建任务、更新/设置前置任务、批量创建带前置任务和内置 task manager 子任务都只能引用同一项目内的前置任务。
- TaskRunner MCP 工具已按请求 header 中的项目上下文隐式过滤；`list_tasks`、`get_task`、task stats、run、prompt、task memory 等按 task/run/prompt 访问的工具都会校验当前项目范围。
- MCP tool schema 和对外 task JSON 不暴露 `project_id`，项目过滤只由程序 header 透传；无项目上下文由 ChatOS 传 `-1` 落到 public。
- TaskRunner 前端已新增 Projects 菜单，任务列表展示“隶属项目”，并支持按项目过滤；在项目过滤视图中新建任务会落到当前项目。
- TaskRunner 前端新建/编辑任务的前置任务下拉只显示当前任务项目内的任务；无项目过滤的新建任务默认只显示 public `-1` 下的前置任务。
- TaskRunner 已补 `create_task` project 归属测试：覆盖项目透传、无项目/public、旧 `"0"`、缺失项目、归档项目和跨 owner 项目。
- TaskRunner 已补同项目前置任务测试和 MCP 项目过滤测试：覆盖同项目可依赖、跨项目创建/设置前置任务被拒绝、MCP `list_tasks` 按 header 项目过滤。
- TaskRunner 已补 Project `git_url` 测试：覆盖常见 Git URL、非法 URL、空字符串清空可选地址和旧 `"0"` 查 public。
- ChatOS memory mapping 新写入已统一使用 `-1`，保留的 `"0"` 只用于读取旧数据/默认名称兼容。

## 目标

直接把 project 源统一到 TaskRunner，不做长期双写、镜像或过渡缓存：

- TaskRunner 提供统一 Project API，作为 ChatOS 和任务系统唯一项目数据源。
- ChatOS 不再维护自己的本地 `projects` 源表；项目创建、列表、详情、更新、归档都走 TaskRunner。
- ChatOS 只在自身业务表里保存 `project_id` 引用，例如 sessions、terminals、project runner settings、contact links。
- ChatOS 在项目内通过 TaskRunner 工具创建任务时，任务自动写入当前 `project_id`，该字段由程序透传，不暴露给 AI 手填。
- ChatOS 不在项目内发起时，统一落到 public project，`project_id = "-1"`。
- TaskRunner 执行任务时，内置 `task_manager_add_task` 创建的子任务继承父任务 `project_id`。

## 核心决策

- `project_id` 使用字符串。现有 ChatOS UUID 项目 id 在一次性迁移时原样导入 TaskRunner，后续新项目 id 由 TaskRunner 生成。
- public 空间固定为 `"-1"`，但它不是跨用户共享任务空间；任务查询/写入始终叠加 owner scope。TaskRunner 只保留一条 public 模板记录，受保护 Project API 会按当前真实用户返回带 owner 信息的虚拟 Public 项目。
- ChatOS 原先 `"0"` 虚拟项目约定废弃，项目边界统一归一为 `"-1"`。
- TaskRunner project 删除采用 archive，不硬删，避免历史 sessions、tasks、terminals 失去引用。
- ChatOS 项目操作失败时直接返回失败，因为 TaskRunner 已是唯一项目源，不再允许 ChatOS 本地先成功、TaskRunner 后补偿。

## 当前代码状态

### ChatOS 当前状态

- 项目 CRUD 在 `chat_app_server_rs/src/api/projects/crud_handlers.rs`。
- `chat_app_server_rs/src/models/project.rs` 已改成 TaskRunner Project API adapter，包含 `PUBLIC_PROJECT_ID = "-1"` 和统一 `normalize_project_id`。
- `chat_app_server_rs/src/repositories/projects.rs` 已从编译路径删除，`repositories/mod.rs` 不再导出本地项目 repository。
- 新库 SQLite/Mongo 初始化不再创建 ChatOS 本地 `projects` 表/collection。
- `ensure_owned_project`、`resolve_project_runtime`、fs policy roots、terminal root、workspace realtime watcher 已改为通过 `ProjectService` 读取 TaskRunner 项目。
- sessions、terminals、memory mapping、project runner settings 等表里保存 `project_id`，这些是引用关系，可以保留。
- `resolve_runtime_context` 已经能得到 `resolved_project_id/resolved_project_root`，并在创建 TaskRunner MCP server 时注入 session、turn、user message、workspace headers。
- memory mapping 新写入 public 项目时统一写 `-1`；旧 `"0"` 只保留读取兼容。

### TaskRunner 当前状态

- TaskRunner 已新增 `TaskProjectRecord`、`TaskProjectService`、`task_projects` store 和 `0020_task_projects.sql`。
- `TaskRecord`、`TaskSummaryRecord`、`TaskSourceContext`、`TaskListFilters` 已新增 `project_id`。
- 受保护 Project API 使用 `TaskProjectService::list_projects_for_user/get_project_for_user` 返回当前真实用户自己的 Public 项目，避免 UI 出现 owner 为空的全局 Public。
- 根任务创建在 `TaskService::create_task` 中归一化并校验 `source_context.project_id`，非 public 项目必须存在、active 且当前 owner 可访问。
- `CreateTaskRequest.project_id` 仅用于 TaskRunner REST/UI 新建任务；MCP 创建任务仍只通过 `TaskSourceContext.project_id` 接收程序透传项目。
- `TaskService::validate_task_prerequisites_for_project` 统一校验前置任务必须和目标任务属于同一 `project_id`。
- TaskRunner MCP 请求上下文在 `task_runner_service/backend/src/mcp_server/context.rs`，header 解析在 `task_runner_service/backend/src/api/mcp.rs`，已读取 `X-Chatos-Project-Id` / `X-Task-Runner-Project-Id`。
- TaskRunner MCP 访问控制在 `task_runner_service/backend/src/mcp_server/access.rs`，会用 `McpRequestContext::project_scope_id()` 对 task/run/prompt 访问做项目范围校验。
- TaskRunner 执行期内置 task manager 子任务入口在 `task_runner_service/backend/src/services/task_manager_bridge/task_ops.rs`，已继承父任务 `project_id`。
- TaskRunner 已提供受保护 Project API 和 sync-secret Project API，复用现有 auth / user_service owner scope。

## TaskRunner 数据模型

### Project 模型

新增 `TaskProjectRecord`：

```rust
pub struct TaskProjectRecord {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub status: TaskProjectStatus, // active / archived
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}
```

约束：

- `id = "-1"` 是 public project id，不允许普通用户删除或覆盖为普通项目；在受保护 API 中它会按当前真实用户 owner scope 虚拟化返回。
- 不同真实用户的 public 任务都保存为 `project_id = "-1"`，但通过任务 owner 字段隔离，因此不会互相看到或引用。
- 非 public project 必须有 owner scope。
- `root_path` 可以为空，但 ChatOS 文件、终端、project runner 相关能力需要 root_path；ChatOS 创建项目时仍要校验目录存在。
- `git_url` 可为空；非空时只做格式归一化和长度校验，允许 `https://...`、`http://...`、`ssh://...`、`git@host:org/repo.git` 这类常见 Git 地址。

SQLite 迁移：

```sql
CREATE TABLE IF NOT EXISTS task_projects (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  name TEXT NOT NULL,
  root_path TEXT,
  git_url TEXT,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_projects_owner_user_id
ON task_projects(owner_user_id);

CREATE INDEX IF NOT EXISTS idx_task_projects_status
ON task_projects(status);

ALTER TABLE tasks ADD COLUMN project_id TEXT NOT NULL DEFAULT '-1';

CREATE INDEX IF NOT EXISTS idx_tasks_project_id
ON tasks(project_id);

CREATE INDEX IF NOT EXISTS idx_tasks_owner_project
ON tasks(owner_user_id, project_id);
```

Mongo：

- 新增 `task_projects` collection。
- 给 `task_projects.id`、`task_projects.owner_user_id`、`tasks.project_id` 建索引。
- 旧任务缺 `project_id` 时模型层默认 `"-1"`。

InMemory：

- `StoreData` 新增 `task_projects: BTreeMap<String, TaskProjectRecord>`。
- 初始化 public project。

### Task 模型

在 `TaskRecord`、`TaskSummaryRecord` 中新增：

```rust
pub project_id: String,
```

在 `TaskSourceContext`、`TaskListFilters` 中新增：

```rust
pub project_id: Option<String>,
```

归一化规则：

- 空值、缺失值、空白字符串统一为 `"-1"`。
- TaskRunner 内部保存的 `project_id` 永远非空。

## TaskRunner API

### 统一 Project API

新增受保护 API，使用现有 TaskRunner auth / user_service owner scope：

- `GET /api/projects`
- `POST /api/projects`
- `GET /api/projects/:id`
- `PATCH /api/projects/:id`
- `DELETE /api/projects/:id`
- `GET /api/projects/:id/tasks`

`POST /api/projects` 请求：

```json
{
  "name": "项目名",
  "root_path": "/path/to/project",
  "git_url": "git@github.com:org/repo.git",
  "description": "可选描述"
}
```

`PATCH /api/projects/:id` 请求：

```json
{
  "name": "新项目名",
  "root_path": "/new/path",
  "git_url": "https://github.com/org/repo.git",
  "description": "新描述"
}
```

行为：

- create 由 TaskRunner 生成 id。
- update 校验当前用户 owner。
- delete 做 archive，返回 archived project。
- public `-1` 可查、可用于任务过滤，但不可删除。
- `GET /api/projects` 默认只返回当前 owner 可见项目，包含 public。

### 任务 API project 过滤

扩展：

- `GET /api/tasks?project_id=...`
- `GET /api/tasks/page?project_id=...`
- `GET /api/tasks/summaries?project_id=...`
- `GET /api/projects/:id/tasks`

过滤规则：

- 普通用户只能查自己 owner scope 下的任务。
- `project_id = "-1"` 查询 public project 下、且属于当前 owner 的任务。
- admin 可以跨 owner 查询，但默认也可加 project filter。

### MCP project 上下文

MCP tool schema 不暴露 `project_id` 给 AI。

TaskRunner 从 header 读取：

```rust
project_id: header_text(headers, "x-chatos-project-id")
    .or_else(|| header_text(headers, "x-task-runner-project-id"))
```

`McpRequestContext::task_source_context` 的生成条件要包含 `project_id`，否则只有 project header 时会丢上下文。

## ChatOS 改造

### 删除本地项目源

移除或停用这些本地源能力：

- `chat_app_server_rs/src/models/project.rs` 不再定义本地持久化模型，可改为 TaskRunner API DTO。
- `chat_app_server_rs/src/repositories/projects.rs` 删除或改成 TaskRunner client adapter。
- SQLite/Mongo 的 `projects` 表/collection 不再作为运行时读写源。
- `ProjectService::{create,get_by_id,list,update,delete}` 改为调用 TaskRunner Project API。

保留这些 `project_id` 引用：

- `sessions.project_id`
- `terminals.project_id`
- project runner catalog/environment settings keyed by `project_id`
- `chatos_project_agent_links`
- message/task graph 展示里的 project scope

这些表不存项目元数据，只存 TaskRunner project id。

### ChatOS 项目 API 变成代理

`chat_app_server_rs/src/api/projects/crud_handlers.rs` 继续保留对前端的 API 形状，但实现改为代理 TaskRunner：

- `list_projects` -> `GET task_runner /api/projects`
- `create_project` -> 校验 root_path 后 `POST task_runner /api/projects`
- `get_project` -> `GET task_runner /api/projects/:id`
- `update_project` -> 校验 root_path 后 `PATCH task_runner /api/projects/:id`
- `delete_project` -> 先关闭相关 terminals，再 `DELETE task_runner /api/projects/:id`

ChatOS 返回给前端的 project DTO 可以保持现有字段：

```json
{
  "id": "...",
  "name": "...",
  "root_path": "...",
  "git_url": "...",
  "description": "...",
  "user_id": "...",
  "latest_session_id": "...",
  "last_message_at": "...",
  "created_at": "...",
  "updated_at": "..."
}
```

其中 `latest_session_id/last_message_at` 仍由 ChatOS 根据本地 sessions/contact links 附加。

### 权限校验改造

把 `ensure_owned_project` 改为调用 TaskRunner `GET /api/projects/:id`：

- TaskRunner 返回 404/403 时，ChatOS 映射为现有错误。
- 成功返回 project 后，ChatOS 使用 `root_path` 继续做 fs policy、project runner、workspace watcher。

`resolve_project_runtime` 改为：

- `project_id` 为空、`"0"`、空白时归一为 `"-1"` 或 `None`，看调用场景。
- 需要项目 root 的场景，如果 project_id 是 `"-1"` 则不返回 root。
- 非 public project 调 TaskRunner 获取项目，并校验 owner。

### Runtime 透传

在 `chat_app_server_rs/src/modules/conversation_runtime/runtime_context.rs`：

- `resolved_project_id` 来自 TaskRunner Project API 校验后的 id。
- 调用 `build_contact_task_runner_runtime` 时新增 `project_id` 参数。
- TaskRunner MCP headers 加：

```text
X-Chatos-Project-Id: <resolved_project_id 或 -1>
```

归一化函数：

```rust
fn task_runner_project_scope(project_id: Option<&str>) -> String {
    project_id
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "0")
        .unwrap_or("-1")
        .to_string()
}
```

### Memory 和联系人项目关系

TaskRunner 是唯一项目源。ChatOS memory mapping 只保留联系人/记忆关系所需的 `project_id` 引用和记忆侧展示缓存，不再作为项目 CRUD 或 root_path 的权威来源。

改造方式：

- contact/project link 仍保留，因为这是 ChatOS 关系数据。
- list contact projects 时，先拿 link 里的 project_id 列表，再批量调用 TaskRunner Project API 补项目名、root_path、git_url、description、status。
- project memory 的 subject key 改用统一 project_id；public 使用 `-1`。
- 旧 `"0"` subject key 需要一次性迁移；代码只保留必要读取兼容：
  - 新写入统一写 `-1`。
  - 旧数据读取时将 `"0"` 归一为 `-1`，避免继续扩散旧 public 约定。

### Workspace realtime watcher

`workspace_realtime_watcher` 当前扫描 ChatOS 本地项目列表。改为：

- 调 TaskRunner `GET /api/projects?status=active` 获取项目列表。
- 过滤 `root_path` 为空或 public `-1`。
- watcher 内部 map 仍以 project_id 为 key。

### Project runner / terminals / fs policy

- project runner 的 catalog/environment settings 继续在 ChatOS 本地按 `project_id` 存。
- terminal 创建时继续保存 `project_id` 引用。
- fs policy roots 从 TaskRunner project API 获取 root_path。
- 删除项目时，ChatOS 先关闭本地相关 terminals，再调用 TaskRunner archive。

## 任务写入 project_id

### 根任务

在 TaskRunner `TaskService::create_task`：

- 从 `source_context.project_id` 读取。
- 空值默认 `"-1"`。
- 非 `"-1"` 时校验 project 存在、状态 active、owner 与创建者一致。
- 写入 `TaskRecord.project_id`。

REST 创建任务：

- TaskRunner UI 手动创建任务时可以选择 project。
- 请求体可支持 `project_id`，但需要后端 owner 校验。

MCP 创建任务：

- 不允许 AI 传 `project_id`。
- 只接受 header/source context 里的系统透传 project。

### 子任务

在 `create_followup_task_for_tool`：

```rust
project_id: parent.project_id.clone()
```

不允许 draft 覆盖。

## 一次性切源迁移

直接切源仍需要一次性迁移，不做长期过渡。

### 迁移前置

1. TaskRunner 先上线 project 模型、Project API、`tasks.project_id`。
2. 写一次性迁移命令，例如：
   - `chat_app_server_rs/src/bin/export_projects_to_task_runner.rs`
   - 或 TaskRunner 管理命令读取 ChatOS DB。
3. 迁移命令使用 `x-chatos-callback-secret` 或管理员 token，只在切源窗口运行。

### 迁移内容

1. 导入 ChatOS `projects` 到 TaskRunner `task_projects`，保留原 id；旧项目没有 `git_url` 时可留空，或者从 `root_path/.git/config` 的 `origin` remote 尝试推断。
2. 创建 public project `-1`。
3. 将 ChatOS sessions/terminals/project runner settings 中的空项目、`"0"` 项目统一规范为 `"-1"`。
4. 回填 TaskRunner 旧任务：
   - 如果 task 有 `source_session_id`，从 ChatOS sessions 查 project_id。
   - 如果 task 是子任务，优先继承父任务 project_id。
   - 无法匹配的任务落 `"-1"`。
5. 迁移 contact/project link、memory subject key 中的 `"0"` 到 `"-1"`，或至少提供兼容查询。

### 切源后删除

切源版本中：

- ChatOS 项目 CRUD 不再写本地 `projects`。
- ChatOS runtime 不再从本地 `projects` 读取 root。
- ChatOS 新库初始化不再创建本地 `projects` 表/collection，运行时代码路径也不再引用本地 projects repository。
- 旧环境里已经存在的本地 `projects` 表/collection 只作为一次性导入 TaskRunner 的迁移输入；切源验证后应由清理迁移 drop 或归档，不作为运行时备份源。

## 前端影响

ChatOS 前端：

- API 路径可以保持 `/api/projects` 不变，后端代理 TaskRunner。
- 项目创建、更新、删除交互不需要感知数据源变化。
- public 空间如果在 UI 显示，统一显示为 `Public`，id 为 `-1`。

TaskRunner 前端：

- 增加 Projects 页面或项目筛选器。
- 任务列表显示 project 名称。
- 手动创建任务时支持选择 project，默认 public。

## 测试计划

本轮已执行：

- `cargo fmt`
- `cargo check -p task_runner_service_backend`
- `cargo check -p chat_app_server_rs`
- `cargo test -p task_runner_service_backend`
- `cargo test -p chat_app_server_rs services::task_runner_api_client::tests::exchange_task_runner_token_via_user_service_sends_bearer_and_body -- --nocapture`
- `cargo test -p chat_app_server_rs modules::conversation_runtime::session_scope::tests -- --nocapture`
- `npm run type-check`
- `npx vitest run src/lib/domain/contactSessions.test.ts src/lib/store/actions/sessions.mutations.test.ts`
- `npx vitest run src/lib/domain/contactSessions.test.ts src/lib/store/actions/projects.test.ts src/lib/store/actions/sessions.mutations.test.ts`

TaskRunner 单元测试：

- public project 初始化幂等。
- project CRUD owner scope 校验。
- project `git_url` 可为空；非空时接受常见 HTTPS/SSH Git 地址并拒绝明显非法输入。
- archive project 后历史任务仍可查询。
- `normalize_project_id(None/"") == "-1"`。
- `create_task` 无 project context 时保存 `-1`。
- MCP header 带 `X-Chatos-Project-Id` 时任务保存该 id。
- 子任务继承父任务 `project_id`。
- SQLite/Mongo/InMemory 的 `TaskListFilters.project_id` 都生效。

ChatOS 单元测试：

- `ProjectService` adapter 调 TaskRunner API。
- ChatOS project DTO 保留并透传 `git_url`。
- `ensure_owned_project` 从 TaskRunner 获取 project 并映射错误。
- `resolve_project_runtime` 对 `"0"`、空值归一为 public。
- `build_contact_task_runner_runtime` 写入 `X-Chatos-Project-Id`。

集成测试：

- ChatOS 创建项目后，TaskRunner `task_projects` 有记录，ChatOS 本地 `projects` 不写入。
- ChatOS 项目内发起 TaskRunner async planner 创建任务，TaskRunner `tasks.project_id = project.id`。
- ChatOS 非项目会话发起任务，TaskRunner `tasks.project_id = "-1"`。
- TaskRunner 执行父任务时用 task manager 创建子任务，子任务 project_id 与父任务一致。
- ChatOS 项目列表、文件浏览、project runner、terminal 创建都通过 TaskRunner project root 正常工作。
- 归档项目后，ChatOS 不再允许新会话/新任务使用该项目，但历史任务和历史会话仍可展示。

## 直接落地顺序

1. TaskRunner 新增 project 模型、store、API、public 初始化。
2. TaskRunner 给 tasks 加 `project_id`，补过滤、MCP header、任务创建、子任务继承。
3. ChatOS 新增 TaskRunner Project API client，并把 `ProjectService` 改成 remote adapter。
4. ChatOS 改造 `ensure_owned_project`、`resolve_project_runtime`、fs policy roots、workspace watcher、project runner 读取路径。
5. ChatOS 项目 CRUD 改为直接调用 TaskRunner，不再写本地 `projects`。
6. ChatOS runtime 给 TaskRunner MCP 注入 `X-Chatos-Project-Id`。
7. 写并执行一次性迁移：导入旧项目、回填任务 project_id、规范 public `-1`。
8. 删除或停用 ChatOS 本地 `projects` repository/schema 运行路径。
9. 补前端项目筛选和 TaskRunner Projects 页面。
