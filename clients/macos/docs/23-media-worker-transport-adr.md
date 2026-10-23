# ADR 23：Media Worker 单用途子进程与继承管道

- 状态：已冻结并用于 P2 图片归一化与预览缩略图生产切片
- 日期：2026-10-23
- 适用版本：`media-worker-v1` / wire protocol v1

## 问题与证据

Story 图片归一化此前在主 App 内使用 ImageIO 解码和编码，视频尾帧则在主 App 内使用 AVFoundation 解码。源文件虽然已经文件化，恶意媒体、压缩炸弹、解码框架崩溃以及高像素工作集仍会直接影响 UI 进程。方案要求 P2 在编码前只冻结一种正式传输，并同时交付版本握手、父进程死亡、取消/超时、文件授权、打包和签名门禁。

当前客户端由 SwiftPM 直接产生 `ChatOSSwift` 可执行文件，再由 `package-app.sh` 组装 `Contents/MacOS`。仓库没有 XPC Service bundle target、独立 Info.plist、entitlements、audit-token 校验和 XPC 文件句柄交付基础；现有 App 也没有启用 App Sandbox。此时只把 executable 改名为 XPC Service 不会自动获得可信身份或沙箱边界。

## 评估的选项

1. XPC Service：生命周期与 macOS 集成最好，也可在完整签名、requirement 和 audit token 基础上形成更强的对端身份；但本阶段缺少真实 bundle/sandbox/entitlements 与文件句柄链路，仓促接入会同时改变构建系统、文件授权和解码位置。
2. 常驻 authenticated local socket：适合后续 durable media job，但需要私有端点、peer credential、连接缓冲、心跳、恢复和端点清理；对当前单次纯函数式 PNG 归一化过重。
3. 单用途直接子进程 + inherited stdio：连接能力来自 App 直接创建且不对其他本机进程暴露；一次请求一次进程，崩溃边界清楚，没有常驻监听端点和跨请求状态。

## 决定

P2 媒体解码垂直切片只采用选项 3，不并行维护 XPC 或 socket 实现。`ChatOSMediaWorker` 是放在 `Contents/MacOS` 的独立 arm64 executable；App 通过匿名 stdin/stdout 管道先完成 hello，再提交一个 `normalize_png_v1` 或 `extract_last_video_frame_png_v1` 请求，收到单个终态响应后进程退出。Story 最终图片与 UI 缩略图复用同一图片操作和安全边界，只通过不同的输出像素上限区分；视频操作固定提取尾帧，不接受任意时间或任意输出格式。

协议规则：

- hello 协商 min/max protocol、`media-worker-v1` build contract、capabilities 和 512 KiB inline 上限；握手前不接受业务请求。
- 每个请求包含 request/account session/session epoch/helper instance、deadline、operation 和小型文件 descriptor 元数据；媒体字节禁止进入 JSON。
- Worker 不接收 API secret、不联网、不拥有 UI/Story 元数据，也不打开 Local Agent 数据库。
- App 预创建 mode `0600` 的 lease-owned 输出文件。请求携带输入/输出的 device、inode 和 byte count；Worker 使用 `O_NOFOLLOW` 打开后 `fstat` 复核普通文件、owner、大小、单链接输出和文件身份。路径替换、symlink、同一输入输出均 fail-closed。
- 图片输入最多 20 MiB/64 MP，归一化长边最多 8192 px；视频输入最多 512 MiB，尾帧长边最多 4096 px；PNG 输出最多 20 MiB。ImageIO/AVFoundation 解码、PNG 编码、同步和 SHA-256 都在 Worker 内执行；App 只持有文件 artifact 和小型 descriptor。

## 生命周期、监督与失败

- Worker 启动参数包含直接父 PID，并要求 `getppid()` 一致；kqueue/Dispatch process source 监控父进程退出，父进程死亡时 Worker 立即 `_exit`。PID payload 本身不被当作远程身份凭证，真正能力是 App 创建的匿名管道和直接父子关系。
- App 取消或 deadline 到期时先发 `SIGTERM`，250 ms 后仍存活则 `SIGKILL`。不可中断 ImageIO 不会拖住 App；输出 artifact 未完成时由既有 lease/orphan 协议删除或启动式回收。
- 一次请求不会在崩溃后自动重放。新的 Worker launch 在异常后按 1/2/4/8/16/30 秒退避；5 分钟内 5 次异常时隔离 30 秒。业务级 invalid-image 拒绝不计为 Worker 崩溃。
- Helper 不兼容、缺失或签名包不完整时明确失败，不把未经隔离的高风险解码自动退回主进程。回滚通过回滚整个 App 包执行，不同时维护第二套正式 IPC。

## 打包、签名与安全边界

`package-app.sh` 单独构建 Worker、生成 Release dSYM、复制到 `Contents/MacOS`，从内到外先签 Worker 再签 App。`verify-app-build.sh` 校验存在性、arm64-only、Release 无 Debug 路径、嵌套签名、整个 bundle 深度签名和 build-evidence 文件哈希；无父能力直接启动必须失败。

这提供崩溃/内存隔离，不宣称普通子进程等价于 App Sandbox。Worker 继承同一用户权限，正式对外分发仍需在 P4 补 Developer ID、hardened runtime、公证以及是否升级为带 audit-token/requirement 校验的 XPC Service 的重新评估。只有在真实 sandbox/TCC、文件句柄和签名矩阵成立时才替换本 ADR；不能仅为形式统一同时保留两套传输。

## 验证与剩余范围

最小门禁包括：协议握手、真实独立进程归一化、symlink/文件替换拒绝、像素/字节/deadline、取消后子进程消失、Debug/Release 回归，以及打包后架构、签名和 build evidence 校验。

本 ADR 当前覆盖 Story PNG 归一化、Media Studio 缩略图/全屏预览、普通聊天和 Agent 聊天持久图片附件预览、项目文件 4K 图片预览、Agent 头像原图，以及 Story 视频尾帧提取。App 为显示或生成最终小图仍需解码 Worker 产生的受限尺寸 PNG，但不再在 UI 进程解析这些路径的原始 20 MiB/64 MP 图片；Story 尾帧则以 lease-owned 文件直接提交 Store，不回到主进程 `Data`。通用视频 poster/任意时间 frame、Markdown/Visual Session 等高频图片消费者、媒体上传/下载、durable media job 所有权、跨账户 durable lease、真实大媒体 physical-footprint、内存压力/取消风暴、ENOSPC/睡眠/断网和发布 soak 仍属于后续 P2/P4 门禁。
