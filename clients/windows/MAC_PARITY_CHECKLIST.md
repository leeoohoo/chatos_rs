# Windows 与 macOS 功能对照清单

## 当前 P0：空目录创建项目并真实操作项目聊天（未通过）

- [ ] 在最终 x64 Debug UI 中选择 `C:\ChatOS-ProjectChat-Acceptance-20261018-01` 并创建项目；该目录 2026-10-18 06:05 +08:00 仍为 0 项、非 reparse point，尚未被 ChatOS 选取。
- [ ] 绑定并持久恢复同一 project/workspace/conversation；覆盖首次聊天、文件页返回、已有项目续聊、关闭重启且不退化为全局聊天或重复会话。
- [ ] 真实 Local Agent Host 完成空目录列举、写审批允许/拒绝/取消、读/搜、修改/回读/diff、复制/移动/重命名及异常、取消、超时、重试无重复副作用矩阵。
- [x] 创建项目弹窗 P0 状态具备稳定 AutomationId：当前路径、无工作区、加载、空目录、错误、说明、项目类型及类型说明；静态桌面自动化合同 27/27 通过。此项仅为 UI 自动化准备，不表示真实项目已创建。
- [x] 审批卡 P0 证据字段具备稳定 AutomationId；新审批把键盘焦点安全地移到“拒绝”，同一审批刷新不会反复抢焦点。定向合同 2/2 通过；此项仍只是自动化准备。
- [x] 创建项目目录验证 fail-closed：当前目录刷新失败后禁止使用残留路径创建，重试成功才恢复；UI 公开绝对路径和目录项/文件夹/文件精确计数，稳定 ID 为 `ChatOS.CreateProject.AbsolutePath` / `ChatOS.CreateProject.EntrySummary`。Create Project 34/34、桌面合同 29/29、最终 x64 Debug 0 警告/错误；仅算实现/自动化。

P0 完成前暂停 Plugin、Skill、MCP、安装器、Solution Studio、REA、Media、Pet、远程和打包验收；历史项目创建或文件操作证据不能替代这条新门禁。

2026-10-17 起按 [macOS 3.0.8 新逻辑与 Windows 全模块对齐任务](docs/24-macos-3.0.8-parity-plan.md) 执行。主要修改 `clients/windows`；必要的共享 Rust Host、crate 与插件契约修复纳入本次范围，macOS 代码作为行为和界面参考。旧完成数属于历史基线，新增逻辑及受影响旧流程需要重新验收。

每项完成标准：阅读 macOS 对应实现 → 对照 Windows 行为/字段/错误处理 → 修改 Windows → Debug 构建 → 使用测试账号实际操作 → 记录结果后再进入下一项。

每 15 分钟持续开发的轮次、基线、测试证据及未验收事项见 [持续开发记录](docs/11-macos-parity-progress.md)。上一轮未完成时跳过本次触发，不并行启动。

当前严格总览、分域计数和执行优先级见 [Windows / macOS 对齐当前总览](docs/18-windows-parity-current-status.md)。截至 2026-10-16 18:13，111 个矩阵行中 51 个已闭环、60 个仍开放；开放项即使已有代码或自动化，只要缺真实 UI、异常边界或重启证据就继续保持未勾选。

2026-10-17 已在 `docs/02-capability-parity-matrix.md` 新增 W308-01..17 扩展矩阵：17 项均保持开放。历史 51/111 不作为当前 3.0.8 通过数，受影响旧项在对应 W308 完整闭环前必须重新回归。

## 1. 登录、会话与生命周期

- [x] 登录页布局、输入校验、登录失败提示（账号或密码为空时按钮禁用并显示本地化提示；登录失败仍保留服务端错误）
- [x] 登录成功后的会话恢复、token 刷新和失效处理（ChatOS 会话 401 会清理当前 token 并通知桌面回到登录态；2026-10-15 补齐 Connector 凭据独立过期时用当前登录会话换取新票据、重启连接并仅重试一次 Local Agent Bootstrap，刷新资源也可再次恢复。真实账号登录后项目、终端和联系人资源正常加载，原始 `invalid or expired token` JSON 不再污染全局错误区）
- [ ] 登出、切换账号、窗口关闭与再次启动（2026-10-16 已完成下方真实登出、未认证重启、同账号重新登录、资源恢复及再次启动窄闭环；仍缺第二测试账号的实际账号切换、未保存状态和跨账号敏感缓存审计，故综合项保持开放）
- [x] 真实登出、未认证重启、同账号重新登录与资源恢复（2026-10-16 最终 Debug 构建中由账号菜单真实登出：工作区与账号标签清除、账号/密码为空、登录按钮禁用、Local Agent Host 停止；未登录关闭后 Desktop/Host 均归零，重启仍停留登录页。使用验收账号重新登录后联系人、项目、本机、远端及真实 `ChatOS 仓库管理` 项目入口恢复；再次正常退出和完整重启恢复认证、项目文件树与 Host。为退出菜单补稳定 `ChatOS.Shell.SignOut` ID；专项 48 项、常规 1,496 项、Rust Host/x64 Debug 0 警告/错误通过，证据见 docs/21）
- [x] 登录页精灵图加载时序、无旧 fallback、加载失败表现（已完成并验证）
- [x] 启动异常、后台恢复、Connector/Local Agent 重启（本地 Agent 预启动失败不再阻塞已认证工作区，主窗口不因可选功能异常退出）

## 2. 主窗口与导航

- [x] 主窗口尺寸、标题栏、主题、字体和缩放（首轮对照；Debug 启动验证通过）
- [ ] 左侧导航、资源分组、项目/联系人/本地资源/远端资源
- [ ] 刷新、创建资源、空状态、加载状态、错误状态
- [x] 页面切换时状态保留、返回行为和关闭确认（Git/Run/聊天/Agent 页面切换前关闭旧项目会话；文件页切换目录标签前复用未保存修改确认；离开项目会清理文件会话）
- [x] 键盘导航、AutomationProperties 和可访问性（聊天控件补齐名称；UI smoke 通过）

## 3. 普通聊天

- [x] 会话打开、切换和历史加载失败后的自动重试（2/5/10/20 秒；创建/发送回归待补）
- [x] 模型列表来源与失效默认模型回退（普通聊天模型筛选、供应商推理级别和失效回退已与 macOS 对齐；2026-10-15 补齐保存模型 revision 过期时迁移到当前 revision、模型已删除时回退到首个可用模型，同时保留推理级别和远端选择。最新版真实联系人聊天已从旧 revision 恢复为 `my_api / gpt-5.6-sol`，历史正常加载且不再显示 revision 错误）
- [x] 消息发送入口、Enter 发送、Shift+Enter 换行、停止和错误恢复（2026-10-16 使用真实联系人会话 `conversation_dbf651de99694928a5c5ac2171c3a0a3` 与 `my_api / gpt-5.6-sol` 完成实机闭环：Shift+Enter 生成双行草稿、Enter 发送并清空、两次唯一标记均收到精确模型回复；长回复开始后 Stop 可见且取消完成，随后新消息正常回复。该会话中既有两次真实 HTTP 400 错误仍可查看，之后成功回复证明恢复；正常退出 256ms 后 Desktop/Host 均为 0，完整重启后发送、停止和恢复消息、模型、远端、中文偏好及空 composer 均恢复。10 项 UI 证据断言通过，见 `.local/parity/2026-10-16-0138/`）
- [x] 附件选择、预览、粘贴、大小限制和删除（2026-10-16 实机完成选择/取消、文本与图片预览、删除、拖放、20 个数量上限、单项及累计 20 MiB、短/长文本粘贴、剪贴板 PNG 规范化、权限拒绝、正文与附件发送失败共同恢复、成功模型消费、历史/vault 持久化和重启恢复。修复异步文本剪贴板导致“附件+正文”重复粘贴、BMP 冒充 PNG、读入前未裁剪预算及登录恢复 runtime 竞态；真实模型分别返回 `ATTACHMENT-OK-20261016-0215` 与失败恢复后的 `POST-FAILURE-OK-20261016-0715`。英文选择器、系统权限拦截、正常退出和完整重启均已通过，详见 docs/16）
- [ ] Markdown、代码块、复制、引用和消息操作（首轮修复代码字面显示/围栏边界；15:05 代码块用单个原生 Paragraph / LineBreak；15:20 修复链接完整目标/转义；15:35 增加语言标签与独立样本；15:50 补齐多反引号代码；16:05 接通普通正文软换行和同段跨源行代码；16:20 补齐一至六级标题及空白分隔，修复普通源行合并后被误判成新标题/列表。2026-10-14 16:35 增加普通双波浪号原生删除线，保留代码、转义和未闭合源文；新增 29 项回归，相关 Markdown/项目预览 193 项、完整常规测试 1,343 项、x64 Debug 构建及匿名 UI smoke 通过。独立原生样本 17 项检查通过，浅色/深色/窄窗口三张截图已查看；不覆盖真实聊天和剪贴板。含竖线和独立图片源行仍按行隔离，表格仅做 RTF 能力探索、未接入产品。登录后选字复制、焦点、浏览器激活、读屏、流式滚动与共享入口仍待验收；硬换行、完整列表/引用、表格/图片、嵌套样式、引用式链接、语法着色和完整消息操作未完成，步骤见 docs/12-windows-chat-markdown-acceptance.md）
- [ ] 同轮追加指导消息的历史恢复、顺序和附件展示（2026-10-14 10:04 补齐 Local Agent 历史映射、缓存字段及原生聊天模板；持久化序号优先，旧历史回退时间，保留交接/任务回执顺序；新增 12 项执行用例，完整 755 项常规测试、x64 Debug 构建及匿名 UI smoke 通过，登录后消息和附件交互待验收）
- [ ] Ask User、审批提示、任务进度和实时事件（10:19 补齐表单约束和处理状态；2026-10-14 10:49 补齐刷新保留草稿/选择/错误/提交中状态、旧刷新与会话隔离、已解决请求防复现、离开或移除时敏感值清理及失效卡片禁用；2026-10-16 已完成下方真实 Ask User 必填/单选/敏感策略/提交继续、普通聊天与 Pet 内嵌多选，以及提交时 Local Agent Host 中断、输入保留、恢复后重试成功窄闭环。Local Ask User 工具固定写入 86,400,000 ms 且 schema 不接受自定义 timeout，本轮无法等待 24 小时；timeout 终态和审批链路仍待验收，故综合项保持开放）
- [x] 真实 Task 触发的 Ask User 提示、提交继续、敏感策略与取消清理（2026-10-16 先以 `WIN-TASK-CANCEL-20261016-0745` 验证选择提示和任务图取消清理；随后以两个真实 Task 验证必填文本、多行、默认 Continue、敏感掩码与 Local Agent 按 macOS 同策略拒绝敏感值持久化、取消清理，以及普通混合表单提交后 Task 精确返回 `ASK-CONTINUE-OK-20261016-0845`。最终 760 DIP 卡片不再被 Pet 遮挡，退出/重启不复现已处理表单，详见 docs/19）
- [x] Ask User 真实多选边界、Pet 内嵌卡、提交继续与可访问入口（2026-10-16 主聊天与 Pet 分别创建真实 waiting_user Task；0/1 项时提交禁用，精确 2 项时启用，第 3 项被上限规则拒绝，取消一项后重新禁用。修复 waiting Task 被 Pet 错映射为 TaskExecution、详情不加载表单，以及 Pet 活动列表/表单缺少稳定可访问入口、必填/范围/描述/多行状态的问题；Pet 最终提交 one+three 后 Task 精确返回 `PET-POSTFIX-OK-one-three`。专项 25 项、常规 1,493 项和最终构建通过，证据 `.local/parity/2026-10-16-1045/`，详见 docs/19）
- [x] Ask User 提交时 Host 中断、输入保留、凭据化恢复与重试成功（2026-10-16 真实 required 文本表单填入唯一标记后强制结束唯一 Local Agent Host；提交明确显示 `Local Agent Host is not running.`，字段值原样保留、提交/取消重新可用。修复异常恢复把尚未注入模型凭据的临时 Host 过早暴露给消费者，导致重试已写 `ask_user_submitted` 后续跑因凭据环境缺失进入 `needs_review`；现恢复只在凭据化重启后重新绑定服务。最终真实 Task 返回精确 `ASK-RECOVERY-FINAL-OK-20261016-1450`，Task/Run 均 succeeded、无 `run_needs_review`；退出、完整重启后认证、Host、项目聊天和结果恢复，已解决表单不复现。专项 16 项、常规 1,497 项、Rust Host/x64 Debug 0 警告/错误，证据 `.local/parity/2026-10-16-1430/`，详见 docs/19）
- [ ] 历史刷新时保留正在查看的回执与轮次过程（2026-10-14 17:05 按轮次 ID 和 Revision 复用未变化条目，保留过程展开、回执详情、读取中及错误状态；实时追加、加载较旧历史、其他轮次更新/重排及乐观发送失败不再重建未变条目，刷新不触发集合 Reset。内容变化的目标轮次仍替换并取消旧读取，切换会话即使 ID 相同也重新创建。新增 9 项执行用例，修改前 1 项复现失败，修复后相关聊天/任务图 185 项、完整常规测试 1,372 项、x64 Debug 构建及匿名 UI smoke 通过；登录后面板、焦点、滚动及截图待验收，变化轮次内回复保留/自动重载和完整滚动定位仍待开发核对，步骤见 docs/13-windows-task-reply-inspector-acceptance.md）

## 4. 执行任务与任务图

- [x] Task 创建、状态轮询/实时更新和取消（11:50 补齐活动图每 2 秒串行轮询；2026-10-14 12:05 接入会话实时订阅，当前轮次终态或 Reconcile 触发图/详情刷新，忙碌时合并通知；收到事件后采用 60 秒保底，枚举结束/失败恢复 2 秒轮询，关闭取消订阅；修复本地任务图事件被相同会话版本过滤的问题。2026-10-16 已以真实任务完成 create/running/waiting_user/cancelled/succeeded/failed 状态、实时终态、取消和重启恢复；本轮强制终止 Host 后 0.3 秒内由 Desktop 拉起新 Host，任务图与等待态不丢失；另将唯一子 Host 暂停超过 75 秒请求期限，旧 Host 被生命周期回收并重建，Local Agent 共享事件 Hub 按 macOS 同样的 1 秒重试策略保持订阅，任务图仍保留同一 Run 并可取消。通用 realtime 枚举异常的 2 秒轮询回退已有 109 项 Task Graph 专项覆盖；本地 Hub 按设计吞掉瞬时 Host 异常并自行重连，因此不虚构本地错误条闪现证据。详见 docs/17）
- [x] 真实单 Task 创建、运行、Ask User 等待、取消、实时终态与重启恢复（2026-10-16 使用真实模型创建 `local-task-cc4cfac8-4605-46de-bd63-3e8900aaac47-1`，UI 从 `running` 实时切换 `cancelled`，显示 `run_cancelled / user requested cancellation`；取消按钮转为禁用，正常退出后完整重启仍恢复任务、Run、事件和可重试状态。Task/Run 只读数据库状态分别为 cancelled/version 4 与 cancelled/version 5。断流回退、失败/重试、多节点依赖和项目边界仍归综合项，见 docs/17）
- [ ] 任务图、节点详情、输出和失败重试（11:19 补齐节点切换、分页和操作隔离；11:34 补齐空图恢复和原生状态提示；11:50 补齐常规轮询及详情/草稿/事件保留；2026-10-14 12:05 增加事件驱动刷新、断流回退、中英文连接错误提示和手动刷新重新订阅。2026-10-16 实机创建 `thinking_level=minimal` 的真实失败 Task，完整显示 400、`run_failed` 和可用重试；点击后产生不同 Run 并再次权威失败。另一个 waiting_user Task 先取消再重试，产生新 Run、重现 Ask User 并精确返回 `RETRY-SUCCESS-OK-20261016-0920`；完整重启后两次失败、成功结果、最新 Run、错误和重试入口均恢复。修复重试在消息专属图为空时没有回退同 turn 持久化图的问题，并扩展 Connector 回归。多节点依赖、真实 61 条事件分页、响应式工作区、可缩放连线画布和项目边界/写审批窄闭环已在下方通过；但真实补充指令重试仍由共享 Host 重放原始 `prompt`，没有消费新 Run 的 `retry_instructions`，故综合项保持开放，详见 docs/17）
- [x] 真实单节点任务图、详情、完整事件链与取消后恢复（2026-10-16 最终构建实机显示标题、目标、Run ID、`task_run_created` 至 `task_callback_written_back` 的过程以及取消终态；重启后取消禁用、重试可用。未点击重试，未验收分页、失败任务和多节点图，且宠物悬浮窗在最终截图中遮住右下部分操作区，仍需单列布局回归）
- [x] 真实多节点依赖、等待解锁、终态级联与重启恢复（2026-10-16 使用 `create_tasks_with_prerequisites` 创建 A→B/C 三节点 DAG；A 在 Ask User 时 B/C 保持 pending，提交后 A/B/C 按依赖全部 succeeded 并返回精确标记。修复 Windows 丢失前置任务标题/状态、依赖节点无可见关系及无 Run 节点残留其他节点 Run ID；最终依赖列表、无 Run 提示、可访问名称和取消级联均通过，完整重启后同样恢复。常规 1,491 项通过、最终构建 0 警告/0 错误，证据 `.local/parity/2026-10-16-0945/`）
- [x] 真实 Task 事件分页、计数、去重与重启恢复（2026-10-16 创建无文件/网络副作用的 `WIN-TASK-EVENT-PAGE-20261016-1015`，真实 Run 产生 61 条持久化事件；首屏严格为 `40 / 61`，点击后为 `61 / 61` 且加载按钮消失，终态操作不被分页改变。修复“加载更早过程”与正序追加语义不符及缺少当前/总数的问题，补齐中英文“加载更多运行事件 / Load more run events”、`40 / 61` 可访问计数和零事件提示；正常退出后 Desktop/Host 归零，最终构建重启恢复同一 Task/Run 并重新从首屏 40 条加载。常规 1,491 项通过、构建 0 警告/0 错误，证据 `.local/parity/2026-10-16-1015/`）
- [x] 任务工作区响应式宽度、窄窗节点/详情/操作可达与重启恢复（2026-10-16 在 125% 缩放、900×700 实机复现固定 620 DIP 抽屉越过会话宿主，节点栏被裁至约 65–75 px；现工作区最大 1280 DIP 且不超过宿主宽度，节点栏按 148/220/最多 360 DIP 自适应。真实 A→B 取消 DAG 在窄窗显示两个节点、依赖摘要、详情、刷新/关闭和固定操作区；有 Run 的 A 重试可用，无 Run 的 B 显示前置任务且重试禁用，刷新保持 B，关闭返回聊天。旧持久化认证在首次最终重启后失效，按验收账号重新登录后正常退出/重启直接恢复认证，并重新打开真实 A→B/C 三节点 DAG；节点栏仍为 185 px。新增 1 项布局契约，常规 1,494 项通过、构建 0 警告/0 错误，证据 `.local/parity/2026-10-16-1300/`。macOS 可缩放连线画布仍归综合项，旧凭据失效仍归登录综合项）
- [x] 任务图可缩放连线画布、拓扑分列、平移/居中/适应与窄窗控制（2026-10-16 对照 macOS `MessageTaskGraphCanvas`/`Layout`，Windows 由线性列表升级为按拓扑深度分列的原生 Canvas，绘制 A→B/C 贝塞尔依赖曲线，并保留右侧 Inspector。真实历史数据服务端 Depth 全为 0，实机首版因此纵向堆叠；现从 prerequisite 关系推导深度并以 2 项自动化覆盖。按钮缩放两次修复 `ChangeView` 竞争后，实机稳定从 86% 到 200%、重置 100%、Fit 86%；Windows 原生 ScrollPattern 平移 50→80，再 Fit 居中。125% 缩放、900×700 下 Fit 为 25%，三节点、五个视口控件、Inspector、刷新和关闭均可达；最后压缩图标按钮并清除内边距，截图确认 Center/Fit 图标完整。最终重启恢复认证、历史、Host 和同一 DAG。常规 1,496 项、布局专项 2 项、契约专项通过，最终构建 0 警告/0 错误，证据 `.local/parity/2026-10-16-1330/`。合成鼠标左拖未形成可靠 WinUI Pointer 证据，因此不宣称该输入路径；Windows 原生滚动平移已通过）
- [x] 真实 Task 项目绑定、目录边界、项目写审批与重启恢复（2026-10-16 在绑定 `C:\project\chatos-repository-manager` 的真实项目聊天中创建 `WIN-TASK-PROJECT-BOUNDARY-FINAL-20261016-1520`：项目内 `REPOSITORY.md` 读取成功，`../README.md` 越界读取以 `Local platform tool failed.` 拒绝；`project_write` 显示绑定正确项目根的真实审批卡，点击“本次允许”后仅创建 `docs/TASK-PROJECT-BOUNDARY-OK-20261016-1520.txt` 并回读精确标记，Task/Run 均 succeeded。正常退出 Desktop/Host 归零，最终构建重启恢复认证、项目聊天、Task/Run、审批事件、标记文件和 Host。专项 Connector 102 + Presentation 111、常规 1,497 项、Rust Host/x64 Debug 构建通过；证据 `.local/parity/2026-10-16-1500/`，详见 docs/17。共享 Host 的补充指令重试缺陷不在 Windows 编辑范围，另行保持阻塞）
- [ ] W308-04 Task 验收合同、逐项证据、最终原文与字符约束（2026-10-17 已实现冻结 `acceptance_report_version=1` / `acceptance_criteria` / `final_response_max_chars` 和 Run `terminal_outcome.task_acceptance` 的严格解析，独立用 `EnumerateRunes()` 复算 Unicode scalar；缺标准、重复索引、非法状态、空/超长 evidence、succeeded 含非 passed、空/超限原文均拒绝。Task 图已展示逐项 passed/failed/not_verified、evidence、最终原文、字符数/上限和非独立审计提示；succeeded 且缺有效报告时 fail-closed。Core 6/6、Connector 6/6、Presentation 113/113 及最终 Debug 构建 0 警告/错误。真实最终客户端的历史 Task、退出和重启兼容通过，但历史 Task 无 v1 合同，不能验证新版报告卡；成功/失败/not_verified、Unicode 超限和持久化重启仍待真实 v1 Task，证据 `.local/parity/2026-10-17-0530/`）
- [ ] 回执内执行过程/任务详情的读取与切换（2026-10-14 16:50 增加每次读取的取消和代次隔离，修复“过程→详情→过程”后旧成功/错误覆盖当前内容及加载状态的问题；折叠、会话切换/释放、历史条目替换时取消并清理旧面板，任务读取取消后不继续读取运行详情。新增 20 项执行用例，修改前两项取消回归失败，修复后相关聊天/任务图 176 项、完整常规测试 1,363 项、x64 Debug 构建及匿名 UI smoke 通过。真实回执点击/焦点/滚动与截图待登录后验收；实时刷新、结构化过程时间线、独立输出状态及 Mac 单面板选择仍待核对，步骤见 docs/13-windows-task-reply-inspector-acceptance.md）
- [ ] 任务模型绑定、项目边界、权限和审批链路（2026-10-16 已完成上方项目读取、越界拒绝、单次项目写审批和重启恢复窄闭环；仍缺模型绑定、拒绝/会话允许、更多权限组合和完整重试链路，且共享 Host 当前未把 `retry_instructions` 送入执行模型，故综合项继续开放）
- [ ] 空状态、恢复状态和历史任务展示（2026-10-14 11:34 增加任务图有限空响应补查、手动刷新重试和已有图保留；自动化通过，真实界面及历史任务完整展示仍待验收/开发）

## 5. 项目与文件工作区

- [x] 创建项目、选择父目录、新建文件夹和命名校验（2026-10-14 11:04 补齐重叠目录请求、工作区切换、旧文件夹创建/刷新结果隔离，读取中禁止保存，保存期间冻结表单及关闭时取消。2026-10-16 最终构建实机在授权工作区用户目录创建 `Windows-Project-Parity-20261016-1645` 文件夹并选择为项目根，完成项目注册；发现固定 700 DIP 内容使目录工具和新建文件夹按钮在 ContentDialog 内被裁掉，改为随视口伸缩并给整套控件增加稳定 AutomationId。非法 `../outside-final-visible` 被拒绝且错误在输入框下即时可见，无越界目录；新建文件夹取消、整个创建弹窗取消均通过。创建项目专项 33 项、桌面契约 25 项、常规 1,505 项及 SDK 8 x64 Debug/Rust Host 构建通过，证据 `.local/parity/2026-10-16-1645/`）
- [x] 项目列表、项目切换、项目持久化和重新打开（2026-10-14 09:19 补齐旧文件/目录读取的成功、错误和加载状态隔离，以及关闭时排队回调的取消处理。2026-10-16 实机创建项目后立即出现在项目列表并打开空目录；切换到既有 `ChatOS-Git-Parity-20261016-1530` 再切回新项目通过。多次正常退出 Desktop/Host 均归零，最终构建重启后认证、Host、三个项目、隔离目录和新项目重新打开均恢复；`final-verified-assertions.json` 全部为真，详见 docs/15）
- [ ] 创建项目后进入项目聊天，读取/搜索/探索、修改、返回结果和重启恢复（2026-10-17 最高优先级：已对齐 macOS `applyPreparedConversation`，首次会话创建后回写工作区、侧栏和当前选择的 `ConversationId`，避免聊天标签被资源替换重置；新增项目内加载、失败、重试 UI。创建→聊天→文件→聊天、只创建一次、失败重试、账号切换隔离及可访问入口自动化通过；Connector 129/129、Presentation 230/230、最终 Debug 0 警告/错误、客户端启动/正常退出通过。当前会话缺少 Computer Use 的 `node_repl/@oai/sky` 控制入口，真实新项目读/搜/写审批/修改/重启未执行，故保持开放；证据 `.local/parity/2026-10-17-2024/`）
- [ ] 文件夹/文件目录、搜索、刷新、排序和目录显示（已有根目录检索、独立结果区、按需展开、面包屑及命中行列定位；2026-10-14 09:34 补齐搜索状态 UI 调度、手动搜索取消、过期请求隔离及单路同步/异步失败时保留另一路结果。2026-10-16 实机发现无命中会直接退回目录面板，现以非空查询维持搜索面板并显示加载/无命中状态，清空后才恢复目录；补齐中英文无命中文案、切换目录视图本地化和清空按钮 AutomationId/无障碍名。真实中文/英文无命中、`README` 命中并打开、清空恢复目录及重启持久化通过；项目文件专项 139 项、常规 1,478 项、x64 Debug/Rust Host 构建通过。完整慢/失败请求、权限、排序、树展开与刷新恢复仍待验收，见 docs/15-windows-project-files-acceptance.md）
- [ ] 代码定义、引用、符号结果和导航历史（2026-10-14 09:49 补齐代码导航独立进度状态、换文件/位置/项目后的取消及旧响应隔离，搜索和导航不再互相结束忙碌状态；新增 19 项执行用例，文件页 79 项、完整 743 项常规测试和构建通过；登录后按钮、进度环及实际跳转仍待验收）
- [ ] 新建、重命名、删除、复制、移动、打开文件（已有保存快照、新草稿保护及旧项目保存结果隔离；09:19 补齐过期文件读取错误隔离。2026-10-14 17:20 修复新建文件/目录、重命名、删除进行中打开文件、刷新或切换目录后操作一直忙碌的问题；按项目会话释放操作状态，继续隔离旧页面结果，旧项目响应不清除新项目的进行中状态。新增 16 项执行用例，修改前 12 失败/4 通过，修复后文件页/文本定位相关 105 项、完整常规测试 1,388 项、x64 Debug 构建及匿名 UI smoke 通过；真实项目慢操作、弹窗/焦点及截图待登录后验收，完整复制/移动与刷新失败恢复仍需继续核对）
- [x] Git/Run 等项目功能页返回文件页，以及真实创建目录、创建/编辑/保存/重命名/删除文件、命名校验与重启恢复窄闭环（2026-10-16 实机复现 Git→文件后文件会话未重开，目录空白、搜索无结果且创建静默无效；补齐 `files` 壳层分支。随后复现目录刷新或重命名后右侧标题残留旧文件名/路径，改为显式派生属性刷新。隔离仓库中完成目录与文件 CRUD、非法 `../outside.txt` 禁用、磁盘核对、完整退出/重启和最终 Git→文件往返；新增 2 项回归，常规 1,501 项、x64 Debug/Rust Host 构建通过，证据见 `docs/15-windows-project-files-acceptance.md`）
- [x] Git 状态、提交、分支、差异和冲突提示（2026-10-16 在隔离真实仓库完成干净/修改/暂存/提交、工作区与暂存 Diff、新建并切换分支、切回 main、制造合并冲突、冲突 Diff、恢复干净状态及完整重启回归。实机发现失败合并会改变索引但 Windows 保留旧的“工作区没有修改”，且 Git 把冲突诊断写到 stdout 时只显示通用失败；现失败变更会对齐 macOS 立即重载权威快照，同时合并 stdout/stderr 识别中文冲突提示。新增 2 项回归；专项 14 项、常规 1,499 项和 x64 Debug/Rust Host 构建通过，证据见 `docs/22-windows-project-git-acceptance.md`）
- [ ] 树内嵌套条目的重命名与删除定位（2026-10-14 17:35 按条目的实际父目录重命名并刷新，修复嵌套重命名被当作移到浏览目录、删除后刷新错误目录的问题；保留 URI 原文、盘符/UNC 和根分隔符，无法解析父目录时提示刷新重试。树菜单补充从已展开原生节点查找条目。新增 18 项执行用例，修改前合计 16 失败/2 通过，修复后文件页/文本定位相关 123 项、完整常规测试 1,406 项、x64 Debug 构建及匿名 UI smoke 通过；树菜单实际点击、真实文件位置/持久化、焦点与截图仍待登录后验收，未宣称完整树交互完成）
- [ ] 文件操作已完成后的目录刷新失败提示与恢复（2026-10-14 17:50 已实现当前浏览目录的成功提示提前发布、刷新失败说明无需重复操作、保留原始错误，以及只读目录的手动刷新重试。新增 25 项回归，首批修改前 8 失败/13 通过，修复后相关 148 项通过；完整常规日志显示 1,431 项通过，但恢复会话后无法取得原命令退出码。2026-10-15 01:43 用户要求先同步 3.0.8，fetch 因 .git/FETCH_HEAD 写权限失败，尚未获取最新上游。x64 Debug 构建、本轮 UI smoke、登录后真实操作与截图尚未执行，不能标记完成；嵌套树的异步刷新恢复仍待开发核对）
- [x] Run 配置、启动、停止、日志和工作目录（2026-10-16 在真实隔离项目中从 Windows 文件页创建并保存 `package.json`，重新分析得到 `npm start/test`、正确工作目录和 Node/npm 工具链。实机发现 `npm run "start"` 在当前 cmd 启动链把引号作为脚本名传给 npm，修复为安全白名单名不再额外引号；成功目标显示实时唯一日志，失败目标保留唯一 stderr 与退出码 7。主动停止原被误标 `failed` 且遗留孤儿 Node，现以 Windows Job Object 约束整棵进程树并映射为 `stopped`；停止后 3 秒及运行中关闭客户端后均无标记 Node。实例删除、正常退出、重启后的认证/Host/目标/配置/空实例恢复均通过。新增 4 个终态回归，最终常规 1,509 项、SDK 8 x64 Debug/Rust Host 0 警告/错误，证据 `.local/parity/2026-10-16-1730/`，详见 docs/23）

  - 最终回归补充：新增真实 PowerShell → `ping.exe` 后代进程的 Job Object 终止测试；Run 专项 **9/9**，常规测试 **1,510 通过、0 失败**，执行后无 `ping.exe` 遗留。以上结果取代上一行的 1,509 项中间基线。
  - 2026-10-17 23:46 自动化重复执行该回归时，子进程原先通过 `Start-Process` 打开默认 Windows Terminal；Job 回收 `ping.exe` 后留下 `0x800700e8` 管道关闭错误窗口。测试夹具现改为 `UseShellExecute=false`、`CreateNoWindow=true` 的直接子进程，仍验证同一 Job 后代回收；定向 1/1，通过前后 `WindowsTerminal`/`OpenConsole`/`ping` 均为 0。产品 Run 行为未改，见 `.local/parity/2026-10-17-2345-terminal-popup/`。

## 6. AI 创作 / Media Studio

- [ ] 图片/视频生成完成但本地历史保存失败时的状态区分和错误提示（与 macOS 对齐；真实供应商生成仍待回归）
- [ ] 图片模型列表、默认模型、尺寸、数量和参考图
- [ ] 图片生成、进度、结果预览、保存和历史
- [ ] 视频模型列表、协议、尺寸、时长、首尾帧和参考素材
- [ ] 视频生成、轮询、失败恢复、下载和历史
- [ ] 剧情项目创建、文本/图片/视频模型选择
- [ ] 剧情规划、优化、分段、连续性和批量生成
- [ ] 图片/视频/剧情的错误、空状态和未完成任务恢复

## 7. Agent / Agent Teams

- [ ] Agent 列表、详情、创建、编辑和删除
- [ ] Agent 能力、模型绑定、工具和权限
- [ ] Agent 对话、群聊、调度、暂停、恢复和结果
- [ ] 本地 Agent 文档、项目绑定和空状态

## 8. Connector、设备与本地运行时

- [ ] 设备配对、连接状态、断线重连和断开（2026-10-15 已修复 Connector 旧凭据 401 自动刷新、连接 revision 重启和 Local Agent 单次重试；真实项目文件与本机终端可用。设备页刷新后仍显示“同一账号已有另一个 Local Connector 客户端在线/等待重连”，需在另一个客户端退出后继续验收服务端互斥、重连和断开，不能把该外部状态标为完成）
- [ ] 供应商、模型配置、字段映射和同步
- [x] 普通聊天模型与任务模型的筛选规则（普通聊天保留所有有凭据模型；任务模型继续使用 task_enabled；推理级别按供应商目录同步）
- [ ] 命令审批、审批模型、审批历史和安全回退（12:20 补齐历史分页/缓存，12:35 接入事件同步与释放保护；2026-10-14 12:50 修复模式已保存但模型检查期间仍显示旧模式、打开时历史/模式被就绪检查阻塞的问题；增加检查中、独立错误和重试，重试不重写策略；请求确认/完全控制不依赖审批模型检查，前台历史读取消费已有通知以避免重复刷新。2026-10-16 已完成下方请求确认、完全控制和自动审批不可用安全回退两个实机窄闭环；审批模型真实自动同意/拒绝、检查异常/重试、远端命令/上传和跨入口矩阵仍待验收，故综合项保持开放）
- [x] 请求确认模式的真实命令审批、拒绝/本次允许/会话允许、历史分页与重启边界（2026-10-16 在绑定本地项目的真实 Task 中执行无文件副作用的 `powershell Write-Output`：拒绝后命令未运行且 Task 明确阻塞；本次允许返回精确标记；本会话允许后同范围第二条命令无重复提示并以 `session` reviewer 审计。累计 13 条真实历史后，设置页 `1–10 / 12`、`11–12 / 12`、刷新保留第二页和重启后的 `1–10 / 13` 均通过。正常退出 Desktop/Host 归零；完整重启后同范围命令重新弹出审批，证明会话授权不持久化。审批专项 81 项、常规 1,493 项、最终 x64 Debug/Rust Host 构建通过；证据 `.local/parity/2026-10-16-1200/`，详见 docs/20。自动审批模型、完全控制确认、检查失败/重试和跨远端入口仍归综合项）
- [x] 完全控制风险确认、策略直通、自动审批不可用安全回退与模式重启持久化（2026-10-16 先实机验证完全控制风险弹窗及取消后保持“需要确认”，确认后真实 Task 无审批卡直接执行只读 `Write-Output`，审计为 `full_control / policy`；完整退出重启恢复完全控制。自动审批风险确认后设置页显示模型配置就绪，真实调用时审批 reviewer 不可用并安全回退为用户确认，本次允许后 Task 成功，审计为 `auto_approval / user`；再次重启恢复自动审批，最终已恢复并持久化推荐的“需要确认”。审批专项 100 项、常规 1,493 项、最终 x64 Debug/Rust Host 构建通过；证据 `.local/parity/2026-10-16-1230/`，详见 docs/20。审批模型实际自动同意/拒绝与检查失败重试仍归综合项）
- [x] 本地终端、命令执行、输出搜索/匹配、停止、清空和切换时旧会话事件隔离（Windows 页面已有完整交互；已修复旧会话输出串入新会话；2026-10-15 真实账号下显示“已连接”，执行 `Write-Output CHATOS-ACCEPTANCE` 并得到 `CHATOS-ACCEPTANCE`）
- [ ] 运行模式、权限控制、沙箱和系统权限
- [ ] Plugins/Skills 安装、启用、更新、卸载和错误

## 9. Notepad

- [x] 保存期间正文输入、整份编辑快照保护及再次保存后的重启恢复（2026-10-15 14:00 对照 Mac save 的三字段共同判断，实机复现正文已变但响应仍重写标签。Windows 改为任一字段改变即保留标题/标签/正文全部输入；新增3项回归修复前全失败。最终记事本194项、常规1476项通过，Rust Host/x64 Debug构建0警告/错误。实机暂停本次客户端子Host制造真实延迟，保存忙碌时键入正文，恢复Host后原始标签与新正文保留、提示仍未保存；数据库仅写提交快照，再次Ctrl+S写入新正文并规范化标签。正常退出进程归零，重启恢复认证与保存内容；10组证据断言通过，见docs/14的14:00章节及.local/parity/2026-10-15-1400/。仅正文输入组合有最终实机证据，标题/标签输入组合为自动化证据；失败期间输入、自动保存导航竞态、版本冲突和权限继续待验收）

- [x] 空标题保存校验、失败切换选择恢复和自动保存入口草稿保护（2026-10-15 13:15 实机复现空标题原样返回英文服务错误，增加本地中文/英文校验并清旧成功提示；6项回归修复前全失败、修复后通过。实机进一步发现同步校验失败后原生列表仍选到目标，改为 ItemClick 结束后排队恢复 SelectedResultNote。最终实机通过无效标题保存、关闭时保存失败留草稿、切换失败保持原蓝条、新建被阻止且无额外记录、正常导航。记事本191项、常规1473项通过，最终x64 Debug/Rust Host构建0警告/错误；证据 .local/parity/2026-10-15-1315/13及14，详见docs/14。创建后错误恢复与完整键盘/权限仍待验收）
- [x] 真实数据库写锁等待、保存失败保留草稿、解锁重试和完整重启（2026-10-15 13:15 现有数据库BEGIN IMMEDIATE只占写锁不改行，真实客户端出现忙碌并禁用保存/关闭/刷新/搜索/新建，约5秒后实际database is locked。失败保留草稿且恢复保存/关闭；释放并回滚辅助锁后重试成功清错/脏标记，正常退出Desktop/Host归零，重启恢复认证与标题/标签/正文、保存禁用。最终证据15–19、锁时间记录、只读数据库快照及10组断言；仅目标标题经两次成功保存version8到10，原笔记不变。此项不覆盖保存期间继续输入、真实并发版本冲突、权限及慢/失败搜索）


- [x] 当前目录定位提示及刷新后的选择恢复（2026-10-15 12:45 实机复现目录列表无选择状态；增加按路径取得当前集合实例的 SelectedFolderItem、根目录选中状态和键盘选择导航。中间版本实测发现刷新/重命名后标记仍丢失，改为集合替换完成后发布选择通知；新增 4 项回归，前三项及通知顺序分别在修复前失败。最终记事本 185 项、常规 1,467 项通过，x64 Debug/Rust Host 构建0警告/错误；实际刷新、重命名、根目录重复点击、方向键上下切换/空目录及重启后选中标记通过。Windows 目录列表保留独立编辑区，不代表 Mac 单树节点结构和全部键盘行为已对齐；详见 docs/14 本轮章节）
- [x] 含笔记目录重命名、目标已存在错误重试、草稿与保存后重启恢复（2026-10-15 12:45 复用测试笔记 ec4d0dda-b983-40d6-a786-e37a320c5661，实机通过含笔记目录改名、真实后端目标冲突不改变路径/草稿、有效重试清除错误、改名后保存无版本错误，以及完整重启恢复路径/标题/标签/正文。最终路径 Windows含笔记已重命名-20261015-1245，标题 Windows记事本验收-跨目录-1115-目录改名保存1245-终验。此项不覆盖真实并发版本冲突、刷新失败、权限拒绝或含笔记子树的全组合；证据 .local/parity/2026-10-15-1245/，综合目录重命名项继续待验收）

- [x] 输入搜索词自动防抖、无命中和清空后的编辑保护（2026-10-15 实机复现原先必须手动搜索，改为 260ms 自动触发；新输入立即取消旧查询，Enter/按钮取消防抖避免重复，关闭取消排队搜索，保存/刷新在防抖期间开始时等待可搜索状态；真实无命中、清空恢复列表、标题草稿/未保存标记保留，最终版本重启后再次通过。跨目录及真实慢请求/失败仍在下方待验收）

- [x] Ctrl+S 保存与未保存标记（2026-10-15 真实中文测试笔记验证编辑后显示标记、快捷键保存后消失并禁用保存；关闭确认取消保留草稿、选择保存后持久化；新增 3 项通知测试，证据见 docs/14-windows-notepad-lifecycle-acceptance.md。完整保存失败/慢请求、放弃和窄窗口仍在下方待验收）
- [x] 窄窗口中记事本操作可达性（2026-10-15 10:15 真实复现 850px 窗口保存/复制/删除被裁切；面板上限由 900×660 改为 Mac 理想 1300×840，正文保留 Mac 最小 1120、编辑区最小 720 DIP，较窄宿主使用原生横向滚动，头部刷新/关闭固定可见。真实中英文宽窄窗口、滚轮、Shift+Tab 自动滚到右端、模式点击及方向键聚焦后空格确认通过；最终英文构建重启恢复笔记与认证，中英文目录标签通过。常规 1,451 项、最终 x64 Debug 构建通过；证据见 docs/14-windows-notepad-lifecycle-acceptance.md。本项只覆盖裁切恢复与操作可达，不代表目录树、可拖动分栏及整页视觉已完全对齐）
- [ ] 打开、关闭、尺寸和 macOS 对齐的完整布局（窄窗口操作可达性已通过；Mac 目录树、侧栏拖动和全部缩放组合仍待验收）
- [ ] 编辑模式标题/标签入口、显示模式与正文布局（2026-10-14 14:20 标题和标签移到正文双栏之外，修复编辑模式将它们随预览列压到零宽的问题；三种模式共用编辑字段，分栏为左编辑/右预览，单栏不保留双栏间距。模式使用原生单选组并同步当前选中项，补齐输入框和模式 AutomationId、输入名称本地化，文本操作按钮按内容宽度显示。既有记事本/设置回归 132 项、完整常规测试 1,111 项、x64 Debug 构建及匿名 UI smoke 通过；实际正常入口探测被登录态阻塞，三种模式的点击/键盘/焦点、布局及中英文显示仍待验收，窄窗口整体对齐未完成）
- [ ] 新建、编辑、保存、复制、重命名和删除（已有关闭前保存/放弃确认及复制当前笔记；2026-10-14 13:05 对齐 Mac 保存快照，元数据响应不再清空正文，保留保存期间的新输入和未保存状态，自动保存后仍有新输入时留在当前笔记；阻止重复保存、保存期间刷新/搜索和关闭，隔离关闭重开、释放和取消后的晚到结果。新增 21 项执行用例，记事本回归 24 项、完整常规测试 1,027 项、x64 Debug 构建及匿名 UI smoke 通过；登录后的保存、列表选择恢复、关闭确认及持久化仍待验收）
- [x] 关闭时放弃修改的实际恢复（2026-10-14 13:35 修复“放弃”只隐藏页面、重开仍出现修改的问题：显式放弃恢复最近成功读取/保存的标题、标签和正文，不写入服务端；保存中禁止放弃，慢搜索先取消，普通生命周期关闭继续保留草稿。新增 11 项执行用例，记事本回归 58 项、完整常规测试 1,061 项、x64 Debug 构建及匿名 UI smoke 通过；2026-10-15 10:00 真实测试笔记验证取消保留三个字段，放弃后返回工作区，重开及完整退出重启均恢复原保存的标题/标签/正文、保存禁用；本轮未提交测试草稿，证据见 docs/14-windows-notepad-lifecycle-acceptance.md）
- [x] 跨目录标题/标签/正文搜索的正常流程、结果高亮和切换持久化（2026-10-15 11:15 实机复现搜索刷新后当前笔记高亮消失；SelectedResultNote 按 ID 绑定当前结果实例，保留独立编辑缓冲区。真实根目录与子目录同时命中、标签/正文查询、重新搜索高亮、无匹配保留草稿、清空恢复当前目录、切换自动保存及完整重启后三字段恢复通过；新增 4 项回归，记事本 173 项、常规 1,455 项及 x64 Debug/Rust Host 构建通过。证据 .local/parity/2026-10-15-1115/ 的 20–22 为重采最终快照；01–06 旧快照保存错误已排除。真实慢请求/失败、权限及搜索期间关闭仍未完成，综合搜索项继续 pending；详见 docs/14-windows-notepad-lifecycle-acceptance.md）
- [x] 空目录创建、当前完整路径处理、空名称校验与返回导航（2026-10-15 12:00 实机复现输入当前完整路径会重复拼接父目录，按 Mac createFolder 的 input == base 分支修复，支持规范化后的同路径；真实空目录、同路径不新增、空名称错误/有效重试、原笔记草稿保留、返回根目录及重启持久化通过。新增路径与提示状态回归共 8 项，记事本 181 项、常规 1,463 项及 x64 Debug/Rust Host 构建通过；证据 .local/parity/2026-10-15-1200/，详见 docs/14。真实权限拒绝和外部服务失败仍待验收）
- [x] 空目录重命名及其子目录路径映射、校验提示与无关笔记草稿保护（2026-10-15 12:00 实机通过拒绝移入自身子目录、有效重试、当前目录和所有子目录映射、无关笔记草稿保留、重启后新目录持久化。修复校验失败仍显示旧成功提示；新建与重命名在校验前清除旧操作提示。只覆盖不含笔记的目录树，包含笔记的重命名、版本冲突、真实刷新失败和全部键盘/焦点仍在综合项待验收；证据见 docs/14 和 ui-assertions.json 的 11 项实机证据断言）
- [ ] 新建笔记的未保存保护及创建后的错误恢复（2026-10-14 13:50 新建前先保存当前修改，保存失败或等待期间又输入时停止新建；创建期间继续输入则保留原编辑区，新笔记可稍后打开。采用创建响应并只重载目录，目录读取失败明确提示已创建、可刷新而无需重复新建；保持当前搜索结果，关闭重开、释放或取消后的旧创建/目录响应不更新当前会话。忙碌时禁用新建入口。新增 25 项执行用例，记事本回归 83 项、完整常规测试 1,086 项、x64 Debug 构建及匿名 UI smoke 通过；登录后的新建弹窗、列表选择、焦点和真实持久化仍待验收）
- [ ] 慢速读取时的编辑保护及会话隔离（2026-10-14 14:05 切换读取和排队 UI 回调到达时核对笔记及三个编辑字段，等待期间的新输入留在原笔记，保存后可重试；初始化、资源重载、详情及通用变更的结果/错误/结束回调均检查当前会话，取消正常结束，旧初始化或变更不继续发起读取。关闭立即结束加载并保留草稿。新增 25 项执行用例，记事本回归 108 项、完整常规测试 1,111 项、x64 Debug 构建及匿名 UI smoke 通过；登录后的慢速切换、列表高亮、焦点及关闭重开仍待实际验收）
- [ ] 删除范围、草稿保护及删除后刷新错误（2026-10-14 14:35 删除只清理目标笔记或目录及其子目录，保留其他笔记的草稿、选择和搜索；仅删除当前笔记期间将标题/标签/正文设为只读，失败或取消恢复，确认删除成功即清理本地目标再刷新目录。目录刷新失败明确提示已删除、无需重复删除，不再重读笔记列表覆盖搜索。规范化目录路径并拒绝递归删除根目录，关闭重开后的晚到结果/错误/结束回调隔离。新增 27 项执行用例，记事本 135 项、完整常规测试 1,138 项、x64 Debug 构建及匿名 UI smoke 通过；登录后的删除确认、只读/焦点、失败重试和真实后端持久化仍待验收）
- [ ] 目录重命名路径、浏览位置与编辑区一致性（2026-10-14 14:50 保留笔记 ID/文件标识和编辑缓冲区，成功响应后立即映射目标目录及子目录、当前受影响笔记和浏览位置，不把无关目录导航到目标。刷新按已应用搜索执行，未命中笔记仍保留在编辑区；重命名后列表读取失败明确提示已完成、无需重复重命名。规范化两端路径，拒绝根目录/空目标/移到自身子目录，同路径不重复提交，旧会话回调隔离。新增 23 项执行用例，修改前 17 失败/6 通过，修复后记事本 158 项、完整常规测试 1,161 项、x64 Debug 构建和匿名 UI smoke 通过；真实弹窗、目录高亮、焦点/光标、版本冲突和持久化仍待登录后验收）
- [ ] 跨目录搜索、无命中和未保存编辑保护（2026-10-14 13:20 搜索独立更新结果并保留当前编辑，关键词搜索展示所有目录的命中项，清空后恢复当前目录过滤；重新搜索取消旧请求，隔离旧结果、错误和加载状态，失败保留原结果，关闭前取消搜索以允许保存草稿。筛选刷新不清空未命中笔记，完整刷新保留缺失笔记的未保存内容；保存也不把未命中的笔记插回搜索结果。新增 23 项执行用例，记事本回归 47 项、完整常规测试 1,050 项、x64 Debug 构建及匿名 UI smoke 通过；2026-10-15 10:00 已移植 Mac 260ms 防抖搜索，真实无命中/清空及草稿保护通过，正文 literal_code 检索命中；新增 8 项防抖/取消/失败重试/保存等待执行用例，常规 1,451 项及最终构建通过。2026-10-15 11:15 跨目录标题/标签/正文、结果选择和刷新高亮、自动保存及重启持久化实机通过（上方窄项）；新增 4 项回归，最终常规 1,455 项通过。真实慢请求/失败、权限及搜索期间关闭仍待验收）
- [ ] Markdown/纯文本展示、持久化和恢复

## 10. Pet / 精灵图

- [x] 精灵图资源、帧切分、动画状态和方向（已完成首轮对照）
- [ ] 拖动、点击、快捷对话、审批提示和窗口位置
- [x] Pet 文件工作台文本文件编辑、保存、取消编辑、外部修改冲突确认和关闭前未保存保护（Windows 单文件工作台已补齐；多标签仍待实现）
- [x] 登录页与桌面宠物使用同一套资源，不回退旧图（已完成并验证）
- [ ] 资源缺失、加载失败和账号切换时的表现

## 11. 插件、远端和全局工具

- [ ] 插件应用列表、搜索、启用和状态
- [ ] 插件应用打开/项目选择/返回与切换时的 WebView、会话 nonce 和本地插件进程清理（真实插件启动仍待外部回归）
- [ ] 远端连接列表、添加、编辑、删除和连接测试（跳板机引用、端口和证书校验已对齐；完整真实 SSH 回归待补）
- [ ] SFTP 目录浏览、搜索、隐藏文件、上传/下载覆盖确认、重命名、删除和二次验证（Windows 页面与 ViewModel 已补齐；真实 SFTP 回归待补）
- [ ] 远端终端命令、验证码取消/重试、清屏、搜索、取消和旧任务隔离（Windows 页面与 ViewModel 已补齐；真实 SSH 终端回归待补）
- [ ] 剪贴板历史、全局快捷键、快速搜索、截图/录屏
- [ ] 权限、冲突快捷键、关闭应用和重启后的恢复

## 12. 设置、语言与视觉一致性

- [ ] 常规与账号、语言、主题、字体缩放（2026-10-14 10:34 增加字体恢复默认、偏好保存进度与失败提示，保存失败/取消恢复已持久化值并允许重试；延迟回调采用当前状态；新增 24 项设置回归，完整 794 项常规测试和 x64 Debug 构建通过，匿名 UI smoke 通过；登录后设置交互、重启恢复及账号语言同步仍待验收）
- [ ] AI 模型配置、重试、审批和提示文案
- [ ] 微信小程序、宠物、全局工具和本地运行时设置（2026-10-14 10:34 全局工具恢复默认改为一次偏好写入，失败不部分恢复；沿用 Windows 既有默认值，快捷键编辑及各模块完整交互仍待验收）
- [ ] 所有页面颜色、间距、圆角、字体、图标和空状态
- [ ] 中英文切换、错误文案和无障碍名称

## 13. 回归验证

- [x] 已保存状态下主窗口退出后桌面/Local Agent 进程回收及再次启动（2026-10-15 复现窗口关闭后进程残留、锁住构建；改为后台异步清理，每步 3 秒期限，失败继续，清理后结束进程。2026-10-16 进一步补齐清理委托在返回 Task 前同步阻塞的单步超时，退出专项 5 项通过；真实搜索回归后退出 0.26 秒完成，聊天回归后退出 256ms，Desktop/Host 均为 0，重启恢复登录与状态。未保存主窗口关闭和账号切换继续待验收）
- [x] Debug 构建 0 警告/0 错误（2026-10-16 最终 x64 Debug/Rust Host 构建通过）
- [x] 登录账号与认证后导航验收（2026-10-15 使用测试账号完成真实登录并恢复会话；最新版依次打开联系人聊天、项目、AI 创作、Agent、需求调研、插件应用、设置、设备与网关、记事本和本机终端，页面未崩溃；聊天模型旧 revision 自动迁移。Connector 网关互斥状态单列在第 8 节继续验收）
- [x] 普通聊天发送一条消息并收到回复（2026-10-16 真实模型两次精确回复，并通过停止后恢复和完整重启历史回归）
- [ ] 创建项目、创建文件夹、显示完整文件树
- [ ] 图片/视频/剧情页面各完成一次可用性检查
- [ ] 设置、Connector、终端、审批、Notepad、Pet 各打开一次
- [ ] 启动、退出、重新启动、账号切换均不崩溃（正常退出、Desktop/Host 清理、重启认证与聊天历史恢复已通过；账号切换仍待验收）

## 14. macOS 3.0.8 新逻辑扩展矩阵

- [ ] W308-01 模型推理能力和 Task/Run 配置冻结（2026-10-17 已接 `gpt-5.6-sol` 的 `low/medium/high/xhigh/max`，模型/开关/`medium` 持久化、连续重启恢复、无启动隐藏写及历史冻结 model ref/revision 展示通过；修复重复 runtime-ready 清空列表导致 SelectedItem 丢失，并用 `IsApplyingSettings` 门控程序恢复。11:20 又完成真实陈旧 revision 的 fail-closed 恢复：当前目录保持可见，只有 Human 显式选择才写入新 revision，随后重启恢复 `gpt-5.6-sol`/`medium` 且 version/timestamp 不再变化；Presentation 57/57、Connector 27/27、Debug x64 0 警告/错误，见 `.local/parity/2026-10-17-1115/`。仍缺真实当前 Task/Run 冻结 thinking level 与刷新/过滤/禁用/关闭/重试完整矩阵）
- [ ] W308-02 需求补充问答、方案版本/确认（Windows 已接同一 Host 协议，严格读取/校验方案 revision、源 Run 状态、Markdown 和 human-only 精确 revision 确认；2026-10-17 06:15 又补齐旧成功 final reply 的显式“保存并确认此方案”入口，只接受准确成功 Run 原文，读取不写入，保存 v1 后确认精确 revision，失败 Run/正文不符/漂移均 fail-closed 且不启动 Task。Connector/桌面契约 30/30、Presentation 全量 862/862、Debug x64 0 警告/错误；最终客户端的汇总空态、项目空态、正常退出和认证重启通过。当前账号无真实调研，成功/失败/取消/冲突/确认/持久化仍缺真数据，见 `.local/parity/2026-10-17-0615/`）
- [ ] W308-03 方案与 Main Chat/Task 冻结绑定（2026-10-17 Windows 已只从 Task 自身输入严格解析已确认方案，独立复算 Markdown SHA-256、校验 task scope/title，并在 Task 图展示固定 revision/正文/prompt/交付/验收/hash；Malformed/未确认/hash 或 scope 不匹配 fail-closed，历史和重试不读取当前方案。Core 39、Presentation 862、Connector/桌面合同定向 31、Host 冻结 4、最终 Debug 构建、旧 Task/21 事件任务图、退出/认证重启通过。当前账号无 plan-bound Task，真实 v1→v2 后旧 Task/重试仍保留 v1 与重启持久化待验收，见 `.local/parity/2026-10-17-0625/`）
- [ ] W308-04 Task 验收合同、证据、原文和字符约束（2026-10-17 Windows v1 冻结合同/报告严格解析、Unicode scalar 复算、Task Graph 逐项 evidence/最终原文/字符上限 UI 和缺失报告 fail-closed 已实现；Core 6/6、Connector 6/6、Presentation 113/113、最终 Debug 构建与旧 Task 兼容/退出/重启通过。仍缺真实 v1 succeeded/failed/not_verified、超限和持久化样本，见 `.local/parity/2026-10-17-0530/`）
- [ ] W308-05 follow-up、补充指令重试与恢复（2026-10-17 修复 Windows IPC 41→42；真实新 Run 只做一次 `project_read`、零 `project_write`，无旧 blocker，退出/最终构建重启恢复通过；完整 `resolve_task_followup`、租约、取消和未知副作用恢复仍待验收）
- [ ] W308-06 去重任务图摘要和历史引用（终态摘要去重自动化通过；完整 Windows 受影响回归待补）
- [ ] W308-07 `pluginKey + componentKey` 全链路选择/冻结（2026-10-17 已实现精确 pair schema、显式选择、Task component/Release 快照、漂移 fail-closed 与 executor 单 component 启动；Connector 27/27、Presentation 52/52、x64 Debug 构建通过，最终客户端空态/重启/退出清理通过。Solution Studio 尚未安装，双 component 选择、真实启动/调用/持久化/重启仍缺 action-time confirmation，故保持未勾选）
- [ ] W308-08 Solution Studio 方案创建器/任务执行器双 MCP（共享包已声明双 MCP；Windows component 全链路已实现，但真实安装及 authoring/execution 分离调用仍待确认与验收）
- [ ] W308-09 自定义 MCP/Plugin 本地/Git/npm 安装、真实预检与指纹（本地与固定 npm 管线保留；2026-10-18 01:00 新增无凭据 HTTPS Git + 明确 ref + 安全子目录的完整设置页/独立安装助手入口。Git 获取无 shell/无交互、禁 hooks/file protocol/子模块、Job 控制，预检后重新获取并拒绝内容或 tools-list 漂移；只读 `.git` 清理不跟随 reparse。定向 47/47、Connector 可执行集 691 通过/5 条件跳过、最终 Debug 0 警告错误。真实公开 fetch 曾成功，后续证据重跑因 `github.com:443` 不可达而阻塞；Computer Use 未提供，真实页面确认安装/目录恢复/重启、生产 AppContainer 与卸载恢复仍缺，故保持未勾选，见 `.local/parity/2026-10-18-0100/`）
- [ ] W308-10 独立安装助手和安装工具隔离（2026-10-17 已新增独立 `plugin-installer` 联系人/持久会话、不可变 installer capability、仅安装工具目录和每次调用的 Run+conversation 重授权；普通 Main Chat/Task 不含安装工具，安装会话隐藏普通 Plugin component 选择器并显示无项目编辑/通用终端权限的边界横幅。Presentation 867/867、installer/conversation/bootstrap/workspace/desktop 定向 55/55、共享 Host planner 隔离 1/1、最终 x64 Debug 0 警告/错误。真机切换、重启恢复及不存在路径的 `plugin_installer_inspect_source` 负向路由通过，未安装、未建目录且进程/sandbox 归零。21:01 已补 macOS 同等安全错误摘要：不存在来源目录等 `PluginPackageException` 返回可修正原因，任意原生异常和敏感标记仍隐藏；定向 34/34、Debug 0 警告错误、最终产物启动/正常退出通过。可见错误仍待真实 UI 复验，成功生产预检/安装仍受 W308-09/13 卷根前置阻塞，故保持未勾选，见 `.local/parity/2026-10-17-1048/`、`.local/parity/2026-10-17-2055/`）
- [ ] W308-11 主流 Agent Plugin/Skill/MCP 配置导入和明确拒绝（既有 ChatOS/Portable/Codex/Claude/独立 Skill 与主流 MCP JSON 归一化、真实 stdio `initialize/tools/list`、HTTP/SSE/UI/明文环境/Git/npm 拒绝，以及真实 Portable Skill 预检/Human 安装/指纹和残留证据保留。20:42 新修复重启目录只枚举 marketplace 的缺口：legacy `custom:` 和新来源记录均合并进设置/runtime catalog，准确 manifest 元数据与本地启用状态持久化，本地启停不写远端 preference，篡改包可见但 fail-closed，Plugin application 列表/启动同步支持；离开设置取消预检并清除旧指纹。定向 32/32、Connector 可执行子集 631 通过/5 条件跳过、Job 连续 5 次、最终 Debug 0 警告错误及两次启动/正常退出/包保留/残留归零通过。`node_repl/@oai/sky` 本轮不可用，真实设置页重启可见/启停/刷新、Main Chat Skill 选择、Claude 独立夹具和 Git/npm 仍缺，故保持未勾选，见 `.local/parity/2026-10-17-1140/`、`.local/parity/2026-10-17-1158/`、`.local/parity/2026-10-17-2025/`）
- [ ] W308-12 canonical `rea-agents` 包真实 stdio MCP 调用（2026-10-17 21:33 Windows 已实现固定版本 npm MCP 的无 shell 获取、禁 lifecycle scripts、精确包名/版本/bin 校验、真实 `initialize/tools/list`、二次获取指纹复检与持久化工具入口；专用安装助手新增 npm inspect/install schema，普通聊天/Task 仍不可见。真实 `rea-agents@6.5.0` 两次获取、临时安装后重开、207 工具及只读 `binary_session({})` 成功；修复 npm 树 8,192 本地来源上限误用和双 64 位摘要目录导致的 Node 深路径 package-scope 失败。定向 37/37、真实 REA 1/1、Debug x64 0 警告错误、最终窗口启动/正常退出及残留归零通过。生产 AppContainer/UI 安装、目录/Main Chat 选择、客户端重启和缺分析引擎诊断仍缺，故不勾选；见 `.local/parity/2026-10-17-2110/`）
- [ ] W308-13 Windows canonical path/reparse/ACL/Job/凭据安全安装矩阵（ACL 已改为精确不传播的 `RawSecurityDescriptor` + `SetFileSecurity` 更新，支持长路径、避开 `C:\Users`，并在 ACL 修改前 journal；创建失败/取消补偿、残留清理和 Job-controlled fixture 已通过。最终设置页生产预检真实启动 child 后快速、可见地受本机卷根 traverse 前置阻塞；正常关闭后 Desktop/Host/Connector/testhost/vstest 及 SandboxProfiles/SandboxTemp 均为 0。安装时最小权限方案、npm scripts、Git hooks/file、凭据与完整矩阵待闭环，见 `.local/parity/2026-10-17-0945/`）
- [ ] W308-14 Skill 门控、渐进披露和 schema 校验（schema preflight、disclosure v2 与 Plugin 精确 Skill gate 已通过；2026-10-18 00:16 又为外部 HTTP MCP 嵌入 canonical `chatos-external-mcp` Product Skill 及两份 reference，发布 schema-v2 冻结 bundle、capability revision/tool catalog 和 `on_demand` binding。绑定指纹覆盖资源/endpoint/工具/schema/远端 gate 而排除凭据 header；HTTP 前验证 bundle/binding/activation，目录漂移、缺激活、无可信远端业务 Skill 均 fail-closed，JSON-RPC 诊断保留。定向 20/20、受影响 52/52、Connector 可执行集 674 通过/5 条件跳过、Debug x64 0 警告错误，最终窗口正常退出且进程/sandbox roots 归零。真实 Plugin/HTTP MCP 成功、远端 Skill 拒绝、schema 分页、取消/错误、持久化和重启矩阵仍缺，故保持未勾选；见 `.local/parity/2026-10-17-2155/`、`.local/parity/2026-10-17-2240/`、`.local/parity/2026-10-18-0005/`）
- [ ] W308-15 不中断活跃任务的配置/凭据刷新（2026-10-17 21:50 已对齐 macOS 生命周期：IPC-only bootstrap Host 带 `--disable-workers`；credentialed worker 只保留 SHA-256 指纹，同 owner/同凭据刷新复用原 worker，模型/memory token 变化与跨 owner 刷新 fail-closed，停止后的正常启动才应用新凭据。生命周期/参数 7/7、合同 21/21、受影响定向 58/58、Connector 可执行子集 639 通过/5 条件跳过、Debug 0 警告错误。真实可控长 Run、历史冻结凭据、可见延后提示与重启应用仍待验收，故不勾选；见 `.local/parity/2026-10-17-2140/`）
- [ ] W308-16 附件与插件文件交付（主聊天附件旧主链已通过；2026-10-17 23:04 Windows 对齐 macOS Plugin Artifact 交付合同：强制 `artifact.create` 与项目 workspace，校验路径/reparse/MIME/大小/SHA-256 后复制到项目 `chatos-plugin-artifacts` 受管目录，并在工具结果首项注入精确 Markdown 链接/结构化交付描述；候选 schema 兼容旧 `mime_type`/Host 计算完整性且严格拒绝未知字段；Task、Agent Team、Relay 三入口均传真实项目根与权限快照，无项目或无权限 fail-closed。定向 17/17、受影响 165/165、Connector 可执行集 666 通过/5 条件跳过、Debug x64 0 警告错误、最终窗口正常退出且进程归零。真实插件文件链接点击、预览/另存、项目文件 UI、篡改/取消/重启仍因 Computer Use 缺失未验收，故保持未勾选；见 `.local/parity/2026-10-17-2255/`）
- [ ] W308-17 本地化、打包资源和运行时定位（2026-10-17 23:40 修复 x64 Release 在无 NASM 环境错误使用 Debug-only `AWS_LC_SYS_NO_ASM`、MSIX 工作目录误命中不可用 SDK，以及 Host 默认从当前目录监控配置导致独立发布启动失败；Release/Debug 均 0 警告错误，无签名 MSIX 真实生成并核对 466 项，包含 `chatos.runtime.json`、Local Agent Host、`resources.pri`、88 帧 Pet 和中英文 manifest。复制 publish 到独立证据目录、从 `C:\Windows\Temp` 启动后主窗口为 `ChatOS`、正常退出 0 且无残留。合同 17/17。签名安装/升级、packaged AUMID 启动、真实中英文切换、ARM64、干净账号和符号包仍待验收，故保持未勾选；见 `.local/parity/2026-10-17-2325/`）
# 2026-10-18 00:36 增量：W308-09 固定 npm 设置入口

- [ ] W308-09 设置来源入口（Settings 自定义导入卡现可在本地目录、HTTPS Git 与固定 npm 三种模式间切换；Git/npm 字段变化均取消进行中的预检并清除旧指纹。Git URL/ref/子目录、npm 精确包/bin/component/参数均结构化进入安全获取器。自动化和构建通过，但 Computer Use 的 `node_repl/@oai/sky` 未暴露，真实页面点击、确认安装、目录发现和重启恢复未验收；生产 AppContainer/卸载恢复仍缺，故不勾选。证据 `.local/parity/2026-10-18-0025/`、`.local/parity/2026-10-18-0100/`）

## 15. P0 空目录项目聊天硬门禁（2026-10-18）

- [ ] 在最终 x64 Debug ChatOS 的创建项目界面选择此前不存在、创建后 0 项且非 reparse point 的 `C:\ChatOS-ProjectChat-Acceptance-20261018-01`；当前仅磁盘前置通过，真实项目尚未创建，缺 project/conversation identity 与 UI 证据。
- [ ] 在同一项目聊天完成空列表、写审批允许/拒绝/取消、真实写入、读/搜唯一标记、修改/回读/diff、复制/移动/重命名、文件页返回、已有项目续聊和关闭重启恢复；全部仍待真实验收。
- [x] `project_move` 的实现与自动化：精确源/目标 schema、`CodeMaintainerWrite`、Host 审批、项目 context/path guard/reparse/ADS 边界和 capability revision 已接入；受影响权威子集 24/24，最终 x64 Debug 0 警告/错误且残留归零。此项只表示自动化通过，不表示 P0 真实验收通过。
- [x] `project_copy` 与真实 Host 工具过滤实现：共享 Host 现按 `CodeMaintainerWrite + requires_execution` 放行 `project_copy/project_move`；复制使用 staging 后切换、覆盖备份恢复、取消清理、目录 reparse 拒绝和双路径 Host 审批。Connector 权威子集 36/36、Host 1/1、最终 x64 Debug 0 警告/错误。此项仍只是实现/自动化通过。
- [ ] Computer Use `node_repl/@oai/sky` 恢复后重跑上述真实客户端矩阵；当前工具入口未暴露，严禁 PowerShell UIA、数据库/API 注入、已有项目或 mock 替代。证据 `.local/parity/2026-10-18-project-chat-real/acceptance-summary.md`。

2026-10-18 06:50 +08:00 新一轮复核：同任务其他轮次均 idle/notLoaded，产品/Host/Connector/测试/构建进程为 0；权威目录仍为 0 项、普通目录、非 reparse point，06:39 最终 x64 Debug 产物存在。工具目录仍无 `node_repl/@oai/sky`，因此没有启动或操作 UI，也没有使用数据库/API、fixture、mock、旧项目或替代 UIA。P0 所有未勾选项保持未通过，扩展项继续冻结。

- [x] 2026-10-18 07:08 P0 UI 可控性：创建项目 `ContentDialog` 的动态主按钮/取消按钮增加稳定 `ChatOS.CreateProject.Submit` / `ChatOS.CreateProject.Cancel` AutomationId 与中英文 accessible name；合同 2/2、Desktop x64 Debug 与最终 x64 Debug 构建均 0 警告/错误。此项仅为自动化准备，不表示真实空目录创建通过。
- [ ] 本轮工具目录仍无 `node_repl/@oai/sky`；真实空目录项目尚未创建/操作，project/workspace/conversation identity 与全部真实聊天矩阵仍缺。07:08 复核目录仍 0 项、普通目录、非 reparse；构建留下的 4 个正常 MSBuild 复用节点已由官方 shutdown 关闭，07:09 进程残留为 0、sandbox root 不存在；P1/P2 继续冻结。
## 2026-10-18 07:21 P0 approval UI automation readiness

- [x] Automation-only: approval source/risk/command/working-directory/reason/queue fields have stable IDs; a newly active approval focuses the safe Deny action. Contract tests 2/2 and final x64 Debug build passed.
- [ ] Real acceptance: `C:\ChatOS-ProjectChat-Acceptance-20261018-01` is still 0-entry/non-reparse and has **not** been selected in Create Project; no real project/conversation/tool/approval/file/diff/restart evidence exists. P1/P2/W308 remain frozen.

## 2026-10-18 07:38 P0 directory verification

- [x] Automation-only: stale-path creation after a directory refresh failure is rejected until a successful refresh; the dialog exposes exact path and actual entry counts. Create Project 34/34, Desktop automation 29/29, final x64 Debug 0 warnings/errors.
- [ ] Real acceptance: the authoritative directory is still not selected in the final client, so no project/conversation identity or real chat/file/restart evidence exists. Extension work remains frozen.

## 2026-10-18 07:53 P0 project conversation binding

- [x] Automation-only: first project-chat entry per authenticated lifecycle revalidates remote/stale conversation identity against the Local Agent Host's exact project resource binding; blank identities fail closed and logout/account change clears verified cache. Shell 22/22, combined binding/automation 52/52, final x64 Debug 0 warnings/errors.
- [x] Automation-only: `ChatOS.Workspace.Root` exposes selected project and opened conversation identities through UI Automation `ItemStatus` / `HelpText` for the later real run.
- [ ] Real acceptance: the empty directory is still unused; no final-client project, real Host chat/tool operation, approval decision, disk/diff, navigation/restart or error-matrix evidence exists. P1/P2/W308 remain frozen.

## 2026-10-18 08:10 P0 workspace UIA state

- [x] Automation-only: `ChatOS.Workspace.Root` exposes workspace path, project identity and conversation identity separately; directory/chat/files/git/run buttons publish deterministic `selected` / `not-selected` UIA state. Desktop automation 31/31 and final x64 Debug 0 warnings/errors.
- [ ] Real acceptance: Computer Use is still unavailable; the authoritative directory remains unused, 0-entry and non-reparse. No real final-client identity/tree, Host tool, approval, file/diff, page round-trip, restart or failure-matrix evidence exists. P1/P2/W308 remain frozen.

## 2026-10-18 08:22 P0 project-files UIA state

- [x] Automation-only: project files expose project/root/current directory plus stable list/search/loading, selected-file path/content, editor, action-status and error IDs. Desktop automation 32/32 and final x64 Debug 0 warnings/errors.
- [ ] Real acceptance: the empty directory is still unused because Computer Use is unavailable. No real final-client project/chat/tool/approval/file/diff/page-round-trip/restart/error evidence exists; P1/P2/W308 remain frozen.

## 2026-10-18 08:37 P0 project-chat UIA evidence

- [x] Implementation/automation only: conversation root identity/state, per-turn/user/guidance/assistant message IDs, live/history process IDs, callback message/task/run/status and chat empty/error/composer regions are now observable. Presentation 63/63, Desktop automation 33/33 and final x64 Debug build passed.
- [ ] Real empty-directory project creation and all real Local Agent Host list/read/search/write/modify/diff/copy/move/rename, approval allow/deny/cancel, files/chat, restart and failure-matrix operations remain unaccepted. The directory is still 0-entry/non-reparse; P1/P2 remain frozen.

## 2026-10-18 08:51 P0 project Git/diff UIA evidence

- [x] Automation-only: Project Git publishes stable project/workspace/repository, branch/sync, change-list, selected diff path/content, empty-state, action-result and error UIA properties; bilingual accessible status labels are included. Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance: the authoritative directory is still unused, empty and non-reparse because the required Computer Use entry is unavailable. Real creation, project/conversation binding, chat tools, approval decisions, disk/diff, navigation, restart and error matrix remain unaccepted; P1/P2 stay frozen.

## 2026-10-18 09:25 P0 project-chat idempotent retry

- [x] Implementation/automation only: an exact retry after an ambiguous Host failure now reuses the original turn, frozen message/run/start payload and stable Host IPC receipt identity; changed retry input creates a new turn or fails closed on identity drift. Connector/Host 16/16, project-chat Presentation 59/59 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the authoritative empty directory has not been selected in the final client, and real timeout/retry-without-duplicate-side-effect plus the rest of the P0 matrix has not run. P1/P2/W308 remain frozen.

## 2026-10-18 09:07 P0 project-files empty-directory evidence

- [x] Implementation/automation only: `ChatOS.ProjectFiles.DirectoryEmpty` now distinguishes a completed successful zero-entry listing from loading, error, search and closed states and exposes the exact current path. Project-files Presentation 141/141, Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the final client has not selected the authoritative empty directory and the real project/chat/Host/tool/approval/disk/diff/navigation/restart/error matrix has not run. P1/P2/W308 remain frozen.

## 2026-10-18 09:37 P0 cancelled-approval cleanup

- [x] Implementation/automation only: while project write/copy/move approval is pending, Windows now verifies the exact owner-scoped Run remains `waiting_tool_result`. Stop/cancel or unverifiable Host identity cancels and removes the local approval fail-closed; no stale decision or post-cancel project-file side effect is allowed. Focused regressions 33/33 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the authoritative empty directory has not been selected in the final client, and approval allow/deny/cancel plus the complete P0 matrix remains unexecuted. Evidence: `.local/parity/2026-10-18-project-chat-real/17-project-chat-cancelled-approval.md`. P1/P2/W308 remain frozen.

## 2026-10-18 09:51 P0 project-conversation ambiguity guard

- [x] Implementation/automation only: project-chat restore now reuses exactly one owner/project-bound Host conversation, creates only when none exists, and fails closed before creation if duplicate bindings make the identity ambiguous. The create-conflict recovery follows the same exact-one rule. Workspace/Shell regressions 26/26 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the final client has not selected the authoritative empty directory and the real identity/tool/approval/disk/diff/navigation/restart/error matrix remains unexecuted. Evidence: `.local/parity/2026-10-18-project-chat-real/18-project-conversation-ambiguity-guard.md`. P1/P2/W308 remain frozen.

## 2026-10-18 10:07 P0 project-conversation response identity guard

- [x] Implementation/automation only: owner-scoped conversation pages reject foreign owners, missing/duplicate conversation identities and repeated cursors; create responses must match the exact requested owner, conversation identity and resource binding. Workspace/Shell regressions 32/32 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the final client has not selected the authoritative 0-entry non-reparse directory. Real identity, Host tools, approval, disk/diff, navigation, restart and failure matrix remain unexecuted. Evidence: `.local/parity/2026-10-18-project-chat-real/19-project-conversation-response-identity.md`. P1/P2/W308 remain frozen.

## 2026-10-18 10:25 P0 project workspace failure contract

- [x] Implementation/automation only: workspace path/file validation now carries stable path-free code/status into project-chat tool results. Deterministic pre-commit rejections are `failed` and safe to correct/retry; arbitrary side-effecting I/O failures remain redacted `needs_review`. Focused 43/43, Connector 715 passed/5 conditional skips/0 failures after excluding only three named symbol-link privilege fixtures, final x64 Debug 0 warnings/errors.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the authoritative empty directory is still unused and 0-entry/non-reparse. The real path/permission/reparse/conflict/cancel/timeout/retry matrix and all create/identity/Host/approval/disk/diff/navigation/restart evidence remain unexecuted. Evidence: `.local/parity/2026-10-18-project-chat-real/20-project-workspace-error-results.md`. P1/P2/W308 remain frozen.

## 2026-10-18 10:35 P0 retry status

- [ ] Real final-client creation from `C:\ChatOS-ProjectChat-Acceptance-20261018-01`: **not run**. Fresh check: 0 entries, ordinary directory, non-reparse; Computer Use callable `node_repl/@oai/sky` still absent.
- [ ] Real project-chat list/read/search/write/approval/diff/navigation/restart/error matrix: **not run**. No alternate UIA, database/API, fixture/mock, existing project, unit test, or build was substituted.
- [x] Fresh mutual-exclusion and residue audit: only this run active; Desktop/Host/Connector/build/test/Rust process count 0. This is diagnostic evidence only, not acceptance.
- Evidence: `.local/parity/2026-10-18-project-chat-real/21-computer-use-retry-audit.md`. P1/P2 remain frozen.

## 2026-10-18 10:50 P0 timeout/cancellation audit

- [x] Implementation audit only: exact-Run approval cancellation, 30-second claim/10-second renewal, canceled-claim no-commit behavior, copy staging cleanup, deterministic pre-commit write/move validation, and unknown-side-effect `needs_review` handling were rechecked; no new reproducible defect was found, so no speculative edit/build/test was performed.
- [ ] Real acceptance remains blocked: Computer Use still has no callable `node_repl/@oai/sky`; `C:\ChatOS-ProjectChat-Acceptance-20261018-01` remains unused, 0-entry and non-reparse. Real create/identity/Host/approval/disk/diff/navigation/restart/timeout-retry evidence is absent. Evidence: `.local/parity/2026-10-18-project-chat-real/22-project-tool-timeout-audit.md`. P1/P2/W308 remain frozen.

## 2026-10-18 11:05 P0 Computer Use gate recheck

- [ ] Real final-client creation remains blocked: after reading the full Computer Use skill/API/confirmation material, the callable catalog still had no `node_repl/@oai/sky`; no substitute UIA or data injection was used.
- [x] Diagnostic-only refresh: the authoritative directory is still 0-entry, ordinary `Directory`, non-reparse; final x64 Debug exists; Git lock, competing run and product/Host/build/test/Rust residue are absent. Evidence: `.local/parity/2026-10-18-project-chat-real/23-computer-use-gate-recheck.md`.
- [ ] P0 still lacks every real identity, Host tool, approval, disk/diff, navigation, restart and failure/retry artifact. P1/P2/W308 remain frozen.

## 2026-10-18 11:23 P0 project-search completeness

- [x] `project_search` no longer silently turns locked/unreadable/enumeration-failed content into an exhaustive zero-match result; it reports `incomplete=true` and `truncated=true`, and the v12 tool contract forbids an absence claim from such a result.
- [x] Focused authoritative subset 39/39; final x64 Debug 0 warnings/errors; two broad-run failures are the named CreateSymbolicLink privilege prerequisites before product execution. Evidence: `.local/parity/2026-10-18-project-chat-real/24-project-search-completeness.md`.
- [ ] Real acceptance remains blocked: the exact empty directory is still unused/0-entry/non-reparse because Computer Use is unavailable; real identity, Host tool, approval, disk/diff, navigation, restart and failure/retry artifacts remain absent. P1/P2/W308 stay frozen.

## 2026-10-18 11:36 P0 project-list reparse completeness

- [x] Implementation/automation: `project_list` now reports `incomplete=true` plus `omitted_reparse_points` when safe listing withholds junction/symbolic-link entries; its model contract forbids treating such output as exhaustive or as an empty-directory proof. Capability revision is `native-windows-main-chat-v13-project-list-completeness`. Focused authoritative subset 39/39 and final x64 Debug 0 warnings/errors passed; the positive symlink fixture is still blocked by this machine's missing creation privilege.
- [ ] Real acceptance remains blocked: Computer Use still exposes no callable `node_repl/@oai/sky`; the exact acceptance directory remains unused/0-entry/non-reparse. No real project identity, Host tool, approval, disk/diff, navigation, restart or reparse/error-matrix artifact exists. Evidence: `.local/parity/2026-10-18-project-chat-real/25-project-list-reparse-completeness.md`; P1/P2/W308 remain frozen.

## 2026-10-18 11:52 P0 project-read invalid UTF-8 truthfulness

- [x] Implementation/automation: invalid UTF-8 can no longer be exposed or searched as replacement-decoded text. `project_read` returns exact base64 with `is_binary=true`; content search excludes the binary bytes. Authoritative subset 19/19 and final x64 Debug 0 warnings/errors passed; two full-class failures are only the known symbolic-link fixture privilege prerequisites.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the exact directory is unused/0-entry/non-reparse. Real project/conversation identity, Host operations, approval branches, disk/diff, navigation, restart and failure/retry evidence remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/26-project-read-invalid-utf8.md`; P1/P2/W308 stay frozen.

## 2026-10-18 12:08 P0 project-write diff result

- [x] Implementation/automation: atomic `project_write` results include before/after SHA-256, changed/available/truncated state and a bounded unified diff; create uses `/dev/null`, no-op writes expose an empty diff, and invalid/oversized/unreadable previous files expose a stable unavailable reason. Capability revision is `native-windows-main-chat-v14-project-write-diff`. Authoritative subset 43/43 and final x64 Debug 0 warnings/errors passed; two broad failures remain only the named symbolic-link fixture privilege prerequisites.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the authoritative directory is still unused/0-entry/non-reparse. Real project/conversation identity, Host write approval branches, disk read-back, UI-visible diff, navigation, restart and failure/retry evidence remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/27-project-write-diff-result.md`; P1/P2/W308 stay frozen.

## 2026-10-18 12:24 P0 project-search reparse completeness

- [x] Implementation/automation: name and content search now set `incomplete=true` and `truncated=true` when a junction/symbolic-link entry is withheld, so the model cannot turn a safe no-traversal result into a false exhaustive absence claim. Capability revision is `native-windows-main-chat-v15-project-search-reparse-completeness`. Non-machine-dependent regressions passed 43/43 and final x64 Debug passed with 0 warnings/errors; the direct symlink fixture remains blocked before product code by the known account privilege prerequisite.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the authoritative directory is still unused/0-entry/non-reparse. Real project/conversation identity, Host tools, approval branches, disk/UI diff, navigation, restart and boundary/failure/retry evidence remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/28-project-search-reparse-completeness.md`; P1/P2/W308 stay frozen.

## 2026-10-18 12:37 P0 project_move case-only rename

- [x] Implementation/automation: Windows file and directory case-only renames no longer return a false no-op. `project_move` uses a rollback-safe same-directory intermediate hop; exact same-path retries remain side-effect-free. Capability revision is `native-windows-main-chat-v16-project-move-case-rename`. Filesystem/control-plane regressions passed 44/44 and final x64 Debug passed with 0 warnings/errors.
- [ ] Real acceptance remains blocked: Computer Use has no callable `node_repl/@oai/sky`; the authoritative directory is still unused/0-entry/non-reparse. Real project/conversation identity, Host tools, approval branches, on-disk copy/move/rename, disk/UI diff, navigation, restart and boundary/failure/retry evidence remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/29-project-move-case-only-rename.md`; P1/P2/W308 stay frozen.

## 2026-10-18 P0 project-mutation approval identity

- [x] Implemented/automation passed: project writes now show and bind create-only mode, exact UTF-8 byte count and content SHA-256; project copy/move shows replacement intent, with replacement-enabled operations elevated to high risk and explicit overwrite wording. Approval/Control Plane regressions passed 41/41; final x64 Debug passed with 0 warnings/errors.
- [ ] Real acceptance remains blocked: no callable Computer Use `node_repl/@oai/sky`; the authoritative empty directory remains unused/0-entry/non-reparse. Real allow/deny/cancel cards, exact project/workspace/conversation identities, Host file operations, disk/UI diff, navigation, restart and full failure matrix are unexecuted. Evidence: `.local/parity/2026-10-18-project-chat-real/30-project-mutation-approval-identity.md`. P1/P2/W308 remain frozen.

## 2026-10-18 13:09 P0 project_write empty-content contract

- [x] Implemented/automation passed: a present empty or whitespace-only `project_write.content` is now accepted consistently by approval and execution, preserved exactly, and bound to its exact UTF-8 length/SHA-256. Missing or non-string content still fails closed. Capability revision is `native-windows-main-chat-v17-project-write-empty-content`; affected regressions passed 66/66 and final x64 Debug passed with 0 warnings/errors.
- [ ] Real acceptance remains blocked: no callable Computer Use `node_repl/@oai/sky`; the authoritative empty directory remains unused/0-entry/non-reparse. Real project/workspace/conversation identities, Host list/read/search/write, allow/deny/cancel, disk/UI diff, copy/move/rename, navigation, restart and failure/retry evidence are absent. Evidence: `.local/parity/2026-10-18-project-chat-real/31-project-write-empty-content.md`. P1/P2/W308 remain frozen.

## 2026-10-18 13:25 P0 Create Project listing identity

- [x] Implemented/automation passed: a usable Create Project listing must match the selected workspace, exact requested absolute path and derived relative project root; stale/outside/mismatched responses fail closed and cannot enable Create. Focused regressions passed 36/36 and final x64 Debug passed with 0 warnings/errors.
- [ ] Real acceptance remains blocked: no callable Computer Use `node_repl/@oai/sky`; the authoritative empty directory remains unused/0-entry/non-reparse. Real project/workspace/conversation identities, Host operations, approval allow/deny/cancel, disk/UI diff, navigation, restart and the failure/retry matrix remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/32-create-project-listing-identity.md`. P1/P2/W308 remain frozen.

## 2026-10-18 13:38 P0 Create Project folder-item automation identity

- [x] Implemented/automation passed: each actionable Create Project folder row exposes its bound directory name and exact path through UI Automation, anchored by `ChatOS.CreateProject.Folders`. Focused contract 1/1, Desktop automation 34/34 and final x64 Debug 0 warnings/errors passed.
- [ ] Real acceptance remains blocked: no callable Computer Use `node_repl/@oai/sky`; the authoritative empty directory remains unused/0-entry/non-reparse. Real project/workspace/conversation identities, Host operations, approval allow/deny/cancel, disk/UI diff, navigation, restart and the failure/retry matrix remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/33-create-project-folder-item-automation.md`. P1/P2/W308 remain frozen.

## 2026-10-18 13:53 P0 Create Project submit-time directory revalidation

- [x] Implemented/automation passed: submit force-refreshes the selected directory before registration, rechecks exact workspace/absolute path/relative root, publishes the latest entry counts, and cancels without a create side effect on refresh failure, identity drift or dialog disposal. Focused tests passed 40/40, Presentation passed 878/878 and final x64 Debug passed with 0 warnings/errors.
- [ ] Real acceptance remains blocked: no callable Computer Use `node_repl/@oai/sky`; the authoritative empty directory remains unused/0-entry/non-reparse. Real project/workspace/conversation identities, Host operations, approval allow/deny/cancel, disk/UI diff, navigation, restart and the failure/retry matrix remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/34-create-project-submit-revalidation.md`. P1/P2/W308 remain frozen.

## 2026-10-18 14:08 P0 Create Project failure/empty-state separation

- [x] Implemented/automation passed: the Create Project empty-directory panel requires a successful usable listing; loading, no-permission, Host and other listing failures stay unverified/error and cannot appear empty. Desktop automation passed 34/34, focused Create Project Presentation passed 40/40, and final x64 Debug passed with 0 warnings/errors.
- [ ] Real acceptance remains blocked: no callable Computer Use `node_repl/@oai/sky`; the authoritative empty directory remains unused/0-entry/non-reparse. Real project/workspace/conversation identities, Host operations, approval allow/deny/cancel, disk/UI diff, navigation, restart and the failure/retry matrix remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/35-create-project-failure-empty-state.md`. P1/P2/W308 remain frozen.

### 2026-10-18 14:19 P0 status clarification

- [ ] Mandatory real acceptance remains 0 passed: final-client selection of the authoritative new empty directory and every downstream project-chat operation are still unexecuted because no callable Computer Use `node_repl/@oai/sky` is exposed.
- [x] Status-only audit preserved all existing changes and used no alternate UIA/database shortcut; same-task predecessors are idle/notLoaded. P1/P2/W308 remain frozen.

### 2026-10-18 14:38 P0 Computer Use native pipe gate

- [x] The repaired runtime now exposes callable `node_repl` and imports `@oai/sky`; initial, delayed-retry and reset/reinitialize `list_apps()` calls all reached the runtime but failed with native-pipe `os error 2`.
- [x] Diagnostic proof shows valid current runtime paths and a signed helper, but no helper process and no `codex-computer-use-*` named pipe. Per the Computer Use skill, no manual helper launch or alternate UIA was used.
- [ ] Mandatory real acceptance remains 0 passed: the authoritative directory is still unused/0-entry/non-reparse, no ChatOS UI was controlled, and all real identities, Host operations, approval branches, disk/UI diff, navigation, restart and failure/retry evidence remain absent. Evidence: `.local/parity/2026-10-18-project-chat-real/36-computer-use-native-pipe-unavailable.md`. P1/P2/W308 remain frozen.

### 2026-10-18 19:51 P0 real Project Files and approval UI repair

- [x] Real final-client Project Files revalidation: the overlapping flat ListView/TreeView defect is repaired; the same acceptance project now renders one TreeView only, with each of the four real disk files appearing once and an actionable `ChatOS.ProjectFiles.TreeEntry`.
- [x] Real approval allow-once revalidation: the approval overlay moved away from the chat composer, all three labels were fully readable, and the user-authorized `ChatOS.Approval.AllowOnce` action completed exactly once. The chat callback and disk both contain the exact 40-byte marker with matching SHA-256.
- [x] Desktop automation passed 35/35 and selected diff check passed; the preceding final x64 Debug build passed with 0 warnings/errors. Evidence: `.local/parity/2026-10-18-project-chat-real/37-project-files-approval-ui-real.md`.
- [ ] P0 is still not complete: Stop/cancel cascade failed earlier, and real copy/move/rename plus permission/path/UNC/ADS/reparse/missing/conflict/timeout/retry/persistence/restart/final-cleanup coverage remains open. P1/P2/W308 stay frozen.

### 2026-10-18 20:49 P0 real Stop cancellation

- [x] Real pre-Task Stop: the same project chat cancelled its exact Turn/Run before Task graph or tool creation; persisted event order is started → claimed → cancelled, tool/graph/Task counts are all zero, no approval appeared and the requested target file is absent.
- [x] Real active-child Stop: when a child Task and Task Run already existed, one Stop cancelled main Run, Task and child Run at the same timestamp, cleared `active_run_id`, recorded `user requested stop`, and allowed no child tool invocation or target-file side effect.
- [x] Pet/main UI evidence, screenshot hashes, read-only database timelines/counts, disk absence and clean shutdown are recorded in `.local/parity/2026-10-18-project-chat-real/38-project-chat-stop-real.md`.
- [ ] P0 remains incomplete: copy/move/rename, permission/absolute/drive/UNC/ADS/reparse/missing/conflict, timeout/retry/idempotency, remaining persistence/restart and final cleanup remain open. P1/P2/W308 stay frozen.

### 2026-10-18 21:10 P0 real copy/move/rename

- [x] Real non-overwriting copy: the same project chat showed the exact `project_copy p0-approval-buttons-20261018-2005.txt p0-copy-real-20261018-2100.txt --replace-existing=false` approval; one allow-once decision completed the succeeded Task Run and produced a byte-identical file.
- [x] Real non-overwriting same-directory move/rename: the exact `project_move p0-copy-real-20261018-2100.txt p0-renamed-real-20261018-2100.txt --replace-existing=false` approval completed once; the intermediate path is absent and the final file preserves the 40-byte content and SHA-256.
- [x] The repaired attachment → Stop → Send → composer layout remained visible and unobscured by the Pet window. Desktop automation passed 35/35, final x64 Debug passed with 0 warnings/errors, and selected diff check passed. Evidence: `.local/parity/2026-10-18-project-chat-real/39-project-chat-copy-move-send-ui-real.md`.
- [ ] P0 remains incomplete: permission and path boundaries, approval deny/cancel, reparse/missing/conflict, timeout/failure retry and idempotency, remaining persistence/restart, affected regressions and final cleanup remain open. No delete was requested or performed; P1/P2/W308 stay frozen.

### 2026-10-18 21:30 P0 real read-only path-boundary matrix

- [x] In the same final-client project/conversation, six separate real `project_read` invocations exercised traversal, absolute path, alternate drive, UNC, ADS and missing in-project entry handling.
- [x] Traversal/absolute/drive/UNC/ADS each returned HTTP 400 `workspace_path_unsafe`; the missing entry returned HTTP 404 `workspace_entry_missing`. All invocations were non-side-effecting, required no approval, and the Task/Run/Turn succeeded.
- [x] Sanitized UI evidence, exact invocation identities, cleanup and screenshot SHA-256 are in `.local/parity/2026-10-18-project-chat-real/40-project-chat-boundary-read-real.md`. Normal close left all product/build/test processes at zero and sandbox roots absent.
- [ ] P0 remains incomplete: no-permission, reparse, conflict, strict approval deny versus cancel, timeout/failure retry without duplicate side effects, remaining persistence/restart, affected regressions and final cleanup remain open. P1/P2/W308 stay frozen.

### 2026-10-18 22:17 P0 real conflict and approval cancellation

- [x] Real non-overwrite conflict: exactly one approved `project_copy` with `replace_existing=false` returned 409 `workspace_copy_target_exists`; both 40-byte files retained the same content and SHA-256.
- [x] Real approval-waiting cancellation: the Task Graph cancel action changed Task/Run/callback/node to `cancelled`, cleared the approval UI, disabled the cancel action and left the target file absent. No approve/reject button and no ordinary Stop action was used.
- [x] Read-only persistence proof records one failed `project_write`, no approval decision actor/time, cleared pending batch, terminal cancellation events and no duplicate side effect. Evidence is in `.local/parity/2026-10-18-project-chat-real/41-project-chat-conflict-real.md` and `43-project-chat-approval-cancel-real.md`.
- [ ] P0 remains incomplete: strict approval decline, no-permission, reparse, timeout/failure retry without duplicate side effects, remaining persistence/restart, affected regressions and final cleanup remain open. P1/P2/W308 stay frozen.

### 2026-10-18 22:40 P0 real strict approval rejection and restart

- [x] Real strict rejection: the final client displayed the exact create-only/37-byte/SHA-256-bound `project_write` approval; `ChatOS.Approval.Deny` was clicked once. Persistence records exactly one failed write with `approval_status=rejected`, decision actor `windows-local-approval` and `The user declined this operation.`
- [x] No duplicate side effect: the rejected target remains absent, no retry or alternate project mutation occurred, and the workspace retained only its five prior files.
- [x] Restart persistence: normal close and final-Debug restart restored the same project/conversation and the same rejected callback. A second normal close left Desktop/Host/Connector/build/test process count at zero and both sandbox roots absent. Evidence: `.local/parity/2026-10-18-project-chat-real/44-project-chat-approval-deny-real.md`.
- [ ] P0 remains incomplete: no-permission, reparse, timeout/failure retry without duplicate side effects, remaining persistence/regression coverage and final full-matrix cleanup remain open. P1/P2/W308 stay frozen.

### 2026-10-19 00:28 P0 real Host timeout/exact retry/callback persistence

- [x] Real timeout attempt while Host was suspended returned `A task was canceled.`, retained the exact request and produced no persisted Turn/graph/Task/tool/file side effect.
- [x] Exact retry in the same project/conversation produced one Turn/graph/Task/Run, one approved `project_write`, one `project_read`, zero alternate mutations, exact 37-byte disk content/SHA-256 and a correct create diff.
- [x] Repaired succeeded-callback sanitization so negated timeout/failure wording is preserved instead of becoming “服务暂时不可用”; storage 109/109 and final x64 Debug 0 warnings/errors passed.
- [x] Normal close/restart restored project `9547b0a2-2d67-42ae-bf61-5874847d8cc6`, conversation `conversation_9dcf8dc35a7644d8a8c7391a78c6a326` and the same successful callback. Evidence: `.local/parity/2026-10-18-project-chat-real/46-project-chat-timeout-retry-real.md`.
- [ ] P0 is not complete: macOS Main Chat/Product Skill/internal-bundle/Task-invariant parity, remaining Host-not-ready/loading/permission/reparse cases, affected regressions and final full-matrix cleanup remain open. P1/P2/W308 stay frozen.

