# 场景 Skill：核对方案执行进度

## 目标与完成标准

将一张调研的正式执行计划与当前项目团队任务事实进行对照，说明已完成、进行中、阻塞、未覆盖和无法确认的部分。

## 执行顺序

### 1. 定位并读取基准方案

```json
{"name":"requirement_survey_list","arguments":{}}
```

```json
{"name":"requirement_survey_get","arguments":{"survey_id":"<来自本轮 list>"}}
```

确认 resolution 存在，并提取 summary 与每个 execution_step 的目标、交付物和验收条件。没有 resolution 时，报告缺少可核对的正式计划并退出。

### 2. 读取项目任务事实

```json
{"name":"requirement_survey_project_tasks","arguments":{}}
```

读取各团队任务的 objective、scope、status、负责人、基础能力、blocked_reason 和 result。

### 3. 建立对应关系

按目标、范围、交付物和验收条件匹配 execution_step 与任务，不只按标题匹配。一个步骤可以对应多个任务；无法可靠匹配时标记“未确认”，不猜测。

状态判断：

- 只有任务状态为 completed 且 result 提供了与验收条件相关的结果，才标记步骤已完成；
- in_progress 表示执行中；
- blocked 必须带出 blocked_reason；
- 没有对应任务的步骤标记为未覆盖；
- resolution 存在本身不是任何步骤完成的证据。

### 4. 输出进度对照

示例：

```text
方案步骤 1「设计灰度路由与回退状态机」：已完成。
证据：任务“灰度状态机设计”状态 completed，结果包含状态转换表和异常恢复路径。

方案步骤 2「实现兼容层并完成验证」：进行中。
当前任务已覆盖灰度路由和旧 API 适配；回退演练尚无完成结果。

未覆盖：变更审计没有对应任务。
阻塞：无。
```

最后分别列出总体进度、步骤证据、阻塞项、未覆盖项和建议的下一任务；不要修改调研或任务状态。
