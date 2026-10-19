# Local Agent Run 执行效率专项（2026-10-19）

状态：已完成

## 目标与范围

本专项只处理真实会话中确认的四类长耗时问题：

1. Capability Provider 重建后重复读取同一工具 schema。
2. 100 KB 以上持久化结果只能按 6–8 KB 顺序分页，造成十几到几十轮读取。
3. 本地服务启动后才发现固定端口冲突，随后检查、停止、重启并改端口。
4. UI 把模型、工具、依赖、审批和用户输入等不同等待统一显示为“执行中”。

明确不在本次范围内：Plugin 超时、审批 Agent 策略、通用重试/熔断、通用迭代次数、任务拆分策略。

## 任务拆解与顺序

### T1 — Run 级 Capability 披露恢复

状态：已完成

- Host 从已经持久化的成功 `capability_describe` 调用中，恢复当前 owner/Run 已完整读取的 `plugin_option + tool_option`。
- Provider 创建时注入恢复结果，并用当前冻结 Release 重新生成的稳定 option 映射校验；只恢复仍然存在的选项。
- Provider 过期、重建或 App 重启后，`capability_invoke` 不再要求同一工具重新 describe。
- 不降低现有目录不可授权、错误结果不可授权、跨 owner/Run 不可授权的安全边界。

验收：同一 Run 的同一工具完整 schema 最多读取一次；Provider 过期和 App 重启后仍可调用。Release 或 schema 已变化时不复用旧披露。

### T2 — 大结果定向查询

状态：已完成

- 保留完整原始结果、字符数和 SHA-256，继续支持精确顺序分页。
- 新增只读 `tool_result_query`，支持 JSON Pointer、文本搜索、前后文、head/tail 和显式字符范围。
- 首次面对大结果时返回紧凑 manifest，告诉模型结果类型、大小、hash、可查询方式和建议入口。
- 所有读取继续受 owner、当前 Run / 已解析前置 Run 边界约束。

验收：100–200 KB 文本或 JSON 的典型有效内容可在 1–3 次定向读取内取得；原始 payload 和 SHA-256 不变；越权与无效范围被拒绝。

### T3 — Run 级本地服务端口分配

状态：已完成

- 抽取现有 loopback 临时端口选择逻辑为共享分配器。
- 服务启动前按 Run 和服务名登记，支持首选端口；冲突时自动获得可用端口。
- 启动结果返回规范 `service_url` / 实际端口，后续进程引用登记结果，不硬编码固定端口。
- 不停止或接管占用端口的无关进程。

验收：并发请求同一首选端口会得到不同可用端口；服务启动前即可知道实际端口；释放后登记清理。

### T4 — UI 明确等待状态

状态：已完成

- 从现有 Run、工具、审批、依赖和进程字段投影单一用户可读状态，不改变执行状态机。
- 至少区分：等待依赖、等待模型、等待工具、等待审批、等待用户、等待进程启动、读取大结果、恢复 Capability。
- 展示当前操作、等待原因、已耗时和可用细节（工具名、依赖任务、端口、结果页/查询）。
- 保留并兼容工作区里已有的任务图轮询优化，不覆盖其未提交改动。

验收：不再以统一“执行中”覆盖上述等待；状态映射有纯单元测试；运行页可观察到当前原因与耗时。

## 完成门槛

- 四项各自的聚焦测试通过，并执行受影响模块的回归测试。
- 计划中的状态和验证证据随实现同步更新。
- `git diff` 中不包含四项范围以外的行为改动，也不覆盖用户已有未提交修改。

## 实施结果

### T1 结果

- 完整的 `capability_describe` 响应持久化 `provider_id`、`plugin_option`、`tool_option` 与 `schema_sha256`。
- Provider 重建时按当前冻结 Release 重新生成 schema hash，四项一致才恢复披露；目录页、未读完的分页 schema、错误结果、跨 owner/Run 和 hash 不一致均不会恢复。
- 有效恢复后可直接执行 `capability_invoke`，同一 Run 无需再次读取相同 schema；关联 Skill 披露状态同步恢复。

### T2 结果

- 新增只读工具 `tool_result_query`，支持 `json_pointer`、`search`、`head`、`tail` 与 `range`，以及搜索上下文、匹配上限、offset 和大小写控制。
- 大结果 manifest 同时提供 `tool_result_query` 与原有 `tool_result_read`；默认建议先用 1–3 次定向查询，只有需要精确重建时才顺序分页。
- 每次返回保留原结果字符数和 SHA-256，并施加 12K 模型输出预算；读取仍限制在当前 owner、当前 Run 或已解析前置 Run。

### T3 结果

- 新增 Run 级工具 `task_run_process_allocate_service`，支持 `allocate` / `release`，并按 `run_id + service_name` 幂等登记。
- 在服务启动前检查首选 loopback 端口；端口已被外部进程或同一 Run 的其他服务占用时，自动选择其他可用端口，不停止或接管占用者。
- 返回实际 `port`、规范 `service_url`、`preferred_port_honored` 与 `CHATOS_SERVICE_HOST/PORT/URL`，执行提示要求后端和依赖进程使用分配结果。

### T4 结果

- 新增不改变执行状态机的纯状态投影，区分等待前置任务、等待/运行模型、等待工具、等待审批、等待用户、启动服务、提取大结果、读取/恢复 Capability、等待重试和暂停。
- 任务图加载活跃 Run，并为前置依赖补齐标题和状态；终态 Run 继续使用既有缓存，审批列表每次图刷新只读取一次。
- 节点卡片在原固定高度内替换状态与描述并显示计时；Inspector 展示当前阶段、操作、详情和耗时，可呈现分配后的 `service_url` 及结果查询参数。

## 验证证据

- `cargo test -p chatos_local_agent_host --lib`：101 passed。
- `cargo test -p chatos_local_agent_runtime --lib`：44 passed。
- `cargo test -p chatos_client_storage --lib`：113 passed。
- `NativeAgentCapabilityDisclosureTests`：6 passed。
- `NativeAgentPluginToolProviderTests`：7 passed。
- `NativeLocalAgentRunActivityProjectorTests`：2 passed。
- 覆盖 Provider/Host 重建后的披露恢复、错误 hash 拒绝、100 KB 以上文本的一次搜索、JSON Pointer 单次提取、完整分页 SHA-256、端口冲突/幂等/释放和 UI 状态投影。
- 最终范围审计：`git diff --check` 通过；本专项改动清单不包含 Plugin 超时、审批 Agent 策略、通用重试/熔断、通用迭代次数或其他任务拆分策略的行为修改。工作区内并发存在的其他改动不计入本专项，也未被覆盖或回退。
