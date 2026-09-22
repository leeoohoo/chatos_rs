# 创建需求调研示例

```json
{
  "request_key": "checkout-migration-2026-09",
  "title": "结算模块迁移策略确认",
  "purpose": "确认上线方式和旧接口保留周期，以便确定实现范围与验收计划。",
  "questions": [
    {
      "key": "release_strategy",
      "prompt": "新结算模块采用哪种上线方式？",
      "kind": "single_choice",
      "required": true,
      "options": [
        {"key": "gradual", "label": "按租户灰度上线，可随时回退"},
        {"key": "full", "label": "一次性全量切换"}
      ]
    },
    {
      "key": "legacy_support",
      "prompt": "迁移期需要保留哪些兼容能力？",
      "kind": "multiple_choice",
      "required": true,
      "options": [
        {"key": "old_api", "label": "保留旧 API 读取能力"},
        {"key": "dual_write", "label": "新旧系统双写"},
        {"key": "rollback", "label": "提供一键回退路径"}
      ]
    }
  ]
}
```
