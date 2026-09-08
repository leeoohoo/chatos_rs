# Web Design Studio v2 阶段 2 验收

阶段 2 的验收对象是布局引擎和连续响应式，不以“页面能打开”或少量示例截图代替完成标准。

## 验收范围

- 12 类网站，12 种不同结构；
- 10 个连续视口：320 至 7680 CSS px；
- 120 张真实 Chrome 截图；
- 每个 Scene DOM 节点回传 rect、scrollWidth、scrollHeight 和求解器预期几何；
- 检查节点内部溢出、页面横向溢出、流内重叠、Grid error、文本截断和宽屏内容约束。

## 最终结果

- 截图成功：120/120；
- 逐节点比较：5053；
- 最大几何误差：0.0155px；
- 节点渲染溢出：0；
- 页面横向溢出：0；
- 4K/8K 全宽背景与 1440px 居中内容失败：0；
- 独立结构：12/12；
- 每个视口的 12 张截图均不相同。

## 可重复命令

```bash
npm run phase2:run
npm run phase2:preview
npm run phase2:capture
npm run phase2:report
```

截图和机器生成报告保存在被 Git 忽略的 `.web-design-studio-baselines/`，代码仓库只保存验收定义、脚本和结论文档。
