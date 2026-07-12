# Plugin Management Service

插件管理服务负责 MCP、skills、skill packages、系统内部 agent 和 agent capability bindings 的统一管理。

当前阶段已经实现服务自身的管理闭环，尚未接入 Chat OS、Task Runner、Project Management 和 Local Connector 的业务执行链路。

## 目录

- `backend`：Rust、axum、MongoDB
- `frontend`：React、TypeScript、Ant Design

## 本地依赖

- MongoDB：默认 `127.0.0.1:27018`
- User Service：默认 `http://127.0.0.1:39190`
- Rust toolchain
- Node.js 和 npm

## 启动后端

```bash
cargo run -p plugin_management_service_backend
```

默认地址：`http://127.0.0.1:39260`

健康检查：

```bash
curl http://127.0.0.1:39260/api/health
```

## 启动前端

```bash
cd plugin_management_service/frontend
npm install
npm run dev
```

默认地址：`http://127.0.0.1:39261`

Vite 会把 `/api` 代理到 `http://127.0.0.1:39260`。

## 鉴权

- `POST /api/auth/login` 代理到 User Service。
- 保护接口使用 Bearer token。
- 后端通过 User Service `/api/auth/verify` 校验 token。
- `super_admin` 可以管理 public、system_private 和系统 agent 的内部 MCP 矩阵。
- 系统 agent 配置页面及接口不向普通用户开放。
- 普通用户自己的 MCP/skills 后续由 Local Connector Client 上报。

## 主要 API

- `/api/mcps`
- `/api/skills`
- `/api/skill-packages`
- `/api/system-agents`
- `/api/system-agents/:agent_key/mcp-bindings`
- `/api/runtime/agent-capabilities`

系统 agent 的 MCP 配置只有三种状态：

- `disabled`：该 agent 不可见。
- `optional`：该 agent 可以按需调用。
- `required`：该 agent 默认必须携带。

项目来源和运行提供方不属于绑定配置。具体 MCP 在运行时根据项目上下文自行选择云端、本机或其他子实现。

`task_runner_run_phase` 是 Task Runner 当前唯一独立的模型/工具执行循环，同时承载普通任务和 `chatos_plan` 任务 profile。它会自动包含当前用户通过 Local Connector 添加的 MCP 和 skills，不需要管理员逐项绑定。

## 当前系统 Agent

系统 Agent registry 登记当前代码中真实存在、具有独立 MCP/skills 能力边界的系统级智能体角色或运行模式：

- `chatos_conversation_agent`：Chat OS 普通对话智能体。可选使用 `task_runner_service`；用户联系人只提供角色上下文，不逐条登记。
- `chatos_planning_agent`：Chat OS 规划智能体。必需使用 `task_runner_service`，并将 Task Runner 切换到 `chatos_plan` profile。
- `task_runner_run_phase`：Task Runner 任务智能体。必需 `TaskManager`、`AskUser`；其余已实现的 Task Runner builtin MCP 为可选。
- `project_management_agent`：项目运行环境智能体。必需 `CodeMaintainerRead`、`project_environment`、`sandbox_images`。
- `local_connector_command_approval_agent`：本机命令审批智能体。必需只读 `CodeMaintainerRead` 和 `local_connector_approval`。

Chat OS 的两个角色共用会话模型循环，但普通模式与规划模式的 MCP 强制性不同，因此分开管理。`chatos_plan` 本身仍是 Task Runner task profile，不额外登记为 `task_runner_plan_phase`。Chat OS 用户联系人、prompt 生成、Agent Builder、浏览器视觉等一次性模型辅助工具不逐条登记。

## 环境变量

- `PLUGIN_MANAGEMENT_SERVICE_HOST`
- `PLUGIN_MANAGEMENT_SERVICE_PORT`
- `PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL`
- `PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE`
- `PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL`
- `PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS`
- `PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME`
- `PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD`
- `PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES`

## 系统 Seed

首次启动会补齐：

- 系统 builtin MCP
- 系统内部 agent registry
- 默认 system_required bindings

Seed 会补齐缺失资源，保留管理员对 MCP 启用状态和绑定模式的修改，同时同步系统 Agent 的规范名称，并清理已经确认不存在的历史伪 Agent 及其绑定。

## 验证

```bash
cargo test -p plugin_management_service_backend
cargo check -p plugin_management_service_backend

cd plugin_management_service/frontend
npm run type-check
npm run build
```
