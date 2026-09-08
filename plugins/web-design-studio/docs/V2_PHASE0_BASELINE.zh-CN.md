# Web Design Studio v2 阶段 0 基线

本基线用于阻止 v2 重构再次退化为“组件很多，但不能稳定设计网站”。后续阶段必须持续运行同一套网站、视口和能力测试。

## 基准范围

- 12 类网站：AI SaaS、消费品牌电商、数字杂志、创意机构、个人作品集、国际大会、餐厅酒店、医疗健康、在线教育、公益行动、地产项目、开发者平台；
- 10 个 CSS 视口宽度：320、390、768、1024、1280、1440、1920、2560、3840、7680；
- 8 个评分维度：视觉层级、布局节奏、连续响应式、排版、品牌原创性、内容完整度、交互状态、可编辑与可追踪性。

可执行的完整 Brief 和评分权重位于 `src/v2/phase0-baseline.ts`，不是仅供阅读的文档。

## 必须保留的现有能力

以下能力允许内部重写，但不允许在 v2 中丢失：

1. 宿主透传的 ChatOS projectId 必须参与运行时隔离；
2. 项目只管理设计归属，不能修改设计布局数据；
3. revision 冲突必须明确失败，不能静默覆盖；
4. 文档继续使用锁与原子写入；
5. MCP 工具从宿主继承 scope，不能让调用者伪造 projectId；
6. 第三方组件继续运行官方真实实现，不能回到仿写组件。

## 当前版本已知基线问题

- 数据以扁平组件数组和绝对坐标为主，复杂嵌套表达不足；
- desktop、tablet、mobile 是离散覆盖，不能表达任意宽度的连续布局；
- 保存可以保留数值，但布局求解过程还不能保证所有内容变化后的像素稳定；
- AI 需要直接管理大量节点坐标，缺少 Design Spec 和布局求解层；
- 视觉检查主要依赖人工，没有统一截图、评分和自动修正闭环；
- 工作区、组件浏览、AI 和内部编辑集中在单体界面状态中。

## 阶段 0 完成门槛

- 基准目录、视口矩阵、评分规则可以由测试直接读取；
- projectId、项目归属、保存精度和 revision 冲突均有冻结测试；
- 每类网站都有生成前 Brief、生成后结构摘要和 10 个视口截图位置；
- 基线报告不能只写“通过”，必须保留失败项与截图；
- 阶段 1 开始后，所有 Scene Graph 与布局引擎变更都必须重新运行此基线。

## 截图采集

截图采集器读取一次运行定义，并展开成 12 类网站 × 10 个视口的 120 个截图任务。每张截图记录来源 URL、projectId、documentId、视口、文件哈希、文件大小和错误信息。

```json
{
  "runId": "v3.0.1-before-scene-graph",
  "sourceVersion": "3.0.1-phase0",
  "createdAt": "2026-09-07T00:00:00.000Z",
  "designs": [
    {
      "benchmarkId": "saas-product",
      "url": "http://127.0.0.1:4188/?studio-project=...&studio-design=...",
      "projectId": "project-...",
      "documentId": "website-..."
    }
  ]
}
```

完整运行要求包含全部 12 个 benchmarkId：

```bash
npm run baseline:capture -- --manifest ./run.json
```

开发采集器时可显式传入 `--allow-partial`，但部分结果不能用于阶段验收。

## 当前生成器基线

阶段 0 使用隔离的数据目录和端口运行旧版工作台，不能把基准设计写入用户的真实项目。当前版本先记录确定性的内置 Landing Page 生成路径，并明确标记为 `legacy-built-in-landing-template`；它不是 v2 的质量目标，也不能冒充新的 AI 设计协议。

```bash
WEB_DESIGN_STUDIO_DATA_DIR=.web-design-studio-baselines/legacy-data \
WEB_DESIGN_STUDIO_PORT=4288 \
CHATOS_CONTEXT_SCOPE=project \
CHATOS_PROJECT_ID=v3-phase0-baseline \
npm run studio

npm run baseline:seed -- --base-url http://127.0.0.1:4288/
npm run baseline:preview -- --manifest .web-design-studio-baselines/legacy-current-run.json
npm run baseline:capture -- --manifest .web-design-studio-baselines/legacy-current-run.json
npm run baseline:report
```

种子脚本会复用相同标题的设计，重复运行不会不断创建副本。清单同时保存工作台 URL 和轻量设计预览 URL。截图针对设计预览执行，避免把组件库浏览器的加载时间混入网站视觉结果；projectId、documentId 和工作台 URL 仍然保留，用于验证宿主范围与设计 URL 的透传行为。

报告会统计截图失败、横向溢出、宽屏内容利用率、不同 Brief 的像素重复率和文档结构重复率。它不会用这些机械指标冒充完整视觉评分；层级、品牌、内容和交互仍需视觉评审。

当前已经建立可执行目录、核心能力冻结测试、隔离种子数据、设计预览服务、截图采集器和自动失败报告。首轮人工评审记录在 `V2_PHASE0_VISUAL_REVIEW.zh-CN.md`，阶段 0 已完成，可以进入 Schema v2 实施。
