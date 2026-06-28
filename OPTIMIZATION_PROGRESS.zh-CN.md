# 优化实施进度表

更新时间：2026-06-28

## 当前策略

按 `PROJECT_REFACTOR_PERFORMANCE_SOLUTION.zh-CN.md` 分阶段推进。Phase 0 治理工具已落地，Phase 2 的 Project Management MCP contract 首轮抽象已完成。Phase 3 已完成 Project Management 后端 router、MCP server、SQLite store、Mongo store 的主要热点收敛。Phase 4 已完成 Project plan snapshot、需求执行启动/停止读取收敛、Project Management requirements/work_items 常用列表排序组合索引，以及 Chat Server 需求执行 handler 的初步 usecase/helper 拆分。当前进入 Phase 5：已完成 Project Management 前端 `ProjectDetailPage`、Chat 前端 `ProjectPlanPane`、`ProjectRunSettingsPanel`、`MessageTaskGraphPanel`、`ConversationProcessTimelineModal`、`MarkdownRenderer.css`、`ToolCallRenderer.css`、Chat Server `ai_model` 配置 API、User Service model API、User Service 前端 `ModelsPage`、DB Hub SQL Server metadata detail node 解析、Chat Server `agent_chat` callback/callback message 测试拆分、Chat Server `agents` skills API、Chat Server `chatos_agents` runtime/provisioning、Chat Server `chatos_skills` import candidate、Chat Server message task graph、Project Run analyzer、Chat Server `message_handlers` compact history、Chat Server memory compat API、Chat Server Task Board runtime snapshot、Chat Server contact prompt builder、Project Management router MCP entrypoint、Project Management `models.rs` DTO 分域、MCP runtime executor、MCP runtime builtin prompt、Java/Rust/Go code-nav analysis、Chat Server code-nav symbol index、Chat Server User Service API client、Chat Server SQLite schema、ChatOS AI runtime request/request tests/tool runtime/traits/stream parse、Chat Server AI request handler helper、Chat Server AI client execution loop follow-up/review helper、Chat Server agent builder prompt 构造、Chat Server browser vision 工具、Chat Server Task Runner API client、Chat Server history process、Task Runner ChatOS message task graph、Task Runner run preparation、Code Maintainer apply_patch、Windows 本地栈 config、Task Runner 前端 `ModelsPage`/`SettingsPage`/`ToolingPage`/`TasksPage`/`TaskDetailDrawer`/API client 首轮拆分和 Task Runner 前端 `types.ts` 类型域拆分。

2026-06-28：补齐剩余硬热点治理，完成 Chat Server `ai_client`/`ai_common` 测试、Task Runner MCP 测试、Chat 前端 `ToolCallRenderer.test` 与 `sessions.selectSession.test` 拆分。当前默认硬预算已通过；`code-size-report` 仍显示 4 个超过 700 行的资源类文件，其中 Chat i18n 两个文件保留为 planned warning，Task Runner i18n 两个文件低于 planned 目标。

## 进度表

| 阶段 | 事项 | 状态 | 产出 | 验证 |
| --- | --- | --- | --- | --- |
| Phase 0 | 根目录进度表 | 已完成 | `OPTIMIZATION_PROGRESS.zh-CN.md` | 本文件持续维护 |
| Phase 0 | 源码体积报告 | 已完成 | `scripts/code-size-report.sh`、`make code-size-report` | `bash scripts/code-size-report.sh --top 10` 通过 |
| Phase 0 | 热点文件预算 warning 化 | 已完成 | `scripts/check-hotspot-line-budgets.sh --warn-planned`、`make hotspot-line-warnings` | 默认硬预算通过，planned warning 正常输出 |
| Phase 0 | Windows 本地栈启动脚本修复 | 已完成 | `scripts/local-dev-stack.ps1` 默认 Mongo，并在启动前 build 到 `target-shared` | `-Action status` 已通过 |
| Phase 2 | Project Management MCP contract 抽象 | 已完成 | `crates/chatos_project_mcp_contract`，集中 MCP/server/tool/schema/args | `cargo test -p chatos_project_mcp_contract` 通过 |
| Phase 2 | PM server / Task Runner builtin 复用 contract | 已完成 | PM MCP server 与 Task Runner builtin provider 复用共享 tool/schema/args | `cargo check -p project_management_service_backend -p task_runner_service_backend` 通过 |
| Phase 3 | domain 层抽象 | 已完成 | `project_management_service/backend/src/domain/{dependency_graph,visibility}.rs` | domain/router/mcp 相关测试通过 |
| Phase 3 | API handler 拆分 | 已完成 | `api/{access,dependencies,dependency_graph,projects,requirements,work_items,task_runner_links,sync}.rs` | `api/router.rs` 672 行，低于 700 目标 |
| Phase 3 | Sync 与 dependency graph 服务层 | 已完成 | `services/{execution_sync,dependency_graph}.rs` | `cargo check` 与相关 lib 测试通过 |
| Phase 3 | MCP tool call 分发拆分 | 已完成 | `mcp_tools.rs` 承接 tool 参数解码、权限校验和 PM 操作 | `mcp_server.rs` 413 行，`mcp_tools.rs` 614 行未形成新热点 |
| Phase 3 | SQLite row mapper 拆分 | 已完成 | `store/sqlite_rows.rs` | `sqlite.rs` 从 2664 行降到 2501 行 |
| Phase 3 | Store 共享输入规范化抽象 | 已完成 | `store/common.rs`、`store/sqlite_util.rs` | Mongo/SQLite 复用 git URL、ID list、Task Runner active status 规则 |
| Phase 3 | SQLite store aggregate 拆分 | 已完成 | `store/sqlite/{projects,requirements,work_items}.rs` 与 `store/sqlite/tests/*` | `sqlite.rs` 降到约 149 行；SQLite lib 测试通过 |
| Phase 3 | Mongo store aggregate 拆分 | 已完成 | `store/mongo/{projects,requirements,work_items}.rs` | `mongo.rs` 322 行；Mongo test profile 编译通过 |
| Phase 4 | Project plan snapshot API | 已完成 | `services/project_plan.rs`、`api/plan.rs`、`/api/projects/:project_id/plan` | PM 快照测试通过；Chat Server Project Plan 代理改为一次 PM HTTP |
| Phase 4 | 需求执行启动读取优化 | 已完成 | `requirement_execution_handlers.rs` 使用 plan snapshot 读取 requirements/work_items/dependency_graph | 启动阶段 PM 读取从 3 次 HTTP 降为 1 次 |
| Phase 4 | 需求执行停止读取优化 | 已完成 | `stop_requirement_execution_inner` 复用 plan snapshot | 停止阶段 PM 读取从 2 次 HTTP 降为 1 次 |
| Phase 4 | Chat Server 需求执行 handler 拆分 | 已完成 | `api/projects/requirement_execution/{context,plan,status,sync,tasks,types,values}.rs` | `requirement_execution_handlers.rs` 从 1634 行降到 457 行，低于 700 目标 |
| Phase 4 | requirements/work_items 组合索引 | 已完成 | SQLite migration + Mongo named indexes | SQLite schema 测试验证索引存在；PM 编译通过 |
| Phase 4 | MCP tool list cache | 已确认已有基础实现 | `crates/chatos_mcp_runtime/src/rpc.rs` 已有 success/error TTL cache 和共享 HTTP client | 后续如有压测需求再做 singleflight/指标 |
| Phase 5 | PM 前端 `ProjectDetailPage` 拆分 | 已完成 | `pages/projectDetail/{ProjectDetailTabs,ProjectDetailOverlays,columns,renderers,styles,types,utils,options}.ts(x)` | `ProjectDetailPage.tsx` 从 1425 行降到 390 行；PM 前端 type-check/build 通过 |
| Phase 5 | Chat 前端 `ProjectPlanPane` 拆分 | 已完成 | `components/projectExplorer/projectPlanPane/{model,components}.ts(x)` | `ProjectPlanPane.tsx` 从 987 行降到 487 行；chat_app type-check/build 通过 |
| Phase 5 | Chat 前端 `ProjectRunSettingsPanel` 拆分 | 已完成 | `components/projectExplorer/projectRunSettingsPanel/{model,RunEnvironmentDetails}.ts(x)` | `ProjectRunSettingsPanel.tsx` 降到 479 行；chat_app type-check/build 通过 |
| Phase 5 | Chat Server `ai_model` 配置 API 拆分 | 已完成 | `api/configs/ai_model/{config_handlers,provider_handlers,settings_handlers,model,provider_models,user_service_proxy}.rs` | `ai_model.rs` 从 1447 行降到 15 行；`cargo check` 与 `ai_model` 单测通过 |
| Phase 5 | User Service model API 拆分 | 已完成 | `api/models/{config_handlers,config_refresh,provider_handlers,provider_sync,provider_fetch,access,normalization,model_values,contracts,settings_handlers}.rs` | `models.rs` 从 1464 行降到 20 行；`cargo check` 通过 |
| Phase 5 | User Service 前端 `ModelsPage` 拆分 | 已完成 | `pages/models/{ModelProviderDrawer,modelPageUtils}.tsx` | `ModelsPage.tsx` 从 652 行降到 347 行；User Service 前端 type-check/build 通过 |
| Phase 5 | DB Hub SQL Server metadata detail node 解析拆分 | 已完成 | `drivers/sqlserver/metadata/detail/nodes.rs` | `detail.rs` 从 667 行降到 512 行；DB Hub 后端编译通过 |
| Phase 5 | Chat Server `agent_chat` callback 拆分 | 已完成 | `api/agent_chat/{task_runner_callback.rs,task_runner_callback/messages.rs}` | `agent_chat.rs` 从 1321 行降到 135 行；callback 单测通过 |
| Phase 5 | Chat Server callback message 测试拆分 | 已完成 | `api/agent_chat/task_runner_callback/messages/tests.rs` | `messages.rs` 从 650 行降到 413 行；callback 单测通过 |
| Phase 5 | Project Run analyzer 拆分 | 已完成 | `services/project_run/analyzer/{change_detection,target_model,node,java,python,go,rust,scan}.rs` | `analyzer.rs` 从 1342 行降到 93 行；Maven reactor 探测单测通过 |
| Phase 5 | Chat Server session message handler 拆分 | 已完成 | `api/sessions/message_handlers/{compact.rs,compact_merge.rs}` | `message_handlers.rs` 从 1039 行降到 278 行；compact history 与 user message turn 单测通过 |
| Phase 5 | MCP runtime executor 拆分 | 已完成 | `crates/chatos_mcp_runtime/src/executor/{execution,registration}.rs` | `executor.rs` 从 1247 行降到 428 行；`chatos_mcp_runtime` 编译与 executor 单测通过 |
| Phase 5 | Java code-nav analysis 拆分 | 已完成 | `services/code_nav/languages/java/analysis/syntax.rs` | `analysis.rs` 从 892 行降到 559 行；Java code-nav 单测通过 |
| Phase 5 | Go code-nav analysis 拆分 | 已完成 | `services/code_nav/languages/go/analysis/syntax.rs` | `analysis.rs` 从 636 行降到 450 行；Go code-nav 与跨语言 references 过滤测试通过 |
| Phase 5 | Chat Server User Service API client 拆分 | 已完成 | `services/user_service_api_client/{types,http}.rs` | `user_service_api_client.rs` 从 886 行降到 611 行；client 单测通过 |
| Phase 5 | Chat Server SQLite schema 拆分 | 已完成 | `db/sqlite_schema/statements.rs` | `sqlite_schema.rs` 从 883 行降到 500 行；Chat Server 编译与 sqlite 过滤测试通过 |
| Phase 5 | ChatOS AI runtime request 拆分 | 已完成 | `crates/chatos_ai_runtime/src/request/{types,http,streaming}.rs` | `request.rs` 从 863 行降到 641 行；`chatos_ai_runtime` 编译与 request 单测通过 |
| Phase 5 | ChatOS AI runtime request 测试拆分 | 已完成 | `crates/chatos_ai_runtime/src/request/tests.rs` | `request.rs` 从 641 行降到 387 行；`chatos_ai_runtime` 编译通过，request 复跑被 Windows 应用控制策略拦截 |
| Phase 5 | Chat Server browser vision 工具拆分 | 已完成 | `services/shared_builtin_browser_tools/{types,support,context,candidates,runner}.rs` | `shared_builtin_browser_tools.rs` 从 826 行降到 118 行；browser vision/caller runtime 单测通过 |
| Phase 5 | Chat Server Task Runner API client 拆分 | 已完成 | `services/task_runner_api_client/{types,tests}.rs` | `task_runner_api_client.rs` 从 813 行降到 473 行；client 编译与 token/skill 单测通过 |
| Phase 5 | Chat Server history process 拆分 | 已完成 | `api/sessions/history_process/{turn_slices,turn_display}.rs` | `history_process.rs` 从 811 行降到 579 行；history process 单测通过 |
| Phase 5 | Task Runner ChatOS message task graph 拆分 | 已完成 | `services/chatos_message_tasks/queries/{graph,tests}.rs` | `queries.rs` 从 646 行降到 348 行；Task Runner 后端编译与 graph 定向测试通过 |
| Phase 5 | Task Runner run preparation 拆分 | 已完成 | `services/run_model_phase/setup/preparation/{mcp_inputs,mcp_builder}.rs` | `preparation.rs` 从 806 行降到 438 行；preparation 单测通过 |
| Phase 5 | ChatOS AI runtime tool runtime 拆分 | 已完成 | `crates/chatos_ai_runtime/src/tool_runtime/{execution_plan,budget,items,tests}.rs` | `tool_runtime.rs` 从 801 行降到 23 行；tool_runtime 单测通过 |
| Phase 5 | Rust code-nav analysis 拆分 | 已完成 | `services/code_nav/languages/rust/{analysis,search}.rs` | `rust/mod.rs` 从 782 行降到 388 行；Chat Server 编译通过，rust_ 测试 exe 被 Windows 应用控制策略拦截 |
| Phase 5 | MCP runtime builtin prompt 拆分 | 已完成 | `crates/chatos_mcp_runtime/src/builtin_prompt/{sections,tests}.rs` | `builtin_prompt.rs` 从 780 行降到 497 行；builtin_prompt 单测通过 |
| Phase 5 | ChatOS AI runtime traits 拆分 | 已完成 | `crates/chatos_ai_runtime/src/traits/{model,records,executor,tests}.rs` | `traits.rs` 从 748 行降到 13 行；traits 单测通过 |
| Phase 5 | Chat Server code-nav symbol index 拆分 | 已完成 | `services/code_nav/symbol_index/{files,persistence,tests}.rs` | `symbol_index.rs` 从 733 行降到 386 行；symbol_index 单测通过 |
| Phase 5 | Code Maintainer apply_patch 拆分 | 已完成 | `crates/chatos_builtin_tools/src/code_maintainer/patch/{parser,hunks,replacement,tests}.rs` | `patch.rs` 从 731 行降到 129 行；格式和热点预算通过，定向测试被 Windows 应用控制策略拦截 |
| Phase 5 | Task Runner 前端 `ModelsPage` 拆分 | 已完成 | `pages/models/{ModelEditorDrawer,ModelDetailDrawer,ModelTestResultModal}.tsx` | `ModelsPage.tsx` 从 701 行降到 382 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Task Runner 前端 `SettingsPage` 拆分 | 已完成 | `pages/settings/{SettingsSections,settingsPageUtils}.ts(x)` | `SettingsPage.tsx` 从 644 行降到 224 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Task Runner 前端 `ToolingPage` 拆分 | 已完成 | `pages/tooling/{ToolingPanels,ToolingDrawers,toolingPageUtils}.ts(x)` | `ToolingPage.tsx` 从 620 行降到 247 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Task Runner 前端 `TasksPage` 表单/payload 映射抽象 | 已完成 | `pages/tasks/taskPageUtils.tsx` 新增 create/edit form value builder 和 submit payload builder | `TasksPage.tsx` 从 714 行降到 636 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Task Runner 前端 `TaskDetailDrawer` 拆分 | 已完成 | `pages/tasks/TaskDetailSections.tsx` | `TaskDetailDrawer.tsx` 从 796 行降到 375 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Chat 前端 `MessageTaskGraphPanel` 拆分 | 已完成 | `components/messageTasks/{MessageTaskGraphModel,MessageTaskGraphNode}.ts(x)` | `MessageTaskGraphPanel.tsx` 从 795 行降到 342 行；Chat 前端 type-check/build 与图标准化测试通过 |
| Phase 5 | Chat 前端 `ConversationProcessTimelineModal` 拆分 | 已完成 | `components/userMessages/{ConversationProcessTimelineModel,ConversationProcessTimelineCards}.ts(x)` | `ConversationProcessTimelineModal.tsx` 从 788 行降到 114 行；Chat 前端 type-check/build 通过 |
| Phase 5 | Task Runner 前端 `types.ts` 拆分 | 已完成 | `src/types/{auth,common,tasks,runs,memory,prompts,models,servers,mcp,tooling,system}.ts` | `types.ts` 从 987 行降到 11 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Chat 前端 `MarkdownRenderer.css` 拆分 | 已完成 | `components/markdownRenderer/{base,code,mermaid,content}.css` | `MarkdownRenderer.css` 从 897 行降到 4 行；Chat 前端 type-check/build 通过 |
| Phase 5 | Chat 前端 `ToolCallRenderer.css` 拆分 | 已完成 | `components/toolCallRenderer/{theme,chip,details,layout}.css` | `ToolCallRenderer.css` 从 786 行降到 4 行；Chat 前端 type-check/build 通过 |
| Phase 5 | Windows 本地栈脚本 config 拆分 | 已完成 | `scripts/local-dev-stack/config.ps1` | `local-dev-stack.ps1` 从 762 行降到 532 行；`-Action status` 通过 |
| Phase 5 | Chat Server Task Board snapshot 拆分 | 已完成 | `modules/conversation_runtime/task_board/snapshot.rs` | `task_board.rs` 从 699 行降到 520 行；task_board 定向测试通过 |
| Phase 5 | ChatOS AI runtime stream parse 拆分 | 已完成 | `crates/chatos_ai_runtime/src/stream_parse/{text,tool_calls}.rs` | `stream_parse.rs` 从 697 行降到 362 行；`chatos_ai_runtime` 编译与 request 单测通过 |
| Phase 5 | Chat Server AI request handler helper 拆分 | 已完成 | `services/agent_runtime/ai_request_handler/{http_client,payload,fingerprint}.rs` | `mod.rs` 从 646 行降到 457 行；Chat Server 编译与 ai_request_handler 定向测试通过 |
| Phase 5 | Chat Server AI client execution loop follow-up 拆分 | 已完成 | `services/agent_runtime/ai_client/execution_loop_follow_up.rs` | `execution_loop.rs` 从 683 行降到 617 行；follow-up helper 单测与 Chat Server 编译通过 |
| Phase 5 | Chat Server agent builder prompt 拆分 | 已完成 | `services/agent_builder/prompt.rs` | `agent_builder.rs` 从 692 行降到 594 行；Chat Server 编译通过 |
| Phase 5 | Chat Server agents skills API 拆分 | 已完成 | `api/agents/skills.rs` | `agents.rs` 从 689 行降到 426 行；Chat Server 编译与 agents 过滤测试通过 |
| Phase 5 | Chat Server `chatos_agents` runtime/provisioning 拆分 | 已完成 | `services/chatos_agents/{runtime,provisioning}.rs` | `chatos_agents.rs` 从 621 行降到 365 行；Chat Server 编译与 agents 过滤测试通过 |
| Phase 5 | Chat Server `chatos_skills` import candidate 拆分 | 已完成 | `services/chatos_skills_import.rs` | `chatos_skills.rs` 从 643 行降到 462 行；Chat Server 编译与 skill 过滤测试通过 |
| Phase 5 | Chat Server contact prompt builder 拆分 | 已完成 | `core/chat_runtime_contact/prompt_builder/{command,locale}.rs` | `prompt_builder.rs` 从 692 行降到 612 行；chat_runtime 定向测试通过 |
| Phase 5 | Chat Server message task graph 测试拆分 | 已完成 | `api/message_task_runner/graph/tests.rs` | `graph.rs` 从 676 行降到 364 行；normalize_graph 定向测试通过 |
| Phase 5 | Task Runner 前端 API HTTP 基础层拆分 | 已完成 | `src/api/http.ts` | `api/client.ts` 从 666 行降到 576 行；Task Runner 前端 type-check/build 通过 |
| Phase 5 | Project Management router MCP entrypoint 拆分 | 已完成 | `api/router/mcp.rs` | `router.rs` 从 672 行降到 281 行；PM 后端编译与 MCP 定向测试通过 |
| Phase 5 | Project Management models DTO 分域拆分 | 已完成 | `backend/src/models/{auth,common,graphs,projects,requirements,work_items}.rs` | `models.rs` 从 655 行降到 12 行；PM 后端编译、MCP 与 project_plan 定向测试通过 |
| Phase 5 | Project Management SQLite requirements store 拆分 | 已完成 | `store/sqlite/requirements/{dependencies,documents}.rs` | `requirements.rs` 从 649 行降到 431 行；PM 后端编译通过，project_plan 复跑被 Windows 应用控制策略拦截 |
| Phase 5 | Chat Server memory compat API 拆分 | 已完成 | `api/memory_compat/{contracts,support}.rs` | `memory_compat.rs` 从 638 行降到 458 行；Chat Server 编译与 memory_compat 过滤测试通过 |
| Phase 5 | Chat Server AI client 测试热点拆分 | 已完成 | `services/agent_runtime/ai_client/tests/{context,follow_up,recovery_http,recovery_retry,recovery_tools,transport}.rs` | `tests.rs` 从 1840 行降到 21 行；`ai_client` 39 个定向测试通过 |
| Phase 5 | Task Runner MCP server 测试热点拆分 | 已完成 | `mcp_server/tests/{schema,plan_profile}.rs` | `tests.rs` 从 1344 行降到 192 行；`mcp_` 34 个定向测试通过 |
| Phase 5 | Chat Server AI common 测试热点拆分 | 已完成 | `services/ai_common/tests/{metadata,tools,stream}.rs` | `tests.rs` 从 865 行降到 14 行；`ai_common` 33 个定向测试通过 |
| Phase 5 | Chat 前端 `ToolCallRenderer.test` 拆分 | 已完成 | `components/ToolCallRenderer.test/{helpers,summaries,codeMaintainer,builtinTools,browserResearch}.tsx` | 入口从 990 行降到 6 行；22 个目标测试、Chat type-check/build 通过 |
| Phase 5 | Chat 前端 `sessions.selectSession.test` 拆分 | 已完成 | `actions/sessions.selectSession.test/{testUtils,realtimeSync,selectionFlow,cache,pagination}.ts` | 入口从 979 行降到 14 行；12 个目标测试、Chat type-check/build 通过 |

## 当前源码基线

来自 `bash scripts/code-size-report.sh --top 40`：

| 指标 | 值 |
| --- | ---: |
| 源码文件数 | 2437 |
| 源码总体积 | 12.0 MB |
| 源码总行数 | 371149 |
| 超阈值热点数 | 4 |
| planned warning 数 | 2 |

Top 10 行数热点：

| 文件 | 行数 |
| --- | ---: |
| `chat_app/src/i18n/messages/enUS.ts` | 1686 |
| `chat_app/src/i18n/messages/zhCN.ts` | 1686 |
| `task_runner_service/frontend/src/i18n/messages/enUS.ts` | 727 |
| `task_runner_service/frontend/src/i18n/messages/zhCN.ts` | 727 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/recovery_tools.rs` | 663 |
| `task_runner_service/backend/src/mcp_server/tests/plan_profile.rs` | 653 |
| `task_runner_service/frontend/src/pages/TasksPage.tsx` | 636 |
| `chat_app/src/components/messageList/useMessageListWindowing.ts` | 623 |
| `chat_app_server_rs/src/services/realtime/hub.rs` | 619 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/execution_loop.rs` | 617 |

## 本轮影响

| 文件 | 当前行数 | 说明 |
| --- | ---: | --- |
| `project_management_service/backend/migrations/0001_init.sql` | 198 | 新增 requirements/work_items 组合索引，覆盖 project/status/sort 常用查询 |
| `project_management_service/backend/src/store/mongo.rs` | 322 | 新增 Mongo named indexes，与 SQLite 组合索引保持一致 |
| `project_management_service/backend/src/store/sqlite/tests/schema.rs` | 36 | 验证 SQLite migration 创建 plan snapshot 排序索引 |
| `project_management_service/backend/src/services/project_plan.rs` | 222 | 一次性读取 Project plan snapshot，并复用数据构建 dependency graph |
| `project_management_service/backend/src/api/plan.rs` | 42 | Project Management plan snapshot HTTP handler |
| `project_management_service/backend/src/api/router.rs` | 281 | 保留 Project Management 路由聚合、公共 auth/health/skill handler 和 protected API 装配 |
| `project_management_service/backend/src/api/router/mcp.rs` | 401 | 承接 `/mcp` JSON-RPC entrypoint、MCP server/tool info handler、Task Runner 内部 MCP header 鉴权和相关单测 |
| `project_management_service/backend/src/models.rs` | 12 | 保留 `crate::models::*` facade 和稳定 re-export 面 |
| `project_management_service/backend/src/models/common.rs` | 36 | 承接时间、字符串规范化、必填校验、通用响应和 `DbStatus` trait |
| `project_management_service/backend/src/models/auth.rs` | 53 | 承接用户角色、登录响应和 Agent token/account DTO |
| `project_management_service/backend/src/models/projects.rs` | 102 | 承接项目状态、项目记录和 profile 请求/响应 DTO |
| `project_management_service/backend/src/models/requirements.rs` | 170 | 承接需求状态、需求类型、需求记录、依赖和文档 DTO |
| `project_management_service/backend/src/models/work_items.rs` | 206 | 承接项目任务状态、Task Runner link/sync 和需求执行同步 DTO |
| `project_management_service/backend/src/models/graphs.rs` | 34 | 承接依赖图和 Task Runner execution option DTO |
| `project_management_service/backend/src/store/sqlite/requirements.rs` | 431 | 保留 SQLite requirements 列表、创建、更新、归档、删除、父子校验和子树执行保护 |
| `project_management_service/backend/src/store/sqlite/requirements/dependencies.rs` | 116 | 承接 SQLite requirement dependency 列表、写入、前置需求校验和循环依赖检测 |
| `project_management_service/backend/src/store/sqlite/requirements/documents.rs` | 118 | 承接 SQLite requirement technical overview document 读取和 upsert |
| `chat_app_server_rs/src/api/projects/plan_handlers.rs` | 59 | Chat Server Project Plan 从三次 PM HTTP 改为一次 PM plan snapshot |
| `chat_app_server_rs/src/api/projects/requirement_execution_handlers.rs` | 457 | 只保留 execute/stop HTTP 流程编排；启动和停止流程均复用 plan snapshot；纯解析、状态、消息同步和任务创建 helper 已下沉 |
| `chat_app_server_rs/src/api/projects/requirement_execution/context.rs` | 224 | 联系人 Task Runner runtime、执行会话和执行消息创建 |
| `chat_app_server_rs/src/api/projects/requirement_execution/plan.rs` | 282 | plan snapshot 解析、需求范围收集、前置校验和项目任务拓扑排序 |
| `chat_app_server_rs/src/api/projects/requirement_execution/status.rs` | 47 | Project work item 与 Task Runner 状态归一判断 |
| `chat_app_server_rs/src/api/projects/requirement_execution/sync.rs` | 199 | 执行关联读取、PM 状态同步和停止消息元数据更新 |
| `chat_app_server_rs/src/api/projects/requirement_execution/tasks.rs` | 361 | Task Runner 执行任务创建、外部前置执行任务解析和活跃执行校验 |
| `chat_app_server_rs/src/api/projects/requirement_execution/types.rs` | 48 | 需求执行内部 DTO |
| `chat_app_server_rs/src/api/projects/requirement_execution/values.rs` | 35 | JSON 字段和 tag 规范化 helper |
| `chat_app_server_rs/src/services/project_management_api_client.rs` | 312 | 新增 `get_project_service_plan`；移除已无调用方的 dependency graph、requirements、work_items 单独读取 client 方法 |
| `project_management_service/frontend/src/pages/ProjectDetailPage.tsx` | 390 | 保留数据查询、状态协调和 mutation；Tabs/Overlays/columns/renderers/styles 已拆出 |
| `project_management_service/frontend/src/pages/projectDetail/ProjectDetailTabs.tsx` | 326 | 承接页面 header、概览、项目详情、需求、项目任务和依赖图 tabs |
| `project_management_service/frontend/src/pages/projectDetail/ProjectDetailOverlays.tsx` | 354 | 承接新建、前置关系、技术文档和详情抽屉 |
| `project_management_service/frontend/src/pages/projectDetail/columns.tsx` | 180 | 承接需求表和项目任务表 columns |
| `project_management_service/frontend/src/pages/projectDetail/renderers.tsx` | 293 | 承接 Markdown 预览、详情预览和状态/依赖图标签渲染 |
| `chat_app/src/components/projectExplorer/ProjectPlanPane.tsx` | 487 | 保留 plan 读取、执行/停止和选中状态协调；模型派生与行级展示已拆出 |
| `chat_app/src/components/projectExplorer/projectPlanPane/model.ts` | 337 | 承接 dependency maps、需求列、任务排序、状态/优先级标签等纯函数 |
| `chat_app/src/components/projectExplorer/projectPlanPane/components.tsx` | 236 | 承接 header、banner、空状态、统计条、依赖行、任务行和 Markdown 内容块 |
| `chat_app/src/components/projectExplorer/ProjectRunSettingsPanel.tsx` | 479 | 保留运行状态、终端协调、保存/删除/重置动作和设置表单的主流程 |
| `chat_app/src/components/projectExplorer/projectRunSettingsPanel/model.ts` | 248 | 承接运行目标、工具链、配置文件提示和环境变量提示等纯函数 |
| `chat_app/src/components/projectExplorer/projectRunSettingsPanel/RunEnvironmentDetails.tsx` | 255 | 承接运行环境、工具链、注入环境变量和配置文件详情展示 |
| `chat_app_server_rs/src/api/configs/ai_model.rs` | 15 | 只保留子模块声明和对外 handler re-export |
| `chat_app_server_rs/src/api/configs/ai_model/config_handlers.rs` | 443 | 承接 AI model config CRUD、refresh 和 provider models HTTP 编排 |
| `chat_app_server_rs/src/api/configs/ai_model/model.rs` | 413 | 承接请求规范化、User Service DTO 映射、响应脱敏和本地模型配置构建测试 |
| `chat_app_server_rs/src/api/configs/ai_model/provider_handlers.rs` | 280 | 承接 AI model provider CRUD/refresh 的 User Service 代理 handler |
| `chat_app_server_rs/src/api/configs/ai_model/provider_models.rs` | 120 | 承接 provider `/models` 拉取、模型字段归一和 fallback model list |
| `chat_app_server_rs/src/api/configs/ai_model/settings_handlers.rs` | 123 | 承接 AI model settings 读取和更新 |
| `chat_app_server_rs/src/api/configs/ai_model/user_service_proxy.rs` | 35 | 承接 User Service base URL、timeout、access token 和错误状态映射 |
| `user_service/backend/src/api/models.rs` | 20 | 只保留子模块声明和对外 handler re-export |
| `user_service/backend/src/api/models/config_handlers.rs` | 248 | 承接 model config 列表、创建、读取、更新和删除 handler |
| `user_service/backend/src/api/models/config_refresh.rs` | 221 | 承接 model config 入口触发 provider 模型刷新和导入流程 |
| `user_service/backend/src/api/models/provider_handlers.rs` | 221 | 承接 model provider 列表、创建、读取、更新、刷新和删除 handler |
| `user_service/backend/src/api/models/provider_sync.rs` | 243 | 承接 provider refresh 后模型导入、过期模型清理和同步 Task Runner/Memory Engine |
| `user_service/backend/src/api/models/provider_fetch.rs` | 136 | 承接 provider `/models` HTTP 拉取、日志和响应解析 |
| `user_service/backend/src/api/models/normalization.rs` | 137 | 承接 provider/thinking/base_url/model id 等规范化 helper |
| `user_service/backend/src/api/models/model_values.rs` | 90 | 承接 model config/provider/settings 响应 JSON 映射和 secret 脱敏 |
| `user_service/backend/src/api/models/settings_handlers.rs` | 90 | 承接 model settings 读取和更新 |
| `user_service/backend/src/api/models/access.rs` | 65 | 承接目标用户解析、权限校验和 owner user 存在性校验 |
| `user_service/frontend/src/pages/ModelsPage.tsx` | 347 | 保留用户范围、模型 provider/config 查询、mutation、设置保存和页面级状态 |
| `user_service/frontend/src/pages/models/modelPageUtils.tsx` | 250 | 承接 provider form 类型、payload 归一、表格列定义和 owner/capability 展示 helper |
| `user_service/frontend/src/pages/models/ModelProviderDrawer.tsx` | 108 | 承接 provider 创建/编辑 Drawer 与表单字段 |
| `db_connection_hub/backend/src/drivers/sqlserver/metadata/detail.rs` | 512 | 保留 SQL Server object/index/trigger/detail 查询编排 |
| `db_connection_hub/backend/src/drivers/sqlserver/metadata/detail/nodes.rs` | 101 | 承接 SQL Server detail node-id 解析和解析单测 |
| `chat_app_server_rs/src/api/agent_chat.rs` | 135 | 保留 agent chat send/stop/reset 路由和流式用例入口 |
| `chat_app_server_rs/src/api/agent_chat/task_runner_callback.rs` | 519 | 承接 Task Runner callback HTTP 验证、消息读取、ask-user prompt 回调和项目执行状态同步 |
| `chat_app_server_rs/src/api/agent_chat/task_runner_callback/messages.rs` | 413 | 承接 callback user/assistant 消息构建、metadata 合并和实时事件载荷 |
| `chat_app_server_rs/src/api/agent_chat/task_runner_callback/messages/tests.rs` | 236 | 承接 callback message identity、metadata、terminal event 和 full-detail 单测 |
| `chat_app_server_rs/src/services/project_run/analyzer.rs` | 93 | 保留 Project Run catalog 分析入口、默认目标应用和对外 helper re-export |
| `chat_app_server_rs/src/services/project_run/analyzer/change_detection.rs` | 115 | 承接运行目标/环境相关路径变更分类 |
| `chat_app_server_rs/src/services/project_run/analyzer/target_model.rs` | 62 | 承接 target id、cwd 规范化、去重和 target 构建 |
| `chat_app_server_rs/src/services/project_run/analyzer/java.rs` | 388 | 承接 Maven/Gradle、Spring Boot、Java main class 和 Maven reactor 探测 |
| `chat_app_server_rs/src/services/project_run/analyzer/node.rs` | 79 | 承接 package.json scripts 和包管理器探测 |
| `chat_app_server_rs/src/services/project_run/analyzer/python.rs` | 74 | 承接 Python entrypoint 和 pytest 探测 |
| `chat_app_server_rs/src/services/project_run/analyzer/go.rs` | 119 | 承接 Go module、cmd entrypoint 和 root main 探测 |
| `chat_app_server_rs/src/services/project_run/analyzer/rust.rs` | 148 | 承接 Cargo target 和 bin entrypoint 探测 |
| `chat_app_server_rs/src/services/project_run/analyzer/scan.rs` | 258 | 承接目录扫描、默认优先级排序和 Maven reactor 单测 |
| `chat_app_server_rs/src/api/sessions/message_handlers.rs` | 278 | 保留 session message CRUD、turn display 与 runtime context HTTP handler 编排，并 re-export compact history handler |
| `chat_app_server_rs/src/api/sessions/message_handlers/compact.rs` | 513 | 承接 compact history 分页、Task Runner callback process message 补齐、user message turns fallback 与 handler 编排 |
| `chat_app_server_rs/src/api/sessions/message_handlers/compact_merge.rs` | 301 | 承接 project requirement execution 消息/turn item 补齐、排序和对应单测 |
| `crates/chatos_mcp_runtime/src/executor.rs` | 428 | 保留 `McpExecutor` 状态、初始化入口、工具列表访问、prompt 入口、并行判定和 gateway tool 输出 |
| `crates/chatos_mcp_runtime/src/executor/execution.rs` | 590 | 承接工具串行/并行执行、单次 tool call、参数解析、stream callback、结果收集和 HTTP header 注入 |
| `crates/chatos_mcp_runtime/src/executor/registration.rs` | 241 | 承接 HTTP/stdio/builtin 工具注册、public tool name 保留、legacy alias 和 unavailable server 记录 |
| `chat_app_server_rs/src/services/code_nav/languages/java/analysis.rs` | 559 | 保留 Java 文件分析、import 类型路径解析、搜索、声明分类、候选评分和路径扫描编排 |
| `chat_app_server_rs/src/services/code_nav/languages/java/analysis/syntax.rs` | 344 | 承接 Java 注解剥离、方法签名识别、字段识别、括号匹配和注释剥离 helper |
| `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs` | 480 | Java references 保留当前使用点，同时继续通过声明分类过滤 definition-only 结果 |
| `chat_app_server_rs/src/services/code_nav/languages/go/analysis.rs` | 450 | 保留 Go 文件分析、import 路径解析、全项目搜索、声明分类入口、候选评分和路径扫描编排 |
| `chat_app_server_rs/src/services/code_nav/languages/go/analysis/syntax.rs` | 201 | 承接 Go import 解析、类型/方法/函数/变量声明识别、短变量 fallback 分类和注释剥离 helper |
| `chat_app_server_rs/src/services/code_nav/languages/shared_nav.rs` | 583 | references 查询保留当前使用点，后续仍通过声明分类在存在 usage 时过滤 declaration-only 结果 |
| `chat_app_server_rs/src/services/user_service_api_client.rs` | 611 | 保留 User Service auth、agent account、model config/provider/settings 业务 API 函数和现有 client 行为测试 |
| `chat_app_server_rs/src/services/user_service_api_client/types.rs` | 188 | 承接 User Service auth、agent account、model config/provider/settings DTO 与请求 payload |
| `chat_app_server_rs/src/services/user_service_api_client/http.rs` | 111 | 承接 reqwest 请求构建、超时、bearer token 注入和远端错误信息解析 |
| `chat_app_server_rs/src/db/sqlite_schema.rs` | 500 | 保留 SQLite schema 执行顺序、兼容迁移、旧列补齐和 project-agent link 清理 |
| `chat_app_server_rs/src/db/sqlite_schema/statements.rs` | 386 | 承接 Chat Server SQLite 建表 SQL 和索引 SQL 常量，便于后续继续按域拆分 schema |
| `crates/chatos_ai_runtime/src/request.rs` | 387 | 保留 AI request handler、payload 选择、provider 判定、请求重试和发送编排 |
| `crates/chatos_ai_runtime/src/request/types.rs` | 37 | 承接 AI response、transport、stream callbacks 和 request options 公开类型 |
| `crates/chatos_ai_runtime/src/request/http.rs` | 66 | 承接 reqwest JSON 发送、abort token、identity encoding、payload 序列化/大小校验和日志预览 |
| `crates/chatos_ai_runtime/src/request/streaming.rs` | 142 | 承接 SSE stream 响应解析、最终 chunk/thinking callback 补发和 chat completions tool call 收集 |
| `crates/chatos_ai_runtime/src/request/tests.rs` | 249 | 承接 AI request payload、provider、stream callback 和 response-to-chat-message 行为测试 |
| `chat_app_server_rs/src/services/shared_builtin_browser_tools.rs` | 118 | 保留 `ChatosBrowserVisionAdapter` 注册入口、响应 metadata 构建和现有单测入口 |
| `chat_app_server_rs/src/services/shared_builtin_browser_tools/types.rs` | 49 | 承接 browser vision 运行上下文、候选模型、输出和运行结果内部类型 |
| `chat_app_server_rs/src/services/shared_builtin_browser_tools/support.rs` | 79 | 承接 prompt/unavailable 文案、文本规范化、model config JSON 映射和截图 data URL 构建 |
| `chat_app_server_rs/src/services/shared_builtin_browser_tools/context.rs` | 132 | 承接当前 session、选中模型配置和 contact agent system prompt 准备 |
| `chat_app_server_rs/src/services/shared_builtin_browser_tools/candidates.rs` | 298 | 承接 caller/session/user/default vision-capable model 候选选择和去重 |
| `chat_app_server_rs/src/services/shared_builtin_browser_tools/runner.rs` | 190 | 承接截图分析候选重试、Responses input 构建、AI runtime 调用和失败 attempts 汇总 |
| `chat_app_server_rs/src/services/task_runner_api_client.rs` | 473 | 保留 Task Runner token/skill/task/prompt/internal message task HTTP client 函数和共享请求 helper |
| `chat_app_server_rs/src/services/task_runner_api_client/types.rs` | 143 | 承接 Task Runner/User Service exchange DTO、task/prompt 请求响应 DTO 和 execution options 规则 |
| `chat_app_server_rs/src/services/task_runner_api_client/tests.rs` | 218 | 承接 Task Runner token exchange 与 skill profile 查询现有单测 |
| `chat_app_server_rs/src/api/sessions/history_process.rs` | 579 | 保留 compact history 入口、旧调用路径兼容包装和现有 history process 单测 |
| `chat_app_server_rs/src/api/sessions/history_process/turn_slices.rs` | 138 | 承接 memory engine turn slice compact、Task Runner plan summary/callback 恢复和 final assistant 判定 |
| `chat_app_server_rs/src/api/sessions/history_process/turn_display.rs` | 143 | 承接 turn_id 定位、turn process message 构建和单 turn display message 构建 |
| `task_runner_service/backend/src/services/chatos_message_tasks/queries.rs` | 348 | 保留 ChatOS message task 查询、source 过滤和 detail hydration helper |
| `task_runner_service/backend/src/services/chatos_message_tasks/queries/graph.rs` | 151 | 承接 ChatOS message task graph root/prerequisite 遍历、节点和边构建 |
| `task_runner_service/backend/src/services/chatos_message_tasks/queries/tests.rs` | 114 | 承接 ChatOS message graph 子任务过滤单测 |
| `task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs` | 438 | 保留运行准备主编排、run spec/runtime config/context snapshot 构建和现有 preparation 单测 |
| `task_runner_service/backend/src/services/run_model_phase/setup/preparation/mcp_inputs.rs` | 166 | 承接外部 MCP 配置加载、外部 MCP prefixed system input 和 Project Management skill input 注入 |
| `task_runner_service/backend/src/services/run_model_phase/setup/preparation/mcp_builder.rs` | 216 | 承接 builtin MCP server 构建、Project Management execution options enrich 和 builtin init warning 持久化 |
| `crates/chatos_ai_runtime/src/tool_runtime.rs` | 23 | 只保留 tool runtime 子模块声明和旧路径 re-export |
| `crates/chatos_ai_runtime/src/tool_runtime/execution_plan.rs` | 76 | 承接 tool call 去重、alias map 和 tool result alias 扩展 |
| `crates/chatos_ai_runtime/src/tool_runtime/budget.rs` | 125 | 承接 tool result 模型输入预算、环境变量默认值和 oversized advisory |
| `crates/chatos_ai_runtime/src/tool_runtime/items.rs` | 289 | 承接 Chat/Responses tool call/output item 构建、缺失 output 补齐和 turn item 合并 |
| `crates/chatos_ai_runtime/src/tool_runtime/tests.rs` | 314 | 承接 tool runtime 现有行为测试 |
| `chat_app_server_rs/src/services/code_nav/languages/rust/mod.rs` | 388 | 保留 Rust provider、definition/reference 编排、评分和现有 Rust code-nav 单测 |
| `chat_app_server_rs/src/services/code_nav/languages/rust/analysis.rs` | 321 | 承接 Rust 文件符号分析、声明分类、注释剥离和分析缓存解析 |
| `chat_app_server_rs/src/services/code_nav/languages/rust/search.rs` | 99 | 承接 Rust 文件遍历、忽略目录过滤和 token occurrence 搜索 |
| `chat_app_server_rs/src/services/agent_runtime/ai_request_handler/mod.rs` | 457 | 保留 AI request handler 主流程、payload 选择、请求预检、prompt-cache retry、发送和持久化编排 |
| `chat_app_server_rs/src/services/agent_runtime/ai_request_handler/http_client.rs` | 54 | 承接上游 HTTP client timeout 环境变量读取、clamp 和 reqwest client 构建 |
| `chat_app_server_rs/src/services/agent_runtime/ai_request_handler/payload.rs` | 90 | 承接 Responses/Chat Completions payload wrapper 和测试用 request payload helper |
| `chat_app_server_rs/src/services/agent_runtime/ai_request_handler/fingerprint.rs` | 66 | 承接 request input/tools hash、prefix hash 和 fingerprint 日志 |
| `crates/chatos_mcp_runtime/src/builtin_prompt.rs` | 497 | 保留 builtin prompt 公开 API、section 选择、effective prompt 和 runtime limitations 编排 |
| `crates/chatos_mcp_runtime/src/builtin_prompt/sections.rs` | 110 | 承接 prompt source path、include_str 源、section registry 缓存和 Markdown section 解析 |
| `crates/chatos_mcp_runtime/src/builtin_prompt/tests.rs` | 184 | 承接 builtin prompt 现有行为测试 |
| `crates/chatos_ai_runtime/src/traits.rs` | 13 | 只保留 traits 子模块声明和公开类型/trait re-export |
| `crates/chatos_ai_runtime/src/traits/model.rs` | 252 | 承接 `RuntimeMessage`、`ModelRuntimeConfig`、`ModelRequest` 和 `RuntimeCallbacks` |
| `crates/chatos_ai_runtime/src/traits/records.rs` | 349 | 承接 runtime record options、assistant/tool/user record input、metadata packing 和 `MemoryRecordWriter` |
| `crates/chatos_ai_runtime/src/traits/executor.rs` | 16 | 承接 `ToolExecutor` trait |
| `crates/chatos_ai_runtime/src/traits/tests.rs` | 135 | 承接 traits 现有行为测试 |
| `chat_app_server_rs/src/services/code_nav/symbol_index.rs` | 386 | 保留项目符号索引构建、缓存命中/失效、dirty path 增量重建和对外查询入口 |
| `chat_app_server_rs/src/services/code_nav/symbol_index/files.rs` | 91 | 承接符号索引文件 fingerprint、路径规范化、扩展名过滤、忽略目录过滤和行预览读取 |
| `chat_app_server_rs/src/services/code_nav/symbol_index/persistence.rs` | 107 | 承接符号索引本地缓存路径、持久化 DTO 和运行时索引/缓存 JSON 互转 |
| `chat_app_server_rs/src/services/code_nav/symbol_index/tests.rs` | 181 | 承接 symbol index 构建、缓存复用、snapshot 变化重建和 dirty path 失效单测 |
| `crates/chatos_builtin_tools/src/code_maintainer/patch.rs` | 129 | 保留 `apply_patch` 公开入口、写入/删除/移动执行流程和结果结构 |
| `crates/chatos_builtin_tools/src/code_maintainer/patch/parser.rs` | 222 | 承接标准 `*** Begin Patch` 格式与 loose replace 格式解析 |
| `crates/chatos_builtin_tools/src/code_maintainer/patch/hunks.rs` | 182 | 承接行切分、EOL 保持、hunk 匹配、增删改应用和 diff marker 容错 |
| `crates/chatos_builtin_tools/src/code_maintainer/patch/replacement.rs` | 58 | 承接唯一文本替换、LF/CRLF 候选构建和多匹配保护 |
| `crates/chatos_builtin_tools/src/code_maintainer/patch/tests.rs` | 151 | 承接 apply_patch 现有行为测试 |
| `task_runner_service/frontend/src/pages/ModelsPage.tsx` | 382 | 保留 model config 查询、过滤、mutation、路由和表单状态编排 |
| `task_runner_service/frontend/src/pages/models/ModelEditorDrawer.tsx` | 201 | 承接 model config 创建/编辑表单、catalog 预览状态展示和保存按钮 |
| `task_runner_service/frontend/src/pages/models/ModelDetailDrawer.tsx` | 234 | 承接 model detail 描述表、绑定任务列表、近期运行列表和详情操作按钮 |
| `task_runner_service/frontend/src/pages/models/ModelTestResultModal.tsx` | 81 | 承接模型连接测试结果展示和 usage JSON 预览 |
| `task_runner_service/frontend/src/pages/SettingsPage.tsx` | 224 | 保留设置页查询、mutation、路由跳转、tab 状态和 runtime form 数据回填 |
| `task_runner_service/frontend/src/pages/settings/SettingsSections.tsx` | 477 | 承接 overview、外部 skill、plan skill 和 internal prompts 的展示组件 |
| `task_runner_service/frontend/src/pages/settings/settingsPageUtils.ts` | 21 | 承接 Settings tab key、locale、runtime form 类型和格式化/error helper |
| `task_runner_service/frontend/src/pages/ToolingPage.tsx` | 247 | 保留工具页 notepad/terminal 查询、mutation、选中项、输入框和刷新/清空编排 |
| `task_runner_service/frontend/src/pages/tooling/ToolingPanels.tsx` | 214 | 承接 Notepad/Terminal 筛选区、统计条和列表表格展示 |
| `task_runner_service/frontend/src/pages/tooling/ToolingDrawers.tsx` | 267 | 承接 Notepad 详情抽屉、Terminal 日志抽屉、日志描述表和日志列表 |
| `task_runner_service/frontend/src/pages/tooling/toolingPageUtils.tsx` | 164 | 承接 Tooling 时间格式化、状态颜色、日志类型颜色、表格列和终端输入 payload 类型 |
| `task_runner_service/frontend/src/pages/TasksPage.tsx` | 636 | 保留任务列表、详情、运行、批量操作和抽屉/modal 编排；创建/编辑表单值映射与提交 payload 构建已下沉 |
| `task_runner_service/frontend/src/pages/tasks/taskPageUtils.tsx` | 493 | 承接任务表单初始值、编辑值、提交 payload、调度 payload 和任务页通用展示/远程操作 helper |
| `task_runner_service/frontend/src/pages/tasks/TaskDetailDrawer.tsx` | 375 | 保留任务详情抽屉入口、动作按钮、基础描述表、文本摘要和 outcome/input snapshot 编排 |
| `task_runner_service/frontend/src/pages/tasks/TaskDetailSections.tsx` | 453 | 承接远程操作、近期运行、相关 prompt、相关任务和共享文本 section 展示 |
| `chat_app/src/components/messageTasks/MessageTaskGraphPanel.tsx` | 342 | 保留图面板 loading/empty 状态、聚焦状态、画布尺寸、SVG marker/edge 渲染和节点定位 |
| `chat_app/src/components/messageTasks/MessageTaskGraphModel.ts` | 304 | 承接图节点/边类型、图标准化、上下游遍历、Flow node/edge 构建和 edge path 计算 |
| `chat_app/src/components/messageTasks/MessageTaskGraphNode.tsx` | 184 | 承接任务卡片 UI、关系标签、节点按钮事件隔离和运行态样式 |
| `chat_app/src/components/userMessages/ConversationProcessTimelineModal.tsx` | 114 | 保留执行过程 modal 外壳、loading/error/empty 状态、summary 和 timeline 布局 |
| `chat_app/src/components/userMessages/ConversationProcessTimelineModel.ts` | 382 | 承接过程消息识别、timeline item 构建、summary 统计、展示值解析和结果摘要 |
| `chat_app/src/components/userMessages/ConversationProcessTimelineCards.tsx` | 325 | 承接模型过程、工具调用、工具返回卡片、timeline dot、summary pill 和值展示 section |
| `task_runner_service/frontend/src/types.ts` | 11 | 保留旧导入路径的稳定 re-export 入口 |
| `task_runner_service/frontend/src/types/auth.ts` | 53 | 承接用户、登录、当前用户和用户创建/更新类型 |
| `task_runner_service/frontend/src/types/common.ts` | 7 | 承接通用分页响应类型 |
| `task_runner_service/frontend/src/types/tasks.ts` | 228 | 承接任务、任务配置、项目、批量操作和任务记忆 payload 类型 |
| `task_runner_service/frontend/src/types/runs.ts` | 58 | 承接任务运行、运行事件、运行摘要和运行列表过滤类型 |
| `task_runner_service/frontend/src/types/memory.ts` | 100 | 承接 Memory Engine thread/record、compose context 和任务记忆响应类型 |
| `task_runner_service/frontend/src/types/prompts.ts` | 56 | 承接 ask-user prompt 状态、记录、列表过滤和提交/取消 payload 类型 |
| `task_runner_service/frontend/src/types/models.ts` | 112 | 承接模型配置、运行时设置、provider catalog 和模型测试类型 |
| `task_runner_service/frontend/src/types/servers.ts` | 132 | 承接远程服务器和外部 MCP 配置类型 |
| `task_runner_service/frontend/src/types/mcp.ts` | 82 | 承接 MCP catalog、server info、内置 prompt 预览和 prompt build result 类型 |
| `task_runner_service/frontend/src/types/tooling.ts` | 120 | 承接 notepad 与 terminal tooling API 响应类型 |
| `task_runner_service/frontend/src/types/system.ts` | 29 | 承接 health 与 system config 响应类型 |
| `chat_app/src/components/MarkdownRenderer.css` | 4 | 保留原导入入口，并按既有级联顺序 import 子样式 |
| `chat_app/src/components/markdownRenderer/base.css` | 154 | 承接 Markdown 主题变量、基础容器、thinking 变体、标题、段落、链接和强调样式 |
| `chat_app/src/components/markdownRenderer/code.css` | 208 | 承接行内代码、代码块、代码块 header/action、展开遮罩和 streaming indicator 样式 |
| `chat_app/src/components/markdownRenderer/mermaid.css` | 266 | 承接 Mermaid block、preview overlay/dialog、状态提示和响应式样式 |
| `chat_app/src/components/markdownRenderer/content.css` | 266 | 承接引用、列表、表格、数学公式、光标、打印、动效降级、样式隔离和用户消息样式 |
| `chat_app/src/components/ToolCallRenderer.css` | 4 | 保留原导入入口，并按既有级联顺序 import 子样式 |
| `chat_app/src/components/toolCallRenderer/theme.css` | 149 | 承接 tool renderer 主题变量、工具族 accent token 和深色模式覆盖 |
| `chat_app/src/components/toolCallRenderer/chip.css` | 219 | 承接 tool chip、icon、badge、name、status、toggle 和展开图标样式 |
| `chat_app/src/components/toolCallRenderer/details.css` | 333 | 承接详情容器、section、summary/detail card、finding/source、代码和错误状态样式 |
| `chat_app/src/components/toolCallRenderer/layout.css` | 82 | 承接 footer、tree table 和移动端响应式样式 |
| `scripts/local-dev-stack.ps1` | 532 | 保留启动、停止、状态检查和进程/WSL/Mongo 操作函数 |
| `scripts/local-dev-stack/config.ps1` | 230 | 承接本地栈环境变量解析、Mongo 默认连接、服务定义和前端定义 |
| `chat_app_server_rs/src/modules/conversation_runtime/task_board.rs` | 520 | 保留 task board prompt、follow-up/review、runtime input 和 refresh 编排入口 |
| `chat_app_server_rs/src/modules/conversation_runtime/task_board/snapshot.rs` | 183 | 承接 turn runtime snapshot 查找、payload patch、task_board system message upsert 和相关单测 |
| `crates/chatos_ai_runtime/src/stream_parse.rs` | 362 | 保留 Responses/Chat Completions stream event 编排、state finalize 和原公开导出路径 |
| `crates/chatos_ai_runtime/src/stream_parse/text.rs` | 158 | 承接 stream text/reasoning delta 提取、嵌套文本扁平化和 trimmed 输出 helper |
| `crates/chatos_ai_runtime/src/stream_parse/tool_calls.rs` | 212 | 承接 Responses/Chat Completions tool call 收集、indexed tool call 合并和 response output 提取 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/execution_loop.rs` | 617 | 保留 AI request 主循环、恢复策略调用、工具执行生命周期和状态推进 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/execution_loop_follow_up.rs` | 83 | 承接 task follow-up/review metadata、turn phase event、async planner final summary prompt 和工具消息持久化策略 |
| `chat_app_server_rs/src/services/agent_builder.rs` | 594 | 保留 AI 创建 agent 入口、请求规范化、落库请求构建、payload 解析、策略归一和默认值推断 |
| `chat_app_server_rs/src/services/agent_builder/prompt.rs` | 106 | 承接 agent builder system/user prompt 构造，以及可见 skills、plugins、reference agents 索引 JSON 映射 |
| `chat_app_server_rs/src/api/agents.rs` | 426 | 保留 agent CRUD、runtime context、sessions、AI create、权限校验和路由兼容别名 |
| `chat_app_server_rs/src/api/agents/skills.rs` | 275 | 承接 skills/plugins 列表、详情、Git 导入和插件安装 HTTP handler |
| `chat_app_server_rs/src/services/chatos_agents.rs` | 365 | 保留 agent 列表、读取、创建、更新、删除、session 列表和 payload 规范化主流程 |
| `chat_app_server_rs/src/services/chatos_agents/runtime.rs` | 178 | 承接 runtime context DTO 组装、插件摘要、命令去重和 runtime skill 映射 |
| `chat_app_server_rs/src/services/chatos_agents/provisioning.rs` | 97 | 承接 Task Runner agent account 创建、已存在账号兜底查找和 username/password 生成 |
| `chat_app_server_rs/src/services/chatos_skills.rs` | 462 | 保留 skills/plugins 列表、详情、Git import 编排和插件安装主流程 |
| `chat_app_server_rs/src/services/chatos_skills_import.rs` | 187 | 承接 marketplace.json 解析、默认 marketplace 文件查找和 plugins 目录 fallback candidate 发现 |
| `chat_app_server_rs/src/api/memory_compat.rs` | 458 | 保留 Memory compat 路由注册和 session/message/summary/runtime snapshot HTTP handler |
| `chat_app_server_rs/src/api/memory_compat/contracts.rs` | 87 | 承接 Memory compat query/body DTO |
| `chat_app_server_rs/src/api/memory_compat/support.rs` | 117 | 承接 Memory compat message input 映射、scope user 解析和错误响应映射 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests.rs` | 21 | 保留 AI client 测试 facade，按上下文、follow-up、HTTP 恢复、retry、tool recovery 和 transport 域导入子测试 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/context.rs` | 284 | 承接 stateless/context/runtime guidance 相关 AI client 测试 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/follow_up.rs` | 257 | 承接 task follow-up/review 和 async planner 行为测试 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/recovery_http.rs` | 181 | 承接 HTTP/provider overload/rate limit 恢复测试 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/recovery_retry.rs` | 258 | 承接 stream parse、empty response 和 network retry 测试 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/recovery_tools.rs` | 663 | 承接 tool call output 缺失、stateless fallback 和 pending tool items 合并测试 |
| `chat_app_server_rs/src/services/agent_runtime/ai_client/tests/transport.rs` | 189 | 承接 Responses/Chat Completions transport 切换测试 |
| `task_runner_service/backend/src/mcp_server/tests.rs` | 192 | 保留 MCP server 测试 facade 和共享 helper |
| `task_runner_service/backend/src/mcp_server/tests/schema.rs` | 503 | 承接 MCP tool schema、async planner、model list scope 和 context 归一测试 |
| `task_runner_service/backend/src/mcp_server/tests/plan_profile.rs` | 653 | 承接 ChatOS plan profile、任务范围、前置任务和 async reuse 测试 |
| `chat_app_server_rs/src/services/ai_common/tests.rs` | 14 | 保留 AI common 测试 facade |
| `chat_app_server_rs/src/services/ai_common/tests/metadata.rs` | 340 | 承接 metadata、error response、abort token 和消息持久化测试 |
| `chat_app_server_rs/src/services/ai_common/tests/tools.rs` | 319 | 承接 tool result metadata、aborted result 和 tool lifecycle 测试 |
| `chat_app_server_rs/src/services/ai_common/tests/stream.rs` | 199 | 承接 SSE stream、abort 和 stream callback 测试 |
| `chat_app/src/components/ToolCallRenderer.test.tsx` | 6 | 保留 jsdom test facade 和按域导入的测试入口 |
| `chat_app/src/components/ToolCallRenderer.test/helpers.tsx` | 55 | 承接 I18n render、auth store reset、LazyMarkdown mock 和 tool call fixture |
| `chat_app/src/components/ToolCallRenderer.test/summaries.tsx` | 84 | 承接 extract summary 与 structured_result metadata 优先级测试 |
| `chat_app/src/components/ToolCallRenderer.test/codeMaintainer.tsx` | 236 | 承接 Code Maintainer 工具名归一、文件卡片、search/list/apply_patch 和 truncated JSON 恢复测试 |
| `chat_app/src/components/ToolCallRenderer.test/builtinTools.tsx` | 290 | 承接 terminal、remote、task manager、agent builder、memory、notepad 和 process 卡片测试 |
| `chat_app/src/components/ToolCallRenderer.test/browserResearch.tsx` | 371 | 承接 browser console/vision/inspect/research 与 web research 卡片测试 |
| `chat_app/src/lib/store/actions/sessions.selectSession.test.ts` | 14 | 保留 selectSession 测试 facade，并在入口 hoist fetch mocks |
| `chat_app/src/lib/store/actions/sessions.selectSession.test/testUtils.ts` | 103 | 承接 session/message fixture、cache helper re-export 和 background sync spy |
| `chat_app/src/lib/store/actions/sessions.selectSession.test/realtimeSync.ts` | 145 | 承接 realtime 连接状态下 compact-history background sync 行为测试 |
| `chat_app/src/lib/store/actions/sessions.selectSession.test/selectionFlow.ts` | 262 | 承接选择请求竞态、切换面板和 uncached session 清空测试 |
| `chat_app/src/lib/store/actions/sessions.selectSession.test/cache.ts` | 227 | 承接可见快照回写、缓存复用和 force refresh 测试 |
| `chat_app/src/lib/store/actions/sessions.selectSession.test/pagination.ts` | 322 | 承接 compact-history page trim、LRU touch、stale cache 和 initial page size 测试 |
| `chat_app_server_rs/src/core/chat_runtime_contact/prompt_builder.rs` | 612 | 保留 contact system prompt 主入口、Disabled/Summary/SelectedFull 分支和 reader tool 提示 |
| `chat_app_server_rs/src/core/chat_runtime_contact/prompt_builder/command.rs` | 68 | 承接 test-only contact command system prompt 构造 |
| `chat_app_server_rs/src/core/chat_runtime_contact/prompt_builder/locale.rs` | 35 | 承接 contact prompt 本地化 text、text_ref 和 field helper |
| `chat_app_server_rs/src/api/message_task_runner/graph.rs` | 364 | 保留 message task graph 节点补齐、边归一、depth 计算和 payload 更新逻辑 |
| `chat_app_server_rs/src/api/message_task_runner/graph/tests.rs` | 312 | 承接 graph edge normalize、missing prerequisite node 和 subtask 过滤单测 |
| `task_runner_service/frontend/src/api/client.ts` | 576 | 保留 Task Runner frontend 的 `api` endpoint 聚合对象和旧 auth/url re-export |
| `task_runner_service/frontend/src/api/http.ts` | 98 | 承接 API base URL 归一、auth token 存储、EventSource URL、fetch request 和 query string helper |

## 决策记录

- 2026-06-27：新增热点文件先进入 warning 模式，不立即失败 CI。原因是当前仓库已有多个历史大文件，先建立可见基线，再逐步降低预算。
- 2026-06-27：Project Management MCP contract 只抽稳定协议常量、tool name、wire args 和 schema builder，不让共享 crate 依赖 PM 服务内部模型。
- 2026-06-27：Phase 3 首步选择抽纯领域函数和服务编排，再拆 API/MCP/store 文件，避免只做机械文件切分。
- 2026-06-27：API handler 按业务边界拆分，router 只保留路由声明、认证、MCP entrypoint 和 header 测试。
- 2026-06-27：Sync callback 拆成 HTTP 层 `api/sync.rs` 和业务编排层 `services/execution_sync.rs`。
- 2026-06-27：Dependency graph 数据读取编排进入 `services/dependency_graph.rs`，API 与 MCP 复用。
- 2026-06-27：MCP JSON-RPC 协议入口与 tool call 分发拆开，`mcp_server.rs` 保留协议 envelope，`mcp_tools.rs` 承接具体 PM 操作。
- 2026-06-27：SQLite/Mongo store 拆分时保留 `AppStore` 和 store 对外方法不变，通过同一模块下的子模块拆 aggregate；SQLite 测试同步拆分，避免把热点从生产代码移动到单个测试文件。
- 2026-06-27：Plan snapshot API 先保持现有前端响应兼容，同时输出 snake_case 与 camelCase 字段；PM 内部仍复用现有 store/domain graph 规则，避免改动可见性和归档过滤行为。
- 2026-06-27：组合索引按现有查询实际排序补充，不删除旧基础索引；SQLite 通过 idempotent `CREATE INDEX IF NOT EXISTS` 覆盖旧库，Mongo 通过 named index 在启动时确保存在。
- 2026-06-27：Chat Server 需求执行 handler 拆分优先移动无 I/O 解析和可独立复用的 helper，再移动会话、同步和 Task Runner 编排；HTTP 路由和响应结构保持不变，先把热点文件降到预算线以下。
- 2026-06-27：Chat Server 需求执行停止流程也改为复用 plan snapshot；旧 requirements/work_items 单独读取 client 已无调用方，直接删除以降低 API client 表面积和 dead_code warning。
- 2026-06-27：Project Management 前端详情页拆分时保留 React Query、mutation 和页面级状态在 `ProjectDetailPage.tsx`，把无副作用展示层、表格列、弹窗抽屉、样式和纯工具函数移到 `projectDetail/` 子模块，避免把数据流分散到多个组件里。
- 2026-06-27：Chat 前端 `ProjectPlanPane` 拆分先保留 `loadPlan/execute/stop` 状态流在主组件，只抽 dependency map、需求列、任务排序和展示型子组件，避免改变执行入口和会话跳转逻辑。
- 2026-06-27：Chat 前端 `ProjectRunSettingsPanel` 拆分先抽纯模型提示函数和运行环境详情块，主组件继续持有运行状态、保存动作和终端协调，避免把副作用分散到展示组件里。
- 2026-06-27：Chat Server `ai_model` 拆分按 handler 类型、纯模型映射、provider 模型拉取和 User Service 代理支撑函数分层；路由路径和响应 JSON 字段保持不变，只调整内部模块边界。
- 2026-06-27：User Service model API 拆分保留 `api/models.rs` 作为路由入口，按 config/provider/settings handler、provider refresh 编排、provider fetch、权限、规范化和响应映射分层；Mongo store 与跨服务同步调用保持原行为。
- 2026-06-27：User Service 前端 `ModelsPage` 只下沉 provider Drawer、表格列和 payload 归一；页面继续持有 React Query、mutation、用户范围、设置保存和刷新流程，避免把跨列表副作用下沉到展示组件。
- 2026-06-27：DB Hub SQL Server metadata detail 首轮只下沉 node-id parser 和 parser 单测到 `detail/nodes.rs`；实际数据库查询 SQL、连接错误映射和 ObjectDetailResponse 组装保留在 `detail.rs`，降低数据库行为漂移风险。
- 2026-06-27：Chat Server `agent_chat` 先按 Task Runner callback 边界拆分；后续把 callback message 内联测试继续下沉到 `messages/tests.rs`，让 `messages.rs` 只保留 user/assistant 消息构建、metadata 合并和实时事件载荷。
- 2026-06-27：Project Run analyzer 按语言探测和扫描编排拆分；`analyzer.rs` 继续作为 `project_run` 模块对外入口，Go/Rust entrypoint helper 通过 re-export 保持 `environment_validation` 调用不变。
- 2026-06-27：Chat Server session `message_handlers` 优先按 compact history/user message turns 边界拆分；主 handler 保留普通消息、turn display 和 runtime context 编排，project requirement execution 补齐逻辑独立为 `compact_merge.rs`，避免分页 handler 与补齐规则继续耦合。
- 2026-06-27：MCP runtime executor 按注册和执行两个稳定边界拆分；`executor.rs` 继续保留 public API 和状态结构，注册侧负责 tools/list 与 alias，执行侧负责参数解析、串并行调度和结果回调，避免改变调用方依赖的 `McpExecutor` 方法形态。
- 2026-06-27：Java code-nav analysis 只下沉纯语法 helper 到 `analysis/syntax.rs`，保留 `analysis.rs` 的对外函数和调用路径；同时将 Java references 调整为保留当前使用点，满足现有 Java 引用测试对“有 usage 时过滤 declaration”的语义。
- 2026-06-27：Go code-nav analysis 对齐 Java 拆分风格，只下沉 import/声明识别、短变量 fallback 和注释剥离等纯语法 helper；`analysis.rs` 继续持有模块路径解析、搜索、声明分类入口和 definition/reference 打分。顺手修正共享 references 选择器，让当前位置作为 usage 保留，跨语言同名用例保持一致。
- 2026-06-27：Chat Server User Service API client 先抽 DTO 与 HTTP helper，不拆公开业务函数；调用方继续通过 `user_service_api_client::...` 使用同一组函数和类型，避免把跨服务契约泄漏到多个 handler。
- 2026-06-27：Chat Server SQLite schema 首轮只抽建表和索引 SQL 常量，保留 `create_tables_sqlite` 中兼容迁移的执行顺序；由于父模块使用 `#[path = "sqlite_schema.rs"]`，子模块显式指定 `sqlite_schema/statements.rs` 路径。
- 2026-06-27：ChatOS AI runtime request 按公开类型、HTTP 发送和 stream 解析拆分；`request.rs` 继续保留 handler 流程编排和 payload/retry 选择，避免改变 `chatos_ai_runtime::{AiRequestHandler,AiRequestOptions,AiResponse,AiTransport,StreamCallbacks}` 的导出面。
- 2026-06-27：ChatOS AI runtime request 继续把内联测试下沉到 `request/tests.rs`；生产入口和私有 helper 名称保持不变，只降低主文件阅读负担。
- 2026-06-27：Chat Server browser vision 工具按 adapter 入口、上下文准备、候选模型选择、运行调用和共享类型拆分；保留 `ChatosBrowserVisionAdapter` 对 `chatos_builtin_tools` 的接入形态，避免影响内置 Browser Tools 注册路径。
- 2026-06-27：Chat Server Task Runner API client 先抽 DTO/execution options 与现有单测，保留 HTTP 函数在入口文件；`TaskRunnerMcpConfigRequest` 作为 `TaskRunnerExecutionOptions::mcp_config_for_tool_ids` 返回类型继续通过 `task_runner_api_client` re-export 暴露。
- 2026-06-27：Chat Server history process 按 memory engine turn slice compact 和 turn display 两个业务边界拆分；`history_process.rs` 保留历史 compact 入口和旧路径包装，避免修改 `history.rs`、`history_compact.rs` 与 message handler 调用方。
- 2026-06-27：Task Runner ChatOS message task `queries.rs` 只下沉 graph 构建和 graph 单测；source 查询、detail hydration、model/run/task summary 读取 helper 仍保留在父模块，避免把查询路径与图遍历同时打散。
- 2026-06-27：Task Runner run preparation 按 MCP prompt/input 准备和 builtin MCP builder/PM schema enrichment 拆分；`preparation.rs` 保留准备阶段主流程、runtime config、run spec 和 context snapshot，避免把 run phase 编排分散到多个入口。
- 2026-06-27：ChatOS AI runtime `tool_runtime` 按 execution plan、tool result budget 和 tool item 构建三类稳定职责拆分；入口文件继续 re-export 原公开函数和类型，避免影响 `runtime`、`memory_context`、Chat Server 和 Task Runner 的旧调用路径。
- 2026-06-27：Rust code-nav 按文件分析/声明分类和全项目搜索拆分；`mod.rs` 保留 provider 与导航编排，避免和 Java code-nav 已拆出的 `analysis/syntax.rs` 风格偏离过大。
- 2026-06-27：MCP runtime `builtin_prompt` 按 prompt source section registry 和测试拆分；公开 API 保持在原文件，`include_str!` 在子模块里调整为多退一级，避免路径变化导致编译失败。
- 2026-06-27：ChatOS AI runtime `traits` 按模型请求类型、记录持久化类型和工具执行 trait 拆分；`traits.rs` 只做 re-export，保持 crate root `pub use traits::{...}` 的导出面不变。
- 2026-06-27：Chat Server code-nav `symbol_index` 按文件扫描/fingerprint、持久化 DTO/cache JSON 和测试拆分；同时修复 dirty path 重建分支在持有 `DashMap::get` guard 时再次 `insert` 同一个 map 的潜在死锁，改为先 clone cache entry 再释放 guard。
- 2026-06-27：Code Maintainer `apply_patch` 按 parser、hunk application、replacement 和测试拆分；`registration_write.rs` 仍只依赖 `patch::apply_patch`，工具名、输入 schema、返回 JSON 和 change log 记录路径保持不变。
- 2026-06-27：Task Runner 前端 `ModelsPage` 拆分只移动展示型 Drawer/Modal，页面继续持有 React Query、mutation、route search params、catalog preview 和表单脏状态，避免把数据流副作用下沉到展示组件。
- 2026-06-27：Task Runner 前端 `TasksPage` 首轮只抽创建/编辑表单值映射到 `taskPageUtils`，不移动 mutation、route search params、drawer/modal open state 和批量操作状态，避免打散页面级工作流。
- 2026-06-27：Task Runner 前端 `TaskDetailDrawer` 按详情 section 拆分展示组件；主 Drawer 继续持有 task、loading、回调和详情主布局，远程操作/近期运行/相关 prompt/相关任务只作为纯展示 section 接收数据和动作回调。
- 2026-06-27：Task Runner 前端 `ToolingPage` 按 notepad/terminal 面板、详情抽屉和列/格式化 helper 拆分；页面继续持有 React Query、mutation、选中项、输入框和刷新/清空流程，避免把工具操作副作用下沉到展示组件。
- 2026-06-27：Chat 前端 `MessageTaskGraphPanel` 按图模型和节点卡片拆分；原文件继续 re-export `normalizeMessageTaskGraph*`，保持现有测试导入路径和调用方 `MessageTaskGraphPanel` 入口不变。
- 2026-06-27：Chat 前端 `ConversationProcessTimelineModal` 按过程数据模型和 timeline 卡片拆分；modal 只负责 overlay、状态分支和布局，工具调用解析、结果展示和卡片展开状态下沉到子模块。
- 2026-06-27：Task Runner 前端 `types.ts` 保留为稳定 re-export facade；协议类型按 auth/common/tasks/runs/memory/prompts/models/servers/mcp/tooling/system 域拆分，现有 `../types` 导入路径不变。
- 2026-06-27：Chat 前端 `MarkdownRenderer.css` 按基础、代码块、Mermaid 和内容/表格四个 CSS 域拆分；原文件只保留 `@import`，维持现有 `MarkdownRenderer.tsx` 导入路径和 CSS 级联顺序。
- 2026-06-27：Chat 前端 `ToolCallRenderer.css` 按 theme、chip、details 和 layout 四个 CSS 域拆分；原文件只保留 `@import`，维持现有 `ToolCallRenderer.tsx` 导入路径和 CSS 级联顺序。
- 2026-06-27：Windows 本地栈脚本只把环境解析和服务定义拆到 dot-sourced `config.ps1`；主入口、`-Action` 参数、默认 Mongo 连接和现有启动/停止流程保持不变。
- 2026-06-27：Chat Server Task Board 只下沉 runtime snapshot patch/upsert 到 `task_board/snapshot.rs`；`task_board.rs` 继续作为对外入口，保留 AI execution loop、chat stream 和 shared task manager 的旧调用路径。
- 2026-06-27：ChatOS AI runtime `stream_parse` 按 text/reasoning extraction 和 tool call merge 两个私有 helper 域拆分；`stream_parse.rs` 继续暴露 `StreamState`、apply/finalize 函数和 `extract_responses_tool_calls` 等旧 API。
- 2026-06-27：Chat Server `ai_request_handler` 按 HTTP timeout/client、payload wrapper 和 request fingerprint 三个纯 helper 域拆分；`mod.rs` 继续持有请求主流程、prompt-cache retry、shared runtime 发送和消息持久化，避免打散调用链。
- 2026-06-27：Chat Server AI client execution loop 首轮只下沉 task follow-up/review helper、turn phase event 和 async planner final-summary prompt；主循环继续保留请求重试、工具执行、恢复策略和 context 推进，避免把异步状态机拆散。
- 2026-06-27：Chat Server `agent_builder` 先抽 LLM prompt/index 构造到 `prompt.rs`；`agent_builder.rs` 继续持有 AI 创建 agent 的外部入口、payload 解析和 CreateChatosAgentRequest 合成，避免 prompt 展示结构和落库策略相互耦合。
- 2026-06-27：Chat Server `agents` API 按 agent 与 skills/plugins 两条 HTTP 资源边界拆分；父模块继续持有 router 和 agent 权限校验，`skills.rs` 复用父模块 scope user 解析，保持 `/api/skills*` 路径和兼容别名行为不变。
- 2026-06-27：Chat Server `chatos_agents` 按 runtime context 组装和 Task Runner account provisioning 两个边界拆分；CRUD、payload 规范化、inline skill 校验和 session 列表继续留在父模块，避免把 agent 生命周期主流程拆散。
- 2026-06-27：Chat Server `chatos_skills` 只下沉 Git import candidate 发现，包括 marketplace 解析和 plugins fallback；list/import/install 的服务编排、仓库写入和 DTO 映射继续保留在父模块。
- 2026-06-27：Chat Server `memory_compat` 按 DTO 和通用 support helper 拆分；父模块继续保留全部 `/api/memory/v1/*` 路由和 handler，避免兼容 API 的可见行为漂移。
- 2026-06-27：Chat Server contact prompt builder 首轮只抽 test-only command prompt 和本地化 helper；`compose_contact_system_prompt` 主入口及 Summary/SelectedFull 状态分支留在原文件，降低联系人提示词行为漂移风险。
- 2026-06-27：Chat Server message task graph 已是纯图标准化实现，首轮只把内联测试移到 `graph/tests.rs`；生产函数和私有 helper 名称保持不变，避免影响 `message_task_runner.rs` 调用。
- 2026-06-27：Task Runner 前端 `TasksPage` 继续只抽无副作用 payload builder，页面仍持有 mutation、路由参数和 drawer/modal 状态，避免把页面级提交流程拆散到工具函数里。
- 2026-06-27：Task Runner 前端 `SettingsPage` 只下沉无副作用展示 tab 和格式化 helper；页面继续持有 React Query、mutation、路由跳转、表单实例和 active tab 状态，避免把数据副作用散到展示组件。
- 2026-06-27：Task Runner 前端 API client 首轮只抽 HTTP/auth/query 基础层到 `api/http.ts`；`api/client.ts` 继续 re-export 旧 auth/url 函数并保留 `api` endpoint 聚合对象，避免改动调用方导入路径。
- 2026-06-27：Project Management `api/router.rs` 只下沉 MCP entrypoint 和 header 鉴权 helper 到 `router/mcp.rs`；普通 API route 装配、auth middleware、health/login 和 skill handler 保留在父 router，避免把路由注册分散。
- 2026-06-27：Project Management `models.rs` 保留 `crate::models::*` facade，DTO 按 auth/project/requirement/work_item/graph/common 分域下沉；状态枚举继续实现共享 `DbStatus` trait，避免影响 store、API、MCP 和服务层现有导入路径。
- 2026-06-27：Project Management SQLite requirements store 先按依赖关系和技术文档两个边界拆分；父文件继续保留 requirement CRUD、归档/删除事务和子树执行保护，避免把生命周期主流程拆散。
- 2026-06-28：剩余硬热点优先拆测试文件而不改变生产导出面；每个原测试文件保留 facade，公共 fixture/mock 下沉到同级子目录，避免改动现有 test runner 入口。
- 2026-06-28：`ToolCallRenderer.test` 的 jsdom 环境和 `sessions.selectSession.test` 的 Vitest mock 必须留在 facade 入口。原因是 Vitest hoist 只对当前测试入口稳定生效，放到 helper 后再由子模块静态导入会导致 mock 注册晚于被测模块加载。
- 2026-06-28：i18n 文案文件继续作为 planned warning，不做资源文件拆分。当前默认硬预算已通过，后续只有在需要治理翻译资源维护方式时再拆 key/domain。

## 本轮验证记录

```powershell
cargo fmt
cargo fmt --check
cargo check -p chatos_mcp_runtime
cargo test -p chatos_mcp_runtime executor --lib
cargo test -p chatos_mcp_runtime builtin_prompt --lib
cargo check -p chat_app_server_rs
cargo test -p chat_app_server_rs execution_loop_follow_up --lib
cargo test -p chat_app_server_rs agents --lib
cargo test -p chat_app_server_rs chat_runtime --lib
cargo test -p chat_app_server_rs normalize_graph --lib
cargo test -p chat_app_server_rs requirement_execution --lib
cargo test -p chat_app_server_rs ai_model --lib
cargo test -p chat_app_server_rs task_runner_callback --lib
cargo test -p chat_app_server_rs detects_maven_reactor_spring_boot_modules --lib
cargo test -p chat_app_server_rs compact_history --lib
cargo test -p chat_app_server_rs history_process --lib
cargo test -p chat_app_server_rs user_message_turn --lib
cargo test -p chat_app_server_rs java_ --lib
cargo test -p chat_app_server_rs go_ --lib
cargo test -p chat_app_server_rs references_skip_definition_when_usage_exists --lib
cargo test -p chat_app_server_rs user_service_api_client --lib
cargo test -p chat_app_server_rs sqlite --lib
cargo test -p chat_app_server_rs browser_vision --lib
cargo test -p chat_app_server_rs caller_runtime --lib
cargo test -p chat_app_server_rs task_runner_skill --lib
cargo test -p chat_app_server_rs skill --lib
cargo test -p chat_app_server_rs memory_compat --lib
cargo test -p chat_app_server_rs exchange_task_runner_token --lib
cargo test -p chat_app_server_rs symbol_index --lib
cargo test -p chat_app_server_rs task_board --lib
cargo test -p chat_app_server_rs ai_request_handler --lib
$env:MEMORY_ENGINE_OPERATOR_TOKEN='chatos-memory-engine-dev-operator-token'; cargo test -p chat_app_server_rs ai_client --lib
cargo test -p chat_app_server_rs ai_common --lib
cargo test -p chatos_builtin_tools apply_patch --lib
cargo check -p chatos_ai_runtime
cargo test -p chatos_ai_runtime request --lib
cargo test -p chatos_ai_runtime tool_runtime --lib
cargo test -p chatos_ai_runtime traits --lib
cargo check -p task_runner_service_backend
cargo test -p task_runner_service_backend chatos_message_graph --lib
cargo test -p task_runner_service_backend preparation --lib
cargo test -p task_runner_service_backend mcp_ --lib
cargo test -p chat_app_server_rs rust_ --lib
cargo check -p project_management_service_backend
cargo test -p project_management_service_backend --lib mcp_
cargo check -p project_management_service_backend -p chat_app_server_rs -p task_runner_service_backend
cd user_service/backend; cargo check
cd db_connection_hub/backend; cargo check
cd db_connection_hub/backend; cargo test sqlserver --lib
cargo test -p project_management_service_backend --lib migrations_create_plan_snapshot_sort_indexes
cargo test -p project_management_service_backend --lib project_plan
npm --prefix project_management_service/frontend run type-check
npm --prefix project_management_service/frontend run build
npm --prefix user_service/frontend run type-check
npm --prefix user_service/frontend run build
npm --prefix task_runner_service/frontend run type-check
npm --prefix task_runner_service/frontend run build
npm --prefix chat_app run type-check
npm --prefix chat_app test -- --run MessageTaskGraphPanel.test.ts
npm --prefix chat_app test -- --run ToolCallRenderer.test.tsx
npm --prefix chat_app test -- --run sessions.selectSession.test.ts
npm --prefix chat_app run build
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action status
bash scripts/check-hotspot-line-budgets.sh
bash scripts/check-hotspot-line-budgets.sh --warn-planned
bash scripts/code-size-report.sh --top 40
git diff --check
```

结果：

| 命令 | 结果 |
| --- | --- |
| `cargo fmt` | 通过 |
| `cargo fmt --check` | 通过 |
| `cargo check -p chatos_mcp_runtime` | 通过 |
| `cargo test -p chatos_mcp_runtime executor --lib` | 通过，4 个相关测试通过，25 个测试被筛选 |
| `cargo test -p chatos_mcp_runtime builtin_prompt --lib` | 通过，10 个相关测试通过，19 个测试被筛选 |
| `cargo check -p chat_app_server_rs` | 通过；仍有历史 dead_code warning |
| `cargo test -p chat_app_server_rs execution_loop_follow_up --lib` | 通过，2 个相关测试通过，452 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs agents --lib` | 通过，0 个测试匹配，454 个测试被筛选；test profile 编译和测试二进制执行正常，仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs chat_runtime --lib` | 通过，8 个相关测试通过，446 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs normalize_graph --lib` | 通过，4 个相关测试通过，450 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs requirement_execution --lib` | 通过，2 个相关测试通过，452 个测试被筛选 |
| `cargo test -p chat_app_server_rs ai_model --lib` | 通过，12 个相关测试通过，442 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs task_runner_callback --lib` | 通过，11 个相关测试通过，443 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs detects_maven_reactor_spring_boot_modules --lib` | 通过，1 个相关测试通过，453 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs compact_history --lib` | 通过，11 个相关测试通过，443 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs history_process --lib` | 通过，10 个相关测试通过，444 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs user_message_turn --lib` | 通过，1 个相关测试通过，453 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs java_ --lib` | 通过，6 个相关测试通过，448 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs go_ --lib` | 通过，10 个匹配测试通过，444 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs references_skip_definition_when_usage_exists --lib` | 通过，4 个跨语言 references 过滤用例通过，450 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs user_service_api_client --lib` | 通过，5 个相关测试通过，449 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs sqlite --lib` | 通过，2 个现有 SQLite 相关测试通过，452 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs browser_vision --lib` | 通过，1 个相关测试通过，453 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs caller_runtime --lib` | 通过，2 个相关测试通过，452 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs task_runner_skill --lib` | 通过，2 个相关测试通过，452 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs skill --lib` | 通过，3 个相关测试通过，451 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs memory_compat --lib` | 通过，0 个测试匹配，454 个测试被筛选；test profile 编译和测试二进制执行正常 |
| `cargo test -p chat_app_server_rs exchange_task_runner_token --lib` | 通过，2 个相关测试通过，452 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs symbol_index --lib` | 通过，4 个相关测试通过，450 个测试被筛选；之前卡住的 dirty path 失效用例已正常退出 |
| `cargo test -p chat_app_server_rs task_board --lib` | 通过，21 个相关测试通过，433 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs ai_request_handler --lib` | 通过，29 个相关测试通过，425 个测试被筛选；仍有历史 unused/dead_code warning |
| `$env:MEMORY_ENGINE_OPERATOR_TOKEN='chatos-memory-engine-dev-operator-token'; cargo test -p chat_app_server_rs ai_client --lib` | 通过，39 个相关测试通过，415 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chat_app_server_rs ai_common --lib` | 通过，33 个相关测试通过，421 个测试被筛选；仍有历史 unused/dead_code warning |
| `cargo test -p chatos_builtin_tools apply_patch --lib` | 未执行成功；依赖 `markup5ever` build script 被 Windows 应用控制策略拦截，`os error 4551` |
| `cargo check -p chatos_ai_runtime` | 通过 |
| `cargo test -p chatos_ai_runtime request --lib` | 历史通过；本轮复跑测试 exe 被 Windows 应用控制策略拦截，`os error 4551` |
| `cargo test -p chatos_ai_runtime tool_runtime --lib` | 通过，12 个相关测试通过，112 个测试被筛选 |
| `cargo test -p chatos_ai_runtime traits --lib` | 通过，4 个相关测试通过，120 个测试被筛选 |
| `cargo check -p task_runner_service_backend` | 通过 |
| `cargo test -p task_runner_service_backend chatos_message_graph --lib` | 通过，1 个相关测试通过，93 个测试被筛选 |
| `cargo test -p task_runner_service_backend preparation --lib` | 通过，3 个相关测试通过，91 个测试被筛选 |
| `cargo test -p task_runner_service_backend mcp_ --lib` | 通过，34 个相关测试通过，60 个测试被筛选 |
| `cargo test -p chat_app_server_rs rust_ --lib` | 未执行成功；测试 exe 被 Windows 应用控制策略拦截，`os error 4551` |
| `cargo check -p project_management_service_backend` | 通过 |
| `cargo test -p project_management_service_backend --lib mcp_` | 通过，13 个相关测试通过，18 个测试被筛选 |
| `cargo check -p project_management_service_backend -p chat_app_server_rs -p task_runner_service_backend` | 通过；Chat Server 仍有历史 dead_code warning |
| `cd user_service/backend; cargo check` | 通过 |
| `cd db_connection_hub/backend; cargo check` | 通过；首次执行下载并编译独立 workspace 依赖 |
| `cd db_connection_hub/backend; cargo test sqlserver --lib` | 未执行成功；该 package 没有 lib target，只能用 binary `cargo check` 覆盖编译 |
| `cargo test -p project_management_service_backend --lib migrations_create_plan_snapshot_sort_indexes` | 通过，1 个测试通过 |
| `cargo test -p project_management_service_backend --lib project_plan` | 历史通过；本轮复跑测试 exe 被 Windows 应用控制策略拦截，`os error 4551` |
| `npm --prefix project_management_service/frontend run type-check` | 通过 |
| `npm --prefix project_management_service/frontend run build` | 通过；Vite 仍提示单 chunk 超 500KB 的既有打包 warning |
| `npm --prefix user_service/frontend run type-check` | 通过 |
| `npm --prefix user_service/frontend run build` | 通过；Vite 仍提示单 chunk 超 700KB 的既有打包 warning |
| `npm --prefix task_runner_service/frontend run type-check` | 通过 |
| `npm --prefix task_runner_service/frontend run build` | 通过 |
| `npm --prefix chat_app run type-check` | 通过 |
| `npm --prefix chat_app test -- --run MessageTaskGraphPanel.test.ts` | 通过，2 个测试通过；仍有 baseline-browser-mapping 数据过期提示 |
| `npm --prefix chat_app test -- --run ToolCallRenderer.test.tsx` | 通过，22 个测试通过；仍有 baseline-browser-mapping 和 Browserslist 数据过期提示 |
| `npm --prefix chat_app test -- --run sessions.selectSession.test.ts` | 通过，12 个测试通过；仍有 baseline-browser-mapping 数据过期提示 |
| `npm --prefix chat_app run build` | 通过；Browserslist/baseline 数据过期和部分 chunk 超 600KB 为既有打包 warning |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/local-dev-stack.ps1 -Action status` | 通过，所有本地栈端口和 WSL Mongo 均显示 up |
| `bash scripts/check-hotspot-line-budgets.sh` | 通过 |
| `bash scripts/check-hotspot-line-budgets.sh --warn-planned` | 通过，输出 2 个 planned warning |
| `bash scripts/code-size-report.sh --top 40` | 通过，报告口径热点数 4；默认硬预算已通过，剩余均为 i18n 资源类文件 |
| `git diff --check` | 通过；仅输出 Git CRLF 替换提示，无实际空白错误 |

历史说明：部分测试执行会被 Windows 应用控制策略拦截测试 exe 或 build script，错误码为 `os error 4551`；本轮 `cargo test -p chat_app_server_rs rust_ --lib` 和 `cargo test -p chatos_builtin_tools apply_patch --lib` 均复现该拦截。

## 下一步

1. 本轮硬热点治理已收尾：默认 `check-hotspot-line-budgets.sh` 通过，报告口径剩余 4 个超 700 行文件均为 i18n 资源类文件。
2. 如继续推进，建议转向 planned i18n 资源治理，或对 600 行级复杂生产模块做性能验证后再决定是否拆分，例如 Chat Server realtime hub、AI client execution loop、Chat 前端 message list windowing。
