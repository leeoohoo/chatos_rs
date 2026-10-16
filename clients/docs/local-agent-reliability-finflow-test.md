# 通用工具与需求方案修复：FinFlow 实机验收

日期：2026-10-13。只使用仓库调试版 `clients/macos/.build/ChatOS.app`；前三项验收使用 `2026-10-13T04:06:49Z` 构建，补充复核完成使用 `2026-10-13T05:59:17Z` 构建，取消修复最后打包时间 `2026-10-13T06:22:24Z`。没有覆盖安装版。项目为 `FinFlow Finance MVP`，根目录 `/Users/lilei/project/finflow-finance-mvp`，不是绑定宽目录的另一个财务项目。

## 测试范围

虚构界面与文字产物测试，不操作真实账务。模型只能使用方案和会话信息，禁止项目读写、命令、业务插件、联网查询和报告落文件。测试人员通过只读SQLite查询及文件SHA-256核对实际状态，不以模型自述替代程序事实。

## 发现并修复的问题

首次新问卷提交后，模型正确调用 `requirement_plan_save`，但 production assembly 仅将 survey_create 分类为无需执行审批的本地草稿工具，遗漏 plan_save。实际调用停在 `pending / requires_approval=true`；没有保存或执行。这是组装层接线错误，不能用手工传requires_approval=false的单元测试掩盖。

已修复统一分类，并补上Task local tool prefix与production safety回归。新save仍是side-effecting草稿写入，不是read-only；文件写入、命令和未知工具的执行审批保持不变。human方案确认与实施授权是独立检查。

旧测试Run `run_29e34704-4ff5-48da-8530-272561529674` 通过正式 `cancel_conversation_turn` 停止，状态为cancelled；原问卷 `local-survey-7b8fcbaf-141c-4941-96a3-2429a513c12d` 保持resolved，答案保留。未直接写DB、补批旧调用或重写冻结审批。随后在更新的构建中启动新的方案生成Run复测。

## 方案保存与确认

- 新方案来源Run：`run_f276c1db-cc6a-4dd0-b2e7-9fb740c592f3`，最终succeeded。
- 原问卷答案为“凭证流程清晰”，交付偏好为“浅色、表格优先，A状态/异常矩阵、B角色/可访问性说明，最后综合；只用虚构文字”。新Run使用这些已提交答案，不捏造或改写它们。
- 真实调用ledger只有Skill激活、方案参考读取与plan_save；save实际 `requires_approval=false / approval_status=not_required / succeeded`。
- 生成 v1，515个正文字符，有sourceRun、sourceSurvey与项目来源，confirmed_at为空。调研详情实机显示Markdown、版本1、等待确认与“确认当前方案”。来源旧Run已取消，界面仍准确读取**新方案的生成Run**，没有错误沿用旧Run终态。
- 点击确认后，UI变为“方案已确认”。Task总数前后均83，Run总数前后均189；确认没有创建任务或Run。

## 后续任务冻结与交付

明确授权纯文字任务后启动Main Run `run_8622b377-4777-4c5f-a92a-fc1b919e94ad`，程序绑定已确认v1。模型创建graph `local-task-graph-98a7707f-8669-497c-9fc1-960024d07e08`，Task后缀分别为-1、-2、-3：

| Task | 实际开始（北京时间） | 完成 | 前置 |
| --- | --- | --- | --- |
| A：凭证状态与异常提示矩阵 | 12:19:39 | 12:20:32 | 无 |
| B：角色操作与可访问性检查 | 12:20:32 | 12:21:06 | 无 |
| 综合：A、B覆盖性结论 | 12:21:06 | 12:23:01 | A与B |

- Main及3个Task均succeeded。Task数量增加3，Run增加4（Main+3个worker），不是确认动作触发。
- 每个Task输入及其Run输入的方案revision均1、完整正文与DB v1逐字相等、SHA-256核对相等、confirmed_at一致；task_scope含各自预期输出和验收标准。
- `requires_execution=false`，plugin_hints与external_mcp_config_ids为空；仅选AskUser基础能力，没有文件、终端、插件执行能力。worker ledger仅过程记录和outcome report，全部成功，没有业务工具调用。
- durable `task.run.started` 回写3次、`task.completed` 回写3次；创建时不批量虚报开始，完成更新原状态消息。UI实际观察到A开始、A完成后B开始，以及最终三个完成状态。
- Task详情展开“引用的需求方案”，实机展示v1全文及“创建任务时固定，重试仍使用此版本”。不是从当前项目方案替换旧Task展示。
- 单条最终graph消息 `task-graph:local-task-graph-98a7707f-8669-497c-9fc1-960024d07e08:1`，ordinal35，位于Main交接31及三条状态消息32–34之后。delivery_text为3548字符，与综合Task准确final content相等，UI实际可见原文、逐项覆盖和缺口、边界及结论。
- 前后9个财务项目文件的路径集合与SHA-256完全相同，包括已有pyc。未重试旧音乐Task、未访问音乐网站、未修改真实账务。

## 验收结论与边界

**功能链路通过**：已提交答案→来源受校验的方案版本→用户确认且不执行→新Main绑定→3Task冻结正文/版本/范围→实际开始/完成通知→完整最终回写。

**不能宣称文字内容全部达标**：A、B原文分别438、442字符；按完整原文去掉换行的口径为424、425，超过400。综合Task如实指出字数限制未达标、状态命名不一致和异常恢复/详情记录缺口，没有代改原文伪造通过。这是本次生成材料的质量保留项，不是界面或财务功能已实测的证据。

版本变更重新确认、旧Task重试保留v1、未确认拒绝、跨owner/项目拒绝、输入上限及失败生成不可确认主要由自动化覆盖；本实机轮没有再改v2或重跑旧任务，不倒算为v2实机成功。

通用Plugin/MCP参数与错误链路有离线模拟/原生fixture回归，浏览器源码9项测试通过，但未逐一实测所有第三方插件业务，也未发布其新release binary。当前worker实际串行调度，因此A、B无前置仍依次开始；此次未实现并行模型池或公平调度优化。

Windows实现契约见 [需求方案绑定](local-agent-requirement-plan-binding.md)，完整任务清单与回归统计见 [修复清单](local-agent-reliability-repair-checklist.md)。

## 追加：内容验收门禁复测

首个门禁构建于 `2026-10-13T04:43:14Z` 打包，正式Main输入于北京时间12:45提交。仅虚构纯文字，不修改或重试上一轮Task。

- Main：`run_c25da100-2cb3-468e-8f94-cf31d80fde22`，succeeded；先激活统一编排Skill，读取四个相关参考，再创建graph并正常handoff。
- Graph：`local-task-graph-2b3e1547-7d2c-4873-8ba6-339aff905609`；A/B无前置，C依赖两者，`progress_summary`，C为最终交付。
- 新Task全部冻结 `acceptance_report_version=1`、方案v1；A/B还冻结 `final_response_max_chars=400`，并非从验收自然语言猜测。
- A Run：`task-run-ed848237-d57f-46b0-8d9b-685a361749aa`；最终378 Unicode scalar。B Run：`task-run-b81b3e17-74e6-4560-beb7-502cec346cd9`；最终366。含所有标题、Markdown、标点、空格、换行，均≤400。
- C Run：`task-run-3f8d14f2-ee13-4130-a1c1-14ef5f60a129`；最终4772 scalar，无400限制；12:53:02完成。本轮3Task均succeeded，接受上报一次，逐项证据齐备；三份最终content逐字等于各自accepted report的final_response。
- ledger仅过程记录及outcome report。3次真实开始+3次完成回调，UI实见C“已开始”再完成。Graph完整最终消息ordinal41，未只回一个短完成提示。

这次长度问题已通过程序门禁，但**不代表文字设计全合格**。C仍如实指出B独立校验/退回原因/失败不报成功等表述及列表下一操作缺口，没有因Task成功而宣称材料全通过。机械门禁校验结构、coverage及长度；不独立证明worker所有自然语言证据为真。

实机还发现A在evidence里手算386，而实际原文为378；C只收到标为结果摘要的前置文本，没有可信长度回执，因此正确标“长度未验证”。随后补充：上报工具返回实际scalar count；Host前置传递从已交付全文重新计算 `acceptance_summary`，附计数、冻结上限、长度结果、摘要是否完整原文，并区分worker内容声明与程序事实。此补丁单独做回归及后续实机复核，不倒算为首轮C已收到该事实。

## 追加：历史前置解锁、回调与复核中断诊断

- 补充Main `run_7cb1fd5b-98a3-4961-b7a1-44ccfdfad47f` 创建一个新复核Task `local-task-133f65f3-7fb5-4f6d-93bf-6d390d2989ab-1`，正式跨图引用已完成A/B。不重试或改写旧A/B/C。
- 首次一直pending的原因是创建路径将有外部前置的Task置pending，却只在前置后续终态事件解锁；调度fast-path又仅检测ready。历史前置早已成功，没有新终态事件，因此永久等待。已修复创建事务与调度恢复，并补owner隔离回归。
- 最新构建启动后原Task自动执行，未重建或手工写DB。Run `task-run-0995eed8-206e-47dd-9373-996ca4be00bd` 的冻结输入实含A=378、B=366，上限均400、程序检查通过且前置结果为完整最终原文。只能证明可信事实已传入，不能证明模型完成消费和最终交付。
- 北京时间13:06:10.341记录真实开始回调。13:11:10.345记录 `claim_expired_needs_review`，Run变needs_review、Task变blocked，无工具调用、accepted report或最终产物。开始/阻塞各回调一次，更新同一状态消息。
- 只读进程检查显示当前Host从13:06:13启动，晚于Task被领取。启动代码先 `restoreLocalAgentHost` 重启带凭据的worker，再刷新控制面；`bootstrapLocalAgentHost` 再次无条件 `host.restart`，`launch` 会停止旧进程。这是与本轮3秒内进程替换相符的代码路径；未捕获逐次restart调用日志，不将推断冒充完整调用trace。此风险尚未修复。
- 缺少具体原因也是代码问题：租约恢复只把原因写进event payload，未写terminal outcome；会话回调只取worker report或terminal outcome，不读取该恢复事件，因而仅显示“我暂时还无法继续处理。”，没有中断原因。该问题尚未修复，不声称本轮复核成功。
- 前两个Task上一轮真实开始/完成均已回写：A 12:47:18开始、12:48:34完成；B 12:48:34开始、12:49:43完成。`progress_summary`完成更新原状态消息，因此回看只见完成摘要；本轮作为历史前置不重新执行，也不再次回调开始。未改成两条独立开始/完成消息。
- 本次结束再次核对财务项目9文件路径集合及SHA-256，全部不变；未重试音乐任务、访问音乐网站、操作真实账务或直接更改任务状态。新增复核最终交付仍待验证。

## 追加：启动修复、CI与复核完成

以下为后续验证，不覆盖上面13:11中断时的事实。

- 启动原因回填曾触发旧回调事件ID重复（`UNIQUE constraint failed: local_agent_events.event_id`），导致Host初始化退出、macOS误呈现invalid frame/repeated exit、资源未加载。这是本轮补丁引入的回归，不是联系人被删除。旧debug Host在只读备份副本上可复现退出1、stdout无健康响应。
- 修复后的回调用写事务中的内容/metadata比较判定幂等，有真实变更才生成UUID事件；同一内容再次恢复仍正常，完全相同的重复回放无新增事件。不忽略UNIQUE错误。旧Run诊断仅在Task仍blocked且该Run是最新尝试时修复回调，已成功重试的Task消息保持不变。
- 相同owner/凭据环境的控制面刷新复用workerPID；真正凭据更新暂缓并明确提示，不无条件替换正在执行的进程。macOS启动旧诊断构建、最终修复构建严格区分，没有让旧失败构建算作通过。
- `2026-10-13T05:59:17Z` 仓库debug构建启动后，实机恢复叽咕狸、项目及财务会话；任务详情显示中文中断原因。旧已成功重试Task `local-task-92d2617b-94ae-4493-9d41-712ea17c8a07-1`仍succeeded、version16，未被旧needs_review原因回填覆盖。
- 本轮纯文字复核旧尝试无业务工具调用/接受报告。正式重试仅此Task，使用Host `retry_task`命令，不直接修改数据库状态、不重跑A/B/C。UI第一次操作被宠物浮窗焦点干扰，没有将未生效的点击算作重试成功。
- 新Run `task-run-1f553bd1-1400-4957-b19a-c42f288e9634`：北京时间14:06:27真实开始，14:08:02完成。每个新Run开始/完成回调各1次；worker仅调用2次过程记录和1次接受的outcome report，无项目/联网/业务工具。
- 原方案v1与验收标准保留；实际消费完整A/B前置事实A378、B366、上限400、完整原文身份true和长度检查true。最终794 Unicode scalar，与接受报告final_response逐字相等；5项验收检查均有证据。会话中可见已完成卡片，展开任务详情实见复核原文。
- 新交付明确“长度通过不等于内容全部通过”，保留上一评审缺口；因本轮输入没有缺口清单，没有虚构细目。A/B/C各自仍1个Run；执行期间workerPID71490保持，未再次被配置刷新替换。
- 再次核对财务项目9文件路径集合及SHA-256，全部不变。未重试音乐Task、访问音乐网站或操作真实账务。

CI增加macOS真实Rust Host构建和storage回归，强制原生生命周期测试，不允许缺二进制时XCTSkip。`bash scripts/verify-repository.sh native-platform`在本机跑通，包括macOS与Computer Use插件测试；可选数据量基线、文档插件实机、评测导出和已安装浏览器路径等5项仍按环境条件skip，真实Host生命周期没有skip，不能把这些可选项说成实测通过。另有3项CI政策检查、中文/英文界面审计通过。修复后的Rust相关234项、macOS取消/诊断相关19 XCTest和17 Swift Testing通过。

## 追加：宠物取消按钮无效

浮窗对needs_review显示取消，但shared Host原先拒绝blocked Task；返回错误仅在另一任务列表展示，当前浮窗没有错误反馈。已统一修复：允许用户取消blocked工作流、级联取消未完成后续，保留原未知Run与诊断；active投影移除已取消Task，发布task_cancelled驱动刷新。取消回调采用任务实际取消时间，不沿用旧Run中断时间；失败直接在当前浮窗显示。

真实Host在隔离历史副本 `/tmp/chatos-fixed-startup.qnkB5G/agent.sqlite3` 上通过正式cancel_task验证：目标Task变cancelled、旧needs_review Run逐字段不变、同command重复请求无增量、active查询不含该Task、health正常。另有owner/版本冲突、级联、无重新启动、幂等及回调时间的storage回归。本测试没有替用户取消截图身份不明确的其他历史任务；其UI取消需使用更新后的仓库debug构建，未把隔离副本测试冒充对原任务的实机点击验收。

## 追加：连接凭据误提示

上一轮“不打断worker”的保护误将目录集合差异视为密钥变化：启动恢复读取全部持久模型，联网刷新只读取当前启用模型。现场数据库保留18个历史model refs。已统一装配：两条路径先读取同owner、credentialRef匹配且有效的历史凭据，联网目录权威值覆盖同key；不将历史模型加入联网后的可选目录，也不扩充能力。缺失/空白凭据忽略，变量名碰撞拒绝，不输出凭据原文。

本轮新增loader首先遇到跨文件访问private字符串helper的编译错误；已改为本文件显式trim检查，随后才执行验证及打包，不将旧构建的成功倒算为本补丁通过。

- 定向8 XCTest（真实Host生命周期）及3 Swift Testing（凭据集合）零失败；模型轮换、memory token轮换/缺失及新增模型key均deferred，PID不变；同集合连续3次刷新复用PID，联系人与默认会话不重复。
- 4项CI政策检查及完整 `scripts/verify-repository.sh native-platform` 通过。storage99项零失败；macOS与Computer Use测试零失败，5项可选环境测试仍skip，真实Host生命周期无skip。日志：`/tmp/chatos-credential-catalog-swift.log`、`/tmp/chatos-credential-catalog-native-ci.log`。
- 确认原数据库没有未完成运行后正常退出仓库测试版，打包 `2026-10-13T06:58:49Z` debug app，未覆盖安装版。新应用启动后能力快照于UTC06:59:40发布，说明联网刷新已通过凭据检查，不是仅启动早期尚未出现错误。
- UI实际可见叽咕狸、财务项目会话及14:06已完成复核卡片；红框连接凭据提示不再出现。点击刷新资源后仍无提示，Host PID80303保持。资源刷新不等同于强制再次刷新模型目录；重复凭据刷新另由真实Host回归覆盖。
- 财务项目9文件SHA-256全部保持。没有重跑任务、修改账务、访问音乐网站或使用项目业务工具。本轮是配置装配与会话恢复验证，不冒充新的财务内容验收。

真正新key、密钥变化及memory token变化仍要求正常重开应用；当前不支持无损热替换，不通过删除提示或忽略token变化绕过保护。Windows原生实现按[配置刷新契约](local-agent-task-acceptance-contract.md)对齐。
