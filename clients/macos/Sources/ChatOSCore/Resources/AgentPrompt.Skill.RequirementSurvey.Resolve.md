# 场景 Skill：生成解决方案与执行计划

## 目标与完成标准

把 Human 已提交的选择和备注转化为一份边界明确、可实施、可验收的解决方案及有序执行计划，并写回同一张调研记录。

## 执行顺序

### 1. 重新读取最新提交

```json
{"name":"requirement_survey_list","arguments":{"status":"submitted"}}
```

定位后立即读取完整记录：

```json
{"name":"requirement_survey_get","arguments":{"survey_id":"<来自本轮 list>"}}
```

只有 `status=submitted` 才继续。若为 pending，报告仍待 Human 提交并退出。

### 2. 建立决策映射

对每道题记录：Human 选择、备注补充、影响的范围、方案、风险和验收条件。备注与选项冲突时，不自行裁决；把冲突列为待确认事项并停止写入。

若 resolution 已存在，先比较是否出现新提交或新事实。内容没有变化时返回既有方案，不重复覆盖。

### 3. 组织解决方案

`summary`：写最终决定、适用边界和关键限制。

`solution_markdown` 至少覆盖：决策依据、采用方案、实施范围、非目标、关键设计、兼容或迁移策略、验证方式。

`execution_steps` 按依赖顺序编排，每步写清：目标、动作、负责人建议、交付物、验收条件。步骤是计划，不宣称已经执行。

### 4. 写回 resolution

示例：

```json
{
  "name": "requirement_survey_resolve",
  "arguments": {
    "survey_id": "<来自本轮 get>",
    "summary": "采用按租户灰度上线；迁移期保留旧 API 读取与一键回退，首批仅内部租户。",
    "solution_markdown": "## 决策依据\nHuman 选择灰度发布，并在备注中限定首批内部租户。\n\n## 实施范围\n新增可配置灰度名单、旧 API 只读适配和回退开关。\n\n## 非目标\n本阶段不实施全量切换。\n\n## 验证\n验证名单内外租户路由、旧 API 读取和回退流程。",
    "execution_steps": [
      {
        "key": "design-routing",
        "title": "设计灰度路由与回退状态机",
        "detail": "定义租户名单、路由优先级、回退触发条件和审计字段。",
        "owner": "架构负责人",
        "deliverable": "设计说明与状态转换表",
        "acceptance_criteria": "覆盖名单命中、未命中、回退和异常恢复路径"
      },
      {
        "key": "implement-and-verify",
        "title": "实现兼容层并完成验证",
        "detail": "实现灰度路由、旧 API 只读适配和回退开关，并执行集成测试。",
        "owner": "开发与测试负责人",
        "deliverable": "代码、测试及验证记录",
        "acceptance_criteria": "名单内外路由正确，旧 API 可读，回退演练通过"
      }
    ],
    "risks_and_open_questions": "灰度名单配置错误可能扩大影响范围，需要变更审计与默认拒绝策略。",
    "related_materials": "关联当前项目的结算迁移设计与测试记录。"
  }
}
```

### 5. 回读验证并退出

再次调用 `requirement_survey_get`，确认同一 survey_id 的 resolution 已保存，关键 Human 限制没有丢失，execution_steps 顺序完整。

最终报告方案已经形成以及后续应创建哪些执行任务。resolution 写入只代表规划完成，不代表实施完成。
