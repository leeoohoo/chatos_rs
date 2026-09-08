# Web Design Studio v2 阶段 3：AI 设计协议与视觉闭环

阶段 3 不让 AI 从坐标 Patch 开始。输入必须先形成 Brief 与 Design Spec，再按语义区域逐段创建 Scene，最后经过布局求解、浏览器截图和视觉评审。

## 第一批已实现

- Design Brief：保留 projectId、documentId、目标、受众、品牌、页面、语义区块、视口、无障碍和禁止模式；
- Design Spec：设计原则、Token 意图、页面区块、布局意图、组件策略和响应式意图；
- Scope 一致性：Spec 必须原样透传 Brief 的 projectId 与 documentId；
- 非坐标优先：布局只能先表达 Auto/Grid/Free 的设计意图、内容宽度和响应式策略，不接受 absolute/x/y 式页面规划；
- 分区生成计划：每个语义区块对应一个独立 Scene Transaction，按依赖顺序执行；
- 每页生成后必须依次 solve、render snapshots、critique and revise。
- Scene Transaction 已支持原子插入 Variable Collection，设计系统不再游离于 revision 之外；
- Generation Executor 严格按依赖执行，设计系统和每个语义区块分别形成 AI Transaction；
- 每个事务记录 revision、Transaction Summary 和字段级 Scene Diff；
- solve 与 snapshot 必须为请求的每个视口返回唯一产物；
- 任一步失败后依赖步骤明确标记 blocked，失败事务不会留下部分数据。
- Visual Quality Report 合并语义区块完整度、布局诊断、必需视口和浏览器校准问题；
- 每个问题包含稳定 issueId、nodeIds、viewportWidth、严重度和定向修复建议；
- Visual Repair Request 只暴露可修复目标节点，人工 locked、不可编辑或存在 lockedFields 的节点进入 protected/blocked；
- 修复请求强制重新布局、截图和校准，不能只在数据层把 issue 标记完成。
- 批注可以形成带 projectId/documentId、baseRevision、目标子树、显式依赖和必验视口的 AI Task；
- 批注任务与视觉 Repair 共用同一个作用域验证器，只允许命中目标子树和明确列出的依赖节点；
- 作用域事务禁止修改 Page、Variable Collection、保护策略或批注状态，也禁止删除目标根和移动/删除依赖节点；
- AI 即使通过作用域校验，仍必须通过 Scene Transaction 的字段级人工锁定校验；
- 批注只有在事务真实产生修改，并且全部目标视口的 layout、snapshot、browser calibration 均通过后，才由 system transaction 关闭。
- Repair Executor 会重新读取当前 revision、验证定向 AI Transaction、原子保存、生成 Diff，并要求每个 Brief 视口都有新的 layout、snapshot 和 calibration；
- SaaS、电商和杂志三种不同结构已覆盖“空 Scene → Design System → 分区事务 → 响应式规则 → 截图 → 视觉评审 → 定向 Repair → 重新验收”的端到端链路；
- 三种网站分别使用分屏 Hero、商品目录与非对称编辑网格，验收会拒绝结构签名或区块角色序列相同的同构模板。

## 阶段结论

阶段 3 的协议、执行、视觉评审、定向修复、批注任务和三类网站端到端闭环已具备正式验收覆盖。
