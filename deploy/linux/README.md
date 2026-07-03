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

如果要同时启用 Linux OS 用户级进程隔离：

```bash
cd /home/ubuntu/chatos_rs
sudo ENABLE_PROCESS_ISOLATION=1 \
  PROCESS_ISOLATION_PRIVILEGE_MODE=capabilities \
  BACKEND_PORT=13001 \
  SERVER_NAME=your.domain.com \
  bash scripts/server-install-nodocker.sh
```

可选变量：

- `APP_ROOT`：默认 `/opt/chatos`
- `SERVICE_NAME`：默认 `chatos-backend`
- `SERVICE_USER` / `SERVICE_GROUP`：默认 `chatos`
- `BACKEND_PORT`：默认 `13001`
- `SERVER_NAME`：Nginx `server_name`，默认 `_`
- `FORCE_ENV_REWRITE=1`：覆盖重建 `/etc/chatos/chatos-backend.env`
- `CHATOS_WORKSPACE_DIR`：用户项目/工作目录根路径，默认 `$APP_ROOT/backend/data/workspace`
- `ENABLE_PROCESS_ISOLATION=1`：写入 `CHATOS_PROCESS_ISOLATION_ENABLED=true` 并生成 systemd drop-in
- `PROCESS_ISOLATION_PRIVILEGE_MODE`：默认 `capabilities`，也可设为 `root`

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

如果注册后选择项目目录时提示“当前用户没有可访问的本地目录”，检查环境文件里
`CHATOS_WORKSPACE_DIR` 是否存在且服务用户可写：

```bash
grep '^CHATOS_WORKSPACE_DIR=' /etc/chatos/chatos-backend.env
sudo -u chatos test -w "$(grep '^CHATOS_WORKSPACE_DIR=' /etc/chatos/chatos-backend.env | cut -d= -f2-)"
sudo systemctl restart chatos-backend
```

启用 OS 用户级进程隔离后，脚本会生成：

- `/etc/systemd/system/chatos-backend.service.d/process-isolation.conf`

已部署环境可以单独开启：

```bash
sudo SERVICE_NAME=chatos-backend bash scripts/configure-linux-process-isolation.sh
```

默认使用 `capabilities` 模式，让服务继续以 `chatos` 用户运行，同时授予
`CAP_SETUID`、`CAP_SETGID`、`CAP_CHOWN`。这是终端和 stdio MCP 降权到用户 UID/GID
所必需的能力。

## 6) HTTPS（推荐）

先保证域名解析到服务器，然后使用 certbot：

```bash
sudo apt-get update
sudo apt-get install -y certbot python3-certbot-nginx
sudo certbot --nginx -d your.domain.com
```
