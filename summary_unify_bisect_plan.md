# 对话总结统一抽象 + 超长二分总结方案（v2/v3 通用）

## 1. 背景与问题

### 1.1 当前现状
- v2 有 `conversation_summarizer`，且具备动态总结与 token-limit fallback。
- v3 目前只会“读取已有摘要”，没有主动触发总结能力。
- 两边总结逻辑、触发条件、fallback 行为不一致，导致维护成本高、行为不稳定。

### 1.2 本次故障根因（已验证）
- 会话上下文（尤其工具输出）累计过大，触发 provider `context_length_exceeded`。
- v3 没有在主链路上触发动态总结，导致只能在超窗后失败。

## 2. 目标

1) **不做单边迁移**，而是抽出共享能力，让 v2/v3 共用同一套总结引擎。  
2) 总结请求若仍超长，支持**二分递归总结**：一次不行就拆两半；还不行继续拆。  
3) 主问答链路支持“先预防、后兜底”：
- 预防：达到阈值先总结再发模型请求
- 兜底：若主请求报超窗，触发总结压缩后自动重试

## 3. 设计原则

- 统一抽象：算法与策略下沉为 shared module；v2/v3 只做 adapter。
- 渐进改造：先接入 v2（行为等价），再接入 v3，减少回归风险。
- 可观测：每次总结都记录触发原因、切分层级、是否截断、压缩率。
- 可回滚：通过开关控制新策略（尤其二分递归与兜底重试）。

## 4. 模块拆分（新建）

建议新增目录：`chat_app_server_rs/src/services/summary/`

### 4.1 `types.rs`
- `SummaryOptions`（message_limit / token_limit / keep_last_n / target_tokens / model / temperature 等）
- `SummaryCallbacks`（on_start/on_stream/on_end）
- `SummaryResult`（summary_text/system_prompt/truncated/stats）
- `SummaryStats`（input_tokens/output_tokens/chunk_count/max_depth/compression_ratio）

### 4.2 `traits.rs`
定义统一适配接口：
- `SummaryLlmClient`：执行“总结请求”的最小能力（屏蔽 v2/v3 请求结构差异）
- `SummaryStore`：消息读取、摘要记录写入、摘要关联关系写入

> v2/v3 分别实现 adapter：
> - `v2/summary_adapter.rs`
> - `v3/summary_adapter.rs`

### 4.3 `token_budget.rs`
- 统一 token 估算、消息 token 统计
- 安全截断函数（字符串/parts/tool 内容）
- “tool call + tool output”边界保护（避免切裂关联）

### 4.4 `splitter.rs`
- 二分切分策略：`split_for_summary(messages) -> (left, right)`
- 切分约束：
  - 不把 tool output 单独落在一侧（尽量跟对应 call 保持同侧）
  - 保证左右都非空
  - 最小切分粒度（避免无限切）

### 4.5 `engine.rs`
核心流程：
- `maybe_summarize(...)`（预防触发）
- `summarize_with_bisect(...)`（超窗递归总结）
- `retry_after_context_overflow(...)`（主请求失败后的兜底压缩与重试入口）

### 4.6 `persist.rs`
- 写入 `session_summaries` 与 `session_summary_messages`
- 写入“摘要占位消息”到 `messages`
- metadata 统一字段：
  - `algorithm: "bisect_v1"`
  - `trigger: "proactive" | "overflow_retry"`
  - `chunk_count`
  - `max_depth`
  - `truncated`

## 5. 二分递归总结算法

## 5.1 触发条件
- 主动触发：
  - `message_count >= SUMMARY_MESSAGE_LIMIT` 或
  - `estimated_tokens >= SUMMARY_MAX_CONTEXT_TOKENS`
- 被动触发：
  - 主请求返回 `context_length_exceeded`

### 5.2 算法步骤（伪流程）
1. 先尝试对全集 `M` 做一次总结。  
2. 若成功，返回摘要。  
3. 若失败且是超窗：
   - 按边界规则切分 `M -> L + R`
   - 分别对 `L`、`R` 递归总结，得到 `SL`、`SR`
   - 再对 `[SL, SR]` 做一次“合并总结”得到 `S`
4. 若“合并总结”仍超窗：
   - 对 summary 列表继续分层归并（pairwise reduction）直到收敛
5. 终止保护：
   - `max_depth`（如默认 6）
   - `min_chunk_messages`（如默认 4）
   - 单条仍超窗则做强制截断并标记 `truncated=true`

### 5.3 错误分类
- 仅对“上下文超窗”触发二分递归。
- 其他错误（鉴权、限流、网络）直接上抛，不进入递归。

## 6. v2/v3 接入方案

### 6.1 v2 接入
- 用 shared engine 替换现有 `conversation_summarizer` 里可复用部分。
- 保留 v2 现有回调语义与输出协议不变。
- `try_compact_for_token_limit` 改为调用 shared `retry_after_context_overflow`。

### 6.2 v3 接入
- 在 `process_with_tools` 请求前加入 `maybe_summarize`（主动）。
- 在 `context_length_exceeded` 分支里调用 `retry_after_context_overflow`（被动）。
- 与当前“history_limit 递减”策略并存：
  - 优先摘要压缩
  - 摘要失败再执行 history_limit 对半退让

## 7. 配置与开关

复用现有配置：
- `DYNAMIC_SUMMARY_ENABLED`
- `SUMMARY_MESSAGE_LIMIT`
- `SUMMARY_MAX_CONTEXT_TOKENS`
- `SUMMARY_KEEP_LAST_N`
- `SUMMARY_TARGET_TOKENS`

新增建议（默认值）：
- `SUMMARY_BISECT_ENABLED=true`
- `SUMMARY_BISECT_MAX_DEPTH=6`
- `SUMMARY_BISECT_MIN_MESSAGES=4`
- `SUMMARY_RETRY_ON_CONTEXT_OVERFLOW=true`
- `SUMMARY_MERGE_TARGET_TOKENS`（默认同 `SUMMARY_TARGET_TOKENS`）

## 8. 兼容性与数据

- 不改数据库表结构。
- 通过 metadata 扩展记录算法信息。
- 保持现有摘要展示文本格式，避免前端适配成本。

## 9. 测试计划

### 9.1 单元测试
- token 估算与截断边界
- 二分切分不破坏 tool call/output 关联
- 递归终止条件（max_depth/min_chunk）
- 超窗错误识别

### 9.2 组件测试
- mock LLM：
  - 大输入返回 `context_length_exceeded`
  - 小输入返回摘要
- 验证递归收敛、合并总结正确

### 9.3 集成回归
- v2：动态总结触发路径不回归
- v3：长会话+工具输出场景可自动压缩并继续对话
- 重点回归会话：`尾坐式无人机`

## 10. 分阶段落地

### 阶段 A（抽象）
- 新建 `services/summary/*`
- 先把 v2 中通用逻辑搬入 shared（功能等价）

### 阶段 B（v2 切换）
- v2 改为调用 shared 引擎
- 行为保持一致，确保测试全绿

### 阶段 C（v3 接入）
- 接入主动总结 + 超窗兜底总结
- 保留 history_limit 递减作为次级兜底

### 阶段 D（开关放量）
- 默认开启二分总结
- 观察日志与摘要质量，必要时调参

## 11. 验收标准

- v2/v3 都使用同一套 summary engine（不再双维护）。
- 长会话发生超窗时，不再直接失败；可自动压缩后继续回复。
- 日志可定位每次总结触发原因与递归深度。
- 回归测试通过，且对现有接口无破坏。

---

如果你确认这个方案，我下一步会按“阶段 A -> B -> C”执行，并在每一轮把变更记录持续写入 `chat_app_server_rs/backend_refactor_plan.md`。
