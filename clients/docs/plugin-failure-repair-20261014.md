# 插件真实模型失败修复（2026-10-14）

本轮针对 [交叉验收](plugin-cross-check-20261014.md) 中 Document、Solution、Web 三项失败，不覆盖此前失败记录，不把源码/MCP 成功等同真实模型成功。

## 根因与修改

### Document：原生沙箱无法创建 Chrome 无头 profile

真实安装 Document 0.1.9 在 Native Stdio + Seatbelt 中可创建 DOCX，但渲染失败。直接运行同沙箱内可信 OfficeCLI，捕获 fixture 的 stdout 后确认错误：`no_screenshot_backend`，Chrome `Failed to create a unique user data directory for headless`。上游 OfficeCLI 1.0.144 的 `HtmlScreenshot` 通过 PATH 找 Chrome，未传 `--user-data-dir`；独立 CLI 不受该限制，所以此前独立渲染成功不足以验收。

Document 0.1.10 新增包内不可变浏览器 launcher。仅 macOS Seatbelt 环境下，OfficeCLI 子进程发现此 launcher；每次渲染用 `mktemp` 在宿主已授权插件缓存内建立独立 profile，完成后仅清理该 profile。仍执行系统已安装 Chrome/Chromium/Edge，保留宿主原有沙箱边界。未开放用户 HOME、整个系统临时目录或外网，也未修改真实安装目录/信任记录。Windows 不使用此 launcher。

引擎错误增加脱敏分类：已知 `no_screenshot_backend` 返回 `ENGINE_UNAVAILABLE`、固定修复提示与退出码；超时保留 `ENGINE_TIMEOUT`；未知错误保留 `ENGINE_ERROR`。不透传 stderr、文档路径、凭据或未知错误文本。

### Solution：中间缺项被误当终止条件

旧真实 Run 保存两项需求后，`design_empty`/`plan_empty` 是尚待写入的内容，不是失败的工具调用。模型激活设计 Skill 后未进行设计/计划/finalize，18/600 iterations 就自行失败退出。

Solution 0.1.11 的 router Skill 明确需求→设计→计划→验证/finalize 要在本 Run 连续推进。成功的业务结果附带 `continuation`，针对空/过期设计、空/过期计划给出阶段 Skill、工具和适用条件；只做需求的请求不会因此被扩成完整方案。finalize 的完成 proof 不再附带继续创作提示。

### Web：示例缺样式，拒绝候选后没有修复

旧真实 Run 的 operations 没有规划的样式；Candidate 真实页面为白底默认文字，并含字面反斜杠 n。不能归因于渲染器丢样式。首次 reject 后模型主动失败，不是预算用尽或工具异常。

Web 3.0.31 的 Scene Skill 示例改为有配色、字体层级、布局、独立按钮容器/标签的可编辑树，明确 lineHeight 的单位和语义文本规则，不提供用户品牌假证据。review/router Skill 明确 reject 是可恢复迭代：候选未提交，查询 canonical Scene、重建被丢弃节点、带最新 Plan revision 重新执行、看新图片后决定接受。reject 结果附带具体 reason 和 recovery 指导，仍保留真实截图及最终 gate/handoff，不自动接受劣质结果。

### 统一 Task 执行策略

macOS 内置 `chatos-task-execution-invariants` 增加继续执行规则。未写完、中间验证缺项、缺 Skill 激活、首次视觉拒绝不等于 Task 失败。要求安全、具体地修正和验证；真正缺权限、重要用户选择、取消、预算耗尽、不明确写入后果、重复失败无进展仍应停止并报具体原因，不无限重试或伪造成功。

## 回归与 CI

- Document：13 项通过，含错误分类/脱敏回归；TypeScript 通过。
- Solution：26 项通过，含需求阶段缺项/继续动作、完整四任务 DAG、回读与 finalize；TypeScript 通过。
- Web：322 项通过；MCP fixture 使用已发布 Skill 内 JSON 示例真实渲染，跑通 reject→检查候选未落盘→纠正生成→accept→移动 PNG→Design Gate→handoff；TypeScript 通过。此自动 fixture 不冒充模型的审美验收。
- Native Plugin Runtime：61 项通过。
- 新增 `NativeDocumentPluginIntegrationTests`：中文 DOCX 标题、正文、2×2 表格通过原生客户端同款 manifest/context/stdio/sandbox 创建和渲染。测试只用临时 fixture；可选原始安装态读取，不写安装状态。
- `bash clients/evals/native-document-sandbox-smoke.sh` 本机通过。此入口构建并解包测试包、审核 Skill gate 和 SBOM、执行真实沙箱渲染；已加入 macOS CI。CI 不再仅以未启用的 opt-in 测试跳过结果判定渲染通过。

本轮 CI 配置尚未在 GitHub 执行，整个 dirty 工作区未被完整重新验收。第三方许可原文的既有尾空格未改写；本轮自身源码与 Skill 的限定 diff whitespace 检查通过。

## 部署、并发修复及真实模型复测状态

修复包已通过正式插件平台发布并由客户端升级：Document 0.1.10、Solution 0.1.11、Web 3.0.31；Browser 0.1.17 同批升级。客户端安装、卸载、启停插件后现在会强制刷新 Main Chat 能力目录和 Release 快照，避免新 Task 仍冻结旧 Release。新建验收图已确认绑定上述版本及平台 SHA；旧 Run 仍保留原冻结版本，不做伪兼容或篡改。

四个无依赖 Plugin Task 原先仍串行，原因有两层：所有 Plugin 都被折叠成整项目运行时写锁；Host 协调器每轮又只有一个模型调度和一个工具调度调用。当前 macOS Host 修改为：

- Plugin-only Task 持有每个 Plugin 独立的合成运行时锁，并叠加显式 `owned_paths`。不同 Plugin 且路径不重叠可并行；同一 Plugin 或同一路径继续串行。
- `TerminalController`、External MCP 和空路径 `CodeMaintainerWrite` 仍持有整项目锁。
- Host 每轮最多并发领取 4 个模型步骤及 4 个工具调用；任务依赖、路径冲突和 Plugin 锁仍由数据库原子领取逻辑裁决。
- 编排 Skill/schema 要求 Plugin 写项目文件时声明 `owned_paths`；可能修改同一文件或范围不确定时添加真实前置依赖或保守串行，不因角色不同就并行写同一文件。

并发回归：协议 35 项、Host 94 项、Storage 100 项通过；新增测试覆盖四个模型 Run 同时进入执行、四个工具调用同时进入执行、Document 与 Browser 同时启动，以及第二个 Document 等待第一个 Document 释放锁。真实图 `local-task-graph-3f8c9d85-a665-4f5b-8e04-b116050cd70a` 在旧客户端中表现为 Document 完成、Solution 运行、Web/Browser 等待；原位升级后 Solution、Web、Browser 同时为 `running/waiting_tool_result`。Web 与 Browser 的“已开始”回调都在实际领取 Run 的 15:59:33 写入，不是在图创建时预写。

Release 客户端已原位安装到 `/Applications/ChatOS.app`，构建时间 `2026-10-14T07:55:52Z`，只运行该目录的一份客户端及其 Host；构建目录 App 未启动。升级前版本完整备份在 `/Applications/ChatOS.app.backup-20261014-155800`。这证明并发调度和开始回调已实机生效；各 Plugin 的最终业务结果仍需与任务本身的验收状态分别记录。
