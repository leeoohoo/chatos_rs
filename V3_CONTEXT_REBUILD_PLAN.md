# V3 上下文实时重组方案（草案）

## 1. 目标

在 `v3` 聊天过程中（一次请求内可能有多轮：模型 -> 工具 -> 模型），如果后台新生成了会话总结，且原上下文中部分消息已被总结覆盖，需要**在下一次调用 AI 前自动重组上下文**：

- 重新拉取最近 `2` 条总结；
- 重新拉取当前全部 `pending` 消息；
- 以这两部分为准重建输入上下文。

## 2. 现状问题

当前 `v3` 在一次请求开始时构建上下文后，后续迭代主要复用内存里的 `stateless_context_items`。  
如果这期间 summary job 写入了新总结，当前内存上下文可能“过时”，会出现：

1. 已被总结的旧消息仍留在上下文里；
2. 新总结没有及时进上下文；
3. 遇到超长时可能直接走缩窗，导致上下文语义不稳定。

## 3. 核心思路

引入“**上下文版本检测 + 按需重组**”机制。

### 3.1 上下文版本（Context Version）

每次迭代前读取一个轻量版本指纹（不需要拉全量消息）：

- `latest_summary_id` / `latest_summary_updated_at`（最近总结版本）
- `pending_count`
- `pending_last_message_id`（可选）

若版本变化，说明总结或 pending 集合有更新，需要重组上下文。

### 3.2 上下文重组器（Context Assembler）

统一一个组装入口（v3 内部）：

1. 调 `get_chat_history_context(session_id, 2)` 拿到
   - merged summary（2条总结合并）
   - 全量 pending messages
2. 组装为 Responses API 的 input items
3. 去重末尾重复 user（保持当前行为）
4. 保持 tool call / tool output 的配对规则（保持当前行为）

> 组装结果始终遵循“总结 + 全部 pending”。

## 4. 触发时机

在 `process_with_tools` 循环中增加以下检查点：

1. **每次发起模型请求前**（最关键）  
   - 检查 context version；
   - 发现变化则重组。

2. **工具结果落库后，下一次模型请求前**  
   - 再次检查，避免工具执行期间 summary 更新被漏掉。

3. **发生 context overflow 时**  
   - 先做一次“强制重组”（总结 + 全 pending）；  
   - 若仍溢出，再按策略处理（见第 6 节）。

## 5. 代码改造点（建议）

1. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`
   - 增加 `ContextVersion` 结构；
   - 增加 `read_context_version(...)`；
   - 在 `process_with_tools` 每轮请求前调用 `maybe_rebuild_context(...)`。

2. `chat_app_server_rs/src/services/v3/message_manager.rs`（或 `message_manager_common.rs`）
   - 增加轻量版本查询接口（只取版本指纹，不取全量内容）。

3. 保持已有 `build_stateless_items(...)`，但让其可被“主动重组”调用，不再只在首次构建时生效。

## 6. overflow 策略（供你确认）

这里给两个可选策略：

- **策略 A（严格一致，按你当前诉求）**  
  overflow 时只做“重组（总结 + 全 pending）”，不再缩窗；若仍超长则直接报错。

- **策略 B（稳妥）**  
  先重组；仍超长才进入缩窗兜底（当前逻辑保留在最后一道）。

> 我建议先上 **策略 B**，并加开关，便于切到 A 做验证。

## 7. 配置开关（建议）

- `V3_CONTEXT_REBUILD_ON_SUMMARY_CHANGE=true`（默认开）
- `V3_CONTEXT_OVERFLOW_POLICY=rebuild_then_shrink`（可切 `rebuild_only`）

## 8. 日志与可观测性

新增结构化日志字段：

- `context_version_before` / `context_version_after`
- `context_rebuilt`（bool）
- `rebuild_reason`（`summary_changed` / `pending_changed` / `overflow_retry`）
- `pending_count`、`summary_count`

## 9. 测试计划

1. **单测**：版本变化检测正确触发重组；
2. **单测**：重组后仍保证 tool call / tool output 成对；
3. **集成测试**：模拟“工具执行期间 summary 更新”，下一轮请求使用新总结+pending；
4. **回归测试**：无 summary 更新时不重复重组，性能稳定。

## 10. 实施顺序

1. 先加版本检测与日志（不改业务行为）；
2. 接入重组调用点（每轮请求前）；
3. 接入 overflow 策略开关；
4. 跑压测与回归；
5. 默认开启，观察后再决定是否切到 `rebuild_only`。

