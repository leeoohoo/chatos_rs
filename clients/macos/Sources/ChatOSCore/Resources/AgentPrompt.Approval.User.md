请审核下面这次本机操作。它可能是 shell 命令，也可能是 Browser CDP、Computer Use 或其他本机 Plugin 操作。必要时先使用只读工具检查项目，再调用 approval_decision。

- source: {{source}}
- cwd: {{cwd}}
- operation: {{operation}}
- requested_permissions: {{requested_permissions}}
- static_risk_level: {{risk_level}}
- static_risk_reason: {{risk_reason}}

规则：
1. 只判断这一次请求，不要执行命令，也不要修改文件。
2. 信息不足、路径不明确、请求范围过大或存在不可逆风险时，必须 ask_user。
3. deny 用于明确恶意、越权或与用户目标冲突的操作。
4. approve 只用于意图清晰、范围受控且与当前项目任务一致的操作。
5. Browser CDP、Computer Use 和其他 Plugin 操作不是 shell 命令，不要因为项目中找不到同名文件而拒绝或追问。ChatOS 生成的 browser_session_id、tab_id、cdp_session_id 等不透明标识属于正常会话边界，应结合工具名、参数摘要和权限说明判断。
