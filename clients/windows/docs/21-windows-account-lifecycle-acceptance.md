# Windows 账号生命周期验收

更新时间：2026-10-16 14:27 +08:00

本记录覆盖 Windows 原生客户端的真实登出、未认证重启、同账号重新登录、资源恢复及再次启动。macOS 对照入口为 `RootView`、`SettingsView` 与 `AuthenticationViewModel` 的登出、凭据清理和新登录等待语义。

## 本轮实现

- 为账号菜单“退出登录”增加稳定 `ChatOS.Shell.SignOut` AutomationId，并绑定同一中英文可访问名称。
- 将该 ID 纳入桌面 XAML 唯一性、完整性与显式可访问名称契约。
- 未改变认证、资源或退出业务逻辑；实机结果确认现有账号范围取消、Local Agent Host 停止、凭据清理和资源重载链路可用。

## 真实客户端矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 已认证账号菜单登出 | 通过；工作区/账号标签清除，登录页显示，账号/密码为空，提交禁用，Host 为 0 | `02-after-logout*`、`11-final-logout-by-id*` |
| 未认证关闭与重启 | 通过；关闭后 Desktop/Host 为 0，重启仍停留登录页，无旧会话恢复 | `03-logout-exit-result.json`、`04-restart-still-logged-out*` |
| 同账号重新登录 | 通过；认证成功，无登录错误，联系人/项目/本机/远端恢复，Host 为 1 | `05-login-ready*`、`06-login-complete*`、`12-final-relogin*` |
| 真实项目重开 | 通过；`ChatOS 仓库管理` 及目录、聊天、运行、文件、Git 入口可达 | `07-project-reopened*`、`15-final-project-restored*` |
| 已认证正常退出与再次启动 | 通过；退出后 Desktop/Host 为 0，重启恢复认证、Host 和项目文件树 | `13-final-exit-result.json`、`14-final-restart*`、`15-final-project-restored*` |
| 稳定登出自动化入口 | 通过；最终构建以 `ChatOS.Shell.SignOut` 直接调用原生菜单项 | `10-final-account-menu*`、`11-final-logout-by-id*` |

截图 `11-final-logout-by-id.png`、`12-final-relogin.png`、`15-final-project-restored.png` 已目视核对，分别对应空登录页、资源恢复后的主窗口和重启后的真实项目文件树。

## 自动化、构建与命令

- `dotnet test ...ChatOS.Connector.Tests.csproj --filter FullyQualifiedName~DesktopAutomationContractTests|FullyQualifiedName~ClientOwnedShellTests`：41/41 通过。
- `dotnet test ...ChatOS.Api.Tests.csproj --filter FullyQualifiedName~AuthenticationServiceTests`：7/7 通过。
- `build/test.ps1 -Configuration Debug -SkipMachineAcceptance`：Core 27、API 46、Presentation 851、Connector 553、NetworkGuard 19，共 1,496/1,496 通过；package/TRX/acceptance verifier 通过。
- `build/build.ps1 -Configuration Debug -Platform x64`：Rust Host 与 x64 Debug 构建成功，0 警告、0 错误。

完整证据目录：`.local/parity/2026-10-16-1415/`。

## 保留开放项

- 当前只提供一个授权验收账号，因此没有把“第二账号切换”和跨账号敏感缓存审计标为通过。
- 未保存的文件、Notepad 草稿、审批卡和执行中任务在跨账号切换时的组合确认仍需第二账号实机验证。
- 以上限制不影响本轮“真实登出 → 未认证重启 → 同账号重新登录 → 资源恢复 → 再次启动”窄闭环结论。
