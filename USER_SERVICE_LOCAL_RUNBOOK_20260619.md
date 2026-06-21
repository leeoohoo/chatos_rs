# User Service 本地联调说明

这份说明对应当前已经落下来的统一用户服务实现。

## 目标

本地跑通这条链路：

1. `user_service` 统一管理真实用户和 agent 账号
2. `chatos` 代理真实用户登录到 `user_service`
3. 真实用户查看和选择自己名下的 agent 账号
4. `chatos` 用真实用户 token + `task_runner_agent_account_id` 向 `user_service` 换取 Task Runner 短期 token
5. `task_runner` 校验这个委托 token

## 一次性准备

1. 在仓库根目录基于 [`.env.example`](./.env.example) 创建 `.env`
2. 至少确认这些变量：

- `CHATOS_USER_SERVICE_BASE_URL=http://127.0.0.1:39190`
- `CHATOS_USER_SERVICE_JWT_SECRET=change_me_user_service_secret`
- `CHATOS_USER_SERVICE_JWT_ISSUER=user_service`
- `CHATOS_USER_SERVICE_USER_AUDIENCE=user_service`
- `TASK_RUNNER_USER_SERVICE_JWT_SECRET=change_me_user_service_secret`
- `TASK_RUNNER_USER_SERVICE_JWT_ISSUER=user_service`
- `TASK_RUNNER_USER_SERVICE_TASK_RUNNER_AUDIENCE=task_runner`
- `USER_SERVICE_JWT_SECRET=change_me_user_service_secret`

本地联调时，这三侧需要先共享同一套 `user_service` JWT secret。

Windows 环境建议直接使用 Git Bash 执行下面的 `.sh` 命令。

如果 Windows 上 `cargo run` 被 Smart App Control / Code Integrity 拦截，优先改走仓库内置的 WSL 开发流：

```powershell
wsl.exe --install -d Ubuntu
make bootstrap-wsl
make restart-user-service-wsl
make restart-wsl
```

## 启动 user_service

直接启动：

```bash
bash user_service/restart_services.sh restart
```

查看状态：

```bash
make status-user-service
```

停止：

```bash
make stop-user-service
```

默认地址：

- backend: `http://127.0.0.1:39190`
- frontend: `http://127.0.0.1:39191`

## Docker Compose 启动

只启动统一用户服务：

```bash
docker compose up -d user-service-backend user-service-frontend
```

连同 ChatOS 主服务一起启动：

```bash
docker compose up -d user-service-backend user-service-frontend backend frontend
```

当前说明：

- `docker compose config` 已校验通过
- 这台机器当前没有可用的 Docker daemon，所以我这里没有继续做镜像实际构建验证
- 仓库内已补充 `crates/memory_engine_sdk`，`chat_app_server_rs` 与 `task_runner_service/backend` 的 `cargo check` 已在 2026-06-19 验证通过
- 已增加可复用的 API 冒烟脚本：`powershell.exe -ExecutionPolicy Bypass -File scripts/smoke-user-service-flow.ps1`

## 启动 ChatOS

```bash
./restart_services.sh restart
```

如果根目录 `.env` 里保留：

- `START_USER_SERVICE=1`
- `CHATOS_USER_SERVICE_BASE_URL=http://127.0.0.1:39190`

那么根启动脚本会先自动拉起本地 `user_service`，再启动 ChatOS 主前后端。

默认地址：

- backend: `http://127.0.0.1:3997`
- frontend: `http://127.0.0.1:8088`

## 当前关键行为

- `user_service` 首次启动会创建默认 `super_admin`
- 默认账号为 `admin / admin123456`
- 真实用户可以在 `user_service` 中创建自己的 agent 账号
- ChatOS 联系人的 Task Runner 配置主路径已经切到 `task_runner_agent_account_id`

## 推荐联调顺序

1. 启动 `user_service`
2. 用默认管理员登录 `http://127.0.0.1:39191`
3. 创建真实用户
4. 用真实用户登录，创建该用户自己的 agent 账号
5. 启动 `chatos`
6. 在 ChatOS 中登录该真实用户
7. 给联系人配置 `Task Runner base URL + agent account`
8. 触发一次需要 Task Runner 的运行，确认 `user_service` 能正常换发 token

## 快速冒烟

在 `user_service/backend` 已启动后，可直接执行：

```bash
make smoke-user-service-flow
```

或：

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts/smoke-user-service-flow.ps1
```

这个脚本会自动验证：

- 注册一个新的真实用户
- 用该用户创建一个属于自己的 agent 账号
- 使用该用户 token 为该 agent 账号换取 Task Runner token
- 校验返回 token 的 `principal_type`、`agent_account_id` 与 `owner_user_id`

## 当前已知限制

- 这台机器当前没有可用的 Docker daemon，所以本次只验证了 `docker compose config`，没有实际执行镜像 build/run
- `user_service/frontend` 的 `vite build` 在 Codex 沙箱内会因为临时配置文件写入权限失败；在真实工作目录执行 `npm.cmd run build` 已验证通过
- `chat_app_server_rs` 的 `cargo run` 在这台机器上会被 Windows application control 拦截部分 Cargo build-script 可执行文件，因此本次未完成 ChatOS 运行态代理冒烟
- 仓库已提供 [scripts/chatos-wsl.ps1](./scripts/chatos-wsl.ps1) 与 [scripts/bootstrap-wsl-dev.sh](./scripts/bootstrap-wsl-dev.sh) 作为 Windows -> WSL 的统一开发入口
