# 项目目录能力升级方案（文件后缀显示 + 一键运行）

## 1. 需求目标

1. 补齐“很多文件后缀不能显示”的问题，让更多文本文件可直接预览。
2. 在项目文件预览区增加“运行”能力：
- 例如 Java 后端、Web `npm run`。
- 运行时自动复用“当前目录下空闲终端”；没有空闲终端则自动创建当前目录终端。

## 2. 现有代码调研结论

### 2.1 文件预览链路

1. 前端预览入口：
- `agent_workspace/src/components/projectExplorer/PreviewPane.tsx`
- 文本文件使用 `highlight.js` 渲染；图片二进制做 data-url 预览；其它二进制只显示“下载”。

2. 前端语言映射：
- `agent_workspace/src/components/projectExplorer/utils.ts` 中 `EXT_LANGUAGE_MAP` + `getHighlightLanguage`。

3. 后端读文件与“文本/二进制”判定：
- `agent_orchestrator/src/api/fs/query_handlers.rs` 的 `/api/fs/read`
- `agent_orchestrator/src/api/fs/read_mode.rs` 的 `should_render_text`

### 2.2 终端能力链路

1. 终端 CRUD + WS：
- `agent_orchestrator/src/api/terminals.rs`
- `agent_orchestrator/src/api/terminals/crud_handlers.rs`
- `agent_orchestrator/src/api/terminals/ws_handlers.rs`

2. 终端 busy 状态来源：
- `agent_orchestrator/src/services/terminal_manager/session.rs`
- `agent_orchestrator/src/api/terminals/support.rs`（`attach_busy`）

3. 前端终端状态与操作：
- `agent_workspace/src/lib/store/actions/terminals.ts`
- `agent_workspace/src/components/TerminalView.tsx`
- `agent_workspace/src/components/terminal/useTerminal*`

## 3. 当前缺口（差什么）

## 3.1 后缀显示问题的真实缺口

1. 后端 `should_render_text` 目前是“UTF-8 + (mime/后缀/文件名白名单)”才当文本。
2. 这会把很多“确实是文本”的文件误判为二进制（前端自然无法显示）：
- 例如当前仓库已有 `Dockerfile.backend`、`Dockerfile.frontend`（后缀不在白名单）。
- 还有 `*.tpl`、`*.example`、`*.typed` 等非标准后缀文本文件。
3. 前端 `EXT_LANGUAGE_MAP` 只影响“高亮质量”，不是“能否显示”的根因。

结论：核心瓶颈在后端文本识别策略过严，前端映射是次级问题。

## 3.2 一键运行能力的缺口

1. 项目预览区没有“运行”按钮与运行面板。
2. 当前只有终端 WS 输入链路，没有“从项目页直接派发命令”的 API。
3. “找当前目录空闲终端，否则创建”这套编排目前不存在。
4. 终端有 `busy` 信息，但未被项目页用于自动调度执行。

## 4. 方案设计

## 4.1 文件后缀显示增强（分两层）

### A. 后端文本判定改造（关键）

目标：只要文件是“高概率文本”，就返回 `is_binary=false` 给前端。

改造点：`agent_orchestrator/src/api/fs/read_mode.rs`

策略建议：
1. 保留现有白名单命中逻辑（快速路径）。
2. 新增“宽松文本启发式”兜底：
- UTF-8 可解码。
- 不包含 `\0`。
- 控制字符占比低于阈值（例如 `< 2%`，排除 `\n\r\t`）。
3. 若命中兜底，则即使后缀未知也按文本返回。

收益：一次性覆盖大量未知后缀文本，不需要不停加后缀。

### B. 前端高亮映射补齐（体验）

改造点：`agent_workspace/src/components/projectExplorer/utils.ts`

1. 扩展常见后缀映射（只做高亮，不影响是否显示）：
- `mts/cts`、`tsx` 已有则保留；补 `vue` 生态与脚本类边缘后缀。
- 配置类：`toml`, `ini`, `editorconfig`, `gitignore` 等名称/后缀增强。
2. 名称级补充：`Dockerfile.*`、`*.env.*`、`Jenkinsfile`、`Procfile`、`nginx.conf` 等。

## 4.2 一键运行设计（项目页触发）

### A. 交互设计

改造点：`agent_workspace/src/components/projectExplorer/PreviewPane.tsx`

1. 在预览头部新增“运行”区域：
- 主按钮：`运行`。
- 下拉按钮：显示可选命令模板（按项目类型动态生成）。
2. 运行后反馈：
- Toast / 文案：`已在终端 xxx 执行`。
- 提供 `查看终端` 快捷入口（不强制页面跳转）。

### B. 命令模板来源（可扩展）

新增前端模块建议：`agent_workspace/src/components/projectExplorer/runProfiles.ts`

按“当前目录”探测：
1. Node/Web：存在 `package.json` 时解析 scripts，给出 `npm run <script>` 候选。
2. Java：
- 有 `pom.xml` -> `mvn spring-boot:run` / `mvn test`。
- 有 `build.gradle` 或 `gradlew` -> `./gradlew bootRun` / `./gradlew test`。
3. 兜底：自定义输入命令后执行。

### C. 终端自动调度（核心编排）

新增后端 API（推荐）
1. `POST /api/terminals/dispatch-command`（名称可再定）
2. 请求体建议：
- `cwd`（必填）
- `command`（必填）
- `project_id`（可选）
- `create_if_missing`（默认 true）
3. 服务端流程：
- 按当前用户查询终端。
- 过滤 `status=running && busy=false && cwd==目标cwd`。
- 有候选：选最近活动终端。
- 无候选：创建新终端（cwd=目标cwd）。
- `ensure_running` 后写入 `command + '\n'`。
- 返回 `terminal_id/terminal_name/cwd/executed_command`。

这样项目页无需直接持有 Terminal WS，也能可靠触发执行。

### D. 前端调用层

改造点：
1. `agent_workspace/src/lib/api/client/workspace.ts`：新增 `dispatchTerminalCommand`。
2. `agent_workspace/src/lib/api/client.ts`：暴露 `dispatchTerminalCommand` 方法。
3. `ProjectExplorer` 层把“当前目录 + 命令”传给新 API。

## 5. 关键规则定义

1. “当前目录”定义：
- 选中文件 -> 文件父目录。
- 选中目录 -> 该目录。
- 未选中 -> 项目根目录。

2. 复用终端规则：
- 仅复用 `cwd` 精确匹配且 `busy=false` 的 running 终端。
- 没有则创建，不复用不同目录终端（避免污染上下文）。

3. 执行行为：
- 默认 fire-and-forget（触发即返回，不等待命令完成）。
- 结果查看通过终端面板实时输出。

## 6. 改动文件清单（计划）

### 后端

1. 文本判定：
- `agent_orchestrator/src/api/fs/read_mode.rs`

2. 终端派发 API：
- `agent_orchestrator/src/api/terminals.rs`（新增 route）
- `agent_orchestrator/src/api/terminals/contracts.rs`（新增请求/响应结构）
- `agent_orchestrator/src/api/terminals/crud_handlers.rs` 或新增 `dispatch_handlers.rs`
- 可能补充：`agent_orchestrator/src/repositories/terminals.rs`（如需更高效查询）

### 前端

1. 预览与运行 UI：
- `agent_workspace/src/components/projectExplorer/PreviewPane.tsx`
- `agent_workspace/src/components/ProjectExplorer.tsx`
- 新增：`agent_workspace/src/components/projectExplorer/runProfiles.ts`

2. API 客户端：
- `agent_workspace/src/lib/api/client/workspace.ts`
- `agent_workspace/src/lib/api/client.ts`

3. 高亮映射补充：
- `agent_workspace/src/components/projectExplorer/utils.ts`

## 7. 分阶段实施建议

## 阶段 1：先修“后缀不能显示”

1. 改 `read_mode` 启发式兜底。
2. 补充前端 `EXT_LANGUAGE_MAP`。
3. 验证 `Dockerfile.backend`、`*.tpl`、`*.example` 可文本预览。

## 阶段 2：落地一键运行最小闭环

1. 后端新增 `dispatch-command` API。
2. 前端预览区加入“运行按钮 + 自定义命令输入”。
3. 完成“空闲同目录终端复用，否则创建”的调度。

## 阶段 3：增强模板与体验

1. `package.json scripts` 自动读取并生成候选。
2. Java(Maven/Gradle) 模板识别。
3. 运行记录与“最近命令”快捷入口。

## 8. 验收标准

1. 文本预览：未知后缀文本文件不再被误判为二进制。
2. 运行调度：
- 有空闲同目录终端时复用该终端。
- 无空闲同目录终端时自动创建并执行。
3. UI 体验：用户在项目页可一键触发运行，不需要手动切到终端输入。
4. 安全与稳定：仅允许当前用户操作自己的终端，非法 terminal/cwd 返回明确错误。

## 9. 风险与注意点

1. 文本启发式过宽可能把极少数二进制误判为文本，需要控制阈值并加测试样本。
2. 并发点击“运行”可能抢同一空闲终端，后端调度要保证幂等和可观测日志。
3. Java 启动命令差异较大（Spring/普通 Java 项目），模板需允许用户一键改命令。

## 10. 建议先后顺序（你确认后我就按这个做）

1. 先做阶段 1（立刻提升可见性，风险低）。
2. 再做阶段 2（一键运行闭环）。
3. 最后做阶段 3（模板智能化与体验优化）。
