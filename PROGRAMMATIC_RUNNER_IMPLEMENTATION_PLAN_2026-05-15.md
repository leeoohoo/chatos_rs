# 程序化启动 / 停止 / 重启实施方案

## 背景

当前项目的启动链路仍然依赖 AI 生成 `./.chatos/project_runner.sh`。前端按钮只是派发脚本，真正的启动逻辑来自提示词生成，导致结果不稳定、不可预测，也很难复用已有的语言解析能力。

## 现状

- `chat_app/src/lib/domain/projectRunner.ts` 负责拼装“生成启动脚本”的 AI 提示词。
- `chat_app/src/components/projectExplorer/useProjectRunnerScriptGenerator.ts` 会把该提示词发给联系人会话。
- `chat_app/src/components/projectExplorer/runState/useProjectRunnerCommands.ts` 只会执行 `bash ./.chatos/project_runner.sh start|stop|restart`。
- `chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts` 目前只检查脚本是否存在，不负责分析入口。
- `chat_app_server_rs/src/services/code_nav/*` 已经有 Java、Rust、Go、Python、TS 等语言的解析与符号分析基础。
- `chat_app_server_rs/src/services/workspace_realtime_watcher.rs` 已经在监听运行态相关文件变化。

## 目标

把“启动脚本生成”改成纯程序化能力，做到：

- 不再依赖 AI 生成启动文件。
- 自动识别项目入口、运行命令、日志目录、PID 文件。
- 支持 `start / stop / restart / status`。
- `stop` 只按本项目维护的 PID 退出，不按端口全局误杀。
- Java、Rust 优先落地，其他语言按同一框架扩展。
- 打开某种语言项目时，允许用户手动配置该语言所需环境，例如 JDK、Maven、Gradle、Cargo、Node、Python 解释器等。

## 方案

### 1. 增加运行计划生成器

新增一个确定性的 `RunPlanBuilder`，输入为项目根目录，输出为结构化运行计划：

- 语言类型
- 启动目标列表
- 每个目标的 cwd / command / env / port / log / pid
- 依赖顺序
- 健康检查方式
- 环境依赖检查结果

### 2. 复用现有解析能力

- Java：识别 `pom.xml` / `build.gradle` / `settings.gradle`，扫描 `src/main/java`，定位 `public static void main`、Spring Boot 主类。
- Rust：识别 `Cargo.toml`，解析 `src/main.rs`、`src/bin/*.rs`、`[[bin]]`，生成 `cargo run` / `cargo run --bin`。
- 其他语言：沿用同样的“manifest + 入口文件 + 默认启动命令”规则。

### 3. 增加语言专属环境配置

新增一层“语言运行环境”配置，按项目或按用户保存，UI 在打开对应语言项目时自动展示：

- Java：`JAVA_HOME`、`JDK` 版本、`mvn` 路径、`gradle` 路径、是否优先使用 wrapper。
- Rust：`cargo`、`rustup`、`rustc` 路径，是否优先使用 `cargo run` / `cargo run --bin`。
- Node：`node`、`npm`、`pnpm`、`yarn` 路径，是否优先使用 lockfile 对应包管理器。
- Python：`python` / `python3` 解释器路径、虚拟环境目录、是否优先激活 venv。
- Go：`go` 路径、`GOMODCACHE`、`GOPROXY` 等基础环境变量。
- 多版本工具链：如果本机检测到多个 `JDK`、`Python`、`Node` 等版本，前端用下拉框列出可选项，让用户直接选定具体版本或安装路径。

配置原则：

- 语言默认值从项目结构自动推断。
- 用户可覆盖默认值。
- 项目级配置优先于全局配置。
- 配置缺失时仍能给出可执行的探测建议。
- 下拉框候选项来自本机自动探测结果，保留“手动输入路径”作为兜底。

### 4. 让 UI 显式暴露这些配置

- 在项目预览区或运行面板里增加“环境设置”入口。
- 识别到 Java / Rust 等项目时，自动展开对应语言的环境卡片。
- 提供“自动检测”“下拉选择已安装版本”“手动填写”“恢复默认”四种操作。
- 在启动前展示最终生效的工具链和命令预览。

### 5. 先做兼容层，再去 AI

短期保留 `.chatos/project_runner.sh` 这个 UI 兼容点，但改成程序生成，不再走 AI。
后续再把前端按钮直接切到后端运行计划接口，彻底取消脚本依赖。

### 6. 重构前端状态

- “生成启动脚本”改成“分析运行方案”。
- `runnerScriptExists` 改为“运行方案是否可用”。
- 启动/停止/重启按钮直接绑定结构化运行目标。
- `runStatus` 需要额外表达“环境缺失”“工具链未配置”“自动检测失败”等状态。

## 落地步骤

1. 后端新增运行计划扫描器、语言策略和环境配置模型。
2. 为 Java / Rust 写入口识别规则、环境探测和测试。
3. 增加用户/项目级配置存储与读取接口。
4. 生成 `.chatos/runtime/runner/` 下的计划、PID、日志文件。
5. 用程序生成的脚本替换 AI 生成脚本。
6. 前端切换到“分析后直接启动”，并提供环境设置面板。
7. 清理 AI 提示词与相关联系人会话依赖。

## 验收标准

- Java / Rust 项目无需 AI 即可启动。
- `start / stop / restart` 行为稳定且可重复。
- 同一项目多次分析结果一致。
- `stop` 不会误伤其他项目进程。
- 日志、PID、端口配置都可追踪。
- 用户可在 UI 里修改 Java / Rust 等语言的工具链配置并立即生效。
- 启动前能明确提示当前使用的 JDK / Maven / Gradle / Cargo 等环境来源。
- 当系统里有多个同类工具链版本时，能直接从下拉框选择，而不是手工输入路径。

## 风险

- 复杂项目可能存在多入口，需要做优先级规则。
- 旧项目可能没有标准 manifest，需要保留人工兜底。
- 迁移期要兼容现有 UI 和状态推送。
- 环境配置过多会让 UI 变复杂，需要做语言感知的分组展示。

## 建议优先级

1. 先做 Java / Rust。
2. 再补 Node / Python / Go。
3. 最后移除 AI 生成脚本链路。
