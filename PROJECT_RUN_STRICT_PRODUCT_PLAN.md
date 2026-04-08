# 项目运行能力加强方案（灰/绿播放按钮 + 多启动目标 + 单文件右键 Run）

## 1. 你的目标（转成可验收产品行为）

你要的不是“能运行一次”，而是稳定、可感知的运行体系。统一定义如下：

1. 新增项目后，系统会自动解析并分析启动方式。
2. 播放按钮状态明确：
- 检测不到可运行目标：灰色、不可点击。
- 检测到至少一个可运行目标：绿色、可点击。
3. 一个目录下存在多个可启动项目时，支持多目标选择与默认目标。
4. 对于可直接运行的单文件（例如 `*.py`、`*.js`），文件右键菜单有 `Run`。

这 4 条作为最终验收标准。

---

## 2. 当前代码现实（重新核对后）

1. 项目右键菜单目前只有：新建目录/新建文件/下载/删除。
- 文件：`agent_workspace/src/components/projectExplorer/Overlays.tsx`

2. 终端系统已有 `busy` 与“空闲终端复用”能力。
- API：`agent_orchestrator/src/api/terminals/*`
- 运行时：`agent_orchestrator/src/services/terminal_manager/*`

3. 后端其实已有一套“空闲复用否则创建”的实现（在 builtin terminal_controller）。
- `agent_orchestrator/src/builtin/terminal_controller/actions.rs`

4. 目前没有“项目启动目标检测/存储/状态”模型与 API。

结论：执行底座有，检测与产品层缺失。

---

## 3. 产品交互方案（你关心的灰/绿）

## 3.1 播放按钮状态机

为每个项目维护 `run_status`：

1. `analyzing`
- UI：灰色 + 小转圈（不可点击）
- 场景：刚创建项目或手动重扫中

2. `ready`
- UI：绿色（可点击）
- 条件：检测到 `targets.length > 0`

3. `empty`
- UI：灰色（不可点击）
- 条件：扫描完成但无可运行目标

4. `error`
- UI：灰色 + 警示（不可点击）
- 提供“重试检测”

## 3.2 新增项目后的行为

1. 项目创建成功后立即异步触发分析。
2. 首次分析完成且 `ready` 时，弹窗“确认默认启动方式”：
- 只有 1 个目标：默认选中，用户可确认。
- 多个目标：用户选择默认目标。
3. 未检测到目标：按钮保持灰色，但允许“手动添加启动命令”（保存后变绿）。

## 3.3 多启动目标（单项目多服务 / monorepo）

播放按钮点击逻辑：

1. 若只有一个目标：直接运行。
2. 若多个目标：弹出选择器（含默认标记）。
3. 支持“设为默认启动方式”。
4. 支持“并行启动多个目标”（V2 功能，可选）。

## 3.4 单文件右键 Run

在项目树右键菜单新增：

1. `运行文件`（仅对可运行文件显示）
- `*.py` -> `python <file>`
- `*.js` -> `node <file>`
- `*.ts` -> 建议 `tsx <file>` 或 `ts-node <file>`（先做环境检查）
2. `以...运行` 子菜单（V2）
- 例如 Python3/PyPy、node/deno

---

## 4. 检测与分析设计（核心）

## 4.1 目标模型

定义 `RunTarget`：

1. `id`
2. `label`（如 `web-app: npm run dev`）
3. `kind`（node/python/java/go/rust/...）
4. `cwd`
5. `command`
6. `source`（auto/custom/file）
7. `confidence`（0~1）
8. `is_default`

## 4.2 自动检测规则（V1）

1. Node/Web
- 识别 `package.json`
- 候选 `npm run dev/start`，缺失脚本则 `npm start` 兜底

2. Java
- `pom.xml` -> `mvn spring-boot:run`（检测 spring 依赖）或 `mvn exec:java`
- `build.gradle|gradlew` -> `./gradlew bootRun` 或 `./gradlew run`

3. Python
- `pyproject.toml`/`requirements.txt`/`main.py|app.py`
- 候选 `python main.py` / `python app.py`

4. Go
- `go.mod` -> `go run .`

5. Rust
- `Cargo.toml` -> `cargo run`

## 4.3 多项目目录检测（monorepo）

扫描策略：

1. 从项目根向下 BFS 扫描（限制深度和文件数）。
2. 忽略目录：`node_modules`, `.git`, `dist`, `build`, `.next`, `.venv`, `target`。
3. 每命中一个 marker（如子目录 `package.json`）就产出一个 `RunTarget`。
4. 同一 `cwd` 重复目标去重。

---

## 5. 后端实现方案（具体）

## 5.1 新增模块

1. `agent_orchestrator/src/services/project_run/mod.rs`
2. `.../detector.rs`（扫描与规则）
3. `.../executor.rs`（运行分发）
4. `.../env_check.rs`（环境检查）
5. `.../types.rs`

## 5.2 复用终端调度（避免重复）

把以下逻辑抽成共享 `terminal_dispatcher`：

1. 选空闲终端（同用户、同项目、同 cwd、running、busy=false）
2. 无则创建终端
3. `ensure_running` + `write_input(command + "\\n")`

复用来源：
- `agent_orchestrator/src/builtin/terminal_controller/actions.rs`

让两处共用：
1. 项目运行 API
2. builtin terminal_controller

## 5.3 运行分析存储（必须持久化）

新增表/集合（建议）：`project_run_catalogs`

字段建议：
1. `project_id`（唯一）
2. `user_id`
3. `status`（analyzing/ready/empty/error）
4. `default_target_id`
5. `targets_json`
6. `error_message`
7. `analyzed_at`
8. `updated_at`

原因：
1. 刷新后状态不丢失
2. 支持“绿色/灰色按钮”稳定展示
3. 便于重扫和诊断

## 5.4 API 设计

1. `POST /api/projects/:id/run/analyze`
- 手动重扫

2. `GET /api/projects/:id/run/catalog`
- 返回状态 + targets + default

3. `POST /api/projects/:id/run/execute`
- 入参：`target_id` 或 `command + cwd`
- 返回：`terminal_id`, `terminal_reused`, `executed_command`

4. `POST /api/projects/:id/run/default`
- 设置默认 target

5. `POST /api/projects/:id/run/execute-file`
- 入参：`file_path`
- 后端映射命令并执行

6. `POST /api/projects/:id/run/check-env`
- 返回缺失 runtime（node/python/java/...）

---

## 6. 前端实现方案（具体）

## 6.1 项目列表中的播放按钮（你最关心）

改动：`agent_workspace/src/components/sessionList/sections/ProjectSection.tsx`

1. 每个项目卡片右侧新增播放按钮。
2. 按 `run_status` 渲染：
- `ready` -> 绿色可点
- 其它 -> 灰色禁用（`analyzing` 显示转圈）
3. hover 提示状态原因（未检测到/检测中/检测失败）。

## 6.2 项目目录页顶部运行入口

改动：
1. `agent_workspace/src/components/projectExplorer/TreePane.tsx`
2. `agent_workspace/src/components/projectExplorer/PreviewPane.tsx`

功能：
1. 显示默认目标
2. 多目标选择下拉
3. 运行后提示“已在终端 xxx 执行”

## 6.3 文件右键 Run

改动：
1. `agent_workspace/src/components/projectExplorer/Overlays.tsx`
2. `agent_workspace/src/components/projectExplorer/ProjectExplorerFilesWorkspace.tsx`
3. `agent_workspace/src/components/ProjectExplorer.tsx`

新增菜单项：
1. `运行文件`（按扩展名条件显示）
2. 触发 `/run/execute-file`

---

## 7. 关键规则（避免歧义）

1. 运行目录选择：
- 目标/文件所在目录优先；
- 无明确目录时用项目根。

2. 终端复用规则：
- 仅复用 `cwd` 精确匹配且 `busy=false` 的终端。
- 否则创建新终端。

3. 灰变绿条件：
- 自动检测有目标，或用户手动添加自定义目标。

4. 多目标默认策略：
- 若只有一个自动设默认。
- 多个需用户确认默认（首次弹窗）。

---

## 8. 分阶段实施

## 阶段 A（先把你的核心体验做出来）

1. `run catalog` 持久化 + 状态机
2. 新增项目后自动分析
3. 项目列表播放按钮灰/绿
4. `execute` 走终端调度

## 阶段 B（复杂目录与多目标）

1. monorepo 深度扫描
2. 多目标选择 + 默认目标设置
3. 手动重扫

## 阶段 C（单文件右键 Run）

1. 右键菜单 `运行文件`
2. `execute-file` + 环境检查

## 阶段 D（增强）

1. 自定义任务文件 `.agent_workspace/tasks.json`
2. 解析器增强（可选，不阻塞主链路）

---

## 9. 验收清单（按你的原话对齐）

1. 添加项目后会自动解析分析启动方式。
2. 未检测出时播放按钮灰色不可点。
3. 检测出时播放按钮绿色可点。
4. 同一目录多可启动项目可选择并设默认。
5. `python/js` 单文件右键有 `Run`。

---

## 10. 我建议的实际开工顺序

1. 先做阶段 A（你立刻能看到灰/绿和可运行）。
2. 再做阶段 B（多目标/monorepo）。
3. 然后阶段 C（文件右键 Run）。

这样你每天都能看到可验证成果，不会变成一个大而慢的重构项目。
