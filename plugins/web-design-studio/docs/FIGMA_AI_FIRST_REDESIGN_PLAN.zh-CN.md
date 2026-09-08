# Web Design Studio v2：Figma 启发的 AI-first 重构方案

## 1. 重新定义产品

Web Design Studio 不是组件库浏览器，也不是以导出 React、Vue、HTML 为中心的低代码工具。

它的核心目标是：

1. AI 根据网站目标、内容、品牌和参考资料，生成结构正确、视觉完整、可响应的网站设计；
2. 人在画布中审阅结果，完成少量移动、缩放、换字、换图和属性调整；
3. 人可以圈选或批注任意元素，把修改意图继续交给 AI；
4. AI 能通过稳定语义 ID、设计层级和视觉回传，精确理解并修改已有设计；
5. 设计文档始终是第一产物，代码只是可选交付物。

这意味着产品的主路径应当是：

```text
设计 Brief → AI 建立设计规范 → AI 分区生成 → 渲染检查 → AI 自我修正
                                                ↓
                                      人审阅、微调、批注
                                                ↓
                                          AI 精确迭代
```

## 2. Figma 调研结论

Figma 当前编辑器并不是简单的“左组件、中画布、右属性”。它由五个长期稳定的工作区域组成：

- 最左侧 Navigation Bar：File、Agents、Assets、Tools、Variables 等核心入口；
- 动态 Left Sidebar：根据当前入口显示页面、图层、组件、变量或 AI 内容；
- 可滚动的连续 Canvas：承载多个顶层 Frame、Section 和嵌套内容；
- 底部 Toolbar：移动、Frame、Section、形状、文本、评论、Actions/AI 等高频工具；
- 右侧 Properties Panel：根据当前选择动态显示 Design、Prototype、组件属性和导出设置。

Figma 真正值得借鉴的是以下设计能力：

### 2.1 Frame 是设计的基本容器

Frame 有明确尺寸，可以嵌套，拥有裁切、布局辅助线、Auto Layout、Constraints 和 Prototype 等能力。Group 只是让若干图层一起移动，其边界自动跟随子元素；二者不是同一种容器。

### 2.2 Auto Layout 表达设计意图

Auto Layout 不只是 Flex 的别名。它同时表达：

- 横向、纵向、Grid 流；
- 四边独立 Padding 与横纵 Gap；
- Hug contents、Fill container、Fixed；
- 最小与最大尺寸；
- Wrap、对齐和空间分布；
- Flow 内元素与 Absolute positioned 元素共存；
- 内容增删、文字变化和父容器变化后的自动重排。

### 2.3 选择和层级是核心交互

Figma 通过父级优先选择、双击/Enter 向内选择、Shift+Enter 返回父级、Command/Ctrl 深度选择、框选、多选、图层悬停高亮和 Select Layer 菜单，解决复杂嵌套中的定位问题。

### 2.4 组件不是一张预览图

组件由 Main Component、Instance、Variant 和 Component Properties 构成。可公开的属性至少包括：

- Boolean：显示或隐藏某层；
- Text：修改文本；
- Instance swap：替换嵌套实例；
- Variant：状态、尺寸、颜色等组合；
- Slot：插入、编辑和重排自定义内容。

实例只暴露设计者允许修改的内容，而不是要求用户进入内部删除 Demo 元素。

### 2.5 Variables 是设计系统，不只是主题色

Variables 支持颜色、数字、字符串、布尔、时间和缓动，并通过 Collection、Group、Mode 和 Alias 形成完整设计 Token。Mode 可以表达明暗主题、设备、语言和品牌上下文。

### 2.6 AI 是工作区的一部分

Figma 当前把 Agents 放进左侧 Navigation Bar，把 AI 生成、替换内容、创建图片和其他 Actions 放进工具入口。AI 不是一个偶尔打开的顶部弹窗，而是可以持续读取文件、选区和设计上下文的工作流。

## 3. 当前实现的主要问题

### 3.1 数据模型仍然偏平面

当前文档以扁平 `components[]` 为中心，通过 `parentId` 补充层级。容器只有 `free / flex-row / flex-column / grid`，缺少 Frame、Group、Section 的不同语义，也缺少 Hug、Fill、独立 Padding、横纵 Gap、流内/绝对定位等关键布局信息。

### 3.2 响应式仍是三个截图

当前响应式模型主要保存 desktop、tablet、mobile 三套位置尺寸覆盖。它可以展示三个结果，但不能表达一个布局如何在任意宽度连续变化，因此从宽屏切到窄屏时仍容易出现溢出或大量人工修正。

### 3.3 设计系统太窄

当前 Token 只有少量颜色、三档圆角和基础字体；样式结构也以单个 fill、stroke、shadow 为主。它不足以表达成熟网站中的语义色、间距比例、排版层级、多个效果、渐变、材质、动效和模式。

### 3.4 组件库建设超过了编辑器核心

我们已经投入大量工作同步 Ant Design、Chakra、shadcn、Magic UI、Spell、Inspira 和 daisyUI，但组件的选择、配置、组合、布局、暴露属性和 AI 调用仍受旧模型限制。继续增加组件只会放大这个问题。

### 3.5 UI 是单体工作台

主工作台集中在一个约 2800 行的 React 文件中，项目、组件库、画布、属性、弹窗、AI 和内部编辑混在同一状态树。任何局部交互都容易触发整个工作区重渲染，也不利于后续加入可平移的自由工作区、多画板、工具模式和多选属性。

### 3.6 AI 工具仍偏底层 Patch

现有 MCP 已有 outline、page、node、catalog、batch、patch 和 validate，但 AI 仍需要自己处理大量坐标、尺寸和原始节点字段。系统缺少“设计规范 → 布局求解 → 渲染截图 → 视觉评价 → 修正”的闭环。

## 4. v2 目标工作区

```text
┌────────┬──────────────────┬──────────────────────────────────────┬────────────────────┐
│ 导航栏 │ 动态侧栏         │ 连续画布                             │ 属性面板           │
│        │                  │                                      │                    │
│ 文件   │ 页面 / 图层      │  Section                             │ Design             │
│ AI     │ AI 任务 / Diff   │   ├─ Desktop Frame                   │ Prototype          │
│ 资产   │ 图片 / 图标      │   ├─ Tablet Frame                    │ Component props    │
│ 组件   │ 独立组件库来源   │   └─ Mobile Frame                    │ Variables          │
│ 变量   │ Collection/Mode  │                                      │ AI context         │
│ 工具   │ 搜索 / 插入      │  评论针、间距手柄、布局辅助线       │                    │
└────────┴──────────────────┴──────────────────────────────────────┴────────────────────┘
                         ┌──────────────────────────────┐
                         │ 底部工具栏：选择 Frame 形状 │
                         │ 文本 评论 Actions/AI        │
                         └──────────────────────────────┘
```

### 4.1 顶部区域

顶部只保留项目/文件路径、保存状态、协作状态、预览和交付入口，不再承载大量设计工具。

### 4.2 左侧 Navigation Bar

- File：页面与图层；
- AI：生成任务、当前计划、变更 Diff、失败与重试；
- Assets：图片、视频、图标和上传资源；
- Components：组件与“我的”；
- Variables：Token、Collection 和 Mode；
- Tools：插件级工具和检查器。

组件来源仍然严格分开。AntD、Chakra、shadcn 等在 Components 侧栏内部使用独立来源 Tab/筛选，不把不同设计系统混为一组。

### 4.3 连续画布

- 支持无限平移、缩放和多个顶层 Frame；
- Page 是文件组织单位，Frame 是实际设计画板；
- Section 用于组织方案、状态和交付分区；
- Frame 可自由嵌套；
- 画布尺寸不再等于网页尺寸；网页高度由根 Frame 内容与布局决定；
- Desktop、Tablet、Mobile 是同一响应式设计的预览 Frame，不是三份互不相关的数据。

### 4.4 右侧属性面板

属性按选择动态出现，不长期展示无关字段：

- Layout：位置、尺寸、旋转、Auto Layout、Constraints；
- Component：Variant、Text、Boolean、Instance swap、Slot；
- Appearance：Fill、Stroke、Radius、Effects、Opacity；
- Typography：字体、字重、字号、行高、字距、段落；
- Prototype：触发器、动作、目标、过渡、滚动；
- Variables：绑定 Token 与 Mode；
- AI Context：语义角色、设计意图、锁定字段、相关批注。

## 5. Schema v2：直接重做，不保留双轨兼容

v2 不在旧 schema 上继续叠字段，也不维护 v1/v2 双运行时。旧文档不作为新编辑器的运行格式。

### 5.1 Scene Graph

```text
Document
  └─ Page
      └─ Section
          └─ Frame
              ├─ Frame / Group
              ├─ Text / Shape / Vector / Media
              ├─ Library Instance
              └─ Component Instance
```

每个节点包含：

- 稳定 ID；
- 类型与语义角色；
- parent/children 有序关系；
- frame 与 transform；
- layout、appearance、variables、interactions；
- human lock、AI edit policy 和 annotations；
- createdBy、updatedBy、revision 等来源信息。

### 5.2 Layout Model

```ts
layout: {
  mode: 'free' | 'auto' | 'grid';
  direction?: 'horizontal' | 'vertical';
  wrap?: boolean;
  padding: { top: number; right: number; bottom: number; left: number };
  gap: { row: number; column: number };
  alignItems?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
  justifyContent?: 'start' | 'center' | 'end' | 'between' | 'around' | 'evenly';
  sizingX: 'fixed' | 'hug' | 'fill';
  sizingY: 'fixed' | 'hug' | 'fill';
  minWidth?: number;
  maxWidth?: number;
  minHeight?: number;
  maxHeight?: number;
  position: 'flow' | 'absolute';
  clipContent?: boolean;
}
```

Grid 增加 track、span、minmax 和 auto-fit/auto-fill，不再只保存列数。

### 5.3 Appearance Model

- 多层 fills；
- 多层 strokes 与独立边；
- 多层 shadow/blur；
- 四角独立 radius；
- gradient stops；
- blend mode、mask、clip path；
- 完整 typography；
- hover、focus、pressed、disabled、selected 等可组合状态。

### 5.4 Variables

- Collection；
- Group；
- Mode；
- Alias；
- color、number、string、boolean、duration、easing；
- 任意支持属性可绑定 variableId，不复制最终值。

### 5.5 Component System

- Main Component / Component Set / Instance；
- Variant properties；
- Text、Boolean、Instance swap 和 Slot properties；
- exposed properties；
- instance overrides 按节点路径记录；
- reset、detach、swap、update main、sync instances；
- 第三方库组件也转换成同一份 Property Contract。

## 6. 连续响应式布局

不再把响应式等同于 desktop/tablet/mobile 三套绝对坐标。

新的计算模型：

1. 根 Frame 有可变 viewport width；
2. Auto Layout、Grid、Constraints、min/max 和变量 Mode 共同求解布局；
3. Breakpoint 只保存真正发生结构变化的规则；
4. 任意输入宽度都可以得到布局结果；
5. 人对某个宽度的手工修改，必须明确选择“修改通用规则”或“仅创建该断点覆盖”；
6. 4K/8K 只是预览视口，不产生新的设计副本。

验收必须覆盖 320、390、768、1024、1280、1440、1920、2560、3840 和 7680 CSS px，并检查溢出、文本截断、重叠和最大内容宽度。

## 7. AI-first 操作协议

### 7.1 AI 不直接从坐标开始

新网站生成分为六步：

1. `create_design_brief`：目标、受众、品牌、内容、参考、禁用项；
2. `create_design_spec`：页面结构、内容层级、视觉方向、Token、组件策略、响应式规则；
3. `apply_scene_transaction`：一次只创建一个语义区域；
4. `solve_layout`：由引擎计算位置和尺寸；
5. `render_frame_snapshot`：生成指定视口截图和结构摘要；
6. `critique_and_revise`：根据视觉质量检查结果自动修正，再交给人审阅。

### 7.2 新的核心工具

- `web_design_get_scene_outline`
- `web_design_query_nodes`
- `web_design_inspect_nodes`
- `web_design_create_design_spec`
- `web_design_apply_scene_transaction`
- `web_design_set_auto_layout`
- `web_design_bind_variable`
- `web_design_create_component_set`
- `web_design_set_component_properties`
- `web_design_render_snapshot`
- `web_design_compare_snapshots`
- `web_design_validate_visual_quality`
- `web_design_apply_annotation_batch`

AI 可以按 name、role、type、ancestor、component、variable、annotation 和创建来源查询节点，不要求人提供 ID。

### 7.3 Transaction 与保护规则

- 每次 AI 修改必须是原子事务；
- 返回新增、修改、删除节点列表和视觉摘要；
- 人工修改过的字段默认受保护；
- AI 必须显式申请覆盖 human lock；
- 批注只在对应修改已经存在且通过验证后才能解决；
- 每次事务可接受、拒绝或局部回滚。

### 7.4 AI 视觉闭环

结构校验不能代替视觉校验。每个重要 Frame 需要生成截图并检查：

- 视觉层级；
- 对齐和间距节奏；
- 文本可读性与行长；
- 色彩对比；
- 图片比例与裁切；
- 重叠、溢出、截断；
- 不同视口的一致性；
- 组件库使用是否统一；
- 页面是否过度模板化或像后台管理界面。

## 8. 人的核心操作

人的功能保持克制，但必须可靠：

- 单击选择父级；双击或 Enter 向内；Shift+Enter 返回父级；
- Command/Ctrl 深度选择；
- 框选、多选、对齐、分布、Tidy up；
- 创建 Group、Frame、Section；
- Shift+A 创建 Auto Layout；
- 画布直接调整 Padding、Gap 和排列顺序；
- 拖动与缩放不改变未操作的任何布局；
- 评论针绑定节点、区域或坐标；
- 批注可直接转为 AI 任务；
- 查看 AI Diff，并接受、拒绝或局部保留。

## 9. 分阶段实施顺序

必须完成一个阶段的验收门槛后才能开始下一个阶段。重构期间暂停新增组件库和旧 UI 功能。

### 阶段 0：冻结与基线

工作：

- 冻结新组件集成；
- 建立 12 类网站基准 Brief 和当前输出截图；
- 建立编辑器交互、响应式、保存恢复和 AI patch 的自动化基线；
- 列出可以保留的组件运行时、存储、projectId 透传和 revision 能力。

验收：每个问题都有可复现用例，旧版能力清单不再以“菜单存在”判定完成。

### 阶段 1：Schema v2 与 Scene Graph

工作：

- 新 Document/Page/Section/Frame/Node 模型；
- 有序 children；
- Group、Frame、Section 分离；
- 新 appearance、variables、component properties；
- 新事务、revision、undo/redo 基础。

验收：不依赖 React UI，可以用测试构造复杂嵌套网站并无损保存、读取、撤销。

### 阶段 2：布局与响应式引擎

工作：

- Auto Layout 横向、纵向、Wrap；
- Grid tracks/span；
- Hug、Fill、Fixed、min/max；
- Constraints；
- 流内/绝对定位；
- 任意视口求解。

验收：一份设计在 10 个基准宽度中连续重排，无节点溢出或覆盖；文字、图片和列表内容变化后父子尺寸正确更新。

### 阶段 3：AI 设计协议与视觉闭环

工作：

- Brief、Design Spec、Scene Transaction；
- 语义查询和节点检查；
- 分区生成；
- Snapshot、视觉校验和自动修正；
- human lock、Diff 和事务回滚。

验收：AI 能从空文档完成三个不同类型的网站；能只修改一个批注目标；能在不破坏人工改动的前提下完成响应式修复。

### 阶段 4：新工作区 Shell

工作：

- Navigation Bar；
- 动态左侧栏；
- 可平移的自由工作区（页面画板之外留出上下左右操作空间，并为多画板预留）；
- 底部工具栏；
- 动态右属性栏；
- 面板折叠、宽度调整和快捷键系统；
- 拆分当前单体 App 的状态与渲染边界。

验收：画布操作不会重渲染全部组件库 iframe；所有面板可独立滚动；编辑区域可最大化；主要功能不依赖中心弹窗。

### 阶段 5：直接编辑与选择系统

工作：

- 层级选择、深度选择、框选和多选；
- Frame/Group/Section 创建与转换；
- 拖动、缩放、旋转、吸附、测距；
- Smart selection、Tidy up、间距手柄；
- 画布内 Auto Layout 编辑。

验收：复杂嵌套内容可以在最多三次操作内选中；保存、大刷新和重新打开后像素与层级不变化。

### 阶段 6：组件、资产与 Variables

工作：

- 统一第三方组件 Property Contract；
- 独立组件库来源侧栏；
- Variant、Text、Boolean、Swap、Slot；
- Main/Instance/Sets；
- Assets 和 Variables 管理器。

验收：用户拖入的是明确的单个组件或 Variant；右侧只显示可编辑属性；Slot 可直接进入设计；实例更新和 override 行为确定。

### 阶段 7：批注、Prototype 与交付

工作：

- 画布评论针与批注列表；
- 批注转 AI 任务；
- Prototype 触发、动作和过渡；
- 全屏预览；
- 代码与资源导出继续作为次要能力。

验收：人可以只通过“选中/圈选 → 写备注 → AI 修改 → 审阅 Diff”完成一轮迭代。

### 阶段 8：生产化验收

工作：

- 大文档性能；
- 崩溃恢复；
- 增量持久化；
- 插件安装升级；
- 多平台和 WebView 验证；
- 视觉回归与 12 类网站基准评分。

验收：所有基准网站、交互测试、响应式测试、保存恢复、AI 精确修改和安装包测试全部通过后再上架。

## 10. 立即执行的第一批任务

1. 停止在旧 schema 和旧工作台上继续增加功能；
2. 为现有编辑器建立 12 个真实网站生成基准和截图；
3. 新建 `schema-v2`、`scene-graph`、`layout-engine`、`design-transaction` 四个独立模块；
4. 先用纯数据测试完成 Frame、Group、Section、Auto Layout、Hug/Fill 和连续响应式；
5. 重写 AI 工具协议，使 AI 不再手工计算整页坐标；
6. AI 闭环达到验收标准后，再开始替换可视化工作区。

## 11. 本轮明确不做

- 不继续新增 UI 组件库；
- 不继续给旧属性面板堆字段；
- 不维护 v1/v2 双编辑器；
- 不用更多页面模板掩盖布局能力不足；
- 不以“能导出代码”代替设计质量；
- 不以“组件数量很多”作为产品完成度指标。

## 12. 最终完成标准

完成不是功能列表被勾选，而是同时满足：

- AI 能稳定生成视觉风格明显不同的高质量网站；
- 结构和命名让 AI 可以精确查找与修改；
- 人工调整保存后不发生任何非预期位移；
- 任意常见视口宽度都能正确响应；
- 复杂嵌套仍能快速选择和编辑；
- 第三方组件是可配置实例，不是不可拆的 Demo；
- 批注可以驱动一次完整、可审阅、可回滚的 AI 修改；
- 页面截图、结构校验、交互验证和保存恢复全部通过。
