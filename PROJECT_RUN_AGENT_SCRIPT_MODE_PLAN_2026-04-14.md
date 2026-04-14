# 项目运行能力改造方案（团队成员固定提示词生成脚本模式）

日期：2026-04-14  
作者：Codex（按当前仓库实现复核后）

## 1. 目标（按你的需求落地）

把“自动分析可运行命令”的模式改为“人工触发 AI 生成项目启动脚本”的模式：

1. 不再自动探测/自动重扫运行目标。
2. 用户点击按钮后，向指定团队成员发送固定提示词。
3. 该提示词要求团队成员在项目根目录生成一个可一键运行脚本。
4. 当脚本存在时，UI 只展示固定动作：`启动` / `停止` / `重启`。
5. `启动` 自动复用空闲终端（无可用则创建）并执行固定命令。
6. 团队成员门禁：
   - 0 人：提示“请先添加联系人”并阻断执行。
   - 1 人：直接使用该成员。
   - 多人：允许用户任选一位执行。

---

## 2. 当前实现复核（关键证据）

### 2.1 自动分析入口（需要去掉）

1. 项目创建后自动异步分析：  
   `chat_app_server_rs/src/api/projects/crud_handlers.rs:99-108`
2. 获取 run catalog 时若无缓存会自动分析：  
   `chat_app_server_rs/src/api/projects/run_handlers.rs:13-23`，`47-56`
3. 前端 Project Explorer 打开项目后默认调用 analyze：  
   `chat_app/src/components/projectExplorer/useProjectExplorerRunState.ts:109-118`
4. 侧边栏项目列表会批量拉取 run catalog：  
   `chat_app/src/components/sessionList/useProjectRunState.ts:141-167`

### 2.2 现有运行 UI（将被脚本模式替换）

1. 预览区有“目标下拉 + 手输命令 + 重扫目标”：  
   `chat_app/src/components/projectExplorer/PreviewPane.tsx:195-251`
2. 侧边栏项目卡使用“目标数量/状态”控制按钮：  
   `chat_app/src/components/sessionList/sections/ProjectSection.tsx:89-205`

### 2.3 可复用能力（本方案直接复用）

1. “复用空闲终端，否则创建”能力已存在：  
   `chat_app_server_rs/src/api/terminals/crud_handlers.rs:194-235`  
   `chat_app_server_rs/src/services/project_run/mod.rs:608-644`
2. 项目联系人（团队成员）接口已存在：  
   `GET/POST /api/projects/:id/contacts`，`DELETE /api/projects/:id/contacts/:contact_id`  
   `chat_app_server_rs/src/api/projects.rs:33-39`
3. 前端已可获取项目成员列表：  
   `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts:143-174`

---

## 3. 目标交互（产品行为）

### 3.1 统一脚本约定

推荐脚本落点（单文件协议）：

1. 绝对路径：`${project.rootPath}/.chatos/project_runner.sh`
2. 相对项目根目录：`./.chatos/project_runner.sh`

运行产物目录（脚本负责创建）：

1. 日志目录：`${project.rootPath}/project_runner/logs/`
2. 进程标记目录：`${project.rootPath}/project_runner/pids/`

固定命令（以 `cwd = project.rootPath` 执行）：

1. 启动：`bash ./.chatos/project_runner.sh start`
2. 停止：`bash ./.chatos/project_runner.sh stop`
3. 重启：`bash ./.chatos/project_runner.sh restart`

说明：  
只要 `${project.rootPath}/.chatos/project_runner.sh` 存在，前端即判定“可运行”，并显示 `启动/停止/重启`。

### 3.2 运行条状态机

建议状态：

1. `no_member`：无团队成员，显示“请先添加联系人”。
2. `script_missing`：有成员但脚本不存在，显示“生成启动脚本”按钮。
3. `generating`：已提交给团队成员执行，展示进行中状态（可轮询脚本是否出现）。
4. `ready`：脚本存在，展示 `启动/停止/重启`。
5. `running`：可从 terminal busy 状态推断；按钮表现与现在一致（运行中时重点显示停止）。
6. `error`：展示错误并允许重试。

### 3.3 团队成员选择规则

1. 成员数 `0`：按钮置灰，文案“请先添加一个联系人再执行”。
2. 成员数 `1`：点击“生成启动脚本”直接执行。
3. 成员数 `>1`：弹出成员选择器（可复用现有联系人选择 UI 思路），任选其一。

---

## 4. 固定提示词设计（核心）

点击后发送固定 prompt（变量替换）：

- `${PROJECT_NAME}`
- `${PROJECT_ROOT}`
- `${SCRIPT_REL_PATH}`（固定：`.chatos/project_runner.sh`）
- `${RUNNER_LOG_DIR}`（固定：`project_runner/logs`）

建议模板：

```text
你是项目运行脚本生成助手。请在项目根目录 ${PROJECT_ROOT} 下创建文件 ${SCRIPT_REL_PATH}。

目标：
1) 生成一个 bash 脚本，支持参数 start / stop / restart。
2) start: 启动当前项目下所有可启动服务（前端/后端/worker 等都包含，能启动的都要启动）。
3) stop: 停止 start 启动的全部进程（使用 pid 文件优先，避免误杀非本脚本启动进程）。
4) restart: 等价于 stop + start。
5) 所有服务日志写入 ${PROJECT_ROOT}/${RUNNER_LOG_DIR}/ 目录。

强制要求：
1) 先读取项目关键文件（如 package.json / pyproject.toml / Cargo.toml / go.mod / pom.xml 等）再决策。
2) 可使用终端工具做必要探测（例如检查命令是否存在）。
3) 生成脚本必须可执行（#!/usr/bin/env bash，set -euo pipefail）。
4) 必须创建日志目录 ${PROJECT_ROOT}/${RUNNER_LOG_DIR}/，并按服务拆分日志文件（例如 frontend.log、backend.log）。
5) 若无法确定某服务启动命令，要在日志与注释里明确标记该服务待人工补充，但其他可启动服务仍需正常启动。
6) 输出完成后，回复“脚本已生成: ${SCRIPT_REL_PATH}”。
```

工具约束（由前端 runtime 注入）：

1. `mcpEnabled: true`
2. `enabledMcpIds` 至少包含：
   - `builtin_code_maintainer_read`
   - `builtin_code_maintainer_write`
   - `builtin_terminal_controller`

注：这些 MCP ID 已在前端项目上下文中是“项目相关内置工具”集合的一部分：  
`chat_app/src/components/inputArea/useMcpSelection.ts:4-9`

---

## 5. 前端改造方案

## 5.1 Project Explorer 运行条改造（主入口）

目标文件：

1. `chat_app/src/components/projectExplorer/PreviewPane.tsx`
2. `chat_app/src/components/projectExplorer/useProjectPreviewRunController.ts`
3. `chat_app/src/components/projectExplorer/useProjectExplorerRunState.ts`
4. `chat_app/src/components/projectExplorer/useProjectExplorerWorkspaceView.ts`
5. `chat_app/src/components/ProjectExplorer.tsx`

改造点：

1. 删除“目标下拉 + 手动命令 + 重扫目标”交互。
2. 新增“脚本模式”状态与按钮组。
3. 点击“生成启动脚本”后：
   - 先做团队成员门禁；
   - 选定成员；
   - 发送固定提示词；
   - 进入 `generating` 状态；
   - 轮询脚本存在性（存在则切 `ready`）。
4. `ready` 状态展示 `启动/停止/重启`，命令固定为脚本命令。

## 5.2 侧边栏项目运行按钮改造（保持一致）

目标文件：

1. `chat_app/src/components/sessionList/useProjectRunState.ts`
2. `chat_app/src/components/sessionList/sections/ProjectSection.tsx`

改造点：

1. 不再依赖“run targets 数量”判断可运行性。
2. 改为依赖“脚本是否存在 + 团队成员门禁”。
3. 按同一状态机显示按钮（启动/停止/重启或生成脚本）。

## 5.3 团队成员与消息发送复用

可复用链路：

1. 项目成员：`useProjectMembersManager`（已有）
2. 发送消息：已有 `sendMessage` runtime 可传 `contactAgentId/projectRoot/mcpEnabled/enabledMcpIds`

建议将“发送固定提示词”抽成共享 action（避免仅 TeamMembersPane 可用）：

1. 新增 `projectExplorer/runScript/useRunScriptGenerator.ts`（或 store action）
2. 输入：`projectId/projectRoot/contactId/contactAgentId`
3. 内部：确保对应 contact session，再发固定 prompt。

---

## 6. 后端改造方案

## 6.1 关闭自动分析（满足“不要自动找命令”）

1. 去掉项目创建后的 `tokio::spawn(analyze_project)`：  
   `chat_app_server_rs/src/api/projects/crud_handlers.rs:99-108`
2. `GET /api/projects/:id/run/catalog` 改为“只读缓存，不自动 analyze”：  
   `chat_app_server_rs/src/api/projects/run_handlers.rs:13-23` / `47-56`

这样做后，系统不再隐式扫描项目。

## 6.2 新增脚本模式状态接口（建议）

新增：

1. `GET /api/projects/:id/run/script/status`
2. `POST /api/projects/:id/run/script/mark-generating`
3. `POST /api/projects/:id/run/script/execute`（action: start|stop|restart）

`execute` 内部直接复用已有终端分发逻辑（空闲复用/自动创建）。

## 6.3 落库（长期状态在后端，不依赖前端缓存）

新增 `project_run_script_profiles`（Mongo/SQLite 双栈对齐）：

1. `project_id` (PK)
2. `user_id`
3. `script_rel_path`（默认 `.chatos/project_runner.sh`）
4. `script_exists`（布尔）
5. `generation_status`（idle/generating/ready/error）
6. `generator_contact_id`
7. `last_error`
8. `updated_at`

说明：  
前端只保留临时 UI 状态；权威状态由后端落库，符合“后端会落库，前端不应承担持久化”的原则。

---

## 7. 脚本存在性检查策略

前端可先采用“轻量无新接口”方案，后续再切后端状态接口：

1. 调 `listFsEntries(project.rootPath)` 或 `listFsEntries(project.rootPath + '/.chatos')`
2. 检查 `project_runner.sh` 是否存在
3. 结果写入本地状态并同步到后端 `script_exists`

最终建议：  
前端每次进入项目时调用后端 `run/script/status`，由后端统一返回 `exists + generation_status`，减少前端重复判断。

---

## 8. 开始/停止/重启执行约定

统一执行入口（前后端均可复用）：

1. `cwd = project.rootPath`
2. `project_id = currentProjectId`
3. `create_if_missing = true`
4. `command`（在 `cwd = project.rootPath` 下执行）：
   - start: `bash ./.chatos/project_runner.sh start`
   - stop: `bash ./.chatos/project_runner.sh stop`
   - restart: `bash ./.chatos/project_runner.sh restart`

脚本行为契约（关键）：

1. `start` 必须一次拉起“当前项目下所有可启动服务”，不是仅主服务。
2. 所有启动输出（stdout/stderr）必须写入 `${project.rootPath}/project_runner/logs/`。
3. `stop/restart` 仅管理脚本自己启动的进程（通过 `${project.rootPath}/project_runner/pids/` 识别）。

可选兜底：

1. 若 `stop` 命令失败且当前存在 busy terminal，可提示是否发送终端中断。

---

## 9. 兼容与迁移

1. V1 不删除旧 `/run/analyze`、`/run/catalog`、`/run/default`、`/run/execute(target)` 接口，先从 UI 隐藏旧入口。
2. 侧边栏与预览区同时切到脚本模式，避免两套运行逻辑并存。
3. 观测稳定后再决定是否下线“目标分析”代码路径。

---

## 10. 分阶段实施计划（推荐）

### Phase 1：后端与数据层

1. 新增 `project_run_script_profiles` 仓储与接口。
2. 关闭 run catalog 的自动分析副作用。
3. 保留旧接口兼容。

### Phase 2：Project Explorer 主链路

1. 替换 PreviewPane 运行条为脚本模式。
2. 接入团队成员门禁与成员选择。
3. 接入固定 prompt 发送 + 脚本存在轮询。

### Phase 3：Sidebar 同步

1. 侧边栏项目卡切脚本状态机。
2. 行为与 PreviewPane 完全一致。

### Phase 4：清理与收口

1. 去除不再使用的目标下拉/重扫 UI。
2. 补充文档与埋点。

---

## 11. 验收标准（必须全部通过）

1. 新建项目后不会自动跑“目标分析”。
2. 无团队成员时，运行入口明确提示需先添加联系人。
3. 有成员时，点击“生成启动脚本”会向成员发送固定提示词并带上指定内置工具。
4. 当 `${project.rootPath}/.chatos/project_runner.sh` 存在后，自动展示 `启动/停止/重启`。
5. `启动` 会拉起当前项目下所有可启动服务（含前后端），而不是只启动一个服务。
6. 日志目录 `${project.rootPath}/project_runner/logs/` 会自动创建，且每个服务都有独立日志文件。
7. `启动` 可复用空闲终端；无空闲终端自动创建。
8. 页面刷新后状态不丢失（以后端状态为准）。

---

## 12. 风险与规避

1. 风险：AI 生成脚本质量不稳定。  
   规避：固定模板 + 强制读取工程文件 + 命令兜底注释 + 失败后允许重新生成。
2. 风险：多人场景选错成员。  
   规避：生成前显式选择成员，并在状态中记录 `generator_contact_id`。
3. 风险：stop/restart 语义实现不一致。  
   规避：统一单脚本参数协议（start/stop/restart），前后端只执行固定命令不做猜测。
4. 风险：多服务并发启动导致日志难追踪。  
   规避：强制 `${project.rootPath}/project_runner/logs/` 按服务拆分日志命名，并约定统一时间戳前缀。
