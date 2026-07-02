# Chatos RS 跨系统安装教程

## 1. 这份文档覆盖什么

本仓库当前包含这些核心服务：

- `chat_app/`：ChatOS 主前端
- `chat_app_server_rs/`：ChatOS 主后端
- `user_service/`：统一用户与 agent 账号管理服务
- `task_runner_service/`：任务执行与 agent 运行时服务
- `memory_engine/`：独立记忆微服务

如果你要部署到服务器，走宿主机安装脚本。
如果你要做 Rust 开发，Windows 下优先走 WSL。

## 2. 按系统选安装方式

| 系统 | 推荐方式 | 适用场景 |
| --- | --- | --- |
| Windows | WSL Ubuntu + 根级 `make` 命令 | 最推荐，适合本地开发和 Rust 调试 |
| macOS | 本机 Node + Rust + MongoDB | 本地开发 |
| Linux | 本机 Node + Rust + MongoDB | 本地开发、联调、测试 |
| Linux 服务器 | `server-install-nodocker.sh` + systemd + nginx | 服务器部署 |

注意：

- 本仓库不再提供应用层 Docker/Compose 部署入口，避免误导生产运维。
- 完整本地联调优先使用根目录 `make restart-all` 或 Windows 下 `make restart-all-wsl`。

## 3. 通用前置条件

建议统一准备这些基础依赖：

- Git
- Bash
- `make`
- Node.js 18 及以上，推荐 20 LTS
- npm
- Rust stable（通过 `rustup` 安装）
- MongoDB（本地完整联调可以使用外部 MongoDB，或按下文 Linux 本地 Mongo 脚本准备）

说明：

- 本仓库很多启动脚本是 Bash 脚本。
- `restart_all_services.sh`、`restart_services.sh`、`restart_task_runner_service.sh` 会在启动时构建 Rust 后端。
- 除 `memory_engine/frontend` 外，其它前端默认不会自动执行 `npm install`，首次使用前建议手动安装前端依赖。

## 4. 统一初始化步骤

### 4.1 复制根级环境变量

Windows PowerShell：

```powershell
Copy-Item .env.example .env
```

macOS / Linux / WSL：

```bash
cp .env.example .env
```

### 4.2 首次安装前端依赖

在仓库根目录执行：

```bash
npm --prefix chat_app install
npm --prefix user_service/frontend install
npm --prefix task_runner_service/frontend install
npm --prefix memory_engine/frontend install
```

可选：

```bash
cargo fetch
```

### 4.3 建议先认识这些关键配置

根级 [`.env.example`](./.env.example) 里最重要的是这些：

- `START_USER_SERVICE=1`
- `START_MEMORY_ENGINE=1`
- `START_CHATOS=1`
- `START_TASK_RUNNER=1`
- `START_DEV_MONGO=auto`
- `DATABASE_TYPE=mongodb`
- `MONGODB_HOST=127.0.0.1`
- `MONGODB_PORT=27018`
- `CHATOS_USER_SERVICE_BASE_URL=http://127.0.0.1:39190`
- `CHATOS_TASK_RUNNER_BASE_URL=http://127.0.0.1:39090`
- `TASK_RUNNER_CHATOS_CALLBACK_SECRET=change_me_chatos_task_runner_secret`
- `TASK_RUNNER_DATABASE_URL=mongodb://admin:admin@127.0.0.1:27018/task_runner_service?authSource=admin`
- `USER_SERVICE_DATABASE_URL=mongodb://admin:admin@127.0.0.1:27018/user_service?authSource=admin`
- `MEMORY_ENGINE_MONGODB_URI=mongodb://admin:admin@127.0.0.1:27018/admin`
- `MEMORY_ENGINE_OPERATOR_TOKEN=chatos-memory-engine-dev-operator-token`

当前默认状态：

- ChatOS 主后端默认走 MongoDB
- `task_runner_service` 默认走 MongoDB
- `memory_engine` 默认走 MongoDB
- `user_service` 默认走 MongoDB：`USER_SERVICE_DATABASE_URL=mongodb://admin:admin@127.0.0.1:27018/user_service?authSource=admin`

### 4.4 统一模型配置说明

- `user_service` 现在同时负责真实用户、agent 账号、以及用户级模型配置。
- ChatOS 里的模型配置允许只保存 `provider/base_url/api_key`，`model` 可以先留空。
- 真正发消息时，ChatOS 仍然是在拉取到供应商模型列表后再选具体模型。
- `task_runner_service` 和 `memory_engine` 只接收带具体 `model` 的可运行配置。
- 如果某条配置没有具体 `model`，保存时会返回 `sync_warnings`，表示下游同步被跳过，这不是保存失败。
- `memory engine` 的默认总结模型必须绑定到一条带具体 `model` 的配置。
- 仓库自带的本地启动脚本会默认使用 `MEMORY_ENGINE_OPERATOR_TOKEN=chatos-memory-engine-dev-operator-token`，方便本地联调。

## 5. Windows 安装：推荐 WSL Ubuntu

这是当前最推荐的 Windows 开发方式。

原因很直接：

- 当前仓库已经明确支持 WSL 调度流
- Windows 上可能被 `Smart App Control / Code Integrity` 拦截 `cargo run` / `cargo test`
- 把 Rust 后端放进 WSL 里跑，稳定性更高

### 5.1 安装 WSL

在 PowerShell 执行：

```powershell
wsl.exe --install -d Ubuntu
```

如果系统提示重启，按提示重启。

### 5.2 初始化 WSL 开发依赖

仍然在仓库根目录 PowerShell 执行：

```powershell
make bootstrap-wsl
```

这个脚本会在 Ubuntu / WSL 里安装：

- `build-essential`
- `pkg-config`
- `libssl-dev`
- `sqlite3`
- `libsqlite3-dev`
- `curl`
- `git`
- `make`
- `python3`
- `nodejs`
- `npm`
- `rustup` / `cargo`

### 5.3 启动服务

完整栈：

```powershell
make restart-all-wsl
make status-all-wsl
make stop-all-wsl
```

只启动 ChatOS 主服务：

```powershell
make restart-wsl
make status-wsl
make stop-wsl
```

只启动某个子服务：

```powershell
make restart-user-service-wsl
make restart-task-runner-wsl
make restart-memory-engine-wsl
```

### 5.4 相关补充文档

- [WSL_RUST_DEV_FLOW_20260619.md](./WSL_RUST_DEV_FLOW_20260619.md)
- [USER_SERVICE_LOCAL_RUNBOOK_20260619.md](./USER_SERVICE_LOCAL_RUNBOOK_20260619.md)

## 6. 部署约定：不走 Docker 容器路径

本仓库不再提供应用层 Dockerfile、Docker Compose 和 Docker 启动脚本。

生产/运维部署请按宿主机进程处理：

- Linux 服务器：构建 release 产物后执行 `sudo bash scripts/server-install-nodocker.sh`
- 服务托管：使用 systemd 管理后端进程
- 前端托管：使用 nginx 指向 `chat_app/dist`
- 数据库：使用宿主机或外部 MongoDB 服务

如果未来确实需要容器化，请单独维护部署仓库或环境专属编排，避免和本仓库的宿主机部署方式混在一起。

## 7. macOS 本地安装

如果你使用 Homebrew，可以按下面准备依赖。

### 7.1 安装基础依赖

```bash
brew install git make node
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup default stable
```

### 7.2 初始化项目

```bash
cp .env.example .env
npm --prefix chat_app install
npm --prefix user_service/frontend install
npm --prefix task_runner_service/frontend install
npm --prefix memory_engine/frontend install
cargo fetch
```

### 7.3 启动完整本地栈

```bash
make restart-all
make status-all
make stop-all
```

说明：

- `.env` 默认 `START_DEV_MONGO=auto`
- 建议准备本机或外部 MongoDB，并把 `.env` 里的 `MONGODB_*`、`TASK_RUNNER_DATABASE_URL`、`MEMORY_ENGINE_MONGODB_URI` 改成你自己的地址

## 8. Linux 本地安装

下面以 Debian / Ubuntu 为例。

### 8.1 安装依赖

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  sqlite3 \
  libsqlite3-dev \
  curl \
  ca-certificates \
  git \
  make \
  unzip \
  zip \
  python3 \
  python3-pip \
  file \
  lsof \
  net-tools \
  nodejs \
  npm
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup default stable
```

### 8.2 初始化项目

```bash
cp .env.example .env
npm --prefix chat_app install
npm --prefix user_service/frontend install
npm --prefix task_runner_service/frontend install
npm --prefix memory_engine/frontend install
cargo fetch
```

### 8.3 启动完整本地栈

```bash
make restart-all
make status-all
make stop-all
```

### 8.4 可选：本地直接跑 Mongo

如果你不想依赖 Docker，并且当前机器是兼容的 Linux x86_64 环境，可以尝试：

```bash
bash scripts/restart_local_mongo.sh start
bash scripts/restart_local_mongo.sh status
bash scripts/restart_local_mongo.sh stop
```

注意：

- 这个脚本会下载 Linux 版 MongoDB 二进制
- 这不是 macOS / Windows 的通用方案

## 9. Linux 服务器部署

### 9.1 宿主机路径

适合主 ChatOS 服务部署，使用 systemd 管理后端进程，使用 nginx 托管前端静态文件。

先构建产物：

```bash
cargo build --release --manifest-path chat_app_server_rs/Cargo.toml
npm --prefix chat_app ci
npm --prefix chat_app run build
```

再执行安装脚本：

```bash
sudo bash scripts/server-install-nodocker.sh
```

这条路径当前需要注意：

- 它主要部署 `chat_app_server_rs + chat_app`
- 不是完整四个微服务统一部署脚本
- 脚本依赖 `systemctl`、`nginx`、`rsync`

## 10. 默认端口

### 10.1 完整本地栈

- ChatOS backend：`3997`
- ChatOS frontend：`8088`
- `user_service` backend：`39190`
- `user_service` frontend：`39191`
- `task_runner_service` backend：`39090`
- `task_runner_service` frontend：`39091`
- `memory_engine` backend：`7081`
- `memory_engine` frontend：`4178`
- 开发用 MongoDB：`27018`

### 10.2 服务器宿主机路径

- ChatOS backend：默认 `13001`，可通过 `BACKEND_PORT` 覆盖
- ChatOS frontend：由 nginx 对外暴露，默认走站点配置的 HTTP 入口

## 11. 常用命令

```bash
make help
make restart
make status
make stop
make restart-all
make status-all
make stop-all
make restart-user-service
make restart-task-runner
make restart-memory-engine
make smoke
```

Windows + WSL 常用命令：

```powershell
make bootstrap-wsl
make restart-wsl
make restart-all-wsl
make status-all-wsl
make stop-all-wsl
```

## 12. 常见问题

### 12.1 为什么 Windows 上 `cargo run` / `cargo test` 可能失败

不是一定没装 Rust。
当前机器可能被 `Smart App Control / Code Integrity` 拦截 Rust 产物执行。
这种情况下直接改走 WSL，不要继续在原生 Windows 上硬顶。

### 12.2 为什么仓库里没有 Docker Compose 部署路径

当前项目按宿主机部署设计，应用层 Dockerfile、Compose 和 Docker 启动脚本会误导运维，所以不在本仓库保留。
完整联调请用根目录本地脚本方案，服务器部署请用 `scripts/server-install-nodocker.sh`。

### 12.3 根目录 `.env` 和 `chat_app_server_rs/.env` 有什么区别

- 根目录 `.env`：给本地根级启动脚本用
- `chat_app_server_rs/.env`：给主后端专属配置覆盖使用

### 12.4 我要先看哪个入口

如果你是：

- Windows 开发：先看 [WSL_RUST_DEV_FLOW_20260619.md](./WSL_RUST_DEV_FLOW_20260619.md)
- 本地完整联调：直接从本文第 4 节开始
- 服务器部署：直接看本文第 9 节宿主机路径
