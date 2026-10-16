# FinFlow 实机编排验收（macOS）

日期：2026-10-12、2026-10-13。本文记录实际 ChatOS UI、真实配置模型与 native Task runner 的测试，不是离线编排回放；保留各轮当时的结果与局限，不把后续修复倒算到旧轮次。

## 项目与安全边界

- 测试项目：ChatOS 本地项目 `FinFlow Finance MVP`，路径 `/Users/lilei/project/finflow-finance-mvp`。
- 不使用另一个 `FinFlow 财务管理系统` 项目；它绑定整个 `/Users/lilei/project`，范围过宽。
- 项目 README 明确使用 Python 标准库、SQLite 与虚构数据，不用于真实财务或合规申报。
- 不修改项目源码、测试、缓存或已有数据库，不联网、不安装依赖、不操作真实账务。动态验证仅允许 Python 禁字节码启动与新建系统临时目录中的 SQLite。
- 测试前记录全部 9 个现有文件的 SHA-256，包含已有 `__pycache__`；测试结束需要核对文件集合与内容均未改变。
- 应用正常使用既有模型配置；测试人员没有导出凭证、读取 Keychain 或将应用凭证写进报告。

## 用户需求

通过当前项目的 Main Chat 提交以下需求；不指定 Task 数量或工具调用顺序：

> 请对当前 FinFlow Finance MVP 做一次综合验收，交付三份有独立依据的结果：①核查凭证录入→提交→审核→过账流程、整数分金额校验、幂等与审计，指出实现缺口；②核查账套隔离、角色权限、总账及资产负债表/利润表口径，给出源码依据与风险；③实际运行现有测试，并用虚构数据验证一笔完整过账及失败请求不留部分数据。最后综合这三份结果，给出能否进入下一开发阶段的结论与优先级建议。此次是测试：不要修改、创建或删除项目文件，不访问网络，不操作真实账务；允许运行 Python 标准库测试，但必须禁止生成项目内 pyc，SQLite 数据库只允许放在新建的系统临时目录中。请如实区分静态检查与实际执行，不要因为 README 写着骨架就跳过源码。结果直接回复会话，不落文件。

## 验收依据

1. Main Chat 的 durable tool ledger 证明先激活编排 Skill，完整读取适用 references，再创建 Task。
2. 实际 Task 图、契约、能力选择及 dependency 表证明有意义的多任务分工与最终汇总依赖，不以回复中声称“已安排”作为创建依据。
3. Task 的 native 文件与终端调用、实际进程输出证明业务执行；终态及源会话回写证明完成交付。
4. 全部项目文件前后 SHA-256 与文件集合一致；测试数据库只能属于本轮新建临时目录。
5. 区分 Task 执行成功与财务产品业务验收通过。发现财务缺口也可以是成功完成验收，不能宣称产品全部正确。

## 执行记录

### 已安装版首轮

- 应用构建时间：`2026-10-12T08:49:13Z`（北京时间 16:49）；早于最后一批准备/写入错误边界修复，不能作为最新代码完整验收。
- Main Run：`run_7bac03a4-5079-4bb0-85e1-5a2126cb489f`。
- Source turn：`turn_4385cbdc-88b3-430b-8e76-d77c8646552b`。
- Graph：`local-task-graph-b846c316-3dfc-4490-8e53-e82b93838f5f`。
- Main 成功激活 `product-skill:chatos-async-task-orchestration`；完整读取 followup、capabilities、software、graph、complexity 五个 reference，返回字符数分别为 1182、1968、1126、3020、1334，均等于总字符数；阅读所在 batch 早于创建图所在 batch。
- 创建 4 个实际 Task：流程/完整性静态审查、隔离/权限/报表静态审查、隔离动态验证、最终综合。最终综合有指向前三项的三条 blocking prerequisite。
- 四项均有非空预期输出和验收标准；只有动态验证选择 `TerminalController`，所有节点均未选择 `CodeMaintainerWrite`、Plugin 或 External MCP。policy 自动加入的 `AskUser` 不算模型主动扩权。
- Main 仅调用一次 `wait_for_task_completion`，交接时没有冒称业务验收完成。
- 测试人员独立运行 `python3 -B -m unittest discover -s tests -v`：4 项排序用例通过。这只是独立基线，不能替代 Task 内动态执行证据。
- 两项静态审查及动态验证已成功；动态进程退出码0、`success=true`、`truncated=false`。实际执行原有4项 unittest（失败/错误/跳过均0），以及独立构造的完整过账、相同创建/过账键重放、不同键重复过账拒绝、幂等冲突、借贷不平衡拒绝、创建中途及过账中途失败回滚验证。
- 回滚验证在临时数据库给 `requests` 插入添加故障触发器，事务内探针证明凭证/分录/审计或过账状态已改变，再比较失败前后全部12个表/视图的行值及总账，确认无部分提交。不是只测试前置校验失败。
- 动态脚本自检项目写入、禁止数据库、socket、子进程及 SQL ATTACH 都被拒；原样源码编译为内存模块，未改业务函数；SQLite connection wrapper 加 authorizer、事务跟踪和内存探针。此适配与普通启动路径的差异已在 Task 报告披露。
- 临时数据库：`/private/var/folders/kd/cq1bxxgd3rg8t8zbwqgprxz80000gn/T/finflow-verify-3yj8z72i/fictional.sqlite3`，测试后保留，不包含真实账务。
- Task 内核对13个文件/目录条目的内容、大小及 mtime 均不变；项目快照摘要为 `0745598ad58e9c63fac88b0c7ae1df3285997f0d9890071d04bf0a545afd40f2`。
- 四项 Task 全部 `succeeded`；最终综合实际消费三份上游证据，区分“内部增量开发有条件进入”与“真实账务/生产不可进入”。源会话有且仅有 generation 1 的 `task_graph_terminal` 聚合回写，UI 显示全部4项完成；完整报告可从任务图节点详情查看。
- 交付局限：`graph_summary` 会话正文目前是每节点简述，最终综合也被截成简述；它不是完整的最终综合正文。若验收要求最终报告无需打开节点详情即可阅读全文，这一项尚未满足，后续应优化最终交付展示，不能把“已聚合回写”表述为“全文已回会话”。

### 最新代码构建

- `clients/macos/scripts/package-debug-app.sh` 成功；修复本轮发现的问题后重新构建，时间 `2026-10-12T10:07:07Z`。
- 构建产物：`clients/macos/.build/ChatOS.app`；安装目录暂不覆盖。
- 正常退出已安装版后，通过 UI 启动本仓库构建产物；实际 Swift 与 Rust Host 进程均来自 `clients/macos/.build/ChatOS.app/Contents/MacOS/`，没有覆盖 `/Applications/ChatOS.app`。
- 最新烟测 Main Run：`run_07c446ab-25a6-4798-8174-3b83239f4ad8`，source turn：`turn_2b63aed1-0502-4068-be47-27f01d042ded`。
- 新 Main 使用 revision `local-agent-f9aa0485d5dc1cf7f95e4ee6080bfa168cac273e0917de4a09c3b896dffcb796`；首轮保留 `local-agent-64f5c2b75cd7381df84ab4aeea799a62490ae5a54af46c42b32e9d4a2461b98c`，没有改写旧 Run 的冻结选择。
- 烟测范围：仅重跑4项原有测试、一笔完整虚构过账及创建中途故障回滚；提供了明确的标准库测试脚本，以验证真实 Main→Skill→Task→native Terminal→SQLite→回写路径，不把这一轮当作无提示拆分质量评测。
- 新 Main 成功激活编排 Skill，完整读取 capabilities/software/followup reference 后创建一个 Task，最后仅一次 handoff。单 Task 有明确预期输出和验收标准，没有 `single_task_rationale`。
- Task：`local-task-d69456e7-e447-4950-aa5f-db706eb79b8c-1`；Task Run：`task-run-ff0c776f-6387-4562-85a0-ec13ca2a5242`。选择 read/terminal，无 write/Plugin/External。
- Task 成功调用 capability search/describe，激活 project-files、project-read、terminal、terminal-command-execution 四个产品 Skill，再调用 native `execute_command`。
- 唯一一次终端调用与 Task 保存的指定脚本逐字相同，只提交 `command`，没有提交 `common`。沿用原有单次审批机制，未改全局权限或会话审批策略；结果 `exit_code=0`、`success=true`。
- 实际原始输出：

```text
Ran 4 tests in 0.207s
OK
LATEST_SMOKE_PASS existing_tests=4 states=draft->submitted->approved->posted mid_create_rollback=True
```

- 本轮数据库位于新建目录 `finflow-latest-smoke-68wcd_5y` 内的 `fictional.sqlite3`；与首轮数据库不同，未重用旧测试结果。
- Main 和 Task Run 均 `succeeded`；源会话仅一条 `task_execution_callback` 承载该 Task 的更新，UI 显示“已完成”并可查看本轮结果。结果明确基于此次进程输出，未引用上轮当作本轮执行证据。
- 两轮结束后，测试人员再次独立核对全部9个项目文件的路径集合与 SHA-256，与首次发送消息前完全一致，包括已有 pyc。未修复或改写 FinFlow 源码。

### 本轮发现并修复的终端参数歧义

首轮动态 Task 同时填写了完整脚本 `command` 和中文摘要 `common`。旧 native resolver 优先选 `common`，因此审批展示和实际执行都是摘要；第一次命令返回127，没有启动 Python或写数据库。模型在下一调用清空 `common` 后成功执行。这不是“审批与执行使用了不同脚本”的漏洞，而是兼容字段语义和优先级不清。

macOS 修复：

- 新模型 schema 只发布必填 `command`，说明必须是完整 shell 命令/脚本而非自然语言摘要，不再发布 `common`。
- shared native resolver 优先非空 `command`；仅缺失/空白时兼容旧 `common`。审批及两条 native 执行路径仍共用同一个 resolver。
- 不覆盖旧 Run 的冻结 schema；新 bootstrap 的实际工具 schema 变化会形成新 capability revision。
- 增加两个 JSON DTO 的 canonical 优先、legacy-only/空白 fallback、schema 不再发布 alias 等回归断言。worker suite 25项、catalog suite 8项、approval suite 10项（共43项）全部通过；重新打包应用通过。

Windows 对齐要求：也应只向新模型发布明确的 canonical 命令字段，历史字段只在 executor 兼容层接收；审批与执行必须使用相同解析逻辑。本次没有修改 Windows 实现。

## 结果边界

本次已完成两轮真实应用验证：已安装版复杂图的4个节点完成并单次聚合回写；最新构建的确定性业务烟测完成完整执行和回写，终端字段修复也通过实机复测。它证明这些具体路径和有限财务断言通过，不证明所有用户场景的自动拆分质量，也不证明 FinFlow 可用于真实账务。

上述2026-10-12两轮未满足或未覆盖：最终综合全文在主会话直接展示；本次没有测试业务源码修改、owned-path 并发写入、Plugin/External、followup 取消/替代和异常进程重启（这些只具有既有自动化回归证据）；Windows、50场景真实模型全量评测、插件平台管理页职责迁移均不属于本次单项目结果。后续改进另列，不改写旧测试事实。

当前运行的是仓库内 debug 测试构建；`/Applications/ChatOS.app` 保留原安装版，未部署本轮修复。后续正式部署需按正常发布流程进行。

## 2026-10-13 进度/完整交付改进复测

### 初始化回归与修复

第一次打包将shared protocol改为42但遗漏mac客户端常量（仍41），真实启动握手失败，UI出现 `Local Agent workspace is not configured for an authenticated account`，联系人“叽咕狸”未加载。不是数据删除。已将 `NativeLocalAgentHostProtocol.version` 同步42并重新打包；构建时间 `2026-10-13T01:19:45Z`。真实Host启动/健康检查/账户切换等7项生命周期测试通过。重新启动后联系人、原项目及会话历史恢复、同步错误消失。此次排查未重建数据库、未清理用户数据。

### 自动化验证

- shared Rust五个包全套：30＋90＋81＋33＋42＝276项通过。
- mac会话展示14项、真实Host生命周期7项（合计21项）通过，包括大于16000字的完整报告、Markdown/代码/尾部证据保留、优先 `delivery_text`、旧消息兼容，以及进度/最终交付/重试generation的持久化顺序。
- Skill catalog7项、immutable migration8项通过；eval export测试未设置输出环境变量，1项跳过，不能计入通过。
- 编排Skill `quick_validate.py` 通过。
- 四节点数据库链路覆盖创建/排队0开始、actual claim可见、同消息结束更新、4次start、综合依赖时机、完整结果与单次终态写回、迟到start不降级。另覆盖失败综合fallback、用户重试新Run/新generation、queued取消不伪造开始、无综合节点全文交付。

### 四 Task 实机复测（已完成）

- 当前只运行仓库内最新debug构建，不覆盖安装目录。
- 使用同一FinFlow Finance MVP项目，发送三份独立验收报告＋最终综合的业务需求，没有指定节点数量、notification policy或工具调用顺序。
- Main Run：`run_5772f450-6da6-48c1-9099-bde84084bd0f`；turn：`turn_3732b6b7-6144-4a6e-8920-fbe4e986c014`。
- 新immutable capability revision：`local-agent-54867069b7e4308bf4bde3d1bda58c4682e98ed8daab553ac1cdf87cf64f1679`。
- 业务执行使用 `2026-10-13T01:19:45Z` 构建。Main完整读取graph、complexity、software、capabilities、followup五份参考后创建图，选择 `progress_summary`、`final_delivery_ref=synthesis`，只handoff一次；没有在交接时冒称验收完成。
- Graph：`local-task-graph-33e55deb-c143-4eec-a799-a97e9ee2ad7c`。四项Task ID为 `local-task-33e55deb-c143-4eec-a799-a97e9ee2ad7c-1` 至 `-4`；前三项分别独立静态审查流程/完整性、静态审查隔离/报表、隔离动态执行；第四项综合直接依赖前三项且标记 `final_delivery=true`。Main及四项均 `succeeded`。
- durable事件有4次实际开始、4次结束；会话有4条状态消息，结束更新原消息且保留开始时间，没有创建时批量虚报开始。时间如下（北京时间）：

| Task | 实际开始 | 完成 |
| --- | --- | --- |
| 1：流程与完整性 | 09:23:34 | 09:26:53 |
| 2：隔离权限与报表 | 09:23:41 | 09:29:01 |
| 3：隔离动态验证 | 09:29:01 | 09:34:55 |
| 4：最终综合 | 09:34:55 | 09:38:40 |

- 唯一native终端调用获本机用户正常批准（`approval_decided_by=macos-local-approval`），返回 `success=true`、`exit_code=0`、`truncated=false`。未改变审批设置，也不是AI自动批准。
- Python 3.14.2以 `-B -I -S` 执行；原4项unittest通过，失败/错误/跳过均0。虚构123456分凭证完成 `draft→submitted→approved→posted`，平衡表断言通过；四阶段同key重放、不平衡、幂等冲突与旧version重复post的全库快照断言通过。
- create/post两项故障在 `requests` 持久化前注入；事务内探针先确认业务写入已发生，回滚后比较全部12个表/视图及 `sqlite_sequence` 恢复一致，输出 `ALL_DYNAMIC_CHECKS_PASS`。项目外写入、项目写入/删除/重命名、其他数据库、socket与子进程负向探针被拒；这是进程内守卫，不宣称为操作系统沙箱。
- 本轮数据库：`/private/tmp/finflow-current-77708273d4ed66664b6036efddbf6212/validation.sqlite3`。仅含虚构数据，测试后保留，不删除任何旧数据。
- 会话只有generation 1的一条最终交付，message ID为 `task-graph:local-task-graph-33e55deb-c143-4eec-a799-a97e9ee2ad7c:1`。`delivery_text` 9312字符，与第四Task的完整终态正文相等；旧 `summary` 761字符仅作兼容预览。UI实见报告全文尾部及“允许继续内部开发和受控虚构验证；真实账务试运行暂不准入”的结论，不需打开节点详情。
- 全部Task结束后再次独立核对9个项目文件（包括已有pyc），路径集合与SHA-256均与发送需求前完全相同。没有修改FinFlow源码或落入新的项目文件。

### 后续 Skill / 展示修复及验证边界

- 用户补充“可能修改同一文件最好不要并行”后，编排Skill入口、图参考及软件工程参考加入潜在写入集合检查、串行前置或唯一写入owner；包括不同函数/段落/单元格/图层、格式化、生成文件、lockfile等隐含写入。不把runtime锁当作规划替代，也不人为串行明确独立的只读任务。
- UI发现graph最终报告被旧排序提升到进度消息前，现改为Main交接确认优先，随后进度与graph交付保留Host持久化ordinal顺序；重试的新generation也不越过新Run进度。新增顺序回归测试通过。
- 最新打包时间 `2026-10-13T01:47:45Z`，包含上述Skill与排序补充；仍只重建仓库内 `.build/ChatOS.app`，没有覆盖 `/Applications/ChatOS.app`。本次后续构建只重载历史验证展示，不重新运行财务业务，不声称实测过并发写冲突场景。
- 最新构建重载后，UI恢复“叽咕狸”、FinFlow项目与原会话，无workspace初始化错误；完整可访问性树确认4条Task状态均位于本轮最终全文之前，全文尾部结论保留。没有通过改写旧消息或删数据来修正顺序。
- 无前置第三Task的等待原因另有边界：已有queued Run却未claim，旧调度器逐步串行await模型/Host工具且优先早创建Run，非写锁阻塞。此次未实现worker公平调度或模型并发池，不能宣称该排队问题已解决。
