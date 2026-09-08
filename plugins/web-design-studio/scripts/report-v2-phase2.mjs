import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function round(value, places = 4) {
  const factor = 10 ** places;
  return Math.round(value * factor) / factor;
}

const capturePath = path.resolve(argument('--capture') ?? '.web-design-studio-baselines/v3.0.1-phase2-layout/capture-manifest.json');
const runPath = path.resolve(argument('--run') ?? '.web-design-studio-baselines/phase2-current-run.json');
const outputDirectory = path.resolve(argument('--output') ?? path.dirname(capturePath));
const tolerance = Number(argument('--tolerance') ?? 1);
if (!Number.isFinite(tolerance) || tolerance < 0 || tolerance > 10) throw new Error('--tolerance must be between 0 and 10 pixels.');
const capture = JSON.parse(await readFile(capturePath, 'utf8'));
const run = JSON.parse(await readFile(runPath, 'utf8'));
const successful = capture.results.filter((result) => result.status === 'captured');
const failed = capture.results.filter((result) => result.status !== 'captured');
const geometryFailures = [];
const overflowFailures = [];
const documentOverflow = [];
const wideFailures = [];
let comparedNodeCount = 0;
let maximumGeometryDelta = 0;

for (const result of successful) {
  const nodes = result.pageState?.sceneNodes ?? [];
  for (const node of nodes) {
    comparedNodeCount += 1;
    const delta = Math.max(
      Math.abs(node.rect.x - node.expected.x),
      Math.abs(node.rect.y - node.expected.y),
      Math.abs(node.rect.width - node.expected.width),
      Math.abs(node.rect.height - node.expected.height)
    );
    maximumGeometryDelta = Math.max(maximumGeometryDelta, delta);
    if (delta > tolerance) geometryFailures.push({ benchmarkId: result.benchmarkId, viewportId: result.viewportId, nodeId: node.nodeId, maximumDelta: delta });
    const overflowX = Math.max(0, node.scrollWidth - node.rect.width);
    const overflowY = Math.max(0, node.scrollHeight - node.rect.height);
    if (overflowX > tolerance || overflowY > tolerance) overflowFailures.push({ benchmarkId: result.benchmarkId, viewportId: result.viewportId, nodeId: node.nodeId, overflowX, overflowY });
  }
  if (result.pageState?.document?.scrollWidth > result.width + tolerance) documentOverflow.push({
    benchmarkId: result.benchmarkId,
    viewportId: result.viewportId,
    viewportWidth: result.width,
    scrollWidth: result.pageState.document.scrollWidth
  });
  if (result.width >= 3840) {
    const root = nodes.find((node) => node.nodeId === `${result.benchmarkId}-root`);
    const content = nodes.find((node) => node.nodeId === `${result.benchmarkId}-content`);
    if (!root || !content || Math.abs(root.rect.width - result.width) > tolerance || Math.abs(content.rect.width - 1440) > tolerance
      || Math.abs(content.rect.x - (result.width - 1440) / 2) > tolerance) {
      wideFailures.push({ benchmarkId: result.benchmarkId, viewportId: result.viewportId, root: root?.rect, content: content?.rect });
    }
  }
}

const byViewport = new Map();
for (const result of successful) {
  const hashes = byViewport.get(result.viewportId) ?? [];
  hashes.push(result.sha256);
  byViewport.set(result.viewportId, hashes);
}
const visualVariation = [...byViewport].map(([viewportId, hashes]) => ({
  viewportId,
  screenshotCount: hashes.length,
  uniqueScreenshotCount: new Set(hashes).size
}));
const distinctStructureCount = new Set(run.designs.map((design) => design.structure?.signature)).size;
const findings = [];
if (failed.length > 0) findings.push(`${failed.length} captures failed.`);
if (geometryFailures.length > 0) findings.push(`${geometryFailures.length} nodes exceeded ${tolerance}px geometry tolerance.`);
if (overflowFailures.length > 0) findings.push(`${overflowFailures.length} nodes have rendered scroll overflow.`);
if (documentOverflow.length > 0) findings.push(`${documentOverflow.length} pages have horizontal document overflow.`);
if (wideFailures.length > 0) findings.push(`${wideFailures.length} 4K/8K pages violate full-bleed background or centered 1440px content constraints.`);
if (distinctStructureCount !== 12) findings.push(`Only ${distinctStructureCount}/12 distinct scene structures were found.`);
if (visualVariation.some((entry) => entry.uniqueScreenshotCount !== entry.screenshotCount)) findings.push('At least one viewport contains pixel-identical screenshots across different briefs.');

const report = {
  schemaVersion: 1,
  runId: capture.runId,
  generatedAt: new Date().toISOString(),
  tolerance,
  expectedCaptureCount: 120,
  successfulCaptureCount: successful.length,
  failedCaptureCount: failed.length,
  comparedNodeCount,
  maximumGeometryDelta: round(maximumGeometryDelta, 6),
  geometryFailureCount: geometryFailures.length,
  renderedOverflowCount: overflowFailures.length,
  documentHorizontalOverflowCount: documentOverflow.length,
  wideViewportFailureCount: wideFailures.length,
  distinctStructureCount,
  visualVariation,
  findings,
  failures: { captures: failed, geometry: geometryFailures, renderedOverflow: overflowFailures, documentOverflow, wide: wideFailures }
};

const markdown = `# Web Design Studio v2 阶段 2 验收报告

生成时间：${report.generatedAt}

## 结论

- 截图：${report.successfulCaptureCount}/${report.expectedCaptureCount}；
- 独立网站结构：${report.distinctStructureCount}/12；
- 浏览器逐节点比较：${report.comparedNodeCount}；
- 最大几何误差：${report.maximumGeometryDelta}px（容差 ${report.tolerance}px）；
- 节点渲染溢出：${report.renderedOverflowCount}；
- 页面横向溢出：${report.documentHorizontalOverflowCount}；
- 4K/8K 全宽背景与 1440px 居中内容失败：${report.wideViewportFailureCount}。

## 视口视觉唯一性

| 视口 | 截图 | 唯一截图 |
| --- | ---: | ---: |
${report.visualVariation.map((entry) => `| ${entry.viewportId} | ${entry.screenshotCount} | ${entry.uniqueScreenshotCount} |`).join('\n')}

## 自动发现

${report.findings.map((finding) => `- ${finding}`).join('\n') || '- 没有自动化失败项。'}
`;

await mkdir(outputDirectory, { recursive: true });
const jsonPath = path.join(outputDirectory, 'phase2-acceptance-report.json');
const markdownPath = path.join(outputDirectory, 'phase2-acceptance-report.md');
await writeFile(jsonPath, `${JSON.stringify(report, null, 2)}\n`);
await writeFile(markdownPath, `${markdown}\n`);
process.stdout.write(`${markdown}\nreport ${jsonPath}\nreport ${markdownPath}\n`);
if (report.findings.length > 0 || report.successfulCaptureCount !== report.expectedCaptureCount) process.exitCode = 1;
