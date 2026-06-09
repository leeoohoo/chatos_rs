# Task Runner Agent MCP Auth 方案

## 背景

Task Runner 现在已经有登录、用户和任务创建人概念，但下一步的核心目标不是让 AI agent 自己理解账号体系，而是让外部宿主系统代替 AI 完成身份交换和透传。

新的业务约定：

- 管理员是人类管理入口，用来配置用户、查看全局任务和维护系统。
- 除管理员外，其他用户都是 AI agent 身份。
- AI agent 不需要知道用户名、密码、token，也不应该在 prompt、memory、工具描述里看到这些内容。
- Chatos 联系人里可以配置某个 Task Runner agent 用户的用户名和密码。
- 当该联系人调用 Task Runner MCP 工具时，由程序先用配置的用户名密码换取 token。
- 程序调用 Task Runner MCP 时自动透传 token。
- Task Runner 收到 token 后识别 agent 身份，只允许该 agent 查看、创建、执行、修改自己创建的任务。

## 目标

1. 给 Task Runner 提供一套完整 MCP 工具，供 AI agent 调用。
2. MCP 调用链路具备身份识别能力，但身份细节对 AI 不可见。
3. 任务数据按 agent 隔离：
   - agent 只能看到自己的任务。
   - agent 只能操作自己的任务。
   - agent 创建的任务自动写入创建人。
4. 管理员保留全局权限：
   - 可以管理用户。
   - 可以查看/操作所有任务。
   - 可以为 agent 重置密码、启用/禁用。
5. 兼容当前已有 `/mcp` 能力，逐步收紧权限，避免一次性打断现有客户端。

## 身份模型

在现有 `users` 基础上增加角色字段：

```text
role: admin | agent
```

建议默认规则：

- 初始默认用户 `admin` 的角色为 `admin`。
- 前端用户管理页新建用户时，默认角色为 `agent`。
- agent 用户不能登录管理后台，或者登录后只能看到极简的“我的任务”页面。第一阶段可以先禁止 agent 登录 UI，只允许通过 MCP 使用。
- admin 用户可以登录后台，并拥有全局管理权限。

用户表建议新增字段：

```text
role TEXT NOT NULL DEFAULT 'agent'
```

前端用户管理需要展示：

- 用户名
- 显示名
- 角色
- 启用状态
- 最近登录时间
- 创建时间

## Token 交换接口

新增一个专门给宿主程序使用的 token 交换接口。

建议接口：

```http
POST /api/auth/agent-token
Content-Type: application/json

{
  "username": "agent_xxx",
  "password": "******",
  "client": "chatos-contact",
  "contact_id": "optional-contact-id"
}
```

返回：

```json
{
  "access_token": "xxxx",
  "token_type": "Bearer",
  "expires_in": 3600,
  "user": {
    "id": "user-id",
    "username": "agent_xxx",
    "display_name": "某个联系人",
    "role": "agent"
  }
}
```

设计要点：

- 这个接口可以复用现有密码校验逻辑，但返回的是短期 access token。
- token 建议设置过期时间，例如 1 小时或 12 小时。
- token 只保存在宿主程序内存或本地安全存储中，不进入 AI 上下文。
- 账号禁用后，已有 token 应立即失效，至少在下一次校验时拒绝。
- 登录失败返回统一错误，避免泄露用户名是否存在。

第一阶段可以继续使用内存 session token；第二阶段建议改成数据库持久化 token 或签名 JWT。

## Chatos 联系人配置

在 Chatos 的联系人配置中增加 Task Runner MCP 身份配置。

建议字段：

```json
{
  "task_runner": {
    "enabled": true,
    "base_url": "http://127.0.0.1:39090",
    "username": "agent_xxx",
    "password": "******"
  }
}
```

安全要求：

- 密码只允许保存在后端配置或加密存储中。
- 不允许把用户名/密码拼进 system prompt、tool prompt、memory record。
- MCP 工具描述里不出现账号、token、鉴权细节。
- 前端展示密码时只允许重置，不回显明文。

调用流程：

```text
联系人发起对话
  -> Chatos 判断该联系人启用了 Task Runner
  -> 程序用联系人配置的 username/password 调 /api/auth/agent-token
  -> 得到 access_token
  -> 程序创建或调用 Task Runner MCP client
  -> 每次 MCP HTTP 请求自动加 Authorization: Bearer <token>
  -> Task Runner 从 token 识别 agent 用户
  -> MCP 工具只返回该 agent 有权限的数据
```

## MCP 鉴权方式

HTTP MCP 推荐使用标准 Header：

```http
Authorization: Bearer <access_token>
```

Task Runner `/mcp` 当前是公开入口。需要改成：

- 没有 token：拒绝调用需要身份的工具。
- token 对应 admin：允许全局操作。
- token 对应 agent：只允许操作自己创建的任务。

如果未来存在 stdio MCP，则不直接让 AI 传 token，而是由启动 stdio MCP 的宿主进程把 token 放入环境变量或启动配置中：

```text
TASK_RUNNER_AGENT_TOKEN=xxxx
```

然后 stdio MCP client 在转发 HTTP 请求时自动加 Header。

## 权限规则

定义统一的任务访问规则：

```text
admin:
  - 可以 list/get/create/update/delete/run 所有任务
  - 可以管理用户
  - 可以查看所有运行记录和提示

agent:
  - list 只返回 creator_user_id = 当前用户 id 的任务
  - get 只能读取自己的任务
  - create 自动写 creator_user_id / creator_username / creator_display_name
  - update/delete/run 只能操作自己的任务
  - run history 只能查看自己任务的运行记录
  - prompts 只能查看/处理自己任务或运行产生的提示
  - model config 可以只读可用模型，不能管理模型配置
  - remote server、tooling、system settings 默认不可管理
```

任务创建人字段已经存在：

```text
creator_user_id
creator_username
creator_display_name
```

后续需要把所有 task/run/prompt 查询都挂到统一的 `CurrentUser` 权限上下文上，不能只在前端过滤。

## MCP 工具分层

建议 MCP 工具按 agent 场景整理为一组“我的任务”工具，避免暴露管理型工具。

面向 agent 的 MCP 工具：

- `task_create`
- `task_list_my`
- `task_get`
- `task_update`
- `task_delete`
- `task_run`
- `task_cancel_run`
- `task_list_runs`
- `task_get_run`
- `task_list_run_events`
- `task_submit_prompt`
- `task_memory_context`
- `task_memory_records`

面向 admin 的 MCP 工具可以单独保留或后续再开放：

- 用户管理
- 模型配置管理
- 远程服务器管理
- 全局任务检索
- 系统配置读取

工具内部不让 AI 传 `creator_user_id`，创建人始终由 token 决定。

## 后端改造步骤

### 第一阶段：身份和接口

1. `UserRecord` 增加 `role`。
2. SQLite/Mongo 增加 role 字段和索引。
3. 默认 admin 初始化时写入 `role=admin`。
4. 用户管理页支持选择角色，默认 agent。
5. 新增 `/api/auth/agent-token`。
6. token 结构中记录 user id、username、display name、role。

### 第二阶段：API 权限收口

1. 把现有 API 的 `CurrentUser` 增加 role。
2. 所有任务列表接口按 role 过滤：
   - admin 不过滤。
   - agent 自动加 `creator_user_id = current_user.id`。
3. 所有单任务操作前调用统一权限函数：

```text
can_access_task(current_user, task)
can_modify_task(current_user, task)
can_run_task(current_user, task)
```

4. run/prompt/memory 接口通过 task 反查权限。
5. 禁止 agent 调用用户管理、模型配置写入、远程服务器写入等管理接口。

### 第三阶段：MCP 鉴权

1. `/mcp` 支持从 `Authorization` Header 读取 token。
2. MCP service handler 接收 `CurrentUser`。
3. MCP list/get/update/run 等工具全部走同一套权限函数。
4. MCP 返回结果不包含其他 agent 的任务。
5. MCP 工具描述改成“我的任务”语义。

### 第四阶段：Chatos 联系人集成

1. 联系人配置增加 Task Runner 身份字段。
2. 后端读取联系人配置，调用 `/api/auth/agent-token`。
3. token 缓存在联系人会话上下文中。
4. MCP 调用自动附带 `Authorization` Header。
5. token 过期或 401 时自动重新换取。
6. 不把账号、密码、token 写入 prompt、memory、trace 文本。

## 数据迁移策略

已有任务可能没有创建人。建议：

- 对老任务保留 `creator_user_id = null`。
- admin 可以看到所有老任务。
- agent 看不到无创建人的老任务，除非后续管理员手动分配。
- 可以新增一个后台管理动作：把某些任务分配给某个 agent。

已有用户没有 role 时：

- username 等于默认 admin 用户名的设为 `admin`。
- 其他用户设为 `agent`。

## 安全注意事项

- agent token 必须有过期时间。
- 密码不能出现在日志、prompt、memory、MCP tool args 里。
- MCP 错误信息不要返回“用户不存在”这类细节。
- 删除/禁用 agent 后，相关 token 需要失效。
- agent 不能通过传参伪造 `creator_user_id`。
- 后端必须强制过滤，不依赖前端过滤。
- 运行记录、人工提示、memory 查询也要通过任务归属校验。

## 推荐落地顺序

1. 先加 `role` 和 `/api/auth/agent-token`。
2. 再把普通 HTTP API 的任务隔离做好。
3. 然后改 `/mcp` 读取 token，并让 MCP 工具走同一套权限。
4. 最后接 Chatos 联系人配置和自动换 token。

这样可以每一步单独验证，不会一次性把 MCP、UI、联系人配置全部搅在一起。

