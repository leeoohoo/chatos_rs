# Linux Docker 一键部署

此压缩包包含当前工作区源码，但不包含 Git 元数据、本机密钥、依赖缓存和编译产物。Linux 服务器不需要安装 Rust、Node.js、PostgreSQL 或前端工具链；这些构建和运行依赖均由 Docker 镜像提供。

## 最短启动方式

服务器需要预先安装：

- Docker Engine
- Docker Compose v2（命令为 `docker compose`）
- Bash
- OpenSSL

解压并进入目录后只运行：

```bash
bash deploy-linux.sh
```

脚本首次运行会从 `docker/bootstrap.conf.example` 创建权限为 `600` 的 `docker/bootstrap.conf`，生成本地 mTLS 材料，在容器中从当前源码构建镜像，然后后台启动完整服务。

常用入口：

- API 网关：`http://服务器地址:9080`
- 官网前端：`http://服务器地址:39251`
- Harness：`http://服务器地址:3000`
- Grafana：`http://服务器地址:3001`

查看状态和日志：

```bash
docker/deploy.sh ps
docker/deploy.sh logs
```

更新源码后重新构建并发布仍使用同一个命令：

```bash
bash deploy-linux.sh
```

停止服务（保留数据库数据）：

```bash
docker/deploy.sh down
```

## 服务器容量与首次构建

这是多服务 Rust/前端项目，首次构建会下载基础镜像和依赖，耗时取决于网络及 CPU。建议至少准备 4 核 CPU、8 GiB 内存和 40 GiB 可用磁盘；低配置机器可以运行，但首次构建会明显更慢。默认串行构建服务，以降低峰值内存占用。

如果 GHCR 中的预构建镜像已经包含你要发布的代码，可以把构建时间更短的发布方式改为：

```bash
docker/deploy.sh up
```

`up` 拉取 `docker/bootstrap.conf` 中配置的镜像；`deploy-linux.sh` 则始终构建压缩包里的当前源码，两者不要混淆。

## 公网或生产环境

首次自动生成的配置使用 `CHATOS_ENV=local` 和示例密码，只适合本机、测试机或受信任内网，不能直接暴露到公网。

生产发布前至少需要：

1. 编辑 `docker/bootstrap.conf`，设置 `CHATOS_ENV=production`。
2. 替换所有 `change_me_...`、`HARNESS_ADMIN_PASSWORD` 以及 `CONFIG_CENTER_*_CALLER_SIGNING_SECRET`。
3. 用 sha256 digest 固定 PostgreSQL 镜像。
4. 配置域名、HTTPS 反向代理、防火墙与备份。
5. 通过 Secret 系统提供各服务 mTLS 材料；生产模式不会自动生成证书。
6. 按 `INSTALL_GUIDE.zh-CN.md` 完成生产检查后，再执行 `bash deploy-linux.sh`。

注意：这个 Docker 栈部署的是 ChatOS 云端服务。macOS 和 Windows 原生客户端不能在 Linux 容器里运行，需要分别安装到用户电脑，并把客户端 API 地址指向这台服务器。
