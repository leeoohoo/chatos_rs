# Agent Run Worker 传输 ADR

状态：v1 已接入生产 `AppModel`；后续协议演进必须保持 fail-closed 和 durable checkpoint 兼容。

## 决策

每个 Agent manager/executor Run 启动一个 `ChatOSAgentWorker`。Worker 只拥有该 Run 的 `AgentRuntime` 状态机和生命周期；主 App 继续独占 SQLite、模型配置/凭据、Memory 服务及工具授权。两者通过继承 pipe 上的版本化 NDJSON 协议交互，Worker 不接收数据库路径，也不直接写业务 Store。

此边界同时满足两项要求：独立任务有独立 PID、取消、超时和崩溃域；数据库仍只有一个业务写所有者。禁止为了减少 IPC 而让每个 Worker 打开同一 SQLite，也禁止在 Worker 缺失或版本不兼容时静默退回主进程。

## 协议与身份

- v1 握手包含父 PID、Run UUID、随机 nonce 和支持的版本区间；响应必须原样绑定 Run/nonce。
- Worker 启动时校验真实父 PID并持续监视父进程退出。没有 `--parent-pid` 能力或被其他进程转交的启动会拒绝。
- 单条 JSON line 最大 64 MiB；读取在分配后、解码前再次检查大小。消息顺序错误、未知响应、请求 ID 不匹配或最终 Run/scope 改变均终止本 Run。
- 消息只在已继承匿名 pipe 中传输，不建立可被同机其他进程连接的 socket，不把 token 放入命令行或环境变量。

## Capability 代理

Worker 可请求 model stream、tool execute、completion check、Memory context prepare 和 checkpoint record。父进程在原账户/Run 闭包中执行请求，因此 Worker 不能选择账户、数据库、任意工具 provider 或新的权限范围。

模型流事件逐条回传给 Worker，保持 inactivity timeout 语义；完整 checkpoint/event 回传 App 后才视为 durable。大文件附件继续使用 App-owned 文件引用与既有校验路径，不把正文复制进工具 IPC。v1 的 checkpoint 上限仍是 64 MiB inline，超过上限必须安全失败，后续如需扩大应改为 App-owned artifact token，而不是继续抬高 pipe 上限。

## 故障与取消

- App 取消或 Run 墙钟超时先终止对应 Worker，两秒后仍存活升级 `SIGKILL`；其他 Worker 不受影响。
- Worker 崩溃时，App 以最后一次 durable checkpoint 判断恢复语义。若存在 `inFlightCallID`，结果被视为未知并进入 `needsReview`；Todo 进入 blocked，禁止自动重放副作用。
- 无 in-flight 副作用的崩溃按普通 Run 失败处理，可由既有业务恢复策略重新领取。
- 父进程退出由 Worker 的 parent monitor 触发退出，避免孤儿进程继续请求 capability。

## 当前边界

v1 把 orchestration loop、状态转换和 Run 生命周期迁入 Worker；模型 provider、Memory 和受权工具仍在父进程 capability 端执行。因此它已经隔离 Worker 自身崩溃和任务生命周期，但尚未隔离第三方模型 SDK 在 App 内的解析崩溃或所有 CPU/内存工作集。后续演进优先迁移可序列化的模型 transport，并加入跨进程动态 CPU/内存 lease、连续崩溃熔断、真实强杀矩阵和长时间组合负载验证。
