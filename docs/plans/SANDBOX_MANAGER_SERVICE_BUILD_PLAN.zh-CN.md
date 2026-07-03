# Sandbox Manager 微服务建设实施方案

## 范围

本方案只做一件事：先把 `sandbox_manager_service` 这个独立微服务建起来。

暂不接入 Task Runner，暂不替换现有终端 MCP，暂不做真实项目结果写回。这个阶段的目标是让沙箱管理服务自身具备完整的管理闭环：

- Rust 后端可启动。
- React + Ant Design 前端可访问。
- 能创建、查看、释放、销毁沙箱租约。
- 能维护一个沙箱池。
- 能展示沙箱状态、租约、日志、资源配置。
- 后端先支持 `mock` backend 和 `docker` backend 两种模式。
- 前后端工程结构和现有项目服务风格保持一致。

## 技术选型

后端：

- Rust 2021。
- Axum 0.7。
- Tokio。
- Serde / serde_json。
- tower-http CORS / trace。
- tracing / tracing-subscriber。
- uuid。
- chrono。
- MongoDB 作为 MVP 持久化。
- reqwest 预留 agent 调用能力。

前端：

- React 18。
- Vite。
- TypeScript。
- Ant Design 5。
- `@ant-design/icons`。
- `@tanstack/react-query`。
- `react-router-dom`。
- dayjs。

服务目录：

```text
sandbox_manager_service/
  README.md
  restart_services.sh
  backend/
    Cargo.toml
    src/
      main.rs
      lib.rs
      config.rs
      state.rs
      error.rs
      models.rs
      api/
      service/
      store/
      pool/
      backend/
      agent/
      workspace/
  frontend/
    package.json
    index.html
    vite.config.ts
    tsconfig.json
    src/
      main.tsx
      App.tsx
      api/
      pages/
      components/
      types.ts
      styles.css
```

## 根 Workspace 调整

根 `Cargo.toml` 增加成员：

```toml
[workspace]
members = [
  "chat_app_server_rs",
  "crates/chatos_builtin_tools",
  "crates/chatos_ai_runtime",
  "crates/chatos_project_mcp_contract",
  "crates/memory_engine_sdk",
  "crates/chatos_mcp_runtime",
  "task_runner_service/backend",
  "project_management_service/backend",
  "sandbox_manager_service/backend"
]
```

后续如果要把协议抽成 crate，再增加：

```text
crates/chatos_sandbox_protocol
```

但 MVP 可以先把协议结构放在 `sandbox_manager_service/backend/src/models.rs`。

## 后端目标

### MVP 能力

1. 服务启动和健康检查。
2. 配置读取。
3. MongoDB 连接和索引初始化。
4. 沙箱租约 CRUD。
5. 沙箱池状态。
6. mock backend。
7. docker backend 基础封装。
8. 后台清理任务。
9. REST API。
10. OpenAPI 可以后续补，MVP 先保持类型清晰。

### 后端端口

建议默认：

```env
SANDBOX_MANAGER_HOST=127.0.0.1
SANDBOX_MANAGER_PORT=8095
SANDBOX_MANAGER_DATABASE_URL=mongodb://admin:admin@127.0.0.1:27018/sandbox_manager_service?authSource=admin
SANDBOX_MANAGER_MONGODB_DATABASE=sandbox_manager_service
SANDBOX_MANAGER_BACKEND=mock
SANDBOX_MANAGER_WORK_ROOT=.chatos/sandboxes
SANDBOX_MANAGER_POOL_MAX_ACTIVE=5
SANDBOX_MANAGER_POOL_MAX_PENDING=50
SANDBOX_MANAGER_LEASE_TTL_SECONDS=7200
SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS=30
```

前端默认端口：

```env
VITE_API_BASE_URL=http://127.0.0.1:8095
```

前端 dev server：

```text
8096
```

## 后端模块设计

### `config.rs`

负责读取环境变量。

核心结构：

```rust
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub backend: SandboxBackendKind,
    pub work_root: PathBuf,
    pub pool_max_active: usize,
    pub pool_max_pending: usize,
    pub lease_ttl: Duration,
    pub cleanup_interval: Duration,
    pub docker_image: String,
    pub docker_network_mode: String,
}

pub enum SandboxBackendKind {
    Mock,
    Docker,
}
```

### `state.rs`

服务共享状态：

```rust
pub struct AppState {
    pub config: AppConfig,
    pub store: SandboxStore,
    pub manager: SandboxManager,
    pub pool: SandboxPool,
}
```

MVP 可以让 `SandboxManager` 持有 `store`、`backend`、`pool`，`AppState` 只暴露 `manager`。

### `models.rs`

核心模型：

```rust
pub enum SandboxStatus {
    Pending,
    Leasing,
    Starting,
    Ready,
    Running,
    Releasing,
    Destroying,
    Destroyed,
    Failed,
    Expired,
}

pub struct SandboxLeaseRecord {
    pub id: String,
    pub sandbox_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub project_id: String,
    pub run_id: String,
    pub workspace_root: String,
    pub run_workspace: String,
    pub backend: String,
    pub status: SandboxStatus,
    pub agent_endpoint: Option<String>,
    pub resource_limits: ResourceLimits,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub destroyed_at: Option<String>,
    pub last_error: Option<String>,
}

pub struct ResourceLimits {
    pub cpu: f32,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub max_processes: u32,
}
```

请求/响应：

```rust
pub struct CreateSandboxLeaseRequest {
    pub tenant_id: String,
    pub user_id: String,
    pub project_id: String,
    pub run_id: String,
    pub workspace_root: String,
    pub tools: Vec<String>,
    pub ttl_seconds: Option<u64>,
    pub resource_limits: Option<ResourceLimits>,
    pub network: Option<NetworkPolicy>,
}

pub struct CreateSandboxLeaseResponse {
    pub lease_id: String,
    pub sandbox_id: String,
    pub status: SandboxStatus,
    pub agent_endpoint: Option<String>,
    pub run_workspace: String,
    pub expires_at: String,
}
```

### `error.rs`

统一 API 错误：

```rust
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}
```

常见错误码：

```text
bad_request
sandbox_not_found
sandbox_capacity_exceeded
sandbox_create_failed
sandbox_agent_unhealthy
sandbox_release_failed
sandbox_destroy_failed
internal_error
```

### `store/`

MVP 使用 MongoDB。

接口：

```rust
pub struct SandboxStore;

impl SandboxStore {
    pub async fn create_lease(&self, record: SandboxLeaseRecord) -> Result<(), StoreError>;
    pub async fn update_lease_status(...);
    pub async fn get_lease(&self, lease_id: &str);
    pub async fn get_by_sandbox_id(&self, sandbox_id: &str);
    pub async fn list_leases(&self, query: ListSandboxQuery);
    pub async fn append_event(&self, event: SandboxEventRecord);
    pub async fn list_events(&self, sandbox_id: &str);
}
```

### `pool/`

负责容量控制。

MVP 先做简单内存池：

```rust
pub struct SandboxPool {
    active: AtomicUsize,
    max_active: usize,
    max_pending: usize,
}
```

行为：

- create lease 前调用 `try_acquire_slot`。
- release/destroy 后释放 slot。
- 超过上限返回 `sandbox_capacity_exceeded`。

第二阶段再做 pending queue。

### `backend/`

抽象：

```rust
#[async_trait]
pub trait SandboxBackend: Send + Sync {
    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, SandboxBackendError>;
    async fn start(&self, sandbox_id: &str) -> Result<(), SandboxBackendError>;
    async fn stop(&self, sandbox_id: &str) -> Result<(), SandboxBackendError>;
    async fn destroy(&self, sandbox_id: &str) -> Result<(), SandboxBackendError>;
    async fn inspect(&self, sandbox_id: &str) -> Result<SandboxInstance, SandboxBackendError>;
}
```

#### Mock Backend

用于先跑通微服务和前端。

行为：

- create 时生成 sandbox id。
- agent endpoint 返回 `http://127.0.0.1:0/mock/{sandbox_id}`。
- 状态直接变成 `Ready`。
- destroy 只更新状态。

#### Docker Backend

MVP 只做最小可运行：

- 通过 `docker run -d` 启动容器。
- 通过 `docker inspect` 获取状态。
- 通过 `docker rm -f` 销毁。
- agent 端口可以先映射随机宿主端口，后续改内部网络。

示例：

```bash
docker run -d \
  --name chatos-sandbox-{sandbox_id} \
  --network none \
  --cpus 2 \
  --memory 4g \
  --pids-limit 128 \
  --read-only \
  --tmpfs /tmp:rw,nosuid,size=512m \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  -v {run_workspace}:/workspace:rw \
  -p 127.0.0.1::{agent_port} \
  chatos-sandbox-agent:latest
```

注意：Docker backend 在本阶段是工程 MVP，不代表最终云生产安全边界。

### `manager/`

核心编排层：

```rust
pub struct SandboxManager {
    store: SandboxStore,
    backend: Arc<dyn SandboxBackend>,
    pool: SandboxPool,
    config: AppConfig,
}
```

主要方法：

```rust
pub async fn create_lease(&self, input: CreateSandboxLeaseRequest) -> Result<CreateSandboxLeaseResponse, ManagerError>;
pub async fn heartbeat(&self, sandbox_id: &str, input: HeartbeatRequest) -> Result<HeartbeatResponse, ManagerError>;
pub async fn release(&self, sandbox_id: &str, input: ReleaseSandboxRequest) -> Result<ReleaseSandboxResponse, ManagerError>;
pub async fn destroy(&self, sandbox_id: &str) -> Result<DestroySandboxResponse, ManagerError>;
pub async fn get(&self, sandbox_id: &str) -> Result<SandboxLeaseRecord, ManagerError>;
pub async fn list(&self, query: ListSandboxQuery) -> Result<Vec<SandboxLeaseRecord>, ManagerError>;
```

### `api/`

路由：

```text
GET    /health
GET    /api/system/config
POST   /api/sandboxes/leases
GET    /api/sandboxes
GET    /api/sandboxes/:sandbox_id
POST   /api/sandboxes/:sandbox_id/heartbeat
POST   /api/sandboxes/:sandbox_id/release
DELETE /api/sandboxes/:sandbox_id
GET    /api/sandboxes/:sandbox_id/events
GET    /api/sandbox-pool/status
```

### `service/cleanup.rs`

后台清理：

- 每 `cleanup_interval` 扫描过期租约。
- 过期状态置为 `Expired`。
- 调 backend destroy。
- 释放 pool slot。
- 写事件。

## MongoDB Collection 设计

MVP 使用两个 collection：

```text
sandbox_leases
sandbox_events
```

`sandbox_leases` 文档结构与 `SandboxLeaseRecord` 对齐：

```json
{
  "id": "lease_...",
  "sandbox_id": "sandbox_...",
  "tenant_id": "tenant-dev",
  "user_id": "user-dev",
  "project_id": "project-dev",
  "run_id": "run-dev-1",
  "workspace_root": "/projects/demo",
  "run_workspace": "/projects/demo/.chatos/sandboxes/runs/run-dev-1/input/workspace",
  "backend": "mock",
  "status": "ready",
  "agent_endpoint": "mock://sandbox_...",
  "resource_limits": { "cpu": 2, "memory_mb": 4096, "disk_mb": 10240, "max_processes": 128 },
  "network": { "mode": "none" },
  "tools": ["filesystem", "terminal"],
  "created_at": "2026-06-30T00:00:00Z",
  "updated_at": "2026-06-30T00:00:00Z",
  "expires_at": "2026-06-30T02:00:00Z",
  "destroyed_at": null,
  "last_error": null
}
```

建议索引：

```text
sandbox_leases.sandbox_id unique
sandbox_leases.tenant_id
sandbox_leases.project_id
sandbox_leases.run_id
sandbox_leases.status + expires_at
sandbox_events.sandbox_id + created_at
```

## 前端目标

前端先做管理控制台，不做复杂营销页。

页面结构：

```text
/                  -> Dashboard
/sandboxes          -> 沙箱列表
/sandboxes/:id      -> 沙箱详情
/pool               -> 沙箱池状态
/create             -> 创建测试沙箱
/settings           -> 运行配置只读展示
```

Ant Design 风格：

- 使用 `Layout` + `Sider` + `Header`。
- 左侧导航：Dashboard、Sandboxes、Pool、Create、Settings。
- 页面以表格、描述列表、统计卡、日志列表为主。
- 不做大 hero，不做营销式首页。
- 操作按钮使用 icon + text，例如 Reload、Create、Release、Destroy。

## 前端模块设计

```text
frontend/src/
  api/
    client.ts
    sandboxes.ts
    system.ts
  pages/
    DashboardPage.tsx
    SandboxesPage.tsx
    SandboxDetailPage.tsx
    PoolPage.tsx
    CreateSandboxPage.tsx
    SettingsPage.tsx
  components/
    AppShell.tsx
    StatusTag.tsx
    SandboxActions.tsx
    EventTimeline.tsx
  types.ts
  App.tsx
  main.tsx
  styles.css
```

### Dashboard

展示：

- active 沙箱数。
- pending 沙箱数。
- failed 沙箱数。
- pool capacity。
- 最近沙箱事件。
- 最近创建的沙箱。

### Sandboxes 列表

Antd `Table` 列：

- status。
- sandbox_id。
- tenant_id。
- user_id。
- project_id。
- run_id。
- backend。
- created_at。
- expires_at。
- actions。

筛选：

- status。
- tenant_id。
- project_id。
- run_id。

操作：

- refresh。
- release。
- destroy。
- view detail。

### Sandbox Detail

展示：

- 基本信息 `Descriptions`。
- resource limits。
- agent endpoint。
- workspace path。
- timeline/events。
- 危险操作区：release、destroy。

### Pool Page

展示：

- max active。
- current active。
- max pending。
- current pending。
- idle clean target。
- cleanup interval。
- backend kind。

### Create Sandbox Page

MVP 测试表单：

- tenant_id。
- user_id。
- project_id。
- run_id。
- workspace_root。
- tools 多选。
- ttl seconds。
- cpu。
- memory。
- disk。
- max_processes。

提交后创建租约并跳转详情。

## 前端 package.json

保持现有服务风格：

```json
{
  "name": "sandbox-manager-service-frontend",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "type-check": "tsc --noEmit"
  },
  "dependencies": {
    "@ant-design/icons": "^5.6.1",
    "@tanstack/react-query": "^5.80.10",
    "antd": "^5.27.1",
    "dayjs": "^1.11.13",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-router-dom": "^6.30.1"
  },
  "devDependencies": {
    "@types/node": "^24.0.1",
    "@types/react": "^18.3.11",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.4.1",
    "typescript": "^5.8.3",
    "vite": "^5.4.19"
  }
}
```

## 开发启动脚本

新增：

```text
sandbox_manager_service/restart_services.sh
```

职责：

- 构建/启动后端。
- 启动前端 Vite。
- 默认端口：
  - backend `8095`
  - frontend `8096`
- 日志输出到：

```text
logs/sandbox_manager_backend.log
logs/sandbox_manager_frontend.log
```

根目录后续可新增：

```text
restart_sandbox_manager_service.sh
```

用于统一入口。

## 后端 Cargo.toml

建议：

```toml
[package]
name = "sandbox_manager_service_backend"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
axum = { version = "0.7", features = ["json"] }
chrono = { version = "0.4", features = ["clock", "serde"] }
dotenvy = "0.15"
futures-util = "0.3"
mongodb = { version = "2.8", features = ["tokio-runtime"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
uuid = { version = "1", features = ["serde", "v4"] }
```

## API 响应格式

成功响应直接返回数据：

```json
{
  "sandbox_id": "sandbox_123",
  "status": "ready"
}
```

错误响应统一：

```json
{
  "error": {
    "code": "sandbox_capacity_exceeded",
    "message": "sandbox pool is full"
  }
}
```

## 沙箱状态和颜色

前端状态展示：

| Status | Antd color |
| --- | --- |
| Pending | default |
| Leasing | processing |
| Starting | processing |
| Ready | success |
| Running | blue |
| Releasing | warning |
| Destroying | warning |
| Destroyed | default |
| Failed | error |
| Expired | error |

## 实施阶段

### Phase 1：创建工程骨架

目标：服务目录存在，前后端能启动。

任务：

1. 创建 `sandbox_manager_service/backend`。
2. 创建 `sandbox_manager_service/frontend`。
3. 根 `Cargo.toml` 加 workspace member。
4. 后端实现 `/health`。
5. 前端实现基础 `AppShell`。
6. 前端实现 Dashboard 空页面。
7. 增加 `restart_services.sh`。

验收：

- `cargo check -p sandbox_manager_service_backend` 通过。
- `npm --prefix sandbox_manager_service/frontend run type-check` 通过。
- 后端 `/health` 返回 ok。
- 前端能打开 Antd 布局。

### Phase 2：后端数据模型和 MongoDB

目标：租约和事件能持久化。

任务：

1. 实现 `models.rs`。
2. 实现 MongoDB collection 和索引初始化。
3. 实现 `SandboxStore`。
4. 实现 create/get/list/update lease。
5. 实现 append/list events。

验收：

- 创建 lease 后能查到。
- 状态更新后能持久化。
- 事件能按 sandbox id 查询。

### Phase 3：Sandbox Manager + Mock Backend

目标：不用 Docker 也能跑通管理流程。

任务：

1. 实现 `SandboxBackend` trait。
2. 实现 `MockSandboxBackend`。
3. 实现 `SandboxPool` active 计数。
4. 实现 `SandboxManager::create_lease`。
5. 实现 heartbeat/release/destroy。
6. 实现 cleanup worker。

验收：

- API 能创建 mock sandbox。
- release 后状态变 `Destroyed`。
- 超过 max active 返回容量错误。
- TTL 过期后 cleanup 自动销毁。

### Phase 4：REST API 完整化

目标：前端可以管理沙箱。

任务：

1. 实现全部 API 路由。
2. 支持列表过滤。
3. 支持 pool status。
4. 支持事件查询。
5. 支持系统配置查询。

验收：

- Postman/curl 能完整操作沙箱。
- 错误响应统一。

### Phase 5：前端管理台

目标：可视化管理。

任务：

1. 实现 API client。
2. 实现 Dashboard。
3. 实现 Sandboxes table。
4. 实现 Sandbox detail。
5. 实现 Pool page。
6. 实现 Create page。
7. 实现 Settings page。
8. 实现 release/destroy 操作确认。

验收：

- 可创建测试沙箱。
- 可查看状态。
- 可释放和销毁。
- 可查看事件。
- UI 使用 Ant Design，布局稳定。

### Phase 6：Docker Backend MVP

目标：能启动真实容器。

任务：

1. 实现 `DockerSandboxBackend`。
2. 支持 `docker run`。
3. 支持 `docker inspect`。
4. 支持 `docker rm -f`。
5. 支持端口映射 agent endpoint。
6. 记录 docker command 和 container id。

验收：

- `SANDBOX_MANAGER_BACKEND=docker` 时能创建容器。
- destroy 能删除容器。
- 容器异常退出能标记 failed。

### Phase 7：测试和文档

任务：

1. 后端单元测试。
2. store 测试。
3. manager mock backend 测试。
4. 前端 type-check。
5. 写 `sandbox_manager_service/README.md`。

验收：

- 文档包含启动方式、环境变量、API 示例。
- 基础测试通过。

## 首版 API 示例

创建：

```bash
curl -X POST http://127.0.0.1:8095/api/sandboxes/leases \
  -H 'content-type: application/json' \
  -d '{
    "tenant_id":"tenant-dev",
    "user_id":"user-dev",
    "project_id":"project-dev",
    "run_id":"run-dev-1",
    "workspace_root":"/Users/lilei/project/my_project/chatos_rs",
    "tools":["filesystem","terminal"],
    "ttl_seconds":3600
  }'
```

查询：

```bash
curl http://127.0.0.1:8095/api/sandboxes
```

释放：

```bash
curl -X POST http://127.0.0.1:8095/api/sandboxes/{sandbox_id}/release \
  -H 'content-type: application/json' \
  -d '{"lease_id":"lease_...","export_result":false,"destroy":true}'
```

池状态：

```bash
curl http://127.0.0.1:8095/api/sandbox-pool/status
```

## 不在本阶段做的事

本阶段不做：

- Task Runner 自动接入。
- 现有终端 MCP 替换。
- sandbox-agent 完整文件/终端实现。
- 项目复制和 diff 生成。
- 真实项目写回。
- gVisor/Kata/microVM。
- 多节点调度。
- 复杂权限系统。

但后端 API 和模型要预留这些字段，避免后续大改。

## 文件创建清单

后端：

```text
sandbox_manager_service/backend/Cargo.toml
sandbox_manager_service/backend/src/main.rs
sandbox_manager_service/backend/src/lib.rs
sandbox_manager_service/backend/src/config.rs
sandbox_manager_service/backend/src/state.rs
sandbox_manager_service/backend/src/error.rs
sandbox_manager_service/backend/src/models.rs
sandbox_manager_service/backend/src/api/mod.rs
sandbox_manager_service/backend/src/api/router.rs
sandbox_manager_service/backend/src/api/handlers.rs
sandbox_manager_service/backend/src/service/mod.rs
sandbox_manager_service/backend/src/service/manager.rs
sandbox_manager_service/backend/src/service/cleanup.rs
sandbox_manager_service/backend/src/store/mod.rs
sandbox_manager_service/backend/src/pool/mod.rs
sandbox_manager_service/backend/src/backend/mod.rs
sandbox_manager_service/backend/src/backend/mock.rs
sandbox_manager_service/backend/src/backend/docker.rs
```

前端：

```text
sandbox_manager_service/frontend/package.json
sandbox_manager_service/frontend/index.html
sandbox_manager_service/frontend/vite.config.ts
sandbox_manager_service/frontend/tsconfig.json
sandbox_manager_service/frontend/src/main.tsx
sandbox_manager_service/frontend/src/App.tsx
sandbox_manager_service/frontend/src/types.ts
sandbox_manager_service/frontend/src/styles.css
sandbox_manager_service/frontend/src/api/client.ts
sandbox_manager_service/frontend/src/api/sandboxes.ts
sandbox_manager_service/frontend/src/api/system.ts
sandbox_manager_service/frontend/src/components/AppShell.tsx
sandbox_manager_service/frontend/src/components/StatusTag.tsx
sandbox_manager_service/frontend/src/components/SandboxActions.tsx
sandbox_manager_service/frontend/src/components/EventTimeline.tsx
sandbox_manager_service/frontend/src/pages/DashboardPage.tsx
sandbox_manager_service/frontend/src/pages/SandboxesPage.tsx
sandbox_manager_service/frontend/src/pages/SandboxDetailPage.tsx
sandbox_manager_service/frontend/src/pages/PoolPage.tsx
sandbox_manager_service/frontend/src/pages/CreateSandboxPage.tsx
sandbox_manager_service/frontend/src/pages/SettingsPage.tsx
```

其他：

```text
sandbox_manager_service/README.md
sandbox_manager_service/restart_services.sh
```

## 推荐第一步

第一步不要碰 Docker，也不要碰现有 Task Runner。先做：

1. Rust 后端 `/health`。
2. MongoDB lease collection。
3. Mock backend。
4. 创建/列表/详情/释放/销毁 API。
5. React + Antd 管理台。

这样最快能看到一个独立微服务跑起来，并且后续接 Docker、agent、Task Runner 都有稳定的管理面。
