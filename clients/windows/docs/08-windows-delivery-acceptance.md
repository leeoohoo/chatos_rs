# Windows 构建、MSIX 与 CI 验收

本清单必须在 Windows 11 和真实远端仓库执行。macOS 只能验证 XML/YAML 与跨平台测试，不能证明 WinUI XAML Compiler、PRI、MSIX 或安装生命周期可用。

## CI 首次运行

1. 将 Windows 客户端目录提交到远端仓库，触发 `.github/workflows/windows.yml`。
2. `test` job 串行通过 Core 20、API 49、Presentation 52、Connector 242、NetworkGuard 19 项测试，共 382 项。
3. `desktop-build` 的 x64 与 ARM64 matrix 均生成包含 `ChatOS.Desktop.exe` 的未打包产物。
4. `msix-package` 的 x64 与 ARM64 matrix 均生成真实 `.msix`/`.appx`，不能只有空目录或 publish 文件夹。
5. 检查日志中没有 Token、Password、插件 Secret、OAuth token 或模型 API Key。
6. x64 `desktop-build` 默认运行匿名登录页 smoke。需要已认证路径时，在仓库 Secrets 配置 `CHATOS_UI_TEST_USERNAME`、`CHATOS_UI_TEST_PASSWORD`，手动触发并设置 `authenticated_ui_smoke=true`；脚本必须完成登录并验证 Shell → 账号菜单 → 设置页，且日志不得出现凭据。
7. `build/native-acceptance.ps1` 的 9 项指定 WindowsNative 测试必须全部真实出现在 TRX 且通过，NetworkGuard Service TRX 至少包含当前 19 项通过结果；上传 TRX 与 schema v3 `acceptance-report.json`，报告的 `passed` 必须为 `true`。
8. `networkguard-driver` 的 x64 与 ARM64 matrix 均生成 SYS/CAT/INF、Service、`manifest.json` 和 `build-report.json`；报告中的 platform/configuration/WDK version 必须与 matrix 一致。此项只证明 WDK 编译，不替代正式驱动签名和真机加载验收。

## Windows 原生验收证据

```powershell
./build/native-acceptance.ps1 `
  -Configuration Release `
  -Platform x64 `
  -BuildDesktop `
  -RunUiSmoke `
  -PackageMsix
```

1. 原生测试必须验证 Win32 命令输出、超时后的子进程树回收、AppContainer 工作区边界、禁网 capability、ConPTY 和 Credential Manager。
2. `artifacts\windows-native-acceptance\acceptance-report.json` 必须包含 Windows 版本、系统/进程架构、实际执行的检查项、TRX 相对路径/测试名/执行数/hash 和产物 SHA-256；缺少指定测试、零测试或 TRX 计数不一致必须失败。
3. 报告和 TRX 不得记录 Token、Secret、API Key、插件原始路径或供应商响应正文。
4. 已登录的开发机运行 `-RunUiSmoke` 前应使用独立测试 Windows 账号或干净 CI Profile，避免恢复登录态后登录控件不再可见。

## 未签名开发包

```powershell
Set-ExecutionPolicy -Scope Process Bypass
./build/bootstrap.ps1
./build/package.ps1 -Platform x64
```

1. 确认 `src\ChatOS.Desktop\AppPackages` 中存在包。
2. 在启用开发人员模式的测试机使用 `Add-AppxPackage -AllowUnsigned` 安装。
3. 启动后完成登录、Connector 配对、宠物、Visual Session 和 Artifact 冒烟测试。
4. 卸载后确认应用进程、宠物窗口和 Connector 后台均退出。

## 正式签名包

1. 发布证书只从受控证书存储或 CI Secret 注入，不提交 `.pfx` 和密码。仓库 Secrets 使用 `CHATOS_MSIX_PFX_BASE64` 和 `CHATOS_MSIX_PFX_PASSWORD`。
2. 手动触发 `workflow_dispatch` 并设置 `sign_packages=true`，确认临时 PFX 写入 Runner 临时目录、导入 `Cert:\CurrentUser\My`，任务结束时 PFX 和证书均被清理。
3. 证书 Subject 必须与 `Package.appxmanifest` 的 Publisher 完全一致；正式发布前替换开发 Publisher。
4. `build/package.ps1` 必须验证 Authenticode 状态、签名证书 Thumbprint 和可信时间戳，任何一项不符都应使任务失败。
5. 从上一版本升级，确认 SQLite、Credential Manager、插件和宠物状态保持。
6. 降级被阻止；同版本重复安装行为明确；卸载后用户数据保留/清除策略符合发布说明。

## MSIX 安装、升级与卸载自动验收

在一次性 Windows 测试账号执行；脚本检测到相同 Package Identity 已安装时会直接拒绝，避免覆盖开发者的真实客户端。

```powershell
./build/msix-lifecycle.ps1 `
  -PreviousPackagePath C:\acceptance\ChatOS-previous.msix `
  -PackagePath C:\acceptance\ChatOS-current.msix
```

未签名开发包额外传入 `-AllowUnsigned`。脚本必须证明：

1. 旧包和新包具有相同 Name/Publisher，且新版本号更高。
2. 签名包的 Authenticode 状态为 `Valid`；未签名只能通过显式 `-AllowUnsigned` 执行。
3. 安装后的版本与 manifest 一致，并通过 `PackageFamilyName!ApplicationId` 启动，不依赖 exe 绝对路径。
4. 打包应用通过登录页 UI Automation 和 accessible-name smoke。
5. 默认执行卸载，结束后同一 Package Identity 数量为 0；`msix-lifecycle-report.json` 保存包 hash 和结果，不保存本机绝对路径或敏感错误正文。

## AppContainer 与原生进程边界

1. 在“项目只读”下验证读取成功、创建/修改/删除失败；在“仅项目可写”下验证工作区内可写、工作区外不可读写。依次授权 A/B 两个工作区后，A 的进程仍不得访问 B，两个 workspace 必须使用不同 AppContainer SID。
2. “禁止网络”下验证 DNS、IPv4、IPv6 和直接 IP 连接均失败；“允许主机网络”下验证网络行为与当前 Windows 用户一致。
3. FullAccess 必须再次明确确认，并确认它确实绕过 AppContainer；切回受限模式后只影响新启动命令。
4. 一次性命令与 ConPTY 分别验证取消、超时、关闭和应用退出；完整进程树必须由 Kill-on-close Job 回收。
5. 检查子进程环境，只允许白名单变量和沙箱临时目录；不得继承 API Key、插件 Secret 或无关用户环境。
6. “受控域名网络”只在 NetworkGuard readiness 为 Ready 时出现；保存和每次执行都重新检查，失败时拒绝启动，不得通过代理环境变量或静默降级宣称已实现。

## NetworkGuard 真机不可绕过验收

在装有 VS2022 + WDK 的管理员 PowerShell 中执行。必须区分三种签名模式：`unsigned` 仅用于显式开启 Windows test-signing 的隔离机；`local_test` 是本机证书签名，只能用于开发验收；正式发布必须使用 Microsoft Hardware Dev Center 返回的 `microsoft_production` 包，本机 `signtool` 成功不能代替 Microsoft 内核驱动签名。

生产签名准备流程：

```powershell
# 1. 构建待提交包
./build/networkguard.ps1 -Configuration Release -Platform x64

# 2. 使用已关联 Hardware Dev Center 的提交证书生成并签名 CAB
./build/networkguard-submission.ps1 `
  -Platform x64 `
  -CertificateThumbprint '<hardware-submission-certificate-thumbprint>'

# 3. 上传 CAB 至 Microsoft Hardware Dev Center，下载并解压微软签名结果

# 4. 将微软签名结果与原始 Service 合并为生产包
./build/networkguard-import-production.ps1 `
  -Platform x64 `
  -BasePackageDirectory .\artifacts\networkguard\x64 `
  -MicrosoftSignedDriverDirectory C:\acceptance\microsoft-signed-x64
```

正式验收使用导入后的生产包：

```powershell
./build/networkguard-acceptance.ps1 `
  -Configuration Release `
  -Platform x64 `
  -PackageDirectory .\artifacts\networkguard-production\x64 `
  -AllowedUrl https://example.com/ `
  -DeniedUrl https://www.example.com/ `
  -Disruptive `
  -UninstallAfter
```

1. 两个域名必须至少共享一个解析 IP；允许 Host/SNI 的 HTTP/TLS 成功，未允许 SNI 失败。
2. IPv4 literal 必须失败；具有 IPv6 连通性的验收机传入 `-Ipv6Literal` 后也必须失败。
3. UDP 53、QUIC/UDP 443、外部 DoH、无 SNI TLS 和子进程直连必须失败。
4. `-Disruptive` 会分别停止并恢复 Windows Service 与 WFP driver；活动进程必须被 fail closed 回收，恢复后 active lease count 必须为 0。
5. 脚本校验驱动包 SHA-256 manifest、SYS/CAT 签名一致性和服务依赖，要求两个指定 NetworkGuard E2E 测试实际出现在 TRX 且通过，输出 TRX、服务状态及 hash；独立验收私钥在报告生成前删除。
6. `build-report.json` 的 `signing_mode` 必须为 `microsoft_production`，SYS/CAT 必须由 `Microsoft Windows Hardware Compatibility Publisher` 签名并具有可信时间戳。`local_test` 结果不能作为正式发布证据。

## 一键最终验收与证据防伪

在一次性管理员 Windows 测试机运行。x64 验收必须在 x64 Windows，ARM64 验收必须在原生 ARM64 Windows；脚本会安装并卸载 MSIX、NetworkGuard Service 和驱动，不得在开发者日常使用的机器执行。

```powershell
$env:CHATOS_UI_TEST_USERNAME = '<acceptance-user>'
$env:CHATOS_UI_TEST_PASSWORD = '<acceptance-password>'

./build/windows-final-acceptance.ps1 `
  -RuntimePlatform x64 `
  -BuildCurrentPackage `
  -MsixCertificateThumbprint '<msix-signing-thumbprint>' `
  -PreviousPackagePath C:\acceptance\ChatOS-previous.msix `
  -NetworkGuardX64PackageDirectory C:\acceptance\networkguard-production-x64 `
  -NetworkGuardArm64PackageDirectory C:\acceptance\networkguard-production-arm64 `
  -RequireAuthenticatedUi `
  -RequireUpgrade `
  -Ipv6Literal '2606:4700:4700::1111' `
  -ConfirmDisposableMachine
```

执行顺序和门禁：

1. 输入从 Microsoft Hardware Dev Center 返回并经 `networkguard-import-production.ps1` 固化的 x64/ARM64 生产包，并构建当前机器架构的签名 MSIX；实际当前 MSIX、用于升级的旧版 MSIX 与两套完整 WDK 包会复制到本次验收目录的 `deliverables`，避免随后普通构建覆盖证据。已有正式 MSIX 也可用 `-PackagePath` 代替 `-BuildCurrentPackage`。
2. 当前机器架构执行 WindowsNative、NetworkGuard Service 单测、Desktop Release 和真实登录后的 Shell → 设置 UI Automation。
3. 安装旧 MSIX、升级当前 MSIX、按 Package Identity 启动并执行认证 UI smoke，随后卸载且要求残留数量为 0。
4. 安装当前架构 NetworkGuard，执行真实签名策略、HTTP/TLS allow、同 IP denied SNI、DNS/DoH/QUIC/UDP/no-SNI/子进程拒绝、Service/driver 重启和 lease residue=0，随后卸载。
5. `verify-windows-acceptance.ps1` 校验五份报告的 schema、平台、逐阶段 requested/passed、签名状态、Microsoft 生产签名主体、disruptive 和卸载；重新解析 native/NetworkGuard TRX 并核对目标测试名、执行数、结果和 hash；同时从 `deliverables` 重新读取实际当前/旧版 MSIX、SYS、CAT、manifest 和双架构 WDK artifacts，校验安全相对路径、文件长度与 SHA-256。任何缺项、零测试、`local_test` 冒充生产或报告/文件不一致都会生成失败 summary 并返回非零。
6. 最终证据位于 `artifacts\windows-final-acceptance\<platform>-<timestamp>-<run-id>`，随机 run ID 防止同秒并发覆盖；其中 `deliverables` 是自包含交付物副本，`orchestration-report.json` 表示执行阶段，`verification-report.json` 表示实际文件和报告证据是否足以支持结论。

只有隔离开发验收才能显式传入 `-AllowUnsignedDriver`、`-AllowTestSignedDriver` 或 `-AllowUnsignedMsix`；带这些开关产生的报告不能作为正式发布证据。使用本机证书构建驱动时必须同时传入 `-NetworkGuardCertificateThumbprint` 与 `-AllowTestSignedDriver`。

## 架构与系统

1. x64 包在 Intel/AMD Windows 11 真机运行。
2. ARM64 包在原生 ARM64 Windows 11 真机运行，不依赖 x64 仿真进程。
3. Windows 10 19041 最低版本至少完成安装与启动检查；主要功能以 Windows 11 为验收基线。
4. 离线启动时显示可操作的服务端错误，不因健康检查或 Realtime 失败退出应用。

未完成上述步骤前，能力矩阵保持“实现中”或“Windows 验收”，不得标记为最终发布完成。
