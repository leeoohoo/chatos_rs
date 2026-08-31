# Computer Use 输入与任务卡死治理实施方案

## 目标

一次性治理 ChatOS 通过 Task Runner 调用 Open Computer Use 时出现的五类关联故障：

1. 鼠标到达文本输入框后没有形成稳定 hover，真实点击未获得文本焦点，键盘输入失败。
2. Computer Use 返回的 AX 文本超过 Task Runner 单工具模型输入预算后被整体替换，Agent 看不到输入框和元素索引。
3. Computer Use 返回截图后，MCP Management 将超过 256 KiB 的结果整体替换成 `result_truncated`，Task Runner 看不到真实界面状态。
4. `type_text` 卡在 App Agent socket 后，插件 stdio 和驻留 App Agent 都没有形成可恢复的超时边界，后续请求继续排队。
5. 模型供应商请求长时间没有任何数据时，Task Runner 仍可等待两小时，界面长期停留在“运行中”。

本方案不修改旧 Local Connector Client。允许修改范围仅包括新的 Swift 客户端、Open Computer Use 插件，以及与新模式直接相关的 ChatOS 后端服务。

## 已确认的故障链路

### 1. 文本输入焦点链路

当前 `type_text` 的关键顺序是：

1. 通过 AX 找到一个可编辑元素。
2. 激活目标应用并尝试设置 AX focus。
3. 软件鼠标移动到目标坐标。
4. 真实鼠标只发送一次 `mouseMoved`，约 30 ms 后立即发送 `mouseDown` / `mouseUp`。
5. 未严格确认真实点击已经让目标输入框获得焦点，就发送真实键盘事件。
6. 输入值未变化时返回失败。

Electron / 飞书需要在目标应用已经前台的前提下处理 hover 和 cursor 更新。30 ms 不足以稳定完成这一过程，因此系统鼠标还没有切换为 I-beam 时点击已经发生。画中画的软件鼠标也始终绘制为箭头，无法反映当前是普通点击目标还是文本输入目标。

### 2. 模型文本预算链路

旧运行中，飞书 `get_app_state` 的持久化工具结果约 44 KB；转换成给模型的 AX 文本后约为 16,000 字符、21,327 字节。Task Runner 当时的单工具模型输入预算是 8,000 字符，因此结果在进入下一次模型请求前被整体替换为：

```text
[Tool result omitted before sending to the model]
```

模型并不是“看到了状态但判断错误”，而是根本没有收到 AX 树。旧逻辑还会在一条结果超限后清空本轮剩余总预算，使后续工具结果也被整体抹掉。

### 3. MCP 工具结果持久化链路

Open Computer Use 的截图以 PNG Base64 形式进入 MCP JSON 结果。原始 PNG 约 190–240 KiB 时，Base64 和 JSON 包装后的结果通常超过 256 KiB。

MCP Management 的 Runtime Invocation Store 当前硬编码：

```text
MAX_INLINE_RESULT_BYTES = 256 * 1024
```

超过该阈值后不会保留原结果，而是整体替换为：

```json
{"status":"result_truncated","result_bytes":265168}
```

这不是单纯的 UI 展示截断。Task Runner 后续拿到的也是替换后的结果，因此图片无法被转换成临时模型视觉输入，Agent 只能再次读取状态或猜测操作结果。

图片本来会通过 `TransientToolModelInput` 独立进入模型；真正破坏图片的是更早发生的 256 KiB Runtime Invocation 持久化截断。它和上一节的文本预算是两个独立阈值，必须分别修复。

### 3.1 模型网关 PNG 解码兼容性

2026-08-27 的失败任务保留了两张 `1280×785` PNG。两张图片均具有完整 PNG signature、IHDR、IEND，macOS ImageIO 与 `sips` 都能正常解码；第一张被同一模型端点接受，第二张却稳定返回：

```text
The image data you provided does not represent a valid image.
```

将失败图片的像素内容重新编码为 baseline JPEG 后，对同一 `gpt-5.6-luna` 端点请求立即返回 HTTP 200。这证明图片没有在 Base64、MCP 持久化或预算阶段被截断，而是模型网关对某类合法 macOS PNG 的解码兼容性问题。

修复采用两层防护：Open Computer Use 给模型返回经过本地解码验证的 JPEG，同时继续用 PNG 发布画中画；ChatOS Swift 在插件原始结果完成本地画中画消费后，再对所有插件 PNG 图片统一解码并转换为 JPEG。无法本地解码的图片不会继续污染模型请求，而是转换成明确的截图失败文本。

### 4. Plugin / App Agent 卡死链路

旧飞书任务的工具时间线已确认：第一次 `type_text` 在约 3 秒内明确失败；第二次 `type_text` 从 17:46 挂到 18:28，持续约 42 分钟。最后出现的“Plugin MCP 进程已退出”是更新插件时人工停止旧进程造成的，不是原请求自行恢复。

完整卡死路径是：

1. Open Computer Use 的 CLI 到 `.app` App Agent 使用 Unix socket，客户端通过阻塞式 `fgetc` 读取，没有 socket 收发截止时间。
2. App Agent 在整个工具调用期间持有请求作用域串行锁；只断开 socket 不能释放卡住调用持有的锁。
3. ChatOS 未收到插件声明的超时时，默认允许工具调用等待 7,200,000 ms。
4. Swift stdio client 即使结束了 continuation，也没有结束卡死子进程；后续 JSON-RPC 请求继续排在旧调用后面。
5. 上层取消如果只移除 pending continuation，同样会留下不可用的插件进程。

因此“给 socket 加超时”本身仍不完整。超时后必须同时使驻留 App Agent 和 stdio 插件会话失效，下一次运行重新建立干净进程。

### 5. 模型请求长期无进展链路

Task Runner 的模型调用使用 `reqwest` 流式读取。当前配置中心默认值为：

```text
task_runner.ai.read_timeout_ms = 7_200_000
task_runner.execution.timeout_ms = 7_200_000
```

因此供应商已经建立连接、但迟迟不返回响应头或下一批流数据时，单次模型请求最多可以静默等待两小时。这是与 42 分钟 `type_text` 卡死并列的另一条无进展路径：前者发生在模型 HTTP 请求，后者发生在 Plugin / App Agent transport，不能用同一个补丁互相替代。

模型请求超时还会进入通用瞬时错误重试策略；默认最多重试 5 次。如果只缩短单次超时而不区分“无响应超时”和其他瞬时网络错误，总等待时间仍然可能过长。

## 设计原则

- 允许任务长时间执行，但不允许长时间没有任何进展。
- 真实鼠标与真实键盘事件是唯一成功路径；AX 只用于识别、命中验证和焦点观测。
- 图片与文本采用不同预算：文本保持有限，视觉输入允许更大的短生命周期载荷。
- 所有自动重试必须有明确上限，并在 UI 中留下可诊断的最终失败原因。
- 内层执行器负责识别单步无响应并尽快返回；客户端外层工具截止时间保持两小时，只作为任务执行合同的最终上限。
- 对无法按“单次请求”取消的 stdio/GUI 进程，超时和取消都必须使整个进程会话失效。
- 每个阶段单独测试和验收，上一阶段未通过不得继续安装到客户端。

## 阈值设计

| 层级 | 最终值 | 依据 |
| --- | ---: | --- |
| Open Computer Use PNG 目标 | 900 KB | 恢复旧的清晰度预算；Base64 后约 1.2 MB，仍低于 4 MiB 持久化上限和 2 MiB 解码图片上限。 |
| MCP Runtime inline 结果 | 4 MiB | 2 MiB 图片经 Base64 约 2.67 MiB，再预留 AX 文本和 JSON 包装；仍远低于 MongoDB 16 MiB 单文档上限。 |
| 模型单工具文本 | 40,000 字符 | 已观测飞书 AX 文本约 16,000 字符，提供约 2.5 倍余量；超限时保留头尾，不再整体抹除。 |
| 模型单轮工具文本总量 | 200,000 字符 | 允许约 5 条满额工具结果，同时继续受模型上下文和结构化截断保护。 |
| App Agent 单请求 | 20 秒 | GUI framework 调用无返回时的内层 watchdog，可通过环境变量在 1–120 秒内调整。 |
| ChatOS 插件工具外层上限 | 7,200 秒 | 保持工具调用两小时合同；明确失败立即返回，App Agent 内层 watchdog 仍会提前终止完全无响应的单步调用。 |
| AI 连续无数据 | 180 秒 | 允许正常长流式生成，只约束没有响应头或没有新数据的静默等待。 |
| Task 总执行时长 | 7,200 秒 | 长任务仍可运行两小时，只要期间持续产生进展。 |

## 分阶段实施

### 阶段一：真实 hover、焦点与 I-beam

Open Computer Use：

1. 引入 `pointer` / `text` 两种软件鼠标类型。
2. 普通目标继续显示箭头；可编辑文本目标到达命中点后切换为 I-beam。
3. 真实鼠标移动与点击拆成两个动作：先移动并等待 hover 稳定，再点击。
4. 文本目标采用更长的 hover settle；普通按钮保持较短延迟。
5. 点击前重新做 AX position hit-test，确认命中元素或其祖先是目标可编辑控件。
6. 点击后必须观察到目标可编辑元素真正 focused，才允许发送键盘事件。
7. 焦点失败时只在工具内部刷新状态并重定位一次；仍失败则返回结构化诊断，不把无限重试留给 Agent。
8. 画中画和本机 overlay 使用同一种 cursor kind 和同一命中锚点。

验收：飞书搜索框中，真实系统鼠标先进入文本形态，再点击并输入；画中画同步显示 I-beam；输入值可由后续 AX 状态确认。

### 阶段二：工具结果承载与模型预算

ChatOS MCP Management：

1. 将 Runtime Invocation 的 inline 结果上限从无上下文的 256 KiB 常量改为与 MCP 图片能力对齐的命名常量。
2. 上限至少覆盖 2 MiB 解码图片经 Base64 和 JSON 包装后的结果，同时保持在 MongoDB 单文档 16 MiB 限制以内。
3. 推荐 inline 上限为 4 MiB；超过后仍保留明确的截断元数据。
4. 增加边界测试：阈值内原样保留，阈值外才替换。
5. 保持图片与文本预算相互独立，验证图片仍通过 `TransientToolModelInput` 进入模型，避免把 Base64 当文本塞入上下文。

Task Runner / AI Runtime：

1. 单工具文本预算从 8,000 调整为 40,000 字符；单轮总预算从 48,000 调整为 200,000 字符。
2. 超限结果改为保留头部和尾部，中间插入结构化截断标记。
3. 一条结果超限只消耗它实际保留的预算，不再清空本轮剩余预算。
4. Configuration Center 的当前环境配置与代码默认值同步调整，避免只改默认值却不影响现有 revision。

Open Computer Use：

1. PNG 默认预算从为了绕过旧阈值的 240 KB 恢复为 900 KB。
2. 明确 MCP 状态截图与 ChatOS 画中画的独立预算。

验收：当前约 265 KiB 的 Computer Use 结果不再变成 `result_truncated`；16,000 字符级 AX 文本不再被整体 omitted；Task Runner 下一次模型请求同时获得 AX 文本和图片视觉输入。

### 阶段三：Plugin / App Agent 超时与进程恢复

Open Computer Use：

1. App Agent Unix socket 增加 20 秒默认收发超时，支持 `OPEN_COMPUTER_USE_APP_AGENT_REQUEST_TIMEOUT_SECONDS` 在 1–120 秒内覆盖。
2. 超时后向当前 MCP 调用返回 JSON-RPC 错误，不再永久阻塞 stdio。
3. 记录并验证 App Agent PID；transport 失败时先尝试 `SIGTERM`，500 ms 内未退出则使用 `SIGKILL`，释放被卡住的请求作用域锁。
4. 下一次工具调用通过 LaunchServices 拉起新的 App Agent，不复用旧 socket 和旧进程。

ChatOS Swift：

1. 插件工具外层截止时间保持 7,200 秒，与任务工具调用合同一致；它不是单步健康检查。
2. App Agent 20 秒内层 watchdog 负责识别完全无响应的 GUI transport，并返回结构化失败。
3. stdio 达到最终截止时间时不只结束 continuation，同时关闭管道、清理全部 pending、终止插件子进程。
4. Task cancellation 采用相同进程失效策略；禁止取消后继续复用已经卡死的 stdio 会话。
5. 后续调用明确返回 `processUnavailable`；新任务重新执行 prepare 并创建新会话。

验收：构造 App Agent transport 永不返回的请求；内层 watchdog 形成明确失败，相关进程被清理，下一次新会话可以正常 initialize 和调用。已经得到的接口错误、进程退出和校验失败立即返回；只有所有内层保护都失效时才触发两小时外层截止时间。

### 阶段四：AI 无进展超时与恢复

ChatOS Task Runner / AI Runtime：

1. 将 `task_runner.ai.read_timeout_ms` 定义为“连续无响应数据超时”，而不是整个任务总时长。
2. 默认值从 7,200,000 ms 调整为生产可用的 120,000–180,000 ms；整体任务执行超时仍可保持更长。
3. 对“响应读取超时”单独限制重试次数，最多自动重试 1 次；其他瞬时错误继续使用模型配置的通用重试策略。
4. 超时或重试耗尽后写入明确失败事件，解除运行状态和 claim，不允许停留在假运行中。
5. 验证用户取消会立即触发 CancellationToken，并中止正在等待的 HTTP 请求。
6. 验证 worker 重启后，运行中的 claim 能恢复、失败或重试，不会永久占用运行状态。
7. 调整本机配置中心的当前值；仅修改代码默认值不足以改变已经持久化的配置。

验收：模拟一个不返回任何数据的模型端点，任务在设定的无响应窗口内超时，最多重试一次，然后在 UI 中显示失败原因；取消操作能在数秒内结束任务。

### 阶段五：模型图片兼容性规范化

Open Computer Use：

1. 画中画继续发布 PNG，保持本机展示清晰度和透明合成质量。
2. MCP 模型截图先通过 ImageIO 本地解码，再编码为 baseline JPEG。
3. 本地解码失败时返回明确截图不可用文本，不发送会导致整轮模型请求 400 的图片块。

ChatOS Swift：

1. 插件运行时先使用原始结果刷新本地画中画。
2. relay 返回云端前，对所有插件的 PNG 图片执行本地解码和 JPEG 规范化，覆盖 Browser MCP 等其他插件。
3. JPEG 图片原样透传，避免重复编码。

验收：失败任务原始 PNG 对同一模型端点稳定复现 400；通过正式规范化路径生成的 JPEG 返回 HTTP 200，模型确认图片可读取。

## 测试矩阵

### Open Computer Use

- 单元测试：cursor kind、I-beam 锚点、渲染状态传递、hover 延时策略、focused 判定。
- 渲染测试：I-beam 中心像素与透明边界。
- 现有 Swift 测试：`swift test`。
- 工具 smoke：`./scripts/run-tool-smoke-tests.sh`。
- 飞书实测：打开搜索、hover、点击、输入、状态确认、Escape。
- App Agent 恢复：构造无返回请求，确认 socket 超时后驻留 Agent 被终止，下一次调用拉起新 PID。

### MCP Management

- `sanitize_terminal_result` 的阈值内/阈值外测试。
- MCP Runtime 图片转换测试，确认 Base64 从持久结构中移除并转成 transient image。
- 相关 Cargo 定向测试。

### Task Runner

- 无响应 HTTP fixture：在 idle timeout 后返回 timeout。
- timeout 重试次数上限测试。
- CancellationToken 中止 pending request 测试。
- Cloud Agent deadline、claim 和 terminal transition 测试。
- Task Runner 定向 Cargo 测试。

### 集成验收

1. 编译并重启 MCP Management、Task Runner worker/backend。
2. 完整替换并重启 Open Computer Use 插件 App。
3. 重启 ChatOS Swift 客户端。
4. 新建一条飞书搜索任务，观察模型请求、工具结果、画中画和任务终态。
5. 再使用故意无响应的模型 endpoint 验证不会长期卡死。
6. 故意卡住一次 App Agent transport，确认内层 watchdog 返回明确失败并且下一次新任务使用新的 App Agent / stdio 进程；客户端外层工具上限仍为两小时。

## 回滚

- Open Computer Use 安装前备份完整 `.app`，不只备份可执行文件。
- ChatOS 后端改动保留为小范围独立提交；服务重启前记录当前 PID 和配置值。
- 配置中心修改记录旧值，必要时可以恢复。
- 不删除当前历史任务和历史工具事件；卡死任务只通过正式取消或服务恢复流程终止。

## 完成标准

- 输入框必须在真实鼠标进入文本 hover 状态后才点击。
- 画中画的软件鼠标能正确显示 I-beam。
- 265 KiB 级 Computer Use 工具结果不再被替换。
- 16,000 字符级 AX 状态不再被整体抹除，超限结果仍保留可诊断的头尾内容。
- App Agent transport 完全无响应时由内层 watchdog 形成明确失败并释放进程会话；已经得到的 400、进程退出和图片校验失败必须立即返回，不等待两小时外层截止时间。
- 模型视觉输入使用本地验证后重新编码的 JPEG，画中画继续使用 PNG；失败任务中的问题 PNG 重新编码后能被同一模型端点接受。
- 模型无数据时不会等待两小时，取消和超时都能形成明确终态。
- 所有定向测试、Swift 全量测试和飞书端到端测试通过后才重新交付客户端。

## 当前实施状态（2026-08-27）

- 阶段一已完成：真实 hover/点击拆分、文本焦点闭环、I-beam 与画中画 cursor kind 传播均已实现；Swift 156 个测试和工具 smoke 通过。
- 阶段二已完成并验证：MCP Runtime Invocation inline 上限由 256 KiB 调整为 4 MiB；模型单工具/单轮预算调整为 40,000/200,000 字符；超限文本改为头尾保留；Open Computer Use PNG 默认预算恢复为 900 KB。AI Runtime 175 个测试、MCP invocation store 22 个测试和配置中心 catalog 27 个测试通过。
- 阶段三已完成并验证：App Agent 增加 20 秒 socket watchdog 和 transport 失败后的驻留进程重启；ChatOS 插件工具外层上限保持两小时；stdio 达到最终截止时间或被取消时都会终止卡死插件会话。确定性测试中，暂停旧 Agent 后 1 秒测试 watchdog 返回 timeout、旧 PID 被清理，同一 MCP 进程下一次请求成功拉起新 PID 并返回 9 个工具。
- 阶段四已完成代码与测试：AI 连续无数据超时默认调整为 180 秒；这类 timeout 最多自动重试 1 次；重试 client 保留原 read timeout；pending request 可被 CancellationToken 立即中止。
- 本机 Configuration Center 的 `local` 环境已发布 revision 51，`task_runner.ai.read_timeout_ms=180000`。整体任务执行超时仍保留 7,200,000 ms，长任务不会因为持续有模型数据而被误杀。
- 阶段五已完成：失败任务原始 PNG 在同一模型端点稳定复现 400，重新编码 JPEG 后返回 200；Open Computer Use 与 ChatOS Swift 均增加模型图片重编码和无效图片显式失败保护。
- Open Computer Use 158 个 Swift 测试、9-tool smoke 与 cursor idle smoke 通过；ChatOS Swift 117 项测试通过，新增图片规范化与两小时外层截止时间回归用例通过。
- Open Computer Use 已整体替换到 ChatOS 插件目录和 managed runtime，三处二进制 SHA-256 均为 `4a773f5b0f8a78e6e961f23f5050dede789c66ac234bcbd0a108e1151405255c`；ChatOS 已重新打包并启动。
- Configuration Center、MCP Management、Task Runner API/worker/scheduler 已定向重启；revision 51 的关键值确认为 `180000 / 40000 / 200000 / 7200000`，HTTP 服务健康。
- 历史卡住 run 已处于 `blocked` 终态，不再显示运行中。
- 待完成：macOS 当前锁屏，解锁后执行飞书“打开搜索、I-beam、真实点击、真实输入、状态与画中画更新”的最终 GUI 验收。
