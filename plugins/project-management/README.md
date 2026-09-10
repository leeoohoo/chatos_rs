# Project Management · 项目管理插件

这是 ChatOS 的项目级业务插件，负责当前宿主项目的需求、规划文档、工作项、依赖、冻结规划和执行交接。

项目实体不属于插件：项目 ID、名称、目录和生命周期以客户端 `ProjectRegistry` 为唯一权威，Git 与文件操作由客户端和 Local Connector 负责。Task/Run 的执行状态以 Task Runner 为唯一权威。插件不提供项目 CRUD、Git、远端数据库、执行状态副本，也不连接或回退到已退役的 Project Service。

## 已实现

- 需求树、父子路径、上下游依赖闭包、关联工作项与关联文档。
- 项目工作项及其独立依赖 DAG，包含归属、循环、归档引用和就绪门禁。
- 独立文档实体，支持技术、设计、API、决策和备注类型，Markdown/SVG 格式，不可变版本历史及需求多对多关联。
- 需求批准和工作项就绪前检查验收标准与已发布规划文档。
- 冻结规划自动纳入完整前置依赖，固定精确需求、工作项和文档版本；后续编辑不会改写历史规划。
- 执行意图与规划批准分离，执行交接只保存不透明的 Task Runner batch/task/run 引用，不保存或伪造运行状态。
- UI 与 stdio MCP 共用同一项目隔离 SQLite；WAL、事务、revision CAS 和持久化幂等回执共同防止重复写入与丢失更新。
- `planning_read`、`planning_get`、`planning_scope`、`planning_change` 提供与页面一致的业务能力。工具不接受项目、账户、目录或执行状态注入。
- 严格宿主绑定：必须提供 `CHATOS_CONTEXT_SCOPE=project`、`CHATOS_CONTEXT_SCOPE_ID`、`CHATOS_PROJECT_ID` 和项目隔离的 `CHATOS_PLUGIN_DATA_DIR`。缺失上下文直接拒绝，不回退到 cwd 或 device 公共存储。
- 产品级三栏界面：系统字体、系统蓝、细分隔线、深浅色、阅读/编辑分离、搜索、空状态、未保存离开保护、取消编辑、Cmd/Ctrl+S、保存状态和中文业务错误。
- loopback UI 使用精确 Host/Origin 校验、进程会话密钥、静态资源白名单、CSP、请求大小和超时限制；数据库路径不通过 API 暴露。
- 通用宿主 Bridge 已实现 `host.context.read`、`task.batch.prepare`、`task.batch.status`、`task.workspace.open`。宿主重新读取客户端项目快照和账户级 Task Runner 默认模型，插件不能传入或覆盖模型 ID，也拿不到账户长期凭据。
- “创建批次”当前以稳定 tag、内容摘要和 opaque metadata 在 Task Runner 中幂等恢复一组 Task，并非 Task Runner 服务端原子 batch 实体；打开原生任务工作区后二次确认才启动 Run。

macOS 与 Windows 客户端原生 Plan 页面、聊天“规划”开关、对应协议判断、专用 Agent/MCP 和需求执行兼容链已经删除。插件只从客户端侧栏“应用”的既有入口启动，不占用项目 Tab，也不增加第二个应用入口。

## 数据与安全边界

项目名称只用于当前页面展示，不写入数据库成为第二份项目实体。数据库 scope binding 仅用于拒绝账户或项目误接。项目改名不会生成新资料库，跨账户或跨项目打开同一数据目录会失败关闭。

当前 schema 版本为 v2。旧 v1 数据库、未知 schema、损坏数据库、符号链接数据目录或数据库文件均明确拒绝；本插件不做隐式兼容迁移。历史资料如需保留，必须走单独的显式导入流程并由用户核对项目 ID 与本机目录。

默认资源上限为 500 个需求、500 个工作项、500 个文档和 100 个规划版本，单个作用域业务快照最大 8 MiB。

## 开发与验证

需要 Node.js >= 22.13，使用内置 `node:sqlite`，不依赖额外数据库或跨平台原生 npm 扩展。

```sh
npm ci --ignore-scripts
npm run pack:verify
npm run preview:fixture
```

`preview:fixture` 使用与正式插件相同的 UI/HTTP 实现，但只创建临时示例数据库，用于本地视觉验收，不会读取真实项目资料。

## 仍需发布环境验收

- 通过正式远端发布链打包、签名、安装并在真实 macOS/Windows 客户端验证侧栏启动、重启、账户切换、项目切换、停用和卸载。
- macOS Bridge 已完成代码与自动化测试；Windows 已完成对应应用目录、项目选择、WebView2 Bridge 和原生任务工作区代码，但当前仓库环境没有 Windows `dotnet` 与 Windows 真机，仍需目标环境编译和 WebView/辅助技术验收。
- 仍需用真实账户、真实项目和正式安装包端到端验证任务创建、状态查询、二次确认与运行。fixture 浏览器预览不具备宿主 Bridge，不能替代该验收。
- 旧资料显式导入尚未实现；不会用旧服务回退或自动迁移填补这一缺口。
- 服务端代码和部署清单已经移除 Project Service，但本工作区尚未执行正式线上发板。新增的账户级 `task_runner_default_model_config_id` 涉及 User Service 与 ChatOS，线上使用前必须发布对应服务端。

迁移状态与验收证据见 [`docs/plans/PROJECT_PLUGIN_MIGRATION_REMEDIATION.zh-CN.md`](../../docs/plans/PROJECT_PLUGIN_MIGRATION_REMEDIATION.zh-CN.md)。
