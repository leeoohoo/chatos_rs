# Sandbox Manager Service

独立的沙箱管理微服务。当前 MVP 提供：

- Rust/Axum 后端。
- MongoDB 持久化。
- mock backend。
- Docker backend。
- Kata backend。
- React + Ant Design 管理台。
- 沙箱租约创建、列表、详情、释放、销毁。
- 沙箱池容量状态。

## 端口

- Backend: `8095`
- Frontend: `8096`

## 环境变量

```env
SANDBOX_MANAGER_HOST=127.0.0.1
SANDBOX_MANAGER_PORT=8095
SANDBOX_MANAGER_DATABASE_URL=mongodb://admin:admin@127.0.0.1:27018/sandbox_manager_service?authSource=admin
SANDBOX_MANAGER_MONGODB_DATABASE=sandbox_manager_service
SANDBOX_MANAGER_BACKEND=auto
SANDBOX_MANAGER_WORK_ROOT=.chatos/sandboxes
SANDBOX_MANAGER_POOL_MAX_ACTIVE=5
SANDBOX_MANAGER_POOL_MAX_PENDING=50
SANDBOX_MANAGER_LEASE_TTL_SECONDS=7200
SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS=30
SANDBOX_MANAGER_AGENT_PORT=49888
SANDBOX_MANAGER_DOCKER_IMAGE=chatos-sandbox-agent:latest
SANDBOX_MANAGER_DOCKER_NETWORK=bridge
SANDBOX_MANAGER_KATA_CONTAINER_CLI=nerdctl
SANDBOX_MANAGER_KATA_RUNTIME=io.containerd.kata.v2
SANDBOX_MANAGER_KATA_IMAGE=chatos-sandbox-agent:latest
SANDBOX_MANAGER_KATA_NETWORK=bridge
SANDBOX_MANAGER_FRONTEND_PORT=8096
SANDBOX_MANAGER_API_PROXY_TARGET=http://127.0.0.1:8095
```

`SANDBOX_MANAGER_BACKEND=auto` 时：

- macOS 默认使用 `docker`。
- Linux 默认使用 `kata`。
- Windows / 其它系统默认使用 `docker`。

可显式设置为 `mock`、`docker` 或 `kata`。

## 启动

先构建沙箱镜像：

```bash
docker build -t chatos-sandbox-agent:latest -f sandbox_manager_service/sandbox_agent/Dockerfile .
```

镜像内置 `chatos-sandbox-mcp-server`，默认监听 `49888`，HTTP MCP 入口为 `/mcp`，并把文件和终端操作限制在容器内的 `/workspace`。

```bash
./sandbox_manager_service/restart_services.sh restart
```

前端：

```text
http://127.0.0.1:8096
```

后端健康检查：

```bash
curl http://127.0.0.1:8095/health
```

## API 示例

创建沙箱：

```bash
curl -X POST http://127.0.0.1:8095/api/sandboxes/leases \
  -H 'content-type: application/json' \
  -d '{
    "tenant_id": "tenant-dev",
    "user_id": "user-dev",
    "project_id": "project-dev",
    "run_id": "run-dev-1",
    "workspace_root": "/tmp/chatos-sandbox-demo",
    "tools": ["filesystem", "terminal"],
    "ttl_seconds": 3600
  }'
```

列表：

```bash
curl http://127.0.0.1:8095/api/sandboxes
```

池状态：

```bash
curl http://127.0.0.1:8095/api/sandbox-pool/status
```

## 当前边界

当前服务先把管理面、生命周期、Docker/Kata 启动和容器内 HTTP MCP server 跑通。它还没有接入 Task Runner 的自动路由。Docker backend 只用于开发和低风险测试，不代表云生产最终隔离边界；Linux/KVM 环境优先使用 Kata backend。
