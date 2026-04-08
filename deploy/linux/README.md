# 无 Docker 部署（Ubuntu 22.04）

## 1) 在本地构建前后端

```bash
cargo build --release --manifest-path agent_orchestrator/Cargo.toml
npm --prefix agent_workspace ci
npm --prefix agent_workspace run build
```

## 2) 上传代码到服务器

建议把整个项目目录上传到服务器（例如 `/home/ubuntu/agent_stack`）。

## 3) 在服务器执行安装脚本

```bash
cd /home/ubuntu/agent_stack
sudo BACKEND_PORT=13001 SERVER_NAME=your.domain.com bash scripts/server-install-nodocker.sh
```

可选变量：

- `APP_ROOT`：默认 `/opt/agent_orchestrator`
- `SERVICE_NAME`：默认 `agent-orchestrator-backend`
- `SERVICE_USER` / `SERVICE_GROUP`：默认 `agent-orchestrator`
- `BACKEND_PORT`：默认 `13001`
- `SERVER_NAME`：Nginx `server_name`，默认 `_`
- `FORCE_ENV_REWRITE=1`：覆盖重建 `/etc/agent_orchestrator/agent-orchestrator-backend.env`

## 4) 部署后检查

```bash
systemctl status agent-orchestrator-backend
journalctl -u agent-orchestrator-backend -f
curl http://127.0.0.1:13001/health
```

## 5) 环境变量文件

脚本会生成：

- `/etc/agent_orchestrator/agent-orchestrator-backend.env`

可手动编辑后重启：

```bash
sudo systemctl restart agent-orchestrator-backend
```

## 6) HTTPS（推荐）

先保证域名解析到服务器，然后使用 certbot：

```bash
sudo apt-get update
sudo apt-get install -y certbot python3-certbot-nginx
sudo certbot --nginx -d your.domain.com
```
