# 后端重构与模块化优化计划（2026-02-26）

## 目标
- 降低重复代码与维护成本。
- 控制大文件复杂度，提升可测试性与可读性。
- 在不改变现有行为的前提下，优先做低风险重构。

## 现状摘要（本次扫描）
- 超大文件（>1000 LOC）：
  - `src/builtin/sub_agent_router/settings.rs`
  - `src/api/configs.rs`
  - `src/services/v3/ai_client.rs`
  - `src/services/terminal_manager.rs`
  - `src/services/v2/ai_client.rs`
  - `src/services/task_manager.rs`
  - `src/db/mod.rs`
- 高重复区域：
  - `text_result`、`block_on_result` 多处重复。
  - MCP `args/env` 解析多处重复。
  - v2/v3 的 MCP 执行器、AI server、message manager 存在结构同构。

## 分阶段执行

### Phase 1（低风险，立即执行）
1. 抽取公共工具模块：
   - `src/core/async_bridge.rs`：统一 `block_on_result`。
   - `src/core/tool_io.rs`：统一 `text_result`。
   - `src/core/mcp_args.rs`：统一 `parse_args/parse_env`。
2. 替换调用点，确保行为不变。
3. `cargo check` 验证。

### Phase 2（中风险，收益高）
1. 拆分大文件：
   - `api/configs.rs` -> MCP CRUD / Builtin 设置 / AI model 配置 / MCP 资源读取
   - `services/terminal_manager.rs` -> session runtime / directory guard / prompt parser / shell path
   - `services/task_manager.rs` -> review hub / store / normalizer / mapper
2. 合并 v2/v3 高重复基础设施：
   - `message_manager`
   - `mcp_tool_execute`

### Phase 3（高收益，改动面大）
1. `sub_agent_router` 拆分：
   - `settings.rs` 按 state/git/plugin/docs 分模块
   - `core/job_executor.rs` 拆 command/ai/stream callbacks
2. DB 初始化拆分：
   - `db/mod.rs` 拆 sqlite schema/migration + mongodb init + config loader

## 抽象建议（Rust 风格）
- 以 trait + 组合替代“父类继承”：
  - `ToolService` trait：统一 builtin 工具服务的 list/call/register 模式。
  - `TaskStore` trait：统一 Mongo/SQLite 的任务存储实现。
  - MCP 执行策略 trait：屏蔽 v2/v3 tool schema 差异。

## 执行记录
- [x] 计划持久化到 `backend_refactor_plan.md`
- [x] Phase 1 抽取公共工具模块
- [x] Phase 1 调用点替换
- [x] Phase 1 编译验证


## 本次执行结果（2026-02-26）
- 新增公共模块：`core/async_bridge.rs`、`core/tool_io.rs`、`core/mcp_args.rs`。
- 已替换调用点：task_manager / terminal_controller / code_maintainer / sub_agent_router(core) 的 `text_result` 与 `block_on_result`，以及 configs 与 mcp_loader 的 MCP 参数解析。
- 已通过 `cargo check`。

## 持续优化记录（2026-02-26, 第2轮）
- 已完成 `api/configs.rs` 拆分：
  - `src/api/configs.rs`（保留路由与 MCP CRUD）
  - `src/api/configs/ai_model.rs`
  - `src/api/configs/builtin_settings.rs`
  - `src/api/configs/mcp_resource.rs`
- 路由路径与请求/响应结构保持不变。
- 已通过 `cargo check`。

## 持续优化记录（2026-02-26, 第3轮）
- 已完成 `services/terminal_manager.rs` 模块拆分（改为目录模块）：
  - `src/services/terminal_manager/mod.rs`（保留对外 API：`TerminalSession` / `TerminalsManager` / `get_terminal_manager`）
  - `src/services/terminal_manager/directory_guard.rs`（目录越界防护、`cd` 指令解析与输入规范化）
  - `src/services/terminal_manager/prompt_parser.rs`（prompt 解析、ANSI 清理、cwd 推断）
  - `src/services/terminal_manager/path_utils.rs`（路径规范化与 root 边界判断）
  - `src/services/terminal_manager/shell_path.rs`（跨平台 shell 选择）
  - `src/services/terminal_manager/io_runtime.rs`（shell spawn 与 terminal 输出持久化）
- 已保持业务行为不变，仅做职责拆分与可读性提升。
- 终端管理相关单测全部通过（13/13）。
- 已通过 `cargo check`。

## 持续优化记录（2026-02-26, 第4轮）
- 已完成 `services/task_manager.rs` 模块拆分（改为目录模块）：
  - `src/services/task_manager/mod.rs`（保留对外导出与单测）
  - `src/services/task_manager/types.rs`（数据结构、常量与 patch 规范化入口）
  - `src/services/task_manager/normalizer.rs`（draft/priority/status/tags 归一化）
  - `src/services/task_manager/mapper.rs`（Mongo `Document` 与 `TaskRecord` 映射）
  - `src/services/task_manager/review_hub.rs`（review register/resolve/timeout 流程）
  - `src/services/task_manager/store.rs`（Mongo/SQLite CRUD 与 complete/delete）
- 已保持外部调用路径不变：`crate::services::task_manager::{...}`。
- 已通过 `cargo check`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第5轮）
- 已完成 `builtin/sub_agent_router/settings.rs` 模块拆分（改为目录模块）：
  - `src/builtin/sub_agent_router/settings/mod.rs`（对外入口与导出保持不变）
  - `src/builtin/sub_agent_router/settings/types.rs`（settings 领域模型与常量）
  - `src/builtin/sub_agent_router/settings/state.rs`（state 路径解析与文件初始化）
  - `src/builtin/sub_agent_router/settings/mcp_permissions.rs`（MCP 权限读写）
  - `src/builtin/sub_agent_router/settings/plugins.rs`（marketplace 统计、插件发现与安装）
  - `src/builtin/sub_agent_router/settings/git_import.rs`（git 导入、仓库文件定位、reference docs 与插件源拷贝）
- 已保持 `crate::builtin::sub_agent_router::settings::{...}` 现有函数签名不变。
- 已通过 `cargo fmt`、`cargo check`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第6轮）
- 已完成 `builtin/sub_agent_router/core/job_executor.rs` 模块拆分（改为目录模块）：
  - `src/builtin/sub_agent_router/core/job_executor/mod.rs`（执行编排、模式分发）
  - `src/builtin/sub_agent_router/core/job_executor/command_mode.rs`（命令模式执行与结果落盘）
  - `src/builtin/sub_agent_router/core/job_executor/ai_mode.rs`（AI 模式执行、模型/MCP 装配、响应归一化）
  - `src/builtin/sub_agent_router/core/job_executor/stream_callbacks.rs`（流式回调、buffer 截断、task review 事件透传）
- 已保持 `core::execution` 对 `execute_job` 的调用路径不变。
- 已通过 `cargo fmt`、`cargo check`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第7轮）
- 已完成 `db/mod.rs` 模块拆分（保留 `crate::db::{...}` 导出不变）：
  - `src/db/mod.rs`（轻量入口 + re-export）
  - `src/db/types.rs`（数据库类型与配置结构体）
  - `src/db/factory.rs`（DatabaseFactory、全局初始化与配置加载/环境变量覆盖）
  - `src/db/sqlite.rs`（SQLite 初始化、建表、索引、列补齐）
  - `src/db/mongodb.rs`（MongoDB 初始化、集合创建与索引初始化）
- 已保持外部调用接口不变：`init_global/get_db/get_db_sync/get_factory/DatabaseFactory` 等。
- 已通过 `cargo fmt`、`cargo check`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第8轮）
- 已完成 `v2/v3 message_manager` 去重：
  - 新增 `src/services/message_manager_common.rs`，沉淀共享状态与核心流程：
    - `save_user_message/save_assistant_message/save_tool_message`
    - `get_session_messages/get_session_history_with_summaries`
    - `get_message_by_id/process_pending_saves/get_stats/get_cache_stats`
  - `src/services/v2/message_manager.rs` 与 `src/services/v3/message_manager.rs` 改为薄封装：
    - v2 继续保留同步读取、缓存统计等扩展方法。
    - v3 继续保留空 summary 过滤与 `get_last_response_id` 行为。
- 已完成 `v2/v3 mcp_tool_execute` 的基础设施去重：
  - `src/core/mcp_tools.rs` 新增公共能力：
    - `parse_tool_definition`
    - `build_function_tool_schema`
    - `ToolSchemaFormat`（`LegacyChatCompletions` / `ResponsesStrict`）
    - `normalize_json_schema`（从 v3 下沉到 core）
  - `src/services/v2/mcp_tool_execute.rs` 与 `src/services/v3/mcp_tool_execute.rs` 统一通过 `register_tool` + core helper 构建 tool schema，仅保留格式差异参数。
- 已通过 `cargo fmt`、`cargo check`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第9轮）
- 已新增 `src/services/ai_common.rs`，下沉 v2/v3 共享 AI 辅助逻辑：
  - `normalize_turn_id`
  - `build_user_message_metadata`
  - `build_user_content_parts`
  - `normalize_reasoning_effort`
  - `truncate_log`
- `src/services/v2/ai_server.rs` 与 `src/services/v3/ai_server.rs` 已复用上述 helper，移除重复的 turn_id/附件 metadata/content parts 组装逻辑。
- `src/services/v2/ai_request_handler.rs` 与 `src/services/v3/ai_request_handler.rs` 已复用共享的 `normalize_reasoning_effort` 与 `truncate_log`，删除重复实现。
- 为下沉后的 MCP 公共 helper 补充单测（`src/core/mcp_tools.rs`）：
  - `parse_tool_definition_rejects_blank_name`
  - `build_legacy_function_tool_schema_matches_expected_shape`
  - `normalize_json_schema_enforces_required_and_no_additional_properties`
- 为新增 AI 公共模块补充单测（`src/services/ai_common.rs`）：
  - `normalize_turn_id_trims_and_filters_empty_values`
  - `truncate_log_adds_suffix_when_exceeding_limit`
- 已通过 `cargo fmt`、`cargo check`、`cargo test mcp_tools`、`cargo test ai_common`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第10轮）
- 已完成 `services/v3/ai_client.rs` 大文件拆分（改为目录模块）：
  - `src/services/v3/ai_client/mod.rs`（保留 `AiClient` 主流程、状态与设置应用）
  - `src/services/v3/ai_client/input_transform.rs`（input/parts 规范化与 message item 构建）
  - `src/services/v3/ai_client/tool_plan.rs`（tool call 去重执行计划、alias 结果展开、tool item 构建）
  - `src/services/v3/ai_client/prev_context.rs`（prev_response_id 策略与错误识别辅助）
- 对外 API 保持不变：`crate::services::v3::ai_client::{AiClient, AiClientCallbacks, ProcessOptions}`。
- 单测迁移：`should_use_prev_id_for_next_turn` 相关测试下沉到 `prev_context.rs`，行为保持一致。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第11轮）
- 已完成 `services/v2/ai_client.rs` 大文件拆分（改为目录模块）：
  - `src/services/v2/ai_client/mod.rs`（保留 `AiClient` 主流程、summarize 与 tool 循环）
  - `src/services/v2/ai_client/history_tools.rs`（历史消息去重与 tool 响应补齐、summary/anchor 定位）
  - `src/services/v2/ai_client/token_compaction.rs`（token 估算、超限错误识别与消息截断压缩）
- 对外 API 保持不变：`crate::services::v2::ai_client::{AiClient, AiClientCallbacks}`。
- 已将原 `src/services/v2/ai_client.rs` 替换为目录模块实现，行为保持不变。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第12轮）
- 已完成 `services/v3/ai_request_handler.rs` 大文件拆分（改为目录模块）：
  - `src/services/v3/ai_request_handler/mod.rs`（保留请求编排、流式/非流式处理主流程）
  - `src/services/v3/ai_request_handler/parser.rs`（抽取 output/tool_calls/reasoning 解析与 response_id 判定辅助）
- 对外 API 保持不变：`crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks, AiResponse}`。
- 已将原 `src/services/v3/ai_request_handler.rs` 替换为目录模块实现，行为保持不变。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_request_handler`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第13轮）
- 已完成 `services/v2/ai_request_handler.rs` 大文件拆分（改为目录模块）：
  - `src/services/v2/ai_request_handler/mod.rs`（保留请求编排、流式/非流式处理主流程）
  - `src/services/v2/ai_request_handler/parser.rs`（抽取 reasoning 归一化与流式 tool_calls 增量聚合辅助）
- 对外 API 保持不变：`crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks, AiResponse}`。
- 已将原 `src/services/v2/ai_request_handler.rs` 替换为目录模块实现，行为保持不变。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_request_handler`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第14轮）
- 已在 `src/services/ai_common.rs` 新增跨版本共享 helper：
  - `build_assistant_message_metadata`（统一构建 assistant 消息 metadata：`toolCalls` / `response_id`）
- `v2/v3 ai_request_handler` 已复用该 helper，移除重复 metadata 拼装逻辑：
  - `src/services/v2/ai_request_handler/mod.rs`
  - `src/services/v3/ai_request_handler/mod.rs`
- 为新增 helper 增补单测（`src/services/ai_common.rs`）：
  - `build_assistant_message_metadata_skips_empty_fields`
  - `build_assistant_message_metadata_keeps_response_id_and_tool_calls`
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_request_handler`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第15轮）
- 已在 `src/services/ai_common.rs` 新增跨版本共享 helper：
  - `build_aborted_tool_results`（统一补齐未返回的中断 tool call 结果）
  - `build_tool_result_metadata`（统一构建 tool message metadata：`toolName/success/isError`）
- `v2/v3 ai_client` 已复用上述 helper，移除重复的 aborted 结果构造与 metadata 拼装逻辑：
  - `src/services/v2/ai_client/mod.rs`
  - `src/services/v3/ai_client/mod.rs`
- 为新增 helper 增补单测（`src/services/ai_common.rs`）：
  - `build_tool_result_metadata_keeps_tool_flags`
  - `build_aborted_tool_results_only_adds_missing_calls`
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_client`、`cargo test ai_request_handler`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第16轮）
- 已为 `v2/v3 ai_request_handler` 的 parser 子模块补齐回归测试，覆盖核心解析行为：
  - `src/services/v2/ai_request_handler/parser.rs`
    - `normalize_reasoning_value` 的字符串/null/JSON 归一化
    - `merge_tool_call_delta` 的增量拼接
    - `collect_tool_calls` 的有序聚合
  - `src/services/v3/ai_request_handler/parser.rs`
    - `extract_tool_calls` 的 function_call 提取
    - `extract_output_text` 的 message parts 拼接
    - `extract_reasoning_from_response` 的 reasoning/reasoning_summary 聚合
    - `looks_like_response_id` 的响应 ID 判定
- 本轮以“先补测试、再继续下沉共享逻辑”为目标，先锁定现有行为，降低后续跨版本合并风险。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_request_handler`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第17轮）
- 已完成 `ai_request_handler` 流式 SSE 解析的跨版本下沉：
  - `src/services/ai_common.rs` 新增 `drain_sse_json_events`（统一处理 `data:` 行、`[DONE]`、JSON 解码与 buffer 尾包保留）
  - `src/services/v2/ai_request_handler/mod.rs` 与 `src/services/v3/ai_request_handler/mod.rs` 均改为复用该 helper，移除重复的 packet/line 解析模板代码
- 已完成 tool message 持久化小工具抽象：
  - `src/services/message_manager_common.rs` 新增 `save_tool_results`，统一批量写入 tool message（含 metadata 组装）
  - `src/services/v2/message_manager.rs` 与 `src/services/v3/message_manager.rs` 暴露同名转发方法
  - `src/services/v2/ai_client/mod.rs` 与 `src/services/v3/ai_client/mod.rs` 复用 `save_tool_results`，移除三处重复持久化循环
- 为新下沉的 SSE helper 增补单测（`src/services/ai_common.rs`）：
  - `drain_sse_json_events_parses_packets_and_keeps_incomplete_tail`
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_request_handler`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第18轮）
- 已继续下沉 `ai_request_handler` 共享流式编排逻辑：
  - `src/services/ai_common.rs` 新增 `consume_sse_stream`（统一处理 chunk 读取、取消检查、UTF-8 解码、SSE 事件分发）
  - `src/services/v2/ai_request_handler/mod.rs` 与 `src/services/v3/ai_request_handler/mod.rs` 改为复用该 helper，移除重复的 `while let Some(chunk)` 模板
- 为新增 helper 增补单测（`src/services/ai_common.rs`）：
  - `consume_sse_stream_emits_events_and_ignores_done_lines`
- 本轮重点是“保留原有事件解析细节，仅抽取 I/O 与事件派发骨架”，进一步降低 v2/v3 双实现维护成本。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_request_handler`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第19轮）
- 已将 v2/v3 流式事件的“状态变更逻辑”进一步下沉到 parser 子模块，减少 `mod.rs` 复杂度：
  - `src/services/v2/ai_request_handler/parser.rs`
    - 新增 `StreamState` / `StreamCallbacksPayload`
    - 新增 `apply_stream_event`（统一处理 usage/finish_reason/content/reasoning/tool_calls 的状态更新与回调负载）
  - `src/services/v3/ai_request_handler/parser.rs`
    - 新增 `StreamState` / `StreamCallbacksPayload`
    - 新增 `apply_stream_event`（统一处理 output_text/reasoning/completed/failed/response_id/usage 的状态更新与回调负载）
- `src/services/v2/ai_request_handler/mod.rs` 与 `src/services/v3/ai_request_handler/mod.rs` 已复用上述 helper，主流程更聚焦于请求编排与持久化。
- 为新增 helper 增补单测：
  - `v2`: `apply_stream_event_updates_state_and_emits_callbacks_payload`
  - `v3`: `apply_stream_event_updates_stream_state_and_payload`
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_request_handler`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。

## 持续优化记录（2026-02-26, 第20轮）
- 已在 `src/services/ai_common.rs` 新增跨版本共享异步桥接 helper：
  - `await_with_optional_abort`（统一 future 等待 + `CancellationToken` 中止语义）
- `v2/v3 ai_request_handler` 的 normal/stream 请求发送阶段已复用该 helper，移除重复的 `tokio::select!` + `send.await` 模板：
  - `src/services/v2/ai_request_handler/mod.rs`
  - `src/services/v3/ai_request_handler/mod.rs`
- 为新增 helper 增补单测（`src/services/ai_common.rs`）：
  - `await_with_optional_abort_returns_future_value_without_token`
  - `await_with_optional_abort_returns_aborted_when_token_cancelled`
- 本轮目标是继续统一跨版本请求生命周期控制，保持行为不变并进一步减少并行演进成本。
- 已通过 `cargo fmt`、`cargo check`、`cargo test ai_common`、`cargo test ai_request_handler`、`cargo test ai_client`、`cargo test sub_agent_router`、`cargo test task_manager`、`cargo test terminal_manager`。
