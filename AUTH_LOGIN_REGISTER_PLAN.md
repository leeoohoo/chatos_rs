# 登录/注册改造方案（V1）

## 1. 目标

在现有聊天系统里补齐最小可用账号体系，先实现：

1. 注册（邮箱 + 密码）
2. 登录（邮箱 + 密码）
3. 基于登录态识别当前用户（`/api/auth/me`）
4. 后端业务接口按登录用户隔离数据（避免前端随意传 `user_id`）

> 本方案优先“尽快可用 + 不大改现有业务结构”，后续再扩展 refresh/logout 多端会话管理。

## 2. 现状排查（当前代码）

1. 后端还没有 auth 路由与中间件；`x-user-id` 仅用于日志字段：
   - `chat_app_server_rs/src/api/mod.rs`
2. 数据库没有 `users` / `auth_sessions` 表：
   - `chat_app_server_rs/src/db/sqlite.rs`
   - `chat_app_server_rs/src/db/mongodb.rs`
3. 前端默认使用硬编码 user id（`custom_user_123` / `default-user`）并把 `user_id` 放在 query/body：
   - `chat_app/src/App.tsx`
   - `chat_app/src/lib/store/ChatStoreContext.tsx`
   - `chat_app/src/lib/store/createChatStoreWithBackend.ts`
   - `chat_app/src/lib/api/client.ts`
4. 会话总结配置仍存在 `default-user` 兜底逻辑：
   - `chat_app_server_rs/src/api/session_summary_job_config.rs`
   - `chat_app_server_rs/src/modules/session_summary_job/config.rs`

## 3. 设计原则

1. **最小侵入**：现有业务表继续用 `user_id` 字段，不做大规模 schema 重构。
2. **先通主链路**：先把“注册/登录 -> 进入聊天 -> 数据隔离”打通。
3. **兼容过渡**：保留一个短期兼容开关，避免老数据立刻不可见。
4. **安全基线**：密码哈希 + JWT 签名 + 过期校验 + 基础限流。

## 4. 技术方案

### 4.1 账号模型

新增 `users`（SQLite）/ `users` collection（Mongo）：

- `id` TEXT/STRING（UUID）
- `email`（唯一）
- `password_hash`
- `display_name`
- `status`（active/disabled）
- `created_at` / `updated_at` / `last_login_at`

索引：
- `email` 唯一索引
- `created_at` 普通索引

### 4.2 密码与 Token

1. 密码哈希：`argon2id`（Rust `argon2` crate）
2. Access Token：JWT（`HS256`）
   - claims：`sub=user_id`、`email`、`exp`、`iat`
   - 默认有效期：2 小时
3. 先不做 refresh（V1 简化），过期后重新登录

新增配置项（`.env`）：
- `AUTH_JWT_SECRET`
- `AUTH_ACCESS_TOKEN_TTL_SECONDS=7200`
- `AUTH_LEGACY_USER_ID_FALLBACK=true`（仅过渡期）

### 4.3 后端接口

新增路由：`/api/auth`

1. `POST /api/auth/register`
   - 入参：`email`, `password`, `display_name?`
   - 校验：邮箱格式、密码长度（>=8）
   - 返回：`user` + `access_token`
2. `POST /api/auth/login`
   - 入参：`email`, `password`
   - 返回：`user` + `access_token`
3. `GET /api/auth/me`
   - Header：`Authorization: Bearer <token>`
   - 返回当前用户信息

错误码约定：
- 400 参数错误
- 401 认证失败/过期
- 409 邮箱已存在

### 4.4 鉴权中间件

新增统一鉴权模块（例如 `core/auth`）：

1. 从 `Authorization` 解析 JWT
2. 校验签名与过期
3. 将 `AuthUser { user_id, email }` 放入 request extensions
4. 提供 `extract_auth_user()` helper 给各 API 使用

### 4.5 业务接口改造策略（重点）

现有很多接口都允许前端直接传 `user_id`。V1 改成：

1. **优先使用 token 中的 user_id**（服务端可信）
2. 请求体/query 里传的 `user_id` 仅做兼容；若与 token 不一致直接 403
3. 逐步移除前端显式传 `user_id`

首批必须接入鉴权的接口：
- sessions / messages / chat_v2 / chat_v3
- projects / terminals
- mcp_configs / ai_model_configs / agents / applications
- user_settings / session_summary_job_config

## 5. 前端改造方案

### 5.1 登录态存储

新增轻量 auth store（Zustand 或 React state）保存：
- `accessToken`
- `currentUser`
- `isAuthenticated`

### 5.2 登录/注册 UI

在 `App` 层增加鉴权门禁：

1. 未登录：展示登录/注册面板
2. 登录后：渲染现有 `ChatInterface`

### 5.3 API Client 统一带 token

在 `chat_app/src/lib/api/client.ts` 的 `request()` 里注入：
- `Authorization: Bearer <accessToken>`

并新增方法：
- `register()`
- `login()`
- `getMe()`

### 5.4 userId 来源切换

把 store 里 `userId` 的来源从硬编码改成 `currentUser.id`，并逐步删除：
- `custom_user_123`
- `default-user`

## 6. 兼容与迁移

### 6.1 老数据兼容（过渡期）

通过 `AUTH_LEGACY_USER_ID_FALLBACK=true` 保留短期兼容：
- 无 token 时允许旧逻辑（仅开发环境）
- 生产环境默认关闭

### 6.2 既有数据归属

如果已有大量 `default-user` 数据，提供一次性脚本：
1. 创建一个初始账号（例如 admin）
2. 将历史 `user_id=default-user` 的数据迁移到该账号 id

> 若你们当前环境数据不重要，可直接跳过迁移脚本。

## 7. 实施分期

### Phase 1（1 天）后端基础能力

1. 增加 users schema/repo/model/service
2. 增加 auth API（register/login/me）
3. 增加 JWT 鉴权中间件
4. `api/mod.rs` 挂载 auth router

### Phase 2（1 天）前端接入

1. 登录/注册页面
2. token 管理与请求头注入
3. App 鉴权门禁 + userId 来源改造

### Phase 3（1~2 天）业务接口收口

1. sessions/chat 等核心接口先强制使用 token user_id
2. 其余配置类接口按模块迁移
3. 去掉 `default-user` 相关兜底

## 8. 测试与验收

### 8.1 后端

1. 单测：密码哈希/校验、JWT 签发/解析/过期
2. 接口测试：register/login/me 全链路
3. 权限测试：A 用户不可访问 B 用户会话

### 8.2 前端

1. 注册 -> 自动登录 -> 进入聊天
2. 刷新页面后仍可携带 token 请求
3. token 失效后回到登录页

### 8.3 回归

1. 会话收发消息正常
2. 终端、项目、模型配置读写正常
3. v2/v3 流式聊天不受影响

## 9. 风险与注意事项

1. 接口量较多，建议先覆盖“聊天主链路”，再铺开到配置模块。
2. 如果立即强制鉴权，现有脚本/IPC 可能受影响，需同步改调用方。
3. `AUTH_JWT_SECRET` 必须在生产环境配置，且长度足够（建议 32+ 字节）。

## 10. 本方案的默认取舍（可改）

1. V1 先不做 refresh token，只做 access token（实现快）
2. V1 先做邮箱密码，不做短信/第三方登录
3. 先支持 Web 端，IPC/Electron 跟随同一 token 机制

## 11. 按当前代码调整（2026-03-02）

考虑到你已确认“移除 IPC，仅保留 HTTP/SSE”，方案补充调整：

1. 前端认证仅走 `ApiClient` 的 HTTP 请求，不再有 IPC fallback。
2. 后端统一在 API 路由层做 token 鉴权拦截（`/api/auth/*` 除外）。
3. 关键资源接口（sessions/projects/terminals/applications/agents/system-contexts/模型配置）增加登录用户一致性校验，拒绝跨用户 `user_id` 伪造。
4. `App` 增加登录门禁：未登录显示登录/注册面板，登录后才加载聊天主界面。

如果你确认这个方案，我就按 Phase 1 -> Phase 2 顺序直接开始落地代码（已开始实现）。

## 12. 当前落地结果（2026-03-02）

已按上面的调整方案完成主链路：

1. 后端已提供 `/api/auth/register`、`/api/auth/login`、`/api/auth/me`。
2. 后端 API 路由已区分：
   - `/api/auth/*` 公开；
   - 其余 `/api/*` 统一要求 Bearer Token。
3. 核心资源接口已做登录用户隔离校验（sessions/messages/projects/terminals/chat/config/agents/applications 等）。
4. task-manager 已增加会话归属校验（包含 review decision 提交前检查）。
5. 前端已接入登录/注册门禁（未登录显示认证面板，登录后进入聊天）。
6. 前端 API Client 已统一注入 `Authorization`，并去掉 IPC fallback。
7. 前端 `default-user/custom_user_123` 硬编码回退已移除，改为使用登录态 userId。
