# Windows React 与 macOS 功能拉齐关闭清单

更新时间：2026-10-26

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
- [x] Plugin 静态应用页面已准入：Windows 仅通过进程内 loopback 服务暴露清单声明的 HTML/静态资源，限制单文件与总大小，拒绝链接、越界和未声明资源，并保留 iframe sandbox、CSP、同源和 bridge 会话绑定。
- [x] Plugin 安装助手不再只是提示文案：专用 `main_chat` capability、精确联系人/Run/capability 复验以及 `plugin_installer_inspect_source/install_source` worker 已接通。本机 ChatOS Plugin 目录执行稳定内容指纹、显式安装意图、重新复制复验、逐文件完整性、Host 元数据事务与目录回滚；链接、reparse、特殊文件、越界/重叠目录、超限内容和未准入环境变量均失败关闭。
- [x] Plugin 安装助手的 Git 来源已闭环：只接受用户当前消息明确给出的无凭据 HTTPS URL 和安全 ref/subdirectory；使用隔离 bare fetch，禁用交互、Hook、文件协议和子模块，不 checkout 工作树或运行仓库脚本；archive、文件数、解包体积、路径和文件类型均有界，安装时重新获取并复验预检指纹后进入同一原子提交/回滚链。
- [x] Plugin 安装助手的固定版本 npm 来源已闭环：`search_npm/resolve_npm` 读取公共 Registry 的有界不可信元数据并固定精确语义版本；source 获取只接受 `registry.npmjs.org` HTTPS tarball，限制 512 MiB 并复验 sha512 integrity，不读取用户 npm 配置、凭据或运行生命周期脚本；安装时重新下载并复验内容指纹后进入同一原子提交/回滚链。
- [x] 普通 npm MCP 的 `inspect_npm/install_npm` 已闭环：使用隔离 npm 配置和私有 cache，禁用 lifecycle scripts/audit/fund，审计 v3 lockfile 的 Registry HTTPS + sha512 固定依赖；移除链接/shim、冻结 Node 运行时并生成 ChatOS 清单后，在 Job Object 中真实执行 initialize/tools/list。内容、启动参数、access profile 与完整工具 schema 共同形成指纹，安装时全部重做并复验后进入 Host 元数据事务和目录回滚链；公网官方 MCP Everything 已完成预检与重复安装 smoke。
- [x] 常见 stdio MCP JSON 兼容导入已接入 source 指纹与原子安装链：支持 `mcp.json`、`.mcp.json`、Cursor、VS Code、Gemini、Claude Desktop、Cline 和 OpenCode 布局；只准入可冻结的 Node 或包内 Windows `.exe`、空环境变量和 stdio transport，转换只发生在隔离 staging，不修改用户源目录。`npx`/shell、远端 transport、明文 Secret 和越界命令均失败关闭。
- [x] portable Agent Plugin、Codex Plugin、Claude Plugin、npm Agent Plugin 与独立 `SKILL.md` 已接入同一隔离 staging、内容指纹、原子安装和回滚链。根 Skill 会安全归入 `skills/<name>` 并携带 `scripts/references/assets`；纯 Skills 包使用完整性保护的 `skills-only` Host 记录，Agent runtime 不启动进程，只有 `capability_describe` 已披露的 Skill 才能激活并分页读取资源。
- [x] Plugin 设置页已开放本机目录 typed 导入：React 只持有原生选择句柄与目录名，不接收绝对路径；预检显示清单、MCP/Skill 组件和内容指纹，确认安装时由原生端复验账号、页面、会话代次、选择租约及完全相同的指纹，再进入既有原子安装和回滚链。Git/npm 来源仍由安装助手处理。
- [x] Plugin Secret 已完成 Windows 原生闭环：`${credential:<name>}` 按账号、Plugin 与 Secret 名生成不透明 Host 引用，明文只写入 Windows Credential Manager；设置页仅投影 Secret 名称与配置状态，已有值永不回读。Agent 启动前会重新核对清单、Host 绑定与安装完整性，再把明文短暂注入目标进程环境；缺值、跨账号、引用篡改或清单漂移均失败关闭，卸载会同步清理当前清单声明的凭据。兼容 MCP JSON 仍拒绝环境变量，避免把无法区分的明文 Secret 导入系统。
- [x] 访问控制页的“审批策略/审批人”已接入现有真实命令门禁：按需审批、审批模型和完全控制分别映射到 `request_approval`、`auto_approval`、`full_control`；自动审批与完全控制必须二次确认，自动审批仍由 Rust 后端强制要求已配置审批模型。文件权限、网络访问和本机权限总开关继续锁定，并明确标注尚无操作系统级隔离。
- [x] Windows MSI 覆盖升级闭环已实测：从仓库保留的 `0.1.0` 安装到隔离目录并使用隔离的真实 AppData，先用截断的 `3.0.9` MSI 验证失败前置不改变旧程序和用户数据哨兵，再完成 `0.1.0 → 3.0.9` Major Upgrade、两代原生启动退出、数据保留和卸载。`3.0.9` 起显式设置 `allowDowngrades=false`，Bundle 审计和 MSI `WIX_DOWNGRADE_DETECTED`/LaunchCondition 双重门禁防止本安装包覆盖更高版本。
- [x] Windows MSI 事务中途故障回滚闭环已实测：仓库内受审计 WiX 模板把 `RemoveExistingProducts` 固定在 `InstallExecute` 之后（执行序号 `6501`）；在复制新文件后注入 Type 19 故障，两轮均返回 `1603`，并完整恢复旧 ProductCode、旧 EXE 哈希和 AppData 哨兵。随后正常升级和卸载成功，旧版、故障版与当前版 ProductCode 均为未安装状态，主 EXE 组件无残留 client。
- [x] Windows MSI 在旧客户端保持原生窗口运行时的覆盖升级闭环已实测：两轮均由 Windows Installer 正常关闭 `0.1.0` 进程，升级返回 `0`，`3.0.9` 原生启动和最终卸载成功，AppData 哨兵保留；测试逐轮对比系统 `PendingFileRenameOperations`，确认未新增待重启文件操作。

## B. 本次交付前必须关闭

- [x] Windows React 前端全量测试、类型检查、生产构建、架构审计和 Bundle 审计全部通过（134 个测试文件、504 项测试）。
- [x] Windows Tauri Rust 全量测试、两项真实 sidecar 集成测试、一次公网 HTTPS Git 获取 smoke、一次公网 npm Registry 获取 smoke、一次公网 npm MCP 隔离预检与安装 smoke、一次兼容 MCP JSON 冻结 Node 预检与安装 smoke、Rustfmt 和 Clippy 全部通过（469 项常规测试 + 2 项真实 sidecar 测试 + 1 项 Git smoke + 1 项 npm Registry smoke + 1 项 npm MCP smoke + 1 项兼容 MCP JSON smoke）。
- [x] 重新生成版本为 `3.0.9` 的 Release EXE、MSI、NSIS，并记录 SHA-256：
  - `chatos-windows-react.exe`：`11ae151ca76d1e8cf8fee9e91dba4a95a28607f20a070e8c309846b3f03dd6e6`
  - `chatos_local_agent_host.exe`：`5f697fced2658ac59a3f9fb4ff5d167ac033545ed0f0c995363c754fcc0ebf66`
  - `ChatOS_3.0.9_x64_en-US.msi`：`86df36df9b1ea443245f948dcca4c3f9d1c294a7e873a335311daa0df6026424`
  - `ChatOS_3.0.9_x64-setup.exe`：`651e6d32151187955d20605b9f04b49fcf4e3f16bcbdd0bfd16ef982215a625d`
- [x] 用新 NSIS 安装到全新临时目录，验证主程序、Host、资源、启动、正常退出、卸载和残留清理；当前事务安全构建产物连续两次通过 20 秒门限，成功报告为 `%TEMP%\chatos-windows-nsis-clean-install-1.json` 和 `%TEMP%\chatos-windows-nsis-clean-install-2.json`。
- [x] MSI 升级 smoke 在既有四轮基础上，以事务安全模板再连续通过两轮；报告 `%TEMP%\chatos-windows-msi-upgrade-transactional-safe-1.json` 和 `%TEMP%\chatos-windows-msi-upgrade-transactional-safe-2.json` 同时记录 `0.1.0 → 3.0.9`、两代原生启动退出、截断包 `1620` 失败、事务中途故障 `1603` 回滚、AppData 哨兵保留、卸载以及降级检测规则。
- [x] MSI 运行中客户端升级 smoke 再连续通过两轮；报告 `%TEMP%\chatos-windows-msi-running-upgrade-1.json` 和 `%TEMP%\chatos-windows-msi-running-upgrade-2.json` 记录旧版原生窗口进程、Installer 正常关闭、无重启升级、新版原生启动、数据保留、卸载和系统待重启队列不变。
- [x] 启动新 Release 原生 EXE，并从 Windows 进程信息确认运行目标为 `target-shared\release\chatos-windows-react.exe`，不是 Vite 或浏览器进程。
- [ ] 在新 Release 原生窗口中确认登录后的默认页仍为用户消息。代码默认状态与自动回归均为 workspace/首个消息资源，但原生窗口视觉取证仍受下一项工具故障阻塞。
- [ ] 按 31 张 macOS 截图完成原生窗口逐页视觉走查。当前 Computer Use 原生管道不可用，需在工具恢复后补验；不能用浏览器截图替代。

## C. 仍存在的真实功能差距

### P1：用户能直接感知

- [ ] Computer Use / 屏幕采集的受信发布闭环。Windows Worker 源码、x64/arm64 构建和 smoke 已通过，但当前用户 Plugin 目录为空；还需产出跨平台 `.tgz`、进入签名 Marketplace Release，并从 Windows 客户端完成安装、启用、`doctor`、屏幕采集和输入实测。
- [ ] 访问控制的文件与网络隔离。当前 Windows 后端真实提供账号/项目边界、可编辑且实际生效的审批策略，以及 Job Object 进程树回收；没有操作系统级文件沙箱或网络隔离，因此第 31 页的文件、网络与总开关仍保持只读，尚未达到 macOS 截图中的完整可编辑能力。

### P2：平台/发布质量

- [ ] Windows 安装包代码签名、发布证书、签名校验和 SmartScreen 信誉验证。仓库可生成安装包，但本机未配置正式签名身份。
- [ ] Windows Installer 无法由 Restart Manager 正常关闭的强制文件锁及需重启场景仍需在一次性 Windows VM 中补验；客户端正常运行造成的文件占用、事务中途故障回滚均已关闭。本机已有其他软件留下的待重启文件操作，不能为了测试篡改系统队列。历史 `0.1.0` 安装包自身允许降级，已发布二进制无法由新版本追溯修改。
- [ ] 固定分辨率与 100%/125%/150% 缩放的原生截图对照、键盘导航、焦点顺序、屏幕阅读器标签和对比度验收。
- [ ] 冷启动、Host ready、首屏会话、万级消息、大目录、Git Diff、终端大输出和 8 小时运行的 Windows 性能/稳定性基线。

## D. 已确认的平台差异，不伪装为缺陷已修复

- 宠物浮窗当前使用 Windows 置顶窗口；Tauri 公共 API 不提供 macOS 同等的“固定到所有虚拟桌面”。
- Windows 设备网关使用账号认证控制面与本地 Rust Host，不宣称具有 macOS 设备签名长连接 Relay。
- Windows 文件访问遵循当前账号 ACL 和用户显式授权工作区，不存在 macOS“完全磁盘访问”的同名总开关。
- Plugin 应用静态页面与 `local_http` UI 已在 Windows 准入；Windows 和 macOS UI Host 目前都只真正实现 `host.context.read`。Manifest 中尚未闭环的 `artifact.list/read/download/create/update` 是两端共同待办，不列为 Windows 落后于 macOS 的差距。

## E. 执行顺序

1. [已完成] 关闭 B 组中不依赖 Computer Use 原生管道的构建、测试、重新打包、安装 smoke 和原生进程启动门禁。
2. [已完成] Plugin 安装助手专用 capability、受限平台工具与原生 ChatOS Plugin 本地目录 inspect/install 闭环。
3. [已完成] Git、固定版本 npm 来源、普通 npm MCP、常见 stdio MCP JSON、portable Agent、Codex、Claude、独立 Skill、skills-only、设置页本机目录 typed 导入与 Windows Credential Manager Secret resolver 均已完成。
4. [外部依赖] 完成 Computer Use `.tgz`、Marketplace 签名发布/客户端安装闭环，以及 Windows 安装包正式代码签名。
5. [待开始] 设计并实现 Windows 文件/网络隔离后，再开放第 31 页的文件、网络与总开关；审批策略和审批人已经接入真实执行门禁。
6. [待验证] 工具恢复后完成 31 页原生逐页视觉、缩放和无障碍验收；升级、失败前置保护、事务中途故障回滚及客户端运行中升级已完成，后续在一次性 VM 补强制锁/需重启场景及性能稳定性基线。
