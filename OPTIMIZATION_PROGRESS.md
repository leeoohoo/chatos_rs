# 优化进度跟进表

## 总览

| 阶段 | 状态 | 目标 | 结果 |
| --- | --- | --- | --- |
| 阶段 1 | 已完成 | 建立治理基线 | 已更新热点预算脚本，新增当前核心热点文件预算 |
| 阶段 2（第一轮） | 已完成 | 拆分 `openai-codex-gateway/server.py` 入口职责 | 已拆出 `gateway_runtime` 和 `gateway_http/handler.py`，`server.py` 保持兼容入口 |
| 阶段 2（第二轮） | 已完成 | 拆分 `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs` | 已按 use case 拆为多个子模块，`mod.rs` 收缩到 43 行 |
| 阶段 2（第三轮） | 已完成 | 拆分 `chat_app_server_rs/src/services/chatos_skills.rs` | 已拆出 discovery / manifest / git / helpers / types，主文件收缩到 629 行 |
| 阶段 3（第一轮） | 已完成 | 拆分 `code_nav` 的 Go / Python provider | 已将 Go / Python provider 拆成 `mod.rs + analysis.rs`，退出热点超预算列表 |
| 阶段 3（第二轮） | 已完成 | 拆分前端 `useSessionWorkbarPanels.ts` | 已拆出 pending / task realtime / ui prompt realtime，主 hook 收缩到 293 行 |
| 阶段 3（第三轮） | 已完成 | 拆分 `code_nav` 的 Java provider | 已拆出 `java/analysis.rs`，`mod.rs` 收缩到 581 行，热点预算全部达标 |
| 阶段 4（第一轮） | 已完成 | 抽取 `db_connection_hub` metadata common 共享逻辑 | 已新增共享 `drivers/metadata_common.rs`，收敛多 driver 重复分页与 node 解析逻辑 |
| 阶段 4（第二轮） | 已完成 | 收敛 `notepad` folder path 公共逻辑 | 已将 folder rename / ancestor / normalize helpers 统一到 `utils.ts` |
| 阶段 4（第三轮） | 已完成 | 抽取 `chat_app_server_rs` 文本规范化共享 helper | 已新增 `services/text_normalization.rs`，收敛多 service 的 trim/required/list 规范化逻辑 |
| 阶段 5（第四轮） | 已完成 | 收敛前端 domain normalize 共享 helper | 已将 `readFirst/readStringFirst/readNumberFirst/readBooleanFirst` 统一到 `normalizerUtils.ts`，并回收 `projectExplorer/codeNav/filesystem/projectSearch` 重复样板 |
| 阶段 5（第五轮） | 已完成 | 收敛前端 `projectExplorer/git` root cache 样板 | 已将 `peek/set/stale/getInflight/setInflight` 的重复流程统一到 `git/cache.ts` 内部共享 helper |
| 阶段 5（第六轮） | 已完成 | 收敛 `chatInterface` session 级缓存底座 | 已抽出 `sessionScopedCache.ts`，统一 `pendingTaskReview / pendingUiPrompt / uiPromptHistory` 三组 session cache 模板 |
| 阶段 5（第七轮） | 已完成 | 收敛 pending panel 预加载同步模板 | 已抽出 `pendingPanelSync.ts`，统一 `chatInterface` 与 `teamMembers` 的 cache-first panel 同步流程 |
| 阶段 5（第八轮） | 已完成 | 收敛 `teamMembers` mutation guard 与成员加载样板 | 已抽出 `useRecentMutationGuard.ts`，统一 teamMembers 与 task realtime 的近期 mutation 去重逻辑，并收敛成员加载状态机 |
| 阶段 6（第一轮） | 已完成 | 收敛 `chatInterface/workbarCache` 缓存样板 | 已将 history cache 接入 `sessionScopedCache.ts`，并统一 current-turn keyed cache 的内部读写 helper |
| 阶段 6（第二轮） | 已完成 | 拆分 `useWorkbarState.ts` 资源状态机 | 已新增 `useWorkbarTaskResourceState.ts`，将 current-turn/history 的加载、缓存、补丁、重置逻辑下沉为资源层 |
| 阶段 7（第一轮） | 已完成 | 收敛 chatInterface session 级 guarded async load 模板 | 已新增 `sessionLoadGuard.ts`，统一 `useWorkbarTaskResourceState / useUiPromptHistory / useContactMemoryContext` 的 requestSeq + currentSession + loading/error 守卫骨架 |
| 阶段 7（第二轮） | 已完成 | 拆解 `useContactMemoryContext.ts` 纯 helper 并扩展 request guard 复用 | 已新增 `contactMemoryContext.helpers.ts`，并将 `usePendingWorkbarPanels.ts` / `useContactProjectScope.ts` 的 requestSeq 判活样板接入 `sessionLoadGuard.ts` |
| 阶段 7（第三轮） | 已完成 | 拆出 `chatInterface` overlay/UI 开关状态并收敛 lazy overlay 包装 | 已新增 `useChatInterfaceOverlayState.ts`，并统一 `ChatInterfaceOverlays.tsx` 的重复 `Suspense` 包装样板 |
| 阶段 7（第四轮） | 已完成 | 拆解 `chatInterface/helpers.ts` 大工具文件 | 已按职责拆出 `panelTransforms / panelStateSync / toolCallHelpers / viewHelpers / workbarTransforms`，`helpers.ts` 收缩为薄 re-export 入口 |
| 阶段 7（第五轮） | 已完成 | 显式迁移 `chatInterface/helpers.ts` 核心依赖 | 已将 `workbar / panel / ui prompt / view helper` 的核心调用方直接切到新模块，`helpers.ts` 保持兼容层角色 |
| 阶段 7（第六轮） | 已完成 | 拆解 `workbarCache.ts` 缓存层大文件 | 已按 `current-turn / history / shared-state` 拆出多个模块，`workbarCache.ts` 收缩为薄 re-export 入口 |
| 阶段 7（第七轮） | 已完成 | 拆解 `useWorkbarMutations.ts` 纯逻辑与 mutation 模板 | 已新增 `workbarMutationHelpers.ts`，将 modal 校验、payload 构造、local patch/apply 模板下沉，主 hook 收回到事件编排层 |
| 阶段 7（第八轮） | 已完成 | 拆分 `usePanelActions.ts` 面板动作编排层 | 已新增 `useTaskReviewPanelActions.ts` / `useUiPromptPanelActions.ts` / `panelActionTypes.ts`，`usePanelActions.ts` 收缩为薄组合入口 |
| 阶段 7（当前） | 进行中 | 继续挖掘可抽象公共逻辑 | 下一步继续评估 `chatInterface` 相邻编排层与资源状态层的瘦身空间，优先选择边界已经清晰的小块继续推进 |

## 详细记录

### 阶段 1：治理基线

- 完成时间：2026-05-12
- 状态：已完成
- 变更：
  - 更新 `scripts/check-hotspot-line-budgets.sh`
  - 将以下热点纳入预算：
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
    - `chat_app_server_rs/src/services/chatos_skills.rs`
    - `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs`
    - `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs`
    - `chat_app_server_rs/src/services/code_nav/languages/go/mod.rs`
    - `chat_app_server_rs/src/services/code_nav/languages/python/mod.rs`
    - `openai-codex-gateway/server.py`
- 说明：
  - 预算数值采用“先纳入治理、后逐步收紧”的策略，先建立持续约束，再配合后续拆分逐步下降。

### 阶段 2：`openai-codex-gateway/server.py` 模块化

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 降低 `server.py` 入口文件职责密度
  - 保持现有测试和启动入口兼容
- 变更：
  - 新增 `openai-codex-gateway/gateway_runtime/sdk_types.py`
  - 新增 `openai-codex-gateway/gateway_runtime/turn_state.py`
  - 新增 `openai-codex-gateway/gateway_runtime/tool_guard.py`
  - 新增 `openai-codex-gateway/gateway_runtime/bridge.py`
  - 新增 `openai-codex-gateway/gateway_runtime/entrypoint.py`
  - 新增 `openai-codex-gateway/gateway_http/handler.py`
  - 将 `openai-codex-gateway/server.py` 收缩为兼容导出入口
- 拆分结果：
  - runtime 状态、tool guard、bridge 执行逻辑独立
  - HTTP handler 和 server 生命周期独立
  - 启动入口独立
  - `server.py` 仍保留原对外导出符号，避免打断现有测试与调用

### 阶段 2：`chat_app_server_rs/src/services/chatos_memory_engine/mod.rs` 模块化

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 按 use case 拆解 memory engine 服务
  - 保持现有对外 API 不变
- 变更：
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/types.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/client.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/mappers.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/sessions.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/review_repair.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/snapshots.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_memory_engine/memories.rs`
  - 将 `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs` 收缩为导出组织层
- 拆分结果：
  - `mod.rs` 从 1255 行降到 43 行
  - session / message / summary / repair / snapshot / memory 查询逻辑独立
  - engine model 映射逻辑独立
  - client 构建逻辑独立
- 验证：
  - `cargo check` 通过

### 阶段 2：`chat_app_server_rs/src/services/chatos_skills.rs` 模块化

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 将 skills 服务从“大而全”重构为编排层
  - 拆出 discovery、manifest、git cache、公共 helper、类型定义
- 变更：
  - 新增 `chat_app_server_rs/src/services/chatos_skills_types.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_skills_helpers.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_skills_manifest.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_skills_git.rs`
  - 新增 `chat_app_server_rs/src/services/chatos_skills_discovery.rs`
  - 更新 `chat_app_server_rs/src/services/mod.rs`
  - 将 `chat_app_server_rs/src/services/chatos_skills.rs` 收缩为对外 API + 编排逻辑
- 拆分结果：
  - `chatos_skills.rs` 从 1692 行降到 629 行
  - plugin 发现、markdown 解析、git clone/copy、缓存刷新逻辑独立
  - 安装流程仍保持对外接口不变
- 验证：
  - `cargo check` 通过
  - 热点脚本中 `chatos_skills.rs` 已不再超预算

### 阶段 3：`code_nav` Go / Python provider 拆分

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 压低 `code_nav` 多语言 provider 单文件体积
  - 先对结构最相似的 Go / Python 做样板拆分
- 变更：
  - 新增 `chat_app_server_rs/src/services/code_nav/languages/go/analysis.rs`
  - 新增 `chat_app_server_rs/src/services/code_nav/languages/python/analysis.rs`
  - 将 Go / Python 的 `mod.rs` 收缩为 provider 外壳和 definition/references 编排层
- 拆分结果：
  - `go/mod.rs` 从 1081 行降到 459 行
  - `python/mod.rs` 从 1033 行降到 460 行
  - 热点脚本中 Go / Python 已退出超预算列表
- 验证：
  - `cargo check` 通过
  - 热点脚本当前只剩：
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
    - `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs`

### 阶段 3：`useSessionWorkbarPanels.ts` 前端拆分

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 将会话 workbar 主 hook 收缩为编排层
  - 拆出 pending panel 同步与 realtime 刷新队列逻辑
- 变更：
  - 新增 `chat_app/src/components/chatInterface/useSessionWorkbarTaskRealtime.ts`
  - 新增 `chat_app/src/components/chatInterface/useSessionWorkbarUiPromptRealtime.ts`
  - 接入已准备好的：
    - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.types.ts`
    - `chat_app/src/components/chatInterface/useTaskRealtimeMutationGuard.ts`
    - `chat_app/src/components/chatInterface/usePendingWorkbarPanels.ts`
  - 将 `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts` 收缩为状态拼装 + 对外返回层
- 拆分结果：
  - `useSessionWorkbarPanels.ts` 从 733 行降到 293 行
  - task board realtime 去重、队列刷新与局部 patch 逻辑独立
  - ui prompt realtime 去重与历史刷新逻辑独立
  - pending review / prompt 面板加载逻辑独立
- 验证：
  - `cd chat_app && npm run type-check` 通过

### 阶段 3：`code_nav` Java provider 拆分

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 压低 `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs` 体积
  - 对齐 Go / Python provider 的模块化结构
- 变更：
  - 新增 `chat_app_server_rs/src/services/code_nav/languages/java/analysis.rs`
  - 将 `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs` 收缩为 provider 外壳、definition/reference 编排与测试入口
- 拆分结果：
  - `java/mod.rs` 从 1453 行降到 581 行
  - Java 文件分析、import/type 解析、符号检索、声明分类、注释处理逻辑独立到 `analysis.rs`
  - Java / Go / Python 三个 provider 现在形成统一的 `mod.rs + analysis.rs` 结构
- 验证：
  - `cd chat_app_server_rs && cargo check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过
  - 当前热点预算文件已全部回到阈值内

### 阶段 4：`db_connection_hub` metadata common 抽象

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `db_connection_hub/backend` 多个 driver 中重复的 metadata helper
  - 保持各 driver 现有导出函数名和调用方式不变
- 变更：
  - 新增 `db_connection_hub/backend/src/drivers/metadata_common.rs`
  - 更新 `db_connection_hub/backend/src/drivers/mod.rs`
  - 更新以下 driver metadata common：
    - `db_connection_hub/backend/src/drivers/sqlite/metadata/common.rs`
    - `db_connection_hub/backend/src/drivers/postgres/metadata/common.rs`
    - `db_connection_hub/backend/src/drivers/mysql/metadata/common.rs`
    - `db_connection_hub/backend/src/drivers/mongodb/metadata/common.rs`
    - `db_connection_hub/backend/src/drivers/oracle/metadata/common.rs`
    - `db_connection_hub/backend/src/drivers/sqlserver/metadata/common.rs`
- 拆分结果：
  - 统一了 metadata 节点分页逻辑
  - 统一了 `db:` 节点与 `prefix:a:b:c` 这类 node id 的基础解析规则
  - 各 driver `common.rs` 保留原有对外函数名，仅作为薄封装承接共享 helper
  - `common.rs` 总行数从 723 行降到 465 行
- 验证：
  - `cd db_connection_hub/backend && cargo check` 通过

### 阶段 4：`notepad` folder path 公共逻辑收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 去除 `notepad` 多个 hook 中重复的 folder path 归一化、祖先收集、删除/重命名映射逻辑
  - 将 folder path 规则收敛到单一工具层
- 变更：
  - 更新 `chat_app/src/components/notepad/utils.ts`
  - 更新 `chat_app/src/components/notepad/useNotepadData.ts`
  - 更新 `chat_app/src/components/notepad/useNotepadPanelController.ts`
- 拆分结果：
  - 新增并集中复用：
    - `normalizeFolders`
    - `collectFolderAncestors`
    - `removeFolderAndDescendants`
    - `renameFolderAndDescendants`
  - `useNotepadData` 与 `useNotepadPanelController` 改为共享同一套 folder path 规则
  - 避免 folder rename 规则在两个 hook 中继续分叉
- 验证：

### 阶段 7：chatInterface guarded async load 模板收敛

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 收敛 `chatInterface` 多个 session 级资源 hook 中重复的 `requestSeq + currentSession + loading/error + inflight` 守卫样板
  - 保持各 hook 现有对外 API 和 cache 语义不变
- 变更：
  - 新增 `chat_app/src/components/chatInterface/sessionLoadGuard.ts`
  - 更新 `chat_app/src/components/chatInterface/useWorkbarTaskResourceState.ts`
  - 更新 `chat_app/src/components/chatInterface/useUiPromptHistory.ts`
  - 更新 `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
- 收敛结果：
  - 统一了 session 级异步加载的 request 序号推进、当前 session 校验、loading/error 生命周期处理
  - `useUiPromptHistory` 的 inflight 复用和 fresh load 分支共享同一套 guarded load 模板
  - `useContactMemoryContext` 两个 loader 现在共用同一套守卫骨架，并顺手收敛了 memory load key / cache entry 应用样板
  - `useWorkbarTaskResourceState` 不再内联专用 guarded load helper，转为复用共享 `sessionLoadGuard.ts`
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`useContactMemoryContext` 纯 helper 拆解与邻近 request guard 收敛

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 继续压低 `useContactMemoryContext.ts` 的文件体积和职责密度
  - 将相邻 hook 中重复的 requestSeq 推进与“是否仍为当前请求”判活样板统一到共享 helper
- 变更：
  - 新增 `chat_app/src/components/chatInterface/contactMemoryContext.helpers.ts`
  - 更新 `chat_app/src/components/chatInterface/useContactMemoryContext.ts`
  - 更新 `chat_app/src/components/chatInterface/sessionLoadGuard.ts`
  - 更新 `chat_app/src/components/chatInterface/usePendingWorkbarPanels.ts`
  - 更新 `chat_app/src/components/chatInterface/useContactProjectScope.ts`
- 收敛结果：
  - `useContactMemoryContext.ts` 中的 memory load key 构建、agent recall 归一化、cache entry 类型下沉到独立 helper 文件
  - `useContactMemoryContext.ts` 从 357 行降到 301 行，主 hook 更聚焦于状态编排
  - `sessionLoadGuard.ts` 新增通用 `isLoadRequestCurrent`，统一“requestSeq 是否仍有效”的基础判断
  - `usePendingWorkbarPanels` 与 `useContactProjectScope` 不再手写 `++ref / ref.current === seq` 判活样板
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`useChatInterfaceController` / `ChatInterfaceOverlays` 编排层瘦身

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `useChatInterfaceController.ts` 中独立的 overlay/UI 开关状态下沉到专门 hook
  - 收敛 `ChatInterfaceOverlays.tsx` 中重复的 lazy overlay `Suspense` 包装样板
- 变更：
  - 新增 `chat_app/src/components/chatInterface/useChatInterfaceOverlayState.ts`
  - 更新 `chat_app/src/components/chatInterface/useChatInterfaceController.ts`
  - 更新 `chat_app/src/components/chatInterface/ChatInterfaceOverlays.tsx`
- 收敛结果：
  - `useChatInterfaceController.ts` 不再内联维护多组 overlay 可见性 state，改为复用 `useChatInterfaceOverlayState.ts`
  - `useChatInterfaceController.ts` 从 367 行降到 349 行，职责更聚焦于事件编排与资源联动
  - `ChatInterfaceOverlays.tsx` 新增共享 `LazyOverlay` 包装，去掉多处重复的 `Suspense + fallback` 结构
  - 对外返回字段与 overlay 打开/关闭行为保持不变
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`chatInterface/helpers.ts` 大文件职责拆解

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `chat_app/src/components/chatInterface/helpers.ts` 由“大而杂”的多职责工具文件拆解为按领域分组的小模块
  - 降低后续继续抽象时的耦合度，同时保持当前调用方迁移风险可控
- 变更：
  - 新增 `chat_app/src/components/chatInterface/panelTransforms.ts`
  - 新增 `chat_app/src/components/chatInterface/panelStateSync.ts`
  - 新增 `chat_app/src/components/chatInterface/toolCallHelpers.ts`
  - 新增 `chat_app/src/components/chatInterface/viewHelpers.ts`
  - 新增 `chat_app/src/components/chatInterface/workbarTransforms.ts`
  - 重建 `chat_app/src/components/chatInterface/helpers.ts` 为薄 re-export 入口
- 拆解结果：
  - `helpers.ts` 从 641 行降到 32 行
  - panel 记录归一化、panel snapshot 同步、tool call 解析、view helper、workbar normalizer 现在分属独立模块
  - 当前调用方仍可通过 `helpers.ts` 兼容访问，便于后续按需逐步迁移为显式依赖
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`helpers.ts` 核心调用方显式依赖迁移

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 在完成 `helpers.ts` 职责拆解后，让核心业务调用方直接依赖对应新模块
  - 将 `helpers.ts` 进一步收敛为兼容入口，而不是继续承载主要业务引用
- 变更：
  - 更新 `chat_app/src/components/chatInterface/useWorkbarState.ts`
  - 更新 `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
  - 更新 `chat_app/src/components/chatInterface/useChatInterfaceDerivedState.ts`
  - 更新 `chat_app/src/components/chatInterface/useGlobalConversationPanelsRealtime.ts`
  - 更新 `chat_app/src/components/projectExplorer/teamMembers/TeamMemberWorkspace.tsx`
  - 更新 `chat_app/src/components/chatInterface/useWorkbarMutations.ts`
  - 更新 `chat_app/src/components/chatInterface/useWorkbarTaskResourceState.ts`
  - 更新 `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
  - 更新 `chat_app/src/components/chatInterface/pendingUiPromptCache.ts`
  - 更新 `chat_app/src/components/chatInterface/pendingTaskReviewCache.ts`
  - 更新 `chat_app/src/components/chatInterface/useUiPromptHistory.ts`
  - 更新 `chat_app/src/components/chatInterface/useSessionWorkbarTaskRealtime.ts`
  - 更新 `chat_app/src/components/chatInterface/useSessionWorkbarUiPromptRealtime.ts`
  - 更新 `chat_app/src/components/chatInterface/usePendingWorkbarPanels.ts`
  - 更新 `chat_app/src/components/chatInterface/useOverlayDrawerProps.ts`
- 收敛结果：
  - `workbar` 相关逻辑显式依赖 `workbarTransforms.ts` / `toolCallHelpers.ts`
  - panel 归一化与同步逻辑显式依赖 `panelTransforms.ts` / `panelStateSync.ts`
  - view 层格式化与 model support helper 显式依赖 `viewHelpers.ts`
  - `chatInterface` 与 `teamMembers` 范围内已无核心业务文件继续从 `helpers.ts` 取值，`helpers.ts` 保留为兼容 re-export 层
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`workbarCache.ts` 缓存层大文件拆解

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `chat_app/src/components/chatInterface/workbarCache.ts` 从“多职责缓存实现文件”拆成按领域分组的缓存模块
  - 保持 `useWorkbarTaskResourceState.ts` 等上层调用点的导出接口稳定
- 变更：
  - 新增 `chat_app/src/components/chatInterface/workbarCache.shared.ts`
  - 新增 `chat_app/src/components/chatInterface/workbarCacheState.ts`
  - 新增 `chat_app/src/components/chatInterface/workbarCurrentTurnCache.ts`
  - 新增 `chat_app/src/components/chatInterface/workbarHistoryCache.ts`
  - 重建 `chat_app/src/components/chatInterface/workbarCache.ts` 为薄 re-export 入口
- 拆解结果：
  - `workbarCache.ts` 从 411 行降到 18 行
  - current-turn cache key/inflight/patch/remove 逻辑独立到 `workbarCurrentTurnCache.ts`
  - history cache 读写、stale、inflight 逻辑独立到 `workbarHistoryCache.ts`
  - 共享类型、list patch helper、WeakMap state 底座分别下沉到 `shared/state` 模块
  - 上层调用方仍继续通过 `workbarCache.ts` 访问，避免一次性改动过多引用点
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`useWorkbarMutations.ts` 纯逻辑与 mutation 模板下沉

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `useWorkbarMutations.ts` 中的 modal draft 校验、payload 构造、local patch/apply 模板抽离到 helper 层
  - 让主 hook 更聚焦于 UI 状态与 mutation 事件编排
- 变更：
  - 新增 `chat_app/src/components/chatInterface/workbarMutationHelpers.ts`
  - 更新 `chat_app/src/components/chatInterface/useWorkbarMutations.ts`
- 收敛结果：
  - `useWorkbarMutations.ts` 从 389 行降到 263 行
  - `TaskModalMode`、`WorkbarMutationResult`、draft 规范化、校验、update payload 构造、realtime guard payload 构造、local mutation 结果应用逻辑下沉到 helper
  - 主 hook 现在主要负责弹窗状态、删除确认和 mutation 调用编排
  - 现有对外 API 与任务完成/编辑/删除行为保持不变
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 7：`usePanelActions.ts` 面板动作编排拆分

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `task review` 与 `ui prompt` 两类面板动作从同一个 hook 中拆开，降低 `usePanelActions.ts` 的职责密度
  - 保持 `useSessionWorkbarPanels.ts` 对外使用的 API 不变
- 变更：
  - 新增 `chat_app/src/components/chatInterface/panelActionTypes.ts`
  - 新增 `chat_app/src/components/chatInterface/useTaskReviewPanelActions.ts`
  - 新增 `chat_app/src/components/chatInterface/useUiPromptPanelActions.ts`
  - 重建 `chat_app/src/components/chatInterface/usePanelActions.ts` 为薄组合入口
- 收敛结果：
  - `usePanelActions.ts` 从 293 行降到 27 行
  - `task review` 的 pending panel 状态、提交、取消、workbar 刷新逻辑下沉到 `useTaskReviewPanelActions.ts`
  - `ui prompt` 的 pending panel 状态、提交、取消、history 刷新逻辑下沉到 `useUiPromptPanelActions.ts`
  - `usePanelActions.ts` 现在只负责组合两个子 hook 并保持原有返回接口
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过
  - `cd chat_app && npm run type-check` 通过

### 阶段 4：`chat_app_server_rs` 文本规范化 helper 收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `chat_app_server_rs/src/services` 中重复的文本 trim、required 校验、字符串列表去重、visible user scope 规范化逻辑
  - 保持各 service 现有行为与调用面稳定
- 变更：
  - 新增 `chat_app_server_rs/src/services/text_normalization.rs`
  - 更新：
    - `chat_app_server_rs/src/services/mod.rs`
    - `chat_app_server_rs/src/services/chatos_skills_helpers.rs`
    - `chat_app_server_rs/src/services/chatos_agents.rs`
    - `chat_app_server_rs/src/services/task_board_prompt.rs`
    - `chat_app_server_rs/src/services/chatos_memory_engine/mapping.rs`
    - `chat_app_server_rs/src/services/agent_builder.rs`
- 拆分结果：
  - 统一了：
    - `normalize_optional_text_ref`
    - `normalize_optional_text_owned`
    - `normalize_required_text_owned`
    - `normalize_string_vec`
    - `resolve_visible_user_ids`
  - `chatos_skills_helpers` 保留原有 helper 导出面，对外调用方无需改接口
  - 降低了 agents / skills / agent_builder / task board / memory mapping 间的文本规范化重复实现
- 验证：
  - `cd chat_app_server_rs && cargo check` 通过

### 阶段 5：前端 domain normalize 共享 helper 收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `chat_app/src/lib/domain` 中重复的 record 读值 helper
  - 让 `projectExplorer / codeNav / filesystem / projectSearch` 共享同一套 normalize 基础能力
- 变更：
  - 更新 `chat_app/src/lib/domain/normalizerUtils.ts`
  - 更新 `chat_app/src/lib/domain/filesystem.ts`
  - 更新 `chat_app/src/lib/domain/projectSearch.ts`
  - 更新 `chat_app/src/lib/domain/projectExplorer.ts`
  - 更新 `chat_app/src/lib/domain/codeNav.ts`
- 拆分结果：
  - 统一了：
    - `readFirst`
    - `readStringFirst`
    - `readNullableStringFirst`
    - `readNumberFirst`
    - `readBooleanFirst`
  - `filesystem / projectSearch / projectExplorer / codeNav` 不再各自维护一份相同的 record 读取模板
  - 保留了原有调用语义，包括 `readString` 的 fallback 兼容能力
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 5：`projectExplorer/git` root cache 样板收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `chat_app/src/components/projectExplorer/git/cache.ts` 中按 `projectRoot` 分组缓存的重复模板
  - 保持 `useProjectGit.ts` 现有调用面与行为稳定
- 变更：
  - 更新 `chat_app/src/components/projectExplorer/git/cache.ts`
- 拆分结果：
  - 统一了：
    - `peekProjectRootCacheEntry`
    - `setProjectRootCacheEntry`
    - `updateProjectRootCacheEntry`
    - `getProjectRootInflight`
    - `setProjectRootInflight`
  - `summary/details` 两组 root cache 的 `peek/set/stale/inflight` 重复样板由内部 helper 统一承接
  - 保留了所有原有对外导出函数名，`useProjectGit.ts` 无需调整调用方式
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 5：`chatInterface` session 级缓存底座收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `chatInterface` 中多个 session 维度缓存模块的重复模板
  - 统一 pending panel / ui prompt history 这类缓存的 session key、cache entry、inflight 管理
- 变更：
  - 新增 `chat_app/src/components/chatInterface/sessionScopedCache.ts`
  - 更新 `chat_app/src/components/chatInterface/pendingTaskReviewCache.ts`
  - 更新 `chat_app/src/components/chatInterface/pendingUiPromptCache.ts`
  - 更新 `chat_app/src/components/chatInterface/uiPromptHistoryCache.ts`
- 拆分结果：
  - 统一了：
    - `normalizeSessionScopedId`
    - `peekSessionScopedCacheEntry`
    - `setSessionScopedCacheEntry`
    - `markSessionScopedCacheStale`
    - `getSessionScopedInflight`
    - `setSessionScopedInflight`
  - `pendingTaskReview / pendingUiPrompt / uiPromptHistory` 三组缓存不再各自维护同一套 session cache / inflight 样板
  - 保留了原有对外导出函数名，调用方无需修改协议
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 5：pending panel 预加载同步模板收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `usePendingWorkbarPanels.ts` 与 `teamMembers/useTeamMembersRuntimeResources.ts` 中重复的 cache-first panel 预加载流程
  - 统一“先读缓存，再决定异步加载，最后同步 snapshot”的执行模板
- 变更：
  - 新增 `chat_app/src/components/chatInterface/pendingPanelSync.ts`
  - 更新 `chat_app/src/components/chatInterface/usePendingWorkbarPanels.ts`
  - 更新 `chat_app/src/components/projectExplorer/teamMembers/useTeamMembersRuntimeResources.ts`
- 拆分结果：
  - 统一了 pending panel 的 cache-hit / cache-miss / load-complete 三段式同步流程
  - `usePendingWorkbarPanels.ts` 的双 effect 重复样板收缩为共享 helper 驱动
  - `teamMembers` 的多 session pending panel 预热流程改为同一 helper 编排
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 5：`teamMembers` mutation guard 与成员加载收敛

- 完成时间：2026-05-12
- 状态：已完成
- 目标：
  - 收敛 `teamMembers/useProjectMembersManager.ts` 中近期 mutation 去重模板
  - 合并项目成员加载的重复异步状态机，避免同一套加载 / 错误 / 取消逻辑维护两份
- 变更：
  - 新增 `chat_app/src/hooks/useRecentMutationGuard.ts`
  - 更新 `chat_app/src/components/chatInterface/useTaskRealtimeMutationGuard.ts`
  - 更新 `chat_app/src/components/projectExplorer/teamMembers/useProjectMembersManager.ts`
- 拆分结果：
  - 统一了近期 mutation guard 的 key 构建、标记与消费时效逻辑
  - `useTaskRealtimeMutationGuard.ts` 改为复用共享 hook
  - `useProjectMembersManager.ts` 中成员首次加载与 reload 流程收敛为同一条异步状态机
  - `project_contact_added / project_contact_removed` 的 guard 逻辑改为结构化 payload 驱动
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 6：`chatInterface/workbarCache` 缓存样板收敛

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `workbarCache.ts` 的 history session cache 对齐到现有 `sessionScopedCache.ts` 底座
  - 收敛 current-turn keyed cache 的内部 `peek/set/stale/inflight` 样板
- 变更：
  - 更新 `chat_app/src/components/chatInterface/workbarCache.ts`
- 拆分结果：
  - `historyCache/historyInflight` 改为复用 `sessionScopedCache.ts` 的 session 级缓存 helper
  - 新增 current-turn 内部 helper，统一：
    - `peekCurrentTurnCacheEntry`
    - `setCurrentTurnCacheEntry`
    - `updateCurrentTurnCacheEntry`
    - `getCurrentTurnInflight`
    - `setCurrentTurnInflight`
  - 保留了 `useWorkbarState.ts` 现有调用面，不改外部协议
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

### 阶段 6：`useWorkbarState.ts` 资源状态机拆分

- 完成时间：2026-05-13
- 状态：已完成
- 目标：
  - 将 `useWorkbarState.ts` 中 current-turn/history 两套资源加载、缓存、补丁、重置逻辑下沉
  - 让 `useWorkbarState.ts` 回到“派生数据 + 编排层”的职责边界
- 变更：
  - 新增 `chat_app/src/components/chatInterface/useWorkbarTaskResourceState.ts`
  - 更新 `chat_app/src/components/chatInterface/useWorkbarState.ts`
- 拆分结果：
  - 新资源 hook 统一承接：
    - current-turn/history 的缓存命中判断
    - inflight 复用
    - requestSeq/currentSession 防抖
    - 本地 patch/remove
    - stale/reset 流程
  - `useWorkbarState.ts` 从大体量资源状态机收缩为 active turn 识别、mutation fallback 推导和结果拼装层
- 验证：
  - `cd chat_app && npm run type-check` 通过
  - `bash scripts/check-hotspot-line-budgets.sh` 通过

## 当前状态

- 已完成的热点治理目标：
  - `openai-codex-gateway/server.py`
  - `chat_app_server_rs/src/services/chatos_memory_engine/mod.rs`
  - `chat_app_server_rs/src/services/chatos_skills.rs`
  - `chat_app_server_rs/src/services/code_nav/languages/go/mod.rs`
  - `chat_app_server_rs/src/services/code_nav/languages/python/mod.rs`
  - `chat_app_server_rs/src/services/code_nav/languages/java/mod.rs`
  - `chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts`
- 当前热点脚本结果：
  - 所有预算项均已达标
- 已开始的横向抽象治理：
  - `db_connection_hub` metadata helper 共享层已建立
  - `notepad` folder path helper 共享层已建立
  - `chat_app_server_rs` text normalization helper 共享层已建立

## 下一步

建议按优先级继续：

1. 评估 `projectExplorer` / `git` / `codeNav` 前端 domain normalize 函数的进一步收敛空间
2. 继续扫描 `chat_app_server_rs/src/services` 下剩余跨 provider / 跨 service 的重复转换逻辑
3. 补充更细粒度的热点预算，进入下一轮治理
