# 客户端项目权威与规划能力退役决策

状态：已执行
决策日期：2026-09-10

## 1. 最终产品决策

ChatOS 不再提供 Project Service，也不再提供项目管理插件。macOS 与 Windows 客户端各自维护的项目注册表是项目主体的唯一权威来源。

本决策不保留旧协议、兼容分支、隐藏入口或备用数据源。已经失去消费者的代码、配置、测试、构建任务、发布任务和文档必须直接删除。

## 2. 权威边界

| 数据或能力 | 唯一责任方 |
| --- | --- |
| 项目 ID、名称、本机目录、项目列表 | 原生客户端 `ProjectRegistry` |
| 工作区授权与设备绑定 | 原生客户端 / Local Connector |
| 文件、Git、终端、运行配置 | 原生客户端 / Local Connector |
| 对话与通用后台任务 | ChatOS / Task Runner |
| 插件安装、发布和运行元数据 | Plugin Management |
| 插件读取当前项目身份 | 通用 `host.context.read` |

服务端不能创建、修改、补全或覆盖客户端项目。插件只能消费客户端在启动时注入的项目上下文，不能成为项目权威。

## 3. 明确退役的产品能力

- Project Service 全部源码、路由、配置、镜像、Compose、部署和后台入口。
- 原生 Plan 页面、规划开关、规划模式、专用判断、Header、Agent、Prompt 和 MCP 绑定。
- `plugins/project-management` 的源码、页面、MCP、Skill、存储、测试、构建、发布和官网展示。
- 项目管理插件曾提供的需求、文档、工作项、依赖、规划版本及冻结规划能力。
- 旧 Project Service JSON 清单导入入口、导入协议和导入回执存储；不保留一次性兼容迁移。
- 旧 Local Connector 工作区自动迁移，以及未声明 `runtimeContext` 时隐式注入 `CHATOS_WORKSPACE` 的插件兼容分支。
- 仅为该插件新增的 Task Runner Bridge：
  - `task.batch.prepare`
  - `task.batch.status`
  - `task.workspace.open`
  - macOS / Windows `TaskRunnerHostService`
  - macOS / Windows 插件任务工作区
  - `task_runner_default_model_config_id` 账户设置及客户端设置项

以上能力不迁移到客户端，也不由另一个插件自动接替。未来若重新提出需求管理产品，必须作为新的产品决策重新设计，不能复活本次退役代码。

## 4. Task Runner 与 Bridge 的区别

Task Runner 是平台通用后台任务服务，普通会话和平台任务仍可使用它，因此服务本身继续存在。

Task Runner Bridge 是曾经暴露给项目管理插件 UI 的宿主适配层。它没有其他消费者，随插件一并删除。插件宿主只保留其他插件正在使用的 `host.context.read`；不再代插件创建任务、选择默认执行模型或展示任务工作区。

## 5. 其他插件的项目绑定

其他插件仍按现有通用流程绑定项目：

1. 用户从客户端“应用”入口选择已安装插件。
2. 对声明 `project` / `workspace` scope 的插件，客户端要求用户选择本机项目。
3. 客户端从自己的项目注册表解析项目上下文。
4. 插件运行环境获得客户端注入的项目 ID、项目名和受授权工作区信息。
5. 插件 UI 如需读取身份，仅调用 `host.context.read`。

插件不得通过已退役 Project Service 回查项目，也不得在 payload 中自行声明或覆盖项目身份。

## 6. 验收标准

- 仓库不存在 `plugins/project-management` 和 Project Service 源码目录。
- 构建、测试、部署、镜像、官网和插件清单均无项目管理插件入口。
- 两端客户端均无 Plan UI、规划开关及其条件分支。
- 两端插件宿主只实现 `host.context.read`，无 Task Runner Bridge 或任务工作区。
- 服务端与客户端模型设置均无 `task_runner_default_model_config_id`。
- 其他项目作用域插件仍能从客户端获得项目上下文。
- 项目内对话创建通用后台 Task 时，冻结的客户端 `project_context` 必须贯穿 ChatOS、MCP Management 和 Task Runner；只有 `project_id` 的请求必须失败，不能由服务端补全。
- 全仓生产代码搜索不到上述退役协议或类型。

## 7. 发布顺序

服务端字段和路由采用直接删除，不做双写或兼容。发布时先确认数据库读取允许旧列自然遗留或执行独立 schema 清理，再发布 User Service、ChatOS 及受影响服务，最后发布新客户端。Task Runner 服务无需因本决策下线。
