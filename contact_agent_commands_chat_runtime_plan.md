# 联系人智能体 Commands 在聊天中发挥作用方案

## 1. 目标与结论

这次要解决的问题不是“把 commands 导入进来”，而是“让 commands 在聊天运行时可用、可触发、可观测”。

最终目标：

1. 模型在每轮聊天里知道当前联系人有哪些可用 commands。
2. 模型能按 `command_ref` 按需读取 command 完整内容，而不是只靠简介。
3. 用户可以显式触发 command（例如 `/team-debug ...`）。
4. 这次是否命中 command、命中了哪个 command，要能进快照并可回放。

核心结论：

1. `commands` 不应只混在 `plugin content_summary` 文本里。
2. 要升级为独立运行时索引：`runtime_commands`。
3. 要补一个联系人内置 MCP：`memory_command_reader_get_command_detail`。
4. 默认不截断 command 原文；仅在总上下文超限时降级为“索引 + 按需拉取”。

## 2. 当前实现现状（基于代码）

当前链路里已经做了这些：

1. Memory 导入时会解析 `plugins/*/commands/*.md` 并入库。
2. 插件详情页可看到 commands。
3. 运行时会把插件内容拼进联系人 system prompt。

当前缺口在聊天运行时：

1. `commands` 还没有独立结构化字段进入 runtime context。
2. ChatOS 只有技能读取器 `memory_skill_reader_get_skill_detail`，没有 command 读取器。
3. 用户输入 `/xxx` 目前不会被当成 command 语义处理。
4. Turn snapshot 里没有记录“本轮用了哪个 command”。

## 3. 目标能力模型

本方案把 command 能力定义成 3 层：

1. 索引层：让模型知道“有哪些 command 可用”，但不默认灌入全文。
2. 详情层：模型或系统按 `command_ref` 拉取完整内容。
3. 触发层：支持显式触发（`/command args`）和隐式触发（模型主动选择）。

## 4. 方案设计

## 4.1 Memory 数据结构升级

### 4.1.1 插件 command 元数据增强

扩展 `MemorySkillPluginCommand`（`memory_server/backend/src/models/agents.rs`）：

1. `name`：命令名（已有）。
2. `source_path`：文件路径（已有）。
3. `content`：命令正文（已有）。
4. `description`：从 frontmatter `description` 提取（新增）。
5. `argument_hint`：从 frontmatter `argument-hint` 提取（新增）。

说明：

1. 不引入随机 UUID 作为 command ID。
2. 运行时引用统一使用短引用 `CMD1/CMD2/...`。

### 4.1.2 运行时上下文字段新增

扩展 `MemoryAgentRuntimeContext`，新增：

1. `runtime_commands: Vec<MemoryAgentRuntimeCommandSummary>`

建议结构：

```rust
pub struct MemoryAgentRuntimeCommandSummary {
    pub command_ref: String,      // CMD1, CMD2...
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub updated_at: Option<String>,
}
```

排序规则（保证稳定）：

1. 按 agent 的 `plugin_sources` 顺序分组。
2. 组内按 `source_path` 字典序。
3. 按排序结果生成 `CMD1..CMDN`。

## 4.2 导入解析升级（Memory）

在 `memory_server/backend/src/services/skills/io_plugin_content.rs`：

1. 继续保留 command 正文全量入库。
2. 从 frontmatter 解析并持久化：
   - `description`
   - `argument-hint`
3. `name` 解析优先级：
   - frontmatter `name`
   - 一级标题
   - 文件名

## 4.3 聊天 system prompt 升级（ChatOS）

在 `chat_app_server_rs/src/core/chat_runtime.rs` 的联系人 system 组装里：

1. 新增“可用 Commands（command_ref）”段落。
2. 每条仅放索引信息：
   - `command_ref`
   - `名称`
   - `plugin_source`
   - `description`
   - `argument_hint`
3. 不默认内联 command 全文。
4. 明确提示模型：需要全文时调用 `memory_command_reader_get_command_detail`。

说明：

1. 这样能显著降低 token 占用。
2. 同时保留命令可发现性与可执行性。

## 4.4 新增内置 MCP：Command Reader

在 ChatOS 增加新的 builtin MCP service（对齐 skill reader 模式）：

1. server name：`memory_command_reader`
2. tool name：`memory_command_reader_get_command_detail`
3. 入参：
   - `command_ref`（必填，例：`CMD2`）
4. 出参：
   - `command_ref`
   - `name`
   - `plugin_source`
   - `source_path`
   - `description`
   - `argument_hint`
   - `content`（完整正文）

解析流程（ChatOS 内部）：

1. 读取当前 contact 的 runtime context。
2. 用 `command_ref` 定位 `plugin_source + source_path`。
3. 调 memory 插件详情接口读取目标 command 正文并返回。
4. 若 `command_ref` 不属于当前联系人，直接拒绝。

涉及文件：

1. `chat_app_server_rs/src/services/builtin_mcp.rs`
2. `chat_app_server_rs/src/core/mcp_runtime.rs`
3. `chat_app_server_rs/src/core/mcp_tools/builtin.rs`
4. `chat_app_server_rs/src/builtin/memory_command_reader/mod.rs`（新增）
5. `chat_app_server_rs/src/services/memory_server_client/dto.rs`
6. `chat_app_server_rs/src/services/memory_server_client/skill_ops.rs`

## 4.5 显式触发：`/command` 语义

在 `chat_v2/chat_v3` 增加轻量解析：

1. 识别输入：`/^\/([a-z0-9-_]+)\b(.*)$/i`
2. 将 `/<name>` 映射到 runtime command（按 `name` 和文件名别名匹配）。
3. 命中后：
   - 自动取 command 正文
   - 注入一条额外 system 消息：声明“用户显式调用 command”
   - 把剩余文本作为 command arguments 传给模型理解
4. 未命中则按普通消息处理。

重要边界：

1. command 只是“任务流程模板”，不是“直接执行 shell”。
2. 是否调用实际工具仍由模型在 MCP 工具约束下完成。

## 4.6 快照可观测性

扩展 turn runtime snapshot 的 `runtime` 字段，新增：

1. `selected_commands`: `[{ command_ref, plugin_source, source_path, trigger, arguments }]`

记录策略：

1. 显式 `/command` 触发时立即写入 `trigger=explicit`。
2. 模型通过 `memory_command_reader_get_command_detail` 拉取时写入 `trigger=implicit`。

这样可以在“轮次上下文抽屉”里清楚看到这轮 command 使用情况。

## 5. 分期落地

### Phase 1（先可用）

1. Memory 增加 `runtime_commands`。
2. ChatOS system prompt 增加 command 索引段。
3. ChatOS 增加 `memory_command_reader_get_command_detail` 内置工具。

验收标准：

1. 模型能看到 `CMDx` 列表。
2. 模型可成功调用 command reader 获取全文。
3. 不依赖随机长 ID。

### Phase 2（增强体验）

1. 增加 `/command args` 显式触发。
2. turn snapshot 记录 `selected_commands`。

验收标准：

1. `/team-debug xxx` 可被正确识别并转化为 command 上下文。
2. 快照可回放到“本轮用了哪个 command”。

### Phase 3（可选优化）

1. 在输入框加 command 自动补全。
2. 基于用户问题给出“建议命令”提示。

## 6. 测试清单

1. 导入测试：`commands/*.md` frontmatter 的 `description/argument-hint` 正确落库。
2. 运行时测试：`runtime_commands` 顺序稳定，`CMDx` 映射稳定。
3. 工具测试：`memory_command_reader_get_command_detail` 权限边界正确。
4. 端到端测试：联系人聊天中能按 `CMDx` 拉全文并按流程执行。
5. 显式触发测试：`/command` 命中、未命中、歧义命中三类分支。
6. 快照测试：running/completed 阶段都能看到 `selected_commands`。

## 7. 风险与规避

1. 风险：command 全文过长导致上下文膨胀。
   - 规避：默认索引 + 按需拉取，保留“不截断原文”的产品语义。
2. 风险：不同插件有同名 command。
   - 规避：以 `plugin_source + source_path` 做真实定位，`CMDx` 仅做人机引用。
3. 风险：插件更新后 command 映射漂移。
   - 规避：按当前 runtime context 动态生成 `CMDx`，并在 snapshot 记录实际 source。

## 8. 本方案与当前代码的关系

这份方案是“在现有已完成的 commands 导入能力上做聊天运行时闭环”，不会推翻现有数据结构，仅做增量：

1. Memory 侧补结构化 runtime command 索引。
2. ChatOS 侧补 command reader 内置 MCP 与显式触发逻辑。
3. 快照侧补 command 使用记录。

