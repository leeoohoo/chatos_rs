---
name: requirement-survey
description: Decide when a project or change needs structured Human requirement confirmation, then route to the matching creation, reading, resolution, or execution-review workflow.
metadata:
  chatos.role: router
  chatos.related-skills: "requirement-survey-create,requirement-survey-read-results,requirement-survey-resolve,requirement-survey-review-execution"
---

# 需求调研

需求调研把会影响范围、方案、风险、时间或验收的 Human 决策，沉淀为当前项目下可追踪的结构化记录。Human 提交后，同一记录继续承载解决方案和执行计划。

## 什么时候使用

- 新项目或新需求存在关键取舍，未确认就无法可靠规划或实施；
- 重大变更前需要确认范围、兼容策略、迁移方式、优先级或验收口径；
- 需要读取 Human 已提交的选择与统一备注，而不是依赖聊天摘要；
- 需要根据已提交结果形成正式解决方案和执行计划；
- 需要把既有方案与当前项目任务状态进行核对。

信息已经明确，或只是不会影响后续工作的临时沟通时，不创建调研。项目由程序绑定，不向 Human 询问项目、Team 或 Room ID。

## 场景路由

一次只激活当前目标对应的专业 Skill：

- 缺少关键 Human 决策：激活 `requirement-survey-create`；
- 读取答案、备注、历史决定或既有方案：激活 `requirement-survey-read-results`；
- Human 已提交，需要形成正式方案：激活 `requirement-survey-resolve`；
- 已有方案，需要核对真实执行状态：激活 `requirement-survey-review-execution`。

目标变化时再激活另一个专业 Skill。创建和生成方案需要写入工具；若当前任务没有对应工具，报告能力配置不匹配。
