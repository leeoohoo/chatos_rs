# 大文件专项重构方案（1000+ 行）

## 1. 目标与范围

本方案面向前后端所有 **1000 行以上** 文件，目标是：

1. 降低单文件职责耦合，避免“一个文件包含业务、状态、UI、协议解析、IO 全部逻辑”。
2. 提炼可复用对象/接口，减少重复实现（特别是 FS、流式事件、请求参数、错误处理）。
3. 建立可渐进迁移路径，优先低风险收益项，再做结构性重构。

审查文件（13 个）：

| 文件 | 行数 |
|---|---:|
| `chat_app_server_rs/src/api/remote_connections.rs` | 3523 |
| `chat_app/src/components/SessionList.tsx` | 2214 |
| `chat_app/src/components/ProjectExplorer.tsx` | 2096 |
| `chat_app/src/components/ChatInterface.tsx` | 1761 |
| `chat_app/src/lib/api/client.ts` | 1657 |
| `chat_app/src/components/McpManager.tsx` | 1498 |
| `chat_app/src/lib/store/actions/sendMessage.ts` | 1368 |
| `chat_app_server_rs/src/services/v3/ai_client/mod.rs` | 1345 |
| `chat_app_server_rs/src/api/fs.rs` | 1255 |
| `chat_app_server_rs/src/api/sessions.rs` | 1089 |
| `chat_app/src/components/InputArea.tsx` | 1041 |
| `chat_app_server_rs/src/services/notepad/store.rs` | 1013 |
| `chat_app/src/components/TerminalView.tsx` | 1006 |

---

## 2. 共性问题（跨文件）

1. 单文件职责过多：路由、业务、状态、协议解析、UI 细节混杂。
2. 重复规范化/校验逻辑：路径、端口、请求参数、响应数据映射分散在多个文件。
3. `any`/`Value` 宽类型过多：接口边界不清晰，回归风险高。
4. 前端存在多处“同类组件重复实现”：文件选择器、目录选择器、确认删除、轮询刷新。
5. 后端缺少统一错误模型：`String` 错误直接返回，HTTP 映射不稳定。
6. 流式协议处理过重：SSE 解析、工具事件、UI 面板状态更新耦合在单函数。

---

## 3. 分文件优化建议

### 3.1 前端

| 文件 | 主要问题 | 拆分建议 | 抽象对象/接口 | 可复用方法 |
|---|---|---|---|---|
| `chat_app/src/components/SessionList.tsx` | 会话/项目/终端/远端 + 多弹窗 + 目录/文件选择器 + 轮询都在一处 | 拆为 `SessionSection`、`ProjectSection`、`TerminalSection`、`RemoteSection`、`RemoteConnectionModal`、`FsDirPickerDialog`、`FsFilePickerDialog` | `SidebarResourceController<T>`、`RemoteConnectionFormState` | `deriveNameFromPath`、`normalizeFsEntry`、`confirmDelete`、`usePollingLoader` |
| `chat_app/src/components/ProjectExplorer.tsx` | 文件树、拖拽移动、预览、diff、变更日志、右键菜单耦合严重 | 拆为 `ProjectTreePane`、`FilePreviewPane`、`ChangeLogPane`、`MoveConflictDialog`；逻辑抽 hook：`useProjectTree`、`useProjectChanges`、`useProjectDragDrop` | `ProjectTreeState`、`ChangeSummaryState`、`MoveConflictPayload` | `normalizePath`、`parseUnifiedDiff`、`normalizeEntry/normalizeFile/normalizeChangeLog` |
| `chat_app/src/components/ChatInterface.tsx` | 页面容器承担 chat/workbar/ui-prompt/summary/modal 全量编排 | 拆 `ChatShellLayout`、`ChatWorkbarContainer`、`SummaryPane`、`UiPromptHistoryDrawer`；逻辑抽 `useWorkbarTasks`、`useUiPromptHistory` | `WorkbarFacade`、`UiPromptHistoryService`、`SummaryPaneState` | `formatSummaryCreatedAt`、`formatUiPromptStatus`、任务 mutation 刷新策略 |
| `chat_app/src/lib/api/client.ts` | 单一巨型 API 类 + domain 混杂 + query 拼接重复 + `any` 大量存在 | 拆 `httpClient` + domain client（`sessionClient`/`projectClient`/`remoteClient`/`notepadClient`...） | `HttpTransport`、`ApiError`、`PaginatedQuery`、`ApiResult<T>` | `buildQueryParams`、`parseErrorResponse`、`downloadWithFilename` |
| `chat_app/src/components/McpManager.tsx` | CRUD、动态配置、权限、Git 导入、插件安装、大弹窗都在一个组件 | 拆 `McpServerList`、`McpServerForm`、`BuiltinMcpSettingsModal`（再拆 tab）+ `useBuiltinMcpSettings` | `McpPermissionState`、`PluginInstallSummary` | `normalizeIdList`、`pluginInstalledTotal`、`pluginDiscoverableTotal` |
| `chat_app/src/lib/store/actions/sendMessage.ts` | 单函数内做附件处理+SSE解析+工具事件+store状态收敛 | 拆 `attachmentEncoder.ts`、`sseEventParser.ts`、`streamEventReducer.ts`、`toolEventAdapters.ts` | `ChatStreamEvent`（判别联合类型）、`StreamingMessageDraft`、`ToolCallUpdate` | `joinStreamingText`、`normalizeStreamedText`、`extractTaskReviewPanelFromToolStream` |
| `chat_app/src/components/InputArea.tsx` | 输入、附件、项目文件选择、AI选择器、拖拽粘贴同层维护 | 拆 `AiSelectorChip`、`ProjectFilePicker`、`AttachmentTray`，逻辑抽 `useAttachmentManager`、`useProjectFilePicker` | `AttachmentPolicy`、`ProjectFilePickerState` | `isFileTypeAllowed`、`fuzzyMatch`、`toRelativeProjectPath` |
| `chat_app/src/components/TerminalView.tsx` | xterm 生命周期、WS连接、命令解析、历史侧栏混在一起 | 拆 `useTerminalSession`、`useCommandHistoryParser`、`TerminalHeader`、`CommandHistoryPanel` | `TerminalTransport`、`CommandHistoryItem`、`HistoryLoader` | `parseOutputChunkForCommands`、`parseInputChunkForCommands`、`mergeCommandHistory` |

### 3.2 后端

| 文件 | 主要问题 | 拆分建议 | 抽象对象/接口 | 可复用方法 |
|---|---|---|---|---|
| `chat_app_server_rs/src/api/remote_connections.rs` | 路由 + SSH连接 + WS会话 + SFTP传输 + 校验 + 路径工具过度集中 | 拆 `api/remote_connections/routes.rs`、`services/remote/session_service.rs`、`services/remote/transfer_service.rs`、`services/remote/ssh_builder.rs`、`services/remote/path_utils.rs` | `RemoteSessionService`、`TransferService`、`ConnectionSpec`、`TransferJob` | `normalize_*` 请求校验、`build_ssh_args`、远端路径 join/parent 逻辑 |
| `chat_app_server_rs/src/services/v3/ai_client/mod.rs` | 请求重试策略、上下文组装、工具循环、错误回退在单循环中 | 拆 `request_loop.rs`、`context_builder.rs`、`recovery_policy.rs`、`tool_orchestrator.rs` | `RecoveryAction`、`ContextBuildOptions`、`AiProviderAdapter` | `truncate_function_call_outputs_in_input`、usage 日志、system->user 重写 |
| `chat_app_server_rs/src/api/fs.rs` | 路由、FS 操作、搜索、下载压缩、响应构造同文件 | 拆 `api/fs/routes.rs`、`services/fs/file_ops.rs`、`services/fs/search.rs`、`services/fs/download.rs`、`api/fs/response.rs` | `FsService`、`FsSearchService`、`FsError` | `is_valid_entry_name`、`json_error_response`、`read_dir_entries` |
| `chat_app_server_rs/src/api/sessions.rs` | CRUD + MCP 绑定 + 消息展示裁剪 + turn process + summary 接口耦合 | 拆 `routes_crud.rs`、`routes_messages.rs`、`message_projection.rs`、`routes_summary.rs` | `MessageProjectionService`、`TurnProcessExtractor` | `parse_tool_calls_value`、`extract_content_segments`、`build_compact_history_messages` |
| `chat_app_server_rs/src/services/notepad/store.rs` | 路径规则、文件锁、索引重建、CRUD、搜索全部耦合 | 拆 `path_rules.rs`、`file_lock.rs`、`index_repository.rs`、`note_repository.rs`、`notepad_service.rs` | `IndexRepository`、`NoteRepository`、`NotepadError` | folder/tag/title 规范化、`atomic_write_text`、索引 normalize |

---

## 4. 可抽象对象与接口（建议优先落地）

### 4.1 前端

1. `AiSelectionScope`  
   字段：`selectedModelId`、`selectedAgentId`、`sessionId`  
   用途：统一会话级 AI 选择读写，避免组件间重复拼接逻辑。

2. `ResourceListController<T>`  
   方法：`load`、`create`、`select`、`remove`、`refresh`  
   用途：统一 SessionList 中 project/terminal/remote 的列表操作模式。

3. `ChatStreamEvent`（判别联合）  
   事件：`chunk`、`thinking`、`tools_start`、`tools_stream`、`tools_end`、`done`、`error`、`cancelled`  
   用途：把 `sendMessage.ts` 从“if-else 链”转为“事件路由 + reducer”。

4. `FsEntryMapper` / `FsPath` 工具集  
   用途：InputArea、SessionList、ProjectExplorer 共享路径规范与条目映射。

### 4.2 后端

1. `ApiError`（统一错误模型）  
   字段：`code`、`message`、`details`、`http_status`  
   用途：替换散落 `Result<T, String>` 到 HTTP 响应的直连方式。

2. `RemoteSessionService` / `TransferService`  
   用途：remote_connections 将 WS 生命周期与传输任务调度解耦。

3. `MessageProjectionService`  
   用途：sessions 消息展示逻辑统一，减少 endpoint 内重复处理。

4. `IndexRepository` + `NoteRepository`  
   用途：notepad store 分离“索引维护”和“文件读写”职责，便于测试与并发控制。

---

## 5. 可共用方法清单（建议建立 shared 模块）

1. 路径/名称规范化  
   前端：`normalizePath`、`toRelativeProjectPath`、`isHiddenProjectPath`  
   后端：`normalize_folder_path`、`is_valid_entry_name`、远端 path join/parent。

2. 查询参数构建  
   `buildQueryParams(params: Record<string, unknown>)`，替代散落的 `URLSearchParams` 拼接。

3. 错误消息归一化  
   前端 `parseErrorResponse` + 后端 `ApiError -> HTTP` 映射。

4. 文件条目映射  
   `normalizeFsEntry` / `normalizeFile` 统一 camel/snake 转换。

5. 流式文本拼接  
   `joinStreamingText` + `normalizeStreamedText` 独立复用，避免多个组件重复处理。

---

## 6. 三阶段实施路线（建议）

### Phase 1（低风险快拆，1~1.5 周）

目标：不改业务行为，只抽纯函数和 UI 子组件。

1. 前端抽出共用 util：`fsPath`、`queryBuilder`、`streamText`。
2. `SessionList`、`InputArea`、`McpManager` 先做组件拆分，不动接口协议。
3. `client.ts` 先拆 domain 文件，保留同名导出，避免上层改动过大。

验收：

1. 行为回归一致（人工回归 + 现有测试）。
2. 单文件目标降到 `<800` 行（优先前端）。

### Phase 2（结构重构，2~3 周）

目标：抽状态编排层、服务层和协议层。

1. 前端：`sendMessage.ts` 迁移为 `event parser + reducer + adapters`。
2. 后端：`remote_connections.rs` / `sessions.rs` / `fs.rs` 拆 routes 与 services。
3. `ai_client/mod.rs` 拆 request loop 与 recovery policy，减少主循环复杂度。

验收：

1. 关键大文件降到 `<600` 行（remote_connections 除外，先降到 `<1200`）。
2. 关键流程单测覆盖（SSE 解析、路径校验、message projection、transfer 状态机）。

### Phase 3（架构固化，1~2 周）

目标：统一接口契约与错误模型，形成长期可维护结构。

1. 引入 `ApiError` 与标准响应包装。
2. notepad 拆 `IndexRepository/NoteRepository` 并补并发写入测试。
3. 完成共享模块迁移并删除旧重复实现。

验收：

1. 所有 1000+ 文件降至 `<700` 行（个别编排文件可例外但需有说明）。
2. 重复 util 删除率 > 60%（以函数签名统计）。

---

## 7. 风险与控制

1. 风险：拆分后状态同步差异导致 UI 回归。  
   控制：先引入 facade 层，再替换组件内部实现；每步保留快照测试。

2. 风险：流式协议重构导致工具调用显示异常。  
   控制：先固定事件模型（`ChatStreamEvent`），编写事件回放测试样本。

3. 风险：后端拆路由时错误码变化。  
   控制：先冻结现有响应 schema，再逐步替换内部实现。

4. 风险：远端连接链路（SSH/SFTP）改造影响稳定性。  
   控制：先提取 builder/validator，不改连接策略；最后再做会话服务化。

---

## 8. 建议的首批改造清单（下一个迭代）

1. `client.ts` 按 domain 拆文件，保留 `apiClient` 兼容导出。
2. `SessionList.tsx` 拆 `RemoteConnectionModal` 与 `FsDirPickerDialog`。
3. `sendMessage.ts` 提取 `sseEventParser.ts` 与 `streamText.ts`（零业务改动）。
4. `api/fs.rs` 抽 `FsError + response` 与 `search` 工具函数模块。
5. `services/notepad/store.rs` 抽 `path_rules.rs` + `file_lock.rs`。

完成这 5 项后，再进入 Phase 2 的大文件主干拆分。

---

## 9. 最新实施进展（2026-03-09）

本轮已完成（含之前已落地项）：

1. `chat_app/src/lib/store/actions/sendMessage.ts`  
   已拆分 `internalId.ts` / `sse.ts` / `streamText.ts` / `toolPanels.ts` / `attachments.ts`，主文件降至 `991` 行。

2. `chat_app/src/components/TerminalView.tsx`  
   已拆分 `commandHistory.ts` / `themeTransport.ts`，主文件降至 `637` 行。

3. `chat_app/src/components/InputArea.tsx`  
   已抽 `inputArea/fileUtils.ts`，主文件降至 `987` 行。

4. `chat_app_server_rs/src/api/fs.rs`  
   已拆出 `response.rs` / `search.rs` / `read_mode.rs` / `roots.rs`，主文件降至 `990` 行。

5. `chat_app_server_rs/src/services/notepad/store.rs`  
   已拆出 `store_normalize.rs` / `store_lock.rs`，主文件降至 `839` 行。

6. `chat_app_server_rs/src/api/sessions.rs`（本轮新增）  
   已拆出 `api/sessions/history.rs`（消息投影与历史裁剪逻辑），主文件由 `1089` 行降至 `625` 行。

7. `chat_app/src/components/ProjectExplorer.tsx`（本轮新增）  
   已拆出 `components/projectExplorer/utils.ts`、`components/projectExplorer/ChangeLogPanels.tsx`、`components/projectExplorer/Overlays.tsx`，主文件由 `2096` 行降至 `1633` 行。

8. `chat_app/src/components/SessionList.tsx`（本轮新增）  
   已拆出 `components/sessionList/helpers.ts`、`components/sessionList/Pickers.tsx`、`components/sessionList/RemoteConnectionModal.tsx`、`components/sessionList/Sections.tsx`，主文件由 `2214` 行降至 `1335` 行。

9. `chat_app/src/lib/api/client.ts`（本轮新增）  
   已拆出 `lib/api/client/shared.ts`、`lib/api/client/workspace.ts`、`lib/api/client/configs.ts`，主文件由 `1665` 行降至 `1405` 行，保持 `apiClient`/`conversationsApi` 兼容导出不变。

10. `chat_app/src/lib/api/client.ts`（本轮继续）  
   进一步拆出 `lib/api/client/conversation.ts`、`lib/api/client/stream.ts`、`lib/api/client/tasks.ts`、`lib/api/client/notepad.ts`、`lib/api/client/summary.ts`、`lib/api/client/account.ts`，主文件由 `1405` 行降至 `995` 行，保持 `ApiClient` 公共方法与 `apiClient`/`conversationsApi` 兼容导出不变。

11. `chat_app/src/components/ProjectExplorer.tsx`（本轮继续）  
   新增 `components/projectExplorer/TreePane.tsx`，将左侧目录树与拖拽面板整体抽离，主文件由 `1633` 行降至 `1324` 行，保持拖拽移动、右键菜单、变更标记与自动滚动行为不变。

12. `chat_app/src/components/ProjectExplorer.tsx`（本轮继续）  
   新增 `components/projectExplorer/PreviewPane.tsx`，将右侧文件预览与 Diff 顶栏抽离，主文件由 `1324` 行降至 `1229` 行，保持文本高亮/图片预览/二进制下载与错误展示行为不变。

13. `chat_app/src/components/ProjectExplorer.tsx`（阶段记录 A）  
   新增 `components/projectExplorer/useProjectTreeActions.ts`，将创建/删除/下载/确认变更/移动冲突处理逻辑抽离，主文件由 `1229` 行降至 `917` 行。

14. `chat_app/src/components/SessionList.tsx`（阶段记录 B）  
   新增 `components/sessionList/useRemoteConnectionForm.ts`，将远端连接表单状态与测试/保存逻辑抽离，主文件由 `1335` 行降至 `1088` 行。

15. `chat_app/src/components/SessionList.tsx`（阶段记录 C）  
   新增 `components/sessionList/CreateResourceModals.tsx` 与 `components/sessionList/useSessionSummaryStatus.ts`，并将 `formatTimeAgo`/`getSessionStatus` 下沉到 `sessionList/helpers.ts`，主文件由 `1088` 行降至 `988` 行。

16. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 D）  
   新增 `api/remote_connections/request_normalize.rs` 与 `api/remote_connections/path_utils.rs`，拆出请求归一化与远端路径工具，主文件由 `3523` 行降至 `3202` 行。

17. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 E）  
   新增 `api/remote_connections/host_keys.rs`，拆出 host key 策略与 known_hosts 写入逻辑，主文件由 `3202` 行降至 `3080` 行。

18. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 F）  
   新增 `api/remote_connections/jump_tunnel.rs` 与 `api/remote_connections/transfer_helpers.rs`，拆出跳板隧道桥接与 SFTP/SCP 传输辅助逻辑，主文件由 `3080` 行降至 `2357` 行；`cargo check` 通过。

19. `chat_app/src/components/ChatInterface.tsx`（阶段记录 G）  
   新增 `components/chatInterface/SummaryPane.tsx` 与 `components/chatInterface/UiPromptHistoryDrawer.tsx`，并将 `UiPromptHistoryItem` 类型下沉到 `components/chatInterface/types.ts`，主文件由 `1761` 行降至 `1574` 行；`npm run type-check` 通过。

20. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 H）  
   新增 `api/remote_connections/ssh_command.rs`，拆出 SSH/SCP 参数构建与命令启动辅助（`build_ssh_args` / `build_scp_args` / `is_password_auth` / `map_command_spawn_error` 等），主文件由 `2357` 行降至 `2150` 行；`cargo check` 通过。

21. `chat_app/src/components/McpManager.tsx`（阶段记录 I）  
   新增 `components/mcpManager/BuiltinSettingsModal.tsx` 与 `components/mcpManager/icons.tsx`，将内置 MCP 设置弹窗与图标组件整体下沉，主文件由 `1498` 行降至 `999` 行；`npm run type-check` 通过。

22. `chat_app/src/components/ChatInterface.tsx`（阶段记录 J）  
   新增 `components/chatInterface/helpers.ts`，将 `UiPrompt` 记录归一化与时间格式化纯函数下沉，主文件由 `1574` 行降至 `1496` 行；`npm run type-check` 通过。

23. `chat_app/src/components/ChatInterface.tsx`（阶段记录 K）  
   新增 `components/chatInterface/HeaderBar.tsx`、`components/chatInterface/ChatComposerPanel.tsx` 与 `components/chatInterface/usePanelActions.ts`，将头部用户菜单、底部 Workbar/输入区与任务审核/交互确认提交动作抽离，主文件由 `1496` 行降至 `1226` 行；`npm run type-check` 通过。

24. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 L）  
   新增 `api/remote_connections/net_utils.rs` 与 `api/remote_connections/ssh_auth.rs`，拆出 TCP 连接/超时配置与 SSH 认证流程，主文件由 `2150` 行降至 `2016` 行；`cargo check` 通过。

25. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 M）  
   新增 `api/remote_connections/transfer_manager.rs` 与 `api/remote_connections/terminal_io.rs`，并将 `normalize_transfer_direction` 下沉到 `request_normalize.rs`，主文件由 `2016` 行降至 `1799` 行；`cargo check` 通过。

26. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 N）  
   新增 `api/remote_connections/remote_terminal.rs`，将远端终端 WS 会话生命周期、事件转发与断连管理下沉，主文件由 `1799` 行降至 `1315` 行；`cargo check` 通过。

27. `chat_app/src/components/ChatInterface.tsx`（阶段记录 O）  
   新增 `components/chatInterface/useWorkbarMutations.ts`，并将任务变更判定/提取/归一化纯函数下沉到 `components/chatInterface/helpers.ts`，主文件由 `1226` 行降至 `850` 行；`npm run type-check` 通过。

28. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 P）  
   新增 `api/remote_connections/remote_sftp.rs`，将 SFTP 列表/上传下载/传输任务/目录操作路由处理整体下沉，主文件由 `1315` 行降至 `750` 行；`cargo check` 通过。

29. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`（阶段记录 Q）  
   新增 `services/v3/ai_client/compat.rs` 与 `services/v3/ai_client/stateless_context.rs`，将输入兼容/裁剪工具与无状态上下文构建逻辑拆出，主文件由 `1345` 行降至 `835` 行；`cargo check` 通过。

30. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`（阶段记录 R）  
   新增 `services/v3/ai_client/recovery_policy.rs`，将 `process_with_tools` 中请求失败/完成失败的恢复策略分支下沉为独立策略方法（`try_recover_from_request_error` / `try_recover_from_completion_error`），主文件由 `835` 行降至 `636` 行；`cargo check` 通过。

31. `chat_app_server_rs/src/api/remote_connections/remote_sftp.rs` + `transfer_helpers.rs`（阶段记录 S）  
   引入 typed error 收敛：`transfer_helpers.rs` 新增 `TransferJobError` 与 typed 包装接口（`run_sftp_transfer_job_typed` / `run_scp_upload_typed` / `run_scp_download_typed` / `estimate_local_total_bytes_typed`）；`remote_sftp.rs` 新增 `RemoteSftpApiError` 并统一错误到 HTTP 响应映射，传输取消分支改为 `err.is_cancelled()` 判定，减少字符串分支耦合；`cargo check` 通过。

32. 回归测试（阶段记录 T）  
   补充并执行最小回归测试：
   - `services::v3::ai_client::compat::tests::keeps_small_function_call_output_items_unchanged`
   - `api::remote_connections::remote_sftp::tests::maps_bad_request_error_to_response`
   - `api::remote_connections::remote_sftp::tests::maps_not_found_error_to_response`
   全部通过。

33. `chat_app_server_rs/src/api/remote_connections/transfer_helpers.rs`（阶段记录 U）  
   typed error 继续下沉：`check_transfer_not_cancelled`、递归上传/下载、SFTP 统计与 SCP 执行路径内部全部切换为 `Result<_, TransferJobError>`，并删除旧的字符串结果桥接函数（`run_sftp_transfer_job` / `run_scp_upload` / `run_scp_download`）；`cargo check` 通过。

34. `chat_app_server_rs/src/services/v3/ai_client/recovery_policy.rs`（阶段记录 V）  
   新增最小纯函数回归测试：
   - `merges_unique_calls_and_matching_outputs_only`
   - `skips_outputs_when_no_pending_calls`
   两项均通过，固定了 pending tool calls/outputs 合并行为。

35. `chat_app_server_rs/src/api/remote_connections/remote_sftp.rs`（阶段记录 W）  
   新增输入边界校验抽象（`require_non_empty_field` / `ensure_local_target_parent_dir_exists` / `validate_mkdir_name`），并补齐测试覆盖：
   - `local_path/remote_path` 空值
   - 本地目标目录不存在
   - 非法目录名（`.`, `..`, 含 `/` 或 `\\`）
   同时补充 `RemoteSftpApiError` 状态码映射测试，新增 `TransferJobError::Timeout -> 408 REQUEST_TIMEOUT`；定向测试通过。

36. `chat_app_server_rs/src/services/v3/ai_client/recovery_policy.rs`（阶段记录 X）  
   新增策略回放纯函数 `replay_request_error_policy`，并将请求恢复主流程接入该策略结果，补齐错误样本回放测试：
   - `prev_id` 禁用（`previous_response_id` 被 provider 拒绝）
   - `history_limit` 递减（`context_length_exceeded`）
   - `input must be a list`
   定向测试通过。

37. `chat_app_server_rs/src/api/remote_connections/transfer_helpers.rs`（阶段记录 Y）  
   `TransferJobError` 细分为 `Cancelled | Timeout | Io | Remote | Message`，将 SSH/SFTP 协议相关失败（连接、SFTP 初始化、远端文件/目录读写、SCP stderr）统一归类为 `Remote`，本地文件系统读写保持 `Io`；
   新增分类测试覆盖取消、IO、超时、远端协议四类，验证 `is_cancelled` 与错误文本输出；定向测试通过。

38. `chat_app_server_rs/src/api/remote_connections/remote_sftp.rs` + `chat_app/src/components/RemoteSftpPanel.tsx`（阶段记录 Z）  
   落地结构化业务错误码与前端统一映射：
   - 后端 `RemoteSftpApiError` 改为 `{ error, code }` 响应结构，覆盖 `invalid_argument` / `invalid_path` / `invalid_directory_name` / `transfer_not_found` / `transfer_not_active` / `transfer_cancelled` / `timeout` / `local_io_error` / `remote_error`。
   - 前端 `ApiClient` 请求异常改为抛出携带 `status/code/payload` 的 `ApiRequestError`。
   - `RemoteSftpPanel` 新增 `resolveSftpErrorMessage` 统一按 `code` 映射展示，减少依赖后端错误文案。

39. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`（阶段记录 AA）  
   新增集成级策略回归（mock provider 返回序列 + 本地 HTTP stub）：
   - `recovers_prev_id_then_completion_overflow_and_succeeds`
   - `recovers_input_must_be_list_and_retries_with_list_payload`
   覆盖真实 `process_with_tools` 循环中的恢复组合顺序，验证 `prev_id` 降级、`input must be list` 重试、完成态失败后的再次恢复链路。

40. 本轮验证（阶段记录 AB）  
   执行并通过：
   - `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml remote_sftp::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml transfer_helpers::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml services::v3::ai_client::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml recovery_policy::tests::`
   - `npm run type-check`（`chat_app`）

当前仍超 1000 行的优先文件：

1. 无（本轮范围内已全部降到 `<1000` 行）。

41. `chat_app_server_rs/src/api/remote_connections.rs` + `chat_app/src/lib/api/remoteConnectionErrors.ts`（阶段记录 AC）  
   将远端连接 connect/test 与 WS 错误统一为 `{ error, code }` 范式并接入前端统一映射：
   - 后端新增连接测试错误分类（`host_key_mismatch` / `host_key_untrusted` / `host_key_verification_failed` / `auth_failed` / `dns_resolve_failed` / `network_timeout` / `network_unreachable` / `connectivity_test_failed`），`/api/remote-connections/test` 与 `/api/remote-connections/:id/test` 统一返回结构化错误。
   - WS `WsOutput::Error` 增加 `code` 字段，并对终端初始化/输入/resize/消息格式/认证/网络等错误分类（`terminal_init_failed` / `terminal_input_failed` / `terminal_resize_failed` / `invalid_ws_message` 等）。
   - 前端新增 `lib/api/remoteConnectionErrors.ts`，`useRemoteConnectionForm.ts` 与 `RemoteTerminalView.tsx` 统一按 `code` 映射展示，减少仅依赖文案判断。

42. `chat_app_server_rs/src/api/remote_connections/transfer_helpers.rs` + `remote_sftp.rs` + `chat_app/src/components/RemoteSftpPanel.tsx`（阶段记录 AD）  
   完成 `Remote` typed error 子码收敛并打通前端展示：
   - `transfer_helpers.rs` 引入并稳定 `RemoteTransferErrorCode`（`AuthFailed` / `PathNotFound` / `PermissionDenied` / `NetworkDisconnected` / `Protocol`），SCP/SFTP 路径统一通过分类函数落码。
   - `remote_sftp.rs` 将 `TransferJobError::Remote { code, message }` 映射到 API 业务码与状态码（网络中断映射 408，其余映射 400）。
   - `RemoteSftpPanel.tsx` 扩展错误码映射（`remote_auth_failed` / `remote_path_not_found` / `remote_permission_denied` / `remote_network_disconnected`）。

43. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`（阶段记录 AE）  
   新增工具调用链路集成回归 `recovers_missing_tool_call_output_with_pending_tool_items_merged`（mock provider 序列）：
   - 首轮返回 `function_call`；
   - 次轮返回 `No tool call found ... function_call_output` 触发恢复；
   - 三轮验证禁用 `previous_response_id` 且 stateless 输入中合并了 `function_call + function_call_output`，最终成功完成。

44. 本轮验证（阶段记录 AF）  
   执行并通过：
   - `cargo fmt --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml remote_connections::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml remote_sftp::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml transfer_helpers::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml services::v3::ai_client::tests::`
   - `npm run type-check`（`chat_app`）

45. `chat_app_server_rs/src/api/remote_connections.rs`（阶段记录 AG）  
   将远端连接 CRUD 入口补齐 `{error, code}` 一致性：
   - `create_remote_connection`：参数归一化失败返回 `invalid_argument`，持久化失败返回 `remote_connection_create_failed`。
   - `update_remote_connection`：参数归一化失败返回 `invalid_argument`，更新失败返回 `remote_connection_update_failed`，回读失败返回 `remote_connection_fetch_failed`，不存在返回 `remote_connection_not_found`。
   - `delete_remote_connection`：删除失败返回 `remote_connection_delete_failed`。
   - 新增 `internal_error_response` 统一 500 错误结构，避免重复拼装。

46. `chat_app/src/lib/api/remoteConnectionErrors.ts` + `SessionList.tsx`（阶段记录 AH）  
   新增统一 `code -> UX action` 层并接入远端连接入口：
   - 在 `remoteConnectionErrors.ts` 中新增 `REMOTE_CONNECTION_ERROR_CODE_ACTIONS` 与 `resolveRemoteConnectionErrorFeedback` / `formatRemoteConnectionErrorFeedback`，将错误文案与操作建议解耦。
   - 补充 CRUD 类 code 映射（`remote_connection_create_failed` / `remote_connection_update_failed` / `remote_connection_fetch_failed` / `remote_connection_delete_failed`）。
   - `resolveRemoteConnectionErrorMessage` / `resolveRemoteTerminalWsErrorMessage` 统一复用反馈层输出，默认附带建议。
   - `SessionList.tsx` 的远端连接删除失败从纯控制台日志改为弹窗提示，统一按 code 映射用户可执行建议。

47. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`（阶段记录 AI）  
   补充流式（SSE）工具恢复回归 `recovers_missing_tool_call_output_in_stream_mode_with_pending_items_merged`：
   - mock provider 第一轮返回 SSE `response.completed` 且携带 `function_call`；
   - 第二轮返回 `No tool call found ... function_call_output` 触发恢复策略；
   - 第三轮返回 SSE 增量文本 + `response.completed`，验证恢复成功；
   - 断言 `stream=true` 请求路径下也会禁用 `previous_response_id` 并合并 pending `function_call + function_call_output`。

48. 本轮验证（阶段记录 AJ）  
   执行并通过：
   - `cargo fmt --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml remote_connections::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml remote_sftp::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml transfer_helpers::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml services::v3::ai_client::tests::`
   - `npm run type-check`（`chat_app`）

49. `chat_app_server_rs/src/core/user_scope.rs`（阶段记录 AK）  
   将用户范围校验错误升级为结构化响应，消除 `resolve_user_id` 盲区：
   - `ensure_user_id_matches` 的拒绝路径改为 `{ "error": "...", "code": "user_scope_forbidden" }`。
   - 新增 `user_scope_forbidden_response` 统一封装。
   - 补充 `core::user_scope::tests`，覆盖 mismatch 结构化返回与默认 user_id 解析路径。

50. `chat_app/src/hooks/useConfirmDialog.ts` + `chat_app/src/components/ui/ConfirmDialog.tsx` + `chat_app/src/components/sessionList/useRemoteConnectionForm.ts` + `RemoteConnectionModal.tsx`（阶段记录 AL）  
   远端连接弹窗新增“建议操作”分层展示：
   - `useConfirmDialog` / `ConfirmDialog` 扩展 `description/details` 双层文案字段，为确认类弹窗提供分层展示能力。
   - `useRemoteConnectionForm` 从 `resolveRemoteConnectionErrorFeedback` 读取 `{ message, action }`，新增 `remoteErrorAction` 状态，避免将建议拼接进错误文案。
   - `RemoteConnectionModal` 新增独立视觉区块“建议操作”，与错误信息分层，提升长文案可读性。
   - `SessionList.tsx` 透传 `remoteErrorAction` 到弹窗组件，并在删除失败确认框中使用 `description/details` 展示错误与建议。

51. `chat_app_server_rs/src/services/v3/ai_client/recovery_policy.rs` + `mod.rs`（阶段记录 AM）  
   流式恢复补强 `response.failed` 分支：
   - `try_recover_from_completion_error` 升级为 `&mut self`，并接入 `pending_tool_calls/pending_tool_outputs`。
   - 新增 completion 失败时的 `missing tool call` 恢复策略：在 `use_prev_id` 场景禁用 `previous_response_id`，回退 stateless 并合并 pending tool call/output。
   - 新增 SSE 回归 `recovers_stream_response_failed_missing_tool_call_without_completed_event`，覆盖 provider 仅发 `response.failed(error)` 且无 `response.completed` 的路径。

52. 本轮验证（阶段记录 AN）  
   执行并通过：
   - `cargo fmt --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml core::user_scope::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml remote_connections::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml services::v3::ai_client::tests::`
   - `npm run type-check`（`chat_app`）

53. `chat_app/src/lib/api/remoteConnectionErrors.test.ts`（阶段记录 AO）  
   为远端连接错误映射补充前端单元测试（Vitest）：
   - 覆盖 `ApiRequestError` code 到 `message/action` 的映射；
   - 覆盖 `formatRemoteConnectionErrorFeedback` 拼接行为；
   - 覆盖未知 code fallback；
   - 覆盖 WS 错误映射；
   - 覆盖 `resolveRemoteConnectionErrorMessage` 的组合输出；
   同时修复 `src/lib/utils/index.ts` 的 `debounce` 定时器类型为 `ReturnType<typeof setTimeout>`，确保 `npm run type-check` 与测试并行稳定。

54. `chat_app_server_rs/src/services/v3/ai_client/mod.rs` + `chat_app_server_rs/src/services/v3/ai_request_handler/parser.rs`（阶段记录 AP）  
   补充 SSE 多事件混合失败回归并增强解析稳健性：
   - 新增回归 `recovers_stream_error_and_failed_without_status_with_pending_items`，覆盖 `error` + `response.failed` + 无 `status` 的异常序列，验证 completion 恢复策略仍能稳定从 prev-id 回退到 stateless 并合并 pending tool item。
   - `apply_stream_event` 在 `response.failed` 事件中显式设置 `finish_reason=failed`，避免 provider 未返回 `status` 时无法进入 completion 失败恢复分支。
   - 新增解析层测试 `apply_stream_event_marks_failed_finish_reason_without_status` 锁定该行为。

55. `chat_app/src/lib/api/remoteConnectionErrors.test.ts`（阶段记录 AQ）  
   扩展错误映射单测覆盖：
   - 新增关键 code 列表断言（critical codes）验证 message/action 非 fallback；
   - 保留未知 code fallback 测试，形成“关键码必映射 + 非关键码可降级”双层保护。

56. 本轮验证（阶段记录 AR）  
   执行并通过：
   - `cargo fmt --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml services::v3::ai_client::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml ai_request_handler::parser::tests::`
   - `cargo test --manifest-path chat_app_server_rs/Cargo.toml core::user_scope::tests::`
   - `npm run type-check`（`chat_app`）
   - `npx vitest run src/lib/api/remoteConnectionErrors.test.ts`（6 tests）

57. `chat_app_server_rs/docs/remote_connection_error_codes.json` + `chat_app/src/lib/api/remoteConnectionErrors.test.ts`（阶段记录 AS）  
   新增“后端 code 清单”对齐自动校验：
   - 后端导出 `docs/remote_connection_error_codes.json`，包含 `remote_connection_codes` 与 `remote_sftp_codes`。
   - 前端单测引入该 JSON，断言 `remote_connection_codes` 中每个 code 在 `REMOTE_CONNECTION_ERROR_CODE_MESSAGES` 与 `REMOTE_CONNECTION_ERROR_CODE_ACTIONS` 中均有映射。
   - 新增后端 code 但未更新前端映射时，测试会自动失败，替代人工约定。

58. `chat_app/src/hooks/useConfirmDialog.ts` + `chat_app/src/components/ui/ConfirmDialog.tsx` + `chat_app/src/components/SessionList.tsx`（阶段记录 AT）  
   确认弹窗复用能力增强：
   - `useConfirmDialog` 增加 `detailsTitle` / `detailsLines` 字段，并保持 `details` 兼容。
   - `ConfirmDialog` 新增 `detailsTitle`（默认 `详情/建议操作`）和多段 `detailsLines` 展示，支持分层文案输出。
   - `SessionList` 删除远端连接失败弹窗改为 `description + detailsTitle + detailsLines`，将“错误说明”和“建议操作”分层展示。

59. `chat_app_server_rs/src/services/v3/ai_client/mod.rs`（阶段记录 AU）  
   新增 SSE 多轮工具调用恢复回归 `recovers_stream_with_second_tool_call_without_pending_duplication`：
   - 模拟第一轮 `function_call` -> 中途 `error + response.failed` -> 恢复后第二轮 `function_call` -> 最终成功。
   - 断言恢复后 stateless 输入中的 pending item 合并不重复膨胀：第一轮与第二轮 `function_call/function_call_output` 均保持单份。

60. 本轮验证（阶段记录 AV）  
   执行并通过：
   - `cargo fmt`
   - `cargo check -q`
   - `cargo test -q services::v3::ai_client::tests:: -- --nocapture`
   - `cargo test -q services::v3::ai_request_handler::parser::tests:: -- --nocapture`
   - `npm run -s type-check`（`chat_app`）
   - `npx vitest run src/lib/api/remoteConnectionErrors.test.ts`（7 tests）

建议下一步（按风险/收益排序）：

1. 将 `remote_connection_error_codes.json` 的维护改为“后端枚举/常量自动导出”，避免手工清单漂移。
2. 为 `ConfirmDialog` 增补组件级单测，覆盖 `details/detailsLines/detailsTitle` 的优先级与渲染回退逻辑。
3. 将后端 `remote_sftp_codes` 也接入前端对齐校验（当前只强约束了 `remote_connection_codes`）。

61. `chat_app_server_rs/src/services/v3/ai_client/mod.rs` + `prev_context.rs` + `ai_request_handler/mod.rs`（阶段记录 AW）  
   修复 MCP/工具链路“无疾而终”核心问题，补齐模型请求失败可见性与重试策略：
   - `AiClient` 主请求循环新增“网络波动/响应解析异常”重试策略：最多重试 5 次（退避），超过预算后返回明确中文错误（包含“已重试 5 次”和最后错误）。
   - `prev_context.rs` 新增 `is_transient_network_error` / `is_response_parse_error` / `is_transient_transport_or_parse_error`，将可重试错误判定从散落字符串判断收敛为策略函数。
   - `AiRequestHandler::handle_stream_request` 对 provider 非 2xx 统一返回 `status + error`；新增“无有效 SSE 事件”解析失败检测，避免空响应被误判为成功。
   - 新增回归测试：
     - `retries_parse_errors_five_times_then_succeeds`
     - `fails_after_five_network_retries_with_explicit_message`
     - `retries_stream_parse_failure_and_then_succeeds`

62. `chat_app/src/lib/store/actions/sendMessage.ts` + `chat_app/src/lib/api/client/stream.ts` + `chat_app_server_rs/src/core/chat_stream.rs`（阶段记录 AX）  
   前端/流事件层补齐失败展示，避免静默中断：
   - `sendMessage.ts` 修复 SSE `error` 事件取值（支持 `message/error/data.message/data.error`），不再丢失后端真实错误原因。
   - SSE JSON 解析失败不再无限吞掉：累计到 5 次后直接失败并展示原因。
   - 流在未收到 `done/complete` 前断开时明确报错（`流式响应在完成前中断`），并将临时助手消息收敛为 `status=error` 与可读失败文案，而不是静默移除。
   - `stream.ts` 非 2xx 时解析后端 `{error, code}`，抛出带状态码/业务码的可读错误。
   - `chat_stream.rs` 的 `error` SSE 事件补充顶层 `message` 与 `data.message`，兼容不同前端解析路径。

63. 本轮验证（阶段记录 AY）  
   执行并通过：
   - `cargo fmt`
   - `cargo check -q`
   - `cargo test -q services::v3::ai_client::tests:: -- --nocapture`（10 passed）
   - `cargo test -q services::v3::ai_client::prev_context::tests:: -- --nocapture`（10 passed）
   - `cargo test -q services::v3::ai_request_handler::parser::tests:: -- --nocapture`（9 passed）
   - `npm run -s type-check`（`chat_app`）
   - `npx vitest run src/lib/api/remoteConnectionErrors.test.ts`（7 tests）

64. `chat_app/src/lib/store/actions/sendMessage.ts` + `chat_app_server_rs/src/services/v2/*`（阶段记录 AZ）  
   针对线上日志 `type=error, data.error=\"error decoding response body\"` 的补强修复：
   - `sendMessage.ts` 将“JSON 解析失败”和“后端 error 事件”分离处理，避免把真实后端错误误归类为“解析流式数据失败”。
   - `sendMessage.ts` 对 `parsed.type === "error"` 直接按后端错误失败，不再进入解析失败计数分支；确保用户看到的错误原因与后端一致。
   - `services/v2/ai_request_handler/mod.rs` 对流式 4xx/5xx 错误统一为 `status + error`，并新增“无有效 SSE 事件”解析失败检测。
   - `services/v2/ai_client/mod.rs` 增加与 v3 一致的“网络波动/响应解析异常”最多 5 次重试（退避），超过预算返回明确中文错误。
   - `services/v2/ai_client/mod.rs` 新增判定函数单测（network/parse/transient 组合）。

65. 补充验证（阶段记录 BA）  
   执行并通过：
   - `npm run -s type-check`（`chat_app`）
   - `cargo fmt`
   - `cargo check -q`
   - `cargo test -q services::v2::ai_client::tests:: -- --nocapture`（5 passed）
   - `cargo test -q services::v2::ai_request_handler::tests:: -- --nocapture`（2 passed）
   - `cargo test -q services::v3::ai_client::tests:: -- --nocapture`（10 passed）
   - `cargo test -q services::v3::ai_client::prev_context::tests:: -- --nocapture`（10 passed）

66. `chat_app_server_rs/src/core/remote_connection_error_codes.rs` + `src/bin/export_remote_connection_error_codes.rs` + `src/api/remote_connections*.rs`（阶段记录 BB）  
   远端连接错误码改为后端常量单一来源并接入自动导出：
   - 新增 `core::remote_connection_error_codes` 作为 `remote_connection_codes` / `remote_sftp_codes` 常量来源，并提供 JSON 导出函数。
   - 新增导出入口 `src/bin/export_remote_connection_error_codes.rs`，支持前端测试前自动生成 `docs/remote_connection_error_codes.json`。
   - `main.rs` 启动时尝试导出 catalog（失败仅告警，不阻断服务）。
   - `api/remote_connections.rs`、`api/remote_connections/remote_sftp.rs`、`api/remote_connections/transfer_helpers.rs` 去除错误码字符串字面量，统一使用常量。
   - `transfer_helpers` 新增 `as_api_code` 映射测试，确保 typed remote 错误码与 API code 保持一致。

67. `chat_app/src/lib/api/remoteConnectionErrors.ts` + `remoteConnectionErrors.test.ts` + `RemoteSftpPanel.tsx` + `components/ui/ConfirmDialog.test.tsx`（阶段记录 BC）  
   前端错误映射和组件回归补齐：
   - `remoteConnectionErrors.ts` 新增 `REMOTE_SFTP_ERROR_CODE_MESSAGES/ACTIONS` 与 `resolveRemoteSftpErrorFeedback/Message`，统一 SFTP 错误映射范式。
   - `RemoteSftpPanel.tsx` 移除本地 SFTP 映射表，统一复用 `resolveRemoteSftpErrorMessage`，减少重复逻辑。
   - `remoteConnectionErrors.test.ts` 改为测试前执行 `cargo run -q --bin export_remote_connection_error_codes` 并读取后端导出 JSON，校验：
     - `remote_connection_codes` 全量有 `message/action` 映射；
     - `remote_sftp_codes` 也全量有 `message/action` 映射（新增强约束）。
   - 新增 `ConfirmDialog.test.tsx`，覆盖 `details/detailsLines/detailsTitle` 的优先级、默认值、自定义标题与 `description -> message` 回退逻辑。

68. 本轮验证（阶段记录 BD）  
   执行并通过：
   - `cargo fmt`（`chat_app_server_rs`）
   - `cargo check -q`（`chat_app_server_rs`）
   - `cargo test -q core::remote_connection_error_codes::tests::`
   - `cargo test -q remote_connections::tests::`
   - `cargo test -q remote_sftp::tests::`
   - `cargo test -q transfer_helpers::tests::`
   - `npm run -s type-check`（`chat_app`）
   - `npx vitest run src/lib/api/remoteConnectionErrors.test.ts`（8 tests）
   - `npx vitest run src/components/ui/ConfirmDialog.test.tsx`（5 tests）
