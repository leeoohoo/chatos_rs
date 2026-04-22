# 内置 MCP 动态 Prompt 拼装方案

## 目标
解决两个问题：

1. 现在模型虽然能拿到内置 MCP 工具，但系统提示对这些工具的“何时该用、怎么优先路由”指导太弱，导致 `task_manager`、`ui_prompter`、文件读取等高价值工具使用不足。
2. 当用户没有勾选某个内置 MCP 时，对应工具 guidance 也应该从系统提示里移除，避免模型围绕一个本轮不可用的能力做计划。

本方案的核心目标是：

1. 把内置 MCP prompt 改成 section 化源文件。
2. 后端按本轮实际启用的内置 MCP 动态拼装 prompt。
3. 后续再升级成“按实际注册成功的工具”做更细粒度裁剪。

## 当前现状

### 1. 内置 MCP 的启用与选择
前端和会话元数据已经有内置 MCP 选择能力：

- `chat_app/src/lib/store/helpers/sessionRuntime.ts`
- `chat_app/src/lib/store/actions/mcp.ts`

后端会在这里读取 `enabled_mcp_ids` / `mcp_enabled` 并按选择加载：

- `chat_app_server_rs/src/api/chat_stream_common.rs`
- `chat_app_server_rs/src/core/mcp_runtime.rs`
- `chat_app_server_rs/src/services/mcp_loader.rs`

### 2. 当前真正注入模型的 prompt 很窄
当前 chat runtime 里真正和 MCP 有关的 system prompt 主要只有一段：

- `chat_app_server_rs/src/api/chat_stream_common.rs`
- `compose_tool_routing_system_prompt(...)`

这段只覆盖了浏览器 / Web 路由偏好，没有覆盖：

- 任务管理为什么要主动用
- 缺信息时为什么应该走 UI 交互
- 本地代码读取和写入的优先顺序
- 本地终端和远程连接的边界
- Notepad 的沉淀时机

### 3. 工具真实暴露名是“server_name + tool_name”
工具在注册到模型前，会被统一加前缀：

- `chat_app_server_rs/src/services/v2/mcp_tool_execute.rs`
- `chat_app_server_rs/src/services/v3/mcp_tool_execute.rs`

例如：

- `task_manager_add_task`
- `ui_prompter_prompt_choices`
- `code_maintainer_read_read_file`
- `code_maintainer_write_patch`

所以 prompt 里必须尽量写模型真正看得到的名字，而不是只写抽象概念。

### 4. 不是所有 builtin 都属于“勾选型 MCP”
需要分两类看：

1. 勾选型 builtin MCP
   这类会直接受 `enabled_mcp_ids` 控制，例如：
   - `builtin_code_maintainer_read`
   - `builtin_code_maintainer_write`
   - `builtin_terminal_controller`
   - `builtin_task_manager`
   - `builtin_notepad`
   - `builtin_ui_prompter`
   - `builtin_remote_connection_controller`
   - `builtin_web_tools`
   - `builtin_browser_tools`

2. 条件附带型 builtin tools
   这类不是用户在 MCP 面板里直接勾选出来的，而是由聊天上下文附带注入，例如 contact agent memory readers：
   - `memory_skill_reader`
   - `memory_command_reader`
   - `memory_plugin_reader`

另一个特殊点：

- `BuiltinMcpKind::AgentBuilder` 虽然存在，但当前常规加载路径里在 `chat_app_server_rs/src/services/mcp_loader.rs` 被跳过了。

所以第一版动态拼装，不建议把 `agent_builder` 当作“用户点击型内置 MCP section”来做。

## 推荐设计

### 一、把 prompt 源改成 section 化
已经在根目录新增：

- `BUILTIN_MCP_PROMPT.md`

这个文件的结构是：

- `## [global]`
- `## [builtin_task_manager]`
- `## [builtin_ui_prompter]`
- `## [builtin_code_maintainer_read]`
- `## [builtin_code_maintainer_write]`
- `## [builtin_terminal_controller]`
- `## [builtin_remote_connection_controller]`
- `## [builtin_browser_tools]`
- `## [builtin_web_tools]`
- `## [builtin_notepad]`
- `## [conditional_contact_memory_readers]`

设计要点：

1. `global` 永远拼进去。
2. 每个内置 MCP 对应一个独立 section。
3. section 里尽量写模型实际可见的 prefixed tool names。
4. 后续后端可以只拼接“被选中的 section”。

### 二、后端新增一个“按 builtin servers 选 section”的 builder
推荐新增模块：

- `chat_app_server_rs/src/core/builtin_mcp_prompt.rs`

建议提供这类函数：

```rust
pub fn compose_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
) -> Option<String>
```

这个函数做三件事：

1. 永远加入 `global`
2. 按 `BuiltinMcpKind` 选择 section
3. 去重并按固定顺序拼装

建议固定顺序：

1. `global`
2. `builtin_task_manager`
3. `builtin_ui_prompter`
4. `builtin_code_maintainer_read`
5. `builtin_code_maintainer_write`
6. `builtin_terminal_controller`
7. `builtin_remote_connection_controller`
8. `builtin_browser_tools`
9. `builtin_web_tools`
10. `builtin_notepad`
11. `conditional_contact_memory_readers`

这样顺序更符合模型的决策流程：先任务与交互，再文件与执行，再浏览器 / Web，再沉淀。

### 三、section 与 builtin kind / mcp id 的映射

#### 勾选型 builtin MCP
- `builtin_task_manager` -> `[builtin_task_manager]`
- `builtin_ui_prompter` -> `[builtin_ui_prompter]`
- `builtin_code_maintainer_read` -> `[builtin_code_maintainer_read]`
- `builtin_code_maintainer_write` -> `[builtin_code_maintainer_write]`
- `builtin_terminal_controller` -> `[builtin_terminal_controller]`
- `builtin_remote_connection_controller` -> `[builtin_remote_connection_controller]`
- `builtin_browser_tools` -> `[builtin_browser_tools]`
- `builtin_web_tools` -> `[builtin_web_tools]`
- `builtin_notepad` -> `[builtin_notepad]`

#### 条件附带型 builtin servers
- `BuiltinMcpKind::MemorySkillReader` -> `[conditional_contact_memory_readers]`
- `BuiltinMcpKind::MemoryCommandReader` -> `[conditional_contact_memory_readers]`
- `BuiltinMcpKind::MemoryPluginReader` -> `[conditional_contact_memory_readers]`

#### 暂不纳入第一版
- `BuiltinMcpKind::AgentBuilder`

原因：

1. 当前用户勾选链路不会把它当普通 builtin MCP 暴露给聊天工具面。
2. 第一版先聚焦你已经明确提出的高频工具：任务、交互、文件、终端、远程、浏览器、Web、笔记。

## 接入位置建议

### 方案 A：低风险、最小改动
保持现有 runtime 字段名，先把“tool routing prompt”升级为“完整 builtin MCP prompt”。

也就是保留：

- `ResolvedChatStreamContext.tool_routing_system_prompt`

但把其构建逻辑从：

- 只拼浏览器 / Web routing

变成：

- 拼完整 builtin MCP sections

这样改动面最小，因为下面这些位置暂时都不用大改字段名：

- `chat_app_server_rs/src/api/chat_stream_common.rs`
- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`
- `chat_app_server_rs/src/core/turn_runtime_snapshot.rs`

缺点是名字会暂时不够准确。

### 方案 B：更干净、但改动面更大
新增一个更准确的字段：

```rust
pub builtin_mcp_system_prompt: Option<String>
```

并在以下地方逐步替换：

- `ResolvedChatStreamContext`
- `build_prefixed_messages(...)`
- `build_prefixed_input_items(...)`
- turn runtime snapshot
- v2 / v3 chat flow

我更推荐最终走这个方向，但第一步落地可以先按方案 A 做，先把能力补起来。

## 推荐的第一版实现步骤

### 第 1 步：新增 prompt builder 模块
新增：

- `chat_app_server_rs/src/core/builtin_mcp_prompt.rs`

模块职责：

1. 读取 `BUILTIN_MCP_PROMPT.md`
2. 解析 `## [section_id]` section
3. 暴露 `compose_builtin_mcp_system_prompt(...)`

解析方式建议保持简单：

1. 启动时或首次调用时，把文件按行扫描。
2. 遇到 `## [section_id]` 视为新 section。
3. 把后续内容收集到下一个 section 开始。
4. 存成 `HashMap<String, String>`。

这样后续改 prompt 文案时，后端不需要再改 Rust 代码。

### 第 2 步：在 `resolve_chat_stream_context(...)` 里按当前 builtin servers 生成 prompt
接入点：

- `chat_app_server_rs/src/api/chat_stream_common.rs`

现状是先得到：

```rust
let (http_servers, stdio_servers, mut builtin_servers) = ...
```

然后当前只会做：

```rust
let tool_routing_system_prompt =
    compose_tool_routing_system_prompt(builtin_servers.as_slice());
```

建议改为：

```rust
let tool_routing_system_prompt =
    compose_builtin_mcp_system_prompt(builtin_servers.as_slice());
```

或者第一版保留两个函数，但最终只保留新的总 builder。

### 第 3 步：保持前端不变，直接复用 `enabled_mcp_ids`
这一点是这个方案最省的地方：

1. 前端“用户点选哪些 builtin MCP”本来就会进 `enabled_mcp_ids`
2. 后端本来就会用 `enabled_mcp_ids` 去构建 `builtin_servers`
3. prompt builder 只需要基于 `builtin_servers` 决定 section 是否拼进来

所以第一版不需要新增前端协议字段。

换句话说，用户勾选行为已经天然可以驱动 prompt 动态裁剪。

### 第 4 步：继续沿用 prefixed system messages / input items 的注入方式
现有注入方式已经成熟：

- v2: `build_prefixed_messages(...)`
- v3: `build_prefixed_input_items(...)`

所以第一版不建议改消息注入形态，只改 prompt 内容来源。

## 为什么这个方案能满足“没勾选文件读取，就把对应 prompt 去掉”
因为第一版的裁剪源不是“静态总 prompt”，而是本轮实际进入 runtime 的 `builtin_servers`。

例如：

1. 用户只勾选了 `builtin_task_manager` 和 `builtin_ui_prompter`
2. 后端 `load_mcp_servers_by_selection(...)` 得到的 builtin servers 里只有这两个 kind
3. `compose_builtin_mcp_system_prompt(...)` 就只拼：
   - `global`
   - `builtin_task_manager`
   - `builtin_ui_prompter`
4. 文件读取、文件写入、终端、浏览器、Web、远程、笔记这些 section 都不会进上下文

因此模型不会再围绕未勾选能力做计划。

## 第二阶段增强：按“实际可用工具”再裁一层
第一版按 `enabled_mcp_ids` 已经能解决大部分问题，但还存在一个边界：

1. 某个 builtin MCP 可能被勾选了
2. 但运行时因为环境原因，工具实际 unavailable

典型例子：

- `browser_tools` 缺少 browser backend
- `remote_connection_controller` 缺少 user context / default remote connection

而当前流程里，真正的 unavailable 信息是在下面这一步之后才知道的：

- `mcp_exec.init().await`

对应代码：

- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`

所以第二阶段可以做：

1. 先初始化 `mcp_exec`
2. 根据 `mcp_tool_metadata` 和 `get_unavailable_tools()` 再判断 section 是否应该保留
3. 如果某个 builtin server 下所有工具都 unavailable，就从 prompt 里整段去掉
4. 如果只有部分 unavailable，可以保留 section，但追加一条简短限制说明

这一步更精细，但第一版不是必须。

## 兼容 contact memory readers 的建议
这类 reader 不是用户通过 MCP 面板勾选出来的，而是：

- `contact_agent_skill_reader_server(...)`
- `contact_agent_command_reader_server(...)`
- `contact_agent_plugin_reader_server(...)`

在 `chat_app_server_rs/src/api/chat_stream_common.rs` 里被条件附带注入。

所以 builder 不能只看 `enabled_mcp_ids`，必须以“最终进入 runtime 的 builtin servers 列表”为准。

这是为什么我建议 builder 的输入是：

```rust
builtin_servers: &[McpBuiltinServer]
```

而不是：

```rust
enabled_mcp_ids: &[String]
```

## 建议新增的测试

### Prompt 选择测试
在 `chat_app_server_rs/src/api/chat_stream_common.rs` 或新模块测试里补：

1. 只有 `TaskManager` 时，prompt 包含任务 section，不包含文件 section
2. 只有 `UiPrompter` 时，prompt 包含交互 section
3. 同时有 `BrowserTools` + `WebTools` 时，prompt 同时包含两段
4. 没有任何 builtin servers 时，返回 `None`
5. 有 memory readers 时，包含 `conditional_contact_memory_readers`

### 稳定顺序测试
保证 section 输出顺序固定，避免 prompt 漂移导致模型行为不稳定。

### AgentBuilder 排除测试
显式测试第一版不会把 `AgentBuilder` section 拼进默认聊天 prompt。

## 推荐落地顺序

1. 先上线第一版 section 化 prompt builder，只按 `builtin_servers` 裁剪
2. 观察模型是否明显更常用：
   - `task_manager`
   - `ui_prompter`
   - `code_maintainer_read`
3. 再决定是否做第二阶段“按实际 available tools 再裁一层”
4. 最后再考虑把字段名从 `tool_routing_system_prompt` 正式升级成 `builtin_mcp_system_prompt`

## 一句话结论
最省改动、最符合你们现有架构的做法是：

1. 用根目录 `BUILTIN_MCP_PROMPT.md` 作为 section 化 prompt 源
2. 后端按本轮 `builtin_servers` 动态挑 section
3. 继续用现有 prefixed system prompt 注入链路送给模型

这样用户勾没勾某个内置 MCP，就会直接体现在本轮上下文里，模型也就不会再围绕未勾选工具做计划了。
