# 本地 Agent 可靠性修复清单

日期：2026-10-13。按用户要求逐项实施、验证；macOS 与 shared Host 在本次范围，Windows 的界面和 native executor 由 Windows 开发对齐。

## 任务 1：统一工具失败诊断与参数检查（已完成自动化验证）

- [x] Main Chat / Task 的失败 continuation 保留错误码和插件/MCP具体原因，不只返回 `native_tool_reported_error`。
- [x] 兼容 MCP 文本块、native 包装、普通文本、结构化诊断与缺失 detail；错误不投递图片或内部 trace。
- [x] Plugin 与外部 MCP 在真实调用前校验参数，并返回可纠正的字段信息；不自动扩充权限。
- [x] JSON-RPC错误详情不退化为通用远端失败；通信中断的未知副作用继续 `needs_review`。
- [x] 自动化回归覆盖成功、不合法参数、已知失败、未知副作用。

## 任务 2：调研方案可靠复用（已实现并验证）

- [x] 将调研后方案保存为有来源的版本化实体，而非仅聊天历史。
- [x] 由用户明确确认准确版本；新版本不能沿用旧确认。
- [x] 后续主聊天可靠加载方案；Task 引用已确认版本并冻结正文与适用范围。
- [x] 项目/owner/会话校验、重试沿用冻结版本、旧任务兼容与取消处理。
- [x] macOS调研详情显示方案、版本与确认状态，Task详情展示自身引用方案；更新统一 Skill 与 Windows 契约。
- [x] 自动化覆盖提交→生成→确认和Task冻结，以及未确认/跨owner/版本变更拒绝。

## 任务 3：集成与实机验收（已完成）

- [x] 定向Rust、macOS与浏览器插件测试，重新构建debug app。
- [x] 使用财务项目的虚构测试验证调研到纯文字任务，不操作真实账务，不重试历史音乐任务。
- [x] 记录实际测试结果及未覆盖边界，更新本清单。

## 任务 4：Task 内容验收与成功上报（已完成本轮验证）

- [x] 新 Task 冻结结构化验收版本；逐条记录证据，不通过/未核验不得报成功。
- [x] 显式最终文本长度限制由程序执行；交付采用已检查原文，防止检查后改写。
- [x] 评审自身完成与被评审材料通过分离；已有 Task/retry 保持旧契约。
- [x] Rust/macOS回归、统一 Skill 与 [Windows对齐契约](local-agent-task-acceptance-contract.md)。
- [x] 已成功跨图前置在创建时解除等待；调度恢复旧pending，按owner隔离，不重复执行历史前置。
- [x] 财务项目虚构纯文字新任务复测，记录内容和生命周期证据。
- [x] 修复启动恢复执行后，控制面刷新再次替换执行进程的中断风险；补齐租约失效的用户可见阻塞原因，并完成补充复核的端到端验收。

任务4追加验证：本轮补充复核于14:06:27真实开始、14:08:02完成；正式消费A378/B366、完整原文身份及400字符程序门禁，最终794 scalar，与接受报告原文相等，并保留“长度通过不等于内容全面合格”的边界。A/B/C各自仍只有1次运行、项目9文件路径及SHA-256不变。启动回填曾引入事件ID重复回归，现已修复并纳入CI；详细时间线及测试边界见[实机记录](local-agent-reliability-finflow-test.md)。

## 任务 5：启动回归与宠物取消失效（已修复并验证协议链路）

- [x] 回调ID不再由版本或内容摘要决定；真实消息变更有独立事件，重复回放无增量。
- [x] 旧中断尝试仅补诊断，不覆盖已成功或正在重试的Task消息。
- [x] blocked工作流可由用户取消，并级联取消未完成后续；原needs_review Run诊断不改写。
- [x] 取消后active投影不再返回该Task；宠物响应task_cancelled事件，取消错误直接在浮窗展示。
- [x] Rust 234项（storage99、Host87、integration5、application43）通过；macOS取消/进程/映射定向19 XCTest及宠物相关17 Swift Testing通过。
- [x] 在隔离真实历史数据库副本上，通过真实Host正式cancel_task、重复请求、active查询和health验证，Taskcancelled、Run逐字段不变、重复请求幂等。
- [x] CI先构建真实Host、跑storage回归，再强制运行原生生命周期测试；缺少二进制不允许静默skip。
- [x] 最终代码再次通过完整 `native-platform` 本机CI入口；5项可选环境测试仍skip，真实Host启动测试无skip。仓库debug构建 `2026-10-13T06:22:24Z` 已启动，联系人、财务会话与复核结果正常恢复。

没有替用户取消无法从截图确定身份的其他历史任务，也未伪称已经实机点击并取消了截图中的那一条。UI代码已修复并重新打包，取消持久化链路由隔离副本验证。

## 任务 6：连接凭据变更的误提示（已修复并实机验证）

- [x] 统一启动恢复与联网刷新的历史模型凭据加载。当前目录过滤历史模型不再被当作密钥变化；保留冻结Run的key，不扩充模型选项或能力权限。
- [x] 同owner、credentialRef、缺失/空白凭据与变量名碰撞检查；真正密钥变化、memory token变化/缺失、新模型key仍暂缓，不打断worker。
- [x] 定向8 XCTest及3 Swift Testing通过；4项CI政策检查通过；完整native-platform入口零失败，5项可选环境测试skip，真实Host生命周期无skip。
- [x] 仓库debug构建 `2026-10-13T06:58:49Z` 已启动，联网能力发布成功；财务会话与联系人正常，截图中的提示不再出现，资源刷新前后PID80303不变。
- [x] 财务项目9文件SHA-256仍不变。本次没有重跑财务任务或其他历史任务；记录与Windows规则已补充到对应文档。

## 前三项验证记录

任务7追加：六插件逐项源码/MCP及Skill门控验证、过期Skill工具名修复与Host兼容、Browser真实catalog的native CI、PPT可读文字/像素回归已完成；Browser/Computer Use真实界面操作及全量模型使用仍未完成。详细证据、Windows规则和当前13项源码行数门禁失败见[插件逐项验收](local-agent-plugin-usage-verification.md)。应用按用户要求只重装并启动 `/Applications/ChatOS.app`，不再并行运行仓库构建版。

- Rust相关包：345项通过（profiles 35、storage 94、Host unit 86、Host integration 5、application 42、MCP runtime 83）。包含方案版本/确认/项目隔离/输入上限/冻结/重试，以及错误详情真正进入模型 function output。
- macOS首组：119 XCTest +10 Swift Testing通过；补充事件、宠物恢复与刷新策略：2 XCTest +37 Swift Testing通过。所有选中测试零失败，不等同于全macOS测试套件。
- 浏览器MCP源码：9项通过。未更新已安装的浏览器插件release binary，也未重试旧音乐任务。
- UI中英文审计：0缺失、0非identity；统一4项Skill validator通过。
- debug app最后于 `2026-10-13T04:06:49Z` 打包，位于仓库 `.build/ChatOS.app`，不覆盖 `/Applications/ChatOS.app`。实机schema31→32成功，有迁移前backup，联系人与项目正常加载。
- 实机发现新save工具遗漏草稿审批分类，修复production assembly及Task local prefix，并补回归；不是仅测试手工传requires_approval=false。旧测试Run按正式取消命令停止，已提交答案保留，未更改其冻结审批；新Run的plan_save实际requires_approval=false且成功。
- 当前方案绑定的限制和Windows契约见 [需求方案绑定](local-agent-requirement-plan-binding.md)。
- FinFlow实机已完成v1生成/确认、确认零Task/Run增量、三个Task冻结与完整交付，9文件SHA不变。模型字数超限与文字设计缺口未算作内容验收通过；具体来源、事件、状态及边界见 [本轮实机验收](local-agent-reliability-finflow-test.md)。
