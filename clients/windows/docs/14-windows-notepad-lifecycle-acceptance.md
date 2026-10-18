# Windows 记事本保存与应用退出验收

## 2026-10-15 本轮范围

本轮只闭环记事本键盘保存、未保存提示和主窗口退出后的进程回收；记事本完整功能及 Windows 全功能验收仍未完成。

Mac 依据：实际查看 `clients/macos/docs/screenshots/20-notebook.png`，阅读 `NotepadSheet.swift` 的 editorToolbar/editorContent：标题和标签在三种正文模式上方，未保存状态显示橙色标记，Command+S 保存。Windows 使用原生 Ctrl+S、主题警告色和现有保存命令。

## 复现与修复

- 修改测试笔记后按 Ctrl+S，保存按钮仍启用，底部仍显示“笔记已创建”；Windows 没有未保存标记。给保存按钮增加以记事本根节点为作用域的 KeyboardAccelerator，复用 CanExecute/SaveCommand；工具栏绑定 IsDirty，关闭按钮使用本地化文案。
- 正常关闭主窗口后，ChatOS.Desktop.exe 仍存活，Local Agent 已停止；构建因旧进程锁住 apphost 失败。单独补充 WinUI Exit 后再次复测，进程仍然残留。因此最终移除 UI 线程上的同步 GetResult 清理，后台依次异步关闭本地终端、远端终端及 Local Agent；每步最多等待 3 秒，失败/超时记录后继续。所有清理步骤结束后显式结束桌面进程。
- 新增 3 项未保存状态通知测试、4 项关闭清理测试，覆盖正常顺序、同步/异步异常和不响应取消的超时步骤。

## 实际 UI 与持久化矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 根目录没有笔记 | 通过：显示暂无笔记、选择一条笔记，保存/复制不可用 | 本轮 Computer Use 初始界面截图 |
| 独立测试笔记新建 | 通过：中文标题弹窗确认后打开笔记 | 笔记 ID `614316c0-9ec1-4fb0-a623-544eb20e2035` |
| 中文、多行、空行及尾部空格 | 通过：编辑正文并保存后只读核对数据库 | `persistence.json` |
| 中文逗号、重复标签 | 通过：保存后为“验收, windows” | `persistence.json` |
| Ctrl+S 与未保存提示 | 通过：编辑后显示标记；快捷键保存后标记消失、保存禁用 | `02-dirty-before-keyboard-save.txt`、`03-keyboard-saved.txt` |
| 编辑/预览/分栏 | 通过当前浅色宽窗口：标题/标签均可见，分栏左编辑右预览，方向键移动焦点、空格选择模式 | `04-split-layout.txt`、`05-keyboard-preview-layout.txt` 及本轮截图 |
| 关闭确认取消 | 通过：Escape 关闭确认，标题草稿与未保存状态仍保留 | `06-unsaved-close-dialog.txt`、`07-cancel-close-keeps-draft.txt` |
| 关闭确认保存 | 通过：保存后返回原工作区；数据库标题为 `Windows记事本验收-20261015-0915-关闭保存`，正文不丢失 | `08-save-close-return.txt` |
| 正常退出 | 通过最终构建：桌面和 Local Agent 进程数为 0 | `exit-final.json`、`exit-lifecycle.log`，09:42:33 +08:00 |
| 完整退出后重启 | 通过：恢复登录态、已保存标题/正文/标签，默认预览、保存禁用 | `09-final-restart-restored.txt`、`09-final-restart-restored-0.jpg` |
| 首次打开笔记时的加载状态 | 通过本轮观察：进度环显示，刷新/搜索/新建禁用；加载后编辑区恢复 | `10-loading-note.txt` |

以上证据目录为 `clients/windows/.local/parity/2026-10-15-0915/`，属于本地忽略目录。测试笔记保留给下一轮复用，没有删除已有数据。截图通过 Computer Use 显示；最终页面另保存 JPEG 证据。

WinUI TextBox 将传入的 CRLF 规范为 CR，数据库存储与编辑缓冲区一致；只读核对验证了规范后的完整正文、空行、尾部空格及标签，不能据此宣称原始 CRLF 字节保持不变。首次按 CRLF 逐字节核对失败的原因已明确，不把它计为通过。

## 执行命令与结果

先给当前 PowerShell 进程设置用户目录 SDK，不修改系统 PATH 或 global.json：

```powershell
$paritySdkRoot = Join-Path $env:LOCALAPPDATA 'ChatOS\build-tools\dotnet'
$env:DOTNET_ROOT = $paritySdkRoot
$env:PATH = "$paritySdkRoot;$env:PATH"
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~Notepad'
dotnet test clients/windows/tests/ChatOS.Connector.Tests/ChatOS.Connector.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~DesktopShutdownTests'
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
git diff --check -- clients/windows
```

- SDK 8.0.426。记事本专项 161 通过；退出专项 4 通过。
- 最终常规回归：Core 27、API 46、Presentation 807、Connector 544、NetworkGuard 19，共 1,443 通过、0 失败、筛选内 0 跳过；三组 evidence 校验器通过，退出 0。
- 最终 x64 Debug/Rust Host 构建：0 警告、0 错误，退出 0；日志 `build-shutdown-final.log`。首次构建锁文件失败及中间未闭环版本日志均保留。
- 机器条件用例由 SkipMachineAcceptance 排除，没有验收驱动、安装升级等能力。本轮使用 Computer Use 完成认证后的 UI 回归，没有运行另一个 PowerShell UI Automation 驱动。

## 下一轮

复用上面的独立测试笔记，继续关闭确认“放弃”、搜索无命中/清空后的编辑保护、真实保存失败/慢请求、窄窗口和英文布局。复制、导出、目录变更、删除、上传图片、完整 Markdown 和权限状态仍需单独真实验收。普通聊天/Task、Connector 互斥、项目文件、视频剧情、审批及远端矩阵仍继续，不暂停自动化。

## 2026-10-15 10:00：自动搜索与关闭放弃恢复

本轮开始先读自动化记忆，list_threads 及 read_thread 确认 09:15 轮次 turn 已 completed，其他同任务轮次均未活动，没有并发构建或测试。保留所有已有修改，修改前状态和 Search 源文件副本在 `clients/windows/.local/parity/2026-10-15-1000/`。

### 复现、对照和修复

- 真实已登录客户端输入 `PARITY-NO-MATCH-20261015-1000`，等待后仍显示原笔记；按 Enter 才显示无匹配。macOS `NotepadSheet.swift` 的 searchQuery onChange 调用 `NotepadViewModel.swift` 的 scheduleSearch，延迟 260ms 后 reloadNotes。
- Windows 搜索词变化现在调度 260ms 防抖，立即取消旧查询，旧返回不能覆盖新输入。Enter/按钮取消排队的防抖，避免重复请求；关闭、放弃和释放同步取消排队请求。防抖期间开始保存或刷新时，等编辑器可搜索后执行最新输入，避免悄悄丢掉查询。
- 搜索框绑定 CanSearch，读取/保存期间禁用；搜索进行中仍可输入替换查询。空状态文案改为“尝试其他关键词，或清空搜索”。复用原搜索服务及结果/编辑区隔离逻辑。
- 新增 8 项执行用例：最新词防抖且不保存草稿、清空恢复目录、关闭/释放取消、手动搜索去重、输入立即取消旧请求、失败后改词重试、保存占用期间保留待执行查询。首次专项暴露旧取消回调测试被无效清理调度干扰，修正已取消调度不再排队清理后 168 项记事本专项通过，随后增加保存等待用例并由最终全量覆盖。
- 首次全量另暴露 `MonitorRefreshesStatusUntilClosed` 的计时竞争：关闭和已越过计时点的下一次读取竞争。测试 double 现在将第二次读取挂起到 Close 之后才释放，明确验收在途请求取消后不再继续轮询；没有修改 Connector 产品行为。
- Mac 当前 close 自动保存；Windows 原有保存/放弃/取消确认是已有扩展。本轮只验收其恢复行为，没有将其描述为 Mac 拥有相同三按钮交互。

### 真实 UI 矩阵

| 场景 | 结果 | 本轮证据文件 |
| --- | --- | --- |
| 输入后不按 Enter | 通过：260ms 调度后真实无匹配，编辑区仍显示保存的笔记 | `01-before-no-auto-search.txt`、`02-auto-empty.txt` |
| 无匹配时编辑标题、清空查询 | 通过：列表恢复原保存标题，编辑区保留草稿标题和未保存标记，没有自动写入 | `03-dirty-hidden-note.txt`、`04-clear-keeps-draft.txt` |
| 关闭取消 | 通过：标题、标签、正文三字段草稿保留 | `05-all-fields-draft.txt`、`06-close-dialog.txt`、`07-cancel-keeps-all-draft.txt` |
| 关闭放弃再打开 | 通过：返回原工作区，重开恢复三个已保存字段，保存按钮禁用 | `08-discard-return.txt`、`09-reopen-restores-saved.txt` |
| 正文关键词查询 | 通过：输入 `literal_code` 自动命中原笔记，标题不包含该词 | `10-body-query-match.txt` |
| 最终构建完整重启 | 通过：恢复认证、原标题/标签/正文；默认预览，无本轮草稿 | `11-final-restart-shell.txt`、`12-final-restored-note.txt` |
| 最终构建自动搜索/清空 | 通过：输入 `FINAL-NO-MATCH-1000` 自动显示空状态，清空自动恢复列表，保存禁用 | `13-final-auto-empty.txt/.jpg`、`14-final-clear-restored.txt/.jpg` |

全部文件在上述 10:00 evidence 目录。截图由 Computer Use 捕获并实际查看，最终两张另保存为 JPEG。测试复用笔记 `614316c0-9ec1-4fb0-a623-544eb20e2035`，原标题及 UpdatedAt `2026-10-15T01:33:32Z` 在重启后列表仍相同；本轮所有临时三字段草稿已通过放弃恢复，没有保存或删除笔记。客户端保留运行，搜索已清空。

### 命令和结果

沿用上节用户目录 SDK 8.0.426 的 DOTNET_ROOT/PATH。执行命令：

```powershell
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~Notepad'
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
git diff --check -- clients/windows
```

- 首次记事本专项 167 通过/1 失败，修正调度后专项 168 通过；新增保存等待用例后，最终常规 Core 27、API 46、Presentation 815、Connector 544、NetworkGuard 19，共 **1,451 通过、0 失败**，三组 evidence 校验器通过，退出 0，日志 `tests-final.log`。先前失败日志保留。
- 最终 Rust Host/x64 Debug 构建 **0 警告、0 错误**，退出 0，日志 `build-final.log`。最终代码构建后再次执行真实重启和自动搜索/清空回归。
- 机器条件分类仍被排除，没有据此验收驱动、安装升级、真实网络权限。保存忙碌、搜索失败、旧响应及取消的特殊时序本轮仅有自动化证据，不能计为真实 UI 通过。

### 下一轮入口

继续复用原测试笔记，先做跨目录搜索和结果切换、真实慢/失败保存及搜索、英文/窄窗口。当前环境没有制造后端故障，未宣称这些状态实机通过。目录变更、复制/导出/删除、图片上传、权限、完整 Markdown 及聊天/Task、Connector 互斥、项目文件、媒体剧情、审批、远端仍待完成；自动化保持活动。

## 2026-10-15 10:15 补充：窄窗口按钮可达与英文目录标签

本轮已在真实客户端复现 850px 宽宿主中的保存/复制/删除被裁切；修复正文最小宽度与横向滚动，面板理想上限改为 1300×840。Mac 基线是 `NotepadSheet.swift` 的 sheet 最小宽 1120、editor 最小宽 720；Windows 小于此宽度时横向滚动适配，刷新/关闭固定在头部。

证据位于 `clients/windows/.local/parity/2026-10-15-1015/`：

| 场景 | 结果/证据 |
| --- | --- |
| 原 850px 按钮裁切 | `04-before-clipped-850-0.jpg` 复现 |
| 大面板和分栏 | `05-final-wide-0.jpg`、`06-final-split-0.jpg` 通过 |
| 窄窗口滚轮 | `09-final-scroll-wheel-right-0.jpg` 通过，水平滚到 539，全部模式/操作按钮可见 |
| 键盘自动滚动 | `08-final-keyboard-reaches-actions-0.jpg` 通过，标题 Shift+Tab 到删除；未删除 |
| 三模式交互 | 编辑 `10-final-narrow-edit-0.jpg`；方向键移动焦点后空格确认预览 `12-final-keyboard-preview-confirmed-0.jpg`；分栏 `16-final-english-narrow-split-0.jpg` |
| 英文目录标签和完整重启 | `14-final-english-wide.txt`、`14-final-english-wide-0.jpg`，5 个 UI 断言见 `english-ui-assertions.json`，原笔记和认证恢复 |
| 最终构建英文窄窗口 | `15-final-english-narrow-right-0.jpg` 所有操作可见；`16-final-english-narrow-split.txt`、`16-final-english-narrow-split-0.jpg` 点击 Split 后正文/预览同时存在 |
| 恢复中文并重开 | `17-final-chinese-restored.txt`、`17-final-chinese-restored-0.jpg` 当前目录、根目录与列表无障碍名称回到中文；原笔记保存禁用 |

最终命令沿用 SDK 8.0.426：`./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance`、`./clients/windows/build/build.ps1 -Configuration Debug -Platform x64`、`git diff --check -- clients/windows`。最终日志 `tests-final-localized.log`：常规 1,451 通过、0 失败，校验器通过，退出 0；`build-final-localized.log`：Rust Host/x64 Debug 0 警告、0 错误，退出 0。未运行机器条件分类或 PowerShell UI smoke；通过 Computer Use 单独进行真实回归。没有添加翻译/XAML 镜像单元测试，补充的是最终 UI 断言和可复现步骤。

原笔记 ID、三个字段和 UpdatedAt 保持不变，本轮未保存或删除笔记；临时英文偏好已恢复中文，客户端保留最大化、原笔记预览。完整 Mac 布局、可拖动分栏、短窗口/字体全部组合、目录模板重命名/删除名称、真实慢/失败/权限/无数据空目录、跨目录结果切换继续待验收。未选中笔记的英文提示本轮仅构建验证，不算真实空编辑区通过。下一轮按活动任务检查后继续跨目录或保存失败闭环。


## 2026-10-15 11:15：跨目录搜索选中状态与切换自动保存

### 范围、基线与修复

开始先读 automation memory；list_threads/read_thread 确认上一轮 01a13d57-6e38-7a40-86ae-7035da171532 completed，同任务无其他执行轮次后再编辑、构建和测试。本轮线程 01a13d8e-607e-7963-9c15-bc18b1592a94。保留所有 staged/unstaged/untracked 修改，修改前源文件与状态保存在本轮 evidence；未 reset/clean/commit/push，代码修改仅限 clients/windows。

Mac 基线：clients/macos/Sources/ChatOSApp/Features/Notepad/NotepadViewModel.swift 的 selectedNoteID/selectedTreeNodeID，以及 NotepadSheet.swift 的 note ID 选择；关键词跨目录命中，选择结果打开详情，清空返回当前目录。Windows 实机复现重新搜索后当前笔记仍在编辑区，但列表蓝色选中条消失：查询更新替换元数据实例，SelectedItem 仍指向旧对象。

NotepadViewModel 新增 SelectedResultNote，按编辑区笔记 ID 在当前 Notes 中取得结果实例；SelectedNote 和 ApplyFilter 更新时发送属性通知。NotepadPage 的列表绑定和点击后恢复均使用该属性。无匹配时列表选择为空，编辑缓冲区独立保留，不因恢复高亮覆盖未保存字段。新增 4 项执行回归：新结果实例/草稿保留，目录与全局搜索切换/清空，选择目标前自动保存/目标目录，空结果通知及恢复。

### 命令与结果

当前进程 DOTNET_ROOT 使用 C:/Users/17986/AppData/Local/ChatOS/build-tools/dotnet，PATH 前置此目录，SDK 8.0.426。执行：

~~~powershell
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~Notepad'
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
git diff --check -- clients/windows
~~~

记事本专项 **173 通过、0 失败**（tests-notepad.log）。最终常规 Core 27、API 46、Presentation 819、Connector 544、NetworkGuard 19，共 **1,455 通过、0 失败**；package/TRX/evidence verifier 用例通过（tests-final.log，退出 0）。最终 Rust Host/x64 Debug 构建 **0 警告、0 错误**（build-final.log，退出 0）。机器条件分类仍排除，不据此验收驱动、安装升级或真实权限。最终测试/构建后无产品代码变更。

### 真实 UI 矩阵

证据根目录：clients/windows/.local/parity/2026-10-15-1115/。通过 Computer Use 获取并查看截图，未混用 PowerShell UIAutomation。

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 根目录和子目录标题全局搜索 | 通过，同时显示两个目录的笔记 | 08-final-global-highlight.txt；重采 21-recheck-global-highlight.txt/-0.jpg |
| 正文关键词 CrossFolderBody1115 | 通过，仅新笔记命中，正文/标签恢复，选中蓝条存在 | 09-final-body-search-highlight.txt/-0.jpg |
| 标签关键词 CrossFolderTag1115 | 通过，保存后及完整重启后可查到新笔记 | 14-final-tag-saved-highlight.txt/-0.jpg、15-restart-tag-results.txt |
| 搜索刷新后结果高亮 | 通过，结果元数据刷新后蓝色选中条保留，编辑内容不变 | 09-final-body-search-highlight-0.jpg；重采 21-recheck-global-highlight-0.jpg |
| 无匹配保留草稿 | 通过，NoMatchCrossFolder1115 显示空状态，标题后缀草稿和未保存标记保留 | 10-final-empty-keeps-draft.txt/-0.jpg |
| 清空查询恢复当前子目录 | 通过，只剩子目录笔记，标题草稿仍在 | 11-final-clear-folder-draft.txt；重采干净状态 22-final-clear-folder.txt/-0.jpg |
| 点击另一目录结果 | 通过，自动保存新笔记标题后缀，再切换根目录旧笔记；回到新笔记已保存且 Save 禁用 | 12-final-switch-autosaves.txt、13-final-switch-settled.txt/-0.jpg、14-final-tag-saved-highlight.txt |
| 正常退出与完整重启 | 通过，11:35:38 +08 退出，11:35:52 检查 Desktop/Local Agent 均为零；重启恢复认证，查询并打开新笔记，三字段持久化 | exit-final.json、exit-final.log、20-current-restart-persisted.txt/-0.jpg、restart-ui-assertions.json（8 项通过） |

实际新建目录 Windows跨目录验收-20261015-1115；笔记 ID ec4d0dda-b983-40d6-a786-e37a320c5661，最终标题 Windows记事本验收-跨目录-1115-自动保存，标签“跨目录, CrossFolderTag1115”，正文标题“跨目录正文验收”、段落“CrossFolderBody1115 中文持久化。”。UpdatedAt 为 2026-10-15T03:33:51Z。保留此数据给后续轮次使用。

原笔记 614316c0-9ec1-4fb0-a623-544eb20e2035 标题仍为 Windows记事本验收-20261015-0915-关闭保存，UpdatedAt 仍为 2026-10-15T01:33:32Z。本轮没有保存或删除原笔记。结束留新笔记预览、搜索为空、当前子目录、Save 禁用。

### 证据校验与运行时恢复

早期 01–06 保存 helper 捕获旧的顶层 state 绑定，文本重复引用起始笔记；这些文件保留但全部排除，不能作为文件名描述场景的证据，特别是 05 不能作为修复前丢高亮截图。修复前复现依据当时实际查看的工具截图及源码定位，未保留有效的独立前置截图。07–16 文本已逐场景复核，支持上表；最终结论优先引用重新捕获的 20–22。evidence-validity.md 和 evidence-sha256.json 记录有效性及文件散列。

进程重启后一度出现 Computer Use “foreground window did not report a process id”；启动日志无应用崩溃，list_windows 仍返回主窗口。重置 Node 内核，重新导入 sky，按返回的唯一主窗口 get_window/activate_window 并获取截图后恢复。之后统一使用 globalThis.uiState，记录函数直接引用最新全局状态，避免闭包旧绑定。ui-recovery.log 已追加解决记录，当前没有此阻塞。

### 未完成与下一轮入口

仅勾选跨目录搜索正常流程/高亮/切换持久化窄项；完整搜索边界项继续 pending。下一轮先检查同任务执行状态，再复用上述两个笔记，优先做真实慢保存/失败保存、搜索期间关闭及重试；真实服务错误、权限拒绝、空目录加载、焦点和键盘选择尚未完整验证。Mac 树形目录/可拖动分栏、复制/导出/图片/删除及目录重命名仍待闭环；继续聊天/Task、Connector 互斥、项目文件、媒体剧情、审批及远端总矩阵。全部功能尚未通过，保持自动化 ACTIVE，不暂停。


## 2026-10-15 12:00：空目录创建路径、重命名校验及持久化

### 并发、保护和 Mac 对照

- 本轮线程 01a13db7-95f9-7ea1-adcf-cf3e0beefdc7。先读 windows memory；list_threads 无其他同任务 active，wait_threads(timeoutMs:0) 确认上一执行线程 01a13d8e-607e-7963-9c15-bc18b1592a94 的 turn completed。保留全部已有暂存、未暂存、未跟踪修改；没有 reset/clean/commit/push，没有修改 Mac/共享 Host。status-before 和修改前源文件保存在本轮证据目录。
- Mac NotepadViewModel.swift createFolder 对 input == base 不再拼接父目录；selectFolder 只切换浏览目录，保留独立编辑区。Windows 的当前完整路径漏掉等值判断。
- 真实输入当前完整目录路径，Windows 将其拼接两遍并创建两层额外目录，截图 03 有效；修复等值判断后，同一路径及含反斜杠/首尾分隔符/空白的规范化输入都保持原路径。保留复现产生的空目录，未删除数据。
- 错误重命名实机出现旧绿色成功提示与新红色错误同时显示（05）；新建和重命名校验前清理旧提示，新增空名称和非法后有效重试回归。创建入口同时在无有效会话或忙碌时直接返回。

### 命令与结果

使用当前进程 DOTNET_ROOT=C:/Users/17986/AppData/Local/ChatOS/build-tools/dotnet 并前置 PATH，SDK 8.0.426。

```powershell
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter FullyQualifiedName~CreateFolderResolvesRelativeAndCurrentPathsWithoutDuplicatingParent
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter FullyQualifiedName~Notepad
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
git diff --check -- clients/windows
```

- 新增路径理论 6 项，修改前 2 失败/4 通过；修复后记事本 179 通过。提示状态 2 项修改前均失败，修复后最终记事本 **181 通过、0 失败**。共新增 8 个执行用例，验证真实提交路径、保留草稿、不额外保存及错误后的重试，不使用 XAML/字符串镜像测试。
- 第一次常规 1,461 通过；发现实机提示残留后再修复并重跑。最终 tests-final.log：Core 27 + API 46 + Presentation 827 + Connector 544 + NetworkGuard 19 = **1,463 通过、0 失败**；package/TRX/三组 acceptance evidence 校验器通过，退出 0。机器条件分类仍排除，不能据此宣称机器验收完成。
- 最终 build-final.log：Rust Host/x64 Debug/WinUI/XAML **0 警告、0 错误**，退出 0。tests-first.log/build-first.log 保留中间版本；最终实机与重启使用提示修复后的最后构建。

### 真实 UI 及端到端矩阵

证据根目录 `clients/windows/.local/parity/2026-10-15-1200/`。全部操作使用 Computer Use；没有混用 PowerShell UIAutomation。每个编号的保存 helper 直接读取 globalThis.uiState；最终快照均为动作后重新观察，异步弹窗关闭期间的即时状态不作为完成证据。

| 项目 | 结果 | 证据 |
| --- | --- | --- |
| 子目录真实创建/无笔记 | 通过，列表显示暂无笔记，原笔记编辑区保留 | 02-created-empty-folder.txt/-0.jpg |
| 当前完整路径重复拼接 | 修复前真实复现；最终规范化同路径不新增，目录数仍为4 | 03-before-duplicated-current-path.txt/-0.jpg；06-final-same-path-keeps-draft.txt |
| 非法重命名 | 拒绝移入自身子目录，目录及草稿不变；最终只显示当前红色错误 | 05-invalid-rename-rejected.txt/-0.jpg 为修复前提示残留；07-final-invalid-rename-only-error.txt/-0.jpg 为最终 |
| 有效重命名重试 | 成功映射当前目录与其全部子目录，无关笔记草稿/未保存标记保留，错误清除 | 08-final-rename-subfolders-draft.txt/-0.jpg |
| 空名称新建及重试 | 空名称不创建且清除旧成功提示；再提交有效同路径恢复成功、错误清除 | 09-final-empty-name-only-error.txt、10-final-create-retry.txt |
| 返回根目录 | 根目录笔记结果恢复，标题草稿仍在 | 11-final-root-navigation-draft.txt |
| 放弃本轮临时草稿/关闭 | 使用既有确认放弃本轮自建临时标题草稿，返回工作区；未保存笔记 | 12-final-draft-discarded-workspace.txt |
| 正常退出与完整重启 | 12:28:08 +08:00 Desktop/Host 进程为0；重启认证与目录持久化，原标题/标签/正文及 UpdatedAt 恢复，Save 禁用 | exit-final.json、14-restart-root-restored.txt/-0.jpg |
| 重启后空目录 | 新路径及其子目录保留，进入后仍无笔记、原笔记编辑区可见且 Save 禁用 | 15-final-restart-empty-folder.txt/-0.jpg |

13-restart-loading.txt 捕获实际加载期搜索禁用，但没有人为延长请求，不能计为慢请求/所有加载状态通过。Computer Use 在关闭应用后返回 window is not a usable app window，是随后捕获已关闭窗口所致；进程退出以 JSON 为准。一次重启后的 coordinate input geometry is unavailable，重新选择唯一返回窗口并捕获截图后恢复，没有产品崩溃。ui-assertions.json 对上述最终证据执行 11 个状态断言，全部通过；证据散列另见 evidence-sha256.json。

### 保留数据与下一轮

- 保留本轮空目录 Windows空目录已重命名-20261015-1200，其下 Windows跨目录验收-20261015-1115/Windows空目录验收-20261015-1200 为修复前复现生成的两层空目录。原来根目录下 Windows跨目录验收-20261015-1115 及其中原测试笔记保持原路径，未重命名已有测试目录。没有新建、保存或删除笔记。
- 原笔记 614316c0-9ec1-4fb0-a623-544eb20e2035 的 UpdatedAt 仍为 2026-10-15T01:33:32Z，临时后缀已放弃；另一笔记 ec4d0dda-b983-40d6-a786-e37a320c5661 未编辑。客户端留重命名后的空目录，原笔记预览、空搜索、Save 禁用。
- 仅勾选空目录创建/同路径/校验/返回/重启及不含笔记的目录重命名窄项。完整目录树、选中高亮、可拖动分栏、含笔记目录重命名/版本冲突/刷新失败/权限、慢保存搜索及关闭仍待验收；复制/导出/图片/删除和聊天/Task、Connector 互斥、项目文件、Media/Story、审批、远端总矩阵继续 pending。
- 下一轮先检查同任务执行状态，优先选择含笔记目录重命名或真实慢/失败保存搜索闭环。全功能尚未完成，保持现有 15 分钟自动化 ACTIVE。

## 2026-10-15 12:45：目录选择、含笔记目录重命名及持久化

### 起始状态与修改

读取自动化记忆，list_threads 只有本轮 active；前执行线程01a13db7由wait_threads确认completed，前计划线程01a13dd3由read_thread确认completed。无并发编辑/构建/测试。保留所有已有修改，status-before.txt及FolderRename修改前副本保存在本轮证据目录；无重置、清理、提交、推送或Mac/共享Host改动。

Mac `NotepadViewModel.swift` 的 selectedTreeNodeID 在目录/笔记选择时更新；Windows 的 Folders 原为 SelectionMode=None，实机没有目录选中提示。增加 SelectedFolderItem 按路径返回当前集合中的实例，SelectedFolder变化时通知，根目录使用选中按钮。目录点击和原生SelectionChanged支持鼠标与键盘导航，目录浏览继续保留独立编辑区。Windows现有平铺目录/笔记双列表与Mac单树节点结构仍有差异；本轮只验收当前目录定位。

初版从Folders.CollectionChanged发选择通知，专项/全量测试通过，但实机重命名后再次丢失选中条。新增模拟后注册的集合消费者回归，在最后一项被选中时复现通知早于列表取得新实例（1项失败）。统一使用ReplaceFolders在整个集合替换结束后再通知，覆盖初始化/刷新、新建笔记目录重载、重命名和删除路径。本轮未操作删除。

Mac当前Notepad页面没有目录重命名UI；两端Connector提供相同rename服务能力。含笔记改名作为Windows已有扩展功能回归。只读参考Host存储实现确认改名递增笔记版本；真实改名后保存通过，未修改共享Host。

### 命令与结果

当前进程使用用户SDK 8.0.426：

```powershell
$env:DOTNET_ROOT='C:/Users/17986/AppData/Local/ChatOS/build-tools/dotnet'
$env:PATH="$env:DOTNET_ROOT;$env:PATH"
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~FolderNavigationNotifiesSelection|FullyQualifiedName~FolderRefreshPublishes|FullyQualifiedName~RenamePublishesMappedSelection'
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter FullyQualifiedName~FolderRefreshPublishesSelectionAfterCollectionConsumers
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter FullyQualifiedName~Notepad
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
git diff --check -- clients/windows
```

- tests-before.log：前三项3失败/0通过（通知缺失）；tests-order-before.log：消费者通知顺序1失败。两批失败均在对应修复前取得。
- 初版tests-notepad.log：184通过；tests-final.log：1,466通过；build-final.log：0警告/错误。这一版本实机仍丢选择，不计最终通过。
- 最终tests-final-order.log：Core27 + API46 + Presentation831 + Connector544 + NetworkGuard19 = **1,467通过、0失败**；package/TRX/acceptance evidence校验器通过，退出0。最终记事本tests-notepad-final.log **185通过**，退出0。
- 最终build-final-order.log：Rust Host/x64 Debug/WinUI/XAML **0警告、0错误**，退出0。正常退出客户端后构建，没有强制结束进程。git diff --check通过。
- 机器条件分类排除，未执行单独PowerShell UIAutomation smoke；所有真实UI由computer-use完成。

### 真实Windows验收矩阵

证据目录 `clients/windows/.local/parity/2026-10-15-1245/`；截图直接从每次新观察的globalThis.uiState保存并实际查看，文本断言另存ui-assertions.json（7组全部通过）。

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 原目录点击没有选择状态 | 复现；目录项目不支持选择 | 01-before-folder-no-selection.txt |
| 中间版本改名刷新后丢选择 | 复现，不计通过；促成通知顺序修复 | 06-final-renamed-draft-highlight-0.jpg |
| 最终版本刷新保持目录选择 | 通过，目录和笔记蓝色条均保留 | 08-order-fixed-refresh-highlight-0.jpg |
| 含笔记目录改名与当前草稿 | 通过，新目录/笔记路径一致，标题草稿和未保存标记保留，目录蓝条正确 | 09-order-fixed-renamed-draft-highlight.txt/-0.jpg |
| 后端目标目录已存在错误与有效重试 | 通过，错误后路径/草稿未变，有效改名成功清除错误 | 05-final-existing-target-error.txt/-0.jpg、06文本；功能代码未因第二次选择修复改变 |
| 改名后的版本与保存 | 通过，保存无版本冲突，标记消失且保存禁用 | 10-order-fixed-save-complete.txt |
| 根目录返回与重复点击 | 通过，根目录选中且结果为根笔记，当前编辑笔记保留 | 11-root-click-observed.txt/-0.jpg；重复点击后12返回目录 |
| 键盘上下导航、空目录 | 通过，Tab/Shift+Tab聚焦后Down进入相邻空目录，Up返回含笔记目录；正文保留 | 13焦点框、14-keyboard-down-empty-folder.txt/-0.jpg、15-keyboard-up-restored-folder.txt |
| 完整退出 | 通过，13:06:12 +08:00 Desktop与Local Agent进程均为0 | exit-final.json |
| 重启后认证与数据恢复 | 通过，新目录/笔记路径、最终标题、原标签/正文恢复，预览、Save禁用，目录/笔记蓝条仍在 | 17-final-restart-persisted.txt/-0.jpg |
| 原根笔记不受影响 | 通过，ID/标题/UpdatedAt仍01:33:32Z | 16-restart-root-original-unchanged.txt |
| 真实权限拒绝、慢请求、并发版本冲突、改名后刷新失败 | 未执行，不标通过 | 保留总清单待验收 |

05的冲突错误原样为storage conflict，Mac同样将服务错误直接呈现；本项验收拒绝/重试行为，不据此宣称错误文案已全面本地化。IsLoading仅自然短暂出现，没有注入慢请求，不声称加载全组合通过。Computer Use的focused_element持续显示旧搜索框；方向键验收依据可见焦点框、路径和结果真实变化，未依赖该旧字段判断焦点。关闭之后刷新已退出窗口会返回window is not a usable app window，由进程数记录证明正常退出。

### 数据与下一轮入口

- 复用测试笔记ec4d0dda-b983-40d6-a786-e37a320c5661。其目录由Windows跨目录验收-20261015-1115改为Windows含笔记已重命名-20261015-1245，中间为重试回归改回原路径后再最终改名；最终标题Windows记事本验收-跨目录-1115-目录改名保存1245-终验，UpdatedAt 2026-10-15T05:00:43Z。标签和正文未编辑，没有创建/删除目录或笔记。
- 根笔记614316c0-9ec1-4fb0-a623-544eb20e2035及原空目录子树未改。最终客户端运行、打开已保存测试笔记预览，搜索为空、Save禁用；仅主动打开目标验证数据恢复，不声称上次浏览位置会跨进程自动恢复。
- 最终证据优先08起及tests-final-order.log/build-final-order.log；01–07记录早期复现和中间状态。通知顺序修复后无其他产品代码修改。只新增当前目录定位、含笔记正常改名/冲突重试/保存恢复两个窄范围通过项；综合布局和重命名矩阵保持待验收。
- 下一轮先检查同任务并发；优先真实慢/失败保存与搜索、关闭待处理搜索、真实并发版本冲突与刷新失败。完整Mac树/拖动分栏、权限、图片/复制/导出/删除以及聊天Task、Connector、项目文件、Media/Story、审批、远端继续总矩阵。全功能未完成，保持自动化ACTIVE。


## 2026-10-15 13:15：无效保存选择恢复与真实写锁失败重试

### 起始状态、对照与修复

读取 windows 自动化记忆，检查同任务线程，wait_threads 确认上一执行01a13de0-cb4e-7e83-b87f-c5755057e7dd已completed后开始。本轮线程01a13dfc-455b-7fb2-8ce3-524181b895ac，无并发编辑、构建或测试。保留所有已有暂存/未暂存/未跟踪改动，修改前status和源文件副本在本轮证据；无reset/clean/commit/push、Mac或共享Host修改。

真实UI复现标题为空保存显示英文notepad title must not be empty。SaveCoreAsync在当前会话/忙碌保护后，使用IsNullOrWhiteSpace本地拒绝并显示“笔记标题不能为空。”，清理旧成功提示，不发送更新，不改变标题/标签/正文及脏状态，允许纠正后重试。新增4种空白与2个自动保存入口（切换/新建）回归，覆盖三字段保留、无非法请求、成功重试规范化及旧提示清理，6项修复前全部失败。

对照Mac NotepadViewModel.swift约235–270的selectNote/save失败恢复selectedTreeNodeID和300–355的保存缓冲行为。Windows实机进一步发现同步空标题校验失败后，编辑区留在原笔记，但原生ListView蓝条仍移到点击目标。直接在ItemClick里恢复SelectedItem早于原生选择更新；修改NotepadPage.OnNoteClick，await命令后通过DispatcherQueue.TryEnqueue恢复SelectedResultNote。此原生事件顺序问题由最终实机截图验证，未用只覆盖ViewModel的单测冒充验证。

### 命令与结果

当前进程SDK8.0.426，DOTNET_ROOT为C:/Users/17986/AppData/Local/ChatOS/build-tools/dotnet，并仅追加当前进程PATH。

~~~powershell
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~BlankTitle'
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter 'FullyQualifiedName~Notepad'
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
git diff --check -- clients/windows
~~~

- tests-before.log：新增6项0通过/6失败；tests-notepad.log：记事本191通过、0失败。
- 最终tests-final-selection.log：Core27 + API46 + Presentation837 + Connector544 + NetworkGuard19 = **1,473通过、0失败**，证据校验器通过，退出0。
- 最终build-final-selection.log：Rust Host/x64 Debug/WinUI/XAML **0警告、0错误**，退出0。无selection后缀日志为选择时序修复前中间版本。
- 机器验收分类排除，未执行独立PowerShell UIAutomation smoke；实机由computer-use完成。文档完成后执行diff --check，不因已有无关修改重新跑全套。

### 真实Windows验收矩阵

证据目录 clients/windows/.local/parity/2026-10-15-1315/。均为真实客户端观察，截图实际查看；ui-assertions.json核对10组保存UI/数据库证据，选择条由截图人工核验。

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 修改前空标题保存 | 复现原始英文错误 | 02-before-empty-title-save.txt/-0.jpg |
| 本地校验与草稿 | 中文错误、未保存标记、正文标签保留、仍可保存 | 03截图及04-local-validation-settled.txt |
| 关闭选择保存遇校验失败 | 仍留记事本原草稿，不关闭 | 05-invalid-close-confirm与06-invalid-close-keeps-draft |
| 校验失败后点击其他笔记 | 修改前蓝条错移，最终原笔记蓝条正确且编辑草稿不变 | 11截图为问题，13-final-invalid-navigation-selection-0.jpg为最终通过 |
| 当前无效草稿下新建提交 | 拒绝保存并留当前草稿，无新增笔记 | 14-final-invalid-create-blocked.txt及最终SQLite快照 |
| 最终真实保存等待 | 忙碌图标、保存/关闭/刷新/搜索/新建禁用，标题标签仍可编辑，草稿保留 | 15-final-saving-busy.txt/-0.jpg |
| 实际写锁失败 | 约5秒后(code: 5) database is locked，草稿/未保存仍在，保存和关闭恢复，不显示旧成功 | 16-final-real-save-failure.txt |
| 解除锁后重试 | 成功提示、错误与未保存标记清除、保存禁用、标题/标签/正文正确 | 17-final-retry-saved.txt |
| 正常笔记导航 | 根笔记和测试笔记可切换 | 18-final-normal-navigation.txt |
| 正常完整退出 | 13:41:30 +08:00 Desktop与Host均0 | exit-final.json |
| 完整重启 | 恢复认证，主动打开目标后标题/标签/正文一致、目录/笔记蓝条、预览及保存禁用 | 19-final-restart-persisted.txt/-0.jpg |
| 原数据与新增防护 | 原根笔记整行不变，总笔记2条；仅目标标题/version/时间改变 | persistence-before.json及persistence-final.json |

写锁辅助脚本hold-writer.py只对现有SQLite连接执行BEGIN IMMEDIATE，占用写锁而不执行行修改，release文件或90秒上限均走finally rollback/close。第一次和最终构建分别执行了一次，failure-lock.json/final-lock.json记录占锁与释放时间；两次已结束，未留下辅助写锁。读写数据库路径为C:/Users/17986/AppData/Local/ChatOS/LocalAgent/local-agent.sqlite3。通过客户端正常保存验证，不修改共享Host、不伪造服务错误。

数据库错误目前保留原始英文诊断，不宣称服务错误全面本地化。保存等待期间仅观察字段可编辑，未在等待中继续输入，不勾选该竞态。真实版本冲突、权限拒绝、慢/失败搜索和搜索期间关闭未执行。

### 数据保留、证据限制与下一轮

- 复用ec4d0dda-b983-40d6-a786-e37a320c5661，路径Windows含笔记已重命名-20261015-1245；最终标题Windows记事本验收-跨目录-1115-失败重试1315-终验，version10（原8，两次解除锁后的成功重试），UpdatedAt2026-10-15T05:39:02Z。正文、标签、目录与修改前相同。
- 根笔记614316c0-9ec1-4fb0-a623-544eb20e2035整行未变，version4、UpdatedAt2026-10-15T01:33:32Z。没有创建、删除笔记/目录。客户端留目标笔记预览，中文、最大化、空搜索、Save禁用。
- 01–11含修改前和中间版本，11明确是错误选择条。最终选择修复后的12–19及带selection的测试/构建日志优先。03即时文本早于渲染，中文错误用04文本；14即时可访问性仍保留新建弹窗输入，不用于断言弹窗消失，无创建以列表和持久化快照交叉验证。19是点击后独立重采的稳定帧。
- focused_element残留旧搜索框，不据此判断焦点。重启后coordinate geometry unavailable通过唯一已返回窗口重新选择/截图恢复；关闭后窗口不可用由零进程交叉验证，不作为产品缺陷。证据说明见evidence-validity.md，目录附SHA256清单。
- 只新增空标题/同步失败选择保护与真实写锁失败恢复两个窄项；综合新建、保存、搜索及布局矩阵继续pending。下轮先检查同任务并发，优先保存等待期间继续输入、真实慢/失败搜索及关闭、真实并发版本冲突。权限、树/拖动分栏、复制/导出/图片/删除，聊天/Task、Connector互斥、项目文件、Media/Story、审批及远端仍待完成。
- 全功能验收未完成，保持现有自动化ACTIVE，未暂停。

文档更新后于2026-10-15 13:48 +08:00执行 git diff --check -- clients/windows，退出0，无空白错误；辅助Python进程查询为空。最终产品代码未在测试/构建后变更。

## 2026-10-15 14:00：保存中输入与三字段元数据快照保护

本轮先确认上一同任务已结束，保留全部工作区修改。对照Mac实际截图20-notebook.png及NotepadViewModel.swift:303后的save：只有标题、标签、正文全部仍匹配提交快照时才接受规范化元数据。Windows原先按字段独立判断，实机在正文变化后仍替换标签输入；现在统一判断三字段，保留整个编辑缓冲，独立记录已保存快照供IsDirty比较。修改仅NotepadViewModel.cs及NotepadSavingTests.cs。

### 命令与结果

使用LOCALAPPDATA/ChatOS/build-tools/dotnet SDK8.0.426，临时设置当前命令DOTNET_ROOT/PATH：

```powershell
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter FullyQualifiedName~EditingAnyFieldDuringSaveDefersAllMetadataNormalization
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --nologo --filter FullyQualifiedName~Notepad
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
python clients/windows/.local/parity/2026-10-15-1400/snapshot.py persistence-final
python clients/windows/.local/parity/2026-10-15-1400/verify-evidence.py
git diff --check -- clients/windows
```

- 新增3项在修改前全失败，覆盖修改任一字段时不改写其他字段、只提交旧快照、保留dirty与提示、再次保存时恢复规范化。修复后记事本194通过。
- 最终正常测试Core27/API46/Presentation840/Connector544/NetworkGuard19，共1476通过、0失败，所有证据校验器通过、退出0。机器验收分类仍排除；没有将其标为通过。
- 最终Rust Host与x64 Debug构建0警告/错误、退出0，日志tests-final.log/build-final.log。未修改XAML/页面结构，最终真实UI回归直接使用本轮构建；未另跑匿名UI smoke。
- 本轮辅助证据断言10组全部通过（ui-assertions.json），不是产品单元测试数量。最终产品代码在测试/构建后未改动。

### 真实UI矩阵

| 场景 | 实际结果 | 保存证据 |
| --- | --- | --- |
| 修改前真实写锁失败后输入 | 真实database is locked、输入/草稿可用；输入已晚于失败，不计保存期间输入 | 06/07、edit-failure-lock.json |
| 第一次55秒延迟 | Host兜底恢复早于输入，已保存提示下新输入仍dirty；不能计并发输入通过 | 11–13、before-host.json |
| 修改前稳定并发输入复现 | Host暂停时Ctrl+S，保存disabled时输入正文；恢复Host后正文保留但原始重复标签被规范化，与Mac不同 | 16/17/18、before-second-host.json |
| 最终保存忙碌状态 | 正文实际输入，保存/关闭/刷新/搜索/新建禁用；原始标签与正文同时可见 | 30/31及31截图 |
| 最终响应保留整份输入 | 新正文保留，标签仍为跨目录，CrossFolderTag1115， 跨目录；未保存与可再次保存保留，提示先前修改已保存 | 32及截图 |
| 首次提交只写旧快照 | version14不含最终保存期间输入1400；数据库标签已规范化，编辑器仍保留原始输入 | persistence-snapshot-saved.json |
| 再次Ctrl+S | 新正文写入version15，标签规范化，未保存消失，Save禁用 | 33/34、persistence-after-retry.json |
| 正常退出 | 2026-10-15 14:16:11 +08:00 Desktop/Host均0 | exit-final.json |
| 完整重启与手动导航 | 认证恢复，打开目录/目标后标题、标签、新正文、Preview及目录/笔记选中条正常，Save禁用 | 35–39，最终稳定39截图 |
| 数据保留 | 原根笔记整行未变，仅现有测试笔记变更，无新增/删除；重启后无额外写入 | before/after-retry/final.json及断言 |

延迟注入在外部测试辅助脚本pause-host.py中，使用psutil定位唯一此工作区Desktop的直接子Host后临时suspend；release标记或180秒兜底均走finally resume。最终仅暂停约54秒，客户端实际请求仍在75秒期限内，通过原有Host执行真实保存。三次暂停已全部恢复，SQLite辅助写锁已rollback/close，没有修改数据库行、程序超时或返回值，没有修改共享Host。辅助Python进程最终查询为空。55秒第一次尝试失败只记录为证据限制，不将其当通过。

### 最终数据与限制

复用ec4d0dda-b983-40d6-a786-e37a320c5661，目录Windows含笔记已重命名-20261015-1245，最终标题Windows记事本验收-保存快照1400，version10到15（本轮5次UI成功保存），UpdatedAt2026-10-15T06:15:45.868Z。保留原正文并增加本轮输入测试串，标签最终语义不变。原根笔记614316c0-9ec1-4fb0-a623-544eb20e2035整行未变，version4/01:33:32Z。无新增/删除笔记或目录，客户端留已保存目标预览，中文、最大化、空搜索。

01–19为修改前，最终优先30–39。31截图证明仍在saving时输入，32证明未重写元数据，39是点击后重新观察的稳定帧。07和12不作为并发输入证据。focused_element可能是旧搜索字段；输入目标由实际点击正文和正文变化交叉验证，不依赖该字段。coordinate geometry unavailable通过唯一返回窗口重选/截图恢复。首次只读snapshot.py表名写错导致no such table，改成local_notepad_notes后所有快照通过；控制台中文输出编码失真不影响UTF-8证据。未宣称自动恢复最后位置。

仅正文输入与原始标签保护有本轮最终实机证据，标题/标签改变组合为单元测试。失败期间输入、自动保存导航期间新输入、真实并发版本冲突、慢/失败搜索与关闭、权限继续待验收。空状态、加载错误、Mac完整树/分栏与所有其他功能不能由本次保存回归推断通过。下一轮先检查并发，优先上述竞态或聊天真实回复闭环，继续Connector互斥、项目文件、Media/Story、审批和远端。全功能未完成，自动化保持ACTIVE。
