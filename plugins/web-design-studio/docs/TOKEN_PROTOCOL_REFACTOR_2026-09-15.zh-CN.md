# Web Design Studio Token 与执行协议重构

日期：2026-09-15

## 目标

降低重复上下文和机械调用造成的 token 浪费，同时保持 Candidate 截图、Diff、响应式检查、语义画板数量、视觉验收和独立可编辑节点的质量要求不变。

## 已确认问题与修复

| 问题 | 修复 | 验收标准 |
| --- | --- | --- |
| 工具结果 pretty JSON 膨胀 | MCP 文本结果改为紧凑 JSON，`structuredContent` 保持完整 | 相同结构不再含格式化空白 |
| `plan_page` 重复返回完整 Plan | 默认只返回状态计数、当前画板/Step、门禁和下一步；完整设计说明仅按需读取 | 规划第 N 个画板时不重复前 N-1 个设计说明 |
| Candidate 同时返回 `artifacts` 与 `nextVisualInputs` | 改为单一 `reviewArtifacts` 索引；下一步基线由程序自动捕获 | 同一 artifact 不在一次回包重复出现 |
| Accept 返回大量节点 ID 和 transaction | 返回 revision、分类数量、最多 24 个受影响节点与根节点 | 完整 transaction 仍持久化但不进入默认模型回包 |
| Catalog 一次返回库、区块、模板、主题 | 默认返回库摘要与种类计数；按 kind 获取或统一搜索 | 默认回包保持小型，组件仍可搜索并读取精确 contract |
| 主 Skill 接近 16 KB 并被宿主截断 | 主 Skill 保留入口、门禁、路由、主循环和质量红线；规划、Scene 构建、Candidate 审核拆为叶子 Skill | 主 Skill 本体明显低于 16 KB；所有 Skill 单独校验通过 |
| 30 个工具、重复生成 Schema | 暴露面压到 18 个；生成/重试/修复合并为 `web_design_execute_step`；生命周期操作合并为 `web_design_control_plan` | ListTools 只有 18 个工具，不再重复三份生成 schema |
| Scene 操作参数重复 | 增加 `insert-simple-tree`，一次提交递归层级，服务端生成完整独立节点树 | 65+ 节点可通过一个树操作表达，节点仍可独立选择和编辑 |
| `execute_step` / `edit_scene` 嵌套 Schema 每轮重复注入 | 工具只传 `operationsJson` / `commandJson`；具体格式放进按需 Scene Skill，服务端解码后继续执行完整严格校验 | 工具定义明显缩小，操作能力、节点数量、可编辑性和校验不变 |
| 机械步骤由 AI 手动串联 | `plan_page` 自动启动画板并创建 root；执行前自动抓取 required viewports；程序自动生成 ID、判断执行模式；接受 handoff 后自动完成画板 | AI 只提交设计变化并做视觉判断 |
| 完成画板后历史仍累积 | handoff 接受后返回紧凑 `artboard-complete` checkpoint 与下一步 | 后续上下文只需 checkpoint、当前 Plan 摘要和下一画板 |
| 多画板平铺导致覆盖、卡顿与注意力分散 | 编辑器改为“画板目录 + 单一当前画布”；点击目录只挂载一个画板；AI 按 `artboardId` 一次查询/编辑一个画板 | 改宽不会影响其他画板；非当前画板无 DOM/runtime；跨画板编辑被拒绝 |

## 保留的质量门禁

- 每个 Candidate 仍由 Chromium 实际渲染。
- 每个 required viewport 仍生成 Candidate 与 Diff。
- AI 仍必须查看图片并显式接受或拒绝。
- Design Gate 与 handoff 仍是必需步骤。
- 所有树中子节点仍是有稳定 ID 的独立 Scene 节点。
- 语义画板可表示页面、菜单、弹窗、Drawer、Popover、overlay 或重要状态，不按设备类型硬拆。

## 新的最短主流程

1. `web_design_get_active_context`
2. `web_design_plan_site`
3. `web_design_plan_page`（自动启动与创建 root）
4. `web_design_get_catalog` / `web_design_search_catalog` / `web_design_get_component_contract`
5. `web_design_execute_step`（自动基线 capture、模式与 ID）
6. 查看 Candidate / Diff
7. `web_design_control_plan` 接受或拒绝
8. handoff 接受后程序自动完成画板并返回 checkpoint

## 不做的优化

- 不减少截图数量或清晰度。
- 不跳过 required viewport。
- 不减少应设计的语义画板。
- 不把节点合并为不可编辑图片或巨型文本。
- 不用模板替代视觉判断。

## 发布前测量（3.0.16）

- 暴露工具数：18。
- 工具定义 JSON：19,185 bytes（重构中间状态为 38,168 bytes）。
- 输入 Schema JSON：10,085 bytes（重构中间状态为 29,104 bytes）。
- 主入口 Skill：低于 8 KB，不再接近宿主 16 KB 截断线。
- 大型操作格式只在 `web-design-scene-building` 按需加载；服务端仍以原 Scene parser、transaction validator、layout validator 和视觉门禁校验。
