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

### CP-20260831-002：Raycast 风格全局快速搜索

- 来源：全局效率工具实施方案。
- 类型：功能更新、平台系统集成。
- 预期行为：用户通过全局快捷键打开搜索面板，统一搜索 ChatOS 项目与联系人、已安装应用、Spotlight 文件和内建动作；支持键盘导航、前缀筛选、模糊排序和最近使用加权。
- 设计原因：快速入口必须脱离主窗口，并保持输入即搜索、上下键选择、回车执行和 Escape 返回原应用的完整键盘流。
- 修复范围：新增快速搜索领域模型、应用索引、Spotlight 元数据查询、排序与使用频率存储、Raycast 风格浮动面板和动作路由。
- macOS 状态：自动化已验证，待安装包真机验收。
- macOS 验证证据：`QuickSearchRankingTests` 通过；全量 Swift 测试 110 个 XCTest 与 93 个 Swift Testing 测试通过；本地化审计无缺项。
- Windows 是否需要代码修改：需要。Windows 应使用原生 WinUI 浮层、Windows Search/索引 API 和 Shell 应用启动，不共享 macOS Spotlight 实现。
- Windows 必做项：实现 ChatOS 数据、应用、文件、动作四类 provider；复刻排序、前缀、最近使用和键盘交互；完成全局快捷键、焦点恢复、应用启动与文件打开真机验收。
- Windows 状态：`待实现`。

### CP-20260831-003：本地剪贴板历史

- 来源：全局效率工具实施方案。
- 类型：功能更新、本地隐私存储。
- 预期行为：后台记录文本、URL、文件和图片剪贴板内容，支持搜索、恢复、固定、删除和清空；敏感或临时剪贴板类型不入库，数据自动按 500 条和 30 天清理。
- 设计原因：剪贴板历史必须完全保存在客户端本机，恢复后回到用户之前使用的应用，并防止恢复动作产生重复记录。
- 修复范围：新增剪贴板监听器、SQLite WAL 存储、独立 payload 文件、SHA256 去重、敏感类型过滤和 Raycast 风格历史面板。
- macOS 状态：自动化已验证，待安装包真机验收。
- macOS 验证证据：`ClipboardHistoryStoreTests` 覆盖文本去重、固定、删除以及文件和图片往返；全量测试与本地化审计通过。
- Windows 是否需要代码修改：需要。Windows 应使用原生剪贴板事件、SQLite 和 WinUI 面板，并实现等价的敏感格式过滤与容量策略。
- Windows 必做项：实现文本、URL、文件、图片采集与恢复；敏感格式过滤；SQLite/payload 生命周期；全局快捷键、焦点恢复、持久化和重启回归。
- Windows 状态：`待实现`。

### CP-20260831-004：原生屏幕录制

- 来源：全局效率工具实施方案。
- 类型：功能更新、平台系统能力。
- 预期行为：用户选择显示器或窗口开始录制，可选择系统声音；录制中显示悬浮停止条，再次按快捷键可停止；完成后明确展示文件并支持打开或在文件管理器中定位。
- 设计原因：录屏必须使用平台原生采集链路；整个显示器录制要包含用户实际看到的 ChatOS 主窗口与宠物，只排除录制控制条，并在高 DPI 屏幕下按实际像素输出。
- 修复范围：新增 ScreenCaptureKit、AVAssetWriter、目标选择面板、录制状态控制条、结果提示和 Movies/ChatOS 文件归档。
- macOS 状态：编译与自动化已验证，待显示器、窗口和系统声音真机验收。
- macOS 验证证据：全量 Swift 测试通过；Retina 窗口按 `contentRect × pointPixelScale` 计算偶数像素尺寸；本地化审计无缺项。
- Windows 是否需要代码修改：需要。Windows 应使用 Windows Graphics Capture/Media Foundation 或等价原生链路，不能复用 ScreenCaptureKit。
- Windows 必做项：实现显示器与窗口选择、系统音频、30fps H.264、悬浮停止条、仅排除录制控制条、结果提示和高 DPI 验收；显示器录制必须包含 ChatOS 主窗口和宠物。
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
