# macOS Agent Group Chat ViewModel 拆分

- 时间：2026-09-20 10:04:54 CST（Asia/Shanghai）
- 本轮目标：把 Agent Group Chat 的管理动作与消息/调度生命周期从主快照 ViewModel 拆出。
- 起始提交：`a2c429fb312443d22a026298fca40b44e20a27f9`
- 代码提交：`dc20602e5515ed1d6ae7f99f68e7b5ce4852038a`

## 实际改动

- `AgentGroupChatViewModel.swift` 从 1,085 行缩减至 302 行，保留状态、主快照和补充数据 generation。
- 新增 `AgentGroupChatManagement.swift`，承载 Agent、成员、团队、资产与提案动作。
- 新增 `AgentGroupChatMessaging.swift`，承载发送、@mention、Run 操作、暂停/停止、附件加载、分页合并和 Scheduler 生命周期。
- 并行存在的未使用 `members` 参数清理在提交后原样恢复为未提交改动，未纳入本轮所有权。

## 业务不变量

- 主快照与补充数据加载、取消 generation、附件合并和旧消息分页不变。
- 成员/项目经理权限、提案审批、消息路由和调度 drain 行为不变。
- 仅调整模块内部可见性与文件布局。

## 验证结果

- macOS Swift build：通过。
- Store 冻结基准查询计数保持冻结值，空闲场景 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- 阶段 3 目标热点已完成职责拆分；下一步审计并行性能修复的证据与阶段 4 门禁，然后执行阶段 5 整体验收。
- 当前 4 个并行修改均已保留且未提交。
