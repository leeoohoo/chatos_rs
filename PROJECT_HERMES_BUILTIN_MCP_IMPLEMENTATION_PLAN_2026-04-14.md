# Rust 内置 MCP 实施方案（仅 Browser + Web Search）

日期：2026-04-14  
作者：Codex（按你最新范围重写）

## 1. 你的真实目标（修正后）

仅补齐 Hermès 里这两块能力到我们当前项目内置 MCP：

1. 浏览器自动化工具（`browser_*`）
2. Web 检索/抽取工具（`web_search`, `web_extract`）

明确不在本次范围内：

1. `terminal`/`process`
2. `read_file`/`write_file`/`search_files`/`patch`
3. 其它 RL / HA / TTS / image_generate

---

## 2. 现有可复用底座

你们已经有内置 MCP 框架，不需要重做：

1. `chat_app_server_rs/src/services/builtin_mcp.rs`
2. `chat_app_server_rs/src/services/mcp_loader.rs`
3. `chat_app_server_rs/src/core/mcp_tools/builtin.rs`
4. `chat_app_server_rs/src/core/mcp_tools/schema.rs`
5. `chat_app_server_rs/src/core/mcp_tools/execution.rs`
6. `chat_app_server_rs/src/services/v2/mcp_tool_execute.rs`

结论：只要新增 Browser/Web 的 builtin service 并注册到现有链路即可。

## 2.1 已存在能力（本次不重复实现）

下面这些在当前代码里已经落地，我这次按“已存在”处理：

1. 内置 MCP 已有 ID（代码维护 + 终端）：
   - `builtin_code_maintainer_read` / `builtin_code_maintainer_write` / `builtin_terminal_controller`
   - 见 `chat_app_server_rs/src/services/builtin_mcp.rs`（常量与注册分支）
2. builtin 服务工厂已接这三类：
   - `BuiltinMcpKind::CodeMaintainerRead`
   - `BuiltinMcpKind::CodeMaintainerWrite`
   - `BuiltinMcpKind::TerminalController`
   - 见 `chat_app_server_rs/src/core/mcp_tools/builtin.rs`
3. 代码维护工具已存在：
   - `read_file_raw`, `read_file_range`, `list_dir`, `search_text`
   - `write_file`, `edit_file`, `append_file`, `delete_path`, `apply_patch`
   - 见 `chat_app_server_rs/src/builtin/code_maintainer/mod.rs`
4. 终端工具已存在：
   - `execute_command`, `get_recent_logs`
   - 见 `chat_app_server_rs/src/builtin/terminal_controller/mod.rs`

## 2.2 当前缺口（本次新增目标）

当前 `chat_app_server_rs/src/builtin/` 模块里没有 `browser_tools` / `web_tools`，  
全局搜索也没有 `web_search` / `web_extract` / `browser_navigate` 等工具实现。  
所以本次新增聚焦为：

1. `browser_*` 工具链
2. `web_search` / `web_extract`

---

## 3. 范围与分期

## 阶段 A（Browser MVP）

首批上线 4 个浏览器工具：

1. `browser_navigate(url)`
2. `browser_snapshot(full?)`
3. `browser_click(ref)`
4. `browser_type(ref, text)`

这 4 个足够覆盖“打开页面 -> 读取结构 -> 点击 -> 输入”的核心工作流。

## 阶段 B（Web Search MVP）

上线 2 个 web 工具：

1. `web_search(query)`
2. `web_extract(urls[])`

优先提供“统一 provider 抽象 + 单 provider 落地”，先跑通再扩展多后端。

## 阶段 C（Browser 增强）

补齐：

1. `browser_scroll`
2. `browser_press`
3. `browser_back`
4. `browser_console`
5. `browser_get_images`
6. `browser_vision`

---

## 4. 实现设计

## 4.1 新增内置 MCP 类型

在 `builtin_mcp.rs` 新增两个内置 MCP：

1. `builtin_browser_tools`
2. `builtin_web_tools`

并接入：

1. `BuiltinMcpKind`
2. `builtin_kind_by_id / builtin_kind_by_command`
3. `get_builtin_mcp_config / list_builtin_mcp_configs`
4. `builtin_display_name`

## 4.2 新增 builtin 服务

新增模块：

1. `chat_app_server_rs/src/builtin/browser_tools/`
2. `chat_app_server_rs/src/builtin/web_tools/`

并接到：

1. `chat_app_server_rs/src/builtin/mod.rs`
2. `chat_app_server_rs/src/core/mcp_tools/builtin.rs`

每个服务都实现现有统一接口：

1. `list_tools()`
2. `call_tool(name, args, ...)`

---

## 5. Provider 策略（关键）

## 5.1 Browser provider

建议先做 trait：

1. `navigate`
2. `snapshot`
3. `click`
4. `type_text`
5. 后续扩展接口（scroll/press/vision）

实现顺序：

1. `Mock/Noop provider`（用于联调和错误回传）
2. `Playwright provider`（本地或远程 browser endpoint）

## 5.2 Web provider

建议统一 trait：

1. `search(query, limit)`
2. `extract(urls)`

实现顺序：

1. `Firecrawl provider`（优先）
2. 后续可插 Tavily/Exa

---

## 6. 安全边界

## 6.1 浏览器

1. URL 安全校验（协议白名单 + 内网地址限制）
2. 单次操作超时（默认 30s）
3. 会话级资源清理（避免浏览器泄漏）

## 6.2 Web 搜索

1. 每次 `web_search` 限制返回条数
2. 每次 `web_extract` 限制 URL 数（例如 <=5）
3. 抽取文本长度上限与截断策略

## 6.3 审计

记录工具调用摘要（不泄漏敏感 token）：

1. tool name
2. host/domain
3. 耗时
4. 成功/失败

---

## 7. 代码改动清单（最小）

1. `chat_app_server_rs/src/services/builtin_mcp.rs`
2. `chat_app_server_rs/src/builtin/mod.rs`
3. `chat_app_server_rs/src/core/mcp_tools/builtin.rs`
4. `chat_app_server_rs/src/builtin/browser_tools/mod.rs`（新增）
5. `chat_app_server_rs/src/builtin/browser_tools/provider.rs`（新增）
6. `chat_app_server_rs/src/builtin/browser_tools/actions.rs`（新增）
7. `chat_app_server_rs/src/builtin/web_tools/mod.rs`（新增）
8. `chat_app_server_rs/src/builtin/web_tools/provider.rs`（新增）
9. `chat_app_server_rs/src/builtin/web_tools/actions.rs`（新增）

---

## 8. 验收标准

## A. Browser MVP 验收

1. MCP 列表可见 `builtin_browser_tools`
2. 4 个 browser 工具可被模型发现
3. 实际可完成“打开页面 + 点击 + 输入 + 再快照”

## B. Web MVP 验收

1. MCP 列表可见 `builtin_web_tools`
2. `web_search` 返回结构化结果
3. `web_extract` 可抽取页面正文并受长度限制

## C. 回归

1. 现有 `terminal/file` 工具链不受影响
2. v2/v3 流式工具事件无回归

---

## 9. 开发顺序（建议）

1. 先做阶段 A（Browser 4 工具）
2. 再做阶段 B（web_search/web_extract）
3. 最后阶段 C（browser 增强）

---

## 10. 备注（针对你这次反馈）

这版方案已经按你的意思收敛为“只做浏览器 + web search”，不再把终端/文件当成本次目标。  
