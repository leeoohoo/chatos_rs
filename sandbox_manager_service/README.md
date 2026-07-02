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
SANDBOX_MANAGER_IMAGE_TAG_PREFIX=chatos-sandbox-agent
SANDBOX_MANAGER_IMAGE_BUILD_CONTEXT=/path/to/chatos_rs
SANDBOX_MANAGER_IMAGE_DOCKERFILE=/path/to/chatos_rs/sandbox_manager_service/sandbox_agent/Dockerfile
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

默认镜像包含 JDK、Node.js、Rust、Go 的默认版本。也可以在管理台的“镜像”菜单里按语言选择具体版本并初始化，例如 JDK 8/11/17/21/25、Node.js 20/22/24/26、Python 3.10-3.14、Go 1.22-1.26、Rust stable/beta/nightly 或固定版本、.NET 8/9/10、PHP 8.2-8.5、Ruby 3.2-4.0、GCC 13/14、Clang 18/19/20。服务会按组合构建类似 `chatos-sandbox-agent:dev-java21-python3.14-go1.26` 的镜像，并在页面中显示初始化任务状态和构建日志。镜像内置 `chatos-sandbox-mcp-server`，默认监听 `49888`，HTTP MCP 入口为 `/mcp`，并把文件和终端操作限制在容器内的 `/workspace`。

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

镜像列表：

```bash
curl http://127.0.0.1:8095/api/sandbox-images
```

初始化自定义组合镜像：

```bash
curl -X POST http://127.0.0.1:8095/api/sandbox-images/initialize \
  -H 'content-type: application/json' \
  -d '{ "features": ["java@17", "python@3.14", "go@1.26"] }'
```

也可以附加自定义构建脚本。脚本会在语言工具链安装完成后以 root 身份执行；不要在脚本里写入密钥或令牌：

```bash
curl -X POST http://127.0.0.1:8095/api/sandbox-images/initialize \
  -H 'content-type: application/json' \
  -d '{
    "features": ["node@24"],
    "custom_build_script": "apt-get update && apt-get install -y --no-install-recommends postgresql-client"
  }'
```

初始化任务列表：

```bash
curl http://127.0.0.1:8095/api/sandbox-images/jobs
```

创建沙箱时指定镜像：

```json
{
  "image_id": "dev-java17-python3.14-go1.26"
}
```

## 当前边界

当前服务先把管理面、生命周期、Docker/Kata 启动和容器内 HTTP MCP server 跑通。它还没有接入 Task Runner 的自动路由。Docker backend 只用于开发和低风险测试，不代表云生产最终隔离边界；Linux/KVM 环境优先使用 Kata backend。
