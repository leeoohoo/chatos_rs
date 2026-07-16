# Chat OS 统一配置中心微服务实施方案

> 文档日期：2026-07-15
>
> 适用范围：Chat OS、Task Runner、User Service、Project Management Service、Plugin Management Service、Memory Engine、Local Connector Service、Sandbox Manager、Official Website 及共享 Rust 运行时
>
> 技术要求：后端 Rust，前端 React + TypeScript + Ant Design
>
> 当前状态：P0/P1 可运行 MVP 已完成，P2/P3 增强项按需继续

## 0. 方案结论

建议新增独立的 Configuration Center 微服务，目录为：

- config_center_service/backend
- config_center_service/frontend
- crates/chatos_config_sdk

该服务作为配置控制面，统一负责：

- 平台全局配置；
- 各微服务运行参数；
- 开发参数和功能开关；
- 配置 Schema、类型、默认值和校验规则；
- 草稿、发布、版本、回滚和审计；
- 服务实例当前生效版本和重启状态；
- 向 Consul KV 发布兼容快照；
- 向各 Rust 服务提供 typed snapshot、长轮询或 SSE 更新。

MongoDB 保存权威配置版本，Consul 继续承担已发布配置快照的分发和启动容灾，不把 Consul UI 直接作为配置管理后台。

目标配置优先级为：

1. 代码内置默认值；
2. 配置中心环境级共享配置；
3. 配置中心服务级配置；
4. 受控的紧急运维覆盖。

最终不再允许普通用户覆盖平台运行参数。Chat OS 当前的用户运行参数面板将被移除，Task Runner 当前保存在 runtime_settings 集合中的系统参数也迁移到配置中心。

以下内容不进入普通配置值：

- 数据库连接地址、配置中心自身地址等启动引导参数；
- JWT 密钥、内部 API Secret、模型 API Key、SMTP 密码等明文凭证；
- 用户模型、项目、会话、设备、工作区等业务资源。

敏感值只保存 secret reference，实际值继续由 Docker Secret、部署环境或后续接入的 Secret Manager 提供。

## 1. 建设目标

### 1.1 用户体验目标

- 普通用户不再看到最大迭代次数、最大输出 Tokens、附件总大小、日志级别等平台级参数。
- 界面语言和内部上下文语言保留为用户偏好，每个账号可以独立选择。
- 普通用户不能通过 Chat OS API 修改这些参数。
- 平台管理员在配置中心统一修改一次，所有相关服务按照发布策略生效。
- 管理员能够看到配置影响哪些服务、是否支持热更新、哪些实例仍需重启。
- 修改错误时可以快速回滚到上一已发布版本。

### 1.2 工程目标

- 消除环境变量、数据库 runtime_settings、用户 settings 和代码默认值相互覆盖造成的配置漂移。
- 防止本次 Task Runner 中“旧版 25 被持久化后长期覆盖新版 600”的问题再次发生。
- 所有受管配置必须有稳定 key、类型、默认值、校验、所有者和生效方式。
- 服务读取配置时使用 typed API，不在业务代码中到处读取 JSON 或任意环境变量。
- 发布版本不可变，可审计、可比较、可回滚。
- 配置中心暂时不可用时，服务可以使用最近一次已发布且校验通过的快照。

### 1.3 非目标

首期不把以下领域数据搬进配置中心：

- 用户模型配置、模型 API Key 和 Base URL；
- MCP、Skill、Agent、插件资源；
- 项目运行环境和项目级沙箱选择；
- 会话选择的模型、思考等级、远程连接、工作区和 Plan Mode；
- Local Connector 本机路径、设备授权和本机私有状态；
- 用户账号资料、角色和登录凭证；
- 任务、需求、记忆、消息和运行记录。

这些属于业务数据或用户选择，不应被当成平台配置。

## 2. 当前实现现状

### 2.1 已有 Consul 配置读取能力

crates/chatos_service_runtime 已经具备：

- 服务注册和发现；
- 从 chatos/{env}/shared/config 读取共享配置；
- 从 chatos/{env}/services/{service}/config 读取服务配置；
- 将 JSON 中的 env 字段转换成环境变量；
- 本地环境变量已存在时不覆盖；
- 配置读取失败时继续使用本地环境。

Chat OS、Task Runner、Project Service、Plugin Management、Local Connector Service、Sandbox Manager、User Service、Memory Engine 和 Official Website 启动时已经调用 apply_config_center_env。

这部分可以继续作为兼容层，但目前存在以下不足：

- 没有独立管理服务和管理 UI；
- 没有配置定义和类型校验；
- 没有草稿、发布、版本和回滚；
- 没有审计日志；
- 只在进程启动时读取；
- 不支持运行时热更新；
- 环境变量优先级高于中心配置；
- 不知道服务实例实际生效了哪个版本；
- 任意大写 key 都可能进入环境变量；
- 没有敏感字段分类；
- 没有配置变更影响分析。

### 2.2 Chat OS 用户运行参数

Chat OS 当前通过 user_settings 保存以下用户级覆盖：

| 当前 Key | 当前用途 | 目标归属 |
| --- | --- | --- |
| MAX_ITERATIONS | Agent 工具调用迭代上限（旧兼容名） | agent.runtime.max_iterations |
| TASK_FOLLOW_UP_MAX_ROUNDS | 任务后置检查轮数 | chatos.task.follow_up_max_rounds |
| LOG_LEVEL | 日志级别 | shared.logging.level 或 chatos.logging.level |
| HISTORY_LIMIT | 历史消息读取数量 | chatos.conversation.history_limit |
| CHAT_MAX_TOKENS | 单次最大输出 Tokens | chatos.ai.max_output_tokens |
| ATTACHMENT_TOTAL_MAX_BYTES | 一次消息附件总大小 | chatos.attachment.total_max_bytes |
| INTERNAL_CONTEXT_LOCALE | 内部系统上下文语言 | 保留为用户偏好 |
| UI_LOCALE | Chat OS 界面语言 | 保留为用户偏好 |
| TERMINAL_UI_ENABLED | 是否展示终端入口 | chatos.ui.terminal_enabled |

当前问题：

- 平台运行参数被错误地定义成用户偏好；
- 不同用户可能使用不同的安全上限；
- LOG_LEVEL 实际不是用户级概念；
- 用户表中保存的历史值会长期覆盖代码默认值；
- 前端依赖 /api/user-settings 获取 UI 和附件限制，形成额外耦合。

### 2.3 Task Runner 系统运行参数

Task Runner 当前 runtime_settings 集合包含：

- task_execution_max_iterations；
- execution_timeout_ms；
- tool_result_model_max_chars；
- tool_results_model_total_max_chars；
- execution_environment_mode；
- sandbox_enabled；
- sandbox_manager_base_url；
- sandbox_lease_ttl_seconds。

数据库记录优先于环境默认值，代码升级不会自动迁移旧值。本次发现的 25 覆盖 600 就是这一机制导致。

目标映射：

| 当前字段 | 配置中心 Key |
| --- | --- |
| task_execution_max_iterations | agent.runtime.max_iterations |
| execution_timeout_ms | task_runner.execution.timeout_ms |
| tool_result_model_max_chars | task_runner.ai.tool_result_max_chars |
| tool_results_model_total_max_chars | task_runner.ai.tool_results_total_max_chars |
| execution_environment_mode | task_runner.execution.environment_mode |
| sandbox_enabled | task_runner.sandbox.enabled |
| sandbox_manager_base_url | task_runner.sandbox.manager_base_url |
| sandbox_lease_ttl_seconds | task_runner.sandbox.lease_ttl_seconds |

### 2.4 各服务环境变量规模

当前主要服务在配置模块中直接读取的环境变量数量约为：

| 服务 | 直接读取数量 | 典型内容 |
| --- | ---: | --- |
| Chat OS | 54 | AI、摘要、日志、附件、下游超时、认证 |
| Task Runner | 72 | Worker、Run、沙箱、Memory、下游服务、认证 |
| Project Service | 66 | 下游服务、Git、上传限制、沙箱、MongoDB |
| User Service | 45 | JWT、账号、限流、SMTP、Harness、下游服务 |
| Sandbox Manager | 36 | 后端类型、Docker、Kata、租约、认证 |
| Local Connector Service | 27 | Relay、签名、租约、Memory、认证 |
| Plugin Management | 19 | MongoDB、下游服务、工具快照限制 |
| Memory Engine | 18 | MongoDB、模型运行时、下游服务 |
| Official Website | 5 | URL、静态资源、发布上传 |

不能一次把所有变量不加区分地搬入配置中心。必须先分类为：

- bootstrap；
- secret 或 secret_ref；
- restart_required；
- hot_reload；
- next_request；
- domain_data；
- deprecated。

## 3. 总体架构

### 3.1 组件职责

#### Configuration Center Backend

- 保存配置定义、草稿、发布版本、快照和审计；
- 校验类型、范围、枚举和跨字段约束；
- 构建环境共享快照和服务快照；
- 发布兼容 JSON 到 Consul KV；
- 提供管理 API 和内部读取 API；
- 接收服务实例心跳和应用结果；
- 维护发布状态并执行失败重试；
- 支持回滚和配置差异比较。

#### Configuration Center Frontend

- 只允许 User Service 中的 super_admin 访问；
- 提供配置目录、编辑、校验、发布、回滚、审计和实例状态；
- 使用 React、TypeScript、Vite、Ant Design、React Query 和 React Router；
- 根据 Schema 自动选择 InputNumber、Switch、Select、Input、TextArea 等控件。

#### chatos_config_sdk

- 定义配置 DTO、类型转换、快照、revision 和错误类型；
- 支持启动拉取、ETag、长轮询或 SSE；
- 支持最近一次有效快照缓存；
- 支持按服务获取 typed config；
- 提供 revision、source、stale 状态和校验结果；
- 支持服务上报当前应用版本。

#### chatos_service_runtime

- 继续处理 Consul 服务注册和发现；
- 兼容现有 Consul KV env 快照；
- 增加配置中心服务发现和 SDK 初始化 helper；
- 最终不再承担业务配置解析，仅保留启动兼容和基础设施能力。

#### Consul

- 保存配置中心发布的只读兼容快照；
- 作为服务启动时的分发和容灾来源；
- 不再允许人工直接维护业务配置；
- 不是权威历史数据库。

#### MongoDB

- 保存配置中心权威数据；
- 保存不可变 release 和 snapshot；
- 保存当前 active pointer；
- 保存审计和实例状态。

### 3.2 数据流

发布流程：

1. 管理员在前端修改草稿。
2. 后端验证 Schema 和跨字段规则。
3. 后端生成不可变 release。
4. 后端生成 shared 和 service snapshots。
5. 后端写入 Consul 兼容 KV。
6. 后端原子切换 active release pointer。
7. SDK 通过长轮询或 SSE 得知 revision。
8. 支持热更新的服务原子替换配置。
9. 需要重启的实例上报 pending_restart。

读取流程：

1. 服务从环境变量读取 bootstrap 配置。
2. SDK 通过服务发现访问 configuration-center。
3. 请求当前服务的 effective snapshot。
4. 若请求失败，尝试 Consul 已发布快照。
5. 若仍失败，读取本地 last-known-good。
6. 只有非关键配置才能最后回退代码默认值。

## 4. 配置分类和边界

### 4.1 Bootstrap 配置

以下参数必须保留在部署环境，因为服务需要先依靠它们找到配置中心：

- CHATOS_ENV；
- CHATOS_CONSUL_HTTP_ADDR；
- CHATOS_SERVICE_NAME、ID、ADDRESS、PORT、HEALTH_PATH；
- CONFIG_CENTER_BASE_URL 的静态 fallback；
- CONFIG_CENTER_INTERNAL_API_SECRET 或服务身份凭证；
- 当前服务 DATABASE_URL；
- 配置中心自身 DATABASE_URL；
- 配置中心用于加密或签名的根密钥；
- Docker 网络和容器启动必须参数。

Bootstrap 配置不在普通管理 UI 中编辑。

### 4.2 Secret 和 Secret Reference

以下值不能作为普通配置明文保存：

- JWT secret；
- 内部 API secret；
- MongoDB 密码；
- SMTP 密码；
- 模型 API Key；
- Sandbox system client key；
- 发布上传 token；
- Memory operator token；
- Harness PAT。

配置中心只保存：

- secret_ref 类型；
- provider，例如 docker_secret、env、vault；
- reference name；
- 是否已解析；
- 最后验证时间；
- 不可逆掩码。

MVP 不在浏览器中提供读取秘密原文的能力。

### 4.3 Restart Required

典型配置：

- 监听 host 和 port；
- 数据库地址；
- Worker 角色和进程并发模型；
- CORS 中间件初始化参数；
- Sandbox backend 类型；
- Docker network；
- tracing subscriber 尚未接入 reload handle 的日志配置；
- 服务内部线程池和运行时参数。

发布时 UI 必须标记需要重启，并展示未应用实例。

### 4.4 Hot Reload

适合原子热更新的配置：

- 最大迭代次数；
- 最大输出 Tokens；
- 工具结果字符预算；
- 附件大小限制；
- 任务后置检查轮数；
- History limit；
- Feature flags；
- API 请求超时；
- 轮询间隔；
- Sandbox lease TTL；
- 登录限流阈值；
- 摘要策略；
- UI locale 和菜单开关。

### 4.5 Next Request 或 Next Run

部分配置不修改正在执行的请求，只对新请求生效：

- Chat OS AI 迭代上限；
- Task Runner Run 迭代上限；
- Task Runner execution timeout；
- AI tool result budget；
- Memory summary policy；
- 项目环境初始化策略。

这类配置发布后不需要重启，但不能修改正在运行任务的上限。

### 4.6 Domain Data

下列内容继续由原领域服务管理：

- 会话模型选择；
- 项目沙箱配置；
- 用户模型和插件；
- 用户设备和工作区；
- 任务和需求参数；
- 每个项目或任务的显式覆盖。

如果未来需要管理员策略，可以由配置中心提供“最大允许值”或 Feature Gate，但不能把领域记录本身搬进配置中心。

## 5. 配置 Key 和 Schema 设计

### 5.1 Key 命名

使用小写点分层，不再把环境变量名作为业务 Key：

- shared.logging.level
- agent.runtime.max_iterations
- chatos.ai.max_output_tokens
- chatos.attachment.total_max_bytes
- task_runner.worker.concurrency
- task_runner.ai.tool_result_max_chars
- user_service.login.max_failed_attempts
- project_service.cloud_project.max_unpacked_bytes

环境变量名只作为兼容映射，例如：

- agent.runtime.max_iterations -> AGENT_MAX_ITERATIONS
- agent.runtime.max_iterations -> MAX_ITERATIONS（Chat OS 旧兼容名）
- agent.runtime.max_iterations -> TASK_RUNNER_MAX_MODEL_REQUEST_ROUNDS（Task Runner 旧兼容名）

### 5.2 Definition 字段

每个配置定义至少包含：

- key；
- display_name；
- description；
- category；
- scope：shared 或 service；
- service_name；
- value_type；
- default_value；
- nullable；
- validation；
- enum_options；
- sensitivity；
- reload_mode；
- criticality；
- env_aliases；
- owner_team；
- introduced_in；
- deprecated；
- replacement_key；
- ui_order；
- tags。

支持的 value_type：

- string；
- integer；
- float；
- boolean；
- duration_ms；
- bytes；
- enum；
- string_list；
- json；
- secret_ref。

### 5.3 校验

单字段校验：

- min 和 max；
- min_length 和 max_length；
- regex；
- enum；
- URL；
- duration；
- byte size；
- JSON Schema。

跨字段校验示例：

- tool_results_total_max_chars 必须大于等于 tool_result_max_chars；
- worker_claim_ttl_ms 必须大于 worker_poll_interval_ms；
- sandbox lease TTL 不得小于任务请求超时；
- summary_keep_last_n 必须小于 summary_message_limit；
- UI locale 和 internal context locale 必须属于支持语言。

### 5.4 配置定义来源

建议把官方定义作为代码资产提交：

- config_center_service/catalog/shared.json
- config_center_service/catalog/chatos-backend.json
- config_center_service/catalog/task-runner.json
- config_center_service/catalog/user-service.json
- config_center_service/catalog/project-service.json
- config_center_service/catalog/plugin-management-service.json
- config_center_service/catalog/memory-engine.json
- config_center_service/catalog/local-connector-service.json
- config_center_service/catalog/sandbox-manager.json
- config_center_service/catalog/official-website.json

服务升级时由配置中心同步 catalog 版本。生产环境不允许无 Schema 的任意 Key 自动注入服务。

管理员可以新增 developer.* 或指定服务命名空间下的自定义开发参数，但必须同时定义类型、默认值、owner 和 reload mode。

## 6. 配置作用域和优先级

### 6.1 作用域

首期只提供：

- environment；
- shared；
- service。

environment 用于区分 development、test、staging、production，不是用户或租户覆盖。

不提供：

- user scope；
- tenant scope；
- session scope。

### 6.2 最终优先级

从低到高：

1. 服务代码默认值；
2. 当前 environment 的 shared 已发布配置；
3. 当前 environment 的 service 已发布配置；
4. 紧急运维覆盖 allowlist。

紧急覆盖必须满足：

- 显式开启 CHATOS_CONFIG_EMERGENCY_OVERRIDE_ENABLED；
- 只允许 Schema 中 emergency_override_allowed=true 的 Key；
- 服务状态接口上报 override key；
- 日志持续告警；
- 配置中心实例页明确显示“本地覆盖中心配置”；
- 不允许普通用户触发。

### 6.3 过渡期优先级

为了兼容现有部署，第一阶段保持：

1. 代码默认；
2. 配置中心；
3. 已存在的 legacy 环境变量；
4. legacy 数据库 runtime settings。

但服务必须上报 shadowed 状态，显示哪些中心配置被旧来源覆盖。

第二阶段移除数据库覆盖。

第三阶段生产环境只保留 bootstrap 和紧急 allowlist，中心配置成为唯一运行参数来源。

## 7. 后端目录和模块

建议目录：

    config_center_service/
      backend/
        Cargo.toml
        src/
          main.rs
          lib.rs
          config.rs
          auth.rs
          api/
            mod.rs
            health.rs
            catalog.rs
            drafts.rs
            releases.rs
            snapshots.rs
            audit.rs
            instances.rs
            internal.rs
          models/
            definition.rs
            draft.rs
            release.rs
            snapshot.rs
            audit.rs
            instance.rs
          services/
            catalog_service.rs
            resolution_service.rs
            validation_service.rs
            release_service.rs
            consul_publisher.rs
            reconciliation_worker.rs
            instance_service.rs
          store/
            mod.rs
            indexes.rs
          error.rs
      frontend/
        package.json
        src/
          App.tsx
          main.tsx
          api/
          components/
          pages/
            DashboardPage.tsx
            ConfigCatalogPage.tsx
            ConfigEditorPage.tsx
            ReleaseHistoryPage.tsx
            AuditLogPage.tsx
            ServiceInstancesPage.tsx
          types/
          i18n/
          styles.css
      catalog/

共享 SDK：

    crates/chatos_config_sdk/
      Cargo.toml
      src/
        lib.rs
        client.rs
        snapshot.rs
        value.rs
        cache.rs
        watcher.rs
        status.rs
        error.rs

## 8. MongoDB 数据模型

### 8.1 config_definitions

保存 Schema 定义。

主要字段：

- id；
- key；
- scope；
- service_name；
- type；
- default_value；
- validation；
- sensitivity；
- reload_mode；
- env_aliases；
- catalog_version；
- created_at；
- updated_at。

索引：

- key 唯一；
- service_name + category；
- deprecated。

### 8.2 config_drafts

保存管理员尚未发布的修改。

主要字段：

- id；
- environment；
- base_release_id；
- changes；
- validation_status；
- validation_errors；
- created_by；
- updated_by；
- created_at；
- updated_at。

同一 environment 默认只允许一个 active draft，避免多人无提示覆盖。

### 8.3 config_releases

保存不可变发布记录。

主要字段：

- id；
- environment；
- revision；
- status：building、published、failed、superseded；
- base_release_id；
- changed_keys；
- checksums；
- publish_message；
- created_by；
- created_at；
- published_at。

revision 在 environment 内单调递增。

### 8.4 config_snapshots

保存每个发布版本编译后的服务快照。

主要字段：

- id；
- release_id；
- environment；
- service_name；
- revision；
- values；
- value_sources；
- reload_modes；
- checksum；
- created_at。

唯一索引：

- environment + service_name + revision。

### 8.5 config_active_releases

每个 environment 一条 active pointer：

- environment；
- release_id；
- revision；
- updated_at。

通过 revision 条件执行 compare-and-set，避免并发发布覆盖。

### 8.6 config_audit_events

记录：

- draft 创建和修改；
- 校验；
- 发布；
- 回滚；
- catalog 同步；
- 登录和权限拒绝；
- Consul 发布失败；
- 实例应用失败；
- 紧急覆盖上报。

审计中不记录 secret 原文。

### 8.7 config_service_instances

记录：

- service_name；
- service_id；
- environment；
- address；
- running_version；
- effective_revision；
- effective_checksum；
- stale；
- pending_restart；
- emergency_overrides；
- last_error；
- last_seen_at。

## 9. 发布一致性和回滚

MongoDB 在当前部署中不保证一定运行在 Replica Set，不能把多文档事务作为唯一正确性基础。

发布采用不可变记录 + 原子 pointer：

1. 校验 draft 和 base revision；
2. 创建 building release；
3. 生成并保存所有 snapshot；
4. 验证 checksum；
5. 发布 Consul shared/service KV；
6. compare-and-set 切换 active release；
7. 将 release 标记 published；
8. 通知 watcher。

任何步骤失败：

- active pointer 保持旧版本；
- 服务继续使用旧 snapshot；
- release 标记 failed；
- reconciliation worker 可安全重试；
- UI 展示失败阶段。

回滚不是修改旧 release，而是：

1. 选择历史 release；
2. 基于其 values 创建新的 release；
3. 重新发布 Consul 快照；
4. 切换 active pointer；
5. 保留完整审计链。

## 10. Consul 兼容方案

配置中心继续发布现有 Key：

- chatos/{env}/shared/config
- chatos/{env}/services/{service}/config

内容保持：

    {
      "revision": 12,
      "checksum": "...",
      "env": {
        "AGENT_MAX_ITERATIONS": 600
      }
    }

chatos_service_runtime 当前只读取 env，额外 metadata 不影响兼容。

新增内部 Key：

- chatos/{env}/config-center/active-release
- chatos/{env}/config-center/services/{service}/snapshot

规则：

- 只有 Configuration Center Backend 写这些 Key；
- 管理后台不直接把用户输入写 Consul；
- MongoDB release 是权威来源；
- reconciliation worker 定期校验 MongoDB active revision 和 Consul revision；
- Consul 丢失后可以由 active release 全量重建。

## 11. Rust SDK 设计

### 11.1 启动接口

建议用法：

    let client = ConfigClient::builder("task-runner")
        .environment(env)
        .bootstrap_from_env()
        .build()
        .await?;

    let snapshot = client.load_initial().await?;
    let config = TaskRunnerManagedConfig::from_snapshot(&snapshot)?;

### 11.2 Snapshot

包含：

- environment；
- service_name；
- revision；
- checksum；
- values；
- sources；
- generated_at；
- stale；
- source_kind：config_center、consul、local_cache、defaults；
- pending_restart_keys。

### 11.3 缓存

每个服务保存：

- /data/config-cache/{service}/{environment}.json；
- 最近一次校验通过的 snapshot；
- checksum；
- 写入使用临时文件 + rename；
- 文件权限不允许普通用户读取 secret reference 解析结果。

### 11.4 Watch

首期使用带 revision 的长轮询，避免复杂消息中间件：

- GET internal snapshot，带 If-None-Match；
- GET watch?after_revision=N&timeout=30；
- 服务收到新 revision 后拉取完整 snapshot；
- 校验通过后原子替换 ArcSwap 或 watch channel；
- 校验失败继续使用旧版本并上报错误。

后续可以切换 SSE，不影响 Snapshot DTO。

### 11.5 Typed Config

每个服务定义受管配置结构，例如：

    struct TaskRunnerManagedConfig {
        execution_max_iterations: usize,
        execution_timeout: Duration,
        tool_result_max_chars: usize,
        tool_results_total_max_chars: usize,
        sandbox_lease_ttl: Duration,
    }

业务代码只读取该结构，不直接通过字符串 Key 访问。

## 12. 配置中心 API

### 12.1 管理 API

- GET /api/config/v1/environments
- GET /api/config/v1/catalog
- GET /api/config/v1/catalog/{key}
- POST /api/config/v1/catalog/custom
- GET /api/config/v1/environments/{env}/effective
- GET /api/config/v1/environments/{env}/draft
- PUT /api/config/v1/environments/{env}/draft
- POST /api/config/v1/environments/{env}/draft/validate
- POST /api/config/v1/environments/{env}/draft/publish
- DELETE /api/config/v1/environments/{env}/draft
- GET /api/config/v1/environments/{env}/releases
- GET /api/config/v1/environments/{env}/releases/{id}
- POST /api/config/v1/environments/{env}/releases/{id}/rollback
- GET /api/config/v1/audit-events
- GET /api/config/v1/instances

写 API 要求 super_admin。

### 12.2 内部服务 API

- GET /internal/config/v1/snapshots/{service}
- GET /internal/config/v1/watch/{service}
- POST /internal/config/v1/instances/heartbeat
- POST /internal/config/v1/instances/apply-result
- GET /internal/config/v1/catalog/{service}

内部 API 需要服务身份认证，不接受普通用户 token。

### 12.3 健康检查

- GET /health：进程存活；
- GET /ready：MongoDB、catalog 和 active release 可读取；
- GET /api/config/v1/status：Consul 同步、reconciliation、实例 stale 数量。

## 13. 认证、权限和安全

### 13.1 管理员认证

复用 User Service：

- 前端使用 User Service 登录或已有 access token；
- 后端调用 /api/auth/verify；
- 只允许 human_user；
- 写操作要求 role=super_admin；
- 普通 user 不提供配置中心入口。

不在配置中心维护第二套用户名密码。

### 13.2 服务身份

MVP：

- CONFIG_CENTER_INTERNAL_API_SECRET 作为 bootstrap secret；
- 服务带 x-internal-api-secret 和 service identity；
- 配置中心校验 service_name 是否匹配允许范围。

后续：

- 迁移到 User Service 颁发的 service token；
- token audience 指向 configuration-center；
- 支持短期 token 和轮换。

### 13.3 敏感数据

- API 响应不返回 secret 原文；
- 审计只记录 reference 和 changed 标记；
- 前端字段默认掩码；
- 导出配置时排除秘密；
- 日志对 URL credential、token、password 和 secret 做 redaction；
- Catalog 明确 sensitivity=secret_ref；
- 普通 JSON 类型不能保存已知 secret key。

### 13.4 变更保护

- 使用 base_revision 乐观锁；
- 发布必须填写 message；
- production 可配置双人审批作为后续能力；
- 危险配置显示影响范围；
- restart_required 配置发布前显示实例列表；
- critical 配置不能发布 null；
- 删除 Key 必须先 deprecated。

## 14. 前端功能

### 14.1 技术栈

- React 19；
- TypeScript；
- Vite；
- Ant Design 6；
- React Query；
- React Router；
- dayjs。

复用 Task Runner 和 Plugin Management 前端的登录、API、布局和错误处理模式。

### 14.2 页面

#### Dashboard

- 当前环境和 active revision；
- 配置中心、MongoDB、Consul 状态；
- 服务实例在线数量；
- stale 实例；
- pending restart 实例；
- 最近发布和失败发布。

#### Config Catalog

- 按 shared、service、category 分组；
- 搜索 Key、名称、标签；
- 查看类型、默认值、当前值、来源和 reload mode；
- 查看 env alias 和使用服务。

#### Config Editor

- 基于 Schema 生成 AntD Form；
- 显示默认值、当前发布值和草稿值；
- InputNumber 支持 min/max；
- Select 支持 enum；
- Switch 支持 bool；
- bytes 和 duration 使用带单位输入；
- JSON 使用校验编辑器或 TextArea；
- secret_ref 使用 provider + reference name；
- 显示修改影响和是否需要重启。

#### Diff and Publish

- 展示 added、changed、removed；
- 展示旧值和新值；
- secret 只显示 changed；
- 展示受影响服务；
- 展示 validation warnings；
- 填写发布说明；
- 发布成功后跳转 release 详情。

#### Release History

- revision、时间、操作者、说明和状态；
- 查看 diff；
- 回滚；
- 查看 Consul 同步结果；
- 查看实例应用进度。

#### Service Instances

- 服务名、实例 ID、版本、revision、checksum；
- online、stale、pending restart；
- 本地紧急覆盖；
- 最后错误；
- 最后心跳。

#### Audit Log

- 按操作者、动作、Key、服务、时间筛选；
- 查看发布、回滚、校验失败和权限拒绝。

## 15. Chat OS 改造

### 15.1 后端

新增 ChatOsManagedConfig，至少包含：

- ai.max_iterations；
- ai.max_output_tokens；
- task.follow_up_max_rounds；
- conversation.history_limit；
- attachment.total_max_bytes；
- ui.locale；
- ui.terminal_enabled；
- ai.internal_context_locale；
- logging.level。

改造点：

- conversation runtime 每次新请求读取当前 snapshot；
- attachment 校验读取 managed config；
- task follow-up 读取 managed config；
- history limit 读取 managed config；
- UI bootstrap API 返回全局 UI 配置和 revision；
- logger 后续接入 tracing_subscriber reload handle；
- 不再合并用户 settings 覆盖。

### 15.2 User Settings API 迁移

分三个阶段：

阶段 A：

- GET /api/user-settings 返回配置中心全局 effective 值，并合并当前用户的语言偏好；
- PUT/PATCH 只接受 UI_LOCALE 和 INTERNAL_CONTEXT_LOCALE；
- 其他平台运行参数不再写 user_settings 集合。

阶段 B：

- Chat OS 前端把 UserSettingsPanel 缩减为用户偏好面板；
- Header 入口改名为“用户偏好”；
- I18nProvider 继续读取当前用户的 UI_LOCALE；
- 发送附件前读取全局 attachment limit；
- 删除 updateUserSettings 调用。

阶段 C：

- 备份并忽略 user_settings 中的历史平台运行 Key；
- repository 和写 API 只允许维护两个语言偏好；
- 后续可将这两个字段迁移到独立 UserPreferences 模型。

### 15.3 明确保留的会话设置

以下仍保留，因为它们属于会话选择：

- selected_model_id；
- selected_model_name；
- selected_thinking_level；
- remote_connection_id；
- workspace_root；
- reasoning_enabled；
- plan_mode_enabled。

不要因为移除 UserSettingsPanel 而删除 SessionRuntimeSettings。

## 16. Task Runner 改造

### 16.1 后端

新增 TaskRunnerManagedConfig，至少包含：

- execution.max_iterations；
- execution.timeout_ms；
- ai.tool_result_max_chars；
- ai.tool_results_total_max_chars；
- execution.environment_mode；
- sandbox.enabled；
- sandbox.manager_base_url；
- sandbox.lease_ttl_seconds；
- worker.poll_interval_ms；
- worker.claim_ttl_ms；
- worker.concurrency；
- scheduler.poll_interval_ms；
- auto_memory_summary。

行为：

- 每个新 Run 构建 runtime config 时读取当前 snapshot；
- Run input_snapshot 记录 config revision 和关键参数；
- 正在运行的 Run 不因发布新值而改变上限；
- 新 Run 立即使用新 revision；
- 运行详情展示实际 max_iterations 和 config revision；
- 不再读取 runtime_settings 作为 effective 值。

### 16.2 Task Runner 设置页面

- 删除可编辑 RuntimeSettingsForm；
- Overview 保留只读 effective config；
- 显示“由 Configuration Center 管理”；
- 提供配置中心页面链接；
- 显示 revision、source 和 pending restart；
- /api/system/config PATCH 在过渡期返回受管错误；
- GET 继续返回只读值，兼容旧前端。

### 16.3 历史数据

迁移工具读取 runtime_settings.system，但不能盲目把历史值当成新基线。

当前环境建议明确发布：

- agent.runtime.max_iterations = 600；
- 对旧值 25 记录迁移警告；
- 迁移报告记录 old_value、selected_value 和理由；
- 发布完成后 Task Runner 忽略 runtime_settings；
- 稳定一个版本后删除 runtime_settings 集合和更新 API。

## 17. 其他服务接入

### 17.1 User Service

首批受管参数：

- access token TTL；
- 注册码 TTL、重发间隔和尝试次数；
- 登录失败阈值、窗口和锁定时间；
- 下游请求超时；
- SMTP 非敏感参数；
- Harness 请求超时和资源命名策略。

JWT secret、SMTP password、Harness PAT 只使用 secret_ref 或 bootstrap。

### 17.2 Project Service

首批受管参数：

- 下游请求超时；
- cloud project zip、unpacked bytes 和文件数量上限；
- Git 操作 timeout；
- sandbox image MCP timeout；
- Memory timeout。

项目自身的 sandbox_enabled、runtime image 等仍是领域数据。

### 17.3 Plugin Management

首批受管参数：

- CORS；
- Local Connector 检查 TTL；
- 工具快照最大字节数；
- 下游请求超时；
- Feature flags。

MCP、Skill、Agent 记录不迁移。

### 17.4 Local Connector Service

首批受管参数：

- Relay timeout；
- active session lease TTL；
- device signature max skew；
- sandbox image relay timeout；
- Memory timeout。

Local Connector Client 需要单独处理：

- 云端受管策略在设备登录后同步；
- 离线时使用本机 last-known-good；
- 本机路径、开发者本地 URL、Docker executable 和设备授权不能变成全局云端值；
- 管理员策略只能收紧本机安全上限，不能静默扩大本机权限。

### 17.5 Sandbox Manager

首批受管参数：

- lease 和 cleanup 策略；
- image tag prefix；
- agent endpoint mode；
- Kata runtime 名称；
- 资源限制和功能开关。

backend、Docker network、work root、database URL 属于 restart 或 bootstrap。

### 17.6 Memory Engine

首批受管参数：

- summary token limit；
- rollup 策略；
- job worker 参数；
- 模型请求超时；
- 下游服务超时。

模型 API Key 和 operator token 不保存明文。

### 17.7 Official Website

首批受管参数：

- public base URL；
- Chat OS app URL；
- release catalog 开关；
- 上传大小和缓存策略。

release upload token 只使用 secret_ref。

## 18. 初始配置目录

建议第一批发布以下 Key：

| Key | 默认值 | 类型 | 生效方式 |
| --- | ---: | --- | --- |
| shared.logging.level | info | enum | restart_required，后续 hot_reload |
| agent.runtime.max_iterations | 600 | integer | next_request |
| chatos.ai.max_output_tokens | null | integer nullable | next_request |
| chatos.task.follow_up_max_rounds | 3 | integer | next_request |
| chatos.conversation.history_limit | 20 | integer | next_request |
| chatos.attachment.total_max_bytes | 20971520 | bytes | hot_reload |
| chatos.ui.terminal_enabled | true | boolean | hot_reload |
| task_runner.execution.timeout_ms | 7200000 | duration_ms | next_run |
| task_runner.ai.tool_result_max_chars | 8000 | integer | next_run |
| task_runner.ai.tool_results_total_max_chars | 48000 | integer | next_run |
| task_runner.sandbox.enabled | true | boolean | next_run |
| task_runner.sandbox.lease_ttl_seconds | 7200 | integer | next_run |
| task_runner.worker.concurrency | 4 | integer | restart_required |
| task_runner.worker.claim_ttl_ms | 120000 | duration_ms | next_claim |
| task_runner.worker.poll_interval_ms | 1000 | duration_ms | hot_reload |

默认值最终以代码当前有效值和生产要求复核，不能仅从历史数据库自动导入。

## 19. 服务状态和可观测性

每个接入服务新增只读状态：

- config environment；
- config revision；
- config checksum；
- source；
- stale；
- loaded_at；
- last_refresh_at；
- pending_restart_keys；
- emergency_override_keys；
- last_error。

建议标准 endpoint：

- GET /api/internal/config-status

日志：

- 启动加载来源和 revision；
- 收到新 revision；
- 校验失败；
- 热更新成功；
- 需要重启；
- fallback 到 Consul 或 local cache；
- 使用 emergency override。

指标：

- config_refresh_success_total；
- config_refresh_failure_total；
- config_revision；
- config_stale；
- config_apply_latency_ms；
- config_pending_restart；
- config_consul_publish_failure_total。

## 20. 故障策略

### 20.1 配置中心不可用

- 已运行服务继续使用当前内存 snapshot；
- 新启动服务依次尝试 Config Center、Consul、local cache；
- 非关键配置最后使用代码默认；
- 关键配置无任何有效快照时 fail closed；
- UI 显示 stale，不把 fallback 伪装成最新。

### 20.2 Consul 不可用

- 配置中心仍可保存草稿；
- 发布保持 failed 或 degraded，不切换 active pointer；
- 已运行服务通过配置中心 API 获取快照；
- Consul 恢复后 reconciliation 自动重发。

### 20.3 配置错误

- Schema 不通过禁止发布；
- 服务端 typed conversion 失败时拒绝新 snapshot；
- 继续使用上一有效版本；
- 上报 apply failure；
- 管理员回滚。

### 20.4 部分实例未更新

- 发布仍记录为 published；
- 实例页显示 rollout progress；
- 超时未更新实例标记 stale；
- restart_required 实例标记 pending restart；
- 不自动重启生产服务，除非后续接入部署编排。

## 21. 迁移实施阶段

### 阶段 0：配置盘点和基线冻结

- 建立完整配置清单；
- 对所有环境变量标记分类；
- 确认当前 production effective 值；
- 明确旧数据库覆盖；
- 冻结新增用户级运行参数；
- 为每个 Key 指定 owner。

交付物：

- catalog JSON；
- legacy mapping；
- migration report 模板；
- 初始 production baseline。

### 阶段 1：配置中心骨架

- 新增 Rust Backend；
- 新增 React + AntD Frontend；
- 接入 MongoDB；
- 接入 User Service super_admin 认证；
- 完成 Catalog、Draft、Validate、Publish、Release、Audit；
- 完成 Consul publisher；
- Docker Compose 和 CI 构建通过。

此阶段暂不修改业务服务读取链路。

### 阶段 2：SDK 和兼容分发

- 新增 chatos_config_sdk；
- 扩展 chatos_service_runtime；
- 配置中心发布现有 Consul env JSON；
- 服务继续使用 apply_config_center_env；
- 实现实例 heartbeat 和 revision 状态；
- 建立 last-known-good。

### 阶段 3：Chat OS 和 Task Runner 首批迁移

- Chat OS 读取 managed config；
- 删除用户运行参数写入；
- 移除 UserSettingsPanel；
- Task Runner 读取 managed config；
- 设置页改只读；
- 明确发布 max_iterations=600；
- Run 记录 config revision；
- 保持兼容 GET API。

### 阶段 4：其他服务迁移

- User Service；
- Project Service；
- Plugin Management；
- Local Connector Service；
- Sandbox Manager；
- Memory Engine；
- Official Website。

按服务逐个迁移，不做一次性大爆炸切换。

### 阶段 5：移除旧配置源

- 删除 Chat OS user_settings 平台 Key；
- 删除 Task Runner runtime_settings；
- 禁止生产环境 legacy env 覆盖受管 Key；
- 保留 bootstrap 和 emergency allowlist；
- 更新文档和部署模板；
- 为旧 API 返回明确废弃状态。

## 22. 代码改动清单

### 22.1 新增

- config_center_service/backend
- config_center_service/frontend
- config_center_service/catalog
- crates/chatos_config_sdk

### 22.2 根工作区和构建

- Cargo.toml：加入 backend 和 SDK workspace member；
- Cargo.lock；
- docker/compose.yml：增加 backend、frontend、MongoDB database 配置；
- docker/compose.build.yml；
- docker/.env.example；
- .github/workflows/docker-images.yml；
- .drone.images.yml；
- .drone.yml；
- Makefile；
- scripts/local-dev-stack.sh；
- README.md 和安装部署文档。

### 22.3 共享运行时

- crates/chatos_service_runtime/src/runtime.rs；
- crates/chatos_service_runtime/src/env_config.rs；
- 新增 config center discovery、revision 和 compatibility snapshot 支持；
- 保留现有服务发现 API。

### 22.4 Chat OS

- chatos/backend/src/services/user_settings.rs；
- chatos/backend/src/api/user_settings.rs；
- chatos/backend/src/modules/conversation_runtime；
- chatos/backend/src/core/ai_settings.rs；
- chatos/backend/src/services/object_storage.rs；
- chatos/frontend/src/components/UserSettingsPanel.tsx；
- chatos/frontend/src/components/ChatInterface.tsx；
- chatos/frontend/src/i18n/I18nProvider.tsx；
- chatos/frontend/src/lib/store/actions/sendMessage.ts；
- API client 和类型。

### 22.5 Task Runner

- task_runner_service/backend/src/services/task_service/runtime_settings.rs；
- task_runner_service/backend/src/services/run_service.rs；
- task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs；
- task_runner_service/backend/src/models/model_config.rs；
- task_runner_service/backend/src/api/core/system.rs；
- task_runner_service/backend/src/store；
- task_runner_service/frontend/src/pages/SettingsPage.tsx；
- task_runner_service/frontend/src/pages/settings/SettingsSections.tsx；
- run 详情类型和 UI。

## 23. 测试方案

### 23.1 后端单元测试

- 所有 value type 解析；
- min/max、enum、regex 和 JSON Schema；
- 跨字段校验；
- shared + service 合并；
- revision compare-and-set；
- checksum 稳定；
- secret redaction；
- rollback 生成新 release；
- Consul payload 兼容；
- catalog 升级和 deprecated。

### 23.2 集成测试

- 创建草稿、发布、服务读取；
- 无效值不能发布；
- 并发编辑冲突；
- Consul 写失败不切 active pointer；
- reconciliation 恢复；
- Config Center 不可用时使用 Consul；
- Config Center 和 Consul 都不可用时使用 LKG；
- 错误 snapshot 不替换旧 snapshot；
- super_admin 可写、普通用户只返回 forbidden；
- 服务 secret 错误被拒绝。

### 23.3 Chat OS 测试

- 普通用户看不到运行参数面板；
- PUT/PATCH user settings 不再写数据库；
- UI locale 来自全局配置；
- attachment limit 全用户一致；
- max_iterations 新请求生效；
- 会话模型和 workspace 设置不受影响。

### 23.4 Task Runner 测试

- 数据库 runtime_settings=25 时，新 Run 仍使用中心 600；
- 新 Run 记录 config revision；
- 运行中的 Run 不被中途修改；
- 发布后下一 Run 使用新值；
- 配置中心 fallback；
- 设置页面只读；
- PATCH system config 被拒绝；
- completion gate 多次 run_report 时每段 effective limit 可追踪。

### 23.5 前端测试

- Schema 生成控件；
- bytes 和 duration 单位转换；
- diff；
- optimistic lock 冲突；
- secret masking；
- publish warning；
- rollback；
- instance stale 和 pending restart。

## 24. 验收标准

### 场景一：统一最大迭代次数

- 管理员在配置中心把 Chat OS 和 Task Runner 最大迭代设置为 600；
- 发布生成新 revision；
- Chat OS 新会话请求使用 600；
- Task Runner 新 Run 使用 600；
- 普通用户没有修改入口；
- Task Runner 数据库历史 25 不再影响结果；
- Run 详情显示实际值和 revision。

### 场景二：全局 UI 配置

- 管理员设置 UI locale 和终端菜单开关；
- 所有用户读取相同配置；
- 不读取 user_settings 覆盖；
- 发布后新页面加载生效。

### 场景三：错误配置

- max_iterations=0 被后端拒绝；
- tool total budget 小于单工具 budget 被拒绝；
- 不生成 release；
- 线上实例不受影响。

### 场景四：回滚

- 发布错误 Feature Flag；
- 管理员选择上一 release 回滚；
- 系统生成新 revision；
- 服务更新到回滚 revision；
- 审计可看到完整链路。

### 场景五：配置中心故障

- 配置中心停止；
- 已运行服务继续运行；
- 新实例从 Consul 或 LKG 启动；
- status 明确显示 fallback 和 stale；
- 配置中心恢复后自动追上 active revision。

## 25. 风险和控制

### 25.1 单点故障

控制：

- MongoDB 权威存储；
- Consul 已发布快照；
- 服务本地 LKG；
- 运行中配置保留；
- 配置中心故障不清空配置。

### 25.2 配置中心范围过大

控制：

- 区分配置和业务数据；
- 分服务迁移；
- Catalog owner；
- 不允许任意 env 注入；
- 不把 secret 明文纳入首期。

### 25.3 热更新引入不一致

控制：

- revision 和 checksum；
- 原子替换 typed config；
- next_request、next_run、restart_required 明确区分；
- 实例心跳上报；
- Run 保存配置快照信息。

### 25.4 历史值污染新基线

控制：

- 迁移报告人工确认；
- 代码默认、环境值和数据库值三方对比；
- 不自动选择“最后写入值”；
- 对 25/600 等已知冲突建立显式迁移规则。

## 26. 推荐开发顺序

P0：

- 配置 Catalog 和分类；
- Configuration Center Backend；
- MongoDB release/snapshot/audit；
- User Service super_admin 认证；
- Consul publisher；
- 基础 React + AntD 管理台。

P1：

- chatos_config_sdk；
- service instance heartbeat；
- Chat OS 用户运行参数迁移；
- Task Runner runtime_settings 迁移；
- 当前环境 max_iterations 修正为 600。

P2：

- 动态 watch 和 LKG；
- 其他服务参数迁移；
- 实例 rollout 和 pending restart；
- 日志级别热更新。

P3：

- Secret Manager 集成；
- production 双人审批；
- 灰度实例组；
- 定时发布；
- 配置模板和跨环境推广。

## 27. 评审时需要确认的产品决策

本方案给出以下推荐默认，实施前应确认：

1. 配置中心只允许 super_admin 管理，推荐确认。
2. 每个部署环境只有一套全局配置，不提供用户覆盖，推荐确认。
3. UI_LOCALE 和 INTERNAL_CONTEXT_LOCALE 保留为每个账号独立的用户偏好。
4. 会话模型、思考等级、工作区和 Plan Mode 继续允许用户选择，推荐保留。
5. secret 首期只存 reference，不存明文，强烈推荐确认。
6. production 发布暂不自动重启服务，只显示 pending restart，推荐确认。
7. 当前 Task Runner max_iterations 迁移基线固定为 600，不导入历史 25，推荐确认。

## 28. 最终交付定义

完成本方案后应达到：

- 有独立 Configuration Center Backend 和 Frontend；
- 管理员可以统一编辑、发布、查看、回滚配置；
- 配置有 Schema、类型、校验、版本和审计；
- 已发布配置同步到 Consul；
- Rust 服务使用共享 SDK；
- Chat OS 不再向普通用户展示平台运行参数；
- Task Runner 不再使用 runtime_settings 覆盖全局默认；
- 新 Run 能显示实际配置 revision；
- 旧值 25 不再覆盖 600；
- 配置中心故障时服务仍能使用最近有效配置；
- secret 不在配置中心明文暴露；
- 部署、CI、测试和文档完整。

## 29. 本次实施结果（2026-07-15）

已完成：

- 新增 Rust Configuration Center Backend 和 React + Ant Design Frontend；
- 完成配置 Catalog、typed 自定义开发参数、草稿、校验、发布、不可变快照、版本、回滚和审计；
- 完成 User Service `super_admin` 登录校验和内部 API Secret 鉴权；
- 完成 MongoDB 权威存储、Consul KV 兼容发布、ETag/304 快照接口；
- 新增 `chatos_config_sdk`，支持 typed getter、ETag、本地 LKG、轮询 watch 和实例 revision 上报；
- Chat OS 与 Task Runner 已切换到统一配置，普通用户侧运行参数入口和数据库覆盖已停用；
- Chat OS 的 UI_LOCALE 和 INTERNAL_CONTEXT_LOCALE 保留为用户偏好，不进入全局配置中心；
- Chat OS 与 Task Runner 的最大迭代次数基线均为 600，历史 Task Runner `runtime_settings=25` 不再参与生效计算；
- 配置中心后端和前端已加入 Cargo workspace、Docker Compose、本地构建、镜像 CI、部署脚本、本地开发栈及根 Makefile；
- README 已补充服务端口、访问地址和安全边界。

后续增强项：

- 逐步把其他微服务的更多业务运行参数迁入 Catalog；
- 将配置 revision/checksum 固化到每个 Task Run 记录并在运行详情展示；
- 完成 restart-required 的自动 rollout、实例差异和灰度发布；
- 接入独立 Secret Manager、生产审批和跨环境推广。
