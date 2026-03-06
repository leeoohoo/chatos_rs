# 项目文件变更标记方案（含删除逻辑）

## 1. 现状核查（基于当前代码）

### 1.1 前端现状
- 项目文件树在 `chat_app/src/components/ProjectExplorer.tsx`。
- 当前只在**选中文件**后调用 `listProjectChangeLogs(projectId, { path })` 拉取该文件历史，无法在树上整体看到“哪些文件有未确认变更”。
- 变更记录侧栏仅是“查看历史”，没有“确认变更后清空标记”的状态机制。

### 1.2 后端现状
- 变更查询入口：`GET /api/projects/:id/changes`（`chat_app_server_rs/src/api/projects.rs`）。
- `mcp_change_logs` 当前字段：`id/server_name/path/action/bytes/sha256/diff/session_id/run_id/created_at`，**没有确认状态字段**。
- MCP 侧写入变更日志在：
  - `chat_app_server_rs/src/builtin/code_maintainer/mod.rs`
  - `chat_app_server_rs/src/builtin/code_maintainer/storage.rs`
- MCP 删除行为：
  - 工具：`delete_path`（`mod.rs`）
  - 实际删除：`fs_ops.delete_path`（目录 `remove_dir_all`，文件 `remove_file`）
  - 记录动作：`action = "delete"`。

### 1.3 当前动作类型
目前日志 action 主要是：
- `write`
- `append`
- `delete`

没有显式 `create/edit` 区分，这会影响你要的“绿色新增 / 黄色编辑 / 红色删除”。

---

## 2. 目标

1. 在项目树上直接标记“有未确认变更”的文件/目录。  
2. 颜色语义固定：
   - 新增：绿色
   - 编辑：黄色
   - 删除：红色
3. 用户执行“确认变更”后，对应标记恢复常规。  
4. 删除场景可见且可确认（重点）。

---

## 3. 总体方案

## 3.1 数据层（必须先做）

### 3.1.1 给变更日志加“确认状态”
为 `mcp_change_logs` 增加字段：
- `confirmed`（bool/int，默认 false）
- `confirmed_at`（datetime，可空）
- `confirmed_by`（user_id，可空）

> SQLite 用迁移 + `ALTER TABLE`；Mongo 走懒加载（无字段视作未确认）。

### 3.1.2 增加“归一化变更类型”
新增一个标准类型（可新字段 `change_kind`），值为：
- `create`
- `edit`
- `delete`

判定规则：
- `delete_path` / patch 删除 => `delete`
- `write/append`：
  - 写前不存在 => `create`
  - 写前存在 => `edit`
- 旧历史数据没有 `change_kind` 时，回退推断：
  - action=delete => delete
  - 其余默认 edit（保守）

---

## 3.2 API 层（新增两个接口）

### 3.2.1 项目未确认变更总览
`GET /api/projects/:id/changes/summary?scope=unconfirmed`

返回：
- `file_marks`: `{ path, kind, last_change_id, updated_at }[]`
- `deleted_marks`: `{ path, kind:"delete", last_change_id, updated_at, parent_path }[]`
- `counts`: `{ create, edit, delete, total }`

说明：
- `file_marks` 包含当前存在文件与目录的标记。
- `deleted_marks` 专门给“已删除但树里不存在”的路径。

### 3.2.2 确认变更
`POST /api/projects/:id/changes/confirm`

入参建议：
- `mode: "all" | "paths" | "change_ids"`
- `paths?: string[]`
- `change_ids?: string[]`

行为：
- 把命中的日志设置 `confirmed=true`。
- 返回确认数量。

---

## 3.3 前端展示层（ProjectExplorer）

### 3.3.1 树标记
- 在树节点名称旁加标记点：
  - create => 绿色点
  - edit => 黄色点
  - delete => 红色点
- 对目录可聚合子节点状态（目录也显示点）。

### 3.3.2 删除可视化（重点）
由于删除后文件不存在，必须做“虚拟节点”：
- 在树里增加一个折叠组：`已删除（未确认）`
- 组内显示红色删除项（按 parent_path 分组）
- 点击删除项可在右侧显示其变更记录（即使文件已不存在）

### 3.3.3 确认操作
- 顶部增加：
  - `确认当前项`
  - `确认全部变更`
- 删除虚拟节点也支持“确认后消失”。

---

## 3.4 删除逻辑细化（重点）

### 3.4.1 MCP 删除行为对方案的影响
`delete_path` 目前是直接物理删除（文件/目录），然后写一条 `delete` 日志。  
这意味着：树中已无文件，必须依赖 `deleted_marks` 才能让用户看见“删了什么”。

### 3.4.2 目录删除
- 若删除目录，记录至少有目录本身的 delete。
- UI 最少要显示目录删除红标记；如果存在子项 delete 日志，可按前缀聚合到该目录下。

### 3.4.3 冲突场景
- 路径先删后建（未确认期间）
  - 以**最新未确认记录**为准展示状态。
  - 例如 delete 后又 create，则显示绿色而非红色。

### 3.4.4 确认语义
- 确认路径时建议支持 prefix：
  - 确认目录 `a/b` 时，连同 `a/b/**` 的未确认记录一起确认。
- 这样能正确清空“目录删除”相关的所有红标记。

---

## 4. 实施步骤（建议顺序）

1. **后端迁移**：`mcp_change_logs` 加确认字段 + 索引。  
2. **MCP 写日志改造**：写入 `change_kind`（create/edit/delete）。  
3. **新增 summary/confirm API**。  
4. **前端树标记**：加载 summary 并在节点展示颜色。  
5. **删除虚拟节点**：`已删除（未确认）` 分组。  
6. **确认交互**：当前项/全部确认，确认后实时清标记。  
7. **回归测试**：重点覆盖删除目录、删后重建、批量确认。

---

## 5. 验收标准

1. 打开项目即能在树上看到未确认变更颜色标记。  
2. 新增/编辑/删除颜色符合预期。  
3. 删除文件即使不存在也能在“已删除（未确认）”看到。  
4. 点击确认后对应标记立即消失。  
5. 删除目录确认可一次清空该目录下所有相关未确认项。  

---

## 6. 影响文件（实施时会改）

- 前端：
  - `chat_app/src/components/ProjectExplorer.tsx`
  - `chat_app/src/lib/api/client.ts`
- 后端 API：
  - `chat_app_server_rs/src/api/projects.rs`
  - `chat_app_server_rs/src/repositories/change_logs.rs`
- MCP 写日志：
  - `chat_app_server_rs/src/builtin/code_maintainer/mod.rs`
  - `chat_app_server_rs/src/builtin/code_maintainer/storage.rs`
  - `chat_app_server_rs/src/builtin/code_maintainer/fs_ops.rs`
- DB：
  - `chat_app_server_rs/src/db/sqlite.rs`（初始化）
  - 对应迁移脚本（新增）

