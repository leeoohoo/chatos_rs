# 项目优化进度（2026-06-16）

## 执行约定

- 按 `PROJECT_REVIEW_20260616.md` 的优先级推进。
- 不启动本地服务，不执行端到端测试。
- 每轮优化完成前至少执行编译/静态编译类检查，并记录结果。
- 每轮都维护本文档。

## 第 1 轮：P0 入口漂移与运行时崩溃点

状态：已完成

目标：
- 移除缺失 `openai-codex-gateway` 对 README、Makefile、CI、Dependabot 和热点预算脚本的影响。
- 修复 `scripts/check-hotspot-line-budgets.sh` 中已经不存在的旧路径。
- 将 memory engine 获取上下文失败时的 `panic!` 改为可降级行为。
- 清理当前非测试 Rust 代码中几个低成本 `unwrap/expect` 命中。

计划检查：
- `bash scripts/check-hotspot-line-budgets.sh`
- `python3 scripts/check-non-test-unwrap-expect.py`
- `bash scripts/check-request-path-panics.sh`
- `cargo check`

进展：
- 已创建本进度文档。
- 已移除 README、Makefile、CI、Dependabot 和治理脚本中的 `openai-codex-gateway` 旧入口。
- 已移除 README 中缺失的 `SYSTEM_BUILD_MATRIX.md` 链接。
- 已刷新热点预算脚本：删除不存在路径，并将两个仍存在热点文件的预算更新到当前基线。
- 已将 `message_manager_common.rs` 中 memory engine 上下文获取失败的 `panic!` 改为返回空 `ChatHistoryContext` 降级。
- 已清理 `message_handlers.rs`、`workspace_realtime_watcher.rs`、`test_support.rs` 中被治理脚本命中的 `unwrap/expect`。
- 已剔除本轮格式化命令带来的无关文件改动，只保留本轮目标范围内的变更。

验证结果：
- `bash scripts/check-hotspot-line-budgets.sh`：通过。
- `python3 scripts/check-non-test-unwrap-expect.py`：通过。
- `bash scripts/check-request-path-panics.sh`：通过。
- `cargo check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 2 轮：收口 builtin 与 shared crate 的低风险重复

状态：已完成

目标：
- 优先处理 `chat_app_server_rs/src/builtin` 与 `crates/chatos_builtin_tools` 中已经完全重复或仅作为兼容层存在的代码。
- 保留 Chatos server 专属 adapter/store，不做大范围迁移。
- 继续保持只编译、不启动、不跑测试。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已开始梳理 `code_maintainer` 相关重复文件。
- 已确认 `chat_app_server_rs/src/builtin/code_maintainer/storage.rs` 是 Chatos server 专属 DB/Mongo/实时事件存储，暂不迁移。
- 已在 `crates/chatos_builtin_tools/src/code_maintainer/mod.rs` 导出 `generate_id`、`now_iso`、`resolve_state_dir`。
- 已将 server 专属 `storage.rs` 改为复用 shared crate 的通用工具函数。
- 已删除 server 侧与 shared crate 完全重复的 `code_maintainer/utils.rs`。
- 已删除 server 侧与 shared crate 完全重复的 `code_maintainer/tests.rs`，测试覆盖保留在 `crates/chatos_builtin_tools`。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 3 轮：下一批重复代码侦查

状态：已完成

目标：
- 评估 `tool_registry`、`tool_call`、`parallelism`、`stream` 等报告中提到的重复代码是否适合继续低风险收口。
- 不在风险不清楚时做跨模块大替换。

进展：
- 已查看 `chat_app_server_rs/src/core/tool_registry.rs` 与 `crates/chatos_builtin_tools/src/tool_registry.rs`，两者结构高度相似，但 shared crate 版本内置了 `block_on_result`、`text_result`，server 版本依赖本地 `async_bridge` 和 `tool_io`；可以作为下一轮候选，需要先确认所有调用点。
- 已查看 `chat_app_server_rs/src/services/mcp_execution_core/parallelism.rs` 与 `crates/chatos_mcp_runtime/src/parallelism.rs`，两者策略相似，但 server 版本暴露了更多 `pub(crate)` 内部函数并耦合本地 `ToolInfo`，直接替换风险较高。
- 已确认本轮不修改代码，保持第 2 轮的编译通过边界。

建议下一轮：
- 优先评估 `chat_app_server_rs/src/core/tool_registry.rs` 是否能改为 re-export shared `ToolRegistry`，或反向把通用 registry 下沉到 `chatos_mcp_runtime`。
- 暂缓 `parallelism` 迁移，除非先补齐类型适配和行为对照。

验证结果：
- 本轮只做侦查，未产生代码变更；沿用第 2 轮 `cargo check` 通过结果。

备注：
- 未启动项目。
- 未执行测试。

## 第 4 轮：删除 server 侧未使用的 tool_registry 旧副本

状态：已完成

目标：
- 收口 `chat_app_server_rs/src/core/tool_registry.rs` 与 `crates/chatos_builtin_tools/src/tool_registry.rs` 的重复。
- 优先删除无调用点的迁移残留，不做跨模块大替换。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `chat_app_server_rs/src/core/tool_registry.rs` 除 `core/mod.rs` 导出外没有真实调用点。
- 已删除 `chat_app_server_rs/src/core/tool_registry.rs`。
- 已从 `chat_app_server_rs/src/core/mod.rs` 移除 `tool_registry` 模块声明。
- shared crate 内仍保留 `crates/chatos_builtin_tools/src/tool_registry.rs`，供 builtin tools 使用。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 5 轮：收窄 core/tool_call 兼容层

状态：已完成

目标：
- 保留 `chat_app_server_rs/src/core/tool_call.rs` 作为 shared runtime 的兼容 re-export。
- 删除 server 侧与 `crates/chatos_ai_runtime/src/tool_call.rs` 重复的测试代码。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `core/tool_call.rs` 生产代码是 re-export，仍被 server 多处调用。
- 已确认本地 `extract_message_tool_calls_from_value` / `message_has_tool_calls` 只在该文件测试中使用。
- 已删除 `core/tool_call.rs` 中与 `crates/chatos_ai_runtime/src/tool_call.rs` 重复的测试代码。
- 已保留 `core/tool_call.rs` 作为 shared runtime 的兼容 re-export，避免改动调用点。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 6 轮：chat_stream 文本合并逻辑侦查

状态：已完成

目标：
- 评估 `chat_app_server_rs/src/core/chat_stream/text.rs` 是否能复用 `chatos_ai_runtime::response_parse::join_stream_text`。

进展：
- 已确认两处函数同名同类，但行为不完全一致。
- server 版本只合并 8 字符以上重叠；shared runtime 版本会合并 1 字符以上重叠。
- 该差异可能影响短文本流片段的拼接行为，因此本轮暂缓替换。

验证结果：
- 本轮只做侦查，未产生代码变更。

备注：
- 未启动项目。
- 未执行测试。

## 第 7 轮：清理根目录临时文档与误跟踪截图

状态：已完成

目标：
- 处理审查报告中提到的 `SDK_USAGE copy.md` 临时文件名问题。
- 删除未被代码或文档入口引用的 `chat_app_server_rs/base64` 截图文件。

计划检查：
- `git diff --check`
- `cargo check`

进展：
- 已确认 `SDK_USAGE copy.md` 是唯一 SDK 使用文档，不是重复副本。
- 已将 `SDK_USAGE copy.md` 重命名为 `SDK_USAGE.md`，并保留原有 SDK 使用内容。
- 已确认 `chat_app_server_rs/base64` 是 1324x1001 PNG 截图，除审查报告外没有引用。
- 已删除未引用的 `chat_app_server_rs/base64`。

验证结果：
- `git diff --check`：通过。
- `cargo check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 8 轮：收缩 server builtin 迁移期空壳模块

状态：已完成

目标：
- 删除 `chat_app_server_rs/src/builtin` 下仅做 re-export、且 server 内部已不再引用的迁移期空壳模块。
- 保留仍承载 Chatos 专属适配/store 的 builtin 模块。
- 将 `browser_tools` 兼容层里的测试搬到 shared crate，避免删除空壳模块时丢失覆盖。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 server 内部实际直接使用 `chatos_builtin_tools` 中的 agent、browser、memory reader、notepad、task、ui prompt、web tool 类型。
- 已保留 `code_maintainer`、`terminal_controller`、`remote_connection_controller` 三个仍有 server 专属逻辑的模块。
- 已从 `builtin/mod.rs` 移除未使用空壳模块声明。
- 已删除对应 re-export-only 模块文件。
- 已将 `browser_tools` 相关测试移动到 `crates/chatos_builtin_tools`。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 9 轮：收窄 code_maintainer 兼容层导出

状态：已完成

目标：
- 让 server 侧 `builtin/code_maintainer` 只暴露仍被本地使用的 `ChangeLogStore`。
- 删除已经改为直接从 `chatos_builtin_tools` 引用的 shared service re-export。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `chat_app_server_rs` 中只有 `workspace_realtime_watcher` 仍通过 `crate::builtin::code_maintainer` 使用 `ChangeLogStore`。
- 已移除 `CodeMaintainerOptions` / `CodeMaintainerService` 的兼容 re-export。
- 已移除对应的 `allow(unused_imports)`。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 10 轮：抽取 stream 文本拼接公共算法

状态：已完成

目标：
- 收口 `chat_app_server_rs/src/core/chat_stream/text.rs` 与 `crates/chatos_ai_runtime/src/response_parse.rs` 中重复的文本拼接算法。
- 保留两边现有行为差异：shared runtime 最小重叠长度为 1，server chat stream 最小重叠长度为 8。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已在 `chatos_ai_runtime::response_parse` 中新增 `join_stream_text_with_min_overlap`。
- 已将原 `join_stream_text` 改为调用公共 helper，保持最小重叠 1。
- 已将 server `chat_stream` 的本地重复实现改为调用公共 helper，传入最小重叠 8。
- 已增加一条 helper 阈值语义的单元测试代码，作为后续测试覆盖入口。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 11 轮：集中 stream 拼接算法测试覆盖

状态：已完成

目标：
- 删除 `request.rs` 和 `tool_call.rs` 中重复验证 `join_stream_text` unicode overlap 的测试断言。
- 将文本拼接算法语义集中保留在 `response_parse.rs` 的测试中。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已删除 `request.rs` 中重复的 `join_stream_text_handles_unicode_snapshot_overlap` 测试。
- 已删除 `tool_call.rs` 中工具调用合并测试里附带的重复 `join_stream_text` 断言。
- 已清理对应的测试模块导入。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 12 轮：清理 builtin tools 剩余死代码

状态：已完成

目标：
- 清理 `chatos_builtin_tools` 中强制 dead_code 探测发现的本仓库未使用项。
- 继续收紧 shared builtin crate 的迁移期残留。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认强制 dead_code 探测的大部分输出来自第三方依赖，不作为本轮治理范围。
- 已删除 browser actions 中未使用的 `DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS` 转发常量。
- 已删除 browser actions config 中未使用的同名常量。
- 已删除 `tool_registry` 中未使用的 `async_text_tool_handler_with_optional_string`。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。
- `RUSTFLAGS='--force-warn dead_code' cargo check -p chatos_builtin_tools` 仅作为侦查使用，输出依赖警告过多，后续不纳入常规检查。

## 第 13 轮：移除 builtin tools 的 dead_code 总开关

状态：已完成

目标：
- 移除 `crates/chatos_builtin_tools/src/lib.rs` 顶层 `#![allow(dead_code)]`。
- 让 shared builtin crate 后续迁移残留能被普通编译警告暴露。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `crates/chatos_builtin_tools/src` 中只剩 crate 顶层 dead_code allow。
- 已移除 `chatos_builtin_tools` 顶层 `#![allow(dead_code)]`。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 14 轮：收窄 server builtin store 适配模块导出

状态：已完成

目标：
- 删除 `terminal_controller` 与 `remote_connection_controller` 适配模块中未使用的 shared crate re-export。
- 保留本地仍承担 Chatos store 适配职责的结构和实现。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 server 内部对 terminal/remote controller service 类型的调用点都直接从 `chatos_builtin_tools` 导入。
- 已移除 `remote_connection_controller/mod.rs` 中未使用的 shared service/options/constants re-export。
- 已移除 `terminal_controller/mod.rs` 中未使用的 shared service/options/constants/helper re-export。
- 已将 `actions_process.rs` 对 `PROCESS_WAIT_MAX_TIMEOUT_MS` 的内部依赖改为直接从 `chatos_builtin_tools` 导入。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 15 轮：移除 shared runtime bridge 的 dead_code 总开关

状态：已完成

目标：
- 移除 `shared_mcp_runtime.rs` 与 `shared_ai_runtime.rs` 顶层 `#![allow(dead_code)]`。
- 保留 bridge 文件中实际被调用的转换与适配逻辑。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 shared MCP bridge 的转换函数、registry 构建和结果转换函数均有调用点。
- 已确认 shared AI bridge 的 runtime builder、context runner、model request/config 转换均有调用点。
- 已移除两个 bridge 文件顶层 dead_code allow。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 16 轮：删除 sessions history 的类型锚点残留

状态：已完成

目标：
- 删除 `api/sessions/history.rs` 中只为压制 dead_code 而存在的 `_type_anchor`。
- 移除对应的未使用 `Message` 导入。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `_type_anchor` 没有调用点，只用于接受 `&[Message]`。
- 已删除 `_type_anchor` 和 `Message` 导入。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 17 轮：删除 auth 注册请求的未用字段

状态：已完成

目标：
- 删除 `RegisterRequest.display_name` 未用字段。
- 移除对应的局部 `#[allow(dead_code)]`。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认注册逻辑只使用 username/email/password。
- 已删除 `display_name` 字段和对应 dead_code allow。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 18 轮：收窄 UI prompt choice limit normalizer

状态：已完成

目标：
- 删除 server 内部 `LimitMode::Strict` 未使用分支。
- 让 `normalize_choice_limits` 只保留当前实际调用的 clamp 行为。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 server 侧 `normalize_choice_limits` 只有 `LimitMode::Clamp` 调用路径。
- 已删除 server 内部 `LimitMode` 枚举和 Strict 分支。
- 已更新 `submission.rs` 调用和 normalizer re-export。
- shared builtin crate 中还有类似公开模块实现，本轮暂不改动，避免扩大公开 API 影响面。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 19 轮：移除 ToolInfo 过期 dead_code 标注

状态：已完成

目标：
- 移除 `ToolInfo.tool_info` 字段上的过期 `#[allow(dead_code)]`。
- 保留字段本身，因为运行时快照仍使用它提取工具描述。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `turn_runtime_snapshot` 会读取 `ToolInfo.tool_info`。
- 已删除字段上的 dead_code allow。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 20 轮：删除未接入的 UI prompt 按 ID 读取 helper

状态：已完成

目标：
- 删除没有调用点的 `get_ui_prompt_record_by_id`。
- 移除对应 re-export，减少未接入 API 面。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认 `get_ui_prompt_record_by_id` 只在自身和 re-export 链上出现。
- 已删除该函数及 `store.rs`、`ui_prompt_manager/mod.rs` 中对应导出。

验证结果：
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 21 轮：收窄 shared UiPrompter choice limit normalizer

状态：已完成

目标：
- 删除 `chatos_builtin_tools::ui_prompter` 中未使用的 `LimitMode::Strict` 分支。
- 保留当前实际调用的 clamp 行为。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 shared `UiPrompter` 中 `normalize_choice_limits` 的两个调用点都使用 clamp 行为。
- 已删除 `LimitMode` 枚举和 strict 分支。
- 已同步更新两个调用点和函数签名。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 22 轮：收窄 UI prompt manager 顶层导出

状态：已完成

目标：
- 删除 `ui_prompt_manager/mod.rs` 中宽泛 re-export 带来的 `allow(unused_imports)`。
- 只保留外部模块真实使用的 UI prompt manager 入口。

计划检查：
- `cargo check`
- `git diff --check`

进展：
- 已确认顶层 re-export 主要被 `shared_builtin_ui_prompter` 和 builtin 配置使用。
- 已保留 prompt 创建/等待、记录创建/更新、响应脱敏、共享类型和超时常量/错误码。
- 已移除未被外部使用的 normalizer、read store 和 record/not_found 等宽出口。
- 已删除对应 `allow(unused_imports)`。
- 已继续收窄 `normalizer.rs` 和 `store.rs` 的内部 re-export，消除编译器暴露的 unused import warnings。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。

## 第 23 轮：收窄 task_manager 顶层导出

状态：已完成

目标：
- 删除 `task_manager/mod.rs` 中宽泛 re-export 带来的 `allow(unused_imports)`。
- 保留 API、shared builtin task manager 和测试实际需要的出口。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认非测试调用点需要任务 CRUD、review 创建/等待、任务 DTO、review timeout 和 task not found 常量。
- 已移除未接入的 review payload 查询导出和未使用常量/类型导出。
- 已将 `submit_task_review_decision` 限定为测试目标导出。
- 已删除对应 `allow(unused_imports)`。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 24 轮：收窄 db 根模块导出

状态：已完成

目标：
- 删除 `db/mod.rs` 中宽泛 re-export 带来的 `allow(unused_imports)`。
- 保留当前运行时和仓储层实际使用的数据库入口。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认项目内直接从 `crate::db` 使用的是 `get_db`、`get_db_sync`、`get_factory`、`init_global` 和 `Database`。
- 已移除未被项目内调用的配置类型和 `DatabaseFactory` 顶层 re-export。
- 已删除对应 `allow(unused_imports)`。
- 已将仅测试支持代码使用的 `get_factory` 调整为 `#[cfg(test)]` 导出，普通编译路径不再产生 unused import warning。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 25 轮：收窄 core tool_call 兼容层

状态：已完成

目标：
- 删除 `core/tool_call.rs` 中宽泛 re-export 带来的 `allow(unused_imports)`。
- 仅保留服务端实际通过 `crate::core::tool_call` 使用的工具调用 helper。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认服务端调用面只需要 `build_function_call_output_item`、`clone_tool_call_arguments`、`extract_message_tool_calls`、`extract_tool_call_id`、`extract_tool_call_name` 和 `tool_calls_value_has_items`。
- 已移除未使用的 stream text、tool call 构造、增量合并、索引解析等宽出口。
- 已删除对应 `allow(unused_imports)`。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 26 轮：拆分 chat_runtime 生产导出与测试辅助符号

状态：已完成

目标：
- 删除 `core/chat_runtime.rs` 中宽泛 re-export 带来的 `allow(unused_imports)`。
- 将联系人命令解析等测试辅助符号限制在测试目标可见，生产路径只保留真实调用入口。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认生产代码通过 `crate::core::chat_runtime` 使用联系人系统提示、运行时 metadata 解析、ID 归一化和项目运行时解析。
- 已将联系人命令解析、命令提示构造和 reader 工具名常量改为仅测试目标从内部 contact 模块导出。
- 已收窄根模块 metadata re-export，移除未被外部调用的 `metadata_bool` 和 `metadata_string_list`。
- 已删除对应 `allow(unused_imports)`。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 27 轮：删除未接入的 session_summary_job 模块

状态：已完成

目标：
- 删除 `modules/session_summary_job` 中未被引用的默认配置类型。
- 移除对应整文件 `dead_code` allow 和空壳模块入口。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `SummaryJobDefaults` 和相关常量只在自身文件内出现。
- 已确认 `modules/session_summary_job` 仅通过 `modules/mod.rs` 挂载，没有业务调用点。
- 已删除 `modules/session_summary_job/mod.rs` 和 `types.rs`，并移除 `modules/mod.rs` 中的模块声明。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 28 轮：移除运行时结果中的未读取 effective_user_id 字段

状态：已完成

目标：
- 删除 `ResolvedConversationRuntimeContext` 中未被读取的 `effective_user_id` 字段。
- 移除对应字段级 `dead_code` allow。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `effective_user_id` 仍作为函数内部变量参与联系人、项目和远程运行时解析。
- 已确认 `ResolvedConversationRuntimeContext.effective_user_id` 没有读取点，且不是 serde/API DTO 字段。
- 已删除结果结构体字段和构造赋值。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 29 轮：收窄任务 follow-up 指令结构体

状态：已完成

目标：
- 删除 `TaskTurnFollowUpDirective` 中仅测试断言读取的统计字段。
- 移除对应字段级 `dead_code` allow。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认生产调用面只读取 `mode`、`locale` 和 `guidance`。
- 已确认未完成、阻塞、完成数量已经写入 `guidance` 文本。
- 已删除 `unfinished_count`、`blocked_count` 和 `done_count` 字段。
- 已将对应测试断言调整为检查 `guidance` 中的统计文本。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 30 轮：移除 db 模块级 dead_code 兜底

状态：已完成

目标：
- 删除 `db/mod.rs` 顶部的整模块 `#![allow(dead_code)]`。
- 让数据库模块依靠真实调用面和明确导出通过编译。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `db/mod.rs` 当前只保留数据库初始化、获取和 `Database` 类型出口。
- 已删除模块级 `dead_code` allow。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 31 轮：移除 utils 模块级 dead_code 兜底

状态：已完成

目标：
- 删除 `utils/mod.rs` 顶部的整模块 `#![allow(dead_code)]`。
- 确认工具模块集合不再需要顶层死代码豁免。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `utils/mod.rs` 仅负责挂载工具子模块。
- 已删除模块级 `dead_code` allow。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 32 轮：移除 agent_runtime 模块级 dead_code 兜底

状态：已完成

目标：
- 删除 `services/agent_runtime/mod.rs` 顶部的整模块 `#![allow(dead_code)]`。
- 确认 agent runtime 聚合模块不再需要顶层死代码豁免。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认该文件只负责挂载 `ai_client`、`ai_request_handler`、`ai_server`、`mcp_tool_execute` 和 `message_manager`。
- 已删除模块级 `dead_code` allow。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 33 轮：移除 repositories 模块级 dead_code 兜底

状态：已完成

目标：
- 删除 `repositories/mod.rs` 顶部的整模块 `#![allow(dead_code)]`。
- 确认仓储聚合层不再需要顶层死代码豁免。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认该文件只负责挂载各仓储子模块。
- 已删除模块级 `dead_code` allow。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 34 轮：移除 models 模块级 dead_code 兜底

状态：已完成

目标：
- 删除 `models/mod.rs` 顶部的整模块 `#![allow(dead_code)]`。
- 避免模型聚合层吞掉未来更具体的 DTO 死代码信号。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认该文件只负责挂载各模型子模块。
- 已删除模块级 `dead_code` allow。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 35 轮：移除未生效的 LOG_MAX_SIZE 配置

状态：已完成

目标：
- 删除配置中读取但未被 logger 使用的 `LOG_MAX_SIZE`。
- 移除 `config.rs` 顶部的整模块 `#![allow(dead_code)]`。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 logger 当前只使用 `LOG_LEVEL` 和 `LOG_MAX_FILES`，没有使用 `LOG_MAX_SIZE`。
- 已删除 `Config.log_max_size` 字段、环境变量读取和测试构造中的字段赋值。
- 已删除部署 env 示例中的 `LOG_MAX_SIZE=10m`。
- 已删除 `config.rs` 模块级 `dead_code` allow。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 36 轮：移除 crate 根 dead_code 兜底

状态：已完成（评估后暂缓）

目标：
- 删除 `lib.rs` 顶部的 crate 级 `#![allow(dead_code)]`。
- 让服务端库不再全局压制死代码提示。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认前序轮次已清理模块级和字段级 `dead_code` 标注。
- 已尝试删除 crate 根 `dead_code` allow。
- `cargo check` 通过，但暴露约 152 个具体 dead code 警告，说明当前还不适合直接移除全局兜底。
- 已恢复 crate 根 `dead_code` allow，避免普通编译输出变脏。
- 后续改为按具体警告源逐块清理，再重新评估 crate 根兜底。

验证结果：
- `cargo check`：通过，但直接移除时产生大量 dead code warnings。

备注：
- 未启动项目。
- 未执行测试。

## 第 37 轮：将联系人命令解析限制为测试目标

状态：已完成

目标：
- 减少 crate 根 dead_code 兜底暴露出的生产编译面。
- 将当前只由单元测试使用的联系人命令解析和命令 prompt 构造改为 `#[cfg(test)]`。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认生产路径只使用联系人系统 prompt 和 `ContactSkillPromptMode::Disabled`。
- 已将 `chat_runtime_contact/command_parser.rs` 模块限制为测试目标编译。
- 已将 `compose_contact_command_system_prompt` 和对应命令解析 DTO 限制为测试目标编译。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 38 轮：收窄 API 测试专用 helper 与 callback DTO

状态：已完成

目标：
- 减少 crate 根 `dead_code` 兜底暴露出的 API 层死代码警告。
- 将仅单元测试使用的 helper 限制为测试目标编译。
- 删除 task runner callback 请求 DTO 中未消费的字段。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已将 `build_task_runner_callback_assistant_message` 标记为 `#[cfg(test)]`，生产路径继续使用带 contact display 的版本。
- 已将 `normalize_message_task_graph_payload_edges` 标记为 `#[cfg(test)]`，生产路径继续使用带任务补全参数的版本。
- 已将 `ensure_message_turn_id` 和 `parse_compact_history_offset` 标记为 `#[cfg(test)]`。
- 已删除 `TaskRunnerCallbackRequest` 中未读取的 `process_log` 和 `prerequisite_task_ids` 字段；serde 仍会忽略请求里的额外字段。
- 已同步更新测试构造 payload。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 39 轮：删除未接入的 core helper 模块

状态：已完成

目标：
- 删除服务端 `core` 中没有调用点的 `async_bridge` 和 `tool_io` 模块。
- 减少与 `chatos_builtin_tools::tool_registry` 中实际使用 helper 的重复概念。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `chat_app_server_rs/src/core/async_bridge.rs` 只在 `core/mod.rs` 中挂载，没有服务端调用点。
- 已确认 `chat_app_server_rs/src/core/tool_io.rs` 只在 `core/mod.rs` 中挂载，没有服务端调用点。
- 已删除两个文件并移除对应模块声明。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 40 轮：收窄 core mcp_runtime helper 面

状态：已完成

目标：
- 删除 `core/mcp_runtime.rs` 中未接入的联系人 reader server 构造函数。
- 将仅单元测试使用的 MCP 选择 helper 限制为测试目标编译。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认生产调用面只需要 `McpServerBundle`、`empty_mcp_server_bundle` 和 `load_mcp_servers_by_selection`。
- 已删除未被调用的 `contact_agent_*_reader_server` 三个构造函数。
- 已将 `normalize_mcp_ids` 和 `has_any_mcp_server` 标记为 `#[cfg(test)]`。
- 已移除对应不再需要的 builtin MCP 常量和 kind import。
- 已同步删除 `services/builtin_mcp.rs` 中无人使用的 memory reader server name re-export。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 41 轮：收窄 core messages 旧 helper

状态：已完成

目标：
- 删除无调用点的按 message id 更新 task runner async 状态 helper。
- 将仅测试使用的文本 helper 限制为测试目标编译。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认生产调用面使用 `set_task_runner_async_overall_status_for_session`，它带 session 约束，替代了旧的 `set_task_runner_async_overall_status_by_id`。
- 已删除无调用点的 `set_task_runner_async_overall_status_by_id`。
- 已将 `owned_non_empty_text` 标记为 `#[cfg(test)]`。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 42 轮：清理 remote_connection_controller 旧上下文 helper

状态：已完成

目标：
- 删除 `remote_connection_controller` 中无调用点的旧 JSON 参数解析 helper。
- 移除 `BoundContext.server_name` 未读字段。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `required_trimmed_string`、`optional_trimmed_string`、`optional_u64`、`optional_usize` 和 `optional_bool` 没有调用点。
- 已确认 `BoundContext.server_name` 仅被构造，没有读取点。
- 已删除上述 helper、字段和对应测试构造赋值。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 43 轮：清理 terminal_controller 旧参数 helper

状态：已完成

目标：
- 删除 `terminal_controller/context.rs` 中无调用点的旧 JSON 参数解析 helper。
- 减少 crate 根 `dead_code` 兜底暴露出的 builtin 层警告。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `required_string` 和 `required_trimmed_string` 没有调用点。
- 已删除两个 helper 和不再需要的 `serde_json::Value` import。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 44 轮：删除未接入的 runtime guidance submit 入口

状态：已完成

目标：
- 删除 `conversation_runtime/guidance.rs` 中无路由、无调用点的 submit API 入口。
- 保留当前实际使用的 runtime guidance enqueue、drain、message content 和 applied event 路径。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `submit_runtime_guidance`、`SubmitRuntimeGuidanceInput`、`SubmitRuntimeGuidanceOutput`、`SubmitRuntimeGuidanceError` 只在自身文件中出现。
- 已删除 submit 入口及其专属的附件归一化、显示内容构造和 session 权限读取 helper。
- 已清理不再需要的 auth、session access、abort registry、metadata 构造和 warning import。
- 已同步移除 `services/ai_common.rs` 中不再对外使用的 `build_user_message_metadata` re-export。
- 已继续移除 `services/ai_common/request_support/mod.rs` 中对应的父模块 re-export，保留 `user_message.rs` 内部实际使用。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 45 轮：删除未接入的 conversation user message 创建入口

状态：已完成

目标：
- 删除 `conversation_runtime/messages.rs` 中已无调用点的 user message 创建 facade。
- 保留该模块中仍被 API 和 memory compat 使用的 message 读写/同步入口。

计划检查：
- `cargo check`
- `cargo check --tests`
- `git diff --check`

进展：
- 已确认 `CreateUserMessageInput` 和 `create_user_message` 只在自身文件中出现。
- 已删除这两个未接入入口。

验证结果：
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试，只计划编译测试目标。

## 第 46 轮：修复用户消息偶发空列表

状态：已完成

目标：
- 修复项目“用户消息”面板偶发显示“暂无用户消息”，但刷新/切换后又能看到消息的问题。
- 保持用户要求：不启动项目，不执行测试，只做编译/类型检查。

进展：
- 已定位用户消息面板数据源为 `getConversationUserMessageTurns` / `/user-message-turns`。
- 已在后端 `get_session_user_message_turns` 增加兜底：compact turns 首次/分页结果为空时，从会话消息表按用户轮次构建响应，避免 compact 索引冷启动或同步延迟时误报空列表。
- 已在前端 `useConversationUserMessages` 中对 session 切换显式失效旧请求并重置分页状态，减少切换联系人/项目时旧响应穿插导致的错误空态。

计划检查：
- `rustfmt`
- `cargo check`
- `cargo check --tests`
- `npm run type-check`（chat_app）
- `git diff --check`

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/api/sessions/message_handlers.rs`：通过。
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `npm run type-check`（chat_app）：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 47 轮：修复从空会话切到有消息会话仍显示空态

状态：已完成

目标：
- 针对复现条件“从本来没有消息的会话切换到有消息的会话”继续修复。
- 保持用户要求：不启动项目，不执行测试，只做编译/类型检查。

进展：
- 已定位到 `useTeamMemberConversation` 的同步 effect 会在联系人切换过程中，用仍未更新的旧 `currentSession` 把 `selectedContactId/selectedSessionId` 改回旧空会话。
- 已在联系人切换中阻止该同步 effect 覆盖用户刚选择的新联系人。
- 已让 `selectedProjectSession` 只接受属于当前选中联系人的 session，避免切换瞬间继续把旧空会话暴露给用户消息侧栏。

计划检查：
- `npm run type-check`（chat_app）
- `cargo check`
- `cargo check --tests`
- `git diff --check`

验证结果：
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 48 轮：修复后端请求命中空 session

状态：已完成

目标：
- 针对“请求已发出，但后端返回空”的继续排查结果，修复实际选择到空 session 的问题。
- 保持用户要求：不启动项目，不执行测试，只做编译/类型检查。

进展：
- 已确认前端会话列表会按同联系人/项目归一化，仅保留最新 session；如果最新 session 为空，旧的有消息 session 可能不在本地列表中。
- 已调整 `useContactSessionResolver.ensureContactSession`：点击联系人时优先通过后端候选列表和 `getConversationMessages(limit: 1)` 预览选择有消息的历史 session，再回退到本地/缓存 session。
- 已在联系人切换过程中暂不暴露本地临时 session，避免侧栏先对最新但空的 session 发起 `user-message-turns` 请求。
- 继续保留第 46 轮后端 `user-message-turns` 的普通消息表兜底，防止 compact turns 索引短暂为空。

计划检查：
- `npm run type-check`（chat_app）
- `cargo check`
- `cargo check --tests`
- `git diff --check`

验证结果：
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 49 轮：修复 user-message-turns 兜底读取空结果

状态：已完成

目标：
- 按“sessionId 参数正确但后端返回空”的观察，继续检查后端读取路径。
- 保持用户要求：不启动项目，不执行测试，只做编译/类型检查。

进展：
- 已确认 `user-message-turns` 在 compact turns 为空时，会通过 `list_all_session_messages` 从普通消息记录构建用户消息 turns。
- 已发现普通聊天消息接口优先使用 desc 最近页读取，而 `list_all_session_messages` 原先使用 asc 全量分页；这会让 user-message-turns 兜底路径与主消息路径不一致。
- 已将 `conversation_runtime::messages::list_all_messages` 改为 desc 分页读取后整体反转，保持上层时间正序不变，同时与普通消息接口的可靠读取方向一致。

计划检查：
- `rustfmt --edition 2024 chat_app_server_rs/src/modules/conversation_runtime/messages.rs`
- `cargo check`
- `cargo check --tests`
- `npm run type-check`（chat_app）
- `git diff --check`

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/modules/conversation_runtime/messages.rs`：通过。
- `cargo check`：通过。
- `cargo check --tests`：通过。
- `npm run type-check`（chat_app）：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 50 轮：收敛联系人会话解析的重复请求

状态：已完成

目标：
- 按用户反馈“刷新后切换又能查到”和前端 Network 面板中 `conversations` / `messages?limit=1&compact=false` 疯狂重复调用的现象，继续定位当前业务链路。
- 不再按历史 compact 缓存缺失方向处理；memory_engine 未保留该方向的改动。
- 保持用户要求：不启动项目，不执行测试，只做编译/类型检查。

进展：
- 已撤回 memory_engine 中“compact turns 缓存 miss 时从 records 兜底回填”的改动，因为用户明确不考虑历史数据场景。
- 已确认重复请求来自 `useContactSessionResolver.ensureContactSession`：为了优先选择有消息会话，它每次都会重新远端扫描项目会话，并对候选会话调用 `getConversationMessages(limit: 1, compact: false)` 预览。
- 已为联系人/项目维度的远端会话解析增加单飞和结果缓存，避免切换状态未稳定时反复触发 `conversations` 与 `messages?limit=1` 请求。
- 已调整 `useTeamMemberConversation`：当已经拿到目标 `selectedSessionId` 但 `currentSession` 尚未同步到 React 视图时，等待状态稳定，不再立即重入联系人选择流程。

计划检查：
- `npm run type-check`（chat_app）
- `cargo check`
- `git diff --check`
- `git -C /Users/lilei/project/my_project/memory_engine/backend status --short`

验证结果：
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。
- `git -C /Users/lilei/project/my_project/memory_engine/backend status --short`：无输出，memory_engine 无遗留改动。

备注：
- 未启动项目。
- 不执行测试。

## 第 51 轮：修复项目成员会话 id 漂移到空 session

状态：已完成

目标：
- 按用户最新观察“同一个会话上下文传出的 id 不一致”，追踪 `user-message-turns` 请求参数来源。
- 解释并修复项目成员侧栏拿到空 session id 的前端路径。
- 保持用户要求：不启动项目，不执行测试，只做编译/类型检查。

进展：
- 已确认 `user-message-turns` 的 `conversationId` 来自 `TeamMembersPane -> workspaceProps.selectedProjectSession.id`。
- 已确认 `selectedProjectSession` 来自项目成员行解析出的 `session`；原逻辑只在本地 `sessions` 里按 contact/project 取最新匹配 session，后端项目成员 DTO 中的 `latest_session_id`、`last_message_at` 没有被前端类型和归一化层接住。
- 已新增 `findBestMatchedSession`：同 contact/project 下优先选择有消息的 session，再回退到绑定候选或最新空 session，避免新建空 session 因更新时间更新而压过有消息历史 session。
- 已将项目成员 DTO 的 `latest_session_id`、`last_message_at` 贯通到 `ProjectContactLink`，并传入项目成员会话解析和 `ensureContactSession`。
- 已让联系人会话缓存识别并丢弃“空缓存 session 遮住有消息 session”的情况，避免旧缓存继续把空 id 传给侧栏。
- 已调整 `useTeamMemberConversation`：当项目成员行已经解析到有消息 session 时，不再让同 contact/project 的空 `currentSession` 覆盖选中 session。

验证结果：
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 52 轮：杜绝空 session 抢占项目成员最新会话

状态：已完成

目标：
- 回答并处理“为什么会有空 session 抢占真实会话”的设计问题。
- 全项目扫描类似的自动补建、无消息绑定、按最新时间猜 session 的路径。
- 从前后端一起杜绝读操作/空会话改写项目成员 `latest_session_id`。

排查结论：
- 前端 `loadSessions` 原先会为没有会话的联系人自动补建 project=0 的空 session。
- 项目成员选择、摘要、运行上下文入口原先复用 `ensureContactSession`，在没有已有 session 时会创建空 session。
- 后端 `create_session` 原先只要 metadata 里有 contact agent，就会把刚创建的 session 写入项目成员绑定表的 `latest_session_id`，但 `last_message_at` 为空。
- 仓储层 `sync_project_agent_link(session_id: None)` 原先会把已有 `latest_session_id` 更新为 `NULL`，导致添加/同步项目成员这类无消息操作也可能擦掉已有绑定。
- 前端 `createSession` 远端查重原先按更新时间取最新匹配 session，仍可能复用空 session。

进展：
- 已移除 `loadSessions` 中联系人自动补建空 session 的逻辑。
- 已为 `ensureContactSession` 增加 `createIfMissing` 开关；项目成员选择、打开摘要、打开运行上下文只解析已有 session，不再创建空 session。
- 已让项目成员 composer 在已有联系人但暂无 session 时仍可发送；只有用户真正发送消息时才允许创建 session。
- 已移除后端 `create_session` 时写项目成员 `latest_session_id` 的行为。
- 已在 `chatos_sessions` 的消息落库统一入口中增加用户消息后的项目成员绑定同步：只有真实 user message 保存成功后，才写 `latest_session_id` 和 `last_message_at`。
- 已调整 project-agent link 仓储：`latest_session_id` 只在传入非空 session_id 时更新；`session_id: None` 不再清空已有 latest。
- 已把前端 `createSession` 的 contact 远端查重改成优先有消息 session，不再只按更新时间取最新空 session。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/modules/conversation_runtime/sessions.rs chat_app_server_rs/src/services/chatos_sessions.rs chat_app_server_rs/src/repositories/chatos_memory_mappings.rs`：通过。
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 53 轮：修复项目成员已有 session 时卡在切换中

状态：已完成

目标：
- 修复页面一直显示“正在切换到某成员的会话...”的问题。
- 保留第 52 轮约束：读操作不创建空 session，但已有 session 必须能正常切换。

原因：
- 第 52 轮把项目成员选择改成“不创建缺失 session”，但 `useTeamMemberConversation` 中的自动 effect 仍沿用旧保护逻辑：当 `selectedSessionId` 已经存在但 `currentSession.id` 还没同步到这个 id 时直接 `return`。
- 这会导致页面已经拿到目标 `selectedProjectSession`，但没有触发 `selectSession`，因此 `isSelectedSessionActive` 一直为 false，界面停在“正在切换”。

进展：
- 已调整 `useTeamMemberConversation`：当存在目标 `selectedSessionId` 且当前会话还不是它时，主动调用一次 `selectSession(selectedSessionId)`。
- 已用 `autoSelectingSessionIdRef` 防止自动切换重复触发。
- 没有恢复空 session 创建行为。

验证结果：
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 54 轮：删除消息气泡重复任务按钮

状态：已完成

目标：
- 直接删除右侧聊天消息气泡右上角的“任务”按钮，避免和用户消息卡片上的任务入口重复。
- 保留用户消息卡片上的“任务”按钮。

进展：
- 已移除 `MessageActions` 中的任务按钮、`ListTodo` 图标和 `messageTasks.action` 文案依赖。
- 已移除 `MessageItem -> MessageActions` 的 `onOpenTasks` 传递链路。
- 已移除 `MessageList` 内部的 `MessageTaskDrawer`、`taskMessage` 状态和消息气泡任务抽屉入口。
- 已删除临时 `showTaskActions` 开关，避免保留无意义的兼容分支。
- 已恢复项目成员页用户消息侧栏的任务抽屉链路，用户消息卡片上的任务按钮继续可用。

验证结果：
- `npm run type-check`（chat_app）：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 55 轮：阻止项目成员空 session id 抢占用户消息查询

状态：已完成

目标：
- 修复项目成员页用户消息侧栏仍然请求前端误建空 uuid 的问题。
- 已经落库的空 session 不能再因为“更新时间最新”抢占真正有消息的 session。

原因：
- 前端 `createSession` 会生成 uuid 并提交后端；之前误建的空 session 重启后仍会从 `/conversations` 列表回来。
- `findBestMatchedSession` 虽然已经写成“优先有消息 session”，但后端 `Session` 响应没有 `message_count`，前端无法判断哪个 session 有消息。
- 后端 `find_existing_active_chatos_session` 原先只取 `limit: 1` 最新候选，也可能直接命中空 session。
- 项目成员行已有 `latest_session_id + last_message_at` 时，当前空 session 仍可能因为 metadata 匹配同 contact/project 被当作有效会话。

进展：
- 后端 `Session` 模型新增 `message_count`，`/api/conversations` 列表、详情和 agent sessions 会用 memory engine `count_thread_records` 填充消息数。
- 后端已有会话查重改为取一页候选，并优先返回 `message_count > 0` 的 session，避免发送消息时继续复用最新空 session。
- 后端更新会话标题/状态时保留已有 `message_count`，避免更新响应短暂丢失消息数。
- 前端项目成员行新增保存 `latestSessionId`、`lastMessageAt`，即使 session 暂时不在前端列表里，也能先选择绑定的有消息 session id。
- 前端项目成员会话解析增加保护：当成员绑定表显示已有消息 session 时，当前空 session 不能覆盖选中会话。
- 已补齐 `ProjectContactRow` 测试构造字段。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/models/session.rs chat_app_server_rs/src/services/chatos_memory_engine/mappers.rs chat_app_server_rs/src/services/chatos_memory_engine/sessions.rs`：通过。
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 56 轮：项目接口直接提供用户消息会话 id

状态：已完成

目标：
- 按“一项目一个联系人，一个项目对应一个用户消息会话”的业务约束收敛前端取 id 的路径。
- 用户消息侧栏不再从 conversations/session 列表猜测 id，而是直接使用项目接口返回的绑定会话 id。

原因：
- 第 55 轮仍然保留了前端从 `selectedProjectSession` 推导侧栏查询 id 的路径；如果当前 workspace session 是误建空 session，侧栏仍可能拿到这个空 uuid。
- 当前 `/api/projects` 响应只返回项目基础字段，没有把项目联系人绑定表里的 `latest_session_id` 带给前端。

进展：
- 后端 `Project` 响应新增 `latest_session_id`、`last_message_at`。
- `GET /api/projects` 和 `GET /api/projects/:id` 会读取项目当前 active 联系人绑定，并把 `latest_session_id`/`last_message_at` 附加到项目响应。
- 如果绑定表暂时没有 `latest_session_id`，项目接口会按“同项目 + 当前联系人/agent + 有消息”从会话列表解析出会话 id 返回，前端仍不参与猜测。
- 前端 `ProjectResponse`、`Project` 类型和 `normalizeProject` 已接住 `latestSessionId`、`lastMessageAt`。
- 项目成员页用户消息侧栏的 `activeSessionId` 改为优先使用 `project.latestSessionId`，只有接口没有给 id 时才回退到 `selectedProjectSession.id`。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/models/project.rs chat_app_server_rs/src/repositories/projects.rs chat_app_server_rs/src/api/projects/crud_handlers.rs`：通过。
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 57 轮：会话 id 解析严格改为 project_id + contact_id

状态：已完成

目标：
- 修正第 56 轮仍然信任脏 `latest_session_id`、以及用 agent 兜底的错误做法。
- 项目会话 id 只允许由 `(project_id, contact_id)` 精确确定；不能用 agent、更新时间或绑定表脏值猜。

原因：
- `chatos_project_agent_links.latest_session_id` 可能已经被历史错误写坏，不能作为项目接口响应的可信来源。
- 旧逻辑用 agent 兜底时，同一个 agent 或脏绑定可能把别的 session 返回给当前项目。
- `/api/projects/:id/contacts` 仍在透传绑定表里的 `latest_session_id`，项目成员页也会被污染。

进展：
- 新增项目 API 内部 resolver：分页扫描同项目 session，只返回 metadata 中 `contact_id == 当前 contact_id` 的精确匹配 session。
- `GET /api/projects`、`GET /api/projects/:id` 不再直接信任绑定表 `latest_session_id`，改用精确 resolver 写入项目响应。
- `latest_session_id`、`last_message_at` 不再使用 `skip_serializing_if`，项目接口会稳定返回字段；解析不到时明确返回 `null`。
- 项目响应会遍历当前项目 active contact links，只有能通过 `(project_id, contact_id)` 精确解析出 session 的 link 才会贡献 `latest_session_id`，避免第一条脏 link 带偏。
- `GET /api/projects/:id/contacts` 也改为逐个 contact 重算 `latest_session_id`；没有精确匹配时返回空，不再返回脏 id。
- 项目联系人绑定表 upsert 改为优先按 `(user_id, project_id, contact_id)` 定位，缺少 contact_id 时才回退 agent。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/api/projects.rs chat_app_server_rs/src/api/projects/session_resolver.rs chat_app_server_rs/src/api/projects/crud_handlers.rs chat_app_server_rs/src/api/projects/contact_handlers.rs chat_app_server_rs/src/repositories/chatos_memory_mappings.rs`：通过。
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 58 轮：彻底切断用户消息侧栏错误 session fallback

状态：已完成

目标：
- 修复项目成员页用户消息侧栏仍偶发使用旧/莫名 session id 请求 `user-message-turns` 的问题。
- 修复项目接口全量解析不到 `latest_session_id` 时，后端 resolver 无法从 memory engine 包装 metadata 中读出 contact/project 归属的问题。

原因：
- `TeamMembersPane` 仍在 `project.latestSessionId` 为空时回退到 `workspaceProps.selectedProjectSession?.id`，这会让侧栏继续拿当前工作区 session 去查用户消息。
- memory engine 返回的 session metadata 外层是 `legacy_session_mapping + source_metadata`，后端 `ChatRuntimeMetadata` 之前只读顶层 `chat_runtime/contact/ui_contact`，导致 `contact_id_from_metadata` 在 engine 包装后的 session 上读不到 contact_id。

进展：
- 前端用户消息侧栏 `activeSessionId` 改为只信任项目接口返回的 `project.latestSessionId`；项目接口没有解析出 id 时，侧栏不再请求 `user-message-turns`，也不会再退回旧 session id。
- 后端 `ChatRuntimeMetadata` 增加对 `source_metadata` 包装结构的读取，并兼容 `legacy_session_mapping.contact_id / agent_id / project_id`。
- 项目 session resolver 保留 `(project_id, contact_id)` 精确匹配；先按 project 索引查，查不到时再按 user 扫描并在服务端精确过滤，避免下游 project 索引不完整时直接返回空。
- 补充 engine 包装 metadata 的解析单测用例，防止后续再次漏读 `source_metadata`。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/core/chat_runtime_metadata.rs chat_app_server_rs/src/core/chat_runtime.rs chat_app_server_rs/src/api/projects/session_resolver.rs`：通过。
- `npm run type-check`（chat_app）：通过。
- `cargo check`：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 不执行测试。

## 第 59 轮：定位项目 latest_session_id 返回空会话的真实来源

状态：已完成

目标：
- 按“项目配置 contact_id 精准定位 session”的业务约束，查清 `zeus` 项目为什么仍返回 `e2ecae24-3b1d-4b06-ba75-a405915cb4c3`。
- 同时排查 chatos_rs 与 memory_engine/backend 的实际数据库数据。

原因：
- `e2ecae24-3b1d-4b06-ba75-a405915cb4c3` 不是代码硬编码，而是运行时数据返回。
- `zeus` 项目在 `chatos_project_agent_links` 中存在两条 active 联系人绑定：`小鸣 contact_id=701...` 指向 `e2ecae...`，`小爱 contact_id=534...` 指向 `c5644aee...`。
- MongoDB `memory_engine.engine_threads` 中 `e2ecae...` 确实是 `(project_id=zeus, contact_id=701...)` 的精准匹配，但 `engine_records` 为 0；`c5644aee...` 有 4920 条记录，但 legacy mapping 里的 `contact_id` 是 null，真实 contact_id 在 `source_metadata.contact.contact_id`。

进展：
- `chat_app_server_rs` 项目 session resolver 继续使用项目配置的 `contact_id` 精准匹配；对用户消息侧栏只返回 `message_count > 0` 的精准 session，空会话不会再作为用户消息会话 id 返回。
- `memory_engine/backend` 的 `list_threads` 查询补齐 contact/project/agent 的 alias 过滤，不再只看 `metadata.legacy_session_mapping.*`，也会匹配 `metadata.source_metadata.*` 中的真实 contact/project/agent 字段。
- 已用 MongoDB 真实数据验证：`project_id=zeus + contact_id=534...` 可匹配 `c5644aee...` 且 records=4920；`project_id=zeus + contact_id=701...` 匹配 `e2ecae...` 且 records=0。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/api/projects/session_resolver.rs`：通过。
- `rustfmt --edition 2024 /Users/lilei/project/my_project/memory_engine/backend/src/repositories/threads/queries.rs`：通过。
- `cargo check`（chat_app_server_rs）：通过。
- `cargo check`（memory_engine/backend）：通过。
- `git diff --check`（chatos_rs）：通过。

备注：
- 当前运行中的 memory_engine 是旧进程，需重启 memory_engine 后 `memory_engine/backend` 的查询修复才会生效。
- 当前运行中的 chatos_rs 后端也需要用新二进制启动；本轮在沙箱内直接启动时因本地端口/network 权限限制报 `Operation not permitted`。

## 第 60 轮：项目联系人表收敛为一项目一联系人

状态：已完成

目标：
- 按最新业务口径移除“历史成员/多成员”模型残留：一个项目只能绑定一个联系人，表里也只能保存一条当前绑定。
- 修复团队成员改造时自动默认选择联系人导致的错绑脏数据。
- 防止用户消息保存路径反向创建或切换项目联系人。

原因：
- `chatos_project_agent_links` 旧 schema 是 `UNIQUE(user_id, project_id, agent_id)`，允许同一个项目保存多条联系人 link。
- 项目设置接口之前通过 archive 旧行来模拟更换联系人，仍然保留历史成员行。
- 用户消息保存后会调用 project-agent link sync，如果旧会话带着同项目、不同联系人 metadata，可能把项目绑定写回错误联系人。
- 当前本地库里 zeus 项目再次出现小爱 active、小鸣 archived；用户确认 zeus 应绑定小鸣，且小鸣本来就没有消息。

进展：
- 仓储 `upsert_project_agent_link` 改为按 `(user_id, project_id)` 更新同一条 link；切换联系人时会清空旧 `latest_session_id` 和 `last_message_at`，不再继承旧联系人会话。
- 项目设置“更换联系人”不再 archive 其它联系人；新增/更换直接替换项目当前唯一 link。
- 项目设置“解绑联系人”改为物理删除当前 link，不再保留 archived 历史行。
- 用户消息保存路径改为只更新“当前项目绑定且 contact_id 匹配”的 `latest_session_id`；如果消息来自旧联系人会话，不会创建或切换项目绑定。
- SQLite schema 新库改为 `UNIQUE(user_id, project_id)`；旧库启动迁移会删除非 active/无 contact link、按项目去重，并创建 `(user_id, project_id)` 唯一索引且不吞掉创建失败。
- Mongo 初始化补充 `(user_id, project_id)` 唯一索引；Mongo upsert 同样按项目维度更新并删除同项目重复 link。
- 已备份并清理当前 SQLite：备份路径 `/tmp/chatos_rs_chat_app_before_single_contact_cleanup_20260617.sqlite`。
- 当前 SQLite 清理后：`chatos_project_agent_links` 从 24 条变为 12 条，非 active 为 0，无 contact 为 0，重复项目绑定查询为空。
- zeus 当前只剩小鸣一条 active 绑定：`contact_id=701fed71-eb07-463d-a378-23a9e365c2db`，`latest_session_id=null`。

验证结果：
- `rustfmt --edition 2024 chat_app_server_rs/src/repositories/chatos_memory_mappings.rs chat_app_server_rs/src/services/chatos_memory_mappings.rs chat_app_server_rs/src/services/chatos_sessions.rs chat_app_server_rs/src/api/projects/contact_handlers.rs chat_app_server_rs/src/db/sqlite_schema.rs chat_app_server_rs/src/db/mongodb.rs`：通过。
- `cargo check`：通过。
- `npm run type-check`（chat_app）：通过。
- `git diff --check`：通过。

备注：
- 未启动项目。
- 未执行测试。
