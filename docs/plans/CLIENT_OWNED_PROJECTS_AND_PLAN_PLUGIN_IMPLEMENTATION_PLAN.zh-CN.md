# 客户端项目权威与 Plan 插件化实施方案

状态：代码迁移主体与通用 Task Runner 宿主 Bridge 已完成，发布验收中。客户端 `ProjectRegistry` 已成为项目唯一权威；两端原生 Plan、聊天规划开关及全部专用判断/协议/Agent 已删除；项目管理插件已覆盖需求、独立版本化文档、工作项、依赖、冻结规划和执行交接；Project Service 源码、消费者、配置与部署项已移除。尚未完成的是旧资料显式导入、Windows 编译/真机、真实账户正式安装验收与服务端远程发板。当前工作树未上线，不保留任何旧链路兼容。最新状态以第 15 节最后一批和整改清单为准。

日期：2026-09-09。

用户验收发现的问题与逐项整改状态，统一跟踪于：[项目管理插件化整改清单](PROJECT_PLUGIN_MIGRATION_REMEDIATION.zh-CN.md)。该清单区分源码实现、测试和真实客户端验收，不以预览代替交付。

范围：macOS / Windows 客户端、ChatOS Backend、Task Runner、MCP Management、Local Connector、Plugin Management、现有插件和部署配置。

## 1. 已确定的目标与边界

2026-09-09 补充硬约束：**不做向后兼容，彻底替换旧链路，不需要的代码直接删除。** 不保留旧项目服务回退、双轨 CRUD、按账户启用开关、旧 API 别名或发布兼容窗口。分批实施只是工作区内的开发顺序，不代表支持半迁移版本上线。数据保留只通过一次性显式文件导入，目录授权、账户隔离及执行确认必须在新链路重新落实。

本方案固化本次讨论最终确定的方向：

2026-09-09 产品交付硬约束：**做的是可持续使用的产品，不是 Demo。** 页面必须沿用现有 Studio 插件的苹果设计语言与交互约定，默认阅读、按需编辑，统一系统字体、工具栏、分栏、层级、状态与表单。不以堆砌技术提示、直接展示 JSON 或仅跑通示例页面作为交付。未保存保护、原子保存、错误与加载恢复、重复提交保护、键盘访问、空状态、真实宿主权限/数据隔离与跨平台验收均是产品工作的一部分。临时测试数据只用于验证，不进入正式启动流程；测试通过、视觉验收、宿主端到端接入、整体验收与可发布必须分别记录，不能互相替代。

1. 完全退役 `project_management_service`，最终构建、运行和部署均不需要该微服务。
2. 客户端本地 ProjectRegistry 是项目实体的唯一权威。项目 ID、名称、路径绑定、归档和删除由客户端管理。
3. Git、文件和本地工作区操作由客户端及其 Local Connector 执行。
4. 项目管理插件只承担需求、技术文档、工作项、依赖、规划数据及自己的应用界面，保存必要的执行引用；它不进入项目 Tab。
5. 客户端聊天输入框的“规划开关”及服务端对应专用分流一并移除。
6. ChatOS / Task Runner 保留通用会话、模型调用、任务依赖、执行、审批和运行记录能力。
7. 所有插件统一消费客户端提供的项目上下文；插件 UI 与任务 MCP 使用一致的身份和数据作用域。
8. 历史项目和插件数据可迁移、可核对；迁移过程中保留原项目 ID。

不采用把项目注册表搬到 ChatOS Backend Mongo 的方案，也不把整个旧服务及其 Mongo、mTLS、Harness、状态同步代码打包进插件。服务端可以保存会话或任务的项目快照、检索索引和关联引用，但它们不能成为项目 CRUD、项目列表或路径变更的权威。

本文第 15 节之前的“当前代码事实”和分批缺口是历史实施记录。最终事实以整改清单及最后一批记录为准；仍标注“拟新增”或“Bridge 尚未实现”的内容是历史状态。新宿主 Bridge 已按通用能力实现，旧链路继续保持删除，不得恢复。

## 2. 改造前审计基线（历史）

### 2.1 项目主体曾依赖微服务

改造前，macOS/Windows Workspace API、ChatOS 项目代理和 reconciliation 都依赖 Project Service。当前这些入口与模型已经删除，两端由本地 `ProjectRegistry` 和 `Native/LocalProjectsService` 管理项目；本节只保留为当时的审计依据。

### 2.2 桌面 Git 已有本地实现

- macOS：[NativeProjectGitService.swift](../../clients/macos/Sources/ChatOSConnector/NativeProjectGitService.swift)，本地执行 status、diff、stage、commit、branch、merge、pull、push。
- Windows：[WindowsProjectGitService.cs](../../clients/windows/src/ChatOS.Connector/Git/WindowsProjectGitService.cs)，客户端 Git 实现。

旧服务中的 Harness 导入、Git 访问、Run Workspace 和分支集成入口及其消费者现已删除；共享的本地 logical path 工具已正名到 `chatos_local_workspace`。

### 2.3 Plan 与规划开关曾是内置功能

改造前，两端项目页、聊天输入框、会话协议、Agent 选择、Requirement 执行编排和消息 metadata 共同构成旧 Plan 链。当前原生页面、按钮、字段、判断、专用 Agent/MCP 与隐藏执行兼容均已删除；保留的异步任务规划是通用 Task Runner 能力。

### 2.4 插件上下文已统一

插件 UI/MCP 的项目身份与隔离目录由客户端注册表和宿主 runtime context resolver 产生；Task Runner/MCP 使用客户端快照，经 Local Connector 授权后冻结 workspace fingerprint 和执行目标。两条链均不再按 project ID 查询 Project Service。

### 2.5 插件平台已有能力与缺口

已有签名安装包、stdio MCP、本地 HTTP 应用、隔离数据目录、Skills 和权限快照。Manifest 有 Agent / Hook 类型，但不能仅凭类型声明认定所有宿主已支持端到端运行。

项目管理插件已使用现有 workbench/local HTTP/stdio MCP 能力完成业务 UI 与数据面。macOS `RestrictedPluginWebView` 与 Windows WebView2 宿主均已实现通用 Task Runner 提交、状态查询和原生任务工作区；Windows 仍待目标环境编译与真机验证。

## 3. 最终数据所有权

| 数据/能力 | 权威来源 | 其他模块如何使用 |
|---|---|---|
| 项目 ID、名称、说明、归档/删除 | 客户端 ProjectRegistry | 服务端仅记录引用或历史快照 |
| 项目与设备、workspace、目录的绑定 | 客户端 ProjectRegistry + 本地授权工作区 | 任务冻结执行目标，本地运行前校验 |
| Git remote、分支、HEAD、工作树状态 | 本地 Git 仓库 | 客户端读取，缓存可失效重建 |
| 需求、文档、工作项、依赖 | 项目管理插件 | UI 与 MCP 共用同一存储 |
| 规划草稿、版本、执行映射 | 项目管理插件 | 引用通用 task / batch / run ID |
| Task / Run 状态、日志、执行依赖 | Task Runner | 插件通过宿主查询，结果可短期缓存 |
| 执行确认、授权、批次释放状态 | 通用任务执行平台 | 插件发起意图，宿主确认并校验 |
| 会话、消息、Memory 内容 | 现有对应服务 | 以用户范围内的 project ID 关联 |
| 插件安装、Release、权限 | 现有插件平台 | 继续固定快照并在执行前校验 |

项目管理插件不管理项目生命周期、Git 凭据、worktree、运行容器或服务端账户。客户端删除项目也不等价于删除仓库、聊天历史、Task 记录或插件数据。

## 4. 客户端 ProjectRegistry

### 4.1 持久化与身份

拟新增用户隔离的本地 SQLite 注册表，分别由 macOS / Windows 原生服务实现相同语义。与插件数据目录分开，宿主不依赖项目管理插件即可加载项目。

建议最小记录：

```text
ProjectRecord
  id                 稳定 UUID；迁移项目沿用旧 ID
  ownerUserId        本地账户命名空间
  name / description
  workspaceId
  relativeRoot       相对于已授权 workspace 的项目目录
  revision           本地单调递增版本
  status             active / archived / removed
  createdAt / updatedAt
```

设备身份从当前配对/执行目标取得；需要保存绑定时与项目 ID 分离。设备重新配对、项目改名、目录移动不会自动产生新项目 ID。原始绝对路径只由本地工作区解析器处理，不作为服务端可信路径。

Git remote、branch、HEAD 直接读本地仓库；不把 `repositoryMode` 的 managed/external 二选一继续作为普通项目创建条件。原 managed 项目的处理见第 11 节。

### 4.2 客户端流程

1. 选择本地文件夹或授权工作区，验证路径，生成稳定项目 ID，原子写入注册表。
2. 侧栏、插件项目选择器、快捷聊天、文件、Git、Run 设置统一读取本地注册表。
3. 创建项目不再要求设备在线、存在默认联系人或会话创建成功；联网后的聊天准备是独立步骤。
4. 联系人和会话继续从服务端获取，在客户端按项目引用合并展示。远端查询失败不阻断本地项目列表。
5. 改名或移动目录更新本地记录和 revision；新任务使用新快照，既有任务保持原执行目标。
6. 删除生成本地 tombstone，防止旧远端快照重新生成项目；仓库和插件文件不随之自动删除。

旧 `reconcile_local_connector_project` 的服务端自动改绑逻辑退役。新设备或新路径的恢复由客户端明确解析和确认，不能根据远端“同名项目”或路径指纹静默改写任务目标。

### 4.3 多设备语义

首期是客户端本地权威，不增加云端 ProjectRegistry 或自动多端双写。另一设备上的同名目录不能自动视为同一项目。

跨设备延续项目通过显式导出/导入或后续专门同步功能完成，保留项目身份并重新验证目录授权。相同账户下保留同一 ID 的项目可以延续会话关联，但插件数据不会因此自动同步。无本地绑定的历史会话仍可阅读，需要执行时再选择有效本地目标。

## 5. 服务端消费项目上下文的方式

### 5.1 冻结任务上下文

版本化 `ProjectContextSnapshot` 由客户端根据本地注册表产生；两端原生 DTO 与 Rust 的 `ClientProjectContextSnapshot` 已采用以下线格式。Task Runner 的创建入口与任务持久化已接入，客户端/会话提交和 MCP 执行传递尚待接通：

```json
{
  "schemaVersion": 1,
  "projectId": "stable-project-id",
  "projectName": "示例项目",
  "projectRevision": 12,
  "executionTarget": {
    "deviceId": "paired-device-id",
    "workspaceId": "authorized-workspace-id",
    "relativeRoot": "apps/example"
  }
}
```

用户身份从认证上下文得到，不由请求中的 owner 字符串决定。服务端校验设备、工作区归属和授权，将规范化快照与任务、插件 Release/工具/权限快照一起固定。实际文件路径由目标客户端解析并校验。

第四批已实现无状态授权入口：ChatOS/Task Runner SDK 使用 owner 绑定的签名调用 MCP Management，后者用自己的 owner 绑定签名请求 Local Connector 控制面核验现有设备/workspace 授权。返回 `ProjectContextAuthorization { snapshot, owner_user_id, workspace_fingerprint }`，只包含客户端原始声明与服务端核验所得身份/绑定信息，不生成项目记录。转换为 MCP 执行上下文时，`revision` 是整个授权结果的 SHA-256 摘要，包含 workspace 指纹；不能只使用客户端的递增 revision，否则同 ID 工作区换绑可能复用旧运行时缓存。

授权结果不是 bearer token，也不代表设备在线或目录存在。第五批已将其存入独立的 `TaskRecord.project_context`，而不是 `input_payload` 或 Memory metadata；任务运行/重试策略解析和 worker 准备阶段重新核验授权及冻结指纹，不刷新已保存的目标。MCP session/工具执行及原生 Relay 的目录存在性、符号链接边界检查仍待接入。允许为已配对但离线的设备生成控制面授权结果，不意味着允许执行时回退到服务器或其他设备。

对本地项目，`projectId` 是关联标识，不是权限凭证。服务端取消查 Project Service 的逻辑后，仍然执行会话所有权、任务所有权、设备和 workspace 授权校验。

### 5.2 调用链改造

```text
客户端 ProjectRegistry
  -> 生成并提交项目上下文
  -> ChatOS / Task Runner 校验并冻结快照
  -> MCP Management 使用快照路由
  -> Local Connector 校验签名、授权、项目绑定与执行目标
  -> 启动已安装插件或执行本地工具
```

具体规则：

- Task Runner 和 MCP Management 不再通过 project ID 调 Project Service。
- Agent 创建子任务、依赖任务、重试任务时继承来源任务的合法上下文；模型不能自行指定其他用户/设备/项目数据作用域。
- 项目缺失、授权撤销、目标离线时返回明确不可用，不降级到服务端执行或另一设备。
- 非项目任务仍允许 device/workspace scope，不强制所有插件具有 project ID。
- 自动重试不读取“项目最新路径”改绑执行。旧路径失效时暂停/失败并要求重新绑定或创建新任务快照。
- 定时任务沿用固定目标；需要改变目标时从客户端当前注册表创建新任务，不允许普通更新重绑定旧任务。

### 5.3 会话、联系人和 Memory

`/projects/{id}/contacts` 等现有项目命名接口要拆出真正的会话/联系人关联能力。删除旧路由和绑定代理，不提供兼容别名；使用通用会话/联系人关联能力，不再调用 `ensure_owned_project` 查询微服务。

关联记录以认证用户 + project ID 为范围，创建/变更上下文时校验绑定，读取具体会话和 Task 时校验资源所有权。Memory 中的项目名称、路径等为来源快照，可保留历史；它们不反向覆盖客户端项目记录，也不被当作执行路径授权。

## 6. 轻量项目管理插件

### 6.1 包结构与实现选择

当前实现沿用 Studio 插件的 TypeScript、本地 HTTP 应用语义和 Apple 设计语言，UI 使用轻量原生 DOM，存储使用 Node 内置 SQLite。无需远端数据库或额外原生 npm 扩展；正式 macOS/Windows 安装仍需发布环境验收。

```text
plugins/project-management/
  chatos.plugin.json
  package.json
  bin/
  src/
    domain.ts        需求、文档、工作项、规划和执行引用协议
    store.ts         SQLite、事务、CAS、幂等和领域规则
    mcp.ts           项目管理工具
    http.ts          受限本地 UI 服务
  ui/
  skills/
  tests/
```

核心组件为 MCP、Skill、项目管理 workbench 页面和本地存储。插件不定义专用 Agent，现有通用 Agent 通过插件 Skill/MCP 操作业务数据。

### 6.2 数据模型

存储位置使用宿主提供的 `CHATOS_PLUGIN_DATA_DIR`。UI 与 MCP 使用同一 schema 和作用域，事务配合 WAL、busy timeout、乐观 revision 和幂等请求防止并发覆盖。

当前表：

- `scope_binding`：只绑定宿主 project/scope identity 和 revision，不复制项目实体。
- `requirements`、`work_items`、`dependencies`：需求层级、工作项和两类业务依赖。
- `documents`、`document_versions`、`document_requirement_links`：独立文档、不可变版本和多对多关联。
- `plans`：保存完整冻结快照，包含纳入范围和精确文档版本。
- `execution_intents`、`execution_references`：单独批准的执行意图与不透明 Task Runner 引用。
- `mutation_receipts`：持久化幂等写入结果。schema 使用 SQLite `user_version=2`；旧版本必须显式导入。

需求业务状态、审核/批准、归档、人工关闭和验收标准仍由插件管理。Task 成功不必然等于需求已通过业务验收。

### 6.3 MCP 与 Skills

已迁移需求/文档/工作项 CRUD、依赖设置、双向闭包、范围图、冻结规划与执行引用规则，并保留非空已发布文档、验收标准、依赖环、跨需求归属和归档引用校验。

业务工具默认操作宿主绑定项目，不接受任意 `project_id` 作为跨作用域访问方式。对 `initialize_project` 等旧工具按职责拆分为“初始化规划资料”，不再创建宿主项目。

工具通过普通插件目录和固定运行快照发布。`builtin_project_management`、System MCP 特殊 provider 及系统默认强制绑定已经移除。普通用户说“先给方案”可以正常输出文本；需要持久化项目业务资料时才使用已安装插件。

### 6.4 项目管理 workbench

覆盖需求浏览、层级/依赖关系、独立版本化文档、工作项、冻结规划和执行交接，并支持在插件内编辑这些数据。任务日志、终端、代码变更和运行审批复用宿主通用执行页面。

页面可离线读写本地业务数据；插件数据读取不以 Task Runner 在线为前提。实时 Task/Run 状态通过宿主 Bridge 按引用查询，不落入插件数据库成为第二权威。

## 7. 通用任务桥接与执行状态

### 7.1 最小宿主接口

拟新增或泛化以下语义能力，最终方法名统一在 SDK 中定义：

| 能力 | 用途 |
|---|---|
| 读取绑定上下文 | 获取当前项目身份及允许披露的执行目标 |
| 提交普通 Agent 任务 | 将插件选择的需求、反馈和 Skill 引用交给通用 Agent |
| 创建待确认任务批次 | 一次提交验证过的任务 DAG，尚不开始执行 |
| 确认/停止/查询批次 | 宿主执行通用任务控制与授权校验 |
| 批量读取 Task 状态 | 按已绑定的任务引用读取状态和运行信息 |
| 打开任务工作区 | 展示宿主任务图、日志、审批和结果 |

接口不能退化为插件任意调用 ChatOS HTTP 路径。宿主按当前 UI/MCP session、用户、项目、component、Release 和授权范围执行请求，插件页面不接收账户长期 token 或内部 mTLS 密钥。UI 请求还需检查来源、session 和用户操作；MCP 调用不能自动取得用户执行确认。

旧 `create_project_execution_tasks` 项目专用入口已经删除。新的通用 DAG/批次宿主 Bridge 已实现；插件业务映射保存为不透明引用，服务端不理解 Requirement/Work Item。

当前宿主能力为 `host.context.read`、`task.batch.prepare`、`task.batch.status`、`task.workspace.open`。项目快照由客户端现场解析，账户级 `task_runner_default_model_config_id` 由宿主现场读取并校验；插件既不能传模型 ID，也拿不到长期 token。批次当前由稳定 tag、内容摘要和 opaque metadata 在 Task Runner 任务集合上幂等恢复，不是服务端原子 batch 实体。

### 7.2 从规划到执行

1. 用户在插件内选择需求或通过对话使用插件，产生规划草稿与版本。
2. 插件/通用 Agent 读取资料、生成工作项和 DAG，校验范围、依赖、技术文档和 revision。
3. 宿主创建待确认批次，固定项目目标、插件快照和计划内容摘要；插件保存返回的映射。
4. 用户查看任务图并确认。平台校验批次版本和授权，再释放执行。
5. Task Runner 执行任务。插件按 Task 引用查询并展示结果。
6. 重新规划生成新版本/批次并标记替代关系；不得无确认释放旧批次或重复提交任务。

创建批次应使用稳定 idempotency key（绑定 plugin、project、plan、revision）。若服务端创建成功而插件保存引用失败，重试可获得原批次并恢复映射。停止/取消、重复确认和部分失败采用通用任务平台语义。

执行控制不能依赖插件 UI 常驻。关闭页面不代表停止任务；禁用插件后的已提交任务仍可在通用任务界面查询和停止。后续需要该插件工具的调用按插件不可用处理。

### 7.3 状态查询替代回写

首期不依赖生命周期 Hook 将 Task 状态写回插件。真实 Task/Run 状态以 Task Runner 为准，Plan 打开、刷新和前台轮询时通过宿主批量查询；已有通用实时事件可以作为刷新提示，不作为唯一事实来源。

状态投影需处理：

- 同一工作项可能关联多个任务，按当前有效映射汇总；被替代、历史失败的任务不阻止新一轮完成。
- 运行、失败、阻塞、取消、成功分别展示；不能把取消直接当完成。
- 查询失败/离线显示未知或带时间戳的缓存状态；不改写本地需求为失败。
- Task 已清理、不可见或映射缺失显示“执行记录不可用”；不得推断为成功。
- 需求的执行进度与审核、验收、归档状态分开，不把所有业务状态压成 Task 状态。

旧微服务 `execution_sync` 的有用聚合规则通过测试迁移至插件状态投影；服务端到项目管理的状态回写接口退役。未来若需要自动业务流转，再独立评估事件消费，不作为本次依赖。

## 8. 客户端插件 UI 扩展与规划开关清理

### 8.1 只使用现有“应用”入口

不新增 `project_tab` surface，不把插件硬编码回项目工作区，也不在项目页重复展示应用列表。项目页只保留目录、消息、设置等内置功能；原生 Plan 删除后没有替代 Tab。

项目管理插件声明普通 `surface: workbench`，component key 为 `project-management-studio`。用户从侧栏“应用”启动，沿用现有应用选择与项目绑定流程；宿主按 component identity 加载，并由客户端注册表注入选中项目上下文。

插件卸载、停用、账户切换、项目删除或改绑时，宿主关闭/撤销对应运行时。重新安装或启用后，只有在同一 owner/plugin/project 隔离身份下才恢复原插件数据。

### 8.2 移除内置规划开关

清理 macOS、Windows、快捷聊天/宠物入口、ViewModel、DTO、发送参数、会话设置接口中的 `planModeEnabled` / `plan_mode_enabled` / `plan_mode` 专用逻辑。

同步清理 `chatos_plan` 的项目管理分流、特殊 MCP bindings、专用 Header、项目规划完整性检查和 Requirement Planner 系统身份。先检查配置和 Prompt 历史，不能只删枚举而留下启动时 seed、反序列化或默认配置失败。

直接删除这些设置字段、读写逻辑和专用执行分支。不支持旧客户端；请求 schema 对已删除字段应按通用严格校验拒绝，不能将旧“仅规划”请求静默当成可能执行代码的普通任务。历史消息只作为历史内容展示，不驱动旧执行模式。

保留通用 Task Runner 的规划阶段、依赖调度和执行前确认。禁止按 `plan` 关键字全局删除；其他插件自己的 generation plan 也不在清理范围内。

## 9. 其他插件统一适配

### 9.1 保留宿主上下文接口

尽量保持现有环境变量语义：

- `CHATOS_PROJECT_ID`：稳定项目身份。
- `CHATOS_PROJECT_NAME`：当前展示名称，不参与持久化身份计算。
- `CHATOS_WORKSPACE_ID`、`CHATOS_WORKSPACE`：本次绑定工作区及本地解析目录，保持既有授权边界。
- `CHATOS_CONTEXT_SCOPE`、`CHATOS_CONTEXT_SCOPE_ID`：宿主确定的数据作用域。
- `CHATOS_PLUGIN_DATA_DIR`、`CHATOS_PLUGIN_CACHE_DIR`：宿主隔离的数据/缓存目录。

应用启动从本地 ProjectRegistry 生成，任务 MCP 从验证后的快照传递，并由本地宿主检查项目映射。两者使用同一上下文规范化与隔离算法。新的执行上下文版本不能改变项目持久化身份；修改执行目标可使旧 session 失效，但不应让数据目录凭空变化。

### 9.2 按插件处理

| 插件 | 已有依赖 | 本次处理 |
|---|---|---|
| Web Design Studio | project scope、注入环境、内部 scope fingerprint | 保持接口，验证旧文档、生成计划及 UI/MCP 访问同一数据 |
| Diagram Studio | project scope、内部文档项目分组和 fingerprint | 保留插件内部分类，校验作用域与迁移 |
| Document | workspace scope、`CHATOS_WORKSPACE`、文件授权 | 保持目录边界及 artifact 行为，不强制改为 project scope |
| Browser / Computer Use | 本地设备、插件 session、授权和任务路由 | 去掉宿主解析时对旧服务的依赖，验证 device-only 使用仍成立 |

Studio 内部 `/api/projects` 管理的是插件自己的文档分组，不能当作旧微服务 API 一并删除。插件无需读客户端注册表数据库；由宿主按声明提供上下文。

### 9.3 保持持久化数据身份

宿主目前根据用户、插件和 `project:<id>` 计算隔离目录，见 `NativePluginRuntimeContext.swift`。迁移必须保留用户命名空间、plugin ID、project ID，并检查已有数据根目录。

Web Design Studio 的 [runtime-scope.ts](../../plugins/web-design-studio/src/runtime-scope.ts) 还把 workspace ID 和数据目录绝对路径纳入内部指纹；Diagram Studio 的 [generation-guides.ts](../../plugins/diagram-studio/src/generation-guides.ts) 也有数据/会话作用域计算。项目 ID 不变并不足以覆盖路径或 workspace 变化。

第一步尽量保持旧环境和存储路径，建立 UI/MCP 双入口一致性测试。需要调整指纹算法时单独版本化：根据可信的用户/项目迁移映射改写持久化归属，旧许可和 session 失效后重新签发，不取消跨作用域校验来绕过迁移。目录移动、设备重新配对和重启都要验证旧内容可见性。

## 10. 代码改造清单

| 区域 | 工作 |
|---|---|
| macOS Core / Connector | 新增本地 ProjectRegistry；统一路径绑定、上下文和插件作用域 |
| macOS App / API | 资源加载改为本地项目 + 远端会话；项目创建与聊天解耦；删除规划开关、原生 Plan 和项目专用执行兼容 |
| Windows Core / Connector / Presentation / Desktop | 同步接口、存储和上下文；删除原生 Plan、规划开关和旧 execution metadata，覆盖重启恢复 |
| ChatOS `models/project.rs`、`api/projects*` | 退役项目 CRUD 代理；拆出会话关联；删除 Plan/Requirement 专用处理与客户端 |
| ChatOS conversation runtime / callbacks | 使用快照；删除专用 planner flags、回写、stale planner 修复等旧分支 |
| Task Runner | 泛化任务批次和执行引用；改上下文解析；删除 Project Service provider/client/回写 |
| MCP Management | 从快照路由；删除 ProjectContextClient 与 ProjectServiceProvider 特殊链路 |
| Local Connector | 保留设备/workspace 认证和 Relay；校验本地注册表绑定，支持新通用宿主接口 |
| Plugin SDK / Management | 复用 workbench UI surface；校验严格 project context；退役系统 MCP / Agent / bindings；发布新插件 |
| `agent`、`mcp`、共享 runtime crates | 清理项目管理 contract、内置目录、Prompt 和专用 profile；保留通用工具能力 |
| `crates/chatos_local_workspace` | 只保留客户端 logical workspace path 工具；旧 `chatos_project_execution` 名称和项目执行领域代码已删除 |
| User Service / 配置 | 退役项目管理专用模型设置及内部调用配置，模型账户和通用模型配置保留 |
| Admin Console | 删除项目管理模块；通用 Task/Run、插件诊断和发布管理继续工作 |
| Docker / scripts / CI / 配置中心 | 删除服务定义、端口、发现、证书关系、环境变量、监控告警和 workspace member |

特别检查 `project_service_sync` 认证模式、内置 MCP Prompt、系统资源 seed、API baselines、默认配置和测试 fixtures。迁移已有记录时要处理序列化的旧系统枚举/资源键，不能只保证新建数据能运行。

## 11. 数据迁移、旧功能退役与回滚

### 11.1 导出和拆分

使用一次性离线导出/导入工具，不保留旧服务导出 API、运行时适配器或同步服务。导出按认证用户和项目范围组织，包含 schema version、来源、记录数量、内容校验和和关联完整性报告。

| 旧数据 | 目标 |
|---|---|
| `projects` | 客户端注册表；保留 ID，重新验证目录绑定 |
| `project_profiles` | 插件业务说明 |
| `requirements`、`requirement_dependencies` | 插件需求/依赖 |
| `requirement_documents` | 插件文档；保留内容、ID 和已有版本信息 |
| `project_work_items`、`project_work_item_dependencies` | 插件工作项/依赖 |
| `project_work_item_task_runner_links` | 插件执行映射，保留历史和当前关联 |
| 消息 metadata 中的执行组、反馈、确认信息 | 与 Task 记录联合导出为 plan/执行引用；不能只导出旧服务数据库 |
| `project_execution_integrations`、`project_branch_promotion_leases` | 完成在途操作后归档审计，不迁入轻量插件 |

客户端先保存稳定 project ID，再让各插件按同一作用域导入。插件导入在事务中完成并记录 import receipt，重复导入幂等；原始数据和未识别字段保留在迁移档案以便核查。

### 11.2 在途任务与切换

每个迁移项目先停止提交新的旧规划写入，等待旧执行批次及 Git 集成结束，或由用户明确处置。不得迁移时自动取消任务。

在停写维护窗口中导出历史数据，先导入客户端项目身份，再导入 Plan 数据和执行映射，核对会话、Task、插件旧数据可见性。不做按账户双轨切换，不保留旧服务只读运行模式，不支持旧客户端继续使用。

完整新链路在无 Project Service 的环境中验收后统一发布客户端/服务端，删除旧 API、调用代码和部署项。历史运行引用从导入数据读取，不转发给旧服务。

### 11.3 Harness / managed 项目

目标架构的日常 Git 由客户端处理，停止新增旧 managed/Harness 自动导入链路。已有仓库不能随微服务删除而遗失：

1. 盘点仍被项目、任务和会话引用的 Harness 仓库及未集成分支。
2. 有本地目录的项目验证本地 Git 和远端信息，保留可用仓库及提交。
3. 仅有远端仓库的项目提供本地克隆/重新绑定迁移路径；迁移前保留历史数据。
4. 未完成的分支集成、冲突和租约单独处置，不将租约记录复制进新插件继续运行。
5. 确认无运行依赖后删除旧服务端 Git/workspace 实现。其他功能仍使用的 Harness 基础设施不因本方案被整套删除。

### 11.4 回滚

切换前对源数据库、客户端注册表和插件目录做可恢复备份，记录最后源版本和目标导入回执。切换后如已有新本地写入，不能直接恢复旧服务为可写权威，否则丢失增量；应暂停相关写入、导出新数据并完成新架构内的前向修复；本方案不实现兼容版本或双权威回滚链路。

退役服务不等于立即销毁数据库和仓库备份。备份清理作为独立操作安排。本方案实施过程中禁止删除用户仓库或覆盖无关客户端数据。

## 12. 分阶段交付

| 阶段 | 交付物 | 通过条件 |
|---|---|---|
| P0：冻结契约与盘点 | ProjectRegistry、快照、桥接、迁移格式；旧调用清单 | 每类旧职责都有迁移/退役去向；新旧行为有明确边界 |
| P1：客户端项目权威 | 两端本地注册表；本地创建/列表/改名/删除；会话关联拆分 | 断网可管理本地项目并使用 Git；重启后身份稳定 |
| P2：通用上下文与现有插件 | Task/MCP 改快照；本地验证；其他插件适配 | 不访问 Project Service 也能完成已有插件 UI 和 MCP 操作 |
| P3：通用 Task Bridge | workbench 受限桥接、幂等待确认批次、状态查询、通用任务打开 | **代码完成**：macOS 已测试；Windows API/Connector 跨目标编译与测试通过，Desktop 待 Windows 原生编译/真机和真实账户端到端验收 |
| P4：项目管理插件 | 领域存储、MCP、Skill、UI、冻结规划和 opaque refs | 需求→文档→工作项→规划→执行意图链通过；**代码完成，待真实宿主验收** |
| P5：迁移与切换 | 显式导入与核验；规划开关和原生 Plan 退役 | 旧模式已删除；显式业务资料导入和跨平台验收待完成 |
| P6：删除微服务 | 源码/配置/部署/seed 清理；停机回归报告 | 代码与部署清单已删除；正式发板、停服和线上回归待完成 |

顺序上先让客户端和任务上下文独立，再切换 Plan。不设置发布兼容窗口；所有新链路齐备后统一验收和发布，不为旧消费者保留代理或分支。

本方案比“完整服务搬入插件”减少了运行和同步职责，但 Task 上下文、两端 UI bridge 与历史数据处理仍是主要工作量。工期在 P0 完成实际调用盘点及宿主能力原型后重新估算，不沿用先前扩大范围的估算。

## 13. 验证与最终验收

### 13.1 客户端与身份

- [ ] macOS / Windows 均可离线创建、列出、改名、归档和恢复项目；会话服务不可用不影响列表。
- [ ] 项目重启、改名和目录移动保持同一 ID；删除后旧远端快照不会复活项目。
- [ ] 重新配对、跨账户、同名项目和路径失效均不会误绑其他项目或访问其他数据。
- [ ] status、diff、commit、branch、pull/push 仍由本地 Git 执行。

### 13.2 插件适配

- [ ] Web Design Studio / Diagram Studio 的 UI 与 MCP 读写同一项目数据，旧内容可见。
- [ ] workspace ID 或数据路径改变时有明确迁移行为，不产生空项目假象或跨作用域读取。
- [ ] Document 的目录授权、导出 Artifact 正常；Browser / Computer Use 的 device-only 使用正常。
- [ ] 未安装、禁用、升级、重启、项目切换及权限撤销均有一致行为。

### 13.3 执行与 Plan

- [ ] 项目快照固定后子任务/重试不丢上下文、不自动改绑。
- [ ] 无用户确认不执行待确认批次；重复提交不创建重复任务。
- [ ] 创建批次成功但本地落库失败可以幂等恢复映射。
- [ ] 多任务关联、重试、替代计划、失败/阻塞/取消和部分完成有正确状态投影。
- [ ] Task 查询失败、记录被清理或离线时不伪造完成状态。
- [ ] 关闭/卸载 Plan 插件后，通用任务界面仍可观察及停止已提交任务。
- [ ] 需求审核、归档、人工验收与执行状态保持独立。

### 13.4 旧链路清理

- [ ] 两端和快捷入口无内置规划开关，历史设置不会触发旧模式。
- [ ] 通用规划阶段及其他插件的 generation plan 未被误删。
- [ ] 停止并移除 Project Service 后，项目、会话、Task、MCP、Memory 关联和插件均正常。
- [ ] 无运行代码通过旧服务获取项目、上下文、规划或任务状态；无内置 ProjectManagement provider/seed 残留。
- [ ] 数据导入数量、ID、内容摘要、依赖边和执行映射核验通过；在途 managed 项目已处置。
- [ ] Cargo workspace、构建/发布、Docker、配置中心、证书调用关系、监控及 API baselines 完成更新。

实施时运行与改动相关的 Rust、Swift、Windows .NET 和插件测试，特别复用依赖图、状态投影、隔离与执行确认测试。文档提交阶段仅验证文档格式和引用；不据此声称上述代码验收已通过。

## 14. 文档关系

本方案作为以下既有设计的后续改造目标：

- [Plugin MCP 平台实现说明](../plugins/plugin-mcp-platform-implementation.zh-CN.md)
- [Task 级插件选择与本地执行](../plugins/task-plugin-selection-and-local-execution-design.zh-CN.md)
- [MCP Local Connector Only 重构方案](../plan/MCP_LOCAL_CONNECTOR_ONLY_ONE_SHOT_REFACTOR_PLAN.zh-CN.md)
- [统一管理后台方案](UNIFIED_ADMIN_CONSOLE_IMPLEMENTATION_PLAN.zh-CN.md)

当实现完成时同步更新上述文档中 Project Service 权威、内置 Project Management MCP、原生 Plan 和后台模块的描述。此次固化不把未来能力改写成当前实现，也不修改现有插件业务设计。

## 15. 实施记录

### 2026-09-09：第一批——本地项目注册表与加载边界（历史记录，后续变更以下一批为准）

本批是可测试的存储/服务层交付，不是项目管理微服务已经剥离完成。没有自动迁移用户数据，没有切换生产项目权威，没有删除旧服务或仓库。

已实现：

1. macOS Core 增加 `ProjectRegistry`、`LocalProjectRecord`、`LocalProjectDraft`、`ProjectContextSnapshot`；Connector 增加 `SQLiteProjectRegistry`。
2. Windows Core 增加对应模型、`IProjectRegistry` 与快照；Connector 增加 `SqliteProjectRegistry`，在已有客户端 SQLite 初始化中创建独立项目表，并注册 DI。
3. 注册表支持本地创建、查询、改名/改绑、归档/恢复和删除 tombstone；以 `(owner_user_id, id)` 为主键，修改使用 revision 检查，保留稳定 ID。操作不依赖联系人、网络、Git remote 或插件。
4. 显式批量导入保留旧 ID；整批记录与回执在同一事务中写入；拒绝重复 ID、跨账户记录和同一 source ID 的不同载荷；已有记录（包括 tombstone）只报告 skipped、不覆盖。导入接口不被远端刷新自动调用。
5. 两端增加 `ClientOwnedWorkspaceLoader`。可先纯本地加载，再查询远端联系人/会话；远端失败返回本地项目和错误信息，取消请求继续抛出取消错误，本地数据库失败不伪装成空列表。远端响应后重新读取本地状态，避免刷新复活已删除项目。
6. 两端 Workspace API 拆出 `fetchWorkspaceRelations` / `FetchWorkspaceRelationsAsync`，只访问 contacts 和 conversations，明确不请求 `/projects`。旧 `fetchWorkspace` 保留原行为供尚未切换的 UI 使用。
7. 快照采用第 5 节的 camelCase 字段，不含 owner 或绝对路径；已生成的快照不会随注册表改名、移动目录或设备变更而变化。当前只是客户端值类型，尚未接到服务端认证/冻结链路。

实现入口：

- [macOS 注册表契约](../../clients/macos/Sources/ChatOSCore/ProjectRegistry.swift)
- [macOS SQLite 实现](../../clients/macos/Sources/ChatOSConnector/SQLiteProjectRegistry.swift)
- [macOS 本地项目加载器](../../clients/macos/Sources/ChatOSCore/ClientOwnedWorkspaceLoader.swift)
- [Windows 注册表契约](../../clients/windows/src/ChatOS.Core/Abstractions/IProjectRegistry.cs)
- [Windows SQLite 实现](../../clients/windows/src/ChatOS.Connector/Persistence/SqliteProjectRegistry.cs)
- [Windows 本地项目加载器](../../clients/windows/src/ChatOS.Core/State/ClientOwnedWorkspaceLoader.cs)

注意事项：

- 注册表只校验规范相对路径语法，不授予目录访问权限。正式接入创建、导入和快照使用时，宿主必须调用已有本地目录解析器验证 workspace、目录存在性和符号链接边界；不能把 `localRootURI` 当作授权结果。
- 客户端原生记录序列化和回执摘要目前仅用于各自实现内的幂等检查，不是已经发布的跨平台迁移文件格式。正式导入工具仍需版本化 envelope、数量/摘要核验、来源归档及路径确认流程。
- 新加载器尚未替换 AppModel / Windows Shell 的默认加载。必须同时处理旧项目导入、创建与聊天准备解耦、服务端项目关联验证，不能只换列表导致新本地 ID 被旧接口拒绝。
- 接入 UI 时按认证账户创建加载器，并在发布本地/远端结果前检查账户和刷新 generation；切换账户必须取消旧加载并丢弃旧结果。加载器不是账户会话管理器，不替代现有 AppModel / Shell 的会话隔离职责。
- 规划开关、原生 Plan、其他插件 UI/MCP 上下文入口、Project Service 的调用和部署仍未切换。现有 Git、插件数据目录及身份算法本批不改动。

验证：

```sh
swift test --package-path clients/macos --filter 'SQLiteProjectRegistryTests|ClientOwnedWorkspaceLoaderTests|ChatOSWorkspaceServiceTests|NativeProjectGitServiceTests|NativePluginRuntimeTests'
```

结果：17 项 XCTest 通过（注册表 8、加载器 4、Workspace API 3、本地 Git 2）；Native Plugin Runtime 测试套件通过，其中依赖真实安装的 Browser CDP 用例按现有条件跳过。覆盖事务中途失败回滚、账户隔离、重启恢复、revision 冲突、冻结快照、历史 ID、重复导入、删除防复活、断网加载及项目作用域回归。

Windows 已补对应注册表/加载器测试和“不请求 projects”的 API 测试。本机未发现可用 .NET SDK，尝试从官方源下载临时 .NET 8 SDK 时多次超时，未完成编译或测试；不能将代码已补齐视为测试已通过。取得 SDK 后运行：

```sh
dotnet test clients/windows/tests/ChatOS.Connector.Tests/ChatOS.Connector.Tests.csproj --filter FullyQualifiedName~SqliteProjectRegistryTests
dotnet test clients/windows/tests/ChatOS.Api.Tests/ChatOS.Api.Tests.csproj --filter FullyQualifiedName~WorkspaceServiceTests
```

文档仍遵循仓库现有 `/docs/` 忽略规则，仅保存在本地；本批未改变该规则、暂存或提交任何文件。

后续紧接的实施顺序：

1. 完成显式旧项目导入与目录确认入口，客户端创建/管理项目接入注册表。
2. 拆开服务端会话/联系人关联与 `ensure_owned_project`，接入认证后的项目快照，替换 Task Runner / MCP 对旧服务的上下文查询。
3. 同批切换侧栏、插件选择器与插件 UI/MCP 双入口，完成 P1/P2 验收。
4. 按 P3–P6 实现轻量 Plan 插件、迁移数据、移除规划开关，最终删除微服务。

### 2026-09-09：第二批——macOS 直接切换本地项目，不留兼容模式

根据用户新增原则，撤销开发中的“按账户确认启用/未迁移账户读取旧服务”设计，未保留激活标记、双轨分支或旧项目服务回退。当前工作区的 macOS 客户端直接使用本地注册表，未部署或操作任何真实项目数据。

已落地：

- AppModel 使用 `ClientOwnedWorkspaceLoader` 先加载本地项目，再加载远端 contacts/conversations；断网不隐藏本地项目，同账户的会话缓存保留，切换账户及过期刷新结果被隔离。
- `NativeLocalProjectsService` 是本地创建/改名/删除和导入确认入口。创建前校验当前账户、已授权 workspace、目录存在性及符号链接边界。SQLite 路径为 native connector state 同级的 `Projects.sqlite3`。
- 创建界面删除默认联系人约束、Git remote 探测/选择、managed/external 选项、Harness 导入和“重试绑定”。本地目录加载成功即可创建，聊天准备是打开消息页后的独立操作。
- 侧栏支持创建、重命名、删除和“导入项目清单”。删除仅写 tombstone，不删除目录、聊天或插件数据。
- 删除 macOS 远端项目 CRUD 方法、`WorkspaceRemoteServicing`、旧创建协议/DTO/托管枚举。替换为 [ChatOSProjectConversationService.swift](../../clients/macos/Sources/ChatOSAPI/ChatOSProjectConversationService.swift)，只查询/创建通用会话，不调用 `/projects/{id}/contacts`。
- 插件 UI 每次启动根据注册表中的当前 ID/名称/目录生成上下文，并验证授权及 revision。删除 `PluginApplicationLaunchRecovery` 及旧回退测试，不再用远端项目列表恢复或把失效项目降级到 device scope。非项目插件的 device 使用方式保留。
- 新增 [NativeLocalProjectsServiceTests.swift](../../clients/macos/Tests/ChatOSConnectorTests/NativeLocalProjectsServiceTests.swift)、[CreateProjectViewModelTests.swift](../../clients/macos/Tests/ChatOSAppTests/CreateProjectViewModelTests.swift)，更新 API 测试为“不请求旧项目 API”的断言。

一次性文件导入格式：

```json
{
  "schemaVersion": 1,
  "ownerUserId": "当前账户 ID",
  "sourceId": "唯一导出批次 ID",
  "projects": [
    {
      "id": "保留的原项目 ID",
      "name": "示例项目",
      "rootPath": "local://connector/当前设备ID/已授权workspaceID/相对目录"
    }
  ]
}
```

导入也接受经用户确认的本机绝对目录。其他设备的 local URI 不会回退到本机同名目录；未知 URI scheme、目录丢失、文件充当目录、跨工作区符号链接和错误账户会被拒绝。预览显示实际目录，确认时重新验证绑定；文件限制 10 MiB / 1,000 条。只导入勾选记录，已有 ID/tombstone 不覆盖；同一 sourceId/选择集重复导入幂等，不同载荷冲突。清单未携带历史时间，因此导入记录时间用 0 表示未知，不伪造为导入时刻。清单不是自动同步文件，不加载旧服务作为数据源。

验证命令：

```sh
swift test --package-path clients/macos --filter 'NativeLocalProjectsServiceTests|CreateProjectViewModelTests|SQLiteProjectRegistryTests|ClientOwnedWorkspaceLoaderTests|ChatOSWorkspaceServiceTests|ChatOSProjectConversationServiceTests|NativeProjectGitServiceTests|NativePluginRuntimeTests'
```

29 项 XCTest 通过（含账户切换时拒绝插件启动的新增用例）；Native Plugin Runtime 套件通过，真实 Browser CDP 安装条件用例依旧跳过。macOS package（含 App/UI）编译通过。没有进行真实账户的数据迁移、安装客户端或部署服务。

Windows 补验完成：使用临时 .NET 8.0.424 SDK，注册表/加载器 21 项、Workspace API 2 项、Plugin Relay 4 项测试通过；这是服务层测试，不代表 Windows UI 已切换。编译过程中修复既有 `PluginRelayHandler.ExecuteAsync` 的局部变量重名错误（`result` 改为 `skillResult`，输出 JSON 的 `result` 字段及行为不变）。

**未完成，不作为兼容保留：** Windows 界面/项目 API 仍待同样替换；Task Runner/MCP 的项目快照认证和传递、消息运行时对 Project Service 的依赖、原生 Plan/规划开关、轻量插件和服务部署清理仍在后续工作中。新本地项目的服务端任务执行尚未端到端验收。本批不能作为完整改造的可发布版本；后续直接删除这些旧链路，不加入新旧判断、旧接口代理或备用 provider。

### 2026-09-09：第三批——Windows Shell 直接切换本地项目

本节更新第二批记录中的 Windows 状态。未增加旧服务回退或按账户启用模式；未执行真实项目数据迁移。

已落地：

- [MainWindowViewModel.cs](../../clients/windows/src/ChatOS.Desktop/AppShell/MainWindowViewModel.cs) 直接使用本地注册表：先发布本地项目，再请求远程 contacts/conversations。断网保留本地项目和同账户会话关联；账户取消令牌、账户代次与刷新代次共同防止迟到结果串号。侧栏批量更新保留当前选择，不因 ListView 清空选择而重新准备聊天。
- [LocalProjectsService.cs](../../clients/windows/src/ChatOS.Connector/Workspaces/LocalProjectsService.cs) 提供本地创建、重命名、移除及项目快照解析。创建校验当前配对账户、已授权 workspace、用户确认的 workspace 根目录、规范相对路径、真实目录存在性和符号链接边界；创建对话框打开后若同 ID 的 workspace 被改绑，拒绝静默改用另一个目录。重命名/移除用 revision CAS，移除只写 tombstone，保留磁盘、Git、会话及插件数据。
- [MainWindow.xaml.cs](../../clients/windows/src/ChatOS.Desktop/MainWindow.xaml.cs) 增加本地创建、重命名、移除入口。创建选择授权工作区并填写已有相对子目录，不要求 Git remote、仓库托管方式或默认联系人。弹窗捕获账户代次，旧账户弹窗不能修改新账户数据。
- 项目默认打开文件页；聊天、Git、Plan、Run 页面按显式选择加载。聊天单独调用通用 conversations API，不再在选中项目时自动绑定联系人或创建会话。原生 Plan/Run 的旧业务接口仍待后续替换，延迟加载不等于已移除它们。
- 删除 `IWorkspaceService`、`IWorkspaceResourceCreationService`、`WorkspaceResourceCreationService`、远程 Project DTO、`LocalProjectCreationDraft` 和 managed/external 枚举，以及对应旧创建测试。不存在同名兼容别名或旧接口代理。
- [WorkspaceService.cs](../../clients/windows/src/ChatOS.Api/Workspace/WorkspaceService.cs) 仅实现 `IWorkspaceRelationsService`；新增 [ProjectConversationService.cs](../../clients/windows/src/ChatOS.Api/Workspace/ProjectConversationService.cs)，只查询/创建通用会话。Windows 代码不再引用 `local-connectors/projects`、`projects/{id}/contacts` 或 `FetchWorkspaceAsync`。
- 本地 `ResolveContextAsync` 读取当前活动记录、校验授权目录、复核 revision 和 Connector 配置，返回与 macOS 相同线格式的快照；这是宿主解析入口，**尚未接入服务端 Task/MCP 的端到端传递，也不代表 Windows 插件的所有入口已切换**。

验证（临时 .NET 8.0.424 SDK，在 macOS ARM64 上执行）：

```sh
dotnet test clients/windows/tests/ChatOS.Api.Tests/ChatOS.Api.Tests.csproj --no-restore
dotnet test clients/windows/tests/ChatOS.Presentation.Tests/ChatOS.Presentation.Tests.csproj --no-restore
dotnet test clients/windows/tests/ChatOS.Connector.Tests/ChatOS.Connector.Tests.csproj --no-restore --filter 'FullyQualifiedName~ClientOwnedShellTests|FullyQualifiedName~LocalProjectsServiceTests|FullyQualifiedName~SqliteProjectRegistryTests|FullyQualifiedName~LocalProjectPathResolverTests|FullyQualifiedName~PetQuickChatViewModelTests|FullyQualifiedName~PluginRelayHandlerTests'
```

结果：API 全套 51 项、Presentation 全套 52 项、Connector 相关用例 53 项通过，共 156 项。其中新增 Shell 9 项、本地宿主服务 13 项，覆盖断网先显本地记录、退出后的迟到结果、账号切换、旧弹窗拒绝、刷新期间改名、会话缓存、选中项目不触发聊天、目录越界/缺失/文件/符号链接、workspace 改绑、删除防复活、revision 冲突。

测试工程链接并编译实际 `MainWindowViewModel`；`MainWindow.xaml` 和 `WorkspaceHostPage.xaml` 通过 XML 语法检查；Windows 范围 `git diff --check` 通过。完整 Desktop `dotnet build --no-restore -p:EnableWindowsTargeting=true` 因缺少 Desktop 的 `obj/project.assets.json` 返回 NETSDK1004，未完成 WinUI 编译或 Windows 真机交互验收，不能将上述服务层测试视为完整桌面验收。

剩余工作与边界：

1. Windows 显式项目清单导入/预览/目录确认 UI 尚未实现，只有底层原子导入协议；历史数据不会从旧服务自动回灌。完整迁移工具、跨设备目录重绑定及 Plan 数据迁移仍待实现和验收。
2. 后续主线仍是认证后的任务项目快照：服务端 conversations/Task Runner/MCP 不再查询 Project Service，子任务/重试继承冻结快照，Local Connector 核验执行目标。客户端生成 DTO 不是服务端授权。
3. 继续落实插件项目页及通用任务桥接，移除原生 Plan、规划开关和专用分流，最终删除微服务及部署/依赖；本批没有把这些未完成项伪装为兼容保留。
4. 当前工作区仍不可作为“Project Service 已退役”的完整发布版本。未修改插件业务数据目录/身份算法，未触碰本批范围外的 MediaStudio 或 Web Design Studio 修改，未提交、部署或删除用户目录/数据库。

文档按仓库现有 `/docs/` 忽略规则保存在本地，未改变 Git 跟踪策略。

### 2026-09-09：第四批——服务端项目快照协议与无状态授权链路

本批完成后续 Task/MCP 切换所需的认证边界，不把请求中的设备/目录声明直接当成授权，不新增服务端项目注册表。

已实现：

- [project_context.rs](../../crates/chatos_mcp_management_sdk/src/project_context.rs)：Rust 版客户端快照、严格字段/路径校验和 `ProjectContextAuthorization`。原始快照拒绝 owner、绝对路径、未知字段、未知版本、无效 revision、路径穿越和非规范相对目录。客户端项目名称、项目 ID、设备、workspace、相对目录、项目 revision、认证 owner 与 workspace 指纹共同决定执行上下文摘要。
- [Local Connector 授权入口](../../local_connector_service/backend/src/api/project_context.rs)：`POST /api/local-connectors/project-context/authorize` 只读取已有设备和工作区授权，核对 owner、设备/workspace 绑定、设备撤销状态、workspace 启用状态及指纹。完全不读写 Project Service、本地服务端项目表、项目绑定表或磁盘目录，也不创建项目。
- [MCP 授权入口](../../mcp_management_service/backend/src/api/project_context.rs)：`POST /api/internal/project-context/authorize` 只接受 ChatOS 或 Task Runner 服务调用；owner 必须与签名 token 中的 owner 一致。仅有 caller/trace 签名但没有 owner 的 token 不能授权项目。
- [MCP 到 Connector 的调用](../../mcp_management_service/backend/src/project_context/authorization.rs) 使用既有 mTLS 客户端和专用 `project-context.authorize` scope，保留 owner 绑定及 trace。Connector 的内部授权路由仅允许 MCP Management，未重新开放 Task Runner 直连 Connector 的执行入口，也不要求新建额外服务密钥。
- [SDK 调用方法](../../crates/chatos_mcp_management_sdk/src/client.rs) `authorize_project_context(owner, snapshot)` 发送 owner 绑定 token，返回结果必须与请求的 owner/快照完全相同。两段调用均限制授权响应为 16 KiB；上游拒绝、响应身份/目标被替换、空指纹或响应超限均失败，不回退到旧服务、服务器执行或另一设备；不把上游原始诊断正文传给调用方。
- 授权结果目前为供后续任务冻结使用的值对象，不是新的可变项目实体，也不是可复用执行凭证。workspace 指纹由 Connector 控制面提供，客户端不能在声明中指定它。

验证：

```sh
cargo test -p chatos_mcp_management_sdk -p local_connector_service_backend -p mcp_management_service_backend --lib --offline -- --quiet
cargo check -p task_runner_service_backend -p chat_app_server_rs --offline
bash scripts/check_api_surface.sh
bash scripts/check_api_path_baseline.sh
```

全套结果：SDK 12 项、Local Connector 91 项、MCP Management 181 项通过，共 **284 项通过，4 项跳过**。跳过项是既有的 1 项 Valkey 和 3 项 MongoDB 集成测试，分别需要 `CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL` 和 `CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL`。新增用例覆盖桌面线格式、owner 注入、版本/revision、路径穿越、设备和工作区越权、撤销/禁用、指纹变化、签名的 caller/scope/owner/path 限制，以及 MCP HTTP 入口到模拟 Connector 上游的授权转发与错误关闭。测试中故意将 Project Service 指向不可达地址，新的授权调用仍可完成。

ChatOS Backend 和 Task Runner 编译检查通过；API 路径/表面基线检查与 `git diff --check` 通过。基线脚本只覆盖现有主 Backend 路由范围，不能代替本批两个新增微服务入口的集成测试。未部署服务或执行真实账户/目录迁移。

**尚未完成，不得误报为已切换：**

1. `TaskRecord` / 会话消息的授权快照持久化、子任务和重试继承、更新不可变字段限制还没有接入。本批没有把可编辑的 `input_payload` 或 Memory metadata 当作可信快照。
2. [旧 Task Runner 上下文解析](../../task_runner_service/backend/src/services/tool_runtime/task_plugin_runtime_context.rs) 和 [旧 MCP ProjectContextClient](../../mcp_management_service/backend/src/project_context/client.rs) 仍是待替换调用点；新授权入口尚未替换实际 runtime-session 创建链路。Task Runner 的服务端项目表回退也仍待删除。此次没有新增任何兼容开关、回退分支或别名。
3. 下一步用本批授权结果完成任务持久化和执行传递，并复核冻结 workspace 指纹；同时替换会话入口和两端客户端提交。完成生产者/消费者切换后直接删除上述旧查询，不支持半迁移版本发布。
4. 原生 Plan/规划开关、轻量插件、Windows 显式清单导入以及微服务/部署退役仍未完成。本批只修改共享 SDK、Local Connector/MCP 的授权链路及文档，未改动客户端、其他插件或用户项目数据。

### 2026-09-09：第五批——Task Runner 冻结授权快照与删除旧项目查询

本批完成：

- `CreateTaskRequest.project_context` 接收客户端原始快照；`project_id` 与快照必须同时提供且精确匹配。owner 只取认证用户。服务端通过第四批 MCP 授权入口核验后，保存独立的 `TaskRecord.project_context: ProjectContextAuthorization`。Mongo 的完整记录序列化会保存此字段，不另建项目集合或绑定表。
- 任务创建验证授权响应的 owner、完整原始快照和非空 workspace 指纹；拒绝响应替换、仅传 ID、无 owner、无配对快照、source/request 上下文冲突以及将授权 envelope 冒充原始快照。失败时不保存任务，也不创建 Memory thread。请求拒绝未知字段，不能自行提交服务器管理的授权证明字段。
- 删除 `ensure_project_available_for_task`、`get_project_from_project_service` 及其无用校验辅助函数。任务创建不再访问旧服务或本地 TaskProject 表；即使这些配置/数据还存在也不参与新任务授权。
- `UpdateTaskRequest` 直接删除 `project_id` 字段，并拒绝未知字段；同样不能编辑 project context、owner 或 tenant。普通 `input_payload` 编辑不改变冻结快照。已有项目任务不会再通过 tenant 自动对齐悄悄修复不一致身份；身份不一致直接拒绝。
- Task Runner 插件策略改为消费传入的冻结授权结果，删除 `resolve_project_execution_context` 和专用旧 scope 常量，删除“项目服务未配置 → server 执行”和 project-bound `WorkspaceProviderKind::None` 路径。非项目任务仍可走既有用户作用域，不能因此把缺少快照的项目任务降级为非项目任务。
- RunService 在运行/重试策略解析及 worker 执行准备链路重新调用 MCP 授权入口，精确比较冻结授权结果；同 workspace ID 换绑、授权撤销、owner/tenant/project 不一致均失败，不把新指纹覆盖回旧任务。自动重试沿用原任务；任务图复制保留原授权结果，并验证冻结字段合法。新增测试覆盖复制保存与重授权指纹变化拒绝。
- 可信 `McpRequestContext`/`TaskSourceContext` 的内存结构已能携带原始快照；创建工具会覆盖模型输入，单任务和依赖图创建使用同一 source 上下文。插件选择与最终创建之间若冻结 revision 变化则拒绝。给 Agent 的任务结果移除完整 `project_context`，模型不需要接触内部授权 envelope。
- 现有 `GET /api/tasks/capabilities/catalog` 增加 URL 编码 JSON 的 `project_context` 参数；在解析项目范围的插件目录前先授权，不凭 ID 查项目。该参数接受原始声明，不接受授权 envelope。**上游 ChatOS 调用方尚待传递此参数。**

验证：

```sh
cargo test -p task_runner_service_backend --lib --offline -- --quiet
cargo check -p task_runner_service_backend -p chat_app_server_rs --offline
bash scripts/check_api_surface.sh
bash scripts/check_api_path_baseline.sh
git diff --check
```

Task Runner **390 项测试全部通过，0 跳过**。覆盖原始快照与 JSON/BSON 往返、无服务端项目记录的创建/依赖作用域、授权失败不落库、响应 owner/target 替换、编辑不可重绑定、payload 伪造不影响授权、运行时冻结指纹复核、任务图复制、可信工具上下文覆盖和 Agent 输出隔离。测试使用显式注入的授权服务替身；生产实现只有 MCP 授权路径，没有运行时兼容开关或测试授权回退。BSON 测试是序列化验证，不是实际 MongoDB 联机读写验收。

**剩余断点，必须接通后才能发布：**

1. MCP `CreateRuntimeSessionRequest`/不可变 session snapshot 尚未携带任务授权 envelope，MCP `ProjectContextClient` 仍在查询旧服务；Task Runner 的工作区准备/分支/集成链路仍有旧 Project/Harness 调用。这一批只完成 Task Runner 的创建和策略边界，不能宣称项目实际执行已经端到端脱离旧微服务。
2. ChatOS 会话与客户端提交、MCP 的签名 Task Runner binding 尚未传入原始快照和冻结指纹。现有 ID-only headers/binding **不重建快照、不查旧服务补全**；项目任务创建/动态工具目录会明确失败。`McpRequestContext` 的内存继承已验证，但跨服务子任务继承与指纹一致性仍未验收。下一批应成对修改 MCP session 的生产者、存储和 Task Runner provider 的消费者，不能从模型参数或可编辑 metadata 获取授权。
3. 原生 Relay 尚待检查冻结 fingerprint、当前账户注册表和实际目录/符号链接边界；完整撤销/改绑后工具调用的端到端拒绝测试仍未完成。
4. 原生 Plan 与规划开关、轻量插件、其他插件项目对接、Windows 显式导入、旧微服务及部署配置退役仍待完成。旧 Plan 测试仅更新为显式快照测试夹具，不代表要保留旧 Plan 产品路径。

未部署、未迁移真实数据库、未删除用户仓库或项目文件；本批未改动客户端及其他插件的既有工作区修改。上文第四批“尚未完成”列表为该批历史状态，以此处当前断点为准。

### 2026-09-09：第六批——真正落地插件业务与统一产品界面

针对“需求与项目工作项应属于插件”的纠偏，本批直接实现 `plugins/project-management` 的业务域，不再把执行授权链路进展描述成业务剥离完成。

已实现：

- 普通 ChatOS v3 插件，贡献 `project-management-mcp` 和 `project-management-plan` workbench 页面；严格绑定客户端项目上下文和宿主隔离目录。权限只有 `process.spawn`，页面只声明 `host.context.read`；未伪造尚未支持的项目 Tab / 执行 bridge capability。
- 本地 SQLite 业务表保存需求、父子关系、当前技术文档、工作项、业务依赖和冻结规划。scope binding 只校验当前 projectId/scopeId，不存项目名称/目录实体，不提供项目 CRUD、Git、运行状态或旧服务访问。
- UI 与 stdio MCP 共用 `PlanningStore`：事务、scope revision CAS、持久化幂等回执、依赖与层级循环检查、文档/验收标准门槛、归档引用约束。正文和依赖支持一次原子更新，失败不改变内容、依赖或 revision。
- 规划只选择就绪工作项及已批准需求，要求文档和完整依赖范围，保存不可变内容快照。批准是业务动作，不触发执行、不伪造 Task/Run 状态。
- UI 沿用 Web Design Studio / Diagram Studio 的系统字体、系统蓝、灰色侧栏、细分隔线与紧凑工具栏。导航/资料列表/详情三栏，默认阅读、显式编辑；需求可进入相关文档和工作项。规划按需求、技术方案和工作项阅读，不输出 JSON 表单。
- 有效列表搜索；空状态与新建门槛；未保存导航/关闭保护；取消编辑；Cmd/Ctrl+S；原生 dialog 语义的确认弹窗；保存中锁定；不确定响应复用原 requestId 重试；确认写入后刷新失败不重新创建。业务错误中文提示，已批准需求的正文和文档只读，需要先退回草稿。
- loopback HTTP 精确 Host/Origin、会话 token、静态资源白名单、CSP、请求大小/超时限制；不暴露 SQLite 路径或下载入口。数据库损坏、未知 schema、误绑定和目录/数据库符号链接均拒绝，不创建空替代库。
- 根 Makefile 和 `plugins/README.md` 加入插件构建/测试入口。新增 SDK manifest 验证测试；未修改其他插件的视觉与业务代码。

实现选择：后端 TypeScript，当前 UI 使用原生 DOM，而不是设计稿中的 React。Node.js 最低 22.13，使用内置 `node:sqlite`，不引入远端数据库或原生 npm 扩展；构建采用共享 ESM chunks，避免独立 bundle 的错误类身份不一致。预览脚本只创建临时测试数据库，不属于正式插件启动流程；退出时仅清理自己创建的临时目录。

验证：

```sh
cd plugins/project-management
npm test
npm pack --dry-run --json
# 仓库根目录
cargo test -p chatos_plugin_management_sdk --lib --offline project_management_plugin_declares_only_bound_local_business_components -- --quiet
git diff --check
```

插件 **14 项测试通过，0 跳过**，包含真实 HTTP 服务与子进程 stdio MCP 交替读写、CAS/幂等、原子正文/依赖修改与回滚、冻结版本、作用域绑定、归档及损坏/符号链接拒绝。TypeScript 与 UI JavaScript 语法检查通过。打包 dry-run 包含 launcher、共享运行时、UI、manifest 和 Skill，不包含数据库或 node_modules。新 manifest 测试单独通过。

本批也运行过完整 SDK 测试：47 项通过、1 项失败，失败为既有 `preserves_published_manifest_hashes` 的 Computer Use manifest hash 断言（实际 `444eb31564cc2aa0852c055874bc2f68edbbc15f7ae2685300437d03c4a205`，期望 `0238257797138f3c7bceb8ac697d087ab4ca6d7467216a57f8c211ff15f00d2a`）。本批没有修改该插件 manifest 或 hash 算法，没有为让测试变绿重写发布基线。

浏览器使用同一正式 UI/HTTP 实现在隔离临时数据库上手工验收：新建需求；有草稿时切换模块弹窗及取消后内容保留；正文和依赖同一次保存且 revision 只增 1；关联导航至技术文档并保存后保持原需求；规划批准确认及冻结版本阅读；搜索无结果仍保留当前详情。浅色桌面截图已检查。此记录不是自动化浏览器回归套件，也不等于真实 macOS/Windows 宿主安装验收。

**仍需完成，不能把当前版本作为完整产品发布：**

1. 原生 Plan Tab 和规划开关仍待替换/删除，通用插件贡献入口与任务桥接未接通。旧业务专用 Agent/MCP、Project Service 微服务及部署尚未退役；本批没有为绕过断点而创建兼容调用。
2. 业务字段覆盖、不同文档类型/独立历史、显式导入还不完整；当前每需求一个技术文档，文本/Markdown 源文阅读编辑，没有富文本渲染。执行引用和通用状态查询未接入。下一步应继续完成插件业务迁移与正式入口替换，不转而扩张项目微服务。
3. Node 运行时在目标客户端上的供应、Windows 插件启动、真实宿主账户/项目隔离需端到端验收。深色、窄窗口 CSS 已实现，尚未完成各断点和 Windows WebView/辅助技术视觉验收；未提供冲突差异合并或崩溃后草稿恢复。
4. 第五批所列 MCP/会话/客户端快照传递、旧 Workspace/Harness 执行链与其他插件对接仍有断点，不因本插件的独立存储测试通过就判定已经切换。

未安装到真实客户端、未发布商店/签名包、未提交或部署、未迁移用户资料。方案继续按仓库既有 `/docs/` 忽略规则保存本地。

### 2026-09-09：第七批——规划开关整链路删除与整改清单

本批优先处理用户实机发现的“规划按钮仍在”和“客户端仍访问旧 Plan”，详情及逐项状态见 [整改清单](PROJECT_PLUGIN_MIGRATION_REMEDIATION.zh-CN.md)。

- 删除 macOS/Windows 聊天规划按钮、状态、允许条件、设置动作、发送字段和服务端持久化/分流，不保留固定 false 的旧实现；独立推理与通用执行确认保留。
- 删除 Task Runner 的 `chatos_plan` 接受和路由、固定工具集、强制只读覆盖，删除规划 Agent 身份/队列/seed/提示词/管理端展示。拒绝旧配置，不把它兼容映射成普通任务。
- macOS 原生 Plan 页面与专用 API 删除。曾擅自增加的“项目插件”重复入口已被用户明确否定并删除；不设置任何替代 Tab，只沿用侧栏已有的“应用”、本地注册表解析上下文和宿主绑定流程。Windows 原生 Plan 删除仍未完成。
- 以真实新插件 manifest 验证宿主 UI/MCP 同作用域、项目改名稳定、账户/项目隔离和缺上下文拒绝。审计现有 Studio 插件，作品容器不等同于 ChatOS 项目主体。
- 运行的是新编译产物的测试，不是已经替换用户的 `/Applications/ChatOS.app`。独立网页仍是插件预览，不作为客户端交付证明。

第六批及之前的“待完成”列表保留为历史记录；当前分项状态、测试与剩余断点以整改清单为准。R03–R09 的插件完整业务覆盖、真实宿主、执行上下文闭环和旧微服务整体退役尚未完成，禁止作为完成迁移的版本发布。

### 2026-09-09：第八批——插件业务补齐、隐藏执行链删除与 Project Service 最终退役

本批覆盖并更新第六、七批中的历史缺口。代码侧已经完成的内容如下：

- 项目管理插件补齐需求树、祖先/子孙路径、前置/后续闭包、工作项关联和关系范围；补齐独立多类型文档、不可变版本、多对多需求关联、历史阅读及安全 Markdown/SVG 展示。
- 冻结规划自动纳入完整前置依赖并固定精确文档版本。执行意图需要在规划批准后另行准备、另行批准；插件只保存 opaque batch/task/run 引用，不保存 Task Runner 状态。
- 插件总名称统一为“项目管理”/`Project Management`，component key 为 `project-management-studio`；`surface` 保持现有 workbench。插件只从侧栏“应用”进入，不新增或恢复任何项目 Tab。
- macOS/Windows 原生 Plan 和规划开关整套删除。macOS 消息任务工作区中残留的旧 requirement execution DTO、API、确认/停止、execution group 覆盖、轮询横幅、失败空状态和宠物 activity 同步删除；Windows 的对应 metadata 与 activity 映射同步删除。
- ChatOS 删除 `project_requirement_execution` 历史替换、内部 prompt 识别、confirmation 专用同步和兼容测试。Memory Engine 的旧需求执行 repair 脚本删除。
- Project Service 微服务目录、Cargo workspace member、ChatOS/Task Runner/MCP Management/User Service 消费者、System MCP/provider、旧 Agent/Prompt/binding、管理后台、配置、mTLS、CI、Compose、Makefile、官网状态和远程部署列表全部删除。
- Task Runner 删除无生产调用的旧项目执行 promotion/finalize/pause/integration 编排。共享 logical path 工具从 `chatos_project_execution` 正名为 `chatos_local_workspace`；ChatOS 的项目 scope 校验文件正名为 `session_project_scope`。
- Plugin Management 的 `chatos_plan` 普通测试样本改为通用 `analysis` profile。保留的 retired Agent key 只用于 seed 删除线上旧记录，保留的 strict reject 测试只证明不兼容。

当前验证：

```sh
cargo fmt --all
cargo check -p chat_app_server_rs -p task_runner_service_backend \
  -p mcp_management_service_backend -p plugin_management_service_backend \
  -p official_website_service_backend
cargo test -p chat_app_server_rs -p task_runner_service_backend \
  -p mcp_management_service_backend --no-run
cargo test -p chat_app_server_rs -p task_runner_service_backend \
  -p mcp_management_service_backend -p plugin_management_service_backend --lib

npm run pack:verify --prefix plugins/project-management

swift build --package-path clients/macos --target ChatOSCore
swift build --package-path clients/macos --target ChatOSAPI
swift build --package-path clients/macos --target ChatOSApp
swift test --package-path clients/macos --filter \
  'ConversationHistoryMapperTests|ConversationHistoryStoreTests|PetActivityRecoveryMapperTests|PetStateReducerTests|ConversationTurnMessageTaskLookupTests|MessageTaskWorkspaceViewModelTests|ProjectManagementPluginContextTests'
```

结果：Rust 生产与测试目标编译通过；ChatOS 424、Task Runner 344、Plugin Management 130 项单测通过，MCP Management 176 项通过/3 项既有集成测试忽略；项目管理插件 17/17 测试与 pack 验证通过；macOS 三目标构建通过，通用 Task Runner Host Service 测试 2/2 通过，相关 Swift 测试通过；部署脚本语法与合并 Compose 配置验证通过。Windows 使用临时 .NET 8.0.424 SDK 完成跨目标编译与五个测试工程：Core 19/19、API 48/48、Presentation 47/47、Connector 292/292、NetworkGuard 19/19，共 425/425 通过，覆盖应用 manifest/runtime、客户端项目 registry、会话、设置和 Task Runner Host Service。Desktop 已完成依赖项目编译，新增插件页 code-behind 也通过隔离 C# 编译；但 WinUI XAML 编译器依赖 Windows `kernel32.dll`，无法在 macOS 执行，因此 Windows Desktop 原生编译和真机验收仍未完成。

代码完成后仍有四项不能伪装为已上线：

1. 需要发布受影响的服务端组件和新插件，然后停止线上 Project Service；当前未执行远端部署。
2. 需要用正式客户端安装/升级链验证侧栏启动、项目绑定、账户/项目切换、停用、卸载和恢复；fixture 预览不是这一验收。
3. 通用 Task Runner 宿主 Bridge 尚待真实账户和正式安装包端到端验收；Windows 尚待编译/真机。不得把静态实现写成全平台已上线，也不得恢复旧项目专用执行 API。
4. 旧资料显式导入与 Windows 真机仍未完成。不做自动 migration、旧服务回退或双写。

最终逐项状态和发布门槛统一以 [整改清单](PROJECT_PLUGIN_MIGRATION_REMEDIATION.zh-CN.md) 为准。

### 2026-09-09：第九批——通用默认模型与两端插件执行宿主

本批完成旧项目执行链删除后的通用执行入口，不恢复项目管理专用 Agent、模型字段或服务端项目权威：

- User Service 新增账户级 `task_runner_default_model_config_id`，ChatOS 完整透传。保存时校验模型归属、启用状态、Task 能力与云端凭据；删除模型时同步清除引用。macOS/Windows 设置页均将它显示为通用 `Task Runner` 默认模型，本机审批模型继续单独保存在设备侧。
- 项目管理插件不能在 Bridge payload 中提供或覆盖模型 ID。宿主在每次创建 Task DAG 前重新读取账户默认模型和模型目录，并把验证后的模型固定到每个 Task；批次内容摘要也包含模型 ID，模型变化不会静默复用旧任务。
- macOS 受限 WebView 宿主实现 `host.context.read`、`task.batch.prepare`、`task.batch.status`、`task.workspace.open`，并通过 Host Service 专项测试。创建任务只准备 DAG，原生任务工作区重新读取 Task Runner 状态并要求用户二次确认后才启动 Run。
- Windows 新增独立侧栏“应用”入口、Apple/Studio 风格应用目录和项目选择，不进入项目 Tab。WebView2 宿主限制为当前 loopback origin 和主页面 Bridge 会话，校验协议、adapter session、nonce、request ID、256 KiB 上限及 capability allowlist；账户切换、停用或卸载会关闭对应本地运行时。
- Windows 同步实现 Task Runner Host Service、稳定批次 identity、部分成功恢复、依赖图冲突检测、真实状态查询和原生任务工作区。临时 .NET 8.0.424 SDK 已完成五个测试工程共 425/425 测试及其生产程序集跨目标编译；新增插件页 code-behind 也通过隔离 C# 编译。Desktop 在 macOS 被 WinUI XAML 编译器的 Windows `kernel32.dll` 依赖阻断，不是已观察到的 C# 业务编译错误。仍必须在 Windows 上完成 Desktop 原生编译，并做 WebView2、键盘、深浅色和辅助技术验收后才能标记为平台完成。
- 当前幂等“批次”是 Task Runner 中带稳定 tag、内容摘要及 opaque metadata 的 Task 集合，不是服务端原子 batch 实体。关闭插件不停止已启动任务，插件数据库也不保存运行状态副本。
- 正式线上启用前必须发布 User Service、ChatOS、其余受影响服务、项目管理插件和客户端，再停止 Project Service。旧资料显式导入、真实账户正式安装验收和 Windows 真机仍是明确未完成项。

### 2026-09-10：第十批——Connector 重新授权后的项目绑定恢复

真实客户端验收发现：项目目录仍在且当前账户拥有本机 `/` 根授权，但 Connector 重新配对/授权生成新的 workspace ID 后，本地 `ProjectRegistry` 仍保存旧 ID，项目设置页因而显示“没有找到可用的本机工作区”。修复落在客户端与 Local Connector，不恢复 Project Service，也不把插件变成项目主体权威。

- `NativeLocalConnectorService` 在 device 匹配、旧 workspace ID 已失效时，允许通过当前有效的 `/` 根授权重新解析原 `relativeRoot`。候选必须是经过 `NativeWorkspaceFilesystem` 边界/符号链接校验的现存目录；多个 grant 只有解析到同一物理目录才可确定性选取，存在不同物理结果则失败关闭。
- `NativeLocalProjectsService` 在项目列表刷新、插件上下文或任务项目上下文读取前检查绑定。恢复成功后以 revision CAS 更新客户端注册表的 workspace ID，保持项目 ID、名称和目录不变；并发修改导致冲突时不覆盖新记录。
- 非 `/` 的项目专用 workspace 变更不能从相同相对路径猜测，仍要求用户显式重新选择授权。跨 device、跨账户、目录缺失、文件目标和符号链接越界均不恢复。
- macOS 启动刷新在读取本地 workspace snapshot 前执行修复，因此侧栏 URI、项目设置、插件 context 和任务 snapshot 使用同一份更新后的权威记录。

验证结果：`swift test --package-path clients/macos --filter NativeLocalProjectsServiceTests` 11/11 通过，覆盖安全恢复、revision 更新和项目专用 workspace 拒绝。`scripts/deploy-online.sh client mac` 完成本地化审计（英文缺失 0、中文 identity 缺失 0）、构建和签名；新包已替换安装并启动。真实 `aide` 项目的 revision 从 1 更新为 2，workspace ID 指向当前有效 `/` grant，项目设置页正常显示目录、运行目标与“没有发现阻塞问题”。本批没有执行任何远程部署。
