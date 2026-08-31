# Browser CDP 用户安装与首次使用指南

Browser CDP 包含两个必须配套安装的组件：ChatOS Marketplace 中的 **Browser CDP 插件**，
以及 Chrome 中的 **Chatos Browser Bridge 扩展**。只安装其中一个，无法控制用户当前的
Chrome；但仍可使用插件的隔离托管浏览器模式。

## 正式用户安装流程

1. 在 ChatOS 的“插件管理 → 插件市场”中找到 **Browser CDP**，点击安装。
2. 安装完成后启用插件，并按需授予“连接现有 Chrome”权限。页面读取、页面控制、网络
   观察和原始 CDP 等高权限仍由 ChatOS 分开审批。
3. 从 Chrome Web Store 安装 **Chatos Browser Bridge**，并建议固定到浏览器工具栏。
4. 在 ChatOS 中启动一个会使用 Browser CDP 的任务，使本机 Browser MCP 进程开始运行。
5. 打开 Chatos Browser Bridge，点击一次“首次连接”。这是用户对本机桥接的显式授权。
6. 状态显示“已连接”后即可使用。以后 Browser MCP、Local Connector 或扩展重启，扩展
   会在已配对状态下每两秒尝试自动恢复连接，不再重复弹出授权。

## 标签组行为

- 每个 ChatOS 任务使用自己的任务标题作为 Chrome 原生标签组名称。
- 任务创建的新标签自动进入该组，后续新标签继续加入同组。
- 用户通过“共享当前标签”授权的已有标签不会被移动，也不会改变原有分组。
- 任务结束后页面不会被关闭；标签组保留并自动折叠，便于用户检查结果。
- 已结束任务的标签会从下一任务的控制目录中移除，避免跨任务误操作。

## 更新与重启

- Browser CDP 插件由 ChatOS Marketplace 负责下载、校验、安装和版本更新。
- Chrome 扩展由 Chrome Web Store 自动更新。
- 正常更新不会清除配对状态。用户主动点击“断开连接”后，自动重连停止，需要再次执行
  一次“首次连接”。

## 常见问题

### 点击“首次连接”后提示找不到 Browser MCP

先确认 ChatOS 和 Local Connector 正在运行，然后在 ChatOS 中启动一个 Browser CDP
任务，再回到扩展点击“首次连接”。扩展不会连接远程服务，只会连接本机 Native
Messaging Host 和回环地址 Bridge。

### 任务可以启动托管浏览器，但不能连接当前 Chrome

确认同时满足以下条件：已安装 Chrome 扩展、已授予“连接现有 Chrome”权限、扩展显示
“已连接”。托管浏览器模式不依赖扩展，因此它可用并不能证明现有 Chrome 已完成配对。

### 扩展升级后一直显示等待连接

先启动一个 Browser CDP 任务并等待几秒。若仍未恢复，打开扩展查看错误提示；只有主动
断开过连接时才需要再次点击“首次连接”。

## 发布方上线检查清单

1. 将 `extension/dist` 打包为 Chrome Web Store ZIP 并提交审核。
2. 获得正式 Chrome Web Store Extension ID 后，以该 ID 设置
   `CHATOS_BROWSER_EXTENSION_ID` 重新编译全部平台的 Browser MCP Release 二进制。
3. 将带固定 Extension ID 的二进制放入 npm 包，重新生成 SBOM、哈希、签名和 Marketplace
   Release。
4. 在全新 Chrome Profile 和全新 ChatOS 安装环境执行上述六步首次安装流程。
5. 验证首次配对、自动重连、任务命名标签组、新标签入组、会话结束后保留并折叠。

在 Chrome Web Store 正式 Extension ID 写入 Browser MCP Release 之前，开发版必须通过
`CHATOS_BROWSER_EXTENSION_ID` 显式传入当前未打包扩展 ID；这种方式仅用于开发验收，
不能作为普通用户安装方案。
