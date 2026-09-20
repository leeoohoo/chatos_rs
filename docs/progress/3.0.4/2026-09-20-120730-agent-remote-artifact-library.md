# Agent 云端文档跨设备发现与预览

- 时间：2026-09-20 12:07:30 CST（Asia/Shanghai）
- 本轮目标：补齐账户级 artifact 发现路径，使另一台已登录设备无需预先知道 artifact ID 也能列出并预览已同步 Markdown。
- 起始提交：`b285d0ab5`
- 代码提交：`d97de1c01`

## 实际改动

- 后端新增账户鉴权的 `GET /api/agent-artifacts`，只列出当前账户已上传完成的 artifact。
- 列表使用 `created_at + artifact_id` 稳定排序、1–100 有界页大小和不透明游标分页；返回值不包含 bucket、object key、幂等键或授权 URL。
- macOS API service 新增列表 DTO、认证会话绑定、artifact ID/MIME/大小/时间校验和分页支持。
- Native Agent 服务增加远端列表与下载入口。
- Agent 工作区新增“云端 Agent 文档”，支持刷新、分页、下载、SHA-256/大小复核、另存为和 Markdown 预览。
- 本地消息附件与远端文档共用同一个 Markdown 预览组件。
- UI 明确提示：云端文档发现能力不等同于本地 SQLite 群聊消息跨设备同步。

## 涉及文件

- `chatos/backend/src/api/agent_artifacts.rs`
- `chatos/backend/src/repositories/agent_artifacts.rs`
- `clients/macos/Sources/ChatOSCore/AgentArtifactSync.swift`
- `clients/macos/Sources/ChatOSAPI/ChatOSAgentArtifactService.swift`
- `clients/macos/Sources/ChatOSConnector/NativeAgentGroupChatService.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentGroupChatWorkspaceView.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentRemoteArtifactLibraryView.swift`
- `clients/macos/Sources/ChatOSApp/Features/AgentGroupChat/AgentMessageAttachmentViews.swift`
- `clients/macos/Tests/ChatOSAPITests/ChatOSAgentArtifactServiceTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/AgentArtifactSyncTests.swift`

## 业务不变量

- owner 来自认证上下文，客户端不能提交 owner ID；数据库查询始终包含 `user_id`。
- 未认证的列表、元数据、内容和删除路由全部拒绝。
- 仅 `uploaded` artifact 进入列表；staged/deleting 对象不作为可预览文档暴露。
- 列表与 UI 不暴露 MinIO bucket/object key、预签名 URL、本地路径或内部幂等键。
- 下载内容仍要求 UTF-8 Markdown 且不超过统一 2 MiB 产品限制；UI 再复核大小和 SHA-256。
- 本地消息、附件关系和 Agent Run 仍由当前设备 SQLite 驱动，远端 artifact 不反向触发任务。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p chat_app_server_rs agent_artifact`：7 通过，0 失败。
- `cargo test -p chat_app_server_rs`：444 通过，0 失败，1 个 PostgreSQL 环境测试按设计忽略。
- `swift test --package-path clients/macos --filter ChatOSAgentArtifactServiceTests`：1 通过，0 失败。
- `swift test --package-path clients/macos --filter AgentArtifactSyncTests`：2 通过，0 失败。
- `swift test --package-path clients/macos`：全量通过；现有环境门禁测试按设计跳过。
- `git diff --check`：通过。

## 剩余风险与下一步

- 此轮解决的是账户级 artifact 发现和预览；群聊消息元数据本身仍未做跨设备同步，不能宣称完整消息历史已跨设备闭环。
- PostgreSQL 跨账户列表/读取集成测试仍需 `CHATOS_TEST_DATABASE_URL` 和已迁移测试库；无认证路由测试已自动执行。
- 真实登录账户、MinIO、第二台设备和 100 条附件消息 UI/性能 E2E 仍需运行时环境。
- 下一步继续 macOS 原重构方案中的大文件拆分与已有基准性能问题。
