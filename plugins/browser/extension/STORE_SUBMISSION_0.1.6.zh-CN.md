# Chatos Browser Bridge 0.1.6 Chrome Web Store 提交清单

## 发布方式

- 在 Chrome Web Store 开发者后台更新现有条目，不要新建条目。
- 上传包：`extension/release/chatos-browser-bridge-0.1.6.zip`。
- 正式 Extension ID 必须保持 `jooaepjckiofmpldinopgdgddcoaofil`。
- 本版本没有新增 Chrome 权限。

## 更新说明

> Adds isolated concurrent connections for multiple Chatos Browser tasks using the same existing
> Chrome. Each task receives its own local Bridge connection, native tab group, target catalog,
> debugger sessions, event subscriptions, and cleanup lifecycle. Ending one task no longer affects
> another active Browser task. No new permissions, remote code, analytics, or data collection were
> added.

## 单一用途说明

> Chatos Browser Bridge lets a user authorize local Chatos Browser tasks to inspect and operate
> task-owned or explicitly shared Chrome tabs. It communicates only with the installed local Native
> Messaging Host and independently authenticated loopback WebSockets owned by active Browser CDP
> processes.

## 权限用途说明

- `debugger`：仅附加到某一任务创建或用户明确共享给该任务的标签，用于用户请求的页面检查、交互和网络诊断。
- `nativeMessaging`：发现并认证本机 `ai.chatos.browser_bridge`，一次返回所有活跃任务的本地 Bridge。
- `storage`：仅保存本机配对和自动重连偏好。
- `tabGroups`：将每个任务创建的标签放入该任务独立的 Chrome 原生标签组。
- `tabs`：读取已授权标签元数据并管理任务创建的标签；拒绝 Chrome 特权页和本地 scheme。

## 数据使用与隐私声明

扩展不销售数据，不用于广告、画像、信贷决策或无关分析。所有 Bridge 发现与控制连接均在
Native Messaging 和 `127.0.0.1`/`::1` 回环地址内完成。任务之间不共享 Bridge 凭据、标签
目录、调试会话或事件订阅。

隐私政策网址：<https://www.jgoool.com/privacy/browser-bridge>

## 审核前检查

1. 上传 `chatos-browser-bridge-0.1.6.zip`，确认后台识别版本为 `0.1.6`。
2. 确认权限列表与 0.1.5 相同，远程代码、数据销售、广告和分析均为否。
3. 保留现有正式 Extension ID，并确认隐私政策无需登录即可访问。
4. Browser CDP 插件配套发布为 `0.1.23`，ChatOS App 包含并行任务调度字段支持。

## 验收标准

1. 两个任务同时打开 `mode: chrome_extension`，扩展徽标显示连接数 `2`。
2. 两个任务创建不同的原生标签组，并只能看到、附加和操作自己的标签。
3. 结束第一个任务后，第二个任务的 Bridge、标签、CDP 会话和事件订阅仍然有效。
4. 更新扩展后保留既有配对；已配对用户无需重新授权。
