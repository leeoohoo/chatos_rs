# macOS 云端 Agent 文档本地化门禁修复

- 时间：2026-09-20 12:34:40 CST（Asia/Shanghai）
- 本轮目标：修复最终打包门禁发现的云端 Agent 文档界面英文 catalog 缺失。
- 起始提交：`d8b74978a`
- 代码提交：`1a72c0d9a`

## 实际改动

- 为“云端 Agent 文档”入口、跨设备说明、加载态、空状态和预览动作补齐 7 条英文翻译。
- 为相同 key 补齐简体中文 identity catalog，确保中英文资源集合一致。
- 未修改远端 artifact 的列表、下载、校验、预览或权限行为。

## 涉及文件

- `clients/macos/Support/Localization/en.lproj/Localizable.strings`
- `clients/macos/Support/Localization/zh-Hans.lproj/Localizable.strings`

## 业务不变量

- 当前账户边界、跨设备发现范围和“本地消息历史未同步”的产品说明保持不变。
- UI 仍使用原 SwiftUI 本地化 key；仅补齐资源 catalog，不调整交互或布局。

## 验证结果

- `swift build --package-path clients/macos`：通过。
- `swift test --package-path clients/macos`：全量通过。
- `make test-macos-client`：通过。
- `clients/macos/scripts/package-debug-app.sh`：通过；UI 中文 literal 419，缺失英文 0；中文 identity 条目 582，缺失或不一致 0。
- `codesign --verify --deep --strict clients/macos/.build/ChatOS.app`：通过。
- `git diff --check`：通过。

## 剩余风险与下一步

- 自动化与打包门禁已闭环；真实账号、MinIO 和第二设备 E2E 仍需安全注入运行时凭证与对应外部环境。
- 工作区中的 Agent ViewModel、Story 布局和 SQLite 测试并发修改继续保持未提交，未纳入本轮所有权。
