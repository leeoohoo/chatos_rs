# Windows React 与 macOS 功能拉齐关闭清单

更新时间：2026-10-25

分支：`3.0.9.3.1`

Windows 产品版本：`3.0.9`

本清单区分“页面已出现”“业务闭环已完成”“平台能力真实可用”和“发布环境已验证”。只有四项同时满足，才算完成；不再把只读占位、失败关闭或测试未运行写成已拉齐。

## A. 本轮已关闭

- [x] 31 个 macOS 一级页面均有 Windows React 路由和对应业务入口；默认登录后进入用户消息，而不是设置页。
- [x] Local Agent Host 打包资源完成真实 v46 健康握手，并通过账号隔离业务请求测试；此前两个 ignored 集成测试已实际运行通过。
- [x] Story H.264 实际尾帧回归修复。旧 `16×16` 夹具低于 Windows Media Foundation H.264 有效尺寸，已替换为 `64×64` Baseline/YUV420P 红到蓝视频，继续断言最后一帧为蓝色。
- [x] Windows Rust 条件编译警告收口，避免发布测试输出掩盖新问题。
- [x] Windows 客户端、Cargo、Tauri 和 npm 产品版本从占位的 `0.1.0` 统一为 `3.0.9`。
- [x] Windows Computer Use Worker 的双架构构建规则和打包校验通过；x64 Worker 已真实启动并返回有效诊断。

## B. 本次交付前必须关闭

- [x] Windows React 前端全量测试、类型检查、生产构建、架构审计和 Bundle 审计全部通过（132 个测试文件、489 项测试）。
- [x] Windows Tauri Rust 全量测试、两项真实 sidecar 集成测试、Rustfmt 和 Clippy 全部通过（434 项常规测试 + 2 项真实 sidecar 测试）。
- [x] 重新生成版本为 `3.0.9` 的 Release EXE、MSI、NSIS，并记录 SHA-256：
  - `chatos-windows-react.exe`：`a351f0ed86f6dffd6919d0d815f629c96e90120bf99decd918505e81ae2940bb`
  - `ChatOS_3.0.9_x64_en-US.msi`：`d0bcb2dc7fd9681149c26e10d84237bf870e444159437a2f75a03e9e705f13a4`
  - `ChatOS_3.0.9_x64-setup.exe`：`03d23c9b0bc33306ef0a180485356483d9889e4f59d3b802a444b3d49a45ad7c`
- [x] 用新 NSIS 安装到全新临时目录，验证主程序、Host、资源、启动、正常退出、卸载和残留清理；20 秒门限连续两次通过，最终报告为 `%TEMP%\chatos-windows-install-smoke-3.0.9-final.json`。
- [x] 启动新 Release 原生 EXE，并从 Windows 进程信息确认运行目标为 `target-shared\release\chatos-windows-react.exe`，不是 Vite 或浏览器进程。
- [ ] 在新 Release 原生窗口中确认登录后的默认页仍为用户消息。代码默认状态与自动回归均为 workspace/首个消息资源，但原生窗口视觉取证仍受下一项工具故障阻塞。
- [ ] 按 31 张 macOS 截图完成原生窗口逐页视觉走查。当前 Computer Use 原生管道不可用，需在工具恢复后补验；不能用浏览器截图替代。

## C. 仍存在的真实功能差距

### P1：用户能直接感知

- [ ] Computer Use / 屏幕采集的受信发布闭环。Windows Worker 源码、x64/arm64 构建和 smoke 已通过，但当前用户 Plugin 目录为空；还需产出跨平台 `.tgz`、进入签名 Marketplace Release，并从 Windows 客户端完成安装、启用、`doctor`、屏幕采集和输入实测。
- [ ] 访问控制的文件与网络隔离。当前 Windows 后端只真实提供账号/项目边界、审批前置校验和 Job Object 进程树回收；没有操作系统级文件沙箱或网络隔离，因此第 31 页只能只读，尚未达到 macOS 截图中的可编辑策略与“文件隔离/网络隔离已就绪”。
- [ ] Plugin 手动本地导入。签名 Marketplace 安装/更新已经实现；任意本地包导入仍关闭，缺少与 macOS 等价的本地信任确认、完整性记录和安全回滚链。
- [ ] Plugin 的 skills-only 与 Secret 支持。Windows Host 安装模型目前要求可执行 stdio component；纯 Skills 包和含 `environment_secret_refs` 的 Plugin 会失败关闭。
- [ ] Plugin 应用兼容面。Windows 当前只准入 `local_http` UI，并且宿主 bridge 只实现 `host.context.read`；静态 Plugin 页面及其他声明式宿主能力尚未拉齐。

### P2：平台/发布质量

- [ ] Windows 安装包代码签名、发布证书、签名校验和 SmartScreen 信誉验证。仓库可生成安装包，但本机未配置正式签名身份。
- [ ] 从旧版安装包到 `3.0.9` 的覆盖升级、数据迁移、失败回滚和降级演练；当前 smoke 只覆盖干净安装与卸载。
- [ ] 固定分辨率与 100%/125%/150% 缩放的原生截图对照、键盘导航、焦点顺序、屏幕阅读器标签和对比度验收。
- [ ] 冷启动、Host ready、首屏会话、万级消息、大目录、Git Diff、终端大输出和 8 小时运行的 Windows 性能/稳定性基线。

## D. 已确认的平台差异，不伪装为缺陷已修复

- 宠物浮窗当前使用 Windows 置顶窗口；Tauri 公共 API 不提供 macOS 同等的“固定到所有虚拟桌面”。
- Windows 设备网关使用账号认证控制面与本地 Rust Host，不宣称具有 macOS 设备签名长连接 Relay。
- Windows 文件访问遵循当前账号 ACL 和用户显式授权工作区，不存在 macOS“完全磁盘访问”的同名总开关。

## E. 执行顺序

1. 关闭 B 组构建、测试、安装和原生启动门禁。
2. 完成 Computer Use 签名发布/安装闭环。
3. 设计并实现 Windows 文件/网络隔离后，再开放第 31 页策略编辑。
4. 依次补 Plugin 手动导入、skills-only/Secret、静态 UI/bridge 能力。
5. 完成签名、升级回滚、缩放/无障碍和性能稳定性验收。
