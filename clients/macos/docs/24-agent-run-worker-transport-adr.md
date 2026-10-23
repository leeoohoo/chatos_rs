# Agent Run Worker 传输 ADR

状态：v2 已接入生产 `AppModel`；后续协议演进必须保持 fail-closed 和 durable checkpoint 兼容。

## 决策

每个 Agent manager/executor Run 启动一个 `ChatOSAgentWorker`。Worker 拥有该 Run 的 `AgentRuntime` 状态机、生产 Responses provider 网络连接和响应解析；主 App 继续独占 SQLite、持久模型配置来源、账户会话、Memory 服务及工具授权。两者通过继承 pipe 上的版本化 NDJSON 协议交互，Worker 不接收数据库路径，也不直接写业务 Store。

此边界同时满足两项要求：独立任务有独立 PID、取消、超时和崩溃域；数据库仍只有一个业务写所有者。禁止为了减少 IPC 而让每个 Worker 打开同一 SQLite，也禁止在 Worker 缺失或版本不兼容时静默退回主进程。

## 协议与身份

- v2 握手包含父 PID、Run UUID、随机 nonce 和支持的版本区间；响应必须原样绑定 Run/nonce。v1 不包含直接模型传输能力，版本区间不重叠时必须拒绝，不能静默降级到旧 Worker。
- Worker 启动时校验真实父 PID并持续监视父进程退出。没有 `--parent-pid` 能力或被其他进程转交的启动会拒绝。
- 单条 JSON line 最大 64 MiB；读取在分配后、解码前再次检查大小。消息顺序错误、未知响应、请求 ID 不匹配或最终 Run/scope 改变均终止本 Run。
- 消息只在已继承匿名 pipe 中传输，不建立可被同机其他进程连接的 socket，不把 token 放入命令行或环境变量。

## Capability 代理

Worker 可请求 tool execute、completion check、Memory context prepare、checkpoint record 和模型会话授权。父进程在原账户/Run 闭包中执行这些请求，因此 Worker 不能选择账户、数据库、任意工具 provider 或新的权限范围。非生产注入模型仍可使用 v2 的 model callback 兼容路径，但生产 Agent 的 Responses transport 不经过该回调。

生产模型配置在 App 确认当前认证 session 后，通过继承 pipe 一次性交付给所属 Worker；只包含 Responses 请求需要的 endpoint/model/参数和凭据，不写入 checkpoint、argv、环境变量、stderr 或 manifest。Worker 每次请求前后都向 App 发起 session 授权挑战，账户切换后的响应不得进入 checkpoint 或工具执行。当前 provider 不支持短期 Run token，因此 Worker 在 Run 生命周期内持有上游 key；这是可信同包进程间的故障隔离，不是凭据安全沙箱，未来后端提供短期 capability 后应替换长期 key。

模型流事件在 Worker 内直接驱动 inactivity timeout 和解析器，不再经 App 往返；完整 checkpoint/event 回传 App 后才视为 durable。大文件附件继续使用 App-owned 文件引用与既有校验路径，不把正文复制进工具 IPC。v2 的 checkpoint 上限仍是 64 MiB inline，超过上限必须安全失败，后续如需扩大应改为 App-owned artifact token，而不是继续抬高 pipe 上限。

## 故障与取消

- App 取消或 Run 墙钟超时先终止对应 Worker，两秒后仍存活升级 `SIGKILL`；其他 Worker 不受影响。
- Worker 崩溃时，App 以最后一次 durable checkpoint 判断恢复语义。若存在 `inFlightCallID`，结果被视为未知并进入 `needsReview`；Todo 进入 blocked，禁止自动重放副作用。
- 无 in-flight 副作用的崩溃按普通 Run 失败处理，可由既有业务恢复策略重新领取。
- 父进程退出由 Worker 的 parent monitor 触发退出，避免孤儿进程继续请求 capability。

## 当前边界

v2 已把生产 Agent 的 Responses HTTP/SSE、请求体构造和响应解析迁入 Worker，因此 provider 卡死、畸形响应解析和对应网络缓冲已进入单 Run 进程边界。Memory 和受权工具仍在父进程 capability 端；部分工具本身再委托既有 Host/Plugin 进程，但不能据此宣称所有工具 CPU/内存工作集都已隔离。Worker 当前也不是 App Sandbox/seatbelt 意义上的凭据安全边界。后续演进应加入跨进程动态 CPU/内存 lease、连续崩溃熔断、真实强杀每个授权/网络/工具 IPC 窗口和长时间组合负载验证，并逐项判断 App 内工具是否可安全迁移。
