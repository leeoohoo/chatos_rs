# MCP 工具整合与大文件拆分实施方案

更新日期：2026-07-07

## 1. 结论

这个项目有继续投入的价值。价值不在于“又接了几个工具”，而在于它已经形成了一个比较完整的 MCP 能力闭环：

1. `crates/chatos_mcp_runtime` 已经能统一注册和执行 HTTP、stdio、builtin MCP 工具。
2. `crates/chatos_builtin_tools` 已经沉淀了 Code Maintainer、Terminal Controller、Browser Tools 等可复用能力。
3. Sandbox MCP server 和 Local Connector 已经把这些能力暴露给不同宿主环境：云端沙箱、本地工作区、本地 relay。
4. Task Runner、Project Management、Chat Server 已经围绕 MCP 做了实际业务集成，而不是停留在 demo。

当前主要问题是：执行端抽象已经有了，服务端工具暴露、工具选择、权限策略、宿主路径/终端适配还散落在几个大文件里。继续堆功能会让 Local Connector 和 Sandbox MCP server 各自变成“第二套 builtin tools”。本方案的目标是先固定契约，再抽公共层，最后拆大文件。

对开发者的客观评价：这个开发者产品推进能力和系统拼装能力很强，能把多服务、多运行环境、多工具链快速接起来；但工程边界意识还需要加强。现在的代码能跑出复杂业务闭环，但很多逻辑靠大文件和就地判断维持，一旦功能继续扩展，维护成本会非线性上升。下一阶段最需要补的是契约测试、模块边界和权限模型的显式化。

## 2. 本次检查到的重点文件

按源码文件行数和 MCP 相关性看，优先治理以下文件：

| 文件 | 当前问题 | 建议优先级 |
| --- | --- | --- |
| `local_connector_client/core/src/main.rs`，约 7990 行 | 配置、状态、relay、MCP、路径策略、终端会话、本地 sandbox、测试全部混在一个文件 | P0 |
| `sandbox_manager_service/backend/src/service/manager.rs`，约 2298 行 | lease 生命周期、health、MCP proxy、access client、image/pool、cleanup 聚合过重 | P1 |
| `local_connector_service/backend/src/api.rs`，约 1743 行 | 设备、workspace、project binding、sandbox pairing、MCP relay、terminal relay、WebSocket relay 全在 API 文件 | P1 |
| `crates/chatos_mcp_runtime/src/process_isolation.rs`，约 1540 行 | 配置、用户映射、workspace、命令包装、Linux mount namespace、helper CLI 混合 | P2 |
| `sandbox_manager_service/sandbox_mcp_server/src/terminal_store/mod.rs`，约 724 行 | 终端 session、日志、进程等待、路径解析、shell runtime 聚合 | P1 |
| `sandbox_manager_service/sandbox_mcp_server/src/main.rs`，约 655 行 | Axum 路由、配置、鉴权、JSON-RPC、兼容 REST、工具分发在入口文件 | P0 |

已有可复用基础：

1. `crates/chatos_mcp_runtime/src/registry.rs` 已有 `BuiltinToolProvider` 和 `BuiltinToolRegistry`。
2. `crates/chatos_mcp_runtime/src/executor/{registration,execution}.rs` 已经把工具注册和工具执行拆开。
3. `crates/chatos_builtin_tools/src/provider.rs` 已有 `SharedBuiltinToolService`，可统一 list/call 多类内置工具。
4. `task_runner_service/backend/src/mcp_server/entrypoints/dispatch.rs` 已经有较清晰的 MCP JSON-RPC dispatch 形态。
5. `crates/chatos_project_mcp_contract` 已经展示了“把 MCP 方法名、工具名、schema 常量抽成 contract crate”的方向。

## 3. 现状问题

### 3.1 服务端 MCP 协议层重复

Sandbox MCP server 在 `sandbox_manager_service/sandbox_mcp_server/src/main.rs` 中手写：

1. `/mcp`
2. `tools/list`
3. `tools/call`
4. JSON-RPC success/error envelope
5. token 鉴权
6. legacy REST 兼容入口

Local Connector client 在 `local_connector_client/core/src/main.rs` 中也手写：

1. `handle_mcp_request`
2. `handle_mcp_body`
3. `initialize`
4. `ping`
5. `tools/list`
6. `tools/call`
7. relay response envelope

这些逻辑语义接近，但错误码、初始化响应、批量请求、鉴权前置、工具选择和参数校验都各自实现。后续一旦支持更多 MCP 方法，两个入口会继续分叉。

### 3.2 工具提供者和工具选择没有统一模型

内置工具侧已经有 `SharedBuiltinToolService` 和 `BuiltinToolProvider`，但 Local Connector 仍在 `main.rs` 中用硬编码字符串判断：

1. `is_code_maintainer_tool`
2. `is_terminal_controller_tool`
3. `is_browser_tool`
4. `local_mcp_tool_selection`
5. `call_builtin_compatible_local_tool`

Sandbox MCP server 也维护了 `file_tool_names`、`terminal_tool_names` 两套集合，然后手动分发。当前能工作，但它把“工具目录”“权限选择”“宿主实现”揉在一起，不适合继续扩展。

### 3.3 宿主能力没有成为显式抽象

Code Maintainer、Terminal Controller、Browser Tools 在不同环境中共用工具 schema，但底层宿主不同：

1. Chat Server builtin：本机 workspace。
2. Sandbox MCP server：容器内 `/workspace`。
3. Local Connector：用户授权的本地 workspace，经 relay 调用。
4. 未来 remote connector：可能是 SSH、Docker、WSL 或其他运行面。

现在差异主要藏在各自构造 service 的参数、路径归一化和 terminal store 实现里。正确方向是把“工具语义”和“宿主能力”拆开。

### 3.4 权限边界分散

权限控制散在多层：

1. Sandbox Manager backend 校验 lease scope 和 tool allowlist。
2. Sandbox MCP server 校验 agent token。
3. Local Connector Service 校验 user/device/workspace。
4. Local Connector client 根据 header 选择可用 builtin kind。
5. Code Maintainer/Terminal Controller 自身还有 read/write/path 限制。

这些边界都需要保留，但应有统一的 `ToolPolicy` / `ToolSelection` 描述，避免各层靠字符串约定。

### 3.5 大文件已经影响修改安全

最大的问题不是行数本身，而是不同变化原因被放进同一文件：

1. 改 relay 协议会碰 Local Connector 的终端实现。
2. 改 MCP 工具选择会碰本地路径工具和浏览器 registry。
3. 改 sandbox lease 会碰 MCP proxy、access client、pool、image job。
4. 改 API route 会碰 WebSocket relay 和 sandbox facade。

这会让 review、测试选择和回滚都变慢。

## 4. 目标架构

### 4.1 分层原则

建议形成四层：

```text
MCP protocol/service layer
  - JSON-RPC request/response/error
  - initialize/ping/tools/list/tools/call dispatch
  - tool catalog and tool policy

Tool provider layer
  - list_tools
  - call_tool
  - unavailable_tools
  - provider composition and filtering

Workspace host layer
  - root/path policy
  - user/project/conversation context
  - terminal process store
  - browser service registry
  - audit/history hook

Transport layer
  - Axum /mcp
  - Local Connector relay message
  - Sandbox Manager proxy
  - one-time migration adapters, not long-lived compatibility endpoints
```

核心原则：工具 schema 和工具执行语义只保留一份；HTTP、relay、sandbox 只是不同 transport 和 host。

### 4.2 新增共享 crate

建议新增 `crates/chatos_mcp_service`，不要继续把所有服务端能力塞进 `chatos_mcp_runtime`。

`chatos_mcp_runtime` 继续负责“作为客户端/执行器去消费 MCP 工具”：

1. HTTP / stdio / builtin 注册。
2. tool name prefix / alias。
3. tool call 参数解析。
4. 并发执行和结果回调。

`chatos_mcp_service` 负责“作为服务端暴露 MCP 工具”：

```text
crates/chatos_mcp_service/src/
  lib.rs
  protocol.rs      # JsonRpcRequest/Response/Error，MCP 方法常量
  service.rs       # McpJsonRpcService，处理 initialize/ping/tools/list/tools/call
  provider.rs      # McpToolProvider trait，CompositeToolProvider
  policy.rs        # ToolSelection, ToolPolicy, ToolAllowlist
  catalog.rs       # ToolCatalog，工具名归属、过滤、排序、snapshot helper
  result.rs        # JSON-RPC error mapping 和工具返回包装
  tests.rs
```

这个 crate 默认不依赖 Axum。Axum handler 留在各服务里，用薄 adapter 调 `McpJsonRpcService`。

### 4.3 统一 ToolProvider

新增服务端 provider trait：

```rust
#[async_trait]
pub trait McpToolProvider: Send + Sync {
    fn server_name(&self) -> &str;
    fn list_tools(&self, context: &McpRequestContext) -> Vec<Value>;
    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: McpRequestContext,
    ) -> Result<Value, String>;
    fn unavailable_tools(&self, context: &McpRequestContext) -> Vec<(String, String)> {
        Vec::new()
    }
}
```

然后提供适配器：

1. `BuiltinToolProviderAdapter`：复用现有 `BuiltinToolProvider`。
2. `SharedBuiltinToolServiceProvider`：复用 `chatos_builtin_tools::SharedBuiltinToolService`。
3. `CompositeToolProvider`：把 Code Maintainer、Terminal Controller、Browser Tools 合成一个 provider。
4. `FilteredToolProvider`：按 `ToolSelection` 过滤工具目录和工具调用。

Local Connector 和 Sandbox MCP server 不再维护硬编码 `is_xxx_tool` 列表，而是让 provider catalog 告诉调用方工具归属。

### 4.4 统一 Workspace Host

在 `chatos_builtin_tools` 或新增 `crates/chatos_workspace_tools` 中抽一个构造层，先不改现有工具内部逻辑：

```text
WorkspaceToolHost
  - host_kind: builtin | local_connector | sandbox
  - root: PathBuf
  - project_id: Option<String>
  - user_id: Option<String>
  - conversation_id: Option<String>
  - path_policy
  - code_maintainer_options
  - terminal_store
  - browser_registry_key
  - audit_hook
```

对应构造器：

1. `WorkspaceToolProviderBuilder::for_builtin(...)`
2. `WorkspaceToolProviderBuilder::for_local_connector(...)`
3. `WorkspaceToolProviderBuilder::for_sandbox(...)`

这样 Local Connector 和 Sandbox 都通过同一个 builder 生成 Code Maintainer / Terminal Controller / Browser Tools provider，只在 host profile 上不同。

### 4.5 Breaking 迁移标准

迁移期间不再以旧工具名、旧 route、旧 relay message type 兼容为目标。新的约束是协议形态统一、权限边界清晰、失败可诊断：

1. 标准工具入口统一为 JSON-RPC `/mcp`，优先支持 `initialize`、`ping`、`tools/list`、`tools/call`。
2. Sandbox MCP server 删除 legacy REST endpoint，不再注册 `/mcp/tools`、`/mcp/call`、`/files/*`、`/terminal/exec`。
3. Local Connector 删除旧 `mcp_request` / `mcp_response` 外壳，统一为 `type: "mcp"` 的 MCP relay envelope。
4. 旧工具名 alias 不作为兼容目标；需要时通过一次性迁移表更新调用方。
5. Snapshot 测试固定“新协议”的工具目录、错误码和权限行为，而不是保护旧兼容层。
6. 所有 breaking change 必须记录到进度文档，标明受影响调用方和迁移动作。

## 5. 模块拆分方案

### 5.1 Local Connector client

目标：把 `local_connector_client/core/src/main.rs` 拆成入口 + 领域模块，先搬代码，再替换共享 service。

建议结构：

```text
local_connector_client/core/src/
  main.rs                         # 启动、组装依赖、信号处理
  config.rs                       # env/config/defaults
  state.rs                        # LocalState、共享 registry、启动状态
  auth.rs                         # 登录态、token、device identity
  workspace/
    mod.rs
    registry.rs                   # workspace 授权和查询
    paths.rs                      # canonicalize/relative path policy
  relay/
    mod.rs
    client.rs                     # outbound cloud connection
    messages.rs                   # RelayRequest/RelayResponse
    dispatch.rs                   # message_type 分发
  mcp/
    mod.rs
    service.rs                    # LocalMcpService，调用 chatos_mcp_service
    selection.rs                  # header -> ToolSelection
    provider.rs                   # local workspace provider builder
    paths.rs                      # request cwd/project root 归一化
    terminal.rs                   # local MCP terminal start/cleanup 特殊方法
    history.rs                    # execute_command history recorder
    browser.rs                    # BrowserToolsService registry
  terminal/
    mod.rs
    store.rs                      # LocalConnectorTerminalControllerStore
    session.rs
    logs.rs
    process.rs
    shell.rs
  sandbox/
    mod.rs
    relay.rs
    docker.rs
  local_api/
    mod.rs
    routes.rs
    handlers.rs
  tests/
    mcp.rs
    relay.rs
```

拆分顺序：

1. 先抽 `relay/messages.rs`、`workspace/paths.rs`、`mcp/selection.rs`，不改行为。
2. 再抽 `mcp/service.rs`，仍调用当前 `call_builtin_compatible_local_tool`。
3. 引入 `chatos_mcp_service` 后替换 `handle_mcp_body`。
4. 最后把 terminal store 从 `main.rs` 拆出。

### 5.2 Sandbox MCP server

目标：`main.rs` 只负责启动和 route 组装。

建议结构：

```text
sandbox_manager_service/sandbox_mcp_server/src/
  main.rs
  config.rs
  state.rs
  api/
    mod.rs
    routes.rs
    health.rs
    mcp.rs                       # /mcp Axum adapter
  auth.rs
  tools/
    mod.rs
    provider.rs                  # SandboxWorkspaceToolProvider
  terminal_store/
    mod.rs
    store.rs
    session.rs
    logs.rs
    process.rs
    paths.rs
    shell.rs
```

迁移要求：

1. `/mcp` 改为调用 `McpJsonRpcService`。
2. 删除 `/mcp/tools`、`/mcp/call`、`/files/*`、`/terminal/exec` 旧入口。
3. 删除 `normalize_compat_tool_call` 这类旧工具名映射。
4. `SandboxTerminalControllerStore` 先只拆文件，不改标准 MCP 下的终端行为。

### 5.3 Sandbox Manager backend

目标：把 `SandboxManager` 从“所有业务的巨型 service”拆成多个 use case 模块。

建议结构：

```text
sandbox_manager_service/backend/src/service/
  manager.rs                     # Facade，保留 public API
  manager/
    mod.rs
    lease_lifecycle.rs            # create/promote/heartbeat/release/destroy/get/list
    health.rs
    mcp_proxy.rs                  # mcp_tools/mcp_call/mcp_proxy/authorize_mcp_proxy_payload
    access_clients.rs
    images.rs
    pool.rs
    events.rs
    cleanup.rs
    backend_routes.rs             # backend endpoint/agent endpoint/helper
```

迁移要求：

1. `SandboxManager` public 方法签名先不变。
2. `mcp_proxy.rs` 先只移动现有逻辑，后续再复用 `chatos_mcp_service::protocol`。
3. lease access 和 tool allowlist 校验必须有独立单元测试。
4. manager facade 可以继续持有 store/config/backend clients，子模块以 helper 函数或 extension trait 形式访问。

### 5.4 Local Connector Service backend

目标：把 `api.rs` 从“全路由实现文件”拆为按资源/relay 类型组织。

建议结构：

```text
local_connector_service/backend/src/api/
  mod.rs                         # build_router
  error.rs
  auth.rs
  devices.rs
  workspaces.rs
  project_bindings.rs
  sandbox_pairings.rs
  relay/
    mod.rs
    mcp.rs                       # mcp_relay
    terminal_exec.rs
    terminal_session.rs
    terminal_ws.rs
    socket.rs                    # connector websocket
    http_bridge.rs               # relay_response_to_http, headers/body helpers
  sandbox_facade.rs
```

迁移要求：

1. route path、query/body contract 不变。
2. `mcp_relay` 单独成模块，后续可复用 `chatos_mcp_service::protocol` 做请求校验。
3. WebSocket relay 和 HTTP relay 分开，避免 terminal 相关状态影响 MCP relay review。

### 5.5 Process Isolation

这是 P2，不要和 MCP 工具整合混在第一轮做。

建议结构：

```text
crates/chatos_mcp_runtime/src/process_isolation/
  mod.rs
  config.rs
  user_mapping.rs
  workspace.rs
  command.rs
  helper.rs
  linux/
    mod.rs
    apply.rs
    filesystem_view.rs
    mounts.rs
    capabilities.rs
  tests.rs
```

同时对比 `chat_app_server_rs/src/services/process_isolation.rs`，如果逻辑基本重复，后续应统一到一个 crate，Chat Server 只保留薄 wrapper。

## 6. MCP 工具整合方案

### 6.1 工具目录统一

新增 `ToolCatalog`：

```text
ToolCatalog
  - tools: Vec<Value>
  - by_name: HashMap<String, ToolDescriptor>
  - provider_by_tool: HashMap<String, String>
  - unavailable: Vec<UnavailableTool>
```

用途：

1. `tools/list` 直接从 catalog 输出。
2. `tools/call` 先通过 catalog 找 provider，再调用。
3. `FilteredToolProvider` 基于 catalog 过滤 read/write/terminal/browser。
4. 测试可以 snapshot catalog，确保工具列表不回退。

### 6.2 工具选择统一

Local Connector 当前通过 `x-local-connector-enabled-builtin-kinds` 控制工具。保留这个入口，但解析成统一结构：

```text
ToolSelection
  - code_read
  - code_write
  - terminal
  - browser
  - web
  - allowed_tools: Option<HashSet<String>>
  - denied_tools: HashSet<String>
```

Sandbox Manager 的 `ensure_tool_allowed` 也可以映射到同一个 `allowed_tools` 模型。

### 6.3 宿主 provider 统一

先支持三种 provider profile：

| Profile | 用途 | 差异 |
| --- | --- | --- |
| `builtin_workspace` | Chat/Task Runner 内置工具 | 本进程 workspace，现有 store |
| `local_connector_workspace` | 用户本机授权目录 | relay request context、cwd 归一化、本地 terminal store、history recorder |
| `sandbox_workspace` | 沙箱容器 `/workspace` | sandbox token、容器内 shell、sandbox terminal store、state_dir change log |

工具 schema 应来自同一套 service：

1. Code Maintainer：`CodeMaintainerService::list_tools/call_tool`
2. Terminal Controller：`TerminalControllerService::list_tools/call_tool`
3. Browser Tools：`BrowserToolsService::list_tools/call_tool_with_context`

差异只在 options 和 store。

### 6.4 特殊 MCP 方法处理

标准方法由 `McpJsonRpcService` 处理：

1. `initialize`
2. `notifications/initialized`
3. `ping`
4. `tools/list`
5. `tools/call`

Local Connector 特有方法暂时保留为 extension handler：

1. `local_connector/terminal/start`
2. `local_connector/terminal/cleanup`

设计为：

```text
McpJsonRpcService
  - standard handler
  - optional extension dispatcher
```

这样 Local Connector 的特殊终端生命周期能力不会污染 sandbox server。

### 6.5 错误模型统一

统一错误码：

| 场景 | JSON-RPC code |
| --- | --- |
| method 不存在 | `-32601` |
| params 缺失或非法 | `-32602` |
| 鉴权失败 | `-32001` |
| 工具执行失败 | `-32000` |
| provider 不可用 | `-32002` |

Local Connector relay 外层 HTTP/relay status 可以保留，但 body 内 JSON-RPC error 需要和 sandbox 一致。

## 7. 分阶段实施

### Phase 0：契约冻结和基线测试

目标：先知道“不能变”的东西是什么。

产出：

1. `tools/list` golden snapshot：
   - builtin Code Maintainer read/write
   - builtin Terminal Controller
   - Local Connector code read/write
   - Local Connector terminal
   - Local Connector browser
   - Sandbox MCP server file + terminal
2. JSON-RPC envelope 测试：
   - `initialize`
   - `ping`
   - `tools/list`
   - `tools/call` 缺 name
   - unknown method
   - unknown tool
3. 权限测试：
   - Local Connector 未启用 code_write 时不能写。
   - Local Connector 未启用 terminal 时不能执行命令。
   - Sandbox Manager proxy 对 `tools/call.name` 继续执行 allowlist。
4. 行数基线：
   - 记录本方案列出的热点文件行数。

建议验证命令：

```powershell
cargo test -p chatos_mcp_runtime
cargo test -p chatos_builtin_tools
cargo test -p local_connector_client_core mcp
cargo test -p chatos_sandbox_mcp_server
cargo test -p sandbox_manager_service_backend mcp
cargo test -p local_connector_service_backend mcp
```

### Phase 1：新增 `chatos_mcp_service`

目标：抽出协议层和服务端 dispatch，不接业务。

产出：

1. `JsonRpcRequest`、`JsonRpcResponse`、`JsonRpcError`。
2. `McpJsonRpcService`。
3. `McpToolProvider`、`CompositeToolProvider`、`FilteredToolProvider`。
4. `ToolSelection`、`ToolPolicy`。
5. 标准 MCP 方法处理。
6. 单元测试覆盖错误码和 response envelope。

验收：

1. 新 crate 无 Axum 依赖。
2. 可用 fake provider 跑通 `tools/list` 和 `tools/call`。
3. 不改现有服务行为。

### Phase 2：统一 builtin-compatible provider

目标：把 Local Connector 和 Sandbox 的工具目录/调用切到共享 provider 构造层。

产出：

1. `WorkspaceToolProviderBuilder`。
2. `local_connector_workspace` profile。
3. `sandbox_workspace` profile。
4. 删除或弱化硬编码 `is_code_maintainer_tool` / `is_terminal_controller_tool` / `is_browser_tool`。
5. Sandbox MCP server 的 `file_tool_names` / `terminal_tool_names` 变成 catalog 内部数据。

验收：

1. 新协议 golden snapshot 稳定。
2. Local Connector 工具启用/禁用迁移到新的 `ToolSelection` 后行为清晰。
3. Sandbox legacy REST 入口不存在，调用方必须改走 `/mcp`。

### Phase 3：拆 Local Connector client

目标：先降低 `main.rs` 复杂度，再替换 MCP 处理实现。

步骤：

1. 抽 `relay/messages.rs` 和 `relay/dispatch.rs`。
2. 抽 `workspace/paths.rs`。
3. 抽 `mcp/selection.rs`。
4. 抽 `mcp/service.rs`。
5. 抽 `mcp/provider.rs` 和 `mcp/browser.rs`。
6. 抽 `terminal/store.rs`、`terminal/session.rs`、`terminal/logs.rs`、`terminal/process.rs`。
7. `handle_mcp_body` 改为调用 `chatos_mcp_service::McpJsonRpcService`。

验收：

1. `local_connector_client/core/src/main.rs` 降到 500 行以内。
2. Local Connector MCP 相关模块单文件尽量不超过 700 行。
3. 现有测试迁移到 `tests/` 或模块内 tests。
4. 旧 relay message type 不再作为兼容目标；迁移完成后只保留新 MCP relay envelope。

### Phase 4：拆 Sandbox MCP server

目标：让 sandbox MCP server 成为共享 MCP service 的一个 host adapter。

步骤：

1. 抽 `config.rs`、`state.rs`、`auth.rs`。
2. 抽 `api/mcp.rs`。
3. 抽 `tools/provider.rs`，用 `WorkspaceToolProviderBuilder::for_sandbox`。
4. 拆 `terminal_store` 子模块。
5. `/mcp` 入口改为调用 `McpJsonRpcService`。

验收：

1. `sandbox_mcp_server/src/main.rs` 降到 200 行以内。
2. 仅注册 `/health` 和 `/mcp`。
3. terminal background / poll / kill / recent logs 相关测试通过。

### Phase 5：拆服务端 relay/proxy 大文件

目标：降低 manager 和 API 文件的 review 成本。

Sandbox Manager backend：

1. 抽 `manager/mcp_proxy.rs`。
2. 抽 `manager/lease_lifecycle.rs`。
3. 抽 `manager/access_clients.rs`。
4. 抽 `manager/images.rs`、`manager/pool.rs`。
5. 抽 `manager/health.rs`、`manager/events.rs`。

Local Connector Service backend：

1. 抽 `api/relay/mcp.rs`。
2. 抽 `api/relay/terminal_exec.rs`。
3. 抽 `api/relay/terminal_ws.rs`。
4. 抽 `api/devices.rs`、`api/workspaces.rs`、`api/project_bindings.rs`、`api/sandbox_pairings.rs`。
5. 抽 `api/relay/http_bridge.rs`。

验收：

1. `sandbox_manager_service/backend/src/service/manager.rs` 降到 400 行以内。
2. `local_connector_service/backend/src/api.rs` 变为 `api/mod.rs`，只保留 router 组装。
3. route contract 不变。

### Phase 6：Process Isolation 后续治理

目标：把进程隔离从 MCP 整合主线中解耦，作为第二轮技术债治理。

步骤：

1. 先拆 `crates/chatos_mcp_runtime/src/process_isolation.rs` 文件结构。
2. 对比 `chat_app_server_rs/src/services/process_isolation.rs`，提取重复实现。
3. Linux-only mount namespace 逻辑放入 `linux/`。
4. helper CLI 参数解析独立测试。

验收：

1. 行为不变。
2. Linux 分支有单元测试或可跳过的环境测试。
3. Windows 构建不受 Linux-only 模块影响。

## 8. 验证策略

### 8.1 Snapshot 测试

为以下工具目录建立 snapshot：

1. `CodeMaintainerRead`
2. `CodeMaintainerWrite`
3. `TerminalController`
4. `BrowserTools`
5. Sandbox file + terminal provider
6. Local Connector 组合 provider

snapshot 内容包含：

1. tool name
2. description
3. input schema
4. 是否出现在当前 selection 中

### 8.2 JSON-RPC 契约测试

覆盖：

1. request id 透传。
2. `params.arguments` 缺失时默认为 `{}`。
3. `tools/call.name` 缺失返回 `-32602`。
4. unknown method 返回 `-32601`。
5. provider error 返回 `-32000`。
6. auth error 返回 `-32001`。

### 8.3 权限测试

覆盖：

1. Local Connector read-only selection 不暴露写工具。
2. Local Connector write selection 同时允许 read + write。
3. Terminal selection 关闭时不暴露 terminal 工具。
4. Browser selection 关闭时不暴露 browser 工具。
5. Sandbox Manager proxy batch request 逐项鉴权。
6. Sandbox tool allowlist 拒绝未授权工具。

### 8.4 集成测试

最低集成路径：

1. 启动 `chatos_sandbox_mcp_server`，请求 `/mcp tools/list`。
2. 通过 Sandbox Manager `mcp_proxy` 转发 `tools/list` 和 `tools/call`。
3. 通过 Local Connector Service `mcp_relay` 转发到 Local Connector client。
4. Local Connector client 在 read-only workspace 下执行 `read_file_raw`。
5. Local Connector client 在 terminal-enabled workspace 下执行 `execute_command`。

## 9. 风险和规避

| 风险 | 影响 | 规避 |
| --- | --- | --- |
| 工具名变化 | 调用方需要迁移 | 用迁移表更新 Task Runner / Chat Server 配置，snapshot 固定新工具目录 |
| 错误格式变化 | 调用方误判失败原因 | 统一 JSON-RPC error 测试 |
| Local Connector 权限扩大 | 本地文件/命令越权 | `ToolSelection` 默认 deny，read/write/terminal/browser 分开测试 |
| Sandbox auth 绕过 | lease scope 失效 | Sandbox Manager proxy 鉴权不下沉，不因共享 service 删除 |
| 终端行为回退 | background/poll/log/kill 不稳定 | terminal store 先拆文件，不改行为 |
| 抽象过早过大 | 新 crate 变成第二个大杂烩 | `chatos_mcp_service` 只管 protocol、provider、policy，不管 Axum 和具体工具 |
| Windows 构建受 Linux 逻辑影响 | 本地开发失败 | Process Isolation 放 Phase 6，Linux-only 代码保持 cfg 隔离 |

## 10. 不建议做的事

1. 不建议直接重写 Local Connector client。它已经有可用业务闭环，应该先迁移边界。
2. 不建议把 Axum handler 放进 `chatos_mcp_runtime` 默认依赖。runtime 应继续偏执行端和协议客户端。
3. 不建议继续给新抽象套旧兼容壳。旧入口应删除，并用迁移记录明确调用方改造点。
4. 不建议为了减少行数把无关逻辑搬进一个 `utils.rs`。拆分应按变化原因，而不是按函数数量。
5. 不建议把权限控制完全下沉到工具 service。Manager/Service 层的用户、设备、lease 校验必须保留。

## 11. 建议落地顺序

最小安全路径：

1. Phase 0：补 snapshot 和契约测试。
2. Phase 1：新增 `chatos_mcp_service`。
3. Phase 4 的一部分：先让 Sandbox MCP server 使用共享 service，因为它比 Local Connector 简单。
4. Phase 2：抽 provider builder，固定 sandbox/local 共用工具构造。
5. Phase 3：拆 Local Connector client，并替换 MCP dispatch。
6. Phase 5：拆 manager 和 local connector service API。
7. Phase 6：再处理 process isolation。

原因：Sandbox MCP server 的场景更窄，适合作为共享 MCP service 的第一块试验田；Local Connector 牵涉 relay、workspace 授权、history、terminal session，应该等共享 service 形态稳定后再迁移。

## 12. 完成标准

最终应达到：

1. 内置工具、sandbox 工具、local connector 工具共享一套 provider/callback/catalog 模型。
2. `tools/list` 和 `tools/call` 的协议处理只有一套核心实现。
3. Local Connector 和 Sandbox 的差异体现在 host profile 和 policy，不体现在重复 JSON-RPC dispatch。
4. 大文件热点显著下降：
   - `local_connector_client/core/src/main.rs` 小于 500 行。
   - `sandbox_mcp_server/src/main.rs` 小于 200 行。
   - `sandbox_manager_service/backend/src/service/manager.rs` 小于 400 行。
   - `local_connector_service/backend/src/api.rs` 被拆为 `api/` 模块。
5. 旧工具名、旧 route、旧 relay message type 已迁移或删除，不再作为运行时兼容层存在。
6. MCP 工具目录、错误码、权限边界都有自动化测试保护。
