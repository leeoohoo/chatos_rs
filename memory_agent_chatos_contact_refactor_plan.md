# Chatos 大改造整改方案（Memory Agent + 联系人会话 + Agent Builder MCP）

## 1. 目标范围（按你最新要求合并）

本次是一次整体系重构，目标统一为：

1. 下线 `sub-agent-router` 的“运行子代理”能力，不再使用 `run_sub_agent`。
2. 将其改造成“只负责生成 Agent”的内置 MCP（下文称 `agent_builder`）。
3. Skill 安装/管理迁到 `memory_server`，Memory 成为 Agent + Skill 唯一真源。
4. Chatos 不再做本地 Agent 创建/管理，改为使用 Memory 的 Agent。
5. Chatos 会话体验改成“添加联系人”：
   - 联系人来源：Memory Agent。
   - 会话和联系人绑定，不再在输入区选 Agent。
6. 输入区只保留模型选择，并新增：
   - MCP 总开关（默认开启，默认可用全部内置 MCP）；
   - 项目选择（发送时透传 `project_root`）。
7. 对话时自动把 Memory Agent 的角色定义 + skills 注入上下文。

---

## 2. 当前代码现状（已核对）

## 2.1 前端发送链路仍是“模型/本地 Agent 二选一”

- `chat_app/src/lib/store/actions/sendMessage.ts`
  - 强制要求 `selectedModelId` 或 `selectedAgentId` 二选一；
  - 选 Agent 走 `client.streamAgentChat`；
  - 选模型走 `client.streamChat`；
  - 请求里没有 `project_root`、没有 MCP 开关参数。
- `chat_app/src/lib/api/client/stream.ts`
  - `streamChat/streamAgentChat` 只传 `session_id/content/model|agent_id/...`；
  - 无 `project_root`、无 `mcp_enabled`、无 `enabled_mcp_ids`。

## 2.2 输入区仍包含 Agent 选择，项目选择只用于“项目文件附件”

- `chat_app/src/components/InputArea.tsx`
  - 顶部 AI picker 同时展示 Agent 和 Model；
  - 发送前校验“必须先选模型或智能体”；
  - 没有“项目选择器”用于发送透传；
  - 项目相关逻辑目前仅服务于“Agent 项目文件”附加。

## 2.3 侧边栏仍是 Session 语义，不是联系人语义

- `chat_app/src/components/SessionList.tsx`
- `chat_app/src/components/sessionList/Sections.tsx`
  - 目前是 `SESSIONS / PROJECTS / TERMINALS / REMOTE` 四段；
  - Session 顶栏有“新建会话”，不是“添加联系人”。

## 2.4 本地 Agent 管理仍存在

- 前端：
  - `chat_app/src/components/AgentManager.tsx`
  - `chat_app/src/components/chatInterface/HeaderBar.tsx`（用户菜单中有“智能体管理”）
- 后端：
  - `chat_app_server_rs/src/api/agents.rs`
  - `chat_app_server_rs/src/api/agents_v3.rs`
  - `chat_app_server_rs/src/core/agent_runtime.rs`
  - `chat_app_server_rs/src/repositories/agents.rs`

## 2.5 Sub-Agent Router 仍是运行型内置 MCP

- 常量与内置注册：
  - `chat_app_server_rs/src/services/builtin_mcp.rs`
  - `chat_app_server_rs/src/core/mcp_tools.rs`
- Tool 集：`run_sub_agent` / `cancel_sub_agent_job`
  - `chat_app_server_rs/src/builtin/sub_agent_router/mod.rs`
- 前端仍有运行结果 UI：
  - `chat_app/src/components/ToolCallRenderer.tsx`
  - `chat_app/src/components/RunSubAgentModal.tsx`
  - `chat_app/src/components/SubAgentRunPanel.tsx`

## 2.6 Session 元数据与项目透传有结构性缺口

- `createChatStoreWithBackend.ts` 里 `getSessionParams().projectId` 固定为空字符串，当前 project 不参与 session 查询/创建过滤。
- `chat_app_server_rs/src/api/sessions.rs` 的 `create/update` 目前忽略 metadata；无法可靠持久化前端会话元数据。
- `memory_server` 的 `Session` 模型当前无 metadata 字段，导致跨服务也无法沉淀联系人绑定、MCP策略、项目透传配置。

---

## 3. 目标架构（改造后）

## 3.1 设计面（Agent Design Plane）

- 内置 MCP 从 `sub_agent_router` 升级为 `agent_builder`。
- 只保留“智能体生成与维护”能力：
  - 根据用户需求推荐 agent 画像；
  - 选择/组合 skills；
  - 创建/更新 Memory Agent。
- 移除所有运行子代理能力。

## 3.2 控制面（Memory Control Plane）

- `memory_server` 统一托管：
  - skills、skill 插件、agents、agent 版本；
  - agent runtime context（角色定义 + skills 摘要/正文）。

## 3.3 运行面（Chat Runtime Plane）

- Chatos 会话由“联系人（Memory Agent）”驱动。
- 输入区只选模型；MCP 与项目在输入区显式可控。
- 每次发送时，后端构建最终上下文：
  - 全局系统提示
  - + 联系人 Agent 角色定义
  - + 联系人 Agent skills
  - + MCP/项目约束策略

---

## 4. 数据模型调整

## 4.1 Memory Server（新增）

建议新增集合：

1. `memory_skills`
2. `memory_skill_plugins`
3. `memory_agents`
4. `memory_agent_versions`
5. `memory_agent_build_jobs`

`memory_agents` 核心字段建议：

- `id, user_id, name, description, role_definition, skill_ids, default_skill_ids`
- `mcp_policy`（默认 MCP 策略，可被会话层覆盖）
- `project_policy`（可选的默认项目限制）
- `status, created_at, updated_at`

## 4.2 Memory Session（扩展）

为满足“联系人会话 + 输入区运行参数”需要，`memory_server` 的 session 需要增加 `metadata`（JSON）。

建议约定：

```json
{
  "contact": {
    "type": "memory_agent",
    "agent_id": "xxx",
    "agent_version": 3
  },
  "chat_runtime": {
    "selected_model_id": "xxx",
    "mcp_enabled": true,
    "enabled_mcp_ids": [],
    "project_id": "xxx",
    "project_root": "/abs/path"
  }
}
```

说明：

- `enabled_mcp_ids=[]` + `mcp_enabled=true` 表示“默认全部内置可用”；
- `mcp_enabled=false` 表示关闭 MCP；
- `project_root` 为发送时透传字段，也可由 `project_id` 服务端反解后写回。

## 4.3 Chat Backend Session 映射（同步）

- `chat_app_server_rs/src/models/session.rs` 已有 `metadata` 字段，可继续复用；
- 但 `api/sessions.rs` 需真正传递 metadata 到 memory；
- `memory_server_client.rs` 的 `MemorySession` / `CreateSessionRequest` / `PatchSessionRequest` 需加入 metadata 映射。

---

## 5. API 设计与改造

## 5.1 Memory API（新增）

新增/扩展：

1. `GET /api/memory/v1/agents`
2. `POST /api/memory/v1/agents`
3. `PATCH /api/memory/v1/agents/:agent_id`
4. `DELETE /api/memory/v1/agents/:agent_id`
5. `POST /api/memory/v1/agents/ai-create`
6. `GET /api/memory/v1/agents/:agent_id/runtime-context`
7. `GET /api/memory/v1/skills`
8. `POST /api/memory/v1/skills/import-git`
9. `POST /api/memory/v1/skills/plugins/install`
10. `POST /api/memory/v1/sessions` / `PATCH /sessions/:id` 支持 metadata。

## 5.2 Chat Backend BFF API（新增）

为前端简化调用，建议在 chat backend 提供代理层：

1. `GET /api/memory-agents`
2. `POST /api/memory-agents`
3. `POST /api/agent-builder/ai-create`
4. `GET /api/memory-agents/:agent_id/runtime-context`

## 5.3 流式聊天请求扩展（关键）

扩展 `chat_v2/chat_v3` 与 `agents_v3` 请求结构，新增字段：

- `contact_agent_id`（联系人 agent id）
- `project_id`
- `project_root`
- `mcp_enabled`（bool）
- `enabled_mcp_ids`（可选数组）

对应前端改造：

- `chat_app/src/lib/api/client/stream.ts` 请求体新增上述字段；
- `sendMessage.ts` 组装并传递。

---

## 6. Chatos 前端整改方案

## 6.1 侧边栏从 Session 改为 Contact

目标交互：

1. 顶部按钮从“新建会话”改为“添加联系人”。
2. 添加流程：
   - 弹出联系人选择框；
   - 数据源是 Memory Agents；
   - 选择后创建（或打开）绑定该 agent 的会话。
3. 列表展示为联系人（本质仍可映射 session_id）。

主要改动点：

- `chat_app/src/components/SessionList.tsx`
- `chat_app/src/components/sessionList/Sections.tsx`
- 新增 `CreateContactModal`（或复用当前资源创建弹窗模式）。

## 6.2 输入区只保留模型选择

目标交互：

1. 移除 Agent 选择项，只显示模型选择。
2. 新增 MCP 开关，默认 `ON`。
3. 新增项目选择器（来自 `projects`），用于发送透传 `project_root`。

主要改动点：

- `chat_app/src/components/InputArea.tsx`
- `chat_app/src/components/chatInterface/ChatComposerPanel.tsx`
- `chat_app/src/components/ChatInterface.tsx`

## 6.3 Store 状态调整

建议新增状态：

- `contactAgentIdBySession: Record<sessionId, agentId>`
- `sessionRuntimeBySession: { selectedModelId, mcpEnabled, enabledMcpIds, projectId, projectRoot }`

建议删除/下线：

- `selectedAgentId`
- `sessionAiSelectionBySession.selectedAgentId`

核心文件：

- `chat_app/src/lib/store/types.ts`
- `chat_app/src/lib/store/actions/sessions.ts`
- `chat_app/src/lib/store/actions/sendMessage.ts`
- `chat_app/src/lib/store/actions/agents.ts`（改为 Memory Agent 只读/选择）

## 6.4 下线本地 Agent 管理入口

- 菜单移除“智能体管理”：
  - `chat_app/src/components/chatInterface/HeaderBar.tsx`
- `AgentManager` 页面改为“跳转到 Memory Agent 管理”或直接移除。

---

## 7. Chatos 后端整改方案

## 7.1 统一走“模型聊天 + 联系人上下文注入”

目标：

1. 运行时不再依赖本地 `agents` 表去执行 agent chat。
2. 由 `contact_agent_id` 拉取 Memory runtime context，然后注入 system prompt。

推荐流程（每次发送）：

1. 校验 session 归属；
2. 读取 session metadata（contact + runtime）；
3. 读取/校验 `project_root`（优先请求参数，其次 project_id 反解）；
4. 拉取 `memory agent runtime-context`；
5. 组装最终 system prompt；
6. 根据 `mcp_enabled/enabled_mcp_ids` 加载 MCP；
7. 进入 v2/v3 ai pipeline。

## 7.2 MCP 加载策略

当前 `chat_v2/chat_v3` 在 model chat 路径 `use_tools=false` 且空 server，需要改为可控加载：

- `mcp_enabled=false` => 空 MCP bundle；
- `mcp_enabled=true` + `enabled_mcp_ids` 空 => 默认加载全部内置（排除 `agent_builder`）；
- `mcp_enabled=true` + `enabled_mcp_ids` 非空 => 按选择加载。

涉及：

- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`
- `chat_app_server_rs/src/core/mcp_runtime.rs`
- `chat_app_server_rs/src/services/mcp_loader.rs`

## 7.3 `project_root` 安全与透传

必须做两层校验：

1. `project_id` 存在时，`project_root` 必须与项目根一致；
2. 对所有文件系统型 MCP，路径必须限制在 `project_root` 内。

传递路径：

- chat request -> MCP loader `workspace_dir` -> builtin services (`code_maintainer` / `terminal_controller`)。

---

## 8. Sub-Agent Router -> Agent Builder MCP 重构

## 8.1 工具集替换

保留（设计类）：

1. `recommend_agent_profile`
2. `list_available_skills`
3. `create_memory_agent`
4. `update_memory_agent`
5. `preview_agent_context`

移除（运行类）：

1. `run_sub_agent`
2. `cancel_sub_agent_job`

## 8.2 常量与配置替换

涉及：

- `chat_app_server_rs/src/services/builtin_mcp.rs`
- `chat_app_server_rs/src/core/mcp_tools.rs`
- `chat_app_server_rs/src/api/configs.rs`
- `chat_app_server_rs/src/api/configs/builtin_settings.rs`

方向：

- `builtin_sub_agent_router` -> `builtin_agent_builder`
- builtin settings 中“导入 skills/agents”等接口改为转发到 memory，或直接下线旧接口。

## 8.3 前端工具渲染清理

移除 run_sub_agent 相关 modal/判定：

- `chat_app/src/components/ToolCallRenderer.tsx`
- `chat_app/src/components/RunSubAgentModal.tsx`
- `chat_app/src/components/SubAgentRunPanel.tsx`

---

## 9. 分阶段实施计划（建议）

## Phase 0：打底修复（必须先做）

1. 打通 session metadata 全链路：
   - memory_server session 模型支持 metadata；
   - chat_app_server sessions create/update 透传 metadata；
   - memory_server_client 映射 metadata。
2. 修复 `getSessionParams().projectId` 固定空值问题，使当前项目可进入 session 查询/创建。

## Phase 1：Memory Agent + Skill 能力上线

1. memory_server 新增 agents/skills API 与数据表。
2. chat backend 增加 memory 代理 API（BFF）。
3. 前端 `loadAgents` 切 Memory 数据源（先兼容老字段）。

## Phase 2：联系人会话与输入区改造

1. SessionList 改“联系人”交互；
2. InputArea 改“模型 + MCP 开关 + 项目选择”；
3. sendMessage 请求增加 `contact_agent_id/project_root/mcp_enabled`。

## Phase 3：运行时注入改造

1. 后端聊天接口支持新字段；
2. 运行时注入 Memory Agent 角色+skills；
3. MCP 由请求参数和默认策略共同决策。

## Phase 4：Sub-Agent Router 下线与清理

1. 内置 MCP 改名并替换工具集；
2. 删除 run_sub_agent 相关代码路径与前端视图；
3. 下线本地 Agent CRUD 入口与存储。

---

## 10. 验收标准（DoD）

1. 可以在侧边栏“添加联系人”，且联系人只来自 Memory Agent。
2. 输入区不再出现 Agent 选择，只能选模型。
3. 输入区有 MCP 开关，默认开启，且不配置时默认全内置 MCP 可用（排除 agent_builder）。
4. 输入区可选项目，发送时后端可收到并校验 `project_root`。
5. 对话时系统上下文中可见联系人 Agent 的角色定义和 skills 注入效果。
6. `run_sub_agent` 在工具链中不可再被调用，相关 UI 不再出现。
7. Header/菜单中不再提供本地智能体创建入口（改为 Memory Agent 管理入口或移除）。

---

## 11. 主要风险与控制

1. 风险：skills 注入导致上下文过长。
   - 控制：runtime-context 提供 `skills_summary + full_text` 双层，按 token 预算裁剪。
2. 风险：project_root 被伪造越权访问文件。
   - 控制：服务端强校验 project_id 与 root 映射，所有文件操作做 root 内路径约束。
3. 风险：迁移期前后端字段不一致导致发送失败。
   - 控制：新增字段先可选并向后兼容，分阶段切换开关。
4. 风险：旧会话无联系人信息。
   - 控制：提供迁移策略：首次进入时引导绑定联系人，或默认到“通用助手联系人”。

---

## 12. 建议配置开关（灰度）

1. `CHAT_CONTACT_MODE_ENABLED=true`
2. `CHAT_RUNTIME_PROJECT_ROOT_REQUIRED=false`（灰度后改 true）
3. `AGENT_BUILDER_MCP_ENABLED=true`
4. `RUN_SUB_AGENT_TOOLS_ENABLED=false`（最终删除）
5. `LEGACY_LOCAL_AGENT_API_ENABLED=false`（过渡期可临时 true）

