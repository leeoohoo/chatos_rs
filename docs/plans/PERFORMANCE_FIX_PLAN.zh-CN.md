# Chatos / Task Runner 模型调用性能修复计划

## 目标

降低 Chatos 与 Task Runner 在“模型调用”场景里的体感延迟，优先减少模型请求前、工具调用期间、模型请求之间的固定开销，让日志能清楚区分 provider 模型耗时、MCP 工具耗时、运行时编排耗时。

## 本轮明确不动

- Memory Engine 的上下文 compose、记录写入、压缩链路暂不改动。
- 不调整 Memory Engine 与 Chatos / Task Runner 的交互语义。
- 只在必要时补充 Memory Engine 外围耗时埋点，不改变同步/异步策略。

## 修复优先级

### P0: 降低模型请求路径上的本地 CPU / 日志开销

- 停止在正常 info 日志里完整输出 AI request payload。
- 停止在正常 info 日志里完整输出 AI response payload。
- 日志改为记录 transport、url、payload_bytes、response_id、finish_reason、tool_call_count、usage 等摘要字段。
- 避免同一请求为了 size 统计、limit 校验、日志输出而多次完整 JSON 序列化。

预期收益：

- 大上下文、大附件、大工具结果场景下，减少首包前 CPU 和日志 I/O 压力。
- 降低日志系统被大 payload 拖慢的概率。

### P1: MCP HTTP 与 TaskRunner HTTP client 复用

- 共享 MCP JSON-RPC HTTP client，避免每个 `tools/list` / `tools/call` 都创建新 client。
- Chatos 调 TaskRunner API 的 client 复用。
- TaskRunner 回调 Chatos 的 client 复用。
- 保留现有 timeout、redirect 策略。

预期收益：

- 复用连接池、DNS、TLS/HTTP keep-alive。
- 降低工具密集型任务的固定网络开销。

### P1: 移除 Chatos MCP 重复初始化

- `SharedMcpToolExecute::build_tools` 当前先构建旧 core 工具，再初始化 shared executor。
- 实际执行优先使用 shared executor，因此避免在正常 shared 路径里重复 discovery。
- 保留测试辅助接口需要的旧 core fallback 能力。

预期收益：

- Chatos 每轮对话启用 MCP 时，减少一次完整的 HTTP / stdio `tools/list`。

### P2: MCP 工具发现并行化与缓存

- 多个 HTTP / stdio MCP server 的 `tools/list` 并行执行。
- 对外部 MCP server config 生成稳定 cache key。
- 在会话级或短 TTL 内缓存 `tools/list` 结果。
- server 配置变化时失效缓存。
- 第一阶段继续推进项：已加入共享 runtime 短 TTL 缓存，成功结果缓存 60 秒，失败结果缓存 10 秒。

预期收益：

- 降低首轮模型请求前的等待。
- 避免每次对话/任务都重复发现同一批工具。

### P2: stdio MCP 会话复用

- 评估将 stdio MCP 从“每次 call spawn 进程”改为“每个 server 维护长生命周期进程”。
- 增加健康检查、退出重启、超时清理。
- 第二阶段继续推进项：已加入共享 runtime stdio session 池，同一 server 配置复用同一个子进程；请求失败、进程退出或超时会移除 session，下次自动重建。

预期收益：

- 大幅降低 stdio MCP 工具调用和工具发现的进程启动开销。

### P3: 性能分段埋点

新增或补齐以下耗时字段：

- `mcp_init_ms`
- `memory_compose_ms`
- `model_request_ms`
- `model_time_to_first_byte_ms`
- `tool_batch_ms`
- `persist_ms`
- `model_request_count`
- `runtime_iteration`

第一阶段继续推进项：

- 已补充 provider 请求总耗时、runtime 每轮模型请求耗时、工具 batch 耗时、MCP init 耗时、Chatos MCP prepare 耗时、TaskRunner runtime init 耗时。

预期收益：

- 能直接判断一次慢调用到底慢在 provider、MCP 初始化、工具执行、持久化还是上下文构建。

## 第一阶段落地范围

第一阶段只做低风险、局部修复：

- AI request / response 日志摘要化。
- MCP HTTP client 复用。
- TaskRunner API / callback client 复用。
- Chatos shared MCP 路径去重初始化。

不包含：

- Memory Engine 行为变更。
- MCP discovery cache。
- stdio MCP 长连接重构。

## 验证方式

- `cargo fmt`
- 针对改动 crate 跑相关 `cargo test`。
- 手动检查日志中不再输出完整模型 request / response payload。
- 对启用 MCP 的 Chatos 对话确认只做一次 shared MCP discovery。
- 对 TaskRunner 工具调用确认功能行为不变。

## 风险与回滚

- 日志摘要化可能影响调试时查看完整 payload。需要完整 payload 时应走调试快照或显式 debug 开关。
- client 复用需要确认 timeout / redirect 策略不丢失。
- 去重 MCP 初始化需要确认旧 core fallback 测试仍可通过。
