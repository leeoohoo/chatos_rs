# ChatOS Swift 重写文档

更新时间：2026-08-24

本目录是 ChatOS 原生客户端重写的产品与工程基线。目标不是把现有 React 页面逐像素搬进 SwiftUI，而是在保持服务端协议和 Local Connector 安全边界的前提下，重做一个符合 Apple 平台习惯的原生 macOS 产品。

## 当前结论

- 第一阶段以 macOS 为唯一交付平台，使用 Swift 6、SwiftUI、Observation、Swift Concurrency 和 URLSession。
- 最终目标是 macOS 客户端主工作区与 Local Connector 客户端都由 Swift 实现；现有浏览器 Web 端继续作为独立产品线，不在本项目中改写。
- macOS 客户端禁止用 `WKWebView`、iframe 或远端 Web 容器承载主工作区；登录、会话、任务、项目、文件、Git、终端和设置全部由原生 SwiftUI 实现。
- 主应用与 Local Connector 采用一个 macOS App、两个窗口场景：`ChatOS` 主工作区和 `Local Connector` 设备设置；菜单栏常驻展示连接与审批状态。
- 服务端继续作为 Project、Session、Message、Task、Requirement、Memory、Agent 的事实数据源。
- 设备文件、Git、终端、插件、凭据、系统权限和审批只在本机执行，不能因为重写而弱化现有安全边界。
- 现有 `chatos_rs` 工作区有大量未提交修改，本轮只读取它并在 `chatos_swift/docs` 新增交付物。

## 文档导航

1. [现有系统审计](./01-current-system-audit.md)
2. [Swift 目标架构](./02-swift-architecture.md)
3. [实施路线与里程碑](./03-implementation-roadmap.md)
4. [Apple 风格 UI 规范](./04-ui-spec.md)
5. [页面清单与验收范围](./05-page-inventory.md)
6. [聊天历史专项架构](./06-chat-history-architecture.md)
7. [原生 UI 架构决定](./07-native-ui-decision.md)
8. [任务流程图专项架构](./08-task-graph-architecture.md)
9. [页面真实逻辑矩阵](./09-page-logic-matrix.md)
10. [插件实时画面与画中画](./10-plugin-visual-preview.md)
11. [后端协议 2.0.14](./11-backend-protocol-2.0.14.md)
12. [资源创建实施方案](./12-resource-creation-implementation-plan.md)
13. [Computer Use 可靠性实施方案](./13-computer-use-reliability-implementation-plan.md)
14. [全局宠物悬浮实施方案](./14-global-pet-overlay-implementation-plan.md)
15. [设置页 UI 架构](./15-settings-ui-architecture.md)
16. [界面本地化架构](./16-interface-localization-architecture.md)
17. [全局效率工具实施方案](./17-global-productivity-tools-implementation-plan.md)
18. [macOS / Windows 跨平台变更登记](./18-cross-platform-change-register.md)
19. [页面设计稿](./design/README.md)

## 设计预览

![SwiftUI 视觉方向板](./design/assets/visual-direction-ai.png)

## 建议的第一个工程提交

建立 Xcode workspace 和最小可运行骨架，只实现：登录、真实资源侧栏、项目四 Tab 壳、Local Connector 状态窗口、统一网络层、Keychain Token、聊天历史 SQLite 骨架、Mock 数据与快照测试。先冻结协议、Turn 模型和页面逻辑矩阵，再迁移复杂运行时。
