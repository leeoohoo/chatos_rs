# 云端统一编排与轻量 Local Connector 回迁实施方案

## 0. 文档状态

- 状态：2.0.10 已完成
- 日期：2026-08-01
- 目标版本：2.0.10
- 替代方向：docs/plan/LOCAL_PROJECT_CLIENT_ORCHESTRATION_SQLITE_IMPLEMENTATION_PLAN.zh-CN.md
- 历史参考点：commit 0e9823d0 的父版本

本文档不删除旧的“本地项目客户端编排与 SQLite”方案，旧文档保留为历史实现说明；从本方案通过评审后，产品和架构方向以本文档为准。

当前实施进度：2.0.10 目标模式已经完成。Project、Session、Message、Task、Memory、Agent 生命周期均由云端持有；ChatOS、Task Runner 与 Project Environment Agent 的模型只连接 MCP Management 聚合 endpoint。Local Connector 不再启动本地 Task Worker，也不再挂载 Chat、Session、Memory、Project Management、Task Board、Task Run 和 Environment Agent 业务 API，只保留 Workspace 文件/Git/代码导航、命令、本地 Skill/Plugin/MCP、Sandbox 和设备控制状态。旧审批 Memory Engine proxy、Memory Policy cache 与云端读取本地模型凭据入口已经删除。Local Command Approval Agent 完整留在设备侧，只读取本地已安装 capability snapshot、只调用本地只读代码工具与 `approval_decision`，不创建云端 MCP Runtime Session、不访问云端 Memory Engine。Task Runner 已停止生成 Harness/Local Connector ephemeral MCP endpoint；Provider、设备、Workspace 与 Sandbox 路由由程序根据权威 Project Execution Context 决定。配置中心参数 `chatos.ui.local_project_creation_enabled` 已控制云端界面是否展示新建本地 Workspace Project 入口。全量 `make verify`、仓库 smoke 和真实 `make smoke-cloud-mcp` 已通过；真实 smoke 会创建并关闭 Runtime Session，验证聚合 `tools/list`、`MCP Management -> Task Runner` 调用、owner/caller 隔离、本地审批 Agent 云端拒绝、Runtime Token 防篡改和 Session 关闭后不可复用。

---

## 1. 结论先行

恢复为“所有业务编排走云端，客户端只提供受控本地能力”的模式，属于中等规模改造，不是重新设计整个系统。

当前代码虽然已经把 Local Connector Project 的 Chat、Session、Message、Task Runner、Memory、Project Management 和模型调用下沉到客户端，但原来的 Local Connector relay 主干仍然存在：

- 客户端主动建立的 outbound WebSocket。
- 云端经 Local Connector Service 调用本机文件、Git、代码导航和终端。
- 本地 Skill/Plugin 的 prepare、execute、cancel。
- 本地 Docker Sandbox 的 pairing、facade 和 lease。
- 云端创建绑定本机 Workspace 的项目。
- Project Management Service 中的 Local Connector 文件与 Sandbox provider。

因此本次工作的本质是：

1. 把项目、会话、消息、任务、记忆、模型推理和 Agent 生命周期重新收归云端。
2. 恢复云端 Task Runner 与 Project Environment Agent 对 Local Connector provider 的使用。
3. 把客户端从“第二套业务执行平面”收缩为“设备能力网关”。
4. 稳定后删除不再使用的本地业务 Runtime，而不是整体回滚历史提交。

用户已确认不需要迁移历史业务数据。旧的本地项目、Session、Message、Turn、Task、Memory、Requirement 和 Run 可以直接放弃，这会移除最复杂的数据转换、双写、对账和冲突处理工作。

预计工作量：

- 完成功能切换并可灰度：单人约 8～12 个工作日。
- 连同大规模本地 Runtime 删除、测试收敛和发布观察：单人约 12～18 个工作日。
- 两人并行时，通常可在 1～2 个自然周完成可上线版本；彻底清理可延后一版。

---

## 2. 当前架构判断

### 2.1 改造前是双执行平面

改造前代码把项目的代码位置与 Agent 的执行位置绑定在了一起：

- Cloud Project：业务数据和执行编排在云端。
- Local Connector Project：业务数据和执行编排在客户端 SQLite 与 Local Runtime。

主要证据：

- README.zh-CN.md 描述了两个执行平面和 local_runtime_required 行为。
- local_connector_client/core/src/lib.rs 启动本地业务数据库、Task Worker 和 Local Runtime。
- local_connector_client/core/src/runtime.rs 持有 LocalDatabase、Turn/Memory/Ask User/Environment Registry 和本地 Worker。
- local_connector_client/core/src/local_runtime/ 包含完整的本地 Chat、Memory、Project Management、Task Board、Task Runner 和存储实现。
- local_connector_client/core/migrations/ 包含本地业务表。
- chatos/frontend/src/lib/api/localRuntime/ 提供前端本地业务 API。
- projectsFacade.ts、sessionsFacade.ts、sendMessage.ts 等根据项目或 Session 类型切换云端与本地 API。

### 2.2 改造前的核心问题

改造前客户端已经不再只是 Connector，而成为了第二套 ChatOS Backend：

- 同一套业务需要同时维护云端和客户端实现。
- 前端到处存在 local/cloud 分流。
- SQLite migration、Worker 恢复、任务租约、Memory Rollup、Ask User 状态机等增加了大量客户端复杂度。
- 云端为了防止误执行 Local Project，加入了多处 local_runtime_required gate。
- 本地插件、MCP、模型、沙箱、业务数据库和桌面安全边界互相耦合。
- 每新增一项云端能力，都需要判断是否还要实现一套本地版本。

从长期维护成本看，当前复杂度主要不在 Local Connector 本身，而在“客户端拥有完整业务事实数据和 Agent 编排”。

### 2.3 可复用资产仍然完整

本方案可以直接复用或选择性恢复以下能力：

| 能力 | 当前状态 | 本方案处理 |
| --- | --- | --- |
| 设备注册与在线状态 | 已存在 | 保留 |
| Workspace 注册与授权 | 已存在 | 保留 |
| outbound WebSocket relay | 已存在 | 保留并加强协议 |
| 文件读写与目录搜索 | 已存在 | 保留 |
| Git 与代码导航 | 已存在 | 保留 |
| 终端和命令执行 | 已存在 | 保留 |
| 本地 MCP | 已存在 | 保留为本地执行组件 |
| 本地 Skill/Plugin | 已存在 | 保留 |
| Plugin 凭据与 OAuth | 已存在 | 保留在设备端 |
| 本地 Docker Sandbox | 已存在 | 保留 |
| 云端 Local Connector Project 创建 | 已存在 | 调整 execution_plane 语义 |
| 云端文件/Git bridge | 已存在 | 继续使用 |
| Project Management Local Connector provider | 大部分仍在 | 移除 gate 后恢复 |
| Task Runner Local Connector MCP/Sandbox routing | 被删除或阻断 | 从历史版本选择性前移植 |

### 2.4 不应整体 Git revert

0e9823d0 同时包含本地 Runtime、配置中心、插件策略、安全加固、打包和大量后续依赖。直接 revert 会覆盖之后的修复，也容易碰到当前正在修改的 Plugin Management 与 Task Runner policy 文件。

正确方式是：

- 用 0e9823d0^ 作为旧链路参考。
- 按模块选择性恢复 Local Connector MCP/Sandbox routing。
- 对当前代码做正向语义调整。
- 本地 Runtime 先停用、后删除。

---

## 3. 目标与非目标

### 3.1 目标

1. 所有 Project、Session、Message、Turn、Task、Requirement、Memory 和 Run 以云端数据库为事实数据源。
2. 所有普通 Chat、Plan Agent、Task Runner、Project Environment Agent 和 Memory 处理在云端执行。
3. Local Connector Client 只提供必须在用户设备执行的能力。
4. 本地 Workspace 不上传绝对路径，云端只保存 device_id、workspace_id 和安全相对路径。
5. 本地命令、Plugin/Skill、OAuth 和 Sandbox 继续遵守设备侧权限与审批。
6. Local Connector 离线时明确失败或等待，不静默切换执行位置。
7. Cloud Project 不依赖 Local Connector；Local Workspace Project 只在需要本地能力时依赖 Connector。
8. 发布期间支持新旧客户端的安全灰度。

### 3.2 非目标

1. 不迁移旧客户端 SQLite 中的历史业务数据。
2. 不做云端与客户端业务数据库双写。
3. 不保留本地模型作为 ChatOS 核心 Agent 的执行方式。
4. 不让云端直接访问用户机器的监听端口。
5. 不把本地绝对路径、Plugin 私钥、OAuth Token 或设备密钥上传云端。
6. 不在第一阶段重写全部 Local Connector 协议。
7. 不自动删除旧 runtime.sqlite3；2.0.10 使用全新的 connector-state.sqlite3 保存最小控制状态。

### 3.3 明确的数据处理边界

切换后允许放弃：

- 本地项目索引。
- 本地 Session、Message、Turn 和 Event。
- 本地 Memory Summary、Recall、Rollup。
- 本地 Requirement、Document、Work Item。
- 本地 Task Board、Task Run、Ask User Prompt。
- 本地 Project Environment 分析状态。

不得误删：

- 用户真实 Workspace 和源码目录。
- 设备身份与私钥。
- Workspace grant 和本机绝对路径映射。
- 已安装 Plugin/Skill 包。
- Plugin 本地凭据、OAuth 连接和授权信息。
- 本地 MCP 配置。
- Sandbox 镜像、运行配置和必要的权限策略。
- 用户明确要求保留的命令审批或审计设置。

注意：本方案允许客户端保留一个小型本地控制数据库，但它只能保存设备能力状态，不能重新承担 ChatOS 业务数据。

---

## 4. 目标架构

~~~mermaid
flowchart LR
    UI["Desktop / Web UI"]
    CHAT["ChatOS Backend<br/>Project / Session / Chat"]
    PM["Project Management Service"]
    TR["Task Runner Service"]
    MEM["Memory Engine"]
    MODEL["Cloud Model Runtime"]
    LCS["Local Connector Service<br/>Relay / Presence / Pairing"]
    WS["Outbound WebSocket"]
    LC["Light Local Connector<br/>Capability Gateway"]
    FS["Workspace Files / Git"]
    TERM["Terminal / Host Command"]
    PLUGIN["Local Plugin / Skill / MCP"]
    SB["Local Docker Sandbox"]
    CLOUDDB[("Cloud Business Data")]
    LOCALCTRL[("Local Control State")]

    UI --> CHAT
    CHAT --> PM
    CHAT --> TR
    CHAT --> MEM
    CHAT --> MODEL
    PM --> MODEL
    TR --> MODEL
    MEM --> MODEL

    CHAT --> CLOUDDB
    PM --> CLOUDDB
    TR --> CLOUDDB
    MEM --> CLOUDDB

    CHAT --> LCS
    PM --> LCS
    TR --> LCS
    LCS --> WS
    WS --> LC

    LC --> FS
    LC --> TERM
    LC --> PLUGIN
    LC --> SB
    LC --> LOCALCTRL
~~~

关键原则：

- 云端拥有业务状态与 Agent 生命周期。
- 客户端拥有设备资源、设备权限和本地执行句柄。
- Local Connector Service 只做认证、在线状态、请求路由和有限状态配对，不成为业务数据库。
- 每一次本地调用都带云端 run_id、tool_call_id、device_id、workspace_id 和权限范围。

---

## 5. 职责所有权

| 领域 | 云端负责 | 客户端负责 |
| --- | --- | --- |
| Project | 创建、更新、归档、成员权限、source metadata | Workspace 授权与路径映射 |
| Session/Message/Turn | 唯一事实数据源、事件、恢复、取消 | 不保存业务历史 |
| 模型推理 | Chat、Plan、Task、Memory、Environment Agent | 核心链路不执行模型 |
| Memory | Summary、Recall、Repair、检索 | 不保存 Chat Memory |
| Project Management | Requirement、Document、Work Item、依赖图 | 只提供本地文件上下文和工具 |
| Task Runner | Queue、Lease、Retry、Heartbeat、Run/Event/Output | 执行被分派的本地工具 |
| 文件/Git | 发起受权调用、记录工具结果 | 在 Workspace grant 内实际读写 |
| 终端 | 决定何时调用、维护 tool call 状态 | 审批、启动、输出、取消进程 |
| Plugin/Skill | Catalog、策略、选择、运行编排 | 安装、校验、本地执行、凭据 |
| Sandbox | 选择 provider、维护云端 Run/Lease 引用 | Docker 容器和本地 lease 实体 |
| 凭据 | 云端模型凭据 | 本地 Plugin/OAuth/设备凭据 |
| 审计 | 业务事件、调用摘要 | 本地敏感操作与审批记录 |

---

## 6. 核心模型语义调整

### 6.1 解耦三个概念

当前 execution_plane 同时被用于推断代码位置和 Agent 位置，应拆开：

~~~text
execution_plane    = cloud
workspace_provider = cloud | local_connector
sandbox_provider   = cloud | local_connector | none
~~~

建议保留 source_type 作为项目来源兼容字段：

~~~text
source_type = cloud | local_connector
~~~

最终规则：

- execution_plane 只表示 ChatOS/Agent 编排位置，本方案中固定为 cloud。
- source_type 或 workspace_provider 表示代码和文件系统位于哪里。
- sandbox_provider 表示命令隔离环境位于哪里。
- Plugin/Skill 继续使用组件级 execution_host=cloud|local|portable。

不能再使用以下推断：

~~~text
source_type == local_connector -> execution_plane == local_connector
root_path startsWith local://connector/ -> execution_plane == local_connector
~~~

### 6.2 Local Workspace Project

Local Workspace Project 在云端应是一个正常 Project：

~~~text
project.id              = cloud project id
project.execution_plane = cloud
project.source_type     = local_connector
project.root_path       = local://connector/{device_id}/{workspace_id}/{relative_path?}
~~~

root_path 是逻辑路由标识，不是用户机器绝对路径。

客户端继续保存：

~~~text
(owner_user_id, device_id, workspace_id) -> absolute_root
~~~

所有云端工具参数只能使用 Workspace 内相对路径。客户端负责路径规范化、符号链接检查和越界拒绝。

### 6.3 旧数据处理

因为不迁移历史数据：

- lc_project_*、lc_session_* 等客户端业务 ID 不转换为云端 ID。
- 旧本地项目切换后不再展示。
- 用户从同一个 Workspace 重新创建一个云端 Project。
- 可提供“重新连接此 Workspace”入口，但它创建的是全新的云端 Project，不导入历史。
- 旧 runtime.sqlite3 停止全部读写且不自动删除；历史业务数据不迁移。

如果云端数据库中存在 execution_plane=local_connector 的旧 Project，可以选择：

1. 直接归档并由用户重新创建，推荐。
2. 只修改 Project metadata 为 execution_plane=cloud，不迁移 Session。

两种方式都不能删除真实 Workspace。

### 6.4 本地控制状态

Local Connector 仍需要少量持久化状态。2.0.10 已拆分为 connector-state.sqlite3，并以全新最小 schema 启动，不迁移旧业务数据库。

允许的控制状态包括：

- MCP manifest 与启用状态。
- Plugin/Skill 安装清单和哈希。
- Plugin OAuth 与凭据引用。
- 本地审批策略。
- Sandbox pairing、镜像、lease 和恢复元数据。
- 命令执行句柄和有限审计摘要。

禁止继续写入：

- projects、sessions、messages、turns、runtime_events。
- memory_summaries、subject_memories。
- requirements、documents、work_items。
- task_board_tasks、task_runs。
- 本地 Agent capability snapshot 中与本地编排绑定的业务快照。

---

## 7. 关键业务流程

### 7.1 创建本地 Workspace Project

1. 客户端登录云端并注册 device_id。
2. 用户在客户端选择本机目录。
3. 客户端生成 workspace_id，保存 workspace_id 到绝对路径的本地映射。
4. 客户端向 Local Connector Service 注册 Workspace 的非敏感 metadata。
5. ChatOS Backend 校验用户、设备、Workspace 在线状态和所有权。
6. ChatOS Backend 创建云端 Project：
   - execution_plane=cloud
   - source_type=local_connector
   - root_path 使用 local://connector/... 逻辑路径
7. ChatOS Backend 创建 Local MCP、Terminal 和可选 Sandbox binding。
8. Project、Session 和后续业务数据全部进入云端服务。

验收点：

- 云端数据库能查到 Project。
- 客户端数据库没有新增本地 Project。
- 云端看不到本机绝对路径。
- 删除 Project 不删除本机目录。

### 7.2 云端 Chat

1. UI 始终调用云端 Chat API。
2. ChatOS Backend 从云端读取 Session、Message、Agent 配置和模型凭据。
3. 模型在云端运行。
4. 当模型调用文件或终端工具时，云端通过 Local Connector Service relay 到目标设备。
5. 工具结果回到云端 Agent loop。
6. Message、Turn、Tool Result 和 Memory 由云端持久化。

客户端不再：

- 创建本地 Turn。
- 请求核心模型。
- 写本地 Message/Event。
- 运行本地 Chat Agent loop。

### 7.3 云端 Task Runner 调本地文件和终端

1. Task Runner 从云端 Project metadata 识别 workspace_provider=local_connector。
2. Workspace MCP 构建 Local Connector HTTP server/facade。
3. Task Runner 获取短期、调用方绑定、scope 绑定的内部 token。
4. 工具请求经 Local Connector Service 发送给设备。
5. 客户端校验 owner、device、workspace、相对路径、工具类型和审批要求。
6. 客户端执行后返回有大小限制的 stdout、stderr、结构化结果或 artifact 引用。
7. Task Runner 在云端维护重试、取消、Run 状态和最终输出。

必须恢复或调整：

- task_runner_service/backend/src/services/workspace_mcp.rs
- task_runner_service/backend/src/services/sandbox_runtime/routing.rs
- Task Runner 中 Local Skill/Plugin relay 的执行策略
- task_runner_service/backend/src/services/model_runtime_resolver.rs 对 Local Workspace Project 的阻断

核心模型仍使用云端凭据，不能再向客户端请求用户模型 API Key。

### 7.4 本地 Plugin/Skill

Plugin Management Service 继续作为 Catalog 和策略事实数据源。

运行流程：

1. 云端解析 Agent 能力和具体组件的 execution_host。
2. execution_host=cloud 的组件在云端运行。
3. execution_host=local 的组件通过 Local Connector relay。
4. 云端发起 prepare，客户端校验安装版本、bundle hash、依赖和本地授权。
5. 云端发起 execute，客户端使用本地凭据或 OAuth。
6. 长任务使用 adapter_session_id / execution_id 支持 cancel。
7. 客户端只返回允许的结果或 artifact，不返回原始凭据。

Skill 需要区分：

- 纯 instructions：云端获取已授权内容后注入模型上下文。
- 本地 executable skill：实际工具在客户端运行。
- portable skill：由策略明确选择执行 host，不能根据失败静默切换。

现有 Connector 中的 skill_prepare、skill_execute、skill_cancel，以及 plugin_prepare、plugin_execute、plugin_cancel 应保留。

### 7.5 本地 Docker Sandbox

沙箱执行位置独立于 Agent 执行位置：

~~~text
Agent execution = cloud
Sandbox provider = local_connector
~~~

流程：

1. 云端 Run 选择 sandbox_provider=local_connector。
2. Task Runner 或 Project Environment Agent 查询设备的 Sandbox pairing。
3. 云端通过 Local Connector Service sandbox facade 创建 lease。
4. 客户端启动或复用本地 Docker 容器。
5. 云端通过 facade 调用 Sandbox MCP。
6. 客户端负责容器、网络、挂载、权限和本地资源限制。
7. 云端负责 Run 状态、取消意图和最终释放。
8. 客户端在断线或超时后使用 TTL/reaper 回收孤儿 lease。

不得在本地 Sandbox 不可用时自动改用 Cloud Sandbox。是否切换 provider 必须由用户或显式策略决定。

### 7.6 离线与重连

Connector 离线时：

- Project 和历史 Session 仍可从云端查看。
- 不需要本地文件的普通管理操作可以继续。
- 依赖 Workspace、终端、本地 Plugin 或本地 Sandbox 的 Run 在启动前返回 connector_offline。
- 已运行到本地工具阶段的 Run 标记为 waiting_for_connector 或明确失败，行为由任务类型决定。

重连时：

- 客户端重新上报 device、workspace、MCP、Skill/Plugin 和 Sandbox 状态。
- 云端不从客户端恢复 Chat Agent loop。
- 云端可以重试幂等的只读工具调用。
- 文件写入、Git mutation 和命令执行默认不自动重放。
- 对可能重放的请求使用 request_id 幂等缓存。

### 7.7 取消和失败

取消链路：

~~~text
UI cancel
  -> cloud Turn/Run cancellation
  -> cancel active tool_call_id
  -> Local Connector Service relay
  -> local process/plugin/sandbox cancellation
  -> cloud records terminal state
~~~

规则：

- 云端是取消状态的最终裁决者。
- 客户端必须支持按 execution_id 取消命令、Plugin/Skill 和 Sandbox 操作。
- 客户端无法确认取消时，云端记录 cancel_requested，而不是伪造 cancelled。
- 连接中断不能把 mutation 当成未执行；必须返回 unknown_execution_state，并要求查询或人工确认。
- stdout/stderr 和错误信息必须脱敏并限制大小。

---

## 8. 分阶段实施计划

### Phase 0：冻结语义并加 Feature Flag

目标：让切换可灰度、可回滚。

新增或统一 Feature Flag：

~~~text
cloud_orchestration_for_local_connector_projects
local_runtime_business_api_enabled
local_task_worker_enabled
local_connector_sandbox_routing_enabled
~~~

工作项：

- 定义 workspace_provider、sandbox_provider 和 execution_host 的最终语义。
- 为 Relay 协议增加 capability_version。
- 为 Cloud Service 增加按用户或设备灰度开关。
- 增加客户端业务表写入计数或日志，供切换验收。
- 明确新旧客户端最低兼容版本。

验收：

- Flag 关闭时当前行为不变。
- Flag 开启只影响测试用户或测试设备。

预计：0.5～1 天。

### Phase 1：恢复云端业务数据和前端 API 路由

目标：Local Workspace Project 的所有业务 UI 重新走云端。

重点修改：

- chatos/frontend/src/lib/api/client/facades/workspace/projectsFacade.ts
- chatos/frontend/src/lib/api/client/facades/workspace/sessionsFacade.ts
- chatos/frontend/src/lib/api/localRuntime/
- chatos/frontend/src/lib/store/actions/sendMessage.ts
- chatos/frontend/src/lib/store/actions/stopMessage.ts
- 本地 Event/Ask User/Memory/Task Board polling
- Project Plan、Requirement、Document、Task 与 Review/Repair 路由

处理方式：

- projectUsesLocalRuntime 不再根据 source_type 或 local:// 路径返回 true。
- Local Workspace Project 使用正常云端 Project/Session API。
- 移除 lc_project_*、lc_session_* 业务分支。
- 继续保留桌面 Surface 和 Local Connector 资源管理 UI。
- 文件浏览和 Git API 也统一调用 ChatOS Backend，由 Backend bridge 到 Connector；前端不直接调用 Local Runtime。
- 创建 Local Workspace Project 后直接使用云端返回的 Project ID。

验收：

- 新建 Local Workspace Project 后，Session、Message 和 Task 均可在云端数据库查询。
- 刷新或更换客户端后，历史仍存在。
- 旧 runtime.sqlite3 不再被打开；connector-state.sqlite3 只产生控制类记录。

预计：2～3 天。

### Phase 2：移除云端 local_runtime_required gates

目标：云端 Chat、Task Runner 和 Project Environment Agent 接受 Local Workspace Project。

重点修改：

- chatos/backend/src/core/project_execution.rs
- chatos/backend/src/api/projects/requirement_execution/request_context.rs
- task_runner_service/backend/src/services/model_runtime_resolver.rs
- project_management_service/backend/src/services/environment_agent/runtime/analysis.rs

处理方式：

- execution_plane=cloud 时允许进入云端 Agent，不再因 source_type=local_connector 拒绝。
- 项目代码位置交给 workspace_provider routing。
- 云端模型解析只使用云端凭据。
- 删除或改写 local_runtime_required 错误，使其只用于真正过期的客户端本地业务 API。
- 保留 Connector offline、workspace unavailable、local execution host unavailable 等明确错误。

验收：

- 云端 Chat 可为 Local Workspace Project 创建 Turn。
- Environment Agent 可进入 Local Connector provider。
- Task Runner 可启动 Run，并在调用本地工具前完成权限解析。
- Cloud Project 行为无回归。

预计：2～3 天。

### Phase 3：恢复 Local Connector MCP、Plugin/Skill 和 Sandbox routing

目标：云端 Agent 能完整调用本机能力。

重点修改：

- task_runner_service/backend/src/services/workspace_mcp.rs
- task_runner_service/backend/src/services/sandbox_runtime/routing.rs
- project_management_service/backend/src/services/environment_agent/routing.rs
- project_management_service/backend/src/services/environment_agent/mcp_servers.rs
- local_connector_service/backend/src/api/internal_auth.rs
- Local Connector relay 与 sandbox facade

处理方式：

- 从 0e9823d0^ 选择性前移植 Task Runner 的 Local Connector MCP/Sandbox 实现。
- 适配当前的 Plugin Management policy、内部 token 和 permission profile。
- 保留当前安全加固，不恢复旧的宽权限 token 或本地模型凭据回拉。
- 对读操作增加短重试和幂等。
- 对 mutation 增加 request_id、执行状态和 unknown 状态处理。
- 完善命令、Plugin/Skill 和 Sandbox cancel。

验收：

- 云端 Task Runner 能读写授权 Workspace 文件。
- 云端 Agent 能执行本地终端命令并经过设备审批。
- 本地 Plugin/Skill 能 prepare、execute、cancel。
- 本地 Docker Sandbox 能 create、execute、release。
- 设备不在线时无静默 fallback。

预计：2～4 天。

### Phase 4：停止客户端本地 Worker 和业务 API

目标：客户端不再运行第二套业务平面。

重点修改：

- local_connector_client/core/src/lib.rs
- local_connector_client/core/src/runtime.rs
- local_connector_client/core/src/api.rs
- local_connector_client/core/src/api/handlers.rs
- local_connector_client/core/src/local_runtime/

处理方式：

- 不再启动 run_local_task_worker_loop。
- 不再创建 Turn/Memory/Ask User/Environment Registry。
- 不再暴露 /api/local/runtime/* 业务 API。
- LocalRuntime 重命名或拆分为 ConnectorRuntime/DeviceRuntime。
- 保留 connector_task、PluginRuntimeHost、PluginInstaller、OAuth、Workspace、Terminal 和 SandboxRuntime。
- 暂时保留数据库实例，只服务 MCP manifest、Plugin/Skill 和 Sandbox 控制状态。
- 禁止远程 model_runtime_request 返回本地模型密钥。

验收：

- 客户端启动后只有 Connector、设置、设备能力与本地审批服务。
- 没有本地 Agent loop 或 Task Worker。
- Chat/Task 执行期间业务表写入为零。
- 文件、命令、Plugin/Skill 和 Sandbox relay 正常。

预计：1～2 天。

### Phase 5：删除本地业务 Runtime 和业务表

目标：降低代码和长期维护成本。

删除候选：

- local_connector_client/core/src/local_runtime/ 中的 Chat、Memory、Project Management、Task Board、Task Runner、业务 API 与业务存储。
- chatos/frontend/src/lib/api/localRuntime/ 中的业务模块。
- local_connector_client/core/migrations/ 中只服务业务数据的 migration。
- 前端 local/cloud 业务分流测试和实现。
- local_runtime_required 的遗留兼容代码。

保留或迁移：

- MCP manifest 与配置。
- Plugin/Skill 安装和本地执行状态。
- Plugin Credential Vault 与 OAuth。
- Workspace grant。
- Command approval 和审计设置。
- Sandbox pairing、权限、镜像和 lease。

建议先做“代码不引用检查”，再删除目录：

~~~text
rg "local_runtime|lc_project_|lc_session_|local_runtime_required"
~~~

数据库处理：

- 不自动删除旧 runtime.sqlite3，但运行时不再打开它。
- connector-state.sqlite3 只创建 Agent Prompt、能力快照和本地 MCP manifest 等控制表。
- 若产品允许用户重新配置 Plugin/MCP，也可以不做数据库迁移，但必须明确提示，不能误删安装包和凭据文件。
- 业务表物理清理应作为独立、可审计的维护动作。

预计：2～4 天。

---

## 9. 具体代码修改区域

### 9.1 ChatOS Frontend

| 区域 | 修改方向 |
| --- | --- |
| projectsFacade.ts | 删除本地业务 CRUD 分流，Local Workspace Project 走云端 |
| sessionsFacade.ts | 所有 Session/Task 走云端 |
| lib/api/localRuntime/ | 仅保留真正的设备控制接口，业务 API 退役 |
| sendMessage.ts | 删除本地 Turn、事件轮询和本地取消 |
| stopMessage.ts | 统一调用云端取消 |
| Memory/Review polling | 统一使用云端 Memory 与 Realtime |
| Project Plan/Requirement UI | 统一调用云端 Project Management |
| Task Board/Ask User | 统一调用云端 Task Runner |
| 文件/Git facade | 前端调用 ChatOS Backend，再由 Backend relay |

### 9.2 ChatOS Backend

| 区域 | 修改方向 |
| --- | --- |
| core/project_execution.rs | execution_plane 与 workspace provider 解耦 |
| api/local_connectors.rs | 创建云端 Project，显式 execution_plane=cloud |
| api/fs/local_connector_bridge.rs | 继续作为文件 relay |
| services/git/local_connector.rs | 继续作为 Git relay |
| services/code_nav/local_connector.rs | 继续作为代码导航 relay |
| Conversation Runtime | 允许 Local Workspace Project 在云端运行 |
| Model Runtime Resolver | 只使用云端模型凭据 |

### 9.3 Task Runner Service

| 区域 | 修改方向 |
| --- | --- |
| services/workspace_mcp.rs | 只解析任务能力选择，不保存或构造 Provider endpoint |
| services/sandbox_runtime/routing.rs | 按权威 Project 状态创建本地或云端 Sandbox，禁止跨 Provider fallback |
| services/model_runtime_resolver.rs | 不再阻断 Local Workspace Project |
| MCP Management gateway | 模型只连接聚合 endpoint，由 Runtime Session 冻结真实 Provider |
| Plugin/Skill policy | 只物化 Agent 的能力集合，真实 execution_host 由 MCP Management 路由 |
| Run cancellation | 向 Connector 传播本地 execution cancel |

### 9.4 Project Management Service

| 区域 | 修改方向 |
| --- | --- |
| environment_agent/runtime/analysis.rs | 移除 local_runtime_required gate |
| environment_agent/routing.rs | 保留并验证 LocalConnector provider |
| environment_agent/mcp_servers.rs | 使用 Local Connector file/sandbox MCP |
| Environment Agent persistence | 继续写云端 Project Management 数据 |

### 9.5 Local Connector Service

| 区域 | 修改方向 |
| --- | --- |
| Relay | 保留并补充幂等、状态查询和取消 |
| Internal Auth | token 绑定 caller、scope、path、device 与 TTL |
| Presence | 设备、Workspace、Plugin/Skill、Sandbox 在线状态 |
| Pairing | 保留 Project/Workspace/Sandbox binding |
| Store | 不保存 ChatOS 业务数据 |

### 9.6 Local Connector Client

| 保留 | 退役 |
| --- | --- |
| device registration | 本地 Project/Session |
| outbound WebSocket | 本地 Message/Turn/Event |
| Workspace grant | 本地 Chat Agent |
| file/Git/code navigation | 本地 Memory |
| terminal/command approval | 本地 Project Management |
| local MCP | 本地 Task Board/Task Runner |
| Plugin/Skill install and runtime | 本地 Environment Agent |
| Plugin OAuth/credentials | 本地业务 API |
| local Docker Sandbox | 本地模型作为核心 Chat runtime |

---

## 10. 安全边界

### 10.1 网络边界

- 客户端只主动连接云端，不开放公网监听。
- 本机 HTTP 服务继续只绑定 127.0.0.1。
- 云端到设备的所有请求必须经过认证的 WebSocket relay。
- 设备连接必须使用 device key 签名和用户身份绑定。

### 10.2 Workspace 边界

- 云端只发送 workspace_id 和相对路径。
- 客户端根据本地 grant 解析绝对路径。
- 拒绝绝对路径、..、非法编码和符号链接越界。
- Workspace grant 撤销后，所有新请求立即失败。
- Project binding 必须校验 owner_user_id、device_id、workspace_id。

### 10.3 内部 Token

内部 token 至少绑定：

~~~text
caller_service
owner_user_id
device_id
workspace_id
scope
request_path
expires_at
run_id
~~~

建议 TTL 为 30～120 秒，不允许跨 endpoint 复用。

### 10.4 命令执行与审批

- 云端只能调用已注册的 Terminal/Command tool。
- 客户端显示命令、工作目录、调用来源、风险级别和理由。
- 高风险命令继续在本机审批，云端不能绕过。
- shell、环境变量和 cwd 由客户端安全构造。
- 长命令必须有 process_id、超时、输出上限和 kill 能力。

### 10.5 Plugin/Skill 与凭据

- 本地 Plugin bundle 必须校验版本和哈希。
- Plugin 凭据和 OAuth Token 不返回云端。
- 云端只得到成功结果、错误摘要或 artifact 引用。
- execution_host 必须显式决定，禁止失败后静默换 host。
- 本地插件产物的读写权限按 Plugin、Workspace 和 Run 隔离。

### 10.6 模型凭据

- Chat、Task、Memory 和 Environment Agent 使用云端加密模型凭据。
- Local Connector 不再响应核心链路的 model_runtime_request。
- 不能为了兼容旧代码重新把本地模型 API Key 回传云端。

### 10.7 日志与可观测性

允许记录：

- run_id、tool_call_id、request_id。
- device_id、workspace_id。
- 工具名、耗时、状态码、输出字节数。
- 用户审批结果。

默认不记录：

- 本机绝对路径。
- 文件全文。
- 命令完整输出。
- Plugin 凭据、OAuth Token。
- 设备私钥。

---

## 11. 发布与混合版本兼容

### 11.1 推荐发布顺序

1. 先发布兼容新旧行为的 Local Connector Service。
2. 发布支持 Local Connector provider 的 ChatOS Backend、Task Runner 和 Project Management。
3. 在服务端保持 Feature Flag 关闭，完成集成测试。
4. 发布前端云端路由，但只对测试用户开启。
5. 发布仍保留 relay、同时可关闭 Local Runtime 的客户端。
6. 灰度开启 cloud_orchestration_for_local_connector_projects。
7. 观察一版后停止本地 Worker。
8. 再下一版删除本地业务代码和业务表。

### 11.2 兼容矩阵

| 云端 | 客户端 | 结果 |
| --- | --- | --- |
| 旧云端 | 旧客户端 | 当前模式 |
| 新云端，Flag 关闭 | 旧客户端 | 当前模式 |
| 新云端，Flag 开启 | 旧客户端 | 可使用旧客户端仍保留的 relay，需验证协议 |
| 新云端，Flag 开启 | 新轻量客户端 | 目标模式 |
| 旧云端 | 新轻量客户端 | 不支持，必须通过最低版本和发布顺序避免 |

因此不能先发布一个已经删除 Local Runtime 的客户端，再等待云端上线。

### 11.3 协议版本

客户端连接时上报：

~~~json
{
  "capability_version": 2,
  "features": [
    "workspace_mcp",
    "terminal_cancel",
    "plugin_relay",
    "skill_relay",
    "local_sandbox",
    "idempotent_tool_request"
  ]
}
~~~

云端在发起 Run 前做 capability preflight，不在执行一半时才发现客户端不支持。

---

## 12. 回滚策略

### 12.1 删除代码前

- 进入物理删除阶段后不再提供切回旧本地业务执行平面的 Feature Flag。
- 客户端只保留 Local Connector 能力 Runtime；旧 runtime.sqlite3 留在磁盘但不再使用。
- 新模式产生的云端 Session 不同步回本地；回滚期间它们仍保留在云端。
- 真实 Workspace 不受回滚影响。

### 12.2 停止 Worker 后

- 保留上一版完整客户端安装包。
- 服务端先关闭新 Project 创建，再决定是否回退路由。
- 已启动的云端 Run 完成或取消后再切换。
- 不尝试把云端业务数据灌回客户端。

### 12.3 删除代码后

- 回滚依赖上一版客户端二进制，而不是数据库逆向 migration。
- 旧 runtime.sqlite3 不自动删除；如需回滚，由上一版客户端继续读取。
- 不自动删除 Workspace、Plugin 包、凭据和 Sandbox 配置。

### 12.4 禁止的回滚方式

- 禁止整体 revert 0e9823d0。
- 禁止递归删除 ~/.chatos/local_connector。
- 禁止通过删除数据库文件来判断切换成功。
- 禁止在 Connector 失败时静默切 Cloud Sandbox 或 Cloud Workspace。

---

## 13. 测试矩阵

### 13.1 单元测试

- Project execution_plane 与 workspace_provider 判定。
- local://connector 路径编码、解析和绝对路径隐藏。
- Workspace 相对路径越界与符号链接检查。
- Internal token 的 caller/scope/path/device/TTL 校验。
- Relay request_id 幂等。
- Plugin/Skill execution_host 路由。
- Sandbox provider 路由。
- mutation 的 unknown_execution_state。

### 13.2 服务集成测试

- ChatOS Backend -> Local Connector Service -> fake Connector 文件读取。
- Task Runner -> MCP Management -> Local Connector 终端执行。
- Project Management Environment Agent -> Local Connector file MCP。
- Task Runner -> Plugin prepare/execute/cancel。
- Task Runner -> Local Sandbox create/execute/release。
- 云端取消 -> 本地进程停止 -> 云端终态。

### 13.3 客户端测试

- 启动后不创建本地 Project/Session/Turn。
- Chat 执行期间业务表无写入。
- Workspace grant 之外路径拒绝。
- 命令审批拒绝后云端收到结构化错误。
- Plugin 凭据不出现在 relay payload。
- Connector 断线自动重连并重新上报能力。
- Sandbox 孤儿 lease 被 TTL/reaper 回收。

### 13.4 E2E 场景

1. 新建 Local Workspace Project，发送消息，读取文件并修改文件。
2. 执行 Git status、diff 和受控 commit。
3. 创建 Requirement，云端 Task Runner 调本地代码工具完成任务。
4. 调用一个 local execution_host Plugin。
5. 调用一个本地 executable Skill。
6. 使用本地 Docker Sandbox 运行测试。
7. Chat 运行中关闭客户端，再重连。
8. 命令运行中点击取消。
9. 客户端离线时查看云端历史。
10. 同一账号的另一台设备打开 Project，但不能访问未授权 Workspace。

### 13.5 回归范围

- 纯 Cloud Project 的 Chat、Task、Memory、Environment Agent。
- Web 浏览器不具备本机 Connector 控制能力。
- 桌面 Surface CORS 与 IPC 安全。
- Plugin Management 当前策略和 availability。
- Cloud Sandbox。
- Project 删除不触及真实文件。

---

## 14. 验收标准

以下条件全部满足才可认为完成目标模式：

1. Local Workspace Project 的 Project、Session、Message、Turn、Task、Memory 和 Requirement 都在云端。
2. UI 不再依据 lc_project_*、lc_session_* 路由业务 API。
3. 客户端不运行 Chat Agent、Task Worker、Memory Worker 或 Environment Agent。
4. 客户端业务表在正常 Chat/Task 流程中没有新增写入。
5. 云端模型可以调用授权 Workspace 的文件、Git 和终端。
6. 本地 Plugin/Skill 能由云端 prepare、execute 和 cancel。
7. 本地 Docker Sandbox 能由云端选择、创建、执行和释放。
8. Connector 离线时返回明确错误或等待状态，不静默 fallback。
9. 云端不知道用户机器绝对路径。
10. 本地模型 API Key 不通过 Connector 返回云端。
11. 删除或归档云端 Project 不删除本地 Workspace。
12. 升级过程不删除 Plugin/Skill、OAuth、凭据或 Sandbox 配置。
13. Cloud Project 全链路无回归。
14. 客户端重启后，云端历史和 Run 状态仍可恢复。
15. Local Command Approval Agent 的模型循环、只读检查、人工审批、白名单、Session Approval 和审批历史完整保留在本机，且不调用云端 MCP 或 Memory Engine。
16. Agent Prompt 和工具参数不承担 Provider、设备、Workspace、Sandbox、Plugin 或 MCP 路由选择；这些身份由程序冻结并注入。
17. Runtime Session 与 Provider 调用按 owner、Agent、Project、run、turn、task 和来源消息隔离，任一绑定漂移都 fail closed。

---

## 15. 风险与应对

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| Task Runner 残留旧 Workspace endpoint 预路由 | Provider 决策重复、Sandbox 判定错误 | 已删除 Harness/Connector ephemeral endpoint 生成；能力选择与 Provider 路由分别由 Plugin Management 和 MCP Management 负责 |
| execution_plane 与 source_type 仍被混用 | 云端继续错误阻断 | 先完成模型语义和集中判定函数 |
| 旧前端与新客户端混用 | 请求不存在的本地 API | 严格发布顺序、最低客户端版本和 Feature Flag |
| Connector 断线时 mutation 状态不明 | 重复写文件或重复命令 | request_id、状态查询、只读自动重试 |
| 旧本地数据库同时保存业务和控制状态 | 清理时误删 Plugin/MCP | 2.0.10 使用全新的 connector-state.sqlite3 最小 schema，旧库不迁移也不自动删除 |
| 本地长命令取消不完整 | 云端显示取消但进程仍在 | 增加 process_id、kill tree 和取消确认 |
| Plugin policy 正在并行修改 | 合并冲突或策略回退 | 不做大范围 revert，基于当前 policy 适配 |
| 云端并发增加 | 成本和容量上升 | 提前压测 Chat、Task、Memory 和模型网关 |
| 用户以为“本地项目”仍代表本地推理 | 隐私预期错误 | UI 明确显示“云端 AI + 本地文件/工具” |

---

## 16. 建议的实际落地顺序

第一批提交只做语义和可运行闭环：

1. 引入 workspace_provider 与集中路由判断。
2. 新建 Local Workspace Project 时显式 execution_plane=cloud。
3. 前端 Project/Session/Message 统一走云端。
4. 移除 ChatOS Backend 的本地项目阻断。
5. Task Runner 只接入 MCP Management 聚合 endpoint，由网关路由 Workspace MCP。
6. 验证文件读取、终端执行和云端消息持久化。

第二批提交补齐生产能力：

1. Local Plugin/Skill routing。
2. Local Sandbox routing。
3. cancel、idempotency、offline/reconnect。
4. 安全测试与 E2E。
5. 客户端关闭本地 Worker。

第三批提交做清理：

1. 删除前端 Local Runtime 业务分支。
2. 删除客户端本地 Chat/Memory/Task/Project Management。
3. 拆分或收敛本地控制数据库。
4. 更新 README、打包清单和历史架构文档。

这个顺序能把风险集中在可观察的小步骤里，也允许在彻底删除本地执行代码前保留一个完整回滚窗口。

---

## 17. 最终判断

从代码基础看，恢复旧模式是可行的，而且长期维护成本会明显下降。

改动最大的部分不是 Local Connector 本身，而是：

- 取消前端大量 local/cloud 业务路由。
- 移除云端为本地执行平面增加的阻断。
- 恢复 Task Runner 的 Local Connector MCP/Sandbox routing。
- 从客户端 Runtime 中安全剥离 Plugin/MCP/Sandbox 控制状态。

不迁移历史数据后，不需要解决 ID 映射、消息顺序、Memory 合并、Task 状态恢复和双写一致性，项目风险从“大规模数据迁移”降为“中等规模运行时路由回迁”。

推荐采用两步交付：

1. 先在 1～2 周内完成云端闭环和客户端停写。
2. 再用一个后续版本删除约三万多行本地业务 Runtime，避免把上线切换和大规模代码清理绑在同一次发布中。
