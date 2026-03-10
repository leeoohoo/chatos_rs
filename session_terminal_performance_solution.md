# 会话切换与终端历史卡顿的彻底优化方案

## 1. 问题定义与目标

### 1.1 线上问题
- 会话消息历史很长时，切换会话明显卡顿（UI 阻塞、加载慢）。
- 本地终端历史很多时，打开终端/加载更多历史非常慢。
- 远端终端在快照较大时，首次进入也可能有明显等待。

### 1.2 目标（验收指标）
- 会话切换（已有大历史）：
  - `GET /sessions/:id/messages?compact=true&limit=50&offset=0` 后端 P95 < 200ms（SQLite），P95 < 300ms（Mongo）。
  - 前端切换到“可交互” P95 < 350ms。
  - 主线程单次阻塞不超过 50ms（Long Task 计数显著下降）。
- 终端打开：
  - 首屏可见输出（TTFP）P95 < 300ms。
  - “加载更多历史”不触发 `term.reset()` 全量重放，单次操作 P95 < 250ms。
- 稳定性：
  - 历史越大，延迟增长近似 O(page_size)，而不是 O(total_history)。

## 2. 当前代码级根因（已定位）

### 2.1 会话切换后端是 O(总历史)
- 文件：`chat_app_server_rs/src/api/sessions/history.rs`
- 当前 `fetch_session_messages_for_display(..., compact=true)` 逻辑：
  1. `MessageService::get_by_session(session_id, None, 0)` 取全会话。
  2. `build_compact_history_messages(messages)` 全量构建。
  3. `apply_recent_offset_limit` 再截取最近页。
- 结果：请求 `limit=50` 也会全量扫描，历史越大越慢。

### 2.2 前端渲染链路仍有大量全量遍历
- `chat_app/src/components/MessageList.tsx`
  - 多个 `useMemo` 对 `messages` 做全量遍历（tool map、process stats、lookup key）。
- `chat_app/src/components/MessageItem.tsx`
  - 仍存在对 `allMessages` 的 `find/filter` 回退路径。
- `chat_app/src/components/ChatInterface.tsx`
  - 多处与任务面板联动逻辑对整段消息循环。

### 2.3 store 订阅粒度过粗，放大重渲染
- `chat_app/src/lib/store/ChatStoreContext.tsx`
  - `useChatStoreFromContext()` 直接 `store()` 返回整个 state+actions。
- 结果：任意状态更新都可能触发大量组件重渲染（含会话列表、终端、面板）。

### 2.4 本地终端历史加载是“全量拼接+全量回放”
- 文件：`chat_app/src/components/TerminalView.tsx`
- 当前逻辑：
  1. 初始拉取 `limit=800`（可继续加到 5000）。
  2. 将 output logs 全量 `join('')`。
  3. `term.reset()` 后 `writeToTerminalInChunks` 回放整段文本。
  4. 命令历史解析也在主线程做全量处理。
- 结果：日志越多，打开越慢，且每次“更多历史”反复重算/重放。

### 2.5 数据库索引不足以支撑高效排序分页
- SQLite：已有 `messages(session_id)`、`messages(created_at)`，缺少高频组合索引 `(session_id, created_at)`。
- Mongo：messages 仅见 `session_id` 索引，缺少 `(session_id, created_at)` 复合索引。
- terminal_logs 也建议补组合索引 `(terminal_id, created_at)`（SQLite/Mongo 对齐）。

## 3. 彻底方案总览（前后端一体）

采用“四层改造 + 灰度开关”策略：

1. **后端数据访问改造（最高收益）**
- 新增 compact v2 查询路径：按最近窗口构建，而不是全量构建后截断。
- 为会话消息与终端日志增加组合索引。
- turn-process 查询从“全量扫描”向“按 turn 定位”演进。

2. **前端状态与渲染改造**
- 把 store 使用方式改成 selector 订阅，避免全局重渲染。
- 消息列表改为真正虚拟列表（双向窗口），减少 DOM/计算规模。
- 将重计算（process/tool map）下沉为“按 messageId 增量维护”。

3. **终端协议与加载模型改造**
- 本地终端改为“先 tail 快速可见 + 历史按游标增量加载”，不再每次 reset 全量回放。
- 命令历史解析从“全量重扫”改为“增量 + 空闲调度/Worker”。
- 远端终端快照增加裁剪与可选差量策略。

4. **可观测性、压测与灰度发布**
- 加后端接口/SQL耗时埋点、前端切换/终端性能指标。
- 先开 feature flag，分阶段灰度，指标不达标自动回滚到旧路径。

## 4. 详细实施方案

## 4.1 后端：会话消息 compact v2

### 4.1.1 API 设计
保持现有接口不变，新增参数：
- `GET /api/sessions/:id/messages?compact=true&strategy=v2&limit=50&offset=0`

默认策略：
- 初期 `strategy=v1`（旧逻辑）
- 灰度后切 `v2`

### 4.1.2 v2 核心算法
目标：只处理“最近窗口所需”消息，不全量扫描。

建议实现（SQLite/Mongo 均可落地）：
1. 先取最近 `N` 条原始消息（例如 `N = max(limit*8, 400)`），按时间倒序取，再转正序。
2. 在该窗口内构建 compact turn；若用户消息数量不足以覆盖 `limit+offset`，指数扩容 N（上限例如 5000）。
3. 只对最终需要返回的范围做 `build_compact_history_messages`。
4. 命中极端会话再降级到 v1，并打告警日志（便于后续优化）。

这样复杂度接近 O(window_size)，而不是 O(total_history)。

### 4.1.3 数据层增强
- SQLite 新增索引：
  - `CREATE INDEX IF NOT EXISTS idx_messages_session_created_at ON messages(session_id, created_at);`
  - `CREATE INDEX IF NOT EXISTS idx_terminal_logs_terminal_created_at ON terminal_logs(terminal_id, created_at);`
- Mongo 新增索引：
  - `messages: {session_id: 1, created_at: 1}`
  - `terminal_logs: {terminal_id: 1, created_at: 1}`

### 4.1.4 turn-process 接口优化（第二阶段）
当前 `get_session_turn_process_messages` 仍全量拉取会话后定位 turn。
建议新增“turn 索引表/字段”以便直接查：
- 方案 A：messages 表新增 `conversation_turn_id` 列并回填。
- 方案 B：新增 `session_turn_index(session_id, turn_id, user_message_id, final_assistant_id, created_at)`。

优先 A（实现更直接，查询链路短）。

## 4.2 前端：状态层与渲染层

### 4.2.1 Store 订阅改造（必须做）
- 新增 `useChatStoreSelector(selector, equalityFn)`。
- 组件改为按需订阅，不再 `useChatStoreFromContext()` 一次拿全量 state。

优先改造组件：
- `ChatInterface.tsx`
- `SessionList.tsx`
- `MessageList.tsx`
- `TerminalView.tsx`
- `RemoteTerminalView.tsx`

预期收益：减少跨面板无关重渲染，降低会话切换时 UI 抖动。

### 4.2.2 会话切换流程优化
当前 `selectSession` 串行：先 `fetchSession` 再 `fetchSessionMessages`。
改为并行：
- `Promise.all([fetchSession, fetchSessionMessages])`

同时引入 session message cache（LRU，按会话存最近 compact 页）：
- 切换命中缓存：先秒开渲染，再后台刷新。
- 切换未命中：展示 skeleton + 最小首屏页。

### 4.2.3 消息列表虚拟化重构
将当前“截断 slice”升级为真正虚拟化（推荐 `@tanstack/react-virtual`）：
- 仅渲染可视区 + overscan。
- 双向虚拟（上滑加载历史时不瞬间扩到全量 DOM）。
- 行高动态测量缓存，避免滚动抖动。

### 4.2.4 MessageItem 去全量依赖
- 去掉 `allMessages` 作为每行 props。
- 所有 tool/process 关联关系在上层按 messageId 预计算并缓存（Map）。
- `MessageItem` 只吃当前消息 + 常量引用，避免每行 `find/filter`。

### 4.2.5 Markdown 与重计算降载
- Markdown 渲染按可见区懒加载。
- 非可见消息只渲染轻量骨架。
- 重计算（如 processSignal）按“增量更新”而不是每次全量重算。

## 4.3 终端链路彻底改造

## 4.3.1 本地终端历史接口改造
现状是 limit 扩大 + 每次全量重放。改为游标分页：
- `GET /api/terminals/:id/history?before=<cursor>&limit=200`
- 返回：
  - `items`
  - `next_before`
  - `has_more`

初次进入：
- 只拉最近 150-200 条日志，保证秒开。

### 4.3.2 前端终端重放策略改造
- 禁止“加载更多”触发 `term.reset()`。
- 终端区只保证“实时 tail 体验”；历史查看用独立历史面板/抽屉（或顶部懒加载区域），不破坏当前 xterm buffer。
- 命令历史解析改增量：
  - 新日志到达时增量解析。
  - 历史补载时用 `requestIdleCallback` 或 Web Worker 分批解析。

### 4.3.3 远端终端优化
- 保持 snapshot + stream 模型，但：
  - 首次 snapshot 上限可从 512KB 下调到 256KB（可配置）。
  - 仅连接切换时发送 snapshot；重连可支持 `since_seq` 差量补发（可选二期）。

## 4.4 可观测性与告警

### 4.4.1 后端埋点
新增结构化耗时日志：
- sessions messages: `session_id, compact, strategy, limit, offset, scan_count, build_count, elapsed_ms`
- terminal history: `terminal_id, limit, cursor, rows, elapsed_ms`

### 4.4.2 前端埋点
- `perf_mark_select_session_start/end`
- `perf_mark_terminal_open_start/first_output`
- Long Task 统计（>50ms）

### 4.4.3 告警阈值
- 会话切换 P95 > 500ms 持续 5 分钟报警。
- 终端首屏 > 700ms 持续 5 分钟报警。

## 5. 分阶段执行计划（可直接排期）

## 阶段 A（1-2 天）：高收益快改
- 后端：实现 compact v2（窗口构建 + 回退机制），加必要索引。
- 前端：`selectSession` 并行请求；加基础性能埋点。
- 验收：大历史会话切换耗时至少下降 40%。

## 阶段 B（2-3 天）：前端重渲染治理
- 引入 selector 订阅，优先改 ChatInterface/MessageList/TerminalView。
- MessageList 的 Map 计算改增量缓存。
- 验收：切会话时 Long Task 次数下降 50% 以上。

## 阶段 C（3-5 天）：终端体验重构
- 新增终端 history cursor API。
- TerminalView 改为 tail-first + 历史独立加载，不再 reset 全回放。
- 命令解析改增量/空闲调度。
- 验收：终端打开 P95 < 300ms。

## 阶段 D（2 天）：灰度与回归
- 加 feature flag：
  - `perf.compact_messages_v2`
  - `perf.store_selector_mode`
  - `perf.terminal_cursor_history`
- 10% -> 30% -> 100% 灰度，观察性能与错误率。

## 6. 回归测试与压测方案

### 6.1 后端
- 构造 1k/5k/20k 消息会话，测 `compact v1/v2` 对比。
- 构造 10w terminal_logs，测 cursor 分页 API。
- 增加单测覆盖：
  - compact 窗口不足时扩容逻辑。
  - offset/limit 边界。
  - v2 回退 v1 的一致性校验。

### 6.2 前端
- E2E：
  - 2000 条消息会话切换 20 次，统计 P95。
  - 终端日志高负载下首次打开与加载更多。
- 增加渲染计数监控（React Profiler 或自定义 dev 标记）。

## 7. 风险与回滚

### 7.1 风险
- compact v2 窗口算法边界错误导致消息缺失。
- selector 改造期间可能出现状态读取遗漏。
- 终端历史 UX 从“全量重放”变更为“tail-first”需要用户适应。

### 7.2 回滚
- 通过 feature flag 一键回到：
  - compact v1
  - 旧终端 history 模式
  - 旧渲染链路
- 所有新接口保持向后兼容，不删除旧参数。

## 8. 建议的落地顺序（我建议按这个顺序直接开工）

1. 先做后端 compact v2 + 索引（最快见效，直接降低切会话耗时）。
2. 同时改 `selectSession` 并行请求 + 埋点，立刻验证收益。
3. 再做 store selector 改造，解决“全局重渲染”放大问题。
4. 最后做终端 cursor 模式重构，彻底解决终端历史卡顿。

## 9. 阶段执行记录

### 2026-03-10（已完成）
- 后端 `compact` 查询路径升级为最近窗口增量构建（默认 `strategy=v2`，保留 `strategy=v1` 回退开关）。
- 前端 `fetchSessionMessages` 已透传 `strategy=v2`。
- 会话切换 `selectSession` 改为并行获取 session 与消息。
- SQLite 与 MongoDB 均补充了 `messages(session_id, created_at)` 与 `terminal_logs(terminal_id, created_at)` 组合索引。
- 本地终端初始历史加载量从 800 下调到 240，首屏更快；“加载更多”改为 offset 分页增量拉取，避免重复下载历史。
- 终端重放过程改为按日志块写入，避免构造超大字符串拼接。
- 新增 `useChatStoreSelector`，并将 `TerminalView` / `RemoteTerminalView` 改为精准订阅，减少无关重渲染。
- `ChatInterface` 已切换为 selector + shallow 订阅，避免“订阅整个 store”导致的全量重渲染放大。
- `MessageList` 新增 assistant->user 展开状态预计算映射，`MessageItem` 改为直接消费预计算结果，减少每条 assistant 的 `allMessages.find(...)` 扫描。
- `MessageList` 主列表渲染不再给每条消息透传整份 `allMessages`，降低大数组引用传递与回退扫描成本。
- Mongo 消息查询链路优化：`get_messages_by_session` / `get_recent_messages_by_session` 从“全量拉取+内存排序/截断”改为数据库 `sort/skip/limit`，复杂度由 O(total) 降到 O(page)。
- Mongo 终端 recent 日志查询去掉二次内存排序，改为直接消费数据库排序结果。
- 终端历史 API 新增 `before` 游标参数，前端 `TerminalView` “加载更多”已切换为游标分页，避免 `offset` 递增带来的查询退化。
- `SessionList` 已切换为 selector + shallow 订阅，降低消息流更新对左侧列表的无关重渲染影响。
- `selectSession` 增加耗时日志（前端），便于线上观测切会话 P95 改善情况。
- `TerminalView` 的“加载更多历史”改为增量预热，不再执行 `term.reset() + 全量重放`；仅首屏执行历史回放，后续只增量更新命令侧栏与分页游标，避免历史越多越卡。
- 终端面板新增 tail-first 提示文案，明确“已预载更早历史但终端窗口保持实时尾部输出”。
- 后端 `/api/terminals/:id/history` 新增 `target=perf` 结构化耗时日志（含 `limit/offset/before/rows/elapsed_ms` 与错误分支），便于观测终端分页链路性能与异常。
- `selectSession` 新增短 TTL（45s）+ LRU（16 项）会话消息页缓存，频繁切换同一批会话时优先命中本地缓存并减少重复拉取/解析。
- `MessageItem` / `ToolCallRenderer` 去掉 `allMessages.find/filter` 回退路径，工具结果仅走预构建 `toolResultById` 索引，避免每行消息渲染时重复线性扫描。
- `MessageItem` 的 memo 比较器改为“message 引用 + 关键外部键（tool/process/link）”比较，移除高成本字符串摘要比较，降低流式与滚动过程中的比较开销。
- `TurnProcessDrawer` 对齐 `assistantToolCallsById` 预索引透传，避免过程面板里的工具调用回退为 `unknown_tool` 并减少重复查找。
- 前端新增会话切换与终端打开性能埋点：
  - `selectSession` 追加 `performance.measure`（日志字段 `perfMs`）
  - `TerminalView` 追加 `history ready` 与 `first realtime output` 两个耗时日志点

---

如果按这个方案执行，预计第一周可把“切会话卡顿”问题明显压下去，第二周完成终端历史体验的结构性修复。
