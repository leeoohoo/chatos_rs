# ChatOS 2.0.14 原生客户端协议

后端改动位于 `chatos_rs` 的 `2.0.14` 分支，基线分支为 `2.0.13`。新协议以 Swift 原生客户端为目标，旧 Electron/Web 客户端不作为长期兼容约束。

## 历史接口

```http
GET /api/chatos/conversations/{conversation_id}/compact-history?limit=50&before={turn_id}
```

响应新增：

- `items[].revision`：消息持久 revision，同一消息每次写入单调增加。
- `items[].sequence_no`：由创建时间和消息 ID 生成的稳定排序号。
- `snapshot_revision`：当前页面内最高 revision。
- `next_before`：继续加载更早 Turn 的 cursor。

Swift 的 `ConversationHistoryMapper` 将用户消息与 final assistant 合并为稳定 `ConversationTurn`，Turn revision 取两者最大值。页面响应只能 merge 到 `ConversationHistoryStore`，不能替换已经加载的历史。

`ConversationTurn` 同时保留项目执行确认上下文。Swift 只接受消息元数据中的明确 `project_id`、`requirement_id`、`execution_group_id` 和当前 `conversation_id` 作为可操作身份；任务图来源字段只用于查询图，不用于猜测确认/停止目标。

任务图查询必须把 `task_runner_async.source_user_message_id`、`source_turn_id` 和会话 ID 原样保留到 `MessageTaskLookup`。项目执行中的 `source_user_message_id` 可能是 execution group，而不是屏幕上用户消息的数据库 ID；丢失它会导致消息声明有任务但 graph 返回空节点。

规划任务图确认与放弃均通过 APISIX：

- `POST /projects/{projectId}/requirements/{requirementId}/confirm-execution`
- `POST /projects/{projectId}/requirements/{requirementId}/stop`，并发送 `discard_tasks: true`

## Realtime

连接流程：

1. `POST /api/chatos/auth/ws-ticket` 获取短期票据。
2. 连接 `wss://.../api/chatos/realtime/ws?ws_ticket=...`。
3. 发送会话订阅：

```json
{
  "type": "subscribe",
  "topics": [{ "scope": "conversation", "id": "conversation-id" }]
}
```

所有事件统一新增：

- `event_id`：服务端生成的 UUID，用于重放去重。
- `event_sequence`：基于 Unix 微秒并通过原子操作保证严格递增。

Swift `ChatOSRealtimeClient` 在 actor 内管理票据、WebSocket、订阅和指数退避重连。View 只接收 `ConversationRealtimeSignal`；终态或持久化事件触发历史增量刷新。

## 当前边界

- event sequence 已能识别进程运行期及正常时钟条件下的事件间隙。
- 服务端尚未提供 `after_event_sequence` 的持久事件回放接口；断线补洞仍需通过历史 snapshot merge。
- SQLite 本地缓存、Token Keychain 和真实登录接入属于下一阶段。
