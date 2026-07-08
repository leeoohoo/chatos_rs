# Chatos 服务治理实施说明

本文档只保留当前采用的方案：**Consul-first + `chatos_service_runtime` shared runtime crate**。全仓库服务治理只按这一条路线执行。

## 已采用方案

Chatos 云端服务统一通过 Docker 部署，服务治理由 Consul 和共享 Rust runtime 承担：

- 注册中心：每个 Rust 后端启动后向 Consul 注册自身。
- 健康检查：Consul 使用各服务已有 HTTP health endpoint 判断实例健康。
- 服务发现：调用方查询 Consul passing 实例。
- 负载均衡：`chatos_service_runtime` 在健康实例之间做 round-robin。
- 配置中心：Consul KV 提供非 secret 配置默认值，现有环境变量仍是最高优先级。
- 兜底：Consul 不可用时继续使用 Docker Compose service DNS / 现有静态 URL。

## 已落地组件

共享 crate：

```text
crates/chatos_service_runtime
```

当前能力：

- `register_current_service(...)`：服务启动时注册当前实例。
- `resolve_service_base_url(...)`：解析下游服务 base URL。
- `resolve_service_url(...)`：解析带固定 path suffix 的下游 URL。
- `apply_config_center_env(...)`：从 Consul KV 加载非 secret 配置默认值。
- `get_config_text(...)` / `get_service_config_text(...)`：读取 Consul KV 文本配置。

Docker Compose 已加入 Consul：

```text
consul -> http://localhost:${CONSUL_HTTP_PORT:-8500}
```

`docker/.env.example` 已包含 runtime 默认值：

```env
CHATOS_ENV=local
CHATOS_SERVICE_RUNTIME_ENABLED=true
CHATOS_SERVICE_DISCOVERY_MODE=consul,static
CHATOS_CONSUL_HTTP_ADDR=http://consul:8500
CHATOS_SERVICE_RUNTIME_REQUEST_TIMEOUT_MS=3000
CONSUL_IMAGE=hashicorp/consul:1.21
CONSUL_HTTP_PORT=8500
```

## 服务命名

业务代码和 Consul catalog 统一使用以下 canonical service name：

| Service name | 模块 |
| --- | --- |
| `chatos-backend` | `chat_app_server_rs` |
| `user-service` | `user_service/backend` |
| `task-runner` | `task_runner_service/backend` |
| `memory-engine` | `memory_engine/backend` |
| `project-service` | `project_management_service/backend` |
| `sandbox-manager` | `sandbox_manager_service/backend` |
| `local-connector-service` | `local_connector_service/backend` |
| `db-connection-hub` | `db_connection_hub/backend` |
| `official-website` | `official_website_service/backend` |
| `harness` | external Harness service |

Consul service ID 默认格式：

```text
{service-name}-{hostname}-{pid}
```

Docker Compose 部署中会显式设置稳定 service ID：

```text
{service-name}-docker
```

这样重建容器时会覆盖同一个 Consul service record，不会累积历史实例。

## 启动流程

每个 Rust 后端启动时按同一顺序执行：

1. 加载本地 `.env`。
2. 初始化 tracing。
3. 调用 `apply_config_center_env(service-name)` 从 Consul KV 加载配置默认值。
4. 调用各服务现有 `AppConfig::from_env()` / `Config::from_env()`。
5. 用 runtime 解析下游服务 URL。
6. 调用 `register_current_service(...)` 注册当前服务实例。
7. 启动 HTTP listener。

这个顺序让 Consul KV 能参与配置，但不会覆盖真实环境变量。

## 配置中心约定

Consul KV 使用固定前缀：

```text
chatos/{env}/shared/config
chatos/{env}/services/{service-name}/config
```

示例：

```text
chatos/local/shared/config
chatos/local/services/task-runner/config
```

KV value 使用 JSON object。推荐把配置放在 `env` 字段中：

```json
{
  "env": {
    "CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS": 30000,
    "TASK_RUNNER_WORKER_ENABLED": true
  }
}
```

加载规则：

- 先读 `shared/config`。
- 再读 `services/{service-name}/config`，同名 key 覆盖 shared。
- 只接受大写字母、数字、下划线组成的 env key。
- 如果真实进程环境变量已经存在，Consul KV 不覆盖它。
- JSON string、number、boolean 会转换为 env value；array/object 会压成 JSON 字符串。

禁止放入 Consul KV：

- 数据库密码。
- JWT secret。
- OpenAI API key。
- Harness admin password。
- 任何长期访问 token。

这些仍然使用 env / Docker secret。

## 服务发现与负载均衡

runtime 通过 Consul HTTP API 查询健康实例：

```text
GET /v1/health/service/{service-name}?passing=true
```

如果返回多个实例，runtime 在调用进程内按 service name 维护 round-robin 计数器。没有 healthy instance 或 Consul 请求失败时，runtime 会记录 warning 并回到调用方传入的静态 URL。

当前已接入：

- `chat_app_server_rs` 调用 `user-service`、`task-runner`、`project-service` 时按请求解析。
- `user-service` 启动时解析 `memory-engine`、`task-runner`、`harness`。
- `task-runner` 启动时解析 `user-service`、`sandbox-manager`、`memory-engine`、`project-service`、`chatos-backend` callback。
- `project-service` 启动时解析 `user-service`、`task-runner`。
- `memory-engine`、`sandbox-manager`、`local-connector-service` 启动时解析 `user-service`。
- 所有 Rust 后端都会注册自身到 Consul。
- Harness 不是 Rust 服务，Docker Compose 通过 `docker/consul/services/harness.json` 静态注册到 Consul。

## Docker 运行

本地或服务器 Docker 部署：

```bash
cp docker/.env.example docker/.env
./docker/deploy.sh up
```

启动后检查：

```bash
docker compose -f docker/compose.yml --env-file docker/.env ps
curl http://localhost:8500/v1/catalog/services
```

Consul UI：

```text
http://localhost:8500
```

## 验收清单

- `docker/deploy.sh up` 能启动 Consul 和所有云端服务。
- Consul catalog 能看到所有 Rust 后端。
- Consul health 页面能看到各服务 health check 状态。
- 多副本服务会被 runtime 轮询选择健康实例。
- 删除或停掉 Consul 后，服务继续使用 static fallback。
- Consul KV 只管理非 secret 配置默认值。
- `cargo check` 覆盖受影响 Rust 后端。
- Docker Compose config 校验通过。

## 关键文件

- `crates/chatos_service_runtime/src/lib.rs`
- `docker/compose.yml`
- `docker/consul/services/harness.json`
- `docker/.env.example`
- `docker/deploy.sh`
- 各 Rust 后端 `backend/src/main.rs`
- `chatos/backend/src/lib.rs`
- `chatos/backend/src/services/user_service_api_client/http.rs`
- `chatos/backend/src/services/task_runner_api_client.rs`
- `chatos/backend/src/services/project_management_api_client.rs`

## 参考

- Consul service discovery: https://developer.hashicorp.com/consul/docs/use-case/service-discovery
- Consul KV: https://developer.hashicorp.com/consul/docs/automate/kv
