# ChatOS Swift

ChatOS 的正式原生 macOS 客户端。主工作区使用 SwiftUI 实现，不嵌入 Web 前端。

## 运行

```bash
swift run ChatOSSwift
```

通过 `swift run` 启动时，默认使用本机 APISIX 网关 `http://127.0.0.1:9080/api/chatos`；打包后的 App 从 `Info.plist` 读取线上 API 与 Local Connector 地址。两种方式都可分别使用 `CHATOS_API_BASE_URL` 和 `CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL` 覆盖。客户端只使用网关协议，不加载或嵌入 Web 前端。

要求 macOS 14+ 与 Swift 6.2+。

需要生成可双击运行、具有稳定 Bundle ID 的 Debug App 时：

```bash
./scripts/package-debug-app.sh
open .build/ChatOS.app
```

## 当前能力

- 原生资源侧栏与项目四个工作区。
- per-session 聊天状态、稳定 Turn 合并与正常消息输入框。
- `ChatOSAPI` 独立传输层，已实现 compact history、WebSocket ticket 与会话 Realtime 订阅。
- 原生登录页、启动 Token 校验与 macOS Keychain 安全存储。
- Requirement 执行规划与实时任务 DAG。
- 跟随会话上下文的 Computer Use / Browser MCP 画中画容器。
- 原生 Local Connector 管理、工作区授权、MCP/Skill/Plugin 目录与权限状态。
- 支持带本地 HTTP Runtime 的原生插件应用，并按用户和项目隔离运行数据。
- 原生项目目录、全文搜索、带行号与语法高亮的查看/编辑器。
- 本机代码导航：符号引用、定义跳转与 `⌘[` 返回历史。
- 原生项目运行配置、可展开实时日志与独立设置窗口。
- 全局快速搜索、剪贴板历史、区域/窗口/长截图、图片标注与屏幕录制。
- 可选的全局桌面宠物与悬浮交互。
- 设置中可统一调整应用字体大小。

代码导航目前采用按需本机索引和语言声明规则，不常驻扫描项目。它覆盖常见语言并提供快速降级导航；需要 IDE 级类型推断时，可在现有分层上继续接入对应语言的 LSP。

## 验证

```bash
swift build
swift test
```

设计、页面逻辑矩阵、聊天历史专项方案和实现路线位于 [`docs`](./docs)。
后端 `2.0.14` 协议变更见 [`docs/11-backend-protocol-2.0.14.md`](./docs/11-backend-protocol-2.0.14.md)。
