# Agent 聊天滚动虚拟化与回归验收

日期：2026-10-08

## 原因与边界

仅停止滚动坐标的 SwiftUI 状态发布，仍不能解决普通 VStack 对整页 Markdown
消息的创建、测量和历史累积。群聊与 Agent 私聊现在共享 NSTableView 时间线；
只有可见行创建 NSHostingView，离屏行由原生表格回收。数据库分页及已加载消息
元数据不截断，也不删除历史。

相同消息和行依赖不重新加载行；头像、附件、提案操作、字号及主题发生变化时
才刷新相关展示。私聊排序结果及 Agent 字典在源数据变化时缓存。
行高度测量合并到下一次主队列处理，缓存上限为 512 个。

阅读位置用消息 ID 和像素偏移保存。追加消息只在底部跟随；加载更早历史、
异步 Markdown 高度变化和窗口宽度变化保持阅读锚点。主动发送仍请求跳到底部。
切换会话使用独立视图身份，退出时清除原生观察器和回调。

## 自动回归

`AgentChatNativeTimelineTests` 覆盖实际 NSWindow/NSScrollView/NSTableView：

- 1,000 条消息首次构建少于 30 个行视图，500 次相同刷新不构建新行。
- 缩放与滚动之后累计构建少于 60 个行视图；定高行缩放后仍正确测量。
- 1,000 条真实 Markdown 消息的异步测量、底部定位、追加和宽度变化。
- 前插 20 条历史后锚点偏移误差小于 2 像素；主动跳转准确到底部。
- 10,000 次高度写入仍限定 512 个缓存条目；拒绝非有限数与负高度。

完整测试：

```sh
swift test --package-path clients/macos
```

优化构建的专项测试：

```sh
swift test --package-path clients/macos \
  --scratch-path clients/macos/.build/release-regression \
  --configuration release -Xswiftc -DDEBUG \
  --filter AgentChatNativeTimelineTests
```

`-DDEBUG` 仅让仓库已有 SQLite 测试计数接口参与编译，否则 SwiftPM 即使按名称
筛选也会编译失败；优化级别仍为 Release。正式安装包不启用此测试标志。
上述构建数量衡量的是按需渲染，不是 FPS，也不是整机性能承诺。

这些测试随现有 macOS 完整测试进入 CI；不可通过删除历史、限制加载条数或跳过
长 Markdown 来满足阈值。正式安装必须从干净的已推送提交生成 Release 包，
核对产物 SHA、构建配置和签名，再检查真实群聊与私聊的滚动展示。
