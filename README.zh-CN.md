# Okra

[English](./README.md) · 简体中文

> 让 AI 进入项目，把事情做完。

Okra 是 Chat OS 的产品名称，也是一位面向真实项目的 AI 工作搭档。

它不只回答问题。Okra 可以和你讨论需求、理解项目背景、拆解工作、读取代码、使用工具、运行任务，并把重要信息整理成可以继续使用的项目记忆。

简单问题可以直接聊；复杂工作可以交给后台任务持续推进。你随时可以回来查看进度、工具输出、代码变更和最终结果，而不必每次重新解释项目的来龙去脉。

## 它能为你做什么

### 把模糊想法变成可执行计划

从一句自然语言开始，和 Okra 一起澄清目标、约束与验收标准。

在项目计划中，你可以逐步沉淀：

- 产品需求与业务目标
- 技术方案和项目文档
- 可以真正执行的项目任务
- 任务之间的前置依赖
- 当前进度、失败原因与后续工作

计划不是一份聊完即丢的回答。确认后，关联任务可以直接进入执行流程。

### 让 AI 在真实项目里工作

Okra 可以进入正确的工程环境，而不是让你不断复制粘贴代码和命令。

根据项目类型，它可以使用：

- 项目文件与全文搜索
- Git 状态、分支、Diff、提交与同步
- 终端和长时间运行的命令
- 浏览器、代码维护和其他工程工具
- 项目的语言、工具链和环境变量
- 云端隔离环境或你授权的本机目录

所有操作都会围绕当前项目进行，并保留可回看的执行过程。

### 把复杂工作交给后台任务

当一个需求需要很多步骤时，Okra 可以把它交给任务系统执行，而不是要求你一直守在聊天窗口前。

你可以看到：

- 当前正在处理什么
- 已完成、进行中、阻塞和失败的任务
- AI 调用了哪些工具
- 命令、运行日志和代码变更
- 是否需要你补充信息或确认操作
- 成功、停止、失败与重试结果

任务生命周期由云端持续管理，因此离开页面后仍可继续。使用本机文件、命令、Plugin、MCP 或沙箱能力的任务，需要桌面客户端和 Local Connector 保持在线。

### 记住项目，而不是只记住一轮聊天

Okra 会持续整理项目背景、重要决定、会话摘要和角色记忆，让长期协作不被单次会话切断。

这意味着你可以：

- 在新的会话里继续之前的工作
- 让 AI 记住项目约定与个人偏好
- 查看当前会话已经沉淀的总结
- 从其他相关会话召回重要信息
- 在本地项目中主动“忘记”不再需要的 Recall

记忆不是无限堆积的聊天记录。系统会通过摘要和分层整理，尽量把真正有用的信息留在后续上下文中。

### 创建适合不同项目的 AI 搭档

不同项目需要不同的合作方式。你可以创建多位职责清晰的智能体，再为每个项目选择一位作为当前“联系人”。聊天、项目上下文和后台任务都会默认由这位联系人继续推进。

例如：

- 负责需求澄清的产品搭档
- 熟悉某套技术栈的开发搭档
- 专注测试、排障或代码评审的工程搭档
- 负责长期维护项目文档与任务状态的项目搭档

每位智能体都可以拥有自己的角色定义、行为边界、模型、技能与工具能力。需要更换合作角色时，可以在项目没有运行中任务的情况下更换联系人。

## 适合哪些场景

### 从零启动一个项目

告诉 Okra 你想做什么，让它协助梳理需求、技术方案、任务依赖和验收方式，再按计划逐步完成工程实现。

### 接手或维护已有代码库

导入 Git 项目或授权本机目录，让 Okra 先阅读工程结构，再处理功能开发、重构、测试补齐、依赖升级和故障修复。

### 推进需要很久的复杂任务

把多步骤工作交给任务系统，在统一界面里查看执行状态、失败回执、重试结果和最终交付，而不是依赖一段不可追踪的长对话。

### 经营长期个人项目

让项目背景、历史决定、待办任务和会话记忆持续积累。隔几天或几周回来，仍然可以从已有上下文继续推进。

### 为不同项目选择合适角色

创建产品、开发、测试或研究型联系人，再根据当前项目选择最合适的一位长期协作。

## 三步开始使用

### 1. 创建项目

你可以选择：

- 从 Git 仓库创建云端项目
- 上传 ZIP 创建云端项目
- 在桌面客户端中选择一个已授权的本机目录

### 2. 告诉 Okra 你想完成什么

直接描述目标，也可以补充限制条件、已有资料和验收标准。

如果问题比较复杂，可以进入 Plan 模式，让 Okra 先整理需求、文档、任务和依赖，再决定是否开始执行。

### 3. 查看过程并继续协作

任务运行期间，你可以查看进度、发送补充引导、回答 AI 提出的问题、停止任务或在失败后重试。

完成后，结果、项目变化和后续建议会回到同一个会话与项目上下文中。

## 云端项目还是本地项目

Okra 不要求所有项目都使用同一种工作方式。

| | 云端项目 | 本地项目 |
| --- | --- | --- |
| 适合 | 希望直接在浏览器使用、需要隔离环境或云端持续运行 | 代码主要保存在自己电脑、需要使用本机工具链和目录 |
| 创建方式 | Git 仓库或 ZIP | 桌面客户端中授权本机目录 |
| 工作环境 | 云端 Git 工作区与隔离运行环境 | 用户授权的本机工作区 |
| 使用入口 | 浏览器或桌面客户端 | 桌面客户端 |
| 任务编排 | 云端后台任务 | 云端编排，按需调用当前设备能力 |
| 项目记忆 | 保存在云端 | 保存在云端 |

两类项目共享同一套云端业务编排。工具的执行位置由 MCP Management 根据 Project Context 明确选择；本机能力不可用时会明确失败或等待，不会偷偷切换到云端文件系统或其他设备。

### 关于本地项目与隐私

本地项目的 Project、Session、Message、Task、Requirement、Memory 和 Agent 生命周期由云端服务管理；文件、Git、命令、本地 Skill/Plugin/MCP、沙箱和审批仍在你明确授权的设备边界内执行。

需要注意：

- Okra 云端只保存 Workspace 和设备路由所需的逻辑标识，不保存你的本机绝对路径。
- 项目业务数据以云端为唯一事实来源，不在客户端保存另一份 Session、Task 或 Memory 数据。
- AI 工作时，完成推理所需的内容仍可能发送给你所选择的模型供应商；请同时了解该供应商的数据政策。
- 账号、智能体能力、模型目录和系统策略等控制信息可以随账号同步。
- 终端、文件和 Git 操作只能在已授权的工作区边界内进行。

## 你可以在 Okra 中看到什么

### 对话空间

和项目联系人持续交流，查看 AI 的回答、思考阶段、工具过程、任务状态和历史消息。

### 项目计划

集中查看需求、技术文档、项目任务、依赖关系与执行状态，并从需求直接发起关联任务。

### 项目工作区

浏览和搜索文件，查看 Git 变化，编辑项目内容，配置运行方式，并启动或检查项目实例。

### 任务中心

查看后台任务、运行记录、人工确认、工具状态、成功结果和失败原因。

### 记忆视图

查看会话摘要和可召回记忆，执行复盘，并管理项目的自动摘要与 Recall。

### 智能体与能力设置

创建智能体，选择模型，启用需要的工具与技能，并设置不同任务使用的默认模型和思考等级。

## 开始之前需要准备什么

### 使用云端项目

1. 打开你所在部署的 Okra 网站。
2. 注册或登录账号；部分测试环境可能需要邀请码。
3. 在设置中添加可用的云端 AI 模型。
4. 创建或导入一个云端项目。
5. 添加项目联系人，然后开始对话或规划。

### 使用本地项目

1. 从 Okra 官网下载并安装 Okra 桌面连接器。
2. 使用与 Web 端相同的账号登录。
3. 添加并授权一个本机工作区。
4. 配置需要使用的本地工具、Skill、Plugin、MCP、沙箱和审批权限。
5. 在桌面端创建本地项目。

本地项目只能在桌面客户端中使用。直接在普通浏览器中打开 Okra，不会获得访问本机目录的能力。

## 常见问题

### Okra 和普通 AI 聊天工具有什么不同？

普通聊天工具主要生成回答；Okra 围绕长期项目协作设计。它可以连接项目环境、使用工具、管理计划和任务、持续汇报进度，并在后续会话中使用已经沉淀的项目上下文。

### 必须把代码上传到云端吗？

不需要。使用桌面客户端创建本地项目后，代码可以继续保存在你授权的本机目录中。如果希望使用浏览器、云端隔离环境和云端后台运行，则可以选择云端项目。

### 可以使用自己的模型服务吗？

可以。Okra 支持配置 OpenAI 兼容的模型服务，并允许为普通聊天、项目管理、环境分析、记忆总结和任务执行选择不同模型。

### AI 执行过程中我还能干预吗？

可以。你可以查看工具过程、回答人工确认、发送补充引导、停止当前运行，并在失败后重新执行。

### 关闭页面后任务还会继续吗？

任务编排和业务状态会在云端继续运行。若本地项目的下一步需要访问本机文件、命令或其他设备能力，桌面客户端和 Local Connector 必须在线。

### 云端项目和本地项目可以自动互相切换吗？

不会。两种项目拥有不同的文件与工具执行边界，但共享云端业务编排。MCP Management 会根据 Project Context 固定路由，不会静默切换到错误的文件系统或设备。

## 当前产品状态

Okra 仍在快速迭代。当前需要留意：

- 本地项目必须使用桌面客户端。
- 本地项目暂不支持对话附件，也暂不支持在任务运行过程中附带图片或文件进行补充引导。
- 本地项目的业务历史保存在云端；2.0.10 不迁移旧客户端 SQLite 中的历史会话、任务和记忆。
- 可公开下载的桌面平台、版本和注册规则以对应部署的 Okra 官网为准。

## 技术与自部署参考

以下内容面向需要部署、调试或二次开发 Okra 的维护者；普通使用者不需要阅读。

<details>
<summary>展开技术架构与开发命令</summary>

### 执行架构

Okra 使用一套云端业务编排平面，并按需调用设备侧能力执行器：

- Project、Session、Task、Requirement、Memory 和 Agent 生命周期全部以云端服务为事实数据源。
- Local Connector Core 只执行必须在用户设备完成的 Workspace 文件、Git、命令、本地 Skill/Plugin/MCP、沙箱和审批能力。
- Agent 工具调用统一由 MCP Management 根据 Project Context 选择 Local Connector、Harness 或 Cloud Sandbox。
- Local Connector 不可用时明确失败，不会把设备侧操作静默切换到其他执行位置。

### 本地数据位置

Local Connector 默认状态路径：

```text
~/.chatos/local_connector/state.json
~/.chatos/local_connector/connector-state.sqlite3
```

可通过 `LOCAL_CONNECTOR_STATE_PATH` 更改状态文件位置；SQLite 只保存 Connector 控制状态并位于同一目录。2.0.10 不迁移、也不再使用旧 `runtime.sqlite3`。

### 启动自托管云端栈

要求 Docker Engine 与 Docker Compose v2：

```bash
cp docker/bootstrap.conf.example docker/bootstrap.conf
make docker-up
```

默认主应用地址为 <http://localhost:8088>。业务配置统一通过配置中心发布；`docker/bootstrap.conf` 只保存配置中心可用前必须提供的基础设施参数和凭据，且不得提交。

从当前源码构建镜像：

```bash
make dev
```

宿主机开发模式：

```bash
make local-dev
make local-dev-status
make local-dev-logs SERVICE=chatos-backend
make local-dev-stop
```

### Local Connector 开发

启动 Core 和设置页：

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

完整本地项目体验依赖 Electron 提供的可信 Runtime Bridge。Core/设置页开发模式本身不等于完整桌面客户端。

macOS 打包：

```bash
./local_connector_client/package-electron-macos-client.sh
```

Windows 打包：

```powershell
powershell -ExecutionPolicy Bypass `
  -File .\local_connector_client\package-electron-windows-client.ps1
```

Linux Core 档打包（必须在 Linux 上运行）：

```bash
./local_connector_client/package-electron-linux-client.sh
```

Linux 包包含桌面客户端、Local Connector Core、沙箱 MCP 服务和经过校验的 Plugin/Skill
清单。浏览器自动化、Chrome Native Messaging、Computer Use 和内置文档运行时需等待
Linux 原生运行资源补齐后再纳入完整发布档。

### 构建与测试

```bash
make build
make smoke
make test
```

核心执行平面可以单独测试：

```bash
cargo test -p chat_app_server_rs
cargo test -p task_runner_service_backend
cargo test -p local_connector_client_core
cd memory_engine/backend && cargo test
```

### 架构事实来源

- 部署边界与端口：`docker/compose.yml`
- Rust workspace：`Cargo.toml`
- 云端业务 API 与本地能力桥：`chatos/frontend/src/lib/api/client/facades/`
- 云端执行边界：`chatos/backend/src/core/project_execution.rs`
- Local Connector 能力执行器：`local_connector_client/core/src/local_runtime/`
- Local Connector 控制状态 Schema：`local_connector_client/core/migrations/`
- 云端 Task Runner：`task_runner_service/backend/src/services/`
- 项目运行环境：`project_management_service/backend/src/services/runtime_environment.rs`
- 开发与部署命令：`Makefile`、`docker/deploy.sh`、`scripts/local-dev-stack.sh`
- `chatos_3d_anime_prototype/` 是实验性界面，不是当前正式产品入口。

</details>

## License

本项目使用 [PolyForm Noncommercial License 1.0.0](./LICENSE)。第三方组件说明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
