# Windows Connector 与代码导航验收

## 构建前提

在 Windows 11、.NET 8 SDK、Visual Studio 2022 Build Tools 和 Windows App SDK 可用的环境执行：

```powershell
dotnet restore ChatOS.Windows.sln
dotnet build ChatOS.Windows.sln -c Debug -p:Platform=x64
dotnet test ChatOS.Windows.sln -c Debug -p:Platform=x64 --no-build
```

macOS 已完成 Core、API、Presentation、Connector 的编译和 211 项自动化测试，并对变更 XAML 做 XML 静态校验；WinUI 的 `XamlCompiler.exe` 只能在 Windows 执行。

## 在线配对

1. 使用有效 ChatOS 账号登录 Windows 客户端。
2. 打开设置中的“本机连接器”，确认初始状态、Gateway 和设备名称正确，且未选择工作区时不能配对。
3. 添加包含空格和中文的本机目录，再添加第二个工作区；重复选择同一路径时不得出现重复项。
4. 点击“配对这台电脑”，确认 ticket 由当前 ChatOS 登录态请求，设备和两个工作区注册成功。
5. 确认配对后主应用仍保持登录，侧栏出现设备名、连接状态和工作区数量。
6. 在服务端确认设备公钥、owner、工作区 fingerprint 和本机状态一致。

## 断开与失败恢复

1. 在设置页点击断开，必须先显示确认对话框。
2. 断开后本机 SQLite 配对状态和 Connector Gateway token 被清除，但 ChatOS API 登录 token 保留。
3. 模拟网关离线后再次断开：界面应明确提示远端未确认，但本机配对仍必须清除并可重新配对。
4. 配对过程中取消、关闭页面或让 ticket 请求失败，界面应恢复可操作状态，不留下半配对数据。

## 睡眠、唤醒和网络重连

1. 已连接时让电脑进入睡眠，状态应变为“系统睡眠，连接已暂停”，旧 WebSocket 不再重连。
2. 唤醒后状态依次进入连接中和已连接，服务端不得出现重复设备或重连风暴。
3. 断开网络，确认进入等待重连；恢复网络后自动回到已连接。
4. 连续睡眠/唤醒两次，并在设置页每两秒状态刷新期间切换页面，确认没有卡死、异常弹窗或后台轮询泄漏。

## 工作区与代码导航

1. 分别打开两个已配对工作区，确认文件、Git、运行和终端请求不会跨越当前 workspace id。
2. 使用中文、空格路径和不同盘符复测文件树与搜索；symlink、junction 和 `..` 不得越过项目根目录。
3. 在 C#、Swift、TypeScript 或 Python 项目中选中文件并输入行列，执行“查定义”和“查引用”。
4. 点击结果后应切换到目标文件和对应行；取消搜索或超过结果上限时 UI 可继续操作。
5. 修改符号后重新搜索，确认短缓存过期后结果更新；无结果时显示明确空状态而不是空白高度变化。

## 通过标准

- 主应用登录凭据与 Connector 凭据互不影响。
- 配对、断开、睡眠、网络恢复和页面切换没有崩溃或卡死。
- 所有远程文件与代码导航操作严格限制在已配对工作区内。
- 侧栏、设置页和服务端显示的设备/连接/工作区状态一致。
