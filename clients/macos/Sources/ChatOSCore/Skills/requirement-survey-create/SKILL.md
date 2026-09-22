---
name: requirement-survey-create
description: Create one deduplicated project-bound choice survey when a material Human decision is missing.
metadata:
  chatos.role: leaf
---

# 创建需求调研

目标是把一个会影响后续工作的 Human 决策整理成一张不重复、可直接选择、状态为 `pending` 的调研单。创建成功即结束；不要等待或代替 Human 提交。

## 执行顺序

1. 从任务合同中区分已确认事实、尚未确认的决定，以及该决定影响的范围、方案、风险、时间或验收。不会改变后续工作的缺失信息不需要调研。
2. 调用 `requirement_survey_list(status="pending")` 查重；对标题、目的或边界相近的候选逐一调用 `requirement_survey_get`。
3. 已有同主题 pending 调研时返回已有记录并退出；历史决定仍适用时复用并退出；否则继续。
4. 一张单只处理一个主题。设计 1–12 个问题，每题只问一个维度，只使用 `single_choice` 或 `multiple_choice`，每题提供 2–12 个具体、平行、可执行的选项。页面统一提供“备注”，不要创建自由文本题。
5. 使用稳定、语义化的 request、question 和 option key 调用 `requirement_survey_create`。
6. 验证返回的 `project_bound=true`、状态为 `pending`，题目和选项完整。结果不明确时只能用相同 request_key 和完全相同的内容重试。

需要确认参数形状时，读取 [创建示例](references/example.md)。最终只报告调研主题、需要 Human 决定的内容和当前状态。
