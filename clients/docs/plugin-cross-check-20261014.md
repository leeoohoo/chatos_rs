# macOS 其他插件交叉验收（2026-10-14）

## 结论与证据边界

本轮测试 Browser 之外的五类插件。五类源码回归通过，五个本机正式安装包的逐文件完整性、MCP 目录和 Skill gate 声明审计通过。安装版 Document、Diagram、Solution、Web 的隔离业务 MCP 测试通过；Computer Use 只验证权限查询，不代表桌面操作验收完成。

原生 UI 在唯一 `/Applications/ChatOS.app` 中测试；没有启动第二份 ChatOS、重装、重启、发布或升级插件。首次模型请求遇到上游 HTTP 500。用户授权仅切换测试项目到 gpt-6-luna 后，五类真实模型测试已跑完：Diagram 成功、Computer Use 权限查询成功、Document/Solution/Web 失败，汇总任务被依赖失败阻塞；系统仍回写了最终部分完成汇总。不能用此前源码/MCP/UI 通过结果覆盖这次实机失败。

## 安装版与覆盖范围

| 插件 | 本机版本 | 工具 / Skills | 源码回归 | 实际安装包业务 MCP | 原生 UI |
| --- | --- | --- | --- | --- | --- |
| Document | 0.1.9 | 15 / 5 | 11 项通过 | 9 项通过：Office 创建/编辑、DOCX/PDF/表格/演示文本提取、渲染/验证、PDF 操作、路径安全 | 本轮未从原生聊天打开生成文档 |
| Diagram | 0.4.6 | 18 / 7 | 57 项通过 | prepare/commit、结构门控、幂等、作用域隔离、validate/export | 新建、源码编辑、应用到画布、保存；5 节点 4 条连线，方向和无重叠目视通过 |
| Solution | 0.1.10 | 10 / 5 | 26 项通过 | 需求→设计→四任务执行计划→回读→finalize；拒绝循环依赖且不落盘，拒绝缺失设计块 | 新建方案、需求保存/详情、执行计划前置设计提示通过；四任务/T-004 原生交互仍待验收 |
| Web Design | 3.0.30 | 18 / 9 | 322 项通过 | 目录/搜索/创建、宿主作用域、站点/页面规划、候选生成→确认提交→Scene 查询、移动端 PNG 捕获 | 新建设计/保存、7 个组件库入口；AntD Button 官方预览实际加载并目视通过。原生空白设计尚未由模型生成 Scene |
| Computer Use | 0.8.19 | 11 / 5 | 16 项 Swift Testing 通过 | initialize/tools-list、`check_permissions` 返回通过；两权限均为 true，未弹 onboarding | 未截图、点击、输入、切换应用或申请权限 |

审计检查每个工具的 router/specialist Skill gate、实际 Skill 文件和版本。MCP `tools/list` 是插件完整目录审计，不是证明模型首轮拿到全部工具，也不是证明主聊天渐进披露已完成实机验收。

Solution 四任务业务 fixture 包含两个需求：T-001→T-002/T-003→T-004，T-004 属于 R-002、依赖 T-002 与 T-003。回读核对任务总数、ID、依赖、所属需求、预期输出与验收标准；这验证持久化/业务接口，不冒充 React 原生点击验收。源码回归另外覆盖真实需求详情组件的 T-004 跨需求展示。

Web fixture 直接使用安装版服务器完成真实渲染，不导入源码 store 伪造持久化；生成候选必须返回 PNG，确认后必须可查询 Scene，移动端截图必须是有效 PNG。仅接受结构步骤时交付 gate 仍拒绝最终实现交付，避免把未完成设计判为交付成功。此 fixture 未完成全部 design-gate/handoff，亦非最终网站质量验收。

## 真实模型执行与结果

隔离项目：`插件验收-20261014`，ID `9006ee1d-e93e-418c-8fd9-bbe35d24b198`。

发出五类插件的虚构业务测试请求后，默认 `my_api / MiniMax-H3` 在 iteration 1 失败：

- Run：`run_b4b5ea5c-a9c7-4b10-a3a6-eadd50783d82`。
- 上游：`500 Internal Server Error`，`new_api_error / convert_request_failed`，`not implemented`。
- Request ID：`202610140322454858780308268d9d6ooIuPaEE`。
- 此 Run 没有创建后台 Task、没有调用插件，不计作任何插件业务成功或插件业务失败。

用户随后明确授权仅将测试项目改为 gpt-6-luna。通过正式客户端模型下拉切换并核对显示 `my_api / gpt-6-luna`，在 2026-10-14 12:37（北京时间）发送新请求，没有重试旧 Run、修改其他项目或改请求协议。

主聊天 Run：`run_9dfe8deb-5f32-40a2-9b1c-9dd9d0298a80`，5 iterations 正常完成。Task graph：`local-task-graph-ae10beea-970d-48c9-9e2e-0cd152ef85fb`，五个独立插件任务和一个汇总任务，没有为五个业务任务人为添加串行依赖；实际受项目级执行锁串行调度。

| 插件 | 业务 Task 结果 | 实际证据与问题 | Task Run / iterations |
| --- | --- | --- | --- |
| Document | 失败 | 创建 DOCX、inspect/extract 成功，标题/正文/表格正确；validate(renderPages=[1]) 与 render 均返回 `ENGINE_ERROR` | `task-run-640ce05c-6595-4c15-9e42-827758c94c68` / 27 |
| Diagram | 成功 | `flowchart-7749d31a`，5 节点 4 线，三个业务节点顺序正确，独立 validate 的 valid/ready=true、overlapCount=0；SVG 已导出。虚构内容无源码证据 warning 不阻断 | `task-run-02cbe847-49a5-4baa-9de3-b2be1c0677c5` / 25 |
| Solution | 失败 | 更新 `solution-a6257ad6` 到 revision 3，保存两个需求后主动报失败；激活 design Skill 后没有调用设计/计划/finalize。日志无工具异常，18/600 iterations 不是运行次数耗尽，不能把设计/计划暂时为空视作无法继续 | `task-run-04901209-7270-45c6-9afd-dd0e89b17365` / 18 |
| Web | 失败 | `website-91acb82d` 完成站点/页面规划及非空候选；候选审查拒绝后主动结束，没有修复、接受或 handoff。目视核对真实页面 PNG：白底默认文字，没有规划的配色/字体层次/按钮，字面 `\\n` 可见。调用 operations 未提供这些 style，不能先归因于渲染器丢样式 | `task-run-523ac0df-2352-462b-bbda-c612c3362b55` / 23 |
| Computer Use | 查询成功 | 仅调用 check_permissions，辅助功能/屏幕录制均为 true；未申请权限、截图、点击、输入或切换应用 | `task-run-1a797a15-a160-4c98-92a0-9d4e31c2eec5` / 10 |

以上 Task Run 的技术状态都是 succeeded，表示成功提交 outcome；Document/Solution/Web 业务 outcome 为 failed，Task 状态也是 failed，不能混淆。

五类 Task 均实际使用 capability_search→目录/单工具 capability_describe→router/leaf Skill activation→capability_invoke；主聊天也激活编排 Skill 并按需读取参考。Document 大 schema 分页读取，Diagram 读取流程图 examples。此轮没有把全部插件 schema 一次注入；这不代表首轮实际模型请求的工具清单数量已逐项导出审计。

### 文档渲染对照

同一模型生成 DOCX、同一安装版 OfficeCLI 1.0.144、同样 page 1 / 1600×1200 / html 参数，独立 CLI 在临时输出目录渲染成功，目视标题、正文和两列两行表格正常。日志：`/tmp/chatos-document-model-render-compare.log`；PNG：`/var/folders/kd/cq1bxxgd3rg8t8zbwqgprxz80000gn/T/chatos-document-model-render-0tXZjB/page.png`。

这个对照不经过 Native 插件 sandbox/session 环境，不能据此把原生失败改成通过。问题已收窄到运行环境/调用链差异，而不是已证明 DOCX 损坏或中文文件名必然不支持。OfficeCLI wrapper 在 `plugins/document/src/engines/officecli.ts` 将非超时异常统一变成无底层原因的 ENGINE_ERROR，当前证据不足以确定哪个 sandbox/环境约束失败。

### 开始、结束与汇总回调

五个 lifecycle 消息的 created_at 与 started_at 对齐，依次为北京时间 12:37:43、12:39:04、12:40:14、12:41:17、12:42:46，不是任务组创建时一次全部产生。Document 开始消息在任务实际执行时已在聊天目视看到；最终五条消息在原位置更新成成功/失败状态，保留 started_at/finished_at。五个结束回调和具体失败理由在聊天中可见。

汇总任务依赖五项成功，故在 Document 失败后被 blocked，未运行；12:43:12 系统生成 `task_graph_terminal`，聊天实际显示“2 个成功，3 个失败，1 个阻塞”以及分项结果，没有把组整体伪装为成功。单独汇总 Task 未执行与最终系统汇总已回写需要分开记录。

### 默认模型混用风险（只诊断，未实施修复）

用户询问为什么带入视频模型。代码路径确认：`NativeLocalAgentBootstrap.swift` 使用 `.general` 筛选；`LocalConnectorModelSelectionPolicy.permits` 只要求 enabled/hasAPIKey；`NativeLocalAgentConversationRuntimeSettingsService.ensureSettings` 新会话直接取 modelSnapshots.first。没有聊天/图像/视频用途或工具调用适配筛选；实机聊天下拉也包含 gpt-image 条目。本次 MiniMax-H3 是新项目自动默认值，不是测试人员刻意选择媒体生成模型；不能只凭自定义模型名判断上游实际能力。

此处应补按明确模型能力/用途的筛选、有效默认值和协议适配预检，而不是凭名字 blacklist 或更改所有项目。本轮仅按授权切换测试项目，没有修改该生产策略。

### 后续修复优先级

1. 文档原生渲染：保留脱敏、受限的引擎退出/错误原因，并加入真实 Native sandbox 下的安装版渲染验收；不能只靠无 sandbox CLI 测试证明可用，也不能直接放开全部权限。
2. 方案/网页工作流：把“中间阶段缺少尚待生成的内容”和真正不可恢复阻塞区分；设计、执行计划及 candidate reject 后应继续可行的生成/修复。完善 Skill 示例/业务对象合同与安全重试指导，不以诚实报失败替代完成可执行工作。
3. 网页产出质量：操作需包含计划中的布局、样式、真实按钮/视觉锚点与正确换行，审查页面 PNG而非 Diff PNG；校验不通过后修复并重新截图，最终 gate 通过再交付。
4. 模型选择：区分聊天/工具调用和媒体生成模型，检查默认模型可用性及实际协议能力，覆盖新项目默认值及误配置回归。

这些发现已记录但本轮未修改/发布生产实现。仍未通过的端到端项包括文档原生渲染、方案四任务/T-004 原生查看和最终网页交付；Computer Use 桌面动作不在此次授权的只查询验收范围内。

## 隔离与留下的产物

原生测试项目目录：`/Users/lilei/Documents/chatos-plugin-acceptance.GRiYCN`。项目及测试产物保留便于复核，没有删除或重试任何历史业务任务，没有直接改写客户端 SQLite。

- Solution：原先 UI 验收保存 v2；模型随后将同一个 `solution-a6257ad6` 更新为「虚构待办清单方案」revision 3，当前有两项需求，设计/执行计划仍未完成。
- Diagram：「UI入口验收（空白流程图）」已填入订单流程并保存 v2；模型另新增「模型验收-订单处理」`flowchart-7749d31a` 及项目内 SVG 导出，没有覆盖原图。
- Web：「UI入口验收（空白网站）」官方组件预览通过；模型在同一 `website-91acb82d` 上创建规划及候选，候选因质量不合格已拒绝，没有确认成最终 Scene，不冒充生成成品。
- Document：模型生成的「插件验收报告.docx」已注册为本项目 artifact，但原生渲染失败，当前是未通过最终交付验收的产物。

MCP fixture 使用单独临时数据目录、workspace、artifacts 和 Computer Use managed-app 根目录，不读取真实财务/博客/私有文档。安装包没有复制源码覆盖或绕过信任记录；按 Native state 原始逐文件 SHA256 核对，拒绝链接、额外文件、缺失文件、版本不一致。

Browser 不在本轮五类测试中。此前 Browser 0.1.16 正文采集失败记录仍独立保留于 [渐进披露修复验收](capability-tool-progressive-disclosure-fix.md)，本轮结果不覆盖该历史记录，也不表示新做了浏览器正文验收。

## 后续修复发布与四任务并发复核

本文件前述结果是旧安装版本和旧调度器的历史证据，不覆盖。其后已发布并升级 Document 0.1.10、Solution 0.1.11、Web 3.0.31、Browser 0.1.17；客户端也修复插件升级后 Main Chat 继续冻结旧 Release 的问题。

新图 `local-task-graph-3f8c9d85-a665-4f5b-8e04-b116050cd70a` 使用四个不同 Plugin、无依赖且产物互不重叠。Document 已完整通过内容抽取、2×2 表格、原生渲染、重新打开和交付验收，产物 `修复后-文档验收-02.docx` 的 SHA-256 为 `5078e30bd193e247a80e0397367db49556c817c1c2f30f0fda7344b4348b51f1`。

该图同时暴露出旧 Host 的真实串行问题：Plugin 被整项目锁互斥，且协调器单槽领取。修复并原位升级后，未完成的 Solution、Web Design、Browser 三个 Run 同时进入 `running/waiting_tool_result`；聊天中 Web 和 Browser 各自在真正开始时出现独立“已开始”消息。新的锁策略只允许不同 Plugin、不同显式路径并行，同一 Plugin、相同文件、Terminal、External MCP 或不确定写入范围仍保守串行。

自动回归已覆盖协议 35 项、Host 94 项、Storage 100 项及 macOS Skill 覆盖 7 项，限定修改 whitespace 检查通过。整个 dirty 工作区仍有其他历史未格式化文件，因此全仓 `cargo fmt --check` 不能作为本轮限定修改的通过证据；本轮涉及的 Rust 文件已单独通过 `rustfmt --check`。

## 可重复执行入口与测试改动

新增 opt-in 真实安装包入口 `clients/evals/installed-plugin-smoke.mjs`，显式指定 Native state 路径，不修改安装状态或安装目录：

```sh
node clients/evals/installed-plugin-smoke.mjs \
  '/Users/lilei/Library/Application Support/ChatOSSwift/NativeConnector/state.json'
```

先安装仓库中对应插件的测试依赖；Node 测试支持 `import.meta.dirname`。Web 完整业务测试需要本机支持的 Chromium 渲染器；缺失或渲染失败会失败，不能静默跳过该业务测试。渲染用隔离的无头 profile，不控制用户现有 Chrome 页面。

新增 `clients/evals/computer-use-permissions-smoke.mjs` 仅查询权限并校验响应契约。本机安装包入口先核对完整性，再执行该 probe；它本身不替代 Native 信任校验。

四个现有 MCP fixture 支持 `CHATOS_PLUGIN_TEST_PACKAGE_ROOT` 指定真实安装 launcher；默认仍测试仓库源码构建。Solution/Web fixture 增加上述四任务/生成与截图链路，仍纳入原有 npm test。本机安装态专用入口不加入云端 CI，以免把没有本机插件/桌面环境的 runner 当作验收通过。

发现并修复的是测试隔离问题：Document「无 workspace」用例曾继承安装版 runner 的 `CHATOS_WORKSPACE`，造成首轮假失败；现在 undefined 时明确移除变量，保留原有严格断言。四任务新 fixture 首轮也因漏关联第二个需求/误读摘要字段失败，已修正 fixture 并重新通过。没有降低产品验证规则或为测试修改生产实现。

## 日志与复核

最终扩展后的五包执行日志：`/tmp/chatos-plugin-retest-installed-expanded.log`，退出码 0、`allPassed: true`。

逐插件安装版 catalog/business 日志目录：

`/var/folders/kd/cq1bxxgd3rg8t8zbwqgprxz80000gn/T/chatos-installed-plugin-smoke-duD6wR`

源码日志：

- Document 初次 `/tmp/chatos-plugin-retest-document.log`；修改 fixture 后完整 npm test 再跑 11/11，退出码 0（终端输出）。
- Diagram `/tmp/chatos-plugin-retest-diagram.log`。
- Solution `/tmp/chatos-plugin-retest-solution-expanded-source.log`。
- Web `/tmp/chatos-plugin-retest-web-expanded-source.log`。
- Computer Use `/tmp/chatos-plugin-retest-computer.log`。
- 四类 TypeScript typecheck `/tmp/chatos-plugin-retest-{document,diagram,solution,web}-typecheck.log`。
- Skill gate 审计政策 `/tmp/chatos-plugin-retest-audit-policy.log`，5 项通过。

新增入口 Node 语法检查及本轮 fixture 的 diff whitespace 检查通过。这是限定范围的验证，不代表整个 dirty 工作区 CI、Windows 或全部业务场景已通过。临时日志不是永久产物，复核需在清理临时目录前进行，或重新执行入口。
