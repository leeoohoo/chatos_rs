# 3.0.4 推进记录：Agent Artifact macOS M2

- 时间：2026-09-18 19:26:27 CST（Asia/Shanghai）
- 本轮目标：完成 M2 的 macOS API、SQLite 同步 outbox、重试和远端恢复闭环。
- 起始提交：`3e0d31155`
- 本轮代码提交：`d4d78966a`

## 实际改动

1. 新增 `ChatOSAgentArtifactService`，完整执行上传申请、短期 PUT、服务端完成校验和登录鉴权下载；上传 URL 与授权信息不进入 Agent 模型上下文。
2. API 原始数据请求复用现有登录凭据刷新和 401 失效语义，并用 authentication session 固定一次 artifact 操作，账户切换后拒绝继续完成旧请求。
3. 扩展 `ProjectAgentMessageAttachment` 和 SQLite 附件表，加入 SHA-256、同步状态、artifact ID、内部对象定位、失败摘要和同步时间等字段。
4. 新增 SQLite schema 23 迁移；旧表逐列补齐，旧附件默认保持 `local_only`，不会被错误上传。
5. Agent 发送 Markdown 时，在原有消息与附件事务中直接写入 `queued` outbox；本地消息成功不等待网络，上传失败不回滚消息。
6. outbox 支持 `queued -> uploading -> synced`、失败退避、手动重新排队、崩溃后 uploading 租约恢复和相同附件幂等键。
7. AppModel 在登录、唤醒、重新激活和后台轮询时 drain 当前账户的 artifact outbox；退出登录会取消旧账户任务。
8. 本地附件文件缺失且状态为 `synced` 时，仍先执行消息/房间/附件归属校验，再通过登录账户下载，校验大小和 SHA-256 后写回 0600 受控缓存。

## 安全与业务不变量

- Agent 工具仍只看到 `message_ref`、`attachment_ref` 和安全展示元数据，不会得到 artifact ID、本机路径、bucket、object key 或预签名 URL。
- 本地消息事务和 Agent delivery 语义未改变；MinIO 离线只影响同步状态，不影响消息发送、同机预览和 Agent 协作。
- 远端恢复必须先通过本地消息归属校验；猜测 artifact ID 不能绕开当前可见消息权限。
- 本地恢复内容必须同时匹配持久化大小与 SHA-256，否则拒绝写入缓存。
- 失败详情使用有界通用摘要，避免 URL、授权 Header 或对象定位进入 SQLite/UI 日志。
- 用户提供的测试账号未写入代码、测试、日志或提交。
- 工作区原有 ViewModel、Workspace 和 Store 测试并行改动未纳入本轮提交。

## 验证结果

1. M2 定向测试通过：
   - `swift test --package-path clients/macos --filter 'ChatOSAgentArtifactServiceTests|AgentArtifactSyncTests'`
2. 定向覆盖上传三段流程、离线失败、指数退避、重启恢复、重复 drain 幂等、旧数据库迁移和远端鉴权恢复。
3. Connector 完整回归通过：
   - `swift test --package-path clients/macos --filter ChatOSConnectorTests`
4. macOS 完整测试通过：
   - `swift test --package-path clients/macos`
5. 完整测试包含 XCTest 与 Swift Testing 套件，0 failed；仅保留 SwiftTerm 构建缓存的既有 warning。
6. `git diff --cached --check` 通过。

## 下一步

进入 M3：统一群聊和两类私聊的附件卡片，加入 Markdown Preview Sheet、同步状态和失败重试入口，并为大 Markdown、历史超长正文和消息切换补齐后台解析、有界缓存及性能测试。
