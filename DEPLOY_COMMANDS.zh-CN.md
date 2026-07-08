# Docker 部署命令速查

## 首次启动

```bash
cp docker/.env.example docker/.env
# 修改 docker/.env
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
docker/deploy.sh logs sandbox-manager-backend
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
