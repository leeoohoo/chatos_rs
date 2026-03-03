# Sub-Agent 独立存储体系改造方案（复核版）

## 1. 背景与目标

用户目标明确：

1. Sub-Agent 运行过程消息不能与主会话 `messages` 混表。
2. Sub-Agent 需要独立的“过程记录 + 最终结果 + 定时总结”体系。
3. Sub-Agent 在每次请求 AI 前，也要像普通会话一样：优先加载“已总结 + 未总结消息”。

本方案在**再次复核现有代码**后给出，目标是低风险落地并保持前端可用。

---

## 2. 现状复核（代码事实）

### 2.1 `run_sub_agent` 过程记录当前不在数据库

- 过程状态和事件目前在内存 `HashMap`：
  - `JOBS` / `JOB_EVENTS` / `JOB_CANCEL_FLAGS` / `JOB_STREAM_CHUNK_SINKS`
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/sub_agent_router/core/jobs.rs:5`
- 同时会写本地 jsonl trace 文件（非 DB）：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/sub_agent_router/core/jobs.rs:12`
  - 默认目录：`~/.chatos/builtin_sub_agent_router`
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/sub_agent_router/settings/state.rs:75`

### 2.2 最终 run 结果会间接进入主会话 `messages`

- `run_sub_agent` 最终返回是 `text_result(...)`，会转成文本：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/sub_agent_router/core/execution.rs:299`
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/core/tool_io.rs:3`
- tool 结果在 chat 流程中会保存为 `messages(role=tool)`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/mod.rs:728`
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/message_manager_common.rs:130`

### 2.3 Sub-Agent 内部 AI 循环默认不持久化

- `purpose == "sub_agent_router"` 时，`persist_messages = false`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_request_handler/mod.rs:132`
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_request_handler/mod.rs:114`
- `persist_tool_messages` 也被关闭：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/mod.rs:315`
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_client/mod.rs:225`

### 2.4 “每轮前总结上下文刷新”目前仅 `chat` 生效

- v3 的 `stable_prefix_mode = purpose == "chat"`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/mod.rs:132`
- 每轮刷新入口限制在 chat 路径：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/mod.rs:248`
- 总结来源是 `SessionSummaryV2 + messages.pending`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/message_manager_common.rs:228`

### 2.5 定时总结 worker 仅扫描主会话消息

- 启动点：`main` 中只启动 `session_summary_job`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/main.rs:41`
- 扫描对象：`messages.summary_status`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/repositories/messages.rs:364`
- 处理目标：`SessionSummaryV2 + messages.mark_summarized`：
  - 文件：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/modules/session_summary_job/executor.rs:187`

**结论**：你的判断完全正确，当前状态无法满足“Sub-Agent 完全独立体系”。

---

## 3. 架构决策

### 3.1 强隔离原则

Sub-Agent 过程域与主会话域分离：

- 主会话域：`sessions` + `messages`（只保留用户对话与最终结论）
- Sub-Agent 域：`sub_agent_runs` + `sub_agent_run_messages` + `sub_agent_run_events` + `sub_agent_run_summaries`

### 3.2 关联而不混存（仅保留最终结论）

主会话只保存 Sub-Agent 的**最终结论文本**（可带极少量状态字段），不保存过程事件、不保存过程消息。  
Sub-Agent 全量过程、工具调用、推理流、总结全部保存在独立表。  
`run_id` 仅作为 Sub-Agent 域内部主键使用，不要求写入主会话消息内容。

---

## 4. 数据模型设计（SQLite/Mongo 双栈）

> 命名延续现有风格，字段尽量与 `messages` / `session_summaries_v2` 对齐，降低改造成本。

### 4.1 `sub_agent_runs`

一条 run 一行，保存生命周期与最终结果。

核心字段：

- `id` (run_id, PK)
- `parent_session_id`（来源会话）
- `user_id`, `project_id`
- `agent_id`, `agent_name`, `command_id`
- `task`, `args_json`
- `status` (`queued|running|done|error|cancelled`)
- `result_preview`, `result_json`, `error`
- `started_at`, `finished_at`, `created_at`, `updated_at`

索引建议：

- `(parent_session_id, created_at DESC)`
- `(status, updated_at)`
- `(user_id, created_at DESC)`

### 4.2 `sub_agent_run_messages`

Sub-Agent 内部消息（替代主会话 `messages` 中的过程数据）。

核心字段：

- `id` (PK)
- `run_id` (FK -> sub_agent_runs.id)
- `role` (`user|assistant|tool|system|developer`)
- `content`
- `message_mode`, `message_source`
- `summary`（保留兼容）
- `tool_calls`（JSON string）
- `tool_call_id`
- `reasoning`
- `metadata`（JSON string）
- `summary_status` (`pending|summarized`)
- `summary_id`
- `summarized_at`
- `created_at`

索引建议：

- `(run_id, created_at ASC)`
- `(run_id, summary_status, created_at ASC)`
- `(run_id, tool_call_id)`
- `(summary_id)`

### 4.3 `sub_agent_run_events`

事件流水（对应当前 `append_job_event`）。

核心字段：

- `id` (PK)
- `run_id` (FK)
- `seq`（run 内递增）
- `event_type`
- `payload_json`
- `created_at`

索引建议：

- `(run_id, seq)`（唯一）
- `(run_id, created_at)`

### 4.4 `sub_agent_run_summaries`

run 级总结（复用 `session_summaries_v2` 结构）。

核心字段：

- `id` (PK)
- `run_id` (FK)
- `summary_text`
- `summary_model`
- `trigger_type`
- `source_start_message_id`, `source_end_message_id`
- `source_message_count`, `source_estimated_tokens`
- `status` (`done|failed`)
- `error_message`
- `created_at`, `updated_at`

索引建议：

- `(run_id, created_at DESC)`
- `(run_id, status, created_at DESC)`

### 4.5 （可选）`sub_agent_summary_job_configs`

若需要独立参数（不同于 session summary job），增加用户级配置表。

---

## 5. 服务层改造

## 5.1 新增 Sub-Agent 独立仓储/模型

新增：

- `models/sub_agent_run.rs`
- `models/sub_agent_run_message.rs`
- `models/sub_agent_run_event.rs`
- `models/sub_agent_run_summary.rs`
- `repositories/sub_agent_runs.rs`
- `repositories/sub_agent_run_messages.rs`
- `repositories/sub_agent_run_events.rs`
- `repositories/sub_agent_run_summaries.rs`

并在：

- `db/sqlite.rs` 增加建表与索引
- Mongo 对应新增 4 个 collection

### 5.2 `core/jobs.rs` 从内存状态改为“DB 主存 + 内存流控”

当前：内存 `JOBS/JOB_EVENTS` 持有主数据。  
改造后：

- DB：run 状态、event 持久化
- 内存：仅保留 `cancel_flag` 和 `stream sink`

对应入口：

- `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/sub_agent_router/core/jobs.rs:5`

### 5.3 `run_sub_agent` 结果改为“结论型返回”

当前返回含全量 `job_events`，导致 tool message 体积大且污染主会话。  
改造后主会话可见返回只保留：

- `status`
- `final_conclusion`（最终结论文本）

不再内嵌全量 `job_events`，也不要求在主会话内容中携带 `run_id`。

> 说明：过程数据只进 Sub-Agent 独立表；主会话只留结论，符合“只看最终答案”的产品目标。

对应入口：

- `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/sub_agent_router/core/execution.rs:283`

### 5.4 AI 持久化改为“按存储作用域”而不是按 purpose

问题点：当前用 `purpose != sub_agent_router` 硬编码决定是否写库。  
改造为：

- `storage_scope = Session(session_id) | SubAgentRun(run_id) | None`
- `purpose` 仅表达业务语义，不再决定持久化位置

涉及：

- `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_request_handler/mod.rs:132`
- `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/mod.rs:315`
- `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_request_handler/mod.rs:114`
- `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v2/ai_client/mod.rs:225`

### 5.5 Sub-Agent 上下文构建使用“run 总结 + run pending”

新增 `SubAgentRunMessageManager`（或在现有 manager 上抽象 scope）：

- `get_run_history_context(run_id, summary_limit)`
- 返回 `merged_summary + pending_messages`

每轮请求前刷新（对 sub-agent 也启用），做到和 chat 一致。

---

## 6. 定时总结体系（Sub-Agent 独立）

新增模块：

- `modules/sub_agent_summary_job/*`

流程：

1. 扫描 `sub_agent_run_messages(summary_status in pending/null)` 找到待处理 run。
2. 按配置阈值（轮数、token）切片总结。
3. 写 `sub_agent_run_summaries`。
4. 回写 `sub_agent_run_messages.summary_status=summarized`。
5. 下轮请求 AI 前自动读取最新 summary + pending。

配置可先用 env（后续再加 user config 表）。

---

## 7. API 与前端改造

### 7.1 新增 API

建议新增：

- `GET /api/sub-agent-runs/:run_id`
- `GET /api/sub-agent-runs/:run_id/messages?limit=&offset=`
- `GET /api/sub-agent-runs/:run_id/events?after_seq=`
- `GET /api/sub-agent-runs/:run_id/summaries`
- `GET /api/sessions/:session_id/sub-agent-runs`（按会话列出）

### 7.2 前端 Run Modal 改为“按 run_id 拉取”

当前 Run 弹窗主要靠 tool result 文本解析。改造后：

- 先从 tool result 取 `run_id`
- 再走新 API 加载详情/事件/总结
- 兼容旧数据：若无 run_id，回退旧解析逻辑

---

## 8. 分阶段实施计划（建议）

### Phase 1：建模与持久化底座

- 新增 4 张 sub-agent 表（SQLite+Mongo）
- 新增 models/repositories/services
- `run_sub_agent` 创建 run 并写事件到 DB（先保留旧返回）

### Phase 2：切换内部消息到独立表

- 引入 `storage_scope=SubAgentRun`
- Sub-Agent 内部 assistant/tool/user 消息全部写 `sub_agent_run_messages`
- 主会话只保留轻量引用型 tool 结果

### Phase 3：总结闭环

- 新增 `sub_agent_summary_job` worker
- 实现“每轮请求前 summary+pending 刷新”对 run 生效

### Phase 4：API/UI 完整切换

- 新增 run 查询 API
- RunSubAgentModal 改为 run_id 拉取
- 移除对大文本 `job_events` 的依赖

### Phase 5：清理旧路径

- 删除 `JOBS/JOB_EVENTS` 作为主存储的逻辑
- 保留必要的内存 cancel/stream 控制

---

## 9. 兼容与迁移策略

1. **前向兼容**：新代码先支持 run_id 读取，也兼容旧文本解析。  
2. **无损上线**：Phase 1/2 可采用双写观察（日志核对）后再关闭旧路径。  
3. **历史数据**：旧 `messages` 中的 run 结果不强制回填；如需可写离线脚本抽取 run_id/摘要。  

---

## 10. 验收标准

1. 主会话 `messages` 不再出现 sub-agent 全量过程（仅引用记录）。
2. run 全量过程可通过 `sub_agent_run_messages + sub_agent_run_events` 重建。
3. 定时总结写入 `sub_agent_run_summaries`，并驱动后续请求上下文压缩。
4. RunSubAgentModal 可在服务重启后仍查看历史过程（不依赖内存态）。
5. 不影响普通 chat 的原有总结链路。

---

## 11. 结论

复核后该方案可行，且方向正确：**Sub-Agent 与主会话彻底分层，主会话只保留引用，过程与总结全部进入独立体系**。这能同时解决你关心的“数据混乱”和“长期可维护性”问题。
