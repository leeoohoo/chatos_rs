# Chatos RS

Chatos RS 是一个面向软件工程场景的 AI 协作与任务执行平台。它把对话式协作、项目管理、模型与插件配置、长期记忆、异步任务编排、云端沙箱和本机工作区连接整合在同一套系统中。

> 当前仓库采用“云端服务 Docker Compose 部署、本机 Connector 独立运行”的形态。主应用负责交互和编排，真正的工程任务可以按项目配置进入云端沙箱或用户本机执行。

[English README](./README.en.md) · [安装指南](./INSTALL_GUIDE.zh-CN.md) · [部署命令](./DEPLOY_COMMANDS.zh-CN.md)

## 项目解决什么问题

Chatos RS 不只是聊天界面，而是一套围绕“理解需求、形成计划、调用工具、执行代码、沉淀上下文”构建的工程协作平台：

- 在 Chatos 中进行普通对话、规划对话和工程任务协作。
- 将长耗时工作交给 Task Runner，通过事件流和回调持续同步结果。
- 用 Project Management Service 管理项目、需求、项目任务及其执行映射。
- 在云端 Docker 沙箱或用户本机工作区中运行终端、文件和 MCP 工具。
- 用 Memory Engine 统一保存线程记录、摘要、上下文和长期记忆。
- 用 Plugin Management Service 统一管理 MCP、Skill、Skill Package 和系统 Agent 能力。
- 用 User Service 统一处理用户、Agent 账号、模型配置、令牌交换与 Harness 账号资源。

## 总体架构

```mermaid
flowchart TB
    user["用户 / 管理员"]
    localHost["用户本机<br/>Local Connector Client"]
    model["OpenAI 兼容模型服务"]

    subgraph access["接入层"]
        web["Chatos Web<br/>React + Vite"]
        consoles["领域管理台<br/>User / Project / Task / Plugin / Sandbox / Memory"]
    end

    subgraph control["控制面"]
        chatos["Chatos Backend<br/>会话、Agent 编排、实时事件"]
        userSvc["User Service<br/>身份、模型配置、令牌"]
        projectSvc["Project Management<br/>项目、需求、项目任务"]
        pluginSvc["Plugin Management<br/>MCP、Skills、Agent 能力"]
        memory["Memory Engine<br/>记录、摘要、上下文、记忆"]
    end

    subgraph execution["执行面"]
        runner["Task Runner<br/>调度、Worker、AI/MCP 执行循环"]
        sandbox["Sandbox Manager<br/>云端沙箱租约与代理"]
        connectorSvc["Local Connector Service<br/>云端中继与设备状态"]
        agent["Sandbox Agent<br/>文件、终端、MCP 工具"]
    end

    subgraph platform["平台基础设施"]
        mongo[("MongoDB")]
        consul["Consul<br/>服务注册、发现、配置"]
        harness["Harness<br/>Git、仓库与 CI"]
        docker["Docker Engine"]
    end

    user --> web
    user --> consoles
    web --> chatos
    consoles --> control
    chatos --> userSvc
    chatos --> projectSvc
    chatos --> pluginSvc
    chatos --> memory
    chatos --> runner
    runner --> userSvc
    runner --> projectSvc
    runner --> pluginSvc
    runner --> memory
    runner --> sandbox
    runner --> connectorSvc
    sandbox --> agent
    connectorSvc <--> localHost
    chatos --> model
    runner --> model
    userSvc --> harness
    control --> mongo
    execution --> mongo
    control -.注册与发现.-> consul
    execution -.注册与发现.-> consul
    sandbox --> docker
    localHost --> docker
```

### 分层职责

| 层次 | 核心组件 | 主要职责 |
| --- | --- | --- |
| 接入层 | Chatos Frontend、各领域管理台、Local Connector UI | 用户交互、配置管理、状态展示 |
| 控制面 | Chatos、User、Project、Plugin、Memory | 身份与配置、会话编排、项目领域数据、能力策略、上下文治理 |
| 执行面 | Task Runner、Sandbox Manager、Local Connector、Sandbox Agent | 任务调度、模型工具循环、命令执行、沙箱生命周期、结果回传 |
| 数据与基础设施 | MongoDB、Consul、Harness、Docker | 持久化、服务发现、代码托管与 CI、隔离运行环境 |

## 两条核心运行链路

### 1. 交互式对话链路

Chatos Backend 负责会话、消息、流式响应和工具编排。它会从 User Service 解析用户与模型配置，从 Plugin Management Service 解析可用能力，并按需读写 Memory Engine。

```mermaid
sequenceDiagram
    autonumber
    actor U as 用户
    participant F as Chatos Frontend
    participant C as Chatos Backend
    participant I as User Service
    participant P as Plugin Management
    participant M as Memory Engine
    participant L as 模型服务

    U->>F: 发送消息
    F->>C: HTTP / WebSocket 请求
    C->>I: 校验令牌并读取模型配置
    C->>P: 解析当前 Agent 的 MCP / Skill 能力
    C->>M: 组装近期记录、摘要和长期记忆
    C->>L: 发起模型请求
    loop 模型要求调用工具
        L-->>C: tool call
        C->>C: 通过共享 MCP Runtime 执行工具
        C->>L: 返回工具结果
    end
    L-->>C: 最终回答
    C->>M: 写入消息、工具记录与快照
    C-->>F: 流式事件
    F-->>U: 展示结果
```

### 2. 工程任务执行链路

长耗时、可调度或需要隔离环境的工作交给 Task Runner。Task Runner 选择执行环境，运行模型与 MCP 工具循环，再将状态同步给 Project Management，并回调 Chatos。

```mermaid
sequenceDiagram
    autonumber
    actor U as 用户
    participant C as Chatos
    participant P as Project Management
    participant T as Task Runner
    participant I as User Service
    participant G as Plugin Management
    participant M as Memory Engine
    participant E as 执行环境

    U->>C: 创建或启动工程任务
    C->>P: 读取项目、需求和运行环境
    C->>T: 创建 Task Runner 任务并启动 Run
    T->>I: 交换执行令牌、解析模型配置
    T->>G: 解析 MCP、Skill 与 Agent 能力
    T->>M: 获取任务上下文
    T->>E: 申请沙箱或连接本机工作区
    loop Worker 执行循环
        T->>T: 模型推理、计划与工具调度
        T->>E: 文件、终端、Git 或 MCP 操作
        E-->>T: 输出、事件和变更
    end
    T->>P: 同步项目任务与执行状态
    T->>M: 写入执行记录和摘要素材
    T-->>C: 回调任务进度与最终结果
    C-->>U: 实时展示状态和结果
```

## 云端与本机执行环境

项目运行环境决定任务落在哪个执行面。两条路径共享 Task Runner 的模型与工具编排，但工作区位置和沙箱控制方不同。

```mermaid
flowchart LR
    task["Task Runner Run"] --> mode{"项目运行环境"}

    mode -->|cloud| cloud["Sandbox Manager"]
    cloud --> lease["创建 / 复用沙箱租约"]
    lease --> cloudAgent["云端 Sandbox Agent"]
    cloudAgent --> cloudResult["输出清单与变更"]

    mode -->|local_connector| relay["Local Connector Service"]
    relay <--> ws["出站 WebSocket"]
    ws <--> client["用户本机 Connector Core"]
    client --> grant["已授权本机工作区"]
    client --> localDocker["可选的本机 Docker 沙箱"]
    grant --> localResult["终端输出与文件变更"]
    localDocker --> localResult

    cloudResult --> task
    localResult --> task
```

本机模式的关键安全边界：

- 云端只保存设备、工作区别名和指纹，不保存用户本机绝对路径。
- Connector Client 主动建立出站连接，云端不会直接访问用户机器的 `localhost`。
- 所有命令和文件操作必须落在用户明确授权的工作区内。
- 本机 Docker 沙箱由 Connector Core 管理，不经过云端 Sandbox Manager。

## 数据与控制关系

```mermaid
flowchart TB
    subgraph stateless["尽量无状态的 API / 编排服务"]
        C["Chatos"]
        U["User Service"]
        P["Project Management"]
        T["Task Runner"]
        G["Plugin Management"]
        M["Memory Engine"]
        L["Local Connector Service"]
        S["Sandbox Manager"]
    end

    DB[("MongoDB<br/>按服务使用独立数据库")]
    V[("Docker Volumes<br/>工作区、运行数据、Harness 数据")]
    D["Consul<br/>服务目录与配置中心"]
    H["Harness<br/>用户仓库、Git 与 CI"]
    DE["Docker Engine<br/>平台容器与动态沙箱"]

    stateless --> DB
    C --> V
    T --> V
    L --> V
    stateless -.注册 / 发现.-> D
    U --> H
    P --> U
    S --> DE
    H --> DE
```

MongoDB 是主要业务存储，但各服务保持独立数据库边界；服务之间通过 HTTP、回调、MCP JSON-RPC 和 WebSocket 协作，不共享业务表。

## 服务清单

| 组件 | 默认地址 | 代码位置 | 职责 |
| --- | --- | --- | --- |
| Chatos | Web `8088` / API `3997` | `chatos/` | 主应用、会话、Agent 编排、实时事件、项目入口 |
| User Service | Web `39191` / API `39190` | `user_service/` | 用户与 Agent 账号、认证、模型配置、令牌交换、Harness 资源 |
| Memory Engine | Web `4178` / API `7081` | `memory_engine/` | 线程记录、摘要、上下文组装、长期记忆、后台记忆任务 |
| Task Runner | Web `39091` / API `39090` | `task_runner_service/` | 任务、调度、Worker、AI/MCP 执行循环、运行事件 |
| Project Management | Web `39211` / API `39210` | `project_management_service/` | 项目、需求、项目任务、依赖关系、运行环境与执行映射 |
| Plugin Management | Web `39261` / API `39260` | `plugin_management_service/` | MCP、Skill、Skill Package、系统 Agent 与能力绑定 |
| Sandbox Manager | Web `8096` / API `8095` | `sandbox_manager_service/` | 云端沙箱、租约、池、镜像与 Sandbox Agent 代理 |
| Local Connector Service | API `39230` | `local_connector_service/` | 设备注册、工作区映射、云端到本机的中继 |
| Local Connector Client | Core `39232` / Dev UI `39233` | `local_connector_client/` | 本机授权、PTY、命令、文件、MCP 与本机 Docker 沙箱 |
| Official Website | Web `39251` / API `39250` | `official_website_service/` | 官网与服务状态展示 |
| Harness | HTTP `3000` / SSH `3022` | 外部镜像 / 独立源码 | Git 仓库、代码托管与 CI |
| Consul | `8500` | Docker Compose | 服务注册、发现和配置中心 |
| MongoDB | 宿主机 `27018` | Docker Compose | 各领域服务的业务持久化 |

## 共享 Rust 能力层

仓库根 `Cargo.toml` 维护主要 Rust workspace；`crates/` 用于承载跨服务复用的运行时和协议，避免 Chatos 与 Task Runner 各自实现一套工具链。

```mermaid
flowchart LR
    apps["Chatos / Task Runner / Project / Connector"] --> ai["chatos_ai_runtime<br/>模型请求、迭代执行、上下文与工具循环"]
    apps --> mcp["chatos_mcp_runtime<br/>MCP 注册、Schema、RPC 与执行器"]
    apps --> builtins["chatos_builtin_tools<br/>终端、浏览器、AskUser、TaskManager 等"]
    apps --> mcpService["chatos_mcp_service<br/>MCP 服务端协议与能力策略"]
    apps --> discovery["chatos_service_runtime<br/>Consul、配置中心、服务发现"]
    apps --> pluginSdk["chatos_plugin_management_sdk<br/>能力查询、缓存与策略"]
    apps --> memorySdk["memory_engine_sdk<br/>记录、摘要、记忆与上下文客户端"]
    apps --> projectContract["chatos_project_mcp_contract<br/>项目管理 MCP 契约"]
    apps --> imageMcp["chatos_sandbox_image_mcp<br/>沙箱镜像 MCP"]
```

## 仓库结构

```text
chatos_rs/
├── chatos/                         # 主应用前后端
├── crates/                         # 跨服务共享 Rust runtime / SDK / contract
├── user_service/                   # 身份、模型配置与令牌
├── memory_engine/                  # 长期记忆与上下文引擎
├── task_runner_service/            # 异步任务与执行 Worker
├── project_management_service/     # 项目领域服务
├── plugin_management_service/      # MCP、Skill 与 Agent 能力管理
├── sandbox_manager_service/        # 云端沙箱管理与 Sandbox Agent
├── local_connector_service/        # 云端本机连接器中继
├── local_connector_client/         # 用户本机 Connector Core / UI / Electron
├── official_website_service/       # 官网与状态页
├── docker/                         # Compose、镜像构建与部署脚本
├── scripts/                        # 本地开发、迁移、质量与契约治理脚本
└── docs/                           # 设计、计划、归档与维护文档
```

> `memory_engine/backend` 当前独立于根 Rust workspace 构建；`user_service/backend` 也保留独立构建入口。完整构建命令已经在根 `Makefile` 中统一编排。

## 快速启动

### 方式一：使用预构建镜像

适合首次体验和部署环境。默认从 GHCR 拉取镜像，不在本机编译源码。

```bash
cp docker/.env.example docker/.env
# 编辑 docker/.env，至少检查外部模型 API Key 和生产环境密钥
docker/deploy.sh up
```

也可以使用 Make：

```bash
make docker-up
```

启动后访问：

- 主应用：<http://localhost:8088>
- Consul：<http://localhost:8500>
- Harness：<http://localhost:3000>

### 方式二：从本地源码构建 Docker 镜像

```bash
docker/deploy.sh dev
# 或
make dev
```

只重建发生变化的服务：

```bash
docker/deploy.sh rebuild task-runner-backend
docker/deploy.sh rebuild chatos-backend chatos-frontend
docker/deploy.sh build-services
```

### 方式三：宿主机快速开发栈

该模式保留 MongoDB、Harness 等基础设施在 Docker 中运行，业务后端使用 `cargo run`，前端使用 Vite，适合频繁修改和调试。

```bash
make local-dev
make local-dev-status
make local-dev-logs SERVICE=chatos-backend
make local-dev-stop
```

## 启动本机 Connector

`local_connector_client/` 必须运行在用户机器上，因为它需要访问本机工作区、PTY 和可选的本机 Docker。云端 Compose 不会启动它。

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

Windows Electron 打包方式见 [Local Connector Client README](./local_connector_client/README.md)。

## 常用运维命令

```bash
docker/deploy.sh ps
docker/deploy.sh logs
docker/deploy.sh restart
docker/deploy.sh fast
docker/deploy.sh clean-images
docker/deploy.sh down
docker/deploy.sh reset
```

- `up`：拉取预构建镜像并启动。
- `fast`：复用本地已有镜像，跳过拉取。
- `dev`：从当前源码构建镜像并启动。
- `rebuild`：只重建指定 Compose 服务。
- `reset`：停止服务并删除数据卷，会清除本地环境数据。

## 配置与安全边界

- Docker 云端配置：`docker/.env.example` → `docker/.env`。
- Local Connector 宿主机配置：根目录 `.env.example`。
- 内部服务 token 带有开发默认值，仅用于本地环境；生产部署必须替换。
- Sandbox Manager 挂载 `/var/run/docker.sock`，等价于拥有较高的宿主机 Docker 管理权限，应部署在受控节点。
- Harness 同样使用 Docker Socket 运行 CI，生产环境应评估独立 Runner、网络隔离和最小权限。
- 不要把模型 API Key、JWT Secret、内部 API Secret 或 Connector 凭据提交到仓库。

## 构建、测试与质量门禁

```bash
make build
make smoke
make test
```

- `make build`：构建 Rust 服务与所有前端。
- `make smoke`：执行 API surface、路径基线、热点文件、Compose 和大文件检查。
- `make test`：在 smoke 基础上运行 Chatos 与 User Service 的重点测试、Lint 和类型检查。

仓库还维护 OpenAPI 契约装配、API 变更基线、依赖漂移、非测试 `unwrap/expect`、请求路径 panic 和代码体积检查，相关脚本位于 `scripts/`。

## CI 与镜像

GitHub Actions 和 Drone 配置用于构建、检查并推送服务镜像。默认镜像命名空间在 `docker/.env.example` 中配置：

```env
CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
CHATOS_IMAGE_TAG=latest
```

部署固定提交时可使用 `sha-<commit>` 镜像标签。若镜像仓库非公开，请先在部署机执行 `docker login ghcr.io`。

## 文档维护约定

- 架构事实以 `docker/compose.yml`、各服务入口和根 `Cargo.toml` 为准。
- 新增服务时同步更新本 README 的架构图、服务表、端口和仓库结构。
- 改动跨服务调用时同步检查环境变量、服务发现名、回调和内部鉴权。
- 本地过程性梳理文档放在 `docs/paln/`，按仓库规则不进入版本管理；历史专项方案仍保留在 `docs/plan/` 与 `docs/plans/`。

## License

本项目使用 [PolyForm Noncommercial License 1.0.0](./LICENSE)。第三方组件说明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
