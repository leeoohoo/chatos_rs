---
name: requirement-survey-read-results
description: Read a project survey and distinguish Human choices, notes, existing resolutions, and unresolved questions.
metadata:
  chatos.role: leaf
---

# 读取需求调研结果

目标是从当前项目的真实调研记录中，准确区分 Human 选择、Human 备注、既有解决方案和仍未确认事项。

## 执行顺序

1. 处理刚提交的调研时调用 `requirement_survey_list(status="submitted")`；回顾历史决定时可不传 status。
2. 根据标题、purpose 和任务边界选择候选，再调用 `requirement_survey_get`；不要凭标题直接下结论。
3. 逐项读取 status、问题、选项、selected、selected_option_keys 和独立的 notes，再读取 resolution 的 summary、solution_markdown、execution_steps、风险与相关资料。
4. `pending` 只表示仍待 Human 提交；`submitted` 且 resolution 为空表示 Human 决策已具备但正式方案尚未形成；resolution 非空时分开描述 Human 决策和 Agent 方案。
5. 输出 Human 已确认、备注补充、既有方案与执行步骤、尚未确认或冲突，以及使用的调研标题和状态。

survey_id 不存在时重新 list。聊天摘要和 Agent Memory 只能帮助定位，不能替代原始记录。需要输出样例时读取 [结果摘要示例](references/example.md)。
