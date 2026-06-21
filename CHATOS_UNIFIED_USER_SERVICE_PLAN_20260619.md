# Chatos 统一用户微服务方案

## 1. 这次需求的准确理解

这次不是在现有 `task_runner` 账号体系上做一个小补丁，而是要把：

- `ChatOS` 的真实用户
- `Task Runner` 的用户
- `Task Runner` 的 agent 账号

统一收拢到一个独立的用户微服务里，由它负责：

- 用户注册、登录、禁用、角色管理
- agent 账号管理
- 真实用户与 agent 的归属关系
- 面向 `chatos` 和 `task_runner` 的 token 签发与验签
- 跨服务的统一身份声明

`chatos` 和 `task_runner` 以后都不再维护各自独立的用户表与登录逻辑，只消费统一用户微服务提供的身份能力。

## 2. 当前项目里的现状

### 2.1 ChatOS 现在有自己的一套用户体系

当前实现位置：

- `chat_app_server_rs/src/api/auth.rs`
- `chat_app_server_rs/src/core/auth.rs`
- `chat_app_server_rs/src/repositories/auth_users.rs`

现状特点：

- `chatos` 自己维护 `auth_users`
- 登录 token 由 `chatos` 自己签发和校验
- token 是兼容式自签名 token，不是统一身份服务发出的跨服务 token

### 2.2 Task Runner 现在也有自己的一套用户体系

当前实现位置：

- `task_runner_service/backend/src/auth/service/login.rs`
- `task_runner_service/backend/src/auth/service/users.rs`
- `task_runner_service/backend/src/models/user.rs`
- `task_runner_service/backend/migrations/0008_users_and_task_creator.sql`
- `task_runner_service/backend/migrations/0009_user_roles.sql`

现状特点：

- `task_runner` 自己维护 `users`
- 角色只有 `admin | agent`
- `/api/auth/agent-token` 允许用 agent 用户名密码换 token
- token 本质上是 `task_runner` 进程内存 session，不是持久化统一票据

### 2.3 ChatOS 当前通过联系人配置保存 Task Runner agent 账号密码

当前实现位置：

- `chat_app_server_rs/src/services/chatos_memory_mappings/contacts.rs`
- `chat_app_server_rs/src/repositories/chatos_memory_mappings/contacts.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/runtime_context.rs`
- `chat_app_server_rs/src/services/task_runner_api_client.rs`

现状特点：

- `chatos_contacts` 里直接保存：
  - `task_runner_base_url`
  - `task_runner_username`
  - `task_runner_password`
- `chatos` 在运行时拿这组账号密码调用 `task_runner /api/auth/agent-token`
- 再把换到的 Bearer token 放进 `/mcp` 请求头

### 2.4 当前最核心的问题

- `chatos` 用户和 `task_runner` 用户是两套数据，身份割裂
- `task_runner` agent 账号没有成为统一身份模型中的一等对象
- “真实用户拥有哪个 agent” 目前没有中心化治理
- `chatos` 侧长期保存 `task_runner` agent 密码，安全边界不干净
- `task_runner` token 是内存 session，服务重启即失效，且不适合作为跨服务统一认证基础
- 任务创建人现在主要是 `task_runner` 本地用户，不足以表达“真实用户 + 代理 agent”的双重归属

## 3. 目标

本次改造后的目标应该是：

1. 在仓库里新增一个独立的用户微服务，统一管理 `ChatOS 用户` 和 `Task Runner agent`。
2. `chatos` 与 `task_runner` 都不再维护独立的登录真相源。
3. agent 必须明确隶属于某个真实用户。
4. `chatos` 调用 `task_runner` 时，使用“真实用户授权下签发的 agent 委托 token”，而不是长期保存 agent 密码。
5. `task_runner` 的任务归属既能表达真实用户，也能表达具体执行 agent。
6. 后续用户管理入口应统一到用户微服务，而不是 `task_runner` 再保留自己的用户后台。

## 4. 总体方案

## 4.1 新增服务

建议新增：

- `user_service_rs/`

建议同时新增共享 crate：

- `crates/chatos_identity/`

职责划分：

- `user_service_rs`
  - 用户与 agent 的真相源
  - 登录、token 签发、token 交换、禁用、角色管理
  - 服务间验签元数据发布
- `crates/chatos_identity`
  - 统一 claims 定义
  - token 校验与声明结构
  - user service client
  - 公共错误码与权限模型
- `chat_app_server_rs`
  - 作为业务服务消费统一身份
  - 保留现有业务 API，不再自管用户
- `task_runner_service/backend`
  - 作为资源服务消费统一身份
  - 只做任务权限校验，不再自管用户

## 4.2 统一身份模型

建议把身份拆成三类主体：

- `human_user`
  - 真实用户
  - 登录 `chatos`
  - 拥有一个或多个 agent
  - 可以自助创建、查看、禁用、重置自己名下的 agent 账号
- `agent_account`
  - 供 `task_runner MCP` 使用的代理账号
  - 必须归属到某个真实用户
  - 不再是 task runner 本地私有用户
- `service_client`
  - 服务到服务调用主体
  - 比如 `chatos-backend`、`task-runner-backend`

注意区分两个不同概念：

- `ChatOS contact/memory agent id`
- `Task Runner agent account id`

这两个现在在代码里都被叫做 `agent`，但不是同一个对象。新方案里必须分开命名，避免继续混淆。

建议命名：

- `contact_agent_id` 继续表示 ChatOS 里的会话/联系人 agent
- `task_runner_agent_account_id` 表示统一用户服务里的 agent 账号

## 5. 数据模型建议

## 5.1 user_service 自己维护的数据

建议核心表：

### `users`

- `id`
- `username`
- `display_name`
- `password_hash`
- `role`
- `status`
- `created_at`
- `updated_at`
- `last_login_at`

建议角色：

- `super_admin`
- `user`

### `agent_accounts`

- `id`
- `username`
- `display_name`
- `password_hash`
- `owner_user_id`
- `status`
- `created_at`
- `updated_at`
- `last_login_at`

说明：

- 第一阶段可以保留 `username/password`，兼容你现在“agent 账号密码换 key”的思路。
- 但最终目标是不再要求 `chatos` 长期保存这份密码。

### `service_clients`

- `id`
- `client_id`
- `client_secret_hash`
- `allowed_audiences`
- `status`

### `token_revocations`

- `jti`
- `subject_id`
- `revoked_at`
- `reason`

如果第一阶段不做 refresh token，可以先只有 access token + revocation。

## 5.2 ChatOS 侧需要保留的关联数据

当前 `chatos_contacts` 里的 task runner 配置建议调整为：

- `task_runner_enabled`
- `task_runner_base_url`
- `task_runner_agent_account_id`
- `task_runner_agent_username_snapshot`

需要淘汰的字段：

- `task_runner_username`
- `task_runner_password`

第一阶段可以双写兼容，第二阶段删除。

## 5.3 Task Runner 任务模型需要补强

当前 `task_runner` 已有：

- `creator_user_id`
- `creator_username`
- `creator_display_name`

这不够表达“真实用户 + agent”的双重归属。建议补充：

- `owner_user_id`
- `owner_username`
- `owner_display_name`
- `creator_agent_account_id`
- `creator_agent_username`
- `auth_subject_type`

推荐语义：

- `owner_user_id`
  - 真实拥有该任务的人
- `creator_agent_account_id`
  - 实际通过 MCP 创建/操作任务的 agent
- `auth_subject_type`
  - `human_user | agent_account | service_client`

这样后续才好支持：

- 用户查看自己所有 agent 产生的任务
- 单个 agent 只能看自己的任务
- 管理员看全局

## 6. Token 与认证模型

## 6.1 不建议把“ChatOS 用户 token + agent 密码”直接发给 Task Runner

你的原始想法是：

- `chatos` 透传 agent 用户名密码换 key 时，再加上 chatos 用户 token

这个方向是对的，但更合理的落点不是让 `task_runner` 同时处理两种身份，而是：

- `chatos` 把当前登录用户 token 发给 `user_service`
- `user_service` 校验该用户是否拥有目标 agent
- `user_service` 再签发一个面向 `task_runner` 的短期委托 token
- `task_runner` 只接收这一种标准 Bearer token

这样职责最清楚：

- `user_service` 负责身份真相与委托关系
- `task_runner` 负责资源权限判断

## 6.2 建议的 token 类型

### 用户登录 token

用于：

- `chatos` 用户登录态
- `chatos` 调用 `user_service` 的受保护接口

关键 claims 建议：

- `iss = user_service`
- `aud = chatos`
- `sub = user:<user_id>`
- `role = user | super_admin`
- `jti`
- `exp`

### agent 委托 token

用于：

- `chatos -> task_runner /mcp`
- `chatos -> task_runner protected api`

关键 claims 建议：

- `iss = user_service`
- `aud = task_runner`
- `sub = agent:<agent_account_id>`
- `principal_type = agent_account`
- `agent_account_id`
- `owner_user_id`
- `owner_username`
- `scopes`
- `client_id = chatos-backend`
- `exp`
- `jti`

### 服务 token

用于：

- `chatos-backend -> user_service`
- `task-runner-backend -> user_service`

可选，如果第一阶段先只依赖用户 token + JWKS 校验，也可以暂缓。

## 6.3 token 校验方式

推荐：

- `user_service` 使用 JWT 签发 access token
- `chatos` 与 `task_runner` 通过 `JWKS` 或统一公钥配置做离线验签
- 对禁用用户、吊销 token、强制踢出这类场景，保留 `introspect/revoke` 能力

原因：

- 比 `task_runner` 当前内存 session 更适合微服务
- 不需要每次请求都回源 user service
- 服务重启不影响 token 有效性

## 7. 核心调用链

## 7.1 用户登录 ChatOS

调用链建议：

- 前端仍调用 `chat_app_server_rs /api/auth/login`
- `chat_app_server_rs` 作为 BFF 转发到 `user_service /api/auth/login`
- 返回的 token 由 `user_service` 签发
- `chatos` 本地不再自己签 token

这样前端改动最小。

## 7.2 ChatOS 调用 Task Runner

建议调用链：

1. 用户先登录 `chatos`
2. `chatos` 拿到当前用户 token
3. `chatos` 根据联系人配置取出 `task_runner_agent_account_id`
4. `chatos` 调用 `user_service /api/token/exchange/task-runner`
5. `user_service` 校验：
   - 用户 token 有效
   - 该 agent 归属当前用户
   - agent 状态可用
6. `user_service` 返回短期 `task_runner` 委托 token
7. `chatos` 把该 token 放入 `Authorization: Bearer ...`
8. `task_runner` 用统一 claims 做权限判断

第一阶段为了兼容旧配置，可以让 exchange 接口支持可选参数：

- `agent_username`
- `agent_password`

但它们只应该发给 `user_service`，不应该再直接发给 `task_runner`。

## 7.3 Task Runner 处理请求

`task_runner` 收到 token 后只做三件事：

1. 验签
2. 解析 claims
3. 按 claims 做资源权限控制

它不再需要：

- 本地登录
- 本地用户表真相源
- 本地 `/api/auth/agent-token`

## 8. API 方案建议

## 8.1 user_service 对外接口

### 用户登录

`POST /api/auth/login`

### 当前用户

`GET /api/auth/me`

### 用户管理

- `GET /api/users`
- `POST /api/users`
- `PATCH /api/users/:id`

### agent 管理

- `GET /api/agent-accounts`
- `POST /api/agent-accounts`
- `PATCH /api/agent-accounts/:id`
- `POST /api/agent-accounts/:id/reset-password`

这里要明确一条业务规则：

- 普通真实用户可以创建自己的 agent 账号
- 创建时 `owner_user_id` 默认强制写成当前登录用户，不能由前端自由指定
- 普通真实用户只能管理自己名下的 agent
- `super_admin` 才能跨用户查看和调整 agent 归属

### token 交换

`POST /api/token/exchange/task-runner`

请求头：

- `Authorization: Bearer <current_user_token>`

请求体建议：

```json
{
  "task_runner_agent_account_id": "agt_xxx",
  "contact_id": "contact_xxx",
  "legacy_agent_password": "optional"
}
```

返回：

```json
{
  "access_token": "jwt",
  "token_type": "Bearer",
  "expires_in": 3600,
  "principal": {
    "type": "agent_account",
    "agent_account_id": "agt_xxx",
    "owner_user_id": "usr_xxx"
  }
}
```

### 验签元数据

- `GET /.well-known/jwks.json`
- `POST /api/auth/introspect`
- `POST /api/auth/revoke`

## 8.2 ChatOS 侧改造点

需要改造的核心位置：

- `chat_app_server_rs/src/api/auth.rs`
- `chat_app_server_rs/src/core/auth.rs`
- `chat_app_server_rs/src/repositories/auth_users.rs`
- `chat_app_server_rs/src/services/task_runner_api_client.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/runtime_context.rs`
- `chat_app_server_rs/src/services/chatos_memory_mappings/contacts.rs`
- `chat_app_server_rs/src/repositories/chatos_memory_mappings/contacts.rs`

改造方向：

- 登录与注册改为代理 `user_service`
- 本地 token 校验改为验 `user_service` token
- 联系人 task runner 配置从“账号密码”改为“agent_account_id”
- 调用 task runner 前先向 `user_service` 做 token exchange

## 8.3 Task Runner 侧改造点

需要改造的核心位置：

- `task_runner_service/backend/src/auth/*`
- `task_runner_service/backend/src/api/core/auth.rs`
- `task_runner_service/backend/src/api/mcp.rs`
- `task_runner_service/backend/src/models/user.rs`
- `task_runner_service/backend/src/services/task_service/tasks/mutations/creation.rs`
- `task_runner_service/backend/src/mcp_server/support/access.rs`

改造方向：

- 本地 `AuthService` 从“用户真相源”降级为“统一 claims 解析器”，或直接删除
- 删除本地 `/api/auth/login` 与 `/api/auth/agent-token` 的主路径地位
- `/mcp` 和保护接口统一校验 `user_service` token
- 任务创建时写入：
  - `owner_user_id`
  - `creator_agent_account_id`
- 权限判断从“本地 user.id”升级为“owner + agent 双字段”

## 9. 权限规则建议

### `super_admin`

- 管理所有用户与 agent
- 查看所有任务
- 操作所有任务

### `human_user`

- 查看自己拥有的所有任务
- 查看自己拥有的所有 agent
- 管理自己名下 agent
- 可以新建自己的 agent 账号，默认不能把 agent 创建到别人名下

### `agent_account`

- 只能访问自己创建或被授权的任务
- 默认按以下条件过滤：
  - `owner_user_id == token.owner_user_id`
  - `creator_agent_account_id == token.agent_account_id`

这比当前只看 `creator_user_id` 更符合“每个 agent 隶属一个真实用户”的要求。

## 10. 迁移策略

## 10.1 ChatOS 用户迁移

来源：

- `chat_app_server_rs` 的 `auth_users`

迁移目标：

- 导入 `user_service.users`

策略：

- 沿用现有 username
- 保留角色
- 第一期可兼容旧密码 hash
- 迁移完成后 `chatos` 不再直连本地 `auth_users`

## 10.2 Task Runner 用户迁移

来源：

- `task_runner users`

迁移目标：

- `admin` 类账号迁到 `users`
- `agent` 类账号迁到 `agent_accounts`

需要解决的问题：

- 现在旧 `task_runner` agent 未必都能自动知道归属哪个真实用户

建议做法：

- 先基于 `chatos_contacts.task_runner_username` 做第一轮匹配
- 匹配到的 agent 写入 `owner_user_id`
- 匹配不到的 agent 标记为 `unassigned`
- 提供一次性后台修复工具让管理员手工绑定

## 10.3 ChatOS 联系人配置迁移

当前字段：

- `task_runner_username`
- `task_runner_password`

目标字段：

- `task_runner_agent_account_id`

建议步骤：

1. 新增 `task_runner_agent_account_id`
2. 基于旧 username 回填 agent id
3. 运行期优先读取新字段
4. 兼容读取旧字段一段时间
5. 删除旧密码字段

## 10.4 Task Runner 任务数据迁移

当前任务主要只有本地创建人字段。

迁移后建议回填：

- 如果旧 `creator_user_id` 对应迁移后的 `agent_account`
  - 回填 `creator_agent_account_id`
  - 再根据 agent 归属回填 `owner_user_id`
- 如果旧 `creator_user_id` 对应迁移后的真实用户
  - 回填 `owner_user_id`
  - `creator_agent_account_id = null`
- 无法识别的历史任务标记为 `owner_user_id = null`
  - 仅管理员可见
  - 后续人工认领

## 11. 分阶段落地顺序

### 第一阶段：建统一身份服务

- 新增 `user_service_rs`
- 落地 `users`、`agent_accounts`、`token` 能力
- 落地 `crates/chatos_identity`

### 第二阶段：让 ChatOS 改为消费统一身份

- `chatos` 登录改为代理 `user_service`
- `chatos` 本地鉴权改为验统一 token
- 联系人配置新增 `task_runner_agent_account_id`

### 第三阶段：让 Task Runner 改为消费统一身份

- `task_runner` 改为验统一 token
- `/mcp` 使用统一 claims
- 任务模型补 `owner_user_id` 与 `creator_agent_account_id`

### 第四阶段：切换 token exchange

- `chatos` 不再直接找 `task_runner /api/auth/agent-token`
- 改为找 `user_service /api/token/exchange/task-runner`

### 第五阶段：删除旧用户体系

- 删除 `chatos auth_users` 的主链路
- 删除 `task_runner users` 的主链路
- 删除 `task_runner_username/password` 存储

## 12. 我对这次改造的明确建议

### 建议一

不要把“统一用户微服务”做成只是多一个登录接口。

如果不把：

- 真实用户
- agent 账号
- agent 归属关系
- task runner 委托 token

一起纳入，最后仍然会回到“两张用户表 + 一堆同步脚本”的旧问题。

### 建议二

第一阶段允许兼容“agent 账号密码 + 用户 token”的交换方式，但这只能是过渡态。

最终目标一定要收敛到：

- 联系人只绑定 `agent_account_id`
- `chatos` 不再长期保存 agent 密码

### 建议三

任务归属字段必须升级为“双归属”。

只保留当前 `creator_user_id` 不够，因为这只能表示“是谁在 task runner 本地被识别成创建人”，不能表达“这个 agent 实际属于哪个真实用户”。

## 13. 推荐实施结果

落地完成后，系统关系应当变成：

- `user_service` 是唯一身份真相源
- `chatos` 负责业务编排和会话
- `task_runner` 负责任务资源与执行
- `agent` 不再是 task runner 私有用户概念，而是统一身份服务里的代理主体
- `task_runner` 上每条任务都能追溯：
  - 属于哪个真实用户
  - 由哪个 agent 发起

这才是真正把 `ChatOS 用户` 和 `任务用户` 合到一起。
