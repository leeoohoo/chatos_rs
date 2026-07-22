# ChatOS 代码质量治理实施文档

## 1. 目标与完成标准

本次治理覆盖整个仓库，目标是处理超大源文件、可维护性较差的重复代码以及会影响稳定性、可诊断性或安全性的重大缺陷。所有修改必须保持现有业务行为，并通过模块测试与仓库质量门禁验证。

完成标准：

1. 生产源文件体积门禁无硬违规：当前 11 个超过 800 行的文件全部完成职责拆分，不新增永久白名单。
2. `chatos_3d_anime_prototype/src/styles.css` 按样式域拆分，主入口不再承载 5000 余行样式。
3. 新增代码精确重复门禁清零，且全仓 jscpd 重复率不高于本次基线 1.64%。
4. 非测试 Rust 代码中的本次 5 处 `expect` 全部改为显式错误处理或可靠的静态初始化。
5. Rust 依赖版本基线与实际工作区保持一致，检查脚本和说明文档同步。
6. 既有热点预算全部达标。
7. 相关 Rust 测试、Clippy、前端测试、类型检查、构建及代码质量门禁全部通过。
8. 本文档中的实施项全部标记为完成，并记录最终验证结果。

## 2. 审计基线与实施结果（2026-07-20）

### 2.1 超大生产源文件

| 状态 | 原始行数 | 当前主文件行数 | 文件 |
| --- | ---: | ---: | --- |
| 已完成 | 1777 | 463 | `chatos_3d_anime_prototype/src/App.tsx` |
| 已完成 | 1679 | 767 | `sandbox_manager_service/backend/src/backend/docker.rs` |
| 已完成 | 1536 | 800 | `chatos_3d_anime_prototype/src/useChatOSBridge.ts` |
| 已完成 | 1231 | 148 | `chatos_3d_anime_prototype/src/scene/AnimeRoom.tsx` |
| 已完成 | 1217 | 706 | `local_connector_client/core/src/model_configs/service.rs` |
| 已完成 | 1153 | 508 | `local_connector_client/core/src/sandbox/compose.rs` |
| 已完成 | 926 | 596 | `sandbox_manager_service/backend/src/service/manager/environments.rs` |
| 已完成 | 842 | 684 | `chatos/backend/src/api/projects/requirement_execution_handlers.rs` |
| 已完成 | 830 | 743 | `task_runner_service/backend/src/services/sandbox_runtime/manager_client.rs` |
| 已完成 | 829 | 578 | `project_management_service/backend/src/services/runtime_environment.rs` |
| 已完成 | 828 | 622 | `chatos/backend/src/api/sessions/history_process_support.rs` |

额外样式热点 `chatos_3d_anime_prototype/src/styles.css` 已从 5104 行缩减为 8 行导入入口，样式按职责拆为 8 个模块，单文件最大 661 行。

### 2.2 计划热点预算

| 状态 | 原始行数 | 完成行数 | 目标 | 文件 |
| --- | ---: | ---: | ---: | --- |
| 已完成 | 416 | 365 | 412 | `chatos/frontend/src/components/chatInterface/useChatStreamRealtimeBridge.ts` |
| 已完成 | 842 | 684 | 700 | `chatos/backend/src/api/projects/requirement_execution_handlers.rs` |
| 已完成 | 503 | 486 | 500 | `chatos/frontend/src/components/projectExplorer/ProjectPlanPane.tsx` |
| 已完成 | 507 | 497 | 500 | `chatos/frontend/src/components/projectExplorer/ProjectRunSettingsPanel.tsx` |

### 2.3 新增精确重复代码

| 状态 | 重复位置 |
| --- | --- |
| 已完成 | 终端进程树终止逻辑已提取到根目录 `mcp/` 共享实现 |
| 已完成 | 删除旧 `chatos/frontend/.eslintrc.cjs`，统一使用 ESLint 10 flat config |
| 已完成 | Task Runner 排队记录改用统一构造器 |
| 已完成 | 多任务创建复用单任务参数解析 |

jscpd 全仓基线：2217 个文件、423372 行、296 个克隆、6941 个重复行，重复率 1.64%。历史重复不做无差别机械重写；优先治理协议、安全、命令执行和状态机相关重复。

### 2.4 明确缺陷

| 状态 | 问题 |
| --- | --- |
| 已完成 | `requirement_execution/context.rs` 的 3 处 `expect` 已改为显式内部错误传播 |
| 已完成 | `task_runner_callback_display.rs` 的正则使用可失败静态缓存，并补充回调脱敏测试 |
| 已完成 | Rust 依赖基线已补充 `config_center_service/backend/Cargo.toml` 和 `crates/chatos_service_runtime/Cargo.toml` |
| 已完成 | SSH/SCP 公共参数、Ask User 字段归一化、Docker/Kata 容器运行参数已提取为共享实现，避免安全与行为规则漂移 |
| 已完成 | 需求执行鉴权、运行配置和计划加载已提取为共享上下文，减少多个入口之间的状态分歧 |
| 已完成 | Clippy `-D warnings` 暴露的路径校验、条件表达式、排序和默认值处理问题已全部修复 |

## 3. 实施顺序

### 阶段 A：质量门禁与稳定性缺陷

- [x] 移除 5 处生产代码 `expect`。
- [x] 补齐 Rust 依赖版本基线，并同步中文说明。
- [x] 消除 4 组新增精确重复。
- [x] 复核命令执行、任务状态转换和回调展示相关历史重复：终端进程树、SSH/SCP 参数、Ask User 字段归一化已提取共享实现；Task Runner 最大的状态重复来自测试夹具，不改变生产状态机；需求执行入口重复纳入阶段 B 拆分。

### 阶段 B：后端和客户端核心大文件拆分

- [x] 拆分 `docker.rs`。
- [x] 拆分 `model_configs/service.rs`。
- [x] 拆分 `sandbox/compose.rs`。
- [x] 拆分 `environments.rs`。
- [x] 拆分 `requirement_execution_handlers.rs`。
- [x] 拆分 `sandbox_runtime/manager_client.rs`。
- [x] 拆分 `runtime_environment.rs`。
- [x] 拆分 `history_process_support.rs`。

### 阶段 C：3D 原型与前端热点拆分

- [x] 拆分 `App.tsx`。
- [x] 拆分 `useChatOSBridge.ts`。
- [x] 拆分 `AnimeRoom.tsx`。
- [x] 拆分 `styles.css`。
- [x] 使其余 3 个前端计划热点达到预算。

### 阶段 D：全量验证与收尾

- [x] 运行代码体积、热点预算、重复代码、生产 panic 和依赖漂移门禁。
- [x] 运行受影响模块的 Rust 测试与 Clippy。
- [x] 运行受影响前端的测试、类型检查和构建。
- [x] 重新运行 jscpd 并与 1.64% 基线比较。
- [x] 更新本文档的所有状态和最终验证记录。

## 4. 验收命令

```bash
python3 scripts/check_source_size_policy.py
bash scripts/check-hotspot-line-budgets.sh
python3 scripts/check_new_code_clones.py --min-lines 25
python3 scripts/check-non-test-unwrap-expect.py
python3 scripts/check-rust-dependency-drift.py
python3 -m unittest discover -s scripts/tests -p 'test_code_quality_*.py'
```

各模块还需运行其现有测试、Clippy、前端类型检查和构建命令。若全仓验证受外部服务或既有非本次问题阻塞，必须在最终记录中给出可复现命令、实际输出和影响范围，不能以未验证代替完成。

## 5. 最终验证记录

### 5.1 质量门禁

- 生产源码体积：扫描 2935 个文件，硬违规 0；仅保留 2 个低于 800 行硬上限的预警文件。
- 热点预算：默认预算与 `--warn-planned` 计划预算全部通过。
- 新增精确重复：检查 238 个变更文件，25 行阈值下违规 0。
- 非测试 Rust `unwrap/expect`：0 处。
- Rust 依赖漂移：0 处。
- 质量门禁脚本单元测试：5/5 通过。
- Clippy：ChatOS backend、Task Runner、Local Connector、Sandbox Manager、Project Management 的库目标在 `-D warnings` 下通过。

### 5.2 重复代码复测

使用 JavaScript、TypeScript、Rust 生产代码口径重新运行 jscpd：

- 扫描 2198 个源码文件、421942 行代码。
- 发现 89 个克隆、3150 个重复行。
- 重复率为 0.7465%，低于治理前 1.6395% 基线。
- 本次变更新增克隆为 0。

### 5.3 Rust 回归测试

- ChatOS backend：492 项通过。
- Task Runner：228 项通过。
- Local Connector：237 项通过，2 项依赖外部运行环境的测试按设计忽略。
- Project Management：77 项通过，11 项依赖 MongoDB 的集成测试按设计忽略。
- Sandbox Manager：29 项通过，1 项需要 Docker 及本地 `chatos-sandbox-agent:latest` 镜像的测试按设计忽略。
- ChatOS Builtin Tools：84 项通过。

### 5.4 前端与原型验证

- ChatOS frontend：ESLint 通过、TypeScript 类型检查通过、Vitest 111 个文件共 383 项测试通过、生产构建通过。
- 3D 原型：TypeScript 类型检查通过、生产构建通过。

### 5.5 结论

本次识别的超大生产文件、新增重复、生产 panic 风险、依赖基线缺口和高风险历史重复均已完成治理。所有原始超过 800 行的生产源文件均已降至门禁范围内，全部计划热点达到预算，最终质量门禁和相关回归验证通过。
