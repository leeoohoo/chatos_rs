# 长链接剩余缺口执行计划

## 目标

在现有 `HTTP snapshot + realtime invalidation/push` 架构上，继续收口几个“看起来已经实时化，但实际还没改透”的高价值链路，优先解决：

1. 聊天主链仍然频繁回退到 SSE。
2. `review-repair` 后端内部仍靠 bridge polling。
3. 部分项目运行 / summary / 列表链路仍然是事件到达后全量 HTTP 回拉。

## 优先级

### P1. 聊天主链继续收口

现状：

1. `sendMessage` 仍按发送瞬间的 realtime 连接态决定：
   - `connected` 走 `/chat/send` + realtime stream
   - 非 `connected` 走 `/chat/stream` SSE fallback
2. 这会导致 websocket 正在建连、但尚未切到 `connected` 时，聊天仍然过早回退到 SSE。

执行步骤：

1. 先给发送入口增加短暂 realtime 建连等待窗口，减少“误回退”。
2. 再补充更清晰的回退原因打点，区分：
   - realtime 未连接
   - realtime 建连超时
   - realtime 命令接口失败
3. 最后评估是否把 SSE fallback 收敛成“只在明确失败时启用”的紧急兜底，而不是常规主路径。

验收口径：

1. websocket 正在 `connecting` 时，发送消息优先等待短窗口，不再立刻走 SSE。
2. 只有 realtime 明确不可用或超时后，才回退 `/chat/stream`。

### P1. review-repair 去掉后端 bridge polling

现状：

1. 前端已经靠 realtime 收状态。
2. 但 chat backend 里仍会每 `900/1200ms` 轮询 memory `review_repair_status`，再转发成 realtime 事件。

执行步骤：

1. 先梳理 memory server 当前 job / summary 能否直接发状态事件。
2. 若短期无法做真正上游 push，则先把 bridge polling 收紧成：
   - 更少的轮询次数
   - 更明确的终态判断
   - 更轻的状态查询负担
3. 中期目标再考虑 memory -> chat backend 的直接事件通道。

验收口径：

1. `review-repair` 完成态与失败态能稳定收口。
2. 后端 bridge polling 次数明显减少，或被更直接的状态流替代。

### P2. 项目运行目录 / summary 链路减少整块回拉

现状：

1. `project.run.catalog.updated` 到达后，前端仍会同时重拉成员和 runner script 状态。
2. `conversation.summaries.updated` 到达后，summary 视图仍主要依赖 HTTP reload。

执行步骤：

1. 优先拆分 `project.run.catalog.updated` 与 `project.members.updated` 的刷新职责。
2. 能 patch 的局部状态先 patch，不能 patch 的部分再回拉。
3. summary 视图优先减少“不可见时刷新”和重复回源。

验收口径：

1. 事件到达后不再默认整块刷新多个接口。
2. 不可见面板不会因为事件反复回源。

### P3. 全局列表 invalidation 继续细化

范围：

1. `sessions.updated`
2. `contacts.updated`
3. `projects.updated`
4. `remote_connections.updated`

执行步骤：

1. 保留现有 `silent refresh` 兜底。
2. 逐步把 create/update/delete 场景改成局部 patch 优先。

验收口径：

1. 常见单条更新不再总是触发整表刷新。
2. 仍保留重连或复杂场景下的全量快照兜底。

## 本轮执行顺序

1. 先推进 P1 聊天主链收口。
2. 再处理 P1 `review-repair` bridge polling。
3. 最后看 P2/P3 哪一条改动收益最高、风险最低，继续往下收。
