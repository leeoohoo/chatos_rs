# 任务流程图专项架构

任务流程图是 ChatOS 的核心工作面，Swift 版必须保留并提升，不降级成静态图片或简单列表。

## 现有图的关键语义

当前实现中值得保留的行为包括：

- DAG 自上而下布局，前置任务指向后续任务。
- “精简图”和“完整图”两种模式。
- 精简图会把同一 Project Task 的执行/复核阶段合并，并做传递约简，减少重复边。
- 完整图显示全部阶段，并加入 `dependency_context_refs` 的上下文关联边。
- 上下文关联使用虚线，仅传递上下文，不阻塞执行。
- 当前消息、直接前置和间接前置有不同视觉层级。
- 选中节点后展示完整上游/下游，弱化无关节点；选中节点和直接相邻边有更强反馈。
- 运行中的节点与依赖边有动态状态。
- 节点可打开执行过程、任务详情、阻塞处理和 Run 详情。
- 支持缩放、滚轮缩放、清除聚焦与画布滚动。
- Task stage 合并后仍可在节点内切换具体阶段。

## Swift 数据模型

```swift
struct TaskGraphSnapshot: Sendable, Equatable {
    var revision: Int64
    var nodes: [TaskNode]
    var edges: [TaskEdge]
    var sourceUserMessageID: MessageID?
}

struct TaskNode: Identifiable, Sendable, Equatable {
    let id: TaskID
    var projectTaskID: ProjectTaskID?
    var title: String
    var summary: String
    var status: TaskStatus
    var stage: TaskStage
    var prerequisiteIDs: [TaskID]
    var dependencyContextRefs: [String]
    var lastRunID: RunID?
}

enum TaskEdgeKind: Sendable {
    case prerequisite
    case context
}
```

图数据必须有 snapshot revision；旧请求不能覆盖新 execution group 或新 graph revision。

## 显示模式

### 精简图

- 按 `projectTaskID` 合并 planning/execution/review 等阶段。
- 状态聚合优先级：failed → running → blocked → ready → cancelled → completed。
- 合并后重建前置依赖。
- DAG 无环时执行传递约简；存在异常环时保留原边并显示诊断。
- 默认用于日常查看和小窗口。

### 完整图

- 展示所有 Task/Stage。
- 展示 prerequisite 实线边和 context 虚线边。
- 节点 Inspector 可切换 stage、Run 与原始 ID。
- 默认用于排障、复盘和复杂计划。

## 布局引擎

不依赖浏览器图形库。实现一个纯 Swift `TaskGraphLayoutEngine`：

1. 校验节点与边，去重、自环过滤、循环检测。
2. 计算 rank/depth，默认 top-to-bottom。
3. 使用 barycenter/median pass 减少交叉。
4. 同 rank 节点按稳定 key 排列，状态刷新时尽量保持位置。
5. prerequisite 边从节点底部端口进入目标顶部端口。
6. 同一 band 的多条边分配 lane，避免重叠。
7. context 边从节点侧边出发，在图外层 lane 绕行，使用虚线。
8. 缓存 layout by `(graphRevision, displayMode, viewportClass)`。

第一版允许借鉴现有 Dagre 参数：节点间距约 64、rank 间距约 132、边间距约 28，但不照搬 320×300 的大节点尺寸。Swift 版默认节点约 240×128，详细信息进入 Inspector。

## SwiftUI 渲染

```text
ZStack
├── Canvas                 # 边、箭头、网格、运行动画
├── TaskNodeView[]         # 可交互、可访问的 SwiftUI 节点
├── GraphToolbar           # 模式、缩放、适应窗口、清除聚焦
└── TaskInspector          # 详情、依赖、Run、阻塞处理
```

- `Canvas` 只画视觉边，不承担点击语义。
- TaskNodeView 使用稳定 TaskID identity。
- 画布通过统一 camera transform 实现 pan/zoom。
- 缩放建议 0.55–1.8；提供 `适应窗口`、100%、`定位运行中`。
- Reduce Motion 开启时，运行边只改变颜色/线宽，不播放流动动画。

## 聚焦规则

点击节点后：

- 选中节点：主强调。
- 直接 parent/child：第二强调，并突出对应边。
- 全部上游/下游：保持正常可读。
- 无关节点：降低至约 35% opacity，不移除。
- Inspector 显示上游数量、下游数量、直接前置、阻塞原因和最近 Run。
- 再次点击或按 Escape 清除聚焦。

## 节点内容

节点默认只显示：

- 关系：当前任务 / 直接前置 / 间接前置。
- 标题。
- 状态。
- 前置依赖数量。
- 阶段数量或 Run 状态。

描述、输入、输出、文件变化、执行过程和操作按钮进入 Inspector，避免节点变成 300 pt 高的大卡片。

状态显示优先考虑集成状态：`integration_pending`、`integrating`、`integration_conflict`、`integration_failed`；`ready` 且前置未完成时展示 `waiting_prerequisite`。

## Inspector 数据边界

- **任务详情**：任务目标、描述、状态、依赖、结果摘要与阻塞处理。
- **执行过程**：只消费任务的 `process_log`，按 `[时间] 节点标题` 解析成关键执行时间线；它回答“任务执行到了哪一步”。
- **运行详情**：消费 Task Runner Run 元数据与诊断事件；它回答模型阶段、工具事件、错误和 Run 级审计。

执行过程不能直接复用 Run 事件列表，否则会和运行详情重复。聊天中的 Task Runner callback agent 回报提供“查看过程 / 查看详情”，用户消息继续提供整轮“查看过程 / 任务图”。

## 加载与分页

- 任务图接口只返回绘图需要的轻量节点，不在图加载阶段逐节点补齐模型配置、Run、过程日志和前置任务详情。
- 选择节点后再请求任务详情；只有切换到“运行详情”时才请求 Run。
- Run 事件首屏加载 40 条，后续每页 50 条，通过 `event_limit` 与 `event_offset` 增量加载。
- SwiftUI 的过程和事件时间线使用惰性容器，禁止一次性创建全部诊断事件视图。

## 交互与快捷键

- 点击：聚焦节点。
- 双击/Return：打开 Inspector 详情。
- Space + drag 或触控板：平移。
- `⌘+` / `⌘-`：缩放。
- `⌘0`：适应窗口。
- `R`：定位运行中节点。
- `F`：切换精简/完整。
- `Esc`：清除聚焦。
- `⌥↑/↓`：在依赖方向上移动选择。

## 无障碍替代视图

Canvas 本身不适合作为唯一访问方式，必须提供“图 / 大纲”切换：

- 大纲按 depth 分组。
- 每个 Task 读出状态、直接前置数量、直接后续数量和当前选择。
- VoiceOver 用户可以从节点打开相同 Inspector 与操作。
- 打印/导出可以生成 SVG/PDF，但导出不是运行时主界面。

## 实时更新

- Realtime patch 按 graph revision 和 task revision 合并。
- 状态变化不重新布局；只有节点/边集合变化才增量布局。
- 新节点尽量出现在其 parent 附近，旧节点保持位置，避免整个画布跳动。
- 运行结束后动画停止，但选中和 camera 不重置。
- graph reload 与旧请求竞态必须使用 request generation 防护。

## 必测场景

- 1、10、50、200 个节点的 DAG。
- 多根、多叶、菱形依赖、长链、宽层级。
- 重复边、自环、异常环和缺失节点引用。
- 精简模式 stage 合并和传递约简正确。
- 完整模式 context 虚线不计入阻塞依赖。
- 选中节点后上游、下游、直接相邻与无关节点分类正确。
- 运行状态高频更新不改变节点位置。
- 旧 execution group 的晚到 response 不覆盖新图。
- 200 节点下 pan/zoom 保持 60 fps，状态更新不触发全图重算。
- VoiceOver 大纲可以完成查看详情、处理阻塞和打开 Run。

## 发布门槛

- 与服务端 task dependency snapshot 节点和边完全一致。
- 精简/完整模式切换无丢节点、无错误阻塞关系。
- 运行中、失败、阻塞、等待前置、集成冲突均有明确视觉状态。
- 用户能从任一节点在两步内进入执行过程或 Run 详情。
- 图形交互和大纲替代视图都有 UI 自动化测试。
