# Task 进度与最终交付契约

日期：2026-10-13。适用于 Main Chat 调用本地 Task runner；shared protocol 42。macOS 本轮实现，Windows 开发按本文对齐。

## 原问题与选择

FinFlow 四节点图执行成功，但旧默认 `graph_summary` 主动过滤全部开始回调，最终报告又缩成节点预览。用户无法在会话里确认各任务何时开始，也看不到完整综合结论。新增策略而不改变旧策略含义，避免改写冻结契约。

新批量工具默认 `progress_summary`，仍使用原来的8个 Task工具，不新增业务工具。`notification_policy` enum 增加此值，创建时将一致策略显式持久化为每个节点的 `graph_notification_policy`。历史显式策略及未声明策略的旧多节点图继续原行为；没有追补历史开始通知。旧单 Task仍为 `all_tasks`。

## 生命周期回调

1. 创建图、ready/queued、等待时间或 prerequisites：不声称开始。
2. worker 第一次从 queued claim 到 model_running，原子持久化 `task.run.started` 回调。简短正文为“已开始：任务名称”，带可打开任务详情的原回调引用。
3. `task.completed`、`task.failed`、`task.blocked`、`task.cancelled` 更新同 Run 同一消息，不为每个结束再插一条全文。失败/阻塞包含经过既有 sanitizer 的短原因；用户主动取消可见，级联/替代取消保留既有抑制规则。
4. 回调 ID 继续 `task_runner_callback::{source_user_message_id}::{task_id}::{run_id}`；无 Run 的阻塞/取消使用状态作为 scope。已有 started_at 保留，结束补 finished_at；事件账本分别保留开始/结束事实，UI 中消息被更新不等于没发过开始。
5. 同 Run 的连接重试不重复开始；用户重试 Task会产生新 Run、新状态消息。结束后的迟到开始不得把卡片改回 running。重复终态回调/重启协调不能重复交付。

不得让 Main 模型靠轮询生成这些消息。原 `wait_for_task_completion` 仍只交接一次；runtime事件负责回写，事件刷新驱动会话显示。

## 完整交付

批量工具可选 `final_delivery_ref: string`，指向本次图中的 `client_ref`。shared graph持久化为该 Task `input.final_delivery=true`，其他节点为false。存在标记时只能有一个，且它必须直接/间接依赖所有其他图内节点；未知引用、多个标记、非boolean或不完整依赖均拒绝，且创建失败无副作用。

当综合 Task成功且整图成功时，最终单条消息正文使用它的完整用户可见报告。不要再把正文经3行/420字的生命周期摘要器处理。没有综合节点时，最终单条消息包含图实际状态和所有成功节点完整报告；综合未成功时同样交付可用报告并说明未完成部分。保留报告中的用户请求的文件路径/链接、Markdown、表格、代码块和尾部结论。

`task_graph_terminal` payload新增 `delivery_text`（非空string时优先显示；旧策略可以为null）。已有 `summary`、`tasks` 保留作为旧客户端兼容与状态预览。每节点用户可见 `delivery` 也是经过过滤的正文，不含完整运行结构。选择正文优先 `content/answer/text`，其次 `report`（string或content），最后 `result_summary`；不读取reasoning/checkpoint/request或整个terminal_outcome。

会话展示顺序为Main handoff确认在前，之后Task进度和graph最终交付按持久化ordinal排列。graph完整报告虽然没有taskCallback引用，仍属于异步交付；不能把所有无callback消息一律提升到所有进度消息之前。重试generation同样保留原序，旧报告不会越过新Run进度。

完整正文沿用凭证/内部标识过滤边界，但不能把报告里讨论“503/timeout”等测试场景误判成运行失败。过滤不等于全面DLP；不要将其视为可以在报告中放凭证的许可。

终态写回继续使用 graph generation/signature，未变签名只交付一次；重试后真实终态变更产生新generation。交付归属标记不证明报告准确，Task仍必须执行预期输出和验收契约。

## Windows 开发对齐清单

- 升级 shared Host 到 protocol42，同步新 enum/schema及输入字段。原冻结Run保持不变；新的 Skill/能力产生新immutable revision，不能覆盖旧revision。
- 编排Skill默认 `progress_summary`，综合场景使用 `final_delivery_ref`，详细规则放引用文档按需加载。不要再从插件平台prompt单独注入另一套通知规则。
- 会话读取/历史恢复/事件刷新都优先非空 `delivery_text`，其次旧 `summary`，最后旧graph payload展开；最终完整消息不拆成多条节点报告。
- 开始/结束使用原 Task callback metadata恢复任务入口和状态，按同message_id更新；开始时间与结束时间同时可追溯。不要客户端自行推测开始。
- 无需数据库迁移，策略/标记存在input JSON，交付存在content JSON；不要回填历史消息。
- 不删除插件管理平台agent管理页：本次只修改进度/交付，不表示该页面其他职责已完成迁移。

## 自动化与实机验收

- 四节点（三独立＋一综合）：创建/排队0开始，claim时逐项可见，综合开始晚于全部前置完成，4次开始事实＋4条状态消息＋1条终态完整消息。
- 一份大于16000字报告保留Markdown/代码块及最后证据；凭证、reasoning、内部ID不进入正文；短preview不能覆盖全文。
- 无综合节点交付全部完整输出；失败/阻塞综合不被当成成功，已有成功报告完整可读。
- 重放开始/终态不产生重复或状态降级；用户重试保留旧Run状态并生成新Run消息/新终态generation。
- queued取消不得伪造started_at；依赖阻塞节点尚未执行，不可发开始。
- 原 `graph_summary/milestones/all_tasks` 与旧payload兼容测试继续通过。
- macOS 最新打包构建在 FinFlow Finance MVP进行实测，项目文件前后校验不变，SQLite只在新临时目录、虚构数据；记录真实Task图、开始事件、UI可见状态和最终完整交付，不能用旧构建或单Task烟测代替。

## 实机发现：无依赖节点仍排队（尚未优化）

2026-10-13本轮FinFlow图的动态验证无前置、已有queued Run、iteration=0，但静态审查持续优先拿到模型worker。不是写锁：Run已被materialize，writer锁只可能在这之前挡住创建Run。

当前 `LocalAgentHostCoordinator::drain_ready_work` 顺序await一次model step和一次Host tool step，`claim_next_run` 按 `created_at_unix_ms, run_id` 排序；早创建的continuation/model_ready Run会反复优先于晚创建queued Run。多个Task生命周期可交错，但没有多个模型请求同时执行的worker池。不能将“独立节点允许调度”描述为“模型调用已充分并行”。

建议下一步独立设计有界model并发/公平step调度和可解释排队原因；区分waiting_prerequisites、queued_worker、resource_lock、scheduled_time、followup_hold。不要为了消除等待文字伪造start，或跳过写锁、审批、lease和账户隔离。本轮进度/交付改动没有改变模型worker容量和上述排序。

## Skill：潜在同文件写入不默认并行

编排Skill入口、graph reference与software reference都明确要求规划前比较可能写入集合。两个任务可能修改同一个文件时，优先用blocking prerequisite串行或指定一个写入owner，其他任务只读审查/提案；不能以角色不同、文件内区域不同为由宣称写入独立。格式化、生成代码、安装/lockfile、迁移、设计源文件、文档/演示/表格输出等隐含写入都计入。范围不明先调查或保守串行，不伪造`owned_paths`；真正不相交的写入和独立只读工作仍可保留无依赖。验证最终版本的reader/reviewer必须在writer后执行。Windows编排Skill也应同步此指导，沿用既有静态冲突拒绝与运行时锁，不替换成仅prompt约束。
