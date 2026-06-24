# Windows 本地全栈启动

入口脚本：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action restart
```

常用动作：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action start
powershell -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action status
powershell -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action stop
```

这个脚本会做这些事：

- 在 WSL 里启动本地 MongoDB，端口 `27018`
- 在 Windows 上启动：
  - `user_service_backend` `39190`
  - `project_management_service_backend` `39210`
  - `task_runner_service_backend` `39090`
  - `memory_engine` `7081`
  - `chat_app_server_rs` `3997`
  - `user_service/frontend` `39191`
  - `project_management_service/frontend` `39211`
  - `task_runner_service/frontend` `39091`
  - `memory_engine/frontend` `4178`
  - `chat_app` `8088`
- 自动等待健康检查
- 自动补齐 Windows 下缺失的 `rollup` / `esbuild` 原生可选包

默认管理员账号：

- 用户名：`admin`
- 密码：`admin123456`

日志目录：

```text
.local/run
```

注意：

- 这个脚本固化的是当前已验证可工作的 Windows 本地联调路径。
- `project_management_service` 在这个入口下默认走 SQLite 运行库文件，避免污染仓库内已有数据，同时减少 Windows 本地启动摩擦。
- `task_runner_service` 在这个脚本里已切到 Mongo 运行，避开当前仓库里 SQLite 迁移版本重复的问题。
- `chat_app_server_rs` 在这个脚本里优先直接运行现有 `target-shared\debug\chat_app_server_rs.exe`，避开 Windows App Control 对新 target build-script 的拦截。
- 微服务之间仍然只通过 HTTP / 配置对接，不做直接文件引用。
