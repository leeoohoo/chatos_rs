# 会话总结重构方案（独立模块版：定时总结 + 专用配置页）

更新时间：2026-02-27  
范围：`chat_app_server_rs` + `chat_app`  

## 0) 先说结论（基于现有代码）

我先看了当前实现，结论是：

1. **总结算法层已经可复用**：`services/summary/engine.rs` + `services/summary/token_budget.rs`。  
2. **AI 文本请求工具已经有雏形**：`services/ai_prompt_tool.rs`（system prompt 生成功能在用）。  
3. 但 `ai_prompt_tool` 当前只走 `v2/chat/completions`，**不能完整覆盖 Responses/代理兼容场景**。  
4. `v2/v3 summary_adapter` 带有旧链路落库与 placeholder 副作用，不适合直接拿来做新定时任务主逻辑。  

所以这次方案重点是：**定时总结模块完全独立**，但 AI 请求与总结算法尽量复用现有能力。

---

## 1) 约束与边界

## 本方案做

1. 新建独立模块处理“扫描 session -> 判断阈值 -> 总结 -> 标记消息”。  
2. 新建专门配置页，配置总结模型、长度阈值、轮数阈值（可扩展定时间隔）。  
3. 优化 AI 请求工具，使 system_prompt 场景与定时总结都能复用一套调用入口。  

## 本方案不做

- 不改现有聊天时历史拼装逻辑（你明确说先不管）。  

---

## 2) 模块拆分（重点：与现有逻辑隔离）

## 2.1 后端新增独立命名空间

新增目录（建议）：

`chat_app_server_rs/src/modules/session_summary_job/`

子模块建议：

- `mod.rs`：模块入口与对外接口  
- `worker.rs`：定时 loop、节流、错误保护  
- `scanner.rs`：候选 session 扫描、pending 消息查询  
- `trigger.rs`：轮数/长度判定  
- `executor.rs`：调用总结 AI、事务写库  
- `config.rs`：读取用户级配置与默认配置  
- `repo.rs`：本模块专用仓储（表查询/更新）  
- `types.rs`：内部 DTO/状态枚举  

> 关键点：不放进 `services/v2` / `services/v3`，避免和现有请求内总结混在一起。

## 2.2 启动方式

- 在 `main.rs` 中单独 `tokio::spawn` 启动 `session_summary_job::start(...)`。  
- 该任务自身可开关（如 `SESSION_SUMMARY_JOB_ENABLED`）。  

---

## 3) AI 请求复用策略（按你要求重点优化）

## 3.1 现状

`services/ai_prompt_tool.rs::run_text_prompt` 已被 `system_context_ai` 复用，但当前局限：

- 仅走 v2 `chat/completions` 请求链路。  
- 无法统一承接 Responses 与代理兼容策略。  

## 3.2 改造目标

把它升级成统一 AI 文本调用器（保留原函数，内部重定向）：

建议新增：

`services/llm_prompt_runner.rs`

对外统一接口（示意）：

- `run_text_prompt_with_runtime(...)`
- `run_text_prompt_with_model_config(...)`

内部能力：

1. 基于模型配置选择传输（chat-completions / responses）。  
2. 统一 provider/base_url/api_key 解析（复用 `core::ai_model_config`）。  
3. 统一代理兼容（例如 system message 不允许时自动降级改写）。  

然后：

- `system_context_ai` 改为调用该 runner（兼容保留旧 API）。  
- `session_summary_job` 也用该 runner 发总结请求。  

## 3.3 总结算法复用

新 job 的 AI 执行层不重写算法，直接复用：

- `services/summary/engine.rs`（触发后的 summarize/bisect 流程）
- `services/summary/token_budget.rs`（长度估算）

实现方式：

- 在 `session_summary_job` 内实现一个轻量 `JobSummaryLlmClient`（实现 `SummaryLlmClient` trait），底层调用上面的统一 runner。  

---

## 4) 数据层设计

## 4.1 总结结果表（新）

`session_summaries_v2`

字段建议：

- `id` PK  
- `session_id`  
- `summary_text`  
- `summary_model`（最终使用模型）  
- `trigger_type`（`round_limit`/`token_limit`/`manual`）  
- `source_start_message_id` / `source_end_message_id`  
- `source_message_count` / `source_estimated_tokens`  
- `status`（`done`/`failed`）  
- `error_message`  
- `created_at` / `updated_at`

## 4.2 messages 增量标记字段（新）

- `summary_status`（`pending`/`summarized`，默认 `pending`）  
- `summary_id`（关联 `session_summaries_v2.id`）  
- `summarized_at`

目的：让 job 只扫描“未总结消息”，不反复处理已总结数据。

---

## 5) 定时任务流程（独立模块内）

每轮 tick：

1. 找到有 `pending` 消息的 session。  
2. 对 session 拉取 pending 消息（时间正序）。  
3. 判定是否触发：  
   - `pending_user_turns >= round_limit`  
   - 或 `pending_estimated_tokens >= token_limit`  
4. 命中则执行总结；未命中跳过。  
5. 总结成功后事务提交：  
   - 插入 `session_summaries_v2`  
   - 本批消息置 `summarized` + 回填 `summary_id/summarized_at`  
6. 失败写失败记录，消息维持 pending，等待重试。

并发与幂等：

- 单实例先串行。  
- 多实例再加 session lease（防止重复总结）。  

---

## 6) 专门配置页面（不是混在 UserSettings 里）

## 6.1 后端配置表

新增：`session_summary_job_configs`（用户级）

字段建议：

- `user_id` PK  
- `enabled`  
- `summary_model_config_id`（推荐存模型配置ID，不直接写死 model string）  
- `token_limit`  
- `round_limit`  
- `target_summary_tokens`  
- `job_interval_seconds`  
- `updated_at`

## 6.2 配置 API（独立）

新增路由模块：`api/session_summary_job_config.rs`

- `GET /api/session-summary-job-config?user_id=...`
- `PUT /api/session-summary-job-config`
- `PATCH /api/session-summary-job-config`
- `GET /api/session-summary-job-config/model-options?user_id=...`

`model-options` 可复用现有 `ai_model_configs` 数据源，只返回 enabled 模型。

另外补充一个**会话总结列表 API**（给 Workbar tab 用）：

- `GET /api/sessions/:session_id/summaries?limit=20&offset=0`

返回建议：

- `items`: 总结列表（按 `created_at DESC`）
- `total`: 当前会话总结总数
- `has_summary`: 是否存在总结（布尔）

## 6.3 前端专用页面

新增组件：`chat_app/src/components/SessionSummaryJobConfigPanel.tsx`

页面项：

- 启用开关  
- 总结模型（下拉，来自 model-options）  
- 长度阈值（token）  
- 轮数阈值（user 轮次）  
- 可选：任务间隔、目标摘要长度  

入口：

- 在 `ChatInterface` 顶栏新增单独按钮（与现有“用户参数设置”分开）。  

## 6.4 Workbar 增加“会话总结”Tab（你新增要求）

现状 `TaskWorkbar` 只展示任务。  
改造后：

- 当当前会话 `has_summary=true` 时，Workbar 显示两个 tab：  
  - `任务`（原有）  
  - `会话总结`（新增）
- 当没有总结时，保持现状，仅显示任务，不展示“会话总结”tab。

前端改造建议：

- 组件：`chat_app/src/components/TaskWorkbar.tsx`
  - 新增 `activeTab` 状态：`tasks | summaries`
  - 新增 props：`summaries`、`summariesLoading`、`summariesError`、`onRefreshSummaries`
- 页面：`chat_app/src/components/ChatInterface.tsx`
  - 增加 summary 列表状态与加载函数
  - 在 session 变化时拉取 `/api/sessions/:session_id/summaries`

“会话总结”tab 展示字段建议：

- `created_at`（总结时间）
- `summary_model`（使用模型）
- `trigger_type`（触发原因）
- `source_message_count` / `source_estimated_tokens`
- `summary_text`（默认折叠，支持展开全文）

---

## 7) 与现有代码关系（避免“混在一起”）

1. 新定时总结代码全在 `modules/session_summary_job`。  
2. 旧 `services/v2|v3` 内联总结逻辑不改动（先并行存在，后续再决定迁移/下线）。  
3. 共用层只复用：  
   - `services/summary/*` 算法  
   - 升级后的 `llm_prompt_runner` 请求工具  

---

## 8) 分阶段落地

### Phase A（抽离复用层）

- 新建 `llm_prompt_runner`  
- `ai_prompt_tool` 改为薄封装到新 runner  

### Phase B（数据层）

- 新表：`session_summaries_v2`、`session_summary_job_configs`  
- `messages` 新增总结状态字段  

### Phase C（独立 job）

- 完成 `modules/session_summary_job/*`  
- 先 dry-run（只打日志），再写库

### Phase D（专用配置页）

- 后端配置 API  
- 前端配置面板与入口按钮  

### Phase E（Workbar 会话总结 Tab）

- 后端增加 session summaries 列表 API
- 前端 `TaskWorkbar` 增加 “任务/会话总结” tab
- 仅在 `has_summary=true` 时显示会话总结 tab

---

## 9) 验收标准

1. 定时总结逻辑代码与现有 v2/v3 目录完全分离。  
2. system_prompt 生成与定时总结共用同一 AI 文本请求工具。  
3. 用户可在专用页面配置“模型 + 长度 + 轮数”，并立即作用于下一轮任务。  
4. 每次总结都有清晰落库记录，且参与消息被正确标记为已总结。  
5. 当前会话存在总结时，Workbar 可切换到“会话总结”tab 查看总结列表。  

---

如果你认可，我下一步直接出“按文件级别的实施清单（精确到新增/修改文件）”，再开始逐步落地。
