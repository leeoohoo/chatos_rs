# Memory 平台收口方案

## 1. 结论

我们现在应该把能力拆成三层来做，而不是继续让 `memory_server` 同时承担数据面、控制面、业务适配和管理后台。

目标形态：

1. `memory_engine`
   - 成为唯一的记忆数据与总结引擎
   - 负责消息、线程、总结、rollup、subject memory、review_repair、上下文组织
   - 负责统一的全局模型配置、全局任务配置、任务运行记录、后台调度、管理前端

2. `memory_server`
   - 短中期只保留 `chatos` 兼容层职责
   - 重点职责是 `chatos session/contact/project/agent -> memory_engine thread/label/subject` 的映射
   - 以及少量 `chatos` 还没迁走的兼容 API

3. `chatos`
   - 承接智能体创建与管理
   - 最终直接接 `memory_engine`，或者接一个更薄的 mapping adapter

所以，`memory_server` 不是现在立刻可以完全删除，但它应该被收缩成一个很薄的兼容映射层；配置、调度、任务列表、管理页面都应该迁到新系统。

---

## 2. 当前真实情况

### 2.1 `memory_server` 目前还在保存什么

当前这些数据仍然在 `memory_server` 的 Mongo 里：

1. 模型配置
   - collection: `ai_model_configs`
   - 代码：`memory_server/backend/src/repositories/configs/model_configs.rs`
   - 现状：按 `user_id` 维度保存

2. 定时任务配置
   - collection: `summary_job_configs`
   - collection: `summary_rollup_job_configs`
   - collection: `agent_memory_job_configs`
   - 代码：
     - `memory_server/backend/src/repositories/configs/job_configs/summary_job.rs`
     - `memory_server/backend/src/repositories/configs/job_configs/summary_rollup_job.rs`
     - `memory_server/backend/src/repositories/configs/job_configs/agent_memory_job.rs`
   - 现状：按 `user_id` 维度保存，并且支持 admin 配置兜底

3. 任务列表 / 运行记录
   - collection: `job_runs`
   - 代码：`memory_server/backend/src/repositories/jobs.rs`
   - 前端首页、运行记录页都在读这里

4. 用户与后台登录
   - collection: `auth_users`

5. `chatos` 兼容会话层
   - sessions/messages/summaries/contacts/projects/agents/skills 等 API 仍在 `memory_server`

### 2.2 `memory_server` 的 worker 还在做什么

`memory_server/backend/src/jobs/worker.rs` 当前仍在：

1. 扫活跃 `user_id`
2. 读取每个用户自己的 summary/rollup/agent-memory 配置
3. 决定是否触发：
   - summary
   - rollup
   - agent memory
4. 其中 summary/rollup/review_repair 已大量转调 `memory_engine`

也就是说：

- 数据引擎已经部分迁到 `memory_engine`
- 但控制面和调度面仍然留在 `memory_server`
- 当前职责是割裂的

### 2.3 `memory_server` 前端现在在管理什么

前端目录：`memory_server/frontend`

它当前主要承担：

1. Dashboard
2. Job Runs
3. User Management
4. User Config Center
5. Model Configs
6. Job Configs
7. Agents
8. Skills
9. Contact memories / recalls 浏览

这里最大的问题是：它还是围绕“每个用户一套 memory 配置”的旧思路设计的。

### 2.4 `memory_server` 还保留着一个不能忽略的职责：会话映射

这个是这次必须保留考虑的一层。

当前 `memory_server` 在 `sync_session()` 时会把 `chatos` 会话同步成 `memory_engine` 线程：

- `thread_id = session_id`
- `source_id = memory_server`
- `subject_id = session:{session_id}`
- `external_thread_id = session_id`
- `labels` 从 `contact_id / agent_id / project_id` 推导
- `metadata.legacy_session_mapping` 保存原始映射信息

代码位置：

- `memory_server/backend/src/services/memory_engine_client.rs`
- `memory_server/backend/src/api/sessions_api.rs`

当前 label 规则大致包括：

- `project:{project_id}`
- `contact:{contact_id}`
- `agent:{agent_id}`
- `contact_project:{contact_id}:{project_id}`
- `agent_project:{agent_id}:{project_id}`

这层的价值不是“存消息”，而是：

1. 帮 `chatos` 旧语义映射到新系统线程
2. 帮新系统在不理解 `chatos` 领域模型的前提下，还能按旧关系组织上下文
3. 兼容 review_repair、project memory、agent recall 等按 label 聚合的能力

所以这层不能跟着配置一起被粗暴删掉。

---

## 3. 对整体方向的判断

你现在这个思路我认同，但要拆成两个阶段：

### 阶段 A：先把“管理与调度”彻底迁到 `memory_engine`

这一步做完后：

- `memory_engine` 成为真正的平台主体
- `memory_server` 退化为 `chatos` 兼容映射层
- 用户级 job/model 配置废弃，改成全局配置

### 阶段 B：再把 `memory_server` 彻底退掉

这一步前提是：

1. 智能体能力已迁到 `chatos`
2. `chatos` 会话映射逻辑迁到 `chatos` 自己，或抽成一个独立 adapter
3. `chatos` 不再依赖 `memory_server` 的 auth/contact/project/session API

如果只把“智能体创建”迁走，`memory_server` 还不能立刻下线，因为目前 `chatos` 对它的依赖面不止这一块。

---

## 4. 目标架构

### 4.1 `memory_engine` 负责什么

`memory_engine` 成为唯一 Memory 平台，负责：

1. 线程与记录
   - threads
   - records
   - summaries
   - rollups
   - subject memories

2. 上下文能力
   - compose context
   - thread summary
   - review_repair
   - agent/subject memory build

3. 控制面
   - 全局模型配置
   - 全局任务配置
   - 任务运行记录
   - worker 调度
   - 管理前端

4. 多子系统接入能力
   - 每个子系统只要按标准接口写入 thread/record 即可
   - engine 不需要理解各子系统自己的“联系人/项目”业务语义
   - 只需要保存 `source_id + thread_id + labels + metadata`

### 4.2 `memory_server` 保留什么

短中期保留以下最小职责：

1. `chatos` 兼容会话映射
   - 从 `session/contact/project/agent` 推导 engine labels 和 metadata
   - 把 chatos 会话同步成 engine thread

2. `chatos` 兼容读写代理
   - 直到 `chatos` 完成直连前，继续代理 session/message/context 等调用

3. 不再承担平台管理职责
   - 不再保存模型配置
   - 不再保存任务配置
   - 不再保存任务运行记录
   - 不再跑自己的 worker
   - 不再提供 memory 管理后台

### 4.3 `chatos` 最终负责什么

1. 智能体创建与管理
2. 联系人 / 项目 / UI 业务语义
3. 自己的账号体系
4. 最终直接调用 `memory_engine` 或薄适配层

---

## 5. 建议的数据归属调整

### 5.1 保留的运行时数据归属

历史消息、总结、上下文相关数据，应该都统一在 `memory_engine`：

1. `engine_threads`
2. `engine_records`
3. `engine_summaries`
4. `engine_subject_memories`

这部分方向已经基本对了，后续只需要继续清掉 `memory_server` 内部剩余的总结逻辑和配置逻辑。

### 5.2 新增的控制面数据归属

建议在 `memory_engine` 新增以下 collection：

1. `engine_model_profiles`
   - 全局模型配置
   - 不再按 `user_id`
   - 只区分用途和是否启用

2. `engine_job_policies`
   - 全局任务策略
   - 可按 job_type 或 source_id 维度配置
   - 例如：
     - summary
     - rollup
     - subject_memory
     - review_repair

3. `engine_job_runs`
   - 统一任务运行记录
   - 用于 Dashboard、列表页、手工排障

4. 可选：`engine_admin_settings`
   - 平台级开关
   - 默认模型
   - 前端展示配置

### 5.3 映射数据怎么处理

这里不建议让 `memory_engine` 去理解 `chatos` 的“联系人 + 项目”模型。

建议保持下面这个原则：

1. `memory_engine` 只存通用字段
   - `source_id`
   - `thread_id`
   - `external_thread_id`
   - `labels`
   - `metadata`

2. `memory_server` 或后续 `chatos` adapter 负责生成映射
   - `legacy_session_mapping.session_id`
   - `legacy_session_mapping.project_id`
   - `legacy_session_mapping.contact_id`
   - `legacy_session_mapping.agent_id`

3. `memory_engine` 只根据 labels 和 metadata 做聚合，不理解业务

这样最符合你前面说的要求：

“新系统不需要理解，只要能和我们之前的对应起来就好了。”

---

## 6. 新的管理前端应该放在哪里

建议新增 `memory_engine/frontend`，作为唯一 Memory 平台后台。

不建议继续在 `memory_server/frontend` 上叠加。

建议页面结构：

1. Overview
   - 服务健康
   - 最近 24h 任务统计
   - pending thread / pending summary / pending subject memory 概览

2. Global Models
   - 全局模型配置
   - 模型连通性测试

3. Job Policies
   - summary policy
   - rollup policy
   - subject memory policy
   - review_repair policy

4. Job Runs
   - 列表
   - SSE 或轮询刷新
   - 失败详情

5. Threads
   - 按 source / tenant / label 查询
   - 查看 thread metadata 和 mapping

6. Summaries
   - 按 thread / label / type 查询
   - 手工重跑 / 删除 / 标记

7. Subject Memories
   - 按 subject_id / relation_subject_id / type 查询

8. Mapping Inspector
   - 查看 `chatos session -> engine thread` 的同步结果
   - 方便排查 label 和 metadata 是否正确

---

## 7. API 收口方案

### 7.1 `memory_engine` 新增 API

需要新增一组控制面 API：

1. `/api/memory-engine/v1/admin/model-profiles`
2. `/api/memory-engine/v1/admin/job-policies`
3. `/api/memory-engine/v1/admin/job-runs`
4. `/api/memory-engine/v1/admin/dashboard`
5. `/api/memory-engine/v1/admin/threads/query`
6. `/api/memory-engine/v1/admin/mappings/query`

现有 job API 可保留，但要改成读全局策略，而不是靠 `memory_server` 传每用户配置。

### 7.2 `memory_server` 要删除的 API

最终要从 `memory_server` 移除：

1. `/configs/models`
2. `/configs/summary-job`
3. `/configs/summary-rollup-job`
4. `/configs/agent-memory-job`
5. `/jobs/runs`
6. `/jobs/stats`
7. `memory_server/frontend` 整个管理页面

### 7.3 `memory_server` 要保留的 API

短期保留：

1. sessions/messages/summaries/context 兼容 API
2. sync_session / sync_message
3. review_repair 的 chatos scope 映射入口
4. 还没迁走的 contacts/projects/agents 兼容接口

---

## 8. 实施顺序

### Phase 1：把控制面迁进 `memory_engine`

1. 在 `memory_engine` 新增：
   - global model profiles
   - global job policies
   - engine job runs
   - admin APIs
2. 把 worker 的配置来源改为 `memory_engine` 自己
3. 把 Dashboard / Job Runs / Config 页面做进 `memory_engine/frontend`
4. `review_repair`、summary、rollup、subject memory 全部由 engine 自治

交付结果：

- `memory_server` 不再保存平台配置
- `memory_server` 不再负责 worker 调度

### Phase 2：把 `memory_server` 收缩成兼容层

1. 删除 `memory_server` 中：
   - model config 存储
   - job config 存储
   - job run 存储
   - worker
   - 管理前端
2. 保留：
   - session mapping
   - chatos 兼容 API
3. 让 `memory_server` 的 session/message/context 调用都只做：
   - 参数兼容
   - label/mapping 生成
   - 调 engine

交付结果：

- `memory_server` 退化为 adapter

### Phase 3：把智能体能力迁到 `chatos`

1. agents / ai-create / runtime-context / skills 相关能力逐步迁到 `chatos`
2. 联系人、项目、智能体等业务实体不再让 `memory_server` 承担主存储

交付结果：

- `memory_server` 剩下的主要就是会话映射兼容层

### Phase 4：决定 `memory_server` 的最终归宿

有两种落点：

1. 方案 A：保留一个超薄 adapter
   - 只做 `chatos -> engine` 映射
   - 风险最小

2. 方案 B：把映射逻辑迁入 `chatos`
   - `chatos` 直接同步 thread/record 到 engine
   - `memory_server` 完全下线

我建议先走 A，再看是否继续收敛到 B。

---

## 9. 风险与注意点

### 9.1 最大风险不是数据迁移，而是依赖面

现在 `chatos` 对 `memory_server` 的依赖远不止“总结”：

1. 登录鉴权
2. session CRUD
3. message 持久化
4. compose context
5. review_repair
6. contacts/projects
7. agent runtime context
8. turn runtime snapshot

所以“把智能体创建迁到 chatos 后就删掉 memory_server”这个目标方向没问题，但不能把依赖面低估。

### 9.2 全局配置不等于没有租户隔离

建议改掉“按用户配模型和任务”，但不要改掉数据隔离：

- 数据仍保留 `tenant_id`
- 配置改为平台级全局

这样：

1. 管理简单
2. 子系统接入统一
3. 历史数据边界不乱

### 9.3 映射规则要版本化

建议给 thread metadata 增加：

- `mapping_version`
- `mapping_source`

方便以后从 `memory_server` 迁到 `chatos` 时平滑切换。

---

## 10. 我建议的最终决策

建议按下面这条线执行：

1. 立即确认 `memory_engine` 是唯一 Memory 平台主体
2. 立即把模型配置、任务配置、任务列表、管理前端迁到 `memory_engine`
3. 立即停止在 `memory_server` 继续发展管理功能
4. 明确保留 `memory_server` 的短期职责是 `chatos` 会话映射兼容层
5. 后续再把智能体和业务实体迁到 `chatos`
6. 最终再决定把映射层也并入 `chatos`，彻底下线 `memory_server`

---

## 11. 下一步建议

如果按这个方案推进，下一步最合理的是直接做下面三件事：

1. 在 `memory_engine` 设计并落库：
   - `engine_model_profiles`
   - `engine_job_policies`
   - `engine_job_runs`

2. 在 `memory_engine` 增加 admin API 与 frontend 骨架

3. 在 `memory_server` 开始删除：
   - per-user model config
   - per-user job config
   - worker
   - job runs dashboard

这三步做完，整体结构就会开始真正变干净。
