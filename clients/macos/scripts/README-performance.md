# Release 构建证据与工程基准

所有命令从仓库根目录执行。工具不安装 App，不访问活动用户数据库；存储基准只创建、销毁合成临时库。

## 构建证据

```sh
CHATOS_BUILD_CONFIGURATION=release CHATOS_VERIFY_LAUNCH_SMOKE=0 \
  clients/macos/scripts/package-app.sh
```

正式 Release 仍要求干净工作区。若明确需要未提交源码的诊断包，可增加 `CHATOS_ALLOW_DIRTY_RELEASE=1`，但其证据分类固定为 `diagnostic-build`，不能当正式基线。`CHATOS_VERIFY_LAUNCH_SMOKE=0` 只打包、不启动 App；既有 `package-release-app.sh` 默认会做隔离启动 smoke，注意区分。

打包现在强制使用 Swift `Package.resolved` 和 Cargo `--locked`，并产出：

- `.build/release/ChatOS.app.build-manifest.json`：Release App、Host、工具及完整 bundle 文件清单/哈希、UUID、签名描述、版本、源码输入摘要、构建命令、工具链、协议/Schema 源码版本和环境。Debug 证据位于对应的 `.build/debug/`，不会覆盖 Release 产物。
- `.build/build-evidence.*/inputs.json`、`commands.json`：构建前输入及实际命令记录。tracked 文件、非 ignored 的 untracked 文件、缺失文件、可执行位、symlink 目标和 Git index 都参与校验。构建期间输入发生变化，拒绝发布证据。
- 同一 evidence 目录下的 Release App、Host、ps dSYM：逐个校验 UUID/架构是否与成品对应，并记录文件哈希。Release Rust 使用 `CARGO_PROFILE_RELEASE_DEBUG=1` 保留行表。

成品签名后才计算哈希，manifest 放在 bundle 外，避免自引用或破坏签名。`verify-app-build.sh` 同时检查签名和成品/符号清单；替换资源、Helper 或符号文件会失败。请保留对应 evidence 目录；当前符号清单记录本机构建路径，跨机器归档/迁移尚需单独处理。DMG 分发、公证、签名信任与符号归档发布不是本工具的完成项。

这些是本地关联证据，不是受签名保护的供应链证明，也不承诺 bit-for-bit 重建。ignored 输入、外部编译配置与工具链没有完整归档；包含源码 symlink 的构建保守分类为诊断构建。`clean-release-build` 只表示干净源码及记录的 Release 构建命令，不表示性能、签名公证或分发验收通过。

## 固定存储工程基准

```sh
zsh clients/macos/scripts/run-performance-baseline.sh
```

默认进行 3 轮 Release 测试，每轮内部重复 3 次，使用 native SwiftPM build system 与 `.build-native` 缓存。输出位于 `.build/performance/store-release-*/`，也可通过 `--output /tmp/新的目录` 指定尚不存在的目录。仓库内输出必须处于 ignored 目录，避免采集工具改变自己测量的输入。

`run.json` 保存逐轮原始数据、日志哈希、测试二进制哈希/UUID、供电与原始热状态信息、UTC 起止时间及所有样本合并的 nearest-rank p50/p95/p99；命令耗时（含构建）与操作计时分开。`inputs.json` 保存源码/工具链输入，Rust profile 为 null，因为该测试不执行 Rust。测试缺失、跳过、重复输出、返回失败、计数/样本异常、输入或测试产物在轮次间变化，都不能产出 completed 结果。失败/超时保留日志与 failed manifest；超时只终止本次运行器创建的进程组。

macOS 仅支持 arm64，不提供 Intel 或 universal 构建。统一入口使用 `/usr/bin/arch -arm64 /usr/bin/python3`，不再从 PATH 选择 Python；Swift/XCTest 也显式固定 arm64。Intel、Rosetta shell、无法确认 Apple Silicon 的环境直接拒绝，不回退 x86_64；成品验证要求 App、Host、ps、rg 全部为 arm64-only。manifest 分开记录本机架构与采集器架构。

夹具为 20 Agent、10 房间、500 消息、500 Run、100 Todo 和一份 128 KiB 附件。记录的是夹具规则/源码哈希：UUID/时间戳仍有变化，所以 `dataset_byte_sha256` 明确为 null，不伪称固定数据库文件。不能在一次运行期间修改源码，也不要并行运行使用同一 Swift scratch path 的打包/测试。

此基准没有测量 App 切页、首个可交互帧、主线程 hang、physical footprint、Host IPC 或媒体链路；小样本分位数不是发布 SLO 证明。应在固定机器、数据集、供电、热状态、缓存和负载下比较，不把不同运行环境直接合并。正式导航脚本、真实组合负载与干净提交的基线/候选对照仍待完成。

基线与候选都完成后，用回归门禁比较：

```sh
zsh clients/macos/scripts/compare-performance-baselines.sh \
  --baseline /绝对路径/baseline/run.json \
  --candidate /绝对路径/candidate/run.json \
  --output /绝对路径/comparison.json
```

比较器要求两份结果都完整、源码采集期间未变化，并且夹具规则、夹具源码、硬件、macOS、架构和供电来源一致。它检查全部存储指标的 P50/P95/P99；默认允许最多 15% 相对波动或 0.25 ms 绝对噪声（取较大者），超限返回非零状态并在 JSON 中列出失败项。阈值可显式覆盖，但不能用更宽阈值把不同机器或夹具强行合并。该门禁仍只代表存储工程基准，不会把它误标成完整 App 性能验收。

## 固定 App CPU 组合场景

```sh
zsh clients/macos/scripts/run-app-performance-scenario.sh
```

该入口使用 Release XCTest 和纯内存合成数据，并行执行 1,000-turn 时间线构建、真实 AppKit/SwiftUI 千行时间线的首次布局与滚动、100 项任务板 presentation、超过 1 MB 的 Markdown 后台解析以及超过 600,000 UTF-16 字符的代码预览回退。它验证千行时间线始终只构造可见行，同时测量 MainActor 最大调度间隙；默认执行三轮并把原始样本、环境、测试二进制身份、P50/P95/P99 与宽松安全上限写入 `.build/performance/app-scenario-release-*/run.json`。

此场景不会登录、打开活动账户数据库或控制当前 App，适合成为每次 Release 性能回归的快速 CPU/UI 虚拟化门禁。它不能替代真实账户窗口导航、滚动帧率、WindowServer、内存或进程组 soak；这些仍需在隔离 Release 实例上验收。

## App 性能 Signpost

Release 与 Debug 客户端都在 `com.chatos.native / Performance` 下发布以下稳定 interval，可在 Instruments 的 Time Profiler、SwiftUI 或 Logging 录制中对齐交互和主线程热点：

- `ConversationTimelineBuild`：消息/任务版本变化后的时间线快照构建。
- `MarkdownParse`、`MarkdownLayout`：Markdown 缓存未命中的后台解析与 AppKit 测量。
- `TodoPresentationBuild`：任务板轻量 Run presentation 构建。
- `PetFrameUpdate`：单个 CALayer 精灵帧提交。
- `ActivationRecoveryDispatch`：应用重新激活时幂等恢复任务的同步调度成本。
- `CodeHighlightScan`、`CodeHighlightApply`：后台语法扫描和主线程最小样式提交。

例如在不停止当前客户端的情况下附加 60 秒 Time Profiler：

```sh
xcrun xctrace record \
  --template 'Time Profiler' \
  --attach ChatOSSwift \
  --time-limit 60s \
  --output /tmp/ChatOS-performance.trace
```

录制期间按固定验证矩阵执行对应操作；结束后按上述 interval 过滤并同时检查 Hangs、主线程调用栈与 SwiftUI invalidation。Signpost 只提供精确边界，不会自动生成输入、判断 P95/P99 是否达标，也不替代固定机器上的基线/候选对照。

## 进程组资源曲线与 8 小时 soak

`run-process-group-soak.sh` 只观察一个明确的根 PID 及其后代，不启动、停止或控制 App，也不会搜索账户数据库。请只把它指向使用隔离数据目录的验收实例，不要把当前日常使用实例当成压测目标：

```sh
zsh clients/macos/scripts/run-process-group-soak.sh \
  --root-pid 12345 \
  --app /绝对路径/ChatOS.app \
  --build-manifest /绝对路径/ChatOS.app.build-manifest.json \
  --duration-seconds 28800 \
  --sample-interval-seconds 5 \
  --minimum-samples 5000 \
  --require-process ChatOSSwift \
  --require-process chatos_local_agent_host \
  --require-process ChatOSAgentWorker \
  --require-process ChatOSMediaWorker \
  --required-process-minimum-samples 100 \
  --temporary-root /绝对路径/isolated-runtime \
  --evaluate-acceptance \
  --max-group-physical-footprint-bytes 4294967296 \
  --max-cpu-percent-p95 400 \
  --max-disk-write-bytes-per-interval 268435456
```

原始样本写入 `samples.jsonl`，每项保留 PID/start-time 身份、进程树、physical footprint、resident size、累计 CPU、累计磁盘读写、FD 和内核线程；`run.json` 保存聚合 p50/p95/p99、首尾/峰值、增减步数和验收失败原因。显式临时目录使用 `lstat` 有界扫描，绝不跟随 symlink。正式验收模式同时校验 App 与构建 manifest 的完整清单，只接受 `clean-release-build`；并拒绝少于 8 小时、间隔大于 5 秒、少于 5,000 样本或没有明确资源上限/进程/临时目录的运行，短诊断不能生成 `passed`。

采集输出目录必须位于所有 `--temporary-root` 之外；否则采集器自己持续写入的 `samples.jsonl` 会被误计为客户端临时文件增长，入口会在开始采集前直接拒绝这种自干扰配置。

采集器不生成负载，也不能从内核线程数推断 Swift Task 或 Notification observer 数；200 次导航、账户循环、内部 Task/observer 计数与主线程 hang 仍须由后续 App 内验收驱动器提供。Plugin 是动态名称，正式命令应额外加入该验收插件的真实 executable 名称。

## 工具回归

```sh
/usr/bin/arch -arm64 /usr/bin/python3 -B -m unittest discover -s clients/macos/scripts/tests -v
zsh -n clients/macos/scripts/compare-performance-baselines.sh
zsh -n clients/macos/scripts/run-app-performance-scenario.sh
zsh -n clients/macos/scripts/package-app.sh
zsh -n clients/macos/scripts/verify-app-build.sh
zsh -n clients/macos/scripts/run-process-group-soak.sh
```
