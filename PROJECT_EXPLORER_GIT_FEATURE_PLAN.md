# Project Explorer Git 功能实现方案

## 0. 目标

在项目浏览器顶部右侧空白区域增加 Git 入口，位置对应截图红框：

- 左侧仍然是现有 `项目目录 / 团队成员` tab。
- 右侧新增一个 Git 分支按钮，例如 `main ▾`、`feature/foo ▾`、`无 Git 仓库`。
- 点击按钮后弹出类似 JetBrains 的 Git 菜单，支持搜索分支和常用动作。
- 首期先做安全、稳定、可解释的 Git 工作流，不直接做危险操作的“一键强制覆盖”。

这个功能应服务于当前“项目”维度，即以 `project.rootPath` 作为 Git repository worktree 根目录或子目录入口。

## 1. 当前代码结构判断

### 前端入口

当前项目浏览器主入口：

- `chat_app/src/components/ProjectExplorer.tsx`
- 顶部 tab 组件：`chat_app/src/components/projectExplorer/WorkspaceTabs.tsx`
- 文件工作区：`chat_app/src/components/projectExplorer/ProjectExplorerFilesWorkspace.tsx`
- 文件树：`chat_app/src/components/projectExplorer/TreePane.tsx`
- 预览区：`chat_app/src/components/projectExplorer/PreviewPane.tsx`

目前 `WorkspaceTabs` 的结构是：

- 外层：`border-b border-border bg-card px-3 py-2`
- 内层：左侧 inline tab group
- 没有右侧 action slot

因此最自然的改造方式是：

- 给 `WorkspaceTabs` 增加 `rightActions?: React.ReactNode`
- 外层改成 `flex items-center justify-between`
- `ProjectExplorer.tsx` 把 Git 入口作为 `rightActions` 传进去

这样不会污染文件树，也不会挤占预览区空间。

### 前端 API 组织

当前 API client 结构：

- `chat_app/src/lib/api/client.ts`
- `chat_app/src/lib/api/client/workspace.ts`
- `chat_app/src/lib/api/client/types.ts`
- `chat_app/src/lib/api/client/facades/workspaceFacade.ts`

现有 fs/code-nav/project-run 都是通过 client facade 暴露给组件。Git 建议沿用这个模式：

- 类型放到 `client/types.ts`
- 请求函数放到 `client/git.ts` 或追加到 `client/workspace.ts`
- facade 增加 `gitFacade.ts` 或合并到 `workspaceFacade.ts`
- `ApiClient` 通过 `Object.assign` 混入 Git 方法

### 后端 API 组织

当前后端 API 入口：

- `chat_app_server_rs/src/api/mod.rs`
- `chat_app_server_rs/src/api/fs.rs`
- `chat_app_server_rs/src/api/code_nav.rs`

后端服务模块入口：

- `chat_app_server_rs/src/services/mod.rs`
- 现有服务类似 `code_nav`、`workspace_search`、`project_run`

Git 功能建议新增独立模块：

- `chat_app_server_rs/src/api/git.rs`
- `chat_app_server_rs/src/services/git/`
- 在 `api/mod.rs` 中 `pub mod git;` 并 `.merge(git::router())`
- 在 `services/mod.rs` 中 `pub mod git;`

## 2. 产品形态

### 顶部入口

放在 `WorkspaceTabs` 右侧，显示当前 Git 状态：

- Git repo 正常：`当前分支名 ▾`
- detached HEAD：`detached: abc1234 ▾`
- 非 Git 仓库：`无 Git 仓库`
- 加载中：`Git 检查中...`
- 有本地变更：分支旁显示小圆点或 `+N`
- ahead/behind：显示 `↑2 ↓1`

建议样式：

- 小按钮，不超过 180px 宽
- 与当前 tab 高度一致
- 不默认展开，点击才弹出菜单

### 弹层结构

弹层宽度建议 360-420px，高度最多 70vh，内容如下：

1. 搜索框

   搜索 branches/actions，类似截图里的 `Search for branches and actions`。

2. 常用动作区

   - `Fetch`
   - `Pull...`
   - `Commit...`
   - `Push...`
   - `New Branch...`

3. 当前分支区

   - 当前分支名
   - upstream
   - ahead/behind
   - worktree dirty summary

4. Recent Branches

   - 最近 checkout 的分支
   - 当前分支置顶
   - 每行展示 local branch、upstream、ahead/behind

5. Local / Remote 分组

   - Local branches
   - Remote branches
   - 点击分支打开二级菜单或 inline actions

### 分支行菜单

首期支持：

- `Checkout`
- `New Branch from ...`
- `Compare with current`
- `Merge ... into current`
- `Pull/Rebase current onto ...` 作为二期
- `Delete` 作为二期，并且必须二次确认

首期不建议直接做 `reset --hard`、`force push`、`delete remote branch`。

## 3. 首期 MVP 范围

### 必做

- 检测当前项目是否在 Git worktree 中
- 获取当前分支、HEAD、upstream、ahead/behind
- 获取工作区状态统计
- 列出 local branches
- 列出 remote branches
- 搜索 branches/actions
- Checkout local branch
- Checkout remote branch 时创建 tracking branch
- New Branch
- Fetch
- Pull
- Push
- 打开 Commit 弹窗，选择文件并输入 message 后 commit

### 可延后

- 分支删除
- rebase/merge
- conflict 可视化解决
- 文件 diff viewer
- stash 管理
- tag checkout
- worktree 创建
- force push

## 4. 后端设计

### 为什么首期建议用 Git CLI

后端 Rust 可以选择两条路：

- `git2` crate
- 系统 `git` 命令

首期建议使用系统 `git` 命令，原因：

- 更贴近用户本地 Git 行为和 credential helper
- 支持 worktree、remote、submodule 等真实场景更完整
- 不需要额外处理 libgit2 与系统认证差异
- 项目已有 `tokio` full，可用 `tokio::process::Command`

执行命令必须遵守：

- 不走 shell，全部使用 `Command::new(<resolved_git_bin>).args([...])`
- 使用 `git -C <root>`，root 必须校验在项目根目录下
- 设置超时，默认 20s，fetch/pull/push 可 120s
- 设置 `GIT_TERMINAL_PROMPT=0`，避免后端进程卡在交互式认证
- 返回 stdout/stderr 摘要，stderr 不直接全量暴露敏感信息

### Git 客户端来源

不能只假设用户机器已经安装系统 Git。后端 Git 执行层按以下顺序解析 Git binary：

1. `CHATOS_GIT_BIN` 环境变量，允许部署或桌面壳显式指定 Git 可执行文件。
2. 安装包资源目录里的内置 Git，例如 `resources/git/bin/git`、`vendor/git/bin/git`、macOS `.app` 常见的 `Contents/Resources/git/bin/git`。
3. 最后 fallback 到系统 `git`。

当解析到内置 Git 的可执行文件路径时，执行层会把该 Git 所在目录及常见 helper 目录临时 prepend 到子进程 `PATH`，例如 `libexec/git-core`、`mingw64/libexec/git-core`、`usr/bin`、`cmd`，降低 helper/credential 子程序找不到的概率。

当前代码已经完成 Git binary 解析和错误提示；真正把 Git 二进制随安装包分发需要在打包阶段处理。建议后续按平台放置：

- macOS: `Contents/Resources/git/bin/git`
- Linux: `resources/git/bin/git`
- Windows: `resources/git/bin/git.exe`

注意事项：

- 内置 Git 需要同时带上运行所需的 helper、template、ssl/curl 相关依赖，不能只拷贝单个 `git` 可执行文件。
- Git 本身是 GPLv2，需要确认分发方式和许可证声明。
- 如果用户配置了自己的 Git，优先使用 `CHATOS_GIT_BIN`，方便排查企业环境、代理和 credential helper 差异。

### 路径安全

每个 API 请求都带 `root` 或使用 `projectRoot`：

- 后端检查 root 存在且是目录
- 使用 `git -C root rev-parse --show-toplevel` 得到真实 repo root
- repo root 必须是 root 本身或 root 的父目录
- 所有文件 path 只允许 repo root 内相对路径
- commit/stage 等操作只接受相对路径，禁止绝对路径和 `..`

### API 草案

#### `GET /api/git/summary?root=...`

用于顶部按钮和弹层初始加载。

返回：

```json
{
  "isRepo": true,
  "root": "/abs/repo",
  "worktreeRoot": "/abs/repo",
  "head": "abc1234",
  "currentBranch": "main",
  "detached": false,
  "upstream": "origin/main",
  "ahead": 1,
  "behind": 2,
  "dirty": true,
  "changes": {
    "staged": 1,
    "unstaged": 3,
    "untracked": 2,
    "conflicted": 0
  }
}
```

实现命令：

- `git -C root rev-parse --show-toplevel`
- `git -C root rev-parse --abbrev-ref HEAD`
- `git -C root rev-parse --short HEAD`
- `git -C root status --porcelain=v2 --branch`

#### `GET /api/git/branches?root=...`

返回 local/remote branches。

返回：

```json
{
  "current": "main",
  "locals": [
    {
      "name": "main",
      "current": true,
      "upstream": "origin/main",
      "ahead": 1,
      "behind": 0,
      "lastCommit": "abc1234",
      "lastCommitSubject": "feat: xxx"
    }
  ],
  "remotes": [
    {
      "remote": "origin",
      "name": "origin/dev",
      "shortName": "dev",
      "trackedBy": null,
      "lastCommit": "def5678",
      "lastCommitSubject": "fix: yyy"
    }
  ]
}
```

实现命令：

- local: `git for-each-ref refs/heads --format=...`
- remote: `git for-each-ref refs/remotes --format=...`
- upstream/ahead/behind 可通过 `%(upstream:short)` 和后续 `rev-list --left-right --count`

#### `GET /api/git/status?root=...`

给 commit 弹窗和未来 changed files 使用。

返回：

```json
{
  "files": [
    {
      "path": "src/App.tsx",
      "oldPath": null,
      "status": "modified",
      "staged": false,
      "conflicted": false
    }
  ]
}
```

实现命令：

- `git status --porcelain=v2 -z`
- 用 `-z` 是为了可靠处理空格和中文路径

#### `POST /api/git/fetch`

请求：

```json
{
  "root": "/abs/project",
  "remote": "origin"
}
```

执行：

- `git fetch --prune <remote>`

#### `POST /api/git/pull`

请求：

```json
{
  "root": "/abs/project",
  "mode": "ff-only"
}
```

首期默认 `ff-only`，避免自动 merge 产生复杂冲突。

执行：

- `git pull --ff-only`

如果失败，返回明确错误，前端提示用户可去终端处理。

#### `POST /api/git/push`

请求：

```json
{
  "root": "/abs/project",
  "remote": "origin",
  "branch": "main",
  "setUpstream": false
}
```

执行：

- 普通：`git push <remote> <branch>`
- 首次 upstream：`git push -u <remote> <branch>`

#### `POST /api/git/checkout`

请求：

```json
{
  "root": "/abs/project",
  "branch": "dev",
  "remoteBranch": null,
  "createTracking": false
}
```

执行前：

- 检查 working tree 是否有冲突
- 如果 dirty，前端要弹确认：切换分支可能失败或影响未提交改动

执行：

- local: `git checkout <branch>`
- remote tracking: `git checkout -b <shortName> --track <remoteBranch>`

#### `POST /api/git/branch`

请求：

```json
{
  "root": "/abs/project",
  "name": "feature/foo",
  "startPoint": "main",
  "checkout": true
}
```

执行：

- checkout: `git checkout -b <name> <startPoint>`
- no checkout: `git branch <name> <startPoint>`

分支名后端必须校验：

- 非空
- 不含空白控制字符
- 不以 `-` 开头
- 通过 `git check-ref-format --branch <name>`

#### `POST /api/git/stage`

请求：

```json
{
  "root": "/abs/project",
  "paths": ["src/App.tsx"]
}
```

执行：

- `git add -- <paths...>`

#### `POST /api/git/unstage`

执行：

- `git restore --staged -- <paths...>`

#### `POST /api/git/commit`

请求：

```json
{
  "root": "/abs/project",
  "message": "feat: add git panel",
  "paths": ["src/App.tsx"]
}
```

流程：

- 如果传了 paths，先 `git add -- <paths...>`
- `git commit -m <message>`
- message 必须 trim 后非空

### 后端结构建议

```text
chat_app_server_rs/src/api/git.rs
chat_app_server_rs/src/services/git/mod.rs
chat_app_server_rs/src/services/git/contracts.rs
chat_app_server_rs/src/services/git/runner.rs
chat_app_server_rs/src/services/git/parser.rs
chat_app_server_rs/src/services/git/service.rs
```

职责：

- `api/git.rs`: axum router、请求/响应状态码映射
- `contracts.rs`: request/response DTO
- `runner.rs`: 安全执行 `git -C root ...`，统一 timeout/stdout/stderr
- `parser.rs`: 解析 porcelain v2、branch refs
- `service.rs`: 组合业务逻辑

## 5. 前端设计

### 新增组件

```text
chat_app/src/components/projectExplorer/git/GitBranchButton.tsx
chat_app/src/components/projectExplorer/git/GitBranchPopover.tsx
chat_app/src/components/projectExplorer/git/GitBranchList.tsx
chat_app/src/components/projectExplorer/git/GitActionRows.tsx
chat_app/src/components/projectExplorer/git/GitCommitDialog.tsx
chat_app/src/components/projectExplorer/git/useProjectGit.ts
```

### `WorkspaceTabs` 改造

新增 props：

```ts
interface WorkspaceTabsProps {
  activeTab: WorkspaceTab;
  onChange: (tab: WorkspaceTab) => void;
  rightActions?: React.ReactNode;
}
```

渲染结构：

```tsx
<div className="border-b border-border bg-card px-3 py-2">
  <div className="flex items-center justify-between gap-3">
    <div className="inline-flex ...">tabs</div>
    <div className="min-w-0 shrink-0">{rightActions}</div>
  </div>
</div>
```

`ProjectExplorer.tsx`：

```tsx
<WorkspaceTabs
  activeTab={workspaceTab}
  onChange={setWorkspaceTab}
  rightActions={<GitBranchButton projectRoot={project.rootPath} />}
/>
```

### `useProjectGit` 状态

负责：

- 加载 `summary`
- 点击展开时加载 branches/status
- action 成功后刷新 summary/branches/status
- project root 变化时重置状态
- 定时轻量刷新 summary，建议 10-15 秒
- 窗口获得焦点时刷新 summary

建议状态：

```ts
interface ProjectGitState {
  summary: GitSummary | null;
  branches: GitBranches | null;
  status: GitStatus | null;
  loadingSummary: boolean;
  loadingBranches: boolean;
  actionLoading: boolean;
  error: string | null;
}
```

### 交互细节

#### 打开弹层

- 如果不是 Git repo：显示 `当前项目不是 Git 仓库` 和 `刷新`。
- 如果是 Git repo：展示 search + actions + branches。
- 默认 focus 搜索框。

#### 搜索

搜索结果合并：

- action rows
- local branches
- remote branches

例如输入 `push` 只展示 `Push...`，输入 `dev` 展示包含 dev 的分支。

#### Checkout

点击分支：

- 如果分支就是当前分支：关闭弹层或展示 disabled
- 如果 dirty：弹确认
- 如果 checkout 成功：
  - 刷新 Git summary
  - 刷新文件树根目录
  - 清空当前选中文件或尝试重新读取当前路径
  - 清理 code-nav/search 结果，因为文件内容可能变化

#### Commit

点击 `Commit...` 打开 dialog：

- 顶部输入 message
- 文件列表支持勾选
- staged/unstaged/untracked 分类
- 默认勾选所有 unstaged/untracked 或默认不勾选，需要产品决策

建议首期默认：

- 全部未 staged 文件默认不勾选
- 用户必须明确选择文件
- 如果已有 staged 文件，显示 staged 并默认参与 commit

## 6. 类型设计

前端 `chat_app/src/types/index.ts` 增加：

```ts
export interface GitSummary {
  isRepo: boolean;
  root?: string;
  worktreeRoot?: string;
  head?: string | null;
  currentBranch?: string | null;
  detached: boolean;
  upstream?: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
  changes: GitChangeCounts;
}

export interface GitChangeCounts {
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
}

export interface GitBranchInfo {
  name: string;
  shortName?: string;
  current?: boolean;
  upstream?: string | null;
  remote?: string | null;
  trackedBy?: string | null;
  ahead?: number;
  behind?: number;
  lastCommit?: string | null;
  lastCommitSubject?: string | null;
}

export interface GitBranchesResult {
  current?: string | null;
  locals: GitBranchInfo[];
  remotes: GitBranchInfo[];
}

export interface GitStatusFile {
  path: string;
  oldPath?: string | null;
  status: 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'untracked' | 'conflicted' | string;
  staged: boolean;
  conflicted: boolean;
}
```

## 7. 与现有功能的关系

### 与“变更记录”

当前项目浏览器右侧有“变更记录”，它来自内部 change log，不等同 Git status。

Git status 不应该直接替换现有变更记录。建议：

- 首期 Git 弹层内展示 Git status
- 后续可把文件树的 Git modified/untracked 标记融合到 `aggregatedChangeKindByPath`
- 但内部 change log 的 `新增/编辑/删除` 仍然保留，因为它代表 agent/工具侧待确认变更

### 与代码导航缓存

Checkout / Pull / Merge 可能改变大量文件，因此 Git action 成功后应触发：

- 文件树刷新
- 当前文件重新读取或清空
- 搜索结果清理
- code-nav token/nav result 清理

后端符号索引现在已有基于 mtime 的快照失效，因此即使不显式通知也能刷新；但前端应清掉旧导航结果，避免展示旧文件位置。

### 与终端

远程操作可能因认证失败。首期 API 设置 `GIT_TERMINAL_PROMPT=0`：

- 如果 credential helper 已配置，fetch/push/pull 正常工作
- 如果需要输入密码，API 返回认证失败
- 前端提示用户到终端完成登录或配置 token/SSH key

后续可提供“在终端中运行该 Git 命令”。

## 8. 风险和保护

### 必须保护

- 不允许 shell 拼接命令
- 不允许任意路径 Git 操作
- destructive 操作默认不做
- checkout/pull 前提示 dirty 状态
- pull 首期默认 `--ff-only`
- push 不支持 force
- delete branch、reset、clean 放到二期，并必须二次确认

### 常见错误处理

- `git` 命令不存在：提示安装 Git
- 非 Git 仓库：按钮显示 `无 Git 仓库`
- detached HEAD：允许查看状态，checkout branch
- rebase/merge 进行中：summary 返回 `operationState`
- conflict：禁用 checkout/pull，提示先解决冲突
- remote auth 失败：展示 stderr 摘要
- branch 名非法：前后端都校验

## 9. 分期计划

### Phase 1: Git 只读能力和顶部入口

后端：

- `/api/git/summary`
- `/api/git/branches`
- `/api/git/status`
- Git runner/parser 单元测试

前端：

- `WorkspaceTabs.rightActions`
- `GitBranchButton`
- `GitBranchPopover`
- 当前分支、dirty、ahead/behind 展示
- branch/action 搜索

验收：

- Git 仓库显示当前分支
- 非 Git 仓库显示无 Git
- 能列出 local/remote branches
- dirty 状态和 ahead/behind 正确

### Phase 2: 安全写操作

后端：

- fetch
- checkout
- new branch
- pull ff-only
- push non-force

前端：

- 操作按钮
- dirty checkout 确认
- action loading/error/message
- 成功后刷新文件树和 Git 状态

验收：

- checkout local branch 后顶部分支更新
- checkout remote branch 能创建 tracking branch
- fetch 后 remote branches 更新
- pull/push 成功或失败都有明确提示

### Phase 3: Commit 流程

后端：

- status porcelain v2 完整解析
- stage/unstage
- commit

前端：

- Commit dialog
- 文件选择
- staged/unstaged/untracked 分组
- commit message 校验

验收：

- 选择文件并 commit 成功
- commit 后 dirty count 更新
- 空 message/空选择有明确提示

### Phase 4: 增强能力

- compare branch
- changed file diff
- stash
- merge/rebase
- conflict 状态面板
- delete branch
- terminal fallback

## 10. 测试策略

### 后端单元测试

建议用临时目录初始化真实 git repo：

- `git init`
- 创建 commit
- 创建 branch
- 修改文件生成 dirty status
- 添加 remote 可用本地 bare repo 模拟

覆盖：

- 非 repo summary
- branch list parser
- status porcelain v2 parser
- checkout branch
- create branch
- commit
- invalid path 被拒绝
- invalid branch name 被拒绝

### 前端测试

如果当前测试栈允许：

- `GitBranchButton` summary 渲染
- `GitBranchPopover` 搜索过滤
- action loading disabled
- 非 repo 状态
- dirty checkout confirm

### 手工验证

- 普通 Git repo
- detached HEAD
- 无 upstream branch
- ahead/behind
- untracked/modified/deleted/renamed
- remote auth 失败
- 非 Git 项目目录

## 11. 推荐文件改动清单

首期预计新增/修改：

```text
chat_app_server_rs/src/api/git.rs
chat_app_server_rs/src/services/git/mod.rs
chat_app_server_rs/src/services/git/contracts.rs
chat_app_server_rs/src/services/git/runner.rs
chat_app_server_rs/src/services/git/parser.rs
chat_app_server_rs/src/services/git/service.rs
chat_app_server_rs/src/api/mod.rs
chat_app_server_rs/src/services/mod.rs

chat_app/src/lib/api/client/git.ts
chat_app/src/lib/api/client/facades/gitFacade.ts
chat_app/src/lib/api/client/types.ts
chat_app/src/lib/api/client.ts
chat_app/src/types/index.ts

chat_app/src/components/projectExplorer/WorkspaceTabs.tsx
chat_app/src/components/ProjectExplorer.tsx
chat_app/src/components/projectExplorer/git/GitBranchButton.tsx
chat_app/src/components/projectExplorer/git/GitBranchPopover.tsx
chat_app/src/components/projectExplorer/git/GitBranchList.tsx
chat_app/src/components/projectExplorer/git/GitActionRows.tsx
chat_app/src/components/projectExplorer/git/GitCommitDialog.tsx
chat_app/src/components/projectExplorer/git/useProjectGit.ts
```

## 12. 实现顺序建议

1. 后端先做 `summary/branches/status`，并用临时 Git repo 写测试。
2. 前端改 `WorkspaceTabs.rightActions`，接入只读 `GitBranchButton`。
3. 做弹层搜索和分支列表。
4. 接入 `fetch/checkout/new branch`。
5. 接入 `pull/push`。
6. 最后做 commit dialog。

这样每一步都能独立编译和验证，不会一次性改太大。

## 13. 当前实现状态

已完成：

- 顶部 Git 入口已接到 `WorkspaceTabs.rightActions`，放在项目浏览器顶部右侧。
- 后端已新增 `chat_app_server_rs/src/api/git.rs` 和 `chat_app_server_rs/src/services/git/`，通过 Git CLI 安全执行，不走 shell。
- 已支持 summary、branches、status、fetch、pull、push、checkout、new branch、stage、unstage、commit。
- 已支持 `GET /api/git/client` 查看当前 Git client 来源、路径、版本和内置 Git 候选路径。
- 已支持 branch compare 和 file diff：
  - `GET /api/git/compare?root=...&target=...`
  - `GET /api/git/diff?root=...&path=...`
- 前端 Git 弹层已支持工作区文件 Diff、Stage/Unstage、分支 Compare、提交差异文件列表、左右分支独有提交摘要和 patch 预览。
- 工作区状态文件已区分 `staged` 和 `unstaged`，同一个文件同时存在两类改动时可以分别查看 staged/worktree diff。
- 未跟踪文本文件已支持 synthetic diff 预览，超大文件和二进制文件会返回安全摘要。
- 未跟踪文件预览会 canonicalize 路径并拒绝仓库外路径或符号链接，避免通过 symlink 读取仓库外文件。
- 后端已支持内置 Git client 解析：`CHATOS_GIT_BIN`、打包资源目录、系统 `git` 三层 fallback。
- 前端 Git 面板会显示当前 Git client 来源和版本，方便确认是否命中内置 Git。
- Commit 弹窗已按 `Staged only / Mixed / Unstaged / Untracked` 分组，并支持按组快速选择或取消。
- Commit 弹窗新增 `Commit staged only`，用于只提交 index 里已有内容，避免 mixed 文件被普通 Commit 自动 stage 全量改动。
- Commit 失败时弹窗不会自动关闭，保留提交信息和选择状态，方便用户修正后重试。
- Checkout 在 dirty worktree 下会前端二次确认；Pull 默认使用 `--ff-only`；没有实现 force push、reset hard、branch delete 等危险操作。

已验证：

- `cargo fmt --manifest-path chat_app_server_rs/Cargo.toml`
- `cargo test --manifest-path chat_app_server_rs/Cargo.toml services::git -- --nocapture`
- `cargo check --manifest-path chat_app_server_rs/Cargo.toml`
- `npm run type-check --prefix chat_app`
- `npm run build --prefix chat_app`
- `git diff --check` 对已跟踪改动通过

后续可以继续优化：

- 打包阶段补齐各平台内置 Git 二进制和许可证说明。
- Commit 弹窗后续可以继续优化为更细粒度的 hunk/line stage。
- 分支比较可以增加“以当前分支为目标 / 以目标分支为目标”的方向切换。
- 增加 stash、rebase、delete branch，但需要保持二次确认和冲突保护。
