import Foundation

extension LocalAgentProgressiveSkillExampleCatalog {
    static let projectTypeProfiles: [String: Profile] = [
        "software_development": .init(
            zh: (
                "一个既有软件模块要新增批量导入能力，但它尚不能进一步归入明确的 Web、移动端、桌面端或后端项目。",
                "先读取现有模块边界、文件格式和错误模型，确认导入是否改变持久数据。定义可验证的解析、校验、预览、提交与恢复阶段，用稳定 ID 防重复；从一个最小样本打通端到端，再覆盖空文件、编码、超限、部分错误、中断和旧版本兼容。保持现有接口与构建方式，不另造平行框架。",
                "格式与接口契约、实现和迁移、样本夹具、单元/集成/失败测试、性能边界、恢复方式、兼容结果和交付说明。",
                "只让一个理想 CSV 在开发机成功导入，错误行直接忽略；为了赶进度另写一套存储路径，未检查旧数据和重复提交。",
                "单一样本不能证明可靠性，静默忽略会污染结果，第二套存储还制造事实分叉。应遵循现有架构并验证数据、不变量和恢复。"
            ),
            en: (
                "An existing software module needs bulk import but does not fit a narrower web, mobile, desktop, or backend project type.",
                "Inspect module boundaries, formats, and error semantics and identify persistent-data impact. Define testable parse, validate, preview, commit, and recover stages with stable IDs for duplicates. Complete one vertical slice, then cover empty, encoding, limit, partial-error, interruption, and legacy compatibility while preserving current architecture.",
                "Format/interface contract, implementation and migration, fixtures, unit/integration/failure tests, performance boundary, recovery, compatibility results, and delivery notes.",
                "Import one ideal CSV on a developer machine, silently skip bad rows, add a parallel storage path, and ignore old data and duplicate submissions.",
                "One sample does not prove reliability, silent skipping corrupts outcomes, and parallel storage splits truth. Follow current architecture and verify data invariants and recovery."
            )
        ),
        "web_application": .init(
            zh: (
                "浏览器管理后台新增多租户成员管理，涉及响应式界面、登录态、角色权限、API、审计和部署。",
                "定义租户/角色/成员关系和逐动作权限，API 以服务端授权为准。界面覆盖列表、邀请、撤销、角色变更、空/错/加载和窄屏，保持 URL 与刷新可恢复；验证键盘、读屏、主流浏览器、慢网和会话过期。用真实环境走邀请到审计的端到端路径，并为数据库变更和发布准备回滚。",
                "用户流与组件状态、API/数据契约、权限矩阵、迁移、E2E 和无障碍结果、浏览器/性能矩阵、审计记录及部署回滚。",
                "仅在前端隐藏管理员按钮作为权限控制，只做宽屏 Chrome 理想态；请求成功返回后就认为成员已安全创建。",
                "前端隐藏不是授权，单一浏览器和响应也不能证明持久化与审计。应在服务端强制权限并验证真实端到端状态。"
            ),
            en: (
                "A browser admin app adds multi-tenant member management across responsive UI, sessions, roles, APIs, audit, and deployment.",
                "Define tenant-role-member relationships and action-level authorization enforced server-side. Cover list, invite, revoke, role change, empty/error/loading, and narrow layouts with recoverable URL/refresh state. Test keyboard, screen reader, browsers, slow network, and session expiry. Exercise invite through audit in a real environment with database and release rollback.",
                "User flow and component states, API/data contracts, authorization matrix, migration, E2E/accessibility results, browser/performance matrix, audit, and deployment rollback.",
                "Use hidden admin buttons as authorization, design only ideal desktop Chrome, and treat an HTTP success response as proof the member is securely created.",
                "Client hiding is not authorization and one response does not prove persistence or audit. Enforce on the server and verify real end-to-end state."
            )
        ),
        "mobile_application": .init(
            zh: (
                "移动应用新增弱网扫码收货，需要处理相机权限、离线队列、同步冲突、重复扫码和多设备行为。",
                "设计扫描—校验—本地待同步—服务端确认的状态机，每次收货使用稳定幂等 ID。相机拒绝时提供手输路径，离线时清楚显示未同步数量；定义数量或批次冲突的处理。分别在 iOS/Android 真机验证飞行模式、杀进程、恢复、低电量、无权限和多设备同时操作。",
                "任务流与状态机、平台权限文案、同步/冲突契约、真机矩阵、离线与恢复测试、性能电量、无障碍及商店发布检查。",
                "扫码后立刻显示“收货成功”，网络请求放内存后台重试；失败或杀进程时不告知用户，也没有幂等。",
                "界面成功与服务器事实不一致会造成漏收或重复。应区分本地待同步与已确认状态，持久化、幂等并验证恢复。"
            ),
            en: (
                "A mobile app adds weak-network receiving by scan with camera permission, offline queue, conflicts, duplicate scans, and multi-device behavior.",
                "Model scan, validate, locally pending, and server-confirmed states with stable idempotency IDs. Offer manual input when camera is denied and expose unsynced count offline; define quantity/lot conflict handling. Test iOS and Android devices in airplane mode, process death, recovery, low battery, denied permission, and concurrent devices.",
                "Task flow/state machine, platform permission copy, sync/conflict contract, device matrix, offline/recovery tests, performance/battery, accessibility, and store checks.",
                "Show 'received successfully' immediately, keep retries only in memory, hide failures after process death, and omit idempotency.",
                "UI success diverges from server fact and causes missing or duplicate receipts. Distinguish pending from confirmed, persist, make idempotent, and test recovery."
            )
        ),
        "desktop_application": .init(
            zh: (
                "跨平台桌面客户端新增本地文件索引，需要处理系统权限、符号链接、资源占用、数据库升级和应用更新。",
                "分别定义 Windows、macOS、Linux 的授权和文件事件差异，默认只索引用户选择目录。规范化路径并限制符号链接边界，增量索引可暂停、恢复和重建；对旧索引数据库做版本迁移。用大目录、权限变更、删除/移动、磁盘不足和升级降级测试，并限制 CPU、内存和 I/O。",
                "平台差异矩阵、权限与隐私范围、索引/数据库设计、迁移、资源剖析、文件系统边界测试、安装更新和恢复结果。",
                "启动后扫描整个用户目录并跟随所有符号链接，索引卡住时建议用户删除数据库；只在开发者的 macOS 上测试。",
                "范围过大可能泄露数据或循环扫描，删除数据库也不是可靠恢复。应最小授权、约束路径、支持重建并做跨平台验证。"
            ),
            en: (
                "A cross-platform desktop client adds local file indexing across OS permissions, symlinks, resource use, database upgrade, and app update.",
                "Define Windows/macOS/Linux authorization and file-event differences and index only user-selected roots. Normalize paths and constrain symlink traversal; make incremental indexing pausable, resumable, and rebuildable with versioned index migration. Test large trees, permission changes, move/delete, low disk, and upgrade/downgrade under CPU/memory/I/O budgets.",
                "Platform matrix, permission/privacy scope, index/database design, migrations, resource profiles, filesystem-boundary tests, install/update, and recovery results.",
                "Scan the whole user directory at startup, follow every symlink, tell users to delete the database when stuck, and test only developer macOS.",
                "Overbroad scanning leaks data or loops, and deletion is not reliable recovery. Use least scope, path constraints, rebuild support, and cross-platform validation."
            )
        ),
        "backend_service": .init(
            zh: (
                "订单服务新增合作方回调，需要处理签名校验、幂等、乱序、重试、版本兼容和可观测性。",
                "定义版本化事件契约与签名规范，先验证时间窗、签名和租户，再以合作方事件 ID 去重。状态机只接受合法转换，乱序事件进入可重放队列；响应与内部处理解耦并设置有限重试和死信。用真实测试端验证伪造、重复、超时、乱序和旧版本，同时建立积压与失败告警。",
                "事件/API 契约、状态机、权限和密钥轮换、幂等约束、重试/死信记录、集成与安全测试、指标告警和运行手册。",
                "收到回调就更新订单为成功，验签失败也返回后继续处理；重复事件靠日志人工发现，无状态转换约束。",
                "这会允许伪造、重复和状态回退，日志不能保证一致性。应先验证和去重，以持久化状态机和可重放机制处理。"
            ),
            en: (
                "An order service adds partner callbacks with signature verification, idempotency, reordering, retries, version compatibility, and observability.",
                "Define versioned events and signing; verify timestamp, signature, and tenant before deduplicating on partner event ID. Permit only legal state transitions and route out-of-order events to replay. Decouple acknowledgement from processing with bounded retry and dead letter. Test forged, duplicate, timeout, reordered, and old-version traffic and alert on backlog/failure.",
                "Event/API contract, state machine, authorization/key rotation, idempotency constraints, retry/dead-letter records, integration/security tests, metrics, alerts, and runbook.",
                "Mark orders successful on receipt, continue processing after signature failure, and rely on logs to find duplicates with no state-transition rules.",
                "This permits forgery, duplication, and state regression; logs cannot enforce consistency. Verify and deduplicate first, then use a persistent state machine and replay."
            )
        ),
        "library_sdk": .init(
            zh: (
                "公共 SDK 新增分页客户端，需要保持旧调用兼容，并为多语言使用者提供清晰升级路径。",
                "先固定旧 API 行为与兼容测试，设计惰性分页迭代器，同时保留显式页接口和取消/超时。定义速率限制、部分读取与错误传播；在支持的语言/版本矩阵运行契约和示例测试。按语义化版本发布预览包，提供迁移指南和废弃周期，并验证包元数据、签名和最小项目安装。",
                "API 决策、兼容基线、跨版本/语言测试、可运行示例、性能和内存结果、迁移文档、版本变更、包校验和发布记录。",
                "直接改变原方法返回类型，README 只给最新语法；包能上传仓库就认为发布成功，不测试旧用户升级。",
                "源/二进制兼容会被破坏，上传不代表可消费。应保留或分阶段废弃接口，验证真实安装和升级。"
            ),
            en: (
                "A public SDK adds pagination while preserving old callers and offering a clear multi-language upgrade path.",
                "Pin old behavior in compatibility tests, add a lazy iterator while retaining explicit page access and cancel/timeout. Define rate limits, partial consumption, and error propagation; run contracts/examples across supported languages and versions. Publish a preview under semantic versioning with migration/deprecation, then validate metadata, signature, and clean-project install.",
                "API decisions, compatibility baseline, cross-version/language tests, runnable examples, performance/memory, migration docs, version changes, package checks, and release record.",
                "Change the existing method's return type, document only new syntax, and call release successful when the registry accepts the package without upgrade tests.",
                "This breaks source/binary compatibility and upload does not prove consumption. Preserve or deprecate in stages and validate clean install and real upgrade."
            )
        ),
        "game_development": .init(
            zh: (
                "在线游戏新增赛季玩法，牵涉客户端、服务、内容、资源经济、反作弊和多平台发布。",
                "先定义赛季核心目标、规则、不变量和结束结算，做可玩垂直切片验证乐趣与技术风险。服务端权威处理进度与奖励，内容工具校验配置，经济模型模拟来源/消耗；安排延迟、断线、作弊、跨版本和平台性能测试。小范围灰度观察留存、平衡、崩溃和负面反馈，再决定扩大。",
                "玩法规则与数值、可玩构建、内容工具、客户端/服务契约、经济模拟、网络/反作弊/性能测试、平台包和灰度数据。",
                "先制作全部赛季美术和奖励，再发现核心循环不好玩；奖励由客户端上报，活动配置可在生产直接修改且无版本。",
                "内容投入无法挽救未经验证的循环，客户端权威和无版本配置还会带来作弊与不可恢复错误。应先验证垂直切片并治理配置。"
            ),
            en: (
                "An online game adds seasonal play spanning client, service, content, economy, anti-cheat, and multi-platform release.",
                "Define season goals, rules, invariants, and settlement and build a playable vertical slice first. Keep progress/reward server-authoritative, validate content configuration, and simulate economy sources/sinks. Test latency, disconnect, cheating, cross-version, and platform performance. Canary to a small cohort and inspect retention, balance, crashes, and negative feedback before expansion.",
                "Rules and tuning, playable build, content tools, client/service contracts, economy simulation, network/anti-cheat/performance tests, platform packages, and canary data.",
                "Produce all season art and rewards before testing the loop, accept client-reported rewards, and edit unversioned live configuration directly.",
                "Content cannot rescue an unvalidated loop; client authority and unversioned config create cheating and unrecoverable errors. Validate a vertical slice and govern config first."
            )
        ),
        "iot_embedded_system": .init(
            zh: (
                "工业传感器新增边缘告警，需要协同固件、现场总线、网关、设备云、离线恢复和 OTA 安全。",
                "明确采样频率、时钟、单位和告警滞回，版本化设备—网关—云契约。设备离线缓冲带序号与容量策略，重连幂等上传；固件签名、分批 OTA 和回退。用硬件在环注入传感器漂移、总线噪声、断网、断电、满缓冲和旧网关，验证误报、丢失和恢复。",
                "硬件/固件矩阵、信号和协议契约、功耗时序、HIL 记录、离线数据对账、安全与 OTA 结果、设备批次监控和现场回退手册。",
                "实验室连云成功就批量升级全部设备；断网数据只存易失内存，恢复后不校验顺序或重复。",
                "真实现场故障会丢数据或制造假告警，全量 OTA 放大风险。应持久化带序号数据、验证边缘条件并分批可回退发布。"
            ),
            en: (
                "An industrial sensor adds edge alerts across firmware, field bus, gateway, device cloud, offline recovery, and secure OTA.",
                "Define sampling, clock, units, and alert hysteresis and version device-gateway-cloud contracts. Buffer offline data with sequence and capacity policy and upload idempotently. Sign firmware and stage OTA with rollback. Inject drift, bus noise, disconnect, power loss, full buffer, and old gateway in HIL tests, measuring false alerts, loss, and recovery.",
                "Hardware/firmware matrix, signal/protocol contracts, power/timing, HIL logs, offline reconciliation, security/OTA results, fleet monitoring, and field rollback guide.",
                "Upgrade the whole fleet after one lab cloud connection, keep offline data only in volatile memory, and ignore order or duplication after reconnect.",
                "Field failures will lose data or create false alerts and global OTA amplifies impact. Persist sequenced data, test edge conditions, and stage a reversible release."
            )
        ),
        "enterprise_erp": .init(
            zh: (
                "企业上线采购、库存和财务集成，需要统一组织、主数据、审批、权限分离、期初和月结。",
                "从业务蓝图和控制目标出发，定义公司/工厂/仓库/核算维度与主数据所有权。优先标准配置，对扩展逐项说明必要性；建立采购到付款、库存到总账和税务场景，验证职责分离。迁移至少演练两次，按主数据、未清业务、库存和总账分层对账；完成 UAT、切换、回退和首月结演练。",
                "蓝图与组织模型、配置/扩展、权限矩阵、迁移和分层对账、UAT 签字、切换回退、培训及月结结果。",
                "按演示公司配置后直接上线，只核对总账总额；为了方便给实施人员长期超级管理员权限，业务例外用线下表补。",
                "演示配置不代表真实控制，总额相等也可能隐藏子账差异，超级权限破坏职责分离。应验证完整业务闭环与控制。"
            ),
            en: (
                "An enterprise launches integrated procurement, inventory, and finance with organization, master data, approvals, segregation, opening data, and close.",
                "Start from blueprint and controls, define company/plant/warehouse/accounting dimensions and master-data ownership. Prefer standard configuration and justify extensions. Test procure-to-pay, inventory-to-ledger, tax, and segregation. Rehearse migration twice and reconcile master data, open business, stock, and ledger separately; complete UAT, cutover, rollback, and first-close rehearsal.",
                "Blueprint and organization model, configuration/extensions, access matrix, migration and layered reconciliation, UAT sign-off, cutover/rollback, training, and close results.",
                "Launch from a demo-company configuration, reconcile only the ledger total, grant implementers permanent super-admin, and handle exceptions in offline sheets.",
                "Demo configuration does not prove controls, equal totals hide subledger differences, and super-admin breaks segregation. Validate the full operational and control loop."
            )
        ),
        "warehouse_management_system": .init(
            zh: (
                "新仓库上线波次拣选，需要贯通入库、库位、补货、库存、PDA、自动化设备、出库和盘点。",
                "建立货主、SKU、批次、效期、状态、库位和数量库存不变量，设计波次释放、短拣、撤波、冻结和设备降级流程。联调 ERP/WCS/PDA/打印并验证消息幂等；用真实动线做从预约到发运的高峰 UAT，异常后逐项核对实物、WMS 与 ERP。切换前冻结策略、盘点、回退和现场支持。",
                "库区与策略、库存不变量、接口和设备契约、标签、正常/异常 UAT、吞吐测量、三方对账、切换清单和回退结果。",
                "正常订单走通就上线，短拣和设备故障靠现场口头处理；只对账 SKU 总数量，不核对批次、状态和库位。",
                "仓储事故主要发生在异常和属性错位，总量一致不代表可用库存。应固化可审计流程并做属性级对账。"
            ),
            en: (
                "A new warehouse launches wave picking across inbound, locations, replenishment, stock, handhelds, automation, outbound, and counts.",
                "Establish inventory invariants across owner, SKU, lot, expiry, status, location, and quantity. Design wave release, short pick, cancellation, holds, and equipment degradation. Integrate ERP/WCS/handheld/print idempotently; run peak floor UAT from appointment to shipment and reconcile physical, WMS, and ERP after exceptions. Freeze strategy, count, rollback, and floor support before cutover.",
                "Zones/strategies, invariants, interface/device contracts, labels, normal/exception UAT, throughput, three-way reconciliation, cutover checklist, and rollback.",
                "Launch after a normal order works, handle short picks and equipment failure verbally, and reconcile only total SKU quantity without lot, status, or location.",
                "Warehouse incidents concentrate in exceptions and attribute mismatch; equal totals do not mean usable stock. Encode auditable flows and reconcile at attribute level."
            )
        ),
        "customer_relationship_management": .init(
            zh: (
                "销售团队迁移线索到回款流程，需要统一客户去重、商机阶段、权限、自动化和历史数据。",
                "先定义客户/联系人/线索/商机对象和唯一识别规则，明确阶段进入/退出条件与销售预测。按区域和角色配置可见性，自动化保留人工例外；迁移前清洗重复客户和负责人，分批导入并对账。让销售、主管和运营用真实报价、丢单、转交和重开场景 UAT，并跟踪使用率和数据质量。",
                "数据和阶段模型、去重规则、权限矩阵、自动化、迁移映射与对账、UAT 签字、报表一致性、培训和采用指标。",
                "把旧系统所有记录原样导入，新 CRM 用姓名去重；阶段由销售自由填写，管理员可看全部客户且无审计。",
                "脏数据和模糊阶段会破坏预测，姓名不是稳定身份，过宽权限泄露客户信息。应治理对象、规则和访问。"
            ),
            en: (
                "A sales team migrates lead-to-cash with customer deduplication, opportunity stages, permissions, automation, and history.",
                "Define account/contact/lead/opportunity objects and stable identity, plus stage entry/exit and forecast rules. Configure regional/role visibility and manual exceptions to automation. Clean duplicates and ownership before batch import and reconciliation. UAT quote, loss, transfer, and reopen with sales, managers, and operations, then track adoption and data quality.",
                "Data/stage model, dedup rules, access matrix, automation, migration mapping/reconciliation, signed UAT, report consistency, training, and adoption metrics.",
                "Import every legacy record unchanged, deduplicate by name, let reps type arbitrary stages, and grant administrators unrestricted customer access without audit.",
                "Dirty data and vague stages break forecasting, names are not identity, and broad access exposes customers. Govern objects, rules, and authorization."
            )
        ),
        "manufacturing_execution_system": .init(
            zh: (
                "工厂上线工单追溯，需要联接 ERP 工单、BOM、工艺路线、设备、质量、人员和在制品。",
                "定义工单、批次/序列号、工序、物料消耗和质量状态模型，以工艺版本锁定生产依据。设备数据带时间与来源，断线可缓冲；设计返工、报废、替代料、设备故障和人工补录的权限与审计。用真实产线做一批从下达到完工入库的追溯 UAT，核对数量、谱系、质量和 ERP 回传，并验证节拍与离线恢复。",
                "生产和追溯模型、接口/设备协议、工艺版本、异常流程、权限审计、产线 UAT、谱系查询、数量对账、性能和切换记录。",
                "只测试标准工单报工，设备断线后允许操作员随意补数据；工艺变更直接覆盖历史版本，追溯查询只看最终成品。",
                "不可审计补录和覆盖版本会破坏追溯链，最终记录无法解释在制与返工。应保持版本、来源和完整谱系。"
            ),
            en: (
                "A factory launches work-order traceability across ERP orders, BOM, routing, equipment, quality, labor, and WIP.",
                "Model work order, lot/serial, operation, consumption, and quality status and lock production to a routing version. Timestamp/source equipment data with offline buffering; govern rework, scrap, substitutions, downtime, and manual entry with authorization/audit. Run a real line batch from release to receipt, reconciling quantity, genealogy, quality, ERP response, cycle time, and recovery.",
                "Production/traceability model, interfaces/device protocols, routing versions, exception flows, authorization/audit, line UAT, genealogy queries, reconciliation, performance, and cutover.",
                "Test only normal reporting, let operators invent data after disconnect, overwrite routing history on change, and query only final goods.",
                "Unaudited entry and overwritten versions destroy genealogy, while final-only records cannot explain WIP or rework. Preserve versions, source, and complete traceability."
            )
        ),
        "ecommerce_platform": .init(
            zh: (
                "商城新增限时促销，需要保证价格、库存、订单、支付、履约、退款和风控在高并发下保持一致。",
                "明确促销资格、叠加、价格快照和库存预占不变量，订单状态机覆盖取消、超时、支付回调、发货和退款。使用幂等键、限流与防刷，账务通过订单/支付/退款三方对账。先压测峰值与热点 SKU，再沙箱故障注入和小流量灰度，监控超卖、重复扣款、支付成功未成单及退款积压。",
                "规则与状态机、价格/库存不变量、API 和事件契约、风控、压测容量、故障测试、三方对账、灰度指标、回滚和客服预案。",
                "页面倒计时结束就修改前端价格，库存用缓存自减；支付超时整单重试，活动结束后只看 GMV 判断成功。",
                "前端价格和缓存计数不是交易事实，盲目重试会重复扣款，GMV 掩盖超卖与退款。应以服务端状态机和对账守住交易不变量。"
            ),
            en: (
                "A commerce platform adds flash promotions across pricing, inventory, orders, payments, fulfillment, refunds, and fraud under concurrency.",
                "Define eligibility, stacking, price snapshot, and reservation invariants with an order state machine for cancel, timeout, callback, shipment, and refund. Use idempotency, rate limits, and abuse controls and reconcile order/payment/refund. Load-test peaks/hot SKUs, inject sandbox failure, and canary while monitoring oversell, duplicate charge, paid-without-order, and refund backlog.",
                "Rules/state machine, price and inventory invariants, API/events, fraud controls, capacity, failure tests, three-way reconciliation, canary metrics, rollback, and support plan.",
                "Change price only in the browser countdown, decrement cache stock, retry the whole order after payment timeout, and judge success only by GMV.",
                "Client price and cache counts are not transaction truth, blind retry duplicates charges, and GMV hides oversell/refund harm. Protect invariants with server state and reconciliation."
            )
        ),
        "data_analysis": .init(
            zh: (
                "管理层希望解释续费下降并决定下季度投入，需要统一指标并验证产品、价格和客群假设。",
                "与决策人确认续费分母、观察窗口、币种和取消/退款处理，冻结数据版本。先做质量与缺失检查，再按套餐、地区、获客月和使用程度分群，比较数量、比例和置信区间；对价格变更和产品使用只做符合设计的因果判断。把结果转成可执行选项，并说明样本与不可观测偏差。",
                "指标字典、数据快照、可复现查询、质量报告、分群图表、统计区间/敏感性、假设状态、限制和带取舍的建议。",
                "从一个仪表盘截图看续费下降 5%，直接建议降价；没有确认分母、退款、汇率或用户结构变化。",
                "口径和构成未确认时百分比不可解释，降价是未经验证的因果行动。应复现指标、分群并明确证据强度。"
            ),
            en: (
                "Leadership wants to explain renewal decline and allocate next-quarter investment across product, pricing, and cohort hypotheses.",
                "Confirm denominator, window, currency, cancellation, and refund handling and pin the data version. Check quality, then segment plan, region, acquisition cohort, and usage while comparing counts, rates, and intervals. Make causal claims only when design supports them. Convert results to decision options with sample and unobserved-bias limits.",
                "Metric dictionary, snapshot, reproducible queries, quality report, segmented charts, intervals/sensitivity, hypothesis status, limitations, and trade-off recommendations.",
                "See a 5% decline in a dashboard screenshot and recommend a price cut without checking denominator, refunds, FX, or cohort composition.",
                "An undefined and composition-sensitive percentage is not interpretable, and price action assumes causality. Reproduce, segment, and state evidence strength."
            )
        ),
        "data_engineering_platform": .init(
            zh: (
                "公司把批处理仓库升级为湖仓并接入实时事件，现有报表不能中断，成本也必须可控。",
                "盘点数据域、消费者、SLA 和血缘，定义分层存储、表格式、事件与批次契约。选择一个低风险域做双跑，实时与批结果按业务键、窗口和迟到策略对账；调度、质量和权限采用代码化配置。按域迁移并保留回放/回退，测量查询延迟、吞吐、存储和计算成本，直到消费者逐项验收。",
                "目标架构与 ADR、契约和血缘、迁移波次、双跑对账、质量/SLA、权限、容量与成本、灾备恢复和消费者签字。",
                "一次性把全部 ETL 改写到新平台，任务绿色就关闭旧仓库；没有消费者清单、语义对账或成本预算。",
                "调度成功不证明数据等价，全量切换无恢复路径且可能成本失控。应按域双跑、对账并逐消费者退出。"
            ),
            en: (
                "A company moves a batch warehouse to a lakehouse with streaming while reports remain available and cost stays controlled.",
                "Inventory domains, consumers, SLAs, and lineage and define storage layers, table format, event, and batch contracts. Dual-run a low-risk domain and reconcile streaming/batch by business key, window, and lateness. Codify orchestration, quality, and access. Migrate by domain with replay/rollback while measuring latency, throughput, storage, compute cost, and consumer acceptance.",
                "Target architecture/ADRs, contracts and lineage, migration waves, dual-run reconciliation, quality/SLA, access, capacity/cost, disaster recovery, and consumer sign-off.",
                "Rewrite all ETL in one cut, retire the warehouse when jobs turn green, and omit consumer inventory, semantic reconciliation, and cost budget.",
                "Green orchestration does not prove data equivalence, the cut has no recovery, and cost may explode. Dual-run by domain, reconcile, and retire per consumer."
            )
        ),
        "machine_learning_system": .init(
            zh: (
                "客服系统新增自动分类模型，需要覆盖标注、训练、评估、在线推理、人工兜底、监控和再训练。",
                "定义标签与标注一致性，固定数据/特征版本并防止时间泄漏。按类别和关键客群报告准确率、召回与校准，设置低置信度转人工；服务满足延迟和容量预算。上线先影子再灰度，监控输入/预测漂移、错误和业务结果，保留旧模型回退并规定再训练批准。",
                "数据和标签契约、标注质量、训练配置、分群评估、模型卡、服务压测、人工兜底、灰度/漂移指标、回退和再训练记录。",
                "用历史工单随机切分得到高准确率就全量自动关闭工单；没有低置信度处理、人工复核或旧模型。",
                "随机切分可能泄漏未来信息，误分类会直接伤害客户，全量自动化不可安全恢复。应时间切分、分群评估、人工兜底和渐进发布。"
            ),
            en: (
                "A support system adds automatic classification across labeling, training, evaluation, online inference, human fallback, monitoring, and retraining.",
                "Define labels and agreement, pin data/features, and prevent temporal leakage. Report accuracy, recall, and calibration by class and critical cohort; route low confidence to humans. Meet serving latency/capacity. Shadow then canary while monitoring input/prediction drift, errors, and business outcomes, preserving old-model rollback and governed retraining.",
                "Data/label contracts, annotation quality, training config, cohort evaluation, model card, serving load tests, human fallback, canary/drift metrics, rollback, and retraining record.",
                "Use a random split of historical tickets, then let the model automatically close every ticket with no confidence handling, review, or old-model fallback.",
                "Random splits may leak future information, errors directly harm customers, and global automation cannot recover safely. Use temporal/cohort evaluation, human fallback, and progressive release."
            )
        ),
        "research": .init(
            zh: (
                "项目要评估进入新行业的可行性，需要综合政策、市场、竞品、客户和技术证据。",
                "先把决策拆成市场规模、客户痛点、准入、竞争、能力差距和经济性，并定义来源与时间边界。优先法规原文、统计数据、客户访谈和可核实产品材料，记录利益关系；对市场规模用上下限和假设，区分事实、推断与未知。用访谈或原型验证最关键假设，形成进入、不进入或分阶段试点的条件。",
                "研究问题/方法、来源台账与引用、数据和计算、竞品证据、访谈记录、假设/不确定性、关键验证和条件化建议。",
                "汇总搜索结果与咨询报告摘要，挑一个最大市场规模数字，列功能表后宣布值得进入。",
                "二手摘要和最大值易受口径与销售偏差影响，功能表也不证明客户价值。应核查原始来源、量化假设并验证关键风险。"
            ),
            en: (
                "A project evaluates entry into a new industry through policy, market, competitor, customer, and technical evidence.",
                "Decompose market size, pain, entry constraints, competition, capability gap, and economics with source/time boundaries. Prefer primary regulation, statistics, interviews, and verifiable products and record interests. Use ranges and explicit assumptions, separating fact, inference, and unknown. Validate the riskiest assumption with interviews or prototype and define conditional enter, reject, or staged-pilot choices.",
                "Questions/method, source register and citations, data/calculations, competitor evidence, interviews, assumptions/uncertainty, critical validation, and conditional recommendation.",
                "Summarize search snippets and consulting abstracts, pick the largest market-size number, compare features, and declare the market attractive.",
                "Secondary abstracts and maxima hide definitions and sales bias, while feature lists do not prove value. Check primary sources, quantify assumptions, and validate critical risks."
            )
        ),
        "product_design": .init(
            zh: (
                "团队要重做企业审批体验，需要从研究、旅程与信息架构走到可用原型、视觉规则和开发验收。",
                "研究申请人、审批人和管理员的真实任务，结合日志找出等待、上下文缺失和移动处理问题。重构对象和任务流，原型覆盖创建、加签、转交、拒绝、撤回、超时和审计；用真实复杂单据做可用性与无障碍测试，依据严重度迭代。交付组件/Token 和内容规则，并在实现版本复验关键任务。",
                "问题和研究依据、旅程/架构、原型版本与完整状态、测试记录、无障碍、视觉/内容规范、开发差异和验收结果。",
                "只把审批卡片画得更漂亮，以理想的三字段单据演示；没有多角色、异常、历史信息和实现复查。",
                "美化不能解决上下文和流程问题，理想数据隐藏企业复杂性。应验证真实角色与边界并闭环到实现。"
            ),
            en: (
                "A team redesigns enterprise approvals from research, journey, and information architecture through usable prototype, visual rules, and implementation acceptance.",
                "Study real applicant, approver, and admin tasks and logs for waiting, missing context, and mobile handling. Redesign objects/flows and prototype create, add approver, delegate, reject, withdraw, timeout, and audit. Test realistic complex requests for usability/accessibility, iterate by severity, hand off components/tokens/content, and revalidate the build.",
                "Problem/research evidence, journey/architecture, prototype versions and states, test records, accessibility, visual/content specs, implementation deltas, and acceptance.",
                "Only beautify approval cards using an ideal three-field request, with no multi-role, exception, history, or implementation review.",
                "Cosmetics do not solve context and workflow, and ideal data hides enterprise complexity. Validate real roles and boundaries and close the build loop."
            )
        ),
        "design_system_brand": .init(
            zh: (
                "多个产品的品牌和组件已经分叉，需要统一 Token、组件接口、无障碍、版本迁移和治理。",
                "先盘点实际使用并聚类差异，定义品牌原则和语义 Token，而不是直接替换颜色。选择按钮、输入、导航和数据展示做参考组件，覆盖主题、状态、响应式和无障碍；提供设计与代码包、版本策略和自动视觉测试。用一个真实产品试迁移，记录破坏性差异、豁免和采用成本，再制定淘汰计划。",
                "审计清单、原则与 Token、设计/代码组件、状态和无障碍测试、版本与迁移指南、试点结果、采用指标和治理责任。",
                "发布一份新 Figma 库并要求所有团队立即替换；同名组件代码行为不同，旧 Token 无映射，也没有版本或豁免。",
                "设计文件不等于可用系统，强制切换会阻塞产品并制造新分叉。应让设计与代码同版本，试点迁移并治理例外。"
            ),
            en: (
                "Brand and components have diverged across products and need shared tokens, APIs, accessibility, version migration, and governance.",
                "Audit real usage and cluster differences, then define brand principles and semantic tokens rather than swapping colors. Build reference button, input, navigation, and data-display components across themes, states, responsive, and accessibility in design and code with versioning and visual tests. Pilot one product, record breaking deltas, exceptions, and cost, then plan retirement.",
                "Audit, principles/tokens, design and code components, state/accessibility tests, versions/migration guide, pilot results, adoption metrics, and governance owners.",
                "Publish a new Figma library and demand immediate replacement while same-name code behaves differently, old tokens have no mapping, and no versions or exceptions exist.",
                "A design file is not an operable system and forced cutover blocks teams and creates new forks. Version design/code together, pilot migration, and govern exceptions."
            )
        ),
        "novel_writing": .init(
            zh: (
                "一部长篇小说进入中段，新增反转必须延续角色动机、伏笔、时间线、视角限制和叙事声音。",
                "先更新故事圣经中的人物欲望、秘密、时间线和不可破坏事实，明确反转要改变读者理解而非凭空增加信息。列出前置伏笔与后续影响，写完整场景并检查视角人物当时能知道什么；从因果、节奏、声音和情感回报四轮修订，再让目标读者反馈困惑与预期。",
                "设定/人物与时间线版本、章节目标、伏笔回收表、完整样章、连续性检查、读者反馈和修订记录。",
                "为了惊喜临时让角色拥有从未出现的能力，并用旁白解释此前所有行为；章节很刺激就保留。",
                "无铺垫能力和事后解释破坏因果与读者信任。应让反转从既有动机与证据生长，并检查全书连续性。"
            ),
            en: (
                "A novel reaches its middle and a reversal must preserve motivation, foreshadowing, timeline, viewpoint limits, and narrative voice.",
                "Update the story bible for desires, secrets, timeline, and invariants. Make the reversal reinterpret existing information rather than invent it. Map prior clues and downstream consequences, draft a complete scene, and check what the viewpoint can know. Revise for causality, pacing, voice, and emotional payoff, then gather target-reader confusion and expectation feedback.",
                "World/character/timeline version, chapter objective, setup/payoff map, complete sample, continuity check, reader feedback, and revision record.",
                "Give a character an unforeshadowed power for surprise and use narration to explain all prior behavior, keeping it because the chapter feels exciting.",
                "Unseeded ability and retrospective explanation break causality and trust. Grow reversal from established motive/evidence and check whole-story continuity."
            )
        ),
        "general_writing": .init(
            zh: (
                "需要为专业受众写一份预算决策报告，来源复杂，事实、分析和建议必须清楚区分。",
                "先确认读者要做的决定、篇幅和语气，建立主张—证据表。优先原始来源并记录日期、版本和适用范围；先搭结论、选项、取舍和建议结构，再写正文。逐项核验数字、引语和版权，标出估算与不确定性；从逻辑、结构和语言三轮编辑，并交给不了解背景的人复核可执行性。",
                "受众/目的、提纲版本、来源与权限、主张证据表、计算、完整稿、事实核验、编辑记录、审批和发布状态。",
                "先写一个有说服力的结论，再寻找支持材料；删去相反数据，用“行业普遍认为”代替来源并直接发送。",
                "倒推证据和选择性引用会误导决策，匿名共识不可核查。应保留正反证据、标注判断并经过审批。"
            ),
            en: (
                "A budget decision report for a professional audience draws on complex sources and must separate fact, analysis, and recommendation.",
                "Confirm the reader's decision, length, and tone and build a claim-evidence table. Prefer primary sources with date, version, and applicability. Structure conclusion, options, trade-offs, and recommendation before drafting. Verify numbers, quotations, and rights, label estimates/uncertainty, edit logic/structure/language, and ask an uninformed reviewer whether action is clear.",
                "Audience/purpose, outline versions, sources/rights, claim-evidence table, calculations, full draft, fact check, edit record, approval, and publication state.",
                "Write a persuasive conclusion first, search only supporting material, remove contrary data, replace citations with 'the industry agrees,' and send directly.",
                "Backfilled and selective evidence misleads decisions, and anonymous consensus is not auditable. Keep opposing evidence, label judgment, and obtain review."
            )
        ),
        "documentation": .init(
            zh: (
                "新 API 上线，需要让首次使用者完成集成，并能诊断鉴权、限流、验证错误和服务故障。",
                "以用户任务组织信息：概念与权限、五分钟快速开始、端点参考、错误恢复和版本变更。从干净环境逐条运行示例，响应来自真实测试并脱敏；每个错误说明原因、是否可重试、退避和支持信息。自动检查代码片段、链接与 OpenAPI 差异，安排内容所有者和废弃更新。",
                "信息架构、源契约版本、可运行示例输出、错误矩阵、链接/代码测试、读者任务测试、版本发布和维护责任。",
                "把 OpenAPI 自动生成页面当作完整文档；示例未运行，401、429 和 500 都只写“请重试”。",
                "参考页不能替代学习和排障路径，错误处理错误还会造成凭据泄漏或重试风暴。应按任务验证并提供准确恢复。"
            ),
            en: (
                "A new API launches and first-time users must integrate and diagnose authentication, rate limit, validation, and service failures.",
                "Organize by task: concepts/permission, five-minute quickstart, endpoint reference, recovery, and versions. Run every sample in a clean environment with sanitized real responses. For each error state cause, retry safety, backoff, and support data. Automate snippet/link/OpenAPI drift checks and assign ownership and deprecation updates.",
                "Information architecture, source contract version, runnable sample output, error matrix, link/code tests, reader task test, release version, and maintenance owner.",
                "Treat generated OpenAPI pages as complete documentation; never run examples and tell users to retry for 401, 429, and 500 alike.",
                "Reference pages do not provide learning or recovery, and wrong retries can leak credentials or cause storms. Validate tasks and document accurate recovery."
            )
        ),
        "marketing_content": .init(
            zh: (
                "产品准备多渠道发布新品，需要统一品牌主张，同时适配社媒、销售材料、官网和邮件，并衡量效果。",
                "从目标受众、痛点和一个可证实价值主张建立信息层级，所有性能数字链接到当前证据。确认图片、引语、商标和用户数据权利；为各渠道重写长度、行动和可访问内容，而非机械裁剪。预先定义对照、归因窗口、品牌与负面指标，经过法务/品牌审批后小范围发布再扩展。",
                "受众和信息策略、主张来源、素材及权利、渠道版本、无障碍检查、审批、发布记录、实验/归因和复盘。",
                "用一个“提升 300%”标题复制到所有渠道，数字来自旧测试；未获授权就使用客户 Logo，发布后只报告曝光。",
                "过时主张和未授权素材带来法律与信任风险，曝光不代表业务效果。应核验权利和证据并按渠道与增量目标评估。"
            ),
            en: (
                "A product launches across social, sales, web, and email with one brand claim, channel adaptation, and measured outcomes.",
                "Build a message hierarchy from audience, pain, and one substantiated value claim with current evidence for every number. Confirm rights for images, quotes, marks, and user data. Rewrite length, action, and accessibility per channel rather than crop. Predefine control, attribution window, brand, and negative metrics; obtain legal/brand approval and start small.",
                "Audience/messaging strategy, claim sources, assets/rights, channel versions, accessibility checks, approvals, publication record, experiment/attribution, and review.",
                "Copy a '300% improvement' headline to every channel from an old test, use customer logos without rights, and report only impressions.",
                "Stale claims and unlicensed assets create legal/trust risk, and impressions do not prove outcomes. Verify rights/evidence and measure channel-specific incrementality."
            )
        ),
        "automation": .init(
            zh: (
                "团队要自动处理每日退款对账，并在异常时告警和转人工，目标系统存在限流与偶发超时。",
                "定义输入日期、订单/支付/退款唯一键和允许误差，先用只读影子模式对比人工结果。工作流带幂等键、水位、最小权限和审计，重试采用退避与上限，部分失败可续跑；金额不符、状态冲突和超过阈值进入人工队列。通过历史回放、重复触发、超时、限流和下游不可用测试后逐步启用写入，并提供停止开关。",
                "触发/数据契约、权限、幂等和水位、影子对比、历史回放、失败/重试测试、审计、告警、人工接管、停止与恢复记录。",
                "定时任务用管理员账号全量写入，超时无限重试；同一天重复运行会再次修改记录，失败只发一条没有对象 ID 的消息。",
                "高权限、非幂等和无限重试会放大事故，模糊告警无法恢复。应限制权限与重试，稳定标识每次影响并支持人工接管。"
            ),
            en: (
                "A team automates daily refund reconciliation with alerts and human handoff while target systems rate-limit and occasionally time out.",
                "Define input date, order/payment/refund keys, and tolerances and shadow-read against manual results first. Use idempotency, watermarks, least privilege, audit, bounded backoff, and resumable partial failure. Route amount/status conflicts and threshold breaches to humans. Replay history and test duplicates, timeout, limit, and outage before staged writes with a kill switch.",
                "Trigger/data contract, permissions, idempotency/watermarks, shadow comparison, replay, failure/retry tests, audit, alerts, human takeover, stop, and recovery records.",
                "Run full writes on an administrator account with infinite timeout retries, mutate records again on same-day rerun, and alert without affected IDs.",
                "Privilege, non-idempotency, and unbounded retry amplify incidents, while vague alerts cannot recover. Limit authority/retry, identify every effect, and support human takeover."
            )
        ),
        "operations": .init(
            zh: (
                "平台进入持续运营，需要建立内容审核、客户问题、指标异常、配置变更和改进发布的日常闭环。",
                "定义服务目标、业务指标和风险阈值，为正常操作和高风险变更编写运行手册与审批边界。所有配置/数据操作记录对象、前后值、执行者和恢复；异常按严重度响应、通信和复盘。建立值班、升级和交接，定期从重复事件生成改进项，并用后续指标验证而非只关闭工单。",
                "SLO/业务指标、值班与责任、运行手册、变更审计、事件时间线、客户沟通、复盘和改进项、恢复演练与趋势结果。",
                "依赖某个熟练成员记住所有操作；异常时直接改生产数据，恢复后不记录原因，月底用已关闭工单数证明运营良好。",
                "个人记忆和无审计修改不可持续，关闭数量会掩盖重复故障。应标准化、可追溯并以服务结果和趋势验收。"
            ),
            en: (
                "A platform enters ongoing operation and needs daily loops for content review, customer issues, metric anomalies, configuration change, and improvements.",
                "Define service objectives, business metrics, and risk thresholds with runbooks and approval boundaries for routine and high-risk actions. Audit object, before/after, actor, and recovery for every data/config change. Respond, communicate, and review by severity with on-call, escalation, and handoff; turn recurring incidents into improvements verified by later metrics.",
                "SLO/business metrics, on-call ownership, runbooks, change audit, incident timeline, customer communication, retrospectives/improvements, recovery exercises, and trends.",
                "Depend on one expert's memory, edit production data during incidents without recording why, and use closed-ticket count as proof of healthy operations.",
                "Personal memory and unaudited change are not sustainable, and closure counts hide recurrence. Standardize, trace, and accept against service outcomes and trends."
            )
        ),
        "implementation_migration": .init(
            zh: (
                "企业从旧系统迁移到新平台，需要配置、清洗、演练、UAT、培训和一个有限停机窗口切换。",
                "冻结范围、数据截止点和成功/回退条件，建立源到目标字段、转换、所有权和敏感数据规则。至少做两轮全流程演练，按对象数量、关键金额、关系和抽样记录对账；业务用户执行正常与例外 UAT。切换计划精确到依赖、人员、通信和检查点，达到停止阈值立即回退；上线后进入带指标和退出条件的稳定期。",
                "范围/差距、配置、迁移映射和脚本、演练日志、分层对账、UAT 签字、培训、切换与回退、稳定期指标和交接。",
                "把首轮全量导入当正式切换，发现脏数据就现场改脚本；只比较记录总数，没有业务签字，超过窗口仍继续。",
                "未经演练的临场修改不可复现，总数不证明关系和金额正确，超窗继续会失去安全回退。应演练、分层对账并遵守停止条件。"
            ),
            en: (
                "An enterprise migrates from a legacy system through configuration, cleansing, rehearsals, UAT, training, and a limited cutover window.",
                "Freeze scope, data cutoff, success, and rollback. Map source-target fields, transformations, ownership, and sensitive-data rules. Run at least two full rehearsals and reconcile object counts, material values, relationships, and sampled records; business users execute normal/exception UAT. Time the cutover by dependencies, staff, communication, and checkpoints, rolling back at stop thresholds, then enter metric-bound hypercare.",
                "Scope/gaps, configuration, mapping/scripts, rehearsal logs, layered reconciliation, UAT sign-off, training, cutover/rollback, hypercare metrics, and handoff.",
                "Use the first full load as production cutover, edit scripts live for dirty data, compare only row counts, omit business sign-off, and continue past the window.",
                "Unrehearsed live edits are irreproducible, counts do not prove relationships or values, and overrunning loses safe rollback. Rehearse, reconcile in layers, and obey stop conditions."
            )
        ),
        "general": .init(
            zh: (
                "一个跨领域协作项目尚不能归入更明确类型，但已有清楚目标、参与者、约束和截止时间。",
                "把目标写成可观察结果，列出范围、非目标、利益相关者和决策权。将工作拆成最小可验证交付，明确依赖、负责人、验收和风险；真实专业判断交给对应职业 Skill。建立短周期检查点，用证据更新状态；若项目逐渐呈现明确类型，再绑定更具体的项目类型而不是无限扩张通用规则。",
                "目标/范围、角色和决策权、里程碑与依赖、交付和验收证据、风险/问题、决策记录、状态和交接。",
                "因为项目是“通用”，所以不定义边界和验收；所有人都可以决定，聊天活跃就报告进展良好。",
                "通用不等于无治理，模糊权力和活跃度不能证明结果。应建立最小结构、证据和升级路径，并在可分类时收窄。"
            ),
            en: (
                "A cross-domain project does not fit a narrower type but has a clear objective, participants, constraints, and deadline.",
                "Express the objective as an observable outcome and list scope, non-goals, stakeholders, and decision rights. Split the smallest verifiable deliverables with dependency, owner, acceptance, and risk; route specialized judgment to profession Skills. Use short evidence-based checkpoints and bind a narrower project type once the shape becomes clear rather than expanding generic rules forever.",
                "Objective/scope, roles and decision rights, milestones/dependencies, deliverables and acceptance evidence, risks/issues, decisions, status, and handoff.",
                "Because the project is 'general,' define no boundary or acceptance, let everyone decide, and report good progress from active chat.",
                "General does not mean ungoverned, and activity does not prove outcomes. Establish minimum structure, evidence, escalation, and narrow the type when possible."
            )
        ),
    ]
}
