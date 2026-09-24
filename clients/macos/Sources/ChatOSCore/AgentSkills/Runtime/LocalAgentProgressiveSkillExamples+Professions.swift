import Foundation

extension LocalAgentProgressiveSkillExampleCatalog {
    static let professionProfiles: [String: Profile] = [
        "project_manager": .init(
            zh: (
                "结算模块计划两周后上线，接口迁移、回归测试和业务审批由不同成员负责；其中测试依赖迁移完成，业务负责人尚未给出最终验收时间。",
                "先读取 Todo、最新进度和项目看板，把目标拆成“迁移完成→回归通过→业务验收→发布”四个可验收节点。为每个节点记录唯一负责人、依赖、截止时间和证据位置；把业务验收列为需要 Human 处理的决策项。只有收到真实结果后才更新看板，并用 expected_revision 防止覆盖并发修改。",
                "Todo 引用与状态、依赖关系、负责人和期限、迁移与测试结果链接、业务审批记录、看板 revision，以及阻塞解除条件。",
                "根据聊天里的口头承诺直接把整体进度改成 90%，把“业务已同意”写成完成结论，并复制一份自算任务统计到看板。",
                "口头承诺不是验收证据，自算统计还会与系统事实产生第二套真相。应保留“待审核/受阻”状态，引用真实 Todo 和系统统计，明确需要 Human 在何时决定什么。"
            ),
            en: (
                "A checkout module is due in two weeks. API migration, regression testing, and business approval have different owners; testing depends on migration and the business acceptance date is unknown.",
                "Read the current Todos, progress, and dashboard. Model four evidence-based milestones: migration, regression, business acceptance, and release. Assign one owner, dependency, due point, and evidence location to each; expose business acceptance as a Human decision. Update the dashboard only from actual results and use expected_revision to protect concurrent edits.",
                "Todo references and states, dependency graph, owners and dates, migration and test evidence, approval record, dashboard revision, and unblock conditions.",
                "Set overall progress to 90% from chat promises, state that the business approved it, and copy hand-calculated task counts into the dashboard.",
                "Promises are not acceptance evidence and duplicated counts create a second source of truth. Keep review or blocked states, reference real Todos and system statistics, and name the exact Human decision and deadline."
            )
        ),
        "product_manager": .init(
            zh: (
                "客户提出“增加批量导出”，但没有说明使用者、可导出字段、权限边界、文件规模、格式或成功标准。",
                "先访谈目标用户并查看现有导出行为，把问题定义为“财务每周对账耗时且易漏项”。明确首版只支持有财务权限的订单 CSV、最多 5 万行、异步生成并通知下载；把自定义模板和跨租户导出列为非目标。用完成时间、失败率和人工步骤减少量作为指标，写出权限、空数据、超限和失败恢复验收场景。",
                "用户问题与来源、范围和非目标、优先级依据、字段与权限规则、完整验收场景、成功指标基线及待决策项。",
                "把用户说的功能名直接写成需求：“做一个导出按钮”，随后因为页面能下载十条测试数据就宣布需求完成。",
                "功能描述没有证明解决了用户问题，也遗漏规模、权限和失败路径。应回到问题和受众，定义可观察结果并让每条验收条件对应真实证据。"
            ),
            en: (
                "A customer asks for bulk export without identifying users, fields, authorization, volume, format, or success criteria.",
                "Inspect current behavior and interview target users, reframing the problem as slow and error-prone weekly reconciliation. Scope v1 to authorized finance users, order CSV, 50k rows, asynchronous generation, and download notification; exclude custom templates and cross-tenant export. Define time-to-complete, failure rate, and removed manual steps, plus authorization, empty, limit, and recovery acceptance cases.",
                "Sourced problem statement, scope and non-goals, priority rationale, field and authorization rules, acceptance scenarios, metric baseline, and open decisions.",
                "Turn the requested feature name into 'add an export button' and call it complete after downloading ten test rows.",
                "That does not prove the user problem is solved and ignores scale, authorization, and failure behavior. Define observable outcomes and map each acceptance condition to current evidence."
            )
        ),
        "technical_manager": .init(
            zh: (
                "一个跨客户端、API 和数据库的性能改造必须在两周内交付，团队成员经验不同，接口冻结时间也不一致。",
                "读取性能基线和代码边界，先确定 p95 延迟与资源预算。把工作拆为查询与索引、API 批处理、客户端渲染和端到端压测，明确接口合同、负责人、依赖和集成日；安排高风险部分先做验证切片，并定义代码评审、回归、压测和回滚门禁。",
                "基线与目标、技术拆分、接口责任矩阵、依赖顺序、人员负载、集成计划、质量门禁、风险升级记录和最终压测结果。",
                "按人数平均分任务，不考虑依赖与经验；等到最后一天才把各分支合并，并用单机微基准代表整体性能。",
                "平均分配会制造关键路径拥堵，末日集成隐藏接口问题，微基准不能证明用户路径。应按依赖和能力分工，尽早集成并用端到端指标验收。"
            ),
            en: (
                "A performance change across client, API, and database must ship in two weeks across contributors with different experience and interface freeze dates.",
                "Read the baseline and code boundaries, then set p95 latency and resource budgets. Split query/index, API batching, client rendering, and end-to-end load work with contracts, owners, dependencies, and integration dates. Validate the riskiest slice first and define review, regression, load, and rollback gates.",
                "Baseline and target, technical breakdown, interface ownership, dependency order, staffing load, integration plan, quality gates, escalations, and final load evidence.",
                "Divide work evenly by headcount, ignore dependencies and experience, merge everything on the last day, and use a local microbenchmark as the product result.",
                "This overloads the critical path, hides interface failures, and does not prove user-path performance. Staff by dependency and capability, integrate early, and accept against end-to-end metrics."
            )
        ),
        "solution_architect": .init(
            zh: (
                "现有单体要接入第三方支付并逐步拆分订单域，同时保持旧客户端和历史订单兼容。",
                "先画出现有支付、订单、库存和账务数据流，定义订单域边界与所有权。设计版本化接口、幂等键、支付回调验签、事务/补偿和失败重放；把迁移分成旁路读取、双写验证、流量灰度和旧路径下线，并为每阶段写进入、退出和回滚条件。用一次真实沙箱支付与故障注入验证关键假设。",
                "上下文与数据流图、接口和事件契约、架构决策记录、非功能预算、失败与一致性模型、迁移阶段、兼容矩阵及验证结果。",
                "只画一张目标微服务图，把支付、订单和库存之间的事务问题标成“最终一致”，没有说明所有权、补偿、迁移或旧客户端行为。",
                "技术名词不是可执行架构；边界和失败语义不清会在实现时产生互相矛盾的假设。应补齐契约、不变量、迁移和验证证据。"
            ),
            en: (
                "An existing monolith must integrate third-party payments and gradually extract the order domain while preserving old clients and historical orders.",
                "Map current payment, order, inventory, and ledger flows and assign domain ownership. Define versioned interfaces, idempotency, callback verification, transaction/compensation, and replay behavior. Stage migration through shadow reads, dual-write verification, traffic canary, and legacy retirement, each with entry, exit, and rollback conditions. Validate key assumptions with a real sandbox payment and fault injection.",
                "Context and data-flow diagrams, interface/event contracts, decision records, non-functional budgets, failure and consistency model, migration stages, compatibility matrix, and validation results.",
                "Draw only a target microservice diagram and label cross-domain transactions 'eventually consistent' without ownership, compensation, migration, or old-client behavior.",
                "Architecture vocabulary is not an executable design. Missing boundaries and failure semantics create conflicting implementation assumptions; add contracts, invariants, migration, and evidence."
            )
        ),
        "security_engineer": .init(
            zh: (
                "团队准备开放文件分享链接，需要评估越权访问、链接泄漏、撤销、审计和自动化滥用。",
                "从资产、主体、信任边界和攻击者能力建立威胁模型。为链接定义随机强度、最短有效期、范围绑定、一次性/次数限制、即时撤销和访问审计；验证跨租户枚举、重放、缓存泄漏、过期竞态和批量抓取。对无法消除的风险明确所有者和上线条件。",
                "威胁模型与滥用案例、控制矩阵、权限与数据流、测试记录、日志样例、漏洞严重度、修复门禁和剩余风险签字。",
                "为了赶发布暂时关闭鉴权，只依赖“链接足够长”，并把安全测试推到上线后。",
                "链接熵不能替代授权、撤销和审计；临时绕过还会扩大攻击面。应停止受影响发布，恢复最小权限控制并在上线前用负向用例验证。"
            ),
            en: (
                "A team plans file-sharing links and must assess unauthorized access, leakage, revocation, audit, and automated abuse.",
                "Build a threat model from assets, actors, trust boundaries, and attacker capabilities. Define entropy, short expiry, scope binding, use limits, immediate revocation, and access audit. Test cross-tenant enumeration, replay, cache leakage, expiry races, and scraping; assign residual risks and release conditions.",
                "Threat model and abuse cases, control matrix, authorization and data flow, test records, log samples, severity, remediation gate, and residual-risk sign-off.",
                "Disable authorization to meet the date, rely on a 'long enough' URL, and defer security testing until after release.",
                "URL entropy does not replace authorization, revocation, or audit, and the bypass expands exposure. Stop the release, restore least privilege, and verify negative cases before launch."
            )
        ),
        "qa_engineer": .init(
            zh: (
                "结算新增优惠叠加规则，影响金额边界、并发下单、退款、旧订单和第三方支付失败。",
                "根据资金、数据和兼容风险建立测试矩阵，使用等价类与边界值覆盖折扣上限、舍入和零金额；设计并发、重试、退款和回滚场景。先验证规则层，再做 API、数据库和沙箱支付端到端检查；缺陷记录必须包含可复现输入、影响和回归范围。最终按门禁给出“可发布/有条件/不可发布”，不替业务签字。",
                "风险到用例追踪、环境与数据版本、自动化和人工结果、缺陷证据、覆盖盲区、回归结果及带条件的发布质量结论。",
                "只跑一遍正常支付和 UI 冒烟，看到成功页面就报告“测试通过”，没有核对账务、退款或并发重复扣款。",
                "页面成功不代表资金状态正确，且遗漏最高风险失败路径。应按风险覆盖系统不变量，并让结论对应可复现证据和未测范围。"
            ),
            en: (
                "Checkout adds promotion stacking across amount boundaries, concurrent orders, refunds, legacy orders, and partner-payment failures.",
                "Build a test matrix from financial, data, and compatibility risk. Use partitions and boundaries for caps, rounding, and zero totals; cover concurrency, retries, refund, and rollback. Verify rules, API/database integration, and sandbox payment end to end. Defects include reproducible input, impact, and regression scope. Issue an evidence-based ready, conditional, or not-ready finding without replacing business sign-off.",
                "Risk-to-test traceability, environment and data version, automated/manual results, defect evidence, coverage gaps, regression results, and a conditional release-quality conclusion.",
                "Run one happy payment and UI smoke test, then report 'testing passed' without checking ledger, refunds, or duplicate charging under concurrency.",
                "A success page does not prove financial state, and the highest-risk failures are missing. Test invariants by risk and tie the conclusion to reproducible evidence and disclosed gaps."
            )
        ),
        "software_engineer": .init(
            zh: (
                "一个没有更明确前后端归属的配置同步模块需要接入现有运行时，并在冲突或断线后恢复。",
                "先读取现有模块边界和配置格式，定义版本、冲突优先级、幂等与失败语义。用最小垂直切片完成读取、比较、写入和恢复，保持已有接口与风格；补单元、集成和故障测试，并验证升级前配置仍能加载。",
                "设计决定、实现路径、配置兼容样本、测试输出、故障注入结果、性能影响、未覆盖环境、回滚说明，以及下一位维护者可以直接复现的交接步骤。",
                "另起一套配置模型绕过现有接口，只验证新安装的顺利同步，然后把模块标记为完成。",
                "第二套模型制造数据真相分叉，且未证明升级和恢复。应复用现有契约，覆盖旧版本、冲突、断线与回滚。"
            ),
            en: (
                "A configuration-sync module without a narrower frontend/backend specialization must join the current runtime and recover from conflicts or disconnects.",
                "Read module boundaries and formats; define versions, conflict precedence, idempotency, and failure semantics. Implement the smallest vertical read-compare-write-recover slice using existing interfaces and conventions. Add unit, integration, and fault tests and verify pre-upgrade configurations still load.",
                "Design decisions, implementation paths, compatibility fixtures, test output, fault-injection results, performance impact, untested environments, rollback notes, and a reproducible handoff for the next maintainer.",
                "Create a parallel configuration model, test only a clean-install happy sync, and call the module complete.",
                "A second model splits the source of truth and does not prove upgrade or recovery. Reuse current contracts and cover legacy data, conflicts, disconnects, and rollback."
            )
        ),
        "fullstack_engineer": .init(
            zh: (
                "管理后台新增批量退款，必须贯通选择交互、权限、API、数据库事务、支付方和审计。",
                "先定义可退款集合和部分失败语义，设计确认与逐项结果界面。API 使用幂等请求键并逐笔授权，事务记录本地状态后驱动支付操作；对超时支持安全查询和重试。完成真实接口集成，覆盖空选择、重复提交、混合结果、支付失败和刷新恢复。",
                "界面状态、API 契约、权限矩阵、迁移/事务说明、审计记录、端到端测试、支付沙箱结果和回滚/补偿路径。",
                "前端循环调用单笔退款接口，失败后整批重试；按钮变灰就当作防重复，也没有逐项审计。",
                "UI 状态不能提供跨网络幂等，整批重试可能重复退款。应由服务端提供幂等和明确结果模型，界面只呈现可恢复状态。"
            ),
            en: (
                "An admin console adds bulk refunds across selection UX, authorization, APIs, database transactions, payment provider, and audit.",
                "Define eligible items and partial-failure semantics, with confirmation and per-item results. Use request idempotency and per-item authorization; persist local state before payment actions and support safe status lookup/retry after timeout. Integrate the real API and test empty, duplicate, mixed, failed, and refresh-recovery paths.",
                "UI states, API contract, authorization matrix, migration/transaction notes, audit records, end-to-end tests, payment-sandbox results, and compensation path.",
                "Loop the single-refund endpoint in the browser and retry the whole batch after failure, treating a disabled button as duplicate protection with no item audit.",
                "UI state cannot ensure network idempotency and whole-batch retry may refund twice. Put idempotency and explicit result semantics on the server and render recoverable states in the client."
            )
        ),
        "frontend_engineer": .init(
            zh: (
                "订单列表需要支持十万级数据筛选和批量操作，同时满足键盘、读屏、加载、空状态和错误恢复。",
                "先核对真实 API 分页与筛选契约，采用服务端查询和稳定选择模型。实现加载骨架、空/错/部分成功状态，焦点管理和读屏反馈；为长列表设性能预算，验证慢网、返回顺序变化和权限失效。",
                "组件与状态模型、真实接口记录、键盘/读屏检查、性能测量、视觉回归、错误恢复测试、浏览器兼容结果，以及支持视口下的实现截图。",
                "把全部数据拉到浏览器内筛选，只做鼠标点击和理想态截图；API 失败时清空选择且不提示。",
                "这会在真实数据量下失效并破坏可访问性与用户信任。应遵守服务端契约，保留可恢复状态并验证真实环境。"
            ),
            en: (
                "An order list needs filtering and bulk actions across 100k records with keyboard, screen-reader, loading, empty, and recovery behavior.",
                "Confirm the real pagination/filter contract and use server-side queries with stable selection identity. Implement loading, empty, error, and partial-success states with focus and assistive feedback. Set a list performance budget and test slow networks, reordered results, and expired permission.",
                "Component and state model, real API traces, keyboard/screen-reader checks, performance measurements, visual regression, recovery tests, browser compatibility, and implementation screenshots from the supported viewport range.",
                "Fetch all rows for browser filtering, design only mouse and ideal-state screenshots, and silently clear selection when the API fails.",
                "This fails at real volume and harms accessibility and trust. Honor server contracts, preserve recoverable state, and validate in realistic conditions."
            )
        ),
        "backend_engineer": .init(
            zh: (
                "开放 API 新增批量写入端点，需要处理租户权限、幂等、部分失败、限流、事务和审计。",
                "先写版本化请求与逐项结果契约，定义整批原子还是允许部分成功。每个对象验证租户所有权，使用幂等键和唯一约束，限制批次大小并记录审计；对外部依赖超时给出可安全重试的状态。用并发、重复、越权、部分失败和回放测试验证。",
                "OpenAPI/契约、权限矩阵、数据库约束与迁移、错误语义、审计样例、集成/并发测试、指标告警和容量结果。",
                "在循环里逐条写数据库，遇错返回 500；客户端不知道前几条是否成功，也没有幂等或租户检查。",
                "结果语义不确定会造成重复写和越权。应在契约中明确原子性与逐项状态，并用服务端约束保证权限和幂等。"
            ),
            en: (
                "A public API adds bulk writes across tenant authorization, idempotency, partial failure, limits, transactions, and audit.",
                "Write a versioned request and per-item result contract, explicitly choosing atomic or partial success. Verify tenant ownership per object, enforce idempotency and uniqueness, cap batch size, and audit. Return safely queryable/retryable state for dependency timeouts; test concurrency, duplicates, unauthorized items, partial failure, and replay.",
                "OpenAPI/contract, authorization matrix, constraints and migrations, error semantics, audit samples, integration/concurrency tests, metrics, alerts, and capacity results.",
                "Write rows in a loop and return 500 on the first error, leaving clients unsure which rows committed, with no idempotency or tenant check.",
                "Ambiguous results cause duplicate writes and unauthorized access. Define atomicity and item states in the contract and enforce authorization and idempotency server-side."
            )
        ),
        "mobile_engineer": .init(
            zh: (
                "移动端新增离线盘点，恢复网络后必须同步，并处理冲突、重复提交、系统杀进程和设备权限。",
                "用持久化队列保存操作与幂等 ID，显示本地待同步状态。定义服务器版本冲突的合并/人工处理规则，后台恢复时遵守平台限制；验证飞行模式、断点重启、时钟偏差、低电量和多设备同时修改。",
                "状态与同步模型、iOS/Android 权限说明、真机矩阵、离线/重启/冲突测试、性能与电量测量、可访问性和发布检查。",
                "把待提交数据只放内存，网络恢复就全部重发，以最后写入覆盖服务器，也只在模拟器上测试。",
                "杀进程会丢数据，盲目重发会重复或覆盖他人修改，模拟器不能证明设备行为。应持久化、幂等、显式处理冲突并做真机验证。"
            ),
            en: (
                "A mobile app adds offline stocktaking that must sync after reconnecting and handle conflicts, duplicate submissions, process death, and device permissions.",
                "Persist operations with idempotency IDs and show pending state. Define merge or human-resolution rules for server version conflicts and respect platform background limits. Test airplane mode, restart, clock skew, low battery, and concurrent device edits.",
                "State/sync model, iOS and Android permission notes, device matrix, offline/restart/conflict tests, performance and battery measurements, accessibility, and release checks.",
                "Keep pending data only in memory, resend everything on reconnect with last-write-wins, and test only in a simulator.",
                "Process death loses work, blind retries duplicate or overwrite edits, and simulators do not prove device behavior. Persist, make idempotent, resolve conflicts explicitly, and validate on devices."
            )
        ),
        "desktop_engineer": .init(
            zh: (
                "macOS 客户端新增后台自动更新，需要兼容签名、公证、权限、断点恢复、运行中进程和旧版本回退。",
                "读取现有安装与更新通道，定义版本和迁移兼容。更新包必须校验签名与哈希，原子替换前保存可回退版本，并在应用忙碌时延迟安装；分别验证普通用户、只读目录、下载中断、磁盘不足、崩溃恢复和跨版本升级。",
                "安装包与签名记录、版本矩阵、系统权限测试、升级/降级数据结果、资源使用、故障注入、回滚演练和发布清单。",
                "下载完成后直接覆盖正在运行的应用，只在开发机验证一次，并把系统提示用户输入管理员密码当成正常流程。",
                "直接覆盖可能损坏安装，开发机权限与真实用户不同，任意提权扩大风险。应做签名验证、原子安装、权限最小化和多版本恢复测试。"
            ),
            en: (
                "A macOS client adds background updates across signing, notarization, permissions, resume, running processes, and rollback.",
                "Inspect install/update channels and define version/data compatibility. Verify package signature and hash, preserve a rollback version before atomic replacement, and defer while the app is busy. Test standard users, read-only locations, interrupted downloads, low disk, crash recovery, and cross-version upgrades.",
                "Package and signing records, version matrix, OS-permission tests, upgrade/downgrade data results, resource use, fault injection, rollback exercise, and release checklist.",
                "Overwrite the running app after download, test once on a developer machine, and treat an administrator-password prompt as normal.",
                "Direct replacement can corrupt installation, developer permissions differ from users, and unnecessary elevation expands risk. Use verified atomic install, least privilege, and multi-version recovery tests."
            )
        ),
        "game_engineer": .init(
            zh: (
                "多人战斗新增技能组合系统，需要保持帧率、网络同步、回放一致、存档兼容和内容工具可用。",
                "先把规则写成确定性状态转换并固定随机种子策略，客户端预测与服务器权威分离。提供可视化配置和校验工具；用延迟/丢包模拟、长局、旧存档、回放校验和目标设备剖析验证，再与策划共同跑可玩测试。",
                "玩法实现与数据格式、同步协议、性能剖析、网络模拟结果、回放哈希、兼容测试、工具链和平台构建。",
                "在客户端直接计算最终伤害并用浮点时间驱动随机效果；本机 60 FPS 就认为多人版本完成。",
                "客户端权威和非确定性会造成作弊与不同步，本机帧率也不代表网络和目标平台。应定义权威与确定性，验证回放、网络和设备。"
            ),
            en: (
                "A multiplayer game adds skill combinations while preserving frame rate, network sync, deterministic replay, save compatibility, and content tooling.",
                "Express rules as deterministic state transitions with an explicit RNG strategy and separate client prediction from server authority. Provide validated visual authoring tools; test latency/loss, long matches, old saves, replay hashes, and target-device profiles, then run playtests with design.",
                "Gameplay code and data format, sync protocol, profiles, network simulations, replay hashes, compatibility tests, tooling, and platform builds.",
                "Calculate final damage on the client and drive random effects from floating-point time, then call multiplayer complete because one machine holds 60 FPS.",
                "Client authority and nondeterminism create cheating and desync, while local frame rate proves neither networking nor target hardware. Define authority/determinism and validate replay, network, and devices."
            )
        ),
        "embedded_iot_engineer": .init(
            zh: (
                "现场设备需要 OTA 升级通信协议，必须应对断电、弱网、旧硬件、签名校验和失败回退。",
                "先建立硬件/Bootloader/固件兼容矩阵，设计 A/B 分区或安全恢复区。协议版本协商保持旧设备可用，固件包验签且防降级攻击；在硬件在环环境注入下载中断、断电、损坏包和传感器异常，验证看门狗与远程诊断。",
                "硬件和版本矩阵、协议契约、固件与构建哈希、功耗/时序测量、HIL 故障记录、签名验证、回滚和批次灰度结果。",
                "直接覆盖唯一固件分区，只验证实验室稳定网络；升级失败时要求现场人员重新刷机。",
                "这会把远程设备变砖并把恢复成本转嫁现场。应提供原子或双分区升级、签名、断点续传和可验证回退。"
            ),
            en: (
                "Field devices need an OTA protocol update that survives power loss, weak networks, old hardware, signature checks, and rollback.",
                "Build a hardware/bootloader/firmware matrix and use A/B slots or a safe recovery partition. Negotiate protocol versions, verify signed firmware, and prevent downgrade attacks. Inject interrupted download, power loss, corrupt packages, and sensor faults in hardware-in-loop tests, including watchdog and remote diagnostics.",
                "Hardware/version matrix, protocol contract, firmware/build hashes, power and timing measurements, HIL fault logs, signature tests, rollback, and staged-fleet results.",
                "Overwrite the only firmware partition, test only a stable lab network, and require field reflashing after failure.",
                "That can brick remote devices and transfers recovery cost to the field. Use atomic or dual-slot updates, signing, resume, and verified rollback."
            )
        ),
        "database_engineer": .init(
            zh: (
                "高流量订单表需要在线增加索引并迁移字段，业务不能停机，失败后还要可恢复。",
                "采集真实查询、数据量、增长率和锁等待基线，选择在线构建和分批回填。先部署兼容读写，再创建约束并切换读取；每批记录水位、校验计数和校验和，设置复制延迟/锁阈值自动暂停。完成备份恢复演练后再下线旧字段。",
                "执行计划与 SQL、Explain/基线、容量和锁评估、备份恢复证明、批次水位、主从监控、对账结果和回滚步骤。",
                "在生产高峰直接执行阻塞 DDL，看到命令返回成功就删除旧字段，没有备份、对账或应用兼容窗口。",
                "DDL 成功不代表业务无影响，立即删除会让旧版本无法回退。应分阶段兼容、监控、对账，并验证恢复后再清理。"
            ),
            en: (
                "A high-traffic order table needs an online index and column migration without downtime and with recovery.",
                "Measure real queries, volume, growth, locks, and replication. Use online build and batched backfill: deploy compatible reads/writes, create constraints, then switch reads. Record watermarks, counts, checksums, and automatic pause thresholds for lag/locks. Retire old columns only after backup recovery is exercised.",
                "Execution plan and SQL, explain/baseline, capacity and lock analysis, restore proof, batch watermarks, replication monitoring, reconciliation, and rollback steps.",
                "Run blocking DDL at peak, delete the old column when the command succeeds, and skip backup, reconciliation, and compatibility windows.",
                "DDL success does not prove business safety and immediate deletion breaks rollback. Stage compatibility, monitor and reconcile, and prove recovery before cleanup."
            )
        ),
        "devops_engineer": .init(
            zh: (
                "服务要从手工部署迁移到多环境流水线，并建立 SLO、告警、渐进发布和事故恢复。",
                "把当前构建、配置和权限差异版本化，用短期凭据和最小权限执行流水线。设置测试、安全扫描和变更审批门禁；部署采用 canary，按错误率、延迟和关键业务指标自动停止。演练回滚、配置恢复和告警通知，并把运行手册交给值班人员。",
                "可复现流水线与基础设施代码、环境差异、制品哈希、权限记录、SLO/告警、canary 结果、回滚演练和运行手册。",
                "把生产密钥写进流水线变量，所有环境共用管理员账号；部署失败后不断重跑直到成功，不知道哪些实例已经更新。",
                "长期高权限凭据和盲目重试扩大影响，且状态不可审计。应使用短期最小权限、明确部署状态、停止条件和已演练回滚。"
            ),
            en: (
                "A service moves from manual deployment to a multi-environment pipeline with SLOs, alerts, progressive delivery, and incident recovery.",
                "Version current build, configuration, and environment differences; run with short-lived least-privilege credentials. Gate tests, security scans, and approvals. Canary against error, latency, and business metrics with automatic stop. Exercise rollback, configuration restore, and notification, then hand off runbooks to on-call.",
                "Reproducible pipeline and infrastructure code, environment delta, artifact hashes, permissions, SLOs/alerts, canary results, rollback exercise, and runbooks.",
                "Store production secrets in pipeline variables, share an administrator identity across environments, and rerun failed deployment until it works without knowing updated instances.",
                "Long-lived privilege and blind retry expand impact and destroy auditability. Use short-lived least privilege, explicit deployment state, stop conditions, and exercised rollback."
            )
        ),
        "data_engineer": .init(
            zh: (
                "客户事件表要修改口径并回填一年数据，同时不能破坏实时消费、报表和下游特征。",
                "版本化事件契约并列出所有消费者，先双写新旧字段和影子计算。回填按可重入分区运行，记录水位、源目标计数和校验和；建立迟到、重复、空值和分布漂移规则。消费者完成迁移且对账通过后才停止旧字段，并保留恢复方案。",
                "契约与血缘、消费者清单、管道代码、回填水位、质量告警、计数/校验和对账、性能成本、恢复记录，以及每个受影响下游负责人的验收时间。",
                "直接改字段含义并重跑全表，任务成功就通知下游完成，未记录数据版本、重复或失败分区。",
                "任务成功不证明语义和数据一致，消费者会静默误读。应版本化、影子验证、可重入回填并逐消费者验收。"
            ),
            en: (
                "A customer-event definition changes with a one-year backfill without breaking streams, reports, or downstream features.",
                "Version the contract and enumerate consumers; dual-write and shadow-compute old/new fields. Run an idempotent partitioned backfill with watermarks, source/target counts, and checksums. Add late, duplicate, null, and drift rules. Retire old fields only after each consumer migrates and reconciliation passes, preserving recovery.",
                "Contract and lineage, consumer register, pipeline code, backfill watermarks, quality alerts, count/checksum reconciliation, performance/cost, recovery record, and dated acceptance from each affected downstream owner.",
                "Change field meaning and rerun the full table, then notify consumers when the job succeeds without versioning, duplicate checks, or failed-partition records.",
                "Job success does not prove semantic or data consistency and consumers may silently misread it. Version, shadow-validate, backfill idempotently, and accept per consumer."
            )
        ),
        "data_analyst": .init(
            zh: (
                "转化率突然下降，需要判断是产品问题、埋点变化、渠道结构还是统计波动。",
                "先冻结转化定义、时间窗口和观察单位，对事件完整率和版本发布做数据质量检查。按渠道、设备、新老用户和实验组分层，比较绝对量与比例并给置信区间；用敏感性分析验证去除异常渠道后的结论。最后区分“已证实”“支持但未证实”和“需要实验”。",
                "指标字典、数据版本、可复现查询、质量检查、样本量、分层图表、区间/敏感性结果、限制和决策建议。",
                "看到总体转化率与发布日同时下降，就断言新版本导致问题；没有检查流量结构、埋点或随机波动。",
                "时间相关不等于因果，聚合还会掩盖构成变化。应核对数据生成过程，分层并量化不确定性，必要时设计实验。"
            ),
            en: (
                "Conversion suddenly drops and the team must distinguish product behavior, instrumentation, channel mix, and statistical noise.",
                "Freeze the conversion definition, window, and unit; check event completeness and release changes. Segment channel, device, cohort, and experiment while comparing counts and rates with intervals. Test sensitivity after removing anomalous channels, then label findings as established, supported, or requiring experiment.",
                "Metric dictionary, data version, reproducible query, quality checks, sample size, segmented charts, interval/sensitivity results, limitations, and recommendation.",
                "Because aggregate conversion falls on release day, assert that the release caused it without checking traffic mix, instrumentation, or noise.",
                "Temporal correlation is not causation and aggregates hide composition. Inspect data generation, segment, quantify uncertainty, and design an experiment when needed."
            )
        ),
        "machine_learning_engineer": .init(
            zh: (
                "推荐模型准备替换线上版本，需要验证离线收益、在线延迟、偏差、漂移、冷启动和安全回退。",
                "固定训练数据快照和特征契约，检查泄漏并按时间切分评估。除总体指标外报告关键群体和冷启动表现；打包模型、代码和配置版本。在线先影子再小流量灰度，监控质量代理、延迟、错误和分布漂移，预设自动回退阈值和再训练触发。",
                "数据/特征/模型版本、训练配置、评估与切片、泄漏检查、推理压测、灰度指标、漂移监控、回退演练和模型卡。",
                "只因离线 AUC 高 2% 就全量替换模型，没有时间切分、群体分析、延迟预算或旧模型回退。",
                "单个离线平均指标不能证明线上价值和安全，甚至可能来自泄漏。应建立可追溯评估、分群门禁和渐进发布。"
            ),
            en: (
                "A recommender replaces production and needs offline gain, online latency, bias, drift, cold-start, and safe rollback validation.",
                "Pin training snapshots and feature contracts, check leakage, and use temporal evaluation. Report critical cohorts and cold-start, not only the aggregate; version model, code, and configuration. Shadow then canary online while monitoring quality proxies, latency, errors, and distribution drift with automatic rollback and retraining triggers.",
                "Data/feature/model versions, training config, evaluation slices, leakage checks, inference load tests, canary metrics, drift monitoring, rollback exercise, and model card.",
                "Replace the model globally because offline AUC is 2% higher, with no temporal split, cohort analysis, latency budget, or old-model fallback.",
                "One offline average neither proves online value nor safety and may reflect leakage. Use traceable evaluation, cohort gates, and progressive release."
            )
        ),
        "research_specialist": .init(
            zh: (
                "管理层要判断一项新技术是否适合未来两年的产品路线，但供应商、论文和社区报告结论冲突。",
                "把问题拆成成熟度、性能、生态、成本、合规和可替代性，预先定义纳入标准。优先原始来源并记录作者、日期、方法、样本和利益关系；对冲突结果解释环境差异。用小型可重复验证检验最关键主张，最后给条件化建议和待验证假设。",
                "研究问题与协议、来源台账、引用和版本、证据等级、冲突矩阵、复现结果、不确定性、适用条件和建议。",
                "引用供应商首页和几篇搜索摘要，把“被广泛讨论”当成成熟可靠，并删除不支持推荐的证据。",
                "来源有商业偏差，摘要丢失方法，选择性证据使结论不可审计。应保留正反证据、核查原文并说明不确定性。"
            ),
            en: (
                "Leadership must decide whether a technology fits the next two product years while vendor, paper, and community claims conflict.",
                "Decompose maturity, performance, ecosystem, cost, compliance, and substitutability with inclusion criteria. Prefer primary sources and record author, date, method, sample, and interests; explain conflicting environments. Reproduce the most material claim in a small test and give conditional recommendations with open hypotheses.",
                "Research questions/protocol, source register, citations and versions, evidence grading, conflict matrix, reproduction results, uncertainty, validity conditions, and recommendation.",
                "Cite a vendor homepage and search snippets, treat popularity as maturity, and omit evidence that weakens the recommendation.",
                "Commercial bias, missing methods, and selective evidence make the conclusion unauditable. Keep supporting and opposing evidence, inspect originals, and state uncertainty."
            )
        ),
        "product_designer": .init(
            zh: (
                "客服工单创建完成率低，需要从用户问题、信息架构、交互、视觉到开发验收形成闭环。",
                "结合行为数据和访谈定位用户在分类与附件处失败，重构任务流并用真实内容制作高保真原型。覆盖新建、草稿、超限、上传失败、权限和成功反馈；与目标用户做可用性测试，记录问题严重度和迭代。交付 Token/组件引用并在开发版本上复验。",
                "问题证据、旅程和任务流、原型版本、完整状态、可用性记录、无障碍检查、设计规范和开发验收差异。",
                "只凭审美重画首页理想态，未验证真实工单内容和失败路径；交付图片后不再检查实现。",
                "视觉变化没有证明任务改善，静态图还遗漏行为与边界。应基于用户证据、测试完整流程并闭环到实现。"
            ),
            en: (
                "Support-ticket completion is low and needs a loop from user problem and information architecture through interaction, visuals, and implementation acceptance.",
                "Use behavior data and interviews to locate category and attachment failures, redesign the task flow, and prototype with real content. Cover new, draft, limits, upload failure, authorization, and success. Test with target users, record severity and iterations, reference tokens/components, and recheck the built version.",
                "Problem evidence, journey/flow, prototype versions, complete states, usability records, accessibility check, design specifications, and implementation deltas.",
                "Redraw only the ideal landing page from taste, never test real ticket content or failures, and stop after handing off pictures.",
                "Visual change does not prove task improvement and static images omit behavior and boundaries. Use user evidence, test the full flow, and close the implementation loop."
            )
        ),
        "ui_designer": .init(
            zh: (
                "管理后台视觉层级混乱，需要统一 Token、组件状态、数据密度和多尺寸适配。",
                "盘点现有颜色、字号、间距和组件变体，合并等价项并定义语义 Token。为表格、表单、弹窗和导航提供默认、悬停、聚焦、禁用、错误、加载与高对比状态；用真实中英文长内容检查响应式和截断，并和前端核对可实现性。",
                "Token 清单与映射、组件状态矩阵、布局与动效标注、对比度结果、真实内容样例、资产版本和实现验收截图。",
                "只提供一张浅色桌面稿，颜色用任意十六进制值，缺少焦点、错误、暗色和长文本状态。",
                "单张理想稿不能形成系统，开发会自行猜测并造成分叉。应交付语义 Token、完整状态和可验证的实现规则。"
            ),
            en: (
                "An admin console has inconsistent visual hierarchy and needs unified tokens, component states, density, and responsive behavior.",
                "Inventory colors, type, spacing, and variants; merge equivalents into semantic tokens. Specify default, hover, focus, disabled, error, loading, and high-contrast states for tables, forms, dialogs, and navigation. Test responsive behavior with real long Chinese/English content and review feasibility with frontend.",
                "Token inventory and mapping, component-state matrix, layout/motion annotations, contrast results, real-content samples, asset versions, and implementation-review captures.",
                "Provide one light desktop mockup with arbitrary hex colors and no focus, error, dark, or long-text states.",
                "One ideal picture is not a system and forces developers to guess. Deliver semantic tokens, complete states, and verifiable implementation rules."
            )
        ),
        "ux_designer": .init(
            zh: (
                "企业新用户无法顺利开户，需要定位旅程断点并验证更简洁、可访问的任务流。",
                "明确招募标准和研究问题，观察目标用户完成真实开户，结合客服记录找出术语、材料和等待反馈问题。重组信息架构与渐进披露，制作可交互原型；测试键盘、读屏、错误修复和中途恢复，按严重度迭代并说明样本限制。",
                "研究计划与同意、原始观察摘要、旅程/任务流、问题严重度、原型版本、可用性指标、无障碍结果、限制和建议。",
                "让内部同事看一遍线框并说“挺清楚”，就认定流程可用；没有真实用户、任务或成功标准。",
                "意见不是行为证据，内部人员也缺少目标用户情境。应以真实任务观察、预定义指标和可追溯迭代验证。"
            ),
            en: (
                "New business users fail onboarding and the team must find journey breakdowns and validate a simpler accessible flow.",
                "Define recruitment and questions, observe target users performing real onboarding, and combine support evidence to find terminology, document, and feedback issues. Restructure information and progressive disclosure, prototype interactively, and test keyboard, screen reader, error repair, and resume. Iterate by severity and disclose sample limits.",
                "Research plan and consent, observation summary, journey/flow, issue severity, prototype versions, usability metrics, accessibility results, limitations, and recommendations.",
                "Ask internal colleagues to glance at wireframes and call the flow usable because they say it looks clear, without users, tasks, or success criteria.",
                "Opinion is not behavioral evidence and insiders lack user context. Validate with real tasks, predefined metrics, and traceable iteration."
            )
        ),
        "game_designer": .init(
            zh: (
                "新成长系统提高短期参与，却可能破坏战斗节奏、资源经济和长期目标。",
                "写明核心循环和不可破坏的设计支柱，建立资源来源/消耗与成长曲线。用可调原型模拟新手、中期和重度玩家，设计观察指标与访谈问题；通过多轮可玩测试检查选择是否有意义、是否出现最优套路和付费压力，再调整参数并记录理由。",
                "规则说明、数值表与假设、经济流图、原型版本、测试参与者和脚本、行为数据、质性反馈、平衡变更及验收结论。",
                "只看七日留存上涨就继续提高奖励，不检查通胀、战斗决策、玩家分层和长期疲劳。",
                "单一短期指标会掩盖经济与体验损害。应保持系统不变量，分群观察并用玩法行为和长期指标共同验证。"
            ),
            en: (
                "A progression system raises short-term engagement but may damage combat pacing, the resource economy, and long-term goals.",
                "State the core loop and inviolable pillars, then model sources, sinks, and progression curves. Use a tunable prototype for new, mid, and heavy players with observation metrics and interview questions. Playtest meaningful choice, dominant strategies, and pressure across rounds, recording every tuning rationale.",
                "Rules, tuning assumptions, economy flow, prototype versions, participant/script details, behavior data, qualitative feedback, balance changes, and acceptance finding.",
                "Keep increasing rewards because day-seven retention rose, without checking inflation, combat decisions, player cohorts, or long-term fatigue.",
                "One short-term metric hides economy and experience damage. Preserve system invariants and validate cohorts with gameplay behavior and longer-term measures."
            )
        ),
        "technical_writer": .init(
            zh: (
                "认证 API 即将开放给外部开发者，需要让首次使用者能从申请凭据走到成功调用并处理常见错误。",
                "从真实干净环境按文档执行集成，组织为概念、五分钟快速开始、端点参考、错误码与排障。所有请求/响应来自可运行样例并去除密钥；标注版本、权限和废弃策略。邀请未参与实现者按教程操作，修复每个卡点并明确内容所有者。",
                "受众与任务、来源代码/契约版本、可运行样例测试、链接检查、读者走查记录、错误恢复验证、发布版本和维护责任。",
                "从旧 README 复制示例，手工美化一个并不存在的响应，只写“调用失败请重试”，然后随 API 发布。",
                "错误示例会直接阻塞用户，模糊排障还可能导致危险重试。应从当前接口生成并实测，解释可重试条件和恢复步骤。"
            ),
            en: (
                "An authentication API is opening to external developers who must progress from credentials to a successful call and common-error recovery.",
                "Execute the integration from a clean environment and organize concepts, five-minute quickstart, endpoint reference, errors, and troubleshooting. Use runnable sanitized requests/responses; state versions, permissions, and deprecation. Have a non-implementer follow the tutorial, fix every block, and assign content ownership.",
                "Audience/task model, source contract version, runnable example tests, link checks, reader walkthrough, recovery verification, publication version, and maintenance owner.",
                "Copy an old README example, beautify a response the API never returns, and document every failure as 'retry' before publishing.",
                "False examples block users and vague retries may be unsafe. Generate and test against the current interface and explain retry conditions and recovery."
            )
        ),
        "business_analyst": .init(
            zh: (
                "企业报销跨员工、审批人和财务，现有制度有金额、地区和票据例外，系统行为还不一致。",
                "访谈各角色并观察真实案例，分别画现状与目标流程。建立术语、业务对象、决策表和例外目录，把每条需求写成可验证规则并关联来源；用正常、边界、拒绝和撤回场景与业务负责人评审，未决定的政策单独升级。",
                "访谈/制度来源、流程模型、术语表、规则与例外、需求及验收场景、追溯矩阵、冲突、业务签字，以及带负责人和期限的未决政策清单。",
                "只把财务主管口述整理成页面字段清单，忽略员工和审批例外，并把未确认政策写成系统必须实现。",
                "单一视角和字段清单不能表达业务规则，未确认政策会固化错误。应多方核实、建模例外并区分事实与决策。"
            ),
            en: (
                "Expense reimbursement spans employees, approvers, and finance with amount, region, and receipt exceptions plus inconsistent system behavior.",
                "Interview and observe each role, model current and target flows, and build vocabulary, business objects, decision tables, and exceptions. Express each requirement as a testable rule linked to its source; review normal, boundary, rejection, and withdrawal scenarios and escalate undecided policy separately.",
                "Interview/policy sources, process models, glossary, rules/exceptions, requirements and acceptance scenarios, traceability, conflicts, business sign-off, and a dated list of unresolved policy decisions with owners.",
                "Convert one finance manager's description into a screen-field list, ignore employee and approval exceptions, and encode undecided policy as a requirement.",
                "One viewpoint and a field list do not model rules, and unconfirmed policy hardens errors. Validate across roles, model exceptions, and separate facts from decisions."
            )
        ),
        "implementation_consultant": .init(
            zh: (
                "客户从旧 CRM 迁移到新系统，需要完成差距分析、配置、数据迁移、UAT、培训和周末切换。",
                "先冻结范围与成功标准，建立现状到标准能力的差距清单并控制定制。对客户、联系人和商机定义清洗、映射与所有权；至少两次演练迁移并按数量、金额和抽样对账。让业务用户用真实场景完成 UAT，切换前检查人员、通信、回退和稳定期支持。",
                "范围/差距、配置工作簿、迁移规则与对账、缺陷和 UAT 签字、培训出席与材料、切换检查表、回退演练及稳定期交接。",
                "演示环境跑通就直接导入生产全量数据，把培训邮件当作用户接受，切换失败再临时决定如何回退。",
                "演示不代表数据和业务准备，通知也不等于采用。应演练、对账、由业务验收并在切换前明确回退和支持。"
            ),
            en: (
                "A customer moves from a legacy CRM through gap analysis, configuration, migration, UAT, training, and a weekend cutover.",
                "Freeze scope and success criteria, map current needs to standard capabilities, and control customization. Define cleansing, mapping, and ownership for accounts, contacts, and opportunities; rehearse migration twice and reconcile counts, values, and samples. Have business users execute real UAT and check staffing, communication, rollback, and hypercare before cutover.",
                "Scope/gaps, configuration workbook, migration rules and reconciliation, defects and UAT sign-off, training evidence, cutover checklist, rollback exercise, and hypercare handoff.",
                "Load all production data after a demo succeeds, treat a training email as adoption, and invent rollback only after cutover fails.",
                "A demo proves neither data nor organizational readiness, and notification is not adoption. Rehearse, reconcile, obtain business acceptance, and predefine rollback/support."
            )
        ),
        "erp_consultant": .init(
            zh: (
                "集团要统一采购到付款，但各法人在税务、科目、主数据、审批额度和关账控制上不同。",
                "按组织与核算边界梳理采购申请、订单、收货、发票匹配和付款，明确三单匹配与例外。设计供应商、物料、科目和税码治理，配置职责分离与审批矩阵；用跨期、退货、预付款和汇率场景做端到端 UAT，对期初和过渡交易逐项对账。",
                "业务蓝图、组织和主数据设计、配置/扩展清单、权限分离、测试场景与签字、期初/交易对账、切换和关账验证。",
                "照搬单一公司的模板到所有法人，只测试一张标准采购单；总账余额对上就忽略未清项目、税和库存差异。",
                "集团一致性不能抹掉法定差异，单一余额也不能证明子账完整。应建模法人例外并做业务、子账和总账多层对账。"
            ),
            en: (
                "A group standardizes procure-to-pay while legal entities differ in tax, chart of accounts, master data, approval limits, and close controls.",
                "Model requisition, order, receipt, invoice match, and payment by organizational/accounting boundary, including three-way-match exceptions. Govern supplier, material, account, and tax data; configure segregation and approval. Run cross-period, return, prepayment, and FX UAT and reconcile opening and transition transactions item by item.",
                "Business blueprint, organization/master-data design, configuration/extensions, segregation, signed scenarios, opening/transaction reconciliation, cutover, and close validation.",
                "Copy one company's template to every entity, test one standard purchase order, and ignore open-item, tax, and inventory differences because the ledger total balances.",
                "Group consistency cannot erase statutory differences and one total does not prove subledger completeness. Model entity exceptions and reconcile operational, subledger, and ledger layers."
            )
        ),
        "wms_consultant": .init(
            zh: (
                "仓库上线批次与效期管理，需要贯通收货、上架、补货、波次、拣选、复核、出库、盘点和 PDA。",
                "先定义库存不变量：货主、SKU、批次、效期、状态、库位和数量任何时点可追溯。设计 FEFO、混放、冻结和差异处理，明确 ERP、自动化设备与标签接口。用短收、破损、过期、断网、撤波和盘点差异做实仓 UAT；每步核对实物、WMS 台账与 ERP。",
                "库区/库位和策略、库存不变量、设备/接口契约、标签样本、异常流程、实仓 UAT、三方库存对账、切换与回退记录。",
                "标准收货到出库走通就上线，异常靠现场口头处理；测试库存数量一致，却不核对批次、效期和状态。",
                "WMS 的关键风险在异常与库存属性，数量相等仍可能不可用。应验证属性级不变量、设备故障和可审计异常处理。"
            ),
            en: (
                "A warehouse launches lot and expiry control across receiving, putaway, replenishment, waves, picking, checking, shipping, counts, and handhelds.",
                "Define inventory invariants across owner, SKU, lot, expiry, status, location, and quantity. Design FEFO, commingling, holds, and discrepancy handling plus ERP, automation, and label interfaces. Run floor UAT for shortage, damage, expiry, disconnect, wave cancellation, and count differences; reconcile physical, WMS, and ERP at each stage.",
                "Zone/location and strategy setup, invariants, device/interface contracts, label samples, exception flows, floor UAT, three-way reconciliation, cutover, and rollback.",
                "Launch after one normal inbound-to-outbound flow, handle exceptions verbally, and check only total quantity without lot, expiry, or status.",
                "WMS risk concentrates in exceptions and inventory attributes; equal totals may still be unusable stock. Validate attribute invariants, device failures, and auditable exceptions."
            )
        ),
        "domain_expert": .init(
            zh: (
                "团队设计医疗预约规则，需要确认术语、真实流程、例外、监管边界和临床可接受场景。",
                "先声明所覆盖地区、机构和专业范围，引用现行政策与领域来源。与一线角色走查正常预约、急诊转介、取消、未到、资源冲突和敏感信息处理；把规则写成条件—动作—例外并标注依据与置信度。对超出资质或政策冲突的问题明确升级，不替代法务或临床批准。",
                "适用范围、带来源术语表、流程与规则模型、例外和风险控制、冲突来源、领域验收场景、待决策项和专家评审记录。",
                "凭个人经验断言“行业都这样做”，把一个机构习惯写成通用规则，也不说明地区、政策版本或例外。",
                "领域知识有适用边界，未经来源和多角色核验会固化错误。应标注范围、证据和置信度，并升级权威决策。"
            ),
            en: (
                "A team designs healthcare scheduling rules and must validate terminology, real workflows, exceptions, regulatory boundaries, and clinically acceptable scenarios.",
                "State jurisdiction, organization, and specialty scope and cite current policy/domain sources. Walk frontline roles through normal booking, urgent referral, cancellation, no-show, resource conflict, and sensitive data. Express condition-action-exception rules with source/confidence and escalate issues beyond qualification without replacing legal or clinical approval.",
                "Applicability boundary, sourced glossary, process/rule models, exceptions and controls, conflicting sources, domain acceptance scenarios, open decisions, and expert review.",
                "Claim 'the industry always does this' from personal experience, encode one organization's habit as universal, and omit jurisdiction, policy version, and exceptions.",
                "Domain knowledge has validity boundaries. Unsourced single-role assumptions harden errors; state scope, evidence, confidence, and escalate authoritative decisions."
            )
        ),
        "operations_specialist": .init(
            zh: (
                "内容平台投诉率上升，需要处理当前异常，并建立日常监控、响应、业务数据维护和改进闭环。",
                "先按严重度、内容类型和来源分层，核对是否因规则或数据变更导致。对当前异常按运行手册处置并保留审计；建立投诉率、处理时长、误判和积压告警，明确值班与升级。每周复盘根因和重复问题，把改进交给对应负责人并跟踪验证。",
                "事件时间线、操作和数据变更审计、分层指标、告警与阈值、SLA、异常处置结果、根因复盘、改进负责人和效果对比。",
                "为了快速降低投诉数字，直接删除争议记录或改指标口径；问题暂时消失就关闭事件。",
                "修改事实或口径会掩盖风险并破坏信任。应保留原始记录，透明说明口径，用根因和持续指标验证恢复。"
            ),
            en: (
                "A content platform's complaint rate rises and needs immediate response plus ongoing monitoring, data maintenance, and improvement.",
                "Segment by severity, content type, and source and check rule/data changes. Handle the incident through the runbook with audit; establish complaint, handling time, false-positive, and backlog alerts with on-call escalation. Review root causes weekly and assign recurring improvements with outcome tracking.",
                "Incident timeline, operation/data audit, segmented metrics, alerts and thresholds, SLA, handling results, root cause, improvement owners, and before/after evidence.",
                "Delete disputed records or redefine the metric to lower complaints, then close the incident when the chart falls.",
                "Changing facts or definitions hides risk and destroys trust. Preserve original records, disclose definitions, and verify recovery through root cause and sustained metrics."
            )
        ),
        "growth_marketing_specialist": .init(
            zh: (
                "团队计划召回沉默用户，需要选择受众、价值主张、渠道、频率，并验证增量而不是只看点击。",
                "用行为和同意状态定义可联系受众，排除敏感与退订用户。为一个明确价值主张制作渠道适配素材，核验所有效果声明和素材权利；预留随机对照组，预先定义召回、转化、投诉和退订指标及归因窗口。小流量发布后按门禁扩大，并记录频控。",
                "受众规则与规模、同意/隐私依据、主张和来源、素材版本与权利、实验设计、发送审计、增量与负面指标、归因和复盘。",
                "向所有历史邮箱群发“限时保证提升效率”，用总点击量证明活动成功，忽略退订、自然回流和重复触达。",
                "未经同意和无法证实的主张带来合规与品牌风险，总点击也不代表增量。应控制受众和频率，用对照组与负面指标评估。"
            ),
            en: (
                "A team plans dormant-user reactivation and must choose audience, claim, channel, and frequency while measuring incrementality rather than clicks.",
                "Define contactable users from behavior and consent, excluding sensitive and opted-out users. Create channel-specific assets for one substantiated value claim with rights evidence. Reserve a randomized holdout and predefine reactivation, conversion, complaint, unsubscribe, and attribution windows. Start small, gate expansion, and audit frequency.",
                "Audience rules/size, consent basis, claims and sources, asset versions/rights, experiment design, send audit, incremental and negative metrics, attribution, and review.",
                "Email every historical address with a guaranteed performance claim, use total clicks as success, and ignore opt-outs, organic return, and repeated contact.",
                "Unconsented contact and unsupported claims create compliance/brand risk, and clicks do not prove incrementality. Control audience/frequency and use holdout plus negative metrics."
            )
        ),
        "general_member": .init(
            zh: (
                "项目经理分配了一项明确任务：整理最近三次同步失败日志并形成可复现问题报告，不修改生产配置。",
                "确认范围和截止时间，读取指定日志与版本信息，按错误签名聚类并选择一个代表样本复现。记录输入、环境、时间线、预期/实际、最小步骤和证据位置；若发现需要配置修改，只提出任务就绪的建议和风险，交由项目经理安排，不擅自创建或改派 Todo。",
                "日志与版本来源、聚类规则、复现命令和真实输出、受影响范围、未知项、阻塞、建议下一任务及交接对象。",
                "看到一个相似报错就猜测根因，顺手修改生产配置验证；最后只汇报“已排查，应该是网络问题”。",
                "猜测不可复核且越过了明确排除项，模糊汇报也无法继续执行。应保留证据、复现、区分推断并把越界动作升级。"
            ),
            en: (
                "The Project Manager assigns a bounded task: organize the last three sync-failure logs and produce a reproducible issue report without changing production configuration.",
                "Confirm scope and due point, read the named logs and versions, cluster error signatures, and reproduce one representative case. Record input, environment, timeline, expected/actual, minimal steps, and evidence. If configuration change is needed, provide a task-ready recommendation and risk to the manager without creating or reassigning Todos.",
                "Log/version sources, clustering rule, reproduction command and actual output, affected scope, unknowns, blockers, next-task recommendation, and handoff owner.",
                "Guess the root cause from one similar error, modify production configuration to test it, and report only 'investigated; probably network.'",
                "The guess is not reviewable and violates the exclusion, while the vague report is not actionable. Preserve evidence, reproduce, label inference, and escalate out-of-scope actions."
            )
        ),
    ]
}
