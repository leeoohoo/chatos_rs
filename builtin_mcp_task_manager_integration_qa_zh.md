# 内置 MCP 任务管理：联调与验收清单（中文）

> 目标：验证“任务创建需弹出可编辑任务列表，确认后创建并返回调用方；任务绑定会话与对话轮次（内部透传）”这条链路完整可用。

## 1. 范围与关键约束

- `session_id`、`conversation_turn_id` 由系统内部自动透传。
- 调用方（模型工具参数）不需要也不应手工传这两个字段。
- 用户每发送一条消息，视为一轮（一个 `conversation_turn_id`）。

## 2. 前置检查（一次性）

1. 后端可启动，数据库初始化成功。
2. 前端可启动，聊天页可正常发送消息。
3. 内置 MCP 已启用 `builtin_task_manager`。

可快速校验：

- 后端路由存在：`POST /api/task-manager/reviews/:review_id/decision`
- 事件常量存在：`task_create_review_required`
- 数据库表/集合存在：`task_manager_tasks`

## 3. 主流程验收（Confirm 路径）

### 步骤 A：触发任务创建

在聊天输入：

- 例如“帮我创建两个任务：1）修复登录重试；2）补充接口测试，优先级高”

预期：

1. 模型触发任务管理工具（`add_task`）。
2. 流式事件中出现 `tools_stream`，其 `content` 内含：
   - `event = task_create_review_required`
   - `data.review_id`
   - `data.session_id`
   - `data.conversation_turn_id`
   - `data.draft_tasks`
3. 输入框上方出现“任务创建确认”面板。

### 步骤 B：在面板编辑

预期能力：

- 可编辑标题/详情/优先级/状态/标签/截止时间。
- 可新增任务。
- 可删除任务。

### 步骤 C：点击“确定并创建任务”

预期：

1. 前端调用 `POST /api/task-manager/reviews/:review_id/decision`，`action=confirm`。
2. 后端解除 review 等待，落库任务。
3. 工具最终结果返回到调用方，含：
   - `confirmed = true`
   - `created_count > 0`
   - `tasks[]`
4. 面板关闭。
5. 本轮对话结束后，任务可按会话/轮次查询。

## 4. 取消流程验收（Cancel 路径）

1. 同样先触发 `task_create_review_required`。
2. 点击“取消”。
3. 预期：
   - decision 接口 `action=cancel`
   - 工具返回 `cancelled = true`
   - 不新增任务记录
   - 面板关闭

## 5. 轮次绑定验收（核心）

同一会话连续发送两条消息，各触发一次任务创建并确认。

预期：

- 两批任务的 `session_id` 相同。
- 两批任务的 `conversation_turn_id` 不同。
- 且分别对应各自那一轮用户消息。

## 6. 异常场景验收

1. **标题为空**：面板不允许确认（或后端返回校验错误）。
2. **review_id 过期/不存在**：decision 返回 `review_not_found`。
3. **超时未确认**：工具返回 `review_timeout`（或 cancelled 语义）。
4. **IPC 未实现提交接口**：前端自动回落 HTTP 提交 decision。

## 7. 数据核对建议

### SQLite

查询示例：

```sql
SELECT id, session_id, conversation_turn_id, title, priority, status, created_at
FROM task_manager_tasks
ORDER BY created_at DESC
LIMIT 20;
```

### MongoDB

集合：`task_manager_tasks`

按 `session_id + conversation_turn_id` 过滤，确认同轮绑定。

## 8. 快速排障

- **面板没弹出**
  - 看是否收到 `tools_stream`。
  - 看 `tools_stream.data.content` 是否可解析成 JSON。
  - 看其中 `event` 是否为 `task_create_review_required`。
- **点确定没生效**
  - 看 decision 接口是否 200。
  - 看 payload 是否为 `action=confirm` 且带有效 `tasks`。
- **任务没落库**
  - 看后端是否走到 `create_tasks_for_turn`。
  - 检查 DB 连接模式（SQLite/Mongo）与表/集合是否存在。

## 9. 最终验收标准（DoD）

- 用户发送消息触发任务创建时，输入框上方稳定弹出可编辑任务面板。
- 支持编辑、增删、确认、取消。
- 确认后任务落库并即时返回调用方结果。
- 每条任务都绑定正确 `session_id` 与 `conversation_turn_id`。
- 调用方无需传 `session_id` / `conversation_turn_id`。
