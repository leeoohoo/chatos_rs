# 项目优化进度

日期：2026-05-20

## 当前目标

按 `PROJECT_ASSESSMENT_AND_IMPLEMENTATION_PLAN_2026-05-20.md` 的第 0 阶段直接推进，不保留过渡态，不增加额外兼容层。

## 范围

1. 按 `memory_engine_sdk` 最新签名修复主后端调用点
2. 修复前端 `chat_app` 类型检查失败
3. 更新热点预算脚本，使其和当前仓库真实文件一致
4. 跑关键检查并继续收口

## 当前状态

- [completed] 创建进度文档并开始维护
- [completed] 修复 `chat_app_server_rs` 的 SDK 调用错误
- [completed] 修复 `chat_app` 的 type-check 错误
- [completed] 修复 `scripts/check-hotspot-line-budgets.sh`
- [completed] 优化数据库连接弹窗自动探测逻辑
- [completed] 优化创建连接表单自动探测逻辑
- [completed] 补充消息归一化回归测试
- [completed] 运行 `cargo test` / `npm run type-check` / 热点预算检查
- [completed] 拆分 `project_run/environment.rs` 的纯辅助能力到独立模块
- [completed] 拆分 `project_run/environment.rs` 的 hint/config/discovery 逻辑
- [completed] 拆分 `project_run/environment.rs` 的运行时命令解析与环境变量逻辑
- [completed] 拆分 `project_run/environment.rs` 的运行前校验逻辑
- [completed] 修复 `turn_runtime_snapshot` 测试构造参数漂移
- [completed] 清理 Rust 侧本轮改动引出的无效导入告警
- [completed] 优化 `workspace_realtime_watcher` 的高频 dirty-path 扫描路径
- [completed] 拆分 `useProjectRunnerCatalogState` 的环境派生逻辑并补充前端测试
- [completed] 拆分 `useChatStreamRealtimeBridge` 的实时派生与恢复判定逻辑
- [completed] 继续拆分 `useChatStreamRealtimeBridge` 的终态收口与消息恢复逻辑
- [completed] 修复 `useChatStreamRealtimeBridge` 断线恢复导入缺失与 realtime stream 拆分类型断裂
- [completed] 重新验证 `chat_app` type-check、聊天流相关测试与热点预算脚本
- [completed] 拆分 `useTerminalInstanceLifecycle` 的历史加载与 snapshot 分页纯逻辑
- [completed] 拆分 `useTerminalInstanceLifecycle` 的初始化与清理状态编排
- [completed] 拆分 `useTerminalInstanceLifecycle` 的输入解析与 websocket 发送编排
- [completed] 拆分 `useTerminalInstanceLifecycle` 的 viewport/handler/history-effect 收口并补足测试
- [completed] 继续拆分 `useProjectRunnerCatalogState` 的请求判定、状态派生与环境更新动作
- [completed] 将 `useProjectRunnerCatalogState` 的环境 mutation 编排独立为专用 hook
- [completed] 拆分 `useTerminalSocketLifecycle` 的 websocket 消息状态收口并补充测试
- [completed] 修复 `project runner` 首次进入未主动加载运行环境的问题
- [completed] 修复 `project runner` 在项目切换时旧请求结果回写新项目 UI 的竞态
- [completed] 拆分 `projectRunnerFailureDiagnostics` 为失败原因、验证文案和建议生成三个纯 helper
- [completed] 将 `useProjectRunnerCatalogState` 的请求编排、项目切换失效和 realtime reset 抽到独立 lifecycle hook
- [completed] 将 `useProjectRunnerTerminalPolling` 的 active run 构建、实例选择和终端状态归并抽到独立 helper
- [completed] 将 `useProjectRunnerCommands` 的终端选择、派发状态构造、删除后目标选择与验证错误提取抽到独立 helper
- [completed] 将 `useProjectRunnerExitInspection` 的退出检查门禁抽到独立 helper
- [completed] 将 `useProjectRunnerCatalogLifecycle` 的请求版本门禁抽到独立 helper
- [completed] 将 `useTerminalInstanceLifecycle` 的历史加载编排抽到独立 helper

## 已确认问题

### 1. 主后端编译失败

- 文件：`chat_app_server_rs/src/services/chatos_memory_engine/sessions.rs`
- 问题：`get_record` / `delete_record` 仍按旧签名调用

### 2. 前端类型检查失败

- 文件：`chat_app/src/lib/domain/messages.ts`
- 问题：`metadata` 仍为 `unknown`，但后续按结构化对象读取

### 3. 热点预算脚本失效

- 文件：`scripts/check-hotspot-line-budgets.sh`
- 问题：存在失效路径，且部分预算与当前文件规模不一致

## 更新日志

### 2026-05-20

- 创建进度文档
- 明确本轮只做直接修复，不引入 SDK 过渡适配层
- 按最新 `memory_engine_sdk` 签名修复 `sessions.rs` 中的 `get_record` / `delete_record` 调用
- 收紧 `chat_app/src/lib/domain/messages.ts` 内部 `metadata` 类型
- 统一 `messages.ts` 中 `metadata` 的空值语义为 `null`，继续收口 type-check
- 更新热点预算脚本中的失效路径和实际预算值
- 验证通过：
  - `cd chat_app_server_rs && cargo check`
  - `cd chat_app && npm run type-check`
  - `bash scripts/check-hotspot-line-budgets.sh`
- 验证通过：
  - `cd chat_app_server_rs && cargo check`（二次确认）
  - `cd chat_app && npm run type-check`（二次确认）
  - `bash scripts/check-hotspot-line-budgets.sh`（二次确认）
- 进行中：
  - `db_connection_hub/frontend/src/components/workbench/ConnectionModal.tsx` 防抖与请求序号保护
  - `db_connection_hub/frontend/src/components/connections/CreateConnectionForm.tsx` 防抖与请求序号保护
  - `chat_app/src/lib/domain/messages.test.ts` 回归测试补充
- 验证通过：
  - `cd chat_app && npm run test -- --run src/lib/domain/messages.test.ts`
  - `cd db_connection_hub/frontend && npm run type-check`
  - `cd chat_app && npm run type-check`
- 结构优化：
  - 新增 `chat_app_server_rs/src/services/project_run/environment_support.rs`
  - 将路径归一化、工具链需求推断、PATH 处理等纯辅助逻辑从 `environment.rs` 拆出
  - `cargo check` 已通过
- 结构优化：
  - 新增 `chat_app_server_rs/src/services/project_run/environment_discovery.rs`
  - 将 toolchain hint 解析、配置文件扫描、工具链发现逻辑从 `environment.rs` 拆出
  - `cargo check` 已通过
- 结构优化：
  - 新增 `chat_app_server_rs/src/services/project_run/environment_runtime.rs`
  - 新增 `chat_app_server_rs/src/services/project_run/environment_validation.rs`
  - 将命令重写、环境变量覆写、运行前校验从 `environment.rs` 继续拆出
  - `chat_app_server_rs/src/services/project_run/environment.rs` 从 739 行降到 195 行
  - `cargo check` 已通过
- 测试与稳定性修复：
  - 修复 `chat_app_server_rs/src/core/turn_runtime_snapshot.rs` 中测试构造器字段遗漏
  - 清理 `chat_app_server_rs/src/repositories/change_logs.rs` 与 `chat_app_server_rs/src/builtin/code_maintainer/storage.rs` 的无效导入
  - 验证通过：
    - `cd chat_app_server_rs && cargo check`
    - `cd chat_app_server_rs && cargo test turn_runtime_snapshot -- --nocapture`
    - `cd chat_app_server_rs && cargo test environment_runtime -- --nocapture`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 性能优化：
  - 调整 `chat_app_server_rs/src/services/workspace_realtime_watcher.rs`
  - 将“dirty path 命中就整项目 WalkDir 全量快照”的路径改为“dirty scope 增量扫描 + 周期性全量校准”
  - 为 dirty scope 合并、增量旧快照摘除、路径边界判断补充单元测试
  - 验证通过：
    - `cd chat_app_server_rs && cargo check`
    - `cd chat_app_server_rs && cargo test workspace_realtime_watcher -- --nocapture`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerEnvironmentState.ts`
  - 将 `useProjectRunnerCatalogState.ts` 中的命令预览、环境变量草稿、toolchain 派生、提示文本等纯计算逻辑抽离为独立 helper
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts`
  - `useProjectRunnerCatalogState.ts` 从 740 行降到 521 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/chatInterface/chatStreamRealtimeBridgeState.ts`
  - 将 `useChatStreamRealtimeBridge.ts` 中的 active stream 识别、payload turn 提取、completion key 构造、恢复触发判定抽离为独立 helper
  - 新增 `chat_app/src/components/chatInterface/chatStreamRealtimeBridgeState.test.ts`
  - `useChatStreamRealtimeBridge.ts` 从 456 行降到 408 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/chatInterface/chatStreamRealtimeBridgeState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/chatInterface/chatStreamRealtimeTerminalState.ts`
  - 将 `useChatStreamRealtimeBridge.ts` 中的 terminal success/failure 收口、reload 判定、snapshot 恢复回退统一抽离为独立 helper
  - 补充 `chat_app/src/components/chatInterface/chatStreamRealtimeTerminalState.test.ts`
  - `useChatStreamRealtimeBridge.ts` 从 408 行继续降到 280 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/chatInterface/chatStreamRealtimeBridgeState.test.ts`
    - `cd chat_app && npm run test -- --run src/components/chatInterface/chatStreamRealtimeTerminalState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/terminal/terminalHistoryState.ts`
  - 将 `useTerminalInstanceLifecycle.ts` 中的历史日志分页、去重合并、snapshot 向上滚动分页判定抽离为独立 helper
  - 新增 `chat_app/src/components/terminal/terminalHistoryState.test.ts`
  - `useTerminalInstanceLifecycle.ts` 从 451 行降到 433 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalHistoryState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/terminal/terminalInstanceState.ts`
  - 将 `useTerminalInstanceLifecycle.ts` 中的初始化状态重置、ref 清理、资源回收编排抽离为独立 helper
  - 新增 `chat_app/src/components/terminal/terminalInstanceState.test.ts`
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalInstanceState.test.ts`
- 前端结构优化：
  - 新增 `chat_app/src/components/terminal/terminalInputState.ts`
  - 将 `useTerminalInstanceLifecycle.ts` 中的输入 chunk 解析、命令纠正、websocket 发送计划抽离为独立 helper
  - 新增 `chat_app/src/components/terminal/terminalInputState.test.ts`
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalInputState.test.ts`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalHistoryState.test.ts`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalInstanceState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/terminal/terminalViewportState.ts`
  - 新增 `chat_app/src/components/terminal/terminalLifecycleHandlers.ts`
  - 新增 `chat_app/src/components/terminal/terminalHistoryEffects.ts`
  - 补充 `chat_app/src/components/terminal/terminalViewportState.test.ts`
  - 补充 `chat_app/src/components/terminal/terminalLifecycleHandlers.test.ts`
  - 补充 `chat_app/src/components/terminal/terminalHistoryEffects.test.ts`
  - 将 `useTerminalInstanceLifecycle.ts` 中的 snapshot 发送计划、resize 发送计划、terminal 事件 handler、历史加载 begin/success/error/finalize 编排进一步抽离
  - `useTerminalInstanceLifecycle.ts` 从 433 行继续降到 372 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalLifecycleHandlers.test.ts src/components/terminal/terminalHistoryEffects.test.ts src/components/terminal/terminalViewportState.test.ts src/components/terminal/terminalInputState.test.ts src/components/terminal/terminalInstanceState.test.ts src/components/terminal/terminalHistoryState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerCatalogState.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerEnvironmentActions.ts`
  - 补充 `chat_app/src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts`
  - 补充 `chat_app/src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
  - 将 `useProjectRunnerCatalogState.ts` 中的请求版本判定、默认 target 选择、runStatus 派生、realtime catalog 事件分发、运行环境更新 payload 与乐观状态收口继续抽离
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/projectExplorer/runState/useProjectRunnerEnvironmentMutations.ts`
  - 将 `useProjectRunnerCatalogState.ts` 中的 `persistEnvironment`、toolchain 选择更新、自定义 toolchain 保存、环境变量保存编排整体下沉到独立 mutation hook
  - 将 `useProjectRunnerCatalogState.ts` 从 521 行继续降到 408 行
  - `projectRunnerEnvironmentActions.ts` 继续收口 direct payload builder 与乐观状态变更逻辑
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/terminal/terminalSocketState.ts`
  - 补充 `chat_app/src/components/terminal/terminalSocketState.test.ts`
  - 将 `useTerminalSocketLifecycle.ts` 中的 open/snapshot/output/exit/state/error 消息收口与 snapshot/reset 状态变更抽离到独立 helper
  - 将 `useTerminalSocketLifecycle.ts` 从 308 行降到 236 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/terminal/terminalSocketState.test.ts src/components/terminal/terminalLifecycleHandlers.test.ts src/components/terminal/terminalHistoryEffects.test.ts src/components/terminal/terminalViewportState.test.ts src/components/terminal/terminalInputState.test.ts src/components/terminal/terminalInstanceState.test.ts src/components/terminal/terminalHistoryState.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 缺陷修复与结构收口：
  - 调整 `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts`
  - 将“项目切换 / disabled / realtime reset”统一走 `invalidateRunnerCatalogState`
  - 修复首次进入项目时只加载 catalog、未主动加载 run environment 的问题
  - 修复旧项目 `catalog/environment` 请求结果在项目切换后回写当前 UI 的竞态
  - 新增 `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.test.tsx`
  - 验证“首次进入即同时加载 catalog + environment”与“旧请求不会覆盖新项目状态”
- 缺陷修复与结构收口：
  - 调整 `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.ts`
  - 将“项目切换 / disabled”统一走 `resetActiveRunState`
  - 修复旧项目 `run state` 请求在项目切换后覆盖当前项目终端状态的竞态
  - 删除 `chat_app/src/components/projectExplorer/useProjectExplorerRunState.ts` 中重复的初始 catalog 加载 effect，初始化责任下沉回 catalog hook
  - 新增 `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.test.tsx`
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/useProjectRunnerCatalogState.test.tsx src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.test.tsx src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化与缺陷修复：
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerFailureReason.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerValidationIssues.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerResolutionSuggestions.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerFailureReason.test.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerValidationIssues.test.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerResolutionSuggestions.test.ts`
  - 将 `projectRunnerFailureDiagnostics.ts` 的日志原因提取、启动前验证文案、诊断建议生成拆成三个纯 helper
  - 将 `useProjectRunnerCommands.ts` 与 `useProjectRunnerExitInspection.ts` 的诊断引用切换到新 helper
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/projectRunnerFailureReason.test.ts src/components/projectExplorer/runState/projectRunnerValidationIssues.test.ts src/components/projectExplorer/runState/projectRunnerResolutionSuggestions.test.ts src/components/projectExplorer/runState/useProjectRunnerCatalogState.test.tsx src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.test.tsx src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogLifecycle.ts`
  - 将 `useProjectRunnerCatalogState.ts` 中的 catalog/environment 请求编排、项目切换失效与 realtime reset 抽离为独立 lifecycle hook
  - `useProjectRunnerCatalogState.ts` 从 426 行降到 207 行
  - `useProjectRunnerCatalogLifecycle.ts` 为 307 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/projectRunnerFailureReason.test.ts src/components/projectExplorer/runState/projectRunnerValidationIssues.test.ts src/components/projectExplorer/runState/projectRunnerResolutionSuggestions.test.ts src/components/projectExplorer/runState/useProjectRunnerCatalogState.test.tsx src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.test.tsx src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`
- 前端结构优化：
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerTerminalState.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerTerminalState.test.ts`
  - 将 `useProjectRunnerTerminalPolling.ts` 中的 active run 构建、selected instance 选择、实例删除归并与 terminal state payload 归并抽离为纯 helper
  - `useProjectRunnerTerminalPolling.ts` 从 271 行降到 202 行
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerCommandState.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerCommandState.test.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerCommandErrors.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerCommandErrors.test.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerExitInspectionState.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerExitInspectionState.test.ts`
  - 将 `useProjectRunnerCommands.ts` 中的终端选择、派发状态构造、删除后目标选择、验证错误提取继续抽离
  - 将 `useProjectRunnerExitInspection.ts` 的 manual-control 门禁抽离为独立 helper
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerRequestGuard.ts`
  - 新增 `chat_app/src/components/projectExplorer/runState/projectRunnerRequestGuard.test.ts`
  - 将 `useProjectRunnerCatalogLifecycle.ts` 中 catalog/environment 请求的版本门禁与 apply 判定抽成共享 guard
  - `useProjectRunnerCatalogLifecycle.ts` 从 307 行降到 285 行
  - 新增 `chat_app/src/components/terminal/terminalHistoryLoader.ts`
  - 新增 `chat_app/src/components/terminal/terminalHistoryLoader.test.ts`
  - 将 `useTerminalInstanceLifecycle.ts` 中的历史加载编排抽到独立 helper
  - `useTerminalInstanceLifecycle.ts` 从 371 行降到 317 行
  - 验证通过：
    - `cd chat_app && npm run type-check`
    - `cd chat_app && npm run test -- --run src/components/projectExplorer/runState/projectRunnerTerminalState.test.ts src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.test.tsx src/components/projectExplorer/runState/useProjectRunnerCatalogState.test.tsx src/components/projectExplorer/runState/projectRunnerFailureReason.test.ts src/components/projectExplorer/runState/projectRunnerValidationIssues.test.ts src/components/projectExplorer/runState/projectRunnerResolutionSuggestions.test.ts src/components/projectExplorer/runState/projectRunnerCatalogState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentState.test.ts src/components/projectExplorer/runState/projectRunnerEnvironmentActions.test.ts`
    - `bash scripts/check-hotspot-line-budgets.sh`

## 当前观察

- `chat_app/src/components/terminal/useTerminalInstanceLifecycle.ts` 已降到 372 行并完成终端生命周期拆分收口
- `chat_app/src/components/terminal/useTerminalSocketLifecycle.ts` 已降到 236 行，socket 消息状态与副作用边界已拆开
- `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts` 当前为 207 行，catalog/environment 派生与 mutation 组合更清晰
- `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogLifecycle.ts` 当前为 307 行，承接了请求编排、项目切换失效与 realtime reset
- `chat_app/src/components/projectExplorer/runState/useProjectRunnerTerminalPolling.ts` 当前为 202 行，项目切换失效路径与实例状态归并已清晰下沉
- `chat_app/src/components/projectExplorer/runState/projectRunnerFailureDiagnostics.ts` 已拆成三个纯 helper，run-state 的诊断层次更清晰

## 下一步

当前这轮阻塞项已经清掉，后续还可以继续做的事情是：

1. 继续收口新的热点文件与高频运行路径
2. 继续拆 `project runner` 剩余的 reset/realtime/load 编排与 terminal state 归并逻辑
3. 把 `db_connection_hub/frontend` 的测试覆盖补到和主前端更接近的水平
4. 如果要推进到“事件驱动优先”的更高性能目标，再继续收紧 `workspace_realtime_watcher` 的全量扫描兜底路径
