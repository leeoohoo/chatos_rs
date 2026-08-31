# Visual Computer Use MCP

macOS 主实现使用 Swift；独立的 Windows 原生核心位于 [`windows/`](./windows/README.md)，使用 C# / .NET 8 / WPF。两端保持同一套 11 个通用 MCP 工具和“真实截图 → 虚拟定位 → 视觉确认 → 真实输入”的工作流。Windows 目录刻意不包含安装器或第三方平台打包代码，避免与单独的平台适配工作冲突。

## macOS（Swift）

这是一个不读取 DOM、不读取 Accessibility UI tree、不调用目标软件内部 API 的 macOS Computer Use MCP。它把屏幕截图作为 UI 事实来源，并通过 CoreGraphics HID 级事件操作真实鼠标和键盘。

推荐的快速工作流：

1. `observe_screen`：获取真实截图与坐标映射。默认使用 JPEG；首次观察可保留完整屏幕。
2. `move_mouse`：只移动服务端自己渲染的虚拟鼠标，不移动 macOS 真实光标；返回截图确认位置。
3. `click`：确认虚拟鼠标位置正确后，在该全局坐标发送真实 CoreGraphics 点击，并观察结果。
4. 后续优先传 `region`，只观察目标窗口或控件附近，减少编码、传输和视觉推理开销。
5. `type_text` / `key_press`：发送真实键盘事件；确定性的中间组合键可用 `capture_after: false` 省去无价值截图，最终可见状态仍需截图验证。
6. `activate_application`：用公开的 macOS 工作区 API 激活应用，随后仍以真实截图为准。

动作工具只有一套，不再区分“普通版”和“and_observe 版”。虚拟鼠标采用明显区别于 macOS 系统箭头的 AI 环形准星：深色玻璃核心、青紫双轨、四向定位刻度和中心青色精确热点；移动轨迹是短尾、无圆点的蓝紫渐变光带，避免遮挡界面文字。它不仅出现在返回给 AI 的截图里，也通过透明、置顶、鼠标穿透的 macOS Overlay 实时显示在用户桌面上。第一次 `observe_screen` 就会初始化并显示 Overlay，不必等到第一次移动。`move_mouse` 只更新虚拟坐标；`click` 才在中心热点位置发送真实点击。`click` 刻意不接受 `x/y`，避免没有经过位置观察便直接点击。

## 系统要求

- macOS 13+
- Swift 6.1+ / Xcode 16+
- Screen Recording 权限（截图）
- Accessibility 权限（真实鼠标和键盘事件）

## 构建

macOS 推荐构建稳定的 `.app` 权限主体：

```bash
./scripts/build-macos-app.sh
```

产物：

```text
dist/Visual Computer Use.app
dist/Visual Computer Use.app/Contents/MacOS/visual-computer-use-mcp
```

脚本固定使用 ad-hoc 签名，并写入稳定的 designated requirement
`identifier "com.visualcomputeruse.mcp"`。构建流程不会查找或使用 Developer ID、
Apple Development 或其他本机证书。

仅开发调试时也可以直接构建裸二进制：

```bash
swift build -c release
```

可执行文件位于：

```text
.build/release/visual-computer-use-mcp
```

裸二进制适合开发，但 macOS 可能把重新构建的副本识别为新的权限主体。正式接入优先使用 `dist/Visual Computer Use.app` 内的可执行文件，并始终从同一路径运行。

## 权限

MCP 提供两个权限工具：

- `check_permissions`：只检查，不弹窗；返回每项权限的状态、用途、系统设置深链接、准确授权目标以及是否正在使用稳定 `.app` 身份。
- `request_permissions`：显示原生 macOS 权限引导窗口。窗口会解释权限用途、打开准确的系统设置页面、在访达中定位授权目标并持续检查状态。

屏幕录制和辅助功能属于独立运行的 managed `Visual Computer Use.app`，不属于
ChatOS Local Connector 宿主进程。请通过包内 `open-computer-use check-permissions`
或 MCP 的 `check_permissions` 获取真实授权状态；缺少权限时运行 `doctor` 或调用
`request_permissions` 显示原生拖拽引导。授权屏幕录制后仍可能需要重连 MCP。

也可以手动启动权限引导或输出诊断：

```bash
open "dist/Visual Computer Use.app" --args --onboarding
"dist/Visual Computer Use.app/Contents/MacOS/visual-computer-use-mcp" --doctor
```

也可以手动前往：

- System Settings > Privacy & Security > Screen & System Audio Recording
- System Settings > Privacy & Security > Accessibility

在系统设置列表中添加或启用 `check_permissions.authorizationTarget` 返回的目标。屏幕录制授权后，macOS 通常需要重启或重新连接 MCP 才会应用到当前进程。

## 接入 Codex

官方 OpenAI 文档说明，本地 Codex 客户端支持 STDIO MCP，并共享 `config.toml` 配置。构建后运行：

```bash
codex mcp add visual-computer-use -- "/absolute/path/to/dist/Visual Computer Use.app/Contents/MacOS/visual-computer-use-mcp"
```

或在 `~/.codex/config.toml` / 项目级 `.codex/config.toml` 中配置：

```toml
[mcp_servers.visual-computer-use]
command = "/absolute/path/to/dist/Visual Computer Use.app/Contents/MacOS/visual-computer-use-mcp"
default_tools_approval_mode = "writes"
startup_timeout_sec = 15
tool_timeout_sec = 60
```

`writes` 会让客户端对鼠标、键盘等非只读工具请求确认；只读截图与权限检查可直接执行。MCP 配置参考：[OpenAI Model Context Protocol 文档](https://developers.openai.com/codex/mcp/)。

## 接入 ChatOS Plugin Marketplace

这个仓库同时提供 ChatOS schema v3 Plugin 包装，并沿用市场中的
`open-computer-use` Plugin identity，以便从旧实现直接升级到当前视觉实现。

构建标准 npm `.tgz`：

```bash
npm run pack:chatos
```

产物：

```text
dist/chatos-artifacts/open-computer-use-0.8.7.tgz
```

包内 `bin/open-computer-use` 会把 `Visual Computer Use.app` 原子复制到稳定目录：

```text
~/Library/Application Support/Visual Computer Use/runtime/Visual Computer Use.app
```

ChatOS MCP component key 保持为 `computer-use`。Local Connector 调用
`open-computer-use doctor` 时会检查真实 App 权限；缺少权限时自动打开当前
managed App 的原生权限引导窗口。

## 坐标与 Retina

鼠标和截图区域使用的是 macOS 全局“点坐标”，不是图片像素：

- 原点：主显示器左上角。
- `x` 向右增长，`y` 向下增长。
- 左侧或上方的副显示器可能出现负坐标。
- `observe_screen` 同时返回显示器 `frame`、原生像素尺寸和截图实际像素尺寸。
- `captureRegionGlobal` 是当前截图在全局点坐标中的范围；完整显示器截图时它等于 `selectedDisplay.frame`。
- `virtualCursorGlobal` 是虚拟鼠标青色热点的全局坐标，每次截图都会返回。
- `activeApplication` 是截图时刻的前台应用公开进程信息，因此通常不需要再单独调用 `active_application`。
- `cursorScreenshotPixel` 是热点在当前图片中的像素坐标，原点同样位于图片左上角；鼠标在截图区域外时为 `null`。
- `globalDesktopBounds` 是所有显示器合并后的全局桌面范围。
- 从截图像素换算全局坐标：

```text
globalX = captureRegionGlobal.x + imageX * globalPointsPerScreenshotPixelX
globalY = captureRegionGlobal.y + imageY * globalPointsPerScreenshotPixelY
```

截图参数：

- `image_format`：默认 `jpeg`，也可使用无损 `png`。
- `jpeg_quality`：默认 `0.82`。
- `max_image_width`：`observe_screen` 默认 1600，动作工具默认 1400；传 `0` 获取原生宽度。
- `region`：可选 `{x, y, width, height}`，必须完整位于同一块显示器内。
- 每一张截图都强制显示鼠标信息，不提供隐藏开关。鼠标位于截图区域内时绘制非系统箭头形态的 AI 环形准星、青色中心点击热点和短尾光带；位于区域外时在最近边缘绘制屏外方向指示器，并通过 `cursorVisualization` 明确区分。
- 纯 `observe_screen` 局部观察允许鼠标在区域外，不会偷偷改变已经确认的位置。
- 对 `move_mouse`、`click`、`scroll`、`type_text`、`key_press` 或带局部截图的应用激活操作，目标/当前虚拟鼠标必须位于 `region` 内；否则工具会要求 AI 先移动鼠标，确保操作截图一定含有真实热点。

例如，只观察全局坐标 `(300, 120)` 开始的 900 × 700 点区域：

```json
{
  "region": { "x": 300, "y": 120, "width": 900, "height": 700 },
  "image_format": "jpeg",
  "max_image_width": 1000
}
```

动作工具还支持 `settle_ms`，用于控制事件发出后到截图之间的等待时间。默认值按动作设为 60–250 ms；界面已经稳定时可调低，网络页面或动画较慢时可调高。

`key_press` 和 `type_text` 额外支持 `capture_after`，默认仍为 `true`。只有在前台应用与焦点刚刚确认、而且步骤是确定性的中间操作时才应设为 `false`，例如连续执行 `Command+A`、`Command+C`；导航、粘贴、提交、打开新界面和最终写入结果必须保留截图。`move_mouse`、`click`、`scroll` 始终返回截图，不提供跳过开关，因此点击精度与滚动观察不会被快速模式削弱。

`move_mouse` 的可见动画默认持续约 1.2 秒，并使用 60 个轨迹采样点，让用户能够清楚看到虚拟鼠标沿弧线移动。需要更快或更慢时，可通过 `duration` 和 `steps` 调整，而不增加新的工具。

`scroll` 使用像素作为滚动单位，并把总滚动量拆成默认 18 段，在约 0.55 秒内按缓入缓出曲线发送真实 CoreGraphics 滚轮事件。`delta_y` 的正值向上、负值向下，建议每次使用 200–500 像素；水平和垂直滚动量均限制在 -1200…1200 像素。拆分前后的累计滚动量保持一致，可用现有工具的 `duration` 和 `steps` 参数调整节奏。查看聊天历史时，应先把虚拟鼠标放在聊天内容区，再反复执行“中等距离滚动 → 截图 → 阅读”，不要尝试一次滚动到底。

## 输入实现

`type_text` 不读取 DOM、Accessibility UI Tree 或应用内部编辑器，也不修改剪贴板。macOS 通过真实 CoreGraphics Unicode 键盘事件输入文字，换行与 Tab 使用对应的真实按键事件；因此可以在保留剪贴板正文的同时输入标题，不再需要等待剪贴板恢复。

## 快捷键目录

内置目录只含少量通用 macOS、Finder、Safari 和 Chrome 快捷键。它不会读取应用菜单、DOM 或 Accessibility tree。

可通过 JSON 扩展或覆盖目录：

```bash
VISUAL_COMPUTER_USE_SHORTCUTS=/absolute/path/to/shortcuts.json \
  .build/release/visual-computer-use-mcp
```

格式见 [`shortcuts.example.json`](./shortcuts.example.json)。应用配置键使用 bundle identifier；`list_shortcuts` 默认针对当前前台应用。

## MCP 工具

| 工具 | 行为 |
| --- | --- |
| `check_permissions` | 检查 Screen Recording / Accessibility |
| `request_permissions` | 请求系统权限 |
| `observe_screen` | JPEG/PNG 全屏或区域截图 + 坐标映射 + 光标标记 |
| `move_mouse` | 移动自绘虚拟鼠标，不移动真实光标，然后截图 |
| `click` | 在虚拟鼠标热点位置发送真实点击，然后截图 |
| `scroll` | 发送真实滚轮事件，然后截图 |
| `type_text` | 用真实 Unicode 键盘事件输入，不改剪贴板；可选跳过中间截图 |
| `key_press` | 发送真实组合键；可选跳过确定性中间截图 |
| `active_application` | 获取前台应用的公开进程元数据 |
| `activate_application` | 激活或启动应用，然后返回真实截图 |
| `list_shortcuts` | 查询前台应用的已知快捷键 |

## 安全边界

截图可能包含敏感信息；点击、输入、快捷键可能触发发送、删除、购买或权限变更。服务端为工具提供了 MCP annotations，但最终确认策略应由 MCP 客户端执行。动作后自动截图减少的是 MCP 往返次数，不会取消视觉确认：仍应先观察目标、移动并确认位置，再点击并检查结果。
