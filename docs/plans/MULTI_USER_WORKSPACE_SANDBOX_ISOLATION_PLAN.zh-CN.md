# 多用户工作空间与终端沙箱隔离实施方案

## 结论

云服务里的安全边界必须以“用户拥有的工作空间”为核心，而不能以客户端传入的本机路径为核心。沙箱也不应该由“本地模式/云端模式”隐式决定：本地模式可以使用沙箱，云端模式也必须有独立的安全基线。

建议拆成两个独立概念：

- `execution_environment_mode`：描述服务运行环境或部署策略，例如 local/cloud。
- `sandbox_enabled`：描述任务里的文件写入、文件读取和终端 MCP 是否路由到沙箱 MCP。

对于正式云服务，终端和可写文件工具必须 fail-closed：没有沙箱或等价隔离运行时可用时，不允许执行会修改文件或运行命令的任务。

## 当前代码风险点

1. 前端新建项目、终端仍以路径为输入。
   - `chatos/frontend/src/components/sessionList/useLocalFsPickers.ts` 通过 `/fs/list`、`/fs/entries` 浏览目录。
   - `chatos/frontend/src/components/sessionList/useSessionListActions.ts` 创建项目传 `rootPath`，创建终端传 `cwd`。
   - `chatos/frontend/src/lib/api/client/workspace/terminals.ts` 的 `createTerminal` 和 `dispatchTerminalCommand` 都接受原始 `cwd`。

2. 后端目录授权仍会暴露过多宿主根。
   - `chatos/backend/src/api/fs/policy_roots.rs` 当前把 `current_dir`、repo parent、workspace、用户 home、`~/.ssh`、`FS_ALLOWED_ROOTS`、项目父目录都放进可访问 roots。
   - 这适合单机开发，但在云服务中会让一个用户看到宿主路径结构，甚至看到不属于自己的路径入口。

3. 项目和终端创建接口还接受任意已存在目录。
   - `chatos/backend/src/api/projects/crud_handlers.rs#create_project` 对 `root_path` 只做存在性校验。
   - `chatos/backend/src/api/terminals/crud_handlers.rs#create_terminal` 和 `dispatch_terminal_command` 对 `cwd` 只做存在性校验。

4. 已有终端 owner 校验只能保护“访问已有 terminal id”。
   - `chatos/backend/src/core/terminal_access.rs` 已有 `ensure_owned_terminal`，这是好的基础。
   - 但它不能阻止用户创建一个指向其他目录的终端，也不能阻止终端脚本访问工作区外路径。

## 目标模型

所有用户态资源都必须有稳定归属字段：

- `tenant_id`
- `user_id`
- `workspace_id`
- `project_id`
- `terminal_id`
- `sandbox_id`

服务间调用必须自动透传这些 ID，不能要求前端或 AI 手工填写。客户端传入的 `user_id` 只能作为兼容字段，后端最终必须以 `AuthUser` 解析出的用户为准。

建议的宿主目录布局：

```text
/srv/chatos/
  tenants/{tenant_id}/
    users/{user_id}/
      workspaces/{workspace_id}/
        project/
        .chatos/
      public/
      envs/
        {runtime_key}/
```

`public` 不是全局公共目录，而是每个用户自己的公共空间。可以实现为一种特殊 workspace：

```text
workspace.kind = "public"
workspace.owner_user_id = user_id
workspace.root = /srv/chatos/tenants/{tenant_id}/users/{user_id}/public
```

## API 改造方案

### 1. 工作空间 API

新增或收敛为 workspace-first API：

- `POST /api/workspaces`
  - 输入：`name`、`kind`、`git_url`、`template_id`
  - 不再接受任意绝对 `root_path`
  - 服务端生成 `workspace_id` 和真实 root
- `GET /api/workspaces`
  - 只返回当前用户拥有的 workspace
- `GET /api/workspaces/{workspace_id}`
  - 必须校验 `tenant_id + user_id + workspace_id`

项目可以继续存在，但项目必须绑定 workspace：

```text
projects.workspace_id NOT NULL
projects.owner_user_id NOT NULL
projects.tenant_id NOT NULL
projects.root_path 仅作为服务端内部派生字段
```

兼容期内可以保留 `root_path`，但云模式下创建项目时必须把传入路径限制在当前用户 workspace root 内。

### 2. 文件系统 API

把路径 API 从“宿主绝对路径”改成“workspace + 相对路径”：

- `GET /api/workspaces/{workspace_id}/fs/entries?path=src`
- `GET /api/workspaces/{workspace_id}/fs/read?path=README.md`
- `POST /api/workspaces/{workspace_id}/fs/write`
- `POST /api/workspaces/{workspace_id}/fs/mkdir`

路径策略：

- 禁止客户端传绝对路径。
- 禁止 `..`、空字节、Windows drive prefix、UNC path。
- 服务端做 `workspace_root.join(relative_path)`。
- 写入、读取、下载、压缩前必须 canonicalize。
- canonicalize 后必须 `starts_with(workspace_root)`。
- 符号链接如果指向 workspace 外，读取、列目录、压缩、写入都拒绝。
- 错误响应统一为 400/403，不泄露宿主真实路径。

现有 `/api/fs/list` 和 `/api/fs/entries` 在云服务中必须停止返回 `home`、`.ssh`、repo parent、current dir。兼容期可以只返回当前用户的 workspace 列表。

### 3. 项目创建

当前：

```text
create_project(name, root_path)
```

目标：

```text
create_project(name, workspace_id, git_url?, description?)
```

后端流程：

1. 从 `AuthUser` 得到 `tenant_id/user_id`。
2. 校验 `workspace_id` 归属当前用户。
3. 项目 root 使用 workspace root 或 workspace 内的固定子目录。
4. `ProjectService::create` 保存 `tenant_id/user_id/workspace_id/root_path`。
5. 同步 memory/project service 时带上这些归属字段。

### 4. 终端创建和命令分发

当前：

```text
create_terminal(cwd)
dispatch_command(cwd, command)
```

目标：

```text
create_terminal(workspace_id, relative_cwd?)
dispatch_command(workspace_id, relative_cwd?, command)
```

后端流程：

1. 校验 workspace 属于当前用户。
2. 把 `relative_cwd` 解析到 workspace 内。
3. 终端记录保存 `tenant_id/user_id/workspace_id/project_id/sandbox_id/runtime_id`。
4. 终端 WebSocket、历史、interrupt、delete 继续用 `ensure_owned_terminal`，并补充 workspace 归属校验。
5. AI 任务触发终端时，系统自动透传 `tenant_id/user_id/workspace_id/project_id/task_id/run_id/sandbox_id`。

云服务中终端执行必须进入隔离运行时：

- 首选：沙箱服务提供的 HTTP MCP terminal。
- 备用：每个 workspace 一个受限容器或 Kata 容器。
- 如果隔离运行时不可用，终端命令必须拒绝执行，不能退回宿主机 shell。

## 编码环境隔离

用户安装 Node、Python、Rust、Go、Java 等环境时，不能写入共享宿主目录。

环境变量建议：

```text
HOME=/home/chatos
WORKSPACE=/workspace
XDG_CACHE_HOME=/home/chatos/.cache
npm_config_prefix=/home/chatos/.local
PIP_CACHE_DIR=/home/chatos/.cache/pip
CARGO_HOME=/home/chatos/.cargo
RUSTUP_HOME=/home/chatos/.rustup
GOMODCACHE=/home/chatos/.cache/go/pkg/mod
GOCACHE=/home/chatos/.cache/go-build
```

存储策略：

- 每个用户或 workspace 独立 writable layer。
- 可选共享基础镜像和只读缓存，不能共享可写 package directory。
- 用户安装依赖只能落到 `{tenant_id}/{user_id}/envs/{runtime_key}` 或沙箱 overlay。
- 禁止挂载宿主 Docker socket。
- 禁止 privileged 容器。
- 设置 CPU、内存、进程数、磁盘、网络 egress 配额。

## 沙箱服务接入要求

沙箱租约请求必须包含：

```json
{
  "tenant_id": "...",
  "user_id": "...",
  "workspace_id": "...",
  "project_id": "...",
  "task_id": "...",
  "run_id": "...",
  "ttl_seconds": 7200
}
```

沙箱内 MCP Server 必须校验：

- `X-Chatos-Tenant-Id`
- `X-Chatos-User-Id`
- `X-Chatos-Workspace-Id`
- `X-Chatos-Sandbox-Id`
- `X-Task-Runner-Task-Id`
- `X-Task-Runner-Run-Id`

沙箱只挂载当前 workspace 副本：

```text
host: /srv/chatos/tenants/{tenant}/users/{user}/workspaces/{workspace}/.chatos/runs/{run_id}
sandbox: /workspace
```

任务结束后只允许把沙箱内 `/workspace` 的变更同步回该 run 副本，再由受控 diff/patch 流程合并回用户 workspace。不能让沙箱直接挂载多个用户目录。

## Task Runner 设置改造

已经明确拆分：

- `execution_environment_mode` 保留为本地/云端模式。
- 新增 `sandbox_enabled` 作为独立开关。

运行判断应为：

```text
sandbox_enabled && task 使用 CodeMaintainerWrite 或 TerminalController
```

不再使用：

```text
execution_environment_mode == "cloud"
```

正式云服务建议增加服务端策略：

```text
if cloud_deployment && task_has_terminal_or_write_tool && !sandbox_available:
    reject run
```

也就是说，页面开关可以控制任务路由，但云服务的安全基线不能被用户关闭。

## 分阶段实施计划

### 阶段 0：立即收口高风险入口

1. Task Runner 增加独立 `sandbox_enabled` 开关。
2. 云服务部署时禁用 `/api/fs` 对 home、`.ssh`、repo parent、current dir 的 roots 暴露。
3. `create_project`、`create_terminal`、`dispatch_terminal_command` 增加 owned workspace/path policy 校验。
4. 所有服务端接口停止信任客户端传入的 `user_id`，只允许管理员代用户操作。

### 阶段 1：引入 Workspace 数据模型

1. 新增 `workspaces` 表或集合。
2. `projects`、`terminals` 绑定 `workspace_id`。
3. 为老项目创建 workspace 记录并回填。
4. `public` 空间迁移为每个用户自己的 workspace。

### 阶段 2：文件 API workspace 化

1. 新增 `/api/workspaces/{workspace_id}/fs/*`。
2. 前端目录选择器改为选择 workspace，不再浏览宿主目录。
3. 老 `/api/fs/*` 仅保留本地开发或管理员诊断用途。

### 阶段 3：终端运行时隔离

1. 终端创建改为 `workspace_id + relative_cwd`。
2. Task Runner 和 Chat App 统一通过 sandbox/container MCP 执行终端命令。
3. 如果隔离运行时不可用，云服务直接拒绝终端任务。

### 阶段 4：编码环境隔离

1. 每个用户/workspace 独立 HOME 和包管理器目录。
2. 共享基础镜像只读化。
3. 加 CPU、内存、磁盘、网络、进程数配额。
4. 记录环境安装审计日志。

### 阶段 5：审计和验收

审计事件必须包含：

- `tenant_id`
- `user_id`
- `workspace_id`
- `terminal_id`
- `sandbox_id`
- `task_id`
- `run_id`
- 操作类型
- 路径或命令摘要
- allow/deny 结果

验收用例：

1. 用户 A 不能在目录选择器看到用户 B 的 workspace。
2. 用户 A 传用户 B 的 `workspace_id` 创建终端，返回 403。
3. 用户 A 传 `/etc`、`/Users`、`~/.ssh`、绝对路径，返回 400/403。
4. workspace 内 symlink 指向外部目录时，读取、下载、终端 cwd 都被拒绝。
5. 终端脚本 `cd ..`、写 `../x`、读 `/etc/passwd`，在沙箱或容器内不能影响宿主和其他用户。
6. 用户 A 安装 npm/pip/cargo 依赖后，用户 B 的环境目录不出现任何新增文件。
7. 沙箱 MCP 请求缺少或伪造 `sandbox_id/user_id/workspace_id`，返回 401/403。
8. 任务 run 记录能追踪到 `sandbox_enabled`、`sandbox_id`、`workspace_id`。

## 推荐优先级

优先做：

1. 独立 `sandbox_enabled` 开关。
2. 云服务禁用宿主目录枚举。
3. workspace 数据模型。
4. 终端从 `cwd` 改为 `workspace_id + relative_cwd`。
5. 沙箱 MCP 强制 ID 透传和校验。

这几项完成后，用户数据隔离才从“路径约束”升级为“资源归属 + 运行时隔离”的安全模型。
