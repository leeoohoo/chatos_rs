# Official Website Service

Chatos RS 官网微服务。后端使用 Rust + Axum，前端使用 React + Vite。

## 本地开发

后端：

```bash
cd official_website_service/backend
cargo run
```

前端：

```bash
cd official_website_service/frontend
npm install
npm run dev
```

默认地址：

- 后端：http://127.0.0.1:39250
- 前端：http://127.0.0.1:39251

## 一键启动

```bash
make restart-official-website
```

默认 `OFFICIAL_WEBSITE_MODE=dev`，会启动两个进程：

- `http://localhost:39250`：Rust 后端，托管 `frontend/dist` 生产构建。
- `http://localhost:39251`：Vite 开发服务器，用于本地调试。

也可以直接调用脚本：

```bash
./official_website_service/restart_services.sh restart
./official_website_service/restart_services.sh status
./official_website_service/restart_services.sh stop
```

生产静态模式只启动 Rust 后端，前端由 `frontend/dist` 托管：

```bash
make build-official-website
OFFICIAL_WEBSITE_MODE=prod make restart-official-website
```

独立生产脚本使用 `49250`，不会占用开发官网端口：

```bash
make restart-official-website-prod
make status-official-website-prod
make stop-official-website-prod
```

如果要让根目录全栈脚本也启动官网：

```bash
START_OFFICIAL_WEBSITE=1 ./restart_all_services.sh restart
```

常用配置：

```bash
OFFICIAL_WEBSITE_MODE=dev
OFFICIAL_WEBSITE_HOST=127.0.0.1
OFFICIAL_WEBSITE_PORT=39250
OFFICIAL_WEBSITE_FRONTEND_PORT=39251
OFFICIAL_WEBSITE_STATIC_DIR=
OFFICIAL_WEBSITE_PUBLIC_BASE_URL=
OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS=true
OFFICIAL_WEBSITE_STATUS_TIMEOUT_MS=800
OFFICIAL_WEBSITE_STATUS_SCHEME=http
OFFICIAL_WEBSITE_STATUS_HOST=127.0.0.1
OFFICIAL_WEBSITE_STATUS_CHATOS_URL=
OFFICIAL_WEBSITE_STATUS_MEMORY_ENGINE_URL=
OFFICIAL_WEBSITE_STATUS_USER_SERVICE_URL=
OFFICIAL_WEBSITE_STATUS_PROJECT_MANAGEMENT_URL=
OFFICIAL_WEBSITE_STATUS_SANDBOX_MANAGER_URL=
OFFICIAL_WEBSITE_STATUS_TASK_RUNNER_URL=
OFFICIAL_WEBSITE_STATUS_OFFICIAL_WEBSITE_URL=
```

## 构建

```bash
make build-official-website
make smoke-official-website
```

生产运行时，后端默认托管 `official_website_service/frontend/dist`。

## Docker

从仓库根目录构建镜像：

```bash
make docker-build-official-website
```

直接使用 Docker 命令等价于：

```bash
docker build -f official_website_service/Dockerfile -t chatos-rs-official-website:local .
docker run --rm -p 39250:39250 chatos-rs-official-website:local
```

容器内默认：

- `OFFICIAL_WEBSITE_HOST=0.0.0.0`
- `OFFICIAL_WEBSITE_PORT=39250`
- `OFFICIAL_WEBSITE_MODE=prod`
- `OFFICIAL_WEBSITE_PUBLIC_BASE_URL=http://localhost:39250`
- `OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS=false`
- `OFFICIAL_WEBSITE_STATIC_DIR=/app/frontend/dist`

Docker 默认关闭 live status，避免公开部署时暴露内部服务拓扑。若部署在可信内网，可用
`OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS=true` 重新开启，并通过
`OFFICIAL_WEBSITE_STATUS_HOST` 或单项 `OFFICIAL_WEBSITE_STATUS_*_URL` 指向真实健康检查地址。

## 站点 API

- `GET /health`：官网后端健康检查。
- `GET /robots.txt`：动态 robots，使用 `OFFICIAL_WEBSITE_PUBLIC_BASE_URL` 生成 sitemap 地址。
- `GET /sitemap.xml`：动态 sitemap，默认公开 URL 为 `http://localhost:<OFFICIAL_WEBSITE_PORT>`。
- `GET /api/site/manifest`：官网内容 manifest。
- `GET /api/site/services`：微服务职责列表。
- `GET /api/site/status`：核心微服务健康检查汇总，默认单服务超时 `800ms`；可用
  `OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS=false` 关闭。

运行中站点 smoke：

```bash
make smoke-official-website-live
OFFICIAL_WEBSITE_SMOKE_BASE_URL=http://127.0.0.1:49250 make smoke-official-website-live
```

## 素材截图

官网素材位于：

```text
official_website_service/frontend/public/showcase/
```

当前已放入本地真实服务截图：

- `chatos-main.png`
- `memory-engine.png`
- `task-runner.png`
- `project-management.png`
- `sandbox-manager.png`

重新采集前请确认本地服务已经启动，并且不要把 token、密钥、邮箱、本地私有路径等敏感信息发布到官网素材里。

可选脚本：

```bash
cd official_website_service/frontend
npm run capture:showcase
```

该脚本需要本地环境提供 `playwright` 包和可用浏览器；如果只是在 Codex 内验收，也可以用浏览器工具手动截取后覆盖同名文件。
