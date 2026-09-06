# Chatos Browser Bridge 0.1.5 Chrome Web Store 提交清单

## 发布方式

- 在 Chrome Web Store 开发者后台更新现有的 Chatos Browser Bridge 条目，不要新建条目。
- 上传包：`extension/release/chatos-browser-bridge-0.1.5.zip`
- 正式 Extension ID 必须保持为现有商店条目的 ID。
- 不要使用本机开发 ID `dgcallmbidliekanmcicenhjdbkaacak` 构建公开发行版。

## 更新说明

> Fixes a reliability issue that could disconnect an authenticated local Browser Bridge about two
> minutes after connection. The short-lived bootstrap credential is still validated and consumed
> only during authentication; an already authenticated local WebSocket session now remains active.
> No new permissions, remote code, analytics, or data collection were added.

## 单一用途说明

> Chatos Browser Bridge lets a user explicitly share selected Chrome tabs with the locally installed
> Chatos Browser CDP service so an authorized Chatos task can inspect and operate those tabs. The
> extension communicates only with the local Native Messaging Host and a loopback WebSocket owned by
> the installed Browser CDP process.

## 权限用途说明

### debugger

> Required to attach Chrome DevTools Protocol sessions only to tabs that the user explicitly shares,
> or to tabs created by the active Chatos task. It is used for page inspection, interaction, network
> diagnostics, and other user-requested browser automation.

### nativeMessaging

> Required to discover and authenticate the locally installed `ai.chatos.browser_bridge` Native
> Messaging Host. The native host starts the authenticated loopback bridge used by the extension.

### storage

> Stores only local pairing and reconnect preferences. It is not used for browsing-history,
> credential, page-content, advertising, or analytics storage.

### tabGroups

> Groups tabs created by one Chatos task into a task-named native Chrome tab group. Existing
> user-shared tabs are never moved into that group.

### tabs

> Reads metadata for explicitly shared tabs and manages tabs created by the active Chatos task.
> Privileged Chrome pages and local schemes are rejected.

## 数据使用与隐私声明

> The extension does not sell user data or use it for advertising, profiling, credit decisions, or
> unrelated analytics. It communicates first with the locally installed Chatos Browser CDP process
> through Native Messaging and an authenticated 127.0.0.1 WebSocket. Only tabs explicitly shared by
> the user, plus tabs created by the active task, are controllable. When the user requests an AI task
> to inspect or operate a page, the minimum data necessary to complete that task may be processed by
> the AI service configured for the task, as disclosed in the privacy policy.

## Chrome 商店数据使用勾选

- 身份验证信息
- 网络记录
- 用户活动
- 网站内容
- 其余数据类别不勾选
- Limited Use 的 3 个承诺全部勾选

## 隐私政策网址

> https://www.jgoool.com/privacy/browser-bridge

提交前确认该地址无需登录即可公开访问。

## 审核前检查

1. 在现有商店条目上传 `chatos-browser-bridge-0.1.5.zip`。
2. 确认商店后台识别的版本为 `0.1.5`，权限列表没有新增项目。
3. 确认隐私政策 URL 可公开访问。
4. 权限声明逐项使用上面的说明。
5. 确认“远程代码”选择否，“数据销售/广告/分析”全部选择否。
6. 提交审核后保留现有正式 Extension ID。
7. Browser CDP 生产插件应发布为 `0.1.9`，且 `doctor` 中
   `extension_id_configured` 必须等于正式 Extension ID。

## 验收标准

1. 已授权用户默认返回 `mode: chrome_extension`，未授权用户自动回退 `mode: managed`。
2. 模型可见的 `browser_session_open` 参数中不存在 `mode`、`headless` 或
   `persistent_profile`。
3. 任务创建的页面进入任务命名的 Chrome 原生标签组，并显示虚拟鼠标。
4. 连续操作至少 3 分钟，Browser Bridge 不因 bootstrap credential 到期而断开。
5. 更新扩展后保留既有配对状态；已配对用户无需再次授权。
