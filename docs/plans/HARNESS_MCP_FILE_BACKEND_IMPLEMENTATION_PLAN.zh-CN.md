# Harness 文件读写 MCP 后端实施方案

更新日期：2026-07-08

## 背景

云端项目创建后，项目代码已经导入到 Harness 仓库。当前内置 `CodeMaintainerRead` / `CodeMaintainerWrite` 文件读写 MCP 主要依赖本机 `workspace_dir` 文件系统；Local Connector 通过本地 MCP relay 覆盖本地项目；Sandbox 通过 sandbox 内的 MCP server 覆盖沙箱文件系统。

目标是把内置文件读写 MCP 明确做成三类项目文件后端：

1. Sandbox：读写沙箱容器内 workspace。
2. Local Connector：读写用户本机授权 workspace。
3. Harness：读写云端项目对应的 Harness repo。

这样云端项目执行 Task Runner 时，不再要求 Task Runner 主机上存在项目目录，而是直接以 Harness repo 作为项目文件系统。

## 已确认的现状

### Chatos 侧

- `crates/chatos_builtin_tools/src/code_maintainer` 当前把文件能力封装在 `FsOps`，对本地目录做 `read_file_raw`、`read_file_range`、`list_dir`、`search_text`、`write_file`、`edit_file`、`append_file`、`delete_path`、`apply_patch`。
- Task Runner builtin provider 通过 `build_shared_builtin_tool_service` 为 `CodeMaintainerRead` / `CodeMaintainerWrite` 创建 `CodeMaintainerService`，输入是 `McpBuiltinServer.workspace_dir`。
- Local Connector 已经用 ephemeral HTTP MCP server 替代服务器本机的 CodeMaintainer / Terminal / Browser 能力。
- Project Service 的项目记录已经包含 Harness 元数据：`harness_space_identifier`、`harness_repo_identifier`、`harness_repo_path`、`harness_git_url`、`harness_git_ssh_url`。

### Harness 侧

Harness repo 已有可复用接口：

- `GET /api/v1/repos/{repo_ref}/content/*`：读取目录或文件 metadata/content，文件内容 base64 返回。
- `GET /api/v1/repos/{repo_ref}/raw/*`：读取 raw 文件内容。
- `GET /api/v1/repos/{repo_ref}/paths?include_directories=true`：列出 repo paths。
- `POST /api/v1/repos/{repo_ref}/commits`：通过 `CREATE`、`UPDATE`、`DELETE`、`MOVE`、`PATCH_TEXT` action 提交文件变更。

第一期可以不改 Harness 核心接口，直接用这些 API 完成 Harness backend。后续如果性能或语义不够，再给 Harness 增补批量读/搜索/临时工作区接口。

## 总体设计

新增一个“文件工作区后端”抽象，让 CodeMaintainer 工具名和 JSON schema 保持不变，后端按项目来源切换：

```text
CodeMaintainer MCP tools
  -> CodeMaintainerService
    -> CodeWorkspaceBackend trait
      -> FsWorkspaceBackend        (Sandbox / server-local fallback)
      -> LocalConnectorMcpBackend  (现有 relay，可作为边界说明)
      -> HarnessRepoBackend        (新增)
```

重点是对外仍暴露同一组 MCP 工具，模型无需知道底层是沙箱、本机还是 Harness。

## 后端选择规则

1. 如果项目 root 是 `local://connector/...`，继续走 Local Connector ephemeral MCP。
2. 如果任务启用了 sandbox 且有 sandbox manager base url，文件读写走 Sandbox MCP。
3. 如果项目记录有 `harness_repo_path` 且 `import_status = ready`，云端项目走 HarnessRepoBackend。
4. 只有没有以上路由时，才保留当前 `workspace_dir` 本地文件系统 fallback。

在 Task Runner 侧，建议在 `resolve_project_root_for_task` / builtin registry 构建前额外加载完整 project record，而不是只拿 `root_path`。这样可以基于 Harness metadata 生成正确的 `McpBuiltinServer` 或 provider options。

## HarnessRepoBackend 第一版能力映射

### 读能力

- `list_dir`
  - 调 `GET /content/{path}?git_ref={branch}`。
  - 如果返回 `dir`，把 `entries` 转成现有 `FileEntry` 格式。
  - 如果需要递归搜索前的路径枚举，可调 `/paths?include_directories=true`。

- `read_file_raw`
  - 调 `GET /content/{path}?git_ref={branch}` 或 `/raw/{path}?git_ref={branch}`。
  - 使用 Harness 返回的 blob sha 作为 `sha256` 字段的兼容值时要改名或补字段：建议返回 `sha256` 继续用内容 sha256，同时额外返回 `harness_blob_sha`。
  - 保持 UTF-8 与二进制拒绝策略。

- `read_file_range`
  - 第一版直接读取完整文件后在 Chatos 侧切行。
  - 保持 `max_file_bytes` 限制，避免通过 Harness 拉超大文件。

- `search_text`
  - 第一版用 `/paths` 枚举文件，再按文件读取并在 Chatos 侧搜索，设置文件数、总字节、超时保护。
  - 第二版建议在 Harness 增补 repo grep/search API，否则大仓库性能不可控。

### 写能力

- `write_file`
  - 先读目标文件确认是否存在和当前 blob sha。
  - 存在则 `UPDATE`，不存在则 `CREATE`。
  - 调 `POST /commits`，提交 title 如 `Chatos: write {path}`。
  - 对 update 带上 Harness `sha` 做乐观锁。

- `append_file`
  - 先读完整文件，在 Chatos 侧拼接，再走 `UPDATE` / `CREATE`。

- `edit_file`
  - 先读完整文件，复用当前 `apply_edit_text` 逻辑生成新内容，再走 `UPDATE`。

- `delete_path`
  - 文件删除走 `DELETE` action。
  - 目录删除 Harness commit API 不一定直接支持目录递归；第一期可以拒绝目录删除并提示先列出文件后逐文件删除，或在 Chatos 侧展开目录成多个 `DELETE` action。

- `apply_patch`
  - 第一版在 Chatos 侧解析 patch，逐文件读取、应用、生成一批 `CREATE` / `UPDATE` / `DELETE` actions，一次 `/commits` 提交。
  - 如果 Harness `PATCH_TEXT` 能满足当前 patch 格式，可以第二期切到 Harness 原生 patch action。

## 认证与安全

当前 Project Service 只保存 Harness repo metadata，真正的 Harness access token 在 user_service 的 Harness provisioning 记录里。因此第一期需要补一个安全取 token/代理方案，二选一：

1. 推荐：Project Service 增加内部 Harness repo file proxy。
   - Task Runner 调 Project Service 内部接口。
   - Project Service 通过 user_service internal secret 换取或代理用户 Harness token。
   - 优点是 Task Runner 不直接接触 Harness token，权限边界集中。

2. 备选：user_service 增加 internal token exchange。
   - Task Runner 传 `project_id` / `owner_user_id`，用内部 secret 换取短期 Harness API token。
   - 优点实现短；缺点 token 扩散到 Task Runner，审计和泄露面更大。

所有路径都必须保留：

- repo path 只能来自项目记录的 `harness_repo_path`，不能信任模型传入 repo。
- file path 必须拒绝绝对路径、`..`、空 path 绕过。
- 写操作必须要求 `CodeMaintainerWrite` 被选择，且保留 Harness repo permission / branch protection 校验。
- commit author 建议使用当前 Chatos 用户或 Task Runner agent，committer 使用系统服务账号。

## 需要改动的 Chatos 模块

1. `crates/chatos_builtin_tools/src/code_maintainer`
   - 抽出 backend trait，拆掉工具注册对 `FsOps` 的直接依赖。
   - 保留 `FsOps` 为 `FsWorkspaceBackend`。
   - 让 read/write registration 调 backend 方法，而不是直接调本地文件系统。
   - 由于 Harness HTTP 是 async，`CodeMaintainerService::call_tool` 需要升级成 async，或新增 `AsyncCodeMaintainerService` 并让 Task Runner 先使用 async provider。

2. `task_runner_service/backend`
   - 扩展 project record client，读取 Harness metadata。
   - 新增 `HarnessRepoBackend` 配置：base url、repo ref、branch、token/proxy client、limits。
   - 在 builtin provider 构建时识别 Harness 项目并创建 Harness backend。
   - 对 `runtime_selected_builtin_kinds` 保持工具选择逻辑不变，只改变 CodeMaintainer 的 host backend。

3. `project_management_service/backend`
   - 建议新增内部接口：读取 content/raw/paths、提交 commits。
   - 负责从 user_service 获取 Harness token，或以当前用户 token 调 user_service 代理。
   - 对 Harness repo metadata、import ready 状态和 owner 权限做统一校验。

4. `user_service/backend`
   - 如果采用 proxy 方案，需要新增内部接口，让 Project Service 能以内部 secret 获取用户 Harness API token或直接代理请求。
   - token 不落库到 Project Service / Task Runner。

5. `harness`
   - 第一期不必改。
   - 第二期候选增强：repo grep API、batch read API、批量 patch API、目录递归删除 action、临时分支/事务 API。

## 分阶段落地

### 当前实施进度

#### 已落地

- user_service 新增内部 Harness API access 获取能力：Project Service 可通过 internal secret 按 owner user id 获取 Harness base url / access token；token 不落到 Task Runner。
- Task Runner 项目模型和 Project Service client 已透传 Harness repo metadata。
- Task Runner 启动 run 前会识别 `import_status = ready` 且带 `harness_repo_path` 的云端项目，并把 `CodeMaintainerRead` / `CodeMaintainerWrite` 从服务器本机 builtin 移到 `harness_code` ephemeral HTTP MCP server。
- Project Service 新增 `/api/chatos-sync/projects/:project_id/harness/mcp` JSON-RPC MCP endpoint，复用 `x-project-service-sync-secret` 鉴权，并校验 `x-task-runner-project-id` 与 URL 项目一致。
- Harness MCP 文件工具已实现：`read_file_raw`、`read_file_range`、`list_dir`、`search_text`、`write_file`、`edit_file`、`append_file`、`delete_path`、`apply_patch`。
- `delete_path` 已支持目录递归删除，会把目录下 tracked files 展开为一次 Harness commit 内的多条 `DELETE` action。
- 写入 commit 已改为不传 `branch` 字段，由 Harness 使用 repo 默认分支。
- Task Runner 已在系统提示中说明云端项目文件应使用 `harness_code_*` 工具，避免模型误用服务器本机 CodeMaintainer。
- 旧 `sandbox_filesystem_*` / `sandbox_terminal_*` Task Runner 配置保存值已在 chat_app_server 侧归一化到标准 builtin kind。

#### 尚未落地

- 真实 Harness 环境端到端 smoke，尤其是 zip/git 导入后的 `list_dir`、`search_text`、`write_file`、`edit_file`、`delete_path`、`apply_patch`。
- per-task branch + PR 策略；当前第一阶段写入会直接调用 Harness commit API。

### Phase 1：最小闭环

- 新增 Harness 项目识别与 backend 配置。
- 实现 `list_dir`、`read_file_raw`、`read_file_range`。
- 写能力先实现 `write_file`、`edit_file`、`append_file`、单文件 `delete_path`。
- `search_text` 用保守枚举实现，限制扫描文件数和总读取字节。
- 云端项目创建执行任务时，CodeMaintainer 走 Harness，不再要求本地 `workspace_dir` 存在。

### Phase 2：补齐 patch 与批量变更

- `apply_patch` 在 Chatos 侧生成一次 Harness commit。[已完成]
- 目录删除展开为批量 `DELETE`。[已完成]
- change log 记录 Harness commit id、branch、changed files。
- UI/回调里展示“变更已提交到 Harness repo”。

### Phase 3：Harness 原生增强

- 在 Harness 增加 repo search/grep API，替换 Chatos 侧逐文件扫描。
- 增加 batch content API，降低 read range / patch 的 N+1 请求。
- 评估是否需要 Chatos 专用 MCP endpoint 放在 Harness 内部，这样 Harness 可以成为标准 MCP host。

## 验证清单

- 云端 zip 项目导入 Harness 后，Task Runner 执行 `read_file_raw` 能读到仓库文件。
- 云端 git_url 项目导入 Harness 后，`list_dir` 和 `search_text` 能返回默认分支内容。
- `write_file` 对新文件创建 commit，Harness repo 页面可见。
- `edit_file` 在文件变更后带 sha 乐观锁，冲突时提示重新读取。
- Local Connector 项目仍走 local relay，不被 Harness 路由误判。
- Sandbox 任务仍走 sandbox MCP，不被 Harness 路由误判。
- 没有 Harness metadata 的普通任务继续使用本地 `workspace_dir` fallback。

## 与 MCP 工具整合 P1/P2 的关系

- P1 旧 Task Runner 配置迁移已完成：旧 `sandbox_filesystem_*` / `sandbox_terminal_*` alias 会在 chat_app_server 侧迁移到标准 builtin kind。
- P2 的 `chatos_mcp_service` policy / 权限模块可以顺手承接 host capability policy：同一工具名在 Sandbox、Local Connector、Harness 下有不同 backend，但 read/write 权限分类应该统一。
- Local Connector catalog 边界梳理时，应把“host backend 选择”和“MCP 工具 catalog 暴露”分开，避免以后 Harness 又复制一套工具名判断。

## 风险和待决策

1. Harness token 方案需要先定：推荐 Project Service 代理，避免 Task Runner 持有 Harness token。
2. `CodeMaintainerService` 当前是同步调用；Harness backend 会迫使它异步化，改动面需要控制。
3. Harness commit API 是“提交变更”语义，不是“工作区未提交文件”语义；MCP 写入会立即产生 commit，需要产品侧接受。
4. `search_text` 第一版性能受限，大仓库必须限制扫描规模或等待 Harness 原生 search。
5. 分支策略需要定：第一阶段由 Harness 选择 repo 默认分支；生产版是否为每次 Task Runner run 创建 `chatos/{task_id}` 分支后再 PR。

## 推荐默认决策

- 第一版请求体省略 `branch` 字段，让 Harness 写 repo 默认分支，并严格遵守 Harness branch protection；如果保护规则拒绝，则返回可读错误。
- 生产版再切到 per-task branch + PR，以降低自动写主分支风险。
- token 走 Project Service 内部 proxy。
- Harness 不新增接口先跑通最小闭环；搜索和批量 patch 作为第二期优化。
