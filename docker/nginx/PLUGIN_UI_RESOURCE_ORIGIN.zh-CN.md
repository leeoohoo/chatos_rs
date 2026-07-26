# Plugin UI 独立资源域名部署

生产环境必须配置两个不同的 HTTPS Origin：

```env
CHATOS_PLUGIN_UI_PARENT_ORIGIN=https://app.jgoool.com
CHATOS_PLUGIN_UI_RESOURCE_ORIGIN=https://plugin-ui.jgoool.com
```

其中 `plugin-ui.jgoool.com` 必须：

- 具有独立 DNS 记录并进入受信任 TLS 证书的 SAN；
- 只把 `/api/plugin-ui/workbench/` 的 GET/HEAD 请求转发到 ChatOS Backend；
- 对根路径、普通 API、登录、会话和健康检查返回 404；
- 保留客户端发送的真实 `Host`，不得用可伪造的 forwarded host 替换；
- 不添加 CORS 响应头，不允许请求体、WebSocket upgrade 或其他 HTTP 方法；
- 不经过 ChatOS Frontend SPA，避免主业务 Host 和资源 Host 路由混合。

仓库内 `jgoool-https.conf` 已包含上述最小路由。部署前可以在不启动 Docker、Nginx 或任何端口的情况下执行：

```bash
docker/verify-plugin-ui-origin.sh
```

DNS、证书和公网反向代理完成后，再显式运行在线验收：

```bash
docker/verify-plugin-ui-origin.sh --live
```

在线验收只发起 HTTPS HEAD 请求，验证：

- parent/resource 两个域名均可解析且证书受信任；
- 主业务 Origin 对 Workbench 资源路径返回 404；
- 资源 Origin 根路径返回 404；
- 资源 Origin 上的无效 session 路径返回 404。

此脚本不能替代带真实短期 Workbench session 的 installed-app E2E；后者仍需在 macOS 和 Windows 发布包上单独完成。
