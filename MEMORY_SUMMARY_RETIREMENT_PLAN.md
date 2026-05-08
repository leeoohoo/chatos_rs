# Memory Server 老 Summary 内核退役清单

## 1. 当前状态

截至当前阶段，`summary` 主路径已经基本迁移到 `memory_engine`：

1. `memory_engine` 已支持 `thread/records` 接入
2. `memory_engine` 已支持 AI/规则双模式 summary
3. `memory_engine` 已支持 worker 自动扫 pending threads
4. `memory_server` 的 `compose_context` 已优先桥接到 `memory_engine`
5. `memory_server` 的 session/message 写路径已双写到 `memory_engine`
6. `memory_server` 的单 session `run_summary_once` 已优先桥到 `memory_engine`
7. `memory_server` 的全局 `run_summary_once` 已优先桥到 `memory_engine`
8. `memory_server` 的老 `summary worker` 已支持通过配置让位

因此现在需要明确：

**哪些老 summary 模块还必须保留，哪些已经进入兼容层，哪些可以开始正式退役。**

---

## 2. 退役原则

本次退役遵循三个原则：

1. 先让调用主路径迁走，再删老实现
2. 先把老实现降级成 fallback，再删
3. summary 先退，rollup 和 agent_memory 暂不连带强删

---

## 3. 第一批退役目标

第一批目标只针对 `summary`，不碰 `rollup` / `agent_memory`。

## 3.1 已进入兼容层

下面这些模块/入口已经不再适合作为默认主路径：

1. `memory_server/backend/src/api/jobs_api.rs::run_summary_once`
   - 已改成 `memory_engine` 优先
2. `memory_server/backend/src/api/configs_job_configs_api.rs`
   - `summary job config` 已显式返回 `compatibility_mode`
3. `memory_server/backend/src/jobs/worker.rs`
   - `summary worker` 已支持让位给 `memory_engine`

这三类已经是“兼容层”，不是平台核心。

## 3.2 仍然保留但应标记为 fallback

下面这些模块暂时保留，用于回退或特殊场景：

1. `memory_server/backend/src/jobs/summary.rs`
2. `memory_server/backend/src/jobs/summary_generation.rs`
3. `memory_server/backend/src/jobs/summary_support.rs`
4. `memory_server/backend/src/jobs/text_summarizer.rs`

保留原因：

1. 老环境未配 `memory_engine` 时仍需要兜底

## 3.3 第一批不建议立即删除

1. `memory_server/backend/src/jobs/rollup.rs`
2. `memory_server/backend/src/jobs/agent_memory.rs`
3. `summary_rollup_job_configs`
4. `agent_memory_job_configs`

原因：

1. 它们还没有迁到 `memory_engine`
2. 直接删会把现有能力打断

---

## 4. 第一批可执行动作

## 4.1 行为层

1. `summary worker` 默认让位给 `memory_engine`
2. `run_summary_once` 默认让位给 `memory_engine`
3. `summary config` API 明确标注兼容模式

这三项已经完成。

## 4.2 文档与约定层

需要明确对内约束：

1. 不再给老 `jobs::summary` 增加新能力
2. 任何新的 summary 能力都只加到 `memory_engine`
3. 老 `summary` 模块只允许做 fallback 修复，不做增强

## 4.3 下一批代码动作

下一批建议按顺序推进：

1. 在 `jobs::summary` 相关日志中明确标注 `legacy` / `fallback`
2. 在 `run_summary_once` 返回结构中明确 `backend=memory_engine|memory_server`
3. 在 `worker` 中为 summary skip 增加更清晰日志
4. 将 `summary job config` 的前端/调用方文案改为兼容说明

以上动作现已继续收口到代码行为层：

1. legacy `jobs::summary` 入口在执行时会输出 `MEMORY-SUMMARY-LEGACY-FALLBACK` 日志
2. `run_summary_once` 响应会显式返回：
   - `backend`
   - `compatibility_mode`
   - `fallback_used`
3. 当 `memory_engine` 已启用、且 legacy fallback 已禁用时：
   - `PUT /configs/summary-job` 将返回冲突错误
   - 防止继续把老 summary config 当成可写主配置

## 4.4 第一批删除执行边界

为了避免“看起来还在双核心并行”，第一批删除/冻结按下面边界推进。

### Batch 1：立即执行 / 已在进行

目标：把 legacy summary 从“可继续演进的功能模块”降级成“只做兼容和回退的封存模块”。

包含动作：

1. `jobs::summary*` 文件头标记 `LEGACY / FROZEN MODULE`
2. legacy 入口增加 fallback 警示日志
3. summary API 响应暴露兼容态字段
4. `summary-job` 配置在 engine preferred + fallback disabled 下冻结写入
5. `summary_job_configs` 在内部实现上开始按 `legacy_summary_fallback` 语义收口

当前状态：已完成

### Batch 2：条件满足后可删

前置条件：

1. `memory_engine` 线上承接增量 summary 稳定
2. 历史 backfill 完成
3. `run_summary_once` 基本不再进入 legacy fallback
4. `review_repair` 已切到 `memory_engine` 且不再依赖旧 summary 执行链

候选删除项：

1. `memory_server/backend/src/jobs/summary.rs`
2. `memory_server/backend/src/jobs/summary_generation.rs`
3. `memory_server/backend/src/jobs/summary_support.rs`
4. `memory_server/backend/src/jobs/text_summarizer.rs`
5. `summary_job_configs` 相关 repo / API / model

### Batch 3：清理兼容 API 壳层

前置条件：

1. 上述 Batch 2 已完成
2. 外部调用方已切到 `memory_engine` 的 summary / context 协议

候选动作：

1. 删除 `summary-job` 配置 API
2. 删除 `run_summary_once` 中 legacy 分支
3. 删除 worker 中 legacy summary 分支
4. 将 `memory_server` 中 summary 角色彻底降为 adapter-only

---

## 5. 第二批退役目标

当下面条件满足后，可以开始真正删除老 summary 代码：

1. `memory_engine` 已稳定处理线上增量 summary
2. `backfill_memory_engine` 已跑完核心历史数据
3. `compose_context` 已稳定依赖 `memory_engine`
4. `run_summary_once` 基本不再落到老 fallback

满足后可删除：

1. `memory_server/backend/src/jobs/summary.rs`
2. `memory_server/backend/src/jobs/summary_generation.rs`
3. `memory_server/backend/src/jobs/summary_support.rs`
4. `memory_server/backend/src/jobs/text_summarizer.rs`
5. `summary_job_configs` 相关 repo 和 API

但删除前要先处理：

1. 老 summary job runs 的兼容查询问题

---

## 6. review_repair 的单独说明

`review_repair` 当前已切到 `memory_engine`，不再占用老 summary 主执行链。

因此它不再是阻止删除老 summary 执行链的前置阻塞项。

---

## 7. rollup / agent_memory 的处理建议

这两块现在不要急着跟 summary 一起强迁。

建议顺序：

1. 先让 `summary` 完整迁移闭环
2. 再评估 `rollup` 是否还有必要存在
3. 再评估 `agent_memory` 是否应升级成 `memory_engine` 的 `subject_memory`

也就是说：

1. `rollup` 未来可能变成 `memory_engine` 的更高层 summary
2. `agent_memory` 未来可能变成 `memory_engine` 的通用 `subject memory`

但这是第二阶段工作，不建议和 summary 主迁移混在一起。

---

## 8. 当前建议

当前阶段最合理的动作是：

1. 正式冻结老 `summary` 模块，不再增强
2. 继续把老 `summary` 标成 fallback
3. 继续清理旧 summary 链里已经失效的 review_repair 残留
4. 暂缓 `rollup` / `agent_memory` 的删除

这比直接暴力删代码要稳，也能保证后续每一步都有清晰边界。
