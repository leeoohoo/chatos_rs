# Task Runner MCP 管理统一规划

## 背景

当前 Task Runner 的 MCP 管理已经出现“存储状态、界面勾选状态、运行期真实工具”不一致的问题。

典型现象：

- 从 Chatos 创建后台任务时，如果项目是本地模式，代码读取能力可能通过 `local_connector` 暴露。
- 如果项目是云端 Harness 模式，代码读取能力可能通过 `harness_code` 暴露。
- 任务创建后，UI 里的内置 `CodeMaintainerRead` 没有勾选，但运行时确实可以读文件。
- 对用户来说，这会表现成“没选读 MCP，但读功能又是好的”，配置语义不直观。

这里的问题不只限于读文件。读、写、终端、浏览器操纵这类和项目执行环境强相关的 MCP，都应该按同一个语义处理：

- 任务需要读文件，就应该选中 `CodeMaintainerRead`。
- 任务需要改文件，就应该选中 `CodeMaintainerWrite`，并自动带上 `CodeMaintainerRead`。
- 任务需要跑命令，就应该选中 `TerminalController`。
- 任务需要观察或操作页面，就应该选中 `BrowserTools`。
- 至于这些能力最终由 Task Runner 本机 builtin、用户本地 `local_connector`，还是云端 `harness_code`/后续其它 host 实现，应该是运行期根据项目环境解析出来的实现路由，而不是改变“任务需要这个能力”的勾选事实。

这不是单点 bug，而是当前模型把几类不同概念混在了 `enabled_builtin_kinds`、`ephemeral_http_servers` 和前端硬编码里：

- 用户或 planner 显式选择的能力。
- 调用方或某类 agent 固定要求的能力。
- Task Runner 自动注入的系统能力。
- 被本地/云端宿主替代的能力。
- 最终运行时实际调用的 MCP provider/tools。
- AI 面向的稳定能力和工具语义。

## 现状梳理

### 1. Builtin catalog 是基础枚举，但不是唯一决策点

`crates/chatos_mcp_runtime/src/builtin_catalog.rs` 定义了：

- `BuiltinMcpKind`
- `configurable_builtin_kinds`
- `default_runtime_builtin_kinds`
- `complete_builtin_kind_dependencies`
- `builtin_servers_from_kinds`

其中 `CodeMaintainerWrite` 自动补 `CodeMaintainerRead` 的依赖在这里已经有一部分统一逻辑。

问题是：系统注入、profile 默认能力、host 替代能力并不都在这里统一表达。

### 2. TaskMcpConfig 的默认值和显式选择没有一等区分

`task_runner_service/backend/src/models/task/config.rs` 里 `TaskMcpConfig::default()` 会默认选择一批 builtin kinds。

但 MCP 创建任务时，`task_mcp_config_for_explicit_tool_selection()` 会把 `enabled_builtin_kinds` 置空，表示“这次由 planner 显式选择”。

当前没有字段明确表达：

- 这是默认选择。
- 这是用户或 planner 显式选择。
- 这是经过路由改写后的结果。

所以空数组既可能代表“什么都不选”，也可能代表“系统后面会补一些东西”，语义容易混乱。

### 3. Chatos async planner 有单独的系统注入规则

`task_runner_service/backend/src/mcp_server/chatos_async_planner.rs` 里硬编码：

```rust
const SYSTEM_INJECTED_BUILTIN_KINDS: &[&str] = &["TaskManager", "AskUser"];
```

`request_guards.rs` 会在创建/更新任务时自动补这两个 kind。

`schema.rs` 又会从工具 schema enum 中移除它们，告诉 planner 不要手动选择。

问题是：这套规则只存在 async planner 路径里，前端、catalog、运行期 resolver 并没有共享同一个元数据源。

### 4. 本地/云端项目通过 ephemeral HTTP MCP 替代 server-local builtin

`task_runner_service/backend/src/services/workspace_mcp.rs` 做了本地/云端路由：

- 本地项目：`local_connector`
- Harness 云端项目：`harness_code`

路由时会：

- 从 `mcp_config.enabled_builtin_kinds` 中移除被宿主替代的 builtin kinds。
- 把替代能力写进 `ephemeral_http_servers` 的 header。
- 运行时再通过这些 header 决定 `local_connector_*` 或 `harness_code_*` 暴露哪些工具。

这解释了“没有勾选 CodeMaintainerRead，但可以读”的现象：读能力已经从 `enabled_builtin_kinds` 被挪到 host MCP header 里了。

### 5. 运行期又会重新解析一次真实 builtin

`workspace_mcp.rs` 的 `runtime_selected_builtin_kinds()` 会根据：

- 任务 profile
- `enabled_builtin_kinds`
- 是否是 Chatos async task
- 是否使用 local connector
- 是否使用 harness code
- 运行期 sandbox 文件/终端替代关系

重新算运行期 server-local builtin。

`run_model_phase/setup/preparation/mcp_builder.rs` 再基于这个结果构建 `McpExecutorBuilder`，并额外处理：

- Chatos plan 固定 allowlist。
- Project Management provider。
- Task Runner skill lookup。
- task process log internal provider。

因此“最终工具”不是数据库字段本身，而是多处逻辑叠加后的结果。

### 6. 前端展示只看存储字段，缺少 effective 视图

`task_runner_service/frontend/src/pages/tasks/taskPageUtils.tsx`：

- 新建任务默认 `enabledBuiltinKinds: []`。
- 编辑任务直接读 `task.mcp_config.enabled_builtin_kinds`。
- `systemInjectedMcpServerNames()` 目前只根据 `chatos_plan` 显示 `project_management_service`。

`taskTableColumns.tsx`：

- builtin 数量直接用 `mcpConfig.enabled_builtin_kinds.length`。

所以前端展示的是“存储里的 server-local configured kinds”，不是“任务实际拥有的能力”。

## 目标

建立一个统一的 MCP 能力解析模型，让所有调用方只依赖同一套规则。

目标状态：

- UI 能清楚展示“我请求了哪些能力”和“运行时实际会有哪些工具”。
- local connector / harness code 提供的读写能力在 UI 里可见，不再像“没勾但能用”。
- TaskManager、AskUser、ProjectManagement 这类固定能力显示为 required/system capability，而不是伪装成用户勾选项。
- Chatos async planner、Task Runner 前端、运行期 executor、catalog 使用同一个 resolver。
- prompt 和 prompt preview 不承担 local/cloud/provider 区分，只消费 resolver 产出的 AI-facing capability manifest。
- 历史任务可以兼容读取，不能因为字段调整导致旧任务 MCP 配置失效。

### 7. Prompt 输入层已经开始泄露 provider 路由

`task_runner_service/backend/src/services/run_model_phase/setup/preparation/mcp_inputs.rs` 当前会根据 ephemeral server summary 追加 `Local Connector` / `Harness Code` 说明，并把 `local_connector_*`、`harness_code_*` 这类实现工具名直接写进模型上下文。

这能短期引导模型用对工具，但本质上是在 prompt 层消化运行环境路由。后续统一后，prompt 不应该根据本地/云端项目生成不同策略，也不应该要求 AI 理解 `local_connector` 和 `harness_code` 的差异；这些应该由 resolver 和工具门面在运行期处理。

### 8. 固定 MCP 选择散落在不同 agent / profile 路径

除了用户或 planner 显式选择之外，代码里还有多处“这个 agent 固定需要某些 MCP”的逻辑：

- Chatos async planner 在 `request_guards.rs` 里把 `TaskManager`、`AskUser` 写进 `enabled_builtin_kinds`。
- `workspace_mcp.rs` 的 `plan_task_runtime_builtin_kinds()` 为 `chatos_plan` profile 固定返回一批 builtin allowlist。
- `mcp_builder.rs` 对 `chatos_plan` 额外塞入 Project Management、Task Runner skill lookup、disabled code write provider 等运行期 provider。
- 创建任务和 follow-up 任务路径会各自调用 local connector routing，把被本地宿主替代的能力从 `enabled_builtin_kinds` 移走。

这些逻辑本质上都应该表达为“调用方固定要求哪些 capability”，而不是各自直接修改 MCP 配置或 runtime server 列表。调用方只传 required capability set，Task Runner 的 resolver 统一完成依赖补齐、系统注入、host routing 和 AI-facing 工具门面。

## 统一概念模型

建议把 MCP 选择拆成五层，同时把 AI-facing 视图和内部路由视图明确分开。

### 1. Requested capabilities

用户或 planner 显式要求任务具备的能力。

示例：

- `CodeMaintainerRead`
- `CodeMaintainerWrite`
- `TerminalController`
- `BrowserTools`
- `WebTools`
- `RemoteConnectionController`
- `Notepad`

这是 UI checkbox 应该表达的东西。

如果用户创建的是本地项目读代码任务，`CodeMaintainerRead` 应该仍然属于 requested capability，即使运行时由 `local_connector` 提供。

这一层是 UI checkbox、planner `enabled_builtin_kinds` 和任务意图应该表达的核心语义。它回答的是“任务需要什么能力”，不是“这个能力由哪个 server 提供”。

关键规则：

- 读文件任务：`CodeMaintainerRead` 必须在 requested capabilities 里。
- 写文件任务：`CodeMaintainerWrite` 必须在 requested capabilities 里，并通过依赖规则补齐 `CodeMaintainerRead`。
- 运行命令任务：`TerminalController` 必须在 requested capabilities 里。
- 浏览器观察/操作任务：`BrowserTools` 必须在 requested capabilities 里。
- 这些能力进入 requested capabilities 后，不应该因为项目是 local connector 或 Harness 云端项目就从选择状态里消失。

### 2. Required capabilities

调用方、agent 类型或 task profile 固定要求任务具备的能力。

示例：

- Chatos async task 固定需要 `TaskManager`、`AskUser`。
- Project Management / `chatos_plan` agent 固定需要 `ProjectManagement`，并可声明规划阶段需要的 memory reader、web/browser/notepad 等能力。
- Local Connector client agent 如果要操作当前本地项目，应声明它固定需要的 `CodeMaintainerRead` / `CodeMaintainerWrite` / `TerminalController` / `BrowserTools`。
- Task Runner 内部 run phase 如果需要 task process log、skill lookup 这类内部 provider，也应该作为 required/internal requirement 进入 resolver，而不是在 builder 里临时 push。

required capabilities 的特点：

- 不来自用户 checkbox，不能被普通编辑界面取消。
- UI 应显示为“调用方固定需要”或“profile 固定需要”。
- 仍然参与 dependency completion，例如 required `CodeMaintainerWrite` 自动补齐 required/read-visible `CodeMaintainerRead`。
- 仍然参与 host replacement，例如 required `TerminalController` 在本地项目可路由到 `local_connector`，在 server-local 项目可路由到 builtin terminal。
- 调用方传 capability，不传 provider。也就是说传 `TerminalController`，不要传 `local_connector_execute_command`。

这一层解决“项目管理 agent、local connector client agent、Task Runner 自己固定要选 MCP”的统一收口问题。

### 3. System capabilities

后端根据 profile、调度模式或调用来源自动加上的能力。

示例：

- 运行期 sandbox 文件/终端替代能力。
- task process log internal provider。
- Task Runner skill lookup internal provider。

这些能力和 required capabilities 一样不应该作为普通 checkbox 让用户勾选。区别是 required 表示调用方明确声明的固定依赖，system 表示 Task Runner 根据运行环境、profile 或安全策略派生出来的内部能力。

注意：这里说的 sandbox 是普通任务执行时的隔离运行环境 overlay，它只会在运行期替换文件读写和终端执行的真实 provider；不是 Project Management 环境初始化 agent 使用的“沙箱镜像 MCP”。

### 3.1 沙箱镜像 MCP 边界

沙箱镜像 MCP 是 Project Management Service 的项目运行环境初始化 agent 专用能力，不进入 Task Runner 普通任务的 requested/required capability，也不出现在任务 MCP checkbox 中。

当前代码里这条链路已经是相对清晰的抽象：

- AI 面向同一个 MCP server：`sandbox_images`。
- 工具语义固定：搜索镜像、创建/复用镜像、读取镜像目录。
- 本地实现走 Local Connector Client 的 `/api/local/sandbox/images/mcp`，云端实现走 Sandbox Manager 的 `/api/sandbox-images/mcp`。
- 本地和云端都实现共享 crate `chatos_sandbox_image_mcp::SandboxImageBackend`。
- 路由发生在 Project Management Service 的 environment agent 内部，根据项目运行环境和 sandbox provider 选择 local/cloud backend。

因此 MCP 统一规划里不应该新增一个通用 `SandboxImage` builtin 给 Task Runner。它如果需要继续收口，收口点应在 Project Management 的 environment agent MCP routing，而不是 Task Runner 的任务 MCP resolver。

### 3.2 沙箱内部 MCP 工具边界

普通任务运行时使用的 sandbox MCP 也不应该成为一类新的用户可选 MCP。它的真实含义是：当任务启用隔离运行环境时，`CodeMaintainerRead`、`CodeMaintainerWrite`、`TerminalController` 这些稳定能力由 sandbox 内部 agent 执行。

当前实现已经基本符合这个方向：

- sandbox agent 内部复用 `chatos_builtin_tools::CodeMaintainerService` 和 `TerminalControllerService`，没有另起一套工具语义。
- Sandbox Manager 暴露 `/api/sandboxes/:sandbox_id/mcp`，只做鉴权、lease 校验、tool allowlist 和 proxy。
- Local Connector 本地沙箱 facade 也按同一套 `/api/sandboxes/:sandbox_id/mcp` 代理到本地 sandbox agent。
- Task Runner 在运行期发现需要 sandbox 后，把普通 builtin 文件/终端 provider 从 server-local 列表里移除，再挂载名为 `sandbox` 的 HTTP MCP。

因此不需要新增 `SandboxRead` / `SandboxWrite` / `SandboxTerminal` 抽象，也不需要在 Chatos/UI/task config 里出现 sandbox 内部 MCP checkbox。

AI-facing 暴露方式已经按同一原则收口：不在 prompt 里提示模型使用 `sandbox_*` 工具，而是和 local/harness 一样，把 sandbox HTTP server 的工具 alias 到稳定 builtin server prefix：

- `read_file_raw` / `list_dir` / `search_text` 等暴露为 `code_maintainer_read_*`。
- `write_file` / `apply_patch` 等暴露为 `code_maintainer_write_*`。
- `execute_command` / `process_*` 等暴露为 `terminal_controller_*`。

这样模型只看到“读、写、终端”能力，Task Runner 内部根据运行环境决定这些能力由 server-local、local connector、harness code 还是 sandbox 执行。

### 4. Hosted capabilities

原本属于 builtin 的能力，但在当前任务中由宿主 MCP 提供。

当前已有宿主：

- `LocalConnector`
  - 替代 `CodeMaintainerRead`
  - 替代 `CodeMaintainerWrite`
  - 替代 `TerminalController`
  - 替代 `BrowserTools`
- `HarnessCode`
  - 替代 `CodeMaintainerRead`
  - 替代 `CodeMaintainerWrite`

这些能力可以在 UI 详情、运行快照、排障日志里显示为：

- `CodeMaintainerRead via local_connector`
- `CodeMaintainerWrite via harness_code`

而不是从 requested capabilities 里删除。

这里的 `via ...` 是给人看实现路由的调试信息，不是模型 prompt 的一部分。AI 不需要知道能力是本地宿主、云端 Harness，还是 Task Runner server-local provider 提供的。

当前 `chatos_mcp_service::HostCapabilityPolicy` 已经把可替代能力抽象成四类：

- code read，对应 `CodeMaintainerRead`
- code write，对应 `CodeMaintainerWrite`
- terminal，对应 `TerminalController`
- browser，对应 `BrowserTools`

现有实现能力边界：

- `local_connector` 可以替代 read/write/terminal/browser。
- `harness_code` 当前只替代 read/write。
- 如果以后云端也支持终端或浏览器，只应该新增 host implementation，不应该改变任务层的能力选择语义。

也就是说，`CodeMaintainerRead`、`CodeMaintainerWrite`、`TerminalController`、`BrowserTools` 应该被看作稳定的能力接口；`code_maintainer_*`、`local_connector_*`、`harness_code_*` 是不同环境下的具体实现。

### 5. Effective runtime routes

最终执行时，resolver 会把 capability 映射到具体 provider route。

示例：

- server-local builtin：`code_maintainer_read_*`
- host MCP：`local_connector_read_file_raw`
- host MCP：`harness_code_read_file_raw`
- external MCP：用户配置的外部 MCP tools
- internal MCP：task process log、skill lookup 等运行期内部工具

这个视图只服务后端执行、详情页、运行快照和排障日志。它不应该直接成为 prompt contract，也不应该要求 AI 知道 `local_connector_*` 和 `harness_code_*` 的差异。

## AI-facing contract

对模型暴露的契约应该是稳定能力，而不是环境实现。

规则：

- planner/schema 仍然使用 `CodeMaintainerRead`、`CodeMaintainerWrite`、`TerminalController`、`BrowserTools` 这类 capability 名称。
- prompt 不根据 local/cloud 变化；不要写“本地用 `local_connector_*`，云端用 `harness_code_*`”这类分支规则。
- prompt preview 使用 resolver 产出的 AI-facing capability manifest，只展示模型实际可用的稳定能力/工具语义。
- provider route 只出现在 UI 详情、debug 面板、运行快照和日志里。
- 如果当前 MCP runtime 必须用 server 前缀注册工具名，应把它当作运行时工具门面的缺口来修，而不是靠 prompt 教模型区分实现。

建议新增 `ToolFacade` / `CapabilityToolNamespace`：

- `CodeMaintainerRead` 映射到当前项目可用的读实现。
- `CodeMaintainerWrite` 映射到当前项目可用的写实现。
- `TerminalController` 映射到当前项目可用的终端实现。
- `BrowserTools` 映射到当前项目可用的浏览器实现。

可选实现方式：

- 在 MCP 注册阶段生成稳定 alias。
- 建一个 selected-provider proxy MCP server，由它按 resolution 转发到 local/harness/server-local provider。
- 在工具描述层统一 capability namespace，内部保留真实 provider route。

最终效果是：AI 只按“我有读/写/终端/浏览器能力”行动；本地还是云端由 Task Runner 内部决定。

## 建议新增核心结构

在 Task Runner 后端新增统一 resolver，例如：

`task_runner_service/backend/src/services/mcp_resolution.rs`

建议核心输出结构：

```rust
pub struct TaskMcpResolutionInput {
    pub task_mcp_config: TaskMcpConfig,
    pub task_profile: String,
    pub schedule_mode: TaskScheduleMode,
    pub project_id: String,
    pub project_route_context: Option<ProjectRouteContext>,
    pub caller_requirements: Vec<McpCapabilityRequirement>,
}

pub struct McpCapabilityRequirement {
    pub kind: BuiltinMcpKind,
    pub source: McpCapabilityRequirementSource,
    pub cancellable: bool,
}

pub enum McpCapabilityRequirementSource {
    CallerContract(AgentMcpCaller),
    TaskProfile(TaskProfileKind),
    RuntimeInternal(RuntimeInternalProviderKind),
}

pub enum AgentMcpCaller {
    ChatosAsyncPlanner,
    ChatosPlanAgent,
    ProjectManagementAgent,
    LocalConnectorClientAgent,
    TaskRunnerRunPhase,
}

pub struct TaskMcpResolution {
    pub requested_builtin_kinds: Vec<BuiltinMcpKind>,
    pub required_builtin_kinds: Vec<RequiredBuiltinCapability>,
    pub system_builtin_kinds: Vec<SystemInjectedBuiltin>,
    pub ai_visible_builtin_kinds: Vec<BuiltinMcpKind>,
    pub ai_tool_manifest: Vec<AiVisibleToolCapability>,
    pub hosted_builtin_routes: Vec<HostedBuiltinRoute>,
    pub server_local_builtin_kinds: Vec<BuiltinMcpKind>,
    pub internal_runtime_routes: Vec<InternalMcpRoute>,
    pub external_mcp_config_ids: Vec<String>,
    pub ephemeral_http_servers: Vec<TaskEphemeralHttpMcpServer>,
    pub skill_ids: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct RequiredBuiltinCapability {
    pub kind: BuiltinMcpKind,
    pub source: McpCapabilityRequirementSource,
}

pub struct SystemInjectedBuiltin {
    pub kind: BuiltinMcpKind,
    pub reason: SystemInjectionReason,
}

pub struct HostedBuiltinRoute {
    pub host: BuiltinHostBackend,
    pub server_name: String,
    pub builtin_kinds: Vec<BuiltinMcpKind>,
    pub tool_prefix: String,
}

pub struct AiVisibleToolCapability {
    pub kind: BuiltinMcpKind,
    pub namespace: String,
}

pub struct InternalMcpRoute {
    pub kind: BuiltinMcpKind,
    pub provider: RuntimeRouteProvider,
    pub server_name: String,
    pub tool_prefix: String,
}
```

resolver 的输入应该包含：

- `TaskMcpConfig`
- `task_profile`
- `schedule.mode`
- `source_session_id/source_user_message_id`
- `project_id`
- Task Runner 根据 `project_id` 解析出的 project route context
- 调用方传入的 `caller_requirements`
- default workspace
- 运行期 sandbox 执行策略由 run phase 单独计算，不把沙箱镜像 MCP 放入 Task Runner resolver

所有现有规则都应该逐步收敛到这个 resolver。

调用方只应该构造 `McpCapabilityRequirement`：

- Chatos async planner 传 `TaskManager`、`AskUser`，source 为 `CallerContract(ChatosAsyncPlanner)`。
- Project Management / plan agent 传 `ProjectManagement` 和规划必需 capability，source 为 `CallerContract(ProjectManagementAgent)` 或 `TaskProfile(ChatosPlan)`。
- Local Connector client agent 传它需要的稳定 capability，例如 `CodeMaintainerRead`、`TerminalController`、`BrowserTools`，source 为 `CallerContract(LocalConnectorClientAgent)`。
- Task Runner run phase 传 task process log / skill lookup 这类 runtime internal requirement。

resolver 负责把这些 requirement 和 requested capabilities 合并、去重、补依赖，再根据项目环境选择 local/harness/server-local route。

## 固定 MCP 调用方改造表

| 当前位置 | 当前行为 | 目标行为 |
| --- | --- | --- |
| `mcp_server/chatos_async_planner/request_guards.rs` | 把 `TaskManager`、`AskUser` 直接写入 `enabled_builtin_kinds` | 构造 `CallerContract(ChatosAsyncPlanner)` requirements，由 resolver 合并 |
| `mcp_server/chatos_async_planner/schema.rs` | schema 文案单独说明不要选 `TaskManager` / `AskUser` | schema 从 catalog policy 读取 selectable/required 元数据 |
| `workspace_mcp.rs::plan_task_runtime_builtin_kinds()` | `chatos_plan` profile 固定返回 builtin allowlist | profile/caller contract 声明 required capabilities，resolver 决定 route |
| `run_model_phase/setup/preparation/mcp_builder.rs` | `chatos_plan` 里临时 push code write、Project Management、skill lookup、process log provider | builder 只消费 `TaskMcpResolution` 的 `server_local_builtin_kinds` / `internal_runtime_routes` |
| `task_service/tasks/mutations/creation.rs` | 创建任务时直接调用 local connector routing | 创建任务只保存 requested/required 语义；routing 由 resolver 派生 |
| `task_manager_bridge/task_ops.rs` | follow-up 任务继承配置后再次调用 local connector routing | follow-up 继承 requested/required 语义；routing 由 resolver 派生 |
| Local Connector client agent 入口 | 需要本地能力时容易直接绑定 `local_connector` 实现 | 只传 `CodeMaintainerRead` / `CodeMaintainerWrite` / `TerminalController` / `BrowserTools` 等 required capabilities |

迁移完成后，任何 agent 新增固定 MCP 都不应该再修改 `enabled_builtin_kinds`、`ephemeral_http_servers` 或直接 push runtime server。它只新增一条 caller requirement policy。

## Catalog 元数据补齐

需要把 `BuiltinMcpKind` 的元信息补成结构化 policy，而不是让各处写 match。

建议增加：

```rust
pub struct BuiltinKindPolicy {
    pub kind: BuiltinMcpKind,
    pub selectable: bool,
    pub default_for_manual_task: bool,
    pub required_by_callers: Vec<AgentMcpCaller>,
    pub required_by_profiles: Vec<TaskProfileKind>,
    pub internal_provider_for: Vec<RuntimeInternalProviderKind>,
    pub host_replaceable_by: Vec<BuiltinHostBackend>,
    pub dependencies: Vec<BuiltinMcpKind>,
}
```

然后由它生成：

- Task Runner MCP schema enum。
- async planner schema enum。
- 前端 catalog。
- AI-facing capability manifest / prompt preview。
- runtime resolver。
- UI 是否可勾选、是否 required、是否系统注入、是否 host routed。

## 实施阶段

### 阶段 1：补测试，锁住语义边界

先不要大改行为，先补覆盖关键场景的测试。对当前已知不合理但要修的行为，可以先写成 pending/ignored 测试，或放进新 resolver 单元测试里作为目标约束。

建议新增/扩展测试：

- `workspace_mcp.rs`
  - local connector 任务请求 `CodeMaintainerRead` 后，requested 仍能还原为 read。
  - caller required `CodeMaintainerRead` 后，local connector / harness code routing 和普通 requested 走同一套逻辑。
  - caller required `CodeMaintainerWrite` 后，自动补齐 `CodeMaintainerRead`，并且两者来源都能追溯。
  - local connector route 生成 header 后，effective server-local 不再包含 `CodeMaintainerRead`。
  - harness code 任务请求 `CodeMaintainerWrite` 后，自动包含 read，并路由到 `harness_code`。
  - `chatos_plan` + local connector 不丢失规划所需系统能力。
- `mcp_catalog_service/prompt_preview.rs`
  - preview 使用同一个 resolution 的 AI-facing view，而不是单独用默认 runtime kinds。
- `mcp_server/chatos_async_planner/schema.rs`
  - TaskManager/AskUser 从可选 enum 移除，但在 resolution 中作为 required capability 出现。
- `mcp_server/chatos_async_planner/request_guards.rs`
  - 不再直接写 `enabled_builtin_kinds`；改成传 caller requirements 后，resolution 中仍出现 `TaskManager`、`AskUser`。
- `run_model_phase/setup/preparation/mcp_builder.rs`
  - `chatos_plan` 固定需要的 Project Management、skill lookup、process log requirement 都能从 resolution 出来。
- 前端单测
  - table/detail 显示 hosted read capability。
  - editor checkbox 表示 requested capability，而不是 server-local effective builtin。
  - required capability 显示为不可取消的 required tag，而不是普通 checkbox。

### 阶段 2：引入 resolver，但保持现有存储兼容

新增 `TaskMcpResolution`，先让它复刻当前行为。

改造优先级：

1. 新增 `McpCapabilityRequirement` / `AgentMcpCaller`
   - 先支持 Chatos async planner、Chatos plan / Project Management agent、Local Connector client agent、Task Runner run phase。
   - 所有 required capability 都以 `BuiltinMcpKind` 表达，不使用 provider-specific 工具名。
2. `workspace_mcp.rs`
   - `selected_builtin_kinds`
   - `runtime_selected_builtin_kinds`
   - `selected_local_connector_builtin_kinds_for_config`
   - `apply_local_connector_routing`
   - `apply_harness_project_routing_to_task`
3. `mcp_server/chatos_async_planner/request_guards.rs`
   - 删除直接向 `enabled_builtin_kinds` 写入 `TaskManager`、`AskUser` 的职责，改成构造 `caller_requirements`。
4. `run_model_phase/setup/preparation/mcp_builder.rs`
   - 从 resolver 读取 `server_local_builtin_kinds` 和 `ephemeral_http_servers`。
   - 删除 `chatos_plan` 下临时 push builtin server/provider 的特殊逻辑，改由 resolution 输出 internal runtime routes。
5. `mcp_catalog_service/prompt_preview.rs`
   - preview 走 resolver 的 AI-facing view。

阶段 2 的目标是：行为尽量兼容现状，但“真实决策入口”只有一个。新代码不再直接改写 `enabled_builtin_kinds` 来表达固定 MCP。

### 阶段 3：建立 AI-facing tool facade

这一阶段解决“运行时真实 provider 名称”和“模型应该看到的能力语义”之间的映射。

建议实现：

- 在 MCP executor 注册工具前，根据 `TaskMcpResolution` 生成稳定的 capability namespace。
- 让 `CodeMaintainerRead`、`CodeMaintainerWrite`、`TerminalController`、`BrowserTools` 对模型表现为稳定工具集合。
- 内部按 `internal_runtime_routes` 转发到 server-local builtin、`local_connector`、`harness_code` 或后续其它 host。
- 如果短期不能完全重命名真实 MCP tools，至少把 provider-specific 工具说明从 prompt 中移除，改成工具注册/描述层统一处理。
- `run_model_phase/setup/preparation/mcp_inputs.rs` 不再按 local/harness 拼接 provider-specific prompt 说明；如需工具说明，从 `ai_tool_manifest` 生成稳定能力文案。

这一阶段的目标是：AI 不需要知道项目是本地还是云端，也不需要学 `local_connector_*` / `harness_code_*` 的差异。

### 阶段 4：API 增加 effective MCP 视图

新增一个只读 API，先不破坏旧 `TaskRecord`：

- `GET /api/tasks/:id/mcp/resolution`
- 或在 task detail response 中增加 `mcp_resolution`

建议返回：

```json
{
  "requested_builtin_kinds": ["CodeMaintainerRead"],
  "required_builtin_kinds": [
    { "kind": "TaskManager", "source": "chatos_async_planner" },
    { "kind": "AskUser", "source": "chatos_async_planner" }
  ],
  "system_builtin_kinds": [],
  "ai_visible_builtin_kinds": ["CodeMaintainerRead", "TaskManager", "AskUser"],
  "ai_tool_manifest": [
    { "kind": "CodeMaintainerRead", "namespace": "code_maintainer_read" }
  ],
  "hosted_builtin_routes": [
    {
      "host": "local_connector",
      "server_name": "local_connector",
      "builtin_kinds": ["CodeMaintainerRead"],
      "tool_prefix": "local_connector"
    }
  ],
  "server_local_builtin_kinds": ["TaskManager", "AskUser"],
  "internal_runtime_routes": [
    {
      "kind": "CodeMaintainerRead",
      "provider": "local_connector",
      "server_name": "local_connector",
      "tool_prefix": "local_connector"
    }
  ],
  "external_mcp_config_ids": [],
  "skill_ids": []
}
```

前端先用这个 API 展示“有效 MCP”，不急着改写任务保存格式。

### 阶段 5：前端统一展示语义

前端改成四块展示：

1. Requested capabilities
   - checkbox
   - 表示用户/planner 要求这项能力
   - local/cloud 任务里 `CodeMaintainerRead` 仍然应该显示选中

2. Required capabilities
   - tag
   - 不可勾选
   - 说明来源：Chatos async planner、Project Management agent、Local Connector client agent、task profile 等

3. System injected / internal
   - tag
   - 不可勾选
   - 说明来源：运行期 sandbox 文件/终端替代、process log、skill lookup、运行期内部 provider 等

4. Effective runtime route
   - tag 或列表
   - 显示真实工具来源：
     - `CodeMaintainerRead -> local_connector`
     - `CodeMaintainerRead -> harness_code`
     - `ProjectManagement -> project_management_service`

列表页的 MCP 列不要再只显示 `enabled_builtin_kinds.length`，应显示：

- requested count
- required count
- system count
- hosted count
- external count
- skill count

详情页使用同一份 resolution 展示 requested/required/system/route 信息。prompt preview 也使用同一份 resolution，但只展示 AI-facing capability manifest，不展示 local/harness/server-local route。

### 阶段 6：调整持久化模型

在 UI/API 先稳定后，再考虑持久化字段重构。

推荐方向：

```rust
pub struct TaskMcpConfig {
    pub enabled: bool,
    pub selection_mode: TaskMcpSelectionMode,
    pub requested_builtin_kinds: Vec<String>,
    pub enabled_builtin_kinds: Vec<String>, // legacy compatibility
    pub hosted_builtin_routes: Vec<TaskHostedBuiltinRoute>,
    ...
}
```

兼容策略：

- 新任务写 `requested_builtin_kinds`。
- 旧任务如果没有 `requested_builtin_kinds`，从 `enabled_builtin_kinds` 和 `ephemeral_http_servers` header 反推。
- 一段时间内继续写 `enabled_builtin_kinds`，避免旧客户端坏掉。
- `ephemeral_http_servers` 只保留真正运行所需的 HTTP server 信息，不再承担“用户选了哪些能力”的语义。

如果不想新增太多字段，也可以先只加：

- `requested_builtin_kinds`
- `mcp_resolution` 只读输出

把 `hosted_builtin_routes` 继续作为 resolver 派生结果。

### 阶段 7：清理重复规则

resolver 稳定后清理这些重复来源：

- 前端 `systemInjectedMcpServerNames()` 的硬编码。
- async planner 的 `SYSTEM_INJECTED_BUILTIN_KINDS` 硬编码。
- prompt preview / `mcp_inputs.rs` 里的默认 builtin 推导和 provider-specific prompt 分支。
- `workspace_mcp.rs` 里散落的 profile/runtime/host replacement 判断。
- catalog 中关于 ProjectManagement 的特殊 message 逻辑，改为来自 policy。

## 关键设计决定

### 1. checkbox 表达“请求能力”，不是“server-local provider”

本地/云端任务里，`CodeMaintainerRead` 应该能保持选中，因为任务确实请求了代码读取能力。

但 effective route 应显示它由 `local_connector` 或 `harness_code` 提供，而不是由 server-local `code_maintainer_read` 提供。

同理：

- `CodeMaintainerWrite` 选中表示任务需要写项目文件；本地项目可路由到 `local_connector_write_file`/`local_connector_edit_file`，Harness 项目可路由到 `harness_code_write_file`/`harness_code_edit_file`。
- `TerminalController` 选中表示任务需要项目执行命令；本地项目可路由到 `local_connector_execute_command`，Task Runner server-local 项目可用 builtin terminal。Harness 当前没有终端实现时，应在 resolution 里明确显示“无 host implementation”，而不是静默取消能力。
- `BrowserTools` 选中表示任务需要浏览器观察或操作；本地项目可路由到 `local_connector_browser_*`，server-local 项目可用 builtin browser tools。Harness 当前没有浏览器实现时，同样应通过 resolution 暴露限制。

### 2. required/system capability 不要混进普通勾选项

`TaskManager`、`AskUser`、`ProjectManagement` 等应该按来源显示为 required capability 或 system capability。

它们不应该靠“偷偷写进 enabled_builtin_kinds”来让 UI 看起来一致。

判断原则：

- 如果是某个调用方或 agent 固定声明的依赖，例如 Chatos async planner 需要 `TaskManager` / `AskUser`，显示为 required。
- 如果是 Task Runner 根据运行环境、安全策略、sandbox 或内部运行阶段派生的 provider，显示为 system/internal。

### 3. host replacement 不能丢失原始意图

当前把 `CodeMaintainerRead` 从 `enabled_builtin_kinds` 中删除，会导致 UI 丢失“任务需要读代码”的原始意图。

后续应该保留 requested capability，只在 effective runtime 阶段决定 server-local provider 是否被 host MCP 替代。

### 4. Chatos 不应该承担 Task Runner 内部 MCP 规则

Chatos 只负责把稳定的任务上下文传给 Task Runner：

- project id
- task profile
- source session/message
- remote server config

project root、workspace、local connector / Harness 等实现细节不从 Chatos 透传。Task Runner 根据 project id 解析项目类型，并统一完成 MCP 能力到具体 provider 的路由。

### 5. prompt 不表达云/本地差异

prompt 层不应该出现“本地项目用 `local_connector_*`，云端项目用 `harness_code_*`”这类业务分支。AI 只需要知道当前有哪些稳定能力，至于能力背后的 provider route 由 Task Runner 内部 resolver 和 tool facade 负责。

当前 `mcp_inputs.rs` 里的 local/harness note 应视为过渡债务：它可以解释现状，但不是目标架构的一部分。

## 风险与注意点

- 历史任务中 `enabled_builtin_kinds` 已经被 local/harness routing 改写过，需要从 `ephemeral_http_servers` header 反推 requested capability。
- prompt 文案和 schema 文案目前有编码异常痕迹，重构时不要扩大问题；同时不要新增 provider-specific prompt 分支。
- `chatos_plan` 当前有固定 allowlist，不能简单套普通任务规则。
- 运行期 sandbox 会替代部分文件/终端 builtin，但这是执行环境 overlay，不是沙箱镜像 MCP；普通任务 resolver 只需要表达有效 provider，不要把镜像工具纳入 Task Runner capability。
- external MCP 和 hosted builtin MCP 要分开：`local_connector`/`harness_code` 是内部 host route，不应该和用户配置的 external MCP 混成一个概念。

## 推荐落地顺序

1. 新增 `TaskMcpResolution` 和测试，先复刻当前行为。
2. 让运行期 builder、prompt preview、catalog 逐步改用 resolver。
3. 建立 AI-facing tool facade，移除 prompt 对 local/harness 的 provider-specific 分支依赖。
4. 暴露 resolution API。
5. 前端用 resolution 展示 requested/required/system/hosted/effective 信息。
6. 新增 `requested_builtin_kinds` 兼容字段，停止把 host replacement 当成删除用户选择。
7. 清理散落硬编码和旧展示逻辑。

## 验收标准

- 本地项目任务请求读代码时，UI 能看到 `CodeMaintainerRead` 是 requested capability，并显示 `via local_connector`。
- 云端 Harness 项目任务请求读代码时，UI 能看到 `CodeMaintainerRead` 是 requested capability，并显示 `via harness_code`。
- 本地项目任务请求写代码时，UI 能看到 `CodeMaintainerWrite` 和自动补齐的 `CodeMaintainerRead` 都是 requested capability，并显示 `via local_connector`。
- 本地项目任务请求终端命令时，UI 能看到 `TerminalController` 是 requested capability，并显示 `via local_connector`。
- 本地项目任务请求浏览器操作时，UI 能看到 `BrowserTools` 是 requested capability，并显示 `via local_connector`。
- Harness 云端项目如果暂不支持终端或浏览器实现，resolution/UI 必须明确提示该能力没有当前项目 host implementation，不能把 checkbox 或 requested capability 静默清掉。
- Chatos async task 不需要用户勾选 `TaskManager`/`AskUser`，但 UI 能显示它们是 caller required。
- Chatos plan / Project Management agent task 能显示 `ProjectManagement` 是 caller/profile required，并能区分普通任务不可选。
- Project Management agent、Local Connector client agent、Task Runner run phase 这类固定 MCP 来源都通过 `caller_requirements` / profile requirements 进入 resolver，不再各自写 `enabled_builtin_kinds` 或直接 push runtime server。
- 调用方传入固定能力时只传 `BuiltinMcpKind` / capability，不传 `local_connector`、`harness_code` 或 server-local 工具名。
- 同一个 required capability 在本地项目、Harness 项目、server-local 项目下能走不同 provider route，但 UI/AI-facing contract 的 capability 名称不变。
- `CodeMaintainerWrite` 在所有路径下都自动补齐 `CodeMaintainerRead`。
- prompt preview、运行期实际 tools、详情页展示、列表页统计使用同一份 resolution；其中 prompt preview 只展示 AI-facing capability manifest。
- prompt / prompt preview 不再出现要求模型区分 `local_connector_*` 和 `harness_code_*` 的分支说明。
- 旧任务不迁移也能正确展示 effective MCP；保存后不会丢失 requested capability。
