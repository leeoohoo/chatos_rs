# Windows Ask User 实机验收

## 范围与 macOS 基线

2026-10-16 使用真实登录联系人会话和真实 Local Agent Task，验收 Ask User 的必填文本、多行文本、单选默认值、敏感字段、提交/取消、Task 继续、布局、正常退出与完整重启。对照 macOS `AskUserPromptCardView.swift`、`ConversationSessionViewModel+AskUser.swift` 和 `NativeLocalAgentAskUserPromptService.swift`。

初始轮次不覆盖多选真实点击、提交中人为断网/服务失败、timeout、Pet 内嵌 Ask User 或命令审批卡。2026-10-16 10:45–11:30 已补齐主聊天与 Pet 内嵌多选，14:30–14:55 已补齐提交时 Local Agent Host 中断、输入保留、凭据化恢复和重试成功；timeout 和审批链路仍开放，因此主清单的 Ask User/审批综合项继续保持未勾选。

## 发现与修复

初始 Windows 表单可以显示和提交，但真实 UI 暴露了以下差异：

- 必填字段没有 `*` 标识；输入框、选择器和操作按钮缺少稳定 AutomationId/可访问名称。
- 多行输入仍使用普通单行高度；缺少“任务正在等待你的答复”“待处理”和选择数量提示。
- 中文按钮是“取消/提交”，没有对齐 macOS 的“取消请求/确认提交”。
- 卡片横跨整个聊天区，操作按钮逼近右下 Pet 悬浮窗；macOS 卡片上限为 760 DIP。

Windows 已补齐必填标识、待处理头部、选择提示、86 DIP 多行输入、字段/选项/提交/取消的可访问标识和文案，并将卡片固定为 macOS 的 760 DIP、左对齐。125% 显示缩放下真实 UIA 宽度为 950 px，提交/取消按钮位于卡片内且不再被右侧 Pet 遮挡。

## 真实流程

| 场景 | 结果 |
| --- | --- |
| 未填必填字段 | 通过；提交按钮禁用，Environment、Access Token、Notes 均有名称和稳定 AutomationId |
| 填写文本/敏感/双行备注 | 通过；默认选中 Continue，字段完整后提交启用，敏感值只暴露为圆点掩码 |
| Local Agent 敏感值提交 | 按 macOS 同策略安全拒绝，显示 `Secret answers cannot be stored in Local Agent state.`；表单保留以便取消/恢复，未进入 Local Agent 持久化命令 |
| 取消敏感表单 | 通过；卡片、敏感控件和掩码立即消失，Task 转为 cancelled |
| 普通混合表单提交 | 通过；Environment=`staging`、双行 Notes、默认 Continue 提交后卡片消失，Task 继续并返回精确结果 `ASK-CONTINUE-OK-20261016-0845` |
| 最终布局 | 通过；最终构建的 760 DIP 卡片、状态、选择提示、提交和取消均可见可达，Pet 浮窗不遮挡操作区 |
| 退出与重启 | 通过；正常退出约 275 ms，Desktop/Host 均归零；完整重启恢复成功结果且不复现已处理/取消表单，composer 为空 |

敏感测试值由验收进程随机生成，从未写入文档、日志或命令输出；UIA 证据只包含等长圆点掩码。

## 自动化与构建

- Ask User Presentation 专项：17 项通过。
- Desktop Automation Contract：22 项通过。
- 最终常规测试：Core 27 + API 46 + Presentation 848 + Connector 549 + NetworkGuard 19，共 **1,489 项通过、0 失败**；package、TRX 和 acceptance evidence 校验器通过。
- 最终 Rust Host/x64 Debug 构建：0 警告、0 错误。
- `git diff --check` 通过。

## 证据

目录：`.local/parity/2026-10-16-0830/`。

- `03-waiting-form.png`：修复前横跨聊天区、输入无稳定名称的表单。
- `06-restored-form.png`：必填、状态、可访问标识和多行高度修复后首次重启恢复。
- `10-form-filled-tree.json`：必填完成、默认 Continue、提交启用，Secret 仅为圆点掩码。
- `12-task-continued-tree.json`：Local Agent 敏感持久化策略的真实错误。
- `13-secret-prompt-cancelled-tree.json`：取消后无 Ask User/Secret 控件。
- `16-success-form-filled.png`、`19-success-bottom.png`：普通混合表单与 Task 精确继续结果。
- `24-final-layout-form.png`：最终 760 DIP 布局与 Pet 操作可达性。
- `26-final-exit-result.json`、`28-final-persisted.png`：进程清理与重启持久化。
- `ui-assertions.json`：13/13 实机证据断言通过。
- `tests-final.log`、`build-final.log`：最终完整测试与构建。

## 剩余入口

下一组优先 Ask User timeout 和审批链路；综合行在这些场景闭环前保持开放。当前真实工具固定生成 `timeout_ms=86400000`，schema 不接受调用方覆写，需等待真实 24 小时或先提供可控时钟/服务端注入方案，不能以短暂等待伪报 timeout 通过。

## 2026-10-16 10:45–11:30：主聊天多选与 Pet 内嵌卡

- 主聊天真实 Task `WIN-TASK-MULTI-20261016-1045` 请求 Alpha/Beta/Gamma 精确选择 2 项。0/1 项时提交禁用，Alpha+Beta 时启用，继续点 Gamma 被 2 项上限拒绝；取消 Beta 后重新禁用，改选 Gamma 后提交，Task 精确返回 `MULTI-OK-alpha-gamma`。
- 对照 macOS `PetAskUserInlineView.swift`，Windows Pet 原先遗漏标题/等待态、必填标识、选择范围、选项描述、多行高度、进度和稳定可访问标识。现与主聊天复用同一表单语义，并为 Pet 卡、字段、单选、多选、提交、取消和活动入口补齐独立 AutomationId。
- 实机发现 Task waiting_user 虽带 `PromptId`，Pet 活动仍被映射为 `TaskExecution`；`PetOverlayViewModel.SelectAsync` 因来源不是 `AskUserPrompt` 直接返回，真实详情只停在“等待你的回复”。`WindowsLocalAgentPetActivityService` 现将带会话上下文的 waiting_user Run 映射为 Ask User 来源和“需要你的回复”，Task 与主聊天均可加载同一真实表单。
- 分层透明 Pet 窗口中的原生 `ListView.ItemClick` 没有稳定活动行可访问入口；活动列表改为滚动的显式按钮列表，鼠标、键盘、读屏和自动化共用 `SelectAsync`。最终真实 prompt 通过 Pet toast 进入详情，显示三项、描述和“选择 2 项”。
- Pet 真实操作同样验证 0/1 项禁用、2 项启用、第 3 项拒绝、取消后禁用和重选；最终 One+Three 由 Pet 提交，真实 Task 精确返回 `PET-POSTFIX-OK-one-three`。截图 `28-pet-form.png` 和 `29-pet-one-three.png` 已实际查看，按钮与滚动条在 125% 缩放下可达。
- 新增 waiting Task→Ask User 来源回归与 Pet 自动化契约。专项 25 项通过；最终常规 Core 27 + API 46 + Presentation 849 + Connector 552 + NetworkGuard 19，共 **1,493 项通过、0 失败**，package/TRX/acceptance evidence verifier 通过；最终 Rust Host/x64 Debug 构建 0 警告、0 错误。
- 证据目录 `.local/parity/2026-10-16-1045/`。重点为 `04-waiting`、`08-alpha-gamma-valid`、`10-result`、`28-pet-form`、`29-pet-one-three`、`tests-pet-final-targeted.log`、`tests-final.log` 和 `build-final-source.log`。提交失败、timeout 和命令审批仍未在本轮闭环。
- 最终正常退出从 `11:31:46.187` 到 `11:31:46.465`，约 278 ms 后 Desktop 与 Local Agent Host 均归零。完整重启最终构建后恢复认证和联系人历史；`PET-POSTFIX-OK-one-three` 仍可见，已解决的 Ask User 卡未复现，Pet `AttentionBadgeText` 为 `0`，composer 为空且可用，模型/远端/推理分别恢复为 `my_api / gpt-5.6-sol`、`未绑定远端`、`none`。`startup.log` 从本次启动到账号恢复完成没有新的 exception/error。最终证据为 `30-final-exit-result.json`、`31-final-restart-shell`、`32-final-restart-contact`、`33-final-restart-bottom` 和 `34-final-restart-assertions.json`；`33-final-restart-bottom.png` 已实际查看。

## 2026-10-16 14:30–14:55：提交中断恢复与重试

- 真实 Task 在一个必填非敏感文本表单等待时填入唯一值，随后强制终止唯一 `chatos_local_agent_host.exe` 并立即提交。Windows 明确显示 `Local Agent Host is not running.`；输入值完整保留，提交与取消重新可用，没有把瞬时失败伪装为已提交。
- 首轮重试暴露异常恢复竞态：恢复流程先启动未注入模型凭据的临时 Host 并过早把消费者重新绑定；UI 重试已成功写入 `ask_user_submitted`，但后续模型运行缺少 `CHATOS_LOCAL_AGENT_MODEL_*`，Task 进入 `needs_review`。这属于 Host 恢复问题，不是 Ask User 输入丢失。
- `WindowsLocalAgentBootstrapService` 现让异常恢复调用延迟绑定路径：先读取最新模型凭据并完成 `RestartForOwnerAsync`，只有凭据化 Host 就绪后才重新配置 workspace、项目会话、Notepad 等消费者。新增桌面原生兼容契约，防止恢复路径重新暴露未凭据化 Host。
- 最终构建中的真实 Task `WIN-ASK-RECOVERY-FINAL-20261016-1450` 在中断时保留 `ASK-RECOVERY-FINAL-OK-20261016-1450`；Host 恢复后再次提交成功，Task 与 Run 均为 `succeeded`，没有 `run_needs_review`，最终回复精确为该唯一值。
- 正常退出后 Desktop/Host 均归零；完整重启最终构建后恢复认证、Host、`ChatOS 仓库管理` 项目聊天及精确成功结果，已处理 Ask User 表单未复现。`12-retry-task-interrupted.png`、`24-final-retry-success.png` 和 `28-final-restart-restored.png` 已实际查看。
- SDK 8.0.426 专项 16/16 通过。最终常规 Core 27 + API 46 + Presentation 851 + Connector 554 + NetworkGuard 19，共 **1,497 项通过、0 失败**；package、TRX、acceptance evidence verifier 通过；最终 Rust Host/x64 Debug 构建 0 警告、0 错误。
- 证据目录 `.local/parity/2026-10-16-1430/`；重点为 `12-retry-task-interrupted`、`24-final-retry-success`、`28-final-restart-restored`、`final-recovery-db.json`、`final-restart-assertions.json`、`tests-recovery-targeted-sdk8.log`、`tests-final.log` 和 `build-final.log`。
- timeout 未在本轮勾选：真实 Local Ask User 工具固定产生 `timeout_ms=86400000`，schema 拒绝自定义 5 秒 timeout。20 秒以上短等待后 prompt 仍处于 waiting 是预期行为，不构成 timeout 证据。

