# Windows 项目文件验收

本文件记录项目文件工作区的真实 Windows 客户端验收。综合能力只有在正常、错误、空、加载、权限、导航、持久化和重启行为全部通过后才能完成；单个窄场景通过不会把总矩阵提前标记完成。

## 2026-10-16：搜索空状态、文件打开与退出清理

证据目录：`.local/parity/2026-10-16-0100/`。

### 本轮修复

- 实机复现非空关键词无命中时搜索面板消失、界面退回目录列表。`ProjectFilesViewModel` 现在以查询是否非空判断搜索面板是否激活，并在搜索结束且两类结果都为空时公开 `HasNoSearchResults`。
- 搜索面板增加加载态与中英文无命中文案；清空搜索是退出搜索态并恢复目录的唯一显式入口。
- “切换目录视图”改用运行时本地化；清空搜索按钮补充稳定 AutomationId 和无障碍名称。
- `DesktopShutdown` 在独立 worker 上调用每个清理委托，使“返回 Task 前同步阻塞”的清理也受单步 3 秒期限保护；异常或超时后继续执行后续清理和进程退出。

### 自动化与构建

- 项目文件专项：139 通过、0 失败；新增空搜索保持搜索面板、清空恢复目录的回归。
- 退出清理专项：5 通过、0 失败；新增同步阻塞清理超时后继续的回归。
- 常规套件：Core 27、API 46、Presentation 841、Connector 545、NetworkGuard 19，共 1,478 通过、0 失败。
- 最终 x64 Debug/Rust Host 构建：0 警告、0 错误。

### 真实 UI 矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| `repository` 正常搜索 | 文件名/内容结果显示，搜索面板保持 | `01-search-repository.png` |
| 中文无命中 | 保持搜索面板，显示“未找到匹配的文件或内容”，可清空 | `02-fixed-empty-search-zh.png` |
| 英文无命中 | 保持搜索面板，显示 `No matching files or content`，按钮和目录视图文案可读 | `03-fixed-empty-search-en.png` |
| README 命中与打开 | 真实结果可打开，Markdown 预览恢复 | 本轮实机操作记录 |
| 清空与重启 | 清空恢复目录；完整退出后 Desktop/Host 为 0，重启恢复认证与项目 | 本轮实机操作及 `startup.log` |

真实退出从此前残留进程恢复为约 0.26 秒完成。测试期间临时切换英文，随后已恢复中文。未删除或改写项目文件。

### 仍未完成

- 慢搜索、单路/双路服务失败、取消与关闭期间的真实 UI 状态。
- 权限拒绝、只读目录、刷新失败恢复、排序和大目录滚动。
- 完整树展开/折叠、键盘焦点、重命名/删除/复制/移动与重启后的真实文件持久化。
- 代码定义/引用、Git 和 Run 页面仍按各自清单继续验收。

## 2026-10-16 16:15–16:38：功能页返回、文件 CRUD 与重启恢复

证据目录：`.local/parity/2026-10-16-1615/`，并复用 `.local/parity/2026-10-16-1530/` 的详细 CRUD 快照。

### 实机缺陷与修复

- 从 Git 返回文件页后，界面只切换回旧 `ProjectFilesPage`，但 `OpenProjectTabAsync` 没有 `files` 分支，文件会话仍处于关闭态。真实表现为目录和树空白、`README` 搜索无结果，创建目录弹窗提交后静默无效。现返回文件页会关闭 Run/Git 并用当前项目重新打开 `ProjectFilesViewModel`。
- 重命名选中的 `draft.txt` 后，目录与磁盘已经变为 `renamed.txt`，正文区也进入未选择状态，但标题栏仍残留旧文件名和路径。现标题绑定到 `SelectedFileName` / `SelectedFileDisplayPath` 派生属性，`SelectedFile` 清空时同步清空标题。
- 新增壳层往返和标题属性清理 2 项回归。

### 真实 UI 矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 修复前 Git→文件 | 空目录、无 `README`、搜索无结果、创建静默无效 | `02-files-loaded.png`、`05-search-readme-tree.txt`、`10-after-create-tree.txt` |
| 修复后 Git→文件 | 立即恢复 `.git`、`README.md` 和可写会话 | `15-files-reopened-tree.txt`、`49-final-files-reopened-tree.txt` |
| 创建与保存 | 创建 `ui-crud-20261016/draft.txt`，编辑保存唯一标记及第二行，磁盘逐步核对 | `19-folder-created`、`25-file-created`、`28-file-saved` |
| 命名校验 | `../outside.txt` 时“创建”禁用，取消后无越界文件 | `41-invalid-name-tree.json`、`42-invalid-cancelled` |
| 重命名与标题恢复 | 文件变为 `renamed.txt`、内容不变；最终构建刷新后标题不再残留 | `32-renamed.png`、`37-renamed-open-tree.json`、`38-refresh-clears-header-tree.txt` |
| 删除 | 文件和空目录均经确认删除，原 `README.md` 保留 | `44-file-deleted`、`47-folder-deleted` |
| 完整重启 | 认证、Host、项目和 `README.md` 恢复；已删除目录在 UI/磁盘均不存在；Git→文件仍正常 | `53-final-files.png`、`final-restart-assertions.json` |

### 自动化、构建与剩余项

- 最终常规测试：Core 27、API 46、Presentation 853、Connector 556、NetworkGuard 19，共 1,501 通过、0 失败；package/TRX/acceptance verifier 通过。
- 最终 SDK 8.0.426 Rust Host/x64 Debug 构建 0 警告、0 错误；最终客户端保持运行、已认证、Host 在线并打开隔离项目文件页。
- 本轮只删除自己创建的隔离目录/文件，未删除用户数据或原仓库文件。综合文件操作行仍开放：还需真实复制/移动、非空目录/树内嵌套操作、只读/权限拒绝、慢请求及“操作成功但刷新失败”的恢复矩阵。

## 2026-10-16 16:45–17:17：项目创建、弹窗可达性与重启恢复

证据目录：`.local/parity/2026-10-16-1645/`。

### 实机缺陷与修复

- 修复前项目创建表单把内容固定为 700 DIP，而 WinUI `ContentDialog` 的真实内容视口更窄；隐藏目录、新建文件夹、刷新以及新建文件夹的“创建/取消”均被裁到右侧，UI Automation 返回 offscreen。现表单最大宽度为 700 并随视口伸缩，目录工具放到独立响应式行，最终三个工具和文件夹操作按钮均可见。
- 为项目创建根、工作区、父目录、目录工具、文件夹表单、列表及项目名增加唯一 AutomationId，纳入桌面契约。
- 非法文件夹名虽然原本安全拒绝，但错误只出现在表单底部，当前视口不可见。现同时在文件夹输入框下方发布可见且可访问的错误；`../outside-final-visible` 实机显示“文件夹名称不能为空，也不能包含 / 或 \\。”，磁盘没有越界目录。
- 新增 3 项非法目录名回归，并为弹窗响应式宽度、稳定控件和可达操作增加桌面契约。

### 真实 UI 矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 修复前工具可达性 | 目录工具及文件夹创建/取消被裁掉，只能依赖回车绕过 | `07-new-folder-panel.png` |
| 修复后工具可达性 | 隐藏目录、新建文件夹、刷新和文件夹创建/取消全部可见、可访问 | `18-dialog-responsive.png`、`19-new-folder-panel.png` |
| 新建文件夹与项目 | 在 `C:\Users\17986` 创建隔离目录并注册同名项目，创建后立即打开空文件页 | `21-folder-create-attempt`、`22-folder-selected`、`23-project-created.png` |
| 项目切换 | 新项目 → 既有 Git 项目 → 新项目往返成功 | `24-switched-existing.png`、`25-switched-back-new.png` |
| 命名错误与取消 | 非法路径不落盘，错误紧邻输入框；文件夹取消与整个弹窗取消均恢复主界面 | `44-final-visible-validation.png`、`45-folder-cancelled`、`46-dialog-cancelled` |
| 重启恢复 | 认证、Host、隔离目录和项目列表恢复，新项目可重新打开且无可见错误 | `47-final-project.png`、`final-verified-assertions.json` |

### 自动化、构建与剩余项

- 创建项目专项 33/33，桌面契约 25/25；最终常规 Core 27、API 46、Presentation 856、Connector 557、NetworkGuard 19，共 1,505 通过、0 失败；package、TRX 与 acceptance verifier 通过。
- SDK 8.0.426 Rust Host/x64 Debug 最终构建 0 警告、0 错误；正常退出证据为 `09`、`13`、`16`、`26`、`32`、`40`，最终客户端保持运行、已认证、Host 在线并打开隔离项目。
- 隔离目录和本地项目保留给后续复制/移动、只读/权限、刷新失败和 Run 回归。完整复制/移动、嵌套树菜单、慢请求及操作成功后刷新失败仍保持开放。
