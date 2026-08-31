# Windows Git 与记事本验收

## 构建

在 Windows 11、Visual Studio 2022 Build Tools 和 .NET 8 SDK 环境执行：

```powershell
dotnet restore ChatOS.Windows.sln
dotnet build ChatOS.Windows.sln -c Debug -p:Platform=x64
dotnet test ChatOS.Windows.sln -c Debug -p:Platform=x64 --no-build
```

ARM64 设备或交叉打包时把 `x64` 改为 `ARM64`。macOS 只能完成 Core/API/Presentation/Connector 编译；WinUI XAML 编译器会调用 Windows `kernel32.dll`，因此 Desktop 的最终编译必须在 Windows 执行。

## Git

1. 安装 Git for Windows，确认 PowerShell 中 `git --version` 可用。
2. 打开一个绑定到当前 Windows Connector 工作区的项目，进入项目顶部 `Git`。
3. 对非仓库目录执行初始化，确认只新增 `.git`，项目文件不改变。
4. 新建、修改、删除和重命名文件，确认工作区状态、暂存状态和两类 Diff 分开显示。
5. 暂存并提交；未配置 `user.name`/`user.email` 时应显示可操作错误，配置后提交成功并进入历史。
6. 创建、切换和合并分支。制造会被覆盖的未提交修改后切换分支，应被 Git 拒绝且文件内容保持不变。
7. 添加、编辑、重命名和删除远端。拉取只允许 fast-forward；无 upstream 的首次推送自动为当前分支设置 upstream。
8. 在拉取或推送期间点击取消，确认 UI 恢复、Git 子进程停止、可再次刷新。
9. 使用包含空格和中文的仓库路径、文件名与分支名复测；再在未安装 Git 的机器确认安装提示。

## 记事本

1. 点击主窗口标题栏的记事本按钮，确认直接进入记事本而不是空白占位。
2. 新建多级目录和笔记，搜索标题、标签或正文；选择和刷新后当前笔记保持稳定。
3. 修改正文后直接切换另一篇笔记，确认旧笔记先保存；保存失败时不得切换并丢失输入。
4. 默认打开为预览；编辑和分栏模式可输入，标题、列表、引用和代码块有基本 Markdown 语义。
5. 重命名或递归删除目录前显示明确确认；删除笔记后编辑区清空。
6. 导出为 `.md`，用记事本或 VS Code 打开确认 UTF-8 中文和换行正确。
7. 关闭记事本后回到此前工作区，项目/联系人选择不变化。
