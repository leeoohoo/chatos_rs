# Agent Builder MCP + Memory Agent 化改造方案（更新版）

## 1. 新约束与目标（按你的最新要求）

本次方案以以下约束为准：

1. 后续不再需要 `run_sub_agent` 工具。
2. 下线 `sub-agent-router` 这个“运行型内置 MCP”。
3. 改造成“生成 Agent 的专用 MCP”（下文统一称 `agent_builder`）。
4. `memory_server` 成为 Agent/Skill 的唯一管理中心。
5. `chatos` 不再使用当前本地 Agent 创建能力，改为直接选择 memory 里的 Agent。
6. 使用 memory Agent 对话时，把 Agent 角色定义 + Skills 内容注入对话上下文。

---

## 2. 现状（关键代码点）

### 2.1 当前 Sub-Agent Router 的问题

当前内置 MCP `builtin_sub_agent_router` 仍是“推荐 + 运行子代理”模式：

- 工具：`get_sub_agent` / `suggest_sub_agent` / `run_sub_agent` / `cancel_sub_agent_job`
- 入口：`chat_app_server_rs/src/builtin/sub_agent_router/mod.rs`
- 执行链路：`run_sub_agent_sync -> core/job_executor/*`

这与“只做 Agent 生成，不做子代理运行”目标冲突。

### 2.2 Skill 安装与状态不在 memory

当前 Skill/Plugin 导入安装在 chat backend 的 builtin settings 接口里：

- `/api/mcp-configs/:config_id/builtin/import-git`
- `/api/mcp-configs/:config_id/builtin/install-plugin`
- 状态文件在：`~/.chatos/builtin_sub_agent_router/*`

不是 memory 托管，不利于统一治理。

### 2.3 Chatos 本地 Agent 仍是一套独立体系

当前 Chatos 有独立 Agent CRUD：

- 后端：`chat_app_server_rs/src/api/agents.rs`
- 前端：`chat_app/src/components/AgentManager.tsx`

这与“后续用 memory agents，chatos 本地 agent 创建不要了”冲突。

---

## 3. 目标架构（重定向）

不再保留“Sub-Agent 运行面”，改成“Agent 设计面 + Chat 运行注入面”。

1. **设计面（Design Plane）**：`agent_builder` MCP
   - 专注：根据需求生成 Agent 配置、选择 skills、创建/更新 memory agent。
   - 不提供任何任务执行工具（无 `run_sub_agent`）。

2. **控制面（Control Plane）**：`memory_server`
   - Skill 导入/安装、Agent 管理、权限策略都归 memory。
   - 提供 Agent runtime context 给 chatos。

3. **对话运行面（Chat Runtime）**：`chatos` 常规对话链路
   - 选择 memory agent 后，在请求模型前拼接“agent role + skills”到 system prompt。
   - 不再启动子代理 job，不走 run_sub_agent 事件流。

---

## 4. 术语与组件重命名

### 4.1 内置 MCP 重命名

从：

- `builtin_sub_agent_router`
- `server_name=sub_agent_router`

改为：

- `builtin_agent_builder`
- `server_name=agent_builder`

### 4.2 能力边界

保留：

- 推荐/生成 agent profile
- 选择 skill
- 创建/更新 memory agent

移除：

- `run_sub_agent`
- `cancel_sub_agent_job`
- 子代理执行事件/作业存储

---

## 5. Memory 数据模型（新增，成为唯一真源）

建议在 `memory_server` 增加集合（全部按 `user_id` 作用域隔离）：

1. `memory_skills`
   - `id, user_id, plugin, source, skill_id, name, description, content, version, updated_at`
2. `memory_skill_plugins`
   - `id, user_id, source, name, category, installed, discoverable_counts, updated_at`
3. `memory_agents`
   - `id, user_id, name, description, category, role_definition, skill_ids, default_skill_ids, mcp_policy, status, created_by(manual|ai), created_at, updated_at`
4. `memory_agent_versions`
   - `id, agent_id, user_id, version, snapshot_json, created_at`
5. `memory_agent_build_jobs`
   - `id, user_id, requirement, suggestion_json, final_agent_id, status, trace, created_at, finished_at`

说明：

- `role_definition` 直接存文本，避免依赖外部文件路径。
- `skill content` 建议可裁剪存储（完整文本 + 运行摘要）。

---

## 6. API 方案

## 6.1 Memory API（新增）

前缀建议：`/api/memory/v1/agents` 与 `/api/memory/v1/skills`

1. `GET /api/memory/v1/skills/plugins`
2. `POST /api/memory/v1/skills/import-git`
3. `POST /api/memory/v1/skills/plugins/install`
4. `GET /api/memory/v1/skills`
5. `GET /api/memory/v1/agents`
6. `POST /api/memory/v1/agents`
7. `PATCH /api/memory/v1/agents/:agent_id`
8. `DELETE /api/memory/v1/agents/:agent_id`
9. `POST /api/memory/v1/agents/ai-create`
10. `GET /api/memory/v1/agents/:agent_id/runtime-context`

`runtime-context` 返回建议：

- `agent_id`
- `role_definition`
- `skills`（name + content/summary）
- `mcp_policy`
- `updated_at`

## 6.2 Chat Backend API（新增/调整）

1. 新增：`POST /api/agent-builder/ai-create`
   - 给 memory 调用（BFF 模式），内部通过 `agent_builder` MCP 完成建议与创建。
2. 新增：`GET /api/agent-builder/tools/health`（可选）
   - 用于诊断 MCP 可用性。
3. 逐步下线：`/api/agents/*` 本地 CRUD 接口。

---

## 7. Agent Builder MCP 设计（核心）

在 `chat_app_server_rs` 内置 MCP 中替换 `sub_agent_router` 为 `agent_builder`，工具建议如下：

1. `recommend_agent_profile`
   - 输入：`user_requirement`, `preferred_category?`, `constraints?`
   - 输出：`name`, `description`, `category`, `candidate_skill_ids`, `reason`

2. `list_available_skills`
   - 输入：`query?`, `limit?`
   - 输出：可选 skill 列表（来自 memory）

3. `create_memory_agent`
   - 输入：`name`, `description`, `category?`, `role_definition`, `skill_ids`, `default_skill_ids?`, `mcp_policy?`
   - 输出：`agent_id`, `created`

4. `update_memory_agent`
   - 输入：`agent_id`, `patch`
   - 输出：`updated`

5. `preview_agent_context`
   - 输入：`agent_id` 或临时 agent 草案
   - 输出：注入模型前的 system prompt 预览

明确移除：

- `run_sub_agent`
- `cancel_sub_agent_job`
- 任何“执行任务/运行命令”的子代理工具

---

## 8. Chatos 侧改造

## 8.1 前端

1. 输入区 Agent 下拉数据源改为 memory agents。
2. 下线 AgentManager 中“创建/编辑/删除本地 agent”。
3. 保留“选择 agent”能力，但仅选择 memory agent。

涉及文件（方向）：

- `chat_app/src/components/InputArea.tsx`
- `chat_app/src/components/ChatInterface.tsx`
- `chat_app/src/components/AgentManager.tsx`（删除或只读替换）
- `chat_app/src/lib/store/actions/agents.ts`（切 memory API）

## 8.2 后端

1. 聊天请求增加 `memory_agent_id`（或复用 `selected_agent_id` 语义，但来源改为 memory）。
2. 发模型前调用 memory：`GET /agents/:id/runtime-context`。
3. 按规则注入 system prompt：

```text
[Global System Context]
+ [Memory Agent Role Definition]
+ [Memory Agent Skills Bundle]
+ [Tool/Policy Guardrails]
```

4. 控制 token 预算：技能文本超长时按优先级裁剪。

---

## 9. Skill 安装迁移（从 chat backend 转到 memory）

### 9.1 阶段 1（快速上线）

- memory 先提供同名能力接口；内部可暂时代理 chat backend 旧接口。
- UI 全部迁到 memory，chatos 不再展示 sub-agent marketplace 设置页。

### 9.2 阶段 2（彻底迁移）

- memory backend 自己实现 import/install/解析/持久化。
- chat backend 删除旧 `/api/mcp-configs/:id/builtin/*` sub-agent 相关接口。

---

## 10. 下线清单

## 10.1 必下线

1. `builtin_sub_agent_router` 常量与配置入口。
2. `run_sub_agent`、`cancel_sub_agent_job` 工具注册。
3. 子代理执行相关作业与事件仓储（确认无依赖后删除）。
4. chatos 本地 Agent 创建入口（前后端）。

## 10.2 兼容期可保留（只读/隐藏）

1. 旧 Agent API 可短期保留为 302/代理到 memory（便于平滑切流）。
2. 旧表结构延迟删除，待观测稳定后清理。

---

## 11. 配置与开关（灰度发布）

建议新增：

1. `AGENT_BUILDER_MCP_ENABLED=true|false`
2. `MEMORY_AGENT_SOURCE=memory|legacy`
3. `LEGACY_AGENT_API_ENABLED=true|false`
4. `RUN_SUB_AGENT_TOOLS_ENABLED=false`（默认 false，最终移除）

---

## 12. 迁移路径（推荐）

### M1：结构切换（3-5 天）

1. 新建 `agent_builder` MCP（不含运行工具）。
2. memory 增加 Skills/Agents 菜单与接口。
3. chatos 前端支持选择 memory agents。

### M2：运行切换（4-6 天）

1. chatos 对话注入 memory agent runtime context。
2. 关闭本地 Agent 创建按钮与 `/api/agents` 写接口。

### M3：清理收口（2-4 天）

1. 删除 `run_sub_agent` 及相关执行模块。
2. 删除 `builtin_sub_agent_router` 残留配置与文档。
3. 数据库与代码清理。

---

## 13. 测试方案

1. 单测：
   - agent_builder 工具参数校验
   - runtime-context 拼接顺序与裁剪
2. 集成：
   - `ai-create` 创建 memory agent
   - 选择 memory agent 后发起对话，system prompt 注入正确
3. E2E：
   - Skills 导入 -> Agent AI 创建 -> Chatos 选择 -> 对话生效

---

## 14. 风险与应对

1. 风险：memory 不可用导致 agent 下拉空。
   - 应对：降级到“无 agent 普通对话”，并提示不可用。
2. 风险：技能内容过长导致 prompt 超限。
   - 应对：skills 摘要化 + 分层裁剪策略。
3. 风险：旧代码删早了影响现网。
   - 应对：按 feature flag 分步下线，先隐藏入口再删实现。

---

## 15. 最终状态定义（Done 标准）

满足以下 6 条即视为改造完成：

1. `sub-agent-router` 不再作为运行型 MCP 存在。
2. 系统无 `run_sub_agent`/`cancel_sub_agent_job` 工具可调用。
3. memory 成为 skills + agents 的唯一管理入口。
4. chatos 不再提供本地 agent 创建。
5. chatos 可以选择 memory agents 并在对话中注入 role + skills。
6. 旧链路全部可控下线，文档与监控齐全。
