# Chatos RS 安装与部署指南

当前项目以 Docker Compose 作为云端/服务器部署入口。旧的宿主机 systemd、nginx 模板、远程 rsync 脚本、Windows/WSL 启动脚本已经不再维护。

## 1. 前置依赖

- Docker Engine
- Docker Compose v2，也就是 `docker compose`
- Git，可选；如果不是通过 git 获取部署文件，可以不装
- Bash
- 可选：`make`

服务器部署使用镜像运行文件：`docker/compose.yml`。本地源码构建只在 `docker/compose.build.yml` overlay 里启用。

## 2. 准备配置

```bash
cp docker/.env.example docker/.env
```

至少检查这些值：

- `OPENAI_API_KEY`
- `MONGODB_USER`
- `MONGODB_PASSWORD`
- `AUTH_JWT_SECRET`
- `USER_SERVICE_JWT_SECRET`
- `USER_SERVICE_INTERNAL_API_SECRET`
- `TASK_RUNNER_INTERNAL_API_SECRET`
- `TASK_RUNNER_CHATOS_CALLBACK_SECRET`
- `HARNESS_ADMIN_PASSWORD`

Memory Engine operator token、Sandbox Manager operator token、Sandbox system client id/key、Sandbox agent token secret 都有内部默认值，单机 Docker 部署不需要手动配置。共享环境和生产环境如果需要轮换这些内部凭证，可以在 `docker/.env` 里显式覆盖。

开发环境可以先用默认值跑通；共享环境和生产环境必须把上面列出的外部可见密钥换成强随机密钥。

## 3. 启动

```bash
docker/deploy.sh up
```

默认模式会从 GHCR 拉取 CI 已构建好的镜像并启动，不需要在部署机器上编译 Rust 或前端。

默认 `docker/compose.yml` 不包含 `build:` 配置；部署机器只要能拉镜像，就不会触发本地源码构建。

等价 Make 入口：

```bash
make docker-up
```

如需使用本地源码重新构建镜像：

```bash
docker/deploy.sh dev
# 或
make dev
```

重复部署但不需要拉最新镜像时，用快路径跳过 pull：

```bash
docker/deploy.sh fast
# 或
make docker-fast
```

本地只改了某个服务时，不要全量 `dev`，直接重建对应 Compose service：

```bash
docker/deploy.sh rebuild task-runner-backend
docker/deploy.sh rebuild chatos-backend chatos-frontend
docker/deploy.sh build-services
# 或
make docker-rebuild SERVICES="task-runner-backend"
```

部署脚本默认会在成功更新镜像后自动清理 `<none>:<none>` dangling 镜像。如果调试时需要保留这些镜像，可以在 `docker/.env` 里设置：

```env
CHATOS_DOCKER_PRUNE_DANGLING_IMAGES=false
```

## 4. 常用运维命令

```bash
docker/deploy.sh ps
docker/deploy.sh logs
docker/deploy.sh logs task-runner-backend
docker/deploy.sh restart
docker/deploy.sh fast
docker/deploy.sh dev
docker/deploy.sh rebuild task-runner-backend
docker/deploy.sh build
docker/deploy.sh clean-images
docker/deploy.sh down
docker/deploy.sh reset
```

`reset` 会删除 Compose volumes，包括 MongoDB 数据，仅用于需要清空环境的时候。

本机开发测试如果不想频繁构建 Docker 镜像，可以使用宿主机开发栈：

```bash
make local-dev
make local-dev-status
make local-dev-stop
```

这个入口只用 Docker 启动 MongoDB/Harness 等基础依赖，业务服务在宿主机运行。DB Connection Hub 已归档到 `docs/db_connection_hub/`，不会启动。

## 5. 默认端口

- 主应用：`8088`
- 主后端：`3997`
- Harness Web：`3000`
- Harness SSH：`3022`
- User Service：`39191`
- User Service backend：`39190`
- Memory Engine：`4178`
- Memory Engine backend：`7081`
- Task Runner：`39091`
- Task Runner backend：`39090`
- Project Management：`39211`
- Project Management backend：`39210`
- Sandbox Manager：`8096`
- Sandbox Manager backend：`8095`
- Local Connector Service backend：`39230`
- Official Website：`39251`
- Official Website backend：`39250`
- MongoDB host port：`27018`

端口可以在 `docker/.env` 里覆盖。

## 6. CI 镜像

CI 会把自研服务镜像推送到 GHCR。默认配置：

```env
CHATOS_DOCKER_MODE=prebuilt
CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
CHATOS_IMAGE_TAG=latest
```

部署机器只需要 Docker 和 Compose：

```bash
cp docker/.env.example docker/.env
docker/deploy.sh up
```

如果 GHCR package 不是公开可读，部署机器需要先登录：

```bash
docker login ghcr.io
```

如果要部署某次固定 CI 构建，把 tag 改成对应提交：

```env
CHATOS_IMAGE_TAG=sha-<commit>
```

本地源码构建入口使用 `docker/compose.build.yml` overlay；CI 会同时校验运行 Compose 和本地构建 overlay。

## 7. Sandbox Manager 和宿主 Docker

Sandbox Manager 自身在容器里运行，但通过挂载 `/var/run/docker.sock` 控制宿主机 Docker：

```yaml
volumes:
  - /var/run/docker.sock:/var/run/docker.sock
```

Compose 默认设置：

```env
SANDBOX_MANAGER_DOCKER_NETWORK=chatos-cloud
SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE=container
SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT=false
```

也就是说，Sandbox Manager 创建的沙箱容器会加入 `chatos-cloud` 网络，Manager 用类似 `http://chatos-sandbox-<id>:49888` 的容器内地址访问 agent，不需要把每个 sandbox agent 端口发布到宿主机。

注意：Docker socket 是高权限能力。能访问它的容器基本可以管理宿主机上的 Docker 资源。

## 8. Harness

Harness 作为独立微服务纳入 Docker 栈：

- Compose 服务名：`harness`
- 默认访问地址：`http://localhost:3000`
- 默认 SSH 端口：`3022`
- 数据 volume：`harness-data`
- 镜像：`harness/harness`

根目录 `harness/` 是独立 Git checkout，不纳入父仓库提交。更新 Harness 源码时进入该目录使用它自己的 Git：

```bash
cd harness
git status
git pull
```

新工作区如需恢复源码副本：

```bash
git clone https://github.com/leeoohoo/harness.git harness
```

Harness 使用 Apache License 2.0。开源说明见 `THIRD_PARTY_NOTICES.md`，原始 license/notice 保留在 `harness/LICENSE` 和 `harness/NOTICE`。

当前 Compose 默认开启 `HARNESS_PROVISIONING_ENABLED=true`，`user_service` 会把 Harness API 地址指向 `http://harness:3000`。

## 9. Local Connector Client

`local_connector_service` 已经在 Docker 栈里运行。`local_connector_client` 仍然需要在用户本机运行，因为它要访问本机工作区、本机凭据和本机 Docker。

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

本机 connector 配置可以从根目录 `.env.example` 复制到 `.env` 后调整。

## 10. 检查

```bash
make smoke
```

这会检查仓库约束、脚本语法和 Docker Compose 配置。

Rust 相关检查：

```bash
cargo fmt --check
cargo check
```

## 11. 旧部署入口

以下路径已经被移除或不再作为部署入口：

- 宿主机 systemd/nginx 安装模板
- `deploy_remote_prod.sh`
- 根目录和各服务的旧 `restart_services.sh`
- Windows/WSL PowerShell 启动脚本
- 本地多进程 Mongo/startup helper

后续部署请统一从 `docker/deploy.sh` 和 `docker/compose.yml` 维护。
