# 项目唯一工作区执行镜像与平等服务拓扑

## 1. 目标

项目运行环境固定采用“一份完整代码、一个执行入口、多个平等组件和独立依赖”的模型：

```text
项目运行环境
├── workspace       唯一执行镜像，挂载全部项目代码，承载文件、终端和 MCP
├── frontend        application，平等业务组件，只保存运行元数据
├── api             application，平等业务组件，只保存运行元数据
├── worker          application，平等业务组件，只保存运行元数据
├── web-prototype   artifact，文档/静态原型，不参与运行
├── postgresql      dependency，独立标准镜像，无 MCP
└── redis           dependency，独立标准镜像，无 MCP
```

这里不存在“主应用”“伴随应用”或“伴随微服务”。frontend、api、worker 和同一目录下识别出的多个项目都是平等的业务组件。程序唯一固定的是执行工作区，而不是某个业务组件。

## 2. 服务角色

| 角色 | 含义 | 独立沙箱镜像 | 文件/终端/MCP | 是否参与执行 |
|---|---|---:|---:|---:|
| `workspace` | 项目级唯一工作区 | 是，且只能有一个 | 是 | 是 |
| `application` | 可启动、测试的业务组件 | 否 | 否 | 通过 workspace 中的命令运行 |
| `artifact` | 文档、静态原型、demo、示例等非运行产物 | 否 | 否 | 否 |
| `dependency` | PostgreSQL、Redis、MySQL 等基础服务 | 使用受信标准镜像 | 否 | 由环境生命周期管理 |
| `unknown` | 无法安全识别的记录 | 否 | 否 | 默认不运行 |

程序会为每个 application 保存：

- `source_root`：相对项目根目录的源码位置；
- `component_kind`：组件类型；
- `startup_command`：启动命令；
- `test_command`：测试命令；
- `depends_on`：该组件实际依赖的基础服务；
- `auto_start`：环境启动时是否自动启动。

这些字段用于在 workspace 内定位和运行组件，不会把业务组件提升为执行目标。

## 3. 唯一执行目标

运行环境固定返回：

```json
{
  "execution_service_id": "workspace"
}
```

workspace 的 MCP 策略只能由程序生成：

```json
{
  "managed_by": "system",
  "attachment": "workspace_gateway_target",
  "filesystem": true,
  "terminal": true
}
```

application、artifact 和 dependency 的策略固定为 `none`。AI 输入协议不接受 MCP、Agent、Provider、执行服务或镜像控制字段，也不能选择某个业务组件作为执行目标。

## 4. 云端执行链路

### 4.1 环境分析

环境分析可以识别任意数量的业务组件。分析结果必须表达组件之间的平等关系，并区分：

- 可运行组件；
- 仅作为产物展示的目录；
- 外部基础依赖。

当一个目录下有 frontend、backend、worker，或者包含多个独立项目时，程序为每个可运行根目录生成一条 application 元数据。静态 HTML 原型、设计稿、说明文档和不可执行 demo 会标记为 artifact。

### 4.2 工作区镜像生成

程序收集所有 application 的语言、运行时和版本需求，合并为 workspace 的 features，只生成一个项目级工作区镜像。业务 application 不保存 Sandbox `image_id`，也不会各自生成 MCP 镜像。

镜像构建开始时清除旧分析进度。分析已经完成但工作区镜像尚未开始构建时，界面显示“等待生成工作区镜像”，不会继续显示旧的 100% 分析进度条。

### 4.3 Task Runner

Task Runner 创建沙箱环境时只发送：

```text
workspace
程序识别并选中的 dependencies
```

它不会发送 application 或 artifact。若任务尝试指定 frontend、api、worker 等业务组件为执行服务，后端直接拒绝；`workspace` 是唯一允许的执行服务。

### 4.4 Sandbox Manager

Sandbox Manager 将完整项目代码挂载到 workspace 的 `/workspace`，文件工具、终端工具和 MCP 都固定连接该容器。用户或 Agent 可在其中执行各组件的启动、测试和构建命令，例如：

```text
cd /workspace/services/api && npm test
cd /workspace/services/worker && cargo test
```

PostgreSQL、Redis 等依赖作为独立服务启动并通过健康检查。依赖容器不会收到项目代码、MCP Token、MCP 端口或 Agent 二进制。

## 5. Compose 与依赖关系

业务组件之间没有默认主从关系。Compose 依赖按每个 application 的 `depends_on` 生成；旧数据缺少显式依赖时，程序才根据环境变量、Dockerfile、启动命令和测试命令中的技术证据推断。

所有 application 不再默认依赖全部数据库和中间件。PostgreSQL 的 Compose 服务名统一为 `postgresql`，连接地址也使用该名称，例如：

```text
postgresql://user:password@postgresql:5432/app
```

## 6. Local Connector

本地模式合成唯一的 `local-workspace` 执行记录：

- `service_id = workspace`；
- `service_role = workspace`；
- `attachment = workspace_gateway_target`。

Local Connector 本身就是本地项目工作区的执行入口，业务组件仍作为平等元数据返回，且全部为 `mcp_policy = none`。本地 Compose 可以按组件元数据启动应用和依赖，但文件、终端和 MCP 始终作用于唯一的本地 workspace。

SQLite 迁移会持久化 `source_root`、`component_kind`、`startup_command`、`test_command`、`depends_on_json` 和 `auto_start`，旧本地项目读取后可继续使用。

## 7. 前端展示

Chat OS、Project Management 和 Task Runner 的界面统一展示：

- 唯一“项目工作区”执行镜像；
- 平等的业务组件及其源码目录、启动/测试命令；
- 不参与运行的 artifact；
- 独立数据库和中间件依赖；
- 工作区镜像的构建状态和真实执行状态。

界面不再提供业务应用执行目标选择器，也不再使用“主应用”“辅助应用”“伴随微服务”之类的表述。

## 8. 兼容策略

- 旧环境中的 application/runtime 记录会重新分类并迁移到新拓扑；
- 旧的主服务字段仅作为反序列化别名读取，输出统一为 `execution_service_id`；
- 旧的 Gateway attachment 仅作为兼容输入接受，程序重新计算后输出 workspace 策略；
- 已知数据库和中间件迁移为 dependency；
- 文档、静态原型和示例项目迁移为 artifact；
- 无法识别的旧记录默认关闭 MCP 和执行能力。

## 9. 验收标准

1. 每个项目恰好有一个 workspace 执行服务和一个工作区镜像。
2. workspace 可以看到完整项目代码，并能启动、测试任意业务组件。
3. frontend、api、worker 和多个子项目都作为平等 application 展示。
4. application 不持有 Sandbox `image_id`，不成为 MCP 或终端目标。
5. artifact 不生成镜像、不启动、不参与依赖编排。
6. PostgreSQL、Redis 等 dependency 使用独立受信镜像，且容器内没有 MCP 数据。
7. Task Runner 发送给 Sandbox Manager 的拓扑只包含 workspace 和 dependencies。
8. 勾选 PostgreSQL、Redis 等依赖后，环境计划和 Compose 中一定存在对应服务。
9. 分析完成但镜像未构建时不显示误导性的 100% 构建进度。
10. 云端和本地都固定使用 workspace 作为唯一执行目标。
