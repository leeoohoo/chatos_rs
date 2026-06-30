# Task Runner Skills 管理实施方案

## 背景

当前 `task_runner_service` 已经具备几块和 Skills 管理相邻的能力：

- 前端左侧菜单在 `task_runner_service/frontend/src/components/AppShell.tsx` 中集中维护。
- 前端路由在 `task_runner_service/frontend/src/App.tsx` 中集中注册。
- 设置页已经通过 `/api/skills/task-runner` 展示 Task Runner 对外 skill 文本，代码位于 `task_runner_service/backend/src/api/core/system.rs` 和 `task_runner_service/frontend/src/pages/SettingsPage.tsx`。
- 外部 MCP 配置已经具备完整的 CRUD、用户归属、任务绑定和运行时注入链路，可作为 Skills 管理的实现参照。
- 任务运行准备阶段在 `task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs` 中组装 prompt、prefixed input items 和 MCP executor。
- Project Management skill 目前通过 `project_management_skill_prefixed_input_items` 在运行时注入，说明系统已经有“运行前加载 skill 文本并注入模型上下文”的模式。

用户希望新增一个左侧菜单 `Skills 管理`，用户可以：

1. 自己添加、编辑、启用、停用、删除自己的 skill。
2. 搜索公开或预置的 skill 市场。
3. 点击安装，把市场中的 skill 安装到当前 Task Runner 系统。
4. 后续任务运行时能使用已安装 skill。

## 核心判断

这里的 `Skill` 不应该和现有 `External MCP Config` 混为一类：

- `External MCP Config` 解决“有哪些外部工具服务器可以调用”。
- `Skill` 解决“模型在某类任务/工具/领域中应该遵守什么工作方法、约束和提示词”。

所以建议新增独立的 `skills` 模块，而不是把 skill 塞进 `/api/external-mcp-configs` 或 MCP 目录页里。

## 产品形态

新增左侧菜单：

- 菜单名：`Skills 管理`
- 路由：`/skills`
- 推荐图标：`ReadOutlined`、`BookOutlined` 或 `ExperimentOutlined`
- 权限：普通用户可管理自己 owner scope 下的 skill；管理员可查看/管理全部或切换筛选。

页面建议分三个 Tab：

1. `我的 Skills`
   - 展示当前用户已安装或自建的 skill。
   - 支持搜索、标签筛选、启用/停用、详情、编辑、删除、复制。

2. `添加 Skill`
   - 支持手动创建。
   - 支持粘贴 Markdown 内容。
   - 支持从 Git URL / raw URL 导入。
   - 支持上传 `SKILL.md` 或 zip 包，第一期可以先不做上传，只做 Markdown/URL。

3. `Skill 市场`
   - 支持关键词搜索。
   - 支持按语言、来源、标签、适用场景筛选。
   - 支持查看详情、预览内容、安装。
   - 第一阶段先接“配置化 registry index”；后续再扩展 GitHub 搜索或官方市场。

## 数据模型

新增后端模型文件：

- `task_runner_service/backend/src/models/skill.rs`

建议记录：

```rust
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub content: String,
    pub locale: String,
    pub tags: Vec<String>,
    pub source: SkillSource,
    pub source_url: Option<String>,
    pub source_registry: Option<String>,
    pub source_package_id: Option<String>,
    pub version: Option<String>,
    pub checksum: Option<String>,
    pub install_status: SkillInstallStatus,
    pub enabled: bool,
    pub auto_inject: bool,
    pub scope: SkillScope,
    pub creator_user_id: Option<String>,
    pub creator_username: Option<String>,
    pub creator_display_name: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub installed_at: Option<String>,
}
```

推荐枚举：

```rust
pub enum SkillSource {
    Manual,
    Url,
    Registry,
    Bundled,
}

pub enum SkillInstallStatus {
    Installed,
    Disabled,
    Failed,
}

pub enum SkillScope {
    User,
    AdminGlobal,
}
```

第一期可以不做 `AdminGlobal` 写入，只保留字段，方便后续管理员发布全局 skill。

新增请求模型：

- `CreateSkillRequest`
- `UpdateSkillRequest`
- `InstallSkillRequest`
- `SearchSkillMarketplaceQuery`
- `SkillMarketplaceEntry`
- `SkillPreviewResponse`

## 存储设计

当前后端同时支持 memory / sqlite / mongo，因此新增 skill 存储必须覆盖三套实现。

### InMemory

在 `StoreData` 中新增：

```rust
skills: BTreeMap<String, SkillRecord>
```

新增 `task_runner_service/backend/src/store/in_memory/skills.rs`：

- `list_skills`
- `get_skill`
- `save_skill`
- `delete_skill`

### SQLite

新增 migration：

- `task_runner_service/backend/migrations/0022_skills.sql`

建议表结构：

```sql
CREATE TABLE IF NOT EXISTS skills (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  display_name TEXT NOT NULL,
  description TEXT,
  content TEXT NOT NULL,
  locale TEXT NOT NULL DEFAULT 'zh-CN',
  tags_json TEXT NOT NULL DEFAULT '[]',
  source TEXT NOT NULL DEFAULT 'manual',
  source_url TEXT,
  source_registry TEXT,
  source_package_id TEXT,
  version TEXT,
  checksum TEXT,
  install_status TEXT NOT NULL DEFAULT 'installed',
  enabled INTEGER NOT NULL DEFAULT 1,
  auto_inject INTEGER NOT NULL DEFAULT 0,
  scope TEXT NOT NULL DEFAULT 'user',
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  installed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skills_owner_user_id
ON skills(owner_user_id);

CREATE INDEX IF NOT EXISTS idx_skills_enabled
ON skills(enabled);

CREATE INDEX IF NOT EXISTS idx_skills_source_package
ON skills(source_registry, source_package_id);

CREATE INDEX IF NOT EXISTS idx_skills_updated_at
ON skills(updated_at DESC);
```

新增：

- `task_runner_service/backend/src/store/sqlite/models/skills.rs`
- `skill_from_row` 到 `sqlite_rows.rs`
- `AppStore` 转发方法到 `store/app_models.rs` 或新增 `store/app_skills.rs`

### Mongo

在 `MongoStore` 中新增：

```rust
skills: Collection<SkillRecord>
```

集合名：`skills`

索引：

- `{ id: 1 }` unique
- `{ owner_user_id: 1 }`
- `{ enabled: 1 }`
- `{ updated_at: -1 }`
- `{ source_registry: 1, source_package_id: 1 }`

新增：

- `task_runner_service/backend/src/store/mongo/skills.rs`

## 后端服务层

新增：

- `task_runner_service/backend/src/services/skill_service.rs`

`SkillService` 职责：

1. CRUD：创建、更新、删除、启用、停用用户 skill。
2. 权限：复用现有 `owner_user_id / creator_user_id` 可见性规则。
3. 校验：校验 name、content、locale、tags、source。
4. 市场搜索：调用 registry provider 搜索可安装 skill。
5. 安装：把 marketplace entry 拉取成 `SkillRecord`。
6. 运行时查询：按任务/用户/语言返回需要注入的 skill。

建议方法：

```rust
impl SkillService {
    pub async fn list_skills(&self, current_user: &CurrentUser, filters: SkillListFilters) -> Result<Vec<SkillRecord>, String>;
    pub async fn get_skill(&self, current_user: &CurrentUser, id: &str) -> Result<Option<SkillRecord>, String>;
    pub async fn create_skill(&self, input: CreateSkillRequest, current_user: &CurrentUser) -> Result<SkillRecord, String>;
    pub async fn update_skill(&self, id: &str, input: UpdateSkillRequest, current_user: &CurrentUser) -> Result<Option<SkillRecord>, String>;
    pub async fn delete_skill(&self, id: &str, current_user: &CurrentUser) -> Result<bool, String>;
    pub async fn search_marketplace(&self, query: SkillMarketplaceQuery) -> Result<Vec<SkillMarketplaceEntry>, String>;
    pub async fn install_marketplace_skill(&self, input: InstallSkillRequest, current_user: &CurrentUser) -> Result<SkillRecord, String>;
    pub async fn runtime_skills_for_task(&self, task: &TaskRecord, locale: BuiltinMcpPromptLocale) -> Result<Vec<SkillRecord>, String>;
}
```

在 `AppState` 中新增：

```rust
pub skill_service: SkillService
```

并在 `AppState::new` 初始化。

## API 设计

新增 API 文件：

- `task_runner_service/backend/src/api/skills.rs`

受登录保护的接口：

```text
GET    /api/skills
POST   /api/skills
GET    /api/skills/:id
PATCH  /api/skills/:id
DELETE /api/skills/:id
POST   /api/skills/:id/enable
POST   /api/skills/:id/disable
POST   /api/skills/preview
GET    /api/skills/marketplace/search
GET    /api/skills/marketplace/:source/:package_id
POST   /api/skills/marketplace/install
```

保留现有公共接口：

```text
GET /api/skills/task-runner
```

这个接口当前用于展示 Task Runner 对外 skill，不建议改语义，避免破坏 Chatos 或外部 agent 读取已有 Skill 文本。

## Skill 市场实现

第一期不建议直接无限制搜索全网并安装。原因：

- skill 内容会进入模型上下文，属于 prompt supply chain 风险。
- 如果未来支持 zip/脚本类 skill，风险更高。
- “市面上的 skill”来源不统一，需要先抽象 registry provider。

推荐分三层：

### 1. Curated Registry

新增配置：

```env
TASK_RUNNER_SKILL_REGISTRY_URL=https://example.com/task-runner-skills/index.json
TASK_RUNNER_SKILL_REGISTRY_TIMEOUT_MS=5000
TASK_RUNNER_SKILL_INSTALL_ALLOW_RAW_URL=0
```

registry index 示例：

```json
{
  "version": 1,
  "skills": [
    {
      "id": "code-review-zh-cn",
      "name": "code-review-zh-cn",
      "display_name": "代码 Review",
      "description": "面向代码审查的中文 skill",
      "locale": "zh-CN",
      "tags": ["code", "review"],
      "version": "1.0.0",
      "content_url": "https://example.com/skills/code-review-zh-cn/SKILL.md",
      "checksum": "sha256:..."
    }
  ]
}
```

第一期可以先做：

- 本地静态 registry 文件。
- 配置化远程 registry URL。
- 搜索只在已加载 registry index 中做关键词过滤。

### 2. Git URL / Raw URL 安装

第二期支持用户输入 GitHub raw URL 或 Git repo 路径：

- 只拉取 `SKILL.md`。
- 限制文件大小，例如 256KB。
- 只保存 Markdown 文本，不执行任何脚本。
- 记录来源、checksum、安装时间。

### 3. 公开市场搜索

第三期再接 GitHub API 或其他公开目录：

- 搜索 `SKILL.md` / `.codex/skills` / 指定 manifest。
- 先预览，后安装。
- 安装前显示来源、作者、license、内容摘要、风险提示。

## 运行时注入设计

新增 Skill 后必须明确什么时候对任务生效。

建议第一期采用两种模式：

1. `auto_inject = true`
   - 对当前用户所有任务自动注入。
   - 只注入 enabled 且 owner 匹配当前任务 owner 的 skill。
   - 默认关闭，避免用户安装后影响所有任务。

2. 任务显式绑定
   - 在任务编辑抽屉中新增 `Skills` 多选。
   - 后端在 `TaskMcpConfig` 或新的 `TaskSkillConfig` 中保存绑定关系。

更清晰的模型是新增：

```rust
pub struct TaskSkillConfig {
    pub enabled: bool,
    pub skill_ids: Vec<String>,
    pub auto_inject_user_skills: bool,
}
```

并在 `TaskRecord` 中新增：

```rust
pub skill_config: TaskSkillConfig
```

这样 Skill 不依赖 MCP 是否启用。即使某个任务不需要 MCP 工具，也可以加载写作、代码 review、测试策略等 skill。

第一期如果想减少改动，也可以临时把 `skill_ids` 放进 `TaskMcpConfig`，但长期不推荐，因为 Skill 是 prompt 能力，不是 MCP server。

运行时落点：

- 在 `prepare_model_execution` 中，构建 `prefixed_input_items` 时新增：

```rust
prefixed_input_items.extend(user_skill_prefixed_input_items(service, task, task.mcp_config.locale()).await);
```

建议新增：

- `task_runner_service/backend/src/services/run_model_phase/setup/preparation/skill_inputs.rs`

注入格式类似现有 Project Management Skill：

```text
[User Installed Skill]
Task Runner loaded these user-installed skills for this task. Follow them when they are relevant to the task objective.

Skill: xxx (zh-CN)

<content>
```

如果多个 skill 同时注入：

- 按显式绑定优先。
- 再注入 auto_inject。
- 去重。
- 限制总字符数，例如 40KB，超过后拒绝运行或截断并写入 run event。

## 前端实现

新增：

- `task_runner_service/frontend/src/pages/SkillsPage.tsx`
- `task_runner_service/frontend/src/pages/skills/SkillListTab.tsx`
- `task_runner_service/frontend/src/pages/skills/SkillEditorDrawer.tsx`
- `task_runner_service/frontend/src/pages/skills/SkillMarketplaceTab.tsx`
- `task_runner_service/frontend/src/pages/skills/SkillDetailDrawer.tsx`
- `task_runner_service/frontend/src/pages/skills/skillPageUtils.ts`

修改：

- `task_runner_service/frontend/src/components/AppShell.tsx`
  - 增加 `/skills` 菜单。
- `task_runner_service/frontend/src/App.tsx`
  - lazy import `SkillsPage`
  - 注册 `<Route path="/skills" element={<SkillsPage />} />`
- `task_runner_service/frontend/src/api/client.ts`
  - 增加 skills API 方法。
- `task_runner_service/frontend/src/types/skills.ts`
  - 增加 Skill 类型。
- `task_runner_service/frontend/src/types.ts`
  - export skills 类型。
- `task_runner_service/frontend/src/i18n/messages/zhCN.ts`
  - 增加 `nav.skills` 和页面文案。
- `task_runner_service/frontend/src/i18n/messages/enUS.ts`
  - 增加英文文案。

页面布局建议：

- 顶部：标题 + 说明 + 新建按钮 + 搜索框。
- 列表：名称、状态、来源、语言、标签、版本、更新时间、操作。
- 操作：详情、编辑、启用/停用、删除、复制。
- 市场卡片：名称、描述、来源、版本、标签、安装状态、预览、安装。

任务编辑页增强：

- `task_runner_service/frontend/src/pages/tasks/TaskEditorDrawer.tsx`
  - 在 MCP 配置附近新增 `Skills` 多选区域。
  - 多选数据来自 `api.listSkills({ enabled: true })`。
  - 显示 `auto inject` 状态说明。
- `task_runner_service/frontend/src/pages/tasks/taskPageUtils.tsx`
  - 表单值增加 `skillIds` 或 `skillConfig`。
- `task_runner_service/frontend/src/types/tasks.ts`
  - 增加 `TaskSkillConfig`。

## 权限与安全

必须做的约束：

1. 用户只能看到自己的 skill。
2. 管理员可以查看全部，但默认仍按 owner 筛选，避免误操作。
3. 安装远程 skill 时只保存文本，不执行脚本。
4. 限制单个 skill content 大小，建议 256KB。
5. 限制单次任务注入 skill 总大小，建议 40KB。
6. 保存 checksum，后续支持“检查更新”时可比较内容是否变化。
7. 市场安装必须有预览确认。
8. 市场源 URL 必须配置 allowlist 或 registry URL，不默认开放任意公网抓取。
9. 删除/停用被任务显式绑定的 skill 时，后端应提示影响范围；第一期可以允许停用但运行时跳过并记录 warning。

## 分阶段实施

### P0：只读规划和菜单骨架

- 新增 `/skills` 页面和菜单。
- 页面先展示空状态和功能说明。
- 不改后端。

### P1：用户自建 Skill CRUD

- 新增模型、migration、三套 store 实现、`SkillService`、`/api/skills` CRUD。
- 前端实现 `我的 Skills` 和 `添加 Skill`。
- 支持 Markdown 内容编辑、启用/停用、删除。
- 加 owner scope 过滤。

### P2：任务绑定与运行时注入

- 新增 `TaskSkillConfig`。
- migration 给 `tasks` 添加 `skill_config_json`。
- 任务创建/编辑支持选择 skills。
- `prepare_model_execution` 注入用户 skill prefixed input。
- run event 记录注入了哪些 skill、是否跳过/超限。

### P3：Skill 市场第一版

- 增加 registry provider。
- 配置 `TASK_RUNNER_SKILL_REGISTRY_URL`。
- 后端实现 marketplace search / detail / install。
- 前端实现市场搜索、预览、安装。

### P4：更新、版本和导入增强

- 支持检查更新。
- 支持 raw URL / Git URL 导入。
- 支持导出 skill。
- 支持管理员发布全局 skill。
- 支持按 project/task profile 配置默认 skill。

## 验收标准

### 后端

- `cargo check -p task_runner_service_backend`
- 新增 service 单元测试：
  - 创建 skill 时填充 creator / owner。
  - 普通用户不能读取其他用户 skill。
  - 管理员可读取全部。
  - marketplace install 会生成 SkillRecord。
  - disabled skill 不会注入运行时。
  - 显式绑定 skill 会注入运行时。
- SQLite migration 能从空库启动。
- Mongo index 初始化不报错。

### 前端

- `npm run type-check`
- `npm run test`
- 页面检查：
  - 左侧出现 `Skills 管理`。
  - 普通用户可新增/编辑/停用/删除自己的 skill。
  - 市场搜索有 loading / empty / error / installed 状态。
  - 任务编辑抽屉能绑定 skill。
  - 中英文文案完整。

### 联调

- 创建一个用户 skill。
- 创建任务并绑定该 skill。
- 执行任务。
- 在 run event 或模型输入快照中确认 skill 已注入。
- 禁用 skill 后再次执行，确认不再注入。

## 推荐文件改动清单

后端：

- `task_runner_service/backend/src/models/skill.rs`
- `task_runner_service/backend/src/api/skills.rs`
- `task_runner_service/backend/src/services/skill_service.rs`
- `task_runner_service/backend/src/store/app_skills.rs`
- `task_runner_service/backend/src/store/in_memory/skills.rs`
- `task_runner_service/backend/src/store/sqlite/models/skills.rs`
- `task_runner_service/backend/src/store/mongo/skills.rs`
- `task_runner_service/backend/migrations/0022_skills.sql`
- `task_runner_service/backend/src/services/run_model_phase/setup/preparation/skill_inputs.rs`
- `task_runner_service/backend/src/state.rs`
- `task_runner_service/backend/src/api/router.rs`
- `task_runner_service/backend/src/services.rs`
- `task_runner_service/backend/src/store.rs`

前端：

- `task_runner_service/frontend/src/pages/SkillsPage.tsx`
- `task_runner_service/frontend/src/pages/skills/*`
- `task_runner_service/frontend/src/types/skills.ts`
- `task_runner_service/frontend/src/api/client.ts`
- `task_runner_service/frontend/src/App.tsx`
- `task_runner_service/frontend/src/components/AppShell.tsx`
- `task_runner_service/frontend/src/i18n/messages/zhCN.ts`
- `task_runner_service/frontend/src/i18n/messages/enUS.ts`
- `task_runner_service/frontend/src/pages/tasks/TaskEditorDrawer.tsx`
- `task_runner_service/frontend/src/pages/tasks/taskPageUtils.tsx`
- `task_runner_service/frontend/src/types/tasks.ts`

## 结论

建议把 `Skills 管理` 做成 Task Runner 的一级能力，而不是设置页里的附属预览，也不要混入 MCP 目录。

第一期先落地“用户自建 Skill CRUD + 任务绑定 + 运行时注入”，这能最快形成闭环。市场搜索建议第二阶段接入配置化 registry，等安全边界稳定后再扩展到公开 GitHub/raw URL 搜索与安装。
