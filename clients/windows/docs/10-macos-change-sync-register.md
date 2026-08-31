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
