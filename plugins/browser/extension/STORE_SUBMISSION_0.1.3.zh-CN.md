# Chatos Browser Bridge 0.1.3 Chrome Web Store 提交清单

## 发布方式

- 如果开发者后台已有 Chatos Browser Bridge 条目，必须更新原条目，不能新建条目。
- 上传包：`extension/release/chatos-browser-bridge-0.1.3.zip`
- 审核通过后记录原条目显示的正式 Extension ID。
- 不要使用本机开发 ID `dgcallmbidliekanmcicenhjdbkaacak` 构建公开发行版。

## 更新说明

> Compatibility release for the production Chatos Browser CDP bridge identity. No new permissions,
> remote code, analytics, or data collection were added.

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

该地址必须先随官网部署上线，并确认无需登录即可公开访问，再提交 Chrome 商店审核。

## 审核前检查

1. 在原有商店条目上传 0.1.3 ZIP。
2. 确认商店后台的隐私政策 URL 可公开访问。
3. 权限声明逐项使用上面的说明。
4. 确认“远程代码”选择否，“数据销售/广告/分析”全部选择否。
5. 提交审核前记录正式 Extension ID。
6. 用正式 ID 执行：
   `CHATOS_BROWSER_EXTENSION_ID=<正式ID> ./scripts/deploy-online.sh plugin browser`
7. 发布脚本必须显示 Browser CDP 0.1.7，且 `doctor` 中
   `extension_id_configured` 必须等于正式 ID。
