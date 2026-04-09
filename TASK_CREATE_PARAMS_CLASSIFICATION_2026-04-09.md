# 创建任务参数清单（模型输入 vs 程序透传）

- 日期：2026-04-09
- 范围：当前 `create_tasks` 链路（`task_planner` / `task_executor` -> `task_manager` -> `contact_task_service`）
- 目的：把“哪些参数必须模型给、哪些由程序透传/补齐”标清楚

## 1. 调用链路（便于看参数来源）

1. 模型调用 MCP `create_tasks`（参数来自 tool schema）。
2. `parse_task_drafts` 解析为 `TaskDraft`（含 camel/snake 兼容和部分别名）。
3. `review_flow` 审批后得到 `decision.tasks`（可能被人工确认页调整）。
4. `create_tasks_for_turn` 做补齐/推断/校验，组装 `CreateTaskRequestDto`。
5. 调 `contact_task_service` 的 `create_task` 落库。

## 2. 模型可传参数（MCP 入参层）

说明：下表里的“模型必须给”是指**不提供就无法创建任务**。

| 字段 | 模型必须给 | 程序如何处理 | 最终是否进入创建请求 |
|---|---|---|---|
| `title` | 是 | 去空格后校验非空；为空报错 | 是（`title`） |
| `details` | 否 | 去空格；为空时 `content` 回退 `title` | 间接是（映射到 `content`） |
| `description`（别名） | 否 | 作为 `details` 别名 | 间接是 |
| `priority` | 否 | 默认 `medium`，并归一化 `high/medium/low` | 是（`priority`） |
| `task_kind` | 否 | 归一化到枚举；未知值会变 `general` | 是（`task_kind`） |
| `task_ref` | 否 | implementation 且缺失时自动生成 `impl_*` | 是（`task_ref`） |
| `depends_on_refs` | 否 | 先缓存 ref，创建后映射成 task_id 再 patch | 间接是（后续写 `depends_on_task_ids`） |
| `verification_of_refs` | 否 | 同上，后置映射并 patch | 间接是（后续写 `verification_of_task_ids`） |
| `acceptance_criteria` | 否 | 去重去空 | 是 |
| `required_builtin_capabilities` | 否 | token -> builtin_mcp_id，校验合法和授权；再和自动推断合并 | 间接是（进 `planned_builtin_mcp_ids`） |
| `required_context_assets[].asset_type` | 否 | 支持 `skill/plugin/common`（代码也兼容 `commons`） | 间接是（进 `planned_context_assets`） |
| `required_context_assets[].asset_ref` | 否 | 解析为运行时真实资产 id | 间接是 |
| `execution_result_contract.result_required` | 否 | 默认 `true` | 是 |
| `execution_result_contract.preferred_format` | 否 | 原样透传 | 是 |
| `tags` | 否 | 可解析（数组/逗号串） | 否（当前创建链路不写入 task_service） |
| `due_at` / `dueAt` | 否 | 可解析 | 否（当前创建链路不写入 task_service） |
| `planned_builtin_mcp_ids`（隐藏字段） | 否 | 可被解析；会参与后续推断合并 | 是（但不建议模型直接传） |
| `planned_context_assets`（隐藏字段） | 否 | 可被解析；再做 hydrate/补齐 | 是（但不建议模型直接传） |

补充：

1. 顶层支持两种形态：`tasks: [...]` 或单任务顶层字段（`title` 等）。
2. 若 `tasks: []` 且顶层有 `title`，会回退成单任务。
3. 若 implementation 没有对应 verification，会自动创建 verification 任务（不是模型必传）。

## 3. 最终创建请求 `CreateTaskRequestDto` 全字段来源矩阵

来源标记：

- `模型输入`：来自 MCP 参数（含人工 review 可能改写）。
- `程序透传`：从当前 session/task/runtime 直接带入。
- `程序推断/生成`：程序计算出来。
- `程序固定值`：代码写死。

| 最终字段 | 来源分类 | 当前来源 | 说明 |
|---|---|---|---|
| `user_id` | 程序透传 | `resolve_task_scope_context(session)` | 模型不需要给 |
| `contact_agent_id` | 程序透传 | session/metadata/contact 解析 | 模型不需要给 |
| `project_id` | 程序透传 | session/metadata | 模型不需要给 |
| `task_plan_id` | 程序推断/生成 | `plan_<uuid>` | 每次创建批次自动生成 |
| `task_ref` | 模型输入 + 程序推断 | 来自 draft；implementation 缺失会自动补 | 可能被自动生成 |
| `task_kind` | 模型输入 | draft.task_kind | 未传则 `None` |
| `depends_on_task_ids` | 程序推断/生成 | 创建时先空，后根据 `depends_on_refs` patch | 模型不直接传 id |
| `verification_of_task_ids` | 程序推断/生成 | 同上 | 模型不直接传 id |
| `acceptance_criteria` | 模型输入 | draft.acceptance_criteria | 去重后写入 |
| `project_root` | 程序透传 | scope.project_root | 模型不需要给 |
| `remote_connection_id` | 程序透传 | scope.remote_connection_id | 模型不需要给 |
| `session_id` | 程序透传 | 当前 tool context / 当前 task | 模型不需要给 |
| `conversation_turn_id` | 程序透传 + 生成 | 当前 turn；执行态缺失时生成 `task-exec-<id>` | 模型不需要给 |
| `source_message_id` | 程序固定值 | `None` | 当前链路未使用 |
| `model_config_id` | 程序透传 | 联系人有效模型配置 | 模型不需要给 |
| `title` | 模型输入 | draft.title | 唯一硬必填输入 |
| `content` | 模型输入 + 程序推断 | `details`，为空时回退 `title` | 由程序构造 |
| `priority` | 模型输入 + 程序默认 | draft.priority（默认 medium） | 一定会传 Some |
| `confirm_note` | 程序固定值 | `None` | 当前链路未使用 |
| `execution_note` | 程序固定值 | `None` | 当前链路未使用 |
| `planned_builtin_mcp_ids` | 程序推断/合并 | `required_builtin_capabilities` 解析 + runtime enabled mcp + 默认/文本推断 +（可含隐藏字段输入） | 服务端要求非空 |
| `planned_context_assets` | 程序推断/合并 | `required_context_assets` 解析 + runtime selected commands 合并 + hydrate | 模型通常不用直接给 |
| `execution_result_contract` | 模型输入 + 程序默认 | 未给则默认 `{result_required: true}` | 总会传 Some |
| `planning_snapshot` | 程序推断/生成 | 当前轮消息摘要 + 约束 + scope/runtime 快照 | 模型不需要给 |

## 4. 程序自动校验与覆盖点（模型常误判区）

1. `required_builtin_capabilities` 只认 registry token，且必须在联系人授权范围内。
2. 即使模型不传能力，程序也会根据 runtime 默认能力和文本做能力推断。
3. capability runtime 要求不满足会报错（如无 `project_root` 却要 `write/terminal`；无 `remote_connection_id` 却要 `remote`）。
4. `required_context_assets` 不是原样透传：会被解析成真实资产 id，不存在会报错。
5. `depends_on_refs/verification_of_refs` 不是创建时直接落库字段，需先建完再映射 task_id patch。

## 5. 目前“模型可给但不会落库”的字段

1. `tags`
2. `due_at` / `dueAt`

这两个字段在 MCP schema 与 parser 中存在，但当前创建流程没有写进 `task_service` 创建请求。

## 6. 结论（给你一眼判断）

1. 模型真正“必须给”的只有：`title`（单任务）或 `tasks[i].title`（批量）。
2. 大部分上下文类字段（user/contact/project/session/turn/model/project_root/remote）都是程序透传。
3. 能力与上下文资产本质是“模型可提示 + 程序解析/推断/校验”的混合模式。
4. 当前参数复杂感主要来自 MCP 层可选字段多，不是因为创建接口真的都要模型手填。

## 7. 代码依据

1. `agent_orchestrator/src/builtin/task_planner/mod.rs`
2. `agent_orchestrator/src/builtin/task_executor/mod.rs`
3. `agent_orchestrator/src/builtin/task_planner/parsing.rs`
4. `agent_orchestrator/src/services/task_manager/store/create_ops.rs`
5. `agent_orchestrator/src/services/task_manager/store/create_ops/draft_graph.rs`
6. `agent_orchestrator/src/services/task_manager/store/remote_support.rs`
7. `agent_orchestrator/src/services/task_service_client.rs`
8. `contact_task_service/backend/src/models.rs`
9. `contact_task_service/backend/src/repository.rs`
