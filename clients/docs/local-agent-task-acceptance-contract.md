# Task 验收与最终交付契约

日期：2026-10-13。适用于 shared Rust Host 与 macOS；Windows native/UI 由 Windows 开发对齐。原 8 个 Main Chat Task 工具不增不减。

## 问题与边界

FinFlow 上轮纯文字测试中，A/B 的最终回复分别为 438/442 个 Unicode scalar，却自行报告 succeeded。执行结束不等于内容通过验收；综合评审正确发现缺口，评审 Task 本身可以成功，被评审产物不能因此标为合格。

本次 gate 检查验收覆盖、结构、成功判定和显式最终文本长度。自然语言标准的证据仍由模型提供，不是独立事实认证；不会用正则猜测任意“400字”描述，也不把财务测试的上限变成所有任务的固定限制。

## 创建与冻结

- `create_task`、`create_tasks_with_prerequisites.tasks[]` 新增可选 `final_response_max_chars`，整数 1..32000。
- 主聊天仅在最终文字交付确有长度限制时填写。Unicode scalar 计数包含全部标题、标点、Markdown、空格及换行；不是 UTF-16 单元、token、单词数或导出文件正文限制。不同计数口径应明确，不能强行转换。
- Host 为这些工具新建的 Task 注入 `acceptance_report_version=1`，与 `acceptance_criteria`、可选长度限制一起冻结。模型不能选择旧版来关闭验收。
- 已有 Task（含重试）没有该标记时维持原契约，不改写历史结果。直接协议创建 Task 的调用方若使用新版，须传入标记与非空标准。
- 每步向 worker 独立发送 `task_acceptance_contract` 数据，避免历史裁剪丢失标准。指导来自统一 worker policy Skill，业务场景定义来自编排 Skill。

## 上报

`task_run_process_report_outcome` 保留 `status` 与 `reason`。新版 Task 还要求：

```json
{
  "status": "succeeded",
  "reason": "已逐项检查文字交付；没有执行界面或账务测试",
  "final_response": "最终给用户的完整原文",
  "acceptance_checks": [
    {
      "criterion_index": 0,
      "status": "passed",
      "evidence": "具体产物、检查方法和实际结果"
    }
  ]
}
```

索引零起始，对应冻结标准顺序，每项恰好一次，不可重复或遗漏。检查状态为 `passed/failed/not_verified`，证据非空、每项≤2000 scalar；succeeded 要求所有项 passed。failed/blocked 允许列出失败或未核验项及原因。评审 Task 检查自己的评审标准，并在交付中保留被评审材料的缺口，不替它宣布通过。

最终原文非空，默认上报工具文本上限 32000；显式上限存在时采用它。超限返回实际字符数，可缩短后重报；拒绝不锁死上报，恰好一个**被接受**的报告。接受后是最后一次工具调用。

工具结果保存标准检查与完整原文。下一次无工具模型响应不能改写已检查产物：profiles reducer 使用报告的 `final_response` 作为 terminal content，CommitStep 再检查一致性，终态保存 `task_acceptance`。现有 Task 状态调和继续按上报 succeeded/failed/blocked；Run succeeded 表示执行与交付链结束，不表示失败报告变成 Task 成功。

上报结果还包含程序计算的 `final_response_chars`。前置结果传递时，从已交付原文重新计算 `acceptance_summary`（字符数、冻结上限、长度检查结果、worker声明状态），以独立 JSON 和提示上下文提供给评审；不采信作者在 evidence 中手算的字数。内容部分明确标为 worker 声明、非独立认证。旧 Task 无这些事实，评审应说明未验证范围。

## 已完成前置与回调

跨图引用成功 Task 时，新任务需在创建事务中判断其前置已满足，不能只因“存在前置”而永久 pending。调度前的只读快速检查也识别满足条件的旧 pending 记录，再在事务内按 owner 解除等待；未完成前置、跟进hold、定时执行和写入冲突约束不取消。不为旧 A/B 重跑或重复发开始回调。

`progress_summary` 的开始/完成是同一个状态消息：真实开始时创建“已开始”，完成后更新为“已完成”，另有最终综合交付。开始与完成事件都保留，但会话回看不会出现两条重复状态。引用历史任务的图节点不会触发一次新的开始回调。Windows应保持这些语义，不能把引用旧成功节点等同于重新执行。

## 配置刷新与执行中断

macOS启动先用IPC-only进程加载持久状态，再恢复带凭据的worker。后续控制面刷新必须复用相同owner、相同凭据环境的worker，不能无条件restart。生命周期仅保留长度分隔的SHA-256指纹，不记录凭据原文。能力/schema/model revision仍照常发布；真正变更的凭据（包括memory token）暂缓应用并明确提示用户，在任务完成后正常重新启动应用。当前不提供运行中凭据热替换或无损drain，不能用“查询当前空闲后立即重启”绕过并发领取竞态。Windows原生生命周期也应遵循此原则。

恢复与联网刷新必须使用一致的模型凭据装配规则：先加载同owner持久模型的有效本地凭据，再以当前目录的权威凭据覆盖匹配key。历史模型不在当前可选目录中不等于密钥发生变化；保留其环境变量供冻结Run使用，不将其重新加入联网刷新后的可选模型或扩充能力权限。缺失、空白凭据和不匹配的credentialRef不加载，规范化变量名碰撞拒绝。真正新增模型key、密钥轮换或memory token变更/缺失仍暂缓；不是凭据热更新。Windows应对齐此规则，避免每次启动后因历史目录过滤反复提示重开。

shared Host将过期模型claim转为 `needs_review`，Task调和为blocked，不自动重跑。运行记录保存 `terminal_outcome.reason_code=execution_interrupted` 及保守的用户说明：执行中断、当前步骤结果无法确认、检查执行记录后决定是否重试，避免重复操作。不能因租约到期就断言“没有任何副作用”或“必然是前置问题”。回调与任务详情应显示该原因，中英本地化不暴露Host、claim token或内部trace。

历史 `needs_review` 且terminal outcome为空的Run，仅在同owner下存在 `claim_expired_needs_review` 事件时补齐上述诊断。保留原中断时间、Task状态和任务版本，不生成新Run；已有诊断不覆盖，其他原因的needs_review不改写。旧状态消息可原地补齐说明，不重复创建开始通知。

回填旧尝试时，只有Task仍blocked、无active Run、该Run仍是最新尝试才允许修复其会话回调。若已经成功重试或正在执行新尝试，只补齐旧Run的诊断，不覆盖当前或历史成功消息。

回调事件ID不能只由Run version或内容摘要决定：显示修复可能不改变Run version，且同一内容可能先前已经出现。消息内容/metadata比较在写事务内判定幂等，完全相同则不写消息、不增加会话版本、不新增事件；确有消息变化时为这次变更生成独立UUID事件ID。不得用忽略所有UNIQUE冲突掩盖错误。恢复为以前出现过的内容仍必须成功，并且随后重复回放无增量。Windows应沿用shared Host的事件，不自行拼装旧版事件ID。

macOS CI必须先构建真实Rust Host、执行storage历史回归，再运行Swift生命周期与工作区集成测试。设置 `CHATOS_REQUIRE_NATIVE_HOST_TESTS=1` 后，缺失或不可执行的Host必须失败，不能XCTSkip。联系人/会话加载、相同凭据PID复用、改变凭据不终止旧进程、旧诊断回填不覆盖成功重试及回调修复循环均为回归重点。

## 待检查任务的取消

宠物对needs_review显示“取消任务”时，底层必须允许用户取消对应blocked Task，不能仅因Task是阻塞终态而拒绝。取消的是后续工作流，不是对原步骤副作用的认证或撤销：Task与未完成的依赖后续变为cancelled，原needs_review Run、terminal outcome、时间和版本保留。真正失败或成功的历史Task不伪装成被取消的执行。owner、当前任务版本、命令幂等和级联数量约束继续有效。

该Task从active Run投影移除；保留的未知Run仍可在执行历史查看。取消blocked Task时发布task_cancelled事件，触发宠物重新查询，不依赖纯UI隐藏；回调显示Task的实际取消时间，而不是旧Run中断时间。取消失败必须在当前浮窗中明确显示原因，不能只在另外的任务列表渲染错误。Windows原生取消入口及错误显示应对齐这些语义；shared Host已实现持久状态处理。

## Windows 对齐

1. 为单Task及批量item的 schema 增加同一可选长度字段，不增加工具，也不增加单Task不拆分理由。
2. 恢复旧配置时对**新能力revision**增补 schema，保留已安装插件选择、模型选项等动态字段；不覆盖旧 Run 快照。
3. 两个调研 companion (`requirement_survey_create` / `requirement_plan_save`) 都去重，迁移重复运行应零发布；仍只生成新revision，不修改历史。
4. 发布更新后的编排/worker Skill。shared Host 执行验收与文本冻结；Windows 不应自行改写 final content 或将 Run 状态替代 Task 状态。
5. UI 如展示验收证据，明确它是 worker 记录，不包装成独立审计认证；可从 terminal `task_acceptance` 读取，旧结果保持兼容。

## 回归重点

全标准覆盖、重复索引拒绝、空证据拒绝、not_verified 不得报成功、失败/阻塞仍可交付、评审发现缺陷本身可成功、含中文/换行/标点的精确边界、超限纠正、检查后模型改写被替换、CommitStep绕过被拒绝、旧 Task不迁移、每步合同恢复、schema迁移幂等及财务项目文件零变化。
