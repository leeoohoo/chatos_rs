# Chatos RS

跨系统安装教程见 [INSTALL_GUIDE.zh-CN.md](./INSTALL_GUIDE.zh-CN.md)

## 项目定位
`Chatos RS` 是一个面向开发与工程协作场景的 AI 平台。  
它把对话交互、工具调用和长期记忆统一到一套系统中，目标是让 AI 能稳定地“持续工作”，而不是只做一次性聊天。

## 这个项目解决什么问题
传统聊天式 AI 在工程场景常见问题：
- 上下文只在当前会话内有效，跨会话信息容易丢失
- 历史越长 token 成本越高，推理效率下降
- 工具链路分散，接入和维护成本高
- 工具执行过程不透明时，工程工作流难以维护和排障

`Chatos RS` 的设计就是为了解决这些问题：  
通过“主对话服务 + 外部记忆平台接入 + MCP 风格工具编排”实现持续上下文、成本控制和可集成性。

## 核心优势
1. 长期记忆能力
- 支持会话总结、再总结、记忆沉淀，保留跨会话的关键事实、决策和待办。

2. 上下文成本可控
- 通过分层总结与定时任务压缩上下文，减少无效 token 消耗，同时保持连续性。

3. 工具协作友好
- 支持工具调用与 MCP 场景，适合接入工程工作流与外部能力。

4. 架构可扩展
- 前端、主后端、外部记忆平台解耦，支持独立部署与水平扩展。

5. 工程工作流可运维
- 让工具调用、任务执行和记忆上下文保持可观察、可维护。

## 架构分层
- `chat_app/`：主前端交互层
- `chat_app_server_rs/`：主后端编排层（会话、消息、工具、流式响应）

## 本地一键启动
在仓库根目录执行：

```bash
./restart_services.sh restart
```

统一根级任务入口：

```bash
make help
make build
make test
make smoke
```

`make smoke` 会执行仓库治理检查以及轻量级的跨子系统探测。
其中也包括根级启动脚本语法检查，以及 Git 关注文件的大文件策略检查。

共享本地配置入口：

- 根目录提供 [`.env.example`](./.env.example)
- `./restart_services.sh` 会先读取根目录 `.env`
- 如果存在 `chat_app_server_rs/.env`，主后端仍可用它覆盖后端专属配置

常用命令：

```bash
./restart_services.sh status
./restart_services.sh stop
```

## WSL Rust 开发流
如果 Windows 上的 Smart App Control / Code Integrity 会拦截 `cargo run` 或 `cargo test`，
优先改走 WSL 内运行 Rust，而不是直接在 Windows 上执行 Rust 产物。

一次性初始化：

```powershell
wsl.exe --install -d Ubuntu
make bootstrap-wsl
```

从 Windows 侧启动 ChatOS（实际运行在 WSL 内）：

```powershell
make restart-wsl
make status-wsl
make stop-wsl
```

只启动 `user_service`：

```powershell
make restart-user-service-wsl
make status-user-service-wsl
make stop-user-service-wsl
```

根目录 `.env` 可选配置：

- `WSL_DEV_DISTRO`
- `WSL_CARGO_TARGET_DIR`
- `WSL_RUNTIME_DIR`
- `WSL_USER_SERVICE_RUNTIME_DIR`

默认日志路径：
- `/tmp/chatos_rs_dev_<repo-hash>/backend.log`
- `/tmp/chatos_rs_dev_<repo-hash>/frontend.log`

## 开发方案归档
方案/评估/契约文档可能位于根目录历史文档，或本地 `docs/plans/` 归档目录。

## WSL 文档
- [WSL Rust 开发流](./WSL_RUST_DEV_FLOW_20260619.md)

## 子项目文档
- [chat_app English](./chat_app/README.en.md)
- [chat_app 中文](./chat_app/README.zh-CN.md)
- [chat_app_server_rs English](./chat_app_server_rs/README.en.md)
- [chat_app_server_rs 中文](./chat_app_server_rs/README.zh-CN.md)
- [db_connection_hub backend](./db_connection_hub/backend/README.md)
- [db_connection_hub frontend](./db_connection_hub/frontend/README.md)

## 许可协议
本项目采用 [PolyForm Noncommercial License 1.0.0](./LICENSE) 以源码可见方式发布。
未经版权持有人另行书面授权，不允许将本项目用于商业用途。
可以用 `python3 scripts/apply_license_headers.py` 检查第一方源码头部声明。
可以用 `python3 scripts/apply_license_headers.py --write` 批量补齐缺失声明。
