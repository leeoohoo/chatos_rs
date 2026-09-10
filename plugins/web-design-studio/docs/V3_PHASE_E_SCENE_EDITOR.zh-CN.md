# Web Design Studio 3.0.1：阶段 E Scene 编辑器进度

## 已完成：统一层级命中与选择导航基础

- 新增与组件库无关的 `selection-model.ts`，输入只有稳定节点 ID、父级、层级、可见性、锁定状态、矩形和堆叠顺序；当前编辑器和后续 Scene v2 节点使用同一套命中语义。
- Command / Ctrl 点击画布对象时，不再直接依赖浏览器最后命中的 DOM，而是按当前指针位置计算全部重叠候选。
- 重叠候选按视觉堆叠、节点深度、可见面积和文档顺序稳定排序，并显示名称、类型、层级与锁定状态。
- Shift 单独负责多选，避免“深度选择”和“追加选择”共用同一个含糊操作。
- Enter 进入当前容器的最高可见子层；可编辑 Slot 容器会进入统一的中心内容编辑；Shift + Enter 返回父层或外层组件。
- 单击统一为选择，双击或 Enter 才进入组件内部；动画、iframe 和复合组件不会再因为一次普通选择直接切换编辑层级。
- 外层画板和 Drawer、Modal、Card、Form 等 Slot 聚焦编辑使用相同候选计算与层级导航。
- 修饰键选择会抑制随后产生的普通 click，避免 Command / Ctrl 点击被错误解释成“编辑内部”。
- 删除选中对象上方长期出现的“拖动整体 · 点击编辑内部”覆盖提示。
- 选中边框、名称和缩放手柄已经从组件 DOM 移到独立 `SelectionOverlay`；第三方组件、iframe 和动画不再拥有或遮挡编辑器控制点，多选轮廓也由同一层统一渲染。

## 已完成：框选、多选合并边界与 Scene v2 组合事务

- 在画板或 Slot 编辑画布的空白区域拖动即可框选；不足 4px 的拖动仍按普通空白点击处理，避免手抖误选。
- Shift + 框选会追加到现有选择，普通框选会替换选择；隐藏节点不会进入结果，包住框选区域的背景层不会被误选。
- 框选优先选择完整落入选框的节点；没有完整落入对象时再选择相交对象，并自动去除同时命中的父子重复项。
- 多选除了保留每个图层的细轮廓，还显示统一合并边界和图层数量；在整体缩放语义接入前不显示虚假的多选缩放手柄。
- 新增 `scene-editor-transaction.ts`，通过现有 Scene v2 原子事务组合出真正的 Group、Frame 和 Ungroup 操作，不再为新路径扩展旧 `section` 模拟分组。
- Group 使用紧贴内容的边界；Frame 可拥有真实 padding。包组时子节点坐标转换为容器局部坐标，解组时还原到父级坐标，前后视觉几何保持一致。
- 只允许同一父级、同一 Slot、自由布局中的兄弟节点成组；跨父级或 Auto/Grid Layout 会明确拒绝，避免布局结果悄然变化。
- Group、Frame、Ungroup 都返回标准 `SceneTransaction`，因此沿用 revision 冲突、AI 锁定字段、原子失败和 Scene Store 历史机制，不增加第二套写入协议。

## 验证

- 纯模型测试覆盖重叠顺序、隐藏节点过滤、父子深度、锁定状态、进入子层、返回父层、双向框选、背景排除、相交回退和合并边界。
- Scene v2 编辑事务测试覆盖 Group、带 padding 的 Frame、Ungroup 往返、视觉坐标保持、层顺序以及跨父级和 Auto Layout 拒绝。
- TypeScript strict typecheck、生产构建通过。
- 隔离真实浏览器确认拖框过程中会实时选择 3 个组件并显示合并边界，松开后框选矩形消失、选择保留，多选状态没有错误缩放手柄；测试未保存到用户设计。

## 已完成：统一 Scene 编辑命令与服务入口

- 新增严格的 `SceneEditorCommand`，以一个判别联合统一 Move、八方向 Resize、Group、Frame、Auto Layout Frame 和 Ungroup；未知字段、重复节点、非法尺寸与非法枚举会在生成事务前拒绝。
- Studio HTTP 与 MCP 共用同一个命令执行器和 Scene Store 原子提交，不复制动作实现；Studio 固定以 `human` 提交，MCP 固定以 `ai` 提交，调用方不能伪造作者绕过保护规则。
- 命令使用 `expectedRevision` 做乐观并发控制，并以稳定 `transactionId` 支持安全重试；已经提交过的同一请求会返回原事务摘要，不会重复移动、重复成组或重复缩放。
- Studio 新增 Scene 读取、命令、历史、Undo 和 Redo 路由；Repository 已提供对应 Scene v2 方法。删除旧设计时也会清理同 ID 的 Scene Store 记录。
- MCP 新增 `web_design_query_scene` 和 `web_design_edit_scene`：AI 可以先按稳定 ID、页面、类型、角色或名称读取有限节点，再执行一次聚焦编辑；编辑结果返回受影响节点、页面以及截图复核建议。
- AI 编辑仍经过 Scene Transaction 的 Hard Lock、`aiPolicy.editable` 和字段锁校验；没有新增旧 `components[]` 双写分支。
- 新增命令解析、Store 幂等、AI 保护、Studio 路由、Undo/Redo 和真实 MCP 子进程回归；本批专项测试 14/14 通过。

## 已完成：Scene v2 主画布与人工微调闭环

- 新增独立 `SceneArtboardCanvas`，直接渲染 Scene v2，按稳定节点 ID 完成单选、Shift 多选、框选、整体移动和八方向 Resize；拖动预览与服务端提交复用同一套 Scene 事务函数，没有在 UI 复制几何算法。
- 工作区画板在 Scene 存在时只渲染 Scene，不再同时叠加旧 `components[]`；Scene 尚未创建时显示明确的 AI 起步提示，不做 v1/v2 双写或静默迁移。
- 画板外框高度由 Scene 求解结果驱动，首屏线仍由画板 viewport 驱动；AI 分多步增加内容后，页面画板会随 Scene 内容高度增长。
- 顶部历史按钮和快捷键已经切换到 Scene Store Undo/Redo；移动、缩放、Group、Frame、Auto Layout、属性修改与删除都进入同一 revision 历史。
- 新增受控 `update-node` 命令，只允许名称、文本内容、可见性、锁定、Frame 几何以及核心布局字段；路径、数值、枚举、重复字段和请求体积都在生成事务前校验，AI 仍受 Hard Lock 与字段保护约束。
- 新增受控 `delete-nodes` 命令，多选中同时包含父子节点时只删除选择根，避免重复删除；删除结果可由 Scene 历史完整撤销。
- 右侧属性栏现在读取 Scene 节点本身，提供名称、文字、X/Y/W/H、布局方式、尺寸模式、padding、gap、可见性、锁定状态和 AI 编辑策略；数值与文字在提交时生成命令，不直接篡改本地对象。
- 左侧图层面板在 Scene 模式下显示 Scene 层级，并可选择、多选、显示/隐藏和锁定；不再把旧组件层级伪装成当前 Scene。
- Group、Frame、横向/纵向 Auto Layout、Ungroup 与删除已经出现在 Scene 选区工具栏；键盘方向键微调也提交标准 Move 命令。

## 本批验证

- TypeScript strict typecheck 和生产构建通过。
- Scene 命令测试新增属性白名单、重复路径拒绝、文本类型保护、父子删除去重与删除后撤销覆盖。
- 隔离真实 Headless Chrome 打开 Studio Scene 画板，确认 Scene 内容可见；点击文字节点后右侧正确显示 `Headline`，独立选区层出现 1 个主选框和 8 个 Resize 手柄。
- 浏览器点击“锁定”后，服务端 Scene revision 从 r1 更新到 r2，按钮切换为“解锁”，History 返回 1 条可撤销事务；QA 设计随后已删除，没有保留到用户项目。

## 阶段 E 后续

1. 为 Scene 图层补齐复制、重排、对齐和分布命令，并将这些操作继续限制在受控命令而不是开放原始 Transaction。
2. 将页面/画板创建、重命名和原型连线完全迁移到 Scene 与 Workspace 模型，随后删除编辑器主路径中的旧页面编辑代码。

## 已完成：Scene 批注与 AI 视觉任务

- Scene 编辑命令新增添加、完成和重新打开批注；批注作者与时间由 Studio 服务端生成，不能由浏览器伪造，所有状态变化进入 Scene revision 与 Undo/Redo 历史。
- AI 的通用 `web_design_edit_scene` 不暴露这些人工批注命令；AI 不能把自己的备注伪装为人工请求，也不能绕过视觉验证自行关闭批注。
- 右侧 Scene 属性栏新增“设计 / 批注与 AI”切换。用户可以对稳定节点添加备注、把既有备注准备给 AI、人工确认完成或重新打开。
- “提交视觉任务”不再写旧 `document.requests`：它创建 Scene Annotation，再调用统一视觉服务截取当前节点真实 PNG，并绑定 projectId、documentId、pageId、nodeId、Scene revision、grounding 与截图 artifact。
- 新增共享 `AnnotationAiService`，Studio HTTP 与 MCP 共用同一任务准备逻辑；截图和 Scene revision 不一致时直接拒绝陈旧上下文。
- MCP 新增 `web_design_prepare_annotation_task`；`web_design_list_requests` 会优先返回 Scene Annotation，并给出准备视觉任务所需的稳定参数。
- 插件 router skill 与渐进生成 skill 已明确：一条 Scene 批注就是一个有界设计步骤，AI 必须先看截图、局部修改、再截图比较，不能只凭节点文字判断完成。

## 已完成：Scene 独立画板的创建与命名

- 新建页面、弹窗、抽屉、浮层、菜单或界面状态时，Studio 直接提交 `create-page` Scene 命令，并同步新增 Workspace 位置；不再把新画板写入旧 `document.pages`。
- 每个新画板拥有独立稳定 pageId 和 page-root Frame。根 Frame 默认使用纵向 Auto Layout、内容高度 hug 和首屏最小高度，适合 AI 分多次追加 Section，而不是一次生成完整页面。
- `create-page` 只允许人工 Studio 命令使用；AI 仍必须通过 Plan / `web_design_start_page` 的渐进页面边界创建规划中的页面，不能绕过站点计划任意批量建页。
- Scene 模式的页面下拉、类型、名称和稳定 ID 已读取 Scene + Workspace；重命名走 `rename-page` Scene Transaction，画板类型只写 Workspace 展示语义。
- Scene 模式不再展示会修改旧页面数据的“复制画板 / 删除”按钮。移出工作区只改变布局位置，不删除 Scene 设计内容。
- Scene 模式的画板宽与首屏高直接修改 Workspace viewport；实际内容高度仍由 Scene 求解结果自动增长。
- 批注工具点击 Scene 节点后只做选择并打开“批注与 AI”，不会同时启动拖动事务。
