# 联系人任务图谱与依赖编排优化方案

## 1. 这次要解决的真实问题

当前这套联系人任务体系已经能做到：

1. 在 ChatOS 里和联系人对话
2. 由联系人创建任务
3. 任务进入待确认 / 待执行 / 执行中
4. 后台调度异步执行
5. 通过 IM 把结果再推回用户

但现在还有一个很明显的结构性问题：

> 不管任务多复杂，模型往往只创建成一个“大任务”，导致任务内容又长又杂，执行阶段没有明确前后顺序，也没有“开发完成后再测试”的结构化约束。

你截图里这一条任务就是典型现象：

1. 一条任务里塞了后端、前端、接口、权限、统计、页面、验收、兼容性等所有内容
2. 这其实不是“一个任务”，而是一个任务计划
3. 真正执行时应该拆成多个有前置关系的任务

所以这次的目标不是简单“再优化 prompt”，而是要把任务模型从**单任务队列**升级成**任务计划 + 子任务图谱 + 依赖调度**。

---

## 2. 当前代码里的根因

我重新看了当前代码，问题不是模型单纯“理解错了”，而是现在的数据结构天然就在鼓励单任务。

### 2.1 任务服务当前只有平铺的 `ContactTask`

位置：

- [models.rs](/Users/lilei/project/my_project/chatos_rs/contact_task_service/backend/src/models.rs)

当前只有：

1. `ContactTask`
2. `queue_position`
3. `status`
4. `scope_key`

没有这些结构：

1. `task_plan_id`
2. `parent_task_id`
3. `depends_on_task_ids`
4. `task_kind`
5. `verification_of_task_id`
6. `blocked_reason`
7. `handoff_payload`

也就是说，数据库层面现在根本没有“任务之间的关系”。

### 2.2 调度器当前是线性队列，不是依赖图调度

位置：

- [repository.rs](/Users/lilei/project/my_project/chatos_rs/contact_task_service/backend/src/repository.rs#L476)

当前 `scheduler_next(...)` 的逻辑本质是：

1. 同 scope 如果已有 `running`，不再启动
2. 如果有 `paused`，等待恢复
3. 否则从 `pending_execute` 里按 `queue_position + priority_rank + created_at` 取第一个

这意味着：

1. 它只认识“排队先后”
2. 它不认识“这个任务必须在另一个任务成功后才能执行”
3. 它不认识“这是测试任务，必须跟随某个开发任务”

### 2.3 `create_tasks` 虽然支持数组，但没有依赖语义

位置：

- [task_planner/mod.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/task_planner/mod.rs#L168)
- [review_flow.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/task_planner/review_flow.rs#L11)

现在 `create_tasks` 的确支持一次传多个 task draft，但这些 task 之间仍然是：

1. 平级
2. 无依赖
3. 无父子关系
4. 无“验证哪个任务”的语义

所以模型即使一次创建了多个任务，系统也不知道它们之间的编排关系。

### 2.4 后继任务拿不到前置任务的“结构化交接上下文”

当前执行记录和结果总结已经有了，但缺的是：

1. 前置任务给后置任务的交接摘要
2. 改了哪些文件
3. 执行了哪些关键命令
4. 哪些点已经完成
5. 哪些风险待验证
6. 当前任务应该消费哪个前置结果

因此即便以后拆成多个任务，如果没有 handoff 机制，后续任务还是会重复探索。

---

## 3. 目标行为

## 3.1 复杂需求不再直接落成一个大任务

未来联系人在规划阶段，应该优先产出一个**任务计划**，而不是一条超长任务。

例如“新增管理员全用户报表页”这类需求，至少应该拆成：

1. 需求澄清 / 约束确认任务
2. 后端接口实现任务
3. 前端页面接入任务
4. 测试 / 验证任务
5. 必要时再加文档或上线检查任务

### 规则

1. 单个任务只负责一个明确目标
2. 单个任务要能在一次执行里完成
3. 任务之间通过依赖表达顺序，不靠内容里写一大段自然语言

---

## 3.2 开发任务必须能挂测试/验证任务

对“开发 / 改造 / 修复 / 接入 / 重构”这类会改代码的任务，系统应该明确支持：

1. 一个实现任务
2. 一个依赖于实现任务的测试/验证任务

不是所有开发任务都必须跑“自动化测试”，但至少必须有一种验证方式：

1. 自动化测试
2. 手工验证
3. 构建验证
4. 运行验证

也就是说：

> 不是强制每次都要 `cargo test` / `npm test`，而是强制这类任务必须有“验证闭环”。

---

## 3.3 后续任务要消费前置任务的结果，而不是重新摸索

例如：

1. 实现任务完成后，测试任务不该再去重新读一遍大量项目内容
2. 它应该直接拿到“实现任务交接包”

这个交接包至少应包括：

1. 实现结果摘要
2. 关键改动文件
3. 关键命令和输出摘要
4. 未解决风险
5. 建议验证点
6. 相关执行记录 / 结果 brief 的引用

这样测试任务才真正是在“接棒”，而不是“重做一遍分析”。

---

## 4. 我建议的总体设计

## 4.1 从“单任务”升级为“任务计划 + 任务节点”

建议新增两层模型：

### A. 任务计划 `TaskPlan`

代表一次联系人对用户需求的规划结果。

建议字段：

1. `id`
2. `user_id`
3. `contact_agent_id`
4. `project_id`
5. `scope_key`
6. `source_session_id`
7. `source_turn_id`
8. `source_message_id`
9. `title`
10. `goal_summary`
11. `status`
12. `created_at`
13. `updated_at`
14. `confirmed_at`
15. `plan_summary`
16. `planning_snapshot`

### B. 任务节点 `TaskNode`

替代现在直接平铺的 `ContactTask`，或者作为 `ContactTask` 的增强版本。

建议新增字段：

1. `task_plan_id`
2. `task_ref`
3. `task_kind`
4. `depends_on_task_ids`
5. `dependency_policy`
6. `verification_of_task_id`
7. `blocked_reason`
8. `ready_at`
9. `handoff_summary`
10. `handoff_artifacts`
11. `acceptance_criteria`

其中：

### `task_kind`

建议先枚举这些：

1. `analysis`
2. `implementation`
3. `verification`
4. `documentation`
5. `delivery`
6. `migration`
7. `research`

### `dependency_policy`

建议先支持：

1. `all_success`
2. `any_success`
3. `manual_release`

当前第一阶段只落 `all_success` 就够了。

---

## 4.2 节点状态要引入“阻塞态”

当前只有：

1. `pending_confirm`
2. `pending_execute`
3. `running`
4. `paused`
5. `completed`
6. `failed`
7. `cancelled`

建议新增：

1. `blocked`
2. `skipped`

这样确认后的任务计划可以变成：

1. 无前置依赖的根任务 -> `pending_execute`
2. 有前置依赖的任务 -> `blocked`

当依赖全部成功后，再从 `blocked` 自动切到 `pending_execute`。

### 这样正好兼容你现在的语义

1. 初始化时：整份计划处于待确认，所有节点仍可统一看作 `pending_confirm`
2. 用户确认后：
   - 根任务：`pending_execute`
   - 依赖任务：`blocked`
3. 调度器只调度 `pending_execute`

这比现在“一确认就都进待执行”更准确。

---

## 4.3 `create_tasks` 升级为“创建任务计划”

当前 `create_tasks` 名字可以保留，但语义应该升级。

建议新 schema 的核心结构改成：

```json
{
  "plan_title": "新增管理员全用户报表页",
  "plan_goal_summary": "为 admin 提供按时间范围查看所有用户使用情况的统计与趋势页面",
  "tasks": [
    {
      "task_ref": "impl_backend",
      "title": "实现报表统计接口",
      "details": "新增管理员统计接口，支持时间范围筛选和聚合维度。",
      "task_kind": "implementation",
      "priority": "high",
      "required_builtin_capabilities": ["builtin_code_maintainer_read", "builtin_code_maintainer_write"],
      "required_context_assets": [],
      "acceptance_criteria": [
        "接口可按时间范围筛选",
        "返回按用户聚合的统计结果"
      ]
    },
    {
      "task_ref": "impl_frontend",
      "title": "接入管理员报表页面",
      "details": "新增 admin 报表页面并接入后端接口。",
      "task_kind": "implementation",
      "priority": "high",
      "depends_on_refs": ["impl_backend"],
      "required_builtin_capabilities": ["builtin_code_maintainer_read", "builtin_code_maintainer_write"],
      "required_context_assets": [],
      "acceptance_criteria": [
        "页面可查看报表",
        "桌面与移动端可正常加载"
      ]
    },
    {
      "task_ref": "verify_report",
      "title": "验证管理员报表功能",
      "details": "执行构建、测试或手工验证，确认报表链路可用。",
      "task_kind": "verification",
      "priority": "medium",
      "depends_on_refs": ["impl_backend", "impl_frontend"],
      "verification_of_refs": ["impl_backend", "impl_frontend"],
      "required_builtin_capabilities": ["builtin_code_maintainer_read", "builtin_terminal_controller"],
      "required_context_assets": [],
      "acceptance_criteria": [
        "至少完成一种可复现的验证方式",
        "输出验证结果与剩余风险"
      ]
    }
  ]
}
```

### 这里有两个关键点

1. 对 AI 暴露的是 `task_ref / depends_on_refs` 这种局部引用，不让它关心数据库 ID
2. 服务端在落库时把这些引用解析成真实依赖关系

这样既简化 AI 参数负担，又能把依赖关系表达清楚。

---

## 4.4 对开发任务加程序级约束，不只靠 prompt

你前面已经明确过很多次：

> 不能只靠 prompt 约束，关键规则必须程序里也管住。

这里同样适用。

### 规则 1：如果任务具备代码修改能力，则必须存在验证节点

判断条件建议优先基于结构，不基于中文关键词瞎猜：

1. 任务 `task_kind == implementation`
2. 或 `required_builtin_capabilities` 包含 `write` / `terminal`
3. 或存在 `project_root`

一旦命中，就要求同一计划中必须至少有一个：

1. `task_kind == verification`
2. 且依赖该实现任务

如果没有，就直接拒绝创建计划，让模型重新规划。

### 规则 2：验证节点不能先于实现节点进入待执行

这由依赖调度保证，不再靠模型“自觉”。

### 规则 3：验证节点必须产出明确结果

建议验证任务默认强制：

1. `execution_result_contract.result_required = true`
2. 必须输出验证结论
3. 必须说明通过 / 不通过 / 风险

---

## 4.5 调度器从线性队列升级为“单 scope 串行 + 依赖就绪”

仍然保留你现在的重要约束：

1. 一个联系人 scope 同时只跑一个任务
2. 如果有 `running` / `paused` / 控制请求，就不直接起下一个

但“选下一个任务”的规则要改为：

1. 从当前计划里找所有 `status == blocked` 或 `pending_execute` 的节点
2. 判断依赖是否全部成功
3. 依赖满足的 blocked 节点先释放为 `pending_execute`
4. 再从所有 `pending_execute` 的 ready 节点中按：
   - `plan_priority`
   - `queue_position`
   - `priority_rank`
   - `created_at`
   选出一个执行

### 额外规则

1. 如果某前置节点 `failed`
   - 其强依赖后继节点自动进入 `blocked`
   - `blocked_reason = upstream_failed`
2. 如果前置节点 `cancelled`
   - 默认后继节点也保持 `blocked`
3. 如果用户重新编排
   - 允许新增新的后继节点
   - 允许把旧 blocked 节点标为 `skipped`

---

## 4.6 前置任务的上下文交接设计

这是这次方案里最关键的一块之一。

如果只加依赖，不加上下文交接，后面的测试任务还是会重复工作。

所以建议新增一份结构化交接记录：

### `TaskHandoffPayload`

建议字段：

1. `task_id`
2. `task_plan_id`
3. `summary`
4. `result_summary`
5. `key_changes`
6. `changed_files`
7. `executed_commands`
8. `verification_suggestions`
9. `open_risks`
10. `artifact_refs`
11. `checkpoint_message_ids`
12. `result_brief_id`
13. `generated_at`

### 生成时机

在任务执行结束时：

1. 成功完成 -> 自动生成 handoff
2. 暂停 -> 生成 checkpoint handoff
3. 失败 -> 生成 failure handoff

### 后继任务取上下文的方式

后继任务启动时，执行上下文按下面顺序组织：

1. 当前联系人原本的聊天上下文策略
2. 当前任务自身定义
3. 所属 `TaskPlan` 的目标摘要
4. 直接前置任务的 `TaskHandoffPayload`
5. 必要时再追加祖先任务的简短摘要
6. 如果模型明确需要，再提供查看前置完整执行记录的工具

### 不建议默认直接塞整段执行聊天记录

因为这会导致：

1. token 爆炸
2. 任务越多越冗长
3. 测试任务再次看到大量低价值细节

所以默认应该是：

1. 给结构化 handoff
2. 给结果 brief
3. 给必要引用
4. 需要细查时再用工具获取完整记录

---

## 5. 对现有 IM 规划链路的影响

## 5.1 IM 规划阶段的职责要变成“生成任务计划”

不是“创建一条大任务”，而是：

1. 理解用户需求
2. 判断是否需要拆分
3. 生成任务计划
4. 发起确认
5. 本轮立即结束

这和你前一个方案是兼容的。

### 也就是说

前一个方案解决的是：

1. 任务一旦创建成功，本轮 IM 规划就立即结束

这一个方案解决的是：

1. 创建出来的内容不应该是一条大任务
2. 而应该是一组有顺序和依赖的任务节点

两者应该合并落地，不冲突。

---

## 5.2 规划 prompt 要强制模型优先拆分

建议在联系人任务规划系统提示里加入明确规则：

1. 如果需求涉及多个阶段，必须拆成多个任务
2. 如果一个任务同时包含后端、前端、验证，必须拆开
3. 如果任务会改代码，必须包含验证任务
4. 如果后续任务依赖前置结果，必须用依赖关系表达，不要把顺序只写在 details 里

### 推荐拆分触发条件

满足任一条就优先拆分：

1. 需求跨后端和前端
2. 需求同时包含开发与验证
3. 需求包含“先分析再修改再验证”
4. 任务描述已经出现多个独立交付物
5. 预估无法在一次任务执行内稳定完成

---

## 6. 前端展示也要跟着改

你这张截图已经说明一个问题：

> 现在任务详情页展示的是一条超长任务正文，对人并不友好。

引入任务计划后，前端应该改成：

## 6.1 任务服务前端

任务列表显示：

1. 计划标题
2. 当前计划状态
3. 节点数
4. 当前执行到哪一个节点

展开后显示：

1. 节点列表
2. 每个节点的 `task_kind`
3. 依赖关系
4. 当前状态
5. 是否为验证节点
6. 对应交接摘要按钮

## 6.2 Workbar

Workbar 不应该只把它们当成一串独立平铺任务。

建议显示成：

1. 当前计划
2. 当前进行中的节点
3. 后续 blocked / pending 节点
4. 当前执行链的进度

---

## 7. 第一阶段我建议怎么落地

为了不把改造做炸，建议分三阶段。

## 第一阶段：先把“任务计划 + 依赖 + 验证节点”跑通

先做：

1. 新增 `task_plan_id`
2. 新增 `task_ref`
3. 新增 `task_kind`
4. 新增 `depends_on_task_ids`
5. 新增 `verification_of_task_id`
6. 新增 `blocked` 状态
7. `create_tasks` 支持 `depends_on_refs`
8. 调度器按依赖释放 blocked 节点
9. 开发任务必须存在验证节点

先不做：

1. 复杂 DAG 可视化
2. 自动重排历史计划
3. 跨计划引用
4. 复杂 dependency_policy

---

## 第二阶段：补上下文交接

做：

1. `TaskHandoffPayload`
2. 前置任务结果结构化写入
3. 后继任务启动时注入 handoff
4. 前端可查看交接详情

---

## 第三阶段：补计划级 UI 与重排

做：

1. 计划级别确认界面
2. 计划视图
3. 节点依赖展示
4. 用户新消息触发重新编排时，允许：
   - 新增节点
   - 取消节点
   - 跳过节点
   - 重挂依赖

---

## 8. 我建议的实施顺序

### 第 1 步：扩任务模型

改：

1. `contact_task_service/backend/src/models.rs`
2. `contact_task_service/backend/src/repository.rs`
3. 对应 Mongo 索引

### 第 2 步：扩 `create_tasks` schema 和 review payload

改：

1. `chat_app_server_rs/src/builtin/task_planner/mod.rs`
2. `chat_app_server_rs/src/builtin/task_planner/parsing.rs`
3. `chat_app_server_rs/src/builtin/task_planner/review_flow.rs`
4. `chat_app_server_rs/src/services/task_manager/*`

### 第 3 步：改确认逻辑

用户确认的不是“孤立单任务”，而是整份计划。

确认后：

1. 根节点 -> `pending_execute`
2. 依赖节点 -> `blocked`

### 第 4 步：改调度器

改：

1. `scheduler_next(...)`
2. 依赖释放逻辑
3. 失败/取消后的下游阻塞逻辑

### 第 5 步：补 handoff

改：

1. 任务执行完成写入 handoff
2. 后继任务上下文组装读取 handoff

### 第 6 步：改前端展示

改：

1. task service 前端
2. Workbar
3. IM 里的任务确认弹层

---

## 9. 关键设计选择

## 9.1 我不建议直接做“任意并行 DAG”

虽然从理论上可以支持，但你现在系统里很明确是：

1. 单联系人 scope 串行执行
2. 用户可以随时通过 IM 干预
3. 暂停/停止/恢复已经是 scope 级控制

所以第一阶段更适合：

1. 支持任务图
2. 但实际执行仍然单 scope 串行
3. 依赖只是决定“谁有资格进入 ready”

这既满足拆分和前置关系，又不会把控制面复杂度一下拉爆。

---

## 9.2 我不建议让 AI 直接传数据库 ID 级依赖

AI 只应该传：

1. `task_ref`
2. `depends_on_refs`
3. `verification_of_refs`

由程序解析成真实 ID。

这样：

1. 参数简单
2. review 阶段稳定
3. 用户编辑时也更友好

---

## 9.3 我建议“验证任务”做成显式节点，不要只做布尔开关

不要只加一个：

1. `needs_test = true`

因为这会让“验证过程”在系统里不可见。

更好的做法是：

1. 验证本身就是一个节点
2. 有自己的状态
3. 有自己的执行记录
4. 有自己的结果摘要

这样用户才看得懂当前到底卡在实现，还是卡在验证。

---

## 10. 最终效果

如果这套方案落下去，未来像你截图这种需求，系统表现应该变成：

1. 联系人先给出一份任务计划，而不是一条超长任务
2. 用户确认的是计划
3. 后端实现、前端接入、测试验证分别是独立节点
4. 测试节点必须等开发节点完成
5. 测试节点自动拿到开发节点的交接摘要
6. Workbar 和任务服务都能清楚看到当前执行链
7. 用户在执行中再发消息时，AI 可以基于现有计划重排，而不是只会往单一任务里追加内容

---

## 11. 我的建议

这件事我建议作为你下一轮任务系统重构的主线来做，优先级很高。

因为它解决的不是一个小 bug，而是现在任务系统最核心的三个体验问题：

1. 任务粒度太粗
2. 没有依赖关系
3. 没有前后文交接

如果你认可，我下一步建议就按这份方案直接开始做第一阶段：

1. 先改模型
2. 再改 `create_tasks`
3. 再改确认和调度
4. 最后补前端
