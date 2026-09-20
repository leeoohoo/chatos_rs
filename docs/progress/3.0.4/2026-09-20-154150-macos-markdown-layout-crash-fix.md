# macOS Markdown 布局崩溃修复进度

- 时间：2026-09-20 15:41:50（Asia/Shanghai）
- 本轮目标：定位并修复客户端在 Agent 聊天更新后直接关闭的问题。
- 起始提交：`1e5affc8da5fad271604c9bd749a65307d7abb7d`
- 代码提交：`a8ca92e91d23c33b0e879c2677b1acfd4fca2b4c`

## 证据与根因

- macOS 崩溃报告记录 `EXC_BREAKPOINT / SIGTRAP`。
- 符号化栈明确落在 `MarkdownDocumentView.swift:272` 的 `MarkdownLayoutTextView.height(fittingWidth:)`。
- Swift 运行时错误为：`Double value cannot be converted to Int because it is either infinite or NaN`。
- 崩溃前相关网络请求均已成功返回；本轮未修改 Responses、网关或 Agent 调度行为。

## 实际改动

- `MarkdownNativeTextView.sizeThatFits` 只接受有限且大于零的提议宽度；SwiftUI 传入无穷宽度时回退给系统布局，不再进入文本高度计算。
- `MarkdownLayoutTextView` 对高度和内容宽度测量增加相同的有限值边界。
- 高度缓存键由易在非有限值转换时触发运行时陷阱的 `Int` 改为有限 `Double` 的位模式键。
- 新增覆盖 `nil`、正负无穷、`NaN`、零、负数和正常宽度的回归测试。

## 涉及文件

- `clients/macos/Sources/ChatOSApp/Features/Shared/MarkdownDocumentView.swift`
- `clients/macos/Tests/ChatOSAppTests/MarkdownRenderCacheTests.swift`

## 业务不变量

- 有限正宽度仍按原有 fill/fit-content 规则测量 Markdown。
- Markdown 内容、选择、渲染缓存及 Agent 消息数据不变。
- 非法或不确定的 SwiftUI 宽度提议交回系统布局处理，不伪造聊天内容尺寸。
- 不改变模型接口、Responses 协议、网关、附件和本地 Agent 调度。

## 验证结果

- `swift test --filter MarkdownRenderCacheTests`：5 项通过，0 失败。
- `make test-macos-client`：全部测试目标通过，0 失败；既有数据量性能基准因未设置显式环境变量而按设计跳过 1 项。
- Debug App 打包成功；SDK 校验为 `27.0 == 27.0`；深度签名校验通过。
- 修复版已安装至 `/Applications/ChatOS.app`，旧版保存在 `/Applications/ChatOS.app.before-markdown-layout-crash-20260920-153950`。
- 使用原有真实数据依次打开“小爱”长消息、“三国横版闯关游戏团队”和“小爱 · 阿策”私聊，客户端保持运行，未复现退出。

## 剩余风险与下一步

- SwiftUI 的布局提议由窗口与容器状态决定，自动测试锁定的是导致崩溃的非有限值边界；本轮同时用原始消息数据做了 UI 回归。
- 当前“小爱 · 阿策”历史 Run 因此前崩溃已被恢复为暂停状态，并显示既有的历史/记忆范围不一致提示；本轮没有丢弃或重放该 Run，避免改变用户数据。后续应由独立工作单元处理该恢复语义。
- 继续观察修复版；若再次退出，以新 `.ips` 的符号化栈为准，不做无证据修改。
