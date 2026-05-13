# Chatos RS 优化方案

## 目标

这份方案聚焦两件事：

1. 抽象项目里已经重复出现、后续还会继续扩散的逻辑。
2. 拆解职责过多的大文件，降低维护成本、测试成本和变更风险。

本次判断主要基于仓库结构、核心源码热点、现有治理脚本，以及几个高频模块的职责分布情况。

## 当前观察

### 1. 热点主要集中在四块

- `chat_app_server_rs/` 是绝对核心区，源码文件数远高于其他模块。
- `chat_app/` 前端状态管理和会话交互逻辑较重。
- `openai-codex-gateway/` 代码量不大，但入口文件职责过于集中。
- `db_connection_hub/` 多数据库驱动已经出现明显的“同构实现”。

### 2. 需要优先关注的大文件

- `chat_app_server_rs/src/services/chatos_skills.rs`：1692 行
- `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs`：1453 行
- `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs`：1255 行
- `chat_app_server_rs/src/services/code_nav/languages/go/mod.rs`：1081 行
- `chat_app_server_rs/src/services/code_nav/languages/python/mod.rs`：1033 行
- `openai-codex-gateway/server.py`：839 行
- `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`：733 行
- `db_connection_hub/backend/src/drivers/sqlserver/metadata/detail.rs`：671 行
- `db_connection_hub/frontend/src/components/workbench/ConnectionModal.tsx`：483 行

### 3. 现有治理已经开始，但覆盖还不够

仓库里已经有：

- `scripts/check-large-files.sh`
- `scripts/check-hotspot-line-budgets.sh`

说明项目已经意识到“大文件”和“热点文件”问题。但当前 `check-hotspot-line-budgets.sh` 里的预算清单偏旧，很多新的热点文件并没有进入治理范围，比如：

- `chatos_skills.rs`
- `chatos_memory_engine/mod.rs`
- `openai-codex-gateway/server.py`
- `useSessionWorkbarPanels.ts`
- 各语言 `code_nav` provider

## 可以抽象出来的逻辑

### A. `db_connection_hub` 的 metadata 节点解析与分页

#### 现象

以下文件存在非常相似的实现：

- `db_connection_hub/backend/src/drivers/mysql/metadata/common.rs`
- `db_connection_hub/backend/src/drivers/postgres/metadata/common.rs`
- `db_connection_hub/backend/src/drivers/sqlserver/metadata/common.rs`
- `db_connection_hub/backend/src/drivers/sqlite/metadata/common.rs`
- `db_connection_hub/backend/src/drivers/mongodb/metadata/common.rs`
- `db_connection_hub/backend/src/drivers/oracle/metadata/common.rs`

重复内容包括：

- `paginate_nodes`
- `make_db_node`
- `parse_database_node`
- `parse_schema_node`
- `parse_table_node`
- `parse_index_node`
- `parse_trigger_node`
- 各类 `detail/relation/collection` 节点的字符串拆分

#### 建议抽象

新增类似 `db_connection_hub/backend/src/drivers/metadata_common/` 的共享层，拆成：

- `pagination.rs`
- `node_factory.rs`
- `node_parser.rs`
- `scope.rs`

建议把“节点字符串协议”抽成统一模型，例如：

- `NodeId`
- `ParsedDatabaseNode`
- `ParsedSchemaNode`
- `ParsedRelationNode`

不同数据库只保留差异化部分：

- 节点层级差异
- 特定对象类型差异
- 数据库作用域校验规则
- SQL/元数据查询差异

#### 收益

- 减少驱动之间复制粘贴。
- 新增数据库驱动时，不需要重新手写一套 `split(':')` 逻辑。
- 节点协议变更时只改一处。

### B. `chat_app_server_rs` 的 code navigation 语言 provider 框架

#### 现象

`chat_app_server_rs/src/services/code_nav/languages/` 下已经有 11 个语言目录。  
其中多个 `mod.rs` 呈现相似结构，尤其是：

- `go/mod.rs`
- `python/mod.rs`
- `java/mod.rs`
- `rust/mod.rs`
- `c/mod.rs`
- `cpp/mod.rs`
- `csharp/mod.rs`

这些文件普遍包含：

- ignored dirs / extension 常量
- regex 规则集
- 文件分析结构体
- `definition`
- `references`
- `document_symbols`
- 文件扫描和搜索逻辑
- indexed symbol 转换逻辑

#### 建议抽象

保留每种语言的语法差异，但抽出统一骨架：

- `provider_base.rs`
- `file_scan.rs`
- `search.rs`
- `symbol_projection.rs`
- `import_resolution.rs`

可抽象的公共能力：

- 目录遍历和忽略规则组合
- 文本搜索结果结构
- symbol -> `IndexedSymbol` -> `NavLocation` 转换
- 去重、排序、截断
- 基于 token 的候选收集模板

语言目录建议从单文件改为子模块：

- `languages/java/mod.rs`
- `languages/java/analyzer.rs`
- `languages/java/definition.rs`
- `languages/java/references.rs`
- `languages/java/symbols.rs`
- `languages/java/imports.rs`

Go、Python、Rust 同理。

#### 收益

- 后续支持新语言时只需要补“语言差异”，不是复制整套 provider。
- 减少 provider 之间逻辑漂移。
- 单文件过千行的问题会自然下降。

### C. `chatos_skills.rs` 的技能导入/发现/安装子系统

#### 现象

`chat_app_server_rs/src/services/chatos_skills.rs` 已经不是一个普通 service 文件，而是一个完整子系统，内部混合了：

- 列表与详情查询
- 缓存技能发现
- plugin 缓存刷新
- markdown/frontmatter 解析
- git 仓库拉取与缓存
- marketplace 候选解析
- 文件复制与安装路径计算
- DTO 转换和分页排序

#### 建议抽象

按职责拆成目录：

- `services/chatos_skills/mod.rs`
- `services/chatos_skills/query.rs`
- `services/chatos_skills/discovery.rs`
- `services/chatos_skills/install.rs`
- `services/chatos_skills/git_cache.rs`
- `services/chatos_skills/plugin_manifest.rs`
- `services/chatos_skills/markdown.rs`
- `services/chatos_skills/pathing.rs`
- `services/chatos_skills/mapping.rs`

核心边界建议：

- `query.rs` 只关心 repo + DTO 输出
- `discovery.rs` 只负责扫描本地缓存和构建领域对象
- `install.rs` 负责编排安装流程
- `git_cache.rs` 只负责 repo clone/fetch/checkout
- `markdown.rs` 只负责 frontmatter 和 skill entry 提取

#### 收益

- 技能安装问题更容易定位。
- 后续接入远端 marketplace 或 zip/source registry 更容易扩展。
- 测试可以按模块补齐，而不是围着一个 1692 行文件写集成测试。

### D. `chatos_memory_engine/mod.rs` 的“会话 + 消息 + 摘要 + 快照 + repair”混合问题

#### 现象

`chat_app_server_rs/src/services/chatos_memory_engine/mod.rs` 目前同时承载：

- session 创建、更新、归档、查询
- message 列表/写入/删除
- summary 列表/删除
- review repair 运行与状态查询
- turn runtime snapshot 同步和读取
- project memory / agent recall 查询
- engine DTO 到本地模型转换

这说明它更像一个 facade，但现在把 facade、mapping、use case 都塞在一起了。

#### 建议抽象

按 use case 拆目录：

- `services/chatos_memory_engine/mod.rs`
- `services/chatos_memory_engine/client.rs`
- `services/chatos_memory_engine/sessions.rs`
- `services/chatos_memory_engine/messages.rs`
- `services/chatos_memory_engine/summaries.rs`
- `services/chatos_memory_engine/review_repair.rs`
- `services/chatos_memory_engine/snapshots.rs`
- `services/chatos_memory_engine/project_memory.rs`
- `services/chatos_memory_engine/mappers.rs`

`mod.rs` 只保留 facade 导出。

#### 收益

- memory engine 的对外接口会更清楚。
- 每个 use case 都能单独补测试和指标。
- 能有效降低未来对 `mod.rs` 的继续堆积。

### E. 前端 `sendMessage` 发送链路的编排层

#### 现象

`chat_app/src/lib/store/actions/sendMessage.ts` 行数不算夸张，但它已经处于“编排器过重”状态。  
虽然同目录已经拆出 26 个辅助文件，主入口仍然混合了：

- runtime 解析
- metadata 合并
- optimistic UI 建立
- request payload 构造
- realtime / SSE 双通道选择
- fallback 策略
- 发送前后的 store 更新

#### 建议抽象

把 `sendMessage.ts` 收缩成真正的 orchestration entry，只保留主流程。  
进一步拆出：

- `sendMessage/orchestrator.ts`
- `sendMessage/transportSelector.ts`
- `sendMessage/optimisticState.ts`
- `sendMessage/sessionRuntimeSync.ts`
- `sendMessage/requestContext.ts`

其中：

- transport 选择独立
- optimistic 消息创建与状态推进独立
- session metadata 回写独立

#### 收益

- 发送链路故障更容易定位是在 transport、state 还是 payload。
- 更容易补充“重试 / 中断 / fallback”场景测试。

### F. 前端 workbar / panel 同步逻辑

#### 现象

`chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts` 733 行，单 hook 覆盖：

- task review panel
- UI prompt panel
- realtime 接入
- cache 回填
- mutation guard
- summary/history 拉取
- 面板开合和工作区状态联动

#### 建议抽象

按子域拆 hook：

- `useTaskReviewPanels.ts`
- `useUiPromptPanels.ts`
- `useWorkbarRealtimeSync.ts`
- `useWorkbarPendingCache.ts`
- `useTaskRealtimeMutationGuard.ts`

当前文件保留为组合层。

#### 收益

- React hook 的依赖更清晰，避免一个 hook 带出大面积重新渲染或闭包复杂度。
- 后续调 panel 行为时不容易误伤另一类 panel。

### G. `openai-codex-gateway/server.py` 入口职责过重

#### 现象

`openai-codex-gateway/server.py` 同时承担：

- HTTP server 启动
- request 路由分发
- turn 生命周期处理
- event 流转
- tool call 白名单拦截
- SSE 输出
- bridge 到 app server 的适配

入口文件更像“框架 + 业务 + 传输协议”糅在一起。

#### 建议抽象

拆为：

- `server.py`
- `gateway_http/handler.py`
- `gateway_http/routes.py`
- `gateway_runtime/turn_state.py`
- `gateway_runtime/turn_notifications.py`
- `gateway_runtime/tool_guard.py`
- `gateway_runtime/codex_bridge.py`

`server.py` 只负责：

- 解析配置
- 初始化依赖
- 启动 server

#### 收益

- gateway 的边界更清晰。
- 更容易做无 HTTP 的单元测试。
- 后续支持更多 route 或 transport 时改动面更小。

## 建议优先拆解的大文件

### P0：优先级最高

1. `chat_app_server_rs/src/services/chatos_skills.rs`
2. `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs`
3. `openai-codex-gateway/server.py`
4. `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`

理由：

- 单文件职责明显跨边界。
- 改动频率高的概率大。
- 对理解成本和回归风险影响最直接。

### P1：第二优先级

1. `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs`
2. `chat_app_server_rs/src/services/code_nav/languages/go/mod.rs`
3. `chat_app_server_rs/src/services/code_nav/languages/python/mod.rs`
4. `db_connection_hub/backend/src/drivers/sqlserver/metadata/detail.rs`
5. `db_connection_hub/frontend/src/components/workbench/ConnectionModal.tsx`

理由：

- 已经很大，但更适合配合“框架抽象”一起拆，而不是孤立切文件。

## 分阶段实施建议

### 第一阶段：建立治理基线

建议先做低风险治理，不碰业务行为：

1. 更新 `scripts/check-hotspot-line-budgets.sh`
2. 把当前热点文件纳入预算
3. 为核心目录补一个“模块职责说明”文档
4. 为超 800 行文件建立强提醒阈值

建议新增预算对象：

- `chat_app_server_rs/src/services/chatos_skills.rs`
- `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs`
- `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs`
- `chat_app_server_rs/src/services/code_nav/languages/go/mod.rs`
- `chat_app_server_rs/src/services/code_nav/languages/python/mod.rs`
- `openai-codex-gateway/server.py`
- `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`

### 第二阶段：先拆“职责混合型”文件

优先处理：

1. `chatos_skills.rs`
2. `chatos_memory_engine/mod.rs`
3. `server.py`
4. `useSessionWorkbarPanels.ts`

原则：

- 先目录化，再迁函数。
- 先不改接口，只改文件边界。
- 每次拆分后补 smoke test 或现有测试回归。

### 第三阶段：处理“同构复制型”模块

优先处理：

1. `db_connection_hub` metadata common
2. `code_nav` 语言 provider 框架

原则：

- 先提公共模型和 helper。
- 再迁移 1 到 2 个代表性语言/驱动验证抽象是否稳定。
- 最后批量迁移剩余实现。

建议先选：

- `db_connection_hub`: postgres + mysql
- `code_nav`: go + python

这样能覆盖两类不同但相近的实现。

### 第四阶段：前端状态与实时链路收口

优先处理：

1. `sendMessage` 编排层继续瘦身
2. workbar/panel hooks 细分
3. `lib/store/actions` 下继续把领域逻辑下沉到 domain/service helper

## 风险提示

### 1. 不要只按行数拆

有些文件虽然大，但如果是稳定的数据表、fixture 或生成代码，不值得优先拆。  
这次方案优先处理的是“经常改、职责混、测试难”的文件。

### 2. 抽象过早会制造反向复杂度

例如 `code_nav` 的不同语言并不完全一致，不能一开始就做过度统一。  
应先抽“确定重复”的部分：

- 遍历
- 搜索
- 去重
- location 转换

把语法规则和 import 解析留在语言模块内部。

### 3. `db_connection_hub` 的驱动抽象要防止过厚

不要把所有数据库差异都塞进一个 trait。  
更适合先抽字符串协议和节点分页这类稳定共性，再逐步抽元数据查询骨架。

## 推荐执行顺序

1. 更新 hotspot 预算脚本。
2. 拆 `chatos_skills.rs`。
3. 拆 `chatos_memory_engine/mod.rs`。
4. 拆 `openai-codex-gateway/server.py`。
5. 拆 `useSessionWorkbarPanels.ts`。
6. 抽 `db_connection_hub` metadata 公共层。
7. 抽 `code_nav` provider 公共骨架。
8. 继续收缩前端 `sendMessage` 编排层。

## 预期效果

如果按这个顺序推进，预期会带来这些收益：

- 新人理解成本明显下降。
- 大文件冲突和回归风险下降。
- 重复逻辑收敛后，后续功能开发速度会更稳。
- 单元测试可以围绕职责边界建立，而不是只能做大集成测试。
- 未来继续做模块化治理时，有明确的优先级和落点。

## 附：适合作为后续任务的拆分主题

- `refactor: split chatos_skills into discovery/install/query modules`
- `refactor: split chatos_memory_engine facade by use case`
- `refactor: modularize openai-codex-gateway server entry`
- `refactor: break useSessionWorkbarPanels into domain hooks`
- `refactor: extract db_connection_hub metadata node parsing common layer`
- `refactor: extract shared code_nav provider pipeline`
- `chore: refresh hotspot line budget policy`
