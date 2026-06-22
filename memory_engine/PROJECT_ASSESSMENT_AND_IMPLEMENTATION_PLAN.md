# Memory Engine 项目评估与实施方案

## 1. 评估范围与方法

本次评估覆盖了以下目录：

- `backend/`：Rust + Axum + MongoDB 后端服务
- `sdk/`：Rust SDK
- `frontend/`：React + Vite + Ant Design 管理台

评估方法：

- 静态代码审查：重点检查 API、Repository、Worker、SDK、前端状态流
- 本地构建与测试验证
- 本轮未做生产数据回放和数据库压测，因此性能结论主要基于代码路径、查询模式和结构设计

本地验证结果：

- 当前默认工具链下执行 `cargo check` 失败，原因是本机默认 Cargo 无法解析 `getrandom 0.4.2` 所需的 `edition2024`
- `cargo +stable check` 在 `backend/` 通过
- `cargo +stable check` 在 `sdk/` 通过
- `cargo +stable test` 在 `backend/` 通过，`20/20` 测试通过
- `cargo +stable test` 在 `sdk/` 通过，但当前没有任何测试用例
- `npm run type-check` 在 `frontend/` 通过
- `npm run build` 在 `frontend/` 通过，但 Vite 报告主包过大：`dist/assets/index-CJ0XkVUR.js` 为 `1,143.04 kB`，gzip 后 `360.98 kB`

## 2. 总体结论

这个项目并不是“完全不可维护”，相反，它已经具备了继续演进的基础：

- 后端分层基本清晰，已有 `api -> repositories -> services` 的结构
- MongoDB 索引不是空白状态，说明项目有考虑数据增长后的查询问题
- 上下文拼装、摘要相关逻辑已经有一小部分单元测试保护
- SDK 与前后端控制台已经做了边界拆分，方向是对的

当前真正的问题不是“代码乱”，而是“几个基础假设互相不一致”。最核心的风险集中在以下几类：

1. 租户、来源、线程这些身份边界定义不一致
2. 核心写接口和管理接口实际上缺少真正的认证边界
3. 若数据量上涨，若干热点路径会线性变慢
4. 前端会放大后端请求压力
5. 工程护栏不够，后续迭代容易把风险带大

综合判断：

- 这个项目适合继续做
- 但不建议在修复身份模型和认证边界之前扩大使用范围
- 下一阶段最值得做的不是“继续叠功能”，而是先完成一轮稳定性和基础模型修复

## 3. 关键问题清单

### P0：租户与来源隔离模型不一致

严重级别：致命

代码证据：

- `engine_sources` 的唯一性是按 `tenant_id + source_id` 建立的  
  `backend/src/db/schema.rs:58-65`
- 但很多写前校验只校验 `source_id`，完全不校验 `tenant_id`  
  `backend/src/api/source_guard.rs:10-33`
- SDK 鉴权时，来源查找也是只按 `source_id` 查  
  `backend/src/repositories/sources/queries.rs:44-77`
- “来源是否激活”的判断同样忽略了 `tenant_id`  
  `backend/src/repositories/sources/queries.rs:80-92`
- SDK 路由虽然会从认证上下文注入 `source_id`，但依然信任请求体里传入的 `tenant_id`  
  `backend/src/api/sdk_api/threads.rs:17-35, 57-79`  
  `backend/src/api/sdk_api/records.rs:24-49, 59-73, 123-151`  
  `backend/src/api/sdk_api/context.rs:17-28`  
  `backend/src/api/sdk_api/subject_memories.rs:20-37, 39-60`

影响：

- 如果不同租户下允许出现相同的 `source_id`，当前认证和写前校验会出现歧义
- 即使团队约定 `source_id` 全局唯一，SDK 这层仍允许调用方通过请求字段指定任意 `tenant_id`
- 这已经不只是设计不优雅，而是真实的数据隔离风险

建议：

- 将认证后的来源身份作为唯一可信来源，同时约束 `source_id` 和 `tenant_id`
- SDK 请求中的 `tenant_id` 应与 `auth.source.tenant_id` 强一致，或直接由服务端注入，不再接受自由覆盖
- 如果业务真的允许跨租户写入，需要把这个能力显式设计出来，而不是靠“请求里可传字段”隐式实现

### P0：线程与记录的身份模型互相矛盾

严重级别：致命

代码证据：

- 线程唯一键是 `tenant_id + source_id + id`  
  `backend/src/db/schema.rs:132-157`
- 记录唯一键却只有 `thread_id + id`  
  `backend/src/db/schema.rs:160-181`
- 记录写入 upsert 也只按 `thread_id + record.id` 定位  
  `backend/src/repositories/records/writes.rs:21-58`
- 多处记录和摘要逻辑只按 `thread_id` 读，不带完整作用域  
  `backend/src/repositories/records/queries.rs:8-46, 92-109`  
  `backend/src/services/context/mod.rs:43-47`  
  `backend/src/repositories/threads/writes.rs:79-97`

影响：

- 从 schema 看，`thread_id` 是租户和来源作用域下的业务键
- 但从查询和写入代码看，它又被当成了全局唯一键
- 一旦两个来源或两个租户重复使用了同一个 `thread_id`，记录很可能会冲突、串数据或统计错误

建议：

- 必须先统一身份模型，再做后续功能和性能优化
- 推荐方向：所有线程下游实体统一按 `(tenant_id, source_id, thread_id, ...)` 建立和查询
- 在迁移前先补回归测试，不要在当前模型上继续堆功能

### P0：管理接口和核心接口缺少真正认证边界

严重级别：致命

代码证据：

- 管理路由直接暴露，没有任何中间件或认证提取器  
  `backend/src/api/router/admin.rs:11-49`
- 核心路由也直接暴露，其中包括写操作和任务触发接口  
  `backend/src/api/router/core.rs:14-149`
- 当前很多“保护”只是在写前检查 `source_id` 是否存在且激活  
  `backend/src/api/source_guard.rs:10-33`

影响：

- 只要能访问服务，就有机会读写数据、触发任务、旋转密钥
- 这只能在非常受控的内网环境里勉强成立，但代码本身并没有强制这个前提

建议：

- 为 admin/core 接口补上明确的认证层
- 将 SDK 机器鉴权 与 管理员操作鉴权 分开
- 高风险管理接口加 RBAC 和审计日志

### P1：批量写入后会全量重算线程待摘要状态

严重级别：高

代码证据：

- 每次批量写入记录后，都会执行 `refresh_summary_queue_state`  
  `backend/src/repositories/records/writes.rs:69-75`
- `refresh_summary_queue_state` 会先 count，再把所有 pending record 拉出来，再逐条估 token  
  `backend/src/repositories/threads/writes.rs:73-126`

影响：

- 写入成本会随线程历史长度增长，而不是随本次 batch 大小增长
- 线程越大，写入越慢
- 也会放大与 worker 的状态竞争

建议：

- 将“每次写入后全量重算”改为“增量维护 + 异步校正”
- `pending_record_count` 和 `pending_summary_tokens` 应作为维护型字段
- 定期 reconcile 只做修复，不要每次写入都触发

### P1：Worker 调度串行且存在饥饿风险

严重级别：高

代码证据：

- worker 在一个循环中顺序执行 summary、rollup、subject memory 三类任务  
  `backend/src/jobs/worker.rs:21-149`
- 待处理线程的 token 阈值过滤，是先在 Mongo 里 limit，再在 Rust 内存里过滤  
  `backend/src/repositories/threads/queries.rs:37-65`

影响：

- 一类任务慢，会拖住后面的任务
- 真正符合阈值条件的线程，可能因为没进入前 `limit` 条结果而被跳过
- 这既影响吞吐，也影响调度正确性

建议：

- 阈值过滤尽量下推到数据库层
- 按任务类型做有界并发，而不是整个 worker 完全串行
- 进一步可以演进到 claim-based queue 或独立 work collection

### P1：前端会放大后端请求压力

严重级别：高

代码证据：

- 控制台初始化时就拉 sources、models、policies、job runs、job stats  
  `frontend/src/app/hooks/useConsoleResources.ts:15-57`
- 打开一个线程详情会触发 count、记录分页、摘要、subject memory 多个请求  
  `frontend/src/app/hooks/useThreadExplorer.ts:44-72, 91-126`
- job run 列表还会进一步按条目补 `getThread` 请求  
  `frontend/src/app/hooks/useRunManagement.ts:26-72`

影响：

- 用户未必会看的数据，也会在首屏阶段全部加载
- 单次页面交互会放大成后端多次查询
- 数据量上来后，管理台会成为一个明显的压力源

建议：

- 改成按 tab 懒加载，而不是启动即全拉
- 后端补一个聚合型线程详情接口，减少前端 fan-out
- job run 接口直接返回展示所需的 thread name，去掉 N+1 查询

### P2：时间字段与审计字段约束过弱

严重级别：中

代码证据：

- 多个核心模型把时间字段存成 `String`，不是 BSON/强类型时间  
  `backend/src/models/threads.rs:28-29`  
  `backend/src/models/records.rs:18-22`  
  `backend/src/models/sources.rs:23-24`
- `upsert_thread` 允许调用方直接传 `created_at` 和 `updated_at`，没有校验  
  `backend/src/repositories/threads/writes.rs:18-20, 52-55`

影响：

- 当前排序逻辑依赖客户端永远传标准 RFC3339 且字符串可直接比较
- 一旦数据格式不一致，分页、排序、worker 调度都可能悄悄出错
- 审计字段更适合由服务端主导，而不是外部自由写入

建议：

- 逐步改为 BSON `DateTime` 或统一的强类型时间表示
- `created_at` 在 insert 场景由服务端维护
- 如需保留外部业务事件时间，单独建字段，不与审计字段混用

### P2：前端包体过大

严重级别：中

代码证据：

- `npm run build` 产出的主 JS 包达到 `1,143.04 kB`，gzip 后 `360.98 kB`
- Vite 已经给出默认大包警告

影响：

- 管理台冷启动偏慢
- 后续继续叠功能，会进一步推高首屏成本

建议：

- 按页面/模块做代码分割
- 重 modal、重表格、重管理区块延迟加载
- 检查 Ant Design 使用方式，避免所有管理能力都进入首屏包

### P2：工程护栏还不够

严重级别：中

代码证据：

- 仓库根目录没有 `rust-toolchain.toml`、`.tool-versions` 等版本固定文件
- 当前环境里默认 `cargo check` 不稳定，但 `cargo +stable` 正常
- `frontend/package.json` 里没有 `test` 或 `lint` 脚本  
  `frontend/package.json:6-25`
- SDK 当前是 0 测试
- 后端构建时仍有 dead code / unused warning

影响：

- 构建可重复性依赖本地机器状态
- SDK 和前端更容易在重构时出现静默回归
- warning 长期不治理，后面维护成本会越来越高

建议：

- 补工具链固定和 CI
- 补 SDK 契约测试和前端最小测试基线
- 把 warning 清理纳入常规维护

## 4. 优先级建议

建议按以下顺序推进：

1. 修复身份模型与认证边界
2. 统一数据主键和查询作用域
3. 优化写路径
4. 稳定 worker 调度
5. 降低前端请求扇出和包体
6. 补齐工程护栏和 CI

这个顺序非常重要。前两项不解决，后面的性能优化可能是在一个错误的数据模型上做加速，收益有限且风险更大。

## 5. 实施方案

### 阶段 0：先稳定交付基线

目标：

- 让项目具备可重复构建、可持续迭代的最小基础

工作项：

- 增加 `rust-toolchain.toml`，固定 Rust 工具链
- 增加 CI：
  - `cargo +stable check`
  - `cargo +stable test`
  - `npm run type-check`
  - `npm run build`
- 在正式认证方案落地前，明确部署约束：
  - admin/core 接口必须放在受控内网或网关认证后面

交付物：

- 新环境可复现构建
- CI 成为合并前必过项
- 当前接口暴露风险有明确运维说明

### 阶段 1：修复身份与授权模型

目标：

- 让 `tenant_id`、`source_id`、认证上下文之间关系明确且可验证

工作项：

- 明确并文档化统一身份模型
- 推荐模型：
  - 认证后的 source 唯一映射到一个 `tenant_id + source_id`
  - SDK 请求里的 `tenant_id` 必须与认证来源一致，或者直接由服务端注入
  - 所有下游数据访问统一带完整作用域
- 重构 SDK handler，不再信任自由传入的租户信息
- 为 admin/core 补 auth middleware 或 extractor
- 为密钥旋转、手工触发任务等高风险操作补审计日志

交付物：

- 鉴权后的作用域是确定的
- 不再存在“请求随手传一个 tenant_id 就生效”的写路径
- 补齐以下回归测试：
  - 租户不匹配拒绝
  - 相同 `source_id` 的作用域行为
  - 未授权 admin/core 调用拒绝

### 阶段 2：统一数据主键与查询模型

目标：

- 让集合主键、索引、Repository 过滤条件完全一致

工作项：

- 逐集合确认 canonical key
- 推荐调整：
  - records：纳入 `tenant_id + source_id + thread_id + id`
  - snapshots：纳入 `tenant_id + source_id + thread_id + snapshot_type + turn_id`
  - 线程下游所有查询统一带完整作用域
- 制定迁移脚本与数据校验脚本
- 清理掉那些“作用域不完整也能查”的 repository 方法

交付物：

- 一份明确的数据键设计文档
- 在测试/预发布环境完成迁移验证
- Repository 级隔离测试通过

### 阶段 3：优化写路径与 worker 执行模型

目标：

- 降低写入成本，提高调度吞吐

工作项：

- 把 pending 状态统计从全量重算改为增量维护
- 增加 reconcile 任务做异步校正
- 将 token threshold 过滤下推到数据库层
- worker 侧按任务类型引入有界并发
- 增加基础指标：
  - batch sync latency
  - pending queue size
  - 各任务成功/失败次数
  - 单线程摘要耗时

交付物：

- 批量写入成本不再跟全线程长度线性绑定
- worker 不再因为 limit 后内存过滤导致遗漏符合条件任务
- 有基础可观测性，能看队列与处理健康度

### 阶段 4：优化管理台数据流

目标：

- 减少无效请求，提升操作体验

工作项：

- 改成按 tab 懒加载
- 后端增加聚合接口：
  - thread detail bundle
  - 带 thread display 信息的 job run 查询
- 合并 count 与 page 数据加载，减少双请求
- 对重模块做代码分割

交付物：

- 首屏请求数下降
- job run 列表不再有明显 N+1 查询
- 构建结果不再出现默认大包警告

### 阶段 5：补测试与质量体系

目标：

- 为后续重构提供稳定保护

工作项：

- 补后端集成测试：
  - source auth
  - tenant isolation
  - batch sync
  - worker 选择逻辑
- 补 SDK 契约测试
- 补前端关键 hooks 的最小测试基线
- 增加 lint 或 warning budget

交付物：

- SDK 不再是 0 测试
- 前端关键数据流具备基本保护
- 身份隔离和鉴权回归可以在合并前被拦住

## 6. 建议排期

假设是一个以后端为主、前端部分参与的小团队，可以按 4 周执行：

### 第 1 周

- 完成阶段 0
- 启动阶段 1 的认证与作用域修复
- 如尚未受控，先冻结外部暴露面

退出标准：

- CI 可用
- admin/core 当前部署约束清晰
- SDK 租户覆盖规则已被修复或被禁止

### 第 2 周

- 完成阶段 1
- 开始阶段 2 的主键和查询模型统一

退出标准：

- 身份模型文档定稿
- 关键读写路径已使用完整作用域
- 隔离性回归测试通过

### 第 3 周

- 完成阶段 3 的写路径和 worker 优化

退出标准：

- batch sync 不再每次触发全线程 pending 扫描
- worker 不再依赖 limit 后的内存阈值过滤
- 基础指标可观测

### 第 4 周

- 完成阶段 4 和阶段 5

退出标准：

- 管理台请求扇出降低
- 主包警告消失
- SDK 与前端最小测试基线建立

## 7. 验收标准

实施完成后，至少应满足以下条件：

- SDK 调用方不能通过修改请求字段访问或写入其他租户数据
- admin/core 接口不能在无授权条件下直接调用
- 线程下游实体的索引、唯一键和查询条件保持一致
- 批量写入成本与批次规模相关，而不是与整个线程历史长度相关
- worker 对候选任务的选择不再依赖“先 limit 再内存过滤”的方式
- 前端常见操作不再触发明显 N+1 请求
- 新环境下可稳定复现构建，不依赖本机默认工具链偶然状态

## 8. 最终建议

这次工作不建议当成一次“小优化”。

更准确地说，这是一次“基础模型稳定化”项目。最值得投入的下一步，是先把身份模型、认证边界、写路径正确性修好。等这几项打稳之后，再继续做性能优化和功能扩展，投入产出比会高很多，后续风险也会明显下降。
