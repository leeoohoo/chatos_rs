# Docker / Kata 沙箱实施方案

## 结论

当前阶段先把沙箱管理微服务稳定在两个后端：

```text
SandboxBackend
  ├── docker  # macOS / 本地开发默认后端
  └── kata    # Linux / 测试和生产候选后端
```

默认选择策略：

```text
SANDBOX_MANAGER_BACKEND 未配置或配置为 auto
  -> macOS: docker
  -> Linux: kata
  -> Windows / 其它系统: docker
```

仍然保留显式覆盖：

```env
SANDBOX_MANAGER_BACKEND=docker
SANDBOX_MANAGER_BACKEND=kata
SANDBOX_MANAGER_BACKEND=mock
```

`mock` 只用于接口和前端联调，不作为真实沙箱。

## 为什么先做 Docker / Kata

完整外部沙箱平台的固定成本较高，通常会包含 API、orchestrator、template builder、storage、logs、monitoring、调度系统等。对当前开发阶段来说，我们更需要低成本验证：

- 沙箱租约。
- 生命周期。
- 工作区副本。
- 文件和终端 agent。
- 健康检查。
- 资源限制。
- 自动销毁。

这些能力可以先用 Docker / Kata 在我们自己的微服务里完成。等业务闭环和安全策略成熟后，再决定是否接入更重的平台。

## 关键要求

沙箱要像一台临时 Linux 开发机，而不是只能执行一段命令的受限容器。

必须支持：

- `apt` / `apk` / `yum` 安装系统依赖。
- `npm` / `pnpm` / `yarn`。
- `pip` / `poetry` / `uv`。
- `rustup` / `cargo`。
- `go`。
- `java` / `maven` / `gradle`。
- 编译 C / C++ native addon。
- 运行 `chatos-sandbox-agent`。
- 长时间执行命令。
- 读写 `/workspace`。
- 网络策略控制。
- CPU / 内存 / 进程 / 磁盘限制。
- 任务结束后彻底销毁。

因此 Docker 和 Kata 后端都不再使用只读 rootfs。沙箱镜像自身的 overlay rootfs 必须可写，否则无法安装系统依赖和语言工具链。

## 推荐镜像

统一使用一个基础镜像：

```text
chatos-sandbox-agent:latest
  base: ubuntu:24.04 或 debian:12
  user: root for setup + optional non-root runtime user
  workdir: /workspace
```

建议预装：

- `bash`
- `curl`
- `wget`
- `git`
- `ca-certificates`
- `openssh-client`
- `build-essential`
- `pkg-config`
- `python3`
- `python3-pip`
- `uv`
- `nodejs`
- `npm`
- `pnpm`
- `rustup`
- `cargo`
- `golang`
- `openjdk`
- `chatos-sandbox-agent`

镜像入口：

```text
chatos-sandbox-agent --host 0.0.0.0 --port 49888 --workspace /workspace
```

当前仓库实现位置：

```text
sandbox_manager_service/sandbox_agent/Dockerfile
sandbox_manager_service/sandbox_agent/agent.py
```

构建命令：

```bash
docker build -t chatos-sandbox-agent:latest sandbox_manager_service/sandbox_agent
```

## Docker 后端

适用场景：

- macOS 本地开发。
- 前端和 API 联调。
- 低成本功能测试。
- 不承载真实多租户安全边界。

启动模型：

```text
sandbox_manager_service
  -> docker run
      -> chatos-sandbox-agent:latest
          -> /workspace
```

默认配置：

```env
SANDBOX_MANAGER_BACKEND=docker
SANDBOX_MANAGER_AGENT_PORT=49888
SANDBOX_MANAGER_DOCKER_IMAGE=chatos-sandbox-agent:latest
SANDBOX_MANAGER_DOCKER_NETWORK=bridge
```

Docker 后端保留资源限制：

- `--cpus`
- `--memory`
- `--pids-limit`
- `--network`
- workspace volume
- agent 端口随机映射到 `127.0.0.1`
- no privileged

为了支持安装依赖，Docker 后端不使用：

- `--read-only`
- `--cap-drop ALL`

后续可以按生产策略再加细粒度 capability、seccomp、AppArmor、egress proxy。

## Kata 后端

适用场景：

- Linux/KVM 测试环境。
- 更接近生产的安全隔离。
- 每个沙箱拥有轻量 VM 边界。

推荐调用方式：

```text
sandbox_manager_service
  -> nerdctl run --runtime io.containerd.kata.v2
      -> Kata VM
          -> chatos-sandbox-agent
          -> /workspace
```

默认配置：

```env
SANDBOX_MANAGER_BACKEND=kata
SANDBOX_MANAGER_AGENT_PORT=49888
SANDBOX_MANAGER_KATA_CONTAINER_CLI=nerdctl
SANDBOX_MANAGER_KATA_RUNTIME=io.containerd.kata.v2
SANDBOX_MANAGER_KATA_IMAGE=chatos-sandbox-agent:latest
SANDBOX_MANAGER_KATA_NETWORK=bridge
```

如果 Linux 节点使用 Docker 集成 Kata，也可以改为：

```env
SANDBOX_MANAGER_KATA_CONTAINER_CLI=docker
SANDBOX_MANAGER_KATA_RUNTIME=kata-runtime
```

Kata 节点启动前必须通过：

```bash
uname -a
ls -l /dev/kvm
egrep -c '(vmx|svm)' /proc/cpuinfo
sudo kata-runtime check
nerdctl run --rm --runtime io.containerd.kata.v2 ubuntu:24.04 uname -a
```

当网络模式不是 `none` 时，Kata 后端会把 sandbox 内 `SANDBOX_MANAGER_AGENT_PORT` 随机映射到宿主机 `127.0.0.1`，并把解析出的 `agent_endpoint` 写入租约记录。这样多个沙箱不会抢同一个宿主机端口。

## 自动选择策略

配置层负责选择真实 backend：

```text
SANDBOX_MANAGER_BACKEND=auto 或未配置
  -> std::env::consts::OS == "linux"  => kata
  -> std::env::consts::OS == "macos"  => docker
  -> std::env::consts::OS == "windows" => docker
```

这样不依赖启动脚本。无论通过 `cargo run`、systemd、Docker Compose 还是开发脚本启动，选择逻辑都一致。

## 工作区模型

任务执行不能直接挂载真实项目目录。

```text
真实项目目录
  -> 复制到 .chatos/sandboxes/runs/{run_id}/input/workspace
  -> 挂载到 sandbox /workspace
  -> AI 文件和终端操作只作用于 /workspace
  -> 任务结束后导出到 .chatos/sandboxes/runs/{run_id}/output/workspace
  -> 生成 diff
  -> 用户确认后再应用到真实项目
```

当前阶段先实现启动和销毁，不直接回写真实项目。

## 网络策略

为了支持安装任意代码环境，开发默认允许 `bridge` 网络。但这不是最终生产策略。

生产建议：

- 默认禁止访问内网网段。
- 禁止访问云 metadata：`169.254.169.254`。
- 出网走 egress proxy。
- 域名 allowlist / denylist。
- 记录包管理器下载域名。
- 给每个 sandbox 设置带宽和连接数限制。

## 安全边界

Docker 后端：

- 只用于开发和低风险测试。
- 不用于真实多租户用户数据。

Kata 后端：

- 作为生产候选方案。
- 仍然需要配合 Linux/KVM、节点隔离、网络策略、镜像签名、审计日志和自动回收。

不允许：

- `--privileged`
- 挂载宿主机 `/`
- 挂载 Docker socket。
- 挂载真实项目目录。
- 注入平台长期密钥。
- sandbox 间复用同一个可写 workspace。

## 实施步骤

### 第一阶段：后端能力

- 删除旧的外部沙箱平台 adapter 和 backend。
- 保留 Docker backend。
- 新增 Kata backend。
- 默认 `auto` backend：macOS Docker，Linux Kata。
- Docker / Kata 都使用可写 rootfs。
- 系统配置接口展示 Docker / Kata 配置。
- 前端配置页展示 Kata runtime。

### 第二阶段：沙箱镜像

- 构建 `chatos-sandbox-agent:latest`。
- 内置常见语言环境。
- 启动 `chatos-sandbox-agent`。
- 暴露 `/health`。
- 支持 terminal exec、file read/write、file list。

### 第三阶段：Linux/Kata 验证

- 准备 Linux/KVM 节点。
- 安装 containerd、nerdctl、Kata Containers。
- 验证 `kata-runtime check`。
- 验证 `nerdctl run --runtime io.containerd.kata.v2`。
- 启动 `sandbox_manager_service`，确认默认 backend 为 `kata`。
- 创建沙箱、健康检查、销毁。

### 第四阶段：安全增强

- egress proxy。
- metadata IP 阻断。
- workspace 大小限制。
- 命令审计。
- 沙箱超时回收。
- 镜像签名和 SBOM。
- 节点级别监控和告警。

## 验收标准

macOS：

- 不配置 `SANDBOX_MANAGER_BACKEND` 时，系统配置显示 `docker`。
- 可以通过 Docker backend 创建沙箱。
- sandbox rootfs 可写，可以安装依赖。
- 可以销毁沙箱。

Linux：

- 不配置 `SANDBOX_MANAGER_BACKEND` 时，系统配置显示 `kata`。
- `kata-runtime check` 通过。
- 可以通过 Kata backend 创建沙箱。
- sandbox rootfs 可写，可以安装依赖。
- 可以销毁沙箱。

通用：

- `POST /api/sandboxes/leases` 返回 `backend_id`。
- `GET /api/sandboxes/:sandbox_id/health` 能检查后端实例和 workspace。
- `DELETE /api/sandboxes/:sandbox_id` 可以销毁后端实例。
- 前端配置页能看到当前 backend、Docker image、Kata runtime。
