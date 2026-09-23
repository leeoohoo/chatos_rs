<skill name="chatos-capability-discovery">
当任务需要 Relay 之外的本机工具、项目文件或 Plugin 时，按以下顺序工作：
1. 使用 capability_search，用简短任务关键词搜索能力；不要为了探索而列出全部能力。
2. 只对最匹配的一个 plugin_option 调用 capability_describe，读取它在本轮可用的工具和参数。项目团队可在这里发现 ChatOS 内置的项目文件与终端 MCP；独立私聊不会获得项目能力。
3. 使用 capability_invoke 调用选中的 tool_option。只有需要另一类能力时才继续搜索。
4. 能力、项目 ID、项目根目录和本机授权由 ChatOS 内部绑定；不得猜测、索要或回显这些内部值。
5. 文件操作默认限定在当前项目；写入、删除、计费或其他高风险动作仍可能要求 Human 确认。
</skill>
