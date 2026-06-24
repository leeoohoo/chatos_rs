# 项目管理微服务拆分实施方案

## 1. 背景与关键澄清

当前 TaskRunner 已经有一部分 Project 能力，主要集中在：

- `task_runner_service/backend/src/models/project.rs`：`TaskProjectRecord`，包含 `name/root_path/git_url/description/status/owner/created_at/updated_at` 等项目基础信息。
- `task_runner_service/backend/migrations/0020_task_projects.sql`：`task_projects` 表，以及 `tasks.project_id`。
- `task_runner_service/backend/src/api/projects.rs`：项目 CRUD 和按项目查询 TaskRunner 任务。
- `task_runner_service/frontend/src/pages/ProjectsPage.tsx`：项目列表页，目前只有基础展示和跳转任务过滤。
- `task_runner_service/backend/migrations/0011_task_prerequisites.sql` 与 `task_runner_service/backend/src/services/task_dependencies.rs`：TaskRunner 执行任务的前置关系。

这次拆分需要明确一个边界：**项目管理里的任务不是 TaskRunner 的执行任务**。

后续文档中统一使用这些命名避免混淆：

- `Project`：项目基础信息。
- `ProjectProfile`：项目一对一扩展信息，保存项目背景、项目介绍等长文本。
- `Requirement`：需求。
- `ProjectWorkItem`：项目管理里的具体任务/工作项，属于某个需求。
- `TaskRunnerTask`：TaskRunner 现有的执行任务，只作为外部系统能力存在。

项目管理微服务可以在未来把某个 `ProjectWorkItem` 发给 TaskRunner 执行，但二者只能通过显式映射表关联，不能复用同一张任务表，也不能把 TaskRunner 的 `TaskRecord` 当作项目管理任务。

## 2. 目标

新增独立的项目管理微服务，作为项目管理领域的数据源和业务入口：

- 提供项目基础信息管理能力，承接现在 TaskRunner 中已有的项目基础字段。
- 新增项目背景、项目介绍字段，但不放入项目基础表，使用一对一扩展表。
- 提供需求模块：需求 CRUD、需求状态、需求前置关系、需求技术总体文档。
- 提供项目工作项模块：每个需求下可以拆分多个具体工作项，工作项之间支持前置关系。
- 提供独立前端，使用 React + Ant Design 管理台风格。
- 后端使用 Rust，建议复用当前仓库里的 Axum、Serde、SQLx/MongoDB、User Service 认证模式。
- TaskRunner 不再作为项目数据源；它只保留“执行任务”能力，按需通过 API 被项目管理服务调用。

## 3. 服务边界

### 项目管理微服务负责

- 项目基础信息：名称、根目录、Git 地址、短描述、状态、owner、创建/更新时间。
- 项目详情：项目背景、项目介绍。
- 需求：需求列表、详情、状态、优先级、验收标准、需求依赖、需求技术总体文档。
- 项目工作项：工作项列表、详情、状态、优先级、负责人、所属需求、工作项依赖。
- 依赖图校验：禁止自依赖、禁止循环依赖、限制依赖深度和数量。
- 与 User Service 对接认证和 owner scope。
- 可选与 TaskRunner 对接：创建/查询外部执行任务，并保存映射关系。

### 项目管理微服务不负责

- 不负责 TaskRunner 的 run、MCP、模型调用、执行日志、执行态恢复。
- 不复用 TaskRunner 的 `tasks` 表保存项目工作项。
- 不在项目工作项中内嵌 TaskRunner 执行状态作为主状态。
- 不把需求依赖和 TaskRunner 的 task prerequisite 混在一起。

## 4. 推荐目录结构

```text
project_management_service/
  backend/
    Cargo.toml
    migrations/
      0001_init.sql
      0002_requirements.sql
      0003_project_work_items.sql
      0004_task_runner_links.sql
    src/
      main.rs
      lib.rs
      config.rs
      state.rs
      auth.rs
      api/
        mod.rs
        router.rs
        projects.rs
        project_profiles.rs
        requirements.rs
        requirement_documents.rs
        work_items.rs
        dependency_graph.rs
      models/
        mod.rs
        project.rs
        project_profile.rs
        requirement.rs
        requirement_document.rs
        work_item.rs
        dependency.rs
        task_runner_link.rs
      services/
        mod.rs
        project_service.rs
        requirement_service.rs
        work_item_service.rs
        dependency_service.rs
        task_runner_adapter.rs
      store/
        mod.rs
        mongo.rs
        sqlite.rs
        in_memory.rs
  frontend/
    package.json
    src/
      api/client.ts
      types.ts
      App.tsx
      components/AppShell.tsx
      pages/
        ProjectsPage.tsx
        ProjectDetailPage.tsx
        RequirementsPage.tsx
        RequirementDetailDrawer.tsx
        WorkItemsPage.tsx
        WorkItemDetailDrawer.tsx
        DependencyGraphPage.tsx
```

根 `Cargo.toml` 增加 workspace member：

```toml
members = [
  "chat_app_server_rs",
  "crates/chatos_builtin_tools",
  "crates/chatos_ai_runtime",
  "crates/memory_engine_sdk",
  "crates/chatos_mcp_runtime",
  "task_runner_service/backend",
  "project_management_service/backend"
]
```

## 5. 后端技术方案

### 5.1 技术栈

- Rust 2021。
- Web 框架：Axum 0.7。
- 序列化：Serde / serde_json。
- 数据库：默认使用 MongoDB，保留 SQLite fallback 方便本地快速测试。
- 异步运行时：Tokio。
- 认证：复用 User Service token verify 方案，不在项目服务里单独维护用户密码。
- 日志：tracing / tracing-subscriber。

### 5.2 配置项

建议使用独立环境变量前缀：

```text
PROJECT_SERVICE_HOST=0.0.0.0
PROJECT_SERVICE_PORT=39210
PROJECT_SERVICE_MONGODB_DATABASE=project_management_service
PROJECT_SERVICE_DATABASE_URL=mongodb://admin:admin@mongo:27017/project_management_service?authSource=admin
PROJECT_SERVICE_USER_SERVICE_BASE_URL=http://user-service-backend:39190
PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS=5000
PROJECT_SERVICE_TASK_RUNNER_BASE_URL=http://task-runner-backend:39200
PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS=10000
PROJECT_SERVICE_SYNC_SECRET=...
```

TaskRunner adapter 是可选能力。第一阶段可以只定义配置和空实现，等项目管理闭环稳定后再接 TaskRunner。

## 6. 数据模型

### 6.1 projects

保存项目基础信息，承接当前 `TaskProjectRecord` 的基础字段。

```sql
CREATE TABLE IF NOT EXISTS projects (
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

CREATE INDEX IF NOT EXISTS idx_projects_owner_user_id
ON projects(owner_user_id);

CREATE INDEX IF NOT EXISTS idx_projects_status
ON projects(status);
```

说明：

- `description` 只作为短描述保留。
- `status` 建议枚举：`active / archived`。
- 删除项目采用 archive，不做物理删除。
- public 项目是否保留 `-1` 取决于 ChatOS 是否还需要默认项目空间；如果保留，仍按 owner scope 隔离。

### 6.2 project_profiles

项目一对一扩展表，用于保存项目背景和项目介绍。

```sql
CREATE TABLE IF NOT EXISTS project_profiles (
  project_id TEXT PRIMARY KEY,
  background TEXT,
  introduction TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
```

说明：

- `background`：项目背景，回答为什么做、业务上下文、约束来源。
- `introduction`：项目介绍，回答这个项目是什么、面向谁、核心能力是什么。
- 不把这两个字段放进 `projects`，避免基础列表查询加载长文本。

### 6.3 requirements

需求表。

```sql
CREATE TABLE IF NOT EXISTS requirements (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  parent_requirement_id TEXT,
  title TEXT NOT NULL,
  summary TEXT,
  detail TEXT,
  business_value TEXT,
  acceptance_criteria TEXT,
  source TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'draft',
  owner_user_id TEXT,
  assignee_user_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY(parent_requirement_id) REFERENCES requirements(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_requirements_project_id
ON requirements(project_id);

CREATE INDEX IF NOT EXISTS idx_requirements_project_status
ON requirements(project_id, status);
```

建议状态：

- `draft`：草稿。
- `reviewing`：评审中。
- `approved`：已确认。
- `in_progress`：实现中。
- `done`：已完成。
- `cancelled`：已取消。
- `archived`：已归档。

### 6.4 requirement_dependencies

需求前置关系表。

```sql
CREATE TABLE IF NOT EXISTS requirement_dependencies (
  requirement_id TEXT NOT NULL,
  prerequisite_requirement_id TEXT NOT NULL,
  relation_type TEXT NOT NULL DEFAULT 'blocks',
  created_at TEXT NOT NULL,
  PRIMARY KEY(requirement_id, prerequisite_requirement_id),
  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE,
  FOREIGN KEY(prerequisite_requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_requirement_dependencies_requirement_id
ON requirement_dependencies(requirement_id);

CREATE INDEX IF NOT EXISTS idx_requirement_dependencies_prerequisite_id
ON requirement_dependencies(prerequisite_requirement_id);
```

校验规则：

- 需求不能依赖自身。
- 前置需求必须属于同一个项目。
- 不能形成循环依赖。
- 依赖数量建议限制在 50 个以内。
- 依赖链深度建议限制在 200 以内，沿用 TaskRunner 当前的防护思路。

### 6.5 requirement_documents

需求技术总体文档表。第一阶段只要求一个需求对应一个实现技术总体文档，但用 `doc_type` 留扩展空间。

```sql
CREATE TABLE IF NOT EXISTS requirement_documents (
  id TEXT PRIMARY KEY,
  requirement_id TEXT NOT NULL,
  doc_type TEXT NOT NULL DEFAULT 'technical_overview',
  title TEXT NOT NULL,
  format TEXT NOT NULL DEFAULT 'markdown',
  content TEXT NOT NULL DEFAULT '',
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(requirement_id, doc_type),
  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
);
```

建议 `technical_overview` 文档模板包含：

- 背景和目标。
- 范围和非范围。
- 关键业务流程。
- 领域模型。
- 接口设计。
- 数据模型。
- 前端交互。
- 风险和开放问题。
- 验收点。

### 6.6 project_work_items

项目管理里的具体任务/工作项表，注意它不是 TaskRunner 的 `tasks`。

```sql
CREATE TABLE IF NOT EXISTS project_work_items (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  requirement_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'todo',
  priority INTEGER NOT NULL DEFAULT 0,
  assignee_user_id TEXT,
  estimate_points INTEGER,
  due_at TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  tags_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_id
ON project_work_items(project_id);

CREATE INDEX IF NOT EXISTS idx_project_work_items_requirement_id
ON project_work_items(requirement_id);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_status
ON project_work_items(project_id, status);
```

建议状态：

- `todo`：待处理。
- `ready`：已就绪。
- `in_progress`：进行中。
- `blocked`：阻塞。
- `done`：完成。
- `cancelled`：取消。
- `archived`：归档。

### 6.7 project_work_item_dependencies

项目工作项前置关系表。

```sql
CREATE TABLE IF NOT EXISTS project_work_item_dependencies (
  work_item_id TEXT NOT NULL,
  prerequisite_work_item_id TEXT NOT NULL,
  relation_type TEXT NOT NULL DEFAULT 'blocks',
  created_at TEXT NOT NULL,
  PRIMARY KEY(work_item_id, prerequisite_work_item_id),
  FOREIGN KEY(work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE,
  FOREIGN KEY(prerequisite_work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_work_item_dependencies_work_item_id
ON project_work_item_dependencies(work_item_id);

CREATE INDEX IF NOT EXISTS idx_project_work_item_dependencies_prerequisite_id
ON project_work_item_dependencies(prerequisite_work_item_id);
```

校验规则：

- 工作项不能依赖自身。
- 前置工作项必须属于同一个项目。
- 默认要求工作项依赖符合需求依赖方向：如果工作项跨需求依赖，两个需求之间应存在依赖关系，或者由管理员显式确认。
- 不能形成循环依赖。
- 已取消或已归档工作项不能作为新的前置项。

### 6.8 project_work_item_task_runner_links

可选映射表，用于把项目工作项和 TaskRunner 执行任务关联起来。

```sql
CREATE TABLE IF NOT EXISTS project_work_item_task_runner_links (
  id TEXT PRIMARY KEY,
  work_item_id TEXT NOT NULL,
  task_runner_task_id TEXT NOT NULL,
  task_runner_run_id TEXT,
  link_type TEXT NOT NULL DEFAULT 'execution',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(work_item_id, task_runner_task_id),
  FOREIGN KEY(work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE
);
```

规则：

- 这张表只保存外部执行系统映射，不参与项目工作项主状态。
- `ProjectWorkItem.status` 是项目管理状态；TaskRunner 的 run status 是外部执行状态。
- 如果未来要“由工作项创建 TaskRunner 执行任务”，必须通过 `task_runner_adapter` 完成，不能直接写 TaskRunner 数据库。

## 7. API 设计

统一前缀：`/api`。

### 7.1 项目

```text
GET    /api/projects
POST   /api/projects
GET    /api/projects/:project_id
PATCH  /api/projects/:project_id
DELETE /api/projects/:project_id
```

`POST /api/projects`：

```json
{
  "name": "项目名",
  "root_path": "/path/to/project",
  "git_url": "git@github.com:org/repo.git",
  "description": "短描述"
}
```

### 7.2 项目详情

```text
GET /api/projects/:project_id/profile
PUT /api/projects/:project_id/profile
```

`PUT /api/projects/:project_id/profile`：

```json
{
  "background": "项目背景",
  "introduction": "项目介绍"
}
```

### 7.3 需求

```text
GET    /api/projects/:project_id/requirements
POST   /api/projects/:project_id/requirements
GET    /api/requirements/:requirement_id
PATCH  /api/requirements/:requirement_id
DELETE /api/requirements/:requirement_id
```

建议列表支持查询参数：

```text
status
keyword
priority
parent_requirement_id
limit
offset
```

### 7.4 需求依赖

```text
GET /api/requirements/:requirement_id/dependencies
PUT /api/requirements/:requirement_id/dependencies
GET /api/requirements/:requirement_id/dependency-graph
```

`PUT /api/requirements/:requirement_id/dependencies`：

```json
{
  "prerequisite_requirement_ids": ["req_1", "req_2"]
}
```

### 7.5 需求技术总体文档

```text
GET /api/requirements/:requirement_id/technical-overview
PUT /api/requirements/:requirement_id/technical-overview
```

`PUT /api/requirements/:requirement_id/technical-overview`：

```json
{
  "title": "实现技术总体文档",
  "format": "markdown",
  "content": "# 技术方案..."
}
```

### 7.6 项目工作项

```text
GET    /api/projects/:project_id/work-items
GET    /api/requirements/:requirement_id/work-items
POST   /api/requirements/:requirement_id/work-items
GET    /api/work-items/:work_item_id
PATCH  /api/work-items/:work_item_id
DELETE /api/work-items/:work_item_id
```

`POST /api/requirements/:requirement_id/work-items`：

```json
{
  "title": "实现登录页面",
  "description": "使用 AntD Form 完成登录页",
  "priority": 10,
  "assignee_user_id": "user_id",
  "estimate_points": 3,
  "tags": ["frontend"]
}
```

### 7.7 工作项依赖

```text
GET /api/work-items/:work_item_id/dependencies
PUT /api/work-items/:work_item_id/dependencies
GET /api/work-items/:work_item_id/dependency-graph
```

`PUT /api/work-items/:work_item_id/dependencies`：

```json
{
  "prerequisite_work_item_ids": ["wi_1", "wi_2"]
}
```

### 7.8 项目总依赖图

```text
GET /api/projects/:project_id/dependency-graph
```

返回内容建议同时包含：

- 需求节点与需求依赖边。
- 工作项节点与工作项依赖边。
- 需求到工作项的包含关系。
- 是否 ready、被哪些节点阻塞。

## 8. 领域规则

### 8.1 Owner scope

- 普通用户只能访问自己 owner scope 下的项目、需求和工作项。
- agent token 访问时使用 `owner_user_id` 作为资源归属。
- admin 可以跨 owner 查看和管理。
- 所有写接口必须校验项目未归档。

### 8.2 项目归档

- 项目归档后，需求、工作项、文档都只读。
- 可保留单独的恢复接口：`POST /api/projects/:id/restore`，第一阶段可以不做。

### 8.3 需求状态推进

第一阶段采用手动状态流转，不做自动强制：

```text
draft -> reviewing -> approved -> in_progress -> done
```

约束建议：

- 需求存在未完成前置需求时，不允许进入 `in_progress`。
- 需求下存在未完成工作项时，不允许进入 `done`，除非管理员强制。
- 已取消、已归档需求不能作为新的前置需求。

### 8.4 工作项状态推进

第一阶段采用手动状态流转：

```text
todo -> ready -> in_progress -> done
```

约束建议：

- 工作项存在未完成前置工作项时，不允许进入 `ready/in_progress`。
- 工作项所属需求未进入 `approved/in_progress` 时，不允许进入 `in_progress`。
- 已取消、已归档工作项不能作为新的前置工作项。

## 9. 与 TaskRunner 的关系

### 9.1 拆分后的职责

TaskRunner 保持执行系统定位：

- 管理 TaskRunnerTask。
- 执行 run。
- 管理 MCP、模型、远程工具、执行日志。
- 处理执行任务之间的执行前置关系。

项目管理微服务保持项目管理定位：

- 管理项目、需求、项目工作项。
- 管理需求依赖和项目工作项依赖。
- 管理技术文档。

### 9.2 迁移 TaskRunner 里的 Project 能力

当前 TaskRunner 已有 `task_projects`。拆分后建议：

1. 在新项目服务中创建 `projects/project_profiles`。
2. 写一次性迁移脚本，从 TaskRunner `task_projects` 复制基础项目数据到新服务。
3. TaskRunner 内部保留 `tasks.project_id` 作为执行任务的项目上下文引用，但不再维护项目基础信息。
4. TaskRunner 的 `/api/projects` 逐步下线或改为代理到项目管理服务。
5. ChatOS 的 ProjectService 从 TaskRunner adapter 改为 Project Management adapter。

注意：TaskRunner 的 `tasks.project_id` 只是外部引用，不代表项目管理里的工作项。

### 9.3 可选执行映射

当用户在项目管理里希望把某个项目工作项交给 TaskRunner 执行时，推荐流程：

1. 用户在 `ProjectWorkItem` 详情页点击“创建执行任务”。
2. 项目服务调用 TaskRunner API 创建 `TaskRunnerTask`。
3. 项目服务写入 `project_work_item_task_runner_links`。
4. 前端在工作项详情中展示外部执行任务链接和最近 run 状态。
5. 工作项是否完成仍由项目管理状态决定，不直接跟随 TaskRunner run 状态自动完成。

## 10. 前端方案

### 10.1 技术栈

- React 18。
- Vite。
- Ant Design 5。
- React Router。
- TanStack React Query。
- dayjs。

与当前 `task_runner_service/frontend` 保持一致，降低维护成本。

### 10.2 信息架构

推荐左侧导航：

```text
项目
需求
项目任务
依赖图
设置
```

项目详情页使用 Tabs：

```text
概览 / 项目详情 / 需求 / 项目任务 / 依赖图 / 设置
```

### 10.3 页面设计

#### ProjectsPage

- 表格展示：项目名、状态、root_path、git_url、owner、更新时间。
- 操作：新建、编辑、归档、进入详情。
- 顶部过滤：状态、关键词。

#### ProjectDetailPage

- 顶部显示项目基础信息和状态。
- `概览`：需求统计、工作项统计、阻塞项、最近更新。
- `项目详情`：编辑项目背景和项目介绍。
- `需求`：当前项目需求表。
- `项目任务`：当前项目工作项表，可按需求过滤。
- `依赖图`：需求和工作项依赖视图。

#### RequirementsPage / RequirementDetailDrawer

- 需求表字段：标题、状态、优先级、前置需求、工作项数量、更新时间。
- 详情抽屉：基础信息、验收标准、前置需求、技术总体文档。
- 技术文档第一阶段使用 Markdown 文本编辑器，后续再考虑富文本或版本 diff。

#### WorkItemsPage / WorkItemDetailDrawer

- 工作项表字段：标题、所属需求、状态、优先级、负责人、前置工作项、更新时间。
- 支持从需求详情中直接新增工作项。
- 支持设置前置工作项。
- 支持查看可选 TaskRunner 执行链接。

#### DependencyGraphPage

第一阶段用 AntD `Tree`、`Table`、`Tag` 展示依赖和阻塞链即可，避免引入复杂图形库。

第二阶段如果需要可视化图，再引入 AntV G6 或 React Flow。

## 11. 实施阶段

### 阶段 1：服务脚手架

- 新增 `project_management_service/backend`。
- 接入根 workspace。
- 实现配置加载、健康检查、User Service token verify、`CurrentUser`。
- 实现 Store 抽象、MongoDB 初始化与索引创建。
- 新增 Dockerfile 和 docker-compose 服务项。

验收：

- `GET /api/health` 正常。
- 带 User Service token 可访问受保护接口。
- MongoDB 可自动创建集合索引；显式使用 SQLite 连接串时可自动创建本地测试表。

### 阶段 2：项目基础信息与项目详情

- 实现 `projects` 和 `project_profiles`。
- 实现项目 CRUD。
- 实现项目详情 GET/PUT。
- 从 TaskRunner `task_projects` 写迁移脚本。
- TaskRunner `/api/projects` 临时代理或标记 deprecated。

验收：

- 可以创建、编辑、归档项目。
- 可以编辑项目背景和项目介绍。
- 项目列表不加载长文本详情。

### 阶段 3：需求模块

- 实现 `requirements`。
- 实现需求 CRUD、列表过滤、状态流转。
- 实现 `requirement_dependencies`。
- 实现需求依赖图和循环依赖校验。
- 实现 `requirement_documents` 的技术总体文档。

验收：

- 需求可创建、编辑、归档。
- 需求可设置前置需求。
- 循环依赖会被拒绝。
- 每个需求可以维护一份技术总体文档。

### 阶段 4：项目工作项模块

- 实现 `project_work_items`。
- 实现工作项 CRUD、按项目/需求列表。
- 实现 `project_work_item_dependencies`。
- 实现工作项依赖图和阻塞判断。
- 明确工作项与 TaskRunner task 的模型隔离。

验收：

- 每个需求下可创建多个项目工作项。
- 工作项可设置前置工作项。
- 工作项循环依赖会被拒绝。
- 项目工作项不写入 TaskRunner 的 `tasks` 表。

### 阶段 5：前端管理台

- 新增 `project_management_service/frontend`。
- 实现登录态复用、API client、AppShell。
- 实现项目列表、项目详情、需求、工作项、依赖图页面。
- UI 风格遵循 AntD 管理台风格，信息密度适中，不做营销页。

验收：

- 用户可以通过 UI 完成项目、项目详情、需求、工作项、依赖配置。
- 所有表格、抽屉、表单在桌面宽度下无明显溢出。

### 阶段 6：TaskRunner 可选集成

- 新增 `task_runner_adapter`。
- 新增 `project_work_item_task_runner_links`。
- 实现“由项目工作项创建 TaskRunner 执行任务”。
- 实现查看外部执行任务状态和最近 run。

验收：

- 项目工作项可以链接一个或多个 TaskRunner 执行任务。
- TaskRunner 执行状态不污染项目工作项主状态。

### 阶段 7：联调与下线旧入口

- ChatOS ProjectService 改为调用项目管理服务。
- TaskRunner Project API 改为代理或下线。
- 更新环境变量、docker-compose、重启脚本和 README。
- 补充迁移回滚方案。

验收：

- 新项目服务是项目基础信息唯一数据源。
- TaskRunner 不再维护 `task_projects` 主数据。
- 旧数据可以完整迁移并按 owner scope 正确隔离。

## 12. 测试计划

### 后端单元测试

- 项目名称、Git URL、root_path 归一化。
- 项目背景/介绍一对一 upsert。
- 需求自依赖拒绝。
- 需求跨项目依赖拒绝。
- 需求循环依赖拒绝。
- 工作项自依赖拒绝。
- 工作项跨项目依赖拒绝。
- 工作项循环依赖拒绝。
- 已归档项目下写操作拒绝。
- owner scope 访问控制。

### API 集成测试

- 项目 CRUD。
- 项目详情 GET/PUT。
- 需求 CRUD + dependency graph。
- 技术总体文档 GET/PUT。
- 工作项 CRUD + dependency graph。
- 可选 TaskRunner link 创建和查询。

### 前端验证

- 项目列表、新建、编辑、归档。
- 项目详情页 Tabs。
- 需求表和详情抽屉。
- 工作项表和详情抽屉。
- 依赖选择器只展示当前项目内可选节点。
- 表格列在常见桌面宽度下不重叠。

## 13. 风险与处理

### 命名混淆

风险：项目任务和 TaskRunner 任务混在一起。

处理：

- 后端模型统一命名 `ProjectWorkItem`。
- 数据表统一命名 `project_work_items`。
- 前端中文可以显示“项目任务”，代码中避免使用裸 `Task`。
- TaskRunner 相关对象必须带 `TaskRunner` 前缀。

### 旧数据迁移

风险：现有 TaskRunner project 数据和 ChatOS project 适配关系不一致。

处理：

- 先做只读迁移报告，列出 project id、owner、root_path、git_url、description。
- 迁移时保留原 id。
- 迁移后 TaskRunner 只保存 project id 引用。
- 保留回滚脚本：项目服务不可用时 TaskRunner 旧 Project API 可以临时恢复只读。

### 依赖图性能

风险：需求和工作项依赖增长后，图查询变慢。

处理：

- 第一阶段限制依赖数量和深度。
- 为依赖边建立双向索引。
- 图接口支持只查某个节点局部图。
- 后续再引入缓存或 materialized path。

### 文档版本

风险：技术总体文档需要历史版本。

处理：

- 第一阶段只保留当前版本和 `version`。
- 第二阶段新增 `requirement_document_versions`，每次保存写历史。

## 14. 建议优先级

最高优先级：

1. 建独立服务骨架和认证。
2. 项目基础信息 + 项目详情一对一表。
3. 需求 + 需求依赖 + 技术总体文档。
4. 项目工作项 + 工作项依赖。

中优先级：

1. 前端完整管理台。
2. 数据迁移脚本。
3. ChatOS ProjectService adapter 切换。

低优先级：

1. TaskRunner 执行任务映射。
2. 图形化依赖画布。
3. 技术文档版本 diff。

## 15. 第一版最小可交付范围

第一版建议不要接 TaskRunner 执行映射，先把项目管理领域闭环做扎实：

- 项目 CRUD。
- 项目背景、项目介绍。
- 需求 CRUD。
- 需求前置关系。
- 需求技术总体文档。
- 需求下项目工作项 CRUD。
- 项目工作项前置关系。
- React + AntD 管理台。

TaskRunner 集成放到第二版，避免“项目任务”和“执行任务”在第一版就发生语义污染。
