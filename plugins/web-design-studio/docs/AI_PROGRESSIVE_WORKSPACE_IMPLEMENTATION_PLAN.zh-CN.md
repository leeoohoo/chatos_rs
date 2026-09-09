# Web Design Studio 3.0.1：AI 渐进式设计工作区实施方案

## 1. 本轮结论

这次不把目标定义为“再做一个更像 Figma 的界面”，而是同时重做工作区模型和 AI 执行模型。

必须同时满足四个原则：

1. AI 可以先规划整站，但一次只执行一个有明确边界、可以视觉验证的设计步骤；画板和页面都不是单次必须完成的单位。
2. 每个步骤独立生成、布局、截图、检查、提交和恢复，任何失败不能污染已经确认的设计。
3. 多画板只是组织和预览能力，不代表要求 AI 同时生成多个页面或多份响应式数据。
4. 设计文档和视觉结果是主产物，代码导出与旧数据兼容都不作为本轮约束。

还必须明确：画板是持续迭代的设计对象，不是一次调用、一次会话甚至一次连续任务必须完成的对象。页面、弹窗、抽屉、菜单、浮层和界面状态都可以经过骨架、局部内容、视觉塑造、响应式修复和最终抛光等多轮 AI 工作。AI 每一轮只承诺完成当前有界步骤，并读取当前 Scene、最新真实渲染图片和历史步骤，在已有结果上继续，而不是重新生成整个画板。

产品核心是视觉设计，不是交互原型或前端功能演示。AI 首先需要把网站设计得有明确的信息层级、构图、品牌气质、排版、色彩、图片策略、留白和节奏；只有视觉方案达到验收门槛后，才补充确有必要的交互状态。按钮能够点击、Drawer 能够打开，不能替代页面设计质量。

本方案不保留旧编辑器数据模型的兼容分支，不创建 v1/v2 双写，不用旧页面模板兜底。

## 2. 当前基础与真实缺口

### 2.1 可以继续使用的基础

- Scene Graph 已有 Page、Section、Frame、Group、Text、Shape、Media、Library Instance 和 Component Instance。
- Scene Transaction 已有 revision、原子修改、字段保护、undo/redo 和 Diff 基础。
- 布局引擎已有 Auto Layout、Grid、Hug/Fill/Fixed、Constraints、连续视口和浏览器校准。
- Design Brief、Design Spec、分区步骤、视觉质量报告、定向修复和批注任务已有数据协议。
- projectId 已由宿主运行时透传并参与项目隔离；MCP 参数不接受模型自行传入 projectId。
- 新工作区 Shell、独立面板、组件运行时边界和基础平移交互已经开始拆分。

### 2.2 尚未形成产品闭环的部分

- 当前中心区域仍是一个有限的大滚动层，不是真正的 Camera + World 坐标工作区。
- 当前编辑器主体仍读写旧 `components[]` 文档，Scene v2 尚未成为实际 UI 的唯一数据源。
- 当前 `executeDesignGenerationPlan` 一次执行全部步骤，不能暂停在单一区块或单一页面。
- 当前生成事务先写入仓库，再进行布局、截图和评价；验证失败时 Scene 已产生 revision。
- 当前计划只有 `pending/completed/failed/blocked`，没有等待确认、重试、暂停、跳过、回滚和过期状态。
- 当前计划没有独立持久化仓库，刷新后无法可靠恢复“正在生成哪个页面、哪个区块”。
- MCP 仍以文档读写和节点 Patch 为主，缺少面向 AI 的高阶渐进式工具。
- Skill 虽然写了“按页面、按区域”，但主要依赖提示约束，服务端没有硬性阻止整站批量生成。
- 当前 AI 上下文仍偏结构化数据，缺少“整页截图、目标裁剪、节点边界映射、前后视觉 Diff”共同参与决策的视觉协议。
- 当前组件和交互能力容易让 AI 把任务做成后台界面或前端 Demo，缺少“先完成视觉设计、后补必要交互”的执行门槛。
- 人工修改保护目前有节点锁和字段锁，但缺少“人工改过、AI 可以提议但不能静默覆盖”的软保护层。
- AI 任务 UI 尚未展示计划树、当前步骤、产物、失败原因和恢复动作。

所以当前状态不是“只差把多画板画出来”，而是底层模块已有较多基础，但工作区、执行编排、MCP、Skill 和审阅 UI 尚未接成同一个产品。

## 3. 目标产品模型

```text
ChatOS Scope
  └─ Project                         项目归属，projectId 由宿主透传
      └─ Design Document             一份网站设计文件
          ├─ Site Plan               页面清单和生成计划，不等于立即执行
          ├─ Artboard                页面、弹层或界面状态的长期设计面
          │   └─ Canonical Scene     该画板唯一可编辑节点树
          │       └─ Root Frame
          │           └─ Section / Frame / Component / Text / Media...
          ├─ Workspace Layout        画板在自由工作区中的摆放
          └─ Generation Runs         可暂停、恢复和审阅的 AI 步骤记录
```

### 3.1 Project

- Project 只负责归属、隔离和设计列表，不参与页面布局。
- `projectId` 只能从 ChatOS 运行时获得，并沿 Project、Document、Plan、Run 和 Artifact 全链路保存。
- MCP 工具不提供 `projectId` 入参；服务端每次读取和写入都验证 documentId 属于当前 projectId。
- URL 中的 `studio-project` 只用于工作台定位，不能成为服务端授权依据。

### 3.2 Document 与 Artboard

- 一个 Document 表示同一网站或同一设计方案。
- Artboard 表示一个逻辑页面、Modal、Drawer、Popover、Menu 或界面状态，而不是一个设备截图。
- 整站规划可以列出多个 Artboard，但规划本身不批量创建其内容。
- 同一时刻最多只有一个有界 Step 处于 `generating/validating`；画板可以在安全步骤之间暂停、切换和稍后恢复，不要求先一次完善当前画板。

### 3.3 Canonical Scene 与响应式检查

- 每个 Page 只有一份 Canonical Scene，不复制 desktop/tablet/mobile 三套节点。
- Root Frame 的宽度由当前预览视口输入，内容高度由布局引擎求解。
- Desktop、Tablet、Mobile、4K 和自定义宽度都是当前画板的响应式检查输入，不默认创建额外画板。
- Breakpoint 只记录确实需要变化的规则：可见性、顺序、布局和变量 Mode。
- 画板外框始终跟随求解结果；内容高度增加时画板向下增长。
- 画板宽度由预览视口或用户明确设置，不因某个错误溢出的子节点自动变宽；这种情况应成为布局错误并进入修复步骤。

### 3.4 Workspace、Artboard 与 Section

工作区和网页内容必须分离：

- Workspace 使用世界坐标，承载页面、弹层、菜单和界面状态 Artboard，以及组件拆解区和对比区。
- Artboard 引用一个独立设计面；`viewportWidth` 是该画板当前的检查宽度，不代表设备副本。
- Workspace Section 只用于在画布上框住和命名一组 Artboard，不进入网页 DOM。
- Scene Section 是页面内部的语义区块，例如 Hero、Features 和 Footer；它属于网页内容。
- Group 让若干节点共同移动，边界跟随子节点。
- Frame 是有独立尺寸、布局、裁切和约束的真实容器。

这样可以同时解决“多页面并排看”“一个页面逐步生成”和“响应式不复制数据”。

## 4. 自由工作区实现

### 4.1 Camera 模型

废弃用 4800×3600 DOM 模拟无限画布的方式，改成稳定 Camera：

```ts
interface WorkspaceCamera {
  x: number;
  y: number;
  zoom: number;
}

interface WorkspacePlacement {
  artboardId: string;
  pageId: string;
  surfaceKind: 'page' | 'modal' | 'drawer' | 'popover' | 'menu' | 'state';
  viewportWidth: number;
  viewportHeight: number;
  worldX: number;
  worldY: number;
  label: string;
}
```

- 平移只改变 Camera，不改 Scene 节点位置。
- 缩放以鼠标指针为锚点，缩放前后的世界坐标保持一致。
- 节点拖动使用 Page 局部坐标；画板拖动使用 Workspace 世界坐标，两者不能混用。
- Camera 和 WorkspacePlacement 独立持久化，不进入页面响应式布局。
- 大量画板使用可见区域裁剪和虚拟化，不渲染视口之外的第三方 iframe。
- 提供“适应选择”“适应当前画板”“显示全部画板”“100%”四个明确动作。

### 4.2 画板布局

- 新页面、弹窗、抽屉、浮层、菜单或界面状态先创建空 Artboard，并出现在工作区可见区域。
- 响应式检查只改变当前画板视口输入；需要比较时后续提供临时对比视图，不污染项目的流程画板清单。
- Artboard 可以独立移动、重命名、聚焦和隐藏，但不能直接改变 Page Scene。
- Root Frame 求解高度变化后，Artboard 外框同步变化，周围画板按用户选择保持位置或自动整理。
- Drawer、Modal、Popover、Menu 和重要 Tabs 状态优先作为独立画板设计；普通组件 Slot 仍使用中心工作区的聚焦编辑。

### 4.3 选择系统

- 单击选择当前层级，双击或 Enter 进入子层，Shift+Enter 返回父层。
- Command/Ctrl 单击执行深度选择；重叠节点显示候选层列表。
- 框选和多选结果可以 Group、Frame 或创建 Auto Layout。
- 选中轮廓、控制点和名称标记位于独立 overlay 层，不得遮挡组件交互。
- “点击选择、按住拖动”等教学提示只在首次空状态出现，不长期覆盖组件。

## 5. AI 渐进式生成状态机

### 5.1 默认工作流

```text
读取当前 Scope 和 Document
  → 生成 Site Plan 草案（只规划，不改 Scene）
  → 选择当前 Page
  → 生成 Page Plan
  → 创建空 Root Frame
  → 生成当前 Section 候选
  → 布局求解
  → 多视口截图与质量检查
  → 通过后提交，或失败后修复/重试
  → 当前 Section 确认
  → 下一个 Section
  → 当前 Page 验收
  → 停止并决定是否进入下一个 Page
```

复杂页面不要求按上述流程一次走完。一个 Page 可以包含多轮 Pass：

```text
Structure Pass       页面骨架、内容层级和主要容器
  → Section Passes   Hero、导航、内容区、表单、Footer 等逐块生成
  → Visual Pass      排版、色彩、图片、间距、材质和品牌表达
  → Design Gate      整页视觉验收；未通过则继续 Visual/Polish Pass
  → Interaction Pass Hover、Focus、Drawer、Modal、Tabs 等状态
  → Responsive Pass  多视口重排、裁切、密度和触控尺寸修复
  → Polish Pass      基于整页截图处理节奏、重复感和局部不协调
```

这些 Pass 不是固定要求全部执行，也不是一次模型调用。Plan 根据页面复杂度拆出必要任务；每个任务可以只处理一个 Section、一个容器、一个交互状态或一个视觉问题。AI 在下一轮继续读取已经提交的页面结果。

硬性规则：

- Site Plan 可以包含多个页面，但不能因此批量创建所有页面内容。
- 同一时刻只允许一个 Page Run 为 `running`。
- 同一时刻只允许一个 Section Step 为 `generating/validating/awaiting-review`。
- 一次 `run_next_step` 最多推进一个有边界的设计任务，不循环执行整个 Plan。复杂 Section 也必须继续拆分。
- 默认在当前 Page 完成后停止；进入下一 Page 必须产生新的明确动作。
- 用户可选择“自动完成当前页面”，调度器仍拆成多个独立作业，不把整页塞进一次模型调用。
- 不提供默认“自动完成整站”。将来若提供，也只能显式开启，并继续使用相同的逐页逐区块状态机。
- 页面未通过 Design Gate 前不能因为“交互能运行”而进入完成状态。
- Interaction Pass 默认不是必需步骤；只有 Brief、组件状态展示或用户请求确实需要时才创建。

### 5.2 状态定义

```ts
type PlanStatus =
  | 'draft'
  | 'ready'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'cancelled';

type StepStatus =
  | 'planned'
  | 'ready'
  | 'generating'
  | 'validating'
  | 'awaiting-review'
  | 'accepted'
  | 'rejected'
  | 'retryable'
  | 'blocked'
  | 'stale'
  | 'skipped'
  | 'rolled-back';
```

每个 Step 保存：

- projectId、documentId、pageId、sectionKey；
- baseRevision、candidateRevision 和 committedRevision；
- 允许修改的目标根、依赖节点和受保护字段；
- Prompt 摘要、结构化输入和模型执行记录；
- Candidate Transaction、Scene Diff；
- 每个视口的 Layout、整页 Snapshot、目标区域 Crop、节点边界映射、视觉 Diff、Calibration 和 Quality Report；
- 重试次数、失败类型、恢复点和前后步骤依赖。

### 5.3 执行模式

- `guided`：每个 Section 通过验证后等待人确认。
- `auto-current-page`：通过验证且未碰到人工保护字段时自动提交当前 Section；当前 Page 完成后停止。
- `review-sensitive`：AI 可连续提出候选，但任何涉及人工改动、删除或结构重排的步骤必须等待确认。

新建网站默认使用 `auto-current-page`，人工批注修改默认使用 `review-sensitive`。两种模式都不会跨页面自动继续。

## 6. 视觉上下文是 AI 的主要输入

AI 不能只根据“页面包含 Header、Card、Button”这种结构摘要判断设计质量。Scene Graph 负责可编辑性和精确修改，真实渲染图片负责让 AI 理解页面实际上看起来怎样，两者必须同时提供。

### 6.1 每次设计任务的视觉输入

根据任务范围，AI 至少获得以下视觉产物：

- 当前视口的整页截图，用于判断页面整体层级、节奏和风格一致性；
- 当前目标 Section 或节点的高分辨率裁剪，用于判断排版、间距、图像裁切和组件状态；
- 带稳定 nodeId 和边界框的视觉映射，但标记层与纯净截图分开保存，避免污染视觉判断；
- 上一次确认结果与当前候选结果的 before/after 图片；
- 像素 Diff 或视觉差异区域，用于确认修改是否只影响目标范围；
- 当前要求覆盖的响应式视口截图，而不是只有宽高和节点列表。

结构信息仍然需要提供，但只提供当前任务所需的语义树、样式 Token、布局意图、组件 Contract、人工锁定字段和相关依赖，不能用整份文档 JSON 挤占图片上下文。

### 6.2 视觉定位协议

截图需要能反向定位 Scene 节点：

```ts
interface VisualGroundingArtifact {
  snapshotId: string;
  documentId: string;
  pageId: string;
  revision: number;
  viewportWidth: number;
  imageWidth: number;
  imageHeight: number;
  nodes: Array<{
    nodeId: string;
    role?: string;
    rect: { x: number; y: number; width: number; height: number };
    visible: boolean;
    zOrder: number;
  }>;
}
```

AI 可以根据图片中的区域、坐标或视觉问题找到候选 nodeId，再读取有限节点树。人也可以直接在截图或画布上框选区域、画箭头、圈出问题或添加文字批注，这些视觉标记会和命中的节点、坐标范围、视口及截图 revision 一起进入 AI Task。

### 6.3 视觉闭环

```text
当前确认截图 + 当前 Scene 摘要 + 用户视觉批注
  → AI 产生有边界的设计意图
  → Candidate Scene
  → 候选截图
  → AI 对比 before / after / diff
  → 结构、布局和视觉质量检查
  → 提交、修复或等待审阅
```

AI 必须能够判断“元素都存在但画面不好看”的问题，例如层级不清、留白失衡、视觉焦点错误、图片风格冲突、卡片过度重复、页面像后台模板以及移动端节奏不自然。这类判断不能只靠 Scene 规则完成。

### 6.4 Design Gate

页面进入交互补充或最终完成前，必须先通过独立的视觉设计门槛：

- 页面有清晰的第一视觉焦点和阅读顺序；
- 构图符合网站类型，不默认退化为后台侧栏、数据卡片和表单堆叠；
- 字体层级、行长、行高和文字密度适合真实内容；
- 色彩、圆角、阴影、材质和图片使用属于同一视觉方向；
- 留白和区块节奏在整页截图中成立，不只是在单个组件内成立；
- 重要内容不是由组件库默认样式决定，组件已经服从页面设计系统；
- 页面与同一项目的其他页面保持品牌一致，但不是机械复制同一个模板；
- 所需视口下仍保留信息层级和视觉节奏。

Design Gate 主要依据整页和局部图片进行判断，结构校验只负责确认节点可编辑、布局合法和语义完整。交互覆盖率、按钮数量、路由数量以及“能够点开弹窗”都不计入视觉完成度评分。

## 7. 候选事务、提交与恢复

当前“先落库、后验证”的顺序需要改为：

```text
读取 baseRevision
  → AI 生成 Candidate Transaction
  → 作用域和人工保护校验
  → 在临时候选 Scene 中应用
  → 求解布局
  → 渲染多视口截图
  → 浏览器校准和视觉质量检查
  → 通过：原子提交正式仓库
  → 失败：丢弃候选或基于候选创建修复尝试
```

### 7.1 原子性

- 验证失败不增加正式 Scene revision。
- 正式提交时再次检查 baseRevision；若人已修改文档，Step 标记为 `stale`，不得覆盖。
- 每个已提交 Step 对应一个可定位的 Scene Transaction 和一个恢复点。
- 回滚只回滚该 Step 的 Diff，不回退其他人在之后完成的无关修改。

### 7.2 人工修改保护

引入两级保护：

- Hard Lock：用户显式锁定，AI 永远不能修改。
- Soft Protection：人工修改过的字段，AI 可以提出 Diff，但不能静默提交。

人工修改需要记录字段级来源和 revision。批注明确要求修改某个受保护字段时，UI 在审阅步骤中展示该字段的 before/after，由用户接受后提交。AI 不能自行解除保护。

### 7.3 失败分类

- `generation_error`：模型输出或组件契约不合法；重试当前 Section。
- `scope_violation`：越过目标子树或人工保护；缩小事务并重试。
- `layout_error`：溢出、重叠或约束冲突；进入定向布局修复。
- `render_error`：组件运行时或截图失败；保留候选并重试渲染。
- `quality_reject`：结构合法但视觉不合格；根据 issueIds 修复当前 Section。
- `revision_conflict`：人已修改；重新读取当前 Page，只重建未确认的候选。

失败和重试都不能推翻已经 accepted 的 Section。

## 8. MCP 高阶工具

底层 Scene Query、Transaction 和组件 Contract 继续保留为内部能力；AI 的默认入口改成高阶工作流工具。

### 8.1 上下文与规划

- `web_design_get_active_context`：返回宿主注入的 projectId、当前 documentId、pageId、selection 和 pending requests；不接受 projectId 参数。
- `web_design_plan_site`：生成或更新页面清单，只写 Plan Store，不修改 Scene。
- `web_design_plan_page`：只为一个 pageId 生成 Section 顺序、依赖、视觉和响应式意图。
- `web_design_get_plan`：返回计划摘要、当前步骤、可执行动作和失败信息。

Page Plan 必须先保存 `artDirection`、`compositionIntent`、`typographyIntent`、`imageStrategy`、`contentHierarchy` 和 `designAcceptanceCriteria`。交互需求单独放入可选的 `interactionIntents`，不能反过来主导页面结构。

### 8.2 执行

- `web_design_start_page`：创建一个空页面 Root Frame，并将该 Page 设为唯一活动生成页。
- `web_design_run_next_step`：只执行当前 Page 的一个 Section Step，返回候选或已提交结果。
- `web_design_retry_step`：只重试指定 Step，不重放已确认步骤。
- `web_design_repair_step`：根据当前 Step 的 issueIds 定向生成一个修复候选。
- `web_design_pause_plan` / `web_design_resume_plan`：持久化暂停和恢复。

### 8.3 视觉读取与定位

- `web_design_capture_page`：生成当前 Page 指定视口的纯净整页截图。
- `web_design_capture_region`：按 nodeId、Section 或画布框选区域生成高分辨率裁剪。
- `web_design_get_visual_grounding`：返回截图坐标与稳定 Scene nodeId 的映射。
- `web_design_compare_snapshots`：返回 before/after 图片、视觉差异区域和受影响节点。
- `web_design_inspect_at_point`：根据截图坐标返回重叠节点候选和有限祖先路径。

截图工具返回可供模型直接查看的图片产物，不能只返回本地路径字符串或结构描述。

### 8.4 审阅与恢复

- `web_design_inspect_step`：读取 Diff、截图、质量报告和受保护字段冲突。
- `web_design_accept_step`：提交已通过验证的候选。
- `web_design_reject_step`：丢弃候选并记录原因。
- `web_design_skip_step`：显式跳过非必需区块，必需区块不可跳过。
- `web_design_rollback_step`：回滚一个已提交步骤，并将后续依赖步骤标记 stale。
- `web_design_complete_page`：只有当前 Page handoff 验证通过时才完成。

### 8.5 服务端强制约束

- 工具描述不是唯一防线；Plan Store 和 Executor 必须在服务端执行状态转换校验。
- `run_next_step` 不能在一次调用中跨过两个 Section。
- 生成工具必须绑定当前 Scope、Document、Page 和 baseRevision。
- projectId 只能从 runtime scope 注入，所有返回产物都带 scope 摘要用于审计。
- 对同一 `stepId + attempt` 使用幂等键，网络重试不能重复插入节点。
- 原始批量 Patch 不作为新网站生成的默认工具，只允许聚焦修复或内部执行器调用。
- 每个生成或修复步骤必须引用当前 revision 的视觉输入；过期截图不能用于提交候选。

## 9. Skill 重构

Skill 只能引导模型，关键限制仍由工具强制。Skill 调整如下：

### 9.1 主 Skill

`web-design-studio` 固定执行顺序：

1. 激活后先调用 `web_design_get_active_context`。
2. 读取文档 outline 和当前 Plan，不得直接创建整站节点。
3. 没有 Plan 时先 `plan_site`；有 Plan 时优先恢复当前 Step。
4. 一次只处理工具返回的 `nextAction`。
5. 当前 Page 未完成前不得启动下一 Page。
6. 生成前读取当前页面截图和目标裁剪；变更后读取候选截图及视觉 Diff。
7. 不得只依据节点清单、工具调用成功或结构校验宣告完成。

### 9.2 新增渐进式工作流 Skill

新增 `web-design-progressive-generation`，作为所有规划和生成工具的必需 Skill Gate，专门规定：

- 规划不等于执行；
- 一页一页、一区块一区块；
- 当前步骤失败只修当前步骤；
- 页面验收后必须停止在页面边界；
- 人工调整和批注优先于旧计划；
- 禁止把导航、表单、卡片或整页压进一个文本节点；
- 禁止为了减少工具次数而扩大事务范围。
- 页面复杂时继续拆分同一个 Page 的任务，不得退化成一次性整页生成。
- 每次视觉决策同时使用图片和有限结构上下文。

### 9.3 现有叶子 Skill

- Documents：负责复用文档、选择当前 Page、读取请求和 revision 冲突恢复。
- Components：只为当前 Section 搜索组件并读取 Contract，不加载整个组件库。
- Responsive Layout：对当前候选执行所需视口求解，不复制设备节点。
- Visual System：在 Site/Page Plan 中建立方向，并约束当前 Section，不每次重新发明风格。
- Validation：成为 Step 提交和 Page 完成的硬门槛。

## 10. 编辑器 UI

### 10.1 AI 任务侧栏

AI 区域不再只是输入框，必须展示：

- Site Plan 页面树；
- 当前 Page 和当前 Section；
- planned、running、review、accepted、failed 状态；
- 当前步骤截图、Diff、质量分和错误原因；
- “继续当前步骤”“自动完成当前页面”“重试”“修复”“接受”“拒绝”“暂停”“回滚”；
- 明确提示“完成当前页面后会停止”。
- 把“视觉设计”作为主进度，将“交互状态”放在通过 Design Gate 后的可选阶段。

按钮必须使用动作名称和结果说明，不能只放难以理解的图标。

### 10.2 画布反馈

- AI 正在生成的 Artboard 显示非阻断状态条，不在整个画布上覆盖半透明遮罩。
- 当前 Section 使用边框和状态标记定位；其他已完成区域仍可浏览。
- Candidate 以可切换的 before/after 或并排 Diff 展示，不直接替换正式画布。
- 失败时保留上一个已确认画面，并在问题节点旁显示 issue 标记。
- 人可以在整页截图或画布上框选、圈画和写批注，AI Task 同时保存图片标记与命中节点。

### 10.3 页面与画板

- 左侧 Page 列表负责逻辑页面切换。
- 工作区可以同时展示多个 Page Artboard，但 AI 任务侧栏只标记一个 Active Page。
- 响应式检查直接改变当前画板的 CSS 视口输入，不把设备预览加入流程画板。
- 空页面显示“规划此页面”和“开始生成第一个区块”，不自动生成所有页面。

### 10.4 内部内容编辑

- Drawer、Modal、Card Slot、Tabs Panel 等进入聚焦编辑时，仍显示在中心工作区。
- 聚焦对象的画布宽高根据其 Slot/内容求解结果增长。
- 面包屑显示 `Page / Component / Slot`，用户可以返回外层。
- 所有内部节点保持独立可选，不要求先插入新的占位内容。

## 11. 实施阶段

每个阶段完成验收后再进入下一阶段，不并行堆 UI 和组件数量。

### 阶段 A：协议收口和持久化计划（已完成）

实施：

- 新建 `generation-plan-schema.ts`、`generation-plan-store.ts` 和 `generation-state-machine.ts`。
- 将 Site Plan、Page Plan、Step、Attempt、Artifact 和状态转换正式建模。
- Page Plan 区分 Design Tasks 与可选 Interaction Tasks，并建立 Design Gate。
- projectId/documentId/pageId/baseRevision 全链路校验。
- 规定一个 Document 只能有一个 running Page 和一个 active Step。

验收：

- 刷新或进程重启后可恢复当前步骤。
- 非法跨页、跨 Scope、越级状态转换全部被拒绝。
- 同一幂等键重复调用不产生重复事务。

完成记录见 `V3_PHASE_A_GENERATION_PLAN.zh-CN.md`。当前定向测试 14/14、插件全量测试 195/195 通过。

### 阶段 B：候选执行器和单步提交（已完成）

实施：

- 将现有全计划循环执行器拆成 `prepareStep → generateCandidate → validateCandidate → commitStep`。
- Layout、Snapshot、Calibration 和 Quality 在正式 commit 前完成。
- 接入整页截图、目标 Crop、视觉 Grounding 和 before/after Diff 产物。
- 增加 reject、retry、stale、rollback 和 soft protection。

验收：

- 在生成、布局、渲染、质量检查各阶段注入失败，正式 Scene revision 均不变化。
- 重试当前 Section 不改变任何 accepted Section。
- 人工修改后旧候选不能提交。
- 只提供节点清单而没有当前视觉产物的步骤不能进入视觉验收。

完成记录见 `V3_PHASE_B_CANDIDATE_EXECUTION.zh-CN.md`。当前 TypeScript strict typecheck、生产构建和插件全量测试 `212/212` 均通过。

### 阶段 C：高阶 MCP 工具（已完成）

实施：

- 接入第 8 节工具并为每个工具配置 Skill Gate。
- 旧节点 Patch 降为内部/高级修复入口。
- 工具结果只返回当前步骤需要的上下文、稳定 ID 和下一动作。

验收：

- AI 可用小于完整文档的上下文完成一个页面。
- 任一生成工具一次最多产生一个 Section Transaction。
- MCP schema 中不存在可伪造的 projectId。
- AI 可以用整页图、局部图和视觉 Diff 精确定位并修改节点。

已完成上下文、Site/Page 规划、单页启动、单步执行/重试/修复、Candidate 审阅/接受/拒绝、跳过、精确 Scene 回滚、页面完成和暂停/恢复入口；并接入 Chromium 整页/局部 PNG、Visual Grounding、before/after Diff 和点选定位。图片通过 MCP image content 直接返回，Artifact 按宿主 Scope 持久化隔离。完成记录见 `V3_PHASE_C_PROGRESSIVE_MCP.zh-CN.md`。

### 阶段 D：Camera 与流程画板工作区（进行中）

实施：

- 用 Camera + CSS transform/world coordinate 替换固定大滚动层。
- 新建 WorkspacePlacement Store、流程 Artboard 和虚拟化。
- 接入适应选择、适应画板、显示全部、指针锚点缩放。
- 响应式宽度作为单画板检查输入，不生成默认设备画板。

验收：

- 在任意方向连续平移和 10%–800% 缩放无跳动。
- 移动画板不改变页面内部节点坐标。
- 页面、弹层和状态画板可以并排组织，并保持各自稳定设计内容和 World 坐标。
- 页面内容增长时画板外框正确增长，保存刷新后位置不变。

当前已删除固定 4800×3600 滚动层，接入 10%–800% Camera、指针锚定缩放、任意方向平移和按宿主 Scope 隔离的 Workspace Placement Store；默认设备三画板已撤除，工作区改为页面、弹窗、抽屉、浮层、菜单和界面状态的独立流程画板，支持选择、创建、移出、World 坐标移动、适应选择、适应当前画板和显示全部。Scene 节点到目标画板的原型关系已经进入正式 Scene schema、事务、复制、删除、Undo/Redo、属性栏、World 坐标贝塞尔箭头和预览导航/叠层渲染，不再读取旧组件交互作为 Scene 主路径。Form、Card、Drawer、Modal 等内部内容继续使用统一 Scene 容器、Camera、选择和布局能力。大量画板按 `anchor / shell / content / runtime` 四级距离分段挂载，离屏页面不会保留完整普通节点或第三方运行时。阶段 D 已完成，进度记录见 `V3_PHASE_D_CAMERA_WORKSPACE.zh-CN.md`。

### 阶段 E：Scene v2 编辑器接入

实施：

- 编辑器只读写 Scene v2，移除旧 `components[]` 主路径。
- 接入层级选择、深度选择、框选、多选、Group、Frame、Auto Layout 和变换手柄。
- 右属性栏改为 Scene 属性与字段级来源。
- Drawer/Modal/Slot 聚焦编辑共用同一套 Scene 选择和布局能力。

验收：

- 所有可见对象均能在最多三次操作内选中。
- 手工移动、缩放、分组和内部编辑保存刷新后像素与层级不变。
- 不存在为了兼容旧文档而走的双写分支。

阶段 E 已完成：实际画布、图层树、选择、工具栏、右属性栏、官方组件插入、批注、原型和响应式人工覆盖都读写 Scene v2；Move、八方向 Resize、Group、Frame、Auto Layout Frame、Ungroup、Align、Distribute、Reorder、复制画板、删除画板和 Undo/Redo 都通过同一套 revision-safe Scene Transaction。第三方 iframe 运行时在设计态有独立选择命中层，在原型态有独立关系命中层。旧 `components[]` 只保留为未创建 Scene 的历史文档表面，不参与 Scene 双写、渐进生成、Candidate、视觉验收或 Scene handoff。进度记录见 `V3_PHASE_E_SCENE_EDITOR.zh-CN.md`。

### 阶段 F：AI 计划、审阅和恢复 UI

实施：

- 实现 Plan Tree、Step Detail、Candidate Diff、截图和恢复动作。
- 接入 guided、auto-current-page 和 review-sensitive。
- 页面边界停止、失败定位和人工保护冲突可视化。

验收：

- 用户能看清 AI 正在做哪一页、哪一区块、为什么失败以及下一步会发生什么。
- 不阅读说明书也能完成继续、重试、接受、拒绝、暂停和回滚。
- AI 生成时不遮挡整个画布，已确认设计始终可查看。

当前阶段 F 已完成 Plan/Step 状态读取、Candidate 截图与 Diff 审阅、接受、拒绝、暂停、恢复、回滚、重试入口、页面边界停止和人工保护冲突展示。生成状态作为右侧独立审阅面板，不替换或遮挡正式 Scene 画布。

### 阶段 G：Skill、真实任务和生产验收

实施：

- 重写主 Skill 并新增渐进式生成 Skill。
- 用 12 类网站重跑逐页逐区块生成，不再用一次性整站调用。
- 增加崩溃恢复、安装包、WebView、长任务和大文档测试。

验收：

- AI 能先规划 5 个页面，但只生成用户指定的当前页面。
- 当前页面由多个独立 Section Step 完成，每步都有截图、Diff 和质量记录。
- 复杂页面可以跨多轮模型调用持续完善，不需要在一次调用中生成完整页面。
- 任一步失败可恢复，已确认页面不受影响。
- 人的手工修改和批注可驱动定向 AI 修改，不发生静默覆盖。
- 通过安装后的客户端真实调用验证，而不只在开发浏览器中通过。

当前阶段 G 的 Skill、MCP 强约束、12 类网站结构基准、渐进式多轮生成、截图与视觉 Grounding、失败不污染正式 Scene、人工批注和 projectId 隔离均已实现并进入自动化测试。发布前最后一步是完成插件校验、缓存版本更新、客户端重新安装及真实 projectId 透传冒烟验收。

## 12. 测试矩阵

### 12.1 数据与状态

- Plan schema、状态转换、依赖拓扑、幂等、暂停恢复、过期候选和回滚。
- projectId 透传、项目归属、documentId/pageId 绑定和越权拒绝。
- 人工 Hard Lock、Soft Protection 和批注明确授权。

### 12.2 AI 执行

- 空文档规划、多页面规划但单页执行、单区块生成、失败重试和页面边界停止。
- 模型输出截断、组件 Contract 错误、revision conflict 和超时恢复。
- 保证模型不需要传入大量 x/y 坐标或整个文档 JSON。
- 验证一个复杂页面能够经过多轮模型调用增量完成，并且每轮只读取有限结构和必要图片。
- 给 AI 相同内容但不同视觉 Brief，验证输出首先产生不同构图和视觉系统，而不是只改变交互组件。

### 12.3 工作区

- Camera 平移缩放、画板移动、多个视口、虚拟化、聚焦编辑和选择 overlay。
- 320、390、768、1024、1280、1440、1920、2560、3840、7680 CSS px。
- 保存、强制刷新、关闭重开和崩溃恢复后的像素稳定。

### 12.4 视觉与交互

- 12 类不同网站结构，避免相同卡片网格和后台管理模板同构。
- 第三方组件使用真实运行时，交互预览和设计编辑模式互不冲突。
- Drawer、Modal、Popover、Select、Tabs 和表单内部内容可独立设计和选择。
- 图片批注、区域框选、视觉定位和截图 revision 过期检查完整可用。
- 视觉未通过时，即使所有按钮、弹窗和路由都可交互，页面仍不能通过 Design Gate。

## 13. 实施纪律

- 本轮暂停继续扩充组件库数量，先完成编辑、组合和 AI 工作流。
- 不把多画板叫作“AI 一次生成多页面”。
- 不再使用“无限画布”描述有限滚动层，产品文案统一为“自由工作区”。
- 不通过仿写第三方组件填数量；继续使用允许集成的真实实现和统一 Contract。
- 不以测试数量、菜单存在或 Demo 截图作为完成标准。
- 每一阶段必须同时包含协议、实现、自动化测试、真实浏览器验证和可恢复性验证。
- 不把 Scene 元素齐全误判为视觉设计完成；最终判断必须查看真实渲染图片。
- 不用交互数量冒充设计质量；默认先做静态视觉设计，必要交互在视觉验收后补充。

## 14. 3.0.1 发布收尾顺序

1. 运行 TypeScript、全量测试、打包 dry-run 和插件 manifest 校验。
2. 清理只用于浏览器验收的临时设计数据。
3. 用插件更新脚本刷新本地 marketplace cachebuster，不手改 marketplace 配置。
4. 重新安装到客户端，验证传入 projectId、项目归属、设计打开、Scene 保存、AI Plan 恢复和截图产物。
5. 只提交 `plugins/web-design-studio` 范围的 3.0.1 改动，避免带入工作树中其他客户端开发内容。
