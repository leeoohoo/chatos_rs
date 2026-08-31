# ChatOS 全局效率工具实施方案

## 1. 目标与范围

在现有 ChatOS macOS 客户端中增加四项进程级全局能力：

1. `Control + A`：区域截屏、窗口截屏和长网页/滚动截屏。
2. `Control + Q`：开始或停止录屏。
3. `Command + E`：打开剪贴板历史，选择一项恢复到系统剪贴板。
4. `Command + Space`：打开类似 Spotlight 的 ChatOS 快速搜索与命令面板。

这些能力必须在 ChatOS 主窗口被隐藏、最小化或关闭后继续可用，只要 ChatOS 进程仍在运行即可。第一期全部在本机完成，不依赖云端接口，也不把剪贴板、截图、录屏或本地文件索引上传到服务端。

本方案的产品定位不是复制四套孤立工具，而是建立一个可以继续扩展的“ChatOS 全局效率入口”：

```text
Global Hotkeys
      │
      ▼
GlobalUtilityCoordinator
  ├── ScreenshotCoordinator
  ├── ScreenRecordingCoordinator
  ├── ClipboardHistoryCoordinator
  └── QuickSearchCoordinator
          ├── ChatOSProvider
          ├── ApplicationProvider
          ├── FileProvider
          └── ActionProvider
```

## 2. 当前项目基础与结论

当前工程已经具备实现这些功能所需的大部分基础：

- 部署目标是 macOS 14，能够使用 `ScreenCaptureKit` 和 `SCScreenshotManager`。
- `PetOverlayWindowController` 已经验证了透明 `NSPanel`、跨 Space、全屏辅助窗口、键盘输入和 App 生命周期级悬浮 UI。
- `NativeSystemPermissions` 已经实现屏幕录制、辅助功能和完全磁盘访问的状态检查与授权引导。
- 打包脚本生成固定 Bundle ID `com.chatos.swift-client`，并维持稳定 designated requirement，适合承载 TCC 权限。
- 项目已有本地项目文件搜索、工作区项目/联系人模型和原生 AppKit/SwiftUI 混合结构。

仍需补齐的基础设施：

- 当前没有全局快捷键注册服务。
- 屏幕权限实现位于 `ChatOSConnector` 内部类型，App 层不能作为通用能力直接复用，需要增加公开的系统能力 facade。
- 当前没有持久化剪贴板数据库。
- 当前项目搜索只覆盖 ChatOS 项目，不是 macOS 全局文件和应用搜索。
- 当前 App 生命周期能力主要直接挂在 `AppModel`，新增四项功能后必须避免继续把所有状态塞进 `AppModel`。

## 3. 必须先接受的快捷键约束

### 3.1 `Control + A` 和 `Control + Q`

这两个组合在 Terminal、Shell、Emacs 风格文本编辑器和部分 IDE 中分别常用于“移动到行首”和其他编辑命令。注册为全局快捷键后，系统会优先交给 ChatOS，其他应用可能收不到原按键。

实施要求：

- 可以把它们作为用户指定的默认值，但首次启用时必须显示冲突提示。
- 设置页必须允许录制和修改快捷键。
- 提供“仅在快捷工具总开关开启时注册”的总开关。
- 注册失败或冲突时显示明确状态，不能静默失效。

### 3.2 `Command + E`

`Command + E` 在很多 macOS 应用中代表“使用所选内容进行查找”。设为全局快捷键会覆盖该行为。处理方式与上面相同：支持用户要求的默认值，同时允许改键和停用。

### 3.3 `Command + Space`

`Command + Space` 默认由 macOS Spotlight 占用。普通应用无法可靠地和系统 Spotlight 同时注册这个快捷键。

正确策略：

1. 优先尝试注册 `Command + Space`。
2. 注册失败时在设置中显示“被系统 Spotlight 占用”。
3. 提供按钮打开“系统设置 > 键盘 > 键盘快捷键 > Spotlight”，指导用户关闭或修改系统 Spotlight 快捷键。
4. 在冲突解除前自动启用一个可用的备用快捷键，建议为 `Option + Space`，并允许用户修改。
5. 不使用 `CGEventTap` 强行拦截系统 Spotlight；这种方案需要更高权限、稳定性差，也会破坏用户对系统快捷键的预期。

## 4. 总体架构

### 4.1 App 生命周期协调器

新增 `GlobalUtilityCoordinator`，由 `AppModel` 持有并通过幂等的 `startGlobalUtilitiesIfNeeded()` 启动，方式与当前宠物悬浮协调器一致。后续若全局能力继续增多，再迁移到 `NSApplicationDelegate`，本期不需要同时重构应用入口。

Coordinator 只负责生命周期和跨模块编排，不直接实现截图编码、SQLite、模糊搜索或窗口布局。

建议成员：

```swift
@MainActor
final class GlobalUtilityCoordinator {
    let preferences: GlobalUtilityPreferencesStore
    let hotKeys: GlobalHotKeyService
    let screenshot: ScreenshotCoordinator
    let recording: ScreenRecordingCoordinator
    let clipboard: ClipboardHistoryCoordinator
    let quickSearch: QuickSearchCoordinator
}
```

启动规则：

- ChatOS 进程启动后即加载偏好和数据库。
- 用户未登录时，截屏、录屏、剪贴板和应用/文件搜索仍可使用。
- ChatOS 项目、联系人、对话和设置搜索在 workspace 可用后动态加入结果。
- 登出只清除云端工作区搜索快照，不停止本机效率工具。
- 系统休眠前安全停止正在写入的录屏；唤醒后恢复剪贴板监控和快捷键状态检查。

### 4.2 全局快捷键服务

使用 Carbon `RegisterEventHotKey` 封装离散全局快捷键。这种方式不需要为了普通快捷键监听额外申请辅助功能权限，也不会像全局 `NSEvent` monitor 一样只能观察而不能可靠消费按键。

新增：

```text
Sources/ChatOSApp/Features/GlobalUtilities/HotKeys/
  GlobalHotKey.swift
  GlobalHotKeyService.swift
  HotKeyRecorderView.swift
```

`GlobalHotKeyService` 负责：

- 注册、注销和重新绑定。
- 把 Carbon key code/modifier 映射为稳定、可持久化的模型。
- 报告 `registered`、`conflict`、`unsupported` 和 `disabled` 状态。
- 对按键自动重复去抖，保证一次按下只触发一次动作。
- 进程退出和偏好关闭时注销所有快捷键。

`Package.swift` 的 `ChatOSApp` target 增加：

- `Carbon`
- `ScreenCaptureKit`
- `AVFoundation`
- `CoreMedia`
- `CoreVideo`
- `UniformTypeIdentifiers`
- `SQLite3`

实际链接项以编译结果为准；系统框架可以按实现文件 import 后再缩减显式 linker settings。

### 4.3 全局面板基类

剪贴板和快速搜索共用一套窗口行为，但不共用业务 ViewModel：

```text
GlobalCommandPanelController
  ├── ClipboardPanelController
  └── QuickSearchPanelController
```

窗口采用可接受键盘输入的无标题 `NSPanel`：

- 屏幕上方约 18% 位置水平居中。
- `level = .floating`。
- `collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient, .ignoresCycle]`。
- `hidesOnDeactivate = true`。
- 打开时记住之前的前台应用。
- `Escape` 关闭，方向键移动选择，`Return` 执行默认动作。
- 再次按同一快捷键关闭面板。
- 多显示器下出现在鼠标所在屏幕，而不是固定主屏。

现有 Pet Panel 的跨 Space 和 SwiftUI hosting 代码可以参考，但不要让效率工具面板继承宠物的拖动、父子窗口或活动状态逻辑。

## 5. `Control + A` 区域截屏

### 5.1 用户流程

1. 用户按下 `Control + A`。
2. 如果没有屏幕录制权限，打开现有授权引导，操作中止。
3. 在所有显示器上创建透明选区 Overlay。
4. 鼠标拖动选择区域；显示尺寸和放大镜辅助。
5. 松开鼠标完成，`Escape` 取消。
6. Overlay 完全隐藏后，通过 `SCScreenshotManager` 获取所选区域。
7. 图片保存到默认目录，并写入系统剪贴板。
8. 屏幕右下角显示短暂结果卡，可执行“打开”“在访达中显示”“复制”“添加到当前对话”。

第一期默认保存目录：

```text
~/Pictures/ChatOS/Screenshots/
```

文件名：

```text
ChatOS Screenshot 2026-08-31 at 14.30.25.png
```

用户可在设置中改为自选目录。当前工程未启用 App Sandbox；如果以后启用 Sandbox，自选目录必须改用 security-scoped bookmark。

### 5.2 技术实现

新增：

```text
Sources/ChatOSApp/Features/GlobalUtilities/Capture/
  ScreenSelectionOverlayController.swift
  ScreenSelectionOverlayView.swift
  ScreenshotCoordinator.swift
  CaptureResultToastController.swift
```

截图底层建议放到 `ChatOSConnector` 的公开本机能力层：

```text
Sources/ChatOSConnector/SystemCapture/
  NativeScreenCaptureService.swift
  NativeScreenCaptureModels.swift
  NativeSystemPermissionService.swift
```

关键实现要求：

- 使用 `SCShareableContent` 获取显示器和窗口。
- 使用 `SCScreenshotManager.captureImage(contentFilter:configuration:)`，不调用 `screencapture` 命令行工具。
- 正确换算 AppKit 左下角坐标、ScreenCaptureKit 显示坐标和 Retina pixel scale。
- 选区跨显示器时第一期拆成各显示器片段再合成，或者限制一次选区只属于一个显示器；推荐第一期限制单显示器并给出明确视觉边界。
- 捕获前隐藏选区窗口、宠物窗口、搜索窗口、剪贴板窗口和结果 Toast，避免把 ChatOS 自身 Overlay 截进去。
- 捕获完成后使用 `NSPasteboard` 同时写入 PNG/TIFF 类型。
- 写文件使用原子替换，失败时仍允许只复制到剪贴板，并展示错误。

### 5.3 “添加到当前对话”

当前聊天已经支持图片附件。结果卡中的“添加到当前对话”应复用现有附件流程，不新建上传协议：

- 有当前 conversation 时，把本地图片转为 composer pending attachment。
- 没有当前 conversation 时禁用该动作并显示原因。
- 未经用户点击不自动上传截图。

### 5.4 长网页与滚动截屏

长网页截屏不能只靠一次 `SCScreenshotManager` 调用完成，因为 ScreenCaptureKit 只能获取当前显示出来的像素。该能力采用两级策略：优先获取浏览器的真实整页内容；无法使用浏览器级能力时，再退化到通用的自动滚动和图像拼接。

```text
长网页截屏
  ├── 浏览器整页捕获（优先）
  │     ├── ChatOS Browser/CDP session
  │     └── 后续可选的 Chrome Extension / Native Messaging
  └── 通用滚动捕获（兜底）
        ├── 选择滚动区域
        ├── 截取当前帧
        ├── 自动滚动
        ├── 重叠区域匹配
        └── 去重拼接
```

#### 5.4.1 用户流程

按下 `Control + A` 后，选区工具栏提供三种模式：

- 区域。
- 窗口。
- 滚动截屏。

滚动截屏流程：

1. 用户点击需要捕获的网页或滚动区域。
2. ChatOS 检测当前目标是否存在可用的浏览器级整页捕获通道。
3. 如果可用，直接获取整页截图，不需要真实滚动页面。
4. 如果不可用，要求用户框选内容区域，并提示“捕获期间请勿操作鼠标和键盘”。
5. ChatOS 逐屏滚动、捕获并拼接，同时显示已捕获长度、缩略预览和停止按钮。
6. 检测到页面底部、连续画面不再变化、用户按 `Escape` 或达到安全上限时结束。
7. 输出单张长 PNG；超过图片安全上限时提供“导出 PDF”或“分段 PNG”。

#### 5.4.2 路径一：浏览器级整页捕获

当网页位于 ChatOS 已控制的 Browser MCP/CDP session 中时，优先通过浏览器协议执行整页捕获，例如 Chromium DevTools Protocol 的页面布局查询和 `Page.captureScreenshot(captureBeyondViewport: true)`。

优点：

- 不需要滚动页面。
- 不受 sticky header、滚动动画、懒加载时序和鼠标干扰影响。
- 能准确覆盖 viewport 之外的页面内容。
- 通常比多帧拼接更快、更清晰。

边界：

- 只能用于 ChatOS 能获得授权控制的浏览器 session。
- 普通用户自己打开、未接入 ChatOS Browser/CDP 的 Chrome、Safari 或 Firefox 页面不能假设存在调试协议。
- 不通过开启任意 Chrome remote debugging 端口来获取用户所有标签页；必须使用已有受控 session，或者后续通过用户明确安装并授权的浏览器扩展接入。

建议新增一个可选协议，不让截图模块依赖具体 Browser Plugin：

```swift
protocol FullPageCaptureProviding: Sendable {
    func canCapture(target: FullPageCaptureTarget) async -> Bool
    func captureFullPage(target: FullPageCaptureTarget) async throws -> FullPageCaptureResult
}
```

`ScreenshotCoordinator` 依次询问已注册 provider；没有 provider 接受目标时自动进入通用滚动捕获。

浏览器级捕获还需要处理：

- 捕获前触发必要的懒加载滚动预热，但不改变最终输出位置。
- 等待网页字体和主要图片加载完成，同时设置最大等待时间。
- 超高页面按浏览器可接受的最大纹理尺寸分片捕获，再在本机合成。
- 默认只捕获网页内容，不包含浏览器地址栏和标签栏；如用户需要浏览器外观，使用窗口或通用滚动模式。
- 页面捕获失败时保留明确错误，并允许一键改用滚动拼接，而不是直接结束操作。

#### 5.4.3 路径二：通用自动滚动与拼接

通用模式面向普通 Chrome、Safari、Firefox、文档阅读器以及其他可滚动应用。

权限要求：

- Screen Recording：获取每一帧画面。
- Accessibility：识别滚动区域、读取可用的滚动值并发送滚动事件。

第一期不申请 Input Monitoring，也不持续监听所有键盘输入；只在捕获会话中使用现有辅助功能能力控制目标滚动区域。

实现步骤：

1. 保存当前前台应用、目标窗口、选区和滚动区域 AX element。
2. 截取第一帧，识别选区内可能固定不动的顶部/底部区域。
3. 以约 70%–85% viewport 高度滚动，保留足够重叠区域用于对齐。
4. 等待滚动稳定和网页重绘，捕获下一帧。
5. 在相邻帧的重叠候选区中做灰度缩小和 normalized cross-correlation/SAD 匹配，计算真实位移。
6. 剔除重复重叠部分，把新内容追加到输出 tile。
7. 连续两次没有有效位移、AX scroll value 已到最大值或相似度表明到达页面末尾时结束。

拼接算法优先使用系统 `Accelerate/vImage/vDSP` 和 Core Image，不为了第一版引入完整 OpenCV 依赖。所有图像匹配在后台 actor/queue 执行，不能阻塞 MainActor。

必须处理的异常：

- sticky/fixed header 和悬浮广告在每一帧重复出现。
- 网页滚动时加载新内容，导致已捕获区域高度变化。
- 无限滚动页面永远没有明确底部。
- 视频、GIF、轮播图和光标造成局部帧变化。
- 页面存在内部嵌套滚动容器，而不是窗口主滚动区域。
- 用户在捕获过程中滚动、缩放、切换标签或改变窗口大小。

处理规则：

- 用户先框选内容区域，尽量排除浏览器工具栏和固定侧栏。
- 对连续多帧位置不变的顶部/底部 strip 标记为固定区域，只在最终图中保留一次。
- 单次匹配低于可信阈值时暂停并提示用户选择“重试本段”“手动对齐”或“结束并保留已捕获内容”。
- 无限滚动默认最多捕获 100 屏或 5 分钟，用户可以提前停止。
- 捕获期间检测目标窗口 frame 和屏幕 scale；发生变化时立即暂停，禁止继续产生错误拼接。

#### 5.4.4 超长图片的内存与输出策略

不能把所有原始帧长期保存在内存中。采用 tile pipeline：

```text
SC screenshot frame
  → crop selected region
  → overlap match
  → discard duplicated rows
  → append encoded/raw tile to temporary workspace
  → release source frame
```

建议限制：

- 默认单张 PNG 最大高度 60,000 px。
- 默认最大总像素 150 megapixels。
- 默认临时磁盘空间上限 2 GB。
- 达到任一限制后停止继续滚动，并允许输出 PDF 或按页分段 PNG。

输出选择：

- 普通长度：单张 PNG。
- 超长网页：多页 PDF，按用户选择的纸张宽度分页，或保持连续长页面 PDF。
- 超过 PNG 安全限制：`Part 001.png`、`Part 002.png` 分段输出，并生成一份 manifest 记录顺序。

临时 tile 位于 `Application Support/ChatOS/CaptureTemp/<session-id>/`。成功、取消或错误后进行清理；App 异常退出后，下次启动清理超过 24 小时的残留会话。

#### 5.4.5 长网页捕获状态机

```text
idle
  → selectingScrollTarget
  → resolvingCaptureStrategy
  → preparingPage
  → capturingFrame
  → scrolling
  → matching
  → capturingFrame ...
  → composing
  → completed | partialResult | cancelled | failed
```

`partialResult` 是正式可交付状态：如果第 18 屏匹配失败，用户仍应能够保存前 17 屏，而不是丢掉整个捕获结果。

#### 5.4.6 长网页首期边界

第一期建议交付：

- ChatOS Browser/CDP session 的真实整页截图。
- 普通浏览器中用户框选单个垂直滚动区域后的自动滚动拼接。
- sticky header 基础去重。
- 到达底部、无变化、用户停止和安全上限四种结束条件。
- PNG、分段 PNG 和 PDF 输出。

第一期暂不承诺：

- 同时跨越横向和纵向滚动的二维大画布。
- 自动处理多个嵌套滚动容器。
- 对所有无限瀑布流生成确定的“完整页面”。
- 在不断变化的视频、游戏或 Canvas 页面上实现像素完美拼接。
- 未经用户安装扩展或授权就直接捕获任意 Safari/Chrome 标签页的 DOM 整页内容。

## 6. `Control + Q` 录屏

### 6.1 用户流程

`Control + Q` 采用 toggle 语义：

- 空闲时按下：进入录制目标选择。
- 正在录制时按下：停止并保存。

首次使用显示轻量选择面板：

- 当前显示器。
- 指定窗口。
- 区域。
- 是否录制系统声音。
- 是否录制麦克风。

后续可以在设置中启用“使用上次目标直接开始”，减少一次交互。录制中显示一个不进入录制画面的悬浮控制条：录制红点、时长、暂停、停止。

### 6.2 macOS 14 实现路线

macOS 14 上使用 `SCStream + AVAssetWriter`，不要依赖较新系统才提供的 `SCRecordingOutput`。

新增：

```text
Sources/ChatOSConnector/SystemCapture/
  NativeScreenRecordingService.swift
  ScreenRecordingWriter.swift

Sources/ChatOSApp/Features/GlobalUtilities/Capture/
  ScreenRecordingCoordinator.swift
  RecordingTargetPickerView.swift
  RecordingControlPanelController.swift
```

职责划分：

- `ScreenRecordingCoordinator`：选择目标、权限、UI 状态和最终结果。
- `NativeScreenRecordingService` actor：管理 `SCStream` 生命周期。
- `ScreenRecordingWriter`：把 `.screen` 和 `.audio` 的 `CMSampleBuffer` 写入 `AVAssetWriter`。

编码建议：

- 容器：`.mov`。
- 视频：H.264，原始帧率上限 60，默认 30 fps。
- 分辨率：按显示器 Retina scale 计算，并限制到编码器稳定范围。
- 系统音频：第一期支持，默认关闭。
- 麦克风：单独通过 `AVCaptureSession` 获取并混流；可放在第二个小阶段完成，不阻塞纯屏幕录制上线。

如启用麦克风，需要在 `Info.plist` 增加 `NSMicrophoneUsageDescription`。屏幕捕获沿用现有 Screen Recording TCC 权限。

### 6.3 状态机

录屏必须由明确状态机驱动：

```text
idle
  → selecting
  → preparing
  → recording
  ↔ paused
  → finishing
  → completed | failed
```

规则：

- `preparing` 和 `finishing` 期间忽略重复快捷键，防止产生两个 writer。
- 系统睡眠、显示器拔出、目标窗口关闭或 stream error 时尝试安全 finish，保留已录制部分。
- App 正常退出且正在录屏时弹出确认；异常退出后依靠临时文件恢复策略保留可修复素材。
- 最终文件完成前先写到 `Application Support/ChatOS/RecordingTemp`，成功 finish 后移动到目标目录。
- 音视频时间戳以第一帧视频的 presentation timestamp 为基准，避免开头黑屏和音画偏移。

默认保存目录：

```text
~/Movies/ChatOS/
```

### 6.4 排除 ChatOS 自身 UI

录制显示器时使用 `SCContentFilter` 排除当前 ChatOS application，确保选区 Overlay、录制控制条、宠物和其他 ChatOS 窗口不进入最终视频。设置中可提供“包含 ChatOS 窗口”高级开关，默认关闭。

## 7. `Command + E` 剪贴板历史

### 7.1 监听与隐私边界

macOS 没有通用剪贴板变化通知。使用低频轮询 `NSPasteboard.general.changeCount`：

- App 活跃时每 300 ms 检查一次。
- App 后台时每 600 ms 检查一次。
- 只在 `changeCount` 变化时读取内容。
- 系统睡眠或功能关闭时停止轮询。

第一期支持：

- 纯文本。
- URL。
- 文件 URL 列表。
- PNG/JPEG/TIFF 图片。
- RTF/HTML 可提取纯文本预览，同时保留原始类型用于恢复。

必须默认跳过：

- 带 `org.nspasteboard.ConcealedType` 的内容。
- 带 `org.nspasteboard.TransientType` 的内容。
- 已知密码管理器的私有 pasteboard 类型。
- 超过配置上限的单项内容。
- 与最近一项内容 hash 相同的重复写入。

界面中明确说明“剪贴板历史仅保存在本机”。提供总开关、立即清空和自动清理策略。

### 7.2 数据模型与存储

新增领域模型：

```text
Sources/ChatOSCore/ClipboardHistoryModels.swift
```

建议字段：

```swift
struct ClipboardHistoryEntry {
    let id: UUID
    let kind: ClipboardContentKind
    let createdAt: Date
    let updatedAt: Date
    let contentHash: String
    let textPreview: String?
    let sourceApplicationBundleID: String?
    let payloadReference: String
    let byteCount: Int64
    let isPinned: Bool
}
```

使用 SQLite 持久化元数据，开启 WAL。数据库位置：

```text
~/Library/Application Support/ChatOS/ClipboardHistory/clipboard.sqlite
```

大图片和复杂 payload 单独存放在同目录的 `Payloads/`，数据库只保存相对路径和摘要，避免数据库快速膨胀。

默认保留策略：

- 最多 500 条。
- 最长 30 天。
- Payload 总量最多 500 MB。
- pinned 项不受条数和时间清理影响，但仍显示总空间占用。

新增：

```text
Sources/ChatOSApp/Features/GlobalUtilities/Clipboard/
  ClipboardHistoryCoordinator.swift
  ClipboardHistoryMonitor.swift
  ClipboardHistoryStore.swift
  ClipboardHistoryViewModel.swift
  ClipboardHistoryView.swift
  ClipboardPanelController.swift
```

Store 和 monitor 使用 actor，UI ViewModel 在 MainActor，避免轮询和图片编码阻塞主线程。

### 7.3 选择行为

默认行为：

1. 用户选中一项并按 `Return`。
2. 恢复该项的原始 pasteboard 类型。
3. 关闭面板。
4. 重新激活打开面板前的应用。

第一期不自动发送 `Command + V`，用户回到原应用后自行粘贴。这种行为不需要额外控制其他应用，也不会误粘贴到错误位置。

高级设置可增加“选择后自动粘贴”：

- 只有辅助功能权限已授权时才允许启用。
- 恢复前台应用并确认激活后，发送一次合成 `Command + V`。
- 默认关闭。

ChatOS 自己恢复历史项时，在 pasteboard 写入自定义标记 `com.chatos.clipboard-restored`；monitor 识别标记或相同 hash 后不再生成重复历史。

### 7.4 面板交互

- 打开后输入即过滤，不需要先点击搜索框。
- 文本按内容搜索；文件按文件名和路径搜索；URL 按 host/title 搜索。
- 左侧显示类型图标或图片缩略图，右侧显示预览、来源应用和时间。
- `Return` 恢复，`Command + Return` 恢复并保持面板打开，`Delete` 删除，`Command + P` pin/unpin。
- 敏感内容一旦因识别失败进入历史，用户可以单项删除或立即清空全部。

## 8. `Command + Space` 快速搜索与命令面板

### 8.1 产品定位

该面板不是只搜索文件，而是统一搜索：

1. ChatOS 项目、联系人、会话和设置页面。
2. macOS 应用。
3. macOS 文件和文件夹。
4. ChatOS 本机动作，例如截屏、开始录屏、打开剪贴板、打开设置。
5. 后续扩展的计算器、网页搜索、AI 提问和插件命令。

第一期结果分组：

```text
建议 / 最近使用
ChatOS
应用程序
文件与文件夹
操作
```

### 8.2 Provider 架构

新增稳定协议：

```swift
protocol QuickSearchProvider: Sendable {
    var id: String { get }
    func search(_ query: QuickSearchQuery) async -> [QuickSearchResult]
}
```

Provider 不能直接返回 SwiftUI View，统一返回 domain result：标题、副标题、图标描述、score、action、稳定 ID 和可选 preview metadata。

新增：

```text
Sources/ChatOSCore/QuickSearchModels.swift

Sources/ChatOSApp/Features/GlobalUtilities/QuickSearch/
  QuickSearchCoordinator.swift
  QuickSearchViewModel.swift
  QuickSearchView.swift
  QuickSearchPanelController.swift
  QuickSearchRanking.swift
  Providers/
    ChatOSSearchProvider.swift
    ApplicationSearchProvider.swift
    MetadataFileSearchProvider.swift
    BuiltInActionSearchProvider.swift
```

### 8.3 各 Provider 实现

#### ChatOSSearchProvider

直接使用 `AppModel` 已加载的：

- projects / workspaceProjects。
- contacts / workspaceContacts。
- 当前可用 conversation。
- 设置页和内建工作区入口。

执行动作必须走 `AppModel` 的统一导航方法，不在 Provider 中直接找 NSWindow 或修改多个 Published 属性。

需要给 `AppModel` 增加稳定入口：

```swift
func openGlobalSearchDestination(_ destination: ChatOSSearchDestination)
```

#### ApplicationSearchProvider

- 启动时扫描 `/Applications`、`/System/Applications` 和 `~/Applications` 的 `.app` bundle metadata。
- 保存 display name、bundle ID、URL 和图标。
- 监听 `NSWorkspace.didLaunchApplicationNotification` 和应用目录变化，更新最近使用和索引。
- 执行时通过 `NSWorkspace.shared.openApplication(at:configuration:)` 启动或激活。

#### MetadataFileSearchProvider

使用 `NSMetadataQuery` 查询 macOS Spotlight 元数据索引，不直接执行 `mdfind` 命令。

- 查询文件名、display name、content type 和可用的文本元数据。
- 对每次输入做 120–180 ms debounce。
- 新 query 取消旧 query，结果按批次增量刷新。
- 默认最多展示 50 条，面板首屏只保留最高分结果。
- 不主动申请完全磁盘访问；无权限目录自然不出现在结果中。
- 如果系统 Spotlight 索引被关闭，界面显示可诊断提示，并继续提供 ChatOS、应用和动作结果。

#### BuiltInActionSearchProvider

第一期至少提供：

- 截屏。
- 开始/停止录屏。
- 打开剪贴板历史。
- 打开 ChatOS 设置。
- 打开 Runtime 与系统权限页面。
- 打开指定项目或联系人。

后续可以增加前缀语法：

- `>`：只搜动作。
- `@`：只搜 ChatOS 项目和联系人。
- `/`：只搜文件。
- `?`：把内容带到当前 ChatOS 对话输入框，不自动发送。

### 8.4 排序

统一 score 由以下部分组成：

```text
finalScore = providerWeight
           + exactMatch
           + prefixMatch
           + wordBoundaryMatch
           + fuzzyMatch
           + recencyBoost
           + frequencyBoost
```

规则：

- 完全匹配和前缀匹配必须高于普通模糊匹配。
- 最近打开过的项目、应用和文件有有限加权，不能永久压过更准确结果。
- 中文使用字符和拼音首字母匹配可以作为第二期；第一期先保证中文子串、英文 token 和大小写无关匹配。
- 排序算法放在纯 Swift 文件中，必须有单元测试，不在 View 中临时排序。

### 8.5 执行动作

- `Return`：执行默认动作。
- `Command + Return`：在 Finder 中显示文件，或在 ChatOS 中打开详情。
- `Option + Return`：显示可用的次级动作菜单。
- `Escape`：关闭并恢复之前的前台应用。

第一期不允许搜索结果直接执行任意 Shell 命令。后续如果加入插件命令或终端动作，必须复用 Native Connector 的审批和权限边界。

## 9. 设置页

在 Settings 的 ChatOS 分组新增“全局工具”页面，使用现有 `SettingsGroupedPage + LocalConnectorCard` 视觉体系，不修改共享卡片的语义容器。

建议分组：

### 全局快捷键

- 总开关。
- 截屏快捷键。
- 录屏快捷键。
- 剪贴板快捷键。
- 快速搜索快捷键。
- 每项注册状态和冲突说明。
- 恢复默认值。

### 截屏与录屏

- 截图保存目录。
- 录屏保存目录。
- 截图后复制到剪贴板。
- 是否排除 ChatOS 窗口。
- 录屏帧率。
- 系统声音。
- 麦克风。
- 使用上次录制目标直接开始。

### 剪贴板

- 启用历史。
- 保留天数和最大条数。
- 最大磁盘空间。
- 自动粘贴高级开关。
- 当前数据库占用。
- 清空历史。

### 快速搜索

- 启用应用搜索。
- 启用文件搜索。
- 启用 ChatOS 搜索。
- 清除最近使用权重。
- Spotlight 快捷键冲突状态和系统设置入口。

偏好放在独立的 `GlobalUtilityPreferencesStore`，沿用 Pet 设置的做法，避免继续扩大 `AppModel` 中的 UserDefaults 属性数量。

## 10. 权限、安全与隐私

| 功能 | 默认所需权限 | 说明 |
| --- | --- | --- |
| 全局快捷键 | 无 | 使用 `RegisterEventHotKey`。 |
| 截屏 | 屏幕与系统音频录制 | 复用现有权限引导。首次授权后可能需要重启 App。 |
| 录屏 | 屏幕与系统音频录制 | 录麦克风时额外需要麦克风权限。 |
| 剪贴板历史 | 无 | 必须明确本机存储、敏感类型排除和清理能力。 |
| 选择后自动粘贴 | 辅助功能 | 默认关闭。只在用户明确开启后发送合成按键。 |
| 应用搜索 | 无 | 读取公开应用 bundle metadata。 |
| 文件搜索 | 取决于文件本身权限 | 使用系统 Spotlight metadata，不以完全磁盘访问作为前置条件。 |

安全规则：

- 截图、录屏和剪贴板内容不得自动上传。
- 添加到对话前必须由用户明确点击。
- 不把剪贴板全文写入日志、analytics 或错误上报。
- 文件搜索结果只显示当前进程能访问的 metadata；执行打开动作仍由系统权限决定。
- 快速搜索动作不得绕过 Native Connector 的命令审批。
- 录屏状态必须始终有明显红点和时长提示，不能静默录制。
- 发布包必须使用稳定签名；TCC 验收必须基于 `.build/ChatOS.app` 或正式签名 App，不能只用 `swift run` 结论代替。

## 11. 文件落点

```text
Sources/ChatOSCore/
  ClipboardHistoryModels.swift
  QuickSearchModels.swift
  GlobalHotKeyModels.swift

Sources/ChatOSConnector/SystemCapture/
  NativeSystemPermissionService.swift
  NativeScreenCaptureModels.swift
  NativeScreenCaptureService.swift
  NativeScreenRecordingService.swift
  ScreenRecordingWriter.swift

Sources/ChatOSApp/Features/GlobalUtilities/
  GlobalUtilityCoordinator.swift
  GlobalUtilityPreferencesStore.swift
  GlobalUtilitiesSettingsView.swift
  GlobalCommandPanelController.swift
  HotKeys/
  Capture/
  Clipboard/
  QuickSearch/

Tests/ChatOSCoreTests/
  ClipboardHistoryModelsTests.swift
  QuickSearchRankingTests.swift
  GlobalHotKeyModelsTests.swift

Tests/ChatOSConnectorTests/
  NativeScreenCaptureCoordinateTests.swift
  ScreenRecordingStateTests.swift
  ScreenRecordingWriterTests.swift

Tests/ChatOSAppTests/（如后续建立 App target 测试）
  ClipboardHistoryStoreTests.swift
  QuickSearchProviderTests.swift
```

现有文件需要调整：

- `Package.swift`：增加系统框架和 SQLite 链接。
- `ChatOSApp.swift`：启动全局工具协调器。
- `AppModel.swift`：持有 coordinator、提供统一搜索导航入口，不承载四个功能的细粒度状态。
- `SettingsView.swift`：增加“全局工具”设置 destination。
- `NativeSystemPermissions.swift`：通过公开 facade 复用权限能力，避免复制判断逻辑。
- `Support/ChatOSSwift-Info.plist`：启用麦克风时增加用途说明。
- `scripts/package-debug-app.sh`：确认新增资源、数据库目录和签名流程不被破坏。

## 12. 分阶段实施

### 阶段 A：全局快捷键与窗口骨架

- [ ] 建立 `GlobalUtilityPreferencesStore`。
- [ ] 实现 Carbon 全局快捷键注册、注销、冲突状态和按键录制控件。
- [ ] 建立通用全局 Panel 行为。
- [ ] 增加“全局工具”设置页。
- [ ] 完成四个快捷键的占位动作和冲突提示。

验收：主窗口关闭后四个快捷键仍能触发对应占位 Panel；`Command + Space` 被 Spotlight 占用时可见冲突状态和备用快捷键。

### 阶段 B：区域、窗口与长网页截屏闭环

- [ ] 公开屏幕权限 facade。
- [ ] 实现多显示器选区 Overlay 和坐标换算。
- [ ] 接入 `SCScreenshotManager`。
- [ ] 保存、复制和结果 Toast。
- [ ] 接入当前对话附件入口。
- [ ] 接入 Browser/CDP `FullPageCaptureProviding` 整页捕获。
- [ ] 实现通用滚动目标识别、自动滚动、重叠匹配和 sticky 区域去重。
- [ ] 实现 tile 临时存储、partial result、分段 PNG 和 PDF 输出。

验收：Retina/非 Retina、多显示器分别截取准确区域；截图中不包含选区 UI；无权限时进入现有授权引导。受控 Browser session 能直接输出整页截图；普通 Chrome/Safari 的稳定长页面可以完成至少 30 屏的自动滚动拼接，固定页头只保留一次；中途失败时可以保存 partial result。

### 阶段 C：基础录屏闭环

- [ ] 实现显示器、窗口和区域目标选择。
- [ ] 实现 `SCStream + AVAssetWriter` 视频写入。
- [ ] 实现系统音频、悬浮控制条和快捷键 toggle。
- [ ] 实现 sleep、显示器变化、窗口关闭和异常 finish。
- [ ] 实现结果 Toast 和输出目录设置。

验收：连续录制 30 分钟无明显音画漂移和内存增长；第二次按 `Control + Q` 可以稳定结束并得到可播放文件。

### 阶段 D：剪贴板历史

- [ ] 实现 changeCount monitor 和敏感类型过滤。
- [ ] 实现 SQLite store、payload 文件和清理策略。
- [ ] 实现剪贴板 Panel、过滤、恢复、删除和 pin。
- [ ] 恢复前台应用。
- [ ] 可选实现辅助功能授权下的自动粘贴。

验收：文本、URL、文件和图片可以记录及恢复；密码管理器 transient/concealed 内容不进入历史；重启 App 后历史仍在。

### 阶段 E：快速搜索一期

- [ ] 实现统一 Provider 和 ranking。
- [ ] 接入 ChatOS 项目、联系人、设置与动作。
- [ ] 接入应用索引和启动。
- [ ] 接入 `NSMetadataQuery` 文件搜索和取消机制。
- [ ] 实现最近使用加权与键盘操作。

验收：输入后 100 ms 内出现本地内存结果；文件 metadata 结果可稍后增量加入；旧 query 不覆盖新 query；结果能正确打开对应目标。

### 阶段 F：麦克风与体验增强

- [ ] 麦克风采集与混流。
- [ ] 截图放大镜、标注和 OCR（可选）。
- [ ] 中文拼音/首字母搜索。
- [ ] 搜索前缀和 AI 输入动作。
- [ ] 剪贴板图片 OCR（可选且默认关闭）。

建议先完成 A–E 再决定 F，避免增强项拖慢四个核心入口交付。

## 13. 测试矩阵

### 单元测试

- 快捷键序列化、modifier 映射、冲突状态和重复事件去抖。
- 截图坐标在主屏、副屏、负坐标屏和不同 scale 下的转换。
- 滚动截图相邻帧位移计算、重叠去重、固定页头识别和低可信匹配中止。
- 超高页面 tile 合成、PNG 分段和 PDF 分页。
- 录屏状态机的合法/非法迁移。
- AVAssetWriter 首帧、音频早于视频、finish 和错误路径。
- 剪贴板 content hash、去重、敏感类型过滤、清理策略和 pin 保护。
- 搜索 exact/prefix/fuzzy/recency 排序。
- Provider 取消后旧结果不得发布。

### 集成测试

- 打包 App 的 Screen Recording TCC 首次授权、拒绝、授权后重启。
- 长网页捕获的 Screen Recording + Accessibility 双权限组合和缺失权限降级。
- Spotlight 占用 `Command + Space` 时的注册失败和备用键。
- 主窗口关闭、Settings 打开、宠物显示和全屏 Space 下的快捷键。
- 剪贴板面板选择后恢复原应用。
- ChatOS 自己写入 pasteboard 后不生成重复历史。
- 文件 Spotlight 索引关闭或暂不可用时，其他 Provider 仍正常工作。

### 人工录屏验收

- Retina 5K/4K、普通分辨率和双显示器。
- 30 秒、30 分钟和 2 小时录制。
- 有/无系统音频，有/无麦克风。
- 睡眠唤醒、锁屏、拔出副屏、关闭目标窗口。
- VLC、QuickTime Player 和 Finder Quick Look 均可播放。
- 控制条和 ChatOS Overlay 默认不出现在视频中。

### 人工长网页验收

- ChatOS Browser/CDP 控制下的静态网页、懒加载网页和超高网页。
- 普通 Chrome、Safari 和 Firefox 中的正文页面。
- 带 sticky header、悬浮侧栏、GIF、视频和延迟加载图片的页面。
- 至少 30 屏高度的页面，检查接缝、重复行、缺失行和固定元素重复。
- 无限滚动页面达到屏数/时间上限后安全停止。
- 第 N 屏故意改变窗口大小，确认捕获暂停且前 N-1 屏可作为 partial result 保存。
- 超过单张 PNG 限制时正确输出 PDF 或有序分段图片。

### 性能门槛

- 空闲剪贴板监控不应造成持续可见 CPU 占用。
- 快速搜索关闭时不保留高频 metadata query。
- 搜索输入期间主线程无明显掉帧，文件查询和图标读取不阻塞 MainActor。
- 录屏内存保持有界，不累计保存未释放的 sample buffer。
- 全局工具与宠物同时运行时不能出现窗口抢焦点循环。

## 14. 完成标准

四项能力只有同时满足以下条件才算可交付：

1. 快捷键在主窗口关闭后仍工作，并能在设置中改键、关闭和看到冲突状态。
2. `Command + Space` 与系统 Spotlight 冲突时不会假装注册成功，也不会使用事件窃取强行覆盖系统。
3. 截图选区坐标准确，结果保存和复制成功，未经点击不会上传；长网页优先使用浏览器整页捕获，通用滚动模式中途失败时仍能保存已完成部分。
4. 录屏有持续可见的状态指示，第二次快捷键可以安全结束并产生可播放文件。
5. 剪贴板历史重启后仍在，支持文本、文件、URL、图片，敏感和 transient 类型默认不记录。
6. 快速搜索至少覆盖 ChatOS、应用、文件和内建动作，旧异步查询不会污染新结果。
7. 所有功能默认保持本机执行和存储，日志不包含剪贴板全文或屏幕内容。
8. `swift build`、`swift test`、本地化审计、打包 App 和上述人工矩阵全部通过。

## 15. 推荐交付顺序

推荐先做：

```text
全局快捷键骨架
  → 区域截屏
  → 剪贴板历史
  → 快速搜索一期
  → 基础录屏
  → 麦克风和增强项
```

原因是截屏可以最快验证快捷键、权限、跨 Space Overlay 和结果交付整条基础链路；剪贴板和搜索可以复用同一 Panel；录屏的编码、时间戳、系统音频和异常恢复风险最高，应该建立在前面基础设施稳定之后。
