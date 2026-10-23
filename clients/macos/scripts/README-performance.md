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

此基准没有测量 App 切页、首个可交互帧、主线程 hang、physical footprint、Host IPC 或媒体链路；小样本分位数不是发布 SLO 证明。应在固定机器、数据集、供电、热状态、缓存和负载下比较，不把不同运行环境直接合并。正式导航脚本、signpost/trace、真实组合负载与干净提交的基线/候选对照仍待完成。

## 工具回归

```sh
/usr/bin/arch -arm64 /usr/bin/python3 -B -m unittest discover -s clients/macos/scripts/tests -v
zsh -n clients/macos/scripts/package-app.sh
zsh -n clients/macos/scripts/verify-app-build.sh
```
