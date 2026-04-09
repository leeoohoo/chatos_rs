# 任务平台系统级排查与修复报告（2026-04-09）

## 1. 现象复现（真实数据）

在同一 scope 下出现以下矛盾：

- `pending_execute` 任务存在（可执行任务已就绪）
- `running` 任务为空
- `paused` 任务为空
- 但 `POST /api/task-service/v1/internal/scheduler/next` 仍返回 `decision=pass`

这意味着问题不在 UI，而在调度 gate 逻辑或 runtime 状态一致性。

---

## 2. 已确认并修复的问题

### 问题 A：runtime 脏状态会长期阻塞调度

**症状**
- scope 没有真实运行中任务，但 runtime 里残留 `control_request / running_task_id / control_reason` 等标记。
- 调度被错误短路，返回 `pass`，导致 `pending_execute` 不被拉起。

**修复**
- `scheduler_next` 中加入 runtime 自愈：当没有“同 scope 且真实 running”的任务时，自动清理脏 runtime 标记。
- 仅在“同 scope + running”成立时才允许 `pass`。

**代码位置**
- `contact_task_service/backend/src/repository/scheduler.rs`

---

### 问题 B：`running_task_id` 未校验 scope，存在跨 scope 误阻塞风险

**症状**
- 若 runtime 的 `running_task_id` 指向其它 scope 的任务，旧逻辑可能把当前 scope 错误判定为“有运行任务”，导致本 scope 无法调度。

**修复**
- 调度时对 `running_task_id` 所指任务增加 scope 一致性校验（`user_id/contact_agent_id/project_id` 三元组）。
- 不一致即视为脏标记并清理。

**代码位置**
- `contact_task_service/backend/src/repository/scheduler.rs`

---

### 问题 C：前置依赖判定规则不一致，导致状态漂移

**症状**
- `confirm/retry` 走的是“只要有依赖就 blocked”的简化逻辑。
- `refresh_blocked` 走的是“依赖真实状态判定”的逻辑。
- 两套规则并存，容易造成 blocked/pending_execute 在不同入口下不一致。

**修复**
- 抽成统一依赖判定函数，并在 `confirm_task / retry_task / refresh_blocked_scope_tasks` 复用：
  - 依赖全为 `completed/skipped` => `pending_execute`
  - 任一依赖 `failed/cancelled` => `blocked(upstream_terminal_failure)`
  - 依赖缺失 => `blocked(dependency_missing)`
  - 其他 => `blocked(waiting_for_dependencies)`

**代码位置**
- `contact_task_service/backend/src/repository/support.rs`
- `contact_task_service/backend/src/repository/lifecycle.rs`
- `contact_task_service/backend/src/repository.rs`

---

### 问题 D：任务离开 running 后 runtime 可能残留

**症状**
- 非 `ack_pause/ack_stop` 路径下，任务状态从 `running` 变更后，runtime 可能未同步清理。

**修复**
- 在通用 `update_task` 流程中增加兜底：若任务从 `running` 切走，且 runtime 指向该任务或存在控制标记，则自动清理 runtime 控制字段。

**代码位置**
- `contact_task_service/backend/src/repository.rs`

---

## 3. 本次改动文件

- `contact_task_service/backend/src/repository/scheduler.rs`
- `contact_task_service/backend/src/repository/support.rs`
- `contact_task_service/backend/src/repository/lifecycle.rs`
- `contact_task_service/backend/src/repository.rs`

---

## 4. 编译与测试

在 `contact_task_service/backend` 执行：

- `cargo check` ✅
- `cargo test -- --nocapture` ✅（当前仓库无单测用例，0 tests）

---

## 5. 仍建议继续补强（未在本次直接改行为）

1. 增加 scheduler 决策可观测性（decision reason + key runtime fields），便于线上快速定位 `pass` 原因。
2. 为调度/依赖判定补充集成测试（尤其是 skipped、dependency_missing、跨 scope runtime）。
3. 为 runtime 增加定期清理或版本戳，进一步降低历史脏数据影响。

