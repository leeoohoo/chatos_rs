# macOS Agent Run 恢复与工具引用修复进度

- 时间：2026-09-20 16:18:24 CST（Asia/Shanghai）
- 本轮目标：复现并修复 Agent 在客户端暂停、退出或崩溃后恢复 Run 时，旧工具消息引用失效并导致 `todo_add` 连续失败的问题；同时修复同一 Memory 记录因 JSON 表示或毫秒时间往返差异而被误判为历史不一致的问题。
- 起始提交：`3e8e0c37ad4bca049f66184efae302b02415ada0`
- 代码提交：`4ebaed1d0f5e5f6954784f60ff42105e1199c9e7`

## 实际改动

1. 将会话、消息、Todo、团队、Agent、负责人、Plugin、附件和团队资产引用改为按 Run 身份密钥加密认证的 opaque reference；同一 Run 重建工具 Provider 后可继续解析旧引用，不暴露真实数据库 ID。
2. 保留 fail-closed 边界：伪造、篡改、错误类型或其他 Run 的引用仍会被拒绝。
3. 修正 `invalid_source_message_ref` 的恢复指引：触发消息明确引导 `chat_get_trigger`，新收件箱消息引导 `chat_read_all_unread`，避免已读游标推进后反复读取空收件箱。
4. 明确工具描述中的引用生命周期：引用在同一 Run 的暂停、客户端重启和恢复之后仍然有效。
5. Memory 缓存重放由原始 JSON 字节比较改为解码后的结构比较；时间戳按毫秒编码精度容忍最多 1 ms 差异，新记录使用排序键稳定编码。
6. 新增回归覆盖：第一次 Provider 读取并标记消息已读，第二次 Provider 在空收件箱状态下仍可用旧消息、团队、负责人和 Plugin 引用创建 Todo；伪造引用和跨 Run 引用继续失败；等价 Memory JSON 与时间往返继续被接受。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/DurableAgentMemoryService.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoCreateTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatTodoUpdateTools.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolProvider.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentChatToolRegistry.swift`
- `clients/macos/Sources/ChatOSConnector/LocalAgentRunReferenceVault.swift`
- `clients/macos/Tests/ChatOSConnectorTests/DurableAgentMemoryServiceTests.swift`
- `clients/macos/Tests/ChatOSConnectorTests/LocalAgentChatToolProviderTests.swift`

## 业务不变量

- 模型只接触 opaque reference，不获得真实账户、项目、房间、Agent、消息、Todo、Plugin 或资产 ID。
- 引用只能在签发它的同一 Run 身份范围内解析；篡改和跨 Run 复用均拒绝。
- `chat_read_all_unread` 仍保持“返回即标记已读”的既有语义，不回退游标，也不通过重复读取伪造未读。
- `todo_add` 仍要求至少一条可信来源消息，仍校验项目经理、团队、负责人、依赖与执行能力。
- Memory 只接受语义相同的既有记录；ID、索引或消息内容发生真实变化时仍返回历史不一致。
- 未加入或覆盖用户和其他进程的并发工作区改动。

## 验证证据

- `swift test --filter LocalAgentChatToolProviderTests`：5 个测试，0 失败。
- `swift test --filter DurableAgentMemoryServiceTests`：3 个测试，0 失败。
- 恢复专项测试：重建 Provider 后 `chat_read_all_unread` 返回 `message_count=0`，但第一次 Provider 签发的旧引用仍成功创建 Todo；伪造引用及不同 Run 引用均被拒绝。
- `make test-macos-client`：Core 44、Connector 114、App 107、Runtime 49、API 112，以及 Swift Testing 22 + 98 + 52，共 598 个测试，0 失败；2 个显式环境变量性能基准按约定跳过。
- `clients/macos/scripts/package-debug-app.sh`：构建与签名成功。
- `/Applications/ChatOS.app`：`codesign --verify --deep --strict` 通过，应用进程已启动。
- 安装前版本备份：`/Applications/ChatOS.app.before-tool-reference-fix-20260920-161722`。

## 剩余风险

- 本轮验证了可持久实体引用的 Run 恢复语义；`document_ref` 指向尚未发送的临时草稿文件，仍按 Provider 生命周期管理，不在本轮扩大为跨崩溃草稿恢复。
- 已经由旧版本随机 UUID 方式签发、且尚未重新读取的历史引用无法由新格式反解；新版本签发的引用从本轮起具备恢复能力。旧运行若再遇到该情况，可用 `chat_get_trigger` 取得当前触发消息的新引用。

## 下一步

- 由 Human 在已安装的新客户端创建一次新的 Agent 消息处理 Run，必要时中断并继续运行，确认 UI 中不再出现 `invalid_source_message_ref` 连续失败。
- 继续处理 macOS 方案中尚未完成的独立有界工作单元，保持每轮代码提交、进度提交与推送分离。
