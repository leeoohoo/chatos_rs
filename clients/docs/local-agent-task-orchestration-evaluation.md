# Local Agent 编排评测运行说明

本文配套 [Skill 架构与跨平台方案](local-agent-task-orchestration-skill-architecture.md)，覆盖 macOS 快照导出与 shared Rust 编排评测。Windows 使用同一数据集和 Rust 入口，另行实现本平台生产快照导出；不复制另一份评测 Prompt。

## 已实现的范围

- `clients/evals/task-orchestration/scenarios.json`：50 个合成中文场景，直接回答 10、单 Task 10、复杂图 20、followup 10。覆盖开发、设计、文案、调研/数据、文档/媒体、桌面与 Plugin。
- macOS `NativeTaskOrchestrationEvalExportTests`：调用生产 Skill loader、bootstrap、工具 schema 与 revision 算法，导出冻结的五个 profile。不是手写一份类似的 Prompt。
- Rust `orchestration_eval` example：使用真实 Main Chat planner、durable Profile、Skill gate、Task executor、SQLite 事务、DAG/写锁和 followup 规则。
- 默认离线回放 8 个代表场景，以及 11 个拒绝/恢复/检查器灵敏度控制样例。
- 显式 opt-in 的真实模型规划评测，逐场景输出实际 durable Task、图、tool ledger、检查项和人工验收 rubric。

离线脚本决定了调用什么工具，只能证明评测链路与运行时规则，不证明模型会正确拆分。任何模式的报告都保留 `model_quality_proven: false` 和 `semantic_status: pending_human_review`。结构检查通过后仍必须人工核对用户意图。

补充实机证据见 [FinFlow 集成验收记录](local-agent-finflow-integration-test.md)：实际应用、既有模型配置、native Task 与隔离财务业务执行；不属于本评测器的业务禁执行模式，也不替代50场景全量评测。记录明确区分旧安装版多任务图和最新构建烟测，并列出主会话最终报告展示的未满足项。

## 运行方法

从仓库根目录执行；需要 Node、Rust，自动导出快照还需要 macOS Swift/Xcode 工具链。

```bash
# 列出全部 50 个场景，不构建/导出客户端快照
node clients/evals/task-orchestration/run.mjs --list

# 自动使用生产 macOS loader 导出快照，然后离线回放；不调用模型
node clients/evals/task-orchestration/run.mjs --smoke --output /absolute/new-smoke-report.json

# 已有本平台导出的冻结快照时，跳过 macOS 导出
node clients/evals/task-orchestration/run.mjs --smoke \
  --fixture /absolute/frozen-capabilities.json --output /absolute/new-report.json

# 评测器自身的单元测试
cargo test -p chatos_local_agent_host --example orchestration_eval
```

`--output` 必须是不存在的新文件，不覆盖历史报告。不传时 JSON 输出到 stdout。退出码：0 结构检查通过，1 有结构/控制样例失败，2 参数、快照、环境或评测初始化错误。0 不代表拆分质量通过。

wrapper 的临时快照放在新建的独立临时目录中，退出后仅清理该目录。它不删除用户文件、项目、数据库或旧报告。

### 真实模型模式

只有明确运行 `--live --case ID` 或 `--live --all` 才会发模型请求并产生费用。本批实现验证没有运行此模式。

在专用环境中显式提供：

- `CHATOS_EVAL_BASE_URL`：Responses 兼容服务端点；使用现有 `chatos_ai_runtime`，不新增模型传输实现。
- `CHATOS_EVAL_API_KEY`：专门的评测凭证，仅放环境，不放参数、fixture、报告或仓库。
- `CHATOS_EVAL_MODEL`：评测模型名。
- 可选 `CHATOS_EVAL_PROVIDER`（默认 `gpt`）、`CHATOS_EVAL_THINKING_LEVEL`。

```bash
# 在已经显式配置上述环境变量的专用终端运行
node clients/evals/task-orchestration/run.mjs --live --case graph-auth \
  --output /absolute/new-live-report.json

node clients/evals/task-orchestration/run.mjs --live --all \
  --output /absolute/new-all-report.json
```

不读取应用登录态、Keychain、用户现有模型配置或应用数据库。端点默认只接受 HTTPS，拒绝 URL 中的账号密码、query 和 fragment。测试本地兼容服务时可显式用 `--allow-loopback-http`，只放行 localhost/loopback IP 的 HTTP；不是任意 HTTP 开关。

限制：每场景最多 16 次 measured 模型 step、每次 45 秒、每场景 180 秒、每请求最多 4096 输出 token、全量最多 800 次 measured 请求。followup 的 seed 阶段是确定性回放，不发模型请求，不计入测量。模型/传输异常后不自动重试，不继续下一场景；报告的 `completed_reports` 必须与请求的集合比对，不能把部分报告当全量完成。初始化错误退出 2，此时新建的输出文件可能为空，应换新文件名重新运行。

## 安全隔离边界

每个场景用独立内存 SQLite 和合成 Conversation。只注册八个 Task 工具及三个 `agent_skill_*`，不注册 Terminal、文件、Plugin、External 或 native business executor；不启动正常 Host Coordinator/Task 模型 scheduler。

Task 创建仍会真实持久化图、冻结选择，并 materialize queued Task Runs。这是验证真实调度/写锁的必要部分，但这些 Run 不执行模型或业务操作。评测器逐 step 手动 claim 后，在模型调用前断言 Run 必须为专用 Main Chat；若意外 claim 到其他 Run，立即停止。Main Chat 完成后不再 claim。模型返回的工具名还会再次与当前 planner 暴露的 Task/Skill allowlist 对照。

快照提供合成 Plugin Release 和不可访问的 External 地址，只用于能力选择与契约验证。`design@eval`、`media@eval` 是评测专用标识，不表示产品已经安装或提供对应能力。这里不验证 Plugin 成功执行、设计成品质量、渲染质量或 External 服务连接。

## 自动检查与人工检查

自动检查基于 durable 结果，不只是模型生成的工具名：

- 新 Task 数量（排除 followup seed）。
- objective、预期输出、验收标准非空；输出标签跨 Task 不重复。
- 实际 `tool_options` 在场景能力 allowlist 内。
- 工具结果必须成功，或属于已在后续模型 step 修正的确定性写入前错误；报告记录 `recovered_tool_errors`。`needs_review`、已开始写入的错误、未修正错误不能借此通过。
- 成功激活 orchestration Skill；在编写契约前完整阅读场景与能力 reference，创建图前完整阅读复杂度和依赖 reference，处理 followup 前读取对应 reference。分页覆盖按成功结果的 Unicode offset/returned_chars/total_chars 合并，半页或失败读取不计完整。读取必须在更早的模型 step 已完成；同一模型 step 并发发出读取和决策，不算模型已经依据返回正文编排。
- handoff 次数，以及 handoff 后没有 Task 调用。
- 复杂任务图真实持久化在同一 graph，有 graph goal/rationale；需要 synthesis 的场景存在一个可接收全部上游结果的下游节点。
- 根据共享 `local_task_runtime_write_scope` 和 `local_task_write_scope_overlap` 检查实际 active writer，继承 macOS 路径大小写/Unicode alias 与 Terminal/Plugin/External 的保守全范围语义。
- followup 决策实际提交；clarify 保持 held，cancel 取消旧 Task，continue 解除 held；替代 Task 保留 supersedes lineage。

人工 rubric 必须判断：是否按有意义的独立产物拆分；目标是否具体、完整；prerequisite 是否必要或缺失；验收是否真的可观察；synthesis 是否确实综合实际产物；能力是否最小，而非只是 allowlist 内；回答是否符合用户范围，是否误称已执行/已完成。

自动检查不使用关键词来冒充这些语义判断。rubric 与 expected counts 从不发送给模型；模型只收到用户 prompt、冻结生产 instructions/catalog 和真实工具结果。followup 初始化后，真实 guidance 机制附加新用户消息，实际生成待评审旧 Task。

### 拒绝与恢复控制

离线控制覆盖：未激活创建被 gate 拒绝、并行重叠 writer 被拒绝、业务工具不可见、handoff 后 Task 工具不可见、重复 handoff 不可见、修正图后继续、单 Task 缺契约修正、错误依赖修正、追问过期 token/遗漏/重复/外部 Task ID 修正且旧工作保持 pending、过宽能力被检查器识别、重复产物标签被检查器识别。

Task executor 明确记录第一个写入边界：在解析、契约/能力/模型/现有依赖选择、取消参数或追问候选检查阶段失败时，返回 `Failed`、`phase: task_preparation` 与 `side_effects_started: false`。纯 `CreateTaskGraphCommand.validate()` 的失败使用 `phase: task_graph_validation`。Main Chat 可以修正后继续，且未决追问不会被错误参数释放。追问事务仍原子地重查候选与最新消息，客户端预检查不替代数据库约束。

一旦开始创建图、派发 Run、取消或提交追问，结果未知的存储、传输或 dispatch 错误仍进入 unknown-result/review；没有把所有错误一律改成可重试。故障注入测试确认“图已提交、派发失败”仍返回未知错误，不能按无副作用错误继续。

读取 required policy 的兼容默认仅适用于明确 `NotFound` 且没有冻结 Skill bundle 的旧 Run；有 Skill bundle 的 Run 缺失 required policy 时要求刷新。数据库故障、损坏/空正文或无法解析的现有策略都停止创建，不能静默丢弃限制。创建参数 DTO 拒绝 schema 外字段；Plugin Release 仍只能由可信 policy 注入，不能由模型自行提交。

单 Task 与 graph 节点现在都支持相同 `owned_paths` schema，且持久化到 Task input，使用同一 shared scope 和 native 写入约束。缺省/空数组仍保守占用全项目，历史 Run 不被重写。

## Windows 快照导出契约

Windows 开发必须复用本平台生产 catalog/bootstrap，不提交手写 Prompt 或从 macOS 文案复制的第二份 Skill。

导出 UTF-8 JSON 数组，恰好五个 `LocalCapabilityPolicySnapshot`：

1. `skill_bundle_internal`：Main Skill bundle JSON 放在 instructions。
2. `task_skill_bundle_internal`：Task Skill bundle JSON 放在 instructions。
3. `task_policy_internal`：required policy JSON，允许 `eval-model`，max iterations 32，required selections 为空，冻结合成 Plugin Releases。
4. `task_execution`：生产 Task policy instructions、prefixed catalog items 与业务工具 schema（仅快照，不执行）。
5. `main_chat`：生产 Main policy instructions、prefixed catalog items 与八个绑定 Task 工具的能力选择 schema。

共同 owner 为 `eval-user`，revision 必须相同、profile 不重复。Main bundle schema 1、Task schema 2、Product ID、binding/gate、文档 checksum、revision 算法遵循架构文档 §11.4。用生产实际渲染结果算 revision，不写死 hash。

合成选择与 macOS fixture 对齐：CodeMaintainerRead/Write、TerminalController；browser `chatos-browser-cdp@chatos-marketplace`、computer `open-computer-use@chatos-marketplace`、document `chatos-document-mcp@chatos-marketplace`、`design@eval`、`media@eval`；External resource `eval-research`，`https://example.invalid/evaluation-only`，空 headers，工具 `external_mcp__eval_research`。Release 仅用于选择测试，artifact SHA256 为 64 个零，不能当作真实可执行安装。

Windows 用同一个 Rust example 或 Node wrapper 的 `--fixture` 运行；共用 scenarios、检查器、报告 schema，不加 Windows 特供判定。Windows 的单 Task schema 也必须发布与图节点一致的 `owned_paths`；Task executor 的准备/写入错误边界已在 shared Rust 中实现，不要另复制一份基于错误字符串的判断。真实 OS executor、UI、平台 Skill parity 还应另行做平台集成测试，不能用这里的内存规划评测代替。
