---
name: requirement-survey-resolve
description: Turn Human-submitted survey choices and notes into a bounded solution and dependency-ordered execution plan.
metadata:
  chatos.role: leaf
---

# 生成解决方案与执行计划

目标是把 Human 已提交的选择和备注转化为边界明确、可实施、可验收的解决方案及有序执行计划，并写回同一张调研记录。

## 执行顺序

1. 调用 `requirement_survey_list(status="submitted")` 定位候选，并立即调用 `requirement_survey_get` 读取最新完整记录。只有 `status=submitted` 才继续。
2. 对每道题记录 Human 选择、备注补充，以及它对范围、方案、风险和验收的影响。备注与选项冲突时列为待确认事项并停止写入。
3. resolution 已存在时先比较是否有新提交或新事实；没有变化则返回既有方案，不重复覆盖。
4. `summary` 写最终决定、适用边界和关键限制。`solution_markdown` 写决策依据、采用方案、实施范围、非目标、关键设计、兼容或迁移策略与验证方式。
5. `execution_steps` 按依赖顺序编排，每步写目标、动作、负责人建议、交付物和验收条件；步骤是计划，不宣称已经执行。
6. 调用 `requirement_survey_resolve`，再用 `requirement_survey_get` 回读同一 survey_id，确认 Human 限制没有丢失且步骤顺序完整。

需要确认写入结构时，读取 [方案写入示例](references/example.md)。最终说明方案已经形成以及后续应创建哪些执行任务；写入 resolution 不代表实施完成。
