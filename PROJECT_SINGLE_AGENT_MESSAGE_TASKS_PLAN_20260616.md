# Chatos 项目唯一联系人与用户消息列表调整方案 2026-06-16

## 我对需求的理解

这次变更是 Chatos 项目页的业务形态调整，不是任务系统改造。

新的产品口径是：

- 项目仍然需要联系人入口。
- 每个项目有且只有一个联系人。
- 这个联系人用于项目 agent 聊天和项目运行上下文。
- 当项目里有任务正在运行时，不允许更换或解绑这个联系人。
- 原“团队成员”区域不再表达成员列表，要改成“用户消息列表”。
- 用户消息列表的数据直接复用 Memory Engine 已有 compact turns 用户轮次表/接口。
- 如果某条用户消息有关联的运行中任务，在这条用户消息上展示特殊标记。
- 原来选择联系人后右侧出现的聊天消息和输入界面必须保留。
- “任务”按钮只负责打开现有任务流程图，不意味着要改任务系统。

一句话：项目页从“多成员协作”变成“唯一联系人 + 用户消息列表 + 原聊天工作区”。

## 明确不做

以下内容不在本次范围内：

- 不修改 `task_runner_service`。
- 不新增 Task Runner 接口。
- 不修改任务系统的查询、存储、调度、运行状态计算。
- 不把 Chatos 的 project/contact/session 业务概念塞进任务系统。
- 不删除现有聊天历史。
- 不用纯任务页替换原聊天工作区。
- 不把用户消息列表做成任务系统列表。

## 页面结构

### 项目设置里的联系人入口

项目设置里保留一个明确入口，让用户给项目添加联系人。

建议 UI 是“项目联系人”卡片：

- 未绑定：显示“添加联系人”按钮。
- 已绑定：显示当前联系人信息。
- 已绑定：提供“更换联系人”和“解绑联系人”。
- 有任务正在运行：更换和解绑按钮禁用，并提示“当前项目有运行中的任务，暂不能更换联系人”。

这里不要再叫“团队成员”，因为项目不是多人协作模型了。

### 原团队成员 tab

保留内部 tab key `team`，避免影响已有 localStorage 和历史状态；UI 文案改成“用户消息”或“消息”。

这个 tab 保持原来的左右布局：

- 左侧：用户消息列表。
- 右侧：原来的联系人聊天工作区。

右侧必须继续复用当前的聊天消息和输入界面，也就是之前选择联系人后看到的那块聊天区域。这个区域不能被任务列表替换。

## 用户消息列表

左侧列表展示当前项目联系人会话里的用户消息。

数据来源：

- 直接使用 Memory Engine 已有 `compact-turns` 用户轮次数据。
- Chatos Server 暴露语义化接口，例如 `/api/conversations/:id/user-message-turns`。
- 接口返回每个 turn 的 `user_message` 和可选 `final_assistant_message`。
- 不从 Task Runner 拉项目级任务列表。
- 不新增任务系统聚合接口。

加载策略：

- 默认加载最新 10 条用户消息。
- 这里的 10 条按用户 turn 计算，不按混合消息流计算。
- 用户可以点击“加载更多”继续向前加载更早的用户消息。
- 也可以实现成分页模式，例如每页 10 条。
- 第一版推荐“加载更多”，交互更轻，也更贴近聊天历史浏览方式。
- 后端用 compact turn 的 `next_before` cursor 继续读取更早用户消息。

列表项展示：

- 用户消息摘要。
- 消息发送时间。
- 如果消息有关联运行中任务，显示特殊标记，例如“运行中”徽标、强调边框或状态点。
- 如果消息有关联任务，显示“任务”按钮。
- 点击“任务”按钮打开现有任务流程图。

列表不是只展示运行中的任务消息，而是用户消息列表；运行中任务只是用户消息上的一种状态。

## 运行中任务标记

运行中标记应该来自 Chatos 自己保存的消息历史数据，也就是 compact turn 的 `user_message.metadata` 或 `final_assistant_message.metadata`。

优先使用历史消息里已经存在的消息 metadata，例如：

- `task_runner_async.running_task_ids`
- `task_runner_async.overall_status`
- `task_runner_async.status`
- 当前历史消息 compact 逻辑中能归并到用户消息上的过程状态

具体字段以现有代码为准，当前优先读取 compact turn 里用户消息的 metadata。

如果现有历史消息数据已经能判断某条用户消息是否有运行中任务，就只做前端读取和展示。

如果字段不够，只允许在 Chatos 消息入库、历史消息查询或 compact history 映射层补充标记；仍然不改 Task Runner。

## 联系人锁定规则

当项目存在运行中任务时，不允许变更联系人。

这个规则需要前后端都做：

- 前端：项目联系人卡片禁用“更换联系人”和“解绑联系人”。
- 后端：`POST /api/projects/:id/contacts` 和 `DELETE /api/projects/:id/contacts/:contact_id` 在保存前检查项目是否有运行中任务标记。
- 如果存在运行中任务，后端返回 `409 Conflict`。
- 检查依据来自 Chatos 历史消息表/消息 metadata，不查询或修改任务系统。

这样可以避免用户绕过前端直接调用 API 造成项目联系人和运行中任务上下文错位。

## 后端联系人语义

现有 `/api/projects/:id/contacts` 路由可以先保留，避免大范围改调用。

但语义要收口成“项目唯一联系人”：

- `GET /contacts` 可以继续返回数组，兼容旧前端，但业务上只认最新 active 联系人。
- `POST /contacts` 表示设置项目联系人。
- 设置新联系人前，如果没有运行中任务，归档同项目其他 active 联系人。
- `DELETE /contacts/:contact_id` 表示解绑项目联系人。
- 如果有运行中任务，`POST` 和 `DELETE` 都返回 `409`。

后续可以补更清晰的单对象接口：

- `GET /api/projects/:id/contact`
- `PUT /api/projects/:id/contact`
- `DELETE /api/projects/:id/contact`

但第一版可以先不改路由名。

## 前端改造范围

计划改动 Chatos 前端：

- `chat_app/src/components/ProjectExplorer.tsx`
- `chat_app/src/components/projectExplorer/WorkspaceTabs.tsx`
- `chat_app/src/components/projectExplorer/ProjectRunSettingsPanel.tsx`
- `chat_app/src/components/projectExplorer/TeamMembersPane.tsx`
- `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersPaneModel.ts`
- `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersPaneSessionResources.ts`
- `chat_app/src/i18n/messages.ts`

建议新增：

- `chat_app/src/components/projectExplorer/ProjectContactSettingsCard.tsx`
- `chat_app/src/components/projectExplorer/userMessages/ProjectUserMessagesSidebar.tsx`
- `chat_app/src/components/projectExplorer/userMessages/ProjectUserMessageItem.tsx`
- `chat_app/src/components/projectExplorer/userMessages/useProjectUserMessages.ts`

命名建议：

- UI 文案使用“联系人”和“用户消息”。
- 代码新增部分尽量不用 `teamMembers` 命名。
- 旧 `TeamMembersPane` 可以先保留文件名，但内部职责改为“用户消息侧栏 + 聊天工作区”。

## 后端改造范围

第一版只改 Chatos 后端：

- `chat_app_server_rs/src/api/projects/contact_handlers.rs`
- 必要时调整 Chatos 历史消息查询或消息 metadata 映射相关代码。
- Memory Engine 已有 compact turns 接口时，优先复用，不新增 Memory Engine 接口。

后端要做两件事：

- 项目联系人唯一化。
- 联系人变更前检查 Chatos 历史消息里是否存在运行中任务标记。

不改任务系统目录。

## 数据流

### 联系人配置

1. 用户进入项目设置。
2. 前端读取当前项目 contacts。
3. 只把当前 active 联系人展示为项目联系人。
4. 用户添加或更换联系人。
5. 前端请求 Chatos Server。
6. Chatos Server 检查是否存在运行中任务标记。
7. 如果没有运行中任务，保存唯一联系人。
8. 如果有运行中任务，返回 `409`，前端提示暂不能变更。

### 用户消息列表

1. 用户进入原 `team` tab。
2. 前端加载当前项目联系人对应的聊天会话。
3. 右侧照常渲染聊天历史和输入框。
4. 左侧通过 Chatos `user-message-turns` 接口读取最新 10 个用户 turn。
5. 前端从消息 metadata 判断是否有运行中任务。
6. 有运行中任务的消息展示特殊标记。
7. 有任务关系的消息展示“任务”按钮。
8. 用户点击“加载更多”时继续读取更早的用户消息。
9. 点击“任务”按钮，复用现有任务流程图打开逻辑。

## 实施阶段

### Phase 0：清理错误方向

- 清掉把整个聊天工作区替换成任务页的实现。
- 清掉任何任务系统改动。
- 确认 `task_runner_service` 没有 diff。
- 确认右侧聊天历史和输入框仍然存在。

### Phase 1：项目唯一联系人

- 设置页新增项目联系人卡片。
- 支持添加、查看、更换、解绑联系人。
- 后端保存时保证一个项目只有一个 active 联系人。
- 有运行中任务时禁止更换或解绑。

### Phase 2：用户消息列表替换成员列表

- tab 文案从“团队成员”改为“用户消息”或“消息”。
- `TeamMembersPane` 继续保留右侧聊天工作区。
- 左侧成员列表替换为用户消息列表。
- 用户消息来自 Memory Engine compact turns，默认加载最新 10 个用户 turn。
- 支持“加载更多”或分页继续查看更早用户消息。
- 运行中任务消息显示特殊标记。
- “任务”按钮复用现有任务流程图。

### Phase 3：状态和体验

- 刷新历史消息时同步刷新运行中标记。
- 联系人锁定状态随运行中标记变化。
- 运行中任务完成后，标记消失或变成最近完成状态。
- 空状态区分“未绑定联系人”和“暂无用户消息”。

### Phase 4：命名清理

稳定后再逐步清理旧命名：

- `TeamMembersPane` 可重命名为项目联系人聊天工作区。
- `team` tab key 可迁移为 `messages`。
- `/contacts` 可迁移成单对象 `/contact` 接口。

这些不是第一版必须项。

## 验收标准

- 项目设置里有添加联系人入口。
- 每个项目最终只有一个 active 联系人。
- 项目存在运行中任务时，不能更换或解绑联系人。
- 原“团队成员”区域不再展示成员列表。
- 原“团队成员”区域左侧变成用户消息列表。
- 用户消息列表从 Memory Engine compact turns 数据加载。
- 用户消息列表默认展示最新 10 个用户 turn，并支持加载更多或分页。
- 有运行中任务的用户消息有明显特殊标记。
- 有任务关系的用户消息可以点击“任务”打开现有流程图。
- 右侧聊天历史和输入框保留，旧聊天记录不丢。
- 不修改任务系统代码。

## 最小闭环

建议先按这个顺序做：

1. 清理上次残留，恢复右侧聊天工作区。
2. 项目设置加“项目联系人”卡片。
3. 后端 contacts 保存逻辑改成唯一联系人。
4. 后端 contacts 变更前用 Chatos 历史消息标记判断是否锁定。
5. 原 `team` tab 改文案。
6. `TeamMembersPane` 左侧换成用户消息列表。
7. 用户消息列表读取 Memory Engine compact turns 数据，默认最新 10 个用户 turn。
8. 增加“加载更多”或分页能力。
9. 对运行中任务消息加特殊标记。
10. “任务”按钮复用现有流程图入口。

这版完成后，项目页会变成“唯一联系人配置 + 用户消息列表 + 原聊天工作区”，同时不会动任务系统。
