# macOS 文档预览本地化门禁修复

- 时间：2026-09-20 10:11:22 CST（Asia/Shanghai）
- 本轮目标：修复阶段 5 打包门禁发现的 Markdown 文档预览本地化缺口。
- 起始提交：`9330f9bbea094bf365833b007c3823b4596c2000`
- 代码提交：`6e31131087e7c1772c05e5edd3c1d4d37e5aab82`

## 实际改动

- 为“另存为”“另存为…”“搜索文档”“正在解析 Markdown…”补齐英文映射。
- 在简体中文目录补齐相同 key 的 identity 条目，保持双语 catalog 完整。

## 涉及文件

- `clients/macos/Support/Localization/en.lproj/Localizable.strings`
- `clients/macos/Support/Localization/zh-Hans.lproj/Localizable.strings`

## 业务不变量

- 不改变文档预览、搜索、另存为或 Markdown 后台解析行为。
- 中文界面文案不变；英文界面使用明确对应翻译。

## 验证结果

- 本地化审计：UI 中文 literals 412，缺失英文条目 0；中文 identity 条目 575，缺失/非 identity 0。
- `swift build --package-path clients/macos`：退出码 0。
- `swift test --package-path clients/macos`：退出码 0。
- `make test-macos-client`：退出码 0。
- `clients/macos/scripts/package-debug-app.sh`：退出码 0，产物 `clients/macos/.build/ChatOS.app`。
- `codesign --verify --deep --strict`：通过；`spctl` 对临时 ad-hoc debug 包拒绝属于未公证开发包的预期限制。

## 剩余风险与下一步

- 已安装的 `/Applications/ChatOS.app` 正在运行，为避免第二实例争抢本地数据库与运行时，本轮未强制启动 debug 包。
- 登录后真实账号关键路径仍需运行时 Secret；当前环境未注入测试账号，不能在不违反凭据规则的前提下自动执行。
