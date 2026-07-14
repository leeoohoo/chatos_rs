# 安全与架构问题修复方案

> 状态：本轮整改已完成（P0、P1、P2 实施项均已关闭，进入持续验证与维护）
> 创建日期：2026-07-12
> 范围：Chatos、Task Runner、Plugin Management、Local Connector、Sandbox Manager、部署与 CI

## 1. 背景与原则

本次审计覆盖服务边界、用户 MCP 执行链路、沙箱控制面、本机 Connector、服务间鉴权、依赖安全、CI 和大型代码热点。

必须长期保持的核心安全原则：

- 普通用户创建的 MCP 只能在其 Local Connector Client 上执行。
- 云端 `stdio`、任意出站 HTTP MCP 只能由系统资源或超级管理员显式创建。
- Task Runner 不得直接执行普通用户提供的命令、环境变量或工作目录。
- Sandbox Manager 必须默认鉴权，且不应直接暴露到公网。
- 生产部署不得使用仓库内置开发密钥或默认管理员密码。
- 安全约束必须同时存在于创建入口、能力解析和最终执行入口，不能只依赖前端隐藏。

## 2. 已确认问题

### P0：用户 MCP 云端执行绕过

当前同时存在新旧两套 MCP 管理链路：

1. Plugin Management 的 Local Connector MCP 会正确通过 Local Connector Service 中继到客户端执行。
2. Task Runner 遗留 `/api/external-mcp-configs` 允许登录用户创建 `stdio/http` MCP，并在创建时直接执行连通性测试。
3. Plugin Management 普通用户 CRUD 仍允许 `stdio_cloud/http` runtime。
4. Task Runner 对 `stdio_cloud` 会直接构造 `McpStdioServer` 并在服务进程中运行。
5. Task Runner 只对 `source_kind=local_connector_discovered` 校验 Local Connector runtime，`user_created` 会绕过该约束。

风险：普通用户可以让 Task Runner 进程执行其指定程序，或让服务端访问任意 HTTP 地址。

### P0：Sandbox Manager 默认无鉴权

- `SANDBOX_MANAGER_REQUIRE_AUTH` 默认值为 `false`。
- `SandboxAuthContext::Disabled` 拥有管理员和全部 scope。
- Docker Compose 映射宿主机 8095 端口，并挂载完整 Docker Socket。

风险：未授权用户可以控制沙箱、镜像、租约和 MCP 调用；控制面服务被攻破后具有接近宿主机 Docker 管理权限的影响面。

### P1：部署默认密钥与公开端口

- MongoDB 默认 `admin/admin`。
- Harness 和系统超级管理员默认密码为 `admin123456`。
- JWT、内部服务 Secret、Memory/Sandbox Token 存在公开固定默认值。
- MongoDB、Consul 和多个内部后端默认映射宿主机端口。

### P1：Local Connector 本地 API 防护不足

- TCP 回退模式绑定 loopback，但允许任意 CORS Origin/Method/Header。
- 未设置 `LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN` 时跳过本地 API 鉴权。
- 本地 API 包含终端执行、工作区、模型配置和沙箱管理能力。

### P1：HTTP MCP SSRF

- HTTP MCP 只检查 `http://`/`https://` 前缀。
- 没有阻断 loopback、私网、链路本地和云元数据地址。
- 允许用户配置请求 Header。

### P1：服务间共享静态 Secret

- 多个服务共享同一个内部 Secret。
- 调用方身份通过普通 Header 声明，没有和凭据进行强绑定。
- 任一服务 Secret 泄露会扩大为跨服务冒充。
- Task Runner 的 `/internal/chatos/*` 消息任务、运行详情、输出变更与 diff 路由原先没有任何鉴权，只依赖调用方提供的 session/message/turn 标识进行数据筛选；Task Runner 端口可达时，未授权请求可能读取任务执行信息。

### P2：依赖与 CI 缺口

- GitHub Actions CI 仅支持手动触发。
- Dependabot 只覆盖 Chatos 前后端。
- CI 没有 RustSec、OSV、npm audit、镜像扫描和 Secret 扫描。
- 当前生产依赖扫描发现 Chatos 前端 2 个高危、1 个中危；Memory Engine 前端 1 个高危。
- Rust 服务镜像现已固定工具链，服务端持久化统一使用 MongoDB。
- Memory Engine 后端被排除在根 workspace 外，仓库存在多套 Cargo.lock 和依赖版本分叉。

### P2：大型文件与门禁失效

- 97 个被跟踪源文件超过 500 行。
- `plugin_management_service/backend/src/api.rs` 约 2771 行、115 个函数。
- `chat_execution.rs` 约 997 行。
- 仓库跟踪了约 16.5 MB 的预编译 `local_connector_client_core.exe`。
- `check-large-files.sh --fail` 与热点行数门禁当前不能通过。

## 3. 实施顺序

### 阶段 A：封闭用户 MCP 云端执行

- 普通用户只能创建或更新 `local_connector_stdio/local_connector_http` MCP。
- `http/stdio_cloud` 只允许超级管理员。
- Task Runner 能力策略强制 `user_created/local_connector_discovered` MCP 使用 Local Connector runtime。
- Task Runner 最终装配层拒绝执行用户来源的 `http/stdio_cloud`。
- Task Runner 遗留 External MCP API 限制为管理员迁移用途，普通用户不可访问。
- 禁止无 Plugin Management policy 时回退执行遗留用户外部 MCP。
- Plugin Management 前端对普通用户隐藏云端 runtime。
- 增加创建、更新、能力解析和执行层回归测试。

### 阶段 B：Sandbox Manager 默认安全

- 默认启用鉴权。
- 生产/容器环境缺少强凭据时拒绝启动。
- Compose 不向宿主机公开 Sandbox Manager 后端端口，或仅绑定 loopback。
- 使用受限 Docker Socket Proxy，避免 Sandbox Manager 直接挂载宿主机 Docker Socket。

### 阶段 C：部署与本机边界

- 将开发默认值和生产配置拆分。
- 生产环境检测已知默认 Secret、默认密码和短密钥并拒绝启动。
- Local Connector TCP API 始终要求随机桌面 Token；IPC 继续作为默认模式。
- CORS 仅允许内置桌面来源或显式配置来源。
- HTTP MCP 增加统一 URL/解析后 IP/重定向安全策略。

### 阶段 D：服务身份与供应链

- 内部服务改为按服务签发、带 audience/scope/过期时间的凭据，逐步替换共享 Secret。
- CI 增加 push/pull_request 触发。
- 增加 cargo audit/deny、npm audit、镜像扫描和 Secret 扫描。
- 扩展 Dependabot 到全部 workspace 和前端。
- 固定 Rust 工具链并统一 workspace 依赖基线。

### 阶段 E：大型文件治理

- 拆分 Plugin Management API 为 router、auth、MCP、Skill、Package、Binding、Runtime Resolution 等模块。
- 继续拆分 Chatos conversation execution/runtime context。
- 将预编译 Connector Core 改为发布流水线产物并校验哈希，不直接提交到源码仓库。
- 修复并启用大型文件与热点行数门禁。

## 4. 验收标准

- 普通用户提交 `http/stdio_cloud` MCP 时 API 返回 403。
- 普通用户 MCP 无论数据来源如何，都无法进入 Task Runner 本地 `Command::new` 或直接 HTTP MCP 路径。
- Local Connector MCP 仍能通过 device/manifest 中继执行。
- Sandbox Manager 未鉴权请求默认返回 401。
- 生产配置使用已知默认 Secret 时服务拒绝启动。
- Local Connector TCP API 没有 Token 时拒绝启动或拒绝受保护请求。
- HTTP MCP 无法访问 loopback、私网、链路本地和云元数据地址。
- CI 在 pull request 和 push 时自动运行，依赖安全扫描通过。
- 大型文件与热点行数门禁恢复为绿色。

## 5. 实施记录

- 2026-07-12：完成初始审计，开始阶段 A。
- 2026-07-12：完成阶段 A。Plugin Management 创建入口、Task Runner 能力策略、最终 MCP 装配层和遗留 External MCP API 均加入强制约束；普通用户创建的 MCP 只能通过 Local Connector Client 执行。
- 2026-07-12：完成阶段 B 的默认鉴权和网络暴露收敛。Sandbox Manager 默认要求鉴权，Compose 默认仅绑定 `127.0.0.1`，并通过受限 Docker Socket Proxy 访问 Docker daemon，不再直接挂载宿主机 Docker Socket。
- 2026-07-12：完成阶段 C。Local Connector TCP API 强制桌面 Token 并收紧 CORS；HTTP MCP 增加 DNS 解析后 SSRF 地址检查；生产部署脚本拒绝默认或短密钥；MongoDB、Consul 等管理端口默认仅绑定 loopback。
- 2026-07-12：增加服务自身的生产密钥校验。共享运行时统一识别 `CHATOS_ENV/NODE_ENV=production|prod`，User、Task Runner、Plugin Management、Project、Local Connector、Memory Engine 和 Sandbox Manager 在使用已知开发密钥时直接拒绝启动。
- 2026-07-12：修复 Chatos 与 Memory Engine 前端生产依赖漏洞，全部九个前端的 `npm audit --omit=dev` 通过。
- 2026-07-12：CI 增加 push/pull request 触发、RustSec 和生产 npm 依赖审计；Dependabot 扩展到全部前端与 Rust lockfile；新增 `rust-toolchain.toml` 固定 Rust 1.94.0。
- 2026-07-12：移除仓库中 16.5 MB 的预编译 `local_connector_client_core.exe`，改由 Electron 打包脚本在构建阶段生成；抽离 Project Explorer 运行设置装配逻辑，`scripts/check-large-files.sh --fail` 与热点行数门禁恢复通过。
- 2026-07-12：完成 Plugin Management 超大 API 文件的主体拆分。路由、MCP、Skill、Skill Package、Agent Binding、能力解析、资源策略、Local Connector 同步、内部鉴权和测试均进入独立模块；`api.rs` 由约 2980 行降至约 290 行，全部模块低于 500 行，30 个回归测试保持通过。
- 2026-07-12：完成 Plugin Management 内部调用凭据的第一阶段拆分。Task Runner、Project Service、Local Connector Service 分别读取独立 Secret，服务端按声明的 caller 选择对应凭据，防止一个调用方使用自己的 Secret 冒充另一个调用方；保留旧共享 Secret 作为滚动升级兼容回退。
- 2026-07-12：Sandbox Manager 不再直接挂载 Docker Socket，改为通过仅开放容器、镜像、构建和网络等必要 API 组的私有 Docker Socket Proxy 访问 Docker daemon。
- 2026-07-12：CI 增加阻断式 Secret 扫描和容器配置高危问题报告；镜像产物漏洞扫描仍待接入镜像构建流水线。
- 2026-07-12：将 Chatos `chat_execution.rs` 的 11 个回归测试迁移到独立测试模块，生产实现文件由 1079 行降至 689 行并退出 700 行热点列表。
- 2026-07-12：将 Chatos `runtime_context.rs` 的 Plugin Management 策略测试迁移到独立模块，主文件由 792 行降至 687 行并退出 700 行热点列表。
- 2026-07-12：将 Task Runner `mcp_inputs.rs` 的 9 个安全与装配回归测试迁移到独立模块，主文件由 827 行降至 595 行并退出 700 行热点列表。
- 2026-07-12：将 Project Management `execution_sync.rs` 的状态聚合回归测试迁移到独立模块，生产实现文件由 756 行降至 307 行并退出热点列表。
- 2026-07-12：将 Local Connector MCP 的云端同步与状态回写逻辑抽到 `configs/cloud_sync.rs`，`configs.rs` 由 751 行降至 591 行，28 个 Core 回归测试保持通过。
- 2026-07-12：将 Task Runner Harness 运行准备、提交和清理流程抽到 `harness_run_git/run_service.rs`，入口文件由 750 行降至 316 行。
- 2026-07-12：将 Task Runner Sandbox 的策略决策与生命周期操作拆到独立 `RunService` 模块，`sandbox_runtime.rs` 由 714 行降至 168 行，各子模块均低于 300 行。
- 2026-07-12：将 Chatos Harness 虚拟项目文件桥接拆为 handlers、client 和 responses 三层，入口文件由 724 行降至约 70 行。
- 2026-07-12：将 Local Connector 前端通用样式与表单/状态控件样式拆分，单个 `styles.css` 由 1023 行降至 591 行，生产构建通过。
- 2026-07-12：将 Chatos 中英文 `inputArea.*` 文案从 Workspace 词典拆到独立模块，Workspace 词典降至 525 行并退出 40 KB 文件大小热点。
- 2026-07-12：将 `scripts/local-dev-stack.sh` 按进程管理、环境/基础设施和服务启动拆为三个 sourced 模块，入口由 790 行降至 94 行；代码大小报告已无超过配置阈值的源文件。
- 2026-07-12：Harness 镜像流水线记录本次实际构建的镜像并使用固定版本 Trivy 逐个扫描 HIGH/CRITICAL 漏洞，发现漏洞时阻断流水线。
- 2026-07-12：Plugin Management CORS 从任意来源改为显式白名单，默认仅允许本机管理前端，并支持通过 `PLUGIN_MANAGEMENT_CORS_ORIGINS` 扩展。
- 2026-07-12：Plugin Management 内部 API 升级为 60 秒 HS256 短期令牌，令牌绑定 issuer、subject、audience、scope、签发时间和过期时间；生产环境默认拒绝仅携带旧静态 Secret 的请求，开发环境保留兼容回退。
- 2026-07-12：Project Service 内部同步链路完成同类迁移。Chatos、Task Runner、Project Service 自调用分别使用独立 Secret；`/api/chatos-sync`、Project MCP、Harness MCP 与 Harness Git 凭据入口分别校验 `project.read`、`project.sync`、`project.mcp`、`project.harness` scope。生产或显式签名模式要求 60 秒 HS256 令牌，并拒绝共享 Secret 回退；非生产环境仍保留旧 `X-Project-Service-Sync-Secret` 兼容路径。
- 2026-07-12：修复短期令牌被固化在 MCP Server 配置中的生命周期缺陷。MCP 配置仅在进程内保存调用方、scope 与签名材料，`chatos_mcp_runtime` 在每次实际 HTTP RPC 或 Codex Gateway 请求组装前重新签发 60 秒令牌，避免长对话、长任务在令牌过期后中断；签名 Secret 与内部 scope 元数据均不会发送到 Project Service 或外部模型网关。
- 2026-07-12：关闭 Task Runner `/internal/chatos/*` 无鉴权暴露。所有 Chatos 消息任务、任务图、运行详情、输出变更和 diff 内部接口现在强制校验 `chatos-backend` 调用方及 `chatos.messages.read` scope；Chatos 每次请求签发 60 秒令牌，长期 Secret 不再通过网络传输。
- 2026-07-12：Task Runner `/internal/users/:owner_user_id/execution-options` 升级为 `project-service` 调用方和 `execution-options.read` scope 的签名令牌鉴权。Chatos 与 Project Service 使用相互独立的 Task Runner 入站 Secret；生产环境拒绝旧静态 Secret，非生产环境保留兼容回退。
- 2026-07-12：User Service 内部 Harness 与模型配置接口完成短期令牌迁移。Project Service 使用独立入站 Secret，并按 `harness.repo.write`、`harness.access.read`、`model-settings.read`、`model-runtime.read` scope 访问对应接口；所有新调用只发送调用方与 60 秒签名令牌，不再传输长期 `X-User-Service-Internal-Secret`。
- 2026-07-12：完成 Local Connector 内部服务身份迁移。Chatos、Task Runner、Project Service、Memory Engine 使用相互独立的入站 Secret，调用时签发 audience 为 `local-connector-service` 的 60 秒 HS256 令牌；服务端按请求路径强制 `relay.mcp`、`relay.terminal`、`model-runtime.read` scope，并仅允许对应 caller。内部服务凭据不能再访问设备、工作区、项目绑定、插件管理、Memory Engine proxy 或 Sandbox facade 等普通受保护接口；审计身份也从统一伪装为 `task_runner` 改为真实 caller。模型运行配置和 terminal lifecycle 在每次请求时即时签名，MCP HTTP/Codex Gateway 在实际发送前动态刷新令牌，长期 Secret 与内部 scope 不通过网络发送。生产环境要求四个调用方专用 Secret，非生产环境保留旧共享 Secret 兼容回退。
- 2026-07-12：完成 Memory Engine operator token 迁移。Chatos、Task Runner、Project Service、User Service、Local Connector Service 分别使用独立 Secret，并在每次实际请求时签发 audience 为 `memory-engine` 的 60 秒 HS256 令牌；共享 SDK 根据目标路径自动选择 `memory.data`、`memory.source`、`memory.admin` 或 `model-profile.sync` scope，Local Connector proxy 与 User Service 模型同步的直接 HTTP 请求也改为即时签名。服务端按路由和 caller 校验 scope，普通内部服务凭据不能访问未授权管理接口；生产环境要求五个调用方专用 Secret 并拒绝旧 `X-Memory-Operator-Token`，非生产环境保留旧 operator token 兼容路径。
- 2026-07-12：完成 Sandbox Manager 服务凭据迁移。Task Runner 与 Project Service 使用独立 Secret，每次直接请求签发 audience 为 `sandbox-manager`、scope 为 `sandbox.service` 的 60 秒令牌；MCP 长任务在实际 HTTP/Codex Gateway 请求前动态刷新令牌，长期 client key 不出网。服务端根据真实 caller 构造不同系统客户端权限：Task Runner 仅获得租约、MCP、Pool 读取和镜像读取能力，Project Service 仅获得镜像读取与初始化能力；生产环境拒绝旧静态 bootstrap system key 回退，但数据库中显式创建和轮换的 Access Client 保持可用。Sandbox Manager 前端代理不再自动注入 operator token 或系统 client key，管理操作改为使用真实登录用户 Bearer token；agent token 签名协议保持不变。
- 2026-07-12：完成新增鉴权代码的热点文件回收。`chatos_mcp_runtime/src/rpc.rs` 中 Project Service、Local Connector、Sandbox Manager 的请求签名与动态刷新逻辑抽到 `rpc/internal_headers.rs`；Chatos Project Management API Client 的内部签名和限流 HTTP transport 拆为独立模块；Local Connector 的 Memory Engine proxy、路径白名单及用户 tenant/source 约束拆到 `api/memory_engine_proxy.rs`。代码体积报告已无超过 700 行或 40 KB 阈值的源文件。
- 2026-07-12：重新执行九个前端的生产依赖审计，全部报告 0 个漏洞。尝试在本机安装 `cargo-audit` 时 crates.io 索引下载长时间无进展，已终止安装以避免阻塞；RustSec 审计仍由 CI 对根目录、Memory Engine、User Service 三份 lockfile 阻断执行。
- 2026-07-12：完成 Rust 警告清理。根 workspace 原有 264 条去重 Clippy 警告、Memory Engine 41 条、User Service 2 条均已清零；三个 workspace 现在全部通过 `cargo clippy --all-targets -- -D warnings`。同时将 Memory Engine 工具链统一为 Rust 1.94.0，并在 GitHub Actions 与 Drone 中加入零警告阻断门禁。清理过程中将高参数构造和实时事件接口改为参数对象、统一消费型转换方法命名、移除无调用内部代码，并修复跨平台路径测试及 MCP schema 快照受 `serde_json/preserve_order` 特性合并影响的问题。

## 6. 后续维护与验证

- 服务间身份迁移已覆盖 Plugin Management、Project Service、Task Runner internal API、User Service internal API、Local Connector internal API、Memory Engine operator API 与 Sandbox Manager 服务客户端；后续新增内部调用必须沿用调用方独立 Secret、audience、scope 和短期令牌模式。
- Secret、容器配置和镜像产物扫描均已进入流水线；本机因 crates.io 索引网络阻塞未完成 `cargo-audit` 安装，需观察 CI RustSec 首次及后续运行持续保持绿色。
- 当前大型文件与热点门禁已全部通过；后续按复杂度和变更频率继续治理 500—700 行文件，并保持 700 行/40 KB 阻断阈值不回退。
