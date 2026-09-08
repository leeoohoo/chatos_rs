# Web Design Studio v2 阶段 1：Scene Graph 与事务

## 实施原则

- v2 使用独立的 `schemaVersion: 2`，不向旧 `components[]` 添加兼容字段；
- 不创建 v1 → v2 自动迁移器，不让旧编辑器读写 v2 文档；
- 数据模型、事务、存储均不依赖 React，可以在没有可视化工作区的情况下完整测试；
- projectId 仍由项目与宿主 scope 管理，不混进可移动的设计文档内容；
- 所有修改先成为原子事务，布局引擎和 AI 工具不能绕过事务直接写文件。

## 已完成的数据模型

文档使用递归 Scene Graph：

```text
Document
  └─ Page
      ├─ Section
      │   └─ Frame
      │       ├─ Group
      │       │   └─ Text / Shape / Media
      │       ├─ Library Instance + Slots
      │       └─ Component Instance
      ├─ Main Component
      └─ Component Set
```

已定义：

- Frame、Group、Section 的独立语义；
- 有序递归 children；
- 节点 frame、transform、layout 和 appearance；
- Auto Layout、Grid、Hug、Fill、Fixed、min/max、flow/absolute；
- 多层 Fill、Stroke、Effect 和 Typography；
- Variables、Collection、Mode 和 Alias 基础结构；
- Main Component、Component Set、Instance、Library Instance 和 Slot；
- annotations、AI edit policy、人工锁定字段和创建来源。

## 已完成的事务能力

- 原子插入、更新、删除、移动和页面重命名；
- 一个批次中任一操作失败，整个批次不产生结果；
- 节点不能移动进自己的子树；
- 更新不能修改 ID、类型、children、slots 和创建元数据；
- 拒绝 `__proto__`、`prototype` 和 `constructor` 路径；
- AI 不能修改保护策略；
- AI 不能覆盖人工锁定字段；
- AI 不能移动或删除受保护子树；
- 每次成功事务只增加一次 revision，并返回新增、更新、删除和移动摘要。

## 已完成的存储能力

- 通用 `AtomicJsonDirectory` 负责目录锁、临时文件、文件 fsync 和原子 rename；
- v2 文档和 undo/redo 历史保存在同一个原子记录内；
- 相同 revision 的并发写入只有一个可以成功；
- undo/redo 恢复内容，但 revision 始终单调递增；
- undo 后产生新事务会清空旧 redo 分支；
- 持久化历史中的 before/after 文档和 transactionId 会在读取时重新校验。

## 已完成的查询能力

- 可组合查询 ID、页面、节点类型、语义 role、名称和创建/更新来源；
- 可限定任意祖先或直接父节点，并保留 Library Slot 上下文；
- 可查询 visible、locked、AI 实际可编辑状态和人工锁定字段；
- 可按批注状态、作者和正文定位需要人工审阅的节点；
- 可查询 Library Instance、Component Instance 和 Variable Binding；
- 同一查询索引可重复使用，结果保持文档顺序、支持 limit，并返回不会反向修改源文档的节点副本。

## 已完成的 Diff 能力

- 对 Document、Page、Node、Variable Collection 和 Variable 返回具体字段路径及 before/after；
- 将新增、删除、移动与字段变化分开表达；
- 新增或删除整个子树时只报告子树根，避免重复输出每个后代；
- 使用最小顺序差异识别重新排序，不把被挤动的所有兄弟误报为移动；
- 跨容器和跨 Library Slot 移动保留精确位置；
- Diff 返回值与源文档隔离，供 AI 审阅时不会意外修改设计。

## 已完成的 Variables 能力

- 每个 Variable 在 Collection 的每个 Mode 中必须且只能定义直接值或 Alias；
- Color、Number、String、Boolean、Duration、Easing 的直接值进行类型校验；
- Alias 可跨 Collection，解析时使用每个 Collection 当前选择的 Mode；
- 未知 Alias、类型不一致和潜在 Alias 循环在持久化前直接拒绝；
- 节点变量绑定校验目标变量、属性路径和语义类型；
- 提供单变量和节点全部绑定的解析结果，并保留完整 Alias chain 供 AI 解释。

## 已完成的 Component Instance 能力

- Instance 必须引用当前文档中真实存在的 Main Component；
- Override 使用明确的 JSON Pointer，区分公开属性和 Main 内部节点字段；
- Override 不能越出 Main 子树，也不能修改 ID、类型、children、slots 和创建审计字段；
- Boolean、Text、Variant、Instance Swap 的默认值与覆盖值进行类型和引用校验；
- Component Slot 成为正式 Scene Graph 子树，可查询、移动、插入和参与 Diff；
- Slot 名称、允许节点类型、min/max 数量在每次事务提交时原子校验。

## 已完成的性能与历史边界

- Scene Diff 的顺序变化检测使用唯一 ID 的最长递增子序列，避免大型同级列表的平方级矩阵；
- 以 3000 个真实 Scene Node 覆盖完整校验、索引查询、事务和字段 Diff；
- undo/redo 快照使用带 SHA-256 校验的 gzip 数据，不在 JSON 中重复保存展开后的完整文档；
- 历史同时受条目数和压缩后字节预算约束，并优先保留离当前状态最近的操作；
- 即使单次设计变更大于预算，仍保留最近一条可撤销记录，不阻断用户保存。

## 阶段 1 验收结果

- 独立 Schema v2、Scene Query、字段 Diff、Variables、Components、原子事务和压缩历史均已完成；
- 阶段 1 的功能与大型文档边界由自动化测试覆盖；
- 下一阶段开始实现 Auto Layout、Grid、Hug/Fill 和连续响应式求解器。
