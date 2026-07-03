# Chatos RS 服务横向扩展与沙箱接入治理方案

## 结论

当前项目已经具备多服务雏形：`chat_app_server_rs` 负责主编排，`user_service` 负责统一身份，`task_runner_service` 负责任务执行，`memory_engine` 负责长期记忆，`project_management_service` 负责项目管理，`sandbox_manager_service` 负责沙箱生命周期。

但服务间调用仍主要依赖固定 `base_url + reqwest + 少量独立 secret`。这种方式在本地单机很好用，到了横向集群会遇到服务发现、实例调度、重复执行、回调认证分散、状态粘在单实例、沙箱容量只在进程内计数等问题。

沙箱安全侧的问题更直接：`sandbox_manager_service` 的管理 API 目前没有像 `memory_engine` 一样的接入系统管理模块；任何能访问 `8095` 的调用方都可以创建、释放、查询、代理调用沙箱。沙箱内 `sandbox_mcp_server` 支持可选 token，但管理服务当前没有给租约签发和注入短期 token，Task Runner 调用 Sandbox Manager 时也没有携带认证 header。

建议建设两条主线：

1. 服务间调用统一成“服务注册 + 受众 token + 发现/负载均衡 + 幂等 + 队列化执行”的平台能力。
2. `sandbox_manager_service` 新增 `Sandbox Access Control Plane`，复用 `memory_engine` 的 source/system 接入思想，做到系统身份可注册、密钥可轮换、权限可收敛、租约可审计、沙箱 agent 只接受短期租约 token。

## 当前项目观察

### 已有可复用基础

- `memory_engine` 已有较成熟的接入模型：
  - `memory_engine/backend/src/api/memory_auth.rs` 支持 Bearer token 通过 `user_service` 验证，也支持 operator token。
  - `memory_engine/backend/src/api/sdk_api/auth.rs` 支持 `x-memory-system-id` 和 `x-memory-system-key`，通过已注册 source 校验系统身份。
  - `memory_engine/backend/src/repositories/sources/` 已有 source 注册、密钥 hash、rotate key、active/retired 判断。
  - `memory_engine/sdk` 已有 `new_direct`、`new_platform`、`new_system` 三种客户端模式。
- `user_service` 已经是统一身份服务，负责 human user、agent account、Task Runner delegation token，以及下游 model config 同步。
- `task_runner_service` 已经有沙箱运行时接入：
  - `task_runner_service/backend/src/services/sandbox_runtime.rs` 会按配置向 Sandbox Manager 申请 lease。
  - `SandboxRuntimeContext` 会把 `sandbox_id`、`lease_id`、`task_id`、`run_id` 等上下文传给 MCP server。
- `task_runner_service` Mongo store 已有部分集群保护：
  - `task_runner_service/backend/src/store/mongo/setup.rs` 给 active task run 建了 partial unique index，限制同一 task 只能存在一个 queued/running run。

### 主要缺口

- `sandbox_manager_service/backend/src/api/router.rs` 没有认证 middleware，所有 `/api/sandboxes/*`、`/api/sandbox-images/*`、`/api/sandbox-pool/status` 都是裸露 API。
- `sandbox_manager_service/backend/src/service/manager.rs` 的 lease 请求虽然带 `tenant_id/user_id/project_id/run_id`，但这些字段完全相信调用方，没有认证上下文校验。
- `sandbox_manager_service/backend/src/pool/mod.rs` 是进程内 `AtomicUsize` 池。多实例部署时每个实例各算各的容量，无法形成全局容量控制。
- Docker/Kata backend 会把 agent 端口发布到 `127.0.0.1` 动态端口，但管理服务没有统一的 agent token 签发、注入、代理和审计闭环。
- `task_runner_service` 的 run 启动仍是在 API 实例内 `tokio::spawn` 执行；调度器也是每个实例启动后循环扫描。多实例下需要从“请求即执行”改成“请求入队，worker 原子 claim 后执行”。
- 根启动脚本和 `.env.example` 面向本地固定端口。生产集群需要服务发现、网关、统一认证和事件总线。

## 目标架构

```text
Client / Frontend
  -> API Gateway / Ingress
    -> chat_app_server_rs
    -> user_service
    -> project_management_service
    -> task_runner_service API
    -> memory_engine API
    -> sandbox_manager_service API

Internal service calls
  -> shared service client
  -> service discovery / DNS / mesh
  -> user_service-issued service token or registered system credentials
  -> idempotency key + trace context

Async execution
  -> durable queue / DB claim table
  -> task_runner worker replicas
  -> memory worker replicas
  -> sandbox scheduler / node agents

Sandbox data plane
  -> sandbox_manager_service proxy endpoint
  -> per-lease short-lived agent token
  -> sandbox_agent inside Docker/Kata/microVM
```

核心原则：

- 生产环境所有服务 API 默认 fail-closed，除 `/health` 外都需要认证或内网策略。
- 服务间调用不再散落自定义 secret header，统一使用 `user_service` 签发的 service token，或注册制 system id/key 作为 bootstrap。
- API 实例无状态化；长期执行、调度、沙箱容量、租约归属都落到共享存储或队列。
- 任何进程内 lock、broadcast、Atomic 只能作为性能优化，不能作为集群正确性边界。
- 沙箱 agent 不直接暴露给普通调用方，默认通过 Sandbox Manager proxy 调用。

## 服务间调用横向扩展方案

### 1. 统一服务注册与发现

新增统一配置模型：

```env
CHATOS_SERVICE_DISCOVERY_MODE=static|dns|kubernetes|consul
CHATOS_SERVICE_ID=task_runner_service
CHATOS_INSTANCE_ID=task-runner-01
CHATOS_INTERNAL_TOKEN_AUDIENCE=task_runner_service
CHATOS_USER_SERVICE_BASE_URL=http://user-service:39190
CHATOS_MEMORY_ENGINE_BASE_URL=http://memory-engine:7081/api/memory-engine/v1
CHATOS_TASK_RUNNER_BASE_URL=http://task-runner:39090
CHATOS_PROJECT_SERVICE_BASE_URL=http://project-service:39210
CHATOS_SANDBOX_MANAGER_BASE_URL=http://sandbox-manager:8095
```

本地开发继续支持 static URL。生产环境优先使用 Kubernetes Service DNS 或服务网格：

```text
http://user-service:39190
http://memory-engine:7081
http://task-runner-api:39090
http://sandbox-manager:8095
```

### 2. 统一内部 HTTP client

新增共享 crate：

```text
crates/chatos_service_client/
```

能力：

- 标准 timeout、connect timeout、body size limit。
- 只对幂等请求重试，指数退避加 jitter。
- 熔断和快速失败。
- 自动附加：
  - `traceparent`
  - `x-request-id`
  - `x-idempotency-key`
  - `x-chatos-caller-service`
  - `Authorization: Bearer <service_token>`
- 统一错误体截断和结构化日志。

现有 `memory_engine/sdk/src/client/transport.rs` 里的错误格式化方式可以迁移为公共实现。

### 3. 统一服务身份

推荐两层身份：

- 人/Agent 令牌：由 `user_service` 签发，带 `principal_type`、`owner_user_id`、`role`、`aud`。
- 服务令牌：由 `user_service` 签发，带 `service_id`、`aud`、`scopes`、短 TTL。

新增或扩展 User Service API：

```text
POST /api/token/exchange/service
```

请求方用已注册的 system id/key 或 operator bootstrap secret 换取短期 service token。之后所有服务间调用都用 Bearer token，不再各服务自定义一套 sync secret。

过渡期保留：

- `x-memory-operator-token`
- `x-chatos-callback-secret`
- `PROJECT_SERVICE_SYNC_SECRET`

但新代码只新增统一 service token 路径。

### 4. Task Runner 从请求内执行改为 durable queue

当前 `start_run` 会保存 `queued` run 后在当前 API 实例 `tokio::spawn`。横向扩展建议改为：

1. API 只负责校验、幂等创建 `task_runs`，状态为 `queued`。
2. Worker replica 扫描或订阅 queued run。
3. 使用原子 claim：

```text
status = queued
lease_until < now or lease_until missing
findOneAndUpdate -> status=running, worker_id, lease_until, started_at
```

4. Worker 执行期间定期 heartbeat 延长 `lease_until`。
5. Worker 崩溃后，另一个 worker 可在 `lease_until` 过期后接管或标记失败。

Mongo MVP 可以先用 `findOneAndUpdate` 实现；后续可切 NATS JetStream、Redis Streams 或 Kafka。关键是 API replica 不再承载长任务。

SQLite 和 in-memory store 仅保留本地开发，不作为集群模式。

#### 多节点防打架规则

两个 worker 节点同时拉任务时，必须只允许一个节点成功进入执行区。这里不能使用“先查 `has_active_run_for_task`，再更新状态”的两步逻辑，因为两个节点可能同时查到可执行。

MVP 用 Mongo 原子更新作为唯一入口：

```text
findOneAndUpdate(
  {
    id: run_id,
    status: "queued",
    "$or": [
      { "claim_until": { "$exists": false } },
      { "claim_until": { "$lte": now } }
    ]
  },
  {
    "$set": {
      status: "running",
      worker_id: current_worker_id,
      claim_token: new_uuid,
      claim_until: now + claim_ttl,
      started_at: now,
      updated_at: now
    },
    "$inc": { "attempt": 1 }
  },
  return_document=After
)
```

只有拿到更新后文档的 worker 才能执行；其他 worker 拿到空结果就必须跳过。执行期间所有 heartbeat、event append、最终成功/失败写回都必须带上 `run_id + claim_token` 条件：

```text
updateOne(
  { id: run_id, claim_token: current_claim_token, worker_id: current_worker_id },
  { "$set": { claim_until: now + claim_ttl, updated_at: now } }
)
```

这样可以防止旧 worker 卡顿后恢复，把已经被新 worker 接管的 run 写成成功或失败。这个 `claim_token` 就是 fencing token。

同一个 task 的 active run 继续保留 Mongo partial unique index：

```text
unique(task_id) where status in ["queued", "running"]
```

它负责防止两个 API 实例同时为同一个 task 创建 active run；worker claim 负责防止两个 worker 同时执行同一个 run。两层都要有。

执行完成时也必须用 fencing 条件：

```text
updateOne(
  {
    id: run_id,
    status: "running",
    claim_token: current_claim_token
  },
  {
    "$set": {
      status: "succeeded" | "failed" | "cancelled",
      finished_at: now,
      claim_until: null,
      updated_at: now
    }
  }
)
```

如果更新影响行数是 0，说明这个 worker 已经失去所有权，必须停止写结果，只能记录本地 warning。

### 5. Scheduler 单点循环改为分布式 claim

当前每个 Task Runner 实例都会 `spawn_task_scheduler`。建议：

- 短期：加 `TASK_RUNNER_SCHEDULER_ENABLED`，生产只开一个 scheduler replica。
- 中期：scheduler 也使用 DB claim，按 task schedule 原子更新 `next_run_at` 和 `scheduler_claim_id`。
- 长期：把 scheduler 事件写入队列，由 worker 消费。

定时任务也不能用“每个节点扫描 due task 后各自 start”的方式。中期实现应使用类似条件：

```text
findOneAndUpdate(
  {
    "schedule.next_run_at": { "$lte": now },
    "schedule.claim_until": { "$lte": now },
    status: { "$in": ["ready", "failed", "succeeded"] }
  },
  {
    "$set": {
      "schedule.claimed_by": scheduler_instance_id,
      "schedule.claim_token": new_uuid,
      "schedule.claim_until": now + claim_ttl
    }
  }
)
```

只有 claim 成功的 scheduler 才能创建 queued run，并在同一个流程里推进 `next_run_at`。如果创建 run 失败，必须释放或过期该 scheduler claim。

### 6. 事件流与回调

`stream_run_events`、进程内 broadcast、WebSocket/SSE 不能依赖单实例内存。

建议：

- MVP：前端 SSE 通过 Mongo 轮询或 change stream 查询 run events。
- 生产：NATS/Redis pubsub 广播 run event，API replica 只做订阅转发。
- 负载均衡短期可开启 sticky session，但不能作为最终正确性依赖。

### 7. 幂等与重试

所有跨服务写请求必须支持幂等：

- `POST /api/tasks/:id/runs`：幂等键为 `caller_service + task_id + request_id`。
- `POST /api/sandboxes/leases`：幂等键为 `tenant_id + run_id + requester_client_id`。
- `POST /release`：幂等键为 `lease_id + release_request_id`。

响应可以返回已存在资源，而不是重复创建。

## Sandbox Access Control Plane

### 目标

为 `sandbox_manager_service` 新增类似 `memory_engine` source/system 的接入治理模块：

- 谁可以调用 Sandbox Manager。
- 调用方能创建什么类型沙箱。
- 能操作哪些 tenant/project/run。
- 能调用哪些 sandbox MCP tool。
- 每个调用方的并发、TTL、资源上限。
- 每个租约、每次 tool call 都有审计记录。

### 新增核心模型

```rust
pub struct SandboxAccessClient {
    pub id: String,
    pub tenant_id: Option<String>,
    pub client_id: String,
    pub client_type: String, // task_runner | chatos_backend | admin_console | sdk_system
    pub name: String,
    pub description: Option<String>,
    pub status: String, // active | disabled | retired
    pub scopes: Vec<String>,
    pub allowed_tenant_ids: Vec<String>,
    pub allowed_project_ids: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub max_lease_ttl_seconds: u64,
    pub max_active_leases: u64,
    pub resource_limits: ResourceLimits,
    pub network_policy: NetworkPolicy,
    pub secret_key_hash: Option<String>,
    pub secret_key_hint: Option<String>,
    pub key_last_rotated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

建议 scopes：

```text
sandbox.lease.create
sandbox.lease.read
sandbox.lease.release
sandbox.lease.destroy
sandbox.mcp.tools
sandbox.mcp.call
sandbox.images.read
sandbox.images.write
sandbox.pool.read
sandbox.admin
```

新增集合：

```text
sandbox_access_clients
sandbox_access_audit_events
sandbox_lease_idempotency
sandbox_nodes
```

### 认证模式

仿照 `memory_engine`，提供三种入口：

1. User token
   - `Authorization: Bearer <user_service_jwt>`
   - Sandbox Manager 调 `user_service /api/auth/verify`
   - 用于管理台、超级管理员、用户查看自己的租约。

2. System key
   - `x-sandbox-client-id`
   - `x-sandbox-client-key`
   - 对应已注册 `SandboxAccessClient`
   - 用于 Task Runner、ChatOS backend 等服务间调用。

3. Operator token
   - `x-sandbox-operator-token`
   - 只用于 bootstrap、注册 client、rotate key、紧急运维。
   - 不建议作为 Task Runner 的长期调用方式。

新增模块建议：

```text
sandbox_manager_service/backend/src/auth.rs
sandbox_manager_service/backend/src/repositories/access_clients.rs
sandbox_manager_service/backend/src/api/access_clients.rs
```

中间件：

```rust
pub enum SandboxAuthContext {
    User(VerifiedPrincipal),
    System(SandboxAccessClient),
    Operator,
}
```

### API 保护矩阵

```text
GET  /health                                      public
GET  /api/system/config                           admin or operator, sensitive fields redacted
GET  /api/sandbox-pool/status                     sandbox.pool.read or admin
GET  /api/sandbox-images                          sandbox.images.read
POST /api/sandbox-images/initialize               sandbox.images.write or admin
POST /api/sandboxes/leases                        sandbox.lease.create, optional x-idempotency-key
GET  /api/sandboxes                               sandbox.lease.read, scoped by auth
GET  /api/sandboxes/:sandbox_id                   sandbox.lease.read, lease scope required
POST /api/sandboxes/:sandbox_id/heartbeat         sandbox.lease.create or lease owner
GET  /api/sandboxes/:sandbox_id/health            sandbox.lease.read, lease scope required
GET  /api/sandboxes/:sandbox_id/mcp/tools         sandbox.mcp.tools, lease scope required
POST /api/sandboxes/:sandbox_id/mcp               sandbox.mcp.tools or sandbox.mcp.call, lease scope + tool policy
POST /api/sandboxes/:sandbox_id/mcp/call          sandbox.mcp.call, lease scope + tool policy
POST /api/sandboxes/:sandbox_id/release           sandbox.lease.release, lease scope required
DELETE /api/sandboxes/:sandbox_id                 sandbox.lease.destroy or admin
GET  /api/sandboxes/:sandbox_id/events            sandbox.lease.read, lease scope required
```

### 租约创建校验

`CreateSandboxLeaseRequest` 当前包含 `tenant_id/user_id/project_id/run_id`。改造后规则：

- `tenant_id`：
  - User token：默认从 `owner_user_id` 派生，普通用户不能覆盖。
  - System key：必须在 `allowed_tenant_ids` 内，或 client 是 global system。
  - Operator：可覆盖。
- `user_id`：
  - User token：从 principal 派生。
  - Task Runner system：可以传 task owner，但必须有 task/run 所属上下文或允许范围。
- `project_id/run_id`：
  - 必填。
  - `run_id` 建唯一幂等约束：`requester_client_id + tenant_id + run_id`。
- `tools`：
  - 必须是 `allowed_tools` 子集。
- `ttl_seconds/resource_limits/network`：
  - 不能超过 client policy。
  - `network.mode=host`、`privileged`、挂载 Docker socket 永久禁止。

### 沙箱 agent 短期 token

当前 `sandbox_mcp_server` 支持 `CHATOS_SANDBOX_MCP_TOKEN`，但管理服务没有闭环使用。建议：

1. 创建 lease 时生成随机 `agent_token`。
2. lease 记录只保存 `agent_token_hash` 和 `agent_token_hint`。
3. Docker/Kata backend 启动容器时注入：

```text
CHATOS_SANDBOX_MCP_TOKEN=<lease_agent_token>
CHATOS_TENANT_ID=<tenant_id>
CHATOS_USER_ID=<user_id>
CHATOS_PROJECT_ID=<project_id>
CHATOS_RUN_ID=<run_id>
CHATOS_SANDBOX_ID=<sandbox_id>
CHATOS_SANDBOX_LEASE_ID=<lease_id>
```

4. Sandbox Manager 调 agent 时使用 Bearer token 或 `x-chatos-sandbox-token`。
5. Task Runner 默认不拿 agent token，不直接打 agent endpoint。
6. release/destroy 后 token 立即失效。

### MCP 调用代理

Task Runner 的 `mcp_url` 已改成 Sandbox Manager proxy：

```text
http://sandbox-manager:8095/api/sandboxes/{sandbox_id}/mcp
```

Task Runner 调用 proxy 时携带 system client 凭证和 lease headers。Sandbox Manager 完成：

1. 校验调用方身份。
2. 校验 lease scope。
3. 校验工具名是否在 lease/tools/client policy 内。
4. 附加 agent token，转发到 sandbox agent `/mcp`。
5. 写审计事件，包括 tool name、run_id、status、耗时、截断后的错误。

这样即便 agent 动态端口泄漏，也缺少短期 token；即便某个系统拿到 sandbox id，也不能越权调用别人的 lease。

### Sandbox Manager SDK

新增：

```text
crates/sandbox_manager_sdk/
```

模式仿照 Memory Engine：

```rust
SandboxManagerClient::new_system(base_url, timeout, client_id, secret_key)
SandboxManagerClient::new_user(base_url, timeout).with_bearer_token(token)
SandboxManagerClient::new_operator(base_url, timeout).with_operator_token(token)
```

Task Runner 不再手写 reqwest 调 Sandbox Manager，而是使用 SDK。

## Sandbox Manager 横向扩展

### 当前问题

`SandboxPool` 是单进程 Atomic 计数；Docker/Kata 容器由接到请求的实例在本机创建。多个 Sandbox Manager 实例后会出现：

- 全局容量失真。
- A 实例创建的容器，B 实例可能无法本地 inspect/destroy。
- agent endpoint 是节点本地动态端口，跨节点不可达。

### 目标拆分

```text
sandbox-manager-api
  - 无状态 API
  - 认证、租约、审计、调度
  - 写 Mongo/Postgres

sandbox-node-agent
  - 每个可运行沙箱的节点一个
  - 负责本节点 Docker/Kata/microVM
  - 上报 capacity 和 heartbeat
  - 接收 create/start/stop/destroy 指令

sandbox-agent
  - 每个沙箱内部一个
  - 提供文件/终端 MCP
```

新增 `sandbox_nodes`：

```text
node_id
zone
backend_kind
capacity_cpu
capacity_memory_mb
max_active
active_count
status
last_heartbeat_at
node_agent_endpoint
```

lease 记录新增：

```text
requester_client_id
node_id
agent_token_hash
placement_status
idempotency_key
lease_owner_service
```

调度流程：

1. API 验证请求。
2. 使用 DB 事务或原子 update 选择有容量节点。
3. 写入 lease `status=leasing,node_id=...`。
4. 调 node agent 创建容器。
5. node agent 返回 agent endpoint。
6. API 更新 lease `status=ready`。

MVP 如果暂不拆 node agent，至少要在 lease 记录里写入 `manager_instance_id`，并让 destroy/release 请求路由到 owner instance 或通过共享远程 Docker/Kata API 操作。

## 数据模型和索引建议

### sandbox_access_clients

```text
unique: tenant_id + client_id
index: status
index: client_type
```

### sandbox_leases

新增索引：

```text
unique: sandbox_id
unique partial: requester_client_id + tenant_id + run_id where status in active statuses
index: tenant_id + user_id + created_at
index: tenant_id + project_id + created_at
index: node_id + status
index: status + expires_at
```

### sandbox_access_audit_events

```text
id
event_type
requester_client_id
principal_type
tenant_id
user_id
project_id
run_id
sandbox_id
lease_id
tool_name
decision // allowed | denied
reason
latency_ms
created_at
```

## 分阶段实施路线

### P0：立即补安全边界

目标：不能再让任意系统裸调 Sandbox Manager。

任务：

- `sandbox_manager_service/backend/src/config.rs`
  - 新增 `SANDBOX_MANAGER_REQUIRE_AUTH=true`
  - 新增 `SANDBOX_MANAGER_OPERATOR_TOKEN`
  - 新增 `SANDBOX_MANAGER_USER_SERVICE_BASE_URL`
  - 新增 `SANDBOX_MANAGER_SYSTEM_CLIENT_BOOTSTRAP_ENABLED`
- 新增 `auth.rs`
  - 实现 `SandboxAuthContext`
  - 支持 Bearer user token、system id/key、operator token。
- `api/router.rs`
  - 除 `/health` 外加认证 middleware。
- `models.rs` / `store/mod.rs`
  - 新增 access client、audit event、agent token hash 字段。
- `service/manager.rs`
  - 所有 lease get/list/release/destroy/mcp 调用都校验 scope。
  - `list` 默认按 auth scope 自动过滤。
- Docker/Kata backend
  - 创建容器时注入 `CHATOS_SANDBOX_MCP_TOKEN` 和租约上下文 env。
- `task_runner_service/backend/src/services/sandbox_runtime.rs`
  - SandboxManagerClient 添加 system auth header。
  - `mcp_url` 优先使用 Sandbox Manager proxy。

### P1：集群可运行

目标：服务可以横向扩 API 副本，不重复执行任务，不丢事件。

任务：

- 新增 `crates/chatos_service_client`。
- Task Runner API 与 worker 分离：
  - `TASK_RUNNER_ROLE=api|worker|scheduler|all`
  - API 只入队。
  - Worker 原子 claim queued runs。
- Scheduler 改成单独 role 或 DB claim。
- Sandbox pool 从进程内 Atomic 改成 DB-backed capacity。
- 所有写 API 增加 `x-idempotency-key`。
- run events 改为 DB polling/change stream 或事件总线。

### P2：生产级沙箱集群

目标：沙箱可以跨节点调度，Docker/Kata/microVM 后端可扩。

任务：

- 拆 `sandbox-node-agent`。
- `sandbox_manager_service` 只做控制面和代理。
- node agent 上报 heartbeat/capacity。
- lease placement 支持 node selection。
- agent endpoint 走内网服务名或 node agent tunnel，不暴露宿主动态端口。
- 引入 NATS JetStream 或 Redis Streams 承接 run、sandbox create、memory jobs。
- 服务网格或 mTLS 加固东西向流量。

## 建议的新环境变量

```env
# shared
CHATOS_SERVICE_DISCOVERY_MODE=static
CHATOS_SERVICE_ID=
CHATOS_INSTANCE_ID=
CHATOS_INTERNAL_TOKEN_TTL_SECONDS=600

# sandbox manager auth
SANDBOX_MANAGER_REQUIRE_AUTH=true
SANDBOX_MANAGER_OPERATOR_TOKEN=
SANDBOX_MANAGER_USER_SERVICE_BASE_URL=http://127.0.0.1:39190
SANDBOX_MANAGER_USER_SERVICE_REQUEST_TIMEOUT_MS=5000
SANDBOX_MANAGER_DEFAULT_CLIENT_MAX_ACTIVE_LEASES=10
SANDBOX_MANAGER_DEFAULT_CLIENT_MAX_LEASE_TTL_SECONDS=7200

# task runner -> sandbox manager
TASK_RUNNER_SANDBOX_MANAGER_BASE_URL=http://127.0.0.1:8095
TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID=task_runner
TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY=
# sandbox MCP 默认通过 Sandbox Manager /api/sandboxes/:id/mcp proxy，不需要直连 agent endpoint。

# task runner scaling
TASK_RUNNER_ROLE=all
TASK_RUNNER_WORKER_ID=
TASK_RUNNER_WORKER_POLL_MS=1000
TASK_RUNNER_WORKER_CLAIM_TTL_MS=120000
TASK_RUNNER_WORKER_CONCURRENCY=4

# sandbox node
SANDBOX_NODE_ID=
SANDBOX_NODE_AGENT_ENDPOINT=
SANDBOX_NODE_HEARTBEAT_INTERVAL_SECONDS=10
```

## 推荐优先改的文件

```text
sandbox_manager_service/backend/src/config.rs
sandbox_manager_service/backend/src/auth.rs
sandbox_manager_service/backend/src/api/router.rs
sandbox_manager_service/backend/src/api/handlers.rs
sandbox_manager_service/backend/src/models.rs
sandbox_manager_service/backend/src/store/mod.rs
sandbox_manager_service/backend/src/service/manager.rs
sandbox_manager_service/backend/src/backend/docker.rs
sandbox_manager_service/backend/src/backend/kata.rs

task_runner_service/backend/src/config.rs
task_runner_service/backend/src/config/env_support.rs
task_runner_service/backend/src/services/sandbox_runtime.rs

crates/sandbox_manager_sdk/
crates/chatos_service_client/
```

## 验收标准

## 本次已落地的 P0/P1 关键实现

- Sandbox Manager `/api/*` 已增加接入鉴权和 scope/policy 校验；Task Runner 调 Sandbox Manager 会携带 system client 凭证。
- Task Runner 的 sandbox MCP URL 已切到 Sandbox Manager raw JSON-RPC proxy：`POST /api/sandboxes/:sandbox_id/mcp`；Manager 侧按 method 检查 `sandbox.mcp.tools`/`sandbox.mcp.call`、tool policy，再用派生 agent token 转发到 sandbox agent。
- Task Runner 已支持 `TASK_RUNNER_ROLE=api|worker|scheduler|all`：
  - `api` 只负责创建 queued run。
  - `worker` 通过共享 store 原子 claim queued run，并定时续租 claim。
  - `scheduler` 可单独扫描定时任务；推进 `next_run_at` 时使用 expected `next_run_at` 条件更新，避免多个 scheduler 重复消费同一个 due slot。
  - `all` 保持本地单进程开发体验。
- `task_runs` 已新增 `worker_id/claim_token/claim_until/attempt`，Mongo、SQLite、InMemory store 均支持 worker claim/renew。
- 带 `claim_token` 的 run 保存已加入 fencing：claim 失效后的旧 worker 不能写回终态，也不会继续污染 task/event/callback。
- API 启动不再恢复并失败化所有未完成 run，避免多 API 副本启动时误伤其他 worker 正在执行的任务。
- Sandbox agent token 不再使用 `lease_id`，改为 `lease_id + nonce + SANDBOX_MANAGER_AGENT_TOKEN_SECRET` 派生的签名 token；旧 lease 无 nonce 时兼容回退到 `lease_id`。
- Sandbox Manager active lease 容量已从进程内 `AtomicUsize` 改为 Mongo `sandbox_capacity_slots` 原子抢占；多 Manager 实例共享 `SANDBOX_MANAGER_POOL_MAX_ACTIVE`，`/api/sandbox-pool/status` 返回全局 active slot 数。
- Sandbox lease create 已支持 `x-idempotency-key`；按 `tenant_id + project_id + run_id + key` 复用已就绪 lease，创建中返回 409 让调用方重试。Task Runner 申请沙箱时会自动发送 `sandbox-lease:{run_id}`。

### 安全验收

- 不带 token 调用 `/api/sandboxes/leases` 返回 401。
- 普通用户只能看到自己的租约。
- Task Runner system client 只能创建、读取、释放自己创建的 lease。
- 没有 `sandbox.images.write` scope 的 client 不能初始化镜像。
- 传入超出 policy 的 `ttl_seconds/resource_limits/tools/network` 会返回 403/400。
- 直接调用 sandbox agent 且无短期 token 会返回 401。
- release/destroy 后旧 agent token 失效。

### 集群验收

- 启动两个 Task Runner API 实例，同时请求同一 task，只产生一个 active run。
- 启动两个 worker，queued run 只被一个 worker claim。
- 杀掉执行中的 worker，claim TTL 后 run 能被接管或被标记 failed。
- 启动两个 Sandbox Manager API 实例，全局 active lease 不超过配置。
- 任意实例都能查询 lease；release 能路由到正确节点或 node agent。
- 前端 event stream 不依赖固定 API 实例。

## 最小可落地顺序

1. 先做 Sandbox Manager P0 鉴权和 Task Runner 调用 header，立刻堵住“谁都能调沙箱”的洞。
2. 再把沙箱 agent token 签发、注入、proxy MCP 做闭环，避免 agent endpoint 成为绕过管理面的后门。
3. 接着做 Task Runner queue/worker claim，让执行从 API 实例里剥离。
4. 最后做 Sandbox Manager node agent 和全局容量调度，完成真正横向扩容。

这条路径改动可控：P0 基本不改变业务流程，只是在现有路由前加认证、在 lease 操作前加 scope 校验、在 Task Runner client 上加凭证；P1/P2 再逐步把“能跑”升级成“能横向稳定跑”。
