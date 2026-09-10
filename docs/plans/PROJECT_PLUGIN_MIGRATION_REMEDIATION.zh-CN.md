# 项目管理插件化整改清单

日期：2026-09-10。当前结论：代码侧的 Project Service 退役、原生 Plan/规划开关删除、插件核心业务及通用 Task Runner 宿主桥接已经完成；Windows API/Connector 已跨目标编译并通过测试。macOS 已修复 Connector 重新授权后本地项目保留失效 workspace ID 的问题，并用真实 `aide` 项目完成重装验收。正式远端发板、完整插件安装验收、Windows Desktop 原生编译/真机验收和旧资料显式导入仍未完成。因此当前工作树不能直接宣称已上线。

关联主方案：[客户端项目权威与 Plan 插件化实施方案](CLIENT_OWNED_PROJECTS_AND_PLAN_PLUGIN_IMPLEMENTATION_PLAN.zh-CN.md)。本清单以当前代码为准；主方案第 15 节前几批中的“尚未完成”属于历史阶段记录。

## 不可退让的边界

- 客户端 `ProjectRegistry` 是项目 ID、名称、目录和生命周期的唯一权威；Git/文件由客户端与 Local Connector 执行。
- 需求、文档、项目工作项、业务依赖、冻结规划和执行意图属于 `plugins/project-management`。
- Task Runner 是 Task/Run 执行状态唯一权威。插件只保存不透明引用，不复制运行状态。
- 不保留旧 Project Service 回退、双写、兼容开关、接口别名、隐式数据迁移或第二份项目实体。
- 删除原生 Plan 就是删除，不替换成项目 Tab 内的插件入口。插件只走客户端侧栏“应用”及既有项目绑定流程。
- 页面按产品交付：统一 Apple/Studio 设计语言，具备真实编辑、错误恢复、并发保护和安全边界，不以 fixture 预览代替宿主验收。

## 当前状态

| 编号 | 范围 | 代码状态 | 尚欠验收/工作 |
| --- | --- | --- | --- |
| R01 | 聊天“规划”开关与专用分流 | **完成**：macOS/Windows UI、状态、协议字段、服务端判断、`chatos_plan` 路由、专用 Agent/MCP 已删除；独立推理开关保留 | Windows 真机和正式安装包回归 |
| R02 | 原生 Plan 页面与旧需求执行链 | **完成**：macOS/Windows 原生页面、DTO、服务、确认/停止/轮询、消息 metadata 兼容、宠物活动已删除；项目页无替代插件 Tab | 正式安装包再次肉眼验收 |
| R03 | 需求与关系 | **完成**：需求树、祖先/子孙路径、前置/后续闭包、工作项和文档关联均在插件 | 真实数据规模与辅助技术验收 |
| R04 | 独立文档 | **完成**：多类型、多格式、独立 identity、不可变版本、多对多需求关联、历史阅读、安全 Markdown/SVG 展示 | 真实项目内容验收 |
| R05 | 规划与执行交接 | **代码完成**：冻结规划、依赖闭包、精确文档版本、执行意图单独批准、opaque refs；两端宿主提供 DAG 创建、真实状态查询和原生任务工作区；Windows API/Connector 测试通过 | 真实账户端到端；Windows Desktop 原生编译/真机；正式安装包 |
| R06 | 插件产品与打包 | **本地完成**：manifest、MCP、SQLite、UI、17 项插件测试与 pack 验证通过 | 正式远端发布、签名、真实客户端安装/停用/卸载/重启 |
| R07 | 其他插件项目绑定与透传 | **代码链完成**：两端宿主从客户端注册表生成项目上下文，UI/MCP 同 scope/data dir，缺上下文拒绝；现有 Studio 插件已审计；Windows 服务层跨目标测试通过 | Windows Desktop/真机、账户/项目切换和授权撤销端到端验收 |
| R08 | Project Service 彻底退役 | **代码完成**：微服务源码、workspace member、消费者、provider、旧 Agent/MCP、管理后台、配置、mTLS、CI、Compose、部署清单均删除 | 线上发布后停服并做无旧服务验收 |
| R09 | 旧数据与跨平台 | **部分完成**：Windows API/Connector 跨目标编译和测试通过 | 显式导入、Windows Desktop 原生构建/真机、真实账户与目录验收 |
| R10 | 禁止把局部结果当上线 | **持续约束** | 最终以正式安装与线上无旧服务流程验收 |
| R11 | Connector 重新授权后的本地项目绑定 | **macOS 已修复并实机验收**：只在旧 workspace ID 失效且当前存在有效 `/` 根授权时安全重绑，CAS 更新客户端注册表 | Windows 对等行为审计；非根授权继续要求用户显式重选 |

## R11：失效 workspace ID 的安全重绑

实机问题表现为项目设置页显示“没有找到可用的本机工作区”。根因不是目录不存在，而是 Connector 重新配对/授权后生成了新的 workspace grant ID，客户端 `ProjectRegistry` 仍持有旧 ID；旧实现只按 ID 查找，因此无法解析仍然存在且仍被当前账户授权的目录。

修复遵循客户端项目唯一权威，不引入 Project Service、隐式服务端迁移或按相对路径随意猜测：

- 当前账户与 device 必须匹配，旧 workspace ID 必须已经失效。
- 只允许使用当前有效且物理根目录为 `/` 的本机授权恢复；URI 中的 `relativeRoot` 在这种授权下是去掉前导 `/` 的完整绝对路径。
- 目录必须存在且为文件夹，并再次通过 `NativeWorkspaceFilesystem` 的规范化、符号链接和授权边界检查。
- 多个 `/` grant 解析到同一物理目录时确定性选择一个；若出现多个不同物理结果则失败关闭。
- 项目专用的非 `/` workspace 不做推测性替换，继续返回不可用并要求用户显式重新选择。
- 恢复成功后以 revision CAS 更新本地 `ProjectRegistry.workspaceID`；项目 ID、名称和 `relativeRoot` 不变，revision 递增。插件上下文、任务上下文和项目列表随后都从新记录生成。

2026-09-10 验证：`NativeLocalProjectsServiceTests` 11/11 通过；正式 macOS 包通过本地化审计、构建和签名校验后重装启动。真实 `aide` 记录的 workspace ID 从失效值更新为当前 `/` grant，revision 从 1 变为 2；项目设置页成功解析目录并显示“没有发现阻塞问题”，原错误不再出现。旧 App 已移动到废纸篓保留，不涉及服务端发板。

## R01：规划开关整链路删除

删除范围包括按钮本身以及与它相关的允许条件、ViewModel 状态、设置动作、请求 DTO、会话持久化、bootstrap/runtime context、Agent 选择、Task Runner profile 归一化、MCP 工具集和管理端配置。旧 snake_case/camelCase 字段被严格请求模型拒绝，不固定成 `false`，也不映射为普通任务。

`CHATOS_ASYNC_PLANNER_TOOL_PROFILE` 和 `task_runner_async` 是通用异步任务图能力，不是被删除的聊天规划模式，必须保留。Plugin Management 的 retired Agent key 只用于 seed 删除线上旧记录，不表示运行时仍支持旧 Agent。

## R02：原生 Plan 与隐藏执行兼容链

macOS 和 Windows 项目工作区不再有 Plan 页。macOS 还删除了此前藏在消息任务工作区中的旧需求执行链，包括：

- `/projects/{id}/requirements/{id}/execution-plan`
- `confirm-execution`
- `stop/discard_tasks`
- `project_requirement_execution` 会话 metadata 解析
- execution group 覆盖通用任务图、旧任务隐藏、专用状态横幅和宠物活动

消息任务工作区只读取通用 `task_runner_async` Task/Run 图、详情、重试和实时 process update。`MessageTaskLookup` 不再从旧 execution group 回退。

插件不会被塞进项目 Tab。它从侧栏“应用”启动，由宿主选择并绑定客户端项目；项目目录、消息、设置仍是项目页自身功能。

## R03–R06：插件业务与产品形态

插件拥有以下本地业务数据：

- 需求层级、业务状态、验收标准和需求依赖。
- 项目工作项、归属、验收标准、业务状态和工作项依赖。
- 独立规划文档、类型/格式、发布状态、不可变版本和需求多对多关联。
- 冻结规划、完整前置闭包、固定需求/工作项/文档版本。
- 执行意图、单独批准和不透明 Task Runner batch/task/run 引用。

插件明确不拥有：项目 CRUD、名称/目录、Git、文件系统授权、Task/Run 状态和用户长期凭据。

UI 与 MCP 使用宿主提供的同一项目隔离 `CHATOS_PLUGIN_DATA_DIR`。数据库使用 SQLite WAL、事务、scope revision CAS 和幂等 request receipt。缺少项目上下文、scope 误绑定、旧 v1 schema、数据库损坏、数据目录或数据库符号链接全部失败关闭，不回退到 cwd/device/旧服务。

页面使用与其他 Studio 插件一致的系统字体、系统蓝、三栏层级、细分隔线、深浅色和紧凑工具栏；默认阅读、显式编辑，包含搜索、空状态、未保存离开保护、取消编辑、Cmd/Ctrl+S、保存状态、确认弹窗和中文业务错误。fixture 只用于视觉检查，不读取真实项目数据。

通用执行 Bridge 由宿主承担，不把内部 HTTP 或长期 token 暴露给插件。macOS 已实现并通过 `ChatOSTaskRunnerHostServiceTests`；Windows 已实现对应 WebView2 会话、origin/session/nonce/大小/allowlist 校验、项目快照重取、Task Runner 默认模型重取、DAG 创建、批量状态和原生任务工作区。Windows 五个测试工程已用临时 .NET 8.0.424 SDK 跨目标编译并共通过 425/425 测试；新增插件页 code-behind 也通过隔离 C# 编译，Desktop 仍须在 Windows 执行 WinUI XAML 编译和真机验收。插件请求模型不含模型字段，Windows 还以严格 JSON 反序列化拒绝未知覆盖字段。

宿主固定账户、项目、component、Release、artifact hash 与默认模型。插件提交稳定 idempotency key 和工作项 DAG；宿主以稳定 tag、内容摘要和 opaque metadata 恢复部分成功或重复提交，当前不是 Task Runner 服务端原子 batch 实体。任务工作区重新读取 Task Runner 状态，二次确认后才启动 Run；关闭插件不会停止已经启动的任务。

Task Runner 默认模型是账户级通用设置，不是恢复“项目管理 Agent 默认模型”。User Service 保存 `task_runner_default_model_config_id`，ChatOS 完整透传；宿主执行前重新读取并校验模型仍存在、启用、允许 Task 且凭据可用。该字段上线前必须发布 User Service 与 ChatOS。

## R07：项目上下文与其他插件审计

| 链路 | 当前实现 |
| --- | --- |
| macOS 应用启动 | `AppModel.launchPluginApplication` 按已认证 owner 和选中 project ID 调用本地项目服务；启动前后复核账户 generation |
| macOS UI/MCP runtime | 共用 `NativePluginRuntimeContextResolver`；owner/plugin/project hash 决定 scope/data dir，项目名只用于展示 |
| Windows runtime | manifest loader 校验 required context，并按账户、插件和 project ID 隔离；`missingContext=reject` 不走 device fallback |
| Task Runner/MCP | 使用客户端项目快照与 Local Connector 授权结果，冻结 owner、workspace fingerprint 和执行目标；不通过 project ID 查询旧服务 |
| Diagram/Web Design Studio | 插件内部的作品集合是业务容器，不是 ChatOS 项目实体；保留其业务模型，但宿主 scope 身份仍来自统一上下文 |

验收约束：项目改名不能产生第二份插件资料；跨账户/项目不能打开同一库；UI 写入后 MCP 可读，反向亦然；项目删除、目录改绑、授权撤销、停用和账户切换必须令旧运行时失效。

## R08：Project Service 退役与发板

已删除：

- `project_management_service/` 全部源码与 Cargo workspace 成员。
- ChatOS、Task Runner、MCP Management、User Service 等消费者中的 Project Service client/provider、旧项目路由、Harness/Workspace/需求执行编排和状态回写。
- System Project Management MCP、专用需求执行 Agent/提示词/绑定和管理后台模块。
- Project Service mTLS 生成脚本、配置项、Docker/CI/Compose/Makefile/官网状态和远程部署列表。
- 旧需求执行数据修复脚本与仅为旧链路存在的测试。

共享逻辑中仍有价值的客户端 logical workspace path 工具已从 `chatos_project_execution` 正名为 `chatos_local_workspace`，不再暗示旧项目执行域仍存在。

代码删除不等于线上已退役。要让线上生效，需要发布所有受影响服务和新插件，然后停止/移除旧 Project Service 实例。当前没有执行线上部署，也没有读取或输出部署密码。

正式发布顺序：

```sh
# 发布受影响的后端、管理后台、官网和新 Compose；使旧 Project Service 离开运行拓扑
scripts/deploy-online.sh cloud

# 构建、校验并发布项目管理插件
scripts/deploy-online.sh plugin project-management

# 服务和插件可用后再发布客户端
scripts/deploy-online.sh client mac
scripts/deploy-online.sh client windows
```

插件发布需要 `CHATOS_DEPLOY_ADMIN_PASSWORD` 或交互输入管理员密码。密码不得写入仓库、日志或本文档。

## R09：剩余明确工作

1. 实现一次性显式旧资料导入。导入必须由用户确认本机目录，保留项目 ID，提供预览、校验、幂等回执和冲突拒绝；禁止自动连接旧服务或双写。
2. 在 Windows 环境原生编译并运行 Desktop 和 WebView2，验证插件 UI/MCP、深色、窄窗口、键盘与辅助技术。本机跨目标服务层编译不能替代这一步。
3. 用真实账户和正式安装包验收通用 Task Runner Bridge：批准前绝不执行、重复提交不重复创建、项目/模型变化失败关闭、二次确认后只由宿主启动 Run、插件查询状态不复制状态。
4. 走正式插件发布/签名/安装链，在真实账户与真实项目上验收启动、重启、切换、撤销、停用、卸载和恢复入口。
5. 发布 User Service、ChatOS 及其余受影响服务后，在 Project Service 完全停止的环境做项目、会话、Task、MCP、Memory 关联和现有插件回归。

## 当前验证证据

2026-09-09 当前工作树已执行：

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

dotnet test \
  clients/windows/tests/ChatOS.Api.Tests/ChatOS.Api.Tests.csproj \
  -p:EnableWindowsTargeting=true
dotnet test \
  clients/windows/tests/ChatOS.Connector.Tests/ChatOS.Connector.Tests.csproj \
  -p:EnableWindowsTargeting=true

# 同样执行 ChatOS.Core.Tests、ChatOS.Presentation.Tests 与 ChatOS.NetworkGuard.Tests

swift build --package-path clients/macos --target ChatOSCore
swift build --package-path clients/macos --target ChatOSAPI
swift build --package-path clients/macos --target ChatOSApp
swift test --package-path clients/macos --filter \
  'ConversationHistoryMapperTests|ConversationHistoryStoreTests|PetActivityRecoveryMapperTests|PetStateReducerTests|ConversationTurnMessageTaskLookupTests|MessageTaskWorkspaceViewModelTests|ProjectManagementPluginContextTests'
```

结果：Rust 生产与测试目标编译通过；ChatOS 424、Task Runner 344、Plugin Management 130 项单测通过，MCP Management 176 项通过/3 项既有集成测试忽略；项目管理插件 17/17 测试与 pack 验证通过；macOS 三个目标构建通过，Task Runner Host Service 测试 2/2 通过，相关 Swift 测试通过。macOS 产品打包审计发现并补齐任务工作区英文文案及状态本地化后，审计 0 缺失、签名校验通过，已在本机将旧 App 移入废纸篓后重装并启动新包；这仍不等于尚未发布插件的正式安装验收。Windows Core 19/19、API 48/48、Presentation 47/47、Connector 292/292、NetworkGuard 19/19，共 425/425 测试通过，对应生产程序集完成跨目标编译；期间修复了 Task Runner 查询参数被编码进 path 的真实路由问题，并补齐客户端项目测试的 Connector 授权状态。Desktop 依赖项目和新增插件页 code-behind 可以编译，WinUI XAML 编译在 macOS 因 Windows `kernel32.dll`/`XamlCompiler.exe` 平台依赖停止，因此不能声称 Windows Desktop 原生编译或真机测试通过。正式远端部署与真实插件安装尚未执行。

账户级通用 Task Runner 默认模型字段为 `task_runner_default_model_config_id`。该字段由 User Service 持久化并经 ChatOS 透传；模型删除会清除引用，保存时校验归属、启用、Task 能力及云端凭据。线上启用插件执行交接前必须发布 User Service 与 ChatOS，不只发布客户端和插件。

## 完成判定

只有同时满足以下条件才能把本次迁移标为上线完成：

- 新客户端和插件正式安装后，无规划按钮、无原生 Plan、无项目 Tab 替代入口。
- 需求、文档、工作项、依赖、冻结规划和执行交接均在插件真实项目数据上可用。
- 客户端注册表是项目唯一权威，插件与服务端没有第二份项目 CRUD。
- Project Service 实例停止后，项目、会话、Task、MCP、Memory 和其他插件仍正常。
- macOS/Windows、账户/项目隔离、目录改绑/撤销、插件停用/卸载和显式导入全部验收。
