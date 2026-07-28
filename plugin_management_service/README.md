# Plugin Management Service

插件管理服务负责 Plugin Marketplace、不可变 Release、MCP、skills、skill packages、系统内部 agent 和 agent capability bindings 的统一管理，并向 ChatOS、Task Runner 与 Local Connector 提供经过签名验证的能力和安装来源。

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
- 普通用户可以创建仅自己可见的 personal Plugin Marketplace；服务端会强制使用 private visibility、`admin_registry`、HTTPS signed Catalog 和显式 Catalog trust root。
- 系统 agent 配置页面及接口不向普通用户开放。
- 普通用户自己的 MCP/skills 后续由 Local Connector Client 上报。

## 主要 API

- `/api/mcps`
- `/api/skills`（只读兼容；创建、修改和删除统一通过 Plugin Release）
- `/api/skill-packages`（只读兼容；旧写接口已移除）
- `/api/system-agents`
- `/api/system-agents/:agent_key/mcp-bindings`
- `/api/plugin-marketplaces`
- `/api/plugin-marketplaces/:marketplace_id/sync`
- `/api/plugin-publishers`
- `/api/admin/plugin-marketplaces`
- `PATCH /api/admin/plugin-marketplaces/:marketplace_id`
- `/api/admin/plugin-marketplaces/:marketplace_id/sync`
- `/api/admin/plugin-publishers`
- `PATCH /api/admin/plugin-publishers/:publisher_record_id/review`
- `/api/admin/plugins`
- `/api/admin/plugins/:plugin_id/releases`
- `/api/runtime/agent-capabilities`

可信网络 Marketplace 的 Catalog 同步同时支持 owner/管理员手动触发和后台定时触发。Catalog 只允许无 credential/fragment 的 HTTPS URL，抓取时禁用 redirect 与系统代理、阻断非公网 DNS 地址，并在写入前验证 Catalog/Release Ed25519 签名、显式 key usage、密钥轮换/撤销、单调 revision/issued_at、Release 不可变性和 artifact URL。personal Marketplace 的 Catalog 和安装源在服务端强制绑定 owner；Local Connector 列表与 exact artifact 代理都会传递并重新校验同一个 effective owner ID，不能通过猜测 Plugin/Release ID 读取其他用户的 private 来源。

`super_admin` 可通过 Marketplace 编辑入口更新名称、HTTPS Catalog URL、enabled/trust 状态和 trust root。签名 key 采用失败关闭的两阶段轮换：先加入 successor key，再把旧 key 标记 `revoked_at`；非 revoked key 不可直接删除，key ID 对应的 publisher、算法、公钥、usage 和 `valid_from` 不可替换，`valid_until` 只能缩短，已有 revocation 不可撤销或改写。下一次更新才可移除已 revoked 的旧 key；可信网络 Marketplace 始终至少保留一个未撤销的 Catalog key。更新使用旧 Marketplace snapshot 做并发比较，Catalog 同步或其他管理员已修改时返回冲突，要求刷新后重试；审计仅记录 key ID 的 added/revoked/removed 集合，不记录公钥内容。

公开可信的 Admin Marketplace 还支持发布者入驻审核。普通用户可提交 publisher ID、名称、HTTPS 网站和 1-32 个 active Ed25519 Release-only 签名 key；服务端强制 key 的 publisher ID 与申请身份一致，pending/approved/suspended 记录不能自行覆盖，rejected 可重新提交。`super_admin` 可审核通过、拒绝、暂停或恢复发布者；通过时会把审核过的 Release key 合并进 Marketplace trust root，并复用 Marketplace snapshot compare-and-replace 与 key progression 校验。管理员手工创建 Plugin Catalog Entry 和 Release 时必须匹配已审核通过的 publisher 身份与 Release key；外部 signed Catalog 同步和 bundled official Registry 不走人工审核入口。publisher 审计只记录 key ID 和决策状态，不记录公钥内容。

系统 agent 的 MCP 配置只有三种状态：

- `disabled`：该 agent 不可见。
- `optional`：该 agent 可以按需调用。
- `required`：该 agent 默认必须携带。

项目来源和运行提供方不属于绑定配置。具体 MCP 在运行时根据项目上下文自行选择云端、本机或其他子实现。

Task Runner 的规划任务与执行任务复用底层模型运行时和 Worker，但使用独立的 Agent 身份、Prompt 与 MCP/skills 能力边界。纯规划任务进入 `task_runner_plan_phase`，工程执行任务进入 `task_runner_run_phase`。

## 当前系统 Agent

系统 Agent registry 登记当前代码中真实存在、具有独立 MCP/skills 能力边界的系统级智能体角色或运行模式：

- `chatos_conversation_agent`：Chat OS 普通对话智能体。可选使用 `task_runner_service`；用户联系人只提供角色上下文，不逐条登记。
- `chatos_planning_agent`：Chat OS 规划智能体。必需使用 `task_runner_service`，并将 Task Runner 切换到 `chatos_plan` profile。
- `task_runner_plan_phase`：Task Runner 规划任务智能体。使用只读代码、任务/项目管理、资料读取和询问用户能力，不开放代码写入与终端执行。
- `task_runner_run_phase`：Task Runner 执行任务智能体。负责代码修改、终端执行、测试、部署及工程验收。
- `project_management_agent`：项目运行环境智能体。必需 `CodeMaintainerRead`、`project_environment`、`sandbox_images`。
- `local_connector_command_approval_agent`：本机命令审批智能体。必需只读 `CodeMaintainerRead` 和 `local_connector_approval`。

Chat OS 的两个角色共用会话模型循环，但普通模式与规划模式的 MCP 强制性不同，因此分开管理。Task Runner 根据 `task_profile=chatos_plan && requires_execution=false` 路由到 `task_runner_plan_phase`；其他任务进入 `task_runner_run_phase`。Chat OS 用户联系人、prompt 生成、Agent Builder、浏览器视觉等一次性模型辅助工具不逐条登记。

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
- `PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED`：默认 `true`
- `PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS`：默认 `900`，范围 `60`–`86400`
- `PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS`：默认 `30000`
- `PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES`：默认 `8388608`，最大 `12582912`

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
