# Cloud Agent 上下文失控与 Responses 空结果统一整改

## 目的

本文件固化 2026-09-11 对 Cloud Agent 长任务失败的证据、根因、不变量、修复项和回归矩阵。后续修改不得重新引入“完整请求逐轮持久化”“无终态也当成功”“无限空结果补问”等行为。

## 故障证据

- Task：`a65d1236-00f9-498e-aa30-803b5e40e6f1`
- Run：`d47b58ae-f63f-4e33-a0b3-9326b4e07833`
- Agent Run：`task_runner_agent_d47b58ae-f63f-4e33-a0b3-9326b4e07833`
- 模型请求共 198 次；第 84 次后没有可见结果，第 85–198 次均为 `empty_final_response_followup`，最终超过 Cloud Agent deadline。
- `task_runner_service.task_run_events` 有 24,602 条记录，逻辑 BSON 约 2.45 GB；其中 `model_request` 占 94.4%。本次 Run 的 198 个请求事件约 1.01 GB。
- 第 84 次 input 有 1,166 项、约 4.64 MB；806 个 message 实例只有 17 种不同内容，说明 runtime lifecycle/sticky 消息被跨 step 重复追加。

这里的 2.45 GB 是 Task Runner 运行事件集合中逐轮复制的模型请求，不是用户聊天历史。旧实现每轮保存完整累计 input、instructions、工具 schema 和 debug metadata，形成近似 O(n²) 增长。

## 根因链

1. Cloud Agent 把上一轮 `request_input_items + response.output` 作为下一轮完整 stateless input。
2. `before_model_request` 每轮再次追加相同 lifecycle 项，旧实现没有在 lifecycle 边界去重。
3. `ContextualTurnRunner` 看到 Responses durable history 后不构建 Memory refresh；token 计数又错误地放在 refresh 分支内，因此后续每轮完全绕过 20 万预检、主动压缩和硬限制。
4. 请求事件保存完整请求体，使运行事件集合随迭代次数平方增长。
5. SSE 解析器只显式处理 `response.completed` 和 `response.failed`，漏掉 `response.incomplete`；先到达的 `response.created` 因而掩盖了正式 incomplete 终态。
6. 解析器只要求“至少一个合法 SSE event”，不要求 Responses 正式终态。
7. Cloud Agent 每次 MQ delivery 是独立进程状态；本地 loop 的空结果布尔保护无法跨 delivery 生效，造成无限补问。
8. 本地启动脚本用 `: > log` 清空旧日志，失败后重启会破坏诊断证据。

## 官方协议基线

- [OpenAI Compaction](https://developers.openai.com/api/docs/guides/compaction)：Responses create 可通过 `context_management: [{"type":"compaction","compact_threshold":200000}]` 启用 server-side compaction；stateless input-array chaining 必须续传 compaction output item，并可删除最新 compaction item 之前的项目。
- [OpenAI Responses streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events)：`response.completed`、`response.incomplete`、`response.failed` 是需要区分的响应终态；incomplete response 携带 `incomplete_details.reason`。
- Server-side compaction 是 OpenAI Responses 能力，不向仅声称 OpenAI-compatible 的第三方接口发送，不实现“兼容降级仿写”。

## 强制不变量

1. 每个模型请求都必须执行 token 安全检查，不能依赖 Memory refresh 是否可用。
2. 20 万 token 是主动压缩阈值，不是所有模型共同的硬 context window。
3. 只在 lifecycle/sticky 注入边界对完全相同的 runtime-owned 项去重；不得全局去重用户消息或工具历史。
4. Responses 流必须看到 `completed / incomplete / failed` 之一；只有 `response.created` 后 EOF 必须失败。
5. `incomplete` 是明确的 provider 终态错误，不能进入普通 empty-final followup。
6. Cloud 单步 empty-final 最多补问一次；第二次必须终止并给出明确错误。
7. durable `model_request` 和 `model_response` 事件只保存有界诊断摘要，禁止保存 input、reasoning、工具输出、完整 response body 或密钥。
8. 最新 compaction item 之前的 stateless history 必须裁剪；compaction item 本身必须保留。
9. 服务重启必须轮转旧日志，不能覆盖诊断证据。

## 统一修复清单

- [x] lifecycle input 使用专用精确去重合并；普通用户/工具 append 语义不变。
- [x] token 计数与 Memory refresh 解耦，每轮均检查。
- [x] OpenAI Responses 请求在 200,000 token 启用 server-side compaction。
- [x] Cloud durable history 保留最新 `type=compaction` 项并裁剪它之前的历史。
- [x] `AiResponse` 保存 response status、incomplete details、terminal event、request ID、HTTP status 和 SSE 计数。
- [x] 流解析显式覆盖 `response.incomplete`；缺少 Responses 终态时报 incomplete stream error。
- [x] incomplete/failed 在 empty-final 判断前转成明确错误。
- [x] Cloud empty-final followup 只允许一次。
- [x] `model_request` 改为有界摘要，历史巨型 payload 查询时省略并强制分页。
- [x] 新增有界 `model_response` 事件，包含终态、usage、item 类型计数、耗时和 request ID。
- [x] 接通 turn phase 与 context summary start/stream/end 事件，summary 正文不持久化。
- [x] Memory recent records 限制为 64 条，usage metadata 仅保留 token 摘要。
- [x] 本地 backend/app 启动日志改为按 UTC 时间轮转并保留 7 天。

## 回归矩阵

| 场景 | 必须结果 | 自动化覆盖 |
|---|---|---|
| durable Responses history 且无 Memory refresh | 仍执行 token guard | `durable_history_without_memory_refresh_still_hits_the_token_guard` |
| 相同 lifecycle item 跨 step 已存在 | 不再增加副本 | `lifecycle_hook_does_not_duplicate_an_existing_runtime_item` |
| `created → incomplete` | 保存 status、reason 和 terminal event | `responses_incomplete_terminal_event_preserves_reason` |
| 只有 `created` 后 EOF | 返回缺少终态错误 | `responses_created_without_terminal_event_is_rejected` |
| incomplete 且 content 为空 | 明确 incomplete error，不发 empty followup | `incomplete_response_is_an_error_and_preserves_the_official_reason` |
| 连续两次 empty-final | 第二次终止 | Cloud single-step runtime tests |
| response.output 含 compaction | 仅保留最新 compaction 起始窗口 | `latest_compaction_item_replaces_the_older_stateless_prefix` |
| model request/response 事件 | 不含原文且体积有界 | runtime/task-runner callback tests |
| 本地服务重启 | 旧日志改名保留，不再清空 | `test_local_dev_log_rotation.py` |

## 运维说明

- 新事件只阻止继续增长，不会自动删除已有 2.45 GB 历史数据。历史清理属于独立、可审计的数据维护动作，执行前必须确定保留策略和备份要求。
- `CHATOS_AI_CONTEXT_WINDOW_TOKENS` 可显式覆盖模型 hard limit；未配置时使用已知模型能力或保守默认值。主动 compaction 阈值保持 200,000。
- Memory Engine 主动总结默认每 10 秒查询一次状态，最长等待 10 分钟；调用方仍可通过 `active_summary_poll_timeout_ms` 显式调整。保留有界等待是为了避免总结任务异常时永久占住 Cloud Agent 执行。
- 本地日志归档格式为 `<service>.log.YYYYMMDDTHHMMSSZ`，保留 7 天。
