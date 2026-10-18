# Windows 项目 Git 验收记录

更新时间：2026-10-16 16:00 +08:00

## 本轮范围

以 macOS `ProjectGitWorkbenchView`、`ProjectGitViewModel`、`NativeProjectGitService` 和 `NativeProjectGitParser` 为基准，在隔离的真实 Git 仓库中验收 Windows 的状态、工作区/暂存 Diff、暂存、提交、分支创建与切换、合并冲突提示、失败恢复和重启持久化。

验收仓库为 `C:\Users\17986\source\ChatOS-Git-Parity-20261016-1530`，只包含本轮创建的 fixture。该项目和仓库保留用于后续项目 Run/文件操作回归；没有修改、重置或清理用户既有仓库。

## 发现与修复

修复前，在 main 与 `feature/windows-parity` 对同一行产生不同提交后，从客户端合并会让真实仓库进入 `UU README.md`，但页面仍显示合并前的“工作区没有修改”。手动刷新后才出现 `Conflicted / 暂存 U / 工作区 U`。

原因有两处：

1. Windows `ProjectGitViewModel.MutateAsync` 仅在 mutation 成功后刷新 snapshot；合并等命令可能失败但已经改变索引/工作区。macOS 在失败时也会重新读取 snapshot。
2. Git for Windows 的合并冲突诊断写在 stdout，原 `GitProcess` 只把 stderr 交给错误映射，因此无法识别 `CONFLICT` / `Automatic merge failed`。

修复后，失败 mutation 会尽力读取并应用权威 snapshot，同时保留原始失败；命令失败诊断会安全合并 stderr/stdout，继续执行凭据脱敏和输出截断规则。最终合并失败无需手动刷新即可显示冲突文件及中文冲突说明。

## 真实客户端矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 干净仓库、当前分支、提交历史、无远端 | 通过 | `07-git-clean.png` |
| 工作区修改与只读 Diff | 通过，精确显示新增行 | `08-working-change`、`09-working-diff` |
| 暂存、暂存 Diff、提交 | 通过，提交 `Windows UI acceptance commit` 后恢复干净 | `11-staged`、`12-staged-diff`、`15-committed` |
| 新建并切换分支 | 通过，`feature/windows-parity` 成为当前分支 | `16-branch-dialog`–`18-branch-created` |
| 分支各自提交与切回 main | 通过 | `22-feature-committed`、`23-main-switched`、`27-main-committed` |
| 合并确认 | 通过，明确说明会保留冲突且不丢弃本地修改 | `28-merge-confirm` |
| 修复前冲突状态 | 复现：真实 Git 为 `UU`，UI 保留旧干净状态；手动刷新后显示冲突 | `29-merge-conflict`、`30-conflict-after-manual-refresh` |
| 修复后冲突状态 | 通过，无需刷新即显示 `README.md / Conflicted / U/U` 和中文错误 | `36-final-conflict-auto.png`、`final-conflict-assertions.json` |
| 冲突 Diff | 通过，工作区显示三段冲突标记，暂存侧显示 `* Unmerged path README.md` | `37-conflict-working-diff`、`39-conflict-staged-diff`、两个 `conflict-*-diff.txt` |
| 中止本轮测试合并并刷新 | 通过，恢复干净 main | `41-conflict-recovered` |
| 正常退出、完整重启、重新打开项目 | 通过，认证/Host/项目/两个分支/4 条提交/干净状态恢复 | `42-final-exit-result.json`、`44-final-git-restored.png`、`final-restart-assertions.json` |

## 自动化与构建

使用 SDK `8.0.426`：

```powershell
dotnet test clients/windows/tests/ChatOS.Connector.Tests/ChatOS.Connector.Tests.csproj -c Debug --filter "FullyQualifiedName~WindowsProjectGitServiceTests|FullyQualifiedName~ProjectGitParserTests"
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj -c Debug --filter "FullyQualifiedName~ProjectGitViewModelTests"
./clients/windows/build/test.ps1 -Configuration Debug -SkipMachineAcceptance
./clients/windows/build/build.ps1 -Configuration Debug -Platform x64
```

- Git 专项：Connector 6 + Presentation 8，共 14 通过、0 失败。
- 常规套件：Core 27 + API 46 + Presentation 852 + Connector 555 + NetworkGuard 19，共 1,499 通过、0 失败。
- package、TRX 和 acceptance evidence verifier 通过。
- Rust Host/x64 Debug 构建 0 警告、0 错误。
- MachineAcceptance 继续按既有口径排除，没有把机器条件测试计入完成结论。

## 保持开放的相邻项

项目创建的父目录/新建文件夹/完整命名错误矩阵、文件写操作、Git 远端网络拉取/推送、Run 配置与生命周期仍按各自行保持开放。本页只支持勾选“Git 状态、提交、分支、差异和冲突提示”，不扩张为整个项目工作区完成。
