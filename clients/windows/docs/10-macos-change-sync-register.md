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
| CP-20260922-001 | Agent 团队与需求调研 | Core、SQLite、Connector、Presentation、WinUI | 待实现 | 项目级统一渐进调研、附件文本读取、跨会话 Inbox、run-scoped opaque refs、文档发送 receipt、成员提案原子审批，以及 Todo 不可变执行合同/来源关系/builtin+Plugin 能力快照/原子串行调度/隔离 executor 上下文与工具权限/v18 资产快照已补；继续收紧 lane 状态边界 |

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
- Windows 代码修改：新增账号隔离 SQLite schema、Agent/Room/Message/Todo/Asset/Survey/Delivery/Run 模型与 Store；Responses API 工具循环；默认插件与成员 allowlist 交集驱动的 MCP 真执行，并复用权限/逐次审批、OAuth/Secret 和 Artifact 管线且按 run 清理；项目文件与审批终端；后台心跳/恢复；Presentation 状态机；项目工作区 WinUI；附件元数据与 payload 分表、UTF-8 正文按需读取；账号级跨会话 Inbox、成员范围过滤、排除自身回复、单调已读游标、原子读取/标记、批量元数据查询和 v16 迁移；真实模型 Run 使用独立 opaque reference vault 隔离 Agent/会话/消息/附件/Todo/资产/调研/Plugin 持久 ID，并提供账号级工作区快照；Agent 间私聊；Todo 不可变执行合同、跨会话来源关系、builtin 能力依赖补全、opaque Plugin 选择快照和 v17 迁移，并以原子 schedule state/start-next 保证每 Agent 单执行槽及优先级选取；共享资产 create/update 分离、分类对齐和项目经理自动维护唤醒；需求调研改为项目归属并接入统一渐进 Skill 协议、跨团队任务核对和 v14 迁移；成员变更提案使用显式 hire+terminate 权限、v15 持久化和 Human 原子审批，新建/入队/移出均不会由 Agent 直接生效。
- Windows 自动化要求：覆盖账号隔离、默认/@ 路由、4-hop/12-run、Todo 依赖/revision/manager 通知、资产版本与 manager/executor 权限、资产维护去重唤醒、需求调研幂等/提交/解决/专职 Agent 权限、成员提案权限组合/幂等冲突/审批前隔离/原子生效/账号隔离/v15 重启、附件按需正文、心跳、模型错误脱敏、完整模型回复/工具循环和插件 MCP run session。
- 已确认剩余代码差距：Todo communication/executor lane 的更严格状态边界，以及触发图片/PDF 多模态输入和项目级调研中心 UI。
- Windows 真机要求：代码差距关闭后，验证 Agent/团队编辑与提案对话框、团队切换、附件/多模态、项目调研中心、模型工具、真实插件进程、Artifact、命令审批、崩溃恢复和长对话内存占用。
- 当前状态：`待实现`；当前已完成部分通过 Windows solution 481 项测试，Windows 本轮源码均低于 800 行；等待剩余代码差距与 Windows 真机验收。
- 关闭条件：先关闭上述代码差距并完成自动化；再在 Windows x64/ARM64 编译，x64 完成 UI/模型/终端 smoke 后，两端登记改为 `已同步`。

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
