# 统一 Agent 抽象实施方案

> 状态：首版实施完成
> 创建日期：2026-07-12
> 适用范围：Chatos、Task Runner、Project Management Service、Local Connector Client、Plugin Management Service

当前进度：阶段 A、阶段 B、阶段 C、阶段 D、阶段 E 的首版均已完成；Chatos 已完成 shared runtime 生产链路切换，旧 AiClient/RequestHandler 实现已经删除。

## 1. 背景

项目已经具备 `chatos_ai_runtime`，统一处理模型请求、工具循环、Memory Engine、记录写入、重试与上下文恢复。但是业务 Agent 仍散落在多个服务中，各自组装能力策略、模型参数、工具、Memory、Prompt、运行 metadata 和最终结果校验。

当前系统 Agent 一共有六个：

1. `chatos_conversation_agent`
2. `chatos_planning_agent`
3. `project_requirement_execution_planner_agent`
4. `task_runner_run_phase`
5. `project_management_agent`
6. `local_connector_command_approval_agent`

当前主要实现位置：

- Chatos 对话、规划和需求执行规划：`chatos/backend/src/modules/conversation_runtime`
- Task Runner：`task_runner_service/backend/src/services/run_model_phase`
- 项目运行环境：`project_management_service/backend/src/services/environment_agent.rs`
- 本地命令审批：`local_connector_client/core/src/approval/ai_agent.rs`
- 系统 Agent 注册与能力绑定：`plugin_management_service/backend/src/seed.rs`

## 2. 目标

在项目根目录建立独立的 `agent` workspace crate，作为业务 Agent 层：

```text
业务服务/API
    -> agent：Agent 身份、Prompt、生命周期、类型化输入输出、结果校验
        -> chatos_ai_runtime：模型、工具循环、Memory、记录、重试
```

目标包括：

- 所有系统 Agent 具有统一、唯一的 descriptor 和 catalog。
- 统一模型配置补全、system prompt 合并、Memory 注入、记录 metadata、上下文溢出恢复和 runtime 调用。
- 每个 Agent 只实现自己的 Prompt、工具装配、业务输入输出和最终结果校验。
- 服务只保留数据库、认证、HTTP、文件系统、流式事件等基础设施 Adapter。
- 新增 Agent 时通过实现 Rust Trait 和注册 catalog 完成，不再复制完整运行流程。

## 3. 非目标

- 不重写 `chatos_ai_runtime`。
- 不把各服务数据库和 `AppState` 直接移动进 `agent` crate。
- 不改变现有 HTTP API、数据库结构、Memory thread 格式和流式事件协议。
- 不在一次提交中整体替换 Chatos 旧对话 runtime。

## 4. 目标目录

```text
agent/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── catalog.rs
    ├── core/
    │   ├── mod.rs
    │   ├── definition.rs
    │   ├── error.rs
    │   └── executor.rs
    └── implementations/
        ├── mod.rs
        ├── chatos.rs
        ├── project_environment.rs
        ├── command_approval.rs
        └── task_runner.rs
```

## 5. 核心抽象

Rust 不使用类继承，统一扩展点采用 Trait 与组合。所有 Agent 首先实现统一身份接口：

```rust
pub trait AgentIdentity: Send + Sync {
    fn descriptor(&self) -> &'static AgentDescriptor;
}

pub trait SystemAgentDefinition: AgentIdentity {
    fn system_prompt(&self) -> &'static str;
    fn message_mode(&self) -> &'static str;
    fn message_source(&self) -> &'static str;
    fn max_iterations(&self) -> usize;
    fn context_overflow_trigger(&self) -> &'static str;
    fn default_temperature(&self) -> Option<f64>;
    fn default_max_output_tokens(&self) -> Option<i64>;
}
```

普通 turn Agent 使用 `SystemAgentDefinition + AgentExecutor`；Task Runner 由于具有独立的 `TaskRuntime`、completion gate 和回调生命周期，使用 `AgentIdentity + TaskRunnerAgent` 类型化 facade，避免把所有差异塞入一个巨型 Trait。

`AgentExecutor` 统一负责：

1. 合并用户模型配置和固定 system prompt。
2. 应用 Agent 默认温度、输出 token 和最大工具循环次数。
3. 注入工具执行器。
4. 注入 Memory composer、writer 和 scope。
5. 构造统一的 user/assistant/tool record options。
6. 执行 `ContextualTurnRunner`。
7. 返回标准 `AiRuntimeResult` 或带 Agent 标识的错误。

具体 Agent 继续负责：

- 业务输入检查。
- 能力策略解析。
- 工具 provider 路由。
- 用户 Prompt 构造。
- 最终工具调用或持久化结果校验。

## 6. 迁移顺序

### 阶段 A：基础框架

- 新增 `agent` workspace crate。
- 新增六个系统 Agent 的统一 catalog。
- 新增 `SystemAgentDefinition`、`AgentExecutor`、`AgentTurnRequest`、`AgentTurnMemory`。
- Plugin Management seed 改为读取统一 catalog。
- 增加 catalog 唯一性和模型参数合并测试。

### 阶段 B：Project Environment Agent

- 将 system prompt、默认模型参数、message mode/source、最大迭代次数和 overflow trigger 移入 `agent`。
- 服务继续负责项目检查、路由、能力解析、MCP executor 和环境结果持久化。
- 删除服务内重复的 runtime、record options 和 turn spec 拼装。

### 阶段 C：Command Approval Agent

- 将审批 Agent 定义和 runtime 拼装移入 `agent`。
- Local Connector 继续负责 workspace 定位、模型解析、审批工具、Memory client 和 decision sink。
- 保持“没有调用 `approval_decision` 则失败”的业务校验。

### 阶段 D：Task Runner Agent

- 迁移 run spec、MCP 装配、Memory scope、record options 和公共生命周期。
- 保留任务记录、沙箱、Harness、回调、取消轮询和 completion gate Adapter。

首版实施结果：

- `TaskRunnerAgent` 已实现统一 `AgentIdentity`。
- `TaskRunSpec` 的标准构造、message mode/source、metadata 和 user record 已迁移。
- 模型运行入口已通过 `TaskRunnerAgent` facade 调用。
- MCP provider、Memory scope、沙箱、Harness、回调、超时和 completion gate 继续由服务 Adapter 负责。

### 阶段 E：Chatos 三个 Agent

- 为普通对话、规划、需求执行规划建立独立定义，共享 Conversation Agent 基础组件。
- 兼容流式输出、附件、reasoning、Codex Gateway passthrough、tool panel 和消息持久化。
- 使用开关并行验证新旧链路后，逐步删除旧 `services/agent_runtime` 重复实现。

当前实施结果：

- 普通对话、规划、需求执行规划的 Agent key 选择已迁移到 `ChatosAgentProfile`。
- 是否要求具体项目、Task Runner tool profile、task profile、plan mode header 和 Project Management MCP 要求已统一维护。
- 保留了 `plan_mode=true` 与需求执行规划同时出现时的旧行为。
- 新增 `ChatosStreamRuntime` 端口和 `ChatosStreamAgent` facade，conversation runtime 不再直接持有或调用旧 `AgentAiServer`。
- 过渡阶段曾由 `AgentAiServer` 兼容 Adapter 承接统一流式端口；当前该端口已直接落到 shared runtime，conversation use case 保持不变。
- `chatos_ai_runtime::RuntimeCallbacks` 已区分逻辑模型输入快照、旧 Task Runner debug payload 和最终 provider payload 快照。
- Chatos 的输入/payload 快照回调已经转换为共享 runtime callbacks，并随统一执行 options 传入 Adapter。
- shared runtime 已提供 turn phase、运行指导和上下文总结回调协议；Memory Engine 上下文溢出恢复会发送结构化 summary start/end 事件。
- shared runtime 已新增 `RuntimeLifecycleHook`：支持每轮前注入临时输入、隐藏内部轮次的流式输出、临时禁用工具，以及在最终响应后接受、替换或追加反馈继续。
- Chatos runtime guidance 已通过 lifecycle Hook Adapter 接入 shared runtime，继续复用现有指导队列和 applied 回调。
- task-board follow-up 已迁移为状态化 lifecycle Hook：统一维护执行/复查模式、follow-up 轮数、上一条用户可见响应、复查 locale/outcome 和内部连续上下文。
- 复查轮次会关闭流式输出并禁用工具；复查通过后使用 `Replace` 恢复上一条用户可见响应，复查未通过时继续同一轮执行。
- lifecycle continuation 会显式携带中间 assistant 响应和系统指导，避免 shared runtime 切换后依赖旧 `previous_response_id` 或把内部复查内容暴露给用户。
- `AgentAiServer` 的统一流式端口已切换到 `ContextualTurnRunner/AiRuntime`：模型配置、Memory scope、MCP executor、callbacks、lifecycle、record options 和 abort checker 均通过 shared runtime 执行。
- 旧 `AiServer::chat/AiClient`、旧 RequestHandler 和相关恢复循环已经删除，不再参与生产或测试构建。
- runtime guidance 输入构造已从旧 `AiClient` 执行循环拆出为独立服务，生产链路不再依赖旧请求处理器。
- shared runtime 最终 assistant 记录支持 lifecycle metadata overlay，task review 的 attempted/outcome/rounds 会继续持久化并返回给前端。
- 旧 request flow、Task Board refresh store、AiClient/RequestHandler 和仅服务旧链路的 runtime carrier 已删除。
- Chatos 生产库已恢复为无新增编译告警，shared runtime 的 user/assistant/tool record contract 已增加独立测试覆盖。
- shared runtime 已增加真实 HTTP mock lifecycle 回归：验证同一轮隐藏复查、复查轮禁用工具、连续上下文、不使用 `prev_id` 和最终可见响应恢复。

## 7. 验收标准

- 六个系统 Agent descriptor 只有一个权威 catalog。
- Project Environment 和 Command Approval 不再自行拼装 `AiRuntime`、`ContextualTurnRunner`、record options 和 `RuntimeTurnSpec`。
- 现有 API、Memory metadata、message mode/source 和错误语义保持兼容。
- 新增 Agent 时只需要新增定义、实现业务 Adapter 并注册 catalog。
- `cargo fmt --all --check` 通过。
- `chatos_agent`、Project Management、Local Connector、Plugin Management 相关编译和测试通过。

## 8. 风险控制

- 不创建包含所有服务状态的巨型 `AgentContext`，通过小型 Adapter 注入依赖。
- 不使用 `serde_json::Value` 作为所有 Agent 的统一业务输入输出，业务层保持类型安全。
- Chatos 流式 runtime 最后迁移，避免第一阶段扩大影响面。
- 每迁移一个 Agent，先做行为等价测试，再删除旧拼装代码。
- 能力策略解析失败继续 fail closed，不恢复成硬编码全量工具集。

## 9. 最终验收记录

2026-07-12 基于当前工作区完成以下验证：

- `cargo fmt --all -- --check`：通过。
- `cargo check -p chat_app_server_rs --tests --ignore-rust-version`：通过。
- `cargo check -p chatos_agent -p chatos_ai_runtime -p chat_app_server_rs -p task_runner_service_backend -p project_management_service_backend -p local_connector_client_core -p plugin_management_service_backend --ignore-rust-version`：通过。
- `chatos_agent`：6 个测试通过。
- `chatos_ai_runtime`：135 个测试通过，包含真实 HTTP mock lifecycle 回归。
- Chatos conversation runtime：34 个定向测试通过。
- Chatos `ai_common`：33 个定向测试通过。
- Local Connector：28 个测试通过。
- Plugin Management：26 个测试通过。
- Project Management：54 个测试通过。
- Task Runner：177 个测试通过。
- 旧 `AiClient`、`AiClientSettings`、`ProcessOptions`、`agent_runtime::ai_client` 和 `agent_runtime::ai_request_handler` 引用扫描结果为空。
- `git diff --check`：通过；仅报告工作区既有的 CRLF/LF 转换提示，没有空白错误。

完整 Chatos 测试套件此前执行 466 个测试，其中 455 个通过、11 个失败。失败项集中在 Windows/POSIX 路径、脚本和既有 prompt 文案断言，与本次 Agent/runtime 迁移文件无交集；本次改动覆盖的 Chatos 定向测试全部通过。
