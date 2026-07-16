# Chatos RS 大文件与重复代码治理实施方案

## 1. 背景与目标

本仓库受版本控制的主要源代码约 47.8 万行，其中 Rust 约 31.9 万行，TypeScript/TSX 约 15.1 万行。审计发现全局重复率并不异常，但重复实现集中在 MCP/AI 运行时、微服务基础设施、终端与 SSH、SDK 契约模型以及多个前端公共层等关键边界；同时存在一批 800～2600 行、职责明显混杂的核心文件。

本方案目标：

- 在不改变外部行为的前提下拆分超大文件，明确模块边界。
- 删除已有公共 crate 与业务服务之间的影子实现，建立唯一权威实现。
- 统一跨服务基础设施和接口契约，降低安全修复与协议变更的遗漏风险。
- 建立持续约束，避免大文件和复制代码再次增长。

## 2. 审计范围与主要发现

审计排除了构建产物、生成类型、翻译表、测试样板以及 `docs/harness` 参考代码。

### 2.1 重点超大文件

- `sandbox_manager_service/sandbox_mcp_server/src/command_sandbox.rs`：约 2658 行。
- `sandbox_manager_service/sandbox_mcp_server/src/network_proxy.rs`：约 2014 行。
- `project_management_service/backend/src/services/environment_agent/tool_provider.rs`：约 1584 行。
- `project_management_service/frontend/src/pages/projectDetail/RuntimeEnvironmentPanel.tsx`：约 1165 行。
- `local_connector_client/core/src/sandbox/permission_layers.rs`：约 1120 行。
- `local_connector_client/core/src/api/handlers/sandbox.rs`：约 1037 行。
- `config_center_service/backend/src/state.rs`：约 1010 行。
- `local_connector_service/backend/src/store/mongo.rs`：约 969 行。
- `plugin_management_service/backend/src/api/mcps.rs`：约 882 行。
- `local_connector_client/frontend/electron/main.cjs`：约 855 行。

### 2.2 重点重复代码簇

- `chatos/backend` 与 `chatos_mcp_runtime`、`chatos_ai_runtime`、`chatos_builtin_tools` 之间存在过渡期影子实现。
- 多个微服务重复实现鉴权、环境配置、内部 HTTP 请求和错误映射。
- `memory_engine_sdk`、`chatos_plugin_management_sdk` 与对应服务端模型存在重复定义。
- Sandbox MCP 与 Task Runner 重复实现终端存储、日志分页和终端运行时。
- Chatos 与 Task Runner 重复实现 SSH、Host Key 和远程连接上下文。
- 多个独立前端重复实现 API request、AppShell、I18nProvider 和通用格式化工具。

## 3. 实施原则

- 当前 Windows 进程隔离和沙箱安全修复先完成并独立验证，再重构相关文件。
- 每个批次只处理一种职责；代码移动与行为变更不放在同一批次。
- 公共实现迁移完成后删除原副本，不长期保留双实现。
- 安全、路径、网络和鉴权逻辑继续保持 fail-closed。
- 数据库存储模型与接口 DTO 确有差异时使用 `From`/`TryFrom` 转换，不强行共用同一结构体。

## 4. 分阶段实施计划

### 阶段 0：完成当前安全修复

- 增加本地沙箱租约 TTL 自动回收。
- 强化 Windows 命名管道和本地 IPC，拒绝远程客户端并避免管道抢占。
- 实现 Windows 管理策略文件 ACL、所有者和重解析点安全校验。
- 使用 Docker Desktop 实机验证 `network none + Unix Socket 命名卷 + 固定中继`、Agent 可达性、禁止公网出站及资源回收。
- 完成 Rust、TypeScript、Electron 和安全回归测试。

### 阶段 1：收口已有公共 crate 的重复实现

- 将 `chatos/backend/core/mcp_tools` 中可复用的 execution、schema、text、parallelism 迁移到 `chatos_mcp_runtime`。
- 将 AI 流解析和模型配置统一到 `chatos_ai_runtime`。
- 将 Ask User、Code Maintainer 等通用实现统一到 `chatos_builtin_tools`。
- 业务侧仅保留类型转换、回调适配和领域特有行为。

### 阶段 2：拆分超大安全与环境模块

`command_sandbox.rs` 拆分为配置、文件策略、权限物化、路径保护、平台实现和清理模块。

`network_proxy.rs` 拆分为运行时、网络策略、HTTP、SOCKS、目标地址和 Linux/seccomp 模块。

`tool_provider.rs` 拆分为工具入口、规范化、环境推断、Compose 生成和验证模块。

`RuntimeEnvironmentPanel.tsx` 拆分为状态 Hook、各业务 Section、Columns 和格式化工具。

`permission_layers.rs` 拆分为配置加载、系统文件安全校验、权限合并、Profile 选择和来源追踪模块。

`main.cjs` 拆分为 Core 进程、本地 IPC、窗口生命周期、沙箱清理及单实例/深链模块。

### 阶段 3：统一微服务基础设施

扩展 `chatos_service_runtime`，承接：

- dotenv 和环境变量解析。
- 内部服务 Token 校验和 CurrentUser 提取。
- 标准 HTTP Client、请求超时和上游错误映射。
- 请求 ID、生产环境安全校验等公共能力。

各服务逐个迁移，领域授权规则仍保留在所属服务。

### 阶段 4：统一契约模型

- Memory Engine 以 `memory_engine_sdk` 的接口模型为权威来源。
- Plugin Management 以 `chatos_plugin_management_sdk` DTO 为权威来源。
- 增加序列化快照和双向转换测试，避免接口字段漂移。

### 阶段 5：抽取终端和远程连接运行时

- 建立 `chatos_terminal_runtime`，统一输出缓冲、日志分页、终端状态和路径处理。
- 建立 `chatos_remote_runtime`，统一 SSH 配置、Host Key、连接上下文和公共错误类型。
- Sandbox MCP、Task Runner 和 Chatos 通过适配层逐步迁移。

### 阶段 6：治理前端公共层

- 先抽取小型本地前端公共包，不立即强制切换整个 npm workspace。
- 统一 HTTP request、认证 Header、错误解析、AppShell、I18nProvider 和通用格式化工具。
- 先迁移两个最相似的服务前端，验证独立 package-lock、Vite 和 Docker 构建后再扩大范围。

### 阶段 7：持续质量门禁

- 新生产文件超过 500 行时告警，超过 800 行时必须进入有期限的白名单。
- CI 中运行代码克隆检测；不允许新增超过 24 行的生产代码克隆。
- 公共实现迁移后检查原副本已删除。
- 每批执行 Rust workspace 检查、相关测试、前端类型检查与构建。

## 5. 建议批次顺序

1. 完成 Windows 沙箱安全第二批加固和验证。
2. MCP runtime 重复实现收口。
3. AI runtime 与 builtin tools 重复实现收口。
4. Sandbox 与环境管理超大文件纯拆分。
5. 微服务配置、鉴权和 HTTP 基础设施统一。
6. SDK 契约模型统一。
7. 终端、SSH 和前端公共层治理。
8. 启用并逐步收紧 CI 门禁。

## 6. 验收标准

- 每个迁移批次对外 API 和序列化结果保持兼容。
- 安全相关用例继续验证默认拒绝和失败关闭。
- 重点生产文件降至 800 行以下；确有必要的例外记录原因和移除期限。
- 已识别的 MCP/AI、终端、SSH、鉴权和 DTO 重复簇不再保留双实现。
- 全部相关编译、类型检查、构建和回归测试通过。

## 7. 当前实施进度（2026-07-17）

### 阶段 0：已完成

- [x] 本地沙箱租约 TTL 自动回收，并在代理入口即时拒绝已过期租约。
- [x] Windows 命名管道仅允许本机客户端，启用首实例保护并强制桌面认证 Token。
- [x] Windows 管理策略文件校验所有者、DACL、符号链接和重解析点。
- [x] Docker 受限 Agent 改为 `network none`，通过命名卷 Unix Socket 与固定中继通信。
- [x] Docker Desktop 实测宿主返回 HTTP 200、Agent 无公网、Agent 网络模式为 `none`、中继为 `bridge`，清理后容器和卷均为 0。
- [x] Core、Sandbox MCP Server 编译检查及租约、Compose、容器、中继定向测试通过。

### 阶段 1：已完成

- [x] 将 Chatos 后端 `mcp_tools/text.rs` 影子实现删除，统一使用 `chatos_mcp_runtime::text`。
- [x] 将 Chatos 后端 `mcp_tools/schema.rs` 删除；公共 runtime 增加严格 MCP `inputSchema` 解析入口，迁移保持原行为。
- [x] 将宽容工具参数解析和严格 JSON 参数解析统一到 `chatos_mcp_runtime::arguments`，删除 execution 内的重复修复解析器。
- [x] 删除 Chatos 后端独立的 `mcp_execution_core/parallelism.rs`，通过元数据适配接口统一使用公共并行访问策略。
- [x] Chatos 与公共 runtime 共用 `ToolResult` 和回调类型，删除结果及回调的逐字段双向转换。
- [x] 顺序及并行 MCP execution 调度统一到公共内核；流式回调、终止检查、Task ID 异常映射和有序结果收集不再保留双实现。
- [x] Chatos builtin tool 服务枚举、调用分派和 provider 包装统一到 `chatos_builtin_tools`；业务侧仅保留 Store、Hook 与 Vision Adapter 构造。
- [x] 删除 Chatos `ai_common` 中仅在测试编译存在的 SSE、终止处理、工具生命周期与请求处理影子实现；保留领域元数据、记录写入和模型配置适配层。
- [x] 共享 `chatos_ai_runtime` 全量 137 个测试通过。

### 阶段 2：已完成

- [x] 将 `permission_layers.rs` 的测试迁到独立模块，生产文件从 1241 行降至 761 行，权限加载、合并、来源追踪和安全校验逻辑未改动。
- [x] 将 `api/handlers/sandbox.rs` 的测试迁到独立模块，生产文件从 1125 行降至 712 行，接口与校验逻辑未改动。
- [x] 将项目环境 `tool_provider.rs` 拆分为 provider 主流程、规范化/验证支持、Compose 生成和测试四个模块；主文件从 1663 行降至 394 行，各生产模块均低于 800 行。
- [x] 将 `network_proxy.rs` 拆分为运行时入口、地址校验、策略、HTTP、SOCKS、Linux 包装器和测试模块；主文件从 2138 行降至约 300 行，各子模块均低于 600 行。
- [x] 将 `command_sandbox.rs` 拆分为命令准备入口、配置/文件策略、平台实现、权限物化和测试模块；主文件从 2796 行降至约 130 行，各生产模块均低于 700 行。
- [x] 将 Local Connector 的 Managed Requirements 与原生进程沙箱内联测试迁出，生产文件分别从 1095、902 行降至 646、408 行。
- [x] 将 Sandbox Manager 租约和插件系统种子内联测试迁出，生产文件分别降至 792、747 行。
- [x] 将 Config Center `state.rs` 的迁移、校验、快照与兼容环境辅助逻辑拆到独立模块，主文件从 1065 行降至 769 行。
- [x] 将 Local Connector Service Mongo Store 拆分为连接入口、设备/工作区实体、会话租约、Managed Requirements 和测试模块；主文件从 1034 行降至 62 行，各子模块均低于 410 行。
- [x] 将 Plugin Management MCP API 拆分为路由处理和 Provider Skill/描述符/User Service 支持模块；主文件从 920 行降至 555 行，支持模块低于 400 行。
- [x] 将项目详情 `RuntimeEnvironmentPanel.tsx` 拆分为状态/交互容器和表格列、标签、JSON 渲染支持模块；主组件从 1226 行降至约 520 行，支持模块低于 750 行。
- [x] 将 Electron `main.cjs` 的 Core 进程启动、日志、IPC 端点和进程树回收迁到独立 `core-runtime.cjs`；主入口从 910 行降至约 680 行，安全策略与关闭顺序保持不变。
- [x] 将项目环境 Agent 编排拆分为稳定入口、环境启动、镜像生成和分析执行模块；主文件从 883 行降至 95 行，各实现模块均低于 500 行。
- [x] 当前已识别的重点生产文件均降至 800 行以内。

### 阶段 3：已完成

- [x] 在 `chatos_service_runtime` 增加统一 Bearer Authorization Header 解析，并用结构化错误区分缺失 Header、非法 Header 与非法 Bearer 格式；各服务适配层继续保留原有错误文案。
- [x] 在 `chatos_service_runtime` 增加标准 HTTP Client 构建入口，统一设置连接超时、总请求超时和单次读取超时；移除公共 Runtime 构建失败后退回无限默认客户端的行为。
- [x] Config Center、Local Connector、Plugin Management、Sandbox Manager 的 User Service 鉴权客户端改为启动时构建并复用，不再为每次鉴权请求重复创建客户端。
- [x] Project Service、Task Runner、User Service 和 Memory Engine 的 Bearer 解析接入公共实现；Project Service 的模型运行时客户端与 Task Runner 的 Project Service 客户端接入标准超时配置。
- [x] 公共 Runtime 增加 reqwest 错误分类，并在 Task Runner、Sandbox Manager、Memory Engine 将超时映射为 504、连接失败映射为 503、其他上游故障映射为 502，同时保留原业务错误结构。
- [x] 公共 Runtime 统一 16 KiB 错误响应预览和 8 MiB JSON/代理响应上限；Config Center、Local Connector、Plugin Management、Sandbox Manager、Memory Engine 的鉴权响应以及 Local Connector 代理响应不再无限读取。
- [x] Local Connector 的 User Service 和 Memory Engine 客户端均改为启动时构建并复用；Plugin Management 的 User Service 请求和 Task Runner 描述符读取接入公共响应限制。
- [x] 公共 Runtime 增加去空白环境文本、严格布尔、宽松开关和泛型环境变量解析；Config Center、Local Connector、Plugin Management、Sandbox Manager、Project Service、Task Runner、User Service、Memory Engine 删除对应重复实现。
- [x] 公共 Runtime 23 项、Config Center 5 项、Local Connector 21 项、Plugin Management 42 项、Sandbox Manager 12 项、Project Service 56 项和 Task Runner 198 项相关测试通过；User Service 独立工作区 22 项、Memory Engine 独立工作区 121 项测试通过。
- [x] Harness、环境分析、模型目录、回调、Sandbox Manager 客户端与 Sandbox MCP Proxy 等专用 HTTP 调用全部接入标准客户端和有界响应读取；长耗时环境启动与 AI 流式请求保留独立长超时配置。
- [x] 阶段 3 范围内的服务生产代码已不再直接创建无标准超时的 reqwest Client，也不再直接无限读取 `response.text/json/bytes`；测试中的请求构造不受影响。
- [x] 公共 Runtime 统一 dotenv 三层发现、请求 ID 生成/透传，以及不涉及领域授权的身份文本规范化辅助能力；领域角色和 owner scope 等授权规则继续保留在各服务内部。

### 阶段 4：已完成

- [x] Memory Engine 后端增加仅模型模式的 `memory_engine_sdk` 依赖；SDK 的 HTTP Client 保持默认开启，但服务端通过 `default-features = false` 避免为复用 DTO 引入客户端运行时。
- [x] Memory Engine 的 27 个 SDK 请求类型改为直接重导出 SDK 权威类型，不再在 `api/sdk_api/requests` 下逐字段复制。
- [x] Memory Engine 的线程、记录、上下文、快照、总结、主体记忆、作业策略和模型配置等完全同构契约改为直接复用 SDK 类型；服务内部携带 `source_id` 的领域请求继续保留。
- [x] Memory Engine 持久化 Source 改为显式 `StoredEngineSource`，通过双向转换进入 SDK `EngineSource`；内部 `secret_key_hash` 不会进入公共序列化结果。
- [x] Plugin Management 的 MCP、Skill、Binding、可用性、Local Connector、用户 Skill 目录和能力解析等共享 DTO 改为直接重导出 `chatos_plugin_management_sdk` 类型。
- [x] `ResourceSecurity::default()` 的安全限制由 SDK 统一定义；Local Connector MCP 同步/状态契约补齐 `workspace_id`，服务端会将其写入运行时引用，批量状态继续保持扁平 JSON 兼容格式。
- [x] 增加 Memory Engine 请求序列化快照、上下文往返测试、Source 双向转换/秘密字段隔离测试，以及 Plugin Management 安全默认值和 Local Connector 批量状态契约快照测试。
- [x] Memory Engine SDK 34 项、Plugin Management SDK 12 项、Plugin Management Service 42 项、Local Connector Service 21 项测试通过；Memory Engine 独立工作区 124 项测试通过。
- [x] 根工作区完整 `cargo check --workspace --offline` 和 User Service 独立工作区编译通过；同时修复 Sandbox MCP 在 Windows 构建中无条件导入 Linux 专用命令准备函数的问题。

### 阶段 5：已完成

- [x] 新建 `chatos_terminal_runtime`，统一 4000 条有界终端日志缓冲、单调 offset、offset/最近日志分页、Unicode 尾部截断、终端名称和工作区显示路径。
- [x] Task Runner 与 Sandbox MCP 的终端 Store 已迁移公共日志缓冲、输出采集和路径越界校验；进程启动、Sandbox Cleanup、配额终止和服务级生命周期仍保留在各自适配层。
- [x] 终端会话元数据、运行/退出状态转换和等待结果模型进入公共 Runtime，重复的 `TerminalSessionMeta` 与 `WaitResult` 已从两个消费者删除。
- [x] 终端异步输出分块读取、会话用户/项目作用域判断、活动时间更新、等待超时边界和退出/超时结果构造进入公共 Runtime；Task Runner 与 Sandbox MCP 不再各自维护这些纯运行时逻辑。
- [x] 新建 `chatos_remote_runtime`，统一 SSH 端口、认证类型、Host Key Policy、known_hosts 路径、记录格式和写入串行化；Chatos 与 Task Runner 已删除各自 Host Key 副本。
- [x] 修复 Chatos `accept_new` 在已知主机指纹不匹配时覆盖 `known_hosts` 的安全问题；现在仅首次未知主机可记录，任何 mismatch 都失败关闭。
- [x] 修复非 22 端口写入 Host Key 时误删除同主机 22 端口记录的问题，并拒绝 0 或大于 65535 的端口进入 Host Key 校验。
- [x] SSH TCP 地址解析/连接、读写超时、目标会话创建与握手、Host Key 后认证状态校验、私钥文件认证、有界 stdout/stderr 读取和远程路径规范化进入公共 Runtime；新增结构化 `RemoteRuntimeError`/`BoundedReadError`，消费者仅保留兼容错误文本映射。
- [x] Chatos 的二次验证、交互式验证码通道、跳板机认证回退与 SFTP/远程终端能力继续保留；Task Runner 的简化密码认证以及两端各自的进程清理、配额终止也继续作为服务适配层，未被较弱实现覆盖。
- [x] `chatos_terminal_runtime` 10 项、`chatos_remote_runtime` 11 项、Task Runner 192 项测试通过；Chatos 远程连接定向回归 43 项通过。减少的服务测试已迁移到公共 crate，不是用例删除。
- [x] Sandbox MCP 在 Windows 与 WSL 均编译通过；最新 WSL 全量测试共 40 项，其中 31 项通过，9 项 Bubblewrap 集成测试仅因当前环境未安装 `bwrap` 失败，未跳过、绕过或弱化安全断言。

### 阶段 6：代码实施完成（Docker 待验证）

- [x] 完成各独立前端第一轮盘点；Project Management 与 Plugin Management 的 JSON request、Token Store、Bearer Header、401 清理、查询参数和空响应处理几乎逐行相同，选为首批迁移对象。
- [x] 新建无构建步骤的本地包 `@chatos/frontend-runtime`，提供 API Base URL/路径、查询参数、浏览器 Token Store、JSON 请求、可定制成功/错误响应构造、Locale 规范化、消息插值、Translator 构造、通用日期/文件大小格式化、React Locale 状态适配器和标准后台 AppShell 工厂；包自身 12 项 Node 测试通过。
- [x] Project Management 与 Plugin Management 已迁移公共 JSON 请求层，并保留 Plugin Management 特有的 SSE 解析、流结束校验和业务事件处理。
- [x] 首批验证通过后，Task Runner 的 JSON 请求、Token Store、EventSource URL 适配、I18n 纯函数和 Locale 持久化状态也已迁移；服务特有路由、SSE Ticket、React Context 和 Ant Design Locale 包装仍保留在自身适配层。
- [x] User Service 已通过可定制错误/成功读取器接入公共请求层，继续保持 `error + detail` 拼接、`BASE_URL` 回退以及直接 JSON 成功响应语义。
- [x] Config Center 已迁移 Token、Bearer Header 和 JSON 请求骨架，并通过可配置 Content-Type 覆盖和直接 JSON 成功读取保持原行为。
- [x] Sandbox Manager 已通过可定制错误构造器迁移公共 API Base、Token、Bearer Header、查询参数和 JSON 请求骨架；本地 `ApiRequestError` 的 `status/code`、Token trim、204/空文本响应语义保持不变。
- [x] Project Management、Plugin Management、Task Runner、User Service、Config Center、Sandbox Manager 六套前端的独立 `package-lock.json` 均只增加本地 `file:../../frontend_runtime` 链接；分别执行 `npm ci`、TypeScript 类型检查和 Vite 生产构建通过。
- [x] Project Management 项目详情页两份 `YYYY-MM-DD HH:mm:ss` 日期格式化和运行环境文件大小格式化已迁移公共包，删除对 `dayjs` 的重复调用并保留本地时间、空值和单位精度语义。
- [x] Plugin Management、Task Runner、Sandbox Manager 的 Locale 读取、规范化、写入时机、存储失败策略和 `document.lang` 更新已迁移可注入 React hooks 的公共适配器；各自 Context、消息表、动态事件翻译和 Ant Design Locale 包装保持在服务内。
- [x] Task Runner 与 User Service 两套结构相同的浅色后台 AppShell 已迁移公共工厂，统一侧栏、导航、顶栏、用户摘要和退出区域；Task Runner 的权限过滤与语言切换继续由本地插槽控制。
- [ ] Docker 前端镜像构建尚未执行：当前 Docker Desktop Linux Engine 未运行，CLI 无法连接命名管道；代码侧已确认镜像构建上下文为仓库根目录，本地 `file:` 依赖位于上下文内。
- [x] 已评估 Project Management、Plugin Management、Sandbox Manager 三套深色/CSS AppShell：DOM 层级、主题、路由方式、用户区和顶栏交互差异较大，继续统一会引入大量开关，因此保留服务适配层；其他前端格式化工具也仅在精确显示语义一致时迁移。

### 阶段 7：已完成

- [x] 新增生产源码行数门禁：仅对新文件超过 500 行发出告警，所有生产文件超过 800 行必须进入 `scripts/source-size-allowlist.tsv`；测试、生成物、构建目录和文档不计入生产源码范围。
- [x] 行数白名单要求记录最大行数、ISO 到期日和原因；过期、文件已降到 800 行以内、文件不存在或实际行数超过预算都会使 CI 失败，当前无需任何超过 800 行的生产代码例外。
- [x] 新增基于 Git diff 的生产代码克隆门禁：忽略空行、纯注释、测试模块和生成目录，不允许新增 25 行及以上的精确规范化代码克隆；PR、Push 和手工触发均解析对应比较基线。
- [x] 首次运行克隆门禁发现并收口 4 组本轮新增副本：查询 Token 参数判断、Task Runner User Service 请求客户端、宽松 JSON 解析和终端等待循环均迁移公共实现，随后门禁通过。
- [x] GitHub Actions 已接入门禁自身 5 项单元测试、源码行数策略和新增克隆策略；本地门禁、`cargo fmt --all -- --check`、Chatos 编译、公共 Runtime 及相关服务回归通过。
- [x] 本轮 Rust 回归包括 Service Runtime 24 项、MCP Runtime 59 项、Terminal Runtime 10 项、Plugin Management 41 项、Project Management 55 项通过（9 项 MongoDB 用例忽略）、Task Runner 192 项通过；Sandbox Contract 在 Windows 下 25 项全部通过，Sandbox MCP Windows 编译通过。Windows 新生成测试二进制受企业 WDAC 签名策略阻止执行，未绕过策略；转由 WSL 完成全量回归，31 项通过，剩余 9 项仅因环境未安装 `bwrap` 失败。
- [x] Sandbox MCP 命令沙箱继续按职责拆分：权限入口、策略物化、原生可写根、路径规范化和临时资源清理成为独立模块；公共平台决策、Linux/Bubblewrap 与 macOS/Seatbelt 实现也已分离。原 664 行 `permissions.rs` 和 653 行 `platform.rs` 均降为薄入口，最大生产子模块 408 行；Windows 编译由 12 条平台专用告警降为零，源码大小门禁不再报告这两个文件。
- [x] 收口源码大小门禁剩余的 4 条告警：Terminal Runtime 将内联测试迁至独立模块，生产入口降至 384 行；Sandbox MCP HTTP 代理将请求头读取与解析迁至 257 行子模块，主转发模块降至约 278 行；Project Management 前端运行环境面板拆为类型、布局、表格列和值渲染模块，最大 360 行；后端 Tool Provider 支持代码拆为产物、环境变量、默认生成和服务校验模块，最大 218 行。当前源码大小策略输出 `Warnings: none`、`Violations: none`。
- [x] 上述拆分回归通过：Terminal Runtime 10 项测试通过；Project Management 前端 TypeScript 检查与 Vite 生产构建通过；Project Management 后端在 WSL 下 55 项通过、9 项 MongoDB 用例忽略；Sandbox MCP WSL 回归保持 31 项通过，9 项仅因缺少 `bwrap` 失败；Windows Sandbox MCP 测试目标可完整编译。
- [x] 完成全仓库存量源码大小复核，并继续拆分 Sandbox Contract：`permissions.rs` 将旧策略快照与校验逻辑、内联测试迁出后降至 472 行，`profiles.rs` 迁出测试后降至 493 行，`toml_profiles.rs` 将合并逻辑与测试迁出后降至 460 行；新增逻辑子模块最大 133 行。契约 25 项在 WSL 全部通过，Windows 下游 Sandbox MCP 编译通过，WSL 沙箱回归仍为 31 项通过、9 项仅缺少 `bwrap`。
- [x] 存量 `code-size-report` 的 500 行热点由 159 个降至 156 个，移除的正是上述三份生产文件；源码大小门禁继续保持 `Warnings: none`、`Violations: none`。下一批高优先级生产热点为 Sandbox Manager 租约管理、Plugin Management MCP 目录页和 Config Center 状态管理。
- [x] 完成下一批三个高优先级热点：Sandbox Manager `leases.rs` 按创建/排队、生命周期、查询和策略拆分，由 792 行降至 39 行入口，最大实现模块 384 行；Config Center `state.rs` 按初始化、发布流程、Consul 和维护迁移拆分，由 773 行降至 41 行入口，最大模块 315 行；Plugin Management `McpCatalogPage.tsx` 将弹窗、Schema 展示和 payload/runtime 纯函数迁出，由 784 行降至 408 行，最大弹窗组件 442 行。
- [x] Plugin Management 的 Skill 与 MCP 编辑表单共用 `CatalogIdentityFields`，删除名称、显示名、可见性、启用状态和描述字段的 27 行重复实现；克隆门禁重新无违规。Sandbox Manager 后端 12 项、Config Center 后端 5 项 WSL 测试通过，Plugin Management TypeScript 与 Vite 生产构建通过。
- [x] 存量 500 行热点由 156 个进一步降至 153 个；源码大小门禁保持 `Warnings: none`、`Violations: none`，格式、门禁单测、克隆门禁与补丁检查均通过。
- [x] 继续拆分 Local Connector 安全权限层：`permission_layers.rs` 按可信配置加载、权限合并/约束解析、默认 Profile 选择和策略版本摘要拆为独立模块，生产入口由 761 行降至 76 行，最大生产子模块 297 行；拒绝符号链接、Unix 所有者/写权限校验、Windows ACL 校验以及未知平台 fail-closed 行为均保持不变。
- [x] 权限层定向回归 14 项全部通过；Local Connector Core 全量回归 196 项中 186 项通过，剩余 10 项均为既有 Windows 平台边界（7 项 SQLite 文件占用、2 项稳定目录身份校验尚未实现、1 项路径分隔符断言），与本次拆分无关。门禁自身 5 项单测、源码大小门禁、克隆门禁、格式和补丁检查通过；存量 500 行热点由 153 个降至 152 个。
- [x] 将 Chatos 会话运行时 `runtime_context.rs` 按能力策略、Project Management MCP、Task Runner、远程连接透传、公共文本处理和工作目录授权拆分，入口由 749 行降至 360 行，最大生产子模块 161 行；原插件策略定向测试 5 项全部通过。
- [x] 将 Chatos 前端 `localRuntime/client.ts` 的单体客户端按资源/Git/文件系统、项目、会话和聊天 API 拆为兼容继承层，公开的 `LocalRuntimeClient` 类名和方法签名保持不变；入口由 748 行降至 79 行，最大子模块 286 行，TypeScript 检查与 Vite 生产构建通过。
- [x] 存量 500 行热点由 152 个进一步降至 150 个；源码大小门禁保持 `Warnings: none`、`Violations: none`，克隆门禁、Rust 格式和补丁检查通过。Chatos 前端现有 ESLint 10 命令因仓库缺少 `eslint.config.*` 无法启动，未通过跳过规则规避，已由类型检查和生产构建完成本批代码验证。
