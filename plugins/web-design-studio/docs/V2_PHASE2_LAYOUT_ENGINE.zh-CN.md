# Web Design Studio v2 阶段 2：布局与连续响应式

## 原则

- 布局引擎只读取 Scene Document，输出独立的 solved boxes，不偷偷改写设计数据；
- AI 表达 Hug、Fill、Fixed、Auto Layout 等设计意图，不手工计算整页坐标；
- viewport width 是求解输入，不为 Desktop、Tablet、Mobile 复制三份节点；
- 内容、变量或组件 Slot 改变后，从叶子到祖先重新求解尺寸；
- 绝对定位节点保留明确坐标，但不参与父级流式尺寸计算。

## 第一批已实现

- 横向和纵向 Auto Layout；
- 四边 Padding、横纵 Gap、对齐和空间分布；
- Hug、Fill、Fixed；
- min/max width/height；
- 横向和纵向 Wrap；
- 流内节点与 absolute 节点分离；
- 确定性的文本换行估算，内容变化会推动 Hug 祖先增长；
- 320 至 7680 CSS px 的同一 Scene 连续求解；
- overflow 与 Fill-in-Hug 诊断，不用静默裁切掩盖问题。

## Grid 已实现

- `px`、`%`、`fr`、`auto` tracks；
- `minmax()` 与固定次数 `repeat()`；
- `repeat(auto-fit, ...)` 和 `repeat(auto-fill, ...)` 随连续视口自动增减列；
- 显式 row/column、rowSpan/columnSpan；
- row、column、dense 自动放置；
- 自动内容行与空行扩展；
- 显式碰撞和越界诊断。

## Constraints 已实现

- Free Layout 和 Auto/Grid 中的 absolute child 使用同一套约束求解；
- 横向支持 left、center、right、stretch、scale；
- 纵向支持 top、center、bottom、stretch、scale；
- 约束依据设计时父 Frame 尺寸和当前求解尺寸计算，不保存多份设备坐标；
- min/max 继续约束 stretch 与 scale 的最终结果。

## 响应式规则已实现

- minWidth 包含、maxWidth 排除的连续宽度区间；
- 多条匹配规则按文档顺序叠加，适合“基础平板规则 + 更窄手机规则”；
- 断点只覆盖节点 layout、visible、child order 和 Variable Collection Mode；
- 求解前生成临时有效 Scene 并解析全部 Variable Binding，源文档和人工布局不被改写；
- 隐藏节点不占据 Auto Layout/Grid 流，恢复到其他宽度时仍是同一个稳定节点。

## 媒体尺寸已实现

- Image/Video 保存资源固有宽高，而不是依赖当前画布截图尺寸；
- Fill width + Hug height 会按固有比例重新计算高度；
- 可显式关闭 preserveAspectRatio，用于需要拉伸的设计效果；
- 无效或缺失的图片/视频固有尺寸在保存前拒绝。

## 真实字体测量接口已实现

- 无浏览器环境继续使用确定性的字体、字号、字距、行高和换行估算；
- Canvas/DOM 工作区可在字体加载后注入同步 textMeasurer；
- 测量请求携带节点 ID、正文、可用宽度和完整 Typography；
- 非有限值或负尺寸直接失败，避免错误测量静默污染整页布局。

## 浏览器校准协议已实现

- 工作区按稳定 nodeId 回传实际 rect、scrollWidth 和 scrollHeight；
- 逐节点比较 solved box 与浏览器几何，支持可配置像素容差；
- 缺失节点、意外节点、几何偏差和真实 scroll overflow 分开报告；
- 只有 warning 时可以通过，任何缺失、偏移或溢出 error 都会阻止视觉验收。

## 正式验收集已完成

- 不是同一个模板换文案，而是 12 种独立结构：SaaS 分屏 Hero、电商目录、杂志非对称封面、创意机构马赛克、作品索引、大会议程、餐旅画廊、医疗服务路径、课程时间线、公益影响叙事、地产户型与开发者三栏文档；
- 每种结构使用同一节点树连续求解 320、390、768、1024、1280、1440、1920、2560、3840、7680 CSS px；
- 数学验收覆盖横向溢出、Grid error、流内兄弟重叠、文字截断、根 Frame 视口宽度和 4K/8K 内容居中；
- 真实 Chrome 完成 120/120 张截图和 5053 个节点校准；
- 最终最大几何误差 0.0155px，节点 scroll overflow、文档横向溢出、4K/8K 宽屏约束失败均为 0；
- 浏览器回归修正了嵌套 Hug、嵌套 Grid、CJK/Emoji、货币数字、Unicode 长横线、长英文断行与 HTML style 属性转义。

## 接下来

1. 进入阶段 3：画布交互、选择模型、变换手柄、多选/分组、容器内编辑和人工锁定字段的完整编辑闭环；
2. 将浏览器真实字体测量接入工作区布局循环，而不是只在截图验收阶段校准；
3. 为媒体 crop/fit、富文本混排和真实第三方组件运行时增加同级验收集。
