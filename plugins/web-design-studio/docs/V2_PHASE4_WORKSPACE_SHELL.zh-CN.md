# Web Design Studio v2 阶段 4：新工作区 Shell

阶段 4 把编辑器从“顶部工具条 + 左组件库 + 中画布 + 右属性栏”的固定三栏结构，拆成可以独立导航、滚动、折叠和调整宽度的工作区。

## 第一批已实现

- 新 Navigation Bar：页面与图层、组件与资产、视觉工具、我的组件、Variables、AI 任务拥有长期稳定入口；
- Dynamic Left Sidebar：导航入口会切换图层、官方组件库、视觉原语、我的组件、Variables 和 AI 任务，不再要求用户在同一排十个 Tab 中寻找所有能力；
- Bottom Toolbar：选择、Frame、Section、形状、文本、批注和 AI Actions 形成画布级工具入口；
- Panel State 独立模型：当前区域、当前工具、左右面板开关、面板宽度和画布最大化不再散落在主 App 状态中；
- 左右面板支持拖动调整宽度，并分别独立滚动；
- 编辑区域支持一键最大化，再次切换或恢复面板不会丢失此前宽度；
- Command/Ctrl + `\\` 切换画布最大化，V/F/T/C 切换常用工具；
- Shell reducer 有独立测试，不依赖组件库 iframe 或文档渲染。
- Workspace Shell 会保存到本地，重开工作台后恢复上次区域、工具、面板宽度和展开状态；非法旧值会被丢弃或收敛到安全尺寸；
- 官方组件运行时增加独立 memo boundary：拖动画布中其他节点时，不再无条件重渲染所有第三方运行时；组件、Token、交互模式或 Slot 内容真正变化时仍会正确刷新；
- 顶部 AI 入口和底部 AI Actions 现在直接打开 AI Sidebar，不再把主要 AI 工作流放进画布中央浮层。
- 普通页面画布已经进入独立无限工作区：画布在工作区中居中，页面尺寸增长时保持当前视觉位置，支持空格 + 左键和中键平移；全屏预览与 Slot 内部编辑保持独立坐标系；
- 组件款式和整站风格选择已从中央阻断式 Modal 改为右侧非阻断 Surface，用户可以一边查看画布一边选择或拖入明确的单个官方元素；
- 右属性栏开始按节点能力动态收敛：文字、媒体、容器和官方组件只显示有意义的内容、排版、媒体、Slot、Property Contract 与布局控制，不再给所有节点展示同一套字段。
- 右属性栏进一步拆成“设计 / 原型 / 批注与 AI”三种上下文，避免把样式、跳转和 AI 工作流堆成长表单；批注工具会直接切到目标组件的批注与 AI 上下文。
- 画布节点及其官方运行时内容已从主 App 抽到独立 `CanvasComponent` 渲染边界，后续 Scene Graph v2 节点接入不再继续扩张工作区 Shell。

## 当前边界

- 第一批先建立 Shell 与状态边界，底部 Frame、Section、文本工具目前负责切换到相应设计入口；直接在画布创建和绘制属于阶段 5；
- Variables 与 AI 已从图层内容中拆为各自的 Sidebar；后续会把 Schema v2 Variable Collections、Modes 和批注任务直接接入这些入口；
- 当前文档编辑主体仍在旧 `WebDesignStudioApp` 中，后续会继续拆出 Canvas、Left Sidebar 和 Inspector 的 memoized boundary；官方组件运行时边界已经独立，Shell 级状态不会触发无意义的运行时刷新。

## 第一批验收

- TypeScript 类型检查通过；
- UI 生产构建通过；
- Workspace Shell reducer 测试通过；
- 全仓 172/172 tests passed。
