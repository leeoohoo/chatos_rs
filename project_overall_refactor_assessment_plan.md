# CHATOS 项目整体体检与重构方案（2026-03-18）

## 1. 目标
- 找出当前项目里“大文件”和“可抽象业务”。
- 重点梳理“团队成员（TeamMembers）前端”可维护性问题与拆分方案。
- 给出可分阶段执行的整改路线，减少后续继续出现“修一个地方、另一个地方行为不一致”的问题。

---

## 2. 体检结论（摘要）

### 2.1 代码规模（按行数）
- 扫描范围：`chat_app/src` + `chat_app_server_rs/src` + `memory_server/backend/src`
- 文件总数：`344`
- 总行数：`94305`

### 2.2 前端（chat_app）大文件 Top
- `chat_app/src/components/InputArea.tsx`：`1568` 行
- `chat_app/src/components/SessionList.tsx`：`1528` 行
- `chat_app/src/components/ChatInterface.tsx`：`1369` 行
- `chat_app/src/lib/store/actions/sendMessage.ts`：`1266` 行
- `chat_app/src/components/projectExplorer/TeamMembersPane.tsx`：`1054` 行
- `chat_app/src/lib/store/actions/sessions.ts`：`1053` 行
- `chat_app/src/lib/api/client.ts`：`999` 行

### 2.3 后端（chat_app_server_rs / memory_server）大文件 Top
- `memory_server/backend/src/api/mod.rs`：`4387` 行（`122` 个函数）
- `chat_app_server_rs/src/services/v3/ai_client/mod.rs`：`1879` 行
- `chat_app_server_rs/src/services/memory_server_client.rs`：`1172` 行
- `chat_app_server_rs/src/services/v3/ai_request_handler/parser.rs`：`1128` 行

### 2.4 测试覆盖现状
- 前后端源码中仅发现 `2` 个测试文件（主要是前端局部组件/错误映射）。
- 团队成员、联系人会话切换、MCP 选择持久化、项目隔离等核心链路缺少自动化回归测试。

### 2.5 类型与复杂度信号
- `chat_app/src` 中 `any/as any` 命中约 `686` 次。
- 核心大组件（`ChatInterface` / `SessionList` / `TeamMembersPane` / `InputArea`）都同时承担“UI + 会话路由 + 业务状态 + API编排”多重职责。

---

## 3. 大文件与大体积产物问题

## 3.1 运行/构建产物体积（非 node_modules/target）
- `chat_app_server_rs/data/chat_app.db`：约 `70MB`
- `chat_app_server_rs/logs/server.log.2026-03-11`：约 `36MB`
- 多个 `server.log.*` 单日 `10MB~17MB`
- `memory_server/backend/data/memory_server.db`：约 `4.3MB`
- `chat_app/dist/assets/index-*.js`：约 `2MB`

## 3.2 仓库中已跟踪的大文件
- `rustup-init.exe`：约 `13.5MB`（已被 git 跟踪）

## 3.3 建议
1. 增加“开发产物清理脚本”并纳入日常命令（logs/db-wal/dist）。
2. 日志按大小或天数轮转，并设置保留策略（如 7 天）。
3. 检查 `rustup-init.exe` 是否必须留仓；若非业务必需，迁出仓库并在文档中给下载地址。
4. CI 增加“大文件守门”（例如 >5MB 阻断，白名单除外）。

---

## 4. 团队成员前端可抽象业务（重点）

当前 `TeamMembersPane.tsx` 过重，至少混合了以下业务域：
- 项目成员列表加载/增删
- 联系人与会话映射（查找、ensure、切换）
- MCP 开关与 MCP 选择持久化
- 会话总结加载/清空/删除
- 聊天消息区 + 输入区编排
- 面板状态（总结视图、切换状态、删除状态）

这些能力在 `SessionList.tsx` 与 `ChatInterface.tsx` 中有明显重复实现，已导致历史 bug（状态错读、MCP 选项切会话丢失）。

## 4.1 推荐拆分（第一优先级）

### A. 共享 Domain Hook：会话定位与切换
新建：`chat_app/src/features/contactSession/useContactSessionResolver.ts`
- 负责：
  - `findSession(contactId/agentId/projectId)`
  - `ensureSession(...)`
  - `resolveSessionProjectScopeId/normalizeProjectScopeId`
- 替代：`SessionList` 与 `TeamMembersPane` 内重复逻辑。

### B. 共享 Domain Hook：会话运行态（MCP/目录）持久化
新建：`chat_app/src/features/sessionRuntime/useSessionRuntimeSettings.ts`
- 负责：
  - 读取 `chat_runtime` 到 UI state
  - 写回 `metadata`（`mcpEnabled`、`enabledMcpIds`、`workspaceRoot` 等）
- 替代：`ChatInterface` 与 `TeamMembersPane` 各自写一套更新逻辑。

### C. 共享 Summary Hook
新建：`chat_app/src/features/sessionSummary/useSessionSummaryPanel.ts`
- 负责：
  - `getSessionSummaries`
  - `deleteSessionSummary`
  - `clearSessionSummaries`
  - loading/error/refresh 状态统一
- 替代：`TeamMembersPane` 与主聊天总结面板各自管理一套摘要状态机。

### D. 组件拆分（UI）
将 `TeamMembersPane.tsx` 拆为：
- `TeamMemberList.tsx`（左侧成员列表）
- `TeamMemberActions.tsx`（添加/移除、总结按钮区域）
- `TeamMemberChatPanel.tsx`（消息列表 + 输入区）
- `TeamMemberSummaryPanel.tsx`（总结视图）

目标：单文件控制在 `300~400` 行内，状态来源尽量来自 hook。

---

## 5. 其它可抽象业务（全局）

### 5.1 联系人列表与团队成员列表共享“状态徽标”组件
新建：`chat_app/src/components/chat/SessionBusyBadge.tsx`
- 统一“执行中/空闲/归档中”等显示规则。

### 5.2 会话列表占位 ID 与真实 ID 映射抽象
新建：`chat_app/src/features/contactSession/sessionIdMapping.ts`
- 防止再次出现 placeholder id 读不到 runtime 状态的问题。

### 5.3 API Client 拆分
- `chat_app/src/lib/api/client.ts` 近 1000 行，建议改为“领域聚合导出 + 独立子模块”。
- 目标：会话、联系人、项目、终端、远程连接、总结分别独立，主 client 仅组装。

### 5.4 后端 API 路由拆分
- `memory_server/backend/src/api/mod.rs` 4387 行，应按领域拆为多文件：
  - `api/auth.rs`
  - `api/sessions.rs`
  - `api/contacts.rs`
  - `api/projects.rs`
  - `api/agents.rs`
  - `api/messages.rs`
  - `api/jobs.rs`
- 主 `mod.rs` 仅保留 route 组合与状态注入。

---

## 6. 当前主要缺陷清单

## P0（优先立即治理）
1. **核心业务重复实现导致行为漂移**
   - 同一业务在多个组件实现（会话解析、MCP 持久化、总结加载），已出现切会话后状态丢失/状态显示错误。
2. **团队成员页文件职责过载**
   - 代码聚合导致修复成本高、回归风险高。

## P1（近期治理）
3. **自动化回归覆盖严重不足**
   - 核心链路无测试：项目隔离、联系人唯一性、MCP 选择持久化、总结面板开关行为。
4. **类型债务高（前端 any 较多）**
   - 影响重构安全性，容易引入隐性运行时错误。
5. **超大后端路由文件可维护性差**
   - 合并冲突频繁，定位副作用困难。

## P2（中期治理）
6. **运行产物体积持续增长**（db/log/dist）
   - 影响开发机磁盘与启动稳定性，需要轮转策略与清理机制。

---

## 7. 分阶段整改计划

## Phase 0（1~2 天）基线与守门
1. 增加脚本：`scripts/cleanup-dev-artifacts.sh`（清理 logs/wal/dist 临时产物）。
2. 增加 CI 检查：大文件守门 + 前端构建 + 后端编译。
3. 输出一份“会话运行时字段规范文档”（metadata 字段唯一来源）。

## Phase 1（3~5 天）团队成员前端抽象
1. 落地 `useContactSessionResolver`。
2. 落地 `useSessionRuntimeSettings`。
3. 落地 `useSessionSummaryPanel`。
4. TeamMembersPane 拆分为 4 个组件（列表/动作/聊天/总结）。

## Phase 2（3~4 天）联系人页/主聊天页对齐
1. `SessionList`、`ChatInterface` 接入上述共享 hooks。
2. 合并重复 util（project scope/session time/session active 判断）。
3. 引入 `SessionBusyBadge` 统一状态显示。

## Phase 3（5~8 天）后端模块化
1. 拆分 `memory_server/backend/src/api/mod.rs`。
2. 拆分 `chat_app_server_rs/src/services/memory_server_client.rs` DTO 与接口域。
3. 将 `v3/ai_client/mod.rs` 中测试迁出到独立 `tests` 或 `mod tests` 子文件。

## Phase 4（3~5 天）测试补齐
1. 前端：
   - 团队成员切换会话时 MCP 设置保持
   - 联系人占位会话状态映射正确
   - 总结面板开关与会话隔离
2. 后端：
   - 联系人与项目隔离
   - project_id=0 与项目会话隔离
   - 总结任务状态流转正确

---

## 8. 验收标准（建议）
1. `TeamMembersPane.tsx` 从 `1054` 行降低到 `< 400` 行。
2. `SessionList.tsx` 和 `ChatInterface.tsx` 去除重复 session resolver/runtime persistence 逻辑。
3. 同一会话下 MCP 开关、MCP 选择、工作目录在“切换会话 -> 切回”后完全一致。
4. 团队成员聊天与外部联系人聊天的历史/总结隔离规则不回归。
5. 自动化测试覆盖至少增加：
   - 前端 8+ 用例
   - 后端 8+ 用例

---

## 9. 建议的首批改造顺序（最稳妥）
1. 先抽 `useSessionRuntimeSettings`（直接解决你最近多次遇到的“切会话丢配置”类问题）。
2. 再抽 `useContactSessionResolver`（统一联系人会话匹配与创建逻辑）。
3. 最后拆 `TeamMembersPane` UI 组件。

> 这样顺序能确保每一步都先降低线上行为风险，再做结构优化。

---

## 10. 实施进展（2026-03-18）

已完成（本轮）：

1. 会话解析抽象
   - 新增 `chat_app/src/features/contactSession/sessionResolver.ts`
   - 新增 `chat_app/src/features/contactSession/useContactSessionResolver.ts`
   - 新增 `chat_app/src/components/sessionList/useContactSessionListState.ts`
   - `SessionList` / `TeamMembersPane` / `ChatInterface` 统一复用 project scope、会话匹配、时间排序逻辑
   - `SessionList` 与 `TeamMembersPane` 的联系人会话 ensure + API 兜底 + 缓存清理已合并到同一 hook
   - `SessionList.tsx` 联系人展示会话映射逻辑已外提（约 `1431` 行降至 `1194` 行）

2. 运行态抽象（MCP/工作目录）
   - 新增 `chat_app/src/features/sessionRuntime/useSessionRuntimeSettings.ts`
   - `TeamMembersPane` 与 `ChatInterface` 已接入，移除重复的 metadata 写回逻辑

3. 总结面板状态机抽象
   - 新增 `chat_app/src/features/sessionSummary/useSessionSummaryPanel.ts`
   - `TeamMembersPane` 已接入统一的加载/删除/清空流程

4. TeamMembers 前端拆分
   - 新增：
     - `chat_app/src/components/projectExplorer/teamMembers/TeamMembersSidebar.tsx`
     - `chat_app/src/components/projectExplorer/teamMembers/TeamMemberSummaryView.tsx`
     - `chat_app/src/components/projectExplorer/teamMembers/types.ts`
     - `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
   - `TeamMembersPane.tsx` 仅保留编排逻辑，列表渲染/总结渲染/项目成员管理均已外提（约 `830` 行降至 `523` 行）

5. 会话忙闲徽标统一
   - 新增 `chat_app/src/components/chat/SessionBusyBadge.tsx`
   - 已应用到联系人列表与团队成员列表

6. 基线脚本与规范文档
   - 新增脚本：
     - `scripts/cleanup-dev-artifacts.sh`
     - `scripts/check-large-files.sh`
   - 新增文档：
     - `session_runtime_metadata_contract.md`

7. `ChatInterface` 记忆上下文抽离（本次补齐）
   - 新增：
     - `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
   - `ChatInterface` 移除本地记忆状态/请求去重/取消逻辑，统一改为通过 hook 提供：
     - `loadContactMemoryContext`
     - `resetMemoryState`
     - `cancelPendingMemoryLoad`
   - 记忆选择策略（保留 L0 + 最高层级 Top2 + 智能体最高层 Top1）下沉为单一实现，避免主聊天页与团队成员页再次分叉。

8. `SessionList` 展示态拆分
   - 新增：
     - `chat_app/src/components/sessionList/useContactSessionListState.ts`
   - 联系人列表中“显示会话 id / 汇总面板 id / ensure + 缓存清理”逻辑从组件主文件抽出，降低 UI 文件复杂度并为后续测试隔离做准备。

9. `ChatInterface` UI Prompt 历史抽离（本次补齐）
   - 新增：
     - `chat_app/src/components/chatInterface/useUiPromptHistory.ts`
   - UI Prompt 历史相关的“请求取消 / 会话级缓存 / 会话切换回填 / 强制刷新”逻辑从主组件移出，`ChatInterface` 只保留面板开关和事件编排。
   - `ChatInterface.tsx` 进一步缩减到约 `1059` 行（由改造前 `1369` 行持续下降）。

10. `ChatInterface` 联系人项目范围抽离（本次新增）
   - 新增：
     - `chat_app/src/components/chatInterface/useContactProjectScope.ts`
   - 会话 project scope 计算、联系人可选项目加载、项目合法性校验、项目名映射从主组件移出，避免和 TeamMembers/SessionList 再次出现并行实现。
   - `ChatInterface.tsx` 继续下降到约 `975` 行。

11. `SessionList` 基础行为抽离（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/useInlineActionMenus.ts`
     - `chat_app/src/components/sessionList/useSectionExpansion.ts`
     - `chat_app/src/components/sessionList/useSessionListBootstrap.ts`
   - 抽离了三类高重复逻辑：
     - 操作菜单显示/关闭与全局点击收起
     - contacts/projects/terminals/remote 四段折叠互斥状态
     - 初始化加载与终端/远端轮询刷新
   - `SessionList.tsx` 下降到约 `1054` 行（由本轮前 `1194` 行下降）。

12. `SessionList` 本地文件选择器状态机抽离（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/useLocalFsPickers.ts`
   - 将目录选择器与密钥文件选择器的全部状态与动作迁移到独立 hook（目录浏览/新建目录/密钥文件回填）。
   - `SessionList.tsx` 继续下降到约 `914` 行。

13. `SessionList` 联系人创建流程抽离（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/useContactSessionCreator.ts`
   - “选择联系人 -> 创建联系人记录 -> ensure 会话 -> 写入运行时 metadata -> 切换会话”迁移到独立 hook，减少主组件中的业务编排噪音。
   - `SessionList.tsx` 当前约 `861` 行。

14. `sessionList/Sections.tsx` 组件分拆（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/sections/SessionSection.tsx`
     - `chat_app/src/components/sessionList/sections/ProjectSection.tsx`
     - `chat_app/src/components/sessionList/sections/TerminalSection.tsx`
     - `chat_app/src/components/sessionList/sections/RemoteSection.tsx`
   - `Sections.tsx` 降为聚合导出入口（约 `4` 行），便于后续按 section 独立演进与测试。

15. `InputArea` 状态机拆分（本次补记）
   - 新增：
     - `chat_app/src/components/inputArea/useMcpSelection.ts`
     - `chat_app/src/components/inputArea/useDismissiblePopover.ts`
     - `chat_app/src/components/inputArea/useWorkspaceDirectoryPicker.ts`
     - `chat_app/src/components/inputArea/useProjectFilePicker.ts`
   - MCP 选中态、弹层关闭、工作目录选择、项目文件浏览/搜索/附加 已从主组件迁出。
   - `InputArea.tsx` 由改造前 `1568` 行下降到约 `1056` 行。

16. `sendMessage` 主流程瘦身（本次新增）
   - 新增：
     - `chat_app/src/lib/store/actions/sendMessage/runtime.ts`
     - `chat_app/src/lib/store/actions/sendMessage/errorParsing.ts`
     - `chat_app/src/lib/store/actions/sendMessage/messageFactory.ts`
     - `chat_app/src/lib/store/actions/sendMessage/requestPayload.ts`
   - 已将运行时解析、错误解析、草稿消息构造、请求 payload 构造迁出为可复用纯函数。
   - `sendMessage.ts` 由 `1266` 行（改造前）进一步下降到约 `1051` 行。

17. `TeamMembersPane` 二次收敛（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspace.tsx`
     - `chat_app/src/components/projectExplorer/teamMembers/useTeamMemberConversation.ts`
   - 右侧工作区渲染和“成员会话状态/动作”从主文件抽离。
   - `TeamMembersPane.tsx` 从前一阶段约 `523` 行进一步下降到约 `350` 行（达到 `< 400` 目标）。

18. `sendMessage` 工具事件分发再拆分（本次新增）
   - 新增：
     - `chat_app/src/lib/store/actions/sendMessage/toolEvents.ts`
   - `tools_start` / `tools_end` / `tools_stream` 的 payload 归一化与消息更新逻辑已抽离，主文件仅保留事件编排。
   - `sendMessage.ts` 从约 `1051` 行继续下降到约 `878` 行。

19. `sessions` action 工具层抽离（本次新增）
   - 新增：
     - `chat_app/src/lib/store/actions/sessionsUtils.ts`
   - 抽离会话缓存、会话身份解析、project scope 归一化、联系人会话去重、stream 草稿用户消息构造等通用函数。
   - `sessions.ts` 从 `1053` 行下降到约 `819` 行。

20. `InputArea` 附件链路拆分（本次新增）
   - 新增：
     - `chat_app/src/components/inputArea/useAttachmentsInput.ts`
   - 将附件类型校验、大小/数量限制、粘贴上传、局部/全局拖拽上传、附件移除与清空从主组件迁出。
   - `InputArea.tsx` 从约 `1056` 行进一步下降到约 `869` 行。

21. `ChatInterface` Workbar 状态机拆分（本次新增）
   - 新增：
     - `chat_app/src/components/chatInterface/useWorkbarState.ts`
   - 将当前轮任务加载、历史任务加载、任务变更触发刷新、workbar reset 生命周期从主组件迁出。
   - `ChatInterface.tsx` 从约 `975` 行下降到约 `700` 行。

22. `ChatInterface` 头部元信息拆分（本次新增）
   - 新增：
     - `chat_app/src/components/chatInterface/useSessionHeaderMeta.ts`
   - 当前会话联系人解析（`contactId/contactAgentId`）与 header 标题选择逻辑迁出主组件。
   - `ChatInterface.tsx` 进一步下降到约 `666` 行。

23. `SessionList` 弹窗区块拆分（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/SessionListDialogs.tsx`
   - 将联系人创建、项目/终端创建、远端连接、目录/密钥选择器、确认弹窗整体抽离。
   - `SessionList.tsx` 由约 `861` 行下降到约 `810` 行。

24. `SessionList` 删除确认流程拆分（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/useSessionListDeleteActions.ts`
   - 抽离项目归档、终端删除、远端连接删除、联系人会话删除确认与异常反馈逻辑。
   - `SessionList.tsx` 继续下降到约 `722` 行。

25. `sendMessage` 流式消息状态机抽离（本次新增）
   - 新增：
     - `chat_app/src/lib/store/actions/sendMessage/streamingState.ts`
   - 将流式文本拼接/草稿持久化/历史过程元数据更新/最终 complete 覆写的 5 个 helper 从主文件迁出，主流程仅保留事件编排。
   - `sendMessage.ts` 从约 `878` 行下降到约 `729` 行。

26. `sendMessage` 工具面板状态更新再拆分（本次新增）
   - 新增：
     - `chat_app/src/lib/store/actions/sendMessage/toolPanelState.ts`
   - 抽离 `tools_stream` 分支中“任务确认面板/UiPrompt 面板 upsert + toolCall 等待态标记”的重复逻辑，降低分支噪音并统一状态写入入口。
   - `sendMessage.ts` 由约 `729` 行继续下降到约 `689` 行。

27. `sendMessage` 思考段落更新逻辑抽离（本次新增）
   - 变更：
     - `chat_app/src/lib/store/actions/sendMessage/streamingState.ts`
   - 将 `parsed.type === 'thinking'` 分支中的 segment 拼接、process 计数与草稿持久化迁移到 `appendThinkingToStreamingMessage`，主流程仅保留事件解析与调用。
   - `sendMessage.ts` 再下降到约 `657` 行。

28. `store/helpers/messages` 标准化层拆分（本次新增）
   - 新增：
     - `chat_app/src/lib/store/helpers/messageNormalization.ts`
   - 将原 `messages.ts` 顶部的消息标准化逻辑（metadata/toolCalls/contentSegments/attachments 解析与 `normalizeRawMessages`）迁移到独立模块，`messages.ts` 聚焦“compact 历史形态 + process cache 合并”。
   - `messages.ts` 从约 `881` 行下降到约 `606` 行。

29. `SessionList` 动作编排拆分（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/useSessionListActions.ts`
   - 将会话选择、summary 打开、刷新、项目/终端创建、remote 选择与 panel 聚焦逻辑迁出主组件。
   - `SessionList.tsx` 从约 `722` 行进一步下降到约 `623` 行。

30. `sessions` 联系人会话匹配逻辑收敛（本次新增）
   - 变更：
     - `chat_app/src/lib/store/actions/sessionsUtils.ts`
     - `chat_app/src/lib/store/actions/sessions.ts`
   - 新增通用函数：`isSessionActive`、`matchSessionContactProjectScope`、`splitSessionsByMappedContacts`，替换 `loadSessions/createSession/selectSession` 内部重复判断。
   - `sessions.ts` 从约 `819` 行下降到约 `745` 行。

31. `MessageList` 派生数据计算拆分（本次新增）
   - 新增：
     - `chat_app/src/components/messageList/derivedData.ts`
   - 将消息解析与“可见列表/工具映射/process 统计/展开态关联”聚合逻辑迁出组件，`MessageList` 聚焦滚动窗口和渲染层。
   - `MessageList.tsx` 从约 `806` 行下降到约 `472` 行。

32. `TerminalView` 历史视图工具层拆分（本次新增）
   - 新增：
     - `chat_app/src/components/terminal/historyViewUtils.tsx`
   - 将命令高亮、历史日志解析、snapshot/history 常量与 websocket 安全关闭函数迁出主组件，终端主文件聚焦连接/输入/滚动状态逻辑。
   - `TerminalView.tsx` 从约 `937` 行下降到约 `711` 行。

33. `NotepadPanel` 工具与树渲染拆分（本次新增）
   - 新增：
     - `chat_app/src/components/notepad/utils.ts`
     - `chat_app/src/components/notepad/NotepadTree.tsx`
   - 将记事本目录树构建/路径与命名工具迁出，并把递归目录/笔记渲染从主组件抽到独立 `NotepadTree`。
   - `NotepadPanel.tsx` 从约 `960` 行下降到约 `783` 行。

34. `ProjectExplorer` 路径与拖拽逻辑拆分（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerPathHelpers.ts`
     - `chat_app/src/components/projectExplorer/useProjectExplorerDnd.ts`
   - 将路径归一化/expanded key 计算/父目录解析与 DnD 的可放置判断、自动展开、自动滚动定时器迁出主组件。
   - `ProjectExplorer.tsx` 从约 `954` 行下降到约 `846` 行。

35. `ProjectExplorer` 变更标记聚合拆分（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerChangeTracking.ts`
   - 将 pending marks、当前路径可确认判断、聚合变更等级映射迁出，避免主组件混杂业务计算细节。
   - `ProjectExplorer.tsx` 进一步下降到约 `805` 行。

36. `ProjectExplorer` UI 持久化与交互副作用拆分（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerUiPersistence.ts`
   - 将 expanded/showOnlyChanged/workspaceTab/treeWidth 持久化、右键菜单自动关闭、树宽拖拽副作用迁出主组件。
   - `ProjectExplorer.tsx` 再下降到约 `736` 行。

37. `InputArea` 内联组件拆分（本次新增）
   - 新增：
     - `chat_app/src/components/inputArea/InlineWidgets.tsx`
   - 抽离附件预览、错误提示、浮动模型选择器、发送/停止按钮，主组件聚焦消息发送与各 picker 状态编排。
   - `InputArea.tsx` 从约 `869` 行下降到约 `781` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

38. `ProjectExplorer` 日志状态拆分（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerLogs.ts`
   - 将变更日志列表、当前选中日志、日志加载错误与按当前路径/文件过滤的派生逻辑迁出主组件。
   - `ProjectExplorer.tsx` 从约 `736` 行下降到约 `697` 行。

39. `InputArea` 选择器弹层拆分（本次新增）
   - 新增：
     - `chat_app/src/components/inputArea/PickerWidgets.tsx`
   - 新增并复用子组件：项目文件选择器、项目下拉、工作目录选择器、MCP 选择器，主组件仅保留状态与回调编排。
   - 同步调整：`chat_app/src/components/inputArea/useMcpSelection.ts` 导出 `SelectableMcpConfig` 供弹层组件复用。
   - `InputArea.tsx` 从约 `781` 行下降到约 `495` 行。

40. `NotepadPanel` 布局区块拆分（本次新增）
   - 新增：
     - `chat_app/src/components/notepad/NotepadSidebar.tsx`
     - `chat_app/src/components/notepad/NotepadEditor.tsx`
     - `chat_app/src/components/notepad/NotepadContextMenu.tsx`
   - 将左侧树区、右侧编辑区、右键菜单 UI 从主组件迁出，保留原有动作回调与状态流。
   - `NotepadPanel.tsx` 从约 `783` 行下降到约 `572` 行。

41. `ProjectExplorer` 项目生命周期与轮询拆分（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerProjectLifecycle.ts`
   - 将“项目切换初始化/重置 + expanded 恢复 + root/summary 首次加载 + 定时 silent summary 轮询”从主组件迁出。
   - `ProjectExplorer.tsx` 从约 `697` 行下降到约 `659` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

42. `RemoteSftpPanel` 视图层拆分（本次新增）
   - 新增：
     - `chat_app/src/components/remoteSftp/types.ts`
     - `chat_app/src/components/remoteSftp/TransferPanels.tsx`
     - `chat_app/src/components/remoteSftp/SftpBrowsers.tsx`
   - 将传输进度/队列展示和远端-本地双栏浏览器从主组件迁出，主文件保留连接状态、传输队列和远端操作编排。
   - `RemoteSftpPanel.tsx` 从约 `727` 行下降到约 `543` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

43. `api/client.ts` 请求类型外提（本次新增）
   - 新增：
     - `chat_app/src/lib/api/client/types.ts`
   - 将 API 客户端中的大段内联 payload/paging/options 类型抽离为独立类型模块，降低主文件噪音并提升参数复用性。
   - `client.ts` 从约 `999` 行下降到约 `830` 行。

44. `sessions` 选择会话状态机拆分（本次新增）
   - 新增：
     - `chat_app/src/lib/store/actions/sessionsSelectHelpers.ts`
   - 将 `selectSession` 中“流式草稿恢复 + 历史消息拼装 + AI 选择恢复 + 会话状态落盘”的大段 set 逻辑迁出主文件。
   - `sessions.ts` 从约 `745` 行下降到约 `569` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

45. `TerminalView` 展示层拆分（本次新增）
   - 新增：
     - `chat_app/src/components/terminal/TerminalHeader.tsx`
     - `chat_app/src/components/terminal/TerminalStatusBanners.tsx`
     - `chat_app/src/components/terminal/TerminalCommandHistoryPanel.tsx`
   - 将头部连接状态/操作按钮、历史加载提示与错误提示、右侧命令历史列表从主组件迁出。
   - `TerminalView.tsx` 从约 `711` 行下降到约 `663` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

46. `ChatInterface` 聊天内容区拆分（本次新增）
   - 新增：
     - `chat_app/src/components/chatInterface/ChatConversationPane.tsx`
   - 将聊天主体区域（消息区、工具过程消息、输入区承载与主要交互编排）从 `ChatInterface` 主组件迁出，主文件聚焦会话级状态与 workbar 协调。
   - `ChatInterface.tsx` 从约 `666` 行下降到约 `614` 行。

47. `api/client.ts` 继续瘦身（本次新增）
   - 新增：
     - `chat_app/src/lib/api/client/fs.ts`
     - `chat_app/src/lib/api/client/messages.ts`
     - `chat_app/src/lib/api/client/memory.ts`
   - 变更：
     - `chat_app/src/lib/api/client/types.ts`（新增 `MemoryAgentsQueryOptions`）
     - `chat_app/src/lib/api/client.ts`（移除内联 `downloadFsEntry/createMessage/getMemoryAgents/getMemoryAgentRuntimeContext` 实现，改为模块转发）
   - `client.ts` 从约 `830` 行下降到约 `776` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

48. `SessionList` store 选择器抽离（本次新增）
   - 新增：
     - `chat_app/src/components/sessionList/useSessionListStoreState.ts`
   - 将 `SessionList` 内部超长的 Zustand selector 与字段解构迁出，主组件保留 UI 状态/事件编排与分区渲染。
   - `SessionList.tsx` 从约 `623` 行下降到约 `585` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

49. `ProjectExplorer` 本地状态初始化抽离（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerState.ts`
   - 将 `ProjectExplorer` 中 refs + `useState` 初始化集中到独立 hook，主组件聚焦目录行为编排与渲染逻辑。

50. `ProjectExplorer` 数据加载逻辑抽离（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/useProjectExplorerDataLoading.ts`
   - 将目录加载 `loadEntries` 与变更摘要加载 `loadChangeSummary` 迁出主组件，统一“silent 模式/并发保护/错误兜底”逻辑。
   - `ProjectExplorer.tsx` 从约 `659` 行下降到约 `623` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。

51. `TerminalView` 运行时逻辑模块化（本次新增）
   - 新增：
     - `chat_app/src/components/terminal/useTerminalRuntime.ts`
     - `chat_app/src/components/terminal/useTerminalViewState.ts`
     - `chat_app/src/components/terminal/useTerminalAppendCommands.ts`
     - `chat_app/src/components/terminal/useTerminalInstanceLifecycle.ts`
     - `chat_app/src/components/terminal/useTerminalSocketLifecycle.ts`
   - 将终端运行时状态、命令历史归并、xterm 初始化与历史加载、websocket 生命周期从 `TerminalView` 组件中抽离为独立 hooks，组件仅保留渲染层与参数装配。
   - `TerminalView.tsx` 从约 `663` 行下降到约 `93` 行。

52. `ProjectExplorer` 文件工作区渲染拆分（本次新增）
   - 新增：
     - `chat_app/src/components/projectExplorer/ProjectExplorerFilesWorkspace.tsx`
   - 将“文件树 + 预览区 + 变更日志侧栏 + 冲突弹窗 + 右键菜单”大段 JSX 从主组件迁出，`ProjectExplorer` 聚焦状态编排与行为逻辑。
   - `ProjectExplorer.tsx` 从约 `623` 行下降到约 `585` 行。

验证：

- `chat_app` 执行 `npm run build` 通过。
