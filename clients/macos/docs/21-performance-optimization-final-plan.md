# ChatOS macOS 客户端最终性能优化方案

更新时间：2026-09-28

状态：待实施

本文是当前 macOS 客户端性能工作的最终执行基线。`20-performance-remediation-plan.md` 保留为 2026-09-20 第一轮资源泄漏与生命周期治理记录；本文覆盖本轮发现的发布构建、SwiftUI 更新边界、Agent 任务板、Markdown 主线程工作、桌面宠物、轮询和代码预览问题。

## 一、结论

当前卡顿主要来自 macOS 客户端自身，不是 Rust 后端、SQLite 或网络请求成为主线程瓶颈。问题由三层叠加：

1. `/Applications/ChatOS.app` 当前是未优化的 Debug 构建，放大了所有 SwiftUI、文本和布局开销。
2. 聊天、Agent 任务板和 Markdown 的状态观察与渲染边界过大，局部变化会让不相关的大视图重新求值、测量和布局。
3. 宠物动画、多个 HostingView、Visual Session、审批、剪贴板和激活恢复任务持续唤醒 MainActor，并增加 WindowServer 合成压力。

只替换 Release 包会明显改善体感，但不能根治；只优化某一个列表也不能解决聊天输入、长 Markdown、切换应用和截图卡顿。必须按本文顺序同时修复发布链路、更新范围和持续后台工作。

## 二、已确认的证据

### 2.1 当前安装包是 Debug 构建

- 已安装二进制包含 `.build-native/arm64-apple-macosx/debug/` 路径。
- `/Applications/ChatOS.app` 当前约 107 MB，Mach-O `__text` 约 39.3 MB。
- `clients/macos/scripts/package-debug-app.sh` 默认 `CHATOS_BUILD_CONFIGURATION=debug`。
- `scripts/deploy-online.sh` 的 `package_client mac` 直接调用上述 Debug 打包入口。
- `package-release-dmg.sh` 才显式将构建配置覆盖为 `release`。

因此，“本地安装成功”目前不能证明安装的是可用于性能验收的生产构建。

### 2.2 Agent 任务板的热点是布局与辅助功能树

对真实任务页进行 20 秒采样时，主线程工作集中在：

- `LayoutEngineBox.sizeThatFits`
- `ViewLayoutEngine.sizeThatFits`
- AttributeGraph update
- Stack layout / child placement
- Text 与 Accessibility attributed text resolution

同一次采样没有发现 JSON 解析、SQLite 查询或 Rust 调用成为主线程热点。

当前真实项目有 35 个 Todo，默认每页 20 个。单页辅助功能树约 187–214 个元素；收起卡片虽然视觉上限制为两行，但仍保留完整长文本和 `.textSelection(.enabled)`。卡片同时包含头像、状态、长阻塞原因、多个 badge、overlay 和 shadow，导致滚动、展开和收起持续触发昂贵测量。

主要文件：

- `Sources/ChatOSApp/Features/AgentGroupChat/ProjectAgentGroupChatTodoView.swift`
- `Sources/ChatOSApp/Features/AgentGroupChat/AgentListPagination.swift`

### 2.3 聊天输入会扩大更新时间线

`ConversationSessionViewModel` 是一个 `@MainActor ObservableObject`，消息、发送状态、错误、附件、运行设置和 `draft` 等状态都通过同一个 `objectWillChange` 发布。

`ConversationTimelineView` 观察整个会话对象，`ComposerView` 又直接绑定 `conversation.draft`。因此输入草稿变化会让时间线根视图参与重新求值。时间线派生数据还会遍历 turns、建立字典并重建数组，长会话下会把一次轻量输入扩大为整条时间线的工作。

主要文件：

- `Sources/ChatOSApp/Features/Chat/ConversationSessionViewModel.swift`
- `Sources/ChatOSApp/Features/Chat/ConversationTimelineView.swift`
- `Sources/ChatOSApp/Features/Chat/ComposerView.swift`
- `Sources/ChatOSApp/Features/Pet/PetQuickChatView.swift`

### 2.4 Markdown 存在主线程重活

当前 `MarkdownLayoutTextView` 为 `@MainActor`。整篇 attributed string 的生成和写入、图片到达后的整篇重渲染、文本尺寸测量均会参与主线程工作。代码注释已经记录过 selectable fragments 导致 AttributeGraph invalidation，以及 lazy stack 发生重复 size/place cycle 的历史问题。

图片下载虽为异步，但 `NSImage(data:)` 解码和最终文档更新仍处于 MainActor 隔离范围；图片缓存使用了 cost，却没有配置 `totalCostLimit` 与 `countLimit`。

主要文件：`Sources/ChatOSApp/Features/Shared/MarkdownDocumentView.swift`。

### 2.5 宠物和轮询是持续放大器

宠物开启/关闭的同机 A/B 采样：

| 状态 | ChatOS 后台平均 CPU | WindowServer 平均 CPU |
| --- | ---: | ---: |
| 宠物关闭 | 约 0.13% | 约 62.3% |
| 宠物开启 | 约 0.90% | 约 67.3% |

WindowServer 的绝对占用还受到虚拟机、ChatGPT Renderer 和其他桌面应用影响，不能全部归因于 ChatOS；但 ChatOS 宠物带来的增量明确存在。

当前宠物只根据“可见且屏幕醒着”决定动画是否活动，不区分实际遮挡、空闲时长和高成本状态。空闲约 0.6 秒换帧，running 约 0.15 秒换帧；控制器还长期持有多个 HostingView。

此外存在以下常驻或激活触发工作：

- Visual Session 在有活动会话时最快每 450 ms 拉取。
- 审批已有事件流，但仍每 2 秒轮询。
- 剪贴板监控在 MainActor 上自适应轮询。
- 每次 `NSApplication.didBecomeActive` 都恢复连接，并取消、重建 Artifact 同步任务。

### 2.6 大文件代码高亮为同步全量正则

代码预览允许最多 600,000 个 UTF-16 字符，并在整篇内容上依次执行数字、关键字、字符串和注释正则匹配。大文件首次打开或切换语法时会形成可预期的主线程停顿。

主要文件：`Sources/ChatOSApp/Features/Project/CodePreviewView.swift`。

## 三、性能设计原则

后续实现必须遵守以下约束：

1. **Release 才是验收基线。** Debug 只用于开发诊断，不能用于最终性能结论。
2. **局部状态只更新局部 UI。** 草稿、按钮、轮询结果或单条消息变化不得让整个工作区重新求值。
3. **数据准备离开 MainActor。** 解析、排序、分组、正则、图片解码和渲染模型构造应在可取消后台任务中完成；只把最终 UI 状态提交到主线程。
4. **不可见即不工作。** 页面、预览、动画和轮询必须与可见性、选中状态、应用生命周期和系统休眠绑定。
5. **相同值不重复发布。** 所有轮询、事件流和桥接层在赋值前执行等值判断或版本检查。
6. **复杂内容按需创建。** 折叠态不创建详情树，不把完整长文暴露为折叠卡片的辅助功能内容。
7. **先测量再宣布完成。** 每个阶段必须保留相同数据集、相同机器和相同交互脚本的前后对比。

## 四、实施顺序

### P0-A：修正构建、安装和性能基线

目标：消除 Debug 放大器，并保证以后不会再次把 Debug App 当成正式客户端安装。

实施：

- 新增语义明确的 `package-app.sh`，要求显式选择 `debug` 或 `release`；保留 Debug 脚本仅供开发。
- `deploy-online.sh package_client mac` 默认产出 Release；本地开发安装命令显式使用 Debug。
- 安装脚本只允许从刚刚完成并验证的提交构建，不复用未知缓存产物。
- 在产物中写入构建配置、Git SHA、构建时间，并在“关于”或诊断页展示。
- CI 对可分发 App 执行以下门禁：
  - 二进制不得包含 `.build-native/.../debug/`；
  - 必须通过 Release 构建与签名校验；
  - 安装包版本、Git SHA 与流水线输入一致；
  - 从已推送提交构建，不允许脏工作区作为发布来源。
- 建立固定性能数据集和 `os_signpost`：聊天派生、Markdown 解析/布局、任务板卡片构造、宠物换帧、激活恢复和代码高亮分别计时。

验收：正式安装包不存在 Debug 路径；诊断页可确认 Release 与 Git SHA；后续所有指标均来自该 Release 包。

### P0-B：收窄聊天状态更新边界

目标：输入一个字符只更新 Composer，不更新时间线和 Markdown。

实施：

- 将 `draft`、附件草稿、附件错误和 Composer 焦点拆到独立 `ComposerState`。
- 时间线只观察消息快照、分页状态和与消息展示直接相关的状态。
- 优先迁移到 `@Observable` 的属性级追踪；若短期保留 Combine，则使用小型 ObservableObject 分区。
- 将 `timelineItems` 改为仅在 turns 或任务图版本变化时重建的缓存快照。
- 为 turn、message、process item 保持稳定 ID，避免数组重建后整段 diff 失效。
- 不让时间线根视图直接观察完整 `AppModel`；通过只读环境值或窄服务传入本地化、主题和动作。
- Quick Chat 使用同一分层，不复制旧的“大对象 + 整页观察”结构。

自动化验证：

- 修改 `draft` 时，时间线派生计数保持不变。
- 1,000-turn 会话连续输入 200 个字符，Markdown 文档不重新构造。
- 附件进度只更新 Composer/附件区域，不更新时间线历史。

### P0-C：重构 Markdown 渲染管线

目标：长 Markdown、流式回复和图片消息不阻塞主线程。

推荐管线：

```text
原始文本 / 流式增量
        ↓  后台、可取消、可合并
Markdown 解析与块级 RenderModel
        ↓  MainActor 只提交版本化快照
可见块渲染 / NSTextStorage 更新
        ↓  后台图片下载与解码
单个图片块更新，不重建整篇文档
```

实施：

- 将 Markdown 解析、块级 diff、语法处理和图片元数据准备移到独立 actor 或非隔离纯函数。
- 不在后台操作 NSTextView/NSTextStorage 等 AppKit UI 对象；后台生成 Sendable render model，主线程只做最小最终提交。
- 流式文本按时间窗批量合并，建议 33–50 ms 提交一次，而不是每个 token 更新一次。
- 按 Markdown block 使用稳定 ID；图片到达时只替换对应图片块。
- 图片使用 ImageIO 在后台按目标像素解码，避免 `NSImage(data:)` 在 MainActor 解码原图。
- 给图片缓存设置 `countLimit` 与 `totalCostLimit`，内存警告、会话关闭和账户切换时清理。
- 测量缓存按“文档版本 + 宽度 + 字体/动态字号”键控并设上限，禁止无界增长。
- 流式期间允许使用轻量纯文本或简化样式；结束后再完成最终 Markdown 排版。

验收：长文流式输出期间无大于 100 ms 主线程 hang；图片到达不触发整篇 attributed string 重建；缓存保持在预算内。

### P0-D：治理 Agent 任务板和复杂列表

目标：35–100 个任务时滚动、展开、收起和分页保持稳定。

实施：

- 默认每页从 20 降为 10；10 项模式优先验证普通 `VStack` 是否比可变高度 `LazyVStack` 更稳定。
- 引入 `TodoCardPresentation`，在 Todo 或 Run 版本变化时预计算状态、短摘要、计数和颜色，SwiftUI body 不做重复聚合。
- 折叠态生成真正的短摘要，不把完整 objective、result 或 blocked reason 交给布局和辅助功能系统。
- 折叠态移除 `.textSelection(.enabled)`；完整复制能力放到详情页。
- 给卡片设置简短、明确的 accessibility label/value，并将装饰元素从辅助功能树隐藏。
- 展开后才创建交付物、验收条件、约束、授权和完整结果视图。
- 移除列表卡片 shadow，使用单层边框或背景区分；减少重复 overlay。
- 用轻量按钮/弹出层替换原生 menu Picker，避免滚动时构造完整菜单辅助功能树。
- 只在任务数组或 Run presentation 版本变化时计算总览统计。
- 为展开状态设置上限或采用单项展开，防止多张超高卡片同时常驻。

验收：35 个真实 Todo、第一页含超长阻塞文本时，辅助功能树控制在 120 个元素以内；连续滚动和展开无大于 100 ms hang。

### P1-A：降低宠物与 WindowServer 合成开销

目标：保留桌面宠物能力，但不让其成为后台持续合成源。

实施：

- 面板按需创建：Quick Chat、翻译、记事本和过程面板首次使用时才创建，关闭后允许释放。
- 每个面板只观察自己的窄状态，不再共同观察完整 `AppModel`。
- 将精灵换帧从 SwiftUI `TimelineView` 整棵视图更新改为轻量 CALayer/contents 更新，或证明现有方案达到预算后保留。
- 细分动画策略：
  - 不可见、屏幕休眠、所在 Space 不可见：完全暂停；
  - 可见但长时间空闲：静态帧或不高于 1 FPS；
  - 有真实运行活动：按状态短时提升帧率；
  - 拖动结束立即回落。
- 减少宠物和浮层的实时 shadow、透明材质与超大绘制区域。
- 对 WindowServer 使用“关闭宠物/开启宠物、相同桌面场景”做增量对比，不把其他应用的系统负载算到 ChatOS。

验收：Release 下宠物隐藏时 ChatOS 后台 CPU 平均不高于 0.3%；宠物可见空闲时不高于 0.8%；相对宠物关闭，WindowServer 增量不高于 2 个百分点。

### P1-B：合并轮询、事件流和激活恢复

目标：空闲时不再存在 450 ms 或 2 秒级无条件常驻工作。

实施：

- Visual Session 仅在对应预览可见且选中时快速刷新；折叠、切换 Tab、切换资源或应用隐藏时停止或退避到低频健康检查。
- 审批以事件流为主；2 秒轮询改为断流后的恢复机制或 30–60 秒一致性校验。
- 所有审批数组和状态赋值前检查版本或等值，避免相同值重复发布。
- 剪贴板保留系统能力，但连续无变化后退避；读取、哈希和持久化尽量离开 MainActor。
- `didBecomeActive` 不再无条件 cancel/restart Artifact 同步；改为幂等 `ensureRunning`，只在任务不存在、身份变化、断流或超过过期时间时恢复。
- 连接恢复、Heartbeat、Artifact Sync 和工作区刷新使用去重协调器，短时间多次激活合并为一次。

验收：完全空闲 60 秒内没有 450 ms/2 秒级永久唤醒；连续切换应用 30 次不会创建重复同步任务，切回交互无大于 100 ms hang。

### P2-A：后台化代码预览与语法高亮

目标：打开大源码文件不阻塞 UI。

实施：

- 文件读取后先立即显示无高亮文本，再在可取消后台任务中生成高亮结果。
- 以 revision/file identity 防止旧任务覆盖新文件。
- 优先使用增量 tokenizer；短期至少按可视范围或分块处理，不再对 600,000 字符同步跑四轮全量正则。
- 切换文件、关闭标签或滚动离开时取消不再需要的任务。
- 超过阈值时默认使用纯文本模式，并允许用户主动启用完整高亮。

验收：600,000 字符文件首次内容在 100 ms 内可见；后台高亮期间滚动和切换标签可响应；旧任务不会回写当前文件。

## 五、统一性能预算

所有预算在同一台目标 Mac、同一份固定数据、Release 构建下测量。Debug 数字只用于开发期回归定位。

| 场景 | 必须达到的预算 |
| --- | --- |
| 离散交互 | 主线程无大于 100 ms hang |
| 连续滚动/动画 | 60 Hz 设备 P95 frame time ≤ 16.7 ms，P99 ≤ 33.3 ms |
| 聊天输入 | 输入到显示 P95 ≤ 50 ms；draft 变化不重建时间线 |
| Agent 任务页 | 35 个 Todo、10 项/页，连续滚动 60 秒无大于 100 ms hang |
| Agent 辅助功能树 | 10 张折叠卡片页面 ≤ 120 个元素，不包含完整长结果文本 |
| Markdown 流式输出 | 2 分钟输出无大于 100 ms hang；UI 提交频率受控 |
| 应用切回 | P95 恢复工作 ≤ 100 ms；无重复同步任务 |
| 空闲 CPU | 宠物隐藏后台平均 ≤ 0.3%；宠物可见空闲平均 ≤ 0.8% |
| WindowServer 增量 | 宠物可见空闲相对关闭宠物 ≤ 2 个百分点 |
| 内存 | 冷启动空闲 physical footprint ≤ 150 MB；压力场景后 ≤ 300 MB 且可回落 |
| Markdown 图片缓存 | 有明确条目和成本上限，建议总成本 ≤ 64 MiB |
| 进程生命周期 | 退出 ChatOS 后无插件、Agent 或辅助进程残留 |

若目标机器当时 WindowServer 或其他应用已有异常负载，必须同时记录系统基线，并报告 ChatOS 的增量，不能只引用绝对值。

## 六、验证矩阵

每次优化至少覆盖以下固定场景：

1. 冷启动、登录、空闲 60 秒。
2. 宠物关闭、宠物可见空闲、宠物 running、跨 Space。
3. 35 个真实 Todo，含一条超长阻塞原因；连续滚动、展开和收起。
4. 1,000-turn 长会话；持续输入、删除、粘贴和附件变化。
5. 长 Markdown、代码块、表格、链接和 10 张远程图片。
6. 流式回复持续 2 分钟。
7. 在 ChatOS 与其他应用之间切换 30 次。
8. 打开 600,000 字符源码并快速切换文件。
9. 连续访问 30 个会话，检查缓存、订阅和任务数量。
10. 退出应用，检查插件、连接器、Agent 与辅助进程清理。

使用工具：

- Time Profiler：确认主线程热点与调用栈。
- SwiftUI Instrument：检查 body update、AttributeGraph invalidation 和布局次数。
- Animation Hitches / Core Animation：检查滚动、合成和 WindowServer 增量。
- Hangs：捕获大于 100 ms 的主线程停顿。
- Allocations、Leaks、Memory Graph、`vmmap`：确认缓存上限和回落。
- `sample`、`top`、`ps`：保留可复现的轻量命令行证据。
- Accessibility Inspector：确认折叠卡片没有暴露完整长文本。

## 七、测试与 CI 门禁

需要补充以下自动化测试：

- 打包测试：Release 产物不得包含 `/debug/`，构建元数据必须匹配 Git SHA。
- Observation 测试：修改 Composer 草稿不触发 timeline snapshot 重建。
- Markdown 测试：取消、版本覆盖、流式合并、图片块局部更新和缓存上限。
- Agent 任务板测试：分页、短摘要、辅助功能 label、展开态按需创建。
- 轮询策略测试：可见性、断流、退避、睡眠/唤醒和相同值去重。
- 激活恢复测试：重复 `didBecomeActive` 只保留一个同步任务。
- 代码高亮测试：取消旧 revision，大文件回退到纯文本。
- 生命周期测试：退出后进程组、任务和订阅全部结束。

性能基准可使用显式环境变量在固定机器上运行，不强制所有开发机每次执行；但合并性能专项 PR 和生成发布包前必须通过。

## 八、交付批次与依赖

按以下批次实施，每批独立提交、测试、推送和安装验证：

1. **批次 1：Release 发布链路与性能标记。** 后续所有数字的前置条件。
2. **批次 2：Agent 任务板。** 快速解决当前最明显的滚动、展开和 AX 树问题。
3. **批次 3：Composer/Timeline 状态拆分。** 解决长会话输入卡顿。
4. **批次 4：Markdown 后台管线和缓存。** 解决流式、长文和图片消息卡顿。
5. **批次 5：宠物、HostingView 和 WindowServer。** 降低后台与系统级合成压力。
6. **批次 6：轮询、激活恢复和代码高亮。** 清理持续唤醒和长尾停顿。
7. **批次 7：完整 Release 回归、灰度和性能报告。** 达标后才替换 `/Applications/ChatOS.app`。

批次 2–6 可以在代码层并行开发，但性能验收必须基于批次 1 的 Release 构建，并在合并后重新跑完整矩阵。

## 九、上线与回滚

- 新 Markdown 渲染器、任务板 presentation 层和宠物渲染器应有短期 Feature Flag，便于出现回归时单独回退。
- 首先在开发安装和内部 Release 上灰度；保留上一份已验证 App 与 SHA-256。
- 发生数据错误、输入丢失、消息错序或辅助功能退化时立即回滚对应模块，而不是用降低刷新频率掩盖正确性问题。
- 每次正式安装遵循：修改 → 测试 → 提交 → 推送 → 从已推送提交构建 Release → 验证构建元数据 → 安装。

## 十、完成定义

以下条件全部满足后，性能优化才算完成：

- 正式部署和安装链路只产出经过校验的 Release App。
- 本文 P0/P1 项全部完成并有测试；P2 项至少满足大文件响应预算。
- 固定验证矩阵全部通过，性能预算无例外项。
- Time Profiler 中交互热点不再由无关的整页 SwiftUI 更新主导。
- 聊天输入不会更新时间线；Markdown 图片不会重建整篇文档。
- Agent 折叠卡片不保留完整长文的选择与辅助功能负担。
- 宠物和轮询在不可见或空闲时达到 CPU 与 WindowServer 增量预算。
- 应用退出后没有残留进程；连续切换应用没有重复任务。
- 最终性能报告记录硬件、macOS 版本、Git SHA、构建配置、测试数据和前后对比。
- 改动已提交并推送，安装包确实从该已推送提交构建。
