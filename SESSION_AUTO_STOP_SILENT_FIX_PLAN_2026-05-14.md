# 会话自动停止且无报错修复方案（2026-05-14）

## 背景

本次问题表现为：

- 会话在执行过程中“自己停了”
- 前端没有展示明确报错
- 用户主观感受是“像异常中断，但界面又不像失败”

本次排查基于今天实际运行日志，而不是仓库根目录旧日志。

真实运行日志：

- [/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4007)

说明：

- 仓库内 [logs/backend.log](/Users/lilei/project/my_project/chatos_rs/logs/backend.log:1) 仍是 `2026-04-09` 的旧日志，不能用于判断这次问题。

## 影响范围

已确认的异常会话：

- session: `c5644aee-4568-43a2-a02d-a501c5ba9b7f`
- 项目目录：`/Users/lilei/project/work/zj/zus/zeus`
- 用户截图时间大致为北京时间 `2026-05-14 13:01` 到 `13:07`

这类问题的潜在影响不止这一条会话：

- 任何使用 `AI_V3` + 工具调用链路的对话
- 尤其是“工具跑完后最后一轮模型响应为空”的场景
- 当前前端容易把这种情况误显示成“正常结束但没回复”

## 时间线

以下时间均来自后端日志，原始日志为 UTC；括号内换算为北京时间 `UTC+8`。

### 1. 第一次请求明确失败：余额/配额不足

- `2026-05-14T05:01:55Z`（北京时间 `13:01:55`）开始 `kimi-k2.6` 请求。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4004)
- `2026-05-14T05:01:56Z`（北京时间 `13:01:56`）上游返回 `429 Too Many Requests`，错误信息明确是 `insufficient balance` / `exceeded_current_quota_error`。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4007) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4008)

结论：

- 这一次不是静默失败，而是明确的上游配额错误。

### 2. 第二次请求未报错，但在工具链后“空结束”

- `2026-05-14T05:02:40Z`（北京时间 `13:02:40`）开始 `gpt-5.4` 请求。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4031) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4046)
- 之后持续进入多轮工具执行与模型续跑，`tool_call_count` 依次出现 `1/1/1/1/5/1/6/8/1/1/4` 等结果，说明整个 turn 一直在正常推进，而不是中途 crash。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4077) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4120) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4138) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4168)
- `2026-05-14T05:07:22Z`（北京时间 `13:07:22`）最后一次流式解析结果为：
  - `response_id=resp_08ca7d...`
  - `tool_call_count=0`
  - 后续没有错误日志。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4174)
- `2026-05-14T05:07:31Z`（北京时间 `13:07:31`）前端继续查询 turn process，接口返回 `200`，说明后端把这轮 turn 当作“正常结束”处理了，而不是失败态。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4178) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4179)

结论：

- 第二次 `gpt-5.4` 请求没有后端异常、没有网络异常、没有 panic。
- 它是“工具链跑完以后，最后一轮模型返回了一个空的终态响应”，随后被后端误判为成功。

### 3. 非主因噪音

- 同一时间段有一条 `open-computer-use` MCP 构建失败警告，但后面仍然成功构建出 `56` 个工具，不是这次自动停掉的直接原因。[backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4029) [backend.log](/private/tmp/chatos_rs_dev_c2fc3ea9/backend.log:4030)

## 根因结论

### 1. `AI_V3` 只把 `finish_reason=failed/error` 视为错误

当前失败判定逻辑在：

- [assistant_response.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/ai_common/request_support/assistant_response.rs:42)

现状：

- 只有 `finish_reason` 是 `failed` 或 `error` 才会返回错误
- 对于“终态但内容为空”的响应，没有独立判错分支

这意味着：

- 只要 provider 没显式给 `failed/error`
- 即使这轮响应没有正文、没有 reasoning、没有 tool call
- 后端也可能继续把它当作成功处理

### 2. 执行环在“无 tool call”分支直接返回 success

关键逻辑在：

- [execution_loop.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs:295)

现状：

- 一旦 `has_tool_calls == false`
- 且没有被 `completion_failed_error(...)` 拦下
- 代码就直接 `return Ok(build_ai_client_success_payload(...))`

问题在于：

- 这里没有校验 `content` 是否为空
- 也没有校验 `reasoning` 是否为空
- 更没有区分“正常终态回复”和“空终态回复”

这正好解释了本次日志：

- 最后一轮 `tool_call_count=0`
- 没有显式 `failed/error`
- 所以后端直接结束 turn

### 3. 空终态 assistant message 可能被当成正常消息持久化

当前 assistant 落库策略在：

- [assistant_response.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/ai_common/request_support/assistant_response.rs:83)

现状：

- 只要响应状态不是 `in_progress/queued/pending/incomplete`
- 即使 `content`、`reasoning`、`tool_calls` 全为空
- `should_persist_assistant_message(...)` 仍可能返回 `true`

风险：

- 系统可能保存一条“空 assistant 最终消息”
- 对话历史上技术上有 final assistant，但用户看不到有效回复
- 前端从数据上很难区分“真正完成”还是“空完成”

### 4. 前端缺少“空终态回复”的显式失败语义

从这次现象看，前端至少存在两个薄弱点：

- 没有拿到明确 error 事件
- 也没有把“final assistant 为空”解释成失败

结果就是：

- 用户看起来像“自己停了”
- 系统看起来像“正常结束了”

这不是简单前端展示问题，而是后端终态语义定义不完整导致的。

## 修复目标

### 目标 A：空终态响应不能再被当成成功

要求：

- 对 `tool_call_count=0`
- 且 `content/reasoning` 都为空
- 且响应已经进入终态的场景

必须：

- 自动重试，或
- 显式失败

不能直接成功返回。

### 目标 B：失败语义必须贯穿到持久化层

要求：

- turn runtime status 能标记失败
- assistant 最终消息不能再以“空成功”形式写入
- 后续历史压缩、process 展开、review-repair 都能看见失败态

### 目标 C：前端必须明确告诉用户“这轮没有成功产出回复”

要求：

- 会话退出 busy/executing 状态
- 显示错误文案或失败占位
- 用户能区分“正常结束”和“空结束异常”

### 目标 D：日志和指标要能直接检索这类问题

要求：

- 后续看到类似 case 时，可以一眼搜出
- 不再靠人工读几十轮 tool call 日志推断

## 修复方案

## 一、后端修复

### 1. 增加“空终态响应”判错逻辑

建议位置：

- [assistant_response.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/ai_common/request_support/assistant_response.rs:42)
- 或 [execution_loop.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs:256)

建议做法：

- 新增辅助判断，例如：
  - `terminal_empty_response_error(...)`
  - 或扩展现有 `completion_failed_error(...)`

判定条件建议为：

- `tool_calls` 为空
- `content` 为空
- `reasoning` 为空
- `finish_reason/status` 已进入终态
- 且不是 `in_progress/queued/pending/incomplete`

建议返回错误文案：

- `ai response invalid: terminal empty response`

日志中必须带：

- `session_id`
- `turn_id`
- `response_id`
- `finish_reason`
- `tool_call_count`
- `iteration`

### 2. 为“空终态响应”增加有限次自动恢复

参考现有非终态空响应恢复逻辑：

- [execution_loop_state.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop_state.rs:14)

建议新增一条与之平行的恢复策略：

- `try_recover_from_terminal_empty_response(...)`

恢复步骤建议：

1. 第一次命中时先记录 `warn`
2. 禁用 `previous_response_id` 复用
3. 强制重建 stateless context
4. 保留已完成的 tool outputs
5. 做 1 到 2 次退避重试

超过阈值后：

- 不再返回 success
- 直接返回显式错误

这样既能兜住 provider 的偶发空响应，也不会让无限重试拖住会话。

### 3. 收紧 `!has_tool_calls` 分支的成功条件

当前问题入口：

- [execution_loop.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs:297)

建议改成：

- `!has_tool_calls` 不再等价于成功
- 必须同时满足“有有效 assistant 输出”才允许 success

建议成功条件至少满足其一：

- `content` 非空
- `reasoning` 非空且前端支持独立展示
- 明确存在可展示的结构化终态载荷

否则：

- 走恢复逻辑
- 或进入失败态

### 4. 禁止把空终态 assistant 当作正常 assistant 持久化

当前风险点：

- [assistant_response.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/ai_common/request_support/assistant_response.rs:83)

建议改动：

- `should_persist_assistant_message(...)` 对“空终态响应”返回 `false`
- 或改为持久化一条合成错误消息，而不是空 assistant 消息

更稳妥的方案：

- 保存一条带错误 metadata 的 assistant/system message
- 文案明确，如：
  - `本轮回复未成功生成，请重试`
  - `AI 返回了空终态响应`

这样历史记录、compact history、turn process 展开都能稳定感知到失败。

### 5. 为 turn runtime / process 接口增加明确失败态

相关接口：

- [history_process.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/sessions/history_process.rs:1)

建议：

- turn runtime 增加失败终态枚举，例如：
  - `completed`
  - `failed`
  - `cancelled`
  - `terminal_empty_failed`

至少保证：

- `/process`
- `review-repair`
- compact history metadata

都能看出这轮是“失败结束”，而不是“成功但没内容”。

### 6. 增强观测性

建议增加以下日志关键字：

- `[AI_V3] terminal empty response detected`
- `[AI_V3] terminal empty response recovered`
- `[AI_V3] terminal empty response failed`

建议增加指标：

- `ai_v3_terminal_empty_response_total`
- `ai_v3_terminal_empty_response_recovered_total`
- `ai_v3_terminal_empty_response_failed_total`

## 二、前端修复

### 1. 把“无有效 final assistant 的完成态”显示成失败

建议检查链路：

- `compactHistory`
- turn process fetch
- 发送消息后的流式收尾状态机

目标行为：

- 如果 turn 已结束
- 但最终 assistant 内容为空
- 且后端给出失败 metadata

前端应：

- 退出执行中状态
- 展示错误提示
- 不把这轮伪装成普通完成

### 2. 给用户明确的错误反馈

建议表现：

- chat 区域内显示一条失败占位消息
- 或 session header / error banner 明确提示

建议文案：

- `本轮回复未成功生成，请重试`
- `模型返回了空结果，系统已停止本轮执行`

### 3. 保证 busy 状态一定可收口

即使后端未来还有其他异常终态，也要满足：

- `completed`
- `failed`
- `cancelled`

三类之一必须落地，不能无限等待。

## 三、测试方案

### 1. 后端单元测试

新增覆盖：

- `completion_failed_error(...)` 扩展后的空终态判定
- `should_persist_assistant_message(...)` 对空终态返回 false
- `terminal_empty_response` 恢复逻辑：
  - 第一次重试成功
  - 多次重试后失败

建议位置：

- `chat_app_server_rs/src/services/ai_common/tests.rs`
- `chat_app_server_rs/src/services/v3/ai_client/tests.rs`

### 2. 后端集成测试

构造一个 Responses mock：

- 前几轮返回 tool calls
- 最后一轮返回：
  - `tool_call_count=0`
  - `content=""`
  - `reasoning=null`
  - `finish_reason="completed"` 或 `stop`

断言：

- 不会直接 success
- 会先走恢复
- 恢复失败后 turn 状态为 failed
- 不会保存空 assistant 最终消息

### 3. 前端回归测试

验证场景：

- 后端返回 `terminal_empty_failed`
- UI 能退出 busy
- 能展示失败提示
- 重试按钮或继续发送新消息不受影响

## 验收标准

满足以下条件才算修复完成：

1. 同类“工具链后空终态响应”不会再静默成功。
2. 后端日志能明确看到 `terminal empty response` 关键字。
3. `/process` 和历史数据中能区分这轮是失败而不是成功。
4. 前端不再出现“自己停了但没报错”的体验。
5. 单测和集成测试覆盖该回归场景。

## 实施顺序

建议按以下顺序落地：

1. 后端新增空终态判错与恢复逻辑
2. 后端收紧 assistant 持久化策略
3. 后端暴露明确失败终态
4. 前端按失败终态展示并清理 busy 状态
5. 补测试与日志指标

## 当前落地进展（2026-05-14）

已完成的后端修复：

- `chat_app_server_rs/src/services/ai_common/request_support/assistant_response.rs`
  - 新增 `terminal_empty_response_error(...)`
  - 现在对“终态 + content/reasoning/tool_calls 全空”的响应会返回明确错误
- 同文件中的 `should_persist_assistant_message(...)`
  - 已收紧为空响应一律不持久化
  - 不再把“空终态 assistant”写成看起来像正常完成的历史消息
- `chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs`
  - `!has_tool_calls` 分支不再直接等价于成功
  - 现在会先尝试恢复 terminal empty response，再在超过阈值后显式失败
- `chat_app_server_rs/src/services/v3/ai_client/execution_loop_state.rs`
  - 新增 terminal empty response 恢复逻辑
  - 恢复策略为：禁用 `previous_response_id`、重建 stateless context、保留 pending tool call/output、有限次退避重试

已补测试：

- `chat_app_server_rs/src/services/ai_common/tests.rs`
  - 增加 terminal empty 判错测试
  - 增加“空 completed 响应不持久化”测试
- `chat_app_server_rs/src/services/v3/ai_client/tests.rs`
  - 增加“terminal empty 首次恢复成功”测试
  - 增加“terminal empty 超过恢复阈值后失败”测试

当前验证结果：

- 新增 3 条定向回归测试已通过
- 这意味着“工具链结束后空 completed 响应被静默当成功”的核心问题，后端已先封住

尚未完成但仍建议继续做的项：

- turn runtime / process 接口是否要引入更细的失败态，例如 `terminal_empty_failed`
- 前端是否要把这类失败态显示成更明确的占位/错误提示，而不是只依赖通用错误消息
- tools/status 调试入口的 `conversation_runtime` 边界收口，可在本次 bug 修复稳定后继续推进

## 非目标

本方案不处理以下独立问题：

- `kimi-k2.6` 的 `429 insufficient balance` 计费/配额问题
- `open-computer-use` MCP 本地连接失败告警
- 其他与本次 silent stop 无直接关系的 provider 可用性问题

## 建议的后续动作

修完这份方案后，建议顺手再补一轮“终态语义审计”：

- `success but empty`
- `error but persisted as normal assistant`
- `tool batch failed but UI still waiting`

这三类都属于同一种设计缺口：系统内部状态和用户感知终态没有完全对齐。
