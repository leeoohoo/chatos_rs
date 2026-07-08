# ChatOS 新用户默认智能体实施方案

## 目标

当用户通过 ChatOS 注册新账号后，系统自动完成以下初始化：

1. 创建一个默认 ChatOS 智能体，名字固定为 `叽咕狸`
2. 自动为该智能体开通可用于 Task Runner 的 `agent account`
3. 自动创建并绑定一个 `contact`
4. 可选：自动创建一个开箱即用的初始会话

目标不是做完整 onboarding，而是让第一次进入系统的用户尽快具备“能聊、能挂 task、少手动配置”的基础状态。

## 现状结论

### 1. 注册链路目前只创建人类用户

- `user_service/backend/src/api/auth.rs`
  - `register()` 现在只写入 `UserRecord`
  - 不会创建 ChatOS agent
  - 不会创建 contact
  - 不会创建 starter session

### 2. ChatOS agent 和 task 账号不是一个东西

- ChatOS 智能体数据在 `chat_app_server_rs`
- Task Runner 使用的执行身份在 `user_service.agent_accounts`
- 当前架构里，所谓“task 的账号”应该理解为 `user_service` 里的 `agent_account`
- 不建议直接去写 `task_runner_service` 自己的 `users` 表；ChatOS 当前走的是 `user_service -> token exchange -> task_runner` 这条链路

### 3. 现有代码已经支持“创建 agent 时顺手开通 task 账号”

- `chatos/backend/src/api/agents.rs`
  - 普通创建 agent 时已经强制带 `auto_provision_task_runner_account = Some(true)`
- `chatos/backend/src/services/chatos_agents.rs`
  - `provision_task_runner_agent_account()` 会调用 `user_service /api/agent-accounts`

这说明“自动建 task 账号”不需要重新发明，只要复用现有 agent 创建逻辑。

### 4. 只有 agent 还不够，Task Runner 真正运行还依赖 contact

- `chatos/backend/src/services/chatos_memory_mappings/contacts.rs`
  - `create_memory_contact()` 创建 contact 后，会通过 `auto_bind_contact_task_runner()` 自动把 contact 绑定到 agent 的 task 账号
- `chatos/backend/src/modules/conversation_runtime/runtime_context.rs`
  - 运行时加载 Task Runner 配置时，最终读的是 contact 绑定信息

结论：如果只创建 `叽咕狸` 和 `agent_account`，但不创建 `contact`，第一次使用时 task 能力仍然起不来。

## 推荐落点

推荐把新用户初始化逻辑放在：

- `chatos/backend/src/api/auth.rs`
- 具体是 `register_via_user_service()` 成功之后

### 原因

1. 用户说的是“chatos 上注册”，入口就在这里
2. 这里能拿到刚注册成功后的用户 token 和 user id
3. 可以直接复用 `chatos_agents::create_agent()` 现有自动开通 task 账号的逻辑
4. 避免让 `user_service` 反向依赖 `chat_app_server_rs` 的 agent/contact/session 数据

## 推荐方案

新增一个专门的引导服务，例如：

- `chatos/backend/src/services/new_user_bootstrap.rs`

提供一个幂等方法，例如：

- `bootstrap_new_user_defaults(access_token, user_id, username)`

其职责如下：

1. 检查该用户是否已经完成过初始化
2. 若没有，则创建默认 agent `叽咕狸`
3. 确保该 agent 已拿到 `task_runner_agent_account_id`
4. 为该 agent 创建默认 contact，并自动绑定 task 配置
5. 可选：创建一个 starter session

## 建议的初始化顺序

### 第一步：创建默认 agent

调用现有 `chatos_agents::create_agent()`，参数建议：

- `name`: `叽咕狸`
- `enabled`: `true`
- `auto_provision_task_runner_account`: `true`
- `role_definition`: 一段简洁、稳定、面向新手的默认角色说明

建议默认角色定义保持实用，不要过度拟人化，例如：

> 你叫叽咕狸，是用户进入 ChatOS 后默认可用的智能体。优先帮助用户快速开始对话、整理需求、拆解任务，并在需要时引导使用项目、工具和 Task Runner 能力。回答保持直接、清晰、可执行。

### 第二步：创建默认 contact

调用现有 contact 创建逻辑，而不是手写数据库：

- 入口建议复用 `chatos_memory_mappings::create_memory_contact()`

这样可以直接复用已有的自动绑定逻辑：

- 新建 contact
- 自动填入 `task_runner_base_url`
- 自动写入 `task_runner_agent_account_id`
- 自动启用 `task_runner_enabled`

### 第三步：可选创建 starter session

如果目标是“注册完立刻可用”，建议同时创建一个初始会话。

可复用：

- `modules/conversation_runtime/sessions::create_session()`

会话 metadata 至少应带上：

- `contact_id`
- `contact_agent_id`
- `selected_agent_id`

这样前端第一次进入时就能直接落到一个已绑定 `叽咕狸` 的会话上下文。

## 幂等与失败处理

这是这次实现里最需要认真处理的部分。

### 幂等建议

第一版建议按下面方式做：

1. 若用户没有任何 agent，则执行完整 bootstrap
2. 若已经有 agent，但缺少默认 contact，则补 contact
3. 若 contact 已有，但缺 starter session，则补 session

这样即使注册后半路失败，也可以安全重试。

### 不建议让注册因为 bootstrap 失败而整体失败

原因很直接：

1. `user_service` 用户可能已经创建成功
2. 如果此时把注册接口整体返回失败，前端重试会遇到“username already exists”
3. 用户感知会非常差

推荐策略：

1. 注册成功后同步执行 bootstrap
2. bootstrap 失败时记录结构化日志
3. 注册接口仍返回成功
4. 再提供一个“静默补偿”的入口

补偿方式推荐二选一：

1. 注册成功后，前端首次进入时静默调用一次 `bootstrap default workspace`
2. 或者后端在 `GET /api/auth/me` / `GET /api/agents` 的首次空数据场景触发一次幂等修复

第一种更清晰，也更容易测试。

## 是否需要改 user_service

不建议把“创建 ChatOS 默认 agent/contact/session”放进 `user_service`。

### `user_service` 只建议继续负责：

1. 人类用户注册
2. `agent_account` 创建
3. task token exchange

### `chat_app_server_rs` 负责：

1. ChatOS agent
2. contact
3. session
4. 首次进入体验

这个边界更符合当前代码组织。

## 模型配置的现实约束

即使默认 agent、contact、task 账号都建好了，真正发起模型调用仍然依赖模型配置。

当前行为是：

- 若用户只有一个启用中的模型配置，可自动兜底使用
- 若用户没有模型配置，仍然无法真正开始对话
- 若用户有多个启用模型且未选中，运行时会要求显式选择

所以本方案解决的是“默认智能体和 task 通路”问题，不自动解决“模型供应商配置”问题。这个约束应在方案评审时明确。

## 建议变更文件

预计会涉及：

- `chatos/backend/src/api/auth.rs`
- `chatos/backend/src/services/mod.rs`
- `chatos/backend/src/services/new_user_bootstrap.rs`（新增）
- 可能少量复用：
  - `chatos/backend/src/services/chatos_agents.rs`
  - `chatos/backend/src/services/chatos_memory_mappings/contacts.rs`
  - `chatos/backend/src/modules/conversation_runtime/sessions.rs`

## 验收标准

### 自动化验证

至少覆盖以下场景：

1. 新用户注册后，`/api/agents` 能看到 `叽咕狸`
2. 新用户注册后，`/api/auth/agent-accounts` 能看到对应 `agent_account`
3. 新用户注册后，`/api/contacts` 能看到已绑定 task 配置的 contact
4. 若启用 starter session，则 `/api/conversations` 能看到初始会话
5. 重复执行 bootstrap 不会产生重复 agent/contact/session
6. bootstrap 中途失败后再次执行可以修复状态

### 手工验证

1. 用全新账号注册
2. 首次进入后无需先手动建 agent
3. 可直接看到 `叽咕狸`
4. 进入该默认 contact / session 后，Task Runner 配置已经就绪

## 实施建议

建议分两步做：

### Phase 1

先完成：

1. 注册后自动创建 `叽咕狸`
2. 自动创建 `agent_account`
3. 自动创建并绑定 contact

### Phase 2

再补体验增强：

1. 自动创建 starter session
2. 前端首次进入时自动跳到默认会话
3. 增加 bootstrap 失败后的静默补偿

这样风险更可控，也更容易定位问题。
