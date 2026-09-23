# 解决方案写入示例

```json
{
  "survey_id": "<来自本轮 requirement_survey_get>",
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
    }
  ],
  "risks_and_open_questions": "灰度名单配置错误可能扩大影响范围，需要变更审计与默认拒绝策略。",
  "related_materials": "关联当前项目的结算迁移设计与测试记录。"
}
```
