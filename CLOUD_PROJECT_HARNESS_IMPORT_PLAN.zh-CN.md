# Chatos 云端项目接入 Harness 方案

## 目标

把 Chatos 创建项目的“云端”方式从选择服务器目录，改成：

- 用户填写项目名。
- 可选填写 Git 地址；填写后导入到我们的 Harness，并把 Chatos 项目的 `git_url` 替换成 Harness 返回的 Git 地址。
- 可选上传 zip；后端解包后初始化 Git 仓库并推送到 Harness，再保存 Harness Git 地址。
- 只填项目名时，在 Harness 初始化一个空仓库。
- 云端项目详情页只保留“用户消息”和“Plan”两个 tab，不再展示项目目录、文件树、Git 操作、项目设置和运行设置；但联系人绑定能力必须保留。
- 不改 Harness 代码，只通过 Harness 公开 API 和 Git push 完成。

## 现状结论

1. 前端项目创建入口在 `chat_app/src/components/sessionList/CreateResourceModals.tsx`，目前 `server` 模式展示目录输入和目录选择按钮，`local_connector` 模式走本地连接器目录浏览。
2. 前端创建调用链是 `useSessionListActions.ts -> store createProject(name, rootPath) -> client.createProject({ name, root_path })`，`rootPath` 是必填语义。
3. `chat_app_server_rs/src/api/projects/crud_handlers.rs` 目前强制 `root_path` 必填，并会校验它是存在且可写的服务端目录。
4. `project_management_service` 的模型里 `root_path` 本身已经是 `Option<String>`，但 Chatos 主后端和前端类型把它当作必填路径使用。
5. 项目详情页的 tab 固定是 `files | team | plan | settings`。`team` 实际承载用户消息/项目联系人，`settings` 里当前还挂了 `ProjectContactSettingsCard` 联系人绑定和 `ProjectRunSettingsPanel` 运行设置。联系人绑定应该统一迁到“用户消息”下面；云端项目再过滤成 `team | plan`，并把默认 tab 从 `files` 改掉。
6. Harness 已有公开 API：
   - `POST /api/v1/repos` 可创建空仓库，body 包含 `parent_ref`、`identifier`、`default_branch`、`readme` 等。
   - `POST /api/v1/repos/import` 可从 provider 导入，但 importer 只覆盖 GitHub/GitLab/Bitbucket/Stash/Gitea/Gogs/Azure，不适合任意 Git URL。
   - `POST /api/v1/user/tokens` 可创建用户 PAT，后续 Git push 可使用。
7. 现有 user_service 已经负责 Harness 公开注册和用户根空间创建，但成功后没有保存 Harness PAT；如果项目导入要持续操作 Harness，需要补 PAT 管理。

## 推荐架构

采用 `project_management_service + user_service` 分工，`project_management_service` 作为云端项目创建的主编排服务：

- `project_management_service` 负责项目生命周期和云端导入：
  - 创建 Chatos 项目记录。
  - 保存 `source_type=cloud`、`import_status`、`harness_git_url`、`source_git_url` 等项目元数据。
  - 接收云端项目创建请求，包括项目名、Git 地址、zip 文件。
  - 执行 Git clone、zip 安全解包、git init/commit/push。
  - 管理导入后台任务、失败状态、重试和状态查询。
  - 继续维护 Plan、需求、work item 等项目域数据。
- `user_service` 继续作为 Harness 身份和授权边界，负责：
  - 确保 Chatos 用户已有 Harness 用户和根空间。
  - 创建/保存加密 Harness PAT。
  - 通过内部接口提供“确保 Harness 凭据”和“创建 Harness repo”的能力。
- `chat_app_server_rs` 只做 BFF/网关：
  - 继续承接前端认证和统一 API 入口。
  - 把云端项目创建请求转发给 `project_management_service`。
  - 不保存 Harness 凭据，不执行 Git/zip 导入主逻辑。
- 前端只调用 Chatos 主后端，不直接接触 Harness token 或 push URL。

这样项目创建、项目导入状态和 Plan 数据都在项目管理服务内聚；同时也符合“user_service 去调用 Harness 更好”的方向，因为 Harness 用户身份、空间和 PAT 仍然由 user_service 统一管理。

## 数据模型调整

在 `project_management_service` 的 `projects` 记录增加字段：

- `source_type`: `local | local_connector | cloud`
- `cloud_import_source`: `empty | git | zip | null`
- `source_git_url`: 用户原始 Git 地址，仅云端 Git 导入时保存。
- `harness_space_ref`: 用户 Harness 根空间，如 `u-leeoohoo`。
- `harness_repo_identifier`: Harness repo identifier。
- `harness_repo_path`: Harness repo path，如 `u-leeoohoo/demo-project`。
- `harness_git_url`: Harness HTTP clone URL。
- `harness_git_ssh_url`: Harness SSH clone URL，可选。
- `import_status`: `none | pending | importing | ready | failed`
- `import_error`: 失败原因摘要。
- `import_started_at` / `import_finished_at`

兼容规则：

- 老项目没有 `source_type` 时：
  - `root_path` 以 `local://connector/` 开头视为 `local_connector`。
  - 其他有 `root_path` 的项目视为 `local`。
- 云端项目 `root_path` 可以为空/null，`git_url` 保存最终 Harness Git URL。
- 前端 `Project.rootPath` 要改成可空，避免云端项目被误当成本地目录。

## API 设计

保留现有本地创建接口：

- `POST /api/projects`
- 用于服务端目录项目，仍然校验 `root_path`。
- local connector 现有 `/api/local-connectors/projects` 不变。

`project_management_service` 新增云端创建接口：

- `POST /api/projects/cloud`
- `multipart/form-data`
- 字段：
  - `name`: 必填。
  - `git_url`: 可选。
  - `zip_file`: 可选。
  - `description`: 可选。
- 校验：
  - `git_url` 和 `zip_file` 二选一，不能同时传。
  - 两者都不传时创建空 Harness 仓库。

响应建议：

- 空仓库可以同步返回 `201`，`import_status=ready`，`git_url=harness_git_url`。
- Git/zip 导入建议返回 `202` 或 `201 + import_status=importing`，前端通过项目详情/列表刷新拿到最终 `git_url`。

`chat_app_server_rs` 新增同名 BFF 接口：

- `POST /api/projects/cloud`
- 校验当前用户 token 后转发到 `project_management_service`。
- 对 multipart 请求做流式或临时文件转发，不在主后端执行导入。
- response 原样返回项目管理服务的项目记录。

user_service 新增内部接口：

- `POST /api/internal/harness/repos`
  - `project_management_service` 携带当前用户 token 和服务间 secret。
  - user_service 校验用户、确认 Harness provisioning、创建 Harness repo。
  - 返回 `space_ref`、`repo_identifier`、`repo_path`、`git_url`、`git_ssh_url`、内部 push 凭据。
- `POST /api/internal/harness/provisioning/ensure-token`
  - 用于补齐或轮换 Harness PAT。

这些内部接口必须加服务间密钥，例如 `X-User-Service-Internal-Secret`，避免浏览器直接拿到 Harness push token。

联系人绑定 API：

- 现有 `GET/POST/DELETE /api/projects/:id/contacts` 和 `/contacts/lock` 能力必须保留。
- 云端项目不能因为 `root_path` 为空而禁用联系人绑定。
- 联系人绑定仍然服务于“用户消息”和 Plan 执行时选择 Task Runner 联系人，不属于“项目设置/运行设置”范畴。
- 联系人绑定入口统一放到“用户消息”tab，不区分本地项目、local connector 项目和云端项目。
- 如果后续项目主数据迁到 `project_management_service` 编排，联系人绑定可以继续由 `chat_app_server_rs` BFF 维护现有 memory mapping，也可以再收敛到项目管理服务；MVP 不强制迁移，重点是前端入口不能丢。

## Harness 凭据策略

现有公开注册流程需要补一步：

1. 注册或登录 Harness 成功后，调用 `POST /api/v1/user/tokens` 创建 PAT。
2. user_service 加密保存 PAT，只保存 token identifier 和密文，不明文入库。
3. 项目导入时，user_service 只把 PAT 通过内部接口返回给 `project_management_service`，并且项目管理服务不得把 token 写日志、写数据库或返回前端。

已有用户的兼容方案：

- 如果 `harness_provisioning` 已成功但没有 PAT：
  - 用户下次 Chatos 登录时可用本次明文密码登录 Harness 并补 PAT。
  - 或管理员重置该用户密码后触发补齐。
  - 或临时配置 Harness admin/service PAT 做一次性修复。

## 云端项目创建流程

### 只填项目名

1. 前端提交 `name`。
2. `chat_app_server_rs` 转发给 `project_management_service`。
3. `project_management_service` 创建 Chatos 项目记录：`source_type=cloud`，`import_status=pending`。
4. `project_management_service` 调用 user_service 内部接口创建 Harness 空仓库。
5. 更新项目记录：`git_url=harness_git_url`，`import_status=ready`。
6. 返回项目。

### 填 Git 地址

1. 前端提交 `name + git_url`。
2. `chat_app_server_rs` 转发给 `project_management_service`。
3. `project_management_service` 创建项目记录，状态为 `importing`。
4. `project_management_service` 调用 user_service 创建 Harness 空仓库并拿到 push 凭据。
5. `project_management_service` 后台任务执行：
   - `git clone --mirror <source_git_url> <temp>`
   - 设置目标 remote 为 Harness push URL。
   - `git push --mirror` 到 Harness。
6. 成功后更新 `git_url=harness_git_url`，`source_git_url=原地址`，`import_status=ready`。
7. 失败时更新 `import_status=failed` 和 `import_error`，前端展示失败状态。

MVP 建议只支持 `http://` 和 `https://` 的公开 Git 地址；SSH 和需要认证的私有源仓库后续再加凭据输入。

### 上传 zip

1. 前端提交 `name + zip_file`。
2. `chat_app_server_rs` 转发给 `project_management_service`。
3. `project_management_service` 限制上传大小并落到临时目录。
4. 安全解包：
   - 禁止绝对路径、`..`、盘符路径。
   - 禁止 symlink/hardlink 逃逸。
   - 限制总文件数、单文件大小、解压后总大小、目录深度。
   - 忽略或拒绝 zip 内的 `.git/`。
5. `project_management_service` 创建 Harness 空仓库。
6. 后台任务执行：
   - `git init -b main`
   - `git add -A`
   - 有文件时创建初始 commit。
   - `git remote add origin <harness_push_url>`
   - `git push -u origin main`
7. 更新项目 `git_url=harness_git_url`，`import_status=ready`。

## 前端改动

项目创建弹窗要把项目和终端拆开处理：

- 终端创建继续保留 `server/local_connector` 目录选择。
- 项目创建：
  - `local_connector` 保持现有逻辑。
  - `server` 文案改为“云端”。
  - 云端表单展示：
    - 项目名输入框。
    - Git 地址输入框，可空。
    - zip 上传控件，可空。
  - 不再展示服务器目录输入和目录选择按钮。
  - Git 地址和 zip 同时填写时提示只能选一种来源。

store/client 增加：

- `createCloudProject({ name, gitUrl?, zipFile? })`
- 使用 `FormData` 调 `chat_app_server_rs` 的 `/api/projects/cloud`，由主后端转发到 `project_management_service`。

项目列表/详情：

- 云端导入中显示 `importing` 状态。
- 导入失败显示错误摘要并支持重试入口。
- 成功后展示 Harness Git 地址。
- 所有项目详情的“用户消息”区域统一提供联系人绑定/更换/解绑入口。

## 云端项目详情页

新增 `isCloudProject(project)` 判断，依据 `source_type === "cloud"`。

云端项目只允许：

- `team` tab：界面文案建议显示“用户消息”，并在这个 tab 内保留联系人绑定能力。
- `plan` tab：保留现有 Plan。

需要处理：

- `WorkspaceTabs` 支持按项目类型传入可见 tab 列表。
- `useProjectExplorerState` 对云端项目默认 tab 使用 `team`，如果 localStorage 里保存的是 `files/settings`，自动纠正。
- 云端项目隐藏 `GitBranchButton`。
- 云端项目不渲染 `ProjectExplorerFilesWorkspace`。
- 云端项目不渲染 `ProjectRunSettingsPanel`。
- `ProjectContactSettingsCard` 从 settings tab 移除，联系人绑定逻辑抽成可复用的 `ProjectContactBindingPanel`/按钮，统一挂到 `TeamMembersPane` 的头部、侧栏空状态或联系人列表工具栏里。
- 本地项目、local connector 项目、云端项目都通过“用户消息”tab 添加、切换、解绑联系人，并保留 `contacts/lock` 逻辑，避免用户消息任务运行中改绑联系人。
- 输入区的项目文件选择、代码导航、文件树相关入口在 `rootPath` 为空或云端项目时禁用。

## 安全和稳定性

- Git URL 不允许 `file://`、本机地址、内网地址，避免 SSRF 和读取本机文件。
- clone/push 进程要有超时、输出大小限制和并发限制。
- zip 要做 zip-slip 防护和资源预算。
- Harness PAT 不能出现在前端、日志、项目记录、错误信息里。
- 临时目录必须在任务完成或失败后清理。
- 导入任务要可重试，失败信息只保存摘要。
- 仓库 identifier 要由后端生成，建议 `p-{project_id_short}-{slug}`，避免同名冲突和非法字符。

## 配置项

新增建议：

- `PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_ENABLED=true`
- `PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES`
- `PROJECT_SERVICE_CLOUD_PROJECT_MAX_UNPACKED_BYTES`
- `PROJECT_SERVICE_CLOUD_PROJECT_MAX_FILES`
- `PROJECT_SERVICE_CLOUD_PROJECT_GIT_TIMEOUT_MS`
- `PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_CONCURRENCY`
- `CHATOS_USER_SERVICE_INTERNAL_SECRET`
- `USER_SERVICE_INTERNAL_API_SECRET`
- `HARNESS_PROJECT_PAT_IDENTIFIER=chatos-project-sync`
- `HARNESS_PROJECT_PAT_LIFETIME_SECONDS`

复用现有：

- `HARNESS_BASE_URL`
- `HARNESS_PROVISIONING_ENABLED`
- `HARNESS_SPACE_PREFIX`
- `HARNESS_REQUEST_TIMEOUT_MS`
- `CHATOS_USER_SERVICE_BASE_URL`

## 实施步骤

1. user_service 补 Harness PAT 管理：
   - provisioning 成功后创建 PAT 并加密保存。
   - 已有用户补 token 的兼容路径。
   - 新增内部 Harness repo 创建接口。
2. project_management_service 增加云端项目字段和迁移：
   - SQLite/Mongo 都要支持。
   - API response 带出新字段。
   - 旧项目按兼容规则推导 `source_type`。
3. project_management_service 增加云端项目创建和导入能力：
   - multipart 支持。
   - Git URL/zip 校验。
   - 调 user_service 创建 Harness repo。
   - 实现 clone/push、zip unpack/init/push。
   - 更新项目导入状态。
4. chat_app_server_rs 增加云端项目 BFF 接口：
   - 接收前端 `/api/projects/cloud`。
   - 转发 multipart 到 `project_management_service`。
   - 保持现有鉴权、用户上下文和错误格式。
5. chat_app 前端改造创建弹窗：
   - 项目云端模式改为项目名/Git/zip。
   - local connector 保持不动。
   - 增加导入状态展示。
6. chat_app 项目详情页过滤云端 tab：
   - 云端只显示用户消息和 Plan。
   - 禁用依赖本地 rootPath 的入口。
   - 从 settings tab 抽出联系人绑定组件，统一放到用户消息 tab 内，确保所有项目的绑定/换绑/解绑入口一致。
7. 增加测试和联调：
   - user_service Harness mock 测试。
   - project_management_service zip 安全解包测试。
   - Git URL 校验测试。
   - 云端项目创建 API 测试。
   - 前端 type-check 和关键组件测试。

## 需要确认的点

1. Git URL MVP 是否只支持公开 `http/https` 仓库；私有 Git 凭据是否后续单独做。
2. Git 导入是否要求 mirror 所有分支和 tag；建议使用 mirror，最符合“把这个 Git 项目传到 Harness”。
3. zip 中如果没有文件，是创建空仓库还是提示 zip 为空；建议提示 zip 为空。
4. “用户消息”是否继续使用当前 `team` tab 组件，只改文案；建议先复用现有组件，并把联系人绑定入口统一移动到该 tab。
5. 导入是否异步；建议 Git/zip 都异步，空仓库同步。
