# Task Manager MCP 子任务生命周期根因分析与修复实现

> 分析日期：2026-07-24
> 分析范围：Task Runner 默认挂载的 `Task Manager (Builtin)`、MongoDB 历史任务与运行事件、父任务完成门禁、失败/取消/重试链路。
> 数据快照：MongoDB `task_runner_service`，共 409 个任务、129 次运行、14,791 条运行事件。
> 实施状态：已完成代码实现与核心回归测试。本文第 2～4 节保留修复前证据，第 5～11 节保留设计推导，第 12 节为当前实际落地结果。
> 实现选择：没有再建立一张重复的 Task Session 表，而是直接以 `run.id` 作为权威 `task_session_id`；run 本身就是会话记录，避免 session 与 run 出现双写漂移。

## 1. 结论摘要

这不是一个单纯的“AI 偶尔忘记调用 `complete_task`”问题，而是一个确定性的生命周期设计缺口：

1. 系统鼓励 AI 在复杂工作开始时主动创建子任务，并且 Task Runner 配置为无需用户确认、立即持久化创建。
2. 这些子任务既被当作 AI 的运行内检查清单，又被当作长期存在的 Task Runner 业务任务，两种语义共用同一套状态和存储。
3. 子任务创建后没有与本次 run 绑定的完整生命周期协议；正常结束、失败、取消、超时、worker claim 过期时，都没有统一的自动收口。
4. 父任务直到模型准备结束时才检查子任务，而且当前门禁检查父任务名下的全部历史子任务，不区分是哪一次 run 创建或接管的。
5. 门禁只会再次用自然语言要求模型处理，最多重复 3 次；状态一致性仍然依赖模型自己记得逐项调用工具。
6. 状态契约本身存在直接冲突：
   - Task Manager 把 `cancelled`、`archived` 映射为 `done`，但父任务门禁只接受数据库状态 `succeeded`；AI 看见“已完成”的任务仍可能阻塞父任务。
   - 门禁提示允许把任务标记为 `blocked`，但 `blocked` 仍被门禁视为未完成；AI 按提示正确声明阻塞后，父任务仍无法结束。
7. 重试会创建新一批子任务，旧 run 的任务仍挂在同一父任务下并继续参与门禁，因此重复运行会不断累积污染。

因此，根因应定义为：**Task Manager 的“任务创建”有持久化副作用，但系统没有提供同等级别的运行级所有权、终态语义、异常清理、重试接管和原子化收口机制。**

推荐的核心修复不是继续加强提示词，而是引入 **Run-scoped Task Session（运行级任务会话）**，把 AI 的运行内检查清单与真正需要长期存在、可独立执行的 Task Runner 任务分开；父任务门禁只检查当前有效会话，并由程序负责在所有终止路径上完成一致性收口。

---

## 2. 当前真实执行链路

当前链路可以概括为：

```text
Task Runner 启动父任务 run
        |
        v
系统 Prompt 鼓励复杂工作尽早 task_manager_add_task
        |
        v
auto_create_task=true，子任务立即写入 tasks 集合
        |
        v
AI 自己调用 update_task / complete_task / delete_task 维护状态
        |
        v
模型准备输出最终答案
        |
        v
查询父任务名下所有 status != succeeded 的子任务
        |
   +----+----+
   |         |
没有       仍存在
   |         |
父任务成功  追加自然语言门禁提示并重新调用模型
             |
             v
          最多 3 次
             |
             v
      仍未全部 succeeded -> 父任务 failed
```

关键代码证据：

- `BUILTIN_MCP_PROMPT.zh-CN.md:52-76`
  - 明确鼓励复杂工作尽早创建任务，并要求 AI 自己及时更新、完成或删除。
  - 第 74 行声称 `task_manager_add_task` 自带用户确认流程。
- `task_runner_service/backend/src/services/builtin_providers/builders.rs:44-57`
  - Task Runner 实际设置 `auto_create_task: true`，与“用户确认”语义不一致。
- `task_runner_service/backend/src/services/task_manager_bridge/task_ops.rs:74-110`
  - 创建的子任务直接写入正式 `TaskRecord`，并记录 `parent_task_id` 和 `source_run_id`。
- `task_runner_service/backend/src/services/task_manager_bridge/store_adapter.rs:48-68`
  - Task Runner 的 review 流程实际直接返回 `confirmed: true`、`auto_created: true`。
- `task_runner_service/backend/src/services.rs:250-265`
  - 父任务的未完成查询只按 `parent_task_id` 查询，并把所有非 `Succeeded` 状态都视为未完成；没有使用已经存在的 `source_run_id`。
- `task_runner_service/backend/src/services/run_model_phase/callbacks/execution.rs:87-166`
  - 只有模型准备结束时才触发门禁；最多 3 次自然语言 follow-up，之后直接把父运行标记失败。
- `task_runner_service/backend/src/services/run_model_phase/callbacks/execution.rs:221-235`
  - 门禁提示允许标记 `blocked`，但代码门禁只接受 `Succeeded`。
- `task_runner_service/backend/src/services/task_manager_bridge/support.rs:113-128`
  - Task Manager 将 `Cancelled/Archived` 映射为 `done`，但完成门禁仍会把它们视为未完成。
- `task_runner_service/backend/src/services/run_model_phase/completion.rs:92-119`
  - 运行终止时会更新父任务和清理终端，但不会收口本 run 创建的 Task Manager 子任务。

这里最关键的一点是：代码其实已经保存了 `source_run_id`，但最重要的父任务完成检查没有使用它。系统拥有运行级归属数据，却仍然以父任务下的全量历史子任务做门禁。

---

## 3. 历史数据结论

### 3.1 总体数据

2026-07-24 的数据库快照如下：

| 指标 | 数量 |
|---|---:|
| 任务总数 | 409 |
| 运行总数 | 129 |
| 运行事件总数 | 14,791 |
| Task Manager 子任务 | 238 |
| 当前未完成子任务 | 45 |
| 创建过子任务的运行 | 69 |
| 当前仍有遗留子任务的创建运行 | 15 |
| 失败或取消且曾创建子任务的运行 | 16 |
| 失败或取消后仍遗留子任务的运行 | 13 |

换算后：

- 当前有 `45 / 238 = 18.9%` 的 Task Manager 子任务未闭环。
- 创建过子任务的 run 中，有 `15 / 69 = 21.7%` 目前仍有遗留子任务。
- 对失败或取消的 run，遗留比例为 `13 / 16 = 81.25%`。
- 53 个最终成功且创建过子任务的 run，目前都完成了自己创建的子任务。说明模型在正常、可完成的路径上经常能维护状态，但系统在异常路径和不可满足路径上几乎没有兜底。

当前 45 个未完成子任务的状态为：

| 状态 | 数量 |
|---|---:|
| `running` | 18 |
| `ready` | 22 |
| `blocked` | 5 |

`blocked` 任务仍在完成门禁的阻塞集合内，这与门禁提示允许 AI 声明阻塞的文案相冲突。

### 3.2 完成门禁数据

历史上共出现：

| 指标 | 数量 |
|---|---:|
| `completion_gate` 事件 | 24 |
| 触发过门禁的运行 | 20 |
| 门禁后成功 | 13 |
| 门禁后失败 | 5 |
| 门禁后取消 | 2 |
| 多次触发门禁的运行 | 2 |
| 多次门禁期间未完成 ID 集合完全不变 | 2 |

这说明自然语言提醒不是完全无效：当任务已经实际完成，只差补状态时，模型通常可以在一次提醒后收口。但它不具备一致性保证；在两个连续触发三次门禁的样本里，未完成任务集合始终没有变化。

### 3.3 样本 A：父任务明确因为未关闭子任务失败

- 父任务 ID：`fa53afe2-3e39-43bd-9a78-400c9c3f1c3a`
- 标题：`实现 OMS 订单录入、审核与内联建档流程`
- run ID：`dddf80d8-bc58-43d6-9564-b57bdd517b59`

运行行为：

1. AI 一开始创建了 4 个子任务。
2. 只完成了“核查仓库现状与需求基线”。
3. 其余任务保持 2 个 `running`、1 个 `ready`。
4. 完成门禁在 `02:32:26`、`02:41:38`、`02:49:48` 连续触发 3 次。
5. 三次门禁的未完成任务集合完全一致。
6. 最终错误明确为：

```text
父任务暂不能完成，已连续 3 次要求继续处理子任务但仍未完成
```

工具记录显示，AI 并非完全不知道这些任务存在：它多次调用了 `task_manager_list_tasks`，也更新过两个任务为 `doing`。真正的问题是任务本身没有完成，AI 又没有一个能让父任务以“受阻/部分完成”合理结束的终态协议。它既不能诚实地把未完成工作标记成功，把任务标记 `blocked` 又仍会被门禁卡住。

这个样本直接证明：**门禁把“工作没做完”和“任务状态忘记更新”混成同一种失败，并试图用同一段提示词解决两种完全不同的问题。**

### 3.4 样本 B：重试累积历史子任务

同一父任务后续重试：

- run ID：`90eb18d1-07ef-4e80-947f-60dfdeba2de6`

运行行为：

1. 前一个 run 的 3 个未完成任务仍然保留。
2. 新 run 先创建 3 个任务，后面又创建 3 个任务。
3. 当前该父任务名下共有 10 个 Task Manager 子任务，其中 9 个仍未完成，来自 2 个不同的 `source_run_id`。
4. 新 run 的完成门禁每次都看见 9 个未完成任务，而不是只检查本 run 创建或接管的任务。
5. 三次门禁期间，9 个任务的 ID 集合完全没有变化。

这不是偶发的模型遗忘，而是重试协议缺失：新 run 没有“接管旧任务 / 克隆并替代 / 放弃旧任务”的明确动作，门禁又把历史任务全部重新算入当前运行。

数据库中目前有 4 个父任务存在多个 `source_run_id` 的子任务；其中该 OMS 父任务是最严重样本。

### 3.5 样本 C：异常退出后遗留任务

以下运行都在异常终止后留下了 `ready/running` 子任务：

| run ID | 终止原因 | 遗留情况 |
|---|---|---|
| `1db10cf6-9385-42ee-87ce-d1ebd89af41a` | 网络请求失败 | 1 个 `running`、1 个 `ready` |
| `8fa36c44-e2d7-4f21-9bcc-4304fe5cd302` | 503 | 2 个 `ready` |
| `7ff21b6b-df9f-474f-82d1-3967d0591b2f` | 网络失败 | `running/ready` 遗留 |
| `9175ce67-ac20-47ec-91b9-007ef6a2dc9b` | 用户取消 | 3 个任务遗留 |
| `8be00678-3a41-4c04-bb4b-8a37ebdf4595` | 工具认证失败 | 6 个任务遗留 |
| `869715b3-901d-4577-9e75-d2e779e6c502` | 最大迭代次数 | `running` 遗留 |

父 run 终止时，系统会清理终端、沙箱等运行资源，却没有对同一 run 创建的 Task Manager 子任务做对应的生命周期清理。这是 81.25% 异常运行遗留率的直接原因。

### 3.6 状态语义冲突样本

历史 `completion_gate` 事件中至少 5 次明确列出了 `blocked` 子任务，例如：

```text
形成新需求差异与风险清单(blocked)
逐项比对需求与现有设计(blocked)
读取云端沙箱运行环境并确认可执行路径(blocked)
```

但门禁同时要求“请先完成所有子任务”。这会产生两种坏结果：

1. 模型坚持真实状态，父任务被卡住或失败。
2. 模型为了通过门禁，把本来真实受阻的任务强制标记为成功，造成“假绿”。

这比单纯遗留任务更危险，因为它会损害任务结果的可信度。

---

## 4. 根因树

### 4.1 一级根因：把运行内 checklist 和持久化任务混为一体

AI 创建的“读取代码”“执行验证”“整理交付说明”等任务，本质上通常只是本次模型运行的内部执行清单。当前实现却把它们直接保存为正式 `TaskRecord`，使它们拥有跨运行、跨重试的生命周期。

推荐明确区分两类对象：

- `run_checklist`：默认类型，仅服务当前 run，随 Task Session 收口，不独立调度。
- `durable_followup`：明确要求长期保留、以后可独立执行的 Task Runner 子任务，需要显式创建或提升。

如果不区分这两类语义，任何提示词层面的“记得完成”都只能降低概率，无法消除状态泄漏。

### 4.2 二级根因：存在创建所有权，但没有生命周期所有权

子任务已经有 `source_run_id`，可以知道由哪个 run 创建；但系统没有：

- 当前由哪个 run 负责处理；
- 是否已被后续重试接管；
- 是否已被新任务替代；
- 当前任务是否仍应阻止父任务结束；
- run 异常结束时应如何处置；
- 历史任务是否还属于当前门禁。

仅记录“谁创建了它”并不等于有完整生命周期所有权。

### 4.3 二级根因：完成门禁检查范围错误

当前门禁只按 `parent_task_id` 查询全部子任务，忽略：

- 当前 `run_id`；
- 任务是否已被 supersede；
- 任务是否属于 durable follow-up；
- 任务是否是可选项；
- blocked 是否为终态；
- 任务是否由历史失败 run 遗留。

因此只要一个历史子任务不是 `succeeded`，未来所有重试都可能被它污染。

### 4.4 二级根因：终态定义自相矛盾

当前至少有三套不同语义：

1. Task Manager 展示层：`Succeeded/Cancelled/Archived` 都是 `done`。
2. 门禁层：只有 `Succeeded` 不阻塞。
3. Prompt 层：允许 `blocked`，但又说只有所有任务成功才能结束。

这三套定义无法同时成立。

### 4.5 二级根因：异常终止没有收口事务

网络失败、503、取消、超时、worker claim 过期、认证失败等路径会结束 run，但没有一个统一的 `finalize_task_session` 在终止前后执行。

模型不可能在连接已经中断后再调用 `complete_task`。把这种遗留归因于“AI 忘记”是不准确的；系统必须为非模型终止路径负责。

### 4.6 二级根因：重试没有接管和去重协议

重试时 Prompt 仍鼓励模型尽早创建任务，`add_task` 也没有 run 内幂等键、标题/目标指纹或“优先接管已有任务”的约束，因此模型自然会重新建立一套 checklist。

旧任务不失效，新任务继续增加，完成门禁再把两批任务一起检查，最终形成累积故障。

### 4.7 三级根因：门禁发生太晚且反馈过于粗糙

门禁直到模型准备最终回答后才运行，此时：

- 可能已经没有足够上下文、迭代预算或真实执行条件；
- 它不知道任务是“工作已完成但漏更新”，还是“工作实际没完成”；
- 它只返回任务标题和状态，没有结构化缺口、最近证据或推荐处置方式；
- 如果任务集合没有变化，仍然重复同样流程，浪费模型调用和执行时间。

两个失败样本分别连续经历了三轮完整 follow-up，未完成 ID 集合始终不变，说明这里需要程序化进展检测，而不是继续重复相同提示。

---

## 5. 推荐目标模型：Run-scoped Task Session

### 5.1 核心对象

为每次父任务 run 建立一个 Task Session：

```text
TaskSession
- id
- root_task_id
- owner_run_id
- retry_of_session_id
- generation
- state: active | reconciling | succeeded | blocked | aborted
- created_at
- finalized_at
```

Task Manager 创建的条目增加或明确以下字段：

```text
TaskEntry
- source_run_id              # 已存在：最初由哪个 run 创建
- active_session_id          # 当前由哪个 Task Session 负责
- scope                      # run_checklist | durable_followup
- required_for_parent_completion
- closure_state              # open | satisfied | blocked_terminal |
                             # cancelled | superseded | waived | orphaned
- superseded_by_run_id
- closure_reason
- idempotency_key / semantic_fingerprint
```

`TaskStatus` 可以继续表达业务执行状态；`closure_state` 专门回答“它是否还应该阻止当前父运行结束”。不要再用一个 `status != succeeded` 同时承担所有语义。

### 5.2 建议的阻塞判定

父运行是否能结束，应由统一函数决定：

```text
blocks_parent_completion(task, current_session):
  task.active_session_id == current_session.id
  AND task.required_for_parent_completion == true
  AND task.closure_state == open
```

终态建议：

| closure_state | 是否阻止当前父 run 成功 | 父 run 建议结果 |
|---|---|---|
| `open` | 是 | 继续执行或进入 reconciliation |
| `satisfied` | 否 | 可成功 |
| `waived` | 否 | 可成功，但必须记录原因和操作者 |
| `superseded` | 否 | 可成功，由替代任务负责 |
| `cancelled` | 否 | 根据是否 required 决定父任务取消或部分完成 |
| `blocked_terminal` | 不应继续等待 | 父 run 应为 `blocked`，不是“因忘记关任务而 failed” |
| `orphaned` | 不阻止未来 run | 仅用于异常终止后的历史保留 |

这里需要明确一个产品语义：**父任务成功要求所有 required 子任务 `satisfied/waived/superseded`；如果存在 required 的 `blocked_terminal`，父任务进入 blocked；如果 run 被取消，其 open 子任务进入 cancelled/orphaned。**

### 5.3 正常完成收口

建议增加程序化 reconciliation：

1. 模型准备结束前，系统获取当前 Task Session 的 open 条目。
2. 返回结构化清单，而不是只有一段自然语言：任务 ID、标题、状态、最后更新时间、最近 outcome、缺失的完成条件。
3. 模型使用批量工具一次性提交最终处置：
   - `satisfied`
   - `blocked_terminal`
   - `superseded`
   - `waived`
4. 服务端在一个事务/一致性操作中校验并保存。
5. 仍有 open required 条目时才继续模型执行。
6. 如果前后两次 open 集合和版本完全相同，判定“无进展”，不要第三次重复相同提示；根据真实状态结束为 blocked 或给出明确缺口。

不建议系统仅凭模型最终自然语言自动把任务标记成功。自动成功必须依赖明确、结构化、可审计的工具结果或产物证据，否则会制造假绿。

### 5.4 异常结束收口

在所有终止路径统一调用：

```text
finalize_task_session(run_id, terminal_reason)
```

建议规则：

- `failed`、网络错误、503、认证失败、超时、worker claim 过期：
  - 本 session 的 `open` 条目标记为 `orphaned` 或 `cancelled`；
  - 保留 outcome、产物引用和历史，不物理删除；
  - 不再阻止未来重试。
- 用户取消：
  - 本 session 的 run checklist 标记 `cancelled`；
  - durable follow-up 是否保留由其策略决定。
- 服务重启恢复：
  - recovery 在恢复 run 终态时同时恢复 Task Session 终态。

终端、沙箱、Harness 分支和 Task Session 应属于同一个终止清理协议，不能只清理计算资源而遗漏任务状态。

### 5.5 重试协议

重试开始时先执行 reconciliation，而不是立即让 AI 再次 `add_task`：

1. 找到上一个 session 的非成功条目。
2. 对每个条目选择：
   - `adopt`：新 session 接管并继续；
   - `clone_and_supersede`：复制有效目标，旧条目标记 superseded；
   - `abandon`：纯临时、已失效条目标记 orphaned/cancelled；
   - `preserve_durable`：真正的长期 follow-up 保留，但不自动纳入当前门禁。
3. 新 session 默认只对接管或自己创建的条目负责。
4. `add_task` 支持 `idempotency_key` 或语义指纹，同一 session 内相同标题、目标和父任务不能重复创建。

### 5.6 运行内清单与长期任务分流

推荐把 Task Manager MCP 的默认行为改为：

- 默认 `scope=run_checklist`。
- 只有用户明确要求创建后续任务，或模型显式设置 `scope=durable_followup` 时，才建立可跨 run 持久存在的 Task Runner 任务。
- durable follow-up 永远不阻止当前父任务成功；即使模型错误传入 `required_for_parent_completion=true`，服务端也会规范化为 `false`。

这样既保留 Task Manager 对复杂工作的价值，也不会让 AI 的内部计划永久污染正式任务树。

---

## 6. 工具契约与 Prompt 调整

### 6.1 推荐新增/调整的工具

```text
task_manager_add_task
  - 新增 scope
  - 新增 required_for_parent_completion
  - 新增 idempotency_key
  - 返回 active_session_id 和 closure_required

task_manager_list_open_tasks
  - 默认 current_session_only=true
  - 返回结构化 closure_state 和缺失条件

task_manager_close_tasks
  - 批量、原子地关闭多个任务
  - 每个任务必须提供 closure_state 和 reason/evidence

task_manager_reconcile_tasks
  - 用于 retry 时 adopt/supersede/abandon

task_manager_finalize_session
  - 尝试结束当前 Task Session
  - 服务端返回 remaining_open_tasks 或最终 session 状态
```

短期不一定要一次新增全部工具，但至少应提供批量收口能力。当前逐个 `complete_task` 在任务较多、上下文较长时很容易漏项。

### 6.2 `add_task` 返回值必须明确副作用

当前工具创建后应明确返回：

```text
这些任务已持久化并属于当前 run 的 Task Session。
当前 run 结束前，每个 required 任务必须被标记为 satisfied、
blocked_terminal、superseded、waived 或 cancelled；不能保持 open。
```

### 6.3 修正 Prompt 矛盾

必须删除或修改“`task_manager_add_task` 自带用户确认流程”的描述，因为 Task Runner 当前是 `auto_create_task=true`。

Prompt 还应增加：

- 优先复用或接管当前 session 已有任务，不重复创建同义任务。
- 单步、短任务不要创建 Task Manager 条目。
- 每个任务创建时必须给出完成条件和失败/阻塞时的关闭策略。
- 在最终答复前调用 session finalize，而不是依赖模型记忆逐项检查。
- `blocked_terminal` 会让父任务进入 blocked；不要把真实阻塞强行标记成功。

Prompt 只能作为行为指导，不能承担状态一致性职责。

---

## 7. 分阶段实施建议

### Phase 0：止血修复（优先级 P0）

目标：先阻止历史任务污染、异常遗留和状态契约冲突继续扩大。

1. 完成门禁增加 `run_id/session` 过滤。
   - 最小改法：默认只检查 `source_run_id == current_run_id` 的子任务。
   - 如果重试需要接管旧任务，通过显式 adopted ID 列表加入，不再全量扫描父任务历史。
2. 抽出统一的 `blocks_parent_completion`，禁止各层自行定义“done”。
3. 修正 `blocked/cancelled/archived` 语义冲突。
   - required 的 terminal blocked -> 父 run `blocked`。
   - cancelled/archived 不再在 UI 显示 done、门禁却继续阻塞。
4. 在 run 的 failed/cancelled/expired/recovered 路径，对本 run 创建的 open 子任务标记 orphaned/cancelled，避免未来重试被污染。
5. 修正 Prompt 的“用户确认”错误描述，并降低默认建任务倾向。
6. 完成门禁增加进展签名；连续一次无变化后返回结构化 blocked/remaining 信息，不再机械重复三次。
7. `list_tasks` 在 Task Runner 场景默认 `current_turn_only=true`，需要历史任务时显式请求。

Phase 0 可以先复用现有 `source_run_id`，不必等待完整 Task Session 表落地。

### Phase 1：完整 Task Session（优先级 P1）

1. 新增 Task Session 模型和存储。
2. 增加 `active_session_id`、`scope`、`closure_state`、`required_for_parent_completion`。
3. 增加批量 `close_tasks/reconcile_tasks/finalize_session`。
4. 正常完成、失败、取消、恢复全部接入统一 finalizer。
5. 父任务完成门禁改为查询当前 session 的 blocking entries。
6. 给 run event 增加：
   - `task_session_created`
   - `task_session_reconciled`
   - `task_session_finalized`
   - `task_session_orphaned`

### Phase 2：重试、去重与产品化（优先级 P2）

1. 重试前提供 adopt/clone/supersede/abandon 策略。
2. 增加 session 内幂等键和语义去重。
3. UI 区分：
   - 本次运行清单；
   - 长期后续任务；
   - 历史 run 遗留/已替代任务。
4. 在运行详情展示 Task Session 的创建、状态变化和收口原因。
5. 增加指标与告警：
   - terminal run 后仍有 open session entries；
   - completion gate 无进展；
   - 同一父任务跨 run 重复创建同义任务；
   - orphaned 比例；
   - forced success/waived 比例。

---

## 8. 数据迁移与兼容策略

现有数据不应直接物理删除，因为其中包含过程证据和历史 outcome。

建议迁移规则：

1. `source_run_id` 已存在，直接作为历史创建所有权依据。
2. 对 `status=succeeded` 的历史子任务：
   - `closure_state=satisfied`。
3. 对 source run 仍在运行的非成功任务：
   - `closure_state=open`，绑定当前 session。
4. 对 source run 已 `failed/cancelled/blocked` 的非成功任务：
   - 默认 `closure_state=orphaned`；
   - 保留原 `status`、blocker 和 outcome。
5. 对 source run 已成功但子任务仍非成功的异常数据：
   - 标记为 `legacy_inconsistent`，不自动认定成功；
   - 不阻塞未来新 run，进入人工或后台 reconciliation 列表。
6. 对没有 `source_run_id` 的旧子任务：
   - 作为 `durable_followup` 保留；
   - 默认不参与新 run 门禁，除非被显式接管。
7. 增加组合索引：
   - `(active_session_id, closure_state)`
   - `(parent_task_id, active_session_id)`
   - `(source_run_id, closure_state)`

迁移过程必须可重复执行，并输出迁移前后各状态数量，确保不会误删或误判成功。

---

## 9. 测试与验收标准

### 9.1 必测场景

1. 正常完成
   - run 创建 3 个 required checklist；全部 satisfied 后父任务成功。
2. 漏更新但已有明确证据
   - reconciliation 返回明确缺口；模型批量收口后成功。
3. 真实阻塞
   - 子任务进入 `blocked_terminal`；父 run 进入 `blocked`，不应报“忘记完成任务”的 failed。
4. 用户取消
   - 当前 session 的 open checklist 自动 cancelled/orphaned；未来 retry 不被其阻塞。
5. 网络失败、503、认证失败、执行超时、最大迭代、worker claim 过期
   - 每条路径结束后都不存在仍会阻止未来 run 的 open 条目。
6. 重试
   - 旧任务必须被 adopt、supersede 或 abandon；不能静默叠加一批同义任务。
7. 历史任务隔离
   - 当前 run 门禁不扫描未接管的历史 `source_run_id`。
8. 状态一致性
   - UI、Task Manager 工具、门禁对 succeeded/blocked/cancelled/archived 的终态定义一致。
9. 无进展检测
   - 连续 reconciliation 的 open task ID + version 未变化时，不再重复相同门禁三次。
10. 并发和幂等
   - 同一 session 重复提交 add/close 请求不会创建重复任务或重复关闭。

### 9.2 验收指标

上线后建议满足：

- terminal run 后仍处于 blocking open 的 session entry 数量：`0`。
- 失败/取消 run 的未来重试污染率：`0%`。
- 同一父任务同义子任务跨 retry 重复创建率：接近 `0%`。
- completion gate 连续无变化重复次数：最多 `1` 次。
- `blocked` 子任务被强制改成 succeeded 以通过门禁的情况：`0`。
- 所有 Task Session 终态都有明确 `finalized_at` 和 `closure_reason`。

---

## 10. 建议的具体代码落点

短期改造优先关注：

1. `task_runner_service/backend/src/services.rs`
   - 将 `unfinished_subtasks_for_task` 改为 session/run-aware 查询。
   - 新增统一 `blocks_parent_completion`。
2. `task_runner_service/backend/src/services/run_model_phase/callbacks/execution.rs`
   - 门禁使用当前 session。
   - 增加无进展检测和 blocked 终态分流。
3. `task_runner_service/backend/src/services/run_model_phase/completion.rs`
   - 在所有 terminal status 保存后调用 Task Session finalizer。
4. `task_runner_service/backend/src/services/run_recovery.rs`
   - 服务重启、claim 过期恢复时同步收口 session。
5. `task_runner_service/backend/src/services/run_control/cancellation.rs`
   - 用户取消时同步取消/孤立当前 run checklist。
6. `task_runner_service/backend/src/services/task_manager_bridge/task_ops.rs`
   - 创建时写入 scope、session、closure policy 和幂等键。
7. `task_runner_service/backend/src/services/task_manager_bridge/store_adapter.rs`
   - 工具返回中明确自动创建和生命周期责任；增加批量 reconciliation 接口。
8. `task_runner_service/backend/src/services/task_manager_bridge/support.rs`
   - 消除工具展示状态与门禁状态定义不一致。
9. `mcp/src/implementations/builtin/task_manager.rs`
   - 扩展工具 schema，优先增加 batch close 和 session finalize。
10. `BUILTIN_MCP_PROMPT.zh-CN.md` 及其他 locale
    - 修正用户确认描述、默认作用域、重试复用和终态规则。

---

## 11. 最终建议

最合理的处理顺序是：

1. **先用现有 `source_run_id` 做 P0 隔离和异常收口**，立即阻止历史任务继续污染新 run。
2. **统一终态语义**，尤其解决 `blocked` 和 `cancelled` 在工具、UI、门禁中的矛盾。
3. **建立 Task Session 和程序化 finalizer**，把状态一致性从模型记忆移到服务端。

---

## 12. 实际修复结果（2026-07-24）

### 12.1 会话与状态模型

已经在 `TaskToolState` 中落地：

- `manager_scope`: `run_checklist | durable_followup`
- `task_session_id`: 当前负责该条目的 run ID
- `required_for_parent_completion`
- `closure_state`: `open | satisfied | blocked_terminal | cancelled | superseded | waived | orphaned`
- `closure_reason`
- `idempotency_key`
- `lifecycle_updated_at`

本地客户端在 SQLite `task_board_tasks` 中落地同名生命周期字段，并以本地 `local_task_runs.id` 作为 `task_session_id`。两端使用同一套 scope、closure、门禁和 finalizer 语义，但实现与存储完全隔离：云端只读写 MongoDB/云端 run，本地只读写 SQLite/local run。

父任务门禁的权威规则现在是：只查询 `task_session_id == current_run.id`、`required_for_parent_completion == true`、`closure_state == open` 的条目。未被当前 retry 接管的历史 run 子任务不会再污染当前 run。

### 12.2 正常完成与真实阻塞

Task Runner 专用 Task Manager 新增并启用：

- `task_manager_reconcile_tasks`
  - 批量、先校验后保存当前 session 的闭环决定。
  - `satisfied` 不要求伪造 reason，但必须由模型基于真实证据选择。
  - `blocked_terminal/cancelled/superseded/waived` 必须提供 reason。
  - 禁止修改其他 run 的历史条目。
- `task_manager_finalize_session`
  - 返回 `open_required`、`open_optional`、`terminal_blocked` 以及 `can_parent_succeed`。

如果存在 `blocked_terminal`，父 run 会进入 `blocked`，不会再报成“忘记关闭子任务”的 `failed`。如果模型连续完成声明但当前 session 的 open 集合和版本完全没有变化，系统停止机械重复门禁，把遗漏关闭的 run checklist 记为 `waived` 并保留原因；不会伪造 `succeeded`。

### 12.3 所有终止路径统一收口

统一 finalizer 已接入：

- 模型正常成功、失败、取消
- run API 取消
- task API 在排队阶段取消 run
- 真正启动前取消
- 前置任务阻塞
- 执行前失败
- worker claim 心跳过期
- 服务重启恢复未完成 run
- worker 已认领 run、但根任务已消失

规则为：

- 成功 run：遗漏的 `open run_checklist -> waived`
- 取消 run：`open/blocked_terminal run_checklist -> cancelled`
- 失败 run：`open/blocked_terminal run_checklist -> orphaned`
- blocked run：保留真实 `blocked_terminal`，其余 open 条目标记 `orphaned`
- durable follow-up：从当前 session 脱离，保持独立任务，不参与当前父任务门禁

通用任务 API 的归档和取消也会同步更新 Task Manager closure，消除了“UI 已取消/归档，但门禁仍认为 open”的状态分裂。

### 12.4 重试、幂等和容量保护

显式 retry 会先把上一 run 的 `cancelled/orphaned/blocked_terminal/open` checklist 接管到新 run，并重置为 `open/ready`；随后模型若再次提交同义 `add_task`，服务端按显式 `idempotency_key` 或标题+目标语义指纹复用原条目，不再重复创建。

本地客户端当前的手工 retry 复用同一条 `local_task_runs.id` 并创建新的执行 turn，因此重试时先把同一 local run session 中的 `cancelled/orphaned/blocked_terminal/open` checklist 原子重开，再解除 `dispatch_paused` 允许本地 worker 领取。接管或主任务复位失败时，新一轮不会带着空 session 开始执行。

同一 run 最多创建 32 个 run checklist，防止模型失控制造无限任务。`list_tasks` 在 Task Runner 中默认只列当前 session，更新、完成、删除和 reconciliation 都拒绝跨 session 修改。

### 12.5 历史数据兼容

服务恢复时会幂等迁移旧 Task Manager 子任务：

- 历史 succeeded -> `satisfied`
- 所属 run 仍 queued/running -> `open`
- 所属 run 已 cancelled -> `cancelled`
- 其他已终止或缺失 run -> `orphaned`

迁移只处理带 `parent_task_id + source_run_id` 且尚无 closure 的旧数据，不会把普通项目子任务误分类为 Task Manager checklist。MongoDB 已增加 session/closure 与 parent/session 组合索引。

### 12.6 Prompt 与共享宿主兼容

中英文 Builtin MCP Prompt 已删除“`add_task` 一定自带用户确认”的错误描述，并明确：

- 单步工作不要建任务
- `add_task` 有持久化副作用
- Task Runner 默认创建 run checklist
- durable follow-up 不阻塞当前 run
- 最终答复前 reconcile + finalize
- 真实阻塞使用 `blocked_terminal`，禁止假绿

生命周期工具只在 Task Runner host 启用：云端 Task Runner 与本地客户端 Task Runner 都会收到 reconcile/finalize 工具并执行同一套 run-scoped 生命周期；云端普通聊天和本地普通聊天仍只暴露原有 5 个 Task Manager 工具，不会被错误要求按 run closure 收口。

本地 Task Runner 的 provider 由 `SystemAgentKey::TaskRunnerPlanPhase | TaskRunnerRunPhase` 显式选择生命周期模式，`task_session_id` 来自本地 run ID。普通本地聊天即使使用 Task Manager，也不会误开启 lifecycle tools。所有本地终态路径——成功、失败、用户取消、排队取消、worker 异常、客户端重启恢复和手工 retry——都只调用 SQLite finalizer，不会访问云端 Task Runner 存储。

### 12.7 已覆盖的回归测试

已增加并通过以下场景：

1. 当前 run 隔离历史 run 的 open checklist。
2. `blocked_terminal` 阻止父任务成功，并把父 run 变为 blocked。
3. 连续无进展只 waive 当前 run 的 required checklist。
4. failed/cancelled 会收口 open 和 terminal blocker。
5. retry 接管旧 checklist，重复 `add_task` 复用原 ID。
6. durable follow-up 永不阻止父任务，并在 run 结束后脱离 session。
7. 通用 archive/cancel API 与 closure 状态保持一致。
8. Task Manager lifecycle tools 仅在启用的 host 暴露。
9. 未显式关闭的 checklist 在成功 run 中变为 waived，而不是伪造 succeeded。
10. 普通非 Task Manager 子任务仍保持原有父子完成门禁，不受 session 规则误伤。
11. 本地 Task Runner 暴露 7 个工具（含 reconcile/finalize），普通本地聊天仍为原有 5 个工具。
12. 本地 run session 拒绝跨 run 更新/完成/删除，历史 checklist 不会污染当前 run。
13. 本地成功 run 自动将模型漏关的 checklist 标为 waived；durable follow-up 脱离 session。
14. 本地失败/阻塞/取消/重启恢复执行程序化收口，手工 retry 在 worker 可领取前重开旧 checklist。
15. 本地按显式幂等键/语义指纹复用任务，并对 32 条容量上限做事务回滚。

验证命令与结果：

- `cargo test -p chatos_mcp`：109 项通过
- `cargo test -p chatos_mcp_runtime`：66 项通过
- `cargo test -p task_runner_service_backend --lib`：255 项通过
- `cargo test -p local_connector_client_core --lib`：259 项通过，2 项需预构建 sandbox 二进制的测试按既有条件忽略
- `cargo test -p chat_app_server_rs core::builtin_mcp_prompt::tests --lib`：9 项通过
- `cargo check -p chat_app_server_rs -p local_connector_client_core`：通过

一句话概括：**AI 可以负责决定任务内容和提交任务结果，但系统必须负责任务归属、终态定义、异常清理、重试接管和父子一致性。**
