# MCP 工具整合与拆分进度

更新日期：2026-07-08

## 2026-07-08 本轮续更

本轮继续按 breaking refactor 推进，没有恢复旧工具名、旧 route 或旧 relay message type 兼容。

新增/调整的 Local Connector client 模块：

1. `local_connector_client/core/src/connector.rs`：承接 websocket 连接循环、心跳、relay 消息分发，只接受新的 `type: "mcp"` MCP relay envelope。
2. `local_connector_client/core/src/config.rs`：承接 `ClientConfig`、默认 URL/端口、env 读取、状态路径、URL 拼接和字符串规范化 helper。
3. `local_connector_client/core/src/state.rs`：承接 `LocalState`、`AuthState`、`AuthUserState`、`WorkspaceState` 以及状态文件 load/save 和 workspace/pairing 查询方法。
4. `local_connector_client/core/src/registration.rs`：承接设备注册、workspace 注册、设备断开、env bootstrap 和远端 HTTP 状态检查。
5. `local_connector_client/core/src/api.rs`：承接本地 HTTP API 路由、handler、请求/响应 DTO、API error、status payload 和 sandbox pairing 更新。
6. `local_connector_client/core/src/runtime.rs`：承接 `LocalRuntime`、运行时构造、已保存 workspace 同步和 connector task 启停。
7. `local_connector_client/core/src/mcp/terminal.rs`：承接 `local_connector/terminal/start` 和 `local_connector/terminal/cleanup` 本地 MCP 扩展方法。

本轮结构结果：

- `local_connector_client/core/src/main.rs` 的非测试职责已收敛到启动入口、少量通用 helper 和测试模块。
- `main.rs` 当前约 928 行，其中主要体量为既有测试；非测试函数只剩 `main`、`local_now_rfc3339`、`select_local_shell`、`tracing_stdout`。
- 旧 root helper 依赖已从 `terminal/relay.rs`、`terminal/exec.rs` 改为直接引用 `workspace::paths`，减少隐式 re-export 耦合。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（二）

继续压缩 Local Connector client 的大文件，重点处理 terminal controller 和 API 层。

新增/调整模块：

1. `local_connector_client/core/src/terminal/controller/registry.rs`：承接 Local MCP terminal session 类型、全局 registry、日志追加、状态刷新、会话查询和输出采集。
2. `local_connector_client/core/src/terminal/controller/store.rs`：承接 `TerminalControllerStore` trait 实现，包括 execute、logs、process list/poll/log/wait/write/kill。
3. `local_connector_client/core/src/api/types.rs`：承接本地 API DTO 和 `LocalApiError`，让 `api.rs` 聚焦 handler 编排。
4. `local_connector_client/core/src/terminal/controller/output.rs` 改为从 `registry` 模块引用 terminal log 类型。

本轮结构结果：

- `terminal/controller.rs` 从约 1249 行降到约 582 行。
- `api.rs` 从约 655 行降到约 553 行。
- `terminal/controller/store.rs` 当前约 381 行，`terminal/controller/registry.rs` 当前约 335 行。
- `main.rs` 当前约 928 行，但主要体量是既有测试；非测试逻辑仍保持为启动入口和少量 helper。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（三）

继续压缩入口和 API 组织边界，仍然保持 breaking refactor，不恢复旧工具名、旧 route 或旧 relay message type。

新增/调整模块：

1. `local_connector_client/core/src/tests.rs`：承接原 `main.rs` 内联测试模块。
2. `local_connector_client/core/src/api/handlers.rs`：承接本地 API handler、status payload、sandbox pairing 更新和 handler 局部 helper。
3. `local_connector_client/core/src/api.rs`：收敛为本地 API 路由装配、监听地址解析和 CORS 配置。

本轮结构结果：

- `main.rs` 从约 928 行降到约 94 行，当前只保留模块声明、入口启动、根常量和少量跨模块 helper。
- `tests.rs` 当前约 813 行，是原 main 内联测试的机械迁移。
- `api.rs` 从约 553 行降到约 70 行。
- `api/handlers.rs` 当前约 504 行。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（四）

继续处理 Local Connector client 的剩余大文件边界。

新增/调整模块：

1. `local_connector_client/core/src/sandbox/proxy.rs`：承接本地 sandbox MCP `/mcp` proxy、agent endpoint 校验、HTTP 响应转发和 sandbox tool call history 记录。
2. `local_connector_client/core/src/sandbox/mod.rs`：新增 `proxy` 子模块导出。
3. `local_connector_client/core/src/sandbox/relay.rs`：收敛为本地 sandbox relay 路由、lease 生命周期、health/release 编排。

本轮结构结果：

- `sandbox/relay.rs` 从约 499 行降到约 402 行。
- `sandbox/proxy.rs` 当前约 110 行。
- 旧协议方向继续保持：sandbox proxy 只转发标准 JSON-RPC `/mcp`，未恢复 `/mcp/tools` 或 `/mcp/call`。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（五）

继续拆分 Local Connector client 的 sandbox workspace 相关逻辑。

新增/调整模块：

1. `local_connector_client/core/src/sandbox/manifest.rs`：承接 local sandbox output change manifest、文件索引、SHA256、变更计数和 manifest 摘要。
2. `local_connector_client/core/src/sandbox/workspace.rs`：收敛为 run workspace 路径、workspace 准备/复制、baseline 路径、请求 body 注入和目录清理。
3. `local_connector_client/core/src/sandbox/relay.rs`：改为从 `sandbox::manifest` 引用 `summarize_local_sandbox_manifest_counts`。

本轮结构结果：

- `sandbox/workspace.rs` 从约 378 行降到约 225 行。
- `sandbox/manifest.rs` 当前约 165 行。
- `main.rs` 保持约 94 行，`api.rs` 保持约 70 行。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（六）

继续收紧 Local Connector client 的 history 记录边界，保持 breaking refactor，不恢复旧工具名、旧 route 或旧 relay message type。

新增/调整模块：

1. `local_connector_client/core/src/history/format.rs`：承接 history 输出预览、命令显示文本格式化、文本截断和 compact JSON helper。
2. `local_connector_client/core/src/history/sandbox.rs`：承接 sandbox tool call 参数解析、命令展示字段构造、sandbox tool result preview 和错误信息抽取。
3. `local_connector_client/core/src/history.rs`：收敛为 history entry 构造、recorder、终端执行/交互提交记录和跨模块编排。

本轮结构结果：

- `history.rs` 当前约 285 行，sandbox 解析和通用格式化 helper 已从主 history 编排逻辑里拆出。
- `main.rs` 保持约 94 行，仍只承担入口、模块声明、根常量和少量跨模块 helper。
- 当前 Local Connector client 最大文件主要集中在测试、terminal controller、API handlers、sandbox relay/images，后续可继续按风险从 API handlers 或 sandbox images 下手。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（七）

继续拆分 Local Connector client 的本地 HTTP API handler 层，路由路径保持当前新协议形态，不恢复任何旧 MCP façade route。

新增/调整模块：

1. `local_connector_client/core/src/api/handlers/auth.rs`：承接 login/register/logout、本地鉴权状态写入、设备下线标记和鉴权后 connector 启动。
2. `local_connector_client/core/src/api/handlers/workspace.rs`：承接文件夹列表、workspace 注册和移除。
3. `local_connector_client/core/src/api/handlers/sandbox.rs`：承接 Docker 状态、本地 sandbox 开关、image/job/lease 查询、image 初始化和 sandbox pairing 同步。
4. `local_connector_client/core/src/api/handlers/terminal.rs`：承接本地 UI 触发的 terminal exec relay。
5. `local_connector_client/core/src/api/handlers/history.rs`：承接 command history 查询和清空。
6. `local_connector_client/core/src/api/handlers/status.rs`：承接 status payload 构造。
7. `local_connector_client/core/src/api/handlers/helpers.rs`：承接 API handler 共享输入校验 helper。

本轮结构结果：

- `api/handlers.rs` 从约 504 行降到约 20 行，只保留子模块声明和 handler re-export。
- 新拆出的 handler 子模块最大约 142 行，HTTP API 的 auth/workspace/sandbox/terminal/history/status 边界更清楚。
- `api.rs` 继续只负责 route 装配、监听地址和 CORS，不承载 handler 业务逻辑。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（八）

继续拆分 Local Connector client 的 local sandbox image 构建逻辑，保持本地 sandbox API 使用当前新协议入口，不恢复旧 MCP helper route。

新增/调整模块：

1. `local_connector_client/core/src/sandbox/images/build.rs`：承接 Docker image inspect、build context/dockerfile 路径解析、feature 规范化和本地 image id 生成。
2. `local_connector_client/core/src/sandbox/images/job.rs`：承接 Docker build 子进程、stdout/stderr 日志流采集、job output 截断、job 状态完成和成功后 selected image 保存。
3. `local_connector_client/core/src/sandbox/images.rs`：收敛为 image catalog 和 image build job 启动入口。

本轮结构结果：

- `sandbox/images.rs` 从约 397 行降到约 107 行。
- Docker build runtime 集中在 `sandbox/images/job.rs`，当前约 216 行。
- image 参数、路径和 id 纯 helper 集中在 `sandbox/images/build.rs`，避免入口模块继续承载细节。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（九）

继续拆分 Local Connector client 的 local sandbox relay 层，将路径分发和 lease 生命周期解耦。

新增/调整模块：

1. `local_connector_client/core/src/sandbox/lease.rs`：承接 lease 创建、image 选择、lease 响应、sandbox 查询、health 检查、release/export/destroy 生命周期。
2. `local_connector_client/core/src/sandbox/relay.rs`：收敛为 relay envelope 解析、本地 sandbox HTTP path 分发、标准 `/mcp` proxy 接入和 404 响应。
3. `local_connector_client/core/src/sandbox/mod.rs`：新增 `lease` 模块声明。

本轮结构结果：

- `sandbox/relay.rs` 从约 403 行降到约 125 行。
- `sandbox/lease.rs` 当前约 292 行，集中承载 local sandbox 生命周期，不再混在 relay 分发里。
- local sandbox MCP proxy 仍只走 POST `/api/sandboxes/:sandbox_id/mcp` 到标准 JSON-RPC `/mcp`，未恢复 `/mcp/tools` 或 `/mcp/call`。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十）

继续拆分 Local Connector client 的 Terminal Controller 组，重点收紧 reusable shell、standalone command、store trait 和 registry 的边界。

新增/调整模块：

1. `local_connector_client/core/src/terminal/controller/reused.rs`：承接 reusable primary shell 命令执行、active marker、sentinel exit code 解析和超时后 marker 清理。
2. `local_connector_client/core/src/terminal/controller/standalone.rs`：承接 standalone terminal command 启动、等待、输出聚合和 response 构造。
3. `local_connector_client/core/src/terminal/controller/store/logs.rs`：承接 TerminalControllerStore 的 recent logs 查询实现。
4. `local_connector_client/core/src/terminal/controller/store/process.rs`：承接 process list/poll/log/wait/write/kill 实现。
5. `local_connector_client/core/src/terminal/controller/registry/types.rs`：承接 terminal registry/session/meta/log/wait result 类型。
6. `local_connector_client/core/src/terminal/controller/registry/logs.rs`：承接 terminal log offset、append 和输出聚合 helper。

本轮结构结果：

- `terminal/controller.rs` 从约 582 行降到约 215 行。
- `terminal/controller/store.rs` 从约 381 行降到约 144 行。
- `terminal/controller/registry.rs` 从约 335 行降到约 228 行。
- Terminal Controller 的入口、store trait、registry、reusable shell 和 standalone command 责任边界更清楚；未改变 MCP 工具名、route 或 relay envelope。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十一）

继续拆分 Local Connector client 的 terminal guard 和 PTY session 层，保持交互式 terminal 行为不变。

新增/调整模块：

1. `local_connector_client/core/src/terminal/guard/parser.rs`：承接 cd/chdir/set-location/pushd/popd 解析、shell word split、动态 cd 语法判断和 cd target resolve。
2. `local_connector_client/core/src/terminal/guard/path.rs`：承接 workspace 内路径判断、路径规范化和命令参数路径越界校验。
3. `local_connector_client/core/src/terminal/guard/text.rs`：承接 terminal input 换行规范化、命令行 ANSI 清理和输入行清空序列构造。
4. `local_connector_client/core/src/terminal/session/input.rs`：承接交互式输入写入、目录防护应用、blocked message 回写和 command submission 记录。
5. `local_connector_client/core/src/terminal/session/output.rs`：承接 PTY resize、snapshot、busy 状态、输出历史截断和 close。

本轮结构结果：

- `terminal/guard.rs` 从约 338 行降到约 95 行。
- `terminal/session.rs` 从约 325 行降到约 163 行。
- guard 的解析、路径、文本 helper 分层后更容易单独测试和维护；session 入口保留 PTY 启动、reader/waiter thread 和 session registry。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十二）

继续拆分 Local Connector client 的 terminal relay handler 层，保持原有新 relay handler 函数路径对内部调用方不变。

新增/调整模块：

1. `local_connector_client/core/src/terminal/relay/types.rs`：承接 terminal session create/input/resize/snapshot/close relay DTO。
2. `local_connector_client/core/src/terminal/relay/create.rs`：承接 terminal session create handler、workspace/cwd/root 解析和 create response 构造。
3. `local_connector_client/core/src/terminal/relay/control.rs`：承接 terminal input、resize、snapshot 和 close handler。
4. `local_connector_client/core/src/terminal/relay.rs`：收敛为子模块声明和 handler re-export。

本轮结构结果：

- `terminal/relay.rs` 从约 275 行降到约 12 行。
- terminal relay 的 create 与 control handler 分离，DTO 独立管理。
- 这次只拆内部模块，不改变 relay message type，也不恢复旧 MCP route。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十三）

继续拆分 Local Connector client 的 Docker、workspace path 和 MCP tool factory 层，保持所有当前新协议入口不变。

新增/调整模块：

1. `local_connector_client/core/src/sandbox/docker/status.rs`：承接 Docker status、ensure running、Docker Desktop 启动和命令捕获 helper。
2. `local_connector_client/core/src/sandbox/docker/container.rs`：承接 local sandbox 容器 run/inspect/rm、agent port 发现和 agent health wait。
3. `local_connector_client/core/src/sandbox/docker.rs`：收敛为 Docker 子模块入口和 re-export。
4. `local_connector_client/core/src/workspace/paths/normalize.rs`：承接 request workspace path 归一化、connector URI 解析、绝对路径折算和相对路径清洗。
5. `local_connector_client/core/src/workspace/paths/resolve.rs`：承接 workspace 路径解析、目录校验、relative path 和 fingerprint。
6. `local_connector_client/core/src/workspace/paths.rs`：收敛为 workspace path 入口、request cwd/helper 和 re-export。
7. `local_connector_client/core/src/mcp/tools/project.rs`：承接 MCP 请求 project root 和 project-relative path 归一化。
8. `local_connector_client/core/src/mcp/tools/code.rs`：承接 Code Maintainer service factory、参数归一化和 structured result helper。
9. `local_connector_client/core/src/mcp/tools/browser.rs`：承接 Browser Tools service factory、conversation id 和 service registry。
10. `local_connector_client/core/src/mcp/tools/terminal_controller.rs`：承接 Terminal Controller tool 调用、execute_command 历史记录包装。
11. `local_connector_client/core/src/mcp/tools.rs`：收敛为 MCP tool helper 子模块入口和 re-export。

本轮结构结果：

- `sandbox/docker.rs` 从约 293 行降到约 14 行。
- `workspace/paths.rs` 从约 289 行降到约 39 行。
- `mcp/tools.rs` 从约 265 行降到约 15 行。
- Docker 状态/容器生命周期、workspace normalize/resolve、MCP tool factory/wrapper 的职责边界更清楚。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十四）

继续拆分 Local Connector client 的 command history 层，把 `history.rs` 收敛为薄入口，不改变 history 记录语义，也不恢复任何旧 MCP 兼容入口。

新增/调整模块：

1. `local_connector_client/core/src/history/types.rs`：承接 `CommandHistoryEntry`、`CommandHistoryRecorder`、`CommandExecutionContext`、history 容量上限和 append 持久化逻辑。
2. `local_connector_client/core/src/history/entries.rs`：承接 terminal exec、interactive terminal submission、sandbox tool call 的 history entry 构造，以及 `output_text` 和 `normalize_history_source`。
3. `local_connector_client/core/src/history.rs`：收敛为 `entries`、`format`、`sandbox`、`types` 子模块入口和必要 re-export。

本轮结构结果：

- `history.rs` 从约 285 行降到约 16 行。
- `history/entries.rs` 当前约 175 行，集中承载跨 terminal/sandbox/MCP 的 history entry builder。
- `history/types.rs` 当前约 102 行，集中承载 history 数据结构和 recorder。
- command history 的类型、格式化、sandbox 解析和 entry 构造职责已分离。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十五）

继续拆分 Local Connector client 的 Terminal Controller reusable shell 层，把 sentinel marker、等待和清理细节从主执行流程中分离出来。

新增/调整模块：

1. `local_connector_client/core/src/terminal/controller/reused/marker.rs`：承接 reusable shell done marker 等待、exit code 解析、active marker 清理和 timeout 后后台清理任务。
2. `local_connector_client/core/src/terminal/controller/reused.rs`：保留 reusable shell 命令提交、输出收集、primary shell session 查找和 busy 判断。

本轮结构结果：

- `terminal/controller/reused.rs` 从约 268 行降到约 163 行。
- `terminal/controller/reused/marker.rs` 当前约 116 行，集中承载 sentinel marker 生命周期。
- reusable shell 主流程与 marker/wait 机制分离，后续若要调整 timeout 或 marker 格式，影响面更小。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十六）

继续拆分 Local Connector client 的 Terminal Controller store process 层，把 process 查询类接口和控制类接口分开，保持 `TerminalControllerStore` trait 实现调用路径不变。

新增/调整模块：

1. `local_connector_client/core/src/terminal/controller/store/process/query.rs`：承接 `process_list`、`process_poll`、`process_log`，集中处理终端进程视图、日志分页和输出预览。
2. `local_connector_client/core/src/terminal/controller/store/process/control.rs`：承接 `process_wait`、`process_write`、`process_kill`，集中处理等待、输入写入和终止。
3. `local_connector_client/core/src/terminal/controller/store/process.rs`：收敛为 process 子模块入口和 re-export。

本轮结构结果：

- `terminal/controller/store/process.rs` 从约 248 行降到约 6 行。
- `terminal/controller/store/process/query.rs` 当前约 159 行。
- `terminal/controller/store/process/control.rs` 当前约 101 行。
- Terminal Controller process API 的查询面和控制面分离，后续维护输出 schema 或进程控制行为时影响边界更明确。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 2026-07-08 本轮续更（十七）

继续压缩 Local Connector client 剩余较大的业务文件，重点拆 `terminal/exec`、Terminal Controller registry 和 local sandbox workspace。仍然保持 breaking refactor，不恢复旧工具名、旧 route 或旧 relay message type。

新增/调整模块：

1. `local_connector_client/core/src/terminal/exec/runner.rs`：承接 terminal exec 请求 DTO、命令执行、timeout 处理、stdout/stderr 截断和 history 写入。
2. `local_connector_client/core/src/terminal/exec.rs`：收敛为 relay envelope 解析和 `terminal_response` 组装。
3. `local_connector_client/core/src/terminal/controller/registry/lifecycle.rs`：承接 terminal session 注册、stdout/stderr reader、状态刷新和 exited 标记。
4. `local_connector_client/core/src/terminal/controller/registry/query.rs`：承接按 context 查询 session、按 terminal id 查找和 wait 逻辑。
5. `local_connector_client/core/src/terminal/controller/registry.rs`：收敛为 registry 单例入口、子模块声明和 re-export。
6. `local_connector_client/core/src/sandbox/workspace/fs.rs`：承接 workspace 清理、复制和 `.chatos` 跳过规则。
7. `local_connector_client/core/src/sandbox/workspace/paths.rs`：承接 sandbox workspace root、baseline workspace 和 run id path segment 规范化。
8. `local_connector_client/core/src/sandbox/workspace/request.rs`：承接 sandbox lease create request body 注入和路径判断。
9. `local_connector_client/core/src/sandbox/workspace.rs`：保留 run workspace 创建、workspace prepare 和 output export 编排。

本轮结构结果：

- `terminal/exec.rs` 从约 218 行降到约 58 行，执行主体迁到 `terminal/exec/runner.rs`。
- `terminal/controller/registry.rs` 从约 214 行降到约 28 行，lifecycle/query/logs/types 分层完成。
- `sandbox/workspace.rs` 从约 210 行降到约 97 行，fs/paths/request helper 分离。
- 当前最大非测试业务文件主要剩 `terminal/exec/runner.rs`、`sandbox/images/job.rs`、`terminal/controller.rs`、`sandbox/catalog.rs`，后续可继续按同样粒度拆。

本轮验证：

- `cargo fmt --all` 通过。
- `cargo check -p local_connector_client_core` 通过。
- `cargo test -p local_connector_client_core --no-run` 通过。
- 旧协议残留扫描已重跑，命中只剩 `handle_mcp_request` 函数名，不包含旧 route、旧 relay message type 或旧 sandbox tool alias。

## 当前策略

本轮按 breaking refactor 推进，不再保持旧工具名、旧 route、旧 relay message type 的运行时兼容。目标是收敛到标准 MCP JSON-RPC `/mcp`、共享 provider/catalog/policy 模型，以及更清晰的模块边界。

## 已完成

| 阶段 | 状态 | 产出 |
| --- | --- | --- |
| Phase 0 | 已建立初始基线 | 部署入口已迁移为 Docker-first，旧远程部署脚本不再作为发布路径 |
| Phase 1 | 已完成首版 | 新增 `crates/chatos_mcp_service` crate |
| Phase 1 | 已完成首版 | 新增共享 `JsonRpcRequest`、`JsonRpcResponse`、`McpToolProvider`、`McpJsonRpcService` |
| Phase 1 | 已完成本轮增量 | 新增共享 `catalog` 工具：`tool_name`、`sort_tools_by_name`、`tool_name_set` |
| Phase 4 前置 | 已完成首版 | `chatos_sandbox_mcp_server` 接入 `chatos_mcp_service` |
| Phase 4 breaking | 已完成首版 | Sandbox MCP server 删除 `/mcp/tools`、`/mcp/call`、`/terminal/exec`、`/files/*` 旧兼容 route |
| Phase 4 拆分 | 已完成首版 | Sandbox MCP server 抽出 `config.rs`、`auth.rs`、`tools/provider.rs` |
| Phase 5 breaking | 已完成首版 | Sandbox Manager backend 删除 `/api/sandboxes/:sandbox_id/mcp/tools` 和 `/api/sandboxes/:sandbox_id/mcp/call` façade |
| Local Connector breaking | 已完成首版 | 本地 sandbox façade 删除 `/mcp/tools` 和 `/mcp/call` 分支，只保留 `/mcp` |
| Local Connector MCP | 已完成首版 | 标准 `initialize`、`ping`、`tools/list`、`tools/call` 改为复用 `chatos_mcp_service::McpJsonRpcService` |
| Local Connector 拆分 | 已完成首版 | MCP 工具选择和工具名分类抽到 `local_connector_client/core/src/mcp/selection.rs` |
| Local Connector 拆分 | 已完成本轮增量 | Relay envelope 抽到 `local_connector_client/core/src/relay/messages.rs` |
| Local Connector 拆分 | 已完成本轮增量 | MCP JSON-RPC dispatch 抽到 `mcp/service.rs`，provider 抽到 `mcp/provider.rs` |
| Local Connector 拆分 | 已完成本轮增量 | Local API handlers 按 auth/workspace/sandbox/terminal/history/status 拆到 `local_connector_client/core/src/api/handlers/` |
| Local Connector 拆分 | 已完成本轮增量 | PTY terminal session 管理抽到 `local_connector_client/core/src/terminal/session.rs` |
| Local Connector 拆分 | 已完成本轮增量 | terminal session relay handler 抽到 `local_connector_client/core/src/terminal/relay.rs` |
| Local Connector 拆分 | 已完成本轮增量 | terminal session relay create/control/types 抽到 `local_connector_client/core/src/terminal/relay/` |
| Local Connector 拆分 | 已完成本轮增量 | terminal exec relay 和执行逻辑抽到 `local_connector_client/core/src/terminal/exec.rs` |
| Local Connector 拆分 | 已完成本轮增量 | terminal exec runner 和 history 写入抽到 `local_connector_client/core/src/terminal/exec/runner.rs` |
| Local Connector 拆分 | 已完成本轮增量 | PTY terminal session input/output helper 抽到 `local_connector_client/core/src/terminal/session/` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox state/runtime/DTO 类型抽到 `local_connector_client/core/src/sandbox/types.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox runtime catalog 抽到 `local_connector_client/core/src/sandbox/catalog.rs` |
| Local Connector 拆分 | 已完成本轮增量 | workspace/path helper 抽到 `local_connector_client/core/src/workspace/paths.rs` |
| Local Connector 拆分 | 已完成本轮增量 | workspace path normalize / resolve helper 抽到 `local_connector_client/core/src/workspace/paths/` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox image catalog / Docker build job 抽到 `local_connector_client/core/src/sandbox/images.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox image build helper 抽到 `local_connector_client/core/src/sandbox/images/build.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox Docker build runtime / log helper 抽到 `local_connector_client/core/src/sandbox/images/job.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox Docker status/runtime helper 抽到 `local_connector_client/core/src/sandbox/docker.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox Docker status / container helper 抽到 `local_connector_client/core/src/sandbox/docker/` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox workspace prepare/export/diff helper 抽到 `local_connector_client/core/src/sandbox/workspace.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox workspace fs/paths/request helper 抽到 `local_connector_client/core/src/sandbox/workspace/` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox lease lifecycle 抽到 `local_connector_client/core/src/sandbox/lease.rs` |
| Local Connector 拆分 | 已完成本轮增量 | local sandbox lease/proxy relay 编排抽到 `local_connector_client/core/src/sandbox/relay.rs` |
| Local Connector 拆分 | 已完成本轮增量 | Local MCP terminal controller store / registry 抽到 `local_connector_client/core/src/terminal/controller.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller.rs` 内部继续拆出 `controller/output.rs` 和 `controller/shell.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller.rs` 内部继续拆出 `controller/reused.rs` 和 `controller/standalone.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller/reused.rs` 内部继续拆出 `reused/marker.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller/store.rs` 内部继续拆出 `store/logs.rs` 和 `store/process.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller/store/process.rs` 内部继续拆出 `process/query.rs` 和 `process/control.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller/registry.rs` 内部继续拆出 `registry/types.rs` 和 `registry/logs.rs` |
| Local Connector 拆分 | 已完成本轮增量 | `terminal/controller/registry.rs` 内部继续拆出 `registry/lifecycle.rs` 和 `registry/query.rs` |
| Local Connector 拆分 | 已完成本轮增量 | command history recorder / entry 构造 / sandbox tool preview 抽到 `local_connector_client/core/src/history.rs` |
| Local Connector 拆分 | 已完成本轮增量 | history 输出格式化 helper 抽到 `local_connector_client/core/src/history/format.rs` |
| Local Connector 拆分 | 已完成本轮增量 | sandbox tool call/result preview 解析抽到 `local_connector_client/core/src/history/sandbox.rs` |
| Local Connector 拆分 | 已完成本轮增量 | command history 类型、recorder 和执行上下文抽到 `local_connector_client/core/src/history/types.rs` |
| Local Connector 拆分 | 已完成本轮增量 | command history entry builder、source normalize 和 output text helper 抽到 `local_connector_client/core/src/history/entries.rs` |
| Local Connector 拆分 | 已完成本轮增量 | MCP 工具宿主 service factory / 参数归一化抽到 `local_connector_client/core/src/mcp/tools.rs` |
| Local Connector 拆分 | 已完成本轮增量 | MCP tool project/code/browser/terminal-controller helper 抽到 `local_connector_client/core/src/mcp/tools/` |
| Local Connector 拆分 | 已完成本轮增量 | terminal directory guard / input sanitizer 抽到 `local_connector_client/core/src/terminal/guard.rs` |
| Local Connector 拆分 | 已完成本轮增量 | terminal guard parser/path/text helper 抽到 `local_connector_client/core/src/terminal/guard/` |
| Local Connector breaking | 已完成本轮增量 | MCP relay message type 从旧 `mcp_request` / `mcp_response` 统一为 `type: "mcp"` |
| Sandbox Agent breaking | 已完成本轮增量 | Python agent 删除 `/mcp/tools`、`/mcp/call` helper route 和旧 `sandbox_*` 工具名 alias，并补齐 `initialize` / `ping` |
| Sandbox Manager frontend | 已完成首版 | MCP 测试页改为 POST `/api/sandboxes/:sandbox_id/mcp` JSON-RPC |
| 验证 | 已通过 | `cargo fmt --all`、`cargo test -p chatos_mcp_service`、`cargo test -p chatos_sandbox_mcp_server`、`cargo check -p sandbox_manager_service_backend`、`cargo test -p sandbox_manager_service_backend mcp`、`cargo check -p local_connector_client_core`、`cargo test -p local_connector_client_core local_mcp`、`npm run type-check` |
| 本轮验证 | 部分通过 | `cargo fmt --all`、`cargo test -p chatos_mcp_service`、`cargo check -p chatos_sandbox_mcp_server`、`cargo test -p chatos_sandbox_mcp_server`、`cargo check -p local_connector_client_core`、`cargo test -p local_connector_client_core --no-run` 已通过；image、docker、workspace、relay、terminal controller 及其 output/shell 子模块、history、mcp tools、terminal guard 模块拆分后均已重跑 `cargo check -p local_connector_client_core` 和 `cargo test -p local_connector_client_core --no-run`；旧协议残留扫描只剩 `handle_mcp_request` 函数名命中；本轮最后一次 `local_mcp` / `local_terminal` 测试执行被 Windows 应用控制策略阻止运行 test exe（os error 4551）；`cargo check -p local_connector_service_backend` 仍停在依赖下载阶段，已终止挂起进程，待网络恢复后重跑 |

## 当前代码变更

1. 根 workspace 新增成员 `crates/chatos_mcp_service`。
2. `chatos_mcp_service` 不依赖 Axum，只提供协议、provider trait 和 JSON-RPC dispatch。
3. `sandbox_manager_service/sandbox_mcp_server` 新增对 `chatos_mcp_service` 的依赖。
4. Sandbox MCP server 的 `/mcp` 入口改为调用 `McpJsonRpcService`。
5. Sandbox MCP server 只注册 `/health` 和 `/mcp`。
6. 旧 REST 兼容函数和旧工具名映射函数已从 Sandbox MCP server 入口删除。
7. Sandbox MCP server 入口拆出配置读取、鉴权和 provider；`main.rs` 从约 655 行降到 172 行。
8. 共享 MCP service 增加 `initialize`、`ping`、`tools/list`、`tools/call`、非法参数、未知方法测试。
9. Sandbox MCP server 增加 bearer token、sandbox token header、缺 token 鉴权测试。
10. Sandbox Manager backend 删除旧 MCP tools/call façade route、handler、DTO 和 manager 方法，只保留 raw JSON-RPC `/mcp` proxy。
11. Local Connector client 删除本地 sandbox 旧 `/mcp/tools` 和 `/mcp/call` 代理分支。
12. Sandbox Manager frontend 删除旧 MCP tools/call API wrapper，测试页改用统一 JSON-RPC proxy。
13. Local Connector client 标准 MCP JSON-RPC dispatch 接入 `chatos_mcp_service`，本地扩展方法 `local_connector/terminal/start|cleanup` 暂留。
14. Local MCP 测试按共享 dispatch 错误码和 Windows standalone terminal 行为更新，`local_mcp` 过滤测试通过。
15. Local Connector client 新增 `mcp/selection.rs`，承接 builtin kind header 解析、工具类型识别和 read/write 权限判断。
16. `chatos_mcp_service` 新增 `catalog.rs`，提供工具名读取、按名称排序和 name set 构造，`CompositeToolProvider` 和 Sandbox MCP provider 复用同一套基础工具目录逻辑。
17. Local Connector client 新增 `relay/messages.rs`，承接 `RelayRequest`、`RelayResponse`、`relay_error_response` 和 MCP relay type 常量。
18. Local Connector client 新增 `mcp/service.rs` 和 `mcp/provider.rs`，将标准 JSON-RPC dispatch、provider、工具目录和工具调用入口从 `main.rs` 拆出。
19. Local Connector client 新增 `terminal/session.rs`，将 `LocalTerminalManager`、PTY session、交互式输入目录防护状态和 terminal event 构造从 `main.rs` 拆出。
20. Local Connector client 新增 `terminal/relay.rs`，将 `terminal_session_create_request`、`terminal_input`、`terminal_resize`、`terminal_snapshot_request`、`terminal_close` handler 从 `main.rs` 拆出；`main.rs` 约降到 6956 行。
21. Local Connector client 新增 `terminal/exec.rs`，将 `terminal_exec_request` relay handler 和 `run_terminal_exec` 从 `main.rs` 拆出；`main.rs` 约降到 6752 行。
22. Local Connector client 新增 `sandbox/types.rs`，将 local sandbox state/runtime/job/lease/resource/network/request DTO 从 `main.rs` 拆出。
23. Local Connector client 新增 `sandbox/catalog.rs`，将 local sandbox runtime specs/catalog 纯数据从 `main.rs` 拆出；`main.rs` 约降到 6436 行。
24. Local Connector client 新增 `workspace/paths.rs`，将 workspace lookup、cwd/header 解析、connector URI/绝对路径规范化、workspace path resolve、fingerprint helper 从 `main.rs` 拆出；`main.rs` 约降到 6172 行。
25. Local Connector service backend 的 MCP relay outbound type 改为 `mcp`，inbound pending response 也只接受 `mcp`，不再接受旧 `mcp_response`。
26. Sandbox Python agent 删除 `/mcp/tools`、`/mcp/call` 和旧 `sandbox_filesystem_*` / `sandbox_terminal_*` 工具名 alias，README 只保留 `/health` 和 `/mcp`。
27. `LOCAL_CONNECTOR_IMPLEMENTATION_PLAN.md` 和相关历史方案中的旧 MCP helper route 示例同步为 `/mcp` JSON-RPC，避免按旧 envelope 接入。
28. Local Connector client 新增 `sandbox/images.rs`，将 local sandbox image catalog、Docker image status、image build job、build log 截断和 image id 生成从 `main.rs` 拆出；`main.rs` 约降到 5795 行。
29. Local Connector client 新增 `sandbox/docker.rs`，将 Docker status、ensure running、sandbox 容器启动/检查/销毁、agent port 发现和 health wait 从 `main.rs` 拆出；`main.rs` 约降到 5526 行。
30. Local Connector client 新增 `sandbox/workspace.rs`，将 sandbox run workspace 路径、workspace 准备、输出导出、文件索引、sha256 和 change manifest helper 从 `main.rs` 拆出；`main.rs` 约降到 5183 行。
31. Local Connector client 新增 `sandbox/relay.rs`，将 sandbox relay request 解析、lease 创建/查询/释放、health、MCP proxy 和 sandbox tool history 记录从 `main.rs` 拆出；`main.rs` 约降到 4713 行。
32. Local Connector client 新增 `terminal/controller.rs`，将 Local MCP terminal controller store、terminal registry、process list/poll/wait/write/kill 和 reusable shell session 逻辑从 `main.rs` 拆出；`main.rs` 约降到 3295 行。
33. Local Connector client 新增 `terminal/controller/output.rs` 和 `terminal/controller/shell.rs`，将 controller 内的日志输出裁剪、marker 过滤、日志选择、shell 脚本构造、cwd 解析和 workspace 显示路径 helper 拆出；`terminal/controller.rs` 约降到 1249 行。
34. Local Connector client 新增 `history.rs`，将 command history entry/recorder、terminal exec history、interactive terminal submission history、sandbox tool call preview 和 history source normalization 从 `main.rs` 拆出；`main.rs` 约降到 2859 行。
35. Local Connector client 新增 `mcp/tools.rs`，将 Code Maintainer / Browser Tools / Terminal Controller 的 service factory、project-relative path normalization 和 terminal tool history wrapper 从 `main.rs` 拆出；`main.rs` 约降到 2620 行。
36. Local Connector client 新增 `terminal/guard.rs`，将 terminal directory-change parser、路径越界保护、ANSI 清理、输入换行规范化和清空输入行 helper 从 `main.rs` 拆出；`main.rs` 约降到 2294 行。
37. Local Connector client 新增 `history/format.rs`，将 history 输出预览、命令展示文本格式化、文本截断和 compact JSON helper 从 `history.rs` 继续拆出。
38. Local Connector client 新增 `history/sandbox.rs`，将 sandbox tool call details、命令参数解析、tool result preview 和错误信息抽取从 `history.rs` 继续拆出。
39. Local Connector client 将 `api/handlers.rs` 拆为 `auth.rs`、`workspace.rs`、`sandbox.rs`、`terminal.rs`、`history.rs`、`status.rs` 和 `helpers.rs`；`api/handlers.rs` 约降到 20 行。
40. Local Connector client 将 `sandbox/images.rs` 继续拆为入口模块、`images/build.rs` 和 `images/job.rs`；`sandbox/images.rs` 约降到 107 行。
41. Local Connector client 新增 `sandbox/lease.rs`，将 local sandbox lease 创建、查询、health、release/export/destroy 从 `sandbox/relay.rs` 拆出；`sandbox/relay.rs` 约降到 125 行。
42. Local Connector client 将 Terminal Controller 继续拆为 `controller/reused.rs`、`controller/standalone.rs`、`controller/store/logs.rs`、`controller/store/process.rs`、`controller/registry/types.rs` 和 `controller/registry/logs.rs`；`controller.rs` 约降到 215 行，`store.rs` 约降到 144 行，`registry.rs` 约降到 228 行。
43. Local Connector client 将 `terminal/guard.rs` 继续拆为入口模块、`guard/parser.rs`、`guard/path.rs` 和 `guard/text.rs`；`terminal/guard.rs` 约降到 95 行。
44. Local Connector client 将 `terminal/session.rs` 继续拆为入口模块、`session/input.rs` 和 `session/output.rs`；`terminal/session.rs` 约降到 163 行。
45. Local Connector client 将 `terminal/relay.rs` 继续拆为入口模块、`relay/types.rs`、`relay/create.rs` 和 `relay/control.rs`；`terminal/relay.rs` 约降到 12 行。
46. Local Connector client 将 `sandbox/docker.rs` 继续拆为入口模块、`docker/status.rs` 和 `docker/container.rs`；`sandbox/docker.rs` 约降到 14 行。
47. Local Connector client 将 `workspace/paths.rs` 继续拆为入口模块、`paths/normalize.rs` 和 `paths/resolve.rs`；`workspace/paths.rs` 约降到 39 行。
48. Local Connector client 将 `mcp/tools.rs` 继续拆为入口模块、`tools/project.rs`、`tools/code.rs`、`tools/browser.rs` 和 `tools/terminal_controller.rs`；`mcp/tools.rs` 约降到 15 行。
49. Local Connector client 新增 `history/types.rs`，将 command history 数据结构、recorder 和 execution context 从 `history.rs` 拆出。
50. Local Connector client 新增 `history/entries.rs`，将 exec/interactive/sandbox history entry 构造、source normalize 和 output text helper 从 `history.rs` 拆出；`history.rs` 约降到 16 行。
51. Local Connector client 新增 `terminal/controller/reused/marker.rs`，将 reusable shell sentinel marker 等待、exit code 解析和 active marker 清理从 `reused.rs` 拆出；`reused.rs` 约降到 163 行。
52. Local Connector client 新增 `terminal/controller/store/process/query.rs` 和 `terminal/controller/store/process/control.rs`，将 process list/poll/log 与 wait/write/kill 分离；`store/process.rs` 约降到 6 行。
53. Local Connector client 新增 `terminal/exec/runner.rs`，将 terminal exec 命令执行、timeout、输出截断和 history 写入从 `terminal/exec.rs` 拆出；`terminal/exec.rs` 约降到 58 行。
54. Local Connector client 新增 `terminal/controller/registry/lifecycle.rs`，将 session 注册、reader、状态刷新和 exited 标记从 `registry.rs` 拆出。
55. Local Connector client 新增 `terminal/controller/registry/query.rs`，将 session context 查询、id 查询和 wait 从 `registry.rs` 拆出；`registry.rs` 约降到 28 行。
56. Local Connector client 新增 `sandbox/workspace/fs.rs`，将 workspace 清理、复制和 `.chatos` 跳过规则从 `sandbox/workspace.rs` 拆出。
57. Local Connector client 新增 `sandbox/workspace/paths.rs`，将 sandbox workspace root、baseline 和 run id path segment helper 从 `sandbox/workspace.rs` 拆出。
58. Local Connector client 新增 `sandbox/workspace/request.rs`，将 sandbox lease create request body 注入逻辑从 `sandbox/workspace.rs` 拆出；`sandbox/workspace.rs` 约降到 97 行。

## Breaking 变更记录

| 变更 | 影响 | 后续动作 |
| --- | --- | --- |
| 删除 Sandbox MCP server `/mcp/tools` | 直接 HTTP 列工具调用方失效 | 改为 JSON-RPC `/mcp` + `tools/list` |
| 删除 Sandbox MCP server `/mcp/call` | 直接 HTTP 调工具调用方失效 | 改为 JSON-RPC `/mcp` + `tools/call` |
| 删除 Sandbox MCP server `/terminal/exec` | 旧终端兼容调用方失效 | 改为 `tools/call` 调 `execute_command` |
| 删除 Sandbox MCP server `/files/*` | 旧文件兼容调用方失效 | 改为 `tools/call` 调 Code Maintainer 工具 |
| 删除 sandbox legacy tool name mapping | `sandbox_filesystem_*` / `sandbox_terminal_*` 旧名失效 | 调用方使用 `tools/list` 返回的新工具名 |
| 删除 Sandbox Manager `/api/sandboxes/:sandbox_id/mcp/tools` | 云端 façade 列工具调用方失效 | 改为 POST `/api/sandboxes/:sandbox_id/mcp` + JSON-RPC `tools/list` |
| 删除 Sandbox Manager `/api/sandboxes/:sandbox_id/mcp/call` | 云端 façade 调工具调用方失效 | 改为 POST `/api/sandboxes/:sandbox_id/mcp` + JSON-RPC `tools/call` |
| 删除 Local Connector 本地 sandbox `/mcp/tools` 和 `/mcp/call` proxy | 本地 sandbox façade 调用方失效 | 改为 `/mcp` JSON-RPC |
| 删除 Local Connector relay `mcp_request` / `mcp_response` | 旧 websocket relay envelope 调用方失效 | 云端 service 和本地 client 使用 `type: "mcp"` |
| 删除 Sandbox Python agent `/mcp/tools` / `/mcp/call` | 备用 Python agent 的 helper route 调用方失效 | 改为 POST `/mcp` + JSON-RPC `tools/list` / `tools/call` |
| 删除 Sandbox Python agent 旧工具名 alias | `sandbox_filesystem_*` / `sandbox_terminal_*` 旧名失效 | 使用 `tools/list` 返回的标准工具名 |

## 下一步

1. 网络恢复后重跑 `cargo check -p local_connector_service_backend`，确认 `type: "mcp"` relay backend 编译。
2. 补 Sandbox MCP server 端到端 JSON-RPC handler 测试，覆盖 `/mcp` 请求和鉴权错误 envelope。
3. 继续抽 `chatos_mcp_service` 的 policy / 权限模块，减少各 host 自己维护工具归属集合。
4. 继续缩小 `local_connector_client/core/src/main.rs`，优先评估 local API/auth/bootstrap 的边界。
5. 查找剩余历史文档里的旧 façade 描述，按新协议补迁移说明。

## 风险

1. Sandbox Manager 或其他调用方如果仍调用旧 route，需要同步迁移到 `/mcp` JSON-RPC。
2. 旧工具名 alias 删除后，Task Runner 配置里如果保存了旧名，需要一次性迁移。
3. Local Connector relay 已改成 breaking envelope，云端 service 和本地 client 必须同批发布或做版本握手。
4. 本轮 `local_connector_service_backend` 检查被 crates.io 下载超时阻断，当前只能确认改动面较小但还需要重跑验证。
5. 本轮最后一次客户端测试执行被 Windows 应用控制策略阻止运行 test exe；`cargo test --no-run` 已通过，待策略放行后重跑 `local_mcp` / `local_terminal` 过滤测试。
