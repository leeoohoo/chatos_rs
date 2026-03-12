# Memory Server 统一记忆服务方案（架构/数据/API/页面）

## 1. 目标与范围

本方案目标：将会话、消息、总结、历史上下文组装能力统一迁移到 `memory_server`，通过标准接口提供给原服务调用；并在 `memory_server` 内独立实现 AI 总结与定时任务，配套独立配置页面与可视化前端。

### 1.1 业务目标

1. 把原项目中的会话/消息/总结存储与查询能力收敛到 `memory_server`。
2. `memory_server` 提供标准 REST API 给原服务使用。
3. 在 `memory_server` 内实现两类定时总结：
   - `L0`：消息 -> 总结
   - `L(n+1)`：总结 -> 更高层总结（rollup）
4. 在 `memory_server` 内独立实现 AI 调用（模型配置、路由、失败重试、限流）。
5. 提供独立前端，用于管理模型配置、总结任务配置、查看会话/消息/总结。
6. 提供“历史上下文组装接口”，给原项目在发 AI 请求前直接调用。

### 1.2 非目标

1. 不在首期重写原项目全部业务 API。
2. 不首期引入复杂消息队列（先用数据库 + worker）。
3. 不首期做跨数据中心部署。

---

## 2. 总体架构

## 2.1 逻辑分层

1. `memory_server_api`：对外 REST API（原服务 + 管理前端调用）。
2. `memory_core`：会话/消息/总结领域逻辑。
3. `memory_ai`：模型配置、AI Provider 适配、Prompt 执行。
4. `memory_jobs`：定时任务执行器（L0/Ln rollup）。
5. `memory_web`：管理后台前端（会话、消息、总结、配置、任务观测）。

## 2.2 部署拓扑（推荐）

1. 进程 A：`memory_server`（API + Worker 同进程，不同 tokio task）。
2. 进程 B：`memory_web`（静态站点，可由 A 托管或独立部署）。
3. 原服务：只保留业务编排，记忆读写改为调用 `memory_server`。

可选扩展：Worker 独立进程化（后期）。

## 2.3 与原服务交互

原服务对 `memory_server` 的主要调用：

1. 写入消息：用户消息、AI消息、tool消息。
2. 查询会话历史/消息列表。
3. 查询总结列表。
4. 调用“历史上下文组装接口”用于 AI 请求输入。
5. 可选：触发即时总结（手动触发接口）。

---

## 3. 数据结构设计

以下为核心集合设计（Mongo 实现）。

## 3.1 sessions

字段：
1. `id TEXT PK`
2. `user_id TEXT NOT NULL`
3. `project_id TEXT NULL`
4. `title TEXT NULL`
5. `status TEXT NOT NULL DEFAULT 'active'` (`active|archiving|archived`)
6. `created_at TEXT NOT NULL`
7. `updated_at TEXT NOT NULL`
8. `archived_at TEXT NULL`

索引：
1. `(user_id, status, created_at DESC)`
2. `(project_id, status, created_at DESC)`

## 3.2 messages

字段：
1. `id TEXT PK`
2. `session_id TEXT NOT NULL`
3. `role TEXT NOT NULL` (`system|user|assistant|tool`)
4. `content TEXT NOT NULL`
5. `message_mode TEXT NULL`
6. `message_source TEXT NULL`
7. `tool_calls TEXT NULL` (JSON)
8. `tool_call_id TEXT NULL`
9. `reasoning TEXT NULL`
10. `metadata TEXT NULL` (JSON)
11. `summary_status TEXT NOT NULL DEFAULT 'pending'` (`pending|summarized`)
12. `summary_id TEXT NULL`
13. `summarized_at TEXT NULL`
14. `created_at TEXT NOT NULL`

索引：
1. `(session_id, created_at ASC)`
2. `(session_id, summary_status, created_at ASC)`
3. `(summary_id)`

## 3.3 session_summaries_v2

字段：
1. `id TEXT PK`
2. `session_id TEXT NOT NULL`
3. `summary_text TEXT NOT NULL`
4. `summary_model TEXT NOT NULL`
5. `trigger_type TEXT NOT NULL`
6. `source_start_message_id TEXT NULL`（L0时用于消息范围）
7. `source_end_message_id TEXT NULL`
8. `source_message_count INTEGER NOT NULL DEFAULT 0`
9. `source_estimated_tokens INTEGER NOT NULL DEFAULT 0`
10. `status TEXT NOT NULL DEFAULT 'done'` (`done|failed`)
11. `error_message TEXT NULL`
12. `level INTEGER NOT NULL DEFAULT 0`
13. `rollup_status TEXT NOT NULL DEFAULT 'pending'` (`pending|summarized`)
14. `rollup_summary_id TEXT NULL`
15. `rolled_up_at TEXT NULL`
16. `created_at TEXT NOT NULL`
17. `updated_at TEXT NOT NULL`

索引：
1. `(session_id, created_at DESC)`
2. `(session_id, status, created_at DESC)`
3. `(session_id, level, status, rollup_status, created_at ASC)`
4. `(rollup_summary_id)`

## 3.4 ai_model_configs（memory_server 独立）

字段：
1. `id TEXT PK`
2. `user_id TEXT NOT NULL`
3. `name TEXT NOT NULL`
4. `provider TEXT NOT NULL` (`openai|azure_openai|anthropic|custom`)
5. `model TEXT NOT NULL`
6. `base_url TEXT NULL`
7. `api_key TEXT NULL`（建议加密存储）
8. `supports_responses INTEGER NOT NULL DEFAULT 0`
9. `temperature REAL NULL`
10. `thinking_level TEXT NULL`
11. `enabled INTEGER NOT NULL DEFAULT 1`
12. `created_at TEXT NOT NULL`
13. `updated_at TEXT NOT NULL`

索引：
1. `(user_id, enabled, updated_at DESC)`

## 3.5 summary_job_configs（L0）

字段：
1. `user_id TEXT PK`
2. `enabled INTEGER NOT NULL DEFAULT 1`
3. `summary_model_config_id TEXT NULL`
4. `token_limit INTEGER NOT NULL DEFAULT 6000`
5. `round_limit INTEGER NOT NULL DEFAULT 8`
6. `target_summary_tokens INTEGER NOT NULL DEFAULT 700`
7. `job_interval_seconds INTEGER NOT NULL DEFAULT 30`
8. `max_sessions_per_tick INTEGER NOT NULL DEFAULT 50`
9. `updated_at TEXT NOT NULL`

## 3.6 summary_rollup_job_configs（L1+）

字段：
1. `user_id TEXT PK`
2. `enabled INTEGER NOT NULL DEFAULT 1`
3. `summary_model_config_id TEXT NULL`
4. `token_limit INTEGER NOT NULL DEFAULT 6000`
5. `round_limit INTEGER NOT NULL DEFAULT 50`
6. `target_summary_tokens INTEGER NOT NULL DEFAULT 700`
7. `job_interval_seconds INTEGER NOT NULL DEFAULT 60`
8. `keep_raw_level0_count INTEGER NOT NULL DEFAULT 5`
9. `max_level INTEGER NOT NULL DEFAULT 4`
10. `max_sessions_per_tick INTEGER NOT NULL DEFAULT 50`
11. `updated_at TEXT NOT NULL`

## 3.7 job_runs（任务运行记录，便于前端观测）

字段：
1. `id TEXT PK`
2. `job_type TEXT NOT NULL` (`summary_l0|summary_rollup`)
3. `session_id TEXT NULL`
4. `status TEXT NOT NULL` (`running|done|failed|skipped`)
5. `trigger_type TEXT NULL`
6. `input_count INTEGER NOT NULL DEFAULT 0`
7. `output_count INTEGER NOT NULL DEFAULT 0`
8. `error_message TEXT NULL`
9. `started_at TEXT NOT NULL`
10. `finished_at TEXT NULL`

---

## 4. 定时总结与 AI 逻辑设计

## 4.1 L0 总结（消息 -> 总结）

流程：
1. 扫描存在 pending 消息的会话。
2. 依据配置决定是否触发（消息条数优先 + token补充触发）。
3. 选定候选消息，做单条超限剔除与说明。
4. 超限分片（递归二分）。
5. chunk 总结 + merge 总结。
6. 写入 `session_summaries_v2(level=0)`。
7. 标记消息 `summary_status=summarized`。

## 4.2 Rollup 总结（总结 -> 更高层总结）

流程：
1. 对每会话从 `level=L` 筛选 `done + rollup_status=pending`。
2. `L=0` 时排除最早 5 条原始总结。
3. 满 50 条触发一批。
4. 超 token 分片 + merge。
5. 写入 `session_summaries_v2(level=L+1)`。
6. 将源 50 条更新为 `rollup_status=summarized` 并回写 `rollup_summary_id`。

## 4.3 AI 调用独立实现

`memory_server` 内建 `PromptRunner`：
1. 根据 `summary_model_config_id` 解析 provider/model/base_url/api_key。
2. 统一执行 `run_text_prompt_with_runtime` 风格接口。
3. 增加超时、重试、并发限制、日志打点。
4. 关键日志字段：`provider/model/session_id/job_type/tokens/chunks/latency`。

## 4.4 可靠性策略

1. 幂等：同批范围重复执行时避免重复写 done 记录。
2. 失败记录落库：便于排查和重试。
3. 任务互斥：同会话同类任务避免并发。
4. 限流：按用户/全局并发控制 AI 调用。

---

## 5. API 设计（标准接口）

前缀：`/api/memory/v1`

鉴权建议：
1. 服务间接口：`X-Service-Token`
2. 用户接口：`Bearer JWT`（可复用原鉴权体系）

## 5.1 会话管理

1. `POST /sessions`
   - 创建会话
2. `GET /sessions?user_id=&status=&limit=&offset=`
   - 会话列表
3. `GET /sessions/{session_id}`
   - 会话详情
4. `PATCH /sessions/{session_id}`
   - 更新标题/状态
5. `DELETE /sessions/{session_id}`
   - 软删除或归档

## 5.2 消息管理

1. `POST /sessions/{session_id}/messages`
   - 写入一条消息
2. `POST /sessions/{session_id}/messages/batch`
   - 批量写入
3. `GET /sessions/{session_id}/messages?limit=&offset=&order=asc|desc`
   - 消息列表
4. `GET /messages/{message_id}`
   - 消息详情

## 5.3 总结查询

1. `GET /sessions/{session_id}/summaries?level=&status=&rollup_status=&limit=&offset=`
2. `GET /sessions/{session_id}/summaries/levels`
   - 返回各层级条数统计
3. `DELETE /sessions/{session_id}/summaries/{summary_id}`

## 5.4 总结任务配置

1. `GET /configs/summary-job?user_id=`
2. `PUT /configs/summary-job`
3. `GET /configs/summary-rollup-job?user_id=`
4. `PUT /configs/summary-rollup-job`

## 5.5 模型配置

1. `GET /configs/models?user_id=`
2. `POST /configs/models`
3. `PATCH /configs/models/{id}`
4. `DELETE /configs/models/{id}`
5. `POST /configs/models/{id}/test`
   - 测试模型连通性

## 5.6 任务控制与观测

1. `POST /jobs/summary/run-once`
2. `POST /jobs/summary-rollup/run-once`
3. `GET /jobs/runs?job_type=&session_id=&status=&limit=`
4. `GET /jobs/stats`

## 5.7 原服务关键接口：历史上下文组装

`POST /context/compose`

入参建议：
1. `session_id`
2. `mode` (`chat|sub_agent`)
3. `summary_limit`（默认 3）
4. `pending_limit`（可空）
5. `include_raw_messages`（bool）

返回：
1. `merged_summary`（已经处理好层级与去重）
2. `summary_count`
3. `messages`（待发送的消息序列）
4. `meta`
   - `used_levels`
   - `filtered_rollup_count`
   - `kept_raw_level0_count`

返回示例（简化）：

```json
{
  "session_id": "s_123",
  "merged_summary": "以下是最近历史会话总结...",
  "summary_count": 3,
  "messages": [
    {"id":"m1","role":"user","content":"..."},
    {"id":"m2","role":"assistant","content":"..."}
  ],
  "meta": {
    "used_levels": [2,1,0],
    "filtered_rollup_count": 48,
    "kept_raw_level0_count": 5
  }
}
```

## 5.8 Webhook（可选）

1. `POST /hooks/session-archived`
   - 通知 memory_server 做归档联动。

---

## 6. 历史上下文组装策略（接口内部算法）

目标：给原服务稳定输出“可直接送 AI”的历史信息。

策略：
1. 优先取 `rollup_status=pending` 的 done 总结。
2. 优先级：高层级 > 低层级；同层按新到旧。
3. 保证 `level=0` 最早 5 条可保留可见（视配置）。
4. 拼装 merged summary 时按时间从旧到新拼接。
5. 消息段取 `summary_status=pending` 的消息作为增量上下文。

这样原服务只关心：
1. 调用 `/context/compose`
2. 把返回内容送给 AI 客户端

---

## 7. 前端页面设计（memory_web）

## 7.1 页面结构

1. 登录页（可选，内部环境可关闭）
2. 仪表盘
3. 会话列表页
4. 会话详情页（消息 + 总结双视图）
5. 总结层级页
6. 模型配置页
7. 定时任务配置页
8. 任务运行记录页
9. 系统设置页

## 7.2 页面细节

### 仪表盘

模块：
1. 今日会话数、消息数、总结数
2. L0/Ln 任务成功率
3. 待处理积压（pending messages / pending summaries）
4. 最近失败任务列表

### 会话列表页

1. 支持 user/project/status 筛选
2. 显示会话最近消息时间、未总结消息数、最高总结层级
3. 点击进入详情

### 会话详情页

左侧：消息时间线
1. 消息 role、source、summary 状态
2. 支持按 `pending/summarized` 过滤

右侧：总结面板
1. 按 `level` 分组展示
2. 展示 `rollup_status` 与 `rollup_summary_id` 链路
3. 可展开查看源范围（message id / summary id）

### 总结层级页

1. 每个会话的层级树（L0/L1/L2...）
2. 每层统计（总数、pending、summarized）
3. 手动触发 rollup 按钮（按会话）

### 模型配置页

1. Provider/Model/API Key/Base URL 配置
2. 连通性测试按钮
3. 启用/停用切换

### 定时任务配置页

Tab1：L0 Summary
1. enabled
2. model
3. round_limit
4. token_limit
5. target_summary_tokens
6. interval

Tab2：Rollup
1. enabled
2. model
3. round_limit(50)
4. keep_raw_level0_count(5)
5. max_level
6. interval

### 任务运行记录页

1. job run 列表
2. 失败详情查看
3. 按 session/job_type/status 筛选

---

## 8. 原服务迁移方案

## 8.1 迁移原则

1. 先双写（短期）再切读。
2. 先切“历史上下文组装”，再切“消息存储读写”。
3. 提供回退开关（env feature flag）。

## 8.2 分阶段

### 阶段 A：接入 context 接口

1. 原服务在 AI 发送前改为调用 `/context/compose`。
2. 原有本地组装逻辑保留开关以便回退。

### 阶段 B：消息写入双写

1. 原服务写本地数据库 + 调用 memory_server 写消息。
2. 做数据对账，确认一致性。

### 阶段 C：消息与总结读取切换

1. 原服务会话历史、总结列表改为调用 memory_server。
2. 本地读取路径下线。

### 阶段 D：本地记忆存储下线

1. 停止本地会话/消息/总结写入。
2. 保留迁移脚本与只读回溯能力。

---

## 9. 可观测与运维

## 9.1 指标

1. API 请求量、延迟、错误率
2. L0/Ln 任务触发次数、成功率、失败率
3. 平均压缩比（源文本tokens/总结tokens）
4. pending 积压量

## 9.2 日志

统一字段：
1. `trace_id`
2. `session_id`
3. `job_type`
4. `summary_level`
5. `provider/model`
6. `latency_ms`
7. `error_code`

## 9.3 告警

1. job 失败率 > 阈值
2. pending 积压持续增长
3. AI provider 连通性失败

---

## 10. 安全与权限

1. 服务间调用使用独立 token。
2. API key 加密存储（至少 AES-GCM + 本地主密钥）。
3. 管理前端按用户权限隔离数据。
4. 审计日志记录关键配置变更。

---

## 11. 里程碑与交付

## M1：方案与骨架（3-5 天）

1. 初始化 `memory_server` 项目结构
2. 建库脚本与核心表
3. 会话/消息/总结基础 API
4. 简版前端框架（会话与消息查看）

## M2：AI 总结与任务（5-7 天）

1. L0 summary job
2. rollup summary job
3. 模型配置与任务配置 API
4. 任务运行日志 API

## M3：上下文组装与原服务对接（3-5 天）

1. `/context/compose` 完成
2. 原服务接入开关
3. 双写验证脚本

## M4：管理前端完善（4-6 天）

1. 模型配置页
2. 任务配置页
3. 总结层级页
4. 任务运行页

---

## 12. 风险与对策

1. 风险：短期双写导致一致性差异
   - 对策：写入幂等 key + 定时对账脚本 + 可回放机制。
2. 风险：AI 成本上升
   - 对策：限流、批次阈值、失败重试上限、高层级上限。
3. 风险：数据库锁争用
   - 对策：任务并发控制、分会话串行化、批量事务优化。
4. 风险：迁移窗口影响线上稳定
   - 对策：feature flag + 分批灰度。

---

## 13. 首期最小可用（MVP）建议

建议首期必须包含：

1. 数据层：sessions/messages/session_summaries_v2 + 新增 level/rollup 字段。
2. API：
   - 会话/消息读写
   - 总结查询
   - `/context/compose`
   - 模型配置 + 两类 job 配置
3. Job：
   - L0 summary
   - rollup summary（含保留 5 条）
4. 前端：
   - 会话列表 + 会话详情
   - 总结层级页
   - 模型与任务配置页

这样可以最短路径支撑原服务切换，并满足你提的“统一迁移 + 独立总结 + 可视化管理”。
