# Docker 部署命令速查

## 一键部署生产环境

在仓库根目录执行：

```bash
./scripts/deploy-online.sh
```

不带参数时会显示交互菜单。也可以直接指定范围：

```bash
# 所有云服务和三个正式插件
./scripts/deploy-online.sh all

# 所有云服务 / 所有后端 / 所有前端
./scripts/deploy-online.sh cloud
./scripts/deploy-online.sh cloud-backends
./scripts/deploy-online.sh cloud-frontends

# 一个或多个指定服务
./scripts/deploy-online.sh service plugin-management-backend local-connector-service-backend

# 一个、多个或全部插件
./scripts/deploy-online.sh plugin computer-use
./scripts/deploy-online.sh plugin browser
./scripts/deploy-online.sh plugin document
./scripts/deploy-online.sh plugin browser computer-use
./scripts/deploy-online.sh plugin all

# 查看部署阶段、当前 Release 和服务状态
./scripts/deploy-online.sh status

# 持续查看指定服务日志，按 Ctrl-C 退出
./scripts/deploy-online.sh logs plugin-management-backend
```

生产云服务部署要求当前位于 `3.0.0` 分支、所有已跟踪文件均已提交，并且提交已经推送到 `origin/3.0.0`。本地未跟踪且不会进入 Release 的目录不会阻断部署。插件部署会安全地提示输入管理员密码；自动化环境可通过 `CHATOS_DEPLOY_ADMIN_PASSWORD` 提供。

## 首次启动

```bash
cp docker/bootstrap.conf.example docker/bootstrap.conf
# 只填写配置中心启动前必需的引导配置
docker/deploy.sh up
```

默认会拉取 GHCR 预构建镜像。

## 本地源码构建启动

```bash
docker/deploy.sh dev
```

## 查看状态

```bash
docker/deploy.sh ps
```

## 查看日志

```bash
docker/deploy.sh logs
docker/deploy.sh logs chatos-backend
docker/deploy.sh logs harness
docker/deploy.sh logs task-runner-backend
```

## 重启

```bash
docker/deploy.sh restart
```

`restart` 默认仍使用预构建镜像；本地构建重启用：

```bash
docker/deploy.sh restart-dev
```

## 只构建镜像

```bash
docker/deploy.sh build
```

## 停止

```bash
docker/deploy.sh down
```

## 清空环境

```bash
docker/deploy.sh reset
```

`reset` 会删除 volumes，包括 MongoDB 数据。

## 校验 Compose 配置

```bash
docker compose -f docker/compose.yml config
docker compose -f docker/compose.yml -f docker/compose.build.yml config
```

## Make 快捷入口

```bash
make docker-up
make dev
make docker-ps
make docker-logs
make docker-restart
make docker-down
make docker-reset
```

`make docker-up` 使用预构建镜像；`make dev` 使用本地源码构建。
