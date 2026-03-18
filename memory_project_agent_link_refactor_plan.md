# Memory 项目索引与项目-智能体关联改造方案

## 1. 背景与问题

当前 `memory` 侧“项目总结”页面的数据来源是：

- 先按联系人读取 `project_memories`
- 再从 `project_memories.project_id` 反推出“项目列表”

这会导致两个直接问题：

1. 项目刚创建、但还没有产出总结时，Memory 页面看不到项目。
2. 智能体已经在某项目下开始对话（会话已创建），但在 Memory 里看不到该项目与智能体的关系。

你提出的方向是正确的：**需要把“项目本身”和“项目-智能体关系”独立建模**，不能继续依赖 `project_memories` 反推。

---

## 2. 目标

1. 在 memory 中新增两张“表”（Mongo 集合）：
   - `memory_projects`
   - `memory_project_agent_links`
2. ChatOS 创建项目时，同步写入 `memory_projects`。
3. ChatOS 在“按项目为智能体创建/确保会话”时，同步写入 `memory_project_agent_links`。
4. Memory 前端“项目总结”页面按关联表展示项目列表，即使项目尚无总结也要可见。
5. 兼容“未选项目”场景（`project_id = "0"`），保证与当前会话逻辑一致。

---

## 3. 总体设计

### 3.1 新增集合一：`memory_projects`

用途：保存项目主数据，作为 Memory 中项目可见性的唯一来源。

建议字段：

- `id`: String（UUID）
- `user_id`: String（租户隔离主键）
- `project_id`: String（ChatOS 项目 ID，跨服务主键）
- `name`: String
- `root_path`: Option<String>
- `description`: Option<String>
- `status`: String（`active | archived | deleted`）
- `is_virtual`: i64（`1/0`，用于 `project_id="0"` 的“未指定项目”虚拟项）
- `created_at`: String
- `updated_at`: String
- `archived_at`: Option<String>

索引：

- unique: `{ user_id: 1, project_id: 1 }`
- index: `{ user_id: 1, status: 1, updated_at: -1 }`
- index: `{ user_id: 1, is_virtual: 1, updated_at: -1 }`

---

### 3.2 新增集合二：`memory_project_agent_links`

用途：记录“某用户下，某项目与某智能体（以及联系人）是否发生过对话关联”。

建议字段：

- `id`: String（UUID）
- `user_id`: String
- `project_id`: String（允许 `"0"`）
- `agent_id`: String
- `contact_id`: Option<String>
- `latest_session_id`: Option<String>
- `first_bound_at`: String
- `last_bound_at`: String
- `last_message_at`: Option<String>
- `status`: String（`active | archived`）
- `created_at`: String
- `updated_at`: String

索引：

- unique: `{ user_id: 1, project_id: 1, agent_id: 1 }`
- index: `{ user_id: 1, contact_id: 1, updated_at: -1 }`
- index: `{ user_id: 1, project_id: 1, updated_at: -1 }`
- index: `{ user_id: 1, agent_id: 1, updated_at: -1 }`

说明：唯一键可保证同一用户同一项目同一 agent 不会重复关联。

---

## 4. API 设计（Memory Backend）

在 `memory_server/backend/src/api/mod.rs` 增加资源：

### 4.1 项目主数据接口

1. `POST /api/memory/v1/projects/sync`
   - 入参：`user_id, project_id, name, root_path, description, status`
   - 语义：按 `(user_id, project_id)` 幂等 upsert
2. `GET /api/memory/v1/projects`
   - 支持：`user_id, status, include_virtual, limit, offset`
3. `PATCH /api/memory/v1/projects/:project_id`
   - 用于重命名/归档/删除状态变更

### 4.2 项目-智能体关联接口

1. `POST /api/memory/v1/project-agent-links/sync`
   - 入参：`user_id, project_id, agent_id, contact_id?, session_id?, last_message_at?`
   - 语义：按 `(user_id, project_id, agent_id)` 幂等 upsert
2. `GET /api/memory/v1/contacts/:contact_id/projects`
   - 返回该联系人的项目索引（来自 link + project join），并附带是否已有 `project_memories` 的标记

### 4.3 兼容规则

- 若 `project_id="0"` 且 `memory_projects` 不存在对应行，接口内部自动创建虚拟项目：
  - `name = "未指定项目"`
  - `is_virtual = 1`

---

## 5. ChatOS 对接方案（BFF + 前端）

### 5.1 chat_app_server_rs（BFF）

在 `chat_app_server_rs/src/services/memory_server_client.rs` 增加 DTO 与调用：

- `sync_memory_project(...)`
- `sync_project_agent_link(...)`
- `list_contact_projects(...)`

调用点：

1. `chat_app_server_rs/src/api/projects.rs`
   - `create_project` 成功后调用 `sync_memory_project`
   - `update_project` 成功后调用 `sync_memory_project`
   - `delete_project` 时调用 memory 项目归档/删除接口
2. `chat_app_server_rs/src/api/sessions.rs::create_session`
   - 从会话 `metadata` 解析 `contact_id/agent_id`
   - 从会话解析 `project_id`（为空归一为 `"0"`）
   - 调用 `sync_project_agent_link`

建议：`sync_*` 接口必须幂等，允许重复调用。

---

### 5.2 chat_app（前端）

前端不需要直接连 memory；保持走现有 BFF 即可。  
现有 `createProject`、`createSession` 流程无需改协议，只需后端补齐同步。

---

### 5.3 memory_frontend（管理台）

调整 `ContactMemoriesPage`：

1. 项目列表来源从 `listContactProjectMemories(contact)` 改为 `listContactProjects(contact)`。
2. 点击项目后再拉 `listContactProjectMemories(contact, project_id)` 显示总结正文。
3. 对“有项目但暂无总结”的行显示 `暂无总结`，而不是整页空表。

这样就能解决你截图里“看不到项目/看不到总结入口”的核心体验问题。

---

## 6. 与现有数据链路的关系

现有集合继续保留并复用：

- `project_memories`: 仍保存项目记忆正文（总结结果）
- `agent_recalls`: 仍保存智能体长期回忆

新增集合只负责：

- 项目索引（`memory_projects`）
- 联系人/智能体在项目下的关系索引（`memory_project_agent_links`）

即：**索引与内容分离**，不再用内容表反推索引。

---

## 7. 历史数据回填

上线后需要一次性回填，避免老数据看不到：

1. 从 ChatOS `projects` 回填 `memory_projects`（按用户维度）。
2. 从 memory `sessions` 回填 `memory_project_agent_links`：
   - 解析 `metadata.contact.agent_id/contact_id`
   - `project_id` 为空归一为 `"0"`
3. 从 `project_memories` 补齐缺失的 `memory_projects` 与 `links`（兜底）。

回填完成后再切 memory 前端到新接口。

---

## 8. 发布步骤（建议）

1. Memory backend：先上新集合索引 + 新接口（不切前端）。
2. ChatOS backend：接入 `sync_memory_project` 与 `sync_project_agent_link`。
3. 跑回填脚本。
4. Memory frontend：切项目列表数据源到 `contacts/:contact_id/projects`。
5. 验证通过后，保留旧 `project_memories` 列表接口作为兼容，不再用于主列表。

---

## 9. 验收标准

1. 新建项目后，不发消息也能在 Memory 的“项目总结”页看到该项目（对应联系人下）。
2. 智能体在某项目首次创建会话后，`memory_project_agent_links` 有且仅有一条关联记录。
3. 同一联系人下，项目切换可看到对应项目总结；无总结项目显示“暂无总结”而非消失。
4. `project_id="0"` 的非项目会话在 Memory 可见且与其他项目隔离。
5. 所有查询按 `user_id` 严格隔离，admin 仅是可见范围更大，不共享记忆数据。

---

## 10. 风险与控制

1. 风险：双写不一致（ChatOS 成功、Memory 同步失败）。
   - 控制：`sync_*` 幂等 + 回填任务 + 启动自愈（从 sessions 补链接）。
2. 风险：历史脏数据导致重复关联。
   - 控制：唯一索引 `(user_id, project_id, agent_id)` + upsert。
3. 风险：`project_id` 空值语义不一致。
   - 控制：统一归一化为 `"0"`，并使用虚拟项目行承载。

---

## 11. 实施清单（代码落点）

1. `memory_server/backend/src/models/mod.rs`：新增 `MemoryProject`、`MemoryProjectAgentLink`。
2. `memory_server/backend/src/db/mod.rs`：新增两集合索引。
3. `memory_server/backend/src/repositories/`：新增 `projects.rs`、`project_agent_links.rs`。
4. `memory_server/backend/src/api/mod.rs`：新增 `projects/sync`、`project-agent-links/sync`、`contacts/:id/projects`。
5. `chat_app_server_rs/src/services/memory_server_client.rs`：新增 `sync/list` 客户端方法。
6. `chat_app_server_rs/src/api/projects.rs`：项目创建/更新/删除后同步 memory 项目。
7. `chat_app_server_rs/src/api/sessions.rs`：创建会话后同步项目-智能体关联。
8. `memory_server/frontend/src/api/client.ts`：新增 `listContactProjects`。
9. `memory_server/frontend/src/pages/ContactMemoriesPage.tsx`：项目列表改为基于新接口。

