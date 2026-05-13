# Memory Engine 主动上下文压缩方案

## 1. 目标

给 `memory_engine` 增加一个线程级“主动压缩上下文”接口，并在 SDK 与 `chatos` 侧补齐调用链，解决下游系统在请求 AI 时收到“上下文窗口超限”错误后，能够主动触发一次总结，把 `pending records` 压进 summary，再重新取上下文并重试。

这次方案的目标是：

- 下游系统有一个语义明确的 API，不需要直接理解 `summary` / `repair summary` 的内部差异。
- `memory_engine sdk` 暴露对应方法。
- `chatos` 在命中 context overflow 时，优先尝试触发一次远端压缩，再走现有本地降级逻辑。
- 明确处理超时，避免把正常读接口的超时策略和“主动压缩”混在一起。

非目标：

- 不改现有 `compose_context` 的 block 结构。
- 不把 `repair summary` 混进普通上下文拼装。
- 不在第一版里做复杂的多轮压缩编排。

## 2. 现状判断

### 2.1 `compose_context` 目前怎么拼上下文

`memory_engine` 当前的上下文来自两部分：

- 已生成的 thread summary
- 仍然处于 `summary_status = pending` 的 recent records

也就是说，真正导致上下文越滚越大的，不是 summary 本身，而是“还没被吃进 summary 的 pending records”。

相关位置：

- `docs/memory_engine/backend/src/services/context/mod.rs`
- `docs/memory_engine/backend/src/services/context/blocks.rs`

### 2.2 现有哪个 summary 能真正帮我们压缩上下文

能直接帮助 `compose_context` 变小的，是 `thread_incremental` 这一类 summary，也就是当前 `run_thread_summary(...)` 走的链路。

`run_thread_repair_summary(...)` 生成的是 `thread_repair`，它目前并不会被 `compose_context` 读进来，所以它适合 review/修复场景，不适合“上下文超限后的主动压缩”。

相关位置：

- `docs/memory_engine/backend/src/services/summary/thread_summary/execution.rs`
- `docs/memory_engine/backend/src/services/summary/thread_repair/summary/runner.rs`

### 2.3 `chatos` 当前怎么处理 overflow

`chatos` 现在已经有本地降级逻辑，但还没有“主动调用 memory_engine 做远端压缩”的环节：

- v2 链路：命中 token/context overflow 后，本地截断消息
  - `chat_app_server_rs/src/services/v2/ai_client/mod.rs`
  - `chat_app_server_rs/src/services/v2/ai_client/token_compaction.rs`
- v3 链路：命中 context overflow / payload too large 后，缩 `history_limit`，或裁剪 `function_call_output`
  - `chat_app_server_rs/src/services/v3/ai_client/recovery_policy/request_error.rs`
  - `chat_app_server_rs/src/services/v3/ai_client/recovery_policy/completion_error.rs`

所以这次改造的关键不是“再造一套 summary”，而是把“overflow -> 触发 memory_engine 压缩 -> 重新 compose context -> 重试”这条链补出来。

## 3. 为什么不直接复用现有 `run_thread_summary`

虽然底层执行逻辑可以复用，但我不建议让下游直接拿现有 `run_thread_summary` 当“主动压缩 API”：

- 它的语义偏内部实现，接口名不够直观。
- 响应不包含 `pending_before_count` / `pending_after_count`，下游很难判断这次压缩是否真的生效。
- 没有显式的“正在运行”语义。
- 现在普通 summary 接口没有为“超时重试后的重复触发”做线程级幂等保护。

所以更合适的做法是：

- 保留现有 `run_thread_summary(...)` 作为底层能力。
- 新增一个更靠近业务语义的包装接口：`context compaction`。

## 4. 推荐接口设计

### 4.1 路由

核心接口：

- `POST /api/memory-engine/v1/threads/:thread_id/context-compaction/run`

SDK 接口：

- `POST /api/memory-engine/v1/sdk/threads/:thread_id/context-compaction/run`

命名理由：

- 跟现有 `summaries/run`、`repair-summaries/run` 的线程级 action 风格一致。
- 对下游来说，“我要压缩上下文”比“我要跑一遍 summary”更容易理解。

### 4.2 请求

核心接口请求：

```json
{
  "tenant_id": "xxx",
  "source_id": "xxx",
  "reason": "context_overflow"
}
```

SDK 请求：

```json
{
  "tenant_id": "xxx",
  "reason": "context_overflow"
}
```

第一版建议只保留一个可选 `reason`：

- 默认值：`context_overflow`
- 主要用于日志和 job metadata 观测

第一版不建议先加：

- `mode`
- `max_passes`
- `wait_for_completion_ms`

先把语义、超时和 `chatos` 自动重试打通，复杂参数可以第二阶段再加。

### 4.3 响应

建议新增响应：

```json
{
  "thread_id": "thread_xxx",
  "accepted": true,
  "running": false,
  "completed": true,
  "compacted": true,
  "generated": true,
  "job_run_id": "job_xxx",
  "summary_id": "summary_xxx",
  "pending_before_count": 128,
  "pending_after_count": 0,
  "source_record_count": 128
}
```

字段语义：

- `accepted`: 本次请求是否被受理
- `running`: 当前线程是否已有同类压缩任务在跑
- `completed`: 本次请求是否已经拿到最终结果
- `compacted`: 是否确实让 `pending_after_count < pending_before_count`
- `generated`: 是否生成了新的 summary
- `job_run_id`: 用于排查和观测
- `summary_id`: 新生成的 summary id
- `pending_before_count`: 压缩前 pending record 数
- `pending_after_count`: 压缩后 pending record 数
- `source_record_count`: 本次 summary 吃掉了多少条 source record

返回约定建议：

- 无 pending records：
  - `accepted=true`
  - `running=false`
  - `completed=true`
  - `compacted=false`
  - `generated=false`
- 已有同线程 compaction / summary 在跑：
  - `accepted=true`
  - `running=true`
  - `completed=false`
  - `compacted=false`
- 正常完成且吃掉 pending：
  - `running=false`
  - `completed=true`
  - `compacted=true`

## 5. 后端实现方案

## 5.1 设计原则

第一版尽量复用现有 `run_thread_summary(...)` 的真正总结逻辑，不改 `compose_context`，也不引入 `repair summary`。

新增一个包装服务：

- `run_thread_context_compaction(...)`

它的职责是：

1. 统计压缩前 pending 数
2. 检查该线程是否已有正在执行的 summary job
3. 没有在跑时，调用现有 `run_thread_summary(...)`
4. 统计压缩后 pending 数
5. 拼出更适合下游使用的响应

## 5.2 推荐改动点

### 路由与 handler

- `docs/memory_engine/backend/src/api/router/core.rs`
- `docs/memory_engine/backend/src/api/router/sdk.rs`
- `docs/memory_engine/backend/src/api/summaries_api.rs`
- `docs/memory_engine/backend/src/api/sdk_api/summaries.rs`
- `docs/memory_engine/backend/src/api/sdk_api/requests/summaries.rs`

### model

- `docs/memory_engine/backend/src/models/summaries.rs`
- `docs/memory_engine/sdk/src/models/summaries.rs`
- `docs/memory_engine/sdk/src/lib.rs`

### service

建议新增：

- `docs/memory_engine/backend/src/services/summary/context_compaction/mod.rs`
- `docs/memory_engine/backend/src/services/summary/context_compaction/execution.rs`

并在：

- `docs/memory_engine/backend/src/services/summary/mod.rs`

中导出。

## 5.3 服务流程

推荐流程如下：

1. `count_records(... summary_status = pending)` 取 `pending_before_count`
2. 用 `thread_id + tenant_id + source_id + job_type=summary + status=running` 查 running job
3. 如果已有 running job：
   - 直接返回 `running=true`
   - `job_run_id` 复用已有 job
4. 如果没有 running job：
   - 调用现有 `summary::run_thread_summary(...)`
5. 再次统计 `pending_after_count`
6. 返回 compaction 响应

这里我建议第一版直接复用现有 `summary` job，而不是先新造一个 `context_compaction` job type，原因是：

- 改动面更小
- 可直接复用现有 summary job 观测
- 不需要把真正的总结逻辑拆第二遍

但要补一个小修正：

- 新包装接口在调用前要先查 running `summary` job，避免下游超时后重复触发

## 5.4 `job_run_id` 的处理

建议 compaction 响应里保留 `job_run_id`，方便排查。

实现上有两种方式：

- 方案 A：把 `run_thread_summary(...)` 的响应扩展出 `job_run_id`
- 方案 B：wrapper 在调用前后查询该线程最新 `summary` job

我更推荐方案 A，改造更干净，也方便后续别的调用方复用。

如果你希望尽量少改现有响应，也可以先落方案 B。

## 5.5 为什么第一版不走 `repair summary`

因为 `compose_context` 当前只读：

- `thread_incremental` level0/top-level summary
- subject memory
- pending recent records

它不读 `thread_repair`。因此就算触发 repair summary，普通聊天上下文也不会变小。

所以这次“主动压缩上下文”的第一版必须走普通 `thread summary` 链路。

## 6. SDK 方案

## 6.1 SDK 暴露

建议在 `memory_engine sdk` 新增：

- `SdkRunThreadContextCompactionRequest`
- `RunThreadContextCompactionResponse`
- `MemoryEngineClient::run_thread_context_compaction(...)`

推荐改动点：

- `docs/memory_engine/sdk/src/models/summaries.rs`
- `docs/memory_engine/sdk/src/client/summaries/triggers.rs`
- `docs/memory_engine/sdk/src/client/summaries/mod.rs`
- `docs/memory_engine/sdk/src/lib.rs`

## 6.2 SDK 方法形态

建议方法签名：

```rust
pub async fn run_thread_context_compaction(
    &self,
    thread_id: &str,
    tenant_id: &str,
    reason: Option<&str>,
) -> Result<RunThreadContextCompactionResponse, String>
```

鉴权分流与现有 summary trigger 一致：

- `AuthMode::Direct` -> `/threads/:thread_id/context-compaction/run`
- `AuthMode::SystemKey` -> `/sdk/threads/:thread_id/context-compaction/run`

## 7. `chatos` 侧改造方案

## 7.1 目标行为

对于接入 `memory_engine` 的 chat session：

1. AI 请求返回 context overflow
2. `chatos` 触发一次 `memory_engine` context compaction
3. 如果 compaction 成功并且上下文确实变小：
   - 重新从 `memory_engine` 取 context
   - 重试 AI 请求一次
4. 如果 compaction 超时 / 失败 / 无效：
   - 回退到现有本地截断逻辑

这样可以保证：

- 新能力生效时，优先使用真正的“远端压缩”
- 新能力异常时，不会破坏现在已有的恢复路径

## 7.2 推荐封装位置

建议在 `chatos_memory_engine` 里新增一个 session 级包装：

- `compact_chatos_session_context(session: &Session, reason: &str)`

推荐改动点：

- `chat_app_server_rs/src/services/chatos_memory_engine/sessions.rs`
- `chat_app_server_rs/src/services/chatos_memory_engine/types.rs`
- `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs`

这样可以把：

- session -> thread mapping
- tenant/source 封装
- compaction SDK 调用

都收在同一层，避免 v2/v3 AI client 直接拼 memory_engine 参数。

## 7.3 v2 接入点

推荐在这里接：

- `chat_app_server_rs/src/services/v2/ai_client/mod.rs`

当前逻辑是：

- 命中 overflow
- 调 `try_compact_for_token_limit(...)`

建议改成：

1. 命中 overflow
2. 如果 `purpose == "chat"` 且存在 `session_id`，先尝试一次 `memory_engine` context compaction
3. compaction 成功后，调用现有 `maybe_refresh_context_from_memory(...)` 刷新消息
4. 重新发起一次 AI 请求
5. 如果 compaction 没成功，再走原来的 `try_compact_for_token_limit(...)`

注意点：

- 单次顶层请求只尝试一次远端 compaction
- 不要在 loop 里无限触发

## 7.4 v3 接入点

推荐在这里接：

- `chat_app_server_rs/src/services/v3/ai_client/recovery_policy/request_error.rs`
- `chat_app_server_rs/src/services/v3/ai_client/recovery_policy/completion_error.rs`

当前逻辑是：

- context overflow -> 降 `history_limit`
- payload too large -> 裁剪 function call output / 降 `history_limit`

建议顺序改成：

1. 仅针对 `context_length_exceeded` 类错误，优先尝试一次 remote compaction
2. 成功后，基于当前 `raw_input` 重新 build stateless context
3. 重新请求 AI
4. 如果仍失败，再继续现有 `reduce_history_limit(...)` 逻辑

我建议第一版先只在 `context_length_exceeded` 上启用远端 compaction，不强行覆盖所有 `request body too large` 场景，因为：

- `request body too large` 很可能来自当前轮巨大的 tool output
- 这种情况远端 summary 不一定有效
- v3 现有的 `truncate_function_call_outputs_in_input(...)` 更直接

## 7.5 上下文刷新方式

这部分可以直接复用现有机制：

- v2：`maybe_refresh_context_from_memory(...)`
- v3：重建 stateless items，本质上会重新走 `message_manager.get_memory_chat_history_context(...)`

也就是说，`chatos` 不需要自己解释新的 summary，只要 compaction 成功后重新取一次 memory context 就够了。

## 8. 超时策略

这块是这次方案里最需要单独处理的。

## 8.1 不要复用通用 memory_engine 请求超时

现在 `chatos` 的 `memory_engine` client 用的是统一的：

- `memory_engine_request_timeout_ms`

这个超时更适合：

- `compose_context`
- `list_thread_records`
- `list_summaries`
- 普通 metadata 同步

但主动压缩会真正触发一次 AI summary，请求时长明显更长，不建议共用。

## 8.2 新增独立超时配置

建议在 `chat_app_server_rs/src/config.rs` 增加：

- `memory_engine_context_compaction_timeout_ms`

建议默认值：

- `20000` 到 `30000` ms

推荐默认先取：

- `25000` ms

原因：

- 这类请求通常会触发一次真实 LLM 总结
- 太短会导致大量“实际上已在后台执行，但客户端先超时”
- 太长又会拖慢一次聊天请求的恢复时间

## 8.3 `chatos` 侧调用策略

建议：

- 普通 `memory_engine` client 继续用现有超时
- compaction 单独建 client，使用 `memory_engine_context_compaction_timeout_ms`

也就是把：

- `chat_app_server_rs/src/services/chatos_memory_engine/client.rs`

改成既能构建默认 client，也能构建 compaction 专用 client。

## 8.4 超时后的行为

如果 compaction 超时：

- 记录 warning 日志
- 不把整个聊天请求直接判死
- 回退到当前已有的本地 compaction / history shrink 逻辑

这样做的原因是：

- 远端压缩是增强项，不应该让原有恢复路径失效
- 用户体验上，“退回到旧逻辑但还能继续回答”优于“因为远端压缩超时直接失败”

## 8.5 并发与重复触发

为了避免超时后重复触发多次 summary：

- 后端新接口在真正执行前先检查该线程是否已有 running `summary` job

这样即使：

- 第一次 compaction 请求客户端超时了
- 服务端还在跑
- 第二次请求又打过来

也会得到 `running=true`，而不是重复跑一遍 summary。

## 9. 测试建议

## 9.1 memory_engine backend

- 无 pending records 时返回 no-op
- 有 running summary job 时返回 `running=true`
- 正常 compaction 后 `pending_after_count < pending_before_count`
- compaction 后重新 `compose_context`，`recent_record_count` 应下降，`summary_count` 应上升或不下降

## 9.2 sdk

- direct 模式请求路径正确
- system key 模式请求路径正确
- 新 response model 能正常反序列化

## 9.3 chatos

- v2：overflow 后先尝试 remote compaction，成功时刷新 memory context 并重试
- v2：compaction 失败或超时时，继续走本地 token compaction
- v3：`context_length_exceeded` 时会触发 remote compaction
- v3：`request body too large` 仍优先保留现有 tool output 裁剪逻辑
- 单次顶层请求不会无限重复触发 compaction

## 10. 风险与边界

### 10.1 不是所有 overflow 都能靠远端 compaction 解决

如果超限来自：

- 当前轮超长 user input
- 当前轮巨大的 tool output
- 图片/结构化输入过大

那么远端压缩历史上下文也不一定能救回来。

所以 `chatos` 侧必须保留现有本地降级逻辑。

### 10.2 第一版建议只压一次

第一版建议每次 top-level AI 请求只做一次 remote compaction，原因是：

- 更容易控制延迟
- 更容易控制重试风暴
- 现有 `run_thread_summary(...)` 已经会把当前扫描到的 pending records 一次性吃进去

如果后面观察到仍有大量 pending 没被压掉，再考虑第二阶段加：

- `max_passes`
- `wait_for_completion_ms`
- 或异步 job + 轮询

### 10.3 `repair summary` 仍保留原用途

这次方案不会替换：

- `run_thread_repair_summary`
- `review_repair`

它们仍然用于“修复/校正上下文”，不是“让普通聊天上下文变小”。

## 11. 推荐实施顺序

1. `memory_engine backend` 新增 `context-compaction/run` 包装接口
2. `memory_engine sdk` 暴露 `run_thread_context_compaction(...)`
3. `chatos_memory_engine` 新增 session 级 compaction 包装
4. 先接 `v2` overflow 恢复链路
5. 再接 `v3` recovery policy
6. 补测试和日志

## 12. 我建议的最终落地口径

第一版我建议按下面的口径实现：

- 新接口名就叫 `context-compaction/run`
- 底层复用普通 `thread summary`
- 不走 `repair summary`
- 后端增加“同线程 running summary job 检查”
- SDK 暴露一个 thread 级 compaction 方法
- `chatos` 在 `context_length_exceeded` 时优先触发一次 remote compaction
- compaction 失败或超时，继续走现有本地降级逻辑
- `chatos` 使用独立的 compaction timeout，不复用通用 memory_engine timeout

如果你认可这个方向，我下一步会按这个方案开始实际改代码。  
