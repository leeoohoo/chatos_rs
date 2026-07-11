# 项目质量与重构实施计划

> 生成日期：2026-07-09
> 范围：`C:\project\learn\chatos_rs` 当前工作区只读审查结果。

## 1. 背景与目标

当前仓库是多服务 monorepo，包含 `chatos`、`task_runner_service`、`project_management_service`、`local_connector_service`、`local_connector_client`、`memory_engine`、`sandbox_manager_service`、`user_service` 以及多个共享 crate。

本次审查发现的主要问题不是单点 bug，而是长期维护成本问题：

- 多个大文件承载过多职责，修改风险高。
- 共享协议、工具逻辑和配置逻辑重复，容易出现行为漂移。
- 部分仓库治理脚本在 Windows 工作区不可直接运行。
- Rust workspace 与依赖版本治理不统一。
- `memory_engine_sdk` 存在跨目录源码引用和双 package 形态。

目标：

1. 恢复并强化仓库治理检查。
2. 优先抽出跨服务重复协议和公共工具逻辑。
3. 分阶段拆分大文件，降低改动冲突和回归风险。
4. 明确 workspace、SDK、依赖版本治理策略。

## 2. 当前规模观察

源文件统计结果：

- 约 2541 个源文件。
- 约 401798 行源代码。
- 超过 500 行的源文件约 102 个。
- 超过 800 行的源文件约 21 个。

已跟踪的大体积文件主要是 `bundled-tools/*/rg` 二进制；`target-*`、`node_modules`、`.cache`、`.local` 等本地产物已被 `.gitignore` 排除，未发现明显误提交的构建产物。

## 3. 明显问题清单

### 3.1 大文件和职责混杂

#### `local_connector_client/frontend/src/main.tsx`

- 约 2510 行。
- 一个文件内同时包含认证、连接状态、工作区、终端、审批、模型配置、沙箱等完整 UI。
- 风险：
  - 单文件修改冲突概率高。
  - UI 状态和业务逻辑难以单独测试。
  - 后续新增功能会继续扩大该文件。

建议拆分方向：

- `components/AuthPanel.tsx`
- `components/WorkspacePanel.tsx`
- `components/TerminalPanel.tsx`
- `components/ApprovalPanel.tsx`
- `components/ModelConfigPanel.tsx`
- `components/SandboxPanel.tsx`
- `hooks/useLocalConnectorStatus.ts`
- `hooks/useModelConfigState.ts`
- `utils/modelConfigPayload.ts`

#### `sandbox_manager_service/backend/src/service/manager.rs`

- 约 2161 行。
- `SandboxManager` 同时处理 lease 生命周期、workspace 准备、输出 diff、MCP proxy 授权、access client 管理、工具函数。
- 风险：
  - 核心服务类变成“上帝对象”。
  - workspace 文件操作、授权、lease 状态流转互相耦合。
  - 测试粒度被迫围绕大 manager 展开。

建议拆分方向：

- `service/leases.rs`
- `service/workspace.rs`
- `service/output_manifest.rs`
- `service/mcp_proxy.rs`
- `service/access_clients.rs`
- `service/path_utils.rs`

#### `local_connector_service/backend/src/api.rs`

- 约 2004 行。
- 同时包含路由、认证、设备/工作区/绑定 CRUD、Memory Engine proxy、MCP relay、terminal relay、WebSocket relay、sandbox facade。
- 风险：
  - API 层、relay 层、权限层和协议转换层混在一起。
  - 修改某一类 endpoint 时容易影响其他 endpoint。
  - 很难建立清晰的模块测试。

建议拆分方向：

- `api/router.rs`
- `api/auth_middleware.rs`
- `api/devices.rs`
- `api/workspaces.rs`
- `api/project_bindings.rs`
- `api/sandbox_pairings.rs`
- `api/memory_proxy.rs`
- `api/mcp_relay.rs`
- `api/terminal_relay.rs`
- `api/sandbox_facade.rs`

#### `project_management_service/backend/src/services/environment_agent.rs`

- 约 1940 行。
- 同时负责项目运行环境分析入口、agent 编排、MCP server 构造、路由决策、本地项目扫描、Memory Engine 初始化、环境变量生成。
- 风险：
  - 业务策略、运行时执行、外部服务接入和本地文件扫描耦合。
  - 路由决策难以独立测试。
  - Agent 行为变更会触及大量无关代码。

建议拆分方向：

- `services/environment_agent/orchestrator.rs`
- `services/environment_agent/routing.rs`
- `services/environment_agent/mcp_servers.rs`
- `services/environment_agent/memory.rs`
- `services/environment_agent/local_inspection.rs`
- `services/environment_agent/env_vars.rs`
- `services/environment_agent/tool_provider.rs`

#### `project_management_service/backend/src/api/harness_mcp.rs`

- 约 1876 行。
- 同时包含 JSON-RPC handler、Harness API access、工具定义、工具执行、patch 解析、文件读写、提交动作构造。
- 风险：
  - MCP 协议层和 Harness REST client 绑定过紧。
  - 工具定义和工具执行难以复用。
  - patch 解析属于独立领域，不应和 HTTP handler 混在一起。

建议拆分方向：

- `api/harness_mcp/router.rs`
- `api/harness_mcp/context.rs`
- `api/harness_mcp/client.rs`
- `api/harness_mcp/tools/read.rs`
- `api/harness_mcp/tools/write.rs`
- `api/harness_mcp/tools/patch.rs`
- `api/harness_mcp/tool_definitions.rs`
- `api/harness_mcp/path_policy.rs`

#### `project_management_service/backend/src/mcp_tools.rs`

- 约 1183 行。
- 主要问题是一个大型 `call_tool` match 负责所有 MCP 工具分发、权限、参数解析和 store 调用。
- 风险：
  - 新增工具会继续扩大单个 match。
  - 工具权限和参数校验不容易统一审计。

建议拆分方向：

- `mcp_tools/dispatch.rs`
- `mcp_tools/project.rs`
- `mcp_tools/requirements.rs`
- `mcp_tools/tasks.rs`
- `mcp_tools/documents.rs`
- `mcp_tools/pagination.rs`
- `mcp_tools/conversions.rs`

## 4. 重复代码问题

### 4.1 Terminal controller JSON schema 重复

重复位置：

- `local_connector_client/core/src/terminal/controller/store/logs.rs`
- `local_connector_client/core/src/terminal/controller/store/process/query.rs`
- `local_connector_client/core/src/terminal/controller/store/process/control.rs`
- `sandbox_manager_service/sandbox_mcp_server/src/terminal_store/mod.rs`
- `task_runner_service/backend/src/terminal_store/ops/controller_api/inspect.rs`

重复内容：

- recent logs 响应结构。
- process list 响应结构。
- process poll 响应结构。
- process wait 响应结构。
- `result_scope`、`terminal_count`、`returned_log_count`、`truncation` 等字段构造。

风险：

- 这是跨运行环境协议，不只是普通重复代码。
- 字段新增、重命名或语义修复时容易只改一处。
- 前端或模型工具依赖这些 JSON 字段，drift 会导致隐性兼容问题。

建议：

- 在共享 crate 中新增 terminal controller response builder。
- 候选位置：
  - `crates/chatos_builtin_tools/src/terminal_response.rs`
  - 或新增 `crates/chatos_terminal_contract`
- 使用 typed struct 或 builder 生成 `serde_json::Value`。
- 三个运行环境只负责采集 session/meta/logs，统一调用 builder。

### 4.2 HTTP response body 限流重复

重复位置：

- `crates/chatos_ai_runtime/src/request/http.rs`
- `project_management_service/backend/src/http_body.rs`
- `task_runner_service/backend/src/http_body.rs`

重复内容：

- `read_response_body_limited`
- `ensure_response_body_within_limit`
- limited text/json 读取。

风险：

- 错误消息、限制语义和 body 读取逻辑不一致。
- 后续修复 streaming body 限流时容易漏服务。

建议：

- 下沉到 `crates/chatos_service_runtime`。
- 暴露：
  - `read_response_bytes_limited`
  - `read_response_text_limited`
  - `read_response_text_limited_or_message`
  - `read_response_json_limited`

### 4.3 远程连接 payload 类型重复

重复位置：

- `chatos/frontend/src/lib/api/client/types/remoteConnection.ts`
- `chatos/frontend/src/lib/api/client/workspace/common.ts`
- `chatos/frontend/src/lib/store/actions/remoteConnections.ts`
- `chatos/frontend/src/lib/store/slices/remoteExecutionSlice.ts`

重复内容：

- `auth_type`
- `password`
- `private_key_path`
- `certificate_path`
- `default_remote_path`
- `host_key_policy`
- jump host 相关字段。

风险：

- API payload、store mutation payload、UI draft payload 容易字段不一致。
- 驼峰/蛇形字段兼容逻辑分散。

建议：

- 在 `chatos/frontend/src/lib/api/client/types/remoteConnection.ts` 定义单一源类型。
- Store action 和 slice 只 import 类型，不再本地重复声明。
- 如果 UI draft 需要不同 optional 规则，使用 `Pick`、`Partial`、`Omit` 组合。

### 4.4 多个 Vite 配置重复

重复位置：

- `user_service/frontend/vite.config.ts`
- `task_runner_service/frontend/vite.config.ts`
- `project_management_service/frontend/vite.config.ts`
- `memory_engine/frontend/vite.config.ts`

重复内容：

- `parsePort`
- `normalizeBasePath`
- basePrefix 代理 rewrite。

风险：

- 子路径部署和 dev proxy 行为容易 drift。
- 每个服务独立修复路径问题，成本高。

建议：

- 新增共享 helper，例如 `scripts/frontend/viteShared.ts` 或 `crates` 外的 `frontend_shared/vite.ts`。
- 各服务只保留自身端口、代理目标和额外 rollup/test 配置。

### 4.5 Code nav 多语言搜索重复

重复位置：

- `chatos/backend/src/services/code_nav/languages/basic/search.rs`
- `chatos/backend/src/services/code_nav/languages/rust/search.rs`
- `chatos/backend/src/services/code_nav/languages/java/analysis.rs`
- `chatos/backend/src/services/code_nav/languages/python/analysis.rs`
- `chatos/backend/src/services/code_nav/languages/go/analysis.rs`

重复内容：

- whole word regex 构造。
- WalkDir 遍历。
- search budget 检查。
- 文件读取、行扫描、列号计算、preview 截断。

风险：

- 搜索行为跨语言不一致。
- budget、忽略目录、preview 规则修复要多处改。

建议：

- 在 `shared_nav.rs` 中继续下沉一个通用 `search_text_occurrences`。
- 语言实现只传：
  - 文件过滤器。
  - ignored dirs。
  - match struct 构造函数。

### 4.6 `memory_engine_sdk` 双 package / 跨路径引用

当前形态：

- `memory_engine/sdk/Cargo.toml` 声明 package `memory_engine_sdk`。
- `crates/memory_engine_sdk/Cargo.toml` 也声明 package `memory_engine_sdk`。
- `crates/memory_engine_sdk/src/lib.rs` 通过 `#[path = "../../../memory_engine/sdk/src/client/mod.rs"]` 和 `#[path = "../../../memory_engine/sdk/src/models/mod.rs"]` 引用源码。

风险：

- IDE、cargo publish、路径重构和源码归属都不清晰。
- `memory_engine/sdk` 和 `crates/memory_engine_sdk` 可能出现实际分叉。
- 根 workspace 使用的是 `crates/memory_engine_sdk`，但源码实质在另一个目录。

建议：

- 将 SDK 源码正式迁入 `crates/memory_engine_sdk/src`。
- `memory_engine/sdk` 改为 README 指向根 shared crate，或删除独立 package。
- 保留一个 package 名，避免两个同名 crate。

## 5. 仓库治理问题

### 5.1 Bash 脚本在 Windows 工作区直接失败

现象：

- `scripts/code-size-report.sh`
- `scripts/check-hotspot-line-budgets.sh`
- `scripts/check-large-files.sh`

在当前工作区运行会出现：

```text
$'\r': command not found
set: pipefail\r: invalid option name
```

原因：

- `git ls-files --eol` 显示这些脚本是 `i/lf w/crlf`。
- 仓库没有 `.gitattributes` 固定 shell 脚本工作区换行为 LF。
- 当前 `core.autocrlf=true`。

建议：

- 新增 `.gitattributes`：

```gitattributes
*.sh text eol=lf
*.py text eol=lf
*.rs text eol=lf
*.ts text eol=lf
*.tsx text eol=lf
*.md text eol=lf
```

- 执行一次文件换行规范化。
- 在 CI 中执行脚本，避免 Windows checkout 后脚本不可用。

### 5.2 Python 检查脚本默认编码问题

文件：

- `scripts/check-non-test-unwrap-expect.py`

问题：

- 使用 `path.read_text()` 未指定 encoding。
- 当前 Windows 默认 GBK 下读取 UTF-8 Rust 源码会触发 `UnicodeDecodeError`。

建议：

- 改为：

```python
text = path.read_text(encoding="utf-8")
```

- 如果要容忍历史文件，可加 `errors="replace"`，但治理脚本更建议 UTF-8 强约束。

## 6. Rust workspace 和依赖版本治理

观察：

- 根 `Cargo.toml` workspace 覆盖大部分服务，但排除了 `memory_engine/backend`、`memory_engine/sdk`。
- `user_service/backend/Cargo.toml` 内有独立 `[workspace]`。
- 依赖版本存在差异：
  - `axum` 同时有 `0.7` 和 `0.8`。
  - `tower-http` 同时有 `0.5` 和 `0.6`。
  - `mongodb` 同时有 `2.8` 和 `3`。

风险：

- 共享 crate 需要兼容多套 web stack。
- 安全升级和 API 迁移成本高。
- 锁文件和构建路径分散，CI 复杂度增加。

建议：

- 明确“统一 workspace”还是“服务独立 workspace”的策略。
- 如果继续 monorepo workspace，优先统一 `axum`、`tower-http`、`mongodb` 版本。
- 如果保持独立 workspace，需要建立依赖版本基线文档和 drift 检查。

## 7. 分阶段实施计划

### Phase 0：建立可运行的治理基线

目标：

- 让已有检查脚本在本地和 CI 都能稳定运行。

任务：

- 新增 `.gitattributes`，固定脚本和源码换行。
- 修复 `scripts/check-non-test-unwrap-expect.py` UTF-8 读取。
- 验证：
  - `bash scripts/code-size-report.sh --top 50`
  - `bash scripts/check-hotspot-line-budgets.sh --warn-planned`
  - `bash scripts/check-large-files.sh --threshold 1`
  - `python scripts/check-non-test-unwrap-expect.py`

验收标准：

- Windows checkout 后上述脚本不再因为 CRLF 或编码失败。
- CI 增加或保留对应 smoke 检查。

### Phase 1：抽取低风险重复代码

目标：

- 先处理跨服务重复、无业务语义变化的 helper。

任务：

1. 将 HTTP response body 限流下沉到 `crates/chatos_service_runtime`。
2. 将 frontend Vite `parsePort`、`normalizeBasePath`、base proxy helper 下沉为共享 TS helper。
3. 将 remote connection payload 类型统一到 API types。
4. 将 code nav 文本搜索通用循环下沉到 `shared_nav.rs`。

验收标准：

- 删除重复实现。
- 相关服务编译通过。
- 原有测试通过。
- 行为保持兼容。

### Phase 2：统一 terminal controller 协议输出

目标：

- 避免 local connector、sandbox MCP server、task runner 三处 terminal JSON schema drift。

任务：

1. 设计共享 DTO / builder：
   - recent logs response。
   - process list response。
   - process poll response。
   - process log response。
   - process wait response。
2. 确定共享 crate：
   - 优先 `chatos_builtin_tools`。
   - 如果依赖方向不合适，新增 `chatos_terminal_contract`。
3. 三个运行环境改为调用共享 builder。
4. 添加 snapshot 风格测试，固定 JSON 字段。

验收标准：

- 三个运行环境输出字段一致。
- 前端和模型工具依赖的字段不变。
- 以后新增字段只需要改共享 builder。

### Phase 3：拆分后端大文件

目标：

- 按边界拆文件，不做行为重写。

优先顺序：

1. `local_connector_service/backend/src/api.rs`
2. `project_management_service/backend/src/api/harness_mcp.rs`
3. `project_management_service/backend/src/services/environment_agent.rs`
4. `sandbox_manager_service/backend/src/service/manager.rs`
5. `project_management_service/backend/src/mcp_tools.rs`

策略：

- 每次只移动一个职责域。
- 保留原 public API。
- 每次拆分后跑对应服务测试或至少 `cargo check -p`。
- 不在同一 PR 混入逻辑重写。

验收标准：

- 单个文件行数逐步降到 700 行以内。
- 新模块职责清晰。
- 测试覆盖原有关键路径。

### Phase 4：拆分 `local_connector_client/frontend/src/main.tsx`

目标：

- 将 2500 行单文件拆成组件、hooks、utils。

任务：

1. 先抽纯函数：
   - model config payload 构造。
   - provider label / thinking options。
   - terminal history/status format。
2. 再抽独立 panel：
   - AuthPanel。
   - WorkspacePanel。
   - TerminalPanel。
   - ApprovalPanel。
   - ModelConfigPanel。
   - SandboxPanel。
3. 最后抽 hooks：
   - status polling。
   - auth state。
   - model catalog loading。
   - sandbox history loading。

验收标准：

- `main.tsx` 降到 300 行以内。
- 每个 panel 可以独立阅读和测试。
- UI 行为不变。

### Phase 5：整理 `memory_engine_sdk`

目标：

- 消除双 package 和跨路径源码引用。

任务：

1. 将 `memory_engine/sdk/src/client` 和 `memory_engine/sdk/src/models` 迁入 `crates/memory_engine_sdk/src`。
2. 删除 `#[path = "../../../memory_engine/sdk/..."]`。
3. 将 `memory_engine/sdk` 改成说明文档或删除独立 crate。
4. 更新所有引用路径。
5. 跑依赖方编译：
   - `chatos/backend`
   - `task_runner_service/backend`
   - `project_management_service/backend`
   - `local_connector_client/core`
   - `crates/chatos_ai_runtime`

验收标准：

- 仓库中只有一个 `memory_engine_sdk` package。
- shared crate 源码归属清晰。
- 不再通过 `#[path]` 跨 package 引用源码。

### Phase 6：workspace 和依赖版本治理

目标：

- 明确并执行依赖版本策略。

任务：

1. 列出所有 Rust 服务当前依赖版本。
2. 决定是否统一到根 workspace。
3. 制定升级路径：
   - `axum 0.7 -> 0.8`
   - `tower-http 0.5 -> 0.6`
   - `mongodb 2.8 -> 3`
4. 增加依赖 drift 检查脚本。

验收标准：

- 有明确文档说明为什么某服务独立 workspace。
- 核心 web stack 版本不再无计划漂移。
- CI 能发现新增 drift。

## 8. 建议执行顺序

推荐顺序：

1. Phase 0：治理脚本先能跑。
2. Phase 1：抽低风险重复 helper。
3. Phase 2：统一 terminal 协议输出。
4. Phase 3：拆后端大文件。
5. Phase 4：拆前端 `main.tsx`。
6. Phase 5：整理 `memory_engine_sdk`。
7. Phase 6：workspace 和依赖版本治理。

原因：

- Phase 0 是后续所有重构的安全网。
- Phase 1 和 Phase 2 能先降低跨服务 drift。
- 大文件拆分应建立在检查可运行之后。
- SDK 和 workspace 治理影响面大，适合在行为重复收敛后做。

## 9. 每个重构 PR 的基本要求

每个 PR 应满足：

- 不混入无关格式化。
- 不重写业务逻辑，除非该 PR 明确声明。
- 保留原有 API 和 JSON 字段兼容。
- 至少跑对应模块的 check/test。
- 如果移动代码，优先“先搬迁、后改名、再抽象”，减少 review 难度。

建议验证命令：

```bash
bash scripts/code-size-report.sh --top 50
bash scripts/check-hotspot-line-budgets.sh --warn-planned
bash scripts/check-large-files.sh --fail
python scripts/check-non-test-unwrap-expect.py
cargo check
```

按服务补充：

```bash
cd chatos/frontend && npm run type-check
cd chatos/frontend && npm run test -- --run
cd task_runner_service/frontend && npm run build
cd user_service/frontend && npm run type-check
```

## 10. 风险与注意事项

- 当前工作区已有大量未提交改动，实际实施前需要确认哪些改动属于当前功能分支。
- 大文件拆分尽量避免和功能开发并行，否则冲突会很高。
- `memory_engine_sdk` 整理会影响多个 Rust crate，建议单独分支实施。
- terminal controller JSON schema 是模型工具和前端共同依赖的协议，必须加快照测试或字段级断言。
- `.gitattributes` 可能导致大量换行变更，建议单独提交，避免和逻辑重构混在一起。

## 11. 第一批可落地任务

建议先开 4 个小 PR：

1. `repo-hygiene-line-endings`
   - 新增 `.gitattributes`。
   - 修复 Python UTF-8 读取。
   - 确认 Bash 脚本可运行。

2. `shared-http-body-limit`
   - 下沉 HTTP body 限流到 `chatos_service_runtime`。
   - 替换 task runner 和 project management 的重复实现。

3. `frontend-remote-connection-types`
   - 统一 remote connection mutation/create/update payload 类型。
   - 删除 store/slice 内重复 interface。

4. `terminal-response-contract`
   - 先设计共享 terminal response builder。
   - 用一处运行环境试点替换。
   - 补 JSON 字段测试。

