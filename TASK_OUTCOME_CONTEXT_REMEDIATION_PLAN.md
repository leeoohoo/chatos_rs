# 任务成果沉淀与上下文联动整改方案

## 1. 背景与问题

当前任务系统已经具备这些能力：

- `task_manager` 可以创建、列出、更新、完成、删除任务。
- 任务看板会被拼进 system prompt，作为 `task_runtime_board` 注入模型上下文。
- 任务看板也会同步到 turn runtime snapshot，前端可在 runtime drawer 里看到。
- Workbar / 历史任务抽屉可以展示当前轮和历史任务。

但当前实现里，任务完成后只会把 `status` 改成 `done`，没有沉淀“本次任务做了什么、得到什么结论、哪些信息值得后续直接复用”。

这会导致两个明显问题：

- 对模型来说：后续轮次虽然能看到“这个任务 done 了”，但看不到 done 的成果，因此容易重复探索。
- 对用户来说：任务看板和历史任务只像状态板，不像工作成果板，缺少可复用的关键结论。

## 2. 现状核查结论

### 2.1 任务数据模型里没有成果字段

当前 `TaskRecord` 只有这些字段：

- `title`
- `details`
- `priority`
- `status`
- `tags`
- `due_at`
- `created_at`
- `updated_at`

代码位置：

- [chat_app_server_rs/src/services/task_manager/types.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/task_manager/types.rs)
- [chat_app_server_rs/src/services/task_manager/store/row.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/task_manager/store/row.rs)
- [chat_app_server_rs/src/services/task_manager/mapper.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/task_manager/mapper.rs)

### 2.2 完成任务接口只改状态，不收成果

当前 `complete_task` 工具没有成果参数：

- 内置工具 `complete_task` 只收 `task_id`
- HTTP `/api/task-manager/tasks/:task_id/complete` 也没有 body 字段
- 存储层 `complete_task_by_id(...)` 本质是 `update_task_by_id(..., { status: done })`

代码位置：

- [chat_app_server_rs/src/builtin/task_manager/mod.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/task_manager/mod.rs)
- [chat_app_server_rs/src/api/task_manager.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/task_manager.rs)
- [chat_app_server_rs/src/services/task_manager/store/write_ops.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/task_manager/store/write_ops.rs)

### 2.3 任务看板 prompt 只列状态，不列成果

当前任务看板 prompt 会列：

- 当前执行任务
- 已完成任务历史
- 每个任务的 `title/details/priority/status/due_at`

但不会列：

- 任务成果摘要
- 已确认的重要发现
- 后续复用提示
- 关键文件 / 关键命令 / 关键结论

代码位置：

- [chat_app_server_rs/src/services/task_board_prompt.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/task_board_prompt.rs)

### 2.4 runtime context 已经有很好的挂载点

当前 `task_board_prompt`：

- 会进入模型的 runtime prefixed system prompt
- 会进入 turn runtime snapshot 的 `system_messages`
- 会在任务变更后通过 `TASK_BOARD_UPDATED` 事件刷新

所以整改不需要新造一套上下文链路，直接增强 task board 内容即可。

代码位置：

- [chat_app_server_rs/src/services/task_board_prompt.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/task_board_prompt.rs)
- [chat_app_server_rs/src/core/turn_runtime_snapshot.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/core/turn_runtime_snapshot.rs)
- [chat_app_server_rs/src/api/chat_stream_common.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/chat_stream_common.rs)

### 2.5 前端 Workbar 也只展示管理字段

当前 Workbar / 历史任务 / task tool card 只展示：

- title
- details
- priority
- status
- due_at
- tags

没有“成果卡片”或“关键结论”区域。

代码位置：

- [chat_app/src/components/TaskWorkbar.tsx](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/TaskWorkbar.tsx)
- [chat_app/src/components/taskWorkbar/TaskCard.tsx](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/taskWorkbar/TaskCard.tsx)
- [chat_app/src/components/taskWorkbar/TaskHistoryDrawer.tsx](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/taskWorkbar/TaskHistoryDrawer.tsx)
- [chat_app/src/components/toolCards/taskManager/TaskManagerToolDetails.tsx](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/toolCards/taskManager/TaskManagerToolDetails.tsx)
- [chat_app/src/components/chatInterface/helpers.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/chatInterface/helpers.ts)

## 3. 整改目标

这次整改建议明确分成两个层次：

### 3.1 任务级成果沉淀

每个任务在推进、完成、阻塞时，不仅记录状态，还要记录：

- 这次做了什么
- 得到了什么重要信息
- 哪些信息值得后续直接复用
- 后续任务接手时应避免重复做什么
- 如果阻塞了，是因为什么阻塞
- 为了解除阻塞，下一步缺什么条件

### 3.2 上下文级复用

这些成果应当反映到至少两个地方：

- 任务看板 prompt：让模型下一步就能直接看到
- 任务看板 UI / 历史任务 UI：让用户也能直接看到

更长期可以再考虑把高价值成果同步进入 memory summary，但这不应成为第一阶段前置条件。

## 4. 设计原则

### 4.1 `details` 继续描述“任务意图”，不要混成成果

建议保留 `details` 的原语义：

- 它更适合描述任务目标、执行要求、约束

新增独立成果字段，而不是把成果硬塞进 `details`。否则会出现：

- 任务开始前和完成后语义混杂
- 看板里难以区分“要做什么”和“已经得到什么”
- 更新 patch 会越来越脏

### 4.2 成果要区分“简短摘要”和“结构化证据”

建议不要只加一个长文本字段。至少拆成两层：

- 简短摘要：给 task board prompt 和 Workbar 卡片直接展示
- 结构化成果：给历史抽屉、工具结果卡片、后续增强使用

### 4.3 强约束在“完成/阻塞时必须补成果”，弱约束在“推进中可追加成果”

建议：

- `complete_task` 时，默认要求提供成果摘要
- `update_task` 把状态改为 `blocked` 时，默认要求提供阻塞成果
- `update_task` 时，允许追加中间成果

这样既能保证完成任务和阻塞任务都有沉淀，又不阻塞中间阶段的灵活更新。

### 4.4 任务看板里只放“高密度、低噪音”的成果

task board prompt 是 system context，不能把长篇原始证据直接灌进去。看板里应只放：

- 1-3 行成果摘要
- 最重要的发现
- 必要的 next hint / avoid redo hint

详细证据仍放在结构化字段和 UI 详情里。

## 5. 建议的数据模型

建议在 `TaskDraft` / `TaskUpdatePatch` / `TaskRecord` 中新增以下字段。

### 5.1 第一层：直接可展示的成果摘要

- `outcome_summary: String`
  说明：
  当前任务的简短成果摘要。适合 1-3 句，供 task board prompt、Workbar 卡片、tool result card 直接展示。

对 `done` 和 `blocked` 都适用：

- `done`：说明完成了什么、得到了什么结论
- `blocked`：说明已经做了什么、确认卡在什么地方

### 5.2 第二层：结构化成果

- `outcome_items: Vec<TaskOutcomeItem>`
  说明：
  结构化成果条目列表，适合沉淀关键发现、结论、决策、风险、产出位置。

建议结构：

```ts
type TaskOutcomeItem = {
  kind: 'finding' | 'decision' | 'artifact' | 'risk' | 'handoff';
  text: string;
  importance?: 'high' | 'medium' | 'low';
  refs?: string[];
};
```

字段解释：

- `finding`: 找到了什么事实
- `decision`: 做了什么判断/取舍
- `artifact`: 产出物位置，如文件、接口、PR、命令结果
- `risk`: 尚未解决的风险
- `handoff`: 给下一任务的接手提示

### 5.3 阻塞专用字段

- `blocker_reason: String`
  说明：
  当前为什么 blocked，要求写成可复用的事实性描述，而不是笼统的“有问题”“失败了”。

- `blocker_needs: Vec<String>`
  说明：
  要继续推进，缺哪些前置条件。比如：
  - 需要用户提供某个账号/权限
  - 需要某个服务先恢复
  - 需要确认设计决策
  - 需要另一个任务先完成

- `blocker_kind: String`
  可选枚举建议：
  - `external_dependency`
  - `permission`
  - `missing_information`
  - `design_decision`
  - `environment_failure`
  - `upstream_bug`
  - `unknown`

说明：

- `blocked` 不是单纯状态，而应该是“有明确上下文的阻塞状态”
- 后续任务或下一轮模型应能直接看懂：这次已经做了哪些尝试、为什么仍然卡住、需要什么才能解阻

### 5.4 第三层：反重复探索提示

- `resume_hint: String`
  说明：
  专门给后续任务/后续轮次看的“不要重复探索”的短提示。

示例：

- `已确认问题根因在 task_board_prompt 只展示 status，不需要再排查 Workbar 刷新链路`
- `目录读取工具不返回总行数，远程 read_file 也没有 total_lines`

### 5.5 审计字段

- `completed_at: Option<String>`
- `last_outcome_at: Option<String>`

说明：

- `completed_at` 用来区分真正完成时间和普通 `updated_at`
- `last_outcome_at` 用来标识最近一次成果沉淀时间

## 6. 工具与 API 协议整改

## 6.1 `complete_task` 工具升级

当前：

- 只收 `task_id`

建议改为：

- `task_id`
- `outcome_summary` 可选但强烈建议，进入 done 时若为空应返回 warning，后续可升级为必填
- `outcome_items` 可选
- `resume_hint` 可选

建议第一阶段先做“软强制”：

- 如果没有传 `outcome_summary`，仍允许完成
- 但在 tool result 中返回显式 warning
- 并在 prompt 指导里要求完成任务时优先补成果

第二阶段再考虑收紧为必填。

## 6.2 `update_task` 工具升级

允许 patch 更新：

- `outcome_summary`
- `outcome_items`
- `resume_hint`
- `blocker_reason`
- `blocker_needs`
- `blocker_kind`
- `completed_at`
- `last_outcome_at`

同时支持两种更新模式：

- replace：整体覆盖
- append：向 `outcome_items` 追加

为避免协议过重，第一阶段可以这样做：

- `outcome_summary`：replace
- `resume_hint`：replace
- `outcome_items`：replace
- `blocker_reason`：replace
- `blocker_needs`：replace
- `blocker_kind`：replace

第二阶段再补 `append_outcome_items`

## 6.3 `blocked` 状态的协议约束

建议增加服务端校验规则：

- 当 `status=blocked` 时，如果以下字段都为空，则返回 warning，后续可升级为拒绝：
  - `outcome_summary`
  - `blocker_reason`
  - `blocker_needs`

推荐最小要求：

- 至少要有一句 `outcome_summary`
- 至少要有一句 `blocker_reason`

推荐更完整要求：

- `outcome_summary`: 这次已经做了什么
- `blocker_reason`: 为什么还卡住
- `blocker_needs`: 缺什么才能继续

## 6.4 HTTP API 升级

需要同步升级：

- `PATCH /api/task-manager/tasks/:task_id`
- `POST /api/task-manager/tasks/:task_id/complete`
- 前端 `TaskManagerUpdatePayload`
- 前端 `TaskManagerTaskResponse`

## 6.5 工具描述文案升级

`task_manager` 的系统描述建议强化：

- 创建任务时写清楚目标与约束
- 更新任务时可补充中间发现
- 完成任务时必须沉淀成果摘要和关键信息，避免后续重复探索
- 阻塞任务时必须写明：已经做了什么、为什么阻塞、缺什么才能继续

## 7. 任务看板 prompt 整改

这是本次整改最关键的一部分。

### 7.1 当前问题

当前已完成任务只展示：

- title
- details
- priority
- status

模型知道“做完了”，但不知道“做出了什么”。

同样，当前如果任务被改成 `blocked`，系统也只知道“它阻塞了”，但不知道：

- 这次已经做了什么
- 为什么阻塞
- 缺什么条件才能继续

### 7.2 建议的看板结构

建议 `format_task_board_prompt(...)` 输出改成四段：

1. 当前执行任务
2. 当前阻塞任务与阻塞信息
3. 最近完成任务与成果
4. 复用提示

建议格式示例：

```text
[Task Board]
当前任务看板由系统维护……

当前执行任务：
- [doing] 梳理 task_manager 成果沉淀改造方案 (high) id=...
  details: 检查任务工具、task board prompt、runtime context、Workbar UI

当前阻塞任务与阻塞信息：
- [blocked] 接入远端文件结果总行数 (medium) id=...
  outcome: 已确认本地 code_maintainer 有 total_lines，但 remote read_file 没有返回该字段。
  blocker: 当前卡在远端 read_file 返回协议未携带 total_lines，需要先确定是否允许在远端读取时额外统计行数。
  needs: 确认协议是否接受新增字段；若接受，再改远端 read_file handler。

最近完成任务与成果：
- [done] 核对文件读取工具是否返回总行数 (medium) id=...
  outcome: 本地 code_maintainer 读文件会返回总行数；list_dir 不返回；远程 read_file/list_directory 也不返回总行数。
  hint: 后续如果要减少重复确认，可直接按这个结论继续，不必重新排查 read_file/list_dir 主链路。

复用提示：
- 优先复用已完成任务成果；不要因为任务是 done 就重新探索同一问题。
```

### 7.3 `blocked` 任务必须优先于 done 历史展示

建议优先级：

1. `doing`
2. `todo`
3. `blocked`
4. `done`

说明：

- `blocked` 不是历史噪音，而是当前推进的重要约束
- 如果当前轮存在阻塞任务，模型应优先看到阻塞原因和解阻条件，而不是只看到 done 历史

### 7.4 已完成任务只保留最近 N 条成果

建议：

- 当前执行任务：保持 1 条
- 当前阻塞任务：保持 1-3 条
- 已完成任务成果：保留最近 3-5 条

原因：

- system prompt 预算有限
- 太多 done 历史会冲掉当前任务
- 目标是“防重复探索”，不是“塞满历史档案”

### 7.5 无成果的 done / blocked 任务要显式标记

如果旧任务没有成果字段，建议在 prompt 中输出：

- `outcome: (未沉淀成果)`

这样模型和用户都能明确看出旧数据质量不足。

如果 blocked 任务没有阻塞信息，建议额外输出：

- `blocker: (未说明阻塞原因)`
- `needs: (未说明解阻条件)`

## 8. Workbar / 前端展示整改

## 8.1 TaskCard 增加成果摘要区

当前卡片里建议新增：

- `outcome_summary` 的单行/双行预览
- 当 `status=blocked` 时展示 `blocker_reason` 的单行预览

展示优先级建议：

1. title
2. status badge
3. details
4. outcome summary
5. priority / turn / due_at

### 8.2 历史任务抽屉展示结构化成果

历史任务抽屉中的非 compact 卡片建议支持：

- `成果摘要`
- `关键条目`
- `接手提示`
- `阻塞原因`
- `解阻条件`

如果 `outcome_items.refs` 存在，可以展示为可复制的引用字符串。

### 8.3 TaskManager 工具结果卡补成果

`TaskManagerToolDetails` 需要在：

- `update_task`
- `complete_task`
- `list_tasks`

展示任务成果字段，不然工具调用结果仍然像“只有状态变化”。

### 8.4 完成任务的交互不能只是一键 done

当前 Workbar 的“完成”按钮直接调用完成接口，这会继续制造无成果 done 任务。

建议改成：

- 点击“完成”后弹一个轻量表单
- 必填或推荐填写 `成果摘要`
- 可选填写 `接手提示`

如果不想一开始上复杂 modal，第一阶段可以用 `window.prompt` 过渡，但不建议长期保留。

### 8.5 阻塞任务的交互不能只改状态

当前编辑任务如果把状态手动改成 `blocked`，没有任何额外输入约束，这会制造“只写 blocked、没写原因”的空阻塞任务。

建议：

- 当用户把任务状态改成 `blocked` 时，弹出轻量表单或至少补充 prompt
- 至少录入：
  - `这次已经做了什么`
  - `为什么阻塞`
  - `缺什么才能继续`

## 9. 与全局上下文 / memory summary 的关系

## 9.1 第一阶段不要把任务成果直接塞进 memory summary 生成逻辑

原因：

- 当前 memory summary 是独立链路
- 直接耦合会扩大改动面
- 本次最核心目标是“任务完成后，下个任务不重复探索”

而这个目标仅靠 task board prompt + runtime snapshot + Workbar UI 就能先达成。

## 9.2 第二阶段可以考虑“高价值任务成果升格为长期总结素材”

后续可增加一个规则：

- 当任务被标记 done 且 `outcome_items` 中存在 `importance=high` 的 finding/decision/handoff
- 可把它们作为 session summary 的额外输入素材

但建议作为第二阶段，而不是本次整改必须项。

## 10. 存储与迁移方案

## 10.1 SQLite

`task_manager_tasks` 表新增列：

- `outcome_summary TEXT NOT NULL DEFAULT ''`
- `outcome_items_json TEXT NOT NULL DEFAULT '[]'`
- `resume_hint TEXT NOT NULL DEFAULT ''`
- `blocker_reason TEXT NOT NULL DEFAULT ''`
- `blocker_needs_json TEXT NOT NULL DEFAULT '[]'`
- `blocker_kind TEXT NOT NULL DEFAULT ''`
- `completed_at TEXT`
- `last_outcome_at TEXT`

对应位置：

- [chat_app_server_rs/src/db/sqlite.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/db/sqlite.rs)

需要补：

- 新建表 SQL
- 旧库 `ALTER TABLE` 迁移

## 10.2 MongoDB

Mongo 不需要强 schema 迁移，但需要：

- mapper 支持新字段
- 旧数据缺字段时给默认值

如有需要可补索引：

- `conversation_id + updated_at`

目前大概率不用单独为成果字段建索引。

## 10.3 兼容策略

旧数据兼容规则建议：

- `outcome_summary` 缺失时视为 `""`
- `outcome_items` 缺失时视为 `[]`
- `resume_hint` 缺失时视为 `""`
- `blocker_reason` 缺失时视为 `""`
- `blocker_needs` 缺失时视为 `[]`
- `blocker_kind` 缺失时视为 `""`

## 11. 推荐落地顺序

建议分三期。

### 第一期：先打通“有成果可存、可显示、可进 prompt”

范围：

- 后端数据结构加字段
- 存储层支持读写
- `update_task` / `complete_task` 协议支持成果字段
- `status=blocked` 时支持阻塞原因 / 解阻条件
- `task_board_prompt` 展示完成任务成果摘要
- `task_board_prompt` 展示 blocked 任务原因与 needs
- Workbar / 历史任务 / task tool card 展示成果摘要

目标：

- 完成任务后，下一轮模型就能在 task board 里看到“完成了什么”
- 阻塞任务后，下一轮模型就能在 task board 里看到“为什么卡住、已经做了什么、缺什么条件”
- 用户也能在任务 UI 里看到成果

### 第二期：补交互约束，减少“空成果 done”

范围：

- 完成任务弹出轻量成果录入
- 阻塞任务弹出阻塞信息录入
- `complete_task` 缺少成果时 warning
- `status=blocked` 缺少阻塞信息时 warning
- 提示词里强化“完成必须带成果”
- 提示词里强化“阻塞必须带原因和解阻条件”

目标：

- 降低空成果任务比例

### 第三期：补长期沉淀与智能提炼

范围：

- `outcome_items` 自动摘要
- 高价值成果向 memory summary 升格
- 支持 artifact refs、文件 refs、命令 refs 的更好展示

目标：

- 从“任务不重复探索”升级到“跨轮长期复用”

## 12. 推荐最小实现集

如果只做一轮最有性价比的整改，我建议最小实现集是：

1. 给任务新增 `outcome_summary`、`resume_hint`、`blocker_reason`、`blocker_needs`
2. `complete_task` / `update_task` 支持写这些字段
3. `task_board_prompt` 在 done 任务下展示 `outcome` 和 `hint`
4. `task_board_prompt` 在 blocked 任务下展示 `outcome`、`blocker`、`needs`
5. Workbar / 历史任务卡片展示 `outcome_summary`，blocked 时额外展示 `blocker_reason`
6. 完成任务和阻塞任务都增加信息录入

这几项做完，已经能显著降低重复探索和重复排障。

## 13. 不建议的方案

### 13.1 不建议把成果全塞回 `details`

问题：

- 语义混乱
- 前后状态难区分
- 难做结构化提炼

### 13.2 不建议只改前端展示，不改数据模型

问题：

- UI 可以显示，但模型上下文拿不到
- 不能进入 task board prompt
- 无法真正减少重复探索

### 13.3 不建议只做 session summary，不改 task board

问题：

- session summary 不是每次任务变更都即时刷新
- 任务推进的最近信息，最应该先进入 task board

## 14. 验收标准

整改完成后，应满足以下验收项：

### 14.1 工具层

- `complete_task` 可以携带成果摘要
- `update_task` 可以补充成果摘要 / 接手提示
- `update_task` 在 `status=blocked` 时可以携带阻塞原因 / 解阻条件
- 工具结果卡能显示这些字段

### 14.2 数据层

- SQLite / Mongo 都能持久化成果字段
- 旧数据兼容不报错

### 14.3 上下文层

- 任务完成后触发 task board refresh
- 新 task board prompt 中能看到刚完成任务的成果摘要
- 任务阻塞后触发 task board refresh
- 新 task board prompt 中能看到阻塞原因、已做动作摘要、解阻条件
- turn runtime snapshot 的 `task_runtime_board` 中也能看到成果摘要

### 14.4 UI 层

- 当前任务卡片能展示成果摘要
- 历史任务抽屉能看见成果内容
- 完成任务时不会默默只改状态而不留成果
- 标记 blocked 时不会只改状态而不写阻塞原因

### 14.5 行为层

- 在同一会话里，后续任务可直接复用前一任务的重要信息
- 明显减少“done 了但又重新排查同一问题”的情况
- 明显减少“blocked 了但后续又从头排查为什么 blocked”的情况

## 15. 总结

这次问题的根本不是“任务没有状态”，而是“任务完成后没有知识沉淀，任务阻塞后也没有阻塞沉淀”。

最值得做的不是单纯增强 UI，而是把任务从：

- 状态管理对象

升级成：

- 带成果沉淀的执行单元

并且这里的“成果”要覆盖两类状态：

- `done`：完成了什么
- `blocked`：为什么卡住、已经做了什么、缺什么才能继续

最合适的主挂载点已经存在，就是：

- `task_manager` 数据模型
- `task_board_prompt`
- `task_runtime_board` snapshot
- Workbar / 历史任务 UI

因此推荐优先做“任务成果字段 + 阻塞字段 + task board 成果/阻塞展示 + 完成/阻塞录入信息”这一组改造。这样既能直接改善用户体验，也能马上减少模型后续重复探索和重复排障。
