# Codex Gateway 与 Rust 后端对接整改方案

## 1. 目标

面向当前链路（`chat_app_server_rs` -> OpenAI Responses 兼容网关 -> Codex app-server），实现以下目标：

1. Rust 项目可以直接稳定使用 `chat_app_server_rs/docs/codex/openai-codex-gateway/server.py` 对话。
2. 明确屏蔽 Codex 自带 MCP 来源（Apps / Plugins / Connectors），只允许我们 Rust 侧内置 MCP（经 function tool 暴露）生效。
3. 保证流式结束信号可靠（客户端能稳定收到 done）。
4. 明确 `previous_response_id` 在 Rust 侧是否启用及启用策略。

---

## 2. 现状与问题定位

### 2.1 `previous_response_id` 在 Rust 聊天主链路默认不启用

在 `chat_app_server_rs/src/services/v3/ai_client/mod.rs` 中：

- `purpose == "chat"` 时，`prefer_stateless = true`。
- `use_prev_id = !prefer_stateless && ...`，因此聊天模式默认不会走 `previous_response_id`。

结论：当前 Rust 主链路主要是“无状态拼接上下文”，不是 prev-id 连续会话模式。

### 2.2 Gateway 对“禁用 Codex 原生 MCP”的触发条件与 Rust 工具形态不匹配

`server.py` 里 `extract_request_config_overrides()` 仅在 `tools[].type == "mcp"` 时才注入：

- `mcp_servers`
- `features.apps = false`
- `features.plugins = false`
- `features.connectors = false`

而 Rust v3 当前发送的是 `type=function` 工具（由内置 MCP 转换而来），不是 `type=mcp`。

结果：多数 Rust 请求不会触发“禁用 Codex 原生 MCP”逻辑，Codex 侧仍可能启用其默认工具来源，干扰我们自己的工具链。

### 2.3 tool 能力在 resume 分支存在丢失风险

在 gateway 的 `_run_turn()`：

- `thread_start` 分支会带 `dynamicTools`
- `thread_resume` 分支未必带 `dynamicTools`（需要统一）

这会导致一旦启用 `previous_response_id`，续轮可能出现工具能力不一致。

### 2.4 流式 done 可靠性

网关已具备 `[DONE]` 发送逻辑，但要确保“发送后连接终止”，避免上游读取端等待 EOF 导致 done 不向下游闭环。

---

## 3. 目标架构（建议）

采用“Rust External Tools Mode（网关开关）”：

- 当请求来自 Rust 后端并声明该模式时：
  - 强制禁用 Codex 原生 MCP 来源（apps/plugins/connectors）。
  - 仅允许请求里的 `function tools`（即 Rust 侧映射出的工具）参与选择。
- 保持 gateway 对通用调用方的兼容性（不强制全局禁用，避免影响其他客户端）。

---

## 4. 详细改造方案

## Phase A（仅改 gateway，最小侵入，优先落地）

### A1. 网关服务级默认“Rust 外部工具模式”

由于该 gateway 只服务 Rust 后端，不再引入请求打标分支，直接服务级默认执行：

- 对每个请求都注入以下 config 覆盖：

- `features.apps = false`
- `features.plugins = false`
- `features.connectors = false`

说明：这样可以稳定屏蔽 Codex 自带 MCP 来源，让模型只通过 dynamicTools 触发我们 Rust 工具。

### A2. 保留请求级 `tools.type = mcp` 兼容（可选）

若请求显式传 `type=mcp`，仍可构造 `mcp_servers` 注入 config；
若只传 `type=function`（Rust 当前主路径），则只注入 features 关闭项。

### A3. 统一 thread_start / thread_resume 的 dynamicTools 行为

在 `_run_turn()` 内统一：

- `thread_start` 带 `dynamicTools`
- `thread_resume` 同样带 `dynamicTools`

避免续轮工具能力丢失。

### A4. done 闭环保持一致

保证 `send_done_marker()` 始终：

- 发送 `event: done`
- 发送 `data: [DONE]`
- flush 后主动关闭连接

---

## Phase B（Rust 侧对接增强，可选）

### B1. `previous_response_id` 策略明确化

当前 chat 模式默认 stateless，建议先维持不变。

如后续要启用 prev-id（为了减少重复上下文 token）：

1. 提供显式配置开关（例如模型级或环境开关）
2. 仅对网关 base_url 开启白名单
3. 同时要求 gateway resume 分支动态工具完整带入

备注：未开启前，不依赖 prev-id 也可稳定使用。

---

## Phase C（验证与回归）

### C1. Gateway 侧验证

基于现有 `openai-codex-gateway/tests` 增补/调整：

1. `function tools + external_tools_mode` 下，确认请求配置确实禁用了 apps/plugins/connectors。
2. 两轮会话（带 `previous_response_id`）验证 resume 分支 dynamicTools 不丢失。
3. SSE 流结束时，客户端稳定收到 `[DONE]` 并正常收尾。

### C2. Rust 端联调验收

以你当前 chat v3 真实链路验收：

1. 模型可正常回答。
2. 需要工具时，只调用 Rust 暴露的 function tools（不出现 Codex 默认工具源干扰）。
3. 工具回传（function_call_output）后可继续完成答案。
4. 流式 UI 能稳定结束，不再卡住等待 done。

---

## 5. 兼容性与风险

1. 若全局默认禁用 apps/plugins/connectors，可能影响非 Rust 客户端；建议通过请求级标记控制。
2. 如果后续开启 prev-id，需要同步关注 thread 生命周期与 response_id 映射存活策略。
3. function tool 命名必须稳定且唯一，避免模型侧冲突/覆盖。

---

## 6. 执行顺序建议

1. 先做 Phase A（只改 gateway）。
2. 联调通过后，再决定是否做 Phase B 的 prev-id 开关。
3. 最后补齐 Phase C 测试，形成长期回归保障。

---

## 7. 本次结论

你现在的核心诉求“Rust 直接接这个 gateway，并且只用我们自己的 MCP”是可实现的。
关键不在 Rust 内置 MCP 本身，而在 gateway 的工具来源裁剪条件与 Rust 工具形态当前不匹配。

先把 gateway 改成“外部工具模式下强制禁用 Codex 原生 MCP + resume 也带 dynamicTools”，就能对齐当前架构。
