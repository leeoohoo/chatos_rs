# 项目质量重构进度表

> 关联计划：`PROJECT_QUALITY_REFACTOR_PLAN.zh-CN.md`
> 更新日期：2026-07-09

| 顺序 | 计划项 | 状态 | 本次处理 | 验证 |
| --- | --- | --- | --- | --- |
| 0 | Phase 0：治理脚本基线 | 已完成 | 新增 `.gitattributes` 固定文本换行为 LF；修复 `scripts/check-non-test-unwrap-expect.py` 显式 UTF-8 读取；规整治理脚本工作区换行；扩展 `scripts/check-large-files.sh` 对多平台 `bundled-tools/*/rg` 的 allowlist。 | `bash scripts/code-size-report.sh --top 50`；`bash scripts/check-hotspot-line-budgets.sh --warn-planned`；`bash scripts/check-large-files.sh --threshold 1 --fail`；`python scripts/check-non-test-unwrap-expect.py`；`git diff --check` |
| 1 | Phase 1.1：HTTP response body 限流下沉 | 待处理 | - | - |
| 2 | Phase 1.2：Frontend Vite 共享 helper | 待处理 | - | - |
| 3 | Phase 1.3：Remote connection payload 类型统一 | 待处理 | - | - |
| 4 | Phase 1.4：Code nav 文本搜索共用循环 | 待处理 | - | - |
| 5 | Phase 2：Terminal controller response contract | 待处理 | - | - |
| 6 | Phase 3：后端大文件拆分 | 待处理 | - | - |
| 7 | Phase 4：拆分 `local_connector_client/frontend/src/main.tsx` | 待处理 | - | - |
| 8 | Phase 5：整理 `memory_engine_sdk` | 待处理 | - | - |
| 9 | Phase 6：Workspace 和依赖版本治理 | 待处理 | - | - |
