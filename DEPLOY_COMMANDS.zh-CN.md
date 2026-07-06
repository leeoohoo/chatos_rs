# 远程部署命令

本文件只记录命令模板，不要写真实服务器密码。把下面占位符替换成实际值后执行：

- `<REMOTE_HOST>`：服务器 IP 或域名
- `<REMOTE_USER>`：SSH 用户，例如 `root`
- `<REMOTE_PASSWORD>`：SSH 密码
- `<REMOTE_PORT>`：SSH 端口，通常是 `22`

脚本入口统一是仓库根目录下的 `deploy_remote_prod.sh`。

## 通用参数

```bash
REMOTE_HOST=<REMOTE_HOST> \
REMOTE_USER=<REMOTE_USER> \
REMOTE_PASSWORD='<REMOTE_PASSWORD>' \
REMOTE_PORT=<REMOTE_PORT> \
REMOTE_CLEAN_TARGET=0 \
REMOTE_DEPLOY_SERVICES=<SERVICE> \
./deploy_remote_prod.sh
```

如果远端 sudo 密码和 SSH 密码不同，额外加：

```bash
REMOTE_SUDO_PASSWORD='<REMOTE_SUDO_PASSWORD>'
```

## 全量部署

会同步仓库、构建主前端/主后端，并重建附属服务。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=all ./deploy_remote_prod.sh
```

## 主服务

只部署 `chat_app` + `chat_app_server_rs` + nginx 配置。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=main ./deploy_remote_prod.sh
```

## 用户服务

只部署 user-service 后端/前端。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=user-service ./deploy_remote_prod.sh
```

## Memory Engine

只部署 memory-engine 后端/前端。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=memory-engine ./deploy_remote_prod.sh
```

## 项目管理服务

只部署 project-management 后端/前端。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=project-management ./deploy_remote_prod.sh
```

## Task Runner

只部署 task-runner 后端/前端。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=task-runner ./deploy_remote_prod.sh
```

## Sandbox Manager

只部署 sandbox-manager 后端/前端。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=sandbox-manager ./deploy_remote_prod.sh
```

## DB Connection Hub

只部署 db-connection-hub。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=db-hub ./deploy_remote_prod.sh
```

## 官网服务

只部署 official-website。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=official-website ./deploy_remote_prod.sh
```

## 只刷新 nginx

只渲染并 reload nginx，不重启业务服务。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=nginx ./deploy_remote_prod.sh
```

## 多服务组合部署

多个服务用英文逗号分隔。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=user-service,memory-engine,nginx ./deploy_remote_prod.sh
```

常用组合：

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=project-management,task-runner ./deploy_remote_prod.sh
```

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_CLEAN_TARGET=0 REMOTE_DEPLOY_SERVICES=user-service,memory-engine,project-management,task-runner,sandbox-manager ./deploy_remote_prod.sh
```

## 只预览部署计划

不连接远端执行变更，只打印计划。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_DEPLOY_SERVICES=<SERVICE> PLAN_ONLY=1 ./deploy_remote_prod.sh
```

## 只同步代码

同步代码到远端 staging 目录，但不构建、不重启。

```bash
REMOTE_HOST=<REMOTE_HOST> REMOTE_USER=<REMOTE_USER> REMOTE_PASSWORD='<REMOTE_PASSWORD>' REMOTE_PORT=<REMOTE_PORT> REMOTE_DEPLOY_SERVICES=<SERVICE> SYNC_ONLY=1 ./deploy_remote_prod.sh
```

## Windows 执行方式

推荐在 WSL 或带 `sshpass` 的 Bash 环境执行。PowerShell 里可以这样传环境变量：

```powershell
$env:REMOTE_HOST="<REMOTE_HOST>"
$env:REMOTE_USER="<REMOTE_USER>"
$env:REMOTE_PASSWORD="<REMOTE_PASSWORD>"
$env:REMOTE_PORT="<REMOTE_PORT>"
$env:REMOTE_CLEAN_TARGET="0"
$env:REMOTE_DEPLOY_SERVICES="<SERVICE>"
bash ./deploy_remote_prod.sh
```

`deploy_remote_prod.sh` 依赖 `ssh`、`rsync`、`sshpass`、`bash`。如果本机没有 `sshpass`，脚本会在执行前报错。
