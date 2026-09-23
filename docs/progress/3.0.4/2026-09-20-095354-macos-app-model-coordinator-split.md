# macOS AppModel 协调职责拆分

- 时间：2026-09-20 09:53:54 CST（Asia/Shanghai）
- 本轮目标：把 AppModel 的长生命周期与领域协调方法从组合根主文件拆出。
- 起始提交：`8dbe773fa94ad210d59abf9e89a87be6f5eeabda`
- 代码提交：`14ffbfa5371eff605a3409cd452e190cd80ff9bb`

## 实际改动

- `AppModel.swift` 从 1,590 行缩减至 417 行，保留状态、依赖与初始化组合根。
- 新增 Agent Runtime、Authentication/Language、Companion Runtime、Pet/Global Utilities、Resources/Conversation Cache、Visual Session 和 Workspace 协调扩展。
- `LocalConnectorCompanionRuntimeProviding` 的协议符合性保留在主类型声明，避免跨文件 Sendable 符合性警告。
- 跨文件扩展所需状态由文件私有调整为模块内部可见；未改变方法实现或调用顺序。

## 业务不变量

- 认证切换、工作区 generation、会话缓存、项目聊天准备和选择恢复不变。
- Agent heartbeat/artifact sync、Visual Session 轮询、宠物活动和本地 Connector 恢复语义不变。
- 语言偏好持久化、远端连接、插件应用和 companion 资源解析不变。

## 验证结果

- macOS Swift build：通过。
- Store 冻结基准查询计数保持冻结值，空闲场景 0 写。
- macOS Swift 全量测试：退出码 0。

## 剩余风险与下一步

- AppModel 主文件已收敛为组合根；协调扩展仍复用同一状态对象，但生命周期和职责边界已独立文件化。
- 阶段 3 尚需处理 Agent Workspace/ViewModel；当前两个目标文件存在并行修改，进入前必须审计并保留这些改动。
