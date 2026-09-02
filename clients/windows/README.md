# ChatOS Windows

ChatOS 的 Windows 原生客户端，使用 C#、WinUI 3 和 Windows App SDK 实现。

本项目与 macOS Swift 客户端保持产品能力和视觉语言一致，但不共享平台源码。两端共用 ChatOS 后端 HTTP、Realtime、Connector 和插件协议。

## 仓库结构

```text
src/ChatOS.Core       平台无关领域模型和状态机
src/ChatOS.Api        HTTP、Realtime、DTO 和服务
src/ChatOS.Connector  Windows 本机能力
src/ChatOS.Desktop    WinUI 3 桌面应用
tests                 Core/API 自动化测试
docs                  实施方案与能力矩阵
```

## Windows 开发环境

需要：

- Windows 11
- Visual Studio 2022 17.10+
- .NET 8 SDK
- Windows App SDK / WinUI 3 工作负载
- Windows 10 SDK 10.0.19041+

命令行构建：

```powershell
./build/bootstrap.ps1
./build/test.ps1
./build/build.ps1
```

生成未签名 MSIX（需要 Windows）：

```powershell
./build/package.ps1 -Platform x64
./build/package.ps1 -Platform ARM64
```

打包脚本会生成所需的 Windows 图标资源，并在没有真实 MSIX/AppX 输出时直接失败。发布证书不会写入仓库；正式发布时由受控 CI 或本机证书存储完成签名。

默认 API 地址是 `http://127.0.0.1:9080/api/chatos`，可通过环境变量覆盖：

```powershell
$env:CHATOS_API_BASE_URL = "https://example.com/api/chatos"
```

## 一键安装到 Windows 本机

将整个目录复制或拉取到 Windows 11 电脑后，直接双击：

```text
scripts\install-client.cmd
```

它会自动识别 x64/ARM64、构建 Release 客户端、安装到 `%LOCALAPPDATA%\Programs\ChatOS`，创建开始菜单和桌面快捷方式，然后启动客户端。默认连接当前线上 ChatOS 和 Local Connector 服务。

也可以在 PowerShell 中运行：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
./scripts/install-client.ps1
```

卸载应用但保留本地设置：

```powershell
./scripts/uninstall-client.ps1
```

只有明确希望同时清除 SQLite 设置、缓存和登录状态时才使用 `-RemoveUserData`。

## 一键生成 EXE 安装包

如果只复制或压缩了 `clients/windows` 目录，可以直接双击：

```text
.\scripts\package-client.cmd
```

也可以在完整仓库根目录运行：

```text
.\scripts\package-windows-client.cmd
```

脚本默认根据当前 Windows 电脑自动选择 x64 或 ARM64，执行 Release 测试、自包含发布，并在缺少时通过 `winget` 安装 Inno Setup 6，最终生成：

```text
clients\windows\BundleArtifacts\installer-x64\ChatOS-Setup-x64.exe
```

直接双击这个 EXE 即可安装。安装程序支持覆盖升级、开始菜单、桌面快捷方式、卸载入口和安装完成后启动。客户端默认连接线上 ChatOS 网关和 Local Connector，不要求使用者另外安装 .NET Runtime。

常用参数：

```powershell
# Windows on ARM
.\scripts\package-client.cmd -Platform ARM64

# 快速重新打包，跳过自动化测试
.\scripts\package-client.cmd -SkipTests

# 连接指定测试环境
.\scripts\package-client.cmd `
  -ApiBaseUrl "http://127.0.0.1:9080/api/chatos" `
  -LocalConnectorCloudBaseUrl "http://127.0.0.1:39230"
```

也可以直接执行 PowerShell 脚本：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
./clients/windows/scripts/package-client.ps1
```

## 从源码直接启动

将项目复制或拉取到 Windows 11 电脑后，在 PowerShell 中运行：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
./scripts/start-client.ps1
```

脚本会检查 .NET 8 SDK 和线上服务健康状态，设置本次进程使用的服务端地址，恢复依赖、构建 x64 Debug 客户端，然后启动 `ChatOS.Desktop`。服务端地址只通过环境变量传入，不会写死到客户端代码。

可选参数：

```powershell
# Release x64
./scripts/start-client.ps1 -Configuration Release

# Windows on ARM
./scripts/start-client.ps1 -Configuration Release -Platform ARM64

# 已构建时跳过恢复和构建
./scripts/start-client.ps1 -NoBuild
```

默认连接：

- API：`https://gateway.jgoool.com/api/chatos`
- Local Connector 云端入口：`https://local-connector.jgoool.com`

如需连接其他环境，可使用 `-ApiBaseUrl` 和 `-LocalConnectorCloudBaseUrl` 覆盖。

详细方案见 [docs/01-windows-client-implementation-plan.md](docs/01-windows-client-implementation-plan.md)。macOS 端后续修复和更新的 Windows 同步队列见 [docs/10-macos-change-sync-register.md](docs/10-macos-change-sync-register.md)。
