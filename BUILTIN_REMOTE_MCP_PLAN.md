# 内置远程 MCP 方案（基于现有代码）

## 1. 目标

把你现在已有的 `remote_connections`（SSH/SFTP）能力，做成一个**内置 MCP**，让 AI 可以直接调用远程服务器能力。

关键点是复用现有代码，不再新建一套平行的远程系统。

## 2. 现有代码可复用能力

当前项目已经具备两条完整链路：

1. 内置 MCP 链路
- `chat_app_server_rs/src/services/builtin_mcp.rs`
- `chat_app_server_rs/src/services/mcp_loader.rs`
- `chat_app_server_rs/src/core/mcp_tools/builtin.rs`
- `chat_app_server_rs/src/services/v2/mcp_tool_execute.rs`
- `chat_app_server_rs/src/services/v3/mcp_tool_execute.rs`
- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`

2. 远程连接链路
- 连接增删改查：`chat_app_server_rs/src/api/remote_connections/handlers.rs`
- SSH 执行：`chat_app_server_rs/src/api/remote_connections/connectivity.rs`
- 远程终端会话：`chat_app_server_rs/src/api/remote_connections/remote_terminal.rs`
- SFTP 能力：`chat_app_server_rs/src/api/remote_connections/remote_sftp/*`
- 连接归属校验：`chat_app_server_rs/src/core/remote_connection_access.rs`

结论：核心能力已具备，只需要把远程能力接入内置 MCP 注册与执行体系。

## 3. 整体设计

## 3.1 新增一个内置 MCP 类型

在 `chat_app_server_rs/src/services/builtin_mcp.rs` 增加：

1. MCP ID：`builtin_remote_connection_controller`
2. 命令：`builtin:remote_connection_controller`
3. 显示名、服务名常量
4. `BuiltinMcpKind` 枚举成员
5. 接入 `builtin_kind_by_id`、`builtin_kind_by_command`
6. 接入 `get_builtin_mcp_config`、`list_builtin_mcp_configs`
7. 接入 `builtin_display_name`

这样它会自动出现在你现有的 MCP 管理与加载流程中。

## 3.2 新增 builtin 工具服务

新增目录：

1. `chat_app_server_rs/src/builtin/remote_connection_controller/mod.rs`
2. `chat_app_server_rs/src/builtin/remote_connection_controller/actions.rs`
3. `chat_app_server_rs/src/builtin/remote_connection_controller/context.rs`

并注册到：

1. `chat_app_server_rs/src/builtin/mod.rs`
2. `chat_app_server_rs/src/core/mcp_tools/builtin.rs`

## 3.3 工具集合（V1）

先做这 5 个工具，足够落地：

1. `list_connections`
- 列出当前用户可用远程连接
- 必须脱敏，不返回密码等敏感字段

2. `test_connection`
- 基于现有连通性测试逻辑，检测连接是否可用

3. `run_command`
- 在指定连接上执行一条 SSH 命令
- 带超时与输出长度限制

4. `list_directory`
- 列出远程目录

5. `read_file`
- 读取远程文件内容
- 带文件大小上限

V1 之后可扩展：

1. `write_file`
2. `upload_file`
3. `download_file`

## 3.4 连接绑定策略

工具调用支持两种方式：

1. 显式传 `connection_id`
2. 不传时，使用会话运行时里的默认 `remote_connection_id`

如果两者都没有，返回结构化错误，引导 AI 先调用 `list_connections`。

## 4. 安全策略

## 4.1 用户隔离

每次工具调用都必须按连接 ID 重新查库，并校验：

1. 连接存在
2. `connection.user_id == 当前用户`

不能只依赖前端传参。

## 4.2 限流与上限

建议默认限制：

1. `run_command` 超时 20 秒，最大 120 秒
2. 命令输出最多 20k 字符
3. 文件读取最多 256 KB

## 4.3 危险命令控制

`run_command` 默认拦截高风险命令模式（如 `rm -rf /`、`mkfs`、`shutdown`）。

如需放行，必须显式参数 `allow_dangerous: true`，并记录日志。

## 4.4 敏感信息脱敏

`list_connections` 返回字段仅保留：

1. `id`
2. `name`
3. `host`
4. `port`
5. `username`
6. `auth_type`
7. `default_remote_path`

禁止返回 `password`、`jump_password`、私钥内容。

## 5. 后端改动清单（按文件）

1. 内置 MCP 注册
- `chat_app_server_rs/src/services/builtin_mcp.rs`

2. builtin 模块导出
- `chat_app_server_rs/src/builtin/mod.rs`

3. builtin 工具工厂
- `chat_app_server_rs/src/core/mcp_tools/builtin.rs`

4. 新增远程 builtin 实现
- `chat_app_server_rs/src/builtin/remote_connection_controller/mod.rs`
- `chat_app_server_rs/src/builtin/remote_connection_controller/actions.rs`
- `chat_app_server_rs/src/builtin/remote_connection_controller/context.rs`

5. 推荐抽公共层（避免直接依赖 API 私有方法）
- 从 `chat_app_server_rs/src/api/remote_connections/*` 抽出通用 SSH/SFTP 逻辑到 `services` 层
- API 继续做薄封装

6. 会话运行时字段（可选但推荐）
- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`
- `chat_app_server_rs/src/core/chat_runtime.rs`

## 6. 前端联动（推荐）

为了让 AI 默认使用你当前选中的远程连接，建议增加运行时字段：

1. 新增 `remoteConnectionId` 到运行时类型
- `chat_app/src/lib/store/types.ts`
- `chat_app/src/lib/store/actions/sendMessage/runtime.ts`
- `chat_app/src/lib/store/actions/sendMessage/requestPayload.ts`

2. 流式请求增加 `remote_connection_id`
- `chat_app/src/lib/api/client/stream.ts`

3. 从当前 UI 选中的远程连接写入发送参数
- `chat_app/src/components/InputArea.tsx`
- `chat_app/src/components/chatInterface/ChatComposerPanel.tsx`

4. 持久化到 session metadata
- `chat_app/src/lib/store/helpers/sessionRuntime.ts`

如果你希望先最小上线，前端这部分可以后做，MVP 阶段 AI 也能通过 `list_connections` 先选连接再执行命令。

## 7. 分阶段实施

## 阶段 A：MVP（先后端）

1. 新增 builtin 类型与服务
2. 实现 `list_connections`、`test_connection`、`run_command`
3. 补单元测试
4. 在 v2/v3 对话里验证工具可调用

## 阶段 B：文件能力增强

1. 增加 `list_directory`、`read_file`
2. 补充输出限制、错误码映射、脱敏
3. 增加集成测试

## 阶段 C：体验联动

1. 前端发送 `remote_connection_id`
2. 会话自动绑定当前远程连接
3. 运行时快照记录该连接

## 8. 测试与验收

## 8.1 单元测试

1. 工具 schema 和参数校验
2. 连接归属校验
3. 危险命令拦截
4. 输出截断行为

## 8.2 集成测试

1. `agent_v3` 场景下可列出连接
2. 可在有权限连接上执行命令
3. 无权限连接被拒绝

## 8.3 手工验收

1. 在 REMOTE 面板创建连接
2. 聊天里开启 MCP
3. 让 AI 执行“列出连接并在目标机器执行 `uname -a`”
4. 检查结果、错误提示、日志是否符合预期

## 9. 主要风险

1. 现有 `api/remote_connections/*` 里有不少 `pub(super)`，builtin 不能直接稳定复用，建议抽公共服务层。
2. 当前连接模型中包含敏感字段，输出必须严格脱敏。
3. 长时间交互式操作不建议在 V1 工具面开放，先做短命令和目录/文件读。

## 10. 验收标准

1. 新内置 MCP 能在 MCP 列表中显示为 `builtin + readonly`
2. AI 能通过工具访问当前用户的远程服务器
3. 权限边界正确，不能跨用户访问连接
4. 输出长度、超时、错误提示可控
5. 不影响现有远程终端与 SFTP 页面功能
