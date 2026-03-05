# request body too large 优化方案（先方案，暂不改代码）

## 0. 背景

生产日志出现：
- `ERROR [AI_V3] stream request failed: status=500 ... request body too large`

这说明问题不是“模型上下文 token 超限”本身，而是**在到达模型前，请求体字节数已经过大**。

---

## 1. 现状定位（基于当前代码）

### 1.1 MCP 工具结果目前是“可非常大 + 会原样进入后续模型请求”

1) MCP 返回文本未做统一大小治理：
- `core/mcp_tools.rs` 的 `to_text` 直接返回文本，无统一截断。

2) AI V3 在 tool call 后会把 tool output 直接放进下一轮 `input`：
- `services/v3/ai_client/mod.rs` 中 `function_call_output.output = r.content`（原样注入）。

3) tool 结果也会落库，后续无总结前会再次被拼进上下文：
- `services/message_manager_common.rs` 的 `save_tool_results` 直接存 `result.content`。

4) terminal 内置 MCP 虽有单次命令输出上限（`max_output_chars=20000`），但“拉日志”接口可组合出很大 payload：
- `builtin/terminal_controller/mod.rs` 的 `get_recent_logs` 返回 `logs` 全量结构；
- `per_terminal_limit` 最多 200，`terminal_limit` 最多 100，累计内容可非常大。

### 1.2 自动总结触发当前是“只看条数，不够条数直接跳过”

- `modules/session_summary_job/executor.rs`：
  - 先取 `round_limit` 条 pending；
  - `pending.len() < message_limit` 则 `threshold_not_met`，不会触发总结；
  - 仅在“条数达标后”才按 `token_limit` 做分片。

- `modules/sub_agent_summary_job/executor.rs` 同样逻辑。

这会导致：
- 少量但超长消息（尤其工具日志）在 pending 中长期堆积，无法被总结清理；
- 下一轮 AI 请求继续携带这些超长消息，触发 body too large 风险。

---

## 2. 目标（按你的思路）

1) **内置 MCP 输出有节制**，尤其 terminal 日志工具，避免一次工具返回过大。
2) **自动总结触发策略升级**：
   - 先看条数（现有优先级不变）；
   - 条数不达标时，再看长度（token）是否达标，达标也触发总结；
   - 条数触发后若超 token，保持“分片多次总结”逻辑不变。
3) **单条消息超 token_limit 的处理统一**：
   - 不进入总结输入；
   - 但最终必须被标记为已总结（避免永远 pending）。

---

## 3. 方案 A：MCP 输出治理（分层限流）

### A1. terminal `get_recent_logs` 增加“可控压缩输出”

改造点：`builtin/terminal_controller/mod.rs`

建议：
- 新增返回裁剪策略（默认开启）：
  - 单条 log 文本上限（例如 1200~2000 chars）；
  - 整体结果总字符上限（例如 12000~20000 chars）；
- 在返回中显式标记：
  - `truncated: true/false`
  - `truncation`: `{ per_log_capped, total_capped, dropped_logs }`
- 默认把 `per_terminal_limit` 从 10 保持不变，但将“最大允许值”从 200 降到更安全值（比如 50）；`terminal_limit` 最大值从 100 降到 20（可配置）。

> 重点：不是禁用日志，而是给模型看的结果必须“可控、可解释、可追踪”。

### A2. 全局 MCP 文本出口增加统一上限（最后一道闸）

改造点：`core/mcp_tools.rs`（`to_text` 或工具执行结果落地前）

建议：
- 对所有 MCP 文本结果做统一 `max_chars` 截断（例如默认 16k，可 env 配置）；
- 截断保留“头 + 尾”并插入标记（比只截头更利于日志诊断）；
- metadata 写入：`tool_output_truncated=true`, `original_chars`。

这样即便某个工具漏了局部限制，也不会无限放大到 AI 请求体。

### A3. AI 请求前增加 payload 大小保护（保险丝）

改造点：`services/v3/ai_request_handler/mod.rs` 或 `services/v3/ai_client/mod.rs`

建议：
- 在发请求前估算 JSON body 字节数；
- 若超过阈值（如 1.5MB，可配置）：
  1. 先降级历史（减少 history_limit）；
  2. 再裁剪 tool output 项；
  3. 仍超限则失败并打明确日志（包含当前 body_size）。

这层是兜底，避免再次出现 500 `request body too large`。

---

## 4. 方案 B：自动总结触发改造（条数优先 + 长度补充）

改造文件（两套都要改，保持一致）：
- `modules/session_summary_job/executor.rs`
- `modules/sub_agent_summary_job/executor.rs`

### B1. 新触发决策（优先级不变）

伪逻辑：

1) 拉取 pending（仍按时间升序）。
2) **先看条数**：
   - `pending_count >= round_limit` -> 触发（`message_count_limit`），候选消息 = 前 `round_limit` 条。
3) **条数不达标再看长度**：
   - 候选消息 = 当前全部 pending（此时 `< round_limit`）；
   - `candidate_tokens >= token_limit` -> 触发（`token_limit`）；
   - 否则维持 `threshold_not_met`。

### B2. 单条超限消息处理（核心）

在进入分片前先拆两组：
- `oversized_single`: 单条 token > `token_limit` 的消息；
- `summarizable`: 其余消息。

规则：
- `oversized_single` **不参与** LLM 总结输入；
- 最终 `mark_summarized` 时，与本轮 summary 一起标记为已总结；
- 若本轮全是 `oversized_single`（`summarizable` 为空）：
  - 创建一条“done 但 summary_text 为空”的 summary 记录（trigger 标记含 `oversized_skipped`）；
  - 用该 `summary_id` 批量标记这些消息为 summarized；
  - 不调用 LLM（避免无效成本和死循环）。

### B3. 分片总结逻辑保持不变

对 `summarizable` 继续沿用当前：
- `split_chunks_by_token_limit` 递归二分；
- chunk 分别总结；
- 最后 merge；
- trigger_type 按实际拼接：
  - `message_count_limit`
  - `message_count_limit+token_limit_split`
  - `token_limit`
  - `token_limit+token_limit_split`
  - 以上任意再附加 `+oversized_single_skipped`（若存在）。

---

## 5. 观测与日志增强

建议增加结构化日志字段（session/sub-agent 两边一致）：
- `trigger_type`
- `selected_messages`
- `selected_tokens`
- `oversized_skipped_count`
- `oversized_skipped_ids_preview`（前 N 个）
- `split_chunks`
- `marked_messages`

这样线上可直接看出：
- 是条数触发还是长度触发；
- 是否因为超长单条被跳过；
- 是否真正消化了 pending。

---

## 6. 验收用例（必须覆盖）

1) **条数触发**：`pending >= round_limit`，正常总结并标记。
2) **条数触发 + 分片**：固定条数总 token 超限，发生多 chunk，总结成功。
3) **长度触发**：`pending < round_limit` 但总 token 超限，也会总结。
4) **单条超限 + 混合**：超限消息被跳过，其他消息正常总结，最终都被 mark summarized。
5) **全是单条超限**：不调 LLM，也能生成 summary 记录并标记 summarized。
6) **terminal get_recent_logs 大返回**：返回被裁剪并有 `truncated` 标记，后续 AI 请求体明显下降。
7) **AI 请求体保险丝**：构造超大输入时触发降级/裁剪，不再出现 500 body too large。

---

## 7. 实施顺序（建议）

1) 先做 **B（总结触发改造）**，快速消除 pending 长期堆积。
2) 再做 **A1 + A2（MCP 输出限流）**，从源头压缩工具输出。
3) 最后加 **A3（请求体保险丝）** 做兜底。

---

## 8. 兼容性与风险

- 风险 1：过度裁剪导致模型信息不足。
  - 规避：保留 tail + 明确 `truncated` 标记；优先裁剪日志类内容。

- 风险 2：空 summary_text 的 done 记录可能影响部分列表展示。
  - 规避：前端列表默认过滤空总结或显示“超长消息已跳过并标记”。

- 风险 3：session/sub-agent 两套逻辑不一致。
  - 规避：提取共用 helper，保持同一套触发与筛选算法。

---

## 9. 预计收益

- 显著降低 `request body too large` 概率；
- 防止超长单条消息卡住 summary 队列；
- 保持现有“条数优先 + 超限分片总结”的主逻辑不变；
- 线上可观测性更好，便于持续调参。
