# MCP Management Service 统一控制与运行网关设计

## 0. 文档状态

- 状态：2.0.10 目标架构
- 日期：2026-07-31
- 新服务名：mcp_management_service
- 关联方案：CLOUD_ORCHESTRATION_LIGHT_LOCAL_CONNECTOR_MIGRATION_PLAN.zh-CN.md
- 复用基础：mcp/、chatos_mcp_runtime、chatos_mcp_service、chatos_plugin_management_sdk

当前 2.0.10 实施状态：Phase 0 已完成；Phase 1 的权威 Project Execution Context、Plugin Management capability 聚合、required MCP 阻断、工具 Schema 快照、稳定 `route_revision` 和短期 Runtime Grant 已接通；Phase 2 的聚合 `/mcp` 已支持 `initialize`、`ping`、`tools/list`、`tools/call`、固定路由校验、结构化 invocation 日志、Provider 超时和响应大小限制，并已接入首个无状态 Embedded WebTools Provider；Phase 3 已接通 Local Connector、Harness 以及 Cloud Sandbox 的文件与终端 Provider；Phase 4 已接通 Project Management、Project Runtime Environment、Task Runner Service、Task Process Log、ChatOS Memory Readers、Notepad、Agent Builder 和 Sandbox Images Provider；Task Runner 已具备 `shadow` 观测和显式 `gateway` canary 模式，部署默认仍为 `shadow`。共享 Session Snapshot 存储、取消传播、云端 Browser、其余 External/Plugin Provider、Memory Agent 调用方迁移和旧直连清理仍待完成，因此旧调用链继续保留。

本文档定义一个新的 MCP Management Service。它同时承担 MCP 控制面聚合和 MCP 运行网关职责，使 ChatOS、Task Runner、Project Management、Memory Agent 等调用方不再各自判断 MCP 在哪里、以什么协议、通过哪个服务执行。

---

## 1. 最终目标

最终希望达到：

1. Plugin Management 是 Agent 到 MCP 配置的权威入口。
2. Agent 配置了某个已启用 MCP 后，运行时真实加载它，不再被各宿主的重复 allowlist 静默过滤。
3. 所有 Agent 工具调用只连接一个聚合 MCP Server：MCP Management Service。
4. MCP Management Service 根据 Project、Run、Device、Sandbox 和组件执行策略选择真实 Provider。
5. 本地 Workspace 的文件、Git、终端等调用通过 Local Connector Service。
6. 云端 Workspace 的文件、Git、终端等调用通过 Harness 或 Cloud Sandbox。
7. Project Management、Task Runner、Memory、Sandbox Images 等内部服务以标准 MCP Provider 接入。
8. 外部 HTTP MCP、云端 stdio MCP、本地 Connector MCP 和 Plugin MCP 使用同一套运行合同。
9. 路由一旦为某个 Runtime Session 固定，执行中不静默切换 Provider。
10. 鉴权、命名空间、取消、重试、审计、限流和错误格式由一个服务统一处理。

一句话定义：

> Plugin Management 决定 Agent 拥有哪些 MCP；MCP Management Service 决定这些 MCP 在本次运行中由谁执行，并提供唯一调用入口。

---

## 2. 服务边界

MCP Management Service 只负责 Agent Tool Plane，不是通用微服务网关。凡是作为 Agent 工具暴露的能力，都必须经过它完成配置解析、Catalog 聚合、Provider 路由和真实调用。

是否经过本服务只有一个判定标准：该调用是否是 Agent 发起的 MCP 工具调用。非工具型的服务间调用继续使用现有 REST、消息队列或内部 SDK，不属于本服务的设计和流量范围。

- Agent 的 tools/list。
- Agent 的 tools/call。
- Agent 调 Project Management、Task Runner、Memory、Sandbox、文件、Git、终端、浏览器等能力。
- Agent 调系统 MCP。
- Agent 调用户配置的外部 MCP。
- Agent 调 Plugin 提供的 MCP、Skill executable tool、Command 和 Agent tool。
- ChatOS、Task Runner、Project Environment Agent 等为模型构建 MCP Tool Catalog。
- MCP Provider 的统一可用性查询、路由解析、调用取消和审计。

> 所有面向 Agent 的内部服务能力调用，统一通过 MCP Management Service。

---

## 3. 与现有统一 MCP 架构的关系

仓库目前已经完成了统一 MCP Catalog：

- mcp/：19 个 System MCP 的身份、Descriptor、Tool Schema、Provider Skills 和共享实现。
- chatos_mcp_runtime：HTTP、stdio、in-process MCP 执行器。
- chatos_mcp_service：JSON-RPC MCP Server 基础设施。
- Plugin Management：MCP 资源、Agent binding、Plugin 和可用性控制面。
- 各宿主：SystemMcpHostAdapter 和具体 Provider。

这些工作不废弃。新的分层为：

| 组件 | 新职责 |
| --- | --- |
| mcp/ | 编译期权威 Catalog、Schema、共享 Provider 实现和合同 |
| chatos_mcp_runtime | MCP Client/Executor 和协议运行时 |
| chatos_mcp_service | 构建标准 MCP JSON-RPC Server |
| Plugin Management | MCP 资源、Agent binding、Plugin catalog 和策略事实数据 |
| MCP Management Service | 聚合 Catalog、运行 Session、Provider 路由、调用网关 |
| 业务微服务 | 拥有业务数据并暴露标准 MCP Provider |
| Local Connector Service | 云端到设备的安全 relay |
| Local Connector Client | 本地文件、命令、Plugin/Skill 和 Sandbox 的实际执行端 |

现有 SystemMcpHost.supported_hosts 不再作为“能否给 Agent 配置”的控制面限制。它只能作为迁移期兼容信息，最终由 MCP Management Service 的 Provider Registry 和 Routing Policy 判断真实路由。

---

## 4. 目标架构

~~~mermaid
flowchart LR
    UI["ChatOS / Desktop UI"]
    AGENTS["Cloud Agent Runtimes<br/>Chat / Plan / Task / Memory / Environment"]
    PMS["Plugin Management Service<br/>Agent MCP Bindings"]
    MCPS["MCP Management Service"]
    CAT["Catalog + Capability Resolver"]
    ROUTER["Routing Policy Engine"]
    GATEWAY["Aggregated MCP JSON-RPC Gateway"]
    INTERNAL["Internal Service Providers<br/>Task / Project / Memory / Sandbox"]
    HARNESS["Cloud Harness / Cloud Sandbox"]
    LCS["Local Connector Service"]
    LCC["Local Connector Client"]
    EXTERNAL["External HTTP / Cloud stdio MCP"]
    AUDIT[("Route Cache / Audit")]

    UI --> AGENTS
    AGENTS -->|"resolve runtime session"| MCPS
    PMS -->|"resolved Agent capabilities"| CAT
    MCPS --> CAT
    CAT --> ROUTER
    ROUTER --> GATEWAY
    GATEWAY --> INTERNAL
    GATEWAY --> HARNESS
    GATEWAY --> LCS
    LCS --> LCC
    GATEWAY --> EXTERNAL
    MCPS --> AUDIT
    AGENTS -->|"one MCP endpoint"| GATEWAY
~~~

Agent Runtime 不再拿到多个内部 MCP URL。它只拿到：

~~~text
mcp_server_url = https://mcp-management.internal/mcp
runtime_session_token = short-lived signed token
~~~

tools/list 由网关聚合，tools/call 由网关路由。

---

## 5. 服务内部模块

建议目录：

~~~text
mcp_management_service/
└── backend/
    └── src/
        ├── api/
        │   ├── health.rs
        │   ├── catalog.rs
        │   ├── runtime_sessions.rs
        │   ├── provider_status.rs
        │   └── mcp.rs
        ├── auth/
        │   ├── internal.rs
        │   └── runtime_grant.rs
        ├── capabilities/
        │   ├── plugin_management.rs
        │   ├── materializer.rs
        │   └── namespace.rs
        ├── project_context/
        │   ├── resolver.rs
        │   └── model.rs
        ├── routing/
        │   ├── engine.rs
        │   ├── policy.rs
        │   ├── system_mcp.rs
        │   ├── external_mcp.rs
        │   └── plugin_mcp.rs
        ├── providers/
        │   ├── internal_service.rs
        │   ├── local_connector.rs
        │   ├── harness.rs
        │   ├── cloud_sandbox.rs
        │   ├── external_http.rs
        │   ├── cloud_stdio.rs
        │   └── embedded.rs
        ├── runtime/
        │   ├── aggregated_provider.rs
        │   ├── invocation.rs
        │   ├── cancellation.rs
        │   ├── idempotency.rs
        │   └── result_policy.rs
        ├── observability/
        │   └── audit.rs
        ├── config.rs
        ├── state.rs
        ├── lib.rs
        └── main.rs
~~~

同时新增：

~~~text
crates/chatos_mcp_management_sdk/
├── src/dto.rs
├── src/client.rs
├── src/error.rs
└── src/lib.rs
~~~

SDK 由 ChatOS、Task Runner、Project Management 和其他 Agent 宿主使用。

---

## 6. 控制面设计

### 6.1 权威数据归属

| 数据 | 权威来源 |
| --- | --- |
| System MCP 身份与静态 Schema | mcp/ Catalog |
| MCP 资源记录 | Plugin Management |
| Agent MCP Binding | Plugin Management |
| Plugin 组件与 execution_host | Plugin Management |
| Local MCP Inventory | Plugin Management + Local Connector 状态 |
| Project source/workspace 信息 | Project Context Provider |
| Device/Workspace 在线状态 | Local Connector Service |
| Sandbox pairing/lease | Sandbox Manager / Local Connector Service |
| Runtime Session 路由快照 | MCP Management Service |
| 工具调用业务结果 | 调用方业务服务 |
| 工具调用审计 | MCP Management Service |

MCP Management Service 不复制 Plugin Management 的配置数据库。它读取 resolved capabilities，并按 policy_revision 缓存。

### 6.2 Agent 配置语义

Plugin Management 的 binding 建议收敛为：

~~~text
disabled  -> 不进入 Runtime Session
enabled   -> 进入 tools/list；Provider 暂时不可用时 tools/call 返回明确错误
required  -> 进入 tools/list；Runtime Session 创建时必须能解析出合法路由
~~~

重要变化：

- available 不再决定是否“配置生效”。
- available 是运行健康状态，不是配置状态。
- binding.enabled && resource.enabled 才决定是否物化。
- required 额外要求 route resolvable。
- 不再由 ChatOS、Task Runner、Local Connector 各自二次过滤。

### 6.3 Runtime Session

每次 Agent Turn 或 Task Run 创建一个 MCP Runtime Session。

请求至少包括：

~~~json
{
  "owner_user_id": "user-1",
  "agent_key": "task_runner_run_phase",
  "project_id": "project-1",
  "run_id": "run-1",
  "turn_id": null,
  "task_id": "task-1",
  "task_profile": "implementation",
  "source_session_id": null,
  "source_user_message_id": null,
  "default_model_config_id": null,
  "expected_project_task_ids": [],
  "requested_device_id": null,
  "requested_sandbox_provider": null
}
~~~

响应：

~~~json
{
  "session_id": "mcp_session_...",
  "policy_revision": "...",
  "route_revision": "...",
  "expires_at": "...",
  "mcp_server_url": "http://mcp-management-service:.../mcp",
  "runtime_token": "...",
  "configured_mcp_count": 8,
  "unavailable_required_mcps": []
}
~~~

Runtime Session 应优先使用签名 Token 表达，不依赖单机内存，确保服务可以水平扩展。

### 6.4 Runtime Grant

签名 Runtime Grant 至少绑定：

~~~text
session_id
caller_service
owner_user_id
agent_key
project_id
run_id / turn_id / task_id
policy_revision
route_revision
allowed_resource_ids
issued_at
expires_at
~~~

TTL 建议：

- Chat Turn：15～30 分钟。
- Task Run：30～120 分钟，可受控刷新。
- 单个 Provider 内部 Token：30～120 秒。

---

## 7. Project Execution Context

路由必须使用标准化的 Project Execution Context，不能继续从 root_path 前缀临时猜测。

建议模型：

~~~json
{
  "project_id": "project-1",
  "owner_user_id": "user-1",
  "execution_plane": "cloud",
  "workspace_provider": "local_connector",
  "workspace": {
    "device_id": "device-1",
    "workspace_id": "workspace-1",
    "relative_root": "apps/backend"
  },
  "sandbox_provider": "local_connector",
  "sandbox_pairing_id": "pairing-1",
  "source_type": "local_connector",
  "revision": "project-revision"
}
~~~

允许的 workspace_provider：

~~~text
local_connector
harness
cloud_sandbox
cloud_storage
none
~~~

允许的 sandbox_provider：

~~~text
local_connector
cloud
none
~~~

Project Context 只提供事实。具体 MCP 应选择哪个 Provider，由 Routing Policy Engine 决定。

### 7.1 Project Context 获取

推荐顺序：

1. MCP Management Service 调用 Project Context 内部接口。
2. Project Context Provider 返回 owner-scoped、revisioned 快照。
3. MCP Management Service 对快照短缓存。
4. Runtime Session 固定该 revision。

不要让调用方任意提交 device_id、workspace_id 或 provider 后直接信任。调用方只提交 project_id 和可选的受控 override。

---

## 8. Provider 路由模型

### 8.1 Provider 类型

~~~text
embedded
internal_service
local_connector
harness
cloud_sandbox
external_http
cloud_stdio
plugin_local
plugin_cloud
unavailable
~~~

### 8.2 Route 结果

~~~json
{
  "resource_id": "builtin_code_maintainer_read",
  "server_name": "code_maintainer_read",
  "provider_kind": "local_connector",
  "provider_ref": "device-1/workspace-1",
  "tool_namespace": "code_maintainer_read",
  "allow_writes": false,
  "retry_class": "idempotent_read",
  "cancel_supported": true,
  "reason": "project workspace_provider is local_connector"
}
~~~

provider_ref 是内部不透明引用。不得把本机绝对路径或敏感 URL写入 Runtime Grant。

### 8.3 路由优先级

~~~text
显式且授权的 Run override
  > Project policy
  > workspace_provider / sandbox_provider 默认规则
  > MCP 资源 execution_host
  > 系统全局默认
~~~

运行时禁止：

- Provider 调用失败后自动换 Provider。
- Local Connector 离线后自动改 Cloud Sandbox。
- Cloud stdio 失败后自动在用户机器启动。
- portable Plugin 根据网络错误静默切 host。

Provider 切换只能在创建新的 Runtime Session 时发生，并记录新的 route_revision。

---

## 9. 文件读取 MCP 的完整示例

以 CodeMaintainerRead 为例。

### 9.1 Local Workspace Project

Project Context：

~~~text
execution_plane = cloud
workspace_provider = local_connector
device_id = device-1
workspace_id = workspace-1
~~~

路由：

~~~text
Agent
  -> MCP Management Service
  -> LocalConnectorProvider
  -> Local Connector Service relay
  -> Local Connector Client
  -> Workspace grant 内执行 read_file/search_text/list_dir
~~~

客户端负责：

- workspace_id 到绝对路径映射。
- 相对路径规范化。
- ..、绝对路径和符号链接越界拒绝。
- 文件大小限制。
- 结果脱敏和长度限制。

云端不接触绝对路径。

### 9.2 Harness Project

Project Context：

~~~text
execution_plane = cloud
workspace_provider = harness
~~~

路由：

~~~text
Agent
  -> MCP Management Service
  -> HarnessProvider
  -> Project Management Harness MCP
  -> Harness workspace
~~~

### 9.3 Cloud Sandbox Project

Project Context：

~~~text
workspace_provider = cloud_sandbox
sandbox_provider = cloud
~~~

路由：

~~~text
Agent
  -> MCP Management Service
  -> CloudSandboxProvider
  -> Sandbox Manager lease MCP
  -> mounted workspace
~~~

### 9.4 不可用状态

如果 Local Connector Project 的设备离线：

- tools/list 仍可以展示配置的文件工具。
- required MCP 创建 Session 时返回 connector_offline。
- enabled MCP 的 tools/call 返回 provider_unavailable。
- 不自动切换 Harness 或 Cloud Sandbox。

---

## 10. System MCP 路由建议

| System MCP | Local Workspace | Cloud/Harness | 业务 Provider |
| --- | --- | --- | --- |
| CodeMaintainerRead | Local Connector | Harness/Cloud Sandbox | Workspace Router |
| CodeMaintainerWrite | Local Connector | Harness/Cloud Sandbox | Workspace Router |
| TerminalController | Local Connector host/sandbox | Cloud Sandbox | Command Router |
| ProjectManagement | Project Management Service | Project Management Service | Internal Service |
| TaskRunnerService | Task Runner Service | Task Runner Service | Internal Service |
| AskUser | ChatOS/Task callback provider | ChatOS/Task callback provider | Internal callback |
| Notepad | ChatOS 云端用户 Store | ChatOS 云端用户 Store | Internal Service |
| AgentBuilder | ChatOS Agent Store | ChatOS Agent Store | Internal Service |
| WebTools | Cloud provider | Cloud provider | Embedded/External |
| BrowserTools | Local Connector 或云端 Browser | 云端 Browser | Policy Router |
| Memory readers | Memory/ChatOS provider | Memory/ChatOS provider | Internal Service |
| SandboxImages | Local Sandbox facade | Sandbox Manager | Sandbox Router |
| ProjectEnvironment | Project Management Service | Project Management Service | Internal Service |
| ProjectRuntimeEnvironment | Project Management Service | Project Management Service | Internal Service |
| LocalCommandApproval | Local Connector | 不可用 | Local Connector |
| TaskProcessLog | Task Runner run scope | Task Runner run scope | Internal Service |

这张表只是默认规则，真实实现必须由 Routing Policy Engine 中的显式策略表示，不能散落为各服务的 if/else。

---

## 11. 外部 MCP 和 Plugin MCP

### 11.1 外部 HTTP MCP

- URL 来自 Plugin Management 的资源记录。
- MCP Management Service 只接受无 URL 凭据、无 fragment 的 HTTPS endpoint，并在创建 Runtime Session 时解析 DNS、拒绝私网/loopback/link-local/保留地址，将通过校验的地址固定到该 Session 的专用无代理 Client；禁止重定向，避免 DNS rebinding 和代理绕过。
- `runtime.headers` 只通过 Plugin Management 的签名内部 capabilities 接口进入服务端私有 Session Binding；非 owner、非 super-admin 的普通 MCP CRUD 和用户侧 capabilities 响应会清空 `runtime.headers`/`runtime.env`/`runtime.args` 并移除 URL query，同时保留 owner/admin 编辑既有配置的能力。凭据不进入 Runtime Token、公开 Route Snapshot 或调用日志；URL query、Header 值、工具参数和结果正文也不写日志。后续 Plugin OAuth 动态凭据应继续通过受限凭据引用解析到同一私有 Binding。
- Host、Content-Type、Content-Length、hop-by-hop Header 和内部服务签名 Secret/Token Header 不能由外部 MCP 配置覆盖。
- tools/list 可以短缓存。
- tools/list 和 tools/call 使用统一超时、大小限制、JSON-RPC id 校验和结构化审计；`allowed_tool_names`/`blocked_tool_names` 同时约束工具曝光和真实执行。
- External HTTP route 为只读时必须配置显式 `allowed_tool_names`，避免无法判断语义的第三方工具在 `allow_writes=false` 下仍执行写操作；允许写入时可继续用 allowlist/blocklist 收窄工具集合。
- Local Connector 的 loopback/局域网 MCP 不允许伪装成 External HTTP MCP，必须继续走 Local Connector Provider。

### 11.2 Cloud stdio MCP

- 不能在 MCP Management Service 主进程直接 spawn 任意命令。
- 必须绑定本次 Runtime Session 的 Cloud Sandbox lease，经 `MCP Management -> Sandbox Manager -> Sandbox Agent` 创建受控进程；没有 sandbox target 的 `stdio_cloud` route 直接 unavailable，required 绑定会阻断 Session。
- command、args、env、cwd 只从 Plugin Management 的签名 capabilities 进入 MCP Management 私有 Binding，不进入 Runtime Token、公开 Route Snapshot、模型参数或日志正文。Sandbox Manager 只允许 `mcp-management-service` 调用该内部代理，并再次校验 lease、owner、project、run 和 environment service。
- Sandbox Agent 只接受 PATH 中的直接可执行文件名，拒绝绝对命令路径、越界 cwd、shell `-c`/`/c`/PowerShell inline command、Host 控制环境变量和超限配置。目标进程启动前清空 Agent 环境，只重新注入固定 PATH、隔离 HOME/TMP、workspace 标识和审核后的 env，避免把 Sandbox Agent Token 或服务 Secret 传给 MCP。
- Runtime Session 创建时通过相同受控链路真实执行 `tools/list`，返回的 live Schema 参与工具曝光和 `route_revision`；Plugin Management 不再为 `stdio_cloud` 在自身服务进程直接 spawn。
- stdio 进程按 `runtime_session_id + resource_id` 固定为持久会话，相同 Session 内配置漂移 fail closed。调用超时、进程错误、显式 Session close、Session TTL 到期或 Sandbox 销毁都会终止完整进程树。当前 route 明确标记 `cancel_supported=false`，逐 invocation 的协议级取消仍需后续补充，不能虚报取消能力。
- 只读 Cloud stdio 与 External HTTP 一样必须配置显式 `allowed_tool_names`；allowlist/blocklist 同时约束 Schema 曝光和真实调用。

### 11.3 Local Connector MCP

- MCP Management Service 不直接连接用户机器。
- 调用 MCP Management Service -> Local Connector Service -> Client。
- 使用 owner、device、workspace、resource、run 和 scope 绑定的短期内部 Token。
- 客户端继续做 manifest hash、安装状态和本地权限检查。

### 11.4 Plugin MCP

按组件 execution_host：

~~~text
cloud    -> PluginCloudProvider
local    -> PluginLocalProvider through Local Connector
portable -> Runtime Session 创建时显式选定一个 host
~~~

Plugin 的 MCP Server、Skill executable tool、Command 和 Agent tool 都可以进入聚合 Tool Catalog，但必须使用 immutable release/component snapshot。

---

## 12. Tool Catalog 聚合与命名空间

### 12.1 问题

多个 MCP 可能都定义 read、search、status 等工具名。聚合后必须避免冲突。

### 12.2 规则

对模型暴露的稳定名称：

~~~text
{server_name}_{tool_name}
~~~

例如：

~~~text
code_maintainer_read_read_file
project_management_service_create_requirement
task_runner_service_create_task
~~~

Runtime Session 保存映射：

~~~text
exposed_tool_name
  -> resource_id
  -> provider route
  -> original_tool_name
~~~

不允许运行时根据字符串模糊匹配 Provider。

### 12.3 Schema 来源

优先级：

1. System MCP：mcp/ 的静态 Catalog。
2. Internal Service Dynamic MCP：调用服务 tools/list。
3. Plugin MCP：immutable component snapshot + Provider tools/list。
4. External MCP：远端 tools/list 缓存。
5. Local MCP 离线：Plugin Management 最近一次已校验的 tool snapshot。

Schema 快照必须进入 route_revision，避免 tools/list 和 tools/call 指向不同版本。

---

## 13. 调用协议

### 13.1 聚合 MCP Endpoint

~~~text
POST /mcp
Authorization: Bearer {runtime_token}
Content-Type: application/json
~~~

支持：

- initialize
- ping
- tools/list
- tools/call
- notifications/cancelled 或等价取消扩展

### 13.2 内部管理 API

~~~text
GET  /health
GET  /api/internal/catalog
POST /api/internal/runtime/sessions/resolve
POST /api/internal/runtime/sessions/refresh
GET  /api/internal/runtime/sessions/{id}/routes
POST /api/internal/runtime/invocations/{id}/cancel
GET  /api/internal/providers/status
~~~

### 13.3 Provider Adapter 合同

建议核心 Trait：

~~~rust
#[async_trait]
pub trait RoutedMcpProvider: Send + Sync {
    fn provider_kind(&self) -> McpProviderKind;
    async fn list_tools(
        &self,
        route: &ResolvedMcpRoute,
        context: &McpInvocationContext,
    ) -> Result<Vec<Value>, McpRuntimeError>;
    async fn call_tool(
        &self,
        route: &ResolvedMcpRoute,
        tool_name: &str,
        arguments: Value,
        context: &McpInvocationContext,
    ) -> Result<Value, McpRuntimeError>;
    async fn cancel(
        &self,
        invocation_id: &str,
        context: &McpInvocationContext,
    ) -> Result<CancelOutcome, McpRuntimeError>;
}
~~~

业务服务只实现自己的 Provider 或暴露标准 /mcp，网关负责统一包装。

---

## 14. 运行时解析算法

~~~text
resolve_runtime_session(request):
  1. authenticate caller service
  2. resolve Project Execution Context
  3. call Plugin Management resolve_for_service(include_unavailable=true)
  4. keep resources where binding.enabled && resource.enabled
  5. for each MCP:
       a. resolve System/External/Plugin identity
       b. select provider using Routing Policy
       c. validate project/device/sandbox requirements
       d. materialize tool schema snapshot
       e. allocate stable namespace
  6. fail if any required MCP has no legal route
  7. calculate route_revision
  8. issue signed Runtime Grant
  9. return the single aggregated MCP endpoint
~~~

tools/call：

~~~text
  1. verify Runtime Grant and expiry
  2. resolve exposed tool name from immutable route snapshot
  3. enforce allow_writes and tool-level policy
  4. create invocation_id and idempotency record
  5. exchange short-lived Provider token
  6. dispatch to exact Provider
  7. normalize result and redact sensitive data
  8. write audit event
  9. return MCP result
~~~

---

## 15. 错误、重试与取消

统一错误码建议：

~~~text
mcp_not_configured
mcp_disabled
required_mcp_unavailable
route_not_found
provider_unavailable
connector_offline
workspace_unavailable
sandbox_unavailable
tool_not_found
tool_schema_changed
approval_required
permission_denied
invocation_cancelled
unknown_execution_state
provider_timeout
result_too_large
~~~

重试规则：

| 类型 | 自动重试 |
| --- | --- |
| tools/list | 可以 |
| 只读文件、查询 | 有 request_id 时可以 |
| 文件写入 | 默认不可以 |
| Git mutation | 默认不可以 |
| 命令执行 | 默认不可以 |
| Plugin action | 由组件声明 |
| Sandbox create | 需要幂等 lease key |

取消：

- Agent Runtime 只向 MCP Management Service 取消。
- 网关根据固定 route 传播到内部服务、Local Connector、Sandbox 或 Plugin。
- Provider 未确认取消时返回 cancel_requested。
- 断线后无法确认 mutation 状态时返回 unknown_execution_state。

---

## 16. 安全模型

### 16.1 服务身份

- 每个调用方有独立 caller_service。
- 内部 Token 绑定 audience、scope、path 和 TTL。
- ChatOS Token 不能调用 Task Runner 专用管理 API。
- Provider Token 不能反向调用 Runtime Session 创建接口。

### 16.2 用户和项目隔离

每个请求必须绑定：

~~~text
owner_user_id
project_id
agent_key
run_id / turn_id
resource_id
provider route
~~~

Project Context owner 必须与 Runtime Grant owner 一致。

### 16.3 本地边界

- 云端不保存绝对路径。
- device_id/workspace_id 必须来自 Project Context。
- 本地命令审批不能被网关绕过。
- Plugin Secret/OAuth Token 不通过网关回传。
- Local Connector 只接受 scope 和路径绑定的内部请求。

### 16.4 外部 MCP

- HTTPS 优先，HTTP 仅允许受控 loopback/内部网络场景。
- 阻止 metadata、私网横向访问和 DNS rebinding。
- Header Secret 使用凭据引用。
- 响应限制大小和深度。
- tools/list Schema 做严格校验。

---

## 17. 高可用与性能

### 17.1 无状态优先

- Runtime Session 使用签名 Grant。
- Route snapshot 可以编码为压缩引用并存 Redis/Mongo，也可以短期放共享缓存。
- 服务实例不拥有不可恢复的 Agent 状态。

### 17.2 缓存

| 数据 | 建议缓存 |
| --- | --- |
| System Catalog | 进程静态 |
| Agent capabilities | policy_revision，30～60 秒 |
| Project Context | project revision，15～30 秒 |
| External tools/list | 30～300 秒 |
| Local MCP tool snapshot | 使用 Plugin Management 已校验快照 |
| Provider health | 5～15 秒 |

### 17.3 限流

按以下维度组合：

- owner_user_id
- caller_service
- agent_key
- project_id
- resource_id
- provider_kind

文件搜索、浏览器、命令和外部网络工具使用不同配额。

---

## 18. 可观测性

每次解析记录：

~~~text
session_id
policy_revision
route_revision
project_revision
configured resource count
provider distribution
required unavailable count
resolution duration
~~~

每次调用记录：

~~~text
invocation_id
session_id
resource_id
exposed_tool_name
provider_kind
duration
result size
status
cancel outcome
~~~

不得默认记录：

- 文件正文。
- 命令完整输出。
- 本机绝对路径。
- Secret、OAuth Token、设备私钥。
- 用户输入中的敏感字段。

---

## 19. 分阶段实施

### Phase 0：合同与服务骨架

- 新建 chatos_mcp_management_sdk。
- 定义 ProjectExecutionContext、McpProviderKind、ResolvedMcpRoute。
- 新建 mcp_management_service/backend。
- 提供 health、catalog 和纯函数 route resolution。
- 复用 mcp/ System Catalog。
- 加入 Cargo workspace。

2.0.10 首次实现额外提供 `POST /api/internal/routes/resolve`，用于验证 Route Engine 和调用方集成。该接口只返回路由预览，不签发 Runtime Grant，也不能据此绕过 Plugin Management 配置直接调用工具。正式运行入口仍然是 Phase 1 的 `/api/internal/runtime/sessions/resolve`。

验收：

- 新服务可以列出全部 System MCP。
- 单元测试可根据 local_connector/harness/cloud_sandbox 得到稳定路由。

### Phase 1：控制面聚合

- 接入 PluginManagementClient.resolve_for_service。
- 实现 Agent capabilities materializer。
- 所有 binding.enabled && resource.enabled MCP 进入 Session。
- required MCP 路由失败时阻断。
- 生成 route_revision 和 Runtime Grant。

当前已完成本阶段主链路：只按 `binding.enabled && resource.enabled` 物化 MCP，`available=false` 不会让已配置 MCP 消失；MCP Management 通过 Project Management 的 owner-scoped 内部接口获取权威 Context，通过专用 caller 身份调用 Plugin Management；required MCP 无合法路由或无可用工具 Schema 时 fail closed；Runtime Grant 绑定 caller、owner、Agent、Project、run/turn/task、policy revision、包含 Schema 快照的 route revision 和精确资源集合。当前 Session Snapshot 暂存进程内存，水平扩展前需迁移到 Redis/Mongo 等共享存储。

验收：

- Plugin Management 给 Agent 配置任意合法 MCP 后，新服务解析结果包含该 MCP。
- 不再因 availability=false 从配置中消失。

### Phase 2：聚合 MCP Server

- 使用 chatos_mcp_service 实现 /mcp。
- 聚合 tools/list。
- 实现稳定命名空间。
- 实现 invocation audit、超时和结果限制。
- 先接 InternalServiceProvider 和 EmbeddedProvider。

当前已完成聚合 MCP JSON-RPC 主链路。`tools/list` 只返回 Runtime Session 的命名空间化工具快照，`tools/call` 必须命中同一快照并还原原始工具名；每次调用生成独立 invocation id，记录结构化元数据但不记录参数与结果正文；Provider Client 禁止 HTTP 重定向，统一限制请求超时、响应大小和 JSON-RPC id，并允许 AskUser 这类长等待工具使用独立的逐工具超时，不放宽普通工具超时。当前已注册的 Internal Service Adapter 是 Project Management、Project Runtime Environment、Task Runner Service、Task Process Log、Task Runner AskUser Callback、ChatOS AskUser Callback、ChatOS Memory Readers、ChatOS Notepad 与 ChatOS Agent Builder；Embedded Provider 已先接入无状态 WebTools，并复用共享 WebTools 实现与公共 URL 安全策略。Notepad 固定使用 Runtime Session 绑定 owner 的 ChatOS 云端用户 Store，Agent Builder 固定使用同一 owner 的 ChatOS Agent Store；两者都不在 MCP Management 主进程或 Local Connector 创建第二份数据。BrowserTools 仍需要受控 Browser Runtime，因此不在 MCP Management 主进程中临时创建本地替代实现。

### Phase 3：Workspace Router

- 实现 LocalConnectorProvider。
- 实现 HarnessProvider。
- 实现 CloudSandboxProvider。
- 迁移 CodeMaintainerRead/Write、Git 和 Terminal。

当前已完成三类 Workspace 真实路由：本地 Workspace 的 CodeMaintainerRead、CodeMaintainerWrite、TerminalController、BrowserTools 经 Local Connector Service 的 `/mcp` relay 到客户端；云端 Harness Workspace 的 CodeMaintainerRead/Write 经 Project Service 的 project-scoped Harness MCP；Cloud Sandbox 的 CodeMaintainerRead、CodeMaintainerWrite、TerminalController 经 Sandbox Manager 的标准 MCP proxy 到已创建的 Sandbox 或 Environment Service。

Cloud Sandbox Runtime Session 只接收 `sandbox_id`、`lease_id`、`is_environment` 和可选 `service_id`，不接收任意 Provider URL。MCP Management 在签发 Session 前通过专用服务身份向 Sandbox Manager 校验 lease、owner、project、run 和 service 绑定，并把目标写入 `route_revision`；每次调用前再次验证 lease 状态。Sandbox Manager 对来自 MCP Management 的 proxy 请求重复校验同一绑定。Provider 故障或绑定漂移均直接失败，不切换到 Harness、本地 Connector 或其他 Sandbox。

验收：

- 同一个文件读取 MCP 在 Local Project 走 Local Connector，在 Cloud Project 走 Harness/Sandbox。

### Phase 4：Agent 内部工具 Provider 统一

- Task Runner Service MCP 接入。
- Project Management MCP 接入。
- Project Runtime Environment MCP 接入。
- Memory 能力接入标准 MCP Provider。
- Sandbox Images 接入。
- ChatOS/Task Runner 删除这些 Agent 工具对应的直接客户端和 URL 拼接。

当前 Project Management MCP、Project Runtime Environment MCP、Task Runner Service MCP、Task Process Log MCP、Task Runner AskUser MCP 和 ChatOS AskUser MCP 已使用 `mcp-management-service` 专用内部身份真实调用，不转发用户 Token。AskUser 不再使用泛化的运行时猜测：创建 Runtime Session 时会按 Agent 宿主固定 `provider_ref`，Task Runner 四个 phase Agent 固定到 `task-runner`，ChatOS Agent 固定到 `chatos`；尚未注册宿主的 Agent fail closed，Session 内不得切换宿主。Task Runner 和 ChatOS 都只开放 `/internal/mcp-management/mcp/{system_key}` 这个工具入口给该身份，不会把普通 REST 纳入网关。Task Process Log 与 Task Runner AskUser 的 owner、Agent、MCP Session、Session 到期时间、Project、run 和 task 全部取自 Runtime Session Snapshot；Task Runner 会再次验证 run 属于 task且仍在运行、task 属于 owner 与 Project、run 是 task 当前 run、Agent phase 与 task 类型一致。ChatOS AskUser 会再次验证 conversation 属于 owner、Project 和 active 状态，绑定的 source user message 位于该 conversation 且属于同一 turn。两端都拒绝模型参数覆盖这些字段，AskUser 的实际提示等待时间不得越过不可变 Session 的剩余生命周期，并预留继续执行所需的安全窗口。Task Runner Service 根据绑定的 System Agent 身份收窄工具 Profile。

Sandbox Images 已接入同一聚合调用入口。Cloud Project 固定为 `sandbox-images:cloud`，由 MCP Management 使用专用内部身份调用 Sandbox Manager 的 `/api/sandbox-images/mcp`；Local Project 固定为 `sandbox-images:local:<pairing_id>`，经 Local Connector Service 的精确 sandbox facade 路径转发到客户端 `/api/local/sandbox/images/mcp`。该 Provider 在镜像创建前不要求已有 Sandbox lease，避免“创建运行镜像却必须先创建 Sandbox”的循环依赖；本地路由必须使用 Project Execution Context 中的权威 `sandbox_pairing_id`。路由固定不支持取消，长耗时 `create_image` 使用工具声明等待时间加传输安全窗口，且失败后不在 cloud/local 间自动切换。

Memory Skill/Command/Plugin Reader 的权威数据实际属于 ChatOS contact-agent runtime，而不是 Memory Engine 微服务，因此已把此前的 `memory-engine` 占位路由纠正为 ChatOS Provider。Runtime Session 会冻结 `contact_agent_id`，路由固定为 `chatos:memory:<contact_agent_id>`，该身份同时写入 Runtime Grant；ChatOS 内部 MCP Provider 再次验证 source session 的 owner、Project、active 状态以及 contact agent 的 owner。模型只能提交 `skill_ref`、`command_ref` 或 `plugin_ref`，不能覆盖 contact agent。Provider 故障或身份漂移直接失败，不回退 ChatOS 旧内嵌工具链。

Notepad 已从不可执行的 Embedded 占位路由迁移为固定 `InternalService/chatos` 路由。本地与云端 Project 都使用 ChatOS 的 owner-scoped 云端 Notepad Store；MCP Management 只转发 Runtime Session 中冻结的 `owner_user_id`，ChatOS 拒绝工具参数中的 `owner_user_id`/`user_id` 覆盖。ChatOS 三类运行 Agent 和 Task Runner 四个 phase Agent 均可配置该 MCP，数据不按 Workspace 复制到 Local Connector 或 Task Runner 本地目录。

Agent Builder 已补齐同一 ChatOS Internal Service Provider。调用只允许 ChatOS 三类运行 Agent，并要求 source session 与 Runtime Session 的 owner、Project 和 active 状态一致；创建 Agent 时 Store 强制写入绑定 owner，工具 Schema 不再暴露 `user_id`，更新 Agent 时入口与 Store 都验证目标 Agent owner，跨用户更新直接拒绝。该 MCP 保持按 Plugin Management 显式配置，不自动加入 Task Runner 默认工具集。

Task Runner 与 ChatOS 调用方均已接入 Runtime Session 解析：`shadow` 模式只观测路由解析结果并继续使用旧工具链，`gateway` 模式只连接 MCP Management endpoint，调用失败时不回退旧 Provider。部署默认仍保持 `shadow`；Memory Agent 调用方迁移和旧直连清理尚未开始。

### Phase 5：External 与 Plugin Runtime

- External HTTP MCP。
- Cloud stdio MCP through Sandbox。
- Plugin cloud/local/portable MCP。
- Skill executable、Command 和 Agent component 聚合。

当前已完成 External HTTP MCP 与普通 `stdio_cloud` MCP 主链路。External HTTP 的私有 Binding 固定 resource/provider_ref、HTTPS endpoint、DNS 公网解析结果、认证 Header、读写标记和工具白/黑名单，并以无代理、无重定向、响应限额和 JSON-RPC 校验执行。Cloud stdio 的私有 Binding 固定 command/args/env/cwd、sandbox target、读写标记和工具策略；MCP Management 不 spawn，而是通过 Sandbox Manager 的专用内部接口把配置送到绑定 lease 的 Sandbox Agent。Agent 使用清空 Host 环境的受控 wrapper 托管持久 stdio 进程，并用 Session TTL、显式 close 和调用失败清理进程树。Runtime Session 创建阶段会在 Sandbox 内真实 `tools/list`，live Schema 参与聚合工具快照和 `route_revision`；Plugin Management 已停止在自身进程探测 `stdio_cloud`。required 配置、lease、进程启动或 Schema 探测失败时 Session fail closed，optional 资源标记 unavailable。`shadow` 调用方解析完成后会显式关闭观测 Session，默认部署模式仍保持 `shadow`。Plugin OAuth 动态凭据解析和 release-managed Plugin cloud/local/portable 的完整执行链仍待后续阶段。

### Phase 6：调用方迁移

顺序：

1. Task Runner。
2. ChatOS Conversation/Plan。
3. Project Environment Agent。
4. Memory Agent。
5. 其他 Agent 宿主。

每个调用方只配置一个 MCP Management endpoint。

Task Runner 当前迁移开关 `TASK_RUNNER_MCP_MANAGEMENT_MODE`，ChatOS 当前迁移开关 `CHATOS_MCP_MANAGEMENT_MODE`：

- `off`：仅使用旧 MCP builder。
- `shadow`：解析并记录 MCP Management Runtime Session，但工具调用仍走旧链路；2.0.10 部署默认值。
- `gateway`：模型只看到 MCP Management 聚合 endpoint；Session 解析或调用失败直接失败，不静默回退。

只有 Agent Tool Plane 使用该 endpoint；调用方的普通 REST、事件、队列和内部 SDK 链路不受此迁移影响。

### Phase 7：删除重复路由

- 删除 TaskRunnerSystemMcpAdapter。
- 删除 LocalConnectorSystemMcpAdapter 中的业务路由职责。
- 删除 ChatOS 独立 builtin MCP Factory。
- 删除各服务 local/cloud workspace if/else。
- SystemMcpHost.supported_hosts 降级为兼容字段或删除。

---

## 20. 与云端回迁方案的组合顺序

新的推荐顺序：

~~~text
MCP Management Phase 0/1
  -> 恢复云端 Project/Session/Chat 数据面
  -> MCP Management Workspace Router
  -> 云端 Chat/Task Runner 使用聚合 MCP
  -> 停止客户端 Local Agent/Worker
  -> 迁移剩余内部微服务 MCP
  -> 删除客户端本地业务 Runtime
  -> 删除各服务重复 MCP 路由
~~~

这样恢复云端编排时，不需要先把旧的 Task Runner Local Connector routing 再复制回各服务，然后过一版又迁移到新网关。

应直接把 Local Connector routing 恢复到 MCP Management Service。

---

## 21. 首批代码修改区域

新增：

- crates/chatos_mcp_management_sdk/
- mcp_management_service/backend/
- docker compose 中 mcp-management-service。

修改：

- Cargo.toml workspace members。
- mcp/ 增加路由分类所需的稳定 metadata，但不加入环境判断。
- chatos_plugin_management_sdk 如需补 Runtime Session DTO，只做兼容扩展。
- Plugin Management 增加 MCP Management caller scope。
- Task Runner 与 ChatOS 已增加 MCP Management client，并分别以 `shadow`/`gateway` 开关逐步替换旧工具 builder。
- Local Connector Service 增加 MCP Management caller token。
- Project Context owner 增加内部 execution-context endpoint。

暂不删除：

- TaskRunnerSystemMcpAdapter。
- LocalConnectorSystemMcpAdapter。
- ChatOS builtin factory。
- 现有 Project Management /mcp。
- 现有 Local Connector relay。

先并行运行，完成调用方切换后再清理。

---

## 22. 测试矩阵

### 22.1 Route Engine

- local_connector + CodeMaintainerRead -> LocalConnector。
- harness + CodeMaintainerRead -> Harness。
- cloud_sandbox + CodeMaintainerRead -> CloudSandbox。
- local_connector + LocalCommandApproval -> LocalConnector。
- harness + LocalCommandApproval -> unavailable。
- ProjectManagement 在所有项目类型 -> InternalService。
- portable Plugin 固定 host 后不再切换。
- required MCP 无路由 -> Session resolve 失败。

### 22.2 Capability Materializer

- disabled binding 不进入 Session。
- enabled + offline 仍进入 Session。
- required + offline 但有合法 route：按 Provider 健康策略决定阻断。
- resource.disabled 不进入 Session。
- 配置更新导致 policy_revision 和 route_revision 变化。

### 22.3 Namespace

- 同名工具不会冲突。
- Schema 和 call 使用同一 snapshot。
- 旧别名只在兼容期开启。
- 不能通过伪造前缀调用未授权 MCP。

### 22.4 E2E

1. Agent 配置文件读取 MCP，本地项目真实读取本机文件。
2. 同一 Agent、同一 MCP，在云端项目读取 Harness 文件。
3. Agent 配置 Project Management MCP，真实创建 Requirement。
4. Agent 配置 Task Runner MCP，真实创建 Task。
5. Agent 配置 Local Plugin MCP，调用经过 Connector。
6. Agent 配置 External HTTP MCP，调用经过 SSRF 防护。
7. Connector 中途离线，不切 Cloud Provider。
8. Runtime Session 过期后调用被拒绝。
9. 取消命令传播到本地进程。
10. Project owner 不一致时路由解析失败。

---

## 23. 验收标准

1. Agent Runtime 只连接一个聚合 MCP Server。
2. Plugin Management 中启用的 MCP 都进入 Runtime Session。
3. 各 Agent 宿主不再维护独立 MCP 可用性 allowlist。
4. Local/Cloud/Harness/Sandbox 路由集中在 MCP Management Service。
5. 文件读取示例可按 Project Context 正确选择 Provider。
6. Route 在一个 Runtime Session 内保持固定。
7. required MCP 无合法 route 时明确阻断。
8. 本地绝对路径和凭据不进入云端 Route Snapshot。
9. 内部服务 MCP、外部 MCP、Local MCP 和 Plugin MCP 使用统一调用审计。
10. Task Runner、ChatOS、Project Management Agent 可逐个迁移，不要求一次切完。
11. 新服务不可用时明确失败，不绕过新服务回退到旧直连。
12. 完成迁移后，可删除各服务重复 MCP builder 和 host routing。

---

## 24. 关键设计决策

### 决策一：Plugin Management 不负责执行

Plugin Management 继续管理配置，不代理 tools/call。这样 Catalog、发布、偏好和运行流量不会耦合在同一个服务。

### 决策二：新服务同时包含管理聚合和运行网关

如果只做 Catalog Service，各宿主仍然会重复写路由。如果只做 Proxy，又无法保证 Agent binding 真正生效。因此控制面聚合和运行路由必须在同一服务中，但代码模块和数据职责分开。

### 决策三：Project source 与 Agent execution 解耦

Agent 始终在云端；workspace_provider 决定文件和命令走本地还是云端。

### 决策四：配置状态与健康状态解耦

配置了就是 Runtime Session 的一部分。Provider 健康异常不能让 MCP 从目录中静默消失。

### 决策五：不允许执行中隐式 fallback

安全边界、数据位置和审批方式会随 Provider 改变，任何切换都必须创建新 Session 并产生新 route_revision。

---

## 25. 工作量判断

新增统一 MCP 服务后，整体改造规模从原方案的中等提升为中大型，但长期收益更高。

建议单人估算：

- Phase 0/1 合同、服务骨架和能力解析：4～6 天。
- 聚合 MCP Server 和内部 Provider：4～7 天。
- Local/Harness/Cloud Sandbox Workspace Router：5～8 天。
- Task Runner 与 ChatOS 首批迁移：5～8 天。
- Plugin/External MCP、取消、审计和安全加固：5～10 天。
- 其他服务迁移与旧路由清理：5～10 天。

完整落地约 4～7 周。可以先在 2.0.10 完成：

1. 新服务 Phase 0/1。
2. 文件读取的 Local Connector/Harness 双路由闭环。
3. Task Runner 或 ChatOS 其中一个调用方接入。

完成这个最小闭环后，再逐项迁移其他 MCP 和服务。
