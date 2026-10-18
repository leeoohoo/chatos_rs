# Windows 命令审批真实验收

日期：2026-10-16 11:45–12:14 +08:00  
证据目录：`clients/windows/.local/parity/2026-10-16-1200/`

## 范围与安全边界

本轮只验收 `RequestApproval`（需要确认）模式。所有真实命令都是绑定本地项目内的 `powershell -NoProfile -Command Write-Output ...`，没有读写、删除或改名用户文件。自动审批模型、完全控制、远端命令/上传和审批检查失败仍不在本轮通过范围。

## 真实客户端矩阵

| 场景 | Windows 结果 | 审计证据 |
| --- | --- | --- |
| 拒绝 | Pet 全局卡显示来源、低风险、命令、工作目录和原因；拒绝后 Task 报告未执行且不重试 | `approved=0`、`reviewer=user` |
| 本次允许 | 命令执行并返回精确 `APPROVAL-ALLOW-ONCE-OK-20261016-1220` | `approved=1`、`reviewer=user` |
| 本会话允许 | 第一条命令经用户允许；同项目/会话第二条不同命令无重复卡并返回精确结果 | 第一条 `reviewer=user`，第二条 `reviewer=session` |
| 历史分页 | 12 条真实记录显示 `1–10 / 12` 与 `11–12 / 12`；按钮状态、允许/拒绝标签正确 | 第二页截图及只读 SQLite 一致 |
| 刷新 | 第二页刷新后仍为 `11–12 / 12` | 当前页未被重置 |
| 正常退出 | Desktop 与 Local Agent Host 均归零 | `31-exit-result.json` |
| 重启边界 | 重启后同范围命令重新要求审批，会话授权未泄漏 | 新卡出现并再次拒绝，历史增至 13 |
| 最终构建恢复 | 登录、设置、需要确认模式和 `1–10 / 13` 两页历史恢复；无待处理按钮 | `41-final-history-restored` |

## 诊断与实现结论

项目会话最初保留的任务推理参数为 `minimal`，当前任务模型只接受 `low/medium/high/xhigh/max`；三个诊断 Task 因服务端 400 在审批前失败。显式设置 Task `thinking_level=low` 后全部审批路径正常，因此未把该模型参数错误记作命令审批产品故障。本轮没有修改生产代码；现有实现不需要修复。

## 自动化、构建与证据

- 审批专项：81 通过、0 失败、0 跳过。
- 常规套件：Core 27、API 46、Presentation 849、Connector 552、NetworkGuard 19，共 1,493 通过、0 失败。
- package、TRX 与 Windows acceptance evidence verifier 通过；机器条件分类仍排除。
- SDK 8.0.426，Rust Host/x64 Debug 构建 0 警告、0 错误。
- 主要文件：`tests-approval-targeted.log`、`tests-final.log`、`build-final.log`、`27-approval-history-bottom.png`、`28-history-page-two.png`、`35-restart-boundary-task.png`、`41-final-history-restored.png`。

## 剩余入口

继续自动审批模型的实际同意/拒绝、模型检查失败/重试、远端命令/上传，以及安全模式和系统权限；完全控制风险确认与自动审批不可用回退已在下方轮次闭环。

## 2026-10-16 12:30–12:44：完全控制与自动审批安全回退

证据目录：`clients/windows/.local/parity/2026-10-16-1230/`

### 真实客户端矩阵

| 场景 | Windows 结果 | 审计/持久化证据 |
| --- | --- | --- |
| 完全控制取消 | 选择“完全控制”显示高风险命令、设备/项目/任务信任说明；点击取消后仍为“需要确认” | 未写入新策略 |
| 完全控制确认 | 再次选择并确认后显示高风险说明和“审批模式已更新” | `connector_approval_settings.mode=full_control` |
| 完全控制真实命令 | durable Task 执行只读 `Write-Output APPROVAL-FULL-CONTROL-OK-20261016-1245`，无审批卡并精确成功 | `mode=full_control`、`approved=1`、`reviewer=policy` |
| 完全控制重启 | 正常退出后 Desktop/Host 均归零；完整重启设置页仍选中“完全控制” | 数据库和 UI 一致 |
| 自动审批确认/就绪 | 风险弹窗文案与 Mac 语义一致；确认后显示“Windows AI 审批已就绪” | 已配置审批模型，`mode=auto_approval` |
| 自动审批调用不可用 | 真实只读命令没有被静默放行，显示“automatic approval reviewer is unavailable”并回退三个用户操作 | 安全回退成立，命令尚未执行 |
| 回退后本次允许 | 点击“本次允许”后 Task 精确返回 `APPROVAL-AUTO-OK-20261016-1255` | `mode=auto_approval`、`approved=1`、`reviewer=user` |
| 自动模式重启/安全恢复 | 重启后 UI 仍为“自动审批”；最终改回“需要确认”并持久化 | 最终 `mode=request_approval` |

### 结论与边界

- 完全控制的风险确认、取消、直接执行、policy 审计和重启持久化全部通过。
- 自动审批模式的风险确认、配置就绪检查、执行时 reviewer 不可用的用户确认回退、回退后继续执行和重启持久化通过。当前运行未获得模型自动同意或自动拒绝，因此不宣称审批模型自动决策通过；执行层只返回安全化的 reviewer unavailable 文案，没有可进一步归因的供应商错误，按外部依赖阻塞保留。
- 最终没有把高权限模式留在客户端：设置和数据库都恢复为推荐的 `request_approval`。
- 本轮未发现需要修改的生产代码。审批专项 100 项通过；完整常规 Core 27、API 46、Presentation 849、Connector 552、NetworkGuard 19，共 1,493 通过、0 失败；package/TRX/acceptance verifier 通过。Rust Host/x64 Debug 构建 0 警告、0 错误。
- 关键证据：`01-full-control-confirm-dialog`、`02-full-control-cancelled`、`09-full-control-task-bottom`、`10-full-control-exit-result.json`、`14-full-control-restored-ui`、`15-auto-approval-confirm-dialog`、`16-auto-approval-enabled`、`21-auto-task-state`、`23-auto-fallback-result`、`24-auto-exit-result.json`、`28-auto-restored-ui`、`29-request-mode-restored`、`tests-approval-targeted.log`、`tests-final.log`、`build-final.log`。
