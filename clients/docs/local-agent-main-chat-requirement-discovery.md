# 项目主聊天与需求调研

日期：2026-10-13。shared Host 与 macOS 实现；Windows 按同一契约对齐。本文前段记录初期 Survey 实现与历史验收。**后续新增版本化方案、human 确认与 Task 冻结，当前契约以 [需求方案绑定](local-agent-requirement-plan-binding.md) 为准**；初期“无迁移／只上下文复用”的结论已被新实现替代。原 8 个 Task 工具不变。

## 现状与变化

需求调研中心已是 Local Agent Host 持久化的项目模块，不属于 Agent 团队。此前项目主聊天创建的后台 Task 可以在选择调研能力后调用 `requirement_survey_create`，但 Main Chat 没有直接工具，executor只允许task_execution；且模型planner缺少 `requirement_survey_resolved` 续跑分支。

现在 Main Chat 可在分解任务前直接发起调研，无需创建“先问问题”的占位Task。Task原有调研路径保留；两者使用同一项目调研中心、存储、答案校验与Run恢复机制。联系人聊天无项目绑定，不能伪造项目调研，仍用普通对话澄清。

## Skill与使用判断

- 自动加载的 `chatos-main-chat-invariants` 指导先理解用户目标；编排入口同样在拆分前检查关键未知项。
- 新 `chatos-requirement-discovery` 作为on-demand Product Skill，`requirement_survey_create`有独立catalog binding与持久化激活门控，不塞入插件平台prompt。
- 入口说明创建、等待、提交后使用答案；reference按开发/财务、设计、文案、研究/数据、混合产物说明如何提问。
- 问题围绕实际目标、受众、限制、优先级、偏好和验收；通常一轮3–6个关键问题，复用已知答案。明确任务直接执行，不一律问卷，不要求用户替模型选择无关技术细节。
- Main直接澄清关键决策，不能将未经确认的用户偏好交给worker猜测。提交后将已确认答案转为任务输入、预期输出及验收标准，而非仅回复“收到”。

## 链路与边界

1. 激活调研Skill，单独调用 `requirement_survey_create`，只传title/description/questions，不允许模型提供项目、源会话或源Run ID。
2. executor校验当前owner；Main要求 `main_chat + conversation_turn` 且原会话turn/run匹配，Task要求 `task_execution + task`。项目绑定从源会话读取，拒绝联系人、越权owner和错误来源。
3. 工具创建成功并commit后Run进入 `waiting_user`。Main会话显示一条持久化通知，指向需求调研；项目聊天工具栏也有入口。创建期间不以“handoff”伪称工作已开始。
4. 用户在同一调研中心提交真实答案。Host验证必填/类型/版本，原子保存与恢复原Run为 `continuation_ready`；重复提交按既有command幂等处理，不另开Run。
5. planner将结构化答案补为原 `requirement_survey_create` 的 `function_call_output`，避免unsupported continuation或挂起的函数调用无返回。Main通知原消息更新为已提交，再继续规划/答复；Task同理。
6. 问卷不是审批，不扩大授权；不收集密码/token等不必要敏感值；可选未答项不捏造。等待期间不创建依赖未知答案的任务。

调研工具应单独调用。Host在提交工具batch前拒绝混合调用；不自动回滚或重放其他工具的副作用。运行时取消及调研版本校验沿用既有Host契约；Main取消时原通知更新为已取消，不再提示填写已删除的开放问卷。

## 发布与Windows对齐

- Main schema新增独立Survey companion，原8个Task工具不变。main bundle包含新的on-demand binding、document与reference；激活仅解释权限，不增加权限。
- macOS首次bootstrap与离线restore迁移均发布新immutable capability revision，并补入当前survey schema。旧Run、旧冻结工具与Plugin版本不覆盖。
- 调研中心监听 `requirement_survey_waiting` 和 `requirement_survey_resolved`，避免创建后等待五分钟一致性刷新。
- Windows需同样发布Main Survey schema与Skill/catalog binding，保证restore产生新revision；消费waiting/resolved事件，显示Main通知、复用调研中心，提交后保持原Main Run。shared Host/planner逻辑共用，不另外实现团队专用存储或零散prompt。

## 验证

自动化覆盖Main与Task创建/暂停/提交/原Run恢复及重放、联系人拒绝、跨owner拒绝、混合batch拒绝、Main原通知更新与取消、两profile答案回填函数调用、catalog门控与immutable迁移。shared Rust三包206项通过（profiles 31、storage 90、Host 85）；macOS定向验证70项通过（XCTest 40、Swift Testing 30）；收尾重新验证会话15项及刷新策略30项，通过。新Main取消测试确认原通知ID和ordinal保留，状态改为cancelled，开放问卷不可再提交。debug app于 `2026-10-13T02:26:40Z`（北京时间10:26）完成重新打包。

## FinFlow实机验收（2026-10-13）

使用仓库debug app，在FinFlow Finance MVP项目主聊天发送明确标记为端到端测试的请求，要求直接创建两题问卷，先等待提交，提交后仅用一句话总结；禁止创建Task、项目读写、命令、插件和联网。

- 真实模型先激活 `product-skill:chatos-requirement-discovery`，再调用 `requirement_survey_create`；原Run `run_e97a00d8-6a0b-4032-99fd-703797742099` 在iteration 2进入 `waiting_user`。
- 主聊天出现待填写通知；工具栏“需求调研”可进入同一中心，项目卡片及详情出现“主聊天需求调研链路测试”。
- 实际提交的测试答案为“真实账务试运行”和“test”。它们仅为链路测试输入，不是真实财务请求或操作授权；未擅自改写已经提交的答案。
- 提交后原Run在iteration 3完成为 `succeeded`，`source_task_id`为空，原 `survey-status:<survey_id>` 消息更新为已提交。UI显示一句总结并明确“不代表真实账务操作授权”。没有另开Run或创建占位Task。
- 该Run ledger仅有Skill激活与问卷创建，两项都成功；Task数为0，没有文件、终端、插件或联网调用。项目9个文件SHA-256与验收前完全一致。
- 详情/列表的残留“仅任务”文案随后修正：来源区分主聊天与任务，状态为“已提交”，按钮为“提交并继续”。重新启动最新debug app后，UI已确认列表来源“主聊天”、详情“主聊天需求调研／已提交”，以及“原对话或任务已恢复执行”。这项文案更新不改变已提交答案或已完成Run。

这是一轮真实模型的创建/等待/提交/恢复验收，不是所有业务场景的调研质量评测。测试使用 `clients/macos/.build/ChatOS.app`，没有覆盖 `/Applications/ChatOS.app`；旧Run的immutable snapshot不变。

## 用户体验与方案闭环修正（2026-10-13）

- 宠物将开放调研与真正的ask-user表单区分：仅 `waiting_user` 的来源Run关联当前owner的开放调研时，route包含 `requirementSurveyID` 和项目ID，不伪造 `local-ask:` prompt。主聊天和Task两种来源共用这条规则。
- “打开并填写”优先导航到需求调研中心，通过当前owner重新读取准确问卷并校验项目；不进入任务图/Task执行过程。已删除或不可访问的问卷提示刷新，不静默跳到空任务图。精确读取支持不在最近200条列表中的问卷。
- 页面不展示Host、Run/Task内部ID或“已恢复”。问卷状态为“待填写／已提交”，统计也是“已提交”；提交仅表示答案保存成功，不表示方案完成。
- 详情新增“方案与后续回复”，只读取同owner、同会话、同profile/Task来源的准确Run最终用户回复，Markdown完整渲染；不展示请求历史、工具trace或内部错误。处理中、需补充输入、失败、暂停、取消、成功但无回复均单独呈现，不能伪报方案成功。
- 结果面板先订阅事件再读取，按来源Run过滤完成/失败/取消等状态变化。问卷中心消费终态事件，保持低频一致性刷新，不使用高频轮询。
- 主policy和调研Skill明确：通常提交后应形成具体方案，包括已确认目标/受众、范围与排除项、推荐方案与取舍、阶段产物/依赖、验收依据和待定决策；深度适配任务。规划请求先交付方案，未经实施授权不得创建任务；已授权实施则转为Task契约并在主回复中给出方案。明确要求仅一句测试总结时仍尊重该例外。
- 没有新增Task工具、模型方案写入工具或数据库表。方案是来源Run的持久化最终回复，中心负责投影显示；当前不把该回复转成独立版本化方案实体，不声称已实现方案审批/历史版本管理。Windows按同一route字段、来源校验、用户文案及结果投影对齐。

### 方案是否用于后续工作

当前方案作为原主聊天的assistant回复持久化，后续Main模型可通过该会话的历史上下文读取。编排Skill要求将确认目标、范围、产物和验收条件转为Task契约，worker使用冻结的Task输入，而不是自动读取调研中心页面。**当前只是上下文级复用，并没有 `proposal_id/revision`、确认状态或Task外键引用；长历史的上下文裁剪和模型遗漏仍可能丢失部分方案内容，不能称为确定性的执行绑定。**

若进一步实现强绑定，应独立保存方案正文/版本、来源问卷和确认事实；后续Main显式加载当前方案，Task冻结所引用版本及适用章节，重试沿用原版本，变更方案后重新确认受影响范围。这是后续改进项，本轮没有实现，不应仅用Skill文字替代。

### 本轮验证

macOS 69项定向测试通过（Core 7、Connector 32、Swift Testing性能/事件策略30），包含主聊天和Task宠物调研route、普通ask-user兼容、精确来源结果、越权/跨会话拒绝，以及处理中/失败/取消与完成的区别。UI中英文审计零遗漏，两个更新的Skill验证通过。最新版debug于 `2026-10-13T02:45:37Z` 重新打包，未覆盖installed app。

FinFlow真实模型方案轮：从主聊天发送仅创建问卷和生成方案的测试，题为“宠物调研与方案验收”；原Run为 `run_cd916013-09fe-4406-bf45-78029a59ba63`。实际答案为“凭证流程清晰／你自己决定”。提交后同Run在iteration 3成功，UI详情已直接显示完整需求方案，包含目标、范围、设计建议、阶段产物与验收要点，并明确仅虚构测试。该轮没有创建Task，ledger仅Skill激活和问卷创建，没有文件、命令、插件或联网业务调用。项目9文件SHA-256仍与验收前一致。

本轮过程中问卷已由界面操作提交，完成了方案展示实测；宠物按钮分支和准确survey route有自动化覆盖，但未将本代理未实际点击的宠物按钮表述为点击实测通过。

## 后续强绑定已实施（2026-10-13）

上文“只上下文级复用”的历史边界现已由程序级版本实体、human确认、每步独立上下文和Task冻结替代；不是仅修改Skill文字。原8个Task工具保留，另有survey/plan companion，不向模型提供确认工具。macOS已实测确认不执行、三个Task引用同一v1正文/哈希/契约及最终全文回写，项目文件不变；详见 [绑定契约](local-agent-requirement-plan-binding.md) 和 [本轮实机验收](local-agent-reliability-finflow-test.md)。Windows按新契约对齐。
