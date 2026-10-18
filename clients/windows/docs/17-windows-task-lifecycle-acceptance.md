# Windows Task 生命周期验收

## 本轮范围

2026-10-16 使用测试账号的真实联系人和项目会话，验证单 Task 和多节点 DAG 创建、running/waiting/pending 回执、Ask User 等待解锁、任务图详情、实时取消与级联、失败/重试、61 条真实事件分页、Host 断流恢复、画布式缩放/连线布局、项目目录边界与单次写审批、历史持久化和完整重启恢复。macOS 参考为消息任务回执、Task Graph/详情、Ask User 原地提示、`MessageTaskGraphCanvas`/`Layout` 和 `NativeLocalAgentEventHub`；项目权限窄闭环已覆盖，但模型绑定、审批组合和补充指令重试仍未完整覆盖。

## 真实流程与结果

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 模型调用 `create_task` | 通过；Task `WIN-TASK-CANCEL-20261016-0745` 进入 running | `03-task-created.png`、`db-before-fix.json` |
| Ask User 等待 | 通过；出现 `WIN-ASK-20261016-0745` 和 Continue/Stop 请求 | `03-task-created.png`、`03-task-created-tree.json` |
| 任务图详情 | 通过；标题、目标、Run ID、运行事件和取消入口可见 | `16-fixed-task-graph-running.png` |
| 实时取消 | 通过；回执/详情切换 cancelled，追加 `run_cancelled`，Ask User 清理 | `17-task-cancel-clicked.png`、`18-cancelled-timeline.png` |
| 持久化 | 通过；Task cancelled/version 4，Run cancelled/version 5，原因精确保存 | `db-after-cancel.json` |
| 正常退出与重启 | 通过；Desktop/Host 归零，重启恢复同一终态、事件和操作状态 | `21-cancelled-exit-result.json`、`24-restart-cancelled-graph.png` |
| 最终构建再回归 | 通过；取消禁用、重试可用，事件链完整 | `32-final-source-cancelled-graph.png` |
| 真实失败展示 | 通过；`WIN-TASK-FAIL-RETRY-20261016-0915` 显示 provider 400、`run_failed` 和可用重试 | `2026-10-16-0900/05-failed-graph.png` |
| 失败后重试 | 通过；第一次 Run `task-run-da8af40b-...`，点击后新建 `task-run-2023aeec-...`，同一无效 minimal 配置再次权威失败 | `2026-10-16-0900/07-retry-terminal.png`、`task-db-before-restart.json` |
| 取消后成功重试 | 通过；原 Run cancelled，重试 Run `task-run-729d7a90-...` 重新进入 Ask User 并返回 `RETRY-SUCCESS-OK-20261016-0920` | `2026-10-16-0900/12-cancelled-for-retry.png`、`15-retried-prompt.png`、`16-retry-succeeded.png` |
| Host 断流恢复 | 通过；强杀后新 Host 自动拉起；超过 75 秒暂停时旧 Host 被回收并重建，同一 waiting_user Task/Run 始终保留且恢复后可取消 | `2026-10-16-0900/20-realtime-disconnected.png`、`22-realtime-timeout.png`、`24-realtime-task-cancelled.png` |
| 新失败/重试重启恢复 | 通过；正常退出 Desktop/Host 均归零，重启恢复两次 failed 回执、成功回执和最新失败 Run/错误/重试入口 | `2026-10-16-0900/26-normal-exit-result.json`、`28-restart-persisted.png`、`31-restart-failed-graph.png` |
| 项目绑定与目录边界 | 通过；真实项目内读取成功，`../README.md` 以 `Local platform tool failed.` 拒绝 | `2026-10-16-1500/task-boundary-db.json`、`16-final-boundary-approval.png` |
| 项目写审批与回读 | 通过；审批卡绑定正确项目根，“本次允许”后创建并回读精确标记，Task/Run succeeded | `2026-10-16-1500/18-final-task-completed.png`、`21-final-task-graph.png`、`task-boundary-db.json` |
| 项目 Task 重启恢复 | 通过；正常退出 Desktop/Host 归零，最终构建重启恢复认证、项目聊天、Task/Run、审批事件、标记和 Host | `2026-10-16-1500/23-pre-test-exit-result.json`、`27-final-restart-task-graph.png`、`final-restart-assertions.json` |

Task ID 为 `local-task-cc4cfac8-4605-46de-bd63-3e8900aaac47-1`，Run ID 为 `task-run-9d252dcb-fafc-4bd3-bb2b-df864dfed7c2`。测试任务按验收意图保留为 cancelled，不重试、不删除。

## 修复与自动化

Windows 任务服务现在在消息专属 Task Graph 返回空节点时回退到同 turn 的持久化图；展示、取消和本轮补齐的重试都采用相同回退，避免短暂空图把已存在任务误报为不存在。Connector 专项 4 项、Task Graph Presentation 专项 109 项通过。

本地共享事件 Hub 与 macOS 一致：Host 请求异常不会结束上层 stream，而是在 Hub 内每 1 秒重试并由生命周期重建 Host。因此强制断流不会稳定出现通用 `RealtimeErrorMessage`，本轮没有把瞬时错误提示伪造成实机证据；通用 realtime 枚举真的结束或抛错时切回 2 秒轮询的行为由专项自动化覆盖。真实 Host 强杀与超时回收均保持同一 Task/Run，恢复后权威读取和取消成功。

本轮最终常规测试为 Core 27 + API 46 + Presentation 848 + Connector 549 + NetworkGuard 19，共 1,489 通过、0 失败；package、TRX 和 acceptance evidence 校验器通过。SDK 8.0.426 下 Rust Host/x64 Debug 最终构建 0 警告、0 错误。

## 证据边界与剩余项

证据目录为 `.local/parity/2026-10-16-0745/`。辅助脚本最初未滚到底部，`04/05` 打开的是旧轮次任务图，因此只证明旧轮次为空，不作为当前 Task 的缺陷或修复证据；`15` 后已明确滚动到底部，最终引用 `16/17/18/24/32`。

Ask User 选项提交、敏感字段策略和 Pet 遮挡已在后续实机轮次闭环，见 `docs/19-windows-ask-user-acceptance.md`。失败/重试与 Host 断流证据为 `.local/parity/2026-10-16-0900/`；多节点依赖证据为 `.local/parity/2026-10-16-0945/`；61 条真实事件分页证据为 `.local/parity/2026-10-16-1015/`；响应式窄窗工作区证据为 `.local/parity/2026-10-16-1300/`；可缩放连线画布证据为 `.local/parity/2026-10-16-1330/`；项目边界与写审批证据为 `.local/parity/2026-10-16-1500/`。仍未通过：模型绑定、拒绝/会话允许等完整项目权限组合，以及共享 Host 的补充指令重试。

## 2026-10-16 15:00–15:27：项目边界、写审批与共享重试阻塞

- 真实项目会话绑定 `C:\project\chatos-repository-manager`。最终 Task `WIN-TASK-PROJECT-BOUNDARY-FINAL-20261016-1520` 读取 `REPOSITORY.md` 得到 `resolved repository version`，而 `../README.md` 被平台边界拒绝，精确错误为 `Local platform tool failed.`。
- `project_write` 审批卡显示正确项目根；“本次允许”后仅创建 `docs/TASK-PROJECT-BOUNDARY-OK-20261016-1520.txt`，回读内容为 `TASK-PROJECT-BOUNDARY-OK-20261016-1520\n`。数据库 invocation 显示 `requires_approval=1`、`approval_status=approved`、`approval_decided_by=windows-local-approval`，Task/Run 均 succeeded。
- 正常退出后 Desktop/Host 为 0；最终构建重启恢复认证、项目聊天、成功任务图、审批事件、磁盘标记和 Host。Connector 102 + Presentation 111 专项通过；常规 1,497 项全通过，Rust Host/x64 Debug 构建 0 警告、0 错误。本轮没有修改 Windows 生产源码。
- 失败样本 `WIN-TASK-PROJECT-BOUNDARY-20261016-1505` 揭示共享 Host 缺陷：新重试 Run 已包含 `retry_instructions`，执行模型却仍收到原始 `prompt` 并重放旧写路径。精确位置为 `clients/local_agent_host/layers/infrastructure/database/src/task_retry_input.rs` 和 `clients/local_agent_host/layers/profiles/src/planner.rs`。这两个文件不属于本 Windows 自动化的允许编辑范围，未修改；因此不能把重试补充指令、项目权限综合项或任务图综合项标为完成。

## 2026-10-16 13:30–14:12：可缩放连线画布与窄窗控制

- 对照 macOS `MessageTaskGraphCanvas` 与 `MessageTaskGraphLayout`，Windows 将左侧线性节点列表替换为原生 Canvas：232×142 DIP 卡片按深度分列，前置关系绘制贝塞尔曲线，选中节点高亮并继续驱动右侧 Inspector；视口支持 25%–200% 缩放、Windows 原生滚动平移、100% 重置、居中和适应窗口。
- 真实 A→B/C 数据暴露服务端三节点 Depth 均为 0，首版画布因此纵向堆叠。新增 `MessageTaskGraphLayout.ResolveDepths`，在保留更大权威 Depth 的同时从 prerequisite 关系推导拓扑层级；最终 A 位于左列，B/C 位于右列，两条曲线和依赖摘要与 Inspector 一致。新增 2 项布局回归。
- 实机发现按钮缩放的两次 `ChangeView`（缩放后再居中）互相覆盖，连续放大始终停在 96%。最终将目标缩放和中心偏移合并为一次原子 `ChangeView`：86% 初始 Fit 后连续放大稳定到 200%，重置为 100%，再 Fit 回 86%。原生 ScrollPattern 将水平位置从 50 移到 80，Fit 后重新居中。合成左键拖拽没有形成可靠 WinUI Pointer 证据，故不把该输入路径记为通过；Windows 原生滚动平移已实际通过。
- 125% 缩放、900×700 窄窗下 Fit 为 25%，三节点、依赖曲线、Inspector、刷新、关闭和五个视口控件同时可达。第一次 Center/Fit 宽 32 DIP 时 Fit 仅露 28 px；缩到 24 DIP 后按钮完整，但默认 Padding 吃掉图标；最终设 `Padding=0`，两枚图标在 30×28 px 目标中完整显示，最终截图已目视检查。
- 正常退出后 Desktop/Host 归零；最终构建启动后恢复测试账号、联系人历史、Local Agent Host 和同一真实 DAG。布局专项 2/2、桌面契约专项通过；最终常规 Core 27 + API 46 + Presentation 851 + Connector 553 + NetworkGuard 19 = **1,496 通过、0 失败**，package/TRX/acceptance verifier 通过；SDK 8.0.426 下 Rust Host/x64 Debug 构建 0 警告、0 错误。
- 证据目录 `.local/parity/2026-10-16-1330/`。关键证据：`00-before-list-only`、`06-dag-canvas-wide`（首版纵向布局）、`09-topology-wide`、`18-atomic-zoom-200-panned`、`23-native-pan-80`、`25-true-narrow-fit`、`31-final-narrow-fit-complete`（图标被 Padding 吃掉的中间失败）、`34-final-narrow-icons`、`atomic-zoom-pan-assertions.json`、`native-pan-narrow-assertions.json`、`final-ui-assertions.json`、`tests-final.log` 与 `build-icon-final.log`。

## 2026-10-16 09:30–09:55：多节点依赖与无 Run 节点显示

- 真实调用 `create_tasks_with_prerequisites` 创建 A→B/C 三节点 DAG。A 进入 Ask User waiting_user 时，B/C 保持 pending 且没有 Run；提交 Continue 后 A/B/C 依次返回 `DAG-A/B/C-OK-20261016-0945`。只读 SQLite 核对三个 Run 均 succeeded，依赖边为 A→B 和 A→C。
- 修复前 Windows 只显示扁平标题/状态，不显示为何 pending；选中无 Run 的 B 后还会在标题区残留 A 的 Run ID。连接层现将前置 Task 标题/状态映射到每个节点；列表行显示 `← 前置标题`并提供稳定可访问名称；详情显示前置任务卡。
- 无 Run 节点现显示本地化的“尚未产生运行记录”，同时隐藏运行结果卡和标题 Run ID。第一次重建实机证明运行结果卡已折叠但标题仍残留 A Run，二次修复/构建后可见 Run ID 数为 0。
- 再创建 A→B 等待 DAG，通过 Ask User 取消 A，B 权威级联为 cancelled；完整重启后 A/B 终态、前置标题/状态、无 Run 提示和空过程均恢复。
- 新增 Connector 前置引用映射回归与 Presentation 依赖/可访问摘要回归。最终 Core 27 + API 46 + Presentation 849 + Connector 550 + NetworkGuard 19 = **1,491 通过、0 失败**；package/TRX/evidence verifier 通过；Rust Host/x64 Debug 构建 0 警告、0 错误。
- 证据断言 9/9 通过。重点证据：`05-dag-graph-open`、`08-gate-submitted`、`15-fixed-dependent-selected`、`20-fixed-pending-dependent`（第一次修复后仍残留 Run，作为中间失败证据）、`28-final-dependent`、`dag-db-completed.json`、`ui-assertions.json`、`tests-final.log` 和 `build-final-after-header.log`。

## 2026-10-16 10:15–10:30：真实 61 条事件分页与中英文计数

- 通过主聊天真实调用 `create_task` 创建 `WIN-TASK-EVENT-PAGE-20261016-1015`。Task 只顺序调用十次进度记录工具并报告 succeeded，不读写项目文件、不使用终端/网络/附件/Plugin/MCP；只读 SQLite 核对 Run `task-run-632f86e6-f215-471d-b48c-6a19ac34ab9a` 最终产生 61 条事件、iteration 12、状态 succeeded。
- 修复前实机首屏正确限制 40 条，但按钮文案为“加载更早过程”，与事件按创建顺序追加的真实语义和 macOS “加载更多运行事件”不一致，界面也不显示当前/总数。Windows 现显示可访问计数 `40 / 61`，按钮改为“加载更多运行事件 / Load more run events”，并补齐零事件提示。
- 中文最终构建首屏为 `40 / 61`，滚动详情点击后变为 `61 / 61`，加载按钮消失，61 条 UI 事件行与数据库一致，取消/重试继续按 succeeded 终态禁用；旧文案计数为 0。切换 English 后真实界面显示 `Execution process 40 / 61` 与 `Load more run events`，随后恢复简体中文。
- 正常关闭耗时约 278 ms，Desktop/Local Agent 均为 0；最终 Rust Host/x64 Debug 重建后完整启动，恢复登录、联系人历史、同一 Task/Run，并重新以 `40 / 61` 首屏和加载按钮进入，证明分页不是仅驻留内存的状态。
- Task Graph 专项 109 项通过；最终 Core 27 + API 46 + Presentation 849 + Connector 550 + NetworkGuard 19 = **1,491 通过、0 失败**，package/TRX/acceptance evidence verifier 通过；最终构建 0 警告、0 错误。
- 证据目录 `.local/parity/2026-10-16-1015/`。重点：`08-initial-40`、`10-loaded-all-61`（修复前真实分页）、`14-final-initial-40`、`15-final-initial-bottom`、`16-final-loaded-61`、`21-english-initial-40`、`28-final-clean-chinese`、`11-exit-before-build-result.json`、`ui-assertions.json`、`tests-final.log` 与 `build-final.log`。

## 2026-10-16 12:45–13:05：响应式任务工作区与真实 DAG 窄窗回归

- 对照 macOS `MessageTaskWorkspaceSheet` 的 1080×720 起步工作区、至少 700 宽图区域和右侧 Inspector。Windows 原实现固定为 620 DIP 右侧抽屉；在系统 125% 缩放、主窗口 900×700 时，抽屉越过 Conversation 宿主并被左侧裁切约 200 个物理像素，节点栏实际只剩约 65–75 px，标题和依赖不可辨。
- Windows 工作区现取 `min(1280, ConversationPage.ActualWidth)`，宽屏给任务详情更多空间，窄屏绝不越过会话宿主；节点栏按宿主宽度取 148、220 或最多 360 DIP。节点列表、Inspector、刷新、关闭和操作区新增稳定 AutomationId/可访问名称。新增 1 项布局契约测试，防止重新引入固定 620 宽度或丢失关键入口。
- 修复后先以真实单 Task 验证 900×700 节点栏恢复为 185 px，详情可滚动且底部操作区固定可达；再打开已有真实 A→B 取消 DAG：两个节点、依赖箭头、前置标题、刷新/关闭和重试同时可见。有 Run 的 A 重试启用；实际点击 B 后显示“尚未产生运行记录”和 A 前置任务，重试禁用；刷新后仍保留 B，关闭返回聊天。
- 1800×1100 实机扩展后节点栏 413 px、Inspector 1059 px；重新缩到 900×700 后仍回到 185 px 且 DAG 数据/选择逻辑不丢失。最终构建首次完整重启后重新定位同一历史 A→B DAG，两个节点、刷新、关闭和重试入口继续可达，证明布局不是旧进程驻留状态。
- 首次最终重启约三分钟后，旧持久化凭据读取失败，客户端明确回到“登录状态已失效，请重新登录”；没有把该状态伪报为持续认证成功。使用验收账号重新登录后 Host 和历史恢复，同一 DAG 可打开；随后再次正常退出，Desktop/Host 均归零，再次完整重启直接恢复认证。最终打开真实成功 A→B/C 三节点 DAG，900×700 下三个节点、依赖、刷新、关闭和操作入口均可达，Host 保持运行。旧凭据失效原因继续归登录综合项，不阻塞本轮布局窄项。
- 首次滚动 UIA 断言把 Border 当作可访问控件且要求滚到底部仍看到顶部事件计数，因此脚本断言失败；截图实际显示事件行和固定操作区正常。该失败是验收脚本假设错误，不是产品回归，后续以真实按钮/列表/Inspector 和截图为准。
- 布局专项 1/1 通过；最终 Core 27 + API 46 + Presentation 849 + Connector 553 + NetworkGuard 19 = **1,494 通过、0 失败**，package/TRX/acceptance evidence verifier 通过；Rust Host/x64 Debug 构建 0 警告、0 错误。证据目录 `.local/parity/2026-10-16-1300/`，重点 `06-real-narrow-before-main`、`10-fixed-narrow-graph`、`12-fixed-wide-graph`、`17-narrow-dag-fixed`、`20-narrow-dag-dependent-clicked`、`21-narrow-dag-dependent-refreshed`、`25-final-restart-narrow-dag`、`27-relogin-result`、`31-fresh-credential-restart`、`33-final-fresh-restart-dag`、`final-fresh-restart-assertions.json`、`tests-final.log` 与 `build-final.log`。
- 本轮只闭环响应式工作区和窄窗操作可达；Windows 仍是节点列表 + Inspector，没有实现 macOS 的缩放、平移、连线、适应/居中控制，因此综合任务图项继续开放，不能宣称画布完整对齐。
