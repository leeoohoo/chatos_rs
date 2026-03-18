# Project 区域 Tab 化与“团队成员”新页面改造方案

## 1. 目标

在你标红的位置（当前 `ProjectExplorer` 顶部区域）新增一级 `Tab`：

1. `项目目录`（保留现有“目录树 + 文件预览”页面）
2. `团队成员`（全新页面，显示已添加联系人，并可直接聊天）

关键约束：

- 点击联系人后要能聊天，但**不能跳转到当前主聊天页面**（`activePanel` 不能被强制切回 `chat`）。
- 组件可以复用，但“团队成员”必须是项目面板内的**新页面**，不是复用现有聊天页路由。
- 原联系人聊天页（`activePanel=chat`）底部不再使用“项目文件 + 项目下拉”这两个选择，改为“本地目录选择（非项目）”。
- 选择该目录后仅作为 MCP 探索上下文使用，**不创建新会话、不绑定项目、不触发项目切换**。

---

## 2. 现状分析（基于当前代码）

### 2.1 页面结构

- `chat_app/src/components/ChatInterface.tsx`
  - `activePanel === 'project'` 时渲染 `ProjectExplorer`。
- `chat_app/src/components/ProjectExplorer.tsx`
  - 当前只包含一套内容：`ProjectTreePane + ProjectPreviewPane (+ ChangeLog)`。

### 2.2 会话切换现状（阻碍点）

- `chat_app/src/lib/store/actions/sessions.ts`
  - `selectSession` 内部固定 `state.activePanel = 'chat'`。
  - `createSession` 内部固定 `state.activePanel = 'chat'`。

这意味着：只要在新“团队成员”页里选中/创建会话，就会被强制跳回主聊天页，和你的要求冲突。

### 2.3 可复用能力

- 可复用消息展示：`MessageList`
- 可复用输入区：`InputArea`（含模型/MCP/项目透传）
- 可复用联系人与会话关联信息：
  - `contacts`（联系人）
  - `sessions`（按联系人+项目聚合后的会话）
  - `session metadata` 中已有 `contactId/contactAgentId/projectId/projectRoot`

### 2.4 主聊天页当前不符合点

- `InputArea` 当前存在项目相关的两处选择：
  - `项目文件`（基于当前项目根目录）
  - `项目下拉`（`selectedProjectId`）
- 当前 MCP 的“需要项目目录”能力是跟 `selectedProjectId/currentProject` 绑定的，不适配“非项目目录”的探索诉求。

---

## 3. 总体设计

## 3.1 ProjectExplorer 顶层 Tab 容器化

在 `ProjectExplorer` 增加顶层页签状态 `workspaceTab`：

- `workspaceTab = 'files'`：渲染现有文件工作区（不改原业务）
- `workspaceTab = 'team'`：渲染新的“团队成员”页面

建议默认值：`files`。
建议按项目记忆：`localStorage['project_workspace_tab_<projectId>']`。

---

## 3.2 “团队成员”页布局（新页面，不跳主聊天页）

新组件：`chat_app/src/components/projectExplorer/TeamMembersPane.tsx`

建议布局：

- 左侧：联系人列表（用户已添加的联系人）
- 右侧：该联系人的聊天区
  - 上部 `MessageList`
  - 下部 `InputArea`

交互：

1. 进入团队成员页 -> 自动选中第一个联系人（若有）。
2. 点击联系人 -> 加载/确保该联系人在当前项目下的会话 -> 显示消息。
3. 发送消息 -> 使用当前会话发送，停留在 `project/team` 页。

---

## 3.3 关键状态改造（必须做）

为避免跳页，扩展 store action 签名（保持向后兼容，默认行为不变）：

1. `selectSession(sessionId, options?)`
   - 新增可选参数：`{ keepActivePanel?: boolean }`
   - 默认 `false`（保持原行为：切到 `chat`）
   - 当 `true` 时，不改 `state.activePanel`

2. `createSession(payload, options?)`
   - 新增可选参数：`{ keepActivePanel?: boolean }`
   - 默认 `false`
   - 当 `true` 时，不改 `state.activePanel`

团队成员页调用：

- `selectSession(..., { keepActivePanel: true })`
- `createSession(..., { keepActivePanel: true })`

这样“选联系人聊天”只切会话，不跳主聊天页。

---

## 3.4 团队成员数据来源

第一阶段（最小改动，优先落地）：

- 直接基于现有 `contacts` 展示联系人列表（不新增后端接口）。
- 点击联系人时按 `contact + currentProject` 查找会话：
  - 有会话：直接切换（保持在项目页）。
  - 无会话：自动创建该联系人在当前项目下的会话并进入聊天。
- 该方案无后端改动，可直接上线。

第二阶段（可选优化）：

- 增加后端接口 `GET /api/projects/:id/contacts`，避免前端推导和潜在 N+1。
- 仅优化性能与可维护性，不是首版必需。

---

## 3.5 主聊天页改造为“工作目录模式”（非项目）

适用范围：`activePanel = 'chat'`（原联系人聊天页）。

目标行为：

1. 去掉原底部两个项目相关选择：`项目文件`、`项目下拉`。
2. 新增一个目录选择器：`工作目录`（选择本机目录路径）。
3. 该目录仅用于 MCP 探索上下文，不是项目，不进入项目列表，不触发 `selectProject/createProject`。
4. 切换工作目录时只更新运行时配置，不新建会话。

---

## 3.6 运行时上下文字段解耦（项目 vs 工作目录）

为避免语义混淆，运行时分两类：

- 项目上下文：`projectId/projectRoot`（仅项目页、团队成员页使用）
- 工作目录上下文：`workspaceRoot`（主聊天页使用，非项目）

发送时统一规则：

1. 若显式选择了 `workspaceRoot`，则 MCP 目录上下文优先用它。
2. 若无 `workspaceRoot`，再回退 `projectRoot`。
3. 目录上下文变化不触发会话创建，只影响本次/当前会话的运行时参数。

---

## 4. 详细改造清单

## 4.1 前端组件层

1. `chat_app/src/components/ProjectExplorer.tsx`
   - 新增 tab 状态与 tab bar 渲染。
   - 将现有文件区域抽为 `FilesWorkspacePane`（或保留原结构分支）。
   - 新增 `TeamMembersPane` 分支。

2. 新增 `chat_app/src/components/projectExplorer/WorkspaceTabs.tsx`
   - 仅负责你红框位置的 tab UI。
   - tab：`项目目录`、`团队成员`。

3. 新增 `chat_app/src/components/projectExplorer/TeamMembersPane.tsx`
   - 使用 store（`contacts/sessions/currentProject/currentSession/messages/...`）。
   - 实现联系人列表、选中、消息展示、发送。

4. 可选新增 `chat_app/src/components/projectExplorer/teamHelpers.ts`
   - 统一解析 session 的 `contact/project` 绑定信息，避免 `SessionList` 与 `TeamMembersPane` 重复逻辑。

---

## 4.2 Store / Action 层

1. `chat_app/src/lib/store/types.ts`
   - 更新 action 类型：
     - `createSession(payload?, options?)`
     - `selectSession(sessionId, options?)`

2. `chat_app/src/lib/store/actions/sessions.ts`
   - 在 `createSession/selectSession` 内按 `keepActivePanel` 决定是否改 `activePanel='chat'`。
   - 默认行为维持不变，确保现有页面无回归。

---

## 4.3 主聊天页输入区改造（非项目目录）

1. `chat_app/src/components/InputArea.tsx`
   - 新增“工作目录选择”UI（目录选择弹层/对话框）。
   - 移除主聊天页下的 `项目文件` 按钮与 `项目下拉`。
   - MCP 可用性判断改为：有 `workspaceRoot` 或有 `projectRoot` 即可启用目录相关 MCP。

2. `chat_app/src/components/ChatInterface.tsx`
   - 在主聊天页维护 `workspaceRoot` 状态并传给 `InputArea`。
   - 发送时透传 `workspaceRoot`，不调用任何项目切换逻辑。

3. `chat_app/src/types/index.ts` 与 `chat_app/src/lib/store/types.ts`
   - `InputAreaProps` 与 `SendMessageRuntimeOptions` 增加 `workspaceRoot?: string | null`。

4. `chat_app/src/lib/store/actions/sendMessage.ts`
   - 运行时目录解析改为优先使用 `workspaceRoot`。
   - 不要求 `projectId != 0` 才能带目录上下文。
   - 保持“不新建会话”的原则：仅更新/使用会话 runtime metadata。

---

## 4.4 聊天复用策略

“团队成员”页右侧聊天建议复用：

- `MessageList`（消息区）
- `InputArea`（输入区）

首版不复用 `ChatComposerPanel`（包含任务条、确认面板、总结入口，耦合较重），避免把主聊天页复杂交互整套搬入项目页。

首版只保留必要能力：

- 发送/停止
- 模型选择
- MCP 开关与选择
- 附件

项目选择在团队成员页内固定为当前项目（`selectedProjectId = currentProject.id`），避免误发到其他项目。

---

## 5. 关键流程

## 5.1 打开团队成员页

1. 用户切到 `团队成员` tab。
2. 前端展示已添加联系人列表（`contacts`）。
3. 若当前已选联系人不存在，自动选第一条。

## 5.2 点击联系人

1. 用 `contactId + currentProjectId` 查找现有会话。
2. 若存在：`selectSession(id, { keepActivePanel: true })`。
3. 若不存在：`createSession(payload, { keepActivePanel: true })`，随后选中。
4. 页面保持在 `project/team`。

## 5.3 发送消息

1. 通过 `sendMessage(... runtimeOptions)` 发送。
2. runtime 强制带当前项目 `projectId/projectRoot`。
3. MCP 能力沿用 `InputArea` 规则（无项目时禁用项目依赖 MCP；本页通常有项目）。

## 5.4 主聊天页选择工作目录（非项目）

1. 用户在主聊天页点击 `工作目录`，选择本机目录。
2. 前端仅更新 `workspaceRoot` 运行时状态，不创建/切换会话。
3. 用户发送消息时透传该目录作为 MCP 探索上下文。
4. 该行为不写入项目关联，不影响 `PROJECTS` 列表。

---

## 6. 风险与规避

1. 风险：改 `selectSession/createSession` 影响原交互。
   - 规避：新增可选参数，默认值保持旧行为。

2. 风险：团队成员列表较大时，首屏排序和会话匹配开销上升。
   - 规避：前端按需匹配当前项目会话，后续可改成后端聚合接口分页。

3. 风险：项目页聊天与主聊天页状态互相影响。
   - 说明：这是有意共享同一会话上下文，避免双份消息状态；但 UI 不跳页。

4. 风险：联系人较多时前端推导开销增大。
   - 规避：后续引入 `GET /api/projects/:id/contacts` 聚合接口。

5. 风险：目录上下文沿用 `project_root` 字段造成语义混淆。
   - 规避：前端类型显式引入 `workspaceRoot`，发送层统一映射并在注释中标明“兼容字段”。

---

## 7. 分阶段实施

### 阶段 A（建议先做，1 次迭代可交付）

1. `ProjectExplorer` 新增 tab + `TeamMembersPane`。
2. store 增加 `keepActivePanel` 参数。
3. 团队成员页基于 `contacts` 展示联系人，按 `contact + project` 复用/创建会话。
4. 主聊天页改为“工作目录模式”（去掉项目文件/项目下拉，新增目录选择）。
5. 可聊天且不跳主聊天页；目录切换不新建会话。

### 阶段 B（优化）

1. 增加 `projects/:id/contacts` 后端聚合接口。
2. 团队成员页改为直接接口拉取。
3. 增加联系人搜索、排序（最近消息时间）。

---

## 8. 验收标准

1. 在项目页红框位置可见 `项目目录 / 团队成员` 两个 tab。
2. `项目目录` tab 下原目录树与文件预览行为完全不变。
3. `团队成员` tab 下可见已添加联系人列表。
4. 点击联系人后可直接聊天，且页面不跳转到主聊天页。
5. 发送消息后，消息进入该联系人+该项目对应会话。
6. 现有左侧联系人点击行为保持原样（默认仍切回 `chat` 面板）。
7. 主聊天页底部不再显示 `项目文件` 和 `项目下拉`。
8. 主聊天页可选择本地“工作目录”。
9. 选择工作目录后可启用 MCP 目录探索能力。
10. 切换工作目录不会新建会话、不会改变项目绑定。

---

## 9. 建议文件落点（实施时）

- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/ProjectExplorer.tsx`
- 新增：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/projectExplorer/WorkspaceTabs.tsx`
- 新增：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/projectExplorer/TeamMembersPane.tsx`
- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/ChatInterface.tsx`
- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/InputArea.tsx`
- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/types.ts`
- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/types/index.ts`
- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sessions.ts`
- 修改：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/sendMessage.ts`
- 可选新增：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/projectExplorer/teamHelpers.ts`
