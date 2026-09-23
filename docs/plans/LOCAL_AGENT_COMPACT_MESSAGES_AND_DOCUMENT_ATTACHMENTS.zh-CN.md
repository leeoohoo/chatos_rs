# ChatOS 本地 Agent 精简消息与长文附件实施方案

## 1. 文档状态

本文是实施前设计方案，当前只固化目标、边界、协议和验收标准，不代表功能已经完成。

本方案解决两个相互关联的问题：

1. Agent 在群聊或私聊中直接发送大段方案、日志、代码和表格，用户阅读负担大，也显著增加消息列表 Markdown/TextKit 排版成本。
2. 现有 Agent 虽然能读取 Human 附加到消息中的文件，但不能主动创建长文附件、把附件随消息发送，也没有面向 Agent 长文的 MinIO 上传与预览闭环。

本方案不改变既有 manager / executor 双线程模型，不新建另一套聊天系统，也不让本地 Agent 的正常运行依赖 MinIO 在线。

## 2. 产品目标

Agent 的默认输出应是一条可以快速读完的消息：

- 先给结论；
- 只保留关键依据、风险和下一步；
- 日常回复建议控制在 300–800 个中文字符；
- 完整方案、长日志、代码、长表格、研究过程和详细报告写入 Markdown 文档；
- 消息正文说明附件是什么、为什么值得打开；
- 用户点击附件卡片即可在客户端预览 Markdown；
- 收到消息的其他 Agent 可以通过 Relay MCP 分段读取附件全文；
- 不允许把一篇长文拆成多条连续消息规避长度限制。

产品体验应是：

```text
Agent 消息
  ├─ 2～6 行摘要、结论、风险、下一步
  └─ 附件卡片：三国自走棋技术方案.md
       ├─ 点击：客户端内 Markdown 预览
       ├─ 下载/另存为
       └─ 同步状态：本机可用 / 云端已同步 / 同步失败可重试
```

附件不是模型写在正文里的裸 URL。模型只选择一个本轮临时文档引用，客户端把它渲染成正式附件卡片和可点击预览入口。

## 3. 架构原则

### 3.1 消息与文档分工

消息承担沟通，文档承载细节：

| 内容 | 放置位置 |
| --- | --- |
| 结论、决定、关键风险、下一步 | 消息正文 |
| 完整实施方案、分析报告、操作记录 | Markdown 附件 |
| 大段日志、堆栈、测试输出 | Markdown 或文本附件 |
| 大型表格、代码、配置样例 | Markdown 附件 |
| 需要团队长期维护的项目背景、技术栈、决策 | 团队共享资产，不以聊天附件替代 |
| Todo executor 的过程记录与完成总结 | Todo progress/result；需要展开时可以同时附长文附件 |

附件用于一次沟通的详细材料；团队共享资产仍然是项目长期权威上下文。二者不能混为一套数据。

### 3.2 本地优先，MinIO 不进入执行主链路

Agent 创建文档时，客户端先在本机生成并持久化 UTF-8 Markdown 文件，再进入 MinIO 同步队列：

```text
Agent 调用 chat_document_create
  -> 客户端校验、清洗文件名并生成本地文档
  -> 返回本轮 document_ref
  -> Agent 调用发送工具并携带 document_ref
  -> SQLite 消息事务绑定本地附件
  -> 后台上传现有 MinIO/S3 对象存储
  -> UI 立即使用本地文件预览
  -> 上传成功后补齐远端 artifact 元数据
```

这样可以保证：

- 断网或服务端暂时不可用时，Agent 之间在同一客户端内仍能发送、预览和读取文档；
- MinIO 负责跨设备分发、云端留存和远端预览，不负责驱动 Agent Run；
- 上传失败不会丢失消息或文档，只会呈现“云端同步失败，可重试”；
- 本机文件是未同步阶段的事实源，远端 artifact 是同步后的可恢复副本。

如果未来产品要求“必须完成云端同步才能发送”，应作为显式策略开关，而不能把它隐式写进 Relay MCP。

### 3.3 ID 和权限由程序透传

模型不得获得或填写以下值：

- owner/user ID；
- project、team、room、message、Agent 的真实 ID；
- MinIO bucket、object key、access key、secret key；
- 本机绝对路径；
- 可自行修改的预签名上传 URL；
- 可跨消息复用的永久附件凭据。

模型在一个 Run 内只看到：

- `document_ref`：刚创建、尚待附加到消息的文档临时引用；
- `message_ref`：读取消息后得到的临时引用；
- `attachment_ref`：读取该消息附件时得到的临时引用。

所有引用都由 `LocalAgentRunReferenceVault` 生成，绑定 owner、当前 Agent、Run、附件和允许的会话范围，Run 结束即失效。

### 3.4 Skill 负责行为指导，工具负责硬约束

只写 Prompt 无法可靠解决超长消息。最终必须同时具备：

1. 共享沟通 Skill：告诉每个 Agent 何时摘要、何时创建文档、如何在消息中介绍附件。
2. 工具 Schema 限制：发送消息的 `content` 不能再允许 64,000 字符。
3. 工具运行时校验：即使模型绕过 JSON Schema，也必须拒绝超长正文并返回结构化修复建议。
4. UI 历史兜底：旧的超长消息仍需分页、惰性 Markdown 和排版缓存，不能假设历史数据会消失。

## 4. 当前代码基线与缺口

### 4.1 已有能力

当前已有以下可复用基础：

- `chatos/backend/src/api/attachments.rs`
  - `POST /api/attachments/uploads` 生成预签名 PUT URL；
  - `GET /api/attachments/object?token=...` 返回可预览对象；
  - 返回 `storageProvider`、`bucket`、`objectKey`、`uploadUrl`、`viewUrl`。
- `chatos/backend/src/services/object_storage.rs`
  - 已封装 MinIO/S3 签名、上传限制和对象读取。
- `clients/macos/Sources/ChatOSAPI/ChatOSAttachmentService.swift`
  - 已能申请上传地址并 PUT 文件内容。
- `ProjectAgentMessageAttachmentDraft`、`ProjectAgentMessageAttachment`
  - 本地团队/私聊消息已经支持附件。
- `SQLiteAgentGroupChatStore`
  - 附件写入 `AgentGroupChatAttachments`；
  - 附件与消息在本地事务内绑定。
- `LocalAgentChatToolProvider.chat_read_attachment`
  - 使用 `message_ref + attachment_ref` 校验附件归属；
  - 文本附件支持 `offset/limit` 分段读取。
- 群聊和私聊 UI
  - Human 已可发送文件和图片；
  - 消息模型已经能承载附件卡片。

### 4.2 现有缺口

1. 现有上传接口强制要求云端 conversation/session ID，并通过 `ensure_owned_session` 校验；本地 Agent room UUID 不是云端 conversation，不能伪装传入。
2. Agent 没有创建 Markdown 文档的工具。
3. `chat_inbox_send`、`chat_direct_send`、`chat_team_send`、`chat_send_message` 只接收 `content`，不能携带 Agent 新建的附件。
4. 四个发送工具允许最多 64,000 字符，缺少运行时超长校验和结构化引导。
5. 本地附件模型只保存本地文件信息，没有远端 artifact 状态、对象来源和可恢复标识。
6. Skill 注入目前以职业 Skill 和项目类型 Rule 为主，缺少所有 Agent 共用的“精简沟通”产品 Skill。
7. 现有 `viewUrl` 不能直接作为模型可见字段；把长期 token 或对象地址交给模型会破坏 ID 透传和权限边界。

## 5. 目标数据模型

### 5.1 本地文档草稿

新增 Run 内文档权威记录，由 `LocalAgentRunReferenceVault` 管理：

```swift
DocumentAuthority {
  ownerUserID
  creatorAgentID
  runID
  localDraftID
  localFileURL
  name
  mimeType = "text/markdown"
  size
  sha256
}
```

模型只得到随机 `document_ref`。发送时客户端从 Vault 解析为真实附件草稿；不同 Run 的引用、猜测引用和已消费引用全部拒绝。

### 5.2 消息附件持久化

扩展 `project_agent_message_attachments`，保留现有本地字段，并增加：

| 字段 | 用途 |
| --- | --- |
| `sha256` | 完整性和去重辅助，不作为权限凭据 |
| `sync_status` | `local_only / queued / uploading / synced / failed` |
| `artifact_id` | 服务端生成的稳定 opaque ID，可空 |
| `storage_provider` | 当前为 `minio`，可空 |
| `bucket` | 程序内部恢复对象使用，不返回模型 |
| `object_key` | 程序内部恢复对象使用，不返回模型 |
| `remote_view_path` | 客户端预览入口或可续签标识，不返回模型 |
| `upload_error` | 有界错误摘要，供 UI 重试提示 |
| `synced_at_unix_ms` | 最近成功同步时间 |

不应只保存短期预签名 GET URL。预签名 URL 会过期，必须保存稳定 `artifact_id` 或对象引用，并由客户端在需要时重新申请预览权限。

### 5.3 服务端 Agent artifact

新增账户级 Agent artifact API，不复用要求云端 conversation ID 的普通聊天附件接口：

```text
POST /api/agent-artifacts/uploads
GET  /api/agent-artifacts/{artifact_id}/content
GET  /api/agent-artifacts/{artifact_id}/metadata
DELETE /api/agent-artifacts/{artifact_id}        // 清理未绑定草稿或用户删除
```

服务端记录至少绑定：

- artifact ID；
- owner user ID；
- content type、name、size、sha256；
- bucket、object key；
- created time、upload status；
- 客户端生成的幂等 key；
- 可选的设备/本地消息绑定审计信息。

上传申请只接受当前登录账户，不接受模型提供 owner ID。对象 key 继续由服务端生成。内容读取必须校验当前账户；不提供可枚举的公开对象地址。

### 5.4 草稿与孤儿清理

上传与本地 SQLite 消息事务无法组成跨系统原子事务，因此采用以下状态机：

```text
local_only -> queued -> uploading -> synced
                         \-> failed -> queued
```

- 消息发送前创建的未使用本地草稿在 Run 结束后清理。
- 已上传但最终未绑定消息的 artifact 标记为 staged，由服务端 TTL 清理。
- 已绑定消息的本地附件不可因上传失败删除。
- 删除消息或清理账户数据时，进入可重试的 artifact 删除 outbox，不能阻塞本地事务。

## 6. Relay MCP 工具设计

### 6.1 创建长文工具

新增 `chat_document_create`：

```json
{
  "name": "三国自走棋技术方案.md",
  "title": "三国自走棋技术方案",
  "markdown": "# ..."
}
```

约束：

- 只允许 UTF-8 Markdown；
- `name` 自动清洗，缺少 `.md` 时由客户端补齐；
- 禁止路径分隔符、`..`、控制字符和绝对路径；
- 建议单文档上限 2 MiB，具体值放入统一产品配置，不散落在 Tool Schema 和实现中；
- 客户端计算 size 和 SHA-256，模型不填写；
- 每个 Run 创建数量和总大小都有上限；
- 工具不接受 room ID、message ID、bucket、object key 或 URL。

成功返回：

```json
{
  "document_ref": "document_<opaque>",
  "name": "三国自走棋技术方案.md",
  "size": 18240,
  "mime_type": "text/markdown",
  "instruction": "请在下一次发送消息时通过 document_refs 附加该文档。"
}
```

返回值不包含真实本机路径、MinIO 信息或可由模型传播的下载 token。

### 6.2 发送工具扩展

以下工具统一新增可选字段：

```json
"document_refs": {
  "type": "array",
  "items": { "type": "string" },
  "maxItems": 5,
  "uniqueItems": true
}
```

涉及工具：

- `chat_inbox_send`
- `chat_direct_send`
- `chat_team_send`
- `chat_send_message`

客户端发送流程：

1. 解析每个 `document_ref`；
2. 校验属于当前 owner、Agent 和 Run；
3. 校验未失效、文件仍存在、哈希与大小未变化；
4. 转成 `ProjectAgentMessageAttachmentDraft`；
5. 与消息在同一个本地 SQLite 事务中持久化；
6. 标记引用已消费，幂等重试复用同一发送结果，不能重复制造附件；
7. 触发后台 MinIO 同步 outbox。

### 6.3 消息长度硬限制

四个 Agent 发送工具统一使用一份 `AgentCommunicationPolicy` 配置：

```text
recommendedMessageCharacters = 800
maximumMessageCharacters     = 2000
maximumDocumentsPerMessage   = 5
maximumDocumentBytes         = 2 MiB
maximumDocumentBytesPerRun   = 8 MiB
```

建议值用于 Skill 引导，最大值由工具强制执行。最终数值应经过真实中文、英文和代码消息测试后集中调整，不能分别写死在四个工具里。

正文超限时返回结构化失败：

```json
{
  "ok": false,
  "error": {
    "code": "message_too_long",
    "field": "content",
    "message": "消息正文超过 2000 字符。请保留结论、风险和下一步，把详细内容写入 Markdown 文档后附加发送。",
    "retryable": true,
    "next_tool": "chat_document_create"
  }
}
```

必须在 Schema 和执行函数两处校验。不得自动静默截断正文，也不得自动把模型输出转换成文档，因为自动拆分会丢失模型对摘要与正文边界的判断。

### 6.4 其他 Agent 读取文档

继续复用 `chat_read_attachment`，不再创造第二套读取协议：

- `message_ref + attachment_ref` 必须属于同一条当前可见消息；
- 优先读取本机文件；
- 本机文件缺失且 artifact 已同步时，由客户端使用登录凭据从服务端恢复到受控缓存；
- Markdown 使用 `offset/limit` 分段返回；
- 每次返回 `has_more` 和 `next_offset`；
- 不把 bucket、object key、远端 URL 暴露给模型；
- 附件不是当前 Agent 可见消息的一部分时，即使猜到 artifact ID 也必须拒绝。

## 7. 共享沟通 Skill

### 7.1 Skill 定位

新增产品级 `chatos-compact-communication` Skill，自动注入每个 Agent 的 manager thread。它不属于某一个职业或项目类型，也不能靠编辑 33 个职业 Skill 逐份复制。

建议资源结构：

```text
ChatOSCore/Resources/Skills/chatos-compact-communication/
  SKILL.md
  SKILL.zh-CN.md
  SKILL.en.md
```

运行时按用户语言加载对应正文，版本和内容哈希进入 Run checkpoint，恢复旧 Run 时保持原版本，新 Run 使用新版本。

### 7.2 Skill 核心规则

Skill 只包含会改变 Agent 决策的规则：

1. 默认先给结论，正文只保留关键依据、风险和下一步。
2. 日常沟通尽量控制在 300–800 个中文字符或相近英文长度。
3. 完整方案、长日志、代码、长表格和研究细节必须使用 `chat_document_create`。
4. 创建文档后，在同一条消息的 `document_refs` 中附加，不要只说“见文档”却不附带。
5. 消息正文需写清附件名称、内容和用户为什么需要打开。
6. 不把长文拆成多条消息规避限制。
7. 不把团队长期共享资产退化为一次性附件；需要长期维护的项目事实仍写入团队共享资产。
8. 收到附件时，只在确实需要细节时调用 `chat_read_attachment`，并按段读取，避免一次把全文灌入上下文。

### 7.3 注入边界

- manager thread：完整启用，用于 Human/Agent 私聊和团队群聊。
- executor thread：只注入“最终汇报要精简，详细交付可附文档”的子集；executor 自己的任务上下文与进度记录规则不变。
- Human 输入：不受 Agent 消息最大长度限制，但 UI 应提示大段内容可以改为文件附件。
- 系统消息、工具结果、Todo progress：使用各自独立上限，不能错误套用聊天正文 2,000 字限制。

## 8. 客户端 UI 与预览

### 8.1 消息附件卡片

Markdown 文档卡片显示：

- 文件名；
- Markdown 类型图标；
- 文件大小；
- 创建者；
- 同步状态；
- “预览”和“另存为”操作。

正文不自动展开附件全文，避免消息列表再次承担长文排版。

### 8.2 预览方式

点击卡片打开客户端统一 Markdown Preview Sheet：

- 使用与聊天消息一致的 Markdown 语法能力；
- 预览内容在独立滚动容器内，不扩大聊天消息 cell；
- 大文档分段加载或后台解析，不能阻塞主线程；
- 支持复制、搜索、另存为；
- 本地文件存在时立即预览；
- 仅远端存在时显示下载进度，下载到受控缓存后预览；
- 远端 token 过期时由客户端自动续签或重新请求，不让用户手工处理 URL。

### 8.3 “链接”的产品含义

用户看到的是可点击附件入口，不是模型拼出的 MinIO 裸链接：

- 聊天消息记录中存附件关系；
- UI 将附件关系渲染为链接/卡片；
- 跨设备时客户端根据 `artifact_id` 获取受权内容；
- 复制分享如确有需要，另设短期分享链接能力，并默认关闭；它不属于首期 Agent 协作协议。

## 9. 上传、缓存和安全

### 9.1 上传安全

- MinIO 密钥只存在服务端对象存储服务；
- 客户端只接收短期 PUT URL；
- PUT URL、授权 Header 和 token 不进入模型上下文、Memory Engine、Run event 或普通日志；
- MIME 固定为 `text/markdown; charset=utf-8`；
- 服务端校验 size、owner、哈希和允许类型；
- artifact 内容按账户隔离，禁止以客户端提交 object key 的方式选取写入位置。

### 9.2 本地文件安全

- 使用现有 `AgentGroupChatAttachments` 受控目录；
- 文件和目录权限沿用 0600/0700；
- 文件名只用于展示，磁盘路径使用程序生成 ID；
- 读取前验证标准化路径仍在附件根目录；
- 不允许 Skill 或模型指定本机输出路径。

### 9.3 日志和 Memory

- Run event 只记录文档名称、大小、哈希前缀、同步状态和临时引用类型；
- 不记录 Markdown 全文、预签名 URL、Authorization 或 MinIO object key；
- manager Memory 可以记录“发送了某文档”及其摘要，不重复写入全文；
- 其他 Agent 需要全文时通过附件工具按需读取。

## 10. 失败与恢复语义

| 场景 | 预期行为 |
| --- | --- |
| 消息正文超限 | 工具拒绝，返回 `message_too_long` 和 `chat_document_create` 引导 |
| 文档过大 | 工具拒绝，提示拆分为少量有意义的文档；不得拆成多条聊天消息 |
| MinIO 离线 | 本地消息和预览照常；同步状态为失败/等待，后台重试 |
| 本地文件丢失但远端已同步 | 经账户鉴权下载恢复后预览/读取 |
| 本地和远端都不可用 | 卡片保留元数据并明确显示不可恢复，不让 UI 无限加载 |
| `document_ref` 属于旧 Run | 拒绝并提示重新创建文档 |
| 引用不属于当前 Agent | `invalid_document_ref`，不透露目标是否存在 |
| 附件不属于所给消息 | 沿用 `invalid_attachment_ref`，要求重新读取消息 |
| 上传成功但消息发送失败 | 草稿保留到 Run 结束；未绑定远端 artifact 进入 TTL 清理 |
| 重试同一发送调用 | 幂等复用消息/附件结果，不能重复发送或重复上传 |

## 11. 实施阶段

### M0：配置与共享 Skill

- 新增集中式 `AgentCommunicationPolicy`；
- 新增中英文 `chatos-compact-communication` Skill；
- 将共享 Skill 作为独立产品层注入 manager/executor，不复制到职业和项目类型 Skill；
- 增加 Skill 版本与 Run checkpoint 固化测试。

退出条件：新 Run 能看到共享 Skill，旧 Run 恢复不漂移，所有阈值只有一个权威配置来源。

### M1：本地文档闭环

- 扩展 Run Reference Vault；
- 实现 `chat_document_create`；
- 扩展四个发送工具的 `document_refs`；
- 将文档与消息原子绑定；
- 实现超长消息结构化失败；
- 让 `chat_read_attachment` 读取 Agent 生成的 Markdown。

退出条件：完全离线时，Agent A 能发摘要加文档，Human 和 Agent B 都能预览/分段读取。

### M2：MinIO Agent artifact

- 新增账户级 artifact API 和持久元数据；
- 扩展 macOS API service；
- 增加本地同步 outbox、重试、退避和 orphan 清理；
- 补齐附件远端字段迁移；
- 远端恢复仍走消息归属校验。

退出条件：在线自动同步，断网不阻塞本地消息，恢复联网后补传，另一设备可鉴权预览。

### M3：产品化预览与性能

- 统一群聊/私聊附件卡片；
- 增加 Markdown Preview Sheet；
- 加入同步状态和失败重试；
- 对大 Markdown 做后台解析和有界缓存；
- 保留历史长消息分页、惰性渲染和 TextKit 性能兜底。

退出条件：消息列表不展开长文，点击预览流畅，切换含历史超长消息的会话不会卡死。

### M4：观测与灰度验收

- 记录正文长度分布、文档创建率、上传成功率、预览耗时和工具拒绝原因；
- 指标不含正文和文档内容；
- 用真实中文、英文、代码、日志和表格场景校准 800/2,000 字阈值；
- 确认没有 Agent 通过多条消息绕过限制。

## 12. 测试计划

### 12.1 Tool 单元测试

- `chat_document_create` 文件名清洗、UTF-8、空内容、大小和数量限制；
- 文档引用跨 Run、跨 Agent、伪造和重复消费；
- 四个发送工具带零个、一个和多个 `document_refs`；
- 2,000 字边界与 `message_too_long` 结构化错误；
- 运行时校验不依赖 JSON Schema；
- 文档消息触发 delivery 的行为与普通消息一致。

### 12.2 Store 与迁移测试

- 旧数据库升级后附件仍可读；
- 本地附件与消息事务原子性；
- 同步状态迁移与失败重试；
- 删除、orphan 和 outbox 幂等；
- 哈希或本地文件被篡改时拒绝发送。

### 12.3 后端测试

- 未登录、跨账户读取和猜测 artifact ID 全部拒绝；
- 客户端不能指定 bucket/object key；
- 重复幂等 key 不制造多个对象；
- 大小、MIME、哈希不匹配被拒绝；
- staged artifact TTL 清理；
- 过期授权可重新获取，不依赖永久预签名 URL。

### 12.4 UI 与性能测试

- 群聊、Human–Agent 私聊、Agent–Agent 私聊使用同一附件渲染组件；
- 消息首次进入时默认滚动到底部；
- 10 KB、100 KB、1 MB Markdown 预览；
- 100 条带附件消息的切换和分页；
- MinIO 慢、失败、离线和恢复时 UI 不白屏、不锁主线程；
- 历史 10,000 字正文仍能正常展示且不会阻塞会话列表。

### 12.5 端到端场景

1. Human 要求项目经理给出完整实施方案。
2. 项目经理创建 Markdown 文档。
3. 项目经理在团队群发送不超过阈值的摘要并附文档。
4. Human 点击卡片预览。
5. 被 @ 的 Agent 读取消息附件并按 offset 分段查看细节。
6. 断网重复以上流程，本地协作仍完成。
7. 恢复联网后 artifact 自动同步，另一设备能够预览。

## 13. 完成定义

以下条件全部满足，才可认为本方案完成：

- 所有本地 Agent manager thread 都加载共享精简沟通 Skill；
- 四个消息发送工具均在 Schema 和执行层拒绝超长正文；
- Agent 能创建 Markdown、随消息发送，Human 能在客户端内预览；
- 其他 Agent 能在消息权限范围内分段读取附件；
- 模型始终看不到真实数据库 ID、本机路径、MinIO bucket/object key 和授权 URL；
- MinIO 上传失败不阻塞本地 Agent 协作，并能可靠补传；
- 群聊和两类私聊共用同一附件与 Markdown 渲染逻辑；
- 历史超长消息的性能兜底仍然有效；
- 完成权限、离线、迁移、幂等、性能和端到端测试；
- 文档、测试和代码中的阈值来自统一配置，不再散落硬编码。

## 14. 本轮明确不实施

本轮只新增本文档，不修改以下内容：

- 不新增或修改 Relay MCP 工具；
- 不修改消息长度限制；
- 不修改 Skill 资源或 Prompt 注入；
- 不修改 SQLite schema；
- 不新增后端 API 或 MinIO 数据结构；
- 不修改附件 UI、Markdown 预览或同步逻辑；
- 不提交、推送、安装或重启客户端。

