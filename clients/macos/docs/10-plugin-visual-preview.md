# Plugin 实时画面与画中画

## 范围

Swift 版只负责 macOS 客户端中的实时画面承载。Computer Use、Browser MCP、Chrome、浏览器 Web 端和 Plugin 内部截图实现均不在重写范围内。

客户端接收统一的 Plugin visual-session：无论帧来自桌面操作还是浏览器操作，都进入同一个 `VisualSessionOverlay`，不为具体 Plugin 写专用页面。画中画的可见性由会话归属决定，不由当前页面类型决定。

## 当前协议事实

现有 Local Connector 通过 `PluginVisualPreview.tsx` 约每 450 ms 读取 `pluginRuntimeVisualSession()`。Core 从 Plugin 私有 `visual-sessions` 目录中选择最新有效 session：

- `status` 必须是 `running`；
- `session_id`、Plugin 与目标应用标签必须通过长度和控制字符校验；
- `captured_at` 超过 15 秒即失效；
- 只接受 `image/jpeg` 或 `image/png`；
- 单帧最大 2 MiB，不读取 symlink；
- 多个 session 同时存在时展示最新活动帧。

当前接口只返回全局最新活动 visual session，字段中没有 conversation、turn 或 task run 标识。这只能作为现状审计，不能直接复制到 Swift 版；否则一个会话的桌面/浏览器画面会错误地出现在另一个会话中。

## 原生状态模型

```swift
struct VisualSession: Sendable, Identifiable, Equatable {
    let id: String
    let adapterSessionID: String
    let pluginID: String
    let componentKey: String
    let title: String
    let targetApplication: String?
    let frameSequence: UInt64
    let capturedAt: Date
    let frame: VisualFrame?
    let owner: VisualSessionOwner
}

struct VisualSessionOwner: Sendable, Equatable {
    let conversationID: String
    let turnID: String?
    let taskRunID: String?
}

enum VisualOverlayMode: Equatable {
    case hidden
    case expanded
    case collapsed
    case waitingForFirstFrame
}
```

`VisualSessionActor` 负责读取、校验、归属映射、去重和过期；`@MainActor VisualSessionModel` 保存当前会话的全部活动 session、当前选中项以及各 adapter 独立的展开状态。同一 session 的新 `frameSequence` 只更新帧，不打断用户的收起选择；新增 session 不抢走用户已经选中的画面。

轮询分成两层：所有活动 session 都读取小体积 metadata，用于数量、归属和切换；只有当前选中 adapter 读取完整帧数据。用户切换画面后，下一次轮询加载对应帧，避免任务数量增长时每 450 ms 重读所有截图。

`adapterSessionID` 必须通过 Task/Plugin runtime 关联到 `conversationID`，并尽可能带上 `turnID` 与 `taskRunID`。若无法确认归属，客户端不得把它当成全局最新画面展示。

## 布局与层级

- Expanded：376×276 pt，主内容区右下角 18 pt 边距。
- Collapsed：168×44 pt，只显示实时点、画中画图标和能力名称。
- 同一 conversation 有多个活动 session 时，浮层显示 `当前位置/总数`、任务标题以及前后切换按钮；收起态保留数量提示。
- 小窗口时按可用内容区缩小，但不覆盖标题栏；无法容纳时退化为 collapsed。
- 画中画在 App Shell 的 z-layer 渲染，以避免被滚动和局部容器裁剪；但它的业务所有者仍是 conversation/task run。
- `selectedConversationID == visualSession.owner.conversationID` 时显示；切换到其他会话后隐藏，后台 Plugin 执行继续。
- 同一会话内切换聊天正文、任务过程、任务详情或其他会话级视图，不应销毁或重建画中画。
- z-order：主工作区 < visual session < 命令/权限审批。

## 并发控制

- Browser MCP 每个 adapter 使用独立进程、浏览器 session、visual-session 目录和 artifact 目录，可以跨任务并行。
- 需要操作独占本机资源的 Plugin MCP 在 Plugin Manifest 中声明 `requiresExclusiveExecution: true`。Task Runner 根据 Plugin Management 解析后的不可变组件快照，在 AI 执行前进入设备级队列；客户端再以 `adapterSessionID` 获取独占租约作为最后一道安全保护。持有者在整个 Plugin runtime session 生命周期内连续操作，其他 adapter 排队，直到持有者取消、结束或调用失败释放控制权。
- 同一 conversation 同时存在 Browser 与 Computer Use 时，两者都保留在画中画集合中，不再用“最近一次工具调用”覆盖另一个来源。

这保证画中画不会改变聊天消息高度、历史 prepend 锚点、Composer 位置计算和自动滚动判定。

## 视觉内容

展开态包含：

1. Plugin 图标、标题、实时状态点和目标应用；
2. 等比 `aspectFit` 的实时帧，空帧时显示等待状态；
3. Plugin 标识和“仅在本机显示”；
4. 收起按钮。

客户端不能在 session 过期后继续显示最后一帧，也不能把预览帧写入聊天历史缓存、诊断日志或云端。

## 验收

- 新 session 自动展开，同 session 帧更新不会反复展开。
- 收起后继续接收帧，点击可恢复。
- 15 秒无新帧后隐藏；恢复时按新 session 处理。
- Computer Use 与 Browser MCP fixture 使用同一 View 和状态机。
- 两个不同会话同时运行 visual session 时，只展示当前选中会话的画面。
- 同一会话两个以上 visual session 时全部保留，可稳定切换，不随工具调用顺序闪烁。
- 多个 Computer Use task run 同时请求真实桌面时按 adapter 串行持有控制权，不交错使用全局 observation、焦点、鼠标或键盘状态。
- 切换会话只改变可见性，不取消 Plugin、清空帧状态或重置聊天历史。
- 画中画出现、收起、结束都不改变当前会话的滚动锚点。
- 审批出现时可完整覆盖画中画并获得键盘焦点。
- 截图快照和日志中不包含真实页面内容、账号、绝对路径或 session 密钥。
