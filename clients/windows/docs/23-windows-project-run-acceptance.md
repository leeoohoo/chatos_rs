# Windows 项目 Run 验收

## 2026-10-16 17:30–18:20：真实 npm 正常、失败、停止、删除与重启

证据目录：`.local/parity/2026-10-16-1730/`。

### macOS 对照与真实配置

- 对照 macOS 项目运行入口的目标、工作目录、环境检查、独立实例和日志语义；Windows 继续使用本机分析器与原生 Run 页面。
- 在隔离项目 `C:\Users\17986\Windows-Project-Parity-20261016-1645` 中通过真实 Windows 文件页创建、编辑并保存 `package.json`，没有用测试替身替代页面操作。
- 重新分析后识别 `Node · npm start` 与 `Node · npm test`，工作目录精确为隔离项目根，Node.js 与 npm 工具链均来自 `C:\Program Files\nodejs`，预检无阻塞问题。

### 实机缺陷与修复

- 首次 `npm start` 立即失败，日志为 `Missing script: "\"start\""`。分析器已把 npm 脚本名限制为 ASCII 字母数字、冒号、下划线和连字符，因此不需要再插入 shell 引号；改为 `npm run start/test` 后真实启动成功。
- 点击停止后旧实现把被终止进程的非零退出码映射为 `failed`。现记录主动停止意图，终态稳定为 `stopped`；正常退出码 0 仍为 `exited`，非主动非零退出仍为 `failed`。
- 更严重的是旧 `Process.Kill(entireProcessTree: true)` 只结束外层 cmd/npm，留下持续输出的孤儿 Node。运行实例现在使用带 `KILL_ON_JOB_CLOSE` 的 Windows Job Object；停止、删除、Run 服务释放和客户端退出都会回收整棵子进程树。退出前先结束 job 并等待重定向输出排空，退出标记保持为日志最后一行。

### 真实 UI 与生命周期矩阵

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 修复前 npm 启动 | 目标被识别但 `npm run "start"` 以缺失脚本失败 | `18-running-log-expanded.png` |
| 正常启动与实时日志 | `npm run start` 进入 running，显示唯一 `RUN-PARITY-START-20261016-1730` 和持续 tick | `38-job-running-log.png` |
| 主动停止 | 状态为 `stopped`，退出标记为最后一行；3 秒后标记 Node 进程为 0 | `40-job-stopped-log.png`、`job-stop-log-assertions.json` |
| 真实失败 | `npm run test` 显示 `failed`、唯一 stderr 标记和退出码 7，无遗留进程 | `44-failure-log-expanded.png`、`failure-assertions.json` |
| 删除实例 | 逐个删除失败与停止实例后显示“还没有运行实例” | `45-failure-deleted`、`46-all-instances-deleted.png` |
| 运行中关闭客户端 | 关闭前存在唯一标记 Node；约 425 ms 后 Desktop、Host、Node 均为 0 | `running-before-close.json`、`48-running-app-exit-result.json` |
| 完整重启 | 恢复认证、Host、项目、两个目标、配置和可用启动按钮；实例列表为空且无标记 Node | `51-final-restart-run.png`、`final-restart-assertions.json` |

### 自动化、构建与范围

- Run 专项 9/9 通过；除 npm 命令无字面引号和主动停止终态回归外，新增真实 PowerShell → `ping.exe` 后代进程的 Job Object 终止回归，执行后无 `ping.exe` 遗留。
- 最终常规 Core 27、API 46、Presentation 856、Connector 562、NetworkGuard 19，共 **1,510 通过、0 失败**；package、TRX 与 acceptance evidence verifier 通过。
- SDK 8.0.426 Rust Host/x64 Debug 构建 **0 警告、0 错误**。
- 本轮闭环本地项目 Run 配置、正常/失败启动、实时日志、工作目录、停止、删除、进程树清理和重启。终端 UI 模式的交互式程序、权限拒绝和自定义环境变量错误仍可在设置/权限综合矩阵继续扩展，但不阻塞本行所列 Run 生命周期。
