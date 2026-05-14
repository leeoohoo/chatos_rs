# 工具卡住与前端持续“执行中”修复方案

## 背景

本次问题不是单一前端展示错误，而是一次后端工具执行异常与前端状态机兜底不足叠加导致的结果。

用户表现：

- 工具面板中出现 `@list_dir` 与多个 `@search_files` 长时间“等待中”
- 对话没有继续往下执行
- 点击停止后，前端仍长期停留在“执行中 / 停止中”

已确认的本次运行实例：

- 会话 `c4b019f8-5f3d-438d-bc4d-f925504c5529`
- turn `turn_4d2e3385b86c42a4950b97f59a816188`
- `2026-05-14 01:35:42` 后端解析到 `tool_call_count=6`
- 紧接着 `2026-05-14 01:35:42.623872` 发生 panic

相关日志：

- 运行日志 `/tmp/chatos_rs_dev_c2fc3ea9/backend.log`

## 根因结论

### 1. 直接根因：`search_files` 命中中文行时触发 Rust 字符串切片 panic

`search_files` 实际走的是 `search_text` 别名链路：

- [aliases.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/code_maintainer/aliases.rs:129)
- [registration_read.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/builtin/code_maintainer/registration_read.rs:173)
- [mod.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/workspace_search/mod.rs:51)

问题代码：

- [mod.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/workspace_search/mod.rs:133)

```rust
let snippet = if normalized.len() > 400 {
    normalized[..400].to_string()
} else {
    normalized.to_string()
};
```

这里的 `400` 是字节偏移，不是字符边界。  
当匹配到包含中文的长行时，`[..400]` 可能切在 UTF-8 多字节字符中间，触发：

- `byte index 400 is not a char boundary`
- 日志明确指出切在字符 `'页'` 内部

这说明本次“卡住”不是工具慢，而是工具执行过程中直接 panic。

### 2. 为什么看起来是 `list_dir` 和多个 `search_files` 都在等

这批工具在当前实现中会走“并发读工具”执行路径，而不是简单串行：

- [execution.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/mcp_execution_core/execution.rs:37)
- [parallelism.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/mcp_execution_core/parallelism.rs:5)

`list_dir`、`search_text` 都被标记为 parallel-safe read tools。  
因此这次不是“前一个没跑完，后一个排队”，而是整批工具中的一个 `search_files` panic 后，整批工具没有拿到正常的结束收尾。

结果就是：

- 工具开始事件已经发给前端，面板已创建
- 但缺少完整的 `tools_end` / 最终结果回填
- UI 上多个工具项就会继续停留在未完成态

### 3. 为什么不会继续往下执行

工具执行结束后，系统才会把 tool outputs 回灌给模型，进入下一轮推理：

- [execution_loop.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/v3/ai_client/execution_loop.rs:338)
- [stream_support.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/ai_common/stream_support.rs:279)

当前流程要求：

1. `tools_start`
2. 执行工具
3. `tools_end`
4. 保存工具结果
5. `advance_after_tool_execution(...)`
6. 继续下一轮模型请求

但本次在第 2 步工具执行期间就 panic 了，所以第 3 至第 6 步没有正常发生。  
这就是“他不继续往下执行”的直接原因。

### 4. 为什么前端还在“执行中”，这部分确实不对

前端停止逻辑与完成逻辑依赖“流式正常收尾”或“明确取消/完成事件”：

- [streaming.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/streaming.ts:66)
- [streamExecution.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/streamExecution.ts:49)
- [useChatStreamRealtimeBridge.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts:296)
- [sessionState.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/sessionState.ts:133)

现状行为：

- 用户点击停止后，前端会把 `isStopping=true`
- 它故意不立即恢复，设计上要等待“后端 cancel / done / complete / recover”来清状态
- 本次后端不是正常取消或正常完成，而是工具执行线程 panic

因此前端没有收到它期待的稳定结束信号：

- 没有可靠的 `tools_end`
- 没有可靠的整轮 `done/complete`
- 恢复逻辑也没有把这类“后端异常终止但没有完整 persisted turn messages”的场景完全收口

所以前端持续显示执行中，这个现象本身说明：

- 后端 panic 需要被转换成结构化失败事件
- 前端需要把“异常终止”视为终态，而不是永远等待

## 修复目标

需要同时修两层问题。

### 目标 A：后端工具执行不再因为 UTF-8 截断 panic

要求：

- `search_files/search_text` 在任何中文、多字节字符、emoji、混合内容下都不会 panic
- 工具结果 snippet 长度可控，但必须按字符边界截断

### 目标 B：即使工具执行或流执行异常，前端也必须退出“执行中”

要求：

- 后端异常必须被转成结构化失败结果或失败终态
- 前端不能无限等待 `tools_end` 或 `done`
- 停止按钮触发后，即使后端发生 panic / stream 中断 / tool batch failure，也要能落到 `completed`、`error` 或 `cancelled` 三种终态之一

## 修复方案

## 一、后端修复

### 1. 修复 `search_text` 的 snippet 截断

文件：

- [mod.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/workspace_search/mod.rs:131)

建议做法：

- 把 `normalized[..400]` 改成按字符截断
- 推荐封装一个小工具函数，例如：
  - `truncate_for_snippet(input: &str, max_chars: usize) -> String`
- 实现方式可选：
  - `input.chars().take(max_chars).collect::<String>()`
  - 或先用 `char_indices()` 找到合法边界再切片

建议：

- `400` 明确解释为“最多 400 个字符”，不是 400 字节
- 统一后续其他 snippet/preview 生成逻辑，避免同类 bug 分散存在

### 2. 给工具执行外层增加 panic 隔离

当前问题不是普通 `Err`，而是 panic。  
panic 会破坏工具批次正常收尾，导致 UI 永远收不到终态。

建议在工具执行层增加 panic 到 error 的转换：

优先落点：

- [execution.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/mcp_execution_core/execution.rs:81)
- 或 builtin tool call 入口

目标行为：

- 任一 builtin tool panic 时，不要让整个会话 silently 卡住
- 将该工具结果转成：
  - `success=false`
  - `is_error=true`
  - `content="工具执行失败: internal panic ..."`
- 保证工具批次仍能发出最终 `tools_end`

Rust 侧可考虑：

- 用 `tokio::spawn` / `JoinSet` 的 `JoinError` 显式映射为 `ToolResult`
- 或在同步 builtin 调用边界用 `std::panic::catch_unwind`

注意：

- `catch_unwind` 只建议包住 tool invocation boundary，不要大面积吞 panic
- 同时要记录 panic backtrace 供排障

### 3. 为工具批次增加统一失败终态

文件：

- [stream_support.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/ai_common/stream_support.rs:279)

目标：

- 不论正常结束、error、aborted、panic，都要有明确的“工具生命周期结束”语义

建议：

- 将 `on_tools_end` 扩展为可携带状态：
  - `completed`
  - `failed`
  - `aborted`
- 前端据此决定每个工具卡片和整轮消息状态

如果短期不改协议，至少要保证：

- 出错时也发 `tools_end`
- 尚未完成的 tool call 全部被补成 error/completed 终态

### 4. turn runtime snapshot / persisted messages 中写入失败终态

当前恢复逻辑大量依赖 snapshot 与 persisted turn messages。

建议后端在工具批次异常时同步写入：

- turn runtime status=`failed`
- assistant message status=`error`
- tool result messages 带 error payload

这样前端即使丢失了实时事件，也能通过恢复接口收敛状态。

## 二、前端修复

### 1. 前端不能只等正常流结束

现状：

- `abortCurrentConversation()` 把 `isStopping=true`
- 之后依赖取消事件、完成事件或恢复逻辑清状态

文件：

- [streaming.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/streaming.ts:66)

改进建议：

- 为“停止中”增加超时收敛策略
- 如果在一定时间内没有收到：
  - `cancelled`
  - `done`
  - `complete`
  - snapshot recovery 终态
- 则主动触发 turn recovery

建议超时：

- 2s 到 5s 内先查 runtime snapshot
- 如果 snapshot 已是 `failed/completed/cancelled`，立即 `finalizeStreamingSessionState` 或 `failSendMessageState`

### 2. realtime bridge 要识别“后端异常终态”

文件：

- [useChatStreamRealtimeBridge.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts:202)
- [turnRecovery.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/turnRecovery.ts:220)

当前 recovery 已能识别：

- `completed`
- `failed`
- `error`
- `cancelled`

但这次问题说明 recovery 触发时机和兜底还不够激进。

建议：

- 在以下情况主动触发 recovery：
  - `isStopping=true` 且超过阈值
  - 工具开始后长时间没有 `tools_end`
  - SSE / realtime 连接断开
  - stream reader 报错

恢复后的收敛规则：

- snapshot.status=`running` 才允许继续显示执行中
- 只要是 `failed/error/cancelled/completed`，就必须强制关闭 streaming state

### 3. 工具面板要支持“批次异常结束”

文件：

- [toolEvents.ts](/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage/toolEvents.ts:130)

现状：

- tool call 初始 `completed=false`
- 主要靠 `tools_end` / 非 stream final result 回填 `completed=true`

建议新增一个“工具批次失败补偿”逻辑：

- 当整轮 turn 被判定为 `error/failed/cancelled`
- 对所有尚未 `completed` 的 tool call：
  - 标记 `completed=true`
  - 填入统一错误说明，如“工具批次异常结束，未收到最终结果”

这样 UI 不会一直显示“等待中”。

### 4. stop 后的 UX 文案与状态拆分

现有 `isStopping` 同时承担：

- 禁止重复发送
- 表示等待后端取消

建议拆成更细状态：

- `running`
- `stopping`
- `completed`
- `failed`
- `cancelled`

如果短期不重构，至少要保证：

- `isStopping` 不能无限期存在
- 一旦确认后端终态，必须立即归零

## 三、联调与验证方案

### 1. 后端单测

为 `workspace_search::search_text` 增加测试：

- 长中文行，长度超过 400 字符
- 400 字节附近恰好落在多字节字符内部
- 中英混合
- emoji/特殊 Unicode

验证：

- 不 panic
- 返回 snippet 合法
- 结果列号仍正确

### 2. 工具执行异常集成测试

模拟某个 builtin tool panic，验证：

- 工具执行接口返回 error tool result，而不是让整个流悬空
- `tools_end` 仍然被发送
- turn runtime snapshot 标记为 `failed`

### 3. 前端状态测试

补以下场景：

- `tools_start` 后无 `tools_end`，但 recovery snapshot=`failed`
- 用户点击停止后，只收到 partial cancel，没有 done
- 工具批次失败后，未完成 tool call 被统一收尾
- streaming session 最终 `isStreaming=false`、`isStopping=false`

### 4. 端到端验证

建议复现脚本：

1. 构造包含长中文行的文件
2. 让模型发起 `search_files`
3. 验证：
   - 不再 panic
   - 工具卡片能正常结束
   - 模型继续下一轮执行
4. 人工点击停止
5. 验证：
   - 前端不会永久停留在“停止中/执行中”
   - 工具面板最终进入 completed/error/cancelled 之一

## 实施优先级

### P0

- 修 `search_text` 的 UTF-8 截断 panic
- 工具执行边界把 panic 转成结构化 error

### P1

- turn runtime snapshot / tool batch failed 终态补齐
- 前端在 stop / tool timeout / stream error 时主动 recovery

### P2

- 工具面板未完成项统一补偿收尾
- streaming / stopping 状态机细化

## 建议落地顺序

1. 先修后端 `search_text` 截断 bug
2. 再补工具执行 panic 隔离
3. 再补后端失败终态持久化
4. 最后补前端 stop/recovery/tool-panel 收敛

## 预期修复结果

修复完成后，这类问题应表现为：

- `search_files` 即使遇到中文长行也不会崩
- 任意工具异常都能以“失败”而不是“永远等待”结束
- 前端不会无限显示执行中
- 点击停止后，最迟在 recovery 超时窗口内回到明确终态
