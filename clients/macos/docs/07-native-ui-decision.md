# ADR-001：macOS 客户端主工作区必须完全原生 SwiftUI

状态：已决定
日期：2026-08-24

## 背景

现有 Local Connector Desktop 使用 Electron 壳，在独立 `WebContentsView` 中加载远端 ChatOS Web 主页面，本机设置则由另一套 React UI 展示。虽然不是 HTML `iframe` 标签，但产品效果和架构问题相同：桌面壳不拥有主界面，主前端与本机界面是两套技术和状态系统。

## 决定

新的 macOS 客户端完全重写客户端内的主工作区；浏览器 Web 端继续独立维护：

- 不加载现有 ChatOS Web bundle。
- 不使用 `WKWebView` 作为主窗口、登录页、会话页或项目工作区。
- 不通过 JavaScript bridge 维持主应用状态。
- Swift 客户端直接调用后端 REST、Realtime 与 Local Connector actor API。
- 所有一级页面、导航、窗口、菜单、Inspector、错误状态和设置均由 SwiftUI 实现。
- Local Connector 与客户端主工作区共享同一个原生 DesignSystem、AuthSession 和 domain model，但保持状态所有权边界。

## 允许的局部 Web 内容

只有以下隔离场景可以使用 `WKWebView`：

1. 第三方 Plugin 明确提供的沙箱化 UI。
2. 第一阶段暂时无法高质量原生渲染的 Mermaid。
3. KaTeX/复杂数学公式的隔离渲染。
4. OAuth 授权页；优先使用 `ASWebAuthenticationSession`，不是常驻 WebView。

这些局部内容必须：

- 有独立 origin 与内容安全策略。
- 使用最小 bridge，不暴露任意本机 API。
- 不能拥有全局导航、认证状态或项目状态。
- 崩溃或加载失败时不能拖垮主工作区。

## 直接结果

- macOS 端不再依赖 Electron、Chromium、React、Vite 或远端前端兼容性。
- 主界面可以正确使用多窗口、Toolbar、Menu Commands、Settings、Drag & Drop、Quick Look、ShareLink、系统服务和无障碍能力。
- 需要重新实现现有前端功能，工作量更大，但数据流和聊天历史等基础模块可以从根本上重做，而不是继续继承浏览器状态问题。
- Web 前端可以继续作为独立产品线存在，但不与 Swift App 共用 View 层；两端只共享 API 契约、领域语义和设计 token。

## 验收

- App 在 ChatOS Web 服务 `8088` 未启动时仍可完整登录和使用主功能。
- Release 包不包含 ChatOS Web bundle、Electron 或 Chromium runtime。
- 断网但本地缓存可用时，主窗口仍能展示历史与项目元数据。
- UI 自动测试可以直接访问 SwiftUI accessibility tree，不需要查询 DOM。
