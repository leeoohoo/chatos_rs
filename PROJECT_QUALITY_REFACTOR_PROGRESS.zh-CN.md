# 项目质量重构进度表

> 关联计划：`PROJECT_QUALITY_REFACTOR_PLAN.zh-CN.md`
> 更新日期：2026-07-09

| 顺序 | 计划项 | 状态 | 本次处理 | 验证 |
| --- | --- | --- | --- | --- |
| 0 | Phase 0：治理脚本基线 | 已完成 | 新增 `.gitattributes` 固定文本换行为 LF；修复 `scripts/check-non-test-unwrap-expect.py` 显式 UTF-8 读取；规整治理脚本工作区换行；扩展 `scripts/check-large-files.sh` 对多平台 `bundled-tools/*/rg` 的 allowlist。 | `bash scripts/code-size-report.sh --top 50`；`bash scripts/check-hotspot-line-budgets.sh --warn-planned`；`bash scripts/check-large-files.sh --threshold 1 --fail`；`python scripts/check-non-test-unwrap-expect.py`；`git diff --check` |
| 1 | Phase 1.1：HTTP response body 限流下沉 | 已完成 | 在 `crates/chatos_service_runtime/src/http_body.rs` 增加共享流式 body 限流读取 helper；Task Runner 和 Project Management 的本地 `http_body` 模块改为薄 re-export；`chatos_ai_runtime` 复用 preview body helper；保留普通 limit 与 preview limit 两类错误文案。 | `cargo test -p chatos_service_runtime --locked --ignore-rust-version`；`cargo check -p chatos_ai_runtime --locked --ignore-rust-version`；`cargo check -p task_runner_service_backend --locked --ignore-rust-version`；`cargo check -p project_management_service_backend --locked --ignore-rust-version` |
| 2 | Phase 1.2：Frontend Vite 共享 helper | 已完成 | 新增 `scripts/frontend/viteShared.ts`，统一 `parsePort`、`normalizeBasePath`、`basePrefixFromBase` 和 base path proxy rewrite；`user_service`、`task_runner_service`、`project_management_service`、`memory_engine` 四个前端 Vite 配置改为复用共享 helper，同时保留各自 build/test/proxy 目标配置。 | `npm run build` in `user_service/frontend`；`npm run build` in `task_runner_service/frontend`；`npm run build` in `project_management_service/frontend`；`npm run build` in `memory_engine/frontend` |
| 3 | Phase 1.3：Remote connection payload 类型统一 | 已完成 | 将 remote connection create/update/test payload 的单一类型源收敛到 `chatos/frontend/src/lib/api/client/types/remoteConnection.ts`；workspace API client、store action 和 remote execution slice 删除重复 interface，改为 import 或兼容 alias。 | `rg -n "interface RemoteConnectionPayload|interface RemoteConnectionMutationPayload|interface RemoteConnectionCreatePayload|interface CreateRemoteConnectionPayload|interface UpdateRemoteConnectionPayload|Omit<RemoteConnectionPayload" chatos/frontend/src -g "*.ts" -g "*.tsx"`；`npm run build` in `chatos/frontend`；`git diff --check` |
| 4 | Phase 1.4：Code nav 文本搜索共用循环 | 已完成 | 在 `chatos/backend/src/services/code_nav/languages/shared_nav.rs` 新增 `search_text_occurrences`、`TextSearchLine` 和 `TextSearchMatchParts`；Rust、Basic、Java、Python、Go 的 occurrence search 删除重复 regex/WalkDir/budget/file-read/preview 循环，语言层只保留文件过滤、行预处理和 match 构造；Go 保留剥注释后匹配、原始行预览的既有行为。 | `cargo check -p chat_app_server_rs --locked --ignore-rust-version`；`rg -n "RegexBuilder|read_code_nav_file_to_string, truncate_preview|fn should_visit_path|fn should_visit_rust_path|for found in regex.find_iter" chatos/backend/src/services/code_nav/languages -g "*.rs"`；`git diff --check` |
| 5 | Phase 2：Terminal controller response contract | 待处理 | - | - |
| 6 | Phase 3：后端大文件拆分 | 待处理 | - | - |
| 7 | Phase 4：拆分 `local_connector_client/frontend/src/main.tsx` | 待处理 | - | - |
| 8 | Phase 5：整理 `memory_engine_sdk` | 待处理 | - | - |
| 9 | Phase 6：Workspace 和依赖版本治理 | 待处理 | - | - |
