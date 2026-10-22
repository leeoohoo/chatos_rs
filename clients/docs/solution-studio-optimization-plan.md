# Solution Studio：Skill、工具与页面优化方案

日期：2026-10-13。对象：`plugins/solution-studio`，以及其与 ChatOS 项目、需求调研和本地 Task Runner 的边界。

## 1. 结论与本轮范围

不是 T-004 丢失，而是需求局部视图把跨需求任务隐藏后，又把依赖关系删除了。图、表格、工具因此不是同一个口径。生成的设计 SVG 另有箭头和业务流程表达问题；它与执行 DAG 是两种不同的图，必须分别修复。

2026-10-13审查阶段完成确定缺陷的源码修复和隔离回归，并制定后续方案，没有发布新 Release或升级本机。2026-10-14经用户明确授权，已发布0.1.10，详细证据与本机状态见文末。两个阶段均没有修改真实财务项目或保存的规划内容，没有启动第二份 ChatOS或直接覆盖安装目录。本文的“已修复”只指源码与已列明的测试，不代表安装版视觉/交互验收完成。

Windows native/UI 不在本轮实现范围；共享 JS 插件的修复同时适用于使用同一新版 Release 的 Windows。后续 Host 契约与 Windows 验收要求见第 9 节。

## 2. T-004 到底在哪里

对截图对应的隔离插件存储进行只读核对：`费用报销审批`，workspace `solution-d313c305`，revision 9。R-002 是“职责分离审批与审计”。

| 任务 | 内容 | 关联需求 | 前置 |
| --- | --- | --- | --- |
| T-001 | 确认报销业务政策与接口决策 | R-001、R-002、R-003 | 无 |
| T-002 | 实现申请单数据模型与校验 | R-001 | T-001 |
| T-003 | 实现审批状态机与审计事件 | R-002 | T-002 |
| T-004 | 实现财务复核及凭证关联适配 | R-003「财务复核及凭证协同」 | T-001、T-003 |
| T-005 | 端到端回归与交付评审 | R-001、R-002、R-003 | T-004 |

这五个任务当前都是 `planned`，只有 T-001 的前置条件满足。完整图应有五个节点、五条边：

```text
T-001 → T-002 → T-003 → T-004 → T-005
   └────────────────────→ T-004
```

“本需求主线 3”表示三个任务关联 R-002，不表示整份计划只有三个任务，也不表示三个任务都能开始。T-004 是 T-005 的跨需求前置；应当可查、可定位，不能因为不属于 R-002 就消失于用户的依赖解释中。

## 3. 代码证据与缺口

### 3.1 执行图与页面

证据文件：`ui-src/studio/SolutionStudioApp.tsx` 的 `RequirementSheet`、`TaskList`、`TaskInspector`；`ui-src/studio/PlanGraph.tsx`。

原始实现：

- `planScope` 默认是 `requirement`。
- `directGraphTasks` 将每个任务的 `dependsOn` 过滤成当前需求内部的 ID。R-002 的三个任务因此都变为无前置。
- `PlanGraph.readyIds` 在过滤后的任务集上重新计算状态，图报告 3；表格在完整 workspace 上计算，报告 1。
- “完整依赖”确实构建了递归前置闭包，但 `onSelect` 只接受当前需求任务。即使看到了 T-004，也无法点开详情。
- `TaskInspector` 只收到本需求的任务、需求和设计数组，跨需求前置不能被完整呈现/编辑。
- 拖拽和自动整理按 `DirtyPart='plan'` 保存；`save()` 会同步更新 `basedOnDesignRevision`。纯布局操作可能掩盖已经过期的业务计划。

本轮已修复：

- 新增 `src/plan-view.ts`，统一完整任务集的 ready 计算与需求视图构造。`validateWorkspace`、概览、图、表格共用同一算法。
- 需求视图不修改任务语义或删除 `dependsOn`；仅筛选可见节点和位置。ELK 与 React Flow 共用可见边生成逻辑，避免不存在的画布端点和重复边。
- 默认完整前置闭包。跨需求节点用虚线及“跨需求支撑”标记，不再改写原任务标题。
- 主线视图保留“图外前置”提示；页面提供跨需求任务按钮。支撑任务可选中，展示原任务说明、状态、所属需求、前置、输出与验收；可跳到所属需求再编辑。
- 本需求编辑器收到完整 workspace 的引用候选。不存在的前置会显式告警，而非被当成已完成。
- 图与表格统一显示“本需求 N 个任务具备开始条件”，并说明不代表已经启动执行。支撑任务不混入本需求计数。
- 布局修改只标记 workspace 的展示修改，不刷新业务计划的设计基准版本。

仍需后续实现：全局任务目录/搜索、直接从表格前置 ID 导航、长链聚焦、大图性能、自动布局失败提示与竞态取消、按视图保存布局、进度事件同步、撤销与冲突处理。不能把本轮的静态组件渲染测试当成拖拽/缩放/全屏的实机验收。

### 3.2 生成的设计 SVG

证据：`src/schema.ts` 的 `assertRenderableSvg`、`validateDesignBlockExpectations`；`DesignBlockPreview`；`skills/solution-design/references/design-content-types.md`；上述 workspace 的保存块。

已实际渲染保存的 `D-002-B-002`「审批状态流」及 `D-000-B-002`「总体技术架构」，发现：

1. 多条独立连线共用一个包含多个 `M` 子路径的 `<path marker-end=...>`，只有组合路径末端出现箭头。审批图第一条和两条分支只有线，没有期望的方向标记。
2. 审批分支没有明确的“批准/退回/拒绝”边标签；退回与拒绝被合在一个节点，不利于理解可修改重提和终止的差别。
3. 同一设计下新增的 `D-002-B-003` 文本明确描述“员工提交→直属经理审批→财务复核→付款归档”，已有 SVG 仍是旧的草稿/待审批/批准/退回/财务图，没有对应的付款归档终点。保存了新文本，不等于关联图也更新了。

现有 SVG 校验是正则检查外壳、`viewBox`、少数危险内容及可见元素标签；不是 XML 完整解析、真实渲染检查或业务流程验证。一个背景矩形或不可见内容也可能满足可见标签检查。`expectedDesignBlocks` 只核对 ID、类型、非空，不能证明内容满足用户的四步要求。

本轮仅补充生成指导：独立连线使用独立 SVG 元素；业务分支分别表达；修改流程文字时同步修改相应图。没有擅自重写真实项目的保存图，也没有声称已解决所有生成质量问题。

### 3.3 工具与存储

证据：`src/mcp-server.ts` 的工具定义、`callTool`、`blockingIssuesForScope`、`exportWorkspace`；`src/store.ts`；`src/studio-server.ts`；`src/schema.ts`。

已有能力应保留：文件锁与原子保存、完整文档结构断言、Host 项目绑定、跨项目读取拒绝、任务重复 ID 检查、前置引用/自依赖/循环校验、文档版本新鲜度、最终交付 proof、内部产物注册。

关键缺口：

| 问题 | 当前代码表现 | 风险/改法 |
| --- | --- | --- |
| 嵌套 schema 太宽 | 三类文档在 MCP schema 中基本是 `type: object`，具体字段依赖 Skill 与运行时断言 | 发布同源、完整 schema 与示例，不让模型猜嵌套参数；保留服务端校验 |
| 整份更新无 CAS | requirements/design/plan 整份 upsert 无 `expectedRevision`，与 block/status 的 CAS 不一致 | 文件锁只防写坏，不防模型用旧全文覆盖新内容；所有更新统一版本前置条件 |
| 业务错误不可定位 | 错误基本拼成字符串 | 返回 code、path、task/block ID、当前 revision、可恢复建议及是否发生写入；UI直达相关位置 |
| 交付完整性偏弱 | task description/deliverables/trace links 可空；里程碑可无验收；blocked 无原因只 warning | 草稿可以不完整，交付阶段按 scope/profile 约束；不可用空字符串或数量冒充质量 |
| 需求/设计覆盖不完整 | 未校验需求空白验收；design decisions 的需求引用、任务关联需求与设计的相容性等不充分 | 加引用关系与输出验收校验；避免只满足“出现过一个 ID” |
| 范围过滤靠 code 黑名单 | `unknown_requirement` 同时用于设计和任务，`blockingIssuesForScope` 按 code 排除 | 加 issue.domain/scope，从归属判断；requirements-only 不应因旧计划中的任务引用被无关阻塞 |
| finalize/export 快照分离 | finalize 校验某次读取，export 函数又读取一次，proof/summary 仍用前一次对象 | 导出已校验的不可变快照并绑定 revision/hash，避免并发写入期间 proof 与产物错位 |
| export 可绕过交付门槛 | description 说 after validation，实现返回 validation 但不拒绝不完整输出 | 区分“草稿导出”和“已校验交付”，不要把成功导出当成审批或实施完成 |
| Skill 跨目录链接 | 部分 leaf 要读 `../solution-studio/...`，Host 的相对路径不允许 `..` | 用 router 的真实 skill_ref 读取其 `references/workspace-schema.md`，不解除路径隔离 |

这些是代码审查结论；除第 3.1 节和上述窄 Skill 修改外，本轮未实现这些工具改造。特别是 finalize 的并发竞态需要故障注入验收，不能宣称已经现场复现。

### 3.4 Skill 和 Host 接入

不是只有平台的一段 prompt。插件 manifest 声明了五个 Skill：`solution-studio` router，及 discovery/design/execution-plan/validation 四个 leaf。十个 Solution 工具的 `_meta.chatos/skillGate.allOf` 已声明 router 与相应 leaf，Host 会基于实际安装 Release 的固定快照实施 gate。

macOS Host 证据：`NativePluginSkillSnapshot.swift` 的 `activateV2/readV2Resource/resourceDescriptorsV2`，以及 `NativeAgentSkillHostProtocol.swift`。已有统一渐进读取协议、资源清单和内容 hash；不是另造一套“插件 prompt”。模型使用当前 Run 目录中的 `agent_skill_activate/list_resources/read_resource` 与精确 `skill_ref`，不能猜 ref 或业务工具。

但内容层仍不足：

- 设计 Skill 大量约束面向开发，工具的校验却对所有项目都要求“技术基线+总体架构”。文案/设计项目可能被迫编造技术栈或画无意义架构图。
- `isDevelopmentProject` 对自由文本做子串匹配，不是稳定业务类型契约。
- router 没有需求调研的触发判据、问卷回填与冲突处置，也没有各业务场景的引用路由。
- 执行计划原来未明确跨需求引用、同文件写入边界和与本地 Task 的区别；本轮已补充这些规则与按需 reference。
- validation Skill 虽要求观察 SVG，工具没有绑定到块内容/revision 的渲染证据，不应由模型自己一句“已看过”充当完成证明。
- UI 的计划状态是人工/工具记录；没有真实 Task 的持久映射，不能显示成已派发、等待调度或运行完成。

## 4. 推荐架构：沿用五 Skill，不再拆一套平台 Agent prompt

### 4.1 分工与渐进披露

保留现有 Skill 名称、入口与工具 gate，避免让旧调用和目录失效。router 只保留：当前项目/保存计划识别、意图和场景路由、调研判据、写入/授权边界、交付 scope 与真实状态解释。详细内容按需读取，不把所有职业说明一股脑注入。

建议资源归属：

| Skill | 必要主规则 | 按条件读取的 reference |
| --- | --- | --- |
| solution-studio | 项目单一规划、正确 scope、规划不等于实施、Host 协议 | workspace-schema、delivery-profiles、requirements-survey-and-handoff |
| solution-discovery | 用户目标/限制/证据/假设分离、可验证验收 | existing-codebase、product-and-service、design-brief、content-and-campaign |
| solution-design | 每需求一个已确定方案、请求内容清单、按块保存 | development-design、visual-and-interaction-design、content-strategy、design-content-types、visual-quality |
| solution-execution-plan | 原始 DAG 不丢、输出/验收、写入互斥 | dependencies-and-handoff（本轮新增）、task-breakdown-by-delivery |
| solution-validation | 同一版本回读、范围校验、真实验收证据 | scope-and-status、visual-verification、recovery-and-conflicts |

只增加实际需要的资源，不为“全面”堆空目录。公共 schema 唯一维护；leaf 说明读 router 的哪份资源、通过哪个目录提供的 ref，不复制五份、不在工具参数里使用 `../`。

### 4.2 场景必须改变判断与交付，而不是仅换文案

| 场景 | 调研重点 | 设计应包含 | 拆分/依赖依据 | 验收 |
| --- | --- | --- | --- | --- |
| 现有软件/财务项目 | 当前服务/数据/权限、业务口径、不可改范围 | 与证据一致的架构、接口/状态/迁移、失败恢复、必要页面 | 政策→数据契约→模块实现→联调；共享文件串行 | 回归、授权/幂等/审计、迁移与失败路径 |
| 新产品/网站/服务 | 用户、价值、平台、MVP、运营限制 | 用户旅程、信息架构、服务流程、必要技术选择 | 业务规则/内容/设计与开发分别交付，真实输入形成前置 | 场景可走通、范围和成功指标清楚 |
| 视觉/交互设计 | 品牌、受众、参考、设备、可访问性、要哪些状态 | 页面/状态、交互与视觉规范、素材/交付格式 | IA/文案基线→页面→状态补齐→评审；共用设计稿指定owner | 可读、状态完整、导出可用、设计规范一致 |
| 文案/活动/内容 | 目标受众、渠道、语气、禁用表述、字数、事实来源 | 信息层级、信息主张、选定策略、分渠道内容和审稿标准 | 核实素材→核心主张→渠道稿→事实/合规/编辑评审 | 事实可查、语气/长度/渠道契合；不强制技术栈/架构图 |
| 分析/研究/业务流程 | 数据可用性、假设、指标、决策对象、涉敏范围 | 方法、证据、流程/决策与不确定性；图表按解释需求 | 数据准备→分析→复核→结论；不执行真实账务/付款 | 来源、方法、可重现性、限制与结论一致 |

不得固定为“一条需求一个执行任务”、固定五任务或固定全链串行。按独立成果、专业/工具差异、风险和可验收边界拆分；小需求允许一个 Task，只要求预期输出和验收，不要求“不拆分理由”。多需求共享政策、基础组件或终验可共用一个规划任务。

### 4.3 需求调研的使用边界

复用产品级需求调研，不在 Solution 插件创建另一套问卷、用户表或恢复事件。仅在会改变范围/方向/验收的关键选择缺失时开启；明确的窄修复不应强制问卷。

流程：识别关键未知→发现/激活现有调研能力→按当前 schema 提问→等待用户提交→原 Run 回唤→读取答案/方案及其版本→更新同一 Solution workspace。把用户确认、调研来源与版本写入 evidence/decisions，仍有未知项进入 openQuestions。不得自动批准、自动执行，或把旧调研答案覆盖本轮明确用户选择。

Main Chat 与团队入口应使用同一项目归属、页面和 durable 回唤契约。用户看到“请补充需求”“正在整理方案”“方案待确认”，不暴露 Host/connector 内部术语。调研后的方案必须通过明确引用和版本进入后续设计/Task 指令，不能只作为聊天记录附注。

参考现有文档：`local-agent-main-chat-requirement-discovery.md` 与 `local-agent-task-orchestration-skill-architecture.md`。产品调研作为 companion capability，不改变 Main Chat 原八个 Task 业务工具。

## 5. 工具优化：先收紧现有十个工具，不盲目新增

当前 Solution Studio 是十个工具，不是 Main Chat 的八个 Task 工具：

| 工具 | 推荐调整 |
| --- | --- |
| solution_get_active_context | 返回简洁项目与当前规划摘要；不存在/不可读区别明确，不要求用户输入内部ID |
| solution_get_workspace | 保留完整读取；按需增加任务/块/变更摘要读取模式，revision 一致，避免大SVG挤掉关键上下文 |
| solution_upsert_requirements | 完整 schema、CAS、保持既有ID、报告引用/删除影响；与调研证据对齐 |
| solution_upsert_design | 同源 schema、按 delivery profile 校验、请求内容清单及 freshness；不静默移除有效图块 |
| solution_upsert_design_block | 继续原子单块写；XML解析/安全白名单、内容hash、质量诊断；全workspace块ID唯一 |
| solution_upsert_execution_plan | 保留原始 DAG，CAS、输出/验收/关联一致性、写入边界；前置非法时不落盘 |
| solution_update_task | 明确记录状态而非执行；blocked原因、状态迁移约束、依据；不要以“写了计划”标done |
| solution_validate | 返回 domain/scope、定位、全局 ready 与原因、缺失内容、图检查分级；不把所有场景当开发 |
| solution_export | 显式草稿/已验收状态，输出同一快照；未保存页面不假装导出最新内容 |
| solution_finalize | scope明确、同revision/hash快照、请求交付清单与验证证据；注册产物/proof而非审批实施 |

### 5.1 同源契约与并发

建议用共享运行时 schema 生成 MCP 的 JSON Schema、TS 类型、HTTP校验与 reference 示例。API/模型参数不依赖 README 中的隐含字段。草稿结构合法即可增量存储，交付前校验业务完整性；这两个门槛不能混为一谈。

所有修改采用 `expectedRevision`（创建另定义明确无现有版本语义），服务端锁内核对；冲突返回 `revision_conflict` 及 actualRevision。模型回读并合并，不自动重放旧全文。升级可先兼容旧参数并告警，再按新版 capability schema 强制；旧冻结 Run 仍遵循其原契约，不热改快照。

finalize 必须导出刚校验的对象，proof 绑定同一 revision、artifact hash、delivery scope、预期内容清单。全workspace design block ID 唯一，避免期待ID映射被另一个section同名块覆盖。

### 5.2 开始条件、业务确认与真实执行分离

不持久化“ready”伪状态。按完整 canonical DAG 派生前置是否满足；返回缺失前置、未完成前置、取消/阻塞前置、循环等具体原因。

三个层次不能合并：

1. 前置条件满足：例如仅 T-001。
2. 方案/业务选择已确认且用户授权实施。
3. 已创建 Task，实际启动/运行/完成。

后续可增加独立的 execution binding 数据：规划 workspace/revision/T-* ↔ Host Task ID/运行结果/验证依据。规划 T-004 不等于 Task Runner 的 Task UUID。先有明确授权和绑定再联动；由既有 Main Chat Task 编排工具派发，Solution 不复制调度器。

`dependsOn` 管输入顺序；`owned_paths`/共享产物边界管写入互斥。优先使用现有 Host 契约，避免插件与 runner 各维护一套不同含义的冲突判断。运行开始回调必须来自实际状态迁移，不在批量创建时一次性伪造所有开始通知。

## 6. 生成图优化：语义、结构、安全、视觉四层

### 6.1 近期：保持 SVG，补全真实检查

- 从需求/方案形成交付清单：图类型、对应需求、必须展示的步骤/状态/分支/终点。清单与请求有关，不靠模型自行提交一个空数组让 finalize 放行。
- 用 XML 解析器和安全白名单替换“正则就是可渲染”的假设：有效数值 viewBox、允许元素/属性、局部引用可解析；拒绝脚本/事件/外部资源/CSS外部引用/危险链接/栅格嵌入等不在契约内的内容。限制大小/节点数，防异常渲染耗尽资源。
- 结构检查：每个有向连接单独元素、marker存在、节点/边引用合法、分支标签、步骤/终点覆盖。仅原始SVG时不能可靠恢复所有业务语义，要如实报告“需语义复核”。
- 渲染保存的块，而非模型未保存的源文本；QA记录绑定 block ID、content hash、revision和渲染尺寸。自动检查边界裁切/重叠风险，人工/模型实际观察方向、标签、可读性和业务一致性。
- 语法、安全、渲染、业务语义四层结果分别输出。工具能证明什么就报告什么，不用“visual verified=true”自述替代证据。
- 文本变更标记相关图可能过期，新增四步说明后不能继续展示旧流程却称已同步。

### 6.2 中期：可编辑结构化图，不让模型手算所有坐标

建议在现有 design block 契约版本化扩展 `diagramModel`（节点/边/容器/端口/标签/需求或步骤关联）；架构/流程用确定性布局生成 self-contained SVG。语义模型是源，SVG是派生产物，保留旧纯SVG读取并标明检查能力差异。

不要让 `diagramModel` 和 SVG变成两份可独立修改、互相矛盾的源；编辑语义后重生成。UI原型仍允许纯SVG，未来模板/组件帮助布局，不强迫文案需求生成页面图。

执行计划始终从 `tasks[].dependsOn` 生成 DAG，不额外保存一份“模型画的执行SVG”。设计的业务流程图不能取代实施顺序，二者名称和入口需区别清楚。

## 7. 页面优化：能找、能解释、能看清、能反映最新状态

### 7.1 项目级与需求级视图

保留需求详情里的“本需求/完整前置”切换，同时增加项目级“执行计划”入口、任务目录、按ID/标题搜索、按需求/阶段/状态筛选。

完整前置闭包默认展示跨需求支撑，图例解释节点样式。选择支撑节点看全部所属需求，不伪造单一owner；提供“查看所属需求”和“定位到图中”。主线视图可用边界stub表现图外任务，不能伪造跳过中间任务的实线。

表格前置ID成为独立可操作控件，注意不能在整个button行内嵌button；用语义合适的表格/行容器。详情至少显示真实前置状态、等待/阻塞原因、输出、验收、来源/设计链接；新增/编辑支持预期输出和验收，而不是创建空deliverables。

### 7.2 状态与更新

- 图、表、概览、工具返回来自相同完整状态分析，计数明确是本需求还是全项目。
- “计划未开始/前置未满足/显式阻塞/已完成”和“未关联执行/运行中/等待用户”分层显示。无Task映射不要冒称本地调度状态。
- SSE 当前只刷新列表，不重读已打开 workspace；补充revision驱动的自动刷新。页面无未保存修改时同步，dirty时提示冲突并比较，不覆盖用户编辑。
- 过期设计/计划明确横幅、源revision和更新入口；拖拽布局不能消除过期信号。
- 保存期间的新编辑不能被返回的旧candidate覆盖或被清空dirty标记；保存请求绑定编辑序号，并保护后续编辑。
- 校验应有加载/失败/重试，错误直达任务/图块/字段；不要只弹2.8秒toast。
- Host正式页面不可在服务暂不可用时静默变成本地浏览器草稿。LocalRepository仅明确开发/离线演示使用，并提示与项目正式规划不互通。

### 7.3 图与内容可读性

设计文本已经声明 Markdown，页面应使用安全的Markdown渲染，而非全部当pre-wrap字符串。SVG提供放大、全屏、原尺寸/pan/zoom与源码，并展示生成/校验/过期状态；`img`加载失败有明确错误与重试，不用空白当成功。

执行 DAG 要有箭头图例、当前任务上下游高亮、长标题省略+完整详情、焦点定位、搜索、缩放/自动整理/布局恢复。ELK异步操作需generation token/cancellation、错误提示，依赖边变更也参与layout signature；仅任务ID相同不表示图结构相同。自动整理不强制改写用户布局。

小窗口、深浅色、全屏返回、键盘关闭与焦点恢复、辅助读屏、大图性能分别验收。当前打包JS约2MB且有Vite体积告警；图组件与ELK可按入口懒加载，不用调高阈值隐藏问题。

## 8. 分阶段任务与验收

| 阶段 | 工作包 | 输出/验收 | 当前状态 |
| --- | --- | --- | --- |
| P0-A | 截图依赖/计数/导航一致性 | 5节点5边、R-002仅1个符合前置条件、主线不删除前置、T004可查可跳、缺失ID告警 | 源码及隔离回归已完成；未做安装版UI验收 |
| P0-B | Skill窄修 | 跨需求/同文件写入/规划与运行区分、逐连线箭头、文本图一致性；资源可发现 | 修改与检查已完成；未做真实模型质量评测 |
| P1-A | 同源schema、CAS、错误定位、scope与快照 | 旧全文更新冲突不覆盖，错误可定位，finalize产物/proof同版本，scope误阻塞用例通过 | 待实现 |
| P1-B | 场景Skill与调研贯通 | 开发/设计/文案/研究真实场景按需reference；调研回填同计划并用于后续 | 待实现 |
| P1-C | 生成图质量与语义清单 | 四步/分支/终点一致；独立箭头；XML/安全/渲染证据按内容版本绑定 | 待实现；仅生成指导已窄修 |
| P1-D | 项目级任务导航、更新/冲突与内容预览 | 搜索定位、前置可点、文字渲染、图加载错误、外部写入同步/dirty保护 | 待实现 |
| P2-A | 可编辑diagramModel/确定性布局 | 同一语义源、兼容旧SVG、不再手算连接坐标 | 待实现 |
| P2-B | 授权后的执行handoff与状态映射 | 同saved revision、保持原DAG/输出验收/写边界，实际启动才回调 | 待实现；先明确契约，不自动派发 |

修改同一文件的工作包串行，尤其 `schema.ts`、`mcp-server.ts`、`SolutionStudioApp.tsx`；文档/隔离验证可独立，不安排多个实现任务同时写这些文件。最终集成与验收建立真实前置，不能仅在描述里说“之后做”。

### 8.1 本轮隔离测试

- `test/plan-view.test.mjs`：主线保留跨需求前置、完整递归闭包/五条边、隐藏前置完成后解锁、缺失/取消前置不解锁、闭包循环终止、位置仅过滤展示、支撑任务不污染本需求计数、重复边去重。
- `test/plan-ui.test.mjs`：内存构建真实React组件并静态渲染；确认默认完整范围、图表同计数、选中T004显示R003/前置/输出验收、当前需求编辑器包含跨需求候选。不启动桌面App或访问生产后端。
- 新fixture是脱敏的同构图，不携带财务源码/客户数据。
- 执行插件typecheck、完整测试、pack dry-run与Skill gate审计；执行两项变动Skill的quick_validate。
- 现有 `.github/workflows/ci.yml` 的 `solution-studio-plugin` 调用 `pack:verify`，上述新 `test/*.test.mjs` 自动进入该CI路径；不必另造同名验收流程。

本轮本机结果：typecheck通过；完整26项测试全部通过（16项原有、10项新增）；pack dry-run通过；Skill gate审计通过（10工具、5 Skills）；execution-plan与design两项quick_validate通过。本机Node为24.14.1，现有CI使用Node20，未在本轮触发远端CI。Vite仍有单JS包约2MB的体积告警，未掩盖或宣称解决。

这套测试防依赖计算和页面输出再次分叉，但不验证浏览器中实际连线几何/操作、冻结安装版、真实模型是否按需读reference或真正会使用调研。下列用例仍必须补齐。

### 8.2 后续CI与实机矩阵

1. 契约：JSON schema边界、嵌套缺字段、空白输出/验收、重复块ID、失效需求/设计引用、完整更新CAS、多写者并发、finalize导出竞态故障注入。
2. 范围：requirements/design/plan/full各自的草稿/缺失/过期/引用错误；旧下游问题不错误阻塞无关requirements交付，完整交付仍拒绝真实错误。
3. 图：XML错误、非法viewBox、远程CSS/资源/链接、不可见空图、缺marker、裁切/重叠、四步少一步、退回与拒绝错误合并、文字变图不变。
4. 页面：同构财务fixture打开R002默认含T004；点T004→R003；切主线再找T004；前置完成后所有口径同步；丢ID/循环显示具体原因；拖拽不会令stale_plan消失；保存期间编辑不丢。
5. 实际模型：至少软件窄改、复杂财务流程、视觉改版、多渠道文案、业务分析各一组；从原始请求考核选择、调研触发、reference阅读、内容/图/任务质量，而非匹配固定措辞或仅检查调用数量。
6. Task联动：得到实施授权前不创建；批量任务保留真实前置和互斥；映射Run重启可恢复；仅真实启动时通知；失败/取消不标done；后续task使用保存的调研方案与验收。
7. 发布：验证解包包的MCP/Skill资源与schema、冻结快照hash、旧数据兼容、升级后正式App单实例；不能用开发构建启动成功冒充安装版已升级。

## 9. 发布、兼容与Windows对齐

审查时源码version为0.1.9，这个版本此前已发布，不能重新覆盖同一Release。2026-10-14已将本轮窄修发布为新的0.1.10，仍不代表本文P1/P2方案全部实现。通过插件平台正常Release升级；不修改安装完整性记录或就地替换Skill。

旧workspace/task/block ID和内容原样保留。增加字段前定义兼容读取/迁移与备份；不要删除所谓“多余任务”，尤其不能清理T004来让图看起来一致。单项目历史多文件/损坏文件的恢复要显式诊断，不因list的最新一个选择逻辑而悄悄隐藏/覆盖旧数据。

Windows开发需要：

- 安装同一共享插件Release，使图、工具、Skill内容一致；原生层不另实现一套过滤后ready计算。
- 对齐统一 `skill_ref`、渐进reference读取、固定Release/hash与Skill gate；resource不能绕过所属快照或 `..` 路径限制。
- 对齐项目上下文注入/隔离、需求调研独立入口、答案提交→原Run回唤、方案版本与后续Task使用。
- 对齐执行绑定和真实状态通知契约、任务取消/失败/阻塞原因；原八个Task业务工具保持不变。
- 在正式Windows客户端验证跨需求导航、字体/缩放/全屏/键盘、SVG本地预览、升级与旧规划数据；共享JS单测通过不等于Windows实机通过。

插件管理平台的Agent配置页不是本轮修复目标。Skill业务指导应统一到可信Skill包，平台只管理发布/目录/权限/版本等；但移除Agent页面前要核查它是否仍承载模型配置、发布元数据或旧记录迁移。不能仅因Skill统一就认定整个页面无用途并直接删除。

## 10. 0.1.10正式发布记录（2026-10-14追加）

- 用户授权：修好后发布到插件平台，继续通过正式客户端升级入口升级。
- 版本：0.1.10；package、lockfile、ChatOS manifest、Codex manifest和MCP serverInfo同步。增加manifest版本一致性测试，修正旧Codex manifest停在0.1.7的问题。
- Release ID：`9c37e31c-02e3-4340-bf67-8feed546a140`。
- SHA256：`df4a83ee93a6e9f26692012a6b4a18bff698ca20ecb42a923bbcbb34f59ae554`。
- 发布时间：2026-10-14 09:04:44（北京时间）；stable、未撤销、最新Release，平台回读与本地hash一致，0.1.9仍有效。
- 发布内容：第3.1节执行图/计数/跨需求详情与导航修复，第3.2和3.4节的窄Skill指导，以及相关回归；不是整个后续优化方案。
- 验证：typecheck、26项测试、pack验证和解包包的10工具/5 Skills门控均通过。另用Node20.18.1运行26项测试全部通过；首次额外运行发现测试fixture未复制package.json，补齐真实包所含文件后通过。Node24不再作为唯一验证环境。未触发远端CI。
- 平台沿用现有publisher、license、visibility、featured和stable通道；权限未增加，没有发布到npm公共仓库。认证只在内存使用，发布回读后清除令牌。
- 候选包：`/tmp/chatos-solution-release.1gEdid/chatos-solution-studio-0.1.10.tgz`；解包核验目录为同级`unpacked/package`。
- 本机升级已完成：用户用原账号重新登录后，通过唯一正式客户端`/Applications/ChatOS.app`的“插件与 Skills”→“更新”安装0.1.10。客户端显示“Plugin 已完成校验并安装到本机”；安装记录时间为2026-10-14 09:16:45（北京时间），Release ID与artifact hash同上述平台发布记录。逐文件核对29个文件与原安装完整性记录全部一致，三个manifest均为0.1.10。没有用管理端admin替换客户端账号、改写安装记录或启动第二份App。
- T004安装版页面交互验收尚未完成：用户正在使用其他ChatOS窗口，界面控制暂停；随后用户提出新的图片读取阻塞问题，转入该问题诊断。待恢复只读验收，不把已安装当成图交互验收完成。
