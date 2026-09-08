import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { V2_WEBSITE_BENCHMARKS } from '../dist/v2-phase0-baseline.test.mjs';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function ratio(value) {
  return Math.round(value * 1000) / 1000;
}

const runPath = path.resolve(argument('--run') ?? '.web-design-studio-baselines/legacy-current-run.json');
const capturePath = path.resolve(argument('--capture') ?? '.web-design-studio-baselines/v3.0.1-legacy-current/capture-manifest.json');
const outputDirectory = path.resolve(argument('--output') ?? path.dirname(capturePath));
const run = JSON.parse(await readFile(runPath, 'utf8'));
const capture = JSON.parse(await readFile(capturePath, 'utf8'));
const benchmarkNames = new Map(V2_WEBSITE_BENCHMARKS.map((benchmark) => [benchmark.id, benchmark.name]));

const successful = capture.results.filter((result) => result.status === 'captured');
const failed = capture.results.filter((result) => result.status === 'failed');
const byViewport = new Map();
for (const result of successful) {
  const items = byViewport.get(result.viewportId) ?? [];
  items.push(result);
  byViewport.set(result.viewportId, items);
}

const visualVariation = [...byViewport.entries()].map(([viewportId, results]) => ({
  viewportId,
  benchmarkCount: results.length,
  uniqueScreenshotCount: new Set(results.map((result) => result.sha256)).size,
  uniqueRatio: ratio(new Set(results.map((result) => result.sha256)).size / Math.max(1, results.length))
}));

const overflowResults = successful.filter((result) =>
  result.pageState?.document?.scrollWidth > result.width + 1);
const underusedWideResults = successful.filter((result) =>
  result.width >= 2560
  && result.pageState?.designSurface?.width
  && result.pageState.designSurface.width / result.width < 0.5);
const structureSignatures = new Map();
for (const design of run.designs) {
  const signature = JSON.stringify(design.structure ?? null);
  const ids = structureSignatures.get(signature) ?? [];
  ids.push(design.benchmarkId);
  structureSignatures.set(signature, ids);
}

const findings = [];
if (failed.length > 0) findings.push({
  severity: 'blocker',
  id: 'capture-failures',
  message: `${failed.length} screenshots failed to capture.`
});
if (overflowResults.length > 0) findings.push({
  severity: 'blocker',
  id: 'horizontal-overflow',
  message: `${overflowResults.length}/${successful.length} screenshots have document width beyond the target viewport.`
});
if (underusedWideResults.length > 0) findings.push({
  severity: 'major',
  id: 'wide-screen-underuse',
  message: `${underusedWideResults.length} QHD/4K/8K screenshots use less than half of the viewport width for the design surface.`
});
const identicalViewportCount = visualVariation.filter((entry) => entry.uniqueScreenshotCount === 1).length;
if (identicalViewportCount > 0) findings.push({
  severity: 'blocker',
  id: 'visual-homogeneity',
  message: `${identicalViewportCount}/${visualVariation.length} viewport groups are pixel-identical across all website briefs.`
});
if (structureSignatures.size < run.designs.length) findings.push({
  severity: 'blocker',
  id: 'structural-homogeneity',
  message: `${run.designs.length} website briefs produce only ${structureSignatures.size} distinct document structures.`
});

const report = {
  schemaVersion: 1,
  runId: run.runId,
  sourceVersion: run.sourceVersion,
  generatedAt: new Date().toISOString(),
  expectedCaptureCount: V2_WEBSITE_BENCHMARKS.length * 10,
  successfulCaptureCount: successful.length,
  failedCaptureCount: failed.length,
  distinctStructureCount: structureSignatures.size,
  visualVariation,
  automatedSignals: {
    horizontalOverflowCount: overflowResults.length,
    horizontalOverflowRatio: ratio(overflowResults.length / Math.max(1, successful.length)),
    underusedWideViewportCount: underusedWideResults.length,
    meanVisualUniqueRatio: ratio(visualVariation.reduce((sum, entry) => sum + entry.uniqueRatio, 0) / Math.max(1, visualVariation.length))
  },
  findings,
  overflow: overflowResults.map((result) => ({ benchmarkId: result.benchmarkId, viewportId: result.viewportId, width: result.width, scrollWidth: result.pageState.document.scrollWidth })),
  underusedWide: underusedWideResults.map((result) => ({ benchmarkId: result.benchmarkId, viewportId: result.viewportId, viewportWidth: result.width, designWidth: result.pageState.designSurface.width })),
  structures: run.designs.map((design) => ({ benchmarkId: design.benchmarkId, name: benchmarkNames.get(design.benchmarkId), ...design.structure }))
};

const markdown = `# v2 阶段 0：${run.runId} 基线报告

生成时间：${report.generatedAt}

## 结论

- 截图：${report.successfulCaptureCount}/${report.expectedCaptureCount} 成功；
- 不同文档结构：${report.distinctStructureCount}/${run.designs.length}；
- 平均视觉唯一率：${Math.round(report.automatedSignals.meanVisualUniqueRatio * 100)}%；
- 横向溢出：${report.automatedSignals.horizontalOverflowCount}/${report.successfulCaptureCount}；
- 宽屏利用不足：${report.automatedSignals.underusedWideViewportCount} 个截图。

## 自动发现

${findings.map((finding) => `- **${finding.severity} · ${finding.id}**：${finding.message}`).join('\n') || '- 未发现自动化失败项。'}

## 各视口视觉差异

| 视口 | 网站数 | 唯一截图 | 唯一率 |
| --- | ---: | ---: | ---: |
${visualVariation.map((entry) => `| ${entry.viewportId} | ${entry.benchmarkCount} | ${entry.uniqueScreenshotCount} | ${Math.round(entry.uniqueRatio * 100)}% |`).join('\n')}

## 文档结构

| 基准 | 页面 | 组件 | 有父级组件 | 组件类型 |
| --- | ---: | ---: | ---: | --- |
${report.structures.map((structure) => `| ${structure.name ?? structure.benchmarkId} | ${structure.pageCount ?? '-'} | ${structure.componentCount ?? '-'} | ${structure.parentedComponentCount ?? '-'} | ${Object.entries(structure.componentTypes ?? {}).map(([type, count]) => `${type}:${count}`).join(', ')} |`).join('\n')}

> 本报告只包含可以自动判定的结构、溢出、视口利用率和重复率。视觉层级、品牌适配、内容质量与交互质量仍需视觉评审，不能由这些代理指标冒充完整评分。
`;

await mkdir(outputDirectory, { recursive: true });
const jsonPath = path.join(outputDirectory, 'baseline-report.json');
const markdownPath = path.join(outputDirectory, 'baseline-report.md');
await writeFile(jsonPath, `${JSON.stringify(report, null, 2)}\n`);
await writeFile(markdownPath, markdown);
process.stdout.write(`${markdown}\nreport ${jsonPath}\nreport ${markdownPath}\n`);
if (failed.length > 0) process.exitCode = 1;
