# Harness 构建发布说明

本文档记录 Chatos RS 使用 Harness 构建镜像、发布服务，以及只选择部分服务构建发布的常用步骤。

## 前置要求

1. 本地代码已经提交，工作区尽量保持干净。
2. 服务器上的 Harness 可访问：`http://8.155.171.124:3001`。
3. 服务器部署目录为：`/opt/chatos/docker-deploy/chatos_rs`。
4. 生产密钥、SMTP 授权码等只放服务器 `docker/.env` 或 Harness secret，不写入仓库。

发布前建议先确认当前分支和改动：

```bash
git status --short --branch
git log --oneline -5
```

## 可构建服务名

查看当前支持的镜像服务名：

```bash
bash docker/deploy.sh build-services
```

当前常用服务名：

```text
sandbox-agent-image
user-service-backend
memory-engine-backend
project-management-backend
plugin-management-backend
local-connector-service-backend
sandbox-manager-backend
task-runner-backend
chatos-backend
official-website-backend
chatos-frontend
user-service-frontend
memory-engine-frontend
project-management-frontend
plugin-management-frontend
task-runner-frontend
sandbox-manager-frontend
official-website-frontend
```

## 全量构建镜像

全量构建会在 Harness 里构建所有服务镜像，耗时较长。

```bash
export HARNESS_ADMIN_PASSWORD='<Harness admin password>'
export HARNESS_CI_BRANCH='2.0.4'
export HARNESS_CI_PIPELINE='chatos-rs-images'
export HARNESS_CI_CONFIG_PATH='.drone.images.yml'
export HARNESS_CI_SNAPSHOT_SCOPE='ci-files'
export HARNESS_CI_RUN='true'

bash ./scripts/build-images-on-harness.sh
```

说明：

- `HARNESS_CI_SNAPSHOT_SCOPE=ci-files` 适合正式发版，要求业务代码已经提交到当前 HEAD。
- 如果需要把未提交工作区也推到 Harness 试跑，可改成 `HARNESS_CI_SNAPSHOT_SCOPE=all`，但正式发版不建议这样做。
- `HARNESS_CI_RUN=false` 只更新 Harness 仓库和 pipeline，不触发构建。

## 只构建部分服务镜像

通过 `HARNESS_CI_IMAGE_SERVICES` 指定服务名，多个服务用空格分隔。

示例：只构建注册流程和 Chatos 主前端/后端相关镜像：

```bash
export HARNESS_ADMIN_PASSWORD='<Harness admin password>'
export HARNESS_CI_BRANCH='2.0.4'
export HARNESS_CI_PIPELINE='chatos-rs-images'
export HARNESS_CI_CONFIG_PATH='.drone.images.yml'
export HARNESS_CI_SNAPSHOT_SCOPE='ci-files'
export HARNESS_CI_IMAGE_SERVICES='user-service-backend user-service-frontend local-connector-service-backend chatos-backend chatos-frontend'
export HARNESS_CI_RUN='true'

bash ./scripts/build-images-on-harness.sh
```

在 Windows PowerShell 调用 WSL `bash -lc` 时，空格需要转义，示例：

```powershell
bash -lc 'HARNESS_ADMIN_PASSWORD=<HarnessAdminPassword> HARNESS_CI_SNAPSHOT_SCOPE=ci-files HARNESS_CI_BRANCH=2.0.4 HARNESS_CI_PIPELINE=chatos-rs-images HARNESS_CI_CONFIG_PATH=.drone.images.yml HARNESS_CI_IMAGE_SERVICES=user-service-backend\ user-service-frontend\ local-connector-service-backend\ chatos-backend\ chatos-frontend HARNESS_CI_RUN=true ./scripts/build-images-on-harness.sh'
```

构建完成后，Harness 输出会包含 execution URL，例如：

```text
Harness execution: http://8.155.171.124:3001/chatos-ci/chatos-rs/pipelines/chatos-rs-images/execution/13
```

## 全量发布

镜像构建成功后，在服务器上执行：

```bash
cd /opt/chatos/docker-deploy/chatos_rs
export CHATOS_IMAGE_TAG=harness-ci
export CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
./docker/deploy-harness-ci.sh
```

这会校验所有 Harness CI 镜像都存在，然后使用本机 `harness-ci` 镜像启动完整服务栈。

## 只发布部分服务

如果只构建了部分镜像，发布时也传同一批服务名：

```bash
cd /opt/chatos/docker-deploy/chatos_rs
export CHATOS_IMAGE_TAG=harness-ci
export CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
./docker/deploy-harness-ci.sh user-service-backend user-service-frontend local-connector-service-backend chatos-backend chatos-frontend
```

脚本会先检查这些服务对应的本地镜像是否存在，再只重建并启动指定容器。未指定的服务不会重新构建。

## 常用发布组合

注册、用户、local connector 登录相关：

```bash
export HARNESS_CI_IMAGE_SERVICES='user-service-backend user-service-frontend local-connector-service-backend chatos-backend chatos-frontend'
bash ./scripts/build-images-on-harness.sh

ssh root@8.155.171.124
cd /opt/chatos/docker-deploy/chatos_rs
export CHATOS_IMAGE_TAG=harness-ci
export CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
./docker/deploy-harness-ci.sh user-service-backend user-service-frontend local-connector-service-backend chatos-backend chatos-frontend
```

只改 Chatos 主应用：

```bash
export HARNESS_CI_IMAGE_SERVICES='chatos-backend chatos-frontend'
bash ./scripts/build-images-on-harness.sh

ssh root@8.155.171.124
cd /opt/chatos/docker-deploy/chatos_rs
export CHATOS_IMAGE_TAG=harness-ci
export CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
./docker/deploy-harness-ci.sh chatos-backend chatos-frontend
```

只改官网：

```bash
export HARNESS_CI_IMAGE_SERVICES='official-website-backend official-website-frontend'
bash ./scripts/build-images-on-harness.sh

ssh root@8.155.171.124
cd /opt/chatos/docker-deploy/chatos_rs
export CHATOS_IMAGE_TAG=harness-ci
export CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
./docker/deploy-harness-ci.sh official-website-backend official-website-frontend
```

## 发布后验证

服务器上检查容器状态：

```bash
cd /opt/chatos/docker-deploy/chatos_rs
./docker/deploy.sh ps
docker ps --format 'table {{.Names}}\t{{.Image}}\t{{.Status}}'
```

常用 health 检查：

```bash
curl -fsS http://127.0.0.1:3997/health
curl -fsS http://127.0.0.1:39190/api/health
curl -fsS http://127.0.0.1:39230/api/health
curl -k -fsS https://local-connector.jgoool.com/api/health
curl -k -sS -o /dev/null -w '%{http_code} %{content_type}\n' https://app.jgoool.com/
curl -k -sS -o /dev/null -w '%{http_code} %{content_type}\n' https://www.jgoool.com/
```

验证部分路由是否可达时，可以用无效参数做 smoke test，返回 `400` 也代表路由已经到达后端：

```bash
curl -k -sS -o /dev/null -w '%{http_code}\n' \
  -H 'Content-Type: application/json' \
  -d '{"email":"invalid@example.com","invite_code":"BAD"}' \
  https://local-connector.jgoool.com/api/auth/register/send-code
```

## 常见问题

如果 Harness 里提示有未提交文件未进入 CI mirror，先执行：

```bash
git status --short
```

正式发版应先提交业务改动，再重新触发 Harness 构建。不要依赖 `HARNESS_CI_SNAPSHOT_SCOPE=all` 发布临时工作区。

如果发布时提示本地镜像不存在，说明服务器还没有成功构建对应的 `harness-ci` 镜像。先重新跑对应服务的 Harness 构建，再执行 `docker/deploy-harness-ci.sh`。

如果只想确认服务器本地镜像是否存在，不发布：

```bash
cd /opt/chatos/docker-deploy/chatos_rs
export CHATOS_IMAGE_TAG=harness-ci
export CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
./docker/deploy-harness-ci.sh check-images user-service-backend chatos-backend
```
