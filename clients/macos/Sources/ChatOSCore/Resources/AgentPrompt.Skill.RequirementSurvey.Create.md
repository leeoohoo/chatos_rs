# 场景 Skill：创建需求调研

## 目标与完成标准

把一个确实会影响后续工作的 Human 决策，整理成一张不重复、可直接选择、状态为 `pending` 的调研单。创建成功即结束本场景；不要等待或代替 Human 提交。

## 执行顺序

### 1. 提取决策缺口

从当前任务合同中写清：已经确认的事实、尚未确认的决定、该决定会影响什么。若缺失信息不会改变范围、方案、风险、时间或验收，则退出，不创建调研。

### 2. 查重

先调用：

```json
{"name":"requirement_survey_list","arguments":{"status":"pending"}}
```

对标题、purpose 或边界可能相同的候选逐一调用：

```json
{"name":"requirement_survey_get","arguments":{"survey_id":"<来自本轮 list>"}}
```

分支：

- 已有同主题 pending 调研：返回已有调研事实并退出；
- 历史调研已经给出仍然适用的决定：复用该决定并退出；
- 没有重复且决策确实缺失：继续设计问题。

### 3. 设计问题和选项

- 一张单只处理一个主题；
- 1–12 个问题，每个问题只问一个维度；
- `single_choice` 用于互斥决定，`multiple_choice` 用于可组合决定；
- 每题 2–12 个具体、平行、可执行的选项；
- 不创建自由文本题，页面会统一提供“备注”；
- question key、option key 和 request_key 使用稳定、可读的语义名称。

### 4. 创建

示例：

```json
{
  "name": "requirement_survey_create",
  "arguments": {
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
}
```

### 5. 验证并退出

检查返回值中的 `project_bound=true`、状态为 `pending`，题目和选项与提交内容一致。调用结果不明确时，使用相同 request_key 和完全相同的内容重试；不要生成新的 request_key。

最终报告：调研主题、需要 Human 决定的内容、当前状态。此后结束任务，不轮询 Human 答案。
