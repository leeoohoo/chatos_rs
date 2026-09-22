# 场景 Skill：读取调研结果

## 目标与完成标准

从当前项目的真实调研记录中，准确区分 Human 选择、Human 备注、既有解决方案和仍未确认事项，并给出有来源的摘要。

## 执行顺序

### 1. 定位候选

处理刚提交的调研：

```json
{"name":"requirement_survey_list","arguments":{"status":"submitted"}}
```

回顾历史决定时可不传 status：

```json
{"name":"requirement_survey_list","arguments":{}}
```

根据标题、purpose 和当前任务边界选择候选，不凭标题直接下结论。

### 2. 读取完整记录

```json
{"name":"requirement_survey_get","arguments":{"survey_id":"<来自本轮 list>"}}
```

逐项读取：

1. `status`；
2. 每个问题的 prompt、options、selected 和 selected_option_keys；
3. 独立读取 `notes`，确认它是否补充、限制或纠正选项；
4. 读取 resolution 的 summary、solution_markdown、execution_steps、风险和相关资料。

### 3. 按状态处理

- `pending`：只报告问题仍待 Human 提交，不推断答案；
- `submitted` 且 resolution 为空：报告 Human 决策已经具备、正式方案尚未形成；
- resolution 非空：分别摘要 Human 决策和 Agent 方案，不把二者混写成同一来源。

### 4. 输出与验证

输出固定分为：

- Human 已确认；
- Human 备注补充；
- 既有解决方案与执行步骤；
- 尚未确认或存在冲突；
- 使用的调研标题与状态。

示例结论：

```text
Human 已确认采用按租户灰度上线，并要求保留旧 API 读取和一键回退。
备注补充：首批只开放内部租户，因此灰度名单需要可配置。
当前调研已 submitted，但 resolution 为空，尚未形成正式技术方案。
```

survey_id 不存在时重新 list。聊天摘要和 Agent Memory 只能帮助定位，不能替代调研原始记录。
