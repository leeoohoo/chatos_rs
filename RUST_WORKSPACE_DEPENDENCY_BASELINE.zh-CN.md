# Rust Workspace 与依赖版本基线

> 更新日期：2026-07-13
> 配套检查：`python scripts/check-rust-dependency-drift.py`

## 当前策略

当前仓库暂时采用“根 workspace + 少量独立服务 workspace”的混合策略。

- 根 `Cargo.toml` 覆盖大多数后端服务和共享 crate。
- `memory_engine/backend` 继续排除在根 workspace 外，因为它已经使用 `mongodb = "3"`，而多数业务服务仍在 `mongodb = "2.8"`；直接纳入根 workspace 会把 MongoDB 3 迁移和 workspace 治理混在一起。
- `user_service/backend` 暂时保留独立 `[workspace]`，避免在未做依赖升级验证前改变它的锁文件和构建边界。
- `memory_engine_sdk` 已统一为 `crates/memory_engine_sdk` 下的唯一 Rust package，不再保留 `memory_engine/sdk` 独立 crate。

这表示当前版本差异是有意冻结的基线，不允许无计划新增第三套版本或悄悄改动单个服务版本。

## 当前依赖基线

| Manifest | axum | tower-http | mongodb |
| --- | --- | --- | --- |
| `chatos/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |
| `crates/chatos_ai_runtime/Cargo.toml` | `0.8` | - | - |
| `local_connector_client/core/Cargo.toml` | `0.8` | `0.7` | - |
| `local_connector_service/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |
| `memory_engine/backend/Cargo.toml` | `0.8` | `0.7` | `3` |
| `official_website_service/backend/Cargo.toml` | `0.8` | `0.7` | - |
| `plugin_management_service/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |
| `project_management_service/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |
| `sandbox_manager_service/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |
| `sandbox_manager_service/sandbox_mcp_server/Cargo.toml` | `0.8` | - | - |
| `task_runner_service/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |
| `user_service/backend/Cargo.toml` | `0.8` | `0.7` | `2.8` |

## 检查方式

本地运行：

```bash
python scripts/check-rust-dependency-drift.py
```

检查脚本会扫描仓库内生产代码的 `Cargo.toml`（排除 `docs` 示例），只要发现 `axum`、`tower-http` 或 `mongodb` 的版本和上表不一致，或者新增 manifest 使用这些依赖但没有登记基线，就会失败。

## 升级路径

1. `axum` 已统一到 `0.8`，`tower-http` 已统一到 `0.7`；新增服务应复用该版本线。
2. `memory_engine/backend` 作为 `mongodb = "3"` 的参考实现，先整理 MongoDB 3 API 差异，再迁移仍在 `mongodb = "2.8"` 的服务。
3. 所有服务完成 MongoDB 3 迁移后，再考虑将 `memory_engine/backend` 纳入根 workspace。
4. `user_service/backend` 依赖版本和锁文件稳定后，再决定是否移除独立 `[workspace]` 并纳入根 workspace。

任何有意升级都需要同步更新本文件和 `scripts/check-rust-dependency-drift.py` 中的 baseline。
