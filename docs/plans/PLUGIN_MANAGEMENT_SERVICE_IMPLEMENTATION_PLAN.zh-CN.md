# 插件管理微服务实施方案

## 当前实施进度（2026-07-10）

第一阶段“插件管理微服务自身建设”已经完成：

- 已创建 Rust + axum + MongoDB 后端。
- 已创建 React + Ant Design 管理前端。
- 已接入 User Service 登录和 token 校验。
- 已实现 MCP、skills、skill packages、system agents 和 agent bindings 管理。
- 已实现 runtime capability preview 和 skill package 展开。
- 已实现系统 builtin MCP、system agents 和默认 bindings 的幂等 seed。
- 已接入 Cargo workspace、Docker Compose 和构建配置。
- 后端单测、前端类型检查、生产构建、Docker Compose 校验和真实 API 烟测均已通过。

当前尚未开始 Chatos、Task Runner、Project Management、Memory Engine 和 Local Connector 的业务调用链集成。这些内容继续按后续阶段实施。

### 系统 Agent 配置设计修正

系统 Agent 配置页面仅供 `super_admin` 使用，并收敛为内部 MCP 能力矩阵：

- 管理员只配置每个系统 Agent 对每个 `system_private` MCP 的三种状态：不可用、可选、必需。
- 页面和公开管理 API 不再暴露 binding scope、owner、priority、task profile、project source、runtime provider、schedule mode 等运行时字段。
- 项目是本地项目还是云项目，以及应该选择哪个运行子实现，由具体 MCP 和运行时根据项目上下文自行判断。
- 用户通过 Local Connector Client 添加的 MCP 和 skills 默认自动提供给 `task_runner_run_phase`；该执行循环同时承载普通任务和 `chatos_plan` 任务 profile。
- 用户本机资源不需要管理员在系统 Agent 页面逐项绑定。

## 1. 背景与目标

当前项目中 MCP、skills、系统内部 agent 能力配置分散在多个服务中：

- Chatos 负责用户 MCP 配置、内置 MCP 拼装、memory skills 管理。
- Task Runner 负责任务运行阶段的 MCP 能力解析。
- Project Management Service 内部有项目环境分析 agent，并直接拼装项目文件 MCP、沙箱镜像 MCP。
- Local Connector Service / Local Connector Client 负责把用户本机能力通过 relay 暴露给云端服务。
- 系统内置 MCP 目前主要通过 `chatos_mcp_runtime::BuiltinMcpKind` 和 Chatos 的 `builtin_mcp.rs` 管理。

这些能力需要统一纳入一个新的“插件管理”微服务中管理。该服务的核心职责不是直接执行 MCP 或 skills，而是统一管理：

1. MCP 和 skills 的目录、元数据、归属、可见性、启用状态。
2. 用户隔离：用户私有 MCP/skills 只能自己看见和使用。
3. 管理员发布能力：`super_admin` 创建的 public MCP/skills 所有用户可见可用。
4. 系统能力：系统内置 MCP/skills 归属 admin，但主要用于系统内部 agent 配置，默认不作为普通用户市场能力暴露。
5. 系统内部 agent 能力矩阵：配置 Chatos、Task Runner、Project Management、Local Connector Client 等系统 agent 可见和可使用的 MCP/skills。
6. 本机-only 能力：用户后期添加的 MCP/skills 可能只能在本机运行，需要通过 Local Connector Client 暴露，插件管理服务必须从设计上把这一点作为一等能力处理。

## 2. 现有代码关键点

本方案基于当前仓库结构和已有实现设计，重点参考以下位置：

- Chatos MCP 模型：`chatos/backend/src/models/mcp_config.rs`
- Chatos MCP CRUD：`chatos/backend/src/api/configs.rs`
- Chatos MCP 加载：`chatos/backend/src/services/mcp_loader.rs`
- Chatos 内置 MCP：`chatos/backend/src/services/builtin_mcp.rs`
- Chatos skills 模型：`chatos/backend/src/models/memory_skill.rs`
- Chatos skills 仓储：`chatos/backend/src/repositories/memory_skills.rs`
- Chatos skills 服务：`chatos/backend/src/services/chatos_skills.rs`
- 系统内置 MCP catalog：`crates/chatos_mcp_runtime/src/builtin_catalog.rs`
- Task Runner MCP 能力解析：`task_runner_service/backend/src/services/mcp_resolution.rs`
- Project Management 环境 agent：`project_management_service/backend/src/services/environment_agent.rs`
- Project Management MCP server 拼装：`project_management_service/backend/src/services/environment_agent/mcp_servers.rs`
- Local Connector relay：`local_connector_service/backend/src/api/mod.rs`
- Local Connector Client MCP service：`local_connector_client/core/src/mcp/service.rs`
- Local Connector Client MCP provider：`local_connector_client/core/src/mcp/provider.rs`
- Local Connector Client 本地能力选择：`local_connector_client/core/src/mcp/selection.rs`
- 用户服务认证和角色：`user_service/backend/src/api/auth.rs`、`user_service/backend/src/models.rs`

## 3. 总体架构

新增服务：

- 后端目录：`plugin_management_service/backend`
- 前端目录：`plugin_management_service/frontend`
- 服务名：`plugin-management-service`
- 后端技术栈：Rust、axum、mongodb、tower-http、serde、reqwest
- 前端技术栈：React、TypeScript、Ant Design、TanStack Query、Vite
- 数据库：MongoDB
- 鉴权：Bearer token，经 `user_service /api/auth/verify` 校验
- 服务发现：沿用 `chatos_service_runtime`

插件管理服务作为“能力控制面”，不作为通用 MCP runtime。它输出各运行方需要的能力解析结果：

- Chatos 请求“当前用户可见 MCP/skills”。
- Task Runner 请求“某个系统 agent 在某个运行场景下允许使用的 MCP/skills”。
- Project Management 请求“项目环境 agent 允许使用的 MCP/skills”。
- Local Connector Client 上报或同步“本机可执行 MCP/skills manifest”。

执行层仍保留在现有组件：

- 云端 HTTP MCP：由各服务直接访问。
- 云端 stdio MCP：由对应运行服务在其容器中启动。
- 系统 builtin MCP：由现有 `chatos_mcp_runtime` / `chatos_builtin_tools` 执行。
- 用户本机 MCP/skills：通过 `local_connector_service -> local_connector_client` relay 执行或读取。

## 4. 核心设计原则

### 4.1 控制面和执行面分离

插件管理服务不直接执行用户工具，避免把用户本机能力错误假设为云端可运行。它只保存：

- metadata
- 权限
- 可见性
- 绑定关系
- runtime reference
- 本机 connector reference
- 健康检查结果

真正执行由调用方根据解析结果选择：

- 直接 HTTP
- 云端 stdio
- builtin provider
- Local Connector relay

### 4.2 用户隔离优先

所有用户创建的 MCP/skills 默认 `private`。查询必须默认带上 owner 限制：

- 普通用户只能看到自己的 private 资源和 admin public 资源。
- 普通用户不能看到其他用户 private 资源。
- 普通用户不能看到 system_private 资源，除非运行时解析结果只用于系统内部 agent，且该 agent 对该用户生效。
- `super_admin` 可以按 owner 查询和管理。

### 4.3 admin public 和 system_private 分离

需要区分两类 admin 资源：

- `public`：面向所有用户可见可用，比如管理员发布的通用 HTTP MCP、通用 skill。
- `system_private`：系统内置能力或系统内部 agent 专用能力，普通用户目录不可见，主要用于管理 agent 能力矩阵。

用户提到“系统本身提供的 MCP 都直接属于 admin 下，但是系统内部 MCP 都是 private”，实现上建议使用 `system_private`，这样不会和 admin 个人 private 工具混淆。

### 4.4 本机-only 是一等运行方式

本机-only MCP/skills 不能只作为一个 `stdio` command 字符串存储。必须显式建模：

- 归属用户
- device_id
- workspace_id
- local path 或 manifest id
- 是否需要本机在线
- 是否可 fallback
- 最近一次上报 hash
- 最近一次健康检查

如果本机不在线，UI 和 runtime resolver 必须明确返回 unavailable，而不是让调用方在执行阶段才失败。

## 5. 领域模型

### 5.1 资源类型

统一抽象为 capability resource，但落库可以拆成 MCP 和 skill 两类。

资源类别：

- `mcp`
- `skill`
- `skill_package`

MCP runtime 类型：

- `builtin`
- `http`
- `stdio_cloud`
- `local_connector_stdio`
- `local_connector_http`
- `local_connector_builtin_proxy`

Skill content 类型：

- `inline_content`
- `cloud_package`
- `git_package`
- `local_connector_file`
- `local_connector_package`

可见性：

- `private`
- `public`
- `system_private`

来源类型：

- `system_seed`
- `admin_created`
- `user_created`
- `imported_legacy_chatos`
- `local_connector_discovered`

### 5.2 系统内部 agent 范围

本服务中的 agent 指系统内部具有独立 MCP/skills 能力边界的智能体角色或运行模式，不要求每一项都对应独立进程，也不包含 Chatos 用户创建的联系人 agent。

按当前代码执行链审计，registry 登记五个 agent 能力角色：

- `chatos_conversation_agent`：Chatos 普通对话模式；联系人提供动态角色上下文，但系统能力矩阵统一管理。
- `chatos_planning_agent`：Chatos `plan_mode` 规划模式；强制接入 Task Runner 并使用 `chatos_plan` task profile。
- `task_runner_run_phase`：Task Runner 唯一的任务模型执行循环，同时执行普通任务和 `chatos_plan` profile。
- `project_management_agent`：Project Management Service 的项目运行环境初始化智能体。
- `local_connector_command_approval_agent`：Local Connector Client 的本机命令审批智能体。

以下名称不代表独立系统智能体，不得登记：

- `chatos_plan_agent`：旧的模糊 key；由明确的 `chatos_planning_agent` 代替，避免和 Task Runner task profile 混淆。
- `chatos_async_planner`：Task Runner 暴露给 Chatos 的 MCP 工具入口。
- `chatos_chat_runtime`：运行模块名称；由系统级能力角色 `chatos_conversation_agent` 代替。
- `task_runner_plan_phase`：当前没有独立模型执行入口。
- `project_environment_agent`：与 `project_management_agent` 指向同一实现的重复别名。
- `local_connector_client_agent`：客户端/服务名称，不是模型智能体。
- `memory_engine_context_agent`：没有独立模型执行链。

Chatos 中的 system prompt 辅助、Agent Builder 和浏览器视觉等功能是一次性模型工具，不具有独立工具循环和生命周期，也不登记为系统 Agent。

## 6. MongoDB Collection 设计

### 6.1 `plugin_mcps`

用途：MCP catalog。

建议字段：

```json
{
  "_id": "ObjectId",
  "id": "string",
  "owner_user_id": "string",
  "owner_kind": "admin|user|system",
  "visibility": "private|public|system_private",
  "source_kind": "system_seed|admin_created|user_created|imported_legacy_chatos|local_connector_discovered",
  "name": "string",
  "display_name": "string",
  "description": "string",
  "enabled": true,
  "runtime": {
    "kind": "builtin|http|stdio_cloud|local_connector_stdio|local_connector_http|local_connector_builtin_proxy",
    "builtin_kind": "CodeMaintainerRead",
    "server_name": "code_maintainer_read",
    "command": "string",
    "args": ["string"],
    "env": { "KEY": "VALUE" },
    "cwd": "string",
    "url": "string",
    "headers": { "Authorization": "Bearer ..." },
    "local_connector": {
      "device_id": "string",
      "workspace_id": "string",
      "manifest_id": "string",
      "relative_path": "string",
      "requires_online": true
    }
  },
  "security": {
    "allow_writes": false,
    "max_file_bytes": 262144,
    "max_write_bytes": 5242880,
    "search_limit": 40,
    "allowed_tool_names": ["string"],
    "blocked_tool_names": ["string"]
  },
  "metadata": {
    "tags": ["string"],
    "version": "string",
    "homepage": "string"
  },
  "created_by": "string",
  "updated_by": "string",
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

索引：

- unique `id`
- `{ owner_user_id: 1, visibility: 1, enabled: 1 }`
- `{ visibility: 1, enabled: 1 }`
- `{ "runtime.kind": 1 }`
- `{ "runtime.local_connector.device_id": 1, "runtime.local_connector.workspace_id": 1 }`
- text index: `name`、`display_name`、`description`、`metadata.tags`

### 6.2 `plugin_skills`

用途：skill catalog。

建议字段：

```json
{
  "_id": "ObjectId",
  "id": "string",
  "owner_user_id": "string",
  "owner_kind": "admin|user|system",
  "visibility": "private|public|system_private",
  "source_kind": "system_seed|admin_created|user_created|imported_legacy_chatos|local_connector_discovered",
  "name": "string",
  "display_name": "string",
  "description": "string",
  "enabled": true,
  "content": {
    "kind": "inline_content|cloud_package|git_package|local_connector_file|local_connector_package",
    "inline": "string",
    "package_id": "string",
    "source_path": "string",
    "repository": "string",
    "branch": "string",
    "local_connector": {
      "device_id": "string",
      "workspace_id": "string",
      "manifest_id": "string",
      "relative_path": "string",
      "requires_online": true
    }
  },
  "metadata": {
    "category": "string",
    "version": "string",
    "tags": ["string"],
    "argument_hint": "string"
  },
  "created_by": "string",
  "updated_by": "string",
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

索引：

- unique `id`
- `{ owner_user_id: 1, visibility: 1, enabled: 1 }`
- `{ visibility: 1, enabled: 1 }`
- `{ "content.kind": 1 }`
- `{ "content.local_connector.device_id": 1, "content.local_connector.workspace_id": 1 }`

### 6.3 `plugin_skill_packages`

用途：管理一组 skills 的安装来源，替代或承接 Chatos `memory_skill_plugins`。

建议字段：

```json
{
  "id": "string",
  "owner_user_id": "string",
  "visibility": "private|public|system_private",
  "source_kind": "git|local_connector|inline_bundle|system_seed",
  "name": "string",
  "description": "string",
  "repository": "string",
  "branch": "string",
  "cache_ref": "string",
  "local_connector": {
    "device_id": "string",
    "workspace_id": "string",
    "manifest_id": "string",
    "relative_path": "string"
  },
  "skill_ids": ["string"],
  "installed": true,
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

### 6.4 `plugin_agents`

用途：系统内部 agent registry。

建议字段：

```json
{
  "id": "string",
  "agent_key": "project_management_agent",
  "display_name": "Project Management Agent",
  "service_name": "project-service",
  "scope": "system_internal",
  "description": "string",
  "enabled": true,
  "managed_by": "system|admin",
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

索引：

- unique `agent_key`
- `{ service_name: 1, enabled: 1 }`

### 6.5 `plugin_agent_bindings`

用途：agent 能力绑定矩阵。

绑定层级：

- `global_default`：admin 配置，对所有用户默认生效。
- `user_override`：某个用户的私有覆盖。
- `system_required`：系统强制绑定，不允许普通管理界面删除。

建议字段：

```json
{
  "id": "string",
  "agent_key": "task_runner_run_phase",
  "binding_scope": "global_default|user_override|system_required",
  "owner_user_id": "string",
  "resource_kind": "mcp|skill|skill_package",
  "resource_id": "string",
  "enabled": true,
  "required": false,
  "priority": 100,
  "conditions": {
    "task_profile": "chatos_plan",
    "project_source_type": "local|cloud|harness",
    "runtime_provider": "local_connector|harness|sandbox_manager",
    "schedule_mode": "immediate|background"
  },
  "created_by": "string",
  "updated_by": "string",
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

索引：

- `{ agent_key: 1, binding_scope: 1, owner_user_id: 1 }`
- `{ resource_kind: 1, resource_id: 1 }`
- unique partial index 可用于避免同一 scope 重复绑定。

### 6.6 `plugin_resource_checks`

用途：保存本地 connector 状态、资源健康检查、工具列表快照。

建议字段：

```json
{
  "id": "string",
  "resource_kind": "mcp|skill|skill_package",
  "resource_id": "string",
  "owner_user_id": "string",
  "status": "available|unavailable|degraded|unknown",
  "last_checked_at": "RFC3339",
  "last_error": "string",
  "tool_snapshot": [
    {
      "name": "string",
      "description": "string",
      "input_schema": {}
    }
  ],
  "manifest_hash": "string"
}
```

### 6.7 `plugin_audit_logs`

用途：记录敏感配置变更。

建议字段：

```json
{
  "id": "string",
  "actor_user_id": "string",
  "actor_role": "super_admin|user",
  "action": "create|update|delete|bind|unbind|check|import|seed",
  "resource_kind": "mcp|skill|skill_package|agent|agent_binding",
  "resource_id": "string",
  "owner_user_id": "string",
  "visibility": "private|public|system_private",
  "before": {},
  "after": {},
  "created_at": "RFC3339"
}
```

## 7. 后端模块设计

后端目录结构建议：

```text
plugin_management_service/backend
├── Cargo.toml
└── src
    ├── main.rs
    ├── lib.rs
    ├── config.rs
    ├── state.rs
    ├── auth.rs
    ├── models
    │   ├── mod.rs
    │   ├── mcp.rs
    │   ├── skill.rs
    │   ├── agent.rs
    │   ├── binding.rs
    │   └── runtime.rs
    ├── store
    │   ├── mod.rs
    │   ├── indexes.rs
    │   ├── mcps.rs
    │   ├── skills.rs
    │   ├── skill_packages.rs
    │   ├── agents.rs
    │   ├── bindings.rs
    │   ├── checks.rs
    │   └── audit.rs
    ├── api
    │   ├── mod.rs
    │   ├── router.rs
    │   ├── mcps.rs
    │   ├── skills.rs
    │   ├── agents.rs
    │   ├── bindings.rs
    │   ├── runtime.rs
    │   └── local_connector.rs
    ├── services
    │   ├── mod.rs
    │   ├── authorization.rs
    │   ├── catalog.rs
    │   ├── runtime_resolver.rs
    │   ├── seed.rs
    │   ├── legacy_import.rs
    │   └── local_connector_client.rs
    └── clients
        ├── mod.rs
        ├── user_service.rs
        └── local_connector_service.rs
```

### 7.1 Config

配置字段：

- `PLUGIN_MANAGEMENT_SERVICE_HOST`
- `PLUGIN_MANAGEMENT_SERVICE_PORT`
- `PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL`
- `PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE`
- `PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL`
- `PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS`
- `PLUGIN_MANAGEMENT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL`
- `PLUGIN_MANAGEMENT_SERVICE_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS`
- `PLUGIN_MANAGEMENT_SERVICE_ADMIN_USER_ID`
- `PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES`
- `PLUGIN_MANAGEMENT_SERVICE_INTERNAL_API_SECRET`

默认端口建议：`39260`。

### 7.2 Auth

认证策略：

1. 所有管理 API 默认要求 Bearer token。
2. 后端调用 `user_service /api/auth/verify`。
3. 从返回 principal 中读取：
   - `user_id`
   - `role`
   - `username`
   - `owner_user_id`，如果是 agent token 或代理身份。
4. `role == "super_admin"` 视为管理员。
5. 运行时内部接口可额外支持 internal secret，但不能替代用户隔离判断。

权限规则：

- 创建 `public`：仅 `super_admin`
- 创建 `system_private`：仅 `super_admin`
- 修改别人的 private 资源：仅 `super_admin`
- 查看别人的 private 资源：仅 `super_admin`
- 查看 system_private catalog：仅 `super_admin` 或 runtime resolver 内部路径
- 修改 global agent binding：仅 `super_admin`
- 修改 user override：用户自己或 `super_admin`

## 8. API 设计

### 8.1 MCP Catalog

`GET /api/mcps`

查询当前用户可见 MCP。

Query：

- `q`
- `visibility`
- `runtime_kind`
- `enabled`
- `owner_user_id`，仅 super_admin 有效
- `include_system`，仅 super_admin 有效

返回：

```json
{
  "items": [],
  "total": 0
}
```

`POST /api/mcps`

创建 MCP。普通用户只能创建 private。

`GET /api/mcps/:mcp_id`

读取 MCP 详情。

`PATCH /api/mcps/:mcp_id`

更新 MCP。

`DELETE /api/mcps/:mcp_id`

删除或软删除 MCP。system seed 资源建议只允许 disable，不允许 hard delete。

`POST /api/mcps/:mcp_id/check`

健康检查：

- HTTP MCP：尝试 initialize/list_tools。
- builtin MCP：检查 kind 是否存在。
- local connector MCP：调用 local connector service 检查 device/workspace 在线，并通过 relay list_tools。

### 8.2 Skills Catalog

`GET /api/skills`

查询当前用户可见 skills。

`POST /api/skills`

创建 skill。

`GET /api/skills/:skill_id`

读取 skill。

`PATCH /api/skills/:skill_id`

更新 skill。

`DELETE /api/skills/:skill_id`

删除或软删除 skill。

`POST /api/skills/:skill_id/check`

检查 skill 内容是否可读取、manifest 是否一致。

### 8.3 Skill Packages

`GET /api/skill-packages`

`POST /api/skill-packages`

`GET /api/skill-packages/:package_id`

`PATCH /api/skill-packages/:package_id`

`DELETE /api/skill-packages/:package_id`

`POST /api/skill-packages/:package_id/sync`

同步 git 或 local connector package 中的 skills。

### 8.4 System Agents

`GET /api/system-agents`

列出系统内部 agent。普通用户可读 basic 信息，super_admin 可读完整管理信息。

`POST /api/system-agents`

创建 agent registry，仅 super_admin。

`PATCH /api/system-agents/:agent_key`

更新 agent 信息，仅 super_admin。

`GET /api/system-agents/:agent_key/bindings`

读取 agent 当前绑定。

Query：

- `scope=global_default|user_override|effective`
- `owner_user_id`，super_admin 可指定

`PUT /api/system-agents/:agent_key/bindings`

批量更新绑定。

`POST /api/system-agents/:agent_key/bindings`

新增绑定。

`DELETE /api/system-agents/:agent_key/bindings/:binding_id`

删除绑定。

### 8.5 Runtime Resolver

`GET /api/runtime/agent-capabilities`

用途：运行时服务获取某个 agent 对当前用户、场景的最终能力集合。

Query：

- `agent_key`
- `owner_user_id`
- `task_profile`
- `project_id`
- `project_source_type`
- `runtime_provider`
- `schedule_mode`
- `include_unavailable`

返回：

```json
{
  "agent_key": "task_runner_run_phase",
  "owner_user_id": "user-id",
  "mcps": [
    {
      "id": "builtin_task_manager",
      "name": "task_manager",
      "visibility": "system_private",
      "runtime": {
        "kind": "builtin",
        "builtin_kind": "TaskManager",
        "server_name": "task_manager"
      },
      "available": true,
      "required": true
    }
  ],
  "skills": [],
  "local_connector_requirements": [
    {
      "resource_id": "string",
      "device_id": "string",
      "workspace_id": "string",
      "required": true,
      "available": false,
      "reason": "device offline"
    }
  ]
}
```

解析逻辑：

1. 读取 `system_required` bindings。
2. 读取 `global_default` bindings。
3. 读取当前用户 `user_override` bindings。
4. 按 conditions 过滤。
5. 检查资源可见性：
   - private 只能 owner 使用。
   - public 所有用户可使用。
   - system_private 只能系统 agent runtime resolver 使用，不出现在普通 catalog。
6. 检查资源 enabled。
7. 对 local connector 资源计算 available。
8. 返回调用方可直接拼装 executor 的 runtime descriptors。

### 8.6 Local Connector 相关 API

`POST /api/local-connector/resources/sync`

由 Local Connector Client 或 Local Connector Service 上报本机 manifest。需要用户 token，并校验 device/workspace 归属。

`GET /api/local-connector/resources`

列出当前用户本机上报资源。

`POST /api/local-connector/resources/:resource_id/refresh`

触发重新扫描或重新校验。

## 9. 前端设计

前端目录：`plugin_management_service/frontend`

页面结构：

```text
src
├── App.tsx
├── main.tsx
├── api
│   ├── client.ts
│   ├── mcps.ts
│   ├── skills.ts
│   ├── agents.ts
│   └── runtime.ts
├── components
│   ├── AppShell.tsx
│   ├── ResourceVisibilityTag.tsx
│   ├── RuntimeKindTag.tsx
│   ├── LocalConnectorStatus.tsx
│   └── AgentBindingMatrix.tsx
├── pages
│   ├── McpCatalogPage.tsx
│   ├── McpEditorPage.tsx
│   ├── SkillCatalogPage.tsx
│   ├── SkillEditorPage.tsx
│   ├── SkillPackagesPage.tsx
│   ├── SystemAgentsPage.tsx
│   ├── AgentBindingsPage.tsx
│   └── RuntimePreviewPage.tsx
└── types.ts
```

### 9.1 MCP 管理页

核心功能：

- 表格列：名称、类型、可见性、owner、运行方式、状态、最近检查、更新时间。
- 筛选：MCP 类型、visibility、owner、enabled、本机资源状态。
- 操作：创建、编辑、启用/停用、健康检查、查看工具列表、删除。
- 创建表单：
  - HTTP MCP
  - 云端 stdio MCP
  - 本机 stdio MCP
  - 本机 HTTP MCP
  - 系统 builtin MCP 仅 admin 可见

本机 MCP 创建时，表单必须选择：

- device
- workspace
- command 或 local URL
- cwd / relative path
- 是否要求本机在线

### 9.2 Skills 管理页

核心功能：

- skill 列表
- package 列表
- inline skill 编辑
- 本机 skill manifest 绑定
- git package 同步
- public/private/system_private 标签

### 9.3 系统 Agent 页

只管理系统内部 agent，不展示用户联系人 agent。

展示：

- agent key
- 所属服务
- 描述
- enabled
- 默认 MCP 数
- 默认 skill 数
- 用户覆盖数量

### 9.4 Agent 能力矩阵

核心页面。建议 UI：

- 左侧 agent 列表。
- 顶部切换 binding scope：
  - 全局默认
  - 当前用户覆盖
  - 最终生效预览
- 中间表格：
  - MCP
  - Skills
  - Required
  - Conditions
  - Priority
  - Availability
- 右侧详情抽屉：绑定条件、运行 descriptor、检查结果。

### 9.5 运行时预览页

用于调试：

- 选择 user
- 选择 agent
- 选择 task profile / runtime provider / project source
- 查看 resolver 返回的最终 MCP/skills
- 标出不可用原因

这个页面对排查本机-only 资源很重要。

## 10. Local Connector 设计扩展

### 10.1 为什么必须扩展 Local Connector

用户后期添加的 MCP 可能是本机命令，例如：

- `npx some-mcp`
- `uvx some-mcp`
- 本机 Python 脚本
- 访问本机私有文件系统的工具
- 访问本机内网的 HTTP MCP

这些能力无法在云端容器执行。插件管理服务不能保存 command 后让 Chatos/Task Runner 在服务器启动，否则会产生错误和安全风险。

### 10.2 Local Connector Client 需要新增能力

建议新增本机插件 registry：

```text
local_connector_client/core/src/plugins
├── mod.rs
├── manifest.rs
├── registry.rs
├── mcp_runtime.rs
├── skill_runtime.rs
└── sync.rs
```

本机 manifest 示例：

```json
{
  "version": 1,
  "mcps": [
    {
      "local_id": "my-local-mcp",
      "name": "My Local MCP",
      "transport": "stdio",
      "command": "npx",
      "args": ["my-mcp-server"],
      "cwd": ".",
      "env": {}
    }
  ],
  "skills": [
    {
      "local_id": "my-local-skill",
      "name": "My Local Skill",
      "source_path": "./skills/my-skill/SKILL.md"
    }
  ]
}
```

Client 负责：

- 扫描 manifest。
- 上报 metadata 到 plugin management service。
- 接到 relay 请求时启动本机 MCP 或读取本机 skill。
- 返回 list_tools / call_tool / read_skill_content。
- 维护进程生命周期和超时。

### 10.3 Local Connector Service 需要新增 relay 类型

现有 local connector service 已有 MCP relay：

- `/api/local-connectors/relay/:device_id/mcp?workspace_id=...`

可以扩展：

- `/api/local-connectors/relay/:device_id/plugin-mcp/:resource_id`
- `/api/local-connectors/relay/:device_id/plugin-skill/:resource_id`
- 或复用 `/mcp` body 中的 target resource 字段。

建议第一版尽量复用现有 `/mcp` relay，在 headers 或 JSON-RPC params 中增加：

- `x-plugin-resource-id`
- `x-plugin-management-runtime-kind`
- `x-local-connector-manifest-id`

这样对已有 relay 改动最小。

## 11. 系统内置 MCP seed

系统内置 MCP 应从 `crates/chatos_mcp_runtime/src/builtin_catalog.rs` seed：

- `CodeMaintainerRead`
- `CodeMaintainerWrite`
- `TerminalController`
- `TaskManager`
- `ProjectManagement`
- `Notepad`
- `AgentBuilder`
- `AskUser`
- `RemoteConnectionController`
- `WebTools`
- `BrowserTools`
- `MemorySkillReader`
- `MemoryCommandReader`
- `MemoryPluginReader`

除 builtin catalog 外，还要 seed 当前代码真实存在的 system-routed MCP：

- `task_runner_service`：Task Runner 服务提供给 Chatos 的 MCP 入口。
- `project_environment`：项目运行环境读写工具。
- `sandbox_images`：沙箱镜像 MCP。
- `local_connector_approval`：本机命令审批决策工具。

seed 规则：

- `owner_kind = "system"`
- `owner_user_id = admin user id`
- `visibility = "system_private"`
- `source_kind = "system_seed"`
- `runtime.kind = "builtin"`
- `runtime.builtin_kind = BuiltinMcpKind.kind_name()`
- `runtime.server_name = BuiltinMcpKind.server_name()`
- `runtime.command = BuiltinMcpKind.command()`

注意：部分 builtin 没有 `config_id` 或 command，需要生成稳定 id，例如：

- `system_builtin_memory_skill_reader`
- `system_builtin_memory_command_reader`
- `system_builtin_memory_plugin_reader`

## 12. 默认 Agent Binding Seed

默认关系必须直接反映当前代码中的真实工具执行器，不要求运行时动态查询插件管理服务。

### 12.1 `chatos_conversation_agent`

可选 global default：

- `task_runner_service`

普通对话必须装配 Task Runner MCP，因此该 MCP 是必需能力。普通模式和规划模式的区别是系统提示、任务 profile、规划约束和调用行为，不是普通模式可以缺少该 MCP。用户联系人只提供动态角色、技能和联系人级策略，不在系统 Agent 页面逐条出现。

### 12.2 `chatos_planning_agent`

system required：

- `task_runner_service`

规划模式会要求具体项目，并通过请求头将 Task Runner 切换到 `chatos_plan` task profile；缺少该 MCP 时规划不能继续。

### 12.3 `task_runner_run_phase`

system required：

- `TaskManager`
- `AskUser`

可选 global default：

- `CodeMaintainerRead`
- `CodeMaintainerWrite`
- `TerminalController`
- `ProjectManagement`
- `Notepad`
- `RemoteConnectionController`
- `WebTools`
- `BrowserTools`

`AgentBuilder` 在 Task Runner catalog 中标记为未实现，memory reader 系列也没有进入 Task Runner 可配置 provider catalog，因此不属于该智能体当前可用能力。

### 12.4 `chatos_plan` 任务 profile

`chatos_plan` 是 Task Runner 的任务 profile，不是独立系统智能体。它仍由
`task_runner_run_phase` 执行，并由 Task Runner 根据 profile 注入规划任务所需能力。

### 12.5 `project_management_agent`

system required：

- `CodeMaintainerRead`
- `project_environment`
- `sandbox_images`

`CodeMaintainerRead` 和 `sandbox_images` 会在运行时根据项目上下文选择本地直连、Local Connector、Harness 或云端 Sandbox Manager 子实现；这不是管理员需要配置的另一层 Agent 关系。该智能体不使用普通 builtin `ProjectManagement` MCP。

### 12.6 `local_connector_command_approval_agent`

system required：

- `CodeMaintainerRead`，但执行器只开放读取、目录列表和文本搜索工具。
- `local_connector_approval`，对应必须调用一次的 `approval_decision` 内部工具。

## 13. 与现有服务集成

### 13.1 Chatos 集成

当前 Chatos MCP 加载在 `chatos/backend/src/services/mcp_loader.rs`：

- 从 `mcp_configs` 读取用户配置。
- 自动追加 builtin MCP。
- 生成 HTTP、stdio、builtin server 列表。

改造目标：

1. 新增 `plugin_management_client`。
2. `load_mcp_configs_for_user` 优先调用 plugin management resolver。
3. 将 resolver 返回的 runtime descriptors 转换为现有：
   - `McpHttpServer`
   - `McpStdioServer`
   - `McpBuiltinServer`
4. 初期保留旧逻辑 fallback。
5. 后续将 Chatos MCP CRUD 改为代理到 plugin management service。

skills 集成：

1. Chatos `chatos_skills` 从 plugin management service 读取当前用户可见 skills。
2. inline/cloud skills 直接返回内容。
3. local connector skills 返回 local ref，由运行时按需读取。
4. 兼容现有 `memory_skills` 到迁移完成。

### 13.2 Task Runner 集成

当前 Task Runner 在 `mcp_resolution.rs` 中硬编码 caller requirements。

改造目标：

1. 保留现有硬性安全依赖作为 fallback。
2. 新增 plugin management resolver 调用。
3. 对 `AgentMcpCaller` 增加到 `agent_key` 的映射。
4. 解析结果合并：
   - system required 永远生效。
   - 用户或任务选择的 external MCP 需要经过 plugin management 可见性校验。
   - local connector hosted builtin 继续通过 header 限制。
5. `external_mcp_config_ids` 后续迁移为 plugin resource ids。

### 13.3 Project Management 集成

当前 Project Management environment agent 直接构造：

- project environment builtin provider
- harness file MCP
- local connector file MCP
- sandbox image MCP

改造目标：

1. `project_management_agent` 启动前调用 plugin management resolver。
2. resolver 决定该 agent 是否允许使用相应 MCP 类别。
3. 动态 project-specific MCP 仍由 project service 根据项目上下文拼装。
4. `ensure_agent_required_tools_available` 保留，作为执行前校验。

### 13.4 Local Connector 集成

Local Connector Service：

1. 提供 device/workspace 状态查询给 plugin management service。
2. 支持本机 plugin resource 的 relay。
3. 保持 owner 校验，确保不能跨用户调用本机资源。

Local Connector Client：

1. 新增本机插件 manifest。
2. 新增本机 MCP runtime 管理。
3. 新增本机 skill 内容读取。
4. 上报 manifest 到 plugin management service。
5. 响应 plugin resource relay。

## 14. 迁移方案

### 14.1 阶段一：服务骨架和系统 seed

目标：

- 新建 plugin management 后端。
- 接入 Mongo。
- 接入 user service auth。
- 创建 indexes。
- seed system builtin MCP。
- seed system agent registry。
- seed default agent bindings。
- 提供基础 MCP/skills/agent API。

验收：

- `GET /api/health` 可用。
- `GET /api/mcps?include_system=true` admin 可看到 system builtin MCP。
- 普通用户看不到 system_private。
- 普通用户不能创建 public。
- `GET /api/system-agents` 可看到系统 agent。

### 14.2 阶段二：前端管理台

目标：

- 新建 React + AntD frontend。
- 登录复用用户服务。
- MCP 管理页。
- Skills 管理页。
- System Agents 页。
- Agent Binding Matrix 页。
- Runtime Preview 页。

验收：

- admin 可以创建 public MCP/skills。
- 普通用户只能创建 private。
- admin 可以配置 agent global bindings。
- 用户可以查看最终生效能力。

### 14.3 阶段三：迁移现有 Chatos MCP/skills

目标：

- 导入现有 `mcp_configs` 到 `plugin_mcps`。
- 导入现有 `memory_skills` 到 `plugin_skills`。
- 导入现有 `memory_skill_plugins` 到 `plugin_skill_packages`。
- Chatos loader 优先读取 plugin management。
- 保留 fallback。

验收：

- 旧用户 MCP 配置在新服务中可见。
- Chatos 会话可正常加载用户 MCP。
- builtin MCP 不再由 Chatos 独立追加，而由 resolver 输出，fallback 阶段除外。

### 14.4 阶段四：Task Runner / Project Management 接入

目标：

- Task Runner 使用 plugin management resolver 获取 agent capabilities。
- Project Management environment agent 使用 resolver 控制可用 MCP。
- 保留硬性 required tools 校验。

验收：

- `task_runner_run_phase` 修改绑定后，任务运行能力解析结果变化。
- `project_management_agent` 缺少必需 MCP 时能明确报错。
- system_private MCP 不暴露给普通用户 catalog，但 runtime resolver 可用于系统 agent。

### 14.5 阶段五：Local Connector 本机-only MCP/skills

目标：

- Local Connector Client 支持本机插件 manifest。
- Local Connector Client 上报本机 MCP/skills。
- Plugin Management 保存 local connector refs。
- Runtime resolver 返回本机资源状态。
- 调用方可通过 local connector relay 使用本机 MCP/skills。

验收：

- 用户添加本机 stdio MCP 后，云端 catalog 能看到 metadata。
- 本机 client 离线时，资源显示 unavailable。
- 本机 client 在线时，runtime resolver 返回 available。
- Chatos 或 Task Runner 能通过 relay 调用该 MCP。
- 其他用户完全看不到该本机 MCP。

### 14.6 阶段六：旧入口收敛

目标：

- Chatos 旧 MCP CRUD 改为代理到 plugin management service。
- Chatos 旧 skills 管理改为代理或下线。
- Task Runner `external_mcp_config_ids` 迁移为 plugin resource ids。
- 文档、环境变量、compose、CI 全部补齐。

验收：

- 新服务成为 MCP/skills/agent capability 的唯一控制面。
- 旧表只读或停止写入。
- 新增 MCP/skills 不再散落到各业务服务数据库。

## 15. 安全设计

### 15.1 权限校验

所有读写必须经过统一 authorization service：

- `can_read_resource(user, resource)`
- `can_create_resource(user, visibility)`
- `can_update_resource(user, resource)`
- `can_delete_resource(user, resource)`
- `can_bind_resource_to_agent(user, binding_scope, resource)`
- `can_resolve_agent_capabilities(user, agent_key, owner_user_id)`

### 15.2 本机资源安全

本机 MCP/skills 必须满足：

- resource owner 与 local connector owner 一致。
- device_id/workspace_id 归属当前 owner。
- relay 调用时 local connector service 再次校验。
- 不在 plugin management service 中保存明文敏感 secret，必要时只保存 secret ref。
- env 中敏感字段需要 mask。

### 15.3 public 资源安全

public MCP 可能影响所有用户，必须限制：

- 仅 `super_admin` 创建。
- 必须记录 audit log。
- 对 HTTP MCP 的 headers/env 做敏感字段 mask。
- public stdio_cloud 只允许管理员配置。
- public 本机-only 不建议支持，因为本机资源天然属于某个用户设备，不适合全局共享。

### 15.4 system_private 资源安全

system_private 不出现在普通 catalog 查询中。只能：

- admin 管理界面查看。
- runtime resolver 给系统内部 agent 解析。

## 16. 部署改动

需要修改：

- 根 `Cargo.toml` workspace members 增加 `plugin_management_service/backend`
- `docker/compose.yml` 增加 backend 和 frontend
- `docker/compose.build.yml` 增加镜像构建
- `docker/.env.example` 增加环境变量
- `scripts/local-dev-stack.sh` 或相关 local dev 脚本增加服务启动
- `.github/workflows` 中 Docker image build matrix 增加新服务
- nginx 或前端入口根据现有部署方式增加路由

建议端口：

- backend：`39260`
- frontend：`39261`

服务发现：

- service name：`plugin-management-service`
- health path：`/api/health`

## 17. 测试计划

### 17.1 后端单元测试

- visibility 过滤。
- 普通用户不能创建 public。
- 普通用户不能读取他人 private。
- admin 可以读取指定 owner 资源。
- system_private 不出现在普通 catalog。
- agent binding 合并顺序。
- runtime resolver conditions 过滤。
- local connector unavailable 状态计算。

### 17.2 API 集成测试

- 登录后创建 private MCP。
- admin 创建 public MCP。
- 普通用户查询能看到 public + own private。
- 普通用户查不到其他用户 private。
- admin 创建 system_private MCP。
- runtime resolver 能返回 system_private 给系统 agent。
- agent bindings CRUD。

### 17.3 迁移测试

- 导入旧 Chatos MCP。
- 导入旧 memory skills。
- 重复导入幂等。
- 已删除或 disabled 资源状态正确。

### 17.4 Local Connector 测试

- 本机 manifest 上报。
- device offline 状态。
- workspace 不归属当前用户时拒绝。
- relay list_tools。
- relay call_tool。
- 本机 skill 内容读取。

### 17.5 前端测试

- 普通用户 UI 不显示 public/system 创建选项。
- admin UI 显示 visibility 选择。
- 能力矩阵修改后刷新生效。
- runtime preview 显示 unavailable 原因。

## 18. 兼容性和风险

### 18.1 风险：执行路径复杂

同一个 MCP 可能是 builtin、HTTP、云端 stdio、本机 stdio、本机 HTTP。解决方式：

- runtime descriptor 必须强类型化。
- 每种 runtime kind 有单独转换函数。
- 调用方不能根据 command 字符串猜测运行位置。

### 18.2 风险：system_private 和用户可见性混淆

解决方式：

- catalog API 默认排除 system_private。
- runtime resolver 单独路径返回 system_private。
- 前端用明显标签区分。

### 18.3 风险：本机资源离线导致 agent 失败

解决方式：

- resolver 返回 availability。
- required 本机资源不可用时，调用方提前失败并给出明确提示。
- optional 本机资源不可用时可跳过。

### 18.4 风险：旧服务同时写旧表和新表

解决方式：

- 迁移阶段先只读 plugin management，旧写入保持。
- 第二阶段双写。
- 第三阶段切写入入口。
- 最后旧表只读或移除。

### 18.5 风险：admin public MCP 误配置

解决方式：

- public 创建前强制 check。
- audit log。
- UI 标注影响范围。
- 支持 disable 而不是 hard delete。

## 19. 推荐实施顺序

1. 创建 `plugin_management_service/backend` 骨架。
2. 实现 auth、Mongo store、indexes。
3. 实现 MCP/skills/system agents/bindings 基础 API。
4. 实现 system builtin MCP seed 和 agent seed。
5. 实现 runtime resolver。
6. 创建 `plugin_management_service/frontend`。
7. 完成 MCP/skills/agent binding 管理台。
8. Chatos 接入 resolver，保留 fallback。
9. 迁移 Chatos 现有 MCP/skills 数据。
10. Task Runner 接入 resolver。
11. Project Management 接入 resolver。
12. Local Connector Client 增加本机 plugin manifest 和 relay runtime。
13. 旧入口收敛和文档补齐。

## 20. 第一版最小可交付范围

为了降低风险，第一版建议做到：

- 插件管理后端可运行。
- Mongo schema 和 indexes 完成。
- user service auth 完成。
- MCP catalog 完成。
- skills catalog 完成。
- system agent registry 完成。
- agent binding 完成。
- runtime resolver 完成。
- seed system builtin MCP 和默认 agent bindings。
- 前端可以管理 MCP、skills、agent bindings。
- Chatos MCP loader 可以读取 plugin management resolver。

第一版暂不强行完成：

- 任意用户本机 MCP 的完整执行 runtime。
- 所有旧 CRUD 完全迁移。
- Task Runner 全量替换旧 MCP resolution。
- Project Management 动态 MCP 拼装完全外置。

这些放第二版完成更稳。

## 21. 验收清单

基础能力：

- [ ] 服务能通过 Docker Compose 启动。
- [ ] 服务能注册到 service runtime。
- [ ] `/api/health` 正常。
- [ ] Mongo indexes 自动创建。
- [ ] 用户 token 校验正常。

权限：

- [ ] 普通用户只能创建 private MCP/skills。
- [ ] 普通用户不能创建 public。
- [ ] 普通用户不能创建 system_private。
- [ ] 普通用户看不到其他用户 private MCP/skills。
- [ ] 普通用户看不到 system_private catalog。
- [ ] admin 可以创建 public 和 system_private。

系统能力：

- [ ] system builtin MCP seed 成功。
- [ ] system agents seed 成功。
- [ ] 默认 agent bindings seed 成功。
- [ ] runtime resolver 能返回 system_private MCP 给系统 agent。

前端：

- [ ] MCP 管理页可用。
- [ ] Skills 管理页可用。
- [ ] System Agents 页可用。
- [ ] Agent Binding Matrix 可用。
- [ ] Runtime Preview 可用。

集成：

- [ ] Chatos 能通过 plugin management 加载 MCP。
- [ ] 旧 Chatos MCP 配置可迁移。
- [ ] 旧 Chatos skills 可迁移。
- [ ] Task Runner 可读取 agent capabilities。
- [ ] Project Management 可读取 agent capabilities。

Local Connector：

- [ ] 本机资源 metadata 可上报。
- [ ] 本机资源 online/offline 状态可显示。
- [ ] 本机 MCP 可通过 relay list_tools。
- [ ] 本机 MCP 可通过 relay call_tool。
- [ ] 本机 skills 可通过 relay 读取内容。

## 22. 后续演进

后续可以继续扩展：

- MCP marketplace。
- skills marketplace。
- 资源版本管理。
- agent 能力变更审批。
- public 资源安全扫描。
- per-project agent capability override。
- per-task capability override。
- 资源使用统计和成本统计。
- 工具调用审计。
- MCP tool schema diff。
- 本机 connector 多设备优先级。

## 23. 结论

插件管理服务应该成为 MCP、skills、系统内部 agent 能力配置的统一控制面。它不替代现有执行 runtime，而是把资源归属、可见性、授权绑定、本机 connector 引用、运行时解析统一起来。

最关键的设计点是：用户本机-only MCP/skills 不能被简化为云端 stdio 配置，必须通过 Local Connector Client 暴露。只要这个边界从一开始建模清楚，后续无论是 Chatos、Task Runner，还是 Project Management，都可以逐步切到统一的 resolver，而不会破坏现有运行链路。
