# macOS 3.0.8 新逻辑与 Windows 全模块对齐任务

建立日期：2026-10-17（北京时间）。

## 2026-10-18 P0 强制门禁

- 当前唯一最高目标改为：从此前不存在、创建后为 0 项且非 reparse point 的目录，在最终 x64 Debug ChatOS 真实“创建项目”界面中创建项目，并在该项目聊天中完成目录、读/搜/写、允许/拒绝/取消审批、回读、修改、diff、复制/移动/重命名、异常矩阵及关闭重启恢复。
- P0 真实 UI 矩阵完成前，W308-01..17、Plugin、Skill、MCP、安装器、Solution Studio、REA、Media、Pet、远程和打包均不得抢占。编译、单测、静态合同、ViewModel、窗口启动或复用旧项目不能替代门禁。
- 权威待选目录为 `C:\ChatOS-ProjectChat-Acceptance-20261018-01`；2026-10-18 06:05 +08:00 复核仍为 0 项、非 reparse point、无预置内容。由于当前工具面没有 Computer Use 所需 `node_repl/@oai/sky`，真实项目仍未创建、项目聊天仍未操作。
- 已补齐创建项目弹窗关键状态 AutomationId，并补上项目聊天 `project_copy`、`project_move` 在共享 Host 的 `CodeMaintainerWrite` 放行、Windows 执行与审批链。自动化和最终构建通过仅表示实现准备，不是 P0 验收通过。证据见 `.local/parity/2026-10-18-project-chat-real/acceptance-summary.md`。
- 审批卡现可通过稳定 ID 读取来源、风险、准确命令、工作目录、原因和队列状态；新的审批把键盘焦点移到安全的“拒绝”动作。该项合同 2/2、最终 x64 Debug 通过，但仍只属于 P0 自动化准备。

用户要求：替换旧定时任务，继续让每个模块在 Windows 上真实跑通，并把新合入的 macOS 逻辑完整实现到 Windows。

新自动化：`windows-macos-3-0-8`，显示名“Windows 全模块验收与 macOS 3.0.8 新逻辑对齐”，本地项目 `chatos`，状态 ACTIVE，每 15 分钟运行一次，沿用原 `gpt-5.6-sol` / high 设置。旧自动化 `windows` 已删除，原配置和历史 memory 已备份；新任务继承历史证据，不继承旧统计作为新版本通过证明。

## 基线和执行范围

- 当前分支 `3.0.8.1`；合并提交 `cf1e99b9f`；本轮上游基线 `a0f5827c5`，比上次基线新增 13 个提交、修改 520 个文件。
- 旧任务记录为 111 项中完成 51 项、开放 60 项。这是历史进度，不能当成新版本已经通过的结论；受新增逻辑影响的旧勾选项必须重新回归。
- 本次合并后 Windows Connector 常规测试 562 项通过。这不等于全模块或新增业务已经验收。
- 保留工作区全部 staged、unstaged、untracked 修改。不得 reset、clean、覆盖现有工作，也不为验收自动提交、推送或变更当前分支。
- 主要实现范围是 `clients/windows`。必要时修复 `clients/local_agent_host`、共享 Rust crates 和相关插件契约，并验证既有调用方；不能再因“共享 Host 不在 Windows 编辑范围”长期挂起可修复问题。
- 历史文档中“本轮不开发 Windows”的表述描述原 macOS 工作包，不限制用户新授权的 Windows 工作包。采用同一协议、存储和权限边界，不另造宽松兼容层。

## 全模块覆盖

继续维护 `../MAC_PARITY_CHECKLIST.md`、`11-macos-parity-progress.md`、`18-windows-parity-current-status.md`、`02-capability-parity-matrix.md` 和各专项验收文档。首次运行审计全部模块及新增业务，补齐矩阵、依赖、实施状态、真实验收状态和下一步；之后每轮选择能形成真实闭环的一组，按风险与依赖推进，并定期巡检所有模块，避免长期只修一个页面。

必须覆盖：

1. 登录、注册、认证恢复、登出、账号切换和生命周期。
2. 主窗口、资源导航、项目及联系人入口、全局状态。
3. 主聊天、流式回复、附件、Markdown、模型与推理、Ask User。
4. Task、DAG、事件、审批、验收、重试、恢复与最终交付。
5. 项目创建、完整文件树、读写、复制移动、Git 和 Run。
6. Media Studio 图片、视频、剧情、历史、取消、失败恢复和导出。
7. Agent、Agent Teams、群聊、需求调研、调度和恢复。
8. Connector、设备、配对、本地 Host、模型、权限、沙箱和审批。
9. Notepad 的完整编辑、目录、搜索、失败保护和持久化。
10. Pet、快捷聊天、审批、文件工作台、窗口交互和恢复。
11. Plugin、Skill、MCP、远端 SSH/SFTP、终端、剪贴板、搜索、截图和录屏。
12. 设置、中文/英文、主题、缩放、视觉、焦点、键盘和可访问性。
13. 构建、资源、启动、退出、进程树回收、重启、安装与升级相关回归。

## 本轮新增逻辑的验收清单

下列状态均为待审计，表示需要对照实际源码确认 Windows 缺口；不预先断言已经缺失或已经完成。

| 编号 | 对齐内容 | 主要依据 | Windows 完成条件 |
| --- | --- | --- | --- |
| W308-01 | 模型与推理选择、Task/Run 冻结的模型配置 | macOS 模型能力目录；共享 `model_reasoning.rs`；上游 `8ad828fd0` | 支持级别、默认值和关闭语义一致；Task/Run 选择不因刷新或历史模型过滤被改写；界面、请求、重试与重启均验证 |
| W308-02 | 需求调研的补充问答、方案生成与版本确认 | `clients/docs/local-agent-requirement-plan-binding.md`；macOS Requirement Survey/Plan 客户端与界面 | 展示真实方案版本、状态和 Markdown；只由 human UI 确认准确 revision；冲突刷新、失败、取消与重启有证据；确认本身不启动执行 |
| W308-03 | 方案绑定 Main Chat/Task 与版本快照 | 同上；共享 requirement-plan 存储与 Task 协议 | Task 冻结方案正文、hash 和适用范围，重试保留原版本；不能用聊天记忆或最新方案替换历史合同 |
| W308-04 | Task 验收合同、证据和最终原文 | `clients/docs/local-agent-task-acceptance-contract.md`；macOS `MessageTaskAcceptance.swift` | 区分执行成功与验收通过；展示逐项 passed/failed/not_verified、证据、原文、字符限制；缺证据不得标为通过；Unicode 与旧 Task 兼容有回归 |
| W308-05 | 跟进处理、重试补充指令与恢复 | 共享 `task_followup*`、`task_retry_input.rs`、`planner.rs`；上游 `f661f8197` | 补充指令真实进入新 Run；`resolve_task_followup` 链路可用；无虚假 blocker、重复执行或未知副作用自动重试；断流/租约/取消与重启可解释 |
| W308-06 | 任务图、历史引用和回调摘要 | 上游 `fc2058e40`；macOS Task Graph/Conversation 展示；共享 writeback | 开始和完成更新同一状态消息；不重复摘要；历史成功节点引用不冒充重新执行；分页、详情与重启语义一致 |
| W308-07 | Plugin MCP component 选择和冻结 | `plugins/solution-studio/docs/2026-10-16-dual-mcp-and-modular-planning.md` | Main Chat 以 `pluginKey + componentKey` 选择；Task hint、快照、provider 贯穿同一 component；缺 component 明确失败，不隐式启动全部服务 |
| W308-08 | Solution Studio 方案创建器与任务执行器 | 同上；新版共享插件 | 同一包/项目数据支持双 MCP；authoring/execution 工具面严格分离；模块视图、依赖、ready 判定、领取/回写与真实项目执行闭环 |
| W308-09 | 自定义 MCP/Plugin 安装与预检 | `docs/plans/3.0.8-agent-plugin-mcp-compatibility-plan.zh-CN.md`；macOS 安装器与设置页 | 本地目录、HTTPS Git、固定版本 npm 来源可预检/安装/卸载/恢复；校验真实 tools/list 和指纹；来源漂移拒绝；失败不留半安装 |
| W308-10 | 独立插件安装助手 | 同上；上游 `6ebc7eba5` | Windows 有真实可用入口和独立安装会话；安装工具只给 `plugin-installer`，普通聊天、项目与 Task 不泄漏安装权限；助手不获得通用终端/项目编辑权限 |
| W308-11 | 主流 Agent Plugin、Skill 与 MCP 导入 | 同上；上游 `7a02a9b5d` | 按 macOS 支持边界验证 Codex、Claude、portable Plugin、独立 Skill 及 Cursor/VS Code/Gemini/Cline/OpenCode 等配置；不支持的传输/贡献类型明确拒绝；不能仅解析清单就宣称可执行 |
| W308-12 | REA canonical 包与真实 stdio MCP | 上游 `a0f5827c5`；macOS REA 示例 | 使用同一正确包名和固定版本；真实预检、安装、工具发现/调用、持久化及重启恢复通过，不能以旧包装脚本替代 |
| W308-13 | 安装器 Windows 路径、权限和进程生命周期 | 兼容方案第 5/6 节；Windows 既有 Job Object/凭据实现 | canonical path、reparse point、隔离目录和 ACL 校验；npm 禁 scripts、Git 禁 hooks/file protocol；参数结构化；secret 走 Credential Manager/DPAPI；取消/超时/卸载清理进程树 |
| W308-14 | Skill 门控、渐进披露与外部工具 schema 校验 | `clients/docs/capability-tool-progressive-disclosure-fix.md`；共享 Skill runtime；macOS schema validator | 复用同一目录/策略/权限，不堆叠零散平台 prompt；真实消费工具声明；已知参数/JSON-RPC 失败可修正，未知副作用保持需检查；越权与不支持 schema 明确失败 |
| W308-15 | 运行中配置刷新、历史模型凭据和 Host 生命周期 | Task 验收合同的配置刷新/执行中断章节；macOS Host 生命周期 | 相同 owner/凭据刷新不重启活跃 worker；历史冻结 Run 的有效凭据保留；真正凭据变化按约定延后；不记录秘密，不用竞态空闲判断冒充安全热重启 |
| W308-16 | 附件、插件产物与项目文件交付 | 新版 macOS attachment/file-link 流程；document/browser/diagram/web-design 相关更新 | 图片/文档真实送入模型，插件产生的文件能定位、预览、打开或导出；项目隔离、失败恢复、版本和权限有真实证据 |
| W308-17 | 本地化、打包资源和运行时资源定位 | 上游 `90b3231d9`、`cc26a6adb`、`05203a342`、`da0b5d879` | Windows 对应的命令用途、验收说明、中英文提示完整；Skill/模型能力/运行资源正确随 Windows 构建产物分发，启动与重启可定位，无开发目录依赖 |

## 2026-10-17 首次全模块差异盘点

状态只使用 `待实现`、`实现中`、`自动化通过`、`真实验收通过`；括号内的子链路通过不等于整行通过。历史 51/111 只作为旧基线，本表不会把合并或 562 项 Connector 测试计为新功能完成。

| 模块组 | Windows / 共享实现位置 | 当前状态 | 缺口、依赖与优先级 |
| --- | --- | --- | --- |
| 1. 账号生命周期 | `AuthenticationService`、Shell 账号菜单、Credential Manager | 实现中 | 同账号登出/恢复已有真机证据；第二账号切换、未保存状态及跨账号缓存审计仍缺。依赖第二验收账号，P1 |
| 2. Shell / 导航 | `MainWindow`、项目/联系人/本机/远端资源路由 | 实现中 | 主路径和重启可用；刷新/创建/空/加载/错误全矩阵未闭环，P1 |
| 3. 主聊天 / 附件 / Markdown / Ask User | `ConversationSessionViewModel*`、`Markdown/`、`AskUserPromptViewModel` | 实现中 | 发送、附件、Ask User 多组真机通过；W308-01、W308-16 受影响回归及完整 Markdown/审批/timeout 仍缺，P0 |
| 4. Task / DAG / 审批 / 恢复 | `MessageTaskGraph*`、`WindowsLocalAgentTaskClient`、共享 Host task runtime | 实现中 | W308-05 的新重试上下文子链路本轮真机通过；W308-02..06 完整合同、follow-up、分页/租约/副作用恢复仍缺，P0 |
| 5. 项目 / 文件 / Git / Run | Projects 相关 Desktop/Presentation/Connector 服务 | 实现中 | 2026-10-17 用户将新项目聊天读/搜/改设为最高优先级；首次会话资源回写、加载/失败/重试和标签重入已实现并自动化通过，真实新项目读/搜/写审批/修改/重启仍缺。历史创建、CRUD、Git、Run 证据不能替代该主链，P0 |
| 6. Media Studio | `MediaGenerationService*`、`StoryPlanningService`、Media Studio 页面 | 实现中 | 图片/视频/剧情代码存在，真实供应商成功、取消、失败、历史、导出和重启尚未系统验收；依赖供应商条件，P2 |
| 7. Agent / Teams / 需求调研 | `AgentTeamWorkspaceViewModel*`、AgentTeams Connector、Requirement Surveys | 实现中 | 调度/存储自动化较多，但 Windows 真机闭环缺；W308-02/03 DTO、方案 revision 确认与 Task 冻结 UI 缺口优先，P0/P1 |
| 8. Connector / 模型 / 沙箱 / Host | `WindowsLocalAgent*`、Connector/模型/审批/沙箱设置 | 实现中 | IPC 41→42 已修复并真机消除 identity 错误；2026-10-17 21:50 又对齐 bootstrap 禁 worker、同凭据 worker 复用和变化凭据延后，自动化/构建通过。设备互斥、真实长 Run/历史凭据/重启矩阵仍缺，P0 |
| 9. Notepad | `NotepadViewModel*` 与 Notepad 页面 | 实现中 | 保存、搜索、目录和重启证据充分；慢/失败/权限/删除及完整布局仍开放，P1 |
| 10. Pet | `PetOverlayViewModel`、Pet ToolWindow/Inbox | 实现中 | Ask User/文件工作台子链路已有证据；多 DPI、位置、失败、账号切换与完整窗口交互仍缺，P1 |
| 11. Plugin / Skill / MCP / SSH / 全局工具 | Plugins、Remote、Terminal、QuickSearch、Clipboard 等 | 实现中 | W308-07 component 链已实现但真实安装/执行仍缺；W308-08..14 的双 MCP 闭环、安装器、导入、REA 仍是明确产品缺口，P0 |
| 12. 设置 / 本地化 / 主题 / 可访问性 | Settings、Localization、资源字典、AutomationId | 实现中 | 局部自动化和真机证据存在；中英/主题/缩放/键盘/读屏与 W308-17 打包资源矩阵未闭环，P1 |
| 13. 构建 / 资源 / 退出 / 重启 | `build.ps1`、`DesktopShutdown`、打包脚本 | 实现中 | Debug/Release x64 构建和无签名 MSIX 已通过；包内 runtime/Host/PRI/Pet/双语声明及独立目录 arbitrary-CWD 启动/退出归零已验证。签名安装/升级、packaged AUMID、ARM64、干净账号和符号包仍缺，P0/P2 |

### W308-01..17 实现位置、缺口与依赖登记

| 编号 | Windows / 共享实现位置 | 首次审计结论 | 状态 | 依赖 / 优先级 |
| --- | --- | --- | --- | --- |
| W308-01 | Windows `WindowsModelReasoningCapabilities`、聊天运行时选择器、Task 图；共享 `model_reasoning.rs`、`task_model_policy.rs`、`planner.rs` | Windows 已接 `gpt-5.6-sol` 的 `low/medium/high/xhigh/max` 能力、模型/推理持久化、历史 model ref/revision 展示；修复重复 runtime-ready 刷新清空集合导致重启选择丢失，并用 `IsApplyingSettings` 阻止程序恢复触发隐藏写。真实陈旧 revision 继续 fail-closed，但模型目录和中英文恢复指引保持可用；Human 显式重选 `gpt-5.6-sol` 后恢复 `medium`，重启 version/timestamp 不再变化。Presentation 57/57、Connector 27/27、Debug x64 通过。仍缺真实当前 Task/Run 冻结 thinking level，以及刷新/过滤/禁用/关闭/重试完整矩阵 | 实现中（能力、持久化、陈旧 revision 显式恢复、重启和无隐藏写子链路真实通过） | 需要真实 Task/Run 样本及完整模型状态矩阵，P0 |
| W308-02 | Windows Requirement Survey 页面；共享 `requirement_survey_tools.rs` | 已接 `get/save/confirm_requirement_plan`、源 Run identity/终态校验、revision/Markdown UI、生成成功门禁及 human-only 精确确认；旧成功 final reply 仅经显式 UI 保存 v1 并确认，读取无副作用，失败/不匹配来源及漂移 fail-closed，且不启动执行。自动化、最终构建、真实空态/退出/重启通过；当前账号无真实调研，完整成功/失败/取消/冲突/持久化仍缺 | 实现中（协议/UI/自动化通过，真实方案样本待验收） | 继续真实方案矩阵，P0 |
| W308-03 | 共享 requirement plan store/planner；Windows `MessageTaskRequirementPlan`、Task graph service/ViewModel/XAML | 共享 Host 已在图创建事务内冻结确认方案；Windows 现从 Task 自身输入严格解码 confirmed snapshot，独立复算 Markdown SHA-256，校验 task scope/title，并展示 revision/正文/prompt/交付/验收/hash，不读取当前项目方案替换历史。Core 39、Presentation 862、Connector/桌面合同定向 31、Host 冻结 4、最终构建和旧 Task/重启通过；当前账号无 plan-bound Task，真实 v1→v2 历史/重试仍缺 | 实现中（代码与自动化通过，真实冻结样本待验收） | 依赖 W308-02 真实方案样本，P0 |
| W308-04 | 共享 `task_acceptance.rs`、task tool schema/planner；Windows `MessageTaskAcceptance`、Task Graph DTO/ViewModel/XAML | Windows 已实现 v1 冻结合同/报告严格校验、Unicode scalar 复算、逐项状态/evidence/最终原文/字符上限 UI 和缺失报告 fail-closed；自动化、最终构建、历史 Task 兼容、退出/重启通过。现存真实数据均为旧合同，尚无 v1 succeeded/failed/not_verified 报告真机样本 | 实现中（代码与自动化通过，真实 v1 报告待验收） | 需要真实 v1 Task 样本，P0 |
| W308-05 | `WindowsLocalAgentTaskClient`；共享 `task_followup*`、`task_retry_input.rs`、`planner.rs` | 本轮修复 IPC 42 后，真实补充指令进入新 Run，未重放旧写操作、无虚假 blocker，退出/重启结果恢复；完整 follow-up/租约/取消/未知副作用矩阵仍开放 | 实现中（重试子链路真实验收通过） | 继续真实 `resolve_task_followup`，P0 |
| W308-06 | Windows `MessageTaskGraph*`；共享 task conversation writeback | 终态摘要去重测试通过，已有 Task 图/重启证据；历史引用、同一状态消息更新、分页和完整回调策略受影响回归未闭环 | 实现中（摘要子链路自动化通过） | 可与 W308-05 联测，P0 |
| W308-07 | `WindowsLocalAgentPluginChoice`、Task authorization、Plugin executor、Main Chat capability picker | 已实现 `pluginKey + componentKey` 精确选择、Plugin/Release/version/artifact 冻结、缺失/不匹配/漂移 fail-closed、executor 单 component 启动与成功后一次性消费；自动化/构建和未安装空态通过，真实双 component 调用/持久化/重启仍缺 | 实现中（代码与自动化通过，真机安装/执行待确认） | W308-08..12 前置，P0 |
| W308-08 | `plugins/solution-studio` 双 MCP manifest/runtime；Windows Plugin 选择 UI | 共享包已声明 authoring/executor 双 MCP，Windows component 链已接入；当前目录显示未安装，未获 action-time confirmation，故 authoring/execution 工具面分离和项目闭环尚未真机验收 | 实现中 | 依赖真实安装/执行，P0 |
| W308-09 | Windows `WindowsPluginPackageInstaller`、`WindowsCustomPluginInstaller`、插件设置页 | 本地目录与固定 npm MCP 保留；HTTPS Git 现要求无凭据/无 query/fragment URL、明确 ref 与安全相对子目录，使用无 shell/无交互 Git、禁 hooks/file protocol/子模块、Job 控制和双获取指纹。设置页与独立安装助手均贯通，来源变化取消预检。定向 47/47、Connector 可执行集 691 通过/5 条件跳过、Debug 0 警告错误。真实公开 fetch 曾成功，随后因 `github.com:443` 不可达无法稳定复验；UI 安装/重启、生产 AppContainer 和卸载恢复仍缺 | 实现中（Git 代码/自动化通过；真实网络与 UI/生产安装未闭环） | Computer Use 恢复后验证三来源 UI；网络恢复后做 Git Plugin 二次获取/安装/重启；继续 W308-13 卷根前置，P0 |
| W308-10 | Windows Shell `plugin-installer` 联系人、Conversation Session、installer capability/tool executor | 已有独立入口与持久会话、不可变 installer capability、仅安装工具目录、Run+conversation 运行时重授权、边界横幅和普通 Plugin picker 隐藏；普通 Main Chat/Task 无安装工具。Presentation 867、定向合同 55、Host planner 隔离 1、最终 Debug 构建通过；真机切换/重启和不存在路径的真实 `plugin_installer_inspect_source` 负向路由通过，未安装且无残留。2026-10-17 21:01 对齐 macOS 安全错误摘要：受信任 `PluginPackageException` 可给出可修正原因，任意原生异常和敏感标记仍脱敏；定向 34/34、Debug 0 警告错误、最终产物启动/正常退出通过。可见文案仍待真实 UI 复验，成功生产预检/安装受 W308-09/13 卷根前置阻塞 | 实现中（核心隔离与负向错误摘要自动化通过） | 依赖 W308-09/13 成功生产协议；补真实错误文案和正向安装/重启，P0 |

## 2026-10-17 11:06 本轮入口：W308-10 独立安装助手核心隔离

- 独立 `插件安装助手` 联系人/会话、installer-only capability/tool catalog、普通会话/Task 排除和运行时 Run+conversation 重授权已落地；界面显示权限边界横幅，且不展示普通 Plugin component picker。
- 自动化为 Presentation 867/867、installer/conversation/bootstrap/workspace/desktop 定向 55/55、共享 Host planner 隔离 1/1；最终 x64 Debug 构建 0 警告/错误，`git diff --check` 通过。
- 最终客户端真实完成入口、普通聊天边界消失、重启会话/历史/模型/推理恢复，以及不存在路径的 `plugin_installer_inspect_source` 负向调用。没有安装或创建目录；正常退出后 Desktop/Host/Connector/testhost/vstest 为 0，两个 sandbox roots 为空。
- 严格状态保持“实现中”：负向错误当前只显示 `Local platform tool failed`，成功生产预检/安装仍依赖 W308-09/13 的卷根 AppContainer 前置。W308 仍 0/17，证据见 `.local/parity/2026-10-17-1048/`。
| W308-11 | Windows `WindowsAgentPluginCompatibilityImporter`、自定义安装器/设置页、Skill snapshot | 已实现 staging-only 归一化 ChatOS、Portable/Codex/Claude Plugin、独立 Skill及主流 MCP JSON；真实 Portable Skill 预检和 Human 安装、文件指纹/残留通过。2026-10-17 20:42 修复 Windows 重启目录只枚举 marketplace、导致本地包消失的缺口：legacy `custom:` 与新来源记录均合并进设置/runtime catalog，checksum-covered manifest 恢复准确元数据，本地启用状态持久化且不误写远端 preference，篡改包可见但 fail-closed；本地 Plugin app 列表/启动同语义。离开设置会取消预检并清除旧指纹。定向 32/32、Connector 可执行子集 631 通过/5 条件跳过、Debug 0 警告错误、最终产物两次启动/正常退出/包保留/残留归零通过。缺真实设置页重启可见与开关/刷新视觉、Main Chat Skill 选择、Claude 独立夹具和 Git/npm | 实现中（重启发现与启用实现/自动化通过；真实 UI 恢复待验收） | Computer Use 可用时先完成已提升的新项目主链，再验证目录/启停/刷新与 Main Chat 选择；补 Claude，Git/npm 归 W308-09，P1 |
| W308-12 | Windows 自定义安装器；REA `rea-agents` 固定版本样例 | Windows 已增加精确版本 npm MCP 获取/校验/双重指纹/持久化工具链和专用安装助手 schema。真实 `rea-agents@6.5.0` 通过两次获取、`initialize/tools/list`、临时安装后重开并发现 207 个工具，`binary_session({})` 成功返回 Object；修复 npm 依赖树独立 65,536 项有界上限和 Node 深路径 package-scope 失败，普通定向 37/37、真实 REA 1/1、Debug 0 警告错误。生产 AppContainer/UI 安装、目录/Main Chat 选择、客户端重启和缺分析引擎诊断仍缺 | 实现中（canonical 包真实 Job-controlled 调用通过） | 依赖 W308-09/13 卷根前置后补生产 UI/AppContainer 与重启，P0 |
| W308-13 | `WindowsProcessJob`、Credential Manager、Workspace 安全路径、现有安装器 | staging 已有 canonical child、安装根互斥、reparse 拒绝、边界/hash 和 Kill-on-close Job。ACL 改为 `RawSecurityDescriptor` + `SetFileSecurity` 的精确、不传播、幂等更新，支持长路径且不修改 `C:\Users`；profile 创建前校验 workspace，首次 ACL 前写 cleanup journal，失败/取消完全补偿。最终设置页生产预检实际启动 child 后因本机卷根未预配 traverse ACE fail-closed，UI 可见，关闭后进程和 sandbox roots 均为 0；npm scripts、Git hooks/file、凭据矩阵仍开放 | 实现中（ACL/残留/真实错误态通过；卷根安装前置仍阻塞生产协议） | 设计最小、可审计的安装时卷根前置，P0 |
| W308-14 | 共享 Skill bundle/profile/schema；Windows `WindowsAgentCapabilityDisclosure`、`WindowsAgentCapabilityAuthorization`、`WindowsExternalToolSchemaValidator`、Plugin/外部 MCP executor | Windows 已有 macOS disclosure v2、Run-scoped describe ledger、精确 Plugin Skill gate 和冻结 schema preflight。外部 HTTP MCP 现嵌入 canonical `chatos-external-mcp` Product Skill/reference，发布 schema-v2 frozen bundle、capability revision/catalog 与 `on_demand` binding；SHA-256 identity 覆盖 resource/server/endpoint/tool/schema/远端 gate 而排除 transient header。HTTP 前验证 bundle/binding/activation，漂移、缺激活和无可信文档的远端业务 Skill 明确拒绝，JSON-RPC 错误保留结构化诊断。定向 20/20、受影响 52/52、Connector 可执行集 674 通过/5 条件跳过、Debug 0 警告错误。真实 Plugin/HTTP MCP 成功、远端 Skill 拒绝、schema 分页、取消/错误、持久化/重启仍缺 | 实现中（外部 Product Skill gate 与自动化负向/运行时路径通过） | Computer Use 恢复先做新项目聊天主链，随后真实 MCP UI/业务矩阵，P0/P1 |
| W308-15 | `WindowsLocalAgentHostLifecycle`、`LocalAgentHostProcessLauncher`、bootstrap/runtime settings、共享 control plane | Windows 现以 `--disable-workers` 启动 IPC-only bootstrap Host；credentialed worker 只保存 SHA-256 指纹，同 owner/同凭据（与字典顺序无关）刷新直接复用，模型或 memory token 变化及跨 owner 刷新 fail-closed，不猜测 worker 空闲；停止后的正常启动才应用新凭据。生命周期/进程参数 7/7、相关合同 21/21、受影响定向 58/58、Connector 可执行子集 639 通过/5 条件跳过、Debug 0 警告错误。真实可控长 Run、历史冻结凭据、变化提示和重启应用仍缺 | 实现中（不中断刷新核心合同自动化通过） | 需可控长 Run 与 UI/重启证据，P0 |
| W308-16 | Chat attachment/vault、Plugin Artifact 中心、Project Files | 主聊天附件已有真机模型消费/失败恢复/重启证据。Windows Plugin 产物现强制 `artifact.create` 与项目 scope，经路径/reparse/MIME/大小/hash 校验后复制到项目受管目录，返回 macOS 同语义的精确 Markdown/结构化交付链接；Task/Agent Team/Relay 三入口自动化通过。真实插件样例的链接点击、预览/另存、项目文件 UI、篡改/取消/重启未验收 | 实现中（持久化/权限/交付链接自动化通过） | Computer Use 恢复后先做新项目聊天主链，再用真实 document/diagram/web-design 样例验收，P1 |
| W308-17 | Localization、Package manifest、`chatos.runtime.json`、Host/Skill 资源复制 | 修复 Release 的 aws-lc NASM fallback、MSIX SDK 工作目录和 Host content root；Debug/Release 0 警告错误，无签名 MSIX 实物包含 runtime/Host/PRI/88 帧 Pet/中英文 manifest，独立复制 publish 从 `C:\Windows\Temp` 启动与退出归零通过。签名安装/升级、packaged AUMID、中英文 UI、ARM64/干净账号/符号包仍缺 | 实现中（x64 Release/MSIX 资源与无开发目录启动子链通过） | 依赖签名证书、ARM64/干净账号和 UI 通道，P1/P2 |

### 本轮最优先闭环

W308-05 的旧共享 Host 阻塞已不再成立：Windows lifecycle 仍发送协议 41，合并后的 Host 要求 42，导致真实客户端 `response identity is invalid`。现已把 Windows 协议改为 42 并更新测试。真实 Task 新 Run `task-run-900a754a-0a33-43f9-9133-300a84d28092` 成功，仅执行一次 `project_read`、未执行 `project_write`，UI 回调为 `RETRY-FRESH-CONTEXT-OK-20261017 resolved repository version`；关闭、最终构建重启和结果持久化均通过。证据见 `.local/parity/2026-10-17-0340/acceptance-summary.md`。整项 W308-05 仍保持开放。

W308-07 已进入“实现中”：Windows 现在把 Plugin 能力建模为精确 `pluginKey + componentKey`，冻结 Release 身份，拒绝缺失/不匹配/漂移，并只启动所选 MCP component；Main Chat 选择只在成功创建新 turn 后消费。Connector 27/27、Presentation 52/52、最终 x64 Debug 构建、认证 Shell/项目聊天重启、无可用插件空态和退出进程清理均通过。Solution Studio 在真实目录中仍为未安装，UI 安装需要 action-time confirmation，未触发安装；因此 W308-07/08 均不勾选。证据见 `.local/parity/2026-10-17-0455/acceptance-summary.md`。

W308-04 已完成 Windows DTO/UI 主链：冻结 v1 合同和 Run 报告严格解码，Unicode scalar 独立复算，Task 图展示逐项结论/evidence/最终原文/字符上限，并对 succeeded 且缺失/非法报告 fail-closed。Core 6/6、Connector 6/6、Presentation 113/113 和最终 x64 Debug 构建通过；最终客户端的认证 Shell、项目聊天、旧 Task 兼容、正常退出及重启恢复通过。历史 Task 不含 v1 合同，未制造新任务，因此真实 v1 succeeded/failed/not_verified、超限和重启证据仍缺，整项保持实现中。证据见 `.local/parity/2026-10-17-0530/acceptance-summary.md`。

W308-02 已补 Windows 原生协议/UI 主链：读取共享 Host 的准确方案 revision/Markdown，验证 owner/project/survey/plan/source Run identity，把 processing/waiting/review/failed/cancelled/succeeded 分开呈现，只在方案源 Run 成功后允许 Human 确认准确 revision，并逐字段拒绝确认时正文或来源漂移；确认不会创建 Task。定向 Connector 3/3、Presentation 4/4、全量 Presentation 861/861 和 x64 Debug 构建通过。computer-use 原生窗口接口本轮未挂载，无法获取真实窗口，因此成功/失败/取消/冲突/确认/重启和旧回复导入仍未真机验收，W308-02 保持实现中。证据见 `.local/parity/2026-10-17-0602/acceptance-summary.md`。

W308-02 的 legacy 兼容链已补齐：Windows 无版本方案时展示准确成功 Run 的 final Markdown，读取保持无副作用；Human 点击 `ChatOS.RequirementSurveys.AdoptPlan` 后才保存 revision 1 并确认准确版本。resolved/owner/project/source/逐字正文/64 KiB/返回快照均严格校验，失败 Run、正文不符和冲突不会冒充确认或启动 Task。Connector/桌面契约 30/30、Presentation 862/862、最终 x64 Debug 构建 0 警告/错误；最终客户端汇总空态、项目空态、正常退出和认证重启通过。当前账号没有调研记录，未造数据，因此真实 legacy 保存/确认及生成方案状态矩阵仍待样本，整项保持实现中。证据见 `.local/parity/2026-10-17-0615/acceptance-summary.md`。

W308-03 的 Windows 展示/校验主链已实现：`MessageTaskRequirementPlan` 只读取 Task 自身冻结输入，要求已确认正 revision、完整 snapshot/scope，并独立复算 UTF-8 Markdown SHA-256；scope 标题必须与持久化 Task 一致。Task 图显示固定 revision、Markdown、prompt、预期交付、验收范围和 hash，历史/重试绝不回读项目当前方案。Core 39、Presentation 862、Connector/桌面合同定向 31、Host 方案冻结 4 和最终 x64 Debug 构建通过；真实最终客户端打开旧 Task 与 21/21 事件任务图、退出、认证重启和再次退出通过。当前账号没有已确认方案和 plan-bound Task，未制造业务数据，因此真实 v1 创建、v2 修订后旧 Task/重试仍显示 v1 及重启持久化待验收，整项保持实现中。证据见 `.local/parity/2026-10-17-0625/acceptance-summary.md`。

W308-01 的 Windows 能力/恢复子链路已闭环：最终客户端显示 `gpt-5.6-sol` 的 `low/medium/high/xhigh/max`，模型、推理开关和 `medium` 等级经连续重启恢复；历史 Task 能展示冻结 model ref/revision。修复了重复 runtime-ready 应用清空 `ReasoningLevels` 导致 WinUI 丢失 `SelectedItem`，并在程序化应用期间设置 `IsApplyingSettings`，防止启动 `SelectionChanged` 写回。会话 56/56、Presentation 866/866、Connector 定向 32/32、x64 Debug 0 警告/错误；最终只读 SQLite 的 version `11` 与 timestamp `1792195564832` 在启动前后不变，正常退出后 Desktop/Host/Connector 为 0。现有真实 Task 仅证明历史冻结模型，不证明当前 `medium` thinking level 已冻结；刷新/过滤/禁用/关闭/重试完整矩阵也未闭环，故 W308-01 仍保持实现中。证据见 `.local/parity/2026-10-17-0645/acceptance-summary.md`。

W308-09/13 的生产 AppContainer ACL 停滞与补偿子链路已修复：祖先 ACL 现以 `RawSecurityDescriptor` + `SetFileSecurity` 写入精确不传播 ACE，支持长路径、保留非 canonical ACL、避开 `C:\Users`；controlled profile 在首次 ACL 前持久化 cleanup journal，创建前校验 workspace，真实 stdio pipe 使用同步 handle 语义。生产 Node 现会实际启动，并在本机缺少管理员预配卷根 traverse ACE 时约 0.5 秒以 `EPERM lstat C:\` fail-closed，profile/ACE/journal/进程均无残留。定向 7/7、回归 22 通过/5 条件跳过、最终 Debug 0 警告/错误，客户端认证 Shell/本机/插件目录回归及正常退出通过。生产 `initialize/tools/list` 未通过，安全安装时卷根前置、设置入口、Git/npm/卸载/恢复仍开放，两项均不勾选。证据见 `.local/parity/2026-10-17-0920/acceptance-summary.md`。

W308-09/13 设置入口已补齐本地目录子链路：Settings → Plugins & Skills 新增路径/目录选择、真实预检、组件/工具/指纹结果与安装前确认，来源改变立即作废预检指纹，界面明确拒绝当前未支持的 Git/npm。首次实机发现卡片泄漏进 General 可访问树，已修为仅插件分区可见并补合同回归。定向 installer/ViewModel/automation 34/34、Plugin+Desktop automation 112/112、最终 Debug x64 0 警告/错误；最终客户端对 `plugins/solution-studio` 真实预检，约 2.5 秒在 UI 显示 `EPERM lstat C:\` 卷根前置，未点击安装。正常关闭后 Desktop/Host/Connector/testhost/vstest 和两个 sandbox roots 为 0。生产协议成功、Git/npm、卸载/恢复仍开放，W308-09/13 保持实现中，扩展仍 0/17。证据见 `.local/parity/2026-10-17-0945/acceptance-summary.md`。

W308-01 的陈旧 revision 恢复子链路已补齐并真机通过：macOS/Windows 都对非当前模型 revision fail-closed，Windows 不做启动静默迁移；错误态先发布当前模型目录并显示中英文恢复边界，只有 Human 显式重选才写入。最终客户端真实从空模型/陈旧 revision 选择 `gpt-5.6-sol`，恢复 `medium`，退出重启后保持，SQLite version/timestamp 未再次变化。Presentation 57/57、Connector 27/27、Debug x64 0 警告/错误，退出进程和 sandbox roots 归零。真实当前 Task/Run thinking-level 冻结及刷新/过滤/禁用/关闭/重试全矩阵仍缺，故 W308-01 保持实现中；证据见 `.local/parity/2026-10-17-1115/acceptance-summary.md`。

## 2026-10-17 20:24 当前最高优先闭环：新项目聊天读/搜/改

- macOS `applyPreparedConversation` 会同步更新工作区项目、项目资源和当前项目会话；Windows 原先只更新 `_workspaceSnapshot`。Windows 现回写 `Projects`/当前选择的 `ConversationId`，并在替换同一项目资源时保留正在打开的聊天标签和 cancellation token。
- 新增项目内会话准备加载、失败说明与重试入口。自动化覆盖创建项目、首次聊天只创建一次、项目 context、文件/聊天重入、失败不缓存、重试成功和账号切换隔离。
- Connector/Workspace/Shell/Desktop 定向 129/129，Presentation 创建项目/文件/会话 230/230，最终 x64 Debug 0 警告/错误，最终客户端启动与正常退出通过。3 个 symlink 夹具因机器缺少 `CreateSymbolicLink` 权限单列阻塞。
- 本轮 Computer Use 工具面没有提供技能要求的 `node_repl/@oai/sky` 入口，因此真实新项目的读取、搜索、写审批、修改、回读、关闭与重启没有执行，状态严格保持“实现中”。下一轮必须从该真实主链继续，完成前不转回后续 W308 工作。证据 `.local/parity/2026-10-17-2024/`。

2026-10-17 20:25–20:42：再次确认 `node_repl/@oai/sky` 不可用，无法继续上述点击式主链，故只推进不依赖 UI 输入的 W308-11 重启发现缺口。对照 macOS 自定义插件合并/本地 preference 后，Windows 已把本地安装记录合并进设置、Main Chat/Task runtime 和 Plugin application 目录，持久化本地启用状态，并在返回设置时取消预检/清除旧指纹。定向 32/32、Job 回归连续 5 次、Connector 排除 3 个既有 symlink 机器前置后 631 通过/5 跳过、最终 Debug 0 警告错误；最终产物两次启动/正常退出且包唯一、进程和 staging/sandbox 归零。真实目录可见/启停/刷新仍因 UI 工具缺失未验收，证据 `.local/parity/2026-10-17-2025/`。下一轮 Computer Use 一旦可用仍先回到新项目聊天读/搜/改/重启主链。

2026-10-17 20:55–21:01：`node_repl/@oai/sky` 仍不可用，故没有绕过技能边界操作项目 UI，新项目聊天主链继续保持第一优先级。对照 macOS `safeErrorSummary` 修复 W308-10 泛化错误：Windows 现在只透传路径/秘密无关的 `PluginPackageException` 产品校验文案，不存在目录能返回准确可修正原因；任意原生异常继续统一隐藏，含授权、API key、access token 或 password 标记的产品错误也强制脱敏，摘要限制 1000 字符且 side-effecting 的 `needs_review` 语义不变。定向 4/4、安装助手/运行时 34/34、最终 Debug 0 警告错误；最终产物主窗口启动、正常退出 exit 0、仓库产品进程为 0。真实 UI 文案仍待工具恢复后复验，证据 `.local/parity/2026-10-17-2055/`。

2026-10-17 21:40–21:50：新项目聊天主链仍因本轮无 `node_repl/@oai/sky` 而不能执行。独立推进 W308-15 时对照 macOS Host lifecycle 找到 Windows 会在每次 bootstrap 无条件重启 credentialed Host、且临时 Host 可领取 durable work 的缺口。Windows 现为 bootstrap 进程传 `--disable-workers`，只保存长度前缀 SHA-256 凭据指纹；同 owner/同凭据刷新复用 worker，任何模型/memory token 变化或跨 owner 刷新都不终止当前 worker，停止后的正常启动才加载新凭据。生命周期/参数 7/7、合同 21/21、受影响定向 58/58、Connector 排除 3 个既有 symlink 机器前置后 639 通过/5 跳过、最终 Debug 0 警告错误，最终产物主窗口启动/正常退出且产品进程归零。真实长 Run、历史冻结凭据、延后提示和重启应用仍未验收，W308-15 保持实现中；证据 `.local/parity/2026-10-17-2140/`。

2026-10-17 21:55–22:10：Computer Use 的 `node_repl/@oai/sky` 仍未提供，故新项目聊天主链未执行且继续保持最高 UI 优先级。独立审计 W308-14 发现 Windows Plugin 与外部 HTTP MCP 均未在执行前消费冻结 `inputSchema`；现新增与 macOS 同边界的有界离线 validator，Plugin 在 Skill/权限/审批/真实 tool call 前校验，HTTP MCP 在发请求前校验，远程 ref/未知断言/非法 pattern fail-closed，确定性参数错误不再误进 side-effect `needs_review`。validator 14/14、受影响 59/59、Connector 排除 3 个既有 symlink 机器前置后 653 通过/5 跳过，最终 Debug 0 警告错误；最终窗口正常关闭 exit 0 且产品/测试进程归零。W308-14 仍缺 disclosure v2、可信外部 Skill bundle gate 和真 MCP 矩阵，保持实现中；证据 `.local/parity/2026-10-17-2155/`。

2026-10-17 22:25–22:40：互斥检查确认无上一轮产品/构建/测试进程。Computer Use 技能的 `node_repl/@oai/sky` 仍未提供，未用 PowerShell UIA 绕过；新项目聊天读/搜/写审批/回读/重启保持最高 UI 入口。独立继续 W308-14：Windows 现对齐 macOS disclosure v2，目录分页不含 schema 且不授权调用，单工具详情和超大 schema 按 8,000 Unicode scalar 安全边界显式分页并附 SHA-256；工具详情 ledger 按 Run 累积，读 B 不撤销 A。Skill 控制面改为 `agent_skill_activate/read_resource` 和精确 run-scoped `skill_ref`，只有已 describe 工具声明的 Skill 可激活。定向 55/55；Connector 原始全量 664 通过/5 条件跳过/3 个既有 symlink fixture 机器前置失败，排除这 3 条后的权威重跑为 664/664 通过、5 条件跳过；最终 Debug x64 0 警告/错误，最终主窗口出现并正常关闭 exit 0，产品进程及 sandbox roots 归零。W308-14 仍为实现中、扩展仍 0/17：外部 MCP 可信 Skill bundle 和真实 Plugin/HTTP MCP 成功/拒绝/分页/重启尚未验收。证据 `.local/parity/2026-10-17-2240/`。

## 2026-10-17 23:25–23:40 本轮入口：W308-17 x64 发布与资源定位

- Computer Use `node_repl/@oai/sky` 仍不可用，新项目聊天读/搜/写审批/回读/重启保持下一次 UI 第一优先；本轮不以进程窗口探针代替业务页面验收。
- 修复 Release aws-lc NASM fallback、MSIX 工作目录/SDK 解析以及任意当前目录导致的 Host 配置监控失败。运行时配置、Local Agent Host 和 Pet 资源继续按 `AppContext.BaseDirectory` 解析。
- 合同 17/17；Debug/Release x64 0 警告错误；无签名 MSIX 真实生成并核对 466 entries、runtime/Host/PRI/88 帧 Pet/中英文 manifest。独立复制 publish 从 `C:\Windows\Temp` 启动 `ChatOS`，正常退出 0 且残留归零。
- W308-17 严格保持实现中，扩展仍 0/17。签名安装/升级、packaged AUMID、中英文实际切换、ARM64、干净账号和符号包待补；证据 `.local/parity/2026-10-17-2325/`。

## 2026-10-17 23:46 验收基础设施修复：隐藏 Job descendant

- 用户现场报告自动化期间出现多个 Windows Terminal，内容为 `PING.EXE 127.0.0.1 -t` / `0x800700e8`。根因是 `WindowsProcessJobTerminatesDescendantProcess` 的 PowerShell `Start-Process` 在默认终端宿主下创建可见窗口；Job 正常终止 child 后，窗口显示管道关闭。
- 夹具改为 `UseShellExecute=false`、`CreateNoWindow=true` 的直接 `ProcessStartInfo` 子进程，仍保留父进程、descendant 和 Job 整树终止语义。定向 1/1，通过前后 WindowsTerminal/OpenConsole/ping 均为 0。产品代码与 W308 状态不变；证据 `.local/parity/2026-10-17-2345-terminal-popup/`。

## 2026-10-18 00:05–00:16 本轮入口：W308-14 外部 MCP Product Skill gate

- 互斥检查通过；Computer Use 仍没有 `node_repl/@oai/sky`，未以 PowerShell UIA 代替真实 UI 验收。新项目聊天读/搜/写审批/结果/关闭/重启继续是第一 UI 优先级。
- Windows 外部 HTTP MCP 直接复用 macOS canonical `chatos-external-mcp` `SKILL.md` 与 references，构造 schema-v2 frozen bundle，随精确 capability revision 发布 `task_skill_bundle_internal`、tool catalog 和 `on_demand` binding。绑定 SHA-256 覆盖 resource/server/endpoint/tool/description/schema/remote gate，并刻意排除可轮换凭据 header。
- 执行前核对 bundle revision、binding identity 和 activation；缺说明、目录漂移、未激活、runtime 不可用或只有远端 `_meta["chatos/skillGate"]` 而无可信业务 Skill 文档均明确 fail-closed 且不发送 HTTP。JSON-RPC 服务端错误保留 code/message，避免把可修正失败泛化成虚假 blocker。
- 外部 schema/Skill 定向 20/20，受影响 Host/control-plane/capability/Plugin 52/52；Connector 排除 3 个既有 symlink 机器前置后 674 通过/5 条件跳过/0 失败。最终 x64 Debug 0 警告错误，最终客户端正常关闭 exit 0，产品/测试进程和 sandbox roots 归零。
- W308-14 保持实现中，扩展仍 0/17。真实 Plugin/HTTP MCP 成功、远端 Skill 拒绝、schema 分页、取消/错误、持久化与重启矩阵仍缺；证据 `.local/parity/2026-10-18-0005/`。

## 每轮闭环与证据

1. 先读取新任务 memory、此计划和旧清单；确认旧/新任务没有其他轮仍在编辑、构建、测试或操作客户端。有重叠则本轮延期，不启动竞争写入。
2. 每个检查项记录：macOS 源码依据、Windows 实现位置、自动化结果、真实操作步骤、截图/日志/产物、重启结果、失败或精确阻塞。沿用 `clients/windows/.local/parity/` 的证据布局。
3. 跑真实 Windows 客户端和必要的 Local Agent Host、模型、插件与服务。覆盖正常、空、加载、错误、取消、权限、返回、持久化和重启；UI 必须目视核对，不用 mock、静态合同测试或单测通过代替业务跑通。
4. 发现差异直接实现/修复并复测。外部账号、供应商、SSH、签名或权限确实阻塞时保存复现与缺口，继续其他可执行模块；blocked 不能勾为通过。
5. 使用适用测试、Windows Debug 构建和真实回归；共享 Rust/插件变更增加相应测试。机器条件测试明确区分，通过数不得掩盖跳过项。通过后不要无理由重复整套测试。
6. 测试使用已有验收账号和安全配置。旧配置的本地备份为 `C:\Users\17986\.codex\automation-history\windows-before-3.0.8-refresh-20261017\previous-task-config.txt`；仅在登录需要时读取，不得向日志、源码、截图、文档或回复输出密码/令牌。不要自动创建账号或改动非验收业务数据。
7. 每轮更新清单、进度、当前总览和新任务 memory，说明新增/重新打开的项目、各模块实际状态、命令与结果、证据、剩余队列和下一步。历史 memory 保存在同目录下的 `memory.md`，必要时只读检索，不能重新从零验收所有已证实且未受影响的流程。
8. 只有旧开放项、新增项和受影响回归全部在 Windows 实际可用并具备证据，才能宣布全模块完成。最终提交逐项矩阵、剩余条件与证据索引；全部完成后通过自动化工具暂停本任务。未完成则持续推进，不把单轮结束当作总任务结束。
# 2026-10-18 00:25–00:36 本轮入口：W308-09 固定 npm 设置入口

- Computer Use `node_repl/@oai/sky` 仍不可用，新项目聊天真实主链继续保持下一次 UI 第一优先；未用 PowerShell UIA 替代。
- Windows Settings 自定义来源从“仅本地目录”扩展为本地目录或固定版本 npm stdio MCP。npm 字段以结构化 DTO 进入现有安全获取器，继续禁 lifecycle scripts、校验精确包版本/bin、执行真实 MCP 预检，并在 Human 确认后重新获取和核对指纹；无效 JSON 参数在任何获取前拒绝。
- 模式与字段变化会取消进行中的预检并使旧指纹失效；Git 仍明确拒绝，没有引入通用终端或字符串命令。定向 52/52，最终 x64 Debug 0 警告错误，最终产物启动/正常退出/进程归零，diff check 通过。
- W308-09 严格保持实现中、扩展仍 0/17。真实 Settings npm 下载/确认安装/重启发现、HTTPS Git 固定 ref、生产 AppContainer 成功以及卸载/恢复尚未完成；证据 `.local/parity/2026-10-18-0025/`。

## 2026-10-18 00:40–01:05 本轮入口：W308-09 HTTPS Git 固定 ref

- 互斥检查通过；Computer Use 技能已完整读取，但必需的 `node_repl/@oai/sky` 未暴露，因此没有使用 PowerShell UIA 绕过，新项目聊天主链与真实设置页点击仍未验收。
- Windows 自定义安装器新增无凭据 HTTPS Git 获取：URL 禁 userinfo/query/fragment，ref 拒绝选项注入、reflog/范围语法，子目录做安全相对路径和 canonical/reparse 校验。Git 使用 `ProcessStartInfo.ArgumentList`、无 shell/无交互、禁 hooks/file protocol/子模块，Job Object 控制超时/取消；`.git` 删除支持只读对象且不跟随 reparse。
- 设置页新增 Git URL/ref/子目录模式，与 npm 互斥并在字段变化时作废预检；独立安装助手 schema/executor 同步接受 `git_repository`，但普通聊天/Task 仍无安装工具。预检与确认安装都重新获取来源并比较来源+Skill/真实 tools-list 指纹，漂移 fail-closed。
- 定向 47/47；Connector 排除 3 个已登记 symlink 机器夹具后 691 通过、5 条件跳过；最终 Debug/Rust Host 0 警告错误，最终窗口标题 `ChatOS`、正常退出 0，产品/测试/Git 进程、sandbox 与测试临时目录均归零。真实公开仓库抓取曾成功；保存权威证据时网络转为 `github.com:443` 不可达，失败 TRX/诊断保留，未把 Git 安装/重启算通过。
- W308-09 保持实现中、扩展仍 0/17。仍缺网络恢复后的真实 Git Plugin 预检→Human 确认→二次获取→安装→目录发现/重启，三来源设置页真实 UI，生产 AppContainer 成功和卸载恢复。证据 `.local/parity/2026-10-18-0100/`。

## 2026-10-18 06:19–06:23 P0 空目录项目聊天状态

- P0 仍是唯一入口，扩展项继续冻结。权威目录 `C:\ChatOS-ProjectChat-Acceptance-20261018-01` 复核为 0 项、非 reparse point；它尚未被最终 x64 Debug 客户端选择，也没有项目/会话 identity 或截图/UI tree。
- Computer Use 要求的 `node_repl/@oai/sky` 仍未暴露，因此未用 PowerShell UIA、数据库/API 或已有项目替代真实创建。真实空目录项目尚未创建/操作。
- 同链路审计补上受项目 context、`CodeMaintainerWrite`、Host 审批与 `WorkspacePathGuard` 保护的 `project_move`，覆盖移动/重命名且明确显示源/目标路径；capability revision 更新为 `native-windows-main-chat-v10-project-move`。权威受影响自动化 24/24，通过状态仅为“实现/自动化通过”。
- 最终 x64 Debug 构建 0 警告/错误，diff check 通过；构建服务器关闭后 Desktop/Host/Connector/测试/构建进程均为 0，两个 sandbox root 均为 0 项。它们只是辅助验证。
- 仍缺真实创建→首次聊天→空列表→允许/拒绝/取消写入→读/搜/修改/diff→移动/重命名→文件页返回→同会话继续→关闭重启，以及完整边界/错误/重试无重复副作用矩阵；P0 不得写为通过。证据 `.local/parity/2026-10-18-project-chat-real/acceptance-summary.md`。

## 2026-10-18 06:33–06:39 P0 复制与 Host 工具过滤修复

- 真实空目录项目仍未创建/操作：Computer Use 的 `node_repl/@oai/sky` 继续未暴露，权威目录复核仍为 0 项且非 reparse point；未用其他 UIA、数据库/API 或旧项目替代。
- 审计发现共享 Host 的 Task 工具过滤只允许 `project_write`，会把上一轮已发布的 `project_move` 从真实模型工具面移除。现已把 `project_copy` 与 `project_move` 同时纳入 `CodeMaintainerWrite + requires_execution`，并新增项目内原子复制、覆盖恢复、取消清理、reparse/ADS/绝对路径/盘符/UNC/越界保护和双路径审批摘要；capability revision 更新为 `native-windows-main-chat-v11-project-copy-move`。
- Connector/文件系统/Host 参数权威子集排除两个明确缺 `CreateSymbolicLink` 特权的机器夹具后 36/36；共享 Host 工具过滤 1/1；最终 x64 Debug 构建 0 警告/错误。P0 仍缺全部真实 UI/模型/审批/磁盘/diff/重启证据，扩展项继续冻结。

## 2026-10-18 06:45–06:50 P0 新一轮真实 UI 前置复核

- 同任务其他轮次均 idle/notLoaded，产品/Host/Connector/测试/构建/Rust 进程为 0；保留全部工作区改动。
- 权威目录 `C:\ChatOS-ProjectChat-Acceptance-20261018-01` 仍为 0 项、普通目录、非 reparse point，最终 x64 Debug 产物存在。
- 工具目录仍没有 Computer Use 必需的 `node_repl/@oai/sky`，因此未启动或操作客户端，也未用其他 UIA、数据库/API、fixture、mock 或旧项目替代。只读 P0 执行链审计未在真实复现前确认新的更早代码缺陷，本轮不做猜测性修复或重复辅助测试。
- P0 仍未通过：项目/工作区/会话 identity、UI 证据、空列表、审批三分支、真实写入/读搜/修改/diff、复制移动重命名、页面往返、重启恢复和错误矩阵全部仍缺。P1/P2/W308 扩展继续冻结。

## 2026-10-18 07:08 P0 创建项目动作自动化准备

- 互斥通过；Computer Use `node_repl/@oai/sky` 仍未暴露，因此没有启动/控制客户端或创建真实项目。权威目录继续保持 0 项、普通 Directory、非 reparse。
- 创建项目对话框的运行时 `PrimaryButton` / `CloseButton` 补齐 `ChatOS.CreateProject.Submit` / `ChatOS.CreateProject.Cancel` 稳定 AutomationId 和中英文 accessible name；模板部件查找 fail-safe。合同 2/2、Desktop x64 Debug 与最终 x64 Debug 构建均 0 警告/错误。
- 这是 P0 UI 可控性实现/自动化通过，不是门禁通过。真实 project/workspace/conversation identity、UI 证据、聊天工具/审批/磁盘/diff/重启/异常矩阵全部仍待验收，P1/P2/W308 保持冻结。
## 2026-10-18 07:21 P0 gate update

- P0 approval automation readiness improved: evidence fields now have stable IDs and a newly active approval receives safe keyboard focus on Deny. This is implementation/automation-only evidence.
- The real gate remains closed: the final Debug Create Project UI has not selected `C:\ChatOS-ProjectChat-Acceptance-20261018-01`, and no real project chat operation has run. Computer Use remained unavailable; P1/P2 and every extension module remain prohibited.

## 2026-10-18 07:24–07:38 P0 directory verification repair

- Real UI remains blocked because Computer Use `node_repl/@oai/sky` is unavailable; the authoritative empty directory has not been selected and no project was created.
- Fixed a fail-open in the exact P0 entry path: a failed refresh of an already selected directory could leave a stale path eligible for Create/Save. The current listing must now be verified successfully; failure stays disabled until a successful retry.
- Create Project exposes the exact absolute path and actual entry/folder/file counts with stable AutomationIds. Focused tests are 34/34 and 29/29; final x64 Debug is 0 warnings/errors. These results are automation preparation only.
- P0 still lacks every real project/conversation, model tool, approval, disk/diff, navigation, restart and error-matrix artifact. P1/P2/W308 remain frozen.

## 2026-10-18 07:40–07:53 P0 authoritative conversation binding

- Real UI remains blocked: no Computer Use `node_repl/@oai/sky`, so the authoritative empty directory is still unused and no project chat has run.
- Closed an implementation gap in the required restart/continue path. A remote workspace snapshot's `LatestConversationId` no longer bypasses Host validation; the first project-chat entry in each authenticated lifecycle resolves the exact `owner + project resource` binding through Local Agent Host. Same-lifecycle re-entry reuses only that verified result; invalid Host identities fail closed, and logout/account change clears verification.
- Added `ChatOS.Workspace.Root` with UIA project/conversation identity properties for real evidence. Shell 22/22, combined binding/automation 52/52, final x64 Debug 0 warnings/errors. This is implementation/automation only; evidence `.local/parity/2026-10-18-project-chat-real/10-project-conversation-binding-verification.md`.
- P0 remains blocked/not accepted until the final client creates the project from the empty directory and completes the full real chat/tool/approval/disk/diff/navigation/restart/error matrix. P1/P2/W308 remain frozen.

## 2026-10-18 08:10 P0 workspace identity/navigation evidence preparation

- Computer Use remains unavailable, so the final client did not select the authoritative empty directory and no real project chat ran.
- The UIA workspace root now exposes workspace path, project identity and conversation identity separately. Navigation selection publishes stable `selected` / `not-selected` state, allowing the later real tree to prove files↔chat continuity and restart restoration.
- Desktop automation 31/31 and final x64 Debug 0 warnings/errors passed; this is implementation/automation only. Evidence: `.local/parity/2026-10-18-project-chat-real/11-project-workspace-uia-observability.md`.
- P0 remains blocked/not accepted with the full real Local Agent Host, approval, disk/diff, navigation, restart and failure matrix outstanding. P1/P2/W308 remain frozen.

## 2026-10-18 10:25 P0 project workspace failure contract

- Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the final client did not select the authoritative 0-entry/non-reparse directory and no real project chat ran.
- Workspace validation now emits stable path-free codes. Project list/read/search/write/copy/move return deterministic pre-commit validation failures as correctable `failed` results with code/status, while unknown side-effecting I/O failures remain redacted `needs_review`. This closes the implementation gap for path escape, absolute/drive/UNC/ADS, missing-entry, conflict, overlap and reparse rejection reporting without claiming UI acceptance.
- Focused regressions passed 43/43; authoritative Connector regression passed 715 with 5 conditional skips after excluding only three named symbol-link privilege fixtures; final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/20-project-workspace-error-results.md`.
- P0 is still not accepted. The complete real create/identity/Host-tool/approval/disk/diff/navigation/restart/failure matrix remains first priority; P1/P2/W308 stay frozen.

## 2026-10-18 10:07 P0 project-conversation response identity guard

- Computer Use remains unavailable, so the final client did not select the authoritative empty directory and no real project chat ran.
- Windows now validates owner-scoped conversation pages and exact create-response identity/resource binding, and rejects repeated pagination cursors instead of hanging project-chat preparation.
- Workspace/Shell 32/32 and final x64 Debug 0 warnings/errors passed; this is implementation/automation only. Evidence: `.local/parity/2026-10-18-project-chat-real/19-project-conversation-response-identity.md`.
- P0 remains blocked/not accepted; the complete real Local Agent Host, approval, disk/diff, navigation, restart and failure matrix is outstanding. P1/P2/W308 remain frozen.

## 2026-10-18 09:51 P0 project-conversation ambiguity guard

- Computer Use remains unavailable, so the final client did not select the authoritative empty directory and no real project chat ran.
- Project conversation resolution now accepts exactly one owner/project Host binding. It creates only for zero matches and fails closed on duplicate bindings both before creation and after a create conflict, preventing nondeterministic wrong-session recovery.
- Workspace/Shell 26/26 and final x64 Debug 0 warnings/errors passed; this is implementation/automation only. Evidence: `.local/parity/2026-10-18-project-chat-real/18-project-conversation-ambiguity-guard.md`.
- P0 remains blocked/not accepted; the complete real Local Agent Host, approval, disk/diff, navigation, restart and failure matrix is outstanding. P1/P2/W308 remain frozen.

## 2026-10-18 09:37 P0 cancelled-approval preparation

- Windows local approval now verifies its exact owner-scoped Host Run throughout the pending decision. If the Run leaves `waiting_tool_result`, is stopped/cancelled, or its identity cannot be verified, the pending approval is removed fail-closed and no stale Host decision or project-file side effect is possible.
- Approval/Host/project-chat focused regressions passed 33/33 and the final x64 Debug build passed with 0 warnings/errors. This is implementation/automation evidence only; see `.local/parity/2026-10-18-project-chat-real/17-project-chat-cancelled-approval.md`.
- Computer Use remained unavailable, so no final-client creation or real approval cancellation ran. P0 remains blocked/not accepted; the next run must again start from the authoritative empty directory. P1/P2/W308 remain frozen.

## 2026-10-18 09:25 P0 idempotent project-chat retry

- The Windows project-chat retry path now keeps the same turn identity for an exact resend after an ambiguous Host failure, freezes the full Connector start command, and uses a stable turn-scoped IPC command identity so Host receipts prevent duplicate starts. Retry input drift creates a new turn or fails closed.
- Connector/Host 16/16, project-chat Presentation 59/59 and final x64 Debug 0 warnings/errors passed. Evidence: `.local/parity/2026-10-18-project-chat-real/16-project-chat-idempotent-retry.md`.
- Computer Use remained unavailable, so no final-client creation or real timeout/retry ran. P0 remains blocked/not accepted; the full create/chat/tool/approval/disk/diff/navigation/restart/error matrix and P1/P2/W308 remain outstanding/frozen.

## 2026-10-18 09:07 P0 empty-directory UIA preparation

- The project-files page now exposes `ChatOS.ProjectFiles.DirectoryEmpty` only for a completed, successful zero-entry listing and includes the exact current path. Loading, error, search and closed states are deliberately distinct.
- Project-files Presentation 141/141, Desktop automation 34/34 and the final x64 Debug build passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/15-project-files-empty-state-uia.md`.
- Computer Use remained unavailable, so no final-client creation or real project conversation ran. P0 remains blocked/not accepted; the complete Host/tool/approval/disk/diff/navigation/restart/error matrix and P1/P2/W308 remain outstanding/frozen.

## 2026-10-18 08:51 P0 project Git/diff evidence preparation

- Computer Use remains unavailable, so the final client did not select the authoritative empty directory and no real project chat ran.
- Project Git now exposes stable project/workspace/repository identity, branch/sync state, init/refresh/cancel, change list/empty state, selected diff path/content/empty state, action result and error. Status/error accessible names follow Chinese/English localization, allowing the later real tree to prove the modified file's actual diff.
- Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed; this is implementation/automation only. Evidence: `.local/parity/2026-10-18-project-chat-real/14-project-git-diff-uia-observability.md`.
- P0 remains blocked/not accepted with the full real Local Agent Host, approval, disk/diff, navigation, restart and failure matrix outstanding. P1/P2/W308 remain frozen.

## 2026-10-18 08:37 P0 project-chat identity/state/message UIA evidence

- `ChatOS.Chat.Root` now publishes exact conversation identity, deterministic session state and turn count. Durable IDs cover turns, user/guidance/assistant messages, live/history process events and actions; callback replies publish exact message/task/run/status, with static empty/error/attachment-error/composer regions.
- Presentation 63/63, Desktop automation 33/33 and final x64 Debug build passed with 0 warnings/errors. This is implementation/automation preparation only.
- Computer Use remained unavailable, so the exact 0-entry non-reparse directory is still unused and no real project/chat/Host/tool/approval/disk/diff/navigation/restart/error evidence exists. P0 remains blocked/real acceptance not passed; all P1/P2/W308 extension work remains frozen.

## 2026-10-18 08:22 P0 project-files evidence preparation

- Computer Use remains unavailable, so the final client did not select the authoritative empty directory and no real project chat ran.
- Project files now expose project/root/current directory and stable list/search/loading, selected-file path/content, editor, outcome and error UIA properties. This supports exact files↔chat and disk-content proof after the real chat writes a file.
- Desktop automation 32/32 and final x64 Debug 0 warnings/errors passed; this is implementation/automation only. Evidence: `.local/parity/2026-10-18-project-chat-real/12-project-files-uia-observability.md`.
- P0 remains blocked/not accepted with the full real Local Agent Host, approval, disk/diff, navigation, restart and failure matrix outstanding. P1/P2/W308 remain frozen.

## 2026-10-18 10:35 P0 Computer Use retry audit

- Fresh mutual-exclusion, directory, product and process checks passed: only this run was active, the authoritative directory remained 0-entry/non-reparse, the final Debug executable existed, and related process count was zero.
- The bundled Computer Use skill was available to read, but its required callable `node_repl/@oai/sky` interface was absent. Per that skill, no alternate UIA or business-data injection was used; no real project was created or operated.
- Current P0 implementation/evidence was audited without finding a new reproducible pre-UI defect. No speculative edit or redundant build/test was run. Strict state remains blocked/real acceptance not passed; evidence: `.local/parity/2026-10-18-project-chat-real/21-computer-use-retry-audit.md`.
- Next run still begins at the final Create Project UI with `C:\ChatOS-ProjectChat-Acceptance-20261018-01`; P1/P2 and every extension lane remain frozen.

## 2026-10-18 10:50 P0 timeout/cancellation audit

- Fresh mutual-exclusion and directory checks passed, but Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the exact 0-entry/non-reparse directory remains unused.
- The P0 timeout slice was inspected end to end: exact-Run approval cancellation, claim renewal/no-commit on loss, cancellable copy staging, pre-commit write/move validation, and unknown-side-effect `needs_review`. No new reproducible defect was found, so no speculative edit/build/test was made.
- This does not advance acceptance. Real create/identity/Host tool/approval/disk/diff/navigation/restart/timeout-retry evidence is still absent. Evidence: `.local/parity/2026-10-18-project-chat-real/22-project-tool-timeout-audit.md`; P1/P2/W308 remain frozen.

## 2026-10-18 11:05 P0 Computer Use gate recheck

- Full Computer Use initialization/API/confirmation instructions were reread, but this run's callable catalog still had no `node_repl/@oai/sky`; no target-window selection or final-client action was possible, and no alternate UIA/data injection was substituted.
- The authoritative directory remains exactly 0 entries, ordinary and non-reparse; final x64 Debug exists and no competing run, Git lock, relevant process or sandbox residue exists.
- This is diagnostic evidence only. The real empty-directory project is uncreated/unoperated and every required identity, Host tool, approval, disk/diff, navigation, restart and failure/retry artifact is still missing. Evidence: `.local/parity/2026-10-18-project-chat-real/23-computer-use-gate-recheck.md`; P1/P2/W308 remain frozen.

## 2026-10-18 11:23 P0 project-search completeness

- Fixed the P0 search truthfulness gap: enumeration/read failures and access denial now produce `incomplete=true` plus `truncated=true`; the Local Agent v12 tool contract says these results cannot prove absence. Locked-file and failed-enumeration regressions are included.
- Authoritative focused subset 39/39 and final x64 Debug 0 warnings/errors passed; the only broad-run failures were two named CreateSymbolicLink machine-prerequisite fixtures before product code.
- Acceptance does not advance: Computer Use is still unavailable, the authoritative directory remains unused/0-entry/non-reparse, and all real identity/Host/approval/disk/diff/navigation/restart/failure evidence is missing. Evidence: `.local/parity/2026-10-18-project-chat-real/24-project-search-completeness.md`; P1/P2/W308 remain frozen.

## 2026-10-18 11:36 P0 project-list reparse completeness

- Computer Use still has no callable `node_repl/@oai/sky`, so the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed one implementation truthfulness gap in the same P0 chain: a safe listing that omits junction/symbolic-link entries now emits `incomplete=true` and `omitted_reparse_points`; `project_list` explicitly forbids treating that output as exhaustive or as proof of emptiness. Capability revision is `native-windows-main-chat-v13-project-list-completeness`.
- Focused authoritative tests passed 39/39 and final x64 Debug passed with 0 warnings/errors. The real reparse fixture cannot be created on this host because symbolic-link privilege is unavailable. Evidence: `.local/parity/2026-10-18-project-chat-real/25-project-list-reparse-completeness.md`; P1/P2/W308 remain frozen.

## 2026-10-18 11:52 P0 project-read invalid UTF-8 truthfulness

- Computer Use still has no callable `node_repl/@oai/sky`; the exact empty directory remains unused/0-entry/non-reparse, so P0 real acceptance does not advance.
- Closed one read/search truthfulness gap: invalid UTF-8 is now returned as exact base64 binary instead of replacement-decoded text, and content search cannot match invented `�` characters.
- Authoritative filesystem subset passed 19/19 and final x64 Debug passed with 0 warnings/errors; two broader failures remain only the named symbolic-link fixture privilege prerequisites. Evidence: `.local/parity/2026-10-18-project-chat-real/26-project-read-invalid-utf8.md`; P1/P2/W308 remain frozen.

## 2026-10-18 12:08 P0 project-write truthful diff

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed the non-Git empty-project diff gap in the same P0 chain: `project_write` now returns bounded unified diff evidence, before/after hashes and explicit changed/available/truncated/unavailable state. Creation compares with `/dev/null`; exact no-op writes expose no change; capability revision is `native-windows-main-chat-v14-project-write-diff`.
- Authoritative focused tests passed 43/43 and final x64 Debug passed with 0 warnings/errors. The two broad-run failures remain only the named symbolic-link privilege fixtures before product execution. Evidence: `.local/parity/2026-10-18-project-chat-real/27-project-write-diff-result.md`; P1/P2/W308 remain frozen.

## 2026-10-18 12:24 P0 project-search reparse completeness

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed a same-chain search truthfulness gap: withheld junction/symbolic-link entries now make both name and content search incomplete/truncated, so a zero-match result cannot prove absence beyond the authorized traversal boundary. Capability revision is `native-windows-main-chat-v15-project-search-reparse-completeness`.
- Non-machine-dependent regressions passed 43/43 and final x64 Debug passed with 0 warnings/errors. The direct symlink fixture remains blocked before product execution by the known Windows account privilege prerequisite. Evidence: `.local/parity/2026-10-18-project-chat-real/28-project-search-reparse-completeness.md`; P1/P2/W308 remain frozen.

## 2026-10-18 12:37 P0 project_move case-only rename

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed a same-chain Windows rename gap: case-only file/directory moves use a rollback-safe same-directory intermediate name instead of returning a false no-op. Exact same-path retries remain idempotent. Capability revision is `native-windows-main-chat-v16-project-move-case-rename`.
- Filesystem/control-plane regressions passed 44/44 and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/29-project-move-case-only-rename.md`; P1/P2/W308 remain frozen.

## 2026-10-18 P0 exact project-mutation approval identity

- Fixed the approval-accuracy gap in the same P0 chain: `project_write` approval identity now includes create-only mode, exact UTF-8 byte count and content SHA-256. `project_copy`/`project_move` display `replace_existing`; replacement-enabled requests are high risk and explicitly warn about target replacement.
- Approval/Control Plane regressions passed 41/41 and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/30-project-mutation-approval-identity.md`.
- Computer Use still has no callable `node_repl/@oai/sky`; the final client did not select the authoritative 0-entry/non-reparse directory. P0 remains blocked/not accepted, the full real approval/tool/disk/diff/navigation/restart/error matrix is outstanding, and P1/P2/W308 remain frozen.

## 2026-10-18 13:09 P0 project_write empty-content contract

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed a same-chain contract gap: the schema requires `project_write.content` but permits empty text, while approval/execution incorrectly treated empty or whitespace-only strings as a missing field. Both paths now preserve the exact content and approval fingerprint; missing/non-string input still fails closed. Capability revision is `native-windows-main-chat-v17-project-write-empty-content`.
- Approval/Control Plane/non-machine-dependent filesystem regressions passed 66/66 and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/31-project-write-empty-content.md`; P1/P2/W308 remain frozen.

## 2026-10-18 13:25 P0 Create Project listing identity

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed a same-chain directory identity gap: Create Project now rejects a returned listing unless its absolute path is inside the selected workspace, equals the requested path and derives the same project relative root. A stale/mismatched response cannot make the UI verify one directory while registering another.
- Create Project regressions passed 36/36 and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/32-create-project-listing-identity.md`; P1/P2/W308 remain frozen.

## 2026-10-18 13:38 P0 Create Project folder-item automation identity

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed a same-chain UI controllability gap: each actionable Create Project folder row now exposes the exact directory name and path to UI Automation under the stable `ChatOS.CreateProject.Folders` list anchor, so the selected empty-directory row can be identified rather than inferred.
- Focused contract 1/1, Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed. Evidence: `.local/parity/2026-10-18-project-chat-real/33-create-project-folder-item-automation.md`; P1/P2/W308 remain frozen.

## 2026-10-18 13:53 P0 Create Project submit-time directory revalidation

- Computer Use still has no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Closed a same-chain freshness gap: submit now force-refreshes and revalidates the exact workspace, requested absolute path and derived relative root immediately before registration. Refresh failure, identity drift or disposal cannot reach the create callback; latest entry counts replace stale UI proof.
- Focused Create Project tests passed 40/40, full Presentation passed 878/878 and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/34-create-project-submit-revalidation.md`; P1/P2/W308 remain frozen.

## 2026-10-18 14:08 P0 Create Project failure/empty-state separation

- Computer Use still exposes no callable `node_repl/@oai/sky`; the final client was not controlled and the authoritative 0-entry/non-reparse directory remains unused. P0 real acceptance does not advance.
- Fixed a same-chain truthfulness gap: `ChatOS.CreateProject.Empty` is now visible only after a successful usable listing. Loading, no-permission, Host and other listing failures remain explicitly unverified/error states and cannot masquerade as an empty directory.
- Desktop automation passed 34/34, focused Create Project Presentation passed 40/40, and final x64 Debug passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/35-create-project-failure-empty-state.md`; P1/P2/W308 remain frozen.

## 2026-10-18 14:19 P0 status clarification

- Current capability remains implementation/automation preparation only; the mandatory final-client empty-directory project run is still 0 accepted.
- Computer Use still exposes no callable `node_repl/@oai/sky`; no substitute UIA/database path was used. Real project/workspace/conversation identities and the complete Host tool, approval, disk/diff, navigation, restart and failure matrix remain absent. P1/P2/W308 remain frozen.

## 2026-10-18 14:38 P0 Computer Use native pipe gate

- Callable `node_repl` is now exposed and `@oai/sky` imports successfully, proving the repaired runtime registration/path. All three allowed `list_apps()` attempts nevertheless fail with native-pipe `os error 2`, including the prescribed delayed retry and full JS-kernel reset/reinitialize retry.
- The current runtime and signed helper files exist, while no helper process or `codex-computer-use-*` pipe exists. The Computer Use skill prohibits manual helper startup and alternate UIA, so the final ChatOS client was not controlled and no project/data shortcut was used.
- The authoritative directory remains unused, 0-entry, ordinary and non-reparse. Real creation/identity/Host-tool/approval/disk/UI-diff/navigation/restart/failure evidence remains absent; P0 is not accepted and P1/P2/W308 remain frozen. Evidence: `.local/parity/2026-10-18-project-chat-real/36-computer-use-native-pipe-unavailable.md`.

## 2026-10-18 19:51 P0 real UI repair checkpoint

- The final x64 Debug acceptance project is now operable through Computer Use. Project Files no longer renders overlapping flat/tree controls; the real directory page shows one actionable tree row per disk file.
- The real approval card now uses readable full-width vertical controls and moves away from the chat composer. The user-authorized allow-once branch completed one exact `project_write`, and the chat callback, disk length/content and SHA-256 agree.
- This is a partial P0 checkpoint only. Do not enter P1/P2/W308: cancellation cascade still has a real failure, and copy/move/rename plus the full boundary, failure, timeout, retry/idempotency, persistence/restart and cleanup matrix must still pass. Evidence: `.local/parity/2026-10-18-project-chat-real/37-project-files-approval-ui-real.md`.

## 2026-10-18 20:49 P0 real Stop checkpoint

- Re-ran Stop in the same final-client acceptance project at two critical boundaries. Before Task creation, cancellation left no tool, Task graph, Task, approval or target file. After a child Task/Run was created, Stop cancelled parent Run, Task and child Run together, cleared the Task's active Run and prevented all child tools/file effects.
- Sanitized UI screenshots, screenshot hashes, database timelines/counts, disk absence and clean-exit proof are in `.local/parity/2026-10-18-project-chat-real/38-project-chat-stop-real.md`. Supporting storage 108/108, Desktop automation 35/35 and final Debug 0 warnings/errors remain green.
- This closes the real Stop/cancellation subpath, not P0. Stay on the same project-chat chain: next cover copy/move/rename, full path/permission/reparse/missing/conflict and timeout/retry/idempotency matrices, then persistence/restart and final complete cleanup. P1/P2/W308 remain frozen.

## 2026-10-18 21:10 P0 real copy/move/rename checkpoint

- Continued in the same final-client acceptance project/conversation and completed one real non-overwriting copy followed by one real same-directory move/rename. Each operation exposed the exact `replace-existing=false` approval, received one allow-once decision and persisted a succeeded invocation/Task Run.
- The disk end state is authoritative: source retained, intermediate copy absent, renamed file present with exact 40-byte marker and matching SHA-256. The repaired attachment/Stop/Send/composer layout remained targetable beside the Pet window. Evidence: `.local/parity/2026-10-18-project-chat-real/39-project-chat-copy-move-send-ui-real.md`.
- Desktop automation 35/35 and final x64 Debug 0 warnings/errors remain green; normal close left no relevant process or sandbox residue. P0 is still not complete: continue with approval deny/cancel, permission/path/drive/UNC/ADS/reparse/missing/conflict, timeout/failure retry and idempotency, remaining persistence/restart, affected regressions and final cleanup. No delete was requested; P1/P2/W308 remain frozen.

## 2026-10-18 21:30 P0 real read-only path-boundary checkpoint

- Continued in the same final-client acceptance project/conversation. The model performed six distinct actual `project_read` calls: traversal, absolute path, alternate drive, UNC and ADS failed closed as HTTP 400 `workspace_path_unsafe`; the missing in-project entry returned HTTP 404 `workspace_entry_missing`.
- Persisted metadata proves all six were non-side-effecting and approval-free, while the Task, Task Run, Main Run and Turn succeeded. Sanitized UI evidence, exact identities and cleanup proof are in `.local/parity/2026-10-18-project-chat-real/40-project-chat-boundary-read-real.md`.
- This closes only those read-boundary cases. Continue P0 with no-permission, reparse, conflict, strict deny-versus-cancel, timeout/failure retry without duplicate effects, remaining persistence/restart, affected regressions and final cleanup. P1/P2/W308 remain frozen.

## 2026-10-18 22:17 P0 real conflict and approval-cancel checkpoint

- The same final-client project/conversation really exercised non-overwrite conflict. Exactly one approved `project_copy(... replace_existing=false)` returned HTTP 409 `workspace_copy_target_exists`; source and pre-existing target remained byte-identical at 40 bytes with SHA-256 `86494C3641D31A25F6F662B628E8E928A7F8D239A938D655585578BECE3A2DD0`.
- A separate `project_write` then remained at the exact local approval gate. The task-graph `ChatOS.Chat.TaskGraph.CancelTask` action cancelled the Task/Run without clicking reject, allow-once, allow-session or ordinary Stop. The write invocation failed with `owning Run was cancelled`, no approval decision was recorded, the approval UI cleared and the target file remained absent.
- Evidence: `.local/parity/2026-10-18-project-chat-real/41-project-chat-conflict-real.md` and `43-project-chat-approval-cancel-real.md`. Normal close left no relevant process or sandbox residue. P0 remains incomplete: strict approval decline, no-permission, reparse, timeout/failure retry without duplicate effects, remaining persistence/restart, affected regressions and final cleanup remain open. P1/P2/W308 stay frozen.

## 2026-10-18 22:40 P0 strict rejection checkpoint

- Real strict rejection and its restart persistence are accepted for one exact final-client `project_write`: the approval bound create-only/byte-count/SHA-256 intent, `ChatOS.Approval.Deny` was clicked once, persistence records one `failed/rejected` write, and no retry or alternate mutation occurred.
- The target remains absent. Normal close/restart restored the same project/conversation and rejected callback; final close left zero relevant processes and no sandbox roots. Evidence: `.local/parity/2026-10-18-project-chat-real/44-project-chat-approval-deny-real.md`.
- This is a P0 subpath only. Keep P1/P2/W308 frozen until no-permission, reparse, timeout/failure retry without duplicate side effects, remaining persistence/regression coverage and final full-matrix cleanup all pass. No new build/test result is claimed.

## 2026-10-19 00:28 P0 timeout/retry execution result

- The real final-client Host-timeout gate now has evidence for no side effect before retry, exact request retention, a single approved retry write, read-back, disk hash/diff truth, callback correctness and restart persistence in the same project/conversation.
- A shared Host display bug was fixed without changing task semantics: succeeded terminal details keep secret sanitization but do not interpret negated timeout/failure wording as a live runtime error. Storage 109/109 and final Windows x64 Debug 0 warnings/errors passed.
- Evidence: `.local/parity/2026-10-18-project-chat-real/46-project-chat-timeout-retry-real.md`.
- This does not advance to P1. Before P0 completion, restore/verify the macOS Main Chat/Product Skill/`skill_bundle_internal`/`task_skill_bundle_internal`/Task-invariant chain, close the remaining Host-not-ready/loading/permission/reparse cases, rerun affected regressions and complete final residue cleanup.

