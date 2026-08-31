# 聊天历史专项架构

聊天历史是 Swift 重写的 P0 基础设施，不是普通 List 页面。本专项要求在任务卡片、Memory、复杂工具渲染之前完成，并拥有独立的协议、状态机和压力测试。

## 现有实现为什么容易出问题

从当前 React 代码看，问题不是某一个组件写坏了，而是同一份历史被多套机制同时管理：

1. 会话选择时先清空 `messages` 和分页状态，再并发请求会话与最近历史。
2. 初始历史页被固定为 **5 个 turn**，切换会话时又明确“不使用前端消息缓存”。
3. 后台同步会用最近一页**整体替换**当前消息数组；用户已经加载的旧历史可能被丢掉。
4. Realtime 收到持久化消息后先 upsert，又立即触发后台同步；实时事件、服务端落库和 HTTP 读取存在竞态。
5. `loadMore` 只有 cursor，没有 per-session in-flight generation；重复触发或刷新与翻页并发时缺少完整排序规则。
6. 当前页面只保留一个全局 `messages` 数组，切换会话后之前加载的页和滚动位置都不再是可靠状态。
7. 服务端和前端都在把 raw message 重组为 compact turn，并处理 placeholder、callback、final assistant、tool/thinking segment 等多种标记，权威模型不唯一。
8. Realtime 可能原地修改 message metadata，渲染层因此每次 render 都重新解析所有消息。
9. 滚动实现同时维护多个 `requestAnimationFrame`、10 帧 bottom lock、估算行高、窗口扩展、prepend snapshot 和 anchor suspend，状态组合过多。
10. 消息排序主要依赖时间和数组原顺序；缺少稳定的服务端 sequence/revision 时，时间相同、缺失或重放事件容易抖动。

最危险的组合是：用户向上加载过历史 → 实时 callback 到达 → 后台同步最近 5 turn → 整个数组被替换 → 旧历史消失且滚动跳动。

## 目标原则

- 历史事实以稳定的 `ConversationTurn` 为单位，而不是散乱 Message 数组。
- HTTP page、Realtime event、optimistic send 和磁盘缓存只能通过同一个 actor 合并。
- 任何刷新都只能 merge，不能无条件 replace 已加载历史。
- 顺序由服务端稳定字段决定，不用本机时间猜测。
- 滚动位置是业务状态，按 session 保存；数据加载和滚动控制分离。
- UI 可以折叠过程，但数据层不能靠删除/隐藏 message 来制造“精简历史”。

## Canonical Turn 模型

```swift
struct ConversationTurn: Identifiable, Sendable, Codable, Equatable {
    let id: TurnID
    let sessionID: SessionID
    var sequence: Int64
    var revision: Int64
    var userMessage: ChatMessage
    var processEvents: [TurnProcessEvent]
    var finalAssistantMessage: ChatMessage?
    var callbackEvents: [TurnCallbackEvent]
    var status: TurnStatus
    var startedAt: Date
    var completedAt: Date?
}
```

要求：

- `TurnID` 是分页、Realtime、Task、Runtime Context 的共同主键。
- `sequence` 是会话内严格递增顺序，不能只依赖 `created_at`。
- `revision` 用于拒绝旧事件覆盖新状态。
- process、final、callback 是 turn 内的明确字段，不使用散落 metadata 标记互相推断。
- 消息 ID 仍保留，用于编辑、引用和兼容，但列表排序按 turn sequence。

如果服务端暂时不能提供 `sequence/revision/event_id`，Phase 0 必须补协议；客户端不应长期用复杂启发式弥补协议缺口。

## Store 与状态机

每个会话有独立状态：

```swift
struct SessionHistoryState: Sendable {
    var turns: OrderedDictionary<TurnID, ConversationTurn>
    var olderCursor: HistoryCursor?
    var hasOlder: Bool
    var initialLoad: LoadPhase
    var olderLoad: LoadPhase
    var stream: StreamPhase
    var lastAppliedEventSequence: Int64
    var snapshotRevision: Int64
    var viewportAnchor: ViewportAnchor?
    var unreadNewerCount: Int
}
```

唯一写入口是 `ConversationHistoryStore` actor：

```text
cacheLoaded
  -> initialRefreshing
  -> ready
       -> loadingOlder
       -> receivingRealtime
       -> reconnecting
       -> initialRefreshFailed(cache remains visible)
```

禁止 View、Realtime listener 或 API callback 直接修改数组。

## 合并规则

### 初次打开

1. 立即读取本地缓存，恢复最近 turn 与滚动锚点。
2. 请求服务端最新 page。
3. 按 `TurnID + revision` merge。
4. 服务端明确返回 tombstone 时才删除本地项。
5. 缓存存在时刷新失败只显示非阻塞提示，不清空历史。

### 加载更早历史

- 同一 cursor 只允许一个 in-flight request。
- response 带 request generation；过期 response 只能安全 merge，不能改写新 cursor。
- 已存在 Turn 按 revision 更新，新 Turn 按 sequence 插入。
- prepend 后保持“原首个可见 Turn + 像素偏移”不变。

### Realtime

- 每个事件有稳定 `event_id` 和单调 `event_sequence`，重连重放可安全去重。
- optimistic user message 通过 client-generated id 与服务端 ack 对账，不新增第二条。
- final assistant 到达只完成当前 Turn，不触发“用最近一页替换全部历史”。
- Realtime 丢失时按 `after_event_sequence` 补事件；必要时获取增量 snapshot 并 merge。

### 删除与编辑

- 使用服务端 tombstone/revision。
- 本地先显示 optimistic state，失败后恢复并提示。
- 已编辑消息保留 edit version，避免旧 realtime payload 回滚内容。

## 滚动契约

### 首次进入会话

- 无保存锚点：定位最新 Turn。
- 有保存锚点：恢复 TurnID 与相对偏移，然后静默刷新。

### 新内容到达

- 用户距离底部 ≤ 80 pt：保持 bottom-pinned。
- 用户已向上阅读：不跳动，底部显示“3 条新消息”按钮。
- 用户发送消息：可显式回到底部，但不能在每个 token 更新时重复动画。

### 加载更早历史

- 用 SwiftUI ScrollPosition/可见 Turn anchor 保存位置。
- 插入旧数据后恢复同一 Turn 的相对位置。
- 禁止使用固定行高估算消息高度；工具卡片、Markdown 和代码块高度不可预测。

### 会话切换

- 每个 session 保存独立 anchor、是否 pinned、已加载范围和草稿。
- A → B → A 必须回到 A 原位置，且无需重新清空再闪烁加载。

## 缓存

- SQLite 保存最近会话的 Turn、page boundary、snapshot revision 和 scroll anchor。
- 默认保留最近 50 个会话、每会话最近 200 个 Turn；按磁盘预算 LRU 清理。
- 缓存只存可显示业务数据，不存 Access Token、插件 Secret 或未经脱敏的诊断内容。
- schema migration 独立测试，损坏缓存可重建但不能影响云端历史。

## UI 结构

- List item identity 永远是 TurnID，不因 streaming content 变化。
- Turn 内部按需要更新子视图，避免全列表重新解析 Markdown。
- 工具过程默认折叠，但“是否存在、状态、数量、耗时”来自结构化字段。
- 长会话使用 lazy rendering 和分页，不手写基于估算高度的切片窗口。
- 顶部历史加载失败不占据整页，用 inline retry row。
- 底部始终使用正常消息 Composer；历史恢复、缓存命中和滚动锚点不能取代输入区。

### 用户可见状态与内部状态

用户只需要看到能理解或能采取动作的状态：加载更早消息、加载失败与重试、离线/重连、发送失败、任务执行状态，以及离开底部后的新消息数量。

以下内容只属于 Store、日志、测试 fixture 和开发诊断，禁止直接渲染到产品 UI：

- `TurnID`、message ID、cursor、revision、generation、event sequence；
- 缓存 Turn 数、已加载 sequence 范围、snapshot revision；
- anchor Turn、像素偏移、bottom-pinned 判定阈值；
- HTTP、Realtime、SQLite 的 merge 来源、去重结果和连接连续性。

这些内部字段可以支撑诊断导出或开发者日志，但不能以 chip、状态卡片或“会话级恢复状态”出现在普通用户会话中。

## 服务端协议建议

```json
{
  "snapshot_revision": 1842,
  "items": [
    {
      "turn_id": "turn_...",
      "sequence": 318,
      "revision": 7,
      "status": "completed",
      "user_message": {},
      "process_events": [],
      "final_assistant_message": {},
      "callback_events": []
    }
  ],
  "older_cursor": "opaque...",
  "has_older": true
}
```

Realtime envelope：

```json
{
  "event_id": "evt_...",
  "event_sequence": 99128,
  "session_id": "conv_...",
  "turn_id": "turn_...",
  "turn_revision": 8,
  "kind": "assistant.final.persisted",
  "payload": {}
}
```

Cursor 必须 opaque，服务端保证稳定；客户端不能从 message metadata 猜下一页 TurnID。

## 必测场景

### 顺序与去重

- 1000 Turn 中时间戳相同、乱序 HTTP、重复 Realtime、旧 revision 重放。
- optimistic user message 与服务端 ack 使用不同到达顺序。
- callback 先于 final、final 先于 callback、断线后批量补发。

### 分页

- 连续加载 20 页不缺失、不重复。
- `load older` 与 latest refresh 并发。
- cursor response 乱序、超时、重试。
- 加载过 10 页后新消息到达，旧页不能消失。

### 会话切换

- 快速 A/B/C/A 切换，旧 response 不污染当前会话。
- 正在 streaming 时切换，再切回状态连续。
- 每个会话恢复自己的 scroll anchor 和草稿。

### 滚动

- 初次进入定位最新。
- 用户向上阅读时 token、图片、代码高亮完成导致高度变化，位置不跳。
- prepend 旧历史后原可见 Turn 保持在同一位置。
- bottom-pinned 时流式输出平滑跟随；离底后停止跟随。

### 失败恢复

- 首屏请求失败但缓存可用。
- Realtime 断开 10 分钟后补齐。
- app 被杀掉后恢复 streaming/terminal task 的最终持久化结果。
- 缓存损坏、数据库迁移失败、磁盘满。

## 发布门槛

- 10,000 Turn 压力数据下首屏可交互 < 500 ms（有缓存），内存稳定。
- 100 次快速会话切换无串会话、无空白闪烁、无崩溃。
- 20 页历史 + 500 个 realtime 事件随机交错，最终模型与服务端 snapshot 完全一致。
- 所有滚动场景有自动 UI 测试和可重复的 fixture。
- 未达到这些门槛前，聊天历史模块不能标记完成，也不能开始旧客户端切换。
