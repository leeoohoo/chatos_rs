---
name: requirement-survey-review-execution
description: Compare a survey resolution's execution plan with current project task facts and identify completion, blockers, and gaps.
metadata:
  chatos.role: leaf
---

# 核对方案执行进度

目标是将一张调研的正式执行计划与当前项目团队任务事实对照，说明已完成、进行中、阻塞、未覆盖和无法确认的部分。

## 执行顺序

1. 调用 `requirement_survey_list` 和 `requirement_survey_get` 定位基准方案，提取 resolution summary 及每个 execution_step 的目标、交付物和验收条件。没有 resolution 时报告缺少正式计划并退出。
2. 调用 `requirement_survey_project_tasks`，读取项目任务的 objective、scope、status、负责人、基础能力、blocked_reason 和 result。
3. 按目标、范围、交付物和验收条件匹配步骤与任务，不只按标题。一个步骤可以对应多个任务；无法可靠匹配时标记“未确认”。
4. 只有任务为 completed 且 result 提供与验收条件相关的结果，才标记步骤已完成。in_progress 表示执行中；blocked 必须带出原因；没有对应任务的步骤标记为未覆盖。
5. 分别输出总体进度、步骤证据、阻塞项、未覆盖项和建议的下一任务，不修改调研或任务状态。

resolution 存在本身不是执行完成证据。需要确认输出形式时读取 [进度核对示例](references/example.md)。
