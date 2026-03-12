# Memory 历史上下文重组 + 主项目清理方案

## 1. 目标

1. 历史上下文统一在 `memory_server` 组装后返回，`chat_app_server_rs` 不再自己拼摘要/历史。
2. 历史拼装规则改为：
   - 先取“最高两个层级”的总结；
   - 再取 `level=0` 的两个总结；
3. 既然会话/消息/总结数据已迁移到 memory，主项目删除所有多余的本地数据源和分支逻辑（不再双轨）。

---

## 2. 现状（当前实现）

当前 `compose_context` 逻辑是：

1. 过滤 `status=done AND rollup_status=pending` 的总结；
2. 按 `level DESC, created_at DESC` 排序后取前 `summary_limit` 条（当前调用一般传 2）；
3. 把选中的总结按时间正序拼接成 `merged_summary`；
4. 取该会话全部 `summary_status='pending'` 的消息作为 `messages`。

这就是你说的“2 条总结 + 全量 pending 消息”的来源。

---

## 3. 新历史拼装规则（落地定义）

### 3.1 总结选取规则

从 `session_summaries_v2` 中只考虑：
- `status='done'`
- `rollup_status='pending'`
- `summary_text` 非空

然后按两段选取并去重：

1. `top_level_part`：
   - 取“最高的两个总结”。
   - 实现为：在全部可用总结里按 `level DESC, created_at DESC` 排序后取前 2 条（可同级，可跨任意层级）。
   - 不做“distinct level”限制。
2. `level0_part`：
   - 从 `level=0` 里取最新 2 条（独立补充）。
3. 合并：
   - `selected = top_level_part + level0_part`
   - 按 `summary_id` 去重（稳定保序，前者优先，避免同一条被重复选中）；
   - 最后按 `created_at ASC` 重新排序后拼接到 `merged_summary`（保证提示词是时间顺序）。

> 说明：这里默认就是“最高优先取 2 条（可同级）”。

### 3.2 消息选取规则

- `messages = list_pending_messages(session_id, None)`
- 即：返回该会话全部 `summary_status='pending'` 消息，按时间升序。

### 3.3 返回结构（不新增字段）

严格按现有接口返回，不新增你没要求的字段：

- `session_id`
- `merged_summary`
- `summary_count`
- `messages`
- `meta`（保持现有结构，不扩字段）

---

## 4. 代码改造点

### 4.1 `memory_server`（必须改）

文件：`memory_server/backend/src/services/context.rs`

改动：
1. 重写 `compose_context` 的 summary selection。
2. 不再依赖 `summary_limit` 控制历史上下文（可保留字段但忽略，或仅用于兼容旧调用）。
3. `messages` 固定取全部 pending（除非明确传 `pending_limit` 才限流）。
4. 补充日志：打印本次选中的 level 分布和 summary id。

可选：在 `ComposeContextRequest.mode` 中支持 `mode=hierarchical_v2`，默认启用新逻辑。

---

### 4.2 `chat_app_server_rs`（必须改）

核心原则：**memory-only，删除 fallback**。

#### A. 上下文获取改为强依赖 memory

文件：`chat_app_server_rs/src/services/message_manager_common.rs`

1. `get_chat_history_context`：
   - 直接调用 `memory_server_client::compose_context`。
   - 删除本地 `SessionSummaryV2Service + MessageService` 回退逻辑。
2. `get_session_history_with_summaries`：
   - 直接走 memory API。
   - 删除本地 `SessionSummaryService/MessageService` 分支。
3. `persist_message/get_session_messages/get_message_by_id`：
   - 直接走 `memory_server_client`。
   - 删除 `remote_only_enabled` 分支。
4. `process_pending_saves`（本地同步残留）删除或置空。

#### B. API 层删除本地双轨

重点文件：
- `chat_app_server_rs/src/api/sessions.rs`
- `chat_app_server_rs/src/api/messages.rs`
- `chat_app_server_rs/src/api/session_summary_job_config.rs`
- `chat_app_server_rs/src/api/chat_v2.rs`
- `chat_app_server_rs/src/api/chat_v3.rs`
- `chat_app_server_rs/src/core/chat_context.rs`
- `chat_app_server_rs/src/core/session_access.rs`
- `chat_app_server_rs/src/core/messages.rs`
- `chat_app_server_rs/src/services/session_title.rs`

改动：
- 删除 `if memory_server_client::remote_only_enabled() { ... } else { local ... }`。
- 统一只保留 memory 调用分支。

#### C. 配置项收敛

文件：`chat_app_server_rs/src/config.rs`

1. 删除：
   - `MEMORY_SERVER_REMOTE_ONLY`
   - `MEMORY_SERVER_CONTEXT_ENABLED`
2. 改为：memory 上下文默认开启且不可关闭。
3. 保留：`MEMORY_SERVER_BASE_URL`、鉴权 token、超时配置。

---

### 4.3 删除本地会话/消息/总结任务（必须删）

文件：
- `chat_app_server_rs/src/main.rs`
- `chat_app_server_rs/src/modules/mod.rs`
- `chat_app_server_rs/src/modules/session_summary_job/**`
- `chat_app_server_rs/src/modules/sub_agent_summary_job/**`（若该数据也已迁走）
- `chat_app_server_rs/src/modules/session_archive_job/**`

改动：
1. `main.rs` 去掉本地 background worker 启动。
2. `modules/mod.rs` 去掉对应模块导出。
3. 删除上述模块代码。

---

## 5. 需删除的本地数据模型/仓储（按依赖清理）

在 API 和 service 全部改成 memory-only 后，再删以下文件（避免先删导致大面积编译报错）：

1. models（会话/消息/总结相关）
   - `models/session.rs`
   - `models/message.rs`
   - `models/session_summary.rs`
   - `models/session_summary_v2.rs`
   - `models/session_summary_job_config.rs`
   - `models/session_summary_message.rs`
2. repositories（对应本地仓储）
   - `repositories/session_summaries.rs`
   - `repositories/session_summaries_v2.rs`
   - `repositories/session_summary_job_configs.rs`
   - `repositories/session_summary_messages.rs`
   - 以及仅被上述模型引用的会话/消息本地仓储代码

> 执行方式：每删一批，跑一次 `cargo check`，用编译错误反推剩余引用点，直到会话/消息/总结链路彻底无本地依赖。

---

## 6. 实施顺序（建议）

1. **先改 memory_server `compose_context` 新规则**（不动主项目）。
2. 主项目改 `message_manager_common` 为 memory-only。
3. API 层移除本地 fallback。
4. 停本地 background job。
5. 删除本地 model/repository 残留。
6. 全量 `cargo check`（`memory_server/backend` + `chat_app_server_rs`）。
7. 联调：创建会话 -> 发送消息 -> 调用 compose_context -> 校验返回结构与顺序。

---

## 7. 验收标准

1. `compose_context` 返回的 summary 满足：
   - 全部总结里按 `level DESC, created_at DESC` 取前 2 条（可同级）；
   - 再补 `level=0` 最新 2 条；
   - 去重后按时间正序拼接。
2. 返回消息是该会话全部 pending。
3. 主项目不再出现会话/消息/总结的本地 fallback 分支。
4. 启动后不再运行本地 summary/archive worker。
5. 发送消息时历史上下文只来源于 memory_server。

---

## 8. 风险与处理

1. 风险：删除本地模型后有隐藏依赖导致编译失败。
   - 处理：按“改 API -> 改 service -> 删 model/repo”顺序，分批 `cargo check`。
2. 风险：顶层总结不足导致 summary 为空。
   - 处理：允许部分缺失；若没有可用总结，则只返回 pending 消息。
3. 风险：“高层总结两条”的细化口径歧义。
   - 处理：当前按“`level > 0` 按 `level DESC, created_at DESC` 取前 2 条（可同级）”实现，如需改成“固定来自两个 level”，只改 selection 函数即可。
