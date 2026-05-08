# Review Repair 迁移方案

## 1. 结论先行

`review_repair` 的最终产物应该进入 `memory_engine`。

原因不是要让 `memory_engine` 原生理解 chatos 的联系人/项目语义，而是：

1. `review_repair` 本质上仍然是总结的一部分
2. 如果纠偏总结不进入新系统，`memory_engine` 组织上下文时就会缺少可信事实源
3. 新系统需要成为 summary / repair summary / context compose 的统一落点

因此当前策略调整为：

1. `scope` 解析仍可由 `memory_server` 负责
2. `repair summary` 产物要写入 `memory_engine`
3. `compose_context` 需要能把 repair summary 一并组织出来
4. `review_repair` 不再保留 legacy fallback

---

## 2. 现状判断

当前 `review_repair` 的实现位置主要在：

1. `memory_server/backend/src/api/jobs_api.rs`
2. `memory_server/backend/src/jobs/review_repair.rs`
3. `memory_server/backend/src/jobs/summary_generation.rs`
4. `memory_server/backend/src/models/job_configs.rs`

它本质上做的事情是：

1. 按 `user_id + project_id + contact_id/agent_id` 取一个 scope
2. 找出 scope 内 session
3. 强制对 session 中 pending messages 重跑总结
4. 使用特殊 prompt：`REVIEW_REPAIR_SUMMARY_PROMPT_TEMPLATE`
5. 目标不是普通压缩，而是纠正幻觉、隔离脏上下文、重新建立可信总结

所以它和常规 summary 的关系是：

1. 底层流程共用
2. 目标语义不同
3. 触发方式不同
4. 作用域模型更强业务化

---

## 3. 当前迁移边界

## 3.1 它依赖强业务 scope

`review_repair` 当前天然依赖：

1. `project_id`
2. `contact_id`
3. `agent_id`

而 `memory_engine` 当前的核心抽象是：

1. `source`
2. `subject`
3. `thread`
4. `record`

如果把 `project_id/contact_id/agent_id` 业务语义硬迁进新系统，会把：

1. chatos 的业务 scope
2. memory_server 的旧领域语义

重新灌进新系统。

这和“新系统保持通用、只承接稳定映射关系”的方向冲突。

## 3.2 应迁的是产物，不一定是 scope 模型

当前更合理的拆分方式是：

1. `memory_server` 继续负责按旧规则解析 scope
2. `memory_engine` 负责执行 / 保存 repair summary
3. 两边通过 thread/session 对应关系衔接

## 3.3 它的 prompt 与产物语义都不同

普通 summary 的目标是：

1. 压缩
2. 保留事实
3. 为后续上下文服务

`review_repair` 的目标是：

1. 查错
2. 排幻觉
3. 纠偏
4. 重建可信上下文

这其实更接近一种“修复型总结任务”，不是普通 summary 的同类项。

---

## 4. 推荐定位

建议把 `review_repair` 定位成：

**`memory_engine` 中的 repair summary capability + `memory_server` 中的 scope compatibility layer`**

也就是：

1. 纠偏总结产物进入 `memory_engine`
2. chatos 特有 scope 解析暂时仍留在 `memory_server`
3. 新系统不需要原生理解联系人/项目，只需要保存稳定映射与总结结果

---

## 5. 具体处理方案

## 5.1 短期方案

短期直接做三件事：

1. 在 `memory_engine` 增加 repair summary 接口
2. `compose_context` 一并纳入 repair summary
3. `memory_server` 将 `review_repair` 改为强制桥接 `memory_engine`

## 5.2 中期方案

中期把它从老 `summary` 主链里拆出语义边界：

1. 不再把它视为“summary 的一种模式”
2. 而是视为“修复型总结任务”

如果后面需要整理代码，可以考虑：

1. 新增 `jobs/review_repair.rs`
2. 把 `run_review_repair_for_scope` 从 `jobs/summary.rs` 中挪出来
3. 让老 summary 退役时，不再被 `review_repair` 拖住

以上代码动作现已完成：

1. 已新增 `memory_server/backend/src/jobs/review_repair.rs`
2. `run_review_repair_once` / `get_review_repair_status` 已改为调用独立 `review_repair` 模块
3. `review_repair` 不再作为 `jobs::summary` 的内部模式入口存在
4. `memory_server` 中 `review_repair` 已改为 engine-only，失败时直接报错，不再 fallback

## 5.3 后期方案

在 `memory_engine` 中建议将其承接为：

1. `repair_summary`
2. `trust_rebuild_summary`
3. `scope_repair_job`

但仍不要求新系统原生重建 chatos 的业务作用域模型。

---

## 6. 对当前代码的直接影响

## 6.1 当前不删

当前先保留：

1. `jobs::review_repair::run_once_for_scope`
2. `jobs::review_repair::get_status_for_scope`
3. `memory_engine` 中的 `thread_repair` summary type 与 run 接口

## 6.2 当前迁移方式

当前按下面方式迁移：

1. `review_repair` prompt/summary 产物进入 `memory_engine`
2. `review_repair` scope 逻辑仍由 `memory_server` 解
3. `review_repair` 执行不再保留 legacy fallback
4. `review_repair` 状态查询仍由 `memory_server` 提供 scope 兼容视图

## 6.3 当前可以做的收口

可以立即做：

1. 在 `memory_engine` 中新增 `repair summary` 接口
2. 在 `compose_context` 中纳入 repair summary
3. 在 `memory_server` 中把它从普通 summary 主链中剥离出来并强制桥接新接口

---

## 7. 推荐下一步代码动作

建议下一步补一个很小的行为收口：

1. 继续补齐 `memory_engine` repair summary 的更完整元数据
2. 让 `run_review_repair_once` 返回体现 `memory_engine` 唯一执行结果
3. 后续视需要再把 scope 查询也做成标准化接口

---

## 8. 最终判断

`review_repair` 当前最合适的定位是：

1. **产物进入新系统**
2. **scope 解析暂由旧系统兼容**
3. **执行路径不再保留 legacy fallback**
4. **后续再决定是否把 scope 本身标准化进统一接口**

这样既能保证 `memory_engine` 上下文完整，也不会把 chatos 的业务语义硬塞进新系统。
