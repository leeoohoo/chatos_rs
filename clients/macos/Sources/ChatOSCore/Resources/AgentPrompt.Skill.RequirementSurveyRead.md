# Skill：读取需求调研与项目执行事实

此 Skill 只在当前 Todo 获得 `requirement_survey_read` 时生效。项目由程序绑定；不得询问、猜测或传入项目 ID、Team ID、Room ID。

## 1. 先判断目标

只读能力用于：查重已有调研、读取 Human 的选项答案与备注、复用既有方案和执行计划、查看当前项目任务及执行状态。它不能创建调研或写入方案。

开始时明确本轮属于哪一种：`查重`、`读取答案`、`复用方案`、`核对执行进度`。目标不明确时先读当前 Todo 合同，不要枚举无关数据。

## 2. 阶段一：定位调研

输入：当前 Todo 的目标、范围、已知标题或状态。

动作：

1. 调用 `capability_search` 搜索“需求调研”；
2. `capability_describe` 展开任务已授权的基础工具；
3. 调用 `requirement_survey_list`。创建前查重用 `status=pending`；处理 Human 提交用 `status=submitted`；回顾全量时可不传状态；
4. 对可能相关的候选逐一调用 `requirement_survey_get`，不能只凭标题作结论。

验证：返回 `project_bound=true`；目标 `survey_id` 必须来自本轮 list；标题、purpose 和问题边界与当前 Todo 一致。

出口：唯一定位后进入阶段二；没有记录时明确报告“当前项目未找到”，不要编造。

## 3. 阶段二：读取 Human 决策

输入：`requirement_survey_get` 的完整结果。

动作：逐题读取 prompt、options、selected 标记和 `selected_option_keys`，再单独读取 `notes`。之后读取 `resolution`：包括 summary、solution_markdown、execution_steps、风险和相关资料。

验证：

- `pending` 表示 Human 尚未提交，不得推断答案；
- `submitted` 必须同时检查选项和备注，备注可能补充、限制或纠正选项；
- resolution 为空表示尚无正式方案；非空时原样区分 Human 决策与 Agent 方案。

出口：输出事实摘要，并标明哪些来自 Human、哪些来自既有方案、哪些仍未确认。

## 4. 阶段三：核对项目任务

仅当当前 Todo 需要了解执行状态时调用 `requirement_survey_project_tasks`。按团队读取任务目标、状态、负责人、基础能力、阻塞原因和结果。

验证：`completed` 只能说明该 Todo 已完成；调研 resolution 的存在不等于执行完成。没有团队或没有任务时如实报告空结果。

## 5. 错误恢复与禁止事项

- survey_id 失效或不存在：重新 list，不得猜 ID。
- 未找到读取工具：停止并报告任务能力配置错误。
- 不得替 Human 选择、不得把 pending 当 submitted、不得写入方案、不得以 Agent Memory 或聊天摘要替代真实调研结果。
