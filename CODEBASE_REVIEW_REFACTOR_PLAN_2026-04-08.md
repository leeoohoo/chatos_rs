# 代码库巡检与重构方案（2026-04-08）

## 1. 本次检查范围与方法

- 范围：`agent_orchestrator`、`agent_workspace`、`memory_server`、`im_service`、`contact_task_service`、`openai-codex-gateway`
- 方法：
  - 代码文件规模统计（按 `.rs/.ts/.tsx/.py`）
  - 结构与命名重复扫描
  - 完全重复文件（内容哈希）扫描
  - 关键大文件抽样阅读（职责边界、重复逻辑、可拆分性）

---

## 2. 总体结论（先给结论）

- 架构层面：**服务边界总体合理**（Orchestrator / Memory / IM / Task / Workspace / Gateway 职责清晰）。
- 工程层面：存在明显的**局部“巨石文件”**和**跨服务重复实现**，尤其集中在：
  - `contact_task_service`（后端 + 前端）
  - `openai-codex-gateway/server.py`
  - `agent_workspace/src/lib/store/createChatStoreWithBackend.ts`
- 维护风险最高的问题不是“功能缺失”，而是：
  - 变更成本高（同一逻辑多处改）
  - 回归风险高（缺少模块边界与统一抽象）
  - 测试覆盖不均（核心改动点测试不足）

---

## 3. 关键事实与发现

## 3.1 大文件热点（建议优先拆分）

- `openai-codex-gateway/server.py`：2439 行
- `contact_task_service/frontend/src/App.tsx`：2050 行
- `contact_task_service/backend/src/repository.rs`：1812 行
- `contact_task_service/backend/src/api.rs`：1559 行
- `agent_orchestrator/src/services/task_execution_runner.rs`：1473 行
- `agent_workspace/src/lib/store/createChatStoreWithBackend.ts`：1316 行
- `agent_orchestrator/src/services/task_manager/store/create_ops.rs`：1178 行
- `agent_orchestrator/src/services/v3/ai_request_handler/parser.rs`：1119 行

说明：`contact_task_service` 仅 13 个代码文件，但平均行数高，已形成明显“单文件承载多职责”。

## 3.2 完全重复代码（可立即抽象）

- `im_service/backend/src/api/shared/auth_token.rs`
- `memory_server/backend/src/api/shared/auth_token.rs`

上述两文件内容一致（1:1 复制）。

- `contact_task_service/frontend/src/main.tsx`
- `memory_server/frontend/src/main.tsx`

上述两文件内容一致（1:1 复制）。

- `contact_task_service/frontend/src/vite-env.d.ts`
- `memory_server/frontend/src/vite-env.d.ts`

上述两文件内容一致（1:1 复制）。

## 3.3 高相似重复（建议归并）

### A) `contact_task_service/backend/src/api.rs`：public/internal handler 成对重复

典型成对函数（public/internal）：

- `create_task` / `internal_create_task`
- `list_tasks` / `internal_list_tasks`
- `get_task_plan` / `internal_get_task_plan`
- `update_task` / `internal_update_task`
- `confirm_task` / `internal_confirm_task`
- `request_pause_task` / `internal_request_pause_task`
- `request_stop_task` / `internal_request_stop_task`
- `resume_task` / `internal_resume_task`
- `retry_task` / `internal_retry_task`
- `scheduler_next` / `internal_scheduler_next`

特点：主体流程一致，仅权限/用户可见域判断不同。

### B) `agent_orchestrator` v2/v3 双栈重复

相似度（字符级）样本：

- `services/v2/ai_server.rs` vs `services/v3/ai_server.rs`：0.814
- `services/v2/mcp_tool_execute.rs` vs `services/v3/mcp_tool_execute.rs`：0.672
- `services/v2/message_manager.rs` vs `services/v3/message_manager.rs`：0.548

结论：v3 是增强版，但大量基础流程重复，维护双栈有持续成本。

### C) `agent_workspace/src/lib/store/createChatStoreWithBackend.ts` 内部重复

同文件内出现近似重复逻辑两套：

- `upsert/removeTaskReviewPanel`（helper 一套 + action 一套）
- `upsert/removeUiPromptPanel`（helper 一套 + action 一套）

这类重复易导致状态一致性问题（一个地方修复，另一处遗漏）。

## 3.4 设计合理性评估

### 合理点

- 服务化边界清晰，核心领域分离方向正确。
- `agent_orchestrator`、`memory_server`、`im_service` 基本按 `api/services/repositories` 分层。
- 前后端目录组织总体有可读性。

### 主要风险点

- 缺少跨服务共享基础库（auth token、权限/错误响应等）。
- `contact_task_service` 与 `openai-codex-gateway` 单文件职责过重。
- v2/v3 并行导致功能迭代要“改两次”。
- 测试分布不均：`contact_task_service`、`memory_server`、`im_service` 测试较少。

---

## 4. 重构目标

- 将“重复逻辑”改为“单一实现 + 可配置差异”。
- 将超大文件拆到“单模块单职责”。
- 控制改造风险：每阶段可回滚、可验证、可并行。

---

## 5. 分阶段落地方案

## Phase 0（1-2 天）：低风险止血

1. 建立重复扫描与体量阈值门禁
- 增加脚本：重复文件扫描、>800 行告警、>1200 行阻断（先告警后阻断）。

2. 提取跨服务 `auth_token` 公共实现
- 新建共享 crate（建议：`shared/auth_core`）。
- 将 `im_service` 与 `memory_server` 的 `auth_token.rs` 改为依赖同一实现。

3. 统一前端 bootstrap 入口
- 提取 `frontend-bootstrap` 模板（或共享包）。
- 统一 `main.tsx`、`vite-env.d.ts` 的复制实现。

验收标准：
- 重复文件数量下降；
- 两服务鉴权 token 行为一致，测试通过。

## Phase 1（3-5 天）：高收益拆分 `contact_task_service`

1. 拆分 `backend/src/api.rs`（1559 行）
- 目标目录建议：
  - `api/routes/public_tasks.rs`
  - `api/routes/internal_tasks.rs`
  - `api/routes/scheduler.rs`
  - `api/handlers/common.rs`（统一响应封装、错误映射）
  - `api/auth_scope.rs`（public/internal 的差异策略）
- 关键改造：把“public/internal 成对 handler”收敛到一个业务处理函数，权限差异用策略参数表示。

2. 拆分 `backend/src/repository.rs`（1812 行）
- 目标目录建议：
  - `repository/query.rs`
  - `repository/plan_ops.rs`
  - `repository/state_transition.rs`
  - `repository/runtime_scope.rs`
  - `repository/handoff.rs`
- 关键改造：抽出任务状态迁移器（state transition helpers），避免 `confirm/pause/stop/resume/retry` 大量重复构造 `UpdateTaskRequest`。

3. 拆分 `frontend/src/App.tsx`（2050 行）
- 目标目录建议：
  - `pages/TaskDashboardPage.tsx`
  - `features/taskPlan/TaskPlanPanel.tsx`
  - `features/taskExecution/ExecutionLogPanel.tsx`
  - `features/taskResult/ResultBriefPanel.tsx`
  - `hooks/useTaskDashboardController.ts`
  - `utils/formatters.ts`
- 关键改造：UI、数据加载、业务动作分层，保持组件可测试。

验收标准：
- 三个文件均降到 < 600 行；
- 原接口行为不变（契约测试与关键手工回归通过）。

## Phase 2（3-4 天）：网关与状态仓库解耦

1. 拆分 `openai-codex-gateway/server.py`（2439 行）
- 目标目录建议：
  - `gateway/http_handler.py`
  - `gateway/streaming.py`
  - `gateway/tool_policy.py`
  - `gateway/payload_parser.py`
  - `gateway/thread_store.py`
  - `gateway/config.py`
- 关键改造：HTTP 层、流式事件组装、工具策略、payload 解析分离。

2. 拆分 `agent_workspace/src/lib/store/createChatStoreWithBackend.ts`（1316 行）
- 目标目录建议：
  - `store/ws/sessionEvents.ts`
  - `store/ws/imEvents.ts`
  - `store/panels/taskReviewPanelState.ts`
  - `store/panels/uiPromptPanelState.ts`
  - `store/bootstrap/imConversationBootstrap.ts`
- 关键改造：去掉同文件双实现（panel upsert/remove）。

验收标准：
- 两文件降到 < 500 行；
- websocket 重连与 panel 状态逻辑有单元测试覆盖。

## Phase 3（可选，按产品节奏）：v2/v3 合并路线

1. 在 `agent_orchestrator` 为 v2/v3 建能力矩阵（已支持/缺失/行为差异）。
2. 优先提炼公共基础层（tool registry、message manager core、request options）。
3. 设定 v2 退场窗口，避免长期双栈。

验收标准：
- v2/v3 共享代码比例提升；
- 新功能默认只进 v3；
- v2 仅保留兼容桥接层。

---

## 6. 模块抽象建议（优先抽哪些）

1. 跨服务共享（优先级最高）
- `auth token`、`auth identity`、标准错误响应体、基础 scope 解析。

2. 任务域共享（`contact_task_service` 内）
- `TaskTransition`（状态迁移）
- `ScopeRuntimeUpdater`
- `TaskPlanGraphOps`（依赖关系/重挂/级联跳过）

3. 前端共享
- `PanelStateUpsertRemove`（task review / ui prompt）
- 统一 `formatters`（状态色、标签、截断、tool 详情展示）。

---

## 7. 风险与控制

1. 风险：拆分期间行为回归
- 控制：先引入薄封装，不改业务语义；每次拆分仅迁移一个子域。

2. 风险：跨服务共享库引入耦合
- 控制：共享库只放“稳定底座能力”，禁止塞业务逻辑。

3. 风险：双栈合并影响线上稳定
- 控制：先做公共层提取，再做路由切换；保留灰度开关。

---

## 8. 建议执行顺序（可直接开工）

1. Phase 0（重复止血 + shared auth token）
2. Phase 1（`contact_task_service` 三大文件拆分）
3. Phase 2（gateway + store 超大文件拆分）
4. Phase 3（v2/v3 逐步收敛）

---

## 9. 预期收益

- 新功能开发改动面下降，回归测试范围收窄。
- 跨服务一致性提升（鉴权、错误语义、响应格式）。
- 大文件热点被拆后，代码评审速度和可读性明显提升。
- 后续演进（如 v2 退场、Task 能力增强）阻力降低。

