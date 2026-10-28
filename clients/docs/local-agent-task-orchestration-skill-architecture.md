# Local Agent 任务编排与统一 Skill 架构改造方案

> 状态：Proposed
>
> 适用范围：shared Rust Local Agent Host、macOS Client、Windows Client、Plugin Management / Admin Console
>
> 最后更新：2026-10-12

## 1. 决策摘要

本方案解决两个相互关联的问题：

1. Main Chat 面对复杂需求时长期偏向只创建一个 Task，没有根据独立交付物、专业边界、能力边界和验收边界建立任务图。
2. Main Chat、Task Execution、Agent Group Chat、内置工具和 Plugin 各自使用不同程度的 Prompt、Skill 和能力发现机制，缺少统一的 Tool-to-Skill 绑定、激活、资源读取和运行时 gate。

目标状态如下：

- Main Chat 的八个 Task 业务工具保持不变。
- Main Chat 使用统一的产品 Skill runtime，不再把任务编排业务规则主要维护在 Managed Agent Prompt 中。
- 复杂需求按“独立可验收工作单元”建立 Task DAG；简单且上下文连续的需求仍使用单 Task。
- Product Skill、身份/项目 Skill 和 Plugin Skill 最终统一使用 `skill_ref`、统一激活工具和统一资源读取协议。
- Tool schema 负责结构，Skill 负责业务决策，shared Rust 负责权限、生命周期、持久化和强制 gate。
- macOS 与 Windows 使用同一份 Skill bundle、同一组绑定、同一 capability revision 算法和同一套行为评测。
- Plugin Management 当前“系统智能体”页面不能直接删除；应先拆走 Prompt 和能力策略职责，再下线当前混合页面。后台 capability resolution、binding 和 revision 服务仍需保留。

推荐模型可见工具结构为：

```text
8 个 Task 业务工具
+ 3 个通用 Skill 控制平面工具
  - agent_skill_activate
  - agent_skill_list_resources
  - agent_skill_read_resource
```

三个 `agent_skill_*` 是基础设施工具，不是新增业务能力。如果产品硬性要求模型总共只能看到八个函数，则只能把任务编排 Skill 设为 `runBound` 并全量注入；这种方案不具备真正的渐进披露，不作为长期推荐方案。

---

## 2. 当前实现与问题定位

### 2.1 当前消息和任务执行链路

Main Chat 当前链路为：

```text
用户消息
  -> Native Conversation Service
  -> start_conversation_turn
  -> shared Rust 创建 profile_key=main_chat 的 Run
  -> Planner 合并 Managed Prompt、capability snapshot 和本地 Task tools
  -> 模型直接回答，或调用 create_task/create_tasks_with_prerequisites
  -> shared Rust 创建 profile_key=task_execution 的 Task Run
  -> 原生客户端执行项目、终端、Plugin、远程连接、External MCP 工具
  -> Task 生命周期回写原会话
```

关键实现：

- `clients/local_agent_host/layers/application/src/conversation_runtime.rs`
- `clients/local_agent_host/layers/profiles/src/planner.rs`
- `clients/local_agent_host/src/application/task_tools.rs`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentBootstrap.swift`

### 2.2 Main Chat 当前八个 Task 工具

权威定义位于：

`clients/local_agent_host/src/application/task_tool_definitions.rs`

当前工具为：

1. `list_tasks`
2. `get_task`
3. `create_task`
4. `create_tasks_with_prerequisites`
5. `cancel_task`
6. `wait_for_task_completion`
7. `get_task_dependency_graph`
8. `resolve_task_followup`

`clients/local_agent_host/README.md` 仍写成 seven tools，应随本次改造修正。

### 2.3 为什么模型偏向单 Task

当前偏置来自四个方面。

#### 工具描述不对称

`create_task` 的描述把“需要读取项目、执行工具或进行持久化工作”全部纳入单 Task；批量工具则主要强调存在 prerequisite 的 investigation、implementation、review 阶段。因此模型自然把单 Task 当成默认入口。

#### 当前 Skill 过度限制拆分条件

`chatos-async-task-orchestration/references/capabilities-lifecycle-and-recovery.md` 当前要求只在存在真实 prerequisite edge 时拆分。它没有覆盖独立交付物、不同专业、不同 Plugin、并行 workstream、风险隔离、独立验收和方案变体。

#### 缺少分解决策字段与运行时证据

模型调用 `create_task` 时不需要说明为什么一个 Task 足够，也不需要声明预期产物和验收条件。运行时无法区分“合理的单 Task”和“未经分析的大包 Task”。

#### 缺少业务行为评测

现有测试主要验证 schema、DAG、依赖约简、scope 和生命周期，没有覆盖“开发、设计、文案、调研”等用户需求应该形成几个 Task、怎样建立依赖的问题。

### 2.4 底层已经支持任务图

当前 runtime 已具备以下能力：

- 每次最多创建 50 个 Task。
- 支持 `client_ref`、`prerequisite_refs`、`context_refs`、`owned_paths`。
- 每个 Task 独立选择模型、built-in capability、External MCP 和 Plugin。
- 图创建后 `start_ready_task_runs_for_graph` 会 materialize 当前全部 ready Task。
- prerequisite 成功后，后继 Task 会收到用户可见的前置结果。
- 冗余的传递依赖会被约简。

因此本次不是重写 Task runtime，而是补齐编排决策、Skill runtime、写入隔离、回调聚合和评测。

---

## 3. 目标架构

```text
Run-bound Policy Skill
  - 身份、语言、安全、真实性、必须遵守绑定 Skill
                     |
                     v
Frozen Skill Router / Catalog
  - 当前 Run 允许的 product/plugin/bound skill_ref
                     |
                     v
agent_skill_activate / list_resources / read_resource
                     |
                     v
Tool-to-Skill Gate
  - provider_id + skill_binding_id + activation_policy
                     |
          +----------+-----------+
          |                      |
          v                      v
Main Chat 8 Task tools     Task Execution tools
          |               project/terminal/plugin/...
          v
Shared Rust Task DAG + policy + persistence + callbacks
```

### 3.1 各层职责

| 层 | 职责 |
|---|---|
| Policy Skill | 自动加载的稳定身份、语言、安全边界、禁止伪造执行、要求遵循绑定 Skill；随 bundle 冻结 |
| Skill Router | 只披露当前 Run 可激活的 Skill 摘要和 `skill_ref` |
| SKILL.md | 稳定的业务决策算法、工具顺序、失败恢复、reference 路由 |
| Skill references | 行业场景、详细正反例、复杂 DAG 模式、恢复细节 |
| Tool schema | 参数类型、必填字段、能力枚举、闭合 JSON 结构 |
| shared Rust | owner/scope、权限、DAG、锁、持久化、Skill gate、handoff、callback |
| 原生客户端 | 本机项目/终端/UI/Plugin/External MCP 的真实执行和审批 |
| Plugin Management | 发布能力策略、Plugin 元数据、安装来源、版本和 checksum，不承载执行 |

### 3.2 Tool-to-Skill 绑定

新增稳定绑定：

```text
provider_id: chatos.local.main-chat-task
skill_binding_id: main-chat.task-orchestration
router_skill_name: chatos-async-task-orchestration
specialist_skill_name: chatos-async-task-orchestration
activation_policy: on_demand
```

该 binding 覆盖八个 Task 工具。所有八个工具必须通过统一 coverage audit。

Skill 控制平面自身使用：

```text
provider_id: chatos.local.skill-control-plane
skill_binding_id: local-agent.skill-control-plane
activation_policy: control_plane
```

控制平面工具不能要求先激活自身。

### 3.3 统一 Skill bundle

当前产品 Skill 的权威文件只位于 macOS Swift Package：

`clients/macos/Sources/ChatOSCore/AgentSkills/Skills`

这不适合作为跨平台权威来源。建议新增：

```text
clients/shared/agent_skills/
```

作为产品 Skill 的唯一源目录。macOS 和 Windows 构建均从该目录生成或嵌入资源，不允许复制两份可独立修改的 SKILL.md。

建议构建产物生成一份规范化 manifest：

```json
{
  "schema_version": 1,
  "bundle_revision": "sha256:...",
  "skills": [
    {
      "name": "chatos-async-task-orchestration",
      "role": "specialist",
      "instructions_sha256": "sha256:...",
      "resources": [
        {
          "relative_path": "references/software-engineering.md",
          "sha256": "sha256:...",
          "size_bytes": 1234
        }
      ]
    }
  ]
}
```

Skill bundle revision 必须参与 capability revision 计算。

---

## 4. Task Orchestration Skill 设计

### 4.1 文件结构

建议重构为：

```text
chatos-async-task-orchestration/
├── SKILL.md
└── references/
    ├── complexity-and-decomposition.md
    ├── graph-patterns-and-dependencies.md
    ├── capabilities-and-plugins.md
    ├── software-engineering.md
    ├── design-and-creative.md
    ├── writing-content-and-marketing.md
    ├── research-data-and-operations.md
    ├── documents-presentations-and-media.md
    ├── followups-cancellation-and-recovery.md
    └── acceptance-examples-and-antipatterns.md
```

### 4.2 `SKILL.md` 必须包含的核心规则

核心 Skill 负责稳定算法，不堆积所有行业例子：

1. 判断当前请求是否需要真实执行。
2. 判断无需 Task、单 Task 或 Task graph。
3. 按独立验收单元拆分，而不是按机械步骤拆分。
4. 为每个 Task 定义目标、产物、验收、能力、输入和边界。
5. 只为真实启动前提建立 `prerequisite_refs`。
6. 区分 prerequisite 与仅用于说明关系的 `context_refs`。
7. 需要统一交付时创建最终 synthesis/verification Task。
8. Plugin 和 built-in capability 最小化选择。
9. 处理历史任务、followup、取消、替换和 clarification。
10. 成功创建或调整后只调用一次 `wait_for_task_completion`。

### 4.3 拆分判定

#### 使用单 Task

同时满足以下条件时使用 `create_task`：

- 只有一个主要交付物。
- 工作上下文高度连续。
- 实现和验证属于同一责任单元。
- 所需 capability/Plugin 基本相同。
- 不存在需要独立报告的中间结果。
- 拆开会造成明显重复读取或状态丢失。

例如：

- 修复一个明确 Bug 并运行相关测试。
- 修改一篇文章的语气和错别字。
- 调整一个现有页面的配色。
- 检查一个模块并给出结论。

#### 使用 Task graph

出现任一强信号，或者同时出现两个以上弱信号时，优先使用 `create_tasks_with_prerequisites`。

强信号：

- 用户要求多个独立交付物。
- 存在多个可以分别验收的 workstream。
- 不同 workstream 需要不同 Plugin、权限或执行环境。
- 存在研究、生产、独立验收等真实责任边界。
- 存在多个方案变体，最后需要比较、选择或整合。
- 某个 workstream 失败不应使其他独立工作失效。
- 存在发布、审批、迁移、破坏性修改等高风险边界。

弱信号：

- 涉及多个子系统或多个互不重叠的产物区域。
- 需要多种专业判断。
- 同时涉及内容、视觉、代码或数据。
- 需要独立事实核查、合规检查或质量审查。
- 预期产生多个不同格式的产物。
- 必须先调查未知事实才能决定后续实现。

#### 禁止机械拆分

以下通常仍是一个 Task：

```text
读文件 -> 修改文件 -> 跑测试
```

它们是一个交付单元的执行步骤，不是三个独立成果。

默认建议创建 2–7 个 Task，50 继续作为运行时硬上限。不得为了表现“复杂”而制造空任务、纯转述任务或没有独立验收的阶段。

### 4.4 DAG 模式

#### 调研—生产—验收

```text
调查/证据
    -> 实现或创作
        -> 独立验收
```

#### 并行 workstream—整合

```text
共同约束调查
    +-> Workstream A -+
    +-> Workstream B -+-> 整合/回归
    +-> Workstream C -+
```

#### 方案变体—评估—定稿

```text
Brief/事实
    +-> 方案 A -+
    +-> 方案 B -+-> 比较与定稿
```

#### 高风险变更

```text
现状与回滚证据
    -> 变更
        -> 验证
            -> 发布/迁移（仅在已授权时）
```

### 4.5 业务场景示例

#### 软件开发

“实现完整功能，包括后端、前端和测试”：

```text
代码库与约束调查
   +-> 后端实现 --------+
   +-> 前端实现 --------+-> 集成与回归验证
```

只有当后端和前端修改范围可安全分离时才允许同时 ready；否则建立 prerequisite 串行执行。

#### 设计与创意

“设计完整活动页方案”：

```text
品牌/受众/需求梳理
   +-> 视觉方向 A ------+
   +-> 视觉方向 B ------+-> 方案整合与设计规范 -> 一致性/可访问性检查
   +-> 内容结构 --------+
```

#### 文案与营销

“产出官网、公众号和社媒发布文案”：

```text
产品事实与受众核查
    -> 核心信息架构
       +-> 官网文案 ------+
       +-> 公众号长文 ----+-> 事实、口径和风格统一审核
       +-> 社媒短文案 ----+
```

#### 调研与数据

```text
数据来源与质量检查
   +-> 定量分析 --------+
   +-> 定性分析 --------+-> 综合结论与建议
```

#### 文档、演示和媒体

```text
内容大纲与证据
   +-> 正文/讲稿 --------+
   +-> 图表与视觉资产 --+-> 组装 -> 渲染/版式/交付验收
```

---

## 5. 八个 Task 工具的 schema 调整

工具数量不增加，但创建工具应提供可验证的规划字段。

### 5.1 `create_task`

建议新增必填字段：

```json
{
  "expected_outputs": ["string"],
  "acceptance_criteria": ["string"]
}
```

建议新增可选字段：

```json
{
  "owned_paths": ["string"],
  "risk_level": "low|medium|high"
}
```

是否适合单 Task 由编排 Skill 根据交付边界判断，不要求模型额外提交或持久化“不拆分理由”；这类内部决策不应成为用户任务契约。

### 5.2 `create_tasks_with_prerequisites`

建议新增图级字段：

```json
{
  "graph_goal": "string",
  "decomposition_rationale": "string",
  "notification_policy": "progress_summary|all_tasks|milestones|graph_summary",
  "final_delivery_ref": "optional client_ref of the synthesis Task"
}
```

每个 Task 建议新增：

```json
{
  "workstream": "string",
  "expected_outputs": ["string"],
  "acceptance_criteria": ["string"],
  "risk_level": "low|medium|high"
}
```

其他调整：

- `tasks.minItems` 从 1 改为 2。
- `expected_outputs` 和 `acceptance_criteria` 至少各一项。
- `prerequisite_refs` 只表示无法提前开始的硬依赖。
- `context_refs` 不延迟调度，也不保证前置结果已产生，不能代替 prerequisite。
- 需要综合上游结果的最终 Task 必须依赖相关上游 Task。

### 5.3 Prompt 构造

`task_prompt()` 应结构化写入：

- Objective
- Workstream
- Expected Outputs
- Acceptance Criteria
- Constraints
- Input Data
- Resolved Prerequisites
- Project Inspection Evidence Contract
- Output Language Policy

不得只把验收信息埋在自由文本 description 中。

---

## 6. shared Rust Local Agent Host 改造

跨平台一致性必须以 shared Rust 为核心，不能把关键行为分别实现两次。

### 6.1 Skill bundle snapshot

建议新增独立的 durable `LocalSkillBundleSnapshot`，而不是把所有 Skill 正文直接塞入每个 capability snapshot。

建议字段：

```text
bundle_revision
schema_version
documents[]
  skill_ref
  name
  role
  description
  instructions
  instructions_sha256
  resources[]
    relative_path
    content
    sha256
tool_skill_bindings[]
  provider_id
  binding_id
  tool_names
  router_skill_ref
  specialist_skill_ref
  activation_policy
```

`LocalCapabilityPolicySnapshot` 增加：

```text
skill_bundle_revision
allowed_skill_refs
tool_skill_binding_ids
```

新增字段使用兼容性默认值，避免旧 snapshot 解码失败。

当前落地采用独立内部 capability profile 承载 bundle JSON，避免修改公开业务 policy 的兼容字段：Main Chat 使用 `skill_bundle_internal`，Task Execution 使用 `task_skill_bundle_internal`；都按 Run 冻结的 `capability_policy_revision` 查询。Task bundle 覆盖能力 broker、项目读写、终端命令、进程观察、进程控制六个 binding，并将每组工具与真实 Task catalog 求交集。

两个 bundle 各自按正文、资源、binding、activation policy 生成 checksum；bootstrap 将两个 bundle checksum 与实际渲染后的 prefixed catalog 一起计算 combined checksum，纳入 capability revision（精确算法见 11.4）。Task 初始上下文内联 run-bound 的能力发现 Skill，项目与终端 on-demand Skill 仍只发布短 catalog。能力发现 reference 可以通过相同 `agent_skill_*` 继续按需读取。

原生执行器通过只读 `get_run_skill_bundle { owner_user_id, run_id }` 获取 `{ type: "run_skill_bundle", run_id, bundle }`。Host 先校验 Run 的账户归属，再按其 profile/revision 返回固定 bundle；不存在 bundle 的旧 Run 返回 `null` 并保留兼容路径。已有 bundle 缺失 document/resource 时不得回退读取升级后的客户端资源。

### 6.2 Durable activation state

Skill 激活状态不能只保存在 Swift actor 或 C# 内存中。建议按 Run 持久化：

```text
run_id
skill_ref
activated_at
bundle_revision
instructions_sha256
```

恢复 Run 时必须继续使用创建时冻结的 bundle revision，不能静默读取升级后的 Skill。

### 6.3 通用 Skill 控制工具

在 shared Rust 注册：

- `agent_skill_activate`
- `agent_skill_list_resources`
- `agent_skill_read_resource`

要求：

- 只能访问当前 capability snapshot 允许的 `skill_ref`。
- 资源路径必须规范化并限制在 Skill manifest 中。
- 支持分页读取。
- 返回 checksum、offset、next_offset 和 truncated。
- 三个工具是 read-only、approval-exempt、control-plane。

### 6.4 Tool gate

在工具 invocation 进入本地或平台 executor 之前执行统一 gate：

1. 根据 tool name 和 provider 找到 binding。
2. `controlPlane`：直接允许。
3. `runBound`：确认 Run 创建时已自动绑定正确 revision。
4. `onDemand`：确认所需 Skill 已在当前 Run 激活。
5. 缺失时返回结构化错误和允许激活的 `skill_ref`。

gate 必须在 shared Rust 执行，不能只依赖 macOS/Windows provider 的提示文字。

shared Rust 现在会在 `claim_next_tool` 返回前执行同一 gate，并把拒绝编码为 claim 的 `preflight_outcome`。Rust 内置 worker 和 macOS native platform worker 都只提交这个结果，不调用真实 executor；因此 side-effecting 工具也不会在 Skill 未激活时先产生本机副作用。旧 Run 没有 bundle 时继续兼容放行。该通道只约束已进入当前 Run bundle binding 的工具；Plugin Skill 的动态 source 已通过 durable execution route 接入，Plugin/External MCP 业务工具本身的完整 bundle binding 仍需继续补齐。

### 6.5 多 Task 写入安全

shared protocol 现在会在 Task graph 创建时对 `CodeMaintainerWrite` 做 fail-closed 静态冲突检查：

- 没有 prerequisite 顺序关系的 `CodeMaintainerWrite` Task，其 `owned_paths` 不得相等或形成祖先/子路径重叠。
- 写 Task 未声明或提供空 `owned_paths` 时，保守地视为占用整个项目。
- `create_task` 和 graph 节点共用 `owned_paths` schema 与持久化字段，不仅多 Task 可以声明局部所有权。Windows 原生 Main Chat schema 需要同步发布该可选字段，缺省仍兼容全范围。
- 有直接或传递 prerequisite 顺序的写 Task 可以复用相同路径，因为它们不会同时 ready。
- `owned_paths` 必须是使用 `/` 的规范化项目相对路径；绝对路径、反斜杠、空组件、`.`、`..` 和 Windows drive path 都会被拒绝。
- 冲突 graph 创建会失败并要求模型声明不重叠范围或增加 prerequisite，不会静默并发写。
- 静态和运行时范围比较采用 NFD + Unicode lowercase 的保守 key，防止常见大小写、组合/分解 Unicode 路径在 macOS 默认卷上别名成同一目标；原路径仍用于模型和实际文件操作。
- macOS 原生 Task executor 从冻结 Run input 解析 `owned_paths`，将范围绑定进 edit session，并在暂存和提交时校验。只允许声明路径本身或其后代；删除范围外祖先、通过符号链接父目录创建目标、复用会话时更换根目录或省略范围均拒绝。失败 batch 不污染原有 staged 状态；缺省/空范围继续兼容全工作目录。

shared Rust scheduler 也已经实现跨 graph 的运行时写锁：

- `active_run_id` 非空且状态为 `ready` 或 `running` 的 `CodeMaintainerWrite` Task 持有其 `owned_paths` 范围；空范围继续按全项目锁处理。
- 选择 `TerminalController`、任意 Plugin 或任意 External MCP 的 Task 在运行期保守持有整个 project/conversation 范围。当前这些 capability 没有可由 shared Rust 强制执行的只读 effect 或资源范围，因此不能依据模型声明放宽。
- project conversation 以 project resource ID 为锁范围；普通 conversation 以 conversation ID 为锁范围。conversation 与 conversation_turn 两种 graph source 会归并到同一范围。
- 调度器遇到被锁住的队首 Task 时会继续选择后面的 reader 或无冲突 writer，避免 head-of-line blocking。
- Task Run 成功、失败、取消或过期 claim 被恢复后，现有 reconciliation 会清除 `active_run_id` 并释放锁；状态可由数据库在 Host 重启后继续恢复。

当前动态范围实现：

- Terminal、Plugin 和 External MCP Task 在运行期默认按整个 project/conversation 范围的 writer 串行化；该约束不会在 graph 创建时静态拒绝无依赖 Task，因此纯 reader 仍可绕过被锁住的队首 Task。
- Task 启动前已经根据冻结的 `tool_options` 取得保守锁，因此原生 Plugin 或 External MCP 在调用具体 leaf tool 时不会发生未加锁的范围扩张。
- malformed `plugin_hints` 或 `external_mcp_config_ids` 会 fail closed，不会绕过写锁。

后续可安全放宽的条件：

- Plugin/External MCP 发布可验证的 effect 和规范化资源范围，并由 native executor 强制执行该范围。
- shared Rust 能验证上述快照属于当前固定 Release/config revision；在此之前继续使用全范围锁。

### 6.6 回调聚合

shared Rust 与 macOS 现在已经实现图级通知策略：

- `progress_summary`：新批量任务默认。每个 Task 在实际 claim 开始执行时写入一条带任务名称的简短状态消息，结束时更新原消息；成功节点报告不进入进度消息，失败/阻塞附简短原因。graph 终态只交付一条完整结果。
- `all_tasks`：保持旧的逐 Task 回调行为。
- `milestones`：只写回标记了 `notification_milestone` 的重要 Task、失败、阻塞和最终汇总。
- `graph_summary`：内部 Task 进度只进入任务面板；聊天保留 Main Chat 的开始确认、仍在运行但需要用户处理的阻塞，以及最终综合结果。

新批量工具默认 `progress_summary`，并将策略显式冻结进每个 Task input；旧单 Task 默认 `all_tasks`。历史未声明策略的多节点 graph 继续按 `graph_summary` 解释，不修改历史契约。`graph_summary` 不再推荐作为默认，只在用户要求安静执行时显式选择。shared protocol 拒绝同 graph 内冲突策略。

`final_delivery_ref` 指定综合交付 Task 的 `client_ref`，持久化为该节点 `input.final_delivery=true`。只能有一个标记节点，它必须直接或间接依赖图内所有其他节点，否则 graph 创建失败且不产生任务。成功图交付这个节点的完整用户正文，不再截取420字；没有指定综合节点时，在一条最终消息内按节点交付所有可用完整报告。综合节点失败、取消或阻塞时，明确实际图状态并交付已成功节点的报告，不冒称综合完成。该标记只确定交付归属，不替代内容验收。

聚合回写使用现有 `local_task_graph_writebacks` generation/signature 做幂等：

- 所有必要 Task 成功时写入一个成功摘要。
- 某 Task 失败且阻断必要后继时，摘要列出失败和被阻塞节点。
- 独立分支失败但不影响其他交付时，继续其余分支，最终摘要明确部分成功。
- 用户取消根 Task 时保持现有级联语义。
- 摘要只使用经过既有 callback sanitizer 处理的用户可见结果，不写入 reasoning、凭证或内部运行细节。
- terminal signature 未变化时不会重复写回；失败 graph 被重试并产生新 terminal signature 时会创建下一 generation。

macOS 会把新聚合 payload 显示为一条普通 Main Chat 回复；优先非空 `delivery_text`，其次 `summary`，最后兼容旧 payload 展开。完整交付仅选取用户可见正文，不序列化整个 terminal outcome；保留 Markdown、代码、正文尾部，过滤凭证和内部标识，不做生命周期预览截断。每个 Run 的进度消息有稳定 ID、开始时间和结束时间，结束覆盖开始；同 Run 模型重试不重复发开始，用户重试产生新 Run 的独立状态消息，终态不会被重放的开始事件降级。

Windows 对齐字段、回调语义和验收用例见 [Task 进度与最终交付契约](local-agent-task-progress-and-delivery.md)。本轮只改 shared Rust 与 macOS，不改 Windows UI。

### 6.7 Handoff

`wait_for_task_completion` 仍是一次性后台交接信号：

- 创建或调整 Task graph 后调用一次。
- 成功后 Main Chat 清空工具。
- 只生成简短开始确认。
- 不在 Main Chat 轮询。
- 最终结果通过 graph/task callback 回写。

---

## 7. Plugin Skill 控制面收敛

模型可见的 Product Skill 与 Plugin Skill 控制面已经统一为：

- `agent_skill_activate`
- `agent_skill_list_resources`
- `agent_skill_read_resource`

旧 `capability_skill_activate`、`capability_skill_read_resource` 只保留为 executor 兼容 alias，不再发布到模型 tool definitions。

### 7.1 目标协议

统一 `skill_ref` 命名：

```text
product-skill:chatos-async-task-orchestration
product-skill:chatos-project-write
bound-skill:<profession-or-project-ref>
plugin-skill:<run-scoped-plugin-option>:<skill-name>
```

`capability_describe` 返回每个工具的 `required_skill_refs`，不再要求模型调用 Plugin 专用 Skill 激活工具。

### 7.2 Durable source 路由

同名 `agent_skill_*` invocation 不能再靠工具名判断由 Rust 还是原生客户端执行。SQLite v31 在每条 Tool Invocation 上持久化 `execution_route`：

- Main Chat 的 `agent_skill_*` 固定为 `shared`，读取冻结的 Product Skill bundle。
- Task Execution 的 `agent_skill_*` 固定为 `platform`，读取当前 Task 已冻结的 Plugin Release snapshot。
- Rust 内置 Task、Ask User、Notepad、Requirement Survey 和 Task Process 工具固定为 `shared`。
- 其他平台工具以及旧 `capability_skill_*` 兼容调用固定为 `platform`。

Rust worker 只 claim `shared`；外部 macOS worker 通过 Host 强制附加 `platform` route。Host 在 commit 时再次核对 route，因此两个 worker 不会竞争同名 invocation，原生 worker 也不能提交 Host-owned 工作。

Plugin provider 重建时调用 `get_run_skill_activations`，从当前 Run 已成功提交的 `agent_skill_activate`（以及兼容期 `capability_skill_activate`）invocation 恢复 `skill_ref`，再对固定 Plugin snapshot 恢复 session。这样 idle eviction、worker 重建和应用重启不会丢失 Skill gate 状态，也不需要另建一份原生 activation 数据库。

后续迁移顺序：

1. Windows 实现相同的 route-aware claim/commit、统一 provider 和 durable activation 恢复。
2. macOS、Windows 都完成兼容并覆盖旧 Run 恢复后，删除旧 `capability_skill_*` executor alias。
3. 删除前保留 wire compatibility，但不得重新把旧名称发布给模型。

Skill 激活只解释已经授权的能力，绝不扩大 Task、项目、Plugin、审批或文件访问权限。

macOS 已完成统一入口：`capability_describe` 在保留旧 `required_skills` 数据字段的同时返回统一 `required_skill_refs` 和带 `skill_ref` 的目录；capability broker 对模型只发布三个 `agent_skill_*` 工具，原生 provider 按 `plugin-skill:<plugin_option>:<name>` 解析当前 Run 固定 Plugin snapshot。旧工具名仍可由 executor 消费统一 ref 或旧 `plugin_option` + `skill_name` 参数，但只是兼容 alias。

---

### 7.3 动态 Plugin gate 与 External MCP（已实现 macOS/shared Rust）

新 Task 的 Product bundle 使用 schema 2，Main Chat 仍使用 schema 1。Plugin 的 Skill 文件继续属于固定 Release，不复制进 Product bundle。`capability_describe` 把固定 Release gate 转成 unified refs；成功结果作为当前 owner/Run 的权威 binding 事实存入原有 invocation ledger，不增加一份注册表或另一套 prompt。

```json
{
  "binding_schema_version": 1,
  "provider_id": "plugin:document-plugin-id",
  "plugin_option": "plugin_1",
  "tools": [{
    "tool_option": "tool_1",
    "invocation_mode": "capability_invoke",
    "required_skill_refs": ["plugin-skill:plugin_1:document", "plugin-skill:plugin_1:document-pdf", "plugin-skill:plugin_1:document-word"],
    "skill_gate": {
      "all_of": ["plugin-skill:plugin_1:document"],
      "select_by_argument": {"pointer": "/format", "map": {
        "pdf": "plugin-skill:plugin_1:document-pdf",
        "word": "plugin-skill:plugin_1:document-word"
      }}
    }
  }]
}
```

shared claim 在原生调用前检查最近一次成功 describe：必须属于当前 owner/Run 与同一 `plugin_option/tool_option`，且允许 broker 调用；按业务参数计算实际 required refs，检查 durable activation。未 describe 返回 `skill_binding_required`；无效 descriptor、缺失/非字符串/未映射选择参数返回 `skill_binding_invalid`；仅缺激活返回 `skill_activation_required`。只要求选中的业务分支，不要求激活所有格式。原生固定 Release/session gate 继续复核；idle eviction 或重启不会改变这些 durable 事实。

返回错误的工具不能成为成功事实：worker 把 `is_error/isError=true` 提交为 failed；shared 查询额外过滤旧错误 envelope，因此失败 activation/describe 不会解锁工具或污染恢复。抛出的副作用异常仍保持未知结果的 `needs_review` 语义。

External MCP 使用同一 Product Skill 控制面与静态 on-demand binding。bootstrap 将真实 public tool names 与当前 endpoint/tool 元数据 digest 写入冻结 bundle，Skill 正文/场景 references 通过三个 `agent_skill_*` 渐进加载。shared 先检查 activation；native 在 HTTP 前再检查冻结 binding、当前配置 digest 与成功 activation。仅 headers 变化不会影响 identity；远端工具定义或 endpoint 变化返回 `external_mcp_snapshot_changed`。纯 External Task 同样能加载 Skill，但未选择的 project/terminal 能力仍不可用。

当前 Gateway 只提供 toolSnapshot，没有可信远端业务 Skill 文件协议。因此 `chatos-external-mcp` 明确只提供通用调用、验收与恢复指导；原始 toolSnapshot 中的 `_meta["chatos/skillGate"]` 被保留并 fail closed，而不是被丢弃或假装由通用 Skill 覆盖。后续要支持远端业务 Skills，必须增加可信文档/resource snapshot 发布和版本绑定，而不是把远端任意返回文本直接当作指令。

---

## 8. macOS 实施清单

### 8.1 Skill 资源与 binding

- 将 `ChatOSCore/AgentSkills/Skills` 的权威源迁至共享目录或由共享目录生成。
- 在 `ProductToolSkillCoverageCatalog` 增加 Main Chat Task provider/binding。
- 八个 Task 工具必须通过 coverage audit。
- 将现有 `chatos-async-task-orchestration` 改造成本文定义的 Router + references。
- External MCP 使用相同 Product bundle 发布 `chatos-external-mcp` 与场景/恢复 references；每个远端配置生成覆盖真实 public tool names 的 on-demand binding。通用调用 Skill 不替代服务器业务 Skill 正文。

### 8.2 Bootstrap

涉及：

- `NativeLocalAgentBootstrap.swift`
- `NativeLocalAgentControlPlaneClient.swift`
- `NativeLocalAgentPlatformToolCatalog.swift`

要求：

- 发布 Skill bundle snapshot 或确认 shared Rust 内置 bundle revision。
- capability revision 加入 Skill bundle checksum。
- Main Chat snapshot 增加允许的 Skill refs 和 binding IDs。
- Main Chat/Task instructions 直接来自统一资源目录里的 minimal policy Skill，随对应 bundle 冻结；不再请求、解析完整远程行为 Prompt。
- 不再在 Prompt 中重复维护完整任务拆分规则。

### 8.3 平台工具执行

涉及：

- `NativeLocalAgentPlatformToolWorker.swift`
- `NativeAgentPluginToolProvider.swift`
- `NativeLocalTaskPluginCapabilityProvider.swift`

要求：

- 已接受 shared Rust gate 的 `preflight_outcome`；拒绝时不执行原生工具，只提交结构化失败。
- Plugin Skill 已接入统一 `agent_skill_*` source adapter；旧名称只作为 executor alias 保留。
- 重建 provider 时从 Host 的 `get_run_skill_activations` 恢复已成功激活的 Product/Plugin Skill；恢复的 Product refs 还必须与当前 Task 选定能力求交集。
- Product Skill 正文和资源通过 `get_run_skill_bundle` 使用 Run 的固定快照，分页 offset 与 Rust 一致按 Unicode scalar 计数；仅没有 bundle 的旧 Run 使用客户端内建资源。
- Task broker 的 built-in provider 仅提供能力描述，`invocation_mode=direct_task_tool` 要求模型直接调用描述中的原生工具名；`capability_invoke` 无条件拒绝 Task built-ins，实际文件操作、审批、edit-session 和终端 ownership 由 Task 原生 executor 处理。Plugin 仍返回 `invocation_mode=capability_invoke`。不能复用旧群聊 built-in 执行器完成 Task 操作。
- 不在 Swift 再实现一份 Main Chat Task Skill gate。

### 8.4 测试

- 扩展 `ToolSkillCoverageCatalogTests`。
- 增加 Main Chat Task tool coverage 测试。
- 增加 Skill activation、resource pagination、run resume 测试。
- 增加 capability revision 随 Skill checksum 变化的测试。
- 增加复杂场景 Task graph 行为评测。

---

## 9. Windows React 实施约束

新的 Windows 客户端位于 `clients/windows-react`，从 macOS 当前产品行为与 shared Rust Host 契约出发实现。已删除的 C# 客户端不作为迁移、兼容或测试基线。

### 9.1 架构边界

- React Feature 只依赖 application use case 和 domain model，不直接调用 Tauri `invoke`。
- Tauri typed commands 实现平台端口，并负责 Rust Host sidecar、Windows 凭据、ConPTY、文件选择器和系统集成。
- Main Chat、Task Execution、Skill bundle、capability revision、Run 恢复和审批状态以 shared Rust Host 为事实源。
- Windows React 消费与 macOS 相同的 Skill manifest、bundle revision、tool schema 和冻结 Run 契约，不复制 Prompt 或 SKILL 正文。
- UI 不保存另一份 Task/Run/Skill 状态，不直接访问 Host SQLite，也不暴露任意 Shell、文件或进程桥。
- 所有产品自有页面使用统一苹果主题，不引入 Fluent、WinUI 或 Material 默认视觉。

### 9.2 契约与安全要求

- Host lifecycle 必须经过 launch、handshake、synchronize、ready；reader/writer 任一退出即终止传输并回收 sidecar。
- 平台 Tool Worker 只 claim `execution_route=platform` 的 invocation，尊重 durable route 与 `preflight_outcome`，不得执行 shared invocation。
- Product/Plugin Skill 激活和资源读取使用 Run 固定快照；重启后从 Host 恢复，不以进程内集合代替持久事实。
- Plugin Release、binding、artifact hash 与权限在执行前 fail closed 校验；版本漂移要求创建新 Task。
- 项目写入、终端、Plugin 与 External MCP 遵循 shared scheduler 的范围锁，并在平台端再次校验 owned paths、工作目录与权限。
- 跨平台 DTO 由 Rust interface schema/fixture 验证，TypeScript 不手写一套含义不同的协议。

### 9.3 必须补齐的测试

- TypeScript、Tauri Rust 与 Host fixture 的协议一致性。
- Host 启动次序、版本不兼容、writer/reader 退出、崩溃重启与账号切换。
- Main Chat Task tools、Skill bundle checksum、activation 恢复与旧 Run 固定快照。
- shared/platform route 隔离，preflight 拒绝时平台 executor 调用次数为零。
- Plugin/External MCP binding 与 Release 漂移 fail closed。
- 写路径越界、reparse point、工作目录扩大、终端 ownership 和审批绕过全部拒绝。
- 多 Task graph、依赖、范围冲突、回写聚合与 macOS 行为一致。

---

## 10. Plugin Management“系统智能体”页面去留

### 10.1 结论

当前页面不能在本次改造开始时直接删除，但“系统智能体”这个混合页面最终可以下线。

需要区分：

- 页面可以重构或删除。
- Plugin Management 的 capability resolution、binding、Plugin/MCP 策略和 revision 后端不能删除。
- 其他系统 Agent 的 Prompt 版本管理目前仍在使用，不能因为 Main Chat 改用 Skill 就全部删除。

当前页面 `admin_console/src/modules/plugin-management/pages/SystemAgentsPage.tsx` 实际混合了两类职责：

1. Agent Prompt 版本状态和编辑入口。
2. MCP 的 `disabled/optional/required` binding、条件和 tool allow/block list。

Skill 架构只替代第一类中的“业务行为教学”，不会自动替代第二类权限策略。

### 10.2 Main Chat 和 Task Execution 的处理

#### `chatos_conversation_agent`

当前 seed 已明确删除它的 managed MCP bindings，因为 Main Chat 的 Task tools 来自 Local Agent Host：

`plugin_management_service/backend/src/seed/agent_bindings.rs`

因此：

- Admin Console 不应再给该 Agent 显示“配置 MCP”。
- 完成 Skill 迁移后，不再提供可编辑的完整行为 Prompt。
- 只保留代码/Bundle 管理的 minimal invariant prompt，或直接由 shared Local Agent bundle 提供。
- 如果没有其他 runtime 依赖该 registry row，可在后续迁移中把它改为只读 runtime profile，而不是可管理系统 Agent。

#### `local_agent_execution_agent`

macOS 和 Windows bootstrap 当前仍通过该 key 获取：

- required/optional built-in MCP policy
- Plugin choices
- External MCP choices
- required capability policy
- capability policy revision 基础输入

因此不能直接删除它的后台记录或 binding。应把 UI 中这部分迁移到新的“Runtime Capability Policies”页面。

### 10.3 建议的新后台页面划分

#### Runtime Capability Policies

负责：

- `main_chat`、`task_execution`、`command_approval` 等 runtime profile。
- MCP/Plugin/Skill 的 required、optional、disabled。
- 条件 binding。
- tool/component allowlist 和 blocklist。
- 当前 policy revision 和生效客户端版本。

这里展示的是能力策略，不再伪装成“编辑一个 Agent”。

#### Runtime Prompt Policies

保留仍然需要 Managed Prompt 的 Agent：

- command approval
- Memory Engine summary/rollup/repair 等服务端 Agent
- 其他尚未迁移到代码/Skill bundle 的系统 Agent

Main Chat 和 Task Execution 的业务流程不再在这里编辑。

#### Product Skill Bundles

第一阶段可以只读展示：

- bundle revision
- Skill 名称、role、checksum
- resources
- Tool-to-Skill coverage
- 客户端最低版本

Product Skill 的编辑和发布建议跟随代码评审及客户端发布，不建议恢复成后台任意文本编辑；否则 macOS、Windows 和 shared Rust 的行为无法保证可复现。

### 10.4 当前页面可删除的前置条件

只有以下条件全部满足后，才能删除 `SystemAgentsPage` 及其路由：

1. Main Chat/Task Execution 的行为规则已经迁移到版本化 Skill bundle。
2. `local_agent_execution_agent` 的 MCP/Plugin policy 已迁移到 Runtime Capability Policies 页面。
3. 其他 Agent 的 Prompt 编辑已迁移到 Runtime Prompt Policies 页面。
4. Runtime Preview 不再依赖从 System Agents 页面选择 Agent。
5. macOS 和 Windows bootstrap 已切换到新的 profile/policy API，或兼容旧 `agent_key` 后端。
6. 后端 capability resolution 和 Prompt bundle API 的调用者已经审计完成。

完成这些迁移后，可以删除“系统智能体”这个混合概念页面，但保留其拆分后的管理能力。

### 10.5 后端长期命名

建议逐步从：

```text
agent_key = local_agent_execution_agent
```

迁移为：

```text
runtime_profile = task_execution
capability_policy_key = local_task_execution
```

短期保留旧 key 做兼容映射，避免同时破坏 macOS、Windows、Local Connector 和 Plugin Management SDK。

---

## 11. Policy Skill 与行为 Prompt 边界

macOS 已将以下两个 minimal policy 作为统一 Skill 资源直接发布；不是把远程完整 Prompt 改名后继续注入。policy 与业务 Skill 一起冻结，但自动加载，不通过八个 Task 工具的激活 gate。旧 Run 仍使用其历史不可变 instructions，只有后续 Run 选择新 capability revision。Windows 必须对齐这一来源和迁移语义。

### 11.1 Main Chat policy Skill

权威源：clients/macos/Sources/ChatOSCore/AgentSkills/Skills/chatos-main-chat-invariants/SKILL.md。

只保留：

- 身份与职责边界。
- 跟随用户语言。
- 不声称未执行的操作。
- 真实项目执行必须使用当前绑定能力。
- 必须遵循当前 Run 的 Skill Router 和 Skill gate。
- 安全、权限和项目边界以 runtime 为准。

删除或迁移到 Skill：

- 单 Task/多 Task 判定。
- 依赖图模式。
- followup 业务流程细节。
- capability 选择教程。
- 场景正反例。
- handoff 具体工具顺序。

### 11.2 Task Execution policy Skill

权威源：clients/macos/Sources/ChatOSCore/AgentSkills/Skills/chatos-task-execution-invariants/SKILL.md。

保留：

- 只在当前绑定项目和授权范围执行。
- 不伪造工具证据。
- 遵循当前 Task 的目标、产物和验收。
- 必须遵循绑定 Skill。

项目读取、写入、终端、Plugin、进度和 UI 自动化的详细流程分别由对应 Product/Plugin Skill 提供。

### 11.3 Prompt 与 Skill 不得双写

同一条业务规则只能有一个权威来源：

- Skill 业务规则不再复制进 Managed Prompt。
- Tool description 只保留调用语义，不复制完整 workflow。
- Rust 注入只提供程序事实和不可绕过的生命周期规则。

### 11.4 revision 与迁移契约

新 macOS 发布使用以下无密钥身份计算；Windows 需用相同 UTF-8 内容、排序和转义语义（对象 key 排序，不转义 `/` 与普通非 ASCII 字符；数组保持发布顺序，整数不得序列化成 `600.0`）：

1. 单个 Product bundle checksum 保持现有算法：`bundle_revision=unversioned` 时对完整 bundle 的 sorted-key JSON 计算 SHA-256，再构造 `sha256:<hex>` revision。policy document、正文 SHA 与资源也参与。
2. `catalogBytes` 是 `[main.prefixedInputItem, task.prefixedInputItem]` 的 sorted-key、without-escaping-slashes JSON 字节；`combinedSkillChecksum = "sha256:" + SHA256(main.checksum + "\n" + task.checksum + "\n" + Base64(catalogBytes))`。Base64 不换行。catalog 模板改动也必须改变 identity。
3. 在线 capability identity 是如下对象的 sorted-key JSON：`revision_schema="product-skills-v1"`、`base_revision=<Gateway policy revision>`、`skill_bundle_checksum=<combined>`、`main_tools=<实际 Main schema array>`、`task_tools=<实际 Task schema array>`、`task_policy=<required policy object>`。revision 为 `local-agent-<SHA256(identity)>`。模型选项标题、工具 schema、迭代上限、required bindings 与 Plugin Release 改动都必须形成新 revision；不能只哈希选项 ID。required policy 各 ID 数组排序，policy 保存时也使用上述稳定 JSON 编码。
4. 离线迁移 revision 为 `skill-policy-<SHA256(baseCapabilityRevision + NUL + "product-policy-v1" + NUL + combinedSkillChecksum)>`。比较当前 policy instructions、完整 bundle、替换 catalog 后的 prefixed items，全部一致且已有 required policy 时不生成新 revision。迁移前按 owner/profile/revision 校验五个旧快照；合法 External binding 必须原样保留，并与 Task 实际远端工具名集合一致。
5. 先发布两个 bundle、required policy 和 Task Execution，最后发布 Main Chat 作为可恢复的完整 revision 标记，再保存模型到 revision 的指针。任何中途失败都保留旧指针，重试使用相同内容和 revision；旧 Run 不变。

跨平台哈希验收向量：

```json
{"base_revision":"gateway-1","main_tools":[{"description":"original","name":"tool"}],"revision_schema":"product-skills-v1","skill_bundle_checksum":"bundle-1","task_policy":{"max_iterations":600},"task_tools":[]}
```

对上述精确 UTF-8（无末尾换行）得到 `local-agent-7b9362e6c50fed7455504a444765a09897e50e0d95fd9e4ed80f43f6e64c8f33`。macOS 测试已有固定向量，Windows 不应只验证两次本平台输出相等。

离线恢复只保留远端工具 schema 和 fingerprint，不持久化认证 headers；如果当前没有可信的 transient endpoint/credential config，native 在 HTTP 前返回确定性的 `external_mcp_runtime_unavailable`，要求刷新配置，不能假装离线已恢复远端执行能力，也不能按未知副作用重复执行。

---

## 12. 测试与行为评测

### 12.1 单元和集成测试

必须覆盖：

- Skill catalog 完整性、frontmatter、checksum、资源路径。
- Tool-to-Skill coverage 100%。
- provider/binding/tool 三者匹配。
- on-demand gate 前失败、激活后成功。
- control-plane 工具不递归要求激活。
- runBound 在 Run 创建时冻结。
- capability revision 包含 Skill bundle revision。
- Run 重启、客户端重启、离线恢复行为。
- macOS/Windows capability snapshot parity。
- Task DAG、依赖约简、外部前置和取消级联。
- `owned_paths` 冲突和默认全项目写锁。
- graph callback aggregation。
- `wait_for_task_completion` 后工具清空。

### 12.2 编排行为 eval

建议最少建立：

- 10 个无需 Task 的直接回答场景。
- 10 个应当单 Task 的执行场景。
- 20 个应当创建 2–7 个 Task 的复杂场景。
- 10 个 followup/cancel/replace/clarify 场景。

覆盖行业：

- 软件开发
- 设计与创意
- 文案与营销
- 调研与数据
- 文档与演示
- 音视频/媒体
- 桌面应用自动化
- 外部服务与 Plugin

每个 eval 不只检查工具名，还检查：

- Task 数量范围。
- 每个 Task 是否有独立产物和验收。
- prerequisite 是否真实、是否缺失或过度。
- 是否存在必要的最终 synthesis/verification 节点。
- capability/Plugin 是否最小化。
- 是否存在同时 ready 的写入冲突。
- 是否把普通执行步骤过度拆分。
- handoff 是否只调用一次。

### 12.3 运行指标

可运行评测入口已补齐，详见 [编排评测运行说明](local-agent-task-orchestration-evaluation.md)：50 个场景、生产 macOS 冻结快照导出、shared Rust 真实工具/SQLite 检查、离线正反例，以及显式 opt-in 的真实模型规划评测。离线回放不代表模型拆分质量，真实模型报告仍需人工语义验收。Windows 应按该文档导出同契约生产快照后复用相同评测入口。

上线后记录不含用户敏感内容的聚合指标：

- 单 Task 与 Task graph 比例。
- 每个复杂请求的平均 Task 数。
- graph 完成、部分失败、阻塞、取消比例。
- 因写路径冲突被拒绝或串行化的次数。
- followup 导致 continue/cancel/clarify/replace 的比例，以及 replace 从冻结到替代图提交的成功率。
- graph callback 数量和用户主动打开任务详情的比例。

指标用于发现过度拆分或仍然偏向单 Task，不能用于绕过用户权限或收集任务正文。

---

## 13. 分阶段迁移

### Phase 1：修复单 Task 偏置

- 重写 `chatos-async-task-orchestration` 及 references。
- 调整两个创建工具的 description 和 schema。
- 增加 expected outputs、acceptance criteria 和 decomposition rationale。
- 新增跨行业编排行为 eval。
- 修复 README 八工具数量。

这一阶段可以先使用 `runBound` 注入，快速验证任务图行为，但不得作为最终渐进披露架构。

### Phase 2：shared Rust Skill runtime

- 建立共享 Skill bundle 和 manifest。
- 增加 durable bundle snapshot、activation state 和通用 `agent_skill_*`。
- 建立 Main Chat Task Tool-to-Skill binding 和 gate。
- capability revision 纳入 Skill checksum。
- macOS、Windows 消费同一协议。

### Phase 3：多 Task 安全与回调体验

- 已实现 `CodeMaintainerWrite` 的 graph 静态 `owned_paths` 冲突检测，以及同 project/conversation 范围内的跨 graph 运行时写锁。
- 已将 Terminal、Plugin 和 External MCP 作为运行期全范围 writer 纳入同一 durable Task 锁；未知 effect 不再允许两个 native writer 并发，reader 仍可绕过等待队首。
- 已实现 `progress_summary`、`all_tasks`、`milestones`、`graph_summary` notification policy；新批量任务默认 `progress_summary`，历史策略不改写。
- 已实现每 Run实际开始回调、终态原消息更新，以及幂等的 graph 完整结果单条交付；最终综合节点由可选 `final_delivery_ref` 标记。
- 已验证失败、取消、运行中阻塞、依赖阻塞、部分成功和失败后重试产生新 generation 的路径。

### Phase 4：Plugin Skill 控制面统一

- macOS `capability_describe` 已返回统一 `required_skill_refs`，同时保留旧 `required_skills` 兼容字段。
- macOS 原生 Plugin runtime 已把统一 `skill_ref` 解析到当前 Plugin Release 固定 snapshot；模型可见入口已经统一为三个 `agent_skill_*`，兼容期旧 `capability_skill_*` 仅由 executor 接受新 ref 和旧参数。
- macOS bootstrap 已把允许用于 Task 的 Plugin `plugin_key/plugin_id/release_id/version/artifact_sha256` 写入内部 capability policy，且这些字段参与 capability revision。shared Rust 只把当前 Task 实际选择的 Release 快照冻结到 `tool_options.plugin_release_snapshots`；native executor 启动 Plugin 前再次核对本机安装记录，Release 或 artifact 已变化时 fail closed，旧 policy/旧 Task 继续走兼容验证路径。
- shared claim path 已把 Skill gate 决策传给 macOS native worker；preflight 拒绝不会进入真实平台 executor。
- SQLite v31 已在 invocation 上持久化 `execution_route`；Main Chat 的 `agent_skill_*` 走 shared Product Skill source，Task Execution 的同名工具走 platform Plugin snapshot source，Rust/macOS worker 不再竞争同名 invocation。
- macOS provider 已通过 `get_run_skill_activations` 从成功 invocation 恢复 Plugin activation，覆盖 idle eviction 和应用重启。
- Windows 对齐后最终移除 `capability_skill_*` executor alias。
- Task Execution built-ins 和 Plugins 使用同一 gate 模型。

### Phase 5：Plugin Management 页面重构

- 新增 Runtime Capability Policies。
- 迁移仍需管理的 Runtime Prompt Policies。
- 增加只读 Product Skill Bundle/coverage 页面。
- 下线 Main Chat/Task Execution 的完整 Prompt 编辑。
- 前置条件满足后删除当前 System Agents 混合页面。

---

## 14. 验收标准

本方案完成需要同时满足：

1. Main Chat 八个 Task 工具全部具有稳定 Tool-to-Skill binding。
2. 未激活 on-demand 编排 Skill 时，创建/取消/followup 工具调用被运行时拒绝。
3. macOS 与 Windows 使用同一个 Skill bundle revision 和同一 Task schema。
4. 复杂开发、设计、文案场景能够稳定创建合理的 2–7 节点 Task graph。
5. 简单任务不会因为新规则被机械拆分。
6. 每个 Task 都有明确产物、验收和最小能力。
7. 同时 ready 的写 Task 不会无保护修改重叠范围。
8. 多 Task 默认不会向主聊天写入大量内部生命周期消息。
9. Plugin Skill 和 Product Skill 最终通过统一 `agent_skill_*` 激活。
10. Prompt、Skill、Tool description 和 Rust 注入之间不存在同一业务规则的多份权威副本。
11. Plugin Management 当前 System Agents 页面下线前，其 capability policy 和其他 Agent Prompt 管理职责已有明确替代入口。
12. capability/Skill revision、离线恢复和旧 Run 恢复均有跨平台测试。

---

## 15. 主要代码触点

### shared Rust

- `clients/local_agent_host/src/application/task_tool_definitions.rs`
- `clients/local_agent_host/src/application/task_tools.rs`
- `clients/local_agent_host/src/application/task_tool_support.rs`
- `clients/local_agent_host/src/application/skill_tools.rs`
- `clients/local_agent_host/src/application/tool_scheduler.rs`
- `clients/local_agent_host/src/application/assembly.rs`
- `clients/local_agent_host/layers/application/src/skill_runtime.rs`
- `clients/local_agent_host/layers/interface/src/skill.rs`
- `clients/local_agent_host/layers/profiles/src/planner.rs`
- `clients/local_agent_host/layers/profiles/src/planner_tools.rs`
- `clients/local_agent_host/layers/interface/src/control_plane.rs`
- `clients/local_agent_host/layers/infrastructure/database/src/capability_snapshot_store.rs`
- `clients/local_agent_host/layers/infrastructure/database/src/task_conversation_writeback.rs`

### macOS

- `clients/macos/Sources/ChatOSCore/AgentSkills`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentBootstrap.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentSkillBundle.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentPlatformToolCatalog.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentPlatformToolWorker.swift`
- `clients/macos/Sources/ChatOSConnector/NativeAgentPluginToolProvider.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatProgressiveSkillTools.swift`

### Windows

- `clients/windows-react/src/application`
- `clients/windows-react/src/infrastructure`
- `clients/windows-react/src-tauri/src/host`
- `clients/windows-react/src-tauri/src/commands`

### Plugin Management / Admin Console

- `plugin_management_service/backend/src/api/runtime_capabilities.rs`
- `plugin_management_service/backend/src/seed/agent_bindings.rs`
- `plugin_management_service/backend/src/seed/agent_prompts.rs`
- `admin_console/src/modules/plugin-management/pages/SystemAgentsPage.tsx`
- `admin_console/src/modules/plugin-management/pages/agentPrompts`

---

## 16. 最终原则

本次改造不是让模型“更喜欢批量建任务”，而是建立可解释、可验证、可恢复的统一决策链：

```text
识别用户场景
  -> 激活任务编排 Skill
  -> 判断无需 Task / 单 Task / Task graph
  -> 按独立验收单元拆分
  -> 建立最小依赖和最小能力
  -> runtime 校验权限、冲突和生命周期
  -> graph 级聚合结果
  -> eval 持续防止退化
```

Skill 负责教会模型如何工作；Rust runtime 负责保证它不能越权或破坏状态；原生客户端负责真实执行；Plugin Management 负责发布可信能力策略。四者职责必须分开，但共享同一份版本化 Skill 和 binding 事实。

---

## 17. 当前实现状态（2026-10-12）

macOS/shared Rust 第一、二阶段的 Task 编排主链路已经落地：

- `chatos-async-task-orchestration` 已改为完整的 router-style Skill，包含 10 个按场景渐进披露的 reference。
- `ProductToolSkillCoverageCatalog` 已为八个 Main Chat Task 工具增加稳定的 `main-chat.task-orchestration` on-demand binding；三个 `agent_skill_*` 工具使用独立的 control-plane binding。
- shared protocol 已定义版本化 Skill bundle、document、resource、Tool-to-Skill binding 和 activation policy；macOS 用 `skill_bundle_internal` profile 按 Main Chat capability revision 发布不可变 bundle snapshot。
- Task Execution 独立发布 `task_skill_bundle_internal`，shared gate 按 Run profile 选择 bundle。两个 checksum 共同参与 capability revision/离线缓存；Task 初始上下文注入 run-bound 能力发现正文并保留项目/终端 on-demand catalog。
- macOS Task broker 使用只描述的 built-in provider，支持项目和联系人工作目录；描述只包含当前 Task catalog 和已选择的基础能力。built-ins 通过 `direct_task_tool` 进入 Task 原生 executor，broker 无法绕过 Task 的审批和终端隔离；Plugin 继续通过 broker 调用。
- `get_run_skill_bundle` 已提供账户隔离的固定 bundle 查询，native Product Skill activation、manifest 与 resource pagination 均使用创建 Run 时的 snapshot；成功 activation 从 durable invocation 恢复，并过滤未选择能力和未知 refs。
- macOS 原生文件编辑器已强制执行冻结 `owned_paths`，从 Task executor 传入并绑定到 edit session；stage/commit 和会话复用均校验范围，符号链接父目录与范围外祖先删除被拒绝。shared 静态/运行时锁比较同时覆盖常见 case 与 Unicode normalization 别名。
- macOS Main Chat 初始上下文只发布最小 Skill catalog，不再内联完整 Task Skill。模型调用 `agent_skill_activate` 后获得 `SKILL.md`，再通过 resource manifest 按需分页读取 10 个场景 reference。
- 完整 Skill、资源内容、binding 和 activation policy 的哈希会形成 bundle revision，并继续参与 Main Chat capability revision 与离线缓存失效判断。
- shared Rust 已实现 `agent_skill_activate`、`agent_skill_list_resources`、`agent_skill_read_resource`，并将成功的 activate invocation 作为当前 Run 的 durable activation 事实；进程重启后无需单独恢复表。
- shared tool claim path 会在执行任何已绑定工具前统一检查 Skill gate，并以 `preflight_outcome` 把拒绝传给 Rust Host worker 或 macOS native worker。Task Skill 未激活时返回确定性的 `skill_activation_required` 且不调用真实 executor；control-plane/run-bound 工具直接放行；没有 bundle 的旧 Run 保持兼容。
- shared Rust 与 macOS fallback schema 已要求单 Task 提供 `expected_outputs`、`acceptance_criteria`；是否不拆分由编排 Skill 判断，不要求提交内部“不拆分理由”。Task graph 提供 `graph_goal`、`decomposition_rationale`，且至少包含两个 Task。
- shared protocol 已对 unordered `CodeMaintainerWrite` Task 强制执行 `owned_paths` 冲突检查；空范围按全项目处理，重叠 writer 必须改为不相交范围或增加 prerequisite。shared Rust scheduler 还会在同一 project/conversation 范围内跨 graph 阻塞重叠 writer，并把 Terminal、Plugin、External MCP Task 保守视为全范围 writer，同时允许后续 reader 或无冲突 writer 越过被阻塞的队首 Task；terminal/cancel/过期 claim reconciliation 会释放锁。Rust/macOS schema 与 Task Skill 已同步说明该约束。
- 新多 Task graph 默认使用 `progress_summary`，worker实际claim时逐节点回调、终态更新原消息；最后使用 `delivery_text` 交付完整综合结果，而不是节点截断预览。`graph_summary/milestones/all_tasks` 保持兼容。generation/signature 保证重试前后幂等，macOS完整交付保持单条回复，旧graph payload继续兼容展开。详见 [Task 进度与最终交付契约](local-agent-task-progress-and-delivery.md)。
- graph notification 回归测试已覆盖全部成功、部分成功、失败导致后继阻塞、运行中 blocker、整图取消，以及失败后重试产生第二代摘要。
- macOS 能力 broker 已发布统一 `required_skill_refs`；产品 Skill 与当前 Run 的 Plugin Skill 使用同一 ref 选择逻辑，模型只看到三个 `agent_skill_*`。旧 `capability_skill_*` 仅作为 executor 兼容 adapter 消费这些 ref。
- shared Rust 已用 SQLite v31 `execution_route` 持久化 Skill source 路由：Main Chat 的统一工具由 Rust Product Skill runtime 执行，Task Execution 的统一工具由 macOS Plugin snapshot provider 执行。route-aware claim/commit 防止两个 worker 竞争同名 invocation。
- macOS Plugin provider 会通过 `get_run_skill_activations` 从成功的 activation invocation 恢复已激活 ref，idle eviction、provider 重建和应用重启不会清空 Plugin Skill gate。
- macOS Task policy、shared Rust Task materialization 和 native Plugin executor 已形成 Plugin Release 冻结链：policy 发布允许 Release，Task 只持久化实际选择项，执行前按 release/version/artifact hash 复核；插件升级不会静默改变已创建 Task 的工具或 Skill 内容。Windows 需使用同名 `plugin_release_snapshots` 字段和相同 fail-closed 语义对齐。
- Task 执行 prompt 与持久化/查询结果已包含产物和验收契约。
- shared Task planner 已按有效 builtin/Plugin/External 选择暴露六个 broker/统一 Skill 控制工具；只选择 builtin 的 Task 不再丢失发现入口，`agent_skill_*` 不再被过滤，旧 alias 不发布给模型。
- schema 2 Plugin 业务 gate 已由 durable describe + activation 事实进入 shared preflight，条件选择、错误 envelope 排除和重启恢复均有回归测试。native provider assembly 与模型 broker 已分文件维护。
- macOS External MCP 已提供产品 Skill 的场景/验收/恢复 references、按配置冻结的 on-demand binding、端点/工具定义 digest 校验和纯 External Task 的 Skill 访问。远端业务 gate 没有可信文档时在 HTTP 前被拒绝。
- macOS Main/Task instructions 已直接来自统一目录里的两个短 policy Skill，冻结到对应 Product bundle；在线 bootstrap 不再读取远程完整行为 Prompt。离线 restore 和 catalog 缺失 fallback 支持只创建新 revision 的幂等迁移，保留能力策略与 Remote fingerprint，旧 Run 不变。catalog rendering、实际 tool schemas 与 required policy 也计入 immutable revision；配套测试覆盖中断重试、模型去重、owner/revision 隔离和跨平台哈希向量。
- Rust Host、durable activation/restart、Unicode resource pagination、旧 Run 兼容、macOS coverage、bundle 编码/bootstrap schema 与状态 round-trip 定向测试已覆盖以上行为。
- 上一批验证：shared Rust Host/runtime/protocol/storage/profiles 共 267 项测试通过；macOS Connector/Core/External MCP 定向测试共 45 项通过；App 与 Connector/Core 测试 target 构建通过。测试涵盖真实本地 stdio Plugin 的条件 gate 描述、durable IPC preflight、错误 activation/describe 排除、纯 External Skill 访问、HTTP 前冻结校验与 credential rotation。
- policy Skill 迁移批次：macOS 67 项定向测试通过（Connector XCTest 44、状态 Swift Testing 10、Core 7、External MCP 6）；shared Rust profiles 30 项通过；App、Connector/Core 测试 target 构建通过，两个新 Skill validator 与 scoped whitespace/fmt 检查通过。尚未运行真实模型行为 eval，不能将这些结果表述为复杂用户需求拆分质量已验收。
- 编排评测批次：新增 50 场景、macOS 生产冻结快照导出和 shared Rust 隔离评测入口；8 个离线正例、8 个拒绝/恢复/检查器控制样例通过，评测器 4 项单元测试通过。shared Rust 全套 267 项重新通过；macOS catalog/迁移 16 项与导出 1 项通过。修复了纯图校验拒绝误进 unknown-result review，失败后可以修改图并继续。完整参考阅读必须在决策前一个模型 step 已返回，不能用同轮并发读写冒充渐进披露。
- 准备/写入边界批次：Task 工具在首个写入前的契约、能力、模型、依赖、取消参数、追问候选错误现在返回已知 `Failed`，可以修正后继续；进入创建/派发/取消/评审事务后仍保守处理未知结果。单 Task 也发布并持久化 `owned_paths`，与图节点共用 schema 和写入约束；创建 DTO 拒绝额外字段和模型伪造 Release。required policy 只有无 Skill bundle 的旧 Run 明确 `NotFound` 时才走默认；现有空/损坏策略、存储故障或新 Run 缺策略均 fail closed。故障注入验证图已提交而派发失败不会被误判为无副作用。shared Rust 全套 274 项、评测器 5 项、macOS catalog/迁移/导出 17 项通过，App 构建通过；离线正例 8 个、控制样例扩展至 11 个，仍不代表真实模型质量已验收。

- FinFlow 实机验证保留2026-10-12两轮历史，又在2026-10-13完成新的4 Task复杂验收：Main完整读取编排参考后自动分工，四项实际开始/结束事实、原状态消息更新、综合前置依赖与单条9312字符全文交付均有durable及UI证据；项目9文件SHA-256不变。旧轮使用 `graph_summary` 的节点简述局限不倒算修复，新轮使用 `progress_summary` 已在主会话直接交付全文。终端 `common` 歧义也已修复，canonical `command` 共用审批/执行解析。详见 [FinFlow 实机验收记录](local-agent-finflow-integration-test.md)。这不是全量场景拆分质量、并发写冲突或财务生产准入验收；旧worker调度的串行与早Run优先排队问题尚未优化。

- 2026-10-13补齐项目Main Chat直接需求调研：自动policy及编排入口先判断关键未知项，再按需激活统一 `chatos-requirement-discovery`，读取场景reference、创建问卷并等待提交。原8个Task工具不变，Survey作为独立companion。shared Host将答案回填原函数调用并恢复同一Main/Task Run；macOS主聊天提供通知和入口，调研中心共用项目存储及事件刷新。FinFlow真实模型两题问卷已完成创建、等待、提交、原Run恢复，未创建Task或改项目文件。Windows发布、binding及UI对齐要求见 [主聊天与需求调研契约](local-agent-main-chat-requirement-discovery.md)。

尚未完成、不得误报为已落地的部分：

- 真实模型全量行为评测与人工语义验收尚未执行；当前已具备 50 场景和可运行的离线/真实模型入口，离线结构通过不能作为业务拆分质量通过。

- Plugin/External MCP 基于可信 effect 与资源范围的细粒度动态锁，以及完整跨平台 eval。macOS/shared Rust 已先对 Terminal、Plugin、External MCP 按整个 project/conversation 范围实现保守运行时锁；在 native executor 可强制执行细粒度范围前不提供 read-only 例外。
- Windows 的 Skill bundle 发布、binding/bootstrap/schema 对齐；Windows 开发应按本文件相同 ID、字段和哈希语义实现。
- External MCP 服务自有业务 Skill 的可信文档/resource 发布协议与冻结 source adapter 尚未提供；当前已实现 Product binding/gate，并拒绝缺少可信正文的远端业务 gate。兼容 `capability_skill_*` executor alias 尚未物理删除，但已不再模型可见；Windows 尚需对齐。
- Plugin Management System Agents 页面迁移/下线；在 capability policy 和其他 Agent Prompt 管理职责迁移完成前仍不能直接删除。
