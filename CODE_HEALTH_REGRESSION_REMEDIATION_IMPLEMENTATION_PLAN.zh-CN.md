# Chatos RS 代码健康回归整改实施计划

审计日期：2026-07-28

## 1. 结论摘要

当前主干的核心 Rust 代码可以编译，全量 Rust 单元测试和现有前端类型检查也能通过；主要问题不是“系统已经普遍不可运行”，而是 2026-07-26 的插件平台大批量合入后，之前建立的大文件、重复代码和 CI 治理成果发生了明显回归。

本轮确认的最高优先级问题如下：

1. 当前分支无法通过仓库已经配置的源码大小 CI 门禁：55 个生产文件超过 800 行，白名单为空。
2. Rust Clippy 在 CI 固定工具链 1.94.0 下至少有 3 个确定错误，`cargo clippy --workspace --all-targets -- -D warnings` 当前不能通过。
3. 全仓库存在 144 处不少于 25 个有效代码行的精确重复片段，其中 92 处跨文件，合计约 5,317 个重复有效代码行。
4. 现有 CI 只执行 `chatos/backend` 当前包的 Rust 测试，没有执行另外 22 个根工作区包以及 Memory Engine、User Service 的测试；这些测试本身当前可以通过，但没有进入合并门禁。
5. 所有 GitHub Actions 都只运行在 Ubuntu。Windows/macOS 专用的 Computer Use、桌面连接器和沙箱路径没有原生平台编译门禁，其中 `computer_use/windows.rs` 为 2,553 行，并有 127 行包含 `unsafe`。
6. 2026-07-26 的 `30581f56` 一次变更涉及 619 个文件、增加 135,428 行代码，在源码大小门禁已经存在的情况下仍进入当前分支。需要补上必需检查和大变更治理，避免治理脚本“存在但不生效”。

建议先用 2～3 天恢复可执行的质量基线，再分 3～5 个独立批次拆分大文件、收口重复实现，最后补齐多工作区、前端和跨平台 CI。

## 2. 审计范围与验证结果

### 2.1 仓库规模

- 受 Git 管理的文件约 4,122 个。
- 主要源码文件约 3,375 个。
- `scripts/code-size-report.sh` 统计约 3,508 个代码文件、25 MB、757,155 行。
- 超过 500 行的热点文件共 295 个；其中生产源码超过 800 行的文件共 55 个。

生产源码超限分布：

| 范围 | 文件数 |
| --- | ---: |
| 2,000 行及以上 | 15 |
| 1,000～1,999 行 | 15 |
| 801～999 行 | 25 |
| 合计 | 55 |

### 2.2 已执行验证

以下检查通过：

- `cargo +1.94.0 check --workspace --all-targets`：通过，但有未使用导入警告。
- `cargo +1.94.0 test --workspace --no-fail-fast`：2,367 项通过，29 项因外部环境被忽略。
- `cargo +1.94.0 test`（Memory Engine）：127 项通过。
- `cargo +1.94.0 test`（User Service）：27 项通过。
- 10 个生产前端的 `npm run type-check`：全部通过。
- Memory Engine 前端：28 项测试通过。
- Local Connector Electron：15 项测试通过。
- 非测试 Rust `unwrap/expect` 门禁与请求路径 panic 检查：通过。

以下检查失败：

- `python3 scripts/check_source_size_policy.py`：55 个生产源码文件超限。
- `cargo +1.94.0 clippy --workspace --all-targets -- -D warnings`：失败。
- 以空 Git 树为基线执行全量克隆扫描：发现 144 处存量精确重复。

## 3. 重要缺陷

### P0-1：源码大小门禁在当前分支必然失败

`scripts/source-size-allowlist.tsv` 当前没有任何例外项，但有 55 个生产文件超过 800 行。源码大小策略会扫描全部生产源码，而不是只检查本次 diff，因此任意 PR 都会在该步骤失败。

这不是单纯的技术债，而是当前 CI 基线已失效。

重点文件：

| 文件 | 行数 | 主要风险 |
| --- | ---: | --- |
| `local_connector_client/core/src/skills/native/artifacts/presentation.rs` | 12,357 | PPTX 创建、解析、关系、表格、图表、编辑和校验混在同一模块 |
| `local_connector_client/core/src/skills/native/artifacts/pdf_edit.rs` | 7,932 | PDF 页面、表单、注释、附件、元数据和写回耦合 |
| `local_connector_client/core/src/skills/native/computer_use.rs` | 6,810 | 平台分派、协议、安全校验、动作与观察混合 |
| `local_connector_client/core/src/skills/native/artifacts/docx_edit.rs` | 6,541 | DOCX 文本、表格、图片、修订、页眉页脚和包操作混合 |
| `local_connector_client/core/src/skills/native/excel_live.rs` | 4,937 | Excel 探测、身份、读写、审批和平台桥接混合 |
| `local_connector_client/core/src/skills/native/artifacts/docx_render.rs` | 4,173 | 运行时发现、渲染、输出验证和多格式适配混合 |
| `task_runner_service/backend/src/services/plugin_runtime_relay.rs` | 2,968 | 插件准备、Hook、Command、Agent、UI、Native Skill 和响应校验混合 |
| `local_connector_client/core/src/skills/native/computer_use/windows.rs` | 2,553 | 大量 Windows UIA/Win32 `unsafe` 代码集中在单文件 |
| `chatos/backend/src/api/projects/requirement_execution_handlers.rs` | 2,336 | 执行、确认、暂停、恢复、重跑、停止和恢复逻辑耦合 |
| `local_connector_client/core/src/plugins/runtime/host.rs` | 2,298 | Session、审批、Hook、UI、Artifact 和遥测职责混合 |
| `local_connector_client/core/src/skills/native/artifacts/spreadsheet.rs` | 2,293 | 表格解析、校验、转换和写回混合 |
| `local_connector_client/core/src/skills/native/artifacts.rs` | 2,285 | 多格式工具注册、路径、限制和公共操作混合 |
| `local_connector_client/core/src/skills/native/artifacts/schemas.rs` | 2,197 | 所有 Artifact JSON Schema 集中维护 |
| `chatos/backend/src/api/message_task_runner/plugin_ui.rs` | 2,140 | Workbench 会话、资源、Artifact 代理和安全 Header 混合 |
| `local_connector_client/core/src/plugins/runtime/artifact_store.rs` | 2,094 | 存储、授权、持久化、并发和 UI Grant 混合 |

此外，测试文件 `local_connector_client/core/src/skills/native/artifacts/tests.rs` 已达到 19,753 行，虽然被生产源码门禁排除，但会显著增加定位、评审和增量编译成本。

### P0-2：Clippy 必需检查当前失败

在仓库固定的 Rust 1.94.0 工具链下已确认：

- `config_center_service/backend/src/state.rs:17`：未使用的 `TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY` 导入。
- `config_center_service/backend/src/catalog.rs:551`：测试模块后继续定义生产项，触发 `items_after_test_module`。
- `crates/chatos_project_execution/src/lib.rs:617`：手工大小写不敏感比较，触发 `manual_ignore_case_cmp`。

这些问题会直接使 GitHub Actions 中的 `cargo clippy --workspace --all-targets -- -D warnings` 失败。修复后必须继续完整运行 Clippy，确认没有被前置错误遮住的后续问题。

### P0-3：本地验证入口和 CI 验证入口不一致

当前 `make smoke` 不执行：

- `check_source_size_policy.py`
- `check_new_code_clones.py`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- 根工作区全量测试

因此开发者可以得到本地绿色结果，但推送后在 CI 的另一路径失败。需要建立单一、可复用的验证入口，Makefile 与 GitHub Actions 调用同一脚本。

### P0-4：大量已有测试没有进入 CI

根 Rust workspace 有 23 个包。CI 在 `chatos/backend` 目录执行无 `--workspace` 的 `cargo test -q`，实际只运行当前包测试；其他 22 个根工作区包没有执行测试。Memory Engine 和 User Service 是独立工作区，CI 也没有执行它们的测试。

本轮手工执行这些测试全部通过，说明这是低改动、高收益的门禁补齐项。

前端方面：

- CI 只对 `chatos/frontend` 执行 type-check、test、build 和 lint。
- 另外 9 个生产前端没有进入构建或类型检查 CI。
- npm 依赖审计列表遗漏 `config_center_service/frontend`。
- `make build-frontends` 又遗漏 `local_connector_client/frontend`。
- 除 Chatos 与 Memory Engine 外，8 个生产前端的 `src` 目录没有发现 test/spec 文件；Local Connector 有独立 Electron Node 测试，但没有进入 CI。

### P0-5：Windows/macOS 专用代码没有原生 CI

所有 GitHub Actions 的 runner 都是 `ubuntu-latest`。这意味着：

- `#[cfg(target_os = "windows")]` 和 `#[cfg(target_os = "macos")]` 的关键路径不会被 Linux 编译器覆盖。
- Windows UI Automation、SendInput、Job Object、ACL、AppContainer 等代码只能依赖源码合同测试。
- macOS JXA、Accessibility、ScreenCapture 和应用控制只能依赖字符串/编译片段测试。

对于 Computer Use 和沙箱这类高权限代码，仅做 Linux CI 不足以作为发布门禁。

### P1-1：精确重复代码集中在核心边界

全量扫描结果：

- 144 处精确重复。
- 92 处跨文件重复，52 处同文件重复。
- 合计约 5,317 个重复有效代码行。

主要集中区域：

| 区域 | 发现数 | 说明 |
| --- | ---: | --- |
| Local Connector Core 内部 | 40 | Artifact、Plugin Runtime、Computer Use 重复最集中 |
| Chatos Frontend 内部 | 24 | 设置面板、终端主题、Props Builder、Store Action 重复 |
| `scripts/` 内部 | 18 | OpenAPI 门禁和报告脚本重复初始化与 Git diff 解析 |
| Chatos Backend 内部 | 16 | Code Nav、文件读取、模型设置、请求适配重复 |
| Task Runner Backend 内部 | 6 | 完成状态、Task Manager、运行时校验重复 |
| Chatos Backend 与根 MCP | 6 | Ask User、Code Maintainer、Task Manager 契约存在影子实现 |

优先处理的重复簇：

1. 四份 OpenAPI contract gate 脚本重复 49～63 个有效代码行，应抽取 `scripts/openapi_contract_common.sh`。
2. `docx_edit.rs` 内有三段 82 行的同文件重复；`pdf_edit.rs` 内有多段 30～76 行重复。
3. Chatos 与 Task Runner 的 SSH 处理仍有 50 行级重复，应继续迁移到 `chatos_remote_runtime`。
4. Sandbox MCP 与 Task Runner 的终端交互仍有 37 行重复，应继续迁移到 `chatos_terminal_runtime`。
5. Chatos 与根 MCP 的 Ask User 选择规范化、Code Maintainer Storage 仍有重复，应保留一个权威实现。
6. Chatos 前端两套 Model Settings Panel 有 37～53 行重复，应抽取共享表单 Controller/Section。
7. Chatos 与 Official Website 的对象存储读取/签名逻辑存在两段 43～47 行重复，应抽取小型对象存储公共组件，而不是复制安全边界。

### P1-2：超大提交绕过了治理目标

`30581f56 feat: implement Codex plugin parity platform`：

- 619 个文件发生变化。
- 增加 135,428 行，删除 1,341 行。
- 一次引入了多个 1,000～12,000 行的新生产文件。
- 提交时源码大小门禁和空白白名单已经存在。

仓库内的质量脚本无法替代分支保护和必需检查。如果允许在失败状态下合并或直接推送，门禁只会成为报告工具。

## 4. 实施原则

1. 先恢复绿色基线，再进行结构重构。
2. 大文件拆分只移动职责，不同时改变业务行为。
3. 删除副本前先建立合同测试、Golden Fixture 或序列化快照。
4. 公共模块只抽取语义完全一致的逻辑；同名但权限、错误或持久化语义不同的实现不得强行合并。
5. Windows/macOS 高权限代码保持 fail-closed，重构不得放宽权限边界。
6. 不把 55 个超限文件永久白名单化；白名单只作为短期、可到期的 CI 恢复措施。
7. 每个批次应控制在可独立评审、可独立回滚的范围内。

## 5. 分阶段实施计划

### 阶段 0：恢复可信 CI 基线（P0，1～3 天）

#### 0.1 修复当前 Clippy 错误

- 删除 Config Center 未使用导入。
- 将 `catalog.rs` 的生产函数移动到测试模块之前，或将测试拆到 `catalog/tests.rs`。
- 使用 `eq_ignore_ascii_case` 修复 Project Execution 比较。
- 重跑完整 Clippy，继续修复被前置错误遮住的问题。

验收：

```bash
cargo +1.94.0 fmt --all -- --check
cargo +1.94.0 clippy --workspace --all-targets -- -D warnings
```

#### 0.2 临时建立源码大小债务基线

- 将当前 55 个超限生产文件写入 `scripts/source-size-allowlist.tsv`。
- `max_lines` 设置为当前实际行数，不允许继续增长。
- 到期日按批次设置为 30～90 天，不设置统一远期日期。
- reason 必须包含整改批次和目标模块；建议扩展格式增加 owner/issue 字段。
- 同时创建自动生成的债务报表，记录文件当前行数、目标行数、负责人和到期日。

验收：

```bash
python3 scripts/check_source_size_policy.py
python3 -m unittest discover -s scripts/tests -p "test_code_quality_*.py"
```

#### 0.3 建立统一验证入口

新增 `scripts/verify-repository.sh`，并由 Makefile 与 GitHub Actions 共同调用。建议分两层：

- `make verify-fast`：格式、质量脚本、Clippy、类型检查和定向测试。
- `make verify`：全量 Rust 测试、所有前端构建/测试、Docker 配置和 API/OpenAPI 门禁。

禁止在 Makefile 和 CI 中分别维护两份命令清单。

#### 0.4 确认分支保护

- 将 `api-surface-contract`、Rust Clippy/Test、Frontend Check 标记为必需检查。
- 禁止失败状态合并和直接推送默认/发布分支。
- 超大变更需要显式审批标签和拆分说明。

该项需要在 GitHub 仓库设置中确认，代码仓库本身无法证明当前是否已启用。

### 阶段 1：拆分插件与原生能力超大文件（P0/P1，2～4 周）

建议按以下独立批次推进，每批只做文件移动、可见性调整和测试迁移。

#### 批次 1A：Presentation

目标结构：

```text
artifacts/presentation/
  mod.rs
  limits.rs
  model.rs
  parse.rs
  create.rs
  package.rs
  relationships.rs
  text.rs
  tables.rs
  charts/
  inspect.rs
  edit.rs
  validate.rs
```

- 将图表解析/XML/检查独立为 `charts/` 子树。
- 将 Package Relationship、Content Types 和 ZIP 写回独立为 `package.rs`/`relationships.rs`。
- 将表格扫描和编辑从通用文本编辑中分离。
- 保留原公共入口，避免调用方大范围修改。

退出标准：生产子文件不超过 800 行，目标不超过 500 行；现有 PPTX 测试全部通过。

#### 批次 1B：PDF、DOCX、Spreadsheet

- PDF 按 `pages`、`forms`、`annotations`、`attachments`、`metadata`、`stamps`、`package_write` 拆分。
- DOCX 按 `text`、`paragraphs`、`tables`、`images`、`headers_footers`、`tracked_changes`、`package_write` 拆分。
- Spreadsheet 按 `csv_tsv`、`xlsx`、`range`、`formula_policy`、`render` 拆分。
- 跨格式共享路径校验、SHA-256、原子写入、ZIP 安全读取和图片尺寸校验，但不得合并格式专属语义。

退出标准：清除 `docx_edit.rs` 与 `pdf_edit.rs` 中已识别的同文件重复；所有 Artifact fixture 保持字节级或语义级一致。

#### 批次 1C：Computer Use 与 Excel Live

- `computer_use.rs` 拆为 `protocol`、`approval`、`observation`、`actions`、`safety`、`platform`。
- macOS JXA 脚本从 Rust 主流程中迁到独立资源文件或小型构建模块，并保留 checksum/编译测试。
- Windows 实现按 `uia`、`input`、`capture`、`windows`、`process`、`security` 拆分。
- 为每个 `unsafe` 模块增加 Safety Invariant 注释和边界测试。
- Excel Live 按 `identity`、`inspect`、`read`、`write`、`number_format`、`platform_bridge` 拆分。

退出标准：Windows/macOS 原生 runner 可编译；高风险动作审批、重放安全和敏感文本不落盘测试保持通过。

#### 批次 1D：Plugin Runtime

- `plugin_runtime_relay.rs` 按 `prepare`、`hooks`、`command`、`agent`、`ui`、`native_skill`、`validation` 拆分。
- `plugins/runtime/host.rs` 按 Session 生命周期、审批、Hook、UI、Native Skill 和 Telemetry 拆分。
- `artifact_store.rs` 按持久化、授权、并发、UI Grant 和审计拆分。
- `plugin_ui.rs` 按 Workbench Session、Asset、Artifact Relay、Security Headers 拆分。

退出标准：Plugin Runtime 现有合同测试全部通过；插件身份、签名、Capability 和 Owner Scope 不发生字段漂移。

#### 批次 1E：业务编排与前端大组件

- `requirement_execution_handlers.rs` 按 execute/confirm/mutate/rerun/stop/recovery/query 拆分。
- `RequirementExecutionProcessModal.tsx` 拆为数据 Controller、状态机、任务图、反馈、操作区和视图组件。
- `TaskEditorDrawer.tsx` 拆出 schema、validation、model mapping 和 sections。
- `BrowserSessionPanel.tsx` 拆出 session controller、network、tabs、preview、actions。

退出标准：大组件主入口不超过 300 行，领域 Hook/Controller 不超过 500 行，现有交互测试保持通过。

### 阶段 2：收口重复代码（P1，1～2 周）

按风险从低到高推进：

1. 抽取 OpenAPI Shell 公共函数，消除 4 份 gate 和 4 份报告脚本的重复初始化。
2. 合并 Chatos 前端同构设置面板、终端主题、值格式化和 Props Builder。
3. 将 Ask User 规范化统一到根 MCP 或共享 Runtime，Chatos 只保留领域适配。
4. 将 Code Maintainer Storage、Task Manager DTO/Schema 的影子实现迁到权威 crate。
5. 将剩余 SSH 和终端纯运行时逻辑迁入 `chatos_remote_runtime`、`chatos_terminal_runtime`。
6. 抽取对象存储签名、范围读取和错误映射公共组件。
7. 清理 Plugin Runtime 内部跨格式和同文件重复。

每个重复簇的完成条件：

- 明确一个权威实现。
- 新增或保留双消费者合同测试。
- 删除旧副本，不保留长期代理层或 feature flag 双实现。
- 全量克隆扫描的对应 finding 消失。

阶段目标：跨文件精确重复从 92 处至少降低 60%，存量总 finding 从 144 降到 60 以下；新增重复继续保持 0。

### 阶段 3：补齐测试与平台矩阵（P0/P1，3～7 天）

#### Rust CI

必须执行：

```bash
cargo +1.94.0 test --workspace --no-fail-fast
(cd memory_engine/backend && cargo +1.94.0 test --no-fail-fast)
(cd user_service/backend && cargo +1.94.0 test --no-fail-fast)
```

将 MongoDB、Docker、Bubblewrap 等外部依赖测试拆成独立集成 job，不阻塞纯单元测试 job 的快速反馈。

#### Frontend CI

- 对 10 个生产前端执行 `npm ci` 和 `npm run type-check`。
- 对所有有 `build` 脚本的生产前端执行构建。
- 对所有有 `test`/`test:electron` 脚本的前端执行测试。
- 将 `config_center_service/frontend` 加入 npm audit。
- 将 `local_connector_client/frontend` 加入根 Makefile 构建。
- 为 Task Runner、Plugin Management、Project Management、Sandbox Manager 至少补齐核心页面 smoke/controller 测试。

#### 平台 CI

- Ubuntu：完整 workspace、服务和容器配置。
- Windows：Local Connector Core、Computer Use Helper、Sandbox MCP Windows wrapper 的 check/clippy/test。
- macOS：Local Connector Core、JXA compile contract、Electron/Chrome Native Host 打包前检查。
- 夜间或发布前运行真实 Excel、Accessibility、ScreenCapture、Docker/Bubblewrap E2E；PR 只运行无副作用的最小平台测试。

### 阶段 4：防止再次回归（P1，2～4 天）

#### 4.1 大变更预算

新增变更规模报告：

- 超过 80 个文件或 10,000 个新增行时发出阻塞性检查。
- 允许通过 `large-change-approved` 标签解除，但必须附拆分理由、回滚策略和专项测试矩阵。
- 自动生成按目录的新增行数和新增超限文件列表。

阈值可以根据团队实际速度调整，但不能完全没有大变更审查机制。

#### 4.2 存量债务趋势

- 每周生成源码大小、精确克隆、测试矩阵和超期 allowlist 报告。
- 报告作为 CI Artifact 保存，并在新增债务时失败。
- 对 500～800 行文件只允许行数持平或下降，避免继续逼近硬限制。

#### 4.3 白名单治理

- 白名单必须有 owner、最大行数、到期日、整改批次。
- 到期前 7 天产生告警。
- 文件拆分后自动要求删除陈旧白名单项。
- 禁止用扩大 `max_lines` 的方式处理普通功能增长。

## 6. 建议批次与依赖顺序

| 批次 | 内容 | 前置 | 预计风险 |
| --- | --- | --- | --- |
| A | 修 Clippy、建立临时大小基线、统一 verify 入口 | 无 | 低 |
| B | Rust/Frontend 全量 CI、补 config npm audit | A | 低 |
| C | Windows/macOS 最小 CI | A | 中 |
| D | OpenAPI 脚本与前端低风险重复 | A | 低 |
| E | Presentation/PDF/DOCX/Spreadsheet 拆分 | A、B | 中 |
| F | Computer Use/Excel Live 拆分 | B、C | 高 |
| G | Plugin Runtime/Plugin UI 拆分 | A、B | 高 |
| H | Requirement Execution 与大前端组件拆分 | A、B | 中 |
| I | SSH、Terminal、Ask User、Storage 权威实现收口 | D～H 按模块完成 | 中 |
| J | 移除临时 allowlist、收紧趋势门禁 | E～I | 低 |

不要把 E、F、G 合并为一个超大重构提交。

## 7. 最终验收标准

### CI 与构建

- `cargo +1.94.0 fmt --all -- --check` 通过。
- 根 workspace、Memory Engine、User Service 的 Clippy 与测试全部通过。
- 10 个生产前端 type-check/build 通过；存在测试脚本的前端测试全部通过。
- Windows/macOS 最小原生编译 job 通过。
- API、OpenAPI、依赖、安全和 Docker 配置门禁通过。

### 大文件

- 所有生产源码低于 800 行。
- 新增生产文件目标不超过 500 行。
- `source-size-allowlist.tsv` 最终恢复为空，或只保留经过架构评审且有明确到期日的极少数例外。
- 19,753 行 Artifact 测试文件按格式和能力拆分，单测试文件建议不超过 1,500 行。

### 重复代码

- 新增 25 行以上精确重复为 0。
- 存量 144 处 finding 降到 60 以下，跨文件重复至少降低 60%。
- OpenAPI Gate、Ask User、SSH、Terminal、Code Maintainer、前端 Model Settings 均只有一个权威实现。

### 流程

- 本地 `make verify-fast` 与 CI 使用同一底层脚本。
- 默认/发布分支启用必需检查，失败状态不可合并。
- 超大变更必须有显式审批和拆分说明。
- 每周债务趋势不会新增超限文件或扩大 allowlist 预算。

## 8. 推荐立即执行的第一批任务

1. 修复 3 个已确认 Clippy 错误，并继续跑完整 Clippy 到全绿。
2. 为 55 个超限文件建立短期、不可增长、分批到期的 allowlist，恢复 CI 可执行性。
3. 新增统一 `verify-repository.sh`，让 Makefile 和 GitHub Actions 共用。
4. 将根 workspace、Memory Engine、User Service 全量测试接入 CI。
5. 将 10 个生产前端的 type-check/build 接入 CI，并补齐 Config Center npm audit 与 Local Connector 构建。
6. 增加 Windows/macOS 最小 runner，优先覆盖 Computer Use 和 Sandbox Wrapper。
7. 先拆 `presentation.rs`，再拆 `pdf_edit.rs`/`docx_edit.rs`，不要同时开工多个高风险格式。
8. 抽取 OpenAPI Shell 公共层，作为重复代码治理的低风险示范批次。

完成以上第一批后，再开始 Plugin Runtime 和 Computer Use 的高风险结构重构。
