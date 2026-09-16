# 空 Scene 与错误阻塞问题记录（2026-09-15）

## 现场证据

- 任务：`重新设计 wms 项目的全部页面`
- Task ID：`05f45048-66d7-4697-8716-e05bf784cc1e`
- Run ID：`c250faea-f237-439f-8114-4d850c8f2a5c`
- 使用插件：Web Design Studio `3.0.14`
- 运行结果：`blocked`

数据库与插件存储显示：AI 激活了 Web Design Studio 及 5 个叶子 Skill，也能看到全部 30 个 Web Design MCP 工具；但它先花约 14 分钟修改 React/Ant Design 源码、运行安装、构建和 Playwright。之后只调用了 `get_active_context`、`list_documents`、`create_document`、`get_plan` 和 `plan_site`，没有调用 `plan_page`、`start_page`、`run_next_step` 或 `accept_step`。

最终设计文档 `website-76b88896` 停留在 revision 1，只有一个空白首页；Generation Plan 的 9 个语义画板全部为 `unplanned`，因此工作台没有 Scene 可以显示。

任务最后执行了用户未要求的 `npm audit fix` 和 `npm audit --audit-level=high`。Taro 4.2.1 的既有依赖链仍包含 high/critical advisory，任务据此主动上报 `blocked`。这不是 Web Design Studio 生成 Scene 的技术阻塞，而是任务错误扩大到依赖安全治理后触发的平台供应链门禁。

## 根因

1. Skill 把“单次 generation tool call 只做一个 Step”写成了容易被理解为“整个任务运行只推进一次”的表述。
2. Site Plan 返回 `nextAction`，但没有独立、机器可读的交付门禁说明“Scene 为空时禁止改源码和上报终态”。
3. Router 建议一次激活多个叶子 Skill，增加上下文，同时仍可能漏掉 `web-design-components`。
4. Validation Skill 还引用未暴露的旧工具 `web_design_validate` 和旧页面读取路径。
5. Skill 没有明确禁止普通视觉任务执行 `npm audit fix`、依赖升级和 lockfile 重写。

## 修复

- 所有 Plan 摘要和 active context 增加 `deliveryGate`：
  - `visibleSceneReady`
  - `projectImplementationAllowed`
  - `taskCompletionAllowed`
  - `requiredNextAction`
  - 可诊断的 gate code 与说明
- `nextAction` 的生成步骤附带目标节点与验收宽度，减少再次读取计划。
- 明确当前任务运行应连续推进多个“小步工具调用”，不能停在 Site Plan 或空根节点。
- 产品源码实现必须排在可见 Scene、Design Gate、handoff 和画板完成之后，并且只能实现已完成画板。
- Router 改为按需激活叶子 Skill；组件搜索前强制激活 `web-design-components`。
- Validation Skill 改为 Scene v2 的 Candidate / query / repair 流程，删除旧工具引用。
- 普通设计任务不执行 `npm audit`、`npm audit fix`、依赖升级或 lockfile 重写；既有 advisory 只能作为风险说明，不能充当视觉设计阻塞。
- 工作台在仅有 Plan、没有 Scene 时明确显示“AI 只完成了规划”，并展示必须执行的下一工具。

## 验收标准

1. 新建文档但没有 Plan 时，`deliveryGate.code = NO_SITE_PLAN`。
2. 只有 Site Plan 或空 Scene 时，`visibleSceneReady = false` 且 `projectImplementationAllowed = false`。
3. 接受首个可见视觉 Step 后，`visibleSceneReady = true`，但画板完成前仍禁止项目实现。
4. 完成一个画板后，只允许实现该画板；全部计划画板完成前不得宣告整个设计任务完成。
5. Skill 不再引用不可用工具，也不再把依赖审计当作默认设计验证。
