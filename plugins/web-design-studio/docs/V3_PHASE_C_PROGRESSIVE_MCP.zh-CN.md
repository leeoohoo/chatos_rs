# Web Design Studio 3.0.1：阶段 C 进度记录

## 已完成的高阶入口

- `web_design_get_active_context`
- `web_design_plan_site`
- `web_design_plan_page`
- `web_design_get_plan`
- `web_design_start_page`
- `web_design_run_next_step`
- `web_design_retry_step`
- `web_design_repair_step`
- `web_design_inspect_step`
- `web_design_accept_step`
- `web_design_reject_step`
- `web_design_skip_step`
- `web_design_rollback_step`
- `web_design_complete_page`
- `web_design_pause_plan`
- `web_design_resume_plan`
- `web_design_capture_page`
- `web_design_capture_region`
- `web_design_get_visual_grounding`
- `web_design_compare_snapshots`
- `web_design_inspect_at_point`

上述工具全部绑定 `web-design-progressive-generation` Skill Gate，输入 schema 不包含 `projectId`。运行时使用 ChatOS 注入的 `CHATOS_PROJECT_ID`，并在每次 Plan/Scene 读写前验证 `documentId` 属于当前宿主 Scope。

## 已形成的服务端约束

- Site Plan 只保存目标、受众和页面清单，不创建 Scene 内容。
- Page Plan 一次只规划一个页面，并强制保存视觉方向和验收标准。
- 未规划页面不能启动；同一时刻只能启动一个页面。
- 页面启动只创建 Canonical Scene 和该页 Root Frame，不生成 Section。
- `run_next_step` 自动选择唯一的下一个 ready Step，不接受模型指定另一个 Step。
- `retry_step` 只能重试失败、拒绝、过期或回滚的指定 Step，不能重放已确认步骤。
- 输入图片必须来自当前 Scene revision，并同时包含纯净截图/区域裁剪与 visual grounding。
- 通过验证的 Candidate 必须同时包含布局、截图/裁剪、grounding、视觉 Diff、calibration 和质量报告。
- `auto-current-page` 最多自动提交当前一个 Step；不会循环，也不会进入下一页。
- `guided` 和 `review-sensitive` 保持 Candidate 在正式 Scene 之外，等待显式接受或拒绝。
- 回滚只允许撤销 Scene 中最新且可精确识别的 Step 事务；存在后续人工或 AI 事务时拒绝回滚。
- 页面完成后停止在页面边界，不自动启动下一页。
- 整页与局部截图由系统 Chromium 真实渲染为 PNG，并通过 MCP `image` content block 直接返回给模型；base64 不进入 `structuredContent`。
- 每次截图同时持久化 Snapshot/Crop、Visual Grounding、Layout 和 Calibration Artifact，坐标来自浏览器 `getBoundingClientRect()`。
- `capture_region` 在同一份 Scene revision 上完成区域求解和截图，避免读取两次 Scene 导致 revision 竞态。
- 快照比较持久化 Visual Diff PNG，返回 before/after/diff 三张图、变化区域和命中的稳定 nodeId。
- 点选定位返回从最小命中节点开始的候选及有限祖先链，使 AI 和人不必从重叠元素中猜节点 ID。
- 所有视觉 Artifact 按宿主注入的 projectId 与 documentId 隔离，跨 Scope 不能读取。

## 验证

- `v2-progressive-generation-service.test.mjs` 覆盖 Site/Page 分离规划、单步执行、过期视觉输入和 guided 审阅。
- `mcp-progressive-generation.test.mjs` 通过真实 MCP stdio 验证工具注册、Skill Gate、宿主 projectId、规划、启动、图片 content block 和一次单步提交。
- `v2-generation-visual-artifact-store.test.mjs` 覆盖 PNG/grounding 持久化、校验和跨项目/文档隔离。
- `v2-generation-visual-service.test.mjs` 覆盖整页、裁剪、grounding、点选与像素 Diff。
- `v2-headless-scene-renderer.test.mjs` 使用真实 Chromium 验证 PNG 尺寸和稳定 nodeId 测量。
- 阶段收口时 TypeScript strict typecheck、生产构建和插件全量测试 `222/222` 通过。

## 阶段结论

阶段 C 的高阶规划、单步执行、审阅恢复和视觉读取/定位入口已经接通。AI 可以在不读取完整文档的情况下，用真实整页图、局部图、稳定节点 Grounding 和视觉 Diff 完成一个有边界的页面 Step。下一阶段进入 Camera 工作区和多画板投影。
