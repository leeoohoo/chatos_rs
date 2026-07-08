# Task Runner 沙箱输出变更与 Diff 预览实施方案

## 目标

当 Task Runner 采用沙箱执行任务后，在运行 output 中维护本次任务造成的文件变更清单，覆盖新增、修改、删除三类文件；用户在 Chat App 的任务卡片中点击“变更”后，打开弹窗查看变更列表，并点击单个文件查看对应 diff 预览。

本方案优先复用当前仓库已有链路：

- `task_runner_service` 已在 `sandbox_runtime.rs` 中申请、释放沙箱，并把沙箱上下文写入 `run.input_snapshot`。
- `task_runner_service` 的 `task_runs.report_json` 和 `task_run_events.payload_json` 已可承载结构化运行输出。
- `sandbox_manager_service` 的 `ReleaseSandboxResponse` 已预留 `output_workspace` / `diff_summary` 字段，但当前 `diff_summary` 始终为 `None`，`output_workspace` 也只是建目录。
- Chat App 已有任务卡片组件 `MessageTaskCardNode` 和运行详情弹窗 `MessageTaskRunDetailModal`。
- Chat App 已有 Git diff 弹窗 `GitDiffDialog.tsx` 与 `diffLineView`，可抽出统一 diff 渲染能力。

## 当前关键代码位置

后端沙箱链路：

- `task_runner_service/backend/src/services/sandbox_runtime.rs`
  - `prepare_sandbox_if_needed`：申请沙箱、复制 workspace、保存 sandbox metadata。
  - `release_sandbox`：调用 Sandbox Manager release，目前只把 release response 放进 `sandbox_released` 事件。
- `task_runner_service/backend/src/services/run_model_phase.rs`
  - 当前顺序是模型执行结束后先 `finalize_model_phase`，再 `release_sandbox`。
- `task_runner_service/backend/src/services/run_model_phase/completion.rs`
  - `finalize_model_phase` 把 `TaskRunReport` 序列化到 `run.report`，触发回调和后续任务。
- `sandbox_manager_service/backend/src/service/manager.rs`
  - `release` 已接收 `export_result`，但只创建 output 目录，不复制结果、不生成 diff。
  - `prepare_run_workspace` 当前返回 `runs/{run_id}/input/workspace`，这个目录实际作为沙箱可写 workspace 使用。

Chat App 与代理链路：

- `task_runner_service/backend/src/api/chatos_internal.rs`
  - 暴露 `/internal/chatos/message-runs/:run_id`，运行详情会截断 `run.report` 到 256KB。
- `chatos/backend/src/api/message_task_runner.rs`
  - Chat App 的任务详情、运行详情都经这里代理到 Task Runner internal API。
- `chatos/backend/src/services/task_runner_api_client.rs`
  - 当前有 `get_message_run` / `get_message_graph_run`，还没有 output changes / diff 代理。
- `chatos/frontend/src/components/messageTasks/MessageTaskGraphNode.tsx`
  - 截图中的任务卡片按钮区域，目前有“执行过程 / 详情 / 运行详情”。
- `chatos/frontend/src/components/projectExplorer/git/GitDiffDialog.tsx`
  - 已有统一 diff 行渲染，可复用视觉样式。

## 推荐总体设计

### 1. 沙箱输出产物目录

把每次沙箱运行的产物固定在 `.chatos/task-runner/runs/{run_id}` 下：

```text
.chatos/task-runner/runs/{run_id}/
  baseline/workspace/        # 执行开始时的基线快照
  runtime/workspace/         # 挂载给沙箱的可写 workspace
  output/workspace/          # release 时导出的最终 workspace
  output/change_manifest.json
  output/diffs/{sha256}.diff
```

建议在 `sandbox_manager_service` 中正式返回 `baseline_workspace`、`run_workspace`、`output_workspace` 三个路径；`Task Runner` 在 `prepare_sandbox_if_needed` 阶段把有效 workspace 复制到 baseline，再复制到 runtime。这样 diff 对比的是“任务开始快照 vs 沙箱结束结果”，不会受宿主原项目后续变化影响。

如果先做最小改动，也可以由 `Task Runner` 在现有 `run_workspace` 的 sibling 目录自行建立 `baseline/workspace`，但长期建议由 Sandbox Manager 统一管理路径。

### 2. 变更清单数据结构

在 release 阶段生成结构化 manifest，不把大 diff 全塞进 `run.report`：

```json
{
  "schema_version": 1,
  "run_id": "run_xxx",
  "sandbox_id": "sandbox_xxx",
  "lease_id": "lease_xxx",
  "generated_at": "2026-07-03T...",
  "output_workspace": ".../output/workspace",
  "counts": {
    "added": 2,
    "modified": 3,
    "deleted": 1,
    "binary": 0,
    "diff_available": 5
  },
  "files": [
    {
      "path": "src/main.rs",
      "status": "modified",
      "old_size": 1200,
      "new_size": 1288,
      "old_sha256": "...",
      "new_sha256": "...",
      "added_lines": 8,
      "deleted_lines": 3,
      "diff_available": true,
      "diff_truncated": false,
      "diff_ref": "output/diffs/abc.diff"
    }
  ]
}
```

状态先支持用户明确要求的三类：

- `added`
- `modified`
- `deleted`

后续可扩展 `renamed`，但第一版不必做重命名检测，避免误判。

### 3. Diff 生成策略

在 `sandbox_manager_service/backend/src/service/manager.rs` 的 release 流程中实现：

1. 将 `runtime/workspace` 复制到 `output/workspace`。
2. 遍历 `baseline/workspace` 与 `output/workspace`，按相对路径建立文件索引。
3. 对新增、删除、内容 hash 不同的普通文件生成 `SandboxFileChange`。
4. 文本文件生成 unified diff，写到 `output/diffs/{hash}.diff`。
5. 二进制或超大文件不生成正文 diff，只返回 `diff_available=false` 和原因。
6. 写 `output/change_manifest.json`。
7. `ReleaseSandboxResponse` 保留 `diff_summary`，同时新增结构化字段，例如 `change_manifest` 或 `file_changes`。

建议限制：

- 单文件 diff 最大 512KB，超出后截断并标记 `diff_truncated=true`。
- manifest 最大返回 1000 个文件；完整清单仍落盘，API 支持分页。
- 默认跳过 `.git`、`.chatos`、`.task-runner`，避免内部状态污染结果。
- 所有路径只返回相对路径，不返回宿主绝对路径给前端。

### 4. Task Runner 持久化与事件

调整 `task_runner_service/backend/src/services/run_model_phase.rs` 的完成顺序：

```text
execute_prepared_model_run
  -> release_sandbox_and_collect_output
  -> finalize_model_phase(report + sandbox_output)
  -> terminal callback / async follow-up
```

也就是说，沙箱 output 需要在 `finalize_model_phase` 前收集好，这样最终回调、任务详情和运行详情都能读到同一份 output。

在 `run.report` 中追加轻量摘要：

```json
{
  "...": "原 TaskRunReport 字段",
  "output": {
    "sandbox": {
      "enabled": true,
      "sandbox_id": "sandbox_xxx",
      "output_workspace": "...",
      "change_manifest_ref": "output/change_manifest.json",
      "file_change_counts": {
        "added": 2,
        "modified": 3,
        "deleted": 1
      },
      "file_changes_preview": [
        { "path": "src/main.rs", "status": "modified", "diff_available": true }
      ],
      "truncated": false
    }
  }
}
```

同时追加事件：

- `sandbox_output_collected`：包含 counts、manifest_ref、output_workspace。
- `sandbox_output_collect_failed`：diff/export 失败时追加 warning，但不把模型任务结果改成 failed。
- `sandbox_released`：继续保留现有 release 事件，避免破坏已有事件流。

### 5. Task Runner API

新增 internal API，供 Chat App Server 代理：

```text
GET /internal/chatos/message-runs/:run_id/output/changes
GET /internal/chatos/message-runs/:run_id/output/diff?path=src/main.rs
```

查询参数沿用当前 message run：

```text
source_session_id
source_user_message_id
source_turn_id
limit
offset
```

校验逻辑复用 `get_chatos_message_run`：

1. 先读取 run。
2. 再确认 run.task_id 属于当前消息来源。
3. 从 `run.report.output.sandbox.change_manifest_ref` 或 release event payload 定位 manifest。
4. `changes` 返回 counts + 文件列表分页。
5. `diff` 只能读取 manifest 中存在且 `diff_available=true` 的文件；禁止绝对路径、`..`、未登记路径。

必要时也给 Task Runner 自己的普通 API 加：

```text
GET /api/runs/:run_id/output/changes
GET /api/runs/:run_id/output/diff?path=...
```

这样 `task_runner_service/frontend` 的运行详情页也能复用。

### 6. Chat App Server 代理

在 `chatos/backend/src/api/message_task_runner.rs` 新增：

```text
GET /api/messages/:message_id/task-runner/runs/:run_id/output/changes
GET /api/messages/:message_id/task-runner/runs/:run_id/output/diff
```

在 `chatos/backend/src/services/task_runner_api_client.rs` 新增：

- `get_message_run_output_changes(...)`
- `get_message_run_output_diff(...)`

代理层继续负责：

- 根据消息解析 Task Runner base URL。
- 带上 source session/message/turn query。
- 校验返回的 run/task 归属。
- 对返回 body 做大小限制，diff 接口建议单次限制 1MB。

### 7. Chat App 前端

类型与 API：

- `chatos/frontend/src/lib/api/client/types/messageTaskRunner.ts`
  - 新增 `MessageTaskRunnerFileChange`
  - 新增 `MessageTaskRunnerRunOutputChangesResponse`
  - 新增 `MessageTaskRunnerRunOutputDiffResponse`
- `chatos/frontend/src/lib/api/client/messages.ts`
  - 新增 `getMessageTaskRunnerRunOutputChanges`
  - 新增 `getMessageTaskRunnerRunOutputDiff`

UI 组件：

- 抽出通用 diff viewer：
  - 从 `GitDiffDialog.tsx` 复用 `diffLineView`，新建 `UnifiedDiffViewer` 或 `RunOutputDiffViewer`。
- 新建 `chatos/frontend/src/components/messageTasks/MessageTaskChangesModal.tsx`：
  - 弹窗宽度建议 `max-w-6xl`，高度 `88vh`。
  - 左侧：文件变更列表，显示状态、路径、增删行数。
  - 右侧：diff 预览，支持 loading、空状态、二进制/超大文件提示。
  - 顶部：新增/修改/删除 counts。
- `MessageTaskGraphNode.tsx`：
  - 在卡片按钮区增加“变更”按钮。
  - 按钮只有 `last_run_id` 存在时可点击；如果 `last_run.file_change_counts` 可用，则无变更时禁用。
  - 当前三列按钮可改为四列，或在窄卡片下换行，避免截图中的卡片宽度出现文字挤压。
- `useMessageTaskGraph.ts` / `useMessageTasks.ts`：
  - 增加 `openChanges(task)`、`closeChanges()`、`loadFileDiff(path)` 状态。
  - 打开弹窗时先拉 changes；点击文件时再拉 diff，避免一次性下载所有 patch。

Task Runner 前端可选补充：

- `task_runner_service/frontend/src/pages/runs/RunDetailSummary.tsx`
- `task_runner_service/frontend/src/pages/tasks/TaskDetailDrawer.tsx`

在运行详情里增加“文件变更”区块，复用同一套数据结构。

## 边界与风险

- 当前 Sandbox Manager 的 `output_workspace` 只是空目录，必须补齐“复制最终 workspace + 生成 manifest/diff”，否则 Task Runner 无法可靠展示变更。
- 当前 Task Runner 在 `finalize_model_phase` 后才 release 沙箱，若不调整顺序，最终回调和任务 output 可能拿不到变更摘要。
- `chatos_internal.rs` 会截断 `run.report`，所以完整 diff 不应放在 report 中，只放摘要和 manifest 引用。
- 文件路径必须严格从 manifest 白名单读取，不能把 diff API 做成任意文件读取。
- 二进制文件、超大文件、权限异常文件要可见地出现在清单中，但不强行生成 diff。
- 第一版不做 renamed 检测，避免复杂度和误报；删除 + 新增即可满足“新增、修改、删除”的要求。

## 测试计划

后端单元测试：

- `sandbox_manager_service`：
  - 新增文件生成 `added`。
  - 修改文件生成 `modified` 和 unified diff。
  - 删除文件生成 `deleted`。
  - 二进制文件不生成正文 diff。
  - 超大 diff 被截断。
  - `.git` / `.chatos` 被跳过。
- `task_runner_service`：
  - 沙箱 release 成功后 `run.report.output.sandbox.file_change_counts` 被写入。
  - release 或 diff 失败时 run 仍按模型结果结束，并追加 warning event。
  - internal changes/diff API 拒绝不属于当前 message source 的 run。
  - diff API 拒绝 `../`、绝对路径、manifest 外路径。
- `chat_app_server_rs`：
  - 新代理路由正确转发 source query。
  - Task Runner 报错时返回稳定 BAD_GATEWAY 结构。

前端测试：

- `MessageTaskChangesModal`：
  - 渲染新增/修改/删除 counts。
  - 点击左侧文件后显示 diff。
  - 二进制/无 diff 文件显示提示。
  - loading/error/empty 状态正常。
- `MessageTaskGraphNode`：
  - 有 `last_run_id` 时显示或启用“变更”入口。
  - 无运行记录时按钮禁用或不展示。

集成验证：

1. 开启 Task Runner 沙箱。
2. 创建一个会在 sandbox 中新增、修改、删除文件的任务。
3. 等运行完成。
4. 在任务卡片点击“变更”。
5. 弹窗左侧能看到三类变更，点击文件后右侧 diff 正确。
6. 关闭弹窗，再打开运行详情，output 中能看到同一份变更摘要。

## 分阶段实施

第一阶段：后端产物与持久化

1. 在 Sandbox Manager 建立 baseline/runtime/output 目录约定。
2. release 时复制最终 workspace，生成 `change_manifest.json` 和 diff 文件。
3. Task Runner 调整 release/finalize 顺序，把变更摘要写入 `run.report.output.sandbox`。
4. 增加 `sandbox_output_collected` 事件。

第二阶段：API 与代理

1. Task Runner 增加 internal changes/diff API。
2. Chat App Server 增加对应代理路由和 client 方法。
3. 加入路径白名单和响应大小限制。

第三阶段：Chat App UI

1. 新增 message task 变更弹窗。
2. 任务卡片增加“变更”按钮。
3. 复用/抽取现有 Git diff viewer。
4. 在运行详情 output 中展示变更摘要。

第四阶段：补齐 Task Runner 自身 UI 与优化

1. Task Runner 前端运行详情页展示文件变更。
2. 根据文件数量补分页、搜索、状态过滤。
3. 后续再评估 renamed 检测、批量复制回宿主 workspace、按任务输出生成 patch bundle。
