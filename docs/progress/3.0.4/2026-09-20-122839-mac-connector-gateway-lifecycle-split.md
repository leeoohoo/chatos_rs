# macOS Local Connector 网关生命周期拆分

- 时间：2026-09-20 12:28:39 CST（Asia/Shanghai）
- 本轮目标：纯拆分 `NativeLocalConnectorService.swift` 的网关连接、心跳、重连、凭证续签和关闭状态机。
- 起始提交：`ee4e9b8a3`
- 代码提交：`13bd3dbca`

## 实际改动

- 新增 `NativeLocalConnectorService+GatewayConnection.swift`，集中承载默认工作区注册、WebSocket 建连与收包、heartbeat、认证拒绝续签、退避重连、Plugin 安装状态上报和连接清理。
- `NativeLocalConnectorService.swift` 从 1,006 行降到 577 行；新网关生命周期文件为 443 行。
- 为跨文件 actor extension 将所需依赖和隔离状态收窄为模块内部可见；没有新增公开 API，也没有把状态移出 actor 隔离。
- 未修改 Relay 消息类型分发、凭证存储位置、重连退避、heartbeat 周期、审批清理或 Plugin session 保留规则。

## 涉及文件

- `clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+GatewayConnection.swift`

## 业务不变量

- 设备配对、默认工作区指纹、账号一致性校验和凭证续签冷却保持不变。
- 网关收到的 terminal、MCP、Plugin、workspace 和 companion 消息仍路由至原处理器。
- 瞬时网关故障不销毁已准备的 Plugin MCP session；显式断开和系统休眠仍执行原清理语义。
- 所有可变连接状态仍由 `NativeLocalConnectorService` actor 串行隔离。

## 验证结果

- `swift test --package-path clients/macos --filter NativeConnectorReconnectPolicyTests`：3 通过，0 失败。
- `swift test --package-path clients/macos --filter NativeConnectorStateStoreTests`：10 通过，0 失败。
- `swift test --package-path clients/macos --filter NativeConnectorDeviceAuthenticationTests`：1 通过，0 失败。
- `swift test --package-path clients/macos`：全量通过；现有环境门禁测试按设计跳过。
- `git diff --check`：通过。

## 剩余风险与下一步

- 本轮是结构拆分，没有连接到真实生产网关执行断线重连 E2E；协议和策略由现有单元及全量回归锁定。
- 下一步按两份方案重新审计 macOS 阶段 0～5 的自动化门禁、热点文件和有证据的性能候选；只处理仍有可复现证据的遗留项。
- 登录后真实账号关键路径仍要求运行时 Secret/环境变量，禁止写入源码、文档、命令输出或 Git。
