# Builtin MCP Task Manager Design v2 (UI Confirm + Session/Turn Binding)

## 1. v2中发生了什么变化
此版本在任务创建之前添加了一个人工确认面板。
当模型调用任务创建时， UI必须在输入框上方显示任务列表面板。
用户可以编辑任务、添加任务，然后选择确认或取消。
确认后，后端立即创建任务，并将创建的任务返回给调用方。
每个创建的任务都必须链接到会话和一个对话回合。

---

## 2.核心要求
1.调用创建任务工具时触发审核面板。
2.面板显示在聊天输入区域上方。
3.面板支持编辑和加行操作。
4.面板有确认和取消按钮。
5.确认应保留任务，并将创建的任务返回给工具调用者。
6.每项任务必须存储`session_id`和`conversation_turn_id`。

---

## 3.端到端流程
1.用户发送聊天消息。
2.前端为本轮生成`turn_id` ，并将其与聊天请求一起发送。
3.模型发出创建任务工具调用（例如`add_task` ）。
4.后端不会立即创建任务。它会创建待审核的申请。
5.后端将SSE事件`task_create_review_required`推送到前端。
6.前端打开带有草稿任务列表的输入框上方的任务面板。
7.用户编辑草稿，添加/删除项目，然后单击确认或取消。
8.前端发布决定后端审核端点。
9.如果确认：后端写入任务并返回创建的任务作为工具结果。
10.如果取消：后端向调用方返回已取消/错误的工具结果。

---

## 4. UI设计（输入框上方）

### 4.1位置和风格
-安装在`InputArea`容器中。
-使用绝对定位，以便面板显示在文本区域/输入栏上方。
-保持面板宽度与输入区域宽度对齐。

### 4.2面板内容
- HEADER ： “任务审核”和当前会话/转弯信息。
-可编辑的任务行。
-添加行按钮。
-删除每行的行操作。
-页脚有两个主要操作：确认/取消。

### 4.3每行可编辑字段
- `title` （必填）
- `details`
- `priority` （ `high|medium|low` ）
- `status` （ `todo|doing|blocked|done` ）
- `tags`

### 4.4交互规则
-当任何行的标题为空时，确认将被禁用。
-取消关闭面板并发送取消决定。
-确认发送已编辑的列表，而不是原始草稿列表。

---

## 5.后端协议设计

### 5.1新的SSE事件
在`chat_app_server_rs/src/utils/events.rs`中添加事件常量：
- `task_create_review_required`
- `task_create_review_resolved` （可选，用于UI同步）

### 5.2审核所需的事件负载
例如：
```json
{
  "type": "task_create_review_required",
  "timestamp": "...",
  "data": {
    "review_id": "rev_xxx",
    "session_id": "sess_xxx",
    "conversation_turn_id": "turn_xxx",
    "tool_call_id": "call_xxx",
    "draft_tasks": [
      { "title": "...", "details": "...", "priority": "medium", "status": "todo", "tags": [] }
    ],
    "timeout_ms": 120000
  }
}
```

### 5.3决策API
添加端点（示例） ：
- `POST /api/task-manager/reviews/:review_id/decision`

Request body
```json
{
  "action": "confirm",
  "tasks": [
    { "title": "...", "details": "...", "priority": "high", "status": "todo", "tags": ["feature"] }
  ]
}
```

取消正文：
```json
{
  "action": "cancel",
  "reason": "user_cancelled"
}
```

### 5.4工具结果合约
确认时：
```json
{
  "confirmed": true,
  "created_count": 2,
  "tasks": [...],
  "session_id": "sess_xxx",
  "conversation_turn_id": "turn_xxx"
}
```

取消时：
```json
{
  "confirmed": false,
  "cancelled": true,
  "reason": "user_cancelled"
}
```

---

## 6.会话和回合绑定设计

### 6.1为什么要添加TURN绑定
会话级链接不足以进行审核。
一个会话包含多个回合。
任务应该可以追溯到触发它的确切用户。

### 6.2新标识符
- `session_id` ：现有聊天会话ID。
- `conversation_turn_id` ：每个用户问题/回合一个ID。
- `source_user_message_id` ：该回合的用户消息ID。
- `source_assistant_message_id` ：该回合的助理消息ID （创建时可选）。

### 6.3如何生成turn id
-当用户点击发送时，前端会生成`turn_id`。
-在聊天流请求负载中发送`turn_id`。
-后端在请求上下文和消息元数据中保留`turn_id`。
-任务创建工具从工具上下文中读取`turn_id`。

---

## 7.数据模型更新

### 7.1任务表（最少新字段）
添加字段
- `session_id TEXT NOT NULL`
- `conversation_turn_id TEXT NOT NULL`
- `source_user_message_id TEXT`
- `source_assistant_message_id TEXT`
- `created_by TEXT DEFAULT 'tool'`
- `created_at`
- `updated_at`

### 7.2建议索引
- `(session_id, conversation_turn_id)`
- `(session_id, created_at DESC)`
- `(conversation_turn_id, created_at DESC)`

---

## 8.代码集成点

### 8.1后端
-内置注册表： `chat_app_server_rs/src/services/builtin_mcp.rs`
-工具厂： `chat_app_server_rs/src/core/mcp_tools.rs`
-新服务： `chat_app_server_rs/src/builtin/task_manager/mod.rs`
-上交所活动： `chat_app_server_rs/src/utils/events.rs`
-流回调触发事件： `chat_app_server_rs/src/core/chat_stream.rs`
-审核决策API ： `chat_app_server_rs/src/api` （新路线文件）

### 8.2前端
-流解析器： `chat_app/src/lib/store/actions/sendMessage.ts`
-输入面板主机： `chat_app/src/components/InputArea.tsx`
-新组件： `chat_app/src/components/TaskDraftPanel.tsx`
-决策POST的API客户端方法： `chat_app/src/lib/api/client.ts`

---

## 9.超时和回退策略
-每个评价请求都有超时（例如120秒）。
-在没有决定的情况下超时：自动取消并返回取消结果。
-如果面板意外关闭：视为取消。
-保持工具调用的确定性：未经明确确认，不得创建静默任务。

---

## 10.里程碑

### M1 （必备）
-输入框上方的审核面板。
-确认/取消工作流程。
-确认后保留任务。
-将创建的任务返回给来电者。
-储存`session_id` + `conversation_turn_id`。

m2
-添加message-id绑定字段。
-添加任务事件时间轴。
-添加重试和超时UX提示。

M3
-依次添加任务列表/历史查询。
-添加更丰富的面板操作（批量编辑、模板）。

---

## 11.验收清单
- Create-task工具调用不会在用户确认之前直接写入。
-创建请求时，任务面板显示在输入区域上方。
-用户可以编辑现有草稿行并添加新行。
-确认创建任务和工具结果包括创建的任务。
-取消将生成已取消的结果，并且不写入任何任务。
-每个创建的任务都有`session_id`和`conversation_turn_id`。
