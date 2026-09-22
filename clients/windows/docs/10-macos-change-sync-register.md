# macOS 变更的 Windows 同步登记

更新时间：2026-09-22

本文是 Windows 客户端针对 macOS 端 Bug 修复、功能更新和协议变化的执行队列。macOS 侧的来源登记位于 `chatos_swift/docs/18-cross-platform-change-register.md`。

## 执行原则

- 每个 `CP-YYYYMMDD-NNN` 编号必须与 macOS 登记一一对应。
- 先判断变更属于共享后端、共享协议、客户端状态机、WinUI 展示还是平台专属能力。
- 共享后端已经修复，不代表 Windows 已完成；仍需验证 DTO、缓存、默认值、空态和刷新行为。
- Windows 状态只有在代码、自动化测试及必要的真机验收完成后才能标记为 `已同步`。
- 完成后同时更新本文件、`02-capability-parity-matrix.md` 以及 macOS 来源登记。

状态定义：`待分析`、`待实现`、`待自动化验证`、`待真机验收`、`已同步`、`不适用`。

## 同步队列

| 编号 | macOS 变更 | 影响层 | Windows 状态 | 下一步 |
| --- | --- | --- | --- | --- |
| CP-20260831-001 | 没有模型供应商时仍展示历史模型 | 共享后端、模型 DTO、设置与聊天模型 UI | 待真机验收 | 已清理 SQLite 中失效的审批模型 ID并覆盖重建回归；验证设置页空态与重启行为 |
| CP-20260831-002 | Raycast 风格全局快速搜索 | WinUI、Windows Search、Shell、全局快捷键 | 待真机验收 | 已实现四类 provider、排序、模式前缀和全局快捷键；验证快捷键冲突与焦点恢复 |
| CP-20260831-003 | 本地剪贴板历史 | Windows Clipboard、SQLite、WinUI、隐私过滤 | 待真机验收 | 已实现采集、恢复、去重、清理和持久化；验证跨应用恢复与敏感格式过滤 |
| CP-20260831-004 | 原生屏幕录制 | Windows 原生 Snipping Tool、WinUI | 待真机验收 | 已接入显示器/窗口选择、系统音频与原生停止条，并自动归档 MP4；验证系统版本兼容性与媒体参数 |
| CP-20260922-001 | Agent 团队与需求调研 | Core、SQLite、Connector、Presentation、WinUI | 待真机验收 | 项目级调研中心、统一渐进调研、多模态输入、跨会话 Inbox、run-scoped opaque refs、成员提案、Todo 隔离调度和失败 delivery/run 恢复均已补；验证 Windows 真机模型、Plugin、崩溃恢复和长对话内存占用 |

## 未编号工作区观察

- 2026-09-22 审计到两处未提交、由外部并行修改的 macOS 远程连接代码：SSH 二次验证码从“携码重连”改为保留原认证进程并在同一会话续交，以支持 session-bound MFA。Windows SSH.NET 当前仍携码创建新连接，已在能力矩阵标为 `实现中`；在 macOS 变更提交且来源登记分配 `CP-*` 编号前不伪造同步编号，也不触碰这些外部修改。

## 详细记录

### CP-20260831-001：没有模型供应商时仍展示历史模型

- 服务端状态：已部署并清理线上账号的 9 条孤儿模型；供应商、模型目录均为空，默认模型 ID 均为 `null`。
- Windows 风险：
  - API 层可能正确返回空数组，但 Presentation 或 WinUI 仍保留之前的选择。
  - 本地缓存、示例数据或 fallback catalog 可能重新生成不存在的模型。
  - 设置页为空后，聊天模型选择器可能继续显示失效名称。
- 已确认的 Windows 缺口：`ModelSettingsViewModel` 会把失效模型从 `SelectedApprovalModel` 清掉，但不会清理 SQLite 中的 `CommandApprovalModelConfigId`；重新进入设置页或重启后会重复进入失效状态。
- Windows 代码修改：模型目录不再包含本地审批模型 ID 时，通过 `IConnectorModelSettingsStore` 原子保存空 ID，并保留一次用户可见的失效提示。
- Windows 自动化要求：
  1. 模型供应商响应 `[]` 时不产生本地供应商。
  2. 模型配置响应 `[]` 时设置页和聊天选择器均为空。
  3. 默认模型 ID 为 `null` 时清除旧选择和 Thinking 配置，不抛解码异常。
  4. 先加载非空模型、随后刷新为空时，旧模型必须从状态中移除。
  5. 重建 ViewModel 或重启应用后，旧缓存不能恢复已删除模型，SQLite 中的失效审批模型 ID 已被清除。
- Windows 真机要求：
  1. 使用同一线上账号打开模型设置页，确认显示空态。
  2. 打开聊天页，确认不存在已删除模型名称。
  3. 执行刷新、退出重进和应用重启，结果保持一致。
- 当前状态：`待真机验收`；SQLite 清理与 ViewModel 重建回归测试已通过。
- 关闭条件：完成 SQLite 失效选择清理和自动化后标记为 `待真机验收`；Windows 真机证据完成后标记为 `已同步`，并回写 macOS 来源登记。

### CP-20260831-002：Raycast 风格全局快速搜索

- macOS 状态：ChatOS、应用、Spotlight 文件和内建动作搜索已实现；排序、本地化和全量测试通过，等待安装包真机验收。
- Windows 风险：Windows Search 查询取消、旧结果回写、全局快捷键冲突、浮层焦点恢复和高频应用索引都可能与 macOS 行为分叉。
- Windows 代码修改：使用 WinUI 浮层和 Windows Search/Shell API，实现四类 provider、`>`/`@`/`/` 前缀、精确/前缀/包含/模糊排序与最近使用加权。
- Windows 自动化要求：覆盖排序稳定性、查询 generation 保护、使用频率上限、空索引降级、ChatOS 项目与联系人动作路由。
- Windows 真机要求：全局快捷键呼出；搜索并启动应用、打开文件、进入 ChatOS 项目；上下键、回车、Escape 与原应用焦点恢复均正确。
- 当前状态：`待真机验收`；四类 provider、排序、前缀过滤、使用频次和快捷键回退自动化已通过。
- 关闭条件：代码和自动化完成后进入 `待真机验收`，真机验证后回写两端登记。

### CP-20260831-003：本地剪贴板历史

- macOS 状态：文本、URL、文件、图片采集与 SQLite/payload 存储已实现；去重、固定、删除和往返测试通过，等待安装包真机验收。
- Windows 风险：密码管理器的敏感格式、Windows 剪贴板延迟渲染、文件列表与图片格式可能导致隐私或恢复问题。
- Windows 代码修改：使用 Windows Clipboard 事件和 SQLite，实现敏感格式过滤、SHA256 去重、恢复标记、500 条/30 天清理、固定与搜索。
- Windows 自动化要求：覆盖四种 payload、重复复制、固定条目不清理、恢复不重复采集、数据库重启恢复和损坏 payload 降级。
- Windows 真机要求：从多个应用复制并恢复测试数据；确认恢复后焦点回到原应用；密码管理器内容不进入历史；重启后记录仍存在。
- 当前状态：`待真机验收`；文本、URL、文件、图片、去重、固定、恢复抑制和清理测试已通过。
- 关闭条件：隐私过滤与持久化测试完成后进入 `待真机验收`，真机证据完成后关闭。

### CP-20260831-004：原生屏幕录制

- macOS 状态：显示器/窗口、可选系统声音、悬浮停止条、H.264 MOV 和结果提示已实现；编译与全量测试通过，等待真机录制验收。
- Windows 风险：高 DPI 尺寸、系统声音回环、最小化窗口、录制中设备变化，以及“包含 ChatOS 与宠物但只排除控制条”的窗口过滤均为平台特有风险。
- Windows 代码修改：通过 Windows 原生 Snipping Tool 录屏协议提供显示器/窗口选择、系统声音、停止控制条及 H.264 MP4，由 ChatOS 状态协调器检测完成文件并复制到 `Videos/ChatOS`。
- Windows 自动化要求：覆盖状态机、重复开始/停止、输出路径、异常终止与文件命名；媒体管线部分提供可替换测试边界。
- Windows 真机要求：分别录制窗口、单显示器和系统声音；检查分辨率、方向、音画时长；显示器录制应包含 ChatOS 主窗口和宠物但不包含录制控制条，并验证录制完成提示。
- 当前状态：`待真机验收`；归档候选状态机自动化已通过，Windows 原生录制与系统音频仍需真机媒体证据。
- 关闭条件：代码、自动化与 Windows 真机媒体文件证据完成后标记 `已同步`。

### CP-20260922-001：Agent 团队与需求调研

- macOS 状态：Agent 团队包含 Agent 配置、项目团队/私聊、消息与附件、Todo 调度、共享资产、项目工具、模型循环，以及最新的结构化需求调研和执行者资产更新建议。
- Windows 风险：Windows 原先完全没有 Agent 团队领域模型、持久化、调度或 UI；普通任务图不能提供 durable delivery、团队权限、项目边界或 Human 调研闭环。
- Windows 代码修改：新增账号隔离 SQLite schema、Agent/Room/Message/Todo/Asset/Survey/Delivery/Run 模型与 Store；Responses API 工具循环；默认插件与成员 allowlist 交集驱动的 MCP 真执行，并复用权限/逐次审批、OAuth/Secret 和 Artifact 管线且按 run 清理；项目文件与审批终端；后台心跳/恢复；Presentation 状态机；项目工作区 WinUI；附件元数据与 payload 分表、UTF-8 正文按需读取；触发消息与冻结 Todo 来源中的 PNG/JPEG/GIF/WebP/PDF 使用 Responses 图片/文件 part，并以最多 8 项、单项 8 MiB、合计 16 MiB、文件签名和会话+消息+附件归属二次校验约束模型输入；账号级跨会话 Inbox、成员范围过滤、排除自身回复、单调已读游标、原子读取/标记、批量元数据查询和 v16 迁移；真实模型 Run 使用独立 opaque reference vault 隔离 Agent/会话/消息/附件/Todo/资产/调研/Plugin 持久 ID，并提供账号级工作区快照；Agent 间私聊；Todo 不可变执行合同、跨会话来源关系、builtin 能力依赖补全、opaque Plugin 选择快照和 v17 迁移，并以原子 schedule state/start-next 保证每 Agent 单执行槽及优先级选取；共享资产 create/update 分离、分类对齐和项目经理自动维护唤醒；需求调研改为项目归属并接入统一渐进 Skill 协议、跨团队任务核对和 v14 迁移，独立项目入口提供跨团队列表、阶段排序、详情、填写/提交、解决方案与执行步骤、空态/错误态/刷新和明确的 Human/Agent 权限状态，提交不再依赖当前选中的团队房间；成员变更提案使用显式 hire+terminate 权限、v15 持久化和 Human 原子审批，新建/入队/移出均不会由 Agent 直接生效。
- Windows 自动化要求：覆盖账号隔离、默认/@ 路由、4-hop/12-run、Todo 依赖/revision/manager 通知、资产版本与 manager/executor 权限、资产维护去重唤醒、需求调研幂等/提交/解决/专职 Agent 权限、成员提案权限组合/幂等冲突/审批前隔离/原子生效/账号隔离/v15 重启、附件按需正文、心跳、模型错误脱敏、完整模型回复/工具循环和插件 MCP run session。
- 已关闭的最新差距：Todo communication/executor lane 已拆分状态工具与服务端权限边界；经理通讯通道只能重排/取消，执行通道只能完成、阻塞或记录当前 delivery 所属 Todo，不能通过隐藏工具越权。两条 lane 现按账号分别加锁并用 trigger 定向原子 claim，过期崩溃恢复只扫描当前 lane；独立 2 秒 communication 后台循环和 UI 触发可在长 Todo executor 仍运行时完成新消息回复，且不会把另一 lane 的活跃 delivery 当作遗留任务。图片/PDF 多模态输入已覆盖普通触发消息和 Todo 冻结来源，二进制不经 UTF-8 解码且只暴露 run-scoped opaque ref。团队与项目 Todo 查询已对齐 macOS `c53e0568b` 的活跃优先语义，并显式容纳 Windows 的 Ready 状态：执行中、就绪、等待依赖、阻塞均排在完成和取消历史前，同状态内再按优先级与手工顺序稳定排序。
- 已关闭的项目调研 UI 差距：新增项目顶层“需求调研”入口和独立 Presentation 状态机，单次项目级查询加载跨团队调研，按待填写、等待方案、已解决排序；查询限制 1–500 条、默认 200 条且由 SQL 优先保留待处理项，避免长期项目产生无界 payload 或 UI 集合；完整展示问卷答案、备注、解决方案、执行步骤、风险与资料，Human 只能填写待处理调研且提交后只读，解决权限状态显式说明；空态、错误态、忙碌态和刷新均已覆盖。自动化验证 project-scoped 提交、阶段排序、只读权限、查询上限和错误恢复，XAML XML 解析与 Automation ID 静态契约通过。
- 已关闭的失败恢复差距：对照 macOS `669665a46`，Windows 会在超时、408、429 或 5xx 时于同一 run 的总调用预算内以 1/2/4/8/16 秒退避最多重试 5 次；显式把失败 Todo 恢复为 Ready 时，事务会复活同一 delivery、清空失败字段并保留 attempt，旧 `todo:{id}:revision:{n}` 键会惰性收敛为稳定 `todo:{id}`。调度器读取该 delivery 的 durable run，复用 run ID 和累计模型调用数，失败请求也先持久化调用计数；不会复制触发消息或越过 16 次总预算。完成/取消路径同时兼容稳定键与旧 revision 键。
- 已关闭的 Windows 性能差距：团队和项目 Todo 查询在活跃优先排序后于 SQL 层限制结果，默认 200、服务端硬上限 1000；模型 `todo_list` 默认 100 且参数在工具 schema 与执行端共同限制为最多 200，WinUI snapshot 和调研任务核对最多 200，避免长期项目把全部历史、来源关系和大字段无界送入内存或模型上下文。executor 改为按 delivery 精确读取当前 Todo，并用单次 `IN` 查询按合同声明顺序加载最多 100 个依赖，不再为了查一个执行合同扫描整块任务板，也不会因终态历史被截断而丢失冻结依赖结果。
- 当前未发现 CP-20260922-001 范围内仍可由本地代码关闭的差距；未编号的 session-bound SSH MFA 工作区变化另列上方观察，等待来源提交和登记。
- Windows 真机要求：代码差距关闭后，验证 Agent/团队编辑与提案对话框、团队切换、附件/多模态、项目调研中心、模型工具、真实插件进程、Artifact、命令审批、崩溃恢复和长对话内存占用。
- 当前状态：`待真机验收`；Windows solution 497 项测试通过，Windows 本轮源码均低于 800 行；全仓源码体积检查仅被未由本批修改的 macOS `TeamRequirementSurveysView.swift` 867 行阻塞。
- 关闭条件：在 Windows x64/ARM64 编译，x64 完成 UI/模型/Plugin/终端/崩溃恢复 smoke 后，两端登记改为 `已同步`。

## 新记录模板

```markdown
### CP-YYYYMMDD-NNN：标题

- 服务端或 macOS 状态：
- Windows 风险：
- Windows 代码修改：
- Windows 自动化要求：
- Windows 真机要求：
- 当前状态：`待分析`。
- 关闭条件：
```
