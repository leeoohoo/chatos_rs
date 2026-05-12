# Memory 平台进度推进方案

## 1. 目标形态

我们现在的目标不是继续给 `memory_server` 打补丁，而是把整个记忆能力收口成下面三层：

1. `memory_engine`
   - 作为统一记忆平台与唯一事实源
   - 承接线程、消息、总结、rollup、subject memory、review_repair、snapshot、上下文构建
   - 提供 SDK / API / 控制面 / 前端管理能力

2. `chat_app_server_rs`
   - 承接 `chatos` 的业务侧能力
   - 负责智能体、技能、联系人、项目、运行时、会话映射适配
   - 通过本地服务或 SDK 接入 `memory_engine`

3. `memory_server`
   - 最终目标是彻底消失
   - 从当前阶段开始，不再继续给它新增任何代码、转发层、字段改写或兼容包装
   - 后续迁移动作只发生在 `chat_app_server_rs` / `memory_engine` / SDK

## 2. 当前已完成

### 2.1 `memory_engine`

- 已具备平台主干能力：
  - 线程 / 记录 / 总结 / subject memory / snapshot
  - context compose
  - review_repair / rollup / worker
  - source 管理、控制面 API、前端页面
- 已开始向真正的“多系统接入平台”收口：
  - source 现在支持全局接入系统定义，不再强依赖单一 tenant
  - source secret 轮换接口已支持全局 source
- 已提供 SDK 接口：
  - `/api/memory-engine/v1/sdk/...`
- 已有 Rust SDK：
  - `/Users/lilei/project/my_project/chatos_rs/memory_engine/sdk_rust`

### 2.2 `chat_app_server_rs`

- 已把核心记忆读写大量直连到 `memory_engine`
  - session sync
  - message CRUD
  - summaries
  - context compose
  - review_repair
  - snapshots
  - subject memories
- 已把 `chatos -> memory_engine` 的主接入身份从 `memory_server` 影子迁到 `chatos`
- 已为后续正式 SDK 接入补上 system-key 配置入口：
  - `MEMORY_ENGINE_SYSTEM_ID`
  - `MEMORY_ENGINE_SYSTEM_KEY`
- 已在启动流程增加 `chatos` source 自注册
  - 启动时会自动向 `memory_engine` upsert 全局 source：`source_id = chatos`
  - 注册内容包含 source type、能力声明、mapping version、SDK enabled
  - 已增加 system id 一致性校验，避免 `chatos` 误写到别的 source 身份下
- 已开始统一兼容层写入身份
  - `memory_server -> memory_engine` 的主数据访问已对齐到 `source_id = chatos`
  - `memory_server` 的 job 观察接口已改为读取 `chatos` source 的运行数据
  - `memory_engine` 内部的 subject memory worker 已去掉 `memory_server` 特判，统一走通用 `scope_worker`
  - `memory_server -> memory_engine` 也已移除 direct fallback，`MEMORY_ENGINE_SYSTEM_ID + MEMORY_ENGINE_SYSTEM_KEY` 现在同样是必需
- 已把智能体 / 技能主流程拉回本地：
  - agents repository / service
  - skills repository / service
- 已把 agent / skill DTO 主定义迁回本地模型：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/models/chatos_agent_types.rs`
- 已补齐 agent / skill 兼容迁移的访问语义：
  - agent list 支持 `include_shared`
  - admin 可带 `user_id` 做 agent / skill scope delegation
  - 单 agent 资源读写校验已对齐旧兼容接口
- 已移除 `chatos -> memory_engine` 的 direct fallback：
  - `MEMORY_ENGINE_SYSTEM_ID` 现在是必填
  - `MEMORY_ENGINE_SYSTEM_KEY` 现在是必填
  - 缺失时显式报错，不再静默退回 `new_direct(...)`
- 已开始在 `chat_app_server_rs` 直接承接原 `memory_server` 的 memory 兼容接口面：
  - 新增 `/api/memory/v1/...` 路由入口，直接落到 `chat_app_server_rs`
  - 当前已覆盖：
    - sessions CRUD / sync
    - message CRUD / batch / clear
    - summaries list / delete
    - turn runtime snapshot sync / lookup
    - context compose
  - 这些能力直接走 `chatos_sessions` / `chatos_memory_engine`，不再依赖 `memory_server`
- 已把 access-token scope 从 `memory_server_client` 里拆成 `access_token_scope`
  - chat stream / review repair / session title rename / auth middleware 继承 access token 时，已经不再语义上依附 `memory_server`
  - 主链路里一批 `memory_server` 残留日志与登录页文案也已同步清理
- 已把受保护接口的 Bearer token 校验切到 `chat_app_server_rs` 本地兼容解析
  - 主服务不再运行时依赖 `memory_server /auth/me`
  - `AUTH_JWT_SECRET` 现在会兼容回退读取 `MEMORY_SERVER_AUTH_SECRET`
- 已把 `auth/login` 切到 `chat_app_server_rs` 本地接管
  - 登录不再通过 `memory_server` HTTP 转发
  - 当前会优先读取 legacy `memory_server` 的 `auth_users` 数据，再同步落回 `chat_app_server_rs` 本地 `auth_users`
  - `chat_app_server_rs` 已补齐本地 `auth_users` repository / sqlite schema / mongodb collection

### 2.3 `memory_server`

- 总结、rollup、review_repair、模型配置、任务运行等主数据面已大量转到 `memory_engine`
- worker / 配置 / 管理页已经开始收缩，说明方向已经转正
- 智能体 / 技能接口已退成兼容代理壳：
  - `/api/memory/v1/agents`
  - `/api/memory/v1/agents/:agent_id`
  - `/api/memory/v1/agents/:agent_id/runtime-context`
  - `/api/memory/v1/agents/:agent_id/sessions`
  - `/api/memory/v1/agents/ai-create`
  - `/api/memory/v1/skills`
  - `/api/memory/v1/skills/:skill_id`
  - `/api/memory/v1/skills/plugins`
  - `/api/memory/v1/skills/plugins/detail`
  - `/api/memory/v1/skills/import-git`
  - `/api/memory/v1/skills/plugins/install`
- 上述接口当前统一代理到 `chat_app_server_rs`，并保留旧前端依赖的 `{"items": [...]}` 包装
- `x-service-token` 内部请求已在 compat client 中转换为 admin bearer，再转发到 `chat_app_server_rs`
- 联系人旧本地实现已从模块树摘除：
  - `contacts_crud_api`
  - `contacts_context_api/*`
- 项目映射兼容接口也已退成代理壳：
  - `/api/memory/v1/projects`
  - `/api/memory/v1/projects/sync`
  - `/api/memory/v1/projects/:project_id/contacts`
  - `/api/memory/v1/project-agent-links/sync`
- 为了承接上述兼容路由，`chat_app_server_rs` 已新增 memory-mapping owner 接口：
  - `/api/memory/projects`
  - `/api/memory/projects/sync`
  - `/api/memory/projects/:project_id/contacts`
  - `/api/memory/project-agent-links/sync`
- `memory_server` 已进一步摘除失去主链路 owner 身份的本地 agent / skill 模块树：
  - `repositories/agents*`
  - `repositories/skills.rs`
  - `services/skills/*`
  - `db/normalize.rs`
- `memory_server` 的 mongodb schema 已不再初始化本地 agent / skill / 旧 job / 旧 config 索引
- `memory_server` 的 job 观察页标签补全已改为通过 compat client 向 `chat_app_server_rs` 拉取 contacts / memory-projects 数据，不再直接读取本地 contacts / memory_projects 旧表
- 新边界已明确：
  - `memory_server` 后续不再新增任何代码
  - 不再继续给 `memory_server` 增加转发、兼容包装、字段改写
  - 后续所有 memory 能力收口动作只在 `chat_app_server_rs` / `memory_engine` 侧完成

## 3. 还没做完的部分

### 3.1 `chat_app_server_rs` 还残留的老依赖

目前 `chat_app_server_rs` 残留的老依赖已经明显缩小，当前主要包括：

1. 认证
   - legacy `auth_users` 数据仍在从 `memory_server` 的 mongodb 库读取
   - 还没有把用户管理入口与用户数据彻底并回 `chat_app_server_rs`

2. 命名层面仍有一部分“memory_server 时代”遗留
   - `MEMORY_SERVER_*` 配置项
   - 少量日志、注释、部署文案

3. `chatos` 还没有完全切到 memory_engine 的 source secret / system-key 正式接入
4. `chatos` source 虽然已经能自动注册，但 source secret 轮换与部署配置闭环还没有完成
5. `memory_server` 里仍有部分 backfill / 配置 / 文案 / 兼容查询残留 `memory_server` 旧 source 命名，还需要继续清空
6. `chat_app_server_rs` 虽然已经新增 `/api/memory/v1/...` 兼容入口，但调用方和部署流量还没有全部切走；仍需把原来打到 `memory_server` 的 session / message / context / snapshot 流量迁过来
7. `memory_server` 的 contacts / projects / project-agent-links 本地 repository 与旧 backfill 脚本仍在仓库里，需要继续清理

### 3.2 `memory_server` 还残留的平台职责

虽然已经瘦了不少，但它仍然还保留着一些不该长期留在那里的东西：

1. 旧配置 API
   - models
   - job configs

2. 旧管理前端
   - dashboard / user config / job run 等页面仍有残留

3. 兼容层边界不够清晰
   - 还混着平台控制面、兼容层和 `chatos` 业务语义

### 3.3 最终架构还差的关键一步

还没有彻底完成的是：

1. 让 `chatos` 用 memory_engine 的正式 source secret / SDK 方式接入
2. 把 legacy `auth_users` 数据与用户管理入口彻底并回 `chat_app_server_rs`
3. 继续把 `memory_server` 的控制面与前端职责彻底迁空
4. 明确 `memory_server` 最终仅保留哪几条映射兼容职责，再继续并回 `chat_app_server_rs`

## 4. 本轮后的建议执行顺序

### 阶段 A：先完成 `chatos` 到 `memory_engine` 的正式接入

1. 在 `memory_engine` 平台里创建并管理 `chatos` source
2. 让 `chat_app_server_rs` 优先走 `MEMORY_ENGINE_SYSTEM_ID + MEMORY_ENGINE_SYSTEM_KEY`
3. 逐步退出 `new_direct(..., source_id)` 这种过渡接入方式

这一步的目标是：

- `chatos` 成为平台正式接入方，而不是继续复用 `memory_server` 的旧身份影子
- 后续其他子系统也可以按同一接入模式进入平台

当前这一阶段已完成一半：

1. `chat_app_server_rs` 启动时已自动确保 `chatos` source 存在
2. 已经把 direct 模式退出主路径
3. `memory_server` compat 层也已经退出 direct 模式
4. 还差 source secret 轮换与部署侧 `MEMORY_ENGINE_SYSTEM_KEY` 闭环

### 阶段 B：继续收口 `chat_app_server_rs` 本地边界

1. 继续清理 `MEMORY_SERVER_*` 旧命名与旧配置
2. 收口 legacy `auth_users` 的读取边界
3. 让 `chatos_memory_engine` / `chatos_memory_mappings` 成为唯一业务接入层

### 阶段 C：停止给 `memory_server` 加代码，并迁空调用方

1. 把原来依赖 `memory_server` 的会话 / 消息 / 总结 / context / snapshot 调用，切到 `chat_app_server_rs` 新增的 `/api/memory/v1/...`
2. 删掉或冻结 `memory_server` 剩余旧模型配置 / job 配置兼容接口
3. 收掉旧前端里已经转移到 `memory_engine` 的平台页
4. 等调用方迁空后，再整体移除 `memory_server`

### 阶段 D：最终并回 `chatos`

1. 把剩余映射适配完全移入 `chat_app_server_rs`
2. 让 `memory_server` 退出主链路
3. `memory_engine` 作为平台，`chatos` 作为接入方

## 6. 本轮完成后的下一批动作

接下来建议继续按这个顺序做：

1. 在 `memory_engine` 控制面里创建 `chatos` source 并完成 secret key 接入闭环
2. 增加 `chatos` source secret 的平台侧轮换与部署配置说明，让 system-key 可以真正启用
3. 把 `chat_app_server_rs` 的 memory 访问切到 system-key SDK 主路径，并收掉 direct 模式作为长期运行路径
4. 把调用方切到 `chat_app_server_rs` 新增的 `/api/memory/v1/...` 兼容接口
5. 继续清理 `memory_server` 里剩余的 `memory_server` source 命名、backfill 脚本、兼容观察文案
6. 迁空后整体下线 `memory_server`
