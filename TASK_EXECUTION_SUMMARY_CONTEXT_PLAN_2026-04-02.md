# 任务执行总结与历史上下文组织方案

## 1. 现状梳理

### 1.1 当前有两套历史数据

1. 联系人聊天历史
   - 原始消息：`memory_server/backend/src/repositories/messages/*`
   - 总结表：`summaries`
   - 上下文拼装：`memory_server/backend/src/services/context.rs::compose_context`
   - 调用入口：`chat_app_server_rs/src/services/memory_server_client/session_ops.rs::compose_context`

2. 任务执行历史
   - 原始消息：`task_execution_messages`
   - 总结表：`task_execution_summaries`
   - 上下文拼装：`memory_server/backend/src/services/context.rs::compose_task_execution_context`
   - 调用入口：`chat_app_server_rs/src/services/memory_server_client/task_execution_ops.rs::compose_task_execution_context`

### 1.2 当前任务执行总结存在的几个关键问题

1. 任务执行总结复用了普通聊天的 `summary_job_config`
   - `memory_server/backend/src/jobs/summary.rs`
   - `memory_server/backend/src/jobs/task_execution_summary.rs`
   - `memory_server/backend/src/jobs/worker.rs`
   - 现在这两个 job 都调用 `configs::get_effective_summary_job_config(...)`
   - 结果是：模型、prompt、token_limit、round_limit、interval 全共用

2. 任务执行总结的文案语义还是“会话总结”
   - `memory_server/backend/src/services/context.rs::compose_summary_section`
   - 这里统一输出 `以下是历史会话总结`
   - 对任务执行上下文来说，这个标签是错的

3. 任务执行的汇总粒度是 `user_id + contact_agent_id + project_id`
   - `task_execution_messages.scope_key`
   - 也就是说它是“联系人在某项目下的后台执行流”
   - 不是单个 task_id 维度
   - 这本身没问题，但意味着“上下文组织”不能只按单任务思维来做

4. 联系人聊天和任务执行之间现在只有一个很弱的桥
   - `chat_app_server_rs/src/services/task_execution_runner.rs::save_task_notice_message`
   - 任务结束时会往源会话写一条 `task_notice`
   - 这更像 UI 通知，不适合作为稳定的长期上下文桥梁

5. 任务执行上下文目前只看“任务执行自己的历史”
   - `MessageStore::TaskExecution` 最终走 `compose_task_execution_context`
   - 它不会结构化地带入“这次任务来自哪次联系人对话、那次对话的用户目标是什么、最近完成了哪些任务”
   - 现在更多靠当前 task 内容和少量 notice 间接补足

## 2. 我建议的总体原则

### 2.1 不能只看 L0，总结链路必须整体考虑

你这次补充得很对，这里不能只看“任务执行总结”本身，还必须把：

1. 总结的总结
2. 最终记忆总结

一起纳入设计。

当前系统实际上已经有这几层抽象：

1. 原始聊天消息
   - `messages`

2. 聊天 L0 总结
   - `summaries`

3. 聊天总结的总结
   - `summary_rollup`

4. 最终长期记忆
   - `agent_memory`

而任务执行目前只有：

1. 原始执行消息
   - `task_execution_messages`

2. 执行 L0 总结
   - `task_execution_summaries`

也就是说，任务执行这条链路现在还缺：

1. 执行总结的总结
2. 进入最终长期记忆的正式路径

### 2.2 四层记忆，职责分离

我建议后续把历史上下文明确拆成四层：

1. 聊天原始记忆
   - 面向“用户在 Chatos 里和联系人聊天”
   - 数据源：`messages`

2. 执行原始记忆
   - 面向“后台任务执行器和任务内置 MCP”
   - 数据源：`task_execution_messages`

3. 过程总结记忆
   - chat:
     - L0: `summaries`
     - rollup: `summary_rollup`
   - task:
     - L0: `task_execution_summaries`
     - rollup: 后续建议新增

4. 结果桥接 / 长期记忆
   - 面向“把任务结果反馈给联系人聊天和长期记忆”
   - 最终沉淀到：
     - 联系人聊天可读的 bridge
     - `agent_memory`

### 2.3 不把聊天历史和执行历史硬合并成一套

不建议把 `task_execution_messages` 直接并入普通 `messages`，也不建议把 `task_execution_summaries` 并入普通 `summaries`。

原因：

1. 两者目标完全不同
   - 聊天历史是面向“理解用户”
   - 执行历史是面向“完成任务”

2. 粒度不同
   - 聊天历史是 session 维度
   - 执行历史是 contact + project 的 scope 维度

3. 噪音不同
   - 执行链路里会有大量工具输出、失败重试、系统提示
   - 这些不应该直接污染联系人聊天上下文

## 3. 配置层方案

### 3.1 新增专门的任务执行总结配置

建议新增一套独立配置：

1. 集合
   - `task_execution_summary_job_configs`

2. 模型
   - 新建 `TaskExecutionSummaryJobConfig`
   - 初版字段尽量与 `SummaryJobConfig` 保持同构，降低改造成本

3. 初版字段建议
   - `user_id`
   - `enabled`
   - `summary_model_config_id`
   - `summary_prompt`
   - `token_limit`
   - `round_limit`
   - `target_summary_tokens`
   - `job_interval_seconds`
   - `max_scopes_per_tick`
   - `updated_at`

4. 默认 prompt 不应复用会话总结 prompt
   - 当前默认 prompt 是：
     - `DEFAULT_SUMMARY_PROMPT_TEMPLATE`
   - 任务执行应改成偏执行语义，例如：
     - 保留目标
     - 保留关键操作和结果
     - 保留失败原因和阻塞点
     - 保留后续接手所需状态
     - 不复述无价值过程噪音

### 3.2 新增专门的任务执行 rollup 配置

既然你要求把“总结的总结”也考虑进去，我建议不要只建 `task_execution_summary_job_configs`，而是一次性把第二层也设计进去：

1. `task_execution_summary_job_configs`
   - 面向 raw task execution messages -> L0 summary

2. `task_execution_rollup_job_configs`
   - 面向 task execution L0 summary -> rollup summary

建议字段与现有 `SummaryRollupJobConfig` 尽量同构：

1. `user_id`
2. `enabled`
3. `summary_model_config_id`
4. `summary_prompt`
5. `token_limit`
6. `round_limit`
7. `target_summary_tokens`
8. `job_interval_seconds`
9. `keep_raw_level0_count`
10. `max_level`
11. `max_scopes_per_tick`
12. `updated_at`

### 3.3 继承规则

建议保持和你现有体系一致：

1. 当前用户有自己的任务执行总结配置
   - 用自己的

2. 当前用户没有
   - 回退到 admin

3. admin 也没有
   - 用系统默认值

### 3.4 API / 前端入口

建议新增：

1. memory server
   - `GET /api/memory/v1/configs/task-execution-summary-job`
   - `PUT /api/memory/v1/configs/task-execution-summary-job`

2. chat_app_server_rs
   - 新增与 `session_summary_job_config` 平行的一套路由
   - 例如：
     - `/api/task-execution-summary-job-config`
     - `/api/task-execution-rollup-job-config`

3. 前端
   - 在现有“总结配置”区域增加一个单独卡片：
     - 会话总结配置
     - 会话总结 rollup 配置
     - 任务执行总结配置
     - 任务执行 rollup 配置
     - 最终记忆配置

### 3.5 最终记忆配置也必须纳入这次方案

这次方案里也必须明确：

1. `agent_memory_job_configs` 继续保留
2. 任务执行结果后续要能进入 `agent_memory` 候选源
3. 但不建议让 `agent_memory` 直接吃 raw execution transcript

也就是说：

1. raw execution 只服务执行器
2. task l0 / task rollup 服务执行连续性
3. bridge / agent memory 服务长期人格与长期协作记忆

### 3.6 关于“联系人记忆是否统一”和“总结表是否共表”的明确结论

这块我给一个明确结论：

1. 联系人的最终记忆可以统一
2. 总结表不建议和聊天共表
3. 但任务域内部的 L0 summary 和 rollup summary 可以共用同一张 task summary 表

原因分别是：

1. 最终记忆是“事实层”
   - 不管事实来自聊天还是任务执行，只要已经被抽象成稳定事实，就应该进入同一套联系人长期记忆
   - 所以这一层统一是合理的

2. 总结是“过程层”
   - 聊天 summary 服务聊天上下文
   - task summary 服务执行上下文
   - 二者 scope、触发方式、消费方都不同
   - 如果聊天和任务共用同一张 summary 表，查询和后续 job 会变复杂很多

3. task 域内部的 L0 和 rollup 是同一类抽象
   - 它们只是 level 不同
   - 这一点和现有 `session_summaries_v2` 的设计一致
   - 所以 task 域没有必要再拆成两张 summary 表

### 3.7 我建议的物理表策略

建议按下面方式组织：

1. 聊天 summary
   - 继续使用 `session_summaries_v2`

2. 任务 summary
   - 继续使用 `task_execution_summaries`
   - 但把 schema 扩成和 `SessionSummary` 接近
   - 让它同时承载：
     - task L0 summary
     - task rollup summary

3. 最终联系人长期记忆
   - 继续落到统一记忆表
   - 主要是 `agent_recalls`
   - project 维度需要的稳定事实继续可沉淀到 `project_memories`

### 3.8 对 `task_execution_summaries` 的推荐扩展

我建议不要新建 `task_execution_rollup_summaries`，而是直接扩展现有 `task_execution_summaries`：

建议补齐这些字段：

1. `rollup_summary_id`
2. `rolled_up_at`
3. `agent_memory_summarized`
4. `agent_memory_summarized_at`

这样它就能和 `session_summaries_v2` 一样承担：

1. L0 summary
2. rollup summary
3. 进入 agent memory 前的标记

### 3.9 对统一长期记忆表的推荐扩展

长期记忆虽然建议统一，但必须加来源元数据，不然以后会失真。

建议在 `agent_recalls` 或 bridge -> memory 写入链路里增加这些信息：

1. `source_kind`
   - `chat_summary`
   - `task_result`
   - `task_rollup`
   - `manual`

2. `source_scope_kind`
   - `session`
   - `task_scope`
   - `project`

3. `contact_agent_id`
4. `project_id`
5. `task_id` 可选
6. `source_summary_id` 可选

这样最终“联系人记忆”虽然放在一起，但我们仍然知道它是从哪里来的。

## 4. 联系人聊天与任务执行历史的关系

### 4.1 当前关系

现在的关系其实是：

1. 联系人聊天看的是 session history
2. 任务执行看的是 task execution history
3. 任务完成后，执行器往源 session 写一条 `task_notice`

这个关系太弱，因为：

1. `task_notice` 是自然语言通知，不是结构化结果
2. 它适合给 UI 展示，不适合做长期记忆桥梁
3. 它不能稳定支持“联系人后续继续基于过去任务结果安排新任务”

### 4.2 我建议的目标关系

我建议改成：

1. 联系人聊天
   - 不直接吃执行过程
   - 只吃“任务结果桥接记忆”

2. 任务执行
   - 不直接吃整段聊天历史
   - 只吃“任务来源摘要 + 任务执行自身历史 + 少量结果桥接”

3. 结果桥接
   - 每个任务在终态时生成一条结构化结果摘要
   - 供联系人聊天侧读取

## 5. 结果桥接层设计

### 5.1 为什么需要这一层

如果没有桥接层，会出现两个问题：

1. 联系人聊天不知道最近任务到底做成了什么
2. 为了让联系人知道结果，只能把大量任务执行轨迹塞进聊天上下文

这两种都不好。

### 5.2 我建议的数据形态

建议新增一类轻量结果记录，名字可以是：

1. `task_result_briefs`
2. 或 `contact_task_result_memories`

建议字段：

1. `id`
2. `user_id`
3. `contact_agent_id`
4. `project_id`
5. `task_id`
6. `source_session_id`
7. `source_turn_id`
8. `task_title`
9. `task_status`
10. `result_summary`
11. `result_format`
12. `finished_at`
13. `created_at`
14. `updated_at`

### 5.3 生成时机

任务进入终态时生成：

1. `completed`
2. `failed`
3. `cancelled` 可选

生成来源优先级：

1. `complete_current_task / fail_current_task` 传入的结果
2. `task.result_summary`
3. 最终 notice 文案兜底

### 5.4 和现有 `task_notice` 的关系

建议：

1. `task_notice` 继续保留
   - 它负责 UI 和普通会话里的即时可见反馈

2. `task_result_brief` 作为正式上下文桥接数据
   - 它负责后续联系人聊天上下文

也就是说：

1. `task_notice` 是“展示层通知”
2. `task_result_brief` 是“记忆层桥”

## 6. 后续历史上下文组织方式

### 6.1 联系人聊天时的上下文顺序

建议联系人聊天时，历史上下文按下面顺序组织：

1. 联系人自身长期记忆 / 项目记忆
   - `agent_memory`
2. 最近任务结果桥接摘要
   - 最近 3 到 5 条终态任务
   - 只看当前 `contact_agent_id + project_id`
3. 当前 session 的高层 rollup 总结
   - `summary_rollup`
4. 当前 session 的 L0 聊天总结
   - `summaries`
5. 当前 session 的最近原始消息
   - `messages`

明确不建议默认加入：

1. `task_execution_messages` 原始执行过程
2. `task_execution_summaries` 执行总结全文
3. task execution rollup 全文

原因很简单：
这些内容太“执行态”，会拉高噪音并误导联系人聊天模型。

### 6.2 任务执行时的上下文顺序

建议任务执行时，历史上下文按下面顺序组织：

1. 当前任务卡片
   - title
   - content
   - result contract
   - planned builtin MCP
   - planned context assets
   - project_root
   - remote_connection_id
   - source_session_id / source_turn_id

2. 本 scope 的任务执行高层 rollup
   - 后续新增
   - 标题应明确写成“历史任务执行高层总结”
3. 本 scope 的任务执行 L0 总结
   - 来自 `task_execution_summaries`
   - 标题应明确写成“历史任务执行总结”
4. 本 scope 的最近原始执行消息
   - 来自 `task_execution_messages`
5. 来源聊天摘要
   - 不是整段 session history
   - 而是“创建该任务时的那次来源对话摘要”
   - 可以优先用：
     - `planning_snapshot`
     - `source_session_id + source_turn_id` 对应的摘要
6. 最近已完成任务的结果桥接摘要
   - 用于 follow-up task / 串行任务接续

### 6.3 为什么不能让任务执行直接吃完整聊天历史

因为这会导致：

1. token 浪费
2. 模型把用户随口讨论误当执行指令
3. 多轮聊天里旧目标污染当前任务
4. 执行器难以聚焦到“当前任务要做什么”

所以任务执行更适合使用：

1. 当前任务卡片
2. 任务来源摘要
3. 执行 rollup / 执行总结
4. 执行历史

而不是整段聊天。

### 6.4 最终记忆总结应该如何参与

`agent_memory` 这一层我建议这样参与：

1. 联系人聊天时参与
   - 作为最高层稳定人格 / 长期协作记忆

2. 任务执行时默认不直接参与大段 recall
   - 除非当前任务明确依赖联系人长期偏好或长期约束

3. `agent_memory` 的候选源增加 task result bridge
   - 任务结果可以进入长期记忆
   - 但完整 task execution transcript 不应直接进入长期记忆

## 7. 对现有代码的直接改造建议

### 7.1 第一阶段：先把配置和语义拆开

建议先做这一步，投入小、收益大：

1. 新增 `TaskExecutionSummaryJobConfig`
2. 新增 `TaskExecutionRollupJobConfig`
3. `task_execution_summary.rs` 不再调用 `get_effective_summary_job_config`
4. `worker.rs` 用单独的 task execution summary config 来决定：
   - 是否启用
   - 间隔
   - 模型
   - prompt
   - token / round limit
5. 后续补 `task_execution_rollup.rs`
6. `compose_task_execution_context` 的 summary section 改名
   - 不再复用“历史会话总结”的标题

### 7.2 第二阶段：补桥接层

1. 新增 `task_result_briefs`
2. 在任务终态时写入 bridge 记录
3. 联系人聊天上下文引入最近 bridge
4. 不把 raw task execution history 混进普通聊天

### 7.3 第三阶段：把任务来源摘要显式化

建议在任务数据上补两类信息：

1. `source_session_id`
2. `source_turn_id`
3. `source_user_goal_summary`
4. `source_constraints_summary`

这样任务执行时就不需要回头去猜“这条任务到底从哪次对话来的”。

### 7.4 第四阶段：让最终记忆层接入任务结果

1. `agent_memory` job 的候选源增加 task result bridge
2. 保持“结构化结果优先”
3. 不让 `agent_memory` 直接吞完整 task execution transcript

## 8. 我认为应该保留和应该调整的点

### 8.1 应该保留

1. `task_execution_messages` / `task_execution_summaries` 作为独立数据域
2. scope 维度保持 `user + contact + project`
3. 任务完成后继续写 `task_notice`
4. admin 配置向普通用户透传的继承逻辑
5. 现有 `summary_rollup` 和 `agent_memory` 的高层抽象思路

### 8.2 应该调整

1. 任务执行总结不要再复用普通聊天总结配置
2. 任务执行还要补自己的 rollup 层
3. 任务执行上下文标题不要再叫“历史会话总结”
4. 联系人聊天不要直接读取 raw task execution history
5. 任务执行不要直接吃完整聊天历史
6. 增加结构化 bridge 层
7. 最终记忆层应吃 bridge / 高层抽象，不应直接吃 raw execution

## 9. 推荐实施顺序

### 9.1 最小可用顺序

1. 拆 task execution summary config
2. 拆 task execution rollup config
3. 改 task execution compose 的标题与语义
4. 新增 task result bridge
5. 联系人聊天先接 bridge，不接 raw execution history
6. 再补“来源聊天摘要”进入任务执行上下文
7. 最后把 bridge 接入 agent memory 候选源

### 9.2 这样做的好处

1. 改动风险最小
2. 不会推翻你现在已经跑通的任务执行链路
3. 能最快把“配置混用”和“上下文混乱”这两个核心问题收掉

## 10. 最终结论

我的结论是：

1. 任务执行总结必须有自己专门的配置
2. 任务执行还需要自己的“总结的总结”配置与链路
3. 联系人聊天历史和任务执行历史必须继续分域
4. 两者之间不要直接互吃原始历史，而要通过“结构化任务结果桥接层”连接
5. 最终 `agent_memory` 也必须纳入设计，但它应该吃 bridge / 高层抽象，不应直接吃 raw execution
6. 后续上下文组织应从“按表读数据”升级为“按职责分层拼上下文”

如果你认可这个方向，下一步最值得先做的是：

1. 新建 `task_execution_summary_job_configs`
2. 新建 `task_execution_rollup_job_configs`
3. 把 `worker + task_execution_summary + task_execution_rollup + API/UI` 这条配置链拆出来
4. 然后我再继续做 bridge 层、最终记忆接入和上下文拼装重构
