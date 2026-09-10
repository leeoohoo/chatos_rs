# Web Design Studio 3.0.1 阶段 A：AI 渐进式计划与状态机

## 完成范围

阶段 A 已将 AI 生成计划从一次性执行器参数提升为独立、可校验、可持久化的产品状态。

新增模块：

- `src/v2/generation-plan-schema.ts`
- `src/v2/generation-state-machine.ts`
- `src/v2/generation-plan-store.ts`

## 已实现能力

- Site Plan 可以规划多个页面、弹层和状态画板，但一次只允许一个有界 Step 生成或验证。
- 画板是可长期恢复的设计上下文，不要求 AI 在一次 Run 中完成；安全提交一个 Step 后可以暂停、等待审阅或稍后回到同一画板继续。
- 页面按 Structure、Visual、Design Gate、可选 Interaction 和 Handoff 等有界任务推进。
- Interaction 必须依赖 Design Gate，不能在视觉设计完成前主导页面生成。
- Handoff 必须覆盖所有必需设计步骤。
- Step 进入审阅前必须包含真实页面截图或区域裁剪，以及质量报告。
- 每个 Attempt 保存 baseRevision、幂等键、视觉产物、错误、提交 revision 和状态。
- 单个 Step 完成后 Plan 返回明确的下一动作，不会把“当前画板”解释成必须连续执行到完成。
- Retry 只重试当前失败步骤，不改变已经接受的步骤。
- Rollback 会将依赖当前步骤的后续工作标记为 stale。
- Pause 只能发生在没有正在执行的 Step 时，防止留下不明确的半执行状态。
- projectId 与 documentId 共同参与 Plan 文件隔离；模型不能通过 documentId 跨项目读取计划。
- Plan Store 使用目录锁、临时文件、fsync 和原子 rename。
- optimistic revision 阻止并发写入静默覆盖。
- 相同 idempotencyKey 的网络重放不会重复创建 Attempt，也不会增加 Plan revision。
- 进程重启后可以恢复 activePageId、当前 Step 状态和全部 Attempt 记录。

## 状态边界

```text
Plan: draft → ready → running ⇄ paused → ready/completed
Page: planned → running ⇄ paused → completed
Step: planned → ready → generating → validating → awaiting-review → accepted
                                      └→ retryable / blocked
```

页面边界是硬边界：完成当前 Page 后必须显式调用下一次 `start-page`，状态机不会自行跨页。

## 验收结果

- 新增定向测试：14/14 通过。
- 插件全量测试：195/195 通过。
- TypeScript strict typecheck 通过。
- UI 与 Node 生产构建通过。
- `git diff --check` 通过。

## 下一阶段

阶段 B 将把候选生成改为：

```text
生成 Candidate Transaction
  → 临时 Scene 应用
  → 布局、截图、视觉定位与质量检查
  → 通过后正式提交
  → 失败时正式 Scene revision 保持不变
```

阶段 B 不再使用“正式写入后再验证”的执行顺序。
