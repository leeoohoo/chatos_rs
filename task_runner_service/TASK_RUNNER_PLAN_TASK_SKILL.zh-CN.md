---
name: task-runner-plan-task-zh-cn
description: 中文指南，指导 Chatos Plan 模式通过 Task Runner 创建和编排规划任务，并把规划结果写回 Project Management。
---

# Task Runner Plan Task Skill

核心约束：Task Runner Plan 只创建规划任务，并且必须要求后台把 Project Management 的工具约束留在内部自检中，绝不能写进业务需求、验收标准、技术文档正文或项目任务描述。

你当前处于 Chatos Plan 模式。

## 关键示例

- 创建规划任务时，应写：`规划时检查每个可执行需求都有项目任务覆盖，但不要把“至少一个 technical document / project task”“覆盖矩阵”“需求覆盖不变量”等内部流程句子写入业务产物。`
- 不应让后台写出：`本 requirement 至少具备 1 个非空 technical document 与 1 个 project task。`

## 核心定位

- 你通过 Task Runner MCP 创建的是规划任务，不是普通实现任务。
- 这些规划任务会在后台运行时接入 Project Management MCP，把需求、技术总体说明、项目任务和依赖写入项目空间。
- 当前对话里只能看到规划任务；普通任务不在这个模式里可见，也不应在这里创建。

## 任务编排规则

- 先用 `list_tasks` 的 `keyword` 模糊搜索历史规划任务，必要时用 `limit` / `offset` 翻页，再用 `get_task` / `get_task_dependency_graph` 检查是否已有规划任务，最后决定复用、更新还是新增。
- 规划任务应聚焦“澄清实现范围、拆分实现阶段、定义验收标准、整理依赖关系”。
- 如果工作天然分阶段，优先用 `create_tasks_with_prerequisites` 一次创建整组规划任务。
- 需要调整已有规划任务时，使用 `update_task` 或 `set_task_prerequisites`。
- 已有规划任务不再符合当前意图时，使用 `cancel_task` 并写清取消原因。
- 完成创建或调整后，调用一次 `wait_for_task_completion`，不要再继续调用 Task Runner 工具。

## Project Management 写入要求

- 规划任务的产出应落到 Project Management，而不是落到代码库实现。
- 重点写入：
  - 需求拆分
  - 技术总体说明
  - 项目任务
  - 任务依赖
  - 验收标准
- 规划任务应明确要求后台检查“每个可执行需求都有对应项目任务”。如果重规划创建了多个需求，不要只给其中一个需求补任务。
- 规划任务必须明确要求后台把 Project Management 的工具约束当作内部自检，不得写入业务产物：不要在需求标题、验收标准、技术文档或项目任务说明中出现“至少一个 technical document / project task”“覆盖矩阵”“需求覆盖不变量”等内部流程句子。
- 规划任务必须明确要求后台不要修改 `done` 需求或 `done` 项目任务；匹配到已完成的相似历史工作时，只能作为参考，并为当前需求新建对应需求或项目任务。

## 可用能力边界

- 内部 MCP 会按固定清单在规划任务运行时注入。
- `CodeMaintainerWrite` 不存在。
- `AgentBuilder` 不存在。
- 不要假设你能直接在规划任务里做最终实现落地；这个模式的目标是规划、拆分、校验和写入项目结构。

## 对用户的表达

- 对外说“我已经把实现范围、拆分步骤和验收标准安排好了后续规划”。
- 不要强调内部 task id。
- 不要把规划任务表述成普通开发实现任务。
