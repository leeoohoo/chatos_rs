## Memory Engine / Chatos 问题清单

### P0

- [x] 修复 `memory_engine` 定时总结扫描聚合报错
  当前生产日志显示 `Unsupported conversion from object to string in $convert with no onError value`，导致 worker 每次 tick 都失败，根本没有进入真正的 summary 执行。

- [x] 修复 `chatos` V2 超限后的主动总结触发语义
  当前 V2 仍然会在一次聊天请求里同步等待 active summary，最长 120 秒；需要改成异步触发后立刻返回“总结中/请稍后重试”。

- [x] 给 `memory_engine` 摘要 AI 请求补瞬时失败重试
  生产上已有 active summary chunk 在执行中因为 `Our servers are currently overloaded` 直接失败，需要对 overloaded / 502 / 503 / 504 / timeout / eof 等瞬时错误自动重试。

### P1

- [x] 补充 V2 context overflow 错误识别验证
  覆盖 `Your request exceeded model token limit` 这类报错，确认一定能命中主动总结恢复逻辑。

- [x] 补充 active summary 触发和状态日志
  便于区分“未触发”“已触发但仍在执行”“执行失败”三类现场。

- [x] 核对 `compose_context` 是否完全符合既定规则
  重点验证“2 条 level0 summary + 1 条最顶层 summary + 1 条 level0 memory + 1 条最顶层 memory + 当前会话全部未总结消息”。

- [x] 调查并修复 `subject_memory` 的 `missing field tenant_id`
  当前是次级问题，但生产日志里持续报错，需要单独收尾。
