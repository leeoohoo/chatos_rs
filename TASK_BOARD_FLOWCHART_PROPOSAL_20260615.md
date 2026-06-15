# 任务看板流程图化方案 2026-06-15

## 结论

可以做，而且当前代码已经具备了 DAG 语义和大部分基础数据。

真正要先解决的问题不是“怎么画图”，而是“当前消息创建的任务集合”目前被限制成了同一条消息内的任务列表，导致跨消息的前置任务不会作为同级节点展示出来。

## 当前代码调研结论

### 1. 任务依赖本身已经是 DAG 模型

- `task_runner_service/backend/src/services/task_dependencies.rs`
  - `validate_task_prerequisites(...)` 已经校验自依赖、权限、循环依赖、深度上限。
  - `resolve_prerequisite_order(...)` 已经能递归展开前置链路。
- `task_runner_service/backend/src/api/router.rs`
  - 已经暴露 `GET /api/tasks/:id/dependency-graph`。
- `task_runner_service/backend/src/models/task/record.rs`
  - `TaskRecord` 已有 `prerequisite_task_ids`。
  - `TaskDependencyGraph` 已有 `prerequisites`、`transitive_prerequisites`、`blocked_by`、`ready`。

结论：数据模型层面没有障碍，流程图化是顺势升级，不是重做。

### 2. 聊天侧任务抽屉现在只显示“这条消息自己的任务”

- `chat_app/src/components/messageTasks/MessageTaskDrawer.tsx`
  - 抽屉只消费 `useMessageTasks(...)` 返回的 `tasks`。
- `chat_app_server_rs/src/api/message_task_runner.rs`
  - `/api/messages/:id/task-runner/tasks` 最终按消息来源做过滤。
- `task_runner_service/backend/src/services/chatos_message_tasks/queries.rs`
  - `list_tasks_for_chatos_source(...)` 只返回匹配当前 `source_user_message_id` 或 `source_turn_id` 的任务。

结论：这就是“当前任务依赖上一条消息任务时，看不见前置任务卡片”的根因。

### 3. 当前详情弹窗能看到前置摘要，但不是“完整卡片/子图”

- `chat_app/src/components/messageTasks/MessageTaskDetailModal.tsx`
  - 详情里会渲染 `prerequisite_task_ids` 和 `prerequisite_tasks`。
  - 但这里只是摘要列表，不是完整节点卡片，也不会把整条依赖链展开成图。

结论：现在不是完全没有前置信息，而是展示层级太浅。

### 4. Task Runner 管理页也还只是文本/Tag 展示

- `task_runner_service/frontend/src/pages/TasksPage.tsx`
  - 前置任务目前只用 `Tag` 展示。
- `task_runner_service/frontend/package.json`
  - 目前没有专门的流程图库。

结论：无论聊天侧还是任务管理页，流程图 UI 都还没真正落地。

## 当前仓库里已有/可复用的图能力

### 已有能力

1. `mermaid`
   - 已存在于 `chat_app/package.json`。
   - `chat_app/src/components/markdownRenderer/useMermaidPreviewController.ts` 已有懒加载、渲染、导出逻辑。
   - 优点：零新增依赖、最快验证。
   - 缺点：更适合静态图；节点点击、选中、悬浮联动、卡片化样式控制都比较弱。

2. `dependency-graph` 后端能力
   - 已存在于 Task Runner backend。
   - 但当前返回的是“某个任务的前置集合摘要”，不是一个前端可直接渲染的完整 `nodes + edges` 子图。

## 候选流程图组件对比

### 方案 A：直接复用 Mermaid

适合：

- 先做一个只读版流程图
- 想最小改动验证“看图是否比看卡片清楚”
- 不要求复杂交互

优点：

- `chat_app` 已经有依赖，最快
- 文本生成 `flowchart LR` 很直接
- 非常适合先做 PoC

缺点：

- 节点交互能力有限
- 很难把节点真正做成我们现在这种“任务卡片”风格
- 多任务状态、当前节点高亮、悬浮联动、点击打开详情都不够顺手

判断：

- 可以做 PoC
- 不建议作为最终主方案

### 方案 B：引入 React Flow 作为主图层

适合：

- 需要节点点击、选中、缩放、拖拽、hover、高亮路径
- 希望把当前任务卡片视觉升级成“图中的节点卡片”
- 后续还可能扩展到任务管理页

优点：

- 很适合 React 场景下的 DAG / workflow UI
- 自定义节点很自然，能直接复用我们现有任务卡片的视觉语言
- 交互能力明显比 Mermaid 强

缺点：

- 需要新增依赖
- 需要补一个布局策略

判断：

- 这是我建议的正式方案
- 在“可维护性 / 交互能力 / 相对轻量”之间最平衡

### 方案 C：引入 X6

适合：

- 未来要做强编辑器能力
- 需要复杂图编辑、节点工具栏、画布级能力

优点：

- 能力很强
- 天生适合复杂图编辑

缺点：

- 对当前聊天侧只读任务面板来说偏重
- 接入和定制成本都明显更高

判断：

- 现阶段不推荐
- 适合作为“以后要做独立任务编排器”时再考虑

## 推荐路线

### 推荐结论

正式方案推荐：

- 聊天侧 `chat_app` 使用 `React Flow`
- 后端新增“消息任务子图”接口
- 第一版先做只读 DAG 视图，不做拖拽编辑

PoC 备选：

- 如果想先 1 天内快速验证视觉方向，可以先用现有 `mermaid` 做静态版
- 但正式落地还是建议上 `React Flow`

## 为什么我不建议直接在现有列表上硬补前置卡片

如果只是把前置任务追加到当前抽屉列表里，会有几个问题：

1. 列表无法表达依赖方向
2. 同一任务被多个后续任务依赖时会重复出现
3. 多层前置链会退化成“长列表”，信息层级仍然不清楚
4. 用户还是看不出“当前阻塞点”在哪个节点

所以更合理的做法是：

- 把当前消息的任务作为 root nodes
- 把它们的直接前置和传递前置一起拉出来
- 用有向边表达 `prerequisite -> task`

## 建议的后端改造

### 新增统一子图接口

建议新增一个聊天侧可直接消费的接口，例如：

`GET /api/messages/:id/task-runner/graph`

返回结构建议：

```json
{
  "roots": ["task_current_a", "task_current_b"],
  "nodes": [
    {
      "id": "task_prev_1",
      "title": "前置任务 A",
      "status": "succeeded",
      "source_user_message_id": "message_prev",
      "source_turn_id": "turn_prev",
      "is_root": false,
      "is_current_message": false,
      "depth": 0
    },
    {
      "id": "task_current_a",
      "title": "当前任务 A",
      "status": "running",
      "source_user_message_id": "message_current",
      "source_turn_id": "turn_current",
      "is_root": true,
      "is_current_message": true,
      "depth": 1
    }
  ],
  "edges": [
    {
      "id": "task_prev_1->task_current_a",
      "source": "task_prev_1",
      "target": "task_current_a",
      "kind": "prerequisite"
    }
  ]
}
```

### 为什么不要直接复用现有 `dependency-graph` 接口

因为现有 `GET /api/tasks/:id/dependency-graph` 只适合单任务摘要：

- 有前置集合
- 有传递前置集合
- 但没有完整边集
- 也没有“多 root 任务合并视图”
- 更没有“当前消息任务 + 跨消息前置任务”的统一聚合结果

所以建议在 Task Runner service 或 `chat_app_server_rs` 这一层补一个“消息级子图聚合接口”。

## 建议的前端实现

### 聊天侧面板结构

建议把 `MessageTaskDrawer` 改成两段式：

1. 顶部视图切换
   - `流程图`
   - `列表`

2. 主体区域
   - 默认显示流程图
   - 列表作为回退视图保留

### 流程图视图交互

第一版建议支持：

1. 当前消息创建的任务默认高亮
2. 前置任务全部展示，不再按消息来源隐藏
3. 点击节点打开现有 `MessageTaskDetailModal`
4. hover 节点时高亮它的直接前置链路
5. 节点上直接显示
   - 标题
   - 状态
   - 是否当前消息
   - 是否阻塞

### 节点视觉建议

不是简单小圆点，建议直接做成“轻量卡片节点”：

- 第一行：标题 + 状态 badge
- 第二行：简短描述或来源消息标识
- 第三行：来源标签
  - `当前消息`
  - `上一条消息`
  - `更早依赖`

这样能保留现有卡片的可读性，又比纯列表更清楚。

## 布局策略建议

第一版不要上太重的自动布局。

建议顺序：

1. 先按 DAG 层级自己做一个简单的 left-to-right 分层布局
   - 无前置的节点在左
   - 当前消息 root 任务在右
   - 同层按创建时间或标题排序

2. 如果后面发现节点数量大了、交叉边太多，再补布局库
   - 到那时再评估 `dagre` 或 `elkjs`

这样可以把新增依赖控制到最低。

## 推荐实施顺序

### Phase 1：接口补齐

1. 新增消息级任务子图接口
2. 放开“跨消息前置任务”的聚合，但仍限制在当前会话内
3. 返回 `nodes + edges + roots`

### Phase 2：聊天侧流程图面板

1. 在 `chat_app` 引入 `React Flow`
2. 新增 `MessageTaskGraphPanel`
3. `MessageTaskDrawer` 增加 `流程图 / 列表` 切换
4. 节点点击复用已有详情弹窗

### Phase 3：任务管理页复用

1. `task_runner_service/frontend` 增加任务详情中的依赖图区域
2. 替换当前 `Tag` 式前置任务展示

## 风险与注意点

1. 一条消息可能创建多个 root 任务
   - 图必须支持多 root 合并展示

2. 前置任务可能来自上一条甚至更早的消息
   - 不能再按当前 `source_user_message_id` 把节点裁掉

3. 图过大时体验会下降
   - 第一版建议限制在“root + 全部前置”
   - 先不把 follow-up / child tasks 一起并入

4. 移动端抽屉空间有限
   - 小屏下建议默认 `fitView`
   - 保留列表视图作为兜底

## 最终建议

如果只看“最快落地”，直接用现有 `mermaid` 就能开始做。

如果看“最终效果和后续可维护性”，建议：

- 正式方案选 `React Flow`
- 第一版只做只读 DAG
- 后端补一个消息级子图接口
- 聊天侧先落地，任务管理页后复用

这条路线既能解决“看不见跨消息前置任务卡片”的核心问题，也不会一步把系统做成过重的图编辑器。

## 参考

- React Flow 官方文档：`https://reactflow.dev/`
- React Flow Quick Start：`https://reactflow.dev/learn`
- Mermaid 官方文档：`https://mermaid.js.org/`
- Mermaid Flowchart 语法：`https://mermaid.js.org/syntax/flowchart.html`
- X6 官方站点：`https://x6.antv.antgroup.com/`
