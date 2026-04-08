# 内置任务 MCP 与任务平台深度改造实施方案

日期：2026-04-02

## 1. 目标

这次改造的目标不是小修小补，而是把 `agent_orchestrator` 联系人对话阶段、任务创建阶段、任务执行阶段三套职责彻底拆清楚，并让“任务平台”成为唯一任务真相源。

最终要达到的产品行为：

1. 在 `agent_orchestrator` 和联系人对话时，默认只允许 3 个内置 MCP 生效：
   - `查看`：即 `builtin_code_maintainer_read`
   - `任务`：新的“任务规划 MCP”，用于查看任务、创建任务、查看联系人已授权能力
   - `ui_prompter`
2. 联系人级别维护“允许该联系人未来任务使用哪些内置 MCP”，入口放到联系人列表，不再放在会话里。
3. 模型在联系人对话里创建任务时，必须同时写清楚：
   - 任务执行需要哪些内置 MCP
   - 任务执行需要哪些技能 / 插件 / commons
4. 定时执行任务时，不再复用联系人会话里的 MCP 选择，而是严格根据任务记录的这些属性来组装执行上下文。
5. 任务执行阶段只能拿到“执行任务所需”的工具，不能继续拿联系人对话阶段那套大而全的工具。
6. 任务成功或失败都必须返回结果。
7. 内置任务 MCP 直接以任务服务为数据来源，不再保留旧兼容路径。
8. 内置任务 MCP 不再暴露租户概念，由程序透传控制。

## 2. 现状诊断

结合当前代码，核心问题有 6 个：

### 2.1 联系人会话的 MCP 选择仍是 session 级临时状态

当前前端把 `mcp_enabled`、`enabled_mcp_ids` 放在 session runtime metadata 里：

- `agent_workspace/src/lib/store/helpers/sessionRuntime.ts`
- `agent_workspace/src/lib/store/actions/sendMessage/requestPayload.ts`
- `agent_workspace/src/features/contactSession/useContactSessionResolver.ts`

这会导致：

1. 联系人的“长期授权能力”与“当前会话临时勾选”混在一起。
2. 任务创建时无法稳定知道“这个联系人到底允许未来任务使用什么能力”。
3. 执行阶段容易错误继承某次聊天的会话态。

### 2.2 任务 MCP 现在是单体工具集合，不符合创建 / 执行分离

当前 `builtin_task_manager` 在：

- `agent_orchestrator/src/builtin/task_manager/mod.rs`

里面同时包含：

- `add_task`
- `list_tasks`
- `update_task`
- `complete_task`
- `delete_task`

这和目标模型不一致。联系人对话阶段只该做“查看任务、创建任务”；执行阶段只该做“获取当前任务、完成 / 失败任务”。

### 2.3 定时执行仍复用聊天运行时，而不是基于任务清单执行

当前任务执行器在：

- `agent_orchestrator/src/services/task_execution_runner.rs`

会构造一个 `ChatStreamRequest`，再调用：

- `resolve_chat_stream_context(...)`

这意味着执行时使用的 MCP、技能、命令上下文，本质上仍来源于联系人/会话运行时，而不是来源于任务本身。

### 2.4 任务模型太薄，无法承载执行清单

当前任务模型在：

- `contact_task_service/backend/src/models.rs`

只记录了标题、内容、状态、模型配置等基础信息，没有记录：

- 本任务计划使用的内置 MCP 列表
- 本任务计划使用的技能 / 插件 / commons
- 结果输出契约
- 创建时的能力快照

### 2.5 技能 / 命令已有“读完整内容”的能力，但未进入任务执行装配链路

当前已有完整内容读取能力：

- 技能全文：`agent_orchestrator/src/builtin/memory_skill_reader/mod.rs`
- 命令全文：`agent_orchestrator/src/builtin/memory_command_reader/mod.rs`
- 运行时技能/插件/命令摘要：`memory_server/backend/src/repositories/agents_runtime.rs`

说明基础能力已经有了，但现在任务创建并不会把这些“真正需要的内容”固化到任务元数据里，执行器也不会按任务所选资产注入全文。

### 2.6 联系人模型尚无“已授权内置 MCP”字段

当前联系人模型在：

- `memory_server/backend/src/models/sessions.rs`
- `memory_server/backend/src/repositories/contacts.rs`

只有 `agent_id`、`agent_name_snapshot` 等字段，没有联系人级 MCP 授权位。

## 3. 目标架构

本次改造建议把能力拆成 4 层：

### 3.1 联系人授权层

定义“该联系人未来允许任务使用的内置 MCP 白名单”。

这是联系人配置，不是会话配置。

### 3.2 联系人对话层

联系人聊天时，固定只开放 3 个 MCP：

1. `builtin_code_maintainer_read`
2. `builtin_task_planner`，替代当前单体 `builtin_task_manager`
3. `builtin_ui_prompter`

这里不再允许会话临时勾选其它内置 MCP。

### 3.3 任务规划层

模型在联系人对话阶段，通过任务规划 MCP：

1. 读取当前联系人已授权的内置 MCP
2. 读取当前联系人可用的技能 / 插件 / commons 摘要
3. 创建任务时，把“执行所需能力清单”一起写入任务

### 3.4 任务执行层

定时任务执行器只读取任务记录，按任务记录装配：

1. 执行所需内置 MCP
2. 执行所需技能 / 插件 / commons 全文
3. 历史消息总结
4. 当前任务内容
5. 专用任务执行 MCP

执行阶段不再读取会话 MCP 勾选状态。

## 4. 关键设计决策

## 4.1 对话阶段与执行阶段使用两套不同的任务 MCP

建议废弃当前“一个 `builtin_task_manager` 干所有事”的做法，拆成两套内置 MCP：

### A. `builtin_task_planner`

用途：仅用于联系人对话阶段。

建议暴露的工具：

1. `list_tasks`
2. `create_tasks`
3. `get_contact_builtin_mcp_grants`
4. `list_contact_runtime_assets`

说明：

- `create_tasks` 必须要求传入任务执行所需的 MCP 列表和上下文资产列表。
- `get_contact_builtin_mcp_grants` 返回当前联系人已授权的内置 MCP 列表。
- `list_contact_runtime_assets` 返回当前联系人可选的技能 / 插件 / commons 摘要，供模型挑选。

### B. `builtin_task_executor`

用途：仅用于任务定时执行阶段。

建议暴露的工具：

1. `get_current_task`
2. `complete_current_task`
3. `fail_current_task`

约束：

1. `complete_current_task` 必填 `result`
2. `fail_current_task` 必填 `result`
3. 同一个执行上下文内只能操作当前任务，不允许传任意 `task_id`

这样模型就不会在执行时乱碰其它任务。

## 4.2 联系人聊天固定 3 个 MCP，不再让 session 决定

建议对联系人聊天建立固定 MCP profile：

- `builtin_code_maintainer_read`
- `builtin_task_planner`
- `builtin_ui_prompter`

其中：

1. `read` 用于查看项目、文件、配置
2. `task_planner` 用于产出任务
3. `ui_prompter` 用于确认、补充、结构化收集

这意味着前端的会话内 `MCP 选择` 对联系人聊天要下线，至少不再影响联系人聊天可用工具。

当前涉及位置：

- `agent_workspace/src/components/inputArea/pickerWidgets/InputAreaMcpPicker.tsx`
- `agent_workspace/src/lib/store/helpers/sessionRuntime.ts`
- `agent_workspace/src/lib/store/actions/sendMessage/requestPayload.ts`
- `agent_orchestrator/src/api/chat_stream_common.rs`

## 4.3 联系人级 MCP 授权是“执行能力白名单”，不是聊天激活列表

联系人的授权 MCP 应表示：

“未来这个联系人创建的任务，最多可以调用哪些内置 MCP”

而不是：

“当前会话立刻可用哪些 MCP”

建议默认规则：

1. 联系人聊天始终只用固定 3 个 MCP
2. 任务执行时，模型只能从联系人已授权的内置 MCP 中挑选本任务需要的 MCP
3. 任务创建时，写入任务的 `planned_builtin_mcp_ids` 必须是该联系人授权集合的子集

## 4.4 “commons” 先映射为现有 runtime commands

当前代码库里没有独立的 commons 仓储模型，但已有：

- `runtime_commands`
- `memory_command_reader`

因此建议第一期把“commons”统一映射为“联系人运行时命令 / common markdown 项”。

也就是：

- `common` 的 canonical source 暂时就是 `runtime_commands`
- 存储时显式标注 `asset_type = common`
- 读取全文时走现有命令内容读取链路

这样可以先落地，不需要等独立 commons 系统。

## 5. 数据模型改造

## 5.1 联系人模型增加内置 MCP 授权字段

建议在 `memory_server/backend/src/models/sessions.rs` 的 `Contact` 上新增：

```rust
pub authorized_builtin_mcp_ids: Vec<String>
```

对应：

- `CreateContactRequest`
- 联系人 CRUD API
- 联系人 repository
- contacts 集合索引/默认值补齐

建议默认值：

```text
[
  "builtin_code_maintainer_read",
  "builtin_task_planner",
  "builtin_ui_prompter"
]
```

但联系人 UI 允许勾选更多“未来任务执行可用的内置 MCP”，如：

- `builtin_code_maintainer_write`
- `builtin_terminal_controller`
- `builtin_remote_connection_controller`
- 未来新增的其他内置 MCP

注意：

`builtin_task_executor` 不需要让用户勾选，它属于系统执行期固定工具。

## 5.2 任务模型增加执行清单字段

建议在 `contact_task_service/backend/src/models.rs` 的 `ContactTask` 上新增：

```rust
pub planned_builtin_mcp_ids: Vec<String>,
pub planned_context_assets: Vec<TaskContextAssetRef>,
pub execution_result_contract: Option<TaskExecutionResultContract>,
pub planning_snapshot: Option<TaskPlanningSnapshot>,
```

建议新增结构：

```rust
pub struct TaskContextAssetRef {
    pub asset_type: String,      // "skill" | "plugin" | "common"
    pub asset_id: String,        // skill_id / plugin_source / command_ref(or stable common id)
    pub display_name: Option<String>,
    pub source_type: Option<String>,
    pub source_path: Option<String>,
}

pub struct TaskExecutionResultContract {
    pub result_required: bool,
    pub preferred_format: Option<String>, // text | markdown | json
}

pub struct TaskPlanningSnapshot {
    pub contact_authorized_builtin_mcp_ids: Vec<String>,
    pub selected_model_config_id: Option<String>,
    pub planned_at: String,
}
```

创建任务接口 `CreateTaskRequest` 也同步新增这些字段。

## 5.3 建议额外增加“执行期解析快照”

为了避免执行时联系人能力集变化导致历史任务不可重现，建议任务记录里再补一个可选快照：

```rust
pub resolved_context_snapshot: Option<TaskResolvedContextSnapshot>
```

推荐策略：

1. 任务创建时先存“引用清单”即可
2. 任务开始执行时再解析成全文快照并回写任务

原因：

1. 创建时不必把任务文档写得过大
2. 执行时可保留“本次实际注入的上下文证据”
3. 后续查看执行过程更容易定位问题

## 6. API 与服务改造

## 6.1 Memory Server: 联系人授权 API

建议新增或扩展联系人接口：

1. `GET /contacts/:id/builtin-mcp-grants`
2. `PUT /contacts/:id/builtin-mcp-grants`

返回格式建议：

```json
{
  "contact_id": "xxx",
  "authorized_builtin_mcp_ids": [
    "builtin_code_maintainer_read",
    "builtin_code_maintainer_write",
    "builtin_terminal_controller"
  ]
}
```

数据源：

- `memory_server/backend/src/api/contacts_crud_api.rs`
- `memory_server/backend/src/repositories/contacts.rs`

## 6.2 Task Service: 任务创建接口增强

增强：

- `POST /tasks`

新增入参：

1. `planned_builtin_mcp_ids`
2. `planned_context_assets`
3. `execution_result_contract`
4. `planning_snapshot`

校验规则：

1. `planned_builtin_mcp_ids` 不能为空
2. 其中每一项都必须属于联系人已授权 MCP
3. `planned_context_assets` 可以为空，但如果任务描述明确依赖技能/插件/commons，模型必须填入
4. 结果契约默认 `result_required = true`

## 6.3 Chat App Server: 联系人聊天上下文装配

改造：

- `agent_orchestrator/src/api/chat_stream_common.rs`
- `agent_orchestrator/src/core/mcp_runtime.rs`
- `agent_orchestrator/src/services/mcp_loader.rs`

目标行为：

1. 当检测到当前 session 是联系人聊天时，不再使用 session metadata 中的 `enabled_mcp_ids` 作为真实工具来源。
2. 改为固定加载 3 个 MCP：
   - `builtin_code_maintainer_read`
   - `builtin_task_planner`
   - `builtin_ui_prompter`
3. 同时保留：
   - skill reader
   - plugin reader
   - command reader

说明：

`memory_skill_reader` / `memory_plugin_reader` / `memory_command_reader` 更适合作为“联系人上下文读取器”，可保留自动挂载。

## 6.4 Chat App Server: 新任务规划 MCP

建议新增目录：

```text
agent_orchestrator/src/builtin/task_planner/
agent_orchestrator/src/builtin/task_executor/
```

并删除旧 `task_manager` 兼容逻辑，不再继续扩展它。

`task_planner` 负责：

1. 查询当前联系人已授权的 builtin MCP
2. 查询当前联系人运行时资产摘要
3. 发起任务创建 review
4. 落库任务到 task service

`task_executor` 负责：

1. 暴露当前任务
2. 完成任务
3. 失败任务

## 6.5 Task Runner: 改成按任务清单执行

重点改造：

- `agent_orchestrator/src/services/task_execution_runner.rs`

改造后执行流程：

1. 读取待执行任务
2. 读取联系人运行时上下文
3. 校验任务记录的 `planned_builtin_mcp_ids` 是否仍在联系人授权集合内
4. 解析 `planned_context_assets`
5. 将选中的技能 / 插件 / commons 全文装配到执行上下文
6. 只加载：
   - 任务记录要求的 builtin MCP
   - 系统固定 `builtin_task_executor`
7. 执行完成后必须调用：
   - `complete_current_task(result)`
   - 或 `fail_current_task(result)`

这样执行器不再依赖会话态，也不会误带上联系人聊天阶段才有的工具。

## 7. 联系人聊天阶段的上下文协议

建议给联系人聊天增加一条更明确的系统规则：

1. 你当前只能查看、规划任务、与用户交互确认
2. 你不能直接代替未来任务去执行写入、终端、远程等操作
3. 如果任务未来需要这些能力，必须在创建任务时把所需能力写入任务属性

这样模型会更稳，不会在联系人对话时误以为自己可以立刻开始执行高权限动作。

## 8. 任务创建协议

建议 `create_tasks` 工具的输入结构升级为：

```json
{
  "tasks": [
    {
      "title": "修复某模块启动失败",
      "details": "检查日志并修复启动报错",
      "priority": "high",
      "planned_builtin_mcp_ids": [
        "builtin_code_maintainer_read",
        "builtin_code_maintainer_write",
        "builtin_terminal_controller"
      ],
      "planned_context_assets": [
        {
          "asset_type": "skill",
          "asset_id": "skill_xxx"
        },
        {
          "asset_type": "plugin",
          "asset_id": "frontend_toolkit"
        },
        {
          "asset_type": "common",
          "asset_id": "CMD2"
        }
      ],
      "execution_result_contract": {
        "result_required": true,
        "preferred_format": "markdown"
      }
    }
  ]
}
```

校验逻辑：

1. `planned_builtin_mcp_ids` 必须为联系人授权集合子集
2. `planned_context_assets` 必须能在当前联系人 runtime context 中找到
3. 若模型遗漏结果契约，系统自动补 `result_required = true`

## 9. 执行上下文装配规则

建议执行上下文由以下内容组成，顺序固定：

1. 系统基础提示词
2. 联系人角色定义
3. 历史消息总结
4. 当前任务正文
5. 当前任务结果契约
6. 任务选中的技能全文
7. 任务选中的插件全文
8. 任务选中的 commons 全文
9. 专用执行 MCP 工具说明

### 9.1 技能

来源：

- `skill_ids`
- `memory_skill_reader`
- `memory_server_client::get_memory_skill(...)`

执行期注入全文，而不是只给摘要。

### 9.2 插件

来源：

- `plugin_sources`
- `runtime_plugins`
- `memory_plugin_reader`

执行期建议注入插件主文档全文，必要时可增加“只注入被任务选中的插件”策略。

### 9.3 Commons

第一期直接来自：

- `runtime_commands`
- `memory_command_reader`

执行期注入选中的 command/common markdown 全文。

### 9.4 历史消息

继续保持当前方式，不改：

- `memory_server_client::compose_context(session_id, 2)`

即你要求的“之前总结相关的获取历史消息方式保持不变”。

## 10. 前端改造

## 10.1 联系人列表增加“内置 MCP 授权”按钮

建议在联系人列表项新增一个按钮，例如：

- `能力`
- 或 `MCP`

目前联系人列表渲染位置在：

- `agent_workspace/src/components/sessionList/sections/SessionSection.tsx`

点击后弹出联系人授权弹窗，展示所有内置 MCP，仅内置，不展示外部 MCP。

弹窗能力：

1. 展示内置 MCP 名称与说明
2. 勾选 / 取消勾选
3. 保存联系人级授权

默认不可取消的项：

- `builtin_code_maintainer_read`
- `builtin_task_planner`
- `builtin_ui_prompter`

因为它们属于联系人对话基础能力。

## 10.2 联系人聊天输入区下线会话级 MCP 选择

当前：

- `agent_workspace/src/components/inputArea/pickerWidgets/InputAreaMcpPicker.tsx`

建议对联系人聊天模式：

1. 隐藏 MCP 开关和 MCP 选择
2. 或只展示只读提示：“联系人聊天固定使用查看/任务/UI”

对非联系人场景可保留现有通用 MCP picker。

## 10.3 任务详情展示新增“执行清单”

任务详情 UI 建议增加只读区块：

1. 本任务计划使用的 MCP
2. 本任务计划使用的技能
3. 本任务计划使用的插件
4. 本任务计划使用的 commons
5. 本次执行实际解析快照

这样排查“为什么任务执行成这样”会容易很多。

## 11. 状态机统一

建议继续统一使用任务服务状态，不再在内置 MCP 内做二次映射。

状态含义：

1. `pending_confirm`
   - 刚创建，待用户确认
2. `pending_execute`
   - 用户已确认，等待调度执行
3. `running`
   - 正在执行
4. `completed`
   - 执行成功，必须有结果
5. `failed`
   - 执行失败，必须有结果
6. `cancelled`
   - 用户取消

其中“失败也必须有结果”建议定义为：

- `result_summary` 必填
- `last_error` 可选但推荐

也可以在完成/失败工具里统一使用一个 `result` 字段，再由服务端拆分：

- 成功：写 `result_summary`
- 失败：写 `result_summary + last_error`

## 12. 迁移方案

建议分 5 个阶段推进，避免一次改太猛。

### Phase 1：数据结构与接口打底

1. 给联系人加 `authorized_builtin_mcp_ids`
2. 给任务加 `planned_builtin_mcp_ids` / `planned_context_assets`
3. 打通联系人授权 API
4. 打通增强版任务创建 API

这一阶段先不切 UI，不切执行器。

### Phase 2：联系人前端改造

1. 联系人列表增加授权按钮和弹窗
2. 联系人聊天隐藏会话级 MCP picker
3. 联系人会话改成固定 MCP profile

### Phase 3：内置任务 MCP 拆分

1. 新增 `task_planner`
2. 新增 `task_executor`
3. 联系人聊天改接 `task_planner`
4. 停止在联系人聊天里挂旧 `task_manager`

### Phase 4：执行器切换到任务清单驱动

1. 按任务记录解析 MCP/技能/插件/commons
2. 注入全文上下文
3. 只挂 `task_executor`
4. 跑通成功 / 失败回写

### Phase 5：清理旧逻辑

1. 删除联系人聊天对 session `enabled_mcp_ids` 的依赖
2. 删除旧 `builtin_task_manager` 路径
3. 删除不再需要的兼容 DTO / 字段 / UI

## 13. 测试方案

必须覆盖以下测试：

### 后端单测 / 集成测试

1. 联系人授权集合保存与读取
2. `create_tasks` 校验未授权 MCP 时失败
3. `create_tasks` 校验不存在的技能/插件/common 时失败
4. 执行器只加载任务记录中的 MCP
5. 执行器注入技能/插件/common 全文
6. `complete_current_task` / `fail_current_task` 必须带结果
7. 历史总结上下文仍保持存在

### 前端交互测试

1. 联系人列表打开授权弹窗
2. 勾选保存后重新进入仍能回显
3. 联系人聊天不再显示会话级 MCP 勾选
4. 任务详情能看到执行清单

### 回归测试

1. 普通非联系人聊天不受影响
2. 项目场景现有 MCP picker 不受影响
3. 任务确认后能进入 `pending_execute`
4. 定时任务执行后状态能正确结束，不会一直卡在 `running`

## 14. 推荐的代码落点

### Memory Server

- `memory_server/backend/src/models/sessions.rs`
- `memory_server/backend/src/repositories/contacts.rs`
- `memory_server/backend/src/api/contacts_crud_api.rs`

### Task Service

- `contact_task_service/backend/src/models.rs`
- `contact_task_service/backend/src/repository.rs`
- `contact_task_service/backend/src/api.rs`

### Chat App Server

- `agent_orchestrator/src/builtin/task_planner/`
- `agent_orchestrator/src/builtin/task_executor/`
- `agent_orchestrator/src/api/chat_stream_common.rs`
- `agent_orchestrator/src/services/task_execution_runner.rs`
- `agent_orchestrator/src/services/task_service_client.rs`
- `agent_orchestrator/src/services/builtin_mcp.rs`
- `agent_orchestrator/src/core/mcp_runtime.rs`
- `agent_orchestrator/src/services/mcp_loader.rs`

### Chat App Frontend

- `agent_workspace/src/components/sessionList/sections/SessionSection.tsx`
- `agent_workspace/src/components/inputArea/pickerWidgets/InputAreaMcpPicker.tsx`
- 联系人授权弹窗新组件
- 任务详情展示组件

## 15. 我的建议

这次不要在旧 `builtin_task_manager` 上继续打补丁，直接按“规划 MCP + 执行 MCP”重做。

原因很明确：

1. 现在的问题不是某个字段漏了，而是职责边界本身错了。
2. 联系人聊天和任务执行是两种完全不同的工具模型。
3. 如果继续兼容旧 `task_manager`，后面状态、权限、上下文装配还会反复出问题。

## 16. 仍需你拍板的两个点

### 16.1 commons 的最终命名

我建议第一期直接把现有 `runtime_commands` 作为 `commons` 落地，这样最稳。

如果你后面要把 commons 单独产品化，再把 `asset_type=common` 的来源切换掉即可，任务模型不用再改。

### 16.2 任务是否要存“创建时快照”还是“执行时快照”

我的建议：

1. 创建时存“引用清单”
2. 执行时存“解析快照”

这是当前成本和可追溯性最平衡的方案。

---

如果按这个方案推进，建议先做 `Phase 1 + Phase 2`，先把“联系人授权”和“任务记录执行清单”立起来，再切 MCP 拆分与执行器，不然容易边跑边乱。
