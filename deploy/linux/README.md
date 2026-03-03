# 无 Docker 部署（Ubuntu 22.04）

## 1) 在本地构建前后端

```bash
cargo build --release --manifest-path chat_app_server_rs/Cargo.toml
npm --prefix chat_app ci
npm --prefix chat_app run build
```

## 2) 上传代码到服务器

建议把整个项目目录上传到服务器（例如 `/home/ubuntu/chatos_rs`）。

## 3) 在服务器执行安装脚本

```bash
cd /home/ubuntu/chatos_rs
sudo BACKEND_PORT=13001 SERVER_NAME=your.domain.com bash scripts/server-install-nodocker.sh
```

可选变量：

- `APP_ROOT`：默认 `/opt/chatos`
- `SERVICE_NAME`：默认 `chatos-backend`
- `SERVICE_USER` / `SERVICE_GROUP`：默认 `chatos`
- `BACKEND_PORT`：默认 `13001`
- `SERVER_NAME`：Nginx `server_name`，默认 `_`
- `FORCE_ENV_REWRITE=1`：覆盖重建 `/etc/chatos/chatos-backend.env`

## 4) 部署后检查

```bash
systemctl status chatos-backend
journalctl -u chatos-backend -f
curl http://127.0.0.1:13001/health
```

## 5) 环境变量文件

脚本会生成：

- `/etc/chatos/chatos-backend.env`

可手动编辑后重启：

```bash
sudo systemctl restart chatos-backend
```

## 6) HTTPS（推荐）

先保证域名解析到服务器，然后使用 certbot：

```bash
sudo apt-get update
sudo apt-get install -y certbot python3-certbot-nginx
sudo certbot --nginx -d your.domain.com
```
