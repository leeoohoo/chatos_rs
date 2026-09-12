# ChatOS 本地 Agent 最终架构与实施规范

## 1. 文档地位

本文定义 ChatOS Agent 系统的最终实现形态，是主聊天 Agent、Task Runner、本地工具执行、模型上下文和 Memory Engine 接入的统一实施依据。

本文只描述最终架构，不设计兼容层、灰度双轨、旧协议适配或临时转发路径。实施完成后，服务端不再运行主聊天 Agent 或 Task Runner Agent，不再保留两套 Agent Loop。

与本文冲突的旧 Task Runner、Cloud Agent、客户端独立循环文档只保留为历史记录，不得继续作为目标架构实现依据。

## 2. 最终目标

ChatOS 客户端拥有完整的 Agent 执行权：

- 主聊天和后台任务都由客户端本地 Agent Host 执行。
- Agent Loop 使用持久化、事件化的单步状态机，不使用进程内长时间 `while` 循环承载完整任务。
- 主聊天和 Task Runner 只实现各自的业务 Profile，不分别实现模型循环、工具循环、重试和上下文管理。
- 模型配置继续由服务端统一保存；客户端不持久化模型 API Key。
- 客户端通过无状态 Model Gateway 完成单次模型请求，Agent 状态和循环不进入网关。
- 所有用户消息、有效助手消息、工具调用和工具结果都形成稳定记录，并同步到 Memory Engine。
- 支持厂商原生上下文压缩的模型使用厂商原生协议；其他模型使用 Memory Engine 的总结与上下文重组。
- 插件和 MCP 在用户设备本地执行，服务端只管理插件目录、不可变发布物、授权和策略。
- macOS 与 Windows 使用同一份 Agent 执行内核和协议，不在 Swift 与 C# 中各写一套业务循环。
- 所有客户端结构化业务数据统一通过 Client Storage Provider 读写，默认使用 SQLite；用户可在高级设置中选择自己的 PostgreSQL 数据库。
- SQLite 与 PostgreSQL 必须提供相同的数据模型、事务语义和功能，不允许任何业务模块只支持其中一种数据库。

本文将用户所说的 “PS” 数据库明确为 PostgreSQL，后文统一使用 PostgreSQL。

## 3. 强制架构不变量

以下规则不得通过配置、降级或特殊业务分支绕过：

1. 一个模型请求只对应一个本地 Agent Step；Step 完成后必须先提交持久化状态，才能安排下一步。
2. Main Chat 和 Task Runner 共用同一 Agent Runtime、模型适配、上下文策略、工具调度、记录同步和恢复机制。
3. 当前选中的 Client Storage Provider 是 Agent Run、事件、工具执行回执和待同步记录的权威执行状态。
4. Memory Engine 是服务端消息归档、会话总结和长期记忆的权威来源，但不保存或驱动本地 Agent 状态机。
5. 服务端模型配置是模型参数、能力和凭据的权威来源；本地只保存不含密钥的 Run 快照。
6. Model Gateway 每次只执行一个模型请求，不保存会话、不推进工具循环、不决定任务是否完成。
7. 每条新语义记录只保存一次；重新发送给模型的旧历史、Memory Engine 摘要和完整请求体不得再次保存成新消息。
8. 模型请求事件只保存有界诊断信息，禁止持久化完整 input、完整 response body、大型工具结果、图片 Base64 或密钥。
9. 工具调用必须先持久化，工具结果必须以原始 call ID 落库，之后才允许发起下一次模型请求。
10. 有副作用工具的未知执行结果不得自动重放，必须进入 `needs_review`。
11. 上下文策略由冻结的模型能力决定，不根据接口名称、模型名称猜测，不向不支持的厂商发送仿造的 compaction 参数。
12. 主聊天与 Task Runner 的最终完成必须由各自 Profile 的业务校验确认，不能以达到迭代上限、模型停止输出或队列为空代替成功。
13. 客户端业务模块不得直接依赖 SQLite 或 PostgreSQL Driver，只能依赖统一 Repository/Transaction 接口。
14. 一个客户端进程在同一时刻只能绑定一个权威数据库；禁止 SQLite/PostgreSQL 双写、按表拆分和连接失败后自动切回 SQLite。
15. PostgreSQL 地址、账号、密码和证书引用只允许保存在系统安全存储中，不得进入 SQLite、PostgreSQL 业务表、日志、事件或 Memory Engine。

## 4. 最终部署拓扑

```text
┌──────────────────────────── ChatOS Client ────────────────────────────┐
│                                                                       │
│  Native UI                                                            │
│      │ typed local IPC + replay cursor                                │
│      ▼                                                                │
│  Local Agent Host                                                     │
│  ├─ Durable Agent Runtime                                             │
│  ├─ MainChatAgentProfile                                              │
│  ├─ TaskRunnerAgentProfile                                            │
│  ├─ Provider Context Strategies                                       │
│  ├─ Local MCP / Plugin Runtime                                        │
│  ├─ Memory Synchronizer                                               │
│  └─ Client Storage Provider                                           │
│     ├─ SQLite（默认）                                                  │
│     └─ PostgreSQL（用户高级设置）                                      │
│                                                                       │
└──────────────┬──────────────────┬────────────────────┬────────────────┘
               │                  │                    │
               ▼                  ▼                    ▼
        Model Configuration   Memory Engine      Plugin Management
        + Stateless Gateway   + Summary Agents   + Artifact Registry
               │                  │                    │
               └──────────────────┴────────────────────┘
                                  │
                          User Service / Auth

Config Center 只服务于服务端运行配置和受管 Agent 绑定，不保存客户端 Run 状态。
```

本方案只改变 Agent 执行边界。与 Agent 无关的媒体、文件分发和其他业务服务不在本文删除范围内。

## 5. 模块职责

| 模块 | 唯一职责 | 明确不负责 |
| --- | --- | --- |
| Local Agent Host | 在用户设备上持久化并推进所有 Agent Run | 用户账户、模型密钥、插件市场 |
| Durable Agent Runtime | claim 一个事件、执行一个 Step、归约结果、原子提交下一状态 | 主聊天或任务的业务判断 |
| Client Storage Provider | 为全部客户端结构化业务数据提供统一 Repository、事务、迁移和备份接口 | 业务流程、模型调用、文件内容 |
| MainChatAgentProfile | 主聊天提示词、可用能力、用户交互和最终消息 | 自己实现 Agent Loop |
| TaskRunnerAgentProfile | Task/Run、项目执行、进度、完成校验和任务结果 | 自己实现 Agent Loop |
| Model Configuration | 模型配置、凭据、能力元数据、配置修订 | Agent 状态和对话历史 |
| Model Gateway | 鉴权后代理一次模型请求并流式返回正式终态 | 会话、工具执行、自动续跑 |
| Memory Engine | 记录、compose、会话总结、Subject Memory、后台 rollup | 本地任务调度和工具执行 |
| Plugin Management | 插件目录、签名发布物、版本、权限策略 | 启动插件进程和执行 MCP |
| Config Center | 服务端基础配置、Memory Summary Agent 绑定和策略 | 用户本地 Agent 设置和 Run 数据 |

## 6. 公共 Local Agent Runtime

### 6.1 唯一运行内核

建立跨平台 Rust 内核：

```text
crates/chatos_local_agent_protocol
crates/chatos_local_agent_runtime
local_agent_host
```

最终代码中删除 Cloud 专用命名和行为。`chatos_cloud_agent_protocol`、`chatos_cloud_agent_runtime` 中与状态归约、claim、幂等和单步执行有关的通用代码进入本地 Runtime；RabbitMQ、MongoDB、Cloud Outbox Driver 和 owner service 路由不得进入最终本地内核。

macOS Swift 与 Windows C# 只通过本地 IPC 使用该内核。现有 Swift `ChatOSAgentRuntime` 不继续作为第二套 Loop；审批、剧情、主聊天和 Task Runner 都应注册为公共 Runtime 的 Profile 或通过类型化工具回调接入。

### 6.2 Profile 接口

```rust
#[async_trait]
pub trait LocalAgentProfile: Send + Sync {
    fn key(&self) -> &'static str;
    async fn prepare_run(&self, context: PrepareRunContext) -> Result<PreparedRun, AgentError>;
    async fn prepare_step(&self, context: PrepareStepContext) -> Result<PreparedStep, AgentError>;
    async fn execute_tool(&self, call: DurableToolCall) -> Result<ToolOutcome, AgentError>;
    async fn evaluate_response(&self, context: ResponseContext) -> Result<ResponseAction, AgentError>;
    async fn verify_completion(&self, context: CompletionContext) -> Result<CompletionDecision, AgentError>;
    async fn finalize_run(&self, context: FinalizeContext) -> Result<FinalOutcome, AgentError>;
}
```

Profile 只能返回业务决策和工具结果。它不能自行调用下一轮模型、递归执行、睡眠重试或维护另一份消息历史。

### 6.3 单步状态机

每个事件只允许以下过程：

```text
load run + event
  → validate ordering/version
  → acquire local claim
  → execute exactly one step
  → reduce outcome
  → transactionally persist state + new events + outbox
  → release claim
```

标准状态：

```text
queued
model_ready
model_running
waiting_tool_result
continuation_ready
retry_scheduled
paused
needs_review
succeeded
failed
cancelled
```

标准事件：

```text
run_started
model_step_requested
model_step_completed
tool_batch_requested
tool_batch_completed
continuation_requested
retry_due
pause_requested
resume_requested
cancel_requested
memory_sync_due
run_terminal
```

标准模型 Step 结果：

```text
ToolCommand
Continue
Retry
AskUser
Final
Failed
Cancelled
```

事件消费者可以长期运行，但 Agent 逻辑本身不得由一个长期 `while true` 调用栈持有。进程退出后，未完成事件必须可以从当前选中的 Storage Provider 恢复。

## 7. 统一客户端数据存储

### 7.1 Storage Provider

建立跨平台公共存储模块：

```text
crates/chatos_client_storage
├─ contracts
├─ repositories
├─ transaction
├─ migrations
├─ sqlite
└─ postgres
```

所有客户端结构化业务数据只能通过该模块访问。业务层只能看到领域 Repository，不能接收数据库连接、拼接 SQL 或判断当前后端。

```rust
pub trait ClientStorage: Send + Sync {
    fn backend(&self) -> StorageBackend;
    async fn transaction<T>(&self, operation: StorageTransaction<T>) -> Result<T, StorageError>;
    fn agents(&self) -> &dyn AgentRepository;
    fn conversations(&self) -> &dyn ConversationRepository;
    fn tasks(&self) -> &dyn TaskRepository;
    fn projects(&self) -> &dyn ProjectRepository;
    fn plugins(&self) -> &dyn PluginStateRepository;
    fn media(&self) -> &dyn MediaStateRepository;
    fn settings(&self) -> &dyn ClientSettingsRepository;
}

pub enum StorageBackend {
    Sqlite,
    Postgres,
}
```

Repository 方法必须表达领域操作和事务边界，不得把 SQLite 方言、PostgreSQL 方言或动态 SQL 暴露给调用者。

### 7.2 必须纳入统一存储的客户端数据

支持范围不是只有本方案新增的 Agent 表。以下现有和新增客户端数据全部必须接入同一个 Storage Provider：

- 会话、turn、本地消息索引、附件引用和未发送草稿。
- Agent Run、事件、Checkpoint、Provider Context、Ask User 和人工复核状态。
- Task、Run、步骤、进度、完成证据及主聊天关联。
- 客户端项目注册、项目设置、运行配置和最近访问状态。
- Notepad、计划、剧情、媒体工作台及生成任务元数据。
- 插件安装状态、固定 Release/Component/Skill 快照、授权和运行记录。
- 本地 MCP 会话元数据、工具执行回执和副作用防重记录。
- Memory Engine 同步游标、Sync Outbox、摘要状态缓存和失败记录。
- 用户可配置的 Agent Runtime 设置及其他需要跨重启保存的业务设置。

以下内容不是结构化业务数据库数据：

- 用户项目文件、插件发布物、图片、音频、视频和大型附件保存在文件系统或对象存储，数据库只保存引用、hash、大小和状态。
- 可重建的缩略图、搜索索引、构建产物和临时文件保存在 Cache 目录，不写 PostgreSQL。
- 数据库选择、连接地址和凭据属于启动引导配置，按 7.3 节保存。
- 窗口位置、临时选中项等纯设备 UI 状态可以使用系统偏好存储，但不得承载业务记录。

### 7.3 高级设置与连接配置

高级设置增加“客户端数据存储”：

```text
存储类型
  ○ SQLite（默认）
  ○ PostgreSQL

PostgreSQL
  地址 / Host
  端口
  数据库名
  用户名
  密码
  TLS 模式
  CA 证书（可选）
  连接超时
  连接池上限
  [测试连接]
  [应用并重启本地 Agent Host]
```

配置规则：

- 首次安装和未配置时固定使用应用容器内的 SQLite。
- PostgreSQL 使用固定 `chatos` schema；不允许用户输入任意 SQL schema 表达式。
- PostgreSQL 15 或更高版本必须启用认证；非 loopback 地址必须使用 TLS。
- 非秘密选择项保存在操作系统应用偏好中。
- 密码、完整 DSN、客户端证书私钥保存在 macOS Keychain 或 Windows Credential Manager/DPAPI 中。
- UI、崩溃报告、诊断导出和日志必须对 DSN、用户名、密码和证书路径脱敏。
- “测试连接”必须校验网络、认证、数据库权限、事务、schema migration 权限和受支持的 PostgreSQL 版本。
- 应用配置前必须确认没有处于 `model_running`、`waiting_tool_result` 或工具副作用执行中的 Run。

### 7.4 数据库选择与切换语义

- 启动时先读取 Bootstrap Storage Profile，再创建唯一 `ClientStorage` 实例。
- 所有 Repository 在 Host 生命周期内绑定同一个实例。
- 选择 PostgreSQL 后，它就是全部客户端结构化业务数据的唯一权威库；SQLite 不作为缓存或备用库继续写入。
- PostgreSQL 连接失败时进入明确的 `storage_unavailable` 状态，UI 提供修改配置和重试，不自动切换 SQLite。
- 切换数据库必须停止本地 Agent Scheduler、关闭旧连接、初始化并校验新 schema，然后重启 Local Agent Host。
- 切换后旧数据库保持原样，不后台双向同步、不自动合并同 ID 记录。
- 数据导入导出是显式数据库操作，必须在所有 Run 停止时执行，并以校验报告结束；不得将导入逻辑实现成长期兼容双写。

### 7.5 等价事务与并发语义

SQLite 和 PostgreSQL 必须通过同一套 Repository Contract 测试。最终行为必须一致：

- Run version 使用 compare-and-swap 更新。
- Event claim、Run 状态变化、新事件和 storage outbox 在同一事务提交。
- `event_id`、`record_id`、`invocation_id` 和 `outbox_id` 使用唯一约束保证幂等。
- SQLite 使用 WAL、foreign keys、busy timeout 和单 Host 写入锁。
- PostgreSQL 使用行锁或 `FOR UPDATE SKIP LOCKED`、事务隔离和有期限的 worker lease。
- 多个客户端连接同一 PostgreSQL 时，以 `owner_user_id + device_id` 隔离设备执行权；同一 Run 在任意时刻只能有一个有效 lease。
- 时间统一保存 UTC，ID 使用相同字符串表示，JSON 使用相同 canonical encoding 和 digest。
- Schema version 在两种后端中一致；每个 migration 必须同时提供 SQLite 与 PostgreSQL 实现并通过同一数据契约测试。
- 不依赖 PostgreSQL JSONB 查询实现业务正确性；必须保证 SQLite 等价查询可实现。

### 7.6 Agent 持久化表

Agent Runtime 至少使用以下逻辑表；SQLite 与 PostgreSQL 使用同一字段语义。

#### `agent_runs`

- `run_id`
- `profile_key`
- `owner_user_id`
- `owner_entity_type`
- `owner_entity_id`
- `project_id`
- `status`
- `phase`
- `version`
- `step_seq`
- `iteration`
- `retry_count`
- `model_config_id`
- `model_config_revision`
- `model_runtime_snapshot_json`
- `context_strategy`
- `prompt_revision`
- `capability_snapshot_ref`
- `pending_batch_id`
- `terminal_outcome_json`
- `deadline_at`
- `created_at`
- `updated_at`

#### `agent_events`

- `event_id`
- `run_id`
- `event_type`
- `expected_version`
- `available_at`
- `status`
- `attempt_count`
- `causation_id`
- `correlation_id`
- `bounded_payload_json`
- `last_error`

同一 `event_id` 只能归约一次。事件失败使用有界指数退避；达到明确上限后将 Run 转为失败或 `needs_review`，不得无限重放。

#### `agent_messages`

- `record_id`
- `run_id`
- `thread_id`
- `turn_id`
- `sequence`
- `role`
- `content`
- `reasoning`
- `structured_payload_json`
- `tool_call_id`
- `response_id`
- `message_mode`
- `message_source`
- `memory_sync_status`
- `created_at`

记录 ID 必须由稳定业务身份生成。重复恢复和重复事件必须得到相同 ID，Memory Engine `batch-sync` 不得产生重复消息。

#### `provider_context_items`

- `item_id`
- `run_id`
- `generation`
- `sequence`
- `provider`
- `item_type`
- `encrypted_payload`
- `created_at`

厂商原生 reasoning、function call、response message 和 compaction item 在此增量保存，每个 item 只保存一次。不得在每个 Step 中复制累计数组。出现新 compaction item 时，在同一事务中提升 generation，并删除该 generation 之前已被替代的厂商上下文。

#### `tool_executions`

- `invocation_id`
- `run_id`
- `batch_id`
- `tool_call_id`
- `tool_name`
- `effect`
- `arguments_digest`
- `status`
- `bounded_result_json`
- `started_at`
- `completed_at`

`idempotent_write`、`write`、`billable` 和 `terminal` 工具执行前必须写入 `started`。其中只有以稳定业务 ID 和唯一约束实现的 `idempotent_write` 可在进程中断后重放；`write`、`billable` 和 `terminal` 没有确定结果时必须进入 `needs_review`。

#### `sync_outbox`

- `outbox_id`
- `destination`
- `record_id`
- `payload_digest`
- `status`
- `attempt_count`
- `available_at`
- `last_error`

它负责向 Memory Engine 同步记录，不与 Agent 事件表混用。

## 8. 两个新增业务 Profile

### 8.1 `MainChatAgentProfile`

负责：

- 解析当前用户、会话、联系人 Agent、项目和附件作用域。
- 构造主聊天系统提示、Skill Catalog、已授权能力和当前用户目标。
- 生成用户可见的流式文本、思考状态、工具状态和 Ask User 请求。
- 将需要真实项目执行的请求创建为本地 Task，并建立当前 turn 与 task/run 的关联。
- 验证最终回复没有伪造工具结果、任务完成或文件修改。
- 保存最终助手消息并完成当前 turn。

主聊天 Profile 不直接获得任意项目写入、终端或 Marketplace 插件工具。需要执行真实操作时，它只能创建本地 Task；TaskRunner Profile 根据固定能力快照执行。

### 8.2 `TaskRunnerAgentProfile`

负责：

- 创建并管理本地 Task、Run、计划步骤和进度。
- 冻结 `project_id`、客户端权威项目快照、工作目录、模型配置修订、插件发布物和能力策略。
- 解析任务需要的 Skill 与 Plugin，并生成固定工具快照。
- 执行项目读取、写入、终端、浏览器、Computer Use 和插件 MCP。
- 对任务完成条件、交付物、验证命令和副作用回执进行确定性校验。
- 将任务结果关联回发起它的主聊天 turn。

TaskRunner 是一个本地 Profile 和本地域模型，不再代表独立服务。UI 可以继续使用“任务”和“Run”概念，但所有数据和执行状态来自 Local Agent Host。

## 9. 服务端模型配置与无状态 Model Gateway

### 9.1 模型配置必须保留在服务端

服务端模型配置至少包含：

- `model_config_id`
- `revision`
- `provider`
- `model`
- `protocol`
- 加密凭据
- `base_url`
- 最大上下文与输出限制
- reasoning、temperature 等允许参数
- 流式协议能力
- token count 能力
- 原生 compaction 能力
- Memory Engine Summary Agent 绑定
- 用户、租户和调用权限

Config Center 保存服务端默认绑定和运行政策；模型配置服务保存具体模型及凭据。Memory Engine 的 Summary、Rollup 和 Subject Memory Agent 通过服务端绑定解析模型，不接收客户端提交的 API Key 或任意模型参数。

### 9.2 客户端运行快照

Run 开始时，Local Agent Host 获取不含密钥的 `ModelRuntimeDescriptor`：

```json
{
  "model_config_id": "...",
  "revision": 12,
  "provider": "openai",
  "model": "...",
  "protocol": "responses",
  "context_window_tokens": 400000,
  "maximum_output_tokens": 32000,
  "context_strategy": "provider_native",
  "supports_streaming": true,
  "supports_native_compaction": true
}
```

该描述符冻结进 `agent_runs`。Run 中途服务端配置变化不得改变当前 Run；新 Run 获取新 revision。

### 9.3 Model Gateway 契约

Local Agent Host 使用 `model_config_id + revision` 请求服务端 Model Gateway。Gateway：

1. 校验用户、模型配置和 revision。
2. 读取服务端凭据。
3. 对厂商执行一次请求。
4. 原样保留正式 streaming terminal、usage、response ID、output item 和 incomplete details。
5. 将流返回客户端后结束，不保存 Agent Run 或自动发起下一次模型请求。

Gateway 不允许：

- 根据 response 自动调用工具。
- 保存或补齐会话历史。
- 自动选择另一个模型。
- 在客户端未声明时开启或关闭 compaction。
- 将 `response.created` 或流 EOF 当作成功终态。

平台拥有的模型密钥不得下发客户端；客户端也不得把密钥写入 Run、Checkpoint、日志或插件环境。

## 10. 消息记录与 Memory Engine

### 10.1 必须保存的语义记录

以下记录必须先写入当前 Client Storage 的 `agent_messages`，再通过稳定 ID 同步 Memory Engine：

1. 用户发送的新消息和附件引用。
2. 有效助手回复。
3. 助手发起的完整工具调用及参数。
4. 每个工具调用对应的结果或明确错误。
5. Ask User 问题和用户回答。
6. Task 的最终结果及其与主聊天 turn 的关联。

以下内容不得作为新对话消息重复保存：

- 每次模型请求重新携带的历史消息。
- Memory Engine 返回的 summary blocks 和 recent records。
- 系统提示词在每个 Step 的重复注入。
- 完整模型请求体。
- SSE delta。
- 仅供厂商续传的加密 compaction item。
- 重试但没有形成正式模型输出的请求。

### 10.2 保存顺序

```text
用户消息
  → storage transaction: message + memory outbox + run event
  → 当前模型 Step
  → validated assistant response
  → storage transaction: assistant record + tool batch / final state
  → local tool execution
  → storage transaction: tool records + next model event
```

Memory Synchronizer 持续发送 `batch-sync`。Memory Engine 不可用时：

- 本地记录和 outbox 不得丢失。
- 使用厂商原生上下文的 Run 可以继续，但 UI 必须展示 `memory_sync_pending`。
- 依赖 Memory Engine compose 的 Run 必须暂停在模型请求之前，恢复同步并 compose 后继续。
- Run 的业务终态和 Memory 同步状态分别展示，不允许把“任务成功”伪装成“记忆已同步”。

### 10.3 Memory Engine 后台总结

Memory Engine 对所有已同步消息持续执行自己的 Summary、Rollup 和 Subject Memory Job。它使用服务端受管模型配置，生成可读、跨设备、跨 Run 的长期记忆。

厂商 compaction 与 Memory Engine 总结互不替代：

- 厂商 compaction 只服务于当前厂商上下文续传，保存在本地 Provider Context。
- Memory Engine 总结来自同步后的语义记录，保存在服务端 Memory Engine。
- 厂商 compaction item 不写成 Memory Engine summary。
- Memory Engine 不解析厂商加密 compaction 内容。

## 11. Provider Context Strategy

Runtime 必须实现显式策略接口：

```rust
#[async_trait]
pub trait ProviderContextStrategy {
    async fn prepare_model_input(&self, context: ContextInput) -> Result<PreparedModelInput, AgentError>;
    async fn accept_model_output(&self, output: ProviderOutput) -> Result<ContextMutation, AgentError>;
}
```

只允许以下两种最终策略。

### 11.1 `ProviderNativeContextStrategy`

适用于服务端能力目录明确标记支持原生语义压缩的协议，例如官方 OpenAI Responses compaction。

实现要求：

- Local Agent Host 保存 Responses output items 和最新 compaction item。
- 每次请求执行完整 token guard。
- 按模型配置发送正式 `context_management` 参数。
- 收到新 compaction item 后，原子裁剪它之前已被替代的 Provider Context。
- tool call/result ID 必须跨 Step 保持一致。
- provider `completed`、`incomplete`、`failed` 必须分别处理；没有正式终态的流必须失败。
- Memory Engine 仍接收语义消息，但其 thread summary 和 recent records 不重复拼入当前厂商的续传历史。
- 新用户 turn 可以读取最新 Subject Memory，并作为有版本的系统记忆块注入一次；同一工具循环不得反复追加相同块。

### 11.2 `MemoryEngineContextStrategy`

适用于没有正式原生 compaction 契约的模型。DeepSeek 即使支持 OpenAI 风格 Responses 调用格式，也必须使用此策略，除非其官方 API 明确提供并且适配器实现了原生 compaction。

每次模型 Step 前必须：

1. 将本地未同步的当前 Run 语义记录同步到 Memory Engine。
2. 查询并等待当前 thread 已在运行的 active summary。
3. 调用 `context/compose` 获取 summary blocks、recent records 和 subject memory。
4. 加入固定系统约束、当前任务目标和未决工具批次。
5. 对完整模型输入执行 token 计数。
6. 超过主动阈值时调用 `active-summary/run`，等待成功后重新 compose。
7. 超过硬限制、总结失败、总结无改善或等待超时则暂停，不删除未总结历史继续请求。

此策略不得实现客户端私有摘要算法，也不得把简单截断冒充总结。

### 11.3 策略选择

`context_strategy` 由服务端模型配置 revision 明确给出：

```text
provider_native
memory_engine
```

禁止 `auto`、名称猜测和失败后静默切换。用户更换模型配置或协议时创建新的 Provider Context generation；不能把一个厂商的不透明 compaction item 发送给另一个厂商。新策略从本地语义记录和 Memory Engine 权威记录重建上下文。

## 12. Plugin 与 MCP 本地执行

最终执行链：

```text
Agent Profile
  → Local Capability Resolver
  → 已安装且已授权的固定 Plugin Release
  → Local MCP Runtime
  → tool result
  → Local Agent Runtime
```

强制要求：

- Plugin Management 只返回签名目录和不可变 Release/Component/Skill 描述。
- 客户端下载后校验签名、hash、版本和平台。
- Task 创建时冻结 Plugin Release、组件、工具 Schema、权限和项目作用域。
- stdio、loopback HTTP、Browser、Computer Use、终端和文件工具都在本地运行。
- 绝对路径、系统凭据和本地端口不上传 Plugin Management、Memory Engine 或 Model Gateway。
- 工具输出进入模型前必须执行大小、媒体、凭据和路径净化。
- 模型不能在运行中按名称替换冻结的插件版本。
- 最终架构不保留服务端 MCP Management 对 Agent 工具循环的调度职责。

## 13. 本地 IPC 与 UI

Local Agent Host 对 Native UI 暴露类型化接口：

```text
create_main_chat_turn
create_task
pause_run
resume_run
cancel_run
answer_user_question
approve_or_reject_tool
get_run
list_runs
subscribe_run_events(after_seq)
get_storage_profile
test_postgres_connection
apply_storage_profile
export_client_data
import_client_data
```

每个 UI 事件包含单调递增 `event_seq`。客户端断线后使用 cursor 补取，不依赖只存在于内存的 stream。

UI 必须分别展示：

- Agent Run 状态。
- 当前模型 Step。
- 工具执行状态。
- Task 进度与完成证据。
- Memory Engine 同步状态。
- 上下文总结/compaction 状态。
- 暂停、需人工复核、失败和取消原因。

关闭聊天页面不能取消 Run。Local Agent Host 作为客户端安装的一部分常驻；macOS 使用受应用管理的后台进程，Windows 使用对应的本地后台 Host。退出账户时暂停该账户未完成 Run，并清除内存中的模型访问令牌。

## 14. 安全与数据边界

- Local Agent Host 仅监听受保护的本地 IPC；如使用 loopback HTTP，必须有每次启动生成的随机会话令牌并校验调用进程身份。
- SQLite 敏感 payload 使用设备密钥加密；设备密钥存入 macOS Keychain 或 Windows DPAPI。
- PostgreSQL 使用用户提供的独立数据库账号和最小权限；生产网络连接必须校验 TLS 服务端证书。
- PostgreSQL 密码、DSN 和客户端私钥只存在系统安全存储和当前连接内存中。
- 模型配置凭据只存在服务端模型配置服务和 Model Gateway 内存中。
- Memory Engine 公网接口只接受当前用户 Bearer 身份，tenant、thread、subject 由客户端可信代码生成，模型不能指定。
- 工具权限以 Run 冻结快照为准；模型输出、插件说明、网页内容和 Memory 文本全部视为不可信数据。
- 文件写入、终端、支付和外部发布等副作用必须有稳定 invocation ID 和明确权限。
- 日志只记录 request ID、状态、耗时、token、item 类型计数、工具名和有界错误，不记录提示词全文、工具正文、密钥或二进制。

## 15. 目标代码结构

```text
crates/
├─ chatos_client_storage/
│  ├─ contracts/
│  ├─ repositories/
│  ├─ migrations/
│  ├─ sqlite/
│  └─ postgres/
├─ chatos_local_agent_protocol/
│  ├─ run.rs
│  ├─ event.rs
│  ├─ message.rs
│  ├─ tool.rs
│  └─ ipc.rs
├─ chatos_local_agent_runtime/
│  ├─ reducer.rs
│  ├─ scheduler.rs
│  ├─ storage.rs
│  ├─ model_step.rs
│  ├─ context/
│  │  ├─ provider_native.rs
│  │  └─ memory_engine.rs
│  ├─ memory_sync.rs
│  └─ tool_runtime.rs
└─ chatos_agent_profiles/
   ├─ main_chat.rs
   ├─ task_runner.rs
   ├─ approval.rs
   └─ story.rs

local_agent_host/
├─ main.rs
├─ ipc_server.rs
├─ profile_registry.rs
└─ lifecycle.rs

clients/macos/
└─ Native UI + typed Local Agent Host client

clients/windows/
└─ Native UI + typed Local Agent Host client
```

最终代码不包含主聊天和 Task Runner 的第二套 Swift/C# Agent Loop。

## 16. 实施工作包

实施者必须按依赖顺序完成以下工作包。每个工作包结束时必须达到对应测试条件，不能用空实现进入下一项。

### A. 统一客户端 Storage Provider

- 建立 `ClientStorage`、领域 Repository 和统一 Transaction Contract。
- 完成全部客户端结构化业务数据的存储访问审计，移除业务模块中的直接 SQLite、JSON 文件和数据库方言调用。
- 实现 SQLite 默认后端和 PostgreSQL 用户后端。
- 实现双后端 schema migration、契约测试、备份、显式导入导出和高级设置。
- 验证 PostgreSQL 故障不会触发 SQLite 回退或双写。

### B. 公共协议和 Durable Runtime

- 建立本地 Run、Event、Message、Provider Context、Tool Execution 和 Sync Outbox 类型。
- 只使用 Client Storage Repository 实现事务、claim、版本比较、事件去重和恢复扫描。
- 实现单步 reducer 和有界重试。
- 在 SQLite 与 PostgreSQL 中分别验证任意事务点杀进程后可以恢复且不重复执行已确认副作用。

### C. Model Configuration 与 Gateway

- 建立服务端 ModelRuntimeDescriptor 和 revision 契约。
- 建立只执行一次请求的 streaming Model Gateway。
- 完整覆盖 Responses/Chat Completions 正式终态、usage、request ID、超时和错误分类。
- 客户端只保存 descriptor，不接触服务端密钥。

### D. Provider Context Strategy

- 实现官方协议的 Provider Native compaction 适配器。
- 实现 Memory Engine compose/active-summary 适配器。
- 将 token guard 放在所有模型请求共同入口。
- 实现策略冻结、generation、厂商切换重建和工具配对验证。

### E. Message 与 Memory 同步

- 所有用户、助手、工具记录先通过当前 Client Storage 事务落库。
- 以稳定 ID 增量同步 Memory Engine。
- 实现断网重试、部分批次核对、账号切换隔离和同步状态 UI。
- 验证不会把完整请求或重复历史写入 Memory Engine。

### F. Main Chat Profile

- 将主聊天 Prompt、Skill、附件、Ask User、流式 UI 和最终结果接入本地 Profile。
- 将项目执行收口为创建本地 Task。
- 删除主聊天服务端 Cloud Agent 执行路径。

### G. Task Runner Profile

- 将 Task/Run、项目快照、Plugin 选择、进度、完成验证和结果回传接入本地 Profile。
- MCP 和 Plugin 工具直接进入本地执行器。
- 删除 Task Runner Service 的模型执行、队列、状态库和服务鉴权职责。

### H. 统一现有本地 Agent

- 审批和剧情改用相同 Runtime/Profile 契约。
- 删除 Swift `while true` Agent Loop 和重复的模型、Memory、重试实现。
- Windows 使用同一 Rust Runtime，不另写 C# Loop。

### I. Native UI 与后台 Host

- 接入本地 IPC、事件 cursor、恢复、暂停、取消、Ask User 和人工复核。
- 接入存储类型选择、PostgreSQL 连接测试、应用配置、错误修复和显式数据导入导出。
- 实现 Host 生命周期、账户隔离、更新和崩溃重启。
- UI 不再通过服务端 Task Runner 或 ChatOS Cloud Agent 状态渲染本地 Run。

### J. 删除最终架构之外的执行面

- 删除服务端主聊天 Cloud Agent consumer/outbox。
- 删除 Task Runner Service 及其 RabbitMQ、MongoDB、worker、模型 phase 和 MCP 调度入口。
- 删除 Cloud Agent 专用协议、配置键、队列、数据库集合和管理 UI。
- 删除服务端 MCP Management 在 Agent 工具执行链中的职责。
- 删除旧客户端 Agent Loop 和所有 fallback/feature flag/双写代码。

## 17. 必须通过的验收矩阵

| 场景 | 必须结果 |
| --- | --- |
| 主聊天普通问答 | 本地 MainChat Run 完成；用户和助手各一条稳定记录同步 Memory Engine |
| 主聊天创建真实项目任务 | 创建本地 Task；Main Chat 不直接获得项目写工具 |
| Task 多次模型/工具循环 | 每个 Step 独立事务；工具 call/result 配对；完成证据可查 |
| 应用在任意 Step 崩溃 | 重启后从最后已提交版本继续，不重复已确认副作用 |
| Model Gateway 重复/乱序流 | 没有正式终态则失败；不会产生空结果无限 continuation |
| 官方 OpenAI Responses 超阈值 | 返回并保存 compaction item；裁剪旧 Provider Context；所有请求仍执行 token guard |
| DeepSeek 长上下文 | 使用 Memory Engine 总结；不发送 OpenAI compaction 参数 |
| Memory Engine 总结无改善 | Run 暂停并说明原因，不截断历史继续请求 |
| Provider Native 时 Memory Engine 暂时不可用 | Run 可继续；记录保留在 sync outbox；UI 显示待同步 |
| Memory Engine Strategy 时服务不可用 | 在模型请求前暂停，恢复后同步、compose 并继续 |
| 工具执行后崩溃且结果未知 | `needs_review`，不自动重放副作用 |
| 重复本地事件 | 状态、消息、工具和 Memory record 均不重复 |
| 切换模型厂商 | 新 generation 从语义记录重建；不传递旧厂商不透明 item |
| 600 次长任务 | 每次模型输入有界；事件和 Provider Context 线性增长，不出现 O(n²) 请求快照 |
| macOS 与 Windows 同一用例 | Run 状态、工具行为、上下文策略和恢复结果一致 |
| 客户端页面关闭 | Run 继续由 Local Agent Host 执行并可重新订阅 |
| 账户切换 | 旧账户 Run 暂停；Memory、模型和插件权限不串号 |
| 首次启动未配置数据库 | 自动使用应用容器 SQLite，所有客户端功能可用 |
| 配置有效 PostgreSQL | 初始化统一 schema；重启后所有客户端业务 Repository 使用 PostgreSQL |
| PostgreSQL 连接或权限错误 | 明确显示 `storage_unavailable`；不创建 SQLite 替代 Run，PostgreSQL 中已有数据保持不变 |
| SQLite 与 PostgreSQL 契约测试 | 相同输入得到相同记录、排序、事务、幂等和恢复结果 |
| 两个客户端连接同一 PostgreSQL | device lease 生效；同一 Run 不会被并发执行 |
| 切换存储后检查旧数据库 | 旧库保持原样；不存在后台双写或隐式合并 |
| 客户端其他业务模块 | 会话、项目、Notepad、剧情、媒体、插件和设置均不直接依赖 SQLite |

## 18. 完成定义

只有同时满足以下条件，本文方案才算完成：

1. 主聊天和 Task Runner 都由 Local Agent Host 执行。
2. 两者只包含业务 Profile，不含重复 Agent Loop。
3. macOS 与 Windows 使用同一 Runtime 和状态协议。
4. 模型配置、密钥及 Summary Agent 绑定保留在服务端。
5. Model Gateway 无状态且一次请求只执行一个模型 Step。
6. 所有用户、有效助手、工具调用和工具结果都可通过稳定 ID 在 Memory Engine 查到。
7. OpenAI 原生 compaction 与 Memory Engine 总结按显式策略运行且不会重复注入历史。
8. DeepSeek 等无原生压缩模型只走 Memory Engine Strategy。
9. 本地插件和 MCP 不经过远程 Task Runner 或服务端工具循环。
10. 崩溃恢复、事件幂等、副作用保护和长上下文验收全部通过。
11. Task Runner Service、服务端主聊天 Cloud Agent、RabbitMQ Agent 队列、Mongo Cloud Agent 状态和旧客户端 Loop 已从生产代码及部署中删除。
12. 最终代码不存在兼容路由、双写、旧逻辑 fallback 或用于恢复旧架构的 feature flag。
13. 全部客户端结构化业务数据已接入 Client Storage Provider，不存在业务层直连 SQLite/PostgreSQL 或以 JSON 文件代替 Repository 的实现。
14. SQLite 是零配置默认后端，用户在高级设置中可配置并完整使用 PostgreSQL。
15. SQLite/PostgreSQL 契约、迁移、并发、崩溃恢复、备份和导入导出验收全部通过。

## 19. 明确禁止的实现

- 不允许简单把 Task Runner Service 二进制作为客户端 sidecar 继续运行。
- 不允许 Main Chat 与 Task Runner 各复制一份循环。
- 不允许保留 Swift Loop，同时再增加 Rust Event Runtime。
- 不允许本地 Runtime 继续依赖 RabbitMQ 或 MongoDB。
- 不允许业务模块直接打开 SQLite 或 PostgreSQL 连接。
- 不允许任何客户端功能只在 SQLite 或只在 PostgreSQL 下工作。
- 不允许 SQLite 与 PostgreSQL 双写、按数据类型拆库或数据库错误时自动 fallback。
- 不允许把 PostgreSQL 密码、DSN、证书私钥写入任一业务数据库、日志或诊断包。
- 不允许 Model Gateway 持有 Agent Run 或自动执行工具。
- 不允许客户端持久化服务端模型 API Key。
- 不允许把厂商 compaction 当作 Memory Engine 可读总结。
- 不允许把 Memory Engine summary 写回成新的用户或助手消息。
- 不允许为每个模型请求持久化累计 input。
- 不允许以“兼容 OpenAI”为依据启用 OpenAI 专用 compaction。
- 不允许因为 Memory Engine 或模型厂商失败而静默换模型、删历史或报告成功。
- 不允许保留旧服务作为备用执行路径。
