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
- 任务依赖继续使用项目现有的 DAG 流程图：当前消息、直接/间接前置、上下文关联、聚焦、缩放和精简/完整图全部保留。
- 选中任务节点后，右侧 Inspector 展示执行过程、任务详情、Run、前置依赖和最近进展。

## 完整页面（23 张）

1. [设计总览](./00-overview-board.svg)
2. [登录](./01-login.svg)
3. [工作中心](./02-command-center.svg)
4. [任务会话 + Changes Inspector](./03-agent-chat.svg)
5. [Files + Diff + Preview](./04-project-workspace.svg)
6. [任务 DAG + Task Inspector](./05-project-plan.svg)
7. [Runtime + Terminal](./06-runtime-terminal.svg)
8. [AI 与模型设置](./07-ai-settings.svg)
9. [Agents & Apps](./08-agents-apps.svg)
10. [Notes](./09-notepad.svg)
11. [System Context](./10-system-context.svg)
12. [设计系统](./11-design-system.svg)
13. [任务阻塞与恢复](./12-task-blocked.svg)
14. [Task Run 执行详情](./13-task-run-detail.svg)
15. [项目计划工作区](./14-project-plan-workspace.svg)
16. [项目成员工作区](./15-project-team.svg)
17. [项目运行设置](./16-project-runtime-settings.svg)
18. [远程终端与 SFTP](./17-remote-workspace.svg)
19. [用户设置](./18-user-settings.svg)
20. [应用管理](./19-applications-manage.svg)
21. [创建流程](./20-creation-flows.svg)
22. [空、加载、离线、权限与失败状态](./21-product-states.svg)
23. [会话摘要与 Runtime Context](./22-session-summary-context.svg)

## 产品覆盖矩阵

| 产品模块 | 对应设计 | 覆盖内容 |
| --- | --- | --- |
| 登录与入口 | 01、02、20、21 | 登录、恢复工作、创建任务/项目/终端、空状态与错误恢复 |
| Agent 会话 | 03、22 | 对话流、执行摘要、Composer 上下文、会话总结、上下文预算 |
| 任务系统 | 05、12、13、21 | DAG 依赖、节点检查器、阻塞修复、Run 时间线、失败状态 |
| 项目工作区 | 04、14、15、16 | 文件 Diff、需求计划、成员协作、运行目标与环境设置 |
| 运行与远程 | 06、17 | 本地服务、终端、SSH、SFTP、传输队列 |
| Agent 与应用 | 08、19、20 | Agent 配置、应用连接、权限范围、安装入口 |
| 知识与配置 | 07、09、10、18 | 模型路由、笔记、System Context、用户偏好 |
| 视觉规范 | 00、11 | 产品总览、颜色、排版、控件、布局契约 |

任务 DAG 不是附属页面，而是 ChatOS 的核心工作面。设计保留项目现有的从上到下依赖布局、直接/间接前置、上下文虚线、运行态连线、聚焦与上下游弱化、精简/完整模式、缩放、节点动作和右侧任务检查器。

## 重新生成

```bash
node docs/ui-redesign/generate-ui-svg.mjs
node --check docs/ui-redesign/generate-ui-svg.mjs
xmllint --noout docs/ui-redesign/*.svg
```

所有文件均为独立 SVG，不包含真实账号、密码、密钥或用户数据。
