# macOS 变更的 Windows 同步登记

更新时间：2026-08-31

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
| CP-20260831-001 | 没有模型供应商时仍展示历史模型 | 共享后端、模型 DTO、设置与聊天模型 UI | 待实现 | 清理 SQLite 中失效的本机审批模型 ID，增加回归测试后做 Windows 真机验收 |
| CP-20260831-002 | Raycast 风格全局快速搜索 | WinUI、Windows Search、Shell、全局快捷键 | 待实现 | 建立四类搜索 provider、排序状态机和原生浮层 |
| CP-20260831-003 | 本地剪贴板历史 | Windows Clipboard、SQLite、WinUI、隐私过滤 | 待实现 | 实现本地采集、恢复、去重、清理和持久化 |
| CP-20260831-004 | 原生屏幕录制 | Windows Graphics Capture、Media Foundation、WinUI | 待实现 | 实现目标选择、系统声音、停止条和 H.264 文件输出 |

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
- 当前状态：`待实现`。
- 关闭条件：完成 SQLite 失效选择清理和自动化后标记为 `待真机验收`；Windows 真机证据完成后标记为 `已同步`，并回写 macOS 来源登记。

### CP-20260831-002：Raycast 风格全局快速搜索

- macOS 状态：ChatOS、应用、Spotlight 文件和内建动作搜索已实现；排序、本地化和全量测试通过，等待安装包真机验收。
- Windows 风险：Windows Search 查询取消、旧结果回写、全局快捷键冲突、浮层焦点恢复和高频应用索引都可能与 macOS 行为分叉。
- Windows 代码修改：使用 WinUI 浮层和 Windows Search/Shell API，实现四类 provider、`>`/`@`/`/` 前缀、精确/前缀/包含/模糊排序与最近使用加权。
- Windows 自动化要求：覆盖排序稳定性、查询 generation 保护、使用频率上限、空索引降级、ChatOS 项目与联系人动作路由。
- Windows 真机要求：全局快捷键呼出；搜索并启动应用、打开文件、进入 ChatOS 项目；上下键、回车、Escape 与原应用焦点恢复均正确。
- 当前状态：`待实现`。
- 关闭条件：代码和自动化完成后进入 `待真机验收`，真机验证后回写两端登记。

### CP-20260831-003：本地剪贴板历史

- macOS 状态：文本、URL、文件、图片采集与 SQLite/payload 存储已实现；去重、固定、删除和往返测试通过，等待安装包真机验收。
- Windows 风险：密码管理器的敏感格式、Windows 剪贴板延迟渲染、文件列表与图片格式可能导致隐私或恢复问题。
- Windows 代码修改：使用 Windows Clipboard 事件和 SQLite，实现敏感格式过滤、SHA256 去重、恢复标记、500 条/30 天清理、固定与搜索。
- Windows 自动化要求：覆盖四种 payload、重复复制、固定条目不清理、恢复不重复采集、数据库重启恢复和损坏 payload 降级。
- Windows 真机要求：从多个应用复制并恢复测试数据；确认恢复后焦点回到原应用；密码管理器内容不进入历史；重启后记录仍存在。
- 当前状态：`待实现`。
- 关闭条件：隐私过滤与持久化测试完成后进入 `待真机验收`，真机证据完成后关闭。

### CP-20260831-004：原生屏幕录制

- macOS 状态：显示器/窗口、可选系统声音、悬浮停止条、H.264 MOV 和结果提示已实现；编译与全量测试通过，等待真机录制验收。
- Windows 风险：高 DPI 尺寸、系统声音回环、最小化窗口、录制中设备变化和 ChatOS 自身窗口排除均为平台特有风险。
- Windows 代码修改：使用 Windows Graphics Capture 与 Media Foundation（或等价原生 API）实现显示器/窗口选择、系统音频、30fps H.264 和结果归档。
- Windows 自动化要求：覆盖状态机、重复开始/停止、输出路径、异常终止与文件命名；媒体管线部分提供可替换测试边界。
- Windows 真机要求：分别录制窗口、单显示器和系统声音；检查分辨率、方向、音画时长、ChatOS 浮层排除以及录制完成提示。
- 当前状态：`待实现`。
- 关闭条件：代码、自动化与 Windows 真机媒体文件证据完成后标记 `已同步`。

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
