# Solution Studio 双 MCP 与模块化规划实施方案

状态：macOS / Rust Host / Solution Studio 已实施，Windows 独立工作包待实施

编写日期：2026-10-16

当前实施范围：`plugins/solution-studio`、Rust Host、macOS Main Chat 与本地任务执行链路

后续实施范围：Windows Main Chat 与本地任务执行链路（本轮不修改 Windows 代码）

## 1. 最终产品形态

Solution Studio 仍然是一个安装包、一个 `pluginID`、一份按项目隔离的数据目录，但向 Main Chat 提供两个可以独立选择的 MCP 能力：

| Main Chat 中显示的能力 | MCP component | 职责 | 明确禁止 |
| --- | --- | --- | --- |
| 方案创建器 | `solution-authoring-mcp` | 创建和修订需求、总体设计、功能模块、模块设计、任务定义与依赖；校验、导出、交付规划 | 领取任务；把任务改成进行中、阻塞、完成或取消 |
| 任务执行器 | `solution-executor-mcp` | 读取计划，查询可领取任务，领取任务，并在实际执行后回写状态、阻塞原因和完成证据 | 修改需求、设计、任务标题/范围/验收条件/依赖；替换整个计划；导出或批准规划 |

Main Chat 的用户体验是两个独立“插件项”；实现层不拆成两个安装包。原因是本地 Plugin 数据目录按 `pluginID` 隔离，两个安装包会产生两份数据，创建器和执行器无法天然共享同一项目计划。一个安装包下的两个 component 则共享同一 `CHATOS_PLUGIN_DATA_DIR`，同时仍能做到工具面和选择面的隔离。

任务执行器只管理 Solution Studio 中的任务权威状态。真正修改源码、运行命令或验证项目时，执行 Task 仍需按任务需要显式取得 `project_read`、`project_write`、`terminal` 等基础能力；任务执行器本身不隐式获得项目写权限。

## 2. 现有问题与修复目标

现有 IAM 规划把技术文档章节当成需求，把业务能力塞进任务标题，并生成了 57 个任务、163 条直接依赖、15 层最长链，其中 76 条是冗余边。当前系统只有引用、环和验收条件校验，无法阻止这种“合法但不可执行”的计划。

本次修复必须同时解决三层问题：

1. **能力边界**：创建规划与执行回写不能继续混在同一个 MCP 工具面。
2. **规划模型**：软件项目必须先有总体设计，再按业务/基础/交付模块组织需求、设计和任务。
3. **图质量**：任务只保留真实直接前置；密度、冗余边和超长关键链需要程序校验，不能只依赖 Skill 提示。

UI 不承担纠正错误数据的责任，但要默认按模块展示，并隐藏可由其他路径推导出的冗余边，避免把已有问题放大成任务墙。

## 3. 总体架构决策

```text
Solution Studio 安装包（一个 pluginID / 一份项目数据）
├── solution-authoring-mcp
│   ├── 需求与模块
│   ├── 总体设计与模块设计
│   └── 任务定义、依赖、校验与交付
├── solution-executor-mcp
│   ├── 查询 ready tasks
│   ├── claim / resume
│   └── block / complete / cancel
└── solution-workbench
    ├── 总体设计入口
    ├── 模块泳道
    └── 任务 DAG
```

### 3.1 Main Chat 的选择单位

保持“安装包”和“可选择能力”两个概念：

- 安装、启停、版本和完整性校验仍以 Plugin 为单位。
- Main Chat 候选项改为 Plugin 的 MCP component。
- `ConversationExplicitCapabilitySelection` 的 Plugin 选择同时要求 `pluginKey` 和 `componentKey`。
- 新 Task 的 `plugin_hints`、Release 快照和最终 Provider 都携带相同的 `componentKey`。
- Provider 只启动选中的 component，不再默认遍历该 manifest 的全部 `mcpServers`。

直接升级规则：Plugin 候选按 MCP component 展开。Plugin 选择、Task hint 或 Release snapshot 缺少 `componentKey` 时一律失败关闭，不启动任何 component；历史选择和历史 Task 不做回退解释。

### 3.2 Manifest 扩展

为 `mcpServers` 的每个 server增加 Agent 展示信息：

```json
{
  "mcpServers": {
    "solution-authoring-mcp": {
      "type": "stdio",
      "bin": "chatos-solution-studio",
      "args": ["mcp-authoring"],
      "agent": {
        "displayName": "方案创建器",
        "description": "创建总体设计、功能模块与依赖清晰的实施计划"
      }
    },
    "solution-executor-mcp": {
      "type": "stdio",
      "bin": "chatos-solution-studio",
      "args": ["mcp-executor"],
      "agent": {
        "displayName": "任务执行器",
        "description": "领取可执行任务并回写进行中、阻塞或完成状态"
      }
    }
  }
}
```

`agent` 缺失时使用 Plugin 界面的显示名称和描述，但候选身份仍然是明确的 component，不能退回包级多服务选择。

## 4. 数据模型 v2

### 4.1 第一等功能模块

新增模块模型，并让需求、设计章节和任务明确归属模块：

```ts
interface RequirementModule {
  id: string;
  title: string;
  kind: 'foundation' | 'business' | 'delivery';
  description: string;
  order: number;
}

interface RequirementItem {
  // 保留现有字段
  moduleId: string;
  kind: 'capability' | 'quality' | 'constraint';
}

interface DesignSection {
  // 保留现有字段
  moduleId: string;
}

interface ExecutionTask {
  // 保留现有字段
  moduleId: string;
}
```

总体设计继续使用 `design.blocks`，不伪装成某个需求或任务。`design.sections` 表达各模块的详细设计。推荐的软件项目结构是：

```text
项目总体设计
├── 平台基础（foundation）
├── 身份管理（business）
│   ├── 用户与分类
│   └── 组织、岗位与关系
├── 应用与授权（business）
│   ├── 应用与帐号
│   └── 角色、菜单、资源与授权
└── 迁移、联调、验收与发布（delivery）
```

`phase` 保留为模块内的实施批次或里程碑标签，不再承担业务分组职责。

### 4.2 任务状态与执行记录

v2 同时定义任务当前 `status` 和由执行器维护的不可变状态记录：

```ts
interface TaskStatusEvent {
  id: string;
  taskId: string;
  from: TaskStatus;
  to: TaskStatus;
  occurredAt: string;
  runtimeSessionId: string;
  reason?: string;
  summary?: string;
  evidence: string[];
}

interface ExecutionPlan {
  // 保留现有字段
  statusEvents: TaskStatusEvent[];
}
```

状态变化使用 workspace revision 做乐观并发控制。`runtimeSessionId` 从 Host 注入的 `CHATOS_PLUGIN_RUNTIME_SESSION_ID` 读取，不能由模型伪造。

### 4.3 状态机

| 当前状态 | 允许的动作 | 目标状态 | 条件 |
| --- | --- | --- | --- |
| `planned` | claim | `in_progress` | 所有直接和间接前置均为 `done` |
| `blocked` | resume | `in_progress` | 前置仍满足；保留此前阻塞事件 |
| `in_progress` | block | `blocked` | 必须提供非空阻塞原因 |
| `in_progress` | complete | `done` | 必须提供完成摘要；有验收证据时一并保存 |
| `planned` / `in_progress` / `blocked` | cancel | `cancelled` | 必须提供取消原因 |
| `done` / `cancelled` | 任意状态动作 | 拒绝 | 终态不可被执行器重开 |

不再提供“任意写 status”的通用工具。需要重新打开终态任务时，必须由方案创建器创建替代任务并明确依赖/替代关系，不能篡改执行历史。

## 5. MCP 工具契约

### 5.1 方案创建器工具

- `solution_get_active_context`
- `solution_get_workspace`
- `solution_upsert_requirements`
- `solution_upsert_design`
- `solution_upsert_design_block`
- `solution_upsert_execution_plan`
- `solution_validate`
- `solution_export`
- `solution_finalize`

`solution_upsert_execution_plan` 增加以下服务端保护：

- 新任务的状态只能是 `planned`；传入其他状态直接拒绝。
- 已存在任务的状态、阻塞原因和状态历史由存储层保留，忽略模型提交的副本。
- `in_progress`、`done`、`cancelled` 任务的定义、依赖和验收条件不可被覆盖或删除。
- 修改 `blocked` 任务的定义前必须先通过执行器恢复或取消；创建器不能借“改计划”绕过状态机。
- 保存前执行模块引用、依赖、环、冗余边和密度分析。

### 5.2 任务执行器工具

- `solution_get_active_context`
- `solution_get_workspace_summary`
- `solution_list_tasks`：支持按 `moduleId`、状态、是否 ready 过滤
- `solution_get_task`
- `solution_claim_task`
- `solution_resume_task`
- `solution_block_task`
- `solution_complete_task`
- `solution_cancel_task`

执行器进程的 `tools/list` 中不得出现任何 `upsert`、设计块、导出、finalize 或整 workspace 替换工具。权限隔离以实际工具面和服务端 handler 白名单实现，不能只靠 Skill 文案。

每个写状态工具必须接收 `workspaceId`、`taskId`、`expectedRevision`，并返回新 revision、最新 task、当前 ready task 集合。状态冲突返回稳定错误码和最新 revision，要求调用者重新读取后决定，不自动覆盖。

## 6. Skill 调整

### 6.1 创建侧

修改 `solution-discovery`、`solution-design`、`solution-execution-plan`：

- 软件项目先建立模块，再创建模块内需求。
- 技术栈、DDL、OpenAPI、安全基线属于总体设计或 foundation 模块，不得直接伪装成业务需求。
- 开放问题优先保留在 `openQuestions` 或 ADR；只有产生可验收交付物时才转成任务。
- 每个任务只表达一个可验收结果，不把每个实现动作拆成节点。
- `dependsOn` 只记录不可缺少的直接前置，不写阶段级全连接或传递性重复边。

### 6.2 执行侧

新增 `solution-task-execution` Skill：

1. 先读取 ready tasks，不从 UI 顺序或文字描述猜可执行性。
2. claim 成功后才开始真实项目工作。
3. 实际工作失败时写 `blocked` 和具体原因，不伪造 `done`。
4. 完成前按任务 acceptance criteria 核验，并将摘要/证据写回。
5. 不调用创建侧工具，不重写依赖来让自己“变 ready”。

两个 MCP 使用不同的 `_meta.chatos/skillGate`。Host 仍加载同一固定 Release 的 Skill snapshot，但活动 component 只暴露其工具所需的 Router/Leaf Skill。

## 7. 程序化质量门禁

新增纯函数图分析模块，供 schema 校验、MCP 返回和 UI 共用：

- 拓扑排序和环检测。
- 最长依赖链。
- 传递闭包与传递约简。
- 冗余直接边列表。
- 每模块、每需求的任务密度。
- 每任务直接前置数和关联需求数。

建议阈值：

| 规则 | 级别 | 默认阈值/行为 |
| --- | --- | --- |
| 软件项目没有模块或没有 `business` 模块 | blocking | 禁止 finalize |
| 模块引用不存在 | blocking | 拒绝保存 |
| 环、未知依赖、自依赖 | blocking | 拒绝保存 |
| 存在传递性冗余依赖 | blocking at finalize | upsert 返回具体边；完成计划前必须消除 |
| 单需求直接关联任务过多 | warning | `> 8` |
| 总任务数量相对需求过密 | warning | `> max(12, 3 × requirementCount)` |
| 单任务关联需求过多 | warning | `> 3` |
| 单任务直接前置过多 | warning | `> 4` |
| 最长链过长 | warning | `> 10` |
| 非 milestone 任务无验收条件 | blocking | 保持现有规则 |

warning 不静默丢弃，应在 `solution_validate`、Workbench 和 finalize 回执中可见。blocking 问题必须修复后才能交付。

## 8. Workbench 改造

规划页从“按 requirement 递归拉出所有前置”改为三层浏览：

1. 项目总体设计。
2. 模块列表/泳道及模块设计。
3. 模块内任务 DAG，可按需求进一步筛选。

默认边集合使用传递约简，只展示直接且必要的依赖；提供“显示全部显式边”开关用于诊断。图外前置继续标注，但不把所有跨模块上游节点递归复制到当前模块画布。

建议修改点：

- `src/plan-view.ts`：新增 `modulePlanView`、`transitiveReductionEdges` 和图指标。
- `ui-src/studio/PlanGraph.tsx`：模块泳道、约简边开关、密度/长链告警。
- Workbench 上层页面：总体设计作为固定入口，模块作为主导航，需求作为模块内筛选。
- 导出 Markdown：按“总体设计 → 模块 → 需求/设计 → 任务”排序，不再平铺全部任务。

## 9. Main Chat 与 Task 冻结链改造

component 选择必须完整贯穿以下链路：

```text
Composer 候选
  -> ConversationExplicitCapabilitySelection
  -> explicit_capability_selection metadata
  -> Rust main-chat planner 的 required policy
  -> Task plugin_hints
  -> plugin_release_snapshots
  -> Native executor authorization
  -> makeAgentPluginSkillToolProviders
  -> 只启动目标 component
```

需要更新的数据结构：

- `ConversationExplicitCapabilitySelection`：Plugin 选择必须包含非空 `componentKey: String`。
- 新增 component 级候选 DTO；不要把两个 component 伪装成两个 `pluginID`。
- `TaskPluginHint`：要求非空 `component_key`。
- `TaskPluginReleaseSnapshot`：要求非空 `component_key`，并参与 capability revision。
- `LocalAgentTodoPluginSelection`：唯一性从 `pluginID` 改为 `(pluginID, componentKey)`。
- Provider assembly：接收结构化选择并只启动该 server；缺少或找不到 component 时失败关闭。

显式选择“方案创建器”后，模型不得在同一个 Task 中额外选择“任务执行器”或同包其他 component；Rust 的 `validate_explicit_plugin_hints` 按 `(plugin_key, component_key)` 校验。反向同理。

Windows 的 `plugin_hints`、任务快照和执行器使用同名字段与相同 fail-closed 语义，避免同一 Plugin 在不同平台获得不同权限。

### 9.1 Windows 独立后续工作包

Windows 不在本轮实现，由后续开发者按以下不可拆分的协议一次完成；在此工作包验收前，Windows 不得宣称支持 Solution Studio 双 MCP 独立选择。

数据契约：

- `WindowsLocalAgentPluginChoice` 从包级候选改为 component 级候选，身份字段至少包含 `pluginId`、`pluginKey`、`componentKey`、展示信息和固定 Release 信息。
- `PluginMcpServer` 解析 manifest 中的 `agent.displayName` 与 `agent.description`；Solution Studio 两个 component 分别显示“方案创建器”和“任务执行器”。
- Main Chat 的 `plugin_hints` 每项必须同时包含非空 `plugin_key` 与 `component_key`。
- `plugin_release_snapshots` 每项必须包含 `plugin_key`、`component_key`、`plugin_id`、`release_id`、`version`、`artifact_sha256`。
- 选择唯一性和授权键统一使用 `(plugin_key, component_key)`；不得继续按包级 `plugin_key` 去重。

执行规则：

- `WindowsLocalAgentCapabilityCatalog` 将同一安装包的两个 MCP 展开成两个独立候选，而不是一个包级候选。
- `WindowsLocalAgentBootstrapService` 在 `task_policy_internal` 发布 `plugin_components` 和 component 级 Release 快照，不再发布包级 `plugin_keys`。
- `WindowsLocalAgentTaskToolAuthorization.Resolve` 要求完整的新 `tool_options`；缺 `tool_options`、`plugin_hints`、`plugin_release_snapshots` 或任一 `component_key` 时直接失败。
- `WindowsLocalAgentPluginToolExecutor` 必须验证冻结 Release 与当前安装记录完全相同，并且只对所选 `componentKey` 调用 `PluginManifestLoader.PrepareAsync`；禁止遍历该包全部 `mcpServers`。
- Todo / Agent Teams 若继续支持 Plugin 选择，`AgentTodoPluginSelection`、引用库、可选项列表和运行时准备也必须携带 `componentKey`，只启动所选 component。
- 不添加 `IsLegacyUnrestricted`、包级全启动、缺字段默认值、旧 Task 推断或旧持久化记录补全等兼容路径；旧数据不可执行时直接删除或要求重新创建。

建议改动文件：

- `clients/windows/src/ChatOS.Connector/Plugins/PluginModels.cs`
- `clients/windows/src/ChatOS.Connector/Plugins/PluginManifestLoader.cs`
- `clients/windows/src/ChatOS.Connector/LocalAgent/WindowsLocalAgentCapabilityCatalog.cs`
- `clients/windows/src/ChatOS.Connector/LocalAgent/WindowsLocalAgentBootstrapService.cs`
- `clients/windows/src/ChatOS.Connector/LocalAgent/WindowsLocalAgentProjectToolExecutor.cs`
- `clients/windows/src/ChatOS.Connector/LocalAgent/WindowsLocalAgentPluginToolExecutor.cs`
- `clients/windows/src/ChatOS.Core/Domain/AgentTodoExecution.cs`
- `clients/windows/src/ChatOS.Connector/AgentTeams/AgentPluginToolRuntime.cs`
- `clients/windows/src/ChatOS.Connector/AgentTeams/AgentTeamToolExecutor.Todos.cs`

Windows 验收测试必须覆盖：一个包展开两个候选、只启动所选 component、跨 component 扩权失败、缺 component 失败、Release 漂移失败、旧包级 Task 失败，以及 authoring/executor 工具面互不越权。

## 10. 实施阶段与依赖

| 阶段 | 内容 | 前置 | 完成判据 |
| --- | --- | --- | --- |
| M0 | 固化 component 选择协议和失败关闭 fixture | 无 | manifest、selection、hint、snapshot 的 v2 样例可被测试解析；缺字段全部拒绝 |
| M1 | Main Chat component 级候选与 Task 冻结链 | M0 | UI 出现两个候选；选一个只启动一个 component；旧 Task 缺字段时拒绝运行 |
| M2 | 拆分 authoring/executor MCP 和 Skill | M1 | 两端 `tools/list` 无越权工具；共享同一 workspace |
| M3 | 状态机、乐观锁、创建侧状态保护和事件记录 | M2 | 所有非法迁移、未完成前置、伪造 done、覆盖执行中任务均失败关闭 |
| M4 | schema v2、模块模型、图分析和旧数据清理 | M2 | v1 文件直接删除；新软件计划无模块不能 finalize；冗余边可定位 |
| M5 | Workbench 模块泳道与传递约简展示 | M4 | 默认视图不再是任务墙；可查看全部显式边 |
| M6 | IAM workspace 定向重构 | M3、M4、M5 | 需求按业务能力归类；依赖无环、无冗余边；ready 集合正确 |
| M7 | 当前平台回归、打包和直接升级验证 | M1–M6 | npm/Swift/Rust 协议测试通过，旧 package/workspace 不会被新运行时读取 |
| W1（后续） | Windows component 选择、冻结、执行与 Agent Teams 同步 | M0、M2 | Windows 协议测试通过；一个包显示两个候选；只启动所选 component；旧包级 Task 失败关闭 |

推荐并行关系：M3 与 M4 可在 M2 后并行；M5 依赖 M4；IAM 数据重构必须等状态保护和新 UI 都可用后再做，避免再次写出旧结构。
W1 由独立开发者实施，不阻塞当前 macOS/Rust/插件交付，但 Windows 发布必须以 W1 完成为前置。

## 11. 主要文件改动清单

### ChatOS macOS / shared host

- `clients/macos/Sources/ChatOSConnector/NativePluginManifestModels.swift`
- `clients/macos/Sources/ChatOSConnector/NativeInstalledAgentPlugin.swift`，或新增 component capability DTO
- `clients/macos/Sources/ChatOSCore/ConversationExplicitCapabilitySelection.swift`
- `clients/macos/Sources/ChatOSApp/Features/Chat/ComposerView.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentConversationService.swift`
- `clients/macos/Sources/ChatOSConnector/NativeAgentPluginProviderAssembly.swift`
- `clients/macos/Sources/ChatOSCore/AgentGroupChat/LocalAgentTodo.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentPlatformToolCatalog.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalAgentProjectToolExecutor.swift`
- `clients/local_agent_host/layers/application/src/conversation_runtime.rs`
- `clients/local_agent_host/src/application/task_tools.rs`
- `clients/local_agent_host/src/application/task_tool_support.rs`
- `clients/local_agent_host/layers/profiles/src/planner.rs`

### Solution Studio

- `plugins/solution-studio/chatos.plugin.json`
- `plugins/solution-studio/.mcp.json`
- `plugins/solution-studio/bin/chatos-solution-studio`
- `plugins/solution-studio/src/mcp-server.ts`：重构为共享 server factory/handler
- 新增 authoring/executor entrypoint
- `plugins/solution-studio/src/schema.ts`
- `plugins/solution-studio/src/store.ts`
- `plugins/solution-studio/src/plan-view.ts`
- `plugins/solution-studio/ui-src/studio/PlanGraph.tsx`
- `plugins/solution-studio/skills/solution-*`
- 新增 `plugins/solution-studio/skills/solution-task-execution`

## 12. 测试矩阵

### 12.1 Host 与选择链

- 一个 Plugin 的两个 selectable MCP 生成两个 Main Chat 候选。
- 候选 ID 由 `pluginID + componentKey` 稳定组成，不因显示名变化而漂移。
- 显式选择 authoring 后，Task hint 和 Release snapshot 都冻结 authoring component。
- Provider 只启动冻结 component。
- 伪造或不存在的 component、跨 component 扩权、Release 变化全部失败关闭。
- 旧 manifest、旧 explicit selection、旧 Task snapshot 缺少 component identity 时失败关闭。

### 12.2 MCP 权限边界

- authoring `tools/list` 不含状态变更工具。
- executor `tools/list` 不含任何规划写入、export、finalize 工具。
- executor 直接调用隐藏的 authoring handler 被拒绝，而不是仅从列表隐藏。
- 两个 component 在同一项目读到同一个 `workspaceId` 和 revision。

### 12.3 状态机

- 未完成前置时 claim 失败。
- ready 任务 claim 成功并从 ready 集合移除。
- block 无原因失败；complete 无摘要失败。
- 非法跳转和终态重开失败。
- revision 冲突不会覆盖他人状态。
- authoring upsert 不能伪造 `done`，不能删除或改写正在执行/已完成任务。

### 12.4 模块与依赖质量

- v1 workspace 初始化时被删除，不进入运行时、不生成任何映射数据。
- 缺模块、未知 moduleId、无 business 模块的软件项目不能 finalize。
- 冗余边检测结果稳定，传递约简不改变可达关系。
- 密度、直接前置、关联需求数和最长链阈值均有边界测试。
- ready 计算始终使用完整 canonical graph，不受 UI 筛选影响。

### 12.5 UI

- 模块切换不会复制跨模块任务。
- 默认边为传递约简；切换后可看到全部显式边。
- 图外前置、ready、blocked、done 状态显示正确。
- 大图自动布局不重叠，模块筛选和全图的坐标持久化互不污染。

## 13. 旧 workspace 清理与 IAM 重建

代码升级不包含任何旧 workspace 转换器：

1. Store 初始化时只接受 `schemaVersion: 2` 且通过结构断言的数据。
2. v1、缺少版本、无效 JSON 或缺少 v2 必填字段的 `.solution.json` 直接删除。
3. localStorage 使用新的 v2 key，不读取旧 key。
4. 不创建“待整理”模块，不猜测 `moduleId`，不保留旧状态、revision 或画布坐标。

IAM workspace 通过方案创建器重新建立 v2 语义：

- 将 DDL、Java/Security/OpenAPI、React UX、数据变更测试从“需求”移到总体设计、foundation 或 delivery。
- 按用户、组织、岗位、应用、帐号、角色/菜单/资源/授权建立业务模块和能力需求。
- 合并机械性小任务，移除 M0 问题任务；未决项回到 open questions/ADR。
- 对 DAG 做传递约简，逐条确认剩余跨模块依赖确实是开工前置。
- 用新 validator 验证并人工抽查 critical path、ready tasks 和每模块验收条件。

旧 IAM workspace 不参与重建流程；新的 v2 计划通过代码证据和用户目标重新生成。

## 14. 发布与直接升级

这是一次不兼容升级：

1. 当前发布中的 macOS 与 Rust Host 同时要求 component identity，并在缺失时失败关闭。
2. Solution Studio 发布双 MCP、schema v2 和新的 Workbench。
3. 启动时删除旧 workspace；用户需要的计划由方案创建器按当前项目重新生成。
4. 不提供旧 Task、旧 selection、旧 manifest 或 v1 workspace 的运行时回退路径。

Windows 必须在 W1 完成后单独发布；不得通过保留包级选择或启动全部 MCP 的方式提前兼容当前协议。

若必须回滚应用版本，只回滚程序包；v2 workspace 不写回 v1，也不由旧版本读取。执行状态事件不得被清空或伪造。

## 15. 最终验收标准

全部满足后才算完成：

- Main Chat 中能分别选择“方案创建器”和“任务执行器”。
- 选择任一项时，进程和工具面都只包含该 component，没有隐式启动另一个 MCP。
- 两个 MCP 共享同一项目 workspace，但不能越过各自写权限。
- 软件项目具有清晰的总体设计和第一等功能模块。
- 任务定义按模块组织，依赖只包含真实直接前置，默认图不展示冗余边。
- 任务只有在前置完成后才能领取；完成、阻塞和取消都有受校验的状态回写。
- 创建器无法伪造执行状态，执行器无法修改规划结构。
- IAM 现有计划完成备份和重构，不再以技术章节充当需求，也不再出现当前的高密度依赖墙。
