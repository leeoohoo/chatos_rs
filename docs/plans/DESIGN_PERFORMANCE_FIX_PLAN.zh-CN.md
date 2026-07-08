# Chatos RS 整体设计缺陷与性能瓶颈修复方案

生成日期：2026-06-28

## 0. 实施进度

### 2026-06-28 第一批已实施

1. WebSocket 出站背压已落地：
   - 新增 `chatos/backend/src/utils/ws_outbound.rs`，统一创建 bounded WebSocket 出站队列，并在队列满时记录慢客户端日志、触发连接关闭。
   - `chatos/backend/src/api/realtime.rs`：`mpsc::unbounded_channel` 改为容量 256 的 bounded channel。
   - `chatos/backend/src/api/terminals/ws_handlers.rs`：`mpsc::unbounded_channel` 改为容量 512 的 bounded channel。
   - `chatos/backend/src/api/remote_connections/terminal_ws_api.rs`：`mpsc::unbounded_channel` 改为容量 512 的 bounded channel；二次验证 challenge 的 blocking 线程改用 `try_send`，避免阻塞或无限排队。
   - `chatos/backend/src/utils/sse.rs`：移除 `UnboundedSender` 类型依赖，改为 bounded `mpsc::Sender` + `try_send`。当前代码未发现实际 SSE sender 创建入口；若恢复 SSE stream endpoint，创建处必须使用 bounded channel，并补上满队列断开逻辑。
2. AI payload 重复序列化已移除：
   - 删除 `chat_app_server_rs` 上层 `validate_request_payload_size(&payload, ...)` 预检，避免请求发送前额外 `serde_json::to_vec`。
   - `request_body_limit_bytes` 现在传入 `chatos_ai_runtime::AiRequestOptions`，保留 shared runtime 内的单次 `serialize_request_payload` + size check。
   - 删除上层 precheck 单测，将 payload size limit 测试迁移到 `crates/chatos_ai_runtime/src/request/tests.rs`。
3. 已完成验证：
   - `cargo fmt --all`
   - `cargo check -p chat_app_server_rs`
   - `cargo check -p chatos_ai_runtime`
   - `cargo check --workspace`
   - `cargo test -p chatos_ai_runtime request_payload_size_limit`
   - `git diff --check`
4. 未完成验证说明：
   - `cargo test -p chat_app_server_rs startup_error_shutdown_flushes_error_message_before_exit` 未能执行到断言阶段；Windows 当前有正在运行的 `target-shared/debug/chat_app_server_rs.exe` 进程占用同名测试 exe，Cargo 删除旧 exe 时返回 `os error 5`。这属于本机运行进程锁文件，不是本次代码编译错误。

### 2026-06-28 第二批已实施

1. stdio MCP session 生命周期已加强：
   - `crates/chatos_mcp_runtime/src/rpc.rs` 的全局 session map 增加 `created_at`、`last_used_at` 元数据。
   - 增加 per-key cold-start lock，同一 stdio server 并发冷启动时只允许一个 spawn 进入临界区。
   - 增加 session 池容量上限，默认最多 32 个 stdio MCP session；超限时按 idle LRU 回收可安全 drop 的 session。
   - 增加 idle TTL，默认 10 分钟；后续访问 session 池时会清理超过 TTL 且没有 in-flight 引用的 session。
   - 增加 `spawned`、`reused`、`evicted`、`removed` 相关 tracing 日志。
2. 长会话 Memory Engine pending 保护已落地：
   - `chatos/backend/src/services/chatos_memory_engine/sessions.rs` 调用 compose context 时显式设置 `recent_record_limit: Some(200)`。
   - 该限制只作为 summary 异常滞后或 pending records 失控时的保护，不改变正常 `summary + pending tail` 流程判断。
3. 本地终端 snapshot tail 优化已落地：
   - `chatos/backend/src/services/terminal_manager/output_history.rs` 的 `snapshot_tail_lines` 改为从尾部 chunk 反向扫描，只拼接最终要返回的尾部内容，避免为了取尾部先构造完整 history 字符串。
   - 增加了 `snapshot_tail_lines` 跨 chunk、边界和 zero limit 单测。
4. 已完成验证：
   - `cargo fmt --all`
   - `cargo check -p chatos_mcp_runtime`
   - `cargo test -p chatos_mcp_runtime`
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`
   - `git diff --check`
5. 未完成验证说明：
   - `cargo test -p chat_app_server_rs services::terminal_manager::output_history::tests --lib` 已编译完成并启动 lib test binary，但在本机环境长时间无输出；为避免后台会话挂住已中止。当前用 `cargo check -p chat_app_server_rs` 和 `cargo check --workspace` 验证编译正确性。

### 2026-06-28 第三批已实施

1. AI tools 大对象 clone 已收敛：
   - `chatos/backend/src/services/agent_runtime/ai_request_handler/mod.rs` 的 `handle_request` 入参从 `Option<Vec<Value>>` 改为 `Option<&[Value]>`。
   - `chatos/backend/src/services/agent_runtime/ai_client/execution_loop.rs` 每次模型请求不再提前 `tools.clone()`，只在最终构建 provider payload 时复制一次工具定义。
2. live request snapshot 回调 clone 已收敛：
   - `AiClientCallbacks::on_before_model_request` 改为接收 `&Value`。
   - `chatos/backend/src/modules/conversation_runtime/chat_runner.rs` 在同步 snapshot 前直接从借用的 request input 提取 context items，不再为了回调 clone 整个 request input。
3. stateless context 重复 rebuild 已减少：
   - `process_request` 已经构建 initial stateless input 后，`process_with_tools` 第一轮不再立刻重复 `maybe_refresh_stateless_context`。
   - tool 执行后的 `advance_after_tool_execution` 已经重建 stateless input，下一轮顶部不再重复刷新。
   - 如果运行时 prefixed input items 被加载到，仍会强制刷新，避免漏掉 Task Board 等运行时上下文。
4. 已完成验证：
   - `cargo fmt --all`
   - `cargo check -p chat_app_server_rs`
   - `cargo test -p chatos_ai_runtime request_payload_size_limit`
   - `cargo test -p chat_app_server_rs build_chat_completions_payload --lib`
   - `cargo check --workspace`
   - `git diff --check`

### 2026-06-28 第四批已实施

1. workspace 内容搜索已加 wall-clock deadline：
   - `chatos/backend/src/services/workspace_search/mod.rs` 增加默认 3 秒 deadline。
   - 命中 deadline 时沿用现有 `truncated: true`，不改变 API 响应结构。
   - `TextSearchRequest` 增加内部可配置 `deadline` 字段，便于测试和后续工具级差异化配置。
2. 文件搜索 API 已从 async worker 隔离同步 IO：
   - `chatos/backend/src/api/fs/query_handlers_search.rs` 的文件名搜索和内容搜索都改为 `tokio::task::spawn_blocking` 执行。
   - 文件名搜索同样增加 3 秒 deadline，超时返回部分结果并标记 `truncated: true`。
3. code-nav 同步 fallback / heuristic provider 已纳入 blocking pool：
   - `chatos/backend/src/services/code_nav/manager.rs` 的 fallback definition / references / document symbols 改为 blocking task。
   - `chatos/backend/src/services/code_nav/languages/shared_nav.rs` 的共享 heuristic provider wrapper 改为 blocking task。
   - `chatos/backend/src/services/code_nav/languages/basic.rs` 增加 BasicSpec blocking helper，C / C++ / C# / Kotlin provider 已切换复用。
4. code-nav symbol index 增加扫描预算：
   - `chatos/backend/src/services/code_nav/symbol_index.rs` 增加 20000 entry 上限和 5 秒 deadline。
   - 超预算时返回错误，不缓存不完整索引；调用方继续走已有文本搜索 fallback。
5. code-nav 语言级文本搜索增加扫描预算：
   - `chatos/backend/src/services/code_nav/languages/shared_nav.rs` 增加共享文本搜索预算 helper，默认 20000 entry / 3 秒。
   - Basic / Rust / Go / Java / Python 搜索路径已接入预算，避免 symbol index fallback 后继续长时间扫仓。
   - Java / Python 的 import 解析兜底 WalkDir 和 Go 包文件枚举也已接入同一预算。
6. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs workspace_search --lib`
   - `cargo test -p chat_app_server_rs code_nav --lib` 输出显示 31 个 code-nav 相关测试全部通过。
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`
   - `git diff --check`
7. 验证限制说明：
   - 后续再次运行 `cargo test -p chat_app_server_rs code_nav --lib` 时，测试 binary 编译完成后被 Windows 应用程序控制策略阻止执行，返回 `os error 4551`；当前用 `cargo check --workspace` 和前一次 code-nav 测试输出确认代码正确性。

### 2026-06-28 第五批已实施

1. 工具执行层重 IO 并发闸门已落地：
   - `chatos/backend/src/services/mcp_execution_core/execution.rs` 对文件读写、目录列表、文本搜索、patch 等重 IO 工具增加 semaphore 限制。
   - 同一 session 默认最多 2 个重 IO 工具并发执行，进程内默认最多 8 个重 IO 工具并发执行。
   - 限制挂在 `call_tool_once` 入口，覆盖串行执行、并行执行、builtin / HTTP / stdio MCP 工具调用。
   - session limiter 使用弱引用缓存，空闲 session 的 limiter 会在后续访问时被清理，避免无界保存历史 session。
2. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs heavy_io_tool_policy --lib`
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`

### 2026-06-28 第六批已实施

1. 内置 code maintainer 搜索路径已加保护：
   - `crates/chatos_builtin_tools/src/code_maintainer/fs_ops.rs` 的 `search_text` 在直接搜索单个文件时，现在同样遵守 `max_file_bytes`。
   - `rg` 搜索增加 `--max-count`，降低单文件大量命中时的 stdout 放大。
   - `rg` 不可用或失败后的 WalkDir fallback 增加 20000 entry / 3 秒预算。
   - 搜索结果行文本统一截断到 400 个字符，并按 UTF-8 字符边界截断，避免超长单行放大工具响应。
2. `read_file_range` 已改为流式扫描：
   - 不再通过 `read_file_raw` 先读完整文件并构造全量行数组。
   - 仍保留原响应字段：整文件 `sha256`、`size_bytes`、`total_lines`、实际返回范围和内容。
   - 只保留请求范围内的行文本，整文件扫描仅用于 hash、二进制检测和总行数统计。
3. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chatos_builtin_tools search_text_`
   - `cargo test -p chatos_builtin_tools code_maintainer::fs_ops::tests --lib`
   - `cargo check -p chatos_builtin_tools`
   - `cargo check --workspace`
   - `git diff --check`

### 2026-06-28 第七批已实施

1. Project Run analyzer 扫描预算已落地：
   - 新增 `chatos/backend/src/services/project_run/analyzer/scan_budget.rs`，统一提供 20000 filesystem entries / 5 秒的项目运行目标分析预算。
   - `chatos/backend/src/services/project_run/analyzer/scan.rs` 的主 BFS 扫描现在按目录和条目计入预算，超限时返回明确错误，不再静默吞掉分析失败原因。
   - `analyze_project` 保留 analyzer 内部错误到 `ProjectRunCatalog.error_message`，并增加 `project run target analysis failed` warn 日志，便于定位大仓库超预算或文件系统异常。
2. Project Run 二级入口探测已加保护：
   - Java 探测现在只有命中 `pom.xml` / Gradle manifest 时才扫描 `src/main/java`，避免普通目录树被无意义 Java WalkDir 放大。
   - Java entrypoint WalkDir、Go `cmd/*` / 根目录 main 探测、Rust `src/bin` 探测都接入同一 `ScanBudget`。
   - 保留 `detect_go_entrypoints`、`detect_rust_bins` 原有签名，环境校验调用无需改动；内部包装同样使用默认预算，避免校验路径扫描失控。
3. Project Run manifest/source 探测读取已限幅：
   - `package.json`、`pom.xml`、`Cargo.toml` 读取限制为 1MB。
   - Java / Go 源码入口探测单文件读取限制为 512KB，避免大生成文件或异常文件在目标识别阶段造成内存峰值。
4. 已完成验证：
   - `cargo fmt --all`
   - `cargo check -p chat_app_server_rs`
5. 验证限制说明：
   - `cargo test -p chat_app_server_rs stops_when_project_scan_budget_is_exhausted --lib` 已完成 lib test 编译，但测试 binary 执行阶段被 Windows 应用程序控制策略阻止，返回 `os error 4551`。这与第四批记录的本机策略限制一致，不是编译错误。

### 2026-06-28 第八批已实施

1. Project Run 环境快照构建已隔离同步 IO：
   - `chatos/backend/src/services/project_run/environment.rs` 在完成数据库读取后，将 `build_environment_snapshot` 放入 `tokio::task::spawn_blocking`。
   - 该快照构建包含工具链发现、项目配置文件读取、目标校验等同步文件系统操作；隔离后不会直接占用 async worker。
2. Project Run 文件读取限幅已统一：
   - 新增 `chatos/backend/src/services/project_run/file_limits.rs`，集中定义 manifest / source probe / config preview 的最大读取字节数。
   - `analyzer/scan_budget.rs` 复用该模块，不再单独维护一份受限读取逻辑。
   - `environment_discovery/config_files.rs` 的配置预览限制为 256KB。
   - `environment_discovery/hints.rs` 的 `.tool-versions`、`go.mod`、`rust-toolchain.toml` 等版本提示读取限制为 1MB。
   - `environment_validation.rs` 的 Node `package.json` script 校验读取限制为 1MB。
   - `environment_discovery/support.rs` 的工具链版本目录枚举限制为最多 512 个子目录，避免异常膨胀的 SDKMAN / pyenv / rustup 等目录放大内存。
3. 已完成验证：
   - `cargo fmt --all`
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`
   - `cargo test -p chat_app_server_rs stops_when_project_scan_budget_is_exhausted --lib --no-run`

### 2026-06-28 第九批已实施

1. 前端工具详情大 payload 渲染保护已落地：
   - 新增 `chatos/frontend/src/components/toolDetails/textPreview.ts`，统一提供长文本截断和 bounded JSON preview。
   - JSON preview 按最大深度、数组项数、对象键数、字符串长度做预览对象后再序列化，避免工具详情为了展示任意大对象执行全量 pretty `JSON.stringify`。
   - `TextBlockCard` 和工具输入详情 `renderTextBlock` 现在会在渲染前自动截断长文本，并用 `truncated` badge 标记。
   - `GenericStructuredResultDetails`、`ToolArgumentsDetails`、fallback tool result renderer、browser console result payload 均改用安全 JSON preview。
2. 前端静态排查结论：
   - 主消息列表已经存在 `useMessageListWindowing`，会对长会话消息进行窗口化渲染。
   - 本地终端输出主要写入 xterm buffer，没有把完整终端输出持续放入 React state；当前无需在这块做大改。
3. 已完成验证：
   - `npm run type-check`
   - `npm run test -- textPreview.test.ts --run`

### 2026-06-28 第十批已实施

1. 前端 realtime 高频失效刷新已合并：
   - 新增 `chatos/frontend/src/lib/realtime/invalidationQueue.ts`，将同 tick / cooldown 窗口内的重复 invalidation 合并，只保留最后一次 payload。
   - `chatos/frontend/src/lib/realtime/useProjectRunRealtime.ts` 的 project run state、catalog、members 更新接入 150ms 合并队列，避免 realtime burst 触发连续刷新。
   - `project.run.instance_changed` 仍保持直接回调，避免 terminal instance 退出等关键 payload 被合并延迟。
   - 组件卸载时清理 pending payload 和 timer，避免卸载后刷新。
2. 前端 realtime debug 渲染隔离已落地：
   - `chatos/frontend/src/lib/realtime/RealtimeProvider.tsx` 将 `client`、`connectionState`、`debugSnapshot` 拆为独立 context。
   - `useRealtimeConnectionState` 不再订阅 debug snapshot；debug 事件、topic 同步、ack/pong 更新不会唤起只关心连接状态的组件。
   - 保持 `useRealtimeContext`、`useRealtimeConnectionState`、`useRealtimeDebugSnapshot` 现有导出 API 不变，降低调用侧迁移风险。
3. 已完成验证：
   - `npm run test -- invalidationQueue.test.tsx --run`
   - `npm run test -- RealtimeProvider.test.tsx invalidationQueue.test.tsx --run`
   - `npm run type-check`

### 2026-06-28 第十一批已实施

1. 本地终端输出 history 单 chunk 放大已收敛：
   - `chatos/backend/src/services/terminal_manager/output_history.rs` 在写入 history 前按 64KB / 1024 换行符分片。
   - history 仍按 2MB / 10000 行总上限回收，但不再因为单个异常大 chunk 导致大块长期保留或整块回收过粗。
   - `snapshot_tail_lines` 保持原有接口，只返回请求的尾部行。
2. 远端终端输出 history 已补齐行数上限：
   - `chatos/backend/src/api/remote_connections/remote_terminal.rs` 增加 10000 行总上限。
   - 远端 history 同样按 64KB / 1024 换行符分片，继续保留 512KB 总字节上限。
   - WebSocket snapshot 响应结构未改变，仍发送 `WsOutput::Snapshot { data }`。
3. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs terminal_manager::output_history --lib`
   - `cargo test -p chat_app_server_rs remote_connections::remote_terminal --lib`
   - `cargo check -p chat_app_server_rs`
   - `git diff --check`

### 2026-06-28 第十二批已实施

1. Project Plan 大列表渲染已加窗口保护：
   - `chatos/frontend/src/components/projectExplorer/ProjectPlanPane.tsx` 对选中需求下的 work item 默认只渲染前 80 个。
   - 用户需要查看更多时通过底部按钮每次递增 80 个，顶部统计仍基于完整 work item 数据。
   - 切换需求时重置可见窗口，避免跨需求保留过大的渲染窗口。
2. Project Plan 依赖标签渲染已限幅：
   - `chatos/frontend/src/components/projectExplorer/projectPlanPane/components.tsx` 的 `DependencyLine` 最多渲染前 16 个依赖标签。
   - 超出部分用 `+N` 汇总，避免异常依赖图生成大量 pill DOM。
3. 已完成验证：
   - `npm run test -- projectPlanPane/model.test.ts --run`
   - `npm run type-check`

### 2026-06-28 第十三批已实施

1. 长会话 Task Board 上下文加载策略已修正：
   - `chatos/backend/src/modules/conversation_runtime/task_board.rs` 不再用单次 `include_done=true LIMIT 200` 作为运行时 task board 输入。
   - 运行时上下文现在先单独加载非 done 任务，再补充 done 历史并按 id 去重，避免旧 done 任务挤掉当前 todo / doing 任务。
   - 该调整只影响模型运行时 task board 上下文，不改变 `/api/task-manager/tasks` 外部接口。
2. current task 选择更稳：
   - `chatos/backend/src/services/task_board_prompt.rs` 选择当前执行任务时改为取最新 `updated_at` / `created_at` 的 `doing` 任务。
   - 不再依赖输入数组顺序，避免查询排序变化时选到旧 doing。
3. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs task_board --lib`
   - `cargo check -p chat_app_server_rs`
   - `git diff --check`

### 2026-06-28 第十四批已实施

1. 日志热路径复查结论：
   - 后端 AI 请求链路日志只记录 transport、model、字节数、耗时、tool call 数等摘要字段，未发现完整 prompt / payload 直接进入 tracing 日志。
   - `println!` 命中点复查后确认是 Rust code-nav 单测 fixture 字符串，不是运行时 stdout 调试输出。
2. 前端 debug 日志开销已收敛：
   - `chatos/frontend/src/lib/utils/index.ts` 的 `debugLog` 改为显式开发开关：仅当 `localStorage.chatos.debugLog` 为 `1` 或 `true` 时输出。
   - 新增 `debugLogLazy`，避免未开启 debug 时仍构造日志对象。
   - `sendMessage` 不再默认构造聊天请求调试 payload；只有 debug 开启时才 lazy 构造。
   - session message cache、compact history 的调试日志也改为 lazy 构造。
3. 调试 payload 已摘要化：
   - `chatos/frontend/src/lib/store/actions/sendMessage/requestPayload.ts` 不再把完整 message、system context、attachment dataUrl/text 放入日志对象。
   - 日志只保留文本 preview、字符数、附件数量、附件总字节数和附件元数据。
4. 已完成验证：
   - `npm run test -- sendMessage/requestPayload.test.ts --run`
   - `npm run type-check`
   - `git diff --check`

### 2026-06-28 第十五批已实施

1. Project Management 跨服务响应读取已限幅：
   - `chatos/backend/src/services/project_management_api_client.rs` 的成功响应不再直接 `response.json()`，改为先通过 `bytes_stream()` 有界读取，再 `serde_json::from_slice`。
   - 普通 Project Service 响应默认限制为 2MB；Project Plan 详情响应限制为 8MB；错误响应只读取 16KB 预览。
   - 读取前先检查 `content_length()`，流式读取过程中继续按累计 bytes 检查，避免缺失 Content-Length 时仍读入超大 body。
   - `send_json()`、`send_optional_json()`、`get_project_service_plan()` 均接入该限制。
2. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs project_management_api_client --lib`
   - `cargo check -p chat_app_server_rs`
   - `git diff --check`

### 2026-06-28 第十六批已实施

1. Task Runner API client 响应读取已限幅：
   - `chatos/backend/src/services/task_runner_api_client.rs` 的 token exchange、skill fetch、task CRUD、prompt submit/cancel、internal message task 查询均改为有界读取后解析 JSON。
   - 普通响应默认限制为 2MB；internal message/task graph 响应限制为 4MB；错误响应只读取 16KB 预览。
2. User Service client 已收敛响应读取和连接复用：
   - `chatos/backend/src/services/user_service_api_client/http.rs` 的 `request_json` 改为有界读取成功响应后解析 JSON，错误响应只读取 16KB 预览。
   - 原来每次请求都构造新的 `reqwest::Client`，已改为 `OnceLock<reqwest::Client>` 复用连接池；timeout 保持按 request 设置，不改变调用方语义。
3. 外部 MCP 配置代理已收敛：
   - `chatos/backend/src/api/task_runner_external_mcp.rs` 不再每次转发都重建 HTTP client。
   - 成功响应限制为 2MB，错误响应预览限制为 16KB，继续保留原 API 响应结构。
4. 模型列表拉取与 MCP HTTP JSON-RPC 已限幅：
   - `chatos/backend/src/api/configs/ai_model/provider_models.rs` 拉取 `/models` 时成功响应限制为 2MB，错误预览限制为 16KB，并复用 HTTP client。
   - `chatos/backend/src/core/mcp_tools/rpc.rs` 的 HTTP JSON-RPC 响应限制为 4MB，错误预览限制为 16KB。
5. 复扫结论：
   - 后端已无剩余无界 `response.text()` / `response.json()` HTTP 解析点。
   - 当前剩余命中为 tracing JSON formatter、code-nav 文本预览，以及新增的有界 `bytes_stream()` 读取。
6. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs task_runner_api_client --lib`
   - `cargo test -p chat_app_server_rs user_service_api_client --lib`
   - `cargo test -p chat_app_server_rs body_limit --lib`
   - `cargo check -p chat_app_server_rs`

### 2026-06-28 第十七批已实施

1. MCP stdio 响应读取已加单行上限：
   - `chatos/backend/src/core/mcp_tools/rpc.rs` 不再使用 `BufReader::lines()` 读取 stdio JSON-RPC 响应，改为 `fill_buf` + `consume` 按缓冲块读取，并在累计单行超过 4MB 前返回错误。
   - `crates/chatos_mcp_runtime/src/rpc.rs` 的 stdio session 池同样改为有界单行读取，避免常驻 session 遇到异常超长 stdout 行时撑大内存。
   - `chatos/backend/src/api/configs/mcp_resource.rs` 的 MCP 配置资源读取入口补充 15 秒 timeout、2MB stdio 响应单行上限、1MB 配置文本上限。
2. MCP HTTP tools/list 响应读取已限幅：
   - `crates/chatos_mcp_runtime/src/rpc.rs` 的 HTTP JSON-RPC 响应不再直接 `response.text()`，成功响应限制为 4MB，错误响应预览限制为 16KB。
   - 非 JSON 响应仍保留原有 preview 报错逻辑，但 preview 来自有界读取结果。
3. stdio 子进程回收更明确：
   - `chatos/backend/src/core/mcp_tools/rpc.rs` 和 `mcp_resource.rs` 的临时 stdio 子进程设置 `kill_on_drop(true)`，避免超时、超限或提前返回后子进程继续残留。
4. 复扫结论：
   - `chatos/backend/src` 与 `crates/chatos_mcp_runtime/src` 已无剩余 `response.text()` / `response.json()` 无界 HTTP 响应解析。
   - 已无核心 stdio JSON-RPC 路径继续使用 `BufReader::new(stdout).lines()` / `next_line()`。
5. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs mcp_ --lib`
   - `cargo test -p chat_app_server_rs mcp_resource --lib`
   - `cargo test -p chatos_mcp_runtime response_ --lib`
   - `cargo test -p chatos_mcp_runtime --lib`
   - `cargo check -p chatos_mcp_runtime`
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`
   - `git diff --check`

### 2026-06-28 第十八批已实施

1. Git 子进程输出读取已改为有界流式读取：
   - `chatos/backend/src/services/git/process.rs` 不再使用 `Command::output()` 一次性收集 stdout/stderr。
   - 新增 Git 命令 runner：stdout/stderr 分别并发读取，并在累计输出超过上限时终止子进程。
   - stdout 上限为 16MB，stderr 上限为 4MB；超限返回明确错误，而不是继续把大输出读入内存。
   - runner 设置 `stdin(Stdio::null())`、`kill_on_drop(true)`，超时、读取失败或输出超限时会主动终止子进程。
2. 覆盖范围：
   - `git status --porcelain`、branch/ref 查询、compare、file diff、push/pull/fetch 等通过 `git_output` / `git_output_with_status` 的路径均接入该保护。
   - `git --version` 也改为同一 runner，保留原超时错误文案。
3. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs git_ --lib`
   - `cargo check -p chat_app_server_rs`

### 2026-06-28 第十九批已实施

1. native SSH 远程命令输出已限幅：
   - `chatos/backend/src/api/remote_connections/connectivity.rs` 的 ssh2 native 路径不再对 stdout/stderr 使用无界 `read_to_end`。
   - stdout 上限为 4MB，stderr 上限为 1MB；超限时返回明确错误，避免远端命令异常输出导致后端内存放大。
   - 覆盖 `run_ssh_command` / `run_ssh_command_with_verification`，包括远程连接 builtin 工具、SFTP 目录操作、连接测试等复用路径。
2. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs remote_connections --lib`
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`
   - `git diff --check`

### 2026-06-28 第二十批已实施

1. Browser runtime 子进程输出已限幅：
   - `crates/chatos_builtin_tools/src/browser_runtime.rs` 不再对 agent-browser stdout/stderr 使用无界 `read_to_end`。
   - stdout 上限为 4MB，stderr 上限为 1MB；超限时主动终止子进程并返回 `success=false` 的结构化错误。
   - 输出读取、进程等待和 timeout 现在通过 `tokio::select!` 协同，避免某个 pipe 超限后停止读取导致子进程继续阻塞。
2. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chatos_builtin_tools browser_stream_limit --lib`
   - `cargo check -p chatos_builtin_tools`

### 2026-06-28 第二十一批已实施

1. TypeScript 语义导航 bridge 子进程已限幅：
   - `chatos/backend/src/services/code_nav/languages/ts_service.rs` 不再使用 `Command::output()` 全量收集 node bridge 输出。
   - stdout 上限为 2MB，stderr 上限为 512KB；输出超限时会主动终止子进程并返回明确错误。
   - bridge 调用增加 20 秒 timeout，避免 TypeScript language service 在异常项目上长期挂住。
2. 已完成验证：
   - `cargo fmt --all`
   - `cargo test -p chat_app_server_rs ts_bridge_stream_limit --lib`
   - `cargo check -p chat_app_server_rs`
   - `cargo check --workspace`
   - `git diff --check`

## 1. 审查结论

本次重点看了主后端、MCP runtime、AI 请求链路、终端/实时通道、Task Runner、Memory Engine、Project Management、User Service 以及前端热点入口。

当前最高优先级问题不是数据库分页或日志本身，而是几个高流量路径缺少背压或存在大对象重复构造：

1. WebSocket / SSE 出站队列使用无界 channel，慢客户端可能导致服务端内存持续增长。
2. AI 请求链路会在每轮模型请求前重建 stateless context，并多次 clone 大 `serde_json::Value`。
3. AI payload 发送前存在重复序列化：先为大小校验序列化一次，再在共享 runtime 里序列化一次。
4. stdio MCP session 池是全局长生命周期，但缺少容量上限、空闲回收和并发冷启动去重。
5. 部分工具型文件扫描/代码导航仍是同步 IO 模型，需要统一纳入 blocking pool、限流和观测。

Memory Engine 是长会话上下文的正常入口。按正常流程，旧消息会被 summary 消化，模型请求应由 Memory Engine summary + 少量 pending records 组成，不应把 1000 条完整历史直接传给模型。因此长会话性能评估重点不是“1000 条历史直发”，而是确认 summary 命中率、pending records 数量、payload bytes 和 context rebuild 成本是否稳定。前端快速扫描未发现高置信的无限定时器或明显渲染雪崩，建议先等后端流式通道和 payload 热点修复后再做前端 profiler。

## 2. P0 修复：实时通道背压

### 2.1 问题

以下路径使用 `mpsc::unbounded_channel` 或 `UnboundedSender` 作为 WebSocket/SSE 出站缓冲：

- `chatos/backend/src/api/realtime.rs`
- `chatos/backend/src/api/terminals/ws_handlers.rs`
- `chatos/backend/src/api/remote_connections/terminal_ws_api.rs`
- `chatos/backend/src/utils/sse.rs`

终端 session 内部的 broadcast channel 有容量，例如 `broadcast::channel(4096)`，但 WebSocket 出站层仍是无界队列。若浏览器端网络慢、页面挂起、代理阻塞，终端输出、realtime 事件或 SSE chunk 会堆在进程内存里。

### 2.2 修复方案

1. 新增统一的 bounded send helper，例如 `BoundedWsSender` / `BoundedSseSender`。
2. 将 WebSocket 出站 channel 改为 `mpsc::channel(capacity)`，初始建议：
   - realtime：256
   - 普通终端：512
   - 远端终端：512
   - SSE chat stream：256
3. 发送策略按事件类型区分：
   - terminal output：允许合并或丢弃旧 output chunk，保留 exit/state/error。
   - realtime invalidation：允许 coalesce，同类 topic 只保留最新一条。
   - chat SSE：不能静默丢 token；队列满时主动断开并记录 `slow_client_disconnect`。
4. 对 sender 增加指标字段：
   - `queue_capacity`
   - `queue_len` 或近似 pending count
   - `dropped_events`
   - `slow_client_disconnects`
5. 增加测试：
   - 慢消费者时 bounded queue 不无限增长。
   - terminal output 满队列时 state/exit 仍可送达。
   - SSE 满队列时请求被明确终止。

### 2.3 验证

- `cargo test -p chat_app_server_rs realtime`
- `cargo test -p chat_app_server_rs terminals`
- 手动打开终端执行大量输出命令，限速浏览器网络，确认后端内存稳定。

## 3. P1 修复：AI 请求链路大对象重复构造

### 3.1 问题

`chatos/backend/src/services/agent_runtime/ai_client/execution_loop.rs` 每轮会：

- `prefixed_input_items.clone()`
- `input.clone()`
- `request_input.clone()` 传给 callback
- `tools.clone()` 传给 request handler

`chatos/backend/src/services/agent_runtime/ai_client/stateless_context.rs` 每次刷新 stateless context 会把历史消息字段 clone 成 `StatelessHistoryMessage`，再构造新的 `Vec<Value>`。

对短会话影响较小；对长会话、大工具列表、大工具输出或多轮 tool-call 场景，会带来明显 CPU 和内存分配压力。

长会话正常流程下由 Memory Engine summary 压缩历史，模型请求不会携带全部历史。这里的性能风险主要是：每轮仍会重建 summary + pending records 对应的 `Value` 列表，并在工具较多或 pending records 较多时产生重复分配。`recent_record_limit` 属于保护性上限，目的是防止 summary 滞后或异常时 pending records 失控，不作为正常流程的主要性能假设。

### 3.2 修复方案

1. 将工具定义改为按请求共享：
   - `Arc<Vec<Value>>` 或 `Arc<[Value]>`
   - request handler 只在最终构建 payload 时需要所有权。
2. 将 stateless context 引入版本缓存：
   - cache key：`session_id + message_history_revision + runtime_prefix_revision + force_text + include_tools`
   - 消息未变化时不重建历史上下文，只 splice 当前 input / follow-up items。
3. `on_before_model_request` 和 `on_before_send_model_request` 默认只传摘要：
   - model
   - transport
   - payload_bytes
   - input_items_count
   - tools_count
   - fingerprint
   - 需要完整 payload 时通过 debug flag 显式开启。
4. 对 `append_input_items`、`rewrite_system_messages_to_user` 增加借用版本，避免在无变更场景 clone 整个 input。
5. Chatos 调 Memory Engine compose context 时显式传 `recent_record_limit`，建议先设为 200 或按 token budget 推导，作为异常保护；正常流程仍依赖 summary 消化旧消息。
6. Memory Engine compose response 增加 `recent_record_count`、`recent_record_limit`、`truncated` / `truncation_reason`，让 Chatos 能在日志和 UI 中识别上下文是否被截断。
7. 增加基准测试或轻量 benchmark：
   - 100 条历史消息
   - 1000 条历史消息
   - 50 个工具
   - 5 轮 tool-call

### 3.3 验证

- `cargo test -p chat_app_server_rs ai_client`
- 增加一次本地 benchmark，记录：
  - context rebuild ms
  - payload build ms
  - payload bytes
  - clone-free path 命中率
- 构造 1000 条总历史消息的正常场景：旧消息已 summary、少量消息 pending，验证模型请求只包含 summary + pending tail，不包含 1000 条完整历史。
- 构造 pending 堆积的异常保护场景，验证 Chatos 显式 `recent_record_limit` 生效，并输出截断标记。

## 4. P1 修复：AI payload 重复序列化

### 4.1 问题

`chatos/backend/src/services/ai_common/request_support/request_transport.rs` 会用 `serde_json::to_vec(payload)` 做预检。

随后 `crates/chatos_ai_runtime/src/request.rs` 在 `send_payload` 中再次 `serialize_request_payload(&payload)`，并再次校验大小。

接近上限的大请求会产生双倍序列化 CPU 和额外峰值内存。

### 4.2 修复方案

1. 移除上层 `validate_request_payload_size(&payload, ...)` 的完整序列化预检。
2. 将显式上限 `request_body_limit_bytes` 传入 shared runtime，保留 shared runtime 中的单次 `serialize_request_payload` + size check。
3. 如果仍需要在上层提前打日志，使用估算或 shared runtime 返回的 `payload_bytes`，不要提前 `to_vec`。
4. 统一错误文案，避免两个路径返回不同格式：
   - `AI request payload too large: {size} bytes exceeds {limit} bytes`

### 4.3 验证

- 保留现有 oversized payload 单测。
- 增加断言：一次请求只调用一次 payload serialization。
- 大 payload 请求失败时仍在发出网络请求前返回错误。

## 5. P1 修复：stdio MCP session 生命周期

### 5.1 问题

`crates/chatos_mcp_runtime/src/rpc.rs` 使用全局 `MCP_STDIO_SESSIONS` 保存 stdio MCP 子进程。

当前行为：

- 同一 server config 会复用 session。
- 请求失败、进程退出、超时会 remove。
- 但没有容量上限、空闲回收、后台清理。
- 并发冷启动时可能为同一个 config 同时 spawn 多个进程，最后只有一个进入 map，其他进程依赖 drop 清理。

### 5.2 修复方案

1. 将 session map 从 `HashMap<String, Arc<AsyncMutex<StdioRpcSession>>>` 扩展为带元数据的 entry：
   - `created_at`
   - `last_used_at`
   - `inflight`
   - `spawn_state`
2. 增加 per-key cold-start lock，避免同一 key 并发 spawn。
3. 增加容量限制：
   - 默认最多 32 个 stdio MCP session。
   - 超限时按 idle LRU 关闭。
4. 增加 idle TTL：
   - 默认 10 分钟无请求则关闭。
5. 提供显式清理 API：
   - 配置变更后按 key 清理。
   - 用户登出或 agent 配置变更后清理相关 session。
6. 工具调用日志增加：
   - `stdio_session_reused`
   - `stdio_session_spawned`
   - `stdio_session_evicted`
   - `stdio_session_timeout`

### 5.3 验证

- 并发 10 个相同 stdio server 的 `tools/list`，只 spawn 1 个子进程。
- idle TTL 到期后 session 被关闭。
- session 超限时最旧 idle session 被回收。

## 6. P2 修复：同步文件扫描与工具调用隔离

### 6.1 问题

代码导航、workspace search、project analyzer 多数已经有限制或使用 `spawn_blocking`。但仍存在多处同步文件读、WalkDir、同步子进程调用分散在内置工具和项目分析路径中。

重点路径：

- `chatos/backend/src/services/workspace_search/mod.rs`
- `chatos/backend/src/services/code_nav/**`
- `chatos/backend/src/services/project_run/analyzer.rs`
- `crates/chatos_builtin_tools/src/code_maintainer/**`

这些工具通常是用户显式触发，风险低于实时通道；但一旦被 agent 高频调用，仍可能抢占 CPU / IO。

### 6.2 修复方案

1. 建立统一 `BlockingIoExecutor`：
   - 限制并发，例如默认 4。
   - 所有 WalkDir、代码扫描、同步 patch/read 操作通过该 executor。
2. workspace search 增加 wall-clock deadline：
   - 默认 3 秒。
   - 返回 `truncated: true` 和原因。
3. 对文件扫描结果增加按 root + query + options 的短 TTL cache。
4. 对工具调用增加 per-session 并发限制：
   - 同一会话最多 2 个重 IO 工具。
5. 给工具返回增加 `truncated_reason`，避免 agent 误以为结果完整。

### 6.3 验证

- 大仓库 search 不阻塞普通 API 请求。
- 超时后返回部分结果，并标记 truncated。
- 连续并发工具调用不会把 blocking pool 打满。

## 7. P2 修复：终端输出处理细节

### 7.1 问题

本地和远端终端 history 已有限制：

- 本地 terminal snapshot：2MB / 10000 lines。
- 远端 terminal snapshot：按 bytes 限制。
- Task Runner terminal logs：保留 4000 条。

主要问题不在 history 无限增长，而在：

- WebSocket 出站无界队列。
- 单个 output chunk 没有统一最大大小。
- snapshot 拼接会创建一个完整 String，再按尾部行切分。

### 7.2 修复方案

1. P0 中先修 WebSocket bounded queue。
2. output chunk 入 history 前做最大 chunk 分片，例如 64KB。
3. `snapshot_tail_lines` 避免先拼接完整 2MB 字符串再倒扫：
   - 从 VecDeque 尾部按 chunk 逆序收集。
   - 达到行数/字节上限即停止。
4. 远端 terminal history 增加行数上限，与本地一致。

### 7.3 验证

- 执行 `yes | head -n 200000` 类大量输出，确认内存峰值稳定。
- 请求 1000 行 snapshot 时不拼接完整 history。

## 8. P3 修复：前端性能专项

### 8.1 当前判断

快速扫描未发现高置信无限循环或定时器泄漏。前端更可能的性能风险是大消息列表、大工具结果 JSON 展示、终端输出渲染和项目详情页大表格。

### 8.2 修复方案

1. 用 React Profiler 采集以下场景：
   - 长对话 500+ 消息。
   - 工具结果包含大 JSON。
   - 终端持续输出。
   - Project Detail 大量 requirements/work items。
2. 对高成本组件做局部修复：
   - 大消息列表虚拟滚动。
   - 工具 JSON 展示懒渲染，默认折叠。
   - 终端输出只交给 xterm buffer，不同步写入 React state。
   - Project Detail 表格后端分页或虚拟表格。
3. 给 realtime/store 更新增加批处理：
   - 高频 terminal/realtime event 合并到 animation frame。

## 9. 推荐实施顺序

第一批，低风险且收益最高：

1. WebSocket / SSE bounded queue。
2. AI payload 单次序列化。
3. stdio MCP cold-start 去重与 idle TTL。

第二批，降低长会话成本：

1. AI tools 使用共享引用。
2. stateless context 版本缓存。
3. callback 默认 payload 摘要化。

第三批，稳定重 IO 工具：

1. Blocking IO executor。
2. workspace search deadline。
3. 工具级并发限制。

第四批，按 profiler 结果修前端：

1. 长消息虚拟滚动。
2. 大 JSON 懒渲染。
3. 大表格分页/虚拟化。

## 10. 总体验证清单

后端：

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test -p chat_app_server_rs`
- `cargo test -p chatos_mcp_runtime`
- `cargo check --manifest-path user_service/backend/Cargo.toml`
- `cargo check --manifest-path memory_engine/backend/Cargo.toml`

前端：

- `npm test` 或现有测试命令。
- `npm run build`。
- 对主聊天页、终端页、项目详情页做手动回归。

压力验证：

- 慢 WebSocket 客户端 + 大量 terminal output。
- 长会话 1000 条总历史消息按正常流程发起模型请求，验证实际发送的是 Memory Engine summary + pending tail，而不是 1000 条完整历史。
- 50 个工具定义 + 多轮 tool-call。
- 并发 10 个同 stdio MCP server 工具调用。

## 11. 风险与回滚

- bounded queue 可能改变慢客户端体验，需要明确 slow-client 断开或丢弃策略。
- payload 摘要化可能影响调试，需要保留 debug 开关。
- stdio MCP idle 回收可能影响某些有状态 MCP server，需要允许按 server 配置关闭 idle TTL。
- stateless context 缓存必须绑定消息版本，否则可能发送旧上下文。

## 12. 实施进展：第二十二批

### 12.1 Project Plan 首屏轻量化

已完成：

1. Project Management `/api/projects/:project_id/plan` 保持默认全量兼容，新增 `include_work_items=false` summary 模式。
2. summary 模式只返回 requirements、需求级 dependency graph 和 work item 状态计数，不再序列化全项目 work items，也不再为全项目 work items 逐个加载依赖。
3. SQLite / Mongo store 新增按 `project_id,status` 的 work item 聚合计数，避免为了首屏统计把所有任务拉回内存。
4. Project Management 新增项目内路径 `/api/projects/:project_id/requirements/:requirement_id/work-items`，支持 `include_dependency_graph=true`，按选中 requirement 返回任务和任务依赖图。
5. Chat Server `/api/projects/:id/plan` 透传 `include_work_items`，并新增 `/api/projects/:id/requirements/:requirement_id/work-items` 代理。
6. 前端 ProjectPlanPane 改为首屏加载轻量 plan，选中 requirement 后按需加载并缓存其 work items；顶部统计优先使用后端 summary count。

验证：

- `cargo fmt --all`
- `cargo test -p project_management_service_backend --lib project_plan`
- `cargo check -p project_management_service_backend -p chat_app_server_rs`
- `cargo test -p chat_app_server_rs --lib project_service_body_limit`
- `npm run type-check`
- `npm run test -- --run src/lib/api/client/workspace/projects.test.ts src/components/projectExplorer/projectPlanPane/model.test.ts`

## 13. 实施进展：第二十三批

### 13.1 子进程输出收尾

已完成：

1. 新增通用 bounded process runner：子进程 stdout/stderr 分流读取、独立字节上限、超时 kill、`kill_on_drop`。
2. 远端 SSH fallback 从 `cmd.output()` 改为 bounded streaming，沿用 stdout 4MB / stderr 1MB 上限。
3. SCP upload/download fallback 从 `cmd.output()` 改为 bounded streaming，stdout/stderr 各 1MB，避免异常输出撑爆内存。
4. 技能插件 Git clone 从同步 `Command::output()` 改为 async bounded runner，并把旧缓存目录删除放入 blocking 线程。
5. 复扫主工程 `cmd.output()`，已无剩余命中。

验证：

- `cargo fmt --all`
- `rg -n "\\.output\\(\\)" chatos/backend/src crates project_management_service/backend/src -S`
- `cargo check -p chat_app_server_rs`
- `cargo test -p chat_app_server_rs --lib process_output`
- `cargo test -p chat_app_server_rs --lib remote_connections`
- `cargo test -p chat_app_server_rs --lib ssh_stream_limit`

## 14. 实施进展：第二十四批

### 14.1 HTTP 错误响应体边界

已完成：

1. Project Management 服务新增 bounded HTTP body helper，错误响应体预览限制为 16KB。
2. PM -> Task Runner 的 task 创建与 execution-options 请求错误体改为 bounded streaming 读取。
3. PM -> User Service 的 login/verify/agent-account 请求错误体改为 bounded streaming 读取。
4. AI runtime provider 错误响应体改为 16KB bounded streaming 读取，日志仍使用 preview。
5. 复扫 `response.text()` / `response.json()`，主工程已无剩余命中。

验证：

- `rg -n "response\\.text\\(\\)|response\\.json\\(" project_management_service/backend/src crates/chatos_ai_runtime/src chatos/backend/src -S`
- `cargo fmt --all`
- `cargo test -p project_management_service_backend --lib response_body_limit`
- `cargo test -p project_management_service_backend --lib task_runner`
- `cargo test -p chatos_ai_runtime --lib response_body_limit`
- `cargo check --workspace`

## 15. 实施进展：第二十五批

### 15.1 code-nav 单文件读取边界

已完成：

1. Chat Server code-nav 新增 `file_limits` 模块，统一限制单个源码文件读取上限为 2MB。
2. 符号索引和导航结果预览从“整文件读取后取一行”改为按行读取目标行。
3. fallback token 提取、fallback document symbols、Rust/Go/Java/Python/C/C++/C#/Kotlin 启发式分析均改为受限文件读取。
4. basic/rust/go/java/python 文本搜索遇到超限文件会跳过，不再把生成文件或超大源码整段读入内存。
5. 搜索预览从按字节切片改为按字符截断，避免中文等 UTF-8 内容在 400 字节边界 panic。

### 15.2 Task Runner HTTP body 边界

已完成：

1. Task Runner 新增 bounded HTTP body helper：
   - 错误响应预览默认 16KB。
   - JSON 成功响应默认 8MB。
   - 模型目录成功响应默认 4MB。
2. Task Runner -> User Service 的登录、鉴权、用户列表和 owner label hydrate 改为限量读取。
3. Task Runner -> Project Service 的 JSON / optional JSON 请求改为限量读取。
4. Chatos callback 和 ask-user callback 错误响应改为 16KB 预览。
5. Provider model catalog 非 2xx 响应只读错误预览，2xx 响应按模型目录上限读取再解析。

### 15.3 Task Runner SSH 输出边界

已完成：

1. Task Runner 远端 SSH 命令 stdout/stderr 从 `read_to_end` 改为分块读取。
2. stdout/stderr 均增加 512KB 硬上限，避免远端命令异常输出先撑满内存再被返回层截断。
3. 增加输出边界单元测试。

### 15.4 技能插件导入读取边界

已完成：

1. Chat Server 新增技能插件读取边界 helper：
   - 单个 markdown 文件 256KB。
   - `.claude-plugin/plugin.json` 256KB。
   - `marketplace.json` 1MB。
2. agents/commands/skills 的 markdown 提取改为限量读取，超限文件跳过，不影响其他插件文件。
3. 显式 marketplace path 超限时返回错误；默认递归发现的 marketplace 超限时跳过并走 fallback。
4. 插件根发现、markdown 收集、默认 marketplace 递归查找增加 20,000 entry 扫描预算。

### 15.5 code maintainer patch 边界

已完成：

1. `apply_patch` 保持原兼容入口，并新增 `apply_patch_limited`。
2. code maintainer 注册层调用 `apply_patch_limited`，复用工具配置里的 `max_write_bytes` 限制 patch 后目标文件大小。
3. patch 的 add/update/replace 都会检查输出大小，update/replace 读取原文件前也检查目标大小。
4. changelog 写入 hash 从 `std::fs::read` 整文件读入改为 64KB buffer 流式 SHA-256。

### 15.6 附件、截图和 notepad 收尾边界

已完成：

1. Chat Server 附件 DOCX 提取对 `word/document.xml` 增加 4MB 读取上限，避免压缩包内超大 XML 被完整读入内存。
2. 附件文本、PDF、DOCX 摘要统一改为按字符截断，避免 UTF-8 字节边界截断导致 panic 或乱码。
3. Browser vision 截图转 base64 前增加 10MB 文件大小上限，避免异常截图文件一次性读入和编码放大。
4. Chat Server notepad 增加内容文件 1MB、索引文件 4MB 上限；读取、重建索引、写入 note/index 都走限量 helper。
5. Task Runner notepad 增加内容文件 1MB、索引文件 4MB 上限；创建、读取、更新、搜索和索引持久化均走限量 helper。

验证：

- `cargo fmt --all`
- `cargo test -p chat_app_server_rs --lib code_nav`
- `cargo test -p chat_app_server_rs --lib chatos_skills_file_limits`
- `cargo test -p chat_app_server_rs --lib notepad`
- `cargo test -p task_runner_service_backend --lib http_body`
- `cargo test -p task_runner_service_backend --lib ssh_output_limit`
- `cargo test -p task_runner_service_backend --lib notepad`
- `cargo test -p chatos_builtin_tools --lib code_maintainer::patch`
- `cargo check --workspace`
- `rg -n "response\\.text\\(\\)|response\\.json\\(" task_runner_service/backend/src project_management_service/backend/src crates/chatos_ai_runtime/src chatos/backend/src -S`
- `rg -n "read_to_end\\(" task_runner_service/backend/src chatos/backend/src/services/code_nav chatos/backend/src/services/chatos_skills_file_limits.rs chatos/backend/src/utils/attachments.rs -S`
- `rg -n "fs::read_to_string\\(|std::fs::read_to_string\\(|tokio::fs::read\\(" chatos/backend/src/services/notepad task_runner_service/backend/src/notepad_store chatos/backend/src/utils/attachments.rs chatos/backend/src/services/shared_builtin_browser_tools/support.rs -S`
- `git diff --check`
