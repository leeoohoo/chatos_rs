# Project Optimization Progress 2026-06-17

## 第 1 轮：移除 chat_app_server_rs crate 根 dead_code 兜底

状态：已完成

目标：
- 移除 `chat_app_server_rs/src/lib.rs` 顶部的 `#![allow(dead_code)]`。
- 优先清理私有、未引用、行为风险低的 dead code。
- 每轮只做编译级验证，不启动项目，不执行测试。

初始观察：
- 当前模块级 `allow(dead_code)` 已清零。
- 移除 crate 根 `allow(dead_code)` 后，`cargo check -p chat_app_server_rs` 初始暴露 111 个 warning。

推进记录：
- 第 1 批：清理 `change_logs` 未接入 project scope、UI prompt normalizer/store 旧模块、Mongo/DB 测试 helper、旧 model constructor、SSE/attachment/abort/events/workspace 未用 helper，warning 降到 61。
- 第 2 批：继续清 `task_manager` review hub、UI prompt hub、memory mapping/runtime DTO、terminal/model config 等未引用项，warning 降到 28。
- 第 3 批：处理 agent runtime、MCP execution、task board refresh、repository 薄包装、review repair scope、contact prompt 未接入模式，warning 从 18 降到 0。

本轮关键改动：
- `chat_app_server_rs/src/lib.rs` 已移除 crate 根 `#![allow(dead_code)]`。
- 删除未接入的 repository helper：`repositories/agents.rs::list_agents`、`repositories/chatos_memory_mappings.rs::update_contact_agent`、`repositories/memory_skills.rs::list_skills_by_ids`。
- 删除/测试化仅测试使用的 request payload、timeout、MCP 并发策略 wrapper、task board refresh context setter。
- 删除 `AiServer` 只写字段与未用 shared runner builder，删除 `PromptRunnerRuntime` 未用 getter。
- 将 `ContactSkillPromptMode` 的 Summary/SelectedFull 及其 prompt builder 分支收进 `cfg(test)`，生产编译只保留当前实际使用的 Disabled 路径。
- 精简 `ChatosReviewRepairScope`，保留映射校验但移除后续未读取字段。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过，当前 0 warning。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 进入热点大文件拆分或继续处理跨模块重复抽象；从风险连续性看，可以优先拆 `chat_app_server_rs/src/api/message_task_runner.rs` 或继续做 repository CRUD helper 抽象。

## 第 2 轮：拆分 message_task_runner graph normalizer

状态：已完成

目标：
- 先从 `chat_app_server_rs/src/api/message_task_runner.rs` 中拆出纯函数密集、边界清晰的任务图 normalizer。
- 保持现有路由、handler、task runner client 调用不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `message_task_runner.rs` 当前 1248 行。
- 文件内部主要分为 lookup/metadata、task graph normalization、context resolver、HTTP handlers、graph normalizer 单元测试几块。
- 第一刀选择 graph normalization，预计可把入口文件降到约 900 行以内，并为后续继续拆 context/handlers 留出边界。

推进记录：
- 新增 `chat_app_server_rs/src/api/message_task_runner/graph.rs`，承接任务图节点补齐、边归一化、深度重算与原 graph normalizer 单元测试。
- 新增 `chat_app_server_rs/src/api/message_task_runner/context.rs`，承接消息/会话 lookup、source id/turn id 解析、联系人 task runner runtime config resolver、消息来源匹配。
- `chat_app_server_rs/src/api/message_task_runner.rs` 保留路由、active request DTO、文本归一化小 helper 与 HTTP handlers。

结果：
- 入口文件从 1248 行降到 390 行。
- 新模块行数：`context.rs` 304 行，`graph.rs` 578 行。
- 路由、handler 返回结构、task runner client 调用保持不变。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续热点大文件拆分时，可转向 `task_runner_service/frontend/src/pages/TasksPage.tsx` / `RunsPage.tsx`。
- 后端侧继续做的话，可拆 `crates/chatos_ai_runtime/src/task.rs` 或推进 repository CRUD helper 抽象。

## 第 3 轮：拆分 chatos_ai_runtime task runtime builder

状态：已完成

目标：
- 从 `crates/chatos_ai_runtime/src/task.rs` 中拆出 `TaskRuntimeBuilder`。
- 保持 `task::TaskRuntimeBuilder` 的对外导出和现有 builder API 不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `crates/chatos_ai_runtime/src/task.rs` 当前 1489 行。
- 文件包含任务 spec/config/report、runtime builder、runtime execution、helper 和大量单元测试。
- 第一刀选择 `TaskRuntimeBuilder`，它依赖边界集中，适合独立到 `task/runtime_builder.rs`。

推进记录：
- 新增 `crates/chatos_ai_runtime/src/task/runtime_builder.rs`，承接 `TaskRuntimeBuilder` 结构体、builder 方法和 `Default` 实现。
- 新增 `crates/chatos_ai_runtime/src/task/memory.rs`，承接 `TaskMemoryRuntimeConfig`、memory engine builder 应用逻辑和 serde 默认值函数。
- 新增 `crates/chatos_ai_runtime/src/task/tests.rs`，承接原 `task.rs` 内联单元测试模块。
- `crates/chatos_ai_runtime/src/task.rs` 保留 task spec/config/execution/report、`TaskRuntime`、`ContextualTurnRunner` task report 扩展和少量 task metadata helper。

结果：
- `task.rs` 从 1489 行降到 734 行。
- 新模块行数：`runtime_builder.rs` 157 行，`memory.rs` 138 行，`tests.rs` 488 行。
- `task::TaskRuntimeBuilder`、`task::TaskMemoryRuntimeConfig` 的对外导出保持不变。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chatos_ai_runtime` 通过。
- `cargo check -p chatos_ai_runtime --tests` 通过，仅编译测试目标，未执行测试。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 后端继续拆分时，可转向 `crates/chatos_ai_runtime/src/runtime.rs`，优先拆 request/input helper 或 runtime result/report 相关结构。
- 也可以切到前端热点：`task_runner_service/frontend/src/pages/TasksPage.tsx` / `RunsPage.tsx`。

## 第 4 轮：拆分 chatos_ai_runtime runtime options/report

状态：已完成

目标：
- 从 `crates/chatos_ai_runtime/src/runtime.rs` 中拆出 runtime options/context refresh 和 turn result/report 结构。
- 保持 `runtime::AiRuntimeOptions`、`runtime::AiTurnReport`、`runtime::AiRuntimeResult` 等对外导出不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `crates/chatos_ai_runtime/src/runtime.rs` 当前 1197 行。
- 文件前半部分包含 options/context refresh/report 数据结构，后半部分是 `AiRuntime::run_turn` 主循环和输入/调试 helper。
- 第一刀选择 options/report，避免直接改主循环控制流。

推进记录：
- 新增 `crates/chatos_ai_runtime/src/runtime/options.rs`，承接 `AiRuntimeOptions`、`IterativeContextRefresh`、`MemoryContextOverflowRecovery` 和 context overflow 通知 helper。
- 新增 `crates/chatos_ai_runtime/src/runtime/report.rs`，承接 `AiRuntimeResult`、`AiTurnStatus`、`AiTurnReport`。
- 新增 `crates/chatos_ai_runtime/src/runtime/tests.rs`，承接原 `runtime.rs` 内联单元测试模块。
- `crates/chatos_ai_runtime/src/runtime.rs` 保留 `AiRuntime` 主流程、record persistence、runtime input/debug/tool summary helper。

结果：
- `runtime.rs` 从 1197 行降到 620 行。
- 新模块行数：`options.rs` 272 行，`report.rs` 114 行，`tests.rs` 199 行。
- `runtime::AiRuntimeOptions`、`runtime::AiRuntimeResult`、`runtime::AiTurnReport`、`runtime::AiTurnStatus` 的对外导出保持不变。
- context overflow recovery 的通知行为保持原来的 `on_thinking` 回调路径。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chatos_ai_runtime` 通过。
- `cargo check -p chatos_ai_runtime --tests` 通过，仅编译测试目标，未执行测试。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- Rust 后端热点大文件已明显收敛；下一轮可进入前端 `task_runner_service/frontend/src/pages/TasksPage.tsx` / `RunsPage.tsx` 抽表格壳、状态标签和详情抽屉。
- 若继续后端，可做 repository CRUD helper 抽象或 code navigation 多语言 provider 重复逻辑收口。

## 第 5 轮：拆分 task runner RunsPage payload/event helper

状态：已完成

目标：
- 从 `task_runner_service/frontend/src/pages/RunsPage.tsx` 中拆出 payload 展示组件、run event 解析 helper 和 remote operation 汇总 helper。
- 保持 runs 列表查询、详情抽屉、事件流刷新、远程操作展示行为不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `RunsPage.tsx` 当前 1380 行。
- 文件尾部混有 `JsonBlock`、`CodeParagraph`、`CollapsiblePayload` 这类展示组件，以及 tool call/tool result/remote operation/event payload 解析 helper。
- `task_runner_service/frontend/src/pages/tasks/` 已经有页面局部拆分目录，本轮沿用同样 colocated 结构新增 `pages/runs/`。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/runs/payloadView.tsx`，承接 `JsonBlock`、`CodeParagraph`、`CollapsiblePayload` 和结构化 payload 摘要文案 helper。
- 新增 `task_runner_service/frontend/src/pages/runs/runEventUtils.tsx`，承接 tool call/result 提取、remote operation 汇总、stream event 统计、event type 描述和 `RunEventPayload`。
- `RunsPage.tsx` 改为从 `./runs/*` 引入这些 helper，自身保留查询状态、筛选、表格、详情抽屉布局和 mutation 逻辑。

结果：
- `RunsPage.tsx` 从 1380 行降到 936 行。
- 新模块行数：`payloadView.tsx` 110 行，`runEventUtils.tsx` 370 行。
- 原有 Ant Design 展示组件和接口调用路径保持不变。

验证：
- `npm run type-check` 通过。
- `npm run build` 通过；仅出现 Vite chunk size 提示和沙盒下 Homebrew shellenv 的 `/bin/ps` 非致命提示。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续前端热点拆分时，优先处理 `task_runner_service/frontend/src/pages/TasksPage.tsx`，可拆 `TaskDetailDrawer`、运行/编辑表单段和 remote operation 展示块。
- 也可以进一步把 `RunsPage` 与 `TasksPage` 中相似的 remote operation 解析/格式化逻辑收口成共享 helper。

## 第 6 轮：收口 task runner remote operation 重复 helper

状态：已完成

目标：
- 收口 `RunsPage` 与 `TasksPage` 周边重复的 remote operation 工具名判断、payload 类型 guard、endpoint 格式化和统计逻辑。
- 保持 `TasksPage.tsx` 既有 import 名称不变，避免扩大页面主体改动。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `task_runner_service/frontend/src/pages/tasks/taskPageUtils.tsx` 和 `pages/runs/runEventUtils.tsx` 都有远程工具白名单、record/string/number guard、远程端点格式化和 success/failed 统计。
- 两边展示维度不同，但底层纯函数稳定，适合先抽共享 helper，而不是强行合并完整 view model。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/shared/remoteOperationUtils.ts`，承接 remote tool name set、payload guard、remote operation stats 和 endpoint formatter。
- `taskPageUtils.tsx` 改为复用共享 helper，并通过 re-export 保留 `formatTaskRemoteEndpoint`。
- `runEventUtils.tsx` 改为复用共享 helper，并继续向 `RunsPage.tsx` 导出 `formatRemoteEndpoint`。

结果：
- 新共享模块 `remoteOperationUtils.ts` 78 行。
- `runEventUtils.tsx` 从 370 行降到 325 行。
- `taskPageUtils.tsx` 删除本地重复 guard/统计/endpoint 实现，保留页面既有 API 面。

验证：
- `npm run type-check` 通过。
- `npm run build` 通过；仅出现 Vite chunk size 提示和沙盒下 Homebrew shellenv 的 `/bin/ps` 非致命提示。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续拆 `task_runner_service/frontend/src/pages/TasksPage.tsx` 的详情抽屉或运行表单区，把 2080 行入口页压到更容易维护的规模。
- 如果继续做前端构建体积，可评估 `App.tsx` 路由级 lazy import，处理当前 build 的大 chunk 提示。

## 第 7 轮：拆分 task runner TasksPage 详情抽屉

状态：已完成

目标：
- 从 `task_runner_service/frontend/src/pages/TasksPage.tsx` 中拆出任务详情抽屉。
- 保持任务列表、query/mutation、路由参数和运行/编辑/Memory/Prompt 跳转行为不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `TasksPage.tsx` 当前 2080 行，是前端最大的热点页面。
- 详情抽屉包含任务基础信息、目标/描述/过程记录、最近 remote operation、最近运行、相关 prompt、follow-up 和 run-derived task 展示，逻辑长但主要是渲染。
- 父页面已经集中计算了 `selectedTask`、remote operation、recent runs、prompts、follow-ups 等数据，适合把抽屉变成纯展示组件。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/tasks/TaskDetailDrawer.tsx`，承接任务详情抽屉及其内部展示分段。
- `TasksPage.tsx` 改为渲染 `<TaskDetailDrawer />`，通过 props 传入 query 结果、Map 索引和跳转/编辑/运行回调。
- 保留原行为差异：主任务详情里的编辑/立即运行/Memory 会先关闭详情抽屉；关联任务列表里的运行按钮仍直接打开运行弹窗。

结果：
- `TasksPage.tsx` 从 2080 行降到 1541 行。
- 新模块 `TaskDetailDrawer.tsx` 773 行。
- 父页面的职责更集中在数据获取、表格筛选、表单提交和 mutation。

验证：
- `npm run type-check` 通过。
- `npm run build` 通过；仅出现 Vite chunk size 提示和沙盒下 Homebrew shellenv 的 `/bin/ps` 非致命提示。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续拆 `TasksPage.tsx` 的 create/edit drawer 表单，建议抽 `TaskEditorDrawer`，进一步把表单字段、MCP 配置块和 schedule 表单段从入口页移走。
- 另一条线可以处理 Vite 路由级 lazy import，降低当前 build 的大 chunk 提示。

## 第 8 轮：拆分 task runner TasksPage 创建/编辑抽屉

状态：已完成

目标：
- 从 `task_runner_service/frontend/src/pages/TasksPage.tsx` 中拆出 create/edit drawer 表单。
- 保持任务创建、编辑、MCP prompt preview、schedule 校验、remote server 选择和保存行为不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- 第 7 轮后 `TasksPage.tsx` 已降到 1541 行，但 create/edit drawer 仍包含大量表单字段、schedule 分支、MCP 配置块和 remote server 提示块。
- 这些逻辑大多是表单渲染与局部 `Form.useWatch` 派生状态，适合放进独立 drawer 组件。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/tasks/TaskEditorDrawer.tsx`，承接任务创建/编辑抽屉。
- 将 `mcpEnabled`、`enabledBuiltinKinds`、`defaultRemoteServerId`、`scheduleMode` 的 `Form.useWatch` 和相关 option/status 派生逻辑移动到 `TaskEditorDrawer`。
- `TasksPage.tsx` 保留表单实例、默认值填充、payload 构建、mutation 提交和 preview mutation 触发。
- `openCreateDrawer` 改为直接基于 `mcpCatalogQuery.data` 设置默认 enabled builtin kinds，避免父页维护重复的 `mcpOptions` 映射。

结果：
- `TasksPage.tsx` 从 1541 行降到 1180 行。
- 新模块 `TaskEditorDrawer.tsx` 466 行。
- 父页面进一步收敛到查询、表格、路由、mutation 和 modal 编排。

验证：
- `npm run type-check` 通过。
- `npm run build` 通过；仅出现 Vite chunk size 提示和沙盒下 Homebrew shellenv 的 `/bin/ps` 非致命提示。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续前端拆分时，可把运行任务/批量运行两个 Modal 抽成 `TaskRunModal` 和 `BatchTaskRunModal`，或先评估是否直接做路由级 lazy import 解决 build 大 chunk 提示。
- 后端方向可以回到 code navigation provider 或 repository CRUD helper 抽象。

## 第 9 轮：拆分 task runner TasksPage 运行弹窗

状态：已完成

目标：
- 从 `task_runner_service/frontend/src/pages/TasksPage.tsx` 中拆出单任务运行和批量运行两个 Modal。
- 保持运行任务、批量运行任务、模型覆盖和 prompt override 提交行为不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- 第 8 轮后 `TasksPage.tsx` 已降到 1180 行，剩余运行弹窗虽然不大，但仍把运行表单 UI 与父页面 mutation 编排混在一起。
- 两个 Modal 只依赖 `RunTaskFormValues`、model options、任务信息和 submit/close 回调，边界清晰。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/tasks/TaskRunModals.tsx`，承接 `TaskRunModal` 和 `BatchTaskRunModal`。
- `TasksPage.tsx` 改为传入 `runForm`、`batchRunForm`、运行任务/批量任务、model options、loading 状态和提交回调。
- 父页面继续保留 `handleRunTask`、`handleBatchRunTask`、mutation 和打开/关闭弹窗的状态管理。

结果：
- `TasksPage.tsx` 从 1180 行降到 1120 行。
- 新模块 `TaskRunModals.tsx` 133 行。
- `TasksPage.tsx` 的运行相关 UI 进一步从入口页移除。

验证：
- `npm run type-check` 通过。
- `npm run build` 通过；仅出现 Vite chunk size 提示和沙盒下 Homebrew shellenv 的 `/bin/ps` 非致命提示。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 前端侧下一步更值得做路由级 lazy import，直接处理当前 build 的大 chunk 提示。
- 如果继续做页面拆分，可把 task 列表筛选/批量操作工具条抽成 `TaskListToolbar`，但收益会小于 lazy import。

## 第 10 轮：优化 task runner frontend vendor 分包

状态：已完成

目标：
- 处理 `npm run build` 中持续出现的 Vite 大 chunk 提示。
- 不通过单纯提高 `chunkSizeWarningLimit` 掩盖问题，而是调整 Rollup manual chunks。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `App.tsx` 已经使用 `React.lazy` 做了路由级页面懒加载。
- 大 chunk 主要来自公共依赖/vendor，而不是页面代码没有 lazy import。
- 初次尝试把 React、Ant Design、React Query 分成几个粗粒度 vendor chunk 后，入口 chunk 变小，但 `antd-vendor` 超过 900KB 且出现 circular chunk 提示。

推进记录：
- 在 `task_runner_service/frontend/vite.config.ts` 中新增 `rollupOptions.output.manualChunks`。
- 将 React 生态包归入 `react-vendor`，TanStack Query 归入 `query-vendor`。
- 将 `antd` 主包单独归入 `antd-vendor`。
- 将 `@ant-design/*`、`@rc-component/*`、`rc-*` 支撑依赖合并归入 `antd-support-vendor`，避免过度碎片化和 circular chunk。
- 其他依赖归入通用 `vendor`。

结果：
- build 输出不再出现 Vite 大 chunk warning。
- 最大 chunk 从此前约 803KB/928KB 级别降为：
  - `antd-vendor` 约 506KB
  - `antd-support-vendor` 约 424KB
  - `react-vendor` 约 164KB
  - `vendor` 约 40KB
- 页面 chunk 仍保持路由级拆分，例如 `TasksPage` 约 52KB、`RunsPage` 约 21KB。

验证：
- `npm run type-check` 通过。
- `npm run build` 通过；不再有 Vite 大 chunk warning、circular chunk 或 empty chunk 提示。
- build 仍有沙盒下 Homebrew shellenv 的 `/bin/ps` 非致命提示。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 前端热点可以继续拆 `TasksPage` 工具条/批量操作区，但收益已经开始变小。
- 后续更建议回到后端重复模式：code navigation 多语言 provider 抽象，或 repository CRUD helper 抽象。

## 第 11 轮：收口 code navigation 多语言 provider 样板

状态：已完成

目标：
- 继续推进 code navigation 多语言 provider 重复逻辑抽象。
- 优先处理 Go / Java / Python / Rust provider 中重复的 symbol 映射、document symbols response、capabilities 和 extension 支持判断。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `go/mod.rs`、`java/mod.rs`、`python/mod.rs`、`rust/mod.rs` 都有同形代码：
  - 将语言 symbol 映射为 `IndexedSymbol`。
  - 将语言 symbol 映射为 `DocumentSymbolItem` 并截断到 `MAX_SYMBOL_RESULTS`。
  - 返回完全相同的 `NavCapabilities`。
  - 基于单一扩展名判断 `supports_file`。
- definition/reference 的核心解析和 scoring 各语言差异较大，本轮不碰这部分。

推进记录：
- 在 `chat_app_server_rs/src/services/code_nav/languages/shared_nav.rs` 新增：
  - `NavSymbolLike` trait。
  - `indexed_symbols_from`。
  - `document_symbols_response`。
  - `heuristic_nav_capabilities`。
  - `supports_extension`。
- 为 Go / Java / Python / Rust 的 symbol 类型实现 `NavSymbolLike`。
- 四个 provider 改用共享 helper 生成 indexed symbols、document symbols response 和 capabilities。

结果：
- 多语言 provider 的重复样板收口到 `shared_nav.rs`。
- 各语言 `definition` / `references` / project detection / scoring 逻辑保持不变。
- 后续如果继续抽象，可以在此基础上进一步处理 project symbol index 查询与 fallback search 的相似流程。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续 code navigation 线时，可以抽 `project_symbol_index` 查询和 current-file symbol 命中这两段重复流程。
- 另一条后端线可转向 repository CRUD helper 抽象。

## 第 12 轮：继续收口 code navigation definition 候选流程

状态：已完成

目标：
- 在第 11 轮 symbol/document/capabilities 样板抽象基础上，继续消化多语言 provider 的 definition 重复控制流。
- 优先抽当前文件同名符号命中、project symbol index 候选加入、definition 结果排序截断。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- Go / Java / Python / Rust 的 definition 中仍保留三段同形逻辑：
  - 从当前文件 symbols 中查同名且非当前位置的定义候选。
  - 从 `project_symbol_index` 中查同名候选，跳过当前文件当前行，再按 score 去重加入。
  - 按 score、relative path、line、column 排序并截断到 `MAX_DEFINITION_RESULTS`。
- `languages/basic/resolution.rs` 也有相同流程，覆盖 C/C++/C#/Kotlin 等 basic provider，适合一起复用公共 helper。

推进记录：
- 在 `chat_app_server_rs/src/services/code_nav/languages/shared_nav.rs` 新增：
  - `push_current_file_symbol_definitions`
  - `push_indexed_definition_candidates`
  - `sort_and_truncate_nav_locations`
- Go / Java / Python / Rust provider 改用上述 helper，保留各自 import resolution、语言特定 score 和 fallback search。
- `languages/basic` 也实现 `NavSymbolLike`，并复用：
  - `heuristic_nav_capabilities`
  - `document_symbols_response`
  - `indexed_symbols_from`
  - definition 候选 helper

结果：
- Go / Java / Python / Rust provider 中 current-file symbol、project index candidate、排序截断重复块已移除。
- basic provider 的同类重复也一并收口，后续新增 basic language provider 不需要重复维护这些候选流程。
- 本轮没有改变 fallback search、语言解析规则或对外 API 行为。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- code navigation 线可以继续抽 fallback search 的“先 whole-word 后宽松搜索、构建 NavLocation、声明/引用排序”模式。
- 如果希望换线，可转向 repository CRUD helper 抽象，继续消化 crate 根 `#![allow(dead_code)]` 背后的重复和未接入 helper。

## 第 13 轮：收口 code navigation references 与 fallback search 流程

状态：已完成

目标：
- 继续第 12 轮后的 code navigation 抽象，把 references 中的重复搜索、去重、声明过滤和排序截断收进公共 helper。
- 同时复用到 definition fallback 中的“先 whole-word 搜索，空了再宽松搜索”流程。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- Go / Java / Python / Rust references 中仍有同形流程：
  - `search_*_occurrences(..., true, true)`。
  - 为空时再 `search_*_occurrences(..., false, true)`。
  - 将 search match 转为 `NavLocation`。
  - 去重后按 declaration/reference 分流，有真实引用时隐藏声明项。
  - 当前文件优先排序并截断到 `MAX_REFERENCE_RESULTS`。
- `languages/basic/resolution.rs` 也有同样 references 流程，并且 definition fallback 里也重复了同样两段搜索策略。

推进记录：
- 在 `chat_app_server_rs/src/services/code_nav/languages/shared_nav.rs` 新增：
  - `NavSearchMatchLike`
  - `search_occurrences_with_fallback`
  - `select_reference_locations`
- 为 Go / Java / Python / Rust / Basic 的 search match 类型实现 `NavSearchMatchLike`。
- Go / Java / Python / Rust references 改成：
  - 语言内执行 search 函数。
  - 语言内保留 declaration 判断闭包。
  - 公共 helper 负责 location 构建、去重、声明/引用选择、排序和截断。
- Basic provider 同步复用上述 helper。
- Go / Java / Python / Rust / Basic 的 definition fallback 搜索也改用 `search_occurrences_with_fallback`。

结果：
- references 的重复控制流从各语言 provider 中移出。
- definition fallback 的双阶段搜索策略统一到公共 helper。
- Java 的 `primary_type`、各语言 declaration classifier、各语言 search 实现均保持在原模块内，行为边界没有扩大。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- code navigation provider 的显性重复已经明显下降，下一步可以评估是否继续抽 language provider trait adapter，或暂时转向 repository CRUD helper 抽象。
- 如果继续消化 crate 根 `#![allow(dead_code)]`，建议优先看 repositories / task manager / UI prompt manager 中仍未接入的 helper 和导出面。

## 第 14 轮：收口 code navigation definition fallback location 构建

状态：已完成

目标：
- 接着第 13 轮，把 definition fallback 中剩余的“搜索结果转 definition location 并去重”重复循环抽到公共层。
- 保留各语言自己的 declaration kind 判断和 score 函数。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- Go / Java / Python / Rust / Basic 的 definition fallback 中仍然有同形循环：
  - 遍历 `search_matches`。
  - 调用各自的 `resolve_*_declaration_kind`。
  - 调用各自的 `score_*_definition_candidate`。
  - 用 `entry.path / relative_path / line / column / text` 构建 `NavLocation`。
  - `push_unique_location` 去重。
- 第 13 轮已经引入了 `NavSearchMatchLike`，这些 search match 字段可以通过统一 trait 访问。

推进记录：
- 在 `chat_app_server_rs/src/services/code_nav/languages/shared_nav.rs` 新增：
  - `push_definition_search_matches`
  - 私有 `nav_location_from_search_match`
- `select_reference_locations` 也改为复用 `nav_location_from_search_match`，避免 references 和 definition fallback 各自构建同一类 `NavLocation`。
- Go / Java / Python / Rust / Basic 的 definition fallback 改为：
  - 公共 helper 负责遍历、构建 location、去重。
  - 语言模块通过闭包传入 declaration kind resolver 和 score 计算。
- 删除替换后不再需要的 `push_unique_location` import/re-export。

结果：
- code navigation fallback search 的公共外壳进一步收口。
- 各语言 provider 中只保留差异点：搜索入口、声明识别、score 规则、import/path 解析。
- 本轮没有改变对外 API，也没有改动语言解析/打分策略。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过且无 warning。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- code navigation 线可以暂时收束，后续若继续做，建议先评估 provider adapter 抽象是否会过度泛化。
- 更实用的下一步是切到 repository CRUD helper 抽象，或者继续消化 crate 根 `#![allow(dead_code)]` 暴露的 repositories / task manager / UI prompt manager 未接入项。

## 第 15 轮：抽出 repository Mongo 文档读写 helper

状态：已完成

目标：
- 从 code navigation 线切换到 repository CRUD helper 抽象。
- 先选择 project run 相关 repository 作为小切口，收口 Mongo 分支里的 `find_one` 和 upsert `$set` 样板。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `repositories/db.rs` 已有 `with_db`、`to_doc` 等公共 helper，许多 repository 已经用它分离 Mongo / SQLite 分支。
- project run 的两个 repository 中仍有重复 Mongo 代码：
  - `db.collection::<Document>(...).find_one(...).await.map_err(...)`
  - `update_one(filter, doc! { "$set": to_doc(set_doc) }, UpdateOptions::builder().upsert(true).build())`
- 两个文件的业务字段不同，但 Mongo 文档读写外壳完全一致。

推进记录：
- 在 `chat_app_server_rs/src/repositories/db.rs` 新增：
  - `mongo_find_one_doc`
  - `mongo_upsert_set_doc`
- `project_run_catalogs.rs` 改用上述 helper 读取和 upsert `project_run_catalogs`。
- `project_run_environment_settings.rs` 改用上述 helper 读取和 upsert `project_run_environment_settings`。
- SQL 分支、字段序列化、业务模型转换保持不变。

结果：
- project run 相关 repository 的 Mongo CRUD 样板减少。
- 后续其他 repository 如果也走 `Document + $set + upsert` 模式，可以继续复用同一 helper。
- 没有改变对外 API 或数据字段结构。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续 repository 线时，可以把更多 `Document` 型 Mongo repository 的 `find_one` / upsert `$set` 接入这两个 helper。
- 更进一步可以再抽 SQLite row JSON parsing helper，但要先挑边界清楚的文件，避免把业务字段映射过度泛化。

## 第 16 轮：扩展 repository Mongo CRUD helper 到 projects/applications

状态：已完成

目标：
- 在第 15 轮 `mongo_find_one_doc` / `mongo_upsert_set_doc` 的基础上，补齐普通 Mongo CRUD 文档 helper。
- 先接入 `projects.rs` 和 `applications.rs`，这两个文件 CRUD 边界清楚，适合作为下一批复用点。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `projects.rs` 和 `applications.rs` 的 Mongo 分支仍有重复外壳：
  - `insert_one(doc, None).await.map_err(...)`
  - `update_one(filter, doc! { "$set": set_doc }, None).await.map_err(...)`
  - `delete_one(filter, None).await.map_err(...)`
  - 单条读取也可以复用第 15 轮的 `mongo_find_one_doc`
- `applications.rs` 删除应用时还有两个关联 collection 的 `delete_many`，这部分是业务级清理，本轮不抽。

推进记录：
- 在 `chat_app_server_rs/src/repositories/db.rs` 新增：
  - `mongo_insert_doc`
  - `mongo_update_set_doc`
  - `mongo_delete_one_doc`
- `projects.rs` 的 Mongo `get/create/update/delete` 分支改用上述 helper。
- `applications.rs` 的 Mongo `get/create/update/delete` 主表操作改用上述 helper；关联表 `delete_many` 逻辑保持原样。
- SQL 分支和字段映射保持不变。

结果：
- `projects` / `applications` 的 Mongo CRUD 样板进一步减少。
- `repositories/db.rs` 现在有一组基础 `Document` CRUD helper，后续可逐步接入 terminals、agents、remote connections 等类似 repository。
- 普通 update helper 不做 `to_doc` 过滤，保留原有 `Bson::Null` 写入语义；upsert helper 继续保留第 15 轮的 `to_doc` 清理语义。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续 repository 线时，可以接入 `terminals.rs` 或 `agents.rs`，它们有类似的 `find_one/insert/update/delete` Mongo 外壳。
- 如果希望减少单文件体积，`chatos_memory_mappings.rs` 仍是 repository 中的大文件，但拆分前需要更谨慎地按 contact/project/link 边界切。

## 第 17 轮：继续接入 repository Mongo CRUD helper

状态：已完成

目标：
- 继续第 16 轮的 repository CRUD helper 线，把更多 `Document` 型 Mongo CRUD 外壳接入公共 helper。
- 优先处理 `terminals.rs` 和 `remote_connections`，它们边界清楚，且和已抽 helper 匹配度高。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `terminals.rs` 仍有多处重复 Mongo 样板：
  - `find_one` 按 id / project run filter 查询。
  - `insert_one` 创建 terminal。
  - `update_one` 更新 status / last_active_at。
  - `delete_one` 删除 terminal。
- `remote_connections/read_ops.rs` / `write_ops.rs` 也有主表 `find_one/insert/update/delete` 重复外壳。
- `agents.rs` 使用 typed collection `Agent`，不适合强行接入当前 `Document` helper，本轮先不动。

推进记录：
- `terminals.rs` 改用：
  - `mongo_find_one_doc`
  - `mongo_insert_doc`
  - `mongo_update_set_doc`
  - `mongo_delete_one_doc`
- `remote_connections/read_ops.rs` 的单条读取改用 `mongo_find_one_doc`。
- `remote_connections/write_ops.rs` 的主表 create/update/touch/delete 改用 `mongo_insert_doc`、`mongo_update_set_doc`、`mongo_delete_one_doc`。
- remote connection 的加密/解密、SQL 分支和字段映射保持不变。

结果：
- terminals / remote connections 的 Mongo 主表 CRUD 样板进一步减少。
- 当前 `Document` 型 repository 的公共 helper 复用范围扩大。
- 没有引入 typed collection 泛化，避免把 `Agent` 这类 serde model collection 过早混入 `Document` helper。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过且无 warning。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续 repository 线时，可以处理 `mcp_configs`、`system_contexts` 或 `session_mcp_servers` 中的主表 CRUD 样板。
- 如果继续处理 typed collection，建议单独设计泛型 helper，不要混进当前 `Document` helper。

## 第 18 轮：接入 mcp/system context/session repository Mongo helper

状态：已完成

目标：
- 继续 repository Mongo CRUD helper 接入。
- 处理 `mcp_configs`、`system_contexts`、`session_mcp_servers` 中的 `Document` 型 Mongo 样板。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `mcp_configs` 读写模块仍有主表 `find_one/insert/update/delete` 和关联表 `delete_many` 样板。
- `session_mcp_servers` 有 `insert_one` 和按 session/config 条件 `delete_many` 样板。
- `system_contexts` 有主表 `find_one/insert/update/delete`、激活状态 `update_many/update_one`、关联应用 `delete_many` 等重复外壳。
- 这些都属于 `Document` 型 Mongo 操作，适合复用现有 helper；列表查询、`insert_many` 和业务字段映射本轮不抽。

推进记录：
- 在 `chat_app_server_rs/src/repositories/db.rs` 新增：
  - `mongo_update_many_set_doc`
  - `mongo_delete_many_doc`
- `mcp_configs/read_ops.rs` 单条读取改用 `mongo_find_one_doc`。
- `mcp_configs/write_ops.rs` 主表 create/update/delete 和关联清理 delete-many 改用公共 helper。
- `session_mcp_servers.rs` 的 add/delete 改用 `mongo_insert_doc`、`mongo_delete_many_doc`。
- `system_contexts.rs` 的主表读取/创建/更新/删除、激活状态更新、关联应用删除改用公共 helper。

结果：
- repository 中更多 `Document` 型 Mongo CRUD 外壳被统一到 `repositories/db.rs`。
- `system_contexts` 的批量 `$set` 和 delete-many 样板也有了公共入口。
- SQL 分支、业务字段映射、列表查询和 insert-many 逻辑保持不变。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续 repository 线时，可以再看 `ai_model_configs.rs`、`project_run_*` 以外的 Document CRUD 是否还能接入 helper。
- typed collection 如 `agents.rs` 需要单独设计泛型 helper，暂时不要和 `Document` helper 混在一起。

## 第 19 轮：接入 ai model configs 与 terminal logs Mongo helper

状态：已完成

目标：
- 继续 repository Mongo CRUD helper 接入。
- 处理 `ai_model_configs.rs` 和 `terminal_logs.rs` 中仍保留的 `Document` 型 Mongo 样板。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `ai_model_configs.rs` 有主表 `find_one/insert/update/delete` 重复外壳，secret backfill 里也有单字段 `$set` 样板。
- `terminal_logs.rs` 有 `insert_one` 创建日志和 `delete_many` 清理日志样板。
- 两个文件的列表查询、分页、加解密和 SQL 分支都带业务语义，本轮不抽。

推进记录：
- `ai_model_configs.rs` 改用：
  - `mongo_find_one_doc`
  - `mongo_insert_doc`
  - `mongo_update_set_doc`
  - `mongo_delete_one_doc`
- `backfill_ai_model_config_secret_storage` 的 Mongo 单字段 `api_key` 更新改用 `mongo_update_set_doc`。
- `terminal_logs.rs` 的 create/delete-many 改用 `mongo_insert_doc`、`mongo_delete_many_doc`。

结果：
- ai model configs / terminal logs 的 Mongo CRUD 外壳进一步收口。
- secret 加解密判断、has_api_key 计算、日志分页查询和 SQL 分支保持不变。
- 当前 `Document` helper 已覆盖多数常规 repository Mongo CRUD 场景。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- repository helper 线可以继续做剩余 `Document` 型文件的小范围接入；如果继续收益变小，可以转向拆分 `chatos_memory_mappings.rs`。
- typed collection helper 仍建议单独设计，避免污染当前 `Document` helper。

## 第 20 轮：补齐复杂 Mongo update helper 并接入小模块

状态：已完成

目标：
- 继续 repository Mongo helper 线。
- 为不能安全套用 `mongo_upsert_set_doc` 的复杂 update 文档提供公共外壳。
- 接入 `user_settings`、`session_runtime_settings`、`mcp_configs/app_links` 这些小而明确的模块。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `user_settings` 的 Mongo upsert 需要保留 `settings: Bson::Null` 的可能性，不能走会过滤 `Bson::Null` 的 `mongo_upsert_set_doc`。
- `session_runtime_settings` 的 Mongo upsert 包含 `$set`、`$setOnInsert`、可选 `$unset`，也不能简化成单纯 `$set` helper。
- `mcp_configs/app_links` 有明确的关联表 `delete_many` 样板，适合直接接入已有 helper。

推进记录：
- 在 `chat_app_server_rs/src/repositories/db.rs` 新增 `mongo_update_one_doc`，接收完整 update document 和可选 `UpdateOptions`。
- `mongo_update_set_doc` 和 `mongo_upsert_set_doc` 内部改为复用 `mongo_update_one_doc`。
- `user_settings.rs` 的 `find_one` 和 upsert update 改用 `mongo_find_one_doc`、`mongo_update_one_doc`。
- `session_runtime_settings.rs` 的 `find_one` 和复杂 upsert update 改用 `mongo_find_one_doc`、`mongo_update_one_doc`。
- `mcp_configs/app_links.rs` 的关联清理改用 `mongo_delete_many_doc`。

结果：
- 复杂 Mongo update 的错误处理和 collection 调用样板进一步收口。
- 保留了原本的 `$set/$setOnInsert/$unset`、`Bson::Null` 写入语义。
- 小模块中的剩余 `Document` CRUD 样板继续减少。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰的 Rust 文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- repository helper 线剩余收益开始变小，建议下一轮转向拆分 `chatos_memory_mappings.rs`，按 contact/project/link 三块切分。
- 或者单独设计 typed collection helper，再处理 `agents.rs` 这类 typed collection。

## 第 21 轮：拆分 chatos_memory_mappings repository 大文件

状态：已完成

目标：
- 处理 `chat_app_server_rs/src/repositories/chatos_memory_mappings.rs` 这个 1234 行热点文件。
- 按联系人、记忆项目、项目联系人绑定三类职责拆分，保留原有 repository 模块出口。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- 原文件同时包含 `chatos_contacts`、`chatos_memory_projects`、`chatos_project_agent_links` 三组 Mongo/SQLite 双后端逻辑。
- `normalize_optional_text` 与 `default_project_name` 是跨分支小 helper，适合独立到 support 模块。
- 上层服务通过 `repositories::chatos_memory_mappings::*` 调用，外部路径可以保持不变。

推进记录：
- `chatos_memory_mappings.rs` 改为模块入口，只保留子模块声明和 public re-export。
- 新增 `chatos_memory_mappings/contacts.rs`，承接联系人列表、按 id/agent 查询、幂等创建、task runner 配置更新和删除。
- 新增 `chatos_memory_mappings/projects.rs`，承接记忆项目查询、upsert、批量 id 查询和列表。
- 新增 `chatos_memory_mappings/project_links.rs`，承接项目联系人绑定 upsert、session touch、删除和列表。
- 新增 `chatos_memory_mappings/support.rs`，承接文本归一化与默认项目名 helper。

结果：
- repository 入口文件从 1234 行降到 20 行。
- 新子模块行数：`contacts.rs` 404 行，`projects.rs` 322 行，`project_links.rs` 495 行，`support.rs` 14 行。
- 原有公开类型和函数通过 re-export 保持可用，服务层/API 层无需改调用路径。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续沿 memory mappings 业务线，可以检查 `services/chatos_memory_mappings.rs` 是否也需要按 DTO/contacts/projects/links 拆分。
- 如果转回 repository helper 线，可单独设计 typed collection helper，但不要和当前 `Document` helper 混用。

## 第 22 轮：拆分 chatos_memory_mappings service 层

状态：已完成

目标：
- 沿第 21 轮的 memory mappings 业务线继续处理 service 层。
- 将 `chat_app_server_rs/src/services/chatos_memory_mappings.rs` 按联系人、项目、项目联系人绑定、记忆查询和 DTO helper 拆分。
- 保持 `services::chatos_memory_mappings::*` 对上层 API/运行时的调用面不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- 原 service 文件 615 行，混合了 contact CRUD/task runner config、project sync/list、project contact link sync/list、memory engine 查询和 DTO 映射。
- 与 repository 层刚完成的 contacts/projects/project_links 边界高度一致，适合继续用相同业务边界拆分。
- `ContactTaskRunnerRuntimeConfig` 当前没有外部直接命名引用，只作为返回类型在子模块内定义即可。

推进记录：
- `services/chatos_memory_mappings.rs` 改为模块入口，只保留子模块声明和函数 re-export。
- 新增 `services/chatos_memory_mappings/contacts.rs`，承接联系人列表、详情、创建、删除、task runner runtime config 和配置更新。
- 新增 `services/chatos_memory_mappings/projects.rs`，承接记忆项目同步和列表。
- 新增 `services/chatos_memory_mappings/project_links.rs`，承接项目联系人绑定同步、当前 session touch、解绑和项目联系人列表。
- 新增 `services/chatos_memory_mappings/memories.rs`，承接联系人项目记忆、联系人项目列表和 agent recall 查询。
- 新增 `services/chatos_memory_mappings/support.rs`，承接 DTO 转换、非空文本归一化和 timestamp helper。

结果：
- service 入口文件从 615 行降到 20 行。
- 新子模块行数：`contacts.rs` 125 行，`projects.rs` 55 行，`project_links.rs` 215 行，`memories.rs` 154 行，`support.rs` 82 行。
- API 层和运行时仍通过原 service 模块函数调用，不需要同步改路径。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- memory mappings 业务线已经完成 repository/service 双层拆分，后续可继续看 API 层 `api/memory_mappings.rs` / `api/contacts.rs` 是否存在 DTO 拼装重复。
- 若切回全局优化，建议重新扫描剩余大文件，优先处理仍超过 700-900 行且职责边界清晰的文件。

## 第 23 轮：拆分 project_run environment discovery 的 hint/config 收集

状态：已完成

目标：
- 处理 `chat_app_server_rs/src/services/project_run/environment_discovery.rs` 这个 1369 行热点文件。
- 先抽出边界清晰、行为风险低的项目工具链 hint 解析和配置文件摘要收集。
- 保持 `environment.rs` 仍通过 `environment_discovery::{collect_project_config_files, discover_toolchain_options}` 调用。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- 原文件同时包含系统/路径工具链发现、项目本地工具链发现、`.tool-versions`/`.sdkmanrc`/`go.mod` 等 hint 解析、配置文件摘要收集和最终排序逻辑。
- `collect_project_toolchain_hints` 只服务于当前文件内部的排序偏好。
- `collect_project_config_files` 是对外给 `environment.rs` 使用的稳定出口，需要通过原模块 re-export。

推进记录：
- 新增 `environment_discovery/hints.rs`，承接 `ProjectToolchainHints`、版本 hint 解析、`.tool-versions`/`.sdkmanrc`/Rust/Go hint 读取。
- 新增 `environment_discovery/config_files.rs`，承接项目配置文件候选表、预览读取和 `collect_project_config_files`。
- `environment_discovery.rs` 保留工具链发现主流程、系统路径/Java/Homebrew/项目本地工具链发现和排序逻辑。
- 因 `environment_discovery.rs` 通过 `#[path]` 接入，子模块显式声明为 `#[path = "environment_discovery/..."]`。

结果：
- `environment_discovery.rs` 从 1369 行降到 923 行。
- 新模块行数：`hints.rs` 214 行，`config_files.rs` 248 行。
- 对外函数路径保持不变，`environment.rs` 无需同步调整。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续拆 `environment_discovery.rs` 时，可再把系统工具链发现与项目本地工具链发现拆成 `system_toolchains.rs` / `project_toolchains.rs`。
- 另一个高价值方向是继续收敛 `TasksPage.tsx` / `TaskDetailDrawer.tsx`，减少 task runner 前端页面容器压力。

## 第 24 轮：修复模型流式响应 UTF-8 chunk 乱码

状态：已完成

目标：
- 排查模型返回内容偶发出现 `���` replacement character 的原因。
- 修复流式响应解析时中文字符跨网络 chunk 被破坏的问题。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

原因定位：
- 模型响应主链路为 `AiRequestHandler -> parse_stream_response -> consume_sse_stream(response.bytes_stream())`。
- `crates/chatos_ai_runtime/src/stream.rs` 原先对每个 HTTP bytes chunk 直接执行 `String::from_utf8_lossy(&bytes)`。
- 当中文等多字节 UTF-8 字符被网络层切成多个 chunk 时，单 chunk 解码会把不完整字节替换成 `�`，后续 JSON/SSE 解析拿到的文本就已经损坏。
- 这与页面中出现 `我���是...` 的现象一致：一个 3 字节中文字符被拆开后变成多个 replacement character。

推进记录：
- 在 `crates/chatos_ai_runtime/src/stream.rs` 新增增量 `Utf8ChunkDecoder`。
- `consume_sse_stream` 改为先累积 bytes，只有形成完整 UTF-8 片段后才追加到 SSE 文本 buffer。
- EOF 时只对真正残留的不完整/非法字节做 lossy fallback，避免正常网络切块产生乱码。
- 为 runtime crate 新增中文字符跨 3 个 chunk 的回归用例。
- 同步更新 `chat_app_server_rs/src/services/ai_common/stream_support.rs` 的测试用 SSE helper，并补同类回归用例，避免测试 helper 继续复制旧问题。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chatos_ai_runtime` 通过。
- `cargo check -p chatos_ai_runtime --tests` 通过，仅编译测试目标，未执行测试。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- `cargo check -p chat_app_server_rs --tests` 未通过，但失败点是既有 test-only 编译问题：`ui_prompt_manager/normalizer/fields.rs` 引用缺失的 `trimmed`，以及 `chat_runtime_contact/prompt_builder.rs` 中 `plugin_entries` / `skill_entries` test cfg 下可能未初始化；与本轮 UTF-8 修复无关。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 如果继续排查模型文本异常，可观察是否还有非流式响应路径或前端展示层出现 `�`；当前主流式模型入口已经修复。
- 后续可单独清理 `chat_app_server_rs --tests` 的既有编译问题。

## 第 25 轮：继续拆分 project_run environment discovery 工具链发现

状态：已完成

目标：
- 继续处理 `chat_app_server_rs/src/services/project_run/environment_discovery.rs`。
- 将系统/路径工具链发现、项目本地工具链发现和共享 push/list helper 从主 orchestration 文件里拆出去。
- 保持 `discover_toolchain_options` 与 `collect_project_config_files` 的外部调用面不变。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- 第 23 轮后 `environment_discovery.rs` 仍有 925 行。
- 剩余逻辑主要分为四块：共享 option 写入 helper、系统工具链发现、项目本地工具链发现、最终合并排序。
- 系统发现和项目本地发现只通过 `ToolchainOptions` / `ToolchainSeen` 写入同一组结果，适合拆成子模块。

推进记录：
- 新增 `environment_discovery/support.rs`，承接 `ToolchainOptions`、`ToolchainSeen`、`push_option*`、`discover_direct_file_option`、`list_child_dirs`、`java_home_candidate`。
- 新增 `environment_discovery/system_toolchains.rs`，承接 `JAVA_HOME`、PATH 命令、Homebrew、SDKMAN/asdf/pyenv/nvm 等系统/用户工具链发现，以及 Maven/Gradle 用户配置发现。
- 新增 `environment_discovery/project_toolchains.rs`，承接项目内 JDK、`.venv`、`.node`、`.cargo`、wrapper 和项目 Maven/Gradle 配置发现。
- `environment_discovery.rs` 保留主流程、手动 custom toolchains 注入、hint 匹配排序和 source priority。

结果：
- `environment_discovery.rs` 从 925 行降到 268 行。
- 新模块行数：`support.rs` 157 行，`system_toolchains.rs` 267 行，`project_toolchains.rs` 261 行。
- `environment.rs` 的调用路径保持不变。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chat_app_server_rs` 通过。
- 根目录 `cargo check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `environment_discovery` 已经从 1369 行拆到 268 行，当前可以先停在这里。
- 后续更有价值的方向是 `crates/chatos_ai_runtime/src/memory_context.rs` 或前端 `TaskDetailDrawer.tsx` / `TasksPage.tsx`。

## 第 26 轮：拆分 chatos_ai_runtime memory/runtime/task 热点入口

状态：已完成

目标：
- 继续处理 `crates/chatos_ai_runtime` 里的热点文件，优先降低运行时核心入口文件的职责密度。
- 保持外部 public API 不变，调用方仍从 `memory_context` / `runtime` / `task` 原路径使用类型和函数。
- 本轮继续只做编译级验证，不启动项目，不执行测试。

初始观察：
- `memory_context.rs` 仍承载 compose、record writer、best-effort writer、log summary、测试等多类职责，上一轮前为 1316 行。
- `runtime.rs` 仍把主循环、输入修复、debug payload、持久化判断、日志摘要放在同一个文件里。
- `task.rs` 仍承载 task spec、runtime config、run report、execution wrapper、runtime facade 等多个公共模型。

推进记录：
- 新增 `memory_context/compose_items.rs`，承接 compose context response 到模型 input item 的转换，以及 tool call/tool output 配对逻辑。
- 新增 `memory_context/log_summary.rs`，承接 memory record 保存日志摘要。
- 新增 `memory_context/record_writer.rs`，承接 `MemoryRecordScope`、`MemoryEngineRecordWriter`、`BestEffortMemoryRecordWriter` 和批量同步逻辑。
- 新增 `memory_context/tests.rs`，把原本内联在主文件里的测试移出。
- 新增 `runtime/input_items.rs`，承接 pending tool turn 修复、runtime follow-up input 追加、debug payload 注入和 input 计数。
- 新增 `runtime/persistence.rs`，承接 tool result 持久化过滤和 option 字符串归一化。
- 新增 `runtime/summaries.rs`，承接 tool call/tool result 名称摘要。
- 新增 `task/config.rs`，承接 `TaskRuntimeConfig` 与 `TaskMcpInitMode`。
- 新增 `task/report.rs`，承接 `TaskRunReport`。

结果：
- `memory_context.rs` 从 1316 行降到 224 行。
- `runtime.rs` 从 620 行降到 521 行。
- `task.rs` 从 734 行降到 509 行。
- 新模块行数：`memory_context/record_writer.rs` 368 行，`memory_context/tests.rs` 374 行，`memory_context/compose_items.rs` 186 行，`memory_context/log_summary.rs` 199 行；`runtime/input_items.rs` 73 行，`runtime/persistence.rs` 26 行，`runtime/summaries.rs` 26 行；`task/config.rs` 174 行，`task/report.rs` 67 行。

验证：
- `rustfmt --edition 2024` 已覆盖本轮触碰文件。
- `cargo check -p chatos_ai_runtime` 通过。
- `cargo check -p chatos_ai_runtime --tests` 通过，仅编译测试目标，未执行测试。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `chatos_ai_runtime` 三个入口文件已经完成一轮低风险拆分，后续可继续拆 `task.rs` 的 `TaskRunSpec`/execution wrapper，或切回更高价值的前端大页面。
- 当前剩余明显热点仍包括 `task_runner_service/frontend/src/i18n/messages.ts`、`TasksPage.tsx`、`RunsPage.tsx`，以及 server 侧 `workspace_realtime_watcher.rs` / `agent_chat.rs` 等大文件。

## 第 27 轮：收尾 task runtime 入口并拆分任务页展示组件

状态：已完成

目标：
- 继续把上一轮未完成的 `crates/chatos_ai_runtime/src/task.rs` 入口文件收干净。
- 开始处理 `task_runner_service/frontend/src/pages/TasksPage.tsx` 这个前端热点页面。
- 保持原 public API、路由和页面交互不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- 第 26 轮后 `task.rs` 仍有 509 行，其中 `TaskRunSpec` 和 `TaskRunExecution` 是两块边界非常清晰的公共模型。
- `TasksPage.tsx` 仍有 1120 行，筛选工具条、批量操作条、表格空态/分页、MCP prompt 预览弹窗都直接内联在页面里。

推进记录：
- 新增 `crates/chatos_ai_runtime/src/task/spec.rs`，承接 `TaskRunSpec`、task metadata、record option 构造和 builtin MCP prompt 注入逻辑。
- 新增 `crates/chatos_ai_runtime/src/task/execution.rs`，承接 `TaskRunExecution` 和 run report wrapper。
- `task.rs` 保留 `TaskRuntime` facade、`ContextualTurnRunner` task report 扩展和 prompt mode/snapshot 类型。
- 新增 `task_runner_service/frontend/src/pages/tasks/TaskListToolbar.tsx`，承接任务页标题、搜索、标签/模型/状态筛选、定时任务开关和刷新/新建按钮。
- 新增 `TaskBatchActionsBar.tsx`，承接选中数量展示和批量运行/设为 ready/归档/删除按钮。
- 新增 `TaskMcpPromptPreviewModal.tsx`，合并任务详情预览与草稿预览两个重复 Modal。
- 新增 `TaskListTable.tsx`，承接任务表格、row selection、分页和空态。

结果：
- `task.rs` 从 509 行降到 139 行；新增 `task/spec.rs` 253 行，`task/execution.rs` 133 行。
- `TasksPage.tsx` 从 1120 行降到 1019 行。
- 新前端组件行数：`TaskListToolbar.tsx` 103 行，`TaskBatchActionsBar.tsx` 55 行，`TaskMcpPromptPreviewModal.tsx` 45 行，`TaskListTable.tsx` 63 行。
- 页面行为入口、query key、mutation 成功后的刷新逻辑和 URL search param 写法保持不变。

验证：
- `rustfmt --edition 2024` 已覆盖本轮 Rust 触碰文件。
- `cargo check -p chatos_ai_runtime` 通过。
- `cargo check -p chatos_ai_runtime --tests` 通过，仅编译测试目标，未执行测试。
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `task.rs` 已经收敛到入口级大小，后续优先继续处理前端 `TasksPage.tsx` 的数据/状态 hook 拆分，或者切到同类热点 `RunsPage.tsx`。
- 另一个高价值方向是 `task_runner_service/frontend/src/i18n/messages.ts`，但它更适合按命名空间整体迁移，改动面会比页面组件拆分更大。

## 第 28 轮：抽出 TasksPage 数据查询与 mutation hook

状态：已完成

目标：
- 继续处理 `task_runner_service/frontend/src/pages/TasksPage.tsx` 的高复杂度问题。
- 本轮不再只搬 JSX，而是把查询、派生数据、mutation 和 query invalidation 从页面入口中抽出。
- 保持页面交互、query key、成功提示、批量操作提示和 URL 参数行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- 第 27 轮后 `TasksPage.tsx` 仍有 1019 行。
- 页面内仍直接承载大量 `useQuery` / `useQueries`、模型/任务/远端服务 Map 派生、任务详情 remote operation 派生、批量运行任务派生。
- 页面内还直接承载 9 个 mutation、全量任务相关 query invalidation、批量操作结果消息拼装。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/tasks/useTasksPageData.ts`。
- 将任务分页查询、任务统计、任务索引、详情任务、最近运行、最后一次运行、运行事件、follow-up、派生任务、prompt、模型、MCP catalog、远端服务、外部 MCP 配置、memory context、memory records、MCP prompt preview 等查询搬入 hook。
- 将 `scheduleModeLabels`、`statusFilterOptions`、`modelOptions`、`modelNameMap`、`modelLabelMap`、`taskSummaryMap`、`prerequisiteTaskOptions`、`tagOptions`、`remoteServerMap`、`externalMcpConfigMap`、详情 remote operation 统计、列表 remote activity、pending prompt count、batch run tasks 等派生数据搬入 hook。
- 新增 `task_runner_service/frontend/src/pages/tasks/useTaskMutations.ts`。
- 将 create/update/delete/run/batch update/batch delete/batch run/summarize memory/draft MCP preview mutation 搬入 hook。
- 将任务相关 query invalidation 和批量操作结果提示拼装搬入 mutation hook。

结果：
- `TasksPage.tsx` 从 1019 行降到 656 行。
- 新增 `useTasksPageData.ts` 410 行，承接查询和派生数据。
- 新增 `useTaskMutations.ts` 183 行，承接请求 mutation、query invalidation 和消息提示。
- 页面主文件现在更集中在本地 UI 状态、URL 参数同步、表单 payload 构建和 JSX 编排。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `TasksPage.tsx` 已经从最初 2080 行降到 656 行，下一步继续拆收益会下降。
- 更高价值的后续方向是把 `useTasksPageData` 内部再按 list/detail/memory 拆小，或转向同类页面 `ModelsPage.tsx` / `RunsPage.tsx` 抽公共 `PageTableShell` 与分页状态 hook。

## 第 29 轮：拆分 ModelsPage 数据层、mutation 和列表 UI

状态：已完成

目标：
- 继续处理 task runner 前端的大页面热点，优先选择仍有 1000+ 行的 `ModelsPage.tsx`。
- 先按低风险边界拆分查询/派生数据、mutation/query invalidation、列表工具条/统计/表格。
- 保持模型配置页面的路由参数、query key、按钮行为、筛选行为和表单提交 payload 不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `task_runner_service/frontend/src/pages/ModelsPage.tsx` 有 1035 行。
- 页面内同时承载模型配置查询、用量查询、详情关联任务/运行查询、筛选派生、模型 catalog 下拉选项、create/update/delete/test mutation、列表 columns、筛选工具条和统计条。
- 这些逻辑与 `TasksPage` 前几轮的问题类似，适合先按页面子域拆到 `pages/models`。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/models/modelPageUtils.ts`。
- 将模型表单类型、enabled filter 类型、provider 规范化、默认 base URL、thinking level 选项、模型下拉选项构造移入工具层。
- 新增 `useModelsPageData.ts`，承接 `model-configs`、`model-config-usage`、详情模型、关联任务、关联运行查询，以及 provider/模型筛选、任务数/运行数 Map、统计数量等派生数据。
- 新增 `useModelMutations.ts`，承接 create/update/delete/test mutation，并集中维护模型相关 query invalidation 和消息提示。
- 新增 `ModelListTable.tsx`，承接模型列表 columns、空态、分页和列表操作按钮。
- 新增 `ModelListToolbar.tsx` 和 `ModelStatsBar.tsx`，承接页面标题/筛选/刷新/新建按钮以及列表统计展示。
- 拆分过程中把模型下拉占位项的 `supports_responses` 读取改为 `Form.useWatch('supports_responses')` 输入，避免依赖非响应式的 `form.getFieldValue`。

结果：
- `ModelsPage.tsx` 从 1035 行降到 701 行。
- 新增 `modelPageUtils.ts` 119 行、`useModelsPageData.ts` 180 行、`useModelMutations.ts` 84 行。
- 新增 `ModelListTable.tsx` 159 行、`ModelListToolbar.tsx` 81 行、`ModelStatsBar.tsx` 28 行。
- 页面主文件现在更集中在 drawer/form/detail 编排和 catalog preview 这类仍需共享表单状态的逻辑。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `ModelsPage.tsx` 仍有 701 行，后续可以继续拆 `ModelFormDrawer` 和 `ModelDetailDrawer`，但收益已低于本轮。
- 当前更高价值的下一个目标是 `RunsPage.tsx`，它仍有约 936 行，并且与 `TasksPage` / `ModelsPage` 有明显的列表、详情抽屉、事件展示重复。

## 第 30 轮：拆分 RunsPage 数据层与列表视图

状态：已完成

目标：
- 继续处理 task runner 前端页面热点，选择 `RunsPage.tsx`。
- 先拆查询/派生数据，再拆列表筛选工具条和表格，保留详情抽屉结构不动。
- 保持运行列表的 URL 参数、分页、SSE 刷新、取消/重试 mutation 和详情打开行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `task_runner_service/frontend/src/pages/RunsPage.tsx` 有 936 行。
- 页面内混合了运行列表查询、任务/模型/远端服务查询、详情运行查询、事件/Prompt 查询、任务搜索、工具调用/工具结果/远端操作统计派生、列表 columns 和筛选工具条。
- 现有 `pages/runs` 下已有 `payloadView.tsx` 和 `runEventUtils.tsx`，适合继续沿这个目录扩展。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/runs/runPageUtils.ts`。
- 将 run status filter 类型、运行状态颜色、Prompt 状态颜色、运行状态筛选值移入工具层。
- 新增 `useRunsPageData.ts`，承接 runs page 查询、模型查询、远端服务查询、选中 run 查询、run events 查询、run prompts 查询、任务 summaries 查询、任务搜索查询。
- 将 `taskMap`、`selectedRun`、`selectedToolCalls`、`selectedToolResults`、`selectedModelRequests`、stream stats、task/model options、modelNameMap、remote operation stats 等派生数据移入 hook。
- 新增 `RunListToolbar.tsx`，承接运行页标题、任务筛选、模型筛选、状态筛选、清空筛选和刷新按钮。
- 新增 `RunListTable.tsx`，承接运行列表 columns、分页、空态、详情/取消/重试按钮。

结果：
- `RunsPage.tsx` 从 936 行降到 667 行。
- 新增 `runPageUtils.ts` 28 行、`useRunsPageData.ts` 227 行。
- 新增 `RunListTable.tsx` 159 行、`RunListToolbar.tsx` 85 行。
- 页面主文件现在更集中在 URL 参数、本地分页状态、SSE 刷新、mutation 和详情抽屉编排。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `RunsPage.tsx` 仍有 667 行，后续可以继续拆详情抽屉中的 remote operations、tool calls/results、model requests、prompts/events 区块。
- 从整体价值看，下一步可优先处理同类前端热点 `McpCatalogPage.tsx` / `ServersPage.tsx`，或回到后端大文件 `workspace_realtime_watcher.rs`。

## 第 31 轮：继续拆分 RunsPage 详情抽屉

状态：已完成

目标：
- 继续沿第 30 轮处理 `RunsPage.tsx`，把剩余最大的详情抽屉 JSX 从主页面中移出。
- 按展示区块拆分 remote operations、tool calls/results、model requests、prompts、events timeline 和 summary。
- 保持详情抽屉打开/关闭、取消/重试、跳转任务/模型/服务器/Prompt、Prompt 分页、事件展示行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- 第 30 轮后 `RunsPage.tsx` 仍有 667 行。
- 主页面里剩余最大块是第 240-664 行的详情抽屉，其中包含多个纯展示区块，和页面的 URL / mutation 编排混在一起。

推进记录：
- 新增 `RunDetailDrawer.tsx`，作为运行详情抽屉总装层。
- 新增 `RunDetailSummary.tsx`，承接详情操作按钮、基础描述和 stream chunk 统计。
- 新增 `RunRemoteOperationsSection.tsx`，承接远端操作统计、服务器跳转、远端命令/路径/输出/结构化结果展示。
- 新增 `RunToolSections.tsx`，承接工具调用计划和工具结果两个展示区。
- 新增 `RunModelRequestsSection.tsx`，承接模型请求 payload 展示。
- 新增 `RunPromptsSection.tsx`，承接运行相关 Prompt 列表、状态、分页和打开动作。
- 新增 `RunEventsTimeline.tsx`，承接运行事件时间线和事件 payload 展示。
- `RunsPage.tsx` 现在只向详情组件传入已派生好的数据和回调，不再直接承载详情 JSX。

结果：
- `RunsPage.tsx` 从 667 行降到 257 行。
- 新增详情相关组件行数：`RunDetailDrawer.tsx` 181 行、`RunDetailSummary.tsx` 137 行、`RunRemoteOperationsSection.tsx` 160 行、`RunToolSections.tsx` 102 行、`RunModelRequestsSection.tsx` 50 行、`RunPromptsSection.tsx` 100 行、`RunEventsTimeline.tsx` 56 行。
- `RunsPage.tsx` 已从最初约 936 行降到 257 行，主文件边界基本收敛为 route/query/mutation 编排。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `RunsPage.tsx` 这一轮已经接近收尾，继续拆收益明显下降。
- 下一步更高价值目标建议转向 `McpCatalogPage.tsx`、`ServersPage.tsx`、`PromptsPage.tsx` 这几个仍偏大的 task runner 前端页面，或者切回后端大文件 `workspace_realtime_watcher.rs`。

## 第 32 轮：拆分 McpCatalogPage 页面入口、builtin tab 与 external config

状态：已完成

目标：
- 继续处理 task runner 前端热点页面，选择 `McpCatalogPage.tsx`。
- 将外部 MCP server metadata、builtin catalog/prompt preview、external config 管理拆到 `pages/mcpCatalog` 子目录。
- 保持 tab 结构、query key、外部配置表单字段、JSON 解析规则、创建/编辑/删除 mutation 和 Prompt preview 参数不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `task_runner_service/frontend/src/pages/McpCatalogPage.tsx` 原有 933 行。
- 页面同时承载 external server metadata 卡片、builtin catalog 表格、RemoteConnectionController 摘要、MCP prompt preview 控件、external MCP config 列表和编辑抽屉。
- external config 区域有独立查询/mutation/表单 payload 构建，builtin 区域也有独立查询和筛选状态，适合按 tab 拆分。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/mcpCatalog/mcpCatalogPageUtils.ts`。
- 将 external config 表单类型、卡片样式、tool profile 颜色、external config payload 构建、JSON 解析、profile label/description 映射移入工具层。
- 新增 `ExternalMcpConfigTab.tsx`，承接 external MCP config 查询、创建/更新/删除 mutation、列表、路线图卡片和创建/编辑抽屉。
- 新增 `ExternalMcpServerCard.tsx`，承接 external MCP server metadata、HTTP/stdio endpoint、tool profile 列表展示。
- 新增 `BuiltinMcpCatalogTab.tsx`，承接 builtin catalog 查询、remote server 摘要、prompt preview 状态/查询和 builtin catalog 表格展开项。
- `McpCatalogPage.tsx` 现在只保留页面标题、server info 查询和三个 Tabs 的编排。

结果：
- `McpCatalogPage.tsx` 从 933 行降到 64 行。
- 新增 `mcpCatalogPageUtils.ts` 126 行、`BuiltinMcpCatalogTab.tsx` 323 行、`ExternalMcpConfigTab.tsx` 351 行、`ExternalMcpServerCard.tsx` 132 行。
- 页面入口已经收敛为轻量壳，三个 tab 的业务逻辑分别归档到独立文件。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `McpCatalogPage.tsx` 主文件已经完成收口，继续拆收益主要集中在 `ExternalMcpConfigTab.tsx` 内部的表单抽屉/表格。
- 更高价值的下一步建议处理 `ServersPage.tsx` 或 `PromptsPage.tsx`，它们仍是 800 行级别页面。

## 第 33 轮：拆分 ServersPage 数据层与列表视图

状态：已完成

目标：
- 继续处理 task runner 前端页面热点，选择 `ServersPage.tsx`。
- 先拆查询/筛选派生、列表工具条、统计条和表格，保留编辑抽屉、详情抽屉、测试结果 Modal 在主页面中。
- 保持远端服务器 query key、筛选条件、创建/更新/删除/测试 mutation、URL `server_id` 行为和表单 payload 不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `task_runner_service/frontend/src/pages/ServersPage.tsx` 原有 832 行。
- 页面内混合了远端服务器列表查询、详情查询、筛选 options、filteredServers 派生、统计数量、表格 columns、编辑抽屉、详情抽屉、测试结果 Modal 和 payload 构建 helper。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/servers/serverPageUtils.tsx`。
- 将远端服务器表单类型、enabled filter 类型、auth type 文案映射、host key policy 选项、creator label、payload 构建、test payload 构建、auth/host policy normalize 和测试状态渲染移入工具层。
- 新增 `useServersPageData.ts`，承接远端服务器列表查询、选中服务器查询、筛选 options、selectedServer、filteredServers 和统计数量派生。
- 新增 `ServerListToolbar.tsx`，承接标题、搜索、认证方式筛选、启用状态筛选、清空筛选、刷新和新建按钮。
- 新增 `ServerStatsBar.tsx`，承接 visible/enabled/test passed/strict 统计展示。
- 新增 `ServerListTable.tsx`，承接远端服务器表格 columns、空态、分页、详情/编辑/测试/删除按钮。

结果：
- `ServersPage.tsx` 从 832 行降到 533 行。
- 新增 `serverPageUtils.tsx` 116 行、`useServersPageData.ts` 119 行。
- 新增 `ServerListTable.tsx` 171 行、`ServerListToolbar.tsx` 82 行、`ServerStatsBar.tsx` 28 行。
- 主文件现在更集中在 mutation、编辑抽屉、详情抽屉和测试结果 Modal。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `ServersPage.tsx` 仍有 533 行，下一步可以继续拆 `ServerEditorDrawer`、`ServerDetailDrawer` 和 `ServerTestResultModal`。
- 如果优先追求覆盖面，也可以转向 `PromptsPage.tsx`，它仍是 800 行级别页面。

## 第 34 轮：继续拆分 ServersPage 编辑/详情抽屉和测试结果 Modal

状态：已完成

目标：
- 继续沿第 33 轮处理 `ServersPage.tsx`，把剩余三块 UI 大段从主页面移出。
- 拆分 `ServerEditorDrawer`、`ServerDetailDrawer` 和 `ServerTestResultModal`。
- 保持表单字段、草稿测试、保存、详情测试、详情编辑/删除、测试结果展示和 URL `server_id` 行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- 第 33 轮后 `ServersPage.tsx` 仍有 533 行。
- 主页面中剩余主要体积是编辑抽屉、详情抽屉和测试结果 Modal，均是纯展示/表单 UI，状态和 mutation 可以继续留在主页面编排。

推进记录：
- 新增 `ServerEditorDrawer.tsx`，承接远端服务器创建/编辑表单、auth type 条件字段、host key policy、草稿测试按钮和保存按钮。
- 新增 `ServerDetailDrawer.tsx`，承接远端服务器详情描述、测试/编辑/删除按钮、连接信息、认证信息、最近测试信息和提示文案。
- 新增 `ServerTestResultModal.tsx`，承接远端服务器测试结果展示。
- `ServersPage.tsx` 继续保留 mutation、表单初始化、URL 参数、打开/关闭逻辑和回调串联。

结果：
- `ServersPage.tsx` 从 533 行降到 318 行。
- 新增 `ServerEditorDrawer.tsx` 172 行、`ServerDetailDrawer.tsx` 141 行、`ServerTestResultModal.tsx` 63 行。
- `ServersPage.tsx` 已从最初 832 行降到 318 行，主文件边界基本收敛为状态、mutation 和页面编排。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `ServersPage.tsx` 已接近收尾，继续拆收益下降。
- 下一轮更高价值建议转向 `PromptsPage.tsx`，或者处理后端热点 `workspace_realtime_watcher.rs`。

## 第 35 轮：拆分 PromptsPage 数据层与列表视图

状态：已完成

目标：
- 继续处理 task runner 前端页面热点，选择 `PromptsPage.tsx`。
- 先拆 prompts 查询/派生数据、列表筛选工具条和表格，保留详情抽屉与提交/取消表单在主页面中。
- 保持 prompt/task/run/status URL 参数、分页、query key、提交/取消 mutation 和详情打开行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `task_runner_service/frontend/src/pages/PromptsPage.tsx` 原有 811 行。
- 页面内混合了 prompts 分页查询、选中 prompt 查询、任务/运行 summaries 查询、模型配置查询、任务/运行筛选 options、task/run/model Map、表格 columns 和详情抽屉表单。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/prompts/promptPageUtils.ts`。
- 将 prompt status filter 类型、状态颜色和状态筛选值移入工具层。
- 新增 `usePromptsPageData.ts`，承接 prompts 分页查询、选中 prompt 查询、task/run summaries 查询、task/run 搜索查询、model configs 查询，以及 selectedPrompt、taskMap、runMap、modelMap、taskOptions、runOptions 派生。
- 新增 `PromptListToolbar.tsx`，承接任务筛选、运行筛选、状态筛选、清空筛选和刷新按钮。
- 新增 `PromptListTable.tsx`，承接 prompt 表格 columns、分页、空态、打开任务/运行/详情按钮。

结果：
- `PromptsPage.tsx` 从 811 行降到 550 行。
- 新增 `promptPageUtils.ts` 19 行、`usePromptsPageData.ts` 213 行。
- 新增 `PromptListTable.tsx` 142 行、`PromptListToolbar.tsx` 92 行。
- 主文件现在更集中在 prompt 详情抽屉、输入字段/选择项渲染和提交/取消 mutation。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `PromptsPage.tsx` 仍有 550 行，下一步可以继续拆 `PromptDetailDrawer` 和 prompt payload/form helper。
- 如果切后端，当前高价值热点仍是 `workspace_realtime_watcher.rs`。

## 第 36 轮：继续拆分 PromptsPage 详情抽屉与表单解析 helper

状态：已完成

目标：
- 沿第 35 轮继续处理 `PromptsPage.tsx`。
- 将 prompt 详情抽屉、输入字段/选择项渲染、JSON 展示从主页面移出。
- 将 prompt payload 中 fields/choice 解析、初始表单值构建从主页面移入独立 helper。
- 保持 prompt 详情打开、任务/运行/模型跳转、提交/取消 mutation、表单校验和响应展示行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- 第 35 轮后 `PromptsPage.tsx` 仍有 550 行。
- 主页面剩余主要体积集中在详情 Drawer、pending prompt 表单渲染、选择项校验、response/payload JSON 展示，以及 `extractFields` / `extractChoice` / `buildInitialValues` 等解析 helper。

推进记录：
- 新增 `task_runner_service/frontend/src/pages/prompts/promptDetailUtils.ts`。
- 将 `PromptField`、`PromptChoiceOption`、`PromptChoice` 类型，以及 `buildInitialValues`、`extractFields`、`extractChoice` 和底层安全转换 helper 移入工具层。
- 新增 `PromptDetailDrawer.tsx`，承接 prompt 详情 Drawer、任务/运行/模型跳转按钮、详情描述、pending prompt 表单、选择项校验、取消按钮、响应和原始 payload 展示。
- `PromptsPage.tsx` 保留 URL 参数、分页状态、query 数据接入、提交/取消 mutation、表单初始化和页面编排。
- 清理拆分后主文件中残留的未使用导入和已迁移派生变量。

结果：
- `PromptsPage.tsx` 从 550 行降到 209 行。
- 新增 `promptDetailUtils.ts` 110 行、`PromptDetailDrawer.tsx` 292 行。
- `PromptsPage.tsx` 已从最初 811 行降到 209 行，主文件边界基本收敛为状态、mutation 和页面编排。
- 当前 prompts 子模块文件规模：
  - `usePromptsPageData.ts` 213 行。
  - `PromptDetailDrawer.tsx` 292 行。
  - `PromptListTable.tsx` 142 行。
  - `PromptListToolbar.tsx` 92 行。
  - `promptDetailUtils.ts` 110 行。
  - `promptPageUtils.ts` 19 行。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `PromptsPage.tsx` 已基本收尾，继续拆收益下降。
- 下一轮建议转向当前剩余高价值热点：后端 `workspace_realtime_watcher.rs`，或继续处理 task runner 前端 `McpCatalogPage` 子组件内部偏大的抽屉/表单。

## 第 37 轮：收口 workspace_realtime_watcher 重复变更处理

状态：已完成

目标：
- 处理后端热点 `chat_app_server_rs/src/services/workspace_realtime_watcher.rs`。
- 优先消除 full scan 与 incremental scan 中重复的 change log 写入、缓存失效和 project-run realtime 通知逻辑。
- 收口项目根目录 missing/available 状态更新和通知逻辑。
- 保持 watcher 启动、全量扫描、增量扫描、suppress、目录缓存失效和实时通知行为不变。
- 本轮继续只做编译检查，不启动项目，不执行测试。

初始观察：
- `workspace_realtime_watcher.rs` 原有 1082 行。
- full scan 在 diff 后有一整段与 `process_workspace_changes` 基本相同的逻辑：过滤 suppressed path、懒初始化 `ChangeLogStore`、写 `workspace_scan`、失效目录缓存、分类 project-run 变更并发布 realtime。
- full scan 与 incremental scan 都内联了 project root missing/available 的状态切换和通知代码。

推进记录：
- full scan 计算出 `changes` 后改为复用 `process_workspace_changes(project, changes)`。
- 新增 `mark_project_root_missing`、`mark_project_root_available`、`publish_project_root_status`。
- full/incremental 两条扫描路径都改为调用同一组 root 状态 helper。
- 使用 `rustfmt --edition 2021` 格式化当前 Rust 文件。

结果：
- `workspace_realtime_watcher.rs` 从 1082 行降到 1001 行。
- 单文件 diff 为 38 行新增、119 行删除，净减少 81 行。
- full scan 和 incremental scan 的变更处理语义统一到 `process_workspace_changes`，后续修 change log/realtime 行为时只需要改一处。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

## 第 50 轮：抽象 db_connection_hub 多数据库连接校验与 pool 默认值

状态：已完成

目标：
- 开始处理原方案中未完成的 `db_connection_hub` 多数据库 driver 样板重复问题。
- 优先收口连接层中最稳定的重复逻辑：网络 host/port 校验、认证字段校验、connect timeout 与 pool min/max 默认值读取。
- 不改各数据库协议连接细节、metadata SQL、query execution 和错误码映射语义。
- 本轮只做编译检查，不启动项目，不执行测试。

初始观察：
- `db_connection_hub/backend` 是根目录下的独立 Rust crate，但没有显式声明独立 workspace，直接 `cargo check` 会被根 workspace 拒绝。
- Postgres / MySQL / SQLServer / MongoDB / Oracle / SQLite 的 `connection.rs` 中重复存在：
  - `network.host` / `network.port` 非空校验。
  - password / token / tls client cert / file key 等认证字段校验。
  - `connect_timeout_ms.unwrap_or(5_000)`。
  - sqlx pool `pool_min.unwrap_or(1)` 与 `pool_max` 默认值。
- 这些逻辑与具体数据库协议无关，适合先放到 drivers 公共层。

推进记录：
- 新增 `db_connection_hub/backend/src/drivers/connection_common.rs`。
- 集中 `DEFAULT_CONNECT_TIMEOUT_MS`、`DEFAULT_POOL_MIN`、`DEFAULT_POOL_MAX`、`DEFAULT_SQLITE_POOL_MAX`。
- 新增 `connect_timeout_ms`、`pool_limits`、`require_network_host`、`require_network_port`、`require_sqlite_file_path`。
- 新增 `validate_network_host_port`、`validate_supported_auth_mode`。
- 新增 `validate_password_auth`、`validate_token_auth`、`validate_tls_client_cert_auth`、`validate_file_key_reference`。
- Postgres / MySQL 连接池改为复用 timeout、pool limits、host/port 和 password/token 校验 helper。
- SQLServer 改为复用 host/port、username/password 和 supported auth mode 校验 helper。
- MongoDB 改为复用 host/port、supported auth mode 和 password/token/tls client cert 校验 helper。
- SQLite 改为复用 file path、supported auth mode、timeout 与 sqlite pool limits helper。
- Oracle 改为复用 host/port、supported auth mode、password/tls client cert/file key 校验 helper，保留 Oracle 专属 database/service_name/sid 目标校验。
- 在 `db_connection_hub/backend/Cargo.toml` 追加空 `[workspace]`，显式保持该 backend crate 独立，避免被根 workspace 半识别后无法单独编译。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- 新增 `connection_common.rs` 124 行，承接连接层公共校验与默认值。
- `postgres/connection.rs` 约从 213 行降到 167 行。
- `mysql/connection.rs` 约从 220 行降到 174 行。
- `sqlserver/connection.rs` 约从 204 行降到 154 行。
- `mongodb/connection.rs` 约从 266 行降到 208 行。
- `oracle/connection.rs` 约从 303 行降到 241 行。
- `sqlite/connection.rs` 当前为 104 行，pool/timeout/file path/auth mode 校验也已复用公共 helper。
- 多数据库 driver 的连接参数校验入口更一致，后续继续抽 metadata nodes/detail 时可以复用同一条治理线。

验证：
- `db_connection_hub/backend` 目录下 `cargo check` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续 `db_connection_hub` 线，处理 `metadata/common.rs` 与各数据库 `metadata/nodes.rs` 中的 node id/path/pagination 重复。
- 或转向剩余 server 大文件：`chat_app_server_rs/src/api/agent_chat.rs`、`chat_app_server_rs/src/services/project_run/analyzer.rs`。

## 第 51 轮：修复 fallback 定义查找误伤 Groovy 多行方法声明

状态：已完成

目标：
- 修复第 49 轮收紧 fallback 定义候选后，`.groovy` 文件中 `Map methodName(` / `def methodName(` 这类多行方法声明找不到定义的问题。
- 保持上一轮对普通调用、当前 token 自身过滤的修复不回退。
- 本轮只做编译检查，不启动项目，不执行测试。

问题定位：
- `.groovy` 当前没有专门 provider，会走 code navigation 的文本 fallback。
- 第 49 轮为了避免普通调用混入定义结果，移除了宽泛的 `token(` 加分，并要求 callable definition 更像单行声明。
- Groovy/Java 风格方法声明经常是 `Map step5BuildOrderCreateResult(` 后面参数跨多行，声明行本身不以 `{` 或 `;` 结束，也不包含 `) {`。
- 因此这类真实定义候选被 fallback 过滤，前端显示“没有找到可跳转定义”。

推进记录：
- 在 `chat_app_server_rs/src/services/code_nav/fallback.rs` 中扩展 `looks_like_callable_definition`。
- 新增 `looks_like_callable_declaration_prefix`，识别 `Map method(`、`private static Result method(`、`def method(`、`void method(` 等声明前缀。
- 保留上一轮防误判规则：
  - 前缀为空不算定义。
  - 前缀以 `.` 或 `::` 结尾不算定义。
  - 前缀包含 `=` 或 `->` 不算定义。
  - `return` / `throw` / `new` 之后的 callable 不算定义。
- 这样 `Map result = step5BuildOrderCreateResult(` 仍会被排除，但 `Map step5BuildOrderCreateResult(` 会被视为定义候选。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- fallback 定义查找重新支持 Groovy/Java 风格多行方法声明。
- 保留对普通调用位置和当前光标 token 自身的过滤。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `workspace_realtime_watcher.rs` 仍有 1001 行，下一步可以继续按职责拆 `snapshot/path filtering/diff` 纯逻辑到子模块。
- 也可以转向 `McpCatalogPage` 子组件内部较大的 external config 表单/抽屉，继续压 task runner 前端热点。

## 第 38 轮：拆分 workspace_realtime_watcher 路径与快照采集子模块

状态：已完成

目标：
- 沿第 37 轮继续处理 `workspace_realtime_watcher.rs`。
- 将路径归一化、路径边界判断等纯工具逻辑移出 watcher 主状态机。
- 将文件系统快照采集、scope 扫描、忽略规则、fingerprint 采集移出 watcher 主状态机。
- 保持 full scan、incremental scan、dirty scope、忽略目录、runtime path 忽略和 suppress 逻辑不变。
- 本轮继续只做编译检查，不启动项目，不执行测试。

初始观察：
- 第 37 轮后 `workspace_realtime_watcher.rs` 仍有 1001 行。
- 文件底部混杂了三类职责：watcher 状态机、路径/scope 工具、文件快照采集。
- 路径工具和快照采集大多是纯函数或 blocking scan，适合先拆为同级私有子模块。

推进记录：
- 新增 `chat_app_server_rs/src/services/workspace_realtime_watcher/path_utils.rs`。
- 将 `normalize_path_string`、`normalize_relative_string`、`path_matches_root`、`is_path_within_scope` 移入 `path_utils`。
- 新增 `chat_app_server_rs/src/services/workspace_realtime_watcher/snapshot.rs`。
- 将 `DirtyScopeSnapshot`、`SnapshotCollectResult`、`DirtyScopeCollectResult`、`collect_workspace_snapshot`、`collect_dirty_scope_snapshots`、blocking scan、ignore 判断和 `current_file_fingerprint` 移入 `snapshot`。
- 主文件保留 watcher 生命周期、项目扫描调度、diff、dirty path collapse、change processing 和测试。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `workspace_realtime_watcher.rs` 从 1001 行降到 735 行。
- 新增 `path_utils.rs` 63 行。
- 新增 `snapshot.rs` 220 行。
- watcher 主文件现在更聚焦于 orchestration，文件系统扫描和路径工具可以独立维护。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `workspace_realtime_watcher.rs` 仍有 735 行，继续拆收益开始下降，但还可以考虑把 diff/dirty scope 相关纯逻辑独立出去。
- 更高价值的下一块建议回到 task runner 前端，处理 `McpCatalogPage` 的 external config 子组件或其它仍偏大的页面组件。

## 第 39 轮：拆分 ExternalMcpConfigTab 列表、Roadmap 与编辑 Drawer

状态：已完成

目标：
- 继续处理 task runner 前端 `McpCatalogPage` 子模块中仍偏大的 external config 页签。
- 将 external MCP config 列表卡片、roadmap 卡片和创建/编辑 Drawer 从 `ExternalMcpConfigTab.tsx` 主容器拆出。
- 保持 external MCP config 的查询、创建、更新、删除、表单初始化、transport 条件字段和删除确认行为不变。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `ExternalMcpConfigTab.tsx` 原有 351 行。
- 文件内混合了 React Query mutation、表格 columns、列表标题/Alert、roadmap 描述和 Drawer 表单。
- columns 与 Drawer 都是纯 UI，可先拆出，主容器保留数据和回调编排。

推进记录：
- 新增 `ExternalMcpConfigListSection.tsx`，承接标题、添加按钮、ready Alert、configs 表格、空态、编辑/删除按钮。
- 新增 `ExternalMcpConfigRoadmap.tsx`，承接 runtime/storage/task binding readiness 描述。
- 新增 `ExternalMcpConfigDrawer.tsx`，承接创建/编辑 Drawer、保存/取消按钮、stdio/http 条件表单字段。
- `ExternalMcpConfigTab.tsx` 保留 query/mutation、表单实例、打开/关闭 Drawer、删除确认和 submit payload 构建。

结果：
- `ExternalMcpConfigTab.tsx` 从 351 行降到 154 行。
- 新增 `ExternalMcpConfigDrawer.tsx` 130 行。
- 新增 `ExternalMcpConfigListSection.tsx` 129 行。
- 新增 `ExternalMcpConfigRoadmap.tsx` 34 行。
- `McpCatalogPage.tsx` 主文件此前已降到 64 行，当前 external config 页签也完成明显收口。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `BuiltinMcpCatalogTab.tsx` 仍有 323 行，可以继续拆 profile/card/detail 展示，但收益中等。
- 更高价值可转向剩余大文件 `task_runner_service/frontend/src/i18n/messages.ts` 或继续治理 `workspace_realtime_watcher.rs` 的 diff/dirty scope 纯逻辑。

## 第 40 轮：拆分 task runner 前端 i18n messages 大字典

状态：已完成

目标：
- 处理 `task_runner_service/frontend/src/i18n/messages.ts` 这个 1000 行以上大文件。
- 将中英文 message dictionary 拆成独立 locale 文件。
- 保持 `I18nProvider` 现有 `import { UI_MESSAGES, type UiLocale } from './messages'` 调用方式不变。
- 不修改任何翻译 key/value 内容，只做机械拆分。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `messages.ts` 原有 1370 行。
- 文件只有 `UiLocale`、`MessageDictionary` 类型、中英文两个大对象和 `UI_MESSAGES` 聚合导出。
- 这是典型可安全拆分的字典大文件，拆分后新增/维护单语文案会更容易。

推进记录：
- 新增 `task_runner_service/frontend/src/i18n/messages/types.ts`，承接 `UiLocale` 和 `MessageDictionary`。
- 新增 `messages/zhCN.ts`，承接中文 `zhCN` 字典。
- 新增 `messages/enUS.ts`，承接英文 `enUS` 字典。
- 将原 `messages.ts` 改为聚合入口：导入 `zhCN`、`enUS`，re-export 类型，并导出 `UI_MESSAGES`。
- 拆分过程按原对象边界机械迁移，没有改动翻译文案。

结果：
- `messages.ts` 从 1370 行降到 10 行。
- 新增 `messages/types.ts` 3 行。
- 新增 `messages/zhCN.ts` 682 行。
- 新增 `messages/enUS.ts` 682 行。
- i18n 入口保持兼容，后续每种语言可以独立维护。

验证：
- `npm run type-check` 在 `task_runner_service/frontend` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 已处理掉一个明确大文件热点。
- 后续高价值方向可以继续看 `crates/chatos_ai_runtime/src/task.rs` / `runtime.rs`，或回到 `workspace_realtime_watcher.rs` 做 diff/dirty scope 纯逻辑拆分。

## 第 41 轮：拆分 chatos_ai_runtime 核心 runtime loop

状态：已完成

目标：
- 继续处理方案里标记的 `crates/chatos_ai_runtime/src/runtime.rs` 热点。
- 优先拆 runtime loop 中职责清晰、边界稳定的大块逻辑。
- 保持模型请求、missing tool turn replay、context overflow recovery、transient retry、空最终响应补问、工具执行和记录持久化行为不变。
- 本轮只做 Rust 编译检查，不启动项目，不执行测试。

初始观察：
- `task.rs` 当前只有 139 行，已经不是主要热点。
- `runtime.rs` 原有 521 行，核心问题是 `run_turn` 同时内联了模型请求派发、请求错误恢复、最终响应处理、工具执行和记录持久化。
- 最适合先抽的是不改变状态机所有权的 helper：模型请求派发、工具执行、最终响应判断和请求错误动作判断。

推进记录：
- 新增 `runtime/model_request.rs`。
- 将 request debug 构建、`on_before_model_request` 包装、`StreamCallbacks` / `AiRequestOptions` 组装和 `handle_request_with_options` 调用移入 `dispatch_model_request`。
- 新增 `runtime/tool_execution.rs`。
- 将 tools start/stream/end 回调、`execute_tools_stream`、tool call/output items 构建和工具执行日志移入 `execute_runtime_tools`。
- 新增 `runtime/final_response.rs`。
- 将无 tool calls 响应下的空响应补问、空响应失败和正常完成日志判断移入 `handle_response_without_tool_calls`，并补充 `runtime_result_from_response` 复用结果转换。
- 新增 `runtime/request_error.rs`。
- 将 missing tool turn replay、context overflow recovery 和 transient retry 判断移入 `handle_model_request_error`，主 loop 只根据返回动作更新状态或继续循环。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `runtime.rs` 从 521 行降到 361 行。
- 新增 `model_request.rs` 85 行。
- 新增 `tool_execution.rs` 73 行。
- 新增 `final_response.rs` 65 行。
- 新增 `request_error.rs` 95 行。
- `run_turn` 主流程现在更聚焦于 runtime 状态推进：准备输入、派发请求、处理 action、保存记录、拼接下一轮输入。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `runtime.rs` 已明显收口，继续拆收益下降。
- 后续高价值方向可转向 `crates/chatos_ai_runtime/src/task/` 下已有子模块是否还有重复逻辑，或继续处理 `workspace_realtime_watcher.rs` 的 diff/dirty scope 纯逻辑。

## 第 42 轮：拆分 workspace_realtime_watcher dirty scope 与 diff 纯逻辑

状态：已完成

目标：
- 沿第 38 轮继续处理 `workspace_realtime_watcher.rs`。
- 将 dirty path 归并、dirty scope 分类、scope previous files 截取、workspace diff 计算从 watcher 主流程中拆出。
- 保持 full scan diff、incremental dirty scope scan、create/edit/delete change 生成和测试覆盖行为不变。
- 本轮只做 Rust 编译检查，不启动项目，不执行测试。

初始观察：
- 第 38 轮后 `workspace_realtime_watcher.rs` 仍有 735 行。
- 主文件中剩余一块明显纯逻辑：`apply_dirty_scope_snapshots`、`diff_workspace_files`、`collect_project_dirty_paths`、`classify_dirty_path_scope`、`collapse_dirty_paths`、`take_scoped_previous_files`。
- 这些函数不依赖 async runtime、ProjectService 或 ChangeLogStore，适合拆成 watcher 私有子模块。

推进记录：
- 新增 `chat_app_server_rs/src/services/workspace_realtime_watcher/dirty_scope.rs`。
- 将 dirty scope enum、dirty path collapse、project dirty path 过滤、scope 分类、workspace diff 和 scoped previous files 迁入该模块。
- 主 watcher 文件改为从 `dirty_scope` 引入 `apply_dirty_scope_snapshots`、`classify_dirty_path_scope`、`collect_project_dirty_paths`、`diff_workspace_files` 和 `DirtyPathScopeKind`。
- 测试导入改为从 `dirty_scope` / `path_utils` 引入对应 helper。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `workspace_realtime_watcher.rs` 从 735 行降到 589 行。
- 新增 `dirty_scope.rs` 163 行。
- 当前 watcher 子模块规模：
  - `dirty_scope.rs` 163 行。
  - `path_utils.rs` 63 行。
  - `snapshot.rs` 220 行。
- watcher 主文件进一步聚焦于启动循环、项目扫描状态、suppression、change log 写入和 realtime 通知。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- `workspace_realtime_watcher.rs` 已从最初 1082 行降到 589 行，继续拆收益明显下降。
- 下一步建议回到方案剩余项：Code navigation 多语言 provider 重复抽象，或 db_connection_hub 多数据库 driver 重复抽象。

## 第 43 轮：拆分 chat_app i18n messages 大字典

状态：已完成

目标：
- 处理 `chat_app/src/i18n/messages.ts` 这个当前源码扫描里最大的维护型 TS 文件。
- 将中英文 message dictionary 拆成独立 locale 文件。
- 保持 `I18nProvider` 现有 `import { UI_MESSAGES, type UiLocale } from './messages'` 调用方式不变。
- 不修改任何翻译 key/value 内容，只做机械拆分。
- 本轮继续只做编译/类型检查，不启动项目，不执行测试。

初始观察：
- `messages.ts` 原有 3230 行。
- 文件只有 `UiLocale`、`MessageDictionary` 类型、中英文两个大对象和 `UI_MESSAGES` 聚合导出。
- 这与第 40 轮 task runner 前端 i18n 拆分是同类问题，属于低风险高收益的大文件治理。

推进记录：
- 新增 `chat_app/src/i18n/messages/types.ts`，承接 `UiLocale` 和 `MessageDictionary`。
- 新增 `messages/zhCN.ts`，承接中文 `zhCN` 字典。
- 新增 `messages/enUS.ts`，承接英文 `enUS` 字典。
- 将原 `messages.ts` 改为聚合入口：导入 `zhCN`、`enUS`，re-export 类型，并导出 `UI_MESSAGES`。
- 拆分过程按原对象边界机械迁移，没有改动翻译文案。

结果：
- `messages.ts` 从 3230 行降到 10 行。
- 新增 `messages/types.ts` 3 行。
- 新增 `messages/zhCN.ts` 1612 行。
- 新增 `messages/enUS.ts` 1612 行。
- chat_app i18n 入口保持兼容，后续每种语言可以独立维护。

验证：
- `npm run type-check` 在 `chat_app` 通过。
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 当前最大纯字典热点已处理。
- 下一步建议处理 `crates/chatos_builtin_tools/src/task_manager.rs`、`terminal_controller.rs`、`ui_prompter.rs` 这组三个 builtin MCP 大文件，先拆 schema/parsing/service 边界。

## 第 44 轮：拆分 chatos_builtin_tools task_manager schema/parsing/tests

状态：已完成

目标：
- 处理 `crates/chatos_builtin_tools/src/task_manager.rs` 这个 builtin MCP 大文件。
- 优先拆最稳定、行为风险最低的 tool schema 构建、JSON 参数解析和测试模块。
- 保持 `TaskDraft`、`TaskUpdatePatch`、`TaskManagerService` 等公开类型与 `parse_task_drafts` / `parse_update_patch` 导出方式兼容。
- 本轮只做编译检查，不启动项目，不执行测试。

初始观察：
- `task_manager.rs` 原有 1067 行。
- 文件混合了公开 DTO、store trait、service/tool 注册、add/list/update/complete/delete handler、tool schema 构建、参数解析和测试。
- schema 与 parsing 逻辑不依赖 service 状态，适合先拆成子模块；测试也适合迁出主文件，降低生产模块阅读成本。

推进记录：
- 新增 `crates/chatos_builtin_tools/src/task_manager/schema.rs`。
- 将 `task_payload_schema`、`task_item_schema`、`outcome_item_schema` 迁入 schema 模块。
- 新增 `task_manager/parsing.rs`。
- 将 `parse_task_drafts`、`parse_update_patch`、`required_string_arg`、`trimmed_non_empty` 及内部解析 helper 迁入 parsing 模块。
- 主文件通过 `pub use self::parsing::{parse_task_drafts, parse_update_patch, trimmed_non_empty};` 保持原公开函数路径。
- 新增 `task_manager/tests.rs`，将原内联测试模块迁出。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `task_manager.rs` 从 1067 行降到 495 行。
- 新增 `schema.rs` 74 行。
- 新增 `parsing.rs` 262 行。
- 新增 `tests.rs` 246 行。
- task_manager 主文件现在更聚焦于公开类型、store trait、service 注册和工具 handler 编排。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续同一条 builtin MCP 大文件治理线，优先处理 `terminal_controller.rs` 或 `ui_prompter.rs`。
- `terminal_controller.rs` 可先拆 process tool schema/argument parsing/tests；`ui_prompter.rs` 可先拆 payload normalizer/schema/tests。

## 第 45 轮：拆分 chatos_builtin_tools terminal_controller schema/parsing/tests

状态：已完成

目标：
- 继续 builtin MCP 大文件治理，处理 `crates/chatos_builtin_tools/src/terminal_controller.rs`。
- 先拆最稳定的 tool schema、process 参数解析和测试模块。
- 保持 `TerminalControllerService`、`TerminalControllerStore` 等公开类型不变。
- 保持 `coerce_process_identifier`、`resolve_wait_timeout_ms` 的公开导出路径兼容。
- 本轮只做编译检查，不启动项目，不执行测试。

初始观察：
- `terminal_controller.rs` 原有约 1051 行。
- 文件同时包含公开配置/上下文/store trait、service 注册、handler 分发、路径解析、process 参数解析、tool schema 构建和测试。
- schema 与 parsing 逻辑不依赖 service 状态，测试模块也可以迁出主文件，属于低风险拆分。

推进记录：
- 新增 `crates/chatos_builtin_tools/src/terminal_controller/schema.rs`。
- 将 `execute_command`、`recent_logs`、`process_list/poll/log/wait/write/kill` 和兼容 process tool 的 schema 构建迁入 schema 模块。
- 新增 `terminal_controller/parsing.rs`。
- 将 `resolve_wait_timeout_ms`、`coerce_process_identifier`、`coerce_process_data`、`required_trimmed_string` 迁入 parsing 模块。
- 主文件通过 `pub use self::parsing::{coerce_process_identifier, resolve_wait_timeout_ms};` 保持原公开 helper 路径。
- 新增 `terminal_controller/tests.rs`，将原内联测试模块迁出。
- 清理拆分后主文件残留的 `json` import，测试模块显式引入 `serde_json::json`。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `terminal_controller.rs` 从约 1051 行降到 580 行。
- 新增 `schema.rs` 202 行。
- 新增 `parsing.rs` 54 行。
- 新增 `tests.rs` 271 行。
- terminal controller 主文件现在更聚焦于公开类型、store trait、service 注册、handler 编排和路径 canonicalization。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 继续同一条 builtin MCP 大文件治理线，优先处理 `crates/chatos_builtin_tools/src/ui_prompter.rs`。
- 建议先拆 prompt payload normalizer、schema 构建和测试模块，降低主文件阅读成本。

## 第 46 轮：拆分 chatos_builtin_tools ui_prompter schema/payload/tests

状态：已完成

目标：
- 继续 builtin MCP 大文件治理，处理 `crates/chatos_builtin_tools/src/ui_prompter.rs`。
- 将 tool schema、prompt payload 归一化/构造和测试模块从 service 主流程中拆出。
- 保持 `UiPrompterService`、`UiPrompterStore`、`UiPromptPayload` 等公开类型不变。
- 保持 `ChoiceOption`、`ChoiceLimits`、`KvField` 在 `ui_prompter` 模块下的原公开路径可用。
- 本轮只做编译检查，不启动项目，不执行测试。

初始观察：
- `ui_prompter.rs` 原有 953 行。
- 文件混合了公开 DTO、store trait、service/tool 注册、三类 prompt handler、schema 构建、字段/选项归一化、mixed payload 组装和测试。
- schema 与 payload normalizer 都是纯逻辑，适合先拆成私有子模块；测试也适合迁出主文件。

推进记录：
- 新增 `crates/chatos_builtin_tools/src/ui_prompter/schema.rs`。
- 将 `kv_schema`、`choice_schema`、`mixed_schema` 迁入 schema 模块。
- 新增 `ui_prompter/payload.rs`。
- 将 `ChoiceOption`、`ChoiceLimits`、`KvField` 以及 key/value field、choice option、default selection、mixed payload 等归一化/构造 helper 迁入 payload 模块。
- 主文件通过 `pub use self::payload::{ChoiceLimits, ChoiceOption, KvField};` 保持原公开类型路径。
- 新增 `ui_prompter/tests.rs`，将原内联测试模块迁出。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `ui_prompter.rs` 从 953 行降到 351 行。
- 新增 `schema.rs` 125 行。
- 新增 `payload.rs` 402 行。
- 新增 `tests.rs` 94 行。
- ui prompter 主文件现在更聚焦于公开类型、store trait、service 注册和 prompt handler 编排。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- builtin MCP 三个明显大文件 `task_manager`、`terminal_controller`、`ui_prompter` 已完成第一轮拆分。
- 下一步建议回到方案中的剩余热点：`crates/chatos_ai_runtime/src/task.rs` / `runtime.rs`，或 task runner 前端 `TasksPage.tsx` / `RunsPage.tsx`。

## 第 47 轮：抽象 Code navigation 启发式 provider 公共框架

状态：已完成

目标：
- 开始处理原方案中尚未完成的 Code navigation 多语言 provider 重复问题。
- 先收口 Go / Java / Python / Rust 四个启发式 provider 中重复的 `CodeNavProvider` 样板实现。
- 不改各语言 definition、references、symbol analysis、搜索打分与声明识别细节。
- 本轮只做编译检查，不启动项目，不执行测试。

初始观察：
- `go/mod.rs`、`java/mod.rs`、`python/mod.rs`、`rust/mod.rs` 都有一段高度相似的 provider 实现：
  - provider/language id。
  - `provider-heuristic` mode。
  - 单扩展名文件支持判断。
  - 项目根探测。
  - `heuristic_nav_capabilities`。
  - definition/references 委托。
  - document symbols 包装。
- `shared_nav.rs` 已经有大量候选位置、引用排序、搜索 fallback、document symbols 响应等公共 helper，适合继续承接 provider 框架。

推进记录：
- 在 `chat_app_server_rs/src/services/code_nav/languages/shared_nav.rs` 新增 `HeuristicNavLanguage` trait。
- 为 `HeuristicNavLanguage` 提供 `CodeNavProvider` blanket impl。
- 将公共的 `provider_id`、`language_id`、三类 mode、`supports_file`、`capabilities`、`document_symbols` 包装逻辑集中到 shared 层。
- `GoCodeNavProvider`、`JavaCodeNavProvider`、`PythonCodeNavProvider`、`RustCodeNavProvider` 改为只声明：
  - symbol 类型。
  - provider/language id。
  - 文件扩展名。
  - symbol 数量上限。
  - 项目探测。
  - definition/references 委托。
  - document symbol analysis 函数。
- `RustSymbol` 调整为 `pub(crate)`，满足 shared trait 关联类型可见性。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `shared_nav.rs` 从 403 行增至 496 行，承接启发式 provider 公共框架。
- `go/mod.rs` 从 431 行降到 391 行。
- `python/mod.rs` 从 431 行降到 392 行。
- `java/mod.rs` 从 557 行降到 517 行。
- `rust/mod.rs` 从 863 行降到 823 行。
- 四个语言 provider 的注册与 trait 分发表面保持不变，但重复框架逻辑已集中。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- 可以继续沿 Code navigation 线，把 Go / Java / Python 的 definition fallback 与 indexed candidate 流程再抽成更小的 shared helper。
- 也可以切到另一个原方案未完成项：`db_connection_hub` 多数据库 driver 样板重复。

## 第 48 轮：抽象 Code navigation symbol/search match 字段转发

状态：已完成

目标：
- 继续收敛 Code navigation 多语言 provider 重复。
- 处理 Go / Java / Python / Rust 四个语言模块里完全一致的 `NavSymbolLike` 与 `NavSearchMatchLike` 字段转发实现。
- 不改符号分析、搜索结果、definition/reference 行为。
- 本轮只做编译检查，不启动项目，不执行测试。

初始观察：
- 四个语言模块中的 symbol/search match 结构字段命名一致：
  - symbol：`name`、`kind`、`line`、`column`、`end_line`、`end_column`。
  - search match：`path`、`relative_path`、`line`、`column`、`text`。
- 对应 trait impl 只是重复返回这些字段，适合放到 shared 层统一。

推进记录：
- 在 `shared_nav.rs` 新增 `impl_nav_symbol_like_for_field_struct!` 宏。
- 在 `shared_nav.rs` 新增 `impl_nav_search_match_like_for_field_struct!` 宏。
- 将 Go / Java / Python / Rust 四个模块的手写 `NavSymbolLike` impl 替换为宏调用。
- 将 Go / Java / Python / Rust 四个模块的手写 `NavSearchMatchLike` impl 替换为宏调用。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- `shared_nav.rs` 从 496 行增至 555 行，集中字段转发宏。
- `go/mod.rs` 从 391 行降到 346 行。
- `python/mod.rs` 从 392 行降到 347 行。
- `java/mod.rs` 从 517 行降到 472 行。
- `rust/mod.rs` 从 823 行降到 778 行。
- 多语言模块不再重复维护相同字段转发逻辑，后续新增同字段形状语言 provider 可以直接复用。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。

下一步建议：
- Code navigation 线还可以继续抽 `definition` 中的 indexed candidate / search fallback 编排，但那会比本轮更靠近行为逻辑，需要更谨慎。
- 如果继续做高价值低风险治理，建议转向 `db_connection_hub` 多数据库 driver 样板，或当前剩余大文件 `agent_chat.rs` / `project_run/analyzer.rs`。

## 第 49 轮：修复 Code navigation 定义/引用结果包含自身与 Java 调用误判

状态：已完成

目标：
- 修复“跳到定义 / 查找引用”结果总是包含当前光标 token 的问题。
- 修复 Java provider 在定义 fallback 中把普通方法调用误判成方法声明的问题。
- 保持现有 provider API 与前端调用方式不变。
- 本轮只做编译检查，不启动项目，不执行测试。

问题定位：
- `select_reference_locations` 会把搜索命中的所有 token 都放入结果，再做声明/引用分组，没有排除请求位置本身。
- `push_definition_search_matches` 在 provider 的 grep fallback 阶段也没有排除请求位置本身，导致光标在定义处时可能把定义自身列回来。
- `fallback_references` / `fallback_definition` 也没有自身过滤；当 provider 返回空时，manager 会走 fallback，于是自身又被文本搜索带回来。
- `fallback_definition` 原先用宽泛的 `token(` 模式加分，普通调用也容易混入定义候选。
- Java `extract_method_signature` 对 `response.setCode(...)` 这类点号调用判断不严，`setCode` 可能被当作方法声明。

推进记录：
- 在 `shared_nav.rs` 新增 `is_request_token_location`，按当前文件、行号和 token column span 判断是否为当前光标 token。
- `select_reference_locations` 新增 `req` 参数，并在引用结果入池前过滤当前 token。
- `push_definition_search_matches` 新增 `ctx/req` 参数，并在定义 fallback 候选入池前过滤当前 token。
- Go / Java / Python / Rust / basic provider 全部接入新的公共过滤参数。
- `fallback_references` 增加当前 token 过滤。
- `fallback_definition` 增加当前 token 过滤，并只保留分数达到定义候选阈值的结果。
- 移除 `fallback_definition` 中过宽的 `token(` 加分，改为更保守的 `looks_like_callable_definition` 判断。
- Java `extract_method_signature` 在方法名前缀以 `.` 结尾时拒绝识别，避免把 `object.method(...)` 当成声明。
- 将 `languages/shared_nav` 从私有模块调整为 `pub(crate)`，供 code-nav fallback 复用内部 helper。
- 使用 `rustfmt --edition 2021` 格式化相关 Rust 文件。

结果：
- 查找引用时，当前光标所在 token 不再出现在结果中。
- 跳到定义时，当前 token 不会因为 fallback 搜索被列回自身。
- Java 普通方法调用不再被声明识别逻辑误判为方法定义候选。
- fallback 定义结果从“所有出现位置排序”收紧为“像定义的候选”，降低把调用/引用混入定义结果的概率。

验证：
- 根目录 `cargo check` 通过。
- `git diff --check` 通过。
- 未启动项目，未执行测试，符合本轮约束。
