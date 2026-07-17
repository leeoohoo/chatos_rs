# 项目运行环境 Compose 编排与程序控制 MCP 实施方案

## 1. 目标

本方案解决项目运行环境初始化链路中的四个核心问题：

1. 数据库、缓存、配置中心、消息队列等依赖服务不得安装或承载文件读写、终端 MCP。
2. 哪些服务可以成为文件或终端操作目标，必须由 Chat OS 程序策略决定，AI 无权设置。
3. 一个项目运行环境应以一个 Docker Compose/环境组作为父级，代码服务和依赖服务作为子级统一启动、停止和展示。
4. 单体和微服务项目使用同一套拓扑模型；微服务项目允许多个代码服务，并能把终端调用路由到明确的代码服务。

图形界面中的目标结构为：

```text
chatos-<project>-<revision>
├── frontend               application，程序注入 Agent，可作为 MCP/终端目标
├── api                    application，程序注入 Agent，可作为 MCP/终端目标
├── worker                 application，程序注入 Agent，可作为 MCP/终端目标
├── postgres               dependency，不允许 MCP
└── redis                  dependency，不允许 MCP
```

Docker Desktop 中的“父级包含多个子容器”来自 Docker Compose 项目，而不是单个 Dockerfile。

## 2. 强制安全边界

### 2.1 AI 允许做什么

AI 只提供项目分析候选结果：

- 候选应用入口和源码子目录；
- 技术栈、运行时、端口和启动命令；
- 候选依赖服务；
- 应用 Dockerfile 建议；
- 环境变量和环境专用配置文件建议。

### 2.2 AI 禁止做什么

AI 输出协议不得接受或持久化下列控制字段：

- `mcp_enabled`；
- `mcp_access`；
- `mcp_image`；
- `mcp_command`；
- `mcp_port`；
- `mcp_token`；
- `agent_version`；
- `agent_install_script`；
- `agent_injection_mode`。

即使 AI 在 JSON、Dockerfile、Compose 或自定义脚本中输出这些配置，Rust 校验层也必须忽略或拒绝。

### 2.3 程序固定策略

| 服务角色 | 文件工具 | 终端工具 | MCP 策略 |
|---|---:|---:|---|
| `application` | 允许 | 允许 | 只能成为程序管理的 Gateway Target |
| `dependency` | 禁止 | 禁止 | `none` |
| `agent` | 按系统配置 | 按系统配置 | 由平台固定创建和升级 |
| `unknown` | 禁止 | 禁止 | 默认拒绝 |

MCP Token 只能发送给系统创建的 Agent；依赖服务不得收到 MCP Token、MCP 端口或 MCP 二进制。

## 3. 现状问题

### 3.1 云端执行仍是单容器

Task Runner 当前从运行环境镜像列表中选择一个 `image_id`，Sandbox Manager 再执行一次 `docker run`。因此：

- 依赖服务不会随任务一起启动；
- 多个 application 只会选择一个；
- 项目没有环境组级生命周期；
- MCP 与单个沙箱容器强绑定。

### 3.2 页面 Dockerfile 与实际构建不一致

项目环境页面保存的是应用 Dockerfile，但镜像生成接口实际上只把 `features` 和 `custom_build_script` 传给固定的 `sandbox_agent/Dockerfile`。应用 Dockerfile目前不是云端镜像的真实构建输入。

必须拆分并明确：

- Application Dockerfile：构建/运行应用服务；
- Sandbox Execution Image：平台维护的开发工具链和 Agent 镜像；
- Dependency Image：MySQL、Redis 等平台标准镜像；
- Environment Deployment：把上述服务组成一个环境组。

### 3.3 本地 Compose 只支持一个应用

现有 Compose 生成器固定查找第一个 application，并固定生成名为 `application` 的服务；Local Connector 请求也只接受一个 `application_dockerfile`。

### 3.4 本地新运行时没有闭环

本地运行时目前只有读取、设置、分析和进度接口，没有环境构建、启动、停止、重启和状态查询接口。本地 Task Runner 的代码与终端工具仍主要直接作用于宿主机工作区。

### 3.5 前端没有拓扑模型

当前前端以平铺镜像表展示，无法表达：

- 父环境和子服务；
- 多个应用服务；
- 服务依赖关系；
- 哪些服务是程序管理的 MCP 目标；
- 当前部署和容器实例状态。

## 4. 目标领域模型

### 4.1 Runtime Environment

```text
project_id
topology_revision
compose_project_name
primary_service_id
deployment_id
deployment_status
services[]
```

### 4.2 Runtime Service Plan

```text
service_id
role                  application | dependency | agent | unknown
display_name
source_subpath
primary
build_context
dockerfile_path
dockerfile
image_id
image_ref
command
ports
env_vars
depends_on
healthcheck
mcp_policy            程序计算，只读
status
error
```

### 4.3 Program Managed MCP Policy

```json
{
  "managed_by": "system",
  "attachment": "project_gateway_target|none",
  "filesystem": true,
  "terminal": true
}
```

该结构不得出现在 AI 更新工具的输入 Schema 中，只能由 Rust 服务计算。

## 5. MCP 执行架构

### 5.1 已实施方案：application 受管 Wrapper

云端不修改 AI 生成的 Application Dockerfile，也不允许 AI 写入任何 Agent 安装内容。Sandbox Manager 按以下两阶段流程运行：

1. 使用保存的 Application Dockerfile 和同步后的项目源码，真实构建应用镜像；
2. 由程序生成受管 Wrapper 镜像，从平台 Agent 镜像复制静态 MCP 二进制；
3. 只有 `application` 子容器获得 Agent、Token、端口和工作区挂载；
4. `dependency` 直接使用平台允许的标准镜像，不获得 Agent、Token、MCP 端口或项目工作区；
5. Agent 不挂载 Docker Socket，跨服务路由由 Sandbox Manager 内部 API 完成。

系统 Agent 二进制使用静态链接构建，可被程序复制到 Alpine、Ubuntu 等不同 Linux 应用基础镜像中。应用进程和 Agent 由程序生成的 launcher 共同托管；任一进程退出时，另一个进程会被终止，避免应用已失败但 Agent 仍被误判健康。

### 5.2 微服务终端路由

终端、进程和日志工具增加服务上下文：

```text
terminal.execute(service_id="api", ...)
process.list(service_id="worker")
process.log(service_id="frontend", ...)
```

未指定 `service_id` 时只能路由到程序确定的 `primary_service_id`。如果没有主服务或存在歧义，必须返回明确错误，不允许 AI 随机选择容器。

### 5.3 依赖服务管理

数据库和中间件只通过环境生命周期管理：

- start/stop/restart；
- healthcheck；
- 端口和日志状态；
- named volume；
- 环境变量。

默认不向通用 AI 暴露数据库 Shell。未来若需要数据库管理能力，应建立独立、受限、可审计的数据库工具，而不是复用终端 MCP。

## 6. 分阶段实施

### P0：程序控制 MCP 策略

状态：已完成。服务角色与 MCP 策略由 Rust 重算；AI 控制字段已从输入结构和工具 Schema 移除；云端与本地 Rust 校验会拒绝在 AI 生成的 Dockerfile/Compose 中安装或配置 Chat OS MCP Agent；Task Runner、云端/本地响应和前端展示已接入程序策略，并有拒绝越权字段与安装内容的测试。

1. 新增强类型的 `RuntimeServiceRole` 和 `ProgramManagedMcpPolicy`。
2. AI 输入结构不包含 MCP 策略字段。
3. Rust 根据经过校验的服务证据计算最终角色和 MCP 策略。
4. application 固定为 `project_gateway_target`；dependency/unknown 固定为 `none`。
5. Task Runner 只允许选择程序标记的 application target，删除字符串排序兜底。
6. 本地和云端 API 均返回相同的只读 MCP 策略。
7. 前端明确展示“系统管理 / MCP 目标 / 无 MCP”。

### P1：多应用 Compose 计划

状态：已完成。Compose 生成器、Local Connector artifacts map、多 Dockerfile 受管目录、程序计算的稳定 `service_id`、显式父子拓扑和多应用测试均已完成。Task Runner 单应用由程序自动选择，多应用必须由用户或程序写入 `execution_service_id`，不会按数组顺序静默选择。

1. Agent 输出从模糊的 `images` 逐步升级为 `services`。
2. 扫描器识别多个源码根目录和构建清单。
3. Compose 生成器遍历所有 application，不再使用第一个 application。
4. Dockerfile 写入：

```text
.chatos/runtime-environment/services/<service_id>/Dockerfile
```

5. Local Connector Compose 请求改为 artifacts map，不再只接受一个 Dockerfile。
6. 验证 service_id 唯一、依赖引用存在、至少一个 application、最多一个 primary。

### P2：本地环境生命周期闭环

状态：已完成当前架构范围。Local Connector 已补齐项目级 Compose 的 `start / status / stop / restart`，Project Management Service 已提供 deployment 查询和整体生命周期 API。deployment 响应按程序计算的 `service_id` 返回父级 Compose 与子服务拓扑，并为每个子服务附带角色、实际状态和只读 MCP 策略；管理界面可展开查看 application/dependency 与“系统管理目标/无 MCP”。本地文件与终端能力继续由 Local Connector 的受管 MCP 执行器承载，dependency Compose 子服务不会获得 MCP 数据。

新增本地 API：

```text
POST /runtime-environment/build
POST /runtime-environment/start
POST /runtime-environment/stop
POST /runtime-environment/restart
GET  /runtime-environment/deployment
```

本地 Task Runner 在 `requires_execution=true` 时连接项目级 Gateway；`requires_execution=false` 不启动依赖服务。

### P3：云端环境组租约

状态：已完成。

Sandbox Manager 新增环境组接口：

```text
POST /api/sandbox-environments/leases
GET  /api/sandbox-environments/{id}
POST /api/sandbox-environments/{id}/stop
POST /api/sandbox-environments/{id}/services/{service_id}/exec
POST /api/sandbox-environments/{id}/mcp
```

Task Runner 使用两阶段租约：先申请父环境和同步源码，再调用 `/start` 构建并启动子服务。Docker 后端使用带标准 Compose project/service 标签的等价多容器实现，因此 Docker Desktop 会按一个父项目聚合 application/dependency 子容器。依赖先启动并通过健康检查，应用再启动；环境支持健康检查、停止、重启、销毁和临时容器/网络/数据卷/镜像清理。未来 Kata/Kubernetes 后端复用同一环境组契约。

### P4：前端拓扑和生命周期操作

状态：已完成。

Chat OS 主界面、Project Management 前端统一展示：

```text
项目环境父节点
  ├─ 应用服务：源码目录、Dockerfile、镜像、MCP目标、状态
  ├─ 依赖服务：平台镜像、端口、健康状态、无MCP
  └─ Agent：版本、状态、权限策略
```

增加查看 Compose、构建、启动、停止、重启和选择终端目标功能。

Project Management 前端已展示父 Compose 状态、application/dependency 子服务、实际容器状态、`service_id` 和只读 MCP 策略，并提供整体启动、停止、重启。Task Runner 前端会读取项目运行环境拓扑；多 application 时强制用户选择 `execution_service_id`，单 application 由程序自动选择。任务详情会显示最终执行服务。AI 工具 Schema 不包含该字段，后端 MCP 入口还会再次硬拒绝 AI 绕过 Schema 提交该字段。

### P5：兼容迁移和灰度

状态：已完成。

1. 保留旧 `images` 响应字段一个兼容周期。
2. 旧 application/runtime 记录迁移为 application。
3. 已知数据库和中间件迁移为 dependency。
4. 单应用项目自动设置 primary。
5. 无法识别的旧记录迁移为 unknown，MCP 默认关闭。
6. 使用 `runtime_topology_v2` 功能开关灰度上线。

### P6：AI 上下文与内部路由隔离

状态：已完成。

1. `RoutingPlan` 只在 Rust 执行链路内部保存文件读取和沙箱 Provider，不再生成面向 AI 或用户的路由摘要。
2. 环境分析提示词不再包含 `routing`、`file_provider`、`sandbox_provider`、`analysis_summary` 或完整的当前环境记录；模型只能看到项目标识、预扫描技术证据以及本轮实际开放的工具定义。
3. Harness 与 Local Connector 的远程文件工具统一以中性的 `code_maintainer_read_*` 命名空间暴露，模型不会通过工具名获知底层文件 Provider。
4. 当前环境读取工具只返回技术分析、变量、配置文件和服务计划的脱敏视图，不返回 Harness 仓库信息、项目根路径、Provider、MCP 策略、镜像 Provider 或用户敏感值。
5. AI 更新工具输入 Schema 不再接受 `analysis_summary`、运行环境 `status`、`last_error`、镜像 `status/error` 以及任何 Provider/MCP/Agent 控制字段。
6. 云端与 Local Connector 的技术分析摘要均由 Rust 根据应用数量、依赖数量、配置文件数量、缺失变量和最终程序状态生成；AI 不能直接提交摘要。
7. 前端将该字段明确标记为“技术分析摘要”；读取旧记录时会自动替换历史内部路由摘要，内部路由状态仍通过独立只读字段展示，不再混入摘要。

### P7：镜像运行时兼容与本地项目联系人修复

状态：已完成。

1. 镜像生成请求中的 `features` 不再直接信任 AI 文本。Project Management Service 会由程序把 Maven、Gradle、Spring、npm、pnpm、Yarn、pip、Poetry、Cargo 等构建工具归一化为 Sandbox Manager 支持的语言运行时，并过滤未知值。
2. 当旧镜像记录只保存了 `maven` 等构建工具时，程序会结合 Dockerfile 和已检测技术栈补全 `java`、`node`、`python` 等运行时，并把归一化结果写回记录；旧失败记录可以直接重试。
3. Sandbox Manager 同时保留构建工具别名兼容层，避免旧调用方再次触发 `unknown sandbox image runtime`。
4. 空 `features` 在没有额外运行时需求时允许使用基础 Agent 镜像，不再被错误拒绝。MCP 是否安装、挂载和开放仍完全由程序计算的服务角色与策略控制，AI 不能决定。
5. 本地项目联系人列表、锁定状态、添加和删除请求显式携带 `local_runtime=true`。后端仅在“桌面客户端来源”和“显式本地路由”同时成立时，允许使用当前登录用户作用域处理仅存在于客户端 SQLite 的项目。
6. 已存在但属于其他用户的云端项目仍返回 Forbidden；普通云端请求不会获得本地项目 fallback，避免绕过项目权限。

### P8：AI 查询复用与父环境批量镜像准备

状态：已完成。

1. Project Management Agent 恢复沙箱镜像目录能力，但分析阶段只开放 `get_image_catalog` 和 `search_images`；`create_image` 不向分析 AI 开放。
2. AI 在分析每个 application 前查询当前真实镜像目录。命中已初始化镜像时，只能把工具返回的准确 `image_id` 写回应用计划；未命中时省略 `image_id`，不得自行构造。
3. Rust 在保存计划时再次查询当前目录，校验 `image_id`、初始化状态和 `image_ref`，并用目录中的权威 features/image_ref 覆盖 AI 文本。dependency 不允许填写应用沙箱 `image_id`。
4. “准备全部镜像”由程序执行：已有 application image_id 仍会实时复验并直接复用；缺失或已失效时按 features 调用程序镜像初始化；显式运行时版本（如 `java8`、`java@8`、`node@22`）会被保留。
5. 同一批次会处理所有 application，支持多代码镜像/微服务；数据库、缓存和配置中心的受信标准镜像会并行检查本地缓存并预拉取，避免首次任务执行才开始下载。
6. 依赖镜像预拉取仅接受平台白名单中的固定 image_ref。AI 仍不能决定 MCP 安装、挂载、权限、Provider、image_ref 或 dependency Agent 策略。
7. Chat OS 客户端由逐行“生成镜像”调整为父级“准备全部镜像”，并统一展示批次中的应用构建和依赖准备状态。

兼容实现：

- 保留旧 `application_dockerfile`，新增多应用 `application_dockerfiles`；
- 旧 lease 缺少 `lease_kind` 时按单容器沙箱处理；
- 旧运行环境记录在读取时由程序重算角色、稳定 `service_id` 和 MCP 策略；
- 单 application 自动成为 primary；多 application 无显式选择时拒绝执行；
- `TASK_RUNNER_RUNTIME_TOPOLOGY_V2=false` 可回退到旧单容器路由，默认开启 v2；
- 原单容器 Sandbox API、健康检查和 release 链路继续保留。

## 7. 验收标准

1. 单应用 + MySQL + Redis 在 Docker Desktop 中显示一个父项目和多个子容器。
2. frontend + api + worker 微服务项目能生成三个 application 子服务。
3. 文件工具只能访问授权项目工作区。
4. 终端工具只能进入 application，且明确记录 service_id。
5. dependency 容器内不存在 MCP Token、MCP 端口和 MCP Agent。
6. AI 即使输出 MCP 字段或相关 Compose 配置也会被程序拒绝。
7. `requires_execution=false` 不启动项目依赖服务。
8. 云端和本地返回相同的拓扑和 MCP 策略结构。
9. 页面展示的应用 Dockerfile与实际应用构建输入一致。
10. 所有服务的启动、停止、失败和健康状态均可审计。

## 8. 推荐提交拆分

1. `runtime-policy-contract`：服务角色、程序管理 MCP 策略、迁移和测试。
2. `runtime-multi-service-plan`：多应用扫描、Compose 和 artifacts。
3. `local-runtime-deployment`：本地生命周期和 Gateway 路由。
4. `cloud-environment-lease`：Sandbox Manager 多容器环境组。
5. `runtime-topology-ui`：统一父子拓扑和操作界面。

## 9. 实施与验证结果

状态：本方案所列 P0-P8 已实施完成。

已验证：

- Sandbox Manager：21 项普通单元测试通过，1 项真实 Docker 环境组 smoke 通过；
- Task Runner：199 项单元测试通过；
- Project Management：68 项通过，9 项依赖 MongoDB 的测试按原设计忽略；
- Local Connector：217 项通过，2 项需要预构建原生 Agent 的测试按原设计忽略；
- MCP Runtime：61 项通过；Plugin Management：42 项通过；
- Chat OS、Project Management、Local Connector、Task Runner 四套前端 type-check 全部通过；
- Chat OS 云端运行环境面板 5/5 测试通过；
- AI 环境上下文不包含内部路由/Provider/历史摘要，远程文件工具使用中性命名空间，AI 更新 Schema 拒绝摘要、状态、错误和 MCP/Agent 控制字段；
- 真实 Docker smoke 已验证父项目聚合标签、application Agent、dependency 无 MCP 环境变量、停止、重启、销毁和资源清理。
- Maven/Gradle 等构建工具归一化回归通过；本地项目联系人请求路由与后端双重保护回归通过；Chat OS 前端联系人 Facade 10/10 测试通过。
- AI 镜像目录只读工具、真实 image_id 回填校验、显式运行时版本保留、多 application 批量准备、dependency 白名单预拉取和客户端父级准备入口回归通过。

真实 smoke 使用本机已有的 MongoDB 基础镜像，实际创建了一个 application 和一个 dependency 子容器。测试完成后，容器、网络、数据卷、临时应用镜像和临时 Agent 测试镜像均已清理。
