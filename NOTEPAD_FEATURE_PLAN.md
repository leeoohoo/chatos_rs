# Notepad Feature & Builtin MCP Integration Plan

## 1. Goal

在当前 `chatos_rs` 项目中同时落地两件事：

1. 在 `chat_app_server_rs` 的 builtin MCP 体系里新增一个内置记事本 MCP（Notepad Builtin MCP）。
2. 在当前项目的产品功能里新增“记事本”能力（后端 API + 前端面板），并与 MCP 共享同一套存储与业务逻辑。

## 2. Design Principles

- **单一业务核心**：记事本读写逻辑只实现一份（`services/notepad`），API 与 MCP 都调用它。
- **用户隔离**：按登录用户分区存储；可选按项目进一步分区。
- **文件型存储**：沿用 `notepad_refactor` 的思路，保存 markdown 文件 + index 文件。
- **可恢复性**：index 丢失或损坏时，支持基于磁盘文件重建。
- **渐进式 UI**：先做可用 MVP（列表、搜索、编辑、保存、删除），后续再增强交互。

## 3. Backend Implementation Plan

### 3.1 Notepad Core Service

新增 `chat_app_server_rs/src/services/notepad/`，包含：

- `types.rs`: 请求/响应和内部数据结构（NoteMeta、ListNotesParams 等）
- `paths.rs`: 计算 dataDir（按 user_id + project_id）
- `store.rs`: 文件存储核心（folders/notes/tags/search）
- `mod.rs`: 对外导出服务函数

核心能力：

- init
- list/create/rename/delete folder
- list/create/read/update/delete note
- list tags
- search notes

存储布局（示例）：

- `~/.chatos/notepad/<user_id>/<project_or_global>/notes/*.md`
- `~/.chatos/notepad/<user_id>/<project_or_global>/notes-index.json`
- `~/.chatos/notepad/<user_id>/<project_or_global>/notes.lock`

### 3.2 Notepad API

新增 `chat_app_server_rs/src/api/notepad.rs`，并挂载到主路由。

API 分组：

- `GET /api/notepad/init`
- `GET /api/notepad/folders`
- `POST /api/notepad/folders`
- `PATCH /api/notepad/folders`
- `DELETE /api/notepad/folders`
- `GET /api/notepad/notes`
- `POST /api/notepad/notes`
- `GET /api/notepad/notes/:note_id`
- `PATCH /api/notepad/notes/:note_id`
- `DELETE /api/notepad/notes/:note_id`
- `GET /api/notepad/tags`
- `GET /api/notepad/search`

鉴权规则：

- 全部走现有 `AuthUser`。
- 如传 `project_id`，校验项目归属后再访问对应分区。

### 3.3 Builtin MCP Integration

在 builtin MCP 体系新增 `notepad`：

- 新增 `chat_app_server_rs/src/builtin/notepad/mod.rs`（工具定义与 `call_tool`）
- `src/builtin/mod.rs` 导出 `pub mod notepad;`
- `src/services/builtin_mcp.rs`：新增常量、`BuiltinMcpKind::Notepad`、config 列表
- `src/core/mcp_tools.rs`：
  - `BuiltinToolService` 增加 `Notepad(NotepadService)`
  - `build_builtin_tool_service` 增加 `BuiltinMcpKind::Notepad` 分支
- 使用前缀机制自动暴露工具，例如：
  - `notepad_<id8>_list_notes`
  - `notepad_<id8>_create_note`

## 4. Frontend Implementation Plan

### 4.1 API Client

在 `chat_app/src/lib/api/client.ts` 新增 notepad API 方法：

- `notepadInit`
- `listNotepadFolders`
- `createNotepadFolder`
- `renameNotepadFolder`
- `deleteNotepadFolder`
- `listNotepadNotes`
- `createNotepadNote`
- `getNotepadNote`
- `updateNotepadNote`
- `deleteNotepadNote`
- `listNotepadTags`
- `searchNotepadNotes`

### 4.2 Notepad Panel

新增 `chat_app/src/components/NotepadPanel.tsx`：

- 左侧：文件夹 + 笔记列表 + 搜索
- 右侧：标题/标签/Markdown 内容编辑
- 支持：新建、打开、保存、删除、按关键词过滤

### 4.3 ChatInterface Entry

在 `chat_app/src/components/ChatInterface.tsx`：

- 新增记事本按钮（header 区域）
- 新增 `showNotepadPanel` state
- 挂载 `<NotepadPanel />` 弹层

## 5. Validation Plan

- Rust 端：`cargo check`（chat_app_server_rs）
- Web 端：`npm run build`（chat_app）
- 手工验证：
  - 新建文件夹/笔记
  - 编辑并保存
  - 关闭后重开可读
  - MCP 配置列表可看到 builtin notepad

## 6. Future Enhancements

- 富文本工具栏、快捷键
- 拖拽移动笔记/文件夹
- 历史版本与回滚
- 导出（md/docx）
- 标签高级筛选与统计面板
