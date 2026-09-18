# 3.0.4 推进记录：Agent Artifact 后端 M2

- 时间：2026-09-18 18:58 CST（Asia/Shanghai）
- 本轮目标：完成 M2 的账户级 Agent artifact 后端、对象存储校验和鉴权下载闭环。
- 起始提交：`d3175f279`
- 本轮代码提交：`bd5d43c52`

## 实际改动

1. 新增 PostgreSQL `agent_artifacts` 表，持久化账户归属、文件元数据、SHA-256、对象存储定位、状态和幂等键。
2. 新增 `POST /api/agent-artifacts/uploads`，为当前登录账户创建或幂等复用 Markdown artifact 上传记录并签发上传 URL。
3. 新增 `POST /api/agent-artifacts/{artifact_id}/complete`，从对象存储重新读取上传结果，校验 MIME、2 MiB 大小上限和 SHA-256 后才完成记录。
4. 新增 `GET /api/agent-artifacts/{artifact_id}/metadata`、`GET /api/agent-artifacts/{artifact_id}/content` 和 `DELETE /api/agent-artifacts/{artifact_id}`。
5. 新增账户级 artifact repository，并将 API 注册到 conversation runtime。
6. 扩展 S3/MinIO object storage service，支持读取验证、受控下载和删除对象。
7. 并发使用相同幂等键时，以数据库权威记录为准重新生成对应上传 URL，避免 URL 与 artifact 记录错配。

## 安全与业务不变量

- 所有 API 都绑定当前登录账户；跨账户访问、下载和删除统一返回 404，不暴露目标是否存在。
- 只接受 Markdown MIME，服务端不信任客户端完成声明，而是重新读取对象验证大小和 SHA-256。
- 对象 key 的账户作用域使用用户 ID 的 SHA-256，避免简单字符清洗产生账户碰撞。
- 模型不会看到真实用户 ID、本机路径、bucket、object key 或预签名授权 URL。
- MinIO 仅承担登录账户的远端恢复与同步，不进入 Agent 本地执行主链路。
- 用户提供的测试账号未写入代码、测试、日志或提交。
- 工作区原有 ViewModel、Workspace 和 Store 测试并行改动未纳入本轮提交。

## 验证结果

1. 完整 Rust 测试通过：
   - `cargo test --manifest-path chatos/backend/Cargo.toml`
2. 测试结果：440 passed，0 failed，1 ignored；忽略项是既有的真实 PostgreSQL 集成测试。
3. backend bin/public facade 额外测试全部通过。
4. `git diff --cached --check` 通过。

## 下一步

继续 M2 macOS：扩展本地附件远端字段和旧库迁移，新增 `ChatOSAgentArtifactService`、本地同步 outbox、失败退避与登录鉴权恢复；上传失败不得影响消息事务，相同同步请求必须保持幂等。
