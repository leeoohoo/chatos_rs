# Chatos RS 抽象与性能优化解决方案

生成日期：2026-06-27

## 1. 结论摘要

这个仓库是多服务 monorepo：主 ChatOS 后端、多个 Rust 共享 crate、Task Runner、Project Management、Memory Engine、User Service、DB Connection Hub 和多个 React 前端并存。当前最大的问题不是单个文件过大，而是服务边界之间已有相似能力但抽象层不统一，导致认证、模型配置、MCP contract、项目依赖图、store CRUD、前端页面状态管理在多个地方重复演进。

优先级建议：

1. 先治理源码大文件和热点文件预算，避免核心业务代码继续集中到千行级文件。
2. 抽 `identity/service-runtime/model-catalog/project-mcp-contract` 四类共享层。
3. 拆 `project_management_service` 的 store/router/MCP 文件，建立领域 service 层。
4. 给列表、依赖图、模型 catalog、MCP 初始化、Memory Engine 摘要流程做有界分页、缓存和并发控制。
5. 前端按 Task Runner 已有模式拆 container/hook/table/drawer，重点处理 Project Detail、Project Plan、Run Settings。

## 2. 源码大文件盘点

本节只统计我们维护的代码文件，排除 `target-*`、`.cache`、`.local`、`bundled-tools` 二进制、lockfile、文档和构建产物。源码大文件同时看两个指标：文件体积和行数。体积偏大常影响加载、审查和前端 bundle；行数偏大通常说明职责集中、测试难拆、变更冲突概率高。

### 2.1 按文件体积排序

| 文件 | 体积 | 判断 |
| --- | ---: | --- |
| `chat_app/src/i18n/messages/enUS.ts` | 104.3 KB | i18n 数据集中，建议按 namespace 拆分 |
| `project_management_service/backend/src/store/sqlite.rs` | 104.0 KB | SQLite CRUD、迁移、映射、测试集中 |
| `chat_app/src/i18n/messages/zhCN.ts` | 101.9 KB | i18n 数据集中，建议按 namespace 拆分 |
| `project_management_service/backend/src/api/router.rs` | 71.2 KB | 路由、认证、同步、依赖图、任务联动混合 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests.rs` | 59.3 KB | AI client 测试集中，可按场景拆 |
| `chat_app_server_rs/src/api/projects/requirement_execution_handlers.rs` | 59.0 KB | 需求执行 usecase 过载 |
| `project_management_service/backend/src/store/mongo.rs` | 56.3 KB | Mongo CRUD 与 SQLite 同构重复 |
| `project_management_service/backend/src/mcp_server.rs` | 56.2 KB | MCP schema、handler、领域逻辑混合 |
| `user_service/backend/src/api/models.rs` | 50.4 KB | 模型配置、provider catalog、同步、响应投影混合 |
| `project_management_service/frontend/src/pages/ProjectDetailPage.tsx` | 49.7 KB | 页面容器过载 |
| `chat_app_server_rs/src/api/configs/ai_model.rs` | 48.7 KB | 与 user_service 模型配置能力重叠 |
| `chat_app_server_rs/src/api/agent_chat.rs` | 48.4 KB | chat handler 与 Task Runner callback 混合 |
| `task_runner_service/backend/src/mcp_server/tests.rs` | 46.8 KB | MCP 测试集中 |
| `crates/chatos_mcp_runtime/src/executor.rs` | 43.6 KB | MCP 注册、调度、解析、结果归并集中 |
| `chat_app_server_rs/src/services/project_run/analyzer.rs` | 42.7 KB | 多语言运行目标探测集中 |
| `chat_app/src/components/projectExplorer/ProjectRunSettingsPanel.tsx` | 40.4 KB | props 与 UI 状态过宽 |
| `chat_app/src/components/projectExplorer/ProjectPlanPane.tsx` | 39.8 KB | 依赖图计算和渲染混合 |
| `task_runner_service/frontend/src/i18n/messages/enUS.ts` | 39.5 KB | i18n namespace 未拆 |
| `task_runner_service/frontend/src/i18n/messages/zhCN.ts` | 39.3 KB | i18n namespace 未拆 |
| `chat_app/src/components/ToolCallRenderer.test.tsx` | 37.2 KB | 测试集中 |

### 2.2 按行数排序

| 文件 | 行数 | 问题类型 |
| --- | ---: | --- |
| `project_management_service/backend/src/store/sqlite.rs` | 2545 | SQLite CRUD、迁移、映射、测试集中 |
| `project_management_service/backend/src/api/router.rs` | 1914 | 路由、认证、同步、依赖图、任务联动混合 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests.rs` | 1698 | 测试集中，可按场景拆分 |
| `chat_app/src/i18n/messages/zhCN.ts` | 1685 | i18n namespace 未拆 |
| `chat_app/src/i18n/messages/enUS.ts` | 1685 | i18n namespace 未拆 |
| `chat_app_server_rs/src/api/projects/requirement_execution_handlers.rs` | 1558 | 需求执行 usecase 过载 |
| `project_management_service/backend/src/store/mongo.rs` | 1448 | Mongo CRUD 与 SQLite 同构重复 |
| `project_management_service/backend/src/mcp_server.rs` | 1446 | MCP schema、handler、领域逻辑混合 |
| `user_service/backend/src/api/models.rs` | 1398 | 模型配置、provider catalog、同步、响应投影混合 |
| `chat_app_server_rs/src/api/configs/ai_model.rs` | 1369 | 与 user_service 模型配置能力重叠 |
| `project_management_service/frontend/src/pages/ProjectDetailPage.tsx` | 1344 | 页面容器过载 |
| `chat_app_server_rs/src/services/project_run/analyzer.rs` | 1265 | 多语言运行目标探测集中 |
| `chat_app_server_rs/src/api/agent_chat.rs` | 1242 | chat handler 与 Task Runner callback 混合 |
| `crates/chatos_mcp_runtime/src/executor.rs` | 1155 | MCP 注册、调度、解析、结果归并集中 |

### 2.3 优先处理分组

第一组：必须优先拆的业务热点。

- `project_management_service/backend/src/store/sqlite.rs`
- `project_management_service/backend/src/api/router.rs`
- `project_management_service/backend/src/store/mongo.rs`
- `project_management_service/backend/src/mcp_server.rs`
- `chat_app_server_rs/src/api/projects/requirement_execution_handlers.rs`
- `user_service/backend/src/api/models.rs`
- `chat_app_server_rs/src/api/configs/ai_model.rs`

第二组：前端页面容器热点。

- `project_management_service/frontend/src/pages/ProjectDetailPage.tsx`
- `chat_app/src/components/projectExplorer/ProjectPlanPane.tsx`
- `chat_app/src/components/projectExplorer/ProjectRunSettingsPanel.tsx`
- `task_runner_service/frontend/src/pages/tasks/TaskDetailDrawer.tsx`

第三组：代码数据文件和测试热点。

- `chat_app/src/i18n/messages/*.ts`
- `task_runner_service/frontend/src/i18n/messages/*.ts`
- `chat_app_server_rs/src/services/agent_runtime/ai_client/tests.rs`
- `task_runner_service/backend/src/mcp_server/tests.rs`

非代码大文件说明：本地 `target-*` 目录、`bundled-tools/**/rg*`、lockfile 确实占体积，但不属于本次“代码大文件”重构对象。它们只需要独立的仓库治理或本地清理策略。

## 3. 可抽象方向

### 3.1 服务运行基础层：`crates/chatos_service_runtime`

范围：

- Axum 通用 router layer：CORS、TraceLayer、request id、health handler。
- 统一 `ApiError` / `ApiResult` / status 映射。
- env/config 解析工具，包含 bounded int、URL normalize、secret normalize。
- pagination parser：当前 `chat_app_server_rs/src/core/pagination.rs` 可作为起点。

收益：

- `chat_app_server_rs`、`project_management_service`、`user_service`、`memory_engine` 都在重复写启动层和错误映射。
- 后续可以统一请求日志字段、超时、跨服务错误格式。

### 3.2 身份与授权层：`crates/chatos_identity`

范围：

- `CurrentPrincipal` / `CurrentUser` / owner scope / tenant scope。
- bearer token 解析、user_service token verify client。
- human user、agent account、super_admin、operator 的统一判定。

现状：

- `user_service/backend/src/auth.rs` 定义 claim 和 token。
- `project_management_service/backend/src/auth.rs`、`memory_engine/backend/src/api/memory_auth.rs`、`chat_app_server_rs/src/services/user_service_api_client.rs` 复写了 verify/owner scope 逻辑。

收益：

- 避免 owner scope 判断在 Memory、Project、Task Runner 间漂移。
- API handler 可以只依赖一个 typed extractor。

### 3.3 模型配置与 Provider Catalog：`crates/chatos_model_catalog`

范围：

- provider normalize、默认 base_url、thinking level、capabilities。
- live model catalog fetch、fallback model list、错误标准化。
- public projection、secret redaction、`has_api_key` 规则。

现状：

- `user_service/backend/src/api/models.rs` 负责模型配置落库、provider refresh、同步集成。
- `chat_app_server_rs/src/api/configs/ai_model.rs` 负责兼容旧接口并代理 user_service。
- `task_runner_service` 也有 model catalog 和 model config service。

收益：

- 减少 GPT/DeepSeek/Kimi/Minimax/OpenAI-compatible 规则重复。
- 后续新增 provider 时只改一处。

### 3.4 Project Management MCP Contract：`crates/chatos_project_mcp_contract`

范围：

- tool name、args struct、JSON schema builder、status enum 值。
- Project Management MCP HTTP/JSON-RPC request/response 类型。
- archived filter / visible filter 等 contract 级规则。

现状：

- `project_management_service/backend/src/mcp_server.rs` 和 `task_runner_service/backend/src/services/builtin_providers/project_management.rs` 都手写同一批 tool schema。
- 新增字段时很容易一边更新、一边遗漏。

收益：

- Project Management 暴露端和 Task Runner 内置调用端共享同一份工具定义。
- 可以把 schema snapshot 纳入测试，防止 contract 漂移。

### 3.5 Project Management Store/Domain 层

范围：

- 把 `AppStore` 的手工 enum 转发改成 `ProjectStore` trait 或按 aggregate 拆分的 repository trait。
- 拆分 SQLite/Mongo 实现：`projects.rs`、`requirements.rs`、`work_items.rs`、`dependencies.rs`、`task_runner_links.rs`。
- 把 validate、normalize、status propagation、dependency graph 从 store/router/MCP 中提到 domain service。

现状：

- `store/mod.rs` 每个方法都 `match Mongo/Sqlite` 转发。
- SQLite/Mongo 中有大量同构 create/update/list 逻辑。
- `api/router.rs` 和 `mcp_server.rs` 都有 access check、dependency graph node、status tag 类逻辑。

收益：

- 文件体积下降。
- 单元测试可以直接打 domain service，不必通过 router 或具体数据库。
- Mongo/SQLite 实现只负责持久化，不承载业务流程。

### 3.6 需求执行与任务依赖编排

范围：

- 把 `execute_requirement_inner`、topological sort、prerequisite validation、status sync 拆为 `RequirementExecutionService`。
- 提供 Project Service 的一次性 plan snapshot API，避免 Chat Server 先拉 requirements、work_items、dependency_graph 三次 HTTP。
- status 映射统一到领域层，例如 task_runner status 到 work item/requirement status。

收益：

- 降低 `requirement_execution_handlers.rs` 和 `project_management_service/api/router.rs` 的复杂度。
- 减少跨服务请求次数和不一致窗口。

### 3.7 前端页面抽象

优先拆分：

- `project_management_service/frontend/src/pages/ProjectDetailPage.tsx`
  - `useProjectDetailData`
  - `useRequirementMutations`
  - `useWorkItemMutations`
  - `RequirementTable`
  - `WorkItemTable`
  - `DependencyDrawer`
  - `ProfileEditor`
- `chat_app/src/components/projectExplorer/ProjectPlanPane.tsx`
  - `projectPlanGraph.ts`
  - `useProjectPlanData`
  - `RequirementColumns`
  - `WorkItemDependencyList`
  - `ExecutionActions`
- `chat_app/src/components/projectExplorer/ProjectRunSettingsPanel.tsx`
  - 将 40+ props 收敛成 view model。
  - 拆 `RunTargetSelector`、`ToolchainSettings`、`ConfigFilesPanel`、`RunnerTerminalPanel`。

参考模板：

- `task_runner_service/frontend/src/pages/tasks/*`
- `task_runner_service/frontend/src/pages/models/*`

Task Runner 前端已经把 data hooks、mutations、table、drawer 拆开，适合作为其他页面的迁移模板。

## 4. 性能优化点

### 4.1 构建与本地运行性能

问题：

- 本地 `target-*` 总量超过 60GB。
- root workspace 未纳入 `user_service/backend`、`memory_engine/backend`，且 `memory_engine/backend` 使用 `mongodb = 3`，主 workspace 多数服务使用 `mongodb = 2.8`，会增加重复编译和共享抽象成本。

方案：

- 统一各 restart 脚本的 `CARGO_TARGET_DIR` 策略，默认进入 `target-shared`，仅隔离确有冲突的服务。
- 给 Windows dev profile 降低 debug info 或提供 `FAST_DEV=1`。
- 评估把 `user_service/backend` 纳入 workspace；`memory_engine/backend` 先处理 `mongodb` 版本差异后再纳入。
- 增加 `make clean-local-artifacts` 和 `make size-report`。

### 4.2 数据访问性能

Project Management：

- 当前 SQLite migration 已有基础索引：project owner/status、requirement project/status、work item project/status、dependency 两端索引。
- 仍建议增加覆盖排序的组合索引：
  - `requirements(project_id, status, priority DESC, updated_at DESC)`
  - `project_work_items(project_id, status, sort_order ASC, priority DESC, updated_at DESC)`
  - `project_work_items(requirement_id, status, sort_order ASC, priority DESC, updated_at DESC)`
- 列表接口增加 limit/cursor，避免页面和 MCP 工具无界 `fetch_all`。
- 对详情页列表引入 summary projection，避免 `SELECT *` 把 detail/document content 一起带出。
- 依赖图 API 可支持 `scope=requirement_id`，大项目下不必每次生成全项目图。

User Service：

- `list_users_summary` 对每个用户再 `count_agents_by_owner`，用户数上来后会 N+1。改成 Mongo aggregation `$lookup` 或一次 group count。
- model provider refresh 现在可能阻塞 create/refresh 请求，建议把 live catalog fetch 缓存化，并允许异步刷新状态。

Memory Engine：

- 已有较好的并发和索引基础：worker 用 `join!` 并发 job，reconcile 用 `buffer_unordered` 控制并发，records page 用 `$facet` 同时取 items/total。
- `summarize_texts_once` 的 leaf chunk 当前顺序请求 AI。可在保证输出顺序的前提下给 leaf chunks 增加有界并发，例如 `summary_leaf_concurrency`，merge round 继续按批次收敛。
- 大 offset 分页可逐步切到 keyset/cursor，尤其 records、summaries、terminal/history 类接口。

Chat Server / AI Runtime / MCP：

- `McpExecutor::init` 每次运行会注册 HTTP、stdio、builtin tools。外部 MCP tool list 可以按 config id + updated_at + headers hash 做 TTL 缓存。
- `crates/chatos_ai_runtime/src/tool_runtime.rs` 已有 tool result budget，建议所有入口统一使用同一套 limits，并在 UI 上提示被截断的工具结果。
- `chat_app_server_rs/src/services/project_run/analyzer.rs` 已有限制 `MAX_SCAN_DIRS`、`MAX_SCAN_DEPTH`、`MAX_TARGETS`，后续可把 Node/Java/Python/Go/Rust detector 拆成独立策略并缓存 manifest hash。
- `requirement_execution_handlers.rs` 可减少三次 project service HTTP 读取，改成一次 plan snapshot。

### 4.3 前端性能

- Project Detail、Project Plan 中依赖图、树、表格数据都应拆成 selector，并保持 `useMemo` 输入稳定。
- 大列表/图视图引入 virtualization，例如 requirements/work items/table/drawer related tasks。
- i18n 按 namespace 拆分：`common`、`tasks`、`models`、`project`、`runSettings`，页面按需加载。
- Project Plan 的依赖排序和图映射可以移入纯函数模块，增加测试，并为超大图预留 Web Worker。
- React Query 增加 `staleTime`、`keepPreviousData`、局部 invalidation，减少 mutation 后整页重拉。

## 5. 分阶段落地方案

### Phase 0：源码大文件治理与基线，0.5-1 天

- 新增 `make code-size-report`，只输出源码文件体积、行数热点和超预算文件，不统计 lockfile、二进制、构建产物。
- 扩展 `scripts/check-hotspot-line-budgets.sh`，加入本次发现的业务热点文件，但先用 warning 模式落地。
- 给新增代码设置预算：后端业务模块建议不超过 700 行，前端页面容器建议不超过 500 行，测试文件建议按场景拆分。
- 为 i18n 文件建立 namespace 拆分规则，避免单个语言文件继续膨胀。

验收：

- 根目录能一键生成 code size report。
- 新增或继续膨胀的热点文件会在本地 smoke 或 CI 中被提示。

### Phase 1：共享基础 crate，3-5 天

- 建 `crates/chatos_service_runtime`：ApiError、pagination、env helpers、Axum layer builder。
- 建或扩展 `crates/chatos_identity`：principal、owner scope、bearer token、user_service verify client。
- 先接入 Project Management 和 Memory Engine 的 auth middleware，保持 API 响应兼容。

验收：

- Project Management / Memory Engine 认证测试通过。
- 原有 token、operator、agent account 场景行为不变。

### Phase 2：模型配置与 MCP contract，4-6 天

- 建 `crates/chatos_model_catalog`，迁移 provider normalize、default base_url、thinking level、catalog fetch。
- 建 `crates/chatos_project_mcp_contract`，迁移 tool schema 和 args。
- Project Management MCP server 与 Task Runner builtin provider 共同引用 contract。

验收：

- schema snapshot 测试覆盖所有 project-management MCP tools。
- 新增/修改 tool 参数只需改共享 contract。

### Phase 3：Project Management 拆分，5-8 天

- `store/sqlite.rs`、`store/mongo.rs` 按 aggregate 拆文件。
- `api/router.rs` 拆为 `projects.rs`、`requirements.rs`、`work_items.rs`、`sync.rs`、`mcp.rs`。
- 引入 `DependencyGraphService`、`RequirementExecutionStateService`。
- 增加 list limit/cursor 和 summary projection。

验收：

- `sqlite.rs`、`router.rs` 都降到 700 行以内。
- 依赖图、状态联动、Task Runner callback 测试覆盖。

### Phase 4：性能专项，5-8 天

- Project Service 增加 plan snapshot API。
- MCP tool list 增加 TTL/cache key。
- User Service model catalog refresh 支持缓存/后台刷新。
- Memory Engine leaf summary chunk 增加有界并发。
- 增加数据库组合索引迁移。

验收：

- 大项目 Project Detail 首屏请求次数减少。
- 需求执行启动阶段跨服务 HTTP 次数减少。
- Memory Engine 多 chunk 摘要在相同限流配置下耗时下降。

### Phase 5：前端拆分，4-6 天

- 拆 `ProjectDetailPage.tsx`。
- 拆 `ProjectPlanPane.tsx` 并补 graph selector 测试。
- 拆 `ProjectRunSettingsPanel.tsx` 的 view model 和子组件。
- 拆 i18n namespace。

验收：

- 单个页面组件不超过 400-500 行。
- 大列表渲染无明显卡顿。
- 类型检查通过，核心交互不回归。

## 6. 风险与注意事项

- `memory_engine/backend` 使用 `mongodb = 3`，主 workspace 多数服务使用 `mongodb = 2.8`。共享 DB helper 前先统一或隔离版本。
- Project Management 同时支持 SQLite 和 Mongo，抽 trait 时不要牺牲 SQLite 本地开发体验。
- MCP contract 迁移要保留现有 tool name 和 JSON schema，避免影响已运行的 Task Runner prompt。
- 模型配置涉及密钥，抽象时必须保留 secret redaction 与 `include_secret` 行为。
- 前端拆分不要先重写 UI，先迁移数据/状态/纯函数，降低回归面。

## 7. 推荐下一步

建议从 Phase 0 + Phase 2 开始。Phase 0 能立刻解决体积治理和 CI smoke 风险；Phase 2 能消除 Project Management 与 Task Runner 最容易漂移的 MCP contract 重复，并且改动边界相对清晰。随后再进入 Project Management store/router 的大拆分。
