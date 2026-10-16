# 插件工具渐进披露与截断修复（2026-10-14）

## 真实失败链路

任务 `local-task-375baf44-48ad-45a8-8997-0c6b9b916bf4-1`、Run `task-run-3578903e-4383-4fb1-828e-7462a35d25ce` 已绑定 Browser 0.1.16。`capability_describe` 一次返回 39 个完整工具定义；外层结果约 2.8 万字符，被共享 MCP runtime 裁成 1.6 万字符，记录中出现 `truncated 11911 chars`。`browser_session_open`、`browser_navigate`、`browser_snapshot` 的完整定义不在模型输入中。

模型已激活 router/navigation/observation，却调用 `tool_1`（原始 `browser_cdp_attach`）并传空 `tab_id`，被 network-debugging Skill gate 拦截。没有成功执行浏览器打开或网页导航，不能归因为目标网页超时。随后模型报告 blocked。此前渐进披露只覆盖插件选择和 Skill 正文/参考资料，没有覆盖单工具 schema。

## 统一契约

- 保留 `capability_search`、`capability_describe`、`capability_invoke` 及三个 `agent_skill_*` 名称；不增加 Task 业务工具或扩大权限。
- 默认 describe 返回 `disclosure_version=2`、`view=catalog`，只含短 Skill 目录和工具简介，不含 input schema 或可执行 gate。支持 query（工具名/关键词）、offset 和 limit；默认 8 条、最多 20 条，并在完整 JSON 条目边界按响应预算分页。`next_offset=null` 表示该筛选目录已完整返回。
- 同一 Run 的全部工具先生成稳定 option，之后才筛选/分页；翻页或 query 不改变 option 对应关系。
- 目录、单工具详情与 schema 分页是三种不同的请求。目录请求不传 tool_option/schema_offset（或显式 null）；详情请求不传 query/schema_offset（或 null）；只有工具明确返回分页后才传 schema_offset。可选字段的 schema 允许 null，运行器工具显式 strict=false，避免 Responses 自动严格化将未选中的字段变成必须填写的占位参数；不会把任意假 token 或 schema_offset=0 容错解释为目录。
- 选中工具后用同名 describe 加 `tool_option`，得到 `view=tool`、该工具完整 schema、effects、required refs 和条件 gate。目录不是调用依据。Provider 拒绝只读取目录、没读取选中工具定义的 Plugin 调用。
- 每页完整编码后的响应不超过 8000 Unicode scalar，低于当前 MCP 16000 和模型请求 12000 的常规单条预算。参数 schema 不通过字符串头尾裁剪控制大小。
- 超大单工具 schema 明确返回 `schema_delivery=paged`、SHA256、总字符数与 next_schema_offset；使用 schema_offset 读取 JSON 字符串的显式 schema_chunk 页。所有外层响应都是完整 JSON。按照 Unicode scalar offset 和一致 hash 拼接，读完后得到原始完整 schema；不能把一个 chunk 当完整 schema 或猜测遗漏约束。
- 仅工具元数据或 Skill 索引本身过大时返回 `capability_description_entry_too_large`，不制造空工具、残缺 schema 或成功假象。
- MCP 归一化和最终模型请求预算两层，遇到超限的能力定义返回完整 `capability_description_budget_exceeded` 诊断，而不是残缺 JSON。普通文件/网页文字仍使用原有有标识的预算策略；没有全局取消内存/传输限制。
- Host 的 owner/Run-scoped invocation ledger 按 plugin_option + 返回内容中的 tool_option 找到最新成功的工具描述。读工具 B 或目录下一页不覆盖工具 A 的有效 gate；目录页、业务 is_error 结果不构成调用授权。条件 gate、Skill 激活、固定 Release 和审批链路仍执行。
- 根据用户要求，不保留旧 Run 的全量披露兼容分支，也不改写历史冻结快照。若旧 Run 无当前字段，需使用当前能力创建新的 Run。在线 bootstrap/离线发布均生成新不可变 revision，更新已存在的 broker schema、去重，并保留其他授权选择，不额外赋予能力。
- 共享授权查询不再把缺失 view 的旧记录默认视为工具详情；只有 disclosure_version=2 且 view=tool 的成功记录能成为 Plugin 调用的描述依据。新增旧全量描述拒绝授权负例。binding_schema_version=1 仍表示未改变的 Skill gate 格式，不是旧 Run 兼容开关。

Windows 原生暂不修改；Windows 开发需同步 catalog、单工具详情、分页、错误契约及能力发布。共享 Host/运行时已支持上述传输和 ledger 行为，不能仅更新提示词。

## 本次日志中的 39 个工具

这是 Browser 0.1.16 的目录，不是 39 个任务；编号仅在该 Run 中有效。排序让原始 CDP 出现在前面，不代表推荐优先调用。

| 编号 | 工具 | 用途 |
| --- | --- | --- |
| 1 | browser_cdp_attach | 为标签页附加原始 CDP 会话 |
| 2 | browser_cdp_detach | 解除原始 CDP 会话 |
| 3 | browser_cdp_events | 读取原始 CDP 事件 |
| 4 | browser_cdp_send | 执行原始 CDP 命令 |
| 5 | browser_cdp_subscribe | 订阅原始 CDP 事件 |
| 6 | browser_cdp_targets | 列举可附加页面目标 |
| 7 | browser_cdp_unsubscribe | 取消原始事件订阅 |
| 8 | browser_click | 点击当前快照元素 |
| 9 | browser_console | 观察控制台及页面异常 |
| 10 | browser_downloads | 观察并收集受控下载文件 |
| 11 | browser_fill_form | 按元素引用填写多字段表单 |
| 12 | browser_find | 按角色/名称/文本查找元素 |
| 13 | browser_handle_dialog | 处理 JavaScript 对话框 |
| 14 | browser_har_start | 开始有界脱敏网络记录 |
| 15 | browser_har_stop | 停止并输出 HAR 文件 |
| 16 | browser_navigate | 导航到指定网址并等待就绪 |
| 17 | browser_network | 观察网络事件流 |
| 18 | browser_network_request | 观察指定网络请求事件 |
| 19 | browser_press | 向页面焦点发送按键 |
| 20 | browser_route_add | 添加授权的请求拦截/固定响应规则 |
| 21 | browser_route_clear | 清除全部请求规则 |
| 22 | browser_route_list | 列举请求规则 |
| 23 | browser_route_remove | 移除一条请求规则 |
| 24 | browser_screenshot | 获取页面 PNG 截图 |
| 25 | browser_scroll | 滚动当前页面 |
| 26 | browser_session_close | 关闭任务浏览器会话 |
| 27 | browser_session_open | 打开或复用绑定会话，返回真实浏览器模式 |
| 28 | browser_session_status | 查看会话模式和状态 |
| 29 | browser_snapshot | 获取页面可访问性快照及元素引用 |
| 30 | browser_tab_close | 关闭标签页 |
| 31 | browser_tab_new | 新建标签页 |
| 32 | browser_tab_switch | 切换活动标签页 |
| 33 | browser_tabs | 列举标签页 |
| 34 | browser_type | 向当前元素输入文字 |
| 35 | browser_upload | 使用授权文件 grant 上传文件 |
| 36 | browser_wait | 等待元素、文字或页面就绪 |
| 37 | browser_websocket_events | 读取 WebSocket 事件 |
| 38 | browser_websocket_start | 开始观察 WebSocket |
| 39 | browser_websocket_stop | 停止观察 WebSocket |

普通新闻查询主要需要 session_open → navigate → snapshot，按实际页面需要再选择 find、交互与观察；原始 CDP、网络拦截不应成为目录缺失时的猜测性替代路径。

## 验证状态

- 新增目录分页/稳定编号/筛选、单工具详情、超大 Unicode schema 无损重组、非法参数和过大元数据负例。
- 新增共享 MCP/模型请求超限定义不裁断回归，以及 Host 分工具 ledger 的重启/隔离/目录非授权回归。
- shared Profiles/AI runtime/Storage/Local runtime/MCP runtime：41+186+99+43+85，共 454 项通过。
- 原生扩展回归：XCTest 157 项（3 项可选集成当时跳过），Swift Testing 15 项通过。随后显式启用已安装 Browser、仓库 Browser 工具目录及 Document 附件集成，3 项均通过。
- 首次真实浏览器验收 Run `task-run-84bcd031-e294-4388-b19c-66973fd69356` 发现可选字段被填成占位值，未能打开会话；这次失败不能算通过。已补可空字段、显式非严格模式、参数模式说明及具体错误指导。
- 补充修复后：原生 42 项（含上述 3 项真实目录/附件集成）全部通过；共享 Storage/Local runtime/MCP runtime 99+43+85 共 227 项通过；能力发现 Skill validator 通过。
- 最终客户端包构建与签名校验通过，构建时间 `2026-10-14T03:06:29Z`；已原位安装到 `/Applications/ChatOS.app` 并重启该路径的唯一客户端。旧包可恢复备份：`/Applications/.chatos-disclosure-backup.VjqeJa/ChatOS.app`；中间验收版备份：`/Applications/.chatos-disclosure-final-backup.Cp9Kuf/ChatOS.app`。
- 最终扩展回归：XCTest 76 项，Swift Testing 11 项全部通过，无跳过。
- 最终真实浏览器 Run `task-run-1523c40d-f0cd-4695-a926-7449084af44b`、Task `local-task-6dfa34c7-525e-429f-92bf-87d566897903-1`：目录分页成功，选中工具的详情读取成功（session_open 1762、navigate 1462、snapshot 1443、session_close 1332 字符，均为完整响应）；实际通过已配对 Chrome 扩展打开、导航至 example.com，并关闭测试会话。没有原始 CDP 猜测调用、占位参数错误或描述截断。
- **整体浏览器内容读取验收未通过**：browser_snapshot 仅返回“Learn more”链接，没有页面标题和正文首句；模型没有编造内容，Task 正确报告 failed（即使 Run 提交结果成功）。这是独立的页面内容采集缺陷，不是本次目录/单工具 schema 披露失败。未宣称完整仓库 CI 或全部插件业务场景已通过。

## 后续独立缺陷：页面正文读取

代码位置 `plugins/browser/crates/browser-cdp-core/src/runtime.rs` 的 SNAPSHOT_SCRIPT 和 snapshot 方法。当前快照只包含可交互节点；普通网页研究没有可靠的标题与正文证据。下一次 Browser Release 应让既有 snapshot/观察工具返回有界的标题、可见正文/静态文本和交互元素引用，不增加工具数量、不依赖 raw CDP 兜底；保持现有点击引用及陈旧引用校验。需补纯文本页面、文章混合交互元素、隐藏内容、超长正文和敏感表单不泄漏测试，发布后重新做打开→读取→关闭真实验收。本轮未修改或发布这部分 Browser 实现。

Responses 的可选参数处理依据：[官方 Function calling 文档](https://developers.openai.com/api/docs/guides/function-calling)。未指定 strict 时会尝试归一化成严格模式；显式 strict=false 保留非严格函数调用，nullable 字段也可表达未使用的参数。Skill 只补充请求模式指导，不能替代真实 schema 和运行器授权校验。
