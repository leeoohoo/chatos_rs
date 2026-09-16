# ChatOS Companion 微信小程序

这是 ChatOS 唯一的移动端，定位为伴随控制端：查看已登录桌面设备、浏览云端会话、发送消息、追加 Guidance、停止运行和处理 Ask User。它不会创建项目、注册设备或在手机本地执行工具。

## 本地开发

1. 在微信开发者工具中导入本目录。
2. 在本机创建 `project.private.config.json`，填入正式或测试小程序 AppID；该文件不会提交。
3. 在微信公众平台把 API Gateway 的 HTTPS/WSS 域名加入 request/socket 合法域名。
4. 体验版联调时将 User Service 的 `USER_SERVICE_WECHAT_MINI_PROGRAM_ENV_VERSION` 设为 `trial`；生产发布使用默认值 `release`。
5. 执行 `npm install && npm run check` 做 TypeScript 校验。

本地开发者工具使用 `develop` 版本时，默认连接 `http://127.0.0.1:9080`，绑定页会显示“进入测试通道”。该入口要求真实 ChatOS 用户名和密码，后端仍只签发受限的 Companion 会话。它仅存在于 User Service 的 debug 构建中，并且要求 `USER_SERVICE_WECHAT_MINI_PROGRAM_ENV_VERSION=develop`；体验版和正式版不可用。

默认生产网关是 `https://app.jgoool.com`。开发/预发环境通过小程序 ext config 提供 `apiOrigin` 覆盖，不允许把微信 AppSecret 放入本工程。

## 发布门禁

- User Service 已配置同一 AppID 对应的 AppSecret、独立身份哈希密钥和正确的小程序版本（`release` / `trial` / `develop`）。
- 微信小程序隐私保护指引、用户协议、服务类目和域名备案审核通过。
- 真机验证扫码绑定的桌面二次确认、Token 撤销、后台恢复对账和 WSS 重连。
- `npm run check`、后端测试及生产配置校验全部通过。
