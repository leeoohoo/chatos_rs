---
name: solution-task-execution
description: Claim dependency-ready Solution Studio tasks, execute their acceptance contract with separately authorized project tools, and write back blocked, completed, or cancelled state.
metadata:
  chatos.role: leaf
  chatos.activation-policy: model-or-user
  chatos.context-mode: inline
---

# Solution Task Execution

This Skill is for the independently selectable “任务执行器” component. Use the Solution Studio task list as the authority for execution order and status. Start by calling `solution_list_tasks` with the ready filter or reading the target through `solution_get_task`. Do not infer readiness from visual order, phase names, or prose.

Claim exactly one ready task with `solution_claim_task` before changing project files or running commands for it. Resume a blocked task only with `solution_resume_task` after its blocking condition has actually cleared. The Solution Studio executor does not itself grant file, terminal, browser, or other project capabilities; use only the separately authorized tools attached to the current Task.

Execute the claimed task against its stated deliverables and acceptance criteria. When progress cannot continue, call `solution_block_task` with the concrete blocking condition. Do not rewrite dependencies, requirements, design, or task definitions to make a task appear ready.

Call `solution_complete_task` only after verifying the acceptance criteria. Include a concise completion summary and durable evidence references when available. A tool call, code change, or command exit by itself is not completion evidence unless it verifies the task contract.

Use `solution_cancel_task` only when the task is intentionally abandoned or superseded, and provide the reason. Completed and cancelled tasks are terminal. They cannot be reopened or edited; select the separate “方案创建器” component to define a replacement task.
