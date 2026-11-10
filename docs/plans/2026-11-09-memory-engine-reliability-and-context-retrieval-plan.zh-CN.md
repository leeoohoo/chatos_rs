# Memory Engine 可靠性、总结与上下文召回完整优化方案

> 状态：一次性全量实施定稿版；全部工作完成并通过全量验收前不得提交、推送或发布
>
> 编制日期：2026-11-09
>
> 适用范围：`memory_engine/backend`、`crates/memory_engine_sdk`、`crates/chatos_ai_runtime`、`clients/local_agent_host`、客户端可信项目上下文、Configuration Center、Memory 管理平台
>
> 文档地位：本文是唯一当前有效的 Memory Engine 实施计划；旧优化方案、整改清单和阶段进度文档已删除，不得再作为实现依据。

> 2026-11-09 二次修订：补齐数据库派生事件 Outbox、版本化 Subject Memory、删除矩阵、Runtime Context Assertion 的真实签发边界、全客户端 Source Unit 契约、模型能力快照前置条件和历史修复依赖顺序。M3a 必须先于 M3b 完成，M4a 必须先于 M4b 完成；这些编号只表达同一次改造中的工程依赖，不构成可单独发布的版本。

> 2026-11-09 三次修订：取消所有 Feature Flag、tenant/source allowlist 灰度、shadow/inject 双路径、v1/v2 双 writer、旧客户端写兼容和分批放量。实施采用停写维护窗口、全量前置校验、单一新路径原子切换；未满足前置条件时整体不发布，切换失败时保持停写并整体恢复切换前数据库与应用快照，禁止在运行时退回旧的不可靠写路径。

> 2026-11-09 四次修订：删除 Compose 旧条数参数、请求级检索模式、检索故障回退、旧 Prompt 只读兼容和配置旧 revision 沿用。M0—M7 改为一次性改造的工程工作包；所有工作包、客户端升级、历史重建和验收必须在同一个最终提交与发布窗口前全部完成，不产生任何可运行、可提交、可推送或可发布的中间态。

> 2026-11-10 五次修订：发布、replay、rearm 等机器运维修复不再依赖人工管理员密码、登录会话或临时用户 JWT。统一改为 Configuration Center 到目标服务的 mTLS 服务身份、短时请求签名、防重放请求标识和双端审计。补充“无 durable fact”正常完成语义：不得为无事实输入编造记忆，也不得因此 dead-letter 或阻塞全量重建。

> 2026-11-10 六次修订：补齐 Worker 中止、超时或终态失败后的 Record claim 自动恢复。恢复器只把所属 Summary Job 已终态、没有 active coverage 的 `summarizing` Record 原子恢复为 `pending`；运行中 Job 或已有 active coverage 的 Record 必须保持不变。不得依靠人工 SQL 清理卡死 claim。

> 2026-11-10 七次修订：父 Job 委托给 durable Cloud Agent 后，不再按父 Job 的固定启动时间误判超时。Cloud Agent 非终态或终态 operator callback 仍为 pending/publishing 时，由 Cloud Agent durable 状态独占生命周期；只有 Cloud Agent 已终结且终态 callback 不再可投递时，父 Job stale recovery 才能接管。

> 2026-11-10 八次修订：generation finalization 的 lease owner 改为可重入逻辑 owner。进程重启后，同一 Job/finalizer 可立即换取新的 fencing token 并使旧 token 失效；不同 owner 在租约到期前仍被拒绝，避免固定租约时长与有限 callback 重试预算互相制造 dead-letter。

> 2026-11-10 九次修订：Memory Engine 定向发布必须在同一次发布事务内恢复发布前的 Worker 运行状态，并在 Worker 稳定运行后才能写入 `complete/healthy`。禁止把 Worker 留在 `created/stopped` 中间态后宣告发布成功；启动失败必须触发整版回滚。

> 2026-11-10 十次修订：可重入 fencing 规则必须由仓储领取 SQL 与 PostgreSQL transition guard 同时执行。数据库只允许稳定逻辑 owner 在活动租约内按 `attempt_count + 1` 原子换取非空新 token 和未来到期时间；异 owner、空 token 或不完整换租继续失败关闭，避免应用规则与数据库规则分裂到上线后才暴露。

> 2026-11-10 十一次修订：发布器本地与服务端 Git fetch 都必须禁用交互式凭据等待，并使用低速超时和有限重试。网络停滞必须在有界时间内明确失败，禁止发布进程无限挂起或依赖人工判断。

> 2026-11-10 十二次修订：durable terminal callback 到达时若父 Summary Job 已由恢复器终结，则父 Job 状态为权威结果；回调必须幂等确认，不再尝试提交已释放的 Record/partition claim，也不得制造新的 outbox dead-letter。只有仍为 `running` 的父 Job 才允许进入 generation finalization。

> 2026-11-10 十三次修订：parent Job、durable Cloud Agent run、业务 partition/scope/covered-range claim、generation fencing 与 terminal callback 统一为一个生命周期协议。parent Job 为唯一业务所有权权威：parent 仍为 `running` 时，lease 到期不授权新 Job 接管；同一 parent 可在进程重启后恢复业务 claim 并换取 generation token；异 owner 已接管时，旧 parent 原子失败且迟到 callback 幂等确认。Thread L0、Thread Rollup、Subject Memory L0/Rollup 和 covered-range resummarization 使用同一规则，不保留按固定 lease 强抢、callback 重试补偿或人工清理旁路。

## 1. 结论

当前 Memory Engine 的总体分层可以继续使用：原始 Record 先形成 Thread L0 Summary，再派生 Thread Rollup、Project Summary 和 Agent Memory；模型请求在运行边界读取 Memory Engine，在轮内继续使用 Responses compaction 管理当前运行历史。

本轮不引入独立向量数据库，也不重写 Agent Loop。改造内部按依赖顺序先建立完整、可恢复、可验证的派生链，再完成预算控制、去重和相关性召回；该顺序不允许形成独立发布或保留旧实现并行运行。

实施顺序固定为：

1. 停止 Record、Thread Rollup、Subject Memory Rollup 的所有静默跳过。
2. 建立 Record、Summary、Subject Memory 之间的显式 coverage 和 lineage。
3. 使 Summary 提交、Job 完成、claim 释放和数据库派生事件 Outbox 写入具备事务性与重放幂等性。
4. 将 Repair 拆成 pending rebuild 与 covered-range resummarize 两种明确操作。
5. 先完成 coverage 初步回填和坏数据隔离，再让 Thread Summary 和 Thread Rollup 从生成、选择到召回都按可信项目分区，最后重建旧跨项目派生结果。
6. 先补齐 Runtime 冻结的模型能力快照，再将 compose 升级为按完整请求预算、候选池和可解释评分选择上下文。
7. 使用结构化事实、来源引用、可索引事实投影和 supersession 解决重复、过期和纠错问题。
8. 在结构化过滤稳定后增加中文可用的词法召回；embedding 只作为经评测证明有效的后续能力。

目标链路：

```text
Records
  │
  ├─ 按 Agent / Thread / Project partition 选择
  ├─ Tool exchange 原子分组
  ├─ oversized 安全分片与分层归并
  └─ 不可处理项隔离，不阻塞其他项目分区
  ▼
Thread L0 Summary
  + Record Coverage
  + Generation Identity
  │
  ├─ Thread Rollup（同一 project partition）
  ├─ Project Summary（同一 project subject）
  └─ Agent Memory（同一 canonical agent）
      + 全链路 Derivation Edges
  ▼
Compose Context Planner
  身份与权限过滤
  → canonical frontier
  → 相关性
  → supersession / 去重
  → 分池预算
  ▼
Runtime 完整请求预检
  → 请求模型
  → Provider overflow 仅作为有界兜底
```

## 2. 当前实现基线

### 2.1 保留的已有能力

- tenant/source/thread 隔离和 canonical Agent identity；
- 稳定 Agent Thread、独立 Task/Todo Thread；
- Record 幂等写入与 Local Agent Host durable outbox；
- Thread L0 Summary 与多层 Rollup；
- Agent Memory 与 Project Summary；
- Task/Todo terminal summary marker；
- Cloud Agent 异步执行、续跑、重试、Outbox 和 dead-letter；
- compose 返回 Thread Summary、Agent Memory、Project Summary 和 recent records；
- 动态项目会话按当前项目过滤 Record 和 Thread L0；
- Responses server-side compaction 和 Provider overflow 有界恢复；
- managed Memory Agent Prompt revision/checksum；
- Agent Memory 的稳定跨项目 generation profile。

### 2.2 必须修复的问题

#### P0-1：Repair Summary 不在正常消费链路

Repair 当前写入 `summary_type=thread_repair`，而 compose、Thread Rollup 和 Subject Memory 主链路消费 `thread_incremental`。源 Record 被标记 summarized 后，Repair 文本可能不再可达。

#### P0-2：oversized Record 被跳过后仍退出原始上下文

单条 Record 超过 Summary token limit 时，当前实现可能写入 `skipped_single_record_token_limit` 并将 Record 标记为 summarized，但正文没有进入模型生成。

#### P0-3：高层 Rollup 仍会静默跳过 oversized 输入

Thread Rollup 会排除超大源 Summary 后仍将整批标记 rolled up；Subject Memory Rollup 对超大 Memory 也存在相同行为。即使修复 L0，信息仍可能在 L1/L2 或长期记忆层丢失。

#### P0-4：派生关系不可完整查询

当前 start/end/count、`summary_id` 和 source digest 不能可靠回答：

- Record 当前由哪份 active L0 Summary 覆盖；
- 一份 Rollup 实际消费了哪些 Summary；
- Subject Memory 来自哪些 Summary 或下层 Memory；
- 某个输入是否被完整处理、被退休、被重建或仍处于隔离状态；
- 删除、迁移、Repair 或重复 callback 后，派生链是否仍闭合。

#### P0-5：生成成功后的提交和异步 Job 状态不是一个幂等闭环

Summary/Memory 创建、源状态更新、Job 完成、lease 释放和 dispatch 状态分散在多个操作中。进程在模型完成或数据库提交附近崩溃时，重复 callback 可能再次执行同一生成结果。

#### P0-6：一个毒丸 Record 可能阻塞稳定 Agent Thread 的其他项目

稳定 Agent Thread 可承载多个项目。若最早 pending Record 永久不可处理，而 selector 总从最早记录开始，整个 Thread 的 Summary slot 可能被反复占用，其他项目分区也无法推进。

#### P1-1：Thread Rollup 可能跨项目混合

L0 已保存 `project_subject_id`，但高层 Rollup 候选仍主要按 Thread 和 level 选择。动态项目线程目前通过不召回 top-level Thread Rollup 规避污染，但仍会生成不可安全使用的派生数据。

#### P1-2：compose 依赖固定条数，不受完整请求预算约束

当前 compose 使用 `summary_limit`、固定 Subject Memory 条数和可选 `recent_record_limit`。默认 recent records 没有总 token 上限，第一次 Provider 请求可能因 Worker 延迟或大 Tool output 过大。

#### P1-3：召回缺少当前问题相关性和 canonical frontier

当前主要选择最高层一条和最近若干条。较早但相关的决定可能无法进入上下文；同一个来源的 L0、Rollup、Project Summary 和 Agent Memory 也可能同时注入。

#### P1-4：自由文本难以表达纠错、来源和时效

自由文本无法稳定表达事实的 scope、source、`active/superseded/uncertain`、观察时间和置信度，多轮压缩后容易保留互相冲突的旧事实。

#### P1-5：项目上下文和预算字段缺少明确的信任边界

compose v2 的项目、tokenizer、retrieval mode 和预算上限不能直接信任普通请求参数。项目必须来自已验证运行身份，预算必须受到服务端策略上限约束。

#### P1-6：Rust Runtime 尚无完整模型能力快照

当前 `ModelRuntimeConfig` 和 Local Agent Host durable model snapshot 只有 `max_output_tokens` 与 `responses_compaction_threshold`，没有可作为硬预算依据的 `context_window_tokens`、协议开销配置和输出 reserve。代码还明确要求 Runtime 不得自行推断模型窗口，因此 M4b 不能假定这些字段已经存在，必须先完成 M4a 的能力快照契约。

#### P1-7：现有物理删除和 Subject Memory 原地 upsert 与版本化派生冲突

当前 Record、Summary、Thread 仍存在物理删除 API；Subject Memory 仍按 `(tenant, source, subject, memory_key)` 原地 upsert。新增 coverage 外键后，旧删除路径可能被阻塞；同一输入在新 Prompt/策略下重建时，原地 upsert 又会破坏历史版本和 generation 唯一性。

#### P2-1：Prompt 和 Pipeline 指令存在多个实际来源

实际生成同时受到 managed Agent Prompt、固定 System Prompt 和代码内 leaf/merge directive 影响。所有生效指令都需要版本化并写入生成审计，不能只记录 managed Prompt revision。

### 2.3 代码实施锚点

| 现状 | 当前代码位置 | 本方案落点 |
| --- | --- | --- |
| Repair 写入 `thread_repair` | `services/summary/thread_repair/summary/runner.rs` | 7.1、7.2 |
| compose 主链只读取 `thread_incremental` | `services/context/blocks.rs` | 4.1、10 |
| oversized Record 可被标记 summarized | `services/summary/thread_summary/execution_part01.rs` | 6.3、6.5 |
| Thread Rollup 跳过 oversized 输入后仍推进状态 | `services/summary/rollup/execution.rs` | 8.2 |
| Subject Memory Rollup 存在相同跳过路径 | `services/subject_memory/job/rollup.rs` | 8.2 |
| Record 渲染没有稳定 source ref | `services/summary/render.rs` | 6.1、9.3 |
| 固定 System Prompt 与硬编码 leaf/merge directive 并存 | `services/ai/client/mod.rs`、`services/summary/builders.rs`、`services/subject_memory/builders.rs` | 13.1 |
| `target_summary_tokens` 已退休 | `config_center_service/backend/src/catalog/constants/shared.rs` | 13.2 |
| Runtime 缺少 context window/output reserve 快照 | `crates/chatos_ai_runtime/src/traits/model.rs`、`clients/local_agent_host/layers/interface/src/control_plane.rs` | 11.1、16 的 M4a |
| Subject Memory 按 memory key 原地 upsert | `repositories/subject_memories/writes.rs` | 5.6、5.7 |
| macOS/Windows 仍直接调用 compose/record API | `clients/macos/Sources/ChatOSAPI/ChatOSMemoryEngineService.swift`、`clients/windows-react/src-tauri/src/story_studio/memory.rs` | 6.2、10.2、17 |

这些路径是实施入口，不是穷举清单；写代码前仍需以 migration、repository contract tests 和调用链搜索结果为准。

## 3. 目标、非目标和不变量

### 3.1 目标

1. 任意 summarized Record 均有且只有一个 active L0 coverage。
2. 任意 marked rolled-up 的 Summary/Memory 在对应 derivation kind 和 output scope 下均有且只有一条 active、可查询的输出派生边。
3. Record、Summary、Memory 不因单项 oversized 被静默跳过。
4. 同一生成任务重复执行或重复 callback 不会产生第二份 active 输出。
5. Repair、退休和重建保留完整历史，并能恢复到可继续处理的状态。
6. Thread、Project、Agent 三类召回严格遵守租户、来源、Agent 和项目边界。
7. 每轮模型输入在发送前满足完整请求预算，并能解释选择、去重和裁剪原因。
8. 新生成记忆能表达来源、scope、时效、置信度和 supersession。
9. 历史迁移可审计、可重跑、可恢复，不物理删除用户历史。
10. 所有客户端和服务端调用方在同一发布窗口完成契约升级；切换后只允许新契约和单一权威实现，旧写契约直接拒绝。

### 3.2 非目标

- 不把 Memory 当作授权、审批、任务状态或工具副作用的权威来源；
- 不把所有历史原文永久注入模型；
- 不允许模型自行选择 Agent、Project 或任意 Memory subject；
- 不部署独立向量数据库；
- 不让生成模型直接修改原始 Record；
- 不重写 Local Agent Loop、Tool Runtime 或 Project 权威状态；
- 不通过清库处理历史污染。

### 3.3 数据与运行不变量

```text
I1. summarized Record → exactly one active L0 coverage
I2. rolled-up input + derivation kind + output scope → exactly one active edge to a valid output
I3. 输出、coverage/lineage、源状态、Job terminal 和数据库派生事件 Outbox 在一个幂等提交边界内
I4. 输出失败、退休或隔离后，源输入可恢复为 pending 或待重建
I5. 同一 generation_id 只能对应一份逻辑输出
I6. Thread Summary、Thread Rollup 和 Project Summary 不得合并 Project/null partition；Agent Memory 仅可按明确规则跨项目提取稳定事实并保留每条输入的 partition lineage
I7. Agent Memory 只能由当前 canonical Agent scope 召回
I8. Project Summary 只能由当前已验证 Project scope 召回
I9. 当前输入、当前授权和权威数据库状态优先于历史 Memory
I10. 闭合 Tool exchange 必须作为原子单元保留；孤立工具消息不得发送 Provider
I11. 任何裁剪、去重和隔离均在 response/job meta 中留下原因；必需依赖不可用时 fail closed，不切换到旧实现或低保证路径
I12. 历史迁移只做可恢复状态变更，不物理删除 Record、Summary 或 Memory
I13. Record upsert、不可变 partition 校验、partition pending 统计和 terminal forced dispatch 必须在同一数据库事务提交
I14. processing coverage 由服务端冻结的完整输入决定；模型 source refs 只表达事实 provenance，不能决定 Record 是否已被完整处理
I15. 同一 Thread partition 只有一个 generation/coverage/lineage 权威 writer；仓库中不得保留可运行的旧生成 writer 或运行时切换分支
I16. 任何能够写入或选择 project partition 的请求都必须携带服务端验证的 Runtime Context Assertion；仅有 project 字符串、Thread label 或 Record metadata 不构成可信身份
I17. 普通业务删除使用 tombstone；用户彻底删除、租户清理和法定擦除使用独立 privileged erasure 事务，并同步删除 coverage、lineage、事实投影、索引和派生正文
I18. 同一逻辑 memory key 可有多个不可变 generation 版本，但任一 output scope 同一时刻只能有一个 active/recall eligible 版本
I19. 完整请求硬预算只能建立在已发布且已冻结的 model capability snapshot 上；缺失 context window 或 output reserve 时 fail closed，不能由模型名猜测
```

## 4. 统一术语和派生模型

### 4.1 Summary 类型与生成原因

正常总结、主动压缩、pending rebuild 和 covered-range resummarize 均写入：

```text
summary_type = thread_incremental
```

差异由生成原因表达：

```text
generation_reason = threshold
                  | terminal
                  | active_compaction
                  | pending_rebuild
                  | resummarize
                  | migration_rebuild
```

首版 profile：

```text
thread_summary_v2
thread_rollup_v2
project_summary_v2
agent_memory_v2
subject_memory_rollup_v2
```

`thread_repair` 仅作为旧数据识别值保留；新写路径不再产生该 summary type。

### 4.2 Project partition

Record、Thread Summary、Thread Rollup 和 Project Summary 统一使用非空数据库分区键：

```text
project_partition_key = "project:<id>"
project_partition_key = "__none__"   // 无项目
```

API 层仍可使用 `Option<String>`，进入数据库前必须规范化。选择器、锁、digest、唯一键、索引和召回过滤均使用同一个规范化分区键，避免 SQL `NULL` 比较语义造成漏过滤。该键来自受信写入上下文并在 Record 创建时冻结；正文、模型输出和普通工具参数不能修改它。Agent Memory 的输出 scope 是 canonical Agent，不伪造单一项目键，但每条输入 edge 必须保留其原始 partition。

### 4.3 Canonical frontier

同一派生树在 compose 和下游生成时只选择一个不重叠前沿：

- 已被 active Rollup 完整覆盖的旧 L0，不与该 Rollup 同时作为相同 pool 的候选；
- 最近且尚未进入 Rollup 的 L0 作为 delta 保留；
- Project/Agent Subject Memory 同样选择最高有效 Rollup加未归并 delta；
- 相关性召回命中祖先和后代时，按 lineage、时效与 scope 只渲染一份，保留 provenance。

## 5. 数据模型与迁移 0011

新增迁移：

```text
memory_engine/backend/migrations/postgres/0011_memory_coverage_and_lineage.sql
```

仓库当前最新 PostgreSQL 迁移是 `0010_compact_turn_conversation_cursor_index.sql`，因此本方案使用下一个连续编号 `0011`。`0011` 将新 schema、历史回填、约束切换和旧结构清理作为一次维护窗口迁移执行，不复用或改写 `0010`，也不留下双写结构。

### 5.1 Record、Summary 与 Subject Memory 查询关键列

为 `engine_records` 增加正式列，替代热路径中的 JSON project filter：

```sql
project_partition_key TEXT NULL
```

为 `engine_summaries` 增加正式列，并在切换前完成旧 JSON metadata 回填；切换后热路径只读正式列：

```sql
generation_id         TEXT NULL,
generation_reason     TEXT NULL,
generation_profile    TEXT NULL,
project_partition_key TEXT NULL,
recall_eligible       BOOLEAN NOT NULL DEFAULT true,
retired_reason        TEXT NULL,
payload_schema        TEXT NULL,
structured_payload    JSONB NULL,
pipeline_revision     TEXT NULL
```

为 `engine_subject_memories` 增加对应的正式列：

```sql
generation_id         TEXT NULL,
generation_profile    TEXT NULL,
project_partition_key TEXT NULL,
recall_eligible       BOOLEAN NOT NULL DEFAULT true,
retired_reason        TEXT NULL,
payload_schema        TEXT NULL,
structured_payload    JSONB NULL,
pipeline_revision     TEXT NULL
```

要求：

- 所有新数据以上述列为查询权威，不依赖 JSON 路径过滤；
- Record 写入时必须显式写入规范化 `project_partition_key`，不得依赖数据库默认值把未知项目误记为 `__none__`；
- Record 的 partition 只能来自认证后的服务端/客户端项目上下文快照；正文、模型输出、普通 metadata 和工具参数均不是可信来源；
- 同一 `(tenant_id, source_id, record_id)` 重放 upsert 时，规范化 partition 必须与首次写入完全一致；冲突必须整笔拒绝并记录审计，禁止通过 `ON CONFLICT DO UPDATE` 改写 partition；
- Thread L0/Rollup 必须有规范化 `project_partition_key`；Project Summary 必须为对应 `project:<id>`，Agent Memory 可为全局 scope；
- `recall_eligible=false` 的数据保留审计，但不能进入生成或 compose 候选；
- 为 Record/Summary 的项目分区、generation profile、recall eligibility 和 payload 建立有界索引；
- 迁移先增加 nullable 列并回填，所有新写路径通过 repository 校验非空；历史未知分区的数据退出 recall 或重建，不能强行归入 `__none__`。

### 5.2 Thread Summary Partition 状态表

现有 `engine_threads` 只有一套 Summary 状态、锁和 pending 计数，无法让同一稳定 Agent Thread 的不同项目独立推进。新增：

```sql
CREATE TABLE engine_thread_summary_partitions (
    tenant_id                          TEXT        NOT NULL,
    source_id                          TEXT        NOT NULL,
    thread_id                          TEXT        NOT NULL,
    project_partition_key              TEXT        NOT NULL,
    summary_status                     TEXT        NOT NULL,
    pending_record_count               BIGINT      NOT NULL DEFAULT 0,
    pending_summary_tokens             BIGINT      NOT NULL DEFAULT 0,
    summary_job_run_id                 TEXT        NULL,
    summary_locked_at                  TIMESTAMPTZ NULL,
    summary_lock_expires_at             TIMESTAMPTZ NULL,
    rollup_status                      TEXT        NOT NULL DEFAULT 'pending',
    rollup_job_run_id                  TEXT        NULL,
    rollup_locked_at                   TIMESTAMPTZ NULL,
    rollup_lock_expires_at             TIMESTAMPTZ NULL,
    summary_dispatch_pending            BOOLEAN     NOT NULL DEFAULT false,
    summary_dispatch_version            BIGINT      NOT NULL DEFAULT 0,
    summary_dispatch_published_version  BIGINT      NOT NULL DEFAULT 0,
    summary_dispatch_consumed_version   BIGINT      NOT NULL DEFAULT 0,
    summary_dispatch_force_version      BIGINT      NOT NULL DEFAULT 0,
    summary_dispatch_requested_at       TIMESTAMPTZ NULL,
    summary_dispatch_published_at       TIMESTAMPTZ NULL,
    summary_dispatch_consumed_at        TIMESTAMPTZ NULL,
    summary_dispatch_last_error         TEXT        NULL,
    summary_dispatch_last_failed_at     TIMESTAMPTZ NULL,
    summary_dispatch_dead_letter_version BIGINT     NULL,
    summary_dispatch_dead_lettered_at   TIMESTAMPTZ NULL,
    summary_dispatch_recovery_count     BIGINT      NOT NULL DEFAULT 0,
    summary_dispatch_recovered_at       TIMESTAMPTZ NULL,
    created_at                          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, source_id, thread_id, project_partition_key),
    FOREIGN KEY (tenant_id, source_id, thread_id)
        REFERENCES engine_threads (tenant_id, source_id, id) ON DELETE CASCADE
);

CREATE INDEX engine_thread_summary_partitions_claim_idx
ON engine_thread_summary_partitions(summary_status, summary_lock_expires_at, updated_at);

CREATE INDEX engine_thread_summary_partitions_dispatch_idx
ON engine_thread_summary_partitions(summary_dispatch_requested_at, updated_at)
WHERE summary_dispatch_pending;

CREATE INDEX engine_thread_summary_partitions_rollup_claim_idx
ON engine_thread_summary_partitions(rollup_status, rollup_lock_expires_at, updated_at);
```

新 selector、claim、lease、pending 统计、Summary dispatch 和 Rollup lease 以该表为唯一权威。迁移在停写维护窗口完成：创建并回填 partition 行，执行全量计数与锁状态校验，通过后部署只使用该表的新实现。旧 Thread 全局 claim/lease 写路径随同一版本删除，不存在双 writer、运行时选择或过渡状态。

`engine_threads` 旧 pending/lock 字段只在数据库迁移事务中做一次最终对账；应用切换后不得继续作为写投影或 claim 依据。所有调用方完成迁移后删除这些旧字段。

#### 5.2.1 Record ingest 原子事务

每批 Record 写入必须在一个 repository 事务中完成：

同一 batch 只能属于一个经过验证的 `(tenant, source, thread, project_partition_key)`；混合 partition batch 在开启事务前整批拒绝。需要写多个 partition 的客户端必须拆成多个有独立 assertion 的 batch，避免多 partition 锁顺序和部分信任问题。

```text
1. 锁定或创建目标 Thread partition 行
2. INSERT Record，或校验幂等重放的内容身份与不可变 partition
3. 仅对首次插入的 Record 增加 pending_record_count / pending_summary_tokens
4. 若是可信 terminal 事件，在同一行推进 summary_dispatch_force_version
5. 写数据库派生事件 Outbox
6. COMMIT
```

Record 已提交而 pending 统计或 forced dispatch 未提交是不允许存在的中间状态。Local Agent durable outbox 只负责客户端到服务端的 Record 投递；partition dispatch state 负责数据库内的待处理版本；数据库派生事件 outbox 负责事务提交后的跨进程发布，三者不得共用一个“Outbox 已完成”状态。

应用切换后只更新 partition 行和事件 Outbox；不得再更新 `engine_threads` 旧聚合计数或从旧字段恢复 claim。迁移前的计数回填与校验由离线迁移命令完成，失败则中止发布。

### 5.3 数据库派生事件 Outbox

partition dispatch state 表达“业务上还有哪个版本待处理”；数据库派生事件 Outbox 表达“哪个已提交版本尚未可靠发布到跨进程队列”。二者职责不同，但必须在产生业务变化的同一事务写入：

```sql
CREATE TABLE engine_derived_event_outbox (
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    event_id              TEXT        NOT NULL,
    event_kind            TEXT        NOT NULL CHECK (event_kind IN (
        'thread_summary_requested',
        'thread_rollup_requested',
        'subject_memory_source_requested',
        'subject_memory_rollup_requested',
        'derivation_rebuild_requested'
    )),
    aggregate_kind        TEXT        NOT NULL CHECK (aggregate_kind IN (
        'thread_partition','thread_summary','subject_memory','subject_memory_scope'
    )),
    aggregate_id          TEXT        NOT NULL,
    project_partition_key TEXT        NULL,
    dispatch_version      BIGINT      NOT NULL,
    generation_id         TEXT        NULL,
    payload               JSONB       NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','claimed','published','dead_letter')),
    attempt_count         BIGINT      NOT NULL DEFAULT 0,
    lease_owner           TEXT        NULL,
    lease_token           TEXT        NULL,
    lease_expires_at      TIMESTAMPTZ NULL,
    next_attempt_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error_code       TEXT        NULL,
    last_error_detail     JSONB       NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at          TIMESTAMPTZ NULL,
    dead_lettered_at      TIMESTAMPTZ NULL,
    PRIMARY KEY (tenant_id, source_id, event_id),
    UNIQUE (
        tenant_id, source_id, event_kind, aggregate_kind,
        aggregate_id, dispatch_version
    )
);

CREATE INDEX engine_derived_event_outbox_claim_idx
ON engine_derived_event_outbox(status, next_attempt_at, lease_expires_at, created_at)
WHERE status IN ('pending','claimed');
```

要求：

- `payload` 只保存 schema version、scope IDs、partition、dispatch version 和 generation ID，不保存正文、query text、凭据或模型输出；
- publisher 使用 `FOR UPDATE SKIP LOCKED` 和有期限 lease claim，发布成功后 CAS 为 `published`；进程崩溃后可接管；
- 队列 consumer 以 `(aggregate scope, dispatch_version)` 幂等消费，旧版本到达不得回退 durable dispatch state；
- Outbox 重放可以重复发布，但不能重复生成逻辑输出；
- dead-letter/rearm 保留原 event，rearm 递增对应业务 dispatch version 并创建新 event；
- Local Agent Record outbox、业务 dispatch state 和本表不得复用状态字段。

### 5.4 Record Coverage 表

```sql
CREATE TABLE engine_summary_record_coverage (
    id                    TEXT        NOT NULL,
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    thread_id             TEXT        NOT NULL,
    project_partition_key TEXT        NOT NULL,
    summary_id            TEXT        NOT NULL,
    record_id             TEXT        NOT NULL,
    ordinal               BIGINT      NOT NULL,
    source_ref            TEXT        NOT NULL,
    coverage_kind         TEXT        NOT NULL CHECK (coverage_kind = 'full'),
    status                TEXT        NOT NULL CHECK (status IN ('active','superseded','retired')),
    generation_id         TEXT        NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at            TIMESTAMPTZ NULL,
    retired_reason        TEXT        NULL,
    PRIMARY KEY (tenant_id, source_id, id),
    UNIQUE (tenant_id, source_id, summary_id, record_id),
    UNIQUE (tenant_id, source_id, summary_id, ordinal),
    UNIQUE (tenant_id, source_id, summary_id, source_ref),
    FOREIGN KEY (tenant_id, source_id, summary_id)
        REFERENCES engine_summaries (tenant_id, source_id, id),
    FOREIGN KEY (tenant_id, source_id, record_id)
        REFERENCES engine_records (tenant_id, source_id, id)
);

CREATE UNIQUE INDEX engine_summary_record_one_active_idx
ON engine_summary_record_coverage(tenant_id, source_id, record_id)
WHERE status='active';
```

仓库从 migration `0006` 起使用 `(tenant_id, source_id, id)` 复合主键，因此必须使用上面的复合外键，不能只引用 `id`。应用事务还要校验 Record、Summary 的 thread 和 project partition 完全一致，数据库 contract tests 覆盖所有不一致情况。

### 5.5 全链路 Derivation Edge

新增统一派生边表，覆盖 Summary→Summary、Summary→Subject Memory、Subject Memory→Subject Memory：

```sql
CREATE TABLE engine_memory_derivation_edges (
    id                          TEXT        NOT NULL,
    tenant_id                   TEXT        NOT NULL,
    source_id                   TEXT        NOT NULL,
    derivation_kind             TEXT        NOT NULL CHECK (derivation_kind IN (
        'thread_rollup','project_memory','agent_memory','subject_memory_rollup'
    )),
    output_scope_key            TEXT        NOT NULL,
    input_project_partition_key TEXT        NOT NULL,
    output_kind                 TEXT        NOT NULL CHECK (output_kind IN (
        'thread_summary','subject_memory'
    )),
    output_id                   TEXT        NOT NULL,
    input_kind                  TEXT        NOT NULL CHECK (input_kind IN (
        'thread_summary','subject_memory'
    )),
    input_id                    TEXT        NOT NULL,
    ordinal                     BIGINT      NOT NULL,
    status                      TEXT        NOT NULL CHECK (status IN ('active','superseded','retired')),
    generation_id               TEXT        NOT NULL,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at                  TIMESTAMPTZ NULL,
    retired_reason              TEXT        NULL,
    PRIMARY KEY (tenant_id, source_id, id),
    UNIQUE (tenant_id, source_id, output_kind, output_id, input_kind, input_id),
    UNIQUE (tenant_id, source_id, output_kind, output_id, ordinal)
);

CREATE UNIQUE INDEX engine_memory_derivation_one_active_idx
ON engine_memory_derivation_edges(
    tenant_id, source_id, input_kind, input_id, derivation_kind, output_scope_key
)
WHERE status='active';
```

`output_kind/input_kind` 首版限定为 `thread_summary` 和 `subject_memory`。`derivation_kind` 首版限定为 `thread_rollup`、`project_memory`、`agent_memory`、`subject_memory_rollup`。`output_scope_key` 使用服务端生成的规范键，区分 Thread partition、Project subject 和 Agent subject。同一 Summary 可以分别派生 Project Summary 和 Agent Memory，但同一输入、派生类型和输出 scope 只能有一条 active edge。

`input_project_partition_key` 描述每个输入自身的项目来源；Agent Memory 允许消费多个项目的稳定事实，因此不能错误地给整张 edge 表只放一个“输出项目分区”。对应 repository 在写入时检查真实表记录、scope、level、memory type 和输入 partition；不能仅依赖自由字符串。

由于 PostgreSQL 不能用普通外键直接约束 polymorphic ID，`0011` 必须同时增加 DEFERRABLE constraint trigger，在事务提交末尾验证：

- `input_kind/input_id` 与 `output_kind/output_id` 各自在且只在指定真实表中存在；
- 输入、输出 tenant/source 与 edge 一致；
- `thread_rollup` 的 thread、partition、level 和 profile 合法；
- `project_memory` 的 Project subject 与输入 partition 一致；
- `agent_memory` 的 canonical Agent 与输入 Thread identity 一致；
- `subject_memory_rollup` 的 subject、memory type 和 level 合法；
- active edge 不得指向 retired、deleted 或 `recall_eligible=false` 的输出。

只做 repository 校验不能满足 I2；数据库 trigger 与 PostgreSQL contract tests 是上线条件。

### 5.6 Generation Registry、Job 与 source digest

新增统一 generation registry，作为 Summary 与 Subject Memory 幂等提交的唯一落点：

```sql
CREATE TABLE engine_generation_commits (
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    generation_id         TEXT        NOT NULL,
    job_run_id            TEXT        NOT NULL REFERENCES engine_job_runs(id),
    generation_step_key   TEXT        NOT NULL,
    generation_kind       TEXT        NOT NULL CHECK (generation_kind IN (
        'thread_summary','thread_rollup','project_memory',
        'agent_memory','subject_memory_rollup'
    )),
    input_digest          TEXT        NOT NULL,
    input_manifest        JSONB       NOT NULL,
    config_snapshot       JSONB       NOT NULL,
    project_partition_key TEXT        NULL,
    status                TEXT        NOT NULL CHECK (status IN ('prepared','running','committed','abandoned')),
    attempt_count         BIGINT      NOT NULL DEFAULT 0,
    lease_owner           TEXT        NULL,
    lease_token           TEXT        NULL,
    lease_acquired_at     TIMESTAMPTZ NULL,
    lease_expires_at      TIMESTAMPTZ NULL,
    last_error_code       TEXT        NULL,
    last_error_detail     JSONB       NULL,
    last_failed_at        TIMESTAMPTZ NULL,
    output_kind           TEXT        NULL CHECK (
        output_kind IS NULL OR output_kind IN ('thread_summary','subject_memory')
    ),
    output_id             TEXT        NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    committed_at          TIMESTAMPTZ NULL,
    abandoned_at          TIMESTAMPTZ NULL,
    abandoned_reason      TEXT        NULL,
    CONSTRAINT engine_generation_output_state_check CHECK (
        (status = 'committed' AND output_kind IS NOT NULL AND output_id IS NOT NULL)
        OR (status <> 'committed' AND output_kind IS NULL AND output_id IS NULL)
    ),
    PRIMARY KEY (tenant_id, source_id, generation_id),
    UNIQUE (tenant_id, source_id, job_run_id, generation_step_key)
);
```

一个 Job 可能依次产生 L0 和多个 Rollup，因此幂等键不是简单的 `job_run_id`，而是 `job_run_id + generation_step_key`。Generation Registry 同时是持久化的 Job step 状态机：`prepared → running → committed`，失败重试通过 attempt 和有期限 lease 接管，不用进程内布尔值表达“step done”。每个逻辑输出在模型调用前插入或复用 `prepared` registry；最终事务写输出和 coverage/edge 后，将同一行原子改为 `committed + output_kind + output_id`。

`engine_summaries`、`engine_subject_memories`、coverage 和 derivation edge 的 `(tenant_id, source_id, generation_id)` 必须引用 registry 复合主键。前两张输出表分别建立 `generation_id IS NOT NULL` 的 tenant/source partial unique index，但这些外键和分表唯一索引不能证明 generation 没有同时出现在两张输出表。`0011` 必须增加 DEFERRABLE constraint trigger：仅当 registry 进入 `committed` 时，提交末尾按其 `output_kind/output_id` 验证恰好一张目标表存在且另一张不存在；`prepared/running` 允许暂时没有输出。所有写入只能经过统一 commit repository；repository 验证是前置错误提示，数据库 trigger 是最终约束。

同一 constraint trigger 还要验证 `engine_job_runs` 的 tenant/source/thread/subject 与 generation scope 一致；不能只依赖全局 `job_run_id` 外键把其他租户或来源的 Job 绑定到当前 generation。

每次生成在调用模型前冻结并持久化，不只计算一个 digest：

```text
generation_id
job_run_id + generation_step_key
input IDs + ordered digest
project_partition_key / output scope
generation profile
prompt revision/checksum
pipeline directive revision/checksum
model provider/model id
tokenizer/estimator id
policy revision/checksum
```

`input_manifest` 保存有序输入种类、真实 ID、版本、partition、section/chunk 边界和每项 digest；`config_snapshot` 保存 generation profile、Prompt/Pipeline、模型、tokenizer 和 policy 的不可变生效值及 checksum。重放只能复用该快照；如果输入或配置已被明确替换，旧 generation 必须 `abandoned`，然后创建新 step/generation，不能在原行上偷偷换配置。

Digest contract 固定为：

```text
digest_algorithm = sha256
digest_encoding  = lowercase hex with "sha256:" prefix
canonical_input  = UTF-8 JSON，字段按固定 schema 顺序编码，数组保持 source ordinal，
                   字符串不做 locale 变换，时间统一 RFC3339 UTC
```

不得继续使用逗号/冒号拼接 ID 作为 generation digest。manifest 保存 `schema_version`、`digest_algorithm` 和 canonicalizer revision；每个输入 digest 同时覆盖真实 ID、不可变内容身份、partition 和版本。相同 ID 但正文、metadata、structured payload 或可信 partition 不同必须拒绝为非幂等重放，不能复用旧 generation。

同一 Job step 重放必须复用同一个 `generation_id`。数据库发现该 generation 已成功提交时，直接返回已存在输出并完成对应 step，不再次生成或写入。registry 为 `prepared` 或过期 `running` 时可由 CAS lease 接管后重新调用模型，但只有一个事务能通过当前 lease token 和 `running → committed` 条件更新提交逻辑输出；可重试失败写入 attempt/error 字段并释放或等待 lease，不提前把 registry 终结。只有 dead-letter、人工取消或输入被明确替换时才标记 `abandoned`，后续 rearm 创建新的 Job/generation 并保留旧审计。

Summary Job 进入任意终态后，低频恢复器必须扫描其仍持有的 `summarizing` Record。只有不存在 active coverage 时才能清除 `summary_job_run_id/summary_started_at` 并恢复为 `pending`；仍在运行的 Job、已提交 coverage 或已被新 owner 接管的 Record 一律不得改动。该恢复与 partition lease 恢复在同一个 Worker 恢复循环执行，使进程重启、模型超时和终态 callback 丢失都不会永久冻结输入。

父 Job 一旦创建 durable Cloud Agent run，stale 判定必须读取 Cloud Agent 权威状态：run 非终态，或 `owner_lifecycle_terminal` 回调仍处于 pending/publishing 时，父 Job 不得仅因 `started_at` 超过固定时长而失败。Cloud Agent run 终态且 callback 已成功发布时，父 Job 应已由 callback 终结；callback dead-letter 或缺失时才允许父 Job stale recovery 进入失败与 Record 自动恢复。父 Job timeout 与 Cloud Agent retry/lease 不得成为两个相互竞争的生命周期控制器。

generation finalization 使用稳定的逻辑 `lease_owner` 和每次领取都变化的 `lease_token`。同一逻辑 owner 在进程重启或 callback 重放后可立即重新领取，数据库原子签发新 token 并使旧 token 的提交 CAS 失败；不同 owner 只有在 lease 到期后才能接管。不能要求有限次 outbox retry 等待完整固定 lease，也不能取消 fencing token 校验。

Subject Memory 必须从当前按逻辑 `memory_key` 原地 upsert 改为不可变 generation 版本：

- 保留 `memory_key` 作为逻辑系列键，不再作为可覆盖版本的唯一身份；
- 删除现有全局 `UNIQUE(tenant_id, source_id, subject_id, memory_key)`；为 generation 建立 `UNIQUE(tenant_id, source_id, generation_id)`；历史 `generation_id IS NULL` 数据在维护窗口归档为不可召回审计记录，不为其保留可写 logical-key 路径；
- 增加 partial unique index，约束同一 `(tenant, source, subject, memory_key)` 最多一份 `status='active' AND recall_eligible`；
- 新 generation INSERT 新行；旧 active 行与其 edges 在同一事务切为 superseded/retired；
- `find by source_digest` 只能在 generation profile、config checksum 和 active status 同时匹配时复用，不能仅凭输入 ID digest 复用旧配置输出；
- 删除生成路径中的 `upsert_generated_subject_memory`；所有生成只能使用不可变 generation INSERT。管理 API 只允许通过版本化命令新增/退休版本，不再原地更新 legacy 行，也不得依赖无 predicate 的 `ON CONFLICT(tenant_id,source_id,subject_id,memory_key)`。

### 5.7 生命周期、版本和删除语义

- active coverage/derivation edge 是派生事实的唯一权威；`engine_records.summary_id`、`summary_status`、`rollup_status`、`subject_memory_summarized` 和 scope key 数组只作为同一事务内更新的查询投影，不能单独证明处理成功；
- 普通对外“删除 Record/Summary/Memory”改为 `retire/tombstone`，不再调用当前物理 DELETE repository；
- 退休输出与其 active edge 在同一事务中退休；
- 失去有效覆盖的 Record 恢复 pending；
- 失去有效输出的 Summary/Memory 恢复待派生状态；
- Thread archive 不删除派生历史，但所有内容退出普通 compose；
- 用户彻底删除、租户删除、法定擦除和数据保留到期使用独立 privileged erasure repository，在一个受控事务内按 `fact projection → edges → coverage → outputs → records → thread/subjects` 顺序物理删除，并清理 FTS/embedding；
- coverage 外键默认 `ON DELETE RESTRICT`，阻止普通 repository 绕过生命周期；privileged erasure 必须显式先删派生关系；Thread 级物理删除不能继续依赖当前隐式 cascade；
- erasure 只保留不含正文、项目名、query text 或可逆标识的合规审计计数；
- 物理清理的 retention schedule、审批和备份擦除由独立数据保留方案配置，但上述 repository 和 FK 行为必须在 `0011` 同时落地，避免现有 DELETE API 被新外键意外阻塞。

删除矩阵：

| 操作 | Record/Summary/Memory | coverage/edge | compose | 可恢复 |
| --- | --- | --- | --- | --- |
| retire/tombstone | 保留，状态退休 | 保留历史，active 改 retired | 不可见 | 是 |
| resummarize/supersede | 保留旧版本，新增版本 | 旧关系 superseded，新关系 active | 只见新版本 | 是 |
| Thread archive | 保留 | 保留 | 默认不可见 | 是 |
| privileged erasure | 物理删除 | 物理删除 | 不可见 | 否，受审批 |

## 6. Thread L0 Summary 生成

### 6.1 Source Unit

模型输入不再使用无 ID 的纯文本块，而是构建：

```text
SummarySourceUnit
  assistant_record_id?
  source_record_ids
  tool_call_ids[]
  tool_result_record_ids[]
  project_partition_key
  conversation_turn_id?
  tool_exchange_id?
  ordered sections
  estimated_tokens
```

每个模型可见 source item 携带稳定、不可伪造的短引用，例如：

```text
[source_ref=r:01][record_id=<id>][role=user][created_at=...]
```

服务端根据冻结 `input_manifest` 为 Source Unit 中的全部 Record 写 processing coverage，不要求模型逐条引用。模型返回的 source refs 必须来自本次冻结输入；未知引用使输出校验失败。提交前由服务端把短引用解析为结构化事实 provenance 中的真实 Record/Summary/Memory ID，持久化 Payload 不依赖脱离 generation 后会歧义的局部编号。模型漏引某条 Record 会降低事实 provenance 完整性或使对应事实校验失败，但不能把已经实际处理的 Source Unit 变成部分 coverage。

### 6.2 Tool exchange 原子分组

- 一个 assistant Record 及其包含的全部 tool calls、每个 call 对应的全部 tool result Record，共同组成一个 Source Unit；同一 assistant Record 不得按单个 call 拆成多份 processing coverage；
- 已闭合 exchange 不允许在批次、token 裁剪或项目分区中拆开；
- 一个 assistant Record 有多个 tool calls 时，必须等所有 call 闭合后整体进入 Memory；事实可分别引用单个 call/result，但 processing coverage 仍覆盖整个 Source Unit；
- 同一 exchange 的记录若出现互相冲突的项目身份，整组进入一致性 Repair，不能拆到两个 partition；
- 当前运行中尚未得到结果的 tool call 由 Runtime checkpoint 管理，不作为跨轮 Memory raw tail 发送；
- orphan call/result 进入一致性告警和 Repair，不伪造成完整 exchange。

所有 Record 生产者必须在切换前满足统一 ingest contract；切换后不存在按来源选择的旧入口：

```text
record_id                    稳定且幂等
conversation_turn_id         每个用户驱动轮稳定
assistant_record_id          Tool result 可反查发起 assistant
tool_exchange_id             同一 assistant 多 call 的稳定 exchange ID
tool_call_id                 每个 call 唯一
tool_result_ordinal/count     支持一个 call 多段结果并判断闭合
project_context_assertion_id  服务端已验证的冻结项目上下文
content_identity_digest       正文、允许 metadata、structured payload 的不可变摘要
```

- `crates/chatos_ai_runtime`、`clients/local_agent_host`、macOS `ChatOSMemoryEngineService` 和 Windows Story Memory 都必须写入上述等价字段；
- 缺少 turn/exchange 字段的客户端写请求直接拒绝；不得由服务端凭相邻时间猜测闭合 exchange；
- 对确实没有工具调用的普通消息，`tool_exchange_id` 可为空，但 `conversation_turn_id` 仍应提供；
- assistant 声明的 call 集合是闭合判定权威；Tool result 只能补全集合，不能自行创造未声明 call；
- 同一 ID 重放时 content identity、turn、exchange、call 和 partition 必须完全一致；
- M0 按 source/client version 统计 contract readiness；任一生产调用方未 ready 时中止整体切换，不允许按来源绕过新契约。

### 6.3 Oversized 安全分片

```text
Source Unit
  → 按 section 和内容类型切分
  → chunk digest
  → unit digest
  → 与其他 unit 生成 Thread L0
  → 校验所有 source ref 均有覆盖
```

分片边界：

| 内容 | 分片策略 |
| --- | --- |
| 文本/Markdown | 段落 → 行 → UTF-8 安全字符边界 |
| JSON | 对象字段、数组元素递归切分，保留 JSON path |
| Tool output | 保留 call identity，按 section/path 切分 |
| Reasoning | 独立 section；长期 Memory 默认不保留原始 reasoning |
| 媒体/二进制 | 不内联；仅保留经过权限校验的引用、类型、大小和安全描述 |

虚拟 chunk ref：

```text
<record_id>#chunk:<index>/<count>
```

chunk 不是新 Record，仅是本次 Pipeline 的内部输入。

现有 `build_summary_text` 和 Thread Rollup builder 虽然已经设置 `split_oversized_items=true`，但当前 L0 selector 会在进入 builder 前把单条 oversized Record 放入 `selection.oversized`。实现时必须删除 `mark_oversized_records_as_summarized` 成功路径，并把完整 Source Unit 送入统一 splitter；仅保留 builder 的开关而不修改 selector 不能解决数据丢失。

### 6.4 不可处理输入和分区级推进

结构合法且所有事实集合均为空，表示源输入没有可持久化事实，不属于不可处理输入。该结果只允许用于 Thread L0：最终事务写入 `thread_noop` 覆盖凭证，将源 Record 标记为 summarized、递减 partition pending 统计、提交 generation/Job，但不写 `engine_memory_facts`，也不派生 Rollup 或 Subject Memory 事件。`thread_noop` 只用于审计和 coverage，不进入 `thread_incremental` 召回链。模型 Payload schema、scope、source refs 或事实字段损坏仍属于校验失败，按冻结请求有限重试，预算耗尽后才进入 dead-letter。

若 Source Unit 在有限切分和归并后仍无法处理：

1. 保持源 Record pending；
2. 释放当前 claim；
3. Job 进入 retry，达到上限后 dead-letter；
4. 保存错误码 `memory_source_unit_unprocessable`；
5. 同一 project partition 不越过该 Source Unit；
6. 其他 project partition 和 `__none__` 仍可由独立 dispatch 继续处理；
7. 管理操作可在修复内容或策略后 rearm。

Summary claim/dispatch identity 调整为：

```text
(tenant_id, source_id, thread_id, project_partition_key)
```

dispatch envelope、强制总结、stale recovery、dead-letter 和 replay 均必须携带 `project_partition_key + version`。未指定 partition 的管理请求直接拒绝，不能 fan-out，也不能退回 Thread 全局锁。Thread 可以保留全局并发上限，但不能让一个 partition 的 dead-letter 永久占用整个 Thread。

### 6.5 幂等原子提交

模型执行在事务外进行。完成后在一个数据库事务中：

```text
1. 锁定 Job step、partition claim、generation registry 和冻结输入
2. 若 generation_id 已提交，复用既有输出并跳到第 10 步
3. INSERT engine_summaries
4. INSERT engine_summary_record_coverage
5. 根据冻结 input manifest 校验 coverage 数量、顺序、身份、partition 和 source digest；模型 provenance 单独校验
6. UPDATE engine_records → summarized + summary_id
7. 更新 engine_thread_summary_partitions pending count 和 token
8. 更新 Rollup/Subject Memory durable dispatch state/version，并写数据库派生事件 Outbox
9. generation registry → committed + output id
10. Job step 标记 done，保存 output id 和统计
11. 释放 record/partition claim
12. COMMIT
```

任一步失败全部回滚。模型调用始终在事务外；只有冻结/claim 短事务和最终 CAS commit 事务持锁。最终事务提交后再尝试发布数据库派生事件 Outbox；发布失败由 Outbox publisher 补偿，不回滚已完成生成。这里的 Outbox 不等同于 Local Agent 的 Record 投递 outbox，也不替代 partition 表上的 durable dispatch version。

## 7. Repair 与重建

### 7.1 Pending rebuild

用途：处理 pending、stale summarizing、旧 oversized skip 和历史不可达数据。

- 只消费当前没有 active coverage 的 Record；
- 按 project partition 选择；
- 输出 canonical `thread_incremental`；
- `generation_reason=pending_rebuild|migration_rebuild`；
- 使用正常 L0 coverage、事务和下游 dispatch。

### 7.2 Covered-range resummarize

用途：纠正已经覆盖但内容错误、Prompt 过期或项目归属错误的 Summary。

```text
1. 短事务 claim 目标 Summary，冻结 active coverage、源 Record、partition 和 digest
2. 事务外生成并校验新 canonical Summary
3. 提交事务重新锁定目标并验证 claim、generation 和 source digest 未变化
4. 将旧 coverage/summary 标记 superseded
5. 写新 Summary 和 active coverage
6. Record.summary_id 指向新 Summary
7. 退休旧下游 active edges，并把受影响输出标为待重建/退出 recall
8. 创建新的 Rollup/Subject Memory durable rebuild dispatch
9. generation registry、Job step 和 claim 原子完成
```

第 4—9 步在同一事务中；任一步失败全部回滚，因此不会出现覆盖空窗。必须先把旧 active coverage 改为 superseded，再插入新 active coverage，否则会触发 active coverage partial unique index。旧 Summary 和旧派生结果保留审计，但退出 recall。不能原地改写旧 Summary 正文。

## 8. Thread Rollup 与 Subject Memory

### 8.1 Project-aware Thread Rollup

选择键：

```text
(tenant_id, source_id, thread_id, project_partition_key, level)
```

要求：

- 所有输入必须为 done、recall eligible、同一 partition 和同一 generation profile；
- `keep_level0_count` 按 partition 计算；
- source digest 包含 partition、level、ordered source IDs 和 profile；
- 输出继承 partition；
- 任何跨 partition 候选直接拒绝并告警；
- 动态项目线程只在分区 Rollup 生产验收后启用 top-level Thread Rollup 召回。

### 8.2 Rollup oversized 处理

Thread Summary 和 Subject Memory 均使用统一的 oversized Pipeline：

- 现有 Thread L0/Thread Rollup builder 已启用 `split_oversized_items=true`，实现时应扩展并验证这条现有能力，不重造第二套基础 splitter；
- Subject Memory 当前未启用该能力，必须接入同一 splitter/merge contract 后才允许推进成功状态；
- 单个输入超限时分片生成 input digest；
- 所有输入都成功形成 digest 后才能生成上层输出；
- 不允许生成“部分输入被跳过”的成功 Rollup；
- 不允许把占位文本作为成功归并结果；
- 失败时源输入保持 pending rollup 状态。

### 8.3 Subject Memory 输入 frontier

Project Summary 与 Agent Memory 只消费 canonical frontier：

- 优先消费未被更高有效 Thread Rollup 覆盖的 delta；
- 若使用 Thread Rollup，则不重复消费其覆盖的 L0；
- Project Summary 仅消费同一 `project:<id>`；
- Agent Memory 可跨项目，但只保留稳定、可复用的 Agent 协作知识；
- 每次输出写完整 derivation edges；
- Subject Memory Rollup 同样执行 lineage 去重和 oversized 完整性校验。

## 9. 结构化 Payload 与事实生命周期

### 9.1 通用 Envelope

```json
{
  "schema_version": "memory_payload_v1",
  "scope": {
    "kind": "thread | project | agent",
    "subject_id": "...",
    "project_partition_key": "..."
  },
  "current_goal": "...",
  "completed": [],
  "decisions": [],
  "open_items": [],
  "blockers": [],
  "artifacts": [],
  "constraints": [],
  "stable_preferences": [],
  "uncertainties": []
}
```

Fact：

```json
{
  "fact_id": "deterministic-or-generated-id",
  "fact_key": "storage.engine",
  "value": "sqlite",
  "status": "active | superseded | uncertain",
  "supersedes": ["fact-id-old"],
  "source_refs": ["r:01"],
  "observed_at": "...",
  "confidence": "explicit_user | tool_evidence | model_inference | uncertain"
}
```

### 9.2 Scope-specific 校验

- Thread Payload 可包含当前实现路径、临时 blocker 和本 Thread 下一步；
- Project Payload 可包含项目路径、接口、交付物和项目 blocker，但不得包含 Agent 私聊与无关个人偏好；
- Agent Payload 只保留跨项目稳定偏好、协作方式、长期约束和可复用经验，不保存项目路径和一次性任务状态；
- authorization、approval、credential、secret 和带签名 URL 不得成为跨会话授权事实；
- 原始 reasoning 不进入 Project/Agent 长期 Payload。

### 9.3 生成与验证

- 使用 Responses JSON Schema output，而不是只依赖 Prompt 要求 JSON；
- 模型 source refs 必须属于冻结输入；
- schema、scope、fact status、supersedes 和 source refs 全部服务端校验；
- 校验失败时保存受限诊断，Job 重试，不写成功派生状态；
- 合法 Payload 通过 deterministic renderer 生成统一文本投影；
- 原始模型输出只在受控审计存储中短期保留，不进入普通日志或管理平台。

### 9.4 事实投影、索引与 supersession

`structured_payload` 是不可变生成快照，不直接承担跨输出检索索引。统一 commit repository 在同一事务把合法事实投影到查询表：

```sql
CREATE TABLE engine_memory_fact_projections (
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    fact_id               TEXT        NOT NULL,
    output_kind           TEXT        NOT NULL CHECK (output_kind IN ('thread_summary','subject_memory')),
    output_id             TEXT        NOT NULL,
    scope_kind            TEXT        NOT NULL CHECK (scope_kind IN ('thread','project','agent')),
    scope_key             TEXT        NOT NULL,
    project_partition_key TEXT        NULL,
    fact_key              TEXT        NOT NULL,
    normalized_value_hash TEXT        NOT NULL,
    searchable_text       TEXT        NOT NULL,
    status                TEXT        NOT NULL CHECK (status IN ('active','superseded','uncertain','retired')),
    confidence            TEXT        NOT NULL CHECK (confidence IN (
        'explicit_user','tool_evidence','model_inference','uncertain'
    )),
    observed_at           TIMESTAMPTZ NULL,
    source_refs           JSONB       NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at            TIMESTAMPTZ NULL,
    PRIMARY KEY (tenant_id, source_id, fact_id),
    UNIQUE (tenant_id, source_id, output_kind, output_id, fact_key, normalized_value_hash)
);

CREATE INDEX engine_memory_fact_scope_idx
ON engine_memory_fact_projections(
    tenant_id, source_id, scope_kind, scope_key, status, observed_at DESC
);

CREATE INDEX engine_memory_fact_key_idx
ON engine_memory_fact_projections(
    tenant_id, source_id, fact_key, normalized_value_hash, status
);

CREATE TABLE engine_memory_fact_supersessions (
    tenant_id          TEXT        NOT NULL,
    source_id          TEXT        NOT NULL,
    newer_fact_id      TEXT        NOT NULL,
    superseded_fact_id TEXT        NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, source_id, newer_fact_id, superseded_fact_id),
    FOREIGN KEY (tenant_id, source_id, newer_fact_id)
        REFERENCES engine_memory_fact_projections(tenant_id, source_id, fact_id),
    FOREIGN KEY (tenant_id, source_id, superseded_fact_id)
        REFERENCES engine_memory_fact_projections(tenant_id, source_id, fact_id),
    CHECK (newer_fact_id <> superseded_fact_id)
);
```

要求：

- `fact_id` 由服务端生成或验证，不接受模型任意引用租户外 ID；模型只能从本次输入提供的可 supersede 候选中选择旧 fact ref；
- supersession 必须保持有向无环；提交事务用递归查询/约束检查拒绝 self-loop、cycle、跨 scope 和“旧事实 supersede 新事实”；
- output 的 polymorphic 存在性由与 derivation edge 相同的 DEFERRABLE trigger 验证；
- output retire/supersede 时，其 fact projection 在同一事务退出 active；
- `normalized_value_hash` 使用版本化 normalization + SHA-256；`searchable_text` 只保存经过 allowlist、secret scanner 和长度上限处理的非敏感检索文本，原始敏感 value 不额外复制到索引表；
- 结构化召回查询本表；词法索引以 `searchable_text` 为来源；
- 事实投影、可索引 supersession 和结构化召回必须在最终发布前同时完成，不允许省略投影表形成精简版路径。

## 10. Compose Context v2

### 10.1 API 契约

Compose 契约在同一发布窗口整体升级；预算与身份字段为必填或由服务端策略明确补齐，不保留旧条数型写法的运行分支：

```rust
pub struct ComposeContextPolicy {
    pub max_memory_tokens: usize,
    pub max_recent_record_tokens: usize,
    pub max_thread_summary_tokens: usize,
    pub max_project_memory_tokens: usize,
    pub max_agent_memory_tokens: usize,
}
```

请求上下文：

```text
query_text?                 当前目标的检索文本，只用于本次排序
context_project_subject_id 已验证 Runtime 项目身份；无项目时显式使用 null partition
current_turn_id?           排除当前已落库消息
model_capability_revision  Runtime 冻结模型能力版本
runtime_context_assertion  签名的 Agent/Thread/Project/Operation 上下文
```

`tokenizer_id` 由服务端根据受支持的 model capability 解析，不接受调用方任意指定。

### 10.2 项目信任边界

`context_project_subject_id` 必须来自 Runtime 创建时冻结的 `MemoryIdentity`，模型和工具参数不得覆盖。签名断言不是只保护 compose 的附加字段，而是所有 partition-sensitive API 的统一信任凭证。

首版签发边界使用仓库内真实组件，不假设不存在的 `project_service`：

```text
macOS/Windows authenticated client
  → Local Agent Host / Local Connector 依据 owner-scoped 本地 Project registry 与运行快照签发
internal Agent runtime
  → 已配置的内部 assertion issuer 依据服务端运行快照签发
  → Memory Engine 使用独立 key set 校验，不复用普通用户 JWT secret
```

Assertion claims 至少包含：

```text
issuer / key_id / audience
principal_id / tenant_id / source_id
canonical_agent_subject_id
thread_id / project_partition_key
allowed_operations[]          // record_ingest, compose, summarize, repair
runtime_identity_revision
issued_at / expires_at / jti
```

服务端要求：

- `/records/batch-sync`：assertion 的 tenant/source/thread/project 与整批 Record 一致，partition 由 assertion 写入正式列，忽略普通 metadata 对 partition 的修改企图；
- `/context/compose`：只允许 assertion 中的 project partition；删除 `related_subject_ids` 对 scope 的任何影响；
- active summary、repair、rebuild：必须绑定同一 partition 或由受审计 operator 明确指定；
- Thread label、Record metadata、正文、模型输出和普通 Bearer token 只能证明 tenant 身份，不能单独证明 Project/Agent/Thread 运行身份；
- assertion 绑定 operation 和 audience；写操作执行 jti 防重放，读操作至少绑定完整 scope、短 TTL 和 request nonce；
- verifier/key set 不可用时 fail closed，不降级为信任请求字符串。

Runtime Context Assertion verifier、签发端和全部调用方必须在切换前一并就绪。未携带有效 assertion 的 partition-sensitive 读写请求直接拒绝；普通公共请求不能仅凭 `project:<id>` 字符串或 `related_subject_ids` 切换项目。首轮新项目不能依赖“已有 Record”作为唯一验证依据。

### 10.3 服务端策略上限

有效值统一计算为：

```text
effective_limit = min(requested_limit, server_policy_limit, model_available_budget)
```

- 所有预算字段必须存在，并由已发布策略给出默认构造值；服务端拒绝缺字段的旧契约请求；
- 请求不能放大服务端上限；
- 检索 Pipeline 由服务端已发布 revision 唯一确定，请求不能选择检索模式；
- 旧条数限制字段从 DTO、SDK、Runtime 和客户端删除；所有候选统一进入完整预算规划；
- 所有实际生效值和 policy revision 写入 response meta。

### 10.4 候选池

1. `raw_tail`：当前 partition 尚未总结的最近原始记录；
2. `thread_continuity`：当前 partition 的 canonical Thread frontier；
3. `project_state`：当前 Project Summary frontier；
4. `agent_stable_memory`：当前 Agent Memory frontier；
5. `relevant_history`：结构化字段或词法召回命中的旧事实。

当前用户输入、System Prompt、工具 schema、未闭合工具状态和 durable Responses history 由 Runtime 持有并优先预留预算，不作为 Memory Engine 候选，也不允许 compose 裁剪或重复拉取。

候选先执行强过滤：

```text
tenant
→ source
→ canonical agent
→ verified project partition
→ status/recall_eligible
→ generation profile
→ lineage validity
```

### 10.5 排序

首版使用确定、可解释的混合评分：

```text
score = scope_match
      + thread_continuity
      + structured_fact_match
      + lexical_relevance
      + recency
      + open_item_or_blocker_weight
      + explicit_reference_weight
      - superseded_penalty
      - duplication_penalty
```

身份和项目不匹配是过滤条件，不是可被其他分数抵消的权重。

### 10.6 去重顺序

1. lineage ancestor/descendant overlap；
2. 相同直接 source refs；
3. `fact_key + normalized value + scope`；
4. 明确 `supersedes`；
5. 相同 content hash；
6. 经离线评测和全量回归验证后的近似文本去重。

跨 scope 去重只减少重复渲染，不改变事实所有权。Project 当前事实优先于 Agent 通用记忆；当前输入和当前工具证据优先于所有历史派生内容。

### 10.7 Block 与 Response Meta

每个可解释候选作为独立 block 或保留独立 item metadata：

```text
block_id
block_type
pool
subject_id
project_partition_key
level
source_refs / derivation root IDs
estimated_tokens
score components
selection_reason
truncated
```

Response meta：

```text
policy_revision
budget_requested
budget_effective
budget_used
estimator_id
candidate_count_by_pool
selected_count_by_pool
dropped_count_by_reason
deduplicated_count
needs_compaction
oldest_pending_record_at
project_context_verified
```

## 11. 预算分配与 Runtime 请求前预检

### 11.1 模型能力快照前置契约

M4b 完整预算强制模式启用前，User Service 模型配置、Local Agent Host durable snapshot、`chatos_ai_runtime::ModelRuntimeConfig` 以及 macOS/Windows 直接 Runtime 必须通过 M4a 统一携带：

```text
model_capability_revision
context_window_tokens
max_output_tokens
output_reserve_tokens
protocol_overhead_estimator_id
tokenizer_id?                 // 不支持时使用 conservative estimator
supports_responses
responses_compaction_threshold?
```

约束：

- `context_window_tokens` 和 `output_reserve_tokens` 来自已发布模型能力目录并随运行冻结，不能根据 model name、provider 文档或 compaction threshold 在 Runtime 内猜测；
- `responses_compaction_threshold` 是 Provider 轮内压缩软阈值，不等于 context window，也不能替代 output reserve；
- `max_output_tokens <= output_reserve_tokens < context_window_tokens`；无显式 `max_output_tokens` 时仍必须有服务端默认 reserve；
- capability revision 与 model/provider/base URL 的生效 revision 一起写入 durable run snapshot；运行中配置刷新不得改变当前 run；
- Memory Engine 只接受已发布的 capability revision，并把请求预算再限制到服务端策略上限；
- 缺失或未知 revision 时硬预算 fail closed，不允许退回无完整预算预检的旧行为。

需要同步修改：User Service 模型 DTO/存储与管理页、Local Agent Host interface/database migration/control-plane mapping、`crates/chatos_ai_runtime` model/runtime request、macOS connector runtime DTO、Windows Runtime 对应模型快照和测试。

### 11.2 Memory Budget

Configuration Center 为各 pool 提供：

```text
min_tokens
target_tokens
max_tokens
priority
```

算法：

1. Runtime 从冻结模型配置取得 context window、output reserve 和协议能力；
2. Runtime 估算 System Prompt、工具 schema、当前输入、轮内历史和协议开销；
3. Runtime 将剩余 Memory Budget 传给 compose；
4. compose 在服务端上限内为必需池分配 minimum；
5. 其余池按 priority 分配 target，空余预算可借用但不得超过 max；
6. 超预算时先处理 lineage overlap、superseded 和 exact duplicate；
7. raw tail 超预算时返回 `needs_compaction=true`；
8. 必需内容本身超硬上限时明确失败，不删除当前用户输入、系统约束或工具状态伪造成功。

### 11.3 Token 计算

```text
Record 写入、后台扫描：带 ID 的保守启发式估算
Compose 最终选择：服务端支持的 tokenizer；否则保守 estimator
Provider 发送前：Runtime 对最终序列化请求再次预检
```

估算器必须有稳定 ID 和测试误差边界。对于无法本地精算的 provider/model，不声明“精确 token”，使用安全系数和 Provider overflow 兜底。

### 11.4 完整请求预检

Runtime 计算：

```text
instructions / system prompt
+ tool schemas
+ Memory blocks
+ raw records
+ current input
+ durable Responses history
+ provider protocol overhead
+ configured output reserve
```

处理结果：

- 未超软阈值：直接发送；
- 超软阈值且有可压缩 pending raw tail：触发一次 active summary，完成后重新 compose 和重新计算；
- 每次压缩必须使输入 token 或 pending count 有明确下降；
- 无改善或达到 `max_compaction_attempts`：暂停并返回明确错误；
- 超硬阈值：不发送 Provider；
- Provider 仍返回 overflow：保留当前单次有界恢复路径。

轮内 Responses compaction 继续由冻结模型配置控制；Memory Engine active summary 只处理跨轮 raw tail，两者不互相替代。

## 12. 词法与语义召回

### 12.1 结构化召回

先按 fact key、status、scope、project、时间、open item 和显式引用检索，不依赖全文或向量。

### 12.2 PostgreSQL 词法召回

中文不能只依赖 PostgreSQL 默认英文 FTS。上线前在以下实现中选定并固化一种：

- `pg_trgm` + 规范化文本；
- 经批准的中文分词扩展；
- 应用层中英文 tokenizer 生成 `tsvector`/lexeme 列。

要求：

- 中英文分别评测 recall/precision；
- 索引或其必需 revision 不可用时拒绝 compose 并告警，不切换到结构化+时间的低保证替代路径；
- 查询始终先应用 tenant/source/agent/project 强过滤；
- query text 不写入长期 Memory，不进入普通日志。

### 12.3 Embedding

Embedding 不属于本次发布路径，也不得以运行时开关、备用检索或隐藏实验代码进入发布包。后续只有在独立方案和固定评测证明相对当前结构化+词法 Pipeline 有显著收益后，才能以另一次完整替换发布实施。向量相似度不能绕过强过滤，也不能成为事实来源。

## 13. Prompt、Pipeline 与配置治理

### 13.1 生效指令模型

每次生成由两类版本化内容共同决定：

```text
Managed Agent Prompt
+ Pipeline Directive / JSON Schema
```

- Managed Prompt 负责各类 Memory 的语义边界；
- Pipeline Directive 负责 leaf/merge、source refs、结构化输出和安全约束；
- 固定 System Prompt 只保留协议级最小约束，并纳入 pipeline checksum；
- 所有硬编码 leaf/merge directive 迁入集中、可版本化的 pipeline profile；
- 每份输出记录两类 revision/checksum。

旧 `EngineJobPolicy.summary_prompt*` 和 Prompt 生成接口从公开配置模型、管理平台、SDK 和运行代码中直接删除，不保留只读兼容模型或解析分支。

### 13.2 Configuration Center

沿用现有命名体系：

```text
memory_engine.policy.summary
  dispatch_threshold_tokens
  max_source_batch_tokens
  max_model_input_tokens
  max_leaf_tokens
  max_merge_rounds

memory_engine.policy.rollup
  dispatch_threshold_tokens
  max_source_batch_tokens
  max_model_input_tokens
  count_limit
  keep_level0_count
  max_level
  partition_by_project = true

memory_engine.policy.subject_memory
  dispatch_threshold_tokens
  max_source_batch_tokens
  max_model_input_tokens
  count_limit
  keep_level0_count
  max_level

memory_engine.compose
  max_memory_tokens
  max_recent_record_tokens
  pool.<name>.min_tokens
  pool.<name>.target_tokens
  pool.<name>.max_tokens
  pool.<name>.priority
  retrieval_pipeline_revision
  max_compaction_attempts
```

Configuration Center 只发布不可变策略、Prompt、Pipeline、模型能力和 tokenizer revision，不发布运行时功能开关。新链路是唯一实现，服务启动时必须完整加载全部必需 revision，缺一项即启动失败；request/job meta 记录实际 revision。管理平台显示“已发布、实例已加载、全量前置校验通过”三种状态。

依赖关系作为发布前静态校验和启动时硬校验执行，不允许按 tenant/source 绕过：generation commit 依赖 partition dispatch；coverage 依赖 generation commit 与全部 source contract ready；project rollup 依赖 coverage 与 Runtime Context Assertion verifier；compose 依赖历史修复与模型能力快照；structured payload 依赖 generation commit；lexical retrieval 依赖 structured payload 与 fact projection ready。任何依赖未满足都中止整体发布。

不恢复已退休的 `target_summary_tokens`。Runtime 的 context window、output reserve、Responses compaction threshold 和模型能力使用 11.1 补齐后的 Agent Runtime 不可变模型快照，不在 Memory Engine 下建立第二套值；当前只含 compaction threshold 的旧快照不满足 v2 硬预算前置条件。

三个 token 字段不得复用语义：`dispatch_threshold_tokens` 只决定何时请求后台生成，`max_source_batch_tokens` 只限制一次冻结多少源输入，`max_model_input_tokens` 才是 Pipeline 单次模型请求硬上限。`max_model_input_tokens` 必须小于等于该 generation 冻结模型快照的输入能力上限减去协议、Prompt 和输出 reserve；配置中心不能把它提升到模型能力之外。

配置要求：

- revision/checksum 可审计；
- API、Worker 和 Runtime 记录实际加载 revision；
- 启动时缺失必需配置则对应能力 fail closed；
- 运行期无法加载当前已发布 revision 时停止接收新 compose/generation Job 并告警，不沿用旧 revision；
- 管理平台区分已发布、服务已加载和请求实际生效值；
- Worker 长期 revision 漂移必须告警。

## 14. 历史迁移

### 14.1 迁移原则

- 先只读盘点和备份；
- 每批迁移有 job id、租户范围、游标、统计和恢复日志；
- 小事务、可重入、`FOR UPDATE SKIP LOCKED`；
- 不物理删除 Record、Summary 或 Subject Memory；
- 无法证明正确性的派生数据退出 recall，源输入恢复待重建。

历史修复在同一维护窗口内按两个依赖步骤执行，任一步未完成都不得恢复生产流量：

- M2a 只做不依赖 project-aware Rollup 的 coverage/partition 回填、坏数据隔离和 pending 恢复；
- M3b 必须等 M3a 的可信项目上下文和 partition-aware Rollup 代码及测试完成后，才重建旧跨项目 Rollup、Project Summary 和 Agent Memory；M3a 不单独投产。

禁止在 M3a 前以旧 Thread 全局 Rollup 实现执行“按 partition 重建”。

### 14.2 Coverage backfill

优先级：

1. 使用真实 `engine_records.summary_id` 精确关联；
2. 排除 `skipped_single_record_token_limit` 等非 Summary sentinel；
3. 使用 tenant/source/thread、start/end、created_at/id 顺序和 count 进行唯一推断；
4. 唯一推断结果先写 migration audit candidate，不写入正式 coverage 表；经确定性校验确认为 `confirmed` 后才写 active coverage；
5. 无法确认的 Record 恢复 pending 并重建；
6. 重建成功后再退休不可信旧 Summary。

### 14.3 旧 Repair 数据

- 单一 partition 且来源可证明：迁移为 canonical Thread Summary，写 coverage 和 `generation_reason=migration_rebuild`；
- 跨项目或来源不确定：旧 Summary 退出 recall，源 Record 按 partition 恢复 pending；
- 旧 Repair 派生出的 Rollup/Subject Memory 根据 lineage 审计结果退休并重建。

### 14.4 旧 oversized skip

```text
定位 sentinel
→ 清除伪 summary 关联
→ 恢复 pending
→ 重算 partition/thread pending 统计
→ 触发 oversized-aware rebuild
→ 新 coverage 成功后完成迁移
```

### 14.5 旧跨项目 Rollup

旧混合 Rollup 与其下游派生结果设置：

```text
recall_eligible=false
retired_reason=cross_project_rollup_v1
```

从 confirmed L0 coverage 按 partition 重建。重建完成前继续禁止动态项目线程召回 top-level Thread Rollup。

该重建属于 M3b，不属于 M2a。M2a 只负责把混合 Rollup 退出 recall、保留审计并把 confirmed L0 标记为等待 partition rebuild。

### 14.6 Subject Memory lineage

- 能由 source digest 和现有状态唯一证明的派生关系回填为 confirmed；
- 同时消费祖先和后代的 Subject Memory 标记为待重建；
- 无法证明来源或 scope 的 Memory 退出 recall；
- Agent Stable Memory 和 Project Summary 分别重建，不能互相替代。

## 15. 可观测性、管理平台和安全

### 15.1 指标

```text
memory_pending_records_total{partition_state}
memory_pending_oldest_age_seconds
memory_unprocessable_source_units_total{kind,status}
memory_summary_jobs_total{reason,status}
memory_active_coverage_ratio
memory_broken_derivation_edges_total
memory_unreachable_summarized_records_total
memory_cross_project_rollup_rejected_total
memory_generation_replay_total{result}
memory_derived_outbox_total{event_kind,status}
memory_derived_outbox_oldest_age_seconds{event_kind}
memory_runtime_assertion_total{operation,result}
memory_source_contract_readiness_total{client_kind,state}
memory_model_capability_precheck_total{result}
memory_privileged_erasure_total{status}
memory_compose_candidates_total{pool}
memory_compose_selected_total{pool}
memory_compose_dropped_total{reason}
memory_compose_tokens{pool}
memory_compose_deduplicated_total{reason}
memory_compose_needs_compaction_total
memory_provider_context_overflow_total
memory_policy_revision_loaded
```

### 15.2 Trace 与日志

每次 Job/compose 携带：

```text
tenant hash
source_id
thread_id hash
agent subject hash
project partition hash
job_run_id / generation_id / request_id
policy revision
prompt revision
pipeline revision
model capability revision / estimator id
assertion issuer / key id / jti hash / verification result
candidate/selected/dropped counts
budget requested/effective/used
```

日志不得输出凭据、完整私聊、query text、原始 reasoning、大段 Tool output、签名 URL 或结构化敏感 payload。

### 15.3 管理平台

- Record → active/history coverage；
- Summary/Memory → 直接输入和完整 lineage；
- generation reason/profile、Prompt/Pipeline/模型版本；
- recall eligibility、退休原因和 rebuild 状态；
- project partition Rollup；
- compose dry-run，只展示 ID、类型、分数、token 和选择原因；
- oversized、unprocessable、rebuild 和 dead-letter 队列；
- derived event Outbox、source contract readiness、Runtime assertion verifier/key revision 和 model capability readiness；
- retire、恢复 pending、rearm、resummarize 的受控操作；
- privileged erasure 只提供独立高权限入口、资源预览和双重审计，不与普通 retire 按钮混用；
- 所有写操作记录操作者、原因、范围和前后状态。

### 15.4 安全

1. Summary 前执行 secret/credential 检测与字段 allowlist；
2. Tool output 默认只处理允许的文本字段；
3. 媒体、base64 和签名 URL 不进入长期文本；
4. Runtime Context Assertion 使用独立签名 key set，绑定 tenant/source/agent/thread/project/operation，有短有效期、受众、jti 和 key rotation；
5. 历史 authorization/approval 永不转换成当前权限；
6. 管理平台默认不展示原始敏感输入；
7. embedding、FTS 索引与原始数据共享 tombstone 生命周期。
8. 管理日志只记录 assertion issuer/key id/jti hash/scope hash 和验证结果，不记录原始 token；
9. privileged erasure 使用独立 operator scope、双重审计和明确资源清单，不复用普通 delete endpoint。

## 16. 一次性改造工作包

M0—M7 仅用于安排代码、迁移和测试的依赖顺序，不是灰度阶段、兼容周期或独立发布版本。本分支必须完成全部工作包后统一测试、统一提交、统一推送；生产只允许第 20 章定义的一次停写切换。任一工作包未完成时，禁止把当前代码视为可运行交付物。

### M0：基线、只读审计与全量切换门禁

交付：

- 建立 partition dispatch、generation commit、coverage、project rollup、compose、structured payload 的全量发布前置检查；
- 在 Configuration Center 建立正式策略 revision、依赖关系和全局 readiness 门禁，不建立功能开关或 tenant/source allowlist；
- 只读审计 Repair、oversized sentinel、高层 oversized skip、无覆盖 summarized Record、跨项目 Rollup、重复祖先/后代派生；
- 按 Rust Runtime、Local Agent Host、macOS、Windows 和内部调用方盘点 Source Unit/Assertion/model capability readiness；
- 固化 compose token、Provider overflow、Job 重放和队列积压基线；
- 增加 P0 回归测试；未满足全部工作包退出条件前不提交、推送或发布任何中间写路径。

退出条件：能按租户和 partition 准确列出全部待迁移对象。

### M1a：typed partition、写入原子性与 fail-closed

交付：

- migration `0011_memory_coverage_and_lineage.sql` 的 typed Record partition 和 Thread Summary Partition 状态表；
- Record ingest、pending 统计、terminal forced dispatch 和数据库派生事件 Outbox 同事务；
- `engine_derived_event_outbox`、publisher lease、幂等 consumer、dead-letter/rearm；
- 单一 partition writer 与 claim；删除旧 Thread 全局 writer；
- 先将 L0、Thread Rollup、Subject Memory 的 skipped-but-success 改为失败关闭：源输入保持 pending，停止继续丢数据；
- partition claim、Rollup lease 与 poison isolation。

退出条件：Record 提交后不存在漏计 pending/forced dispatch；仓库中只有一个 partition writer；所有 oversized 跳过均不能推进成功状态。

### M1b：Generation manifest、step 状态机与统一提交

交付：

- 持久化 generation input manifest、config snapshot、attempt/error 和 lease；
- 统一 commit repository 与跨输出表 constraint trigger；
- Subject Memory 不可变 generation 版本和 active logical key 唯一约束；
- L0/派生输出、状态和数据库派生事件 Outbox 的最终 CAS 原子提交；
- callback 重复、乱序、进程崩溃和 lease 接管幂等。

退出条件：重复执行只产生一个逻辑输出；重放使用同一冻结输入和配置；任一提交步骤失败均可安全重试。

### M1c：coverage、lineage、重建与 tombstone

交付：

- Record coverage、derivation edges；
- 多 tool-call Source Unit，processing coverage 与 fact provenance 分离；
- pending rebuild 和 covered-range resummarize；
- Record/Summary/Memory 普通物理删除改 tombstone；privileged erasure repository 与删除矩阵。

退出条件：新数据 broken active coverage/lineage 为 0；退休和重建无 coverage 空窗；模型引用不会改变 processing coverage。

### M1d：oversized 完整处理与 poison isolation

交付：

- 复用现有 L0/Thread Rollup splitter 并补齐 Subject Memory；
- Record/Summary/Memory 统一 leaf/merge 完整性校验；
- 不可处理 Source Unit dead-letter、rearm 和 partition 隔离。

退出条件：新路径不再产生 skipped-but-success；超大唯一事实可达；一个项目的毒丸不阻塞同 Thread 其他项目。

### M2a：历史 coverage 回填与坏数据隔离

交付：

- coverage/lineage backfill；
- 旧 Repair 和 oversized sentinel 恢复 pending/rebuild；
- 旧跨项目 Rollup 和不可信 Subject Memory 退出 recall，并登记 partition rebuild backlog；同一维护窗口内只允许在新 partition-aware Rollup 完成后执行重建，旧 Rollup 实现不得运行；
- 迁移报告、游标、可逆状态日志；
- 管理平台迁移视图。

退出条件：所有旧 summarized 状态均有 confirmed active coverage，或已恢复 pending/rebuild；所有无法证明 partition/lineage 的高层输出已退出 recall，并能按 partition 列出 M3b backlog。

### M3a：Project-aware Rollup 与可信项目上下文

交付：

- Rollup selector、digest、锁和索引按 partition；
- Runtime Context Assertion 的签发、key rotation、Memory Engine verifier 和内部等价验证；
- Record ingest、compose、summary/repair 全部使用同一可信 partition；
- macOS、Windows、Local Agent Host 和内部 Agent Runtime 完成 assertion/Source Unit contract 升级；
- 动态项目线程安全召回 top-level Rollup；
- Project/null 隔离 contract tests。

退出条件：任何 compose 和派生结果都不存在其他项目的 Thread/Project 内容。

### M3b：跨项目派生结果重建

交付：

- 从 M2a confirmed L0 coverage 按 partition 重建 Thread Rollup；
- 重建 Project Summary 和 Agent Stable Memory，写完整 derivation edges；
- 旧混合 Rollup、重复 ancestor/descendant Memory 和无来源 Memory 保留 retired 审计；
- 每批重建有 generation manifest、配置快照、游标和可恢复日志。

退出条件：所有旧 rolled-up/subject-memory 成功状态均有 confirmed active lineage，或已恢复 pending/rebuild；动态项目线程的历史高层派生不再包含混合 partition。

### M4a：模型能力快照

交付：

- User Service 模型能力字段、校验、发布 revision 和管理页；
- Local Agent Host durable model snapshot/database migration；
- `chatos_ai_runtime`、macOS、Windows Runtime 冻结 capability revision；
- tokenizer/estimator registry、协议开销 estimator 和误差测试；
- 缺少能力字段的旧运行升级或终止，不提供降级状态。

退出条件：所有生产 Runtime 均能提供已发布且冻结的 context window、output reserve 和 estimator revision；Runtime 不根据模型名或 compaction threshold 猜测窗口。

### M4b：Compose v2 与 Runtime 完整预算

交付：

- SDK 强制升级契约；
- canonical frontier、候选池、服务端 cap、预算分配；
- Runtime 完整请求预检和有进展 compaction；
- response block/meta；
- 单一 compose 实现及全量集成测试。

退出条件：

- 所有最终请求满足硬预算；
- Tool exchange 不被拆坏；
- 正常流量不依赖 Provider overflow 做常规压缩；
- 所有已升级客户端无隔离和可用性回归。

### M5：结构化 Payload、supersession 与去重

交付：

- JSON Schema output；
- scope-specific validator；
- deterministic renderer；
- fact projection、supersession edge 和结构化查询索引；
- source ref 校验、fact key、confidence 和 supersedes；
- lineage/fact/content-hash 去重。

退出条件：旧事实能被新事实明确覆盖；跨层重复只渲染一次，同时保留完整 provenance。

### M6：相关性召回

交付：

- 结构化检索；
- 中文可用的 PostgreSQL 词法索引；
- 可解释评分；
- 中英文离线评测集；
- 发布包不包含 embedding 运行时代码、开关或备用路径。

退出条件：历史决定、偏好变更、项目切换和旧 blocker 场景优于固定最近窗口，且无 scope 隔离回归。

### M7：平台、告警和旧路径清理

交付：

- coverage/lineage/dry-run/rebuild 管理视图；
- Dashboard、告警、Runbook；
- 删除旧 Repair type 写路径、所有 oversized skip 成功路径和旧 Prompt 配置；
- 删除生成路径中的 Subject Memory 原地 upsert、普通物理 DELETE repository 和未验证动态 project scope；
- 删除全部旧写、旧读和旧 API 路径；
- 生产验收报告。

## 17. 代码改动地图

| 模块 | 主要改动 |
| --- | --- |
| `memory_engine/backend/migrations/postgres` | 0011 typed partition、partition 状态、derived event Outbox、generation registry、coverage、lineage、事实投影、版本/删除约束、索引 |
| `services/summary/selectors.rs` | Source Unit、partition 选择、poison isolation |
| `services/summary/thread_summary/*` | oversized Pipeline、generation identity、原子提交 |
| `services/summary/thread_repair/*` | pending rebuild、resummarize、canonical output |
| `services/summary/rollup/*` | project partition、完整输入、lineage |
| `services/subject_memory/*` | canonical frontier、oversized、lineage、结构化输出 |
| `services/ai_pipeline/*` | 内容感知分片、JSON Schema、source ref 校验、版本化 directive |
| `repositories/records/*` | typed partition、不可变内容身份、coverage 状态、partition claim、tombstone/privileged erasure、恢复 pending |
| `repositories/threads/dispatch.rs`、`summary_queue.rs` | dispatch envelope/版本/强制总结按 partition 迁移 |
| `repositories/summaries/*` | typed filters、coverage/lineage 事务、tombstone |
| `repositories/subject_memories/*` | 不可变 generation 版本、lineage、structured payload、fact projection、recall eligibility |
| `services/context/*` | frontier、Planner、预算、排序、去重、meta |
| `crates/memory_engine_sdk/src/models` | compose 强制升级契约 |
| `crates/chatos_ai_runtime/src/memory_context` | query/current turn/runtime assertion、Source Unit 字段、block/meta |
| `crates/chatos_ai_runtime/src/runtime` | 模型能力快照、完整请求预检、有进展 compaction、硬上限 |
| `user_service` | 模型 context window/output reserve/capability revision 的配置、校验、发布和内部 DTO |
| `clients/local_agent_host` | durable model capability migration、冻结 MemoryIdentity、Source Unit、Runtime Context Assertion 签发/传递 |
| `local_connector_service` | 依据 owner-scoped project binding 提供 assertion issuer/key rotation 或受信验证输入；不得假设独立 project_service |
| `clients/macos/Sources/ChatOSAPI/ChatOSMemoryEngineService.swift`、相关 Runtime DTO | Record/compose assertion、turn/exchange contract、compose v2 block/meta、模型能力快照 |
| `clients/windows-react/src-tauri/src/story_studio/memory.rs`、相关 Runtime | Record/compose assertion、turn/exchange contract、compose v2 block/meta、模型能力快照 |
| `agent/src/implementations/memory_engine.rs` | managed Prompt 身份与 Pipeline profile 绑定 |
| `config_center_service` | 新策略、退休旧 Prompt/target keys |
| `admin_console` | coverage、lineage、dry-run、迁移和受控 Repair |

## 18. 测试与评测

### 18.1 单元测试

- 中文、英文、Markdown、代码、递归 JSON、Tool output 分片；
- UTF-8 边界、chunk 顺序和 source ref；
- Tool call/result 原子分组与 orphan 检测；
- 一个 assistant Record 含多个 tool calls/results 时只形成一个完整 Source Unit；
- Rust Runtime、Local Agent Host、macOS 和 Windows 产生相同语义的 turn/exchange/call identity；旧客户端请求明确拒绝；
- processing coverage 由冻结输入确定，fact provenance 缺失或非法时按独立规则失败；
- project/null partition；
- structured schema、scope validator、supersession；
- canonical frontier 和 lineage 去重；
- Budget allocator 与硬上限；
- ranking 确定性。
- fact projection normalization、fact ID、supersession cycle 拒绝和 scope 索引选择；
- model capability revision、context window、output reserve 与 compaction threshold 不混用。

### 18.2 PostgreSQL Contract Tests

- Summary、coverage、Record、Job、claim、数据库派生事件 Outbox 同事务；
- derived event Outbox claim/lease 接管、重复发布、乱序消费、dead-letter/rearm；
- Record commit 与 partition pending/terminal forced dispatch 同事务，提交后崩溃重放不漏计也不重复计数；
- 事务每一步失败全部回滚；
- generation callback 重复/乱序、`running → committed` CAS 竞争；
- generation manifest/config snapshot 与模型调用、重放完全一致；配置变化必须创建新 generation；
- active coverage 唯一，历史 superseded 可保留；
- Subject Memory 同 logical memory key 只允许一个 active 版本，新 generation 不原地覆盖；
- retired Summary 恢复源 Record；
- Rollup/Subject Memory derivation edge 完整；
- polymorphic edge/fact projection constraint trigger 拒绝不存在、跨 tenant/source、scope/level/partition 不匹配的输入输出；
- 普通删除只 tombstone；privileged erasure 按顺序清除 fact/edge/coverage/output/source 且不留下索引残片；
- 跨项目候选拒绝；
- Record 幂等重放若试图改变 immutable partition 必须整笔拒绝；
- 仓库和发布产物中不存在旧 writer 符号、路由、claim SQL 或可执行入口；同一 partition 只有单一 writer；
- poison partition 不阻塞其他 partition；
- backfill/rebuild 重跑幂等；
- tenant/source/agent/project 越权拒绝。

### 18.3 Runtime 集成测试

- 当前 turn 已落库但不重复注入；
- durable Responses history 替换旧 Memory block；
- current run 未闭合工具状态不从跨轮 Memory 注入；
- macOS/Windows/Local Agent Host assertion 与 Source Unit contract 端到端一致；
- raw tail 超预算触发 active summary；
- compaction 无改善时停止；
- tokenizer/estimator 两种路径；
- Provider overflow 兜底只执行一次；
- Runtime Context Assertion 无效、过期、跨租户和重放均拒绝；
- Assertion 的 source/agent/thread/project/operation/audience 任一不匹配均拒绝；Record metadata 不能覆盖 assertion partition；
- capability revision 未知或缺失时硬预算 fail closed，不允许降级；
- Memory Engine 不可用时按当前生产策略明确失败，不引入未授权缓存回退。

### 18.4 场景评测

1. 用户偏好被后续明确修改；
2. 项目 A 切到 B 再返回 A；
3. 旧架构决定被新决定替代；
4. 三个月前相关决定不在最近窗口；
5. 相同事实存在于 Thread、Project、Agent；
6. 超大 Tool output 含唯一关键结论；
7. 超大 Thread Summary 和 Subject Memory 继续归并；
8. Repair 后旧 Summary 退出 recall，新 Summary 生效；
9. 一个项目 poison Record 不阻塞其他项目；
10. Worker 延迟造成大量 pending；
11. 历史批准不能批准当前危险操作；
12. Agent 私人偏好不进入 Project Summary；
13. 项目路径和一次性 blocker 不进入 Agent Memory；
14. 中英文旧事实词法召回；
15. ancestor/descendant 不重复注入。

### 18.5 故障注入

- 模型超时、空输出、非法 JSON、伪造 source ref；
- Cloud Agent callback 重复、乱序和延迟；
- 模型成功后、事务前崩溃；
- 事务提交后、callback ack 前崩溃；
- Outbox 重复投递；
- claim 过期和 Worker 接管；
- 配置 revision 漂移；
- tokenizer/FTS 不可用时 compose fail closed，且不执行替代检索路径；
- Runtime Context Assertion verifier 不可用时 partition-sensitive 请求 fail closed，且不接受未验证 scope。

## 19. 验收标准

### 19.1 正确性

- `unreachable_summarized_records_total = 0`；
- `broken_active_coverage = 0`；
- `broken_active_derivation_edges = 0`；
- 新产生 skipped-but-success 记录为 0；
- 新产生 `summary_type=thread_repair` 为 0；
- 跨项目召回与派生错误为 0；
- 重复 generation 的多 active output 为 0；
- Tool exchange 配对错误为 0；
- 同 logical Subject Memory key 的多 active version 为 0；
- derived event Outbox 已提交业务事件漏发布为 0，重复/乱序发布不产生重复输出；
- 普通删除绕过 tombstone 和 privileged erasure 遗留派生/索引数据均为 0；
- 所有迁移与 callback 重放幂等。

### 19.2 质量

- 固定评测集正确事实召回不低于基线；
- 项目切换、事实覆盖、旧决定召回全部通过；
- 中英文词法召回分别达到 M0 后确定的目标；
- 去重显著减少 Memory token，关键事实不减少；
- 结构化解析、source ref 或 scope 校验失败不产生成功状态。
- 所有请求 100% 携带已验证 capability revision，不使用模型名或 compaction threshold 推断窗口。

### 19.3 运维

- 任意 Record 可查看 active/history coverage；
- 任意 Summary/Memory 可查看完整 lineage；
- 任意生成物可反查 Prompt、Pipeline、模型和策略版本；
- 任意 partition-sensitive 写入/召回可反查 assertion issuer、key id、runtime identity revision 和已验证 scope，不保存原始 bearer/assertion；
- compose dry-run 可解释每个候选；
- Repair/rebuild/retire/rearm 均有审计；
- 告警覆盖 backlog、poison、coverage、lineage、跨项目候选、revision 漂移和 overflow 激增。

具体百分比、延迟和成本 SLO 在 M0 采集基线后写入发布验收，不在方案中使用无依据数值。

## 20. 一次性发布、切换和故障恢复

### 20.1 发布前硬门禁

以下条件必须全部满足，否则不进入发布窗口：

- 数据库 migration、backfill、coverage/lineage 审计和 rebuild 在生产快照副本完整演练通过；
- Rust Runtime、Local Agent Host、macOS、Windows 和所有内部调用方均已升级新契约；
- Runtime Context Assertion、model capability snapshot、tokenizer/estimator revision 全链路就绪；
- 新写路径、compose、结构化 Payload 和检索的单元、PostgreSQL contract、Runtime 集成与离线评测全部通过；
- 仓库内旧 writer、旧 compose、旧 Repair type 写入、原地 Subject Memory upsert 和普通物理删除路径已经删除；
- 发布包不包含 Feature Flag、allowlist、shadow/inject、双写或旧协议降级分支。

### 20.2 一次性切换顺序

```text
进入维护窗口并停止 Memory 写入/后台派生
→ 备份与记录迁移游标
→ 执行 schema migration、历史 backfill、坏数据隔离和按 partition rebuild
→ 运行全量一致性审计，要求所有硬不变量通过
→ 同时部署服务端、SDK/Runtime 和全部客户端新版本
→ 启动单一新 writer 与单一 compose 实现
→ 执行端到端 smoke、重复 callback、崩溃接管、预算和跨项目隔离验证
→ 验证通过后恢复写入和后台任务
```

M2a 与 M3b 必须在恢复生产流量前完成；不能让新 Planner 消费未治理的旧派生数据。M3b 不得早于 project-aware rollup 和 assertion verifier。

### 20.3 发布控制与修复身份

发布自动化不得要求操作员在服务器上输入管理员密码、复制浏览器登录态或临时签发用户 JWT。业务用户 JWT 只表达用户会话，不承担服务间运维授权。正式 replay、rearm 和受控修复统一使用以下单一路径：

```text
release/repair command
  → Configuration Center 内部控制面
  → mTLS 校验 configuration-center 服务身份
  → 请求体摘要 + caller + audience + operation + timestamp + nonce 的短时签名
  → 目标服务按 operation/scope 执行 CAS 修复
  → Configuration Center 审计 + 目标服务 operator audit
```

- 服务私钥只由部署 secret 管理，发布脚本不生成、打印或传递可复用 Bearer token；
- 签名必须绑定请求体、目标服务、操作、tenant/source/partition/version 和短 TTL，nonce/JTI 单次消费；
- 目标服务只接受明确 allowlist 的服务调用方和操作，不能把服务签名兑换成通用管理员身份；
- replay/rearm 必须调用正式接口并执行版本/CAS 校验，禁止直接 SQL 改 dead-letter、backlog 或 Cloud Agent 状态；
- 任一认证、签名、重放或审计写入失败都 fail closed，并保持公网写关闭；
- 上述通道是唯一正式修复通道，不保留人工 JWT 备用路径或无审计旁路。

### 20.4 故障恢复

- 切换前失败：数据库事务回滚，保持维护停写，修复后重新执行；
- 切换后 smoke 失败：立即停止写入和 Worker，整体恢复切换前数据库与应用快照；恢复完成并通过一致性审计前不开放流量，绝不混跑新旧版本；
- Worker 中止或 Job 超时：由新版本恢复器释放过期 partition lease，并把终态 Summary Job 遗留且无 active coverage 的 Record 恢复为 pending；禁止直接 SQL 修改 Record、Job 或 backlog 状态；
- 已提交的新格式数据不得交给旧 writer 继续处理，禁止通过运行时开关、旧 endpoint 或旧 compose 局部降级；
- coverage/lineage、typed columns、generation registry 和 tombstone 历史必须保持一致；恢复操作只能整体执行并再次通过全量审计；
- 不允许恢复 oversized skip、跨项目 Rollup、Repair 旁路写入、无 assertion scope 或无硬预算预检的任何旧行为。

## 21. 最终完成定义

以下条件全部满足才算完成：

1. Record、Summary 和 Subject Memory 全链路没有静默跳过。
2. 所有成功派生状态都有 confirmed active coverage/lineage。
3. 重复 callback、进程崩溃和 Outbox 重放不会产生重复逻辑输出。
4. Repair/rebuild 能版本化替换错误派生数据并保留历史。
5. 一个项目的 poison 输入不会阻塞同 Agent Thread 的其他项目。
6. Thread Rollup、Project Summary 和 compose 不跨项目污染。
7. 每轮模型输入在发送前满足完整预算并可解释。
8. 结构化事实能表达 scope、source、confidence 和 supersession。
9. 中英文相关性召回通过固定评测，embedding 不是基础正确性的依赖。
10. Prompt、Pipeline、模型和策略版本均可重现。
11. 历史迁移可审计、可重跑、可恢复，未物理删除用户历史。
12. 旧 `thread_repair` 写路径、所有 skipped-but-success 路径、退休 Prompt 字段和退休 token 配置已移除。
13. Record、compose、summary 和 repair 的 project partition 均来自已验证 Runtime Context Assertion 或等价内部冻结 scope。
14. Rust、Local Agent Host、macOS 和 Windows 的 Source Unit/compose v2 契约均通过集成测试。
15. 完整请求硬预算使用已发布且冻结的 model capability revision；旧快照不会被误报为已预检。
16. 普通 tombstone、版本 supersession 和 privileged erasure 各自具备明确且通过测试的生命周期。

## 22. 本分支实施验收记录（2026-11-09）

`3.0.9.7` 已按本方案完成一次性硬切换实现。发布代码中不保留 Feature Flag、tenant/source allowlist、shadow/inject、双写、旧协议降级、旧 Repair 写入口或旧 Prompt/策略字段。新契约缺失、配置不完整、模型能力未知、Runtime Context Assertion 无效或派生关系无法证明时统一 fail closed。

已完成的代码验收包括：

- typed project partition、partition 单 writer、数据库派生事件 Outbox、generation registry、coverage、lineage、fact projection、supersession、tombstone 和 privileged erasure；
- pending rebuild 与 covered-range resummarize 的独立入口，以及 claim、Job、operator audit 和最终 generation commit 的原子边界；
- Thread L0、Thread Rollup、Subject Memory L0/Rollup 的版本化 Pipeline profile、结构化 Payload、source ref 校验和 oversized 完整处理；
- Compose Context v2、canonical frontier、中文词法召回、完整 token budget、冻结 model capability revision 和请求前硬预算预检；
- Runtime Context Assertion 在服务端、SDK、Local Agent Host、macOS 和 Windows 调用链中的强制绑定；
- Configuration Center shared Memory Policy、管理平台 reliability 视图、运维审计、历史隔离与 partition rebuild；
- SDK 和文档已删除兼容别名、旧 Repair 说明和退休策略字段，只保留单一新接口。

本分支完成并通过以下门禁：

- `cargo fmt --all`；
- `cargo check --workspace --all-targets`；
- `cargo test --workspace --all-targets`；
- Memory Engine 全量 PostgreSQL migration 后执行 `cargo test --manifest-path memory_engine/backend/Cargo.toml --all-targets -- --test-threads=1`；
- Admin Console TypeScript type-check 与 48 项测试；
- Windows React TypeScript type-check 与 472 项测试；
- macOS Memory Engine、Agent Memory Context、Durable Agent Memory 定向集成测试；
- 退休符号、旧 Prompt/策略字段和旧 Repair API 的仓库扫描为零。

生产发布仍严格执行第 20 节维护窗口顺序：生产快照演练、备份、migration、M2a 历史隔离、M3b 全量 rebuild、全量一致性审计和端到端 smoke 必须在恢复写入前全部完成。该顺序是一次性发布流程，不是灰度、兼容或运行时双路径。
