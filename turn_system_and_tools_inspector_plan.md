# 联系人会话“当前轮次 System 消息 + 工具列表”方案（Memory 下沉版）

## 1. 目标与结论

本次方案按以下职责边界调整：

1. **Memory Server 负责快照能力本体**：
   - 快照数据落 MongoDB
   - 负责“当前轮次/最后一轮”解析
   - 提供查询 API
2. **ChatOS 只做采集与中转**：
   - 在发起聊天时采集当轮运行时上下文（system + tools）
   - 调用 memory API 写入快照
   - 给前端提供 relay API（转发 memory 的结果）
3. **前端只展示**：
   - 联系人列表新增按钮
   - 抽屉展示当前轮次（或最后一轮）system 消息与工具列表

这个分层比“快照直接挂在 ChatOS 消息 metadata”更清晰，也更符合你说的“快照是 memory 领域能力”。


## 2. 现状核对（基于代码）

### 2.1 ChatOS 当前只提供 process，不提供 runtime snapshot

- 现有接口：
  - `GET /api/sessions/:session_id/turns/:user_message_id/process`
  - `GET /api/sessions/:session_id/turns/by-turn/:turn_id/process`
- 代码位置：
  - `chat_app_server_rs/src/api/sessions.rs`
  - `chat_app_server_rs/src/api/sessions/message_handlers.rs`

这两个接口返回 assistant/tool 过程消息，不包含“当轮 system 文本 + tools 快照”。

### 2.2 system/tool 在 ChatOS 运行时动态生成

- `chat_v2` / `chat_v3` 在发请求前动态拼装：
  - base system（active system context）
  - contact system（角色定义 + 插件 + 技能简介）
  - MCP tools（http/stdio/builtin）
- 代码位置：
  - `chat_app_server_rs/src/api/chat_v2.rs`
  - `chat_app_server_rs/src/api/chat_v3.rs`

### 2.3 消息持久化本来就经过 memory

- user/assistant/tool message 都是通过 `memory_server_client::upsert_message` 入 memory。
- 代码位置：
  - `chat_app_server_rs/src/services/message_manager_common.rs`
  - `chat_app_server_rs/src/services/memory_server_client/session_ops.rs`

### 2.4 你提到“内置 MCP 看不到”是因为口径不同

- MCP 管理器页展示的是“配置层内置 + 用户配置”，不是每轮运行时动态注入。
- 联系人 skill reader 是运行时动态注入 builtin：
  - `chat_app_server_rs/src/core/mcp_runtime.rs`
  - `chat_app_server_rs/src/services/builtin_mcp.rs`
- 所以在 MCP 管理器里看不到它，是当前设计行为。


## 3. 新架构（调整后）

### 3.1 职责划分

1. **Memory Server（主）**
   - 持久化 turn runtime snapshot（数据库）
   - 提供 latest/by-turn 查询
   - 统一做权限校验（基于 session）
2. **ChatOS（从）**
   - 负责“采集快照”与“调用 memory upsert”
   - 前端查询时转发 memory 返回
3. **Frontend**
   - 调用 ChatOS relay API
   - 展示 snapshot 数据

### 3.2 请求链路

1. 用户发送消息
2. ChatOS 在 `chat_v2/chat_v3` 已拿到 base/contact system + MCP tool 清单
3. ChatOS 保存 user message（拿到 `user_message_id`）
4. ChatOS 调 memory：upsert 当前 turn snapshot
5. 前端点击“上下文”按钮
6. 前端 -> ChatOS relay API -> memory latest/by-turn API -> 返回 snapshot


## 4. Memory 数据库存储设计（推荐）

> 推荐新增独立集合：`turn_runtime_snapshots`（而不是继续塞在 message.metadata 里）。

### 4.1 文档结构（建议）

```json
{
  "id": "uuid",
  "session_id": "sess_xxx",
  "user_id": "user_xxx",
  "turn_id": "turn_xxx",
  "user_message_id": "msg_xxx",
  "status": "running|completed|failed|unknown",
  "snapshot_source": "captured",
  "snapshot_version": 1,
  "captured_at": "2026-03-24T12:00:00Z",
  "updated_at": "2026-03-24T12:00:03Z",
  "system_messages": [
    {
      "id": "base_system",
      "source": "active_system_context",
      "content": "..."
    },
    {
      "id": "contact_system",
      "source": "contact_runtime_context",
      "content": "..."
    }
  ],
  "tools": [
    {
      "name": "memory_skill_reader_get_skill_detail",
      "server_name": "memory_skill_reader",
      "server_type": "builtin",
      "description": "..."
    }
  ],
  "runtime": {
    "model": "...",
    "provider": "...",
    "contact_agent_id": "...",
    "project_id": "...",
    "project_root": "...",
    "mcp_enabled": true,
    "enabled_mcp_ids": ["..."]
  }
}
```

### 4.2 索引（Mongo）

在 `memory_server/backend/src/db/schema.rs` 增加：

1. 唯一索引：`{ session_id: 1, turn_id: 1 }`
2. 查询索引：`{ session_id: 1, user_message_id: 1 }`
3. 最新轮次索引：`{ session_id: 1, captured_at: -1 }`


## 5. Memory API 设计

### 5.1 Upsert 接口（给 ChatOS 调用）

`PUT /api/memory/v1/sessions/:session_id/turn-runtime-snapshots/:turn_id/sync`

用途：
- 首次写入（running）
- 结束后补写状态（completed/failed）

### 5.2 查询接口（给 ChatOS relay）

1. `GET /api/memory/v1/sessions/:session_id/turn-runtime-snapshots/latest`
2. `GET /api/memory/v1/sessions/:session_id/turn-runtime-snapshots/by-turn/:turn_id`

### 5.3 latest 解析规则（放在 memory）

1. 在 `messages` 中按 `created_at desc` 找最后一条 `role=user`
2. 取其 `metadata.conversation_turn_id`；若无则退化 `message.id`
3. 用 `session_id + turn_id` 查询 snapshot
4. 返回：
   - 命中：`snapshot_source=captured`
   - 未命中：`snapshot_source=missing`（不做 ChatOS 重建）

这样“正在执行的这轮”与“已完成的最后一轮”统一为一个规则。


## 6. Memory Server 代码改造点

### 6.1 API 层

1. 新增模块：
   - `memory_server/backend/src/api/turn_runtime_snapshots_api.rs`
2. 路由注册：
   - `memory_server/backend/src/api/mod.rs`
3. 权限复用：
   - `ensure_session_access`（已存在）

### 6.2 Model / Repository

1. 新增 model：
   - `memory_server/backend/src/models/turn_runtime_snapshots.rs`
   - 并在 `models/mod.rs` 导出
2. 新增 repository：
   - `memory_server/backend/src/repositories/turn_runtime_snapshots.rs`
   - 或子目录 `repositories/turn_runtime_snapshots/*`
3. `repositories/mod.rs` 导出新模块
4. `db/schema.rs` 增加索引初始化


## 7. ChatOS 改造点（中转定位）

### 7.1 采集并写入 memory

在 `chat_v2` / `chat_v3` 中：

1. 在 MCP 初始化完成后，构建 runtime snapshot payload
2. 在 `AiServer::chat` 保存 user message拿到 `user_message_id` 后，调用 memory upsert
3. 可选：在对话结束后再补一次 status

涉及文件：

- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`
- `chat_app_server_rs/src/services/v2/ai_server.rs`
- `chat_app_server_rs/src/services/v3/ai_server.rs`
- `chat_app_server_rs/src/services/memory_server_client/dto.rs`
- `chat_app_server_rs/src/services/memory_server_client/session_ops.rs`

### 7.2 ChatOS 对前端的 relay API

在现有 `/api/sessions/...` 下新增：

1. `GET /api/sessions/:session_id/turns/latest/runtime-context`
2. `GET /api/sessions/:session_id/turns/by-turn/:turn_id/runtime-context`

实现原则：

- ChatOS 仅做 `ensure_owned_session` + 转发 memory 返回
- 不在 ChatOS 本地做重建

涉及文件：

- `chat_app_server_rs/src/api/sessions.rs`
- `chat_app_server_rs/src/api/sessions/message_handlers.rs`（或拆新 handler）


## 8. 前端改造点

### 8.1 联系人列表按钮

在 `CONTACTS` 每行“总结”旁新增“上下文”按钮：

- `chat_app/src/components/sessionList/sections/SessionSection.tsx`
- `chat_app/src/components/sessionList/useSessionListActions.ts`
- `chat_app/src/components/SessionList.tsx`
- `chat_app/src/components/ChatInterface.tsx`

### 8.2 新增抽屉

建议新增：

- `chat_app/src/components/chatInterface/TurnRuntimeContextDrawer.tsx`

展示：

1. 轮次信息：`turn_id`、`status`、`snapshot_source`
2. system 消息（完整文本）
3. 工具列表（name/server_type/description）

### 8.3 前端 API

新增 client 方法：

- `getSessionLatestTurnRuntimeContext(sessionId)`
- `getSessionTurnRuntimeContextByTurn(sessionId, turnId)`

文件：

- `chat_app/src/lib/api/client/workspace.ts`
- `chat_app/src/lib/api/client.ts`


## 9. 兼容与回退策略

1. **历史消息无快照**：返回 `snapshot_source=missing`，前端明确提示“该轮未存档”。
2. **不做 ChatOS 重建**：保持“memory 负责快照能力”的边界。
3. **MCP 管理器不变**：仍是配置视图，不等价于 turn runtime 视图。


## 10. 验收标准

1. 联系人会话正在执行时，点击“上下文”能看到当前轮次 snapshot。
2. 会话完成后，点击显示最后一轮 snapshot。
3. 工具列表包含动态注入的 `memory_skill_reader_get_skill_detail`。
4. 历史未存档轮次返回 `snapshot_source=missing` 且前端有清晰提示。
5. 跨用户访问被拒绝（session 权限校验生效）。


## 11. 实施顺序（建议）

### Phase 1（主干）

1. Memory：新集合 + 新 API + 索引
2. ChatOS：采集并调用 memory upsert；新增 relay 查询接口
3. 前端：联系人按钮 + 抽屉展示

### Phase 2（增强）

1. 增加状态细分（running/completed/failed）
2. 抽屉增加刷新和搜索
3. 增加埋点（打开率、missing 命中率）
