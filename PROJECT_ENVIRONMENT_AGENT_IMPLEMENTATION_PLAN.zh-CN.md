# 项目运行环境初始化 Agent 实施方案

## 背景

目标是在 Project Management Service 中新增一个项目级 AI agent，用于读取项目文件、判断项目运行所需环境、准备 sandbox 镜像，并把结果沉淀到项目详情中。

这次能力需要按项目来源和用户本地沙箱设置自动选择 MCP：

- 项目详情 MCP：Project Management Service 自身提供，项目 ID 由程序透传，不暴露给模型输入。
- 文件读取 MCP：
  - 云端项目使用 Harness MCP。
  - 本地项目使用 Local Connector Client MCP。
- 沙箱镜像 MCP：
  - 用户开启本地沙箱时优先使用 Local Connector Client 提供的本地沙箱镜像 MCP。
  - 否则使用云端 Sandbox Manager 的沙箱镜像 MCP。

补充约束：

- 如果项目为空，或分析后不具备运行条件，agent 不创建镜像，直接把原因写入项目详情，前端展示给用户。
- 本地项目创建时需要增加“是否使用沙箱”选项；用户选择“否”时，不触发运行环境初始化 agent，也不准备镜像。
- 镜像创建工具必须同步返回最终成功或失败，不让模型轮询创建进度。工具调用超时时间要足够长。

## 当前代码基础

### Project Management Service

- 项目模型在 `project_management_service/backend/src/models/projects.rs`。
- 项目表已有：
  - `source_type`: `local` / `local_connector` / `cloud`
  - cloud import / Harness repo 字段
  - `project_profiles` 副表，用于项目背景和介绍
- HTTP API 在 `project_management_service/backend/src/api/projects.rs` 和 `project_management_service/backend/src/api/router.rs`。
- MCP server 在：
  - `project_management_service/backend/src/mcp_server.rs`
  - `project_management_service/backend/src/mcp_tools.rs`
  - `project_management_service/backend/src/api/router/mcp.rs`
- 已有 Harness 项目文件 MCP 入口：
  - `project_management_service/backend/src/api/harness_mcp.rs`
  - `/api/chatos-sync/projects/:project_id/harness/mcp`
  - 工具包含 `read_file_raw`、`read_file_range`、`list_dir`、`search_text` 以及写入工具。

### Local Connector

- Local Connector Service 已有项目 binding 和 sandbox pairing：
  - `local_connector_service/backend/src/models.rs`
  - `local_connector_service/backend/src/api.rs`
- 本地 MCP 透传入口：
  - `/api/local-connectors/relay/:device_id/mcp?workspace_id=...&cwd=...`
- 本地 sandbox facade 入口：
  - `/api/local-connectors/sandbox-facade/:pairing_id/...`
- Local Connector Client 已有本地沙箱镜像 HTTP 能力：
  - `local_connector_client/core/src/api/handlers/sandbox.rs`
  - `GET /api/local/sandbox/images`
  - `GET /api/local/sandbox/images/jobs`
  - `POST /api/local/sandbox/images/initialize`
- 但这些镜像能力目前没有包装为 MCP，也没有进入 `sandbox_request` relay path。

### Sandbox Manager

- 云端 Sandbox Manager 已有镜像 HTTP API：
  - `sandbox_manager_service/backend/src/api/router.rs`
  - `GET /api/sandbox-images`
  - `GET /api/sandbox-images/jobs`
  - `POST /api/sandbox-images/initialize`
- 镜像初始化当前是 job 模式：
  - `sandbox_manager_service/backend/src/service/images.rs`
  - `sandbox_manager_service/backend/src/service/manager.rs`
- 现有 Sandbox MCP server 主要暴露 sandbox 内文件和终端工具：
  - `sandbox_manager_service/sandbox_mcp_server/src/tools/provider.rs`

### AI/MCP Runtime

- 共享 AI runtime 在 `crates/chatos_ai_runtime`。
- 共享 MCP runtime 在 `crates/chatos_mcp_runtime`。
- Project Management Service 当前尚未直接依赖 `chatos_ai_runtime` / `chatos_mcp_runtime`。
- Task Runner 已经使用这套 runtime 执行模型任务：
  - `task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs`
  - `task_runner_service/backend/src/services/run_model_phase/callbacks/execution.rs`
  - `task_runner_service/backend/src/models/model_config.rs`
- 当前 `chatos_mcp_runtime::jsonrpc_http_call` 内部 HTTP 超时为 15 秒，不满足镜像创建长耗时需求，需要扩展。

## AI Runtime 复用结论

Project Management Service 需要直接引用已经抽出的 AI 交互模块，在服务内部实现一个专用的项目环境初始化 agent。这个 agent 当前业务边界固定，只负责分析项目运行环境、准备沙箱镜像并写回项目详情，不走 Task Runner 的通用任务执行链路。

### 可以直接引用的 crate

Project Management Service 最小新增依赖为：

```toml
chatos_ai_runtime = { path = "../../crates/chatos_ai_runtime" }
chatos_mcp_runtime = { path = "../../crates/chatos_mcp_runtime" }
chatos_sandbox_image_mcp = { path = "../../crates/chatos_sandbox_image_mcp" }
memory_engine_sdk = { path = "../../crates/memory_engine_sdk" }
```

可复用能力：

- `AiRuntime`
- `ModelRuntimeConfig`
- `McpExecutorBuilder`
- `MemoryContextComposer`
- `MemoryEngineRecordWriter`
- `McpExecutorBuilder`

典型执行形态直接放在 Project Management Service 内：

1. Project Management Service 根据项目状态组装文件 MCP、沙箱镜像 MCP、项目详情 MCP。
2. 构造 `McpExecutorBuilder`。
3. 构造 `AiRuntime` / `ModelRequest`，设置 agent 最大迭代次数、prompt、metadata 和 tool executor。
4. 使用 Project Management Service 自己的状态机同步执行，并把结果写回项目运行环境副表。
5. 通过 `MemoryContextComposer` 读取同一项目的历史环境初始化记忆，通过 `MemoryEngineRecordWriter` 写入本次分析输入和输出。

明确不使用 Task Runner 的通用任务执行链路，也不把沙箱镜像 MCP 注册给 Task Runner。

### 模型配置前置条件

Project Management Service 不能只依赖当前 `task_runner_api_client.rs` 里的 execution options，因为那是 Task Runner 创建任务场景，只返回：

- 可用模型配置 ID
- 可用工具 ID
- 可用 skill ID

但运行 agent 还需要完整的 `ModelRuntimeConfig`：

- provider
- base_url
- api_key
- model
- temperature / max_output_tokens / thinking_level
- responses 支持情况

所以 Project Management Service 运行专用 agent 时，需要新增一条内部模型配置解析链路，把用户级 `project_management_agent_model_config_id` 解析为完整的 `ModelRuntimeConfig`。否则只能拿到模型 ID，无法真正发起模型请求。

### 推荐方案

采用“Project Management Service 内置专用 agent”的方案：

- Project Management Service 直接依赖 `chatos_ai_runtime` 和 `chatos_mcp_runtime`。
- Project Management Service 自己负责 agent run 状态机、prompt、MCP 路由、结果校验和落库。
- Task Runner 不参与该业务链路，只作为 `chatos_ai_runtime` 的既有使用范例参考。
- 模型密钥仍由 User Service 统一管理；Project Management Service 通过内部受信接口按当前用户和模型配置 ID 获取运行时所需配置。

需要新增或复用一个内部接口，例如：

- `GET /api/internal/users/:user_id/model-configs/:model_config_id/runtime`
- 返回 Project Management Service 构造 `ModelRuntimeConfig` 所需字段。
- 接口需要校验 service token、用户归属、模型启用状态和具体 model name。
- 返回值只在服务间传输，不直接暴露给浏览器。

## 项目管理 Agent 默认模型配置

默认模型配置按 Memory Engine 的设置方式实现：用户级保存一个默认模型配置 ID 和一个可选 thinking level，而不是沿用 Task 模型设置里“逐模型用途说明”的方式。

### 配置入口

在 Chatos 右上角用户菜单中，在 `Task 模型设置` 附近新增入口：

- 中文建议：`项目管理 Agent 模型`
- 英文建议：`Project agent model`

打开后使用和 `MemoryModelSettingsPanel` 一致的表单形态：

- 一个模型下拉框，只展示已启用且有具体 `model_name` 的 AI 模型配置。
- 一个 thinking level 下拉框，根据所选模型 provider 动态展示可选项。
- 支持“不选择”，表示项目管理环境初始化 agent 没有用户级默认模型；后端触发时需要走兜底策略或提示用户配置。

前端实现建议直接参考：

- `chatos/frontend/src/components/MemoryModelSettingsPanel.tsx`

新增组件建议：

- `chatos/frontend/src/components/ProjectManagementAgentModelSettingsPanel.tsx`

### 配置存储

复用现有 `/api/ai-model-settings` 链路，在 User Service 的 `user_model_settings` 中新增字段：

- `project_management_agent_model_config_id`
- `project_management_agent_thinking_level`

涉及结构：

- `user_service/backend/src/models.rs`
  - `UserModelSettingsRecord`
  - `UpdateUserModelSettingsRequest`
- `user_service/backend/src/api/models/settings_handlers.rs`
- `user_service/backend/src/api/models/model_values.rs`
- `chatos/backend/src/api/configs.rs`
  - `AiModelSettingsRequest`
- `chatos/backend/src/services/user_service_api_client/types.rs`
  - `UserServiceModelSettingsRecord`
  - `UpdateUserServiceModelSettingsRequest`
- `chatos/backend/src/api/configs/ai_model/settings_handlers.rs`
- `chatos/backend/src/api/configs/ai_model/model.rs`
- `chatos/frontend/src/lib/api/client/types/config.ts`

校验规则和 `memory_summary_model_config_id` 保持一致：

- 模型配置必须存在。
- 模型配置必须属于当前用户。
- 模型配置必须有具体 `model` / `model_name`。
- thinking level 按 provider 归一化，非法值保存为空或返回 bad request，建议沿用现有 `normalize_thinking_level_input`。

### Agent 使用方式

项目管理环境初始化 agent 触发时按以下优先级选择模型：

1. 用户级 `project_management_agent_model_config_id`。
2. 如果为空，使用系统级 fallback 配置，例如 `PROJECT_SERVICE_ENV_AGENT_FALLBACK_MODEL_CONFIG_ID`。
3. 如果仍为空，写入运行环境状态为 `failed` 或 `pending_configuration`，项目详情提示用户先配置“项目管理 Agent 模型”。

Project Management Service 内置专用 agent 方案下：

- Project Management Service 触发环境初始化 agent run 时，读取用户级默认模型配置 ID。
- 通过 User Service 内部受信接口把模型配置 ID 解析为完整运行时配置。
- 将 `project_management_agent_thinking_level` 叠加到运行时配置中。
- 使用 `chatos_ai_runtime::ModelRuntimeConfig` / `AiRuntime` 在 Project Management Service 内同步执行。
- 直接接入 Memory Engine，默认 source id 为 `project_management_agent`，项目环境初始化线程为 `project_environment:{project_id}`。
- 模型密钥不落 Project Management Service 数据库，只在本次服务端运行时内存中使用。

### 可复用的运行环境分析逻辑

`chatos/backend/src/services/project_run` 已经有项目运行目标和环境发现逻辑：

- `analyzer.rs`
- `environment_discovery.rs`
- `environment.rs`
- `environment_discovery/config_files.rs`

建议把其中“识别运行目标、读取 manifest、判断工具链和配置文件”的纯逻辑抽到共享 crate，供 Project Management agent 复用。

## 总体架构

新增一条项目环境初始化链路：

1. 用户创建或打开项目。
2. Project Management Service 判断是否需要运行环境初始化。
3. 根据项目来源和本地沙箱配置，构造 MCP server 集合。
4. 启动项目环境初始化 agent。
5. agent 读取文件，判断项目运行条件和依赖环境。
6. 若项目为空或不可运行，写入项目详情中的分析结论。
7. 若可运行，调用沙箱镜像 MCP 搜索/确保镜像。
8. 为数据库、Redis、Nacos 等服务生成环境变量和默认凭据。
9. 将镜像、环境变量、状态、失败原因等写入项目运行环境副表。
10. 前端项目详情展示运行环境初始化结果。

## 路由规则

| 项目类型 | 用户本地沙箱 | 文件 MCP | 沙箱镜像 MCP | 是否初始化 |
| --- | --- | --- | --- | --- |
| 本地项目，用户创建时选择使用沙箱 | 已开启 | Local Connector 文件 MCP | Local Connector 本地沙箱镜像 MCP | 是 |
| 本地项目，用户创建时选择使用沙箱 | 未开启 | Local Connector 文件 MCP | 云端 Sandbox Manager 镜像 MCP | 是 |
| 本地项目，用户创建时选择不使用沙箱 | 任意 | 不需要 | 不需要 | 否 |
| 云端项目 | 已开启 | Harness 文件 MCP | Local Connector 本地沙箱镜像 MCP | 是 |
| 云端项目 | 未开启 | Harness 文件 MCP | 云端 Sandbox Manager 镜像 MCP | 是 |

说明：

- 本地项目的“是否使用沙箱”是项目级显式配置。选择“否”时，即使用户本地沙箱已开启，也不执行初始化。
- 云端项目没有这个跳过条件，默认需要初始化。
- 当本地沙箱开启但缺少可用 sandbox pairing 时，降级到云端 Sandbox Manager，并记录降级原因。

## 数据模型设计

不把运行环境信息直接塞进 `projects` 表，新增副表。

### 1. 项目运行环境配置表

SQLite 表建议名：`project_runtime_environments`

Mongo collection 建议名：`project_runtime_environments`

字段：

- `project_id`: 主键，关联项目。
- `status`: `disabled` / `pending` / `analyzing` / `ready` / `not_runnable` / `failed`
- `sandbox_enabled`: 用户是否允许该项目使用沙箱。
- `sandbox_provider`: `local_connector` / `cloud_sandbox_manager` / `none`
- `file_provider`: `local_connector` / `harness` / `none`
- `analysis_summary`: 给用户看的摘要。
- `not_runnable_reason`: 项目为空或不可运行时的原因。
- `detected_stack_json`: 检测到的技术栈、语言、启动目标。
- `required_services_json`: `database`、`redis`、`nacos` 等依赖服务清单。
- `env_vars_json`: 生成的环境变量，按服务分组。
- `last_agent_run_id`: 最近一次 agent 运行 ID。
- `last_error`: 初始化失败原因。
- `created_at`
- `updated_at`

### 2. 项目运行环境镜像表

SQLite 表建议名：`project_runtime_environment_images`

Mongo collection 建议名：`project_runtime_environment_images`

字段：

- `id`
- `project_id`
- `environment_key`: 例如 `app`、`mysql`、`redis`、`nacos`
- `environment_type`: `runtime` / `database` / `cache` / `registry` / `message_queue` / `custom`
- `display_name`
- `image_id`
- `image_ref`
- `image_provider`: `local_connector` / `cloud_sandbox_manager`
- `features_json`: 例如 `["java@21", "node@24"]`
- `ports_json`
- `env_vars_json`: 该镜像启动需要的环境变量名和值引用。
- `status`: `ready` / `failed`
- `error`
- `created_at`
- `updated_at`

### 3. 项目环境变量表

如果后续需要支持按变量编辑、脱敏展示和审计，建议独立表：

`project_runtime_environment_variables`

字段：

- `id`
- `project_id`
- `environment_key`
- `key`
- `value_ciphertext` 或 `value`
- `masked`: 是否前端脱敏。
- `source`: `generated` / `agent_detected` / `user_override`
- `created_at`
- `updated_at`

第一阶段可以先落在 `project_runtime_environments.env_vars_json`，但接口层要按独立变量对象返回，避免后续迁移影响前端。

## 项目创建与触发条件

### 本地项目创建新增字段

新增字段：

- `sandbox_enabled?: boolean`

涉及位置：

- Chatos 主前端：
  - `chatos/frontend/src/components/sessionList/CreateResourceModals.tsx`
  - `chatos/frontend/src/components/sessionList/useSessionListActions.ts`
  - `chatos/frontend/src/lib/api/client/types/localConnector.ts`
- Chatos backend local connector 项目创建：
  - `chatos/backend/src/api/local_connectors.rs`
- Project Management Service 项目创建模型：
  - `project_management_service/backend/src/models/projects.rs`

本地项目创建行为：

- 默认值建议为 `true`，但 UI 必须让用户明确看到选项。
- 选择 `false` 时：
  - 创建项目后写入 `project_runtime_environments.status = disabled`
  - `sandbox_enabled = false`
  - 不启动 agent
  - 项目详情展示“该项目已关闭沙箱初始化”

### 云端项目创建

云端项目在 Harness import 成功后触发初始化：

- import 状态必须为 `ready`
- Harness repo 信息必须存在
- 若 import 失败，不触发 agent，项目详情展示 import error

### 空项目或不可运行项目

agent 判断以下情况时不创建镜像：

- 根目录为空。
- 没有可识别的 manifest 或入口文件。
- 有 manifest，但没有可运行 target，例如 package.json 没有 scripts。
- 关键配置损坏，例如 manifest 无法解析。
- 项目需要用户补充信息，例如缺失私有依赖配置。

写入：

- `status = not_runnable`
- `analysis_summary`
- `not_runnable_reason`
- `detected_stack_json`
- `last_error = null`

前端项目详情展示为“无法自动初始化”，并列出原因和建议动作。

## MCP 设计

### Project Management 自身 MCP

新增或扩展工具：

1. `get_current_project_detail`
   - 无 `project_id` 入参，项目 ID 从 server context/header 透传。
   - 返回项目、profile、runtime environment、images、env vars。

2. `update_current_project_runtime_environment`
   - 无 `project_id` 入参。
   - 用于 agent 写入分析结果、不可运行原因、镜像清单、环境变量。

建议不要复用 `initialize_project` 写运行环境，因为它现在偏项目基础信息和 profile。

### Harness 文件 MCP

复用现有 `harness_code`：

- `read_file_raw`
- `read_file_range`
- `list_dir`
- `search_text`

项目 ID 继续由 `/api/chatos-sync/projects/:project_id/harness/mcp` 和 headers 透传。

### Local Connector 文件 MCP

复用现有 relay MCP：

- `/api/local-connectors/relay/:device_id/mcp?workspace_id=...&cwd=...`
- 使用 `x-local-connector-enabled-builtin-kinds` 暴露 code read。

项目管理服务需要根据 local project binding 找到：

- `device_id`
- `workspace_id`
- `relative_path`

### 沙箱镜像 MCP

新增共享 crate，建议名：

- `crates/chatos_sandbox_image_mcp`

提供统一工具 schema 和客户端/服务端适配：

1. `get_image_catalog`
   - 返回支持的 runtime features 和当前已知镜像。

2. `search_images`
   - 入参：`image_id?`、`features`、`status?`、`include_unavailable?`
   - 返回匹配镜像、状态、provider。

3. `create_image`
   - 入参：`features`、`service_type`、`custom_build_script?`
   - 语义：有现成镜像就返回；没有就同步创建并等待完成。
   - 成功返回最终 `image_id` / `image_ref`。
   - 失败返回明确失败原因和构建日志摘要。

关键要求：

- `create_image` 不向模型返回可继续轮询的中间 `job_id`；工具内部等待最终 `succeeded` / `failed`。
- 工具内部可以调用现有 job API，但必须在服务端循环等待到 `succeeded` 或 `failed`。
- Project Management agent 的 MCP HTTP client 必须支持长超时，建议 30 到 60 分钟可配置。
- 沙箱镜像 MCP 是 Project Management Service 内置环境初始化 agent 专用工具，不注册到 Task Runner 任务执行 MCP 中。
- 不能把镜像工具挂进 Local Connector 通用 `/mcp` relay；本地镜像工具使用单独 `/api/local/sandbox/images/mcp` endpoint，再通过 Local Connector sandbox facade 暴露给 PM Service。

需要改造：

- `crates/chatos_mcp_runtime`：
  - 为 `McpHttpServer` 增加 `timeout_ms` 或 per-call timeout。
  - `jsonrpc_http_call` 不再固定 15 秒。
- `sandbox_manager_service/backend`：
  - 新增 `/api/sandbox-images/mcp` JSON-RPC 入口。
  - 使用共享 `chatos_sandbox_image_mcp` provider。
- `local_connector_client/core`：
  - 新增 `/api/local/sandbox/images/mcp` 专用 JSON-RPC 入口。
  - 不改通用 Local Connector MCP provider，避免 Task Runner 看到镜像工具。
- `local_connector_service/backend`：
  - sandbox facade 支持镜像相关路径。
  - relay timeout 对镜像创建增加长超时分支，不能沿用默认 30 秒。

## Agent 编排设计

新增模块：

- `project_management_service/backend/src/services/environment_agent.rs`
- `project_management_service/backend/src/services/environment_agent/context.rs`
- `project_management_service/backend/src/services/environment_agent/mcp_routing.rs`
- `project_management_service/backend/src/services/environment_agent/prompt.rs`
- `project_management_service/backend/src/services/environment_agent/result.rs`

职责：

1. 加载项目和 runtime environment 配置。
2. 判断是否需要初始化。
3. 解析 MCP 路由。
4. 构造 MCP executor。
5. 解析用户级项目管理 agent 默认模型配置。
6. 在 Project Management Service 内直接使用 `chatos_ai_runtime` 执行分析 prompt。
7. 校验 agent 输出。
8. 写入 project runtime environment 副表。

### Prompt 要点

系统提示需要明确：

- 先用文件 MCP 读取项目结构。
- 优先读取 manifest：
  - `package.json`
  - `pom.xml`
  - `build.gradle`
  - `go.mod`
  - `Cargo.toml`
  - `pyproject.toml`
  - `requirements.txt`
  - `docker-compose.yml`
  - `.env.example`
  - `application.yml`
- 判断是否为空或不可运行。
- 输出结构化 JSON：
  - `runnable`
  - `reason`
  - `detected_stack`
  - `runtime_features`
  - `required_services`
  - `images`
  - `env_vars`
  - `summary`
- 只有 `runnable = true` 时调用沙箱镜像 MCP。
- 数据库、Redis、Nacos 等服务需要生成启动凭据和环境变量。

### 建议的结构化输出

```json
{
  "runnable": true,
  "summary": "检测到 Java + Maven 项目，依赖 MySQL、Redis、Nacos。",
  "detected_stack": {
    "languages": ["java"],
    "frameworks": ["spring_boot"],
    "entrypoints": ["mvn spring-boot:run"]
  },
  "runtime_features": ["java@21", "maven"],
  "required_services": [
    {
      "key": "mysql",
      "type": "database",
      "display_name": "MySQL",
      "image_hint": "mysql",
      "env": {
        "MYSQL_DATABASE": "app",
        "MYSQL_USER": "app_user"
      }
    }
  ],
  "env_vars": {
    "DATABASE_URL": "mysql://app_user:${MYSQL_PASSWORD}@mysql:3306/app"
  },
  "not_runnable_reason": null
}
```

## 前端展示设计

项目详情页新增 Tab 或概览区块：

- 建议 Tab 名：`运行环境`
- 涉及文件：
  - `project_management_service/frontend/src/pages/ProjectDetailPage.tsx`
  - `project_management_service/frontend/src/pages/projectDetail/ProjectDetailTabs.tsx`
  - `project_management_service/frontend/src/types.ts`
  - `project_management_service/frontend/src/api/client.ts`

展示内容：

- 初始化状态：未启用 / 分析中 / 已就绪 / 不可运行 / 失败。
- 分析摘要。
- 不可运行原因和建议动作。
- 检测到的技术栈。
- 依赖服务列表。
- 镜像列表：环境名、镜像 ID、镜像 ref、provider、状态。
- 环境变量列表：
  - 默认脱敏展示敏感值。
  - 可复制变量名。
  - 后续可加“重新生成凭据”。

操作：

- `重新分析`
- `启用/关闭沙箱初始化`，仅本地项目展示。
- `重新准备镜像`

## API 设计

新增 Project Management HTTP API：

- `GET /api/projects/:project_id/runtime-environment`
- `PUT /api/projects/:project_id/runtime-environment/settings`
  - 修改 `sandbox_enabled`
- `POST /api/projects/:project_id/runtime-environment/analyze`
  - 手动触发重新分析
- `GET /api/projects/:project_id/runtime-environment/images`
- `GET /api/projects/:project_id/runtime-environment/env-vars`

创建项目 API：

- `CreateProjectRequest` 增加 `sandbox_enabled?: boolean`
- Chatos local connector 创建项目请求也增加 `sandbox_enabled?: boolean`
- 同步到 Project Management Service 时带上该字段

模型设置 API：

- 复用 `GET /api/ai-model-settings`
- 复用 `PUT /api/ai-model-settings`
- 请求和响应新增：
  - `project_management_agent_model_config_id?: string | null`
  - `project_management_agent_thinking_level?: string | null`

## 配置项

Project Management Service 新增配置：

- `PROJECT_SERVICE_ENV_AGENT_ENABLED`
- `PROJECT_SERVICE_ENV_AGENT_FALLBACK_MODEL_CONFIG_ID`
- `PROJECT_SERVICE_ENV_AGENT_MAX_ITERATIONS`
- `PROJECT_SERVICE_ENV_AGENT_REQUEST_TIMEOUT_MS`
- `PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL`
- `PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID`
- `PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY`
- `PROJECT_SERVICE_LOCAL_CONNECTOR_BASE_URL`
- `PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_SECRET`
- `PROJECT_SERVICE_SANDBOX_IMAGE_CREATE_TIMEOUT_MS`

建议默认：

- image create timeout：`3600000` 毫秒。
- agent max iterations：`20`。
- 默认模型优先读取用户级 `project_management_agent_model_config_id`，环境变量只作为兜底。

## 实施步骤

### 阶段 1：数据模型和 API

1. 新增 runtime environment 相关 models。
2. SQLite migration 和 Mongo collection/index。
3. Store 增加 CRUD：
   - get/upsert runtime environment
   - list/upsert images
   - list/upsert env vars
4. Project Management HTTP API 增加 runtime environment endpoints。
5. 创建项目请求增加 `sandbox_enabled`。
6. User Service / Chatos backend 的 `ai-model-settings` 增加项目管理 agent 默认模型字段。

验收：

- 创建本地项目选择不使用沙箱后，详情能看到 `disabled` 状态。
- 云端项目和本地启用沙箱项目能得到 `pending` 或 `analyzing` 状态。
- 右上角菜单能保存和读取项目管理 agent 默认模型。

### 阶段 2：沙箱镜像 MCP

1. 新建共享 `chatos_sandbox_image_mcp` crate。
2. 云端 Sandbox Manager 接入共享 provider。
3. Local Connector Client 接入共享 provider。
4. Local Connector Service sandbox facade/relay 支持镜像路径。
5. `crates/chatos_mcp_runtime` 支持 MCP HTTP 长超时。

验收：

- 云端 MCP 可 `search_sandbox_images`、`get_sandbox_image`、`ensure_sandbox_image`。
- 本地 MCP 可调用同名工具。
- `ensure_sandbox_image` 创建失败时返回具体失败原因，不返回轮询 job。

### 阶段 3：文件 MCP 路由

1. Project Management Service 增加 MCP 路由解析模块。
2. 本地项目根据 local connector project binding 构造文件 MCP。
3. 云端项目根据 Harness repo 信息构造 Harness MCP。
4. 根据 sandbox pairing 决定本地或云端沙箱镜像 MCP。
5. 降级逻辑记录到 runtime environment。

验收：

- 四种路由组合都能初始化 MCP executor。
- 项目 ID 不出现在模型工具 schema 入参中。

### 阶段 4：环境初始化 Agent

1. Project Management Service 接入 `chatos_ai_runtime` 和 `chatos_mcp_runtime`。
2. 增加 User Service 内部模型运行配置解析 API，供 Project Management Service 按用户和模型配置 ID 获取 `ModelRuntimeConfig` 所需字段。
3. Project Management Service 读取用户级 `project_management_agent_model_config_id` 和 `project_management_agent_thinking_level`。
4. 实现 project environment agent prompt。
5. 复用或抽取 `chatos/backend/src/services/project_run` 的识别逻辑作为 agent 前置上下文。
6. 构造文件 MCP、沙箱镜像 MCP、项目详情 MCP，并初始化 `McpExecutorBuilder`。
7. 实现 agent 运行状态机。
8. 实现结构化输出校验和写库。
9. 对空项目/不可运行项目短路，不创建镜像。

验收：

- 空目录项目写入 `not_runnable`。
- Node/Java/Rust/Go/Python 常见项目能识别 runtime features。
- 依赖 MySQL/Redis/Nacos 的项目能生成镜像和 env vars。

### 阶段 5：前端展示和触发

1. Chatos 创建本地项目弹窗增加“使用沙箱”选项。
2. Chatos 右上角用户菜单增加“项目管理 Agent 模型”入口。
3. 按 `MemoryModelSettingsPanel` 方式新增项目管理 agent 默认模型设置面板。
4. Project Management 前端项目详情增加运行环境 Tab。
5. 增加状态、镜像、环境变量、失败原因展示。
6. 增加重新分析按钮。
7. 本地项目可切换 sandbox 初始化开关。

验收：

- 本地项目选择不使用沙箱后不会自动初始化。
- 可以为项目管理环境初始化 agent 保存一个默认模型和 thinking level。
- 初始化失败和不可运行原因对用户可见。
- 凭据类环境变量默认脱敏。

## 风险和注意事项

- 当前 MCP HTTP 固定 15 秒超时必须先改，否则同步镜像创建无法工作。
- 现有云端和本地镜像初始化都是 job 模式，MCP 工具层必须负责等待最终状态。
- 运行环境变量涉及密码，建议后续接入统一 secret 加密，而不是长期明文 JSON。
- 本地沙箱开启状态来自 Local Connector sandbox pairing，需要处理设备离线、workspace 禁用、pairing disabled。
- Harness cloud 项目 import 未 ready 时不能读取文件。
- agent 输出必须做 schema 校验，不能把模型任意 JSON 直接写库。

## 推荐优先级

1. 先做数据模型、前端“是否使用沙箱”、项目详情展示。
2. 再做共享沙箱镜像 MCP，同步长超时是关键路径。
3. 然后做 MCP 路由和 agent。
4. 最后增强运行环境识别和服务依赖推断。

## 最小可交付版本

MVP 范围：

- 支持本地项目 sandbox_enabled 开关。
- 支持云端 Harness 文件读取、本地 Local Connector 文件读取。
- 支持云端 Sandbox Manager 镜像 MCP。
- 本地沙箱镜像 MCP 可以作为第二步补齐，但路由接口先预留。
- 支持 Node/Java/Python/Rust/Go 基础 manifest 识别。
- 能写入 `ready`、`not_runnable`、`failed` 三类结果并在项目详情展示。

完成 MVP 后，再补：

- Local Connector 本地镜像 MCP。
- Nacos/数据库/Redis 更完整的配置解析。
- 变量加密和重新生成。
- 定期或代码变更后的自动重新分析。
