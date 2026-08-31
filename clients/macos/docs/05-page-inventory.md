# 页面清单与验收范围

## 一级页面

| ID | 页面 | 关键内容 | 首次阶段 |
| --- | --- | --- | --- |
| A01 | 登录/注册 | 平台账号、错误、开发环境标识 | Phase 1 |
| A02 | 全局资源壳 | 联系人、项目、终端、远端；记事本、主题、账号 | Phase 1 |
| A03 | 联系人会话 | 无项目范围的联系人会话、总结、上下文、Composer | Phase 2 |
| A03-P | 项目用户消息 | Turn 列表、完整历史、任务回调、Composer | Phase 1–2 |
| A03-H | 聊天历史状态 | 分页、会话恢复、滚动锚点、新消息提示、重连 | Phase 1–2 |
| A03-V | 会话内 Plugin 实时画面 | Computer Use / Browser MCP 画中画、会话归属、收起、等待首帧、过期隐藏 | Phase 2 |
| A04 | 项目目录 | 文件搜索与树、Preview/Edit、Git 菜单与弹层 | Phase 3 |
| A05 | 项目 Plan | Requirement 多列、需求/文档/任务详情、执行入口 | Phase 2–3 |
| A06 | Requirement 执行工作台 | 规划、确认、暂停/恢复/停止、任务运行过程 | Phase 2 |
| A06-G | 任务流程图 | 精简/完整 DAG、焦点、阻塞、Run 详情 | Phase 2 |
| A07 | 项目运行设置 | 运行目标、预检、实例、工具链、环境、运行终端 | Phase 3–4 |
| A08-T | 本地终端 | PTY、日志分页、断线恢复 | Phase 3–4 |
| A08-R | 远端连接 | SSH 配置、跳板机、验证、远端终端、SFTP | Phase 5 |
| A08 | Memory / Summary | 会话摘要、Recall、Runtime Context | Phase 2 |
| A09 | Agents | 联系人、角色、模型、能力、项目绑定 | Phase 3 |
| A10 | 云端 AI 偏好 | 云端默认模型、Task 模型、账号语言 | Phase 3 |
| A12 | Notes / System Context | 笔记树、编辑、上下文版本与激活 | Phase 3 |
| C01 | Connector 设备配对 | Core 状态、用户、设备、本机安全边界 | Phase 1/4 |
| C03 | Plugin Marketplace | 浏览、安装、更新、回滚、详情 | Phase 6 |
| C04 | 命令审批 | 三种审批模式、待审批、白名单、审计历史 | Phase 5 |
| C05 | 模型配置 | 云端模型只读同步、本机审批模型与参数 | Phase 5 |
| C06 | 运行与系统权限 | 开发环境、本机 Agent 版本、macOS 权限 | Phase 4–5 |
| C07 | 权限控制 | 文件、网络、AI 审批、运行 lease | Phase 5 |
| C08 | Connector 本机终端 | 链路测试、输出、命令历史 | Phase 4 |

## 关键创建流程

- 新建项目：选择 Local Connector 工作区 → 浏览/可新建目录 → 选择单个目录 → 由路径推导名称 → 创建。
- 新建会话：项目 → 联系人/Agent → 模型与能力 → 会话。
- 新建任务：目标 → 依赖 → 项目范围 → 权限 → 运行。
- 安装插件：来源 → 版本与签名 → 权限清单 → 安装 → 凭据/OAuth → 可用性验证。

## 全局状态页

每个一级页面必须覆盖：

- 首次空状态。
- 加载与分页。
- 网络离线。
- Local Connector 离线。
- 登录过期。
- 权限不足。
- 部分数据失败。
- 任务运行、阻塞、失败、取消、完成。
- 插件进程崩溃或不兼容。

## 设计稿覆盖

`docs/design/screens` 按 [页面真实逻辑矩阵](./09-page-logic-matrix.md) 落图。视觉组件可以复用，但页面 Shell、实体和操作必须逐页匹配真实逻辑。
