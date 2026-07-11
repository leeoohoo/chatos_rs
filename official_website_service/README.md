# Official Website Service

Chatos RS 面向最终用户的产品官网，负责产品介绍、邀请注册、桌面连接器下载和公开站点元数据。

## 主要能力

- 面向最终用户的产品落地页，而不是内部服务状态或技术架构博客。
- 通过 User Service 完成邀请码校验、邮箱验证码发送和账号注册。
- 从 MinIO / S3 兼容对象存储读取最新客户端发布清单。
- 为客户端下载生成短期签名 URL，MinIO 凭据不会暴露给浏览器。
- 通过受保护的发布接口为客户端制品和发布清单生成上传 URL。

## 本地构建

```bash
cd official_website_service/frontend
npm install
npm run build
```

```bash
cargo build -p official_website_service_backend
```

默认地址：

- 前端：`http://localhost:39251`
- 后端：`http://localhost:39250`

## 注册配置

官网后端代理以下 User Service 接口，浏览器不需要直接跨域访问 User Service：

- `POST /api/site/auth/register/send-code`
- `POST /api/site/auth/register`

相关配置：

```env
OFFICIAL_WEBSITE_USER_SERVICE_BASE_URL=http://user-service-backend:39190
OFFICIAL_WEBSITE_APP_URL=https://app.example.com
```

当前注册流程沿用平台的邀请测试规则，需要邀请码、邮箱验证码和至少 6 位密码。

## MinIO 客户端发布配置

官网使用 S3 Signature V4 与 MinIO 通信。部署时创建一个用于客户端制品的 bucket，并配置：

```env
OFFICIAL_WEBSITE_RELEASES_ENDPOINT=https://minio.example.com
OFFICIAL_WEBSITE_RELEASES_REGION=us-east-1
OFFICIAL_WEBSITE_RELEASES_BUCKET=chatos-releases
OFFICIAL_WEBSITE_RELEASES_ACCESS_KEY=your-access-key
OFFICIAL_WEBSITE_RELEASES_SECRET_KEY=your-secret-key
OFFICIAL_WEBSITE_RELEASE_CHANNEL=stable
OFFICIAL_WEBSITE_RELEASE_PRESIGN_EXPIRES_SECONDS=900
OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN=replace-with-a-long-random-token
```

如果专用配置为空，endpoint、region 和凭据会回退到现有的 `CHATOS_OBJECT_STORAGE_*` 配置；客户端 bucket 仍默认使用 `chatos-releases`，避免与聊天附件混放。

## 打包并发布 Windows 客户端

先生成 Electron 客户端压缩包：

```powershell
powershell -ExecutionPolicy Bypass -File .\local_connector_client\package-electron-windows-client.ps1
```

再通过官网后端获取 MinIO 上传 URL并发布：

```powershell
$env:OFFICIAL_WEBSITE_API_BASE = "https://www.example.com"
$env:OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN = "replace-with-your-token"

powershell -ExecutionPolicy Bypass `
  -File .\local_connector_client\publish-release-to-minio.ps1 `
  -Version "2.0.4"
```

发布脚本会：

1. 计算客户端 ZIP 的 SHA-256。
2. 请求 `POST /api/site/admin/releases/presign`。
3. 直接把 ZIP 上传到 MinIO。
4. 最后上传 `latest.json` 发布清单，使官网立即展示新版本。

默认对象结构：

```text
chatos-releases/
└── releases/local-connector/stable/
    ├── latest.json
    └── 2.0.4/ChatOS-Local-Connector-windows-x64.zip
```

## 公开接口

- `GET /health`
- `GET /api/site/manifest`
- `GET /api/site/downloads`
- `GET /api/site/downloads/{platform}`
- `POST /api/site/auth/register/send-code`
- `POST /api/site/auth/register`
- `GET /robots.txt`
- `GET /sitemap.xml`

发布接口 `POST /api/site/admin/releases/presign` 必须携带：

```http
Authorization: Bearer <OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN>
```

## 验证

```bash
cd official_website_service/frontend
npm run type-check
npm run build

cargo test -p official_website_service_backend
bash scripts/smoke-official-website.sh
```
