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
2. `docx_edit.rs` 原有三段 82 行的同文件重复已在 table row 公共选择层收口；`pdf_edit.rs` 内仍有多段 30～76 行重复。
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

## 9. 2026-07-28 实施进展

阶段 0 的仓库内代码整改已完成，外部仓库设置项仍需人工确认：

- 已修复固定 Rust 1.94.0 工具链下的 Clippy 错误及其后续暴露问题；根工作区、Memory Engine、User Service 的格式与 Clippy 全部通过。
- 已为 55 个存量超限生产文件建立不可增长的临时大小基线；当前源码大小、热点预算和新增代码克隆门禁全部通过，未通过扩大预算掩盖新增增长。
- 已新增统一验证入口 `scripts/verify-repository.sh`，Makefile 与 GitHub Actions 共用质量、Rust、前端验证命令。
- 已将根 Rust workspace、Memory Engine、User Service 全量测试接入验证入口；macOS 签名 Plugin Hook 测试改为先构建 sandbox agent，再作为专项测试显式执行，避免干净构建目录下的伪失败。
- 已将 10 个生产前端接入统一验证；Chatos 517 项测试、Memory Engine 28 项测试、Local Connector Electron 15 项测试以及全部前端类型检查和生产构建均通过。
- 已修复 Chrome 浏览器下载结果无法渲染结构化详情的问题，并将运行环境面板测试迁移到“唯一工作区执行镜像”契约。
- GitHub Actions YAML 可解析，CI 已调用统一的 `quality`、`rust-lint`、`rust-test` 与逐前端验证模式。
- 已新增 `native-platform-contracts` 矩阵，使用 Windows x64 与 macOS ARM64 原生 runner；统一的 `native-platform` 验证模式会对 Local Connector Core、Computer Use、Chrome Native Host 与 Sandbox MCP Wrapper 执行 check、Clippy、二进制构建和定向合同测试。
- macOS 本机原生复验已通过：Computer Use 30 项、Chrome Native Host 4 项、Excel/Computer Use JXA 编译合同、Sandbox Wrapper 5 项、安装包合同 16 项和 Electron 15 项。Windows 分支由新增的远端原生 runner 负责实际编译验证。
- 已修复安装包合同测试与新增迁移清单校验不一致的问题：测试夹具现在使用正式的 23 份 SQLite 迁移，并仅在主机平台和架构精确匹配时执行目标二进制，避免把 ARM64/x64 交叉产物误当成本机可执行文件。
- 已新增 `scripts/openapi_contract_common.sh`，将四个 OpenAPI 门禁重复的比率、策略、路径和紧急豁免逻辑收口；相关脚本总行数由 1,150 行降至 753 行，净减少 397 行，同时补齐快速 diff 路由与 Harness CI 快照依赖。
- 已修复 OpenAPI waiver 示例中未加引号的空格值，避免 `source` 时把原因文本误执行为 Shell 命令。
- 已启动 Presentation 批次 1A 的低风险拆分：新增 `presentation/limits.rs`、`presentation/model.rs`、`presentation/chart_model.rs`、`presentation/chart_axis_inspection.rs`、`presentation/chart_context_inspection.rs`、`presentation/chart_package_inspection.rs`、`presentation/chart_parse.rs`、`presentation/chart_replacement.rs`、`presentation/chart_result_inspection.rs`、`presentation/chart_series_style_inspection.rs`、`presentation/chart_structure_inspection.rs`、`presentation/chart_data_parse.rs`、`presentation/chart_input.rs`、`presentation/chart_inspection.rs`、`presentation/chart_event_inspection.rs`、`presentation/chart_axes_xml.rs`、`presentation/chart_snapshot.rs`、`presentation/chart_xml.rs`、`presentation/chart_xml_common.rs`、`presentation/drawing_text_edit.rs`、`presentation/package_paths.rs`、`presentation/presentation_inspection.rs`、`presentation/relationship_inspection.rs`、`presentation/package_metadata.rs`、`presentation/package_edit.rs`、`presentation/package_entries.rs`、`presentation/package_io.rs`、`presentation/slide_append.rs`、`presentation/slide_order_operations.rs`、`presentation/slide_parse.rs`、`presentation/slide_selection.rs`、`presentation/slide_shapes.rs`、`presentation/slide_xml.rs`、`presentation/table_edit.rs`、`presentation/table_cell_operations.rs`、`presentation/table_column_operations.rs`、`presentation/table_row_operations.rs`、`presentation/table_scan.rs`、`presentation/table_selection.rs`、`presentation/table_structure.rs`、`presentation/templates.rs`、`presentation/text_edit.rs`、`presentation/text_operations.rs`、`presentation/text_validation.rs`、`presentation/xml_structure.rs`、`presentation/inspection_common.rs`、`presentation/render_validation.rs` 和 `presentation/image.rs`，分别承接安全限额、稳定的 slide/table/text/chart-inspection 数据模型、图表类型/坐标轴/marker/legend/series 配置、chart axis 归属解析、metadata 输出与 canonical value-axis 校验、chart XML 文本/数据上下文识别、布尔属性/公式唯一性/文本预算校验及 series 预览 JSON、标准图表 Slide 引用、包内 ownership、content-type 与 relationship 校验、图表颜色/marker/smooth/坐标轴参数解析与边界校验、图表替换输入/并发快照校验/原子包重写、series value-axis 归属、主/次坐标轴选择、标题投影与最终 inspection 模型装配、chart XML series 颜色/marker/smooth 状态采集及规范化判定、chart group/axis 结构元素记录、命名空间与标准类型识别、categories/X values/series 的完整数据解析及图表类型专属校验、chart 输入属性白名单、标题/坐标轴选项/图例/数据标签解析及 series-axis 兼容性校验、chart package/relationship/canonical snapshot 检查流程及最终元数据装配、chart XML 解析状态、start/empty/text/end 事件分派与完整性校验、分类轴/数值轴/散点图双轴的 OOXML 生成、标准图表 JSON 快照与 byte-exact 规范化校验、图表 group/series/cache/legend OOXML 生成、无环共享的图表标题与数值格式化、DrawingML 文本 XML 事件缓存、实体/CDATA 解码、替换上限和闭合校验、OOXML 包路径/Relationship ID/relationship target 规范化辅助函数、Presentation 整体尺寸/Slide 文本/表格/notes/media inspection、可见 Slide 关系精确性、notes ownership 与 Slide 关系 inspection、Presentation/Relationship/Content Types 元数据解析、Package XML 追加/删除/重排及动态 Slide/notes relationship XML、新建 PPTX 的核心模板/Slide/媒体/备注/图表条目装配、ZIP 安全校验/原子重写、既有 PPTX 的 Slide/media/chart/notes 安全追加与继承关系装配、Slide 重排/删除及相关包关系维护、Slide/table 输入解析与总量约束、Slide 顺序、删除位置和可选范围参数校验、文本/表格/图表形状 OOXML、Slide 布局与备注页 XML、表格移动与统一 XML edit/编辑后复验、表格检查/单元格格式复制/文本替换入口、表格列删除/插入/移动入口、表格行删除/插入/移动入口、表格预览扫描与可安全编辑简单模型识别、表格选择、索引/预期行列值校验和单元格 XML SHA-256 并发修改防护、表格行列结构解析、规范 opening 生成及行/单元格克隆、固定模板、跨 run 文本扫描/格式一致性校验/重写及普通 Slide/notes 共用替换输入解析、普通/跨 run/notes 文本替换流程及共享包打开上下文、跨 Slide/table/chart 共用的文本安全校验、通用 PPTX XML element range/层级/opening 解析、Slide part/尺寸/文本预览 inspection 公共辅助、渲染前包/内容类型/关系目标安全校验，以及图片读取、Presentation 尺寸策略、DrawingML 输出和 contain/cover 布局；原公共入口及行为保持不变。
- `presentation.rs` 已由 12,357 行降至 685 行，已拆出的四十八个生产子文件分别为 45、261、433、278、322、416、353、170、110、321、349、347、415、280、324、265、453、366、19、112、116、146、278、299、434、121、289、447、268、278、84、164、324、199、374、410、364、367、214、213、165、400、287、28、278、62、122、194 行，均低于 500 行目标；Presentation 文件集合当前共 13,249 行，仅增加 892 行模块/可见性边界开销，主文件净减少 11,672 行，且未复制保留已迁移实现。主文件已低于 800 行硬限制并删除陈旧源码大小白名单。原约 742 行的 `inspect_standard_pptx_chart_xml` 已降至 21 行，仅保留事件读取、受支持事件分派和最终结果装配。
- 图表 XML 迁移时将散点图 X values、气泡图 X values/bubble sizes 和分类图 categories 的内部 `expect` 改为显式 `Result` 错误；即使未来内部调用绕过输入解析，也不再因缺失已验证数据而 panic。
- Slide XML 迁移时将 image/table/chart 布局资源、relationship 以及表格维度的内部断言改为显式 `Result` 错误，并为意外空表增加安全拒绝，避免未来内部调用绕过输入解析时 panic 或除零。
- 简单表格扫描迁移时将“已验证恰好一个文本 run”的内部 `expect` 改为显式 `Result` 错误，畸形或未来内部状态漂移不再触发 panic。
- 通用 PPTX XML 结构扫描迁移时将 opening/closing tag 分支选择的两个内部 `expect` 改为显式 `Result` 错误，未来条件重构或状态漂移不再触发 panic。
- 跨 run 文本重写迁移时为意外空匹配增加显式 `Result` 错误，避免内部不变量被绕过时出现下标下溢。
- chart 结构 inspection 迁移时将 bubble metadata 分派中的 `unreachable!` 改为显式 `Result` 错误，未来元素匹配集合和分派逻辑漂移时不再 panic。
- chart 文本/data context 迁移时将 series 公式与缓存值分派中的两个 `unreachable!` 改为显式 `Result` 错误，未来上下文识别和字段分派集合漂移时不再 panic。
- table selection 迁移时将 row/column 完整单元格数组解析合并为同一个内部实现，删除两份等价的边界、类型和文本安全校验代码。
- Slide selection 迁移时将删除与普通选择使用的 `slide_numbers` 范围、去重和排序解析合并为同一个内部实现。
- Package 元数据迁移时将三个依赖内部不变量的 `expect` 改为显式 `Result` 错误，畸形 slide id/namespace 输入不再存在潜在 panic 路径。
- Chart inspection 迁移时复用既有 axis metadata 装配函数，删除主、次 value-axis 两段等价字段投影，并将 metadata 对象内部 `expect` 改为显式 `Result` 错误。
- Slide append 迁移时将 notes master 的内部 `expect` 改为显式 `Result` 错误；普通 Slide 文本和 speaker notes 文本替换现共用 `find`、`replacement`、文本安全及 `max_replacements` 输入解析，删除 35 行以上的等价校验流程。
- 表格行、列入口已分别迁入独立模块；列移动时查找源单元格和参考单元格的两个内部 `expect` 已改为显式 `Result` 错误，畸形简单表格状态不再触发 panic。
- 表格检查、单元格格式复制和文本替换入口已迁入同一独立模块，并统一复用 `selected_pptx_table`，删除两套 ZIP/Presentation/Slide 定位流程；简单表格单元格定位及编辑后复验中的四处 `expect` 已改为显式 `Result` 错误。
- 普通、跨 run 和 speaker notes 文本替换入口已迁入同一独立模块，并共用包校验、Presentation relationship、可见 Slide 顺序与数量限制上下文；两个 Slide 路径索引和一个 notes 归属索引不再使用断言或直接下标。
- Slide 重排和删除入口已迁入同一独立模块；Slide 路径、relationship ID 与 notes ownership 的索引访问均改为显式 `Result` 错误，畸形包关系或未来内部状态漂移不再触发 panic。
- 图表替换入口已迁入独立模块；chart ownership 的可见 Slide 索引不再使用 `expect`，内部 ownership 状态不一致时会返回显式错误。
- Presentation 整体 inspection 已迁入独立模块；Slide ID 投影不再使用直接下标，元数据与可见 Slide 顺序意外不一致时会返回显式错误。
- 质量门禁在图片模块拆出后识别出 DOCX、PDF 与 Presentation 的 JPEG/PNG 元数据解析重复；已新增 127 行的共享 `artifacts/image_metadata.rs`，统一严格 PNG chunk 和 JPEG frame header 解析，并让三个格式复用。`docx_edit.rs` 由 6,541 行降至 6,431 行，`pdf_edit.rs` 由 7,932 行降至 7,882 行；相关生产文件合计净减少 69 行，新增代码克隆门禁恢复为零违规。
- 已启动批次 1B 的 DOCX 拆分：新增 240 行的 `docx_edit/package_write.rs`、246 行的 `docx_edit/image.rs`、290 行的 `docx_edit/metadata.rs`、384 行的 `docx_edit/header_footer_operations.rs`、224 行的 `docx_edit/header_footer_selection.rs`、470 行的 `docx_edit/paragraph_operations.rs`、367 行的 `docx_edit/table_cell_operations.rs`、221 行的 `docx_edit/table_row_common.rs`、86 行的 `docx_edit/table_row_delete.rs`、151 行的 `docx_edit/table_row_insert.rs`、120 行的 `docx_edit/table_row_move.rs`、201 行的 `docx_edit/table_row_operations.rs`、75 行的 `docx_edit/tracked_change_model.rs`、270 行的 `docx_edit/tracked_change_operations.rs`、180 行的 `docx_edit/tracked_change_replacement.rs` 和 344 行的 `docx_edit/tracked_change_resolution.rs`，分别承接目标路径保护、ZIP 新建/重写、临时文件同步与原子持久化，图片输入、尺寸安全、DrawingML 布局和媒体关系写入，Core Properties inspection、包关系一致性校验与字段更新编排，header/footer 创建、文本替换、引用解析、part 选择和 XML 根校验，段落按文本/索引的插入、删除、移动和结构化替换操作编排，表格单元格文本替换/简单内容解析/复杂内容拒绝，表格行共享安全校验、顶层表格/行选择与 clone identity 清理，行删除/插入/移动的差异化 XML 实现和公共操作编排，以及 tracked changes 的数据模型、公共操作编排、replacement run 选择/OOXML 生成、revision 扫描/校验/接受与拒绝；十六个生产子模块均低于 500 行。`docx_edit.rs` 已由原始 6,541 行降至 2,859 行，DOCX edit 文件集合当前共 6,728 行，增加 187 行模块/可见性边界与显式错误处理开销。
- DOCX 包写入拆分时将新建包和重写包重复的目标存在性/覆盖策略、父目录建立、临时文件同步、大小复验和原子替换收口为共享内部实现；图片插入公共入口保持不变，并继续复用跨格式 PNG/JPEG 元数据解析。Metadata 迁移时将字段到 XML tag 映射的 `unreachable!` 改为显式 `Result` 错误，并将通用 XML element range 扫描中的两个内部 `expect` 改为显式错误，未来分支条件或状态漂移不再触发 panic。Header/footer 两个公共入口保持不变，关系解析与编辑编排已分离，既有 relationship、content type、part selection 和文本运行边界校验继续复用。Tracked changes 的三个公共入口同样保持不变，操作编排、修订模型、replacement XML 和 revision resolution/validation 均已迁出主文件；revision ID 分配结果原有的 `[0]` 直接下标访问改为 `.first()` 加显式错误，异常状态不再触发越界 panic。段落八个公共入口保持不变，按文本/按索引的成对输入校验、包读写和结果元数据装配已统一迁出，底层段落 XML 选择与重写暂留主文件供下一批拆分。表格单元格替换公共入口保持不变，简单单元格解析和复杂内容拒绝规则已迁出主文件；表格行删除、插入和移动三个公共入口同样保持不变，其参数校验、包读写、底层表格/行选择、并发文本匹配、clone identity 清理和 XML 重写均已迁出主文件，并继续共享同一套范围标记、复杂内容与单元格安全校验。拆分后暴露出的三段 82 行顶层表格选择重复已收口为共享实现，新增代码克隆门禁恢复为零违规。表格与普通文本路径仍复用的 run 复杂内容和 XML wrapper 检查继续保留为主文件通用辅助函数，避免错误收窄共享边界。
- 后续 DOCX 段落底层 XML 批次已完成：新增 309 行的 `docx_edit/paragraph_edit.rs` 和 279 行的 `docx_edit/paragraph_selection.rs`，分别承接插入、删除、移动、结构化替换的底层 XML 修改，以及顶层段落选择、索引校验和复杂结构拒绝；主文件中的旧实现已删除，未复制保留。`docx_edit.rs` 进一步由 2,859 行降至 2,298 行，DOCX edit 文件集合当前共 6,755 行，相对原始单文件仅增加 214 行模块/可见性边界与显式错误处理开销；十八个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- DOCX comments 批次已完成：新增 317 行的 `docx_edit/comment_operations.rs`，统一承接评论输入校验、comments part/relationship/content type 一致性校验、comment ID 分配、单一完整 run 标记插入、comments XML 生成与原子包重写；主文件仅保留公共委托入口，旧实现已删除。`docx_edit.rs` 进一步由 2,298 行降至 2,009 行，DOCX edit 文件集合当前共 6,783 行，相对原始单文件增加 242 行模块/可见性边界与显式错误处理开销；十九个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- DOCX package XML 公共层批次已完成：新增 318 行的 `docx_edit/package_xml.rs`，集中承接 package part/relationship/drawing ID 分配、属性解码、根元素子节点追加、Content Types default/override 维护、relationship 列表解析和目标路径安全归一化；父模块保留内部导入别名，metadata、图片、comments、header/footer 等既有业务模块无需改写调用接口。`docx_edit.rs` 进一步由 2,009 行降至 1,725 行，DOCX edit 文件集合当前共 6,817 行，相对原始单文件增加 276 行模块/可见性边界与显式错误处理开销；二十个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- DOCX metadata XML 批次已完成：新增 273 行的 `docx_edit/metadata_xml.rs`，承接 metadata 更新/删除字段解析、字段到 Core Properties tag 映射、标准 Core Properties 模板、Content Types 严格 inspection、Core Properties 根/命名空间/唯一性校验，以及属性值读取、转义更新和删除；既有 `metadata.rs` 继续负责包级业务编排。`docx_edit.rs` 进一步由 1,725 行降至 1,471 行，DOCX edit 文件集合当前共 6,836 行，相对原始单文件增加 295 行模块/可见性边界与显式错误处理开销；二十一个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- DOCX 文档生成批次已完成：新增 287 行的 `docx_edit/document_generation.rs`，承接结构化 DOCX 创建、既有文档内容追加、block 数量/字符/表格单元格限额、段落样式和表格 OOXML 渲染、section 前安全插入，以及默认 Content Types、document relationships 和 styles 模板；公共创建/追加/模板入口仍由主文件委托，段落编辑和图片插入继续通过原内部名称复用渲染与 section 插入。`docx_edit.rs` 进一步由 1,471 行降至 1,235 行，DOCX edit 文件集合当前共 6,887 行，相对原始单文件增加 346 行模块/可见性边界与显式错误处理开销；二十二个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- DOCX 普通与跨 run 文本编辑批次已完成：新增 423 行的 `docx_edit/text_edit.rs` 和 143 行的 `docx_edit/text_operations.rs`，分别承接 run 内替换、跨 run 唯一匹配、简单 run/可见文本解析、相同格式与相邻标记校验、XML 安全重写，以及两个公共入口的输入、包读取、原子写回与结果装配；header/footer、paragraph、table row、comments 和 tracked changes 继续通过父模块内部别名复用所需文本辅助函数。`docx_edit.rs` 由 1,235 行降至 716 行，DOCX edit 文件集合当前共 6,934 行，相对原始单文件增加 393 行模块/可见性边界与显式错误处理开销；二十四个生产子模块均低于 500 行。主文件已低于 800 行硬限制，陈旧的 6,541 行源码大小白名单已删除；该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- 已启动 PDF 批次：新增 222 行的 `pdf_edit/metadata.rs`，承接 PDF Info 更新/删除输入解析、Info dictionary 安全读取、Unicode 文本规范化、已知字段 inspection/预览截断、未知字段计数和原子写回；`update_pdf_metadata` 与 `inspect_pdf_metadata` 公共入口保持不变。迁移时将已验证 `remove_fields` 后的内部 `expect` 改为显式 `Result` 错误，未来校验字段集合与映射漂移时不再 panic。`pdf_edit.rs` 由共享图片元数据拆分后的 7,882 行降至 7,691 行，PDF edit 文件集合当前共 7,913 行，仍比原始 7,932 行少 19 行；首个生产子模块低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁；PDF 源码大小白名单暂保留到主文件实际降至限制以内。
- PDF package write 批次已完成：新增 185 行的 `pdf_edit/package_write.rs`，统一承接加密/空页拒绝、目标路径扩展名与同文件保护、既有 symlink/regular-file 复验、文档压缩与对象重编号、临时文件同步、100 MiB 限额、源文件及附件 SHA-256 并发变更守卫，以及最终原子持久化；页面、表单、注释、附件、metadata 和 stamping 调用点继续通过父模块内部别名复用同一写入边界。`pdf_edit.rs` 进一步由 7,691 行降至 7,531 行，PDF edit 文件集合当前共 7,938 行，相对原始单文件仅增加 6 行模块/可见性边界开销；两个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- PDF 页面操作入口批次已完成：新增 285 行的 `pdf_edit/page_operations.rs`，承接 merge、extract、arrange、rotate 四个公共入口的输入文件/总字节/页数限额、页面选择与顺序变更校验、页面树重写编排、目标保护、原子保存和结果 metadata 装配；公共入口保持不变，底层页面选择、页面树安全校验、materialization 和 merge 算法暂由父模块内部函数提供。`pdf_edit.rs` 进一步由 7,531 行降至 7,298 行，PDF edit 文件集合当前共 7,990 行，相对原始单文件增加 58 行模块/可见性边界开销；三个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试和新增代码克隆门禁。
- PDF 页面选择与页面树底层批次已完成：新增 108 行的 `pdf_edit/page_selection.rs` 和 182 行的 `pdf_edit/page_tree.rs`，分别承接 PDF 路径数组、必选/可选页码、唯一顺序校验，以及页面树安全检查、继承页面属性 materialization、合并对象图重建；`page_operations.rs` 继续通过父模块内部别名复用这些职责，主文件中的旧实现和页面树常量已删除，未复制保留。`pdf_edit.rs` 进一步由 7,298 行降至 7,048 行，PDF edit 文件集合当前共 8,030 行，相对原始单文件增加 98 行模块/可见性边界与显式校验开销；五个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy 和 131 项 Artifacts 定向测试；格式、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过。全工作树新增代码克隆门禁当前唯一报告为范围外 `local_runtime/task_runner/execution/mod.rs` 的两段 29 行重复，PDF 文件无新增克隆违规。
- PDF 文本与图片生成批次已完成：新增 137 行的 `pdf_edit/generation_common.rs`、361 行的 `pdf_edit/text_generation.rs` 和 267 行的 `pdf_edit/image_generation.rs`，分别承接有限数值与页面尺寸解析、ASCII/Helvetica 度量共享层，分页文本布局、Info/字体/内容流生成，以及图片输入总量校验、contain/cover 变换、页面/XObject 构建；stamping 和 annotations 继续通过父模块内部别名复用同一数值、文本和字体度量实现，PNG/JPEG 解码及嵌入对象构建仍与图片 stamping 共用，未复制保留。`pdf_edit.rs` 进一步由 7,048 行降至 6,354 行，PDF edit 文件集合当前共 8,101 行，相对原始单文件增加 169 行模块/可见性边界开销；八个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试、格式、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；全工作树克隆门禁仍仅报告范围外 `local_runtime/task_runner/execution/mod.rs` 的两段 29 行重复，PDF 文件无新增克隆违规。
- PDF AcroForm 批次已完成：新增 88 行的 `pdf_edit/form_model.rs`、59 行的 `form_decode.rs`、282 行的 `form_tree.rs`、294 行的 `form_field_description.rs`、289 行的 `form_field_options.rs`、145 行的 `form_inspection.rs`、267 行的 `form_validation.rs` 和 334 行的 `form_operations.rs`，分别承接表单限额与稳定模型、文本/name 解码、字段树递归与继承、字段类型及支持度描述、checkbox/radio/choice appearance 和 option 校验、inspection 结果装配、更新输入及并发值验证，以及目标保护、字段写回、appearance 维护和写后复验；公共 `inspect_pdf_form`/`fill_pdf_form_fields` 入口保持不变，主文件旧实现已删除。迁移时将文本、checkbox、multi-select 写回以及 MaxLen 分支中依赖前置验证的 `expect`/`unreachable!` 改为显式 `Result` 错误，未来内部状态漂移不再 panic。`pdf_edit.rs` 进一步由 6,354 行降至 4,719 行，PDF edit 文件集合当前共 8,224 行，相对原始单文件增加 292 行模块/可见性与显式错误处理开销；十六个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试、格式、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；PDF 文件无新增克隆违规。
- PDF annotation inspection 与 Text 操作批次已完成：新增 152 行的 `pdf_edit/annotation_common.rs`、328 行的 `annotation_inspection.rs`、244 行的 `annotation_link.rs`、247 行的 `annotation_operation_common.rs` 和 148 行的 `annotation_text_operation.rs`，分别承接 Annots/markup 数组公共校验、页面几何与 annotation inspection、HTTPS/internal link 目标解析、普通与 SHA-256 guarded annotation 文档加载/容量校验/目标选择/追加与保存，以及 Text annotation 输入、几何和结果装配；公共 inspection 与 Text annotation 入口保持不变，主文件旧实现已删除。拆分暴露出的 Text/markup 添加流程重复，以及 link/reply/update/delete/attachment 共有的严格源文件读取、annotation 状态 inspection 和 update/delete 目标选择重复，已全部收口到共享操作层；新增代码克隆门禁当前不再报告 PDF 文件。`pdf_edit.rs` 进一步由 4,719 行降至 3,648 行，PDF edit 文件集合当前共 8,272 行，相对原始单文件增加 340 行模块/可见性与显式共享上下文开销；二十一个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试、格式、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁。
- PDF annotation 剩余写操作批次已完成：新增 284 行的 `pdf_edit/annotation_markup_operation.rs`、201 行的 `annotation_link_operation.rs`、144 行的 `annotation_reply_operation.rs`、224 行的 `annotation_text_update.rs` 和 135 行的 `annotation_delete_operation.rs`，分别承接 markup 矩形模型/去重/页面边界与字典生成、HTTPS/内部页 Link 目标写入、reply parent 关系和 Rect 校验、Text/markup 内容及作者更新与写后计数复验，以及支持类型、结构树/Popup 关系和可达引用保护下的删除。五个公共入口保持稳定，主文件旧实现和 markup 专属辅助代码已删除；`annotation_operation_common.rs` 扩展至 299 行，进一步统一 guarded annotation 原子保存和 root/reply/group 关系判定，`package_write.rs` 中失去调用的 reply 专属保存包装已删除。迁移时将 `remove_fields` 已校验分支中的 `unreachable!` 改为显式 `Result` 错误，内部状态漂移不再触发 panic。`pdf_edit.rs` 进一步由 3,648 行降至 2,680 行，PDF edit 文件集合当前共 8,327 行，相对原始单文件增加 395 行模块/可见性与显式共享边界开销；二十六个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试、格式、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；新增代码克隆门禁不报告 PDF 文件，仅保留两个既知范围外违规。
- PDF file attachment 与 embedded file 批次已完成：新增 252 行的 `pdf_edit/attachment_add_operation.rs`、330 行的 `attachment_common.rs`、270 行的 `attachment_extract_operation.rs`、221 行的 `attachment_filespec.rs` 和 260 行的 `embedded_file_inspection.rs`，分别承接 FileAttachment annotation 输入/几何/Filespec 与 EmbeddedFile 构建、文件格式/安全文件名/内容签名/提取目标与原子落盘、两条提取入口的共享 SHA-256 guarded PDF 加载和结果持久化、Filespec/EmbeddedFile stream 严格 inspection，以及 catalog EmbeddedFiles Name Tree 的有界遍历与预览。添加入口继续复用 annotation 共享追加层，两条提取入口删除了重复的源文件读取与 SHA-256 校验；Name Tree 原八参数递归函数改为显式 collector 状态对象，移除了 `too_many_arguments` 豁免。四个公共入口保持稳定，主文件旧实现和附件专属类型/辅助函数已删除。`pdf_edit.rs` 进一步由 2,680 行降至 1,461 行，PDF edit 文件集合当前共 8,445 行，相对原始单文件增加 513 行模块/可见性与显式共享状态开销；三十一个生产子模块均低于 500 行。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试、PDF scoped Rustfmt、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；新增代码克隆门禁不报告 PDF 文件。
- PDF stamping 收尾批次已完成：新增 416 行的 `pdf_edit/embedded_image.rs`、233 行的 `stamp_image_operation.rs`、50 行的 `stamp_resource_common.rs`、189 行的 `stamp_text_common.rs` 和 244 行的 `stamp_text_operation.rs`，分别承接 PNG/JPEG 严格解码与 PDF Image XObject/alpha mask 构建、图片 stamp 输入/布局/资源与内容流写入、Font/XObject/ExtGState 唯一资源命名和页面 Contents 安全追加、Helvetica 文本 stamp 渲染/旋转/透明度共享层，以及普通文本和动态页码两个公共操作。图片 PDF 生成现直接复用新的 embedded image 模块，三个 stamping 公共入口保持稳定，主文件旧实现和 stamp 专属类型/辅助函数已删除。迁移时将页码 format 已验证分支中的 `unreachable!` 改为显式 `Result` 错误。`pdf_edit.rs` 进一步由 1,461 行降至 444 行，PDF edit 文件集合当前共 8,558 行，相对原始单文件增加 626 行模块/可见性与显式共享边界开销；三十六个生产子模块全部低于 500 行。主文件已低于 800 行硬限制，陈旧的 7,932 行源码大小白名单已删除。该批次通过库编译、`-D warnings` Clippy、131 项 Artifacts 定向测试、全仓及 PDF scoped Rustfmt、补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；PDF 不再出现在源码大小或新增代码克隆违规中。
- Spreadsheet/XLSX 批次 1B 已完成：新增 90 行的 `spreadsheet/xlsx_model.rs`、302 行的 `spreadsheet/xlsx_input.rs`、260 行的 `spreadsheet/xlsx_generation.rs`、208 行的 `spreadsheet/xlsx_package.rs`、275 行的 `spreadsheet/xlsx_inspection.rs`、395 行的 `spreadsheet/xlsx_rewrite.rs` 和 177 行的 `spreadsheet/xlsx_package_write.rs`，分别承接稳定 cell/worksheet 模型、worksheet/cell/number-format 输入与公式策略适配、新建工作簿及 OOXML 条目生成、ZIP/relationship/workbook part 安全解析、inspection 与渲染前主动内容/外部关系/网络公式拒绝、范围更新的有序 worksheet XML 重写和公式重算标记，以及目标 symlink/覆盖策略、ZIP 新建/替换、临时文件同步、大小复验与原子持久化。package write 拆分同时合并了新建与重写流程重复的目标检查、父目录准备和 finalize/persist 边界。CSV/TSV 已确认由上层 `artifacts.rs` 独立实现，本批次未复制迁移。`create_xlsx`、`inspect_xlsx`、`validate_xlsx_for_render` 等公共入口保留稳定委托，旧实现已从主文件删除；`spreadsheet.rs` 由 2,293 行降至 624 行，Spreadsheet 文件集合当前共 2,331 行，仅增加 38 行模块/可见性与共享写回边界开销，七个生产子模块全部低于 500 行。主文件已低于 800 行硬限制，陈旧的 2,293 行源码大小白名单已删除。
- Spreadsheet 公式拆分触发的 25 行跨文件克隆已收口到 152 行的 `native/formula_safety.rs`：XLSX 与 Excel Live 共享字符集、函数 allowlist、worksheet reference、named range 和数值指数扫描，各自仅保留输入形态、长度、外部引用和 A1 parser 适配；`excel_live.rs` 由 4,937 行降至 4,836 行。该批次通过库编译、全仓 Rustfmt、`-D warnings` Clippy、131 项 Artifacts 定向测试和 Excel Live 17 项测试（1 项本机 Excel 探测按设计 ignored）；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，Spreadsheet/XLSX 不再出现在源码大小或新增代码克隆违规中。
- 已启动 Computer Use 批次 1C：新增 248 行的 `computer_use/tool_schema.rs`、129 行的 `computer_use/helper/helper_binary.rs` 和 180 行的 `computer_use/helper/protocol.rs`，分别承接只读/控制工具 Schema 与 macOS/Windows 平台过滤，helper 可执行文件定位、regular non-symlink/executable 校验、codesign 完整性和团队身份绑定，以及版本化请求/响应模型、严格字段白名单和有界 stdio 帧编解码。公共 `tool_definitions` 与 helper 调用入口保持稳定，现有 macOS 权限请求、签名策略和协议字段修改原样保留；`computer_use.rs` 由 6,896 行降至 6,667 行并将不可增长预算从 6,810 收紧到 6,667，`computer_use/helper.rs` 由 983 行降至 707 行并删除 890 行白名单。三个新生产模块均低于 500 行；该批次通过库编译、全仓 Rustfmt、`-D warnings` Clippy 和 30 项 Computer Use 定向测试，补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，两个 Computer Use 文件不再出现在源码大小或新增代码克隆违规中。
- Computer Use approval 批次已完成：形成当前 368 行的 `computer_use/approval.rs`，集中承接十类控制操作的审批命令参数、typed-text/layout 隐私隐藏策略、审计详情、审批安全/恢复语义，以及 display/window/application/snapshot 并发身份绑定所需的本地前置检查；父模块保留 `requires_interactive_approval`、`approval_command` 和 `redact_approval_arguments` 稳定委托入口，原实现已删除。`computer_use.rs` 进一步由 6,667 行降至 6,340 行，不可增长预算同步收紧到 6,340；新模块低于 500 行。该批次再次通过库编译、全仓 Rustfmt、`-D warnings` Clippy、30 项 Computer Use 定向测试和全部专项质量门禁，Computer Use 无新增源码大小或克隆违规。
- Computer Use observation 批次已完成：新增 140 行的 `computer_use/observation_model.rs` 和 395 行的 `computer_use/observation.rs`，分别承接 post-action observation 目标、frontmost/window identity 与状态守卫、rollback guard，以及 display/window/application activation 动作后的瞬态截图、取消恢复、rollback 结果装配、失败分类和 input release 契约。父模块继续保留原操作分派和测试契约，旧类型及实现已删除，未复制保留；`computer_use.rs` 进一步由 6,340 行降至 5,835 行，不可增长预算同步收紧到 5,835。两个新生产模块均低于 500 行；该批次通过库编译、全仓 Rustfmt、`-D warnings` Clippy、30 项 Computer Use 定向测试以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁。源码大小和新增代码克隆门禁仅保留计划中已记录的范围外违规，Computer Use 文件没有新增 finding。
- Computer Use action contract 批次已完成：新增 281 行的 `computer_use/action.rs`，集中承接 click、drag、key、typed text 与 scroll 的稳定动作模型、严格字段白名单、显示器坐标边界、按键/修饰键 allowlist、文本 Unicode/UTF-16/摘要限制、滚动范围、取消检查和 drag step 计算。approval、macOS 主实现与 Windows 模块直接复用同一 action contract，父模块旧实现已删除；`computer_use.rs` 进一步由 5,835 行降至 5,579 行，不可增长预算同步收紧到 5,579，新模块低于 500 行，`windows.rs` 保持既有 2,553 行预算不增长。该批次通过库编译、全仓 Rustfmt、30 项 Computer Use 定向测试以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；仅豁免范围外 `task_board` 的 `dead_code` 与 `items_after_test_module` 后，`-D warnings` Clippy 通过。完整严格 Clippy 当前被该范围外新增违规阻断；源码大小和克隆门禁也未报告 Computer Use 新 finding。
- Computer Use pointer action execution 批次已完成：新增 223 行的 `computer_use/pointer_action.rs` 和 44 行的 macOS 专用 `computer_use/input_guard.rs`，分别承接 click/drag 的 macOS CoreGraphics 执行、Windows 委托、unsupported 平台拒绝、稳定结果装配，以及键盘与鼠标动作共享的 RAII key/mouse-up 保证。双击间取消、drag 分步取消、事件创建失败释放和 unwind 后强制 up 的原安全语义保持不变，父模块旧实现已删除；`computer_use.rs` 进一步由 5,579 行降至 5,353 行，不可增长预算同步收紧到 5,353。两个新生产模块均低于 500 行，`windows.rs` 继续保持 2,553 行不增长。该批次通过库编译、全仓 Rustfmt、30 项 Computer Use 定向测试以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；仅豁免同一范围外 `task_board` 告警后，`-D warnings` Clippy 通过，完整严格 Clippy 仍仅被该外部违规阻断。源码大小和新增代码克隆门禁只报告既知范围外违规，两个新模块均无 finding。
- Computer Use keyboard/text execution 批次已完成：新增 82 行的 `computer_use/key_action.rs`、53 行的 `computer_use/scroll_action.rs`、87 行的 `computer_use/text_action.rs` 和 430 行的 macOS 专用 `computer_use/macos_text_target.rs`，分别承接普通按键 CoreGraphics 执行与 Windows 委托、滚轮事件执行与平台拒绝、UTF-16 文本事件/结果隐私元数据/平台委托，以及 Accessibility 对 frontmost application、focused element、PID、enabled/focused、secure/protected field、native/contenteditable writability、非空 bounds 和执行前身份重验证的完整安全边界。键盘与文本继续复用 44 行 RAII up guard，原实现和类型已从父模块删除；`computer_use.rs` 由 5,353 行降至 4,739 行，不可增长预算同步收紧到 4,739。四个新生产模块均低于 500 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、全仓 Rustfmt、30 项 Computer Use 定向测试以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；仅豁免同一范围外 `task_board` 告警后，`-D warnings` Clippy 通过，完整严格 Clippy 仍仅被该外部违规阻断。源码大小和新增代码克隆门禁只报告范围外违规，四个新模块均无 finding。
- Computer Use display safety 批次已完成：新增 314 行的 `computer_use/display.rs`，集中承接 display/approved display 稳定模型、审批身份 JSON、批准后身份与几何复验、active display layout 守卫、窗口最小可见区域约束、window-bounds display layout 审批绑定、display index 白名单、macOS CoreGraphics 与 Windows display 枚举、main/selected display 解析、只读 display 列表和平台名称。action、approval、window layout、capture、observation 与 Windows 模块继续复用同一类型和函数，父模块旧实现已删除；`computer_use.rs` 由 4,739 行降至 4,447 行，不可增长预算同步收紧到 4,447，新模块低于 500 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；仅豁免同一范围外 `task_board` 告警后，`-D warnings` Clippy 通过。完整全仓 Rustfmt 当前被范围外 `task_runner_service/backend/src/models/task.rs` 和 `plugin_management_policy.rs` 的并行未格式化修改阻断；源码大小和新增代码克隆门禁只报告范围外违规，`display.rs` 没有 finding。
- Computer Use capture 批次已完成：新增 353 行的 `computer_use/capture.rs`，集中承接 frontmost-window capture target 与 macOS identity 模型、macOS screencapture 参数白名单/私有临时目录/超时/有界 stderr/文件类型及 2 MiB 限额、捕获前后窗口身份与几何复验、macOS/Windows display 和 frontmost-window 平台委托，以及 JPEG/PNG、SHA-256、敏感内容标记和 transient `_model_input` 结果装配。Windows capture 继续复用同一公共结果边界，旧类型及实现已从父模块删除；`computer_use.rs` 由 4,447 行降至 4,127 行，不可增长预算同步收紧到 4,127，新模块低于 500 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；仅豁免同一范围外 `task_board` 告警后，`-D warnings` Clippy 通过。完整全仓 Rustfmt 仍被范围外两个 Task Runner 文件的未格式化修改阻断；源码大小和新增代码克隆门禁只报告范围外违规，`capture.rs` 没有 finding。
- Computer Use window control safety 批次已完成：新增 215 行的 `computer_use/window_control.rs`，集中承接 frontmost-window 批准身份与平台状态契约、窗口位置/尺寸参数边界、fullscreen/maximized 布尔输入、移动缩放与平台状态能力校验、批准参数序列化/反序列化，以及审计几何格式化。approval、display、observation、macOS 主实现和 Windows 模块继续复用同一稳定类型与校验函数，旧实现已从父模块删除；`computer_use.rs` 由 4,127 行降至 3,933 行，不可增长预算同步收紧到 3,933，新模块低于 500 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，两个 Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use window layout safety 批次已完成：新增 417 行的 `computer_use/window_layout.rs`，集中承接普通窗口布局身份与几何模型、显示器最小可见区域校验、规范 UUID 与 SHA-256 引用、短期 volatile 快照存储、10 分钟 TTL、最多 8 个快照的最旧项淘汰、批准参数绑定、一次性消费、重复窗口身份拒绝和只读捕获结果装配。approval、helper protocol、macOS 主实现与 Windows 模块继续复用同一布局类型；平台捕获、预检与恢复执行仍保留在父模块，旧模型及快照安全实现已删除且未复制保留。`computer_use.rs` 由 3,933 行降至 3,554 行，不可增长预算同步收紧到 3,554，新模块低于 500 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，两个 Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use platform dispatch 批次已完成：新增 368 行的 `computer_use/dispatch.rs`，集中承接 macOS helper 与本地执行路径选择、Windows 平台委托、unsupported 平台拒绝、只读窗口/显示器/截图/控件树操作分派、布局捕获与预检、已批准动作分派、取消前置检查，以及窗口/应用/普通输入动作后的 observation 与 rollback guard 装配。父模块继续保留 `execute` 和 `execute_approved` 两个稳定委托入口；helper 回环改为直接复用 dispatch 的本地执行函数，window layout 安全模块直接复用同一预检分派。旧实现已从父模块删除且未复制保留；`computer_use.rs` 由 3,554 行降至 3,242 行，不可增长预算同步收紧到 3,242，新模块低于 500 行，`helper.rs` 仅因三项显式 dispatch 导入的格式化边界由 707 行变为 709 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use permission/runtime safety 批次已完成：新增 193 行的 `computer_use/permissions.rs`，集中承接 macOS Accessibility 与 Screen Recording TCC 状态探测、提示专用 CoreFoundation dictionary FFI、Automation/ScreenCapture 可执行依赖检查、Windows/unsupported 平台行为、helper permission/dependency 路由，以及 observation runtime 的 fail-closed 前置检查。父模块继续保留 `dependency_error`、`screen_capture_dependency_error` 和 `request_permission` 三个稳定委托入口；helper 直接复用本地权限函数，dispatch 直接复用 runtime guard。主文件中权限专用 FFI 声明、callback 模型及旧实现已删除，其他 Accessibility/CoreGraphics 输入和显示器 FFI 保持原位且未复制；`computer_use.rs` 由 3,242 行降至 3,079 行，不可增长预算同步收紧到 3,079，新模块低于 500 行，`helper.rs` 仅因四项显式 permissions 导入的格式化边界由 709 行变为 711 行，`windows.rs` 保持 2,553 行不增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use window execution 批次已完成：新增 385 行的 `computer_use/window_execution.rs`，集中承接 macOS helper/Windows/unsupported 平台的前台窗口身份分派、普通窗口布局恢复、bounds/fullscreen/maximized 执行与回滚，以及 macOS 动作前批准能力复验、执行后取消检测、补偿恢复和失败元数据装配。approval、dispatch、helper 与 observation 通过限制在 `computer_use` 模块树内的统一重导出复用同一实现；主文件旧平台分派、macOS JXA 调用和取消恢复实现已删除且未复制保留。`computer_use.rs` 由 3,079 行降至 2,730 行，不可增长预算同步收紧到 2,730，新模块低于 500 行，`helper.rs` 与 `windows.rs` 未增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use application execution 批次已完成：新增 253 行的 `computer_use/application_execution.rs`，集中承接正整数 PID 输入、批准应用身份反序列化、macOS/Windows/unsupported 应用查询、前台应用身份捕获、批准后激活、取消回滚 guard，以及回滚结果 allowlist 和稳定安全元数据。approval、dispatch 与 observation 通过限制在 `computer_use` 模块树内的统一重导出复用同一类型和函数；通用字段拒绝和审批标签清洗继续留在父模块供其他职责共享。主文件旧应用模型、平台分派和激活/回滚实现已删除且未复制保留；`computer_use.rs` 由 2,730 行降至 2,510 行，不可增长预算同步收紧到 2,510，新模块低于 500 行，`window_execution.rs` 与 `windows.rs` 未增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use JXA execution runtime 批次已完成：新增 174 行的 `computer_use/jxa_runtime.rs`，集中承接 `/usr/bin/osascript` 启动、空 stdin 与管道输出、stdout/stderr 独立有界读取、8 秒超时后的 kill/wait/reader join 回收、只读 observation 与 action 结果策略、JSON object 解码，以及 macOS Accessibility 和 Screen Recording 错误分类。capture、application execution、dispatch 与 window execution 通过限制在 `computer_use` 模块树内的统一重导出复用同一运行时，测试继续复用同一解码和错误分类实现；脚本常量保持原位，主文件旧运行时实现已删除且未复制保留。`computer_use.rs` 由 2,510 行降至 2,361 行，不可增长预算同步收紧到 2,361，新模块低于 500 行，其他 Computer Use 子模块与 `windows.rs` 未增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use JXA script boundaries 批次已完成：新增 437 行的 `computer_use/jxa_observation_scripts.rs`、414 行的 `computer_use/jxa_window_scripts.rs` 和 145 行的 `computer_use/jxa_application_scripts.rs`，分别承接窗口枚举、普通窗口布局捕获/预检/恢复/回滚、前台窗口控件树 inspection 与截图目标，前台窗口身份/能力查询、bounds/fullscreen 执行及取消恢复，以及应用 PID 查询、激活、前台身份和取消恢复脚本。16 个脚本常量通过限制在 `computer_use` 模块树内的统一重导出继续供 capture、dispatch、window execution、application execution 和既有脚本编译合同复用，脚本文本保持原样；主文件旧常量已全部删除且未复制残留。`computer_use.rs` 由 2,361 行降至 1,394 行，不可增长预算同步收紧到 1,394，三个新生产模块均低于 500 行，其他 Computer Use 子模块与 `windows.rs` 未增长。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未扩大任何源码预算或增加 waiver。
- Computer Use test boundaries 收尾批次已完成：新增 6 行的 `computer_use/tests.rs`、266 行的 `computer_use/tests/actions.rs`、347 行的 `computer_use/tests/contracts.rs` 和 321 行的 `computer_use/tests/observation.rs`，分别路由并承接 click/drag/key/text/scroll/application 合同与恢复 metadata，工具发布、Windows/macOS 平台合同、窗口 bounds 与一次性布局快照安全，以及 JXA 解码/权限分类、文本目标、截图 transient metadata、post-action observation、窗口状态绑定和嵌入脚本编译合同。原测试断言保持不变，移动后的 Windows 源码合同改用相对于测试模块的稳定 `include_str!` 路径；主文件内约 930 行 `#[cfg(test)]` 实现已删除且未复制残留。`computer_use.rs` 由 1,394 行降至 464 行，四个测试路由/模块均低于 500 行，陈旧的 Computer Use 源码大小白名单已删除。该批次通过库编译、Computer Use scoped Rustfmt、30 项定向测试、仅排除既知范围外告警后的 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁只报告范围外违规，Computer Use 文件均无 finding。未增加 waiver。
- Excel Live platform script boundaries 批次已完成：新增 287 行的 `excel_live/platform_snapshot_scripts.rs`、222 行的 `excel_live/range_read_scripts.rs`、337 行的 `excel_live/macos_range_write_script.rs`、332 行的 `excel_live/windows_range_write_script.rs` 和 147 行的 `excel_live/script_fragments.rs`，分别承接 macOS/Windows 状态与快照脚本、两平台 bounded range read 模板、macOS/Windows range write/formatting 模板，以及 read/write 共用的单元格状态读取和 Windows 启动前缀。八个大型平台脚本常量已从主文件删除；read/write 执行路径在调用前展开受控静态模板，安全合同与 macOS JXA 编译测试验证最终完整脚本。拆分中发现的 Windows 单元格状态 50 行、macOS 单元格状态 37 行和 Windows 公共前缀 34 行重复已收口到唯一共享片段，未以复制方式迁移。`excel_live.rs` 由 4,836 行降至 3,460 行，不可增长预算同步精确收紧到 3,460；五个新生产模块均低于 500 行，Excel Live 文件集合当前共 4,785 行，较拆分前净减少 51 行。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小和新增代码克隆门禁仅报告范围外既有违规，Excel Live 文件均无 finding。未扩大预算或增加 waiver。
- Excel Live snapshot identity 批次已完成：新增 273 行的 `excel_live/snapshot_identity.rs`，集中承接平台快照 schema、安装/运行状态、workbook/sheet 数量与顺序、active/visibility 元数据的一致性校验，基于运行实例和私有 identity source 的不透明 workbook/worksheet SHA-256 身份，以及 status/workbook list 的只读公共投影。主文件继续负责平台执行和具体操作编排，通过限制在 `excel_live` 模块树内的内部函数复用同一规范化结果；旧实现已删除且未复制保留。迁移时将规范化 workbook identity、worksheet name、status 布尔状态和 workbook 列表的五处内部 `expect` 改为显式 `Result` 错误，未来规范化结构漂移不再触发 panic。`excel_live.rs` 由 3,460 行降至 3,207 行，不可增长预算同步精确收紧到 3,207；新模块低于 500 行，Excel Live 文件集合当前共 4,805 行，较原始 4,836 行仍净减少 31 行。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，`snapshot_identity.rs` 无 finding。未扩大预算或增加 waiver。
- Excel Live range target 批次已完成：新增 140 行的 `excel_live/range_target.rs`，集中承接 normalized/raw snapshot 的 workbook/worksheet 双向身份复验、运行实例与私有 identity source 绑定、一基 workbook/worksheet 位置校验、read-only/protected/visibility 可写状态拒绝，以及 bounded range 平台 bridge 请求装配。主文件保留共享 `RangeReadTarget`/`A1Range` 模型和具体 read/write 编排，旧目标解析、可写状态守卫与请求装配实现已删除且未复制保留。迁移时将 workbook/worksheet 索引、名称、只读状态、可见性和保护状态的七处内部 `expect` 改为显式 `Result` 错误，并在索引参与数组定位前增加显式非零复验，未来规范化结构漂移不再触发 panic 或下标下溢。`excel_live.rs` 由 3,207 行降至 3,068 行，不可增长预算同步精确收紧到 3,068；新模块低于 500 行，Excel Live 文件集合当前共 4,806 行，较原始 4,836 行仍净减少 30 行。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，`range_target.rs` 无 finding。未扩大预算或增加 waiver。
- Excel Live range response normalization 批次已完成：新增 254 行的 `excel_live/range_response.rs`，集中承接平台 bounded range 响应的 schema、运行实例、workbook/worksheet、range geometry、cell 顺序和数量复验，JSON scalar/text/formula redaction/external-reference/number-format 元数据的严格规范化，以及 read/write/formatting 三条返回路径共享的公共单元格行投影。私有原始 number format 字段继续只参与 snapshot 与并发校验，不进入公共 cells；异常 normalized cell object 和零列行装配改为显式 `Result` 拒绝，write/formatting 响应也同步使用可失败投影边界。旧规范化与投影实现已从主文件删除且未复制保留。迁移时删除公共单元格投影的一处内部 `expect`，未来规范化结构漂移不再触发 panic。`excel_live.rs` 由 3,068 行降至 2,839 行，不可增长预算同步精确收紧到 2,839；新模块低于 500 行，Excel Live 文件集合当前共 4,831 行，较原始 4,836 行仍净减少 5 行。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，`range_response.rs` 无 finding。未扩大预算或增加 waiver。
- Excel Live range snapshot/mutation response 批次已完成：新增 31 行的 `excel_live/range_snapshot.rs` 和 289 行的 `excel_live/mutation_response.rs`，分别承接绑定平台、运行实例、workbook/worksheet、规范 range 与完整私有 cell snapshot 的稳定 SHA-256 ID，以及 write/number-format bridge 的 written/formatted/rolled_back/rollback_failed 状态处理、目标内容/公式/number format 保持性比较和最终公共 mutation 响应装配。`range_response.rs`、read/write/formatting 编排继续复用同一可失败 snapshot ID；写入与格式化均通过共享公共单元格投影隐藏私有原始格式字段。旧 mutation 校验、比较、响应装配与 snapshot 哈希已从主文件删除且未复制保留。迁移时将 normalized cells 的 JSON 序列化 `expect` 改为显式 `Result` 错误，未来序列化行为变化不再触发 panic。`excel_live.rs` 由 2,839 行降至 2,552 行，不可增长预算同步精确收紧到 2,552；两个新模块均低于 500 行，Excel Live 文件集合当前共 4,865 行，相对原始 4,836 行仅增加 29 行模块边界与显式错误传播开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，两个新模块均无 finding。未扩大预算或增加 waiver。
- Excel Live mutation input/snapshot safety 批次已完成：新增 336 行的 `excel_live/mutation_input.rs` 和 121 行的 `excel_live/mutation_safety.rs`，分别承接 write/number-format 精确输入字段、snapshot ID、矩阵形状、typed cell、公式/文本注入边界、number-format preset、审批内容摘要、content/format bridge request 装配，以及 rollback snapshot 的完整对象/地址、截断、隐藏或外部公式、number format 可恢复性和既有 cell 内容 allowlist 校验。写入与格式化共用单一 `exact_restorable_cell` 安全边界；迁移中门禁发现并随后消除了两个安全函数的 25 行重复。旧输入解析、bridge request、摘要与 rollback 安全实现已从主文件删除且未复制保留。迁移时将 normalized cell 对象、地址、状态和值的六处内部 `expect` 改为显式 `Result` 错误，并删除已由 snapshot ID 格式验证保证的冗余 debug assertion。`excel_live.rs` 由 2,552 行降至 2,099 行，不可增长预算同步精确收紧到 2,099；两个新模块均低于 500 行，Excel Live 文件集合当前共 4,868 行，相对原始 4,836 行仅增加 32 行模块边界、共享校验与显式错误传播开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，两个新模块均无 finding。未扩大预算或增加 waiver。
- Excel Live A1 range/common validation 批次已完成：新增 130 行的 `excel_live/range_reference.rs` 和 96 行的 `excel_live/validation.rs`，分别承接规范 uppercase A1 range/cell 解析、Excel 1,048,576 行与 16,384 列边界、256-cell 上限、列名生成、公式外部 workbook/path/URL 引用识别，以及 exact arguments、必选/可选有界文本、布尔值和受限整数读取。read、write、formatting、snapshot identity、range target/response 与公式 allowlist 继续通过父模块内部导入复用同一权威实现；旧解析与通用验证实现已从主文件删除且未复制保留。迁移时将 A1 首段获取和 ASCII 列名转换的两处内部 `expect` 分别改为显式 `Result` 错误和确定性字符构造，未来内部输入或实现变化不再触发 panic。`excel_live.rs` 由 2,099 行降至 1,903 行，不可增长预算同步精确收紧到 1,903；两个新模块均低于 500 行，Excel Live 文件集合当前共 4,898 行，相对原始 4,836 行增加 62 行模块边界与显式错误传播开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，两个新模块均无 finding。未扩大预算或增加 waiver。
- Excel Live platform bridge runtime 批次已完成：新增 378 行的 `excel_live/platform_bridge.rs`，集中承接 macOS/Windows 状态、快照、bounded range read 与 write/formatting 的平台分派，Excel 安装与固定 PowerShell 可执行文件探测，regular-file/symlink 拒绝，JSON bridge stdin 写入，stdout/stderr 独立有界读取，8/20 秒超时后的 kill/wait/reader join 回收，以及写入失败后的 fail-closed 错误分类。父模块仅保留稳定的 `dependency_error` 委托入口，测试脚本导入限制为 `cfg(test)`；旧平台运行时实现已删除且未复制保留。`excel_live.rs` 由 1,903 行降至 1,553 行，不可增长预算同步精确收紧到 1,553；新模块低于 500 行，Excel Live 文件集合当前共 4,926 行，相对原始 4,836 行增加 90 行模块边界、显式平台路由与进程回收开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，`platform_bridge.rs` 无 finding。未扩大预算或增加 waiver。
- Excel Live tool schema/approval 批次已完成：新增 195 行的 `excel_live/tool_schema.rs` 和 71 行的 `excel_live/approval.rs`，分别承接 status、workbook list/inspection、bounded range read、write 与 number-format 工具定义、mutation 工具发布开关和审批需求判定，以及 write/formatting 精确参数重解析、内容摘要、无明文审批参数绑定、批准参数复验与批准后执行路由。父模块保留 `tool_definitions`、`requires_interactive_approval`、`approval_command` 和 `execute_approved` 四个稳定委托入口，实际 read/write mutation 编排继续保持原位。迁移后新增代码克隆门禁发现 write/formatting schema 的 workbook、worksheet、range 与 snapshot 四组字段存在 27 行重复，已收口到唯一的 `mutation_target_properties` 构造函数，未以 waiver 掩盖。`excel_live.rs` 由 1,553 行降至 1,335 行，不可增长预算同步精确收紧到 1,335；两个新模块均低于 500 行，Excel Live 文件集合当前共 4,974 行，相对原始 4,836 行增加 138 行模块边界、共享 schema 构造与显式稳定委托开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，两个新模块均无 finding。未扩大预算或增加 waiver。
- Excel Live read/write execution orchestration 批次已完成：新增 147 行的 `excel_live/read_execution.rs` 和 181 行的 `excel_live/mutation_execution.rs`，分别承接审批门禁后的只读操作路由、status/workbook list/inspection 快照执行、bounded range read 前后身份复验与未安装/未运行错误分类，以及全局串行写锁、批准取消检查、write/formatting 输入解析、可写目标复验、乐观 snapshot ID、恢复安全校验、bridge mutation、写后 workbook/worksheet 身份与 cell 内容/格式二次复验和最终响应。父模块保留 `execute`、批准 write/formatting 委托和测试专用 `execute_with_snapshot` 稳定入口；各执行模块直接依赖权威 input、target、response、snapshot 与 platform bridge 模块，没有复制校验逻辑。`excel_live.rs` 由 1,335 行降至 1,078 行，不可增长预算同步精确收紧到 1,078；两个新模块均低于 500 行，Excel Live 文件集合当前共 5,045 行，相对原始 4,836 行增加 209 行模块边界、稳定委托与显式执行依赖开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，两个新模块均无 finding。未扩大预算或增加 waiver。
- Excel Live test boundaries 收尾批次已完成：新增 101 行的 `excel_live/tests.rs`、368 行的 `excel_live/tests/contracts.rs`、237 行的 `excel_live/tests/responses.rs` 和 192 行的 `excel_live/tests/safety_and_platform.rs`，分别承接共享 snapshot/range bridge fixture 与测试路由，工具发布、身份、status、A1、输入、审批和 target 合同，read/write/formatting 响应与 snapshot ID 合同，以及 mutation 安全、外部/隐藏公式拒绝、macOS JXA 编译和本机 no-launch 探测。原 18 项测试断言保持不变，测试发现路径仅增加职责分组；迁移中删除了一处重复 `use super::*` 机械残留。`excel_live.rs` 由 1,078 行降至 192 行，四个测试路由/模块均低于 500 行，陈旧的 Excel Live 源码大小白名单已删除；包含主文件、生产模块与测试模块的 Excel Live 文件集合当前共 5,057 行，相对原始 4,836 行增加 221 行明确模块边界、共享 fixture 与显式依赖开销。该批次通过库编译、scoped Rustfmt、17 项 Excel Live 定向测试（另 1 项本机 Excel 探测按设计忽略）、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小与新增代码克隆门禁仅报告范围外既有违规，Excel Live 文件均无 finding。未增加 waiver。
- Plugin Runtime Relay test boundaries 起始批次已完成：新增 47 行的 `plugin_runtime_relay/tests.rs`、294 行的 `plugin_runtime_relay/tests/validation.rs`、215 行的 `plugin_runtime_relay/tests/constraints.rs` 和 120 行的 `plugin_runtime_relay/tests/native.rs`，分别承接共享 immutable plugin/component snapshot fixture 与测试路由，UI/Command/Agent/Hook 响应和 runtime error/interactive timeout 合同，Agent/Command 约束、relay origin、prepare identity、server name 与 tool lifecycle 映射合同，以及 Native Skill 与 transient model image 合同。原 16 项断言保持不变，生产逻辑未改；`plugin_runtime_relay.rs` 由 2,972 行降至 2,309 行，不可增长预算从 2,968 精确收紧到 2,309，四个测试路由/模块均低于 500 行，Relay 文件集合当前共 2,985 行，相对拆分前仅增加 13 行模块边界。该批次通过 Task Runner 库编译、scoped Rustfmt、16 项 Plugin Runtime Relay 定向测试、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；`plugin_runtime_relay.rs` 已从源码大小违规列表移除，全工作树大小违规由 12 项降至 11 项，新增代码克隆门禁仅报告范围外既有两项 finding。未扩大预算或增加 waiver。
- Plugin Runtime Relay prepare response validation 批次已完成：新增 339 行的 `plugin_runtime_relay/prepare_validation.rs`，集中承接 immutable plugin/release/version/artifact/component/permission prepare identity，Hook set canonical normalization、command hash coverage 与 snapshot SHA-256，UI source/metadata/assets/bridge/CSP/sandbox/size 与 snapshot SHA-256 校验，以及唯一共享的小写 SHA-256 格式边界。父文件保留 `validate_prepare_response`、`validate_hook_response` 和 `validate_ui_response` 三个稳定委托入口，artifact descriptor 校验直接复用同一 `is_lower_sha256`；Command、Agent 与 Native Skill 校验保持原位，未复制共享逻辑。`plugin_runtime_relay.rs` 由 2,309 行降至 2,007 行，不可增长预算同步精确收紧到 2,007；新生产模块低于 500 行，Relay 文件集合当前共 3,022 行，相对原始 2,972 行增加 50 行显式模块边界与稳定委托开销。该批次通过 Task Runner 库编译、scoped Rustfmt、16 项 Plugin Runtime Relay 定向测试、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小门禁仍只报告 11 项范围外既有违规，新增代码克隆门禁仍只报告范围外既有两项 finding，`prepare_validation.rs` 无 finding。未扩大预算或增加 waiver。
- Plugin Runtime Relay prepare orchestration 批次已完成：新增 412 行的 `plugin_runtime_relay/prepare_execution.rs`，整体承接 immutable prepare 请求体构造、Skill/MCP/Command/Agent/Hook/UI 组件分派、prepare response/session identity 与 operations 装配、skill/command/agent prompt item 生成、Native Skill/MCP provider 和 bounded relay server 创建，以及 prepared runtime 的 server/provider/prompt/session 聚合。父文件继续保留 `RunService::prepare_plugin_runtime` 中 HookSet 优先准备、BeforePluginPrepare、普通组件、SessionStart、UI ready 和任一失败后的 cancel-all 生命周期顺序；新模块通过既有稳定入口复用 prepare/Hook/UI/Command/Agent/Native 校验，没有复制验证逻辑。`plugin_runtime_relay.rs` 由 2,007 行降至 1,616 行，不可增长预算同步精确收紧到 1,616；新生产模块低于 500 行，Relay 文件集合当前共 3,043 行，相对原始 2,972 行增加 71 行显式状态机边界、依赖和稳定入口开销。该批次通过 Task Runner 库编译、scoped Rustfmt、16 项 Plugin Runtime Relay 定向测试、专项 `-D warnings` Clippy，以及补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁；源码大小门禁仍只报告 11 项范围外既有违规，新增代码克隆门禁仍只报告范围外既有两项 finding，`prepare_execution.rs` 无 finding。未扩大预算或增加 waiver。
- Plugin Runtime Relay component response validation 批次已完成：新增 350 行的 `plugin_runtime_relay/component_response_validation.rs`，集中承接 Command 参数定义与 snapshot SHA-256、Agent immutable metadata 与 snapshot SHA-256，以及 Native Skill plugin/release/component/bundle identity、tool/audit snapshot 和权限元数据校验。父文件保留 `validate_command_response`、`validate_agent_response`、`validate_native_skill_response` 三个稳定委托入口，并继续唯一承接 Command/Agent 约束读取、prompt 生成和共享必填响应文本边界；新模块直接复用 SDK 权威 snapshot hash 函数，没有复制约束或 prompt 逻辑。`plugin_runtime_relay.rs` 由 1,616 行降至 1,304 行，不可增长预算同步精确收紧到 1,304；新生产模块低于 500 行，Relay 文件集合当前共 3,081 行，相对原始 2,972 行增加 109 行显式模块边界、依赖与稳定委托开销。该批次通过 Task Runner 库编译、16 项 Plugin Runtime Relay 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过，源码大小门禁仍只报告 11 项范围外既有违规，新增代码克隆门禁仍只报告范围外既有两项 finding，`component_response_validation.rs` 无 finding。未扩大预算或增加 waiver。
- Plugin Runtime Relay relay client 批次已完成：新增 322 行的 `plugin_runtime_relay/relay_client.rs`，集中承接 Relay base URL 环境配置与 HTTP(S) origin 校验、内部服务令牌和请求头、设备/workspace 路由、普通与 Hook dispatch 分级超时、4 MiB 有限响应读取、JSON/status 错误映射、运行阶段审计事件、耗时统计与有界错误脱敏。父文件继续通过稳定的 `PluginRelayClient::from_task` / `request` 边界完成 prepare、execute、cancel 和 Hook 生命周期调用，并仅访问运行身份、store 与 workspace 等明确可见字段；`plugin_relay_base_url` 继续经原服务入口供 MCP API 复用。迁移后 `plugin_runtime_relay.rs` 由 1,304 行降至 994 行，不可增长预算同步精确收紧到 994；新生产模块低于 500 行，Relay 文件集合当前共 3,093 行，相对原始 2,972 行增加 121 行显式客户端边界、可见性与稳定导出开销。新增代码克隆门禁最初识别出错误脱敏实现与 Local Connector telemetry 的重复，已改为独立 token 分类与单次长度累积实现，在保持脱敏输出合同的同时消除本批 finding。该批次通过 Task Runner 库编译、scoped Rustfmt、16 项 Plugin Runtime Relay 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过，源码大小门禁仍只报告 11 项范围外既有违规，新增代码克隆门禁最终仍只报告范围外既有两项 finding。未扩大预算或增加 waiver。
- Plugin Runtime Relay Hook lifecycle 收尾批次已完成：新增 246 行的 `plugin_runtime_relay/hook_lifecycle.rs`，集中承接 Hook 生命周期 outcome、prepared Hook session 顺序分发、工具调用前后置事件与 outcome/summary SHA-256 映射、server-to-component 归属、fail-run 阻断错误汇总与有界脱敏审计，以及 Hook execute response 的 event/snapshot/blocking failure 复验。父文件保留 `PreparedPluginRuntime` 与 `PreparedPluginSession` 数据边界，通过固有方法继续向 Run preparation 和 model execution 暴露 `dispatch_hook_event`、`tool_lifecycle_hook` 与 `dispatch_prepared_plugin_hooks` 稳定入口；测试专用 Hook mapper 类型仅在测试构建中导入，没有扩大生产 API。`plugin_runtime_relay.rs` 由 994 行降至 779 行并满足 800 行硬上限，陈旧源码大小白名单已删除；新生产模块低于 500 行，Relay 文件集合当前共 3,124 行，相对原始 2,972 行增加 152 行明确生命周期边界、可见性和稳定入口开销。该批次通过 Task Runner 库编译、scoped Rustfmt、16 项 Plugin Runtime Relay 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，最终源码大小与克隆门禁只报告范围外既有 11 项体积违规和两项重复 finding。未扩大预算或增加 waiver。
- Plugin Management Policy immutable selection 起始批次已完成：新增 372 行的 `plugin_management_policy/plugin_selection.rs`，集中承接 resolved Plugin 支持性判断、active installation 与 immutable Release identity 复验、Skill/Command/Agent 唯一选择和目标 Agent 兼容性、Skill/MCP/Command/Agent/Hook/UI effective component inclusion、component snapshot pinning、Command invocation arguments 注入，以及 permission/auth connection 去重后的 `RunPluginSnapshot` 构造。父文件继续唯一承接 capability policy、可选择目录投影、Task config 应用、服务解析和通用 ID/Command 输入规范化，并通过 `validate_supported_plugin`、`validate_plugin_component_selection` 与 `plugin_snapshot` 稳定内部入口复用新模块；没有复制 snapshot 或兼容性逻辑。`plugin_management_policy.rs` 由 1,455 行降至 1,106 行，不可增长预算从 1,442 精确收紧到 1,106；新生产模块低于 500 行，Policy 文件集合当前共 1,478 行，相对拆分前增加 23 行显式模块边界与依赖开销。该批次通过 Task Runner 库编译、scoped Rustfmt、22 项 Plugin Management Policy 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，`plugin_management_policy.rs` 已从源码大小违规列表移除，全工作树体积违规由 11 项降至 10 项，克隆门禁仍只报告范围外既有两项 finding。未扩大预算或增加 waiver。
- Plugin Management Policy selectable catalog projection 批次已完成：新增 204 行的 `plugin_management_policy/selectable_views.rs`，集中承接 External MCP、Plugin、Command 与 Agent 的 crate 内序列化视图，以及 resolved capability 到可选择目录的投影；Command target Agent/default execution compatibility、confirmation/argument hint/allowed tools 和 Agent base Agent/max iterations 元数据保持原判定。父文件继续唯一承接 selectable MCP/Plugin 权威集合筛选，新模块通过 `TaskRunnerCapabilityPolicy` 固有方法保留 `selectable_external_mcp_views` 与 `selectable_plugin_views` 调用方式；迁移中将测试对父模块偶然 `PluginComponentKind` 导入的依赖改为测试文件显式 SDK 导入。`plugin_management_policy.rs` 由 1,106 行降至 911 行，不可增长预算同步精确收紧到 911；新生产模块低于 500 行，Policy 文件集合当前共 1,487 行，相对拆分前增加 32 行显式 DTO 模块边界、依赖与 crate 内可见性开销。该批次通过 Task Runner 库编译、scoped Rustfmt、22 项 Plugin Management Policy 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，Policy 文件无体积或克隆 finding。门禁期间共享工作树新增范围外 `sandbox_manager_service/backend/src/service/images.rs` 体积违规，因此当前全局体积违规为 11 项；克隆门禁仍只报告范围外既有两项 finding。未扩大预算或增加 waiver。
- Plugin Management Policy Task config application 生产收尾批次已完成：新增 213 行的 `plugin_management_policy/task_config_application.rs`，集中承接 required capability availability 复验、optional/required builtin 与 external MCP 交并集、planning allowlist 注入、required Skill policy revision、optional/required Plugin effective selection、Command invocation 与 device/workspace 规范化，以及 browser Plugin 到 `BrowserTools` 的可用性校验和依赖注入。父文件继续唯一承接 capability 解析、选择校验、snapshot 与 prompt 查询；`apply_to_task` 和 `apply_plugins_to_task` 继续作为 `TaskRunnerCapabilityPolicy` 固有方法供 Run Control 与 prerequisite queueing 原路径调用。`plugin_management_policy.rs` 由 911 行降至 715 行并满足 800 行硬上限，陈旧源码大小白名单已删除；新生产模块低于 500 行，Policy 文件集合当前共 1,504 行，相对拆分前增加 49 行显式应用边界、依赖和稳定方法开销。该批次通过 Task Runner 库编译、scoped Rustfmt、22 项 Plugin Management Policy 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，Policy 生产文件无体积或克隆 finding，当前全局门禁仍只报告范围外既有 11 项体积违规与两项重复 finding。未扩大预算或增加 waiver。
- Plugin Management Policy test boundaries 收尾批次已完成：原 1,421 行的 `plugin_management_policy/tests.rs` 已缩减为 6 行测试路由，并新增 117 行的 `tests/fixtures.rs`、184 行的 `tests/fixtures/base_plugin.rs`、340 行的 `tests/fixtures/component_plugins.rs`、142 行的 `tests/fixtures/core.rs`、213 行的 `tests/capability.rs` 和 472 行的 `tests/plugin_selection.rs`，分别承接 Task/Policy 共享 fixture、基础 Plugin fixture、UI/Command/Agent/Hook Plugin fixture、MCP/Skill fixture、capability/MCP policy 合同，以及 immutable Plugin selection/snapshot 合同；全部测试模块低于 500 行。原 22 项断言与测试语义保持不变，测试文件集合当前共 1,474 行，相对原单文件增加 53 行明确模块边界、fixture 路由与显式导入开销；嵌套 fixture helper 的可见性精确限制在 `tests` 模块内，未扩大生产 API。该批次通过 Task Runner 库编译、scoped Rustfmt、22 项 Plugin Management Policy 定向测试和专项 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过，本批未新增源码大小或代码克隆 finding。当前全局门禁仍只报告范围外既有 11 项体积违规与两项重复 finding，未扩大预算或增加 waiver。
- Local Connector Plugin API credential/OAuth 起始批次已完成：新增 275 行的 `api/handlers/plugins/credential_oauth.rs`，整体承接活动 Plugin installation 复验、Credential scope 构造与 list/upsert/delete、OAuth connection list/begin/browser open/callback query/complete/error mapping/disconnect，以及 owner/device/release/component 身份绑定。父文件通过原 `pub(crate)` handler 名称继续供顶层 API 路由使用，未扩大可见性或改变端点；共享工作树中既有的安装后 Skill 发布与卸载请求解析改动原样保留。迁移后 `plugins.rs` 由 1,354 行降至 1,101 行，不可增长预算从 1,231 精确收紧到 1,101；新生产模块低于 500 行，Plugin API 文件集合当前共 1,376 行，相对拆分前增加 22 行显式模块边界与导入开销。该批次通过 `local_connector_client_core` 库编译、scoped Rustfmt、4 项 Plugin API handler 测试、5 项 Credential Vault 安全测试和生产库专项 `-D warnings` Clippy；全目标 Clippy 当前仅被范围外 `local_runtime/api/task_board/mod.rs` 的 `items_after_test_module` 阻断，本批未增加 waiver。补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，`plugins.rs` 已从源码大小违规列表移除，全工作树体积违规由 11 项降至 10 项，克隆门禁仍只报告范围外既有两项 finding。未扩大预算或增加持久化 waiver。
- Local Connector Plugin API network lifecycle 收尾批次已完成：新增 442 行的 `api/handlers/plugins/network_lifecycle.rs`，集中承接 auto-update 周期调度、retry/backoff 状态推进、远程 Marketplace install source 拉取与签名身份校验、云端认证读取、artifact proxy 状态/Header/Content-Length/SHA-256 复验、有限流式响应读取、下载进度节流持久化、失败事务拒绝，以及临时下载文件自动清理。父文件继续唯一承接 catalog 投影、网络安装事务、用户偏好、安装后 Skill 发布、rollback/uninstall/recovery 与事件 API，并通过内部 helper 复用新模块；auto-update handler 与后台 checker 保持原 `pub(crate)` 路由名称。迁移后 `plugins.rs` 由 1,101 行降至 685 行并满足 800 行硬上限，陈旧源码大小白名单已删除；两个新生产模块分别为 275 行和 442 行，Plugin API 文件集合当前共 1,402 行，相对拆分前 1,354 行增加 48 行显式模块边界、依赖与稳定转发开销。该批次通过 `local_connector_client_core` 库编译、scoped Rustfmt、4 项 Plugin API handler 测试、3 项 auto-update policy/state 测试、10 项 Plugin install/download/rollback/recovery 安全测试和生产库专项 `-D warnings` Clippy；全目标 Clippy 仍仅被范围外 `local_runtime/api/task_board/mod.rs` 的 `items_after_test_module` 阻断，本批未增加 waiver。补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，Plugin API 文件无体积或克隆 finding，当前全工作树仍只报告范围外既有 10 项体积违规与两项重复 finding。未扩大预算或增加持久化 waiver。
- Requirement Execution plan query/recovery 起始批次已完成：新增 442 行的 `requirement_execution_handlers/plan_query.rs`，集中承接精确 execution identity 与最新 cloud execution source message 查询、session/message 分页筛选、项目/需求 scope 校验、execution status/confirmation/pause/failure 响应投影、停止 marker 恢复，以及无 Task link 的 stale Planner 超时检测、fail-closed metadata 修复和持久化。父文件保留公共 GET handler 与 execute/confirm/mutate/rerun/stop 写路径，并通过 `get_requirement_execution_plan_inner`、`load_cloud_execution_source_message` 和 `execution_message_status` 三个稳定内部入口复用查询模块；测试 helper 仅在测试构建中导入，没有复制消息或状态逻辑。迁移后 `requirement_execution_handlers.rs` 由 2,508 行降至 2,100 行，不可增长预算从 2,336 精确收紧到 2,100；新生产模块低于 500 行，Requirement Execution handler 文件集合当前共 2,542 行，相对拆分前增加 34 行显式模块边界、依赖与稳定入口开销。该批次通过 Chatos Backend 库编译、scoped Rustfmt、18 项 Requirement Execution 定向合同测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，主文件已从源码大小违规列表移除，全工作树体积违规由 10 项降至 9 项，克隆门禁仍只报告范围外既有两项 finding。未扩大预算或增加 waiver。
- Requirement Execution execute/planning 批次已完成：新增 338 行的 `requirement_execution_handlers/execute_planning.rs`，整体承接执行请求 model/feedback/replacement identity 规范化、旧停止批次校验、feedback history 合并、需求与 Work Item scope/DAG/依赖校验、未完成任务选择和拓扑顺序、活动执行冲突检查、requirement document scope 拉取、Planner prompt 与用户可见消息构造、执行消息持久化、reviewing 状态同步、Chat use case 异步启动，以及 Planner recovery 上下文装配。父文件继续唯一承接 recovery reconcile 状态机，并通过既有查询模块 helper 复用旧批次消息与状态判定；Planner prompt/user message helper 仅为原合同测试保留测试构建导入，没有重新耦合生产父文件。迁移后 `requirement_execution_handlers.rs` 由 2,100 行降至 1,793 行，不可增长预算从 2,100 精确收紧到 1,793；两个新生产模块分别为 442 行和 338 行，Requirement Execution handler 文件集合当前共 2,573 行，相对原始 2,508 行增加 65 行显式模块边界、依赖与稳定委托开销。该批次通过 Chatos Backend 库编译、scoped Rustfmt、18 项 Requirement Execution 定向合同测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，Requirement Execution 文件无体积或克隆 finding，当前全工作树仍只报告范围外既有 9 项体积违规与两项重复 finding。未扩大预算或增加 waiver。
- Requirement Execution confirmation/dispatch 批次已完成：新增 282 行的 `requirement_execution_handlers/execution_dispatch.rs`，整体承接确认请求 execution identity 与 Planner project task scope 复验、实际 DAG scope 扩展和精确覆盖校验、Task Runner 批次确认、started run 到 execution link 的 run/status/callback 映射、执行中 Requirement 状态推进、message task tracking 与隐藏 turn 揭示，以及 pause/resume Task Runner dispatch、消息暂停状态持久化和计数响应装配。父文件保留公共 confirm/pause/resume handlers 与统一错误响应包装；确认和暂停分支仅共享既有上下文/身份 helper，没有改变调用顺序或合并差异化语义。迁移后 `requirement_execution_handlers.rs` 由 1,793 行降至 1,538 行，不可增长预算从 1,793 精确收紧到 1,538；三个新生产模块分别为 442 行、338 行和 282 行，Requirement Execution handler 文件集合当前共 2,600 行，相对原始 2,508 行增加 92 行显式模块边界、依赖与稳定委托开销。该批次通过 Chatos Backend 库编译、scoped Rustfmt、18 项 Requirement Execution 定向合同测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，Requirement Execution 文件无体积或克隆 finding，当前全工作树仍只报告范围外既有 9 项体积违规与两项重复 finding。未扩大预算或增加 waiver。
- Requirement Execution rerun/recovery support 批次已完成：新增 429 行的 `requirement_execution_handlers/rerun_support.rs`，集中承接 execution project task scope 读取与项目/需求身份复验、实际 DAG scope 扩展、rerun clone 精确范围校验、旧批次活动 Task Runner 状态同步与取消防护、`RequirementPlannerRecovery` 上下文、Planner outcome reconcile、replacement link scope 合并，以及规划覆盖失败消息。父文件继续保留公共 rerun handler、rerun 主编排与批次 retirement 状态机；测试对 `WorkItemPlanItem` 的依赖改为显式测试构建导入，没有重新扩大生产依赖。迁移后 `requirement_execution_handlers.rs` 由 1,538 行降至 1,141 行，不可增长预算从 1,538 精确收紧到 1,141；四个新生产模块分别为 442 行、338 行、282 行和 429 行，Requirement Execution handler 文件集合当前共 2,632 行，相对原始 2,508 行增加 124 行显式模块边界、依赖与稳定委托开销。该批次通过 Chatos Backend 库编译、scoped Rustfmt、18 项 Requirement Execution 定向合同测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过，源码大小与新增代码克隆门禁仍只报告范围外既有 9 项体积违规与两项重复 finding，Requirement Execution 文件无 finding。未扩大预算或增加 waiver。
- Requirement Execution rerun orchestration 生产收尾批次已完成：新增 499 行的 `requirement_execution_handlers/rerun_execution.rs`，整体承接停止批次身份与消息状态复验、原 project task scope 解析、活动旧任务防护、replacement execution message 创建、Task Runner DAG clone、映射范围扩展与校验、Work Item link 建立、旧批次 retirement、新批次确认启动、run/link/Requirement 状态回写、消息 tracking/reveal，以及最终响应与 post-start warning 装配。为确保新生产模块严格低于 500 行，clone mapping 解析、映射 task id 集合构造、started run 映射和失败 clone 丢弃统一收口到 `rerun_support.rs`，该支撑模块由 429 行增长到 498 行但仍低于上限，未复制逻辑。父文件只保留公共 rerun handler、通用 batch retirement 和 stop 编排；迁移后 `requirement_execution_handlers.rs` 由 1,141 行降至 645 行并满足 800 行硬上限，陈旧源码大小白名单已删除。五个新生产模块分别为 442 行、338 行、282 行、499 行和 498 行，Requirement Execution handler 生产文件集合当前共 2,704 行，相对原始 2,508 行增加 196 行明确模块边界、依赖、失败清理 helper 和稳定委托开销。该批次通过 Chatos Backend 库编译、scoped Rustfmt、18 项 Requirement Execution 定向合同测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过，源码大小与新增代码克隆门禁最终仍只报告范围外既有 9 项体积违规与两项重复 finding，Requirement Execution 文件无 finding。未扩大预算或增加 waiver。
- Plugin Manifest validation support 收尾批次已完成：新增 196 行的 `plugin_manifest/validation_support.rs`，集中承接 stdio MCP 环境变量数量/命名/Host 控制项/credential template 校验、品牌色、可选邮箱、HTTPS 与 loopback MCP URL、必填文本和统一 validation issue 构造。`validator.rs` 继续负责 manifest 顶层、组件、权限、依赖、路径、UI 与 transport 编排，通过模块内 helper 复用同一 issue 边界；旧实现已删除且未复制保留。迁移后 `validator.rs` 由 910 行降至 739 行并满足 800 行硬上限，陈旧源码大小白名单已删除，新生产模块低于 500 行。该批次通过 SDK 库编译、scoped Rustfmt、48 项 `chatos_plugin_management_sdk` 库测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，Plugin Manifest 文件无 finding。门禁期间共享工作树新增范围外 `CloudProjectRuntimeEnvironmentPanel.tsx` 892 行无白名单违规，因此全局体积违规仍为 9 项；两项既有重复 finding 未变化。未扩大预算或增加 waiver。
- Plugin Management SDK DTO test boundaries 收尾批次已完成：原嵌入 `dto.rs` 的 5 项 System MCP/Agent key、resource security、Local Connector status batch 与 Plugin component ownership 合同测试已整体迁入 135 行的 `dto/tests.rs`，断言和序列化语义保持不变，生产 DTO 类型与默认值实现未改。`dto.rs` 由 814 行降至 681 行并满足 800 行硬上限，无需新增源码大小白名单；测试模块低于 500 行。该批次通过 scoped Rustfmt、48 项 `chatos_plugin_management_sdk` 库测试和全目标 `-D warnings` Clippy；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，DTO 文件无 finding。全局源码大小违规由 9 项降至 8 项，两项范围外既有重复 finding 未变化。未扩大预算或增加 waiver。
- Plugin Management frontend locale boundaries 收尾批次已完成：原 886 行的 `i18n/messages.ts` 已拆为 445 行的 `messages.zhCN.ts` 与 445 行的 `messages.enUS.ts`，原文件缩减为 5 行稳定重导出入口；中英文 key/value 字典保持原样，`I18nProvider` 调用路径无需修改。两个新生产模块均低于 500 行，陈旧源码大小白名单已删除；文件集合相对拆分前只增加 9 行许可证与模块边界。该批次通过 TypeScript `--noEmit` 类型检查和 Vite 生产构建；补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，两份语言字典未产生 clone finding。全局源码大小违规由 8 项降至 7 项，两项范围外既有重复 finding 未变化。未扩大预算或增加 waiver。
- Local Task Runner execution test boundaries 收尾批次已完成：原内嵌于 `local_runtime/task_runner/execution/mod.rs` 的 memory context task-id redaction 合同迁入 88 行的 `execution/tests.rs`，finalization round、retry idempotency key 和 local Agent key 合同迁入 63 行的 `execution/execution_policy_tests.rs`；4 项原断言和模块路径保持不变，生产执行逻辑未改。主文件由 850 行降至 703 行并满足 800 行硬上限，两份测试模块均低于 500 行，无需新增源码大小白名单。该批次通过 `local_connector_client_core` 库编译、scoped Rustfmt、包含 completion 合同在内的 8 项 execution 定向测试和生产库 `-D warnings` Clippy；全目标 Clippy 仍只被范围外 `local_runtime/api/task_board/mod.rs` 的 `items_after_test_module` 告警阻断。补丁、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和本批新增代码克隆门禁均通过，全局源码大小违规由 7 项降至 6 项，两项范围外既有重复 finding 未变化。未扩大预算或增加 waiver。
- Presentation 拆分复验通过：`local_connector_client_core` 库编译、`-D warnings` Clippy 以及 Artifacts 定向测试 131 项全部成功；源码大小、热点预算、新增代码克隆和补丁格式门禁无违规。表格结构模块拆分后，插入、删除、移动、格式复制及安全拒绝等相关回归继续全部通过。
- 最近的 Presentation、DOCX、PDF、Spreadsheet、Computer Use、Excel Live、Plugin Runtime Relay、Plugin Management Policy、Local Connector Plugin API、Requirement Execution、Plugin Manifest、Plugin Management SDK DTO、Plugin Management locale 与 Local Task Runner execution 拆分均通过对应库编译和定向测试；本轮 scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过。最新源码大小快照仍被范围外的 `RequirementExecutionProcessModal.tsx`（2,036 行，预算 1,967 行）、`sandbox_manager_service/backend/src/service/images.rs`（1,166 行，预算 803 行）、`BrowserSessionPanel.tsx`（1,047 行，预算 843 行）、`local_runtime/storage/task_board/lifecycle.rs`（900 行，预算 806 行）、无白名单的 `CloudProjectRuntimeEnvironmentPanel.tsx`（897 行），以及 `native.rs`（849 行，预算 845 行）共 6 项阻断；这些文件当前均存在并行工作树修改，本轮未跨范围改写。全工作树新增代码克隆门禁另被范围外 `MessageTaskDrawer.tsx` 与 `RequirementExecutionProcessModal.tsx` 的 27 行重复，以及 `managed_preview.rs` 的两段 27 行重复阻断。全目标 Local Connector Clippy 另被范围外 `local_runtime/api/task_board/mod.rs` 的测试模块位置阻断；这些范围外变化未在当前拆分批次中改写、扩大预算或豁免。

完整复验命令（当前全工作树 `quality` 受上述外部源码大小变化和两项范围外克隆 finding 阻断）：

```bash
bash scripts/verify-repository.sh quality
bash scripts/verify-repository.sh rust-lint
bash scripts/verify-repository.sh rust-test
bash scripts/verify-repository.sh frontends
bash scripts/verify-repository.sh native-platform
bash scripts/precommit_openapi_contracts.sh
cargo +1.94.0 check -p local_connector_client_core --lib
cargo +1.94.0 fmt --all -- --check
cargo +1.94.0 clippy -p local_connector_client_core --all-targets -- -D warnings
cargo +1.94.0 test -p local_connector_client_core --lib skills::native::artifacts::tests:: -- --nocapture
git diff --check
```

阶段 0 尚未能由代码仓库自行完成的事项：确认默认/发布分支保护、必需检查和直接推送限制。Windows 原生 job 还需要在 GitHub runner 上完成首次实际执行。Presentation 批次 1A，以及 DOCX、PDF、Spreadsheet 批次 1B 均已满足生产子文件不超过 500 行且主文件低于 800 行的退出标准；DOCX、PDF 与 Spreadsheet 已完成既定职责拆分并删除陈旧源码大小白名单。Computer Use、Excel Live、Plugin Runtime Relay、Plugin Management Policy 与 Local Connector Plugin API 的生产拆分均已达到主文件低于 800 行、新生产模块低于 500 行并退出源码大小白名单的标准；Plugin API 主文件当前为 685 行，两个新生产模块分别为 275 行和 442 行。Plugin Management Policy 原 1,421 行测试单文件也已完成职责拆分，6 个测试/fixture 模块均低于 500 行。Phase 1E 的 Requirement Execution 生产拆分已达到主文件低于 800 行、新生产模块低于 500 行并退出源码大小白名单的标准：查询/恢复、execute/planning、confirmation/dispatch、rerun/recovery support 和 rerun orchestration 五批均已完成，主文件当前为 645 行。后续如继续收口，可按职责迁出 stop/retirement 编排，但不再属于解除源码大小阻断的必需项。

## 10. 2026-07-30 实施进展

- Native 入口测试边界收尾批次已完成：原嵌入 `local_connector_client/core/src/skills/native.rs` 的 runtime 入口合同迁入 62 行的 `skills/native/tests.rs`，生产逻辑未改；主文件由 849 行降至 789 行并退出源码大小白名单。2 项定向测试、`local_connector_client_core` 库编译和生产库 `-D warnings` Clippy 均通过。
- Task Board lifecycle support 批次已完成：新增 151 行的 `local_runtime/storage/task_board/lifecycle/support.rs`，集中承接 snapshot/summary 类型、scope/closure 规范化、状态映射、fingerprint 与 prerequisite 校验；`lifecycle.rs` 由 900 行降至 768 行并退出源码大小白名单。3 项 Task Board 存储定向测试、库编译和生产库 `-D warnings` Clippy 均通过。
- Cloud Runtime 前端视图边界批次已完成：新增 315 行的 `cloudRuntimeEnvironmentView.ts`，承接纯数据读取、格式化、detected stack 和 service config 投影；`CloudProjectRuntimeEnvironmentPanel.tsx` 由 951 行降至 663 行。TypeScript 类型检查、7 项组件测试和 scoped ESLint 均通过。
- Browser Session 详情视图批次已完成：新增 418 行的 `BrowserSessionDetails.tsx` 和 30 行的 `browserSessionView.ts`，承接详情渲染与纯视图投影；`BrowserSessionPanel.tsx` 由 1,047 行降至 705 行并退出源码大小白名单。TypeScript 类型检查、2 项组件测试和 scoped ESLint 均通过。
- Requirement Execution Modal 边界与去重批次已完成：新增 156 行的 `RequirementExecutionStartingModal.tsx` 和 94 行的 `RequirementExecutionModalShell.tsx`，通过共享 `RequirementExecutionModalFrame` 统一遮罩、shell、execution plane 徽章、标题与窗口控制；`RequirementExecutionProcessModal.tsx` 由 2,036 行降至 1,776 行，不可增长预算精确收紧到 1,776。Hook 返回值改为分组解构后，已消除与 `MessageTaskDrawer.tsx` 的 27 行克隆。TypeScript 类型检查、5 个测试文件共 20 项测试和 scoped ESLint 均通过。
- Project Environment image generation 测试边界批次已完成：原嵌入 `image_generation.rs` 的 runtime 推断 helper、fixture 与 6 项合同测试迁入 343 行的 `image_generation/tests.rs`，生产状态机未改；主文件由 960 行降至 616 行。`project_management_service_backend` 库编译、6 项定向测试和全目标 `-D warnings` Clippy 均通过。
- Managed Browser CDP 重复代码收尾批次已完成：`managed_preview.rs` 中输入命令与 PDF preview 共用的 WebSocket 限额配置、活动页面查找和 CDP target 附加逻辑已收口为共享 helper；调用方原有连接超时与响应超时边界保持不变。6 项 `managed_preview` 定向测试和 `chatos_mcp` 全目标 `-D warnings` Clippy 均通过，新增代码克隆门禁已由两段 27 行重复恢复为零违规。
- Native Artifacts operation dispatch 起始批次已完成：新增 263 行的 `skills/native/artifacts/dispatch.rs`，整体承接 PDF、DOCX、Spreadsheet、Presentation 与 Template Creator operation 到权威实现的分派；父模块继续通过原 `execute_with_cancellation` 名称向 `native` 路由暴露同一入口，函数可见性精确限制在 `crate::skills::native`，实际格式实现、取消参数和返回语义均未改变。`artifacts.rs` 由 2,285 行降至 2,041 行，不可增长预算同步精确收紧到 2,041；新生产模块低于 500 行。该批次通过 `local_connector_client_core` 库编译、131 项 Artifacts 定向测试、生产库 `-D warnings` Clippy、补丁格式和新增代码克隆门禁，未扩大预算或增加 waiver。
- Native Artifacts CSV/TSV 边界批次已完成：新增 380 行的 `artifacts/delimited.rs` 和 331 行的 `artifacts/delimited_format.rs`，分别承接 CSV/TSV inspect/create/range update、常规文件与原子持久化策略，以及 bounded cell 输入、严格引用解析、BOM/行尾/引号状态机和序列化。PDF annotation/attachment 同样复用的 lowercase SHA-256 校验继续保留在父模块，没有错误收窄为表格专用实现；测试仍通过父模块稳定内部名称调用。`artifacts.rs` 由 2,041 行降至 1,386 行，两个新生产模块均低于 500 行。该批次通过库编译、131 项 Artifacts 定向测试、生产库 `-D warnings` Clippy、补丁格式和新增代码克隆门禁。
- Native Artifacts Template Creator 生产收尾批次已完成：新增 365 行的 `artifacts/artifact_template.rs`、262 行的 `artifact_template_model.rs` 和 350 行的 `artifact_template_zip.rs`，分别承接模板 inspect/create/instantiate/render preview 编排、placeholder manifest/输入/值模型，以及 ZIP/XML occurrence 扫描、语义替换与原子重写。模板 placeholder count 原依赖内部不变量的 `expect` 已改为显式 `Result` 错误；`tests.rs` 对 `NamedTempFile` 的偶然父模块导入依赖改为测试文件显式导入。`artifacts.rs` 由 1,386 行降至 472 行并满足 500 行目标，陈旧的 2,285 行源码大小白名单已删除；operation dispatch、CSV/TSV 与 Template Creator 共 6 个新生产模块均低于 500 行。Artifacts 生产文件集合相对原单文件增加 138 行明确模块边界、依赖与显式错误处理开销。该批次通过 `local_connector_client_core` 库编译、131 项 Artifacts 定向测试、生产库 `-D warnings` Clippy、补丁格式和新增代码克隆门禁，未增加 waiver。
- DOCX Render test boundaries 起始批次已完成：原内嵌于 `artifacts/docx_render.rs` 的 15 项 packaged runtime manifest、fake runtime、PDF page export、DOCX/PDF/Spreadsheet/Presentation render、process-tree 与真实打包 runtime smoke 合同整体迁入 1,311 行的 `docx_render/tests.rs`，测试命名空间和 fixture 保持不变；`include_str!` 源码合同路径按新目录边界调整为 `../docx_render.rs`。测试文件低于计划建议的 1,500 行上限，主生产文件由 4,173 行降至 2,864 行。
- DOCX Render runtime support 批次已完成：新增 350 行的 `docx_render/runtime.rs` 和 141 行的 `docx_render/runtime_environment.rs`，分别承接 runtime manifest 数据模型、平台/版本/hash/规范化路径/普通文件与字体目录校验，以及私有 HOME/TMP/XDG、动态库路径、可信字体路径与 fontconfig 生成。规范化 runtime 路径遍历中原依赖前置验证的 `unreachable!` 已改为显式 `Result` 错误；测试所需 manifest 名称和平台标识只在 `docx_render` 内可见并由测试显式导入。`docx_render.rs` 由 2,864 行进一步降至 2,413 行，不可增长预算从 4,173 精确收紧到 2,413；两个新生产模块均低于 500 行。该批次通过库编译、15 项 DOCX Render 定向测试（10 项通过、5 项真实打包 runtime smoke 按设计忽略）、生产库 `-D warnings` Clippy、补丁格式和新增代码克隆门禁，未扩大预算或增加 waiver。
- DOCX Render process/output 边界批次代码迁移已完成：新增 237 行的 `docx_render/process.rs` 和 407 行的 `docx_render/output.rs`，分别承接有界 stdout/stderr 读取、子进程组配置、取消/超时后的进程树回收与诊断脱敏，以及 PNG page 集合/尺寸/hash/总量校验、全新导出目录事务、失败回滚和 PDF 原子持久化。PNG header 固定切片上的两个 `expect` 已改为显式 `Result` 错误；父模块继续通过稳定内部名称调用 process/output helper，测试构造所需 `RenderedPage` 仅在测试构建中导入。`docx_render.rs` 由 2,413 行降至 1,814 行，不可增长预算同步精确收紧到 1,814；两个新生产模块均低于 500 行。scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算、依赖漂移和新增代码克隆门禁均通过。Rust 库编译、定向测试与 Clippy 复验当前被范围外并行修改阻断：`crates/chatos_mcp_runtime/src/lib.rs` 仍导出已从 `builtin_catalog.rs` 删除的 `TASK_MANAGER_COMMAND`、`TASK_MANAGER_MCP_ID` 和 `TASK_MANAGER_SERVER_NAME`；本批未修改该并行工作树区域，待其恢复一致后补跑动态验证。
- Requirement Execution Modal 纯逻辑边界批次已完成：新增 230 行的 `requirementExecutionProcessModel.ts` 和 290 行的 `requirementExecutionPhase.ts`，分别承接执行批次数据模型、API 响应归一化、规划反馈历史、pending/cancellation 错误识别与 replacement/cancel 决策，以及阶段状态机、fallback message、流程文案、Runner 进度条目和恢复动作投影。主文件继续以原导出名称重导出公共合同，现有测试与调用方无需改路径；JSX、请求编排和 Hook 生命周期未改。`RequirementExecutionProcessModal.tsx` 由 1,776 行降至 1,320 行，不可增长预算同步精确收紧到 1,320；两个新生产模块均低于 500 行。TypeScript `--noEmit`、5 个 Requirement Execution 测试文件共 20 项测试和 scoped ESLint 均通过，未扩大预算或增加 waiver。
- Requirement Execution Modal 视图与操作区收尾批次已完成：新增 303 行的 `RequirementExecutionActionDialogs.tsx` 和 409 行的 `RequirementExecutionProcessView.tsx`，分别承接失败任务重试、取消/清理/重跑确认对话框，以及阶段侧栏、DAG 表面和底部操作区；三类确认对话框共用单一骨架，没有把重复 JSX 复制到新模块。`requirementExecutionPhase.ts` 进一步承接规划过程条目投影后为 411 行；主 `RequirementExecutionProcessModal.tsx` 由 1,320 行降至 794 行，正式低于 800 行硬上限并删除陈旧源码大小白名单。本批所有新生产模块均低于 500 行；TypeScript `--noEmit`、5 个 Requirement Execution 测试文件共 20 项测试和 scoped ESLint 再次全部通过。
- DOCX Render options/range 边界批次代码迁移已完成：新增 364 行的 `docx_render/options.rs`，集中承接 DOCX/Spreadsheet/PDF/Presentation 的页码或幻灯片范围、DPI、超时、PDF 持久化目标、PNG 导出目录与文件名前缀校验，以及实际页数已知后的最终范围选择。DOCX/Spreadsheet 与 Presentation 原本两份高度重复的 render option 解析已收口到一个 `RangeOptionSpec` 驱动的共享实现，没有通过 Clippy allow 或 clone waiver 掩盖参数边界。`docx_render.rs` 由 1,814 行降至 1,487 行，不可增长预算同步精确收紧到 1,487；新生产模块低于 500 行。scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁均通过，新模块无 clone finding；库编译复验仍在进入本模块前被范围外 `chatos_mcp_runtime` 的三个旧 Task Manager 常量导出阻断，本批未修改该并行区域。
- DOCX Render LibreOffice 共享运行时批次已完成：新增 113 行的 `docx_render/libreoffice.rs`，统一承接 Spreadsheet、Presentation 和 DOCX 转换共用的私有 output/profile/home/tmp 目录、固定 headless/safe-mode 参数、受信字体路径、私有 fontconfig 与 LibreOffice profile URL 构造。三条路径原有的 `fontconfig.parent().expect(...)` 全部改为统一显式错误，不再依赖内部路径不变式触发 panic；Spreadsheet/Presentation 仍启用 safe mode，DOCX 保持原参数行为。`docx_render.rs` 由 1,487 行降至 1,372 行，不可增长预算同步精确收紧到 1,372；新模块低于 500 行。
- DOCX Render Spreadsheet 格式适配批次已完成：新增 290 行的 `docx_render/spreadsheet_render.rs`，整体承接 XLSX 源文件/快照 hash、主动内容与 workbook 结构校验、LibreOffice PDF 转换、页数上限、Poppler rasterization、源文件二次 hash、可选 PDF 持久化和最终结构化响应。Spreadsheet、Presentation 和 DOCX 共用的 PNG metadata 与 transient model input 构造已收口到 `output.rs::transient_page_payload`，避免格式适配拆文件后引入新 clone；`output.rs` 由 407 行增至 447 行但仍低于 500 行。`docx_render.rs` 由 1,372 行降至 1,038 行，不可增长预算同步精确收紧到 1,038；新生产模块低于 500 行。
- DOCX Render PDF rasterization 去重批次已完成：新增 79 行的 `docx_render/pdf_rasterization.rs`，统一承接 PDF transient render 与 PDF page export 的私有 output/home/tmp 目录、Poppler 环境、页范围参数、超时/取消和失败分类。该 helper 消除了克隆门禁在主文件中报告的两段 25 行重复，复验后本批文件无 clone finding。`docx_render.rs` 由 1,038 行降至 986 行，不可增长预算同步精确收紧到 986；新生产模块低于 500 行。
- DOCX Render 动态复验状态已更新：范围外旧 Task Manager 常量导出不一致已在并行工作树中恢复，`cargo +1.94.0 check -p local_connector_client_core --lib` 现已成功。编译发现的 `docx_render/process.rs` Windows-only `PathBuf` 导入已改为 `cfg(windows)` 精确边界。15 项 DOCX Render 定向测试当前在进入本模块前被范围外 `chatos/backend/src/core/builtin_mcp_prompt.rs` 将 `Option<McpBuiltinServer>` 直接 `collect` 为 `Vec<McpBuiltinServer>` 的类型错误阻断；专项 `-D warnings` Clippy 被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。本批未跨范围修改这两个并行区域。
- DOCX Render Presentation 格式适配收尾批次已完成：新增 309 行的 `docx_render/presentation_render.rs`，整体承接 PPTX 源文件/快照 hash、主动内容与 package/relationship 校验、LibreOffice PDF 转换、页数与幻灯片选择边界、Poppler rasterization、源文件二次 hash、可选 PDF 持久化和最终结构化响应，并继续复用共享 transient PNG payload。`docx_render.rs` 由 986 行降至 706 行，正式低于 800 行硬上限并删除陈旧源码大小白名单；新生产模块低于 500 行。父模块仅以 `pub(super) use` 保留稳定内部入口，子模块函数可见性限制在 `crate::skills::native::artifacts`。测试模块补齐迁移后自身需要的 `PathBuf`、`Write` 与 SHA-256 导入，Windows 进程树源码合同改为读取权威 `process.rs`；15 项 DOCX Render 定向测试中 10 项通过、5 项真实打包 runtime smoke 按设计忽略。`local_connector_client_core` 库编译成功，scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Artifact Schema 格式与能力收尾批次已完成：新增 310 行的 `artifacts/schemas/pdf.rs`、339 行的 `pdf_annotations.rs`、380 行的 `docx.rs`、232 行的 `docx_advanced.rs`、166 行的 `spreadsheet.rs`、314 行的 `presentation.rs`、335 行的 `presentation_edit.rs` 和 100 行的 `artifact_template.rs`，分别承接 PDF 基础与 annotation/attachment/stamp、DOCX 文档结构与高级编辑、Spreadsheet、Presentation 创建/图表与编辑/表格，以及 Template Creator Schema。父模块按原顺序组合各定义集合；Presentation create/append 继续复用单一权威 input Schema，工具名称、描述、字段、约束和输出顺序均未改变。共享 text-table rows、Presentation chart properties 和 create/append Schema 变形中原依赖 JSON literal 形状的 5 个 `expect` 已全部改为无 panic 的 `Value::Object` 匹配。`schemas.rs` 由 2,197 行降至 125 行并删除陈旧源码大小白名单，8 个新生产模块均低于 500 行；Schema 文件集合当前共 2,301 行，仅增加 104 行模块边界、显式依赖和定义集合装配开销，未复制保留旧实现。该批次通过 `local_connector_client_core` 库编译、131 项 Artifacts 定向测试、scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Plugin Runtime Host 职责收尾批次已完成：新增 364 行的 `plugins/runtime/host/artifact_ui.rs`、195 行的 `plugin_lifecycle.rs`、260 行的 `support.rs`、381 行的 `execution.rs` 和 498 行的 `prepare.rs`，分别承接 UI asset 与 Artifact list/read/create/update/grant/审批、PluginDisabled Hook 与会话取消遥测、精确 session identity/权限请求/审计 hash/telemetry 公共辅助、已准备 Session 的 Skill/Agent/Command/Hook/UI/Native/MCP 执行分派，以及按组件类型构造不可变 Session 快照。父模块保留公开 handler、稳定构造接口、取消/会话存储与 Hook workspace-write 审批入口；PluginDisabled 和 mark-enabled 公开方法签名保持不变。`host.rs` 由 2,298 行降至 661 行并删除陈旧源码大小白名单，5 个新生产模块均低于 500 行；Host 文件集合当前共 2,359 行，仅增加 61 行模块边界、显式可见性和共享导入开销，未复制保留旧实现。该批次通过 `local_connector_client_core` 库编译和 37 项 Plugin Runtime 定向测试（34 项通过，3 项需预构建沙箱服务或真实 MongoDB、按设计忽略），scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Artifact Store 职责收尾批次已完成：原内嵌 6 项测试迁入 522 行的 `plugins/runtime/artifact_store/tests.rs`，新增 242 行的 `persistence.rs`、123 行的 `grant.rs` 和 449 行的 `validation.rs`，分别承接带 HMAC 完整性保护的 registry key/load/save 与原子持久化、UI Grant 请求匹配/读写授权/Artifact 保留判定，以及写入体/显示名/工作区路径/安全目录/原子文件写入和持久化状态全量校验。父模块保留 Store 数据模型、公开操作编排、输出注册和容量回收入口；原有调用签名、乐观并发语义、持久化格式和错误映射保持不变。`artifact_store.rs` 由 2,094 行降至 791 行并删除陈旧源码大小白名单，3 个新生产模块均低于 500 行；Artifact Store 文件集合当前共 2,127 行，仅增加 33 行模块边界、显式可见性和依赖导入开销，未复制保留旧实现。该批次通过 `local_connector_client_core` 库编译和 6 项 Artifact Store 定向测试；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 复验仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Plugin MCP Adapter 准备与校验边界收尾批次已完成：新增 425 行的 `plugins/runtime/mcp_adapter/preparation.rs`，整体承接 MCP required permission 校验、stdio/HTTP transport 构造、signed command 与 cwd 安全解析、参数/网络域名/credential/OAuth scope 权限校验、工具目录过滤去重与大小限制、活动 Release 快照复验，以及工具、MCP 和健康状态 hash/timestamp 辅助。父模块保留公开 Adapter/Prepared Session 数据模型、健康探测、调用/取消执行、Invoker 和 Manifest 加载入口；stdio sandbox、credential/OAuth binding、工具排序及快照内容保持不变。`mcp_adapter.rs` 由 1,098 行降至 685 行并删除陈旧源码大小白名单，新生产模块低于 500 行；MCP Adapter 文件集合当前共 1,110 行，仅增加 12 行模块边界和显式调用开销，未复制保留旧实现。该批次通过 `local_connector_client_core` 库编译和 37 项 Plugin Runtime 定向测试（34 项通过，3 项需预构建沙箱服务或真实 MongoDB、按设计忽略）；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Plugin OAuth Broker 刷新与支持边界收尾批次已完成：新增 227 行的 `plugins/runtime/oauth_broker/refresh.rs` 和 448 行的 `support.rs`，分别承接按 connection key 去重的 Token refresh、精确绑定复验、Refresh Token 临时解析与零化、刷新后持久化、失败后的 needs-auth 转换和 metadata seal 校验，以及 Connected App Manifest/endpoint/scope/token response 校验、PKCE 随机值与 callback 输入、连接 expiry/hash/key、Credential scope、受限状态读取和原子持久化。父模块保留公开 Broker/Connection/Authorization/Token Binding 数据模型、授权开始与回调、Token endpoint 请求、初次连接持久化和断开入口；OAuth PKCE、HTTP 限额、Vault handle 生命周期、并发锁和错误语义保持不变。`oauth_broker.rs` 由 1,358 行降至 724 行并删除陈旧源码大小白名单，两个新生产模块均低于 500 行；OAuth Broker 文件集合当前共 1,399 行，仅增加 41 行模块边界、显式依赖和可见性开销，未复制保留旧实现。该批次通过 `local_connector_client_core` 库编译和 37 项 Plugin Runtime 定向测试（34 项通过，3 项需预构建沙箱服务或真实 MongoDB、按设计忽略），覆盖 PKCE exchange、callback error 单次消费、refresh 去重/轮换、refresh 失败重新授权和 OAuth MCP 注入；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Bundled Plugin 测试边界收尾批次已完成：原内嵌于 `plugins/bundled.rs` 的 5 项 native skill inventory、staged bundle 安装/卸载、防篡改拒绝、更新/回滚和全量 release identity 合同测试整体迁入 220 行的 `plugins/bundled/tests.rs`；生产安装、bundle index/checksum/SBOM 校验、原子 staging、registry 更新和 rollback 逻辑未改。`bundled.rs` 由 966 行降至 747 行并删除陈旧源码大小白名单；文件集合当前共 967 行，仅增加 1 行测试模块边界。Bundled 专项测试中 1 项纯 inventory 合同通过、4 项需要 `CHATOS_TEST_BUNDLED_PLUGINS_DIR` 预构建 bundle 而按设计忽略，另有 2 项 bundled native runtime 权限与 bundle drift 测试通过；`local_connector_client_core` 库编译、scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；生产库 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Plugin Runtime SDK UI/Artifact wire contract 收尾批次已完成：新增 326 行的 `crates/chatos_plugin_management_sdk/src/plugin_runtime/ui_artifacts.rs`，整体承接 UI Bridge 协议常量、UI asset/snapshot/ready event、Bridge request/response/method、Artifact owner/descriptor/list/read/create/update/write response，以及 UI snapshot canonical hash；父模块通过 `pub use` 保持原 `plugin_runtime::*` 和 crate 根导出路径不变。原内嵌 4 项 preference、UI Bridge 与 Artifact closed-schema/乐观更新合同测试迁入 138 行的 `plugin_runtime/tests.rs`。`plugin_runtime/mod.rs` 由 1,164 行降至 712 行并删除陈旧源码大小白名单，新生产模块低于 500 行；文件集合当前共 1,176 行，仅增加 12 行模块边界、显式依赖和重导出开销，未复制保留旧定义。`chatos_plugin_management_sdk` 全量 48 项单元测试和 2 项集成测试通过，SDK 全目标 `-D warnings` Clippy 与 Local Connector Core 下游库编译通过；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding。
- Chatos Plugin UI Asset/CSP 与测试边界起始批次已完成：新增 265 行的 `chatos/backend/src/api/message_task_runner/plugin_ui/asset_response.rs`，集中承接 UI asset 身份、声明大小与 SHA-256 复验、HTML/static response 构造、no-store/nosniff/CORP/permissions-policy/origin-agent-cluster 安全 Header、不可变 CSP 与精确 parent origin 绑定、canonical asset path/media type 校验，以及 relay status 和标准 API error 映射。原内嵌 8 项 Ready Event、Workbench Session、Asset/CSP、Artifact read/write/download 与 relay token 合同测试迁入 499 行的 `plugin_ui/tests.rs`。`plugin_ui.rs` 由 2,140 行降至 1,398 行，不可增长预算同步从 2,140 精确收紧到 1,398；两个新文件均低于 500 行，文件集合当前共 2,162 行，仅增加 22 行模块边界、显式依赖与测试导入开销，未复制保留旧实现。8 项 Plugin UI 定向测试和 `chat_app_server_rs` 库编译通过；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding。主文件仍高于 800 行，后续继续迁出 Workbench Session 与 Artifact Relay；严格 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Chatos Plugin UI Workbench/Ready Validation 收尾批次已完成：新增 278 行的 `plugin_ui/workbench_handlers.rs`、231 行的 `workbench_session.rs` 和 154 行的 `ready_validation.rs`，分别承接 UI asset 与 Workbench/Artifact HTTP handlers、短生命周期会话签发/撤销/访问控制/能力校验/写入体解码，以及 Ready Event 身份和 UI Snapshot 安全描述符校验。父模块保留 Router、Local Connector relay 请求、Artifact response/descriptor 校验和下载响应构造；路由、公开行为、session 绑定、artifact 乐观并发与安全 Header 语义保持不变。`plugin_ui.rs` 由 1,398 行进一步降至 769 行，正式低于 800 行硬上限并删除陈旧源码大小白名单；三个新生产模块均低于 500 行。Plugin UI 文件集合当前共 2,196 行，相对原单文件仅增加 56 行模块边界、显式依赖和测试导入开销，未复制保留旧实现。`chat_app_server_rs` 库编译和 8 项 Plugin UI 定向测试全部通过；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；严格 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Plugin Management System Agent API 测试边界收尾批次已完成：原内嵌于 `plugin_management_service/backend/src/api/agents.rs` 的 3 项 Task Runner 云端/本地运行平面、规划/执行阶段、Builtin/System/External/Private MCP 可绑定性策略测试及其夹具整体迁入 222 行的 `api/agents/tests.rs`，并将仅供测试使用的 `agent_can_bind_mcp` 一并收口到测试模块；生产 handler、MCP/Plugin binding 持久化、不可用原因和排序策略未改。`agents.rs` 由 845 行降至 624 行，正式低于 800 行硬上限；文件集合当前共 846 行，仅增加 1 行测试模块边界。`plugin_management_service_backend` 库编译和 3 项定向测试全部通过；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；严格 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Chatos Task Runner API Client Message Task 边界收尾批次已完成：新增 258 行的 `chatos/backend/src/services/task_runner_api_client/message_tasks.rs`，整体承接 session active message tasks、message task/graph/run/event/output changes/output diff 查询和 run retry 请求；父模块继续通过原 `task_runner_api_client::*` 路径重导出全部十个公开函数，签名、内部鉴权 scope、URL/query 编码和响应语义保持不变。`task_runner_api_client.rs` 由 820 行降至 572 行，正式低于 800 行硬上限；新生产模块低于 500 行。包含既有 `types.rs` 和 `tests.rs` 的文件集合当前共 1,063 行，相对拆分前仅增加 10 行模块边界与显式重导出开销，未复制保留旧实现。`chat_app_server_rs` 库编译和 5 项内部签名、Token Exchange 与响应大小边界测试全部通过；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；严格 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Plugin Management Store Agent/Prompt/Binding 边界收尾批次已完成：新增 284 行的 `plugin_management_service/backend/src/store/agents.rs`，整体承接 System Agent CRUD、Agent Prompt 当前值/Bundle Version/历史 Release 持久化，以及运行时和管理端 Binding 查询、替换与删除；集合名称、MongoDB filter/sort/projection/upsert、Bundle Version 原子递增和公开 `AppStore` 方法签名保持不变。`store.rs` 由 959 行降至 682 行，正式低于 800 行硬上限；新生产模块低于 500 行。包含既有 `indexes.rs` 和 `plugins.rs` 的 Store 文件集合当前共 1,838 行，相对拆分前仅增加 7 行模块边界开销，未复制保留旧实现。`plugin_management_service_backend` 库编译和完整 92 项单元测试全部通过；scoped Rustfmt、补丁格式、生产 `unwrap`/`expect`、请求路径 panic、热点预算和依赖漂移门禁通过，本批文件无新增 clone finding；严格 `-D warnings` Clippy 仍在进入本模块前被范围外 `mcp/src/implementations/builtin/tool_registry.rs::block_on_option` 的 dead-code 告警阻断。
- Requirement Execution 前端大组件收尾完成：`RequirementExecutionProcessModal.tsx` 由回归后的 842 行降至 800 行，任务详情弹窗、任务运行态判定、稳定公共导出和阶段文案解析分别收口到 `RequirementExecutionTaskModals.tsx`、`requirementExecutionTaskRuntime.ts`、`requirementExecutionProcessPublic.ts` 和 `requirementExecutionPhase.ts`；原导入路径与交互行为保持不变。Scoped ESLint、TypeScript type-check 与 5 个专项测试文件的 21 项测试全部通过。
- Local Task Runner Service Provider 测试边界收尾完成：`service_provider.rs` 由 1,581 行降至 1,553 行，内嵌测试迁入 `service_provider/dependency_reduction_tests.rs`，不可增长预算同步收紧至 1,553；未扩大预算或增加 waiver。已降至 736 行的 `runtime_state.rs` 与已删除的 `task_manager_bridge/task_ops.rs` 陈旧白名单条目已删除。
- Task Manager 更新模型克隆收尾完成：新增共享泛型 `chatos_mcp_runtime::TaskUpdatePatch<T>`，Chatos Backend 与 MCP 仅保留绑定各自 `TaskOutcomeItem` 的类型别名；Chatos 专属规范化与空值判定保留为本地函数。原 32 行跨 crate 重复已消除，新增生产代码克隆门禁恢复为零违规。
- 严格 Clippy 收尾完成：删除已无调用的 Task Manager/Task Board/Tool Registry 退役代码与无用重导出，将 Local Task Board 内嵌测试迁入 `local_runtime/api/task_board/tests.rs`，解决 `items-after-test-module` 阻断。`cargo +1.94.0 clippy --workspace --all-targets -- -D warnings` 全绿；Task Manager 4 项、Local Task Board 2 项与 MCP 140 项专项测试全部通过。
- 最新全局质量快照：`rerun_support.rs` 已由 501 行降至 499 行，新文件体积警告清零。新增代码克隆、生产 `unwrap`/`expect`、请求路径 panic、热点行数预算、Rust 依赖漂移、严格全仓 Clippy 和 `git diff --check` 均通过。源码大小门禁只剩明确禁止改写的 `sandbox_manager_service/backend/src/service/images.rs`（1,283 行，现有预算 803 行）一项阻断；本轮未修改该文件、未扩大预算、未增加 allowlist 或 waiver。

本轮最新复验命令：

```bash
python3 scripts/check_source_size_policy.py
python3 scripts/check_new_code_clones.py --min-lines 25
python3 scripts/check-non-test-unwrap-expect.py
bash scripts/check-request-path-panics.sh
bash scripts/check-hotspot-line-budgets.sh
python3 scripts/check-rust-dependency-drift.py
cargo +1.94.0 fmt --all -- --check
cargo +1.94.0 clippy --workspace --all-targets -- -D warnings
cargo +1.94.0 test -p chat_app_server_rs --lib services::task_manager::tests:: -- --nocapture
cargo +1.94.0 test -p local_connector_client_core --lib local_runtime::api::task_board::tests:: -- --nocapture
cargo +1.94.0 test -p chatos_mcp --lib -- --nocapture
(cd chatos/frontend && npm run type-check)
(cd chatos/frontend && npm test -- --run src/components/projectExplorer/projectPlanPane/RequirementExecutionProcessModal.test.ts src/components/projectExplorer/projectPlanPane/RequirementExecutionProcessModal.pause.test.tsx src/components/projectExplorer/projectPlanPane/RequirementExecutionProcessModal.retryFailedTasks.test.tsx src/components/projectExplorer/projectPlanPane/RequirementExecutionProcessModal.regenerate.test.tsx src/components/projectExplorer/projectPlanPane/RequirementExecutionProcessModal.fullscreen.test.tsx)
git diff --check
```
