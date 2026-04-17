# Tool Card Rework Plan

## Goal

把 ChatOS 里内置工具的展示，从“字段/值树表 + 技术原始 JSON”重做成“面向用户的结果卡片”。

这次改造的核心目标：

- 所有内置工具优先走定制卡片，不再默认掉回树表。
- 工具名统一去掉 server/builtin 前缀，只保留用户能理解的短名。
- 结果区优先展示结论、目标对象、关键输出、下一步提示。
- 默认隐藏 provider、transport、terminal_id、process_id、sha256、provider_attempts、raw snapshot 等内部实现字段。
- 保留兜底方案，但兜底也必须是卡片化结构视图，而不是表格。

## Design Principles

- 用户视角优先：先展示“做了什么、作用到哪里、结果如何”。
- 技术细节降噪：只保留对用户决策有帮助的字段。
- 同类结果同一种视觉语义：文件、搜索命中、来源列表、日志、研究结论要有稳定样式。
- 组件按工具族拆分，后续允许继续细拆到单工具文件。

## Builtin Inventory

### 1. Code Maintainer

工具：

- `read_file_raw`
- `read_file_range`
- `read_file`
- `list_dir`
- `search_text`
- `search_files`
- `write_file`
- `edit_file`
- `append_file`
- `delete_path`
- `apply_patch`
- `patch`

展示策略：

- 读文件：展示文件内容卡片，范围作为 meta。
- 目录：展示目录条目列表，显示名称、路径、类型、大小、时间。
- 搜索：展示命中列表，显示路径、行号、片段。
- 写工具：展示变更文件列表、补丁摘要、diff 预览、message、hint。

默认隐藏：

- `sha256`
- `size_bytes`
- `count`
- 原始 patch 文本
- 写工具里的通用 `Change summary` 技术行

当前状态：

- 本轮实现

### 2. Browser Tools

工具：

- `browser_navigate`
- `browser_snapshot`
- `browser_click`
- `browser_type`
- `browser_scroll`
- `browser_back`
- `browser_press`
- `browser_console`
- `browser_get_images`
- `browser_inspect`
- `browser_research`
- `browser_vision`

展示策略：

- inspect/research：展示当前页面卡、研究概览卡、研究结论卡、选中 URL、来源摘要。
- console：展示控制台消息卡、JS 错误卡、JS 结果卡。
- vision：展示视觉分析卡，隐藏 transport/provider/fallback 细节。
- get_images：展示图片资源卡。
- navigate/click/type/scroll/back/press/snapshot：展示页面状态和必要提示，不展示 refs/raw snapshot 大块内容。

默认隐藏：

- `provider`
- `transport`
- `prompt_source`
- `refs`
- `snapshot`
- `console_messages`
- `js_errors`
- `provider_attempts`

当前状态：

- 本轮继续沿用专门卡片，并把实现拆到独立组件文件

### 3. Web Tools

工具：

- `web_search`
- `web_extract`
- `web_research`

展示策略：

- search：展示搜索命中列表。
- extract：展示来源摘要卡、提取来源列表。
- research：展示研究概览、选中 URL、搜索命中、提取来源。

默认隐藏：

- `backend`
- `fallback_used`
- `provider_attempts`
- `data.web`
- `results`
- `_summary_text` 原始字段名

当前状态：

- 本轮实现

### 4. Terminal Controller

工具：

- `execute_command`
- `get_recent_logs`
- `process_list`
- `process_poll`
- `process_log`
- `process_wait`
- `process_write`
- `process_kill`
- `process`

展示策略：

- execute_command：命令状态卡 + 输出卡。
- get_recent_logs：终端分组卡 + 最近日志列表。
- process_list：进程列表卡。
- process_poll：运行状态卡 + 增量日志卡。
- process_log：日志窗口卡 + 文本输出卡。
- process_wait：等待结果卡 + 输出卡。
- process_write：输入发送结果卡。
- process_kill：终止结果卡。
- process：按 `action` 分流到对应卡片。

默认隐藏：

- `terminal_id`
- `process_id`
- `project_id`
- `has_session`
- `pid`
- `output_tail_chars`

当前状态：

- 本轮实现

### 5. Remote Connection Controller

工具：

- `list_connections`
- `test_connection`
- `run_command`
- `list_directory`
- `read_file`

展示策略：

- 先走非表格通用结构卡。
- 第二阶段补 SSH 连接、远程目录、远程命令输出专门卡。

默认隐藏：

- 内部用户上下文字段
- 过长 stderr/stdout 元信息

当前状态：

- 已实现专门卡
- 本轮补了连接摘要、远程命令输出卡、目录条目卡、远程文件卡、连通性结果卡

### 6. Notepad

工具：

- `init`
- `list_folders`
- `create_folder`
- `rename_folder`
- `delete_folder`
- `list_notes`
- `create_note`
- `read_note`
- `update_note`
- `delete_note`
- `list_tags`
- `search_notes`

展示策略：

- 第二阶段补文件夹列表、便签列表、标签列表、便签内容卡。
- 当前先保证不走树表。

当前状态：

- 已实现专门卡
- `init` 已隐藏 `data_dir` / `notes_root` / `index_path`
- 文件夹、笔记、标签、笔记正文都已有专门卡

### 7. Task Manager

工具：

- `add_task`
- `list_tasks`
- `update_task`
- `complete_task`
- `delete_task`

展示策略：

- 第二阶段补任务列表卡、任务状态变更卡、待确认卡。

当前状态：

- 已实现专门卡
- 任务创建、列表、更新、完成、删除都已有专门卡

### 8. UI Prompter

工具：

- `prompt_key_values`
- `prompt_choices`
- `prompt_mixed_form`

展示策略：

- 第二阶段补“等待用户输入”的表单说明卡。

当前状态：

- 已实现专门卡
- 键值表单、选择结果、混合表单结果已拆成独立结果卡
- 表单值与选择值分开展示，不再混在一个通用结构块里

### 9. Agent Builder / Memory Readers

工具：

- `recommend_agent_profile`
- `list_available_skills`
- `create_memory_agent`
- `update_memory_agent`
- `preview_agent_context`
- `get_command_detail`
- `get_plugin_detail`
- `get_skill_detail`

展示策略：

- 第二阶段补 profile 推荐卡、skills 列表卡、agent 结果卡、技能/命令详情卡。

当前状态：

- 已实现专门卡
- Agent 结果补了 description / role / plugin sources / skill ids / embedded skills
- Memory 结果补了 command / plugin / skill 的详情卡

## Rendering Rules

### Always Hide

- server/builtin 前缀
- provider/backend/fallback 技术字段
- transport / prompt_source
- terminal_id / process_id / project_id
- sha256 / raw file hashes
- provider_attempts
- refs / raw snapshot / raw source text
- 原始 patch payload

### Show Conditionally

- `diff`：有真实变更时显示
- `message` / `hint`：非空时显示
- `warning`：非空时显示
- `output`：有内容时显示
- `selected_urls` / `results_brief`：非空时显示

## Frontend File Split

第一阶段文件结构：

- `chat_app/src/components/toolCards/shared/*`
- `chat_app/src/components/toolCards/codeMaintainer/*`
- `chat_app/src/components/toolCards/browser/*`
- `chat_app/src/components/toolCards/web/*`
- `chat_app/src/components/toolCards/process/*`

组织方式：

- `BuiltinToolDetails.tsx` 只负责 dispatch/router。
- 每个工具族在自己的目录里管理结果卡。
- 通用 structured fallback 单独一个组件，彻底替代树表。

## Implementation Phases

### Phase 1

- 落根目录计划文件
- 工具名统一短名化
- 拆出共享卡片 primitives
- Code Maintainer / Browser / Web / Terminal 结果卡片化
- 去掉树表 fallback

### Phase 2

- Remote Connection / Notepad / Task Manager / UI Prompter / Agent Builder / Memory Readers 专门卡
- family routing key / allowlist 拆分，避免同名工具串路由
- 继续把结果卡按真实返回结构细化，隐藏内部路径、ID、payload

当前进度：

- Phase 2 主体已完成
- 仍可继续做更细的单工具组件拆分与“高级信息”折叠层

## Validation

- `npm run type-check`
- `npm run test -- --run src/components/ToolCallRenderer.test.tsx src/components/messageItem/ToolCallTimeline.test.tsx`
- 后续补充：
  - prefixed builtin tool name short-name tests
  - terminal tool card tests
  - structured result fallback no-table tests

## Progress

- 已完成：
  - 根目录改造计划文件
  - 结果选择优先使用可解析结构化结果
  - 代码维护工具短名化
  - 参数卡片基础去噪
  - Browser / Web / Code / Terminal 专门卡
  - Remote / Notepad / Task / UI / Agent / Memory 专门卡
  - family 级工具分类与 routing key
  - 新增工具族的 family 配色与视觉区分
  - Notepad `init` / Remote `test_connection` / UI mixed form 去噪回归测试
  - `npm run type-check`
  - `npm run test -- --run src/components/ToolCallRenderer.test.tsx src/components/messageItem/ToolCallTimeline.test.tsx`

- 本轮进行中：
  - 继续把各工具族进一步细拆到更小组件
  - 评估是否增加“高级信息”二级折叠层
