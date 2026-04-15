# ChatOS 内置工具对标 Hermes-Agent 优化实施方案（2026-04-15）

## 1. 结论先说

当前 `chatos` 的内置工具体系已经覆盖了 Hermes 的核心浏览器/Web 能力，并在任务、远程连接、记事本、Agent 构建上有自有扩展。  
真正值得借鉴 Hermes 的，不是“再加一批工具名”，而是三类工程能力：

1. 工具可用性治理（可用才暴露）
2. 安全并行执行（提速且不互相踩文件）
3. 终端与 Web 的长任务/多后端能力

---

## 2. 工具现状盘点

## 2.1 Hermes-Agent（来自 `toolsets.py` + 注册代码）

1. Hermes 核心工具 `_HERMES_CORE_TOOLS`：36 个
2. 全 toolset 并集：47 个
3. 典型能力：
   - `web_search/web_extract`
   - `terminal/process`
   - `read_file/write_file/patch/search_files`
   - `browser_*`（10 个）
   - `todo/memory/session_search/clarify`
   - `execute_code/delegate_task`
   - `text_to_speech/image_generate/vision_analyze`
   - `send_message/homeassistant/rl/*`

## 2.2 ChatOS（来自 `src/builtin/*` 实际注册）

1. 当前唯一工具名：56 个
2. 默认内置 MCP（`list_builtin_mcp_configs`）：10 类
   - `code_maintainer_read`
   - `code_maintainer_write`
   - `terminal_controller`
   - `task_manager`
   - `notepad`
   - `agent_builder`
   - `ui_prompter`
   - `remote_connection_controller`
   - `web_tools`
   - `browser_tools`
3. 条件内置（按 contact agent 注入）：3 类
   - `memory_skill_reader`
   - `memory_command_reader`
   - `memory_plugin_reader`

## 2.3 两边重叠能力

已重叠的关键工具：`web_search/web_extract` + 10 个 `browser_*`，以及文件读写核心语义（命名有差异）。

---

## 3. 可借鉴优化点（按优先级）

## P0-1 安全并行执行工具调用（高收益）

### 借鉴点
Hermes 在 `run_agent.py` 中实现了“可并行工具白名单 + 路径冲突检测 + 保序回写”。

### ChatOS 当前
`src/core/mcp_tools/execution.rs` 逐个串行执行。

### 改造方案
1. 在 `ToolInfo` 增加执行 hint（只读/路径作用域/不可并行）
2. 在 `execute_tools_stream` 增加并行分发器
3. 规则：
   - `web_search/web_extract/browser_snapshot/list_*` 等只读可并行
   - 涉及文件写入、终端写入、UI 交互强制串行
   - 同路径或父子路径写操作不可并行
4. 结果保持原始 `tool_call` 顺序输出

### 目标
多工具批次（特别是读类工具）总耗时下降 30%-50%。

---

## P0-2 工具可用性检查与按需暴露（高稳定性）

### 借鉴点
Hermes `registry.register(... check_fn/requires_env ...)`，不可用工具不暴露。

### ChatOS 当前
大多工具“注册即暴露”，例如 `web_tools` 在缺少 `FIRECRAWL_API_KEY` 时仍会展示，运行时才失败。

### 改造方案
1. 为 builtin 服务统一增加 `availability` 机制（tool-level）
2. 在 `build_tools_from_builtin` 时只注册可用工具
3. 对不可用工具提供结构化诊断（缺 env、缺二进制、缺 session context）
4. 首批接入：
   - `web_tools`（Firecrawl key/base url）
   - `browser_tools`（`agent-browser` 可执行）
   - `remote_connection_controller`（默认连接上下文）

### 目标
“工具可见但必失败”问题显著下降，减少模型盲调和无效调用。

---

## P1-1 终端后台进程工具（提升长任务体验）

### 借鉴点
Hermes 通过 `terminal(background=true)` + `process` 支持长任务轮询、写入、终止。

### ChatOS 当前
`execute_command` + `get_recent_logs` 偏一次性执行，缺少标准的后台任务会话控制。

### 改造方案
1. 在 `terminal_controller` 新增 `process` 风格工具：
   - `process_list`
   - `process_poll`
   - `process_wait`
   - `process_kill`
   - `process_write`
2. 复用现有 terminal manager 状态，不新造执行引擎
3. 为长任务返回 `session_token/process_id`

### 目标
构建、测试、部署等长任务可边跑边协作，减少阻塞式等待。

---

## P1-2 Web 工具多后端与大文本摘要策略（提升成功率）

### 借鉴点
Hermes 的 `web_tools` 有多后端路由与大内容摘要压缩策略。

### ChatOS 当前
仅 Firecrawl 单后端；抽取文本有截断但缺乏摘要分层。

### 改造方案
1. 在 `src/builtin/web_tools/provider.rs` 抽象 provider trait
2. 路由策略：
   - 首选 Firecrawl
   - 失败时回退次级 provider（可配置）
3. 对超长抽取内容引入“分块摘要 + 合并”路径，返回结构化摘要元数据

### 目标
降低 `web_extract` 因后端波动失败的概率，提高大页面可用性。

---

## P1-3 文件工具语义对齐与兼容别名（提升模型迁移效果）

### 借鉴点
Hermes 文件工具命名稳定：`read_file/write_file/patch/search_files`。

### ChatOS 当前
`code_maintainer` 用 `read_file_raw/read_file_range/search_text/apply_patch`，语义丰富但迁移成本高。

### 改造方案
1. 在 `code_maintainer` 增加 Hermes 兼容别名（内部映射现有实现）：
   - `read_file` -> `read_file_range/read_file_raw` 路由
   - `patch` -> `apply_patch`
   - `search_files` -> `search_text`
2. 保留现有高级工具不变，兼容层只做入口统一

### 目标
减少模型 prompt/tool 适配成本，提升跨 Agent 迁移成功率。

---

## 4. 分阶段落地计划

## 阶段 A（1 周，必须做）

1. 并行执行框架（P0-1）
2. 工具可用性过滤（P0-2）
3. 回归测试补齐

### 代码改动
1. `chat_app_server_rs/src/core/mcp_tools.rs`
2. `chat_app_server_rs/src/core/mcp_tools/execution.rs`
3. `chat_app_server_rs/src/services/v3/mcp_tool_execute.rs`
4. `chat_app_server_rs/src/builtin/web_tools/mod.rs`
5. `chat_app_server_rs/src/builtin/browser_tools/mod.rs`

### 验收
1. 并行批次与串行结果一致（顺序一致、错误一致）
2. 无 key/无 agent-browser 时工具不再暴露

## 阶段 B（1-2 周，建议做）

1. 终端后台进程控制（P1-1）
2. Web 多后端路由与摘要（P1-2）

### 代码改动
1. `chat_app_server_rs/src/builtin/terminal_controller/mod.rs`
2. `chat_app_server_rs/src/builtin/terminal_controller/actions.rs`
3. `chat_app_server_rs/src/builtin/web_tools/provider.rs`
4. `chat_app_server_rs/src/builtin/web_tools/actions.rs`

### 验收
1. 支持后台任务 list/poll/wait/kill
2. Web 后端异常时可自动回退

## 阶段 C（0.5-1 周，可选）

1. 文件工具兼容别名（P1-3）
2. 评估是否引入“toolset 配置面板”（类似 Hermes）

### 代码改动
1. `chat_app_server_rs/src/builtin/code_maintainer/mod.rs`
2. `chat_app_server_rs/src/services/builtin_mcp.rs`（若加 profile 配置）

---

## 5. 风险与控制

1. 并行执行引入竞态风险  
控制：先只并行只读工具；写操作默认串行，灰度开关控制。

2. 可用性过滤导致“工具突然消失”  
控制：保留诊断事件，前端展示“不可用原因”。

3. 多后端 Web 增加复杂度  
控制：先做 2 层回退，不一次性接太多 provider。

---

## 6. 建议的起步顺序

1. 先做阶段 A（收益最大、风险可控）
2. 再做阶段 B（补长任务与稳定性）
3. 最后做阶段 C（兼容体验优化）

