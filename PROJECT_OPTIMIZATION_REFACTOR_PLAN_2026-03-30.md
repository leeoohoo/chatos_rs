# 项目优化与重构审查方案

日期：2026-03-30

## 1. 总体判断

这个项目不属于“整体失控”的状态，但复杂度明显集中在少数几个核心文件和运行目录里。

- 本次审查范围：`659` 个源码文件，约 `123,858` 行 `ts/tsx/rs`
- 当前 git 跟踪文件数：`751`
- 主要问题形态：少数核心文件承担了过多职责
- 次要问题形态：编译产物、运行数据、本地缓存、复制文档占用了大量仓库空间

我的结论是：

- 这个仓库适合做渐进式重构，不适合推倒重来
- 第一优先级是仓库卫生和边界清理
- 第二优先级是拆分聊天链路、agent builder、大型前端容器组件
- 第三优先级是补类型、补测试、收敛协议

## 2. 体积概览

### 目录体积热点

- `chat_app_server_rs/target`：`53G`
- `memory_server/backend/target`：`6.9G`
- `chat_app/node_modules`：`594M`
- `memory_server/frontend/node_modules`：`204M`
- `chat_app_server_rs/docs/codex`：`400M`
- `chat_app_server_rs/data/chat_app.db`：`80M`
- `chat_app_server_rs/logs`：`21M`
- `memory_server/backend/data/memory_server.db`：`5.1M`

### 仓库卫生问题

我确认到以下运行态文件目前仍在 git 跟踪范围里：

- `openai-codex-gateway/gateway_state.sqlite3`
- `openai-codex-gateway/gateway_state.sqlite3-shm`
- `openai-codex-gateway/gateway_state.sqlite3-wal`

这三个文件本质上是运行时数据库，不应该继续留在版本管理里。

另外仓库里还存在不少本地产物：

- 多个 `.DS_Store`
- 多个 `__pycache__`
- 多个 `.pyc`
- `chat_app_server_rs/docs/codex` 下存在嵌套复制内容

## 3. 主要大文件

下面这份列表已经排除了明显的 generated / vendor 文件，保留的是更值得优先处理的业务文件。

| 领域 | 文件 | 行数 | 说明 |
| --- | --- | ---: | --- |
| Memory 后端 | `memory_server/backend/src/services/agent_builder.rs` | 2410 | 非常典型的 god file |
| Memory 前端 | `memory_server/frontend/src/pages/AgentsPage.tsx` | 1292 | 列表、编辑、AI 创建、预览、会话记录混在一起 |
| Rust 后端 | `chat_app_server_rs/src/services/v3/ai_request_handler/parser.rs` | 1119 | 协议解析器过大 |
| 前端 store | `chat_app/src/lib/store/actions/sendMessage.ts` | 885 | optimistic UI、SSE、工具面板、错误处理、runtime 合并都在一起 |
| 前端 UI | `chat_app/src/components/SessionList.tsx` | 881 | 状态和动作编排过重 |
| Memory 前端 | `memory_server/frontend/src/pages/JobConfigsPage.tsx` | 852 | 管理端配置职责集中 |
| 前端数据层 | `chat_app/src/lib/api/client.ts` | 831 | API 面过大，`Promise<any>` 很多 |
| Rust 后端 | `chat_app_server_rs/src/core/chat_runtime.rs` | 826 | metadata 兼容逻辑和运行时解析热点 |
| Rust 后端 | `chat_app_server_rs/src/api/chat_v3.rs` | 818 | API、鉴权、SSE、runtime guidance、编排混在一起 |
| Memory 后端 | `memory_server/backend/src/repositories/agents.rs` | 779 | repository + normalize + hydrate 逻辑过密 |
| Memory 前端 | `memory_server/frontend/src/i18n.tsx` | 772 | 文案字典直接嵌在运行模块里 |
| 前端 UI | `chat_app/src/components/ProjectExplorer.tsx` | 729 | 即使已经抽了 hook，主容器仍然过重 |

## 4. 主要优化与重构点

### P0. 先处理仓库卫生和输出物管理

依据：

- `chat_app_server_rs/target` 已达 `53G`
- `memory_server/backend/target` 已达 `6.9G`
- `chat_app_server_rs/docs/codex` 达到 `400M`
- `openai-codex-gateway` 下的 sqlite 运行文件仍被 git 跟踪

问题：

- 本地运行态数据和源码目录混在一起
- Rust 编译输出在多个模块内重复堆积
- 复制文档和缓存让仓库看起来比源码本身大很多
- 清理成本高，拉起成本高，磁盘压力大

建议动作：

1. 停止跟踪 `openai-codex-gateway/gateway_state.sqlite3*`
2. 扩展忽略规则，补上 `*.sqlite3`、`*.sqlite3-shm`、`*.sqlite3-wal`、`*.db`、`*.pyc`
3. 运行时 DB / log 迁移到单独的临时目录或 `.local/` 目录
4. 给 Rust 增加共享构建输出策略
5. 要么建立根级 Cargo workspace，要么统一使用 `CARGO_TARGET_DIR`
6. `chat_app_server_rs/docs/codex` 更适合作为外部缓存或子模块，而不是常规仓库内容
7. 增加一个健康检查脚本，统一统计 `target`、`node_modules`、`logs`、`data`、复制文档

预期收益：

- 立刻降低磁盘占用
- 减少误提交运行文件的风险
- 降低新开发者理解成本

### P0. 收敛 runtime metadata 契约

依据：

- `chat_app_server_rs/src/core/chat_runtime.rs:42`
- `chat_app_server_rs/src/core/chat_runtime.rs:90`
- `chat_app_server_rs/src/core/chat_runtime.rs:101`
- `chat_app_server_rs/src/core/chat_runtime.rs:110`
- `chat_app_server_rs/src/core/chat_runtime.rs:120`
- `chat_app_server_rs/src/core/chat_runtime.rs:130`

这个模块现在同时兼容 snake_case 和 camelCase，而且还从多个路径读取同类字段，说明协议边界已经漂了。

问题：

- 后端内部承担了过多兼容适配逻辑
- 协议漂移会持续增加重构难度
- UI 和服务端之间的 bug 很难快速定位

建议动作：

1. 定义唯一的 `chat_runtime` 标准结构
2. 把老结构兼容集中放到一个 adapter 层
3. 后续内部逻辑只读标准结构
4. 为旧字段到新字段的兼容增加 schema 测试

预期收益：

- 后端核心逻辑明显简化
- 降低前后端行为不一致风险

### P1. 拆分前端消息发送主链路

依据：

- `chat_app/src/lib/store/actions/sendMessage.ts:50`
- `chat_app/src/lib/store/actions/sendMessage.ts:56`
- `chat_app/src/lib/store/actions/sendMessage.ts:69`
- `chat_app/src/lib/store/actions/sendMessage.ts:144`
- 文件内 `any` 使用较多

当前这个文件同时承担了：

- session / runtime 选择
- metadata 合并和持久化
- 附件预处理
- optimistic user message 创建
- streaming assistant draft 生命周期
- SSE 解析
- tool call 状态更新
- UI prompt / task review 面板状态处理
- 错误格式化和收尾逻辑

建议拆分：

1. `sendMessage/runtimeContext.ts`
2. `sendMessage/optimisticState.ts`
3. `sendMessage/streamProcessor.ts`
4. `sendMessage/toolEventReducer.ts`
5. `sendMessage/finalizeTurn.ts`

关键原则：

- 入口函数可以保留一个
- 但状态变更规则要下沉到纯函数 reducer 或小型 service 中

预期收益：

- 可测试性明显提升
- 后续协议调整更稳
- SSE / tool 面板链路回归风险更低

### P1. 收敛 v2 / v3 聊天链路的重复实现

依据：

- `chat_app_server_rs/src/api/chat_v2.rs`：`596` 行
- `chat_app_server_rs/src/api/chat_v3.rs`：`818` 行
- 两者 diff 结果显示重叠度很高
- `chat_app_server_rs/src/services/v2/ai_request_handler/parser.rs`：`439` 行
- `chat_app_server_rs/src/services/v3/ai_request_handler/parser.rs`：`1119` 行

问题：

- 版本差异逻辑和共享编排逻辑混在一起
- 修一个版本时，另一个版本容易漏改
- 后续继续演进成本越来越高

建议动作：

1. 从 `chat_v2.rs` 和 `chat_v3.rs` 中提取共享的聊天请求内核
2. 版本差异只保留在请求校验、回调构造、能力分支上
3. 拆分 v3 parser，按事件类别分文件
4. provider 特有的解析逻辑隔离到单独模块

建议的 parser 结构：

- `parser/text_events.rs`
- `parser/reasoning_events.rs`
- `parser/tool_call_events.rs`
- `parser/response_snapshot.rs`
- `parser/state.rs`

预期收益：

- 单文件复杂度降低
- 协议测试更容易补齐
- 重复修 bug 的概率下降

### P1. 拆掉 Memory Agent Builder 这个大文件

依据：

- `memory_server/backend/src/services/agent_builder.rs:123`
- `memory_server/backend/src/services/agent_builder.rs:190`
- `memory_server/backend/src/services/agent_builder.rs:210`
- 文件总体积：`2410` 行，约 `77.8KB`

当前这个文件承担了：

- 请求规范化
- 模型运行时解析
- 可见范围计算
- prompt 构造
- tools schema 定义
- tool loop 执行
- fallback 策略
- 输出解析
- agent 创建持久化

建议拆分：

1. `agent_builder/request.rs`
2. `agent_builder/runtime.rs`
3. `agent_builder/prompts.rs`
4. `agent_builder/tools.rs`
5. `agent_builder/tool_loop.rs`
6. `agent_builder/output.rs`

当前进度：

- 已新增 `memory_server/backend/src/services/agent_builder_support.rs`，承接 prompt/index 构造、输入归一化、policy 解析、transport helper
- 已新增 `memory_server/backend/src/services/agent_builder_stream.rs`，承接 SSE 读取、stream 聚合、responses -> chat completion 适配
- 已新增 `memory_server/backend/src/services/agent_builder_tools.rs`，承接 tool call 执行、技能/agent 查询与最终创建动作
- 已新增 `memory_server/backend/src/services/agent_builder_request.rs`，承接 chat completions / responses 的流式请求发送与错误包装
- 已新增 `memory_server/backend/src/services/agent_builder_create.rs`，承接 create payload 构造、skill/plugin 校验与 policy 装配
- 已新增 `memory_server/backend/src/services/agent_builder_flow.rs`，承接 tool loop / fallback 编排与 fallback 输出落库兜底
- 已新增 `memory_server/backend/src/services/agent_builder_runtime.rs`，承接模型运行时解析与显式配置兼容
- 当前主文件 `memory_server/backend/src/services/agent_builder.rs` 已从 `2410` 行压到 `212` 行，主文件只保留入口、核心类型与错误包装

另外建议把 repository 相关的 normalize / hydrate 逻辑尽量从 service 热路径里抽开。

预期收益：

- 错误定位更容易
- mock 测试更容易做
- 改动影响面更小

### P1. 拆分大型前端容器组件

优先级最高的几个页面 / 组件：

- `chat_app/src/components/SessionList.tsx:55`
- `chat_app/src/components/ProjectExplorer.tsx:41`
- `memory_server/frontend/src/pages/AgentsPage.tsx:63`

我观察到的共性：

- `useState` 数量多
- 派生状态和动作处理混在一个组件里
- 数据拉取、权限判断、modal 状态、渲染逻辑耦合严重
- `AgentsPage` 里还存在 `eslint-disable-next-line react-hooks/exhaustive-deps`

建议目标形态：

1. 页面容器
2. 数据 hook
3. 动作 hook
4. 纯展示分区
5. modal / drawer 子组件

具体建议：

- `SessionList` 拆成 session tree、project actions、terminal actions、remote connection actions、dialogs
- `ProjectExplorer` 拆成 explorer controller、run panel controller、file preview controller
- `AgentsPage` 拆成列表页、编辑 drawer、AI 创建 drawer、插件/技能预览、会话记录 drawer

预期收益：

- re-render 影响面更可控
- 状态归属更清晰
- UI 回归测试更好写

### P1. 缩小 API Client 并补强类型

依据：

- `chat_app/src/lib/api/client.ts:95`
- `chat_app/src/lib/api/client.ts:167`
- 大量接口返回 `Promise<any>`
- `memory_server/frontend/src/api/client.ts:96` 也是一个很大的总入口对象

问题：

- 数据层边界过厚
- 类型洞太多，UI 不得不大量 `as any`
- 错误和响应清洗逻辑分散

建议动作：

1. 让领域 client 真正独立
2. 公开方法只返回 typed DTO
3. 在 API 层统一做响应 normalize
4. 逐步清掉公开接口上的 `Promise<any>`

建议模块划分：

- `client/sessions`
- `client/projects`
- `client/terminals`
- `client/remoteConnections`
- `client/chat`
- `client/agents`
- `client/skills`

预期收益：

- UI 里强转和兜底逻辑减少
- 领域边界更清楚

### P2. 把国际化字典从运行模块中拆出去

依据：

- `memory_server/frontend/src/i18n.tsx`：`772` 行

问题：

- 文案字典和 provider / context 运行逻辑写在同一个模块里

建议动作：

1. 把 `ZH` 和 `EN` 分离到 `src/locales/zh-CN.ts` 和 `src/locales/en-US.ts`
2. `i18n.tsx` 只负责 provider、状态和查词

预期收益：

- 运行模块更轻
- 纯文案变更更容易审查

### P2. 优先补关键链路测试

我看到的现状：

- 前端测试文件数：`2`
- Rust 后端测试文件数：`14`
- 但复杂热点文件的测试密度和其复杂度并不匹配

建议优先补的测试：

1. `sendMessage` 的 streaming 生命周期和 finalize / rollback
2. v2 / v3 parser 的事件 fixture 测试
3. `chat_runtime` metadata 兼容测试
4. `agent_builder` 的 tool loop 和 fallback 测试
5. `SessionList` / `AgentsPage` 的关键交互流程测试

## 5. 建议执行顺序

### Phase 0：仓库卫生

- 清理被跟踪的运行态 sqlite 文件
- 扩展 ignore 规则
- 统一 Rust 构建输出目录
- 把 runtime data / logs 移出源码目录

### Phase 1：类型和协议收敛

- 定义标准 `chat_runtime` DTO
- 收缩前端 API client 的 `Promise<any>`
- 增加 DTO adapter

### Phase 2：聊天链路重构

- 拆分前端 `sendMessage`
- 提取后端 v2 / v3 共享聊天内核
- 拆分 parser

### Phase 3：Memory 领域重构

- 拆 `agent_builder.rs`
- 减少 repository normalize 在 service 热路径中的占比
- 把 agent link 规则下沉到独立模块

### Phase 4：前端大页面拆分

- `AgentsPage`
- `SessionList`
- `ProjectExplorer`
- `JobConfigsPage`

### Phase 5：测试与回归保护

- parser fixtures
- streaming 生命周期测试
- metadata 兼容测试
- 大页面关键操作测试

## 6. 如果让我先动手，我会这样排

如果这是我自己的仓库，我会按这个顺序开始：

1. 清理运行态产物的 git 跟踪并修正 ignore
2. 配置共享 Rust 构建输出目录
3. 定义标准 `chat_runtime` DTO 和兼容 adapter
4. 拆 `sendMessage.ts`
5. 提取 `chat_v2.rs` / `chat_v3.rs` 共享逻辑
6. 拆 `agent_builder.rs`
7. 拆 `AgentsPage.tsx`

## 7. 最后总结

这个仓库已经具备模块边界，所以正确方向不是重写，而是把少数几个过热文件拆开，同时把仓库里的运行态和构建产物清干净。

最值得优先投入的点是：

- 仓库卫生和输出目录治理
- runtime metadata 契约收敛
- 聊天链路模块化
- agent builder 拆分
- 前端 API 类型化和大页面拆分

这次我没有跑完整 build 和完整测试套件。结论主要基于仓库结构、体积、热点文件、重复度和代表性代码抽查得出。

## 8. 已执行进展（2026-03-30）

### 已完成

- 修复 `kimi-k2` 在 v2 历史消息中因空 assistant message 导致的 400 报错
- 修复 `deepseek` 走 v2 分支时运行时引导不生效的问题
- 补充仓库清理脚本和仓库卫生报告脚本
- 增加根目录 `.cargo/config.toml`，统一 Rust 构建输出到 `target-shared`
- 将 `openai-codex-gateway` 的 sqlite 运行时文件从 Git 索引移除
- 将 `target-shared` 纳入 `.dockerignore`、清理脚本和卫生报告统计
- 将前端 `sendMessage.ts` 从 `885` 行收缩到 `601` 行
- 将 `createChatStoreWithBackend.ts` 从 `454` 行收缩到 `271` 行
- 将前端 `SessionList.tsx` 从 `881` 行收缩到 `629` 行
- 将前端 `MessageItem.tsx` 从 `737` 行收缩到 `395` 行
- 将前端 `MessageList.tsx` 从 `606` 行收缩到 `172` 行
- 将前端 `SessionList.tsx` 从 `621` 行继续收缩到 `281` 行，新增 `chat_app/src/components/sessionList/useSessionListController.ts`，把 store 选择、刷新状态、远程连接表单、本地 picker、确认弹窗和各 section controller 编排集中外提
- 将前端 `ProjectExplorer.tsx` 从 `729` 行收缩到 `570` 行
- 将前端 `chat_app/src/components/projectExplorer/PreviewPane.tsx` 从 `525` 行收缩到 `278` 行，新增 `chat_app/src/components/projectExplorer/useProjectPreviewRunController.ts`，把运行命令状态、终端轮询、退出诊断与重启/停止编排从预览组件外提
- 将前端 `ChatInterface.tsx` 收缩并改为由控制器 hook 驱动，当前为 `475` 行，并把主体区、错误条和弹层继续拆为独立视图组件
- 将前端 `SystemContextEditor.tsx` 从 `695` 行收缩到 `186` 行，列表与工作区渲染拆分为独立子组件
- 将前端 `InputArea.tsx` 收缩到 `275` 行，并新增控制器 hook、组合器视图和拖拽遮罩组件，拆出发送逻辑、运行时上下文派生和 picker 状态编排
- 将前端 `NotepadPanel.tsx` 从 `581` 行收缩到 `78` 行，新增 `chat_app/src/components/notepad/useNotepadPanelController.ts`，把记事本的数据加载、目录/笔记 CRUD、右键菜单、剪贴板与编辑器状态全部外提
- 将前端 `chat_app/src/components/notepad/useNotepadPanelController.ts` 从 `642` 行继续收缩到 `563` 行，新增 `chat_app/src/components/notepad/controllerHelpers.ts`、`useNotepadPanelEffects.ts`，把 clipboard / markdown 导出 / context menu style 和面板副作用从 controller 本体外提
- 新增 `runtimeGuidanceState.ts`、`sessionState.ts`、`runtimeGuidance.ts`，把运行时引导状态和会话流转状态外提
- 新增 `useProjectRunState.ts`，把项目运行目录拉取、运行、停止、重启动作从 `SessionList` 中外提
- 新增 `sessionList/types.ts`，收敛 `ContactItem` 等重复类型
- 新增 `messageItem/` 子目录，把工具时间线、正文分段渲染、辅助类型和 helper 从 `MessageItem` 中外提
- 新增 `useProjectExplorerRunState.ts`，把项目运行目标分析、终端运行和单文件执行逻辑从 `ProjectExplorer` 中外提
- 新增 `useProjectExplorerWorkspaceView.ts`，把 `ProjectExplorer` 的 pane props、拖拽和 context menu 视图装配从主组件外提
- 新增 `useMessageListDerivedState.ts`、`useMessageListWindowing.ts`，把消息列表的派生缓存和窗口化滚动逻辑从 `MessageList` 中外提
- 新增 `useChatInterfaceController.ts`，把 `ChatInterface` 的初始化、副作用、抽屉状态和消息发送控制统一外提
- 新增 `ChatInterfaceMainContent.tsx`、`ChatInterfaceOverlays.tsx`、`ChatInterfaceErrorBanner.tsx`，把 `ChatInterface` 的主体区和弹层视图拆开
- 新增 `systemContextEditor/` 子目录和 `useSystemContextEditorController.ts`，把系统提示词管理的列表、工作区和 AI 动作控制拆开
- 新增 `useInputAreaController.ts`，把输入区的消息发送、附件联动、MCP / 工作区 / 项目文件 picker 控制集中外提
- 新增 `InputAreaComposer.tsx`、`InputAreaDragOverlay.tsx`，把输入区主渲染层从 `InputArea.tsx` 继续下沉
- 为 `SystemContext` 相关 client/store action 补上生成、优化、评估响应类型，清理一批 `Promise<any>`
- 为 `InputArea` 相关 hook 补上文件系统响应、本地目录列表、MCP 配置项等局部类型，减少热路径 `any`
- 为 `ChatConversationPane`、`useSessionHeaderMeta`、`useChatInterfaceController` 补用现有实体类型和组件 props 类型
- 为 `chatInterface/helpers.ts`、`useSessionWorkbarPanels.ts`、`useWorkbarState.ts`、`useWorkbarMutations.ts`、`usePanelActions.ts` 补本地 API 契约和消息/tool call 类型
- 为 `sessionResolver.ts`、`sessionRuntime.ts` 清理兼容层 `any`，把 metadata/session 访问改为显式结构
- 收敛 `sendMessage` / `streaming` / runtime guidance 相关 store action 的类型定义，减少 `any` 扩散
- 新增 `chat_app/src/lib/store/actions/sendMessage/types.ts`，为附件、SSE 事件、工具调用、流式消息 metadata 建立局部契约层
- 完成 `sendMessage/attachments.ts`、`messageFactory.ts`、`requestPayload.ts`、`toolEvents.ts`、`toolPanelState.ts`、`toolPanels.ts`、`errorParsing.ts`、`streamingState.ts` 的类型收口
- 新增 `chat_app/src/lib/store/actions/sendMessage/streamEventHandler.ts`，把 `chunk/content/thinking/tools/runtime_guidance/cancelled/complete` 等流式事件分发从主流程外提
- 新增 `chat_app/src/lib/store/actions/sendMessage/streamReader.ts`，把 reader/buffer/SSE 事件消费循环从主流程外提
- 新增 `chat_app/src/lib/store/actions/sendMessage/streamExecution.ts`，把流式执行、finalize 和失败回滚从主流程外提
- 将 `sendMessage.ts` 主流程中的 `attachments/tempAssistantMessage/parsed/current` 等宽泛类型改为显式结构，移除热路径残留 `any`
- 将 `messages.ts` 的 store draft、消息回填、turn process 缓存与折叠状态访问改为显式 `ChatStoreDraft` / `Message[]`
- 当前 `chat_app/src/lib/store/actions/sendMessage.ts` 为 `264` 行，`chat_app/src/lib/store/actions/messages.ts` 为 `606` 行；`sendMessage/` 子目录总计 `3591` 行，主文件已进一步收缩为请求组装、草稿创建与顶层异常边界
- 当前 `chat_app/src/lib/store/actions/messages.ts`、`chat_app/src/lib/store/actions/sendMessage.ts` 以及 `chat_app/src/lib/store/actions/sendMessage/` 目录内显式 `any` 已清空
- 将 `memory_server/frontend/src/pages/AgentsPage.tsx` 从 `1292` 行收缩到 `737` 行，新增 `pages/agentsPage/` 子目录，拆出会话抽屉、编辑弹窗、AI 创建弹窗、插件预览、技能预览、表格列定义和纯派生 helper
- 继续拆 `memory_server/frontend/src/pages/AgentsPage.tsx`，当前主页面已压到 `139` 行，新增 `useAgentsPageController.ts`、`useAgentsPageData.ts`、`useAgentsPageInspectors.ts`，把数据加载/编辑态 与 预览/会话抽屉态分层
- 将 `memory_server/backend/src/repositories/agents.rs` 从 `779` 行收缩到 `570` 行，新增 `memory_server/backend/src/repositories/agents_support.rs`，把 normalize/hydrate/links 校验逻辑外提
- 将 `memory_server/backend/src/repositories/agents.rs` 从 `570` 行继续收缩到 `235` 行，新增 `memory_server/backend/src/repositories/agents_runtime.rs`，把 runtime plugin 刷新、命令汇总、技能汇总和 markdown 描述推断逻辑移出主仓储文件
- 将 `memory_server/backend/src/services/agent_builder.rs` 从 `2410` 行收缩到 `1150` 行，新增 `memory_server/backend/src/services/agent_builder_support.rs` 与 `memory_server/backend/src/services/agent_builder_stream.rs`，把 prompt/index 构造、输入归一化、policy 解析、transport helper、SSE/stream 聚合与 responses 适配外提
- 将 `memory_server/backend/src/services/agent_builder.rs` 从 `1150` 行继续收缩到 `630` 行，新增 `memory_server/backend/src/services/agent_builder_tools.rs`、`memory_server/backend/src/services/agent_builder_request.rs`、`memory_server/backend/src/services/agent_builder_create.rs`，把 tool 执行、AI transport 请求和 create payload 校验从主文件外提
- 将 `memory_server/backend/src/services/agent_builder.rs` 从 `630` 行继续收缩到 `212` 行，新增 `memory_server/backend/src/services/agent_builder_flow.rs`、`memory_server/backend/src/services/agent_builder_runtime.rs`，把 tool loop / fallback 编排和模型运行时解析彻底移出主文件
- 将 `memory_server/frontend/src/i18n.tsx` 从 `772` 行收缩到 `48` 行，新增 `memory_server/frontend/src/locales/zh-CN.ts` 与 `memory_server/frontend/src/locales/en-US.ts`，将运行时 provider 与文案字典解耦
- 将 `chat_app/src/lib/store/actions/messages.ts` 从 `606` 行收缩到 `56` 行，新增 `messagesLoading.ts`、`messagesTurnProcess.ts`、`messagesState.ts`，把消息加载、流式 draft 合并、turn process 状态机拆开
- 为 `chat_app/src/lib/api/client/account.ts`、`tasks.ts`、`summary.ts` 以及 `chat_app/src/lib/api/client/types.ts` 补充认证、用户设置、TaskManager、UI Prompt、SessionSummary DTO，清理一批公开接口上的 `Promise<any>`
- 为 `chat_app/src/lib/api/client/workspace.ts`、`messages.ts`、`conversation.ts` 以及 `chat_app/src/lib/api/client/types.ts` 补充 `SessionResponse`、`ContactResponse`、`SessionMessageResponse`、conversation envelope 等 DTO，清理一批 session/contact/message 相关 `Promise<any>`
- 为 `chat_app/src/lib/api/client/workspace.ts` 与 `chat_app/src/lib/api/client/types.ts` 继续补充联系人项目记忆、联系人项目列表、agent recall、project run、terminal、remote、SFTP、FS 等 DTO，进一步收口 workspace 领域 API 返回值
- 为 `chat_app/src/lib/api/client/configs.ts`、`memory.ts`、`notepad.ts`、`stream.ts` 与 `chat_app/src/lib/api/client/types.ts` 补充 `ApplicationResponse`、`MemoryAgentResponse`、`MemoryAgentRuntimeContextResponse`、notepad 系列响应以及 `StopChatResponse`
- 当前 `chat_app/src/lib/api/client.ts`、`workspace.ts`、`configs.ts`、`memory.ts`、`notepad.ts`、`stream.ts` 中公开接口上的 `Promise<any>` / `Promise<any[]>` 已清空
- 将 `chat_app/src/lib/api/client.ts` 从 `952` 行收缩到 `154` 行，新增 `chat_app/src/lib/api/client/facades/workspaceFacade.ts`、`configFacade.ts`、`runtimeFacade.ts`，把主入口改为 request/context 壳层 + facade 注入，保留现有实例方法调用方式不变
- 继续收敛 `chat_app/src/lib/api/client/configs.ts`、`conversation.ts`、`runtimeFacade.ts` 与 `chat_app/src/lib/store/actions/mcp.ts` 的类型边界，补齐 `McpConfigUpdatePayload`、`ConversationDetailsResponse` fallback、`SessionMessageResponse` fallback 与 `StreamChatModelConfigPayload` 约束，并将 `mcp` store 从“强转后端返回”改为显式 normalize
- 继续收敛 `chat_app/src/lib/store/actions/aiModels.ts`，新增 `AiModelConfigResponse -> AiModelConfig` normalize helper，补齐 `AiModelConfigCreatePayload` / `ChatStoreDraft` 边界，并清空该文件内显式 `any`
- 将 `chat_app/src/components/McpManager.tsx` 从 `514` 行收缩到 `252` 行，新增 `chat_app/src/components/mcpManager/helpers.ts`、`types.ts`、`McpManagerForm.tsx`、`DynamicConfigFields.tsx`、`McpServerList.tsx`，把表单区、动态参数区、列表区和兼容 helper 从主组件外提
- 将 `chat_app/src/components/AiModelManager.tsx` 从 `522` 行收缩到 `168` 行，新增 `chat_app/src/components/aiModelManager/helpers.ts`、`types.ts`、`icons.tsx`、`AiModelManagerForm.tsx`、`AiModelList.tsx`，把 provider/thinking 规则、表单区和列表区从主组件外提，并去掉组件内随机 `Math.random()` ID 生成
- 将 `chat_app/src/components/ApplicationsPanel.tsx` 从 `482` 行收缩到 `231` 行，新增 `chat_app/src/components/applicationsPanel/helpers.ts`、`types.ts`、`icons.tsx`、`ApplicationsManageView.tsx`、`ApplicationsBrowseView.tsx`，把 modal/embedded 共享内容抽成管理视图和浏览视图，减少重复 JSX
- 继续收敛 `chat_app/src/lib/store/actions/applications.ts`，将 `client/set/get` 改为显式 `ApiClient` / `ChatStoreDraft` / DTO 边界，补齐 `ApplicationResponse.icon_url` / `iconUrl` 兼容，并将系统提示词应用关联持久化改为读取当前 `name/content` 后再提交，移除 `undefined as any`
- 将 `chat_app/src/components/TaskWorkbar.tsx` 从 `557` 行收缩到 `273` 行，新增 `chat_app/src/components/taskWorkbar/types.ts`、`helpers.ts`、`TaskCard.tsx`、`RuntimeGuidanceSection.tsx`、`TaskHistoryDrawer.tsx`，把 workbar 卡片、引导区、历史抽屉和显示文案 helper 从主组件外提
- 将 `chat_app/src/components/inputArea/PickerWidgets.tsx` 从 `587` 行收缩到 `5` 行，新增 `chat_app/src/components/inputArea/pickerWidgets/` 子目录，拆出 `InputAreaProjectFilePicker.tsx`、`InputAreaProjectSelector.tsx`、`InputAreaWorkspacePicker.tsx`、`InputAreaRemoteConnectionPicker.tsx`、`InputAreaMcpPicker.tsx`，让主文件只保留导出壳层
- 将 `chat_app/src/components/RemoteSftpPanel.tsx` 从 `543` 行收缩到 `334` 行，新增 `chat_app/src/components/remoteSftp/helpers.ts`、`useRemoteSftpBrowsers.ts`、`useRemoteSftpTransfer.ts`，把目录浏览状态、传输队列轮询、路径 helper 与响应 normalize 从主组件外提
- 将 `chat_app/src/components/projectExplorer/TeamMembersPane.tsx` 从 `585` 行收缩到 `536` 行，新增 `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberRuntimeContext.ts`，把“联系人会话最近一轮 runtime 上下文”的加载、跨项目校验、抽屉状态和刷新逻辑从主组件外提
- 继续收敛 `chat_app/src/components/projectExplorer/TeamMembersPane.tsx`、`teamMembers/useTeamMemberConversation.ts`、`useProjectMembersManager.ts`、`TeamMemberWorkspace.tsx`、`TeamMemberSummaryView.tsx` 的类型边界，移除 `messages as any[]`，将会话工作区 / 成员管理 / 总结视图的 props 与 hook 参数改为显式 `Message`、`AiModelConfig`、`SendMessageRuntimeOptions`、`ProjectContactLinkResponse`、`ContactRecord`
- 对齐 `ProjectExplorer`、终端历史和 `NotepadPanel` 的调用侧类型边界，修复 `TerminalLogResponse[]` 与 `TerminalLog[]` 的错误归属，并让运行面板 / notepad 不再依赖宽泛返回值
- 补强联系人会话 runtime 可视化：Turn Runtime 抽屉新增 `remote_connection_id`、`workspace_root` 展示，并明确说明这里展示的是“已发送到后端的最近一轮 runtime 快照”
- 修正流式收尾协议：将 `chunk` 定义为正文唯一真相，`complete` 仅作为结束/兜底事件；当本轮已收到正文 chunk 时，前端忽略 `complete.result.content`，后端也不再合并 streamed_content 与 complete content，避免重复正文再次被拼接

### 已验证

- `chat_app` 执行 `npm run type-check` 通过
- `memory_server/frontend` 执行 `npx tsc --noEmit` 通过
- `memory_server/backend` 执行 `cargo check` 通过
- `memory_server/backend` 执行 `cargo test agents_runtime -- --nocapture` 通过，新增 4 条 `agents_runtime` 单测覆盖 markdown 描述推断、命令名回退、命令去重、inline skill 回填
- `bash scripts/repo-hygiene-report.sh` 已确认 `Tracked Runtime Artifacts` 为空
- `chat_app_server_rs` 执行 `cargo test join_stream_text --package chat_app_server_rs` 通过
- `chat_app_server_rs` 执行 `cargo test ensure_complete_event_content --package chat_app_server_rs` 已在上一版完整性合并策略下通过；当前实现已进一步简化为“不再合并 complete 内容”
- `chat_app` 再次执行 `npm run type-check` 通过，确认 API client DTO 收口、`ProjectExplorer` 终端日志类型对齐和 `NotepadPanel` 响应类型修正已稳定
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `NotepadPanel` controller 拆分后前端类型与行为边界保持稳定
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `PreviewPane` 运行状态机外提到 `useProjectPreviewRunController.ts` 后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `SessionList` orchestration 外提到 `useSessionListController.ts` 后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `ApiClient` facade 化拆分和 `useNotepadPanelController` 二次下沉后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `conversation/config/runtimeFacade/mcp` 这一轮 API 类型收口和 `McpManager` 组件拆分后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `aiModels.ts` 的 DTO normalize / store draft 类型收口后无回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `AiModelManager` / `ApplicationsPanel` 拆分为 `aiModelManager/`、`applicationsPanel/` 子目录后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `applications.ts` 的 store 类型收口和 `TaskWorkbar` 拆分为 `taskWorkbar/` 子目录后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `PickerWidgets.tsx` 下沉为 `inputArea/pickerWidgets/` 子目录后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `RemoteSftpPanel` 的浏览器/传输状态 hook 下沉和 `TeamMembersPane` 的 runtime context hook 外提后无类型回归
- `chat_app` 再次执行 `npm run type-check` 通过，确认 `TeamMembersPane` 团队成员会话工作区链路去掉 `messages as any[]` 并补齐会话 / 成员管理 / 总结视图 DTO 后无类型回归

### 下一批建议

- 考虑将 `sendMessage` 的流式状态收尾与 finalize/failure rollback 再下沉到 controller，进一步压缩主文件并补前端单测
- 继续处理 `chat_app/src/components/projectExplorer/TeamMembersPane.tsx` 周边的 `useSessionWorkbarPanels.ts` / `useWorkbarState.ts` / `useSessionSummaryPanel.ts`，把 `unknown[]`、`RuntimeGuidanceWorkbarItem[]` 强转和总结列表 normalize 进一步收敛到显式 DTO
- 为 `memory_server/backend/src/services/agent_builder_flow.rs` 补 tool loop / fallback 测试，优先覆盖“先列技能再创建”和“fallback JSON 落库”两条主路径
- 继续拆 `memory_server/frontend/src/pages/AgentsPage.tsx` 的列表交互与 modal props 装配，考虑把页面头部提示区和表格容器再下沉成纯展示组件
- 考虑为 `memory_server/backend/src/repositories/agents_runtime.rs` 补 runtime context 组装测试，重点覆盖 plugin refresh、命令去重、inline skill 回填
- 继续拆 `messages.ts`，把 turn process 的读取/写入、draft merge、分页拼接拆到 helper 文件
- 把 `streaming.ts` 与 `runtimeGuidanceState.ts` 一并压到统一 session streaming controller
- 把 `ChatInterface` 中仍然偏重的渲染装配继续下沉到更细粒度 view model / section 组件
- 把 `SystemContextEditor` 相关 store action 返回值继续类型化，逐步替换残留的 `Promise<any>`
- 把 `InputArea` 的按钮区 / 组合器视图继续拆成更细粒度子组件，减少主文件 JSX 密度
- 继续把 `ProjectExplorer` 运行面板和 `NotepadPanel` 中仍然存在的局部 `any` / 宽泛对象访问替换成显式 view model，避免 UI 层再做 snake_case / camelCase 兼容
