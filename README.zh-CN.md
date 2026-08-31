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

任务生命周期由云端持续管理，因此离开页面后仍可继续。使用本机文件、命令、Plugin、MCP 或受权限控制的设备能力时，需要桌面客户端和 Local Connector 保持在线。

### 记住项目，而不是只记住一轮聊天

Okra 会持续整理项目背景、重要决定、会话摘要和角色记忆，让长期协作不被单次会话切断。

这意味着你可以：

- 在新的会话里继续之前的工作
- 让 AI 记住项目约定与个人偏好
- 查看当前会话已经沉淀的总结
- 从其他相关会话召回重要信息
- 在当前项目中主动“忘记”不再需要的 Recall

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

安装桌面客户端，注册 Local Connector 工作区，然后从已授权目录创建项目。所有项目的文件、Git、搜索和命令都使用这个工作区。

### 2. 告诉 Okra 你想完成什么

直接描述目标，也可以补充限制条件、已有资料和验收标准。

如果问题比较复杂，可以进入 Plan 模式，让 Okra 先整理需求、文档、任务和依赖，再决定是否开始执行。

### 3. 查看过程并继续协作

任务运行期间，你可以查看进度、发送补充引导、回答 AI 提出的问题、停止任务或在失败后重试。

完成后，结果、项目变化和后续建议会回到同一个会话与项目上下文中。

## 项目与工作区

Okra 只有一种项目模型。每个项目都绑定一个已授权的 Local Connector 工作区，不再区分云端项目和本地项目。

- Project、Session、Message、Task、Requirement、Memory 和 Agent 生命周期由服务端编排。
- 项目文件、Git、搜索、命令、本地 Skill/Plugin/MCP、权限 relay 和审批只通过 Local Connector Client 执行。
- Harness 可以管理仓库资产、分支、同步、CI 和集成，但不会作为 MCP 项目文件或命令 provider。
- Local Connector 不可用时，工作区操作会明确失败或等待，不会回退到服务端文件系统、Harness 或其他执行环境。

### 关于项目与隐私

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

### 创建项目

1. 从 Okra 官网下载并安装 Okra 桌面连接器。
2. 使用你的 Okra 云端账号登录。
3. 添加并授权一个本机工作区。
4. 配置需要使用的本地工具、Skill、Plugin、MCP、权限控制和审批权限。
5. 在桌面端从该工作区创建项目。
6. 添加项目联系人，然后开始对话或规划。

ChatOS 主应用采用原生桌面客户端。浏览器中的管理页面不会获得访问本机目录的能力；工作区操作要求桌面客户端及其 Local Connector 在线。

## 常见问题

### Okra 和普通 AI 聊天工具有什么不同？

普通聊天工具主要生成回答；Okra 围绕长期项目协作设计。它可以连接项目环境、使用工具、管理计划和任务、持续汇报进度，并在后续会话中使用已经沉淀的项目上下文。

### 必须把代码上传到云端吗？

不需要。项目代码继续保存在 Local Connector 授权的目录中。AI 推理所需内容仍可能发送给你配置的模型供应商。

### 可以使用自己的模型服务吗？

可以。Okra 支持配置 OpenAI 兼容的模型服务，并允许为普通聊天、项目规划、记忆总结和任务执行选择不同模型。

### AI 执行过程中我还能干预吗？

可以。你可以查看工具过程、回答人工确认、发送补充引导、停止当前运行，并在失败后重新执行。

### 关闭页面后任务还会继续吗？

任务编排和业务状态会在服务端继续运行。若下一步需要访问项目文件、命令或其他设备能力，桌面客户端和 Local Connector 必须在线。

### 设备离线时，项目会回退到云端工作区吗？

不会。项目工作区只使用已绑定的 Local Connector。MCP Management 不会静默切换到 Harness、服务端文件系统、其他执行环境或其他设备。

## 当前产品状态

Okra 仍在快速迭代。当前需要留意：

- 项目工作区访问要求桌面客户端和 Local Connector 在线。
- 项目暂不支持对话附件，也暂不支持在任务运行过程中附带图片或文件进行补充引导。
- 业务历史由服务端保存；3.0.0 不导入已退役 Electron 客户端 SQLite 中的历史会话、任务和记忆。
- 可公开下载的桌面平台、版本和注册规则以对应部署的 Okra 官网为准。

## 技术与自部署参考

以下内容面向需要部署、调试或二次开发 Okra 的维护者；普通使用者不需要阅读。

<details>
<summary>展开技术架构与开发命令</summary>

### 执行架构

Okra 使用一套云端业务编排平面，并按需调用设备侧能力执行器：

- Project、Session、Task、Requirement、Memory 和 Agent 生命周期全部以云端服务为事实数据源。
- Local Connector Core 只执行必须在用户设备完成的 Workspace 文件、Git、命令、本地 Skill/Plugin/MCP、受权限控制的任务租约和审批能力。
- MCP Management 只把项目文件、Git、搜索和命令路由到已绑定的 Local Connector；Harness 只保留仓库与集成控制面职责。
- Local Connector 不可用时明确失败，不会把设备侧操作静默切换到其他执行位置。

### 启动自托管云端栈

要求 Docker Engine 与 Docker Compose v2：

```bash
cp docker/bootstrap.conf.example docker/bootstrap.conf
make docker-up
```

官网默认地址为 <http://localhost:39251>，统一 API 网关为 <http://localhost:9080>；ChatOS 主应用运行在原生客户端中。业务配置统一通过配置中心发布；`docker/bootstrap.conf` 只保存配置中心可用前必须提供的基础设施参数和凭据，且不得提交。

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

### 原生桌面客户端

在 macOS 14 或更高版本构建和测试 macOS 客户端：

```bash
make build-macos-client
make test-macos-client
clients/macos/scripts/package-debug-app.sh
```

在安装 .NET 8 与 WinUI 工作负载的 Windows 11 上构建和测试 Windows 客户端：

```powershell
dotnet build clients/windows/ChatOS.Win.sln --configuration Release
dotnet test clients/windows/ChatOS.Win.sln --configuration Release
clients\windows\build\package.ps1 -Platform x64
```

### 第一方插件

三个第一方插件与客户端、服务端一起维护：

- `plugins/browser`：Browser CDP 与 Chrome Bridge 扩展。
- `plugins/computer-use`：macOS 与 Windows 原生 Computer Use。
- `plugins/document`：受工作区边界约束的 Office 与 PDF 工具。

在受支持的原生开发环境使用 `make build-plugins` 和 `make test-plugins`。生成的安装包、下载依赖和 vendor 可执行文件不会提交到仓库。

### 构建与测试

```bash
make build
make smoke
make test
```

核心服务可以单独测试：

```bash
cargo test -p chat_app_server_rs
cargo test -p task_runner_service_backend
cd memory_engine/backend && cargo test
```

### 架构事实来源

- 部署边界与端口：`docker/compose.yml`
- Rust workspace：`Cargo.toml`
- macOS 原生客户端与 Local Connector：`clients/macos/Sources/`
- Windows 原生客户端与 Local Connector：`clients/windows/src/`
- 第一方插件：`plugins/browser/`、`plugins/computer-use/`、`plugins/document/`
- 项目编排边界：`chatos/backend/src/core/project_execution.rs`
- 云端 Task Runner：`task_runner_service/backend/src/services/`
- 开发与部署命令：`Makefile`、`docker/deploy.sh`、`scripts/local-dev-stack.sh`

</details>

## License

本项目使用 [PolyForm Noncommercial License 1.0.0](./LICENSE)。第三方组件说明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
