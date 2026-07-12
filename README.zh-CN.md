# Chat OS

Chat OS 现在按云端 Docker 部署优先维护。除了 `local_connector_client/` 需要留在用户本机，其它服务都走 Docker Compose。

安装说明: [INSTALL_GUIDE.zh-CN.md](./INSTALL_GUIDE.zh-CN.md)

## 一次启动

在仓库根目录执行：

```bash
cp docker/.env.example docker/.env
# 修改 docker/.env 里的外部密钥和 API Key；内部服务 token 有默认值
docker/deploy.sh up
```

`docker/deploy.sh up` 默认从 GHCR 拉 CI 预构建镜像，不在本机编译。需要用本地源码构建镜像时：

```bash
docker/deploy.sh dev
```

日常重复部署如果不需要拉最新镜像，可以跳过 pull，直接复用本机已有镜像：

```bash
docker/deploy.sh fast
```

本地只改了某个服务时，可以只重建并重启对应 Compose service：

```bash
docker/deploy.sh rebuild task-runner-backend
docker/deploy.sh rebuild chatos-backend chatos-frontend
docker/deploy.sh build-services  # 查看可重建的 service 名
```

部署脚本默认会在成功更新镜像后自动清理 `<none>:<none>` dangling 镜像；如果需要临时关闭，可以在 `docker/.env` 设置 `CHATOS_DOCKER_PRUNE_DANGLING_IMAGES=false`。

Make 快捷入口：

```bash
make docker-up  # 拉预构建镜像启动
make docker-fast # 跳过 pull，复用已有镜像
make dev        # 用本地源码构建并启动
make docker-rebuild SERVICES="task-runner-backend"
```

## 本地快速测试

如果不想每次都走 Docker 镜像部署，可以用本地开发栈。它只用 Docker 启动 MongoDB/Harness 这类基础依赖，业务后端用 `cargo run`，前端用 Vite：

```bash
make local-dev
make local-dev-status
make local-dev-stop
```

DB Connection Hub 已归档到 `docs/db_connection_hub/`，不再随 Docker 栈或本地开发栈启动。

## 常用命令

```bash
docker/deploy.sh ps
docker/deploy.sh logs
docker/deploy.sh restart
docker/deploy.sh fast
docker/deploy.sh rebuild task-runner-backend
docker/deploy.sh clean-images  # 手动清理 dangling 镜像
docker/deploy.sh down
docker/deploy.sh reset
```

## 默认访问地址

- 主应用：`http://localhost:8088`
- 主后端：`http://localhost:3997`
- Harness：`http://localhost:3000`
- User Service：`http://localhost:39191`
- Memory Engine：`http://localhost:4178`
- Task Runner：`http://localhost:39091`
- Project Management：`http://localhost:39211`
- Sandbox Manager：`http://localhost:8096`
- Local Connector Service：`http://localhost:39230`
- Official Website：`http://localhost:39251`

## Local Connector Client

云端 relay service 已经容器化；`local_connector_client/` 仍然在宿主机运行，因为它要访问用户本机工作区和本机 Docker：

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

Docker 云端配置看 `docker/.env.example`。宿主机 Local Connector Client 的配置看根目录 `.env.example`。

## Harness

`harness/` 是从 `https://github.com/leeoohoo/harness.git` 拉下来的独立 Git 仓库；父级 Chat OS 仓库会忽略这个目录。以后更新 Harness，请进入 `harness/` 目录用它自己的 Git 流程处理。

新工作区如果需要源码副本，可以在根目录执行：

```bash
git clone https://github.com/leeoohoo/harness.git harness
```

Docker Compose 使用 `harness/harness` 镜像运行 Harness，数据放在 `harness-data` volume。

开源说明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。

## CI 镜像

GitHub Actions 会在 `main`、`master` 和版本 tag 上构建并推送所有 Chat OS 服务镜像到 GHCR。默认镜像配置在 `docker/.env.example`：

```env
CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
CHATOS_IMAGE_TAG=latest
```

如果要部署某个固定提交，可以把 `CHATOS_IMAGE_TAG` 改成 `sha-<commit>`。

如果 GHCR package 不是公开可读，部署机器需要先执行 `docker login ghcr.io`。

## 沙箱服务

Sandbox Manager 运行在 Compose 容器里，并挂载宿主机 `/var/run/docker.sock`。所以它可以从容器内控制当前宿主机 Docker，并把创建出来的沙箱容器加入同一个 Docker 网络，用容器名访问 agent。

这个模式可行，但权限很高：拿到 Docker socket 的容器基本等同于拥有宿主机 Docker 管理权限。

## 检查

```bash
make smoke
make test
```
