# Task 级 Plugin 选择与 Local Connector 本地执行方案

> 编写日期：2026-08-21
> 适用范围：ChatOS、Task Runner、Plugin Management、MCP Management、Project Management、Local Connector Service/Client
> 文档状态：已实施并完成真实链路验收
> 本文是 Task Plugin 发现、选择、持久化和本地执行的实现基线。旧的 Conversation 级 Plugin 工具授权与选择传输不属于最终架构。

## 1. 目标

最终调用链固定为：

```text
ChatOS 收到用户请求
  -> 根据项目解析 Local Connector device_id
  -> 获取该设备可供 Task Runner 使用的 Plugin 描述
  -> ChatOS 创建 Task 并提交非可信 plugin_hints
  -> Task Runner 解析、匹配并验证可信 TaskPluginConfig
  -> TaskPluginConfig 固化到 TaskRecord
  -> Task Runner 启动 Task
  -> MCP Management 再次验证并准备本地路由
  -> Local Connector Service Relay
  -> Local Connector Client 启动或调用本地 stdio/HTTP MCP
  -> 工具结果返回 Task Runner
```

必须满足：

1. ChatOS Conversation Agent 不获得 Plugin 工具，也不能直接调用 Plugin。
2. Plugin 选择属于具体 Task，不属于 Conversation Runtime。同一次对话创建的不同 Task 可以选择不同 Plugin。
3. AI 只能提交 `plugin_hints`，不能直接构造可信的 `selected_plugins`、Release、设备或运行路由。
4. Task Runner 是可信 Plugin 选择和持久化边界。
5. Plugin Management 是 Catalog、Release、安装、权限、认证和 Agent policy 的控制面。
6. MCP Management 只准备和路由执行；所有 stdio/HTTP Plugin MCP 都必须由 Local Connector Client 本地执行。
7. 不允许服务端直接执行、服务端 fallback、设备替换或插件不可用时静默降级。

## 2. 职责边界

| 模块 | 最终职责 |
| --- | --- |
| ChatOS | 获取可供 Task 使用的 Plugin 描述；根据用户需求在创建 Task 时给出 `plugin_hints`；自身不持有 Plugin 工具 |
| Task Runner | 解析项目设备上下文；校验 hints；生成可信 `TaskPluginConfig`；写入 TaskRecord；执行前重新验证 |
| Plugin Management | 管理 Marketplace、Catalog、Release、安装、权限、认证、组件快照和 Agent binding/policy |
| Project Management | 以项目记录为权威来源解析 Local Connector `device_id`、`workspace_id` 和相对根目录 |
| MCP Management | 根据 TaskRecord 中的可信选择和项目上下文创建 runtime session 与 Local Connector route |
| Local Connector Service | 只承担双向 Relay 和设备连接管理 |
| Local Connector Client | 下载、验证、安装、授权并启动本地 npm Plugin MCP；执行 stdio/HTTP MCP 调用 |

## 3. 项目 Plugin 运行上下文

Task Runner 新增统一的项目 Plugin 上下文解析器，创建目录、创建 Task 和启动 Task 必须复用同一逻辑：

```rust
pub struct TaskPluginRuntimeContext {
    pub owner_user_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub runtime_provider: String,
    pub project_context_revision: String,
}
```

解析规则：

1. 使用服务端 `project_id` 和 `owner_user_id` 调用 Project Management execution-context internal API。
2. `workspace_provider` 必须为 `local_connector`。
3. `workspace.device_id` 和 `workspace.workspace_id` 必须非空。
4. 不接受前端、ChatOS Prompt 或 AI 参数提供的设备身份。
5. 调用 Plugin Management 时必须同时传递 `runtime_provider=local_connector` 和真实 `device_id`。
6. Public Project 或未绑定 Local Connector Workspace 的项目不能发现或选择本地 Plugin。

当前 `resolve_task_runner_policy_for_agent_project` 忽略 `project_id` 的实现必须删除。Task 创建和 Run 路径都必须通过同一 resolver 构造 `ResolveAgentCapabilitiesRequest`。

## 4. Task Plugin 候选目录

Task Runner 对 ChatOS 只返回经过设备、安装和 policy 过滤后的描述 DTO：

```rust
pub struct TaskPluginCandidate {
    pub plugin_key: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub usage_examples: Vec<String>,
    pub components: Vec<TaskPluginCandidateComponent>,
}
```

候选目录不得包含 MCP URL、npm 启动命令、Header、Token、OAuth secret、Relay 地址、安装绝对路径或其他运行凭证。

Plugin Release Manifest 增加选择元数据：

```json
{
  "selection": {
    "capabilities": [
      "computer.use",
      "computer.application.list",
      "computer.application.control"
    ],
    "usage_examples": [
      "列出本机正在运行的应用",
      "打开一个桌面应用",
      "查看当前屏幕内容"
    ]
  }
}
```

## 5. ChatOS Runtime 隔离

ChatOS Conversation Agent 创建 MCP Management session 时必须固定：

```rust
selected_plugins: Vec::new(),
plugin_command_invocations: Vec::new(),
```

不得再根据 Chat stream 的选择为 ChatOS materialize Plugin MCP/tool component。可供 Task 使用的 Plugin 描述通过独立的 Prompt context 注入，Prompt 必须明确：

```text
这些 Plugin 只能分配给创建的 Task。ChatOS 不能直接调用这些 Plugin。
```

## 6. create_task 协议

AI 继续禁止提交 `selected_plugins`、`TaskPluginConfig`、Release ID、device ID 和运行参数。`create_task` 新增非可信提示：

```rust
pub struct CreateTaskPluginHint {
    pub plugin_key: String,
    pub reason: Option<String>,
}

pub struct CreateTaskArgs {
    // existing fields
    pub plugin_hints: Vec<CreateTaskPluginHint>,
}
```

批量任务、前置依赖任务、项目需求执行任务和子任务必须为每个 Task 单独提供 hints，不能复用 Conversation 级选择。

## 7. 可信选择器

Task Runner 新增：

```rust
pub struct TrustedTaskPluginSelector;

impl TrustedTaskPluginSelector {
    pub async fn resolve_plugin_config_override(
        &self,
        task: &CreateTaskRequest,
        plugin_hints: &[CreateTaskPluginHint],
        context: &McpRequestContext,
    ) -> Result<TrustedTaskPluginSelection, TaskPluginError>;
}
```

选择器必须：

1. 解析真实项目和设备上下文。
2. 以实际 Task Runner planning/run Agent 查询 Plugin Management。
3. 仅在当前 owner、项目、设备和 policy 的候选集中解析 `plugin_key`。
4. 自动加入 required Plugin。
5. 拒绝未知、未安装、禁用、离线、版本或哈希不一致、缺依赖、缺权限、缺认证的 Plugin。
6. 生成内部 `SelectedPluginRef`，AI 不接触内部 ID。
7. 对每个 Task 独立执行，支持一次 Conversation 创建不同 Plugin 配置的多个 Task。

## 8. TaskRecord 持久化

现有 `TaskRecord.plugin_config` 继续作为执行授权范围。增加选择审计快照：

```rust
pub struct TaskPluginSelectionAudit {
    pub selection_source: String,
    pub policy_revision: String,
    pub selected_at: String,
    pub project_context_revision: String,
    pub plugins: Vec<TaskSelectedPluginSnapshot>,
}

pub struct TaskSelectedPluginSnapshot {
    pub plugin_id: String,
    pub plugin_key: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub device_id: String,
    pub reason: Option<String>,
}
```

创建 Task 时顺序固定为：

```text
decode CreateTaskArgs
  -> 提取非可信 plugin_hints
  -> 生成普通 CreateTaskRequest
  -> TrustedTaskPluginSelector
  -> 可信 plugin_config_override
  -> Task Service 再次校验
  -> 保存 TaskRecord.plugin_config 和 audit snapshot
```

## 9. Run 前重新验证

Task 启动时重新解析项目设备，并确认：

1. 当前项目仍绑定同一 Local Connector 设备。
2. Plugin installation 仍为 active/installed/ready。
3. Release、version、artifact SHA-256 与可信快照一致。
4. dependency、permission、auth 状态仍为 satisfied。
5. 选择的组件仍存在于 immutable component snapshot。

失败时必须返回明确的 `task_plugin_unavailable` 类错误并阻止执行。不得静默移除、替换设备或回退到服务端。

验证通过后继续使用现有链路：

```rust
selected_plugins: task.plugin_config.selected_plugins.clone(),
plugin_command_invocations: task.plugin_config.command_invocations.clone(),
```

MCP Management 再次从 Project Management 获取 execution context，准备 Local Connector route，Local Connector Client 执行本地 MCP。

## 10. 旧逻辑删除

最终删除或重构：

- Chat stream `selected_plugin_ids`
- Chat stream `plugin_command_invocations`
- ChatOS `selected_plugins_for_runtime`
- ChatOS Conversation Runtime 中的 Plugin MCP/tool materialization
- `x-task-runner-selected-plugins`
- `x-task-runner-plugin-command-invocations`
- Conversation 级 Plugin localStorage selection
- 输入框 Plugin picker 直接授权 ChatOS Runtime 的行为

如果保留 UI picker，它只能表达“即将创建的 Task 的 Plugin 偏好”，最终仍转换成非可信 hints 并由 Task Runner 校验。

浏览器权限申请、Chrome Native Messaging、optional host permissions 和 Local Connector 本地审批流程必须保留。

## 11. 实施清单

- [x] Task Runner 根据项目 execution context 解析真实 Local Connector `device_id`
- [x] 插件目录、Task 创建和 Task Run 复用统一设备上下文 resolver
- [x] ChatOS Conversation Agent 不再获得 Plugin 工具
- [x] ChatOS 只注入 Task Plugin 描述 Prompt
- [x] `create_task` 和所有批量/子任务协议增加 `plugin_hints`
- [x] 实现 `TrustedTaskPluginSelector`
- [x] 将可信 `TaskPluginConfig` 和 audit snapshot 固化到 TaskRecord
- [x] Run 前重新验证设备、安装、Release、权限、认证和组件快照
- [x] 删除旧 Conversation 级选择传输和 Task Runner Header 覆盖入口
- [x] 为 `ChatosAsyncPlanner` 动态 Schema 注入当前设备可选的 Plugin key enum
- [x] 将输入框 Plugin picker 改为非可信 Task Plugin 偏好，只向 ChatOS Prompt 发送安全 `plugin_key`
- [x] 清理 picker 中不再参与执行授权的旧 Plugin Command 预选交互
- [x] 使用 Marketplace `open-computer-use` 完成真实安装、自动选择、`list_apps` 本地调用和结果回传验收

## 12. 验收标准

正向场景：

```text
用户：创建一个任务，列出本机所有应用
  -> ChatOS 看到 open-computer-use 的描述
  -> create_task.plugin_hints = [open-computer-use]
  -> TaskRecord.plugin_config 包含可信 Plugin ID
  -> Task Runner 启动
  -> MCP Management -> Local Connector -> Local MCP list_apps
  -> 结果写回 Task
```

负向场景必须 fail closed：

- 项目无 Local Connector Workspace
- 设备离线或 owner 不匹配
- Plugin 未安装或已禁用
- Release/version/hash 不匹配
- 权限、依赖或认证未满足
- ChatOS 伪造不存在的 `plugin_key`
- HTTP MCP 尝试由服务端直接执行
- ChatOS Conversation Agent 尝试直接调用 Plugin

## 13. 真实链路验收记录

验收日期：2026-08-21

Marketplace Plugin：

```text
Plugin ID: 69c1d99f-dcfc-47b3-80de-294e86a5a7f8
Plugin key: open-computer-use@chatos-marketplace
Release ID: d793e2fd-264f-4a5c-ad66-0c7f4ba4f1bc
Version: 0.3.1
Artifact SHA-256: 7574b7a35b642bcbe299a639ce01428c19555fb29ef730796db4f87f2b3291b5
```

Local Connector 与项目：

```text
Device ID: c27f6ed5-fa6f-4e08-9573-c09427501251
Platform: macos-arm64
Project ID: d634e8eb-95c1-4419-9186-5b4205ae2ed9
Workspace ID: c2110b9f-9eca-44b7-9316-ede32c64a69b
```

成功执行记录：

```text
Task ID: d38ddbfc-b0a4-4830-ac7d-03b73f3801b9
Run ID: 7b0a1463-1a54-4bf5-bd6c-c2dc3b7d639e
Task status: succeeded
Tool: plugin_open_computer_use_chatos_marketplace_computer_use_list_apps
Tool success: true
Returned applications: 24
```

Task 创建后保存的可信快照已确认包含内部 Plugin ID、Plugin key、Release ID、version、artifact SHA-256、device ID、policy revision、project context revision 和选择原因。`list_apps` 由 Local Connector Client 在本机执行，结果经 Local Connector Service、MCP Management 返回 Task Runner，并写入 Task completed event、`result_summary` 和 `process_log`。

真实验收期间发现并修复：

1. Plugin Release 的 `macos` 平台族与客户端 `macos-arm64` 精确匹配不兼容，现统一使用平台族匹配。
2. 单 Task 和 prerequisite DAG Task 在可信选择前未把 MCP request context 的项目 scope 写入创建请求，现统一补齐项目上下文。
3. MCP Management 批量 invocation ID 包含安全分隔符 `:`，客户端校验规则现允许该字符，并继续拒绝路径分隔符、换行等不安全字符。

最终验收调用链：

```text
ChatOS create_task(plugin_hints)
  -> Task Runner TrustedTaskPluginSelector
  -> TaskRecord trusted plugin config + audit snapshot
  -> MCP Management
  -> Local Connector Service Relay
  -> Local Connector Client
  -> local Open Computer Use MCP list_apps
  -> Task Runner result persistence
```
