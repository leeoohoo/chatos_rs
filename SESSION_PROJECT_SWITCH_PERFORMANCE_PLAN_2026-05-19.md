# 会话/项目切换性能优化方案

日期：2026-05-19

## 1. 结论

当前性能问题的主因不是 Rust 语言本身，而是“切换一次触发了太多同步工作”：

1. 前端把一次会话切换或项目切换放大成了多条并发初始化链路。
2. `ProjectExplorer`、会话 summary、runtime context、review-repair 都存在 eager loading。
3. 后端若干接口把项目分析、Git 状态、runtime context 组装、Memory Engine 查询串在了同步请求里。
4. 开发环境启用了 `React.StrictMode`，会把 mount 阶段的副作用故意执行两次，进一步放大卡顿体感。

判断：这是“架构编排问题 + 加载策略问题 + 部分后端接口过重”的组合问题，不是简单的“Rust 没有性能优势”。

## 2. 已确认的证据

### 2.0 2026-05-19 当天已落地的新修复

- `chat_app_server_rs/src/services/project_fs_cache.rs`
- `chat_app_server_rs/src/api/fs/query_handlers_listing.rs`
- `chat_app_server_rs/src/api/fs/mutate_handlers_create.rs`
- `chat_app_server_rs/src/api/fs/mutate_handlers_delete.rs`
- `chat_app_server_rs/src/api/fs/mutate_handlers_move.rs`
- `chat_app/src/components/projectExplorer/useProjectExplorerDataLoading.ts`
- `chat_app/src/components/projectExplorer/useProjectTreeRefreshAction.ts`
  - 项目目录树列表改为项目级 `.chatos/cache/fs/...` 持久缓存
  - 默认目录展开 / 项目切换优先命中缓存
  - 创建、删除、移动、watcher 外部变更都会失效受影响目录缓存
  - 树面板手动刷新改为真正 `force refresh`
- `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts`
  - `project.run.catalog.updated` 不再一律走 `POST /run/analyze`
  - 普通同步改为优先读取 `GET /run/catalog`
  - 只有真正影响运行目标的事件才重分析
- `chat_app_server_rs/src/services/workspace_realtime_watcher.rs`
- `chat_app_server_rs/src/builtin/code_maintainer/storage.rs`
- `chat_app_server_rs/src/services/project_run/analyzer.rs`
  - 后端把“运行配置变更”事件收紧到了真正影响运行目标的文件
  - 普通源码编辑不再把 settings 页推入反复重分析
- `chat_app/src/components/projectExplorer/useProjectExplorerRunState.ts`
  - 切出 settings 再切回时，不再把 runner 状态整套 reset 后重拉
- `chat_app/src/components/projectExplorer/codeNav/useCodeNavResources.ts`
  - 文件符号树改成展开时才加载，不再每次切文件都自动拉 symbols
- `chat_app/src/components/projectExplorer/git/useProjectGitLifecycle.ts`
- `chat_app/src/components/projectExplorer/git/useGitBranchButtonModel.ts`
  - Git 面板关闭时不再后台强刷 summary/details
  - Git 面板首次打开改为缓存优先，手动刷新才强制刷新
- `chat_app_server_rs/src/services/git/parsing.rs`
  - Git summary / status / compare 过滤 `.chatos/cache`，避免缓存文件反向污染 Git 结果
- `chat_app/src/components/projectExplorer/useProjectExplorerDataLoading.ts`
  - 文件页左侧变更摘要默认改走项目 change summary snapshot
  - 首屏不再默认同步触发 `getGitSummary + getGitStatus`
- `chat_app_server_rs/src/services/project_run/environment.rs`
  - `run/environment` 在 catalog 缺失时不再偷偷触发一次项目重分析

判断：

这一批修复以后，当前最严重的“项目设置页闪烁 + `run/analyze` 风暴 + 切 tab / 切项目时隐藏重请求过多”已经被从触发源和请求编排两端一起收缩。

### 2.1 项目切换会触发整套 ProjectExplorer 子系统初始化

- `chat_app/src/components/projectExplorer/useProjectExplorerProjectLifecycle.ts`
  - 项目一切换就会：
    - `loadEntries(root)`
    - `loadChangeSummary()`
    - 依次恢复所有已展开目录并 `tryLoadEntries(full, { silent: true })`
- `chat_app/src/components/projectExplorer/useProjectExplorerRunState.ts`
  - 只要 `project.id` 变化，就会 `runnerCatalog.refreshRunnerState()`
  - 这会继续触发：
    - `analyzeProjectRun`
    - `getProjectRunEnvironment`
    - `getProjectRunState`
- `chat_app/src/components/ProjectExplorer.tsx`
  - 即使当前不是 `settings` 页签，也会先初始化 `useProjectExplorerViewModel`，而 `runState` 已经在里面启动了

结果：用户只是切一个项目，实际上把文件树、Git、运行配置、运行状态、团队成员工作区都一起唤醒了。

### 2.2 项目运行设置链路存在重复重分析

- `chat_app_server_rs/src/services/project_run/environment.rs`
  - `load_environment_snapshot()` 内部会再次调用 `analyze_project(project).await`
- `chat_app_server_rs/src/services/project_run/analyzer.rs`
  - `analyze_project()` 会进入 `detect_targets_sync(PathBuf::from(root_path))`
  - 这是一次真正的项目目录探测，不是纯内存读取

结果：前端打开或切换项目时，`run/environment` 不只是读配置，还会重新分析整个项目运行目标。

### 2.3 项目文件树初始化不是“只加载根目录”

- `chat_app/src/components/projectExplorer/useProjectExplorerProjectLifecycle.ts`
  - 切项目后会恢复本地保存的 expanded paths
  - 并对每个已展开目录再次调用 `tryLoadEntries`

结果：如果你上一次展开了很多目录，下一次切回项目时会自动重放很多 `entries` 请求。

### 2.4 Git 变更摘要不是轻请求

- `chat_app/src/components/projectExplorer/useProjectExplorerDataLoading.ts`
  - `loadChangeSummary()` 优先走：
    - `getGitSummary(projectRootPath, selectedGitRepoRoot)`
    - 如果是 repo，再走 `getGitStatus(projectRootPath)`
  - 失败后才 fallback 到 `getProjectChangeSummary(projectId)`

结果：文件页一挂载就可能同步触发 Git summary + Git status，两次仓库状态读取对大仓库会非常伤。

### 2.5 会话切换总会拉 compact history

- `chat_app/src/lib/store/actions/sessions/selectSession.ts`
  - 即使已有缓存，也会并发：
    - `fetchSession(client, sessionId)` 或复用已有 session
    - `fetchSessionMessages(client, sessionId, { limit: 50, before: null })`
- `chat_app/src/lib/store/helpers/messages.ts`
  - `fetchSessionMessages` 实际走的是 `messages/compactHistory`
- `chat_app/src/lib/store/helpers/messages/compactHistory.ts`
  - 对应接口是 `/conversations/:id/compact-history?limit=50`

结果：切会话并不是“先秒切缓存、后台补齐”，而是每次都要打一轮 compact history。

### 2.6 summary pane 打开后会额外拉很多数据

- `chat_app/src/components/chatInterface/useChatSessionEffects.ts`
  - 只要 summary pane 可见，就会 `loadContactMemoryContext(currentSession.id)`
- `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
  - 会并发拉：
    - `loadConversationSummaryItems(..., { limit: 300 })`
    - `getContactAgentRecalls(..., { limit: 200, offset: 0 })`

结果：用户切会话时如果 summary pane 是开着的，就会额外带出大页 summary 和记忆 recall。

### 2.7 runtime context 接口本身就很重

- `chat_app_server_rs/src/modules/conversation_runtime/runtime_context.rs`
  - `resolve_runtime_context()` 会串上：
    - session 读取
    - contact 映射查找
    - agent runtime context 读取
    - project runtime 解析
    - MCP server selection / 装配
    - `compose_chatos_context(session, true)` 生成 memory summary prompt

结果：runtime context 不是轻量 metadata 查询，而是一条“动态组装完整运行上下文”的重路径。

### 2.8 review-repair 状态接口不是纯本地轻读

- `chat_app_server_rs/src/modules/conversation_runtime/review_repair.rs`
  - `get_review_repair_status()` 直接走 `chatos_memory_engine::get_chatos_review_repair_status()`
  - 后台任务完成等待逻辑还会每 `1500ms` 轮询一次，最多 `210` 次

结果：review-repair 这条链虽然已经比以前好一些，但状态读取仍然不够便宜。

### 2.9 当前数据量看起来不是根因

- `.local/memory_server/data/memory_server.db` 只有约 `5.1MB`
- 现有 SQLite 索引也不算少，至少 `messages/session_id`、`summaries/session_id`、`sessions/user_id` 这类基础索引都在

判断：当前更像“请求编排和接口重量问题”，不是“库表大到拖垮一切”。

### 2.10 开发环境会放大问题

- `chat_app/src/main.tsx`
- `docs/memory_engine/frontend/src/main.tsx`

两边都启用了 `React.StrictMode`。

结果：开发环境下 mount effect 双跑，重复请求会比生产环境更明显。

## 3. 根因排序

按影响度排序：

1. 前端 eager loading 和 mount fan-out
2. 项目运行设置链路的重复分析与同步探测
3. 文件树恢复展开目录导致的多次 `entries` 请求
4. 会话切换默认拉 compact history，且会叠加 summary/runtime/review 状态链路
5. runtime context / review-repair / Git status 接口本身偏重
6. 开发环境 `StrictMode` 放大了重复副作用

## 4. 优化目标

目标不是把所有请求都删掉，而是把“切换首屏阻塞路径”压到最小。

建议目标：

1. 切项目到文件页首屏：
   - 阻塞请求最多只保留 `root entries` 和一个轻量 `summary snapshot`
   - 不再默认触发 run catalog / run environment / run state
2. 切会话到聊天首屏：
   - 先立即显示缓存
   - compact history 改为后台补齐
   - summary/runtime/review 状态全部按需加载
3. 项目 settings 页首次打开时：
   - 才触发 run analysis / environment / state
4. review-repair 和 runtime context：
   - 以 realtime / cached snapshot 为主
   - HTTP 只做显式刷新或真正恢复

## 5. 第一阶段：快速止血

建议优先做，风险低，收益高。

### 5.1 把 ProjectExplorer 改成真正的“按页签加载”

目标：

- `files` 页签只负责：
  - root entries
  - 变更摘要
  - 当前选中文件
- `settings` 页签才负责：
  - run catalog
  - run environment
  - run state
- `team` 页签才负责：
  - team members 会话资源

建议改法：

1. `useProjectExplorerRunState` 增加 `enabled` 参数
2. 只有 `workspaceTab === 'settings'` 时才初始化 runner 相关 hook
3. `ProjectRunSettingsPanel` 改为第一次打开时再加载，不在 `ProjectExplorer` 根层初始化

预期收益：

- 项目切换时立刻少掉 `analyze`、`environment`、`state` 三组请求
- 这是当前最值得先做的一刀

### 5.2 文件树改成“root first, expanded later”

目标：

- 切项目时先只加载根目录
- 已展开目录放到后台恢复
- 加并发上限和数量上限

建议改法：

1. `useProjectExplorerProjectLifecycle.ts`
   - 先 `loadEntries(root)`
   - expanded path 恢复改成后台任务
2. 恢复时加：
   - 最大恢复目录数，例如 `10`
   - 最大并发数，例如 `2` 或 `3`
3. 对超深或超多的展开状态做裁剪

预期收益：

- 大项目切换时不会因为上次展开状态而瞬间触发几十个 `entries`

### 5.3 会话切换改成“缓存优先，后台刷新”

目标：

- 用户点击会话后先秒切本地缓存
- 再后台刷新 compact history
- 相同 session 的重复 select 要彻底去重

建议改法：

1. `selectSession()` 内：
   - 如果已有完整缓存，先直接完成 UI 切换
   - 后台调用 `syncSessionMessagesInBackground(sessionId)`
2. 对同一个 `sessionId` 增加 inflight 去重
3. 如果只是从项目面板切回同一会话，不再重复打 `compact-history`

预期收益：

- 切会话手感会明显改善
- 首屏不再绑死在 `/compact-history?limit=50`

### 5.4 summary pane 降载

目标：

- summary 默认只拉首屏足够的数据

建议改法：

1. `loadConversationSummaryItems(..., { limit: 300 })` 改成：
   - 首屏 `50` 或 `100`
   - 滚动/展开时再翻页
2. `getContactAgentRecalls(..., { limit: 200 })` 改成：
   - 首屏 `50`
   - 或 summary 和 recall 分两段加载
3. summary pane 关闭时不自动 refresh

预期收益：

- 会话切换时少掉一大段不必要的数据拉取

### 5.5 review-repair 状态探测收敛

目标：

- 切会话时不自动探测 review-repair 状态
- 只有相关面板可见、realtime 断开恢复、或显式刷新时才打 HTTP

建议改法：

1. 复用现有 realtime cache
2. 去掉会话切换时的默认状态探测
3. 保留 reconnect 和 visible restore 的兜底

### 5.6 开发环境去重防抖

目标：

- 把 `StrictMode` 造成的重复 mount 请求抑制住

建议改法：

1. 对明确只应执行一次的初始化 effect 加 strict-mode guard
2. benchmark 时一定区分：
   - dev build
   - production build

注意：

- 不建议直接移除 `StrictMode`
- 正确做法是让初始化逻辑具备幂等和去重能力

## 6. 第二阶段：后端接口减重

### 6.1 `run/environment` 不要重新分析项目

当前问题：

- `load_environment_snapshot()` 内部又调用了 `analyze_project()`

建议改法：

1. `environment` 优先读取缓存 catalog
2. 没有缓存时返回“待分析”状态，而不是同步重跑分析
3. `analyze` 单独作为显式动作或后台预热任务

更合理的职责：

- `GET /run/catalog`
  - 读缓存
- `POST /run/analyze`
  - 触发分析
- `GET /run/environment`
  - 只读 toolchain snapshot 和 validation snapshot
  - 不再隐式触发目录分析

### 6.2 Git change summary 读缓存快照

当前问题：

- 前端文件页挂载就可能同步触发 `getGitSummary + getGitStatus`

建议改法：

1. 后端维护 per-project 的 Git snapshot
2. 由 watcher / TTL 异步刷新
3. 前端默认只读 snapshot
4. 用户手动点刷新时才强制实时扫描

### 6.3 runtime context 拆成轻重两层

当前问题：

- runtime context 读取串了 session、contact、project、MCP、memory summary 等一整套逻辑

建议改法：

拆成两个接口：

1. `runtime-context/meta`
   - 最近一次 snapshot
   - session/project/contact/tool 选择信息
   - 便宜、可缓存
2. `runtime-context/full`
   - 真的需要展开 drawer 时再加载
   - 包含完整 prompt 拼装和 memory summary

### 6.4 review-repair 状态读取本地化

建议改法：

1. 把 running / pending count 镜像到 chat server 自己的本地状态表
2. `GET /review-repair` 优先读本地镜像
3. 只有镜像缺失时才 fallback 到 memory engine

### 6.5 compact history 提供 bootstrap 读模型

建议改法：

增加一个轻量 bootstrap 接口，例如：

`GET /api/conversations/:id/bootstrap`

返回：

- session 基本信息
- 最近一页 compact messages
- summary 是否存在
- review-repair 是否 running
- 当前 project/contact 绑定信息

这样前端一次请求就能拿到会话首屏必需信息，而不是切会话时打 4 到 8 个请求。

## 7. 第三阶段：架构重构

如果准备直接动架构，建议这样改，而不是继续堆局部 patch。

### 7.1 前端改为“资源域”而不是“页面挂载即拉全量”

当前问题：

- `ProjectExplorer` 通过很多 hook 叠加初始化
- `ChatSession` 也通过很多 side panel effect 叠加初始化

建议的新结构：

1. `project-files-resource`
   - 只负责目录树、文件内容、搜索、code nav
2. `project-settings-resource`
   - 只负责 run catalog、run env、run instances
3. `project-team-resource`
   - 只负责 team members 相关会话资源
4. `conversation-resource`
   - 只负责消息首屏、分页、summary 状态、review 状态
5. `runtime-context-resource`
   - 独立按需打开

核心原则：

- 非活动资源不加载
- 资源自己管理 cache / inflight / stale
- 页面切换只切 resource，不要把整个工作区重新初始化

### 7.2 后端增加“快照读模型”

建议新增两类 read model：

1. `project_workspace_snapshot`
   - root entries
   - git summary
   - selected repo root
   - refreshed_at
2. `project_run_snapshot`
   - last analyzed targets
   - toolchain options
   - validation issues
   - run instances summary

更新来源：

- workspace watcher
- 手动 refresh
- TTL 异步重算

这样 UI 读的是快照，不是每次现算。

### 7.3 memory engine 相关状态做本地镜像

建议镜像：

1. summary count
2. latest summary ids / timestamps
3. review-repair running / pending count
4. latest runtime snapshot metadata

目标：

- 高频 UI 状态读取尽量不跨进程、不跨服务
- Memory Engine 保持为事实来源，但不是每个按钮点击都去直连

## 8. 推荐执行顺序

建议按下面顺序推进：

1. `ProjectExplorer` runner 改成按 `settings` 页签懒加载
2. 文件树改成 root first，expanded later
3. 会话切换改成缓存优先，compact history 后台刷新
4. summary pane 首屏 limit 下调，recall 分段加载
5. `run/environment` 去掉隐式 `analyze_project`
6. runtime context 拆分轻重接口
7. 引入会话 bootstrap 接口
8. 做 workspace snapshot / local mirror 架构重构

## 9. 验收指标

建议把优化是否生效量化。

### 9.1 项目切换

目标：

- 切到文件页首屏阻塞请求 <= 2
- 不再默认出现：
  - `run/analyze`
  - `run/environment`
  - `run/state`
- 根目录首屏展示时间：
  - warm path < 150ms
  - cold path < 400ms

### 9.2 会话切换

目标：

- 点击会话到消息可见：
  - warm path < 80ms
  - cold path < 300ms
- summary pane 关闭时：
  - 不触发 summaries / recalls 加载
- runtime context drawer 关闭时：
  - 不触发 runtime-context 请求

### 9.3 settings 页签

目标：

- 首次打开 settings 时才允许触发 run analysis / environment
- 第二次打开命中缓存，不重复全量分析

## 10. 最终判断

当前架构确实需要改，但不建议一上来全量推倒重来。

最划算的路径是：

1. 先砍掉 eager loading 和重复分析
2. 再把重接口改成 snapshot / bootstrap / local mirror
3. 最后再做前端资源域化重构

如果只做第一阶段，切项目和切会话的体感就应该会立刻改善一大截。
如果做完第二阶段，系统才会从“切一下就现算很多东西”转成“平时异步准备、切换时读快照”的架构。
