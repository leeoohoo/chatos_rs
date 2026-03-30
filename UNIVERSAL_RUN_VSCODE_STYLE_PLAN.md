# 通用一键运行方案（对标 VSCode，可落地版）

## 1. 先回答你的关键问题

## 1.1 要不要先装“代码解析器”

结论：**第一阶段不需要**。

原因：
1. VSCode 的 Run/Task 主体并不依赖 AST 解析器，而是“任务系统 + 检测规则 + 用户可编辑配置”。
2. 你要的“覆盖所有流行语言”，核心瓶颈是运行时环境（JDK/Node/Python/Go 等）和命令模板，不是语法解析。
3. 解析器（Tree-sitter/LSP）适合做“更智能的入口猜测”，可以作为后续增强，不应该阻塞主功能上线。

## 1.2 怎么做到“尽量支持所有流行语言”

结论：用 **VSCode 风格的任务模型**：
1. 先做通用任务引擎（language-agnostic）。
2. 再做语言适配器（Node/Python/Java/Go/Rust/.NET/PHP/Ruby...）。
3. 最后加“用户自定义任务”。

这样即使某语言自动识别失败，用户也能直接运行命令，不会卡死在“未支持”。

---

## 2. 设计原则（对标 VSCode，但贴合你现有架构）

1. **任务优先，不做硬编码按钮**：按钮只触发任务。
2. **检测与执行分离**：`detect` 只给候选；`run` 才执行。
3. **终端调度统一**：同目录空闲复用，否则创建。
4. **环境显式检查**：运行前告诉用户缺什么（例如 `java` 未安装）。
5. **可覆盖策略**：项目内 `.chatos/tasks.json` > 自动检测。
6. **共享执行内核**：API 与 AI 工具复用同一 terminal dispatcher。

---

## 3. 与当前代码的复用点（很关键）

你现在已经有可复用基础：

1. 终端状态与 busy：
- `chat_app_server_rs/src/api/terminals/*`
- `chat_app_server_rs/src/services/terminal_manager/*`

2. 空闲终端复用逻辑（已存在）：
- `chat_app_server_rs/src/builtin/terminal_controller/actions.rs`
- `find_idle_terminal` + `manager.create` + `ensure_running` + `write_input`

3. 项目上下文解析：
- `chat_app_server_rs/src/builtin/terminal_controller/context.rs`

结论：最优路径不是新造一套，而是把 `terminal_controller` 的调度逻辑抽成共享服务。

---

## 4. 总体架构（V1 -> V3）

## 4.1 V1（最快可用）

目标：先把“一键运行”跑通。

1. 新增任务检测 API：`POST /api/project-run/detect`
2. 新增任务执行 API：`POST /api/project-run/execute`
3. 前端项目预览区新增“运行”按钮 + 候选命令列表 + 自定义命令输入
4. 后端执行逻辑：复用空闲终端，否则创建，再写命令

## 4.2 V2（覆盖主流语言）

1. 语言适配器扩展到 8~10 类生态
2. 新增环境检查 API：`POST /api/project-run/check-env`
3. 提供缺失依赖提示与一键复制安装命令

## 4.3 V3（智能增强）

1. 可选接入 Tree-sitter/LSP 做入口推断
2. 支持“运行当前文件/当前测试”
3. 任务历史、收藏、最近成功命令排序

---

## 5. 数据模型（VSCode 风格）

## 5.1 项目任务文件

路径：`<project_root>/.chatos/tasks.json`

示例：
```json
{
  "version": 1,
  "tasks": [
    {
      "id": "web.dev",
      "label": "Web: npm run dev",
      "cwd": ".",
      "command": "npm run dev",
      "group": "run",
      "langs": ["javascript", "typescript"],
      "env": {
        "NODE_ENV": "development"
      }
    },
    {
      "id": "java.boot",
      "label": "Java: mvn spring-boot:run",
      "cwd": ".",
      "command": "mvn spring-boot:run",
      "group": "run",
      "langs": ["java"]
    }
  ]
}
```

## 5.2 运行请求模型

```json
{
  "project_id": "...",
  "cwd": "/abs/path/or/relative",
  "task_id": "web.dev",
  "command": "npm run dev",
  "prefer_existing_terminal": true,
  "create_if_missing": true
}
```

规则：
1. `task_id` 与 `command` 至少一个。
2. 有 `task_id` 时从任务定义取命令；`command` 可作为覆盖。

---

## 6. API 设计（后端）

## 6.1 检测候选任务

`POST /api/project-run/detect`

入参：`project_id`, `path`（可选）

出参：
1. `detected_tasks`: 自动检测候选
2. `custom_tasks`: `.chatos/tasks.json` 中任务
3. `recommended`: 推荐任务 id 列表
4. `hints`: 识别依据（命中 `package.json` / `pom.xml` 等）

## 6.2 执行任务

`POST /api/project-run/execute`

入参：上面的运行请求模型。

出参：
1. `terminal_id`
2. `terminal_reused`
3. `executed_command`
4. `cwd`
5. `status`: `dispatched`

注意：V1 不等待执行完成，只负责派发到终端。

## 6.3 环境检查

`POST /api/project-run/check-env`

入参：`project_id`, `task_id` 或 `command`

出参：
1. `ok`: bool
2. `missing`: 例如 `node`, `npm`, `java`, `mvn`
3. `suggestions`: 安装提示（按系统给建议）

---

## 7. 后端实现方案（具体到模块）

## 7.1 新增服务模块

建议新增：
1. `chat_app_server_rs/src/services/project_run/mod.rs`
2. `.../detector.rs`（检测任务）
3. `.../executor.rs`（终端调度执行）
4. `.../env_check.rs`（环境检查）
5. `.../task_file.rs`（读写 `.chatos/tasks.json`）
6. `.../types.rs`（请求/响应结构）

## 7.2 抽取共享终端调度内核

从以下位置抽到共享层（避免重复逻辑）：
1. `builtin/terminal_controller/actions.rs`
2. `builtin/terminal_controller/context.rs`

抽取目标：
1. `select_idle_terminal(user_id, project_id, cwd)`
2. `create_terminal_if_needed(...)`
3. `dispatch_command_to_terminal(...)`

然后：
1. `builtin terminal_controller` 继续用它
2. 新 `project-run execute` 也用它

## 7.3 路由与控制器

新增 API 文件：
1. `chat_app_server_rs/src/api/project_run.rs`

并在：
1. `chat_app_server_rs/src/api/mod.rs`
中注册 `merge(project_run::router())`

---

## 8. 前端实现方案（具体到文件）

## 8.1 API client

改动：
1. `chat_app/src/lib/api/client/workspace.ts`（新增 `detectProjectRunTasks` / `executeProjectRunTask` / `checkProjectRunEnv`）
2. `chat_app/src/lib/api/client.ts` 暴露方法

## 8.2 项目预览 UI

改动：
1. `chat_app/src/components/projectExplorer/PreviewPane.tsx`

新增：
1. 运行按钮
2. 候选任务下拉
3. 自定义命令输入
4. “在终端中查看”快捷动作

## 8.3 运行配置辅助

新增：
1. `chat_app/src/components/projectExplorer/runProfiles.ts`
2. `chat_app/src/components/projectExplorer/useProjectRun.ts`

---

## 9. 语言适配器清单（V2 建议）

按优先级：

1. Node/Web
- 识别：`package.json`
- 候选：`npm run dev/start/test/build`

2. Python
- 识别：`pyproject.toml`, `requirements.txt`, `main.py`, `app.py`
- 候选：`python main.py`, `uv run`, `pytest`

3. Java (Maven/Gradle)
- 识别：`pom.xml`, `build.gradle`, `gradlew`
- 候选：`mvn spring-boot:run`, `mvn test`, `./gradlew bootRun`, `./gradlew test`

4. Go
- 识别：`go.mod`
- 候选：`go run .`, `go test ./...`

5. Rust
- 识别：`Cargo.toml`
- 候选：`cargo run`, `cargo test`

6. .NET
- 识别：`*.csproj`, `*.sln`
- 候选：`dotnet run`, `dotnet test`

7. PHP
- 识别：`composer.json`
- 候选：`php -S 0.0.0.0:8000 -t public`, `composer test`

8. Ruby
- 识别：`Gemfile`
- 候选：`bundle exec ruby app.rb`, `bundle exec rspec`

说明：这已经覆盖大多数“流行语言项目”的主干路径。

---

## 10. 为什么这比“先上解析器”更可行

1. 交付速度快：2~4 天可以出 V1 闭环。
2. 风险低：复用现有 terminal manager，不动核心协议。
3. 可持续：后续逐步加适配器，不需要一次性完成“全语法理解”。
4. 与 VSCode 一致：Task-first + 用户可覆盖。

---

## 11. 风险与规避

1. 命令误判
- 规避：检测只给建议，执行前允许用户确认或编辑。

2. 终端并发抢占
- 规避：执行前再次判断 busy；必要时乐观锁或短时互斥。

3. 环境不一致（本机/容器/WSL）
- 规避：`check-env` 显式告警，不隐式失败。

4. 长时进程（dev server）
- 规避：统一放到终端运行，不在 API 层等待完成。

---

## 12. 分阶段里程碑（建议）

## 里程碑 A（V1，先可用）

1. `detect + execute` API
2. PreviewPane 运行按钮
3. 终端复用/创建调度
4. Node + Java + Python 基础适配

## 里程碑 B（V2，广覆盖）

1. `check-env`
2. Go/Rust/.NET/PHP/Ruby 适配
3. `.chatos/tasks.json` 自定义任务

## 里程碑 C（V3，智能化）

1. 解析器可选接入
2. 当前文件/当前测试运行
3. 历史与收藏

---

## 13. 验收标准

1. 用户在项目页可直接一键运行命令，不需要先切终端手输。
2. 调度符合规则：优先复用同目录空闲终端；否则创建。
3. 主流语言项目均可自动检测到至少一个可运行候选。
4. 环境缺失时有可读错误与建议，不是静默失败。
5. API 与 AI 工具复用同一执行内核，不出现行为分叉。

---

## 14. 下一步落地建议

1. 先按里程碑 A 实现最小闭环。
2. 你确认后，我可以直接开始改代码，先从：
- 抽取共享 terminal dispatcher
- 新增 `/api/project-run/detect` 与 `/api/project-run/execute`
- 前端 `PreviewPane` 运行入口
