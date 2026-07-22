# ChatOS 3D 房间式聊天主页改造实施方案

## 1. 结论

这个设想可以在现有项目中实现，而且不需要重写聊天、项目管理或任务系统的后端。

推荐采用“WebGL 3D 房间 + React 2D 功能面板”的混合方案：

- 用 Three.js / React Three Fiber 渲染写实房间、书桌、窗景、电脑、手机、实体档案架、右侧任务墙和镜头动画。
- 用户坐在书桌前，通过有限角度转头或点击热点在电脑、左侧档案架、右侧任务画面之间切换。
- 聊天输入、消息列表、Markdown、项目文件浏览器、任务详情等高密度界面继续使用现有 React DOM，不直接重做成 3D 文字和 3D 控件。
- 3D 电脑屏幕负责呈现预览和入口；进入电脑工作态后，镜头真实靠近实体屏幕，聊天组件绑定在屏幕表面，不使用脱离房间的全屏网页弹层伪装。
- 保留经典 2D 模式，作为低性能设备、移动端、无 WebGL 环境、辅助功能和故障恢复入口。

首版应以 WebGL 为生产渲染后端。WebGPU 可以作为后续实验能力，但不能作为上线的硬依赖，因为截至 2026 年 7 月仍未在所有广泛使用的浏览器中达到一致支持。

## 2. 当前代码基础

当前主应用具备实施这一方案所需的大部分业务能力：

- `chatos/frontend/src/App.tsx` 负责认证、Realtime、对话 Store 和应用壳层。
- `chatos/frontend/src/components/ChatInterface.tsx` 是当前聊天主页入口。
- `chatos/frontend/src/components/chatInterface/ChatInterfaceMainContent.tsx` 在聊天、项目、终端、远程终端、SFTP 等工作面板之间切换。
- `chatos/frontend/src/components/chatInterface/ChatConversationPane.tsx` 已经把消息列表、输入区、用户消息侧栏、任务抽屉和摘要面板拆开。
- `chatos/frontend/src/components/ProjectExplorer.tsx` 已经具备项目详情、文件树、运行环境、成员和相关项目操作能力。
- `chatos/frontend/src/lib/api/client/tasks.ts` 已具备当前会话任务的查询、更新、完成和删除接口。
- `chatos/frontend/src/lib/realtime/useConversationTaskBoardRealtime.ts` 已能实时接收当前会话任务看板更新。
- `chatos/frontend/src/lib/realtime/useProjectsRealtime.ts` 和项目运行相关 Realtime Hook 可用于实时刷新档案架与投影状态。
- 前端已经使用 React 19、Vite、TypeScript、Zustand、Framer Motion、Tailwind CSS 和 WebSocket Realtime。

当前尚未引入 Three.js、React Three Fiber、Drei 或其他 3D 引擎，因此 3D 能力可以作为一个隔离的新功能模块加入，不必侵入现有数据层。

## 3. 产品形态

### 3.1 默认视角

用户进入主页后处于坐在书桌前的第一人称固定座位视角：

- 正前方：窗户、自然风景、电脑显示器、键盘和鼠标。
- 桌面一侧：放在支架上的手机。
- 左侧：项目档案架，每个项目是一份有名称、状态和更新时间的档案夹。
- 右侧：直接呈现在墙面上的大尺寸实时任务画面，展示当前会话正在执行、阻塞和刚完成的任务；不要求建模实体投影仪、幕布或支架。

不建议首版做自由行走。固定座位加三个镜头锚点更容易使用，也能减少眩晕、误操作和性能成本。

### 3.2 三个主要工作区

1. 电脑工作区
   - 点击显示器、按 Enter，或点击底部“电脑”导航后，镜头平滑移动到显示器正前方。
   - 显示器空闲时显示当前联系人、最后一条消息、AI 状态和未读状态预览。
   - 进入工作态后加载现有 `ChatConversationPane`，保留消息滚动、Markdown、附件、模型选择、推理模式、计划模式、停止生成、输入法和复制选择能力。
   - Esc、返回按钮或镜头导航返回房间总览。

2. 左侧项目档案架
   - 档案夹由现有 `projects` 数据生成，标签显示项目名称，颜色或角标显示本地、云端、导入中、运行中等状态。
   - 点击档案夹后播放“抽出档案并打开”的短动画。
   - 打开后的档案页面承载项目概览；继续进入详情时复用现有 `ProjectExplorer`。
   - 项目很多时不在 3D 中一次性生成全部实体，只展示最近或置顶项目，其余项目通过抽屉式列表搜索。

3. 右侧实时任务墙
   - 默认显示当前会话任务数量、进行中、阻塞、待办和最近完成状态。
   - 数据来自 `getTaskManagerTasks(currentSession.id)`，并通过 `useConversationTaskBoardRealtime` 增量刷新。
   - 点击任务后打开清晰的 2D 任务详情；对于异步 Task Runner 任务，可继续复用现有 `MessageTaskDrawer`、任务 DAG、运行详情和文件变更视图。
   - 任务画面作为墙面空间的一部分呈现；聚焦后镜头推进到近乎铺满视野，内容仍采用绑定在 3D 平面上的清晰 DOM UI。

### 3.3 手机

首版将手机作为场景资产和状态入口，不承担新的核心业务：

- 屏幕可显示当前时间、AI 在线状态、任务数量和一条最近通知。
- 点击后弹出轻量快捷面板，例如切换会话、切换日夜、打开设置。
- 语音输入、通知中心或远程控制放到后续版本，避免首版范围失控。

### 3.4 日夜与窗景

- 默认根据用户本地时间自动判断白天、黄昏和夜晚。
- 用户可以在设置中固定白天、固定夜晚或继续自动模式。
- 白天使用暖色太阳方向光、天空环境光和明亮风景。
- 夜晚使用较弱的冷色月光、室内桌灯、屏幕自发光和夜景。
- 黄昏作为两个环境之间的平滑过渡，不需要实时物理大气模拟。
- 窗外优先使用 HDR 环境图、远景球面或多层视差风景，而不是构建完整可行走的室外世界。

## 4. 技术选型

### 4.1 推荐方案

新增依赖建议：

```text
three
@types/three
@react-three/fiber@9
@react-three/drei
@react-three/postprocessing（可选，首版克制使用）
```

选择原因：

- React Three Fiber 是 Three.js 的 React Renderer，可以直接沿用当前 React 19 组件体系和 Zustand 状态管理。
- 官方文档明确说明 `@react-three/fiber@9` 与 React 19 配套。
- Drei 提供 `CameraControls`、`Html`、`useGLTF`、`PerformanceMonitor`、`DetectGPU` 等成熟能力。
- 当前业务 UI 已经高度 React 化，用 R3F 可以在一个组件树中组合 3D 场景和现有 React 面板。
- Three.js 的 `GLTFLoader` 支持 glTF 2.0、Draco、Meshopt 和 KTX2/Basis 压缩，适合房间资产发布。

### 4.2 为什么不用纯 3D UI

不建议把完整聊天窗口、项目文件树和任务详情全部绘制成 Canvas 纹理或 3D 字体：

- 中文输入法、文本选择、复制粘贴、代码块、Markdown、链接、弹窗和滚动处理成本高。
- 复杂 DOM 直接用 Drei `Html transform` 贴在倾斜屏幕上，部分设备可能出现文字模糊。
- 当前界面存在弹窗、Portal、Tooltip、Drawer 和大规模消息列表，全部嵌入 3D 变换层容易出现定位和裁剪问题。
- 无障碍、键盘导航和自动化测试会明显变差。

因此采用两级表现：

- 房间总览态：3D 屏幕或精简 HTML 预览。
- 工作聚焦态：镜头对准目标后，用正常分辨率的 React DOM 覆盖目标屏幕区域。

### 4.3 备选方案比较

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| Three.js + React Three Fiber | 与现有 React、Zustand 和组件体系最匹配，渐进接入容易 | 需要自行组织场景和资产流程 | 推荐 |
| Babylon.js | 完整引擎、编辑器和高级渲染能力强 | 会形成另一套偏游戏引擎式架构，复用当前 React UI 的成本更高 | 可做独立 3D 产品时考虑 |
| PlayCanvas | 在线编辑器和协作式场景制作方便 | 与当前代码和构建体系结合不如 R3F 直接 | 不作为首选 |
| Unity WebGL | 美术和游戏开发工具链成熟 | 包体大、启动慢、DOM UI 集成复杂、移动端成本高 | 不适合当前主页改造 |
| CSS 3D | 实现简单、DOM 清晰 | 光照、模型、遮挡和真实空间感有限 | 只适合原型或降级模式 |

## 5. 前端架构设计

建议保留现有 `ChatInterface` 作为业务控制层，在其下增加经典与空间两种视图：

```text
ChatInterface
├── Header / 全局 Overlay / Dialog
├── ClassicWorkspace
│   └── 现有 ChatInterfaceMainContent
└── SpatialWorkspace
    ├── SpatialCanvas
    │   ├── RoomShell
    │   ├── WindowEnvironment
    │   ├── DeskArea
    │   ├── ComputerHotspot
    │   ├── PhoneHotspot
    │   ├── ProjectArchiveShelf
    │   ├── TaskProjectionScreen
    │   └── SpatialCameraRig
    └── SpatialOverlayLayer
        ├── ComputerChatOverlay
        ├── ProjectDossierOverlay
        ├── TaskProjectionOverlay
        ├── SpatialHud
        └── ClassicModeFallback
```

### 5.1 建议新增目录

```text
chatos/frontend/src/features/spatialWorkspace/
├── SpatialWorkspace.tsx
├── SpatialCanvas.tsx
├── SpatialOverlayLayer.tsx
├── spatialWorkspaceTypes.ts
├── spatialWorkspaceStore.ts
├── spatialWorkspaceSelectors.ts
├── camera/
│   ├── SpatialCameraRig.tsx
│   ├── cameraAnchors.ts
│   └── useSpatialNavigation.ts
├── scene/
│   ├── RoomScene.tsx
│   ├── WindowEnvironment.tsx
│   ├── DeskArea.tsx
│   ├── ComputerScreen.tsx
│   ├── PhoneOnStand.tsx
│   ├── ProjectArchiveShelf.tsx
│   └── TaskProjectionScreen.tsx
├── overlays/
│   ├── ComputerChatOverlay.tsx
│   ├── ProjectDossierOverlay.tsx
│   ├── TaskProjectionOverlay.tsx
│   └── SpatialHud.tsx
├── hooks/
│   ├── useSpatialProjects.ts
│   ├── useSpatialTaskBoard.ts
│   ├── useTimeOfDay.ts
│   └── useSpatialQuality.ts
└── tests/
```

静态资产建议放在：

```text
chatos/frontend/public/spatial/
├── models/
├── textures/
├── environments/
└── audio/
```

### 5.2 空间状态

新增独立的轻量 Zustand Store，建议只保存 UI 级状态：

```text
mode: room | computer | archive | project | taskWall | task
cameraAnchor: desk | computer | shelf | taskWall | phone
selectedProjectId
selectedTaskId
timeOfDay: auto | day | dusk | night
quality: auto | high | medium | low
classicMode
```

不要把每一帧的镜头位置、模型旋转或动画进度写入 Zustand。这些高频状态应保存在 Three.js 对象 Ref 中，避免触发 React 重渲染。

### 5.3 与现有 Store 的连接

- `ComputerChatOverlay` 直接接收当前 `conversationPaneProps`，复用 `ChatConversationPane`。
- `ProjectArchiveShelf` 读取现有 `projects`、`currentProject` 和项目选择 Action。
- `ProjectDossierOverlay` 在选择项目后复用 `ProjectExplorer`，不复制项目数据请求逻辑。
- `TaskWallSurface` 通过现有 ApiClient 查询 `TaskManagerTaskResponse[]`。
- `useConversationTaskBoardRealtime` 收到事件后局部更新或重新拉取任务列表。
- 当前项目运行状态可复用 `useProjectRunRealtime` 和 `getProjectRunState`，在档案夹或投影上显示运行标记。
- 全局 `ChatInterfaceOverlays` 继续位于 3D Canvas 外部，避免弹窗被 Canvas 或透视层裁切。

## 6. 镜头与交互设计

### 6.1 镜头锚点

预定义位置、观察目标和 FOV：

- `desk`：默认正对显示器与窗户。
- `computer`：靠近显示器，屏幕接近正视角。
- `shelf`：向左旋转并略微靠近档案架。
- `taskWall`：向右旋转并推进到墙面任务画面，使内容接近铺满视野。
- `phone`：向桌面侧下方看向手机。

使用 Drei `CameraControls` 完成平滑转场，并限制：

- 水平转向范围。
- 俯仰范围。
- 缩放距离。
- 禁止穿墙和自由飞行。

### 6.2 输入方式

- 鼠标拖拽或触控拖动：有限角度转头。
- A/D 或左右方向键：在三个主要区域间切换。
- Enter：进入当前高亮对象。
- Esc：退出聚焦态或返回房间。
- 点击对象：直接聚焦。
- 屏幕底部保留可见的 2D 快捷导航，保证第一次使用时不依赖用户猜测 3D 操作。

### 6.3 交互反馈

- 可交互对象悬停时只使用轻微轮廓、亮度变化和光标变化。
- 档案夹抽出、屏幕点亮、任务墙聚焦等动画控制在 250–600ms。
- 镜头大范围移动控制在 600–900ms，并支持 `prefers-reduced-motion` 下立即或快速切换。
- 避免持续漂浮、镜头呼吸和不必要的粒子效果，保证长时间聊天时界面稳定。

## 7. 3D 资产与视觉制作

### 7.1 美术方向

首版视觉改为“温暖、写实、适度克制”的房间：

- 以真实空间比例、PBR 材质、自然光照和可信的窗外摄影景观为目标。
- 在性能允许范围内提高桌面、墙面、档案纸张、猫和家具的材质细节。
- 木质书桌、柔和墙面、简洁显示器、少量绿植和远景山水即可形成氛围。

### 7.2 模型拆分

不要把整个房间做成一个不可拆分的大模型。建议拆成：

- 房间壳体与固定家具。
- 书桌和桌面物品。
- 电脑、键盘、鼠标。
- 手机和支架。
- 档案架、档案夹模板。
- 墙面任务显示区域；实体投影仪不是必需资产。
- 窗框与远景载体。

档案夹使用共享几何体和材质，项目名称由 DOM/SDF 文本或贴花生成，避免每个项目生成独立高面数模型。

### 7.3 发布格式

- 模型统一导出为 glTF 2.0 / GLB。
- 网格优先使用 Meshopt 或 Draco 压缩。
- 纹理优先使用 KTX2/Basis，必要时提供 WebP 回退。
- 对重复物体使用 Instancing。
- 对固定环境尽量烘焙 AO 和部分静态光照。
- 动态阴影只保留最关键的太阳/月光和少量近景物体。

### 7.4 建议性能预算

首个可用版本目标：

- 桌面高质量场景：不超过约 150k 可见三角形。
- 低质量场景：不超过约 60k 可见三角形。
- 稳态 Draw Call：尽量控制在 150 以内。
- 首次必须加载的 3D 资产：压缩后控制在 8 MB 左右，其他高清环境和细节延迟加载。
- 低性能/移动降级资产：控制在 3 MB 左右。
- 常见桌面设备目标 60 FPS，低性能设备目标稳定 30 FPS。
- 进入电脑聊天工作态后减少阴影、后处理和远景更新，把资源优先留给 DOM 滚动与消息渲染。

## 8. 性能策略

- Canvas 默认使用 `frameloop="demand"`，静止时不持续 60 FPS 渲染。
- 镜头动画、日夜过渡和交互时通过 `invalidate()` 请求帧。
- 使用 `PerformanceMonitor` 动态调整 DPR、阴影、环境贴图分辨率和后处理。
- 使用 `DetectGPU` 对 Tier 0、部分移动设备或软件渲染环境直接进入轻量或经典模式。
- 3D 模块使用 `React.lazy` 动态加载，认证页和经典聊天页不提前下载 Three.js 包与房间资产。
- 使用嵌套 Suspense：先显示静态背景或低质量房间，再加载完整 GLB 和环境图。
- 复用 geometry、material 和 loader cache；项目档案夹使用 InstancedMesh。
- 首版不启用实时体积光、实时全局光照、高采样景深或持续屏幕空间反射。
- 监听 `webglcontextlost`，发生上下文丢失时自动切回经典模式，并提供重试按钮。

## 9. 兼容性、无障碍与降级

必须保留“经典界面”开关，且不把 3D 作为访问核心功能的唯一通道。

自动降级条件：

- WebGL 初始化失败。
- GPU Tier 过低。
- 用户开启减少动态效果。
- Canvas 上下文丢失且重建失败。
- 移动端屏幕过小或内存压力明显。

无障碍要求：

- 所有 3D 热点必须有对应的 DOM 按钮和可读名称。
- 支持键盘完成电脑、项目、任务三区切换。
- 屏幕阅读器默认可以使用经典 DOM 导航，不需要理解 3D 场景。
- 不能只用颜色表达任务状态。
- 日夜模式不能降低文字对比度。
- 音效默认关闭或非常克制，并提供独立关闭选项。

## 10. 分阶段实施

### 阶段 0：技术原型

目标：证明 3D 壳层、镜头和现有聊天 DOM 能稳定共存。

工作内容：

- 安装并锁定 Three.js、R3F v9、Drei。
- 用基础几何体搭建书桌、显示器、左架、右幕布。
- 实现三个镜头锚点和点击切换。
- 把 `ChatConversationPane` 放入聚焦态 2D Overlay。
- 验证中文输入法、Markdown、滚动、弹窗和停止生成。
- 验证 WebGL 失败时自动返回经典模式。

验收标准：

- 不修改聊天后端即可正常对话。
- 经典模式和 3D 模式可随时切换。
- 镜头移动期间不卡住聊天流和 WebSocket 更新。

### 阶段 1：房间框架与日夜

- 接入正式低模房间、桌面设备和窗景。
- 实现自动日夜、手动切换和灯光过渡。
- 加入加载进度、错误边界和质量设置。
- 完成鼠标、触控和键盘空间导航。

验收标准：

- 主流桌面浏览器可以稳定进入房间。
- 静止状态 GPU 占用明显下降。
- 低质量档和经典模式可正常工作。

### 阶段 2：电脑聊天工作区

- 完成显示器聚焦、聊天预览和聊天 Overlay。
- 调整 `ChatConversationPane` 在空间模式下的布局变体。
- 保证 Header、Drawer、Dialog、Tooltip 和附件选择不被 Canvas 遮挡。
- 恢复焦点、快捷键和离开工作态时的滚动位置。

验收标准：

- 现有聊天核心能力没有功能回归。
- 中文输入、代码复制、长消息滚动和任务抽屉正常。

### 阶段 3：项目档案架

- 将项目数据映射为档案夹。
- 实现最近项目、置顶项目、状态标记和搜索抽屉。
- 实现抽档案动画与项目概览。
- 从档案详情进入现有 `ProjectExplorer`。
- 通过项目 Realtime 更新档案状态。

验收标准：

- 项目新增、归档、更新后档案架能同步变化。
- 大量项目不会导致一次性生成大量 3D 节点。

### 阶段 4：实时任务墙

- 新增 `useSpatialTaskBoard`，查询当前会话任务。
- 接入 `useConversationTaskBoardRealtime`。
- 任务墙显示 doing、todo、blocked、done 概览和当前任务。
- 点击后进入任务详情；异步任务继续衔接现有 DAG 和运行详情。

验收标准：

- AI 新建、更新、完成任务时，任务墙无需整页刷新即可更新。
- 阻塞任务和当前执行任务有明确但不过度闪烁的状态提示。

### 阶段 5：美术、声音和性能收尾

- 替换最终模型、材质、HDRI 和环境音。
- 完成 GLB、Meshopt/Draco、KTX2 压缩流水线。
- 建立高、中、低质量档。
- 完成浏览器兼容、内存、长时间运行和上下文丢失测试。
- 灰度开放 3D 模式，收集启用率、失败率、平均 FPS 和回退率。

## 11. 预计工作量

以一名前端开发为主、3D 美术阶段性参与估算：

| 阶段 | 预计时间 |
| --- | --- |
| 技术原型 | 3–5 个工作日 |
| 房间框架与日夜 | 5–8 个工作日 |
| 电脑聊天工作区 | 5–7 个工作日 |
| 项目档案架 | 4–6 个工作日 |
| 实时任务墙 | 4–6 个工作日 |
| 美术、压缩、兼容与性能 | 6–10 个工作日 |

使用占位模型时，约 3–5 周可以得到功能完整的 MVP；包含定制美术、完整优化和多浏览器验收时，建议按 5–8 周安排。

## 12. 测试计划

### 单元测试

- 空间状态机和镜头锚点切换。
- 日夜计算和用户覆盖设置。
- 项目到档案夹模型的映射。
- 任务状态分组和 Realtime 增量更新。
- GPU/能力检测后的降级决策。

### 组件测试

- 使用 R3F Test Renderer 测试热点和场景组件。
- 使用现有 Vitest + Testing Library 测试 Overlay 与现有聊天组件的连接。
- 测试退出空间模式后原有 Store 状态不丢失。

### 集成测试

- 在 3D 模式下发送、停止、继续聊天。
- 新建和选择项目后档案架同步。
- AI 更新任务后投影实时刷新。
- 打开任务详情、项目详情和全局设置弹窗。
- WebGL 初始化失败和 context lost 回退。

### 浏览器矩阵

- Chrome / Edge 最新稳定版。
- Safari 最新稳定版。
- Firefox 最新稳定版。
- macOS、Windows 常见集显设备。
- iOS / Android 使用轻量或经典模式验证。

## 13. 主要风险与处理方式

| 风险 | 处理方式 |
| --- | --- |
| 复杂聊天 DOM 在 3D 变换后模糊 | 聚焦态使用正常 DOM Overlay，不长期使用 transform HTML |
| 3D 占用 GPU，长时间聊天发热 | 按需渲染、动态 DPR、聚焦聊天时关闭高成本效果 |
| 中文输入法、弹窗和 Portal 异常 | 功能 UI 留在 Canvas 外层，3D 只做导航和氛围 |
| 场景包体过大 | GLB 拆分、延迟加载、Meshopt/Draco、KTX2、低质量资产 |
| 项目数量很多导致节点爆炸 | 只生成最近/置顶项目，其他项目走搜索列表 |
| 镜头导致眩晕 | 固定座位、有限转向、短转场、减少动态效果支持 |
| WebGPU 兼容性不一致 | 生产默认 WebGL，WebGPU 只做可选实验 |
| 3D 功能影响现有稳定主页 | Feature Flag、经典模式、独立模块、灰度发布 |

## 14. MVP 不需要的后端改造

首个版本原则上不需要新增后端接口：

- 聊天继续使用现有消息、流式响应和 Realtime。
- 项目继续使用现有 Projects、Project Run 和 Project Explorer 接口。
- 任务继续使用现有 Task Manager 接口和 `conversation.task_board.updated` 事件。

后续可选后端能力：

- 跨设备保存空间模式、画质、日夜和常用镜头设置。
- 项目置顶和档案架排序。
- 房间主题包、窗景主题和用户自定义陈设。
- 3D 性能与降级匿名统计。

## 15. 实施前需要确定的产品选项

建议默认采用以下决定推进原型：

- 视觉风格：温暖、写实、具有真实比例和自然光照的书房。
- 导航方式：固定座位 + 三个主镜头锚点，不自由行走。
- 电脑交互：镜头推进到实体显示器，清晰聊天 UI 绑定在 3D 屏幕表面。
- 手机：首版只做状态和快捷入口。
- 窗景：山水或湖景，自动白天/黄昏/夜晚。
- 发布策略：默认保留经典模式，3D 模式先灰度开放。

## 16. 技术调研依据

- React Three Fiber 介绍及 React 19 版本配套：<https://r3f.docs.pmnd.rs/getting-started/introduction>
- R3F 性能策略、按需渲染、缓存、Instancing、LOD 和 PerformanceMonitor：<https://r3f.docs.pmnd.rs/advanced/scaling-performance>
- Drei `Html` 绑定 HTML 与 3D 对象、遮挡和 transform 模式说明：<https://drei.docs.pmnd.rs/misc/html>
- Drei `CameraControls` 镜头与输入控制：<https://drei.docs.pmnd.rs/controls/camera-controls>
- Drei GPU 分级与低性能回退：<https://drei.docs.pmnd.rs/misc/detect-gpu-use-detect-gpu>
- Three.js `GLTFLoader`、glTF 2.0、Draco、Meshopt 和 KTX2 支持：<https://threejs.org/docs/#GLTFLoader>
- MDN WebGL 平台说明：<https://developer.mozilla.org/en-US/docs/Web/API/WebGL_API>
- MDN WebGPU 兼容性说明：<https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API>
- Babylon.js 作为完整 Web 3D 引擎的备选参考：<https://www.babylonjs.com/>
