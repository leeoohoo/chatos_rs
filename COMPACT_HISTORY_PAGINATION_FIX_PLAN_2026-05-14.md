# 历史消息长期优化方案：Turn Projection + 按需 `showprocess`

## 目标

这份方案只讨论长期优化，不讨论临时兜底或最小修复。

目标非常明确：

- 主聊天列表加载时，只查询“用户消息 + 该轮最终 assistant 总结/最终答复”
- 工具调用记录、tool result、thinking、中间 assistant 片段，不进入主列表查询结果
- 这些过程数据只在点击 `showprocess` 时，按 turn 单独查询
- `加载更多` 必须基于 turn/compact history 自身分页，不能再被 raw message 数量污染

## 现状问题

当前问题不是 compact 规则不对，而是读模型不对。

现状里：

- 前端主列表走的是 `compact=true` 的历史接口
- 后端 compact 规则本身也确实只保留 user message 和 final assistant message
- 但分页入口仍然建立在 raw messages 上，再做 compact

这意味着：

- 主列表读的是“原始消息流的一个窗口”
- 而不是“按轮次投影后的展示历史”

一旦某一轮工具很多，raw message 数量暴涨，主列表分页就会失真。  
所以这不是单纯的分页参数问题，而是“主列表错误地依赖了原始消息存储作为读模型”。

## 设计结论

主列表和过程明细必须拆成两条独立读路径：

### 主列表

只读 turn-level projection：

- user message
- final assistant message
- process 计数摘要

### `showprocess`

只读 turn-level process projection：

- thinking
- tool call
- tool result
- 中间 assistant 片段

不允许再让主列表通过 `/messages?compact=true` 从 raw messages 临时拼装出来。  
长期来看，`compact history` 必须从“运行时转换逻辑”升级为“持久化 read model”。

## 为什么要做 Turn Projection

当前系统已经有一个稳定的 turn 标识基础：

- 消息元数据里的 `conversation_turn_id`

代码位置：

- [core/messages.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/core/messages.rs:306)

而且当前 `showprocess`、realtime、runtime snapshot 等能力也都已经围绕 turn 在工作。  
所以长期方案不应该继续围绕“消息数组里找下一个 user message”这种扫描式逻辑演进，而应该正式把 turn 提升为一等实体。

## 目标架构

## 一、保留 raw messages 作为事实源

raw messages 继续存在，职责不变：

- 完整保留 user / assistant / tool / 中间片段
- 作为审计、重建、调试和回放的 source of truth

但是：

- 主聊天列表不再直接读取 raw messages
- `showprocess` 也不再通过“扫完整 session raw messages 再切 turn”的方式生成

raw messages 只负责写入和事实保留，不再直接承担历史 UI 的主读模型。

## 二、新增 turn 级历史投影

建议新增独立的 read model，例如：

- `conversation_turns`

每一行代表一个 turn，在主列表中对应一组可展示历史。

### 主键建议

- `conversation_id`
- `turn_key`

其中：

- 优先使用 `conversation_turn_id`
- 对历史脏数据或旧数据，允许退化为 `user_message_id`

换句话说，长期读模型里应当明确一个统一键：

- `turn_key = conversation_turn_id || user_message_id`

## 三、新增 turn 级过程明细投影

建议新增独立的过程读模型，例如：

- `conversation_turn_process_items`

每一行表示该 turn 内一个需要 `showprocess` 时展示的过程项。

类型可以标准化为：

- `assistant_thinking`
- `assistant_tool_call`
- `tool_result`
- `assistant_intermediate_text`

这样主列表和过程明细从存储层开始就彻底解耦。

## 读模型建议

## 一、`conversation_turns`

建议字段至少包含：

- `conversation_id`
- `turn_key`
- `conversation_turn_id`
- `user_message_id`
- `user_created_at`
- `user_sort_key`
- `user_payload_json`
- `final_assistant_message_id`
- `final_assistant_created_at`
- `final_assistant_payload_json`
- `has_process`
- `tool_call_count`
- `thinking_count`
- `process_message_count`
- `turn_status`
- `created_at`
- `updated_at`

说明：

- `user_payload_json` 存主列表需要的用户消息展示数据
- `final_assistant_payload_json` 存主列表需要的最终 assistant 展示数据
- `turn_status` 表示该轮是否 `open / completed / stopped / error`
- `user_sort_key` 作为主列表排序与 cursor 分页基准

### 这里的关键点

主列表不再需要根据 raw messages 重新计算：

- 哪条是用户消息
- 哪条是最终 assistant
- 这轮有没有过程
- 过程里有几个工具、几段 thinking

这些在投影写入时就应该定型。

## 二、`conversation_turn_process_items`

建议字段至少包含：

- `conversation_id`
- `turn_key`
- `ordinal`
- `source_message_id`
- `item_type`
- `role`
- `display_payload_json`
- `created_at`

说明：

- `ordinal` 保证一轮内过程展示顺序稳定
- `display_payload_json` 存前端可直接渲染的数据，不要求前端再从 raw message 临时提取 segment
- `source_message_id` 用于回溯到原始消息

### 关键原则

`showprocess` 查的是“这轮过程明细投影”，不是“整段原始消息再临时 compact”。

## 写入链路

## 一、在消息持久化后同步驱动 turn projector

建议在 raw message 成功落库后，统一进入一个 `turn_projector`：

- 输入：当前消息
- 输出：增量更新 `conversation_turns` 与 `conversation_turn_process_items`

不要把这个逻辑继续散落在 API handler 或前端 helper 里。

## 二、不同消息类型的投影规则

### 1. user message

创建或初始化一个 turn：

- 建立 `conversation_turns` 行
- 写入 `user_payload_json`
- 初始化 `turn_status=open`

### 2. assistant 中间消息

如果属于过程消息：

- 更新 `thinking_count / tool_call_count / process_message_count`
- 生成一个或多个 `conversation_turn_process_items`
- 不写入主列表最终答复字段

### 3. tool message / tool result

- 只进入 `conversation_turn_process_items`
- 更新 `process_message_count`
- 不影响主列表 final assistant payload

### 4. 最终 assistant message

满足“最终答复”判定后：

- 更新 `final_assistant_message_id`
- 更新 `final_assistant_payload_json`
- 标记 `turn_status=completed`

这里应沿用现有的最终 assistant 选择语义，而不是让前端再猜。

现有相关逻辑位置：

- [history_process_support.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/sessions/history_process_support.rs:181)

但长期方案里，这段逻辑应搬进 projector/rebuilder，而不是在每次请求主列表时运行。

## 三、投影更新必须幂等

要求：

- 同一条 message 重放、补写、恢复时，不会重复累加 process 计数
- 同一个 turn 重新生成最终 assistant 时，旧 projection 能被覆盖或重算

建议：

- 每条 process item 用 `source_message_id + derived_sub_index` 做唯一键
- turn summary 字段允许 `upsert`
- 提供 `rebuild_turn(conversation_id, turn_key)` 能力做强制重建

## 四、删除与修复场景要支持重建

长期方案里必须预留：

- 通过 raw messages 重新构建单个 turn projection
- 通过 raw messages 重新构建整段 session projection

否则后面处理：

- 消息删除
- 数据回填
- 逻辑升级
- 历史数据修复

会非常被动。

## 读取接口设计

## 一、废弃主列表对通用 `/messages?compact=true` 的依赖

主列表不应继续依赖：

- `/conversations/:id/messages?compact=true&strategy=v2`

这个接口最多保留给：

- 调试
- 管理后台
- 兼容旧逻辑的过渡期

但不应该再作为正式聊天主列表数据源。

## 二、主列表改为专用 turn history API

建议新增：

- `GET /conversations/:id/history-turns?limit=50&cursor=...`

返回结构建议：

```json
{
  "items": [
    {
      "turnKey": "turn_xxx",
      "conversationTurnId": "turn_xxx",
      "userMessage": { "...": "..." },
      "finalAssistantMessage": { "...": "..." },
      "historyProcess": {
        "hasProcess": true,
        "toolCallCount": 6,
        "thinkingCount": 3,
        "processMessageCount": 14
      },
      "turnStatus": "completed"
    }
  ],
  "nextCursor": "opaque_cursor",
  "hasMore": true
}
```

### 设计要求

- `items` 只包含主列表需要的内容
- 不夹带 tool/process 明细
- 排序稳定，按 `user_sort_key desc`
- `cursor` 必须 opaque，不允许前端自己推 offset

## 三、`showprocess` 改为专用 turn process API

建议新增或替换为：

- `GET /conversations/:id/history-turns/:turnKey/process`

如果单轮过程也可能很大，可以直接支持 cursor：

- `GET /conversations/:id/history-turns/:turnKey/process?cursor=...&limit=100`

返回结构建议：

```json
{
  "items": [
    {
      "itemType": "assistant_tool_call",
      "displayPayload": { "...": "..." }
    },
    {
      "itemType": "tool_result",
      "displayPayload": { "...": "..." }
    }
  ],
  "nextCursor": null,
  "hasMore": false
}
```

### 原则

- 主列表和过程接口完全分离
- `showprocess` 不再扫全 session raw messages
- 明细接口直接按 `turn_key` 查 projection

## 四、兼容旧 turn id 入口

由于当前前端部分逻辑仍以 `user_message_id` 为关联键，过渡期建议兼容：

- `turnKey`
- `conversation_turn_id`
- `user_message_id`

但长期对外主键应统一成：

- `turnKey`

前端不应继续把“用户消息 id”当成过程明细的唯一主键。

## 为什么必须改成 Cursor，而不是 Offset

长期方案里不建议再让主列表使用：

- `offset = 当前已加载消息数`

原因：

- 主列表数据不是 raw messages，也不是单条 message 的稳定数组
- 一轮天然包含 user + final assistant 两个主展示节点
- 展开 `showprocess` 后还会在视觉上插入额外节点
- offset 容易和“实际渲染条数”混淆

cursor 的好处是：

- 只围绕 turn 排序边界分页
- 不受前端已展开过程面板影响
- 不依赖客户端推断“已经加载了多少条”
- 更适合并发写入、补消息、恢复场景

推荐 cursor 组成：

- `user_created_at`
- `user_message_id`

必要时可再带：

- `turn_key`

保证排序稳定且可去重。

## 前端改造方案

## 一、主列表 store 变成 turn-aware

前端 store 不再以“消息数组”作为主历史真相，而应改为：

- `historyTurnsBySession`
- `historyTurnOrder`
- `historyTurnsNextCursor`
- `historyTurnsHasMore`

每个 turn item 里只放：

- user message
- final assistant message
- historyProcess summary

## 二、主列表渲染时再 flatten

UI 层仍然可以把一个 turn 渲染成两条主消息：

- user bubble
- final assistant bubble

但这个 flatten 发生在 view model 层，不是分页层。

分页层永远按 turn 走，不按渲染条数走。

## 三、`showprocess` 缓存独立

建议单独维护：

- `turnProcessBySessionAndTurn`
- `turnProcessLoadingState`
- `turnProcessNextCursor`

要求：

- 点击 `showprocess` 才发请求
- 过程明细缓存不参与主列表分页计数
- 展开与收起只影响 UI，不影响主历史 cursor

## 四、删除现有主列表 offset 逻辑

长期方案切换完成后，前端应移除这类逻辑：

- `countLoadedBaseMessages(current.messages)`

因为它属于“消息数组分页”时代的思路，不再适用于 turn projection。

## 为什么不复用 Turn Runtime Snapshot

仓库里已经有 turn runtime snapshot 能力，但不建议把它直接当主聊天历史读模型。

原因：

- runtime snapshot 关注的是当轮运行时上下文
- 它包含 system messages、selected tools、builtin prompt 等运行态信息
- 这和“聊天主列表展示历史”不是同一个领域模型

所以：

- 可以复用 `conversation_turn_id` 作为主键语义
- 但不要把 runtime snapshot 表/结构硬改成 history projection

这两个模型应该分开维护。

## 数据迁移与切换方案

## 一、先引入 projector，不先切前端

步骤建议：

1. 新增 `conversation_turns`
2. 新增 `conversation_turn_process_items`
3. 接入写时 projector
4. 对新消息开始实时维护 projection

## 二、对历史会话做 backfill

必须提供后台任务：

- 按 session 扫描 raw messages
- 生成 `turn_key`
- 重建 `conversation_turns`
- 重建 `conversation_turn_process_items`

要求：

- 可中断重试
- 幂等
- 可按单个 session 重跑

## 三、双读校验

在前端切换之前，建议短期做双读比对：

- 新 `history-turns` 结果
- 旧 compact builder 结果

校验项：

- user message 是否一致
- final assistant 是否一致
- process 计数是否一致
- turn 顺序是否一致

只有双读比对稳定后，才切主列表。

## 四、前端切换

切换顺序建议：

1. 主列表先切到 `history-turns`
2. `showprocess` 再切到新 `turn process` API
3. 最后下线主列表对 `/messages?compact=true` 的依赖

## 五、最终收口

切换完成后：

- `/messages?compact=true` 从主路径退役
- compact builder 只保留给数据修复、调试或兼容逻辑
- 主历史分页语义在代码层面只剩一种：`turn cursor pagination`

## 测试与验收

## 必测场景

1. 最近 5 轮每轮都有大量 tool/process 记录，主列表仍能稳定翻到更早历史
2. `showprocess` 未点击前，主列表接口响应中不包含过程明细
3. 点击 `showprocess` 后，只拉该 turn 的过程数据
4. 展开或收起 `showprocess` 不影响主列表 `nextCursor/hasMore`
5. 一轮内存在多个 assistant 中间消息，仅最终 assistant 出现在主列表
6. 同一 turn 重新生成最终答复时，projection 能正确覆盖
7. 历史 backfill 后，新旧会话在主列表表现一致
8. 超长会话下主列表首屏加载时间与“最近几轮工具数量”解耦

## 验收标准

长期方案完成后，应该满足：

1. 主列表查询完全不依赖 raw message window
2. 主列表分页单位是 turn，不是 message
3. `showprocess` 数据只在点击后按 turn 查询
4. 主列表返回中不包含 tool/process 明细 payload
5. 工具调用再多，也不会影响更早历史能否被翻到
6. 前端不再通过“当前消息数组长度”推导分页 offset
7. 新旧数据都可以通过 `turn_key` 统一访问

## 最终建议

这次不要再继续修 compact builder 的分页技巧了。  
长期正确方向只有一个：

- 把“聊天主列表历史”从 raw messages 临时计算，升级成 turn 级持久化读模型
- 把“过程明细”从 session 级扫描，升级成 turn 级按需查询
- 把分页从 offset on messages，升级成 cursor on turns

也就是三件事一起成立：

- `Turn Projection`
- `Process Projection`
- `Cursor Pagination`

只有这样，主列表才能真正做到：

- 加载时只看用户消息和最终答复
- 过程数据点击 `showprocess` 时再查
- 工具再多也不会把历史“挤没”
