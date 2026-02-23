# System Prompt 管理页优化方案（AI 生成 + 编辑体验升级）

## 1. 现状复盘（基于当前项目代码）

当前 system prompt 管理页是一个模态框，核心能力已经具备：
- 列表查看、创建、编辑、删除、激活
- 关联应用多选
- Markdown 文本输入与 Ctrl+S 保存

关键代码位置：
- 页面组件：`chat_app/src/components/SystemContextEditor.tsx`
- 前端 API：`chat_app/src/lib/api/client.ts`
- Store actions：`chat_app/src/lib/store/actions/systemContexts.ts`
- 后端路由：`chat_app_server_rs/src/api/configs.rs`

目前的主要痛点：
1. **新建过程完全手写**：没有“AI 生成/优化”入口，新用户上手慢。
2. **编辑器能力偏基础**：当前是单一 `textarea`，缺少预览、差异对比、质量提示、结构化辅助。
3. **缺少质量闭环**：保存前没有对“清晰度、约束完整度、冲突风险、长度”做自动评估。
4. **交互反馈偏弱**：保存/失败主要依赖 `alert` 与按钮状态，缺少更细粒度引导。
5. **缺少版本能力**：没有历史版本、回滚、AI 改写前后对比，调优成本高。

---

## 2. 目标（你这次要的重点）

围绕“新增 + 编辑”两大场景，目标是：

- **新增时可借助 AI 快速生成高质量 system prompt**
- **编辑框体验显著升级**（写得更快、改得更稳、看得更清晰）
- **保留人工主控**：AI 只给建议，不自动覆盖；用户最终确认后才保存
- **兼容现有架构**：在当前 routes/store/API 基础上平滑演进

---

## 3. 交互方案（产品层）

### 3.1 新建流程：从“空白输入”升级为“AI 向导 + 候选草案”

新建按钮点击后，进入三段式流程：

1) **需求向导（Step 1）**
- 目标场景：编程助手 / 翻译 / 写作 / 客服 / 数据分析...
- 输出风格：简洁 / 专业 / 严格 / 友好
- 约束条件：语言、格式、是否允许推测、是否要求步骤化
- 禁止项：不能编造、不能泄露敏感信息、不能越权执行

2) **AI 生成候选（Step 2）**
- 一次返回 2~3 个候选版本（不同“风格强度”）
- 每个候选附带说明：结构特点、适用场景、token 预估
- 用户可“一键采用”或“继续改写”

3) **进入编辑器精修（Step 3）**
- 右侧显示 AI 质量评分（清晰度/约束性/可执行性）
- 提供一键动作：更简洁、补充边界、加强格式要求、改成中文优先

### 3.2 编辑流程：从纯文本框升级为“编辑 + 预览 + 差异”

编辑区改为三标签页：
- **编辑**：主编辑器（支持快捷插入模板片段）
- **预览**：Markdown 实时预览
- **Diff**：与上一版/当前生效版差异对比

建议新增工具栏按钮：
- `AI 优化`：在保留原意下重写
- `AI 纠错`：发现冲突规则（例如“尽量简短”+“必须覆盖所有细节”）
- `AI 补全`：自动补“角色定义/边界/输出格式/拒答策略”缺失段
- `长度控制`：压缩到短版 / 扩展到详细版

### 3.3 列表页补强

- 增加“质量分”与“最后评估时间”字段
- 增加“复制并改写”入口（高频操作）
- 激活前可预览“本条与当前激活条差异”

---

## 4. 技术实现方案（架构层）

### 4.1 前端改造点（chat_app）

1. `SystemContextEditor` 拆分子组件：
- `SystemContextWizard.tsx`（新建向导）
- `SystemContextAiPanel.tsx`（AI 候选与优化动作）
- `SystemContextDiffPanel.tsx`（版本差异）

2. 编辑器增强（优先轻量实现）：
- 保留 `textarea` 起步 + 实时预览
- 后续可升级为 CodeMirror（若你希望更强编辑体验）

3. Store actions 增加：
- `generateSystemContextDraft(input)`
- `optimizeSystemContextDraft(input)`
- `evaluateSystemContextDraft(input)`

4. 交互反馈：
- 用 toast/inline message 替换 `alert`
- AI 请求增加 loading skeleton 与失败重试

### 4.2 后端改造点（chat_app_server_rs）

在现有 `/api/system-contexts` 基础上新增 AI 相关接口：

- `POST /api/system-contexts/ai/generate`
- `POST /api/system-contexts/ai/optimize`
- `POST /api/system-contexts/ai/evaluate`

实现建议：
- 新建 `PromptAssistantService`（可放 `src/services/`）
- 复用现有 AI 模型配置与调用链（避免重复造轮子）
- 对返回结果统一结构化：`content`, `highlights`, `score`, `warnings`

### 4.3 数据结构扩展（建议）

新增表 `system_context_versions`：
- `id`, `system_context_id`, `content`, `source`(manual/ai_generate/ai_optimize), `created_at`

可选新增字段：
- `quality_score`（0-100）
- `quality_report`（JSON）

这样可支持“回滚、对比、A/B 迭代”。

### 4.4 AI 生成策略（关键）

统一元提示（meta prompt）模板，强制输出结构：
- 角色定位
- 目标任务
- 能做/不能做
- 输出格式
- 异常与拒答策略

并加入防劣化规则：
- 禁止输出空泛词堆砌
- 禁止互相冲突约束
- 限制最大长度与重复率

---

## 5. API 草案（供开发直接落地）

### 5.1 生成
`POST /api/system-contexts/ai/generate`

请求示例：
```json
{
  "user_id": "u_123",
  "scene": "编程助手",
  "style": "专业简洁",
  "constraints": ["使用中文", "优先给可运行代码"],
  "forbidden": ["编造不存在的 API"],
  "candidate_count": 3
}
```

响应示例：
```json
{
  "candidates": [
    {
      "title": "平衡版",
      "content": "...",
      "score": 86,
      "highlights": ["边界清晰", "格式明确"]
    }
  ]
}
```

### 5.2 优化
`POST /api/system-contexts/ai/optimize`

请求示例：
```json
{
  "user_id": "u_123",
  "content": "当前的 system prompt 文本",
  "goal": "增强约束和可执行性",
  "keep_intent": true
}
```

响应示例：
```json
{
  "optimized_content": "...",
  "score_before": 71,
  "score_after": 89,
  "warnings": ["有一条约束语义重复"]
}
```

### 5.3 评估
`POST /api/system-contexts/ai/evaluate`

响应返回分维度评分：
- clarity
- constraint_completeness
- conflict_risk
- verbosity

---

## 6. 分阶段落地计划（建议 3 个迭代）

### Phase 1（快速见效，1~2 天）
- 新建流程接入 AI 生成（单候选也可）
- 编辑区增加 Markdown 预览
- 引入基础评分与 warning 展示

### Phase 2（体验升级，2~4 天）
- 候选多版本 + 一键改写
- Diff 对比 + 复制改写 + 回滚
- toast/状态反馈完整化

### Phase 3（专业化，3~5 天）
- 版本表落库
- 评估模型完善（可加入 token 成本估算）
- 可选升级为 CodeMirror 编辑器

---

## 7. 验收标准（Done Definition）

1. 新建时可通过 AI 在 30 秒内生成可用初稿。
2. 编辑时可一键优化且能看到前后差异。
3. 保存前能看到质量评分和至少 1 条可执行建议。
4. 任何 AI 改写都不直接覆盖原文，必须用户确认。
5. 失败场景有清晰提示且可重试。

---

## 8. 建议优先修改文件清单

前端：
- `chat_app/src/components/SystemContextEditor.tsx`
- `chat_app/src/lib/store/actions/systemContexts.ts`
- `chat_app/src/lib/api/client.ts`

后端：
- `chat_app_server_rs/src/api/configs.rs`
- `chat_app_server_rs/src/services/`（新增 PromptAssistantService）
- `chat_app_server_rs/src/db/mod.rs`（如启用版本表）


如果你愿意，我下一步可以直接按这个方案给你做 **Phase 1 的代码实现**（先把 AI 生成 + 预览 + 优化按钮跑通）。
