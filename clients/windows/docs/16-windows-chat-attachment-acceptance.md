# Windows 普通聊天附件验收

## 2026-10-16 02:15–05:42：选择、粘贴、PNG 规范化和历史持久化

### 对照与修复

- 对照 macOS `ConversationSessionViewModel+Attachments.swift`、`ComposerView.swift` 与 `ComposerPasteTextEditor.swift`：附件上限为 20 个、单项/累计 20 MiB，长文本只生成附件，TIFF/BMP 等剪贴板位图先规范化为 PNG。
- Windows 实机复现：粘贴 4,129 字符后同时生成长文本附件并把全文写入 composer。`TextControlPasteEventArgs.Handled` 原先在 `await GetTextAsync()` 之后设置，原生 TextBox 已先完成粘贴。现进入文本分支即接管事件；短文本以选择区替换策略写回，长文本只生成附件。
- Windows 实机复现：剪贴板截图生成名为 `.png` 的附件，但 MIME 为 `image/bmp`、大小 15.4 MiB。现用 `BitmapDecoder`/`BitmapEncoder.PngEncoderId` 真正转码，最终 320×200 样本为 `image/png`、3.4 KiB，预览内容正确。
- 文件选择阶段现按已有草稿计算剩余数量和剩余 20 MiB，在读取字节前拒绝超限项；选择器标题、读取错误和移除按钮无障碍名随中英文切换。

### 自动化验证

- 新增 `ConversationComposerTextPasteTests` 3 项：选择区替换/光标、4,000 字符边界、UTF-8 8,000 字节边界。
- 新增 Desktop 原生合同 2 项：文本 paste 必须在首个 await 前 `Handled`，剪贴板位图必须经过 PNG encoder。
- `build/test.ps1 -Configuration Debug -SkipMachineAcceptance` 最终通过：Core 27、API 46、Presentation 844、Connector 547、NetworkGuard 19，共 **1,483 通过、0 失败**；三类 evidence verifier 均通过。SDK 为 8.0.426，机器条件分类仍排除。
- `build/build.ps1 -Configuration Debug -Platform x64` 最终 Rust Host 与 WinUI 构建通过，**0 警告、0 错误**。

### 真实 UI 与持久化证据

| 场景 | 结果 |
| --- | --- |
| 文件选择、135 B Markdown 条、文本预览、删除 | 通过 |
| 20 MiB + 1 B 单文件 | 显示超限错误且未加入草稿，通过 |
| 修复前长文本 | 1 个附件且 composer 仍有 4,129 字符，已复现 |
| 修复后长文本 | 1 个附件、composer 空、中文移除按钮，通过 |
| 修复后短文本 | 得到精确 `SHORT-PASTE-OK-20261016-0241`，未转附件，通过 |
| 修复前剪贴板图 | `.png` 名称但 `image/bmp`、15.4 MiB，已复现 |
| 修复后剪贴板图 | `image/png`、3.4 KiB、预览显示 `WIN PNG 20261016`，通过 |
| 附件发送与重启 | 用户消息、`attachment-sample.md` 和 135 B 大小重启后仍在；composer 空、无草稿附件，通过 |
| 本地附件完整性 | vault 中恰有 1 个文件与源 Markdown SHA-256 `b63e5251…ae61` 一致，通过 |
| 模型最终消费 | 未通过：附件轮次先收到 provider 400（`minimal` 不支持），后续 runtime bootstrap 报 `Local Agent conversation settings are not configured.`；未把该外部/配置阻塞误记为成功 |

正常退出证据 `33-final-exit-result.json` 显示 Desktop 与 Local Agent Host 均归零。最终再次启动后保留认证、附件历史、空 composer；客户端按惯例保持运行。

证据目录：`.local/parity/2026-10-16-0215/`。关键文件为 `09-long-paste-added`（修复前重复）、`14-long-paste-fixed`、`16-short-paste-fixed`、`08-oversize-rejected`、`25-image-paste-added-stable`（修复前 BMP）、`29-image-paste-png`、`30-image-preview-png`、`35-final-restart-stable`、`ui-assertions.json`、`attachment-vault-verification.json`、`tests-final-png.log` 与 `build-final-png.log`。

### 尚未通过

- 真实拖放、20 个数量边界、多个文件累计 20 MiB、英文文件选择器、不可读/权限拒绝和取消选择。
- 真实发送失败后的正文/附件共同恢复。runtime 恢复与成功模型附件消费已在下方 06:15 轮次闭环；附件矩阵在其余项目完成前保持 pending。

## 2026-10-16 06:15：runtime 恢复与真实模型附件消费

### 复现与修复

- 最后两次修复前完整启动都在 `startup.log` 记录 `Local Agent conversation settings are not configured.` 的未观察任务异常。账号恢复流程先发布工作区并异步打开上次联系人，之后才完成 Local Agent control-plane bootstrap；聊天同时启动 runtime 设置读取，其中同步抛出的模型目录读取还会留下已启动、随后 fault 的设置任务。
- 对照 macOS `configureLocalAgentServices` 完成后对每个聊天调用 `localAgentRuntimeDidBecomeReady()` 的恢复路径，Windows 增加 `LocalAgentRuntimeDidBecomeReadyAsync`。完整 bootstrap（包括 Connector 401 换票重试成功）后只重载当前会话 runtime、历史和 Ask User 状态，不重建会话、历史或 composer 草稿；首轮失败时未启动的 realtime 也会恢复。
- `LoadRuntimeAsync` 先等待模型目录，再并行读取会话设置和远端，避免 bootstrap context 同步校验失败时遗留另一个未观察的 faulted task。

### 自动化与构建

- 新增 `RuntimeBecomingReadyReloadsModelsAfterInitialBootstrapFailure`，复现首次打开未配置、bootstrap 完成后自动恢复模型/选中项/加载状态/错误清理。聊天专项 48 项、Shell/runtime 专项 24 项通过。
- 最终 `build/test.ps1 -Configuration Debug -SkipMachineAcceptance`：Core 27、API 46、Presentation 845、Connector 547、NetworkGuard 19，共 **1,484 通过、0 失败**；package/TRX/acceptance evidence 校验器通过。机器条件分类仍排除。
- 最终 `build/build.ps1 -Configuration Debug -Platform x64`：Rust Host 与 WinUI x64 Debug 成功，0 警告、0 错误。

### 真实客户端闭环

| 场景 | 结果 |
| --- | --- |
| 新构建首次完整启动 | 登录自动恢复；打开联系人后模型为 `my_api / gpt-5.6-sol`、远端为“未绑定远端”、推理为 `none`，composer 空且可用；界面无 runtime 未配置错误 |
| 启动日志 | 06:08 和 06:17 两次新构建启动后均无新的 `Unhandled exception during unobserved task` |
| 真实附件选择与发送 | 通过系统文件选择器重新选择原始 135 B `attachment-sample.md`；双行正文与附件共同发送，composer 清空 |
| 模型消费 | 模型精确返回附件中要求的 `ATTACHMENT-OK-20261016-0215`，证明 runtime、附件 vault、读取工具和模型调用完整链路可用 |
| 正常退出 | 261 ms 后 Desktop 与 Local Agent Host 均为 0 |
| 完整重启 | 登录、附件名称/135 B、用户消息、精确模型回复、模型、远端及空 composer 均恢复；最终滚动到底部后附件和回复同时可见 |

证据目录为 `.local/parity/2026-10-16-0615/`。优先引用 `03-contact-open-tree.json`、`06-attachment-added.png`、`07-attachment-composed-result.json`、`08-attachment-sent-result.json`、`09-attachment-response-tree.json`、`10-normal-exit-result.json`、`11-restart-contact-tree.json` 和已实际查看的 `12-restart-response-visible.png`。`04-attachment-picker`/`05-after-picker-attempt` 是文件选择器 modal 期间被阻塞的辅助采集，不作为产品失败或最终通过证据。

### 剩余范围

真实成功模型消费与 runtime 恢复阻塞已经闭环；综合附件行仍不勾选。下一轮继续拖放、20 个数量边界、多文件累计 20 MiB、英文文件选择器、取消选择、不可读/权限拒绝，以及发送失败后的正文与附件共同恢复。

## 2026-10-16 06:45–07:20：边界、权限、拖放和失败恢复闭环

### 真实客户端矩阵

| 场景 | 结果 |
| --- | --- |
| 英文选择器与取消 | 标题为 `Choose chat attachments`；取消后 composer 和附件草稿不变，通过 |
| 数量上限 | 原生多选返回 67 个既有验收证据文件，客户端只接收前 20 个，显示 `20 attachments · 1009 KB` 与 `You can add at most 20 attachments at a time`；20 个草稿随后全部移除且未发送，通过 |
| 累计大小 | 先接收精确 10 MiB `total-a.bin`，再选择 10 MiB + 1 B 文件；第二项未进入草稿，仍为 1 个/10 MB 并显示 `The total attachment size cannot exceed 20 MB`，通过 |
| 权限拒绝 | 对隔离夹具 `blocked.txt` 临时施加当前账号显式读拒绝；Windows 原生选择器直接显示“没有打开该文件的权限”，取消后附件为 0、composer 为空；随后已移除拒绝 ACL，通过 |
| 真实拖放 | 将资源管理器中的 `count-01.txt` 拖到窄窗口 composer，显示 `1 attachment · 3 B`、文件名/MIME 和移除按钮，通过 |
| 真实发送失败 | composer 写入 `FAIL-RESTORE-20261016-0710` 并保留 3 B 附件，终止当前客户端的 Local Agent Host 后发送；界面显示 `Local Agent Host is not running.`，正文与附件同时恢复且发送重新可用，通过 |
| 失败后恢复 | Host 自动重启后移除测试附件，发送精确回复请求；模型返回 `POST-FAILURE-OK-20261016-0715`，通过 |
| 退出和重启 | 正常退出 246 ms 后 Desktop/Host 均归零；新构建完整启动恢复登录、中文、模型 `my_api / gpt-5.6-sol`、未绑定远端、空 composer、0 个草稿附件及失败后成功回复，通过 |

权限场景在 Windows 原生文件对话框层即被拦截，因此没有把“应用收到不可读 `StorageFile`”虚报为已覆盖；用户可见的权限状态、取消与草稿保护已经实机通过。所有临时附件草稿均已删除或发送前移除，只有用于恢复验证的纯文本请求及精确模型回复进入历史。

### 自动化、构建和证据

- 新增 `AttachmentValidationKeepsOnlyTheFirstTwentyDrafts` 与 `AttachmentValidationRejectsDraftThatWouldExceedTheTotalByteLimit`；附件专项 4 项通过。
- 最终常规测试：Core 27、API 46、Presentation 847、Connector 547、NetworkGuard 19，共 **1,486 通过、0 失败**；机器条件分类仍排除。
- 最终 `build/build.ps1 -Configuration Debug -Platform x64`：Rust Host 与 WinUI 成功，0 警告、0 错误。
- 证据目录 `.local/parity/2026-10-16-0645/`。优先引用 `02-english-picker.png`、`04-count-boundary`、`07-total-boundary`、`08-permission-dialog.png`、`10-side-by-side-chat.png`、`10-drag-drop-added`、`11-send-failure-restored`、`12-post-failure-recovery`、`14-normal-exit-result.json`、`16-final-bottom`、`tests-attachment-targeted.log`、`tests-final.log` 与 `build-final.log`。`03-picker-stuck`/`04-*-raw` 是文件对话框自动化调试过程，不作为产品失败证据。

附件综合项至此闭环并在主清单勾选。全功能矩阵仍未完成，下一轮继续真实 Task 创建/状态/取消、Ask User/审批、完整 Markdown 消息操作或其他 pending 项。
