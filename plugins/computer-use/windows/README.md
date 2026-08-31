# Visual Computer Use MCP for Windows

这是 macOS Swift 版本的独立 Windows 原生后端，使用 C#、.NET 8、WPF 和 Win32。它只把真实屏幕像素作为 UI 事实来源，不读取 DOM、不读取 UI Automation / Accessibility tree，也不调用目标软件内部接口。

当前目录只包含 Windows 核心与 MCP 工具层，不包含安装器、自动更新或第三方平台打包代码，方便与单独进行的平台适配工作合并。

## 实现范围

- GDI `BitBlt` 直接抓取完整显示器或指定的全局像素区域。
- Per-Monitor V2 DPI 和 Windows Virtual Screen 全局坐标；左侧或上方显示器可以使用负坐标。
- WPF 透明、置顶、点击穿透的虚拟鼠标 Overlay；使用明显区别于系统箭头的 AI 环形准星，移动时显示蓝紫到青色的短尾轨迹。
- Overlay 使用 `WDA_EXCLUDEFROMCAPTURE` 请求排除在系统截图之外；每张返回给 AI 的截图会重新合成同一虚拟鼠标、热点和轨迹。
- `move_mouse` 只移动虚拟鼠标。
- 点击和滚动使用真实 Win32 `SendInput`。
- `type_text` 使用 `KEYEVENTF_UNICODE` 真实键盘事件，不使用剪贴板。
- `key_press` / `type_text` 支持 `capture_after: false`，只用于已经确认焦点后的确定性中间步骤；点击、滚动、导航和最终写入仍应保留截图。
- `key_press` 使用真实虚拟键与修饰键事件。
- 应用激活只使用公开进程和前台窗口 API，之后仍以截图为准。
- 手写 newline-delimited JSON-RPC STDIO MCP，工具数量保持 11 个。

## Windows 平台限制

Windows 的 `SendInput` 鼠标点击不能像 macOS `CGEvent` 一样独立携带一个任意屏幕坐标。因此 `click` 和 `scroll` 会：

1. 保存用户真实鼠标位置。
2. 瞬时移动到已经视觉确认的 `virtualCursorGlobal`。
3. 发出真实点击或滚轮事件。
4. 立即恢复原位置。

移动和观察阶段不会移动真实鼠标。UAC 安全桌面、锁屏、断开的远程桌面会话和受保护/DRM 表面不能由该 MCP 控制或可靠截图。控制以管理员身份运行的应用时，MCP 宿主也必须以管理员身份运行，否则会被 Windows UIPI 拦截。

## 开发构建

需要 Windows 10 2004+ 或 Windows 11，以及 .NET 8 SDK：

```powershell
dotnet build .\VisualComputerUse.Windows\VisualComputerUse.Windows.csproj -c Release -r win-x64
```

ARM64 Windows 使用：

```powershell
dotnet build .\VisualComputerUse.Windows\VisualComputerUse.Windows.csproj -c Release -r win-arm64
```

本项目的安装器和面向具体宿主平台的发布方式由外部打包层决定；MCP 入口是生成的 `visual-computer-use-mcp.exe`。

## MCP 工具

工具名与 macOS 版本一致：

- `check_permissions`
- `request_permissions`
- `observe_screen`
- `move_mouse`
- `click`
- `scroll`
- `type_text`
- `key_press`
- `active_application`
- `activate_application`
- `list_shortcuts`

Windows 的 `activate_application` 使用 `application` 参数，可传进程名、可执行文件、Shell 应用名或路径。其余截图、区域、移动、动作后观察参数与 macOS 版本保持同一语义。

## 实机验收顺序

1. 运行 `visual-computer-use-mcp.exe --doctor`，确认处于解锁的交互桌面。
2. 用 MCP `initialize`、`tools/list` 确认只暴露上述 11 个工具。
3. `observe_screen` 检查多显示器范围、负坐标和虚拟鼠标首帧显示。
4. 对右半屏区域截图，确认 `captureRegionGlobal.x` 保留原始全局起点，图片像素可按返回比例映射回全局坐标。
5. `move_mouse` 确认用户可看到完整轨迹且系统真实鼠标不动。
6. `click`、`scroll` 确认真实事件命中虚拟热点，并恢复原物理鼠标位置。
7. 在记事本测试中英文、Emoji、换行输入和常用快捷键。
8. 在 100%、150%、200% 缩放及横向/纵向混合显示器布局下复测截图与 Overlay 对齐。
