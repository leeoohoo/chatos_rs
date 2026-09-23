# 3.0.4 推进记录：Agent 文档预览与性能 M3

- 时间：2026-09-18 19:41:33 CST（Asia/Shanghai）
- 本轮目标：完成 M3 的统一附件卡片、Markdown 独立预览、同步状态与大文档性能闭环。
- 起始提交：`4b4c01d8a`
- 本轮代码提交：`7408de48d`

## 实际改动

1. 群聊、Human–Agent 私聊和 Agent–Agent 私聊继续共用 `AgentMessageAttachmentChips`，统一展示文件名、大小、创建者和同步状态。
2. Markdown 附件点击后进入独立 Preview Sheet，不在消息 cell 中展开全文；支持搜索计数、复制全文和另存为。
3. 普通文件卡片提供另存为，图片继续使用共享图片预览；本地文件缺失时自动复用 M2 的鉴权远端恢复。
4. 同步状态覆盖“本机可用、等待同步、正在同步、云端已同步、同步失败”，失败卡片提供显式重试入口和有界错误提示。
5. 同步成功或失败后通过现有本地 change stream 通知对应 room，消息列表重新读取 SQLite 权威状态，不维护第二份 UI 状态源。
6. 新增 `DeferredMarkdownDocumentView`，大 Markdown 在后台解析，完成后复用现有 NSTextView 单布局渲染和 16 MiB/有界条数缓存。
7. 将附件卡片与预览从 composer 文件拆到独立 `AgentMessageAttachmentViews.swift`，避免继续扩大输入组件文件。
8. 新增超过 1 MiB 的 Markdown 后台解析与缓存复用测试。

## 安全与业务不变量

- 预览和另存为都先通过 owner、room、message、attachment 四层本地归属校验；远端 artifact ID 不能直接驱动 UI 读取。
- UI 不显示 bucket、object key、预签名 URL 或授权 Header。
- 消息正文、路由、delivery、已读和附件事务语义未改变。
- 重试只重新排队当前账户附件；云端失败仍不影响本地消息和同机预览。
- 历史长消息继续使用分页、单 NSTextView 布局和有界 Markdown cache，不因新附件预览回退为消息内全文展开。
- 用户提供的测试账号未写入代码、测试、日志或提交。
- 工作区原有 ViewModel、Workspace 和 Store 测试并行改动未纳入本轮提交。

## 验证结果

1. M3/M2 定向测试通过：
   - `swift test --package-path clients/macos --filter 'AgentArtifactSyncTests|MarkdownRenderCacheTests'`
2. 1 MiB 以上 Markdown 后台解析并命中缓存的测试通过。
3. macOS 完整测试通过：
   - `swift test --package-path clients/macos`
4. 完整测试覆盖 Core、Connector、App、AgentRuntime、API 的 XCTest 与 Swift Testing 套件，0 failed；仅保留 SwiftTerm 构建缓存的既有 warning。
5. 拆分附件视图文件后再次完成编译和定向测试，0 failed。
6. `git diff --cached --check` 通过。

## 下一步

进入 M4：增加不含正文/文档内容的本地观测统计，覆盖正文长度分布、文档创建率、上传结果、预览耗时和工具拒绝原因；补齐阈值校准测试与完整端到端验收，然后再开始 3.0.4 原生客户端重构与 macOS/Windows 对齐。
