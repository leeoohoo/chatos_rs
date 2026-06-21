# WSL Rust 开发流

这份说明对应当前仓库在 Windows 上被 `Smart App Control / Code Integrity` 拦截 Rust 运行产物时的推荐开发路径。

## 为什么需要 WSL

当前这台 Windows 机器上已经确认存在如下限制：

- `cargo check` 可以通过
- `cargo run` / `cargo test` 会在执行 Rust 生成的 `build-script-build.exe`、`proc-macro`、`time_macros*.dll` 时被系统策略拦截
- 事件日志显示拦截来源是 `Code Integrity` / `Smart App Control`

这不是 Rust 环境缺失，而是 Windows 执行策略阻止了 Rust 构建产物加载和执行。

## 推荐方案

把 Rust 后端运行到 WSL 里，Windows 只负责发起命令。

仓库内已经提供：

- Windows 侧调度脚本：[scripts/chatos-wsl.ps1](./scripts/chatos-wsl.ps1)
- WSL 内依赖初始化脚本：[scripts/bootstrap-wsl-dev.sh](./scripts/bootstrap-wsl-dev.sh)

## 一次性初始化

1. 安装一个 WSL 发行版，例如 Ubuntu

```powershell
wsl.exe --install -d Ubuntu
```

2. 如果系统要求，重启 Windows

3. 首次打开 Ubuntu，完成用户名/密码初始化

4. 在仓库根目录执行：

```powershell
make bootstrap-wsl
```

这个步骤会在 WSL 内安装：

- `build-essential`
- `pkg-config`
- `libssl-dev`
- `sqlite3`
- `libsqlite3-dev`
- `nodejs`
- `npm`
- `rustup` / `cargo`

## 从 Windows 启动服务

启动主服务：

```powershell
make restart-wsl
```

查看状态：

```powershell
make status-wsl
```

停止：

```powershell
make stop-wsl
```

只启动统一用户服务：

```powershell
make restart-user-service-wsl
make status-user-service-wsl
make stop-user-service-wsl
```

## 等价直接命令

不走 `make` 时，也可以直接执行：

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target main
powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action status -Target main
powershell.exe -ExecutionPolicy Bypass -File scripts/chatos-wsl.ps1 -Action restart -Target user-service
```

## 可选配置

可在根目录 `.env` 中设置：

- `WSL_DEV_DISTRO`
  说明：指定要使用的 WSL 发行版名称；不填则默认取第一项已安装发行版
- `WSL_CARGO_TARGET_DIR`
  说明：覆盖 WSL 内 Cargo target 目录；默认是 `$HOME/.cache/chatos_rs/<repo-hash>/cargo-target`
- `WSL_RUNTIME_DIR`
  说明：覆盖主服务运行日志目录
- `WSL_USER_SERVICE_RUNTIME_DIR`
  说明：覆盖 `user_service` 运行日志目录

## 设计说明

- 现有根级 `restart_services.sh` 已支持外部传入 `CARGO_TARGET_DIR`
- WSL 调度脚本默认把 Rust target 放到 Linux 用户目录下，而不是继续复用 Windows 侧 `target-shared`
- 这样可以避免把 Linux 构建产物和 Windows 构建产物混在一起，也能减少 DrvFS 上的构建开销

## 备注

- 如果当前机器还没有安装任何 WSL 发行版，`scripts/chatos-wsl.ps1` 会直接报出安装提示
- 如果你后面要做 Docker、本地 Rust 调试、或完整 `cargo test`，优先在 WSL 里执行
