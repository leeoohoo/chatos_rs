# 需求方案版本、确认与任务绑定

日期：2026-10-13。实现范围为 shared Rust Host 与 macOS；Windows 使用下述同一协议与存储，不另建方案或 prompt 系统。

## 用户流程

调研提交答案后原 Run 继续生成方案。方案显示为“待确认”的明确版本，生成成功后用户可确认。**确认仅认可正文，不是实施授权，不创建 Task 或新 Run。**后续用户授权实施时，由正常的 8 个 Task 工具编排工作。

修改方案使用新的生成 Run 和新 revision，必须重新确认。已创建 Task 冻结原版本，重试仍用原版本，不偷偷替换成新方案。同文件/共享写入范围可能冲突时，Skill 要求依赖串行或单一 writer，不盲目并行。

## shared Host 契约（Windows 对齐）

Protocol 保持 42，增加命令和结果；旧 Host 不支持这些新增命令，应明确提示需升级，不退化成聊天记忆绑定。

| 命令 | 字段（除 type） | 作用 |
| --- | --- | --- |
| `get_requirement_plan` | `owner_user_id`, `survey_id`, 可选 `revision` | 读取最新或准确版本；只读，无记录返回 `plan:null` |
| `save_requirement_plan` | `owner_user_id`, `survey_id`, `source_run_id`, `expected_revision`, `markdown` | 新建 v1 使用 expected=0；其后乐观锁保存新版本 |
| `confirm_requirement_plan` | `owner_user_id`, `survey_id`, `expected_revision` | 仅 human UI 发送，生成成功且版本仍最新才可确认 |

统一响应：`{"type":"requirement_plan","plan":...}`。实体字段为 `plan_id`, `revision`, `owner_user_id`, `project_resource_id`, `source_conversation_id`, `source_survey_id`, `source_run_id`, `title`, `markdown`, `created_at_unix_ms`, `confirmed_at_unix_ms`。plan ID 为 `requirement-plan:<survey_id>`，正文上限 64 KiB。正文和来源按版本不变，确认时间另写；幂等 receipt 重放保持原结果。

DB schema 32 增加 `local_requirement_plans` 和项目索引，沿用现有迁移与迁移前备份。需升级 shared Host 后再启用 native UI，不手工修改用户数据库。owner 隔离，来源必须是已提交调研的原 Task 或同项目的真实 Main Chat Turn/Run。已经结束的 Run 只能导入准确 final reply；改稿使用新 Run，不能伪造旧回复。

模型 companion 工具为 `requirement_survey_create` 与 `requirement_plan_save`，**不属于原 8 个 Task 工具，不提供 confirm 工具**。save 的模型参数只允许 survey_id、expected_revision、markdown；owner、项目、sourceRun 均由程序绑定。激活 `chatos-requirement-discovery` 及其 catalog/tool 门控，不能凭工具存在扩大授权。

## 程序绑定，不依赖历史长度

当前实现把**项目最新方案作为该项目后续工作的 baseline**。启动新项目 Main Chat Turn 时，同事务将完整最新方案放入 `run.input.requirement_plan`，每个模型 step 单独投递方案上下文，独立于聊天历史裁剪。新增方案本身不启动工作。

Task graph 创建同事务再次校验：当前项目最新版本必须已确认，Main 绑定的 plan ID、revision、确认事实必须一致，否则拒绝并要求在新 Turn 查看当前方案。即使同项目的新工作与方案无关，当前也会应用该 baseline；暂未提供按业务范围选择/绕过 baseline 的机制。

Task 冻结结构：

```json
{
  "requirement_plan": {
    "snapshot": "完整 LocalRequirementPlan 实体，不是字符串占位",
    "markdown_sha256": "正文 UTF-8 的 SHA-256",
    "task_scope": {
      "title": "该任务标题",
      "prompt": "任务说明",
      "expected_outputs": ["预期产物"],
      "acceptance_criteria": ["验收标准"]
    }
  }
}
```

模型不能自行供给权威 snapshot/confirmed_at；程序覆盖或拒绝伪造，输入仍受 256 KiB 上限。适用范围是 Task 契约的冻结副本，**不是已实现逐章节语义匹配或自动变更影响分析**。Task 详情展示自身冻结正文，不读取项目最新方案替换旧 Task。

## native UI 与兼容

Windows 需求调研详情应显示版本、Markdown、待确认/已确认；源 Run 处理中、失败、取消、需补充信息要分别呈现。确认按钮仅在生成成功后启用，发准确 revision；冲突后要求刷新并重新查看，不自动接受新正文。消费 `requirement_plan_saved`、`requirement_plan_confirmed` 及原/方案来源 Run 终态事件；先订阅后读取，沿用一致性刷新，不高频轮询。

旧问卷仅 final reply 时，读取不写入。界面显式“保存并确认此方案”先从准确成功 Run 导入 v1，再调用 human confirm；每一步可能失败，不能声称已确认。旧 Task 没有 snapshot 时兼容显示，不补入今天的新方案。取消/失败的生成方案不能确认。

统一 Skill 入口位于 main-chat/task-execution invariants、async-task-orchestration、requirement-discovery；后者按需读取 `references/proposal-binding.md`。native executor 与 UI 不另塞零散平台 prompt。

## 通用插件/MCP 修复边界

所有通过通用 Plugin registry 或 native external MCP executor 的工具都执行参数 preflight；已知失败向模型提供安全的错误码和实际详情。JSON-RPC error 为已知失败；通信断开且写副作用未知仍需 review，不自动重试。

macOS 使用有界、离线 JSON Schema 子集（local refs、类型、必填、对象/数组、常见约束及组合），不是完整 JSON Schema 标准。已知显示注解不影响校验；不支持的断言/远程 ref 明确拒绝执行，不能谎称参数校验通过。修复共享链路不等于每个第三方插件业务逻辑均已验收，浏览器源码的额外 navigation 校验也需随该插件版本发布才生效。
