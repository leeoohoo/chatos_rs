# MCP 工具整合未完成工作项

更新日期：2026-07-08

## 当前状态

- 本地已提交：`f0184001 refactor: unify MCP tooling and local connector modules`。
- 当前分支：`2.0.1`，本地领先 `origin/2.0.1` 1 个提交。
- 远端推送未完成：GitHub HTTPS 连接持续 reset / 443 端口连接失败。
- 工作树仍有一个既有本地修改：`deploy_remote_prod.sh`。该文件未纳入 MCP 重构提交，后续也不要误提交，除非明确要处理它。
- 代码层旧协议扫描目前只剩内部函数名 `handle_mcp_request` 命中，没有旧 route、旧 relay message type、旧 sandbox tool alias 回退。

## P0：发布前必须完成

1. 重新推送本地提交到远端。
   - 命令：`git push origin 2.0.1`
   - 当前阻塞：本机到 `https://github.com/leeoohoo/chatos_rs.git` 的 HTTPS 连接失败。
   - 推送成功后确认：`git status -sb` 不再显示 `[ahead 1]`。

2. 网络恢复后重跑 `local_connector_service_backend` 编译。
   - 命令：`cargo check -p local_connector_service_backend`
   - 原因：之前停在依赖下载阶段，尚未形成最终验证结果。
   - 重点确认：云端 service 的 MCP relay outbound/inbound 统一 `type: "mcp"` 后没有编译或类型回归。

3. 补齐发布前全量验证。
   - `cargo fmt --all`
   - `cargo test -p chatos_mcp_service`
   - `cargo test -p chatos_sandbox_mcp_server`
   - `cargo check -p sandbox_manager_service_backend`
   - `cargo test -p sandbox_manager_service_backend mcp`
   - `cargo check -p local_connector_client_core`
   - `cargo test -p local_connector_client_core --no-run`
   - `python -m py_compile sandbox_manager_service/sandbox_agent/compat_agent.py`
   - `npm run type-check`

4. Windows 策略放行后重跑客户端过滤测试。
   - 之前 `cargo test --no-run` 已通过，但运行 test exe 被 Windows 应用控制策略阻止。
   - 待策略允许后重跑：`cargo test -p local_connector_client_core local_mcp`。
   - 如 terminal 相关行为继续调整，也重跑：`cargo test -p local_connector_client_core local_terminal`。

## P1：协议与迁移收尾

1. 补 Sandbox MCP server 端到端 JSON-RPC handler 测试。
   - 覆盖 POST `/mcp`。
   - 覆盖 `initialize`、`ping`、`tools/list`、`tools/call`。
   - 覆盖 bearer token / sandbox token 缺失、错误、成功路径。
   - 确认错误响应保持 JSON-RPC envelope，而不是旧 REST error shape。

2. 审计剩余历史文档和配置样例。
   - 目标：删除或迁移旧 `/mcp/tools`、`/mcp/call`、`mcp_request`、`mcp_response`、`sandbox_filesystem_*`、`sandbox_terminal_*` 描述。
   - 代码扫描已基本干净，但历史方案和外部接入说明仍可能有旧 façade 文案。

3. 制定旧 Task Runner 配置迁移策略。
   - 旧工具名 alias 已删除。
   - 如果已有任务配置里保存了 `sandbox_filesystem_*` / `sandbox_terminal_*`，需要一次性迁移到 `tools/list` 返回的新工具名。

4. 明确 breaking relay 发布策略。
   - Local Connector relay 已从 `mcp_request` / `mcp_response` 改为统一 `type: "mcp"`。
   - 云端 service 和本地 client 需要同批发布，或补版本握手 / 最低版本拦截，避免新旧端混跑。

## P2：继续拆分和抽象

1. 抽 `chatos_mcp_service` 的 policy / 权限模块。
   - 目标：减少各 host 自己维护 read/write、builtin kind、工具归属集合。
   - 候选内容：工具权限分类、工具来源标记、catalog filtering、host capability policy。

2. 继续压缩 Local Connector client 剩余大文件。
   - `local_connector_client/core/src/terminal/exec/runner.rs`：可继续拆 request/response/history helper。
   - `local_connector_client/core/src/sandbox/images/job.rs`：可拆 Docker build process、log capture、job finalize。
   - `local_connector_client/core/src/terminal/controller.rs`：可拆 shell session start/cleanup 编排。
   - `local_connector_client/core/src/sandbox/catalog.rs`：可拆 runtime spec 数据和 lookup/selection helper。
   - `local_connector_client/core/src/registration.rs`、`connector.rs`：后续可按 HTTP 注册、websocket loop、状态同步继续拆。

3. 拆分测试文件。
   - 当前 `local_connector_client/core/src/tests.rs` 仍是最大文件。
   - 建议按 local_mcp、terminal、sandbox、workspace/history 分组拆，避免单个测试文件继续膨胀。

4. 梳理 Local Connector 内置 MCP 工具和 Local Connector service 工具 catalog 的统一边界。
   - 当前已经共享 JSON-RPC service/provider/catalog 基础。
   - 后续还可以把工具能力描述、权限、schema metadata、host-specific extension 的职责继续收拢。

## P3：产品化和运维收尾

1. 补 breaking change 迁移说明。
   - 给调用方明确：旧工具名、旧 route、旧 relay message type 全部失效。
   - 给出新调用方式：POST `/mcp` + JSON-RPC `tools/list` / `tools/call`，relay 使用 `type: "mcp"`。

2. 补端到端 smoke checklist。
   - Sandbox Manager 前端 MCP 测试页调用 `/api/sandboxes/:sandbox_id/mcp`。
   - 云端 backend proxy 到 sandbox agent `/mcp`。
   - Local Connector 本地 sandbox proxy 到 agent `/mcp`。
   - Local Connector 内置 MCP 工具通过共享 dispatch 正常 list/call。

3. 检查部署脚本和发布流程。
   - `deploy_remote_prod.sh` 当前是未提交本地修改，尚未判断是否与本轮相关。
   - 如果要发布本轮 breaking 变更，需要单独审查部署脚本，不要混入 MCP 重构提交。

## 当前风险

1. 推送失败导致远端尚未包含本地提交，团队其他环境拿不到这批重构。
2. `local_connector_service_backend` 尚未完成网络恢复后的最终编译验证。
3. 删除旧 alias 后，外部保存的旧工具名配置会直接失效。
4. relay envelope breaking 后，新旧 local client / cloud service 混跑会失败。
5. Windows 应用控制策略阻止测试 exe 运行，当前只能确认 `--no-run`，还需要补实际运行结果。
