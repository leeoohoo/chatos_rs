# Chatos 注册用户自动开通 Harness 账号与空间方案

## 结论

可以在不改 Harness 代码的情况下实现。推荐在 `chatos` 的 `user_service` 注册流程里增加一个 Harness provisioning 集成：用户在 Chatos 注册成功后，调用 Harness 现有 HTTP API 创建同名/映射账号，再用新用户自己的 Harness token 创建 root space。这样 Harness 会自动把该用户设为空间 owner。

`2.0.1` 分支第一版实现位置：

- `user_service/backend/src/config.rs`：新增 Harness provisioning 配置。
- `user_service/backend/src/integrations.rs`：新增 Harness public-register、登录复用、root space 初始化逻辑。
- `user_service/backend/src/api/auth.rs`：公开注册成功后触发 Harness provisioning。
- `user_service/backend/src/api/users.rs`：管理员创建启用用户后触发 Harness provisioning。
- `user_service/backend/src/models.rs`、`user_service/backend/src/store.rs`：新增 `harness_provisioning` Mongo 状态记录。
- `POST /api/users/:id/harness-provisioning/retry`：super_admin 可对失败记录发起手动重试。
- `user_service/frontend/src/pages/UsersPage.tsx`：用户管理页展示 Harness 状态，失败时提供重试按钮。
- `.env.example`：新增 Harness provisioning 环境变量示例。

第一版不会改变 Chatos 注册接口响应结构；Harness 同步失败不会阻断 Chatos 用户创建。当前实现会写入 `harness_provisioning` 状态，用户列表返回的 `harness_provisioning` 字段可用于管理端展示；失败记录会暂存加密后的注册密码，手动重试成功后清空。后续如果要更自动化，可以再加后台定时重试。

我已核对本地代码和线上实例：

- Chatos 注册入口：`user_service/backend/src/api/auth.rs` 的 `POST /api/auth/register`。
- Chatos 已有下游同步模式：`user_service/backend/src/integrations.rs` 用 `reqwest` 同步 memory/task-runner，可沿用这个风格。
- Harness 公开注册接口：`POST /api/v1/register`，入参 `uid/email/display_name/password`，返回 `access_token`。
- Harness 管理员用户接口：`POST /api/v1/admin/users`，需要 admin token。
- Harness 空间创建接口：`POST /api/v1/spaces`，用普通用户 token 创建 root space 时，创建者会自动获得 `space_owner` 成员关系。
- 线上 `http://8.155.171.124:3000` 已验证：管理员登录可用，当前账号是 admin，`user_signup_allowed=true`，`/api/v1/admin/users` 可访问。

## 推荐流程

### 主流程：使用 Harness 公开注册

适用于当前线上配置，因为 Harness `user_signup_allowed=true`。

1. 用户调用 Chatos `POST /api/auth/register`。
2. Chatos 正常写入自己的 `users` 集合。
3. Chatos 立即调用 Harness：

```http
POST {HARNESS_BASE_URL}/api/v1/register
Content-Type: application/json

{
  "uid": "<harness_uid>",
  "email": "<harness_email>",
  "display_name": "<display_name>",
  "password": "<用户注册时的明文密码>"
}
```

4. Harness 返回 `access_token` 后，Chatos 用这个 token 创建用户 root space：

```http
POST {HARNESS_BASE_URL}/api/v1/spaces
Authorization: Bearer <new_user_harness_access_token>
Content-Type: application/json

{
  "identifier": "<space_identifier>",
  "parent_ref": "",
  "description": "Chatos workspace for <username>",
  "is_public": false
}
```

5. Chatos 返回自己的登录 token 给前端。

这个路径的优势是：不需要 Harness admin token；空间创建者就是新用户本人；Harness 自动生成 root space owner 成员关系。

### 兜底流程：Harness 关闭公开注册时使用 admin API

如果以后 `user_signup_allowed=false`：

1. Chatos 用配置中的 Harness admin PAT，或用 admin 用户名密码换取短期 token。
2. 调用：

```http
POST {HARNESS_BASE_URL}/api/v1/admin/users
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "uid": "<harness_uid>",
  "email": "<harness_email>",
  "display_name": "<display_name>",
  "password": "<用户注册时的明文密码>"
}
```

3. 再用新用户账号密码登录 Harness `POST /api/v1/login`，拿新用户 token 创建 root space。
4. 如果新用户登录失败，则用 admin token 创建空间后调用 `POST /api/v1/spaces/{space_ref}/members` 给用户加 `space_owner`，但这种方式下 `created_by` 会是管理员，不是最理想路径。

## 身份映射规则

Chatos 当前只把用户名做 trim/lowercase，没有限制字符集；Harness 的 `uid` 和 root space `identifier` 只能包含 `[a-zA-Z0-9-_.]`，root space 还不能是纯数字、`api`、`git`，也不能以 `.git` 结尾。因此不能直接假设 Chatos username 一定可作为 Harness uid。

建议规则：

- `harness_uid`：优先使用合法的 Chatos username；不合法时用 `chatos-<user_id前12位>`。
- `harness_email`：如果 Chatos username 是邮箱就直接用；否则用 `<harness_uid>@${HARNESS_SYNTHETIC_EMAIL_DOMAIN}`，默认域名如 `chatos.local`。
- `space_identifier`：用 `${HARNESS_SPACE_PREFIX}${harness_uid}`，默认 `u-` 前缀，避免纯数字和保留字冲突。
- 所有映射结果写入 Chatos 的 provisioning 状态表，后续重试和排查都以这个映射为准。

## 幂等与失败处理

推荐做成“同步尝试 + 状态记录 + 可重试”的模式。

新增 MongoDB 集合建议命名为 `external_provisioning_jobs` 或 `harness_provisioning_records`，字段包括：

```json
{
  "user_id": "...",
  "username": "...",
  "harness_uid": "...",
  "harness_email": "...",
  "space_identifier": "...",
  "status": "pending|provisioned|failed|conflict",
  "attempts": 0,
  "last_error": null,
  "created_at": "...",
  "updated_at": "..."
}
```

`2.0.1` 实现采用集合名 `harness_provisioning`，字段包括 `user_id`、`harness_uid`、`harness_email`、`space_identifier`、`status`、`attempts`、`last_error`、`last_attempt_at`、`provisioned_at`、`encrypted_password`、`created_at`、`updated_at`。`user_id` 为唯一索引，`status` 建普通索引。

注意：Harness 创建账号需要明文密码，而 Chatos 只保存密码哈希。如果要异步重试并保持两个系统密码一致，只能在注册时短期加密保存待同步密码，成功后立刻清空；可以复用 `user_service/backend/src/secrets.rs` 的 AES-GCM 加密能力。如果不接受短期保存明文密码，则必须在注册请求内同步调用 Harness，失败后让用户稍后重试或走人工修复。

错误策略建议：

- Harness 网络失败或 5xx：Chatos 注册不回滚，记录 `failed`，后台重试。
- Harness 用户已存在且能用同一密码登录：视为幂等成功，继续创建/确认空间。
- Harness 用户已存在但密码不匹配：标记 `conflict`，需要管理员处理。
- Harness 空间已存在且当前用户可访问：视为幂等成功。
- Harness 空间已存在但当前用户无权限：标记 `conflict`，避免误绑定别人的空间。

## 配置项

建议加到 `.env.example` 和生产 `/etc/chatos/user-service.env`：

```env
HARNESS_PROVISIONING_ENABLED=true
HARNESS_BASE_URL=http://8.155.171.124:3000
HARNESS_PROVISIONING_MODE=public_register
HARNESS_SYNTHETIC_EMAIL_DOMAIN=chatos.local
HARNESS_SPACE_PREFIX=u-
HARNESS_REQUEST_TIMEOUT_MS=5000

# 仅 mode=admin 时需要。优先用 PAT，避免长期保存管理员密码。
HARNESS_ADMIN_TOKEN=
HARNESS_ADMIN_USERNAME=
HARNESS_ADMIN_PASSWORD=
```

## 实施步骤

1. 在 `user_service/backend/src/config.rs` 增加 Harness provisioning 配置。
2. 新建 `user_service/backend/src/integrations/harness.rs`，封装 `register/login/create_space/admin_create_user` 请求。
3. 在 `user_service/backend/src/api/auth.rs` 的注册成功路径调用 provisioning；最小版本可同步调用，正式版本建议加状态表和重试。
4. 为 `create_user` 管理员创建用户路径也加同样 provisioning，避免后台管理员手工创建 Chatos 用户时漏同步。
5. 增加单元测试覆盖映射规则、已存在用户、空间已存在、Harness 失败不影响 Chatos 注册等情况。
6. 生产启用前先用测试账号跑一遍端到端：Chatos 注册、Harness 用户可登录、Harness root space 存在且用户为 `space_owner`。

## 不建议的方案

不建议直接写 Harness 数据库。Harness 创建用户、token、space、membership 时会走校验、事件、权限和缓存逻辑；绕过 API 直接写库很容易漏掉关联表或破坏权限状态，后续升级也不稳定。

## 二期建议

- Chatos 修改密码时同步调用 Harness `PATCH /api/v1/user` 或管理员用户更新接口。
- Chatos 禁用/删除用户时同步禁用或删除 Harness 用户。
- 在 Chatos 管理后台显示 Harness provisioning 状态和手动重试按钮。
- 如果后续有统一身份需求，改为 SSO/OIDC 会比长期维护双密码更干净。
