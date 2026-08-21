# ChatOS Desktop UI · Revision 03

Revision 03 已推翻前两版的“深色科技控制台”和“浅色卡片式管理后台”方向，重新以成熟桌面 AI 工作区为目标设计。

参考来源：[Codex desktop app](https://developers.openai.com/codex/app)、[Claude Code on desktop](https://docs.anthropic.com/en/docs/claude-code/desktop)。参考的是产品工作流，不复制品牌视觉。

## 核心原则

- **Session first**：当前任务/会话是唯一视觉主角，不再用仪表盘和状态卡片争夺注意力。
- **Review on demand**：Files、Changes、Terminal、Memory 是同一个 Inspector 的不同状态，仅在需要审查时出现。
- **Context at the composer**：环境、权限、模型靠近输入框显示；不再设置一整条常驻 Context Ribbon。
- **One sidebar**：取消“导航轨 + 工作侧栏”的双层左导航，只保留项目、会话和入口都能承载的单侧栏。
- **Mostly monochrome**：整体以中性灰白为主；橙色只代表 ChatOS/执行，绿色代表完成，蓝紫表示模型、记忆和权限。
- **Product density**：正文使用可读字号；减少小标签、卡片边框和平均分栏，让空间服务于内容，而不是展示组件数量。

## ChatOS 的辨识度

- 会话侧栏同时承载项目、长期任务与 Task Runner 状态。
- 执行摘要内联在对话流中，详细变更进入右侧 Inspector。
- Memory 不再是常驻大侧栏，而是与 Changes / Files / Terminal 同级的审查视图。
- Local / Workspace / Model 状态集中在 Composer 底部，是任务真正生效的上下文。
- Project Plan 与会话、文件修改、运行记录保持可回溯连接。

## 页面

1. [设计总览](./00-overview-board.svg)
2. [登录](./01-login.svg)
3. [工作中心](./02-command-center.svg)
4. [任务会话 + Changes Inspector](./03-agent-chat.svg)
5. [Files + Diff + Preview](./04-project-workspace.svg)
6. [Project Plan](./05-project-plan.svg)
7. [Runtime + Terminal](./06-runtime-terminal.svg)
8. [AI 与模型设置](./07-ai-settings.svg)
9. [Agents & Apps](./08-agents-apps.svg)
10. [Notes](./09-notepad.svg)
11. [System Context](./10-system-context.svg)

## 重新生成

```bash
node docs/ui-redesign/generate-ui-svg.mjs
node --check docs/ui-redesign/generate-ui-svg.mjs
xmllint --noout docs/ui-redesign/*.svg
```

所有文件均为独立 SVG，不包含真实账号、密码、密钥或用户数据。
