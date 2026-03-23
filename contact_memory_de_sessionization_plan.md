# Chatos 去会话化改造方案（联系人唯一 + 项目隔离记忆 + Agent 全局回忆）

## 1. 目标（基于你这次最新要求）

1. 对用户语义彻底去掉 `session` 概念，产品主语义改为 `联系人(contact)`。
2. 联系人不可重复添加：同一用户下同一 agent 只能有一个联系人。
3. 总结体系从“会话总结”改为“Agent 记忆构建”：
   - 项目内短/中期记忆（按项目隔离）
   - Agent 跨项目长期回忆（超出项目范围）
4. 记忆隔离必须同时满足：
   - 用户隔离（不同用户绝不串）
   - 项目隔离（项目内记忆不外溢）
5. 旧会话不再在 Chatos 可见，不再作为主业务对象。

---

## 2. 当前问题（已对应现状）

1. 前端仍加载 `/sessions`，所以旧会话会继续出现。
2. `messages / summaries / summary jobs / rollup jobs` 全链路还是 `session_id` 维度。
3. “联系人”目前只是 `session` 的展示改名，未形成独立唯一实体。
4. 总结仍是会话级，不是 agent 记忆级，不支持“项目隔离 + 全局回忆”双层结构。

---

## 3. 目标域模型（无 Session 业务语义）

## 3.1 Contact（联系人）

- 含义：某用户与某 agent 的唯一对话入口。
- 唯一键：`(owner_user_id, agent_id)`。
- 说明：admin 创建的 agent 可被所有用户选用，但每个用户自己的 contact 独立。

## 3.2 Conversation（内部技术对象，不对外暴露 Session）

- 含义：联系人在某项目下的一条会话流容器（技术分片，不是产品概念）。
- 关键字段：`contact_id + project_id + status`。
- 仅用于消息存储和流式处理，不在 UI 中显示“会话列表”。

## 3.3 Project Memory（项目记忆，短/中期）

- 含义：某用户某联系人在某项目下沉淀出的层级记忆。
- 主键维度：`owner_user_id + contact_id + project_id`。
- 用途：对话时提供该项目上下文记忆，严格项目隔离。

## 3.4 Agent Recall（Agent 全局回忆，长期）

- 含义：某用户某 agent 的跨项目长期事实/偏好/约束记忆。
- 主键维度：`owner_user_id + agent_id`（不带 project）。
- 来源：由各项目记忆抽取汇总得到，可设置衰减与版本。

---

## 4. 数据结构与索引方案（Mongo）

## 4.1 新增集合

1. `contacts`
2. `contact_conversations`
3. `contact_messages`
4. `project_memories`
5. `project_memory_fragments`
6. `agent_recalls`
7. `agent_recall_events`（可选，记录回忆变更轨迹）

## 4.2 关键字段建议

### `contacts`
- `id`
- `owner_user_id`
- `agent_id`
- `agent_name_snapshot`
- `status` (`active|disabled|deleted`)
- `created_at / updated_at`

索引：
- unique: `{owner_user_id:1, agent_id:1}`（如果要软删复用，则改 partial unique）
- index: `{owner_user_id:1, updated_at:-1}`

### `contact_conversations`
- `id`
- `owner_user_id`
- `contact_id`
- `project_id`
- `project_root`
- `model_id_last`
- `mcp_policy_last`（包含开关与白名单）
- `status`
- `created_at / updated_at`

索引：
- index: `{owner_user_id:1, contact_id:1, project_id:1, updated_at:-1}`

### `contact_messages`
- `id`
- `owner_user_id`
- `contact_id`
- `conversation_id`
- `project_id`
- `role/content/metadata`
- `memory_state` (`pending|summarized|rolled_up`)
- `created_at`

索引：
- unique: `{id:1}`
- index: `{owner_user_id:1, contact_id:1, project_id:1, created_at:1}`
- index: `{conversation_id:1, created_at:1}`
- index: `{owner_user_id:1, memory_state:1, created_at:1}`

### `project_memory_fragments`
- `id`
- `owner_user_id`
- `contact_id`
- `agent_id`
- `project_id`
- `level`（L0/L1...）
- `summary_text`
- `source_message_ids`
- `status`
- `created_at / updated_at`

索引：
- index: `{owner_user_id:1, contact_id:1, project_id:1, level:1, created_at:1}`
- index: `{status:1, created_at:1}`

### `project_memories`
- `id`
- `owner_user_id`
- `contact_id`
- `agent_id`
- `project_id`
- `memory_text`
- `memory_version`
- `last_source_at`
- `updated_at`

索引：
- unique: `{owner_user_id:1, contact_id:1, project_id:1}`
- index: `{owner_user_id:1, agent_id:1, updated_at:-1}`

### `agent_recalls`
- `id`
- `owner_user_id`
- `agent_id`
- `recall_key`（去重键，如偏好/约束哈希）
- `recall_text`
- `confidence`
- `last_seen_at`
- `updated_at`

索引：
- unique: `{owner_user_id:1, agent_id:1, recall_key:1}`
- index: `{owner_user_id:1, agent_id:1, updated_at:-1}`

---

## 5. API 重构方案

## 5.1 Memory Server（新增主接口）

1. `GET /api/memory/v1/contacts`
2. `POST /api/memory/v1/contacts`
   - 幂等：若 `(owner_user_id, agent_id)` 已存在，返回已有 contact（`created=false`）
3. `DELETE /api/memory/v1/contacts/:contact_id`
4. `GET /api/memory/v1/contacts/:contact_id/messages?project_id=...`
5. `POST /api/memory/v1/contacts/:contact_id/messages`（写消息并触发记忆流水线）
6. `GET /api/memory/v1/contacts/:contact_id/project-memories/:project_id`
7. `GET /api/memory/v1/contacts/:contact_id/agent-recalls`

## 5.2 Chat App Server（BFF）

1. 下线对前端暴露的 `sessions` 主流程（兼容期可保留但前端不再调用）。
2. 新增 `/api/contacts` 系列代理接口，统一把 auth user 透传给 memory。
3. 流式聊天请求改为必带：
   - `contact_id`
   - `project_id`
   - `project_root`
   - `model_id`
   - `mcp_enabled / enabled_mcp_ids`

---

## 6. 记忆流水线改造（替代 summary/rollup 语义）

## 6.1 L0 项目记忆生成

- 输入：`contact_messages` 中某 `owner_user_id + contact_id + project_id` 的 pending 消息。
- 输出：`project_memory_fragments(level=0)`。
- 同步把消息 `memory_state` 标为 `summarized`。

## 6.2 项目记忆聚合（L1+）

- 输入：同项目下 fragments。
- 输出：`project_memories`（upsert）。
- 达到阈值后做 rollup，控制 tokens。

## 6.3 全局回忆提取

- 输入：各项目 `project_memories`。
- 输出：`agent_recalls`（按 `recall_key` 去重/更新）。
- 规则：只沉淀“跨项目稳定信息”，不沉淀临时任务细节。

## 6.4 对话上下文拼装

每次发送时拼装：
1. Agent 角色定义 + skills
2. 当前项目 `project_memories`（强相关）
3. `agent_recalls`（跨项目长期记忆）
4. 最近若干原始消息（短窗口）

---

## 7. 隔离与权限规则（重点）

1. 所有查询必须带 `owner_user_id`（来自 token，不信任前端入参）。
2. 项目记忆必须强制 `project_id` 过滤，不允许“空项目回退全局”。
3. admin 共享 agent 只代表“可选”，不代表“共享记忆”：
   - A 用户和 B 用户使用同一个 admin agent，记忆完全分离。
4. contact 不可跨用户访问；删除 contact 不影响其他用户同 agent 的 contact。

---

## 8. 前端（Chatos）改造点

1. 左侧列表只展示 `contacts`，彻底移除“会话”命名和入口。
2. 添加联系人弹窗：
   - 已有联系人的 agent 不再可重复添加（前端过滤 + 后端幂等兜底）。
3. 输入区保留：
   - 模型选择
   - MCP 总开关（默认开）
   - 项目选择（决定 `project_root` 透传）
4. “历史总结”入口改为“记忆视图”：
   - 当前项目记忆
   - Agent 全局回忆

---

## 9. 迁移策略（避免一次性硬切失败）

## Phase A：并行建模（不切流量）

1. 上线新集合与索引。
2. 补齐 contacts/conversations/messages 新写入链路（灰度开关控制）。

## Phase B：历史数据迁移

1. 从旧 `sessions` 按 `(user_id, metadata.contactAgentId)` 生成 `contacts`。
2. 旧 `messages` 迁移到 `contact_messages`（补 `project_id`）。
3. 旧 `session_summaries_v2` 迁移到 `project_memory_fragments/project_memories`。
4. 基于项目记忆批量生成首版 `agent_recalls`。

## Phase C：前端切流

1. 前端停止调用 `/sessions`。
2. 首页仅拉 `/contacts`。
3. 旧会话入口与文案彻底去除。

## Phase D：下线旧链路

1. 下线 `session summary/rollup` 作业。
2. `/sessions` 路由转只读兼容（短期）后删除。
3. 清理 `session_*` 相关前端 store、组件、接口。

---

## 10. 验收标准（DoD）

1. 新登录后 UI 不再出现任何历史会话列表，仅联系人列表。
2. 同一用户重复添加同一 agent，返回同一个 contact，不会产生重复记录。
3. 同一联系人在项目 A 与项目 B 的记忆完全隔离（抽样验证无串数据）。
4. 两个不同用户使用同一个 admin agent，互相看不到对方记忆。
5. 对话上下文确实携带“项目记忆 + 全局回忆”，并可追踪来源。
6. 旧 `session` 路由在前端主流程中 0 调用。

---

## 11. 风险与回滚

1. 风险：迁移期间上下文缺失导致回答质量波动。  
   方案：保留短期双读（新记忆缺失时回退旧总结，只在灰度期启用）。
2. 风险：唯一索引上线前已有脏数据冲突。  
   方案：先跑去重脚本，再建 unique 索引。
3. 回滚：保留旧 `sessions + summaries` 读链路开关，可一键切回旧上下文拼装。

---

## 12. 建议实施顺序（两周节奏）

1. 第 1-2 天：新模型/索引/API skeleton。
2. 第 3-5 天：写入链路 + 记忆作业改造。
3. 第 6-8 天：前端切换到 contacts + 输入区参数收敛。
4. 第 9-10 天：历史迁移脚本 + 校验报表。
5. 第 11-12 天：灰度、回归、清理旧路由。
