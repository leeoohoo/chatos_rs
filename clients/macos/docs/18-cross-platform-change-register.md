# macOS / Windows 跨平台变更登记

更新时间：2026-08-31

本文是 macOS 端发现或实现的产品行为变更的权威登记。目标是避免 macOS 修复、功能更新或协议调整只停留在 Swift 客户端，导致 Windows 客户端随后出现行为分叉。

Windows 客户端中的同步镜像位于：

`../../windows/docs/10-macos-change-sync-register.md`

## 登记规则

以下变化必须新增一条记录：

- 用户可见的 Bug 修复。
- 页面、交互、状态流转或默认行为调整。
- 新增功能或删除旧能力。
- HTTP、Realtime、Connector、插件或本地存储协议变化。
- 后端修复会改变客户端列表、状态、按钮可用性或错误展示。
- macOS 平台专属变化，但需要明确说明 Windows 为什么不适用。

每条记录使用稳定编号 `CP-YYYYMMDD-NNN`，并同时写明：

- 问题与预期行为。
- 变更所在层：共享后端、共享协议、macOS 客户端或平台能力。
- macOS 状态和验证证据。
- Windows 是否需要代码修改。
- Windows 的测试与真机验收项。
- Windows 最终同步状态。

Windows 状态只允许使用：

- `待分析`：尚未确认 Windows 是否受影响。
- `待实现`：确认需要 Windows 代码修改。
- `待自动化验证`：代码或共享后端已具备，但缺 Windows 回归测试。
- `待真机验收`：自动化通过，仍需 Windows UI 或系统能力验收。
- `已同步`：Windows 代码、自动化和必要的真机验收全部完成。
- `不适用`：仅限明确的平台专属能力，必须写明理由。

不能因为“Windows 调用同一个后端”就直接标记为 `已同步`。共享后端变更仍需验证 Windows 的 DTO 解码、状态清理、空态展示和刷新行为。

## 完成标准

一条记录只有同时满足以下条件才算关闭：

1. macOS 端行为已验证。
2. 共享后端或协议已部署到目标环境。
3. Windows 影响已分析。
4. Windows 需要的代码和自动化测试已完成。
5. 涉及 WinUI、焦点、窗口、系统权限或本机能力时，已完成 Windows 真机验收。
6. macOS 与 Windows 两份登记的状态一致。

## 变更记录

### CP-20260831-001：没有模型供应商时仍展示历史模型

- 来源：macOS 设置页问题反馈。
- 类型：Bug 修复、共享后端数据一致性。
- 预期行为：账号没有模型供应商时，模型供应商列表和模型配置列表都为空；历史孤儿模型不能展示；默认模型不能继续引用已经失效的模型。
- 根因：User Service 中保留了没有真实供应商关联的旧模型；数据库中的陈旧 `has_api_key` 与实际空密钥不一致；ChatOS 主模型目录读取了 Task Runner 的旧副本。
- 修复范围：
  - User Service 只返回有同用户真实供应商支撑的模型。
  - `has_api_key` 只根据实际解密后的非空密钥计算。
  - 默认模型禁止引用孤儿模型。
  - ChatOS 模型目录优先读取 User Service 权威目录。
  - 通过正式删除 API 清理 9 条旧模型，并清空两个默认模型引用。
- macOS 状态：已验证。
- macOS/线上证据：
  - `GET /api/model-providers` 返回 `[]`。
  - `GET /api/model-configs` 返回 `[]`。
  - `GET /api/model-configs/settings` 中两个默认模型 ID 均为 `null`。
  - `GET /api/chatos/ai-model-configs` 返回 `[]`。
  - User Service 与 ChatOS Backend 容器均为 `healthy`。
- Windows 是否需要代码修改：需要。现有 WinUI 会清空界面中的失效审批模型选择，但本地 SQLite 仍保留旧 `CommandApprovalModelConfigId`，重启或重新进入设置页后会反复恢复为“旧模型不可用”状态。
- Windows 必做项：
  1. 模型目录不包含本地审批模型 ID 时，把 SQLite 中的失效 ID 原子清空，同时保留一次明确的用户提示。
  2. 为模型目录为空、默认模型 ID 为 `null` 和“非空刷新为空”增加 API/Presentation 回归测试。
  3. 确认模型设置页展示空态，不展示示例模型或缓存模型。
  4. 确认聊天模型选择器清除已失效选择，不保留不可用模型名称。
  5. 刷新、退出重进和客户端重启后仍保持空列表，且 SQLite 不再恢复旧审批模型 ID。
  6. 在 Windows 真机连接线上账号完成一次设置页与聊天页验收。
- Windows 状态：`待实现`。

## 新记录模板

```markdown
### CP-YYYYMMDD-NNN：标题

- 来源：
- 类型：Bug 修复 / 功能更新 / 协议变化 / 平台专属。
- 预期行为：
- 根因或设计原因：
- 修复范围：
- macOS 状态：
- macOS 验证证据：
- Windows 是否需要代码修改：
- Windows 必做项：
- Windows 状态：`待分析`。
```
